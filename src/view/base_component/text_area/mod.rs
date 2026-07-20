//! TextArea v2 — inline formatting context container.
//!
//! P1 skeleton. Real impls land in P2 (layout/render), P3 (edit), P4 (IME),
//! P5 (projection), P6 (reconcile diff), P7 (migration). See
//! `docs/design/textarea-v2.md` for the design and phase plan.
//!
//! v2 lives under tag `<TextArea>` while v1 (`<TextArea>`) is unchanged.
//! Rename + v1 removal happens in P7.

#![allow(dead_code)] // P1: stubs; fields wired in later phases.

mod caret_map;
pub(crate) use caret_map::CaretAffinity;

/// Test probe: the caret stop the navigation map resolves for `char_index`
/// with the TextArea's current affinity.
#[cfg(test)]
pub(crate) fn caret_map_probe(
    text_area: &TextArea,
    arena: &crate::view::node_arena::NodeArena,
    char_index: usize,
) -> Option<(usize, f32, f32)> {
    let map = caret_map::CaretNavigationMap::build(text_area, arena);
    map.caret_stop_for_char(char_index, text_area.cursor_affinity)
        .map(|stop| (stop.char_index, stop.x, stop.y_top))
}

/// Test probe for comparing both retained soft-wrap affinity branches without
/// exposing the private affinity enum outside the TextArea module.
#[cfg(test)]
pub(crate) fn caret_map_probe_with_affinity(
    text_area: &TextArea,
    arena: &crate::view::node_arena::NodeArena,
    char_index: usize,
    upstream: bool,
) -> Option<(usize, f32, f32)> {
    let map = caret_map::CaretNavigationMap::build(text_area, arena);
    let affinity = if upstream {
        caret_map::CaretAffinity::Upstream
    } else {
        caret_map::CaretAffinity::Downstream
    };
    map.caret_stop_for_char(char_index, affinity)
        .map(|stop| (stop.char_index, stop.x, stop.y_top))
}

#[cfg(test)]
pub(crate) fn set_caret_affinity_probe(text_area: &mut TextArea, upstream: bool) {
    text_area.cursor_affinity = if upstream {
        caret_map::CaretAffinity::Upstream
    } else {
        caret_map::CaretAffinity::Downstream
    };
}
mod edit;
mod events;
mod hit_test;
mod ime_context;
mod inline_ifc;
mod layout;
mod projection;
mod reconcile;
mod render;
mod render_string;
mod run;
mod segment;
mod state;
mod style;

pub use ime_context::TextAreaImeContext;
pub use render_string::{TextAreaRenderProjection, TextAreaRenderString};
#[allow(unused_imports)] // re-exported for P2+; not yet referenced outside the module.
pub(crate) use run::{TextAreaLineBreak, TextAreaRunStyle, TextAreaTextRun};
#[allow(unused_imports)] // P8 M1+: emitted by TextArea schema render.
pub(crate) use segment::TextAreaProjectionSegment;

use slotmap::Key;
use std::ops::Range;
use std::sync::Arc;

use crate::style::Cursor;
use crate::ui::{
    Binding, BlurHandlerProp, Rect, TextAreaFocusHandlerProp, TextAreaRenderHandlerProp,
    TextChangeHandlerProp,
};
use crate::view::base_component::{BoxModelSnapshot, DirtyFlags, ElementTrait, LayoutConstraints};
use crate::view::layout::{FlexLayoutInfo, LayoutState};
use crate::view::node_arena::{NodeArena, NodeKey};

use super::{next_ui_node_id, round_layout_value};

/// TextArea v2 — see `docs/design/textarea-v2.md`.
///
/// Decision A1: NOT-IS-A Element. Box model lives in a wrapping `<Element>`.
/// Decision A3: children are mixed `TextAreaTextRun` + projection RsxNodes,
/// all real arena children laid out via `view/layout/*` Inline pipeline.
/// Decision A9: char index is the single source of truth; children carry no
/// cursor/selection/IME state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RetainedTextAreaPaintGrammar {
    GlyphOnly,
    SelectionGlyphs {
        start_char: usize,
        end_char: usize,
        color_rgba_bits: [u32; 4],
    },
}

impl RetainedTextAreaPaintGrammar {
    pub(crate) fn is_canonical(self) -> bool {
        match self {
            Self::GlyphOnly => true,
            Self::SelectionGlyphs {
                start_char,
                end_char,
                color_rgba_bits,
            } => {
                start_char < end_char
                    && color_rgba_bits
                        .map(f32::from_bits)
                        .into_iter()
                        .all(|channel| channel.is_finite() && (0.0..=1.0).contains(&channel))
            }
        }
    }
}

/// Closed resident-base grammar for focused plain TextArea retention.
/// Caret visibility deliberately lives in a separate dynamic overlay seal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RetainedInteractiveTextAreaPaintGrammar {
    FocusedGlyphs,
    FocusedSelectionGlyphs {
        start_char: usize,
        end_char: usize,
        color_rgba_bits: [u32; 4],
    },
    FocusedPreeditGlyphs,
}

impl RetainedInteractiveTextAreaPaintGrammar {
    pub(crate) fn is_canonical(self) -> bool {
        match self {
            Self::FocusedGlyphs | Self::FocusedPreeditGlyphs => true,
            Self::FocusedSelectionGlyphs {
                start_char,
                end_char,
                color_rgba_bits,
            } => {
                start_char < end_char
                    && color_rgba_bits
                        .map(f32::from_bits)
                        .into_iter()
                        .all(|channel| channel.is_finite() && (0.0..=1.0).contains(&channel))
            }
        }
    }

    pub(crate) fn has_preedit(self) -> bool {
        matches!(self, Self::FocusedPreeditGlyphs)
    }
}

