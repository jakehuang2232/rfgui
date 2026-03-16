use crate::view::frame_graph::slot::OutSlot;
use crate::view::frame_graph::{BufferDesc, BufferResource};
use crate::view::frame_graph::{
    BufferReadUsage, GraphicsColorAttachmentDescriptor, GraphicsPassBuilder,
    GraphicsPassMergePolicy, PrepareContext,
};
use crate::view::render_pass::draw_rect_pass::RenderTargetOut;
use crate::view::render_pass::render_target::{
    GraphicsPassContext as RenderPassContext, render_target_size,
};
use crate::view::render_pass::{GraphicsCtx, GraphicsPass};
use glyphon::cosmic_text::{Align, Weight};
use glyphon::{
    Attrs, Buffer, Cache, Color as GlyphonColor, Family, FontSystem, Metrics, Resolution, Shaping,
    SwashCache, TextArea, TextAtlas, TextBounds, TextRenderer, Viewport as GlyphonViewport,
};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
pub struct TextPass {
    params: TextPassParams,
    staging_buffer: TextStagingBufferOut,
    prepared: Option<TextPreparedState>,
    input: TextInput,
    output: TextOutput,
}

pub struct TextPassParams {
    pub content: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub color: [f32; 4],
    pub opacity: f32,
    pub font_size: f32,
    pub line_height: f32,
    pub font_weight: u16,
    pub font_families: Vec<String>,
    pub align: Align,
    pub allow_wrap: bool,
    pub scissor_rect: Option<[u32; 4]>,
    pub stencil_clip_id: Option<u8>,
}

struct TextPreparedState {
    renderer_key: TextRendererKey,
    renderer: Option<TextRenderer>,
    prepare_signature: u64,
    stencil_clip_id: Option<u8>,
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

#[derive(Clone, Copy)]
pub struct TextStagingBufferTag;
pub type TextStagingBufferOut = OutSlot<BufferResource, TextStagingBufferTag>;

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
            staging_buffer: TextStagingBufferOut::default(),
            prepared: None,
            input,
            output,
        }
    }

    fn prepare_signature(&self, bounds: TextBounds, scale: f32, screen_size: (u32, u32)) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.params.content.hash(&mut hasher);
        self.params.x.to_bits().hash(&mut hasher);
        self.params.y.to_bits().hash(&mut hasher);
        self.params.width.to_bits().hash(&mut hasher);
        self.params.height.to_bits().hash(&mut hasher);
        self.params.font_size.to_bits().hash(&mut hasher);
        self.params.line_height.to_bits().hash(&mut hasher);
        self.params.opacity.to_bits().hash(&mut hasher);
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
        for channel in self.params.color {
            channel.to_bits().hash(&mut hasher);
        }
        hasher.finish()
    }
}

impl GraphicsPass for TextPass {
    fn setup(&mut self, builder: &mut GraphicsPassBuilder<'_, '_>) {
        builder.set_graphics_merge_policy(GraphicsPassMergePolicy::Mergeable);
        self.staging_buffer = builder.create_buffer(BufferDesc {
            size: (self.params.content.len().max(1) as u64).next_power_of_two(),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::UNIFORM,
            label: Some("TextPass Staging Buffer"),
        });
        builder.read_buffer(&self.staging_buffer, BufferReadUsage::Uniform);
        if let Some(target) = builder.texture_target(&self.output.render_target) {
            builder.write_color(
                &self.output.render_target,
                GraphicsColorAttachmentDescriptor::load(target),
            );
        } else {
            builder.write_surface_color(GraphicsColorAttachmentDescriptor::load(
                builder.surface_target(),
            ));
        }
        self.params.scissor_rect = intersect_scissor_rects(
            self.input.pass_context.scissor_rect,
            self.params.scissor_rect,
        );
        if self.params.stencil_clip_id.is_none() {
            self.params.stencil_clip_id = self.input.pass_context.stencil_clip_id;
        }
        if let Some(target) = self.input.pass_context.depth_stencil_target {
            builder.read_depth(target);
            builder.read_stencil(target);
        }
    }

