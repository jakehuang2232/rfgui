mod buffer_resource;
pub mod builder;
mod frame_graph;
mod render_node;
pub mod slot;
pub mod texture_resource;

pub(crate) use crate::view::render_pass::ClearPass;
pub(crate) use buffer_resource::{BufferDesc, BufferResource};
pub(crate) use builder::PassBuilderState;
pub use builder::{BufferReadUsage, ComputePassBuilder, GraphicsPassBuilder, TransferPassBuilder};
pub use frame_graph::{
    AllocationId, AttachmentLoadOp, AttachmentTarget, CacheStatSnapshot, CacheStats,
    CompileProfile, CompiledGraph, ComputeRecordContext, ExternalSinkKind, FrameGraph,
    FrameResourceContext, GraphicsColorAttachmentOps, GraphicsPassMergePolicy,
    GraphicsRecordContext, PrepareContext, ResourceCache, ResourceLifetime, SampleCountPolicy,
    TransferRecordContext, dump_cache_stats, register_cache_stats,
};
pub use texture_resource::TextureDesc;
