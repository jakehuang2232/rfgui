use super::*;

#[test]
fn textarea_inherits_font_size_from_parent_style() {
    let parent_style = ElementStylePropSchema {
        font_size: Some(FontSize::px(24.0)),
        ..empty_element_style()
    };

    let tree = host_element_node()
        .with_prop("style", parent_style)
        .with_child(
            host_text_area_node()
                .with_prop("content", "hello")
                .with_prop("multiline", false),
        );

    let mut arena = crate::view::test_support::new_test_arena();
    let roots = commit_rsx_tree(&mut arena, &tree);
    let root = *roots.first().expect("single root");
    measure_and_place(&mut arena, root, std_constraints(), std_placement());

    let mut text_boxes = Vec::new();
    collect_text_like_boxes(&arena, root, &mut text_boxes);
    let (_width, height) = text_boxes
        .iter()
        .copied()
        .find(|(_, h)| *h > 0.0)
        .expect("textarea box should exist");
    assert!(height >= 24.0);
}

#[test]
fn textarea_uses_style_color_and_inherits_parent_color() {
    let parent_color = IntoColor::<Color>::into_color(Color::hex("#336699"));
    let local_color = IntoColor::<Color>::into_color(Color::hex("#aa5500"));

    let parent_style = ElementStylePropSchema {
        color: Some(Box::new(parent_color)),
        ..empty_element_style()
    };

    let textarea_style = ElementStylePropSchema {
        color: Some(Box::new(local_color)),
        ..empty_element_style()
    };

    let inherited_tree = host_element_node()
        .with_prop("style", parent_style.clone())
        .with_child(
            host_text_area_node()
                .with_prop("content", "hello")
                .with_prop("multiline", false),
        );
    let explicit_tree = host_element_node()
        .with_prop("style", parent_style)
        .with_child(
            host_text_area_node()
                .with_prop("style", textarea_style)
                .with_prop("content", "hello")
                .with_prop("multiline", false),
        );

    let mut inherited_arena = crate::view::test_support::new_test_arena();
    let inherited = commit_rsx_tree(&mut inherited_arena, &inherited_tree);
    let mut explicit_arena = crate::view::test_support::new_test_arena();
    let explicit = commit_rsx_tree(&mut explicit_arena, &explicit_tree);

    let inherited_ta_key = {
        let root = *inherited.first().expect("inherited root");
        *inherited_arena
            .children_of(root)
            .first()
            .expect("inherited ta child")
    };
    let explicit_ta_key = {
        let root = *explicit.first().expect("explicit root");
        *explicit_arena
            .children_of(root)
            .first()
            .expect("explicit ta child")
    };

    let inherited_rgba = inherited_arena
        .get(inherited_ta_key)
        .unwrap()
        .element
        .as_any()
        .downcast_ref::<TextArea>()
        .expect("inherited textarea")
        .color
        .to_rgba_f32();
    let explicit_rgba = explicit_arena
        .get(explicit_ta_key)
        .unwrap()
        .element
        .as_any()
        .downcast_ref::<TextArea>()
        .expect("explicit textarea")
        .color
        .to_rgba_f32();

    assert_eq!(inherited_rgba, parent_color.to_rgba_f32());
    assert_eq!(explicit_rgba, local_color.to_rgba_f32());
}

#[test]
fn textarea_style_bridge_resolves_em_font_size_from_inherited_parent() {
    let parent_style = ElementStylePropSchema {
        font_size: Some(FontSize::px(24.0)),
        ..empty_element_style()
    };
    let textarea_style = ElementStylePropSchema {
        font_size: Some(FontSize::em(1.25)),
        ..empty_element_style()
    };

    let tree = host_element_node()
        .with_prop("style", parent_style)
        .with_child(
            host_text_area_node()
                .with_prop("style", textarea_style)
                .with_prop("content", "hello")
                .with_prop("multiline", false),
        );

    let mut arena = crate::view::test_support::new_test_arena();
    let roots = commit_rsx_tree(&mut arena, &tree);
    let root = *roots.first().expect("single root");
    let text_area_key = *arena.children_of(root).first().expect("textarea child");
    let text_area_node = arena.get(text_area_key).unwrap();
    let text_area = text_area_node
        .element
        .as_any()
        .downcast_ref::<TextArea>()
        .expect("textarea");

    assert!((text_area.font_size - 30.0).abs() < f32::EPSILON);
}

#[test]
fn textarea_style_bridge_applies_existing_text_fields() {
    let local_color = IntoColor::<Color>::into_color(Color::hex("#aa5500"));
    let textarea_style = ElementStylePropSchema {
        color: Some(Box::new(local_color)),
        font: Some(FontFamily::new(["Inter", "system-ui"])),
        font_weight: Some(FontWeight::new(650)),
        line_height: Some(1.7),
        cursor: Some(Cursor::Pointer),
        ..empty_element_style()
    };

    let tree = host_text_area_node()
        .with_prop("style", textarea_style)
        .with_prop("content", "hello")
        .with_prop("multiline", false);

    let mut arena = crate::view::test_support::new_test_arena();
    let roots = commit_rsx_tree(&mut arena, &tree);
    let text_area_node = arena.get(*roots.first().expect("textarea root")).unwrap();
    let text_area = text_area_node
        .element
        .as_any()
        .downcast_ref::<TextArea>()
        .expect("textarea");

    assert_eq!(text_area.color.to_rgba_f32(), local_color.to_rgba_f32());
    assert_eq!(text_area.font_families, vec!["Inter", "system-ui"]);
    assert_eq!(text_area.font_weight, 650);
    assert!((text_area.line_height - 1.7).abs() < f32::EPSILON);
    assert_eq!(text_area.cursor, Cursor::Pointer);
}

#[test]
fn textarea_style_width_height_remain_box_model_noops() {
    let textarea_style = ElementStylePropSchema {
        width: Some(Length::px(120.0)),
        height: Some(Length::px(48.0)),
        font_size: Some(FontSize::px(18.0)),
        ..empty_element_style()
    };

    let tree = host_text_area_node()
        .with_prop("style", textarea_style)
        .with_prop("content", "hello")
        .with_prop("multiline", false);

    let mut arena = crate::view::test_support::new_test_arena();
    let roots = commit_rsx_tree(&mut arena, &tree);
    let root = *roots.first().expect("textarea root");
    measure_and_place(&mut arena, root, std_constraints(), std_placement());

    let snapshot = arena.get(root).unwrap().element.box_model_snapshot();
    assert!(
        snapshot.width > 120.0,
        "TextArea style.width should remain a no-op; got {}",
        snapshot.width
    );
    assert!(
        (snapshot.height - 48.0).abs() > 0.5,
        "TextArea style.height should remain a no-op; got {}",
        snapshot.height
    );
}

#[test]
fn textarea_accepts_on_blur_prop() {
    let tree = rsx! {
        <crate::view::TextArea
            on_blur={move |event: &mut crate::ui::BlurEvent| event.meta.stop_propagation()}
            content="hello"
            multiline={false}
        />
    };

    let mut arena = crate::view::test_support::new_test_arena();
    let roots = commit_rsx_tree(&mut arena, &tree);
    assert_eq!(roots.len(), 1);
    assert!(
        arena
            .get(roots[0])
            .unwrap()
            .element
            .as_any()
            .downcast_ref::<TextArea>()
            .is_some()
    );
}

// v1 TextArea accepted width/height directly; per design A1 v2 does
// not — the box model lives on a wrapping `<Element>`. The two old
// size-on-textarea tests were dropped in P7.
