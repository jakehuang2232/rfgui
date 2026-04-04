//! `rfgui` is a retained-mode GUI framework for Rust built around a typed style system,
//! an RSX authoring model, and a frame-graph-driven renderer.
//!
//! The crate re-exports commonly used style and view APIs at the crate root.

extern crate self as rfgui;

mod style;
pub mod time {
    pub use std::time::Duration;
    #[cfg(not(target_arch = "wasm32"))]
    pub use std::time::Instant;
    #[cfg(target_arch = "wasm32")]
    pub use web_time::Instant;
}
/// Transition and animation primitives used by the retained UI runtime.
pub mod transition;
/// RSX authoring, component, state, event, and reconciliation APIs.
pub mod ui;
/// Viewport integration, built-in host tags, and low-level base components.
pub mod view;

pub use style::*;
pub use view::*;
