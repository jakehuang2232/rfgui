//! Desktop backend helpers for native (non-wasm) hosts.
//!
//! Only ships a clipboard implementation — cursor and redraw plumbing live
//! in `platform::callback` because they're host-specific glue (winit,
//! egui_winit, tao, custom event loops, …) and rfgui must stay winit-free.
//!
//! To assemble a desktop `PlatformServices`, combine `ArboardClipboard` with
//! `CallbackCursorSink` / `CallbackRedrawRequester` from `platform::callback`.

#![cfg(not(target_arch = "wasm32"))]

use super::Clipboard;

/// Clipboard backed by `arboard`. Construction is fallible because the
/// system clipboard may be unavailable on headless CI machines; callers
/// should fall back to `NullClipboard` in that case.
pub struct ArboardClipboard {
    inner: arboard::Clipboard,
}

impl ArboardClipboard {
    pub fn new() -> Option<Self> {
        arboard::Clipboard::new().ok().map(|inner| Self { inner })
    }
}

impl Clipboard for ArboardClipboard {
    fn get(&mut self) -> Option<String> {
        self.inner.get_text().ok()
    }
    fn set(&mut self, text: &str) {
        let _ = self.inner.set_text(text.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arboard_constructor_does_not_panic() {
        // May return None on headless CI; either outcome is fine — we just
        // care that the call itself is sound.
        let _ = ArboardClipboard::new();
    }
}
