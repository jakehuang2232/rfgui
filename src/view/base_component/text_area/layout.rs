//! `Layoutable` impl for `TextArea`.
//!
//! Drives `view/layout/measure_axis` and `view/layout/place_axis_children`
//! over its mixed inline children — same Element template established in
//! the P0.1 spike. `TextAreaTextRun`'s Layoutable + Renderable live in
//! [`super::run`] beside the shared text layout adapter state.

use crate::style::{Align, CrossSize, JustifyContent, Layout};
use crate::ui::Rect;
use crate::view::base_component::{
    InlineMeasureContext, InlineNodeSize, InlinePlacement, LayoutConstraints, LayoutPlacement,
    Layoutable, Position, Size,
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
            width: inner_width,
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
        let viewport_height = self
            .layout_state
            .content_size
            .height
            .min(placement.available_height.max(0.0))
            .max(0.0);
        if (self.viewport_size.height - viewport_height).abs() > f32::EPSILON {
            self.viewport_size.height = viewport_height;
            self.clamp_scroll_to_content();
        }

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
            self.scroll_caret_into_ancestor_views(arena);
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
                max_height: context.available_height.max(0.0),
                viewport_width: context.viewport_width,
                viewport_height: context.viewport_height,
                percent_base_width: context.percent_base_width,
                percent_base_height: context.percent_base_height,
            },
            arena,
        );
    }

    fn get_inline_nodes_size(
        &self,
        _arena: &crate::view::node_arena::NodeArena,
    ) -> Vec<InlineNodeSize> {
        if self.should_expose_inline_fragments() {
            let info = self.flex_info.as_ref().expect("checked above");
            return info
                .lines
                .iter()
                .enumerate()
                .map(|(line_idx, _)| InlineNodeSize {
                    width: info.line_main_sum[line_idx].max(0.0),
                    height: (info.line_ascent.get(line_idx).copied().unwrap_or(0.0)
                        + info.line_descent.get(line_idx).copied().unwrap_or(0.0))
                    .max(0.0),
                    baseline: info.line_ascent.get(line_idx).copied().unwrap_or(0.0),
                    force_break_after: line_idx + 1 < info.lines.len(),
                    ..Default::default()
                })
                .collect();
        }
        let (width, height) = self.measured_size();
        vec![InlineNodeSize {
            width,
            height,
            baseline: height,
            ..Default::default()
        }]
    }

    fn place_inline(
        &mut self,
        placement: InlinePlacement,
        arena: &mut crate::view::node_arena::NodeArena,
    ) {
        if !self.should_expose_inline_fragments() {
            self.set_layout_offset(placement.offset_x, placement.offset_y);
            self.place(
                LayoutPlacement {
                    parent_x: placement.parent_x,
                    parent_y: placement.parent_y,
                    visual_offset_x: placement.visual_offset_x,
                    visual_offset_y: placement.visual_offset_y,
                    available_width: placement.available_width,
                    available_height: placement.available_height,
                    viewport_width: placement.viewport_width,
                    viewport_height: placement.viewport_height,
                    percent_base_width: placement.percent_base_width,
                    percent_base_height: placement.percent_base_height,
                },
                arena,
            );
            return;
        }
        let info = self.flex_info.as_ref().expect("checked above");
        let Some(line) = info.lines.get(placement.node_index).cloned() else {
            return;
        };
        let line_count = info.lines.len();
        let line_width = info
            .line_main_sum
            .get(placement.node_index)
            .copied()
            .unwrap_or(0.0)
            .max(0.0);
        let line_ascent = info
            .line_ascent
            .get(placement.node_index)
            .copied()
            .unwrap_or(0.0);
        let line_descent = info
            .line_descent
            .get(placement.node_index)
            .copied()
            .unwrap_or(0.0);
        let line_height = (line_ascent + line_descent).max(0.0);

        if placement.node_index == 0 {
            self.inline_paint_fragments.clear();
            self.layout_state.layout_position = Position {
                x: placement.x,
                y: placement.y,
            };
            self.layout_state.layout_size = Size {
                width: 0.0,
                height: 0.0,
            };
            self.layout_state.should_render = false;
        }

        let fragment_info = FlexLayoutInfo {
            lines: vec![line],
            line_main_sum: vec![line_width],
            line_cross_max: Vec::new(),
            line_ascent: vec![line_ascent],
            line_descent: vec![line_descent],
            total_main: line_width,
            total_cross: line_height,
        };
        place_axis_children(
            PlaceAxisChildrenInputs {
                layout: Layout::Inline,
                children: &self.children,
                flex_info: fragment_info,
                is_row: true,
                gap: 0.0,
                main_limit: placement.available_width,
                cross_limit: line_height,
                origin_x: placement.x,
                origin_y: placement.y,
                visual_offset_x: -self.scroll_x,
                visual_offset_y: -self.scroll_y,
                child_available_width: placement.available_width,
                child_available_height: placement.available_height,
                child_parent_hit_test_clip: None,
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

        let rect = Rect {
            x: placement.x,
            y: placement.y,
            width: line_width,
            height: line_height,
        };
        self.extend_inline_bounds(rect);
        self.inline_paint_fragments.push(rect);
        if self.pending_caret_scroll && placement.node_index + 1 == line_count {
            self.pending_caret_scroll = false;
            if self.scroll_caret_into_view(arena) {
                self.place_inline(placement, arena);
            }
            self.scroll_caret_into_ancestor_views(arena);
        }
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
    fn should_expose_inline_fragments(&self) -> bool {
        self.flex_info
            .as_ref()
            .is_some_and(|info| info.lines.len() > 1)
            && self.viewport_size.width + 0.5 >= self.layout_state.content_size.width
            && self.viewport_size.height + 0.5 >= self.layout_state.content_size.height
    }

    fn extend_inline_bounds(&mut self, rect: Rect) {
        let left = rect.x;
        let top = rect.y;
        let right = rect.x + rect.width.max(0.0);
        let bottom = rect.y + rect.height.max(0.0);
        if self.layout_state.should_render {
            let current_right =
                self.layout_state.layout_position.x + self.layout_state.layout_size.width;
            let current_bottom =
                self.layout_state.layout_position.y + self.layout_state.layout_size.height;
            self.layout_state.layout_position.x = self.layout_state.layout_position.x.min(left);
            self.layout_state.layout_position.y = self.layout_state.layout_position.y.min(top);
            self.layout_state.layout_size.width =
                current_right.max(right) - self.layout_state.layout_position.x;
            self.layout_state.layout_size.height =
                current_bottom.max(bottom) - self.layout_state.layout_position.y;
        } else {
            self.layout_state.layout_position = Position { x: left, y: top };
            self.layout_state.layout_size = Size {
                width: (right - left).max(0.0),
                height: (bottom - top).max(0.0),
            };
        }
        self.layout_state.should_render =
            self.layout_state.layout_size.width > 0.0 && self.layout_state.layout_size.height > 0.0;
        self.layout_state.layout_inner_position = self.layout_state.layout_position;
        self.layout_state.layout_inner_size = self.layout_state.layout_size;
        self.layout_state.layout_flow_position = self.layout_state.layout_position;
        self.layout_state.layout_flow_inner_position = self.layout_state.layout_inner_position;
        self.layout_state.content_size = self.layout_state.layout_size;
    }

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
        crate::view::base_component::scroll_rect_into_view_from(
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
                child_parent_hit_test_clip: None,
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
        self.apply_unified_inline_ifc_projection_placements(arena, placement);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::base_component::{ElementTrait, hit_test};

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

    fn projection_chip_text_area(
        token: &'static str,
        max_width: f32,
        max_height: f32,
        auto_wrap: bool,
    ) -> (
        crate::view::node_arena::NodeArena,
        crate::view::node_arena::NodeKey,
    ) {
        let content = format!("pre {token} post");
        let token_byte_start = content.find(token).expect("token");
        let range_start = content[..token_byte_start].chars().count();
        let range = range_start..range_start + token.chars().count();
        let mut text_area = TextArea::new();
        text_area.content = content;
        text_area.font_size = 14.0;
        text_area.line_height = 1.25;
        text_area.auto_wrap = auto_wrap;
        text_area.on_render_handler = Some(crate::ui::on_text_area_render(move |render| {
            render.range(range.clone(), move |_node| {
                crate::ui::RsxNode::tagged(
                    "Element",
                    crate::ui::RsxTagDescriptor::for_tag::<crate::view::tags::Element>(),
                )
                .with_prop(
                    "style",
                    crate::view::ElementStylePropSchema {
                        padding: Some(
                            crate::style::Padding::uniform(crate::style::Length::px(0.0))
                                .x(crate::style::Length::px(20.0)),
                        ),
                        font_size: Some(crate::style::FontSize::Px(24.0)),
                        border: Some(crate::style::Border::uniform(
                            crate::style::Length::px(1.0),
                            &crate::style::Color::hex("#42566f"),
                        )),
                        ..Default::default()
                    },
                )
                .with_child(
                    crate::ui::RsxNode::tagged(
                        "Text",
                        crate::ui::RsxTagDescriptor::for_tag::<crate::view::tags::Text>(),
                    )
                    .with_child(crate::ui::RsxNode::text(token)),
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

    fn first_projection_segment(
        arena: &crate::view::node_arena::NodeArena,
        root: crate::view::node_arena::NodeKey,
    ) -> crate::view::node_arena::NodeKey {
        let root_node = arena.get(root).expect("TextArea root");
        let text_area = root_node
            .element
            .as_any()
            .downcast_ref::<TextArea>()
            .unwrap();
        text_area
            .children
            .iter()
            .copied()
            .find(|key| {
                arena.get(*key).is_some_and(|node| {
                    node.element
                        .as_any()
                        .is::<crate::view::base_component::text_area::TextAreaProjectionSegment>()
                })
            })
            .expect("projection segment")
    }

    fn first_projection_text_line_count(
        arena: &crate::view::node_arena::NodeArena,
        root: crate::view::node_arena::NodeKey,
    ) -> usize {
        fn visit(
            arena: &crate::view::node_arena::NodeArena,
            key: crate::view::node_arena::NodeKey,
        ) -> Option<usize> {
            let node = arena.get(key)?;
            if let Some(text) = node
                .element
                .as_any()
                .downcast_ref::<crate::view::base_component::Text>()
            {
                return Some(text.visual_line_heads().len().max(1));
            }
            for child in node.element.children() {
                if let Some(count) = visit(arena, *child) {
                    return Some(count);
                }
            }
            None
        }
        visit(arena, root).expect("projection Text descendant")
    }

    #[test]
    fn projection_chip_wraps_inside_text_area_width_when_auto_wrap_enabled() {
        let max_width = 160.0;
        let (arena, root) = projection_chip_text_area(
            "{{USERAAAAASSZZsdc_ID_USERAAAAASSZZsdc_ID}}",
            max_width,
            240.0,
            true,
        );
        let segment = first_projection_segment(&arena, root);
        let segment_snapshot = arena
            .get(segment)
            .expect("projection segment")
            .element
            .box_model_snapshot();

        assert!(
            segment_snapshot.width <= max_width + 0.5,
            "projection segment must not report wider than TextArea viewport, width={}",
            segment_snapshot.width,
        );
        assert!(
            segment_snapshot.x + segment_snapshot.width <= max_width + 0.5,
            "projection segment must fit the remaining TextArea line width, x={} width={}",
            segment_snapshot.x,
            segment_snapshot.width,
        );
        assert!(
            first_projection_text_line_count(&arena, segment) > 1,
            "projection Text must wrap inside the constrained chip when auto_wrap=true",
        );
    }

    #[test]
    fn projection_chip_shrinks_without_internal_wrap_when_auto_wrap_disabled() {
        let max_width = 160.0;
        let (arena, root) = projection_chip_text_area(
            "{{USERAAAAASSZZsdc_ID_USERAAAAASSZZsdc_ID}}",
            max_width,
            240.0,
            false,
        );
        let segment = first_projection_segment(&arena, root);
        let segment_snapshot = arena
            .get(segment)
            .expect("projection segment")
            .element
            .box_model_snapshot();

        assert!(
            segment_snapshot.width <= max_width + 0.5,
            "projection segment must shrink to TextArea viewport when auto_wrap=false, width={}",
            segment_snapshot.width,
        );
        assert_eq!(
            first_projection_text_line_count(&arena, segment),
            1,
            "projection Text must stay on one line when auto_wrap=false",
        );
    }

    #[test]
    fn projection_badges_do_not_overlap_following_text_runs() {
        let content = "First line with a long value that can wrap when auto wrap is enabled.{{API_HOST}}/v1/users/{{USER_ID}}/activity/with/a/very/long/path\nTail line";
        let mut text_area = TextArea::new();
        text_area.content = content.to_string();
        text_area.font_size = 14.0;
        text_area.line_height = 1.25;
        text_area.auto_wrap = true;
        text_area.on_render_handler = Some(crate::ui::on_text_area_render(move |render| {
            let ranges = [(69..81), (91..102)];
            for range in ranges {
                let slice: String = content
                    .chars()
                    .skip(range.start)
                    .take(range.len())
                    .collect();
                render.range(range.clone(), move |_node| {
                    let slice = slice.clone();
                    crate::ui::RsxNode::tagged(
                        "Element",
                        crate::ui::RsxTagDescriptor::for_tag::<crate::view::tags::Element>(),
                    )
                    .with_prop(
                        "style",
                        crate::view::ElementStylePropSchema {
                            padding: Some(
                                crate::style::Padding::uniform(crate::style::Length::px(0.0))
                                    .x(crate::style::Length::px(20.0)),
                            ),
                            font_size: Some(crate::style::FontSize::Px(24.0)),
                            border: Some(crate::style::Border::uniform(
                                crate::style::Length::px(1.0),
                                &crate::style::Color::hex("#42566f"),
                            )),
                            ..Default::default()
                        },
                    )
                    .with_child(
                        crate::ui::RsxNode::tagged(
                            "Text",
                            crate::ui::RsxTagDescriptor::for_tag::<crate::view::tags::Text>(),
                        )
                        .with_child(crate::ui::RsxNode::text(slice)),
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
                max_width: 360.0,
                max_height: 240.0,
                viewport_width: 360.0,
                viewport_height: 240.0,
                percent_base_width: Some(360.0),
                percent_base_height: Some(240.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 360.0,
                available_height: 240.0,
                viewport_width: 360.0,
                viewport_height: 240.0,
                percent_base_width: Some(360.0),
                percent_base_height: Some(240.0),
            },
        );

        let root_node = arena.get(root).expect("TextArea root");
        let text_area = root_node
            .element
            .as_any()
            .downcast_ref::<TextArea>()
            .unwrap();
        let mut api = None;
        let mut user = None;
        let mut users_path = None;
        let mut activity_path = None;
        for &child in &text_area.children {
            let node = arena.get(child).expect("TextArea child");
            let snapshot = node.element.box_model_snapshot();
            if let Some(segment) = node
                .element
                .as_any()
                .downcast_ref::<crate::view::base_component::text_area::TextAreaProjectionSegment>(
            ) {
                match segment.char_range() {
                    range if range == (69..81) => api = Some(snapshot),
                    range if range == (91..102) => user = Some(snapshot),
                    _ => {}
                }
            } else if let Some(run) =
                node.element
                    .as_any()
                    .downcast_ref::<crate::view::base_component::text_area::TextAreaTextRun>()
            {
                if run.text == "/v1/users/" {
                    users_path = Some(snapshot);
                }
                if run.text.contains("/activity/with") {
                    activity_path = Some(snapshot);
                }
            }
        }

        fn assert_no_same_line_overlap(
            left: crate::view::base_component::BoxModelSnapshot,
            right: crate::view::base_component::BoxModelSnapshot,
            label: &str,
        ) {
            let vertical_overlap =
                left.y < right.y + right.height - 0.5 && right.y < left.y + left.height - 0.5;
            if vertical_overlap {
                assert!(
                    right.x + 0.5 >= left.x + left.width,
                    "{label} overlap: left={left:?}, right={right:?}",
                );
            }
        }

        assert_no_same_line_overlap(
            api.expect("API projection"),
            users_path.expect("/v1/users/ run"),
            "API projection and following path",
        );
        assert_no_same_line_overlap(
            user.expect("USER projection"),
            activity_path.expect("/activity run"),
            "USER projection and following path",
        );
    }

    #[test]
    fn auto_height_parent_grows_after_trailing_newline_insert() {
        let line_height = 14.0 * 1.25;
        let mut text_area = TextArea::new();
        text_area.content = "abc".to_string();
        text_area.cursor_char = text_area.content.chars().count();
        text_area.font_size = 14.0;
        text_area.line_height = 1.25;

        let mut parent = crate::view::base_component::Element::new(0.0, 0.0, 200.0, 0.0);
        let mut parent_style = crate::style::Style::new();
        parent_style.insert(
            crate::style::PropertyId::Layout,
            crate::style::ParsedValue::Layout(
                crate::style::Layout::flow().column().no_wrap().into(),
            ),
        );
        parent_style.insert(
            crate::style::PropertyId::Width,
            crate::style::ParsedValue::Length(crate::style::Length::px(200.0)),
        );
        parent_style.insert(
            crate::style::PropertyId::Height,
            crate::style::ParsedValue::Auto,
        );
        parent.apply_style(parent_style);

        let mut arena = crate::view::test_support::new_test_arena();
        let parent_key = crate::view::test_support::commit_element(
            &mut arena,
            Box::new(parent) as Box<dyn ElementTrait>,
        );
        let text_area_key =
            crate::view::test_support::commit_child(&mut arena, parent_key, Box::new(text_area));
        arena.with_element_taken(text_area_key, |el, _| {
            el.as_any_mut()
                .downcast_mut::<TextArea>()
                .expect("TextArea child")
                .set_self_node_key(text_area_key);
        });

        let constraints = LayoutConstraints {
            max_width: 200.0,
            max_height: 600.0,
            viewport_width: 200.0,
            viewport_height: 600.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(600.0),
        };
        let placement = LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 200.0,
            available_height: 600.0,
            viewport_width: 200.0,
            viewport_height: 600.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(600.0),
        };

        crate::view::test_support::measure_and_place(
            &mut arena,
            parent_key,
            constraints,
            placement,
        );
        let before = arena
            .get(parent_key)
            .expect("parent")
            .element
            .box_model_snapshot()
            .height;

        arena.with_element_taken(text_area_key, |el, _| {
            el.as_any_mut()
                .downcast_mut::<TextArea>()
                .expect("TextArea child")
                .insert_text("\n");
        });
        arena.refresh_subtree_dirty_cache(parent_key);
        crate::view::test_support::measure_and_place(
            &mut arena,
            parent_key,
            constraints,
            placement,
        );

        let after = arena
            .get(parent_key)
            .expect("parent")
            .element
            .box_model_snapshot()
            .height;
        assert!(
            after >= before + line_height - 0.5,
            "expected trailing newline to grow auto-height parent by about one line: before={before}, after={after}, line_height={line_height}",
        );
    }

    #[test]
    fn auto_height_parent_grows_for_each_trailing_newline_insert() {
        let line_height = 14.0 * 1.25;
        let mut text_area = TextArea::new();
        text_area.content = "abc".to_string();
        text_area.cursor_char = text_area.content.chars().count();
        text_area.font_size = 14.0;
        text_area.line_height = 1.25;

        let mut parent = crate::view::base_component::Element::new(0.0, 0.0, 200.0, 0.0);
        let mut parent_style = crate::style::Style::new();
        parent_style.insert(
            crate::style::PropertyId::Layout,
            crate::style::ParsedValue::Layout(
                crate::style::Layout::flow().column().no_wrap().into(),
            ),
        );
        parent_style.insert(
            crate::style::PropertyId::Width,
            crate::style::ParsedValue::Length(crate::style::Length::px(200.0)),
        );
        parent_style.insert(
            crate::style::PropertyId::Height,
            crate::style::ParsedValue::Auto,
        );
        parent.apply_style(parent_style);

        let mut arena = crate::view::test_support::new_test_arena();
        let parent_key = crate::view::test_support::commit_element(
            &mut arena,
            Box::new(parent) as Box<dyn ElementTrait>,
        );
        let text_area_key =
            crate::view::test_support::commit_child(&mut arena, parent_key, Box::new(text_area));
        arena.with_element_taken(text_area_key, |el, _| {
            el.as_any_mut()
                .downcast_mut::<TextArea>()
                .expect("TextArea child")
                .set_self_node_key(text_area_key);
        });

        let constraints = LayoutConstraints {
            max_width: 200.0,
            max_height: 600.0,
            viewport_width: 200.0,
            viewport_height: 600.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(600.0),
        };
        let placement = LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 200.0,
            available_height: 600.0,
            viewport_width: 200.0,
            viewport_height: 600.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(600.0),
        };

        crate::view::test_support::measure_and_place(
            &mut arena,
            parent_key,
            constraints,
            placement,
        );
        let before = arena
            .get(parent_key)
            .expect("parent")
            .element
            .box_model_snapshot()
            .height;

        arena.with_element_taken(text_area_key, |el, _| {
            el.as_any_mut()
                .downcast_mut::<TextArea>()
                .expect("TextArea child")
                .insert_text("\n\n\n");
        });
        arena.refresh_subtree_dirty_cache(parent_key);
        crate::view::test_support::measure_and_place(
            &mut arena,
            parent_key,
            constraints,
            placement,
        );

        let after = arena
            .get(parent_key)
            .expect("parent")
            .element
            .box_model_snapshot()
            .height;
        assert!(
            after >= before + line_height * 3.0 - 0.5,
            "expected each trailing newline to grow auto-height parent: before={before}, after={after}, line_height={line_height}",
        );
        arena.with_element_taken_ref(text_area_key, |el, arena| {
            let text_area = el.as_any().downcast_ref::<TextArea>().expect("TextArea child");
            let (_, caret_y, caret_h) = text_area.caret_screen_position(arena).expect("caret");
            let text_area_bottom =
                text_area.layout_state.layout_position.y + text_area.layout_state.layout_size.height;
            assert!(
                caret_y + caret_h <= text_area_bottom + 0.5,
                "caret must fit inside auto-height TextArea: caret_y={caret_y}, caret_h={caret_h}, text_area_bottom={text_area_bottom}",
            );
        });
    }

    #[test]
    fn place_preserves_text_area_origin_and_children_fractional_layout() {
        let mut text_area = TextArea::new();
        text_area.content = "snap me".to_string();

        let mut arena = crate::view::test_support::new_test_arena();
        let root = crate::view::test_support::commit_element(
            &mut arena,
            Box::new(text_area) as Box<dyn ElementTrait>,
        );
        crate::view::test_support::measure_and_place(
            &mut arena,
            root,
            LayoutConstraints {
                max_width: 200.0,
                max_height: 40.0,
                viewport_width: 200.0,
                viewport_height: 40.0,
                percent_base_width: Some(200.0),
                percent_base_height: Some(40.0),
            },
            LayoutPlacement {
                parent_x: 10.25,
                parent_y: 20.75,
                visual_offset_x: 0.25,
                visual_offset_y: -0.25,
                available_width: 200.0,
                available_height: 40.0,
                viewport_width: 200.0,
                viewport_height: 40.0,
                percent_base_width: Some(200.0),
                percent_base_height: Some(40.0),
            },
        );

        arena.with_element_taken_ref(root, |el, arena| {
            let text_area = el.as_any().downcast_ref::<TextArea>().unwrap();
            assert_eq!(text_area.layout_state.layout_position.x, 10.5);
            assert_eq!(text_area.layout_state.layout_position.y, 20.5);
            let first_child = *text_area.children.first().expect("text run child");
            let child_snapshot = arena
                .get(first_child)
                .expect("child node")
                .element
                .box_model_snapshot();
            assert_eq!(child_snapshot.x, 10.5);
            assert_eq!(child_snapshot.y, 20.5);
        });
    }

    #[test]
    fn text_run_inline_placement_preserves_fractional_line_metrics() {
        let mut run =
            crate::view::base_component::text_area::TextAreaTextRun::new("snap".to_string(), 0..4);
        run.place_inline(
            crate::view::base_component::InlinePlacement {
                node_index: 0,
                x: 11.0,
                y: 21.4,
                offset_x: 0.0,
                offset_y: 0.4,
                parent_x: 11.0,
                parent_y: 21.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 200.0,
                available_height: 40.0,
                viewport_width: 200.0,
                viewport_height: 40.0,
                percent_base_width: Some(200.0),
                percent_base_height: Some(40.0),
            },
            &mut crate::view::test_support::new_test_arena(),
        );

        assert_eq!(run.box_model_snapshot().x, 11.0);
        assert_eq!(run.box_model_snapshot().y, 21.4);
    }

    #[test]
    fn wrapped_text_run_text_pass_fragments_apply_paint_offset_after_layout() {
        let content = "alpha beta gamma delta epsilon zeta eta theta iota kappa";
        let (arena, root) = placed_text_area(content, 0, 90.0, 240.0, true);
        let paint_offset = [0.35, -0.6];

        arena.with_element_taken_ref(root, |el, arena| {
            let text_area = el.as_any().downcast_ref::<TextArea>().expect("TextArea root");
            let mut checked = false;
            for key in &text_area.children {
                let Some(node) = arena.get(*key) else {
                    continue;
                };
                let Some(run) = node.element.as_any().downcast_ref::<
                    crate::view::base_component::text_area::TextAreaTextRun,
                >() else {
                    continue;
                };
                let raw = run.inline_text_pass_fragment_positions();
                let painted = run.inline_text_pass_fragment_positions_with_offset(paint_offset);

                assert!(
                    raw.len() > 1,
                    "fixture must produce wrapped fragments, got {raw:?}"
                );
                assert_eq!(raw.len(), painted.len());
                for ((raw_content, raw_rect), (painted_content, painted_rect)) in
                    raw.iter().zip(painted.iter())
                {
                    assert_eq!(raw_content, painted_content);
                    assert!(
                        (painted_rect.x - (raw_rect.x + paint_offset[0])).abs() < 0.001,
                        "paint x must apply offset after layout: raw={raw_rect:?}, painted={painted_rect:?}"
                    );
                    assert!(
                        (painted_rect.y - (raw_rect.y + paint_offset[1])).abs() < 0.001,
                        "paint y must apply offset after layout: raw={raw_rect:?}, painted={painted_rect:?}"
                    );
                    assert_eq!(painted_rect.width, raw_rect.width);
                    assert_eq!(painted_rect.height, raw_rect.height);
                }
                checked = true;
                break;
            }
            assert!(checked, "text run");
        });
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
    fn inline_place_scrolls_viewport_down_to_caret_with_late_height() {
        let content = "one\ntwo\nthree\nfour\nfive";
        let mut text_area = TextArea::new();
        text_area.content = content.to_string();
        text_area.cursor_char = content.chars().count();
        text_area.font_size = 14.0;
        text_area.line_height = 1.25;
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
        arena.with_element_taken(root, |el, arena| {
            let text_area = el.as_any_mut().downcast_mut::<TextArea>().unwrap();
            text_area.measure_inline(
                InlineMeasureContext {
                    first_available_width: 200.0,
                    full_available_width: 200.0,
                    available_height: 35.0,
                    viewport_width: 200.0,
                    viewport_height: 35.0,
                    percent_base_width: Some(200.0),
                    percent_base_height: Some(35.0),
                },
                arena,
            );
            assert_eq!(
                text_area.scroll_y, 0.0,
                "inline measure does not know the eventual placement height yet",
            );
            let fragments = text_area.get_inline_nodes_size(arena);
            let mut y = 0.0;
            for (idx, fragment) in fragments.iter().enumerate() {
                text_area.place_inline(
                    crate::view::base_component::InlinePlacement {
                        node_index: idx,
                        x: 0.0,
                        y,
                        offset_x: 0.0,
                        offset_y: y,
                        parent_x: 0.0,
                        parent_y: 0.0,
                        visual_offset_x: 0.0,
                        visual_offset_y: 0.0,
                        available_width: 200.0,
                        available_height: 35.0,
                        viewport_width: 200.0,
                        viewport_height: 35.0,
                        percent_base_width: Some(200.0),
                        percent_base_height: Some(35.0),
                    },
                    arena,
                );
                y += fragment.height;
            }
        });

        arena.with_element_taken_ref(root, |el, arena| {
            let text_area = el.as_any().downcast_ref::<TextArea>().unwrap();
            let (_, caret_y, caret_h) = text_area.caret_screen_position(arena).expect("caret");
            let viewport_bottom =
                text_area.layout_state.layout_position.y + text_area.viewport_size.height;

            assert_eq!(text_area.viewport_size.height, 35.0);
            assert!(text_area.scroll_y > 0.0);
            assert!(caret_y + caret_h <= viewport_bottom + 0.5);
        });
    }

    #[test]
    fn inline_text_area_exposes_projection_newline_fragments() {
        let content = "First line with a long value that can wrap when auto wrap is enabled.{{API_HOST}}/v1/users/{{USER_ID}}/activity/with/a/very/long/path\nTail line";
        let mut text_area = TextArea::new();
        text_area.content = content.to_string();
        text_area.font_size = 14.0;
        text_area.line_height = 1.25;
        text_area.auto_wrap = true;
        text_area.on_render_handler = Some(crate::ui::on_text_area_render(move |render| {
            let ranges = [(69..81), (91..102)];
            for range in ranges {
                let slice: String = content
                    .chars()
                    .skip(range.start)
                    .take(range.len())
                    .collect();
                render.range(range.clone(), move |_node| {
                    let slice = slice.clone();
                    crate::ui::RsxNode::tagged(
                        "Element",
                        crate::ui::RsxTagDescriptor::for_tag::<crate::view::tags::Element>(),
                    )
                    .with_prop(
                        "style",
                        crate::view::ElementStylePropSchema {
                            padding: Some(
                                crate::style::Padding::uniform(crate::style::Length::px(0.0))
                                    .x(crate::style::Length::px(20.0)),
                            ),
                            font_size: Some(crate::style::FontSize::Px(24.0)),
                            border: Some(crate::style::Border::uniform(
                                crate::style::Length::px(1.0),
                                &crate::style::Color::hex("#42566f"),
                            )),
                            ..Default::default()
                        },
                    )
                    .with_child(
                        crate::ui::RsxNode::tagged(
                            "Text",
                            crate::ui::RsxTagDescriptor::for_tag::<crate::view::tags::Text>(),
                        )
                        .with_child(crate::ui::RsxNode::text(slice)),
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

        arena.with_element_taken(root, |el, arena| {
            let text_area = el.as_any_mut().downcast_mut::<TextArea>().unwrap();
            text_area.measure_inline(
                InlineMeasureContext {
                    first_available_width: 360.0,
                    full_available_width: 360.0,
                    available_height: 600.0,
                    viewport_width: 360.0,
                    viewport_height: 600.0,
                    percent_base_width: Some(360.0),
                    percent_base_height: Some(600.0),
                },
                arena,
            );
            let fragments = text_area.get_inline_nodes_size(arena);
            assert!(
                fragments.len() >= 3,
                "projection + wrap + hard newline must expose visual fragments, got {fragments:?}",
            );
            let mut y = 0.0;
            for (idx, fragment) in fragments.iter().enumerate() {
                text_area.place_inline(
                    InlinePlacement {
                        node_index: idx,
                        x: 0.0,
                        y,
                        offset_x: 0.0,
                        offset_y: y,
                        parent_x: 0.0,
                        parent_y: 0.0,
                        visual_offset_x: 0.0,
                        visual_offset_y: 0.0,
                        available_width: 360.0,
                        available_height: 600.0,
                        viewport_width: 360.0,
                        viewport_height: 600.0,
                        percent_base_width: Some(360.0),
                        percent_base_height: Some(600.0),
                    },
                    arena,
                );
                y += fragment.height;
            }
        });

        arena.with_element_taken_ref(root, |el, arena| {
            let text_area = el.as_any().downcast_ref::<TextArea>().unwrap();
            let tail_run = text_area
                .children
                .iter()
                .copied()
                .find(|key| {
                    arena.get(*key).is_some_and(|node| {
                        node.element
                            .as_any()
                            .downcast_ref::<crate::view::base_component::text_area::TextAreaTextRun>()
                            .is_some_and(|run| run.text == "Tail line")
                    })
                })
                .expect("Tail line run");
            let first_child_y = arena
                .get(text_area.children[0])
                .expect("first child")
                .element
                .box_model_snapshot()
                .y;
            let tail_y = arena
                .get(tail_run)
                .expect("tail run")
                .element
                .box_model_snapshot()
                .y;
            assert!(
                tail_y > first_child_y + 1.0,
                "Tail line must be placed below the first visual row, first_y={first_child_y}, tail_y={tail_y}",
            );
        });
    }

    #[test]
    fn text_area_vertical_align_reaches_plain_text_runs_next_to_projection() {
        fn first_run_y(
            arena: &crate::view::node_arena::NodeArena,
            root: crate::view::node_arena::NodeKey,
        ) -> f32 {
            let root_node = arena.get(root).expect("TextArea root");
            let text_area = root_node
                .element
                .as_any()
                .downcast_ref::<TextArea>()
                .expect("TextArea root");
            text_area
                .children
                .iter()
                .find_map(|key| {
                    let node = arena.get(*key)?;
                    node.element
                        .as_any()
                        .is::<crate::view::base_component::text_area::TextAreaTextRun>()
                        .then(|| node.element.box_model_snapshot().y)
                })
                .expect("plain text run")
        }

        fn text_area_inline_alignments(
            arena: &crate::view::node_arena::NodeArena,
            root: crate::view::node_arena::NodeKey,
        ) -> Vec<crate::style::VerticalAlign> {
            let root_node = arena.get(root).expect("TextArea root");
            let text_area = root_node
                .element
                .as_any()
                .downcast_ref::<TextArea>()
                .expect("TextArea root");
            text_area
                .children
                .iter()
                .filter_map(|key| arena.get(*key))
                .filter_map(|node| node.element.get_inline_nodes_size(arena).first().cloned())
                .map(|node| node.vertical_align)
                .collect()
        }

        fn build_placed_text_area(
            vertical_align: crate::style::VerticalAlign,
        ) -> (
            crate::view::node_arena::NodeArena,
            crate::view::node_arena::NodeKey,
        ) {
            let content = "aaa{{BIG}}bbb";
            let mut text_area = TextArea::new();
            text_area.content = content.to_string();
            text_area.font_size = 14.0;
            text_area.line_height = 1.25;
            text_area.vertical_align = vertical_align;
            text_area.auto_wrap = true;
            text_area.on_render_handler = Some(crate::ui::on_text_area_render(move |render| {
                render.range(3..10, move |_node| {
                    crate::ui::RsxNode::tagged(
                        "Element",
                        crate::ui::RsxTagDescriptor::for_tag::<crate::view::tags::Element>(),
                    )
                    .with_prop(
                        "style",
                        crate::view::ElementStylePropSchema {
                            padding: Some(
                                crate::style::Padding::uniform(crate::style::Length::px(0.0))
                                    .x(crate::style::Length::px(20.0)),
                            ),
                            font_size: Some(crate::style::FontSize::Px(24.0)),
                            ..Default::default()
                        },
                    )
                    .with_child(
                        crate::ui::RsxNode::tagged(
                            "Text",
                            crate::ui::RsxTagDescriptor::for_tag::<crate::view::tags::Text>(),
                        )
                        .with_child(crate::ui::RsxNode::text("BIG")),
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
                    max_width: 240.0,
                    max_height: 120.0,
                    viewport_width: 240.0,
                    viewport_height: 120.0,
                    percent_base_width: Some(240.0),
                    percent_base_height: Some(120.0),
                },
                LayoutPlacement {
                    parent_x: 0.0,
                    parent_y: 0.0,
                    visual_offset_x: 0.0,
                    visual_offset_y: 0.0,
                    available_width: 240.0,
                    available_height: 120.0,
                    viewport_width: 240.0,
                    viewport_height: 120.0,
                    percent_base_width: Some(240.0),
                    percent_base_height: Some(120.0),
                },
            );
            (arena, root)
        }

        let (top_arena, top_root) = build_placed_text_area(crate::style::VerticalAlign::Top);
        let top_y = first_run_y(&top_arena, top_root);
        let (bottom_arena, bottom_root) =
            build_placed_text_area(crate::style::VerticalAlign::Bottom);
        let bottom_y = first_run_y(&bottom_arena, bottom_root);
        assert_eq!(
            text_area_inline_alignments(&bottom_arena, bottom_root),
            vec![
                crate::style::VerticalAlign::Bottom,
                crate::style::VerticalAlign::Bottom,
                crate::style::VerticalAlign::Bottom,
            ],
            "plain runs and projection segment must expose the same TextArea vertical_align",
        );
        assert!(
            bottom_y > top_y + 1.0,
            "plain TextArea run must move when vertical_align changes, top_y={top_y}, bottom_y={bottom_y}",
        );

        let (mut arena, root) = build_placed_text_area(crate::style::VerticalAlign::Top);
        let before_y = first_run_y(&arena, root);
        let viewport_style = crate::style::Style::new();
        let ctx = crate::view::fiber_work::ApplyContext {
            viewport_style: &viewport_style,
            viewport_width: 240.0,
            viewport_height: 120.0,
        };
        arena.with_element_taken(root, |element, arena_ref| {
            element.apply_prop(
                arena_ref,
                root,
                &ctx,
                "style",
                crate::ui::IntoPropValue::into_prop_value(crate::view::ElementStylePropSchema {
                    vertical_align: Some(crate::style::VerticalAlign::Bottom),
                    ..Default::default()
                }),
            );
        });
        crate::view::test_support::measure_and_place(
            &mut arena,
            root,
            LayoutConstraints {
                max_width: 240.0,
                max_height: 120.0,
                viewport_width: 240.0,
                viewport_height: 120.0,
                percent_base_width: Some(240.0),
                percent_base_height: Some(120.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 240.0,
                available_height: 120.0,
                viewport_width: 240.0,
                viewport_height: 120.0,
                percent_base_width: Some(240.0),
                percent_base_height: Some(120.0),
            },
        );
        let after_y = first_run_y(&arena, root);
        assert!(
            after_y > before_y + 1.0,
            "hot style update must recascade vertical_align into existing plain TextArea runs, before_y={before_y}, after_y={after_y}",
        );
    }

    #[test]
    fn textarea_test_bottom_aligns_wrapped_plain_fragments_with_projection_segments() {
        let content = "First line with a long value that can wrap when auto wrap is enabled.{{API_HOST}}/v1/users/{{USER_ID}}/activity/with/a/very/long/path\nTail line";
        let mut text_area = TextArea::new();
        text_area.content = content.to_string();
        text_area.font_size = 14.0;
        text_area.line_height = 1.25;
        text_area.vertical_align = crate::style::VerticalAlign::Bottom;
        text_area.auto_wrap = true;
        text_area.on_render_handler = Some(crate::ui::on_text_area_render(move |render| {
            for range in [69..81, 91..102] {
                let slice: String = content
                    .chars()
                    .skip(range.start)
                    .take(range.len())
                    .collect();
                render.range(range.clone(), move |_node| {
                    let slice = slice.clone();
                    crate::ui::RsxNode::tagged(
                        "Element",
                        crate::ui::RsxTagDescriptor::for_tag::<crate::view::tags::Element>(),
                    )
                    .with_prop(
                        "style",
                        crate::view::ElementStylePropSchema {
                            padding: Some(
                                crate::style::Padding::uniform(crate::style::Length::px(0.0))
                                    .x(crate::style::Length::px(20.0)),
                            ),
                            font_size: Some(crate::style::FontSize::Px(24.0)),
                            ..Default::default()
                        },
                    )
                    .with_child(
                        crate::ui::RsxNode::tagged(
                            "Text",
                            crate::ui::RsxTagDescriptor::for_tag::<crate::view::tags::Text>(),
                        )
                        .with_child(crate::ui::RsxNode::text(slice)),
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
                max_width: 420.0,
                max_height: 220.0,
                viewport_width: 420.0,
                viewport_height: 220.0,
                percent_base_width: Some(420.0),
                percent_base_height: Some(220.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 420.0,
                available_height: 220.0,
                viewport_width: 420.0,
                viewport_height: 220.0,
                percent_base_width: Some(420.0),
                percent_base_height: Some(220.0),
            },
        );

        let root_node = arena.get(root).expect("TextArea root");
        let text_area = root_node
            .element
            .as_any()
            .downcast_ref::<TextArea>()
            .expect("TextArea root");
        let mut enabled_fragment_y = None;
        let mut enabled_selection_y = None;
        let mut activity_fragment_y = None;
        let mut activity_selection_y = None;
        let mut api_segment_y = None;
        let mut user_segment_y = None;
        for key in &text_area.children {
            let node = arena.get(*key).expect("TextArea child");
            if let Some(run) = node
                .element
                .as_any()
                .downcast_ref::<crate::view::base_component::text_area::TextAreaTextRun>()
            {
                for (content, rect) in run.inline_text_pass_fragment_positions() {
                    if content.contains("enabled.") {
                        enabled_fragment_y = Some(rect.y);
                    }
                    if content.contains("/activity/with") {
                        activity_fragment_y = Some(rect.y);
                    }
                }
                if let Some(start) = run.text.find("enabled.") {
                    let rect = run
                        .local_selection_rects(start, start + "enabled.".len())
                        .into_iter()
                        .next()
                        .expect("enabled. selection rect");
                    enabled_selection_y = Some(node.element.box_model_snapshot().y + rect.y);
                }
                if let Some(start) = run.text.find("/activity/with") {
                    let rect = run
                        .local_selection_rects(start, start + "/activity/with".len())
                        .into_iter()
                        .next()
                        .expect("/activity selection rect");
                    activity_selection_y = Some(node.element.box_model_snapshot().y + rect.y);
                }
                if run.text.contains("/activity/with") {
                    activity_fragment_y = Some(node.element.box_model_snapshot().y);
                }
            } else if let Some(segment) = node
                .element
                .as_any()
                .downcast_ref::<crate::view::base_component::text_area::TextAreaProjectionSegment>()
            {
                match segment.char_range() {
                    range if range == (69..81) => {
                        api_segment_y = Some(node.element.box_model_snapshot().y);
                    }
                    range if range == (91..102) => {
                        user_segment_y = Some(node.element.box_model_snapshot().y);
                    }
                    _ => {}
                }
            }
        }

        let enabled_y = enabled_fragment_y.expect("enabled. fragment");
        let enabled_sel_y = enabled_selection_y.expect("enabled. selection");
        let api_y = api_segment_y.expect("{{API_HOST}} segment");
        let activity_y = activity_fragment_y.expect("/activity fragment");
        let activity_sel_y = activity_selection_y.expect("/activity selection");
        let user_y = user_segment_y.expect("{{USER_ID}} segment");
        assert!(
            enabled_y > api_y + 1.0,
            "Bottom align should place the shorter enabled. fragment below the taller API badge top, enabled_y={enabled_y}, api_y={api_y}",
        );
        assert!(
            (enabled_sel_y - enabled_y).abs() < 0.5,
            "Selection rect should follow bottom-aligned enabled. fragment, selection_y={enabled_sel_y}, fragment_y={enabled_y}",
        );
        assert!(
            activity_y > user_y + 1.0,
            "Bottom align should place the shorter /activity fragment below the taller USER_ID badge top, activity_y={activity_y}, user_y={user_y}",
        );
        assert!(
            (activity_sel_y - activity_y).abs() < 0.5,
            "Selection rect should follow bottom-aligned /activity fragment, selection_y={activity_sel_y}, fragment_y={activity_y}",
        );
    }

    #[test]
    fn fixed_text_area_projection_newline_places_tail_on_next_line() {
        let content = "First line with a long value that can wrap when auto wrap is enabled.{{API_HOST}}/v1/users/{{USER_ID}}/activity/with/a/very/long/path\nTail line";
        let mut text_area = TextArea::new();
        text_area.content = content.to_string();
        text_area.font_size = 14.0;
        text_area.line_height = 1.25;
        text_area.auto_wrap = true;
        text_area.on_render_handler = Some(crate::ui::on_text_area_render(move |render| {
            let ranges = [(69..81), (91..102)];
            for range in ranges {
                let slice: String = content
                    .chars()
                    .skip(range.start)
                    .take(range.len())
                    .collect();
                render.range(range.clone(), move |_node| {
                    let slice = slice.clone();
                    crate::ui::RsxNode::tagged(
                        "Element",
                        crate::ui::RsxTagDescriptor::for_tag::<crate::view::tags::Element>(),
                    )
                    .with_prop(
                        "style",
                        crate::view::ElementStylePropSchema {
                            padding: Some(
                                crate::style::Padding::uniform(crate::style::Length::px(0.0))
                                    .x(crate::style::Length::px(20.0)),
                            ),
                            font_size: Some(crate::style::FontSize::Px(24.0)),
                            border: Some(crate::style::Border::uniform(
                                crate::style::Length::px(1.0),
                                &crate::style::Color::hex("#42566f"),
                            )),
                            ..Default::default()
                        },
                    )
                    .with_child(
                        crate::ui::RsxNode::tagged(
                            "Text",
                            crate::ui::RsxTagDescriptor::for_tag::<crate::view::tags::Text>(),
                        )
                        .with_child(crate::ui::RsxNode::text(slice)),
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
                max_width: 342.0,
                max_height: 176.0,
                viewport_width: 342.0,
                viewport_height: 176.0,
                percent_base_width: Some(342.0),
                percent_base_height: Some(176.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 342.0,
                available_height: 176.0,
                viewport_width: 342.0,
                viewport_height: 176.0,
                percent_base_width: Some(342.0),
                percent_base_height: Some(176.0),
            },
        );

        arena.with_element_taken_ref(root, |el, arena| {
            let text_area = el.as_any().downcast_ref::<TextArea>().unwrap();
            let mut path_y = None;
            let mut path_fragment_count = None;
            let mut tail_y = None;
            let mut tail_x = None;
            for &child in &text_area.children {
                let Some(node) = arena.get(child) else {
                    continue;
                };
                let Some(run) = node
                    .element
                    .as_any()
                    .downcast_ref::<crate::view::base_component::text_area::TextAreaTextRun>()
                else {
                    continue;
                };
                if run.text.contains("/activity/") {
                    path_y = Some(node.element.box_model_snapshot().y);
                    path_fragment_count = Some(run.inline_paint_fragments.len());
                }
                if run.text == "Tail line" {
                    let snap = node.element.box_model_snapshot();
                    tail_x = Some(snap.x);
                    tail_y = Some(snap.y);
                }
            }
            let path_y = path_y.expect("path run");
            let path_fragment_count = path_fragment_count.expect("path run fragments");
            let tail_x = tail_x.expect("tail run x");
            let tail_y = tail_y.expect("tail run");
            assert_eq!(
                path_fragment_count, 1,
                "middle hard newline must not synthesize an extra blank fragment before Tail line",
            );
            assert!(
                tail_x <= 0.5,
                "hard newline must place Tail line at the beginning of the next line, tail_x={tail_x}",
            );
            assert!(
                tail_y > path_y + 1.0,
                "hard newline must place Tail line below path run, path_y={path_y}, tail_y={tail_y}",
            );
        });
    }

    #[test]
    fn fixed_wrapper_text_area_projection_newline_places_tail_on_next_line() {
        let content = "First line with a long value that can wrap when auto wrap is enabled.{{API_HOST}}/v1/users/{{USER_ID}}/activity/with/a/very/long/path\nTail line";
        let mut text_area = TextArea::new();
        text_area.content = content.to_string();
        text_area.font_size = 14.0;
        text_area.line_height = 1.25;
        text_area.auto_wrap = true;
        text_area.on_render_handler = Some(crate::ui::on_text_area_render(move |render| {
            let ranges = [(69..81), (91..102)];
            for range in ranges {
                let slice: String = content
                    .chars()
                    .skip(range.start)
                    .take(range.len())
                    .collect();
                render.range(range.clone(), move |_node| {
                    let slice = slice.clone();
                    crate::ui::RsxNode::tagged(
                        "Element",
                        crate::ui::RsxTagDescriptor::for_tag::<crate::view::tags::Element>(),
                    )
                    .with_prop(
                        "style",
                        crate::view::ElementStylePropSchema {
                            padding: Some(
                                crate::style::Padding::uniform(crate::style::Length::px(0.0))
                                    .x(crate::style::Length::px(20.0)),
                            ),
                            font_size: Some(crate::style::FontSize::Px(24.0)),
                            border: Some(crate::style::Border::uniform(
                                crate::style::Length::px(1.0),
                                &crate::style::Color::hex("#42566f"),
                            )),
                            ..Default::default()
                        },
                    )
                    .with_child(
                        crate::ui::RsxNode::tagged(
                            "Text",
                            crate::ui::RsxTagDescriptor::for_tag::<crate::view::tags::Text>(),
                        )
                        .with_child(crate::ui::RsxNode::text(slice)),
                    )
                });
            }
        }));

        let mut wrapper = crate::view::base_component::Element::new(0.0, 0.0, 0.0, 0.0);
        let mut wrapper_style = crate::style::Style::new();
        wrapper_style.insert(
            crate::style::PropertyId::Width,
            crate::style::ParsedValue::Length(crate::style::Length::px(360.0)),
        );
        wrapper_style.insert(
            crate::style::PropertyId::Height,
            crate::style::ParsedValue::Length(crate::style::Length::px(176.0)),
        );
        wrapper_style.insert(
            crate::style::PropertyId::PaddingTop,
            crate::style::ParsedValue::Length(crate::style::Length::px(8.0)),
        );
        wrapper_style.insert(
            crate::style::PropertyId::PaddingRight,
            crate::style::ParsedValue::Length(crate::style::Length::px(8.0)),
        );
        wrapper_style.insert(
            crate::style::PropertyId::PaddingBottom,
            crate::style::ParsedValue::Length(crate::style::Length::px(8.0)),
        );
        wrapper_style.insert(
            crate::style::PropertyId::PaddingLeft,
            crate::style::ParsedValue::Length(crate::style::Length::px(8.0)),
        );
        wrapper.apply_style(wrapper_style);

        let mut arena = crate::view::test_support::new_test_arena();
        let wrapper_key = crate::view::test_support::commit_element(
            &mut arena,
            Box::new(wrapper) as Box<dyn ElementTrait>,
        );
        let text_area_key =
            crate::view::test_support::commit_child(&mut arena, wrapper_key, Box::new(text_area));
        arena.with_element_taken(text_area_key, |el, _| {
            el.as_any_mut()
                .downcast_mut::<TextArea>()
                .expect("TextArea child")
                .set_self_node_key(text_area_key);
        });

        crate::view::test_support::measure_and_place(
            &mut arena,
            wrapper_key,
            LayoutConstraints {
                max_width: 800.0,
                max_height: 600.0,
                viewport_width: 800.0,
                viewport_height: 600.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 800.0,
                available_height: 600.0,
                viewport_width: 800.0,
                viewport_height: 600.0,
                percent_base_width: Some(800.0),
                percent_base_height: Some(600.0),
            },
        );

        arena.with_element_taken_ref(text_area_key, |el, arena| {
            let text_area = el.as_any().downcast_ref::<TextArea>().unwrap();
            let mut path_y = None;
            let mut tail_y = None;
            let mut tail_x = None;
            for &child in &text_area.children {
                let Some(node) = arena.get(child) else {
                    continue;
                };
                let Some(run) = node
                    .element
                    .as_any()
                    .downcast_ref::<crate::view::base_component::text_area::TextAreaTextRun>()
                else {
                    continue;
                };
                if run.text.contains("/activity/") {
                    path_y = Some(node.element.box_model_snapshot().y);
                }
                if run.text == "Tail line" {
                    let snap = node.element.box_model_snapshot();
                    tail_x = Some(snap.x);
                    tail_y = Some(snap.y);
                }
            }
            let path_y = path_y.expect("path run");
            let tail_x = tail_x.expect("tail run x");
            let tail_y = tail_y.expect("tail run y");
            assert!(
                tail_x <= 8.5,
                "Tail line must start at fixed wrapper inner left, tail_x={tail_x}",
            );
            assert!(
                tail_y > path_y + 1.0,
                "Tail line must sit below path after hard newline, path_y={path_y}, tail_y={tail_y}",
            );
        });
    }

    #[test]
    fn parent_relayout_scrolls_viewport_down_after_cursor_move() {
        let content = "one\ntwo\nthree\nfour\nfive";
        let mut text_area = TextArea::new();
        text_area.content = content.to_string();
        text_area.font_size = 14.0;
        text_area.line_height = 1.25;

        let mut arena = crate::view::test_support::new_test_arena();
        let mut parent = crate::view::base_component::Element::new(0.0, 0.0, 200.0, 35.0);
        let mut parent_style = crate::style::Style::new();
        parent_style.insert(
            crate::style::PropertyId::ScrollDirection,
            crate::style::ParsedValue::ScrollDirection(crate::style::ScrollDirection::Vertical),
        );
        parent_style.insert(
            crate::style::PropertyId::Layout,
            crate::style::ParsedValue::Layout(
                crate::style::Layout::flow().column().no_wrap().into(),
            ),
        );
        parent.apply_style(parent_style);
        let root = crate::view::test_support::commit_element(
            &mut arena,
            Box::new(parent) as Box<dyn ElementTrait>,
        );
        let spacer = crate::view::base_component::Element::new(0.0, 0.0, 200.0, 70.0);
        crate::view::test_support::commit_child(&mut arena, root, Box::new(spacer));
        let text_area_key =
            crate::view::test_support::commit_child(&mut arena, root, Box::new(text_area));
        arena.with_element_taken(text_area_key, |el, _| {
            el.as_any_mut()
                .downcast_mut::<TextArea>()
                .expect("TextArea child")
                .set_self_node_key(text_area_key);
        });

        crate::view::test_support::measure_and_place(
            &mut arena,
            root,
            LayoutConstraints {
                max_width: 200.0,
                max_height: 35.0,
                viewport_width: 200.0,
                viewport_height: 35.0,
                percent_base_width: Some(200.0),
                percent_base_height: Some(35.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 200.0,
                available_height: 35.0,
                viewport_width: 200.0,
                viewport_height: 35.0,
                percent_base_width: Some(200.0),
                percent_base_height: Some(35.0),
            },
        );

        arena.with_element_taken(text_area_key, |el, _| {
            el.as_any_mut()
                .downcast_mut::<TextArea>()
                .expect("TextArea child")
                .move_cursor_to(content.chars().count());
        });
        arena.refresh_subtree_dirty_cache(root);
        crate::view::test_support::measure_and_place(
            &mut arena,
            root,
            LayoutConstraints {
                max_width: 200.0,
                max_height: 35.0,
                viewport_width: 200.0,
                viewport_height: 35.0,
                percent_base_width: Some(200.0),
                percent_base_height: Some(35.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 200.0,
                available_height: 35.0,
                viewport_width: 200.0,
                viewport_height: 35.0,
                percent_base_width: Some(200.0),
                percent_base_height: Some(35.0),
            },
        );

        arena.with_element_taken_ref(text_area_key, |el, arena| {
            let text_area = el.as_any().downcast_ref::<TextArea>().unwrap();
            let (_, caret_y, caret_h) = text_area.caret_screen_position(arena).expect("caret");
            let viewport_bottom =
                text_area.layout_state.layout_position.y + text_area.viewport_size.height;

            assert!(text_area.scroll_y > 0.0);
            assert!(caret_y + caret_h <= viewport_bottom + 0.5);
        });
    }

    #[test]
    fn parent_relayout_scrolls_viewport_down_after_text_insert() {
        let mut text_area = TextArea::new();
        text_area.content = "one\ntwo\nthree".to_string();
        text_area.cursor_char = text_area.content.chars().count();
        text_area.font_size = 14.0;
        text_area.line_height = 1.25;

        let mut arena = crate::view::test_support::new_test_arena();
        let parent = crate::view::base_component::Element::new(0.0, 0.0, 200.0, 35.0);
        let root = crate::view::test_support::commit_element(
            &mut arena,
            Box::new(parent) as Box<dyn ElementTrait>,
        );
        let text_area_key =
            crate::view::test_support::commit_child(&mut arena, root, Box::new(text_area));
        arena.with_element_taken(text_area_key, |el, _| {
            el.as_any_mut()
                .downcast_mut::<TextArea>()
                .expect("TextArea child")
                .set_self_node_key(text_area_key);
        });

        let constraints = LayoutConstraints {
            max_width: 200.0,
            max_height: 35.0,
            viewport_width: 200.0,
            viewport_height: 35.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(35.0),
        };
        let placement = LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 200.0,
            available_height: 35.0,
            viewport_width: 200.0,
            viewport_height: 35.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(35.0),
        };
        crate::view::test_support::measure_and_place(&mut arena, root, constraints, placement);

        arena.with_element_taken(text_area_key, |el, _| {
            el.as_any_mut()
                .downcast_mut::<TextArea>()
                .expect("TextArea child")
                .insert_text("\nfour\nfive");
        });
        arena.refresh_subtree_dirty_cache(root);
        crate::view::test_support::measure_and_place(&mut arena, root, constraints, placement);

        arena.with_element_taken_ref(text_area_key, |el, arena| {
            let text_area = el.as_any().downcast_ref::<TextArea>().unwrap();
            let (_, caret_y, caret_h) = text_area.caret_screen_position(arena).expect("caret");
            let viewport_bottom =
                text_area.layout_state.layout_position.y + text_area.viewport_size.height;

            assert!(text_area.scroll_y > 0.0);
            assert!(caret_y + caret_h <= viewport_bottom + 0.5);
        });
    }

    #[test]
    fn parent_relayout_scrolls_viewport_down_after_repeated_cursor_moves() {
        let content = "one\ntwo\nthree\nfour\nfive";
        let mut text_area = TextArea::new();
        text_area.content = content.to_string();
        text_area.font_size = 14.0;
        text_area.line_height = 1.25;

        let mut arena = crate::view::test_support::new_test_arena();
        let parent = crate::view::base_component::Element::new(0.0, 0.0, 200.0, 35.0);
        let root = crate::view::test_support::commit_element(
            &mut arena,
            Box::new(parent) as Box<dyn ElementTrait>,
        );
        let text_area_key =
            crate::view::test_support::commit_child(&mut arena, root, Box::new(text_area));
        arena.with_element_taken(text_area_key, |el, _| {
            el.as_any_mut()
                .downcast_mut::<TextArea>()
                .expect("TextArea child")
                .set_self_node_key(text_area_key);
        });

        let constraints = LayoutConstraints {
            max_width: 200.0,
            max_height: 35.0,
            viewport_width: 200.0,
            viewport_height: 35.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(35.0),
        };
        let placement = LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 200.0,
            available_height: 35.0,
            viewport_width: 200.0,
            viewport_height: 35.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(35.0),
        };
        crate::view::test_support::measure_and_place(&mut arena, root, constraints, placement);

        let line_ends = [
            "one".chars().count(),
            "one\ntwo".chars().count(),
            "one\ntwo\nthree".chars().count(),
            "one\ntwo\nthree\nfour".chars().count(),
            content.chars().count(),
        ];
        for cursor in line_ends {
            arena.with_element_taken(text_area_key, |el, _| {
                el.as_any_mut()
                    .downcast_mut::<TextArea>()
                    .expect("TextArea child")
                    .move_cursor_to(cursor);
            });
            arena.refresh_subtree_dirty_cache(root);
            crate::view::test_support::measure_and_place(&mut arena, root, constraints, placement);
        }

        arena.with_element_taken_ref(text_area_key, |el, arena| {
            let text_area = el.as_any().downcast_ref::<TextArea>().unwrap();
            let (_, caret_y, caret_h) = text_area.caret_screen_position(arena).expect("caret");
            let viewport_bottom =
                text_area.layout_state.layout_position.y + text_area.viewport_size.height;

            assert_eq!(text_area.cursor_char, content.chars().count());
            assert!(text_area.scroll_y > 0.0);
            assert!(caret_y + caret_h <= viewport_bottom + 0.5);
        });
    }

    #[test]
    fn caret_follow_scrolls_vertical_parent_to_caret() {
        let content = "one\ntwo\nthree\nfour\nfive";
        let mut text_area = TextArea::new();
        text_area.content = content.to_string();
        text_area.cursor_char = content.chars().count();
        text_area.font_size = 14.0;
        text_area.line_height = 1.25;
        text_area.pending_caret_scroll = true;

        let mut arena = crate::view::test_support::new_test_arena();
        let mut parent = crate::view::base_component::Element::new(0.0, 0.0, 200.0, 35.0);
        let mut parent_style = crate::style::Style::new();
        parent_style.insert(
            crate::style::PropertyId::ScrollDirection,
            crate::style::ParsedValue::ScrollDirection(crate::style::ScrollDirection::Vertical),
        );
        parent_style.insert(
            crate::style::PropertyId::Layout,
            crate::style::ParsedValue::Layout(crate::style::Layout::flex().column().into()),
        );
        parent.apply_style(parent_style);
        let root = crate::view::test_support::commit_element(
            &mut arena,
            Box::new(parent) as Box<dyn ElementTrait>,
        );
        let spacer = crate::view::base_component::Element::new(0.0, 0.0, 200.0, 70.0);
        crate::view::test_support::commit_child(&mut arena, root, Box::new(spacer));
        let text_area_key =
            crate::view::test_support::commit_child(&mut arena, root, Box::new(text_area));
        arena.with_element_taken(text_area_key, |el, _| {
            el.as_any_mut()
                .downcast_mut::<TextArea>()
                .expect("TextArea child")
                .set_self_node_key(text_area_key);
        });

        let constraints = LayoutConstraints {
            max_width: 200.0,
            max_height: 35.0,
            viewport_width: 200.0,
            viewport_height: 35.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(35.0),
        };
        let placement = LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 200.0,
            available_height: 35.0,
            viewport_width: 200.0,
            viewport_height: 35.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(35.0),
        };

        crate::view::test_support::measure_and_place(&mut arena, root, constraints, placement);
        arena.with_element_taken(text_area_key, |el, _| {
            el.as_any_mut()
                .downcast_mut::<TextArea>()
                .expect("TextArea child")
                .move_cursor_to(content.chars().count());
        });
        arena.with_element_taken(root, |el, _| {
            el.as_any_mut()
                .downcast_mut::<crate::view::base_component::Element>()
                .expect("parent")
                .layout_state
                .content_size
                .height = 160.0;
        });
        arena.with_element_taken(text_area_key, |el, arena| {
            el.as_any_mut()
                .downcast_mut::<TextArea>()
                .expect("TextArea child")
                .scroll_caret_into_view(arena);
        });
        let parent_scroll = arena
            .with_element_taken_ref(root, |el, _| el.get_scroll_offset())
            .expect("parent scroll");
        assert!(parent_scroll.1 > 0.0, "parent_scroll={parent_scroll:?}");

        arena.with_element_taken(root, |el, arena| {
            el.place(placement, arena);
        });
        arena.with_element_taken_ref(text_area_key, |el, arena| {
            let text_area = el.as_any().downcast_ref::<TextArea>().unwrap();
            let (_, caret_y, caret_h) = text_area.caret_screen_position(arena).expect("caret");
            assert!(caret_y >= -0.5);
            assert!(caret_y + caret_h <= 35.5);
        });
    }

    #[test]
    fn caret_follow_scrolls_horizontal_parent_to_caret() {
        let content = "abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz";
        let mut text_area = TextArea::new();
        text_area.content = content.to_string();
        text_area.font_size = 14.0;
        text_area.line_height = 1.25;
        text_area.auto_wrap = false;

        let mut arena = crate::view::test_support::new_test_arena();
        let mut parent = crate::view::base_component::Element::new(0.0, 0.0, 80.0, 40.0);
        let mut parent_style = crate::style::Style::new();
        parent_style.insert(
            crate::style::PropertyId::ScrollDirection,
            crate::style::ParsedValue::ScrollDirection(crate::style::ScrollDirection::Horizontal),
        );
        parent_style.insert(
            crate::style::PropertyId::Layout,
            crate::style::ParsedValue::Layout(crate::style::Layout::flex().row().into()),
        );
        parent.apply_style(parent_style);
        let root = crate::view::test_support::commit_element(
            &mut arena,
            Box::new(parent) as Box<dyn ElementTrait>,
        );
        let spacer = crate::view::base_component::Element::new(0.0, 0.0, 120.0, 40.0);
        crate::view::test_support::commit_child(&mut arena, root, Box::new(spacer));
        let text_area_key =
            crate::view::test_support::commit_child(&mut arena, root, Box::new(text_area));
        arena.with_element_taken(text_area_key, |el, _| {
            el.as_any_mut()
                .downcast_mut::<TextArea>()
                .expect("TextArea child")
                .set_self_node_key(text_area_key);
        });

        let constraints = LayoutConstraints {
            max_width: 80.0,
            max_height: 40.0,
            viewport_width: 80.0,
            viewport_height: 40.0,
            percent_base_width: Some(80.0),
            percent_base_height: Some(40.0),
        };
        let placement = LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 80.0,
            available_height: 40.0,
            viewport_width: 80.0,
            viewport_height: 40.0,
            percent_base_width: Some(80.0),
            percent_base_height: Some(40.0),
        };

        crate::view::test_support::measure_and_place(&mut arena, root, constraints, placement);
        arena.with_element_taken(root, |el, _| {
            el.as_any_mut()
                .downcast_mut::<crate::view::base_component::Element>()
                .expect("parent")
                .layout_state
                .content_size
                .width = 280.0;
        });
        crate::view::base_component::scroll_rect_into_view_from(
            &arena,
            text_area_key,
            crate::ui::Rect::new(240.0, 0.0, 1.0, 18.0),
            crate::ui::ScrollIntoViewOptions::default(),
            false,
            true,
        );
        let parent_scroll = arena
            .with_element_taken_ref(root, |el, _| el.get_scroll_offset())
            .expect("parent scroll");
        assert!(parent_scroll.0 > 0.0, "parent_scroll={parent_scroll:?}");
        assert!(240.0 - parent_scroll.0 >= -0.5);
        assert!(241.0 - parent_scroll.0 <= 80.5);
    }

    #[test]
    fn short_content_hit_test_extends_to_viewport_width() {
        let content = "hi";
        let (arena, root) = placed_text_area(content, 0, 300.0, 40.0, true);

        arena.with_element_taken_ref(root, |el, arena| {
            let text_area = el.as_any().downcast_ref::<TextArea>().unwrap();
            assert!(
                text_area.layout_state.layout_size.width < 250.0,
                "fixture should keep content width narrower than the click x",
            );
            assert!(
                text_area.box_model_snapshot().width >= 300.0,
                "TextArea hit box should extend to its viewport width",
            );
            let target = text_area.cursor_target_at_screen(arena, 250.0, 8.0);
            assert_eq!(
                target.char_index,
                content.chars().count(),
                "clicking to the right of short content should place caret at line tail",
            );
        });

        assert_eq!(
            hit_test(&arena, root, 250.0, 8.0),
            Some(root),
            "TextArea should receive pointer hits across its configured width",
        );
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
