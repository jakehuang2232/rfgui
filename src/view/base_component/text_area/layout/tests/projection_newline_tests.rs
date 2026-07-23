use super::*;

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
