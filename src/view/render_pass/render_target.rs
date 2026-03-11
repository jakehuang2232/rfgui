use crate::view::frame_graph::texture_resource::{TextureDesc, TextureHandle};
use crate::view::frame_graph::{AllocationId, AttachmentTarget, FrameResourceContext};
use std::collections::HashMap;

pub(crate) trait RenderTargetPass {
    fn apply_clip(&mut self, _scissor_rect: Option<[u32; 4]>) {}
    fn apply_stencil_clip(&mut self, _clip_id: Option<u8>) {}
    fn set_color_target(&mut self, _color_target: Option<TextureHandle>) {}
    fn set_depth_stencil_target(&mut self, _depth_stencil_target: Option<AttachmentTarget>) {}
}

struct RenderTargetEntry {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    msaa_texture: Option<wgpu::Texture>,
    msaa_view: Option<wgpu::TextureView>,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    dimension: wgpu::TextureDimension,
    msaa_sample_count: u32,
    frame_busy_epoch: u64,
    last_used_epoch: u64,
}

pub(crate) struct RenderTargetBundle {
    pub view: wgpu::TextureView,
    pub msaa_view: Option<wgpu::TextureView>,
    pub size: (u32, u32),
}

pub(crate) struct OffscreenRenderTargetPool {
    entries: HashMap<u32, RenderTargetEntry>,
    frame_bindings: HashMap<u32, u32>,
    frame_epoch: u64,
    next_entry_id: u32,
}

impl OffscreenRenderTargetPool {
    const MAX_ENTRIES: usize = 128;
    const MAX_TOTAL_PIXELS: u64 = 32_000_000;
    const EVICT_UNUSED_AFTER_FRAMES: u64 = 180;

    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            frame_bindings: HashMap::new(),
            frame_epoch: 0,
            next_entry_id: 0,
        }
    }

    pub fn begin_frame(&mut self) {
        self.frame_epoch = self.frame_epoch.saturating_add(1);
        self.frame_bindings.clear();
        self.evict();
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.frame_bindings.clear();
        self.frame_epoch = 0;
        self.next_entry_id = 0;
    }

    pub fn acquire(
        &mut self,
        device: &wgpu::Device,
        allocation_id: AllocationId,
        desc: TextureDesc,
        msaa_sample_count: u32,
    ) -> Option<RenderTargetBundle> {
        if let Some(entry_id) = self.frame_bindings.get(&allocation_id.0).copied() {
            return self.bundle_for_entry(entry_id);
        }

        let width = desc.width().max(1);
        let height = desc.height().max(1);
        let format = desc.format();
        let dimension = desc.dimension();

        let mut best_fit: Option<(u32, u64)> = None;
        for (&entry_id, entry) in &self.entries {
            if entry.frame_busy_epoch == self.frame_epoch
                || entry.format != format
                || entry.dimension != dimension
                || entry.msaa_sample_count != msaa_sample_count
                || entry.width < width
                || entry.height < height
            {
                continue;
            }
            let waste = (entry.width as u64 * entry.height as u64)
                .saturating_sub(width as u64 * height as u64);
            match best_fit {
                Some((_, best_waste)) if waste >= best_waste => {}
                _ => best_fit = Some((entry_id, waste)),
            }
        }

        let entry_id = if let Some((entry_id, _)) = best_fit {
            entry_id
        } else {
            let entry_id = self.next_entry_id;
            self.next_entry_id = self.next_entry_id.saturating_add(1);
            self.entries.insert(
                entry_id,
                Self::create_entry(device, width, height, format, dimension, msaa_sample_count),
            );
            entry_id
        };

        if let Some(entry) = self.entries.get_mut(&entry_id) {
            entry.frame_busy_epoch = self.frame_epoch;
            entry.last_used_epoch = self.frame_epoch;
        }
        self.frame_bindings.insert(allocation_id.0, entry_id);
        self.evict();
        self.bundle_for_entry(entry_id)
    }

    fn bundle_for_entry(&self, entry_id: u32) -> Option<RenderTargetBundle> {
        let entry = self.entries.get(&entry_id)?;
        let _ = (&entry.texture, &entry.msaa_texture);
        Some(RenderTargetBundle {
            view: entry.view.clone(),
            msaa_view: entry.msaa_view.clone(),
            size: (entry.width, entry.height),
        })
    }

    fn create_entry(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
        dimension: wgpu::TextureDimension,
        msaa_sample_count: u32,
    ) -> RenderTargetEntry {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Render Target Texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let (msaa_texture, msaa_view) = if msaa_sample_count > 1 {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Render Target MSAA Texture"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: msaa_sample_count,
                dimension,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            (Some(texture), Some(view))
        } else {
            (None, None)
        };
        RenderTargetEntry {
            texture,
            view,
            msaa_texture,
            msaa_view,
            width,
            height,
            format,
            dimension,
            msaa_sample_count,
            frame_busy_epoch: 0,
            last_used_epoch: 0,
        }
    }

    fn evict(&mut self) {
        if self.entries.is_empty() {
            return;
        }

        let stale_before = self
            .frame_epoch
            .saturating_sub(Self::EVICT_UNUSED_AFTER_FRAMES);
        let stale_ids: Vec<u32> = self
            .entries
            .iter()
            .filter_map(|(&entry_id, entry)| {
                if entry.frame_busy_epoch == self.frame_epoch {
                    return None;
                }
                if entry.last_used_epoch <= stale_before {
                    return Some(entry_id);
                }
                None
            })
            .collect();
        for entry_id in stale_ids {
            self.remove_entry(entry_id);
        }

        while self.entries.len() > Self::MAX_ENTRIES || self.total_pixels() > Self::MAX_TOTAL_PIXELS
        {
            let Some(entry_id) = self.pick_lru_evictable() else {
                break;
            };
            self.remove_entry(entry_id);
        }
    }

    fn total_pixels(&self) -> u64 {
        self.entries
            .values()
            .map(|entry| entry.width as u64 * entry.height as u64)
            .sum()
    }

    fn pick_lru_evictable(&self) -> Option<u32> {
        self.entries
            .iter()
            .filter(|(_, entry)| entry.frame_busy_epoch != self.frame_epoch)
            .min_by(|a, b| {
                let (_, a_entry) = a;
                let (_, b_entry) = b;
                a_entry
                    .last_used_epoch
                    .cmp(&b_entry.last_used_epoch)
                    .then_with(|| {
                        let a_pixels = a_entry.width as u64 * a_entry.height as u64;
                        let b_pixels = b_entry.width as u64 * b_entry.height as u64;
                        b_pixels.cmp(&a_pixels)
                    })
            })
            .map(|(&entry_id, _)| entry_id)
    }

    fn remove_entry(&mut self, entry_id: u32) {
        self.entries.remove(&entry_id);
        self.frame_bindings
            .retain(|_, bound_id| *bound_id != entry_id);
    }
}

