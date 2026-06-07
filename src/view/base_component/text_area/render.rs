//! `Renderable` impl for `TextArea`.
//!
//! `TextAreaTextRun`'s Renderable lives in [`super::run`] — it owns the
//! glyph buffer and emits the actual `TextPass`.
//!
//! Render layer order (per design):
//!   Layer 0 — selection background  (P3.5b)
//!   Layer 1 — children (Run glyphs / projection self-render)
//!   Layer 2 — caret                  (P3.5a, this file)

use std::time::Duration;

use crate::style::ColorLike;
use crate::ui::Rect;
use crate::view::base_component::{
    BuildState, DirtyFlags, Renderable, TextAreaSelectionRenderContext, UiBuildContext,
    round_layout_value, with_text_area_selection_render_context,
};
use crate::view::frame_graph::FrameGraph;
use crate::view::node_arena::{NodeArena, NodeKey};
use crate::view::render_pass::DrawRectPass;
use crate::view::render_pass::draw_rect_pass::{
    DrawRectInput, DrawRectOutput, RectPassParams, RenderTargetIn,
};
use crate::view::render_pass::text_pass::{
    TextInput, TextOutput, TextPassPreparedFragment, TextPassPreparedParams, TextPreparedInputPass,
};

use super::TextArea;
use super::run::{TextAreaLineBreak, TextAreaTextRun};
use crate::view::base_component::Text;

const CARET_BLINK_PERIOD: Duration = Duration::from_millis(1060);
const CARET_BLINK_VISIBLE: Duration = Duration::from_millis(530);
const CARET_WIDTH: f32 = 1.0;

impl TextArea {
    /// Caret is drawn only while focused, blinking at the standard cadence.
    pub(super) fn should_draw_caret(&self) -> bool {
        if !self.is_focused {
            return false;
        }
        let elapsed = self.caret_blink_started_at.elapsed().as_millis();
        let period = CARET_BLINK_PERIOD.as_millis();
        let visible = CARET_BLINK_VISIBLE.as_millis();
        (elapsed % period) < visible
    }

    /// Resolve `cursor_char` to a screen-space `(x, y_top, line_height)`.
    ///
    /// Walks `children` for a `TextAreaTextRun` whose `char_range` covers
    /// the cursor (boundary cases prefer the *following* Run per the caret
    /// boundary rules). Falls back to TextArea's own layout origin when
    /// no Run exists (empty content, no placeholder).
    pub(super) fn caret_screen_position(&self, arena: &NodeArena) -> Option<(f32, f32, f32)> {
        if self.children.is_empty() {
            // No child Run yet — caret pinned to TextArea's own origin.
            return Some((
                self.layout_state.layout_position.x,
                self.layout_state.layout_position.y,
                self.font_size.max(1.0) * self.line_height,
            ));
        }

        for &child_key in self.children.iter() {
            if let Some(pos) = arena
                .with_element_taken_ref(child_key, |el, _| {
                    let run = el.as_any().downcast_ref::<TextAreaTextRun>()?;
                    if run.inline_preedit.is_none() && !run.is_preedit_run() {
                        return None;
                    }
                    let (x, y_top, lh) = run.preedit_caret_local_position()?;
                    Some((
                        run.layout_state.layout_position.x + x,
                        run.layout_state.layout_position.y + y_top,
                        lh,
                    ))
                })
                .flatten()
            {
                return Some(pos);
            }
        }

        let cursor_host_is_projection = self
            .child_char_ranges
            .iter()
            .enumerate()
            .find_map(|(idx, range)| {
                (self.cursor_char >= range.start && self.cursor_char < range.end).then_some(idx)
            })
            .and_then(|idx| self.children.get(idx).copied())
            .map(|key| {
                !arena
                    .with_element_taken_ref(key, |el, _| {
                        el.as_any().is::<TextAreaTextRun>() || el.as_any().is::<TextAreaLineBreak>()
                    })
                    .unwrap_or(false)
            })
            .unwrap_or(false);
        if !cursor_host_is_projection {
            let map = super::caret_map::CaretNavigationMap::build(self, arena);
            if let Some(stop) = map.caret_stop_for_char(self.cursor_char, self.cursor_affinity) {
                return Some((stop.x, stop.y_top, stop.height));
            }
        }

        // Fallback for projection-hosted carets and legacy callers:
        // walk children in order, first child whose half-open range
        // contains the cursor wins. Boundary positions prefer the
        // following child (`cursor == projection.start` belongs to that
        // projection), with tail-of-content falling back to the last text
        // run or line break.
        let mut chosen_idx: Option<usize> = None;
        let mut last_text_idx: Option<usize> = None;
        for (idx, child_range) in self.child_char_ranges.iter().enumerate() {
            let &child_key = self.children.get(idx)?;
            let is_text = arena
                .with_element_taken_ref(child_key, |el, _| {
                    el.as_any().is::<TextAreaTextRun>() || el.as_any().is::<TextAreaLineBreak>()
                })
                .unwrap_or(false);
            if is_text {
                last_text_idx = Some(idx);
            }
            if chosen_idx.is_none()
                && self.cursor_char >= child_range.start
                && self.cursor_char < child_range.end
            {
                chosen_idx = Some(idx);
                break;
            }
        }
        let idx = chosen_idx.or(last_text_idx)?;
        let &key = self.children.get(idx)?;
        let range = self.child_char_ranges.get(idx)?.clone();
        let line_h = self.font_size.max(1.0) * self.line_height;

        // Branch on host kind without holding a take-borrow on `key`,
        // since the projection branch needs to DFS the same subtree
        // (calling `with_element_taken_ref(key, ...)` recursively would
        // deadlock on the host slot).
        let host_is_text = arena
            .with_element_taken_ref(key, |el, _| {
                el.as_any().is::<TextAreaTextRun>() || el.as_any().is::<TextAreaLineBreak>()
            })
            .unwrap_or(false);

        if host_is_text {
            return arena.with_element_taken_ref(key, |el, _| {
                if let Some(line_break) = el.as_any().downcast_ref::<TextAreaLineBreak>() {
                    let local = self.cursor_char.saturating_sub(range.start).min(1);
                    let line = line_break
                        .caret_stops()
                        .into_iter()
                        .find(|line| line.stops.iter().any(|stop| stop.local_char == local))?;
                    let stop = line
                        .stops
                        .into_iter()
                        .find(|stop| stop.local_char == local)?;
                    return Some((
                        line_break.layout_state.layout_position.x + stop.local_x,
                        line_break.layout_state.layout_position.y + stop.local_y_top,
                        stop.height,
                    ));
                }
                let run = el.as_any().downcast_ref::<TextAreaTextRun>()?;
                let (x, y_top, lh) = if run.inline_preedit.is_some() {
                    run.preedit_caret_local_position()?
                } else {
                    let start = run.char_range.start;
                    let visible_chars = run.text.chars().count();
                    let local = self.cursor_char.saturating_sub(start).min(visible_chars);
                    run.local_char_to_screen_position_with_affinity(local, self.cursor_affinity)?
                };
                let screen_x = run.layout_state.layout_position.x + x;
                let screen_y = run.layout_state.layout_position.y + y_top;
                Some((screen_x, screen_y, lh))
            })?;
        }

        // Projection host: prefer real glyph coordinates from the first
        // text-bearing descendant. For image/icon-only projections, fall
        // back to proportional positioning inside the projection root box.
        //
        // Affinity disambiguation lives at the TextArea layer, not in
        // the inner Text — that's why we *post-process* the projection
        // descendant's reported caret position here. When the user
        // explicitly chose `Upstream` (e.g. Cmd+Right that lands at the
        // head of a wrapped visual line) and the descendant's caret
        // sits at the lower line's head, walk the
        // `CaretNavigationMap` to find the corresponding upper-line
        // tail stop and prefer that. This preserves the Cocoa rule
        // without requiring `Text` to know about caret affinity.
        let span = range.end.saturating_sub(range.start);
        let local_char = self.projection_caret_local_char(range.start, span);
        if let Some(found) = glyph_caret_in_projection(arena, key, local_char, self.cursor_affinity)
        {
            if let Some(override_pos) = self.projection_caret_affinity_override(arena, key, found.1)
            {
                return Some(override_pos);
            }
            return Some(found);
        }
        let snap = arena.with_element_taken_ref(key, |el, _| el.box_model_snapshot())?;
        let ratio = if span == 0 {
            0.0
        } else {
            (local_char as f32 / span as f32).clamp(0.0, 1.0)
        };
        let x = snap.x + snap.width * ratio;
        let caret_h = line_h.max(1.0);
        let y = snap.y + (snap.height - caret_h).max(0.0) * 0.5;
        Some((x, y, caret_h))
    }

