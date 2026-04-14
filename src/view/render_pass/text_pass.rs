use crate::view::font_system::with_shared_font_system;
use crate::view::frame_graph::{
    GraphicsColorAttachmentOps, GraphicsPassBuilder, GraphicsPassMergePolicy, PrepareContext,
};
use crate::view::render_pass::draw_rect_pass::RenderTargetOut;
use crate::view::render_pass::render_target::{
    GraphicsPassContext as RenderPassContext, logical_scissor_to_target_physical,
    render_target_origin, render_target_sample_count, resolve_texture_ref,
};
use crate::view::render_pass::{GraphicsCtx, GraphicsPass};
use crate::view::text_layout::build_text_buffer;
use cosmic_text::{
    Align, Buffer, CacheKey, Color as CosmicColor, FontSystem, SwashCache, SwashContent, SwashImage,
};
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::num::NonZeroU64;
use std::sync::Arc;
use wgpu::util::DeviceExt;
pub struct TextPass {
    params: TextPassParams,
    prepared: Option<TextPreparedState>,
    input: TextInput,
    output: TextOutput,
}

const TEXT_RASTER_SCALE_SMALL: f32 = 2.0;
const TEXT_RASTER_SCALE_LARGE: f32 = 1.0;
const TEXT_RASTER_SMALL_FONT_SIZE: f32 = 24.0;
const TEXT_RASTER_LARGE_FONT_SIZE: f32 = 96.0;

fn text_raster_scale(font_size: f32) -> f32 {
    if !font_size.is_finite() {
        return TEXT_RASTER_SCALE_SMALL;
    }
    if font_size <= TEXT_RASTER_SMALL_FONT_SIZE {
        return TEXT_RASTER_SCALE_SMALL;
    }
    if font_size >= TEXT_RASTER_LARGE_FONT_SIZE {
        return TEXT_RASTER_SCALE_LARGE;
    }

    let t = (font_size - TEXT_RASTER_SMALL_FONT_SIZE)
        / (TEXT_RASTER_LARGE_FONT_SIZE - TEXT_RASTER_SMALL_FONT_SIZE);
    TEXT_RASTER_SCALE_SMALL + (TEXT_RASTER_SCALE_LARGE - TEXT_RASTER_SCALE_SMALL) * t
}

#[derive(Clone)]
pub struct TextPassFragment {
    pub content: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub color: [f32; 4],
    pub opacity: f32,
    pub layout_buffer: Option<Arc<Buffer>>,
}

pub struct TextPassParams {
    pub fragments: Vec<TextPassFragment>,
    pub font_size: f32,
    pub line_height: f32,
    pub font_weight: u16,
    pub font_families: Vec<String>,
    pub align: Align,
    pub allow_wrap: bool,
    pub scissor_rect: Option<[u32; 4]>,
    pub stencil_clip_id: Option<u8>,
}

impl TextPassParams {
    pub fn single_fragment(
        fragment: TextPassFragment,
        font_size: f32,
        line_height: f32,
        font_weight: u16,
        font_families: Vec<String>,
        align: Align,
        allow_wrap: bool,
        scissor_rect: Option<[u32; 4]>,
        stencil_clip_id: Option<u8>,
    ) -> Self {
        Self {
            fragments: vec![fragment],
            font_size,
            line_height,
            font_weight,
            font_families,
            align,
            allow_wrap,
            scissor_rect,
            stencil_clip_id,
        }
    }
}

