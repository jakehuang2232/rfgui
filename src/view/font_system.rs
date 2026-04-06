use std::cell::RefCell;

use glyphon::FontSystem;
use glyphon::fontdb;
#[cfg(target_arch = "wasm32")]
use js_sys::Uint8Array;
use std::sync::Arc;
use std::sync::Mutex;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::JsFuture;

#[cfg(target_arch = "wasm32")]
const WASM_FALLBACK_FONT_BYTES: &[u8] = include_bytes!("../../assets/NotoSans-Regular.ttf");
#[cfg(target_arch = "wasm32")]
const WEB_CJK_FONT_FAMILY: &str = "Noto Sans TC";
#[cfg(target_arch = "wasm32")]
const WEB_CJK_FONT_URL: &str = "https://fonts.gstatic.com/ea/notosanstc/v1/NotoSansTC-Regular.otf";

static RUNTIME_WEB_FONTS: Mutex<Vec<Arc<Vec<u8>>>> = Mutex::new(Vec::new());

thread_local! {
    static SHARED_FONT_SYSTEM: RefCell<FontSystem> = RefCell::new(create_font_system());
}

pub(crate) fn create_font_system() -> FontSystem {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut font_system = FontSystem::new();
        if let Ok(runtime_fonts) = RUNTIME_WEB_FONTS.lock() {
            for font in runtime_fonts.iter() {
                font_system
                    .db_mut()
                    .load_font_source(fontdb::Source::Binary(font.clone()));
            }
        }
        return font_system;
    }

    #[cfg(target_arch = "wasm32")]
    {
        let mut db = fontdb::Database::new();
        db.load_font_source(fontdb::Source::Binary(Arc::new(
            WASM_FALLBACK_FONT_BYTES.to_vec(),
        )));
        let has_runtime_fonts = if let Ok(runtime_fonts) = RUNTIME_WEB_FONTS.lock() {
            for font in runtime_fonts.iter() {
                db.load_font_source(fontdb::Source::Binary(font.clone()));
            }
            !runtime_fonts.is_empty()
        } else {
            false
        };
        if has_runtime_fonts {
            db.set_sans_serif_family(WEB_CJK_FONT_FAMILY);
            db.set_serif_family(WEB_CJK_FONT_FAMILY);
        } else {
            db.set_sans_serif_family("Noto Sans");
            db.set_serif_family("Noto Sans");
        }
        db.set_monospace_family("Noto Sans");
        return FontSystem::new_with_locale_and_db(String::from("en-US"), db);
    }
}

pub(crate) fn with_shared_font_system<R>(f: impl FnOnce(&mut FontSystem) -> R) -> R {
    SHARED_FONT_SYSTEM.with(|slot| {
        let mut font_system = slot.borrow_mut();
        f(&mut font_system)
    })
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn reset_shared_font_system() {
    SHARED_FONT_SYSTEM.with(|slot| {
        slot.replace(create_font_system());
    });
}

pub fn register_font_bytes(bytes: &[u8]) -> bool {
    let font = Arc::new(bytes.to_vec());
    let inserted = {
        let Ok(mut fonts) = RUNTIME_WEB_FONTS.lock() else {
            return false;
        };
        if fonts.iter().any(|font| font.as_slice() == bytes) {
            false
        } else {
            fonts.push(font.clone());
            true
        }
    };

    if inserted {
        with_shared_font_system(|font_system| {
            font_system
                .db_mut()
                .load_font_source(fontdb::Source::Binary(font));
        });
    }

    inserted
}

#[cfg(target_arch = "wasm32")]
fn has_runtime_web_fonts() -> bool {
    RUNTIME_WEB_FONTS
        .lock()
        .map(|fonts| !fonts.is_empty())
        .unwrap_or(false)
}

#[cfg(target_arch = "wasm32")]
pub async fn load_default_web_cjk_font() -> Result<(), wasm_bindgen::JsValue> {
    if has_runtime_web_fonts() {
        return Ok(());
    }

    let window =
        web_sys::window().ok_or_else(|| wasm_bindgen::JsValue::from_str("window not available"))?;
    let response_value = JsFuture::from(window.fetch_with_str(WEB_CJK_FONT_URL)).await?;
    let response: web_sys::Response = response_value.dyn_into()?;
    if !response.ok() {
        return Err(wasm_bindgen::JsValue::from_str(
            "failed to fetch web CJK font",
        ));
    }
    let buffer = JsFuture::from(response.array_buffer()?).await?;
    let bytes = Uint8Array::new(&buffer).to_vec();
    let should_reset = {
        let mut fonts = RUNTIME_WEB_FONTS
            .lock()
            .map_err(|_| wasm_bindgen::JsValue::from_str("web font mutex poisoned"))?;
        if fonts.is_empty() {
            fonts.push(Arc::new(bytes));
            true
        } else {
            false
        }
    };
    if should_reset {
        reset_shared_font_system();
    }
    Ok(())
}
