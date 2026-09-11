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

/// Names of the 23 ignored native GPU tests present at the start of Stage C,
/// plus the named V2 hardware gates, including the materialization,
/// style-pipeline and single-Viewport suites. Their files also contain CPU
/// tests, so only `native_` test names participate in this inventory.
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
        include_str!("gpu_equivalence_tests/native_transform_surface_tests.rs"),
        include_str!("gpu_equivalence_tests/native_artifact_surface_tests.rs"),
        include_str!("gpu_equivalence_tests/native_artifact_scroll_content_tests.rs"),
        include_str!("gpu_equivalence_tests/native_scroll_boundary_tests.rs"),
        include_str!("gpu_equivalence_tests/native_nested_scroll_tests.rs"),
        include_str!("gpu_equivalence_tests/native_nested_scroll_segment_tests.rs"),
        include_str!("gpu_equivalence_tests/native_scroll_forest_tests.rs"),
        include_str!("gpu_equivalence_tests/native_scroll_scene_pixel_tests.rs"),
        include_str!("gpu_equivalence_tests/native_artifact_surface_materialization_tests.rs"),
        include_str!("gpu_equivalence_tests/native_artifact_surface_materialization_tests/text_area_recording_tests.rs"),
        include_str!("gpu_equivalence_tests/native_artifact_surface_materialization_tests/recording_overlay_tests.rs"),
        include_str!("gpu_equivalence_tests/native_artifact_surface_materialization_tests/style_pipeline_tests/single_viewport_tests/execution_failure_tests.rs"),
        include_str!("gpu_equivalence_tests/native_artifact_surface_materialization_tests/style_pipeline_tests/single_viewport_tests/invalidation_tests.rs"),
        include_str!("gpu_equivalence_tests/native_artifact_surface_materialization_tests/style_pipeline_tests/single_viewport_tests/residency_tests.rs"),
        include_str!("gpu_equivalence_tests/native_artifact_surface_materialization_tests/style_pipeline_tests/single_viewport_tests/resource_lifecycle_tests/pressure_tests.rs"),
        include_str!("gpu_equivalence_tests/native_artifact_surface_materialization_tests/style_pipeline_tests/single_viewport_tests/resource_lifecycle_tests/slot_content_tests.rs"),
        include_str!("gpu_equivalence_tests/native_artifact_surface_materialization_tests/style_pipeline_tests/single_viewport_tests/resource_lifecycle_tests/svg_resource_tests.rs"),
        include_str!("gpu_equivalence_tests/native_artifact_surface_materialization_tests/style_pipeline_tests/single_viewport_tests/resource_lifecycle_tests/wrapper_effect_tests/ancestor_slot_tests.rs"),
        include_str!("gpu_equivalence_tests/native_artifact_surface_materialization_tests/style_pipeline_tests/single_viewport_tests/resource_lifecycle_tests/wrapper_effect_tests/ready_owner_scope_tests.rs"),
        include_str!("gpu_equivalence_tests/native_artifact_surface_materialization_tests/style_pipeline_tests/single_viewport_tests/resource_lifecycle_tests/wrapper_effect_tests.rs"),
        include_str!("gpu_equivalence_tests/native_artifact_surface_materialization_tests/style_pipeline_tests/single_viewport_tests/resource_lifecycle_tests.rs"),
        include_str!("gpu_equivalence_tests/native_artifact_surface_materialization_tests/style_pipeline_tests/single_viewport_tests.rs"),
        include_str!("gpu_equivalence_tests/native_artifact_surface_materialization_tests/style_pipeline_tests.rs"),
    ]
    .into_iter()
    .flat_map(declared_test_function_names)
    .filter(|name| name.starts_with("native_"))
    .collect::<BTreeSet<_>>();
    let expected = expected_names([
        "native_generic_text_area_frozen_ime_caret_and_reuse",
        "native_generic_and_legacy_frozen_scrollbar_absolute_alpha",
        "native_legacy_text_area_frozen_ime_caret_pixels",
        "native_offscreen_legacy_and_artifact_pixels_match",
        "native_zero_surface_v2_matches_legacy_pixels",
        "native_forced_transform_surface_matches_legacy_pixels",
        "native_forced_nested_transform_surfaces_match_legacy_pixels",
        "native_forced_nested_r_u_and_u_u_frames_match_legacy_pixels",
        "native_production_transform_surface_reuses_real_pool_on_second_frame",
        "native_production_retained_surface_tree_reuses_real_pool_on_second_frame",
        "native_fractional_host_offset_property_scene_bounds_translate_exactly",
        "native_nonzero_host_transform_legacy_artifact_and_independent_translation_agree",
        "native_production_direct_scroll_transform_matches_legacy_and_reuses_real_pair",
        "native_production_nested_scroll_image_svg_text_frozen_payloads_match_legacy_and_reuse_real_r1",
        "native_production_nested_scroll_matches_legacy_within_one_lsb_and_reuses_real_r1",
        "native_direct_nested_scroll_segment_image_svg_dpr1_dpr2_one_lsb_gate",
        "native_direct_nested_scroll_segment_rect_matches_legacy_within_one_lsb_at_dpr1_dpr2",
        "native_direct_nested_scroll_segment_rgba8_physical_integer_logical_fractional_text_dpr2_exact_gate",
        "native_direct_nested_scroll_segment_rgba8_fractional_phase_text_dpr2_exact_gate",
        "native_direct_nested_scroll_segment_rgba8_zero_offset_text_dpr2_one_lsb_gate",
        "native_direct_nested_scroll_segment_rgba8_logical_integer_text_dpr2_one_lsb_gate",
        "native_nested_scroll_segment_phase_sensitive_text_direct_primitive_dpr2",
        "native_direct_nested_scroll_segment_real_pool_reuse_and_leaf_paint_r",
        "native_focused_atomic_projection_scroll_forest_matches_legacy_and_reuses_real_pair",
        "native_production_multi_root_scroll_forest_matches_legacy_and_reuses_real_pool",
        "native_scroll_scene_single_backing_pixels_match_and_reuse",
        "native_scroll_scene_tiled_cross_tile_pixels_match_and_reuse",
        "native_production_artifact_transform_matches_legacy_and_reuses_real_pool",
        "native_production_artifact_effect_matches_property_scene_and_reuses_real_pool",
        "native_production_artifact_nested_scroll_multi_leaf_matches_legacy_within_one_lsb_and_reuses_real_pool",
        "native_production_artifact_nested_scroll_inline_ifc_text_matches_legacy_within_one_lsb_and_reuses_real_pool",
        "native_production_artifact_scroll_offset_only_reuses_real_pool_and_matches_legacy_immediate",
        "native_materialized_direct_scroll_transform_matches_the_pre_cutover_pixels_and_reuses_one_pair",
        "native_materialization_absolute_pixels_and_reuse_at_both_dprs",
        "native_materialization_translation_effect_pixels_and_reuse",
        "native_single_viewport_execute_failure_legacy_recovery_and_explicit_retry",
        "native_single_viewport_content_opacity_and_dpr_invalidation",
        "native_single_viewport_legacy_content_opacity_and_dpr_pixels",
        "native_single_viewport_mode_switch_discards_stale_retained_content",
        "native_single_viewport_missing_backing_rerasterizes_then_reuses",
        "native_single_viewport_real_sampled_pressure_preserves_raster_and_recovers_source",
        "native_single_viewport_legacy_real_sampled_pressure_pixels",
        "native_single_viewport_sampled_idle_eviction_and_active_legacy_protection",
        "native_single_viewport_nonempty_resource_slots",
        "native_single_viewport_legacy_nonempty_resource_slots",
        "native_single_viewport_svg_raster_generation_and_freeze",
        "native_single_viewport_ancestor_state_nonempty_slots_image_artifact_dpr1",
        "native_single_viewport_ancestor_state_nonempty_slots_image_artifact_dpr2",
        "native_single_viewport_ancestor_state_nonempty_slots_image_legacy_dpr1",
        "native_single_viewport_ancestor_state_nonempty_slots_image_legacy_dpr2",
        "native_single_viewport_ancestor_state_nonempty_slots_svg_artifact_dpr1",
        "native_single_viewport_ancestor_state_nonempty_slots_svg_artifact_dpr2",
        "native_single_viewport_ancestor_state_nonempty_slots_svg_legacy_dpr1",
        "native_single_viewport_ancestor_state_nonempty_slots_svg_legacy_dpr2",
        "native_single_viewport_resource_wrapper_effect_and_reuse",
        "native_single_viewport_legacy_resource_wrapper_effect_pixels",
        "native_single_viewport_resource_completion_and_generation_invalidation",
        "native_single_viewport_legacy_resource_completion_and_generation_pixels",
        "native_single_viewport_completion_after_freeze_waits_until_next_frame",
        "native_single_viewport_artifact_layout_paint_and_pool_reuse",
        "native_single_viewport_legacy_layout_and_paint",
        "native_materialization_style_pipeline_scroll_executor_pixels_and_reuse",
        "native_materialization_style_pipeline_transform_effect_production_pixels_and_reuse",
        "native_materialization_style_pipeline_legacy_pixels",
        "native_single_viewport_group_opacity_overlap_and_sibling_scope",
        "native_ready_owner_scope_image_artifact_dpr1",
        "native_ready_owner_scope_image_artifact_dpr2",
        "native_ready_owner_scope_image_legacy_dpr1",
        "native_ready_owner_scope_image_legacy_dpr2",
        "native_ready_owner_scope_svg_artifact_dpr1",
        "native_ready_owner_scope_svg_artifact_dpr2",
        "native_ready_owner_scope_svg_legacy_dpr1",
        "native_ready_owner_scope_svg_legacy_dpr2",
    ]);
    assert_eq!(actual, expected);
}