struct TextPreparedState {
    renderer_key: TextRendererKey,
    mask_draw: Option<PreparedTextDraw>,
    color_draw: Option<PreparedTextDraw>,
    globals_bind_group: wgpu::BindGroup,
    prepare_signature: u64,
    atlas_generation: AtlasGenerations,
    stencil_clip_id: Option<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct TextRendererKey {
    sample_count: u32,
    stencil_enabled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum TextPipelineKind {
    Mask,
    Color,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
struct AtlasGenerations {
    mask: u64,
    color: u64,
}

struct TextDebugOverlay {
    vertices: Vec<TextDebugVertex>,
    indices: Vec<u32>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(crate) struct TextBounds {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
struct TextScreenUniform {
    screen_size: [f32; 2],
    _pad: [f32; 2],
}

#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
struct TextFragmentUniform {
    origin: [f32; 2],
    clip_min: [f32; 2],
    clip_max: [f32; 2],
    _pad: [f32; 2],
}

#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
struct TextGlyphVertex {
    local_pos: [f32; 2],
    size: [f32; 2],
    uv_min: [f32; 2],
    uv_max: [f32; 2],
    color: [f32; 4],
    opacity: f32,
    fragment_index: u32,
}

struct TextArea<'a> {
    buffer: &'a Buffer,
    left: f32,
    top: f32,
    scale: f32,
    clip_min: [f32; 2],
    clip_max: [f32; 2],
    default_color: CosmicColor,
}

#[derive(Clone)]
struct PreparedTextDraw {
    vertex_buffer: wgpu::Buffer,
    vertex_count: u32,
}

#[derive(Clone, Default)]
struct PreparedTextDrawSet {
    mask_draw: Option<PreparedTextDraw>,
    color_draw: Option<PreparedTextDraw>,
}

#[derive(Default, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
struct TextDebugVertex {
    position: [f32; 2],
    color: [f32; 4],
}

#[derive(Default)]
pub struct TextInput {
    pub pass_context: RenderPassContext,
}

#[derive(Default)]
pub struct TextOutput {
    pub render_target: RenderTargetOut,
}

impl TextPass {
    pub fn new(params: TextPassParams, input: TextInput, output: TextOutput) -> Self {
        Self {
            params,
            prepared: None,
            input,
            output,
        }
    }

    fn prepare_signature(
        &self,
        scale: f32,
        screen_size: (u32, u32),
        target_origin: (u32, u32),
    ) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.params.font_size.to_bits().hash(&mut hasher);
        self.params.line_height.to_bits().hash(&mut hasher);
        self.params.font_weight.hash(&mut hasher);
        self.params.font_families.hash(&mut hasher);
        std::mem::discriminant(&self.params.align).hash(&mut hasher);
        self.params.allow_wrap.hash(&mut hasher);
        self.params.scissor_rect.hash(&mut hasher);
        self.params.stencil_clip_id.hash(&mut hasher);
        scale.to_bits().hash(&mut hasher);
        screen_size.hash(&mut hasher);
        target_origin.hash(&mut hasher);
        for fragment in &self.params.fragments {
            fragment.x.to_bits().hash(&mut hasher);
            fragment.y.to_bits().hash(&mut hasher);
            fragment.content.hash(&mut hasher);
            fragment.width.to_bits().hash(&mut hasher);
            fragment.height.to_bits().hash(&mut hasher);
            fragment.opacity.to_bits().hash(&mut hasher);
            for channel in fragment.color {
                channel.to_bits().hash(&mut hasher);
            }
        }
        hasher.finish()
    }
}

impl GraphicsPass for TextPass {
    fn setup(&mut self, builder: &mut GraphicsPassBuilder<'_, '_>) {
        builder.set_graphics_merge_policy(GraphicsPassMergePolicy::Mergeable);
        if let Some(target) = builder.texture_target(&self.output.render_target) {
            let _ = target;
            builder.write_color(
                &self.output.render_target,
                GraphicsColorAttachmentOps::load(),
            );
        } else {
            builder.write_surface_color(GraphicsColorAttachmentOps::load());
        }
        self.params.scissor_rect = intersect_scissor_rects(
            self.input.pass_context.scissor_rect,
            self.params.scissor_rect,
        );
        if self.params.stencil_clip_id.is_none() {
            self.params.stencil_clip_id = self.input.pass_context.stencil_clip_id;
        }
        if self.input.pass_context.uses_depth_stencil {
            builder.read_output_depth();
            builder.read_output_stencil();
        }
    }

    fn prepare(&mut self, ctx: &mut PrepareContext<'_, '_>) {
        if self.params.fragments.is_empty() {
            return;
        }

        let output_handle = self.output.render_target.handle();
        let fallback_surface_size = ctx.viewport.surface_size();
        let target_meta = resolve_texture_ref(output_handle, ctx, fallback_surface_size, None);
        let target_size = target_meta.physical_size;
        let target_origin = output_handle
            .and_then(|handle| render_target_origin(ctx, handle))
            .unwrap_or((0, 0));
        let output_sample_count = output_handle
            .and_then(|handle| render_target_sample_count(ctx, handle))
            .unwrap_or_else(|| ctx.viewport.msaa_sample_count());

        let viewport = &mut ctx.viewport;
        let device = match viewport.device().cloned() {
            Some(device) => device,
            None => return,
        };
        let queue = match viewport.queue().cloned() {
            Some(queue) => queue,
            None => return,
        };
        let (screen_w, screen_h) = target_size;
        let scale = viewport.scale_factor();
        let format = viewport.surface_format();
        with_text_resources(&device, &queue, format, |resources| {
            let renderer_key = TextRendererKey {
                sample_count: output_sample_count,
                stencil_enabled: self.input.pass_context.uses_depth_stencil,
            };
            let atlas_generation = resources.atlas_generations();
            let prepare_signature =
                self.prepare_signature(scale, (screen_w, screen_h), target_origin);
            if let Some(prepared) = self.prepared.as_mut() {
                if prepared.renderer_key == renderer_key
                    && prepared.prepare_signature == prepare_signature
                    && prepared.atlas_generation == atlas_generation
                {
                    prepared.stencil_clip_id = self.params.stencil_clip_id;
                    return;
                }
            }
            // Fast path: if both the globals bind group and the glyph vertex draw are cached,
            // skip the fragment loop entirely.
            let cached_globals = resources
                .globals_bind_groups
                .get(&prepare_signature)
                .cloned();
            if let Some(ref globals) = cached_globals {
                if let Some(draw) =
                    resources.take_prepared_draw(renderer_key, prepare_signature, atlas_generation)
                {
                    self.prepared = Some(TextPreparedState {
                        renderer_key,
                        mask_draw: draw.mask_draw,
                        color_draw: draw.color_draw,
                        globals_bind_group: globals.bind_group.clone(),
                        prepare_signature,
                        atlas_generation,
                        stencil_clip_id: self.params.stencil_clip_id,
                    });
                    return;
                }
            }
            let physical_scissor_rect = self.params.scissor_rect.and_then(|scissor_rect| {
                logical_scissor_to_target_physical(
                    viewport,
                    scissor_rect,
                    target_origin,
                    (screen_w, screen_h),
                )
            });
            let mut resolved_buffers = vec![None; self.params.fragments.len()];
            let mut resolved_bounds = vec![None; self.params.fragments.len()];
            let mut combined_bounds: Option<TextBounds> = None;

            for (index, fragment) in self.params.fragments.iter().enumerate() {
                if fragment.content.is_empty()
                    || fragment.width <= 0.0
                    || fragment.height <= 0.0
                    || !fragment.x.is_finite()
                    || !fragment.y.is_finite()
                    || !fragment.width.is_finite()
                    || !fragment.height.is_finite()
                {
                    continue;
                }
                let bounds = match resolve_text_bounds(
                    fragment.x * scale - target_origin.0 as f32
                        + target_meta.logical_origin.0 as f32,
                    fragment.y * scale - target_origin.1 as f32
                        + target_meta.logical_origin.1 as f32,
                    fragment.width * scale,
                    fragment.height * scale,
                    screen_w,
                    screen_h,
                    physical_scissor_rect,
                ) {
                    Some(bounds) => bounds,
                    None => continue,
                };
                resolved_bounds[index] = Some(bounds);
                combined_bounds = Some(match combined_bounds {
                    Some(current) => TextBounds {
                        left: current.left.min(bounds.left),
                        top: current.top.min(bounds.top),
                        right: current.right.max(bounds.right),
                        bottom: current.bottom.max(bounds.bottom),
                    },
                    None => bounds,
                });
                if fragment.layout_buffer.is_none() {
                    let layout_signature = single_fragment_layout_signature(
                        fragment,
                        self.params.font_size,
                        self.params.line_height,
                        self.params.font_weight,
                        self.params.font_families.as_slice(),
                        self.params.align,
                        self.params.allow_wrap,
                    );
                    resolved_buffers[index] = Some(resources.prepare_buffer_cached(
                        layout_signature,
                        fragment.content.as_str(),
                        fragment.width,
                        fragment.height,
                        self.params.font_size,
                        self.params.line_height,
                        self.params.font_weight,
                        self.params.font_families.as_slice(),
                        self.params.align,
                        self.params.allow_wrap,
                    ));
                }
            }
            let Some(_bounds) = combined_bounds else {
                return;
            };
            let mut text_areas = Vec::new();
            for (index, fragment) in self.params.fragments.iter().enumerate() {
                let Some(fragment_bounds) = resolved_bounds[index] else {
                    continue;
                };
                let buffer = fragment
                    .layout_buffer
                    .as_deref()
                    .or_else(|| resolved_buffers[index].as_deref())
                    .expect("buffer should be resolved for visible text fragment");
                text_areas.push(build_text_area(
                    buffer,
                    fragment.x * scale - target_origin.0 as f32
                        + target_meta.logical_origin.0 as f32,
                    fragment.y * scale - target_origin.1 as f32
                        + target_meta.logical_origin.1 as f32,
                    scale,
                    fragment_bounds,
                    to_cosmic_color(fragment.color, fragment.opacity),
                ));
            }
            let (_, _, globals_bind_group, _) = resources.get_or_create_globals_bind_group(
                &device,
                &queue,
                screen_w as f32,
                screen_h as f32,
                &text_areas,
                prepare_signature,
            );
            if let Some(draw) =
                resources.take_prepared_draw(renderer_key, prepare_signature, atlas_generation)
            {
                self.prepared = Some(TextPreparedState {
                    renderer_key,
                    mask_draw: draw.mask_draw,
                    color_draw: draw.color_draw,
                    globals_bind_group,
                    prepare_signature,
                    atlas_generation,
                    stencil_clip_id: self.params.stencil_clip_id,
                });
                return;
            }
            if viewport.debug_geometry_overlay() {
                let (overlay_w, overlay_h) = viewport.surface_size();
                let overlay = build_text_debug_overlay_multi(
                    &self.params.fragments,
                    &resolved_buffers,
                    &resolved_bounds,
                    scale,
                    target_origin,
                    target_meta.logical_origin,
                    overlay_w as f32,
                    overlay_h as f32,
                );
                if !overlay.vertices.is_empty() && !overlay.indices.is_empty() {
                    let overlay_vertices: Vec<
                        crate::view::render_pass::debug_overlay_pass::DebugOverlayVertex,
                    > = overlay
                        .vertices
                        .iter()
                        .map(|vertex| {
                            crate::view::render_pass::debug_overlay_pass::DebugOverlayVertex {
                                position: vertex.position,
                                color: vertex.color,
                            }
                        })
                        .collect();
                    viewport.push_debug_overlay_geometry(&overlay_vertices, &overlay.indices);
                }
            }

            let (mask_vertices, color_vertices) = with_shared_font_system(|font_system| {
                let mut retry_count = 0;
                loop {
                    let mut mask_vertices = Vec::new();
                    let mut color_vertices = Vec::new();
                    let mut atlas_full = false;
                    for (fragment_index, text_area) in text_areas.iter().enumerate() {
                        if resources
                            .prepare_text_area(
                                font_system,
                                &device,
                                &queue,
                                text_area,
                                fragment_index as u32,
                                &mut mask_vertices,
                                &mut color_vertices,
                            )
                            .is_err()
                        {
                            atlas_full = true;
                            break;
                        }
                    }
                    if !atlas_full {
                        break (mask_vertices, color_vertices);
                    }
                    if retry_count >= 1 {
                        break (Vec::new(), Vec::new());
                    }
                    retry_count += 1;
                    resources.reset_atlas();
                }
            });
            if mask_vertices.is_empty() && color_vertices.is_empty() {
                return;
            }
            resources.flush_atlas_uploads(&queue);
            let prepared_draw = PreparedTextDrawSet {
                mask_draw: create_prepared_draw(
                    &device,
                    "Text Mask Glyph Vertex Buffer",
                    &mask_vertices,
                ),
                color_draw: create_prepared_draw(
                    &device,
                    "Text Color Glyph Vertex Buffer",
                    &color_vertices,
                ),
            };
            let atlas_generation = resources.atlas_generations();
            resources.put_prepared_draw(
                renderer_key,
                prepare_signature,
                atlas_generation,
                prepared_draw.clone(),
            );
            self.prepared = Some(TextPreparedState {
                renderer_key,
                mask_draw: prepared_draw.mask_draw,
                color_draw: prepared_draw.color_draw,
                globals_bind_group,
                prepare_signature,
                atlas_generation,
                stencil_clip_id: self.params.stencil_clip_id,
            });
        });
    }

    fn execute(&mut self, ctx: &mut GraphicsCtx<'_, '_, '_, '_>) {
        let Some(prepared) = self.prepared.as_mut() else {
            return;
        };
        let device = match ctx.viewport().device().cloned() {
            Some(device) => device,
            None => return,
        };
        let format = ctx.viewport().surface_format();
        with_text_resources_for_render(format, |resources| {
            let fallback_surface_size = ctx.viewport().surface_size();
            let target_meta = resolve_texture_ref(
                self.output.render_target.handle(),
                ctx.frame_resources(),
                fallback_surface_size,
                None,
            );
            let target_size = target_meta.physical_size;
            let target_origin = self
                .output
                .render_target
                .handle()
                .and_then(|handle| render_target_origin(ctx.frame_resources(), handle))
                .unwrap_or((0, 0));
            let scissor_rect = self.params.scissor_rect.and_then(|scissor_rect| {
                logical_scissor_to_target_physical(
                    ctx.viewport(),
                    scissor_rect,
                    target_origin,
                    target_size,
                )
            });
            let stencil_reference = prepared.stencil_clip_id.map(|id| id as u32).unwrap_or(0);
            if let Some([x, y, width, height]) = scissor_rect {
                ctx.set_scissor_rect(x, y, width, height);
            } else {
                ctx.set_scissor_rect(0, 0, target_size.0, target_size.1);
            }
            ctx.set_stencil_reference(stencil_reference);
            ctx.set_bind_group(0, &prepared.globals_bind_group, &[]);
            if let Some(draw) = prepared.mask_draw.as_ref() {
                resources.ensure_pipeline(&device, prepared.renderer_key, TextPipelineKind::Mask);
                let pipeline = resources.pipeline(prepared.renderer_key, TextPipelineKind::Mask);
                ctx.set_pipeline(pipeline);
                ctx.set_bind_group(1, &resources.mask_atlas.bind_group, &[]);
                ctx.set_vertex_buffer(0, draw.vertex_buffer.slice(..));
                ctx.draw(0..6, 0..draw.vertex_count);
            }
            if let Some(draw) = prepared.color_draw.as_ref() {
                resources.ensure_pipeline(&device, prepared.renderer_key, TextPipelineKind::Color);
                let pipeline = resources.pipeline(prepared.renderer_key, TextPipelineKind::Color);
                ctx.set_pipeline(pipeline);
                ctx.set_bind_group(1, &resources.color_atlas.bind_group, &[]);
                ctx.set_vertex_buffer(0, draw.vertex_buffer.slice(..));
                ctx.draw(0..6, 0..draw.vertex_count);
            }
        });
    }
}

fn intersect_scissor_rects(a: Option<[u32; 4]>, b: Option<[u32; 4]>) -> Option<[u32; 4]> {
    match (a, b) {
        (None, None) => None,
        (Some(rect), None) | (None, Some(rect)) => Some(rect),
        (Some([ax, ay, aw, ah]), Some([bx, by, bw, bh])) => {
            let a_right = ax.saturating_add(aw);
            let a_bottom = ay.saturating_add(ah);
            let b_right = bx.saturating_add(bw);
            let b_bottom = by.saturating_add(bh);
            let left = ax.max(bx);
            let top = ay.max(by);
            let right = a_right.min(b_right);
            let bottom = a_bottom.min(b_bottom);
            if right <= left || bottom <= top {
                return None;
            }
            Some([left, top, right - left, bottom - top])
        }
    }
}

fn single_fragment_layout_signature(
    fragment: &TextPassFragment,
    font_size: f32,
    line_height: f32,
    font_weight: u16,
    font_families: &[String],
    align: Align,
    allow_wrap: bool,
) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    fragment.content.hash(&mut hasher);
    fragment.width.to_bits().hash(&mut hasher);
    fragment.height.to_bits().hash(&mut hasher);
    font_size.to_bits().hash(&mut hasher);
    line_height.to_bits().hash(&mut hasher);
    font_weight.hash(&mut hasher);
    font_families.hash(&mut hasher);
    std::mem::discriminant(&align).hash(&mut hasher);
    allow_wrap.hash(&mut hasher);
    hasher.finish()
}

fn to_cosmic_color(color: [f32; 4], opacity: f32) -> CosmicColor {
    fn to_u8(v: f32) -> u8 {
        (v.clamp(0.0, 1.0) * 255.0).round() as u8
    }

    let alpha = to_u8(color[3] * opacity.clamp(0.0, 1.0));
    CosmicColor::rgba(to_u8(color[0]), to_u8(color[1]), to_u8(color[2]), alpha)
}

fn intersect_rect(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    let left = a[0].max(b[0]);
    let top = a[1].max(b[1]);
    let right = a[2].min(b[2]);
    let bottom = a[3].min(b[3]);
    [left, top, right, bottom]
}

fn resolve_text_bounds(
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    screen_w: u32,
    screen_h: u32,
    scissor_rect: Option<[u32; 4]>,
) -> Option<TextBounds> {
    let viewport_bounds = [0.0, 0.0, screen_w as f32, screen_h as f32];
    let text_bounds = [x, y, (x + width).max(x), (y + height).max(y)];
    let mut clipped = intersect_rect(text_bounds, viewport_bounds);

    if let Some([sx, sy, sw, sh]) = scissor_rect {
        let scissor_bounds = [
            sx as f32,
            sy as f32,
            sx.saturating_add(sw) as f32,
            sy.saturating_add(sh) as f32,
        ];
        clipped = intersect_rect(clipped, scissor_bounds);
    }

    if clipped[2] <= clipped[0] || clipped[3] <= clipped[1] {
        return None;
    }

    Some(TextBounds {
        left: clipped[0].floor() as i32,
        top: clipped[1].floor() as i32,
        right: clipped[2].ceil() as i32,
        bottom: clipped[3].ceil() as i32,
    })
}

#[cfg(test)]
fn physical_scissor_rect(
    scissor_rect: Option<[u32; 4]>,
    scale: f32,
    target_size: (u32, u32),
) -> Option<[u32; 4]> {
    let [x, y, width, height] = scissor_rect?;
    let scale = scale.max(0.0001);
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

fn build_text_area<'a>(
    buffer: &'a Buffer,
    left: f32,
    top: f32,
    scale: f32,
    bounds: TextBounds,
    default_color: CosmicColor,
) -> TextArea<'a> {
    TextArea {
        buffer,
        left,
        top,
        scale,
        clip_min: [bounds.left as f32 - left, bounds.top as f32 - top],
        clip_max: [bounds.right as f32 - left, bounds.bottom as f32 - top],
        default_color,
    }
}

fn create_prepared_draw(
    device: &wgpu::Device,
    label: &'static str,
    vertices: &[TextGlyphVertex],
) -> Option<PreparedTextDraw> {
    if vertices.is_empty() {
        return None;
    }
    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: bytemuck::cast_slice(vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    Some(PreparedTextDraw {
        vertex_buffer,
        vertex_count: vertices.len() as u32,
    })
}

#[derive(Clone, Copy)]
struct AtlasGlyph {
    x: u32,
    y: u32,
    raster_scale: f32,
    layout_width: f32,
    layout_height: f32,
    layout_left: f32,
    layout_top: f32,
}

struct TextAtlas {
    texture: wgpu::Texture,
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    bytes_per_pixel: u32,
    size: u32,
    cursor_x: u32,
    cursor_y: u32,
    row_height: u32,
    glyphs: HashMap<CacheKey, AtlasGlyph>,
    pending_uploads: Vec<AtlasUpload>,
    generation: u64,
}

struct AtlasUpload {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    data: Vec<u8>,
}

impl TextAtlas {
    fn new(
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        label: &'static str,
        format: wgpu::TextureFormat,
        bytes_per_pixel: u32,
    ) -> Self {
        let size = device.limits().max_texture_dimension_2d.min(4096).max(256);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Text Atlas Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Text Atlas Bind Group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        Self {
            texture,
            bind_group_layout,
            bind_group,
            bytes_per_pixel,
            size,
            cursor_x: 1,
            cursor_y: 1,
            row_height: 0,
            glyphs: HashMap::new(),
            pending_uploads: Vec::new(),
            generation: 0,
        }
    }

    fn generation(&self) -> u64 {
        self.generation
    }

    fn reset(&mut self) {
        self.cursor_x = 1;
        self.cursor_y = 1;
        self.row_height = 0;
        self.glyphs.clear();
        self.pending_uploads.clear();
        self.generation = self.generation.wrapping_add(1);
    }

    fn get_glyph(&self, cache_key: CacheKey) -> Option<AtlasGlyph> {
        self.glyphs.get(&cache_key).copied()
    }

    fn cache_glyph(
        &mut self,
        cache_key: CacheKey,
        image_data: Vec<u8>,
        width: u32,
        height: u32,
        left: i32,
        top: i32,
        raster_scale: f32,
    ) -> Result<Option<AtlasGlyph>, TextAtlasCacheError> {
        if let Some(glyph) = self.get_glyph(cache_key) {
            return Ok(Some(glyph));
        }
        if width == 0 || height == 0 {
            return Ok(None);
        }
        let padded_width = width.saturating_add(2);
        let padded_height = height.saturating_add(2);
        if padded_width >= self.size || padded_height >= self.size {
            return Ok(None);
        }
        if self.cursor_x.saturating_add(padded_width) >= self.size {
            self.cursor_x = 1;
            self.cursor_y = self
                .cursor_y
                .saturating_add(self.row_height)
                .saturating_add(1);
            self.row_height = 0;
        }
        if self.cursor_y.saturating_add(padded_height) >= self.size {
            return Err(TextAtlasCacheError::Full);
        }

        let allocation_x = self.cursor_x;
        let allocation_y = self.cursor_y;
        self.cursor_x = self.cursor_x.saturating_add(padded_width);
        self.row_height = self.row_height.max(padded_height);

        let mut padded_data =
            vec![
                0_u8;
                padded_width as usize * padded_height as usize * self.bytes_per_pixel as usize
            ];
        for row in 0..height as usize {
            let row_bytes = width as usize * self.bytes_per_pixel as usize;
            let source_start = row * row_bytes;
            let source_end = source_start + row_bytes;
            let target_start =
                ((row + 1) * padded_width as usize + 1) * self.bytes_per_pixel as usize;
            let target_end = target_start + row_bytes;
            padded_data[target_start..target_end]
                .copy_from_slice(&image_data[source_start..source_end]);
        }
        self.pending_uploads.push(AtlasUpload {
            x: allocation_x,
            y: allocation_y,
            width: padded_width,
            height: padded_height,
            data: padded_data,
        });

        let glyph = AtlasGlyph {
            x: allocation_x + 1,
            y: allocation_y + 1,
            raster_scale,
            layout_width: width as f32 / raster_scale,
            layout_height: height as f32 / raster_scale,
            layout_left: left as f32 / raster_scale,
            layout_top: top as f32 / raster_scale,
        };
        self.glyphs.insert(cache_key, glyph);
        Ok(Some(glyph))
    }

    fn flush_uploads(&mut self, queue: &wgpu::Queue) {
        let uploads = std::mem::take(&mut self.pending_uploads);
        if uploads.is_empty() {
            return;
        }
        let mut index = 0;
        while index < uploads.len() {
            let row_y = uploads[index].y;
            let row_x = uploads[index].x;
            let mut row_right = uploads[index].x + uploads[index].width;
            let mut row_height = uploads[index].height;
            let mut end = index + 1;
            while end < uploads.len() && uploads[end].y == row_y {
                row_right = row_right.max(uploads[end].x + uploads[end].width);
                row_height = row_height.max(uploads[end].height);
                end += 1;
            }

            let row_width = row_right - row_x;
            let mut row_data =
                vec![
                    0_u8;
                    row_width as usize * row_height as usize * self.bytes_per_pixel as usize
                ];
            for upload in &uploads[index..end] {
                for row in 0..upload.height as usize {
                    let row_bytes = upload.width as usize * self.bytes_per_pixel as usize;
                    let source_start = row * row_bytes;
                    let source_end = source_start + row_bytes;
                    let target_x = upload.x - row_x;
                    let target_start = (row * row_width as usize + target_x as usize)
                        * self.bytes_per_pixel as usize;
                    let target_end = target_start + row_bytes;
                    row_data[target_start..target_end]
                        .copy_from_slice(&upload.data[source_start..source_end]);
                }
            }

            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: row_x,
                        y: row_y,
                        z: 0,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &row_data,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row_width * self.bytes_per_pixel),
                    rows_per_image: None,
                },
                wgpu::Extent3d {
                    width: row_width,
                    height: row_height,
                    depth_or_array_layers: 1,
                },
            );
            index = end;
        }
    }
}

