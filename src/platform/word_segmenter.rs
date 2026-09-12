//! Process-wide text segmenter used by caret word navigation.
//!
//! The implementation comes directly from `rfgui-segmenter`, so native
//! and web runners do not need a host-side adapter or install step.

use once_cell::sync::OnceCell;
pub use rfgui_segmenter::{GraphemeSegmenter, LineSegmenter, TextSegmenter, WordSegmenter};

static SYSTEM: OnceCell<Box<dyn TextSegmenter>> = OnceCell::new();

/// Active system segmenter.
pub fn word_segmenter() -> &'static dyn TextSegmenter {
    &**SYSTEM.get_or_init(rfgui_segmenter::system_segmenter)
}

/// macOS Option+Left target: nearest non-whitespace word **start**
/// strictly before `from`. Returns `0` when no such start exists.
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

/// macOS Option+Right target: nearest non-whitespace word **end**
/// strictly after `from`. Returns total char length if past last word.
pub fn next_word_boundary(text: &str, seg: &dyn WordSegmenter, from: usize) -> usize {
    let total = text.chars().count();
    for (_, e) in word_segments(text, seg) {
        if e > from {
            return e;
        }
    }
    total
}

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

#[cfg(test)]
mod tests;
