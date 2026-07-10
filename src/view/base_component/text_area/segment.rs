//! `TextAreaProjectionSegment` — transparent inline-flow host that wraps
//! a single user projection in a TextArea's inline child list.
//!
//! Each segment carries the source content `char_range` it covers, used by
//! TextArea for hit-test, selection, caret, and IME routing. Projection IME
//! state is supplied to descendants by the stable
//! `<Provider<TextAreaImeContext>>` wrapper built in `projection.rs`.
//!
//! Layout: forwards measure / place to its children, reporting the
//! children's content size as its own measured size. Renderable is a no-op
//! — children draw themselves; the segment is structurally invisible.

use std::ops::Range;

use crate::style::VerticalAlign;
use crate::view::base_component::{
    BoxModelSnapshot, BuildState, DirtyFlags, ElementTrait, EventTarget, LayoutConstraints,
    LayoutPlacement, Layoutable, Position, Rect, Renderable, Size, UiBuildContext,
};
use crate::view::frame_graph::FrameGraph;
use crate::view::layout::{FlexLayoutInfo, LayoutState};
use crate::view::node_arena::NodeKey;

use super::next_ui_node_id;

/// Host element backing the `<TextAreaProjectionSegment>` RSX tag.
///
/// `pub(crate)` — emitted exclusively by the `<TextArea>` schema render
/// when slicing content into Plain / Projection segments. User code
/// should never construct or reference this directly.
pub(crate) struct TextAreaProjectionSegment {
    char_range: Range<usize>,
    children: Vec<NodeKey>,
    flow_offset: Position,
    layout_state: LayoutState,
    flex_info: Option<FlexLayoutInfo>,
    vertical_align: VerticalAlign,
    owner_inline_baseline: f32,
    auto_wrap: bool,
    inline_full_available_width: f32,
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
            flow_offset: Position { x: 0.0, y: 0.0 },
            layout_state: LayoutState::new(0.0, 0.0, 0.0, 0.0),
            flex_info: None,
            vertical_align: VerticalAlign::Baseline,
            owner_inline_baseline: 0.0,
            auto_wrap: true,
            inline_full_available_width: 0.0,
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

    pub(crate) fn set_vertical_align(&mut self, vertical_align: VerticalAlign) {
        if self.vertical_align == vertical_align {
            return;
        }
        self.vertical_align = vertical_align;
        self.dirty_flags = self.dirty_flags.union(DirtyFlags::LAYOUT);
    }

    pub(crate) fn set_owner_inline_baseline(&mut self, font_size: f32, _line_height: f32) {
        let font_size = font_size.max(1.0);
        let baseline = (font_size * 0.875).max(0.0);
        if (self.owner_inline_baseline - baseline).abs() <= f32::EPSILON {
            return;
        }
        self.owner_inline_baseline = baseline;
        self.dirty_flags = self.dirty_flags.union(DirtyFlags::LAYOUT);
    }

    pub(crate) fn set_auto_wrap(&mut self, auto_wrap: bool) {
        if self.auto_wrap == auto_wrap {
            return;
        }
        self.auto_wrap = auto_wrap;
        self.dirty_flags = self.dirty_flags.union(DirtyFlags::LAYOUT);
    }