enum TextAtlasCacheError {
    Full,
}

fn swash_color_image_to_rgba(image: &SwashImage) -> Vec<u8> {
    let pixel_count = image.placement.width.saturating_mul(image.placement.height) as usize;
    let mut out = Vec::with_capacity(pixel_count * 4);
    for index in 0..pixel_count {
        let offset = index * 4;
        if offset + 4 <= image.data.len() {
            out.extend_from_slice(&image.data[offset..offset + 4]);
        } else {
            out.extend_from_slice(&[0, 0, 0, 0]);
        }
    }
    out
}

fn swash_mask_image_to_r8(image: &SwashImage) -> Vec<u8> {
    let pixel_count = image.placement.width.saturating_mul(image.placement.height) as usize;
    let mut out = Vec::with_capacity(pixel_count);
    match image.content {
        SwashContent::Mask => {
            for index in 0..pixel_count {
                out.push(image.data.get(index).copied().unwrap_or(0));
            }
        }
        SwashContent::SubpixelMask => {
            for index in 0..pixel_count {
                let offset = index * 3;
                let chunk = image
                    .data
                    .get(offset..offset.saturating_add(3))
                    .unwrap_or(&[]);
                out.push(chunk.iter().copied().max().unwrap_or(0));
            }
        }
        SwashContent::Color => {}
    }
    out
}

