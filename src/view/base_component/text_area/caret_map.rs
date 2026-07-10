//! `CaretNavigationMap` — single source of truth for TextArea caret
//! geometry, vertical navigation, and pointer hit-test.
//!
//! See `docs/design/textarea-caret-navigation.md` for the design.
//!
//! Text runs, line breaks, and projection children contribute stops:
//! - `TextAreaTextRun` exposes per-line caret stops via `caret_stops()`.
//! - `TextAreaLineBreak` owns the `\n` source char and exposes the caret
//!   positions before and after the hard break.
//! - Projection roots DFS for the first text-bearing descendant
//!   (`<Text>` / `TextAreaTextRun`) and use its real glyph stops, mirroring
//!   `render.rs` / `hit_test.rs`. Icon-only projections (no text descendant)
//!   fall back to one synthesized line per `inline_fragment_rects` entry,
//!   distributing the projection's char span across fragments by width.

use std::ops::Range;

use crate::view::base_component::{Element, Text};
use crate::view::node_arena::{NodeArena, NodeKey};

use super::TextArea;
use super::run::{TextAreaLineBreak, TextAreaTextRun};

pub(super) struct CaretNavigationMapCache {
    origin_bits: [u32; 2],
    map: std::rc::Rc<CaretNavigationMap>,
}

/// One caret stop in screen coordinates. `char_index` is in the root
/// content's char space (i.e. directly comparable with
/// `TextArea::cursor_char`).
#[derive(Clone, Copy, Debug)]
pub(super) struct CaretStop {
    pub char_index: usize,
    pub x: f32,
    pub y_top: f32,
    pub height: f32,
    pub affinity: Option<CaretAffinity>,
}

/// One visual line — a contiguous horizontal band of caret stops the user
/// would consider a single row when pressing Up / Down.
#[derive(Clone, Debug)]
pub(super) struct CaretVisualLine {
    pub y_top: f32,
    pub y_bottom: f32,
    pub stops: Vec<CaretStop>,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum VerticalDirection {
    Up,
    Down,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct VerticalTarget {
    pub char_index: usize,
    pub affinity: CaretAffinity,
}

/// Caret affinity at a soft-wrap boundary. `Downstream` = caret renders
/// at the start of the **lower** visual line (the char's own glyph);
/// `Upstream` = caret sticks to the **end** of the upper line. Mirrors
/// Cocoa's `NSSelectionAffinity` and Flutter's `TextAffinity`.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub(crate) enum CaretAffinity {
    Upstream,
    Downstream,
}

impl Default for CaretAffinity {
    fn default() -> Self {
        Self::Downstream
    }
}

#[derive(Default, Debug)]
pub(super) struct CaretNavigationMap {
    pub(super) lines: Vec<CaretVisualLine>,
}

impl CaretNavigationMap {
    /// Build the map from the TextArea's unified IFC root package.
    /// Falls back to child stops only when no root package exists yet.
    /// Visual lines that share a vertical band are normalized so a
    /// sentence split across runs/projections still navigates as one row.
    pub(super) fn build(text_area: &TextArea, arena: &NodeArena) -> std::rc::Rc<Self> {
        if let Some(package) = text_area.unified_inline_ifc_render_package(arena) {
            let origin_x = text_area.layout_state.layout_position.x - text_area.scroll_x;
            let origin_y = text_area.layout_state.layout_position.y - text_area.scroll_y;
            let origin_bits = [origin_x.to_bits(), origin_y.to_bits()];
            if let Some(cached) = package.caret_navigation_map_cache.borrow().as_ref()
                && cached.origin_bits == origin_bits
            {
                return cached.map.clone();
            }
            let mut lines = package
                .visual_caret_lines_ref()
                .iter()
                .map(|line| CaretVisualLine {
                    y_top: origin_y + line.y_top,
                    y_bottom: origin_y + line.y_bottom,
                    stops: line
                        .stops
                        .iter()
                        .map(|stop| CaretStop {
                            char_index: stop.char_index,
                            x: origin_x + stop.x,
                            y_top: origin_y + stop.y_top,
                            height: stop.height,
                            affinity: Some(stop.affinity),
                        })
                        .collect(),
                })
                .collect::<Vec<_>>();
            normalize_caret_navigation_lines(&mut lines);
            let map = std::rc::Rc::new(Self { lines });
            *package.caret_navigation_map_cache.borrow_mut() = Some(CaretNavigationMapCache {
                origin_bits,
                map: map.clone(),
            });
            return map;
        }

        let mut raw_lines: Vec<CaretVisualLine> = Vec::new();
        for (idx, &child_key) in text_area.children.iter().enumerate() {
            let is_text_child = arena
                .with_element_taken_ref(child_key, |el, _| {
                    el.as_any().is::<TextAreaTextRun>() || el.as_any().is::<TextAreaLineBreak>()
                })
                .unwrap_or(false);
            if is_text_child {
                // Text runs are covered by the unified package path above
                // (a TextArea with any text child always builds a package);
                // only LineBreak carries standalone caret geometry here.
                let lines = arena
                    .with_element_taken_ref(child_key, |el, _| {
                        let (origin_x, origin_y, char_offset, caret_lines) = {
                            let line_break = el.as_any().downcast_ref::<TextAreaLineBreak>()?;
                            (
                                line_break.layout_state.layout_position.x,
                                line_break.layout_state.layout_position.y,
                                line_break.char_range.start,
                                line_break.caret_stops(),
                            )
                        };
                        let mut translated: Vec<CaretVisualLine> = Vec::new();
                        for line in caret_lines {
                            let stops = line
                                .stops
                                .into_iter()
                                .map(|s| CaretStop {
                                    char_index: char_offset + s.local_char,
                                    x: origin_x + s.local_x,
                                    y_top: origin_y + s.local_y_top,
                                    height: s.height,
                                    affinity: None,
                                })
                                .collect();
                            translated.push(CaretVisualLine {
                                y_top: origin_y + line.local_y_top,
                                y_bottom: origin_y + line.local_y_bottom,
                                stops,
                            });
                        }
                        Some(translated)
                    })
                    .flatten();
                if let Some(lines) = lines {
                    raw_lines.extend(lines);
                }
                continue;
            }

            // Projection branch — see module doc.
            let Some(range) = text_area.child_char_ranges.get(idx).cloned() else {
                continue;
            };
            raw_lines.extend(build_projection_lines(arena, child_key, &range));
        }

        // Merge visual lines from different runs / projections that share
        // a vertical band — e.g. two runs sitting on the same inline row,
        // or a projection sharing a row with a Run. Sort by y_top first so
        // neighboring entries are candidates.
        raw_lines.sort_by(|a, b| {
            a.y_top
                .partial_cmp(&b.y_top)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let merged = merge_visual_lines(raw_lines);

        // Sort stops within each line by x and dedup boundary chars (Run
        // trailing stop vs following projection's leading stop, etc.):
        // when two stops share a `char_index`, keep the rightmost one so
        // the boundary char's caret stop reflects the **owning** sibling
        // (cursor at boundary belongs to the following sibling per the
        // existing `caret_screen_position` rule).
        let mut lines = merged;
        for line in lines.iter_mut() {
            line.stops
                .sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));
            let mut deduped: Vec<CaretStop> = Vec::with_capacity(line.stops.len());
            for stop in line.stops.drain(..) {
                if let Some(last) = deduped.last_mut() {
                    if last.char_index == stop.char_index {
                        if stop.x > last.x {
                            *last = stop;
                        }
                        continue;
                    }
                }
                deduped.push(stop);
            }
            line.stops = deduped;
            for stop in line.stops.iter_mut() {
                stop.y_top = line.y_top;
                stop.height = (line.y_bottom - line.y_top).max(1.0);
            }
        }
        std::rc::Rc::new(Self { lines })
    }