    fn measure_with_inline_first_width(
        &mut self,
        constraints: LayoutConstraints,
        _inline_first_available_width: f32,
        arena: &mut crate::view::node_arena::NodeArena,
    ) {
        let inner_width = constraints.max_width.max(0.0);
        let child_max_height = constraints.max_height.max(0.0);
        self.inline_full_available_width = inner_width;
        let mut measured_rect: Option<Rect> = None;
        for child_key in self.children.clone() {
            arena.with_element_taken(child_key, |child, arena| {
                child.measure(
                    LayoutConstraints {
                        max_width: inner_width,
                        max_height: child_max_height,
                        viewport_width: constraints.viewport_width,
                        viewport_height: constraints.viewport_height,
                        percent_base_width: constraints.percent_base_width,
                        percent_base_height: constraints.percent_base_height,
                    },
                    arena,
                );
                let (width, height) = child.measured_size();
                measured_rect = Some(merge_rect(
                    measured_rect,
                    Rect {
                        x: 0.0,
                        y: 0.0,
                        width,
                        height,
                    },
                ));
            });
        }
        let content_size = measured_rect
            .map(|rect| Size {
                width: rect.width.max(0.0),
                height: rect.height.max(0.0),
            })
            .unwrap_or(Size {
                width: 0.0,
                height: 0.0,
            });
        self.layout_state.content_size = content_size;
        let reported_width = self.reported_line_width(content_size.width);
        self.layout_state.layout_size = Size {
            width: reported_width,
            height: content_size.height,
        };
        self.layout_state.layout_inner_size = self.layout_state.layout_size;
        self.flex_info = Some(FlexLayoutInfo {
            lines: if content_size.width > 0.0 || content_size.height > 0.0 {
                vec![Vec::new()]
            } else {
                Vec::new()
            },
            line_main_sum: if content_size.width > 0.0 || content_size.height > 0.0 {
                vec![reported_width]
            } else {
                Vec::new()
            },
            line_cross_max: if content_size.width > 0.0 || content_size.height > 0.0 {
                vec![content_size.height]
            } else {
                Vec::new()
            },
            total_main: reported_width,
            total_cross: content_size.height,
        });
        self.dirty_flags = self
            .dirty_flags
            .without(DirtyFlags::LAYOUT)
            .union(DirtyFlags::PLACE)
            .union(DirtyFlags::BOX_MODEL)
            .union(DirtyFlags::HIT_TEST)
            .union(DirtyFlags::PAINT);
    }

    fn reported_line_width(&self, line_width: f32) -> f32 {
        line_width
            .max(0.0)
            .min(self.inline_full_available_width.max(0.0))
    }

    fn placement_flex_info(&self) -> Option<FlexLayoutInfo> {
        let mut info = self.flex_info.clone()?;
        for width in &mut info.line_main_sum {
            *width = self.reported_line_width(*width);
        }
        info.total_main = info
            .line_main_sum
            .iter()
            .fold(0.0_f32, |acc, &w| acc.max(w));
        Some(info)
    }

    fn clamp_reported_inline_geometry(&mut self) {
        let max_width = self.inline_full_available_width.max(0.0);
        if max_width <= 0.0 {
            return;
        }
        self.layout_state.layout_size.width = self.layout_state.layout_size.width.min(max_width);
        self.layout_state.layout_inner_size.width =
            self.layout_state.layout_inner_size.width.min(max_width);
        self.layout_state.content_size.width = self.layout_state.content_size.width.min(max_width);
        for fragment in &mut self.inline_paint_fragments {
            fragment.width = fragment.width.min(max_width);
        }
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

        for child_key in self.children.clone() {
            arena.with_element_taken(child_key, |child, arena| {
                child.set_layout_offset(0.0, 0.0);
                child.place(
                    LayoutPlacement {
                        parent_x: x,
                        parent_y: y,
                        visual_offset_x: 0.0,
                        visual_offset_y: 0.0,
                        available_width: placement.available_width,
                        available_height: placement.available_height,
                        viewport_width: placement.viewport_width,
                        viewport_height: placement.viewport_height,
                        percent_base_width: placement.percent_base_width,
                        percent_base_height: placement.percent_base_height,
                    },
                    arena,
                );
            });
        }
        self.dirty_flags = self.dirty_flags.without(
            DirtyFlags::PLACE
                .union(DirtyFlags::BOX_MODEL)
                .union(DirtyFlags::HIT_TEST),
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

    fn set_layout_offset(&mut self, x: f32, y: f32) {
        self.flow_offset = Position { x, y };
    }

    fn inline_relative_position(&self) -> (f32, f32) {
        (self.flow_offset.x, self.flow_offset.y)
    }
}

fn merge_rect(current: Option<Rect>, next: Rect) -> Rect {
    let Some(current) = current else {
        return next;
    };
    let left = current.x.min(next.x);
    let top = current.y.min(next.y);
    let right = (current.x + current.width.max(0.0)).max(next.x + next.width.max(0.0));
    let bottom = (current.y + current.height.max(0.0)).max(next.y + next.height.max(0.0));
    Rect {
        x: left,
        y: top,
        width: (right - left).max(0.0),
        height: (bottom - top).max(0.0),
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