/// The six generic no-scroll forest executor suites are the reusable C3
/// asset. Their arena-built planner input is disposable, but these exact 30
/// depth, branch, multi-root, cursor, reuse, and atomicity behaviors must be
/// ported to the artifact-built Surface DAG before the old path is deleted.
#[test]
fn stage_c_property_boundary_forest_executor_gate_names_are_a_closed_set() {
    let actual = [
        include_str!("property_boundary_forest_branching_executor_tests.rs"),
        include_str!("property_boundary_forest_depth_three_executor_tests.rs"),
        include_str!("property_boundary_forest_executor_tests.rs"),
        include_str!("property_boundary_forest_linear_executor_tests.rs"),
        include_str!("property_boundary_forest_multi_root_executor_tests.rs"),
        include_str!("property_boundary_forest_plain_root_executor_tests.rs"),
    ]
    .into_iter()
    .flat_map(declared_test_function_names)
    .collect::<BTreeSet<_>>();
    let expected = expected_names([
        "transform_and_effect_branch_forests_dpr1_dpr2_cold_warm_and_cursor_stamps",
        "branch_content_invalidation_rerasterizes_ancestor_and_changed_branch_only",
        "parent_content_change_rerasterizes_parent_while_both_branches_reuse",
        "adding_and_removing_a_branch_updates_one_atomic_full_set",
        "branch_prepare_tamper_rejects_before_graph_pool_or_stage_mutation",
        "transform_effect_transform_direct_and_neutral_dpr1_dpr2_cold_warm",
        "effect_transform_effect_direct_and_neutral_dpr1_dpr2_cold_warm",
        "transform_effect_transform_composite_invalidation_is_ancestor_scoped",
        "effect_transform_effect_composite_invalidation_is_ancestor_scoped",
        "depth_three_plain_sibling_isolation_and_role_aware_opaque_cursor_are_exact",
        "depth_three_tamper_rejects_before_graph_pool_or_stage_mutation",
        "direct_effect_transform_dpr1_cold_then_warm_reuses_both_residents",
        "neutral_effect_transform_dpr2_descriptors_and_warm_reuse_are_exact",
        "alternating_child_composites_into_parent_before_top_level_composite",
        "top_level_effect_opacity_is_composite_only_and_reuses_both_residents",
        "nested_transform_matrix_reuses_child_and_rerasterizes_effect_parent",
        "descriptor_action_and_forest_tamper_reject_before_graph_pool_or_stage_mutation",
        "depth_four_tete_and_depth_five_etete_dpr1_dpr2_cold_warm",
        "middle_effect_invalidation_propagates_only_to_ancestors",
        "depth_five_prepare_tamper_is_atomic",
        "heterogeneous_roots_are_one_cold_warm_joint_transaction_at_dpr1_and_dpr2",
        "root_a_content_mutation_does_not_pollute_root_b_residents",
        "root_reorder_preserves_residents_but_stages_the_new_exact_joint_order",
        "removing_one_root_commits_one_smaller_atomic_full_set",
        "multi_root_prepare_tampers_reject_before_graph_pool_or_stage_mutation",
        "plain_roots_add_no_residents_to_cold_warm_joint_transactions",
        "plain_root_invalidation_keeps_all_property_residents_reusable",
        "property_root_invalidation_does_not_pollute_plain_or_other_property_root",
        "reorder_and_plain_then_property_removal_commit_exact_smaller_full_sets",
        "plain_root_joint_stamp_and_action_tampers_reject_before_mutation",
    ]);
    assert_eq!(actual, expected);
}

