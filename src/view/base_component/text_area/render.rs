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
use crate::view::base_component::DirtyFlags;
use crate::view::node_arena::{NodeArena, NodeKey};
use crate::view::render_pass::draw_rect_pass::{RectPassParams, RectRenderMode};
use crate::view::render_pass::text_pass::{TextPassPreparedFragment, TextPassPreparedParams};

use super::TextArea;
use super::inline_ifc::TextAreaUnifiedIfcSourceKind;
use super::run::{TextAreaLineBreak, TextAreaTextRun};
use crate::view::base_component::Text;

const CARET_BLINK_PERIOD: Duration = Duration::from_millis(1060);
const CARET_BLINK_VISIBLE: Duration = Duration::from_millis(530);
const CARET_WIDTH: f32 = 1.0;

#[derive(Clone, Copy)]
struct ProjectionSelectionAuthority {
    witness: crate::view::paint::PaintTextSelectionWitness,
}

#[derive(Clone, Copy)]
struct ProjectionPreeditAuthority {
    projection_owner: NodeKey,
    witness: crate::view::paint::PaintTextPreeditWitness,
}

pub(super) struct PlainTextAreaPaintPayload {
    pub(super) glyph_bounds: crate::view::base_component::Rect,
    pub(super) glyph_op: Option<crate::view::paint::PreparedTextOp>,
    pub(super) selection: Option<PlainTextAreaSelectionPayload>,
    pub(super) decoration: Option<PlainTextAreaDecorationPayload>,
    pub(super) caret: Option<PlainTextAreaCaretPayload>,
}

pub(super) struct PlainTextAreaSelectionPayload {
    pub(super) bounds: crate::view::base_component::Rect,
    pub(super) ops: Vec<crate::view::paint::DrawRectOp>,
}

pub(super) struct PlainTextAreaDecorationPayload {
    pub(super) bounds: crate::view::base_component::Rect,
    pub(super) ops: Vec<crate::view::paint::DrawRectOp>,
}

pub(super) struct PlainTextAreaCaretPayload {
    pub(super) bounds: crate::view::base_component::Rect,
    pub(super) op: crate::view::paint::DrawRectOp,
}

/// A fill rect, unless any of its values is not finite.
fn fill_rect_op(params: RectPassParams) -> Option<crate::view::paint::DrawRectOp> {
    params
        .position
        .iter()
        .chain(params.size.iter())
        .chain(params.fill_color.iter())
        .all(|value| value.is_finite())
        .then_some(crate::view::paint::DrawRectOp {
            params,
            mode: RectRenderMode::FillOnly,
        })
}

impl TextArea {
    fn plain_selection_range(&self) -> Option<std::ops::Range<usize>> {
        let content_chars = self.content.chars().count();
        let anchor = self.selection_anchor_char?.min(content_chars);
        let focus = self.selection_focus_char?.min(content_chars);
        (anchor != focus).then(|| anchor.min(focus)..anchor.max(focus))
    }

    /// The part of the selection inside the projection `projection_owner`,
    /// delegated to the Text it realizes.
    fn projection_selection_authority_for_child_from_package(
        &self,
        projection_owner: NodeKey,
        arena: &NodeArena,
        package: &super::inline_ifc::TextAreaUnifiedIfcRootPackage,
    ) -> Option<ProjectionSelectionAuthority> {
        let selection = self.plain_selection_range()?;
        let segment = package.source_segments.iter().find(|segment| {
            segment.kind == TextAreaUnifiedIfcSourceKind::ProjectionAtomicBox
                && segment.child_key == projection_owner
                && selection.end > segment.char_range.start
                && selection.start < segment.char_range.end
        })?;
        let projection_children = arena.children_of(projection_owner);
        let [target_owner] = projection_children.as_slice() else {
            return None;
        };
        let target_owner = *target_owner;
        let target_stable_id = arena.get(target_owner)?.element.stable_id();
        Some(ProjectionSelectionAuthority {
            witness: crate::view::paint::PaintTextSelectionWitness {
                target_owner,
                target_stable_id,
                local_start: selection.start.saturating_sub(segment.char_range.start),
                local_end: selection
                    .end
                    .min(segment.char_range.end)
                    .saturating_sub(segment.char_range.start),
                fill: self.selection_background_color.to_rgba_f32(),
            },
        })
    }

