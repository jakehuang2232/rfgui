use crate::rfgui::{ColorLike, Viewport};
use crate::rfgui_components::Theme;
#[cfg(target_arch = "wasm32")]
use std::sync::Arc;

pub fn should_dispatch_keyboard_text(viewport: &Viewport, text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    if !is_printable_keyboard_text(text) {
        return false;
    }
    if text.chars().any(|ch| ch.is_control()) {
        return false;
    }
    // Keep shortcuts (Ctrl/Alt/Cmd + key) out of text-input path.
    let has_alt = viewport.is_key_pressed("Named(Alt)")
        || viewport.is_key_pressed("Named(AltGraph)")
        || viewport.is_key_pressed("Code(AltLeft)")
        || viewport.is_key_pressed("Code(AltRight)");
    let has_ctrl = viewport.is_key_pressed("Named(Control)")
        || viewport.is_key_pressed("Code(ControlLeft)")
        || viewport.is_key_pressed("Code(ControlRight)");
    let has_meta = viewport.is_key_pressed("Named(Super)")
        || viewport.is_key_pressed("Named(Meta)")
        || viewport.is_key_pressed("Code(SuperLeft)")
        || viewport.is_key_pressed("Code(SuperRight)")
        || viewport.is_key_pressed("Code(MetaLeft)")
        || viewport.is_key_pressed("Code(MetaRight)");
    !(has_alt || has_ctrl || has_meta)
}

#[cfg(target_arch = "wasm32")]
pub fn should_dispatch_web_keyboard_text(viewport: &Viewport, key: &str, code: &str) -> bool {
    if !should_dispatch_keyboard_text(viewport, key) {
        return false;
    }
    !is_non_text_web_key_code(code)
}

fn is_printable_keyboard_text(text: &str) -> bool {
    // Web keydown uses DOM KeyboardEvent.key; named keys like "Delete" or
    // "ArrowLeft" must stay on the keydown path instead of being inserted.
    let mut chars = text.chars();
    let Some(ch) = chars.next() else {
        return false;
    };
    if chars.next().is_some() {
        return false;
    }
    !matches!(ch as u32, 0xF700..=0xF8FF)
}

#[cfg(target_arch = "wasm32")]
fn is_non_text_web_key_code(code: &str) -> bool {
    code.starts_with("Arrow")
        || code.starts_with("F")
        || matches!(
            code,
            "AltLeft"
                | "AltRight"
                | "Backspace"
                | "CapsLock"
                | "ContextMenu"
                | "ControlLeft"
                | "ControlRight"
                | "Delete"
                | "End"
                | "Enter"
                | "Escape"
                | "Help"
                | "Home"
                | "Insert"
                | "MetaLeft"
                | "MetaRight"
                | "NumLock"
                | "PageDown"
                | "PageUp"
                | "Pause"
                | "ScrollLock"
                | "ShiftLeft"
                | "ShiftRight"
                | "Tab"
        )
}

pub fn app_background_color(is_dark: bool) -> Box<dyn ColorLike> {
    if is_dark {
        Theme::dark().color.background.base
    } else {
        Theme::light().color.background.base
    }
}
