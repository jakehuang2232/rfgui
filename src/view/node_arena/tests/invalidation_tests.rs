use super::*;

#[test]
fn mutate_element_with_invalidation_bubbles_invalidated_flags() {
    let mut arena = NodeArena::new();
    let root = insert_test_node(&mut arena, 1, DirtyFlags::NONE);
    let child = insert_test_node(&mut arena, 2, DirtyFlags::NONE);
    link_child(&mut arena, root, child);

    arena.refresh_subtree_dirty_cache(root);
    assert!(
        !arena
            .cached_subtree_dirty(root)
            .intersects(DirtyFlags::PAINT)
    );

    arena
        .mutate_element_with_invalidation(child, |_element, cx| {
            cx.invalidate(DirtyFlags::PAINT);
        })
        .expect("child exists");

    assert!(arena.arena_local_dirty(child).contains(DirtyFlags::PAINT));
    assert!(
        arena
            .cached_subtree_dirty(root)
            .intersects(DirtyFlags::PAINT)
    );
}

#[test]
fn mutate_element_with_invalidation_context_can_clear_arena_dirty() {
    let mut arena = NodeArena::new();
    let root = insert_test_node(&mut arena, 1, DirtyFlags::NONE);
    let child = insert_test_node(&mut arena, 2, DirtyFlags::NONE);
    link_child(&mut arena, root, child);

    arena.refresh_subtree_dirty_cache(root);
    assert_cached_paint_clean(&arena, child);
    assert_cached_paint_clean(&arena, root);

    arena
        .mutate_element_with_invalidation(child, |_element, cx| {
            cx.invalidate(DirtyFlags::PAINT);
            cx.clear_arena_dirty(DirtyFlags::PAINT);
        })
        .expect("child exists");

    assert_eq!(arena.arena_local_dirty(child), DirtyFlags::NONE);
    assert_cached_paint_clean(&arena, child);
    assert_cached_paint_clean(&arena, root);
}

#[test]
fn element_opacity_with_invalidation_updates_local_and_arena_dirty() {
    let mut arena = NodeArena::new();
    let root = arena.insert(Node::new(Box::new(clean_element())));
    let child = arena.insert(Node::new(Box::new(clean_element())));
    link_child(&mut arena, root, child);

    arena.refresh_subtree_dirty_cache(root);
    assert!(
        !arena
            .get(child)
            .expect("child exists")
            .element
            .local_dirty_flags()
            .intersects(DirtyFlags::PAINT)
    );
    assert_eq!(arena.arena_local_dirty(child), DirtyFlags::NONE);
    assert!(
        !arena
            .cached_subtree_dirty(root)
            .intersects(DirtyFlags::PAINT)
    );

    arena
        .mutate_element_with_invalidation(child, |element, cx| {
            element
                .as_any_mut()
                .downcast_mut::<Element>()
                .expect("element")
                .set_opacity_with_invalidation(0.5, cx);
        })
        .expect("child exists");

    assert!(
        arena
            .get(child)
            .expect("child exists")
            .element
            .local_dirty_flags()
            .contains(DirtyFlags::PAINT)
    );
    assert!(
        arena
            .get(child)
            .expect("child exists")
            .element
            .local_dirty_flags()
            .contains(DirtyFlags::COMPOSITE)
    );
    assert!(arena.arena_local_dirty(child).contains(DirtyFlags::PAINT));
    assert!(
        arena
            .arena_local_dirty(child)
            .contains(DirtyFlags::COMPOSITE)
    );
    assert!(
        arena
            .cached_subtree_dirty(child)
            .intersects(DirtyFlags::PAINT)
    );
    assert!(
        arena
            .cached_subtree_dirty(root)
            .intersects(DirtyFlags::PAINT)
    );
    assert!(
        arena
            .cached_subtree_dirty(child)
            .contains(DirtyFlags::COMPOSITE)
    );
    assert!(
        arena
            .cached_subtree_dirty(root)
            .contains(DirtyFlags::COMPOSITE)
    );
}