    pub(super) fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Find the visual line index that owns `char_index`. Boundary
    /// chars shared by two lines (soft-wrap point) resolve via
    /// `affinity`: `Upstream` → upper line, `Downstream` → lower line
    /// (matches the legacy renderer rule that `cursor == range.end`
    /// belongs to the following sibling).
    pub(super) fn line_index_for_char(
        &self,
        char_index: usize,
        affinity: CaretAffinity,
    ) -> Option<usize> {
        let mut found: Option<usize> = None;
        for (idx, line) in self.lines.iter().enumerate() {
            if line.stops.iter().any(|s| s.char_index == char_index) {
                match affinity {
                    CaretAffinity::Upstream => {
                        if found.is_none() {
                            found = Some(idx);
                        }
                    }
                    CaretAffinity::Downstream => {
                        found = Some(idx);
                    }
                }
            }
        }
        found
    }

    /// First char on the visual line that owns `char_index`. Used by
    /// macOS Cmd+Left to jump to the wrap-aware line head (vs paragraph
    /// head, which is `\n`-based).
    pub(super) fn visual_line_home_for_char(
        &self,
        char_index: usize,
        affinity: CaretAffinity,
    ) -> Option<usize> {
        let idx = self.line_index_for_char(char_index, affinity)?;
        self.lines[idx].stops.first().map(|s| s.char_index)
    }

    /// Last char on the visual line that owns `char_index`. Used by
    /// macOS Cmd+Right.
    pub(super) fn visual_line_end_for_char(
        &self,
        char_index: usize,
        affinity: CaretAffinity,
    ) -> Option<usize> {
        let idx = self.line_index_for_char(char_index, affinity)?;
        self.lines[idx].stops.last().map(|s| s.char_index)
    }

    pub(super) fn caret_stop_for_char(
        &self,
        char_index: usize,
        affinity: CaretAffinity,
    ) -> Option<CaretStop> {
        let line_idx = self.line_index_for_char(char_index, affinity)?;
        let line = &self.lines[line_idx];
        // Per-line dedup keeps a single stop per char_index, so a
        // straight find suffices once `affinity` has picked the right
        // line.
        line.stops
            .iter()
            .find(|s| s.char_index == char_index)
            .copied()
    }

    /// Pointer hit-test: `(x, y)` screen → root-content char index, in
    /// the same three-step shape as the design note —
    /// (1) pick a visual line by `y`, (2) pick the nearest stop by `x`
    /// within that line, (3) return its `char_index`. Clicks above the
    /// first / below the last line clamp to the nearest line. Returns
    /// `None` only when the map is empty (no children at all).
    pub(super) fn pointer_target(&self, x: f32, y: f32) -> Option<VerticalTarget> {
        if self.lines.is_empty() {
            return None;
        }
        // Step 1: nearest line by vertical distance. `vertical_distance`
        // returns 0 inside the band, so clicks landing inside a line win
        // outright. Stable order (insertion / y_top sort) breaks ties at
        // a shared edge in favor of the upper line.
        let (line_idx, line) = self.lines.iter().enumerate().min_by(|(_, a), (_, b)| {
            vertical_distance(a, y)
                .partial_cmp(&vertical_distance(b, y))
                .unwrap_or(std::cmp::Ordering::Equal)
        })?;
        // Step 2: nearest stop by horizontal distance. Stops are sorted
        // by `x` after build / dedup, so a linear scan is fine.
        let stop = line.stops.iter().min_by(|a, b| {
            let ad = (a.x - x).abs();
            let bd = (b.x - x).abs();
            ad.partial_cmp(&bd)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.char_index.cmp(&b.char_index))
        })?;
        Some(VerticalTarget {
            char_index: stop.char_index,
            affinity: self.affinity_for_char_on_line(stop.char_index, line_idx),
        })
    }

    pub(super) fn vertical_target(
        &self,
        current_char: usize,
        current_affinity: CaretAffinity,
        sticky_x: f32,
        direction: VerticalDirection,
    ) -> Option<usize> {
        self.vertical_target_with_affinity(current_char, current_affinity, sticky_x, direction)
            .map(|target| target.char_index)
    }

    pub(super) fn vertical_target_with_affinity(
        &self,
        current_char: usize,
        current_affinity: CaretAffinity,
        sticky_x: f32,
        direction: VerticalDirection,
    ) -> Option<VerticalTarget> {
        let current_line = self.line_index_for_char(current_char, current_affinity)?;
        let target_idx = match direction {
            VerticalDirection::Up => current_line.checked_sub(1)?,
            VerticalDirection::Down => {
                let next = current_line + 1;
                if next >= self.lines.len() {
                    return None;
                }
                next
            }
        };
        let line = &self.lines[target_idx];
        // Snap to nearest x. Stops are sorted by x; linear scan is fine —
        // visual lines rarely exceed a few hundred glyphs.
        let mut best: Option<&CaretStop> = None;
        let mut best_d = f32::INFINITY;
        for stop in line.stops.iter() {
            let d = (stop.x - sticky_x).abs();
            if d < best_d {
                best_d = d;
                best = Some(stop);
            }
        }
        let stop = best?;
        Some(VerticalTarget {
            char_index: stop.char_index,
            affinity: self.affinity_for_char_on_line(stop.char_index, target_idx),
        })
    }

    fn affinity_for_char_on_line(&self, char_index: usize, line_idx: usize) -> CaretAffinity {
        if let Some(affinity) = self
            .lines
            .get(line_idx)
            .and_then(|line| line.stops.iter().find(|stop| stop.char_index == char_index))
            .and_then(|stop| stop.affinity)
        {
            return affinity;
        }

        let first = self
            .lines
            .iter()
            .position(|line| line.stops.iter().any(|s| s.char_index == char_index));
        let last = self
            .lines
            .iter()
            .rposition(|line| line.stops.iter().any(|s| s.char_index == char_index));

        match (first, last) {
            (Some(first), Some(last)) if first != last && line_idx == first => {
                CaretAffinity::Upstream
            }
            _ => CaretAffinity::Downstream,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::{RsxNode, RsxTagDescriptor};
    use crate::view::TextArea as HostTextArea;
    use crate::view::base_component::TextArea;
    use crate::view::test_support::{commit_rsx_tree, measure_and_place};

    fn host_text_area_node() -> RsxNode {
        RsxNode::tagged("TextArea", RsxTagDescriptor::for_tag::<HostTextArea>())
    }

    fn std_constraints() -> crate::view::base_component::LayoutConstraints {
        crate::view::base_component::LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        }
    }

    fn std_placement() -> crate::view::base_component::LayoutPlacement {
        crate::view::base_component::LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 800.0,
            available_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        }
    }