struct TextResources {
    swash_cache: SwashCache,
    mask_atlas: TextAtlas,
    color_atlas: TextAtlas,
    format: wgpu::TextureFormat,
    screen_bind_group_layout: wgpu::BindGroupLayout,
    pipelines: HashMap<(TextRendererKey, TextPipelineKind), wgpu::RenderPipeline>,
    layout_buffers: HashMap<u64, Arc<Buffer>>,
    layout_buffer_lru: VecDeque<u64>,
    prepared_draws: HashMap<PreparedTextDrawKey, PreparedTextDrawSet>,
    prepared_draw_lru: VecDeque<PreparedTextDrawKey>,
    globals_bind_groups: HashMap<u64, CachedGlobalsBindGroup>,
    globals_bind_group_lru: VecDeque<u64>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct PreparedTextDrawKey {
    renderer_key: TextRendererKey,
    prepare_signature: u64,
    atlas_generation: AtlasGenerations,
}

#[derive(Clone)]
struct CachedGlobalsBindGroup {
    screen_buffer: wgpu::Buffer,
    fragment_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    capacity: usize,
}

impl TextResources {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let swash_cache = SwashCache::new();
        let screen_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Text Screen Bind Group Layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: Some(
                                NonZeroU64::new(std::mem::size_of::<TextScreenUniform>() as u64)
                                    .expect("text screen uniform size must be non-zero"),
                            ),
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: Some(
                                NonZeroU64::new(std::mem::size_of::<TextFragmentUniform>() as u64)
                                    .expect("text fragment uniform size must be non-zero"),
                            ),
                        },
                        count: None,
                    },
                ],
            });
        let mask_atlas = TextAtlas::new(
            device,
            queue,
            "Text Mask Atlas Texture",
            wgpu::TextureFormat::R8Unorm,
            1,
        );
        let color_atlas = TextAtlas::new(
            device,
            queue,
            "Text Color Atlas Texture",
            wgpu::TextureFormat::Rgba8Unorm,
            4,
        );

        Self {
            swash_cache,
            mask_atlas,
            color_atlas,
            format,
            screen_bind_group_layout,
            pipelines: HashMap::new(),
            layout_buffers: HashMap::new(),
            layout_buffer_lru: VecDeque::new(),
            prepared_draws: HashMap::new(),
            prepared_draw_lru: VecDeque::new(),
            globals_bind_groups: HashMap::new(),
            globals_bind_group_lru: VecDeque::new(),
        }
    }

    fn reset_atlas(&mut self) {
        self.mask_atlas.reset();
        self.color_atlas.reset();
        self.prepared_draws.clear();
        self.prepared_draw_lru.clear();
        self.swash_cache.image_cache.clear();
        self.swash_cache.outline_command_cache.clear();
    }

    fn atlas_generations(&self) -> AtlasGenerations {
        AtlasGenerations {
            mask: self.mask_atlas.generation(),
            color: self.color_atlas.generation(),
        }
    }

    fn flush_atlas_uploads(&mut self, queue: &wgpu::Queue) {
        self.mask_atlas.flush_uploads(queue);
        self.color_atlas.flush_uploads(queue);
    }

    fn take_prepared_draw(
        &mut self,
        renderer_key: TextRendererKey,
        prepare_signature: u64,
        atlas_generation: AtlasGenerations,
    ) -> Option<PreparedTextDrawSet> {
        let key = PreparedTextDrawKey {
            renderer_key,
            prepare_signature,
            atlas_generation,
        };
        let draw = self.prepared_draws.get(&key).cloned()?;
        self.prepared_draw_lru.retain(|current| *current != key);
        self.prepared_draw_lru.push_back(key);
        Some(draw)
    }

    fn put_prepared_draw(
        &mut self,
        renderer_key: TextRendererKey,
        prepare_signature: u64,
        atlas_generation: AtlasGenerations,
        draw: PreparedTextDrawSet,
    ) {
        let key = PreparedTextDrawKey {
            renderer_key,
            prepare_signature,
            atlas_generation,
        };
        self.prepared_draws.insert(key, draw);
        self.prepared_draw_lru.retain(|current| *current != key);
        self.prepared_draw_lru.push_back(key);
        while self.prepared_draw_lru.len() > 512 {
            if let Some(old_key) = self.prepared_draw_lru.pop_front() {
                self.prepared_draws.remove(&old_key);
            }
        }
    }

    fn ensure_pipeline(
        &mut self,
        device: &wgpu::Device,
        key: TextRendererKey,
        kind: TextPipelineKind,
    ) {
        if self.pipelines.contains_key(&(key, kind)) {
            return;
        }
        let pipeline = create_text_pipeline(
            device,
            self.format,
            key.sample_count,
            key.stencil_enabled,
            kind,
            &self.screen_bind_group_layout,
            &self.mask_atlas.bind_group_layout,
        );
        self.pipelines.insert((key, kind), pipeline);
    }

    fn pipeline(&self, key: TextRendererKey, kind: TextPipelineKind) -> &wgpu::RenderPipeline {
        self.pipelines
            .get(&(key, kind))
            .expect("text pipeline should be created before render")
    }

    fn create_globals_bind_group(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        screen_w: f32,
        screen_h: f32,
        text_areas: &[TextArea<'_>],
        existing: Option<(&wgpu::Buffer, &wgpu::Buffer, &wgpu::BindGroup, usize)>,
    ) -> (wgpu::Buffer, wgpu::Buffer, wgpu::BindGroup, usize) {
        let screen_uniform = TextScreenUniform {
            screen_size: [screen_w, screen_h],
            _pad: [0.0, 0.0],
        };
        let fragment_uniforms: Vec<TextFragmentUniform> = text_areas
            .iter()
            .map(|text_area| TextFragmentUniform {
                origin: [text_area.left, text_area.top],
                clip_min: [
                    text_area.left + text_area.clip_min[0],
                    text_area.top + text_area.clip_min[1],
                ],
                clip_max: [
                    text_area.left + text_area.clip_max[0],
                    text_area.top + text_area.clip_max[1],
                ],
                _pad: [0.0, 0.0],
            })
            .collect();
        let fragment_capacity = fragment_uniforms.len().max(1);
        let (screen_buffer, fragment_buffer, bind_group, capacity) =
            if let Some((screen_buffer, fragment_buffer, bind_group, capacity)) = existing {
                if capacity >= fragment_capacity {
                    (
                        screen_buffer.clone(),
                        fragment_buffer.clone(),
                        bind_group.clone(),
                        capacity,
                    )
                } else {
                    let screen_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("Text Screen Uniform Buffer"),
                        size: std::mem::size_of::<TextScreenUniform>() as u64,
                        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    });
                    let fragment_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("Text Fragment Uniform Buffer"),
                        size: (fragment_capacity * std::mem::size_of::<TextFragmentUniform>())
                            as u64,
                        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    });
                    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("Text Globals Bind Group"),
                        layout: &self.screen_bind_group_layout,
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
                    (
                        screen_buffer,
                        fragment_buffer,
                        bind_group,
                        fragment_capacity,
                    )
                }
            } else {
                let screen_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Text Screen Uniform Buffer"),
                    size: std::mem::size_of::<TextScreenUniform>() as u64,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                let fragment_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Text Fragment Uniform Buffer"),
                    size: (fragment_capacity * std::mem::size_of::<TextFragmentUniform>()) as u64,
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Text Globals Bind Group"),
                    layout: &self.screen_bind_group_layout,
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
                (
                    screen_buffer,
                    fragment_buffer,
                    bind_group,
                    fragment_capacity,
                )
            };
        queue.write_buffer(&screen_buffer, 0, bytemuck::bytes_of(&screen_uniform));
        if fragment_uniforms.is_empty() {
            let default_fragment = TextFragmentUniform {
                origin: [0.0, 0.0],
                clip_min: [0.0, 0.0],
                clip_max: [0.0, 0.0],
                _pad: [0.0, 0.0],
            };
            queue.write_buffer(&fragment_buffer, 0, bytemuck::bytes_of(&default_fragment));
        } else {
            queue.write_buffer(
                &fragment_buffer,
                0,
                bytemuck::cast_slice(fragment_uniforms.as_slice()),
            );
        }
        (
            screen_buffer,
            fragment_buffer,
            bind_group,
            capacity.max(fragment_capacity),
        )
    }

    fn get_or_create_globals_bind_group(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        screen_w: f32,
        screen_h: f32,
        text_areas: &[TextArea<'_>],
        prepare_signature: u64,
    ) -> (wgpu::Buffer, wgpu::Buffer, wgpu::BindGroup, usize) {
        if let Some(cached) = self.globals_bind_groups.get(&prepare_signature) {
            let result = (
                cached.screen_buffer.clone(),
                cached.fragment_buffer.clone(),
                cached.bind_group.clone(),
                cached.capacity,
            );
            self.globals_bind_group_lru
                .retain(|k| *k != prepare_signature);
            self.globals_bind_group_lru.push_back(prepare_signature);
            return result;
        }
        let (screen_buffer, fragment_buffer, bind_group, capacity) =
            self.create_globals_bind_group(device, queue, screen_w, screen_h, text_areas, None);
        self.globals_bind_groups.insert(
            prepare_signature,
            CachedGlobalsBindGroup {
                screen_buffer: screen_buffer.clone(),
                fragment_buffer: fragment_buffer.clone(),
                bind_group: bind_group.clone(),
                capacity,
            },
        );
        self.globals_bind_group_lru
            .retain(|k| *k != prepare_signature);
        self.globals_bind_group_lru.push_back(prepare_signature);
        while self.globals_bind_group_lru.len() > 512 {
            if let Some(old_key) = self.globals_bind_group_lru.pop_front() {
                self.globals_bind_groups.remove(&old_key);
            }
        }
        (screen_buffer, fragment_buffer, bind_group, capacity)
    }

    fn prepare_text_area(
        &mut self,
        font_system: &mut FontSystem,
        _device: &wgpu::Device,
        _queue: &wgpu::Queue,
        text_area: &TextArea<'_>,
        fragment_index: u32,
        mask_out: &mut Vec<TextGlyphVertex>,
        color_out: &mut Vec<TextGlyphVertex>,
    ) -> Result<(), TextAtlasCacheError> {
        let is_run_visible = |run: &cosmic_text::LayoutRun<'_>| {
            let start_y = run.line_top * text_area.scale;
            let end_y = start_y + (run.line_height * text_area.scale);
            start_y <= text_area.clip_max[1] && text_area.clip_min[1] <= end_y
        };

        for run in text_area
            .buffer
            .layout_runs()
            .skip_while(|run| !is_run_visible(run))
            .take_while(is_run_visible)
        {
            for glyph in run.glyphs.iter() {
                let raster_scale = text_raster_scale(glyph.font_size * text_area.scale);
                let physical_glyph = glyph.physical((0.0, 0.0), text_area.scale * raster_scale);
                let glyph_x_offset = glyph.font_size * glyph.x_offset;
                let glyph_y_offset = glyph.font_size * glyph.y_offset;
                let color = glyph.color_opt.unwrap_or(text_area.default_color);
                let Some(image) = self
                    .swash_cache
                    .get_image(font_system, physical_glyph.cache_key)
                    .clone()
                else {
                    continue;
                };
                let (atlas_glyph, atlas_size, is_color_glyph) = match image.content {
                    SwashContent::Color => {
                        let atlas_glyph = if let Some(atlas_glyph) =
                            self.color_atlas.get_glyph(physical_glyph.cache_key)
                        {
                            atlas_glyph
                        } else {
                            let Some(atlas_glyph) = self.color_atlas.cache_glyph(
                                physical_glyph.cache_key,
                                swash_color_image_to_rgba(&image),
                                image.placement.width,
                                image.placement.height,
                                image.placement.left,
                                image.placement.top,
                                raster_scale,
                            )?
                            else {
                                continue;
                            };
                            atlas_glyph
                        };
                        (atlas_glyph, self.color_atlas.size as f32, true)
                    }
                    SwashContent::Mask | SwashContent::SubpixelMask => {
                        let atlas_glyph = if let Some(atlas_glyph) =
                            self.mask_atlas.get_glyph(physical_glyph.cache_key)
                        {
                            atlas_glyph
                        } else {
                            let Some(atlas_glyph) = self.mask_atlas.cache_glyph(
                                physical_glyph.cache_key,
                                swash_mask_image_to_r8(&image),
                                image.placement.width,
                                image.placement.height,
                                image.placement.left,
                                image.placement.top,
                                raster_scale,
                            )?
                            else {
                                continue;
                            };
                            atlas_glyph
                        };
                        (atlas_glyph, self.mask_atlas.size as f32, false)
                    }
                };

                let mut x = (glyph.x + glyph_x_offset) * text_area.scale + atlas_glyph.layout_left;
                let mut y = (run.line_y + glyph.y - glyph_y_offset) * text_area.scale
                    - atlas_glyph.layout_top;
                let mut width = atlas_glyph.layout_width;
                let mut height = atlas_glyph.layout_height;
                let mut atlas_x = atlas_glyph.x as f32;
                let mut atlas_y = atlas_glyph.y as f32;
                let max_x = x + width;
                let max_y = y + height;
                let bounds_left = text_area.clip_min[0];
                let bounds_top = text_area.clip_min[1];
                let bounds_right = text_area.clip_max[0];
                let bounds_bottom = text_area.clip_max[1];

                if x > bounds_right
                    || max_x < bounds_left
                    || y > bounds_bottom
                    || max_y < bounds_top
                {
                    continue;
                }
                if x < bounds_left {
                    let shift = bounds_left - x;
                    x = bounds_left;
                    width = max_x - bounds_left;
                    atlas_x += shift * atlas_glyph.raster_scale;
                }
                if x + width > bounds_right {
                    width = bounds_right - x;
                }
                if y < bounds_top {
                    let shift = bounds_top - y;
                    y = bounds_top;
                    height = max_y - bounds_top;
                    atlas_y += shift * atlas_glyph.raster_scale;
                }
                if y + height > bounds_bottom {
                    height = bounds_bottom - y;
                }
                width = width.max(0.0);
                height = height.max(0.0);
                if width <= 0.0 || height <= 0.0 {
                    continue;
                }
                if width <= 0.0 || height <= 0.0 {
                    continue;
                }

                let rgba = color.as_rgba();
                let vertex = TextGlyphVertex {
                    local_pos: [x, y],
                    size: [width, height],
                    uv_min: [atlas_x / atlas_size, atlas_y / atlas_size],
                    uv_max: [
                        (atlas_x + width * atlas_glyph.raster_scale) / atlas_size,
                        (atlas_y + height * atlas_glyph.raster_scale) / atlas_size,
                    ],
                    color: [
                        rgba[0] as f32 / 255.0,
                        rgba[1] as f32 / 255.0,
                        rgba[2] as f32 / 255.0,
                        rgba[3] as f32 / 255.0,
                    ],
                    opacity: 1.0,
                    fragment_index,
                };
                if is_color_glyph {
                    color_out.push(vertex);
                } else {
                    mask_out.push(vertex);
                }
            }
        }
        Ok(())
    }

    fn prepare_buffer_cached(
        &mut self,
        signature: u64,
        content: &str,
        width: f32,
        height: f32,
        font_size: f32,
        line_height: f32,
        font_weight: u16,
        font_families: &[String],
        align: Align,
        allow_wrap: bool,
    ) -> Arc<Buffer> {
        if let Some(buffer) = self.layout_buffers.get(&signature) {
            self.layout_buffer_lru
                .retain(|current_signature| *current_signature != signature);
            self.layout_buffer_lru.push_back(signature);
            return Arc::clone(buffer);
        }
        let buffer = with_shared_font_system(|font_system| {
            build_text_buffer(
                font_system,
                content,
                Some(width),
                Some(height),
                allow_wrap,
                font_size,
                line_height,
                font_weight,
                align,
                font_families,
            )
        });
        let buffer = Arc::new(buffer);
        self.layout_buffers.insert(signature, Arc::clone(&buffer));
        self.layout_buffer_lru
            .retain(|current_signature| *current_signature != signature);
        self.layout_buffer_lru.push_back(signature);
        while self.layout_buffer_lru.len() > 4096 {
            if let Some(old_signature) = self.layout_buffer_lru.pop_front() {
                self.layout_buffers.remove(&old_signature);
            }
        }
        buffer
    }
}

