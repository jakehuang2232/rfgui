use super::*;

#[test]
fn stale_frame_owner_rejects_before_graph_or_pool_mutation() {
    let mut viewport = Viewport::new();
    let stale = viewport
        .begin_retained_surface_frame_stage()
        .expect("frame owner");
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(stale), true));
    let mut graph = FrameGraph::new();
    let before = graph.build_state_snapshot_for_test();

    let Err(error) = emit_prepared_artifact_surface_frame_for_forced_test(
        &mut viewport,
        stale,
        prepared_child_mask_surface_frame(),
        &mut graph,
        execution_context(),
    ) else {
        panic!("stale owner must reject")
    };

    assert_eq!(
        error,
        ArtifactSurfaceExecutionError::InactiveFrameStageOwner
    );
    assert_eq!(execution_error_name(error), "inactive-frame-stage-owner");
    assert_eq!(graph.build_state_snapshot_for_test(), before);
    assert_eq!(
        viewport.pending_artifact_surface_resident_keys_for_test(),
        None
    );
}

#[test]
fn persistent_key_collision_rejects_the_whole_frame_before_first_append() {
    let prepared = prepared_child_mask_surface_frame();
    let resident = prepared
        .residents()
        .ordered_entries()
        .first()
        .expect("one surface resident");
    let key = resident.stamp().identity.color_key;
    let desc = resident.stamp().target.color.clone();
    let mut graph = FrameGraph::new();
    let _ = graph.declare_persistent_texture_internal::<()>(desc, key);
    let before = graph.build_state_snapshot_for_test();
    let mut viewport = Viewport::new();
    let owner = viewport
        .begin_retained_surface_frame_stage()
        .expect("frame owner");

    let Err(error) = emit_prepared_artifact_surface_frame_for_forced_test(
        &mut viewport,
        owner,
        prepared,
        &mut graph,
        execution_context(),
    ) else {
        panic!("persistent key collision must reject")
    };

    assert_eq!(
        error,
        ArtifactSurfaceExecutionError::PersistentKeyAlreadyDeclared(key)
    );
    assert_eq!(
        execution_error_name(error),
        "persistent-key-already-declared"
    );
    assert_eq!(graph.build_state_snapshot_for_test(), before);
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
        cold_owner,
        cold_frame,
        &mut cold_graph,
        execution_context(),
    )
    .expect("cold artifact emission");
    assert_eq!(cold_graph.declared_persistent_texture_keys().count(), 6);
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
        warm_owner,
        warm_frame,
        &mut warm_graph,
        execution_context(),
    )
    .expect("warm artifact emission");
    assert_eq!(warm_graph.declared_persistent_texture_keys().count(), 6);
    assert!(
        warm_actions
            .iter()
            .all(|action| *action == RetainedSurfaceCompileAction::Reuse)
    );
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(warm_owner), true));
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
        baseline_owner,
        baseline,
        &mut baseline_graph,
        execution_context(),
    )
    .expect("baseline emission");
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
        owner,
        warm,
        &mut graph,
        execution_context(),
    )
    .expect("mixed-action emission");

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
