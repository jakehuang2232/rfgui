//! `Layoutable` impl for `TextArea`.
//!
//! Drives `view/layout/measure_axis` and `view/layout/place_axis_children`
//! over its mixed inline children — same Element template established in
//! the P0.1 spike. `TextAreaTextRun`'s Layoutable + Renderable live in
//! [`super::run`] beside the cosmic-text shape state.

use crate::style::{Align, CrossSize, JustifyContent, Layout};
use crate::view::base_component::{
    InlineMeasureContext, LayoutConstraints, LayoutPlacement, Layoutable, Position, Size,
};
use crate::view::layout::measure::{MeasureAxisInputs, measure_axis};
use crate::view::layout::place::{PlaceAxisChildrenInputs, place_axis_children};

use super::TextArea;

impl Layoutable for TextArea {
    fn measure(
        &mut self,
        constraints: LayoutConstraints,
        arena: &mut crate::view::node_arena::NodeArena,
    ) {
        // Sync run subtree to latest `content` before measuring (edits
        // flag `children_dirty`; see projection.rs).
        self.rebuild_children_if_dirty(
            arena,
            constraints.viewport_width,
            constraints.viewport_height,
        );

        let inner_width = constraints.max_width.max(0.0);
        let absolute_mask = vec![false; self.children.len()];
        let outputs = measure_axis(
            MeasureAxisInputs {
                layout: Layout::Inline,
                children: &self.children,
                absolute_mask: &absolute_mask,
                is_row: true,
                is_real_flex: false,
                solver_wrap: self.auto_wrap,
                solver_gap: 0.0,
                main_limit: inner_width,
                inner_width,
                child_available_width: inner_width,
                child_available_height: constraints.max_height.max(0.0),
                child_percent_base_width: constraints.percent_base_width,
                child_percent_base_height: constraints.percent_base_height,
                viewport_width: constraints.viewport_width,
                viewport_height: constraints.viewport_height,
                inline_wrap: self.auto_wrap,
                inline_gap: 0.0,
                inline_first_available_width: Some(inner_width),
            },
            arena,
        );

        self.layout_state.content_size = outputs.content_size;
        // Auto-size to content (decision A1: TextArea has no box-model;
        // wrap an `<Element>` for explicit dimensions).
        self.layout_state.layout_size = Size {
            width: outputs.content_size.width,
            height: outputs.content_size.height,
        };
        self.layout_state.layout_inner_size = self.layout_state.layout_size;
        self.flex_info = Some(outputs.flex_info);
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
        // Allow being placed *as* an inline child of a parent Element.
        self.measure(
            LayoutConstraints {
                max_width: context.first_available_width.max(0.0),
                max_height: 1_000_000.0,
                viewport_width: context.viewport_width,
                viewport_height: context.viewport_height,
                percent_base_width: context.percent_base_width,
                percent_base_height: context.percent_base_height,
            },
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

    fn set_layout_offset(&mut self, x: f32, y: f32) {
        self.flow_offset = Position { x, y };
    }

    fn inline_relative_position(&self) -> (f32, f32) {
        (self.flow_offset.x, self.flow_offset.y)
    }
}
