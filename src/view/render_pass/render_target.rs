use crate::render_pass::draw_rect_pass::{RenderTargetIn, RenderTargetOut};
use crate::view::frame_graph::PassContext;
use crate::view::frame_graph::texture_resource::{TextureDesc, TextureHandle};
use std::collections::HashMap;

const RENDER_TARGET_STORE: u64 = 200;

pub(crate) trait RenderTargetPass {
    fn set_input(&mut self, input: RenderTargetIn);
    fn set_output(&mut self, output: RenderTargetOut);
    fn apply_clip(&mut self, _scissor_rect: Option<[u32; 4]>) {}
    fn set_color_target(&mut self, _color_target: Option<TextureHandle>) {}
}

struct RenderTargetEntry {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
}

struct RenderTargetStore {
    entries: HashMap<u32, RenderTargetEntry>,
    descs: HashMap<u32, TextureDesc>,
}

impl RenderTargetStore {
    fn new() -> Self {
        Self {
            entries: HashMap::new(),
            descs: HashMap::new(),
        }
    }

    fn ensure(&mut self, device: &wgpu::Device, handle: TextureHandle, desc: TextureDesc) {
        let recreate = match self.descs.get(&handle.0) {
            Some(existing) => {
                existing.width() != desc.width()
                    || existing.height() != desc.height()
                    || existing.format() != desc.format()
                    || existing.dimension() != desc.dimension()
            }
            None => true,
        };

        if !recreate {
            return;
        }

        let width = desc.width().max(1);
        let height = desc.height().max(1);
        let format = desc.format();

        let entry = {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Render Target Texture"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: desc.dimension(),
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC
                    | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            RenderTargetEntry { texture, view }
        };

        self.entries.insert(handle.0, entry);
        self.descs.insert(handle.0, desc);
    }
}

fn texture_desc_for_handle(
    ctx: &PassContext<'_, '_>,
    handle: TextureHandle,
) -> Option<TextureDesc> {
    ctx.textures.get(handle.0 as usize).copied()
}

pub(crate) fn ensure_render_target(ctx: &mut PassContext<'_, '_>, handle: TextureHandle) {
    let device = match ctx.viewport.device() {
        Some(device) => device,
        None => return,
    };
    let Some(desc) = texture_desc_for_handle(ctx, handle) else {
        return;
    };
    let store = ctx
        .cache
        .get_or_insert_with::<RenderTargetStore, _>(RENDER_TARGET_STORE, || {
            RenderTargetStore::new()
        });
    store.ensure(device, handle, desc);
}

pub(crate) fn render_target_view(
    ctx: &mut PassContext<'_, '_>,
    handle: TextureHandle,
) -> Option<wgpu::TextureView> {
    ensure_render_target(ctx, handle);
    let store = ctx
        .cache
        .get_or_insert_with::<RenderTargetStore, _>(RENDER_TARGET_STORE, || {
            RenderTargetStore::new()
        });
    let entry = store.entries.get(&handle.0)?;
    Some(entry.view.clone())
}

pub(crate) fn render_target_size(
    ctx: &PassContext<'_, '_>,
    handle: TextureHandle,
) -> Option<(u32, u32)> {
    let desc = texture_desc_for_handle(ctx, handle)?;
    Some((desc.width().max(1), desc.height().max(1)))
}
