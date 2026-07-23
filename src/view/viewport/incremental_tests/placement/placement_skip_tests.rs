use super::*;

#[test]
fn placement_skip_clean_child_does_not_call_place_and_preserves_box_models() {
    let mut viewport = Viewport::new();
    viewport
        .render_rsx(&two_child_grid_box_model_tree())
        .expect("cold render");
    run_layout_for_test(&mut viewport, 120.0, 80.0);
    viewport.refresh_frame_box_models();

    let root_key = viewport.scene.ui_root_keys[0];
    let children = viewport.scene.node_arena.children_of(root_key);
    let first_before = box_model_snapshot_for_node(&viewport, children[0]);
    let second_before = box_model_snapshot_for_node(&viewport, children[1]);
    crate::view::viewport::scene_helpers::clear_subtree_dirty_flags_with_arena_dirty(
        &mut viewport.scene.node_arena,
        root_key,
        crate::view::base_component::DirtyFlags::ALL,
    );
    mark_place_dirty_for_test(&mut viewport, root_key);

    let (gate_profile, place_profile) =
        run_layout_for_test_with_gate_profile(&mut viewport, 120.0, 80.0);

    assert_eq!(gate_profile.placement_candidate_clean_children, 2);
    assert_eq!(gate_profile.placement_dirty_children, 0);
    assert_eq!(
        place_profile.child_place_calls, 0,
        "clean in-flow children with unchanged placement context should not be placed again"
    );
    assert_eq!(place_profile.skipped_child_place_calls, 2);
    assert_eq!(place_profile.placement_skip_failures.total(), 0);

    viewport.refresh_frame_box_models();
    assert_same_box_model_snapshot(
        box_model_snapshot_for_node(&viewport, children[0]),
        first_before,
    );
    assert_same_box_model_snapshot(
        box_model_snapshot_for_node(&viewport, children[1]),
        second_before,
    );
}

#[test]
fn placement_skip_clean_child_is_visible_in_layout_trace() {
    let mut viewport = Viewport::new();
    viewport
        .render_rsx(&two_child_grid_box_model_tree())
        .expect("cold render");
    run_layout_for_test(&mut viewport, 120.0, 80.0);
    let root_key = viewport.scene.ui_root_keys[0];
    crate::view::viewport::scene_helpers::clear_subtree_dirty_flags_with_arena_dirty(
        &mut viewport.scene.node_arena,
        root_key,
        crate::view::base_component::DirtyFlags::ALL,
    );
    mark_place_dirty_for_test(&mut viewport, root_key);

    let (gate_profile, place_profile) =
        run_layout_for_test_with_gate_profile(&mut viewport, 120.0, 80.0);

    let traversal_profile = super::super::super::frame::LayoutTraversalProfile {
        root_count: 1,
        measure_candidate_clean_children: gate_profile.measure_candidate_clean_children,
        measure_dirty_children: gate_profile.measure_dirty_children,
        placement_candidate_clean_children: gate_profile.placement_candidate_clean_children,
        placement_dirty_children: gate_profile.placement_dirty_children,
        skipped_child_place_calls: place_profile.skipped_child_place_calls,
        ..Default::default()
    };
    let trace_root = super::super::super::debug::TraceRenderNode::with_children(
        "layout_traversal",
        0.0,
        super::super::super::debug::build_layout_traversal_trace_nodes(&traversal_profile),
    );
    let trace = super::super::super::debug::format_trace_render_tree(&trace_root);
    let place_trace_root = super::super::super::debug::TraceRenderNode::with_children(
        "place",
        10.0,
        super::super::super::debug::build_layout_place_trace_nodes(&place_profile),
    );
    let place_trace = super::super::super::debug::format_trace_render_tree(&place_trace_root);

    assert_eq!(place_profile.skipped_child_place_calls, 2);
    assert_eq!(place_profile.placement_skip_failures.total(), 0);
    assert!(trace.contains("skipped_child_place_calls (count=2)"));
    assert!(place_trace.contains("skipped_child_place (calls=2)"));
    assert!(place_trace.contains("placement_skip_failures (total=0"));
}