    /// Post-process the descendant's reported caret position to honour
    /// `cursor_affinity`. The boundary char between two wrapped visual
    /// lines is logically one source char index but visually has two
    /// caret slots — affinity decides which slot to render:
    ///
    ///   * `cursor_char` IS line N's last stop AND a continuation line
    ///     N+1 exists:
    ///       - `Upstream`   → upper line's tail (descendant already
    ///                        reports this; no override needed).
    ///       - `Downstream` → lower line's head from the projection's
    ///                        first text-bearing descendant.
    ///   * `cursor_char` IS line N+1's first stop (CJK shared boundary
    ///     where the same source char appears on both lines):
    ///       - `Upstream`   → upper line's tail.
    ///       - `Downstream` → descendant's report (= lower head).
    ///
    /// Falls through to a y-mismatch repair when neither case applies.
    fn projection_caret_affinity_override(
        &self,
        arena: &NodeArena,
        projection_key: NodeKey,
        descendant_y: f32,
    ) -> Option<(f32, f32, f32)> {
        use super::caret_map::{CaretAffinity, CaretNavigationMap};
        let affinity = self.cursor_affinity;
        let map = CaretNavigationMap::build(self, arena);
        let line_idx = map.line_index_for_char(self.cursor_char, affinity)?;
        let line = map.lines.get(line_idx)?;

        // Upstream cursor at the head of a non-leading visual line →
        // pin to upper tail (CJK shared boundary case).
        if affinity == CaretAffinity::Upstream
            && line_idx > 0
            && line.stops.first().map(|s| s.char_index) == Some(self.cursor_char)
        {
            let upper_tail = map.lines.get(line_idx - 1)?.stops.last()?;
            return Some((upper_tail.x, upper_tail.y_top, upper_tail.height));
        }

        // Downstream cursor at the tail of a *multi-stop* visual line
        // that has a continuation → pin to the lower line's head from
        // the projection's text-bearing descendant. Without this, the
        // descendant's `local_char_to_screen_position` always returns
        // the upper-fragment tail at this source char (its `<= frag_chars`
        // match keeps the boundary char on the prior fragment), so the
        // caret can't reach the visual lower-line head via Downstream.
        //
        // The `len() >= 2` guard skips degenerate single-char lines
        // where every char is simultaneously line head and line tail —
        // there's no genuine "after the last visible glyph" position
        // in those, and firing the override would shift the caret
        // forward by a whole visual line for ordinary mid-line moves.
        if affinity == CaretAffinity::Downstream
            && line.stops.len() >= 2
            && line.stops.last().map(|s| s.char_index) == Some(self.cursor_char)
            && let Some(next_line) = map.lines.get(line_idx + 1)
            && let Some(pos) =
                projection_lower_fragment_head(arena, projection_key, next_line.y_top)
        {
            return Some(pos);
        }

        // Fallback: the descendant reported a `y` that disagrees with
        // the map (e.g. legacy `Text` inline path snapping a gap byte
        // to the wrong fragment). Re-anchor to whichever map stop
        // matches `cursor_char` on the affinity-resolved line.
        let line_height = (line.y_bottom - line.y_top).max(1.0);
        if (descendant_y - line.y_top).abs() > line_height * 0.5
            && let Some(stop) = line.stops.iter().find(|s| s.char_index == self.cursor_char)
        {
            return Some((stop.x, stop.y_top, stop.height));
        }
        None
    }

    fn projection_caret_local_char(
        &self,
        projection_start: usize,
        projection_span: usize,
    ) -> usize {
        let base = self
            .cursor_char
            .saturating_sub(projection_start)
            .min(projection_span);
        if self.ime_preedit.is_empty() {
            return base;
        }
        base + preedit_cursor_char_offset(self.ime_preedit.as_str(), self.ime_preedit_cursor)
    }

    /// Walk Run children, collect each Run's preedit underline rects, and
    /// translate to screen coords. When the caret sits inside a projection,
    /// TextArea only draws the IME underline overlay inside the projection;
    /// the projection remains responsible for text rendering.
    fn preedit_underline_screen_rects(&self, arena: &NodeArena) -> Vec<Rect> {
        if !self.ime_preedit.is_empty()
            && let Some(rects) = self.projection_preedit_underline_screen_rects(arena)
        {
            return rects;
        }

        let mut out = Vec::new();
        for &child_key in self.children.iter() {
            arena.with_element_taken_ref(child_key, |el, _| {
                let Some(run) = el.as_any().downcast_ref::<TextAreaTextRun>() else {
                    return;
                };
                let origin_x = run.layout_state.layout_position.x;
                let origin_y = run.layout_state.layout_position.y;
                for r in run.preedit_underline_rects() {
                    out.push(Rect {
                        x: origin_x + r.x,
                        y: origin_y + r.y,
                        width: r.width,
                        height: r.height,
                    });
                }
            });
        }
        out
    }

    fn projection_preedit_underline_screen_rects(&self, arena: &NodeArena) -> Option<Vec<Rect>> {
        let preedit_chars = self.ime_preedit.chars().count();
        if preedit_chars == 0 {
            return None;
        }
        let cursor = self.cursor_char.min(self.content.chars().count());
        for (idx, range) in self.child_char_ranges.iter().enumerate() {
            if cursor < range.start || cursor >= range.end {
                continue;
            }
            let &child_key = self.children.get(idx)?;
            let is_projection = arena
                .with_element_taken_ref(child_key, |el, _| {
                    !el.as_any().is::<TextAreaTextRun>() && !el.as_any().is::<TextAreaLineBreak>()
                })
                .unwrap_or(false);
            if !is_projection {
                return None;
            }

            let local_start = cursor.saturating_sub(range.start);
            let local_end = local_start + preedit_chars;
            if let Some(rects) =
                glyph_selection_rects_in_projection(arena, child_key, local_start, local_end)
            {
                let underlines = rects
                    .into_iter()
                    .map(|rect| Rect {
                        x: rect.x,
                        y: rect.y + rect.height.max(1.0) - 1.0,
                        width: rect.width.max(1.0),
                        height: 1.0,
                    })
                    .collect::<Vec<_>>();
                if !underlines.is_empty() {
                    return Some(underlines);
                }
            }

            let local_caret = self
                .projection_caret_local_char(range.start, range.end.saturating_sub(range.start));
            if let Some((x, y, line_h)) =
                glyph_caret_in_projection(arena, child_key, local_caret, self.cursor_affinity)
            {
                let width = (self.font_size.max(1.0) * 0.6 * preedit_chars as f32).max(1.0);
                return Some(vec![Rect {
                    x,
                    y: y + line_h.max(1.0) - 1.0,
                    width,
                    height: 1.0,
                }]);
            }
            return None;
        }
        None
    }

    /// Walk Run children whose `char_range` overlaps the selection range.
    /// Projection selections are rendered by the projection's Text children
    /// via `TextAreaSelectionRenderContext` so they appear above projection
    /// backgrounds and below projection text.
    fn selection_screen_rects(&self, arena: &NodeArena) -> Vec<Rect> {
        let Some((sel_start, sel_end)) = self.selection_range_chars() else {
            return Vec::new();
        };
        if let Some(package) = self.unified_inline_ifc_render_package(arena) {
            let origin_x = self.layout_state.layout_position.x - self.scroll_x;
            let origin_y = self.layout_state.layout_position.y - self.scroll_y;
            return package
                .selection_rects_for_char_range(sel_start..sel_end)
                .into_iter()
                .map(|rect| Rect {
                    x: origin_x + rect.x,
                    y: origin_y + rect.y,
                    width: rect.width,
                    height: rect.height,
                })
                .collect();
        }
        let mut out = Vec::new();
        for (idx, &child_key) in self.children.iter().enumerate() {
            let Some(child_range) = self.child_char_ranges.get(idx).cloned() else {
                continue;
            };
            if child_range.end <= sel_start || child_range.start >= sel_end {
                continue;
            }

            let is_run = arena
                .with_element_taken_ref(child_key, |el, _| el.as_any().is::<TextAreaTextRun>())
                .unwrap_or(false);
            if is_run {
                arena.with_element_taken_ref(child_key, |el, _| {
                    let Some(run) = el.as_any().downcast_ref::<TextAreaTextRun>() else {
                        return;
                    };
                    let cr = run.char_range.clone();
                    let local_start = sel_start.saturating_sub(cr.start);
                    let local_end = sel_end.saturating_sub(cr.start).min(cr.end - cr.start);
                    let origin_x = run.layout_state.layout_position.x;
                    let origin_y = run.layout_state.layout_position.y;
                    for r in run.local_selection_rects(local_start, local_end) {
                        out.push(Rect {
                            x: origin_x + r.x,
                            y: origin_y + r.y,
                            width: r.width,
                            height: r.height,
                        });
                    }
                });
                continue;
            }
        }
        out
    }

    pub(super) fn projection_selection_context_for_child(
        &self,
        idx: usize,
        child_key: NodeKey,
        arena: &NodeArena,
    ) -> Option<TextAreaSelectionRenderContext> {
        let (sel_start, sel_end) = self.selection_range_chars()?;
        let range = self.child_char_ranges.get(idx)?;
        if range.end <= sel_start || range.start >= sel_end {
            return None;
        }
        let is_projection = arena
            .with_element_taken_ref(child_key, |el, _| !el.as_any().is::<TextAreaTextRun>())
            .unwrap_or(false);
        if !is_projection {
            return None;
        }
        let local_start = sel_start.saturating_sub(range.start);
        let local_end = sel_end
            .saturating_sub(range.start)
            .min(range.end.saturating_sub(range.start));
        if local_start >= local_end {
            return None;
        }
        Some(TextAreaSelectionRenderContext {
            start: local_start,
            end: local_end,
            fill: self.selection_background_color.to_rgba_f32(),
        })
    }

    fn content_paint_anchor(&self, arena: &NodeArena) -> Option<(f32, f32)> {
        self.children.iter().find_map(|&child_key| {
            arena.with_element_taken_ref(child_key, |el, _| {
                let snap = el.box_model_snapshot();
                snap.should_render.then_some((snap.x, snap.y))
            })?
        })
    }
}