/// Closed C3a source grammar for one realized atomic projection.
///
/// This freezes only engine-owned, post-layout facts.  The `on_render`
/// callback is deliberately absent: admission must never execute or trust a
/// user `FnMut` during planning.  Recorder/compiler authority is a later
/// migration segment and must independently seal its artifact.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RetainedAtomicProjectionTextAreaTopologyKind {
    TextRun,
    LineBreak,
    ProjectionSegment,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RetainedAtomicProjectionTextAreaTopologySeal {
    pub(crate) topology_index: usize,
    pub(crate) owner: NodeKey,
    pub(crate) stable_id: u64,
    pub(crate) source_id: u64,
    pub(crate) kind: RetainedAtomicProjectionTextAreaTopologyKind,
    pub(crate) start_char: usize,
    pub(crate) end_char: usize,
    pub(crate) backing_start_byte: usize,
    pub(crate) backing_end_byte: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RetainedAtomicProjectionIntrinsicSizeSeal {
    pub(crate) min_content_width_bits: u32,
    pub(crate) max_content_width_bits: u32,
    pub(crate) preferred_width_bits: Option<u32>,
    pub(crate) preferred_height_bits: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RetainedAtomicProjectionMeasureConstraintsSeal {
    pub(crate) max_width_bits: Option<u32>,
    pub(crate) available_height_bits: Option<u32>,
    pub(crate) viewport_bits: Option<[u32; 2]>,
    pub(crate) percent_base_width_bits: Option<u32>,
    pub(crate) percent_base_height_bits: Option<u32>,
    pub(crate) min_width_bits: Option<u32>,
    pub(crate) max_sizing_width_bits: Option<u32>,
    pub(crate) min_height_bits: Option<u32>,
    pub(crate) max_height_bits: Option<u32>,
    pub(crate) intrinsic_size: Option<RetainedAtomicProjectionIntrinsicSizeSeal>,
}

/// Unforgeable TextArea-module proof for the complete C3a source snapshot.
/// Sibling paint/element modules can read the public grammar witnesses but
/// cannot construct or update this identity when attempting to mutate them.
#[derive(Clone, Debug, PartialEq, Eq)]
struct RetainedAtomicProjectionTextAreaFrozenSourceIdentity {
    projection_index: usize,
    projection_owner: NodeKey,
    projection_stable_id: u64,
    projection_text_owner: NodeKey,
    projection_text_stable_id: u64,
    projection_start_char: usize,
    projection_end_char: usize,
    projection_backing_start_byte: usize,
    projection_backing_end_byte: usize,
    atomic_source_id: u64,
    atomic_id: u64,
    atomic_insertion_byte: usize,
    atomic_line_index: usize,
    measurement_constraints: RetainedAtomicProjectionMeasureConstraintsSeal,
    measured_size_bits: [u32; 2],
    placement_rect_bits: [u32; 4],
    projection_segment_bounds_bits: [u32; 4],
    projection_text_bounds_bits: [u32; 4],
    flow_offset_bits: [u32; 2],
    owner_inline_baseline_bits: u32,
    inline_full_available_width_bits: u32,
    auto_wrap: bool,
    vertical_align: crate::style::VerticalAlign,
    unified_ifc_source_revision: u64,
    last_unified_apply_bits: (u32, u32, u64),
    topology: Arc<[RetainedAtomicProjectionTextAreaTopologySeal]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RetainedAtomicProjectionTextAreaPaintGrammar {
    pub(crate) projection_index: usize,
    pub(crate) projection_owner: NodeKey,
    pub(crate) projection_stable_id: u64,
    pub(crate) projection_text_owner: NodeKey,
    pub(crate) projection_text_stable_id: u64,
    pub(crate) projection_start_char: usize,
    pub(crate) projection_end_char: usize,
    pub(crate) projection_backing_start_byte: usize,
    pub(crate) projection_backing_end_byte: usize,
    pub(crate) atomic_source_id: u64,
    pub(crate) atomic_id: u64,
    pub(crate) atomic_insertion_byte: usize,
    pub(crate) atomic_line_index: usize,
    pub(crate) measurement_constraints: RetainedAtomicProjectionMeasureConstraintsSeal,
    pub(crate) measured_size_bits: [u32; 2],
    pub(crate) placement_rect_bits: [u32; 4],
    pub(crate) projection_segment_bounds_bits: [u32; 4],
    pub(crate) projection_text_bounds_bits: [u32; 4],
    pub(crate) flow_offset_bits: [u32; 2],
    pub(crate) owner_inline_baseline_bits: u32,
    pub(crate) inline_full_available_width_bits: u32,
    pub(crate) auto_wrap: bool,
    pub(crate) vertical_align: crate::style::VerticalAlign,
    pub(crate) unified_ifc_source_revision: u64,
    pub(crate) last_unified_apply_bits: (u32, u32, u64),
    pub(crate) topology: Arc<[RetainedAtomicProjectionTextAreaTopologySeal]>,
    frozen_source_identity: RetainedAtomicProjectionTextAreaFrozenSourceIdentity,
}

impl RetainedAtomicProjectionTextAreaPaintGrammar {
    fn from_frozen_source_identity(
        frozen: RetainedAtomicProjectionTextAreaFrozenSourceIdentity,
    ) -> Option<Self> {
        let grammar = Self {
            projection_index: frozen.projection_index,
            projection_owner: frozen.projection_owner,
            projection_stable_id: frozen.projection_stable_id,
            projection_text_owner: frozen.projection_text_owner,
            projection_text_stable_id: frozen.projection_text_stable_id,
            projection_start_char: frozen.projection_start_char,
            projection_end_char: frozen.projection_end_char,
            projection_backing_start_byte: frozen.projection_backing_start_byte,
            projection_backing_end_byte: frozen.projection_backing_end_byte,
            atomic_source_id: frozen.atomic_source_id,
            atomic_id: frozen.atomic_id,
            atomic_insertion_byte: frozen.atomic_insertion_byte,
            atomic_line_index: frozen.atomic_line_index,
            measurement_constraints: frozen.measurement_constraints,
            measured_size_bits: frozen.measured_size_bits,
            placement_rect_bits: frozen.placement_rect_bits,
            projection_segment_bounds_bits: frozen.projection_segment_bounds_bits,
            projection_text_bounds_bits: frozen.projection_text_bounds_bits,
            flow_offset_bits: frozen.flow_offset_bits,
            owner_inline_baseline_bits: frozen.owner_inline_baseline_bits,
            inline_full_available_width_bits: frozen.inline_full_available_width_bits,
            auto_wrap: frozen.auto_wrap,
            vertical_align: frozen.vertical_align,
            unified_ifc_source_revision: frozen.unified_ifc_source_revision,
            last_unified_apply_bits: frozen.last_unified_apply_bits,
            topology: Arc::clone(&frozen.topology),
            frozen_source_identity: frozen,
        };
        grammar.is_canonical().then_some(grammar)
    }

    pub(crate) fn is_canonical(&self) -> bool {
        let finite = |bits: u32| f32::from_bits(bits).is_finite();
        let non_negative = |bits: u32| finite(bits) && f32::from_bits(bits) >= 0.0;
        if self.projection_start_char >= self.projection_end_char
            || self.projection_backing_start_byte != self.projection_backing_end_byte
            || self.projection_owner == self.projection_text_owner
            || self.projection_owner.is_null()
            || self.projection_text_owner.is_null()
            || self.projection_stable_id == 0
            || self.projection_text_stable_id == 0
            || self.atomic_source_id == 0
            || self.atomic_insertion_byte != self.projection_backing_start_byte
            || self.unified_ifc_source_revision == 0
            || self.last_unified_apply_bits.2 != self.unified_ifc_source_revision
            || !finite(self.last_unified_apply_bits.0)
            || !finite(self.last_unified_apply_bits.1)
            || self
                .measurement_constraints
                .max_width_bits
                .is_some_and(|bits| !finite(bits) || f32::from_bits(bits) <= 0.0)
            || self.measurement_constraints.available_height_bits.is_some()
            || self.measurement_constraints.viewport_bits.is_some()
            || self
                .measurement_constraints
                .percent_base_width_bits
                .is_some()
            || self
                .measurement_constraints
                .percent_base_height_bits
                .is_some()
            || self.measurement_constraints.min_width_bits.is_some()
            || self.measurement_constraints.max_sizing_width_bits.is_some()
            || self.measurement_constraints.min_height_bits.is_some()
            || self.measurement_constraints.max_height_bits.is_some()
            || self.measurement_constraints.intrinsic_size.is_some()
            || self.auto_wrap != self.measurement_constraints.max_width_bits.is_some()
            || !self.measured_size_bits.into_iter().all(non_negative)
            || !self.placement_rect_bits.into_iter().all(finite)
            || !self.placement_rect_bits[2..]
                .iter()
                .copied()
                .all(non_negative)
            || !self.flow_offset_bits.into_iter().all(finite)
            || !self.projection_segment_bounds_bits.into_iter().all(finite)
            || !self.projection_text_bounds_bits.into_iter().all(finite)
            || !self.projection_segment_bounds_bits[2..]
                .iter()
                .copied()
                .all(non_negative)
            || !self.projection_text_bounds_bits[2..]
                .iter()
                .copied()
                .all(|bits| non_negative(bits) && f32::from_bits(bits) > 0.0)
            || self.projection_segment_bounds_bits != self.projection_text_bounds_bits
            || self.flow_offset_bits != [self.placement_rect_bits[0], self.placement_rect_bits[1]]
            || !non_negative(self.owner_inline_baseline_bits)
            || !non_negative(self.inline_full_available_width_bits)
            || self.measured_size_bits != [self.placement_rect_bits[2], self.placement_rect_bits[3]]
            || f32::from_bits(self.inline_full_available_width_bits)
                < f32::from_bits(self.measured_size_bits[0])
            || self.projection_index >= self.topology.len()
        {
            return false;
        }

        let mut owners = std::collections::HashSet::with_capacity(self.topology.len());
        let mut stable_ids = std::collections::HashSet::with_capacity(self.topology.len());
        let mut sources = std::collections::HashSet::with_capacity(self.topology.len());
        let mut char_cursor = 0usize;
        let mut backing_cursor = 0usize;
        let mut projection_count = 0usize;
        for (index, node) in self.topology.iter().enumerate() {
            if node.topology_index != index
                || node.start_char != char_cursor
                || node.end_char < node.start_char
                || node.backing_start_byte != backing_cursor
                || node.backing_end_byte < node.backing_start_byte
                || node.owner.is_null()
                || node.stable_id == 0
                || !owners.insert(node.owner)
                || !stable_ids.insert(node.stable_id)
                || !sources.insert(node.source_id)
            {
                return false;
            }
            match node.kind {
                RetainedAtomicProjectionTextAreaTopologyKind::ProjectionSegment => {
                    projection_count += 1;
                    if index != self.projection_index
                        || node.owner != self.projection_owner
                        || node.stable_id != self.projection_stable_id
                        || node.source_id != self.atomic_source_id
                        || node.start_char != self.projection_start_char
                        || node.end_char != self.projection_end_char
                        || node.backing_start_byte != self.projection_backing_start_byte
                        || node.backing_end_byte != self.projection_backing_end_byte
                    {
                        return false;
                    }
                }
                RetainedAtomicProjectionTextAreaTopologyKind::TextRun
                | RetainedAtomicProjectionTextAreaTopologyKind::LineBreak => {}
            }
            char_cursor = node.end_char;
            backing_cursor = node.backing_end_byte;
        }
        projection_count == 1
            && !owners.contains(&self.projection_text_owner)
            && !stable_ids.contains(&self.projection_text_stable_id)
            && self.frozen_source_identity
                == RetainedAtomicProjectionTextAreaFrozenSourceIdentity {
                    projection_index: self.projection_index,
                    projection_owner: self.projection_owner,
                    projection_stable_id: self.projection_stable_id,
                    projection_text_owner: self.projection_text_owner,
                    projection_text_stable_id: self.projection_text_stable_id,
                    projection_start_char: self.projection_start_char,
                    projection_end_char: self.projection_end_char,
                    projection_backing_start_byte: self.projection_backing_start_byte,
                    projection_backing_end_byte: self.projection_backing_end_byte,
                    atomic_source_id: self.atomic_source_id,
                    atomic_id: self.atomic_id,
                    atomic_insertion_byte: self.atomic_insertion_byte,
                    atomic_line_index: self.atomic_line_index,
                    measurement_constraints: self.measurement_constraints,
                    measured_size_bits: self.measured_size_bits,
                    placement_rect_bits: self.placement_rect_bits,
                    projection_segment_bounds_bits: self.projection_segment_bounds_bits,
                    projection_text_bounds_bits: self.projection_text_bounds_bits,
                    flow_offset_bits: self.flow_offset_bits,
                    owner_inline_baseline_bits: self.owner_inline_baseline_bits,
                    inline_full_available_width_bits: self.inline_full_available_width_bits,
                    auto_wrap: self.auto_wrap,
                    vertical_align: self.vertical_align,
                    unified_ifc_source_revision: self.unified_ifc_source_revision,
                    last_unified_apply_bits: self.last_unified_apply_bits,
                    topology: Arc::clone(&self.topology),
                }
    }
}

/// Exact focused-caret paint sealed at the atomic-projection source boundary.
/// Clipping is deliberately not represented here: a present caret remains a
/// source fact even when a later compositor phase culls it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FocusedAtomicCaretSourcePaintSeal {
    Hidden,
    Present {
        bounds_bits: [u32; 4],
        payload_identity: crate::view::paint::PaintPayloadIdentity,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FocusedAtomicCaretFrozenSourceIdentity {
    owner: NodeKey,
    stable_id: u64,
    focused: bool,
    should_render: bool,
    caret_visible: bool,
    foreground_color_bits: [u32; 4],
    cursor_char: usize,
    cursor_affinity: CaretAffinity,
    ime_preedit_cursor: Option<(usize, usize)>,
    local_scroll_bits: [u32; 2],
    unified_ifc_source_revision: u64,
    last_unified_apply_bits: Option<(u32, u32, u64)>,
    paint: FocusedAtomicCaretSourcePaintSeal,
}

/// Component-owned focused caret proof.  It is independent of the older
/// interactive TextArea oracle so projection topology never inherits that
/// oracle's generated-run assumptions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FocusedAtomicCaretSourceSeal {
    pub(crate) owner: NodeKey,
    pub(crate) stable_id: u64,
    pub(crate) focused: bool,
    pub(crate) should_render: bool,
    pub(crate) caret_visible: bool,
    pub(crate) foreground_color_bits: [u32; 4],
    pub(crate) cursor_char: usize,
    pub(crate) cursor_affinity: CaretAffinity,
    pub(crate) ime_preedit_cursor: Option<(usize, usize)>,
    pub(crate) local_scroll_bits: [u32; 2],
    pub(crate) unified_ifc_source_revision: u64,
    pub(crate) last_unified_apply_bits: Option<(u32, u32, u64)>,
    pub(crate) paint: FocusedAtomicCaretSourcePaintSeal,
    frozen_source_identity: FocusedAtomicCaretFrozenSourceIdentity,
}

impl FocusedAtomicCaretSourceSeal {
    fn from_frozen_source_identity(frozen: FocusedAtomicCaretFrozenSourceIdentity) -> Option<Self> {
        let seal = Self {
            owner: frozen.owner,
            stable_id: frozen.stable_id,
            focused: frozen.focused,
            should_render: frozen.should_render,
            caret_visible: frozen.caret_visible,
            foreground_color_bits: frozen.foreground_color_bits,
            cursor_char: frozen.cursor_char,
            cursor_affinity: frozen.cursor_affinity,
            ime_preedit_cursor: frozen.ime_preedit_cursor,
            local_scroll_bits: frozen.local_scroll_bits,
            unified_ifc_source_revision: frozen.unified_ifc_source_revision,
            last_unified_apply_bits: frozen.last_unified_apply_bits,
            paint: frozen.paint.clone(),
            frozen_source_identity: frozen,
        };
        seal.is_canonical().then_some(seal)
    }

    pub(crate) fn is_canonical(&self) -> bool {
        let last_apply_is_current = self
            .last_unified_apply_bits
            .is_some_and(|(x, y, revision)| {
                f32::from_bits(x).is_finite()
                    && f32::from_bits(y).is_finite()
                    && revision == self.unified_ifc_source_revision
            });
        let paint_is_canonical = match &self.paint {
            FocusedAtomicCaretSourcePaintSeal::Hidden => !self.caret_visible,
            FocusedAtomicCaretSourcePaintSeal::Present {
                bounds_bits,
                payload_identity,
            } => {
                let [x, y, width, height] = bounds_bits.map(f32::from_bits);
                matches!(
                    payload_identity,
                    crate::view::paint::PaintPayloadIdentity::PreparedRects(rects)
                        if rects.len() == 1
                ) && self.caret_visible
                    && [x, y, width, height].into_iter().all(f32::is_finite)
                    && width.to_bits() == 1.0_f32.to_bits()
                    && height > 0.0
            }
        };
        !self.owner.is_null()
            && self.stable_id != 0
            && self.focused
            && self.should_render
            && self
                .ime_preedit_cursor
                .is_none_or(|(start, end)| start <= end)
            && self.local_scroll_bits == [0.0_f32.to_bits(); 2]
            && self.unified_ifc_source_revision != 0
            && last_apply_is_current
            && self
                .foreground_color_bits
                .map(f32::from_bits)
                .into_iter()
                .all(|channel| channel.is_finite() && (0.0..=1.0).contains(&channel))
            && paint_is_canonical
            && self.frozen_source_identity
                == FocusedAtomicCaretFrozenSourceIdentity {
                    owner: self.owner,
                    stable_id: self.stable_id,
                    focused: self.focused,
                    should_render: self.should_render,
                    caret_visible: self.caret_visible,
                    foreground_color_bits: self.foreground_color_bits,
                    cursor_char: self.cursor_char,
                    cursor_affinity: self.cursor_affinity,
                    ime_preedit_cursor: self.ime_preedit_cursor,
                    local_scroll_bits: self.local_scroll_bits,
                    unified_ifc_source_revision: self.unified_ifc_source_revision,
                    last_unified_apply_bits: self.last_unified_apply_bits,
                    paint: self.paint.clone(),
                }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FocusedAtomicPreeditSourceSeal {
    pub(crate) owner: NodeKey,
    pub(crate) stable_id: u64,
    pub(crate) content: Arc<str>,
    pub(crate) backing_text: Arc<str>,
    pub(crate) ime_preedit: Arc<str>,
    pub(crate) ime_preedit_cursor: Option<(usize, usize)>,
    pub(crate) cursor_char: usize,
    pub(crate) cursor_affinity: CaretAffinity,
    pub(crate) foreground_color_bits: [u32; 4],
    pub(crate) glyph_bounds_bits: [u32; 4],
    pub(crate) underline_bounds_bits: [u32; 4],
    pub(crate) glyph_identity: crate::view::paint::PaintPayloadIdentity,
    pub(crate) underline_identity: crate::view::paint::PaintPayloadIdentity,
    pub(crate) unified_ifc_source_revision: u64,
    pub(crate) last_unified_apply_bits: Option<(u32, u32, u64)>,
}

impl FocusedAtomicPreeditSourceSeal {
    pub(crate) fn is_canonical(&self) -> bool {
        if self.owner.is_null()
            || self.stable_id == 0
            || self.ime_preedit.is_empty()
            || self.cursor_char > self.content.chars().count()
            || self.backing_text.is_empty()
            || !self.backing_text.is_char_boundary(self.backing_text.len())
            || !self.ime_preedit_cursor.is_none_or(|(start, end)| {
                start <= end
                    && end <= self.ime_preedit.len()
                    && self.ime_preedit.is_char_boundary(start)
                    && self.ime_preedit.is_char_boundary(end)
            })
            || self.unified_ifc_source_revision == 0
            || !self
                .last_unified_apply_bits
                .is_some_and(|(x, y, revision)| {
                    f32::from_bits(x).is_finite()
                        && f32::from_bits(y).is_finite()
                        && revision == self.unified_ifc_source_revision
                })
            || self
                .foreground_color_bits
                .map(f32::from_bits)
                .into_iter()
                .any(|channel| !channel.is_finite() || !(0.0..=1.0).contains(&channel))
            || !crate::view::paint::preedit_glyph_identity_is_exact(
                &self.glyph_identity,
                self.glyph_bounds_bits,
                self.foreground_color_bits,
            )
            || !crate::view::paint::preedit_underline_identity_is_exact(
                &self.underline_identity,
                self.underline_bounds_bits,
                self.foreground_color_bits,
            )
        {
            return false;
        }
        true
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RetainedFocusedAtomicProjectionTextAreaFrozenSourceIdentity {
    atomic_source: RetainedAtomicProjectionTextAreaPaintGrammar,
    caret: FocusedAtomicCaretSourceSeal,
    preedit: Option<FocusedAtomicPreeditSourceSeal>,
}

/// Closed focused glyph source with exactly one atomic projection and its
/// post-children caret source fact.  No planner or compositor decision is
/// encoded in this source grammar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RetainedFocusedAtomicProjectionTextAreaPaintGrammar {
    pub(crate) atomic_source: RetainedAtomicProjectionTextAreaPaintGrammar,
    pub(crate) caret: FocusedAtomicCaretSourceSeal,
    pub(crate) preedit: Option<FocusedAtomicPreeditSourceSeal>,
    frozen_source_identity: RetainedFocusedAtomicProjectionTextAreaFrozenSourceIdentity,
}

impl RetainedFocusedAtomicProjectionTextAreaPaintGrammar {
    fn from_frozen_source_identity(
        frozen: RetainedFocusedAtomicProjectionTextAreaFrozenSourceIdentity,
    ) -> Option<Self> {
        let grammar = Self {
            atomic_source: frozen.atomic_source.clone(),
            caret: frozen.caret.clone(),
            preedit: frozen.preedit.clone(),
            frozen_source_identity: frozen,
        };
        grammar.is_canonical().then_some(grammar)
    }

    pub(crate) fn is_canonical(&self) -> bool {
        self.atomic_source.is_canonical()
            && self.caret.is_canonical()
            && self.preedit.as_ref().is_none_or(|preedit| {
                preedit.is_canonical()
                    && preedit.owner == self.caret.owner
                    && preedit.stable_id == self.caret.stable_id
                    && preedit.cursor_char == self.caret.cursor_char
                    && preedit.cursor_affinity == self.caret.cursor_affinity
                    && preedit.ime_preedit_cursor == self.caret.ime_preedit_cursor
                    && preedit.foreground_color_bits == self.caret.foreground_color_bits
                    && preedit.unified_ifc_source_revision == self.caret.unified_ifc_source_revision
                    && preedit.last_unified_apply_bits == self.caret.last_unified_apply_bits
            })
            && self.frozen_source_identity
                == RetainedFocusedAtomicProjectionTextAreaFrozenSourceIdentity {
                    atomic_source: self.atomic_source.clone(),
                    caret: self.caret.clone(),
                    preedit: self.preedit.clone(),
                }
    }
}

/// Closed source grammar for one root-owned nonempty selection plus one
/// realized atomic projection.  The nested atomic grammar retains the full
/// topology/geometry proof; this sibling additionally freezes the exact
/// root selection that must paint before the root glyph.
#[derive(Clone, Debug, PartialEq, Eq)]
struct RetainedAtomicProjectionSelectionTextAreaFrozenSourceIdentity {
    atomic_source: RetainedAtomicProjectionTextAreaPaintGrammar,
    selection: RetainedTextAreaPaintGrammar,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RetainedAtomicProjectionSelectionTextAreaPaintGrammar {
    pub(crate) atomic_source: RetainedAtomicProjectionTextAreaPaintGrammar,
    pub(crate) selection: RetainedTextAreaPaintGrammar,
    frozen_source_identity: RetainedAtomicProjectionSelectionTextAreaFrozenSourceIdentity,
}

impl RetainedAtomicProjectionSelectionTextAreaPaintGrammar {
    fn from_frozen_source_identity(
        frozen: RetainedAtomicProjectionSelectionTextAreaFrozenSourceIdentity,
    ) -> Option<Self> {
        let grammar = Self {
            atomic_source: frozen.atomic_source.clone(),
            selection: frozen.selection,
            frozen_source_identity: frozen,
        };
        grammar.is_canonical().then_some(grammar)
    }

    pub(crate) fn is_canonical(&self) -> bool {
        self.atomic_source.is_canonical()
            && matches!(
                self.selection,
                RetainedTextAreaPaintGrammar::SelectionGlyphs { .. }
            )
            && self.selection.is_canonical()
            && self.frozen_source_identity
                == RetainedAtomicProjectionSelectionTextAreaFrozenSourceIdentity {
                    atomic_source: self.atomic_source.clone(),
                    selection: self.selection,
                }
    }
}

pub struct TextArea {
    // text
    pub(crate) content: String,
    pub(crate) placeholder: String,
    pub(crate) placeholder_color: crate::style::Color,
    pub(crate) read_only: bool,
    pub(crate) multiline: bool,
    pub(crate) auto_wrap: bool,
    pub(crate) max_length: Option<usize>,
    pub(crate) text_binding: Option<Binding<String>>,
    pub(crate) font_families: Vec<String>,
    pub(crate) font_size: f32,
    pub(crate) font_weight: u16,
    pub(crate) line_height: f32,
    pub(crate) vertical_align: crate::style::VerticalAlign,
    pub(crate) color: crate::style::Color,
    pub(crate) cursor: Cursor,

    // cursor / selection / IME / focus
    pub(crate) cursor_char: usize,
    /// Soft-wrap caret affinity. char index alone is ambiguous at a wrap
    /// boundary (= "end of upper line" === "start of lower line"); this
    /// disambiguates. Default `Downstream` (start of lower line) matches
    /// the long-standing pre-affinity behaviour. Set to `Upstream` by
    /// Cmd+Right and other line-end navigations.
    pub(crate) cursor_affinity: caret_map::CaretAffinity,
    pub(crate) selection_anchor_char: Option<usize>,
    pub(crate) selection_focus_char: Option<usize>,
    pub(crate) selection_background_color: crate::style::Color,
    pub(crate) pointer_selecting: bool,
    pub(crate) is_focused: bool,
    pub(crate) scroll_x: f32,
    pub(crate) scroll_y: f32,
    pub(crate) viewport_size: crate::view::base_component::Size,
    pub(crate) pending_caret_scroll: bool,
    pub(crate) ime_preedit: String,
    pub(crate) ime_preedit_cursor: Option<(usize, usize)>,
    pub(crate) vertical_cursor_x: Option<f32>,
    /// Retained caret phase consumed by paint and metadata.
    pub(crate) caret_visible: bool,
    /// Viewport-owned monotonic anchor. Only the generic animation tick may
    /// install or read it; paint paths are pure retained-state readers.
    pub(crate) caret_blink_epoch: Option<crate::time::Instant>,

    // children (TextAreaTextRun + projection mixed)
    pub(crate) on_render_handler: Option<TextAreaRenderHandlerProp>,
    pub(crate) children: Vec<NodeKey>,
    pub(crate) child_char_ranges: Vec<Range<usize>>,
    /// P6 reconcile metadata, parallel to `children` / `child_char_ranges`.
    /// `Run` slots carry no user-state identity (Runs are owned by TextArea);
    /// `Projection` slots remember the projection root's `RsxNodeIdentity`
    /// (post-Provider-unwrap) plus the last `RsxNode` so the next rebuild
    /// can identity-match → `reconcile_existing_subtree` instead of full
    /// teardown.
    pub(crate) child_slots: Vec<crate::view::base_component::text_area::projection::ChildSlot>,
    pub(crate) self_node_key: Option<NodeKey>,
    pub(crate) children_dirty: bool,
    /// Bumped whenever unified-IFC source inputs change (content edits,
    /// projection rebuilds, atomic child resizes). Lets the package cache
    /// validate with an O(1) revision check instead of rebuilding the
    /// whole source (per-run text clones + full content hash) per call.
    pub(crate) unified_ifc_source_revision: std::cell::Cell<u64>,
    /// Constraints of the last full measure; a clean subtree re-measured
    /// with identical constraints skips the O(children) child loops.
    pub(crate) last_measure_constraints: Option<LayoutConstraints>,
    /// (origin_x, origin_y, source revision) of the last child placement
    /// apply; identical values skip the O(children) apply loop and a pure
    /// move applies as an in-place delta shift.
    pub(crate) last_unified_apply: std::cell::Cell<Option<(f32, f32, u64)>>,
    pub(crate) unified_inline_ifc_root_cache:
        std::cell::RefCell<inline_ifc::TextAreaUnifiedIfcRootCache>,

    // layout output
    pub(crate) flow_offset: crate::view::base_component::Position,
    pub(crate) layout_state: LayoutState,
    pub(crate) inline_paint_fragments: Vec<Rect>,
    pub(crate) flex_info: Option<FlexLayoutInfo>,
    pub(crate) dirty_flags: DirtyFlags,

    #[cfg(test)]
    pub(crate) retained_source_test_active_animator: bool,
    #[cfg(test)]
    pub(crate) retained_source_test_deferred: bool,

    // handlers
    pub(crate) on_change_handlers: Vec<TextChangeHandlerProp>,
    pub(crate) on_focus_handlers: Vec<TextAreaFocusHandlerProp>,
    pub(crate) on_blur_handlers: Vec<BlurHandlerProp>,

    // identity
    pub(crate) node_id: u64,
    pub(crate) parent_id: Option<u64>,
}

impl Default for TextArea {
    fn default() -> Self {
        Self {
            content: String::new(),
            placeholder: String::new(),
            placeholder_color: crate::style::Color::rgba(125, 133, 150, 255),
            read_only: false,
            multiline: true,
            auto_wrap: true,
            max_length: None,
            text_binding: None,
            font_families: Vec::new(),
            font_size: 14.0,
            font_weight: 400,
            line_height: 1.25,
            vertical_align: crate::style::VerticalAlign::Baseline,
            color: crate::style::Color::rgba(17, 17, 17, 255),
            cursor: Cursor::Text,

            cursor_char: 0,
            cursor_affinity: caret_map::CaretAffinity::Downstream,
            selection_anchor_char: None,
            selection_focus_char: None,
            selection_background_color: crate::style::Color::rgba(71, 133, 240, 89),
            pointer_selecting: false,
            is_focused: false,
            scroll_x: 0.0,
            scroll_y: 0.0,
            viewport_size: crate::view::base_component::Size {
                width: 0.0,
                height: 0.0,
            },
            pending_caret_scroll: false,
            ime_preedit: String::new(),
            ime_preedit_cursor: None,
            vertical_cursor_x: None,
            caret_visible: false,
            caret_blink_epoch: None,

            on_render_handler: None,
            children: Vec::new(),
            child_char_ranges: Vec::new(),
            child_slots: Vec::new(),
            self_node_key: None,
            children_dirty: true,
            unified_ifc_source_revision: std::cell::Cell::new(0),
            last_measure_constraints: None,
            last_unified_apply: std::cell::Cell::new(None),
            unified_inline_ifc_root_cache: std::cell::RefCell::default(),

            flow_offset: crate::view::base_component::Position { x: 0.0, y: 0.0 },
            layout_state: LayoutState::new(0.0, 0.0, 0.0, 0.0),
            inline_paint_fragments: Vec::new(),
            flex_info: None,
            dirty_flags: DirtyFlags::ALL,

            #[cfg(test)]
            retained_source_test_active_animator: false,
            #[cfg(test)]
            retained_source_test_deferred: false,

            on_change_handlers: Vec::new(),
            on_focus_handlers: Vec::new(),
            on_blur_handlers: Vec::new(),

            node_id: next_ui_node_id(),
            parent_id: None,
        }
    }
}

impl TextArea {
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct with an externally-supplied stable id (matches the
    /// `from_*_with_id` pattern used by v1 builtin host tags so the
    /// descriptor pipeline can keep node identity stable across renders).
    pub fn with_stable_id(node_id: u64) -> Self {
        Self {
            node_id,
            ..Self::default()
        }
    }

    pub(crate) fn set_self_node_key(&mut self, key: NodeKey) {
        self.self_node_key = Some(key);
    }

    pub(crate) fn run_style(&self, color: crate::style::Color) -> TextAreaRunStyle<'_> {
        TextAreaRunStyle {
            font_families: &self.font_families,
            font_size: self.font_size,
            line_height: self.line_height,
            vertical_align: self.vertical_align,
            font_weight: self.font_weight,
            color,
            cursor: self.cursor,
            auto_wrap: self.auto_wrap,
        }
    }

    /// Patch entrypoint for `Patch::SetText` from incremental commit.
    /// Mirrors v1's surface so `fiber_work::apply_set_text_to_host` can
    /// route SetText to either Text or TextArea uniformly.
    pub fn set_text(&mut self, value: String) {
        self.set_content_from_external(value);
    }

    /// Re-apply ancestor-derived inherited cascade to props the user
    /// didn't author explicitly. Called by
    /// [`crate::view::fiber_work::recascade_text_subtree`] after an
    /// ancestor style change.
    ///
    /// v2 doesn't yet track per-prop "explicit" flags (all TextArea
    /// authors set values through `apply_prop` which writes
    /// unconditionally). Conservative behaviour: overwrite
    /// font_families when empty (default), font_size only when it still
    /// matches the previously-cached inherited value (i.e. nothing else
    /// has touched it since), and likewise for color / font_weight.
    /// Marks content dirty so the next rebuild re-cascades the Run
    /// children.
    pub(crate) fn apply_inherited(
        &mut self,
        inherited: &crate::view::renderer_adapter::StyleCascadeContext,
    ) -> bool {
        let mut changed = false;
        if self.font_families.is_empty()
            && let Some(font_families) = inherited.inherited_font_families()
            && !font_families.is_empty()
        {
            self.font_families = font_families.to_vec();
            changed = true;
        }
        if let Some(fs) = inherited.inherited_font_size()
            && (self.font_size - fs).abs() > f32::EPSILON
            && (self.font_size - 14.0).abs() < f32::EPSILON
        {
            self.font_size = fs;
            changed = true;
        }
        if let Some(fw) = inherited.inherited_font_weight()
            && self.font_weight == 400
            && fw != 400
        {
            self.font_weight = fw;
            changed = true;
        }
        if let Some(line_height) = inherited.inherited_line_height()
            && (self.line_height - line_height).abs() > f32::EPSILON
        {
            self.line_height = line_height;
            changed = true;
        }
        if let Some(color) = inherited.inherited_color()
            && self.color == crate::style::Color::rgba(17, 17, 17, 255)
        {
            self.color = color;
            changed = true;
        }
        if let Some(vertical_align) = inherited.inherited_vertical_align()
            && self.vertical_align != vertical_align
        {
            self.vertical_align = vertical_align;
            changed = true;
        }
        if changed {
            self.mark_content_dirty();
        }
        changed
    }
}

impl ElementTrait for TextArea {
    fn has_active_animator(&self) -> bool {
        #[cfg(test)]
        {
            return self.retained_source_test_active_animator;
        }
        #[cfg(not(test))]
        {
            false
        }
    }

    fn is_deferred_to_root_viewport_render(&self) -> bool {
        #[cfg(test)]
        {
            return self.retained_source_test_deferred;
        }
        #[cfg(not(test))]
        {
            false
        }
    }

    fn tick_animation_frame(&mut self, now: crate::time::Instant) -> DirtyFlags {
        self.tick_caret_blink(now)
    }

    fn contents_logical_scissor(&self) -> Option<[u32; 4]> {
        Some(self.viewport_logical_scissor_rect())
    }

    #[allow(private_interfaces)]
    fn shadow_paint_recording_context(
        &self,
        mut parent: crate::view::paint::PaintRecordingContext,
    ) -> crate::view::paint::PaintRecordingContext {
        let paint_x = self.layout_state.layout_position.x + parent.paint_offset[0];
        let paint_y = self.layout_state.layout_position.y + parent.paint_offset[1];
        parent.paint_offset[0] += round_layout_value(paint_x) - paint_x;
        parent.paint_offset[1] += round_layout_value(paint_y) - paint_y;
        parent.inside_text_area = true;
        parent
    }

    #[allow(private_interfaces)]
    fn shadow_paint_recording_context_for_child(
        &self,
        child: NodeKey,
        arena: &NodeArena,
        parent: crate::view::paint::PaintRecordingContext,
    ) -> crate::view::paint::PaintRecordingContext {
        let mut child_context = parent.without_text_area_child_authority();
        child_context.text_area_selection =
            self.projection_selection_witness_for_child(child, arena);
        child_context.text_area_preedit = self.projection_preedit_witness_for_child(child, arena);
        child_context
    }

    #[allow(private_interfaces)]
    fn shadow_paint_recording_capability(
        &self,
        arena: &crate::view::node_arena::NodeArena,
        deferred_phase_root: bool,
        recording_context: crate::view::paint::PaintRecordingContext,
    ) -> crate::view::base_component::ShadowPaintRecordingCapability {
        let Some(owner) = self.self_node_key else {
            return crate::view::base_component::ShadowPaintRecordingCapability::Unsupported;
        };
        match self.prepared_plain_shadow_text_payload(
            owner,
            arena,
            deferred_phase_root,
            recording_context.paint_offset,
        ) {
            Ok(payload)
                if payload.glyph_op.is_some()
                    || payload.selection.is_some()
                    || payload.decoration.is_some()
                    || payload.caret.is_some() =>
            {
                crate::view::base_component::ShadowPaintRecordingCapability::Recordable
            }
            Ok(_) => crate::view::base_component::ShadowPaintRecordingCapability::Transparent,
            Err(render::PlainTextAreaPaintFailure::Unsupported) => {
                crate::view::base_component::ShadowPaintRecordingCapability::Unsupported
            }
            Err(render::PlainTextAreaPaintFailure::Legacy(blocker)) => {
                crate::view::base_component::ShadowPaintRecordingCapability::Legacy(blocker)
            }
        }
    }

    #[allow(private_interfaces)]
    fn record_shadow_paint_metadata_plan(
        &self,
        owner: crate::view::node_arena::NodeKey,
        _properties: crate::view::compositor::property_tree::PropertyTreeState,
        contents_properties: crate::view::compositor::property_tree::PropertyTreeState,
        content_revision: crate::view::paint::PaintContentRevision,
        arena: &crate::view::node_arena::NodeArena,
        recording_context: crate::view::paint::PaintRecordingContext,
    ) -> Option<crate::view::paint::PaintNodePlan<crate::view::paint::PaintChunkMetadata>> {
        let mut payload = self
            .prepared_plain_shadow_text_payload(owner, arena, false, recording_context.paint_offset)
            .ok()?;
        if recording_context.suppresses_interactive_text_area_caret(owner) {
            payload.caret = None;
        }
        let mut before_children = Vec::with_capacity(2);
        if let Some(selection) = payload.selection.as_ref() {
            before_children.push(crate::view::paint::PaintChunkMetadata {
                id: crate::view::paint::PaintChunkId {
                    owner,
                    scope: crate::view::paint::PaintPropertyScope::Contents,
                    phase: crate::view::paint::PaintNodePhase::BeforeChildren,
                    slot: 0,
                    role: crate::view::paint::PaintChunkRole::SelectionUnderlay,
                },
                owner,
                bounds: selection.bounds,
                properties: contents_properties,
                content_revision,
                payload_identity: crate::view::paint::PaintPayloadIdentity::prepared_rects(
                    selection.ops.iter(),
                )?,
            });
        }
        if let Some(op) = payload.glyph_op.as_ref() {
            before_children.push(crate::view::paint::PaintChunkMetadata {
                id: crate::view::paint::PaintChunkId {
                    owner,
                    scope: crate::view::paint::PaintPropertyScope::Contents,
                    phase: crate::view::paint::PaintNodePhase::BeforeChildren,
                    slot: 1,
                    role: crate::view::paint::PaintChunkRole::TextGlyphs,
                },
                owner,
                bounds: payload.glyph_bounds,
                properties: contents_properties,
                content_revision,
                payload_identity: crate::view::paint::PaintPayloadIdentity::prepared_texts([op]),
            });
        }
        let mut after_children = Vec::with_capacity(2);
        if let Some(decoration) = payload.decoration.as_ref() {
            after_children.push(crate::view::paint::PaintChunkMetadata {
                id: crate::view::paint::PaintChunkId {
                    owner,
                    scope: crate::view::paint::PaintPropertyScope::Contents,
                    phase: crate::view::paint::PaintNodePhase::AfterChildren,
                    slot: 0,
                    role: crate::view::paint::PaintChunkRole::TextDecoration,
                },
                owner,
                bounds: decoration.bounds,
                properties: contents_properties,
                content_revision,
                payload_identity: crate::view::paint::PaintPayloadIdentity::prepared_rects(
                    decoration.ops.iter(),
                )?,
            });
        }
        if let Some(caret) = payload.caret.as_ref() {
            after_children.push(crate::view::paint::PaintChunkMetadata {
                id: crate::view::paint::PaintChunkId {
                    owner,
                    scope: crate::view::paint::PaintPropertyScope::Contents,
                    phase: crate::view::paint::PaintNodePhase::AfterChildren,
                    slot: 1,
                    role: crate::view::paint::PaintChunkRole::Caret,
                },
                owner,
                bounds: caret.bounds,
                properties: contents_properties,
                content_revision,
                payload_identity: crate::view::paint::PaintPayloadIdentity::prepared_rects([
                    &caret.op,
                ])?,
            });
        }
        (!before_children.is_empty() || !after_children.is_empty()).then_some(
            crate::view::paint::PaintNodePlan {
                before_children,
                after_children,
            },
        )
    }

    #[allow(private_interfaces)]
    fn record_shadow_paint_artifact_plan(
        &self,
        owner: crate::view::node_arena::NodeKey,
        _properties: crate::view::compositor::property_tree::PropertyTreeState,
        contents_properties: crate::view::compositor::property_tree::PropertyTreeState,
        content_revision: crate::view::paint::PaintContentRevision,
        arena: &crate::view::node_arena::NodeArena,
        recording_context: crate::view::paint::PaintRecordingContext,
    ) -> Option<crate::view::paint::PaintNodePlan<crate::view::paint::PaintArtifact>> {
        let mut payload = self
            .prepared_plain_shadow_text_payload(owner, arena, false, recording_context.paint_offset)
            .ok()?;
        if recording_context.suppresses_interactive_text_area_caret(owner) {
            payload.caret = None;
        }
        let mut before_children = Vec::with_capacity(2);
        if let Some(selection) = payload.selection {
            let payload_identity =
                crate::view::paint::PaintPayloadIdentity::prepared_rects(selection.ops.iter())?;
            let op_count = selection.ops.len();
            before_children.push(crate::view::paint::PaintArtifact {
                target: Default::default(),
                chunks: vec![crate::view::paint::PaintChunk {
                    id: crate::view::paint::PaintChunkId {
                        owner,
                        scope: crate::view::paint::PaintPropertyScope::Contents,
                        phase: crate::view::paint::PaintNodePhase::BeforeChildren,
                        slot: 0,
                        role: crate::view::paint::PaintChunkRole::SelectionUnderlay,
                    },
                    owner,
                    op_range: 0..op_count,
                    bounds: selection.bounds,
                    properties: contents_properties,
                    content_revision,
                    payload_identity,
                }],
                ops: selection
                    .ops
                    .into_iter()
                    .map(crate::view::paint::PaintOp::DrawRect)
                    .collect(),
                clip_nodes: Vec::new(),
                effect_nodes: Vec::new(),
                owner_nodes: vec![crate::view::paint::PaintOwnerSnapshot {
                    owner,
                    parent: None,
                }],
            });
        }
        if let Some(op) = payload.glyph_op {
            let payload_identity = crate::view::paint::PaintPayloadIdentity::prepared_texts([&op]);
            before_children.push(crate::view::paint::PaintArtifact {
                target: Default::default(),
                chunks: vec![crate::view::paint::PaintChunk {
                    id: crate::view::paint::PaintChunkId {
                        owner,
                        scope: crate::view::paint::PaintPropertyScope::Contents,
                        phase: crate::view::paint::PaintNodePhase::BeforeChildren,
                        slot: 1,
                        role: crate::view::paint::PaintChunkRole::TextGlyphs,
                    },
                    owner,
                    op_range: 0..1,
                    bounds: payload.glyph_bounds,
                    properties: contents_properties,
                    content_revision,
                    payload_identity,
                }],
                ops: vec![crate::view::paint::PaintOp::PreparedText(op)],
                clip_nodes: Vec::new(),
                effect_nodes: Vec::new(),
                owner_nodes: vec![crate::view::paint::PaintOwnerSnapshot {
                    owner,
                    parent: None,
                }],
            });
        }
        let mut after_children = Vec::with_capacity(2);
        if let Some(decoration) = payload.decoration {
            let payload_identity =
                crate::view::paint::PaintPayloadIdentity::prepared_rects(decoration.ops.iter())?;
            let op_count = decoration.ops.len();
            after_children.push(crate::view::paint::PaintArtifact {
                target: Default::default(),
                chunks: vec![crate::view::paint::PaintChunk {
                    id: crate::view::paint::PaintChunkId {
                        owner,
                        scope: crate::view::paint::PaintPropertyScope::Contents,
                        phase: crate::view::paint::PaintNodePhase::AfterChildren,
                        slot: 0,
                        role: crate::view::paint::PaintChunkRole::TextDecoration,
                    },
                    owner,
                    op_range: 0..op_count,
                    bounds: decoration.bounds,
                    properties: contents_properties,
                    content_revision,
                    payload_identity,
                }],
                ops: decoration
                    .ops
                    .into_iter()
                    .map(crate::view::paint::PaintOp::DrawRect)
                    .collect(),
                clip_nodes: Vec::new(),
                effect_nodes: Vec::new(),
                owner_nodes: vec![crate::view::paint::PaintOwnerSnapshot {
                    owner,
                    parent: None,
                }],
            });
        }
        if let Some(caret) = payload.caret {
            let payload_identity =
                crate::view::paint::PaintPayloadIdentity::prepared_rects([&caret.op])?;
            after_children.push(crate::view::paint::PaintArtifact {
                target: Default::default(),
                chunks: vec![crate::view::paint::PaintChunk {
                    id: crate::view::paint::PaintChunkId {
                        owner,
                        scope: crate::view::paint::PaintPropertyScope::Contents,
                        phase: crate::view::paint::PaintNodePhase::AfterChildren,
                        slot: 1,
                        role: crate::view::paint::PaintChunkRole::Caret,
                    },
                    owner,
                    op_range: 0..1,
                    bounds: caret.bounds,
                    properties: contents_properties,
                    content_revision,
                    payload_identity,
                }],
                ops: vec![crate::view::paint::PaintOp::DrawRect(caret.op)],
                clip_nodes: Vec::new(),
                effect_nodes: Vec::new(),
                owner_nodes: vec![crate::view::paint::PaintOwnerSnapshot {
                    owner,
                    parent: None,
                }],
            });
        }
        if before_children.is_empty() && after_children.is_empty() {
            return None;
        }
        #[cfg(test)]
        crate::view::paint::note_full_artifact_record();
        Some(crate::view::paint::PaintNodePlan {
            before_children,
            after_children,
        })
    }

    fn placement_eligibility_metadata(
        &self,
    ) -> crate::view::node_arena::PlacementEligibilityMetadata {
        // Conservative: the TextArea family manages an internal projection /
        // IME / caret subtree whose placement is not yet proven stable under
        // ancestor-skip, so it blocks placement-skip for now (preserving the
        // pre-trait behavior). Text/Image/Svg leaves are transparent instead.
        crate::view::node_arena::PlacementEligibilityMetadata::non_base_blocker()
    }

    fn stable_id(&self) -> u64 {
        self.node_id
    }

    fn box_model_snapshot(&self) -> BoxModelSnapshot {
        BoxModelSnapshot {
            node_id: self.node_id,
            parent_id: self.parent_id,
            x: self.layout_state.layout_position.x,
            y: self.layout_state.layout_position.y,
            width: self
                .layout_state
                .layout_size
                .width
                .max(self.viewport_size.width),
            height: self.layout_state.layout_size.height,
            border_radius: 0.0,
            should_render: self.layout_state.should_render,
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn children(&self) -> &[NodeKey] {
        &self.children
    }

    fn sync_children_mirror(&mut self, children: &[NodeKey]) {
        self.children.clear();
        self.children.extend_from_slice(children);
    }

    fn parent_id(&self) -> Option<u64> {
        self.parent_id
    }

    fn set_parent_id(&mut self, parent_id: Option<u64>) {
        self.parent_id = parent_id;
    }

    fn local_dirty_flags(&self) -> DirtyFlags {
        self.dirty_flags
    }

    fn clear_local_dirty_flags(&mut self, flags: DirtyFlags) {
        self.dirty_flags = self.dirty_flags.without(flags);
    }

    /// Hash every visible-state field so retained paint generations advance
    /// on edit, cursor, selection, IME, focus, and blink changes.
    fn retained_paint_signature(&self) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        self.layout_state.should_render.hash(&mut hasher);
        self.layout_state
            .layout_position
            .x
            .to_bits()
            .hash(&mut hasher);
        self.layout_state
            .layout_position
            .y
            .to_bits()
            .hash(&mut hasher);
        self.layout_state
            .layout_size
            .width
            .max(0.0)
            .to_bits()
            .hash(&mut hasher);
        self.layout_state
            .layout_size
            .height
            .max(0.0)
            .to_bits()
            .hash(&mut hasher);
        self.content.hash(&mut hasher);
        self.placeholder.hash(&mut hasher);
        self.color.to_rgba_u8().hash(&mut hasher);
        self.placeholder_color.to_rgba_u8().hash(&mut hasher);
        self.selection_background_color
            .to_rgba_u8()
            .hash(&mut hasher);
        self.font_families.hash(&mut hasher);
        self.font_size.to_bits().hash(&mut hasher);
        self.line_height.to_bits().hash(&mut hasher);
        self.vertical_align.hash(&mut hasher);
        self.font_weight.hash(&mut hasher);
        self.multiline.hash(&mut hasher);
        self.auto_wrap.hash(&mut hasher);
        self.read_only.hash(&mut hasher);
        self.cursor_char.hash(&mut hasher);
        self.cursor_affinity.hash(&mut hasher);
        self.selection_anchor_char.hash(&mut hasher);
        self.selection_focus_char.hash(&mut hasher);
        self.scroll_x.to_bits().hash(&mut hasher);
        self.scroll_y.to_bits().hash(&mut hasher);
        self.ime_preedit.hash(&mut hasher);
        self.ime_preedit_cursor.hash(&mut hasher);
        self.is_focused.hash(&mut hasher);
        self.caret_visible.hash(&mut hasher);
        self.children.len().hash(&mut hasher);
        hasher.finish()
    }

    fn retained_paint_signature_is_complete(&self) -> bool {
        true
    }

    fn apply_inherited(&mut self, inherited: &crate::view::renderer_adapter::StyleCascadeContext) {
        TextArea::apply_inherited(self, inherited);
    }

    fn after_commit(&mut self, _arena: &mut crate::view::node_arena::NodeArena, self_key: NodeKey) {
        self.set_self_node_key(self_key);
    }

    fn build_children(
        &self,
        _node: &crate::ui::RsxElementNode,
        _path: &[u64],
        _global_path: Option<&crate::view::renderer_adapter::GlobalNodePath>,
        _inherited: &crate::view::renderer_adapter::StyleCascadeContext,
    ) -> Result<Vec<crate::view::renderer_adapter::ElementDescriptor>, String> {
        // Spawn a single `TextAreaTextRun` from `self.content` (or
        // placeholder fallback). RSX `node.children` are not walked
        // here — projection segments rebuild lazily via
        // `rebuild_projection_tree_if_dirty` after the TextArea
        // exists in the arena.
        let mut child_descriptors: Vec<crate::view::renderer_adapter::ElementDescriptor> =
            Vec::new();
        let (display_text, is_placeholder) = if !self.content.is_empty() {
            (self.content.clone(), false)
        } else if !self.placeholder.is_empty() {
            (self.placeholder.clone(), true)
        } else {
            (String::new(), false)
        };
        if !display_text.is_empty() {
            let char_count = display_text.chars().count();
            let mut run = run::TextAreaTextRun::new(display_text, 0..char_count);
            run.is_placeholder = is_placeholder;
            run.cascade_style(self.run_style(if is_placeholder {
                self.placeholder_color
            } else {
                self.color
            }));
            child_descriptors.push(crate::view::renderer_adapter::ElementDescriptor::leaf(
                Box::new(run) as Box<dyn ElementTrait>,
            ));
        }
        Ok(child_descriptors)
    }

    fn ingest_props(&mut self, node: &crate::ui::RsxElementNode) -> Result<(), String> {
        use crate::ui::FromPropValue;
        use crate::view::base_component::as_blur_handler;
        use crate::view::renderer_adapter::{
            as_binding_string, as_bool, as_owned_string, as_usize,
        };
        for (key, value) in node.props.iter() {
            match *key {
                // Cold-path-owned: identity, layered style, explicit
                // font-priority block (cascade-resolved).
                "key" | "style" | "font" | "font_size" => {}
                "content" => self.content = as_owned_string(value, key)?,
                "placeholder" => self.placeholder = as_owned_string(value, key)?,
                "binding" => self.text_binding = Some(as_binding_string(value, key)?),
                "multiline" => self.multiline = as_bool(value, key)?,
                "auto_wrap" => self.auto_wrap = as_bool(value, key)?,
                "read_only" => self.read_only = as_bool(value, key)?,
                "max_length" => self.max_length = as_usize(value, key)?,
                "on_focus" => self.on_focus_handlers.push(
                    crate::ui::TextAreaFocusHandlerProp::from_prop_value(value.clone()).map_err(
                        |_| format!("prop `{key}` expects text area focus handler value"),
                    )?,
                ),
                "on_blur" => self.on_blur_handlers.push(as_blur_handler(value, key)?),
                "on_change" => self.on_change_handlers.push(
                    crate::ui::TextChangeHandlerProp::from_prop_value(value.clone())
                        .map_err(|_| format!("prop `{key}` expects text change handler value"))?,
                ),
                "on_render" => {
                    self.on_render_handler = Some(
                        crate::ui::TextAreaRenderHandlerProp::from_prop_value(value.clone())
                            .map_err(|_| {
                                format!("prop `{key}` expects text area render handler value")
                            })?,
                    );
                }
                _ => return Err(format!("unknown prop `{}` on <TextArea>", key)),
            }
        }
        Ok(())
    }

    /// Real incremental apply path. Mirrors v1's surface (decision: keep
    /// the apply matrix shape parity-with-v1 to ease P7 migration), minus
    /// the box-model props that v2 rejects per design A1.
    fn apply_prop(
        &mut self,
        arena: &mut crate::view::node_arena::NodeArena,
        self_key: crate::view::node_arena::NodeKey,
        ctx: &crate::view::fiber_work::ApplyContext<'_>,
        name: &'static str,
        value: crate::ui::PropValue,
    ) -> crate::view::fiber_work::PropApplyOutcome {
        use crate::ui::FromPropValue;
        use crate::view::fiber_work::{PropApplyOutcome, resolve_font_size_px_with_inherited};
        use crate::view::renderer_adapter::{StyleCascadeContext, style_cascade_at_parent};

        self.set_self_node_key(self_key);

        let resolve_inherited =
            |arena: &crate::view::node_arena::NodeArena| -> StyleCascadeContext {
                match arena.parent_of(self_key) {
                    Some(p) => style_cascade_at_parent(
                        arena,
                        p,
                        ctx.viewport_style,
                        ctx.viewport_width,
                        ctx.viewport_height,
                    ),
                    None => StyleCascadeContext::from_viewport_style(
                        ctx.viewport_style,
                        ctx.viewport_width,
                        ctx.viewport_height,
                    ),
                }
            };

        match name {
            "key" => PropApplyOutcome::Applied,
            "binding" => {
                let Ok(bound) = crate::ui::Binding::<String>::from_prop_value(value) else {
                    return PropApplyOutcome::DecodeFailed(name);
                };
                self.set_content_from_external(bound.get());
                self.text_binding = Some(bound);
                PropApplyOutcome::Applied
            }
            "content" => {
                let Ok(s) = String::from_prop_value(value) else {
                    return PropApplyOutcome::DecodeFailed(name);
                };
                self.set_content_from_external(s);
                PropApplyOutcome::Applied
            }
            "placeholder" => {
                let Ok(s) = String::from_prop_value(value) else {
                    return PropApplyOutcome::DecodeFailed(name);
                };
                if self.placeholder != s {
                    self.placeholder = s;
                    self.mark_content_dirty();
                }
                PropApplyOutcome::Applied
            }
            "multiline" => {
                let Ok(v) = bool::from_prop_value(value) else {
                    return PropApplyOutcome::DecodeFailed(name);
                };
                if self.multiline != v {
                    self.multiline = v;
                    if !v && self.content.contains('\n') {
                        self.content = self.content.replace('\n', " ");
                        self.sync_bound_text();
                    }
                    self.mark_content_dirty();
                }
                PropApplyOutcome::Applied
            }
            "auto_wrap" => {
                let Ok(v) = bool::from_prop_value(value) else {
                    return PropApplyOutcome::DecodeFailed(name);
                };
                if self.auto_wrap != v {
                    self.auto_wrap = v;
                    // mark_content_dirty triggers rebuild_children_if_dirty
                    // which re-cascades the Run subtree. No standalone
                    // recascade needed.
                    self.mark_content_dirty();
                }
                PropApplyOutcome::Applied
            }
            "read_only" => {
                let Ok(v) = bool::from_prop_value(value) else {
                    return PropApplyOutcome::DecodeFailed(name);
                };
                self.read_only = v;
                PropApplyOutcome::Applied
            }
            "max_length" => {
                let v = match &value {
                    crate::ui::PropValue::I64(i) => Some((*i).max(0) as usize),
                    crate::ui::PropValue::F64(f) => Some((*f).max(0.0) as usize),
                    _ => return PropApplyOutcome::DecodeFailed(name),
                };
                self.set_max_length(v);
                PropApplyOutcome::Applied
            }
            "font" => {
                let Ok(s) = String::from_prop_value(value) else {
                    return PropApplyOutcome::DecodeFailed(name);
                };
                self.font_families = vec![s];
                self.mark_content_dirty();
                PropApplyOutcome::Applied
            }
            "font_size" => {
                let inherited = resolve_inherited(arena);
                let Some(px) = resolve_font_size_px_with_inherited(&value, &inherited) else {
                    return PropApplyOutcome::DecodeFailed(name);
                };
                if (self.font_size - px).abs() > f32::EPSILON {
                    self.font_size = px;
                    self.mark_content_dirty();
                }
                PropApplyOutcome::Applied
            }
            "style" => {
                let Ok(style) = crate::view::renderer_adapter::as_element_style(&value, name)
                else {
                    return PropApplyOutcome::DecodeFailed(name);
                };
                let inherited = resolve_inherited(arena);
                self.apply_style_incremental(&style, &inherited);
                PropApplyOutcome::Applied
            }
            "on_change" => {
                let Ok(handler) = crate::ui::TextChangeHandlerProp::from_prop_value(value) else {
                    return PropApplyOutcome::DecodeFailed(name);
                };
                self.on_change_handlers.clear();
                self.on_change_handlers.push(handler);
                PropApplyOutcome::Applied
            }
            "on_focus" => {
                let Ok(handler) = crate::ui::TextAreaFocusHandlerProp::from_prop_value(value)
                else {
                    return PropApplyOutcome::DecodeFailed(name);
                };
                self.on_focus_handlers.clear();
                self.on_focus_handlers.push(handler);
                PropApplyOutcome::Applied
            }
            "on_blur" => {
                let Ok(handler) = crate::ui::BlurHandlerProp::from_prop_value(value) else {
                    return PropApplyOutcome::DecodeFailed(name);
                };
                self.on_blur_handlers.clear();
                self.on_blur_handlers.push(handler);
                PropApplyOutcome::Applied
            }
            "on_render" => {
                let Ok(handler) = crate::ui::TextAreaRenderHandlerProp::from_prop_value(value)
                else {
                    return PropApplyOutcome::DecodeFailed(name);
                };
                self.on_render_handler = Some(handler);
                self.mark_content_dirty();
                PropApplyOutcome::Applied
            }
            _ => PropApplyOutcome::UnknownProp,
        }
    }

    fn reset_prop(
        &mut self,
        _arena: &mut crate::view::node_arena::NodeArena,
        _self_key: crate::view::node_arena::NodeKey,
        _ctx: &crate::view::fiber_work::ApplyContext<'_>,
        name: &'static str,
    ) -> crate::view::fiber_work::PropApplyOutcome {
        use crate::view::fiber_work::PropApplyOutcome;
        match name {
            "key" => PropApplyOutcome::Applied,
            "binding" => {
                self.text_binding = None;
                PropApplyOutcome::Applied
            }
            "content" => {
                self.set_content_from_external(String::new());
                PropApplyOutcome::Applied
            }
            "placeholder" => {
                if !self.placeholder.is_empty() {
                    self.placeholder.clear();
                    self.mark_content_dirty();
                }
                PropApplyOutcome::Applied
            }
            "multiline" => {
                self.multiline = true;
                self.mark_content_dirty();
                PropApplyOutcome::Applied
            }
            "auto_wrap" => {
                self.auto_wrap = true;
                self.mark_content_dirty();
                PropApplyOutcome::Applied
            }
            "read_only" => {
                self.read_only = false;
                PropApplyOutcome::Applied
            }
            "max_length" => {
                self.max_length = None;
                PropApplyOutcome::Applied
            }
            "on_change" => {
                self.on_change_handlers.clear();
                PropApplyOutcome::Applied
            }
            "on_focus" => {
                self.on_focus_handlers.clear();
                PropApplyOutcome::Applied
            }
            "on_blur" => {
                self.on_blur_handlers.clear();
                PropApplyOutcome::Applied
            }
            "on_render" => {
                self.on_render_handler = None;
                self.mark_content_dirty();
                PropApplyOutcome::Applied
            }
            // Style / font props can't be reverted to "inherited" without a
            // full descriptor rebuild — fall back to the cold path.
            "style" | "font" | "font_size" => PropApplyOutcome::CannotReset(name),
            _ => PropApplyOutcome::CannotReset(name),
        }
    }
}

fn known_prop(name: &str) -> bool {
    matches!(
        name,
        "content"
            | "binding"
            | "style"
            | "on_focus"
            | "on_blur"
            | "on_change"
            | "on_render"
            | "placeholder"
            | "font"
            | "font_size"
            | "multiline"
            | "auto_wrap"
            | "read_only"
            | "max_length"
    )
}
