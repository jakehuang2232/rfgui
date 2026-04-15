//! Public view-layer APIs, including host tags, low-level base components, and the viewport.

#[allow(missing_docs)]
pub mod base_component;
pub(crate) mod font_system;
pub(crate) mod frame_graph;
pub(crate) mod image_resource;
/// Layer promotion analysis and configuration APIs.
pub mod promotion;
mod promotion_builder;
pub(crate) mod render_pass;
mod renderer_adapter;
pub(crate) mod svg_resource;
mod tags;
pub(crate) mod text_layout;
/// The retained viewport runtime and platform-facing integration surface.
pub mod viewport;

#[cfg(target_arch = "wasm32")]
pub use font_system::load_browser_fonts;
#[cfg(target_arch = "wasm32")]
pub use font_system::load_default_web_cjk_font;
#[cfg(target_arch = "wasm32")]
pub use font_system::load_web_font_from_url;
pub use font_system::register_font_bytes;
pub use font_system::set_default_font_families;
pub use renderer_adapter::*;
pub use tags::*;
pub use viewport::*;
