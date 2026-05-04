//! Cross-platform word segmentation for rfgui.
//!
//! Provides a `WordSegmenter` trait returning ascending **char-index**
//! boundaries for a string. Boundaries always include `0` and the total
//! char length; intermediate entries mark token edges.
//!
//! Backends:
//! - macOS — `CFStringTokenizer` (system, includes CJK / Thai / Khmer /
//!   Burmese dictionaries).
//! - Windows — `Windows.Data.Text.WordsSegmenter` (system, ICU-backed).
//! - Web (wasm32) — `Intl.Segmenter` (browser, ICU-backed; full CJK).
//! - Other (Linux / fallback) — UAX#29 via `unicode-segmentation`. CJK
//!   quality is poor (per-char boundaries) — host can swap in a richer
//!   impl by implementing `WordSegmenter` and injecting it where the
//!   segmenter is consumed.
//!
//! `system_segmenter()` returns the best impl available at compile time.

pub mod fallback;

#[cfg(all(target_os = "macos", not(target_arch = "wasm32")))]
pub mod macos;

#[cfg(all(target_os = "windows", not(target_arch = "wasm32")))]
pub mod windows;

#[cfg(target_arch = "wasm32")]
pub mod web;

/// Word segmentation interface. Boundaries are **char indices** into the
/// input `text`, ascending, with `0` and `text.chars().count()` always
/// present. Whitespace-only spans are still reported as their own
/// segment — callers that want word-only navigation must filter.
pub trait WordSegmenter: Send + Sync {
    fn boundaries(&self, text: &str) -> Vec<usize>;
}

/// Construct the best segmenter available on the current target.
pub fn system_segmenter() -> Box<dyn WordSegmenter> {
    #[cfg(target_arch = "wasm32")]
    {
        return Box::new(web::IntlWordSegmenter::new());
    }
    #[cfg(all(target_os = "macos", not(target_arch = "wasm32")))]
    {
        return Box::new(macos::CfStringTokenizerSegmenter::new());
    }
    #[cfg(all(target_os = "windows", not(target_arch = "wasm32")))]
    {
        return Box::new(windows::WindowsWordsSegmenter::new());
    }
    #[cfg(not(any(target_arch = "wasm32", target_os = "macos", target_os = "windows")))]
    {
        Box::new(fallback::Uax29Segmenter::new())
    }
}

/// Char-index lookup table for UTF-16 code-unit offsets. `vec[utf16_off]`
/// = char index of the code point starting at that offset; surrogate
/// continuation positions map to the same char index as the lead. A
/// trailing sentinel `vec[utf16_len] = total_chars` makes token-end
/// lookups branch-free. Used by the macOS / Windows / Web backends to
/// translate the OS APIs' UTF-16 ranges back to char indices.
#[allow(dead_code)] // not all cfgs reach this code path
pub(crate) fn build_utf16_to_char_map(text: &str) -> Vec<usize> {
    let mut map = Vec::new();
    let mut char_idx: usize = 0;
    for c in text.chars() {
        let n = c.len_utf16();
        for _ in 0..n {
            map.push(char_idx);
        }
        char_idx += 1;
    }
    map.push(char_idx);
    map
}

/// Pull `(start, end)` pairs for **word** segments only, dropping
/// whitespace-only segments. Both indices are char indices into `text`.
fn word_segments(text: &str, seg: &dyn WordSegmenter) -> Vec<(usize, usize)> {
    let bs = seg.boundaries(text);
    if bs.len() < 2 {
        return Vec::new();
    }
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    for w in bs.windows(2) {
        let (s, e) = (w[0], w[1]);
        let is_word = chars
            .get(s..e)
            .map(|sl| sl.iter().any(|c| !c.is_whitespace()))
            .unwrap_or(false);
        if is_word {
            out.push((s, e));
        }
    }
    out
}

/// macOS-style Option+Left target: nearest word **start** strictly
/// before `from`. Returns `0` if no word start exists below `from` —
/// e.g. cursor sits in leading whitespace.
pub fn prev_word_boundary(text: &str, seg: &dyn WordSegmenter, from: usize) -> usize {
    let mut best = 0usize;
    for (s, _) in word_segments(text, seg) {
        if s >= from {
            break;
        }
        best = s;
    }
    best
}

/// macOS-style Option+Right target: nearest word **end** strictly after
/// `from`. Returns total char length if `from` is past the last word.
pub fn next_word_boundary(text: &str, seg: &dyn WordSegmenter, from: usize) -> usize {
    let total = text.chars().count();
    for (_, e) in word_segments(text, seg) {
        if e > from {
            return e;
        }
    }
    total
}
