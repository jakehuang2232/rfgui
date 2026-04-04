use glyphon::FontSystem;
#[cfg(target_arch = "wasm32")]
use glyphon::fontdb;
#[cfg(target_arch = "wasm32")]
use std::sync::Arc;

#[cfg(target_arch = "wasm32")]
const WASM_FALLBACK_FONT_BYTES: &[u8] = include_bytes!("../../assets/NotoSans-Regular.ttf");

pub(crate) fn create_font_system() -> FontSystem {
    #[cfg(target_arch = "wasm32")]
    {
        let mut db = fontdb::Database::new();
        db.load_font_source(fontdb::Source::Binary(Arc::new(
            WASM_FALLBACK_FONT_BYTES.to_vec(),
        )));
        db.set_sans_serif_family("Noto Sans");
        db.set_serif_family("Noto Sans");
        db.set_monospace_family("Noto Sans");
        return FontSystem::new_with_locale_and_db(String::from("en-US"), db);
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        FontSystem::new()
    }
}
