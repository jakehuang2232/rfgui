//! `TextAreaProjectionSegment` — transparent inline-flow host that wraps
//! a single user projection in a TextArea's inline child list.
//!
//! Each segment carries the source content `char_range` it covers
//! (used by TextArea for hit-test, selection, caret, IME routing) and an
//! optional `TextAreaImeContext` written by TextArea's pre-measure
//! routing step (used by descendant `<Text>` elements to render IME
//! preedit at arena-walk time, replacing the v1 RSX-time
//! `<Provider<TextAreaImeContext>>` mechanism).
//!
//! Layout: forwards inline measure / place to its children, reporting
//! the children's content size as its own measured size. Renderable is a
//! no-op — children draw themselves; the segment is structurally
//! invisible.

use std::ops::Range;

use crate::style::{
    Align, CrossSize, FlowDirection, JustifyContent, Layout, Length, VerticalAlign,
};
use crate::view::base_component::{
    BoxModelSnapshot, BuildState, DirtyFlags, ElementTrait, EventTarget, InlineMeasureContext,
    InlineNodeSize, InlinePlacement, LayoutConstraints, LayoutPlacement, Layoutable, Position,
    Rect, Renderable, Size, UiBuildContext,
};
use crate::view::frame_graph::FrameGraph;
use crate::view::layout::inline_fragment::{PlaceInlineFragmentInputs, place_inline_fragment};
use crate::view::layout::measure::{MeasureAxisInputs, measure_axis};
use crate::view::layout::place::{PlaceAxisChildrenInputs, place_axis_children};
use crate::view::layout::{FlexLayoutInfo, LayoutState};
use crate::view::node_arena::NodeKey;

use super::ime_context::TextAreaImeContext;
use super::next_ui_node_id;

/// Host element backing the `<TextAreaProjectionSegment>` RSX tag.
///
/// `pub(crate)` — emitted exclusively by the `<TextArea>` schema render
/// when slicing content into Plain / Projection segments. User code
/// should never construct or reference this directly.
pub(crate) struct TextAreaProjectionSegment {
    char_range: Range<usize>,
    children: Vec<NodeKey>,
    /// Set by TextArea's pre-measure IME routing when the caret falls
    /// inside this segment's char range. Read by descendant `<Text>`
    /// elements via arena ancestor walk during their measure/shape.
    ime_context: Option<TextAreaImeContext>,
    flow_offset: Position,
    layout_state: LayoutState,
    flex_info: Option<FlexLayoutInfo>,
    vertical_align: VerticalAlign,
    inline_paint_fragments: Vec<Rect>,
    dirty_flags: DirtyFlags,
    node_id: u64,
    parent_id: Option<u64>,
}

impl Default for TextAreaProjectionSegment {
    fn default() -> Self {
        Self {
            char_range: 0..0,
            children: Vec::new(),
            ime_context: None,
            flow_offset: Position { x: 0.0, y: 0.0 },
            layout_state: LayoutState::new(0.0, 0.0, 0.0, 0.0),
            flex_info: None,
            vertical_align: VerticalAlign::Baseline,
            inline_paint_fragments: Vec::new(),
            dirty_flags: DirtyFlags::ALL,
            node_id: next_ui_node_id(),
            parent_id: None,
        }
    }
}

impl TextAreaProjectionSegment {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn with_stable_id(node_id: u64) -> Self {
        Self {
            node_id,
            ..Self::default()
        }
    }

    pub(crate) fn char_range(&self) -> Range<usize> {
        self.char_range.clone()
    }

    pub(crate) fn set_char_range(&mut self, range: Range<usize>) {
        self.char_range = range;
    }

    pub(crate) fn ime_context(&self) -> Option<&TextAreaImeContext> {
        self.ime_context.as_ref()
    }

    pub(crate) fn set_ime_context(&mut self, ctx: Option<TextAreaImeContext>) {
        self.ime_context = ctx;
    }

    pub(crate) fn set_vertical_align(&mut self, vertical_align: VerticalAlign) {
        if self.vertical_align == vertical_align {
            return;
        }
        self.vertical_align = vertical_align;
        self.dirty_flags = self.dirty_flags.union(DirtyFlags::LAYOUT);
    }

