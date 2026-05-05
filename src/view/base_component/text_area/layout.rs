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
use crate::view::layout::FlexLayoutInfo;
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
        self.viewport_size = Size {
            width: outputs.content_size.width.min(inner_width).max(0.0),
            height: outputs
                .content_size
                .height
                .min(constraints.max_height.max(0.0))
                .max(0.0),
        };
        self.layout_state.layout_size = Size {
            width: outputs.content_size.width,
            height: outputs.content_size.height,
        };
        self.layout_state.layout_inner_size = self.layout_state.layout_size;
        self.clamp_scroll_to_content();
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
        self.place_inline_children(&info, placement, arena);
        if self.pending_caret_scroll {
            self.pending_caret_scroll = false;
            if self.scroll_caret_into_view(arena) {
                self.place_inline_children(&info, placement, arena);
            }
        }
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
        self.viewport_size.width = self.viewport_size.width.min(self.layout_state.layout_size.width);
        self.clamp_scroll_to_content();
    }

    fn set_layout_height(&mut self, height: f32) {
        self.layout_state.layout_size.height = height.max(0.0);
        self.viewport_size.height = self
            .viewport_size
            .height
            .min(self.layout_state.layout_size.height);
        self.clamp_scroll_to_content();
    }

    fn set_layout_offset(&mut self, x: f32, y: f32) {
        self.flow_offset = Position { x, y };
    }

    fn inline_relative_position(&self) -> (f32, f32) {
        (self.flow_offset.x, self.flow_offset.y)
    }
}

impl TextArea {
    fn max_scroll(&self) -> (f32, f32) {
        (
            (self.layout_state.content_size.width - self.viewport_size.width).max(0.0),
            (self.layout_state.content_size.height - self.viewport_size.height).max(0.0),
        )
    }

    fn clamp_scroll_to_content(&mut self) {
        let (max_x, max_y) = self.max_scroll();
        self.scroll_x = self.scroll_x.clamp(0.0, max_x);
        self.scroll_y = self.scroll_y.clamp(0.0, max_y);
    }

    pub(super) fn scroll_caret_into_view(
        &mut self,
        arena: &crate::view::node_arena::NodeArena,
    ) -> bool {
        let Some((cx, cy, line_height)) = self.caret_screen_position(arena) else {
            return false;
        };
        let viewport_left = self.layout_state.layout_position.x;
        let viewport_top = self.layout_state.layout_position.y;
        let viewport_right = viewport_left + self.viewport_size.width.max(0.0);
        let viewport_bottom = viewport_top + self.viewport_size.height.max(0.0);

        let caret_right = cx + 1.0;
        let caret_bottom = cy + line_height.max(1.0);
        let mut dx = 0.0;
        if cx < viewport_left {
            dx = cx - viewport_left;
        } else if caret_right > viewport_right {
            dx = caret_right - viewport_right;
        }
        let mut dy = 0.0;
        if cy < viewport_top {
            dy = cy - viewport_top;
        } else if caret_bottom > viewport_bottom {
            dy = caret_bottom - viewport_bottom;
        }

        let (content_max_x, content_max_y) = self.max_scroll();
        let max_x = content_max_x.max((self.scroll_x + dx).max(0.0));
        let max_y = content_max_y.max((self.scroll_y + dy).max(0.0));
        if max_x > content_max_x {
            self.layout_state.content_size.width = self.viewport_size.width + max_x;
        }
        if max_y > content_max_y {
            self.layout_state.content_size.height = self.viewport_size.height + max_y;
        }
        let next_x = (self.scroll_x + dx).clamp(0.0, max_x);
        let next_y = (self.scroll_y + dy).clamp(0.0, max_y);
        let changed = (next_x - self.scroll_x).abs() > f32::EPSILON
            || (next_y - self.scroll_y).abs() > f32::EPSILON;
        self.scroll_x = next_x;
        self.scroll_y = next_y;
        if changed {
            self.dirty_flags = self
                .dirty_flags
                .union(crate::view::base_component::DirtyFlags::RUNTIME);
        }
        changed
    }

    fn place_inline_children(
        &self,
        info: &FlexLayoutInfo,
        placement: LayoutPlacement,
        arena: &mut crate::view::node_arena::NodeArena,
    ) {
        place_axis_children(
            PlaceAxisChildrenInputs {
                layout: Layout::Inline,
                children: &self.children,
                flex_info: info.clone(),
                is_row: true,
                gap: 0.0,
                main_limit: placement.available_width,
                cross_limit: placement.available_height,
                origin_x: self.layout_state.layout_position.x,
                origin_y: self.layout_state.layout_position.y,
                visual_offset_x: -self.scroll_x,
                visual_offset_y: -self.scroll_y,
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::base_component::ElementTrait;

    fn placed_text_area(
        content: &str,
        cursor_char: usize,
        max_width: f32,
        max_height: f32,
        auto_wrap: bool,
    ) -> (
        crate::view::node_arena::NodeArena,
        crate::view::node_arena::NodeKey,
    ) {
        let mut text_area = TextArea::new();
        text_area.content = content.to_string();
        text_area.cursor_char = cursor_char;
        text_area.font_size = 14.0;
        text_area.line_height = 1.25;
        text_area.auto_wrap = auto_wrap;
        text_area.pending_caret_scroll = true;

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
                max_width,
                max_height,
                viewport_width: max_width,
                viewport_height: max_height,
                percent_base_width: Some(max_width),
                percent_base_height: Some(max_height),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: max_width,
                available_height: max_height,
                viewport_width: max_width,
                viewport_height: max_height,
                percent_base_width: Some(max_width),
                percent_base_height: Some(max_height),
            },
        );
        (arena, root)
    }

    #[test]
    fn place_scrolls_viewport_down_to_caret() {
        let content = "one\ntwo\nthree\nfour\nfive";
        let (arena, root) = placed_text_area(content, content.chars().count(), 200.0, 35.0, true);

        arena.with_element_taken_ref(root, |el, arena| {
            let text_area = el.as_any().downcast_ref::<TextArea>().unwrap();
            let (_, caret_y, caret_h) = text_area.caret_screen_position(arena).expect("caret");
            let viewport_bottom =
                text_area.layout_state.layout_position.y + text_area.viewport_size.height;

            assert!(text_area.scroll_y > 0.0);
            assert!(caret_y + caret_h <= viewport_bottom + 0.5);
        });
    }

    #[test]
    fn place_scrolls_viewport_right_to_caret_when_nowrap() {
        let content = "abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz";
        let (arena, root) = placed_text_area(content, content.chars().count(), 80.0, 40.0, false);

        arena.with_element_taken_ref(root, |el, arena| {
            let text_area = el.as_any().downcast_ref::<TextArea>().unwrap();
            let (caret_x, _, _) = text_area.caret_screen_position(arena).expect("caret");
            let viewport_right =
                text_area.layout_state.layout_position.x + text_area.viewport_size.width;

            assert!(text_area.scroll_x > 0.0);
            assert!(caret_x + 1.0 <= viewport_right + 0.5);
        });
    }

    #[test]
    fn nowrap_keeps_content_width_as_reported_layout_width() {
        let content = "abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz";
        let (arena, root) = placed_text_area(content, content.chars().count(), 80.0, 40.0, false);

        arena.with_element_taken_ref(root, |el, _| {
            let text_area = el.as_any().downcast_ref::<TextArea>().unwrap();
            assert!(text_area.viewport_size.width <= 80.0);
            assert!(
                text_area.layout_state.layout_size.width > text_area.viewport_size.width,
                "TextArea must keep reporting content width so the parent Element can clip overflow",
            );
        });
    }
}
