#[cfg(not(target_arch = "wasm32"))]
mod handler_panic;
#[cfg(not(target_arch = "wasm32"))]
mod render_panic;

use crate::rfgui::ui::{RsxNode, component, rsx};

#[cfg(not(target_arch = "wasm32"))]
#[component]
pub fn ErrorBoundaryPage() -> RsxNode {
    use handler_panic::HandlerPanic;
    use render_panic::RenderPanic;
    rsx! {
        <RenderPanic />
        <HandlerPanic />
    }
}

/// Error boundaries catch panics by unwinding, and WebAssembly builds abort
/// on a panic instead.
#[cfg(target_arch = "wasm32")]
#[component]
pub fn ErrorBoundaryPage() -> RsxNode {
    use crate::showcase::Demo;
    rsx! {
        <Demo
            title="Desktop only"
            description="Error boundaries catch a panic by unwinding the stack, and a WebAssembly build aborts on a panic instead, so this page would stop the whole demo here. Run the desktop example to try it: cargo run -r -p examples --bin 01_window"
        />
    }
}
