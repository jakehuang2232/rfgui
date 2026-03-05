mod buffer_resource;
pub(crate) mod builder;
mod dependency_resource;
mod frame_graph;
mod render_node;
pub(crate) mod slot;
pub(crate) mod texture_resource;

pub use crate::view::render_pass::{ClearPass, DrawRectPass, RenderPass};
pub use buffer_resource::{BufferDesc, BufferHandle, BufferResource};
pub use dependency_resource::{DepHandle, DepIn, DepOut, DepResource};
pub use frame_graph::{
    FrameGraph, FrameGraphError, PassContext, PassHandle, ResourceCache, ResourceHandle,
};
pub use slot::{InSlot, OutSlot};
pub use texture_resource::TextureDesc;
