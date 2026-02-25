use std::any::Any;

use crate::view::frame_graph::PassContext;
use crate::view::frame_graph::builder::BuildContext;

pub mod clear_pass;
pub mod blur_pass;
pub mod composite_layer_pass;
pub mod draw_rect_pass;
pub(crate) mod render_target;
pub mod shadow_pass;
pub mod text_pass;
pub use blur_pass::BlurPass;
pub use clear_pass::ClearPass;
pub use composite_layer_pass::{CompositeLayerPass, LayerOut, LayerTag};
pub use draw_rect_pass::DrawRectPass;
pub use shadow_pass::{ShadowMesh, ShadowParams, ShadowPass};
pub use text_pass::{TextPass, prewarm_text_pipeline};

pub trait RenderPass {
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
    fn name(&self) -> &'static str;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

pub struct PassWrapper<P: RenderPass> {
    pub pass: P,
}

impl<P: RenderPass + 'static> RenderPassDyn for PassWrapper<P> {
    fn build(&mut self, builder: &mut BuildContext) {
        self.pass.build(builder);
    }

    fn execute(&mut self, ctx: &mut PassContext<'_, '_>) {
        self.pass.execute(ctx);
    }

    fn name(&self) -> &'static str {
        std::any::type_name::<P>()
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        &mut self.pass
    }
}