/// Stage A freezes one retained scroll-content surface under five content
/// shapes. C3b keeps this exact old-path test unchanged while adding a
/// separate exact per-fixture V2 Surface DAG matrix. The old test is replaced,
/// never loosened to an arbitrary Vec, in the hard-cutover batch.
#[test]
fn stage_c_stage_a_reuse_baseline_has_one_exact_test() {
    let actual = declared_test_function_names(include_str!(
        "../../viewport/render/legacy_root_render_tests/stage_a_reuse_contract_tests.rs"
    ));
    assert_eq!(
        actual,
        expected_names(["stage_a_one_surface_reuse_contract_covers_five_content_shapes"]),
    );
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

/// Cutover survival is intentionally asymmetric.
///
/// `compiler.rs` survives because V2 needs generic artifact compilation and
/// retained raster stamps, so its 435 TextArea grammar tokens must reach zero.
/// `scroll_scene.rs` is the existing retained middle layer and is deleted as
/// a complete file; its 857-token count is observational until file removal,
/// not a request to rename symbols inside doomed code. Both scans are limited
/// to production files. Legitimate TextArea fixtures remain under `*/tests/`
/// in accordance with the repository's test-separation rule.
#[test]
fn stage_c_cutover_file_survival_and_token_baseline_are_explicit() {
    let compiler = include_str!("../compiler.rs");
    let scroll_scene = include_str!("../scroll_scene.rs");
    let token_count =
        |source: &str| source.matches("TextArea").count() + source.matches("text_area").count();
    let compiler_tokens = token_count(compiler);
    let scroll_scene_tokens = token_count(scroll_scene);
    assert!(
        compiler_tokens <= 435,
        "compiler.rs survives cutover and may not regress above 435 tokens: actual={compiler_tokens}",
    );
    assert!(
        scroll_scene_tokens <= 857,
        "scroll_scene.rs is removed whole at cutover and may not regress above 857 tokens: actual={scroll_scene_tokens}",
    );
}
