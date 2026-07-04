//! Minimal `icu_segmenter` compatibility shim for Parley.
//!
//! This crate intentionally implements only the API surface that Parley
//! 0.11.x uses (`complex-scripts` feature off). It delegates segmentation
//! to `rfgui-segmenter`, which uses platform text services where available
//! and Unicode rule fallbacks elsewhere.

use std::marker::PhantomData;

pub mod options {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct WordBreakInvariantOptions;

    impl WordBreakInvariantOptions {
        pub const fn default() -> Self {
            Self
        }
    }

    impl Default for WordBreakInvariantOptions {
        fn default() -> Self {
            Self
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct LineBreakOptions<'a> {
        pub word_option: Option<LineBreakWordOption>,
        // Real icu_segmenter carries `content_locale: Option<&'a
        // LanguageIdentifier>` here; Parley never sets it, so the shim only
        // keeps the lifetime alive.
        _marker: core::marker::PhantomData<&'a ()>,
    }

    impl LineBreakOptions<'_> {
        pub const fn default() -> Self {
            Self {
                word_option: None,
                _marker: core::marker::PhantomData,
            }
        }
    }

    impl Default for LineBreakOptions<'_> {
        fn default() -> Self {
            Self::default()
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum LineBreakWordOption {
        Normal,
        BreakAll,
        KeepAll,
    }
}

use options::{LineBreakOptions, LineBreakWordOption, WordBreakInvariantOptions};

#[derive(Clone, Copy, Debug)]
pub struct GraphemeClusterSegmenter;

impl GraphemeClusterSegmenter {
    pub const fn new() -> GraphemeClusterSegmenterBorrowed<'static> {
        GraphemeClusterSegmenterBorrowed {
            _marker: PhantomData,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct GraphemeClusterSegmenterBorrowed<'data> {
    _marker: PhantomData<&'data ()>,
}

impl GraphemeClusterSegmenterBorrowed<'_> {
    pub fn segment_str(self, input: &str) -> SegmentIterator {
        let seg = rfgui_segmenter::system_segmenter();
        let mut boundaries = seg.grapheme_boundaries_byte_indices(input);
        boundaries.sort_unstable();
        boundaries.dedup();
        SegmentIterator::new(split_oversized_grapheme_segments(input, boundaries))
    }
}

/// Parley builds one shaping cluster per grapheme segment and counts the
/// cluster's shaping-relevant chars in a `u8` (`map_len` in
/// `parley::shape::fill_cluster_in_place`, still unguarded as of parley
/// 0.11), so a segment with more than 255 chars panics with "attempt to
/// add with overflow" in debug builds and silently wraps in release.
/// Real grapheme clusters stay far below this; only pathological input
/// (thousands of combining marks on one base) reaches it. Splitting such
/// segments at the boundary level keeps every byte offset intact, unlike
/// the legacy zero-width-space insertion it replaces.
const MAX_GRAPHEME_SEGMENT_CHARS: usize = 240;

fn split_oversized_grapheme_segments(input: &str, boundaries: Vec<usize>) -> Vec<usize> {
    let mut out = Vec::with_capacity(boundaries.len());
    let mut prev = 0usize;
    for &end in &boundaries {
        // Chars are at least one byte, so short byte ranges cannot exceed
        // the char cap; only walk char indices for oversized candidates.
        if end - prev > MAX_GRAPHEME_SEGMENT_CHARS {
            for (chars_seen, (idx, _)) in input[prev..end].char_indices().enumerate() {
                if chars_seen > 0 && chars_seen % MAX_GRAPHEME_SEGMENT_CHARS == 0 {
                    out.push(prev + idx);
                }
            }
        }
        out.push(end);
        prev = end;
    }
    out
}

#[derive(Clone, Copy, Debug)]
pub struct WordSegmenter;

impl WordSegmenter {
    pub const fn new_for_non_complex_scripts(
        _options: WordBreakInvariantOptions,
    ) -> WordSegmenterBorrowed<'static> {
        WordSegmenterBorrowed {
            _marker: PhantomData,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct WordSegmenterBorrowed<'data> {
    _marker: PhantomData<&'data ()>,
}

impl WordSegmenterBorrowed<'_> {
    pub fn segment_str(self, input: &str) -> SegmentIterator {
        let seg = rfgui_segmenter::system_segmenter();
        SegmentIterator::new(seg.word_boundaries_byte_indices(input))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LineSegmenter;

impl LineSegmenter {
    pub const fn new_for_non_complex_scripts(
        options: LineBreakOptions<'_>,
    ) -> LineSegmenterBorrowed<'static> {
        LineSegmenterBorrowed {
            word_option: options.word_option,
            _marker: PhantomData,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LineSegmenterBorrowed<'data> {
    word_option: Option<LineBreakWordOption>,
    _marker: PhantomData<&'data ()>,
}

impl LineSegmenterBorrowed<'_> {
    pub fn segment_str(self, input: &str) -> SegmentIterator {
        let boundaries = match self.word_option {
            Some(LineBreakWordOption::BreakAll) => break_all_boundaries(input),
            Some(LineBreakWordOption::KeepAll) => keep_all_boundaries(input),
            _ => {
                let seg = rfgui_segmenter::system_segmenter();
                seg.line_boundaries_byte_indices(input)
            }
        };
        SegmentIterator::new(boundaries)
    }
}

#[derive(Clone, Debug)]
pub struct SegmentIterator {
    boundaries: Vec<usize>,
    index: usize,
}

impl SegmentIterator {
    fn new(mut boundaries: Vec<usize>) -> Self {
        boundaries.sort_unstable();
        boundaries.dedup();
        Self {
            boundaries,
            index: 0,
        }
    }
}

impl Iterator for SegmentIterator {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        let value = self.boundaries.get(self.index).copied()?;
        self.index += 1;
        Some(value)
    }
}

fn break_all_boundaries(input: &str) -> Vec<usize> {
    let mut out: Vec<usize> = input.char_indices().map(|(idx, _)| idx).collect();
    if out.first() != Some(&0) {
        out.insert(0, 0);
    }
    if out.last() != Some(&input.len()) {
        out.push(input.len());
    }
    out
}

fn keep_all_boundaries(input: &str) -> Vec<usize> {
    if input.is_empty() {
        return vec![0];
    }

    let mut out = vec![0];
    for (idx, ch) in input.char_indices() {
        let end = idx + ch.len_utf8();
        if ch.is_whitespace() && Some(&end) != out.last() {
            out.push(end);
        }
    }
    if out.last() != Some(&input.len()) {
        out.push(input.len());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn max_segment_chars(input: &str, boundaries: &[usize]) -> usize {
        boundaries
            .windows(2)
            .map(|pair| input[pair[0]..pair[1]].chars().count())
            .max()
            .unwrap_or(0)
    }

    #[test]
    fn short_segments_pass_through_unchanged() {
        let input = "hello \u{4E16}\u{754C}";
        let boundaries: Vec<usize> = input
            .char_indices()
            .map(|(idx, _)| idx)
            .chain([input.len()])
            .collect();
        assert_eq!(
            split_oversized_grapheme_segments(input, boundaries.clone()),
            boundaries
        );
    }

    #[test]
    fn oversized_combining_run_is_capped_on_char_boundaries() {
        let input = format!("a{}", "\u{0301}".repeat(2_000));
        let boundaries = vec![0, input.len()];
        let split = split_oversized_grapheme_segments(&input, boundaries);

        assert!(split.iter().all(|&b| input.is_char_boundary(b)));
        assert!(split.windows(2).all(|pair| pair[0] < pair[1]));
        assert_eq!(split.last(), Some(&input.len()));
        assert!(max_segment_chars(&input, &split) <= MAX_GRAPHEME_SEGMENT_CHARS);
    }

    #[test]
    fn grapheme_segment_str_never_exceeds_char_cap() {
        let input = format!("x{}tail", "\u{0300}".repeat(1_000));
        let boundaries: Vec<usize> = GraphemeClusterSegmenter::new()
            .segment_str(&input)
            .collect();
        assert!(max_segment_chars(&input, &boundaries) <= MAX_GRAPHEME_SEGMENT_CHARS);
        assert_eq!(boundaries.last(), Some(&input.len()));
    }
}
