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
    AttachmentStoreOp, AttachmentTarget, BufferAllocationPlanEntry, BufferResourceState,
    BufferVersionId, CompileGraphProfile, CompileProfile, CompiledGraph, CompiledPass,
    CompiledPassResourceTransition, CompiledResource, CompiledResourceTimeline,
    CompiledResourceTransition, ComputePassDescriptor, ComputeRecordContext, ConsumedVersion,
    ExternalAllocationPlanEntry, ExternalResource, ExternalSink, ExternalSinkId, ExternalSinkKind,
    ExternalSinkTarget, FrameGraph, FrameGraphError, FrameResourceContext,
    GraphicsColorAttachmentDescriptor, GraphicsColorAttachmentOps, GraphicsDepthAspectDescriptor,
    GraphicsDepthStencilAttachmentDescriptor, GraphicsPassDescriptor, GraphicsPassMergePolicy,
    GraphicsPipelineRequirements, GraphicsRecordContext, GraphicsStencilAspectDescriptor,
    PassDescriptor, PassDetails, PassHandle, PassKind, PassResourceUsage, PrepareContext,
    ProducedVersion, RecordContext, ResourceAccess, ResourceCache, ResourceHandle, ResourceKind,
    ResourceLifetime, ResourceMetadata, ResourceState, ResourceUsage, ResourceVersionId,
    SampleCountPolicy, ScissorPolicy, TextureAllocationPlanEntry, TextureAspectState,
    TextureResourceState, TextureVersionId, TransferPassDescriptor, TransferRecordContext,
    ViewportPolicy,
};
pub use slot::{InSlot, OutSlot};
pub use texture_resource::TextureDesc;
