use rustc_hash::FxHashMap;
use std::sync::Arc;

use crate::style::{ColorLike, Cursor, HexColor, TextWrap};
use cosmic_text::{Align, Buffer as GlyphBuffer};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use super::{
    BoxModelSnapshot, ElementTrait, InlineMeasureContext,
    Position, Size,
};
use crate::view::layout::LayoutState;
use crate::view::promotion::PromotionNodeInfo;

mod cache;
mod events;
mod hit_test;
mod inline_plan;
mod layout;
mod measure;
mod profile;
mod props;
mod render;
mod style;

#[cfg(test)]
mod tests;

use self::cache::{
    FirstLineLayoutCacheEntry, FirstLineLayoutCacheKey, InlinePlanCacheKey,
    MeasuredTextLayout, TextLayoutCacheKey, WrappedSuffixCacheKey,
};

use self::inline_plan::{InlineTextFragment, InlineTextPlan};

pub(crate) use self::hit_test::{with_text_area_selection_render_context, TextAreaSelectionRenderContext};

pub struct Text {
    pub(super) position: Position,
    pub(super) size: Size,
    pub(super) render_size: Size,
    pub(super) layout_override_width: Option<f32>,
    pub(super) layout_override_height: Option<f32>,
    pub(super) content: String,
    pub(super) color: Box<dyn ColorLike>,
    pub(super) font_families: Vec<String>,
    pub(super) font_size: f32,
    pub(super) line_height: f32,
    pub(super) font_weight: u16,
    pub(super) align: Align,
    pub(super) opacity: f32,
    pub(super) auto_width: bool,
    pub(super) auto_height: bool,
    pub(super) text_wrap: TextWrap,
    pub(super) cursor: Cursor,
    /// Effective `vertical-align` for this Text node. Default
    /// `Baseline`; written by parent cascade or explicit prop. Read by
    /// `get_inline_nodes_size` to fan out into the inline solver.
    pub(super) vertical_align: crate::style::VerticalAlign,
    pub(super) allow_wrap: bool,
    pub(super) measure_revision: u64,
    pub(super) cached_intrinsic_layout: Option<(u64, MeasuredTextLayout)>,
    pub(super) cached_height_for_width: Option<(u64, f32, f32)>,
    pub(super) layout_cache: FxHashMap<TextLayoutCacheKey, MeasuredTextLayout>,
    pub(super) inline_plan_cache: FxHashMap<InlinePlanCacheKey, InlineTextPlan>,
    pub(super) first_line_fragment_cache: FxHashMap<FirstLineLayoutCacheKey, FirstLineLayoutCacheEntry>,
    pub(super) wrapped_suffix_cache: FxHashMap<WrappedSuffixCacheKey, Vec<InlineTextFragment>>,
    pub(super) layout_buffer: Option<Arc<GlyphBuffer>>,
    pub(super) inline_plan: Option<InlineTextPlan>,
    pub(super) last_inline_measure_context: Option<InlineMeasureContext>,
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
    pub(super) font_family_explicit: bool,
    pub(super) font_size_explicit: bool,
    pub(super) font_weight_explicit: bool,
    pub(super) color_explicit: bool,
    pub(super) cursor_explicit: bool,
    pub(super) text_wrap_explicit: bool,
    pub(super) line_height_explicit: bool,
}

pub(crate) use self::profile::{
    reset_text_measure_profile, set_text_measure_profile_enabled, take_text_measure_profile,
    TextMeasureProfile,
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
            render_size: Size { width, height },
            layout_override_width: None,
            layout_override_height: None,
            content: content.into(),
            color: Box::new(HexColor::new("#111111")),
            font_families: Vec::new(),
            font_size: 16.0,
            line_height: 1.25,
            font_weight: 400,
            align: Align::Left,
            opacity: 1.0,
            auto_width: false,
            auto_height: false,
            text_wrap: TextWrap::Wrap,
            cursor: Cursor::Default,
            vertical_align: crate::style::VerticalAlign::Baseline,
            allow_wrap: true,
            measure_revision: 0,
            cached_intrinsic_layout: None,
            cached_height_for_width: None,
            layout_cache: FxHashMap::default(),
            inline_plan_cache: FxHashMap::default(),
            first_line_fragment_cache: FxHashMap::default(),
            wrapped_suffix_cache: FxHashMap::default(),
            layout_buffer: None,
            inline_plan: None,
            last_inline_measure_context: None,
            dirty_flags: super::DirtyFlags::ALL,
            last_layout_placement: None,
            layout_state: LayoutState::new(x, y, width, height),
            font_family_explicit: false,
            font_size_explicit: false,
            font_weight_explicit: false,
            color_explicit: false,
            cursor_explicit: false,
            text_wrap_explicit: false,
            line_height_explicit: false,
        }
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

use self::measure::measure_text_layout;
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

    fn has_active_animator(&self) -> bool {
        false
    }

    fn promotion_self_signature(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.layout_state.should_render.hash(&mut hasher);
        self.layout_state.layout_position.x.to_bits().hash(&mut hasher);
        self.layout_state.layout_position.y.to_bits().hash(&mut hasher);
        self.content.hash(&mut hasher);
        self.color.to_rgba_u8().hash(&mut hasher);
        self.font_families.hash(&mut hasher);
        self.font_size.to_bits().hash(&mut hasher);
        self.line_height.to_bits().hash(&mut hasher);
        self.font_weight.hash(&mut hasher);
        std::mem::discriminant(&self.align).hash(&mut hasher);
        self.allow_wrap.hash(&mut hasher);
        self.layout_state.layout_size.width.max(0.0).to_bits().hash(&mut hasher);
        self.layout_state.layout_size.height.max(0.0).to_bits().hash(&mut hasher);
        let inline_runs = self
            .inline_plan
            .as_ref()
            .map(|plan| plan.runs.as_slice())
            .unwrap_or(&[]);
        for fragment in inline_runs {
            fragment.content.hash(&mut hasher);
            fragment.width.to_bits().hash(&mut hasher);
            fragment.height.to_bits().hash(&mut hasher);
            if let Some(position) = fragment.position {
                position.x.to_bits().hash(&mut hasher);
                position.y.to_bits().hash(&mut hasher);
            }
        }
        hasher.finish()
    }

    fn local_dirty_flags(&self) -> super::DirtyFlags {
        self.dirty_flags
    }

    fn clear_local_dirty_flags(&mut self, flags: super::DirtyFlags) {
        self.dirty_flags = self.dirty_flags.without(flags);
    }

    fn apply_inherited(&mut self, inherited: &crate::view::renderer_adapter::InheritedTextStyle) {
        Text::apply_inherited(self, inherited);
    }

    fn build_children(
        &self,
        _node: &crate::ui::RsxElementNode,
        _path: &[u64],
        _global_path: Option<&crate::view::renderer_adapter::GlobalNodePath>,
        _inherited: &crate::view::renderer_adapter::InheritedTextStyle,
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

