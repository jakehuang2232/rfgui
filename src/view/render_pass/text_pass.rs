use std::sync::{Mutex, OnceLock};

use glyphon::{
    Attrs, Buffer, Cache, Color as GlyphonColor, Family, FontSystem, Metrics, Resolution, Shaping,
    SwashCache, TextArea, TextAtlas, TextBounds, TextRenderer, Viewport as GlyphonViewport,
};

use crate::view::frame_graph::builder::BuildContext;
use crate::view::frame_graph::slot::OutSlot;
use crate::view::frame_graph::texture_resource::{TextureHandle, TextureResource};
use crate::view::frame_graph::PassContext;
use crate::view::render_pass::draw_rect_pass::{RenderTargetIn, RenderTargetOut, RenderTargetTag};
use crate::view::render_pass::render_target_store::render_target_view;
use crate::view::render_pass::RenderPass;

pub struct TextPass {
    content: String,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    color: [f32; 4],
    opacity: f32,
    font_size: f32,
    line_height: f32,
    font_families: Vec<String>,
    scissor_rect: Option<[u32; 4]>,
    color_target: Option<TextureHandle>,
    input: TextInput,
    output: TextOutput,
}

#[derive(Default)]
pub struct TextInput {
    pub render_target: RenderTargetIn,
}

#[derive(Default)]
pub struct TextOutput {
    pub render_target: RenderTargetOut,
}

impl TextPass {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        content: String,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: [f32; 4],
        opacity: f32,
        font_size: f32,
        line_height: f32,
        font_families: Vec<String>,
    ) -> Self {
        Self {
            content,
            x,
            y,
            width,
            height,
            color,
            opacity,
            font_size,
            line_height,
            font_families,
            scissor_rect: None,
            color_target: None,
            input: TextInput::default(),
            output: TextOutput::default(),
        }
    }

    pub fn set_input(&mut self, input: RenderTargetIn) {
        self.input.render_target = input;
    }

    pub fn set_output(&mut self, output: RenderTargetOut) {
        self.output.render_target = output;
    }

    pub fn set_scissor_rect(&mut self, scissor_rect: Option<[u32; 4]>) {
        self.scissor_rect = scissor_rect;
    }

    pub fn set_color_target(&mut self, color_target: Option<TextureHandle>) {
        self.color_target = color_target;
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
        if let Some(handle) = self.input.render_target.handle() {
            let source: OutSlot<TextureResource, RenderTargetTag> = OutSlot::with_handle(handle);
            builder.read_texture(&mut self.input.render_target, &source);
        }
        if self.output.render_target.handle().is_some() {
            builder.write_texture(&mut self.output.render_target);
        }
    }

    fn execute(&mut self, ctx: &mut PassContext<'_, '_>) {
        if self.content.is_empty() || self.width <= 0.0 || self.height <= 0.0 {
            return;
        }

        let offscreen_view = match self.color_target {
            Some(handle) => render_target_view(ctx, handle),
            None => None,
        };

        let viewport = &mut ctx.viewport;
        let device = match viewport.device() {
            Some(device) => device,
            None => return,
        };
        let queue = match viewport.queue() {
            Some(queue) => queue,
            None => return,
        };

        let (screen_w, screen_h) = viewport.surface_size();
        let format = viewport.surface_format();

        let mut global = text_resources(device, queue, format);
        let resources = global.resources.as_mut().unwrap();
        resources.prepare_buffer(
            self.content.as_str(),
            self.width,
            self.height,
            self.font_size,
            self.line_height,
            self.font_families.as_slice(),
        );
        resources
            .viewport
            .update(queue, Resolution { width: screen_w, height: screen_h });

        let left = self.x.max(0.0);
        let top = self.y.max(0.0);
        let right = (self.x + self.width).max(left);
        let bottom = (self.y + self.height).max(top);

        let text_area = TextArea {
            buffer: &resources.buffer,
            left,
            top,
            scale: 1.0,
            bounds: TextBounds {
                left: left as i32,
                top: top as i32,
                right: right as i32,
                bottom: bottom as i32,
            },
            default_color: to_glyphon_color(self.color, self.opacity),
            custom_glyphs: &[],
        };

        if resources
            .renderer
            .prepare(
                device,
                queue,
                &mut resources.font_system,
                &mut resources.atlas,
                &resources.viewport,
                [text_area],
                &mut resources.swash_cache,
            )
            .is_err()
        {
            return;
        }

        let parts = match viewport.frame_parts() {
            Some(parts) => parts,
            None => return,
        };

        let color_view = offscreen_view.as_ref().unwrap_or(parts.view);
        let mut pass = parts.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("TextPass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color_view,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
                resolve_target: None,
            })],
            depth_stencil_attachment: None,
            ..Default::default()
        });

        if let Some([x, y, width, height]) = self.scissor_rect {
            pass.set_scissor_rect(x, y, width, height);
        }

        let _ = resources
            .renderer
            .render(&resources.atlas, &resources.viewport, &mut pass);
    }
}

fn to_glyphon_color(color: [f32; 4], opacity: f32) -> GlyphonColor {
    fn to_u8(v: f32) -> u8 {
        (v.clamp(0.0, 1.0) * 255.0).round() as u8
    }

    let alpha = to_u8(color[3] * opacity.clamp(0.0, 1.0));
    GlyphonColor::rgba(to_u8(color[0]), to_u8(color[1]), to_u8(color[2]), alpha)
}

struct TextResources {
    font_system: FontSystem,
    swash_cache: SwashCache,
    atlas: TextAtlas,
    renderer: TextRenderer,
    viewport: GlyphonViewport,
    buffer: Buffer,
    format: wgpu::TextureFormat,
}

impl TextResources {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let mut font_system = FontSystem::new();
        let swash_cache = SwashCache::new();
        let cache = Cache::new(device);
        let mut atlas = TextAtlas::new(device, queue, &cache, format);
        let viewport = GlyphonViewport::new(device, &cache);
        let renderer = TextRenderer::new(&mut atlas, device, wgpu::MultisampleState::default(), None);
        let metrics = Metrics::new(16.0, 16.0 * 1.25);
        let buffer = Buffer::new(&mut font_system, metrics);

        Self {
            font_system,
            swash_cache,
            atlas,
            renderer,
            viewport,
            buffer,
            format,
        }
    }

    fn prepare_buffer(
        &mut self,
        content: &str,
        width: f32,
        height: f32,
        font_size: f32,
        line_height: f32,
        font_families: &[String],
    ) {
        self.buffer.set_metrics(
            &mut self.font_system,
            Metrics::new(font_size.max(1.0), (font_size * line_height.max(0.8)).max(1.0)),
        );
        self.buffer
            .set_size(&mut self.font_system, Some(width.max(1.0)), Some(height.max(1.0)));

        let attrs = if let Some(first) = font_families.first() {
            Attrs::new().family(Family::Name(first.as_str()))
        } else {
            Attrs::new()
        };

        self.buffer
            .set_text(&mut self.font_system, content, &attrs, Shaping::Advanced, None);
        self.buffer.shape_until_scroll(&mut self.font_system, false);
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

pub fn prewarm_text_pipeline(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) {
    drop(text_resources(device, queue, format));
}