    /// The IME preedit when the caret sits inside a projection, whose
    /// realized Text then carries the inserted characters.
    fn projection_preedit_authority_from_package(
        &self,
        arena: &NodeArena,
        package: &super::inline_ifc::TextAreaUnifiedIfcRootPackage,
    ) -> Option<ProjectionPreeditAuthority> {
        if self.ime_preedit.is_empty() {
            return None;
        }
        let cursor = self.cursor_char.min(self.content.chars().count());
        let segment = package.source_segments.iter().find(|segment| {
            segment.kind == TextAreaUnifiedIfcSourceKind::ProjectionAtomicBox
                && cursor >= segment.char_range.start
                && cursor < segment.char_range.end
        })?;
        let projection_owner = segment.child_key;
        let projection_children = arena.children_of(projection_owner);
        let [target_owner] = projection_children.as_slice() else {
            return None;
        };
        let target_owner = *target_owner;
        let target = arena.get(target_owner)?;
        let text = target.element.as_any().downcast_ref::<Text>()?;
        let target_stable_id = target.element.stable_id();
        let local_start_char = cursor.saturating_sub(segment.char_range.start);
        let local_end_char = local_start_char.saturating_add(self.ime_preedit.chars().count());
        let content = text.content();
        let target_start_byte = super::edit::byte_index_at_char(content, local_start_char);
        let target_end_byte = super::edit::byte_index_at_char(content, local_end_char);
        let target_caret_byte = target_start_byte.saturating_add(preedit_caret_byte_offset(
            self.ime_preedit.as_str(),
            self.ime_preedit_cursor,
        ));
        let target_caret_char = content
            .get(..target_caret_byte)
            .map_or(local_end_char, |prefix| prefix.chars().count());
        Some(ProjectionPreeditAuthority {
            projection_owner,
            witness: crate::view::paint::PaintTextPreeditWitness {
                projection_owner,
                target_owner,
                target_stable_id,
                local_start_char,
                local_end_char,
                target_start_byte,
                target_end_byte,
                target_caret_byte,
                target_caret_char,
            },
        })
    }

    /// The content's origin: its box scrolled by the current offset.
    fn plain_content_origin(&self) -> [f32; 2] {
        [
            self.layout_state.layout_position.x - self.scroll_x,
            self.layout_state.layout_position.y - self.scroll_y,
        ]
    }

