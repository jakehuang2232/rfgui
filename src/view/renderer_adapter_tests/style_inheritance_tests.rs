use super::*;

#[test]
fn cursor_style_inherits_to_child_when_child_has_no_cursor() {
    let parent_style = ElementStylePropSchema {
        width: Some(Length::px(100.0)),
        height: Some(Length::px(100.0)),
        background: Some(crate::style::Background::Color(Box::new(
            IntoColor::<Color>::into_color(Color::hex("#101010")),
        ))),
        cursor: Some(Cursor::Pointer),
        ..empty_element_style()
    };

    let child_style = ElementStylePropSchema {
        width: Some(Length::px(40.0)),
        height: Some(Length::px(40.0)),
        background: Some(crate::style::Background::Color(Box::new(
            IntoColor::<Color>::into_color(Color::hex("#ff0000")),
        ))),
        ..empty_element_style()
    };

    let tree = host_element_node()
        .with_prop("style", parent_style)
        .with_child(host_element_node().with_prop("style", child_style));

    let mut arena = crate::view::test_support::new_test_arena();
    let roots = commit_rsx_tree(&mut arena, &tree);
    let root = *roots.first().expect("single root");
    measure_and_place(&mut arena, root, std_constraints(), std_placement());

    let target_key = hit_test(&arena, root, 10.0, 10.0).expect("hit child");
    let target_stable_id = arena.get(target_key).unwrap().element.stable_id();
    let cursor = get_cursor_by_id(&arena, root, target_stable_id).expect("cursor exists");
    assert_eq!(cursor, Cursor::Pointer);
}

#[test]
fn cursor_style_inherits_to_text_child() {
    let parent_style = ElementStylePropSchema {
        width: Some(Length::px(200.0)),
        height: Some(Length::px(80.0)),
        background: Some(crate::style::Background::Color(Box::new(
            IntoColor::<Color>::into_color(Color::hex("#101010")),
        ))),
        cursor: Some(Cursor::Pointer),
        ..empty_element_style()
    };

    let tree = host_element_node()
        .with_prop("style", parent_style)
        .with_child(
            host_text_node()
                .with_prop("font_size", 16.0)
                .with_child(RsxNode::text("Button label")),
        );

    let mut arena = crate::view::test_support::new_test_arena();
    let roots = commit_rsx_tree(&mut arena, &tree);
    let root = *roots.first().expect("single root");
    measure_and_place(&mut arena, root, std_constraints(), std_placement());

    let target_key = hit_test(&arena, root, 10.0, 10.0).expect("hit text child");
    let target_stable_id = arena.get(target_key).unwrap().element.stable_id();
    let cursor = get_cursor_by_id(&arena, root, target_stable_id).expect("cursor exists");
    assert_eq!(cursor, Cursor::Pointer);
}

#[test]
fn text_style_font_size_em_inherits_from_parent_font_size() {
    let parent_style = ElementStylePropSchema {
        font_size: Some(FontSize::px(20.0)),
        ..empty_element_style()
    };
    let child_style = TextStylePropSchema {
        font_size: Some(FontSize::em(1.5)),
        ..empty_text_style()
    };

    let tree = host_element_node()
        .with_prop("style", parent_style)
        .with_child(
            host_text_node()
                .with_prop("style", child_style)
                .with_child(RsxNode::text("MMMMMMMM")),
        );

    let mut arena = crate::view::test_support::new_test_arena();
    let roots = commit_rsx_tree(&mut arena, &tree);
    let root = *roots.first().expect("single root");
    measure_and_place(&mut arena, root, std_constraints(), std_placement());

    let mut text_boxes = Vec::new();
    collect_text_like_boxes(&arena, root, &mut text_boxes);
    let (width, height) = text_boxes.first().copied().expect("text box should exist");
    assert!(width > 150.0);
    assert!(height >= 30.0);
}

#[test]
fn rem_font_size_uses_viewport_style_root_font_size() {
    let text_tree = host_text_node()
        .with_prop(
            "style",
            TextStylePropSchema {
                font_size: Some(FontSize::rem(2.0)),
                ..empty_text_style()
            },
        )
        .with_child(RsxNode::text("MMMMMMMM"));

    let mut small_root_style = Style::new();
    small_root_style.insert(
        PropertyId::FontSize,
        ParsedValue::FontSize(FontSize::px(10.0)),
    );
    let mut large_root_style = Style::new();
    large_root_style.insert(
        PropertyId::FontSize,
        ParsedValue::FontSize(FontSize::px(20.0)),
    );

    let mut small_arena = crate::view::test_support::new_test_arena();
    let small = crate::view::test_support::commit_rsx_tree_with_context(
        &mut small_arena,
        &text_tree,
        &small_root_style,
        800.0,
        600.0,
    );
    let mut large_arena = crate::view::test_support::new_test_arena();
    let large = crate::view::test_support::commit_rsx_tree_with_context(
        &mut large_arena,
        &text_tree,
        &large_root_style,
        800.0,
        600.0,
    );

    for root in &small {
        measure_and_place(&mut small_arena, *root, std_constraints(), std_placement());
    }
    for root in &large {
        measure_and_place(&mut large_arena, *root, std_constraints(), std_placement());
    }

    let small_snapshot = small_arena
        .get(*small.first().expect("small root"))
        .unwrap()
        .element
        .box_model_snapshot();
    let large_snapshot = large_arena
        .get(*large.first().expect("large root"))
        .unwrap()
        .element
        .box_model_snapshot();
    assert!(large_snapshot.width > small_snapshot.width * 1.5);
    assert!(large_snapshot.height > small_snapshot.height * 1.5);
}