    fn prepare(&mut self, ctx: &mut PrepareContext<'_, '_>) {
        let Some(handle) = self.staging_buffer.handle() else {
            return;
        };
        let mut packed = self.params.content.as_bytes().to_vec();
        packed.push(0);
        let _ = ctx.upload_buffer(handle, 0, &packed);

        if self.params.content.is_empty()
            || self.params.width <= 0.0
            || self.params.height <= 0.0
            || !self.params.x.is_finite()
            || !self.params.y.is_finite()
            || !self.params.width.is_finite()
            || !self.params.height.is_finite()
        {
            return;
        }

        let viewport = &mut ctx.viewport;
        let device = match viewport.device().cloned() {
            Some(device) => device,
            None => return,
        };
        let queue = match viewport.queue().cloned() {
            Some(queue) => queue,
            None => return,
        };
        let (screen_w, screen_h) = viewport.surface_size();
        let scale = viewport.scale_factor();
        let format = viewport.surface_format();
        let mut global = text_resources(&device, &queue, format);
        let resources = global.resources.as_mut().unwrap();
        resources.viewport.update(
            &queue,
            Resolution {
                width: screen_w,
                height: screen_h,
            },
        );

        let bounds = match resolve_text_bounds(
            self.params.x * scale,
            self.params.y * scale,
            self.params.width * scale,
            self.params.height * scale,
            screen_w,
            screen_h,
            physical_scissor_rect(self.params.scissor_rect, scale, (screen_w, screen_h)),
        ) {
            Some(bounds) => bounds,
            None => return,
        };
        let prepare_signature = self.prepare_signature(bounds, scale, (screen_w, screen_h));
        let buffer = resources.prepare_buffer(
            self.params.content.as_str(),
            self.params.width * scale,
            self.params.height * scale,
            self.params.font_size * scale,
            self.params.line_height,
            self.params.font_weight,
            self.params.font_families.as_slice(),
            self.params.align,
            self.params.allow_wrap,
        );
        let text_area = build_text_area(
            &buffer,
            self.params.x * scale,
            self.params.y * scale,
            bounds,
            to_glyphon_color(self.params.color, self.params.opacity),
        );
        let renderer_key = TextRendererKey {
            sample_count: viewport.msaa_sample_count(),
            stencil_enabled: self.input.pass_context.depth_stencil_target.is_some(),
        };
        if let Some(prepared) = self.prepared.as_mut() {
            if prepared.renderer_key == renderer_key
                && prepared.prepare_signature == prepare_signature
            {
                prepared.stencil_clip_id = self.params.stencil_clip_id;
                return;
            }
        }
        if viewport.debug_geometry_overlay() {
            let overlay = build_text_debug_overlay(
                &buffer,
                self.params.x * scale,
                self.params.y * scale,
                bounds,
                screen_w as f32,
                screen_h as f32,
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
        if renderer
            .prepare(
                &device,
                &queue,
                &mut resources.font_system,
                &mut resources.atlas,
                &resources.viewport,
                vec![text_area],
                &mut resources.swash_cache,
            )
            .is_err()
        {
            resources.put_renderer(renderer_key, renderer);
            return;
        }
        self.prepared = Some(TextPreparedState {
            renderer_key,
            renderer: Some(renderer),
            prepare_signature,
            stencil_clip_id: self.params.stencil_clip_id,
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
        let mut global = text_resources(&device, &queue, format);
        let resources = global.resources.as_mut().unwrap();
        let target_size = match self.output.render_target.handle() {
            Some(handle) => render_target_size(ctx.frame_resources(), handle)
                .unwrap_or(ctx.viewport().surface_size()),
            None => ctx.viewport().surface_size(),
        };
        let scissor_rect = physical_scissor_rect(
            self.params.scissor_rect,
            ctx.viewport().scale_factor(),
            target_size,
        );
        let stencil_reference = prepared.stencil_clip_id.map(|id| id as u32).unwrap_or(0);
        let Some(renderer) = prepared.renderer.as_mut() else {
            return;
        };
        if let Some([x, y, width, height]) = scissor_rect {
            ctx.set_scissor_rect(x, y, width, height);
        } else {
            ctx.set_scissor_rect(0, 0, target_size.0, target_size.1);
        }
        ctx.set_stencil_reference(stencil_reference);
        let _ = renderer.render(&resources.atlas, &resources.viewport, ctx.raw_render_pass());
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
    font_system: FontSystem,
    swash_cache: SwashCache,
    atlas: TextAtlas,
    viewport: GlyphonViewport,
    format: wgpu::TextureFormat,
    renderers: HashMap<TextRendererKey, TextRenderer>,
}

impl TextResources {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let font_system = FontSystem::new();
        let swash_cache = SwashCache::new();
        let cache = Cache::new(device);
        let atlas = TextAtlas::new(device, queue, &cache, format);
        let viewport = GlyphonViewport::new(device, &cache);

        Self {
            font_system,
            swash_cache,
            atlas,
            viewport,
            format,
            renderers: HashMap::new(),
        }
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

    fn prepare_buffer(
        &mut self,
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
        let mut buffer = Buffer::new(
            &mut self.font_system,
            Metrics::new(
                font_size.max(1.0),
                (font_size * line_height.max(0.8)).max(1.0),
            ),
        );
        buffer.set_wrap(
            &mut self.font_system,
            if allow_wrap {
                glyphon::Wrap::WordOrGlyph
            } else {
                glyphon::Wrap::None
            },
        );
        buffer.set_size(
            &mut self.font_system,
            Some(width.max(1.0)),
            Some(height.max(1.0)),
        );

        let attrs = if let Some(first) = font_families.first() {
            Attrs::new()
                .family(Family::Name(first.as_str()))
                .weight(Weight(font_weight))
        } else {
            Attrs::new().weight(Weight(font_weight))
        };

        buffer.set_text(
            &mut self.font_system,
            content,
            &attrs,
            Shaping::Advanced,
            Some(align),
        );
        buffer.shape_until_scroll(&mut self.font_system, false);
        buffer
    }
}

struct TextGlobalCache {
    resources: Option<TextResources>,
}

fn text_global_cache() -> &'static Mutex<TextGlobalCache> {
    static CACHE: OnceLock<Mutex<TextGlobalCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(TextGlobalCache { resources: None }))
}

fn build_text_debug_overlay(
    buffer: &Buffer,
    left: f32,
    top: f32,
    bounds: TextBounds,
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
                [rect[0], rect[1]],
                [rect[2], rect[1]],
                [rect[2], rect[3]],
                [rect[0], rect[3]],
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

fn text_resources<'a>(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    format: wgpu::TextureFormat,
) -> std::sync::MutexGuard<'a, TextGlobalCache> {
    let cache = text_global_cache();
    let mut guard = cache.lock().unwrap();
    let rebuild = guard
        .resources
        .as_ref()
        .map(|r| r.format != format)
        .unwrap_or(true);

    if rebuild {
        guard.resources = Some(TextResources::new(device, queue, format));
    }

    guard
}

fn text_depth_stencil_state() -> wgpu::DepthStencilState {
    wgpu::DepthStencilState {
        format: wgpu::TextureFormat::Depth24PlusStencil8,
        depth_write_enabled: false,
        depth_compare: wgpu::CompareFunction::Always,
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
    let mut global = text_resources(device, queue, format);
    let resources = global
        .resources
        .as_mut()
        .expect("text resources must exist");
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
}

pub fn clear_text_resources_cache() {
    let cache = text_global_cache();
    let mut guard = cache.lock().unwrap();
    guard.resources = None;
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

        let overlay = build_text_debug_overlay(&buffer, 0.0, 0.0, bounds, screen_w, screen_h);
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
