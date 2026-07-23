use super::*;

#[test]
fn cursor_resolves_pointer_from_hovered_text_child_ancestor() {
    let mut viewport = Viewport::new();
    viewport.set_size(160, 40);
    viewport
        .render_rsx(&pointer_cursor_with_text_child_tree())
        .expect("render pointer cursor tree");
    run_layout_for_test(&mut viewport, 160.0, 40.0);

    let root_key = viewport.scene.ui_root_keys[0];
    let text_key =
        find_text_node(&viewport.scene.node_arena, root_key, "Hover target").expect("text child");
    let text_snapshot = viewport
        .scene
        .node_arena
        .get(text_key)
        .expect("text child")
        .element
        .box_model_snapshot();
    let target = crate::view::base_component::hit_test(
        &viewport.scene.node_arena,
        root_key,
        text_snapshot.x + text_snapshot.width * 0.5,
        text_snapshot.y + text_snapshot.height * 0.5,
    );

    assert_eq!(target, Some(text_key), "hit-test should land on text child");
    viewport.input_state.hovered_node_id = target;

    assert_eq!(
        viewport.resolve_cursor(),
        Cursor::Pointer,
        "cursor should inherit from the hovered text child's pointer ancestor",
    );
}

#[test]
fn pointer_move_cursor_respects_root_stacking_over_anchor_parent_resize_handle() {
    let mut viewport = Viewport::new();
    viewport.set_size(200, 120);
    viewport
        .render_rsx(&overlapping_root_with_anchor_parent_resize_handle_tree())
        .expect("render overlapping root tree");
    run_layout_for_test(&mut viewport, 200.0, 120.0);

    let lower_root = viewport.scene.ui_root_keys[0];
    let handle_key = viewport.scene.node_arena.children_of(lower_root)[0];
    let higher_root = viewport.scene.ui_root_keys[1];
    assert_eq!(
        crate::view::base_component::hit_test_roots(
            &viewport.scene.node_arena,
            &viewport.scene.ui_root_keys,
            101.0,
            20.0,
        ),
        Some((1, higher_root)),
        "root children follow sibling stacking; an earlier root's escape descendant is not a top layer",
    );

    viewport.set_pointer_position_viewport(101.0, 20.0);
    assert!(
        viewport.dispatch_pointer_move_event(),
        "pointer move should update hover at the resize handle",
    );
    assert_eq!(
        viewport.input_state.hovered_node_id,
        Some(higher_root),
        "production pointer-move path should hover the later root body, not an earlier root descendant",
    );
    assert_eq!(
        viewport.resolve_cursor(),
        Cursor::Default,
        "escape clipping does not promote an earlier root descendant above a later root",
    );

    let mut popup_stack = crate::view::popup_stack::PopupStack::new();
    let handle_id = viewport
        .scene
        .node_arena
        .get(handle_key)
        .expect("handle node exists")
        .element
        .stable_id();
    popup_stack.register(handle_id);
    assert_eq!(
        crate::view::base_component::hit_test_stacked(
            &viewport.scene.node_arena,
            &popup_stack,
            101.0,
            20.0,
        ),
        Some((lower_root, handle_key)),
        "PopupStack is the explicit top-layer interaction path",
    );
}

#[test]
fn hit_test_same_root_escape_descendant_respects_later_sibling_stacking() {
    let mut viewport = Viewport::new();
    viewport.set_size(220, 120);
    viewport
        .render_rsx(&same_root_escape_descendant_under_later_sibling_tree())
        .expect("render same-root stacking tree");
    run_layout_for_test(&mut viewport, 220.0, 120.0);

    let root_key = viewport.scene.ui_root_keys[0];
    let root_children = viewport.scene.node_arena.children_of(root_key);
    let earlier_parent = root_children[0];
    let escape_child = viewport.scene.node_arena.children_of(earlier_parent)[0];
    let later_sibling = root_children[1];

    assert_eq!(
        crate::view::base_component::hit_test(&viewport.scene.node_arena, root_key, 105.0, 15.0,),
        Some(later_sibling),
        "within one root, a later sibling stacks above an earlier sibling's escape descendant",
    );

    let mut popup_stack = crate::view::popup_stack::PopupStack::new();
    let escape_child_id = viewport
        .scene
        .node_arena
        .get(escape_child)
        .expect("escape child exists")
        .element
        .stable_id();
    popup_stack.register(escape_child_id);
    assert_eq!(
        crate::view::base_component::hit_test_stacked(
            &viewport.scene.node_arena,
            &popup_stack,
            105.0,
            15.0,
        ),
        Some((root_key, escape_child)),
        "PopupStack can intentionally promote an escape descendant above normal sibling stacking",
    );
}