struct TextGlobalCache {
    resources: Option<TextResources>,
}

thread_local! {
    static TEXT_GLOBAL_CACHE: RefCell<TextGlobalCache> =
        const { RefCell::new(TextGlobalCache { resources: None }) };
}

fn build_text_debug_overlay(
    buffer: &Buffer,
    left: f32,
    top: f32,
    bounds: TextBounds,
    global_origin: [f32; 2],
    screen_w: f32,
    screen_h: f32,
) -> TextDebugOverlay {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let clip_rect = [
        bounds.left as f32,
        bounds.top as f32,
        bounds.right as f32,
        bounds.bottom as f32,
    ];
    for run in buffer.layout_runs() {
        let run_top = top + run.line_top;
        let run_bottom = run_top + run.line_height;
        for glyph in run.glyphs.iter() {
            let glyph_left = left + glyph.x;
            let glyph_right = glyph_left + glyph.w.max(0.0);
            let rect = intersect_rect([glyph_left, run_top, glyph_right, run_bottom], clip_rect);
            if rect[2] <= rect[0] || rect[3] <= rect[1] {
                continue;
            }
            let corners = [
                [rect[0] + global_origin[0], rect[1] + global_origin[1]],
                [rect[2] + global_origin[0], rect[1] + global_origin[1]],
                [rect[2] + global_origin[0], rect[3] + global_origin[1]],
                [rect[0] + global_origin[0], rect[3] + global_origin[1]],
            ];
            for (u, v) in [(0_usize, 1_usize), (1, 2), (2, 3), (3, 0)] {
                append_text_debug_line_quad(
                    &mut vertices,
                    &mut indices,
                    corners[u],
                    corners[v],
                    1.0,
                    [0.2, 1.0, 0.95, 0.9],
                    screen_w,
                    screen_h,
                );
            }
            for corner in corners {
                append_text_debug_point_quad(
                    &mut vertices,
                    &mut indices,
                    corner,
                    2.5,
                    [1.0, 0.35, 0.2, 0.95],
                    screen_w,
                    screen_h,
                );
            }
        }
    }
    TextDebugOverlay { vertices, indices }
}

