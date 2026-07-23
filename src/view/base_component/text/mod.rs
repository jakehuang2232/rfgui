use std::sync::Arc;

use crate::style::{
    ColorLike, Cursor, HexColor, TextWrap, Transform, TransformKind, TransformOrigin,
};
use crate::view::inline_formatting_context::{
    InlineFormattingContext, InlineIfcAlignment, InlineIfcTextPassPaintInput,
};
use glam::{Mat4, Vec3, Vec4};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use super::{BoxModelSnapshot, ElementTrait, Position, Size};
use crate::view::layout::LayoutState;

mod cache;
mod events;
mod hit_test;
mod layout;
mod measure;
mod profile;
mod props;
mod render;
mod style;

#[cfg(test)]
mod tests;

use self::cache::TextLayoutCache;

pub(in crate::view::base_component) use self::measure::measure_text_layout;

pub(crate) use self::hit_test::{
    TextAreaSelectionRenderContext, with_text_area_selection_render_context,
};

/// Per-visual-line geometry installed by an inline IFC root that owns this
/// Text node's glyphs. `rect` is the full line box (layout bounds, caret
/// height); `text_rect` is the baseline-aligned glyph box (where text
/// paints, used for fragment-position and selection geometry). `caret_xs`
/// holds one x per char boundary (`len == char_range.len() + 1`).
/// Coordinates are absolute viewport space once installed.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextIfcOwnedLine {
    pub(crate) rect: crate::ui::Rect,
    pub(crate) text_rect: crate::ui::Rect,
    pub(crate) char_range: std::ops::Range<usize>,
    pub(crate) caret_xs: Vec<f32>,
}

struct TextInlineIfcOwnedState {
    lines: Vec<TextIfcOwnedLine>,
    paint_input: Arc<InlineIfcTextPassPaintInput>,
    /// Absolute glyph-derived bounds used only as the TextPass fragment.
    /// Hit testing and caret geometry continue to use `lines`.
    paint_bounds: crate::ui::Rect,
}

#[derive(Clone, Copy, Default)]
pub(super) struct TextExplicitProps(u16);

impl TextExplicitProps {
    pub(super) const FONT_FAMILY: u16 = 1 << 0;
    pub(super) const FONT_SIZE: u16 = 1 << 1;
    pub(super) const FONT_WEIGHT: u16 = 1 << 2;
    pub(super) const COLOR: u16 = 1 << 3;
    pub(super) const CURSOR: u16 = 1 << 4;
    pub(super) const TEXT_WRAP: u16 = 1 << 5;
    pub(super) const LINE_HEIGHT: u16 = 1 << 6;
    pub(super) const VERTICAL_ALIGN: u16 = 1 << 7;

    pub(super) fn contains(self, flag: u16) -> bool {
        self.0 & flag != 0
    }

    pub(super) fn insert(&mut self, flag: u16) {
        self.0 |= flag;
    }
}

impl TextIfcOwnedLine {
    pub(crate) fn shifted(mut self, dx: f32, dy: f32) -> Self {
        self.rect.x += dx;
        self.rect.y += dy;
        self.text_rect.x += dx;
        self.text_rect.y += dy;
        for x in &mut self.caret_xs {
            *x += dx;
        }
        self
    }
}

pub struct Text {
    pub(super) position: Position,
    pub(super) size: Size,
    pub(super) layout_override_width: Option<f32>,
    pub(super) layout_override_height: Option<f32>,
    pub(super) content: String,
    pub(super) color: Box<dyn ColorLike>,
    pub(super) font_families: Vec<String>,
    pub(super) font_size: f32,
    pub(super) line_height: f32,
    pub(super) font_weight: u16,
    pub(super) align: InlineIfcAlignment,
    pub(super) opacity: f32,
    pub(super) transform: Transform,
    pub(super) transform_origin: TransformOrigin,
    pub(super) resolved_transform: Option<Mat4>,
    pub(super) auto_width: bool,
    pub(super) auto_height: bool,
    pub(super) text_wrap: TextWrap,
    pub(super) cursor: Cursor,
    /// Effective `vertical-align` for this Text node. Default
    /// `Baseline`; written by parent cascade or explicit prop.
    pub(super) vertical_align: crate::style::VerticalAlign,
    pub(super) layout_cache: TextLayoutCache,
    /// Shaped context installed by the last measure; render and the
    /// hit-test/caret APIs consume this same context.
    pub(super) shaped_context: Option<Arc<InlineFormattingContext>>,
    inline_ifc_owned: Option<Box<TextInlineIfcOwnedState>>,
    pub(super) node_id: u64,
    pub(super) parent_id: Option<u64>,
    pub(super) dirty_flags: super::DirtyFlags,
    pub(super) last_layout_constraints: Option<crate::view::base_component::LayoutConstraints>,
    pub(super) last_layout_placement: Option<crate::view::base_component::LayoutPlacement>,
    pub(super) layout_state: LayoutState,
    // 軌 A #7: per-prop "set explicitly by the author?" flags. Flipped
    // to `true` by the public setters (cold convert and incremental
    // updates both go through setters, so the flags stay accurate).
    // `apply_inherited` (the cascade side of this pair) only writes
    // props whose flag is currently `false`, so explicit authorship
    // always wins over an ancestor's cascade.
    pub(super) explicit_props: TextExplicitProps,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextInlineIfcStyleMetadata {
    pub(crate) font_size: f32,
    pub(crate) line_height: f32,
    pub(crate) font_weight: u16,
    pub(crate) brush: [u8; 4],
    pub(crate) font_families: Vec<String>,
    pub(crate) vertical_align: crate::style::VerticalAlign,
}

pub(crate) use self::profile::{
    TextMeasureProfile, reset_text_measure_profile, set_text_measure_profile_enabled,
    take_text_measure_profile,
};
impl Text {
    pub fn from_content(content: impl Into<String>) -> Self {
        let mut text = Self::new(0.0, 0.0, 10_000.0, 10_000.0, content);
        text.set_auto_width(true);
        text.set_auto_height(true);
        text
    }