    /// Same fixture as `build_map_for` but returns the live arena +
    /// TextArea pointer so callers can poke the underlying Run for
    /// affinity-aware position probes.
    fn build_wrapped_textarea(
        content: &str,
        max_width: f32,
    ) -> (
        *const crate::view::base_component::TextArea,
        crate::view::node_arena::NodeArena,
    ) {
        let tree = host_text_area_node().with_prop("content", content);
        let mut arena = crate::view::test_support::new_test_arena();
        let roots = commit_rsx_tree(&mut arena, &tree);
        let root = *roots.first().expect("single root");
        let mut constraints = std_constraints();
        constraints.max_width = max_width;
        let mut placement = std_placement();
        placement.available_width = max_width;
        measure_and_place(&mut arena, root, constraints, placement);
        let ptr: *const crate::view::base_component::TextArea = arena
            .with_element_taken_ref(root, |el, _| {
                el.as_any()
                    .downcast_ref::<crate::view::base_component::TextArea>()
                    .unwrap() as *const _
            })
            .unwrap();
        (ptr, arena)
    }

    fn build_map_for(content: &str, max_width: f32) -> (std::rc::Rc<CaretNavigationMap>, usize) {
        let tree = host_text_area_node().with_prop("content", content);
        let mut arena = crate::view::test_support::new_test_arena();
        let roots = commit_rsx_tree(&mut arena, &tree);
        let root = *roots.first().expect("single root");
        let mut constraints = std_constraints();
        constraints.max_width = max_width;
        let mut placement = std_placement();
        placement.available_width = max_width;
        measure_and_place(&mut arena, root, constraints, placement);
        let text_area_ptr: *const TextArea = arena
            .with_element_taken_ref(root, |el, _| {
                el.as_any().downcast_ref::<TextArea>().unwrap() as *const TextArea
            })
            .unwrap();
        // SAFETY: arena is borrowed read-only for the duration of the
        // build call below. The pointer stays valid because `arena`
        // outlives this block and we never mutate it here.
        let text_area: &TextArea = unsafe { &*text_area_ptr };
        let map = CaretNavigationMap::build(text_area, &arena);
        let _ = &arena;
        let len = content.chars().count();
        (map, len)
    }

    #[test]
    fn repeated_build_reuses_unified_package_navigation_map() {
        let (text_area_ptr, arena) = build_wrapped_textarea("cache me", 800.0);
        // SAFETY: the arena remains alive and is only borrowed immutably.
        let text_area = unsafe { &*text_area_ptr };
        let first = CaretNavigationMap::build(text_area, &arena);
        let second = CaretNavigationMap::build(text_area, &arena);
        assert!(std::rc::Rc::ptr_eq(&first, &second));
    }

    #[test]
    fn hard_newline_down_lands_on_visual_line_below_at_similar_x() {
        let (map, len) = build_map_for("line1\nline2", 800.0);
        assert!(map.lines.len() >= 2, "expected >=2 visual lines");
        assert_eq!(len, 11);

        // Caret at end of "line1" (char 5) — Down should land on "line2"
        // near the same column (sticky_x = caret x at char 5 ≈ tail of
        // "line1").
        let stop = map
            .caret_stop_for_char(5, CaretAffinity::Downstream)
            .expect("caret stop for char 5 exists");
        let target = map
            .vertical_target(
                5,
                CaretAffinity::Downstream,
                stop.x,
                VerticalDirection::Down,
            )
            .expect("Down target exists");
        // char 5 is end of "line1", char 6 is start of "line2"
        // (the \n char itself); after one Down the caret should land at
        // a char inside the "line2" paragraph (>= 6, <= 11).
        assert!(
            (6..=11).contains(&target),
            "Down target should be inside line2 paragraph, got {target}",
        );
    }

    #[test]
    fn hard_newline_up_then_down_round_trips_to_original_line() {
        let (map, _) = build_map_for("line1\nline2", 800.0);
        let start_char = 8; // somewhere mid "line2"
        let stop = map
            .caret_stop_for_char(start_char, CaretAffinity::Downstream)
            .expect("caret stop for start char");
        let up = map
            .vertical_target(
                start_char,
                CaretAffinity::Downstream,
                stop.x,
                VerticalDirection::Up,
            )
            .expect("Up target");
        // Up should leave the line2 paragraph (target < 6, the start of
        // paragraph 2).
        assert!(up <= 5, "Up should land in line1, got {up}");
        let down = map
            .vertical_target(
                up,
                CaretAffinity::Downstream,
                stop.x,
                VerticalDirection::Down,
            )
            .expect("Down round-trip target");
        // The round-trip should approximate the original char (within a
        // glyph or two of slop). Tighter equality requires a fixed font
        // metric; we assert it lands back on the line2 paragraph.
        assert!(
            (6..=11).contains(&down),
            "Down round-trip should re-enter line2, got {down}",
        );
    }

    #[test]
    fn soft_wrap_within_run_yields_two_visual_lines() {
        // 60-char-ish content with a tight max_width forces the text
        // layout adapter to soft-wrap inside a single Run.
        let content = "the quick brown fox jumps over the lazy dog";
        let (map, _) = build_map_for(content, 80.0);
        assert!(
            map.lines.len() >= 2,
            "soft-wrap should create multiple visual lines, got {}",
            map.lines.len()
        );
        // Down from char 0 should land on a char in the second visual
        // line (i.e. y_top strictly greater than line 0's y_top).
        let line0_y = map.lines[0].y_top;
        let stop0 = map
            .caret_stop_for_char(0, CaretAffinity::Downstream)
            .expect("char 0 stop");
        let target = map
            .vertical_target(
                0,
                CaretAffinity::Downstream,
                stop0.x,
                VerticalDirection::Down,
            )
            .expect("Down from char 0");
        let target_stop = map
            .caret_stop_for_char(target, CaretAffinity::Downstream)
            .expect("target stop exists");
        assert!(
            target_stop.y_top > line0_y,
            "Down should land on a visually lower line; line0_y={line0_y}, target_y={}",
            target_stop.y_top,
        );
    }

    #[test]
    fn build_translates_unified_root_caret_stops_for_wrapped_navigation() {
        let (text_area_ptr, arena) =
            build_wrapped_textarea("甲乙丙丁戊己庚辛壬癸子丑寅卯辰巳午未申酉戌亥", 80.0);
        let text_area: &TextArea = unsafe { &*text_area_ptr };
        let map = CaretNavigationMap::build(text_area, &arena);
        let origin_x = text_area.layout_state.layout_position.x - text_area.scroll_x;
        let origin_y = text_area.layout_state.layout_position.y - text_area.scroll_y;
        let expected_lines = text_area
            .unified_inline_ifc_render_package(&arena)
            .expect("root package")
            .visual_caret_lines()
            .into_iter()
            .map(|line| {
                let stops = line
                    .stops
                    .into_iter()
                    .map(|stop| {
                        (
                            stop.char_index,
                            origin_x + stop.x,
                            origin_y + stop.y_top,
                            stop.height,
                        )
                    })
                    .collect::<Vec<_>>();
                (origin_y + line.y_top, origin_y + line.y_bottom, stops)
            })
            .collect::<Vec<_>>();

        assert!(
            expected_lines.len() >= 2,
            "fixture should exercise wrapped root IFC-backed caret stops"
        );
        assert_eq!(map.lines.len(), expected_lines.len());
        for (actual, (expected_y_top, expected_y_bottom, expected_stops)) in
            map.lines.iter().zip(expected_lines.iter())
        {
            assert_eq!(
                (actual.y_top, actual.y_bottom),
                (*expected_y_top, *expected_y_bottom)
            );
            assert_eq!(actual.stops.len(), expected_stops.len());
            for (actual_stop, expected_stop) in actual.stops.iter().zip(expected_stops.iter()) {
                assert_eq!(
                    (
                        actual_stop.char_index,
                        actual_stop.x,
                        actual_stop.y_top,
                        actual_stop.height,
                    ),
                    *expected_stop,
                    "CaretNavigationMap should build from TextArea unified root caret stops"
                );
            }
        }
    }

