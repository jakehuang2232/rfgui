use super::*;

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
fn typing_with_projections_keeps_caret_at_insertion_point() {
    for cursor in [10_usize, 68, 69, 70, 81, 82, 90, 91, 102, 103] {
        typing_with_projections_keeps_caret_at_insertion_point_at(cursor);
    }
}

#[test]
fn arrow_right_traverses_projection_in_reading_order() {
    for width in [342.0_f32, 300.0, 240.0, 180.0] {
        arrow_right_traverses_projection_in_reading_order_at(width);
    }
}