    pub fn from_content_with_id(id: u64, content: impl Into<String>) -> Self {
        let mut text = Self::new_with_id(id, 0.0, 0.0, 10_000.0, 10_000.0, content);
        text.set_auto_width(true);
        text.set_auto_height(true);
        text
    }

    pub fn new(x: f32, y: f32, width: f32, height: f32, content: impl Into<String>) -> Self {
        Self::new_with_id(0, x, y, width, height, content)
    }

    pub fn new_with_id(
        id: u64,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        content: impl Into<String>,
    ) -> Self {
        Self {
            node_id: id,
            parent_id: None,
            position: Position { x, y },
            size: Size { width, height },
            layout_override_width: None,
            layout_override_height: None,
            content: content.into(),
            color: Box::new(HexColor::new("#111111")),
            font_families: Vec::new(),
            font_size: 16.0,
            line_height: 1.25,
            font_weight: 400,
            align: InlineIfcAlignment::Left,
            opacity: 1.0,
            transform: Transform::default(),
            transform_origin: TransformOrigin::center(),
            resolved_transform: None,
            auto_width: false,
            auto_height: false,
            text_wrap: TextWrap::Wrap,
            cursor: Cursor::Default,
            vertical_align: crate::style::VerticalAlign::Baseline,
            layout_cache: TextLayoutCache::default(),
            shaped_context: None,
            inline_ifc_owned: None,
            dirty_flags: super::DirtyFlags::ALL,
            last_layout_constraints: None,
            last_layout_placement: None,
            layout_state: LayoutState::new(x, y, width, height),
            explicit_props: TextExplicitProps::default(),
        }
    }

    fn update_resolved_transform(&mut self) {
        self.resolved_transform = self.compute_transform_matrix();
    }

    fn compute_transform_matrix(&self) -> Option<Mat4> {
        if self.transform.as_slice().is_empty() {
            return None;
        }
        let size = self.layout_state.layout_size;
        let origin = Vec3::new(
            self.transform_origin
                .x()
                .resolve_with_base(Some(size.width.max(0.0)), 0.0, 0.0)
                .unwrap_or(0.0),
            self.transform_origin
                .y()
                .resolve_with_base(Some(size.height.max(0.0)), 0.0, 0.0)
                .unwrap_or(0.0),
            self.transform_origin.z(),
        );
        let mut transform = Mat4::IDENTITY;
        for entry in self.transform.as_slice() {
            let next = match entry.kind() {
                TransformKind::Translate { x, y, z } => Mat4::from_translation(Vec3::new(
                    x.resolve_with_base(Some(size.width.max(0.0)), 0.0, 0.0)
                        .unwrap_or(0.0),
                    y.resolve_with_base(Some(size.height.max(0.0)), 0.0, 0.0)
                        .unwrap_or(0.0),
                    z,
                )),
                TransformKind::Scale { x, y, z } => Mat4::from_scale(Vec3::new(x, y, z)),
                TransformKind::Rotate { x, y, z } => {
                    Mat4::from_rotation_x(x.to_radians())
                        * Mat4::from_rotation_y(y.to_radians())
                        * Mat4::from_rotation_z(z.to_radians())
                }
                TransformKind::Perspective { depth } => {
                    text_css_perspective_matrix(depth.max(0.0001))
                }
                TransformKind::Matrix { matrix } => Mat4::from_cols_array(&matrix),
            };
            transform *= next;
        }
        let origin_world = Vec3::new(
            self.layout_state.layout_position.x + origin.x,
            self.layout_state.layout_position.y + origin.y,
            origin.z,
        );
        Some(
            Mat4::from_translation(origin_world)
                * transform
                * Mat4::from_translation(-origin_world),
        )
    }

    fn untransformed_retained_paint_bounds(&self) -> super::RetainedSurfaceBounds {
        let bounds = self
            .inline_ifc_owned_paint_bounds()
            .unwrap_or(crate::ui::Rect {
                x: self.layout_state.layout_position.x,
                y: self.layout_state.layout_position.y,
                width: self.layout_state.layout_size.width,
                height: self.layout_state.layout_size.height,
            });
        super::RetainedSurfaceBounds {
            x: bounds.x,
            y: bounds.y,
            width: bounds.width.max(0.0),
            height: bounds.height.max(0.0),
            corner_radii: [0.0; 4],
        }
    }

    /// Shell placement for a node whose geometry is owned by an inline
    /// IFC root: adopt the bounding box so arena hit-testing and bbox
    /// queries see the fragment union, without running a layout pass.
    pub(crate) fn place_as_inline_ifc_owned_box(&mut self, bounds: crate::ui::Rect) {
        self.layout_state.layout_position = Position {
            x: bounds.x,
            y: bounds.y,
        };
        self.layout_state.layout_flow_position = self.layout_state.layout_position;
        self.layout_state.layout_inner_position = self.layout_state.layout_position;
        self.layout_state.layout_size = Size {
            width: bounds.width,
            height: bounds.height,
        };
        self.layout_state.layout_inner_size = self.layout_state.layout_size;
        self.layout_state.should_render = bounds.width > 0.0 && bounds.height > 0.0;
        self.update_resolved_transform();
        // Mirrors Element::place_as_inline_ifc_owned_box: the install is
        // this node's placement pass, so clear local PLACEMENT dirt here
        // or the subtree aggregate stays dirty every frame.
        self.dirty_flags = self.dirty_flags.without(super::DirtyPassMask::PLACEMENT);
    }

