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
    /// Resolve a screen-space `(x, y)` hit to a root-content char index.
    /// See module doc for the three-step shape. Always returns some
    /// char index — falls back to the current `cursor_char` only when
    /// the TextArea has no children (empty content with no placeholder).
    pub(super) fn cursor_char_at_screen(&self, arena: &NodeArena, x: f32, y: f32) -> usize {
        self.cursor_target_at_screen(arena, x, y).char_index
    }

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
mod tests {
    use super::*;
    use crate::view::base_component::ElementTrait;

    /// `"a\n\nb"` splits into 3 Runs ("a"[0..2,ttn], ""[2..3,ttn],
    /// "b"[3..4]). The middle empty Run has `snap.width=0` (no shaping
    /// for empty paragraphs), so first-pass `point_in_rect` always
    /// misses it. The fallback path's x-midpoint rule used to bias every
    /// click in that band to `range_end` (= start of "b" Run), making
    /// it impossible to click into the visible blank middle line. This
    /// test pins the empty-Run behavior: any click in the empty Run's
    /// vertical band must collapse to `range_start` (= the empty
    /// paragraph's own char index).
    #[test]
    fn click_in_middle_empty_paragraph_lands_in_empty_run_not_following_sibling() {
        let mut text_area = TextArea::new();
        text_area.content = "a\n\nb".to_string();
        text_area.font_size = 14.0;
        text_area.line_height = 1.25;

        let mut arena = crate::view::test_support::new_test_arena();
        let root = crate::view::test_support::commit_element(
            &mut arena,
            Box::new(text_area) as Box<dyn ElementTrait>,
        );
        arena.with_element_taken(root, |el, _| {
            el.as_any_mut()
                .downcast_mut::<TextArea>()
                .expect("TextArea root")
                .set_self_node_key(root);
        });
        crate::view::test_support::measure_and_place(
            &mut arena,
            root,
            crate::view::base_component::LayoutConstraints {
                max_width: 300.0,
                max_height: 300.0,
                viewport_width: 300.0,
                viewport_height: 300.0,
                percent_base_width: None,
                percent_base_height: None,
            },
            crate::view::base_component::LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 300.0,
                available_height: 300.0,
                viewport_width: 300.0,
                viewport_height: 300.0,
                percent_base_width: None,
                percent_base_height: None,
            },
        );

        // Visual lines stack at line_height = 14 * 1.25 = 17.5.
        // Empty Run sits in the y band [17.5, 35.0].
        let click_y = 25.0;
        let click_x_left = 5.0;
        let click_x_right = 250.0;
        let cursor_left = arena
            .with_element_taken_ref(root, |el, arena| {
                el.as_any()
                    .downcast_ref::<TextArea>()
                    .unwrap()
                    .cursor_char_at_screen(arena, click_x_left, click_y)
            })
            .unwrap();
        let cursor_right = arena
            .with_element_taken_ref(root, |el, arena| {
                el.as_any()
                    .downcast_ref::<TextArea>()
                    .unwrap()
                    .cursor_char_at_screen(arena, click_x_right, click_y)
            })
            .unwrap();
        assert_eq!(
            cursor_left, 2,
            "click on empty middle line at small x should land on the empty Run's char (got {cursor_left})",
        );
        assert_eq!(
            cursor_right, 2,
            "click on empty middle line at large x should land on the empty Run's char (got {cursor_right})",
        );
    }
}
