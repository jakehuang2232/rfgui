use super::*;

impl Viewport {
    /// Attach a surface target to the viewport.
    ///
    /// Accepts any `SurfaceTarget` — on native this is typically
    /// `Arc<winit::window::Window>`, on wasm it is
    /// `crate::platform::web::WebCanvasSurfaceTarget`. Both paths go through
    /// the same entry point; the viewport itself has no knowledge of the
    /// concrete type.
    pub async fn attach<T>(&mut self, target: T)
    where
        T: WindowHandle + Send + Sync + 'static,
    {
        self.gpu.window = Some(Arc::new(target));
        self.create_surface().await;
    }

    pub fn set_surface_format_preference(&mut self, pref: SurfaceFormatPreference) {
        self.gpu.surface_format_preference = pref;
    }

    pub fn set_size(&mut self, mut width: u32, mut height: u32) {
        if width == 0 {
            width = 1;
        }
        if height == 0 {
            height = 1;
        }
        self.update_logical_size(width, height);
        if self.gpu.surface_config.width == width
            && self.gpu.surface_config.height == height
            && self.pending_size.is_none()
        {
            return;
        }
        self.pending_size = Some((width, height));
        self.needs_reconfigure = true;
        self.invalidate_promoted_layer_reuse();
    }

    pub fn set_style(&mut self, style: Style) {
        self.style = style;
        self.scene.last_rsx_root = None;
        self.request_redraw();
    }

    pub fn style(&self) -> &Style {
        &self.style
    }

    pub fn set_clear_color(&mut self, clear_color: Box<dyn ColorLike>) {
        self.clear_color = clear_color;
    }

    pub fn set_cursor(&mut self, cursor: Option<Cursor>) {
        self.cursor_override = cursor;
    }

    /// Push text the viewport wants written to the host clipboard into the
    /// pending platform request queue, and mirror it to the in-memory
    /// fallback so immediate reads from within this frame still see it.
    pub fn set_clipboard_text(&mut self, text: impl Into<String>) {
        let text = text.into();
        self.clipboard_fallback = Some(text.clone());
        self.pending_platform_requests.clipboard_write = Some(text);
    }

    /// Return the in-memory clipboard fallback. Actual host-clipboard reads
    /// are the backend's responsibility.
    pub fn clipboard_text(&mut self) -> Option<String> {
        self.clipboard_fallback.clone()
    }

    /// Drain the outbound platform requests accumulated since the last
    /// drain. Backends call this after each render/event batch and apply
    /// the results to the real window/clipboard.
    pub fn drain_platform_requests(&mut self) -> PlatformRequests {
        // Fold the internal `redraw_requested` flag into the drain so the
        // backend only has to look in one place.
        if self.redraw_requested {
            self.pending_platform_requests.request_redraw = true;
            self.redraw_requested = false;
        }
        std::mem::take(&mut self.pending_platform_requests)
    }

    pub fn set_scale_factor(&mut self, scale_factor: f32) {
        self.scale_factor = scale_factor.max(0.0001);
        self.update_logical_size(
            self.gpu.surface_config.width,
            self.gpu.surface_config.height,
        );
        self.invalidate_promoted_layer_reuse();
    }

    pub fn scale_factor(&self) -> f32 {
        self.scale_factor
    }

    pub fn logical_size(&self) -> (f32, f32) {
        (self.logical_width, self.logical_height)
    }

    pub fn logical_scissor_to_physical(
        &self,
        scissor_rect: [u32; 4],
        target_size: (u32, u32),
    ) -> Option<[u32; 4]> {
        let scale = self.scale_factor.max(0.0001);
        let [x, y, width, height] = scissor_rect;
        let left = (x as f32 * scale).floor().max(0.0) as i64;
        let top = (y as f32 * scale).floor().max(0.0) as i64;
        let right = ((x as f32 + width as f32) * scale).ceil().max(0.0) as i64;
        let bottom = ((y as f32 + height as f32) * scale).ceil().max(0.0) as i64;
        let max_w = target_size.0 as i64;
        let max_h = target_size.1 as i64;

        let clamped_left = left.clamp(0, max_w);
        let clamped_top = top.clamp(0, max_h);
        let clamped_right = right.clamp(0, max_w);
        let clamped_bottom = bottom.clamp(0, max_h);
        if clamped_right <= clamped_left || clamped_bottom <= clamped_top {
            return None;
        }

        Some([
            clamped_left as u32,
            clamped_top as u32,
            (clamped_right - clamped_left) as u32,
            (clamped_bottom - clamped_top) as u32,
        ])
    }