    /// Install per-line geometry from the inline IFC root that owns this
    /// Text node's glyphs. While owned, the Text renders the root-shaped,
    /// source-filtered payload and answers geometry from the installed lines.
    pub(crate) fn install_inline_ifc_owned_geometry(
        &mut self,
        lines: Vec<TextIfcOwnedLine>,
        paint_input: Arc<InlineIfcTextPassPaintInput>,
        paint_bounds: crate::ui::Rect,
    ) {
        let changed = self.inline_ifc_owned.as_deref().is_none_or(|owned| {
            owned.lines.as_slice() != lines.as_slice()
                || !Arc::ptr_eq(&owned.paint_input, &paint_input)
                || owned.paint_bounds != paint_bounds
        });
        if changed {
            self.dirty_flags = self.dirty_flags.union(super::DirtyPassMask::PAINT);
        }
        self.inline_ifc_owned = Some(Box::new(TextInlineIfcOwnedState {
            lines,
            paint_input,
            paint_bounds,
        }));
    }

    /// In-place delta shift of installed owned lines: the owning IFC
    /// root moved without reshaping, so every absolute coordinate moves
    /// by the same delta.
    pub(crate) fn shift_inline_ifc_owned_geometry(&mut self, dx: f32, dy: f32) {
        if let Some(owned) = self.inline_ifc_owned.as_mut() {
            for line in &mut owned.lines {
                line.rect.x += dx;
                line.rect.y += dy;
                line.text_rect.x += dx;
                line.text_rect.y += dy;
                for x in &mut line.caret_xs {
                    *x += dx;
                }
            }
            owned.paint_bounds.x += dx;
            owned.paint_bounds.y += dy;
            // Parity with install_inline_ifc_owned_geometry: changed
            // geometry marks PAINT.
            self.dirty_flags = self.dirty_flags.union(super::DirtyPassMask::PAINT);
        }
        self.layout_state.layout_position.x += dx;
        self.layout_state.layout_position.y += dy;
        self.layout_state.layout_flow_position = self.layout_state.layout_position;
        self.layout_state.layout_inner_position = self.layout_state.layout_position;
        self.update_resolved_transform();
    }

    pub(crate) fn clear_inline_ifc_owned_geometry(&mut self) {
        if self.inline_ifc_owned.take().is_some() {
            self.dirty_flags = self.dirty_flags.union(super::DirtyPassMask::PAINT);
        }
    }

    pub(crate) fn matches_inline_ifc_owned_install(
        &self,
        expected_lines: &[TextIfcOwnedLine],
        expected_paint_input: &InlineIfcTextPassPaintInput,
        expected_paint_bounds: crate::ui::Rect,
        expected_shell_bounds: crate::ui::Rect,
    ) -> bool {
        fn rect_bits_eq(left: crate::ui::Rect, right: crate::ui::Rect) -> bool {
            left.x.to_bits() == right.x.to_bits()
                && left.y.to_bits() == right.y.to_bits()
                && left.width.to_bits() == right.width.to_bits()
                && left.height.to_bits() == right.height.to_bits()
        }

        fn line_bits_eq(left: &TextIfcOwnedLine, right: &TextIfcOwnedLine) -> bool {
            rect_bits_eq(left.rect, right.rect)
                && rect_bits_eq(left.text_rect, right.text_rect)
                && left.char_range == right.char_range
                && left.caret_xs.len() == right.caret_xs.len()
                && left
                    .caret_xs
                    .iter()
                    .zip(&right.caret_xs)
                    .all(|(left, right)| left.to_bits() == right.to_bits())
        }

        let Some(owned) = self.inline_ifc_owned.as_deref() else {
            return false;
        };
        owned.lines.len() == expected_lines.len()
            && owned
                .lines
                .iter()
                .zip(expected_lines)
                .all(|(left, right)| line_bits_eq(left, right))
            && owned.paint_input.as_ref() == expected_paint_input
            && rect_bits_eq(owned.paint_bounds, expected_paint_bounds)
            && self.layout_state.layout_position.x.to_bits() == expected_shell_bounds.x.to_bits()
            && self.layout_state.layout_position.y.to_bits() == expected_shell_bounds.y.to_bits()
            && self.layout_state.layout_flow_position.x.to_bits()
                == expected_shell_bounds.x.to_bits()
            && self.layout_state.layout_flow_position.y.to_bits()
                == expected_shell_bounds.y.to_bits()
            && self.layout_state.layout_inner_position.x.to_bits()
                == expected_shell_bounds.x.to_bits()
            && self.layout_state.layout_inner_position.y.to_bits()
                == expected_shell_bounds.y.to_bits()
            && self.layout_state.layout_size.width.to_bits()
                == expected_shell_bounds.width.to_bits()
            && self.layout_state.layout_size.height.to_bits()
                == expected_shell_bounds.height.to_bits()
            && self.layout_state.layout_inner_size.width.to_bits()
                == expected_shell_bounds.width.to_bits()
            && self.layout_state.layout_inner_size.height.to_bits()
                == expected_shell_bounds.height.to_bits()
            && self.layout_state.should_render
                == (expected_shell_bounds.width > 0.0 && expected_shell_bounds.height > 0.0)
    }

    fn inline_ifc_owned_lines(&self) -> Option<&[TextIfcOwnedLine]> {
        self.inline_ifc_owned
            .as_deref()
            .map(|owned| owned.lines.as_slice())
    }

    fn inline_ifc_owned_paint_input(&self) -> Option<&InlineIfcTextPassPaintInput> {
        self.inline_ifc_owned
            .as_deref()
            .map(|owned| owned.paint_input.as_ref())
    }

