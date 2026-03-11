use crate::view::frame_graph::{
    ComputePassBuilder, ComputeRecordContext, GraphicsPassBuilder, GraphicsRecordContext,
    PrepareContext, TransferPassBuilder, TransferRecordContext,
};

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

pub trait GraphicsPass {
    fn setup(&mut self, builder: &mut GraphicsPassBuilder<'_, '_>);
    fn prepare(&mut self, _ctx: &mut PrepareContext<'_, '_>) {}
    fn encode(
        &mut self,
        ctx: &mut GraphicsRecordContext<'_, '_>,
        scope: GraphicsEncodeScope<'_, '_>,
    );
}

pub trait ComputePass {
    fn setup(&mut self, builder: &mut ComputePassBuilder<'_, '_>);
    fn prepare(&mut self, _ctx: &mut PrepareContext<'_, '_>) {}
    fn encode(&mut self, ctx: &mut ComputeRecordContext<'_, '_>, scope: ComputeEncodeScope<'_, '_>);
}

pub trait TransferPass {
    fn setup(&mut self, builder: &mut TransferPassBuilder<'_, '_>);
    fn prepare(&mut self, _ctx: &mut PrepareContext<'_, '_>) {}
    fn encode(&mut self, ctx: &mut TransferRecordContext<'_, '_>, scope: TransferEncodeScope<'_>);
}

pub(crate) trait PassNodeDyn {
    fn setup(&mut self, builder: &mut crate::view::frame_graph::PassBuilderState<'_>);
    fn prepare(&mut self, ctx: &mut PrepareContext<'_, '_>);
    fn encode_graphics(
        &mut self,
        ctx: &mut GraphicsRecordContext<'_, '_>,
        scope: GraphicsEncodeScope<'_, '_>,
    );
    fn encode_compute(
        &mut self,
        ctx: &mut ComputeRecordContext<'_, '_>,
        scope: ComputeEncodeScope<'_, '_>,
    );
    fn encode_transfer(
        &mut self,
        ctx: &mut TransferRecordContext<'_, '_>,
        scope: TransferEncodeScope<'_>,
    );
    fn name(&self) -> &'static str;
}

pub enum GraphicsEncodeScope<'a, 'pass> {
    Command(&'a mut wgpu::CommandEncoder),
    Render(&'a mut wgpu::RenderPass<'pass>),
}

pub enum ComputeEncodeScope<'a, 'pass> {
    Command(&'a mut wgpu::CommandEncoder),
    Compute(&'a mut wgpu::ComputePass<'pass>),
}

pub enum TransferEncodeScope<'a> {
    Command(&'a mut wgpu::CommandEncoder),
}

pub(crate) struct GraphicsPassWrapper<P: GraphicsPass> {
    pub pass: P,
}

impl<P: GraphicsPass + 'static> PassNodeDyn for GraphicsPassWrapper<P> {
    fn setup(&mut self, builder: &mut crate::view::frame_graph::PassBuilderState<'_>) {
        let mut builder = GraphicsPassBuilder::new(builder);
        self.pass.setup(&mut builder);
    }

    fn prepare(&mut self, ctx: &mut PrepareContext<'_, '_>) {
        self.pass.prepare(ctx);
    }

    fn encode_graphics(
        &mut self,
        ctx: &mut GraphicsRecordContext<'_, '_>,
        scope: GraphicsEncodeScope<'_, '_>,
    ) {
        self.pass.encode(ctx, scope);
    }

    fn encode_compute(
        &mut self,
        _ctx: &mut ComputeRecordContext<'_, '_>,
        _scope: ComputeEncodeScope<'_, '_>,
    ) {
        unreachable!("graphics pass encoded through compute path");
    }

    fn encode_transfer(
        &mut self,
        _ctx: &mut TransferRecordContext<'_, '_>,
        _scope: TransferEncodeScope<'_>,
    ) {
        unreachable!("graphics pass encoded through transfer path");
    }

    fn name(&self) -> &'static str {
        std::any::type_name::<P>()
    }
}

pub(crate) struct ComputePassWrapper<P: ComputePass> {
    pub pass: P,
}

impl<P: ComputePass + 'static> PassNodeDyn for ComputePassWrapper<P> {
    fn setup(&mut self, builder: &mut crate::view::frame_graph::PassBuilderState<'_>) {
        let mut builder = ComputePassBuilder::new(builder);
        self.pass.setup(&mut builder);
    }

    fn prepare(&mut self, ctx: &mut PrepareContext<'_, '_>) {
        self.pass.prepare(ctx);
    }

    fn encode_graphics(
        &mut self,
        _ctx: &mut GraphicsRecordContext<'_, '_>,
        _scope: GraphicsEncodeScope<'_, '_>,
    ) {
        unreachable!("compute pass encoded through graphics path");
    }

    fn encode_compute(
        &mut self,
        ctx: &mut ComputeRecordContext<'_, '_>,
        scope: ComputeEncodeScope<'_, '_>,
    ) {
        self.pass.encode(ctx, scope);
    }

    fn encode_transfer(
        &mut self,
        _ctx: &mut TransferRecordContext<'_, '_>,
        _scope: TransferEncodeScope<'_>,
    ) {
        unreachable!("compute pass encoded through transfer path");
    }

    fn name(&self) -> &'static str {
        std::any::type_name::<P>()
    }
}

pub(crate) struct TransferPassWrapper<P: TransferPass> {
    pub pass: P,
}

impl<P: TransferPass + 'static> PassNodeDyn for TransferPassWrapper<P> {
    fn setup(&mut self, builder: &mut crate::view::frame_graph::PassBuilderState<'_>) {
        let mut builder = TransferPassBuilder::new(builder);
        self.pass.setup(&mut builder);
    }

    fn prepare(&mut self, ctx: &mut PrepareContext<'_, '_>) {
        self.pass.prepare(ctx);
    }

    fn encode_graphics(
        &mut self,
        _ctx: &mut GraphicsRecordContext<'_, '_>,
        _scope: GraphicsEncodeScope<'_, '_>,
    ) {
        unreachable!("transfer pass encoded through graphics path");
    }

    fn encode_compute(
        &mut self,
        _ctx: &mut ComputeRecordContext<'_, '_>,
        _scope: ComputeEncodeScope<'_, '_>,
    ) {
        unreachable!("transfer pass encoded through compute path");
    }

    fn encode_transfer(
        &mut self,
        ctx: &mut TransferRecordContext<'_, '_>,
        scope: TransferEncodeScope<'_>,
    ) {
        self.pass.encode(ctx, scope);
    }

    fn name(&self) -> &'static str {
        std::any::type_name::<P>()
    }
}
