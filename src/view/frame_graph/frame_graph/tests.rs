use super::*;
use crate::view::frame_graph::slot::{InSlot, OutSlot};
use crate::view::frame_graph::texture_resource::{TextureDesc, TextureResource};
use crate::view::frame_graph::{
    BufferReadUsage, BufferResource, ComputePassBuilder, GraphicsPassBuilder, TransferPassBuilder,
};
use crate::view::render_pass::draw_rect_pass::{
    DrawRectInput, DrawRectOutput, DrawRectPass, OpaqueRectPass, RectPassParams, RenderTargetIn,
    RenderTargetOut,
};
use crate::view::render_pass::present_surface_pass::{
    PresentSurfaceInput, PresentSurfaceOutput, PresentSurfaceParams, PresentSurfacePass,
};
use crate::view::render_pass::{
    ComputeCtx, ComputePass, GraphicsCtx, GraphicsPass, TransferCtx, TransferPass,
};

#[derive(Default)]
struct WritePass {
    output: OutSlot<TextureResource, ()>,
}

impl GraphicsPass for WritePass {
    fn setup(&mut self, builder: &mut GraphicsPassBuilder<'_, '_>) {
        let target = builder
            .texture_target(&self.output)
            .expect("test output should have texture target");
        let _ = target;
        builder.write_color(
            &self.output,
            GraphicsColorAttachmentOps::clear([0.0, 0.0, 0.0, 0.0]),
        );
    }

    fn execute(&mut self, _ctx: &mut GraphicsCtx<'_, '_, '_, '_>) {}
}

#[derive(Default)]
struct ReadPass {
    input: InSlot<TextureResource, ()>,
}

impl GraphicsPass for ReadPass {
    fn setup(&mut self, builder: &mut GraphicsPassBuilder<'_, '_>) {
        if let Some(handle) = self.input.handle() {
            builder.read_texture(&mut self.input, &OutSlot::with_handle(handle));
        }
    }

    fn execute(&mut self, _ctx: &mut GraphicsCtx<'_, '_, '_, '_>) {}
}

#[derive(Default)]
struct ModifyPass {
    target: OutSlot<TextureResource, ()>,
}

impl GraphicsPass for ModifyPass {
    fn setup(&mut self, builder: &mut GraphicsPassBuilder<'_, '_>) {
        let target = builder
            .texture_target(&self.target)
            .expect("test output should have texture target");
        let _ = target;
        builder.write_color(&self.target, GraphicsColorAttachmentOps::load());
    }

    fn execute(&mut self, _ctx: &mut GraphicsCtx<'_, '_, '_, '_>) {}
}

#[derive(Default)]
struct SurfacePass;

#[derive(Default)]
struct MergeableSurfacePass;

struct MergeablePrepPass {
    output: OutSlot<TextureResource, ()>,
}

struct MergeableFinalReadPass {
    input: InSlot<TextureResource, ()>,
}

impl GraphicsPass for SurfacePass {
    fn setup(&mut self, builder: &mut GraphicsPassBuilder<'_, '_>) {
        builder.write_surface_color(GraphicsColorAttachmentOps::clear([0.0, 0.0, 0.0, 0.0]));
    }

    fn execute(&mut self, _ctx: &mut GraphicsCtx<'_, '_, '_, '_>) {}
}

impl GraphicsPass for MergeableSurfacePass {
    fn setup(&mut self, builder: &mut GraphicsPassBuilder<'_, '_>) {
        builder.set_graphics_merge_policy(GraphicsPassMergePolicy::Mergeable);
        builder.write_surface_color(GraphicsColorAttachmentOps::clear([0.0, 0.0, 0.0, 0.0]));
    }

    fn execute(&mut self, _ctx: &mut GraphicsCtx<'_, '_, '_, '_>) {}
}

impl GraphicsPass for MergeablePrepPass {
    fn setup(&mut self, builder: &mut GraphicsPassBuilder<'_, '_>) {
        builder.set_graphics_merge_policy(GraphicsPassMergePolicy::Mergeable);
        let target = builder
            .texture_target(&self.output)
            .expect("prep output should have texture target");
        let _ = target;
        builder.write_color(
            &self.output,
            GraphicsColorAttachmentOps::clear([0.0, 0.0, 0.0, 0.0]),
        );
    }

    fn execute(&mut self, _ctx: &mut GraphicsCtx<'_, '_, '_, '_>) {}
}

impl GraphicsPass for MergeableFinalReadPass {
    fn setup(&mut self, builder: &mut GraphicsPassBuilder<'_, '_>) {
        builder.set_graphics_merge_policy(GraphicsPassMergePolicy::Mergeable);
        if let Some(handle) = self.input.handle() {
            builder.read_texture(&mut self.input, &OutSlot::with_handle(handle));
        }
        builder.write_surface_color(GraphicsColorAttachmentOps::clear([0.0, 0.0, 0.0, 0.0]));
    }

    fn execute(&mut self, _ctx: &mut GraphicsCtx<'_, '_, '_, '_>) {}
}

#[derive(Default)]
struct PersistentInternalPass {
    output: OutSlot<TextureResource, ()>,
}

#[derive(Default)]
struct BufferWritePass {
    output: OutSlot<BufferResource, ()>,
}

struct ExistingBufferWritePass {
    output: OutSlot<BufferResource, ()>,
}

struct BufferReadPass {
    input: OutSlot<BufferResource, ()>,
}