fn build_text_debug_overlay_multi(
    fragments: &[TextPassFragment],
    resolved_buffers: &[Option<Arc<Buffer>>],
    resolved_bounds: &[Option<TextBounds>],
    scale: f32,
    target_origin: (u32, u32),
    logical_origin: (u32, u32),
    screen_w: f32,
    screen_h: f32,
) -> TextDebugOverlay {
    let mut combined = TextDebugOverlay {
        vertices: Vec::new(),
        indices: Vec::new(),
    };
    for (index, fragment) in fragments.iter().enumerate() {
        let Some(bounds) = resolved_bounds[index] else {
            continue;
        };
        let Some(buffer) = fragment
            .layout_buffer
            .as_deref()
            .or_else(|| resolved_buffers[index].as_deref())
        else {
            continue;
        };
        let overlay = build_text_debug_overlay(
            buffer,
            fragment.x * scale - target_origin.0 as f32 + logical_origin.0 as f32,
            fragment.y * scale - target_origin.1 as f32 + logical_origin.1 as f32,
            bounds,
            [
                target_origin.0 as f32 - logical_origin.0 as f32,
                target_origin.1 as f32 - logical_origin.1 as f32,
            ],
            screen_w,
            screen_h,
        );
        let base = combined.vertices.len() as u32;
        combined.vertices.extend(overlay.vertices);
        combined
            .indices
            .extend(overlay.indices.into_iter().map(|index| index + base));
    }
    combined
}