    pub fn physical_to_logical_point(&self, x: f32, y: f32) -> (f32, f32) {
        let scale = self.scale_factor.max(0.0001);
        (x / scale, y / scale)
    }

    pub fn logical_to_physical_rect(&self, x: f32, y: f32, w: f32, h: f32) -> (i32, i32, u32, u32) {
        let scale = self.scale_factor.max(0.0001);
        (
            (x.max(0.0) * scale).round() as i32,
            (y.max(0.0) * scale).round() as i32,
            (w.max(1.0) * scale).ceil() as u32,
            (h.max(1.0) * scale).ceil() as u32,
        )
    }

    pub fn request_redraw(&mut self) {
        self.redraw_requested = true;
    }

    pub fn redraw_requested(&self) -> bool {
        self.redraw_requested
    }

    /// Returns true when the most recent render reported that one or
    /// more transition / animation plugins still wanted additional
    /// frames. Hosts use this to decide whether to pump the next frame
    /// immediately or sleep until the next user event.
    pub fn is_animating(&self) -> bool {
        self.is_animating
    }

    pub fn take_redraw_request(&mut self) -> bool {
        std::mem::take(&mut self.redraw_requested)
    }

    pub async fn create_surface(&mut self) {
        let Some(surface_target) = self.gpu.window.clone() else {
            return;
        };
        {
            let backends = wgpu::Backends::all();

            let instance = Instance::new(wgpu::InstanceDescriptor {
                backends,
                flags: wgpu::InstanceFlags::empty(),
                memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
                backend_options: wgpu::BackendOptions::default(),
                display: None,
            });

            let surface = instance.create_surface(surface_target).unwrap();

            let Ok(adapter) = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::default(),
                    compatible_surface: Some(&surface),
                    force_fallback_adapter: false,
                })
                .await
            else {
                eprintln!("[warn] failed to acquire a GPU adapter");
                return;
            };

            let (device, queue) = adapter
                .request_device(&wgpu::DeviceDescriptor {
                    label: None,
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    experimental_features: wgpu::ExperimentalFeatures::default(),
                    memory_hints: wgpu::MemoryHints::default(),
                    trace: wgpu::Trace::Off,
                })
                .await
                .unwrap();

            let caps = surface.get_capabilities(&adapter);
            let format = Self::pick_surface_format(
                &caps.formats,
                self.gpu.surface_format_preference,
                self.gpu.surface_config.format,
            );
            let Some(format) = format else {
                eprintln!("[warn] surface reported no supported formats");
                return;
            };
            // On adapters that only expose a non-sRGB canvas format (notably
            // WebGPU on the browser), we keep the non-sRGB format for storage
            // but add the sRGB variant to `view_formats` so the per-frame
            // surface view can perform linear→sRGB encoding on store. Pipelines
            // that write to the surface compile against `surface_target_format`,
            // which equals the sRGB variant when we want that encoding.
            let wants_srgb = matches!(
                self.gpu.surface_format_preference,
                SurfaceFormatPreference::PreferSrgb
            );
            let (storage_format, target_format) = if wants_srgb && !format.is_srgb() {
                let srgb = format.add_srgb_suffix();
                (format, srgb)
            } else {
                (format, format)
            };
            self.gpu.surface_config.format = storage_format;
            self.gpu.surface_target_format = target_format;
            self.gpu.surface_config.alpha_mode =
                Self::alpha_mode_from_capabilities(&caps.alpha_modes);
            self.gpu.surface_config.view_formats = if storage_format == target_format {
                vec![storage_format]
            } else {
                vec![storage_format, target_format]
            };
            if let Some((width, height)) = self.pending_size.take() {
                self.gpu.surface_config.width = width;
                self.gpu.surface_config.height = height;
            }