    /// The unified package layout realized for the current content.
    fn plain_unified_package<'a>(
        &'a self,
        arena: &'a NodeArena,
    ) -> Option<std::cell::Ref<'a, super::inline_ifc::TextAreaUnifiedIfcRootPackage>> {
        if self.children.is_empty() {
            return None;
        }
        self.strictly_current_unified_inline_ifc_render_package(arena)
    }

    pub(super) fn projection_selection_witness_for_child(
        &self,
        child: NodeKey,
        arena: &NodeArena,
    ) -> Option<crate::view::paint::PaintTextSelectionWitness> {
        let package = self.plain_unified_package(arena)?;
        let authority =
            self.projection_selection_authority_for_child_from_package(child, arena, &package)?;
        Some(authority.witness)
    }

    pub(super) fn projection_preedit_witness_for_child(
        &self,
        child: NodeKey,
        arena: &NodeArena,
    ) -> Option<crate::view::paint::PaintTextPreeditWitness> {
        let package = self.plain_unified_package(arena)?;
        let authority = self.projection_preedit_authority_from_package(arena, &package)?;
        (authority.projection_owner == child).then_some(authority.witness)
    }

    fn selection_payload(
        &self,
        package: &super::inline_ifc::TextAreaUnifiedIfcRootPackage,
        origin: [f32; 2],
    ) -> Option<PlainTextAreaSelectionPayload> {
        let range = self.plain_selection_range()?;
        let fill = self.selection_background_color.to_rgba_f32();
        let ops = package
            .selection_rects_for_char_range(range)
            .into_iter()
            .filter_map(|rect| {
                fill_rect_op(RectPassParams {
                    position: [origin[0] + rect.x, origin[1] + rect.y],
                    size: [rect.width.max(1.0), rect.height.max(1.0)],
                    fill_color: fill,
                    opacity: 1.0,
                    ..Default::default()
                })
            })
            .collect::<Vec<_>>();
        let bounds = draw_rect_union(&ops)?;
        Some(PlainTextAreaSelectionPayload { bounds, ops })
    }

    fn preedit_decoration_payload(
        &self,
        package: &super::inline_ifc::TextAreaUnifiedIfcRootPackage,
        origin: [f32; 2],
    ) -> Option<PlainTextAreaDecorationPayload> {
        if self.ime_preedit.is_empty() {
            return None;
        }
        let fill = self.color.to_rgba_f32();
        #[cfg(test)]
        let test_offset = self.preedit_underline_test_offset;
        #[cfg(not(test))]
        let test_offset = [0.0; 2];
        let ops = package
            .preedit_underline_rects()
            .into_iter()
            .filter_map(|rect| {
                fill_rect_op(RectPassParams {
                    position: [
                        origin[0] + rect.x + test_offset[0],
                        origin[1] + rect.y + rect.height.max(1.0) - 1.0 + test_offset[1],
                    ],
                    size: [rect.width.max(1.0), 1.0],
                    fill_color: fill,
                    opacity: 1.0,
                    ..Default::default()
                })
            })
            .collect::<Vec<_>>();
        let bounds = draw_rect_union(&ops)?;
        Some(PlainTextAreaDecorationPayload { bounds, ops })
    }

    fn projection_preedit_decoration_payload(
        &self,
        arena: &NodeArena,
        authority: ProjectionPreeditAuthority,
        paint_offset: [f32; 2],
    ) -> Option<PlainTextAreaDecorationPayload> {
        let target = arena.get(authority.witness.target_owner)?;
        let text = target.element.as_any().downcast_ref::<Text>()?;
        let rects = text.local_selection_screen_rects(
            authority.witness.local_start_char,
            authority.witness.local_end_char,
        );
        drop(target);
        let fill = self.color.to_rgba_f32();
        #[cfg(test)]
        let test_offset = self.preedit_underline_test_offset;
        #[cfg(not(test))]
        let test_offset = [0.0; 2];
        let ops = rects
            .into_iter()
            .filter_map(|rect| {
                fill_rect_op(RectPassParams {
                    position: [
                        rect.x + paint_offset[0] + test_offset[0],
                        rect.y + rect.height.max(1.0) - 1.0 + paint_offset[1] + test_offset[1],
                    ],
                    size: [rect.width.max(1.0), 1.0],
                    fill_color: fill,
                    opacity: 1.0,
                    ..Default::default()
                })
            })
            .collect::<Vec<_>>();
        let bounds = draw_rect_union(&ops)?;
        Some(PlainTextAreaDecorationPayload { bounds, ops })
    }

    fn caret_draw_rect_payload(
        &self,
        arena: &NodeArena,
        paint_offset: [f32; 2],
    ) -> Option<PlainTextAreaCaretPayload> {
        if !self.should_draw_caret() {
            return None;
        }
        let (x, y, line_height) = self.caret_screen_position(arena)?;
        let op = fill_rect_op(RectPassParams {
            position: [x + paint_offset[0], y + paint_offset[1]],
            size: [CARET_WIDTH, line_height.max(1.0)],
            fill_color: self.color.to_rgba_f32(),
            opacity: 1.0,
            ..Default::default()
        })?;
        Some(PlainTextAreaCaretPayload {
            bounds: crate::view::base_component::Rect {
                x: op.params.position[0],
                y: op.params.position[1],
                width: op.params.size[0],
                height: op.params.size[1],
            },
            op,
        })
    }

    /// What this TextArea draws itself, with its content placed at
    /// `effective_offset`: the owner's paint offset after its own and its
    /// content anchor's snaps. Recording passes zero; the compiler applies
    /// those snaps.
    pub(super) fn prepared_plain_shadow_text_payload(
        &self,
        arena: &NodeArena,
        effective_offset: [f32; 2],
    ) -> PlainTextAreaPaintPayload {
        let content_origin = self.plain_content_origin();
        let origin = [
            content_origin[0] + effective_offset[0],
            content_origin[1] + effective_offset[1],
        ];
        let mut payload = PlainTextAreaPaintPayload {
            glyph_bounds: crate::view::base_component::Rect {
                x: origin[0],
                y: origin[1],
                width: 0.0,
                height: 0.0,
            },
            glyph_op: None,
            selection: None,
            decoration: None,
            caret: self.caret_draw_rect_payload(arena, effective_offset),
        };
        let Some(package) = self.plain_unified_package(arena) else {
            return payload;
        };
        // Root-owned selection rects cover generated text runs. Projection
        // intersections are delegated independently to each realized child
        // through `PaintTextSelectionWitness`, so a selection may cross any
        // number of projection boundaries without double painting.
        payload.selection = self.selection_payload(&package, origin);
        payload.decoration = match self.projection_preedit_authority_from_package(arena, &package) {
            Some(authority) => {
                self.projection_preedit_decoration_payload(arena, authority, effective_offset)
            }
            None => self.preedit_decoration_payload(&package, origin),
        };
        let staging_input = package.text_pass_staging_input(origin, 1.0, 0, 1.0);
        if staging_input.glyphs.is_empty() {
            return payload;
        }
        let size = package
            .content_rect()
            .filter(|rect| {
                [rect.x, rect.y, rect.width, rect.height]
                    .into_iter()
                    .all(f32::is_finite)
            })
            .map(|rect| [rect.width.max(1.0), rect.height.max(1.0)])
            .unwrap_or([
                self.layout_state.layout_size.width.max(1.0),
                self.layout_state.layout_size.height.max(1.0),
            ]);
        payload.glyph_op = crate::view::paint::PreparedTextOp::new(TextPassPreparedParams {
            staging_input,
            fragments: vec![TextPassPreparedFragment { origin, size }],
            scissor_rect: None,
            stencil_clip_id: None,
        });
        payload.glyph_bounds.width = size[0];
        payload.glyph_bounds.height = size[1];
        payload
    }

    pub(super) fn caret_animation_frame_request(
        &self,
        now: crate::time::Instant,
    ) -> crate::view::base_component::AnimationFrameRequest {
        use crate::view::base_component::AnimationFrameRequest;
        if !self.is_focused || !self.layout_state.should_render {
            return AnimationFrameRequest::None;
        }
        let Some(epoch) = self.caret_blink_epoch else {
            return AnimationFrameRequest::NextFrame;
        };
        let elapsed = now.saturating_duration_since(epoch);
        let phase = Duration::from_nanos(
            (elapsed.as_nanos() % CARET_BLINK_PERIOD.as_nanos()) as u64,
        );
        // State may have been reset after the pre-layout tick. Paint that
        // state before sleeping through another blink interval.
        if self.caret_visible != (phase < CARET_BLINK_VISIBLE) {
            return AnimationFrameRequest::NextFrame;
        }
        let remaining = if phase < CARET_BLINK_VISIBLE {
            CARET_BLINK_VISIBLE - phase
        } else {
            CARET_BLINK_PERIOD - phase
        };
        AnimationFrameRequest::At(now + remaining)
    }

    pub(super) fn tick_caret_blink(&mut self, now: crate::time::Instant) -> DirtyFlags {
        if !self.is_focused || !self.layout_state.should_render {
            self.caret_blink_epoch = None;
            if self.caret_visible {
                self.caret_visible = false;
                self.dirty_flags = self.dirty_flags.union(DirtyFlags::PAINT);
                return DirtyFlags::PAINT;
            }
            return DirtyFlags::NONE;
        }

        let Some(epoch) = self.caret_blink_epoch else {
            self.caret_blink_epoch = Some(now);
            if !self.caret_visible {
                self.caret_visible = true;
                self.dirty_flags = self.dirty_flags.union(DirtyFlags::PAINT);
                return DirtyFlags::PAINT;
            }
            return DirtyFlags::NONE;
        };
        let elapsed = now.duration_since(epoch).as_millis();
        let next_visible =
            (elapsed % CARET_BLINK_PERIOD.as_millis()) < CARET_BLINK_VISIBLE.as_millis();
        if next_visible == self.caret_visible {
            return DirtyFlags::NONE;
        }
        self.caret_visible = next_visible;
        self.dirty_flags = self.dirty_flags.union(DirtyFlags::PAINT);
        DirtyFlags::PAINT
    }

    /// Caret paint is a pure read of viewport-ticked retained state.
    pub(super) fn should_draw_caret(&self) -> bool {
        self.is_focused && self.layout_state.should_render && self.caret_visible
    }

    /// Resolve `cursor_char` to a screen-space `(x, y_top, line_height)`.
    ///
    /// Walks `children` for a `TextAreaTextRun` whose `char_range` covers
    /// the cursor (boundary cases prefer the *following* Run per the caret
    /// boundary rules). Falls back to TextArea's own layout origin when
    /// no Run exists (empty content, no placeholder).
    pub(crate) fn caret_screen_position(&self, arena: &NodeArena) -> Option<(f32, f32, f32)> {
        if self.children.is_empty() {
            // No child Run yet — caret pinned to TextArea's own origin.
            return Some((
                self.layout_state.layout_position.x,
                self.layout_state.layout_position.y,
                self.font_size.max(1.0) * self.line_height,
            ));
        }

        let cursor_host_is_projection = self.cursor_host_is_projection(arena);
        let cursor_inside_projection = self.cursor_strictly_inside_projection(arena);
        let mut has_unified_package = false;
        if let Some(package) = self.unified_inline_ifc_render_package(arena) {
            has_unified_package = true;
            let origin_x = self.layout_state.layout_position.x - self.scroll_x;
            let origin_y = self.layout_state.layout_position.y - self.scroll_y;
            if let Some(geometry) = package.preedit_caret_geometry_for_char(self.cursor_char) {
                return Some((
                    origin_x + geometry.x,
                    origin_y + geometry.y_top,
                    geometry.height,
                ));
            }
            // A cursor strictly inside a projection renders from the
            // chip's real inner glyphs (below); the navigation map only
            // has proportional rect-fraction stops there, which drift
            // off the rendered chip text.
            if !(cursor_host_is_projection && !self.ime_preedit.is_empty())
                && !cursor_inside_projection
                && let Some(geometry) =
                    package.caret_geometry_for_char(self.cursor_char, self.cursor_affinity)
            {
                return Some((
                    origin_x + geometry.x,
                    origin_y + geometry.y_top,
                    geometry.height,
                ));
            }
        }

        if !has_unified_package && !cursor_host_is_projection && !cursor_inside_projection {
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
            // Text runs resolve through the unified package: run-level
            // caret geometry was removed with the legacy shaping engine.
            if let Some(package) = self.unified_inline_ifc_render_package(arena) {
                let origin_x = self.layout_state.layout_position.x - self.scroll_x;
                let origin_y = self.layout_state.layout_position.y - self.scroll_y;
                if let Some(geometry) =
                    package.caret_geometry_for_char(self.cursor_char, self.cursor_affinity)
                {
                    return Some((
                        origin_x + geometry.x,
                        origin_y + geometry.y_top,
                        geometry.height,
                    ));
                }
            }
            return arena.with_element_taken_ref(key, |el, _| {
                let line_break = el.as_any().downcast_ref::<TextAreaLineBreak>()?;
                let local = self.cursor_char.saturating_sub(range.start).min(1);
                let line = line_break
                    .caret_stops()
                    .into_iter()
                    .find(|line| line.stops.iter().any(|stop| stop.local_char == local))?;
                let stop = line
                    .stops
                    .into_iter()
                    .find(|stop| stop.local_char == local)?;
                Some((
                    line_break.layout_state.layout_position.x + stop.local_x,
                    line_break.layout_state.layout_position.y + stop.local_y_top,
                    stop.height,
                ))
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
        let caret_h = snap.height.max(line_h).max(1.0);
        Some((x, snap.y, caret_h))
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
        if affinity == CaretAffinity::Downstream
            && let Some(next_line) = map.lines.get(line_idx + 1)
            && next_line
                .stops
                .first()
                .is_some_and(|s| s.char_index == self.cursor_char + 1)
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

    /// True when the cursor sits strictly inside a projection's char
    /// range (boundaries belong to the chip head/tail caret stops).
    fn cursor_strictly_inside_projection(&self, arena: &NodeArena) -> bool {
        self.child_char_ranges
            .iter()
            .enumerate()
            .find_map(|(idx, range)| {
                (self.cursor_char > range.start && self.cursor_char < range.end).then_some(idx)
            })
            .and_then(|idx| self.children.get(idx).copied())
            .map(|key| {
                !arena
                    .with_element_taken_ref(key, |el, _| {
                        el.as_any().is::<TextAreaTextRun>() || el.as_any().is::<TextAreaLineBreak>()
                    })
                    .unwrap_or(false)
            })
            .unwrap_or(false)
    }

    fn cursor_host_is_projection(&self, arena: &NodeArena) -> bool {
        self.child_char_ranges
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
            .unwrap_or(false)
    }

    /// Walk Run children, collect each Run's preedit underline rects, and
    /// translate to screen coords. When the caret sits inside a projection,
    /// TextArea only draws the IME underline overlay inside the projection;
    /// the projection remains responsible for text rendering.
    fn preedit_underline_screen_rects(&self, arena: &NodeArena) -> Vec<Rect> {
        if let Some(package) = self.unified_inline_ifc_render_package(arena) {
            let origin_x = self.layout_state.layout_position.x - self.scroll_x;
            let origin_y = self.layout_state.layout_position.y - self.scroll_y;
            let rects = package
                .preedit_underline_rects()
                .into_iter()
                .map(|rect| Rect {
                    x: origin_x + rect.x,
                    y: origin_y + rect.y,
                    width: rect.width,
                    height: rect.height,
                })
                .collect::<Vec<_>>();
            if !rects.is_empty() {
                return rects;
            }
        }

        if !self.ime_preedit.is_empty()
            && let Some(rects) = self.projection_preedit_underline_screen_rects(arena)
        {
            return rects;
        }

        Vec::new()
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

    pub(super) fn content_paint_anchor(&self, arena: &NodeArena) -> Option<(f32, f32)> {
        self.children.iter().find_map(|&child_key| {
            arena.with_element_taken_ref(child_key, |el, _| {
                let snap = el.box_model_snapshot();
                snap.should_render.then_some((snap.x, snap.y))
            })?
        })
    }
}

impl TextArea {
    pub(super) fn viewport_logical_scissor_rect(&self) -> [u32; 4] {
        let rect = Rect {
            x: self.layout_state.layout_position.x,
            y: self.layout_state.layout_position.y,
            width: self.viewport_size.width,
            height: self.viewport_size.height,
        };
        rect_to_logical_scissor_rect(rect)
    }

    /// The viewport clip in this TextArea's own layout frame; placing it at
    /// the layout origin reproduces `viewport_logical_scissor_rect`.
    pub(super) fn local_viewport_clip_geometry(
        &self,
    ) -> crate::view::compositor::property_tree::ClipGeometry {
        crate::view::compositor::property_tree::ClipGeometry::OwnerLocal(
            crate::view::base_component::Rect {
                x: 0.0,
                y: 0.0,
                width: self.viewport_size.width,
                height: self.viewport_size.height,
            },
        )
    }
}

fn rect_to_logical_scissor_rect(rect: Rect) -> [u32; 4] {
    crate::view::base_component::logical_scissor_for_clip_rect(crate::view::base_component::Rect {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: rect.height,
    })
    .unwrap_or_default()
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
    let byte = preedit_caret_byte_offset(preedit, cursor);
    preedit[..byte].chars().count()
}

fn preedit_caret_byte_offset(preedit: &str, cursor: Option<(usize, usize)>) -> usize {
    cursor
        .map(|(_, end)| clamp_utf8_boundary(preedit, end))
        .unwrap_or(preedit.len())
}

fn clamp_utf8_boundary(value: &str, mut byte_index: usize) -> usize {
    byte_index = byte_index.min(value.len());
    while byte_index > 0 && !value.is_char_boundary(byte_index) {
        byte_index -= 1;
    }
    byte_index
}

fn same_optional_f32_bits(left: Option<f32>, right: Option<f32>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => left.to_bits() == right.to_bits(),
        (None, None) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests;

/// The union of `ops`' rects. A single rect is its own bounds bit for bit:
/// recomputing its extent from fractional edges could round its size.
fn draw_rect_union(
    ops: &[crate::view::paint::DrawRectOp],
) -> Option<crate::view::base_component::Rect> {
    let (first, rest) = ops.split_first()?;
    let [x, y] = first.params.position;
    let [width, height] = first.params.size;
    let bounds = if rest.is_empty() {
        crate::view::base_component::Rect {
            x,
            y,
            width,
            height,
        }
    } else {
        let (mut left, mut top, mut right, mut bottom) = (x, y, x + width, y + height);
        for op in rest {
            left = left.min(op.params.position[0]);
            top = top.min(op.params.position[1]);
            right = right.max(op.params.position[0] + op.params.size[0]);
            bottom = bottom.max(op.params.position[1] + op.params.size[1]);
        }
        crate::view::base_component::Rect {
            x: left,
            y: top,
            width: right - left,
            height: bottom - top,
        }
    };
    ([bounds.x, bounds.y, bounds.width, bounds.height]
        .into_iter()
        .all(f32::is_finite)
        && bounds.width >= 0.0
        && bounds.height >= 0.0)
        .then_some(bounds)
}

/// The retained grammar oracles below describe recorded TextArea geometry on
/// the logical grid, where their fixtures place it.
const ORACLE_SCALE_FACTOR: f32 = 1.0;
