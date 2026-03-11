mod buffer_resource;
pub(crate) mod builder;
mod frame_graph;
mod render_node;
pub(crate) mod slot;
pub(crate) mod texture_resource;

pub use crate::view::render_pass::{ClearPass, DrawRectPass, RenderPass};
pub use buffer_resource::{BufferDesc, BufferHandle, BufferResource};
pub use builder::PassBuilder;
pub use frame_graph::{
    AllocationClass, AllocationId, AllocationOwner, AllocationPlan, AttachmentLoadOp,
    AttachmentStoreOp, AttachmentTarget, BufferAllocationPlanEntry, CompiledExecuteStep,
    CompiledGraph, CompiledPass, CompiledResource, ComputePassDescriptor, ExecutionPlan,
    ExternalAllocationPlanEntry, ExternalResource, FrameGraph, FrameGraphError,
    FrameResourceContext, GraphicsColorAttachmentDescriptor, GraphicsDepthAspectDescriptor,
    GraphicsDepthStencilAttachmentDescriptor, GraphicsPassDescriptor,
    GraphicsPipelineRequirements, GraphicsRecordContext, GraphicsStencilAspectDescriptor,
    PassDetails, PassDescriptor, PassHandle, PassKind, PassResourceUsage, PrepareContext,
    RecordContext, ResourceAccess, ResourceCache, ResourceHandle, ResourceKind,
    ResourceLifetime, ResourceMetadata, ResourceUsage, SampleCountPolicy, ScissorPolicy,
    TextureAllocationPlanEntry, TransferPassDescriptor, ViewportPolicy,
};
pub use slot::{InSlot, OutSlot};
pub use texture_resource::TextureDesc;
