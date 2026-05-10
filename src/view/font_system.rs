use std::cell::RefCell;

#[cfg(target_arch = "wasm32")]
use js_sys::Uint8Array;
use parley::fontique::{Blob, GenericFamily};
use parley::{FontContext as ParleyFontContext, LayoutContext as ParleyLayoutContext};
use std::sync::Arc;
use std::sync::Mutex;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::JsFuture;

#[cfg(target_arch = "wasm32")]
const WASM_FALLBACK_FONT_BYTES: &[u8] = include_bytes!("../../assets/NotoSans-Regular.ttf");

static RUNTIME_FONTS: Mutex<Vec<Arc<Vec<u8>>>> = Mutex::new(Vec::new());

thread_local! {
    static SHARED_PARLEY_CONTEXT: RefCell<ParleyTextContext> =
        RefCell::new(create_parley_text_context());
}

pub(crate) struct ParleyTextContext {
    pub(crate) font: ParleyFontContext,
    pub(crate) layout: ParleyLayoutContext,
}

fn create_parley_text_context() -> ParleyTextContext {
    let mut ctx = ParleyTextContext {
        font: ParleyFontContext::new(),
        layout: ParleyLayoutContext::new(),
    };
    if let Ok(runtime_fonts) = RUNTIME_FONTS.lock() {
        for font in runtime_fonts.iter() {
            ctx.font
                .collection
                .register_fonts(Blob::new(font.clone()), None);
        }
    }

    #[cfg(target_arch = "wasm32")]
    {
        ctx.font.collection.register_fonts(
            Blob::from(WASM_FALLBACK_FONT_BYTES.to_vec()),
            Some(parley::fontique::FontInfoOverride {
                family_name: Some("Noto Sans"),
                ..Default::default()
            }),
        );
        set_parley_default_font_families(&mut ctx, "Noto Sans", "Noto Sans", "Noto Sans");
    }

    ctx
}

pub(crate) fn with_shared_parley_context<R>(f: impl FnOnce(&mut ParleyTextContext) -> R) -> R {
    SHARED_PARLEY_CONTEXT.with(|slot| {
        let mut ctx = slot.borrow_mut();
        f(&mut ctx)
    })
}

pub fn register_font_bytes(bytes: &[u8]) -> bool {
    let font = Arc::new(bytes.to_vec());
    let inserted = {
        let Ok(mut fonts) = RUNTIME_FONTS.lock() else {
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
        with_shared_parley_context(|ctx| {
            ctx.font
                .collection
                .register_fonts(Blob::new(font.clone()), None);
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
    with_shared_parley_context(|ctx| {
        set_parley_default_font_families(ctx, sans_serif, serif, monospace);
    });
}

fn set_parley_default_font_families(
    ctx: &mut ParleyTextContext,
    sans_serif: &str,
    serif: &str,
    monospace: &str,
) {
    if let Some(id) = ctx.font.collection.family_id(sans_serif) {
        ctx.font
            .collection
            .set_generic_families(GenericFamily::SansSerif, [id].into_iter());
    }
    if let Some(id) = ctx.font.collection.family_id(serif) {
        ctx.font
            .collection
            .set_generic_families(GenericFamily::Serif, [id].into_iter());
    }
    if let Some(id) = ctx.font.collection.family_id(monospace) {
        ctx.font
            .collection
            .set_generic_families(GenericFamily::Monospace, [id].into_iter());
    }
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