#[test]
fn composite_shadow_dirty_bubbles_and_clears_independently() {
    let mut arena = NodeArena::new();
    let root = arena.insert(Node::new(Box::new(clean_element())));
    let child = arena.insert(Node::new(Box::new(clean_element())));
    link_child(&mut arena, root, child);
    arena.refresh_subtree_dirty_cache(root);

    arena.mark_dirty(child, DirtyFlags::COMPOSITE);
    assert!(
        arena
            .arena_local_dirty(child)
            .contains(DirtyFlags::COMPOSITE)
    );
    assert!(
        arena
            .cached_subtree_dirty(root)
            .contains(DirtyFlags::COMPOSITE)
    );
    assert!(
        !arena
            .cached_subtree_dirty(root)
            .intersects(DirtyFlags::PAINT)
    );

    arena.clear_arena_dirty(child, DirtyFlags::COMPOSITE);
    assert!(
        !arena
            .arena_local_dirty(child)
            .intersects(DirtyFlags::COMPOSITE)
    );
    assert!(
        !arena
            .cached_subtree_dirty(root)
            .intersects(DirtyFlags::COMPOSITE)
    );
}

#[test]
fn element_background_color_with_invalidation_updates_value_and_paint_dirty() {
    let mut arena = NodeArena::new();
    let root = arena.insert(Node::new(Box::new(clean_element())));
    let child = arena.insert(Node::new(Box::new(clean_element())));
    let color = Color::rgba(12, 34, 56, 200);
    link_child(&mut arena, root, child);

    arena.refresh_subtree_dirty_cache(root);
    assert!(
        !arena
            .cached_subtree_dirty(root)
            .intersects(DirtyFlags::PAINT)
    );

    arena
        .mutate_element_with_invalidation(child, |element, cx| {
            element
                .as_any_mut()
                .downcast_mut::<Element>()
                .expect("element")
                .set_background_color_value_with_invalidation(color, cx);
        })
        .expect("child exists");

    let render_state = arena
        .get(child)
        .expect("child exists")
        .element
        .as_any()
        .downcast_ref::<Element>()
        .expect("element")
        .debug_render_state();
    assert_eq!(render_state.background_rgba, color.to_rgba_u8());
    assert_element_and_arena_paint_dirty(&arena, root, child);
}

#[test]
fn element_foreground_color_with_invalidation_updates_value_and_paint_dirty() {
    let mut arena = NodeArena::new();
    let root = arena.insert(Node::new(Box::new(clean_element())));
    let child = arena.insert(Node::new(Box::new(clean_element())));
    let color = Color::rgb(90, 80, 70);
    link_child(&mut arena, root, child);

    arena.refresh_subtree_dirty_cache(root);
    assert!(
        !arena
            .cached_subtree_dirty(root)
            .intersects(DirtyFlags::PAINT)
    );

    arena
        .mutate_element_with_invalidation(child, |element, cx| {
            element
                .as_any_mut()
                .downcast_mut::<Element>()
                .expect("element")
                .set_foreground_color_with_invalidation(color, cx);
        })
        .expect("child exists");

    let render_state = arena
        .get(child)
        .expect("child exists")
        .element
        .as_any()
        .downcast_ref::<Element>()
        .expect("element")
        .debug_render_state();
    assert_eq!(render_state.foreground_rgba, color.to_rgba_u8());
    assert_element_and_arena_paint_dirty(&arena, root, child);
}

macro_rules! border_color_with_invalidation_test {
    ($name:ident, $setter:ident, $field:ident, $color:expr) => {
        #[test]
        fn $name() {
            let mut arena = NodeArena::new();
            let root = arena.insert(Node::new(Box::new(clean_element())));
            let child = arena.insert(Node::new(Box::new(clean_element())));
            let color = $color;
            link_child(&mut arena, root, child);

            arena.refresh_subtree_dirty_cache(root);
            assert!(
                !arena
                    .cached_subtree_dirty(root)
                    .intersects(DirtyFlags::PAINT)
            );

            arena
                .mutate_element_with_invalidation(child, |element, cx| {
                    element
                        .as_any_mut()
                        .downcast_mut::<Element>()
                        .expect("element")
                        .$setter(color, cx);
                })
                .expect("child exists");

            let render_state = arena
                .get(child)
                .expect("child exists")
                .element
                .as_any()
                .downcast_ref::<Element>()
                .expect("element")
                .debug_render_state();
            assert_eq!(render_state.$field, color.to_rgba_u8());
            assert_element_and_arena_paint_dirty(&arena, root, child);
        }
    };
}

