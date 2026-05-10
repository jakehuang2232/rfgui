//! Minimal `icu_segmenter` compatibility shim for Parley.
//!
//! This crate intentionally implements only the API surface that Parley
//! 0.9.x uses. It delegates segmentation to `rfgui-segmenter`, which uses
//! platform text services where available and Unicode rule fallbacks
//! elsewhere.

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
    pub struct LineBreakOptions {
        pub word_option: Option<LineBreakWordOption>,
    }

    impl LineBreakOptions {
        pub const fn default() -> Self {
            Self { word_option: None }
        }
    }

    impl Default for LineBreakOptions {
        fn default() -> Self {
            Self { word_option: None }
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
        SegmentIterator::new(seg.grapheme_boundaries_byte_indices(input))
    }
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
        options: LineBreakOptions,
    ) -> LineSegmenterBorrowed<'static> {
        LineSegmenterBorrowed {
            options,
            _marker: PhantomData,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LineSegmenterBorrowed<'data> {
    options: LineBreakOptions,
    _marker: PhantomData<&'data ()>,
}

impl LineSegmenterBorrowed<'_> {
    pub fn segment_str(self, input: &str) -> SegmentIterator {
        let boundaries = match self.options.word_option {
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