#[test]
fn placement_skip_clean_nested_non_axis_subtree_does_not_call_place() {
    let mut viewport = Viewport::new();
    viewport
        .render_rsx(&nested_grid_box_model_tree())
        .expect("cold render");
    run_layout_for_test(&mut viewport, 120.0, 80.0);
    viewport.refresh_frame_box_models();

    let root_key = viewport.scene.ui_root_keys[0];
    let wrapper_key = viewport.scene.node_arena.children_of(root_key)[0];
    let leaf_key = viewport.scene.node_arena.children_of(wrapper_key)[0];
    let wrapper_before = box_model_snapshot_for_node(&viewport, wrapper_key);
    let leaf_before = box_model_snapshot_for_node(&viewport, leaf_key);
    crate::view::viewport::scene_helpers::clear_subtree_dirty_flags_with_arena_dirty(
        &mut viewport.scene.node_arena,
        root_key,
        crate::view::base_component::DirtyFlags::ALL,
    );
    mark_place_dirty_for_test(&mut viewport, root_key);

    let (_gate_profile, place_profile) =
        run_layout_for_test_with_gate_profile(&mut viewport, 120.0, 80.0);

    assert_eq!(place_profile.child_place_calls, 0);
    assert_eq!(place_profile.skipped_child_place_calls, 1);
    assert_eq!(place_profile.placement_skip_failures.non_leaf, 0);
    assert_eq!(place_profile.placement_skip_failures.total(), 0);

    viewport.refresh_frame_box_models();
    assert_same_box_model_snapshot(
        box_model_snapshot_for_node(&viewport, wrapper_key),
        wrapper_before,
    );
    assert_same_box_model_snapshot(
        box_model_snapshot_for_node(&viewport, leaf_key),
        leaf_before,
    );
}

#[test]
fn placement_skip_clean_nested_subtree_preserves_descendant_hit_test() {
    let mut viewport = Viewport::new();
    viewport
        .render_rsx(&nested_grid_box_model_tree())
        .expect("cold render");
    run_layout_for_test(&mut viewport, 120.0, 80.0);
    viewport.refresh_frame_box_models();

    let root_key = viewport.scene.ui_root_keys[0];
    let wrapper_key = viewport.scene.node_arena.children_of(root_key)[0];
    let leaf_key = viewport.scene.node_arena.children_of(wrapper_key)[0];
    let leaf_before = box_model_snapshot_for_node(&viewport, leaf_key);
    crate::view::viewport::scene_helpers::clear_subtree_dirty_flags_with_arena_dirty(
        &mut viewport.scene.node_arena,
        root_key,
        crate::view::base_component::DirtyFlags::ALL,
    );
    mark_place_dirty_for_test(&mut viewport, root_key);

    let (_gate_profile, place_profile) =
        run_layout_for_test_with_gate_profile(&mut viewport, 120.0, 80.0);

    assert_eq!(place_profile.child_place_calls, 0);
    assert_eq!(place_profile.skipped_child_place_calls, 1);
    assert_eq!(place_profile.placement_skip_failures.total(), 0);

    let target = crate::view::base_component::hit_test(
        &viewport.scene.node_arena,
        root_key,
        leaf_before.x + leaf_before.width * 0.5,
        leaf_before.y + leaf_before.height * 0.5,
    );
    assert_eq!(
        target,
        Some(leaf_key),
        "skipped nested subtree must retain descendant hit-test bounds",
    );
}

#[test]
fn placement_skip_does_not_skip_dirty_child() {
    let mut viewport = Viewport::new();
    viewport
        .render_rsx(&two_child_grid_box_model_tree())
        .expect("cold render");
    run_layout_for_test(&mut viewport, 120.0, 80.0);
    let root_key = viewport.scene.ui_root_keys[0];
    crate::view::viewport::scene_helpers::clear_subtree_dirty_flags_with_arena_dirty(
        &mut viewport.scene.node_arena,
        root_key,
        crate::view::base_component::DirtyFlags::ALL,
    );
    let dirty_child = viewport.scene.node_arena.children_of(root_key)[0];
    mark_place_dirty_for_test(&mut viewport, dirty_child);

    let (gate_profile, place_profile) =
        run_layout_for_test_with_gate_profile(&mut viewport, 120.0, 80.0);

    assert_eq!(gate_profile.placement_candidate_clean_children, 1);
    assert_eq!(gate_profile.placement_dirty_children, 1);
    assert_eq!(place_profile.child_place_calls, 1);
    assert_eq!(place_profile.skipped_child_place_calls, 1);
    assert_eq!(place_profile.placement_skip_failures.dirty_subtree, 1);
    assert_eq!(place_profile.placement_skip_failures.total(), 1);
}

