use super::*;
use crate::style::{Layout, Length, ParsedValue, PropertyId, ScrollDirection, Style};
use crate::ui::{RsxNode, RsxTagDescriptor};
use crate::view::ElementStylePropSchema;
use crate::view::base_component::{
    Element, ElementTrait, EventTarget, LayoutConstraints, LayoutPlacement, Size, Text,
};
use crate::view::frame_graph::FrameGraph;
use std::cell::Cell;
use std::rc::Rc;




fn projection_fixture(cursor_char: usize, with_text_child: bool) -> (NodeArena, NodeKey) {
    let mut text_area = TextArea::new();
    text_area.content = "abXYZcd".to_string();
    text_area.font_size = 14.0;
    text_area.line_height = 1.25;
    text_area.cursor_char = cursor_char;
    text_area.on_render_handler = Some(crate::ui::on_text_area_render(move |render| {
        render.range(2..5, |_text_area_node| {
            let style = ElementStylePropSchema {
                width: Some(Length::px(90.0)),
                height: Some(Length::px(42.0)),
                ..Default::default()
            };
            let node = RsxNode::tagged(
                "Element",
                RsxTagDescriptor::for_tag::<crate::view::tags::Element>(),
            )
            .with_prop("style", style);
            if with_text_child {
                node.with_child(
                    RsxNode::tagged(
                        "Text",
                        RsxTagDescriptor::for_tag::<crate::view::tags::Text>(),
                    )
                    .with_child(RsxNode::text("XYZ")),
                )
            } else {
                node
            }
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
            max_width: 300.0,
            max_height: 300.0,
            viewport_width: 300.0,
            viewport_height: 300.0,
            percent_base_width: None,
            percent_base_height: None,
        },
        LayoutPlacement {
            parent_x: 10.0,
            parent_y: 20.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 300.0,
            available_height: 300.0,
            viewport_width: 300.0,
            viewport_height: 300.0,
            percent_base_width: None,
            percent_base_height: None,
        },
    );
    (arena, root)
}

fn retained_atomic_projection_fixture_with(
    content: &str,
    projection_range: std::ops::Range<usize>,
    projected_text: &str,
) -> (NodeArena, NodeKey, NodeKey, NodeKey, Rc<Cell<usize>>) {
    retained_atomic_projection_fixture_with_selection(
        content,
        projection_range,
        projected_text,
        None,
    )
}

fn retained_atomic_projection_fixture_with_selection(
    content: &str,
    projection_range: std::ops::Range<usize>,
    projected_text: &str,
    selection: Option<(usize, usize)>,
) -> (NodeArena, NodeKey, NodeKey, NodeKey, Rc<Cell<usize>>) {
    let call_count = Rc::new(Cell::new(0));
    let handler_call_count = Rc::clone(&call_count);
    let projected_text = projected_text.to_string();
    let mut text_area = TextArea::new();
    text_area.content = content.to_string();
    text_area.font_size = 14.0;
    text_area.line_height = 1.25;
    if let Some((anchor, focus)) = selection {
        text_area.selection_anchor_char = Some(anchor);
        text_area.selection_focus_char = Some(focus);
    }
    text_area.on_render_handler = Some(crate::ui::on_text_area_render(move |render| {
        handler_call_count.set(handler_call_count.get() + 1);
        let projected_text = projected_text.clone();
        render.range(projection_range.clone(), move |_text_area_node| {
            RsxNode::text(projected_text)
        });
    }));

    let mut arena = crate::view::test_support::new_test_arena();
    let root = crate::view::test_support::commit_element(
        &mut arena,
        Box::new(text_area) as Box<dyn ElementTrait>,
    );
    arena.with_element_taken(root, |element, _| {
        element
            .as_any_mut()
            .downcast_mut::<TextArea>()
            .expect("TextArea root")
            .set_self_node_key(root);
    });
    crate::view::test_support::measure_and_place(
        &mut arena,
        root,
        LayoutConstraints {
            max_width: 132.0,
            max_height: 240.0,
            viewport_width: 320.0,
            viewport_height: 240.0,
            percent_base_width: Some(320.0),
            percent_base_height: Some(240.0),
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 132.0,
            available_height: 240.0,
            viewport_width: 320.0,
            viewport_height: 240.0,
            percent_base_width: Some(320.0),
            percent_base_height: Some(240.0),
        },
    );
    let projection = arena
        .children_of(root)
        .into_iter()
        .find(|&child| {
            arena
                .get(child)
                .is_some_and(|node| node.element.as_any().is::<TextAreaProjectionSegment>())
        })
        .expect("fixture must realize one projection segment");
    let projection_children = arena.children_of(projection);
    let [projection_text] = projection_children.as_slice() else {
        panic!("fixture projection must own one direct Text leaf")
    };
    let projection_text = *projection_text;
    assert!(
        arena
            .get(projection_text)
            .unwrap()
            .element
            .as_any()
            .is::<Text>()
    );
    let mut stack = vec![root];
    while let Some(key) = stack.pop() {
        stack.extend(arena.children_of(key));
        arena
            .get_mut(key)
            .unwrap()
            .element
            .clear_local_dirty_flags(DirtyFlags::ALL);
    }
    arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
    arena.refresh_subtree_dirty_cache(root);
    (arena, root, projection, projection_text, call_count)
}

fn retained_atomic_projection_fixture()
-> (NodeArena, NodeKey, NodeKey, NodeKey, Rc<Cell<usize>>) {
    retained_atomic_projection_fixture_with("before projected after", 7..16, "projected")
}













fn retained_atomic_projection_scroll_shell() -> (NodeArena, NodeKey, NodeKey, NodeKey) {
    let (mut arena, text_area, ..) = retained_atomic_projection_fixture();
    let outer_scroll_y = 20.0;
    let wrapper = crate::view::test_support::commit_element(
        &mut arena,
        Box::new(Element::new_with_id(
            0xc3a_2001,
            0.0,
            -outer_scroll_y,
            132.0,
            300.0,
        )),
    );
    let root = crate::view::test_support::commit_element(
        &mut arena,
        Box::new(Element::new_with_id(0xc3a_2000, 0.0, 0.0, 132.0, 80.0)),
    );
    arena.set_parent(text_area, Some(wrapper));
    arena.set_children(wrapper, vec![text_area]);
    arena.set_parent(wrapper, Some(root));
    arena.set_children(root, vec![wrapper]);
    arena.with_element_taken(text_area, |element, arena| {
        element.place(
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: -outer_scroll_y,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 132.0,
                available_height: 240.0,
                viewport_width: 320.0,
                viewport_height: 240.0,
                percent_base_width: Some(320.0),
                percent_base_height: Some(240.0),
            },
            arena,
        );
    });
    let mut wrapper_style = Style::new();
    wrapper_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    crate::view::test_support::get_element_mut::<Element>(&arena, wrapper)
        .apply_style(wrapper_style);
    let mut root_style = Style::new();
    root_style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(ScrollDirection::Vertical),
    );
    root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    {
        let mut root_element =
            crate::view::test_support::get_element_mut::<Element>(&arena, root);
        root_element.apply_style(root_style);
        root_element.layout_state.content_size = Size {
            width: 132.0,
            height: 300.0,
        };
        root_element.set_scroll_offset((0.0, outer_scroll_y));
        root_element
            .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    }
    arena
        .get_mut(wrapper)
        .unwrap()
        .element
        .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    let mut stack = vec![text_area];
    while let Some(key) = stack.pop() {
        stack.extend(arena.children_of(key));
        arena
            .get_mut(key)
            .unwrap()
            .element
            .clear_local_dirty_flags(DirtyFlags::ALL);
    }
    arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
    arena.refresh_subtree_dirty_cache(root);
    (arena, root, wrapper, text_area)
}







fn caret_position(arena: &NodeArena, root: NodeKey) -> (f32, f32, f32) {
    arena
        .with_element_taken_ref(root, |el, arena| {
            el.as_any()
                .downcast_ref::<TextArea>()
                .expect("TextArea root")
                .caret_screen_position(arena)
                .expect("caret position")
        })
        .expect("root exists")
}

fn projection_key(arena: &NodeArena, root: NodeKey) -> NodeKey {
    arena.children_of(root)[1]
}

fn projection_snapshot(
    arena: &NodeArena,
    root: NodeKey,
) -> crate::view::base_component::BoxModelSnapshot {
    let projection_key = projection_key(arena, root);
    arena
        .get(projection_key)
        .expect("projection child")
        .element
        .box_model_snapshot()
}

fn first_text_descendant(arena: &NodeArena, root: NodeKey) -> NodeKey {
    let mut stack: Vec<NodeKey> = arena.children_of(root).into_iter().rev().collect();
    while let Some(key) = stack.pop() {
        if arena
            .get(key)
            .is_some_and(|node| node.element.as_any().is::<Text>())
        {
            return key;
        }
        for child in arena.children_of(key).into_iter().rev() {
            stack.push(child);
        }
    }
    panic!("expected Text descendant");
}

fn snapshot(arena: &NodeArena, key: NodeKey) -> crate::view::base_component::BoxModelSnapshot {
    arena.get(key).expect("node").element.box_model_snapshot()
}

fn plain_preedit_fixture(content: &str, cursor_char: usize) -> (NodeArena, NodeKey) {
    plain_preedit_fixture_with_options(
        content,
        cursor_char,
        "\u{4E2D}",
        Some((3, 3)),
        super::super::caret_map::CaretAffinity::Downstream,
        300.0,
    )
}

fn plain_preedit_fixture_with_options(
    content: &str,
    cursor_char: usize,
    preedit: &str,
    preedit_cursor: Option<(usize, usize)>,
    affinity: super::super::caret_map::CaretAffinity,
    width: f32,
) -> (NodeArena, NodeKey) {
    let mut text_area = TextArea::new();
    text_area.content = content.to_string();
    text_area.font_size = 14.0;
    text_area.line_height = 1.25;
    text_area.multiline = true;
    text_area.cursor_char = cursor_char;
    text_area.cursor_affinity = affinity;
    text_area.ime_preedit = preedit.to_string();
    text_area.ime_preedit_cursor = preedit_cursor;

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
            max_width: width,
            max_height: 300.0,
            viewport_width: width,
            viewport_height: 300.0,
            percent_base_width: None,
            percent_base_height: None,
        },
        LayoutPlacement {
            parent_x: 10.0,
            parent_y: 20.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: width,
            available_height: 300.0,
            viewport_width: width,
            viewport_height: 300.0,
            percent_base_width: None,
            percent_base_height: None,
        },
    );
    (arena, root)
}

