use super::*;

#[test]
fn phase_4k_samples_workload_placement_skip_failure_distribution() {
    let grid_leaf = sample_clean_parent_relayout_for_placement_profile(
        &two_child_grid_box_model_tree(),
        120.0,
        80.0,
    );
    assert_eq!(grid_leaf.child_place_calls, 0);
    assert_eq!(grid_leaf.skipped_child_place_calls, 2);
    assert_eq!(grid_leaf.placement_skip_failures.total(), 0);

    let nested_grid = sample_clean_parent_relayout_for_placement_profile(
        &nested_grid_box_model_tree(),
        120.0,
        80.0,
    );
    assert_eq!(nested_grid.child_place_calls, 0);
    assert_eq!(nested_grid.skipped_child_place_calls, 1);
    assert_eq!(nested_grid.placement_skip_failures.non_leaf, 0);
    assert_eq!(nested_grid.placement_skip_failures.total(), 0);

    let scrollable_grid = sample_clean_parent_relayout_for_placement_profile(
        &scrollable_grid_box_model_tree(),
        100.0,
        50.0,
    );
    assert_eq!(scrollable_grid.child_place_calls, 0);
    assert_eq!(scrollable_grid.skipped_child_place_calls, 1);
    assert_eq!(scrollable_grid.placement_skip_failures.total(), 0);

    let retained_accordion = sample_clean_parent_relayout_for_placement_profile(
        &retained_window_accordion_button_tree(),
        460.0,
        380.0,
    );
    assert_eq!(retained_accordion.child_place_calls, 2);
    assert_eq!(retained_accordion.skipped_child_place_calls, 0);
    assert_eq!(retained_accordion.placement_skip_failures.total(), 0);
}

#[test]
fn phase_5a_axis_placement_eligibility_observes_retained_flow_without_skipping() {
    let retained_accordion = sample_clean_parent_relayout_for_placement_profile(
        &retained_window_accordion_button_tree(),
        460.0,
        380.0,
    );

    assert_eq!(retained_accordion.child_place_calls, 2);
    assert_eq!(retained_accordion.skipped_child_place_calls, 0);
    assert_eq!(
        retained_accordion
            .axis_placement_eligibility
            .candidate_child_places,
        retained_accordion.child_place_calls,
        "Phase 5a observes axis candidates without reducing actual place calls"
    );
    assert_eq!(
        retained_accordion
            .axis_placement_eligibility
            .clean_subtree_child_places,
        2
    );
    assert_eq!(
        retained_accordion
            .axis_placement_eligibility
            .dirty_subtree_child_places,
        0
    );
    assert_eq!(
        retained_accordion
            .axis_placement_eligibility
            .flow_child_places,
        2
    );
    assert_eq!(
        retained_accordion
            .axis_placement_eligibility
            .blockers
            .non_base_element,
        0,
        "text descendants no longer block placement-skip (they declare \
         transparent placement eligibility via ElementTrait)"
    );

    let place_trace_root = super::super::super::debug::TraceRenderNode::with_children(
        "place",
        0.0,
        super::super::super::debug::build_layout_place_trace_nodes(&retained_accordion),
    );
    let place_trace = super::super::super::debug::format_trace_render_tree(&place_trace_root);
    assert!(place_trace.contains("axis_placement_eligibility (candidates=2"));
    assert!(place_trace.contains("flow=2"));
    assert!(place_trace.contains("axis_placement_blockers (total=0"));
    assert!(place_trace.contains("non_base_element=0"));
}

#[test]
fn phase_5a_axis_placement_eligibility_counts_dirty_and_clean_children() {
    let mut viewport = Viewport::new();
    viewport.set_size(460, 380);
    viewport
        .render_rsx(&retained_window_accordion_button_tree())
        .expect("cold render");
    run_layout_for_test(&mut viewport, 460.0, 380.0);

    let root_key = viewport.scene.ui_root_keys[0];
    let first_child = viewport
        .scene
        .node_arena
        .children_of(root_key)
        .first()
        .copied()
        .expect("retained root has first child");
    crate::view::viewport::scene_helpers::clear_subtree_dirty_flags_with_arena_dirty(
        &mut viewport.scene.node_arena,
        root_key,
        crate::view::base_component::DirtyFlags::ALL,
    );
    mark_place_dirty_for_test(&mut viewport, root_key);
    mark_place_dirty_for_test(&mut viewport, first_child);

    let (_gate_profile, place_profile) =
        run_layout_for_test_with_gate_profile(&mut viewport, 460.0, 380.0);

    assert!(
        place_profile.child_place_calls
            >= place_profile
                .axis_placement_eligibility
                .candidate_child_places,
        "Phase 5a observes axis candidates without suppressing actual child placement"
    );
    assert_eq!(place_profile.skipped_child_place_calls, 0);
    assert_eq!(
        place_profile
            .axis_placement_eligibility
            .dirty_subtree_child_places,
        1
    );
    let axis_profile = place_profile.axis_placement_eligibility;
    assert_eq!(
        axis_profile.clean_subtree_child_places + axis_profile.dirty_subtree_child_places,
        axis_profile.candidate_child_places
    );
    assert_eq!(
        place_profile
            .axis_placement_eligibility
            .blockers
            .dirty_subtree,
        1
    );
}

