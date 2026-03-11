use std::any::Any;

use crate::view::frame_graph::{GraphicsRecordContext, PassBuilder, PrepareContext};
use crate::view::frame_graph::texture_resource::TextureHandle;

pub mod blur_pass;
pub mod clear_pass;
pub mod composite_layer_pass;
pub mod debug_overlay_pass;
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
    fn setup(&mut self, builder: &mut PassBuilder<'_>);
    fn prepare(&mut self, _ctx: &mut PrepareContext<'_, '_>) {}
    fn record(&mut self, ctx: &mut GraphicsRecordContext<'_, '_, '_>);
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
    fn setup(&mut self, builder: &mut PassBuilder<'_>);
    fn prepare(&mut self, ctx: &mut PrepareContext<'_, '_>);
    fn record(&mut self, ctx: &mut GraphicsRecordContext<'_, '_, '_>);
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
    fn setup(&mut self, builder: &mut PassBuilder<'_>) {
        self.pass.setup(builder);
    }

    fn record(&mut self, ctx: &mut GraphicsRecordContext<'_, '_, '_>) {
        self.pass.record(ctx);
    }

    fn prepare(&mut self, ctx: &mut PrepareContext<'_, '_>) {
        self.pass.prepare(ctx);
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
