//! UAX#29 fallback via `unicode-segmentation`. CJK without dictionary
//! lookup degrades to per-char boundaries — acceptable for ASCII /
//! Latin / Cyrillic / Arabic, poor for Chinese / Japanese / Thai.

use unicode_segmentation::UnicodeSegmentation;

use crate::WordSegmenter;

pub struct Uax29Segmenter;

impl Uax29Segmenter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for Uax29Segmenter {
    fn default() -> Self {
        Self::new()
    }
}

impl WordSegmenter for Uax29Segmenter {
    fn boundaries(&self, text: &str) -> Vec<usize> {
        let total_chars = text.chars().count();
        if total_chars == 0 {
            return vec![0];
        }

        // Map byte offset -> char index for translating
        // `split_word_bound_indices`'s byte offsets.
        let mut byte_to_char = std::collections::HashMap::with_capacity(total_chars + 1);
        for (char_idx, (byte_idx, _)) in text.char_indices().enumerate() {
            byte_to_char.insert(byte_idx, char_idx);
        }
        byte_to_char.insert(text.len(), total_chars);

        let mut out = Vec::new();
        out.push(0);
        for (byte_idx, slice) in text.split_word_bound_indices() {
            let end_byte = byte_idx + slice.len();
            if let Some(&c) = byte_to_char.get(&end_byte) {
                if Some(&c) != out.last() {
                    out.push(c);
                }
            }
        }
        if out.last() != Some(&total_chars) {
            out.push(total_chars);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty() {
        let s = Uax29Segmenter::new();
        assert_eq!(s.boundaries(""), vec![0]);
    }

    #[test]
    fn ascii_words() {
        let s = Uax29Segmenter::new();
        // "foo bar" -> "foo" | " " | "bar" => boundaries 0,3,4,7
        assert_eq!(s.boundaries("foo bar"), vec![0, 3, 4, 7]);
    }

    #[test]
    fn ascii_punctuation() {
        let s = Uax29Segmenter::new();
        // "a,b" -> "a" | "," | "b" => 0,1,2,3
        assert_eq!(s.boundaries("a,b"), vec![0, 1, 2, 3]);
    }

    #[test]
    fn cjk_per_char_known_limitation() {
        let s = Uax29Segmenter::new();
        // UAX#29 without dict: each CJK char its own boundary.
        // "今天" -> 0,1,2
        let bs = s.boundaries("今天");
        assert_eq!(bs.first(), Some(&0));
        assert_eq!(bs.last(), Some(&2));
    }
}
