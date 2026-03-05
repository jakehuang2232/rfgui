use crate::render_pass::render_target::RenderTargetPass;
use crate::view::frame_graph::PassContext;
use crate::view::frame_graph::builder::BuildContext;
use crate::view::frame_graph::slot::OutSlot;
use crate::view::frame_graph::{BufferDesc, BufferResource};
use crate::view::frame_graph::{DepIn, DepOut};
use crate::view::render_pass::draw_rect_pass::RenderTargetOut;
use crate::view::render_pass::render_target::{
    render_target_msaa_view, render_target_size, render_target_view,
};
use crate::view::render_pass::RenderPass;
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

#[derive(Clone, Copy)]
pub struct TextStagingBufferTag;
pub type TextStagingBufferOut = OutSlot<BufferResource, TextStagingBufferTag>;

#[derive(Default)]
pub struct TextInput {
    pub dep: DepIn,
}

#[derive(Default)]
pub struct TextOutput {
    pub render_target: RenderTargetOut,
    pub dep: DepOut,
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

impl RenderPass for TextPass {
    type Input = TextInput;
    type Output = TextOutput;

    fn input(&self) -> &Self::Input {
        &self.input
    }

    fn input_mut(&mut self) -> &mut Self::Input {
        &mut self.input
    }

    fn output(&self) -> &Self::Output {
        &self.output
    }

    fn output_mut(&mut self) -> &mut Self::Output {
        &mut self.output
    }

    fn build(&mut self, builder: &mut BuildContext) {
        self.staging_buffer = builder.create_buffer(BufferDesc {
            size: (self.params.content.len().max(1) as u64).next_power_of_two(),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::UNIFORM,
            label: Some("TextPass Staging Buffer"),
        });
        if let Some(handle) = self.input.dep.handle() {
            let source: DepOut = DepOut::with_handle(handle);
            builder.read_dep(&mut self.input.dep, &source);
        }
        if self.output.render_target.handle().is_some() {
            builder.write_texture(&mut self.output.render_target);
        }
        if self.output.dep.handle().is_some() {
            builder.write_dep(&mut self.output.dep);
        }
    }

    fn compile_upload(&mut self, ctx: &mut PassContext<'_, '_>) {
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
            self.params.scissor_rect.and_then(|scissor| {
                viewport.logical_scissor_to_physical(scissor, (screen_w, screen_h))
            }),
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
            stencil_enabled: self.params.stencil_clip_id.is_some(),
        };
        if let Some(prepared) = self.prepared.as_mut() {
            if prepared.renderer_key == renderer_key
                && prepared.prepare_signature == prepare_signature
            {
                prepared.stencil_clip_id = self.params.stencil_clip_id;
                return;
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

    fn execute(
        &mut self,
        ctx: &mut PassContext<'_, '_>,
        mut render_pass: Option<&mut wgpu::RenderPass<'_>>,
    ) {
        let Some(prepared) = self.prepared.as_mut() else {
            return;
        };
        let device = match ctx.viewport.device().cloned() {
            Some(device) => device,
            None => return,
        };
        let queue = match ctx.viewport.queue().cloned() {
            Some(queue) => queue,
            None => return,
        };
        let format = ctx.viewport.surface_format();
        let mut global = text_resources(&device, &queue, format);
        let resources = global.resources.as_mut().unwrap();

        if let Some(pass) = render_pass.as_mut() {
            let target_size = match self.output.render_target.handle() {
                Some(handle) => render_target_size(ctx, handle).unwrap_or(ctx.viewport.surface_size()),
                None => ctx.viewport.surface_size(),
            };
            let scissor_rect = self.params.scissor_rect.and_then(|scissor| {
                ctx.viewport
                    .logical_scissor_to_physical(scissor, target_size)
            });
            if let Some([x, y, width, height]) = scissor_rect {
                pass.set_scissor_rect(x, y, width, height);
            } else {
                pass.set_scissor_rect(0, 0, target_size.0, target_size.1);
            }
            if let Some(stencil_clip_id) = prepared.stencil_clip_id {
                pass.set_stencil_reference(stencil_clip_id as u32);
            } else {
                pass.set_stencil_reference(0);
            }
            let Some(renderer) = prepared.renderer.as_mut() else {
                return;
            };
            let _ = renderer.render(&resources.atlas, &resources.viewport, pass);
            return;
        }

        let (offscreen_view, offscreen_msaa_view) = match self.output.render_target.handle() {
            Some(handle) => (
                render_target_view(ctx, handle),
                render_target_msaa_view(ctx, handle),
            ),
            None => (None, None),
        };

        let viewport = &mut ctx.viewport;
        let msaa_enabled = viewport.msaa_sample_count() > 1;
        let parts = match viewport.frame_parts() {
            Some(parts) => parts,
            None => return,
        };
        let surface_resolve = if msaa_enabled {
            parts.resolve_view
        } else {
            None
        };
        let (color_view, resolve_target) =
            match (offscreen_view.as_ref(), offscreen_msaa_view.as_ref()) {
                (Some(resolve_view), Some(msaa_view)) => (msaa_view, Some(resolve_view)),
                (Some(resolve_view), None) => (resolve_view, None),
                (None, _) => (parts.view, surface_resolve),
            };
        let mut pass = parts
            .encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("TextPass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color_view,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                    resolve_target,
                })],
                depth_stencil_attachment: if prepared.stencil_clip_id.is_some() {
                    parts.depth_stencil_attachment(wgpu::LoadOp::Load, wgpu::LoadOp::Load)
                } else {
                    None
                },
                ..Default::default()
            });
        if let Some(stencil_clip_id) = prepared.stencil_clip_id {
            pass.set_stencil_reference(stencil_clip_id as u32);
        }
        let Some(renderer) = prepared.renderer.as_mut() else {
            return;
        };
        let _ = renderer.render(&resources.atlas, &resources.viewport, &mut pass);
    }

    fn batchable(&self) -> bool {
        true
    }

    fn batch_key(&self) -> Option<crate::view::render_pass::RenderPassBatchKey> {
        Some(crate::view::render_pass::RenderPassBatchKey {
            color_target: self.output.render_target.handle(),
            uses_depth_stencil: true,
        })
    }

    fn shared_render_pass_capable(&self) -> bool {
        true
    }
}

impl RenderTargetPass for TextPass {
    fn apply_clip(&mut self, scissor_rect: Option<[u32; 4]>) {
        self.params.scissor_rect = intersect_scissor_rects(self.params.scissor_rect, scissor_rect);
    }

    fn apply_stencil_clip(&mut self, clip_id: Option<u8>) {
        self.params.stencil_clip_id = clip_id;
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
