//! Public view-layer APIs, including host tags, low-level base components, and the viewport.

#[allow(missing_docs)]
pub mod base_component;
pub mod fiber_work;
pub(crate) mod font_system;
pub mod frame_graph;
pub(crate) mod image_resource;
pub mod node_arena;
/// Layer promotion analysis and configuration APIs.
pub mod promotion;
mod promotion_builder;
pub mod render_pass;
mod renderer_adapter;
pub(crate) mod svg_resource;
mod tags;
pub(crate) mod text_layout;
/// The retained viewport runtime and platform-facing integration surface.
pub mod viewport;

#[cfg(test)]
pub(crate) mod test_support;

#[cfg(target_arch = "wasm32")]
pub use font_system::load_browser_fonts;
#[cfg(target_arch = "wasm32")]
pub use font_system::load_default_web_cjk_font;
#[cfg(target_arch = "wasm32")]
pub use font_system::load_web_font_from_url;
pub use font_system::register_font_bytes;
pub use font_system::set_default_font_families;
pub use node_arena::{NodeArena, NodeKey, NodeRef, ViewportRef};
pub use renderer_adapter::*;
pub use tags::*;
pub use viewport::*;
