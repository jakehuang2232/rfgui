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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::frame_graph::slot::InSlot;
    use crate::view::frame_graph::texture_resource::{TextureDesc, TextureHandle, TextureResource};
    use crate::view::frame_graph::{
        AttachmentLoadOp, AttachmentTarget, BufferDesc, GraphicsDepthAspectDescriptor,
        GraphicsStencilAspectDescriptor, PassDescriptor, PassDetails, PassResourceUsage,
        ResourceHandle, ResourceMetadata, ResourceUsage,
    };
    use crate::view::render_pass::blur_pass::{BlurInput, BlurOutput, BlurPass, BlurPassParams};
    use crate::view::render_pass::clear_pass::{ClearInput, ClearOutput, ClearParams, ClearPass};
    use crate::view::render_pass::composite_layer_pass::{
        CompositeLayerInput, CompositeLayerOutput, CompositeLayerParams, CompositeLayerPass,
        LayerIn,
    };
    use crate::view::render_pass::debug_overlay_pass::{
        DebugOverlayInput, DebugOverlayOutput, DebugOverlayPass,
    };
    use crate::view::render_pass::draw_rect_pass::{
        DrawRectInput, DrawRectOutput, DrawRectPass, OpaqueRectPass, RectPassParams,
        RenderTargetIn, RenderTargetOut,
    };
    use crate::view::render_pass::present_surface_pass::{
        PresentSurfaceInput, PresentSurfaceOutput, PresentSurfaceParams, PresentSurfacePass,
    };
    use crate::view::render_pass::shadow_pass::{
        ShadowInput, ShadowMesh, ShadowOutput, ShadowParams, ShadowPass,
    };
    use crate::view::render_pass::text_pass::{TextInput, TextOutput, TextPass, TextPassParams};
    use crate::view::render_pass::texture_composite_pass::{
        TextureCompositeInput, TextureCompositeMaskTag, TextureCompositeOutput,
        TextureCompositeParams, TextureCompositePass, TextureCompositeSourceTag,
    };
    use glyphon::cosmic_text::Align;

    fn collect_setup_contract<P: RenderPass>(
        mut pass: P,
    ) -> (PassDescriptor, Vec<PassResourceUsage>) {
        let mut descriptor = PassDescriptor::graphics(std::any::type_name::<P>());
        let mut textures: Vec<TextureDesc> = Vec::new();
        let mut buffers: Vec<BufferDesc> = Vec::new();
        let mut texture_metadata: Vec<ResourceMetadata> = Vec::new();
        let mut buffer_metadata: Vec<ResourceMetadata> = Vec::new();
        let mut usages = Vec::new();
        let mut build_errors = Vec::new();
        let mut builder = PassBuilder {
            descriptor: &mut descriptor,
            textures: &mut textures,
            buffers: &mut buffers,
            texture_metadata: &mut texture_metadata,
            buffer_metadata: &mut buffer_metadata,
            usages: &mut usages,
            build_errors: &mut build_errors,
        };
        pass.setup(&mut builder);
        assert!(
            build_errors.is_empty(),
            "setup should not emit build errors: {:?}",
            build_errors
        );
        (descriptor, usages)
    }

    fn texture_usage(handle: TextureHandle, usage: ResourceUsage) -> PassResourceUsage {
        PassResourceUsage {
            resource: ResourceHandle::Texture(handle),
            usage,
        }
    }

    fn count_buffer_usages(usages: &[PassResourceUsage], usage: ResourceUsage) -> usize {
        usages
            .iter()
            .filter(|entry| matches!(entry.resource, ResourceHandle::Buffer(_)) && entry.usage == usage)
            .count()
    }

    #[test]
    fn blur_pass_setup_declares_input_uniform_and_color_target() {
        let layer = LayerIn::with_handle(TextureHandle(1));
        let output = RenderTargetOut::with_handle(TextureHandle(2));
        let (descriptor, usages) = collect_setup_contract(BlurPass::new(
            BlurPassParams::new(8.0),
            BlurInput { layer },
            BlurOutput { render_target: output },
        ));

        assert!(usages.contains(&texture_usage(TextureHandle(1), ResourceUsage::SampledRead)));
        assert!(usages.contains(&texture_usage(TextureHandle(2), ResourceUsage::ColorAttachmentWrite)));
        assert_eq!(count_buffer_usages(&usages, ResourceUsage::Produced), 1);
        assert_eq!(count_buffer_usages(&usages, ResourceUsage::UniformRead), 1);
        let PassDetails::Graphics(graphics) = descriptor.details else { panic!("expected graphics"); };
        assert_eq!(graphics.color_attachments.len(), 1);
    }

    #[test]
    fn clear_pass_setup_matches_surface_vs_offscreen_targets() {
        let (surface_descriptor, _) = collect_setup_contract(ClearPass::new(
            ClearParams::new([0.0, 0.0, 0.0, 0.0]),
            ClearInput,
            ClearOutput::default(),
        ));
        let PassDetails::Graphics(surface_graphics) = surface_descriptor.details else {
            panic!("expected graphics");
        };
        assert_eq!(surface_graphics.color_attachments[0].target, AttachmentTarget::Surface);
        let depth = surface_graphics
            .depth_stencil_attachment
            .expect("surface clear should declare depth/stencil");
        assert_eq!(depth.target, AttachmentTarget::Surface);
        assert_eq!(
            depth.depth,
            Some(GraphicsDepthAspectDescriptor::write(AttachmentLoadOp::Clear, Some(1.0)))
        );
        assert_eq!(
            depth.stencil,
            Some(GraphicsStencilAspectDescriptor::write(AttachmentLoadOp::Clear, Some(0)))
        );

        let offscreen = RenderTargetOut::with_handle(TextureHandle(3));
        let (offscreen_descriptor, usages) = collect_setup_contract(ClearPass::new(
            ClearParams::new([1.0, 0.0, 0.0, 1.0]),
            ClearInput,
            ClearOutput { render_target: offscreen },
        ));
        assert!(usages.contains(&texture_usage(TextureHandle(3), ResourceUsage::ColorAttachmentWrite)));
        let PassDetails::Graphics(offscreen_graphics) = offscreen_descriptor.details else {
            panic!("expected graphics");
        };
        assert!(offscreen_graphics.depth_stencil_attachment.is_none());

        let mut offscreen_with_depth = ClearPass::new(
            ClearParams::new([1.0, 0.0, 0.0, 1.0]),
            ClearInput,
            ClearOutput { render_target: offscreen },
        );
        crate::view::render_pass::render_target::RenderTargetPass::set_depth_stencil_target(
            &mut offscreen_with_depth,
            Some(AttachmentTarget::Texture(TextureHandle(18))),
        );
        let (offscreen_descriptor, usages) = collect_setup_contract(offscreen_with_depth);
        assert!(usages.contains(&texture_usage(
            TextureHandle(18),
            ResourceUsage::DepthWrite
        )));
        assert!(usages.contains(&texture_usage(
            TextureHandle(18),
            ResourceUsage::StencilWrite
        )));
        let PassDetails::Graphics(offscreen_graphics) = offscreen_descriptor.details else {
            panic!("expected graphics");
        };
        let depth = offscreen_graphics
            .depth_stencil_attachment
            .expect("configured offscreen clear should declare depth/stencil");
        assert_eq!(depth.target, AttachmentTarget::Texture(TextureHandle(18)));
    }

    #[test]
    fn composite_layer_pass_setup_declares_layer_and_geometry_buffers() {
        let (descriptor, usages) = collect_setup_contract(CompositeLayerPass::new(
            CompositeLayerParams {
                rect_pos: [0.0, 0.0],
                rect_size: [10.0, 10.0],
                corner_radii: [0.0; 4],
                opacity: 1.0,
                scissor_rect: None,
            },
            CompositeLayerInput {
                layer: LayerIn::with_handle(TextureHandle(4)),
            },
            CompositeLayerOutput {
                render_target: RenderTargetOut::with_handle(TextureHandle(5)),
            },
        ));

        assert!(usages.contains(&texture_usage(TextureHandle(4), ResourceUsage::SampledRead)));
        assert!(usages.contains(&texture_usage(TextureHandle(5), ResourceUsage::ColorAttachmentWrite)));
        assert_eq!(count_buffer_usages(&usages, ResourceUsage::Produced), 2);
        assert_eq!(count_buffer_usages(&usages, ResourceUsage::VertexRead), 1);
        assert_eq!(count_buffer_usages(&usages, ResourceUsage::IndexRead), 1);
        let PassDetails::Graphics(graphics) = descriptor.details else { panic!("expected graphics"); };
        assert_eq!(graphics.color_attachments.len(), 1);
    }

    #[test]
    fn debug_overlay_pass_setup_declares_surface_depth_stencil_reads() {
        let mut pass = DebugOverlayPass::new(
            DebugOverlayInput,
            DebugOverlayOutput {
                render_target: RenderTargetOut::with_handle(TextureHandle(6)),
            },
        );
        pass.set_depth_stencil_target(Some(AttachmentTarget::Texture(TextureHandle(19))));
        let (descriptor, usages) = collect_setup_contract(pass);
        assert!(usages.contains(&texture_usage(TextureHandle(6), ResourceUsage::ColorAttachmentWrite)));
        let PassDetails::Graphics(graphics) = descriptor.details else { panic!("expected graphics"); };
        let depth = graphics
            .depth_stencil_attachment
            .expect("debug overlay should declare depth/stencil");
        assert_eq!(depth.target, AttachmentTarget::Texture(TextureHandle(19)));
        assert_eq!(depth.depth, Some(GraphicsDepthAspectDescriptor::read()));
        assert_eq!(depth.stencil, Some(GraphicsStencilAspectDescriptor::read()));
    }

    #[test]
    fn draw_rect_pass_setup_declares_input_uniform_color_and_depth_stencil() {
        let mut draw = DrawRectPass::new(
            RectPassParams::default(),
            DrawRectInput {
                render_target: RenderTargetIn::with_handle(TextureHandle(7)),
            },
            DrawRectOutput {
                render_target: RenderTargetOut::with_handle(TextureHandle(8)),
            },
        );
        draw.set_depth_stencil_target(Some(AttachmentTarget::Texture(TextureHandle(20))));
        let (descriptor, usages) = collect_setup_contract(draw);
        assert!(usages.contains(&texture_usage(TextureHandle(7), ResourceUsage::SampledRead)));
        assert!(usages.contains(&texture_usage(TextureHandle(8), ResourceUsage::ColorAttachmentWrite)));
        assert_eq!(count_buffer_usages(&usages, ResourceUsage::Produced), 1);
        assert_eq!(count_buffer_usages(&usages, ResourceUsage::UniformRead), 1);
        let PassDetails::Graphics(graphics) = descriptor.details else { panic!("expected graphics"); };
        let depth = graphics
            .depth_stencil_attachment
            .expect("draw rect should declare depth/stencil");
        assert_eq!(depth.target, AttachmentTarget::Texture(TextureHandle(20)));
    }

    #[test]
    fn opaque_rect_pass_setup_matches_draw_rect_contract() {
        let mut draw = DrawRectPass::new(
            RectPassParams::default(),
            DrawRectInput {
                render_target: RenderTargetIn::with_handle(TextureHandle(9)),
            },
            DrawRectOutput {
                render_target: RenderTargetOut::with_handle(TextureHandle(10)),
            },
        );
        draw.set_depth_stencil_target(Some(AttachmentTarget::Texture(TextureHandle(21))));
        let (descriptor, usages) = collect_setup_contract(OpaqueRectPass::from_draw_rect_pass(draw));
        assert!(usages.contains(&texture_usage(TextureHandle(9), ResourceUsage::SampledRead)));
        assert!(usages.contains(&texture_usage(TextureHandle(10), ResourceUsage::ColorAttachmentWrite)));
        assert_eq!(count_buffer_usages(&usages, ResourceUsage::UniformRead), 1);
        let PassDetails::Graphics(graphics) = descriptor.details else { panic!("expected graphics"); };
        assert!(graphics.depth_stencil_attachment.is_some());
    }

    #[test]
    fn present_surface_pass_setup_declares_sampled_source_and_surface_target() {
        let (descriptor, usages) = collect_setup_contract(PresentSurfacePass::new(
            PresentSurfaceParams,
            PresentSurfaceInput {
                source: RenderTargetIn::with_handle(TextureHandle(11)),
            },
            PresentSurfaceOutput,
        ));
        assert!(usages.contains(&texture_usage(TextureHandle(11), ResourceUsage::SampledRead)));
        let PassDetails::Graphics(graphics) = descriptor.details else { panic!("expected graphics"); };
        assert_eq!(graphics.color_attachments.len(), 1);
        assert_eq!(graphics.color_attachments[0].target, AttachmentTarget::Surface);
    }

    #[test]
    fn shadow_pass_setup_declares_uniform_buffers_and_outputs() {
        let (descriptor, usages) = collect_setup_contract(ShadowPass::new(
            ShadowMesh::default(),
            ShadowParams::default(),
            ShadowInput,
            ShadowOutput {
                render_target: RenderTargetOut::with_handle(TextureHandle(12)),
                mask_render_target: RenderTargetOut::with_handle(TextureHandle(13)),
            },
        ));
        assert!(usages.contains(&texture_usage(TextureHandle(12), ResourceUsage::ColorAttachmentWrite)));
        assert!(usages.contains(&texture_usage(TextureHandle(13), ResourceUsage::ColorAttachmentWrite)));
        assert_eq!(count_buffer_usages(&usages, ResourceUsage::Produced), 3);
        assert_eq!(count_buffer_usages(&usages, ResourceUsage::UniformRead), 3);
        let PassDetails::Graphics(graphics) = descriptor.details else { panic!("expected graphics"); };
        assert_eq!(graphics.color_attachments.len(), 2);
    }

    #[test]
    fn text_pass_setup_declares_staging_and_optional_stencil_reads() {
        let params = TextPassParams {
            content: "hello".into(),
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 20.0,
            color: [1.0; 4],
            opacity: 1.0,
            font_size: 14.0,
            line_height: 16.0,
            font_weight: 400,
            font_families: Vec::new(),
            align: Align::Left,
            allow_wrap: true,
            scissor_rect: None,
            stencil_clip_id: Some(1),
        };
        let mut pass = TextPass::new(
            params,
            TextInput,
            TextOutput {
                render_target: RenderTargetOut::with_handle(TextureHandle(14)),
            },
        );
        pass.set_depth_stencil_target(Some(AttachmentTarget::Texture(TextureHandle(22))));
        let (descriptor, usages) = collect_setup_contract(pass);
        assert!(usages.contains(&texture_usage(TextureHandle(14), ResourceUsage::ColorAttachmentWrite)));
        assert_eq!(count_buffer_usages(&usages, ResourceUsage::Produced), 1);
        assert_eq!(count_buffer_usages(&usages, ResourceUsage::UniformRead), 1);
        let PassDetails::Graphics(graphics) = descriptor.details else { panic!("expected graphics"); };
        let depth = graphics
            .depth_stencil_attachment
            .expect("text pass with stencil clip should declare depth/stencil");
        assert_eq!(depth.target, AttachmentTarget::Texture(TextureHandle(22)));
        assert_eq!(depth.depth, Some(GraphicsDepthAspectDescriptor::read()));
        assert_eq!(depth.stencil, Some(GraphicsStencilAspectDescriptor::read()));
    }

    #[test]
    fn texture_composite_pass_setup_declares_all_inputs_outputs_and_stencil_reads() {
        let mut pass = TextureCompositePass::new(
            TextureCompositeParams::default(),
            TextureCompositeInput {
                source: InSlot::<TextureResource, TextureCompositeSourceTag>::with_handle(TextureHandle(15)),
                mask: InSlot::<TextureResource, TextureCompositeMaskTag>::with_handle(TextureHandle(16)),
            },
            TextureCompositeOutput {
                render_target: RenderTargetOut::with_handle(TextureHandle(17)),
            },
        );
        pass.set_depth_stencil_target(Some(AttachmentTarget::Texture(TextureHandle(23))));
        let (descriptor, usages) = collect_setup_contract(pass);
        assert!(usages.contains(&texture_usage(TextureHandle(15), ResourceUsage::SampledRead)));
        assert!(usages.contains(&texture_usage(TextureHandle(16), ResourceUsage::SampledRead)));
        assert!(usages.contains(&texture_usage(TextureHandle(17), ResourceUsage::ColorAttachmentWrite)));
        assert_eq!(count_buffer_usages(&usages, ResourceUsage::Produced), 3);
        assert_eq!(count_buffer_usages(&usages, ResourceUsage::UniformRead), 1);
        assert_eq!(count_buffer_usages(&usages, ResourceUsage::VertexRead), 1);
        assert_eq!(count_buffer_usages(&usages, ResourceUsage::IndexRead), 1);
        let PassDetails::Graphics(graphics) = descriptor.details else { panic!("expected graphics"); };
        let depth = graphics
            .depth_stencil_attachment
            .expect("texture composite should declare depth/stencil");
        assert_eq!(depth.target, AttachmentTarget::Texture(TextureHandle(23)));
        assert_eq!(depth.depth, Some(GraphicsDepthAspectDescriptor::read()));
        assert_eq!(depth.stencil, Some(GraphicsStencilAspectDescriptor::read()));
    }
}
