use std::any::Any;

use crate::view::frame_graph::PassContext;
use crate::view::frame_graph::builder::BuildContext;
use crate::view::frame_graph::texture_resource::TextureHandle;

pub mod blur_pass;
pub mod clear_pass;
pub mod composite_layer_pass;
pub mod draw_rect_pass;
pub mod present_surface_pass;
pub(crate) mod render_target;
pub mod shadow_pass;
pub mod text_pass;
pub mod texture_composite_pass;
pub use blur_pass::BlurPass;
pub use clear_pass::ClearPass;
pub use composite_layer_pass::{CompositeLayerPass, LayerOut, LayerTag};
pub use draw_rect_pass::{AlphaRectPass, DrawRectPass, OpaqueRectPass, RectRenderMode};
pub use shadow_pass::{ShadowMesh, ShadowParams, ShadowPass};
pub use text_pass::{TextPass, prewarm_text_pipeline};
pub use texture_composite_pass::{
    TextureCompositeInput, TextureCompositeMaskIn, TextureCompositeOutput, TextureCompositeParams,
    TextureCompositePass, TextureCompositeSourceIn,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RenderPassBatchKey {
    pub color_target: Option<TextureHandle>,
    pub uses_depth_stencil: bool,
}

pub trait RenderPass {
    type Input: Default;
    type Output: Default;

    fn input(&self) -> &Self::Input;
    fn input_mut(&mut self) -> &mut Self::Input;

    fn output(&self) -> &Self::Output;
    fn output_mut(&mut self) -> &mut Self::Output;

    fn build(&mut self, builder: &mut BuildContext);
    fn compile_upload(&mut self, _ctx: &mut PassContext<'_, '_>) {}
    fn execute(
        &mut self,
        ctx: &mut PassContext<'_, '_>,
        render_pass: Option<&mut wgpu::RenderPass<'_>>,
    );
    fn batchable(&self) -> bool {
        false
    }
    fn batch_key(&self) -> Option<RenderPassBatchKey> {
        None
    }
    fn shared_render_pass_capable(&self) -> bool {
        false
    }
}

pub trait RenderPassDyn {
    fn build(&mut self, builder: &mut BuildContext);
    fn compile_upload(&mut self, ctx: &mut PassContext<'_, '_>);
    fn execute(
        &mut self,
        ctx: &mut PassContext<'_, '_>,
        render_pass: Option<&mut wgpu::RenderPass<'_>>,
    );
    fn batchable(&self) -> bool;
    fn batch_key(&self) -> Option<RenderPassBatchKey>;
    fn shared_render_pass_capable(&self) -> bool;
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

    fn execute(
        &mut self,
        ctx: &mut PassContext<'_, '_>,
        render_pass: Option<&mut wgpu::RenderPass<'_>>,
    ) {
        self.pass.execute(ctx, render_pass);
    }

    fn compile_upload(&mut self, ctx: &mut PassContext<'_, '_>) {
        self.pass.compile_upload(ctx);
    }

    fn batchable(&self) -> bool {
        self.pass.batchable()
    }

    fn batch_key(&self) -> Option<RenderPassBatchKey> {
        self.pass.batch_key()
    }

    fn shared_render_pass_capable(&self) -> bool {
        self.pass.shared_render_pass_capable()
    }

    fn name(&self) -> &'static str {
        std::any::type_name::<P>()
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        &mut self.pass
    }
}
