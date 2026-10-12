use super::*;

#[test]
fn stale_frame_owner_draws_every_surface_without_the_cache() {
    let mut viewport = Viewport::new();
    let stale = viewport
        .begin_retained_surface_frame_stage()
        .expect("frame owner");
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(stale), true));
    let mut graph = FrameGraph::new();

    let (_, actions) = emit_prepared_artifact_surface_frame_for_forced_test(
        &mut viewport,
        Some(stale),
        prepared_child_mask_surface_frame(),
        &mut graph,
        execution_context(),
    );

    assert!(!actions.is_empty());
    assert!(
        actions
            .iter()
            .all(|action| *action == RetainedSurfaceCompileAction::Transient)
    );
    assert_eq!(graph.declared_persistent_texture_keys().count(), 0);
    assert!(!graph.pass_descriptors().is_empty());
    assert_eq!(
        viewport.pending_artifact_surface_resident_keys_for_test(),
        None
    );
}

#[test]
fn co_located_roles_keep_three_keys_and_cold_warm_actions_through_staging() {
    let cold_frame = prepared_co_located_surface_frame();
    let sealed_keys = cold_frame.residents().resident_keys_for_test();
    assert_eq!(sealed_keys.len(), 3);
    let mut viewport = Viewport::new();
    let cold_owner = viewport
        .begin_retained_surface_frame_stage()
        .expect("cold owner");
    let mut cold_graph = FrameGraph::new();
    let (_, cold_actions) = emit_prepared_artifact_surface_frame_for_forced_test(
        &mut viewport,
        Some(cold_owner),
        cold_frame,
        &mut cold_graph,
        execution_context(),
    );
    assert_eq!(cold_graph.declared_persistent_texture_keys().count(), 3);
    assert!(
        cold_actions
            .iter()
            .all(|action| *action == RetainedSurfaceCompileAction::Reraster)
    );
    assert_eq!(
        viewport.pending_artifact_surface_resident_keys_for_test(),
        Some(sealed_keys.clone())
    );
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(cold_owner), true));

    let warm_frame = prepared_co_located_surface_frame();
    let warm_owner = viewport
        .begin_retained_surface_frame_stage()
        .expect("warm owner");
    let mut warm_graph = FrameGraph::new();
    let (_, warm_actions) = emit_prepared_artifact_surface_frame_for_forced_test(
        &mut viewport,
        Some(warm_owner),
        warm_frame,
        &mut warm_graph,
        execution_context(),
    );
    assert_eq!(warm_graph.declared_persistent_texture_keys().count(), 3);
    assert!(
        warm_actions
            .iter()
            .all(|action| *action == RetainedSurfaceCompileAction::Reuse)
    );
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(warm_owner), true));
}

#[test]
fn empty_artifact_resident_set_replaces_committed_surface_residents() {
    let populated = prepared_co_located_surface_frame();
    let expected_released_colors = populated
        .residents()
        .ordered_entries()
        .iter()
        .map(|entry| entry.stamp().identity.color_key)
        .collect::<Vec<_>>();
    assert_eq!(expected_released_colors.len(), 3);
    for (index, color_key) in expected_released_colors.iter().enumerate() {
        assert!(
            expected_released_colors[..index]
                .iter()
                .all(|previous| previous != color_key),
            "the populated fixture must own three distinct color targets"
        );
    }

    let mut viewport = Viewport::new();
    let populated_owner = viewport
        .begin_retained_surface_frame_stage()
        .expect("populated owner");
    let mut populated_graph = FrameGraph::new();
    emit_prepared_artifact_surface_frame_for_forced_test(
        &mut viewport,
        Some(populated_owner),
        populated,
        &mut populated_graph,
        execution_context(),
    );
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(populated_owner), true));
    assert_eq!(
        viewport
            .committed_retained_surface_resident_keys_for_test()
            .len(),
        3
    );
    assert_eq!(
        viewport.pending_artifact_surface_resident_keys_for_test(),
        None,
        "the first finish must clear pending before the empty replacement stages"
    );
    assert!(viewport.retained_surface_release_log_for_test().is_empty());

    let empty_owner = viewport
        .begin_retained_surface_frame_stage()
        .expect("empty replacement owner");
    let mut empty_graph = FrameGraph::new();
    let (_, actions) = emit_prepared_artifact_surface_frame_for_forced_test(
        &mut viewport,
        Some(empty_owner),
        prepared_zero_surface_frame(),
        &mut empty_graph,
        execution_context(),
    );

    assert!(actions.is_empty());
    assert_eq!(
        empty_graph
            .declared_persistent_texture_keys()
            .filter(|key| {
                !matches!(
                    key,
                    crate::view::frame_graph::PersistentTextureKey::ShadowTemplate(_)
                )
            })
            .count(),
        0,
        "only shared shadow templates may be declared without retained surfaces"
    );
    assert_eq!(
        viewport.pending_artifact_surface_resident_keys_for_test(),
        Some(Vec::new()),
        "an empty replacement remains distinct from no pending transaction"
    );
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(empty_owner), true));
    assert!(
        viewport
            .committed_retained_surface_resident_keys_for_test()
            .is_empty()
    );
    assert_eq!(
        viewport.pending_artifact_surface_resident_keys_for_test(),
        None
    );
    assert_eq!(viewport.retained_surface_release_log_for_test().len(), 3);
    for color_key in expected_released_colors {
        assert_eq!(
            viewport
                .retained_surface_release_log_for_test()
                .iter()
                .filter(|released| **released == color_key)
                .count(),
            1,
            "each displaced artifact surface pair must be released exactly once"
        );
    }
}

