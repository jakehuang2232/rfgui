//! Cross-platform text segmentation for rfgui.
//!
//! Provides explicit char-index APIs for rfgui's editor/navigation code
//! and byte-index APIs for text engines such as Parley. Boundaries always
//! include `0` and the input length in the matching index space.
//!
//! Backends:
//! - macOS — `CFStringTokenizer` (system, includes CJK / Thai / Khmer /
//!   Burmese dictionaries).
//! - Windows — `Windows.Data.Text.WordsSegmenter` (system, ICU-backed).
//! - Web (wasm32) — `Intl.Segmenter` (browser, ICU-backed; full CJK).
//! - Other (Linux / fallback) — Unicode rule based segmentation.
//!
//! `system_segmenter()` returns the best impl available at compile time.

pub mod fallback;

#[cfg(all(target_os = "macos", not(target_arch = "wasm32")))]
pub mod macos;

#[cfg(all(target_os = "windows", not(target_arch = "wasm32")))]
pub mod windows;

#[cfg(target_arch = "wasm32")]
pub mod web;

/// Word segmentation interface.
pub trait WordSegmenter: Send + Sync {
    /// Boundaries as char indices into `text`.
    fn word_boundaries_char_indices(&self, text: &str) -> Vec<usize> {
        byte_indices_to_char_indices(text, self.word_boundaries_byte_indices(text))
    }

    /// Boundaries as byte indices into `text`.
    fn word_boundaries_byte_indices(&self, text: &str) -> Vec<usize> {
        char_indices_to_byte_indices(text, self.word_boundaries_char_indices(text))
    }
}

/// Line-break opportunity segmentation interface.
pub trait LineSegmenter: Send + Sync {
    /// Boundaries as char indices into `text`.
    fn line_boundaries_char_indices(&self, text: &str) -> Vec<usize> {
        byte_indices_to_char_indices(text, self.line_boundaries_byte_indices(text))
    }

    /// Boundaries as byte indices into `text`.
    fn line_boundaries_byte_indices(&self, text: &str) -> Vec<usize> {
        char_indices_to_byte_indices(text, self.line_boundaries_char_indices(text))
    }
}

/// Grapheme cluster segmentation interface.
pub trait GraphemeSegmenter: Send + Sync {
    /// Boundaries as char indices into `text`.
    fn grapheme_boundaries_char_indices(&self, text: &str) -> Vec<usize> {
        byte_indices_to_char_indices(text, self.grapheme_boundaries_byte_indices(text))
    }

    /// Boundaries as byte indices into `text`.
    fn grapheme_boundaries_byte_indices(&self, text: &str) -> Vec<usize> {
        char_indices_to_byte_indices(text, self.grapheme_boundaries_char_indices(text))
    }
}

/// Full text segmenter used by platform backends.
pub trait TextSegmenter: WordSegmenter + LineSegmenter + GraphemeSegmenter {}

impl<T> TextSegmenter for T where T: WordSegmenter + LineSegmenter + GraphemeSegmenter {}

/// Default cross-platform segmenter type for non-specialized callers.
pub type Segmenter = dyn TextSegmenter;

/// Backward-compatible alias for code that still imports `SystemSegmenter`.
pub type SystemSegmenter = dyn TextSegmenter;

/// Construct the best segmenter available on the current target.
pub fn system_segmenter() -> Box<dyn TextSegmenter> {
    #[cfg(target_arch = "wasm32")]
    {
        return Box::new(web::IntlSegmenter::new());
    }
    #[cfg(all(target_os = "macos", not(target_arch = "wasm32")))]
    {
        return Box::new(macos::CfStringTokenizerSegmenter::new());
    }
    #[cfg(all(target_os = "windows", not(target_arch = "wasm32")))]
    {
        return Box::new(windows::WindowsTextSegmenter::new());
    }
    #[cfg(not(any(target_arch = "wasm32", target_os = "macos", target_os = "windows")))]
    {
        Box::new(fallback::UnicodeSegmenter::new())
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

/// Byte-index lookup table for UTF-16 code-unit offsets.
#[allow(dead_code)] // not all cfgs reach this code path
pub(crate) fn build_utf16_to_byte_map(text: &str) -> Vec<usize> {
    let mut map = Vec::new();
    for (byte_idx, c) in text.char_indices() {
        for _ in 0..c.len_utf16() {
            map.push(byte_idx);
        }
    }
    map.push(text.len());
    map
}

/// Convert sorted char indices to byte indices.
pub fn char_indices_to_byte_indices(text: &str, char_indices: Vec<usize>) -> Vec<usize> {
    let mut char_to_byte: Vec<usize> = text.char_indices().map(|(byte_idx, _)| byte_idx).collect();
    char_to_byte.push(text.len());
    char_indices
        .into_iter()
        .filter_map(|idx| char_to_byte.get(idx).copied())
        .collect()
}

/// Convert sorted byte indices to char indices.
pub fn byte_indices_to_char_indices(text: &str, byte_indices: Vec<usize>) -> Vec<usize> {
    let mut out = Vec::with_capacity(byte_indices.len());
    let mut chars = text.char_indices().enumerate().peekable();
    let total_chars = text.chars().count();
    for byte_idx in byte_indices {
        if byte_idx == text.len() {
            out.push(total_chars);
            continue;
        }
        while let Some(&(char_idx, (current_byte, _))) = chars.peek() {
            if current_byte >= byte_idx {
                out.push(char_idx);
                break;
            }
            let _ = chars.next();
        }
    }
    out
}

/// Pull `(start, end)` pairs for **word** segments only, dropping
/// whitespace-only segments. Both indices are char indices into `text`.
fn word_segments(text: &str, seg: &dyn WordSegmenter) -> Vec<(usize, usize)> {
    let bs = seg.word_boundaries_char_indices(text);
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
