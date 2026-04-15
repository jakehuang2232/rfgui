//! Web backend helpers for wasm hosts.
//!
//! Contains:
//! - `cursor_to_css_name` — maps rfgui's `Cursor` enum to CSS cursor names
//! - `CanvasCursorSink` — applies the cursor to an `HtmlCanvasElement`'s
//!   inline style via `canvas.style().set_property("cursor", …)`
//! - `InMemoryClipboard` — a pure-memory `Clipboard` impl; real browser
//!   clipboard access is async and outside the sync `Clipboard` trait, so
//!   hosts that need real clipboard integration drive it from JS and push
//!   reads back via `Viewport::set_clipboard_fallback`.
//!
//! Cursor and canvas glue lives here (not in `desktop_backend`) because it
//! is the minimum wasm-only code we need — the viewport itself still has
//! zero wasm references.

#![cfg(target_arch = "wasm32")]

use super::{Clipboard, CursorSink};
use crate::Cursor;
use web_sys::HtmlCanvasElement;

/// Return the CSS `cursor` property value for a given `Cursor`. Keeps the
/// mapping in one place so backends, docs, and tests agree.
pub fn cursor_to_css_name(cursor: Cursor) -> &'static str {
    match cursor {
        Cursor::Default => "default",
        Cursor::ContextMenu => "context-menu",
        Cursor::Help => "help",
        Cursor::Pointer => "pointer",
        Cursor::Progress => "progress",
        Cursor::Wait => "wait",
        Cursor::Cell => "cell",
        Cursor::Crosshair => "crosshair",
        Cursor::Text => "text",
        Cursor::VerticalText => "vertical-text",
        Cursor::Alias => "alias",
        Cursor::Copy => "copy",
        Cursor::Move => "move",
        Cursor::NoDrop => "no-drop",
        Cursor::NotAllowed => "not-allowed",
        Cursor::Grab => "grab",
        Cursor::Grabbing => "grabbing",
        Cursor::EResize => "e-resize",
        Cursor::NResize => "n-resize",
        Cursor::NeResize => "ne-resize",
        Cursor::NwResize => "nw-resize",
        Cursor::SResize => "s-resize",
        Cursor::SeResize => "se-resize",
        Cursor::SwResize => "sw-resize",
        Cursor::WResize => "w-resize",
        Cursor::EwResize => "ew-resize",
        Cursor::NsResize => "ns-resize",
        Cursor::NeswResize => "nesw-resize",
        Cursor::NwseResize => "nwse-resize",
        Cursor::ColResize => "col-resize",
        Cursor::RowResize => "row-resize",
        Cursor::AllScroll => "all-scroll",
        Cursor::ZoomIn => "zoom-in",
        Cursor::ZoomOut => "zoom-out",
        Cursor::DndAsk => "alias",
        Cursor::AllResize => "move",
    }
}

/// CursorSink that writes the cursor as a CSS property on a canvas element.
pub struct CanvasCursorSink {
    canvas: HtmlCanvasElement,
}

impl CanvasCursorSink {
    pub fn new(canvas: HtmlCanvasElement) -> Self {
        Self { canvas }
    }
}

impl CursorSink for CanvasCursorSink {
    fn set_cursor(&mut self, cursor: Cursor) {
        let _ = self
            .canvas
            .style()
            .set_property("cursor", cursor_to_css_name(cursor));
    }
}

/// Pure in-memory clipboard for wasm hosts.
///
/// The browser clipboard API is asynchronous and permission-gated, which
/// does not fit the sync `Clipboard` trait. Hosts that want real clipboard
/// support should listen to JS paste/copy events, push the resulting text
/// into `Viewport::set_clipboard_fallback`, and drain
/// `PlatformRequests::clipboard_write` into a JS call after each frame.
#[derive(Default)]
pub struct InMemoryClipboard {
    buf: Option<String>,
}

impl Clipboard for InMemoryClipboard {
    fn get(&mut self) -> Option<String> {
        self.buf.clone()
    }
    fn set(&mut self, text: &str) {
        self.buf = Some(text.to_string());
    }
}
