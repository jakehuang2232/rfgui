//! Public view-layer APIs, including host tags, low-level base components, and the viewport.

#[allow(missing_docs)]
pub mod base_component;
pub(crate) mod frame_graph;
pub(crate) mod image_resource;
/// Layer promotion analysis and configuration APIs.
pub mod promotion;
mod promotion_builder;
pub(crate) mod render_pass;
mod renderer_adapter;
pub(crate) mod svg_resource;
mod tags;
/// The retained viewport runtime and platform-facing integration surface.
pub mod viewport;

pub use renderer_adapter::*;
pub use tags::*;
pub use viewport::*;
