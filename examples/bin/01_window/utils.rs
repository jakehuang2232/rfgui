//! Scene-side asset helpers retained after the winit_runner migration.
//!
//! Everything viewport- or winit-aware that used to live here was pruned
//! when app.rs was retired. Only the asset resolution helpers remain;
//! scene modules (about/render/inline/... tests) still depend on them.

use crate::rfgui::view::ImageSource;

#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;

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
        ImageSource::Path(format!("assets/{file_name}").into())
    }
}