fn wrapped_plain_fixture(content: &str, width: f32) -> (NodeArena, NodeKey) {
    let mut text_area = TextArea::new();
    text_area.content = content.to_string();
    text_area.font_size = 14.0;
    text_area.line_height = 1.25;
    text_area.multiline = true;

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
            max_width: width,
            max_height: 300.0,
            viewport_width: width,
            viewport_height: 300.0,
            percent_base_width: None,
            percent_base_height: None,
        },
        LayoutPlacement {
            parent_x: 10.0,
            parent_y: 20.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: width,
            available_height: 300.0,
            viewport_width: width,
            viewport_height: 300.0,
            percent_base_width: None,
            percent_base_height: None,
        },
    );
    (arena, root)
}

fn consumed_soft_wrap_slots(arena: &NodeArena, root: NodeKey) -> (usize, usize) {
    arena
        .with_element_taken_ref(root, |el, arena| {
            let text_area = el
                .as_any()
                .downcast_ref::<TextArea>()
                .expect("TextArea root");
            let map = super::super::caret_map::CaretNavigationMap::build(text_area, arena);
            map.lines.windows(2).find_map(|pair| {
                let upper = pair[0].stops.last()?;
                let lower = pair[1].stops.first()?;
                let consumed = text_area
                    .content
                    .chars()
                    .skip(upper.char_index)
                    .take(lower.char_index.saturating_sub(upper.char_index));
                let consumed = consumed.collect::<String>();
                (!consumed.is_empty() && consumed.chars().all(char::is_whitespace))
                    .then_some((upper.char_index, lower.char_index))
            })
        })
        .expect("root exists")
        .expect("fixture should contain a soft wrap that consumes whitespace")
}