    fn measure_with_inline_first_width(
        &mut self,
        constraints: LayoutConstraints,
        inline_first_available_width: f32,
        arena: &mut crate::view::node_arena::NodeArena,
    ) {
        let inner_width = constraints.max_width.max(0.0);
        let absolute_mask = vec![false; self.children.len()];
        let outputs = measure_axis(
            MeasureAxisInputs {
                layout: Layout::Inline,
                children: &self.children,
                absolute_mask: &absolute_mask,
                is_row: true,
                is_real_flex: false,
                solver_wrap: true,
                solver_gap: 0.0,
                main_limit: inner_width,
                inner_width,
                child_available_width: inner_width,
                child_available_height: constraints.max_height.max(0.0),
                child_percent_base_width: constraints.percent_base_width,
                child_percent_base_height: constraints.percent_base_height,
                viewport_width: constraints.viewport_width,
                viewport_height: constraints.viewport_height,
                inline_wrap: true,
                inline_gap: 0.0,
                inline_first_available_width: Some(inline_first_available_width.max(0.0)),
            },
            arena,
        );
        self.layout_state.content_size = outputs.content_size;
        self.layout_state.layout_size = Size {
            width: outputs.content_size.width,
            height: outputs.content_size.height,
        };
        self.layout_state.layout_inner_size = self.layout_state.layout_size;
        self.flex_info = Some(outputs.flex_info);
    }
}

impl Layoutable for TextAreaProjectionSegment {
    fn measure(
        &mut self,
        constraints: LayoutConstraints,
        arena: &mut crate::view::node_arena::NodeArena,
    ) {
        self.measure_with_inline_first_width(constraints, constraints.max_width, arena);
    }

    fn place(
        &mut self,
        placement: LayoutPlacement,
        arena: &mut crate::view::node_arena::NodeArena,
    ) {
        let x = placement.parent_x + placement.visual_offset_x + self.flow_offset.x;
        let y = placement.parent_y + placement.visual_offset_y + self.flow_offset.y;
        self.layout_state.layout_position = Position { x, y };
        self.layout_state.layout_inner_position = Position { x, y };
        self.layout_state.layout_inner_size = self.layout_state.layout_size;
        self.layout_state.layout_flow_position = self.layout_state.layout_position;
        self.layout_state.layout_flow_inner_position = self.layout_state.layout_inner_position;

        let Some(info) = self.flex_info.clone() else {
            return;
        };
        place_axis_children(
            PlaceAxisChildrenInputs {
                layout: Layout::Inline,
                children: &self.children,
                flex_info: info,
                is_row: true,
                gap: 0.0,
                main_limit: placement.available_width,
                cross_limit: placement.available_height,
                origin_x: x,
                origin_y: y,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                child_available_width: placement.available_width,
                child_available_height: placement.available_height,
                viewport_width: placement.viewport_width,
                viewport_height: placement.viewport_height,
                child_percent_base_width: placement.percent_base_width,
                child_percent_base_height: placement.percent_base_height,
                align: Align::Start,
                justify_content: JustifyContent::Start,
                cross_size: CrossSize::Fit,
            },
            arena,
        );
    }

    fn measure_inline(
        &mut self,
        context: InlineMeasureContext,
        arena: &mut crate::view::node_arena::NodeArena,
    ) {
        self.measure_with_inline_first_width(
            LayoutConstraints {
                max_width: context.full_available_width.max(0.0),
                max_height: 1_000_000.0,
                viewport_width: context.viewport_width,
                viewport_height: context.viewport_height,
                percent_base_width: context.percent_base_width,
                percent_base_height: context.percent_base_height,
            },
            context.first_available_width,
            arena,
        );
    }

    fn measured_size(&self) -> (f32, f32) {
        (
            self.layout_state.layout_size.width,
            self.layout_state.layout_size.height,
        )
    }

    fn set_layout_width(&mut self, width: f32) {
        self.layout_state.layout_size.width = width.max(0.0);
    }

    fn set_layout_height(&mut self, height: f32) {
        self.layout_state.layout_size.height = height.max(0.0);
    }

    fn get_inline_nodes_size(
        &self,
        _arena: &crate::view::node_arena::NodeArena,
    ) -> Vec<InlineNodeSize> {
        let vertical_align = self.vertical_align;
        let Some(info) = self.flex_info.as_ref() else {
            let (width, height) = self.measured_size();
            return vec![InlineNodeSize {
                width,
                height,
                baseline: height,
                vertical_align,
                ..Default::default()
            }];
        };
        info.lines
            .iter()
            .enumerate()
            .map(|(line_idx, _)| {
                let baseline = info.line_ascent.get(line_idx).copied().unwrap_or(0.0);
                let descent = info.line_descent.get(line_idx).copied().unwrap_or(0.0);
                let last = info.lines.len().saturating_sub(1);
                InlineNodeSize {
                    width: info
                        .line_main_sum
                        .get(line_idx)
                        .copied()
                        .unwrap_or(0.0)
                        .max(0.0),
                    height: (baseline + descent).max(0.0),
                    baseline,
                    vertical_align,
                    force_break_after: line_idx < last,
                    ..Default::default()
                }
            })
            .collect()
    }