// This test proves the residency-loss half: a reused parent still visits and
// materializes its evicted child. Content changes enter parent equality by
// construction through `ArtifactSurfaceNestedRasterDependency::child_stamp`.
#[test]
fn reused_parent_still_materializes_one_evicted_reraster_child() {
    let baseline = prepared_depth_four_surface_frame();
    let mut viewport = Viewport::new();
    let baseline_owner = viewport
        .begin_retained_surface_frame_stage()
        .expect("baseline owner");
    let mut baseline_graph = FrameGraph::new();
    emit_prepared_artifact_surface_frame_for_forced_test(
        &mut viewport,
        Some(baseline_owner),
        baseline,
        &mut baseline_graph,
        execution_context(),
    );
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(baseline_owner), true));

    let warm = prepared_depth_four_surface_frame();
    let evicted_index = warm.residents().len() - 1;
    let evicted_color = warm.residents().ordered_entries()[evicted_index]
        .stamp()
        .identity
        .color_key;
    viewport.forget_retained_surface_pair_witness_for_test(evicted_color);
    let owner = viewport
        .begin_retained_surface_frame_stage()
        .expect("mixed-action owner");
    let mut graph = FrameGraph::new();
    let (_, actions) = emit_prepared_artifact_surface_frame_for_forced_test(
        &mut viewport,
        Some(owner),
        warm,
        &mut graph,
        execution_context(),
    );

    assert_eq!(
        actions[evicted_index],
        RetainedSurfaceCompileAction::Reraster
    );
    assert!(
        actions[..evicted_index]
            .iter()
            .all(|action| *action == RetainedSurfaceCompileAction::Reuse)
    );
    assert_eq!(graph.test_graphics_passes::<ClearPass>().len(), 1);
    assert_eq!(graph.test_graphics_passes::<CompositeLayerPass>().len(), 1);
}

#[test]
fn unchanged_span_reuses_its_seal() {
    use crate::view::paint::compiler::ArtifactSurfaceRasterProgramStepStamp;
    let (plan, _) = prepared_child_mask_surface_frame().into_parts();
    let first = seal_prepared_artifact_surface_frame(plan.clone()).unwrap();
    let warm = seal_prepared_artifact_surface_frame(plan.clone()).unwrap();
    assert_eq!(first.residents(), warm.residents());
    let mut shared = 0;
    for (a, b) in first
        .residents()
        .ordered_entries()
        .iter()
        .zip(warm.residents().ordered_entries())
    {
        for (a, b) in a
            .stamp
            .artifact_surface_program
            .as_ref()
            .unwrap()
            .steps
            .iter()
            .zip(&b.stamp.artifact_surface_program.as_ref().unwrap().steps)
        {
            if let (
                ArtifactSurfaceRasterProgramStepStamp::ArtifactSpan(a),
                ArtifactSurfaceRasterProgramStepStamp::ArtifactSpan(b),
            ) = (a, b)
            {
                assert!(a.shares_seal(b));
                shared += 1;
            }
        }
    }
    assert!(shared > 0);
}
