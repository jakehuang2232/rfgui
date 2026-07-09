//! Windows — `Windows.Data.Text.WordsSegmenter` (WinRT) for word
//! segmentation. Line and grapheme boundaries use Unicode fallbacks.
//!
//! `WordSegment::SourceTextSegment` reports **UTF-16 code-unit** offsets
//! into the original string; we translate to char indices for the trait
//! API.

use windows::Data::Text::WordsSegmenter;
use windows::core::HSTRING;

use crate::fallback::UnicodeSegmenter;
use crate::{
    GraphemeSegmenter, LineSegmenter, WordSegmenter as WordSegmenterTrait, build_utf16_to_byte_map,
    build_utf16_to_char_map,
};

pub struct WindowsTextSegmenter {
    inner: Option<WordsSegmenter>,
}

impl WindowsTextSegmenter {
    pub fn new() -> Self {
        // Empty language → "undetermined" → WinRT picks a sensible
        // locale-neutral segmenter (still applies CJK dict for CJK text).
        let inner = WordsSegmenter::CreateWithLanguage(&HSTRING::from("")).ok();
        Self { inner }
    }
}

impl Default for WindowsTextSegmenter {
    fn default() -> Self {
        Self::new()
    }
}

/// Backward-compatible alias.
pub type WindowsWordsSegmenter = WindowsTextSegmenter;

impl WordSegmenterTrait for WindowsTextSegmenter {
    fn word_boundaries_char_indices(&self, text: &str) -> Vec<usize> {
        windows_word_boundaries(
            self.inner.as_ref(),
            text,
            &build_utf16_to_char_map(text),
            text.chars().count(),
        )
    }

    fn word_boundaries_byte_indices(&self, text: &str) -> Vec<usize> {
        windows_word_boundaries(
            self.inner.as_ref(),
            text,
            &build_utf16_to_byte_map(text),
            text.len(),
        )
    }
}

impl LineSegmenter for WindowsTextSegmenter {
    fn line_boundaries_char_indices(&self, text: &str) -> Vec<usize> {
        UnicodeSegmenter::new().line_boundaries_char_indices(text)
    }

    fn line_boundaries_byte_indices(&self, text: &str) -> Vec<usize> {
        UnicodeSegmenter::new().line_boundaries_byte_indices(text)
    }
}

impl GraphemeSegmenter for WindowsTextSegmenter {
    fn grapheme_boundaries_char_indices(&self, text: &str) -> Vec<usize> {
        UnicodeSegmenter::new().grapheme_boundaries_char_indices(text)
    }

    fn grapheme_boundaries_byte_indices(&self, text: &str) -> Vec<usize> {
        UnicodeSegmenter::new().grapheme_boundaries_byte_indices(text)
    }
}

fn windows_word_boundaries(
    seg: Option<&WordsSegmenter>,
    text: &str,
    utf16_to_index: &[usize],
    total: usize,
) -> Vec<usize> {
    let total_chars = text.chars().count();
    if total_chars == 0 {
        return vec![0];
    }

    let Some(seg) = seg else {
        return vec![0, total];
    };

    let mut out = Vec::new();
    out.push(0usize);

    let hs = HSTRING::from(text);
    let Ok(tokens) = seg.GetTokens(&hs) else {
        return vec![0, total];
    };

    let Ok(count) = tokens.Size() else {
        return vec![0, total];
    };

    for i in 0..count {
        let Ok(token) = tokens.GetAt(i) else { continue };
        let Ok(span) = token.SourceTextSegment() else {
            continue;
        };
        let end_utf16 = (span.StartPosition + span.Length) as usize;
        let end = utf16_to_index.get(end_utf16).copied().unwrap_or(total);
        if Some(&end) != out.last() {
            out.push(end);
        }
    }

    if out.last() != Some(&total) {
        out.push(total);
    }
    out
}
