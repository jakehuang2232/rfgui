use crate::view::frame_graph::{
    FrameResourceContext, GraphicsColorAttachmentOps, GraphicsPassBuilder, GraphicsPassMergePolicy,
    PrepareContext,
};
use crate::view::render_pass::draw_rect_pass::RenderTargetOut;
use crate::view::render_pass::render_target::{
    GraphicsPassContext as RenderPassContext, logical_scissor_to_target_physical,
    render_target_format, render_target_origin, render_target_sample_count, resolve_texture_ref,
};
use crate::view::render_pass::{GraphicsCtx, GraphicsPass};
use parley::FontData as ParleyFontData;
use rustc_hash::FxHashMap;
use std::cell::RefCell;
use swash::FontRef as SwashFontRef;
use swash::scale::image::{Content as SwashRasterContent, Image as SwashRasterImage};
use swash::scale::{
    Render as SwashRender, ScaleContext as SwashScaleContext, Source, StrikeWith as SwashStrikeWith,
};
use swash::zeno::{Format as SwashFormat, Vector as SwashVector};
use wgpu::util::DeviceExt;

pub(crate) struct TextPreparedInputPass {
    params: TextPassPreparedParams,
    prepared: Option<TextPreparedState>,
    input: TextInput,
    output: TextOutput,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextPassPreparedParams {
    pub(crate) staging_input: TextPassPreparedStagingInput,
    pub(crate) fragments: Vec<TextPassPreparedFragment>,
    pub(crate) scissor_rect: Option<[u32; 4]>,
    pub(crate) stencil_clip_id: Option<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TextPassPreparedFragment {
    pub(crate) origin: [f32; 2],
    pub(crate) size: [f32; 2],
}

#[derive(Default)]
pub struct TextInput {
    pub pass_context: RenderPassContext,
}

#[derive(Default)]
pub struct TextOutput {
    pub render_target: RenderTargetOut,
}

impl TextPreparedInputPass {
    pub(crate) fn new(
        params: TextPassPreparedParams,
        input: TextInput,
        output: TextOutput,
    ) -> Self {
        Self {
            params,
            prepared: None,
            input,
            output,
        }
    }
}

struct TextPreparedState {
    renderer_key: TextRendererKey,
    globals_bind_group: wgpu::BindGroup,
    screen_buffer: wgpu::Buffer,
    fragment_buffer: wgpu::Buffer,
    mask_draw: Option<std::rc::Rc<PreparedTextDraw>>,
    color_draw: Option<std::rc::Rc<PreparedTextDraw>>,
    scissor_rect: Option<[u32; 4]>,
    stencil_clip_id: Option<u8>,
}

impl Drop for TextPreparedState {
    fn drop(&mut self) {
        self.screen_buffer.destroy();
        self.fragment_buffer.destroy();
    }
}

struct PreparedTextDraw {
    vertex_buffer: wgpu::Buffer,
    instance_count: u32,
    atlas: PreparedAtlasBinding,
}

/// Which atlas a prepared draw samples from: the shared persistent atlas
/// (owned by [`TextResources`], survives across frames) or a transient
/// per-pass atlas built when the persistent one overflowed this frame.
enum PreparedAtlasBinding {
    Persistent(AtlasKind),
    Transient {
        texture: wgpu::Texture,
        _view: wgpu::TextureView,
        bind_group: wgpu::BindGroup,
    },
}

impl Drop for PreparedTextDraw {
    fn drop(&mut self) {
        self.vertex_buffer.destroy();
        if let PreparedAtlasBinding::Transient { texture, .. } = &self.atlas {
            texture.destroy();
        }
    }
}

/// Shared cross-frame glyph atlas: glyphs are uploaded once (per raster
/// key) into a fixed-size texture; steady-state frames sample it with no
/// texture creation or pixel uploads at all.
struct PersistentAtlas {
    texture: wgpu::Texture,
    _view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
    width: u32,
    height: u32,
    cursor_x: u32,
    cursor_y: u32,
    row_height: u32,
    slots: FxHashMap<TextRasterKey, PersistentAtlasSlot>,
    /// Set when an insert failed; the atlas resets at the next frame
    /// boundary (mid-frame resets would invalidate slots already baked
    /// into earlier passes' instance buffers).
    overflowed: bool,
}

#[derive(Clone, Copy)]
struct PersistentAtlasSlot {
    uv_min: [f32; 2],
    uv_max: [f32; 2],
}

const PERSISTENT_ATLAS_PADDING: u32 = 1;

impl PersistentAtlas {
    fn new(
        device: &wgpu::Device,
        atlas_layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        kind: AtlasKind,
    ) -> Self {
        let (width, height) = match kind {
            AtlasKind::Mask => (2048, 2048),
            AtlasKind::Color => (1024, 1024),
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(match kind {
                AtlasKind::Mask => "Text Persistent Mask Atlas",
                AtlasKind::Color => "Text Persistent Color Atlas",
            }),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Text Persistent Atlas Bind Group"),
            layout: atlas_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        });
        Self {
            texture,
            _view: view,
            bind_group,
            width,
            height,
            cursor_x: PERSISTENT_ATLAS_PADDING,
            cursor_y: PERSISTENT_ATLAS_PADDING,
            row_height: 0,
            slots: FxHashMap::default(),
            overflowed: false,
        }
    }

    fn reset(&mut self) {
        self.cursor_x = PERSISTENT_ATLAS_PADDING;
        self.cursor_y = PERSISTENT_ATLAS_PADDING;
        self.row_height = 0;
        self.slots.clear();
        self.overflowed = false;
    }

    /// Ensure `image` is resident and return its UVs. `None` means the
    /// atlas is full; the caller falls back to a transient atlas for this
    /// pass and the atlas resets next frame.
    fn ensure_slot(
        &mut self,
        queue: &wgpu::Queue,
        kind: AtlasKind,
        key: TextRasterKey,
        image: &SwashRasterImage,
    ) -> Option<PersistentAtlasSlot> {
        if let Some(slot) = self.slots.get(&key) {
            return Some(*slot);
        }
        let w = image.placement.width.max(1);
        let h = image.placement.height.max(1);
        if self.cursor_x + w + PERSISTENT_ATLAS_PADDING > self.width {
            self.cursor_y += self.row_height + PERSISTENT_ATLAS_PADDING;
            self.cursor_x = PERSISTENT_ATLAS_PADDING;
            self.row_height = 0;
        }
        if self.cursor_y + h + PERSISTENT_ATLAS_PADDING > self.height
            || self.cursor_x + w + PERSISTENT_ATLAS_PADDING > self.width
        {
            self.overflowed = true;
            return None;
        }
        let dst_x = self.cursor_x;
        let dst_y = self.cursor_y;
        self.cursor_x += w + PERSISTENT_ATLAS_PADDING;
        self.row_height = self.row_height.max(h);

        // Convert this glyph alone through the shared copy helper and
        // upload just its region.
        let mut pixels = vec![0_u8; w as usize * h as usize * 4];
        copy_glyph_to_atlas(kind, image, &mut pixels, w, 0, 0);
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: dst_x,
                    y: dst_y,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            &pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * 4),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        if std::env::var_os("RFGUI_ATLAS_DEBUG").is_some() {
            eprintln!("[ATLASDBG] upload {}x{} at ({}, {})", w, h, dst_x, dst_y);
        }
        let slot = PersistentAtlasSlot {
            uv_min: [
                dst_x as f32 / self.width as f32,
                dst_y as f32 / self.height as f32,
            ],
            uv_max: [
                (dst_x + w) as f32 / self.width as f32,
                (dst_y + h) as f32 / self.height as f32,
            ],
        };
        self.slots.insert(key, slot);
        Some(slot)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct TextRendererKey {
    format: wgpu::TextureFormat,
    sample_count: u32,
    stencil_enabled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum TextPipelineKind {
    Mask,
    Color,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum AtlasKind {
    Mask,
    Color,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct ScreenUniform {
    screen_size: [f32; 2],
    _pad: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct FragmentUniform {
    origin: [f32; 2],
    clip_min: [f32; 2],
    clip_max: [f32; 2],
    _pad: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct TextGlyphInstance {
    local_pos: [f32; 2],
    size: [f32; 2],
    uv_min: [f32; 2],
    uv_max: [f32; 2],
    color: [f32; 4],
    opacity: f32,
    fragment_index: u32,
}

struct PendingGlyphInstance {
    kind: AtlasKind,
    raster_key: Option<TextRasterKey>,
    local_pos: [f32; 2],
    size: [f32; 2],
    image: std::sync::Arc<SwashRasterImage>,
    color: [f32; 4],
    opacity: f32,
    fragment_index: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextPassRasterGlyphInput {
    pub(crate) glyph_id: u32,
    pub(crate) font_size: f32,
    pub(crate) font_data: Option<ParleyFontData>,
    pub(crate) font_data_id: u64,
    pub(crate) font_index: u32,
    pub(crate) normalized_coords_hash: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TextPassGlyphPaintInput {
    pub(crate) local_pos: [f32; 2],
    pub(crate) color: [f32; 4],
    pub(crate) opacity: f32,
    pub(crate) fragment_index: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextPassPreparedStagingInput {
    pub(crate) scale_factor: f32,
    pub(crate) glyphs: Vec<TextPassPreparedStagingGlyphInput>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextPassPreparedStagingGlyphInput {
    pub(crate) raster: TextPassRasterGlyphInput,
    pub(crate) paint: TextPassGlyphPaintInput,
    pub(crate) final_paint_pos: [f32; 2],
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextPassPreparedStagingProbe {
    pub(crate) scale_factor: f32,
    pub(crate) glyphs: Vec<TextPassPreparedStagingGlyph>,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextPassPreparedStagingGlyph {
    pub(crate) glyph_index: usize,
    pub(crate) raster_key: Option<TextRasterKey>,
    pub(crate) paint: TextPassGlyphPaintInput,
    pub(crate) final_paint_pos: [f32; 2],
    pub(crate) instance_local_pos: [f32; 2],
    pub(crate) instance_size: [f32; 2],
    pub(crate) atlas_kind: TextPassPreparedStagingAtlasKind,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextPassPreparedStagingAtlasKind {
    Mask,
    Color,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TextRasterKey {
    font_blob_id: u64,
    font_index: u32,
    glyph_id: u32,
    font_size_bits: u32,
    scale_factor_bits: u32,
    normalized_coords_hash: u64,
}

pub(crate) struct CachedRasterImage {
    image: std::sync::Arc<SwashRasterImage>,
    last_used_frame: u64,
}

#[derive(Default)]
struct TextResources {
    screen_layout: Option<wgpu::BindGroupLayout>,
    atlas_layout: Option<wgpu::BindGroupLayout>,
    sampler: Option<wgpu::Sampler>,
    pipelines: FxHashMap<(TextRendererKey, TextPipelineKind), wgpu::RenderPipeline>,
    raster_cache: FxHashMap<TextRasterKey, CachedRasterImage>,
    persistent_atlases: FxHashMap<AtlasKind, PersistentAtlas>,
    /// Cross-frame instance-buffer cache keyed by glyph content (raster
    /// keys, colors, snapped positions) — pure moves and scrolls keep
    /// vertex data identical (the shader adds the per-fragment origin),
    /// so prepare reuses the buffer and only rebuilds fragment uniforms.
    draw_cache: FxHashMap<u64, CachedTextDrawEntry>,
    frame_epoch: u64,
    scale_context: SwashScaleContext,
}

struct CachedTextDrawEntry {
    mask_draw: Option<std::rc::Rc<PreparedTextDraw>>,
    color_draw: Option<std::rc::Rc<PreparedTextDraw>>,
    last_used_frame: u64,
}

thread_local! {
    static TEXT_RESOURCES: RefCell<TextResources> = RefCell::new(TextResources::default());
}

impl GraphicsPass for TextPreparedInputPass {
    fn setup(&mut self, builder: &mut GraphicsPassBuilder<'_, '_>) {
        builder.set_graphics_merge_policy(GraphicsPassMergePolicy::Mergeable);
        if builder.texture_target(&self.output.render_target).is_some() {
            builder.write_color(
                &self.output.render_target,
                GraphicsColorAttachmentOps::load(),
            );
        } else {
            builder.write_surface_color(GraphicsColorAttachmentOps::load());
        }

        if self.input.pass_context.uses_depth_stencil {
            builder.read_output_depth();
            builder.read_output_stencil();
        }
    }

    fn prepare(&mut self, ctx: &mut PrepareContext<'_, '_>) {
        self.prepared =
            prepare_text_prepared_input_pass(&self.params, &self.input, &self.output, ctx);
    }

    fn execute(&mut self, ctx: &mut GraphicsCtx<'_, '_, '_, '_>) {
        let Some(prepared) = self.prepared.as_ref() else {
            return;
        };
        if let Some(scissor) = prepared.scissor_rect {
            ctx.set_scissor_rect(scissor[0], scissor[1], scissor[2], scissor[3]);
        }
        if let Some(reference) = prepared.stencil_clip_id {
            ctx.set_stencil_reference(reference as u32);
        }

        draw_prepared_text(
            ctx,
            prepared,
            TextPipelineKind::Mask,
            prepared.mask_draw.as_deref(),
        );
        draw_prepared_text(
            ctx,
            prepared,
            TextPipelineKind::Color,
            prepared.color_draw.as_deref(),
        );
    }

    fn name(&self) -> &'static str {
        "TextPreparedInputPass"
    }
}

fn prepare_text_prepared_input_pass(
    params: &TextPassPreparedParams,
    input: &TextInput,
    output: &TextOutput,
    ctx: &mut PrepareContext<'_, '_>,
) -> Option<TextPreparedState> {
    if params.fragments.is_empty() || params.staging_input.glyphs.is_empty() {
        return None;
    }

    let target_handle = output.render_target.handle();
    let (device, queue, surface_format, surface_size, scale_factor) = {
        let viewport = ctx.viewport();
        (
            viewport.device()?.clone(),
            viewport.queue()?.clone(),
            viewport.surface_format(),
            viewport.surface_size(),
            viewport.scale_factor().max(0.0001),
        )
    };

    let target_format = target_handle
        .and_then(|handle| render_target_format(ctx, handle))
        .unwrap_or(surface_format);
    let sample_count = target_handle
        .and_then(|handle| render_target_sample_count(ctx, handle))
        .unwrap_or(1)
        .max(1);
    let target = resolve_texture_ref(target_handle, ctx, surface_size, None);
    let target_origin = target_handle
        .and_then(|handle| render_target_origin(ctx, handle))
        .unwrap_or((0, 0));
    let scissor_rect = params
        .scissor_rect
        .or(input.pass_context.scissor_rect)
        .and_then(|rect| {
            logical_scissor_to_target_physical(
                ctx.viewport(),
                rect,
                target_origin,
                target.physical_size,
            )
        });
    let stencil_clip_id = params
        .stencil_clip_id
        .or(input.pass_context.stencil_clip_id);

    let renderer_key = TextRendererKey {
        format: target_format,
        sample_count,
        stencil_enabled: input.pass_context.uses_depth_stencil,
    };

    let fragments = params
        .fragments
        .iter()
        .map(|fragment| {
            let width = fragment.size[0].max(1.0);
            let height = fragment.size[1].max(1.0);
            let origin = [
                fragment.origin[0] * scale_factor - target_origin.0 as f32
                    + target.logical_origin.0 as f32,
                fragment.origin[1] * scale_factor - target_origin.1 as f32
                    + target.logical_origin.1 as f32,
            ];
            FragmentUniform {
                origin,
                clip_min: origin,
                clip_max: [
                    origin[0] + width * scale_factor,
                    origin[1] + height * scale_factor,
                ],
                _pad: [0.0, 0.0],
            }
        })
        .collect::<Vec<_>>();

    // Instance data is origin-independent apart from sub-pixel snapping,
    // so a content hash keyed on glyphs + fragment origin fractions lets
    // scroll/move frames reuse the previous vertex buffers outright.
    let draw_cache_key =
        text_draw_cache_key(&params.staging_input, fragments.as_slice(), scale_factor);
    let cached_draws = TEXT_RESOURCES.with(|slot| {
        let mut resources = slot.borrow_mut();
        let frame_epoch = resources.frame_epoch;
        resources.draw_cache.get_mut(&draw_cache_key).map(|entry| {
            entry.last_used_frame = frame_epoch;
            (entry.mask_draw.clone(), entry.color_draw.clone())
        })
    });

    let mut pending = Vec::new();
    if cached_draws.is_none() {
        TEXT_RESOURCES.with(|slot| {
            let mut resources = slot.borrow_mut();
            let frame_epoch = resources.frame_epoch;
            let TextResources {
                scale_context,
                raster_cache,
                ..
            } = &mut *resources;
            collect_prepared_staging_glyphs(
                scale_context,
                raster_cache,
                frame_epoch,
                &params.staging_input,
                fragments.as_slice(),
                scale_factor,
                &mut pending,
            );
        });

        if pending.is_empty() {
            return None;
        }
    }

    let screen_uniform = ScreenUniform {
        screen_size: [
            target.physical_size.0.max(1) as f32,
            target.physical_size.1.max(1) as f32,
        ],
        _pad: [0.0, 0.0],
    };
    let screen_buffer = super::create_transient_buffer(
        &device,
        &wgpu::util::BufferInitDescriptor {
            label: Some("Text Screen Uniform Buffer"),
            contents: bytemuck::bytes_of(&screen_uniform),
            usage: wgpu::BufferUsages::UNIFORM,
        },
    );
    let fragment_buffer = super::create_transient_buffer(
        &device,
        &wgpu::util::BufferInitDescriptor {
            label: Some("Text Fragment Storage Buffer"),
            contents: bytemuck::cast_slice(&fragments),
            usage: wgpu::BufferUsages::STORAGE,
        },
    );

    let (globals_bind_group, mask_draw, color_draw) = TEXT_RESOURCES.with(|slot| {
        let mut resources = slot.borrow_mut();
        resources.ensure_common(&device);
        let screen_layout = resources
            .screen_layout
            .as_ref()
            .expect("screen bind group layout initialized");
        let globals_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Text Globals Bind Group"),
            layout: screen_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: screen_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: fragment_buffer.as_entire_binding(),
                },
            ],
        });
        let (mask_draw, color_draw) = match cached_draws {
            Some(draws) => draws,
            None => {
                let mask_draw = build_prepared_draw(
                    &device,
                    &queue,
                    &mut resources,
                    AtlasKind::Mask,
                    pending.iter().filter(|glyph| glyph.kind == AtlasKind::Mask),
                )
                .map(std::rc::Rc::new);
                let color_draw = build_prepared_draw(
                    &device,
                    &queue,
                    &mut resources,
                    AtlasKind::Color,
                    pending
                        .iter()
                        .filter(|glyph| glyph.kind == AtlasKind::Color),
                )
                .map(std::rc::Rc::new);
                // Only persistent-atlas draws survive across frames: a
                // transient atlas is destroyed with the draw and the
                // persistent one resets at the next frame boundary.
                let cacheable = |draw: &Option<std::rc::Rc<PreparedTextDraw>>| {
                    draw.as_ref().is_none_or(|draw| {
                        matches!(draw.atlas, PreparedAtlasBinding::Persistent(_))
                    })
                };
                if (mask_draw.is_some() || color_draw.is_some())
                    && cacheable(&mask_draw)
                    && cacheable(&color_draw)
                {
                    let frame_epoch = resources.frame_epoch;
                    resources.draw_cache.insert(
                        draw_cache_key,
                        CachedTextDrawEntry {
                            mask_draw: mask_draw.clone(),
                            color_draw: color_draw.clone(),
                            last_used_frame: frame_epoch,
                        },
                    );
                }
                (mask_draw, color_draw)
            }
        };
        resources.ensure_pipeline(&device, renderer_key, TextPipelineKind::Mask);
        resources.ensure_pipeline(&device, renderer_key, TextPipelineKind::Color);
        (globals_bind_group, mask_draw, color_draw)
    });

    if mask_draw.is_none() && color_draw.is_none() {
        screen_buffer.destroy();
        fragment_buffer.destroy();
        return None;
    }

    Some(TextPreparedState {
        renderer_key,
        globals_bind_group,
        screen_buffer,
        fragment_buffer,
        mask_draw,
        color_draw,
        scissor_rect,
        stencil_clip_id,
    })
}

/// Content hash of everything that shapes vertex-buffer bytes: glyph
/// raster identity, paint colors/opacity, fragment indices, the scale
/// factor, and each fragment origin's sub-pixel fraction and sign (the
/// snap in `collect_prepared_staging_glyphs` depends only on those, so
/// integer-pixel moves and scrolls hash identically).
fn text_draw_cache_key(
    input: &TextPassPreparedStagingInput,
    fragments: &[FragmentUniform],
    scale_factor: f32,
) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = rustc_hash::FxHasher::default();
    scale_factor.to_bits().hash(&mut hasher);
    fragments.len().hash(&mut hasher);
    input.glyphs.len().hash(&mut hasher);
    for glyph in input.glyphs.iter() {
        glyph.raster.font_data_id.hash(&mut hasher);
        glyph.raster.font_index.hash(&mut hasher);
        glyph.raster.glyph_id.hash(&mut hasher);
        glyph.raster.font_size.to_bits().hash(&mut hasher);
        glyph.raster.normalized_coords_hash.hash(&mut hasher);
        glyph.paint.local_pos[0].to_bits().hash(&mut hasher);
        glyph.paint.local_pos[1].to_bits().hash(&mut hasher);
        for channel in glyph.paint.color {
            channel.to_bits().hash(&mut hasher);
        }
        glyph.paint.opacity.to_bits().hash(&mut hasher);
        glyph.paint.fragment_index.hash(&mut hasher);
    }
    hasher.finish()
}

fn collect_prepared_staging_glyphs(
    scale_context: &mut SwashScaleContext,
    raster_cache: &mut FxHashMap<TextRasterKey, CachedRasterImage>,
    frame_epoch: u64,
    input: &TextPassPreparedStagingInput,
    fragments: &[FragmentUniform],
    scale_factor: f32,
    out: &mut Vec<PendingGlyphInstance>,
) {
    for glyph in input.glyphs.iter() {
        let Some(fragment) = fragments.get(glyph.paint.fragment_index as usize) else {
            continue;
        };
        let Some(image) = rasterize_text_pass_glyph_input(
            scale_context,
            raster_cache,
            frame_epoch,
            &glyph.raster,
            scale_factor,
        ) else {
            continue;
        };
        let width = image.placement.width.max(1) as f32;
        let height = image.placement.height.max(1) as f32;
        if width <= 0.0 || height <= 0.0 || image.data.is_empty() {
            continue;
        }
        let kind = match image.content {
            SwashRasterContent::Color => AtlasKind::Color,
            SwashRasterContent::Mask | SwashRasterContent::SubpixelMask => AtlasKind::Mask,
        };
        // Pixel snapping happens in the vertex shader (trunc of origin +
        // local_pos), keeping instance data origin-independent so the
        // cross-frame draw cache hits on scrolls and moves.
        let local_pos = [
            glyph.paint.local_pos[0] * scale_factor + image.placement.left as f32,
            glyph.paint.local_pos[1] * scale_factor - image.placement.top as f32,
        ];
        let _ = fragment;
        out.push(PendingGlyphInstance {
            kind,
            raster_key: text_raster_key_for_raster_input(&glyph.raster, scale_factor),
            local_pos,
            size: [width, height],
            image,
            color: glyph.paint.color,
            opacity: glyph.paint.opacity,
            fragment_index: glyph.paint.fragment_index,
        });
    }
}

#[cfg(test)]
pub(crate) fn build_text_pass_prepared_staging_probe(
    input: &TextPassPreparedStagingInput,
) -> TextPassPreparedStagingProbe {
    let scale_factor = input.scale_factor.max(0.0001);
    let glyphs = TEXT_RESOURCES.with(|slot| {
        let mut resources = slot.borrow_mut();
        let frame_epoch = resources.frame_epoch;
        let TextResources {
            scale_context,
            raster_cache,
            ..
        } = &mut *resources;
        input
            .glyphs
            .iter()
            .enumerate()
            .filter_map(|(glyph_index, glyph)| {
                let image = rasterize_text_pass_glyph_input(
                    scale_context,
                    raster_cache,
                    frame_epoch,
                    &glyph.raster,
                    scale_factor,
                )?;
                let width = image.placement.width.max(1) as f32;
                let height = image.placement.height.max(1) as f32;
                if width <= 0.0 || height <= 0.0 || image.data.is_empty() {
                    return None;
                }
                let atlas_kind = match image.content {
                    SwashRasterContent::Color => TextPassPreparedStagingAtlasKind::Color,
                    SwashRasterContent::Mask | SwashRasterContent::SubpixelMask => {
                        TextPassPreparedStagingAtlasKind::Mask
                    }
                };
                let fragment_origin = [
                    (glyph.final_paint_pos[0] - glyph.paint.local_pos[0]) * scale_factor,
                    (glyph.final_paint_pos[1] - glyph.paint.local_pos[1]) * scale_factor,
                ];
                let local_pos = snap_text_local_pos(
                    fragment_origin,
                    [
                        glyph.paint.local_pos[0] * scale_factor + image.placement.left as f32,
                        glyph.paint.local_pos[1] * scale_factor - image.placement.top as f32,
                    ],
                );
                Some(TextPassPreparedStagingGlyph {
                    glyph_index,
                    raster_key: text_raster_key_for_raster_input(&glyph.raster, scale_factor),
                    paint: glyph.paint,
                    final_paint_pos: glyph.final_paint_pos,
                    instance_local_pos: local_pos,
                    instance_size: [width, height],
                    atlas_kind,
                })
            })
            .collect()
    });

    TextPassPreparedStagingProbe {
        scale_factor,
        glyphs,
    }
}

fn snap_text_local_pos(fragment_origin: [f32; 2], local_pos: [f32; 2]) -> [f32; 2] {
    [
        text_render_trunc(fragment_origin[0] + local_pos[0]) - fragment_origin[0],
        text_render_trunc(fragment_origin[1] + local_pos[1]) - fragment_origin[1],
    ]
}

fn text_render_trunc(value: f32) -> f32 {
    if value.is_sign_negative() {
        value.ceil()
    } else {
        value.floor()
    }
}

pub(crate) fn rasterize_text_pass_glyph_input(
    scale_context: &mut SwashScaleContext,
    raster_cache: &mut FxHashMap<TextRasterKey, CachedRasterImage>,
    frame_epoch: u64,
    glyph: &TextPassRasterGlyphInput,
    scale_factor: f32,
) -> Option<std::sync::Arc<SwashRasterImage>> {
    let font_data = glyph.font_data.as_ref()?;
    let key = text_raster_key_for_raster_input(glyph, scale_factor)?;
    if let Some(entry) = raster_cache.get_mut(&key) {
        entry.last_used_frame = frame_epoch;
        return Some(entry.image.clone());
    }

    let font_ref = swash_font_ref(font_data)?;
    let mut scaler = scale_context
        .builder(font_ref)
        .size((glyph.font_size * scale_factor).max(1.0))
        .hint(false)
        .build();
    let sources = [
        Source::ColorBitmap(SwashStrikeWith::BestFit),
        Source::ColorOutline(0),
        Source::Bitmap(SwashStrikeWith::BestFit),
        Source::Outline,
    ];
    let mut image = SwashRasterImage::new();
    let rendered = SwashRender::new(&sources)
        .format(SwashFormat::Alpha)
        .offset(SwashVector::new(0.0, 0.0))
        .render_into(&mut scaler, glyph.glyph_id as u16, &mut image);
    if !rendered {
        return None;
    }
    let image = std::sync::Arc::new(image);
    raster_cache.insert(
        key,
        CachedRasterImage {
            image: image.clone(),
            last_used_frame: frame_epoch,
        },
    );
    Some(image)
}

pub(crate) fn text_raster_key_for_raster_input(
    glyph: &TextPassRasterGlyphInput,
    scale_factor: f32,
) -> Option<TextRasterKey> {
    let font_data = glyph.font_data.as_ref()?;
    if font_data.data.id() != glyph.font_data_id || font_data.index != glyph.font_index {
        return None;
    }
    Some(text_raster_key_from_parts(
        glyph.font_data_id,
        glyph.font_index,
        glyph.glyph_id,
        glyph.font_size,
        scale_factor,
        glyph.normalized_coords_hash,
    ))
}

fn text_raster_key_from_parts(
    font_blob_id: u64,
    font_index: u32,
    glyph_id: u32,
    font_size: f32,
    scale_factor: f32,
    normalized_coords_hash: u64,
) -> TextRasterKey {
    TextRasterKey {
        font_blob_id,
        font_index,
        glyph_id,
        font_size_bits: font_size.to_bits(),
        scale_factor_bits: scale_factor.to_bits(),
        normalized_coords_hash,
    }
}

fn swash_font_ref(font_data: &ParleyFontData) -> Option<SwashFontRef<'_>> {
    SwashFontRef::from_index(font_data.data.data(), font_data.index as usize)
}

fn build_prepared_draw<'a>(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    resources: &mut TextResources,
    atlas_kind: AtlasKind,
    glyphs: impl Iterator<Item = &'a PendingGlyphInstance>,
) -> Option<PreparedTextDraw> {
    let glyphs = glyphs.collect::<Vec<_>>();
    if glyphs.is_empty() {
        return None;
    }
    if let Some(instances) =
        build_persistent_atlas_instances(device, queue, resources, atlas_kind, glyphs.as_slice())
    {
        let vertex_buffer =
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Text Glyph Instance Buffer"),
                contents: bytemuck::cast_slice(instances.as_slice()),
                usage: wgpu::BufferUsages::VERTEX,
            });
        return Some(PreparedTextDraw {
            vertex_buffer,
            instance_count: instances.len() as u32,
            atlas: PreparedAtlasBinding::Persistent(atlas_kind),
        });
    }
    build_transient_prepared_draw(device, queue, resources, atlas_kind, glyphs.as_slice())
}

/// Try to serve every glyph from the persistent atlas. Returns `None` when
/// the atlas overflowed (or a glyph has no stable raster key); the caller
/// then falls back to a transient per-pass atlas and the persistent atlas
/// resets at the next frame boundary.
fn build_persistent_atlas_instances(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    resources: &mut TextResources,
    atlas_kind: AtlasKind,
    glyphs: &[&PendingGlyphInstance],
) -> Option<Vec<TextGlyphInstance>> {
    resources.ensure_common(device);
    if !resources.persistent_atlases.contains_key(&atlas_kind) {
        let atlas_layout = resources
            .atlas_layout
            .as_ref()
            .expect("atlas bind group layout initialized");
        let sampler = resources.sampler.as_ref().expect("sampler initialized");
        let atlas = PersistentAtlas::new(device, atlas_layout, sampler, atlas_kind);
        resources.persistent_atlases.insert(atlas_kind, atlas);
    }
    let atlas = resources
        .persistent_atlases
        .get_mut(&atlas_kind)
        .expect("persistent atlas initialized");
    if atlas.overflowed {
        return None;
    }
    let mut instances = Vec::with_capacity(glyphs.len());
    for glyph in glyphs {
        let slot = glyph
            .raster_key
            .and_then(|key| atlas.ensure_slot(queue, atlas_kind, key, &glyph.image))?;
        instances.push(TextGlyphInstance {
            local_pos: glyph.local_pos,
            size: glyph.size,
            uv_min: slot.uv_min,
            uv_max: slot.uv_max,
            color: glyph.color,
            opacity: glyph.opacity,
            fragment_index: glyph.fragment_index,
        });
    }
    Some(instances)
}

fn build_transient_prepared_draw(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    resources: &mut TextResources,
    atlas_kind: AtlasKind,
    glyphs: &[&PendingGlyphInstance],
) -> Option<PreparedTextDraw> {
    let atlas = pack_atlas(atlas_kind, glyphs)?;
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(match atlas_kind {
            AtlasKind::Mask => "Text Mask Atlas",
            AtlasKind::Color => "Text Color Atlas",
        }),
        size: wgpu::Extent3d {
            width: atlas.width,
            height: atlas.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        atlas.pixels.as_slice(),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(atlas.width.saturating_mul(4)),
            rows_per_image: Some(atlas.height),
        },
        wgpu::Extent3d {
            width: atlas.width,
            height: atlas.height,
            depth_or_array_layers: 1,
        },
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let atlas_layout = resources
        .atlas_layout
        .as_ref()
        .expect("atlas bind group layout initialized");
    let sampler = resources.sampler.as_ref().expect("sampler initialized");
    let atlas_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Text Atlas Bind Group"),
        layout: atlas_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    });
    let vertex_buffer = super::create_transient_buffer(
        device,
        &wgpu::util::BufferInitDescriptor {
            label: Some("Text Glyph Instance Buffer"),
            contents: bytemuck::cast_slice(atlas.instances.as_slice()),
            usage: wgpu::BufferUsages::VERTEX,
        },
    );
    Some(PreparedTextDraw {
        vertex_buffer,
        instance_count: atlas.instances.len() as u32,
        atlas: PreparedAtlasBinding::Transient {
            texture,
            _view: view,
            bind_group: atlas_bind_group,
        },
    })
}

struct PackedAtlas {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    instances: Vec<TextGlyphInstance>,
}

fn pack_atlas(atlas_kind: AtlasKind, glyphs: &[&PendingGlyphInstance]) -> Option<PackedAtlas> {
    const PADDING: u32 = 1;
    const MAX_WIDTH: u32 = 2048;

    // Repeated glyphs share one raster image (Arc from the raster cache);
    // pack each unique image once and point every instance's UVs at the
    // shared slot instead of re-copying the bitmap per instance.
    let mut slot_for_image: FxHashMap<*const SwashRasterImage, usize> = FxHashMap::default();
    let mut unique_images: Vec<&std::sync::Arc<SwashRasterImage>> = Vec::new();
    let mut slot_index_per_glyph = Vec::with_capacity(glyphs.len());
    for glyph in glyphs {
        let key = std::sync::Arc::as_ptr(&glyph.image);
        let slot = *slot_for_image.entry(key).or_insert_with(|| {
            unique_images.push(&glyph.image);
            unique_images.len() - 1
        });
        slot_index_per_glyph.push(slot);
    }

    let mut x = PADDING;
    let mut y = PADDING;
    let mut row_height = 0;
    let mut placements = Vec::with_capacity(unique_images.len());
    let mut atlas_width = PADDING;
    for image in &unique_images {
        let w = image.placement.width.max(1);
        let h = image.placement.height.max(1);
        if x + w + PADDING > MAX_WIDTH && x > PADDING {
            y += row_height + PADDING;
            x = PADDING;
            row_height = 0;
        }
        placements.push((x, y, w, h));
        atlas_width = atlas_width.max(x + w + PADDING);
        x += w + PADDING;
        row_height = row_height.max(h);
    }
    let atlas_height = (y + row_height + PADDING).max(1);
    let atlas_width = atlas_width.max(1);
    let mut pixels = vec![0_u8; atlas_width as usize * atlas_height as usize * 4];

    for (image, &(dst_x, dst_y, _, _)) in unique_images.iter().zip(placements.iter()) {
        copy_glyph_to_atlas(atlas_kind, image, &mut pixels, atlas_width, dst_x, dst_y);
    }

    let mut instances = Vec::with_capacity(glyphs.len());
    for (glyph, &slot) in glyphs.iter().zip(slot_index_per_glyph.iter()) {
        let (dst_x, dst_y, width, height) = placements[slot];
        let uv_min = [
            dst_x as f32 / atlas_width as f32,
            dst_y as f32 / atlas_height as f32,
        ];
        let uv_max = [
            (dst_x + width) as f32 / atlas_width as f32,
            (dst_y + height) as f32 / atlas_height as f32,
        ];
        instances.push(TextGlyphInstance {
            local_pos: glyph.local_pos,
            size: glyph.size,
            uv_min,
            uv_max,
            color: glyph.color,
            opacity: glyph.opacity,
            fragment_index: glyph.fragment_index,
        });
    }

    Some(PackedAtlas {
        width: atlas_width,
        height: atlas_height,
        pixels,
        instances,
    })
}

fn copy_glyph_to_atlas(
    atlas_kind: AtlasKind,
    image: &SwashRasterImage,
    pixels: &mut [u8],
    atlas_width: u32,
    dst_x: u32,
    dst_y: u32,
) {
    let width = image.placement.width;
    let height = image.placement.height;
    for row in 0..height {
        for col in 0..width {
            let dst = (((dst_y + row) * atlas_width + dst_x + col) * 4) as usize;
            match (atlas_kind, image.content) {
                (AtlasKind::Color, SwashRasterContent::Color) => {
                    let src = ((row * width + col) * 4) as usize;
                    if src + 3 < image.data.len() && dst + 3 < pixels.len() {
                        pixels[dst..dst + 4].copy_from_slice(&image.data[src..src + 4]);
                    }
                }
                (_, SwashRasterContent::Mask) => {
                    let src = (row * width + col) as usize;
                    if src < image.data.len() && dst + 3 < pixels.len() {
                        let coverage = image.data[src];
                        pixels[dst] = coverage;
                        pixels[dst + 1] = coverage;
                        pixels[dst + 2] = coverage;
                        pixels[dst + 3] = coverage;
                    }
                }
                (_, SwashRasterContent::SubpixelMask) => {
                    let src = ((row * width + col) * 4) as usize;
                    if src + 3 < image.data.len() && dst + 3 < pixels.len() {
                        let coverage = image.data[src]
                            .max(image.data[src + 1])
                            .max(image.data[src + 2]);
                        pixels[dst] = coverage;
                        pixels[dst + 1] = coverage;
                        pixels[dst + 2] = coverage;
                        pixels[dst + 3] = image.data[src + 3].max(coverage);
                    }
                }
                (AtlasKind::Mask, SwashRasterContent::Color) => {}
            }
        }
    }
}

fn draw_prepared_text(
    ctx: &mut GraphicsCtx<'_, '_, '_, '_>,
    prepared: &TextPreparedState,
    kind: TextPipelineKind,
    draw: Option<&PreparedTextDraw>,
) {
    let Some(draw) = draw else {
        return;
    };
    if draw.instance_count == 0 {
        return;
    }
    TEXT_RESOURCES.with(|slot| {
        let resources = slot.borrow();
        let Some(pipeline) = resources.pipelines.get(&(prepared.renderer_key, kind)) else {
            return;
        };
        let atlas_bind_group = match &draw.atlas {
            PreparedAtlasBinding::Persistent(kind) => {
                let Some(atlas) = resources.persistent_atlases.get(kind) else {
                    return;
                };
                &atlas.bind_group
            }
            PreparedAtlasBinding::Transient { bind_group, .. } => bind_group,
        };
        ctx.set_pipeline(pipeline);
        ctx.set_bind_group(0, &prepared.globals_bind_group, &[]);
        ctx.set_bind_group(1, atlas_bind_group, &[]);
        ctx.set_vertex_buffer(0, draw.vertex_buffer.slice(..));
        ctx.draw(0..6, 0..draw.instance_count);
    });
}

impl TextResources {
    fn begin_frame(&mut self) {
        self.frame_epoch = self.frame_epoch.wrapping_add(1);
        self.evict_raster_cache();
        let mut atlas_reset = false;
        for atlas in self.persistent_atlases.values_mut() {
            if atlas.overflowed {
                atlas.reset();
                atlas_reset = true;
            }
        }
        if atlas_reset {
            // Cached instance buffers bake atlas UVs; a reset invalidates
            // every slot.
            self.draw_cache.clear();
        }
        self.evict_draw_cache();
    }

    fn evict_draw_cache(&mut self) {
        const MAX_DRAW_CACHE_ENTRIES: usize = 4096;
        const MAX_UNUSED_FRAMES: u64 = 120;
        let frame_epoch = self.frame_epoch;
        self.draw_cache.retain(|_, entry| {
            frame_epoch.saturating_sub(entry.last_used_frame) <= MAX_UNUSED_FRAMES
        });
        if self.draw_cache.len() <= MAX_DRAW_CACHE_ENTRIES {
            return;
        }
        let mut entries = self
            .draw_cache
            .iter()
            .map(|(key, entry)| (*key, entry.last_used_frame))
            .collect::<Vec<_>>();
        entries.sort_by_key(|(_, last_used)| *last_used);
        let remove_count = self.draw_cache.len() - MAX_DRAW_CACHE_ENTRIES;
        for (key, _) in entries.into_iter().take(remove_count) {
            self.draw_cache.remove(&key);
        }
    }

    fn evict_raster_cache(&mut self) {
        const MAX_RASTER_CACHE_ENTRIES: usize = 4096;
        const MAX_UNUSED_FRAMES: u64 = 180;
        let frame_epoch = self.frame_epoch;
        self.raster_cache.retain(|_, entry| {
            frame_epoch.saturating_sub(entry.last_used_frame) <= MAX_UNUSED_FRAMES
        });
        if self.raster_cache.len() <= MAX_RASTER_CACHE_ENTRIES {
            return;
        }
        let mut entries = self
            .raster_cache
            .iter()
            .map(|(key, entry)| (*key, entry.last_used_frame))
            .collect::<Vec<_>>();
        entries.sort_by_key(|(_, last_used)| *last_used);
        let remove_count = self.raster_cache.len() - MAX_RASTER_CACHE_ENTRIES;
        for (key, _) in entries.into_iter().take(remove_count) {
            self.raster_cache.remove(&key);
        }
    }

    fn ensure_common(&mut self, device: &wgpu::Device) {
        if self.screen_layout.is_none() {
            self.screen_layout = Some(device.create_bind_group_layout(
                &wgpu::BindGroupLayoutDescriptor {
                    label: Some("Text Globals Bind Group Layout"),
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::VERTEX,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Uniform,
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::VERTEX,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                    ],
                },
            ));
        }
        if self.atlas_layout.is_none() {
            self.atlas_layout = Some(device.create_bind_group_layout(
                &wgpu::BindGroupLayoutDescriptor {
                    label: Some("Text Atlas Bind Group Layout"),
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                            count: None,
                        },
                    ],
                },
            ));
        }
        if self.sampler.is_none() {
            self.sampler = Some(device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("Text Atlas Sampler"),
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Nearest,
                min_filter: wgpu::FilterMode::Nearest,
                mipmap_filter: wgpu::MipmapFilterMode::Nearest,
                ..Default::default()
            }));
        }
    }

    fn ensure_pipeline(
        &mut self,
        device: &wgpu::Device,
        key: TextRendererKey,
        kind: TextPipelineKind,
    ) {
        self.ensure_common(device);
        if self.pipelines.contains_key(&(key, kind)) {
            return;
        }
        let screen_layout = self.screen_layout.as_ref().expect("screen layout");
        let atlas_layout = self.atlas_layout.as_ref().expect("atlas layout");
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Text Pipeline Layout"),
            bind_group_layouts: &[Some(screen_layout), Some(atlas_layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Text Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../../shader/text.wgsl").into()),
        });
        let fragment_entry = match kind {
            TextPipelineKind::Mask => "fs_mask",
            TextPipelineKind::Color => "fs_color",
        };
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Text Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(text_glyph_vertex_layout())],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some(fragment_entry),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: key.format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: key.stencil_enabled.then_some(text_depth_stencil_state()),
            multisample: wgpu::MultisampleState {
                count: key.sample_count,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        });
        self.pipelines.insert((key, kind), pipeline);
    }

    fn destroy(&mut self) {
        self.pipelines.clear();
        self.raster_cache.clear();
        self.draw_cache.clear();
        for (_, atlas) in self.persistent_atlases.drain() {
            atlas.texture.destroy();
        }
        self.screen_layout = None;
        self.atlas_layout = None;
        self.sampler = None;
        self.scale_context = SwashScaleContext::new();
    }
}

