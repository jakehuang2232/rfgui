//! Host-injectable word segmenter for caret word-jump (Option/Alt+Arrow,
//! double-click word selection, …). Engine core stays platform-clean —
//! the OS-native dictionary segmentation lives in the separate
//! `rfgui-segmenter` crate, which the host bridges into rfgui via
//! [`install_word_segmenter`].
//!
//! When no host segmenter is installed, [`DefaultWordSegmenter`] falls
//! back to alphanumeric/underscore runs. CJK then degrades to per-char
//! navigation; install `rfgui-segmenter::system_segmenter()` from the
//! host to get OS dictionary support.

use once_cell::sync::OnceCell;

/// Word segmentation interface. Boundaries are **char indices** into
/// `text`, ascending, with `0` and `text.chars().count()` always
/// present. Whitespace/punctuation segments are still reported —
/// callers that want word-only navigation must filter via
/// [`prev_word_boundary`] / [`next_word_boundary`].
pub trait WordSegmenter: Send + Sync {
    fn boundaries(&self, text: &str) -> Vec<usize>;
}

/// Engine fallback. Treats alphanumeric (incl. CJK Unified Ideographs)
/// + underscore as word chars, everything else as separators. CJK
/// scripts that need dictionary segmentation should install a richer
/// impl via [`install_word_segmenter`].
pub struct DefaultWordSegmenter;

impl WordSegmenter for DefaultWordSegmenter {
    fn boundaries(&self, text: &str) -> Vec<usize> {
        let chars: Vec<char> = text.chars().collect();
        let mut out = vec![0usize];
        if chars.is_empty() {
            return out;
        }
        let mut prev = is_word_char(chars[0]);
        for (i, c) in chars.iter().enumerate().skip(1) {
            let cur = is_word_char(*c);
            if cur != prev {
                out.push(i);
                prev = cur;
            }
        }
        out.push(chars.len());
        out
    }
}

#[inline]
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

static INSTALLED: OnceCell<Box<dyn WordSegmenter>> = OnceCell::new();
static FALLBACK: DefaultWordSegmenter = DefaultWordSegmenter;

/// Install the process-wide segmenter. Idempotent on first success;
/// subsequent calls return `Err(seg)` so the caller can drop or retry.
/// Hosts call this once at startup.
pub fn install_word_segmenter(seg: Box<dyn WordSegmenter>) -> Result<(), Box<dyn WordSegmenter>> {
    INSTALLED.set(seg)
}

/// Active segmenter — installed one if present, else
/// [`DefaultWordSegmenter`].
pub fn word_segmenter() -> &'static dyn WordSegmenter {
    match INSTALLED.get() {
        Some(b) => &**b,
        None => &FALLBACK,
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_ascii_words() {
        let s = DefaultWordSegmenter;
        // "foo bar" -> [0, 3, 4, 7]
        assert_eq!(s.boundaries("foo bar"), vec![0, 3, 4, 7]);
    }

    #[test]
    fn prev_next_skip_whitespace() {
        let s = DefaultWordSegmenter;
        // "  foo  bar  "
        assert_eq!(prev_word_boundary("  foo  bar  ", &s, 11), 7);
        assert_eq!(next_word_boundary("  foo  bar  ", &s, 0), 5);
        assert_eq!(next_word_boundary("  foo  bar  ", &s, 5), 10);
        assert_eq!(prev_word_boundary("  foo  bar  ", &s, 5), 2);
    }
}
