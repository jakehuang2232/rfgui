//! `Renderable` impl for `TextArea`.
//!
//! `TextAreaTextRun`'s Renderable lives in [`super::run`] — it owns the
//! glyph buffer and emits the actual `TextPass`.
//!
//! Render layer order (per design):
//!   Layer 0 — selection background  (P3.5b)
//!   Layer 1 — children (Run glyphs / projection self-render)
//!   Layer 2 — caret                  (P3.5a, this file)

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use crate::style::ColorLike;
use crate::ui::Rect;
use crate::view::base_component::{
    BuildState, DirtyFlags, DirtyPassMask, ElementTrait, Renderable, ShadowPaintBlocker,
    ShadowPaintRecordingCapability, TextAreaSelectionRenderContext, UiBuildContext,
    round_layout_value, with_text_area_selection_render_context,
};
use crate::view::frame_graph::FrameGraph;
use crate::view::node_arena::{NodeArena, NodeKey};
use crate::view::render_pass::DrawRectPass;
use crate::view::render_pass::draw_rect_pass::{
    DrawRectInput, DrawRectOutput, RectPassParams, RectRenderMode, RenderTargetIn,
};
use crate::view::render_pass::text_pass::{
    TextInput, TextOutput, TextPassPreparedFragment, TextPassPreparedParams, TextPreparedInputPass,
};

use super::TextArea;
use super::inline_ifc::TextAreaUnifiedIfcSourceKind;
use super::run::{TextAreaLineBreak, TextAreaTextRun};
use super::segment::TextAreaProjectionSegment;
use crate::view::base_component::Text;

const CARET_BLINK_PERIOD: Duration = Duration::from_millis(1060);
const CARET_BLINK_VISIBLE: Duration = Duration::from_millis(530);
const CARET_WIDTH: f32 = 1.0;

pub(super) enum PlainTextAreaPaintFailure {
    Unsupported,
    Legacy(ShadowPaintBlocker),
}

#[derive(Clone, Copy)]
struct ProjectionSelectionAuthority {
    projection_owner: NodeKey,
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

/// Private interaction-independent proof produced only from the realized
/// unified package and its arena topology. Public source grammars must add
/// their own exact interaction and paint gates before consuming it.
struct AtomicProjectionSourceCoreSeal {
    atomic_source: super::RetainedAtomicProjectionTextAreaPaintGrammar,
}

impl TextArea {
    fn plain_shadow_state_failure(
        &self,
        deferred_phase_root: bool,
    ) -> Result<(), PlainTextAreaPaintFailure> {
        if deferred_phase_root {
            return Err(PlainTextAreaPaintFailure::Legacy(
                ShadowPaintBlocker::Deferred,
            ));
        }
        if self.pointer_selecting || self.pending_caret_scroll {
            return Err(PlainTextAreaPaintFailure::Legacy(
                ShadowPaintBlocker::StatefulPaint,
            ));
        }
        self.exact_plain_selection_range()?;
        if self.ime_preedit.is_empty() {
            if self.ime_preedit_cursor.is_some() {
                return Err(PlainTextAreaPaintFailure::Legacy(
                    ShadowPaintBlocker::StatefulPaint,
                ));
            }
        } else if !self.is_focused {
            return Err(PlainTextAreaPaintFailure::Legacy(
                ShadowPaintBlocker::StatefulPaint,
            ));
        }
        self.exact_plain_baked_content_origin()?;
        if self.children_dirty {
            return Err(PlainTextAreaPaintFailure::Legacy(
                ShadowPaintBlocker::MissingPreparedInlineRoot,
            ));
        }
        Ok(())
    }

    fn exact_plain_selection_range(
        &self,
    ) -> Result<Option<std::ops::Range<usize>>, PlainTextAreaPaintFailure> {
        let (anchor, focus) = match (self.selection_anchor_char, self.selection_focus_char) {
            (None, None) => return Ok(None),
            (Some(anchor), Some(focus)) => (anchor, focus),
            _ => {
                return Err(PlainTextAreaPaintFailure::Legacy(
                    ShadowPaintBlocker::TextAreaSelection,
                ));
            }
        };
        let content_chars = self.content.chars().count();
        let anchor = anchor.min(content_chars);
        let focus = focus.min(content_chars);
        let start = anchor.min(focus);
        let end = anchor.max(focus);
        let start_byte = super::edit::byte_index_at_char(&self.content, start);
        let end_byte = super::edit::byte_index_at_char(&self.content, end);
        if start_byte > end_byte || self.content.get(start_byte..end_byte).is_none() || start == end
        {
            return if start == end {
                Ok(None)
            } else {
                Err(PlainTextAreaPaintFailure::Legacy(
                    ShadowPaintBlocker::TextAreaSelection,
                ))
            };
        }
        Ok(Some(start..end))
    }

    fn projection_selection_authority_from_package(
        &self,
        arena: &NodeArena,
        package: &super::inline_ifc::TextAreaUnifiedIfcRootPackage,
    ) -> Result<Option<ProjectionSelectionAuthority>, PlainTextAreaPaintFailure> {
        let Some(selection) = self.exact_plain_selection_range()? else {
            return Ok(None);
        };
        let mut selected_segment = package.source_segments.iter().filter(|segment| {
            segment.kind == TextAreaUnifiedIfcSourceKind::ProjectionAtomicBox
                && selection.start >= segment.char_range.start
                && selection.end <= segment.char_range.end
        });
        let Some(segment) = selected_segment.next() else {
            return Ok(None);
        };
        if selected_segment.next().is_some() {
            return Ok(None);
        }

        let projection_owner = segment.child_key;
        let projection_children = arena.children_of(projection_owner);
        let [target_owner] = projection_children.as_slice() else {
            return Ok(None);
        };
        let target_owner = *target_owner;
        if arena.parent_of(target_owner) != Some(projection_owner) {
            return Ok(None);
        }
        let Some(projection) = arena.get(projection_owner) else {
            return Ok(None);
        };
        if projection.element.children() != [target_owner] {
            return Ok(None);
        }
        drop(projection);

        let dirty_mask = DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT);
        let Some(target) = arena.get(target_owner) else {
            return Ok(None);
        };
        let Some(text) = target.element.as_any().downcast_ref::<Text>() else {
            return Ok(None);
        };
        let target_stable_id = target.element.stable_id();
        let projection_span = segment
            .char_range
            .end
            .saturating_sub(segment.char_range.start);
        if !target.element.children().is_empty()
            || !arena.children_of(target_owner).is_empty()
            || target.element.local_dirty_flags().intersects(dirty_mask)
            || arena.arena_local_dirty(target_owner).intersects(dirty_mask)
            || text.content().chars().count() != projection_span
        {
            return Ok(None);
        }
        drop(target);
        if arena
            .iter()
            .filter(|(_, node)| node.element.stable_id() == target_stable_id)
            .count()
            != 1
        {
            return Ok(None);
        }

        let local_start = selection.start - segment.char_range.start;
        let local_end = selection.end - segment.char_range.start;
        let fill = self.selection_background_color.to_rgba_f32();
        let witness = crate::view::paint::PaintTextSelectionWitness {
            target_owner,
            target_stable_id,
            local_start,
            local_end,
            fill,
        };
        if local_end > projection_span || !witness.is_canonical_for(target_owner, target_stable_id)
        {
            return Ok(None);
        }
        Ok(Some(ProjectionSelectionAuthority {
            projection_owner,
            witness,
        }))
    }

    fn projection_preedit_authority_from_package(
        &self,
        arena: &NodeArena,
        package: &super::inline_ifc::TextAreaUnifiedIfcRootPackage,
    ) -> Result<Option<ProjectionPreeditAuthority>, PlainTextAreaPaintFailure> {
        if self.ime_preedit.is_empty() {
            return Ok(None);
        }
        let cursor = self.cursor_char.min(self.content.chars().count());
        let mut candidates = package.source_segments.iter().filter(|segment| {
            segment.kind == TextAreaUnifiedIfcSourceKind::ProjectionAtomicBox
                && cursor >= segment.char_range.start
                && cursor < segment.char_range.end
        });
        let Some(segment) = candidates.next() else {
            return Ok(None);
        };
        if candidates.next().is_some() || self.exact_plain_selection_range()?.is_some() {
            return Err(PlainTextAreaPaintFailure::Legacy(
                ShadowPaintBlocker::TextAreaSelection,
            ));
        }

        let projection_owner = segment.child_key;
        let Some(projection) = arena.get(projection_owner) else {
            return Err(PlainTextAreaPaintFailure::Unsupported);
        };
        let Some(projection_segment) = projection
            .element
            .as_any()
            .downcast_ref::<TextAreaProjectionSegment>()
        else {
            return Err(PlainTextAreaPaintFailure::Unsupported);
        };
        let projection_children = arena.children_of(projection_owner);
        let [target_owner] = projection_children.as_slice() else {
            return Err(PlainTextAreaPaintFailure::Legacy(
                ShadowPaintBlocker::TextAreaSelection,
            ));
        };
        let target_owner = *target_owner;
        if projection.element.children() != [target_owner]
            || projection_segment.char_range() != segment.char_range
            || arena.parent_of(target_owner) != Some(projection_owner)
        {
            return Err(PlainTextAreaPaintFailure::Legacy(
                ShadowPaintBlocker::TextAreaSelection,
            ));
        }
        drop(projection);

        let dirty_mask = DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT);
        let Some(target) = arena.get(target_owner) else {
            return Err(PlainTextAreaPaintFailure::Unsupported);
        };
        let Some(text) = target.element.as_any().downcast_ref::<Text>() else {
            return Err(PlainTextAreaPaintFailure::Legacy(
                ShadowPaintBlocker::TextAreaSelection,
            ));
        };
        let target_stable_id = target.element.stable_id();
        let snapshot = target.element.box_model_snapshot();
        let opacity = target.element.promotion_node_info().opacity;
        let projection_span = segment
            .char_range
            .end
            .saturating_sub(segment.char_range.start);
        let preedit_chars = self.ime_preedit.chars().count();
        let local_start_char = cursor.saturating_sub(segment.char_range.start);
        let local_end_char = local_start_char.saturating_add(preedit_chars);
        let content = text.content();
        let target_start_byte = super::edit::byte_index_at_char(content, local_start_char);
        let target_end_byte = super::edit::byte_index_at_char(content, local_end_char);
        let relative_caret_byte =
            preedit_caret_byte_offset(self.ime_preedit.as_str(), self.ime_preedit_cursor);
        let target_caret_byte = target_start_byte.saturating_add(relative_caret_byte);
        let target_caret_char = content
            .get(..target_caret_byte)
            .map(str::chars)
            .map(Iterator::count)
            .unwrap_or(usize::MAX);
        if !target.element.children().is_empty()
            || !arena.children_of(target_owner).is_empty()
            || target.element.local_dirty_flags().intersects(dirty_mask)
            || arena.arena_local_dirty(target_owner).intersects(dirty_mask)
            || !snapshot.should_render
            || !opacity.is_finite()
            || opacity <= 0.0
            || preedit_chars == 0
            || local_start_char >= projection_span
            || content.chars().count() != projection_span.saturating_add(preedit_chars)
            || content.get(target_start_byte..target_end_byte) != Some(self.ime_preedit.as_str())
            || !content.is_char_boundary(target_caret_byte)
            || target_caret_byte > target_end_byte
            || target_caret_char > local_end_char
        {
            return Err(PlainTextAreaPaintFailure::Legacy(
                ShadowPaintBlocker::TextAreaSelection,
            ));
        }
        drop(target);
        if arena
            .iter()
            .filter(|(_, node)| node.element.stable_id() == target_stable_id)
            .count()
            != 1
        {
            return Err(PlainTextAreaPaintFailure::Legacy(
                ShadowPaintBlocker::TextAreaSelection,
            ));
        }

