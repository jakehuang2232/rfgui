use super::*;

#[test]
fn drag_drop_retargets_after_drag_over_rerender() {
    let dropped = global_state(|| Vec::<String>::new());
    let mut viewport = Viewport::new();
    viewport.set_size(200, 120);

    viewport
        .render_rsx(&drag_drop_rerender_tree(false, dropped.binding()))
        .expect("cold render");
    run_layout_for_test(&mut viewport, 200.0, 120.0);
    let old_target = viewport
        .scene
        .ui_root_keys
        .iter()
        .rev()
        .find_map(|&root_key| {
            crate::view::base_component::hit_test(&viewport.scene.node_arena, root_key, 20.0, 45.0)
        })
        .expect("initial target should hit-test");

    viewport.input_state.drag_state = Some(super::super::super::DragState {
        source_id: old_target,
        data: crate::ui::DataTransfer::default(),
        effect_allowed: DragEffect::Move,
        last_over_target: Some(old_target),
        last_drop_effect: Some(DragEffect::Move),
    });
    viewport.set_pointer_position_viewport(20.0, 45.0);

    viewport
        .render_rsx(&drag_drop_rerender_tree(true, dropped.binding()))
        .expect("drag-over indicator render");
    run_layout_for_test(&mut viewport, 200.0, 120.0);

    viewport.dispatch_pointer_up_event(crate::view::viewport::PointerButton::Left);

    assert_eq!(dropped.get(), vec!["target".to_string()]);
}

#[test]
fn retained_window_accordion_button_false_to_true_hit_tests_without_scroll() {
    let mut viewport = Viewport::new();
    viewport.set_size(460, 380);
    viewport
        .render_rsx(&retained_window_accordion_button_tree())
        .expect("render collapsed retained tree");
    run_layout_for_test(&mut viewport, 460.0, 380.0);

    let root_key = viewport.scene.ui_root_keys[0];
    let section_key = viewport.scene.node_arena.children_of(root_key)[1];
    let button_key = viewport.scene.node_arena.children_of(section_key)[0];
    let label_key = find_text_node(&viewport.scene.node_arena, root_key, "Contained")
        .expect("contained button label");

    viewport
        .scene
        .node_arena
        .mutate_element_with_invalidation(section_key, |element, cx| {
            element
                .as_any_mut()
                .downcast_mut::<crate::view::base_component::Element>()
                .expect("section element")
                .replace_style(expanded_retained_accordion_section_style());
            cx.invalidate(crate::view::base_component::DirtyFlags::ALL);
        })
        .expect("section exists");

    run_layout_for_test(&mut viewport, 460.0, 380.0);
    let post_layout = viewport.run_post_layout_transitions(1.0, 1.0);
    if post_layout.relayout_required {
        run_layout_for_test(&mut viewport, 460.0, 380.0);
    }

    assert_eq!(
        viewport.scene.node_arena.children_of(section_key)[0],
        button_key,
        "button NodeKey should be retained across false -> true expansion",
    );
    assert!(
        find_text_node(&viewport.scene.node_arena, root_key, "Contained") == Some(label_key),
        "label NodeKey should be retained across false -> true expansion",
    );

    let label_snapshot = viewport
        .scene
        .node_arena
        .get(label_key)
        .expect("contained label")
        .element
        .box_model_snapshot();
    let hit_x = label_snapshot.x + label_snapshot.width * 0.5;
    let hit_y = label_snapshot.y + label_snapshot.height * 0.5;
    let target =
        crate::view::base_component::hit_test(&viewport.scene.node_arena, root_key, hit_x, hit_y);
    assert!(
        matches!(target, Some(target) if target == label_key || target == button_key),
        "hit at retained expanded button ({hit_x}, {hit_y}) should target button branch; got {target:?}",
    );

    viewport.input_state.hovered_node_id = target;
    assert_eq!(
        viewport.resolve_cursor(),
        Cursor::Pointer,
        "retained expanded button should resolve pointer cursor before any scroll",
    );
}