fn text_glyph_vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    const ATTRS: [wgpu::VertexAttribute; 7] = wgpu::vertex_attr_array![
        0 => Float32x2,
        1 => Float32x2,
        2 => Float32x2,
        3 => Float32x2,
        4 => Float32x4,
        5 => Float32,
        6 => Uint32
    ];
    wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<TextGlyphInstance>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &ATTRS,
    }
}

fn text_depth_stencil_state() -> wgpu::DepthStencilState {
    wgpu::DepthStencilState {
        format: wgpu::TextureFormat::Depth24PlusStencil8,
        depth_write_enabled: Some(false),
        depth_compare: Some(wgpu::CompareFunction::Always),
        stencil: wgpu::StencilState {
            front: wgpu::StencilFaceState {
                compare: wgpu::CompareFunction::Equal,
                fail_op: wgpu::StencilOperation::Keep,
                depth_fail_op: wgpu::StencilOperation::Keep,
                pass_op: wgpu::StencilOperation::Keep,
            },
            back: wgpu::StencilFaceState {
                compare: wgpu::CompareFunction::Equal,
                fail_op: wgpu::StencilOperation::Keep,
                depth_fail_op: wgpu::StencilOperation::Keep,
                pass_op: wgpu::StencilOperation::Keep,
            },
            read_mask: 0xff,
            write_mask: 0x00,
        },
        bias: Default::default(),
    }
}

