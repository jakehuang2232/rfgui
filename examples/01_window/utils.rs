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