fn run_text_pass_fragments(arena: &NodeArena, root: NodeKey) -> Vec<(String, Rect)> {
    arena
        .with_element_taken_ref(root, |el, arena| {
            let text_area = el
                .as_any()
                .downcast_ref::<TextArea>()
                .expect("TextArea root");
            let Some(package) = text_area.unified_inline_ifc_render_package(arena) else {
                return Vec::new();
            };
            let origin = [
                text_area.layout_state.layout_position.x - text_area.scroll_x,
                text_area.layout_state.layout_position.y - text_area.scroll_y,
            ];
            let paint_input = package.ifc.text_pass_paint_input();
            let staging_input = package.text_pass_staging_input(origin, 1.0, 0, 1.0);
            let backing = package.ifc.backing_text().to_string();
            let text_sources = package
                .source_segments
                .iter()
                .filter(|segment| {
                    segment.kind
                        == super::super::inline_ifc::TextAreaUnifiedIfcSourceKind::TextRun
                })
                .map(|segment| segment.source)
                .collect::<Vec<_>>();

            let mut out = Vec::new();
            for line in &paint_input.lines {
                for source in &text_sources {
                    let mut left: Option<f32> = None;
                    let mut right: Option<f32> = None;
                    let mut start: Option<usize> = None;
                    let mut end: Option<usize> = None;
                    for (glyph, staged) in
                        paint_input.glyphs.iter().zip(staging_input.glyphs.iter())
                    {
                        if glyph.line_index != line.line_index || glyph.source != *source {
                            continue;
                        }
                        let x = staged.final_paint_pos[0];
                        left = Some(left.map_or(x, |current| current.min(x)));
                        right = Some(right.map_or(x + glyph.advance, |current| {
                            current.max(x + glyph.advance)
                        }));
                        start = Some(start.map_or(glyph.cluster_range.start, |current| {
                            current.min(glyph.cluster_range.start)
                        }));
                        end = Some(end.map_or(glyph.cluster_range.end, |current| {
                            current.max(glyph.cluster_range.end)
                        }));
                    }
                    let (Some(left), Some(right), Some(start), Some(end)) =
                        (left, right, start, end)
                    else {
                        continue;
                    };
                    if start >= end || right <= left {
                        continue;
                    }
                    out.push((
                        backing[start..end].to_string(),
                        Rect {
                            x: left,
                            y: origin[1] + line.y,
                            width: (right - left).max(0.0),
                            height: line.height.max(1.0),
                        },
                    ));
                }
            }
            out
        })
        .expect("root exists")
}
















