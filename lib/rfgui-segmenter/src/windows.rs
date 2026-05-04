//! Windows — `Windows.Data.Text.WordsSegmenter` (WinRT). ICU-backed,
//! includes CJK / Thai dictionaries.
//!
//! `WordSegment::SourceTextSegment` reports **UTF-16 code-unit** offsets
//! into the original string; we translate to char indices for the trait
//! API.

use windows::Data::Text::WordsSegmenter;
use windows::core::HSTRING;

use crate::{WordSegmenter as WordSegmenterTrait, build_utf16_to_char_map};

pub struct WindowsWordsSegmenter {
    inner: Option<WordsSegmenter>,
}

impl WindowsWordsSegmenter {
    pub fn new() -> Self {
        // Empty language → "undetermined" → WinRT picks a sensible
        // locale-neutral segmenter (still applies CJK dict for CJK text).
        let inner = WordsSegmenter::Create(&HSTRING::from("")).ok();
        Self { inner }
    }
}

impl Default for WindowsWordsSegmenter {
    fn default() -> Self {
        Self::new()
    }
}

impl WordSegmenterTrait for WindowsWordsSegmenter {
    fn boundaries(&self, text: &str) -> Vec<usize> {
        let total_chars = text.chars().count();
        if total_chars == 0 {
            return vec![0];
        }

        let Some(seg) = self.inner.as_ref() else {
            return vec![0, total_chars];
        };

        let utf16_to_char = build_utf16_to_char_map(text);

        let mut out = Vec::new();
        out.push(0usize);

        let hs = HSTRING::from(text);
        let Ok(tokens) = seg.GetTokens(&hs) else {
            return vec![0, total_chars];
        };

        let Ok(count) = tokens.Size() else {
            return vec![0, total_chars];
        };

        for i in 0..count {
            let Ok(token) = tokens.GetAt(i) else { continue };
            let Ok(span) = token.SourceTextSegment() else {
                continue;
            };
            let end_utf16 = (span.StartPosition + span.Length) as usize;
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
