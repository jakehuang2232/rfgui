use super::*;

/// Fine-grained observations for one successfully completed offscreen frame.
/// Opt-in: enabling this changes timing overhead, never rendering decisions.
#[derive(Clone, Debug, Default)]
pub struct RendererTestDiagnostics {
    /// Inclusive phase budgets. Do not add these to the exclusive details.
    pub phases_ms: Vec<(&'static str, f64)>,
    /// Exclusive time under one shared recursive timing stack. Uninstrumented
    /// work and profiler overhead are not assigned to a synthetic residual.
    pub exclusive_ms: Vec<(&'static str, f64)>,
    /// Actual observed events. Cause counters overlap: one place may have
    /// both dirty descendants and changed input. Eligibility is not a reuse.
    pub counts: Vec<(&'static str, usize)>,
}

impl RendererTestDiagnostics {
    pub(in crate::view::viewport) fn capture(
        t: &FrameTimings,
        compositor: &CompositorState,
        downstream: crate::view::base_component::LayoutPlaceProfile,
    ) -> Self {
        let profiles = [
            &t.layout_place_profile,
            &t.relayout_place_profile,
            &downstream,
        ];
        let a = &t.layout_traversal_profile;
        let b = &t.relayout_traversal_profile;
        Self {
            phases_ms: vec![
                ("measure", t.layout_measure_ms + t.relayout_measure_ms),
                ("place", t.layout_place_ms + t.relayout_place_ms),
                (
                    "box_models",
                    t.layout_invalidate_box_models_ms + t.relayout_invalidate_box_models_ms,
                ),
                (
                    "sync_registered",
                    a.sync_registered_elements_ms + b.sync_registered_elements_ms,
                ),
                (
                    "dirty_before_measure",
                    a.dirty_refresh_before_measure_ms + b.dirty_refresh_before_measure_ms,
                ),
                (
                    "dirty_before_place",
                    a.dirty_refresh_before_place_ms + b.dirty_refresh_before_place_ms,
                ),
            ],
            exclusive_ms: vec![
                (
                    "measure_body",
                    profiles.iter().map(|p| p.measure_body_ms).sum(),
                ),
                ("axis_solve", profiles.iter().map(|p| p.axis_solve_ms).sum()),
                (
                    "inline_ifc_measure",
                    profiles.iter().map(|p| p.inline_ifc_measure_ms).sum(),
                ),
                (
                    "inline_ifc_collect",
                    profiles.iter().map(|p| p.inline_ifc_collect_ms).sum(),
                ),
                (
                    "inline_ifc_candidate",
                    profiles.iter().map(|p| p.inline_ifc_candidate_ms).sum(),
                ),
                (
                    "inline_ifc_geometry",
                    profiles.iter().map(|p| p.inline_ifc_geometry_ms).sum(),
                ),
                ("place_body", profiles.iter().map(|p| p.place_body_ms).sum()),
                ("place_self", profiles.iter().map(|p| p.place_self_ms).sum()),
                (
                    "place_children",
                    profiles.iter().map(|p| p.place_children_ms).sum(),
                ),
                (
                    "place_flex_children",
                    profiles.iter().map(|p| p.place_flex_children_ms).sum(),
                ),
                (
                    "place_layout_inline",
                    profiles.iter().map(|p| p.place_layout_inline_ms).sum(),
                ),
                (
                    "place_layout_flex",
                    profiles.iter().map(|p| p.place_layout_flex_ms).sum(),
                ),
                (
                    "place_layout_flow",
                    profiles.iter().map(|p| p.place_layout_flow_ms).sum(),
                ),
                (
                    "non_axis_child_place",
                    profiles.iter().map(|p| p.non_axis_child_place_ms).sum(),
                ),
                (
                    "absolute_child_place",
                    profiles.iter().map(|p| p.absolute_child_place_ms).sum(),
                ),
                (
                    "inline_ifc_root_install",
                    profiles.iter().map(|p| p.inline_ifc_root_install_ms).sum(),
                ),
                (
                    "update_content_size",
                    profiles.iter().map(|p| p.update_content_size_ms).sum(),
                ),
                (
                    "clamp_scroll",
                    profiles.iter().map(|p| p.clamp_scroll_ms).sum(),
                ),
                (
                    "recompute_hit_test",
                    profiles.iter().map(|p| p.recompute_hit_test_ms).sum(),
                ),
                ("box_models", profiles.iter().map(|p| p.box_models_ms).sum()),
                (
                    "dirty_clear",
                    profiles.iter().map(|p| p.dirty_clear_ms).sum(),
                ),
                (
                    "property_sync",
                    profiles.iter().map(|p| p.property_sync_ms).sum(),
                ),
                (
                    "generation_sync",
                    profiles.iter().map(|p| p.generation_sync_ms).sum(),
                ),
                (
                    "change_capture",
                    profiles.iter().map(|p| p.change_capture_ms).sum(),
                ),
            ],
            counts: vec![
                (
                    "assignment_restores",
                    profiles.iter().map(|p| p.assignment_restores).sum(),
                ),
                (
                    "assignment_clears",
                    profiles.iter().map(|p| p.assignment_clears).sum(),
                ),
                (
                    "assignment_dirty_calls",
                    profiles.iter().map(|p| p.assignment_dirty_calls).sum(),
                ),
                (
                    "assignment_dirty_same_placed_size",
                    profiles
                        .iter()
                        .map(|p| p.assignment_dirty_same_placed_size)
                        .sum(),
                ),
                (
                    "assignment_dirty_previously_clean",
                    profiles
                        .iter()
                        .map(|p| p.assignment_dirty_previously_clean)
                        .sum(),
                ),
                (
                    "ifc_candidate_calls",
                    profiles.iter().map(|p| p.ifc_candidate_calls).sum(),
                ),
                (
                    "ifc_candidate_rebuilt",
                    profiles.iter().map(|p| p.ifc_candidate_rebuilt).sum(),
                ),
                (
                    "axis_solve_calls",
                    profiles.iter().map(|p| p.axis_solve_calls).sum(),
                ),
                (
                    "measure_ran_self_dirty",
                    profiles.iter().map(|p| p.measure_ran_self_dirty).sum(),
                ),
                (
                    "measure_ran_child_dirty",
                    profiles.iter().map(|p| p.measure_ran_child_dirty).sum(),
                ),
                (
                    "measure_ran_proposal_changed",
                    profiles
                        .iter()
                        .map(|p| p.measure_ran_proposal_changed)
                        .sum(),
                ),
                (
                    "ifc_measure_cheap",
                    profiles.iter().map(|p| p.ifc_measure_cheap).sum(),
                ),
                (
                    "ifc_measure_shortcircuit",
                    profiles.iter().map(|p| p.ifc_measure_shortcircuit).sum(),
                ),
                (
                    "ifc_measure_full",
                    profiles.iter().map(|p| p.ifc_measure_full).sum(),
                ),
                ("node_count", profiles.iter().map(|p| p.node_count).sum()),
                (
                    "place_returned_clean",
                    profiles.iter().map(|p| p.place_returned_clean).sum(),
                ),
                (
                    "place_self_dirty",
                    profiles.iter().map(|p| p.place_self_dirty).sum(),
                ),
                (
                    "place_descendant_dirty",
                    profiles.iter().map(|p| p.place_descendant_dirty).sum(),
                ),
                (
                    "place_ifc_dirty",
                    profiles.iter().map(|p| p.place_ifc_dirty).sum(),
                ),
                (
                    "place_input_changed",
                    profiles.iter().map(|p| p.place_input_changed).sum(),
                ),
                (
                    "place_clip_changed",
                    profiles.iter().map(|p| p.place_clip_changed).sum(),
                ),
                (
                    "axis_replay_reject_layout",
                    profiles.iter().map(|p| p.axis_replay_reject_layout).sum(),
                ),
                (
                    "axis_replay_reject_axis",
                    profiles.iter().map(|p| p.axis_replay_reject_axis).sum(),
                ),
                (
                    "axis_replay_reject_gap",
                    profiles.iter().map(|p| p.axis_replay_reject_gap).sum(),
                ),
                (
                    "inline_ifc_root_install_calls",
                    profiles
                        .iter()
                        .map(|p| p.inline_ifc_root_install_calls)
                        .sum(),
                ),
                (
                    "inline_ifc_root_install_reuse_calls",
                    profiles
                        .iter()
                        .map(|p| p.inline_ifc_root_install_reuse_calls)
                        .sum(),
                ),
                (
                    "ifc_rebuild_paint_dirty",
                    profiles.iter().map(|p| p.ifc_rebuild_paint_dirty).sum(),
                ),
                (
                    "ifc_rebuild_children_changed",
                    profiles
                        .iter()
                        .map(|p| p.ifc_rebuild_children_changed)
                        .sum(),
                ),
                (
                    "ifc_rebuild_viewport_changed",
                    profiles
                        .iter()
                        .map(|p| p.ifc_rebuild_viewport_changed)
                        .sum(),
                ),
                (
                    "ifc_rebuild_width_changed",
                    profiles.iter().map(|p| p.ifc_rebuild_width_changed).sum(),
                ),
                (
                    "skipped_child_place_calls",
                    profiles.iter().map(|p| p.skipped_child_place_calls).sum(),
                ),
                (
                    "translated_subtree_nodes",
                    profiles.iter().map(|p| p.translated_subtree_nodes).sum(),
                ),
                (
                    "axis_clean_candidates",
                    profiles
                        .iter()
                        .map(|p| p.axis_placement_eligibility.clean_subtree_child_places)
                        .sum(),
                ),
                (
                    "axis_dirty_candidates",
                    profiles
                        .iter()
                        .map(|p| p.axis_placement_eligibility.dirty_subtree_child_places)
                        .sum(),
                ),
                (
                    "property_observed",
                    compositor.property_trees.observed_nodes,
                ),
                (
                    "property_replayed",
                    compositor.property_trees.replayed_nodes,
                ),
                (
                    "generation_replayed",
                    compositor.paint_generations.native_observation_replays,
                ),
                ("recording_replayed", compositor.recording_cache.hits),
                (
                    "planning_localized",
                    compositor.planning_cache.localized_hits,
                ),
                ("planning_geometry", compositor.planning_cache.geometry_hits),
            ],
        }
    }
}