fn projection_fixture_with_preedit_cursor(
    preedit_cursor: Option<(usize, usize)>,
) -> (NodeArena, NodeKey) {
    let mut text_area = TextArea::new();
    text_area.content = "abXYZcd".to_string();
    text_area.font_size = 14.0;
    text_area.line_height = 1.25;
    text_area.is_focused = true;
    text_area.cursor_char = 3;
    text_area.ime_preedit = "\u{4E2D}".to_string();
    text_area.ime_preedit_cursor = preedit_cursor;
    text_area.on_render_handler = Some(crate::ui::on_text_area_render(move |render| {
        render.range(2..5, |_text_area_node| {
            RsxNode::tagged(
                "Element",
                RsxTagDescriptor::for_tag::<crate::view::tags::Element>(),
            )
            .with_prop(
                "style",
                ElementStylePropSchema {
                    width: Some(Length::px(90.0)),
                    height: Some(Length::px(42.0)),
                    ..Default::default()
                },
            )
            .with_child(
                RsxNode::tagged(
                    "Text",
                    RsxTagDescriptor::for_tag::<crate::view::tags::Text>(),
                )
                .with_child(RsxNode::text("XYZ")),
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
            max_width: 300.0,
            max_height: 300.0,
            viewport_width: 300.0,
            viewport_height: 300.0,
            percent_base_width: None,
            percent_base_height: None,
        },
        LayoutPlacement {
            parent_x: 10.0,
            parent_y: 20.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 300.0,
            available_height: 300.0,
            viewport_width: 300.0,
            viewport_height: 300.0,
            percent_base_width: None,
            percent_base_height: None,
        },
    );
    (arena, root)
}

mod caret_blink_tests;
mod atomic_projection_source_tests;
mod atomic_projection_identity_tests;
mod viewport_scissor_tests;
mod unified_ifc_render_tests;
mod caret_geometry_tests;
mod preedit_underline_tests;
mod selection_render_tests;
