//! Web (wasm32) — `Intl.Segmenter` for word/grapheme segmentation.
//!
//! Modern browsers ship ICU-backed Unicode segmentation under
//! `Intl.Segmenter`, available since:
//! - Chrome 87 (2020-11)
//! - Safari 14.1 (2021-04)
//! - Firefox 125 (2024-04)
//!
//! Each segment object exposes `index`, its start as a UTF-16 code-unit
//! offset into the input. Segments are contiguous, so their starts plus the
//! input length are the boundaries; they map back to char or byte indices
//! via shared lookup tables. Browsers do not expose line segmentation via
//! `Intl.Segmenter`, so line boundaries use the Unicode fallback.

use js_sys::{Object, Reflect};
use wasm_bindgen::JsValue;
use wasm_bindgen::prelude::*;

use crate::fallback::UnicodeSegmenter;
use crate::{
    GraphemeSegmenter, LineSegmenter, WordSegmenter as WordSegmenterTrait, build_utf16_to_byte_map,
    build_utf16_to_char_map,
};

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = Intl, js_name = Segmenter)]
    type RawIntlSegmenter;

    // A constructor names its class with `js_class`; `js_name` would leave
    // the Rust type name in the generated `new Intl.RawIntlSegmenter`.
    #[wasm_bindgen(catch, constructor, js_namespace = Intl, js_class = "Segmenter")]
    fn new(locales: &JsValue, options: &JsValue) -> Result<RawIntlSegmenter, JsValue>;

    #[wasm_bindgen(method, structural, js_class = "Segmenter")]
    fn segment(this: &RawIntlSegmenter, input: &str) -> JsValue;
}

pub struct IntlSegmenterAdapter {
    word: Option<RawIntlSegmenter>,
    grapheme: Option<RawIntlSegmenter>,
}

impl IntlSegmenterAdapter {
    pub fn new() -> Self {
        Self {
            word: new_segmenter("word"),
            grapheme: new_segmenter("grapheme"),
        }
    }
}

impl Default for IntlSegmenterAdapter {
    fn default() -> Self {
        Self::new()
    }
}

/// Backward-compatible alias.
pub type IntlSegmenter = IntlSegmenterAdapter;

impl WordSegmenterTrait for IntlSegmenterAdapter {
    fn word_boundaries_char_indices(&self, text: &str) -> Vec<usize> {
        intl_boundaries_char_indices(self.word.as_ref(), text)
    }

    fn word_boundaries_byte_indices(&self, text: &str) -> Vec<usize> {
        intl_boundaries_byte_indices(self.word.as_ref(), text)
    }
}

impl LineSegmenter for IntlSegmenterAdapter {
    fn line_boundaries_char_indices(&self, text: &str) -> Vec<usize> {
        UnicodeSegmenter::new().line_boundaries_char_indices(text)
    }

    fn line_boundaries_byte_indices(&self, text: &str) -> Vec<usize> {
        UnicodeSegmenter::new().line_boundaries_byte_indices(text)
    }
}

impl GraphemeSegmenter for IntlSegmenterAdapter {
    fn grapheme_boundaries_char_indices(&self, text: &str) -> Vec<usize> {
        intl_boundaries_char_indices(self.grapheme.as_ref(), text)
    }

    fn grapheme_boundaries_byte_indices(&self, text: &str) -> Vec<usize> {
        intl_boundaries_byte_indices(self.grapheme.as_ref(), text)
    }
}

fn new_segmenter(granularity: &str) -> Option<RawIntlSegmenter> {
    // `undefined` locale -> user's default (browser picks).
    let opts = Object::new();
    let _ = Reflect::set(
        &opts,
        &JsValue::from_str("granularity"),
        &JsValue::from_str(granularity),
    );
    RawIntlSegmenter::new(&JsValue::UNDEFINED, &opts.into()).ok()
}

fn intl_boundaries_char_indices(seg: Option<&RawIntlSegmenter>, text: &str) -> Vec<usize> {
    intl_boundaries(
        seg,
        text,
        &build_utf16_to_char_map(text),
        text.chars().count(),
    )
}

fn intl_boundaries_byte_indices(seg: Option<&RawIntlSegmenter>, text: &str) -> Vec<usize> {
    intl_boundaries(seg, text, &build_utf16_to_byte_map(text), text.len())
}

fn intl_boundaries(
    seg: Option<&RawIntlSegmenter>,
    text: &str,
    utf16_to_index: &[usize],
    total: usize,
) -> Vec<usize> {
    if text.is_empty() {
        return vec![0];
    }

    let Some(seg) = seg else {
        return vec![0, total];
    };

    let segments_js = seg.segment(text);

    // `Intl.Segmenter.segment(...)` returns a `Segments` object; `try_iter`
    // gives us a Rust iterator over the JS iterator protocol.
    let iter = match js_sys::try_iter(&segments_js) {
        Ok(Some(it)) => it,
        _ => return vec![0, total],
    };

    let mut out = Vec::new();
    out.push(0usize);

    // Segments are contiguous: each one ends where the next starts, and the
    // last at the end of the text. Reading only `index`, with its key made
    // once, keeps each segment to one number crossing into wasm instead of
    // two key strings out and the segment's text back in.
    let index_key = JsValue::from_str("index");
    for item in iter {
        let Ok(obj) = item else { continue };
        let Some(start_utf16) = Reflect::get(&obj, &index_key).ok().and_then(|v| v.as_f64()) else {
            continue;
        };
        let end = utf16_to_index
            .get(start_utf16 as usize)
            .copied()
            .unwrap_or(total);
        if Some(&end) != out.last() {
            out.push(end);
        }
    }

    if out.last() != Some(&total) {
        out.push(total);
    }
    out
}
