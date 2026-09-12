use std::collections::BTreeSet;

fn declared_test_function_names(source: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let mut awaiting_function = false;
    for line in source.lines() {
        let line = line.trim();
        if line == "#[test]" {
            awaiting_function = true;
            continue;
        }
        if !awaiting_function {
            continue;
        }
        if line.is_empty() || line.starts_with("#[") || line.starts_with("//") {
            continue;
        }
        if let Some(declaration) = line.strip_prefix("fn ") {
            let name = declaration
                .split(['(', '<'])
                .next()
                .expect("test function name");
            names.insert(name.to_string());
        }
        awaiting_function = false;
    }
    names
}

fn expected_names<const N: usize>(names: [&str; N]) -> BTreeSet<String> {
    names.into_iter().map(str::to_string).collect()
}

#[test]
fn stage_c_gate_name_scan_reaches_nested_tests() {
    let source = r#"
#[test]
fn top_level_gate() {}

mod nested {
    #[test]
    fn nested_gate() {}
}
"#;
    assert_eq!(
        declared_test_function_names(source),
        expected_names(["nested_gate", "top_level_gate"]),
    );
}

/// Independent native GPU acceptance retained after the user-authorized
/// removal of the seven canary modes and their dedicated planners. Retired
/// implementation gates are intentionally removed from this closed set;
/// materialization, style-pipeline and single-Viewport behavior gates remain.
/// Their files also contain CPU tests; only `native_` names belong here.
/// Keep failing Legacy acceptance gates in the set until explicitly retired;
/// their presence does not mean their pixel/reuse assertions pass.
///
/// This inventory guards deletion and renaming only. It neither executes the
/// hardware tests in normal CI nor prevents a named test body from being
/// hollowed out. V2 pixel and reuse coverage must therefore be demonstrated
/// by running the replacement native gates, not by satisfying this name set.
#[test]
fn stage_c_retained_native_pixel_and_reuse_gate_names_are_a_closed_set() {
    let actual = [
        include_str!("../../render_pass/buffer_bindings/tests.rs"),
        include_str!("gpu_equivalence_tests/buffer_binding_tests.rs"),
        include_str!("gpu_equivalence_tests/text_buffer_tests.rs"),
        include_str!("../../render_pass/render_target/pending_submission_tests.rs"),
        include_str!("../../render_pass/render_target/working_set_tests.rs"),
        include_str!("../../../../lib/rfgui-components/tests/retained_controls.rs"),
        include_str!("../../../../examples/bin/01_window/scene_windows/particle_demo/native_tests.rs"),
        include_str!("portable_renderer_tests.rs"),
        include_str!("gpu_equivalence_tests/native_transform_surface_tests.rs"),
        include_str!("gpu_equivalence_tests/native_artifact_surface_tests.rs"),
        include_str!("gpu_equivalence_tests/native_artifact_scroll_content_tests.rs"),
        include_str!("gpu_equivalence_tests/native_artifact_surface_materialization_tests.rs"),
        include_str!("gpu_equivalence_tests/native_artifact_surface_materialization_tests/text_area_recording_tests.rs"),
        include_str!("gpu_equivalence_tests/native_artifact_surface_materialization_tests/text_area_recording_tests/viewport_tests.rs"),
        include_str!("gpu_equivalence_tests/native_artifact_surface_materialization_tests/recording_overlay_tests.rs"),
        include_str!("gpu_equivalence_tests/native_artifact_surface_materialization_tests/planning_corpus_tests/pixel_tests.rs"),
        include_str!("gpu_equivalence_tests/native_artifact_surface_materialization_tests/planning_corpus_tests/culling_reentry_tests.rs"),
        include_str!("gpu_equivalence_tests/native_artifact_surface_materialization_tests/planning_corpus_tests/gpu_source_tests.rs"),
        include_str!("gpu_equivalence_tests/native_artifact_surface_materialization_tests/planning_corpus_tests/raster_window_tests.rs"),
        include_str!("gpu_equivalence_tests/native_artifact_surface_materialization_tests/planning_corpus_tests/gpu_source_tests/lifecycle_tests.rs"),
        include_str!("gpu_equivalence_tests/native_artifact_surface_materialization_tests/planning_corpus_tests/gpu_source_tests/mixed_changes_tests.rs"),
        include_str!("gpu_equivalence_tests/native_artifact_surface_materialization_tests/planning_corpus_tests/viewport_tests.rs"),
        include_str!("gpu_equivalence_tests/native_artifact_surface_materialization_tests/planning_corpus_tests/budget_tests.rs"),
        include_str!("gpu_equivalence_tests/native_artifact_surface_materialization_tests/planning_corpus_tests/multi_target_tests.rs"),
        include_str!("gpu_equivalence_tests/native_artifact_surface_materialization_tests/style_pipeline_tests/single_viewport_tests/execution_failure_tests.rs"),
        include_str!("gpu_equivalence_tests/native_artifact_surface_materialization_tests/style_pipeline_tests/single_viewport_tests/invalidation_tests.rs"),
        include_str!("gpu_equivalence_tests/native_artifact_surface_materialization_tests/style_pipeline_tests/single_viewport_tests/residency_tests.rs"),
        include_str!("gpu_equivalence_tests/native_artifact_surface_materialization_tests/style_pipeline_tests/single_viewport_tests/resource_lifecycle_tests/pressure_tests.rs"),
        include_str!("gpu_equivalence_tests/native_artifact_surface_materialization_tests/style_pipeline_tests/single_viewport_tests/resource_lifecycle_tests/slot_content_tests.rs"),
        include_str!("gpu_equivalence_tests/native_artifact_surface_materialization_tests/style_pipeline_tests/single_viewport_tests/resource_lifecycle_tests/svg_resource_tests.rs"),
        include_str!("gpu_equivalence_tests/native_artifact_surface_materialization_tests/style_pipeline_tests/single_viewport_tests/resource_lifecycle_tests/source_transition_tests.rs"),
        include_str!("gpu_equivalence_tests/native_artifact_surface_materialization_tests/style_pipeline_tests/single_viewport_tests/resource_lifecycle_tests/wrapper_effect_tests/ancestor_slot_tests.rs"),
        include_str!("gpu_equivalence_tests/native_artifact_surface_materialization_tests/style_pipeline_tests/single_viewport_tests/resource_lifecycle_tests/wrapper_effect_tests/ready_owner_scope_tests.rs"),
        include_str!("gpu_equivalence_tests/native_artifact_surface_materialization_tests/style_pipeline_tests/single_viewport_tests/resource_lifecycle_tests/wrapper_effect_tests.rs"),
        include_str!("gpu_equivalence_tests/native_artifact_surface_materialization_tests/style_pipeline_tests/single_viewport_tests/resource_lifecycle_tests.rs"),
        include_str!("gpu_equivalence_tests/native_artifact_surface_materialization_tests/style_pipeline_tests/single_viewport_tests.rs"),
        include_str!("gpu_equivalence_tests/native_artifact_surface_materialization_tests/style_pipeline_tests.rs"),
        include_str!("gpu_equivalence_tests/native_artifact_surface_materialization_tests/text_recovery_tests.rs"),
        include_str!("gpu_equivalence_tests/native_artifact_surface_materialization_tests/text_area_recording_tests/ime_lifecycle_tests.rs"),
    ]
    .into_iter()
    .flat_map(declared_test_function_names)
    .filter(|name| name.starts_with("native_"))
    .collect::<BTreeSet<_>>();
    let expected = expected_names([
        "native_buffer_bindings_preserve_ranges_formats_and_pass_scope",
        "native_graphics_group_elides_buffers_without_losing_per_draw_uniforms",
        "native_rect_bindings_preserve_solid_and_gradient_layout_switches",
        "native_text_globals_reuse_buffers_without_overwriting_other_passes",
        "native_persistent_retirement_preserves_unsubmitted_attachments",
        "native_transient_texture_working_set_survives_pressure_between_frames",
        "native_controls_record_and_reuse_real_component_trees",
        "native_full_window_budget_descriptors_reuse_and_whole_frame_legacy",
        "native_generic_and_legacy_frozen_scrollbar_absolute_alpha",
        "native_generic_text_area_frozen_ime_caret_and_reuse",
        "native_legacy_planning_corpus_absolute_coordinates",
        "native_legacy_text_area_frozen_ime_caret_pixels",
        "native_materialization_absolute_pixels_and_reuse_at_both_dprs",
        "native_materialization_style_pipeline_legacy_pixels",
        "native_materialization_style_pipeline_scroll_executor_pixels_and_reuse",
        "native_materialization_style_pipeline_transform_effect_production_pixels_and_reuse",
        "native_materialization_translation_effect_pixels_and_reuse",
        "native_materialized_direct_scroll_transform_matches_the_pre_cutover_pixels_and_reuses_one_pair",
        "native_multi_target_failure_at_every_execution_step_recovers_the_whole_forest",
        "native_offscreen_legacy_and_artifact_pixels_match",
        "native_planning_corpus_absolute_coordinates_and_reuse",
        "native_portable_renderer_acceptance",
        "native_production_artifact_effect_matches_legacy_group_and_reuses_real_pool",
        "native_production_artifact_nested_scroll_inline_ifc_text_matches_legacy_within_one_lsb_and_reuses_real_pool",
        "native_production_artifact_nested_scroll_multi_leaf_matches_legacy_within_one_lsb_and_reuses_real_pool",
        "native_production_artifact_scroll_offset_only_reuses_real_pool_and_matches_legacy_immediate",
        "native_production_artifact_transform_matches_legacy_and_reuses_real_pool",
        "native_ready_owner_scope_image_artifact_dpr1",
        "native_ready_owner_scope_image_artifact_dpr2",
        "native_ready_owner_scope_image_legacy_dpr1",
        "native_ready_owner_scope_image_legacy_dpr2",
        "native_ready_owner_scope_svg_artifact_dpr1",
        "native_ready_owner_scope_svg_artifact_dpr2",
        "native_ready_owner_scope_svg_legacy_dpr1",
        "native_ready_owner_scope_svg_legacy_dpr2",
        "native_single_viewport_ancestor_state_nonempty_slots_image_artifact_dpr1",
        "native_single_viewport_ancestor_state_nonempty_slots_image_artifact_dpr2",
        "native_single_viewport_ancestor_state_nonempty_slots_image_legacy_dpr1",
        "native_single_viewport_ancestor_state_nonempty_slots_image_legacy_dpr2",
        "native_single_viewport_ancestor_state_nonempty_slots_svg_artifact_dpr1",
        "native_single_viewport_ancestor_state_nonempty_slots_svg_artifact_dpr2",
        "native_single_viewport_ancestor_state_nonempty_slots_svg_legacy_dpr1",
        "native_single_viewport_ancestor_state_nonempty_slots_svg_legacy_dpr2",
        "native_single_viewport_artifact_layout_paint_and_pool_reuse",
        "native_single_viewport_completion_after_freeze_waits_until_next_frame",
        "native_single_viewport_content_opacity_and_dpr_invalidation",
        "native_single_viewport_execute_failure_legacy_recovery_and_explicit_retry",
        "native_single_viewport_group_opacity_overlap_and_sibling_scope",
        "native_single_viewport_legacy_content_opacity_and_dpr_pixels",
        "native_single_viewport_legacy_layout_and_paint",
        "native_single_viewport_legacy_nonempty_resource_slots",
        "native_single_viewport_legacy_real_sampled_pressure_pixels",
        "native_single_viewport_legacy_resource_completion_and_generation_pixels",
        "native_single_viewport_legacy_resource_wrapper_effect_pixels",
        "native_single_viewport_missing_backing_rerasterizes_then_reuses",
        "native_single_viewport_mode_switch_discards_stale_retained_content",
        "native_single_viewport_nonempty_resource_slots",
        "native_single_viewport_planning_corpus_artifact_selection_and_reuse",
        "native_single_viewport_planning_corpus_legacy_pixels",
        "native_scroll_culling_reentry_and_deferred_pixels",
        "native_nested_deferred_zero_size_reentry_preserves_late_order",
        "native_long_content_windows_scroll_across_color_boundary",
        "native_asymmetric_child_mask_keeps_large_corner_geometry",
        "native_gpu_source_updates_preserve_independent_native_raster",
        "native_gpu_source_residency_and_shared_consumers",
        "native_gpu_sources_update_independently_and_recover_from_execution_failure",
        "native_gpu_source_visibility_resize_and_opacity_use_current_pixels",
        "native_particle_canvas_changes_pixels_while_native_raster_reuses",
        "native_single_viewport_real_sampled_pressure_preserves_raster_and_recovers_source",
        "native_single_viewport_resource_completion_and_generation_invalidation",
        "native_single_viewport_resource_wrapper_effect_and_reuse",
        "native_single_viewport_sampled_idle_eviction_and_active_legacy_protection",
        "native_single_viewport_svg_raster_generation_and_freeze",
        "native_single_viewport_resource_sources_stay_retained_through_preparation",
        "native_text_preparation_loss_recovers_without_changing_content",
        "native_text_area_ime_event_lifecycle_stays_retained",
        "native_single_viewport_text_area_caret_selection_ime_artifact",
        "native_single_viewport_text_area_caret_selection_ime_legacy",
        "native_zero_surface_v2_matches_legacy_pixels",
    ]);
    assert_eq!(actual, expected);
}

/// C0b replaces only the named-anchor rejection with canonical success. The
/// other three C0a rejection names remain frozen; removing or renaming any of
/// them changes this name set and exposes a wider spatial-baseline shift.
/// This is a spatial proof only: it does not satisfy C1 classification,
/// reconstruction, planner-order, or ArtifactCursor gates, nor later
/// layerization, emission, reuse, or pixel-parity coverage.
#[test]
fn stage_c_named_anchor_success_baseline_is_isolated() {
    let tests = declared_test_function_names(include_str!(
        "../../compositor/property_tree/spatial_projection_tests.rs"
    ));
    let rejection_contracts = expected_names([
        "interleaved_projection_rejects_missing_layout_position",
        "interleaved_projection_rejects_missing_scroll",
        "interleaved_projection_rejects_cyclic_visual_offset",
        "named_anchor_projection_uses_canonical_reference_edge",
    ]);
    assert!(rejection_contracts.is_subset(&tests));
}
