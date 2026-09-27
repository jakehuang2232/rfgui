use super::*;

impl Viewport {
    /// Acquire only when execution reaches a pass that writes the surface.
    /// Offscreen preparation and recording use the already-created encoder.
    pub(crate) fn acquire_frame_surface(&mut self) -> bool {
        let Some(frame) = self.frame.frame_state.as_ref() else {
            return false;
        };
        if frame.view.is_some() {
            return true;
        }
        #[cfg(any(test, feature = "renderer-test-support"))]
        if self.gpu.surface.is_none() {
            return self.acquire_offscreen_surface();
        }
        let started = Instant::now();
        let texture = (|| {
            let surface = self.gpu.surface.as_ref()?;
            let device = self.gpu.device.as_ref()?;
            #[cfg(any(test, feature = "renderer-test-support"))]
            {
                self.frame.completion_counts.acquires += 1;
            }
            match surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(texture) => Some(texture),
                wgpu::CurrentSurfaceTexture::Suboptimal(texture) => {
                    // Configure after this image is presented, not while held.
                    self.needs_reconfigure = true;
                    Some(texture)
                }
                wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
                    surface.configure(device, &self.gpu.surface_config);
                    #[cfg(any(test, feature = "renderer-test-support"))]
                    {
                        self.frame.completion_counts.acquires += 1;
                    }
                    match surface.get_current_texture() {
                        wgpu::CurrentSurfaceTexture::Success(texture) => Some(texture),
                        wgpu::CurrentSurfaceTexture::Suboptimal(texture) => {
                            self.needs_reconfigure = true;
                            Some(texture)
                        }
                        _ => None,
                    }
                }
                _ => None,
            }
        })();
        let frame = self.frame.frame_state.as_mut().expect("active frame");
        frame.surface_acquire_ms = started.elapsed().as_secs_f64() * 1000.;
        let Some(texture) = texture else {
            return false;
        };
        let started = Instant::now();
        frame.view = Some(texture.texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(self.gpu.surface_target_format),
            ..Default::default()
        }));
        frame.surface_create_view_ms = started.elapsed().as_secs_f64() * 1000.;
        frame.render_texture = Some(texture);
        true
    }

    #[cfg(any(test, feature = "renderer-test-support"))]
    fn acquire_offscreen_surface(&mut self) -> bool {
        #[cfg(feature = "renderer-test-support")]
        if std::mem::take(&mut self.frame.fail_next_surface_acquisition) {
            self.frame.completion_counts.acquires += 1;
            return false;
        }
        let Some(device) = self.gpu.device.as_ref() else {
            return false;
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rfgui native pixel parity output"),
            size: wgpu::Extent3d {
                width: self.gpu.surface_config.width,
                height: self.gpu.surface_config.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.gpu.surface_config.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let frame = self.frame.frame_state.as_mut().expect("active frame");
        frame.view = Some(texture.create_view(&wgpu::TextureViewDescriptor::default()));
        frame.offscreen_texture = Some(texture.clone());
        #[cfg(feature = "renderer-test-support")]
        self.note_offscreen_acquisition(texture);
        true
    }

    /// Low-level GPU tests explicitly acquire before recording readbacks.
    #[cfg(any(test, feature = "renderer-test-support"))]
    pub(crate) fn begin_offscreen_test_frame(
        &mut self,
        device: wgpu::Device,
        queue: wgpu::Queue,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) -> Result<(), String> {
        self.prepare_offscreen_test_frame(device, queue, width, height, format)?;
        self.acquire_frame_surface()
            .then_some(())
            .ok_or_else(|| "offscreen acquisition failed".into())
    }
}
