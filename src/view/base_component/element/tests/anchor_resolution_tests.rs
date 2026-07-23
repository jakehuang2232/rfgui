use super::*;

#[test]
fn anchor_parent_resolves_to_immediate_parent_box() {
    let (arena, child_k) = place_grandparent_parent_child(
        (100.0, 50.0, 200.0, 120.0),
        crate::style::Anchor::Parent,
        10.0,
        5.0,
    );
    let snap = child_snapshot(&arena, child_k);
    // child positioned at parent.x + left, parent.y + top
    assert!(
        (snap.x - (100.0 + 10.0)).abs() < 0.01,
        "layout_x = {}",
        snap.x
    );
    assert!(
        (snap.y - (50.0 + 5.0)).abs() < 0.01,
        "layout_y = {}",
        snap.y
    );
}

#[test]
fn anchor_root_resolves_to_root_box() {
    // root is grandparent at (0,0,800,600). left=12, top=8 → child at (12,8).
    let (arena, child_k) = place_grandparent_parent_child(
        (100.0, 50.0, 200.0, 120.0),
        crate::style::Anchor::Viewport,
        12.0,
        8.0,
    );
    let snap = child_snapshot(&arena, child_k);
    assert!((snap.x - 12.0).abs() < 0.01, "layout_x = {}", snap.x);
    assert!((snap.y - 8.0).abs() < 0.01, "layout_y = {}", snap.y);
}

#[test]
fn anchor_ancestor_n_walks_up_n_levels() {
    // Ancestor(1) == Parent.
    let (arena, child_k) = place_grandparent_parent_child(
        (100.0, 50.0, 200.0, 120.0),
        crate::style::Anchor::Ancestor(1),
        10.0,
        5.0,
    );
    let snap = child_snapshot(&arena, child_k);
    assert!((snap.x - 110.0).abs() < 0.01);
    assert!((snap.y - 55.0).abs() < 0.01);

    // Ancestor(2) == grandparent (root) at (0,0).
    let (arena2, child_k2) = place_grandparent_parent_child(
        (100.0, 50.0, 200.0, 120.0),
        crate::style::Anchor::Ancestor(2),
        12.0,
        8.0,
    );
    let snap2 = child_snapshot(&arena2, child_k2);
    assert!((snap2.x - 12.0).abs() < 0.01, "layout_x = {}", snap2.x);
    assert!((snap2.y - 8.0).abs() < 0.01, "layout_y = {}", snap2.y);
}

#[test]
fn anchor_str_still_resolves_via_named_map() {
    // Regression: From<&str> for Anchor still flows through the named-anchor map.
    let parent = Element::new(0.0, 0.0, 500.0, 200.0);
    let mut anchor = Element::new(0.0, 0.0, 40.0, 40.0);
    let mut anchor_style = Style::new();
    anchor_style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .left(Length::px(300.0))
                .top(Length::px(20.0)),
        ),
    );
    anchor.apply_style(anchor_style);
    anchor.set_anchor_name(Some(AnchorName::new("menu_button")));
    let mut child = Element::new(0.0, 0.0, 30.0, 20.0);
    let mut child_style = Style::new();
    child_style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .anchor("menu_button")
                .left(Length::px(5.0))
                .top(Length::px(0.0)),
        ),
    );
    child.apply_style(child_style);

    let mut arena = new_test_arena();
    let parent_key = commit_element(&mut arena, Box::new(parent));
    let _anchor_k = commit_child(&mut arena, parent_key, Box::new(anchor));
    let child_k = commit_child(&mut arena, parent_key, Box::new(child));

    measure_and_place(
        &mut arena,
        parent_key,
        LayoutConstraints {
            max_width: 600.0,
            max_height: 300.0,
            viewport_width: 600.0,
            percent_base_width: Some(600.0),
            percent_base_height: Some(300.0),
            viewport_height: 300.0,
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 600.0,
            available_height: 300.0,
            viewport_width: 600.0,
            percent_base_width: Some(600.0),
            percent_base_height: Some(300.0),
            viewport_height: 300.0,
        },
    );

    let snap = child_snapshot(&arena, child_k);
    // Anchored to menu_button at (300,20). left=5, top=0 → child at (305, 20).
    assert!((snap.x - 305.0).abs() < 0.01, "layout_x = {}", snap.x);
    assert!((snap.y - 20.0).abs() < 0.01, "layout_y = {}", snap.y);
}
