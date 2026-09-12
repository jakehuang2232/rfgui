pub struct Gpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}
impl Gpu {
    pub fn new() -> Result<Self, String> {
        pollster::block_on(async {
            let i = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
            let a = i
                .request_adapter(&wgpu::RequestAdapterOptions::default())
                .await
                .map_err(|e| e.to_string())?;
            eprintln!("native controls: {:?}", a.get_info());
            let (device, queue) = a
                .request_device(&wgpu::DeviceDescriptor::default())
                .await
                .map_err(|e| e.to_string())?;
            Ok(Self { device, queue })
        })
    }
    pub fn read(&self, t: &wgpu::Texture, [w, h]: [u32; 2]) -> Result<Vec<u8>, String> {
        let stride = (w * 4).div_ceil(256) * 256;
        let b = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: u64::from(stride * h),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut e = self.device.create_command_encoder(&Default::default());
        e.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: t,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &b,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(stride),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([e.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        b.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| e.to_string())?;
        rx.recv()
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        let data = b.slice(..).get_mapped_range().map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for row in data.chunks_exact(stride as usize) {
            out.extend_from_slice(&row[..w as usize * 4]);
        }
        drop(data);
        b.unmap();
        Ok(out)
    }
}
impl Drop for Gpu {
    fn drop(&mut self) {
        rfgui::view::render_pass::text_pass::clear_text_resources_cache();
    }
}