    #[test]
    fn visual_line_home_end_split_at_soft_wrap() {
        // Soft-wrap forces "the quick brown fox jumps over the lazy dog"
        // into multiple visual lines at width 80. Cmd+Left/Right must
        // honour the *visual* edge, not the paragraph (no `\n` here).
        let content = "the quick brown fox jumps over the lazy dog";
        let (map, len) = build_map_for(content, 80.0);
        assert!(
            map.lines.len() >= 2,
            "soft-wrap expected, got {}",
            map.lines.len()
        );
        let mid_char = len / 2;
        let line_idx = map
            .line_index_for_char(mid_char, CaretAffinity::Downstream)
            .expect("mid char on a visual line");
        let expected_home = map.lines[line_idx].stops.first().unwrap().char_index;
        let expected_end = map.lines[line_idx].stops.last().unwrap().char_index;
        assert_eq!(
            map.visual_line_home_for_char(mid_char, CaretAffinity::Downstream),
            Some(expected_home)
        );
        assert_eq!(
            map.visual_line_end_for_char(mid_char, CaretAffinity::Downstream),
            Some(expected_end)
        );
        // Non-trivial: visual home must not be `0` for any line past the
        // first wrap, otherwise this collapsed to paragraph behaviour.
        if line_idx > 0 {
            assert_ne!(expected_home, 0, "visual home of wrapped line ≠ 0");
        }
    }

    /// At a soft-wrap, the consumed whitespace byte has no glyph on
    /// either visual line — it's a single source position with two
    /// caret slots. `cursor_affinity` decides which:
    ///   * `Upstream`   → upper line's tail (caret immediately after
    ///                    the last visible glyph of the upper run).
    ///   * `Downstream` → lower line's head (caret at the first glyph
    ///                    of the lower run).
    #[test]
    fn wrap_gap_byte_caret_splits_by_affinity() {
        let (text_area_ptr, arena) =
            build_wrapped_textarea("甲乙丙丁戊己庚辛壬癸子丑寅卯辰巳午未申酉戌亥", 80.0);
        let text_area: &crate::view::base_component::TextArea = unsafe { &*text_area_ptr };
        let map = CaretNavigationMap::build(text_area, &arena);
        assert!(map.lines.len() >= 2);
        let boundary_char = map
            .lines
            .windows(2)
            .find_map(|pair| {
                pair[0].stops.iter().find_map(|upper| {
                    pair[1]
                        .stops
                        .iter()
                        .any(|lower| lower.char_index == upper.char_index)
                        .then_some(upper.char_index)
                })
            })
            .expect("adapter should synthesize a shared boundary stop at the wrap");
        let (up, down) = {
            let package = text_area
                .unified_inline_ifc_render_package(&arena)
                .expect("unified package");
            let origin_x = text_area.layout_state.layout_position.x - text_area.scroll_x;
            let origin_y = text_area.layout_state.layout_position.y - text_area.scroll_y;
            let u = package
                .caret_geometry_for_char(boundary_char, CaretAffinity::Upstream)
                .expect("upstream caret");
            let d = package
                .caret_geometry_for_char(boundary_char, CaretAffinity::Downstream)
                .expect("downstream caret");
            (
                (origin_x + u.x, origin_y + u.y_top, u.height),
                (origin_x + d.x, origin_y + d.y_top, d.height),
            )
        };
        assert!(
            up.1 < down.1,
            "Upstream y ({}) on upper line, Downstream y ({}) on lower",
            up.1,
            down.1,
        );
        assert!(
            up.1 < map.lines[1].y_top,
            "Upstream caret y ({}) should be on upper line (< {})",
            up.1,
            map.lines[1].y_top,
        );
        assert!(
            (down.1 - map.lines[1].y_top).abs() < 1.0,
            "Downstream caret y ({}) should match lower line top ({})",
            down.1,
            map.lines[1].y_top,
        );
    }

    /// At the lower-line head char (first glyph of the wrapped run)
    /// affinity *is* meaningful: Downstream → lower head, Upstream →
    /// upper tail. This is the position Cmd+Right may pin Upstream when
    /// the visual line end stop coincides with the lower-run head.
    #[test]
    fn wrap_lower_head_caret_honours_affinity() {
        let (text_area_ptr, arena) =
            build_wrapped_textarea("甲乙丙丁戊己庚辛壬癸子丑寅卯辰巳午未申酉戌亥", 80.0);
        let text_area: &crate::view::base_component::TextArea = unsafe { &*text_area_ptr };
        let map = CaretNavigationMap::build(text_area, &arena);
        let lower_head = map.lines[1].stops.first().unwrap().char_index;
        let (up, down) = {
            let package = text_area
                .unified_inline_ifc_render_package(&arena)
                .expect("unified package");
            let origin_x = text_area.layout_state.layout_position.x - text_area.scroll_x;
            let origin_y = text_area.layout_state.layout_position.y - text_area.scroll_y;
            let u = package
                .caret_geometry_for_char(lower_head, CaretAffinity::Upstream)
                .expect("upstream caret");
            let d = package
                .caret_geometry_for_char(lower_head, CaretAffinity::Downstream)
                .expect("downstream caret");
            (
                (origin_x + u.x, origin_y + u.y_top, u.height),
                (origin_x + d.x, origin_y + d.y_top, d.height),
            )
        };
        assert!(
            up.1 < down.1,
            "Upstream y on upper line, Downstream on lower"
        );
        assert!(
            up.0 > down.0,
            "Upstream sits at upper tail x ({}) > Downstream lower head x ({})",
            up.0,
            down.0,
        );
    }

    #[test]
    fn vertical_target_preserves_boundary_line_with_affinity() {
        let (map, _) = build_map_for("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ", 80.0);
        let (upper_idx, boundary_char, sticky_x) = map
            .lines
            .windows(2)
            .enumerate()
            .find_map(|(idx, pair)| {
                let upper_tail = pair[0].stops.last()?;
                let lower_head = pair[1].stops.first()?;
                (upper_tail.char_index == lower_head.char_index).then_some((
                    idx,
                    upper_tail.char_index,
                    upper_tail.x,
                ))
            })
            .expect("fixture should have a shared soft-wrap boundary char");

        let lower_line = map.lines.get(upper_idx + 1).expect("lower line");
        let current = lower_line
            .stops
            .iter()
            .find(|stop| stop.char_index != boundary_char)
            .or_else(|| lower_line.stops.first())
            .expect("lower line has a current stop");
        let target = map
            .vertical_target_with_affinity(
                current.char_index,
                CaretAffinity::Downstream,
                sticky_x,
                VerticalDirection::Up,
            )
            .expect("Up target exists");

        assert_eq!(target.char_index, boundary_char);
        assert_eq!(target.affinity, CaretAffinity::Upstream);
        assert_eq!(
            map.line_index_for_char(target.char_index, target.affinity),
            Some(upper_idx),
            "target affinity should resolve back to the selected upper visual line",
        );
    }

