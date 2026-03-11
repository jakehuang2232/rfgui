use crate::view::frame_graph::{
    AttachmentLoadOp, GraphicsColorAttachmentDescriptor, GraphicsPassBuilder,
    GraphicsPassMergePolicy, GraphicsRecordContext,
};
use crate::view::render_pass::draw_rect_pass::RenderTargetOut;
use crate::view::render_pass::render_target::GraphicsPassContext;
use crate::view::render_pass::{GraphicsEncodeScope, GraphicsPass};

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
    pub pass_context: GraphicsPassContext,
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
            builder.write_color(
                &self.output.render_target,
                GraphicsColorAttachmentDescriptor::clear(target, color),
            );
            if self.input.clear_depth_stencil {
                if let Some(depth_stencil_target) = self.input.pass_context.depth_stencil_target {
                    builder.write_depth(depth_stencil_target, AttachmentLoadOp::Clear, Some(1.0));
                    builder.write_stencil(depth_stencil_target, AttachmentLoadOp::Clear, Some(0));
                }
            }
        } else {
            builder.write_surface_color(GraphicsColorAttachmentDescriptor::clear(
                builder.surface_target(),
                color,
            ));
            builder.write_depth(builder.surface_target(), AttachmentLoadOp::Clear, Some(1.0));
            builder.write_stencil(builder.surface_target(), AttachmentLoadOp::Clear, Some(0));
        }
    }

    fn encode(
        &mut self,
        _ctx: &mut GraphicsRecordContext<'_, '_>,
        _scope: GraphicsEncodeScope<'_, '_>,
    ) {
        // Clear work is fully represented by attachment load ops declared in setup().
    }
}
