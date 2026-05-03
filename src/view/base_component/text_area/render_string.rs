//! `TextAreaRenderString` — the user-facing handler argument for `on_render`.
//!
//! Built up from inside an `on_render` closure via [`TextAreaRenderString::range`].
//! Each `range` call records a `(char_range, RsxNode)` projection which the
//! TextArea consumes during projection rebuild
//! ([`super::projection::TextArea::collect_normalized_projections`]) to slice
//! the content into Plain / Projection segments.
//!
//! v2 difference vs v1: `range`'s closure receives `RsxNode::text(slice)` —
//! the projected slice text — rather than a synthetic `<TextArea>` host node.
//! Users who want to render the slice themselves consume the node; users who
//! want to render arbitrary content (badge, image, etc.) ignore it.

use std::ops::{Bound, Range, RangeBounds};

use crate::ui::RsxNode;

#[derive(Clone, Debug, PartialEq)]
pub struct TextAreaRenderProjection {
    pub range: Range<usize>,
    pub node: RsxNode,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextAreaRenderString {
    content: String,
    projections: Vec<TextAreaRenderProjection>,
}

impl TextAreaRenderString {
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            projections: Vec::new(),
        }
    }

    pub fn content(&self) -> &str {
        self.content.as_str()
    }

    pub fn projections(&self) -> &[TextAreaRenderProjection] {
        self.projections.as_slice()
    }

    /// Record a projection for `range` (char indices). The closure receives
    /// the slice text wrapped as `RsxNode::text` and returns the RSX subtree
    /// that should replace the slice in the TextArea's inline flow.
    pub fn range<R, F>(&mut self, range: R, render: F)
    where
        R: RangeBounds<usize>,
        F: FnOnce(RsxNode) -> RsxNode,
    {
        let Some(range) = clamp_char_range(self.content.as_str(), range) else {
            return;
        };
        let start_byte = byte_index_at_char(self.content.as_str(), range.start);
        let end_byte = byte_index_at_char(self.content.as_str(), range.end);
        let slice = &self.content[start_byte..end_byte];
        let slice_node = RsxNode::text(slice.to_string());
        self.projections.push(TextAreaRenderProjection {
            range,
            node: render(slice_node),
        });
    }
}

fn byte_index_at_char(value: &str, char_index: usize) -> usize {
    if char_index == 0 {
        return 0;
    }
    value
        .char_indices()
        .nth(char_index)
        .map(|(idx, _)| idx)
        .unwrap_or(value.len())
}

fn clamp_char_range<R>(content: &str, range: R) -> Option<Range<usize>>
where
    R: RangeBounds<usize>,
{
    let len = content.chars().count();
    let start = match range.start_bound() {
        Bound::Included(value) => *value,
        Bound::Excluded(value) => value.saturating_add(1),
        Bound::Unbounded => 0,
    }
    .min(len);
    let end = match range.end_bound() {
        Bound::Included(value) => value.saturating_add(1),
        Bound::Excluded(value) => *value,
        Bound::Unbounded => len,
    }
    .min(len);
    if start >= end {
        return None;
    }
    Some(start..end)
}