    #[test]
    fn vertical_target_returns_none_at_edges() {
        let (map, len) = build_map_for("solo line", 800.0);
        assert!(!map.is_empty());
        let stop0 = map
            .caret_stop_for_char(0, CaretAffinity::Downstream)
            .unwrap();
        assert!(
            map.vertical_target(0, CaretAffinity::Downstream, stop0.x, VerticalDirection::Up)
                .is_none()
        );
        let stop_end = map
            .caret_stop_for_char(len, CaretAffinity::Downstream)
            .unwrap();
        assert!(
            map.vertical_target(
                len,
                CaretAffinity::Downstream,
                stop_end.x,
                VerticalDirection::Down
            )
            .is_none()
        );
    }

    #[test]
    fn empty_paragraph_between_text_yields_navigable_stop() {
        let (map, _) = build_map_for("a\n\nb", 800.0);
        // Three visual lines: "a", "", "b".
        assert!(
            map.lines.len() >= 3,
            "expected >=3 visual lines for a\\n\\nb, got {}: {:#?}",
            map.lines.len(),
            map.lines
        );
        // Down from char 0 ('a') should land on the empty middle line —
        // char 2 (after the first \n).
        let stop0 = map
            .caret_stop_for_char(0, CaretAffinity::Downstream)
            .unwrap();
        let target = map
            .vertical_target(
                0,
                CaretAffinity::Downstream,
                stop0.x,
                VerticalDirection::Down,
            )
            .expect("Down from char 0");
        // char 2 = start of the empty paragraph (after the first `\n`).
        // It must be a distinct caret target rather than skipping straight
        // to char 3 (the following paragraph).
        assert_eq!(target, 2, "Down from line 1 must land on the empty line");
    }

    #[test]
    fn newline_only_content_exposes_a_pointer_caret_stop_on_each_empty_line() {
        let (map, _) = build_map_for("\n", 300.0);
        assert!(
            map.lines.len() >= 2,
            "a lone newline must create two empty visual lines: {:#?}",
            map.lines
        );
        for (line_index, line) in map.lines.iter().take(2).enumerate() {
            let target = map
                .pointer_target(0.0, (line.y_top + line.y_bottom) * 0.5)
                .expect("empty visual line should accept a caret pointer target");
            assert_eq!(
                map.line_index_for_char(target.char_index, target.affinity),
                Some(line_index),
                "pointer target must remain on empty line {line_index}: {target:?}"
            );
        }
    }

    // ---------------------------------------------------------------
    // Projection-aware navigation map tests.
    //
    // Fixture builds a TextArea whose `on_render_handler` projects a
    // single contiguous char range onto an `<Element>` (optionally
    // containing a `<Text>` so glyph stops light up). Mirrors
    // render.rs's projection_fixture style but with caret_map's
    // assertions.
    // ---------------------------------------------------------------

    use crate::style::Length;
    use crate::view::ElementStylePropSchema;
    use crate::view::base_component::{ElementTrait, LayoutConstraints, LayoutPlacement};

    struct ProjectionFixture {
        arena: crate::view::node_arena::NodeArena,
        root: crate::view::node_arena::NodeKey,
    }

    impl ProjectionFixture {
        fn map(&self) -> std::rc::Rc<CaretNavigationMap> {
            let ptr: *const TextArea = self
                .arena
                .with_element_taken_ref(self.root, |el, _| {
                    el.as_any().downcast_ref::<TextArea>().unwrap() as *const TextArea
                })
                .unwrap();
            // SAFETY: arena is read-only for the duration of build().
            let text_area: &TextArea = unsafe { &*ptr };
            CaretNavigationMap::build(text_area, &self.arena)
        }
    }

    fn build_projection_fixture(
        content: &'static str,
        projection_range: std::ops::Range<usize>,
        inner_text: Option<&'static str>,
        projection_style: ElementStylePropSchema,
        max_width: f32,
    ) -> ProjectionFixture {
        let mut text_area = TextArea::new();
        text_area.content = content.to_string();
        text_area.font_size = 14.0;
        text_area.line_height = 1.25;
        text_area.cursor_char = 0;
        text_area.on_render_handler = Some(crate::ui::on_text_area_render(move |render| {
            let style = projection_style.clone();
            let inner = inner_text;
            render.range(projection_range.clone(), move |_node| {
                let element = RsxNode::tagged(
                    "Element",
                    RsxTagDescriptor::for_tag::<crate::view::tags::Element>(),
                )
                .with_prop("style", style.clone());
                if let Some(text) = inner {
                    element.with_child(
                        RsxNode::tagged(
                            "Text",
                            RsxTagDescriptor::for_tag::<crate::view::tags::Text>(),
                        )
                        .with_child(RsxNode::text(text)),
                    )
                } else {
                    element
                }
            });
        }));

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
            LayoutConstraints {
                max_width,
                max_height: 600.0,
                viewport_width: max_width,
                viewport_height: 600.0,
                percent_base_width: Some(max_width),
                percent_base_height: Some(600.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: max_width,
                available_height: 600.0,
                viewport_width: max_width,
                viewport_height: 600.0,
                percent_base_width: Some(max_width),
                percent_base_height: Some(600.0),
            },
        );
        ProjectionFixture { arena, root }
    }

    fn fixed_box_style() -> ElementStylePropSchema {
        ElementStylePropSchema {
            width: Some(Length::px(60.0)),
            height: Some(Length::px(28.0)),
            ..Default::default()
        }
    }

    /// Projection containing `<Text>` — caret-stop_for_char must succeed
    /// for char indices that fall inside the projection. Without the
    /// projection branch the map had no entry for these chars and
    /// vertical-arrow handling silently bailed.
    #[test]
    fn projection_with_text_emits_stops_for_inner_chars() {
        // chars: 0..3 "abc", 3..6 "XYZ" (projection w/ Text), 6..9 "def".
        let fx = build_projection_fixture(
            "abcXYZdef",
            3..6,
            Some("XYZ"),
            ElementStylePropSchema::default(),
            800.0,
        );
        let map = fx.map();
        for cur in 3..=6 {
            assert!(
                map.caret_stop_for_char(cur, CaretAffinity::Downstream)
                    .is_some(),
                "missing caret stop for projection char {cur}",
            );
        }
    }

    /// Caret inside a projection (with `<Text>` descendant) on its own
    /// visual line: Down should leave to the next paragraph's line, Up
    /// should leave to the previous paragraph's line. Pre-fix this used
    /// to be a no-op because projection chars had no map entry at all.
    #[test]
    fn projection_with_text_caret_inside_can_move_up_and_down() {
        // 3 paragraphs, projection on its own line (line 1).
        // chars: 0..3 "abc", 3..4 "\n", 4..7 "XYZ" (projection),
        //        7..8 "\n", 8..11 "def".
        let fx = build_projection_fixture(
            "abc\nXYZ\ndef",
            4..7,
            Some("XYZ"),
            ElementStylePropSchema::default(),
            800.0,
        );
        let map = fx.map();
        let inside = 5; // mid-projection char
        let stop = map
            .caret_stop_for_char(inside, CaretAffinity::Downstream)
            .expect("projection char has a stop");
        let down = map
            .vertical_target(
                inside,
                CaretAffinity::Downstream,
                stop.x,
                VerticalDirection::Down,
            )
            .expect("Down should land somewhere");
        // Down should leave the projection line (chars 4..=7) to "def"
        // (chars 8..=11).
        assert!(
            (8..=11).contains(&down),
            "Down from projection char {inside} should land on the def line, got {down}",
        );
        let up = map
            .vertical_target(
                inside,
                CaretAffinity::Downstream,
                stop.x,
                VerticalDirection::Up,
            )
            .expect("Up should land somewhere");
        // Up should leave the projection line to "abc" (chars 0..=3).
        assert!(
            up <= 3,
            "Up from projection char {inside} should land on the abc line, got {up}",
        );
    }

