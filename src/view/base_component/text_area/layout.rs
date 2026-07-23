//! `Layoutable` impl for `TextArea`.
//!
//! Drives TextArea layout from the unified IFC root package. Direct run
//! and projection child boxes are installed from root-owned fragment
//! geometry so editable layout, hit-test, and caret geometry share one
//! source.

use crate::view::base_component::{
    DirtyFlags, LayoutConstraints, LayoutPlacement, Layoutable, Position, Size,
};
use crate::view::layout::FlexLayoutInfo;

use super::TextArea;

impl Layoutable for TextArea {
    fn measure(
        &mut self,
        constraints: LayoutConstraints,
        arena: &mut crate::view::node_arena::NodeArena,
    ) {
        // Clean fast path: nothing feeding the unified IFC changed, the
        // subtree carries no LAYOUT dirt, and the constraints match the
        // previous full measure — every derived size is already correct,
        // so skip the O(children) child loops (arena take + downcast per
        // run, per line) that otherwise dominate ancestor-move frames on
        // editor-sized content.
        if !self.children_dirty
            && self.last_measure_constraints == Some(constraints)
            && self.unified_ifc_package_cache_is_current(arena)
            && self.self_node_key.is_some_and(|key| {
                !arena.subtree_dirty_intersects(
                    key,
                    crate::view::base_component::DirtyPassMask::LAYOUT,
                )
            })
        {
            return;
        }

        // Sync run subtree to latest `content` before measuring (edits
        // flag `children_dirty`; see projection.rs).
        let had_children_dirty = self.children_dirty;
        let previous_layout_height = self.layout_state.layout_size.height;
        self.rebuild_children_if_dirty(
            arena,
            constraints.viewport_width,
            constraints.viewport_height,
        );

        let inner_width = constraints.max_width.max(0.0);
        self.viewport_size.width = inner_width;
        self.measure_unified_inline_ifc_atomic_children(constraints, arena);
        self.clear_generated_text_children_layout_dirty(arena);

        let (mut content_size, flex_info) =
            if let Some(package) = self.unified_inline_ifc_render_package(arena) {
                (
                    package.content_size(),
                    package.flex_info_for_children(&self.children),
                )
            } else {
                (
                    Size {
                        width: 0.0,
                        height: self.font_size.max(1.0) * self.line_height.max(0.8),
                    },
                    FlexLayoutInfo {
                        lines: Vec::new(),
                        line_main_sum: Vec::new(),
                        line_cross_max: Vec::new(),
                        total_main: 0.0,
                        total_cross: 0.0,
                    },
                )
            };
        if had_children_dirty {
            let trailing_newline_count = self
                .content
                .chars()
                .rev()
                .take_while(|ch| *ch == '\n')
                .count();
            if trailing_newline_count > 0 {
                let line_height = self.font_size.max(1.0) * self.line_height.max(0.8);
                content_size.height = content_size
                    .height
                    .max(previous_layout_height + line_height * trailing_newline_count as f32);
            }
        }

        self.layout_state.content_size = content_size;
        self.viewport_size = Size {
            width: inner_width,
            height: content_size
                .height
                .min(constraints.max_height.max(0.0))
                .max(0.0),
        };
        self.layout_state.layout_size = content_size;
        self.layout_state.layout_inner_size = self.layout_state.layout_size;
        self.clamp_scroll_to_content();
        self.flex_info = Some(flex_info);
        self.last_measure_constraints = Some(constraints);
        self.dirty_flags = self
            .dirty_flags
            .without(DirtyFlags::LAYOUT)
            .union(DirtyFlags::PLACE)
            .union(DirtyFlags::BOX_MODEL)
            .union(DirtyFlags::HIT_TEST)
            .union(DirtyFlags::PAINT);
    }

    fn place(
        &mut self,
        placement: LayoutPlacement,
        arena: &mut crate::view::node_arena::NodeArena,
    ) {
        // `measure` clamps the viewport to its incoming max-height, but an
        // auto-height parent can then grow from TextArea's measured content.
        // By place time `layout_size.height` is the final assigned height
        // (or the measured content height when no setter was needed), so use
        // it to reveal rows added by a wrap reflow. Do not derive this from
        // `content_size`: an explicitly shorter parent assignment must stay
        // authoritative so caret-follow scrolling continues to work.
        let final_height = self.layout_state.layout_size.height.max(0.0);
        if final_height <= placement.available_height.max(0.0) + f32::EPSILON
            && (self.viewport_size.height - final_height).abs() > f32::EPSILON
        {
            self.viewport_size.height = final_height;
            self.clamp_scroll_to_content();
        }

        let x = placement.parent_x + placement.visual_offset_x + self.flow_offset.x;
        let y = placement.parent_y + placement.visual_offset_y + self.flow_offset.y;
        self.layout_state.layout_position = Position { x, y };
        self.layout_state.layout_inner_position = Position { x, y };
        self.layout_state.layout_inner_size = self.layout_state.layout_size;
        self.layout_state.layout_flow_position = self.layout_state.layout_position;
        self.layout_state.layout_flow_inner_position = self.layout_state.layout_inner_position;

        self.place_inline_children(placement, arena);
        if self.pending_caret_scroll {
            self.pending_caret_scroll = false;
            if self.scroll_caret_into_view(arena) {
                self.place_inline_children(placement, arena);
            }
            self.scroll_caret_into_ancestor_views(arena);
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
        self.viewport_size.width = self
            .viewport_size
            .width
            .min(self.layout_state.layout_size.width);
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
        let old_x = self.scroll_x;
        let old_y = self.scroll_y;
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
        self.scroll_rect_into_ancestor_views(
            arena,
            crate::ui::Rect::new(
                cx - (next_x - old_x),
                cy - (next_y - old_y),
                1.0,
                line_height.max(1.0),
            ),
        );
        changed
    }

    pub(super) fn scroll_caret_into_ancestor_views(
        &self,
        arena: &crate::view::node_arena::NodeArena,
    ) -> bool {
        if self.self_node_key.is_none() {
            return false;
        }
        let Some((cx, cy, line_height)) = self.caret_screen_position(arena) else {
            return false;
        };
        self.scroll_rect_into_ancestor_views(
            arena,
            crate::ui::Rect::new(cx, cy, 1.0, line_height.max(1.0)),
        )
    }

    fn scroll_rect_into_ancestor_views(
        &self,
        arena: &crate::view::node_arena::NodeArena,
        rect: crate::ui::Rect,
    ) -> bool {
        let Some(self_key) = self.self_node_key else {
            return false;
        };
        crate::view::viewport::dispatch::scroll_rect_into_view_from(
            arena,
            self_key,
            rect,
            crate::ui::ScrollIntoViewOptions::default(),
            false,
            true,
        )
    }

    fn place_inline_children(
        &self,
        placement: LayoutPlacement,
        arena: &mut crate::view::node_arena::NodeArena,
    ) {
        self.apply_unified_inline_ifc_child_placements(arena, placement);
    }
}

#[cfg(test)]
mod tests;