#[test]
fn placement_skip_does_not_skip_nested_subtree_with_dirty_descendant() {
    let mut viewport = Viewport::new();
    viewport
        .render_rsx(&nested_grid_box_model_tree())
        .expect("cold render");
    run_layout_for_test(&mut viewport, 120.0, 80.0);
    let root_key = viewport.scene.ui_root_keys[0];
    crate::view::viewport::scene_helpers::clear_subtree_dirty_flags_with_arena_dirty(
        &mut viewport.scene.node_arena,
        root_key,
        crate::view::base_component::DirtyFlags::ALL,
    );
    let wrapper_key = viewport.scene.node_arena.children_of(root_key)[0];
    let dirty_leaf = viewport.scene.node_arena.children_of(wrapper_key)[0];
    mark_place_dirty_for_test(&mut viewport, dirty_leaf);

    let (_gate_profile, place_profile) =
        run_layout_for_test_with_gate_profile(&mut viewport, 120.0, 80.0);

    assert_eq!(place_profile.skipped_child_place_calls, 0);
    assert!(
        place_profile.child_place_calls >= 1,
        "dirty descendant must force placement through the subtree"
    );
    assert_eq!(place_profile.placement_skip_failures.dirty_subtree, 1);
    assert_eq!(place_profile.placement_skip_failures.total(), 1);
}

#[test]
fn placement_skip_does_not_skip_when_child_placement_context_changes() {
    let mut viewport = Viewport::new();
    viewport
        .render_rsx(&two_child_grid_box_model_tree())
        .expect("cold render");
    run_layout_for_test(&mut viewport, 120.0, 80.0);
    let root_key = viewport.scene.ui_root_keys[0];
    crate::view::viewport::scene_helpers::clear_subtree_dirty_flags_with_arena_dirty(
        &mut viewport.scene.node_arena,
        root_key,
        crate::view::base_component::DirtyFlags::ALL,
    );

    let (gate_profile, place_profile) =
        run_layout_for_test_with_gate_profile(&mut viewport, 140.0, 80.0);

    assert_eq!(gate_profile.placement_candidate_clean_children, 2);
    assert_eq!(gate_profile.placement_dirty_children, 0);
    assert_eq!(
        place_profile.child_place_calls, 2,
        "clean children must still be placed when the child placement key changes"
    );
    assert_eq!(place_profile.skipped_child_place_calls, 0);
    assert_eq!(place_profile.placement_skip_failures.placement_mismatch, 2);
    assert_eq!(place_profile.placement_skip_failures.total(), 2);
}

#[test]
fn placement_skip_does_not_skip_nested_subtree_when_context_changes() {
    let mut viewport = Viewport::new();
    viewport
        .render_rsx(&nested_grid_box_model_tree())
        .expect("cold render");
    run_layout_for_test(&mut viewport, 120.0, 80.0);
    let root_key = viewport.scene.ui_root_keys[0];
    crate::view::viewport::scene_helpers::clear_subtree_dirty_flags_with_arena_dirty(
        &mut viewport.scene.node_arena,
        root_key,
        crate::view::base_component::DirtyFlags::ALL,
    );

    let (_gate_profile, place_profile) =
        run_layout_for_test_with_gate_profile(&mut viewport, 140.0, 80.0);

    assert_eq!(place_profile.skipped_child_place_calls, 0);
    assert!(
        place_profile.child_place_calls >= 1,
        "placement key change must force nested subtree placement"
    );
    assert_eq!(place_profile.placement_skip_failures.placement_mismatch, 1);
    assert_eq!(place_profile.placement_skip_failures.total(), 1);
}

#[test]
fn placement_skip_does_not_skip_scroll_offset_context_change() {
    let mut viewport = Viewport::new();
    viewport
        .render_rsx(&scrollable_grid_box_model_tree())
        .expect("cold render");
    run_layout_for_test(&mut viewport, 100.0, 50.0);
    viewport.refresh_frame_box_models();

    let root_key = viewport.scene.ui_root_keys[0];
    let root_id = viewport
        .scene
        .node_arena
        .get(root_key)
        .expect("root exists")
        .element
        .stable_id();
    crate::view::viewport::scene_helpers::clear_subtree_dirty_flags_with_arena_dirty(
        &mut viewport.scene.node_arena,
        root_key,
        crate::view::base_component::DirtyFlags::ALL,
    );

    assert!(crate::view::viewport::dispatch::set_scroll_offset_by_id(
        &viewport.scene.node_arena,
        root_key,
        root_id,
        (24.0, 16.0),
    ));

    let (_gate_profile, place_profile) =
        run_layout_for_test_with_gate_profile(&mut viewport, 100.0, 50.0);

    assert_eq!(place_profile.child_place_calls, 1);
    assert_eq!(place_profile.skipped_child_place_calls, 0);
    assert_eq!(place_profile.placement_skip_failures.placement_mismatch, 1);
    assert_eq!(place_profile.placement_skip_failures.total(), 1);
}