    fn inline_ifc_owned_paint_bounds(&self) -> Option<crate::ui::Rect> {
        self.inline_ifc_owned
            .as_deref()
            .map(|owned| owned.paint_bounds)
    }

    #[cfg(test)]
    pub(crate) fn inline_ifc_owned_paint_geometry_for_test(
        &self,
    ) -> Option<(crate::ui::Rect, &InlineIfcTextPassPaintInput)> {
        let owned = self.inline_ifc_owned.as_deref()?;
        Some((owned.paint_bounds, owned.paint_input.as_ref()))
    }

    /// Test observation: per visual line owned by an inline IFC root,
    /// the line's text slice and its absolute origin.
    #[cfg(test)]
    pub(crate) fn inline_fragment_positions(&self) -> Vec<(String, Position)> {
        let Some(lines) = self.inline_ifc_owned_lines() else {
            return Vec::new();
        };
        let chars: Vec<char> = self.content.chars().collect();
        lines
            .iter()
            .map(|line| {
                let start = line.char_range.start.min(chars.len());
                let end = line.char_range.end.min(chars.len());
                let content: String = chars[start..end].iter().collect();
                (
                    content,
                    Position {
                        x: line.text_rect.x,
                        y: line.text_rect.y,
                    },
                )
            })
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn line_height_value(&self) -> f32 {
        self.line_height
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn vertical_align(&self) -> crate::style::VerticalAlign {
        self.vertical_align
    }

    #[cfg(test)]
    pub(crate) fn text_wrap(&self) -> crate::style::TextWrap {
        self.text_wrap
    }

    #[cfg(test)]
    pub(crate) fn set_should_render_for_test(&mut self, should_render: bool) {
        self.layout_state.should_render = should_render;
    }

    #[cfg(test)]
    pub(crate) fn clear_prepared_standalone_text_for_test(&mut self) {
        self.shaped_context = None;
        self.dirty_flags = self.dirty_flags.union(super::DirtyPassMask::PAINT);
    }

    #[cfg(test)]
    pub(crate) fn tamper_layout_position_for_test(&mut self, dx: f32, dy: f32) {
        self.layout_state.layout_position.x += dx;
        self.layout_state.layout_position.y += dy;
    }
}

fn text_css_perspective_matrix(depth: f32) -> Mat4 {
    if depth.abs() <= 0.000_001 {
        return Mat4::IDENTITY;
    }
    Mat4::from_cols(
        Vec4::new(1.0, 0.0, 0.0, 0.0),
        Vec4::new(0.0, 1.0, 0.0, 0.0),
        Vec4::new(0.0, 0.0, 1.0, -1.0 / depth),
        Vec4::new(0.0, 0.0, 0.0, 1.0),
    )
}

#[cfg(test)]
pub(crate) use self::measure::measure_text_size;

struct PreparedShadowTextSelectionPayload {
    bounds: crate::view::base_component::Rect,
    ops: Vec<crate::view::paint::DrawRectOp>,
}

impl Text {
    fn shadow_text_recording_bounds(
        &self,
        owner: crate::view::node_arena::NodeKey,
        mut bounds: crate::view::base_component::Rect,
        recording_context: crate::view::paint::PaintRecordingContext,
    ) -> crate::view::base_component::Rect {
        if recording_context.authorizes_scroll_content_local_owner(owner) {
            bounds.x += recording_context.paint_offset[0];
            bounds.y += recording_context.paint_offset[1];
        }
        bounds
    }

    fn validate_shadow_text_preedit_witness(
        &self,
        owner: crate::view::node_arena::NodeKey,
        arena: &crate::view::node_arena::NodeArena,
        recording_context: crate::view::paint::PaintRecordingContext,
    ) -> Result<(), super::ShadowPaintBlocker> {
        let Some(witness) = recording_context.text_area_preedit else {
            return Ok(());
        };
        let effective_opacity = recording_context.paint_opacity(self.opacity);
        let content_char_count = self.content.chars().count();
        let start_byte = self
            .content
            .char_indices()
            .nth(witness.local_start_char)
            .map(|(byte, _)| byte)
            .unwrap_or(self.content.len());
        let end_byte = self
            .content
            .char_indices()
            .nth(witness.local_end_char)
            .map(|(byte, _)| byte)
            .unwrap_or(self.content.len());
        let caret_char = self
            .content
            .get(..witness.target_caret_byte)
            .map(str::chars)
            .map(Iterator::count);
        if !recording_context.inside_text_area
            || !witness.is_canonical_for(owner, self.node_id)
            || arena.parent_of(owner) != Some(witness.projection_owner)
            || witness.local_end_char > content_char_count
            || start_byte != witness.target_start_byte
            || end_byte != witness.target_end_byte
            || !self.content.is_char_boundary(witness.target_start_byte)
            || !self.content.is_char_boundary(witness.target_end_byte)
            || !self.content.is_char_boundary(witness.target_caret_byte)
            || caret_char != Some(witness.target_caret_char)
            || !self.is_paint_visible(effective_opacity)
        {
            return Err(super::ShadowPaintBlocker::TextAreaSelection);
        }
        Ok(())
    }

    fn prepared_shadow_text_selection_payload(
        &self,
        owner: crate::view::node_arena::NodeKey,
        recording_context: crate::view::paint::PaintRecordingContext,
    ) -> Result<Option<PreparedShadowTextSelectionPayload>, super::ShadowPaintBlocker> {
        let Some(witness) = recording_context.text_area_selection else {
            return Ok(None);
        };
        let owner_matches = witness.target_owner == owner;
        let stable_id_matches = witness.target_stable_id == self.node_id;
        if !owner_matches && !stable_id_matches {
            return Ok(None);
        }
        let effective_opacity = recording_context.paint_opacity(self.opacity);
        if owner_matches && stable_id_matches && !self.is_paint_visible(effective_opacity) {
            return Ok(None);
        }
        if !owner_matches
            || !stable_id_matches
            || !recording_context.inside_text_area
            || !witness.is_canonical_for(owner, self.node_id)
            || witness.local_end > self.content.chars().count()
        {
            return Err(super::ShadowPaintBlocker::TextAreaSelection);
        }

        let rects = self.local_selection_screen_rects(witness.local_start, witness.local_end);
        if rects.is_empty() {
            return Err(super::ShadowPaintBlocker::TextAreaSelection);
        }
        let mut ops = Vec::with_capacity(rects.len());
        let mut left = f32::INFINITY;
        let mut top = f32::INFINITY;
        let mut right = f32::NEG_INFINITY;
        let mut bottom = f32::NEG_INFINITY;
        for rect in rects {
            let params = crate::view::render_pass::draw_rect_pass::RectPassParams {
                position: [
                    rect.x + recording_context.paint_offset[0],
                    rect.y + recording_context.paint_offset[1],
                ],
                size: [rect.width.max(1.0), rect.height.max(1.0)],
                fill_color: witness.fill,
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
                return Err(super::ShadowPaintBlocker::TextAreaSelection);
            }
            left = left.min(params.position[0]);
            top = top.min(params.position[1]);
            right = right.max(params.position[0] + params.size[0]);
            bottom = bottom.max(params.position[1] + params.size[1]);
            ops.push(crate::view::paint::DrawRectOp {
                params,
                mode: crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly,
            });
        }
        if ![left, top, right, bottom].into_iter().all(f32::is_finite)
            || right < left
            || bottom < top
            || crate::view::paint::PaintPayloadIdentity::prepared_rects(ops.iter()).is_none()
        {
            return Err(super::ShadowPaintBlocker::TextAreaSelection);
        }
        Ok(Some(PreparedShadowTextSelectionPayload {
            bounds: crate::view::base_component::Rect {
                x: left,
                y: top,
                width: right - left,
                height: bottom - top,
            },
            ops,
        }))
    }
}

impl ElementTrait for Text {
    fn stable_id(&self) -> u64 {
        self.node_id
    }

    fn retained_scroll_normalized_paint_capability(
        &self,
    ) -> Option<super::RetainedScrollNormalizedPaintCapability> {
        Some(super::RetainedScrollNormalizedPaintCapability::native(
            super::RetainedScrollNormalizedPaintKind::Text,
        ))
    }

    #[allow(private_interfaces)]
    fn shadow_paint_recording_capability(
        &self,
        arena: &crate::view::node_arena::NodeArena,
        _deferred_phase_root: bool,
        recording_context: crate::view::paint::PaintRecordingContext,
    ) -> super::ShadowPaintRecordingCapability {
        let effective_opacity = recording_context.paint_opacity(self.opacity);
        if recording_context.text_area_preedit.is_some_and(|witness| {
            self.validate_shadow_text_preedit_witness(
                witness.target_owner,
                arena,
                recording_context,
            )
            .is_err()
        }) {
            return super::ShadowPaintRecordingCapability::Legacy(
                super::ShadowPaintBlocker::TextAreaSelection,
            );
        }
        // Empty, culled and fully transparent Text has no paint in either
        // renderer. Treat it as a transparent retained leaf in every host
        // context; reporting Recordable with no glyph plan later becomes a
        // spurious MissingPaintIdentity Legacy boundary.
        if !self.is_paint_visible(effective_opacity) {
            return super::ShadowPaintRecordingCapability::Transparent;
        }
        if let Some(witness) = recording_context.text_area_selection
            && witness.target_stable_id == self.node_id
            && self
                .prepared_shadow_text_selection_payload(witness.target_owner, recording_context)
                .is_err()
        {
            return super::ShadowPaintRecordingCapability::Legacy(
                super::ShadowPaintBlocker::TextAreaSelection,
            );
        }
        if let Err(blocker) =
            self.prepared_shadow_text_payload(recording_context.paint_offset, effective_opacity)
        {
            return super::ShadowPaintRecordingCapability::Legacy(blocker);
        }
        super::ShadowPaintRecordingCapability::Recordable
    }

    #[allow(private_interfaces)]
    fn record_shadow_paint_metadata_plan(
        &self,
        owner: crate::view::node_arena::NodeKey,
        properties: crate::view::compositor::property_tree::PropertyTreeState,
        _contents_properties: crate::view::compositor::property_tree::PropertyTreeState,
        content_revision: crate::view::paint::PaintContentRevision,
        arena: &crate::view::node_arena::NodeArena,
        recording_context: crate::view::paint::PaintRecordingContext,
    ) -> Option<crate::view::paint::PaintNodePlan<crate::view::paint::PaintChunkMetadata>> {
        let effective_opacity = recording_context.paint_opacity(self.opacity);
        if recording_context.inside_text_area && !self.is_paint_visible(effective_opacity) {
            return None;
        }
        self.validate_shadow_text_preedit_witness(owner, arena, recording_context)
            .ok()?;
        let glyph = self
            .prepared_shadow_text_payload(recording_context.paint_offset, effective_opacity)
            .ok()?;
        let glyph_bounds =
            self.shadow_text_recording_bounds(owner, glyph.bounds, recording_context);
        let selection = self
            .prepared_shadow_text_selection_payload(owner, recording_context)
            .ok()?;
        let glyph_slot = u16::from(recording_context.inside_text_area);
        let mut before_children = Vec::with_capacity(1 + usize::from(selection.is_some()));
        if let Some(selection) = selection {
            before_children.push(crate::view::paint::PaintChunkMetadata {
                id: crate::view::paint::PaintChunkId {
                    owner,
                    scope: crate::view::paint::PaintPropertyScope::SelfPaint,
                    phase: crate::view::paint::PaintNodePhase::BeforeChildren,
                    slot: 0,
                    role: crate::view::paint::PaintChunkRole::SelectionUnderlay,
                },
                owner,
                bounds: selection.bounds,
                properties,
                content_revision,
                payload_identity: crate::view::paint::PaintPayloadIdentity::prepared_rects(
                    selection.ops.iter(),
                )?,
            });
        }
        before_children.push(crate::view::paint::PaintChunkMetadata {
            id: crate::view::paint::PaintChunkId {
                owner,
                scope: crate::view::paint::PaintPropertyScope::SelfPaint,
                phase: crate::view::paint::PaintNodePhase::BeforeChildren,
                slot: glyph_slot,
                role: crate::view::paint::PaintChunkRole::TextGlyphs,
            },
            owner,
            bounds: glyph_bounds,
            properties,
            content_revision,
            payload_identity: crate::view::paint::PaintPayloadIdentity::prepared_texts(
                glyph.op.iter(),
            ),
        });
        Some(crate::view::paint::PaintNodePlan {
            before_children,
            after_children: Vec::new(),
        })
    }

    #[allow(private_interfaces)]
    fn record_shadow_paint_artifact_plan(
        &self,
        owner: crate::view::node_arena::NodeKey,
        properties: crate::view::compositor::property_tree::PropertyTreeState,
        _contents_properties: crate::view::compositor::property_tree::PropertyTreeState,
        content_revision: crate::view::paint::PaintContentRevision,
        arena: &crate::view::node_arena::NodeArena,
        recording_context: crate::view::paint::PaintRecordingContext,
    ) -> Option<crate::view::paint::PaintNodePlan<crate::view::paint::PaintArtifact>> {
        let effective_opacity = recording_context.paint_opacity(self.opacity);
        if recording_context.inside_text_area && !self.is_paint_visible(effective_opacity) {
            return None;
        }
        self.validate_shadow_text_preedit_witness(owner, arena, recording_context)
            .ok()?;
        let glyph = self
            .prepared_shadow_text_payload(recording_context.paint_offset, effective_opacity)
            .ok()?;
        let glyph_bounds =
            self.shadow_text_recording_bounds(owner, glyph.bounds, recording_context);
        let selection = self
            .prepared_shadow_text_selection_payload(owner, recording_context)
            .ok()?;
        #[cfg(test)]
        crate::view::paint::note_full_artifact_record();
        let glyph_slot = u16::from(recording_context.inside_text_area);
        let mut before_children = Vec::with_capacity(1 + usize::from(selection.is_some()));
        if let Some(selection) = selection {
            let payload_identity =
                crate::view::paint::PaintPayloadIdentity::prepared_rects(selection.ops.iter())?;
            let ops = selection
                .ops
                .into_iter()
                .map(crate::view::paint::PaintOp::DrawRect)
                .collect::<Vec<_>>();
            before_children.push(crate::view::paint::PaintArtifact {
                target: Default::default(),
                chunks: vec![crate::view::paint::PaintChunk {
                    id: crate::view::paint::PaintChunkId {
                        owner,
                        scope: crate::view::paint::PaintPropertyScope::SelfPaint,
                        phase: crate::view::paint::PaintNodePhase::BeforeChildren,
                        slot: 0,
                        role: crate::view::paint::PaintChunkRole::SelectionUnderlay,
                    },
                    owner,
                    op_range: 0..ops.len(),
                    bounds: selection.bounds,
                    properties,
                    content_revision,
                    payload_identity,
                }],
                ops,
                clip_nodes: Vec::new(),
                effect_nodes: Vec::new(),
                owner_nodes: vec![crate::view::paint::PaintOwnerSnapshot {
                    owner,
                    parent: None,
                }],
            });
        }
        let payload_identity =
            crate::view::paint::PaintPayloadIdentity::prepared_texts(glyph.op.iter());
        let ops = glyph
            .op
            .into_iter()
            .map(crate::view::paint::PaintOp::PreparedText)
            .collect::<Vec<_>>();
        before_children.push(crate::view::paint::PaintArtifact {
            target: Default::default(),
            chunks: vec![crate::view::paint::PaintChunk {
                id: crate::view::paint::PaintChunkId {
                    owner,
                    scope: crate::view::paint::PaintPropertyScope::SelfPaint,
                    phase: crate::view::paint::PaintNodePhase::BeforeChildren,
                    slot: glyph_slot,
                    role: crate::view::paint::PaintChunkRole::TextGlyphs,
                },
                owner,
                op_range: 0..ops.len(),
                bounds: glyph_bounds,
                properties,
                content_revision,
                payload_identity,
            }],
            ops,
            clip_nodes: Vec::new(),
            effect_nodes: Vec::new(),
            owner_nodes: vec![crate::view::paint::PaintOwnerSnapshot {
                owner,
                parent: None,
            }],
        });
        Some(crate::view::paint::PaintNodePlan {
            before_children,
            after_children: Vec::new(),
        })
    }

    #[allow(private_interfaces)]
    fn record_shadow_paint_metadata(
        &self,
        owner: crate::view::node_arena::NodeKey,
        properties: crate::view::compositor::property_tree::PropertyTreeState,
        content_revision: crate::view::paint::PaintContentRevision,
        arena: &crate::view::node_arena::NodeArena,
        recording_context: crate::view::paint::PaintRecordingContext,
    ) -> Option<crate::view::paint::PaintChunkMetadata> {
        let effective_opacity = recording_context.paint_opacity(self.opacity);
        if recording_context.inside_text_area && !self.is_paint_visible(effective_opacity) {
            return None;
        }
        self.validate_shadow_text_preedit_witness(owner, arena, recording_context)
            .ok()?;
        let payload = self
            .prepared_shadow_text_payload(recording_context.paint_offset, effective_opacity)
            .ok()?;
        let payload_bounds =
            self.shadow_text_recording_bounds(owner, payload.bounds, recording_context);
        Some(crate::view::paint::PaintChunkMetadata {
            id: crate::view::paint::PaintChunkId {
                owner,
                scope: crate::view::paint::PaintPropertyScope::SelfPaint,
                phase: crate::view::paint::PaintNodePhase::BeforeChildren,
                slot: 0,
                role: crate::view::paint::PaintChunkRole::TextGlyphs,
            },
            owner,
            bounds: payload_bounds,
            properties,
            content_revision,
            payload_identity: crate::view::paint::PaintPayloadIdentity::prepared_texts(
                payload.op.iter(),
            ),
        })
    }

    #[allow(private_interfaces)]
    fn record_shadow_paint_artifact(
        &self,
        owner: crate::view::node_arena::NodeKey,
        properties: crate::view::compositor::property_tree::PropertyTreeState,
        content_revision: crate::view::paint::PaintContentRevision,
        arena: &crate::view::node_arena::NodeArena,
        recording_context: crate::view::paint::PaintRecordingContext,
    ) -> Option<crate::view::paint::PaintArtifact> {
        let effective_opacity = recording_context.paint_opacity(self.opacity);
        if recording_context.inside_text_area && !self.is_paint_visible(effective_opacity) {
            return None;
        }
        self.validate_shadow_text_preedit_witness(owner, arena, recording_context)
            .ok()?;
        let payload = self
            .prepared_shadow_text_payload(recording_context.paint_offset, effective_opacity)
            .ok()?;
        let payload_bounds =
            self.shadow_text_recording_bounds(owner, payload.bounds, recording_context);
        #[cfg(test)]
        crate::view::paint::note_full_artifact_record();
        let payload_identity =
            crate::view::paint::PaintPayloadIdentity::prepared_texts(payload.op.iter());
        let ops = payload
            .op
            .into_iter()
            .map(crate::view::paint::PaintOp::PreparedText)
            .collect::<Vec<_>>();
        Some(crate::view::paint::PaintArtifact {
            target: Default::default(),
            chunks: vec![crate::view::paint::PaintChunk {
                id: crate::view::paint::PaintChunkId {
                    owner,
                    scope: crate::view::paint::PaintPropertyScope::SelfPaint,
                    phase: crate::view::paint::PaintNodePhase::BeforeChildren,
                    slot: 0,
                    role: crate::view::paint::PaintChunkRole::TextGlyphs,
                },
                owner,
                op_range: 0..ops.len(),
                bounds: payload_bounds,
                properties,
                content_revision,
                payload_identity,
            }],
            ops,
            clip_nodes: Vec::new(),
            effect_nodes: Vec::new(),
            owner_nodes: vec![crate::view::paint::PaintOwnerSnapshot {
                owner,
                parent: None,
            }],
        })
    }

    fn box_model_snapshot(&self) -> BoxModelSnapshot {
        BoxModelSnapshot {
            node_id: self.node_id,
            parent_id: self.parent_id,
            x: self.layout_state.layout_position.x,
            y: self.layout_state.layout_position.y,
            width: self.layout_state.layout_size.width,
            height: self.layout_state.layout_size.height,
            border_radius: 0.0,
            should_render: self.layout_state.should_render,
        }
    }

    fn placement_eligibility_metadata(
        &self,
    ) -> crate::view::node_arena::PlacementEligibilityMetadata {
        // A standalone Text leaf renders its glyphs relative to
        // `layout_position` at draw time (see `text/render.rs`), so a pure
        // ancestor move is correctly handled by `translate_in_place`. An
        // inline-IFC-owned Text installs absolute glyph boxes instead, but
        // such a Text always sits under an inline Element root that is
        // itself non-translatable, so the whole subtree falls back there.
        crate::view::node_arena::PlacementEligibilityMetadata::empty()
    }

    fn last_placement(&self) -> Option<crate::view::base_component::LayoutPlacement> {
        self.last_layout_placement
    }

    fn translate_in_place(&mut self, dx: f32, dy: f32) {
        let shift = |p: &mut crate::view::base_component::Position| {
            p.x += dx;
            p.y += dy;
        };
        shift(&mut self.layout_state.layout_position);
        shift(&mut self.layout_state.layout_inner_position);
        shift(&mut self.layout_state.layout_flow_position);
        shift(&mut self.layout_state.layout_flow_inner_position);
        if let Some(placement) = self.last_layout_placement.as_mut() {
            placement.parent_x += dx;
            placement.parent_y += dy;
        }
        self.update_resolved_transform();
    }

    fn parent_id(&self) -> Option<u64> {
        self.parent_id
    }

    fn set_parent_id(&mut self, parent_id: Option<u64>) {
        self.parent_id = parent_id;
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    // Phase B: snapshot_state / restore_state removed (see ElementTrait def).

    fn retained_paint_properties(&self) -> super::RetainedPaintProperties {
        super::RetainedPaintProperties {
            opacity: self.opacity,
            ..Default::default()
        }
    }

    fn admits_exact_retained_root_opacity_artifact(&self) -> bool {
        true
    }

    fn retained_transform_output_bounds(
        &self,
        arena: &crate::view::node_arena::NodeArena,
        paint_offset: [f32; 2],
    ) -> Option<super::RetainedSurfaceBounds> {
        if let Some(matrix) = self.resolved_transform {
            let source_bounds = self.retained_transform_surface_bounds(arena, paint_offset)?;
            let visual_bounds =
                crate::view::viewport::scene_helpers::paint_snapped_retained_surface_bounds(
                    self,
                    source_bounds,
                    paint_offset,
                );
            return super::TransformSurfaceGeometrySnapshot::new(
                source_bounds,
                visual_bounds,
                matrix,
                None,
            )?
            .quad_aabb();
        }
        let mut bounds = self.untransformed_retained_paint_bounds();
        if paint_offset.iter().any(|value| !value.is_finite()) {
            return None;
        }
        bounds.x += paint_offset[0];
        bounds.y += paint_offset[1];
        (bounds.x.is_finite()
            && bounds.y.is_finite()
            && bounds.width.is_finite()
            && bounds.height.is_finite())
        .then_some(bounds)
    }

    fn exact_nested_isolation_render_output_bounds(
        &self,
        owner: crate::view::node_arena::NodeKey,
        arena: &crate::view::node_arena::NodeArena,
        parent_snapped_paint_offset: [f32; 2],
    ) -> Option<super::RetainedSurfaceBounds> {
        super::exact_native_nested_isolation_render_output_bounds(
            self,
            owner,
            arena,
            parent_snapped_paint_offset,
        )
    }

    fn legacy_transform_output_bounds(
        &self,
        arena: &crate::view::node_arena::NodeArena,
        paint_offset: [f32; 2],
    ) -> Option<super::RetainedSurfaceBounds> {
        self.retained_transform_output_bounds(arena, paint_offset)
    }

    fn has_active_animator(&self) -> bool {
        false
    }

    fn retained_paint_signature(&self) -> u64 {
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
        self.content.hash(&mut hasher);
        self.color.to_rgba_u8().hash(&mut hasher);
        self.font_families.hash(&mut hasher);
        self.font_size.to_bits().hash(&mut hasher);
        self.line_height.to_bits().hash(&mut hasher);
        self.font_weight.hash(&mut hasher);
        self.align.hash(&mut hasher);
        self.resolved_transform.is_some().hash(&mut hasher);
        if let Some(matrix) = self.resolved_transform {
            for value in matrix.to_cols_array() {
                value.to_bits().hash(&mut hasher);
            }
        }
        (self.text_wrap == TextWrap::Wrap).hash(&mut hasher);
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
        let owned_lines = self.inline_ifc_owned_lines().unwrap_or(&[]);
        for line in owned_lines {
            line.rect.x.to_bits().hash(&mut hasher);
            line.rect.y.to_bits().hash(&mut hasher);
            line.rect.width.to_bits().hash(&mut hasher);
            line.rect.height.to_bits().hash(&mut hasher);
            line.char_range.start.hash(&mut hasher);
            line.char_range.end.hash(&mut hasher);
        }
        if let Some(bounds) = self.inline_ifc_owned_paint_bounds() {
            bounds.x.to_bits().hash(&mut hasher);
            bounds.y.to_bits().hash(&mut hasher);
            bounds.width.to_bits().hash(&mut hasher);
            bounds.height.to_bits().hash(&mut hasher);
        }
        hasher.finish()
    }

    fn retained_paint_signature_is_complete(&self) -> bool {
        true
    }

    fn retained_transform_surface_bounds(
        &self,
        _arena: &crate::view::node_arena::NodeArena,
        _paint_offset: [f32; 2],
    ) -> Option<super::RetainedSurfaceBounds> {
        self.resolved_transform
            .map(|_| self.untransformed_retained_paint_bounds())
    }

    fn retained_transform_raster_seed_bounds(&self) -> Option<super::RetainedSurfaceBounds> {
        Some(self.untransformed_retained_paint_bounds())
    }

    fn has_retained_transform_surface(&self) -> bool {
        self.resolved_transform.is_some()
    }

    fn compositor_viewport_transform_snapshot(&self) -> Option<super::ViewportTransformSnapshot> {
        self.resolved_transform
            .map(super::ViewportTransformSnapshot::from_matrix)
    }

    fn local_dirty_flags(&self) -> super::DirtyFlags {
        self.dirty_flags
    }

    fn clear_local_dirty_flags(&mut self, flags: super::DirtyFlags) {
        self.dirty_flags = self.dirty_flags.without(flags);
    }

    fn apply_inherited(&mut self, inherited: &crate::view::renderer_adapter::StyleCascadeContext) {
        Text::apply_inherited(self, inherited);
    }

    fn build_children(
        &self,
        _node: &crate::ui::RsxElementNode,
        _path: &[u64],
        _global_path: Option<&crate::view::renderer_adapter::GlobalNodePath>,
        _inherited: &crate::view::renderer_adapter::StyleCascadeContext,
    ) -> Result<Vec<crate::view::renderer_adapter::ElementDescriptor>, String> {
        // Text is a descriptor leaf: its RSX children collapse into
        // the host's String content (assembled by the cold path /
        // schema render via `append_text_children`).
        Ok(Vec::new())
    }

    fn ingest_props(&mut self, node: &crate::ui::RsxElementNode) -> Result<(), String> {
        Text::ingest_props_impl(self, node)
    }

    fn apply_prop(
        &mut self,
        arena: &mut crate::view::node_arena::NodeArena,
        self_key: crate::view::node_arena::NodeKey,
        ctx: &crate::view::fiber_work::ApplyContext<'_>,
        name: &'static str,
        value: crate::ui::PropValue,
    ) -> crate::view::fiber_work::PropApplyOutcome {
        Text::apply_prop_impl(self, arena, self_key, ctx, name, value)
    }

    fn reset_prop(
        &mut self,
        arena: &mut crate::view::node_arena::NodeArena,
        self_key: crate::view::node_arena::NodeKey,
        ctx: &crate::view::fiber_work::ApplyContext<'_>,
        name: &'static str,
    ) -> crate::view::fiber_work::PropApplyOutcome {
        Text::reset_prop_impl(self, arena, self_key, ctx, name)
    }
}
