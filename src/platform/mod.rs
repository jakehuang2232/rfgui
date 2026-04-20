//! Platform abstraction traits for rfgui.
//!
//! Phase 0 of the viewport decoupling work. Defines the boundary between the
//! rendering/runtime core (`view::viewport`) and any concrete platform backend
//! (winit, web, headless, …). Nothing in this module is allowed to depend on
//! `winit`, `arboard`, `web_sys`, or `wasm_bindgen`.
//!
//! The only platform-shaped dependency allowed here is `raw-window-handle`
//! (re-exported via `wgpu::rwh`), because the wgpu surface creation API is
//! expressed in its terms and is itself cross-platform.

use crate::Cursor;
use wgpu::rwh::{HasDisplayHandle, HasWindowHandle};

pub mod callback;
pub mod headless;
pub mod input;

#[cfg(target_arch = "wasm32")]
pub mod web;

#[cfg(not(target_arch = "wasm32"))]
pub mod desktop_backend;

#[cfg(target_arch = "wasm32")]
pub mod web_backend;

pub use callback::{CallbackCursorSink, CallbackRedrawRequester};
pub use headless::{HeadlessBackend, NullClipboard, NullCursorSink, NullRedrawRequester};
pub use input::{
    PlatformImePreedit, PlatformKeyEvent, PlatformPointerButton, PlatformPointerEvent,
    PlatformPointerEventKind, PlatformTextInput, PlatformWheelEvent, PointerType,
};

/// Anything that can back a `wgpu::Surface`.
///
/// Native windows (winit `Window`), web canvases (wrapped in a
/// `HasWindowHandle`/`HasDisplayHandle` shim), and offscreen test targets all
/// satisfy this trait. The viewport only ever sees `SurfaceTarget`; it never
/// names `winit::window::Window` or `web_sys::HtmlCanvasElement`.
pub trait SurfaceTarget: HasWindowHandle + HasDisplayHandle + Send + Sync {}

impl<T> SurfaceTarget for T where T: HasWindowHandle + HasDisplayHandle + Send + Sync + ?Sized {}

/// Read/write access to the host clipboard.
///
/// Backend-provided. On platforms without a clipboard (headless tests, some
/// wasm contexts) implementors may return `None` from `get` and silently drop
/// writes in `set`.
pub trait Clipboard {
    fn get(&mut self) -> Option<String>;
    fn set(&mut self, text: &str);
}

/// Sink for mouse-cursor changes produced by the viewport.
///
/// The viewport records the desired cursor per frame; the backend is expected
/// to apply it to the native window. Idempotent — the backend is responsible
/// for deduping redundant updates if it cares.
pub trait CursorSink {
    fn set_cursor(&mut self, cursor: Cursor);
}

/// Request that the host schedule another redraw.
///
/// The viewport can mark itself dirty internally; this trait exists so a
/// backend can also be poked externally (e.g. by an animation plugin inside
/// the viewport asking the event loop to wake up on the next vsync).
pub trait RedrawRequester {
    fn request_redraw(&self);
}

/// Bundle of platform capabilities passed into user-facing `App` hooks.
///
/// The viewport itself never stores one of these; it is assembled by the
/// backend once per event/frame and handed down through `AppContext`.
pub struct PlatformServices<'a> {
    pub clipboard: &'a mut dyn Clipboard,
    pub cursor: &'a mut dyn CursorSink,
    pub redraw: &'a dyn RedrawRequester,
}

/// Outbound requests drained from the viewport after a frame or event
/// dispatch. The backend applies these to real platform APIs.
///
/// This replaces the old callback-based `CursorHandler` and the viewport's
/// direct `arboard` ownership. Kept as plain data so the viewport has zero
/// platform coupling on the write path.
#[derive(Debug, Default, Clone)]
pub struct PlatformRequests {
    /// Most-recently-requested cursor, if the viewport wants to change it.
    pub cursor: Option<Cursor>,
    /// Text the viewport wants written to the host clipboard.
    pub clipboard_write: Option<String>,
    /// Whether the viewport wants another redraw scheduled.
    pub request_redraw: bool,
}

impl PlatformRequests {
    pub fn is_empty(&self) -> bool {
        self.cursor.is_none() && self.clipboard_write.is_none() && !self.request_redraw
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_requests_empty_by_default() {
        let r = PlatformRequests::default();
        assert!(r.is_empty());
    }
}