    /// sticky-x at a column inside the projection's horizontal extent
    /// should land *on* a projection char when Up/Down crosses the
    /// projection's row. Pre-fix the projection's row had no stops so
    /// vertical_target snapped to a non-projection char on the same y.
    #[test]
    fn vertical_target_lands_inside_projection_when_sticky_x_overlaps() {
        // Same fixture: projection on its own line.
        let fx = build_projection_fixture(
            "abc\nXYZ\ndef",
            4..7,
            Some("XYZ"),
            ElementStylePropSchema::default(),
            800.0,
        );
        let map = fx.map();
        // Pick the projection's middle char and use its x as sticky_x;
        // a Down from line 0 ("abc") at that x should land on a
        // projection char (4..=7).
        let mid_stop = map
            .caret_stop_for_char(5, CaretAffinity::Downstream)
            .expect("projection mid stop");
        let line0_target = map
            .vertical_target(
                0,
                CaretAffinity::Downstream,
                mid_stop.x,
                VerticalDirection::Down,
            )
            .expect("Down from char 0");
        assert!(
            (4..=7).contains(&line0_target),
            "Down from line0 at projection-x should land in projection chars, got {line0_target}",
        );
    }

    /// Icon-only projection (no `<Text>` descendant) sharing a line
    /// with surrounding Run text. The projection's char range still
    /// needs map entries so caret-inside Up/Down isn't a no-op. Stops
    /// land at the projection's box (per `inline_fragment_rects` /
    /// box-snapshot fallback).
    #[test]
    fn icon_only_projection_caret_inside_can_move_up_and_down() {
        // chars: 0..3 "abc", 3..4 "\n", 4..7 "XYZ" (icon projection),
        //        7..8 "\n", 8..11 "def".
        let fx = build_projection_fixture("abc\nXYZ\ndef", 4..7, None, fixed_box_style(), 800.0);
        let map = fx.map();
        let inside = 5;
        let stop = map
            .caret_stop_for_char(inside, CaretAffinity::Downstream)
            .expect("icon projection should still emit stops");
        let down = map
            .vertical_target(
                inside,
                CaretAffinity::Downstream,
                stop.x,
                VerticalDirection::Down,
            )
            .expect("Down from inside icon projection");
        assert!(
            (8..=11).contains(&down),
            "Down from icon projection char {inside} should land on the def line, got {down}",
        );
        let up = map
            .vertical_target(
                inside,
                CaretAffinity::Downstream,
                stop.x,
                VerticalDirection::Up,
            )
            .expect("Up from inside icon projection");
        assert!(
            up <= 3,
            "Up from icon projection char {inside} should land on the abc line, got {up}",
        );
    }