    fn place_inline(
        &mut self,
        placement: InlinePlacement,
        arena: &mut crate::view::node_arena::NodeArena,
    ) {
        let absolute_mask = vec![false; self.children.len()];
        place_inline_fragment(
            PlaceInlineFragmentInputs {
                placement,
                children: &self.children,
                absolute_mask: &absolute_mask,
                flex_info: self.flex_info.as_ref(),
                left_inset: 0.0,
                right_inset: 0.0,
                top_inset: 0.0,
                bottom_inset: 0.0,
                gap_length: Length::px(0.0),
                direction: FlowDirection::Row,
                align: Align::Start,
            },
            &mut self.layout_state,
            &mut self.inline_paint_fragments,
            arena,
        );
        self.flow_offset = Position {
            x: self.layout_state.layout_position.x - placement.parent_x - placement.visual_offset_x,
            y: self.layout_state.layout_position.y - placement.parent_y - placement.visual_offset_y,
        };
        self.dirty_flags = self.dirty_flags.without(
            DirtyFlags::PLACE
                .union(DirtyFlags::BOX_MODEL)
                .union(DirtyFlags::HIT_TEST),
        );
    }

    fn set_layout_offset(&mut self, x: f32, y: f32) {
        self.flow_offset = Position { x, y };
    }

    fn inline_relative_position(&self) -> (f32, f32) {
        (self.flow_offset.x, self.flow_offset.y)
    }
}

impl Renderable for TextAreaProjectionSegment {
    fn build(
        &mut self,
        graph: &mut FrameGraph,
        arena: &mut crate::view::node_arena::NodeArena,
        mut ctx: UiBuildContext,
    ) -> BuildState {
        // Transparent: no self paint. TextArea calls build() only on its
        // direct children, so this wrapper must explicitly forward rendering
        // into the projection subtree.
        let child_keys = self.children.clone();
        for child_key in child_keys {
            let viewport = ctx.viewport();
            let taken_state = ctx.state_clone();
            let ctx_in = UiBuildContext::from_parts(viewport.clone(), taken_state);
            if let Some(next_state) = arena.with_element_taken(child_key, |child, arena| {
                let next_state = child.build(graph, arena, ctx_in);
                UiBuildContext::from_parts(viewport, next_state)
            }) {
                ctx = next_state;
            }
        }
        ctx.into_state()
    }
}

impl EventTarget for TextAreaProjectionSegment {
    // Transparent: events pass through to children.
}

impl ElementTrait for TextAreaProjectionSegment {
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

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn children(&self) -> &[NodeKey] {
        &self.children
    }

    fn children_mut(&mut self) -> Option<&mut Vec<NodeKey>> {
        Some(&mut self.children)
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

    fn promotion_node_info(&self) -> crate::view::promotion::PromotionNodeInfo {
        crate::view::promotion::PromotionNodeInfo::default()
    }

    fn apply_prop(
        &mut self,
        _arena: &mut crate::view::node_arena::NodeArena,
        _self_key: crate::view::node_arena::NodeKey,
        _ctx: &crate::view::fiber_work::ApplyContext<'_>,
        name: &'static str,
        value: crate::ui::PropValue,
    ) -> crate::view::fiber_work::PropApplyOutcome {
        use crate::view::fiber_work::PropApplyOutcome;
        match name {
            "key" => PropApplyOutcome::Applied,
            "char_range_start" => {
                let v = match &value {
                    crate::ui::PropValue::I64(i) => (*i).max(0) as usize,
                    crate::ui::PropValue::F64(f) => (*f).max(0.0) as usize,
                    _ => return PropApplyOutcome::DecodeFailed(name),
                };
                self.char_range.start = v;
                PropApplyOutcome::Applied
            }
            "char_range_end" => {
                let v = match &value {
                    crate::ui::PropValue::I64(i) => (*i).max(0) as usize,
                    crate::ui::PropValue::F64(f) => (*f).max(0.0) as usize,
                    _ => return PropApplyOutcome::DecodeFailed(name),
                };
                self.char_range.end = v;
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
            "char_range_start" => {
                self.char_range.start = 0;
                PropApplyOutcome::Applied
            }
            "char_range_end" => {
                self.char_range.end = 0;
                PropApplyOutcome::Applied
            }
            _ => PropApplyOutcome::UnknownProp,
        }
    }
}