pub fn prewarm_text_pipeline(
    device: &wgpu::Device,
    _queue: &wgpu::Queue,
    format: wgpu::TextureFormat,
    sample_count: u32,
) {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (device, format, sample_count);
        return;
    }
    #[cfg(not(target_arch = "wasm32"))]
    TEXT_RESOURCES.with(|slot| {
        let mut resources = slot.borrow_mut();
        let regular = TextRendererKey {
            format,
            sample_count: sample_count.max(1),
            stencil_enabled: false,
        };
        let stencil = TextRendererKey {
            stencil_enabled: true,
            ..regular
        };
        resources.ensure_pipeline(device, regular, TextPipelineKind::Mask);
        resources.ensure_pipeline(device, regular, TextPipelineKind::Color);
        resources.ensure_pipeline(device, stencil, TextPipelineKind::Mask);
        resources.ensure_pipeline(device, stencil, TextPipelineKind::Color);
    });
}

pub fn clear_text_resources_cache() {
    TEXT_RESOURCES.with(|slot| slot.borrow_mut().destroy());
}

pub fn begin_text_resources_frame() {
    TEXT_RESOURCES.with(|slot| slot.borrow_mut().begin_frame());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::inline_formatting_context::{
        InlineFormattingContext, InlineIfcInput, InlineIfcItem, InlineIfcLayoutOptions,
        InlineIfcSourceId, InlineIfcStyle,
    };
    use crate::view::inline_text_pass_adapter::inline_ifc_glyph_to_text_pass_raster_input;

    #[test]
    fn snap_text_local_pos_snaps_absolute_pixel_position() {
        let fragment_origin = [10.25, 4.75];
        let local = snap_text_local_pos(fragment_origin, [3.90, 2.40]);

        assert_eq!(fragment_origin[0] + local[0], 14.0);
        assert_eq!(fragment_origin[1] + local[1], 7.0);
    }

    #[test]
    fn text_render_trunc_moves_toward_zero() {
        assert_eq!(text_render_trunc(4.9), 4.0);
        assert_eq!(text_render_trunc(-4.9), -4.0);
    }

    #[test]
    fn text_glyph_instance_layout_matches_shader_locations() {
        assert_eq!(std::mem::size_of::<TextGlyphInstance>(), 56);
        let attrs = text_glyph_vertex_layout();
        assert_eq!(attrs.array_stride, 56);
        assert_eq!(attrs.attributes.len(), 7);
        assert_eq!(attrs.attributes[0].offset, 0);
        assert_eq!(attrs.attributes[4].offset, 32);
        assert_eq!(attrs.attributes[6].offset, 52);
    }

    fn first_renderable_raster_input() -> TextPassRasterGlyphInput {
        let ifc = InlineFormattingContext::build_with_options(
            InlineIfcInput::new(vec![InlineIfcItem::TextSpan {
                source: InlineIfcSourceId(1),
                text: "Raster key".to_string(),
                style: Some(InlineIfcStyle {
                    font_size: 17.0,
                    line_height: 1.2,
                    font_weight: 500,
                    brush: [0, 0, 0, 255],
                    font_families: vec!["sans-serif".to_string()],
                }),
            }]),
            InlineIfcLayoutOptions::new(Some(200.0), true),
        );
        let glyph = ifc
            .text_pass_paint_input()
            .glyphs
            .into_iter()
            .find(|glyph| glyph.font_data.is_some())
            .expect("test layout should produce a glyph with font data");
        inline_ifc_glyph_to_text_pass_raster_input(&glyph)
    }

    #[test]
    fn raster_input_key_matches_existing_text_glyph_key_fields() {
        let input = first_renderable_raster_input();
        let scale_factor = 1.75;

        let input_key = text_raster_key_for_raster_input(&input, scale_factor)
            .expect("neutral input should produce a raster key");

        assert_eq!(input_key.glyph_id, input.glyph_id);
        assert_eq!(input_key.font_size_bits, input.font_size.to_bits());
        assert_eq!(input_key.font_blob_id, input.font_data_id);
        assert_eq!(input_key.font_index, input.font_index);
        assert_eq!(
            input_key.normalized_coords_hash,
            input.normalized_coords_hash
        );
        assert_eq!(input_key.scale_factor_bits, scale_factor.to_bits());
    }

    #[test]
    fn raster_input_rejects_stale_font_handle_identity() {
        let mut input = first_renderable_raster_input();
        input.font_data_id = input.font_data_id.wrapping_add(1);

        assert!(text_raster_key_for_raster_input(&input, 1.0).is_none());
    }

    #[test]
    fn raster_input_uses_existing_rasterize_path() {
        let input = first_renderable_raster_input();
        let scale_factor = 1.0;
        let key = text_raster_key_for_raster_input(&input, scale_factor)
            .expect("neutral input should produce a raster key");
        let mut scale_context = SwashScaleContext::new();
        let mut raster_cache = FxHashMap::default();

        let image = rasterize_text_pass_glyph_input(
            &mut scale_context,
            &mut raster_cache,
            42,
            &input,
            scale_factor,
        )
        .expect("neutral input should rasterize through the existing glyph path");

        assert!(!image.data.is_empty());
        assert!(raster_cache.contains_key(&key));
    }

    #[test]
    fn paint_input_is_separate_from_raster_key_fields() {
        let input = first_renderable_raster_input();
        let first_paint = TextPassGlyphPaintInput {
            local_pos: [1.0, 2.0],
            color: [1.0, 0.0, 0.0, 1.0],
            opacity: 0.25,
            fragment_index: 3,
        };
        let second_paint = TextPassGlyphPaintInput {
            local_pos: [8.0, 13.0],
            color: [0.0, 0.0, 1.0, 1.0],
            opacity: 0.95,
            fragment_index: 7,
        };

        let before = text_raster_key_for_raster_input(&input, 2.0)
            .expect("neutral input should produce a raster key");
        let after = text_raster_key_for_raster_input(&input, 2.0)
            .expect("paint changes are not part of raster key input");

        assert_ne!(first_paint, second_paint);
        assert_eq!(before, after);
    }

    #[test]
    fn prepared_staging_probe_uses_existing_raster_and_instance_metadata() {
        let raster = first_renderable_raster_input();
        let scale_factor = 1.5;
        let paint = TextPassGlyphPaintInput {
            local_pos: [2.25, 7.5],
            color: [0.2, 0.4, 0.6, 1.0],
            opacity: 0.5,
            fragment_index: 11,
        };
        let input = TextPassPreparedStagingInput {
            scale_factor,
            glyphs: vec![TextPassPreparedStagingGlyphInput {
                raster: raster.clone(),
                paint,
                final_paint_pos: [23.0 + paint.local_pos[0], 29.0 + paint.local_pos[1]],
            }],
        };

        let probe = build_text_pass_prepared_staging_probe(&input);

        assert_eq!(probe.scale_factor, scale_factor);
        assert_eq!(probe.glyphs.len(), 1);
        let staged = &probe.glyphs[0];
        assert_eq!(staged.glyph_index, 0);
        assert_eq!(
            staged.raster_key,
            text_raster_key_for_raster_input(&raster, scale_factor)
        );
        assert_eq!(staged.paint, paint);
        assert_eq!(staged.final_paint_pos, input.glyphs[0].final_paint_pos);
        assert!(staged.instance_size[0] >= 1.0);
        assert!(staged.instance_size[1] >= 1.0);
        assert!(matches!(
            staged.atlas_kind,
            TextPassPreparedStagingAtlasKind::Mask | TextPassPreparedStagingAtlasKind::Color
        ));
    }
}

#[cfg(test)]
mod shader_validation_tests {
    #[test]
    fn text_wgsl_validates() {
        let source = include_str!("../../shader/text.wgsl");
        let module = naga29::front::wgsl::parse_str(source).expect("parse text.wgsl");
        naga29::valid::Validator::new(
            naga29::valid::ValidationFlags::default(),
            naga29::valid::Capabilities::default(),
        )
        .validate(&module)
        .expect("validate text.wgsl");
    }
}