        let witness = crate::view::paint::PaintTextPreeditWitness {
            projection_owner,
            target_owner,
            target_stable_id,
            local_start_char,
            local_end_char,
            target_start_byte,
            target_end_byte,
            target_caret_byte,
            target_caret_char,
        };
        if !witness.is_canonical_for(target_owner, target_stable_id) {
            return Err(PlainTextAreaPaintFailure::Legacy(
                ShadowPaintBlocker::TextAreaSelection,
            ));
        }
        Ok(Some(ProjectionPreeditAuthority {
            projection_owner,
            witness,
        }))
    }

    fn effective_paint_offset(&self, arena: &NodeArena, parent: [f32; 2]) -> [f32; 2] {
        let mut offset = parent;
        let paint_x = self.layout_state.layout_position.x + offset[0];
        let paint_y = self.layout_state.layout_position.y + offset[1];
        offset[0] += round_layout_value(paint_x) - paint_x;
        offset[1] += round_layout_value(paint_y) - paint_y;

        if let Some((content_x, content_y)) = self.content_paint_anchor(arena) {
            let paint_x = content_x + offset[0];
            let paint_y = content_y + offset[1];
            offset[0] += round_layout_value(paint_x) - paint_x;
            offset[1] += round_layout_value(paint_y) - paint_y;
        }
        offset
    }

    fn plain_shadow_geometry_is_finite(&self) -> bool {
        [
            self.layout_state.layout_position.x,
            self.layout_state.layout_position.y,
            self.layout_state.layout_size.width,
            self.layout_state.layout_size.height,
            self.viewport_size.width,
            self.viewport_size.height,
        ]
        .into_iter()
        .all(f32::is_finite)
            && self.layout_state.layout_size.width >= 0.0
            && self.layout_state.layout_size.height >= 0.0
            && self.viewport_size.width >= 0.0
            && self.viewport_size.height >= 0.0
    }

    fn exact_plain_baked_content_origin(&self) -> Result<[f32; 2], PlainTextAreaPaintFailure> {
        if !self.scroll_x.is_finite()
            || !self.scroll_y.is_finite()
            || self.scroll_x.is_sign_negative()
            || self.scroll_y.is_sign_negative()
        {
            return Err(PlainTextAreaPaintFailure::Legacy(
                ShadowPaintBlocker::StatefulPaint,
            ));
        }
        if !self.layout_state.content_size.width.is_finite()
            || !self.layout_state.content_size.height.is_finite()
            || !self.viewport_size.width.is_finite()
            || !self.viewport_size.height.is_finite()
            || self.layout_state.content_size.width < 0.0
            || self.layout_state.content_size.height < 0.0
            || self.viewport_size.width < 0.0
            || self.viewport_size.height < 0.0
        {
            return Err(PlainTextAreaPaintFailure::Unsupported);
        }
        let max_x = (self.layout_state.content_size.width - self.viewport_size.width).max(0.0);
        let max_y = (self.layout_state.content_size.height - self.viewport_size.height).max(0.0);
        if self.scroll_x > max_x || self.scroll_y > max_y {
            return Err(PlainTextAreaPaintFailure::Legacy(
                ShadowPaintBlocker::StatefulPaint,
            ));
        }
        let origin = [
            self.layout_state.layout_position.x - self.scroll_x,
            self.layout_state.layout_position.y - self.scroll_y,
        ];
        if origin.into_iter().all(f32::is_finite) {
            Ok(origin)
        } else {
            Err(PlainTextAreaPaintFailure::Unsupported)
        }
    }

    fn exact_plain_unified_package<'a>(
        &'a self,
        owner: NodeKey,
        arena: &'a NodeArena,
        deferred_phase_root: bool,
    ) -> Result<
        Option<std::cell::Ref<'a, super::inline_ifc::TextAreaUnifiedIfcRootPackage>>,
        PlainTextAreaPaintFailure,
    > {
        self.plain_shadow_state_failure(deferred_phase_root)?;
        let content_char_count = self.content.chars().count();
        let preedit_active = !self.ime_preedit.is_empty();
        let preedit_insert_char = self.cursor_char.min(content_char_count);
        if self.self_node_key != Some(owner) || !self.plain_shadow_geometry_is_finite() {
            return Err(PlainTextAreaPaintFailure::Unsupported);
        }

        let Some(root) = arena.get(owner) else {
            return Err(PlainTextAreaPaintFailure::Unsupported);
        };
        let root_is_self = root
            .element
            .as_any()
            .downcast_ref::<TextArea>()
            .is_some_and(|candidate| std::ptr::eq(candidate, self));
        drop(root);
        if !root_is_self || arena.children_of(owner) != self.children {
            return Err(PlainTextAreaPaintFailure::Unsupported);
        }

        let dirty_mask = DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT);
        if self.dirty_flags.intersects(dirty_mask)
            || arena.arena_local_dirty(owner).intersects(dirty_mask)
        {
            return Err(PlainTextAreaPaintFailure::Legacy(
                ShadowPaintBlocker::MissingPreparedInlineRoot,
            ));
        }

        let mut ancestry = HashSet::new();
        let mut cursor = Some(owner);
        while let Some(key) = cursor {
            if !ancestry.insert(key) || arena.get(key).is_none() {
                return Err(PlainTextAreaPaintFailure::Unsupported);
            }
            cursor = arena.parent_of(key);
        }

        if self.children.is_empty() {
            if !self.content.is_empty() || !self.placeholder.is_empty() || preedit_active {
                return Err(PlainTextAreaPaintFailure::Legacy(
                    ShadowPaintBlocker::MissingPreparedInlineRoot,
                ));
            }
            return Ok(None);
        }

        // The single-run reconciliation fast path intentionally retains an
        // already-allocated Run when live content becomes empty. Treat that
        // exact zero-length topology like the no-child case: an older shaped
        // package / apply stamp must not become paint authority for content
        // that no longer paints anything.
        if self.content.is_empty() && self.placeholder.is_empty() && !preedit_active {
            if self.child_char_ranges.len() != self.children.len() {
                return Err(PlainTextAreaPaintFailure::Unsupported);
            }
            let mut seen = HashSet::with_capacity(self.children.len());
            for (&child_key, range) in self.children.iter().zip(self.child_char_ranges.iter()) {
                if !seen.insert(child_key)
                    || range != &(0..0)
                    || arena.parent_of(child_key) != Some(owner)
                    || !arena.children_of(child_key).is_empty()
                {
                    return Err(PlainTextAreaPaintFailure::Unsupported);
                }
                let Some(child) = arena.get(child_key) else {
                    return Err(PlainTextAreaPaintFailure::Unsupported);
                };
                let Some(run) = child.element.as_any().downcast_ref::<TextAreaTextRun>() else {
                    return Err(PlainTextAreaPaintFailure::Unsupported);
                };
                let snapshot = child.element.box_model_snapshot();
                if ![
                    snapshot.x,
                    snapshot.y,
                    snapshot.width,
                    snapshot.height,
                    snapshot.border_radius,
                ]
                .into_iter()
                .all(f32::is_finite)
                    || snapshot.width < 0.0
                    || snapshot.height < 0.0
                    || !child.element.children().is_empty()
                    || child.element.local_dirty_flags().intersects(dirty_mask)
                    || arena.arena_local_dirty(child_key).intersects(dirty_mask)
                    || run.char_range != (0..0)
                    || !run.effective_text().is_empty()
                    || run.is_placeholder
                    || run.is_preedit_run
                    || run.preedit_cursor.is_some()
                    || run.inline_preedit.is_some()
                {
                    return Err(PlainTextAreaPaintFailure::Legacy(
                        ShadowPaintBlocker::MissingPreparedInlineRoot,
                    ));
                }
            }
            return Ok(None);
        }

        let package = self
            .strictly_current_unified_inline_ifc_render_package(arena)
            .ok_or(PlainTextAreaPaintFailure::Legacy(
                ShadowPaintBlocker::MissingPreparedInlineRoot,
            ))?;
        if package.source_segments.len() != self.children.len()
            || self.child_char_ranges.len() != self.children.len()
        {
            return Err(PlainTextAreaPaintFailure::Unsupported);
        }
        let projection_count = package.projection_segment_count();
        if projection_count == 0 {
            if self.on_render_handler.is_some() {
                return Err(PlainTextAreaPaintFailure::Legacy(
                    ShadowPaintBlocker::StatefulPaint,
                ));
            }
        } else if self.on_render_handler.is_none() {
            return Err(PlainTextAreaPaintFailure::Legacy(
                ShadowPaintBlocker::TextAreaSelection,
            ));
        }
        let expected_width_constraint = if self.auto_wrap {
            Some(
                (if self.viewport_size.width > 0.0 {
                    self.viewport_size.width
                } else {
                    self.layout_state.layout_size.width
                })
                .max(1.0),
            )
        } else {
            None
        };
        if !same_optional_f32_bits(package.width_constraint, expected_width_constraint)
            || package.allow_wrap != self.auto_wrap
            || package.vertical_align != self.vertical_align
        {
            return Err(PlainTextAreaPaintFailure::Legacy(
                ShadowPaintBlocker::MissingPreparedInlineRoot,
            ));
        }

        let [expected_x, expected_y] = self.exact_plain_baked_content_origin()?;
        let Some((applied_x, applied_y, applied_revision)) = self.last_unified_apply.get() else {
            return Err(PlainTextAreaPaintFailure::Legacy(
                ShadowPaintBlocker::MissingPreparedInlineRoot,
            ));
        };
        if applied_x.to_bits() != expected_x.to_bits()
            || applied_y.to_bits() != expected_y.to_bits()
            || applied_revision != self.unified_ifc_source_revision.get()
        {
            return Err(PlainTextAreaPaintFailure::Legacy(
                ShadowPaintBlocker::MissingPreparedInlineRoot,
            ));
        }

        let mut child_keys = HashSet::with_capacity(self.children.len());
        let mut source_ids = HashSet::with_capacity(self.children.len());
        let mut atomic_ids = HashSet::with_capacity(projection_count);
        let mut expected_atomic_sources = Vec::with_capacity(projection_count);
        let backing = package.ifc.backing_text();
        let mut expected_backing = String::with_capacity(backing.len());
        let mut expected_committed = String::with_capacity(self.content.len());
        let mut byte_cursor = 0usize;
        let mut char_cursor = 0usize;
        let mut preedit_run_count = 0usize;
        for ((&child_key, live_range), segment) in self
            .children
            .iter()
            .zip(self.child_char_ranges.iter())
            .zip(package.source_segments.iter())
        {
            if !child_keys.insert(child_key)
                || segment.child_key != child_key
                || segment.char_range != *live_range
                || live_range.start != char_cursor
                || segment.backing_byte_range.start != byte_cursor
                || arena.parent_of(child_key) != Some(owner)
            {
                return Err(PlainTextAreaPaintFailure::Unsupported);
            }
            let Some(child) = arena.get(child_key) else {
                return Err(PlainTextAreaPaintFailure::Unsupported);
            };
            let snapshot = child.element.box_model_snapshot();
            if ![
                snapshot.x,
                snapshot.y,
                snapshot.width,
                snapshot.height,
                snapshot.border_radius,
            ]
            .into_iter()
            .all(f32::is_finite)
                || snapshot.width < 0.0
                || snapshot.height < 0.0
                || child.element.local_dirty_flags().intersects(dirty_mask)
                || arena.arena_local_dirty(child_key).intersects(dirty_mask)
            {
                return Err(PlainTextAreaPaintFailure::Legacy(
                    ShadowPaintBlocker::MissingPreparedInlineRoot,
                ));
            }
            let source = crate::view::inline_formatting_context::InlineIfcSourceId(
                child.element.stable_id(),
            );
            if segment.source != source || !source_ids.insert(source) {
                return Err(PlainTextAreaPaintFailure::Unsupported);
            }

            match segment.kind {
                TextAreaUnifiedIfcSourceKind::TextRun => {
                    let Some(run) = child.element.as_any().downcast_ref::<TextAreaTextRun>() else {
                        return Err(PlainTextAreaPaintFailure::Unsupported);
                    };
                    if !child.element.children().is_empty()
                        || !arena.children_of(child_key).is_empty()
                        || run.char_range != *live_range
                        || run.inline_preedit.is_some()
                    {
                        return Err(PlainTextAreaPaintFailure::Legacy(
                            ShadowPaintBlocker::MissingPreparedInlineRoot,
                        ));
                    }
                    if run.is_preedit_run {
                        preedit_run_count += 1;
                        let preedit_range = segment.preedit_backing_byte_range.clone();
                        let expected_caret_byte = segment.backing_byte_range.start
                            + preedit_caret_byte_offset(
                                self.ime_preedit.as_str(),
                                self.ime_preedit_cursor,
                            );
                        if !preedit_active
                            || preedit_run_count != 1
                            || live_range != &(preedit_insert_char..preedit_insert_char)
                            || run.text != self.ime_preedit
                            || run.is_placeholder
                            || run.preedit_cursor != self.ime_preedit_cursor
                            || preedit_range != Some(segment.backing_byte_range.clone())
                            || segment.preedit_caret_backing_byte != Some(expected_caret_byte)
                            || !backing.is_char_boundary(segment.backing_byte_range.start)
                            || !backing.is_char_boundary(segment.backing_byte_range.end)
                            || !backing.is_char_boundary(expected_caret_byte)
                            || expected_caret_byte < segment.backing_byte_range.start
                            || expected_caret_byte > segment.backing_byte_range.end
                        {
                            return Err(PlainTextAreaPaintFailure::Legacy(
                                ShadowPaintBlocker::MissingPreparedInlineRoot,
                            ));
                        }
                        expected_backing.push_str(self.ime_preedit.as_str());
                    } else {
                        if run.preedit_cursor.is_some()
                            || segment.preedit_backing_byte_range.is_some()
                            || segment.preedit_caret_backing_byte.is_some()
                        {
                            return Err(PlainTextAreaPaintFailure::Legacy(
                                ShadowPaintBlocker::MissingPreparedInlineRoot,
                            ));
                        }
                        let effective_text = run.effective_text();
                        let char_count = effective_text.chars().count();
                        if live_range.end != char_cursor.saturating_add(char_count) {
                            return Err(PlainTextAreaPaintFailure::Legacy(
                                ShadowPaintBlocker::MissingPreparedInlineRoot,
                            ));
                        }
                        char_cursor = live_range.end;
                        expected_backing.push_str(effective_text.as_ref());
                        expected_committed.push_str(effective_text.as_ref());
                    }
                }
                TextAreaUnifiedIfcSourceKind::LineBreak => {
                    let Some(line_break) =
                        child.element.as_any().downcast_ref::<TextAreaLineBreak>()
                    else {
                        return Err(PlainTextAreaPaintFailure::Unsupported);
                    };
                    if !child.element.children().is_empty()
                        || !arena.children_of(child_key).is_empty()
                        || line_break.char_range != *live_range
                        || segment.preedit_backing_byte_range.is_some()
                        || segment.preedit_caret_backing_byte.is_some()
                    {
                        return Err(PlainTextAreaPaintFailure::Legacy(
                            ShadowPaintBlocker::MissingPreparedInlineRoot,
                        ));
                    }
                    if live_range.end != char_cursor.saturating_add(1) {
                        return Err(PlainTextAreaPaintFailure::Legacy(
                            ShadowPaintBlocker::MissingPreparedInlineRoot,
                        ));
                    }
                    char_cursor = live_range.end;
                    expected_backing.push('\n');
                    expected_committed.push('\n');
                }
                TextAreaUnifiedIfcSourceKind::ProjectionAtomicBox => {
                    let Some(projection) = child
                        .element
                        .as_any()
                        .downcast_ref::<TextAreaProjectionSegment>()
                    else {
                        return Err(PlainTextAreaPaintFailure::Unsupported);
                    };
                    let arena_children = arena.children_of(child_key);
                    let witness = projection.exact_atomic_layout_witness();
                    let span = live_range.end.saturating_sub(live_range.start);
                    let start_byte =
                        super::edit::byte_index_at_char(&self.content, live_range.start);
                    let end_byte = super::edit::byte_index_at_char(&self.content, live_range.end);
                    let atomic_package = package.atomic_package_for_child(child_key).ok_or(
                        PlainTextAreaPaintFailure::Legacy(
                            ShadowPaintBlocker::MissingPreparedInlineRoot,
                        ),
                    )?;
                    let [placement] = atomic_package.placements.as_slice() else {
                        return Err(PlainTextAreaPaintFailure::Legacy(
                            ShadowPaintBlocker::MissingPreparedInlineRoot,
                        ));
                    };
                    let expected_constraints =
                        crate::view::inline_formatting_context::InlineIfcAtomicMeasureConstraints::new(
                            package.width_constraint,
                        );
                    let expected_baseline = (self.font_size.max(1.0) * 0.875).max(0.0);
                    if span == 0
                        || live_range.end > content_char_count
                        || child.element.children() != arena_children.as_slice()
                        || arena_children.is_empty()
                        || projection.char_range() != *live_range
                        || segment.backing_byte_range.start != segment.backing_byte_range.end
                        || segment.preedit_backing_byte_range.is_some()
                        || segment.preedit_caret_backing_byte.is_some()
                        || atomic_package.source != source
                        || placement.source != source
                        || !atomic_ids.insert(placement.id)
                        || placement.insertion_byte != byte_cursor
                        || placement.measurement.constraints != expected_constraints
                        || !same_optional_f32_bits(
                            placement.measurement.constraints.max_width,
                            expected_constraints.max_width,
                        )
                        || [
                            placement.measurement.measured_size.width,
                            placement.measurement.measured_size.height,
                        ]
                        .map(f32::to_bits)
                            != [snapshot.width, snapshot.height].map(f32::to_bits)
                        || [placement.rect.width, placement.rect.height].map(f32::to_bits)
                            != [snapshot.width, snapshot.height].map(f32::to_bits)
                        || [expected_x + placement.rect.x, expected_y + placement.rect.y]
                            .map(f32::to_bits)
                            != [snapshot.x, snapshot.y].map(f32::to_bits)
                        || [witness.flow_offset.x, witness.flow_offset.y].map(f32::to_bits)
                            != [placement.rect.x, placement.rect.y].map(f32::to_bits)
                        || witness.vertical_align != self.vertical_align
                        || witness.auto_wrap != self.auto_wrap
                        || witness.owner_inline_baseline.to_bits() != expected_baseline.to_bits()
                        || !witness.inline_full_available_width.is_finite()
                        || witness.inline_full_available_width < snapshot.width
                        || witness.has_inline_paint_fragments
                        || snapshot.border_radius.to_bits() != 0.0_f32.to_bits()
                        || [
                            placement.rect.x,
                            placement.rect.y,
                            placement.rect.width,
                            placement.rect.height,
                        ]
                        .into_iter()
                        .any(|value| !value.is_finite())
                        || placement.rect.width < 0.0
                        || placement.rect.height < 0.0
                        || self.content.get(start_byte..end_byte).is_none()
                    {
                        return Err(PlainTextAreaPaintFailure::Legacy(
                            ShadowPaintBlocker::MissingPreparedInlineRoot,
                        ));
                    }
                    char_cursor = live_range.end;
                    expected_committed.push_str(&self.content[start_byte..end_byte]);
                    expected_atomic_sources.push(source);
                }
            }
            byte_cursor = expected_backing.len();
            if segment.backing_byte_range.end != byte_cursor
                || backing.get(segment.backing_byte_range.clone())
                    != expected_backing.get(segment.backing_byte_range.clone())
            {
                return Err(PlainTextAreaPaintFailure::Legacy(
                    ShadowPaintBlocker::MissingPreparedInlineRoot,
                ));
            }
        }

        if package.atomic_sources != expected_atomic_sources {
            return Err(PlainTextAreaPaintFailure::Legacy(
                ShadowPaintBlocker::MissingPreparedInlineRoot,
            ));
        }

        let semantic_text = if self.content.is_empty() && !self.placeholder.is_empty() {
            self.placeholder.as_str()
        } else {
            self.content.as_str()
        };
        let projection_preedit = if preedit_active {
            self.projection_preedit_authority_from_package(arena, &package)?
        } else {
            None
        };
        if byte_cursor != backing.len()
            || char_cursor != semantic_text.chars().count()
            || expected_backing != backing
            || if preedit_active {
                preedit_run_count != usize::from(projection_preedit.is_none())
                    || expected_committed != self.content
            } else {
                preedit_run_count != 0 || expected_committed != semantic_text
            }
        {
            return Err(PlainTextAreaPaintFailure::Legacy(
                ShadowPaintBlocker::MissingPreparedInlineRoot,
            ));
        }
        if projection_count > 0 {
            if let Some(selection) = self.exact_plain_selection_range()? {
                let projection_owned = self
                    .projection_selection_authority_from_package(arena, &package)?
                    .is_some();
                let root_owned = package.source_segments.iter().all(|segment| {
                    segment.kind != TextAreaUnifiedIfcSourceKind::ProjectionAtomicBox
                        || selection.end <= segment.char_range.start
                        || selection.start >= segment.char_range.end
                });
                if !projection_owned && !root_owned {
                    return Err(PlainTextAreaPaintFailure::Legacy(
                        ShadowPaintBlocker::TextAreaSelection,
                    ));
                }
            }
        }
        Ok(Some(package))
    }

    pub(super) fn projection_selection_witness_for_child(
        &self,
        child: NodeKey,
        arena: &NodeArena,
    ) -> Option<crate::view::paint::PaintTextSelectionWitness> {
        let owner = self.self_node_key?;
        let package = self
            .exact_plain_unified_package(owner, arena, false)
            .ok()??;
        let authority = self
            .projection_selection_authority_from_package(arena, &package)
            .ok()??;
        (authority.projection_owner == child).then_some(authority.witness)
    }

    pub(super) fn projection_preedit_witness_for_child(
        &self,
        child: NodeKey,
        arena: &NodeArena,
    ) -> Option<crate::view::paint::PaintTextPreeditWitness> {
        let owner = self.self_node_key?;
        let package = self
            .exact_plain_unified_package(owner, arena, false)
            .ok()??;
        let authority = self
            .projection_preedit_authority_from_package(arena, &package)
            .ok()??;
        (authority.projection_owner == child).then_some(authority.witness)
    }

    fn selection_draw_rect_ops(
        &self,
        package: &super::inline_ifc::TextAreaUnifiedIfcRootPackage,
        origin: [f32; 2],
    ) -> Result<Vec<crate::view::paint::DrawRectOp>, PlainTextAreaPaintFailure> {
        let Some(range) = self.exact_plain_selection_range()? else {
            return Ok(Vec::new());
        };
        let fill = self.selection_background_color.to_rgba_f32();
        package
            .selection_rects_for_char_range(range)
            .into_iter()
            .map(|rect| {
                let params = RectPassParams {
                    position: [origin[0] + rect.x, origin[1] + rect.y],
                    size: [rect.width.max(1.0), rect.height.max(1.0)],
                    fill_color: fill,
                    opacity: 1.0,
                    ..Default::default()
                };
                if params
                    .position
                    .iter()
                    .chain(params.size.iter())
                    .chain(params.fill_color.iter())
                    .any(|value| !value.is_finite())
                {
                    return Err(PlainTextAreaPaintFailure::Unsupported);
                }
                Ok(crate::view::paint::DrawRectOp {
                    params,
                    mode: RectRenderMode::FillOnly,
                })
            })
            .collect()
    }

    fn selection_payload(
        &self,
        package: &super::inline_ifc::TextAreaUnifiedIfcRootPackage,
        origin: [f32; 2],
    ) -> Result<Option<PlainTextAreaSelectionPayload>, PlainTextAreaPaintFailure> {
        let ops = self.selection_draw_rect_ops(package, origin)?;
        let mut iter = ops.iter();
        let Some(first) = iter.next() else {
            return Ok(None);
        };
        let mut left = first.params.position[0];
        let mut top = first.params.position[1];
        let mut right = left + first.params.size[0];
        let mut bottom = top + first.params.size[1];
        for op in iter {
            left = left.min(op.params.position[0]);
            top = top.min(op.params.position[1]);
            right = right.max(op.params.position[0] + op.params.size[0]);
            bottom = bottom.max(op.params.position[1] + op.params.size[1]);
        }
        if ![left, top, right, bottom].into_iter().all(f32::is_finite)
            || right < left
            || bottom < top
        {
            return Err(PlainTextAreaPaintFailure::Unsupported);
        }
        Ok(Some(PlainTextAreaSelectionPayload {
            bounds: crate::view::base_component::Rect {
                x: left,
                y: top,
                width: right - left,
                height: bottom - top,
            },
            ops,
        }))
    }

    fn preedit_underline_rect_ops(
        &self,
        package: &super::inline_ifc::TextAreaUnifiedIfcRootPackage,
        origin: [f32; 2],
    ) -> Result<Vec<crate::view::paint::DrawRectOp>, PlainTextAreaPaintFailure> {
        if self.ime_preedit.is_empty() {
            return Ok(Vec::new());
        }
        let fill = self.color.to_rgba_f32();
        package
            .preedit_underline_rects()
            .into_iter()
            .map(|rect| {
                let params = RectPassParams {
                    position: [
                        origin[0] + rect.x,
                        origin[1] + rect.y + rect.height.max(1.0) - 1.0,
                    ],
                    size: [rect.width.max(1.0), 1.0],
                    fill_color: fill,
                    opacity: 1.0,
                    ..Default::default()
                };
                if params
                    .position
                    .iter()
                    .chain(params.size.iter())
                    .chain(params.fill_color.iter())
                    .any(|value| !value.is_finite())
                {
                    return Err(PlainTextAreaPaintFailure::Unsupported);
                }
                Ok(crate::view::paint::DrawRectOp {
                    params,
                    mode: RectRenderMode::FillOnly,
                })
            })
            .collect()
    }

    fn preedit_decoration_payload(
        &self,
        package: &super::inline_ifc::TextAreaUnifiedIfcRootPackage,
        origin: [f32; 2],
    ) -> Result<Option<PlainTextAreaDecorationPayload>, PlainTextAreaPaintFailure> {
        let ops = self.preedit_underline_rect_ops(package, origin)?;
        let mut iter = ops.iter();
        let Some(first) = iter.next() else {
            return Ok(None);
        };
        let mut left = first.params.position[0];
        let mut top = first.params.position[1];
        let mut right = left + first.params.size[0];
        let mut bottom = top + first.params.size[1];
        for op in iter {
            left = left.min(op.params.position[0]);
            top = top.min(op.params.position[1]);
            right = right.max(op.params.position[0] + op.params.size[0]);
            bottom = bottom.max(op.params.position[1] + op.params.size[1]);
        }
        if ![left, top, right, bottom].into_iter().all(f32::is_finite)
            || right < left
            || bottom < top
        {
            return Err(PlainTextAreaPaintFailure::Unsupported);
        }
        Ok(Some(PlainTextAreaDecorationPayload {
            bounds: crate::view::base_component::Rect {
                x: left,
                y: top,
                width: right - left,
                height: bottom - top,
            },
            ops,
        }))
    }

    fn projection_preedit_decoration_payload(
        &self,
        arena: &NodeArena,
        authority: ProjectionPreeditAuthority,
        paint_offset: [f32; 2],
    ) -> Result<Option<PlainTextAreaDecorationPayload>, PlainTextAreaPaintFailure> {
        let Some(target) = arena.get(authority.witness.target_owner) else {
            return Err(PlainTextAreaPaintFailure::Unsupported);
        };
        let Some(text) = target.element.as_any().downcast_ref::<Text>() else {
            return Err(PlainTextAreaPaintFailure::Unsupported);
        };
        let rects = text.local_selection_screen_rects(
            authority.witness.local_start_char,
            authority.witness.local_end_char,
        );
        drop(target);
        if rects.is_empty() {
            return Err(PlainTextAreaPaintFailure::Legacy(
                ShadowPaintBlocker::TextAreaSelection,
            ));
        }
        let fill = self.color.to_rgba_f32();
        let ops = rects
            .into_iter()
            .map(|rect| {
                let params = RectPassParams {
                    position: [
                        rect.x + paint_offset[0],
                        rect.y + rect.height.max(1.0) - 1.0 + paint_offset[1],
                    ],
                    size: [rect.width.max(1.0), 1.0],
                    fill_color: fill,
                    opacity: 1.0,
                    ..Default::default()
                };
                if params
                    .position
                    .iter()
                    .chain(params.size.iter())
                    .chain(params.fill_color.iter())
                    .any(|value| !value.is_finite())
                    || params.size.iter().any(|value| *value <= 0.0)
                {
                    return Err(PlainTextAreaPaintFailure::Unsupported);
                }
                Ok(crate::view::paint::DrawRectOp {
                    params,
                    mode: RectRenderMode::FillOnly,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut iter = ops.iter();
        let Some(first) = iter.next() else {
            return Err(PlainTextAreaPaintFailure::Unsupported);
        };
        let mut left = first.params.position[0];
        let mut top = first.params.position[1];
        let mut right = left + first.params.size[0];
        let mut bottom = top + first.params.size[1];
        for op in iter {
            left = left.min(op.params.position[0]);
            top = top.min(op.params.position[1]);
            right = right.max(op.params.position[0] + op.params.size[0]);
            bottom = bottom.max(op.params.position[1] + op.params.size[1]);
        }
        if ![left, top, right, bottom].into_iter().all(f32::is_finite)
            || right < left
            || bottom < top
            || crate::view::paint::PaintPayloadIdentity::prepared_rects(ops.iter()).is_none()
        {
            return Err(PlainTextAreaPaintFailure::Unsupported);
        }
        Ok(Some(PlainTextAreaDecorationPayload {
            bounds: crate::view::base_component::Rect {
                x: left,
                y: top,
                width: right - left,
                height: bottom - top,
            },
            ops,
        }))
    }

    fn caret_draw_rect_payload(
        &self,
        arena: &NodeArena,
        paint_offset: [f32; 2],
    ) -> Result<Option<PlainTextAreaCaretPayload>, PlainTextAreaPaintFailure> {
        if !self.should_draw_caret() {
            return Ok(None);
        }
        let (x, y, line_height) = self
            .caret_screen_position(arena)
            .ok_or(PlainTextAreaPaintFailure::Unsupported)?;
        let params = RectPassParams {
            position: [x + paint_offset[0], y + paint_offset[1]],
            size: [CARET_WIDTH, line_height.max(1.0)],
            fill_color: self.color.to_rgba_f32(),
            opacity: 1.0,
            ..Default::default()
        };
        if params
            .position
            .iter()
            .chain(params.size.iter())
            .chain(params.fill_color.iter())
            .any(|value| !value.is_finite())
        {
            return Err(PlainTextAreaPaintFailure::Unsupported);
        }
        Ok(Some(PlainTextAreaCaretPayload {
            bounds: crate::view::base_component::Rect {
                x: params.position[0],
                y: params.position[1],
                width: params.size[0],
                height: params.size[1],
            },
            op: crate::view::paint::DrawRectOp {
                params,
                mode: RectRenderMode::FillOnly,
            },
        }))
    }

    pub(super) fn prepared_plain_shadow_text_payload(
        &self,
        owner: NodeKey,
        arena: &NodeArena,
        deferred_phase_root: bool,
        paint_offset: [f32; 2],
    ) -> Result<PlainTextAreaPaintPayload, PlainTextAreaPaintFailure> {
        let package = self.exact_plain_unified_package(owner, arena, deferred_phase_root)?;
        let effective_offset = self.effective_paint_offset(arena, paint_offset);
        if effective_offset.iter().any(|value| !value.is_finite()) {
            return Err(PlainTextAreaPaintFailure::Unsupported);
        }
        let baked_origin = self.exact_plain_baked_content_origin()?;
        let origin = [
            baked_origin[0] + effective_offset[0],
            baked_origin[1] + effective_offset[1],
        ];
        let caret = self.caret_draw_rect_payload(arena, effective_offset)?;
        let Some(package) = package else {
            return Ok(PlainTextAreaPaintPayload {
                glyph_bounds: crate::view::base_component::Rect {
                    x: origin[0],
                    y: origin[1],
                    width: 0.0,
                    height: 0.0,
                },
                glyph_op: None,
                selection: None,
                decoration: None,
                caret,
            });
        };
        let projection_owns_selection = self
            .projection_selection_authority_from_package(arena, &package)?
            .is_some();
        let selection = if projection_owns_selection {
            None
        } else {
            self.selection_payload(&package, origin)?
        };
        let projection_preedit = self.projection_preedit_authority_from_package(arena, &package)?;
        let decoration = if let Some(authority) = projection_preedit {
            self.projection_preedit_decoration_payload(arena, authority, effective_offset)?
        } else {
            self.preedit_decoration_payload(&package, origin)?
        };
        let staging_input = package.text_pass_staging_input(origin, 1.0, 0, 1.0);
        if staging_input.glyphs.is_empty() {
            if package
                .ifc
                .backing_text()
                .chars()
                .any(|character| !character.is_whitespace())
            {
                return Err(PlainTextAreaPaintFailure::Legacy(
                    ShadowPaintBlocker::MissingPreparedText,
                ));
            }
            return Ok(PlainTextAreaPaintPayload {
                glyph_bounds: crate::view::base_component::Rect {
                    x: origin[0],
                    y: origin[1],
                    width: 0.0,
                    height: 0.0,
                },
                glyph_op: None,
                selection,
                decoration,
                caret,
            });
        }
        let content_rect = package.content_rect();
        if content_rect.is_some_and(|rect| {
            ![rect.x, rect.y, rect.width, rect.height]
                .into_iter()
                .all(f32::is_finite)
                || rect.width < 0.0
                || rect.height < 0.0
        }) {
            return Err(PlainTextAreaPaintFailure::Unsupported);
        }
        let size = content_rect
            .map(|rect| [rect.width.max(1.0), rect.height.max(1.0)])
            .unwrap_or([
                self.layout_state.layout_size.width.max(1.0),
                self.layout_state.layout_size.height.max(1.0),
            ]);
        let params = TextPassPreparedParams {
            staging_input,
            fragments: vec![TextPassPreparedFragment { origin, size }],
            scissor_rect: None,
            stencil_clip_id: None,
        };
        let op = crate::view::paint::PreparedTextOp::new(params).ok_or(
            PlainTextAreaPaintFailure::Legacy(ShadowPaintBlocker::MissingPreparedText),
        )?;
        drop(package);
        Ok(PlainTextAreaPaintPayload {
            glyph_bounds: crate::view::base_component::Rect {
                x: origin[0],
                y: origin[1],
                width: size[0],
                height: size[1],
            },
            glyph_op: Some(op),
            selection,
            decoration,
            caret,
        })
    }

    pub(super) fn plain_leaf_shadow_capability(
        &self,
        owner: NodeKey,
        child_key: NodeKey,
        child_stable_id: u64,
        expected_kind: TextAreaUnifiedIfcSourceKind,
        arena: &NodeArena,
        deferred_phase_root: bool,
    ) -> ShadowPaintRecordingCapability {
        if let Err(failure) = self.plain_shadow_state_failure(deferred_phase_root) {
            return match failure {
                PlainTextAreaPaintFailure::Unsupported => {
                    ShadowPaintRecordingCapability::Unsupported
                }
                PlainTextAreaPaintFailure::Legacy(blocker) => {
                    ShadowPaintRecordingCapability::Legacy(blocker)
                }
            };
        }
        if self.self_node_key != Some(owner)
            || arena.parent_of(child_key) != Some(owner)
            || !self.children.contains(&child_key)
            || !arena.children_of(child_key).is_empty()
        {
            return ShadowPaintRecordingCapability::Unsupported;
        }
        let dirty_mask = DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT);
        let Some(child) = arena.get(child_key) else {
            return ShadowPaintRecordingCapability::Unsupported;
        };
        if !child.element.children().is_empty()
            || child.element.local_dirty_flags().intersects(dirty_mask)
            || arena.arena_local_dirty(child_key).intersects(dirty_mask)
        {
            return ShadowPaintRecordingCapability::Legacy(
                ShadowPaintBlocker::MissingPreparedInlineRoot,
            );
        }
        drop(child);
        let package = match self.exact_plain_unified_package(owner, arena, deferred_phase_root) {
            Ok(Some(package)) => package,
            Ok(None) => return ShadowPaintRecordingCapability::Transparent,
            Err(PlainTextAreaPaintFailure::Unsupported) => {
                return ShadowPaintRecordingCapability::Unsupported;
            }
            Err(PlainTextAreaPaintFailure::Legacy(blocker)) => {
                return ShadowPaintRecordingCapability::Legacy(blocker);
            }
        };
        let mut matches = package.source_segments.iter().filter(|segment| {
            segment.child_key == child_key
                && segment.kind == expected_kind
                && segment.source
                    == crate::view::inline_formatting_context::InlineIfcSourceId(child_stable_id)
        });
        if matches.next().is_none() || matches.next().is_some() {
            return ShadowPaintRecordingCapability::Unsupported;
        }
        ShadowPaintRecordingCapability::Transparent
    }

    pub(super) fn projection_segment_shadow_capability(
        &self,
        owner: NodeKey,
        child_key: NodeKey,
        child_stable_id: u64,
        arena: &NodeArena,
        deferred_phase_root: bool,
    ) -> ShadowPaintRecordingCapability {
        if self.self_node_key != Some(owner)
            || arena.parent_of(child_key) != Some(owner)
            || !self.children.contains(&child_key)
        {
            return ShadowPaintRecordingCapability::Unsupported;
        }
        let package = match self.exact_plain_unified_package(owner, arena, deferred_phase_root) {
            Ok(Some(package)) => package,
            Ok(None) => return ShadowPaintRecordingCapability::Unsupported,
            Err(PlainTextAreaPaintFailure::Unsupported) => {
                return ShadowPaintRecordingCapability::Unsupported;
            }
            Err(PlainTextAreaPaintFailure::Legacy(blocker)) => {
                return ShadowPaintRecordingCapability::Legacy(blocker);
            }
        };
        let source = crate::view::inline_formatting_context::InlineIfcSourceId(child_stable_id);
        let mut matches = package.source_segments.iter().filter(|segment| {
            segment.child_key == child_key
                && segment.kind == TextAreaUnifiedIfcSourceKind::ProjectionAtomicBox
                && segment.source == source
        });
        if matches.next().is_none() || matches.next().is_some() {
            return ShadowPaintRecordingCapability::Unsupported;
        }
        ShadowPaintRecordingCapability::Transparent
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
        ctx.set_paint_offset(self.effective_paint_offset(arena, parent_paint_offset));

        let previous_scissor = ctx.push_scissor_rect(self.viewport_scissor_rect());
        let unified_render_package = self.unified_inline_ifc_render_package(arena);
        let text_origin = ctx.paint_point(
            self.layout_state.layout_position.x - self.scroll_x,
            self.layout_state.layout_position.y - self.scroll_y,
        );
        let mut preedit_underline_ops = unified_render_package
            .as_ref()
            .and_then(|package| self.preedit_underline_rect_ops(package, text_origin).ok())
            .unwrap_or_default();

        // Layer 0 — selection background. Drawn under children so glyphs
        // overlay the highlight.
        if let (Some(package), Some(target)) = (&unified_render_package, ctx.current_target()) {
            for op in self
                .selection_draw_rect_ops(package, text_origin)
                .unwrap_or_default()
            {
                let mut sel_pass = DrawRectPass::new(
                    op.params,
                    DrawRectInput {
                        pass_context: ctx.graphics_pass_context(),
                        ..Default::default()
                    },
                    DrawRectOutput {
                        render_target: target,
                        ..Default::default()
                    },
                );
                sel_pass.set_render_mode(op.mode);
                sel_pass.set_scissor_rect(ctx.graphics_pass_context().scissor_rect);
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

        if let (Some(package), Some(target)) = (&unified_render_package, ctx.current_target()) {
            let [origin_x, origin_y] = text_origin;
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
        drop(unified_render_package);

        // Layer 1.5 — IME preedit underline (above glyphs, below caret).
        if preedit_underline_ops.is_empty()
            && !self.ime_preedit.is_empty()
            && let Some(rects) = self.projection_preedit_underline_screen_rects(arena)
        {
            let fill = self.color.to_rgba_f32();
            preedit_underline_ops = rects
                .into_iter()
                .filter_map(|rect| {
                    let position = ctx.paint_point(rect.x, rect.y);
                    position
                        .iter()
                        .chain([rect.width, rect.height].iter())
                        .chain(fill.iter())
                        .all(|value| value.is_finite())
                        .then_some(crate::view::paint::DrawRectOp {
                            params: RectPassParams {
                                position,
                                size: [rect.width.max(1.0), rect.height.max(1.0)],
                                fill_color: fill,
                                opacity: 1.0,
                                ..Default::default()
                            },
                            mode: RectRenderMode::FillOnly,
                        })
                })
                .collect();
        }
        if ctx.current_target().is_some() {
            for op in preedit_underline_ops {
                let mut underline_pass = DrawRectPass::new(
                    op.params,
                    DrawRectInput {
                        pass_context: ctx.graphics_pass_context(),
                        ..Default::default()
                    },
                    DrawRectOutput::default(),
                );
                underline_pass.set_render_mode(op.mode);
                ctx.emit_draw_rect_pass(graph, underline_pass);
            }
        }

        // Layer 2 — caret.
        if let Some(caret) = self
            .caret_draw_rect_payload(arena, ctx.paint_offset())
            .ok()
            .flatten()
        {
            let mut caret_pass = DrawRectPass::new(
                caret.op.params,
                DrawRectInput {
                    pass_context: ctx.graphics_pass_context(),
                    ..Default::default()
                },
                DrawRectOutput::default(),
            );
            caret_pass.set_render_mode(caret.op.mode);
            ctx.emit_draw_rect_pass(graph, caret_pass);
        }

        self.dirty_flags = self.dirty_flags.without(DirtyFlags::PAINT);
        ctx.restore_scissor_rect(previous_scissor);
        ctx.set_paint_offset(parent_paint_offset);
        ctx.into_state()
    }
}

impl TextArea {
    /// Closed C3a source oracle for one already-realized atomic projection.
    ///
    /// The user `on_render` handler is intentionally never called here.  The
    /// authority is the current arena topology plus the strictly-current
    /// unified IFC package produced by layout.  Recorder/compiler admission
    /// remains a later, independent migration segment.
    pub(crate) fn exact_retained_property_scroll_atomic_projection_subtree(
        &self,
        owner: NodeKey,
        arena: &NodeArena,
        parent_paint_offset: [f32; 2],
    ) -> Option<super::RetainedAtomicProjectionTextAreaPaintGrammar> {
        if self.selection_anchor_char.is_some() || self.selection_focus_char.is_some() {
            return None;
        }
        self.exact_retained_property_scroll_atomic_projection_source(
            owner,
            arena,
            parent_paint_offset,
            super::RetainedTextAreaPaintGrammar::GlyphOnly,
        )
    }

    /// Root-owned nonempty selection followed by the root glyph and one
    /// realized atomic projection. Selection wholly owned by the projection
    /// remains outside this grammar because its live chunk order differs.
    pub(crate) fn exact_retained_property_scroll_atomic_projection_selection_subtree(
        &self,
        owner: NodeKey,
        arena: &NodeArena,
        parent_paint_offset: [f32; 2],
    ) -> Option<super::RetainedAtomicProjectionSelectionTextAreaPaintGrammar> {
        let (Some(anchor), Some(focus)) = (self.selection_anchor_char, self.selection_focus_char)
        else {
            return None;
        };
        let content_chars = self.content.chars().count();
        if anchor > content_chars || focus > content_chars || anchor == focus {
            return None;
        }
        let selection = super::RetainedTextAreaPaintGrammar::SelectionGlyphs {
            start_char: anchor.min(focus),
            end_char: anchor.max(focus),
            color_rgba_bits: self
                .selection_background_color
                .to_rgba_f32()
                .map(f32::to_bits),
        };
        let atomic_source = self.exact_retained_property_scroll_atomic_projection_source(
            owner,
            arena,
            parent_paint_offset,
            selection,
        )?;
        let package = self
            .exact_plain_unified_package(owner, arena, false)
            .ok()??;
        if self
            .projection_selection_authority_from_package(arena, &package)
            .ok()?
            .is_some()
        {
            return None;
        }
        super::RetainedAtomicProjectionSelectionTextAreaPaintGrammar::from_frozen_source_identity(
            super::RetainedAtomicProjectionSelectionTextAreaFrozenSourceIdentity {
                atomic_source,
                selection,
            },
        )
    }

    fn exact_retained_property_scroll_atomic_projection_source(
        &self,
        owner: NodeKey,
        arena: &NodeArena,
        parent_paint_offset: [f32; 2],
        paint_grammar: super::RetainedTextAreaPaintGrammar,
    ) -> Option<super::RetainedAtomicProjectionTextAreaPaintGrammar> {
        if self.self_node_key != Some(owner)
            || self.on_render_handler.is_none()
            || self.is_focused
            || self.caret_visible
            || self.pointer_selecting
            || self.pending_caret_scroll
            || !self.ime_preedit.is_empty()
            || self.ime_preedit_cursor.is_some()
            || self.scroll_x.to_bits() != 0.0_f32.to_bits()
            || self.scroll_y.to_bits() != 0.0_f32.to_bits()
            || !self.layout_state.should_render
            || self.has_active_animator()
            || self.is_deferred_to_root_viewport_render()
            || self.children.is_empty()
            || self.children.as_slice() != arena.children_of(owner).as_slice()
            || parent_paint_offset.iter().any(|value| !value.is_finite())
        {
            return None;
        }
        let paint_x = self.layout_state.layout_position.x + parent_paint_offset[0];
        let paint_y = self.layout_state.layout_position.y + parent_paint_offset[1];
        if (round_layout_value(paint_x) - paint_x).to_bits() != 0.0_f32.to_bits()
            || (round_layout_value(paint_y) - paint_y).to_bits() != 0.0_f32.to_bits()
        {
            return None;
        }

        let payload = self
            .prepared_plain_shadow_text_payload(owner, arena, false, parent_paint_offset)
            .ok()?;
        let Some(root_glyph) = payload.glyph_op.as_ref() else {
            return None;
        };
        let paint_is_exact = match (paint_grammar, payload.selection.as_ref()) {
            (super::RetainedTextAreaPaintGrammar::GlyphOnly, None) => true,
            (
                selection_grammar @ super::RetainedTextAreaPaintGrammar::SelectionGlyphs { .. },
                Some(selection),
            ) => {
                !selection.ops.is_empty()
                    && crate::view::paint::PaintPayloadIdentity::prepared_text_area_selection(
                        selection_grammar,
                        selection.ops.iter(),
                    )
                    .is_some()
            }
            _ => false,
        };
        if !root_glyph.has_canonical_identity()
            || payload.decoration.is_some()
            || payload.caret.is_some()
            || !paint_is_exact
        {
            return None;
        }

        self.exact_retained_property_scroll_atomic_projection_core(owner, arena)
            .map(|core| core.atomic_source)
    }

    /// Closed focused glyph source for one realized atomic projection.  This
    /// is a sibling of both the non-focused atomic grammars and the generated-
    /// run interactive grammar; none of their predicates are widened.
    pub(crate) fn exact_retained_property_scroll_focused_atomic_projection_glyph_subtree(
        &self,
        owner: NodeKey,
        arena: &NodeArena,
        parent_paint_offset: [f32; 2],
    ) -> Option<super::RetainedFocusedAtomicProjectionTextAreaPaintGrammar> {
        if self.self_node_key != Some(owner)
            || self.on_render_handler.is_none()
            || !self.is_focused
            || !self.layout_state.should_render
            || self.selection_anchor_char.is_some()
            || self.selection_focus_char.is_some()
            || self.pointer_selecting
            || self.pending_caret_scroll
            || self.scroll_x.to_bits() != 0.0_f32.to_bits()
            || self.scroll_y.to_bits() != 0.0_f32.to_bits()
            || self.has_active_animator()
            || self.is_deferred_to_root_viewport_render()
            || self.children.is_empty()
            || self.children.as_slice() != arena.children_of(owner).as_slice()
            || parent_paint_offset.iter().any(|value| !value.is_finite())
            || self.cursor_char > self.content.chars().count()
        {
            return None;
        }
        let preedit_active = !self.ime_preedit.is_empty();
        if !preedit_active && self.ime_preedit_cursor.is_some() {
            return None;
        }
        if let Some((start, end)) = self.ime_preedit_cursor {
            if start > end
                || end > self.ime_preedit.len()
                || !self.ime_preedit.is_char_boundary(start)
                || !self.ime_preedit.is_char_boundary(end)
            {
                return None;
            }
        }
        let paint_x = self.layout_state.layout_position.x + parent_paint_offset[0];
        let paint_y = self.layout_state.layout_position.y + parent_paint_offset[1];
        if (round_layout_value(paint_x) - paint_x).to_bits() != 0.0_f32.to_bits()
            || (round_layout_value(paint_y) - paint_y).to_bits() != 0.0_f32.to_bits()
        {
            return None;
        }

        let payload = self
            .prepared_plain_shadow_text_payload(owner, arena, false, parent_paint_offset)
            .ok()?;
        let root_glyph = payload.glyph_op.as_ref()?;
        if !root_glyph.has_canonical_identity()
            || payload.selection.is_some()
            || (payload.decoration.is_some() != preedit_active)
            || payload.caret.is_some() != self.caret_visible
        {
            return None;
        }
        let preedit = if preedit_active {
            let decoration = payload.decoration.as_ref()?;
            let underline_identity =
                crate::view::paint::PaintPayloadIdentity::prepared_rects(decoration.ops.iter())?;
            let glyph_identity =
                crate::view::paint::PaintPayloadIdentity::prepared_texts([root_glyph]);
            let package = self
                .exact_plain_unified_package(owner, arena, false)
                .ok()??;
            let seal = super::FocusedAtomicPreeditSourceSeal {
                owner,
                stable_id: self.stable_id(),
                content: std::sync::Arc::from(self.content.as_str()),
                backing_text: std::sync::Arc::from(package.ifc.backing_text()),
                ime_preedit: std::sync::Arc::from(self.ime_preedit.as_str()),
                ime_preedit_cursor: self.ime_preedit_cursor,
                cursor_char: self.cursor_char,
                cursor_affinity: self.cursor_affinity,
                foreground_color_bits: self.color.to_rgba_f32().map(f32::to_bits),
                glyph_bounds_bits: [
                    payload.glyph_bounds.x,
                    payload.glyph_bounds.y,
                    payload.glyph_bounds.width,
                    payload.glyph_bounds.height,
                ]
                .map(f32::to_bits),
                underline_bounds_bits: [
                    decoration.bounds.x,
                    decoration.bounds.y,
                    decoration.bounds.width,
                    decoration.bounds.height,
                ]
                .map(f32::to_bits),
                glyph_identity,
                underline_identity,
                unified_ifc_source_revision: self.unified_ifc_source_revision.get(),
                last_unified_apply_bits: self
                    .last_unified_apply
                    .get()
                    .map(|(x, y, revision)| (x.to_bits(), y.to_bits(), revision)),
            };
            Some(seal.is_canonical().then_some(seal)?)
        } else {
            None
        };
        let paint = match payload.caret.as_ref() {
            None => super::FocusedAtomicCaretSourcePaintSeal::Hidden,
            Some(caret) => {
                if caret.op.mode != RectRenderMode::FillOnly
                    || caret.op.params.size[0].to_bits() != CARET_WIDTH.to_bits()
                    || !caret.op.params.size[1].is_finite()
                    || caret.op.params.size[1] <= 0.0
                    || caret.op.params.opacity.to_bits() != 1.0_f32.to_bits()
                    || caret.op.params.fill_color.map(f32::to_bits)
                        != self.color.to_rgba_f32().map(f32::to_bits)
                {
                    return None;
                }
                super::FocusedAtomicCaretSourcePaintSeal::Present {
                    bounds_bits: [
                        caret.bounds.x,
                        caret.bounds.y,
                        caret.bounds.width,
                        caret.bounds.height,
                    ]
                    .map(f32::to_bits),
                    payload_identity: crate::view::paint::PaintPayloadIdentity::prepared_rects([
                        &caret.op,
                    ])?,
                }
            }
        };
        let caret = super::FocusedAtomicCaretSourceSeal::from_frozen_source_identity(
            super::FocusedAtomicCaretFrozenSourceIdentity {
                owner,
                stable_id: self.stable_id(),
                focused: self.is_focused,
                should_render: self.layout_state.should_render,
                caret_visible: self.caret_visible,
                foreground_color_bits: self.color.to_rgba_f32().map(f32::to_bits),
                cursor_char: self.cursor_char,
                cursor_affinity: self.cursor_affinity,
                ime_preedit_cursor: self.ime_preedit_cursor,
                local_scroll_bits: [self.scroll_x.to_bits(), self.scroll_y.to_bits()],
                unified_ifc_source_revision: self.unified_ifc_source_revision.get(),
                last_unified_apply_bits: self
                    .last_unified_apply
                    .get()
                    .map(|(x, y, revision)| (x.to_bits(), y.to_bits(), revision)),
                paint,
            },
        )?;
        let core = self.exact_retained_property_scroll_atomic_projection_core(owner, arena)?;
        super::RetainedFocusedAtomicProjectionTextAreaPaintGrammar::from_frozen_source_identity(
            super::RetainedFocusedAtomicProjectionTextAreaFrozenSourceIdentity {
                atomic_source: core.atomic_source,
                caret,
                preedit,
            },
        )
    }

    /// Interaction-independent atomic topology/layout builder.  It consumes
    /// only the already-realized unified package and arena-owned descendants;
    /// no user callback is executed and no focus/caret/selection state is
    /// consulted here.
    fn exact_retained_property_scroll_atomic_projection_core(
        &self,
        owner: NodeKey,
        arena: &NodeArena,
    ) -> Option<AtomicProjectionSourceCoreSeal> {
        let package = self
            .exact_plain_unified_package(owner, arena, false)
            .ok()??;
        if package.projection_segment_count() != 1 || package.atomic_sources.len() != 1 {
            return None;
        }
        let projection_index = package.source_segments.iter().position(|segment| {
            segment.kind == TextAreaUnifiedIfcSourceKind::ProjectionAtomicBox
        })?;
        let projection_source = package.source_segments.get(projection_index)?;
        let projection_owner = projection_source.child_key;
        let projection_node = arena.get(projection_owner)?;
        let projection = projection_node
            .element
            .as_any()
            .downcast_ref::<TextAreaProjectionSegment>()?;
        let projection_children = arena.children_of(projection_owner);
        let [projection_text_owner] = projection_children.as_slice() else {
            return None;
        };
        let projection_text_owner = *projection_text_owner;
        let projection_text_node = arena.get(projection_text_owner)?;
        let projection_text = projection_text_node
            .element
            .as_any()
            .downcast_ref::<Text>()?;
        let dirty_mask = DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT);
        if projection_node.element.children() != [projection_text_owner]
            || arena.parent_of(projection_owner) != Some(owner)
            || arena.parent_of(projection_text_owner) != Some(projection_owner)
            || !arena.children_of(projection_text_owner).is_empty()
            || !projection_text_node.element.children().is_empty()
            || projection_text_node
                .element
                .local_dirty_flags()
                .intersects(dirty_mask)
            || arena
                .arena_local_dirty(projection_text_owner)
                .intersects(dirty_mask)
            || projection_text_node
                .element
                .is_deferred_to_root_viewport_render()
            || projection_text_node.element.has_active_animator()
            || !projection_text.is_exact_standalone_retained_text_leaf()
        {
            return None;
        }

        let atomic_package = package.atomic_package_for_child(projection_owner)?;
        let [placement] = atomic_package.placements.as_slice() else {
            return None;
        };
        let witness = projection.exact_atomic_layout_witness();
        let projection_snapshot = projection_node.element.box_model_snapshot();
        let projection_text_snapshot = projection_text_node.element.box_model_snapshot();
        if ![
            projection_text_snapshot.x,
            projection_text_snapshot.y,
            projection_text_snapshot.width,
            projection_text_snapshot.height,
            projection_text_snapshot.border_radius,
        ]
        .into_iter()
        .all(f32::is_finite)
            || projection_text_snapshot.width <= 0.0
            || projection_text_snapshot.height <= 0.0
            || placement.source != projection_source.source
            || atomic_package.source != projection_source.source
            || package.atomic_sources.as_slice() != [projection_source.source]
            || placement.insertion_byte != projection_source.backing_byte_range.start
            || projection_source.backing_byte_range.start
                != projection_source.backing_byte_range.end
            || projection.char_range() != projection_source.char_range
            || [
                placement.measurement.measured_size.width,
                placement.measurement.measured_size.height,
            ]
            .map(f32::to_bits)
                != [projection_snapshot.width, projection_snapshot.height].map(f32::to_bits)
            || [placement.rect.width, placement.rect.height].map(f32::to_bits)
                != [projection_snapshot.width, projection_snapshot.height].map(f32::to_bits)
            || [
                projection_text_snapshot.x,
                projection_text_snapshot.y,
                projection_text_snapshot.width,
                projection_text_snapshot.height,
            ]
            .map(f32::to_bits)
                != [
                    projection_snapshot.x,
                    projection_snapshot.y,
                    projection_snapshot.width,
                    projection_snapshot.height,
                ]
                .map(f32::to_bits)
            || [witness.flow_offset.x, witness.flow_offset.y].map(f32::to_bits)
                != [placement.rect.x, placement.rect.y].map(f32::to_bits)
            || witness.has_inline_paint_fragments
            || witness.vertical_align != self.vertical_align
            || witness.auto_wrap != self.auto_wrap
        {
            return None;
        }

        let topology = package
            .source_segments
            .iter()
            .enumerate()
            .map(|(topology_index, source)| {
                let node = arena.get(source.child_key)?;
                let kind = match source.kind {
                    TextAreaUnifiedIfcSourceKind::TextRun => {
                        node.element.as_any().is::<TextAreaTextRun>().then_some(
                            super::RetainedAtomicProjectionTextAreaTopologyKind::TextRun,
                        )?
                    }
                    TextAreaUnifiedIfcSourceKind::LineBreak => {
                        node.element.as_any().is::<TextAreaLineBreak>().then_some(
                            super::RetainedAtomicProjectionTextAreaTopologyKind::LineBreak,
                        )?
                    }
                    TextAreaUnifiedIfcSourceKind::ProjectionAtomicBox => node
                        .element
                        .as_any()
                        .is::<TextAreaProjectionSegment>()
                        .then_some(
                            super::RetainedAtomicProjectionTextAreaTopologyKind::ProjectionSegment,
                        )?,
                };
                Some(super::RetainedAtomicProjectionTextAreaTopologySeal {
                    topology_index,
                    owner: source.child_key,
                    stable_id: node.element.stable_id(),
                    source_id: source.source.0,
                    kind,
                    start_char: source.char_range.start,
                    end_char: source.char_range.end,
                    backing_start_byte: source.backing_byte_range.start,
                    backing_end_byte: source.backing_byte_range.end,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        let last_unified_apply_bits = self
            .last_unified_apply
            .get()
            .map(|(x, y, revision)| (x.to_bits(), y.to_bits(), revision))?;
        let frozen = super::RetainedAtomicProjectionTextAreaFrozenSourceIdentity {
            projection_index,
            projection_owner,
            projection_stable_id: projection_node.element.stable_id(),
            projection_text_owner,
            projection_text_stable_id: projection_text_node.element.stable_id(),
            projection_start_char: projection_source.char_range.start,
            projection_end_char: projection_source.char_range.end,
            projection_backing_start_byte: projection_source.backing_byte_range.start,
            projection_backing_end_byte: projection_source.backing_byte_range.end,
            atomic_source_id: projection_source.source.0,
            atomic_id: placement.id,
            atomic_insertion_byte: placement.insertion_byte,
            atomic_line_index: placement.line_index,
            measurement_constraints: {
                let constraints = placement.measurement.constraints;
                super::RetainedAtomicProjectionMeasureConstraintsSeal {
                    max_width_bits: constraints.max_width.map(f32::to_bits),
                    available_height_bits: constraints.available_height.map(f32::to_bits),
                    viewport_bits: constraints
                        .viewport
                        .map(|size| [size.width.to_bits(), size.height.to_bits()]),
                    percent_base_width_bits: constraints.percent_base.width.map(f32::to_bits),
                    percent_base_height_bits: constraints.percent_base.height.map(f32::to_bits),
                    min_width_bits: constraints.sizing.min_width.map(f32::to_bits),
                    max_sizing_width_bits: constraints.sizing.max_width.map(f32::to_bits),
                    min_height_bits: constraints.sizing.min_height.map(f32::to_bits),
                    max_height_bits: constraints.sizing.max_height.map(f32::to_bits),
                    intrinsic_size: constraints.sizing.intrinsic_size.map(|intrinsic| {
                        super::RetainedAtomicProjectionIntrinsicSizeSeal {
                            min_content_width_bits: intrinsic.min_content_width.to_bits(),
                            max_content_width_bits: intrinsic.max_content_width.to_bits(),
                            preferred_width_bits: intrinsic.preferred_width.map(f32::to_bits),
                            preferred_height_bits: intrinsic.preferred_height.map(f32::to_bits),
                        }
                    }),
                }
            },
            measured_size_bits: [
                placement.measurement.measured_size.width.to_bits(),
                placement.measurement.measured_size.height.to_bits(),
            ],
            placement_rect_bits: [
                placement.rect.x.to_bits(),
                placement.rect.y.to_bits(),
                placement.rect.width.to_bits(),
                placement.rect.height.to_bits(),
            ],
            projection_segment_bounds_bits: [
                projection_snapshot.x.to_bits(),
                projection_snapshot.y.to_bits(),
                projection_snapshot.width.to_bits(),
                projection_snapshot.height.to_bits(),
            ],
            projection_text_bounds_bits: [
                projection_text_snapshot.x.to_bits(),
                projection_text_snapshot.y.to_bits(),
                projection_text_snapshot.width.to_bits(),
                projection_text_snapshot.height.to_bits(),
            ],
            flow_offset_bits: [
                witness.flow_offset.x.to_bits(),
                witness.flow_offset.y.to_bits(),
            ],
            owner_inline_baseline_bits: witness.owner_inline_baseline.to_bits(),
            inline_full_available_width_bits: witness.inline_full_available_width.to_bits(),
            auto_wrap: witness.auto_wrap,
            vertical_align: witness.vertical_align,
            unified_ifc_source_revision: self.unified_ifc_source_revision.get(),
            last_unified_apply_bits,
            topology: Arc::from(topology),
        };
        let atomic_source =
            super::RetainedAtomicProjectionTextAreaPaintGrammar::from_frozen_source_identity(
                frozen,
            )?;
        Some(AtomicProjectionSourceCoreSeal { atomic_source })
    }

    /// Closed component-owned oracle for the first property-scroll TextArea
    /// subtree.  It accepts only the generated plain-run grammar and exactly
    /// one glyph payload; all interactive/projection paint stays fail-closed.
    pub(crate) fn exact_retained_property_scroll_glyph_subtree(
        &self,
        owner: NodeKey,
        arena: &NodeArena,
        parent_paint_offset: [f32; 2],
    ) -> bool {
        if self.self_node_key != Some(owner)
            || self.on_render_handler.is_some()
            || self.is_focused
            || self.caret_visible
            || self.pointer_selecting
            || self.selection_anchor_char.is_some()
            || self.selection_focus_char.is_some()
            || !self.ime_preedit.is_empty()
            || self.ime_preedit_cursor.is_some()
            || self.has_active_animator()
            || self.is_deferred_to_root_viewport_render()
            || self.children.is_empty()
            || self.children.as_slice() != arena.children_of(owner).as_slice()
            || parent_paint_offset.iter().any(|value| !value.is_finite())
        {
            return false;
        }
        let paint_x = self.layout_state.layout_position.x + parent_paint_offset[0];
        let paint_y = self.layout_state.layout_position.y + parent_paint_offset[1];
        if (round_layout_value(paint_x) - paint_x).to_bits() != 0.0_f32.to_bits()
            || (round_layout_value(paint_y) - paint_y).to_bits() != 0.0_f32.to_bits()
        {
            return false;
        }
        for &child in &self.children {
            let Some(node) = arena.get(child) else {
                return false;
            };
            if arena.parent_of(child) != Some(owner)
                || !node.element.children().is_empty()
                || !(node.element.as_any().is::<TextAreaTextRun>()
                    || node.element.as_any().is::<TextAreaLineBreak>())
                || node.element.is_deferred_to_root_viewport_render()
                || node.element.has_active_animator()
            {
                return false;
            }
        }
        matches!(
            self.prepared_plain_shadow_text_payload(owner, arena, false, parent_paint_offset),
            Ok(PlainTextAreaPaintPayload {
                glyph_op: Some(_),
                selection: None,
                decoration: None,
                caret: None,
                ..
            })
        )
    }

    /// Closed C2a sibling oracle. It admits only one non-focused, nonempty
    /// plain-run selection followed by the existing glyph payload. Keeping it
    /// separate preserves the glyph-only C1 contract above.
    pub(crate) fn exact_retained_property_scroll_selection_glyph_subtree(
        &self,
        owner: NodeKey,
        arena: &NodeArena,
        parent_paint_offset: [f32; 2],
    ) -> Option<super::RetainedTextAreaPaintGrammar> {
        if self.self_node_key != Some(owner)
            || self.on_render_handler.is_some()
            || self.is_focused
            || self.caret_visible
            || self.pointer_selecting
            || self.pending_caret_scroll
            || !self.ime_preedit.is_empty()
            || self.ime_preedit_cursor.is_some()
            || self.has_active_animator()
            || self.is_deferred_to_root_viewport_render()
            || self.children.is_empty()
            || self.children.as_slice() != arena.children_of(owner).as_slice()
            || parent_paint_offset.iter().any(|value| !value.is_finite())
        {
            return None;
        }
        let (Some(anchor), Some(focus)) = (self.selection_anchor_char, self.selection_focus_char)
        else {
            return None;
        };
        let content_chars = self.content.chars().count();
        if anchor > content_chars || focus > content_chars || anchor == focus {
            return None;
        }
        let range = anchor.min(focus)..anchor.max(focus);
        let paint_x = self.layout_state.layout_position.x + parent_paint_offset[0];
        let paint_y = self.layout_state.layout_position.y + parent_paint_offset[1];
        if (round_layout_value(paint_x) - paint_x).to_bits() != 0.0_f32.to_bits()
            || (round_layout_value(paint_y) - paint_y).to_bits() != 0.0_f32.to_bits()
        {
            return None;
        }
        for &child in &self.children {
            let node = arena.get(child)?;
            if arena.parent_of(child) != Some(owner)
                || !node.element.children().is_empty()
                || !(node.element.as_any().is::<TextAreaTextRun>()
                    || node.element.as_any().is::<TextAreaLineBreak>())
                || node.element.is_deferred_to_root_viewport_render()
                || node.element.has_active_animator()
            {
                return None;
            }
        }
        let PlainTextAreaPaintPayload {
            glyph_op: Some(glyph),
            selection: Some(selection),
            decoration: None,
            caret: None,
            ..
        } = self
            .prepared_plain_shadow_text_payload(owner, arena, false, parent_paint_offset)
            .ok()?
        else {
            return None;
        };
        if !glyph.has_canonical_identity()
            || selection.ops.is_empty()
            || crate::view::paint::PaintPayloadIdentity::prepared_rects(selection.ops.iter())
                .is_none()
        {
            return None;
        }
        Some(super::RetainedTextAreaPaintGrammar::SelectionGlyphs {
            start_char: range.start,
            end_char: range.end,
            color_rgba_bits: self
                .selection_background_color
                .to_rgba_f32()
                .map(f32::to_bits),
        })
    }

    /// Closed C2b/C2c sibling oracle. It admits focused generated-run content
    /// while keeping the caret outside the resident base. Projection,
    /// stateful hooks, in-progress pointer selection and pending caret-follow
    /// remain fail-closed.
    pub(crate) fn exact_retained_property_scroll_interactive_subtree(
        &self,
        owner: NodeKey,
        arena: &NodeArena,
        parent_paint_offset: [f32; 2],
    ) -> Option<super::RetainedInteractiveTextAreaPaintGrammar> {
        if self.self_node_key != Some(owner)
            || self.on_render_handler.is_some()
            || !self.is_focused
            || !self.layout_state.should_render
            || self.pointer_selecting
            || self.pending_caret_scroll
            || self.has_active_animator()
            || self.is_deferred_to_root_viewport_render()
            || self.children.is_empty()
            || self.children.as_slice() != arena.children_of(owner).as_slice()
            || parent_paint_offset.iter().any(|value| !value.is_finite())
            || self.cursor_char > self.content.chars().count()
        {
            return None;
        }
        let paint_x = self.layout_state.layout_position.x + parent_paint_offset[0];
        let paint_y = self.layout_state.layout_position.y + parent_paint_offset[1];
        if (round_layout_value(paint_x) - paint_x).to_bits() != 0.0_f32.to_bits()
            || (round_layout_value(paint_y) - paint_y).to_bits() != 0.0_f32.to_bits()
        {
            return None;
        }
        for &child in &self.children {
            let node = arena.get(child)?;
            if arena.parent_of(child) != Some(owner)
                || !node.element.children().is_empty()
                || !(node.element.as_any().is::<TextAreaTextRun>()
                    || node.element.as_any().is::<TextAreaLineBreak>())
                || node.element.is_deferred_to_root_viewport_render()
                || node.element.has_active_animator()
            {
                return None;
            }
        }
        let content_chars = self.content.chars().count();
        let selection = match (self.selection_anchor_char, self.selection_focus_char) {
            (None, None) => None,
            (Some(anchor), Some(focus)) if anchor <= content_chars && focus <= content_chars => {
                (anchor != focus).then_some(anchor.min(focus)..anchor.max(focus))
            }
            _ => return None,
        };
        let preedit_active = !self.ime_preedit.is_empty();
        if !preedit_active && self.ime_preedit_cursor.is_some() {
            return None;
        }
        if preedit_active {
            if selection.is_some() || self.selection_anchor_char.is_some() {
                return None;
            }
            if let Some((start, end)) = self.ime_preedit_cursor {
                if start > end
                    || end > self.ime_preedit.len()
                    || !self.ime_preedit.is_char_boundary(start)
                    || !self.ime_preedit.is_char_boundary(end)
                {
                    return None;
                }
            }
        }
        let payload = self
            .prepared_plain_shadow_text_payload(owner, arena, false, parent_paint_offset)
            .ok()?;
        let glyph = payload.glyph_op.as_ref()?;
        if !glyph.has_canonical_identity() {
            return None;
        }
        if preedit_active {
            let decoration = payload.decoration.as_ref()?;
            if payload.selection.is_some()
                || decoration.ops.is_empty()
                || crate::view::paint::PaintPayloadIdentity::prepared_rects(decoration.ops.iter())
                    .is_none()
            {
                return None;
            }
            return Some(super::RetainedInteractiveTextAreaPaintGrammar::FocusedPreeditGlyphs);
        }
        if payload.decoration.is_some() {
            return None;
        }
        match (selection, payload.selection.as_ref()) {
            (None, None) => Some(super::RetainedInteractiveTextAreaPaintGrammar::FocusedGlyphs),
            (Some(range), Some(selection_payload))
                if !selection_payload.ops.is_empty()
                    && crate::view::paint::PaintPayloadIdentity::prepared_rects(
                        selection_payload.ops.iter(),
                    )
                    .is_some() =>
            {
                Some(
                    super::RetainedInteractiveTextAreaPaintGrammar::FocusedSelectionGlyphs {
                        start_char: range.start,
                        end_char: range.end,
                        color_rgba_bits: self
                            .selection_background_color
                            .to_rgba_f32()
                            .map(f32::to_bits),
                    },
                )
            }
            _ => None,
        }
    }

    pub(crate) fn retained_interactive_preedit_raster_seal(
        &self,
        owner: NodeKey,
        arena: &NodeArena,
        parent_paint_offset: [f32; 2],
    ) -> Option<crate::view::paint::RetainedTextAreaPreeditRasterSeal> {
        if self.exact_retained_property_scroll_interactive_subtree(
            owner,
            arena,
            parent_paint_offset,
        )? != super::RetainedInteractiveTextAreaPaintGrammar::FocusedPreeditGlyphs
        {
            return None;
        }
        let actual_payload = self
            .prepared_plain_shadow_text_payload(owner, arena, false, parent_paint_offset)
            .ok()?;
        let actual_glyph = actual_payload.glyph_op.as_ref()?;
        let actual_decoration = actual_payload.decoration.as_ref()?;
        if actual_payload.selection.is_some() || actual_decoration.ops.is_empty() {
            return None;
        }
        let actual_glyph_identity =
            crate::view::paint::PaintPayloadIdentity::prepared_texts([actual_glyph]);
        let actual_underline_identity =
            crate::view::paint::PaintPayloadIdentity::prepared_rects(actual_decoration.ops.iter())?;
        let package = self
            .exact_plain_unified_package(owner, arena, false)
            .ok()??;
        let mut generated_topology = Vec::with_capacity(package.source_segments.len());
        for (topology_index, segment) in package.source_segments.iter().enumerate() {
            let node = arena.get(segment.child_key)?;
            let stable_id = node.element.stable_id();
            let (kind, text, preedit_cursor) =
                if let Some(run) = node.element.as_any().downcast_ref::<TextAreaTextRun>() {
                    (
                        if run.is_preedit_run {
                            crate::view::paint::RetainedTextAreaGeneratedNodeKind::PreeditRun
                        } else {
                            crate::view::paint::RetainedTextAreaGeneratedNodeKind::TextRun
                        },
                        Arc::<str>::from(run.text.as_str()),
                        run.preedit_cursor,
                    )
                } else if node
                    .element
                    .as_any()
                    .downcast_ref::<TextAreaLineBreak>()
                    .is_some()
                {
                    (
                        crate::view::paint::RetainedTextAreaGeneratedNodeKind::LineBreak,
                        Arc::<str>::from(""),
                        None,
                    )
                } else {
                    return None;
                };
            generated_topology.push(crate::view::paint::RetainedTextAreaGeneratedNodeSeal {
                topology_index,
                owner: segment.child_key,
                parent: owner,
                stable_id,
                source_id: segment.source.0,
                kind,
                char_range: segment.char_range.clone(),
                backing_byte_range: segment.backing_byte_range.clone(),
                preedit_backing_byte_range: segment.preedit_backing_byte_range.clone(),
                preedit_caret_backing_byte: segment.preedit_caret_backing_byte,
                text,
                preedit_cursor,
            });
        }
        let seal = crate::view::paint::RetainedTextAreaPreeditRasterSeal {
            text_area_root: owner,
            paint_grammar: super::RetainedInteractiveTextAreaPaintGrammar::FocusedPreeditGlyphs,
            content: Arc::from(self.content.as_str()),
            backing_text: Arc::from(package.ifc.backing_text()),
            ime_preedit: Arc::from(self.ime_preedit.as_str()),
            ime_preedit_cursor: self.ime_preedit_cursor,
            cursor_char: self.cursor_char,
            cursor_affinity: self.cursor_affinity,
            unified_ifc_source_revision: self.unified_ifc_source_revision.get(),
            last_unified_apply_bits: self
                .last_unified_apply
                .get()
                .map(|(x, y, revision)| (x.to_bits(), y.to_bits(), revision)),
            generated_topology: generated_topology.into(),
            foreground_color_bits: self.color.to_rgba_f32().map(f32::to_bits),
            glyph_bounds_bits: [
                actual_payload.glyph_bounds.x,
                actual_payload.glyph_bounds.y,
                actual_payload.glyph_bounds.width,
                actual_payload.glyph_bounds.height,
            ]
            .map(f32::to_bits),
            underline_bounds_bits: [
                actual_decoration.bounds.x,
                actual_decoration.bounds.y,
                actual_decoration.bounds.width,
                actual_decoration.bounds.height,
            ]
            .map(f32::to_bits),
            glyph_identity: actual_glyph_identity,
            underline_identity: actual_underline_identity,
        };
        seal.is_canonical().then_some(seal)
    }

    pub(crate) fn retained_interactive_caret_oracle_bounds_bits(
        &self,
        owner: NodeKey,
        arena: &NodeArena,
        admission_parent_paint_offset: [f32; 2],
        live_parent_paint_offset: [f32; 2],
        admitted_grammar: super::RetainedInteractiveTextAreaPaintGrammar,
    ) -> Option<Option<[u32; 4]>> {
        if self.exact_retained_property_scroll_interactive_subtree(
            owner,
            arena,
            admission_parent_paint_offset,
        ) != Some(admitted_grammar)
        {
            return None;
        }
        let effective_offset = self.effective_paint_offset(arena, live_parent_paint_offset);
        let caret = self.caret_draw_rect_payload(arena, effective_offset).ok()?;
        Some(caret.map(|caret| {
            [
                caret.bounds.x,
                caret.bounds.y,
                caret.bounds.width,
                caret.bounds.height,
            ]
            .map(f32::to_bits)
        }))
    }

    pub(crate) fn retained_interactive_caret_overlay(
        &self,
        owner: NodeKey,
        arena: &NodeArena,
        admission_parent_paint_offset: [f32; 2],
        live_parent_paint_offset: [f32; 2],
        text_area_clip: crate::view::compositor::property_tree::ClipNodeSnapshot,
        outer_clip: crate::view::compositor::property_tree::ClipNodeSnapshot,
        admitted_grammar: super::RetainedInteractiveTextAreaPaintGrammar,
        admitted_caret_oracle_bounds_bits: Option<[u32; 4]>,
    ) -> Option<crate::view::paint::RecordedRetainedTextAreaCaretOverlay> {
        if self.exact_retained_property_scroll_interactive_subtree(
            owner,
            arena,
            admission_parent_paint_offset,
        ) != Some(admitted_grammar)
        {
            return None;
        }
        let effective_offset = self.effective_paint_offset(arena, live_parent_paint_offset);
        let caret = self.caret_draw_rect_payload(arena, effective_offset).ok()?;
        let oracle_bounds_bits = caret.as_ref().map(|caret| {
            [
                caret.bounds.x,
                caret.bounds.y,
                caret.bounds.width,
                caret.bounds.height,
            ]
            .map(f32::to_bits)
        });
        if oracle_bounds_bits != admitted_caret_oracle_bounds_bits {
            return None;
        }
        let (paint, op) = if let Some(caret) = caret {
            let bounds_bits = [
                caret.bounds.x,
                caret.bounds.y,
                caret.bounds.width,
                caret.bounds.height,
            ]
            .map(f32::to_bits);
            let payload_identity =
                crate::view::paint::PaintPayloadIdentity::prepared_rects([&caret.op])?;
            let visible = live_caret_bounds_intersect_clip_chain(
                &caret.bounds,
                text_area_clip.logical_scissor,
                outer_clip.logical_scissor,
            );
            if visible {
                (
                    crate::view::paint::RetainedTextAreaCaretOverlayPaintIdentity::Visible {
                        bounds_bits,
                        payload_identity,
                    },
                    Some(caret.op),
                )
            } else {
                (
                    crate::view::paint::RetainedTextAreaCaretOverlayPaintIdentity::Culled {
                        bounds_bits,
                        payload_identity,
                    },
                    None,
                )
            }
        } else {
            (
                crate::view::paint::RetainedTextAreaCaretOverlayPaintIdentity::Hidden,
                None,
            )
        };
        let overlay = crate::view::paint::RecordedRetainedTextAreaCaretOverlay {
            identity: crate::view::paint::RetainedTextAreaCaretOverlayIdentity {
                owner,
                stable_id: self.node_id,
                focused: self.is_focused,
                should_render: self.layout_state.should_render,
                caret_visible: self.caret_visible,
                foreground_color_bits: self.color.to_rgba_f32().map(f32::to_bits),
                cursor_char: self.cursor_char,
                cursor_affinity: self.cursor_affinity,
                ime_preedit_cursor: self.ime_preedit_cursor,
                local_scroll_bits: [self.scroll_x.to_bits(), self.scroll_y.to_bits()],
                unified_ifc_source_revision: self.unified_ifc_source_revision.get(),
                last_unified_apply_bits: self
                    .last_unified_apply
                    .get()
                    .map(|(x, y, revision)| (x.to_bits(), y.to_bits(), revision)),
                oracle_bounds_bits,
                text_area_clip,
                outer_clip,
                paint,
            },
            op,
        };
        overlay.is_canonical().then_some(overlay)
    }

    /// Recomputes the TextArea contents clip in the detached scroll-content
    /// coordinate space.  Recomputing from float layout geometry avoids trying
    /// to translate an already quantized live scissor for fractional offsets.
    pub(crate) fn retained_property_scroll_local_contents_scissor(
        &self,
        normalization_offset: [f32; 2],
    ) -> Option<[u32; 4]> {
        normalization_offset
            .iter()
            .all(|value| value.is_finite())
            .then(|| {
                rect_to_logical_scissor_rect(Rect {
                    x: self.layout_state.layout_position.x + normalization_offset[0],
                    y: self.layout_state.layout_position.y + normalization_offset[1],
                    width: self.viewport_size.width,
                    height: self.viewport_size.height,
                })
            })
    }

    fn viewport_scissor_rect(&self) -> Option<[u32; 4]> {
        Some(self.viewport_logical_scissor_rect())
    }

    pub(super) fn viewport_logical_scissor_rect(&self) -> [u32; 4] {
        let rect = Rect {
            x: self.layout_state.layout_position.x,
            y: self.layout_state.layout_position.y,
            width: self.viewport_size.width,
            height: self.viewport_size.height,
        };
        rect_to_logical_scissor_rect(rect)
    }
}

fn live_caret_bounds_intersect_clip_chain(
    bounds: &crate::view::base_component::Rect,
    text_area_scissor: [u32; 4],
    outer_scissor: [u32; 4],
) -> bool {
    let left = text_area_scissor[0].max(outer_scissor[0]) as f32;
    let top = text_area_scissor[1].max(outer_scissor[1]) as f32;
    let (Some(text_right), Some(outer_right), Some(text_bottom), Some(outer_bottom)) = (
        text_area_scissor[0].checked_add(text_area_scissor[2]),
        outer_scissor[0].checked_add(outer_scissor[2]),
        text_area_scissor[1].checked_add(text_area_scissor[3]),
        outer_scissor[1].checked_add(outer_scissor[3]),
    ) else {
        return false;
    };
    let right = text_right.min(outer_right) as f32;
    let bottom = text_bottom.min(outer_bottom) as f32;
    bounds.x < right
        && bounds.x + bounds.width > left
        && bounds.y < bottom
        && bounds.y + bounds.height > top
}

fn rect_to_logical_scissor_rect(rect: Rect) -> [u32; 4] {
    let left = rect.x.floor().max(0.0) as i64;
    let top = rect.y.floor().max(0.0) as i64;
    let right = (rect.x + rect.width).ceil().max(0.0) as i64;
    let bottom = (rect.y + rect.height).ceil().max(0.0) as i64;
    [
        u32::try_from(left).unwrap_or(u32::MAX),
        u32::try_from(top).unwrap_or(u32::MAX),
        if rect.width <= 0.0 {
            0
        } else {
            u32::try_from(right.saturating_sub(left).max(0)).unwrap_or(u32::MAX)
        },
        if rect.height <= 0.0 {
            0
        } else {
            u32::try_from(bottom.saturating_sub(top).max(0)).unwrap_or(u32::MAX)
        },
    ]
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
mod tests {
    use super::*;
    use crate::style::{Layout, Length, ParsedValue, PropertyId, ScrollDirection, Style};
    use crate::ui::{RsxNode, RsxTagDescriptor};
    use crate::view::ElementStylePropSchema;
    use crate::view::base_component::{
        Element, ElementTrait, EventTarget, LayoutConstraints, LayoutPlacement, Size, Text,
    };
    use crate::view::frame_graph::FrameGraph;
    use std::cell::Cell;
    use std::rc::Rc;

    #[test]
    fn retained_caret_blink_has_deterministic_boundaries_and_paint_only_dirty() {
        let mut text_area = TextArea::new();
        text_area.layout_state.should_render = true;
        assert!(!text_area.caret_visible);
        assert!(text_area.caret_blink_epoch.is_none());

        assert!(text_area.set_focused(true));
        assert!(text_area.caret_visible);
        assert!(text_area.caret_blink_epoch.is_none());
        assert!(
            <TextArea as crate::view::base_component::EventTarget>::wants_animation_frame(
                &text_area
            )
        );

        let t0 = crate::time::Instant::now();
        text_area.dirty_flags = DirtyFlags::NONE;
        assert_eq!(text_area.tick_caret_blink(t0), DirtyFlags::NONE);
        assert_eq!(text_area.caret_blink_epoch, Some(t0));
        assert!(text_area.caret_visible);

        assert_eq!(
            text_area.tick_caret_blink(t0 + Duration::from_millis(529)),
            DirtyFlags::NONE
        );
        assert!(text_area.caret_visible);
        assert!(text_area.dirty_flags.is_empty());

        assert_eq!(
            text_area.tick_caret_blink(t0 + Duration::from_millis(530)),
            DirtyFlags::PAINT
        );
        assert!(!text_area.caret_visible);
        assert_eq!(text_area.dirty_flags, DirtyFlags::PAINT);
        assert!(
            <TextArea as crate::view::base_component::EventTarget>::wants_animation_frame(
                &text_area
            ),
            "the invisible blink phase must keep requesting frames"
        );

        text_area.dirty_flags = DirtyFlags::NONE;
        assert_eq!(
            text_area.tick_caret_blink(t0 + Duration::from_millis(1059)),
            DirtyFlags::NONE
        );
        assert!(!text_area.caret_visible);
        assert!(text_area.dirty_flags.is_empty());
        assert_eq!(
            text_area.tick_caret_blink(t0 + Duration::from_millis(1060)),
            DirtyFlags::PAINT
        );
        assert!(text_area.caret_visible);
        assert_eq!(text_area.dirty_flags, DirtyFlags::PAINT);
        assert!(
            !text_area.dirty_flags.intersects(
                DirtyFlags::LAYOUT
                    .union(DirtyFlags::PLACE)
                    .union(DirtyFlags::BOX_MODEL)
                    .union(DirtyFlags::HIT_TEST)
                    .union(DirtyFlags::COMPOSITE)
            )
        );
    }

    #[test]
    fn retained_caret_focus_reset_blur_and_unrender_restart_without_clock_reads() {
        let mut text_area = TextArea::new();
        text_area.layout_state.should_render = true;
        text_area.set_focused(true);
        let t0 = crate::time::Instant::now();
        assert_eq!(text_area.tick_caret_blink(t0), DirtyFlags::NONE);
        assert_eq!(
            text_area.tick_caret_blink(t0 + Duration::from_millis(530)),
            DirtyFlags::PAINT
        );
        assert!(!text_area.caret_visible);

        text_area.dirty_flags = DirtyFlags::NONE;
        text_area.reset_caret_blink();
        assert!(text_area.caret_visible);
        assert!(text_area.caret_blink_epoch.is_none());
        assert_eq!(text_area.dirty_flags, DirtyFlags::PAINT);

        text_area.caret_visible = false;
        text_area.caret_blink_epoch = Some(t0);
        assert!(text_area.insert_text("x"));
        assert!(text_area.caret_visible);
        assert!(text_area.caret_blink_epoch.is_none());

        text_area.dirty_flags = DirtyFlags::NONE;
        text_area.layout_state.should_render = false;
        assert_eq!(
            text_area.tick_caret_blink(t0 + Duration::from_millis(600)),
            DirtyFlags::PAINT
        );
        assert!(!text_area.caret_visible);
        assert!(text_area.caret_blink_epoch.is_none());
        assert!(
            !<TextArea as crate::view::base_component::EventTarget>::wants_animation_frame(
                &text_area
            )
        );

        text_area.dirty_flags = DirtyFlags::NONE;
        text_area.layout_state.should_render = true;
        assert_eq!(
            text_area.tick_caret_blink(t0 + Duration::from_millis(700)),
            DirtyFlags::PAINT
        );
        assert!(text_area.caret_visible);
        assert_eq!(
            text_area.caret_blink_epoch,
            Some(t0 + Duration::from_millis(700))
        );

        text_area.dirty_flags = DirtyFlags::NONE;
        assert!(text_area.set_focused(false));
        assert!(!text_area.caret_visible);
        assert!(text_area.caret_blink_epoch.is_none());
        assert_eq!(text_area.dirty_flags, DirtyFlags::PAINT);
        assert!(
            !<TextArea as crate::view::base_component::EventTarget>::wants_animation_frame(
                &text_area
            )
        );
    }

    #[test]
    fn legacy_and_typed_viewport_scissor_preserve_explicit_empty() {
        assert_eq!(
            rect_to_logical_scissor_rect(Rect {
                x: 12.0,
                y: 18.0,
                width: 0.0,
                height: 20.0,
            }),
            [12, 18, 0, 20]
        );
        assert_eq!(
            rect_to_logical_scissor_rect(Rect {
                x: -5.0,
                y: -7.0,
                width: -10.0,
                height: -20.0,
            }),
            [0, 0, 0, 0]
        );

        let mut text_area = TextArea::new();
        text_area.layout_state.layout_position =
            crate::view::base_component::Position { x: 12.0, y: 18.0 };
        text_area.viewport_size = crate::view::base_component::Size {
            width: 0.0,
            height: 20.0,
        };
        assert_eq!(text_area.viewport_logical_scissor_rect(), [12, 18, 0, 20]);
        assert_eq!(text_area.viewport_scissor_rect(), Some([12, 18, 0, 20]));
        assert_eq!(
            ElementTrait::contents_logical_scissor(&text_area),
            Some([12, 18, 0, 20]),
            "legacy and typed clip authorities must agree on explicit empty",
        );
    }

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

    fn retained_atomic_projection_fixture_with(
        content: &str,
        projection_range: std::ops::Range<usize>,
        projected_text: &str,
    ) -> (NodeArena, NodeKey, NodeKey, NodeKey, Rc<Cell<usize>>) {
        retained_atomic_projection_fixture_with_selection(
            content,
            projection_range,
            projected_text,
            None,
        )
    }

    fn retained_atomic_projection_fixture_with_selection(
        content: &str,
        projection_range: std::ops::Range<usize>,
        projected_text: &str,
        selection: Option<(usize, usize)>,
    ) -> (NodeArena, NodeKey, NodeKey, NodeKey, Rc<Cell<usize>>) {
        let call_count = Rc::new(Cell::new(0));
        let handler_call_count = Rc::clone(&call_count);
        let projected_text = projected_text.to_string();
        let mut text_area = TextArea::new();
        text_area.content = content.to_string();
        text_area.font_size = 14.0;
        text_area.line_height = 1.25;
        if let Some((anchor, focus)) = selection {
            text_area.selection_anchor_char = Some(anchor);
            text_area.selection_focus_char = Some(focus);
        }
        text_area.on_render_handler = Some(crate::ui::on_text_area_render(move |render| {
            handler_call_count.set(handler_call_count.get() + 1);
            let projected_text = projected_text.clone();
            render.range(projection_range.clone(), move |_text_area_node| {
                RsxNode::text(projected_text)
            });
        }));

        let mut arena = crate::view::test_support::new_test_arena();
        let root = crate::view::test_support::commit_element(
            &mut arena,
            Box::new(text_area) as Box<dyn ElementTrait>,
        );
        arena.with_element_taken(root, |element, _| {
            element
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .expect("TextArea root")
                .set_self_node_key(root);
        });
        crate::view::test_support::measure_and_place(
            &mut arena,
            root,
            LayoutConstraints {
                max_width: 132.0,
                max_height: 240.0,
                viewport_width: 320.0,
                viewport_height: 240.0,
                percent_base_width: Some(320.0),
                percent_base_height: Some(240.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 132.0,
                available_height: 240.0,
                viewport_width: 320.0,
                viewport_height: 240.0,
                percent_base_width: Some(320.0),
                percent_base_height: Some(240.0),
            },
        );
        let projection = arena
            .children_of(root)
            .into_iter()
            .find(|&child| {
                arena
                    .get(child)
                    .is_some_and(|node| node.element.as_any().is::<TextAreaProjectionSegment>())
            })
            .expect("fixture must realize one projection segment");
        let projection_children = arena.children_of(projection);
        let [projection_text] = projection_children.as_slice() else {
            panic!("fixture projection must own one direct Text leaf")
        };
        let projection_text = *projection_text;
        assert!(
            arena
                .get(projection_text)
                .unwrap()
                .element
                .as_any()
                .is::<Text>()
        );
        let mut stack = vec![root];
        while let Some(key) = stack.pop() {
            stack.extend(arena.children_of(key));
            arena
                .get_mut(key)
                .unwrap()
                .element
                .clear_local_dirty_flags(DirtyFlags::ALL);
        }
        arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
        arena.refresh_subtree_dirty_cache(root);
        (arena, root, projection, projection_text, call_count)
    }

    fn retained_atomic_projection_fixture()
    -> (NodeArena, NodeKey, NodeKey, NodeKey, Rc<Cell<usize>>) {
        retained_atomic_projection_fixture_with("before projected after", 7..16, "projected")
    }

    #[test]
    fn retained_atomic_projection_source_oracle_is_exact_and_never_calls_handler() {
        let (arena, root, projection, projected_text, call_count) =
            retained_atomic_projection_fixture();
        assert_eq!(call_count.get(), 1, "layout realizes the handler once");
        let text_area_node = arena.get(root).unwrap();
        let text_area = text_area_node
            .element
            .as_any()
            .downcast_ref::<TextArea>()
            .unwrap();
        let grammar = text_area
            .exact_retained_property_scroll_atomic_projection_subtree(root, &arena, [0.0, 0.0])
            .expect("single bare Text projection must satisfy C3a source authority");
        assert!(grammar.is_canonical());
        assert_eq!(grammar.projection_owner, projection);
        assert_eq!(grammar.projection_text_owner, projected_text);
        assert_eq!(
            (grammar.projection_start_char, grammar.projection_end_char),
            (7, 16)
        );
        assert_eq!(
            grammar.projection_backing_start_byte,
            grammar.projection_backing_end_byte
        );
        assert_eq!(grammar.topology.len(), 3);
        assert_eq!(
            grammar.topology[grammar.projection_index].kind,
            super::super::RetainedAtomicProjectionTextAreaTopologyKind::ProjectionSegment
        );
        assert!(
            !text_area.exact_retained_property_scroll_glyph_subtree(root, &arena, [0.0, 0.0]),
            "C1 must remain projection-free"
        );
        assert!(
            text_area
                .exact_retained_property_scroll_selection_glyph_subtree(root, &arena, [0.0, 0.0])
                .is_none(),
            "C2a must remain projection-free"
        );
        assert!(
            text_area
                .exact_retained_property_scroll_interactive_subtree(root, &arena, [0.0, 0.0])
                .is_none(),
            "C2b/C2c must remain projection-free"
        );
        assert!(
            text_area
                .exact_retained_property_scroll_atomic_projection_subtree(root, &arena, [0.0, 0.0],)
                .is_some()
        );
        assert_eq!(
            call_count.get(),
            1,
            "source admission must never execute the FnMut handler"
        );
    }

    #[test]
    fn focused_atomic_projection_source_seals_visible_hidden_and_culled_caret_without_handler() {
        for (case, caret_visible, parent_paint_offset) in [
            ("visible", true, [0.0, 0.0]),
            ("hidden", false, [0.0, 0.0]),
            // Source authority does not classify clip visibility. A far-away
            // present caret is still sealed and can be culled only later.
            ("culled_source", true, [10_000.0, 10_000.0]),
        ] {
            let (arena, root, projection, _, call_count) = retained_atomic_projection_fixture();
            {
                let mut node = arena.get_mut(root).unwrap();
                let text_area = node
                    .element
                    .as_any_mut()
                    .downcast_mut::<TextArea>()
                    .unwrap();
                text_area.is_focused = true;
                text_area.caret_visible = caret_visible;
                text_area.cursor_char = 7;
            }
            let node = arena.get(root).unwrap();
            let text_area = node.element.as_any().downcast_ref::<TextArea>().unwrap();
            let grammar = text_area
                .exact_retained_property_scroll_focused_atomic_projection_glyph_subtree(
                    root,
                    &arena,
                    parent_paint_offset,
                )
                .unwrap_or_else(|| panic!("{case} focused source must admit"));
            assert!(grammar.is_canonical(), "{case}");
            assert_eq!(grammar.atomic_source.projection_owner, projection, "{case}");
            assert_eq!(grammar.caret.cursor_char, 7, "{case}");
            match (&grammar.caret.paint, caret_visible) {
                (super::super::FocusedAtomicCaretSourcePaintSeal::Hidden, false) => {}
                (
                    super::super::FocusedAtomicCaretSourcePaintSeal::Present {
                        bounds_bits,
                        payload_identity,
                    },
                    true,
                ) => {
                    assert_eq!(bounds_bits[2], 1.0_f32.to_bits(), "{case}");
                    assert!(matches!(
                        payload_identity,
                        crate::view::paint::PaintPayloadIdentity::PreparedRects(rects)
                            if rects.len() == 1
                    ));
                }
                _ => panic!("{case} caret source classification drifted"),
            }
            assert_eq!(
                call_count.get(),
                1,
                "{case}: source oracle must not rerun on_render",
            );
            assert!(
                text_area
                    .exact_retained_property_scroll_atomic_projection_subtree(
                        root,
                        &arena,
                        parent_paint_offset,
                    )
                    .is_none(),
                "the existing non-focused atomic grammar must remain closed",
            );
        }
    }

    #[test]
    fn focused_atomic_projection_source_rejects_interaction_stale_dirty_and_multi_states() {
        for case in [
            "unfocused",
            "selection",
            "collapsed_selection",
            "preedit",
            "ime_cursor",
            "pointer",
            "pending",
            "scroll_x",
            "scroll_y",
            "animator",
            "deferred",
            "not_rendered",
            "cursor_oob",
            "children_dirty",
            "stale_package",
            "dirty_projection",
            "multi_projection_source",
        ] {
            let (arena, root, projection, _, call_count) = retained_atomic_projection_fixture();
            {
                let mut node = arena.get_mut(root).unwrap();
                let text_area = node
                    .element
                    .as_any_mut()
                    .downcast_mut::<TextArea>()
                    .unwrap();
                text_area.is_focused = case != "unfocused";
                text_area.caret_visible = true;
                text_area.cursor_char = 7;
                match case {
                    "selection" => {
                        text_area.selection_anchor_char = Some(0);
                        text_area.selection_focus_char = Some(1);
                    }
                    "collapsed_selection" => {
                        text_area.selection_anchor_char = Some(1);
                        text_area.selection_focus_char = Some(1);
                    }
                    "preedit" => text_area.ime_preedit = "x".to_string(),
                    "ime_cursor" => text_area.ime_preedit_cursor = Some((0, 0)),
                    "pointer" => text_area.pointer_selecting = true,
                    "pending" => text_area.pending_caret_scroll = true,
                    "scroll_x" => text_area.scroll_x = 1.0,
                    "scroll_y" => text_area.scroll_y = 1.0,
                    "animator" => text_area.retained_source_test_active_animator = true,
                    "deferred" => text_area.retained_source_test_deferred = true,
                    "not_rendered" => text_area.layout_state.should_render = false,
                    "cursor_oob" => text_area.cursor_char = text_area.content.chars().count() + 1,
                    "children_dirty" => text_area.children_dirty = true,
                    "stale_package" => text_area
                        .unified_ifc_source_revision
                        .set(text_area.unified_ifc_source_revision.get() + 1),
                    "multi_projection_source" => {
                        text_area.tamper_cached_unified_atomic_sources_for_test(true)
                    }
                    "unfocused" | "dirty_projection" => {}
                    _ => unreachable!(),
                }
            }
            if case == "dirty_projection" {
                arena.mark_dirty(projection, DirtyFlags::LAYOUT);
            }
            let node = arena.get(root).unwrap();
            let text_area = node.element.as_any().downcast_ref::<TextArea>().unwrap();
            assert!(
                text_area
                    .exact_retained_property_scroll_focused_atomic_projection_glyph_subtree(
                        root,
                        &arena,
                        [0.0, 0.0],
                    )
                    .is_none(),
                "{case}",
            );
            assert_eq!(call_count.get(), 1, "{case}: callback count drifted");
        }
    }

    #[test]
    fn focused_atomic_projection_source_private_identity_rejects_public_tamper() {
        let (arena, root, ..) = retained_atomic_projection_fixture();
        {
            let mut node = arena.get_mut(root).unwrap();
            let text_area = node
                .element
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .unwrap();
            text_area.is_focused = true;
            text_area.caret_visible = true;
            text_area.cursor_char = 7;
        }
        let node = arena.get(root).unwrap();
        let text_area = node.element.as_any().downcast_ref::<TextArea>().unwrap();
        let grammar = text_area
            .exact_retained_property_scroll_focused_atomic_projection_glyph_subtree(
                root,
                &arena,
                [0.0, 0.0],
            )
            .unwrap();

        let mut cursor = grammar.clone();
        cursor.caret.cursor_char += 1;
        assert!(!cursor.is_canonical(), "cursor drift must fail closed");

        let mut style = grammar.clone();
        let changed_color_bits = [
            0.9_f32.to_bits(),
            0.2_f32.to_bits(),
            0.1_f32.to_bits(),
            1.0_f32.to_bits(),
        ];
        let super::super::FocusedAtomicCaretSourcePaintSeal::Present {
            bounds_bits,
            payload_identity,
        } = &mut style.caret.paint
        else {
            panic!("visible fixture must seal a present caret")
        };
        let [x, y, width, height] = bounds_bits.map(f32::from_bits);
        let changed_style_op = crate::view::paint::DrawRectOp {
            params: RectPassParams {
                position: [x, y],
                size: [width, height],
                fill_color: changed_color_bits.map(f32::from_bits),
                opacity: 1.0,
                ..Default::default()
            },
            mode: RectRenderMode::FillOnly,
        };
        *payload_identity =
            crate::view::paint::PaintPayloadIdentity::prepared_rects([&changed_style_op]).unwrap();
        style.caret.foreground_color_bits = changed_color_bits;
        assert!(
            !style.is_canonical(),
            "synchronized style and payload drift must fail closed",
        );

        let mut bounds = grammar.clone();
        let unchanged_color = bounds.caret.foreground_color_bits.map(f32::from_bits);
        let super::super::FocusedAtomicCaretSourcePaintSeal::Present {
            bounds_bits,
            payload_identity,
        } = &mut bounds.caret.paint
        else {
            panic!("visible fixture must seal a present caret")
        };
        bounds_bits[0] = (f32::from_bits(bounds_bits[0]) + 1.0).to_bits();
        let [x, y, width, height] = bounds_bits.map(f32::from_bits);
        let changed_bounds_op = crate::view::paint::DrawRectOp {
            params: RectPassParams {
                position: [x, y],
                size: [width, height],
                fill_color: unchanged_color,
                opacity: 1.0,
                ..Default::default()
            },
            mode: RectRenderMode::FillOnly,
        };
        *payload_identity =
            crate::view::paint::PaintPayloadIdentity::prepared_rects([&changed_bounds_op]).unwrap();
        assert!(
            !bounds.is_canonical(),
            "synchronized bounds and payload drift must fail closed",
        );

        let mut nested = grammar;
        nested.atomic_source.atomic_line_index += 1;
        assert!(
            !nested.is_canonical(),
            "nested private source identity must remain authoritative",
        );
    }

    #[test]
    fn retained_atomic_projection_selection_source_is_root_owned_and_handler_free() {
        let (arena, root, projection, _, call_count) =
            retained_atomic_projection_fixture_with_selection(
                "before projected after",
                7..16,
                "projected",
                Some((0, 6)),
            );
        let node = arena.get(root).unwrap();
        let text_area = node.element.as_any().downcast_ref::<TextArea>().unwrap();
        let grammar = text_area
            .exact_retained_property_scroll_atomic_projection_selection_subtree(
                root,
                &arena,
                [0.0, 0.0],
            )
            .expect("root-owned selection plus one projection must admit");
        assert!(grammar.is_canonical());
        assert_eq!(grammar.atomic_source.projection_owner, projection);
        assert!(matches!(
            grammar.selection,
            super::super::RetainedTextAreaPaintGrammar::SelectionGlyphs {
                start_char: 0,
                end_char: 6,
                ..
            }
        ));
        assert_eq!(call_count.get(), 1, "oracle must not rerun on_render");
        assert!(
            text_area
                .exact_retained_property_scroll_atomic_projection_subtree(root, &arena, [0.0, 0.0],)
                .is_none(),
            "existing atomic glyph grammar must remain selection-free",
        );

        let mut synchronized = grammar.clone();
        synchronized.selection = super::super::RetainedTextAreaPaintGrammar::SelectionGlyphs {
            start_char: 1,
            end_char: 6,
            color_rgba_bits: match synchronized.selection {
                super::super::RetainedTextAreaPaintGrammar::SelectionGlyphs {
                    color_rgba_bits,
                    ..
                } => color_rgba_bits,
                _ => unreachable!(),
            },
        };
        assert!(
            !synchronized.is_canonical(),
            "private frozen identity must reject a still-valid synchronized public drift",
        );
    }

    #[test]
    fn retained_atomic_projection_selection_source_rejects_outside_bounded_grammar() {
        for case in [
            "empty",
            "projection_owned",
            "crossing_projection",
            "focused",
            "caret",
            "preedit",
            "pointer",
            "pending",
            "inner_scroll",
            "duplicate_atomic",
        ] {
            let (arena, root, ..) = retained_atomic_projection_fixture();
            {
                let mut node = arena.get_mut(root).unwrap();
                let text_area = node
                    .element
                    .as_any_mut()
                    .downcast_mut::<TextArea>()
                    .unwrap();
                text_area.selection_anchor_char = Some(match case {
                    "projection_owned" => 8,
                    "crossing_projection" => 5,
                    _ => 0,
                });
                text_area.selection_focus_char = Some(match case {
                    "empty" => 0,
                    "projection_owned" => 15,
                    "crossing_projection" => 10,
                    _ => 6,
                });
                match case {
                    "focused" => text_area.is_focused = true,
                    "caret" => text_area.caret_visible = true,
                    "preedit" => text_area.ime_preedit = "x".to_string(),
                    "pointer" => text_area.pointer_selecting = true,
                    "pending" => text_area.pending_caret_scroll = true,
                    "inner_scroll" => text_area.scroll_y = 1.0,
                    "duplicate_atomic" => {
                        text_area.tamper_cached_unified_atomic_sources_for_test(true)
                    }
                    _ => {}
                }
            }
            let node = arena.get(root).unwrap();
            let text_area = node.element.as_any().downcast_ref::<TextArea>().unwrap();
            assert!(
                text_area
                    .exact_retained_property_scroll_atomic_projection_selection_subtree(
                        root,
                        &arena,
                        [0.0, 0.0],
                    )
                    .is_none(),
                "{case}",
            );
        }
    }

    #[test]
    fn retained_atomic_projection_source_oracle_rejects_stateful_and_interactive_states() {
        for case in [
            "focused",
            "caret",
            "pointer",
            "pending",
            "selection",
            "preedit",
            "scroll_x",
            "scroll_y",
            "children_dirty",
        ] {
            let (arena, root, ..) = retained_atomic_projection_fixture();
            {
                let mut node = arena.get_mut(root).unwrap();
                let text_area = node
                    .element
                    .as_any_mut()
                    .downcast_mut::<TextArea>()
                    .unwrap();
                match case {
                    "focused" => text_area.is_focused = true,
                    "caret" => text_area.caret_visible = true,
                    "pointer" => text_area.pointer_selecting = true,
                    "pending" => text_area.pending_caret_scroll = true,
                    "selection" => {
                        text_area.selection_anchor_char = Some(0);
                        text_area.selection_focus_char = Some(1);
                    }
                    "preedit" => text_area.ime_preedit = "x".to_string(),
                    "scroll_x" => text_area.scroll_x = 1.0,
                    "scroll_y" => text_area.scroll_y = 1.0,
                    "children_dirty" => text_area.children_dirty = true,
                    _ => unreachable!(),
                }
            }
            let node = arena.get(root).unwrap();
            let text_area = node.element.as_any().downcast_ref::<TextArea>().unwrap();
            assert!(
                text_area
                    .exact_retained_property_scroll_atomic_projection_subtree(
                        root,
                        &arena,
                        [0.0, 0.0],
                    )
                    .is_none(),
                "{case}"
            );
        }
    }

    #[test]
    fn retained_atomic_projection_source_oracle_rejects_package_geometry_and_topology_drift() {
        for case in [
            "segment_width",
            "flow_offset",
            "range",
            "source",
            "backing",
            "atomic_missing",
            "atomic_duplicate",
            "measurement_constraint",
            "measurement_size",
            "insertion",
            "orphan_projection",
            "dirty_projection",
            "extra_projection_child",
            "leaf_geometry",
            "leaf_invisible",
            "leaf_unprepared",
        ] {
            let (mut arena, root, projection, projection_text, _) =
                retained_atomic_projection_fixture();
            let projection_index = arena
                .children_of(root)
                .iter()
                .position(|child| *child == projection)
                .unwrap();
            match case {
                "segment_width" => {
                    let width = arena
                        .get(projection)
                        .unwrap()
                        .element
                        .box_model_snapshot()
                        .width;
                    arena.with_element_taken(projection, |element, _| {
                        element.set_layout_width(width + 1.0);
                    });
                }
                "flow_offset" => {
                    arena.with_element_taken(projection, |element, _| {
                        element.set_layout_offset(999.0, 0.0);
                    });
                }
                "range" => {
                    arena
                        .get_mut(projection)
                        .unwrap()
                        .element
                        .as_any_mut()
                        .downcast_mut::<TextAreaProjectionSegment>()
                        .unwrap()
                        .set_char_range(0..1);
                }
                "source" => arena
                    .get(root)
                    .unwrap()
                    .element
                    .as_any()
                    .downcast_ref::<TextArea>()
                    .unwrap()
                    .tamper_cached_unified_segment_source_for_test(projection_index),
                "backing" => arena
                    .get(root)
                    .unwrap()
                    .element
                    .as_any()
                    .downcast_ref::<TextArea>()
                    .unwrap()
                    .tamper_cached_unified_segment_backing_range_for_test(projection_index, 0..1),
                "atomic_missing" | "atomic_duplicate" => arena
                    .get(root)
                    .unwrap()
                    .element
                    .as_any()
                    .downcast_ref::<TextArea>()
                    .unwrap()
                    .tamper_cached_unified_atomic_sources_for_test(case == "atomic_duplicate"),
                "measurement_constraint" | "measurement_size" => arena
                    .get(root)
                    .unwrap()
                    .element
                    .as_any()
                    .downcast_ref::<TextArea>()
                    .unwrap()
                    .tamper_cached_unified_atomic_measurement_for_test(
                        projection_index,
                        case == "measurement_constraint",
                    ),
                "insertion" => arena
                    .get(root)
                    .unwrap()
                    .element
                    .as_any()
                    .downcast_ref::<TextArea>()
                    .unwrap()
                    .tamper_cached_unified_atomic_insertion_for_test(projection_index),
                "orphan_projection" => arena.set_parent(projection, None),
                "dirty_projection" => arena.mark_dirty(projection, DirtyFlags::LAYOUT),
                "extra_projection_child" => {
                    let extra = crate::view::test_support::commit_element(
                        &mut arena,
                        Box::new(crate::view::base_component::Element::new_with_id(
                            0xc3a_1001, 0.0, 0.0, 1.0, 1.0,
                        )),
                    );
                    arena.set_parent(extra, Some(projection));
                    arena.set_children(projection, vec![projection_text, extra]);
                }
                "leaf_geometry" => arena
                    .get_mut(projection_text)
                    .unwrap()
                    .element
                    .as_any_mut()
                    .downcast_mut::<Text>()
                    .unwrap()
                    .tamper_layout_position_for_test(1.0, 0.0),
                "leaf_invisible" => arena
                    .get_mut(projection_text)
                    .unwrap()
                    .element
                    .as_any_mut()
                    .downcast_mut::<Text>()
                    .unwrap()
                    .set_should_render_for_test(false),
                "leaf_unprepared" => arena
                    .get_mut(projection_text)
                    .unwrap()
                    .element
                    .as_any_mut()
                    .downcast_mut::<Text>()
                    .unwrap()
                    .clear_prepared_standalone_text_for_test(),
                _ => unreachable!(),
            }
            let node = arena.get(root).unwrap();
            let text_area = node.element.as_any().downcast_ref::<TextArea>().unwrap();
            assert!(
                text_area
                    .exact_retained_property_scroll_atomic_projection_subtree(
                        root,
                        &arena,
                        [0.0, 0.0],
                    )
                    .is_none(),
                "{case}"
            );
        }
    }

    #[test]
    fn retained_atomic_projection_grammar_rejects_forged_leaf_constraints_and_geometry() {
        let (arena, root, ..) = retained_atomic_projection_fixture();
        let node = arena.get(root).unwrap();
        let text_area = node.element.as_any().downcast_ref::<TextArea>().unwrap();
        let grammar = text_area
            .exact_retained_property_scroll_atomic_projection_subtree(root, &arena, [0.0, 0.0])
            .unwrap();

        let mut tampered = grammar.clone();
        tampered.projection_text_stable_id = tampered.topology[0].stable_id;
        assert!(!tampered.is_canonical());

        let mut tampered = grammar.clone();
        tampered.measurement_constraints.available_height_bits = Some(1.0_f32.to_bits());
        assert!(!tampered.is_canonical());

        let mut tampered = grammar.clone();
        tampered.projection_text_bounds_bits[0] =
            (f32::from_bits(tampered.projection_text_bounds_bits[0]) + 1.0).to_bits();
        assert!(!tampered.is_canonical());

        let mut tampered = grammar.clone();
        tampered.atomic_insertion_byte += 1;
        assert!(!tampered.is_canonical());

        let mut tampered = grammar;
        tampered.flow_offset_bits[0] =
            (f32::from_bits(tampered.flow_offset_bits[0]) + 1.0).to_bits();
        assert!(!tampered.is_canonical());
    }

    #[test]
    fn retained_atomic_projection_private_source_identity_rejects_synchronized_public_tamper() {
        let (arena, root, ..) = retained_atomic_projection_fixture();
        let node = arena.get(root).unwrap();
        let text_area = node.element.as_any().downcast_ref::<TextArea>().unwrap();
        let grammar = text_area
            .exact_retained_property_scroll_atomic_projection_subtree(root, &arena, [0.0, 0.0])
            .unwrap();

        let mut tampered = grammar.clone();
        tampered.atomic_line_index += 1;
        assert!(
            !tampered.is_canonical(),
            "atomic line is frozen source identity"
        );

        let mut tampered = grammar.clone();
        tampered.vertical_align = crate::style::VerticalAlign::Top;
        assert!(
            !tampered.is_canonical(),
            "vertical-align is frozen source identity"
        );

        let mut tampered = grammar.clone();
        tampered.last_unified_apply_bits.0 =
            (f32::from_bits(tampered.last_unified_apply_bits.0) + 1.0).to_bits();
        assert!(
            !tampered.is_canonical(),
            "finite apply-origin drift is frozen source identity"
        );

        let mut tampered = grammar;
        let topology = Arc::make_mut(&mut tampered.topology);
        assert_eq!(
            topology[0].kind,
            super::super::RetainedAtomicProjectionTextAreaTopologyKind::TextRun
        );
        topology[0].kind = super::super::RetainedAtomicProjectionTextAreaTopologyKind::LineBreak;
        assert!(
            !tampered.is_canonical(),
            "nonprojection topology kind is frozen source identity"
        );
    }

    #[test]
    fn retained_atomic_projection_source_oracle_keeps_outside_realized_grammars_legacy() {
        let (arena, root, ..) =
            retained_atomic_projection_fixture_with("projected", 0..9, "projected");
        let node = arena.get(root).unwrap();
        assert!(
            node.element
                .as_any()
                .downcast_ref::<TextArea>()
                .unwrap()
                .exact_retained_property_scroll_atomic_projection_subtree(root, &arena, [0.0, 0.0],)
                .is_none(),
            "whole-content projection without a root glyph is a later grammar"
        );

        let (arena, root, ..) =
            retained_atomic_projection_fixture_with("before projected after", 7..16, "");
        let node = arena.get(root).unwrap();
        assert!(
            node.element
                .as_any()
                .downcast_ref::<TextArea>()
                .unwrap()
                .exact_retained_property_scroll_atomic_projection_subtree(root, &arena, [0.0, 0.0],)
                .is_none(),
            "zero-op projection Text is outside C3a"
        );

        let (arena, root) = projection_fixture(0, true);
        let mut stack = vec![root];
        while let Some(key) = stack.pop() {
            stack.extend(arena.children_of(key));
            arena
                .get_mut(key)
                .unwrap()
                .element
                .clear_local_dirty_flags(DirtyFlags::ALL);
        }
        arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
        arena.refresh_subtree_dirty_cache(root);
        let node = arena.get(root).unwrap();
        assert!(
            node.element
                .as_any()
                .downcast_ref::<TextArea>()
                .unwrap()
                .exact_retained_property_scroll_atomic_projection_subtree(root, &arena, [0.0, 0.0],)
                .is_none(),
            "Element -> Text projection remains Legacy"
        );

        for case in ["zero", "multiple"] {
            let (mut arena, root, ..) = retained_atomic_projection_fixture();
            arena.with_element_taken(root, |element, _| {
                let text_area = element.as_any_mut().downcast_mut::<TextArea>().unwrap();
                text_area.on_render_handler = Some(if case == "zero" {
                    crate::ui::on_text_area_render(|_render| {})
                } else {
                    crate::ui::on_text_area_render(|render| {
                        render.range(0..6, |_text_area| RsxNode::text("before"));
                        render.range(17..22, |_text_area| RsxNode::text("after"));
                    })
                });
                text_area.children_dirty = true;
                text_area.bump_unified_ifc_source_revision();
                text_area.dirty_flags = DirtyFlags::ALL;
            });
            crate::view::test_support::measure_and_place(
                &mut arena,
                root,
                LayoutConstraints {
                    max_width: 132.0,
                    max_height: 240.0,
                    viewport_width: 320.0,
                    viewport_height: 240.0,
                    percent_base_width: Some(320.0),
                    percent_base_height: Some(240.0),
                },
                LayoutPlacement {
                    parent_x: 0.0,
                    parent_y: 0.0,
                    visual_offset_x: 0.0,
                    visual_offset_y: 0.0,
                    available_width: 132.0,
                    available_height: 240.0,
                    viewport_width: 320.0,
                    viewport_height: 240.0,
                    percent_base_width: Some(320.0),
                    percent_base_height: Some(240.0),
                },
            );
            let mut stack = vec![root];
            while let Some(key) = stack.pop() {
                stack.extend(arena.children_of(key));
                arena
                    .get_mut(key)
                    .unwrap()
                    .element
                    .clear_local_dirty_flags(DirtyFlags::ALL);
            }
            arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
            arena.refresh_subtree_dirty_cache(root);
            let node = arena.get(root).unwrap();
            assert!(
                node.element
                    .as_any()
                    .downcast_ref::<TextArea>()
                    .unwrap()
                    .exact_retained_property_scroll_atomic_projection_subtree(
                        root,
                        &arena,
                        [0.0, 0.0],
                    )
                    .is_none(),
                "{case} projection handler"
            );
        }
    }

    fn retained_atomic_projection_scroll_shell() -> (NodeArena, NodeKey, NodeKey, NodeKey) {
        let (mut arena, text_area, ..) = retained_atomic_projection_fixture();
        let outer_scroll_y = 20.0;
        let wrapper = crate::view::test_support::commit_element(
            &mut arena,
            Box::new(Element::new_with_id(
                0xc3a_2001,
                0.0,
                -outer_scroll_y,
                132.0,
                300.0,
            )),
        );
        let root = crate::view::test_support::commit_element(
            &mut arena,
            Box::new(Element::new_with_id(0xc3a_2000, 0.0, 0.0, 132.0, 80.0)),
        );
        arena.set_parent(text_area, Some(wrapper));
        arena.set_children(wrapper, vec![text_area]);
        arena.set_parent(wrapper, Some(root));
        arena.set_children(root, vec![wrapper]);
        arena.with_element_taken(text_area, |element, arena| {
            element.place(
                LayoutPlacement {
                    parent_x: 0.0,
                    parent_y: -outer_scroll_y,
                    visual_offset_x: 0.0,
                    visual_offset_y: 0.0,
                    available_width: 132.0,
                    available_height: 240.0,
                    viewport_width: 320.0,
                    viewport_height: 240.0,
                    percent_base_width: Some(320.0),
                    percent_base_height: Some(240.0),
                },
                arena,
            );
        });
        let mut wrapper_style = Style::new();
        wrapper_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        crate::view::test_support::get_element_mut::<Element>(&arena, wrapper)
            .apply_style(wrapper_style);
        let mut root_style = Style::new();
        root_style.insert(
            PropertyId::ScrollDirection,
            ParsedValue::ScrollDirection(ScrollDirection::Vertical),
        );
        root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        {
            let mut root_element =
                crate::view::test_support::get_element_mut::<Element>(&arena, root);
            root_element.apply_style(root_style);
            root_element.layout_state.content_size = Size {
                width: 132.0,
                height: 300.0,
            };
            root_element.set_scroll_offset((0.0, outer_scroll_y));
            root_element
                .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
        }
        arena
            .get_mut(wrapper)
            .unwrap()
            .element
            .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
        let mut stack = vec![text_area];
        while let Some(key) = stack.pop() {
            stack.extend(arena.children_of(key));
            arena
                .get_mut(key)
                .unwrap()
                .element
                .clear_local_dirty_flags(DirtyFlags::ALL);
        }
        arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
        arena.refresh_subtree_dirty_cache(root);
        (arena, root, wrapper, text_area)
    }

    #[test]
    fn retained_atomic_projection_scroll_admission_is_graph_inert_and_exact() {
        let (arena, root, wrapper, text_area) = retained_atomic_projection_scroll_shell();
        let root_node = arena.get(root).unwrap();
        let root_element = root_node
            .element
            .as_any()
            .downcast_ref::<Element>()
            .unwrap();
        let admission = root_element
            .exact_retained_scroll_atomic_projection_text_area_subtree_admission(root, &arena, 1.0)
            .expect("C3a source shell must admit the exact sibling snapshot");
        assert_eq!(admission.boundary_root, root);
        assert_eq!(admission.content_wrapper, wrapper);
        assert_eq!(admission.text_area_root, text_area);
        assert!(admission.paint_grammar.is_canonical());
        assert!(
            root_element
                .exact_retained_scroll_text_area_subtree_admission(root, &arena, 1.0)
                .is_none(),
            "C1/C2 admission must not inherit C3a semantics"
        );
        assert!(
            root_element
                .exact_retained_scroll_atomic_projection_text_area_subtree_admission(
                    root, &arena, 2.0,
                )
                .is_none(),
            "noncanonical scale remains outside the sibling shell"
        );
    }

    #[test]
    fn unified_ifc_staging_applies_vertical_align_to_paint_local_pos() {
        let mut text_area = TextArea::new();
        text_area.content = "abXYZcd".to_string();
        text_area.font_size = 14.0;
        text_area.line_height = 1.25;
        text_area.vertical_align = crate::style::VerticalAlign::Bottom;
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

        arena
            .with_element_taken_ref(root, |el, arena| {
                let text_area = el
                    .as_any()
                    .downcast_ref::<TextArea>()
                    .expect("TextArea root");
                let package = text_area
                    .unified_inline_ifc_render_package(arena)
                    .expect("unified render package");
                let origin = [7.0_f32, 11.0_f32];
                let paint_input = package.ifc.text_pass_paint_input();
                let staging = package.text_pass_staging_input(origin, 1.0, 0, 1.0);
                assert!(!staging.glyphs.is_empty(), "fixture should stage glyphs");
                let mut saw_vertical_shift = false;
                for (staged, raw) in staging.glyphs.iter().zip(paint_input.glyphs.iter()) {
                    let raw_local_y = raw.baseline_y + raw.glyph_y;
                    if (staged.paint.local_pos[1] - raw_local_y).abs() > 0.5 {
                        saw_vertical_shift = true;
                    }
                    assert!(
                        (staged.final_paint_pos[0] - (origin[0] + staged.paint.local_pos[0]))
                            .abs()
                            < 1e-3
                            && (staged.final_paint_pos[1]
                                - (origin[1] + staged.paint.local_pos[1]))
                                .abs()
                                < 1e-3,
                        "prepared pass consumes paint.local_pos; it must stay in sync with final_paint_pos",
                    );
                }
                assert!(
                    saw_vertical_shift,
                    "fixture should exercise a nonzero vertical-align delta"
                );
            })
            .expect("root exists");
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
        ctx.replace_scissor_rect(Some([5, 6, 70, 30]));
        let ctx_for_build = UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone());
        let next_state = arena
            .with_element_taken(root, |el, a| el.build(&mut graph, a, ctx_for_build))
            .expect("TextArea build returns state");
        ctx.set_state(next_state);

        assert_eq!(
            ctx.graphics_pass_context().scissor_rect,
            Some([5, 6, 70, 30]),
            "TextArea viewport scissor must restore the exact ancestor authority",
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

    #[test]
    fn text_area_inline_ifc_plain_unified_render_skips_per_run_text_passes() {
        let (mut arena, root) = wrapped_plain_fixture("plain root render", 240.0);

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
            "plain TextArea should render glyphs once from the TextArea-level unified IFC package, got {pass_names:?}"
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

    fn consumed_soft_wrap_slots(arena: &NodeArena, root: NodeKey) -> (usize, usize) {
        arena
            .with_element_taken_ref(root, |el, arena| {
                let text_area = el
                    .as_any()
                    .downcast_ref::<TextArea>()
                    .expect("TextArea root");
                let map = super::super::caret_map::CaretNavigationMap::build(text_area, arena);
                map.lines.windows(2).find_map(|pair| {
                    let upper = pair[0].stops.last()?;
                    let lower = pair[1].stops.first()?;
                    let consumed = text_area
                        .content
                        .chars()
                        .skip(upper.char_index)
                        .take(lower.char_index.saturating_sub(upper.char_index));
                    let consumed = consumed.collect::<String>();
                    (!consumed.is_empty() && consumed.chars().all(char::is_whitespace))
                        .then_some((upper.char_index, lower.char_index))
                })
            })
            .expect("root exists")
            .expect("fixture should contain a soft wrap that consumes whitespace")
    }

    fn run_text_pass_fragments(arena: &NodeArena, root: NodeKey) -> Vec<(String, Rect)> {
        arena
            .with_element_taken_ref(root, |el, arena| {
                let text_area = el
                    .as_any()
                    .downcast_ref::<TextArea>()
                    .expect("TextArea root");
                let Some(package) = text_area.unified_inline_ifc_render_package(arena) else {
                    return Vec::new();
                };
                let origin = [
                    text_area.layout_state.layout_position.x - text_area.scroll_x,
                    text_area.layout_state.layout_position.y - text_area.scroll_y,
                ];
                let paint_input = package.ifc.text_pass_paint_input();
                let staging_input = package.text_pass_staging_input(origin, 1.0, 0, 1.0);
                let backing = package.ifc.backing_text().to_string();
                let text_sources = package
                    .source_segments
                    .iter()
                    .filter(|segment| {
                        segment.kind
                            == super::super::inline_ifc::TextAreaUnifiedIfcSourceKind::TextRun
                    })
                    .map(|segment| segment.source)
                    .collect::<Vec<_>>();

                let mut out = Vec::new();
                for line in &paint_input.lines {
                    for source in &text_sources {
                        let mut left: Option<f32> = None;
                        let mut right: Option<f32> = None;
                        let mut start: Option<usize> = None;
                        let mut end: Option<usize> = None;
                        for (glyph, staged) in
                            paint_input.glyphs.iter().zip(staging_input.glyphs.iter())
                        {
                            if glyph.line_index != line.line_index || glyph.source != *source {
                                continue;
                            }
                            let x = staged.final_paint_pos[0];
                            left = Some(left.map_or(x, |current| current.min(x)));
                            right = Some(right.map_or(x + glyph.advance, |current| {
                                current.max(x + glyph.advance)
                            }));
                            start = Some(start.map_or(glyph.cluster_range.start, |current| {
                                current.min(glyph.cluster_range.start)
                            }));
                            end = Some(end.map_or(glyph.cluster_range.end, |current| {
                                current.max(glyph.cluster_range.end)
                            }));
                        }
                        let (Some(left), Some(right), Some(start), Some(end)) =
                            (left, right, start, end)
                        else {
                            continue;
                        };
                        if start >= end || right <= left {
                            continue;
                        }
                        out.push((
                            backing[start..end].to_string(),
                            Rect {
                                x: left,
                                y: origin[1] + line.y,
                                width: (right - left).max(0.0),
                                height: line.height.max(1.0),
                            },
                        ));
                    }
                }
                out
            })
            .expect("root exists")
    }

    #[test]
    fn projection_fallback_caret_start_uses_projection_left_edge() {
        let (arena, root) = projection_fixture(2, false);
        let snap = projection_snapshot(&arena, root);
        let (x, y, height) = caret_position(&arena, root);

        assert!((x - snap.x).abs() < 0.5, "x={x}, snap.x={}", snap.x);
        assert!((height - snap.height).abs() < 0.01, "height={height}");
        assert!((y - snap.y).abs() < 0.5, "y={y}, snap.y={}", snap.y);
    }

    #[test]
    fn projection_fallback_caret_interpolates_inside_projection() {
        let (arena, root) = projection_fixture(3, false);
        let snap = projection_snapshot(&arena, root);
        let (x, _, height) = caret_position(&arena, root);
        let expected_x = snap.x + snap.width / 3.0;

        assert!((x - expected_x).abs() < 0.5, "x={x}, expected={expected_x}");
        assert!((height - snap.height).abs() < 0.01, "height={height}");
    }

    #[test]
    fn projection_caret_prefers_inner_text_glyphs_when_descendant_exists() {
        // Interior caret positions align with the chip's rendered text
        // (real glyph coordinates), not the root box's char-fraction —
        // the fraction drifts off the visible characters (e.g. a caret
        // floating over the wrong letter inside {{USER_ID}}).
        let (arena, root) = projection_fixture(4, true);
        let snap = projection_snapshot(&arena, root);
        let (x, y, height) = caret_position(&arena, root);
        let expected = arena
            .with_element_taken_ref(root, |el, arena| {
                let text_area = el
                    .as_any()
                    .downcast_ref::<TextArea>()
                    .expect("TextArea root");
                let key = text_area.children[1];
                glyph_caret_in_projection(arena, key, 2, text_area.cursor_affinity)
            })
            .flatten()
            .expect("inner glyph caret");

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
            x > snap.x && x < snap.x + snap.width,
            "caret must sit inside the chip: x={x}, chip=({}, {})",
            snap.x,
            snap.x + snap.width
        );
        assert!(
            (height - expected.2).abs() < 0.01,
            "height={height}, expected={}",
            expected.2
        );
        let projection_snap = projection_snapshot(&arena, root);
        assert!(
            x >= projection_snap.x - 0.5 && x <= projection_snap.x + projection_snap.width + 0.5,
            "caret x should be inside projection bounds: x={x}, projection=({}, {})",
            projection_snap.x,
            projection_snap.width
        );
        assert!(
            y >= projection_snap.y - 0.5 && y <= projection_snap.y + projection_snap.height + 0.5,
            "caret y should be inside projection bounds: y={y}, projection=({}, {})",
            projection_snap.y,
            projection_snap.height
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

    /// TextArea caret geometry now comes from the unified root IFC. A
    /// projected badge contributes root-level atomic caret stops, so
    /// affinity is resolved by the root line boundary rather than by
    /// probing the projection's descendant Text.
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

        let probe_chars = [usr_start, (usr_start + usr_end) / 2, usr_end];
        arena
            .with_element_taken_ref(root, |el, arena| {
                let ta = el.as_any().downcast_ref::<TextArea>().unwrap();
                let map = super::super::caret_map::CaretNavigationMap::build(ta, arena);
                for char_index in probe_chars {
                    assert!(
                        map.caret_stop_for_char(char_index, CaretAffinity::Downstream)
                            .is_some(),
                        "root map should expose projection atomic caret stop for char {char_index}"
                    );
                }
            })
            .expect("TextArea root");

        // Interior carets render from the badge's inner text glyphs;
        // the badge bounds still contain them and affinity must not
        // produce a caret outside the badge.
        let badge_snap = arena
            .with_element_taken_ref(root, |el, arena| {
                let ta = el.as_any().downcast_ref::<TextArea>().unwrap();
                let key = ta
                    .children
                    .iter()
                    .copied()
                    .filter(|&key| {
                        arena
                            .with_element_taken_ref(key, |child, _| {
                                !child.as_any().is::<TextAreaTextRun>()
                                    && !child.as_any().is::<TextAreaLineBreak>()
                            })
                            .unwrap_or(false)
                    })
                    .nth(1)
                    .expect("user badge child");
                arena.with_element_taken_ref(key, |child, _| child.box_model_snapshot())
            })
            .flatten()
            .expect("badge snapshot");
        for affinity in [CaretAffinity::Upstream, CaretAffinity::Downstream] {
            let cursor = (usr_start + usr_end) / 2;
            arena.with_element_taken(root, |el, _| {
                let ta = el
                    .as_any_mut()
                    .downcast_mut::<TextArea>()
                    .expect("TextArea root");
                ta.cursor_char = cursor;
                ta.cursor_affinity = affinity;
            });
            let (cx, cy, height) = caret_position(&arena, root);
            assert!(
                cx >= badge_snap.x - 0.5
                    && cx <= badge_snap.x + badge_snap.width + 0.5
                    && cy >= badge_snap.y - 0.5
                    && cy + height <= badge_snap.y + badge_snap.height + 0.5,
                "interior caret must stay inside the wrapped badge for {affinity:?}: caret=({cx},{cy},{height}) badge=({},{},{},{})",
                badge_snap.x,
                badge_snap.y,
                badge_snap.width,
                badge_snap.height,
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

        let proj_key = projection_key(&arena, root);
        let projection_rect = snapshot(&arena, proj_key);
        let cursor = 2 + "/activity/with/a/very/long/path".chars().count() / 2;
        let mut positions = Vec::new();
        for affinity in [CaretAffinity::Upstream, CaretAffinity::Downstream] {
            arena.with_element_taken(root, |el, _| {
                let ta = el
                    .as_any_mut()
                    .downcast_mut::<TextArea>()
                    .expect("TextArea root");
                ta.cursor_char = cursor;
                ta.cursor_affinity = affinity;
            });
            positions.push((affinity, caret_position(&arena, root)));
        }

        for (affinity, (x, y, height)) in positions {
            assert!(
                projection_rect.x <= x && x <= projection_rect.x + projection_rect.width,
                "{affinity:?} root caret x should stay inside projection atomic box: x={x}, rect={projection_rect:?}"
            );
            assert!(
                projection_rect.y <= y && y <= projection_rect.y + projection_rect.height,
                "{affinity:?} root caret y should stay inside projection atomic box: y={y}, rect={projection_rect:?}"
            );
            assert!(
                height > 0.0,
                "{affinity:?} root caret should expose visible height"
            );
        }
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
        let (upper_tail, lower_head) = consumed_soft_wrap_slots(&base_arena, base_root);
        let cursor = upper_tail;
        let (upper_y, lower_y) = base_arena
            .with_element_taken_ref(base_root, |el, arena| {
                let text_area = el
                    .as_any()
                    .downcast_ref::<TextArea>()
                    .expect("TextArea root");
                let map = super::super::caret_map::CaretNavigationMap::build(text_area, arena);
                let upper = map
                    .caret_stop_for_char(upper_tail, CaretAffinity::Upstream)
                    .expect("upstream upper-tail stop");
                let lower = map
                    .caret_stop_for_char(lower_head, CaretAffinity::Downstream)
                    .expect("downstream lower-head stop");
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
        let def_y = fragments
            .iter()
            .find_map(|(content, rect)| content.contains("def").then_some(rect.y))
            .expect("def fragment");
        // The CJK preedit run has a taller ascent than the latin "abc"
        // run, so sharing a baseline legitimately separates the two
        // fragment tops by a few pixels. Assert line membership against
        // the next line's midpoint instead of per-font fragment tops.
        let line_midpoint = (abc_y + def_y) * 0.5;
        assert!(
            preedit_y < line_midpoint,
            "hard-newline tail preedit should stay before newline when space allows: fragments={fragments:?}"
        );
        let (_, caret_y, _) = caret_position(&arena, root);
        assert!(
            caret_y < line_midpoint,
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
        // Tight enough that the first preedit glyph cannot fit on the
        // prefix line even with the wrap epsilon slack.
        let width = 66.0;
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
        let (upper_tail, _) = consumed_soft_wrap_slots(&base_arena, base_root);
        let preedit = "\u{4E2D}".repeat(12);
        let (arena, root) = plain_preedit_fixture_with_options(
            content,
            upper_tail,
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

    #[test]
    fn projection_selection_underlay_renders_for_ifc_owned_inner_text() {
        use crate::view::base_component::UiBuildContext;
        use crate::view::frame_graph::FrameGraph;

        // Select-all must paint selection rects inside the chip: the inner
        // Text is IFC-owned (glyphs come from the chip root's unified
        // pass), but the selection underlay still belongs to the Text.
        let build_pass_names = |selected: bool| -> Vec<&'static str> {
            let (mut arena, root) = projection_fixture(0, true);
            arena.with_element_taken(root, |el, _| {
                let ta = el
                    .as_any_mut()
                    .downcast_mut::<TextArea>()
                    .expect("TextArea root");
                if selected {
                    // Select exactly the projection range (2..5): the
                    // committed-text selection layer contributes nothing
                    // here, so any added rect is the chip's underlay.
                    ta.selection_anchor_char = Some(2);
                    ta.selection_focus_char = Some(5);
                }
            });
            let mut graph = FrameGraph::new();
            let mut ctx = UiBuildContext::new(400, 200, wgpu::TextureFormat::Bgra8Unorm, 1.0);
            let target = ctx.allocate_target(&mut graph);
            ctx.set_current_target(target);
            let ctx_for_build = UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone());
            arena
                .with_element_taken(root, |el, a| el.build(&mut graph, a, ctx_for_build))
                .expect("build result");
            graph
                .pass_descriptors()
                .into_iter()
                .map(|descriptor| descriptor.name)
                .collect()
        };

        let unselected_rects = build_pass_names(false)
            .iter()
            .filter(|name| name.contains("DrawRectPass"))
            .count();
        let selected_rects = build_pass_names(true)
            .iter()
            .filter(|name| name.contains("DrawRectPass"))
            .count();
        assert!(
            selected_rects > unselected_rects,
            "selecting across a projection must add selection underlay rects inside the chip: unselected={unselected_rects} selected={selected_rects}"
        );
    }
}
