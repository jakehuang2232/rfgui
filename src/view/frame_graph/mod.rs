mod buffer_resource;
pub(crate) mod builder;
mod frame_graph;
mod render_node;
pub(crate) mod slot;
pub(crate) mod texture_resource;

pub use crate::view::render_pass::{ClearPass, DrawRectPass, GraphicsPass};
pub use buffer_resource::{BufferDesc, BufferHandle, BufferResource};
pub(crate) use builder::PassBuilderState;
pub use builder::{BufferReadUsage, ComputePassBuilder, GraphicsPassBuilder, TransferPassBuilder};
pub use frame_graph::{
    AllocationClass, AllocationId, AllocationOwner, AllocationPlan, AttachmentLoadOp,
    AttachmentStoreOp, AttachmentTarget, BufferAllocationPlanEntry, CompiledExecuteStep,
    CompiledGraph, CompiledPass, CompiledResource, ComputePassDescriptor, ComputeRecordContext,
    ExecutionPlan, ExternalAllocationPlanEntry, ExternalResource, ExternalSink, ExternalSinkId,
    ExternalSinkKind, ExternalSinkTarget, FrameGraph, FrameGraphError, FrameResourceContext,
    GraphicsColorAttachmentDescriptor, GraphicsDepthAspectDescriptor,
    GraphicsDepthStencilAttachmentDescriptor, GraphicsPassDescriptor, GraphicsPassMergePolicy,
    GraphicsPipelineRequirements, GraphicsRecordContext, GraphicsStencilAspectDescriptor,
    PassDescriptor, PassDetails, PassHandle, PassKind, PassResourceUsage, PrepareContext,
    RecordContext, RenderPassCompatibilityKey, RenderPassGroup, ResourceAccess, ResourceCache,
    ResourceHandle, ResourceKind, ResourceLifetime, ResourceMetadata, ResourceUsage,
    SampleCountPolicy, ScissorPolicy, TextureAllocationPlanEntry, TransferPassDescriptor,
    TransferRecordContext, ViewportPolicy,
};
pub use slot::{InSlot, OutSlot};
pub use texture_resource::TextureDesc;
