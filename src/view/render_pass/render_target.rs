use crate::view::frame_graph::texture_resource::{TextureDesc, TextureHandle};
use crate::view::frame_graph::{AllocationId, FrameResourceContext, PersistentTextureKey};
use rustc_hash::FxHashMap;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GraphicsPassContext {
    pub scissor_rect: Option<GraphicsPassScissor>,
    pub stencil_clip_id: Option<u8>,
    pub uses_depth_stencil: bool,
}

/// A graphics-pass scissor with its coordinate space carried in the value.
///
/// Logical scissors still require viewport scaling and target-origin
/// subtraction. Target-physical scissors have already been projected into the
/// destination texture and must not be transformed a second time. This is a
/// value-space distinction, not a derivation proof; the C3b raster planner owns
/// the future production construction of `TargetPhysical` values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphicsPassScissor {
    Logical([u32; 4]),
    TargetPhysical([u32; 4]),
}

impl GraphicsPassContext {
    pub fn logical_scissor_rect(self) -> Option<[u32; 4]> {
        match self.scissor_rect {
            None => None,
            Some(GraphicsPassScissor::Logical(scissor_rect)) => Some(scissor_rect),
            Some(GraphicsPassScissor::TargetPhysical(_)) => {
                panic!("target-physical scissor cannot be consumed as a logical scissor")
            }
        }
    }
}

struct RenderTargetEntry {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    msaa_texture: Option<wgpu::Texture>,
    msaa_view: Option<wgpu::TextureView>,
    label: String,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    dimension: wgpu::TextureDimension,
    msaa_sample_count: u32,
    frame_busy_epoch: u64,
    last_used_epoch: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RenderTargetCompatibility {
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    dimension: wgpu::TextureDimension,
    sample_count: u32,
    label: String,
}

impl RenderTargetCompatibility {
    fn from_desc(desc: &TextureDesc, sample_count: u32) -> Self {
        Self {
            width: desc.width().max(1),
            height: desc.height().max(1),
            format: desc.format(),
            dimension: desc.dimension(),
            sample_count,
            label: color_texture_label(desc),
        }
    }

