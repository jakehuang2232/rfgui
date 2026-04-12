use crate::rfgui::view::ImageSource;
use crate::rfgui::{ColorLike, Viewport};
use crate::rfgui_components::Theme;
#[cfg(target_arch = "wasm32")]
use std::sync::Arc;

#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;
#[cfg(not(target_arch = "wasm32"))]
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(not(target_arch = "wasm32"))]
pub fn current_unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

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

#[cfg(not(target_arch = "wasm32"))]
pub fn output_asset_path(file_name: &str) -> PathBuf {
    let executable = std::env::current_exe().expect("failed to resolve current executable path");
    executable
        .parent()
        .expect("failed to resolve executable directory")
        .join("assets")
        .join(file_name)
}

pub fn output_image_source(file_name: &str) -> ImageSource {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let path = output_asset_path(file_name);
        return ImageSource::Path(path);
    }

    #[cfg(target_arch = "wasm32")]
    {
        let bytes = match file_name {
            "rfgui-logo.png" => include_bytes!("../assets/rfgui-logo.png").as_slice(),
            "test.png" => include_bytes!("../assets/test.png").as_slice(),
            other => panic!("unsupported embedded asset: {other}"),
        };
        let decoded = image::load_from_memory(bytes).expect("failed to decode embedded image");
        let rgba = decoded.to_rgba8();
        let (width, height) = rgba.dimensions();
        return ImageSource::Rgba {
            width,
            height,
            pixels: Arc::<[u8]>::from(rgba.into_raw()),
        };
    }
}

#[cfg(test)]
mod tests {
    #[cfg(target_arch = "wasm32")]
    use super::is_non_text_web_key_code;
    use super::is_printable_keyboard_text;

    #[test]
    fn accepts_single_character_text_keys() {
        assert!(is_printable_keyboard_text("a"));
        assert!(is_printable_keyboard_text(" "));
        assert!(is_printable_keyboard_text("中"));
    }

    #[test]
    fn rejects_named_function_keys() {
        assert!(!is_printable_keyboard_text("Delete"));
        assert!(!is_printable_keyboard_text("ArrowLeft"));
        assert!(!is_printable_keyboard_text("Enter"));
        assert!(!is_printable_keyboard_text("Tab"));
    }

    #[test]
    fn rejects_private_use_function_key_chars() {
        assert!(!is_printable_keyboard_text("\u{F728}"));
        assert!(!is_printable_keyboard_text("\u{F72C}"));
    }

    #[cfg(target_arch = "wasm32")]
    #[test]
    fn rejects_non_text_web_codes() {
        assert!(is_non_text_web_key_code("Delete"));
        assert!(is_non_text_web_key_code("ArrowLeft"));
        assert!(is_non_text_web_key_code("Enter"));
        assert!(is_non_text_web_key_code("F5"));
        assert!(!is_non_text_web_key_code("KeyA"));
        assert!(!is_non_text_web_key_code("Digit1"));
        assert!(!is_non_text_web_key_code("Space"));
    }
}