fn texture_desc_for_handle(
    ctx: &impl FrameResourceContext,
    handle: TextureHandle,
) -> Option<TextureDesc> {
    ctx.textures().get(handle.0 as usize).copied()
}

pub(crate) fn render_target_view(
    ctx: &mut impl FrameResourceContext,
    handle: TextureHandle,
) -> Option<wgpu::TextureView> {
    Some(render_target_bundle(ctx, handle)?.view)
}

pub(crate) fn render_target_msaa_view(
    ctx: &mut impl FrameResourceContext,
    handle: TextureHandle,
) -> Option<wgpu::TextureView> {
    Some(render_target_bundle(ctx, handle)?.msaa_view?)
}

pub(crate) fn render_target_attachment_view(
    ctx: &mut impl FrameResourceContext,
    handle: TextureHandle,
) -> Option<wgpu::TextureView> {
    render_target_msaa_view(ctx, handle).or_else(|| render_target_view(ctx, handle))
}

pub(crate) fn render_target_bundle(
    ctx: &mut impl FrameResourceContext,
    handle: TextureHandle,
) -> Option<RenderTargetBundle> {
    let desc = texture_desc_for_handle(ctx, handle)?;
    let allocation_id = ctx.texture_allocation_id(handle)?;
    ctx.viewport()
        .acquire_offscreen_render_target(allocation_id, desc)
}

pub(crate) fn render_target_size(
    ctx: &mut impl FrameResourceContext,
    handle: TextureHandle,
) -> Option<(u32, u32)> {
    Some(render_target_bundle(ctx, handle)?.size)
}
