use super::*;

#[test]
fn setting_border_radius_does_not_mark_layout_dirty() {
    let mut el = Element::new(0.0, 0.0, 20.0, 20.0);
    el.layout_dirty = false;

    el.set_border_radius(12.0);

    assert_eq!(el.border_radius, 12.0);
    assert!(!el.layout_dirty);
}

#[test]
fn setting_opacity_marks_paint_and_composite_without_layout_or_placement() {
    let mut el = Element::new(0.0, 0.0, 20.0, 20.0);
    el.clear_local_dirty_flags(DirtyFlags::ALL);

    el.set_opacity(0.5);

    let dirty = el.local_dirty_flags();
    assert!(dirty.contains(DirtyFlags::PAINT));
    assert!(dirty.contains(DirtyFlags::COMPOSITE));
    assert!(!dirty.intersects(DirtyFlags::LAYOUT));
    assert!(
        !dirty.intersects(
            DirtyFlags::PLACE
                .union(DirtyFlags::BOX_MODEL)
                .union(DirtyFlags::HIT_TEST),
        )
    );
}

#[test]
fn border_radius_style_sample_preserves_resolved_corner_ratios() {
    let mut arena = new_test_arena();
    let mut el = Element::new(0.0, 0.0, 200.0, 150.0);
    let mut style = Style::new();
    style.set_border_radius(
        BorderRadius::uniform(Length::px(10.0))
            .top_right(Length::px(32.0))
            .bottom_left(Length::percent(90.0)),
    );
    el.apply_style(style);
    let node_id = el.stable_id();
    let key = commit_element(&mut arena, Box::new(el));

    assert!(set_style_field_by_id(
        &mut arena,
        key,
        node_id,
        crate::transition::StyleField::BorderRadius,
        crate::transition::StyleValue::Scalar(50.0),
    ));

    let el = crate::view::test_support::get_element::<Element>(&arena, key);
    assert!((el.border_radii.top_left - 3.7037036).abs() < 0.001);
    assert!((el.border_radii.top_right - 11.851851).abs() < 0.001);
    assert!((el.border_radii.bottom_right - 3.7037036).abs() < 0.001);
    assert!((el.border_radii.bottom_left - 50.0).abs() < 0.001);
    assert!((el.border_radius - 50.0).abs() < 0.001);
}

#[test]
fn clear_subtree_dirty_flags_with_arena_dirty_clears_element_and_arena_dirty() {
    let mut arena = new_test_arena();
    let root_key = commit_element(&mut arena, Box::new(clean_bridge_element(100.0, 100.0)));
    let child_key = commit_child(
        &mut arena,
        root_key,
        Box::new(clean_bridge_element(80.0, 40.0)),
    );
    let grandchild_key = commit_child(
        &mut arena,
        child_key,
        Box::new(clean_bridge_element(40.0, 20.0)),
    );
    arena.with_element_taken(root_key, |root, _arena| {
        root.clear_local_dirty_flags(DirtyFlags::PAINT);
    });
    arena.clear_arena_dirty_subtree(root_key, DirtyFlags::ALL);
    arena.refresh_subtree_dirty_cache(root_key);
    mark_arena_paint_dirty_for_subtree(&arena, child_key);

    assert!(
        arena
            .cached_subtree_dirty(root_key)
            .intersects(DirtyFlags::PAINT)
    );
    assert!(
        arena
            .cached_subtree_dirty(child_key)
            .intersects(DirtyFlags::PAINT)
    );

    assert!(
        crate::view::viewport::scene_helpers::clear_subtree_dirty_flags_with_arena_dirty(
            &mut arena,
            child_key,
            DirtyFlags::PAINT,
        )
    );

    let child = crate::view::test_support::get_element::<Element>(&arena, child_key);
    let grandchild = crate::view::test_support::get_element::<Element>(&arena, grandchild_key);
    assert!(!child.local_dirty_flags().contains(DirtyFlags::PAINT));
    assert!(!grandchild.local_dirty_flags().contains(DirtyFlags::PAINT));
    assert_eq!(arena.arena_local_dirty(child_key), DirtyFlags::NONE);
    assert_eq!(arena.arena_local_dirty(grandchild_key), DirtyFlags::NONE);
    assert!(
        !arena
            .cached_subtree_dirty(root_key)
            .intersects(DirtyFlags::PAINT)
    );
    assert!(
        !arena
            .cached_subtree_dirty(child_key)
            .intersects(DirtyFlags::PAINT)
    );
}

#[test]
fn clear_subtree_dirty_flags_with_arena_dirty_preserves_sibling_arena_dirty() {
    let mut arena = new_test_arena();
    let root_key = commit_element(&mut arena, Box::new(clean_bridge_element(100.0, 100.0)));
    let child_key = commit_child(
        &mut arena,
        root_key,
        Box::new(clean_bridge_element(80.0, 40.0)),
    );
    let grandchild_key = commit_child(
        &mut arena,
        child_key,
        Box::new(clean_bridge_element(40.0, 20.0)),
    );
    let sibling_key = commit_child(
        &mut arena,
        root_key,
        Box::new(clean_bridge_element(60.0, 30.0)),
    );
    arena.with_element_taken(root_key, |root, _arena| {
        root.clear_local_dirty_flags(DirtyFlags::PAINT);
    });
    arena.clear_arena_dirty_subtree(root_key, DirtyFlags::ALL);
    arena.refresh_subtree_dirty_cache(root_key);
    mark_arena_paint_dirty_for_subtree(&arena, child_key);
    arena.mark_dirty(sibling_key, DirtyFlags::PAINT);

    assert!(
        crate::view::viewport::scene_helpers::clear_subtree_dirty_flags_with_arena_dirty(
            &mut arena,
            child_key,
            DirtyFlags::PAINT,
        )
    );

    let child = crate::view::test_support::get_element::<Element>(&arena, child_key);
    let grandchild = crate::view::test_support::get_element::<Element>(&arena, grandchild_key);
    assert!(!child.local_dirty_flags().contains(DirtyFlags::PAINT));
    assert!(!grandchild.local_dirty_flags().contains(DirtyFlags::PAINT));
    assert_eq!(arena.arena_local_dirty(child_key), DirtyFlags::NONE);
    assert_eq!(arena.arena_local_dirty(grandchild_key), DirtyFlags::NONE);
    assert!(
        arena
            .arena_local_dirty(sibling_key)
            .contains(DirtyFlags::PAINT)
    );
    assert!(
        !arena
            .cached_subtree_dirty(child_key)
            .intersects(DirtyFlags::PAINT)
    );
    assert!(
        arena
            .cached_subtree_dirty(sibling_key)
            .intersects(DirtyFlags::PAINT)
    );
    assert!(
        arena
            .cached_subtree_dirty(root_key)
            .intersects(DirtyFlags::PAINT)
    );
}

#[test]
fn clear_subtree_dirty_flags_with_arena_dirty_returns_false_for_missing_root() {
    let mut arena = new_test_arena();
    let root_key = commit_element(&mut arena, Box::new(clean_bridge_element(100.0, 100.0)));

    assert!(
        !crate::view::viewport::scene_helpers::clear_subtree_dirty_flags_with_arena_dirty(
            &mut arena,
            crate::view::node_arena::NodeKey::default(),
            DirtyFlags::PAINT,
        )
    );
    assert!(
        arena
            .cached_subtree_dirty(root_key)
            .intersects(DirtyFlags::PAINT)
    );
}
