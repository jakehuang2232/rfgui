use std::sync::Arc;

use crate::style::{ColorLike, Cursor, HexColor, TextWrap};
use crate::view::inline_formatting_context::{
    InlineFormattingContext, InlineIfcAlignment, InlineIfcTextPassPaintInput,
};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use super::{BoxModelSnapshot, ElementTrait, Position, Size};
use crate::view::layout::LayoutState;
use crate::view::promotion::PromotionNodeInfo;

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
            auto_width: false,
            auto_height: false,
            text_wrap: TextWrap::Wrap,
            cursor: Cursor::Default,
            vertical_align: crate::style::VerticalAlign::Baseline,
            layout_cache: TextLayoutCache::default(),
            shaped_context: None,
            inline_ifc_owned: None,
            dirty_flags: super::DirtyFlags::ALL,
            last_layout_placement: None,
            layout_state: LayoutState::new(x, y, width, height),
            explicit_props: TextExplicitProps::default(),
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
    }

    pub(crate) fn clear_inline_ifc_owned_geometry(&mut self) {
        if self.inline_ifc_owned.take().is_some() {
            self.dirty_flags = self.dirty_flags.union(super::DirtyPassMask::PAINT);
        }
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
}

#[cfg(test)]
pub(crate) use self::measure::measure_text_size;

impl ElementTrait for Text {
    fn stable_id(&self) -> u64 {
        self.node_id
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

    fn promotion_node_info(&self) -> PromotionNodeInfo {
        PromotionNodeInfo {
            estimated_pass_count: 1,
            opacity: self.opacity,
            ..Default::default()
        }
    }

    fn promotion_composite_bounds(&self) -> super::PromotionCompositeBounds {
        let bounds = self
            .inline_ifc_owned_paint_bounds()
            .unwrap_or(crate::ui::Rect {
                x: self.layout_state.layout_position.x,
                y: self.layout_state.layout_position.y,
                width: self.layout_state.layout_size.width,
                height: self.layout_state.layout_size.height,
            });
        super::PromotionCompositeBounds {
            x: bounds.x,
            y: bounds.y,
            width: bounds.width.max(0.0),
            height: bounds.height.max(0.0),
            corner_radii: [0.0; 4],
        }
    }

    fn has_active_animator(&self) -> bool {
        false
    }

    fn promotion_self_signature(&self) -> u64 {
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
