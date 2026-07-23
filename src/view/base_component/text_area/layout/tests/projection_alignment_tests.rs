use super::*;

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