#[derive(Default)]
struct InlineLoadPass {
    target: OutSlot<TextureResource, ()>,
}

#[derive(Default)]
struct DepthStencilWritePass {
    target: OutSlot<TextureResource, ()>,
}

#[derive(Default)]
struct DepthStencilReadPass {
    target: OutSlot<TextureResource, ()>,
}

#[derive(Default)]
struct ComputeStubPass;

#[derive(Default)]
struct TransferStubPass;

impl GraphicsPass for BufferWritePass {
    fn setup(&mut self, builder: &mut GraphicsPassBuilder<'_, '_>) {
        self.output = builder.create_buffer(BufferDesc {
            size: 16,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::UNIFORM,
            label: Some("Test Buffer"),
        });
        builder.read_buffer(&self.output, BufferReadUsage::Uniform);
    }

    fn execute(&mut self, _ctx: &mut GraphicsCtx<'_, '_, '_, '_>) {}
}

impl GraphicsPass for ExistingBufferWritePass {
    fn setup(&mut self, builder: &mut GraphicsPassBuilder<'_, '_>) {
        builder.write_buffer(&self.output);
    }

    fn execute(&mut self, _ctx: &mut GraphicsCtx<'_, '_, '_, '_>) {}
}

impl GraphicsPass for BufferReadPass {
    fn setup(&mut self, builder: &mut GraphicsPassBuilder<'_, '_>) {
        builder.read_buffer(&self.input, BufferReadUsage::Uniform);
    }

    fn execute(&mut self, _ctx: &mut GraphicsCtx<'_, '_, '_, '_>) {}
}

impl GraphicsPass for InlineLoadPass {
    fn setup(&mut self, builder: &mut GraphicsPassBuilder<'_, '_>) {
        builder.set_graphics_merge_policy(GraphicsPassMergePolicy::Mergeable);
        let target = builder
            .texture_target(&self.target)
            .expect("inline test output should have texture target");
        let _ = target;
        builder.write_color(&self.target, GraphicsColorAttachmentOps::load());
    }

    fn execute(&mut self, _ctx: &mut GraphicsCtx<'_, '_, '_, '_>) {}
}

impl GraphicsPass for DepthStencilWritePass {
    fn setup(&mut self, builder: &mut GraphicsPassBuilder<'_, '_>) {
        let target = builder
            .texture_target(&self.target)
            .expect("depth/stencil target should exist");
        builder.write_depth(target, AttachmentLoadOp::Clear, Some(1.0));
        builder.write_stencil(target, AttachmentLoadOp::Clear, Some(0));
    }

    fn execute(&mut self, _ctx: &mut GraphicsCtx<'_, '_, '_, '_>) {}
}

impl GraphicsPass for DepthStencilReadPass {
    fn setup(&mut self, builder: &mut GraphicsPassBuilder<'_, '_>) {
        let target = builder
            .texture_target(&self.target)
            .expect("depth/stencil target should exist");
        builder.read_depth(target);
        builder.read_stencil(target);
    }

    fn execute(&mut self, _ctx: &mut GraphicsCtx<'_, '_, '_, '_>) {}
}

impl ComputePass for ComputeStubPass {
    fn setup(&mut self, _builder: &mut ComputePassBuilder<'_, '_>) {}

    fn execute(&mut self, _ctx: &mut ComputeCtx<'_, '_, '_, '_>) {}
}

impl TransferPass for TransferStubPass {
    fn setup(&mut self, _builder: &mut TransferPassBuilder<'_, '_>) {}

    fn execute(&mut self, _ctx: &mut TransferCtx<'_, '_, '_>) {}
}

impl GraphicsPass for PersistentInternalPass {
    fn setup(&mut self, builder: &mut GraphicsPassBuilder<'_, '_>) {
        self.output = builder.create_texture_internal(
            test_texture_desc(),
            ResourceLifetime::Persistent,
            Some(0xCAFE),
        );
        let target = builder
            .texture_target(&self.output)
            .expect("persistent output should have texture target");
        let _ = target;
        builder.write_color(
            &self.output,
            GraphicsColorAttachmentOps::clear([0.0, 0.0, 0.0, 0.0]),
        );
    }

    fn execute(&mut self, _ctx: &mut GraphicsCtx<'_, '_, '_, '_>) {}
}

fn test_texture_desc() -> TextureDesc {
    TextureDesc::new(
        1,
        1,
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureDimension::D2,
    )
}

fn test_depth_stencil_texture_desc() -> TextureDesc {
    TextureDesc::new(
        1,
        1,
        wgpu::TextureFormat::Depth24PlusStencil8,
        wgpu::TextureDimension::D2,
    )
}

fn test_buffer_desc() -> BufferDesc {
    BufferDesc {
        size: 16,
        usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
        label: Some("test-buffer"),
    }
}

fn make_present_pass(texture: &OutSlot<TextureResource, ()>) -> PresentSurfacePass {
    PresentSurfacePass::new(
        PresentSurfaceParams,
        PresentSurfaceInput {
            source: RenderTargetIn::with_handle(
                texture.handle().expect("test texture should have handle"),
            ),
        },
        PresentSurfaceOutput,
    )
}

mod execution_plan_tests;
mod pass_timing_tests;
mod persistent_resource_tests;
mod resource_allocation_tests;
mod resource_timeline_tests;
mod topology_cache_tests;
mod version_flow_tests;