fn append_text_debug_line_quad(
    vertices: &mut Vec<TextDebugVertex>,
    indices: &mut Vec<u32>,
    p0: [f32; 2],
    p1: [f32; 2],
    thickness_px: f32,
    color: [f32; 4],
    screen_w: f32,
    screen_h: f32,
) {
    let dx = p1[0] - p0[0];
    let dy = p1[1] - p0[1];
    let len = (dx * dx + dy * dy).sqrt();
    if len <= 1e-5 {
        return;
    }
    let nx = -dy / len;
    let ny = dx / len;
    let half = thickness_px * 0.5;
    let offset = [nx * half, ny * half];
    append_text_debug_quad(
        vertices,
        indices,
        [
            [p0[0] + offset[0], p0[1] + offset[1]],
            [p0[0] - offset[0], p0[1] - offset[1]],
            [p1[0] - offset[0], p1[1] - offset[1]],
            [p1[0] + offset[0], p1[1] + offset[1]],
        ],
        color,
        screen_w,
        screen_h,
    );
}

fn append_text_debug_point_quad(
    vertices: &mut Vec<TextDebugVertex>,
    indices: &mut Vec<u32>,
    center: [f32; 2],
    size_px: f32,
    color: [f32; 4],
    screen_w: f32,
    screen_h: f32,
) {
    let half = size_px * 0.5;
    append_text_debug_quad(
        vertices,
        indices,
        [
            [center[0] - half, center[1] - half],
            [center[0] + half, center[1] - half],
            [center[0] + half, center[1] + half],
            [center[0] - half, center[1] + half],
        ],
        color,
        screen_w,
        screen_h,
    );
}