#[test]
fn placement_skip_does_not_skip_nested_subtree_with_active_layout_transition_runtime_state() {
    let mut viewport = Viewport::new();
    viewport
        .render_rsx(&nested_grid_box_model_tree())
        .expect("cold render");
    run_layout_for_test(&mut viewport, 120.0, 80.0);

    let root_key = viewport.scene.ui_root_keys[0];
    let wrapper_key = viewport.scene.node_arena.children_of(root_key)[0];
    let wrapper_id = viewport
        .scene
        .node_arena
        .get(wrapper_key)
        .expect("wrapper exists")
        .element
        .stable_id();

    assert!(
        crate::view::viewport::transitions_tick::set_layout_field_by_id(
            &mut viewport.scene.node_arena,
            root_key,
            wrapper_id,
            crate::transition::LayoutField::Width,
            72.0,
        )
    );
    crate::view::viewport::scene_helpers::clear_subtree_dirty_flags_with_arena_dirty(
        &mut viewport.scene.node_arena,
        root_key,
        crate::view::base_component::DirtyFlags::ALL,
    );
    mark_place_dirty_for_test(&mut viewport, root_key);

    let (_gate_profile, place_profile) =
        run_layout_for_test_with_gate_profile(&mut viewport, 120.0, 80.0);

    assert_eq!(place_profile.skipped_child_place_calls, 0);
    assert!(
        place_profile.child_place_calls >= 1,
        "active transition runtime state must force placement traversal"
    );
    assert_eq!(place_profile.placement_skip_failures.runtime_state, 1);
    assert_eq!(place_profile.placement_skip_failures.total(), 1);
}

#[test]
fn placement_skip_does_not_skip_nested_subtree_with_anchor_descendant() {
    let place_profile = sample_clean_parent_relayout_for_placement_profile(
        &nested_grid_with_anchor_descendant_tree(),
        120.0,
        80.0,
    );

    assert_eq!(place_profile.skipped_child_place_calls, 0);
    assert!(
        place_profile.child_place_calls >= 1,
        "anchor descendants need placement runtime replay"
    );
    assert_eq!(place_profile.placement_skip_failures.anchor_name, 1);
    assert_eq!(place_profile.placement_skip_failures.total(), 1);
}

#[test]
fn placement_skip_does_not_skip_nested_subtree_with_absolute_descendant() {
    let place_profile = sample_clean_parent_relayout_for_placement_profile(
        &nested_grid_with_absolute_descendant_tree(),
        120.0,
        80.0,
    );

    assert_eq!(place_profile.skipped_child_place_calls, 0);
    assert!(
        place_profile.child_place_calls >= 1,
        "absolute descendants are still excluded from the Phase 4l expansion"
    );
    assert_eq!(place_profile.placement_skip_failures.absolute_descendant, 1);
    assert_eq!(place_profile.placement_skip_failures.total(), 1);
}

#[test]
fn placement_skip_does_not_skip_nested_subtree_with_text_area_descendant() {
    let place_profile = sample_clean_parent_relayout_for_placement_profile(
        &nested_grid_with_text_area_descendant_tree(),
        180.0,
        100.0,
    );

    assert_eq!(place_profile.skipped_child_place_calls, 0);
    assert!(
        place_profile.child_place_calls >= 1,
        "TextArea descendants are non-base elements and must not be replay-skipped",
    );
    assert_eq!(place_profile.placement_skip_failures.non_base_element, 1);
    assert_eq!(place_profile.placement_skip_failures.total(), 1);
}

#[test]
fn placement_skip_ignores_paint_only_dirty_and_reuses_box_model_cache() {
    let mut viewport = Viewport::new();
    viewport
        .render_rsx(&two_child_box_model_tree())
        .expect("cold render");
    run_layout_for_test(&mut viewport, 120.0, 80.0);
    viewport.refresh_frame_box_models();
    let root_key = viewport.scene.ui_root_keys[0];
    crate::view::viewport::scene_helpers::clear_subtree_dirty_flags_with_arena_dirty(
        &mut viewport.scene.node_arena,
        root_key,
        crate::view::base_component::DirtyFlags::ALL,
    );
    let paint_child = viewport.scene.node_arena.children_of(root_key)[0];
    mark_paint_dirty_for_test(&mut viewport, paint_child);

    let (_gate_profile, place_profile) =
        run_layout_for_test_with_gate_profile(&mut viewport, 120.0, 80.0);

    assert_eq!(place_profile.child_place_calls, 0);
    assert_eq!(
        place_profile.skipped_child_place_calls, 0,
        "paint-only dirty should let the clean root placement early-return"
    );
    assert!(
        viewport
            .scene
            .node_arena
            .cached_subtree_dirty(root_key)
            .contains(crate::view::base_component::DirtyPassMask::PAINT)
    );

    viewport.refresh_frame_box_models();
    assert_eq!(viewport.box_model_refresh_stats().collected_roots, 0);
    assert_eq!(viewport.box_model_refresh_stats().reused_roots, 1);
}
