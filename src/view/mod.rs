//! Public view-layer APIs, including host tags, low-level base components, and the viewport.

#[allow(missing_docs)]
pub mod base_component;
pub(crate) mod compositor;
pub mod debug;
pub mod fiber_work;
pub(crate) mod font_system;
pub mod frame_graph;
pub mod host_element;
pub(crate) mod image_resource;
pub(crate) mod inline_formatting_context;
pub(crate) mod inline_text_pass_adapter;
pub(crate) mod layout;
pub mod node_arena;
pub(crate) mod paint;
pub mod popup_stack;
/// Layer promotion analysis and configuration APIs.
pub mod promotion;
mod promotion_builder;
pub(crate) mod raster_cost;
pub mod render_pass;
mod renderer_adapter;
#[cfg(test)]
mod renderer_adapter_tests;
pub(crate) mod sampled_texture;
pub(crate) mod svg_resource;
mod tags;
/// The retained viewport runtime and platform-facing integration surface.
pub mod viewport;

#[cfg(test)]
pub(crate) mod test_support;

pub use debug::DebugType;
#[cfg(target_arch = "wasm32")]
pub use font_system::load_browser_fonts;
#[cfg(target_arch = "wasm32")]
pub use font_system::load_web_font_from_url;
pub use font_system::register_font_bytes;
pub use font_system::set_default_font_families;
pub use host_element::{
    BuildCtx, HostBuilder, HostElementDescBox, erased_host_builder, host_builder_descriptor,
    host_builder_node, host_builder_of,
};
pub use node_arena::{NodeArena, NodeKey, NodeRef, ViewportRef};
pub use renderer_adapter::{
    ElementDescriptor, commit_descriptor_tree, rsx_to_descriptors_with_context,
};
pub use tags::*;
pub use viewport::*;