    fn from_entry(entry: &RenderTargetEntry) -> Self {
        Self {
            width: entry.width,
            height: entry.height,
            format: entry.format,
            dimension: entry.dimension,
            sample_count: entry.msaa_sample_count,
            label: entry.label.clone(),
        }
    }
}

fn persistent_compatibility_matches(
    actual: Option<&RenderTargetCompatibility>,
    expected: &RenderTargetCompatibility,
) -> bool {
    actual.is_some_and(|actual| actual == expected)
}

fn color_texture_label(desc: &TextureDesc) -> String {
    desc.label().unwrap_or("Render Target Texture").to_string()
}

fn msaa_texture_label(desc: &TextureDesc) -> String {
    match desc.label() {
        Some(label) => format!("{label} / MSAA"),
        None => "Render Target MSAA Texture".to_string(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextureRef {
    pub texture: TextureHandle,
    pub logical_origin_x: u32,
    pub logical_origin_y: u32,
    pub logical_width: u32,
    pub logical_height: u32,
    pub physical_width: u32,
    pub physical_height: u32,
}

impl TextureRef {
    #[allow(dead_code)]
    pub fn uv_offset_x(&self) -> f32 {
        self.logical_origin_x as f32 / self.physical_width.max(1) as f32
    }

    #[allow(dead_code)]
    pub fn uv_offset_y(&self) -> f32 {
        self.logical_origin_y as f32 / self.physical_height.max(1) as f32
    }

    #[allow(dead_code)]
    pub fn uv_scale_x(&self) -> f32 {
        self.logical_width as f32 / self.physical_width.max(1) as f32
    }

    #[allow(dead_code)]
    pub fn uv_scale_y(&self) -> f32 {
        self.logical_height as f32 / self.physical_height.max(1) as f32
    }

    #[allow(dead_code)]
    pub fn logical_size(&self) -> (u32, u32) {
        (self.logical_width, self.logical_height)
    }

    #[allow(dead_code)]
    pub fn physical_size(&self) -> (u32, u32) {
        (self.physical_width, self.physical_height)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResolvedTextureRef {
    pub global_origin: (u32, u32),
    pub logical_origin: (u32, u32),
    pub logical_size: (u32, u32),
    pub physical_size: (u32, u32),
}

impl ResolvedTextureRef {
    pub fn global_origin_f32(self) -> [f32; 2] {
        [self.global_origin.0 as f32, self.global_origin.1 as f32]
    }

    pub fn logical_origin_f32(self) -> [f32; 2] {
        [self.logical_origin.0 as f32, self.logical_origin.1 as f32]
    }

    pub fn with_fallback_origin(mut self, origin: (u32, u32)) -> Self {
        if self.global_origin == (0, 0) {
            self.global_origin = origin;
        }
        self
    }

    pub fn with_fallback_logical_origin(mut self, logical_origin: (u32, u32)) -> Self {
        if self.logical_origin == (0, 0) {
            self.logical_origin = logical_origin;
        }
        self
    }
}

pub(crate) struct RenderTargetBundle {
    pub texture_ref: TextureRef,
    pub view: wgpu::TextureView,
    pub msaa_view: Option<wgpu::TextureView>,
}

pub(crate) struct OffscreenRenderTargetPool {
    entries: FxHashMap<u32, RenderTargetEntry>,
    retired_frame_entries: Vec<RenderTargetEntry>,
    frame_bindings: FxHashMap<u32, u32>,
    persistent_bindings: FxHashMap<PersistentTextureKey, PersistentRenderTargetBinding>,
    frame_epoch: u64,
    next_entry_id: u32,
}

#[derive(Clone, Copy)]
struct PersistentRenderTargetBinding {
    entry_id: u32,
    last_used_epoch: u64,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PersistentRenderTargetObservation {
    pub(crate) stable_key: PersistentTextureKey,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) format: wgpu::TextureFormat,
    pub(crate) dimension: wgpu::TextureDimension,
    pub(crate) sample_count: u32,
    pub(crate) last_used_epoch: u64,
}

impl OffscreenRenderTargetPool {
    const MAX_ENTRIES: usize = 64;
    const MAX_TOTAL_PIXELS: u64 = 16_000_000;
    /// ~1 s @60 fps (Chromium tile manager evicts after 1 s idle).
    const EVICT_UNUSED_AFTER_FRAMES: u64 = 60;

    pub fn new() -> Self {
        Self {
            entries: FxHashMap::default(),
            retired_frame_entries: Vec::new(),
            frame_bindings: FxHashMap::default(),
            persistent_bindings: FxHashMap::default(),
            frame_epoch: 0,
            next_entry_id: 0,
        }
    }

    pub fn begin_frame(&mut self) {
        // The previous encoder has been submitted or discarded. Retired
        // attachments may now be explicitly destroyed, including on WebGPU.
        for entry in self.retired_frame_entries.drain(..) {
            entry.texture.destroy();
            if let Some(msaa) = entry.msaa_texture {
                msaa.destroy();
            }
        }
        self.frame_epoch = self.frame_epoch.saturating_add(1);
        self.frame_bindings.clear();
        let frame_epoch = self.frame_epoch;
        self.persistent_bindings.retain(|_, binding| {
            frame_epoch.saturating_sub(binding.last_used_epoch) < Self::EVICT_UNUSED_AFTER_FRAMES
        });
        self.evict();
    }

    pub fn touch_persistent(&mut self, stable_key: PersistentTextureKey) {
        if let Some(binding) = self.persistent_bindings.get_mut(&stable_key) {
            binding.last_used_epoch = self.frame_epoch;
        }
    }

    #[allow(dead_code)] // C2b build-time reuse decision consumes this read-only query.
    pub fn has_compatible_persistent(
        &self,
        stable_key: PersistentTextureKey,
        desc: &TextureDesc,
        sample_count: u32,
    ) -> bool {
        let expected = RenderTargetCompatibility::from_desc(desc, sample_count);
        let actual = self
            .persistent_bindings
            .get(&stable_key)
            .and_then(|binding| self.entries.get(&binding.entry_id))
            .map(RenderTargetCompatibility::from_entry);
        persistent_compatibility_matches(actual.as_ref(), &expected)
    }

    pub fn release_persistent_pair(&mut self, color_key: PersistentTextureKey) -> bool {
        let mut released = self.release_persistent(color_key);
        if let Some(depth_key) = color_key.depth_stencil() {
            released |= self.release_persistent(depth_key);
        }
        released
    }

    #[cfg(test)]
    pub(crate) fn persistent_resident_observations(
        &self,
    ) -> Vec<PersistentRenderTargetObservation> {
        self.persistent_bindings
            .iter()
            .filter_map(|(&stable_key, binding)| {
                let entry = self.entries.get(&binding.entry_id)?;
                Some(PersistentRenderTargetObservation {
                    stable_key,
                    width: entry.width,
                    height: entry.height,
                    format: entry.format,
                    dimension: entry.dimension,
                    sample_count: entry.msaa_sample_count,
                    last_used_epoch: binding.last_used_epoch,
                })
            })
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn encode_persistent_readback(
        &self,
        stable_key: PersistentTextureKey,
        encoder: &mut wgpu::CommandEncoder,
        buffer: &wgpu::Buffer,
        padded_bytes_per_row: u32,
        width: u32,
        height: u32,
    ) -> Result<(), String> {
        let binding = self
            .persistent_bindings
            .get(&stable_key)
            .ok_or_else(|| format!("persistent target {stable_key:?} is not bound"))?;
        let entry = self
            .entries
            .get(&binding.entry_id)
            .ok_or_else(|| format!("persistent target {stable_key:?} lost its pool entry"))?;
        if width == 0 || height == 0 || width > entry.width || height > entry.height {
            return Err(format!(
                "persistent readback extent is invalid: requested={width}x{height}, resident={}x{}",
                entry.width, entry.height
            ));
        }
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &entry.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bytes_per_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        Ok(())
    }

    pub fn clear(&mut self) {
        for entry in self.entries.values().chain(self.retired_frame_entries.iter()) {
            entry.texture.destroy();
            if let Some(msaa) = entry.msaa_texture.as_ref() {
                msaa.destroy();
            }
        }
        self.entries.clear();
        self.retired_frame_entries.clear();
        self.frame_bindings.clear();
        self.persistent_bindings.clear();
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
            return self.bundle_for_entry(entry_id, &desc);
        }

        let logical_width = desc.width().max(1);
        let logical_height = desc.height().max(1);
        let physical_width = round_up_to_power_of_two(logical_width);
        let physical_height = round_up_to_power_of_two(logical_height);
        let format = desc.format();
        let dimension = desc.dimension();
        let label = color_texture_label(&desc);

        let mut best_fit: Option<u32> = None;
        for (&entry_id, entry) in &self.entries {
            if entry.frame_busy_epoch == self.frame_epoch
                || self
                    .persistent_bindings
                    .values()
                    .any(|binding| binding.entry_id == entry_id)
                || entry.format != format
                || entry.dimension != dimension
                || entry.msaa_sample_count != msaa_sample_count
                || entry.width != physical_width
                || entry.height != physical_height
                || entry.label != label
            {
                continue;
            }
            best_fit = Some(entry_id);
            break;
        }

        let entry_id = if let Some(entry_id) = best_fit {
            entry_id
        } else {
            let entry_id = self.next_entry_id;
            self.next_entry_id = self.next_entry_id.saturating_add(1);
            let physical_desc = desc.clone().with_size(physical_width, physical_height);
            self.entries.insert(
                entry_id,
                Self::create_entry(device, &physical_desc, msaa_sample_count),
            );
            entry_id
        };

        if let Some(entry) = self.entries.get_mut(&entry_id) {
            entry.frame_busy_epoch = self.frame_epoch;
            entry.last_used_epoch = self.frame_epoch;
        }
        self.frame_bindings.insert(allocation_id.0, entry_id);
        self.evict();
        self.bundle_for_entry(entry_id, &desc)
    }

    pub fn acquire_persistent(
        &mut self,
        device: &wgpu::Device,
        stable_key: PersistentTextureKey,
        desc: TextureDesc,
        msaa_sample_count: u32,
    ) -> Option<RenderTargetBundle> {
        let expected = RenderTargetCompatibility::from_desc(&desc, msaa_sample_count);

        let entry_id = match self
            .persistent_bindings
            .get(&stable_key)
            .map(|binding| binding.entry_id)
        {
            Some(entry_id) => {
                let actual = self
                    .entries
                    .get(&entry_id)
                    .map(RenderTargetCompatibility::from_entry);
                let recreate = !persistent_compatibility_matches(actual.as_ref(), &expected);
                if recreate {
                    self.remove_entry(entry_id);
                    let new_entry_id = self.next_entry_id;
                    self.next_entry_id = self.next_entry_id.saturating_add(1);
                    self.entries.insert(
                        new_entry_id,
                        Self::create_entry(device, &desc, msaa_sample_count),
                    );
                    self.persistent_bindings.insert(
                        stable_key,
                        PersistentRenderTargetBinding {
                            entry_id: new_entry_id,
                            last_used_epoch: self.frame_epoch,
                        },
                    );
                    new_entry_id
                } else {
                    entry_id
                }
            }
            None => {
                let new_entry_id = self.next_entry_id;
                self.next_entry_id = self.next_entry_id.saturating_add(1);
                self.entries.insert(
                    new_entry_id,
                    Self::create_entry(device, &desc, msaa_sample_count),
                );
                self.persistent_bindings.insert(
                    stable_key,
                    PersistentRenderTargetBinding {
                        entry_id: new_entry_id,
                        last_used_epoch: self.frame_epoch,
                    },
                );
                new_entry_id
            }
        };

        if let Some(binding) = self.persistent_bindings.get_mut(&stable_key) {
            binding.last_used_epoch = self.frame_epoch;
        }

        if let Some(entry) = self.entries.get_mut(&entry_id) {
            entry.frame_busy_epoch = self.frame_epoch;
            entry.last_used_epoch = self.frame_epoch;
        }
        self.bundle_for_entry(entry_id, &desc)
    }

    fn bundle_for_entry(
        &self,
        entry_id: u32,
        logical_desc: &TextureDesc,
    ) -> Option<RenderTargetBundle> {
        let entry = self.entries.get(&entry_id)?;
        let _ = (&entry.texture, &entry.msaa_texture);
        Some(RenderTargetBundle {
            texture_ref: TextureRef {
                texture: TextureHandle(0),
                logical_origin_x: 0,
                logical_origin_y: 0,
                logical_width: logical_desc.width().max(1),
                logical_height: logical_desc.height().max(1),
                physical_width: entry.width,
                physical_height: entry.height,
            },
            view: entry.view.clone(),
            msaa_view: entry.msaa_view.clone(),
        })
    }

    fn create_entry(
        device: &wgpu::Device,
        desc: &TextureDesc,
        msaa_sample_count: u32,
    ) -> RenderTargetEntry {
        let width = desc.width().max(1);
        let height = desc.height().max(1);
        let format = desc.format();
        let dimension = desc.dimension();
        let color_label = color_texture_label(desc);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(&color_label),
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
            let msaa_label = msaa_texture_label(desc);
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some(&msaa_label),
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
            label: color_label,
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
                if self
                    .persistent_bindings
                    .values()
                    .any(|binding| binding.entry_id == entry_id)
                {
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
            .filter(|(entry_id, entry)| {
                entry.frame_busy_epoch != self.frame_epoch
                    && !self
                        .persistent_bindings
                        .values()
                        .any(|binding| binding.entry_id == **entry_id)
            })
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
        if let Some(entry) = self.entries.remove(&entry_id) {
            // Removing residency must be immediate, but an attachment used
            // this frame may still be referenced by an unsubmitted encoder.
            // This includes selection fallback: Legacy can render into the
            // same key before the retained transaction releases old stamps.
            if entry.frame_busy_epoch == self.frame_epoch {
                self.retired_frame_entries.push(entry);
            } else {
                entry.texture.destroy();
                if let Some(msaa) = entry.msaa_texture.as_ref() {
                    msaa.destroy();
                }
            }
        }
        self.frame_bindings
            .retain(|_, bound_id| *bound_id != entry_id);
        self.persistent_bindings
            .retain(|_, binding| binding.entry_id != entry_id);
    }

    fn release_persistent(&mut self, stable_key: PersistentTextureKey) -> bool {
        let Some(entry_id) = self
            .persistent_bindings
            .get(&stable_key)
            .map(|binding| binding.entry_id)
        else {
            return false;
        };
        self.remove_entry(entry_id);
        true
    }
}

fn round_up_to_power_of_two(value: u32) -> u32 {
    value.max(1).next_power_of_two()
}

fn texture_desc_for_handle(
    ctx: &impl FrameResourceContext,
    handle: TextureHandle,
) -> Option<TextureDesc> {
    ctx.textures().get(handle.0 as usize).cloned()
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

pub(crate) fn resolve_texture_ref(
    handle: Option<TextureHandle>,
    ctx: &mut impl FrameResourceContext,
    fallback_size: (u32, u32),
    sampled_size: Option<(u32, u32)>,
) -> ResolvedTextureRef {
    let texture_ref = handle.and_then(|texture_handle| render_target_ref(ctx, texture_handle));
    let global_origin = handle
        .and_then(|texture_handle| render_target_origin(ctx, texture_handle))
        .unwrap_or((0, 0));
    let logical_size = texture_ref
        .map(|resolved| resolved.logical_size())
        .or(sampled_size)
        .unwrap_or(fallback_size);
    let physical_size = texture_ref
        .map(|resolved| resolved.physical_size())
        .or(sampled_size)
        .or_else(|| {
            handle.and_then(|texture_handle| render_target_physical_size(ctx, texture_handle))
        })
        .unwrap_or(fallback_size);
    let logical_origin = texture_ref
        .map(|resolved| (resolved.logical_origin_x, resolved.logical_origin_y))
        .unwrap_or((0, 0));
    ResolvedTextureRef {
        global_origin,
        logical_origin,
        logical_size,
        physical_size,
    }
}

pub(crate) fn render_target_bundle(
    ctx: &mut impl FrameResourceContext,
    handle: TextureHandle,
) -> Option<RenderTargetBundle> {
    let desc = texture_desc_for_handle(ctx, handle)?;
    if let Some(allocation_id) = ctx.texture_allocation_id(handle) {
        let mut bundle = ctx
            .viewport()
            .acquire_offscreen_render_target(allocation_id, desc.clone())?;
        bundle.texture_ref.texture = handle;
        bundle.texture_ref.logical_origin_x = 0;
        bundle.texture_ref.logical_origin_y = 0;
        return Some(bundle);
    }
    let stable_key = ctx.texture_stable_key(handle)?;
    let mut bundle = ctx
        .viewport()
        .acquire_persistent_render_target(stable_key, desc.clone())?;
    bundle.texture_ref.texture = handle;
    bundle.texture_ref.logical_origin_x = 0;
    bundle.texture_ref.logical_origin_y = 0;
    Some(bundle)
}

#[allow(dead_code)]
pub(crate) fn render_target_ref(
    ctx: &mut impl FrameResourceContext,
    handle: TextureHandle,
) -> Option<TextureRef> {
    Some(render_target_bundle(ctx, handle)?.texture_ref)
}

#[allow(dead_code)]
pub(crate) fn render_target_logical_size(
    ctx: &mut impl FrameResourceContext,
    handle: TextureHandle,
) -> Option<(u32, u32)> {
    Some(
        render_target_bundle(ctx, handle)?
            .texture_ref
            .logical_size(),
    )
}

#[allow(dead_code)]
pub(crate) fn render_target_physical_size(
    ctx: &mut impl FrameResourceContext,
    handle: TextureHandle,
) -> Option<(u32, u32)> {
    Some(
        render_target_bundle(ctx, handle)?
            .texture_ref
            .physical_size(),
    )
}

pub(crate) fn render_target_origin(
    ctx: &impl FrameResourceContext,
    handle: TextureHandle,
) -> Option<(u32, u32)> {
    Some(texture_desc_for_handle(ctx, handle)?.origin())
}

pub(crate) fn render_target_format(
    ctx: &impl FrameResourceContext,
    handle: TextureHandle,
) -> Option<wgpu::TextureFormat> {
    Some(texture_desc_for_handle(ctx, handle)?.format())
}

pub(crate) fn render_target_sample_count(
    ctx: &impl FrameResourceContext,
    handle: TextureHandle,
) -> Option<u32> {
    Some(texture_desc_for_handle(ctx, handle)?.sample_count().max(1))
}

pub(crate) fn logical_scissor_to_target_physical(
    viewport: &crate::view::viewport::Viewport,
    scissor_rect: [u32; 4],
    target_origin: (u32, u32),
    target_size: (u32, u32),
) -> Option<[u32; 4]> {
    let scale = viewport.scale_factor().max(0.0001);
    let [x, y, width, height] = scissor_rect;
    let left = (x as f32 * scale).floor().max(0.0) as i64 - target_origin.0 as i64;
    let top = (y as f32 * scale).floor().max(0.0) as i64 - target_origin.1 as i64;
    let right = ((x as f32 + width as f32) * scale).ceil().max(0.0) as i64 - target_origin.0 as i64;
    let bottom =
        ((y as f32 + height as f32) * scale).ceil().max(0.0) as i64 - target_origin.1 as i64;
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

pub(crate) fn resolve_graphics_pass_scissor_to_target_physical(
    viewport: &crate::view::viewport::Viewport,
    pass_scissor: Option<GraphicsPassScissor>,
    explicit_logical_scissor: Option<[u32; 4]>,
    target_origin: (u32, u32),
    target_size: (u32, u32),
) -> Option<[u32; 4]> {
    let pass_scissor = match pass_scissor {
        None => None,
        Some(GraphicsPassScissor::Logical(scissor_rect)) => {
            logical_scissor_to_target_physical(viewport, scissor_rect, target_origin, target_size)
        }
        Some(GraphicsPassScissor::TargetPhysical(scissor_rect)) => {
            clamp_target_physical_scissor(scissor_rect, target_size)
        }
    };
    let explicit_scissor = explicit_logical_scissor.and_then(|scissor_rect| {
        logical_scissor_to_target_physical(viewport, scissor_rect, target_origin, target_size)
    });
    intersect_target_physical_scissors(pass_scissor, explicit_scissor)
}

fn clamp_target_physical_scissor(
    [x, y, width, height]: [u32; 4],
    target_size: (u32, u32),
) -> Option<[u32; 4]> {
    let right = x.saturating_add(width).min(target_size.0);
    let bottom = y.saturating_add(height).min(target_size.1);
    let left = x.min(target_size.0);
    let top = y.min(target_size.1);
    (right > left && bottom > top).then_some([left, top, right - left, bottom - top])
}

fn intersect_target_physical_scissors(
    a: Option<[u32; 4]>,
    b: Option<[u32; 4]>,
) -> Option<[u32; 4]> {
    match (a, b) {
        (None, None) => None,
        (Some(rect), None) | (None, Some(rect)) => Some(rect),
        (Some([ax, ay, aw, ah]), Some([bx, by, bw, bh])) => {
            let left = ax.max(bx);
            let top = ay.max(by);
            let right = ax.saturating_add(aw).min(bx.saturating_add(bw));
            let bottom = ay.saturating_add(ah).min(by.saturating_add(bh));
            (right > left && bottom > top).then_some([left, top, right - left, bottom - top])
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod pending_submission_tests;
