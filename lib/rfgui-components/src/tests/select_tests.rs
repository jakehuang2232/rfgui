use super::*;

#[test]
fn select_trigger_click_does_not_change_binding_value() {
    let selected = global_state(|| String::from("Option A"));
    let tree = rsx! {
        <Select::<String, String>
            data={vec![
                String::from("Option A"),
                String::from("Option B"),
                String::from("Option C"),
            ]}
            to_label={select_label as fn(&String, usize) -> String}
            value={selected.binding()}
        />
    };

    let mut arena = NodeArena::new();
    let roots = commit_rsx_tree_into(&mut arena, &tree);
    let mut viewport = rfgui::view::Viewport::new();
    click_once(&mut arena, roots[0], &mut viewport, 10.0, 10.0);
    assert_eq!(selected.get(), "Option A");
    assert_ne!(take_state_dirty(), UiDirtyState::NONE);
}

#[test]
fn select_open_state_persists_across_rerender() {
    let selected = global_state(|| String::from("Option A"));

    let build_tree = || {
        rsx! {
            <Select::<String, String>
                data={vec![
                    String::from("Option A"),
                    String::from("Option B"),
                    String::from("Option C"),
                ]}
                to_label={select_label as fn(&String, usize) -> String}
                value={selected.binding()}
            />
        }
    };

    let first_tree = build_tree();
    let mut arena = NodeArena::new();
    let roots = commit_rsx_tree_into(&mut arena, &first_tree);
    let mut viewport = rfgui::view::Viewport::new();
    click_once(&mut arena, roots[0], &mut viewport, 10.0, 10.0);
    assert_ne!(take_state_dirty(), UiDirtyState::NONE);

    let second_tree = build_tree();
    let RsxNode::Element(root) = second_tree else {
        panic!("select should render element root");
    };
    assert_eq!(
        root.children.len(),
        2,
        "select menu should remain open after rerender"
    );
}

#[test]
fn select_menu_option_row_keeps_content_height() {
    let selected = global_state(|| String::from("Option A"));

    let build_tree = || {
        rsx! {
            <Select::<String, String>
                data={vec![
                    String::from("Option A"),
                    String::from("Option B"),
                    String::from("Option C"),
                ]}
                to_label={select_label as fn(&String, usize) -> String}
                value={selected.binding()}
            />
        }
    };

    let first_tree = build_tree();
    let mut arena = NodeArena::new();
    let roots = commit_rsx_tree_into(&mut arena, &first_tree);
    let mut viewport = rfgui::view::Viewport::new();
    click_once(&mut arena, roots[0], &mut viewport, 10.0, 10.0);
    assert_ne!(take_state_dirty(), UiDirtyState::NONE);

    let second_tree = build_tree();
    let mut arena = NodeArena::new();
    let roots = commit_rsx_tree_into(&mut arena, &second_tree);
    let root_key = *roots.first().expect("has root");
    measure_and_place_root(
        &mut arena,
        root_key,
        LayoutConstraints {
            max_width: 320.0,
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
            available_width: 320.0,
            available_height: 240.0,
            viewport_width: 320.0,
            viewport_height: 240.0,
            percent_base_width: Some(320.0),
            percent_base_height: Some(240.0),
        },
    );

    let menu_key = arena.children_of(root_key)[1];
    let first_option_key = arena.children_of(menu_key)[0];
    let option_snapshot = arena
        .get(first_option_key)
        .expect("first option node")
        .element
        .box_model_snapshot();

    assert!(
        option_snapshot.height < 80.0,
        "expected option row to keep content height, got {}",
        option_snapshot.height
    );
}