            surface.configure(&device, &self.gpu.surface_config);

            self.gpu.instance = Some(instance);
            self.gpu.surface = Some(surface);
            self.gpu.device = Some(device);
            self.gpu.queue = Some(queue);
            self.release_render_resource_caches();
            self.invalidate_promoted_layer_reuse();
            self.create_frame_attachments();
            self.needs_reconfigure = false;
            if let Some(device) = self.gpu.device.as_ref() {
                if let Some(queue) = self.gpu.queue.as_ref() {
                    crate::view::render_pass::prewarm_text_pipeline(
                        device,
                        queue,
                        self.gpu.surface_config.format,
                        self.gpu.msaa_sample_count,
                    );
                }
            }
        }
    }

    /// Data-driven surface format selection. `preference` decides whether
    /// sRGB or non-sRGB formats win in the tiebreaker; `current` is kept if
    /// it's present in the capability list, otherwise the first matching
    /// format by preference wins. No `cfg` — the viewport has no platform
    /// knowledge; callers supply the preference.
    fn pick_surface_format(
        available: &[wgpu::TextureFormat],
        preference: SurfaceFormatPreference,
        current: wgpu::TextureFormat,
    ) -> Option<wgpu::TextureFormat> {
        match preference {
            // Native default. Match the pre-phase-2 behavior exactly: first
            // srgb format, else first available. The caller's `current`
            // default (`Bgra8Unorm`) is ignored on purpose — it's
            // non-srgb and would wash out colors if picked on a device
            // that also advertises the srgb variant.
            SurfaceFormatPreference::PreferSrgb => available
                .iter()
                .copied()
                .find(|f| f.is_srgb())
                .or_else(|| available.first().copied()),
            // Web/wasm default. Prefer a non-srgb format so the browser
            // compositor doesn't double-apply gamma. Honors `current` if it
            // is advertised by the surface (matches pre-phase-2 wasm code).
            SurfaceFormatPreference::PreferNonSrgb => available
                .iter()
                .copied()
                .find(|f| *f == current)
                .or_else(|| available.iter().copied().find(|f| !f.is_srgb()))
                .or_else(|| available.iter().copied().find(|f| f.is_srgb()))
                .or_else(|| available.first().copied()),
        }
    }

    pub(super) fn apply_pending_reconfigure(&mut self) -> bool {
        if !self.needs_reconfigure {
            return true;
        }
        if let Some((width, height)) = self.pending_size.take() {
            self.gpu.surface_config.width = width;
            self.gpu.surface_config.height = height;
        }
        let surface = match &self.gpu.surface {
            Some(surface) => surface,
            None => return false,
        };
        let device = match &self.gpu.device {
            Some(device) => device,
            None => return false,
        };
        surface.configure(device, &self.gpu.surface_config);
        let device_for_prewarm = device.clone();
        self.release_render_resource_caches();
        self.invalidate_promoted_layer_reuse();
        self.create_frame_attachments();
        if let Some(queue) = self.gpu.queue.as_ref() {
            crate::view::render_pass::prewarm_text_pipeline(
                &device_for_prewarm,
                queue,
                self.gpu.surface_config.format,
                self.gpu.msaa_sample_count,
            );
        }
        self.needs_reconfigure = false;
        true
    }

    pub(super) fn create_frame_attachments(&mut self) {
        self.create_depth_texture();
    }

    fn create_depth_texture(&mut self) {
        let device = match &self.gpu.device {
            Some(d) => d,
            None => return,
        };

        let size = wgpu::Extent3d {
            width: self.gpu.surface_config.width,
            height: self.gpu.surface_config.height,
            depth_or_array_layers: 1,
        };
        let desc = wgpu::TextureDescriptor {
            label: Some("Depth Texture"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth24PlusStencil8,
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        };
        let texture = device.create_texture(&desc);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        self.gpu.depth_texture = Some(texture);
        self.gpu.depth_view = Some(view);
    }
}