#[test]
fn retained_window_accordion_button_rerender_false_to_true_hit_tests_without_scroll() {
    let collapsed = retained_window_accordion_button_tree_with_expanded(false);
    let expanded = retained_window_accordion_button_tree_with_expanded(true);

    let mut viewport = Viewport::new();
    viewport.set_size(460, 380);
    viewport.set_use_incremental_commit(true);
    viewport
        .render_rsx(&collapsed)
        .expect("render collapsed retained tree");
    run_layout_for_test(&mut viewport, 460.0, 380.0);

    let root_key = viewport.scene.ui_root_keys[0];
    let section_key = viewport.scene.node_arena.children_of(root_key)[1];
    let button_key = viewport.scene.node_arena.children_of(section_key)[0];
    let label_key = find_text_node(&viewport.scene.node_arena, root_key, "Contained")
        .expect("contained button label");

    viewport
        .render_rsx(&expanded)
        .expect("rerender expanded retained tree");
    run_layout_for_test(&mut viewport, 460.0, 380.0);
    let post_layout = viewport.run_post_layout_transitions(1.0, 1.0);
    if post_layout.relayout_required {
        run_layout_for_test(&mut viewport, 460.0, 380.0);
    }

    assert_eq!(
        viewport.scene.ui_root_keys[0], root_key,
        "root NodeKey should be retained across false -> true rerender",
    );
    assert_eq!(
        viewport.scene.node_arena.children_of(root_key)[1],
        section_key,
        "section NodeKey should be retained across false -> true rerender",
    );
    assert_eq!(
        viewport.scene.node_arena.children_of(section_key)[0],
        button_key,
        "button NodeKey should be retained across false -> true rerender",
    );
    assert_eq!(
        find_text_node(&viewport.scene.node_arena, root_key, "Contained"),
        Some(label_key),
        "label NodeKey should be retained across false -> true rerender",
    );

    let label_snapshot = viewport
        .scene
        .node_arena
        .get(label_key)
        .expect("contained label")
        .element
        .box_model_snapshot();
    let hit_x = label_snapshot.x + label_snapshot.width * 0.5;
    let hit_y = label_snapshot.y + label_snapshot.height * 0.5;
    let target =
        crate::view::base_component::hit_test(&viewport.scene.node_arena, root_key, hit_x, hit_y);
    assert!(
        matches!(target, Some(target) if target == label_key || target == button_key),
        "hit at rerendered expanded button ({hit_x}, {hit_y}) should target button branch before any scroll; got {target:?}",
    );

    viewport.input_state.hovered_node_id = target;
    assert_eq!(
        viewport.resolve_cursor(),
        Cursor::Pointer,
        "rerendered expanded button should resolve pointer cursor before any scroll",
    );
}

#[test]
fn component_test_button_section_rerender_false_to_true_hit_tests_without_scroll() {
    let collapsed = retained_component_test_button_section_tree_with_expanded(false);
    let expanded = retained_component_test_button_section_tree_with_expanded(true);

    let mut viewport = Viewport::new();
    viewport.set_size(460, 380);
    viewport.set_use_incremental_commit(true);
    viewport
        .render_rsx(&collapsed)
        .expect("render collapsed component-test-like tree");
    run_layout_for_test(&mut viewport, 460.0, 380.0);

    let root_key = viewport.scene.ui_root_keys[0];
    let contained_key = find_text_node(&viewport.scene.node_arena, root_key, "Contained")
        .expect("contained button text");
    let button_key = viewport
        .scene
        .node_arena
        .parent_of(contained_key)
        .expect("contained text parent button");

    viewport
        .render_rsx(&expanded)
        .expect("rerender expanded component-test-like tree");
    run_layout_for_test(&mut viewport, 460.0, 380.0);
    let post_layout = viewport.run_post_layout_transitions(1.0, 1.0);
    if post_layout.relayout_required {
        let profile = run_layout_for_test_with_gate_profile(&mut viewport, 460.0, 380.0);
        eprintln!("profile1 {profile:?}");
    }
    let post_layout = viewport.run_post_layout_transitions(1.0, 2.0);
    if post_layout.relayout_required {
        let profile = run_layout_for_test_with_gate_profile(&mut viewport, 460.0, 380.0);
        eprintln!("profile2 {profile:?}");
    }

    assert_eq!(
        find_text_node(&viewport.scene.node_arena, root_key, "Contained"),
        Some(contained_key),
        "contained text NodeKey should be retained across component-test-like expansion",
    );

    let contained_snapshot = viewport
        .scene
        .node_arena
        .get(contained_key)
        .expect("contained button text")
        .element
        .box_model_snapshot();
    let hit_x = contained_snapshot.x + contained_snapshot.width * 0.5;
    let hit_y = contained_snapshot.y + contained_snapshot.height * 0.5;
    let target =
        crate::view::base_component::hit_test(&viewport.scene.node_arena, root_key, hit_x, hit_y);
    {
        let bm_btn = viewport
            .scene
            .node_arena
            .get(button_key)
            .unwrap()
            .element
            .box_model_snapshot();
        let bm_txt = viewport
            .scene
            .node_arena
            .get(contained_key)
            .unwrap()
            .element
            .box_model_snapshot();
        eprintln!("DBG hit=({hit_x},{hit_y}) target={target:?}");
        eprintln!(
            "DBG button {button_key:?} x={} y={} w={} h={} render={}",
            bm_btn.x, bm_btn.y, bm_btn.width, bm_btn.height, bm_btn.should_render
        );
        eprintln!(
            "DBG text   {contained_key:?} x={} y={} w={} h={} render={}",
            bm_txt.x, bm_txt.y, bm_txt.width, bm_txt.height, bm_txt.should_render
        );
    }
    assert!(
        matches!(target, Some(target) if target == contained_key || target == button_key),
        "hit at component-test-like expanded button ({hit_x}, {hit_y}) should target the button branch before any scroll; got {target:?}, button={button_key:?}, text={contained_key:?}",
    );

    viewport.input_state.hovered_node_id = target;
    assert_eq!(
        viewport.resolve_cursor(),
        Cursor::Pointer,
        "component-test-like expanded button should resolve pointer cursor before any scroll",
    );
}
