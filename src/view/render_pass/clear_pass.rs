use crate::view::frame_graph::{
    AttachmentLoadOp, GraphicsColorAttachmentOps, GraphicsPassBuilder, GraphicsPassMergePolicy,
};
use crate::view::render_pass::draw_rect_pass::RenderTargetOut;
use crate::view::render_pass::render_target::GraphicsPassContext as RenderPassContext;
use crate::view::render_pass::{GraphicsCtx, GraphicsPass};

pub struct ClearPass {
    params: ClearParams,
    input: ClearInput,
    output: ClearOutput,
}

pub struct ClearParams {
    pub color: [f32; 4],
}

impl ClearParams {
    pub fn new(color: [f32; 4]) -> Self {
        Self { color }
    }
}

#[derive(Default)]
pub struct ClearInput {
    pub pass_context: RenderPassContext,
    pub clear_depth_stencil: bool,
}

#[derive(Default)]
pub struct ClearOutput {
    pub render_target: RenderTargetOut,
}

impl ClearPass {
    pub fn new(params: ClearParams, input: ClearInput, output: ClearOutput) -> Self {
        Self {
            params,
            input,
            output,
        }
    }
}

impl GraphicsPass for ClearPass {
    fn setup(&mut self, builder: &mut GraphicsPassBuilder<'_, '_>) {
        builder.set_graphics_merge_policy(GraphicsPassMergePolicy::Mergeable);
        let color = [
            self.params.color[0] as f64,
            self.params.color[1] as f64,
            self.params.color[2] as f64,
            self.params.color[3] as f64,
        ];
        if let Some(target) = builder.texture_target(&self.output.render_target) {
            let _ = target;
            builder.write_color(
                &self.output.render_target,
                GraphicsColorAttachmentOps::clear(color),
            );
            if self.input.clear_depth_stencil {
                builder.write_output_depth(AttachmentLoadOp::Clear, Some(1.0));
                builder.write_output_stencil(AttachmentLoadOp::Clear, Some(0));
            }
        } else {
            builder.write_surface_color(GraphicsColorAttachmentOps::clear(color));
            builder.write_output_depth(AttachmentLoadOp::Clear, Some(1.0));
            builder.write_output_stencil(AttachmentLoadOp::Clear, Some(0));
        }
    }

    fn execute(&mut self, _ctx: &mut GraphicsCtx<'_, '_, '_, '_>) {
        // Clear work is fully represented by attachment load ops declared in setup().
    }
}
