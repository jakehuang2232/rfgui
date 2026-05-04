//! Web (wasm32) — `Intl.Segmenter` with `granularity: 'word'`.
//!
//! Modern browsers ship ICU-backed Unicode segmentation under
//! `Intl.Segmenter`, available since:
//! - Chrome 87 (2020-11)
//! - Safari 14.1 (2021-04)
//! - Firefox 125 (2024-04)
//!
//! Each segment object exposes `index` (UTF-16 code-unit offset into the
//! input), `segment` (the substring), and `isWordLike` (true for
//! word-shaped tokens, false for punctuation / whitespace runs). We use
//! `index + segment.length(utf16)` as the segment end, then map back to
//! char indices via the shared utf16-to-char table.

use js_sys::{Object, Reflect};
use wasm_bindgen::JsValue;
use wasm_bindgen::prelude::*;

use crate::{WordSegmenter as WordSegmenterTrait, build_utf16_to_char_map};

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = Intl, js_name = Segmenter)]
    type IntlSegmenter;

    #[wasm_bindgen(catch, constructor, js_namespace = Intl, js_name = Segmenter)]
    fn new(locales: &JsValue, options: &JsValue) -> Result<IntlSegmenter, JsValue>;

    #[wasm_bindgen(method, structural)]
    fn segment(this: &IntlSegmenter, input: &str) -> JsValue;
}

pub struct IntlWordSegmenter {
    inner: Option<IntlSegmenter>,
}

impl IntlWordSegmenter {
    pub fn new() -> Self {
        // `undefined` locale → user's default (browser picks).
        let opts = Object::new();
        let _ = Reflect::set(
            &opts,
            &JsValue::from_str("granularity"),
            &JsValue::from_str("word"),
        );
        let inner = IntlSegmenter::new(&JsValue::UNDEFINED, &opts.into()).ok();
        Self { inner }
    }
}

impl Default for IntlWordSegmenter {
    fn default() -> Self {
        Self::new()
    }
}

impl WordSegmenterTrait for IntlWordSegmenter {
    fn boundaries(&self, text: &str) -> Vec<usize> {
        let total_chars = text.chars().count();
        if total_chars == 0 {
            return vec![0];
        }

        let Some(seg) = self.inner.as_ref() else {
            return vec![0, total_chars];
        };

        let utf16_to_char = build_utf16_to_char_map(text);
        let segments_js = seg.segment(text);

        // `Intl.Segmenter.segment(...)` returns a `Segments` object — an
        // iterable of segment data records. `js_sys::try_iter` gives us
        // a Rust iterator over the JS iterator protocol.
        let iter = match js_sys::try_iter(&segments_js) {
            Ok(Some(it)) => it,
            _ => return vec![0, total_chars],
        };

        let mut out = Vec::new();
        out.push(0usize);

        for item in iter {
            let Ok(obj) = item else { continue };
            let index = Reflect::get(&obj, &JsValue::from_str("index"))
                .ok()
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0) as usize;
            let segment_str = Reflect::get(&obj, &JsValue::from_str("segment"))
                .ok()
                .and_then(|v| v.as_string())
                .unwrap_or_default();
            let utf16_len = segment_str.encode_utf16().count();
            let end_utf16 = index + utf16_len;
            let end_char = utf16_to_char.get(end_utf16).copied().unwrap_or(total_chars);
            if Some(&end_char) != out.last() {
                out.push(end_char);
            }
        }

        if out.last() != Some(&total_chars) {
            out.push(total_chars);
        }
        out
    }
}
