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
use glyphon::cosmic_text::Align;
use glyphon::{
    Buffer, Cache, Color as GlyphonColor, Resolution, SwashCache, TextArea, TextAtlas, TextBounds,
    TextRenderer, Viewport as GlyphonViewport,
};
use std::cell::RefCell;
use std::collections::HashMap;
pub struct TextPass {
    params: TextPassParams,
    prepared: Option<TextPreparedState>,
    input: TextInput,
    output: TextOutput,
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
    pub layout_buffer: Option<Buffer>,
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
    renderer: Option<TextRenderer>,
    prepare_signature: u64,
    stencil_clip_id: Option<u8>,
    resolution: Resolution,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct TextRendererKey {
    sample_count: u32,
    stencil_enabled: bool,
}

struct TextDebugOverlay {
    vertices: Vec<TextDebugVertex>,
    indices: Vec<u32>,
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

    fn prepare_signature(&self, bounds: TextBounds, scale: f32, screen_size: (u32, u32)) -> u64 {
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
        bounds.left.hash(&mut hasher);
        bounds.top.hash(&mut hasher);
        bounds.right.hash(&mut hasher);
        bounds.bottom.hash(&mut hasher);
        for fragment in &self.params.fragments {
            fragment.content.hash(&mut hasher);
            fragment.x.to_bits().hash(&mut hasher);
            fragment.y.to_bits().hash(&mut hasher);
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
        let resolution = Resolution {
            width: screen_w,
            height: screen_h,
        };
        with_text_resources(&device, &queue, format, |resources| {
            let glyphon_viewport = resources.take_viewport(&device, &queue, resolution);
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
                        scale,
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
                        fragment.width * scale,
                        fragment.height * scale,
                        self.params.font_size * scale,
                        self.params.line_height,
                        self.params.font_weight,
                        self.params.font_families.as_slice(),
                        self.params.align,
                        self.params.allow_wrap,
                    ));
                }
            }
            let Some(bounds) = combined_bounds else {
                resources.put_viewport(resolution, glyphon_viewport);
                return;
            };
            let prepare_signature = self.prepare_signature(bounds, scale, (screen_w, screen_h));
            let mut text_areas = Vec::new();
            for (index, fragment) in self.params.fragments.iter().enumerate() {
                let Some(fragment_bounds) = resolved_bounds[index] else {
                    continue;
                };
                let buffer = fragment
                    .layout_buffer
                    .as_ref()
                    .or_else(|| resolved_buffers[index].as_ref())
                    .expect("buffer should be resolved for visible text fragment");
                text_areas.push(build_text_area(
                    buffer,
                    fragment.x * scale - target_origin.0 as f32
                        + target_meta.logical_origin.0 as f32,
                    fragment.y * scale - target_origin.1 as f32
                        + target_meta.logical_origin.1 as f32,
                    fragment_bounds,
                    to_glyphon_color(fragment.color, fragment.opacity),
                ));
            }
            let renderer_key = TextRendererKey {
                sample_count: output_sample_count,
                stencil_enabled: self.input.pass_context.uses_depth_stencil,
            };
            if let Some(renderer) =
                resources.take_prepared_renderer(renderer_key, prepare_signature)
            {
                self.prepared = Some(TextPreparedState {
                    renderer_key,
                    renderer: Some(renderer),
                    prepare_signature,
                    stencil_clip_id: self.params.stencil_clip_id,
                    resolution,
                });
                resources.put_viewport(resolution, glyphon_viewport);
                return;
            }
            if let Some(prepared) = self.prepared.as_mut() {
                if prepared.renderer_key == renderer_key
                    && prepared.prepare_signature == prepare_signature
                {
                    prepared.stencil_clip_id = self.params.stencil_clip_id;
                    resources.put_viewport(resolution, glyphon_viewport);
                    return;
                }
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
            let mut renderer = match self.prepared.take() {
                Some(mut previous) => {
                    if previous.renderer_key == renderer_key {
                        previous
                            .renderer
                            .take()
                            .unwrap_or_else(|| resources.take_renderer(&device, renderer_key))
                    } else {
                        if let Some(old_renderer) = previous.renderer.take() {
                            resources.put_renderer(previous.renderer_key, old_renderer);
                        }
                        resources.take_renderer(&device, renderer_key)
                    }
                }
                None => resources.take_renderer(&device, renderer_key),
            };
            let prepare_result = with_shared_font_system(|font_system| {
                renderer.prepare(
                    &device,
                    &queue,
                    font_system,
                    &mut resources.atlas,
                    &glyphon_viewport,
                    text_areas,
                    &mut resources.swash_cache,
                )
            });
            resources.put_viewport(resolution, glyphon_viewport);
            if prepare_result.is_err() {
                resources.put_renderer(renderer_key, renderer);
                return;
            }
            self.prepared = Some(TextPreparedState {
                renderer_key,
                renderer: Some(renderer),
                prepare_signature,
                stencil_clip_id: self.params.stencil_clip_id,
                resolution,
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
        let queue = match ctx.viewport().queue().cloned() {
            Some(queue) => queue,
            None => return,
        };
        let format = ctx.viewport().surface_format();
        with_text_resources(&device, &queue, format, |resources| {
            let Some(glyphon_viewport) = resources.viewport(prepared.resolution) else {
                return;
            };
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
            let Some(renderer) = prepared.renderer.take() else {
                return;
            };
            if let Some([x, y, width, height]) = scissor_rect {
                ctx.set_scissor_rect(x, y, width, height);
            } else {
                ctx.set_scissor_rect(0, 0, target_size.0, target_size.1);
            }
            ctx.set_stencil_reference(stencil_reference);
            let _ = renderer.render(&resources.atlas, glyphon_viewport, ctx.raw_render_pass());
            resources.put_prepared_renderer(
                prepared.renderer_key,
                prepared.prepare_signature,
                renderer,
            );
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
    scale: f32,
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
    (fragment.width * scale).to_bits().hash(&mut hasher);
    (fragment.height * scale).to_bits().hash(&mut hasher);
    (font_size * scale).to_bits().hash(&mut hasher);
    line_height.to_bits().hash(&mut hasher);
    font_weight.hash(&mut hasher);
    font_families.hash(&mut hasher);
    std::mem::discriminant(&align).hash(&mut hasher);
    allow_wrap.hash(&mut hasher);
    hasher.finish()
}

fn to_glyphon_color(color: [f32; 4], opacity: f32) -> GlyphonColor {
    fn to_u8(v: f32) -> u8 {
        (v.clamp(0.0, 1.0) * 255.0).round() as u8
    }

    let alpha = to_u8(color[3] * opacity.clamp(0.0, 1.0));
    GlyphonColor::rgba(to_u8(color[0]), to_u8(color[1]), to_u8(color[2]), alpha)
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
    bounds: TextBounds,
    default_color: GlyphonColor,
) -> TextArea<'a> {
    TextArea {
        buffer,
        left,
        top,
        scale: 1.0,
        bounds,
        default_color,
        custom_glyphs: &[],
    }
}

struct TextResources {
    cache: Cache,
    swash_cache: SwashCache,
    atlas: TextAtlas,
    format: wgpu::TextureFormat,
    renderers: HashMap<TextRendererKey, TextRenderer>,
    prepared_renderers: HashMap<(TextRendererKey, u64), TextRenderer>,
    layout_buffers: HashMap<u64, Buffer>,
    viewports: HashMap<(u32, u32), GlyphonViewport>,
}

impl TextResources {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let swash_cache = SwashCache::new();
        let cache = Cache::new(device);
        let atlas = TextAtlas::new(device, queue, &cache, format);

        Self {
            cache,
            swash_cache,
            atlas,
            format,
            renderers: HashMap::new(),
            prepared_renderers: HashMap::new(),
            layout_buffers: HashMap::new(),
            viewports: HashMap::new(),
        }
    }

    fn take_viewport(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        resolution: Resolution,
    ) -> GlyphonViewport {
        let key = (resolution.width, resolution.height);
        let mut viewport = self
            .viewports
            .remove(&key)
            .unwrap_or_else(|| GlyphonViewport::new(device, &self.cache));
        viewport.update(queue, resolution);
        viewport
    }

    fn put_viewport(&mut self, resolution: Resolution, viewport: GlyphonViewport) {
        self.viewports
            .insert((resolution.width, resolution.height), viewport);
    }

    fn viewport(&self, resolution: Resolution) -> Option<&GlyphonViewport> {
        self.viewports.get(&(resolution.width, resolution.height))
    }

    fn take_renderer(&mut self, device: &wgpu::Device, key: TextRendererKey) -> TextRenderer {
        if let Some(renderer) = self.renderers.remove(&key) {
            return renderer;
        }
        TextRenderer::new(
            &mut self.atlas,
            device,
            wgpu::MultisampleState {
                count: key.sample_count,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            key.stencil_enabled.then(text_depth_stencil_state),
        )
    }

    fn put_renderer(&mut self, key: TextRendererKey, renderer: TextRenderer) {
        self.renderers.insert(key, renderer);
    }

    fn take_prepared_renderer(
        &mut self,
        key: TextRendererKey,
        signature: u64,
    ) -> Option<TextRenderer> {
        self.prepared_renderers.remove(&(key, signature))
    }

    fn put_prepared_renderer(
        &mut self,
        key: TextRendererKey,
        signature: u64,
        renderer: TextRenderer,
    ) {
        self.prepared_renderers.insert((key, signature), renderer);
        if self.prepared_renderers.len() > 512 {
            self.prepared_renderers.clear();
        }
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
    ) -> Buffer {
        if let Some(buffer) = self.layout_buffers.get(&signature) {
            return buffer.clone();
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
        self.layout_buffers.insert(signature, buffer.clone());
        if self.layout_buffers.len() > 4096 {
            self.layout_buffers.clear();
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
    resolved_buffers: &[Option<Buffer>],
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
            .as_ref()
            .or_else(|| resolved_buffers[index].as_ref())
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

pub fn prewarm_text_pipeline(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    format: wgpu::TextureFormat,
    sample_count: u32,
) {
    with_text_resources(device, queue, format, |resources| {
        let regular_key = TextRendererKey {
            sample_count,
            stencil_enabled: false,
        };
        let stencil_key = TextRendererKey {
            sample_count,
            stencil_enabled: true,
        };
        let renderer_regular = resources.take_renderer(device, regular_key);
        resources.put_renderer(regular_key, renderer_regular);
        let renderer_stencil = resources.take_renderer(device, stencil_key);
        resources.put_renderer(stencil_key, renderer_stencil);
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
        TextBounds, TextDebugOverlay, build_text_debug_overlay, physical_scissor_rect,
        text_pixel_to_ndc,
    };
    use glyphon::cosmic_text::{Weight, Wrap};
    use glyphon::{Attrs, Buffer, FontSystem, Metrics, Shaping};

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
}