#[test]
fn cursor_resolves_from_hovered_node_key_when_stable_ids_collide() {
    let mut lower =
        crate::view::base_component::Element::new_with_id(0xC0DE, 0.0, 0.0, 100.0, 80.0);
    let mut lower_style = Style::new();
    lower_style.insert(PropertyId::Cursor, ParsedValue::Cursor(Cursor::EwResize));
    lower.apply_style(lower_style);

    let higher = crate::view::base_component::Element::new_with_id(0xC0DE, 0.0, 0.0, 100.0, 80.0);

    let mut viewport = Viewport::new();
    let lower_key = viewport
        .scene
        .node_arena
        .insert(crate::view::node_arena::Node::new(Box::new(lower)));
    let higher_key = viewport
        .scene
        .node_arena
        .insert(crate::view::node_arena::Node::new(Box::new(higher)));
    viewport.scene.ui_root_keys = vec![lower_key, higher_key];
    viewport
        .scene
        .node_arena
        .set_roots(viewport.scene.ui_root_keys.clone());
    viewport.input_state.hovered_node_id = Some(lower_key);

    assert_eq!(
        viewport.resolve_cursor(),
        Cursor::EwResize,
        "cursor resolution must use the hovered NodeKey, not a colliding stable id from a later root"
    );
}

#[test]
fn placement_only_position_update_moves_anchor_parent_resize_handle_hit_test() {
    let mut viewport = Viewport::new();
    viewport.set_size(240, 140);
    viewport
        .render_rsx(&movable_root_with_anchor_parent_resize_handle_tree(20.0))
        .expect("render initial movable root tree");
    run_layout_for_test(&mut viewport, 240.0, 140.0);

    let root_key = viewport.scene.ui_root_keys[0];
    let handle_key = viewport.scene.node_arena.children_of(root_key)[0];
    assert_eq!(
        crate::view::base_component::hit_test_roots(
            &viewport.scene.node_arena,
            &viewport.scene.ui_root_keys,
            121.0,
            50.0,
        ),
        Some((0, handle_key)),
        "initial right resize handle should be hit-testable"
    );

    viewport
        .render_rsx(&movable_root_with_anchor_parent_resize_handle_tree(80.0))
        .expect("render moved tree through placement-only update");
    run_layout_for_test(&mut viewport, 240.0, 140.0);

    assert_ne!(
        crate::view::base_component::hit_test_roots(
            &viewport.scene.node_arena,
            &viewport.scene.ui_root_keys,
            121.0,
            50.0,
        ),
        Some((0, handle_key)),
        "old handle position must not remain clickable after placement-only move"
    );
    assert_eq!(
        crate::view::base_component::hit_test_roots(
            &viewport.scene.node_arena,
            &viewport.scene.ui_root_keys,
            181.0,
            50.0,
        ),
        Some((0, handle_key)),
        "new right resize handle position should be hit-testable after placement-only move"
    );

    viewport.set_pointer_position_viewport(181.0, 50.0);
    assert!(
        viewport.dispatch_pointer_move_event(),
        "production pointer move should update hover at the moved resize handle",
    );
    assert_eq!(
        viewport.input_state.hovered_node_id,
        Some(handle_key),
        "hover target should follow the placement-only moved resize handle"
    );
    assert_eq!(viewport.resolve_cursor(), Cursor::EwResize);
}
