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
    AttachmentLoadOp, AttachmentStoreOp, AttachmentTarget, CompiledExecuteStep, CompiledGraph,
    CompiledPass, CompiledResource, ComputePassDescriptor, ExecutionPlan, FrameGraph,
    FrameGraphError, FrameResourceContext, GraphicsColorAttachmentDescriptor,
    GraphicsDepthAspectDescriptor, GraphicsDepthStencilAttachmentDescriptor,
    GraphicsPassDescriptor, GraphicsPipelineRequirements, GraphicsRecordContext,
    GraphicsStencilAspectDescriptor, PassDetails, PassDescriptor, PassHandle, PassKind,
    PassResourceUsage, PrepareContext, RecordContext, ResourceAccess, ResourceCache,
    ResourceHandle, ResourceUsage, SampleCountPolicy, ScissorPolicy, TransferPassDescriptor,
    ViewportPolicy,
};
pub use slot::{InSlot, OutSlot};
pub use texture_resource::TextureDesc;
