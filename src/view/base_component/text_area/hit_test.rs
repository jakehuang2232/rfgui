//! Screen `(x, y)` → root-content char index, in three steps mirroring
//! the design note:
//!   1. Pick the visual **line** by `y` (`CaretNavigationMap`).
//!   2. Pick the nearest **stop** by `x` within that line.
//!   3. Return the stop's **char index**.
//!
//! `CaretNavigationMap` already carries every Run / projection child's
//! caret stops in screen coordinates (Run = real glyph stops, projection
//! = DFS Text glyph stops, icon-only projection = synthesized fragment
//! stops). Going through the map keeps caret display, vertical arrows,
//! and pointer hit-test consistent — they all read the same source of
//! truth.
//!
//! Fall-throughs handled by the map naturally:
//! - Empty paragraph Runs (`text=""`, no glyph buffer): contribute one
//!   synth stop on their own visual line, so a click in that band
//!   collapses to the empty paragraph's char.
//! - Wrapped projection fragments: `inline_fragment_rects` → one synth
//!   line per fragment, char span split across fragments.
//! - Click in a wrapped-inline fragment gap: nearest-line-by-y rule
//!   resolves to the fragment above or below, never a stop in the gap.

use crate::view::node_arena::NodeArena;

use super::TextArea;
use super::caret_map::{CaretAffinity, CaretNavigationMap, VerticalTarget};

impl TextArea {
    pub(super) fn cursor_target_at_screen(
        &self,
        arena: &NodeArena,
        x: f32,
        y: f32,
    ) -> VerticalTarget {
        let fallback = VerticalTarget {
            char_index: self.cursor_char.min(self.content.chars().count()),
            affinity: CaretAffinity::Downstream,
        };
        if self.children.is_empty() {
            return fallback;
        }
        let map = CaretNavigationMap::build(self, arena);
        map.pointer_target(x, y)
            .map(|mut target| {
                target.char_index = target.char_index.min(self.content.chars().count());
                target
            })
            .unwrap_or(fallback)
    }
}

#[cfg(test)]
mod tests;
