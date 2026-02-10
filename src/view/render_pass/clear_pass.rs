use crate::view::frame_graph::builder::BuildContext;
use crate::view::render_pass::draw_rect_pass::{RenderTargetIn, RenderTargetOut, RenderTargetTag};
use crate::view::frame_graph::PassContext;
use crate::view::frame_graph::slot::OutSlot;
use crate::view::frame_graph::texture_resource::{TextureHandle, TextureResource};
use crate::view::render_pass::render_target_store::render_target_view;
use crate::view::render_pass::RenderPass;

pub struct ClearPass {
    color: [f32; 4],
    color_target: Option<TextureHandle>,
    input: ClearInput,
    output: ClearOutput,
}

#[derive(Default)]
pub struct ClearInput {
    pub render_target: RenderTargetIn,
}

#[derive(Default)]
pub struct ClearOutput {
    pub render_target: RenderTargetOut,
}

impl ClearPass {
    pub fn new(color: [f32; 4]) -> Self {
        Self {
            color,
            color_target: None,
            input: ClearInput::default(),
            output: ClearOutput::default(),
        }
    }

    pub fn set_color(&mut self, color: [f32; 4]) {
        self.color = color;
    }

    pub fn set_input(&mut self, input: RenderTargetIn) {
        self.input.render_target = input;
    }

    pub fn set_output(&mut self, output: RenderTargetOut) {
        self.output.render_target = output;
    }

    pub fn set_color_target(&mut self, color_target: Option<TextureHandle>) {
        self.color_target = color_target;
    }
}

impl RenderPass for ClearPass {
    type Input = ClearInput;
    type Output = ClearOutput;

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
        let color = wgpu::Color {
            r: self.color[0] as f64,
            g: self.color[1] as f64,
            b: self.color[2] as f64,
            a: self.color[3] as f64,
        };

        let offscreen_view = match self.color_target {
            Some(handle) => render_target_view(ctx, handle),
            None => None,
        };
        let parts = match ctx.viewport.frame_parts() {
            Some(parts) => parts,
            None => return,
        };
        let color_view = offscreen_view.as_ref().unwrap_or(parts.view);
        let _pass = parts.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Clear"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color_view,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(color),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
                resolve_target: None,
            })],
            depth_stencil_attachment: parts.depth_stencil_attachment(
                wgpu::LoadOp::Clear(1.0),
                wgpu::LoadOp::Clear(0),
            ),
            ..Default::default()
        });
    }
}
