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

mod dirty_flag_tests;
mod projection_wrap_tests;
mod projection_alignment_tests;
mod projection_newline_tests;
mod auto_height_tests;
mod caret_follow_tests;
mod viewport_metrics_tests;
mod selection_geometry_tests;
