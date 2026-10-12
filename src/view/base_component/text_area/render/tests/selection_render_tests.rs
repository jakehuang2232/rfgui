use super::*;

#[test]
fn projection_selection_uses_text_rects_instead_of_projection_bounds() {
    let mut text_area = TextArea::new();
    text_area.content = "ab/activity/with/a/very/long/pathcd".to_string();
    text_area.font_size = 14.0;
    text_area.line_height = 1.25;
    text_area.select_range(19, 28);
    text_area.on_render_handler = Some(crate::ui::on_text_area_render(move |render| {
        render.range(2..34, |_text_area_node| {
            let style = ElementStylePropSchema {
                width: Some(Length::px(120.0)),
                height: Some(Length::px(80.0)),
                ..Default::default()
            };
            RsxNode::tagged(
                "Element",
                RsxTagDescriptor::for_tag::<crate::view::tags::Element>(),
            )
            .with_prop("style", style)
            .with_child(
                RsxNode::tagged(
                    "Text",
                    RsxTagDescriptor::for_tag::<crate::view::tags::Text>(),
                )
                .with_child(RsxNode::text("/activity/with/a/very/long/path")),
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

    let projection_snap = projection_snapshot(&arena, root);
    let witness = arena
        .with_element_taken_ref(root, |el, arena| {
            el.as_any()
                .downcast_ref::<TextArea>()
                .expect("TextArea root")
                .projection_selection_witness_for_child(projection_key(&arena, root), arena)
        })
        .expect("root exists")
        .expect("expected projection selection witness");
    assert_eq!(witness.local_start, 17);
    assert_eq!(witness.local_end, 26);
    let text_key = first_text_descendant(&arena, projection_key(&arena, root));
    let rects = arena
        .with_element_taken_ref(text_key, |el, _| {
            el.as_any()
                .downcast_ref::<Text>()
                .expect("projection Text")
                .local_selection_screen_rects(witness.local_start, witness.local_end)
        })
        .expect("text exists");

    assert!(
        !rects.is_empty(),
        "expected projection text selection rects"
    );
    assert!(
        rects
            .iter()
            .all(|rect| rect.height < projection_snap.height - 1.0),
        "selection should use visual text-line rects, not projection bounds: rects={rects:?}, projection={projection_snap:?}"
    );
    assert!(
        rects
            .iter()
            .any(|rect| rect.width < projection_snap.width - 1.0),
        "selection should be narrower than the projection union bounds: rects={rects:?}, projection={projection_snap:?}"
    );
}
