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
mod tests;

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