    /// Integration check for the boundary-cursor affinity behavior.
    /// With content soft-wrapped and `cursor_char` parked at the
    /// wrap-consumed whitespace char (= the boundary cursor), caret y
    /// follows `cursor_affinity`:
    ///   * `Upstream`   → upper visual line.
    ///   * `Downstream` → lower visual line.
    #[test]
    fn caret_at_boundary_cursor_splits_by_affinity() {
        let content = "甲乙丙丁戊己庚辛壬癸子丑寅卯辰巳午未申酉戌亥";
        let max_width = 80.0;
        let upper_tail = {
            let (map, _) = build_map_for(content, max_width);
            assert!(map.lines.len() >= 2, "soft-wrap expected");
            map.lines[0].stops.last().unwrap().char_index
        };

        let mut up_y: Option<f32> = None;
        let mut down_y: Option<f32> = None;
        let mut upper_y_ref = 0.0;
        let mut lower_y_ref = 0.0;
        for affinity in [CaretAffinity::Downstream, CaretAffinity::Upstream] {
            let mut text_area = TextArea::new();
            text_area.content = content.to_string();
            text_area.font_size = 14.0;
            text_area.line_height = 1.25;
            text_area.is_focused = true;
            text_area.cursor_char = upper_tail;
            text_area.cursor_affinity = affinity;
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
                LayoutConstraints {
                    max_width,
                    max_height: 600.0,
                    viewport_width: max_width,
                    viewport_height: 600.0,
                    percent_base_width: Some(max_width),
                    percent_base_height: Some(600.0),
                },
                LayoutPlacement {
                    parent_x: 0.0,
                    parent_y: 0.0,
                    visual_offset_x: 0.0,
                    visual_offset_y: 0.0,
                    available_width: max_width,
                    available_height: 600.0,
                    viewport_width: max_width,
                    viewport_height: 600.0,
                    percent_base_width: Some(max_width),
                    percent_base_height: Some(600.0),
                },
            );
            let (caret, upper_y, lower_y) = arena
                .with_element_taken_ref(root, |el, arena| {
                    let ta = el.as_any().downcast_ref::<TextArea>().unwrap();
                    let map = CaretNavigationMap::build(ta, arena);
                    let upper = map.lines[0].y_top;
                    let lower = map.lines[1].y_top;
                    let caret = ta.caret_screen_position(arena);
                    (caret, upper, lower)
                })
                .unwrap();
            let (_, y, _) = caret.expect("caret resolves");
            upper_y_ref = upper_y;
            lower_y_ref = lower_y;
            match affinity {
                CaretAffinity::Upstream => up_y = Some(y),
                CaretAffinity::Downstream => down_y = Some(y),
            }
        }
        let bup = up_y.unwrap();
        let bdown = down_y.unwrap();
        assert!(
            bup < bdown,
            "Upstream y ({bup}) on upper line, Downstream y ({bdown}) on lower",
        );
        assert!(
            (bup - upper_y_ref).abs() < (lower_y_ref - upper_y_ref) * 0.5,
            "Upstream y ({bup}) should match upper line ({upper_y_ref})",
        );
        assert!(
            (bdown - lower_y_ref).abs() < (lower_y_ref - upper_y_ref) * 0.5,
            "Downstream y ({bdown}) should match lower line ({lower_y_ref})",
        );
    }

    /// Repro: caret should resolve to a screen position on every kind of
    /// empty visual line (middle empty paragraph, trailing newline, fully
    /// empty content). Failure here = caret invisible in editor.
    #[test]
    fn caret_screen_position_resolves_on_every_empty_line_kind() {
        fn check(content: &str, cursor: usize, label: &str) {
            let mut text_area = TextArea::new();
            text_area.content = content.to_string();
            text_area.font_size = 14.0;
            text_area.line_height = 1.25;
            text_area.is_focused = true;
            text_area.cursor_char = cursor;
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
                LayoutConstraints {
                    max_width: 300.0,
                    max_height: 300.0,
                    viewport_width: 300.0,
                    viewport_height: 300.0,
                    percent_base_width: None,
                    percent_base_height: None,
                },
                LayoutPlacement {
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
            let pos = arena
                .with_element_taken_ref(root, |el, arena| {
                    el.as_any()
                        .downcast_ref::<TextArea>()
                        .unwrap()
                        .caret_screen_position(arena)
                })
                .flatten();
            assert!(pos.is_some(), "{label}: caret should resolve");
        }
        check("", 0, "fully empty");
        check("\n", 0, "newline-only first empty line");
        check("\n", 1, "newline-only trailing empty line");
        check("a\n", 2, "trailing-newline empty line");
        check("a\n\nb", 2, "middle empty paragraph");
    }

    /// `pointer_target` is the three-step shape used by hit-test:
    /// (1) line-by-y (inside-band wins, else nearest), (2) nearest stop
    /// by x within that line, (3) return its char_index. Verify all three
    /// steps independently.
    #[test]
    fn pointer_target_picks_line_by_y_then_nearest_stop_by_x() {
        // Two paragraphs side by side stacked vertically gives us two
        // visual lines with predictable y bands.
        let (map, _) = build_map_for("line one\nline two", 800.0);
        assert!(map.lines.len() >= 2, "expected >= 2 visual lines");

        // Step 1: y above the first line clamps to line 0.
        let line0 = &map.lines[0];
        let line0_mid_y = (line0.y_top + line0.y_bottom) * 0.5;
        let target = map
            .pointer_target(0.0, line0.y_top - 1000.0)
            .expect("clamp above-first should find a target");
        let stop = map
            .caret_stop_for_char(target.char_index, target.affinity)
            .expect("stop");
        assert!(
            (stop.y_top - line0.y_top).abs() < 0.5,
            "above-first click should clamp to line 0 (y_top={})",
            line0.y_top,
        );

        // Step 2 & 3: y inside line 0, x near line 0's last stop should
        // pick that last stop's char (= 8, end of "line one").
        let last_stop_x = line0.stops.last().expect("non-empty").x;
        let target = map
            .pointer_target(last_stop_x + 1000.0, line0_mid_y)
            .expect("inside line 0 picks a target");
        assert_eq!(
            target.char_index,
            line0.stops.last().expect("non-empty").char_index,
            "x past line 0 should snap to its rightmost stop",
        );

        // y below the last line clamps to last line.
        let last_line = map.lines.last().expect("non-empty");
        let target_below = map
            .pointer_target(0.0, last_line.y_bottom + 1000.0)
            .expect("clamp below-last");
        let stop_below = map
            .caret_stop_for_char(target_below.char_index, target_below.affinity)
            .expect("stop");
        assert!(
            (stop_below.y_top - last_line.y_top).abs() < 0.5,
            "below-last click should clamp to last line",
        );
    }

    #[test]
    fn pointer_target_preserves_upper_affinity_at_soft_wrap_tail() {
        let content = "甲乙丙丁戊己庚辛壬癸子丑寅卯辰巳午未申酉戌亥";
        let (map, _) = build_map_for(content, 80.0);
        assert!(map.lines.len() >= 2, "soft-wrap expected");
        let line0 = &map.lines[0];
        let line0_mid_y = (line0.y_top + line0.y_bottom) * 0.5;
        let upper_tail = line0.stops.last().expect("upper line has tail stop");

        let target = map
            .pointer_target(upper_tail.x + 1000.0, line0_mid_y)
            .expect("line-tail click should resolve");

        assert_eq!(target.char_index, upper_tail.char_index);
        assert_eq!(
            target.affinity,
            CaretAffinity::Upstream,
            "clicking the upper visual line tail must keep the caret on that line",
        );
    }

    /// Boundary char between a Run and the following projection: only
    /// one stop survives per visual line (the projection's owning stop)
    /// so vertical_target's nearest-x search isn't fooled by a duplicate
    /// at the same x.
    #[test]
    fn boundary_char_between_run_and_projection_is_deduped_per_line() {
        // chars: 0..3 "abc", 3..6 "XYZ" (projection w/ Text), 6..9 "def".
        // Boundary chars: 3 (Run "abc" tail == projection leading) and
        // 6 (projection tail == Run "def" leading).
        let fx = build_projection_fixture(
            "abcXYZdef",
            3..6,
            Some("XYZ"),
            ElementStylePropSchema::default(),
            800.0,
        );
        let map = fx.map();
        // All siblings lay on a single visual line at this width.
        assert_eq!(
            map.lines.len(),
            1,
            "expected single visual line, got {}",
            map.lines.len()
        );
        let line = &map.lines[0];
        for boundary in [3usize, 6usize] {
            let count = line
                .stops
                .iter()
                .filter(|s| s.char_index == boundary)
                .count();
            assert_eq!(
                count, 1,
                "boundary char {boundary} should have a single deduped stop, got {count}",
            );
        }
    }
}

/// Projection branch: emit caret stops for `child_key`'s slice of the
/// TextArea content (`range`). Mirrors `render.rs` / `hit_test.rs` —
/// prefer real glyph stops from the first text-bearing descendant; fall
/// back to one synthesized line per `inline_fragment_rects` entry,
/// distributing the projection's char span across fragments by width.
fn build_projection_lines(
    arena: &NodeArena,
    child_key: NodeKey,
    range: &Range<usize>,
) -> Vec<CaretVisualLine> {
    let span = range.end.saturating_sub(range.start);
    if span == 0 {
        return Vec::new();
    }
    if let Some(lines) = projection_text_lines(arena, child_key, range.start, span) {
        return lines;
    }
    projection_box_lines(arena, child_key, range.start, span)
}

/// DFS the projection subtree for the first `<Text>` / `TextAreaTextRun`
/// and probe its `local_char_to_screen_position` for `local in 0..=span`.
/// Group probes into visual lines by y-band (mirrors `merge_visual_lines`'s
/// half-line overlap rule). Returns `None` when no text-bearing descendant
/// exists *or* every probe came back empty.
fn projection_text_lines(
    arena: &NodeArena,
    root_key: NodeKey,
    char_offset: usize,
    span: usize,
) -> Option<Vec<CaretVisualLine>> {
    let text_key = find_text_descendant(arena, root_key)?;
    let adapter_lines = arena
        .with_element_taken_ref(text_key, |el, _| {
            if let Some(text) = el.as_any().downcast_ref::<Text>() {
                let visible = text.content().chars().count();
                let lines = text
                    .visual_caret_screen_lines()
                    .into_iter()
                    .map(|line| CaretVisualLine {
                        y_top: line.y_top,
                        y_bottom: line.y_bottom,
                        stops: line
                            .stops
                            .into_iter()
                            .filter_map(|stop| {
                                (stop.local_char <= visible.min(span)).then_some(CaretStop {
                                    char_index: char_offset + stop.local_char,
                                    x: stop.x,
                                    y_top: stop.y_top,
                                    height: stop.height,
                                    affinity: None,
                                })
                            })
                            .collect(),
                    })
                    .filter(|line| !line.stops.is_empty())
                    .collect::<Vec<_>>();
                return (!lines.is_empty()).then_some(lines);
            }
            None
        })
        .flatten();
    if adapter_lines.is_some() {
        return adapter_lines;
    }

    let mut probes: Vec<(usize, f32, f32, f32)> = Vec::with_capacity(span + 1);
    for local in 0..=span {
        let probe = arena
            .with_element_taken_ref(text_key, |el, _| {
                if let Some(text) = el.as_any().downcast_ref::<Text>() {
                    let visible = text.content().chars().count();
                    text.local_char_to_screen_position(local.min(visible))
                } else {
                    None
                }
            })
            .flatten();
        if let Some((x, y_top, height)) = probe {
            probes.push((char_offset + local, x, y_top, height));
        }
    }
    if probes.is_empty() {
        return None;
    }
    Some(group_probes_into_visual_lines(probes))
}

