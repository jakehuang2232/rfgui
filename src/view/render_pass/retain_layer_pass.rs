use crate::view::frame_graph::{
    GraphicsColorAttachmentDescriptor, GraphicsPassBuilder, GraphicsPassMergePolicy,
};
use crate::view::render_pass::draw_rect_pass::RenderTargetOut;
use crate::view::render_pass::{GraphicsCtx, GraphicsPass};

#[derive(Clone, Copy)]
pub struct RetainLayerPass {
    output: RenderTargetOut,
}

impl RetainLayerPass {
    pub fn new(output: RenderTargetOut) -> Self {
        Self { output }
    }
}

impl GraphicsPass for RetainLayerPass {
    fn setup(&mut self, builder: &mut GraphicsPassBuilder<'_, '_>) {
        builder.set_graphics_merge_policy(GraphicsPassMergePolicy::Mergeable);
        if let Some(target) = builder.texture_target(&self.output) {
            builder.write_color(
                &self.output,
                GraphicsColorAttachmentDescriptor::load(target),
            );
        }
    }

    fn execute(&mut self, _ctx: &mut GraphicsCtx<'_, '_, '_, '_>) {}
}