#[test]
fn phase_5c_axis_trace_summarizes_retained_flow_hit_rate_without_skipping() {
    let retained_accordion = sample_clean_parent_relayout_for_placement_profile(
        &retained_window_accordion_button_tree(),
        460.0,
        380.0,
    );
    let axis = retained_accordion.axis_placement_eligibility;

    assert_eq!(retained_accordion.child_place_calls, 2);
    assert_eq!(retained_accordion.skipped_child_place_calls, 0);
    assert_eq!(
        axis.candidate_child_places,
        retained_accordion.child_place_calls
    );
    assert_eq!(axis.clean_subtree_child_places, 2);
    assert_eq!(axis.dirty_subtree_child_places, 0);
    assert_eq!(axis.flow_child_places, 2);
    // Text descendants are transparent now, so the flow children record no
    // non-base blocker and become replay candidates instead (their replay
    // still fails for flow, so they are placed rather than skipped).
    assert_eq!(axis.potential_replay_child_places, 2);
    assert_eq!(axis.flow_potential_replay_child_places, 2);
    assert_eq!(axis.blockers.non_base_element, 0);

    let place_trace_root = super::super::super::debug::TraceRenderNode::with_children(
        "place",
        0.0,
        super::super::super::debug::build_layout_place_trace_nodes(&retained_accordion),
    );
    let place_trace = super::super::super::debug::format_trace_render_tree(&place_trace_root);
    assert!(place_trace.contains("axis_placement_eligibility (candidates=2"));
    assert!(place_trace.contains("potential_replay=2"));
    assert!(place_trace.contains("flow=2"));
    assert!(place_trace.contains("axis_placement_potential_replay_by_layout"));
}

#[test]
fn phase_5c_axis_trace_counts_flex_base_only_replay_candidates() {
    let flex_base = sample_clean_parent_relayout_for_placement_profile(
        &flex_base_only_axis_workload_tree(),
        240.0,
        80.0,
    );
    let axis = flex_base.axis_placement_eligibility;

    assert_eq!(flex_base.child_place_calls, 0);
    assert_eq!(flex_base.skipped_child_place_calls, 2);
    assert_eq!(axis.candidate_child_places, 2);
    assert_eq!(axis.clean_subtree_child_places, 2);
    assert_eq!(axis.dirty_subtree_child_places, 0);
    assert_eq!(axis.flex_child_places, 2);
    assert_eq!(axis.potential_replay_child_places, 2);
    assert_eq!(axis.flex_potential_replay_child_places, 2);
    assert_eq!(axis.blockers.total(), 0);
}

#[test]
fn phase_5c_axis_trace_counts_inline_non_base_blockers_without_skipping() {
    let inline_text = sample_clean_parent_relayout_for_placement_profile(
        &inline_text_axis_workload_tree(),
        320.0,
        80.0,
    );
    let axis = inline_text.axis_placement_eligibility;
    // S1 inline IFC cutover: the Text child is owned by the inline root's
    // IFC install plan, so the axis placement path never visits it — no
    // child_place, no skip, no replay candidacy, and no non-base blocker.
    assert_eq!(inline_text.child_place_calls, 0);
    assert_eq!(inline_text.skipped_child_place_calls, 0);
    assert_eq!(axis.candidate_child_places, inline_text.child_place_calls);
    assert_eq!(axis.clean_subtree_child_places, 0);
    assert_eq!(axis.dirty_subtree_child_places, 0);
    assert_eq!(axis.inline_child_places, 0);
    assert_eq!(axis.potential_replay_child_places, 0);
    assert_eq!(axis.inline_potential_replay_child_places, 0);
    assert_eq!(axis.blockers.non_base_element, 0);
}

#[test]
fn layout_gate_profile_counts_clean_children_as_candidates_without_skipping() {
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
        run_layout_for_test_with_gate_profile(&mut viewport, 120.0, 80.0);

    assert_eq!(gate_profile.measure_candidate_clean_children, 2);
    assert_eq!(gate_profile.measure_dirty_children, 0);
    assert_eq!(gate_profile.placement_candidate_clean_children, 2);
    assert_eq!(gate_profile.placement_dirty_children, 0);
    assert_eq!(
        place_profile.child_place_calls, 0,
        "Phase 4g is observational: the existing clean-root early return still governs traversal"
    );
}

#[test]
fn layout_gate_profile_excludes_dirty_child_and_still_places_it() {
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
    viewport
        .scene
        .node_arena
        .mutate_element_with_invalidation(dirty_child, |element, cx| {
            element
                .as_any_mut()
                .downcast_mut::<crate::view::base_component::Element>()
                .expect("host child")
                .mark_layout_dirty_with(cx);
        })
        .expect("child exists");

    let (gate_profile, place_profile) =
        run_layout_for_test_with_gate_profile(&mut viewport, 120.0, 80.0);

    assert_eq!(gate_profile.measure_candidate_clean_children, 1);
    assert_eq!(gate_profile.measure_dirty_children, 1);
    assert_eq!(gate_profile.placement_candidate_clean_children, 1);
    assert_eq!(gate_profile.placement_dirty_children, 1);
    assert!(
        place_profile.child_place_calls >= 1,
        "dirty child must still drive placement traversal"
    );
    assert!(
        place_profile.skipped_child_place_calls >= 1,
        "clean sibling may be skipped by the Phase 4h child placement gate"
    );
}
