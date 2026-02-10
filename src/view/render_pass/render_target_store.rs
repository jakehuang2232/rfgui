use std::collections::HashMap;

use crate::view::frame_graph::PassContext;
use crate::view::frame_graph::texture_resource::TextureHandle;

const RENDER_TARGET_STORE: u64 = 200;

struct RenderTargetEntry {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
}

struct RenderTargetStore {
    entries: HashMap<u32, RenderTargetEntry>,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
}

impl RenderTargetStore {
    fn new(format: wgpu::TextureFormat, width: u32, height: u32) -> Self {
        Self {
            entries: HashMap::new(),
            format,
            width,
            height,
        }
    }

    fn ensure(&mut self, device: &wgpu::Device, handle: TextureHandle, format: wgpu::TextureFormat, width: u32, height: u32) {
        if self.format != format || self.width != width || self.height != height {
            self.entries.clear();
            self.format = format;
            self.width = width;
            self.height = height;
        }

        self.entries.entry(handle.0).or_insert_with(|| {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Render Target Texture"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC
                    | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            RenderTargetEntry { texture, view }
        });
    }
}

pub(crate) fn ensure_render_target(ctx: &mut PassContext<'_, '_>, handle: TextureHandle) {
    let device = match ctx.viewport.device() {
        Some(device) => device,
        None => return,
    };
    let format = ctx.viewport.surface_format();
    let (width, height) = ctx.viewport.surface_size();
    let store = ctx
        .cache
        .get_or_insert_with::<RenderTargetStore, _>(RENDER_TARGET_STORE, || {
            RenderTargetStore::new(format, width, height)
        });
    store.ensure(device, handle, format, width, height);
}

pub(crate) fn render_target_view(
    ctx: &mut PassContext<'_, '_>,
    handle: TextureHandle,
) -> Option<wgpu::TextureView> {
    ensure_render_target(ctx, handle);
    let format = ctx.viewport.surface_format();
    let (width, height) = ctx.viewport.surface_size();
    let store = ctx
        .cache
        .get_or_insert_with::<RenderTargetStore, _>(RENDER_TARGET_STORE, || {
            RenderTargetStore::new(format, width, height)
        });
    let entry = store.entries.get(&handle.0)?;
    Some(entry.view.clone())
}