fn group_probes_into_visual_lines(probes: Vec<(usize, f32, f32, f32)>) -> Vec<CaretVisualLine> {
    let mut lines: Vec<CaretVisualLine> = Vec::new();
    for (char_index, x, y_top, height) in probes {
        let stop = CaretStop {
            char_index,
            x,
            y_top,
            height,
            affinity: None,
        };
        let merge_into_last = lines
            .last()
            .map(|last| {
                let smaller = (last.y_bottom - last.y_top).min(height).max(1.0);
                let overlap_top = last.y_top.max(y_top);
                let overlap_bottom = last.y_bottom.min(y_top + height);
                let overlap = (overlap_bottom - overlap_top).max(0.0);
                overlap >= smaller * 0.5
            })
            .unwrap_or(false);
        if merge_into_last {
            let last = lines.last_mut().expect("checked above");
            last.y_top = last.y_top.min(y_top);
            last.y_bottom = last.y_bottom.max(y_top + height);
            last.stops.push(stop);
        } else {
            lines.push(CaretVisualLine {
                y_top,
                y_bottom: y_top + height,
                stops: vec![stop],
            });
        }
    }
    lines
}

/// Icon-only / text-less projection: synthesize one line per
/// `inline_fragment_rects` entry (or the union snapshot when the
/// projection root is not fragmentable). The projection's char span is
/// distributed across fragments by width proportion; within a fragment
/// stops are evenly spaced. Adjacent fragments share their boundary
/// char_index so vertical-arrow round-trip across a wrap is symmetric.
fn projection_box_lines(
    arena: &NodeArena,
    root_key: NodeKey,
    char_offset: usize,
    span: usize,
) -> Vec<CaretVisualLine> {
    let rects = arena
        .with_element_taken_ref(root_key, |el, _| {
            if let Some(element) = el.as_any().downcast_ref::<Element>() {
                let frags = element.inline_fragment_rects();
                if !frags.is_empty() {
                    return frags
                        .iter()
                        .map(|r| (r.x, r.y, r.width, r.height))
                        .collect::<Vec<_>>();
                }
            }
            let snap = el.box_model_snapshot();
            vec![(snap.x, snap.y, snap.width, snap.height)]
        })
        .unwrap_or_default();
    if rects.is_empty() {
        return Vec::new();
    }
    let total_w: f32 = rects.iter().map(|(_, _, w, _)| w.max(0.0)).sum();
    let last = rects.len().saturating_sub(1);
    let mut lines: Vec<CaretVisualLine> = Vec::with_capacity(rects.len());
    let mut consumed: usize = 0;
    for (idx, (rx, ry, rw, rh)) in rects.into_iter().enumerate() {
        let chunk = if idx == last {
            span.saturating_sub(consumed)
        } else if total_w > 0.0 {
            let approx = (span as f32 * rw / total_w).round() as usize;
            approx.min(span.saturating_sub(consumed))
        } else {
            0
        };
        let stop_count = chunk + 1;
        let mut stops = Vec::with_capacity(stop_count);
        for s in 0..stop_count {
            let local_char = consumed + s;
            if local_char > span {
                break;
            }
            let frac = if chunk == 0 {
                0.0
            } else {
                (s as f32 / chunk as f32).clamp(0.0, 1.0)
            };
            stops.push(CaretStop {
                char_index: char_offset + local_char,
                x: rx + rw * frac,
                y_top: ry,
                height: rh.max(1.0),
                affinity: None,
            });
        }
        lines.push(CaretVisualLine {
            y_top: ry,
            y_bottom: ry + rh,
            stops,
        });
        consumed += chunk;
    }
    lines
}

fn find_text_descendant(arena: &NodeArena, root_key: NodeKey) -> Option<NodeKey> {
    let root_is_text = arena
        .with_element_taken_ref(root_key, |el, _| {
            el.as_any().is::<Text>() || el.as_any().is::<TextAreaTextRun>()
        })
        .unwrap_or(false);
    if root_is_text {
        return Some(root_key);
    }
    let mut stack: Vec<NodeKey> = arena.children_of(root_key).into_iter().rev().collect();
    while let Some(key) = stack.pop() {
        let is_text = arena
            .with_element_taken_ref(key, |el, _| {
                el.as_any().is::<Text>() || el.as_any().is::<TextAreaTextRun>()
            })
            .unwrap_or(false);
        if is_text {
            return Some(key);
        }
        for child in arena.children_of(key).into_iter().rev() {
            stack.push(child);
        }
    }
    None
}

fn vertical_distance(line: &CaretVisualLine, y: f32) -> f32 {
    if y < line.y_top {
        line.y_top - y
    } else if y > line.y_bottom {
        y - line.y_bottom
    } else {
        0.0
    }
}

fn merge_visual_lines(lines: Vec<CaretVisualLine>) -> Vec<CaretVisualLine> {
    let mut out: Vec<CaretVisualLine> = Vec::with_capacity(lines.len());
    for line in lines {
        let merge_into_last = out
            .last()
            .map(|last| {
                // Same band when vertical extents overlap by more than
                // half of the smaller line's height — keeps stacked runs
                // separate while still merging side-by-side runs that
                // share a y_top within a few pixels of float drift.
                let smaller = (last.y_bottom - last.y_top)
                    .min(line.y_bottom - line.y_top)
                    .max(1.0);
                let overlap_top = last.y_top.max(line.y_top);
                let overlap_bottom = last.y_bottom.min(line.y_bottom);
                let overlap = (overlap_bottom - overlap_top).max(0.0);
                overlap >= smaller * 0.5
            })
            .unwrap_or(false);
        if merge_into_last {
            let last = out.last_mut().expect("checked above");
            last.y_top = last.y_top.min(line.y_top);
            last.y_bottom = last.y_bottom.max(line.y_bottom);
            last.stops.extend(line.stops);
        } else {
            out.push(line);
        }
    }
    out
}

fn normalize_caret_navigation_lines(lines: &mut Vec<CaretVisualLine>) {
    lines.sort_by(|a, b| {
        a.y_top
            .partial_cmp(&b.y_top)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let merged = merge_visual_lines(std::mem::take(lines));
    *lines = merged;
    for line in lines.iter_mut() {
        line.stops
            .sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));
        let mut deduped: Vec<CaretStop> = Vec::with_capacity(line.stops.len());
        for stop in line.stops.drain(..) {
            if let Some(existing) = deduped
                .iter_mut()
                .find(|existing| existing.char_index == stop.char_index)
            {
                if stop.x > existing.x {
                    *existing = stop;
                }
                continue;
            }
            deduped.push(stop);
        }
        line.stops = deduped;
        for stop in line.stops.iter_mut() {
            stop.y_top = line.y_top;
            stop.height = (line.y_bottom - line.y_top).max(1.0);
        }
    }
}
