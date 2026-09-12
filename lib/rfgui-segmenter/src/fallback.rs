//! Unicode rule based fallback. CJK word segmentation without dictionary
//! lookup degrades to per-char boundaries.

use unicode_segmentation::UnicodeSegmentation;

use crate::{GraphemeSegmenter, LineSegmenter, WordSegmenter};

pub struct UnicodeSegmenter;

impl UnicodeSegmenter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for UnicodeSegmenter {
    fn default() -> Self {
        Self::new()
    }
}

impl WordSegmenter for UnicodeSegmenter {
    fn word_boundaries_byte_indices(&self, text: &str) -> Vec<usize> {
        if text.is_empty() {
            return vec![0];
        }

        let mut out = Vec::new();
        out.push(0);
        for (byte_idx, slice) in text.split_word_bound_indices() {
            let end_byte = byte_idx + slice.len();
            if Some(&end_byte) != out.last() {
                out.push(end_byte);
            }
        }
        if out.last() != Some(&text.len()) {
            out.push(text.len());
        }
        out
    }
}

impl LineSegmenter for UnicodeSegmenter {
    fn line_boundaries_byte_indices(&self, text: &str) -> Vec<usize> {
        if text.is_empty() {
            return vec![0];
        }

        let mut out = Vec::new();
        out.push(0);
        for (byte_idx, _) in unicode_linebreak::linebreaks(text) {
            if Some(&byte_idx) != out.last() {
                out.push(byte_idx);
            }
        }
        if out.last() != Some(&text.len()) {
            out.push(text.len());
        }
        out
    }
}

impl GraphemeSegmenter for UnicodeSegmenter {
    fn grapheme_boundaries_byte_indices(&self, text: &str) -> Vec<usize> {
        if text.is_empty() {
            return vec![0];
        }

        let mut out: Vec<usize> = text.grapheme_indices(true).map(|(idx, _)| idx).collect();
        if out.first() != Some(&0) {
            out.insert(0, 0);
        }
        if out.last() != Some(&text.len()) {
            out.push(text.len());
        }
        out
    }
}

#[cfg(test)]
mod tests;
