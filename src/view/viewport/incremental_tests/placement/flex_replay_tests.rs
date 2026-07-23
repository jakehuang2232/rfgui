use super::*;

#[test]
fn phase_5d_flex_clean_base_only_subtree_replays_without_stale_hit_test() {
    let mut viewport = Viewport::new();
    viewport.set_size(240, 80);
    viewport
        .render_rsx(&flex_nested_base_only_axis_workload_tree())
        .expect("cold render");
    run_layout_for_test(&mut viewport, 240.0, 80.0);
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
        run_layout_for_test_with_gate_profile(&mut viewport, 240.0, 80.0);

    assert_eq!(place_profile.child_place_calls, 0);
    assert_eq!(place_profile.skipped_child_place_calls, 2);
    assert_eq!(place_profile.placement_skip_failures.total(), 0);
    assert_eq!(
        place_profile
            .axis_placement_eligibility
            .flex_potential_replay_child_places,
        2
    );

    viewport.refresh_frame_box_models();
    assert_same_box_model_snapshot(
        box_model_snapshot_for_node(&viewport, wrapper_key),
        wrapper_before,
    );
    assert_same_box_model_snapshot(
        box_model_snapshot_for_node(&viewport, leaf_key),
        leaf_before,
    );
    let target = crate::view::base_component::hit_test(
        &viewport.scene.node_arena,
        root_key,
        leaf_before.x + leaf_before.width * 0.5,
        leaf_before.y + leaf_before.height * 0.5,
    );
    assert_eq!(target, Some(leaf_key));
}

#[test]
fn phase_5d_flex_dirty_descendant_does_not_replay_skip() {
    let mut viewport = Viewport::new();
    viewport.set_size(240, 80);
    viewport
        .render_rsx(&flex_nested_base_only_axis_workload_tree())
        .expect("cold render");
    run_layout_for_test(&mut viewport, 240.0, 80.0);

    let root_key = viewport.scene.ui_root_keys[0];
    let wrapper_key = viewport.scene.node_arena.children_of(root_key)[0];
    let dirty_leaf = viewport.scene.node_arena.children_of(wrapper_key)[0];
    crate::view::viewport::scene_helpers::clear_subtree_dirty_flags_with_arena_dirty(
        &mut viewport.scene.node_arena,
        root_key,
        crate::view::base_component::DirtyFlags::ALL,
    );
    mark_place_dirty_for_test(&mut viewport, root_key);
    mark_place_dirty_for_test(&mut viewport, dirty_leaf);

    let (_gate_profile, place_profile) =
        run_layout_for_test_with_gate_profile(&mut viewport, 240.0, 80.0);

    assert!(
        place_profile.child_place_calls >= 1,
        "dirty descendant must force flex child placement"
    );
    assert_eq!(place_profile.skipped_child_place_calls, 1);
    assert_eq!(place_profile.placement_skip_failures.dirty_subtree, 1);
}

#[test]
fn phase_5d_flex_context_changes_do_not_replay_skip() {
    let mut viewport = Viewport::new();
    viewport.set_size(240, 80);
    viewport
        .render_rsx(&flex_base_only_axis_workload_tree_with_gap(0.0))
        .expect("cold render");
    run_layout_for_test(&mut viewport, 240.0, 80.0);
    let root_key = viewport.scene.ui_root_keys[0];
    crate::view::viewport::scene_helpers::clear_subtree_dirty_flags_with_arena_dirty(
        &mut viewport.scene.node_arena,
        root_key,
        crate::view::base_component::DirtyFlags::ALL,
    );
    viewport
        .render_rsx(&flex_base_only_axis_workload_tree_with_gap(12.0))
        .expect("gap rerender");
    let (_gate_profile, gap_profile) =
        run_layout_for_test_with_gate_profile(&mut viewport, 240.0, 80.0);

    assert_eq!(gap_profile.skipped_child_place_calls, 0);
    assert_eq!(gap_profile.child_place_calls, 2);

    crate::view::viewport::scene_helpers::clear_subtree_dirty_flags_with_arena_dirty(
        &mut viewport.scene.node_arena,
        root_key,
        crate::view::base_component::DirtyFlags::ALL,
    );
    viewport
        .render_rsx(&flex_base_only_column_axis_workload_tree())
        .expect("axis direction rerender");
    let (_gate_profile, direction_profile) =
        run_layout_for_test_with_gate_profile(&mut viewport, 240.0, 80.0);

    assert_eq!(direction_profile.skipped_child_place_calls, 0);
    assert_eq!(direction_profile.child_place_calls, 2);
}

#[test]
fn phase_5d_flex_available_size_change_does_not_replay_skip() {
    let mut viewport = Viewport::new();
    viewport.set_size(240, 80);
    viewport
        .render_rsx(&flex_base_only_axis_workload_tree())
        .expect("cold render");
    run_layout_for_test(&mut viewport, 240.0, 80.0);

    let root_key = viewport.scene.ui_root_keys[0];
    crate::view::viewport::scene_helpers::clear_subtree_dirty_flags_with_arena_dirty(
        &mut viewport.scene.node_arena,
        root_key,
        crate::view::base_component::DirtyFlags::ALL,
    );
    mark_place_dirty_for_test(&mut viewport, root_key);

    let (_gate_profile, place_profile) =
        run_layout_for_test_with_gate_profile(&mut viewport, 260.0, 80.0);

    assert_eq!(place_profile.skipped_child_place_calls, 0);
    assert_eq!(place_profile.child_place_calls, 2);
    assert_eq!(place_profile.placement_skip_failures.placement_mismatch, 2);
}

#[test]
fn phase_5d_flex_non_base_descendant_does_not_replay_skip() {
    let place_profile = sample_clean_parent_relayout_for_placement_profile(
        &flex_with_text_area_descendant_tree(),
        240.0,
        80.0,
    );

    assert_eq!(place_profile.skipped_child_place_calls, 0);
    assert!(
        place_profile.child_place_calls >= 1,
        "non-base descendant must keep the flex subtree on the normal placement path"
    );
    assert!(
        place_profile
            .axis_placement_eligibility
            .blockers
            .non_base_element
            >= 1
    );
    assert!(place_profile.placement_skip_failures.non_base_element >= 1);
}

#[test]
fn phase_5d_flow_and_inline_child_place_counts_do_not_drop() {
    let retained_accordion = sample_clean_parent_relayout_for_placement_profile(
        &retained_window_accordion_button_tree(),
        460.0,
        380.0,
    );
    let inline_text = sample_clean_parent_relayout_for_placement_profile(
        &inline_text_axis_workload_tree(),
        320.0,
        80.0,
    );

    assert_eq!(retained_accordion.child_place_calls, 2);
    assert_eq!(retained_accordion.skipped_child_place_calls, 0);
    assert_eq!(
        retained_accordion
            .axis_placement_eligibility
            .flow_child_places,
        2
    );
    // S1 inline IFC cutover: owned inline children are installed by the
    // root's IFC plan (`run_inline_ifc_root_after_place`), not routed
    // through the per-child place path — so no child_place is recorded
    // and nothing is "skipped" either.
    assert_eq!(inline_text.child_place_calls, 0);
    assert_eq!(inline_text.skipped_child_place_calls, 0);
    assert_eq!(
        inline_text.axis_placement_eligibility.inline_child_places,
        0
    );
}
