//! Web-platform surface target. Only compiled on `wasm32`.
//!
//! Provides a `HasWindowHandle` / `HasDisplayHandle` shim around
//! `HtmlCanvasElement` so the rendering viewport can take the generic
//! `SurfaceTarget` trait without knowing anything about `web_sys`.
//!
//! User code on wasm constructs a `WebCanvasSurfaceTarget` and passes it to
//! `Viewport::attach`. The viewport itself never names `HtmlCanvasElement`.

#![cfg(target_arch = "wasm32")]

use wasm_bindgen::JsValue;
use web_sys::HtmlCanvasElement;
use wgpu::rwh::{
    DisplayHandle as BorrowedDisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle,
    RawDisplayHandle, RawWindowHandle, WebCanvasWindowHandle, WebDisplayHandle,
    WindowHandle as BorrowedWindowHandle,
};

/// Wraps an `HtmlCanvasElement` as a wgpu-compatible surface target.
///
/// Implements `HasWindowHandle` + `HasDisplayHandle` via the
/// `WebCanvas` raw-window-handle variant; `unsafe` borrows below are
/// sound because the canvas pointer is held alive by `self` for the
/// duration of the borrow.
pub struct WebCanvasSurfaceTarget {
    canvas: HtmlCanvasElement,
}

impl WebCanvasSurfaceTarget {
    pub fn new(canvas: HtmlCanvasElement) -> Self {
        Self { canvas }
    }

    pub fn canvas(&self) -> &HtmlCanvasElement {
        &self.canvas
    }
}

impl HasWindowHandle for WebCanvasSurfaceTarget {
    fn window_handle(&self) -> Result<BorrowedWindowHandle<'_>, HandleError> {
        let value: &JsValue = self.canvas.as_ref();
        let raw = RawWindowHandle::WebCanvas(WebCanvasWindowHandle::new(
            std::ptr::NonNull::from(value).cast(),
        ));
        Ok(unsafe { BorrowedWindowHandle::borrow_raw(raw) })
    }
}

impl HasDisplayHandle for WebCanvasSurfaceTarget {
    fn display_handle(&self) -> Result<BorrowedDisplayHandle<'_>, HandleError> {
        Ok(unsafe {
            BorrowedDisplayHandle::borrow_raw(RawDisplayHandle::Web(WebDisplayHandle::new()))
        })
    }
}
