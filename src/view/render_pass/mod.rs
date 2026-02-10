use crate::view::frame_graph::PassContext;
use crate::view::frame_graph::builder::BuildContext;

pub mod clear_pass;
pub mod composite_layer_pass;
pub mod draw_rect_pass;
pub(crate) mod render_target_store;
pub mod text_pass;
pub use clear_pass::ClearPass;
pub use composite_layer_pass::{CompositeLayerPass, LayerOut, LayerTag};
pub use draw_rect_pass::DrawRectPass;
pub use text_pass::{TextPass, prewarm_text_pipeline};

pub trait RenderPass{
    type Input: Default;
    type Output: Default;

    fn input(&self) -> &Self::Input;
    fn input_mut(&mut self) -> &mut Self::Input;

    fn output(&self) -> &Self::Output;
    fn output_mut(&mut self) -> &mut Self::Output;

    fn build(&mut self, builder: &mut BuildContext);
    fn execute(&mut self, ctx: &mut PassContext<'_, '_>);
}

pub trait RenderPassDyn {
    fn build(&mut self, builder: &mut BuildContext);
    fn execute(&mut self, ctx: &mut PassContext<'_, '_>);
}

pub struct PassWrapper<P: RenderPass> {
    pub pass: P,
}

impl<P: RenderPass> RenderPassDyn for PassWrapper<P> {
    fn build(&mut self, builder: &mut BuildContext) {
        self.pass.build(builder);
    }

    fn execute(&mut self, ctx: &mut PassContext<'_, '_>) {
        self.pass.execute(ctx);
    }
}
