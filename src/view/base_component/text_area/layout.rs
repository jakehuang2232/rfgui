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
mod tests {
    use super::*;
    use crate::view::base_component::{DirtyFlags, ElementTrait, Layoutable, hit_test};

    fn placement_dirty_flags() -> DirtyFlags {
        DirtyFlags::PLACE
            .union(DirtyFlags::BOX_MODEL)
            .union(DirtyFlags::HIT_TEST)
    }

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
    fn text_area_measure_and_place_clear_local_layout_dirty_flags() {
        let mut text_area = TextArea::new();
        text_area.content = "dirty flag contract".to_string();
        text_area.font_size = 14.0;
        text_area.line_height = 1.25;

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

        let constraints = LayoutConstraints {
            max_width: 180.0,
            max_height: 80.0,
            viewport_width: 180.0,
            viewport_height: 80.0,
            percent_base_width: Some(180.0),
            percent_base_height: Some(80.0),
        };
        let placement = LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 180.0,
            available_height: 80.0,
            viewport_width: 180.0,
            viewport_height: 80.0,
            percent_base_width: Some(180.0),
            percent_base_height: Some(80.0),
        };

        arena.with_element_taken(root, |el, arena| {
            el.measure(constraints, arena);
        });
        {
            let measured = crate::view::test_support::get_element::<TextArea>(&arena, root);
            assert!(!measured.local_dirty_flags().intersects(DirtyFlags::LAYOUT));
            assert!(measured.local_dirty_flags().intersects(DirtyFlags::PLACE));
        }

