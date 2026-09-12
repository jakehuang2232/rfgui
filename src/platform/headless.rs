//! Headless backend: null implementations of every platform trait.
//!
//! Intended for unit tests, offscreen rendering, and any host that has no
//! real window, clipboard, or cursor. Zero external dependencies.

use super::{Clipboard, CursorSink, RedrawRequester};
use crate::style::Cursor;

#[derive(Default)]
pub struct NullClipboard {
    buf: Option<String>,
}

impl Clipboard for NullClipboard {
    fn get(&mut self) -> Option<String> {
        self.buf.clone()
    }
    fn set(&mut self, text: &str) {
        self.buf = Some(text.to_string());
    }
}

#[derive(Default)]
pub struct NullCursorSink;

impl CursorSink for NullCursorSink {
    fn set_cursor(&mut self, _cursor: Cursor) {}
}

#[derive(Default)]
pub struct NullRedrawRequester;

impl RedrawRequester for NullRedrawRequester {
    fn request_redraw(&self) {}
}

/// Convenience bundle: owns all three null services so a test harness can
/// construct a `PlatformServices` handle with one local value.
#[derive(Default)]
pub struct HeadlessBackend {
    pub clipboard: NullClipboard,
    pub cursor: NullCursorSink,
    pub redraw: NullRedrawRequester,
}

#[cfg(test)]
mod tests;