impl Renderable for TextArea {
    fn build(
        &mut self,
        graph: &mut FrameGraph,
        arena: &mut NodeArena,
        mut ctx: UiBuildContext,
    ) -> BuildState {
        let parent_paint_offset = ctx.paint_offset();
        let [paint_offset_x, paint_offset_y] = parent_paint_offset;
        let paint_x = self.layout_state.layout_position.x + paint_offset_x;
        let paint_y = self.layout_state.layout_position.y + paint_offset_y;
        ctx.translate_paint_offset(
            round_layout_value(paint_x) - paint_x,
            round_layout_value(paint_y) - paint_y,
        );

        if let Some((content_x, content_y)) = self.content_paint_anchor(arena) {
            let [paint_offset_x, paint_offset_y] = ctx.paint_offset();
            let paint_x = content_x + paint_offset_x;
            let paint_y = content_y + paint_offset_y;
            ctx.translate_paint_offset(
                round_layout_value(paint_x) - paint_x,
                round_layout_value(paint_y) - paint_y,
            );
        }

        let previous_scissor = ctx.push_scissor_rect(self.viewport_scissor_rect());

        // Layer 0 — selection background. Drawn under children so glyphs
        // overlay the highlight.
        if let Some(target) = ctx.current_target() {
            let fill = self.selection_background_color.to_rgba_f32();
            for rect in self.selection_screen_rects(arena) {
                let [x, y] = ctx.paint_point(rect.x, rect.y);
                let mut sel_pass = DrawRectPass::new(
                    RectPassParams {
                        position: [x, y],
                        size: [rect.width.max(1.0), rect.height.max(1.0)],
                        fill_color: fill,
                        opacity: 1.0,
                        ..Default::default()
                    },
                    DrawRectInput {
                        pass_context: ctx.graphics_pass_context(),
                        ..Default::default()
                    },
                    DrawRectOutput {
                        render_target: target,
                        ..Default::default()
                    },
                );
                sel_pass.set_input(
                    target
                        .handle()
                        .map(RenderTargetIn::with_handle)
                        .unwrap_or_default(),
                );
                graph.add_graphics_pass(sel_pass);
            }
            ctx.set_current_target(target);
        }

        let unified_render_package = self.unified_inline_ifc_render_package(arena);
        if let (Some(package), Some(target)) = (&unified_render_package, ctx.current_target()) {
            let [origin_x, origin_y] = ctx.paint_point(
                self.layout_state.layout_position.x - self.scroll_x,
                self.layout_state.layout_position.y - self.scroll_y,
            );
            let staging_input = package.text_pass_staging_input([origin_x, origin_y], 1.0, 0, 1.0);
            if !staging_input.glyphs.is_empty() {
                let content_rect = package.content_rect();
                let size = content_rect
                    .map(|rect| [rect.width.max(1.0), rect.height.max(1.0)])
                    .unwrap_or([
                        self.layout_state.layout_size.width.max(1.0),
                        self.layout_state.layout_size.height.max(1.0),
                    ]);
                let pass = TextPreparedInputPass::new(
                    TextPassPreparedParams {
                        staging_input,
                        fragments: vec![TextPassPreparedFragment {
                            origin: [origin_x, origin_y],
                            size,
                        }],
                        scissor_rect: None,
                        stencil_clip_id: None,
                    },
                    TextInput {
                        pass_context: ctx.graphics_pass_context(),
                    },
                    TextOutput {
                        render_target: target,
                        ..Default::default()
                    },
                );
                graph.add_graphics_pass(pass);
                ctx.set_current_target(target);
            }
        }

        // Layer 1 — walk arena children (Run / projection self-render).
        //
        // TextArea is promotion-aware (Phase 2): a child that ends up in
        // the promoted set goes through `Element::build_promoted_child`,
        // which allocates its own layer target, runs the build into it,
        // and composites the layer back onto TextArea's current target.
        // Non-promoted children render inline directly.
        let child_keys: Vec<NodeKey> = self.children.clone();
        for (idx, child_key) in child_keys.into_iter().enumerate() {
            if unified_render_package.is_some()
                && arena
                    .with_element_taken_ref(child_key, |el, _| {
                        el.as_any().is::<TextAreaTextRun>() || el.as_any().is::<TextAreaLineBreak>()
                    })
                    .unwrap_or(false)
            {
                continue;
            }
            let selection_context =
                self.projection_selection_context_for_child(idx, child_key, arena);
            let child_promoted = arena
                .get(child_key)
                .map(|n| ctx.is_node_promoted(n.element.stable_id()))
                .unwrap_or(false);
            if child_promoted {
                with_text_area_selection_render_context(selection_context, || {
                    crate::view::base_component::Element::build_promoted_child(
                        graph, arena, &mut ctx, child_key, None,
                    );
                });
                continue;
            }
            let viewport = ctx.viewport();
            let taken_state = ctx.state_clone();
            let ctx_in = UiBuildContext::from_parts(viewport.clone(), taken_state);
            let next_ctx = with_text_area_selection_render_context(selection_context, || {
                arena.with_element_taken(child_key, |child, arena| {
                    let ctx_local = ctx_in;
                    let vp = ctx_local.viewport();
                    let next_state = child.build(graph, arena, ctx_local);
                    UiBuildContext::from_parts(vp, next_state)
                })
            });
            if let Some(c) = next_ctx {
                ctx = c;
            }
        }

        // Layer 1.5 — IME preedit underline (above glyphs, below caret).
        if let Some(target) = ctx.current_target() {
            let stroke = self.color.to_rgba_f32();
            for rect in self.preedit_underline_screen_rects(arena) {
                let [x, y] = ctx.paint_point(rect.x, rect.y);
                let mut underline_pass = DrawRectPass::new(
                    RectPassParams {
                        position: [x, y],
                        size: [rect.width.max(1.0), rect.height.max(1.0)],
                        fill_color: stroke,
                        opacity: 1.0,
                        ..Default::default()
                    },
                    DrawRectInput {
                        pass_context: ctx.graphics_pass_context(),
                        ..Default::default()
                    },
                    DrawRectOutput {
                        render_target: target,
                        ..Default::default()
                    },
                );
                underline_pass.set_input(
                    target
                        .handle()
                        .map(RenderTargetIn::with_handle)
                        .unwrap_or_default(),
                );
                graph.add_graphics_pass(underline_pass);
            }
            ctx.set_current_target(target);
        }

        // Layer 2 — caret.
        if self.should_draw_caret()
            && let Some((cx, cy, line_height)) = self.caret_screen_position(arena)
            && let Some(target) = ctx.current_target()
        {
            let [x, y] = ctx.paint_point(cx, cy);
            let mut caret_pass = DrawRectPass::new(
                RectPassParams {
                    position: [x, y],
                    size: [CARET_WIDTH, line_height.max(1.0)],
                    fill_color: self.color.to_rgba_f32(),
                    opacity: 1.0,
                    ..Default::default()
                },
                DrawRectInput {
                    pass_context: ctx.graphics_pass_context(),
                    ..Default::default()
                },
                DrawRectOutput {
                    render_target: target,
                    ..Default::default()
                },
            );
            caret_pass.set_input(
                target
                    .handle()
                    .map(RenderTargetIn::with_handle)
                    .unwrap_or_default(),
            );
            graph.add_graphics_pass(caret_pass);
            ctx.set_current_target(target);
        }

        self.dirty_flags = self.dirty_flags.without(DirtyFlags::PAINT);
        ctx.restore_scissor_rect(previous_scissor);
        ctx.set_paint_offset(parent_paint_offset);
        ctx.into_state()
    }
}

impl TextArea {
    fn viewport_scissor_rect(&self) -> Option<[u32; 4]> {
        let rect = Rect {
            x: self.layout_state.layout_position.x,
            y: self.layout_state.layout_position.y,
            width: self.viewport_size.width,
            height: self.viewport_size.height,
        };
        rect_to_scissor_rect(rect)
    }
}

fn rect_to_scissor_rect(rect: Rect) -> Option<[u32; 4]> {
    let left = rect.x.floor().max(0.0) as i64;
    let top = rect.y.floor().max(0.0) as i64;
    let right = (rect.x + rect.width).ceil().max(0.0) as i64;
    let bottom = (rect.y + rect.height).ceil().max(0.0) as i64;
    if right <= left || bottom <= top {
        return None;
    }
    Some([
        left as u32,
        top as u32,
        (right - left) as u32,
        (bottom - top) as u32,
    ])
}