        arena.with_element_taken(root, |el, arena| {
            el.place(placement, arena);
        });
        {
            let placed = crate::view::test_support::get_element::<TextArea>(&arena, root);
            assert!(
                !placed
                    .local_dirty_flags()
                    .intersects(placement_dirty_flags())
            );
        }
    }

    #[test]
    fn text_area_projection_segment_measure_and_place_clear_layout_dirty_flags() {
        let mut segment = super::super::TextAreaProjectionSegment::new();
        let mut arena = crate::view::test_support::new_test_arena();
        let constraints = LayoutConstraints {
            max_width: 120.0,
            max_height: 40.0,
            viewport_width: 120.0,
            viewport_height: 40.0,
            percent_base_width: Some(120.0),
            percent_base_height: Some(40.0),
        };
        let placement = LayoutPlacement {
            parent_x: 8.0,
            parent_y: 12.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 120.0,
            available_height: 40.0,
            viewport_width: 120.0,
            viewport_height: 40.0,
            percent_base_width: Some(120.0),
            percent_base_height: Some(40.0),
        };

        segment.measure(constraints, &mut arena);
        assert!(!segment.local_dirty_flags().intersects(DirtyFlags::LAYOUT));
        assert!(segment.local_dirty_flags().intersects(DirtyFlags::PLACE));

        segment.place(placement, &mut arena);
        assert!(
            !segment
                .local_dirty_flags()
                .intersects(placement_dirty_flags())
        );
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
    fn place_preserves_parent_assigned_height_for_vertical_caret_follow() {
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
            let text_area = el.as_any_mut().downcast_mut::<TextArea>().unwrap();
            text_area.set_self_node_key(root);
        });
        arena.with_element_taken(root, |el, arena| {
            el.measure(
                LayoutConstraints {
                    max_width: 200.0,
                    max_height: 600.0,
                    viewport_width: 200.0,
                    viewport_height: 600.0,
                    percent_base_width: Some(200.0),
                    percent_base_height: Some(600.0),
                },
                arena,
            );
            el.set_layout_height(35.0);
            el.place(
                LayoutPlacement {
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
                },
                arena,
            );
        });

        arena.with_element_taken_ref(root, |el, arena| {
            let text_area = el.as_any().downcast_ref::<TextArea>().unwrap();
            let (_, caret_y, caret_h) = text_area.caret_screen_position(arena).expect("caret");
            assert_eq!(text_area.viewport_size.height, 35.0);
            assert!(text_area.scroll_y > 0.0);
            assert!(caret_y + caret_h <= 35.5);
        });
    }

    #[test]
    fn text_area_places_projection_tail_line_below_wrapped_rows() {
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
                max_height: 600.0,
                viewport_width: 360.0,
                viewport_height: 600.0,
                percent_base_width: Some(360.0),
                percent_base_height: Some(600.0),
            },
            LayoutPlacement {
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
        );

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
        let mut api_segment_y = None;
        let mut user_segment_y = None;
        for key in &text_area.children {
            let node = arena.get(*key).expect("TextArea child");
            if let Some(segment) = node
                .element
                .as_any()
                .downcast_ref::<crate::view::base_component::text_area::TextAreaProjectionSegment>(
            ) {
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

        // Text geometry comes from the unified package's selection rects,
        // whose y is the staged glyph paint position — i.e. the aligned
        // text position. (Selection and glyphs share one shaping now, so
        // the old cross-pipeline "selection follows fragment" assertion is
        // structural and no longer tested separately.)
        let package = text_area
            .unified_inline_ifc_render_package(&arena)
            .expect("unified package");
        let origin_y = text_area.layout_state.layout_position.y - text_area.scroll_y;
        let text_y = |needle: &str| {
            let byte = content.find(needle).expect("needle in content");
            let start = content[..byte].chars().count();
            let rect = package
                .selection_rects_for_char_range(start..start + needle.chars().count())
                .into_iter()
                .next()
                .expect("selection rect for needle");
            origin_y + rect.y
        };

        let enabled_y = text_y("enabled.");
        let api_y = api_segment_y.expect("{{API_HOST}} segment");
        let activity_y = text_y("/activity/with");
        let user_y = user_segment_y.expect("{{USER_ID}} segment");
        assert!(
            enabled_y > api_y + 1.0,
            "Bottom align should place the shorter enabled. fragment below the taller API badge top, enabled_y={enabled_y}, api_y={api_y}",
        );
        assert!(
            activity_y > user_y + 1.0,
            "Bottom align should place the shorter /activity fragment below the taller USER_ID badge top, activity_y={activity_y}, user_y={user_y}",
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
            let mut path_fragment_count = 0usize;
            let mut path_bottom = None;
            let mut path_has_empty_fragment = false;
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
                    path_fragment_count = run.inline_paint_fragments.len();
                    path_bottom = run
                        .inline_paint_fragments
                        .iter()
                        .map(|fragment| fragment.y + fragment.height)
                        .reduce(f32::max);
                    path_has_empty_fragment = run
                        .inline_paint_fragments
                        .iter()
                        .any(|fragment| fragment.width <= 0.5 || fragment.height <= 0.5);
                }
                if run.text == "Tail line" {
                    let snap = node.element.box_model_snapshot();
                    tail_x = Some(snap.x);
                    tail_y = Some(snap.y);
                }
            }
            let path_y = path_y.expect("path run");
            let path_bottom = path_bottom.expect("path run bottom");
            let tail_x = tail_x.expect("tail run x");
            let tail_y = tail_y.expect("tail run");
            assert!(
                path_fragment_count >= 1,
                "path run should expose at least one root visual fragment",
            );
            assert!(
                !path_has_empty_fragment,
                "middle hard newline must not synthesize an empty fragment before Tail line",
            );
            assert!(
                tail_x <= 0.5,
                "hard newline must place Tail line at the beginning of the next line, tail_x={tail_x}",
            );
            assert!(
                tail_y + 0.5 >= path_bottom,
                "hard newline must place Tail line below path run, path_y={path_y}, path_bottom={path_bottom}, tail_y={tail_y}",
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
        crate::view::viewport::dispatch::scroll_rect_into_view_from(
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

    #[test]
    fn unified_selection_rects_align_with_painted_text_band() {
        use crate::view::base_component::ElementTrait;
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
            let package = text_area
                .unified_inline_ifc_render_package(arena)
                .expect("unified package");

            // Selection band for the leading committed text must overlap
            // the painted text band of the run's first fragment.
            let needle = "First line";
            let rects =
                package.selection_rects_for_char_range(0..needle.chars().count());
            assert!(!rects.is_empty(), "selection rects for leading text");
            let selection = rects[0];
            drop(package);

            let mut first_fragment: Option<crate::ui::Rect> = None;
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
                if run.text.starts_with("First line") {
                    first_fragment = run.inline_paint_fragments.first().copied();
                }
            }
            let fragment = first_fragment.expect("first run fragment");
            // Selection rects are in content coords; fragments are
            // absolute (origin 0,0 here, so directly comparable).
            let sel_top = selection.y;
            let sel_bottom = selection.y + selection.height;
            let frag_top = fragment.y;
            let frag_bottom = fragment.y + fragment.height;
            let overlap =
                sel_bottom.min(frag_bottom) - sel_top.max(frag_top);
            assert!(
                overlap >= fragment.height * 0.6,
                "selection band must cover the painted text band: selection=({sel_top}, {sel_bottom}) fragment=({frag_top}, {frag_bottom})"
            );
        });
    }

    #[test]
    fn typing_with_projections_keeps_caret_at_insertion_point() {
        for cursor in [10_usize, 68, 69, 70, 81, 82, 90, 91, 102, 103] {
            typing_with_projections_keeps_caret_at_insertion_point_at(cursor);
        }
    }

    fn projection_fixture_text_area(content: String, cursor_char: usize) -> TextArea {
        let mut text_area = TextArea::new();
        text_area.content = content;
        text_area.font_size = 14.0;
        text_area.line_height = 1.25;
        text_area.auto_wrap = true;
        text_area.cursor_char = cursor_char;
        text_area.on_render_handler = Some(crate::ui::on_text_area_render(move |render| {
            let chars: Vec<char> = render.content().chars().collect();
            let mut ranges = Vec::new();
            let mut index = 0_usize;
            while index + 1 < chars.len() {
                if chars[index] == '{' && chars[index + 1] == '{' {
                    let start = index;
                    let mut cursor = index + 2;
                    while cursor + 1 < chars.len() {
                        if chars[cursor] == '}' && chars[cursor + 1] == '}' {
                            ranges.push(start..cursor + 2);
                            index = cursor + 2;
                            break;
                        }
                        cursor += 1;
                    }
                    if cursor + 1 >= chars.len() {
                        break;
                    }
                    continue;
                }
                index += 1;
            }
            for range in ranges {
                let slice: String = chars[range.clone()].iter().collect();
                render.range(range, move |_node| {
                    let slice = slice.clone();
                    crate::ui::RsxNode::tagged(
                        "Element",
                        crate::ui::RsxTagDescriptor::for_tag::<crate::view::tags::Element>(),
                    )
                    .with_prop(
                        "style",
                        crate::view::ElementStylePropSchema {
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
        text_area
    }

    fn typing_with_projections_keeps_caret_at_insertion_point_at(cursor_char: usize) {
        use crate::view::base_component::ElementTrait;
        let content = "First line with a long value that can wrap when auto wrap is enabled.{{API_HOST}}/v1/users/{{USER_ID}}/activity/with/a/very/long/path\nTail line";
        let text_area = projection_fixture_text_area(content.to_string(), cursor_char);

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
        let constraints = LayoutConstraints {
            max_width: 342.0,
            max_height: 176.0,
            viewport_width: 342.0,
            viewport_height: 176.0,
            percent_base_width: Some(342.0),
            percent_base_height: Some(176.0),
        };
        let placement = LayoutPlacement {
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
        };
        crate::view::test_support::measure_and_place(&mut arena, root, constraints, placement);

        arena.with_element_taken(root, |el, _| {
            let text_area = el.as_any_mut().downcast_mut::<TextArea>().unwrap();
            assert!(text_area.insert_text("X"), "insert should succeed");
        });
        crate::view::test_support::measure_and_place(&mut arena, root, constraints, placement);

        let (caret_after, post_content, post_cursor) = arena
            .with_element_taken_ref(root, |el, arena| {
                let text_area = el.as_any().downcast_ref::<TextArea>().unwrap();
                (
                    text_area.caret_screen_position(arena),
                    text_area.content.clone(),
                    text_area.cursor_char,
                )
            })
            .expect("root");
        let caret_after = caret_after.expect("caret after typing");

        // Oracle: a fresh fixture laid out with the post-edit content and
        // the same cursor gives the ground-truth caret position.
        let oracle = projection_fixture_text_area(post_content, post_cursor);
        let mut oracle_arena = crate::view::test_support::new_test_arena();
        let oracle_root = crate::view::test_support::commit_element(
            &mut oracle_arena,
            Box::new(oracle) as Box<dyn ElementTrait>,
        );
        oracle_arena.with_element_taken(oracle_root, |el, _| {
            el.as_any_mut()
                .downcast_mut::<TextArea>()
                .expect("oracle root")
                .set_self_node_key(oracle_root);
        });
        crate::view::test_support::measure_and_place(
            &mut oracle_arena,
            oracle_root,
            constraints,
            placement,
        );
        let expected = oracle_arena
            .with_element_taken_ref(oracle_root, |el, arena| {
                el.as_any()
                    .downcast_ref::<TextArea>()
                    .unwrap()
                    .caret_screen_position(arena)
            })
            .expect("oracle root")
            .expect("oracle caret");

        let dx = (caret_after.0 - expected.0).abs();
        let dy = (caret_after.1 - expected.1).abs();
        assert!(
            dx < 1.0 && dy < 1.0,
            "cursor_char={cursor_char}: incremental caret must match a fresh layout: incremental={caret_after:?} fresh={expected:?}"
        );
    }

    #[test]
    fn arrow_right_traverses_projection_in_reading_order() {
        for width in [342.0_f32, 300.0, 240.0, 180.0] {
            arrow_right_traverses_projection_in_reading_order_at(width);
        }
    }

    fn arrow_right_traverses_projection_in_reading_order_at(width: f32) {
        use crate::view::base_component::ElementTrait;
        let content = "First line with a long value that can wrap when auto wrap is enabled.{{API_HOST}}/v1/users/{{USER_ID}}/activity/with/a/very/long/path\nTail line";
        let text_area = projection_fixture_text_area(content.to_string(), 67);

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
        let constraints = LayoutConstraints {
            max_width: width,
            max_height: 176.0,
            viewport_width: width,
            viewport_height: 176.0,
            percent_base_width: Some(width),
            percent_base_height: Some(176.0),
        };
        let placement = LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: width,
            available_height: 176.0,
            viewport_width: width,
            viewport_height: 176.0,
            percent_base_width: Some(width),
            percent_base_height: Some(176.0),
        };
        crate::view::test_support::measure_and_place(&mut arena, root, constraints, placement);

        arena.with_element_taken(root, |el, arena| {
            let text_area = el.as_any_mut().downcast_mut::<TextArea>().unwrap();
            let mut trail: Vec<(usize, f32, f32)> = Vec::new();
            let start = text_area
                .caret_screen_position(arena)
                .expect("caret at start");
            trail.push((text_area.cursor_char, start.0, start.1));
            for _ in 0..18 {
                if !text_area.handle_horizontal_arrow(arena, true) {
                    break;
                }
                let (x, y, _) = text_area
                    .caret_screen_position(arena)
                    .expect("caret after arrow");
                trail.push((text_area.cursor_char, x, y));
            }
            // Reading order: within a visual line (same y band), repeated
            // ArrowRight must never move the caret left.
            for pair in trail.windows(2) {
                let (c0, x0, y0) = pair[0];
                let (c1, x1, y1) = pair[1];
                if (y1 - y0).abs() < 6.0 {
                    assert!(
                        x1 >= x0 - 0.5,
                        "width {width}: ArrowRight moved caret left within a line: {c0}@({x0},{y0}) -> {c1}@({x1},{y1}); trail={trail:?}"
                    );
                }
            }
        });
    }
}