fn append_text_debug_quad(
    vertices: &mut Vec<TextDebugVertex>,
    indices: &mut Vec<u32>,
    quad: [[f32; 2]; 4],
    color: [f32; 4],
    screen_w: f32,
    screen_h: f32,
) {
    let base = vertices.len() as u32;
    for point in quad {
        vertices.push(TextDebugVertex {
            position: text_pixel_to_ndc(point[0], point[1], screen_w, screen_h),
            color,
        });
    }
    indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

fn text_pixel_to_ndc(x: f32, y: f32, screen_w: f32, screen_h: f32) -> [f32; 2] {
    let nx = x / screen_w.max(1.0);
    let ny = y / screen_h.max(1.0);
    [nx * 2.0 - 1.0, 1.0 - ny * 2.0]
}

fn with_text_resources<R>(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    format: wgpu::TextureFormat,
    f: impl FnOnce(&mut TextResources) -> R,
) -> R {
    TEXT_GLOBAL_CACHE.with(|cache| {
        let mut guard = cache.borrow_mut();
        let rebuild = guard
            .resources
            .as_ref()
            .map(|r| r.format != format)
            .unwrap_or(true);

        if rebuild {
            guard.resources = Some(TextResources::new(device, queue, format));
        }

        let resources = guard.resources.as_mut().expect("text resources must exist");
        f(resources)
    })
}

fn with_text_resources_for_render(format: wgpu::TextureFormat, f: impl FnOnce(&mut TextResources)) {
    TEXT_GLOBAL_CACHE.with(|cache| {
        let mut guard = cache.borrow_mut();
        let Some(resources) = guard.resources.as_mut() else {
            return;
        };
        if resources.format != format {
            return;
        }
        f(resources);
    });
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
            read_mask: 0xFF,
            write_mask: 0xFF,
        },
        bias: wgpu::DepthBiasState::default(),
    }
}

const TEXT_GLYPH_VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 7] = wgpu::vertex_attr_array![
    0 => Float32x2,
    1 => Float32x2,
    2 => Float32x2,
    3 => Float32x2,
    4 => Float32x4,
    5 => Float32,
    6 => Uint32
];

fn create_text_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    sample_count: u32,
    stencil_enabled: bool,
    kind: TextPipelineKind,
    screen_bind_group_layout: &wgpu::BindGroupLayout,
    atlas_bind_group_layout: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Text Shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("../../shader/text.wgsl").into()),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Text Pipeline Layout"),
        bind_group_layouts: &[
            Some(screen_bind_group_layout),
            Some(atlas_bind_group_layout),
        ],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Text Pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<TextGlyphVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &TEXT_GLYPH_VERTEX_ATTRIBUTES,
            }],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some(match kind {
                TextPipelineKind::Mask => "fs_mask",
                TextPipelineKind::Color => "fs_color",
            }),
            targets: &[Some(wgpu::ColorTargetState {
                format,
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
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            ..Default::default()
        },
        depth_stencil: stencil_enabled.then(text_depth_stencil_state),
        multisample: wgpu::MultisampleState {
            count: sample_count,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview_mask: None,
        cache: None,
    })
}

pub fn prewarm_text_pipeline(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    format: wgpu::TextureFormat,
    sample_count: u32,
) {
    // Prewarm kicks a full render, which stalls startup on wasm and gives no
    // measurable benefit there. Native keeps the prewarm to warm the pipeline
    // cache before the first user frame.
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (device, queue, format, sample_count);
        return;
    }
    #[cfg(not(target_arch = "wasm32"))]
    with_text_resources(device, queue, format, |resources| {
        let regular_key = TextRendererKey {
            sample_count,
            stencil_enabled: false,
        };
        let stencil_key = TextRendererKey {
            sample_count,
            stencil_enabled: true,
        };
        for kind in [TextPipelineKind::Mask, TextPipelineKind::Color] {
            resources.ensure_pipeline(device, regular_key, kind);
            resources.ensure_pipeline(device, stencil_key, kind);
        }
    });
}

pub fn clear_text_resources_cache() {
    TEXT_GLOBAL_CACHE.with(|cache| {
        cache.borrow_mut().resources = None;
    });
}

#[cfg(test)]
mod tests {
    use super::{
        TextBounds, TextDebugOverlay, TextGlyphVertex, build_text_debug_overlay,
        physical_scissor_rect, text_pixel_to_ndc,
    };
    use cosmic_text::{Attrs, Buffer, FontSystem, Metrics, Shaping, Weight, Wrap};

    fn build_buffer(content: &str, width: f32, font_size: f32, line_height: f32) -> Buffer {
        let mut font_system = FontSystem::new();
        let mut buffer = Buffer::new(
            &mut font_system,
            Metrics::new(font_size, (font_size * line_height).max(1.0)),
        );
        buffer.set_wrap(&mut font_system, Wrap::WordOrGlyph);
        buffer.set_size(&mut font_system, Some(width.max(1.0)), None);
        buffer.set_text(
            &mut font_system,
            content,
            &Attrs::new().weight(Weight(400)),
            Shaping::Advanced,
            None,
        );
        buffer.shape_until_scroll(&mut font_system, false);
        buffer
    }

    fn overlay_rects(overlay: &TextDebugOverlay, screen_w: f32, screen_h: f32) -> Vec<[f32; 4]> {
        overlay
            .vertices
            .chunks_exact(4)
            .map(|quad| {
                let xs: Vec<f32> = quad
                    .iter()
                    .map(|v| ((v.position[0] + 1.0) * 0.5) * screen_w)
                    .collect();
                let ys: Vec<f32> = quad
                    .iter()
                    .map(|v| ((1.0 - v.position[1]) * 0.5) * screen_h)
                    .collect();
                [
                    xs.iter().copied().fold(f32::INFINITY, f32::min),
                    ys.iter().copied().fold(f32::INFINITY, f32::min),
                    xs.iter().copied().fold(f32::NEG_INFINITY, f32::max),
                    ys.iter().copied().fold(f32::NEG_INFINITY, f32::max),
                ]
            })
            .collect()
    }

    #[test]
    fn text_debug_overlay_clips_glyph_rects_to_bounds() {
        let screen_w = 400.0;
        let screen_h = 300.0;
        let buffer = build_buffer("clip me please clip me please", 80.0, 16.0, 1.25);
        let bounds = TextBounds {
            left: 0,
            top: 0,
            right: 80,
            bottom: 22,
        };

        let overlay =
            build_text_debug_overlay(&buffer, 0.0, 0.0, bounds, [0.0, 0.0], screen_w, screen_h);
        let rects = overlay_rects(&overlay, screen_w, screen_h);

        assert!(!rects.is_empty());
        assert!(
            rects.iter().all(|rect| {
                rect[0] >= -2.0 && rect[1] >= -2.0 && rect[2] <= 82.0 && rect[3] <= 24.0
            }),
            "all overlay quads should stay near the resolved text bounds"
        );
    }

    #[test]
    fn physical_scissor_rect_scales_and_clamps_to_target() {
        assert_eq!(
            physical_scissor_rect(Some([10, 5, 40, 20]), 2.0, (90, 40)),
            Some([20, 10, 70, 30])
        );
        assert_eq!(
            physical_scissor_rect(Some([200, 0, 10, 10]), 1.0, (50, 50)),
            None
        );
    }

    #[test]
    fn text_pixel_to_ndc_maps_screen_corners() {
        assert_eq!(text_pixel_to_ndc(0.0, 0.0, 200.0, 100.0), [-1.0, 1.0]);
        assert_eq!(text_pixel_to_ndc(200.0, 100.0, 200.0, 100.0), [1.0, -1.0]);
    }

    #[test]
    fn glyph_vertices_can_preserve_fractional_positions() {
        let vertex = TextGlyphVertex {
            local_pos: [10.25, 20.75],
            size: [8.5, 12.25],
            uv_min: [0.0, 0.0],
            uv_max: [1.0, 1.0],
            color: [1.0, 1.0, 1.0, 1.0],
            opacity: 1.0,
            fragment_index: 0,
        };

        assert!((vertex.local_pos[0] - 10.25).abs() < 0.001);
        assert!((vertex.local_pos[1] - 20.75).abs() < 0.001);
        assert!((vertex.size[0] - 8.5).abs() < 0.001);
        assert!((vertex.size[1] - 12.25).abs() < 0.001);
    }
}
