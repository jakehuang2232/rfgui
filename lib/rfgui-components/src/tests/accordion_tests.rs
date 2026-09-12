use super::*;

#[test]
fn accordion_default_expanded_renders_children() {
    let tree = rsx! {
        <Accordion
            title="Section A"
            default_expanded={Some(true)}
        >
            <Text>"Content A"</Text>
        </Accordion>
    };

    let mut texts = Vec::new();
    collect_text_nodes(&tree, &mut texts);
    assert!(texts.iter().any(|text| text == "Section A"));
    assert!(texts.iter().any(|text| text == "Content A"));
}

#[test]
fn accordion_collapsed_keeps_children_in_tree() {
    let tree = rsx! {
        <Accordion title="Section B">
            <Text>"Content B"</Text>
        </Accordion>
    };

    let mut texts = Vec::new();
    collect_text_nodes(&tree, &mut texts);
    assert!(texts.iter().any(|text| text == "Section B"));
    assert!(texts.iter().any(|text| text == "Content B"));
}

#[test]
fn accordion_click_updates_expanded_binding() {
    let expanded = global_state(|| false);

    let tree = rsx! {
        <Accordion
            title="Section C"
            expanded_binding={Some(expanded.binding())}
        >
            <Text>"Content C"</Text>
        </Accordion>
    };

    let mut arena = NodeArena::new();
    let roots = commit_rsx_tree_into(&mut arena, &tree);
    let mut viewport = rfgui::view::Viewport::new();
    click_once(&mut arena, roots[0], &mut viewport, 10.0, 10.0);

    assert!(expanded.get());
}

#[test]
fn accordion_header_title_grows_and_icon_stays_intrinsic() {
    let tree = rsx! {
        <Accordion title="Button">
            <Text>"Content"</Text>
        </Accordion>
    };

    let mut arena = NodeArena::new();
    let roots = commit_rsx_tree_into(&mut arena, &tree);
    let root_key = *roots.first().expect("root");
    measure_and_place_root(
        &mut arena,
        root_key,
        LayoutConstraints {
            max_width: 420.0,
            max_height: 200.0,
            viewport_width: 420.0,
            viewport_height: 200.0,
            percent_base_width: Some(420.0),
            percent_base_height: Some(200.0),
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 420.0,
            available_height: 200.0,
            viewport_width: 420.0,
            viewport_height: 200.0,
            percent_base_width: Some(420.0),
            percent_base_height: Some(200.0),
        },
    );

    let mut boxes = Vec::new();
    collect_layout_boxes(&arena, root_key, 0, &mut boxes);
    let header = &boxes[1];
    let title = &boxes[2];
    let icon = &boxes[4];

    assert!(header.4 > 400.0, "header should use full width: {boxes:#?}");
    assert!(
        title.4 > 300.0,
        "title should grow to fill remaining width: {boxes:#?}"
    );
    assert!(icon.4 < 40.0, "icon text should stay intrinsic: {boxes:#?}");
    assert!(
        icon.2 > title.2 + title.4 - 1.0,
        "icon should be pushed after title: {boxes:#?}"
    );
}

#[test]
fn window_accordion_button_label_hit_tests_inside_button_branch() {
    let tree = rsx! {
        <Window
            title="Component Test"
            width={Some(460.0)}
            height={Some(380.0)}
            position={Some((96.0, 96.0))}
        >
            <Accordion
                title="Button"
                default_expanded={Some(true)}
            >
                <Button variant={Some(ButtonVariant::Contained)}>
                    Contained
                </Button>
            </Accordion>
        </Window>
    };

    let mut arena = NodeArena::new();
    let roots = commit_rsx_tree_into(&mut arena, &tree);
    let root_key = *roots.first().expect("root");
    measure_and_place_root(
        &mut arena,
        root_key,
        LayoutConstraints {
            max_width: 1280.0,
            max_height: 800.0,
            viewport_width: 1280.0,
            viewport_height: 800.0,
            percent_base_width: Some(1280.0),
            percent_base_height: Some(800.0),
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 1280.0,
            available_height: 800.0,
            viewport_width: 1280.0,
            viewport_height: 800.0,
            percent_base_width: Some(1280.0),
            percent_base_height: Some(800.0),
        },
    );

    let label_key = find_text_node(&arena, root_key, "Contained").expect("button label");
    let label_snapshot = arena
        .get(label_key)
        .expect("button label")
        .element
        .box_model_snapshot();
    let x = label_snapshot.x + label_snapshot.width * 0.5;
    let y = label_snapshot.y + label_snapshot.height * 0.5;
    let target = rfgui::view::base_component::hit_test(&arena, root_key, x, y)
        .expect("button label should hit-test");
    let target_id = arena.get(target).expect("hit target").element.stable_id();
    let cursor = rfgui::view::base_component::get_cursor_by_id(&arena, root_key, target_id)
        .expect("cursor for hit target");

    assert!(
        is_ancestor_or_self(&arena, target, label_key)
            || is_ancestor_or_self(&arena, label_key, target),
        "hit at button label ({x}, {y}) should stay in the button label branch; target={target:?}, label={label_key:?}",
    );
    assert_eq!(cursor, rfgui::style::Cursor::Pointer);
}
