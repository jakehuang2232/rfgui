use std::cell::RefCell;

use cosmic_text::FontSystem;
use cosmic_text::fontdb;
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
const WEB_CJK_FONT_FAMILY: &str = "Noto Sans CJK TC";
#[cfg(target_arch = "wasm32")]
const WEB_CJK_FONT_URL: &str = "https://raw.githubusercontent.com/notofonts/noto-cjk/main/Sans/OTF/TraditionalChinese/NotoSansCJKtc-Regular.otf";

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
        if let Ok(runtime_fonts) = RUNTIME_WEB_FONTS.lock() {
            for font in runtime_fonts.iter() {
                db.load_font_source(fontdb::Source::Binary(font.clone()));
            }
        }
        db.set_sans_serif_family("Noto Sans");
        db.set_serif_family("Noto Sans");
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

/// Update the default font family mappings on the shared font system.
///
/// On WASM this is essential after loading custom fonts so that generic
/// family names (sans-serif, serif, monospace) resolve to the newly
/// registered typefaces.
pub fn set_default_font_families(sans_serif: &str, serif: &str, monospace: &str) {
    with_shared_font_system(|font_system| {
        let db = font_system.db_mut();
        db.set_sans_serif_family(sans_serif);
        db.set_serif_family(serif);
        db.set_monospace_family(monospace);
    });
}

// ---------------------------------------------------------------------------
// WASM-only font loading helpers
// ---------------------------------------------------------------------------

#[cfg(target_arch = "wasm32")]
async fn fetch_font_bytes(url: &str) -> Result<Vec<u8>, wasm_bindgen::JsValue> {
    let window =
        web_sys::window().ok_or_else(|| wasm_bindgen::JsValue::from_str("window not available"))?;
    let response_value = JsFuture::from(window.fetch_with_str(url)).await?;
    let response: web_sys::Response = response_value.dyn_into()?;
    if !response.ok() {
        return Err(wasm_bindgen::JsValue::from_str(&format!(
            "failed to fetch font from {url}"
        )));
    }
    let buffer = JsFuture::from(response.array_buffer()?).await?;
    Ok(Uint8Array::new(&buffer).to_vec())
}

/// Fetch a font file from `url` and register it into the engine's font
/// database. Returns `true` if a new font was added (not a duplicate).
#[cfg(target_arch = "wasm32")]
pub async fn load_web_font_from_url(url: &str) -> Result<bool, wasm_bindgen::JsValue> {
    let bytes = fetch_font_bytes(url).await?;
    Ok(register_font_bytes(&bytes))
}

/// Load fonts already present in the browser's `document.fonts`
/// ([`FontFaceSet`]) by re-fetching their source URLs.
///
/// Iterates every `FontFace` whose `src` descriptor contains a `url(…)`
/// value, fetches the first URL found, and registers the binary data.
/// Fonts with only `local()` sources or unparseable descriptors are
/// silently skipped.
///
/// Returns the number of newly registered fonts.
#[cfg(target_arch = "wasm32")]
pub async fn load_browser_fonts() -> Result<usize, wasm_bindgen::JsValue> {
    let document = web_sys::window()
        .and_then(|w| w.document())
        .ok_or_else(|| wasm_bindgen::JsValue::from_str("document not available"))?;

    let font_face_set = document.fonts();

    // FontFaceSet is iterable — use JS iteration via js_sys.
    let iterator = js_sys::try_iter(&font_face_set)?
        .ok_or_else(|| wasm_bindgen::JsValue::from_str("FontFaceSet is not iterable"))?;

    let mut urls: Vec<String> = Vec::new();
    for entry in iterator {
        let face_val = entry?;
        let face: web_sys::FontFace = face_val.dyn_into()?;
        // FontFace.src is a DOMString with the CSS src descriptor,
        // e.g. `url("https://example.com/font.woff2"), local("Arial")`
        let src: String = js_sys::Reflect::get(&face, &"src".into())?
            .as_string()
            .unwrap_or_default();
        if let Some(url) = extract_first_url(&src) {
            if !urls.contains(&url) {
                urls.push(url);
            }
        }
    }

    let mut count = 0usize;
    for url in &urls {
        match fetch_font_bytes(url).await {
            Ok(bytes) => {
                if register_font_bytes(&bytes) {
                    count += 1;
                }
            }
            Err(_) => continue,
        }
    }
    Ok(count)
}

/// Extract the first `url(...)` value from a CSS `src` descriptor string.
#[cfg(target_arch = "wasm32")]
fn extract_first_url(src: &str) -> Option<String> {
    let start = src.find("url(")?;
    let after_open = start + 4;
    let rest = src.get(after_open..)?;
    // Handle both url("...") and url('...') and url(...)
    let (url, _) = if rest.starts_with('"') {
        let inner = rest.get(1..)?;
        let end = inner.find('"')?;
        (inner.get(..end)?, end + 2)
    } else if rest.starts_with('\'') {
        let inner = rest.get(1..)?;
        let end = inner.find('\'')?;
        (inner.get(..end)?, end + 2)
    } else {
        let end = rest.find(')')?;
        (rest.get(..end)?, end + 1)
    };
    if url.is_empty() {
        return None;
    }
    Some(url.to_string())
}

/// Load the built-in CJK fallback font from a remote URL and configure
/// default font families to use it.
///
/// This is a convenience wrapper around [`load_web_font_from_url`] +
/// [`set_default_font_families`]. No-op if runtime fonts are already
/// registered.
#[cfg(target_arch = "wasm32")]
pub async fn load_default_web_cjk_font() -> Result<(), wasm_bindgen::JsValue> {
    if RUNTIME_WEB_FONTS
        .lock()
        .map(|f| !f.is_empty())
        .unwrap_or(false)
    {
        return Ok(());
    }

    load_web_font_from_url(WEB_CJK_FONT_URL).await?;
    set_default_font_families(WEB_CJK_FONT_FAMILY, WEB_CJK_FONT_FAMILY, "Noto Sans");
    Ok(())
}