/// DFS the projection subtree rooted at `root_key` for the first
/// text-bearing element (a `<Text>` or a `TextAreaTextRun`) and query
/// its glyph buffer for the screen-space caret position at `local_char`
/// (0-based char offset into the projected slice).
/// Resolve the caret position at a wrapped projection's *lower line
/// head* — the leading edge of the visual line whose top edge matches
/// `target_y`. DFS the projection subtree for the first text-bearing
/// descendant, ask it for `visual_line_heads()`, and pick the entry
/// whose y matches `target_y`. The descendant's heads already include
/// inline-fragment offsets (Text inline path) or the descendant's own
/// `layout_position` (Text block path / `TextAreaTextRun`), so the
/// returned position is screen-space.
fn projection_lower_fragment_head(
    arena: &NodeArena,
    root_key: NodeKey,
    target_y: f32,
) -> Option<(f32, f32, f32)> {
    let heads = collect_projection_visual_line_heads(arena, root_key);
    heads
        .into_iter()
        .min_by(|a, b| {
            (a.1 - target_y)
                .abs()
                .partial_cmp(&(b.1 - target_y).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .filter(|head| (head.1 - target_y).abs() < head.2)
}

fn collect_projection_visual_line_heads(
    arena: &NodeArena,
    root_key: NodeKey,
) -> Vec<(f32, f32, f32)> {
    fn extract(arena: &NodeArena, key: NodeKey) -> Option<Vec<(f32, f32, f32)>> {
        arena
            .with_element_taken_ref(key, |el, _| {
                if let Some(text) = el.as_any().downcast_ref::<Text>() {
                    return Some(text.visual_line_heads());
                }
                if let Some(run) = el.as_any().downcast_ref::<TextAreaTextRun>() {
                    let origin_x = run.layout_state.layout_position.x;
                    let origin_y = run.layout_state.layout_position.y;
                    return Some(
                        run.caret_stops()
                            .into_iter()
                            .filter_map(|line| {
                                let head = line.stops.first()?;
                                Some((
                                    origin_x + head.local_x,
                                    origin_y + head.local_y_top,
                                    head.height,
                                ))
                            })
                            .collect(),
                    );
                }
                None
            })
            .flatten()
    }

    if let Some(heads) = extract(arena, root_key)
        && !heads.is_empty()
    {
        return heads;
    }
    let mut stack: Vec<NodeKey> = arena.children_of(root_key).into_iter().rev().collect();
    while let Some(key) = stack.pop() {
        if let Some(heads) = extract(arena, key)
            && !heads.is_empty()
        {
            return heads;
        }
        for child in arena.children_of(key).into_iter().rev() {
            stack.push(child);
        }
    }
    Vec::new()
}

fn glyph_caret_in_projection(
    arena: &NodeArena,
    root_key: NodeKey,
    local_char: usize,
    affinity: super::caret_map::CaretAffinity,
) -> Option<(f32, f32, f32)> {
    if let Some(found) = query_caret_on(arena, root_key, local_char, affinity) {
        return Some(found);
    }
    let mut stack: Vec<NodeKey> = arena.children_of(root_key).into_iter().rev().collect();
    while let Some(key) = stack.pop() {
        if let Some(found) = query_caret_on(arena, key, local_char, affinity) {
            return Some(found);
        }
        for child in arena.children_of(key).into_iter().rev() {
            stack.push(child);
        }
    }
    None
}

fn query_caret_on(
    arena: &NodeArena,
    key: NodeKey,
    local_char: usize,
    affinity: super::caret_map::CaretAffinity,
) -> Option<(f32, f32, f32)> {
    arena
        .with_element_taken_ref(key, |el, _| {
            if let Some(run) = el.as_any().downcast_ref::<TextAreaTextRun>() {
                let visible = run.text.chars().count();
                let local = local_char.min(visible);
                let (x, y_top, lh) =
                    run.local_char_to_screen_position_with_affinity(local, affinity)?;
                return Some((
                    run.layout_state.layout_position.x + x,
                    run.layout_state.layout_position.y + y_top,
                    lh,
                ));
            }
            if let Some(text) = el.as_any().downcast_ref::<Text>() {
                let visible = text.content().chars().count();
                let local = local_char.min(visible);
                return text.local_char_to_screen_position_with_affinity(
                    local,
                    affinity == super::caret_map::CaretAffinity::Upstream,
                );
            }
            None
        })
        .flatten()
}

fn glyph_selection_rects_in_projection(
    arena: &NodeArena,
    root_key: NodeKey,
    local_start: usize,
    local_end: usize,
) -> Option<Vec<Rect>> {
    if let Some(found) = query_selection_on(arena, root_key, local_start, local_end) {
        return Some(found);
    }
    let mut stack: Vec<NodeKey> = arena.children_of(root_key).into_iter().rev().collect();
    while let Some(key) = stack.pop() {
        if let Some(found) = query_selection_on(arena, key, local_start, local_end) {
            return Some(found);
        }
        for child in arena.children_of(key).into_iter().rev() {
            stack.push(child);
        }
    }
    None
}

fn query_selection_on(
    arena: &NodeArena,
    key: NodeKey,
    local_start: usize,
    local_end: usize,
) -> Option<Vec<Rect>> {
    arena
        .with_element_taken_ref(key, |el, _| {
            if let Some(run) = el.as_any().downcast_ref::<TextAreaTextRun>() {
                let visible = run.text.chars().count();
                let start = local_start.min(visible);
                let end = local_end.min(visible);
                let origin_x = run.layout_state.layout_position.x;
                let origin_y = run.layout_state.layout_position.y;
                let rects = run
                    .local_selection_rects(start, end)
                    .into_iter()
                    .map(|rect| Rect {
                        x: origin_x + rect.x,
                        y: origin_y + rect.y,
                        width: rect.width,
                        height: rect.height,
                    })
                    .collect();
                return Some(rects);
            }
            if let Some(text) = el.as_any().downcast_ref::<Text>() {
                let visible = text.content().chars().count();
                let start = local_start.min(visible);
                let end = local_end.min(visible);
                return Some(text.local_selection_screen_rects(start, end));
            }
            None
        })
        .flatten()
}

fn preedit_cursor_char_offset(preedit: &str, cursor: Option<(usize, usize)>) -> usize {
    let byte = cursor
        .map(|(_, end)| clamp_utf8_boundary(preedit, end))
        .unwrap_or(preedit.len());
    preedit[..byte].chars().count()
}

fn clamp_utf8_boundary(value: &str, mut byte_index: usize) -> usize {
    byte_index = byte_index.min(value.len());
    while byte_index > 0 && !value.is_char_boundary(byte_index) {
        byte_index -= 1;
    }
    byte_index
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::Length;
    use crate::ui::{RsxNode, RsxTagDescriptor};
    use crate::view::ElementStylePropSchema;
    use crate::view::base_component::{ElementTrait, LayoutConstraints, LayoutPlacement, Text};
    use crate::view::frame_graph::FrameGraph;

    fn projection_fixture(cursor_char: usize, with_text_child: bool) -> (NodeArena, NodeKey) {
        let mut text_area = TextArea::new();
        text_area.content = "abXYZcd".to_string();
        text_area.font_size = 14.0;
        text_area.line_height = 1.25;
        text_area.cursor_char = cursor_char;
        text_area.on_render_handler = Some(crate::ui::on_text_area_render(move |render| {
            render.range(2..5, |_text_area_node| {
                let style = ElementStylePropSchema {
                    width: Some(Length::px(90.0)),
                    height: Some(Length::px(42.0)),
                    ..Default::default()
                };
                let node = RsxNode::tagged(
                    "Element",
                    RsxTagDescriptor::for_tag::<crate::view::tags::Element>(),
                )
                .with_prop("style", style);
                if with_text_child {
                    node.with_child(
                        RsxNode::tagged(
                            "Text",
                            RsxTagDescriptor::for_tag::<crate::view::tags::Text>(),
                        )
                        .with_child(RsxNode::text("XYZ")),
                    )
                } else {
                    node
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
                max_width: 300.0,
                max_height: 300.0,
                viewport_width: 300.0,
                viewport_height: 300.0,
                percent_base_width: None,
                percent_base_height: None,
            },
            LayoutPlacement {
                parent_x: 10.0,
                parent_y: 20.0,
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
        (arena, root)
    }

    #[test]
    fn viewport_scissor_uses_text_area_viewport_not_content_width() {
        let mut text_area = TextArea::new();
        text_area.layout_state.layout_position =
            crate::view::base_component::Position { x: 10.2, y: 20.6 };
        text_area.viewport_size = crate::view::base_component::Size {
            width: 120.1,
            height: 40.2,
        };
        text_area.layout_state.content_size = crate::view::base_component::Size {
            width: 360.0,
            height: 90.0,
        };

        assert_eq!(text_area.viewport_scissor_rect(), Some([10, 20, 121, 41]));
    }

    #[test]
    fn text_area_build_restores_viewport_scissor() {
        let (mut arena, root) = projection_fixture(3, true);
        crate::view::test_support::measure_and_place(
            &mut arena,
            root,
            LayoutConstraints {
                max_width: 90.0,
                max_height: 36.0,
                viewport_width: 90.0,
                viewport_height: 36.0,
                percent_base_width: Some(90.0),
                percent_base_height: Some(36.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 90.0,
                available_height: 36.0,
                viewport_width: 90.0,
                viewport_height: 36.0,
                percent_base_width: Some(90.0),
                percent_base_height: Some(36.0),
            },
        );

        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(160, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let target = ctx.allocate_target(&mut graph);
        ctx.set_current_target(target);
        let ctx_for_build = UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone());
        let next_state = arena
            .with_element_taken(root, |el, a| el.build(&mut graph, a, ctx_for_build))
            .expect("TextArea build returns state");
        ctx.set_state(next_state);

        assert_eq!(
            ctx.graphics_pass_context().scissor_rect,
            None,
            "TextArea viewport scissor must not leak to sibling roots",
        );
    }

    #[test]
    fn text_area_inline_ifc_projection_unified_render_skips_per_run_text_passes() {
        let (mut arena, root) = projection_fixture(3, false);

        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(320, 200, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let target = ctx.allocate_target(&mut graph);
        ctx.set_current_target(target);
        let ctx_for_build = UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone());
        arena
            .with_element_taken(root, |el, a| el.build(&mut graph, a, ctx_for_build))
            .expect("TextArea build returns state");

        let pass_names = graph
            .pass_descriptors()
            .into_iter()
            .map(|desc| desc.name.to_string())
            .collect::<Vec<_>>();
        let prepared_text_pass_count = pass_names
            .iter()
            .filter(|name| name.ends_with("render_pass::text_pass::TextPreparedInputPass"))
            .count();
        assert_eq!(
            prepared_text_pass_count, 1,
            "projection TextArea should render plain glyphs once from the TextArea-level unified IFC package, got {pass_names:?}"
        );
    }

    fn caret_position(arena: &NodeArena, root: NodeKey) -> (f32, f32, f32) {
        arena
            .with_element_taken_ref(root, |el, arena| {
                el.as_any()
                    .downcast_ref::<TextArea>()
                    .expect("TextArea root")
                    .caret_screen_position(arena)
                    .expect("caret position")
            })
            .expect("root exists")
    }

    fn projection_key(arena: &NodeArena, root: NodeKey) -> NodeKey {
        arena.children_of(root)[1]
    }

    fn projection_snapshot(
        arena: &NodeArena,
        root: NodeKey,
    ) -> crate::view::base_component::BoxModelSnapshot {
        let projection_key = projection_key(arena, root);
        arena
            .get(projection_key)
            .expect("projection child")
            .element
            .box_model_snapshot()
    }

    fn first_text_descendant(arena: &NodeArena, root: NodeKey) -> NodeKey {
        let mut stack: Vec<NodeKey> = arena.children_of(root).into_iter().rev().collect();
        while let Some(key) = stack.pop() {
            if arena
                .get(key)
                .is_some_and(|node| node.element.as_any().is::<Text>())
            {
                return key;
            }
            for child in arena.children_of(key).into_iter().rev() {
                stack.push(child);
            }
        }
        panic!("expected Text descendant");
    }

    fn snapshot(arena: &NodeArena, key: NodeKey) -> crate::view::base_component::BoxModelSnapshot {
        arena.get(key).expect("node").element.box_model_snapshot()
    }

    fn plain_preedit_fixture(content: &str, cursor_char: usize) -> (NodeArena, NodeKey) {
        plain_preedit_fixture_with_options(
            content,
            cursor_char,
            "\u{4E2D}",
            Some((3, 3)),
            super::super::caret_map::CaretAffinity::Downstream,
            300.0,
        )
    }

    fn plain_preedit_fixture_with_options(
        content: &str,
        cursor_char: usize,
        preedit: &str,
        preedit_cursor: Option<(usize, usize)>,
        affinity: super::super::caret_map::CaretAffinity,
        width: f32,
    ) -> (NodeArena, NodeKey) {
        let mut text_area = TextArea::new();
        text_area.content = content.to_string();
        text_area.font_size = 14.0;
        text_area.line_height = 1.25;
        text_area.multiline = true;
        text_area.cursor_char = cursor_char;
        text_area.cursor_affinity = affinity;
        text_area.ime_preedit = preedit.to_string();
        text_area.ime_preedit_cursor = preedit_cursor;

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
                max_width: width,
                max_height: 300.0,
                viewport_width: width,
                viewport_height: 300.0,
                percent_base_width: None,
                percent_base_height: None,
            },
            LayoutPlacement {
                parent_x: 10.0,
                parent_y: 20.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: width,
                available_height: 300.0,
                viewport_width: width,
                viewport_height: 300.0,
                percent_base_width: None,
                percent_base_height: None,
            },
        );
        (arena, root)
    }

    fn wrapped_plain_fixture(content: &str, width: f32) -> (NodeArena, NodeKey) {
        let mut text_area = TextArea::new();
        text_area.content = content.to_string();
        text_area.font_size = 14.0;
        text_area.line_height = 1.25;
        text_area.multiline = true;

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
                max_width: width,
                max_height: 300.0,
                viewport_width: width,
                viewport_height: 300.0,
                percent_base_width: None,
                percent_base_height: None,
            },
            LayoutPlacement {
                parent_x: 10.0,
                parent_y: 20.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: width,
                available_height: 300.0,
                viewport_width: width,
                viewport_height: 300.0,
                percent_base_width: None,
                percent_base_height: None,
            },
        );
        (arena, root)
    }

    fn shared_soft_wrap_boundary(arena: &NodeArena, root: NodeKey) -> usize {
        arena
            .with_element_taken_ref(root, |el, arena| {
                let text_area = el
                    .as_any()
                    .downcast_ref::<TextArea>()
                    .expect("TextArea root");
                let map = super::super::caret_map::CaretNavigationMap::build(text_area, arena);
                map.lines.windows(2).find_map(|pair| {
                    pair[0].stops.iter().find_map(|upper| {
                        pair[1]
                            .stops
                            .iter()
                            .any(|lower| lower.char_index == upper.char_index)
                            .then_some(upper.char_index)
                    })
                })
            })
            .expect("root exists")
            .expect("fixture should contain a shared soft-wrap boundary")
    }

    fn run_text_pass_fragments(arena: &NodeArena, root: NodeKey) -> Vec<(String, Rect)> {
        arena
            .with_element_taken_ref(root, |el, arena| {
                let text_area = el
                    .as_any()
                    .downcast_ref::<TextArea>()
                    .expect("TextArea root");
                text_area
                    .children
                    .iter()
                    .flat_map(|key| {
                        arena
                            .with_element_taken_ref(*key, |child, _| {
                                child
                                    .as_any()
                                    .downcast_ref::<super::super::run::TextAreaTextRun>()
                                    .map(|run| run.inline_text_pass_fragment_positions())
                                    .unwrap_or_default()
                            })
                            .unwrap_or_default()
                    })
                    .collect::<Vec<_>>()
            })
            .expect("root exists")
    }

    #[test]
    fn projection_fallback_caret_start_uses_projection_left_edge() {
        let (arena, root) = projection_fixture(2, false);
        let snap = projection_snapshot(&arena, root);
        let (x, y, height) = caret_position(&arena, root);
        let line_h = 14.0 * 1.25;

        assert!((x - snap.x).abs() < 0.5, "x={x}, snap.x={}", snap.x);
        assert!((height - line_h).abs() < 0.01, "height={height}");
        assert!(
            (y - (snap.y + (snap.height - line_h) * 0.5)).abs() < 0.5,
            "y={y}, snap.y={}, snap.height={}",
            snap.y,
            snap.height
        );
    }

    #[test]
    fn projection_fallback_caret_interpolates_inside_projection() {
        let (arena, root) = projection_fixture(3, false);
        let snap = projection_snapshot(&arena, root);
        let (x, _, height) = caret_position(&arena, root);
        let expected_x = snap.x + snap.width / 3.0;

        assert!((x - expected_x).abs() < 0.5, "x={x}, expected={expected_x}");
        assert!((height - 17.5).abs() < 0.01, "height={height}");
    }

    #[test]
    fn projection_caret_uses_real_text_coordinates_when_available() {
        let (arena, root) = projection_fixture(4, true);
        let (x, y, height) = caret_position(&arena, root);
        let text_key = first_text_descendant(&arena, projection_key(&arena, root));
        let text_snap = snapshot(&arena, text_key);
        let expected = arena
            .with_element_taken_ref(text_key, |el, _| {
                el.as_any()
                    .downcast_ref::<Text>()
                    .expect("Text descendant")
                    .local_char_to_screen_position(2)
                    .expect("text caret position")
            })
            .expect("text key exists");

        assert!(
            (x - expected.0).abs() < 0.5,
            "x={x}, expected={}",
            expected.0
        );
        assert!(
            (y - expected.1).abs() < 0.5,
            "y={y}, expected={}",
            expected.1
        );
        assert!(
            (height - expected.2).abs() < 0.01,
            "height={height}, expected={}",
            expected.2
        );
        assert!(
            x >= text_snap.x - 0.5 && x <= text_snap.x + text_snap.width + 0.5,
            "caret x should be inside text bounds: x={x}, text=({}, {})",
            text_snap.x,
            text_snap.width
        );
        assert!(
            y >= text_snap.y - 0.5 && y <= text_snap.y + text_snap.height + 0.5,
            "caret y should be inside text bounds: y={y}, text=({}, {})",
            text_snap.y,
            text_snap.height
        );
    }

    #[test]
    fn hard_newline_caret_honours_affinity() {
        use crate::view::base_component::text_area::caret_map::CaretAffinity;

        fn fixture(affinity: CaretAffinity) -> (NodeArena, NodeKey) {
            let mut text_area = TextArea::new();
            text_area.content = "line1\nline2".to_string();
            text_area.font_size = 14.0;
            text_area.line_height = 1.25;
            text_area.is_focused = true;
            text_area.cursor_char = "line1\n".chars().count();
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
            (arena, root)
        }

        let (up_arena, up_root) = fixture(CaretAffinity::Upstream);
        let (_, up_y, _) = caret_position(&up_arena, up_root);
        let (down_arena, down_root) = fixture(CaretAffinity::Downstream);
        let (_, down_y, _) = caret_position(&down_arena, down_root);

        assert!(
            up_y < down_y,
            "Upstream should render before the newline on the upper line; \
             Downstream should render after it on the lower line (up={up_y}, down={down_y})",
        );
    }

    /// Integration check for "projection 內 cursor_affinity 沒作用".
    /// Sets up a TextArea whose projection wraps a long path-like Text
    /// across multiple visual lines; sets the caret to a wrap-tail char
    /// inside the projection; asserts that with `Upstream` affinity the
    /// caret y resolves to the **upper** visual line (i.e. the affinity
    /// actually flows into the projection's text descendant). The
    /// pre-fix path returned the lower line's head for any soft-wrapped
    /// Text inside a projection because `query_caret_on` dropped the
    /// affinity argument before forwarding to `Text`.
    /// Repro of the live `textarea_test` scenario. Content has a
    /// `{{USER_ID}}`-style badge projection mid-paragraph that wraps
    /// across two visual lines (the projection box itself splits, like
    /// a CSS inline-block on the boundary). The caret-affinity override
    /// must let cursor at the lower-line head of the badge render at
    /// the upper line's tail when `Upstream`, and at the lower head
    /// when `Downstream`.
    #[test]
    fn projection_badge_wrap_caret_affinity() {
        use crate::view::base_component::text_area::caret_map::CaretAffinity;
        // Mirror textarea_test: `{{API_HOST}}/v1/users/{{USER_ID}}/activity/...`
        // — single paragraph, badge projection in the middle, narrow
        // wrap forces the second badge to split.
        let user_token = "{{USER_ID_WITH_A_VERY_LONG_PROJECTION_BADGE_THAT_MUST_WRAP}}";
        let content = format!("{{{{API_HOST}}}}/v1/users/{user_token}/activity/with/path");
        let usr_start = content.find(user_token).unwrap();
        let usr_end = usr_start + user_token.len();

        let mut text_area = TextArea::new();
        text_area.content = content.to_string();
        text_area.font_size = 14.0;
        text_area.line_height = 1.25;
        text_area.cursor_char = 0;
        text_area.on_render_handler = Some(crate::ui::on_text_area_render(move |render| {
            // Two badge ranges.
            let host = "{{API_HOST}}";
            let host_start = render.content().find(host).unwrap();
            let host_end = host_start + host.len();
            for (start, end) in [(host_start, host_end), (usr_start, usr_end)] {
                let slice: String = render
                    .content()
                    .chars()
                    .skip(start)
                    .take(end - start)
                    .collect();
                render.range(start..end, move |_node| {
                    let style = ElementStylePropSchema {
                        width: Some(crate::style::Length::px(120.0)),
                        padding: Some(
                            crate::style::Padding::uniform(crate::style::Length::px(0.0))
                                .x(crate::style::Length::px(8.0)),
                        ),
                        ..Default::default()
                    };
                    RsxNode::tagged(
                        "Element",
                        RsxTagDescriptor::for_tag::<crate::view::tags::Element>(),
                    )
                    .with_prop("style", style)
                    .with_child(
                        RsxNode::tagged(
                            "Text",
                            RsxTagDescriptor::for_tag::<crate::view::tags::Text>(),
                        )
                        .with_child(RsxNode::text(slice.clone())),
                    )
                });
            }
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
                max_width: 320.0,
                max_height: 300.0,
                viewport_width: 320.0,
                viewport_height: 300.0,
                percent_base_width: None,
                percent_base_height: None,
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 320.0,
                available_height: 300.0,
                viewport_width: 320.0,
                viewport_height: 300.0,
                percent_base_width: None,
                percent_base_height: None,
            },
        );

        // Inspect the navigation map to confirm the badge truly
        // wrapped (otherwise the test fixture failed to repro).
        let (upper_tail_char, lower_head_char, upper_y, lower_y) = arena
            .with_element_taken_ref(root, |el, arena| {
                let ta = el.as_any().downcast_ref::<TextArea>().unwrap();
                let map = super::super::caret_map::CaretNavigationMap::build(ta, arena);
                eprintln!("[badge test] map has {} visual lines", map.lines.len());
                for (i, line) in map.lines.iter().enumerate() {
                    let chars: Vec<usize> = line.stops.iter().map(|s| s.char_index).collect();
                    eprintln!("  line {i} y_top={} stops={:?}", line.y_top, chars);
                }
                // Find a wrap inside USER_ID badge.
                let mut upper_tail = None;
                let mut lower_head = None;
                let mut upper_y = 0.0;
                let mut lower_y = 0.0;
                for (idx, line) in map.lines.iter().enumerate() {
                    if idx + 1 < map.lines.len() {
                        let next = &map.lines[idx + 1];
                        let last = line.stops.last().map(|s| s.char_index).unwrap_or(0);
                        let first = next.stops.first().map(|s| s.char_index).unwrap_or(0);
                        if last >= usr_start
                            && last < usr_end
                            && first > usr_start
                            && first <= usr_end
                        {
                            upper_tail = Some(last);
                            lower_head = Some(first);
                            upper_y = line.y_top;
                            lower_y = next.y_top;
                            break;
                        }
                    }
                }
                Some((upper_tail?, lower_head?, upper_y, lower_y))
            })
            .flatten()
            .expect("USER_ID badge should split across two visual lines");
        eprintln!(
            "[badge test] upper_tail={upper_tail_char} lower_head={lower_head_char} upper_y={upper_y} lower_y={lower_y}",
        );

        // Cursor at lower-head with Upstream → upper line.
        for affinity in [CaretAffinity::Upstream, CaretAffinity::Downstream] {
            arena.with_element_taken(root, |el, _| {
                let ta = el
                    .as_any_mut()
                    .downcast_mut::<TextArea>()
                    .expect("TextArea root");
                ta.cursor_char = lower_head_char;
                ta.cursor_affinity = affinity;
            });
            let (cx, cy, _) = caret_position(&arena, root);
            eprintln!("[badge test] {affinity:?}: ({cx}, {cy})");
            let expect_y = match affinity {
                CaretAffinity::Upstream => upper_y,
                CaretAffinity::Downstream => lower_y,
            };
            assert!(
                (cy - expect_y).abs() < (lower_y - upper_y) * 0.5,
                "[badge] affinity={affinity:?} cursor={lower_head_char}: caret y={cy} \
                 should match {expect_y} (upper={upper_y}, lower={lower_y})",
            );
        }
    }

    #[test]
    fn projection_caret_inside_wrapped_text_honours_affinity() {
        use crate::view::base_component::text_area::caret_map::CaretAffinity;
        let mut text_area = TextArea::new();
        text_area.content = "ab/activity/with/a/very/long/pathcd".to_string();
        text_area.font_size = 14.0;
        text_area.line_height = 1.25;
        text_area.cursor_char = 0;
        text_area.on_render_handler = Some(crate::ui::on_text_area_render(move |render| {
            render.range(2..34, |_text_area_node| {
                let style = ElementStylePropSchema {
                    width: Some(Length::px(120.0)),
                    height: Some(Length::px(80.0)),
                    ..Default::default()
                };
                RsxNode::tagged(
                    "Element",
                    RsxTagDescriptor::for_tag::<crate::view::tags::Element>(),
                )
                .with_prop("style", style)
                .with_child(
                    RsxNode::tagged(
                        "Text",
                        RsxTagDescriptor::for_tag::<crate::view::tags::Text>(),
                    )
                    .with_child(RsxNode::text("/activity/with/a/very/long/path")),
                )
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
                max_width: 300.0,
                max_height: 300.0,
                viewport_width: 300.0,
                viewport_height: 300.0,
                percent_base_width: None,
                percent_base_height: None,
            },
            LayoutPlacement {
                parent_x: 10.0,
                parent_y: 20.0,
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

        // Discover wrap structure inside the projection's Text descendant
        // by walking each char and recording when y jumps. The wrap-tail
        // char is the LAST char on the upper line (i.e. the one whose y
        // matches the upper line, with the next char's y on the lower).
        let proj_key = projection_key(&arena, root);
        let text_key = first_text_descendant(&arena, proj_key);
        let inner = "/activity/with/a/very/long/path";
        let (wrap_local_char, upper_y, lower_y) = arena
            .with_element_taken_ref(text_key, |el, _| {
                let text = el.as_any().downcast_ref::<Text>().expect("Text");
                let mut prev_y: Option<f32> = None;
                let mut upper: Option<f32> = None;
                let mut lower: Option<f32> = None;
                let mut wrap_char: Option<usize> = None;
                for c in 0..=inner.chars().count() {
                    let (_, y, _) = text.local_char_to_screen_position(c)?;
                    if let Some(py) = prev_y {
                        if y > py + 0.5 && wrap_char.is_none() {
                            wrap_char = Some(c.saturating_sub(1));
                            upper = Some(py);
                            lower = Some(y);
                            break;
                        }
                    }
                    prev_y = Some(y);
                }
                Some((wrap_char?, upper?, lower?))
            })
            .flatten()
            .expect("wrap boundary discoverable");

        // ── Case A: gap-byte caret = upper line regardless of affinity.
        // (cursor_char in TextArea-source space; projection covers 2..34.)
        // The boundary cursor (= first source char that *could* belong
        // to either visual line) is `wrap_local_char + 1` — that's the
        // first char whose probe via Text returns the upper tail
        // because of the inline-path `<= frag_chars` capture. Affinity
        // disambiguates: `Upstream` keeps the caret on the upper
        // line's tail; `Downstream` moves it to the lower line's head.
        let boundary_cursor = 2 + wrap_local_char;
        let mut boundary_up: Option<f32> = None;
        let mut boundary_down: Option<f32> = None;
        for affinity in [CaretAffinity::Upstream, CaretAffinity::Downstream] {
            arena.with_element_taken(root, |el, _| {
                let ta = el
                    .as_any_mut()
                    .downcast_mut::<TextArea>()
                    .expect("TextArea root");
                ta.cursor_char = boundary_cursor;
                ta.cursor_affinity = affinity;
            });
            let (_, y, _) = caret_position(&arena, root);
            match affinity {
                CaretAffinity::Upstream => boundary_up = Some(y),
                CaretAffinity::Downstream => boundary_down = Some(y),
            }
        }
        let bup = boundary_up.unwrap();
        let bdown = boundary_down.unwrap();
        assert!(
            bup < bdown,
            "[boundary] Upstream y ({bup}) on upper line, Downstream y ({bdown}) on lower",
        );
        assert!(
            (bup - upper_y).abs() < (lower_y - upper_y) * 0.5,
            "[boundary] Upstream y ({bup}) should match upper line ({upper_y})",
        );
        assert!(
            (bdown - lower_y).abs() < (lower_y - upper_y) * 0.5,
            "[boundary] Downstream y ({bdown}) should match lower line ({lower_y})",
        );

        // ── Case B: lower-head char (the *next* char past the gap) — this
        // is where affinity actually splits. Upstream pins the caret to
        // the upper line's tail; Downstream lands on the lower line's
        // head. This is what Cmd+Right relies on when the wrap point
        // happens to coincide with a non-leading run's first glyph.
        let lower_head_cursor = boundary_cursor + 1;
        let mut upstream_y: Option<f32> = None;
        let mut downstream_y: Option<f32> = None;
        for affinity in [CaretAffinity::Upstream, CaretAffinity::Downstream] {
            arena.with_element_taken(root, |el, _| {
                let ta = el
                    .as_any_mut()
                    .downcast_mut::<TextArea>()
                    .expect("TextArea root");
                ta.cursor_char = lower_head_cursor;
                ta.cursor_affinity = affinity;
            });
            let (_, y, _) = caret_position(&arena, root);
            match affinity {
                CaretAffinity::Upstream => upstream_y = Some(y),
                CaretAffinity::Downstream => downstream_y = Some(y),
            }
        }
        let up_y = upstream_y.unwrap();
        let down_y = downstream_y.unwrap();
        assert!(
            up_y < down_y,
            "[lower-head] Upstream y ({up_y}) should be on upper line, \
             Downstream y ({down_y}) on lower",
        );
        assert!(
            (up_y - upper_y).abs() < (lower_y - upper_y) * 0.5,
            "[lower-head] Upstream y ({up_y}) should match upper line ({upper_y})",
        );
        assert!(
            (down_y - lower_y).abs() < (lower_y - upper_y) * 0.5,
            "[lower-head] Downstream y ({down_y}) should match lower line ({lower_y})",
        );
    }

    #[test]
    fn preedit_underline_uses_middle_empty_paragraph_run() {
        let (arena, root) = plain_preedit_fixture("a\n\nb", 2);

        let rects = arena
            .with_element_taken_ref(root, |el, arena| {
                el.as_any()
                    .downcast_ref::<TextArea>()
                    .expect("TextArea root")
                    .preedit_underline_screen_rects(arena)
            })
            .expect("root exists");

        assert!(!rects.is_empty(), "expected empty-line IME underline");
        assert!(
            rects
                .iter()
                .all(|rect| rect.height == 1.0 && rect.width >= 1.0),
            "IME underline should be visible 1px strokes: {rects:?}"
        );
    }

    #[test]
    fn preedit_underline_uses_trailing_empty_paragraph_run() {
        let (arena, root) = plain_preedit_fixture("a\n", 2);

        let rects = arena
            .with_element_taken_ref(root, |el, arena| {
                el.as_any()
                    .downcast_ref::<TextArea>()
                    .expect("TextArea root")
                    .preedit_underline_screen_rects(arena)
            })
            .expect("root exists");

        assert!(
            !rects.is_empty(),
            "expected trailing empty-line IME underline"
        );
        assert!(
            rects
                .iter()
                .all(|rect| rect.height == 1.0 && rect.width >= 1.0),
            "IME underline should be visible 1px strokes: {rects:?}"
        );
    }

    #[test]
    fn soft_wrap_tail_preedit_uses_current_line_when_space_allows() {
        use super::super::caret_map::CaretAffinity;

        let content = "the quick brown fox jumps over the lazy dog";
        let width = 80.0;
        let (base_arena, base_root) = wrapped_plain_fixture(content, width);
        let boundary = shared_soft_wrap_boundary(&base_arena, base_root);
        let cursor = boundary.saturating_sub(1);
        let (upper_y, lower_y) = base_arena
            .with_element_taken_ref(base_root, |el, arena| {
                let text_area = el
                    .as_any()
                    .downcast_ref::<TextArea>()
                    .expect("TextArea root");
                let map = super::super::caret_map::CaretNavigationMap::build(text_area, arena);
                let upper = map
                    .caret_stop_for_char(boundary, CaretAffinity::Upstream)
                    .expect("upstream boundary stop");
                let lower = map
                    .caret_stop_for_char(boundary, CaretAffinity::Downstream)
                    .expect("downstream boundary stop");
                (upper.y_top, lower.y_top)
            })
            .expect("root exists");
        assert!(
            upper_y < lower_y,
            "fixture boundary must span two visual lines"
        );
        let midpoint = (upper_y + lower_y) * 0.5;

        let (up_arena, up_root) = plain_preedit_fixture_with_options(
            content,
            cursor,
            ".",
            Some((".".len(), ".".len())),
            CaretAffinity::Upstream,
            width,
        );
        let (_, up_caret_y, _) = caret_position(&up_arena, up_root);
        let up_fragments = run_text_pass_fragments(&up_arena, up_root);
        let up_rects = up_arena
            .with_element_taken_ref(up_root, |el, arena| {
                el.as_any()
                    .downcast_ref::<TextArea>()
                    .expect("TextArea root")
                    .preedit_underline_screen_rects(arena)
            })
            .expect("root exists");
        assert!(
            up_caret_y < midpoint,
            "upstream preedit caret should stay on upper visual line: caret_y={up_caret_y}, upper_y={upper_y}, lower_y={lower_y}"
        );
        assert!(
            up_fragments
                .iter()
                .any(|(content, rect)| content.contains('.') && rect.y < midpoint),
            "upstream preedit glyph should be painted on upper visual line: fragments={up_fragments:?}, upper_y={upper_y}, lower_y={lower_y}"
        );
        assert!(
            up_rects.iter().any(|rect| rect.y < lower_y),
            "upstream preedit underline should start on upper visual line: rects={up_rects:?}, upper_y={upper_y}, lower_y={lower_y}"
        );

        let (down_arena, down_root) = plain_preedit_fixture_with_options(
            content,
            cursor,
            ".",
            Some((".".len(), ".".len())),
            CaretAffinity::Downstream,
            width,
        );
        let (_, down_caret_y, _) = caret_position(&down_arena, down_root);
        let down_fragments = run_text_pass_fragments(&down_arena, down_root);
        assert!(
            down_caret_y < midpoint,
            "preedit should stay on current line when there is enough remaining space even with downstream affinity: caret_y={down_caret_y}, upper_y={upper_y}, lower_y={lower_y}"
        );
        assert!(
            down_fragments
                .iter()
                .any(|(content, rect)| content.contains('.') && rect.y < midpoint),
            "preedit glyph should be painted on current line when there is enough remaining space: fragments={down_fragments:?}, upper_y={upper_y}, lower_y={lower_y}"
        );
    }

    #[test]
    fn hard_newline_tail_preedit_uses_current_line_when_space_allows() {
        use super::super::caret_map::CaretAffinity;

        let content = "abc\ndef";
        let width = 120.0;
        let cursor = 3;
        let (arena, root) = plain_preedit_fixture_with_options(
            content,
            cursor,
            "\u{4E2D}",
            Some(("\u{4E2D}".len(), "\u{4E2D}".len())),
            CaretAffinity::Downstream,
            width,
        );
        let fragments = run_text_pass_fragments(&arena, root);
        let abc_y = fragments
            .iter()
            .find_map(|(content, rect)| content.contains("abc").then_some(rect.y))
            .expect("abc fragment");
        let preedit_y = fragments
            .iter()
            .find_map(|(content, rect)| content.contains('\u{4E2D}').then_some(rect.y))
            .expect("preedit fragment");
        assert!(
            (preedit_y - abc_y).abs() <= 1.5,
            "hard-newline tail preedit should stay before newline when space allows: fragments={fragments:?}"
        );
        let (_, caret_y, _) = caret_position(&arena, root);
        assert!(
            (caret_y - abc_y).abs() <= 1.5,
            "preedit caret should stay with glyph before newline: caret_y={caret_y}, fragments={fragments:?}"
        );
        let rects = arena
            .with_element_taken_ref(root, |el, arena| {
                el.as_any()
                    .downcast_ref::<TextArea>()
                    .expect("TextArea root")
                    .preedit_underline_screen_rects(arena)
            })
            .expect("root exists");
        assert!(
            rects.iter().any(|rect| (rect.y - abc_y).abs() <= 20.0),
            "preedit underline should stay with glyph before newline: rects={rects:?}, fragments={fragments:?}"
        );
    }

    #[test]
    fn hard_newline_tail_preedit_wraps_when_space_is_insufficient() {
        use super::super::caret_map::CaretAffinity;

        let content = "abcdefgh\nz";
        let width = 70.0;
        let cursor = 8;
        let (arena, root) = plain_preedit_fixture_with_options(
            content,
            cursor,
            "\u{4E2D}\u{4E2D}",
            Some(("\u{4E2D}\u{4E2D}".len(), "\u{4E2D}\u{4E2D}".len())),
            CaretAffinity::Downstream,
            width,
        );
        let fragments = run_text_pass_fragments(&arena, root);
        let first_y = fragments
            .iter()
            .find_map(|(content, rect)| content.contains("abcdefgh").then_some(rect.y))
            .expect("prefix fragment");
        let preedit_y = fragments
            .iter()
            .find_map(|(content, rect)| content.contains('\u{4E2D}').then_some(rect.y))
            .expect("preedit fragment");
        assert!(
            preedit_y > first_y + 1.0,
            "hard-newline tail preedit should wrap when remaining space is insufficient: fragments={fragments:?}"
        );
        let (_, caret_y, _) = caret_position(&arena, root);
        assert!(
            caret_y > first_y + 1.0,
            "preedit caret should wrap with glyph before newline: caret_y={caret_y}, fragments={fragments:?}"
        );
        let rects = arena
            .with_element_taken_ref(root, |el, arena| {
                el.as_any()
                    .downcast_ref::<TextArea>()
                    .expect("TextArea root")
                    .preedit_underline_screen_rects(arena)
            })
            .expect("root exists");
        assert!(
            rects.iter().any(|rect| rect.y > first_y + 1.0),
            "preedit underline should wrap with glyph before newline: rects={rects:?}, fragments={fragments:?}"
        );
    }

    #[test]
    fn upstream_soft_wrap_preedit_can_wrap_across_lines() {
        use super::super::caret_map::CaretAffinity;

        let content = "the quick brown fox jumps over the lazy dog";
        let width = 80.0;
        let (base_arena, base_root) = wrapped_plain_fixture(content, width);
        let boundary = shared_soft_wrap_boundary(&base_arena, base_root);
        let preedit = "\u{4E2D}".repeat(12);
        let (arena, root) = plain_preedit_fixture_with_options(
            content,
            boundary,
            &preedit,
            Some((preedit.len(), preedit.len())),
            CaretAffinity::Upstream,
            width,
        );

        let fragments = run_text_pass_fragments(&arena, root);
        let preedit_fragment_ys = fragments
            .iter()
            .filter_map(|(content, rect)| content.contains('\u{4E2D}').then_some(rect.y))
            .collect::<Vec<_>>();
        assert!(
            preedit_fragment_ys.len() >= 2,
            "long preedit glyphs should be painted as multiple visual fragments: fragments={fragments:?}"
        );
        assert!(
            preedit_fragment_ys
                .iter()
                .fold(f32::NEG_INFINITY, |max, y| max.max(*y))
                - preedit_fragment_ys
                    .iter()
                    .fold(f32::INFINITY, |min, y| min.min(*y))
                > 1.0,
            "long preedit glyph fragments should span multiple visual lines: fragments={fragments:?}"
        );

        let rects = arena
            .with_element_taken_ref(root, |el, arena| {
                el.as_any()
                    .downcast_ref::<TextArea>()
                    .expect("TextArea root")
                    .preedit_underline_screen_rects(arena)
            })
            .expect("root exists");
        assert!(
            rects.len() >= 2,
            "long preedit should keep multi-line underline fragments: {rects:?}"
        );
        let min_y = rects
            .iter()
            .map(|rect| rect.y)
            .fold(f32::INFINITY, f32::min);
        let max_y = rects
            .iter()
            .map(|rect| rect.y)
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(
            max_y - min_y > 1.0,
            "long preedit underline should span multiple visual lines: {rects:?}"
        );

        let (_, caret_y, _) = caret_position(&arena, root);
        assert!(
            caret_y >= min_y - 24.0 && caret_y <= max_y + 1.0,
            "preedit caret should land on one of the composed visual lines: caret_y={caret_y}, rects={rects:?}"
        );
    }

    #[test]
    fn projection_selection_uses_text_rects_instead_of_projection_bounds() {
        let mut text_area = TextArea::new();
        text_area.content = "ab/activity/with/a/very/long/pathcd".to_string();
        text_area.font_size = 14.0;
        text_area.line_height = 1.25;
        text_area.select_range(19, 28);
        text_area.on_render_handler = Some(crate::ui::on_text_area_render(move |render| {
            render.range(2..34, |_text_area_node| {
                let style = ElementStylePropSchema {
                    width: Some(Length::px(120.0)),
                    height: Some(Length::px(80.0)),
                    ..Default::default()
                };
                RsxNode::tagged(
                    "Element",
                    RsxTagDescriptor::for_tag::<crate::view::tags::Element>(),
                )
                .with_prop("style", style)
                .with_child(
                    RsxNode::tagged(
                        "Text",
                        RsxTagDescriptor::for_tag::<crate::view::tags::Text>(),
                    )
                    .with_child(RsxNode::text("/activity/with/a/very/long/path")),
                )
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
                max_width: 300.0,
                max_height: 300.0,
                viewport_width: 300.0,
                viewport_height: 300.0,
                percent_base_width: None,
                percent_base_height: None,
            },
            LayoutPlacement {
                parent_x: 10.0,
                parent_y: 20.0,
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

        let projection_snap = projection_snapshot(&arena, root);
        let root_el = arena
            .with_element_taken_ref(root, |el, arena| {
                el.as_any()
                    .downcast_ref::<TextArea>()
                    .expect("TextArea root")
                    .projection_selection_context_for_child(1, projection_key(&arena, root), arena)
            })
            .expect("root exists");

        let context = root_el.expect("expected projection selection render context");
        assert_eq!(context.start, 17);
        assert_eq!(context.end, 26);
        let text_key = first_text_descendant(&arena, projection_key(&arena, root));
        let rects = arena
            .with_element_taken_ref(text_key, |el, _| {
                el.as_any()
                    .downcast_ref::<Text>()
                    .expect("projection Text")
                    .local_selection_screen_rects(context.start, context.end)
            })
            .expect("text exists");

        assert!(
            !rects.is_empty(),
            "expected projection text selection rects"
        );
        assert!(
            rects
                .iter()
                .all(|rect| rect.height < projection_snap.height - 1.0),
            "selection should use visual text-line rects, not projection bounds: rects={rects:?}, projection={projection_snap:?}"
        );
        assert!(
            rects
                .iter()
                .any(|rect| rect.width < projection_snap.width - 1.0),
            "selection should be narrower than the projection union bounds: rects={rects:?}, projection={projection_snap:?}"
        );
    }

    #[test]
    fn projection_preedit_underline_uses_projection_text_rects() {
        let mut text_area = TextArea::new();
        text_area.content = "abXYZcd".to_string();
        text_area.font_size = 14.0;
        text_area.line_height = 1.25;
        text_area.cursor_char = 3;
        text_area.ime_preedit = "\u{4E2D}".to_string();
        text_area.on_render_handler = Some(crate::ui::on_text_area_render(move |render| {
            render.range(2..5, |_text_area_node| {
                RsxNode::tagged(
                    "Element",
                    RsxTagDescriptor::for_tag::<crate::view::tags::Element>(),
                )
                .with_prop(
                    "style",
                    ElementStylePropSchema {
                        width: Some(Length::px(90.0)),
                        height: Some(Length::px(42.0)),
                        ..Default::default()
                    },
                )
                .with_child(
                    RsxNode::tagged(
                        "Text",
                        RsxTagDescriptor::for_tag::<crate::view::tags::Text>(),
                    )
                    .with_child(RsxNode::text("XYZ")),
                )
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
                max_width: 300.0,
                max_height: 300.0,
                viewport_width: 300.0,
                viewport_height: 300.0,
                percent_base_width: None,
                percent_base_height: None,
            },
            LayoutPlacement {
                parent_x: 10.0,
                parent_y: 20.0,
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

        let projection_snap = projection_snapshot(&arena, root);
        let rects = arena
            .with_element_taken_ref(root, |el, arena| {
                el.as_any()
                    .downcast_ref::<TextArea>()
                    .expect("TextArea root")
                    .preedit_underline_screen_rects(arena)
            })
            .expect("root exists");

        assert!(!rects.is_empty(), "expected projection IME underline");
        assert!(
            rects.iter().all(|rect| rect.height == 1.0),
            "IME underline should be 1px high: {rects:?}"
        );
        assert!(
            rects.iter().all(|rect| {
                rect.x >= projection_snap.x - 0.5
                    && rect.x + rect.width <= projection_snap.x + projection_snap.width + 0.5
                    && rect.y >= projection_snap.y - 0.5
                    && rect.y <= projection_snap.y + projection_snap.height + 0.5
            }),
            "IME underline should be drawn inside projection bounds: rects={rects:?}, projection={projection_snap:?}"
        );
    }

    #[test]
    fn projection_preedit_caret_follows_preedit_cursor() {
        let (arena_start, root_start) = projection_fixture_with_preedit_cursor(Some((0, 0)));
        let (arena_end, root_end) = projection_fixture_with_preedit_cursor(Some((3, 3)));

        let (start_x, start_y, _) = caret_position(&arena_start, root_start);
        let (end_x, end_y, _) = caret_position(&arena_end, root_end);

        assert!(
            end_x > start_x + 0.5,
            "preedit caret should move right when IME cursor moves to the end: start_x={start_x}, end_x={end_x}"
        );
        assert!(
            (end_y - start_y).abs() < 0.5,
            "same-line preedit caret should keep y stable: start_y={start_y}, end_y={end_y}"
        );
    }

    fn projection_fixture_with_preedit_cursor(
        preedit_cursor: Option<(usize, usize)>,
    ) -> (NodeArena, NodeKey) {
        let mut text_area = TextArea::new();
        text_area.content = "abXYZcd".to_string();
        text_area.font_size = 14.0;
        text_area.line_height = 1.25;
        text_area.is_focused = true;
        text_area.cursor_char = 3;
        text_area.ime_preedit = "\u{4E2D}".to_string();
        text_area.ime_preedit_cursor = preedit_cursor;
        text_area.on_render_handler = Some(crate::ui::on_text_area_render(move |render| {
            render.range(2..5, |_text_area_node| {
                RsxNode::tagged(
                    "Element",
                    RsxTagDescriptor::for_tag::<crate::view::tags::Element>(),
                )
                .with_prop(
                    "style",
                    ElementStylePropSchema {
                        width: Some(Length::px(90.0)),
                        height: Some(Length::px(42.0)),
                        ..Default::default()
                    },
                )
                .with_child(
                    RsxNode::tagged(
                        "Text",
                        RsxTagDescriptor::for_tag::<crate::view::tags::Text>(),
                    )
                    .with_child(RsxNode::text("XYZ")),
                )
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
                max_width: 300.0,
                max_height: 300.0,
                viewport_width: 300.0,
                viewport_height: 300.0,
                percent_base_width: None,
                percent_base_height: None,
            },
            LayoutPlacement {
                parent_x: 10.0,
                parent_y: 20.0,
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
        (arena, root)
    }
}