border_color_with_invalidation_test!(
    element_border_top_color_with_invalidation_updates_value_and_paint_dirty,
    set_border_top_color_with_invalidation,
    border_top_rgba,
    Color::rgba(11, 22, 33, 210)
);
border_color_with_invalidation_test!(
    element_border_right_color_with_invalidation_updates_value_and_paint_dirty,
    set_border_right_color_with_invalidation,
    border_right_rgba,
    Color::rgba(44, 55, 66, 220)
);
border_color_with_invalidation_test!(
    element_border_bottom_color_with_invalidation_updates_value_and_paint_dirty,
    set_border_bottom_color_with_invalidation,
    border_bottom_rgba,
    Color::rgba(77, 88, 99, 230)
);
border_color_with_invalidation_test!(
    element_border_left_color_with_invalidation_updates_value_and_paint_dirty,
    set_border_left_color_with_invalidation,
    border_left_rgba,
    Color::rgba(101, 112, 123, 240)
);

#[test]
fn mutate_element_ref_with_invalidation_bubbles_invalidated_flags() {
    let mut arena = NodeArena::new();
    let root = insert_test_node(&mut arena, 1, DirtyFlags::NONE);
    let child = insert_test_node(&mut arena, 2, DirtyFlags::NONE);
    link_child(&mut arena, root, child);

    arena.refresh_subtree_dirty_cache(root);
    assert_eq!(arena.arena_local_dirty(child), DirtyFlags::NONE);
    assert!(
        !arena
            .cached_subtree_dirty(root)
            .intersects(DirtyFlags::HIT_TEST)
    );

    arena
        .mutate_element_ref_with_invalidation(child, |_element, cx| {
            cx.invalidate(DirtyFlags::HIT_TEST);
        })
        .expect("child exists");

    assert!(
        arena
            .arena_local_dirty(child)
            .contains(DirtyFlags::HIT_TEST)
    );
    assert!(
        arena
            .cached_subtree_dirty(child)
            .intersects(DirtyFlags::HIT_TEST)
    );
    assert!(
        arena
            .cached_subtree_dirty(root)
            .intersects(DirtyFlags::HIT_TEST)
    );
}

#[test]
fn mutate_element_ref_with_invalidation_context_can_clear_arena_dirty() {
    let mut arena = NodeArena::new();
    let root = insert_test_node(&mut arena, 1, DirtyFlags::NONE);
    let child = insert_test_node(&mut arena, 2, DirtyFlags::NONE);
    link_child(&mut arena, root, child);

    arena.refresh_subtree_dirty_cache(root);
    assert_cached_paint_clean(&arena, child);
    assert_cached_paint_clean(&arena, root);

    arena
        .mutate_element_ref_with_invalidation(child, |_element, cx| {
            cx.invalidate(DirtyFlags::PAINT);
            cx.clear_arena_dirty(DirtyFlags::PAINT);
        })
        .expect("child exists");

    assert_eq!(arena.arena_local_dirty(child), DirtyFlags::NONE);
    assert_cached_paint_clean(&arena, child);
    assert_cached_paint_clean(&arena, root);
}

#[test]
fn mutate_element_without_invalidation_keeps_arena_local_dirty_unchanged() {
    let mut arena = NodeArena::new();
    let node = insert_test_node(&mut arena, 1, DirtyFlags::NONE);

    let before = arena.arena_local_dirty(node);
    arena
        .mutate_element_with_invalidation(node, |element, _cx| {
            element
                .as_any_mut()
                .downcast_mut::<TestElement>()
                .expect("test element")
                .dirty_flags = DirtyFlags::PAINT;
        })
        .expect("node exists");

    assert_eq!(arena.arena_local_dirty(node), before);
}
