#![allow(unused_imports)]

use super::super::Viewport;
use super::common::*;
use crate::style::{
    ClipMode, Color, Cursor, Layout, Length, Padding, ParsedValue, Position, PropertyId,
    ScrollDirection, Style, Transition, TransitionProperty, Transitions, VerticalAlign,
};
use crate::ui::{
    Binding, DragEffect, RsxNode, RsxTagDescriptor, global_state, on_drag_over, on_drop, rsx,
};
use crate::view::Element as HostElement;

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
        2,
        "retained accordion children contain text descendants, so Phase 5a records a blocker"
    );

    let place_trace_root = super::super::debug::TraceRenderNode::with_children(
        "place",
        0.0,
        super::super::debug::build_layout_place_trace_nodes(&retained_accordion),
    );
    let place_trace = super::super::debug::format_trace_render_tree(&place_trace_root);
    assert!(place_trace.contains("axis_placement_eligibility (candidates=2"));
    assert!(place_trace.contains("flow=2"));
    assert!(place_trace.contains("axis_placement_blockers (total=2"));
    assert!(place_trace.contains("non_base_element=2"));
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
    crate::view::base_component::clear_subtree_dirty_flags_with_arena_dirty(
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
fn phase_5b_cached_placement_metadata_marks_base_only_nested_subtree_replayable() {
    let mut viewport = Viewport::new();
    viewport
        .render_rsx(&nested_grid_box_model_tree())
        .expect("cold render");
    run_layout_for_test(&mut viewport, 120.0, 80.0);

    let root_key = viewport.scene.ui_root_keys[0];
    let candidate = viewport.scene.node_arena.children_of(root_key)[0];
    viewport
        .scene
        .node_arena
        .refresh_subtree_dirty_cache(root_key);

    let metadata = viewport
        .scene
        .node_arena
        .cached_placement_eligibility_metadata(candidate);
    assert!(
        metadata.first_blocker().is_none(),
        "base-only nested Element subtree should have no cached placement replay blocker"
    );
    assert!(!metadata.contains_non_base_element);
    assert!(!metadata.contains_anchor_name);
    assert!(!metadata.contains_anchor_ref);
    assert!(!metadata.contains_absolute_descendant);
    assert!(!metadata.contains_runtime_layout_state);
}

#[test]
fn phase_5b_cached_placement_metadata_marks_text_area_descendant_as_non_base() {
    let mut viewport = Viewport::new();
    viewport
        .render_rsx(&nested_grid_with_text_area_descendant_tree())
        .expect("cold render");
    run_layout_for_test(&mut viewport, 180.0, 100.0);

    let root_key = viewport.scene.ui_root_keys[0];
    let candidate = viewport.scene.node_arena.children_of(root_key)[0];
    viewport
        .scene
        .node_arena
        .refresh_subtree_dirty_cache(root_key);

    let metadata = viewport
        .scene
        .node_arena
        .cached_placement_eligibility_metadata(candidate);
    assert!(metadata.contains_non_base_element);
    assert_eq!(
        metadata.first_blocker(),
        Some(crate::view::base_component::PlacementSkipFailureReason::NonBaseElement)
    );
}

#[test]
fn phase_5b_cached_placement_metadata_marks_anchor_and_absolute_descendants() {
    let mut anchor_viewport = Viewport::new();
    anchor_viewport
        .render_rsx(&nested_grid_with_anchor_descendant_tree())
        .expect("cold render");
    run_layout_for_test(&mut anchor_viewport, 120.0, 80.0);
    let anchor_root = anchor_viewport.scene.ui_root_keys[0];
    let anchor_candidate = anchor_viewport.scene.node_arena.children_of(anchor_root)[0];
    anchor_viewport
        .scene
        .node_arena
        .refresh_subtree_dirty_cache(anchor_root);
    let anchor_metadata = anchor_viewport
        .scene
        .node_arena
        .cached_placement_eligibility_metadata(anchor_candidate);
    assert!(anchor_metadata.contains_anchor_name);
    assert_eq!(
        anchor_metadata.first_blocker(),
        Some(crate::view::base_component::PlacementSkipFailureReason::AnchorName)
    );

    let mut absolute_viewport = Viewport::new();
    absolute_viewport
        .render_rsx(&nested_grid_with_absolute_descendant_tree())
        .expect("cold render");
    run_layout_for_test(&mut absolute_viewport, 120.0, 80.0);
    let absolute_root = absolute_viewport.scene.ui_root_keys[0];
    let absolute_candidate = absolute_viewport
        .scene
        .node_arena
        .children_of(absolute_root)[0];
    absolute_viewport
        .scene
        .node_arena
        .refresh_subtree_dirty_cache(absolute_root);
    let absolute_metadata = absolute_viewport
        .scene
        .node_arena
        .cached_placement_eligibility_metadata(absolute_candidate);
    assert!(absolute_metadata.contains_absolute_descendant);
    assert_eq!(
        absolute_metadata.first_blocker(),
        Some(crate::view::base_component::PlacementSkipFailureReason::AbsoluteDescendant)
    );
}

#[test]
fn phase_5b_cached_placement_metadata_refreshes_after_anchor_mutation() {
    let mut viewport = Viewport::new();
    viewport
        .render_rsx(&nested_grid_box_model_tree())
        .expect("cold render");
    run_layout_for_test(&mut viewport, 120.0, 80.0);

    let root_key = viewport.scene.ui_root_keys[0];
    let candidate = viewport.scene.node_arena.children_of(root_key)[0];
    let descendant = viewport.scene.node_arena.children_of(candidate)[0];
    viewport
        .scene
        .node_arena
        .refresh_subtree_dirty_cache(root_key);
    assert!(
        !viewport
            .scene
            .node_arena
            .cached_placement_eligibility_metadata(candidate)
            .contains_anchor_name
    );

    viewport
        .scene
        .node_arena
        .mutate_element_with_invalidation(descendant, |element, cx| {
            element
                .as_any_mut()
                .downcast_mut::<crate::view::base_component::Element>()
                .expect("descendant element")
                .set_anchor_name(Some(crate::style::AnchorName::new("phase_5b_anchor")));
            cx.invalidate(crate::view::base_component::DirtyPassMask::PLACEMENT);
        })
        .expect("descendant exists");

    assert!(
        viewport.scene.node_arena.subtree_dirty_intersects(
            candidate,
            crate::view::base_component::DirtyPassMask::PLACEMENT,
        ),
        "dirty cache remains the first guard while metadata may be stale"
    );
    viewport
        .scene
        .node_arena
        .refresh_subtree_dirty_cache(root_key);
    let refreshed = viewport
        .scene
        .node_arena
        .cached_placement_eligibility_metadata(candidate);
    assert!(refreshed.contains_anchor_name);
    assert_eq!(
        refreshed.first_blocker(),
        Some(crate::view::base_component::PlacementSkipFailureReason::AnchorName)
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
    assert_eq!(axis.potential_replay_child_places, 0);
    assert_eq!(axis.flow_potential_replay_child_places, 0);
    assert_eq!(axis.blockers.non_base_element, 2);

    let place_trace_root = super::super::debug::TraceRenderNode::with_children(
        "place",
        0.0,
        super::super::debug::build_layout_place_trace_nodes(&retained_accordion),
    );
    let place_trace = super::super::debug::format_trace_render_tree(&place_trace_root);
    assert!(place_trace.contains("axis_placement_eligibility (candidates=2"));
    assert!(place_trace.contains("potential_replay=0"));
    assert!(place_trace.contains("flow=2"));
    assert!(place_trace.contains("axis_placement_potential_replay_by_layout"));
    assert!(place_trace.contains("flow=0"));
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
    crate::view::base_component::clear_subtree_dirty_flags_with_arena_dirty(
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
    crate::view::base_component::clear_subtree_dirty_flags_with_arena_dirty(
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
    crate::view::base_component::clear_subtree_dirty_flags_with_arena_dirty(
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

    crate::view::base_component::clear_subtree_dirty_flags_with_arena_dirty(
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
    crate::view::base_component::clear_subtree_dirty_flags_with_arena_dirty(
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
    assert_eq!(inline_text.child_place_calls, 1);
    assert_eq!(inline_text.skipped_child_place_calls, 0);
    assert_eq!(
        inline_text.axis_placement_eligibility.inline_child_places,
        1
    );
}

#[test]
fn phase_5c_axis_trace_counts_inline_non_base_blockers_without_skipping() {
    let inline_text = sample_clean_parent_relayout_for_placement_profile(
        &inline_text_axis_workload_tree(),
        320.0,
        80.0,
    );
    let axis = inline_text.axis_placement_eligibility;

    assert_eq!(inline_text.child_place_calls, 1);
    assert_eq!(inline_text.skipped_child_place_calls, 0);
    assert_eq!(axis.candidate_child_places, inline_text.child_place_calls);
    assert_eq!(axis.clean_subtree_child_places, 1);
    assert_eq!(axis.dirty_subtree_child_places, 0);
    assert_eq!(axis.inline_child_places, 1);
    assert_eq!(axis.potential_replay_child_places, 0);
    assert_eq!(axis.inline_potential_replay_child_places, 0);
    assert_eq!(axis.blockers.non_base_element, 1);
}

#[test]
fn layout_gate_profile_counts_clean_children_as_candidates_without_skipping() {
    let mut viewport = Viewport::new();
    viewport
        .render_rsx(&two_child_grid_box_model_tree())
        .expect("cold render");
    run_layout_for_test(&mut viewport, 120.0, 80.0);
    let root_key = viewport.scene.ui_root_keys[0];
    crate::view::base_component::clear_subtree_dirty_flags_with_arena_dirty(
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
    crate::view::base_component::clear_subtree_dirty_flags_with_arena_dirty(
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
    crate::view::base_component::clear_subtree_dirty_flags_with_arena_dirty(
        &mut viewport.scene.node_arena,
        root_key,
        crate::view::base_component::DirtyFlags::ALL,
    );
    mark_place_dirty_for_test(&mut viewport, root_key);

    let (gate_profile, place_profile) =
        run_layout_for_test_with_gate_profile(&mut viewport, 120.0, 80.0);

    let traversal_profile = super::super::frame::LayoutTraversalProfile {
        root_count: 1,
        measure_candidate_clean_children: gate_profile.measure_candidate_clean_children,
        measure_dirty_children: gate_profile.measure_dirty_children,
        placement_candidate_clean_children: gate_profile.placement_candidate_clean_children,
        placement_dirty_children: gate_profile.placement_dirty_children,
        skipped_child_place_calls: place_profile.skipped_child_place_calls,
        ..Default::default()
    };
    let trace_root = super::super::debug::TraceRenderNode::with_children(
        "layout_traversal",
        0.0,
        super::super::debug::build_layout_traversal_trace_nodes(&traversal_profile),
    );
    let trace = super::super::debug::format_trace_render_tree(&trace_root);
    let place_trace_root = super::super::debug::TraceRenderNode::with_children(
        "place",
        0.0,
        super::super::debug::build_layout_place_trace_nodes(&place_profile),
    );
    let place_trace = super::super::debug::format_trace_render_tree(&place_trace_root);

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
    crate::view::base_component::clear_subtree_dirty_flags_with_arena_dirty(
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
    crate::view::base_component::clear_subtree_dirty_flags_with_arena_dirty(
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
fn layout_gate_profile_excludes_dirty_child_and_still_places_it() {
    let mut viewport = Viewport::new();
    viewport
        .render_rsx(&two_child_grid_box_model_tree())
        .expect("cold render");
    run_layout_for_test(&mut viewport, 120.0, 80.0);
    let root_key = viewport.scene.ui_root_keys[0];
    crate::view::base_component::clear_subtree_dirty_flags_with_arena_dirty(
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

#[test]
fn placement_skip_does_not_skip_dirty_child() {
    let mut viewport = Viewport::new();
    viewport
        .render_rsx(&two_child_grid_box_model_tree())
        .expect("cold render");
    run_layout_for_test(&mut viewport, 120.0, 80.0);
    let root_key = viewport.scene.ui_root_keys[0];
    crate::view::base_component::clear_subtree_dirty_flags_with_arena_dirty(
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
    crate::view::base_component::clear_subtree_dirty_flags_with_arena_dirty(
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
    crate::view::base_component::clear_subtree_dirty_flags_with_arena_dirty(
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
    crate::view::base_component::clear_subtree_dirty_flags_with_arena_dirty(
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
    crate::view::base_component::clear_subtree_dirty_flags_with_arena_dirty(
        &mut viewport.scene.node_arena,
        root_key,
        crate::view::base_component::DirtyFlags::ALL,
    );

    assert!(crate::view::base_component::set_scroll_offset_by_id(
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

    assert!(crate::view::base_component::set_layout_field_by_id(
        &mut viewport.scene.node_arena,
        root_key,
        wrapper_id,
        crate::transition::LayoutField::Width,
        72.0,
    ));
    crate::view::base_component::clear_subtree_dirty_flags_with_arena_dirty(
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
    crate::view::base_component::clear_subtree_dirty_flags_with_arena_dirty(
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
