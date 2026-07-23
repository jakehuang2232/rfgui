use super::*;

#[test]
fn retained_auto_direct_scroll_transform_production_preflight_and_rejection_dispatch() {
    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let (arena, roots, _, _) = prepared_exact_scroll_scene();
    let child = arena.children_of(roots[0])[0];
    crate::view::test_support::get_element_mut::<Element>(&arena, child)
        .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
            3.0, 0.0, 0.0,
        ))));
    arena.refresh_subtree_dirty_cache(roots[0]);
    let (properties, generations) = synced_paint_state(&arena, &roots);
    let select = || {
        select_retained_auto_authority(&arena, &roots, &properties, &generations, &ctx, true)
    };

    let AutoAuthorityDecision::DirectScrollTransformScene { scene, trace } = select() else {
        panic!("exact S->T must reach the production direct preflight")
    };
    assert_eq!(
        trace.rejections.len(),
        6,
        "DPR-capable direct S->T authority is selected after the six earlier candidates"
    );
    let mut viewport = Viewport::new();
    let owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut graph = FrameGraph::new();
    let (selection, outcome) = preflight_direct_scroll_transform_selection(
        &mut viewport,
        &mut graph,
        UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0),
        [0.125, 0.25, 0.5, 1.0],
        Some(owner),
        RetainedTransformCanarySelection::DirectScrollTransformScenePlanned(scene),
    );
    assert!(matches!(
        selection,
        RetainedTransformCanarySelection::DirectScrollTransformScenePrepared
    ));
    let outcome = outcome.expect("production preflight pre-emits one sealed S->T outcome");
    let (_, build_trace) = outcome.into_parts();
    assert_eq!(
        (
            build_trace.generic_surface_count,
            build_trace.scroll_group_count,
            build_trace.reraster_count,
            build_trace.reuse_count,
        ),
        (1, 0, 1, 0)
    );
    assert_eq!(
        graph
            .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
            .len(),
        2,
        "pre-emitted S->T owns the root and cold T clears; common clear must be skipped"
    );
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));

    let AutoAuthorityDecision::DirectScrollTransformScene { scene, .. } = select() else {
        unreachable!()
    };
    assert!(viewport.stage_retained_surface_clear());
    let missing_owner = viewport.begin_retained_surface_frame_stage();
    assert!(missing_owner.is_none());
    let mut rejected_graph = FrameGraph::new();
    let graph_before = rejected_graph.build_state_snapshot_for_test();
    let (selection, outcome) = preflight_direct_scroll_transform_selection(
        &mut viewport,
        &mut rejected_graph,
        UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0),
        [0.0; 4],
        missing_owner,
        RetainedTransformCanarySelection::DirectScrollTransformScenePlanned(scene),
    );
    assert!(outcome.is_none());
    let RetainedTransformCanarySelection::DirectScrollTransformScenePrepareRejected(error) =
        &selection
    else {
        panic!("occupied pending slot must become a prepare-stage fallback")
    };
    assert_eq!(
        *error,
        crate::view::paint::RetainedPropertyScrollScenePrepareError::StageUnavailable
    );
    assert_eq!(rejected_graph.build_state_snapshot_for_test(), graph_before);
    assert_eq!(
        direct_scroll_transform_prepare_rejection_fallback_stage(),
        PaintAuthorityFallbackStage::Prepare
    );
    let (legacy, label) = direct_scroll_transform_prepare_rejection_dispatch(error);
    assert!(legacy);
    assert!(label.contains("direct-scroll-transform-prepare-rejected=StageUnavailable"));
    assert!(viewport.compositor.pending_retained_surfaces.is_some());
    viewport.finish_retained_surface_transaction(true);
}

#[test]
fn retained_auto_transform_effect_scroll_production_preflight_and_rejection_dispatch() {
    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let (arena, roots, properties, generations) = prepared_transform_effect_scroll_scene();
    let select = || {
        select_retained_auto_authority(&arena, &roots, &properties, &generations, &ctx, true)
    };

    let AutoAuthorityDecision::TransformEffectScrollScene { scene, trace } = select() else {
        panic!("exact T->E->S must reach the production joint preflight")
    };
    let mut viewport = Viewport::new();
    let owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut graph = FrameGraph::new();
    let (selection, outcome) = preflight_transform_effect_scroll_selection(
        &mut viewport,
        &mut graph,
        UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0),
        [0.0; 4],
        Some(owner),
        RetainedTransformCanarySelection::TransformEffectScrollScenePlanned(scene),
    );
    assert!(matches!(
        selection,
        RetainedTransformCanarySelection::TransformEffectScrollScenePrepared
    ));
    let outcome = outcome.expect("production preflight emits one sealed joint outcome");
    let (_, build_trace) = outcome.into_parts();
    assert_eq!(
        (
            build_trace.generic_surface_count,
            build_trace.effect_surface_count,
            build_trace.scroll_group_count,
        ),
        (2, 1, 1)
    );
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));

    let AutoAuthorityDecision::TransformEffectScrollScene { scene, .. } = select() else {
        unreachable!()
    };
    assert!(viewport.stage_retained_surface_clear());
    let missing_owner = viewport.begin_retained_surface_frame_stage();
    assert!(missing_owner.is_none());
    let mut rejected_graph = FrameGraph::new();
    let graph_before = rejected_graph.build_state_snapshot_for_test();
    let (selection, outcome) = preflight_transform_effect_scroll_selection(
        &mut viewport,
        &mut rejected_graph,
        UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0),
        [0.0; 4],
        missing_owner,
        RetainedTransformCanarySelection::TransformEffectScrollScenePlanned(scene),
    );
    assert!(outcome.is_none());
    let RetainedTransformCanarySelection::TransformEffectScrollScenePrepareRejected(error) =
        &selection
    else {
        panic!("occupied production stage must become the typed prepare rejection")
    };
    assert_eq!(
        error,
        &crate::view::paint::RetainedPropertyScrollScenePrepareError::StageUnavailable
    );
    assert_eq!(rejected_graph.build_state_snapshot_for_test(), graph_before);
    let (whole_frame_legacy, detail) =
        transform_effect_scroll_prepare_rejection_dispatch(error);
    assert!(whole_frame_legacy);
    assert!(detail.contains("authority=legacy"));
    let fallback_stage = transform_effect_scroll_prepare_rejection_fallback_stage();
    assert_eq!(fallback_stage, PaintAuthorityFallbackStage::Prepare);
    let mut telemetry = PaintAuthorityTelemetry::from_selection(
        ViewportPaintRendererMode::RetainedAuto,
        &selection,
        Some((AutoAuthorityKind::PropertyScene, trace)),
    );
    telemetry.note_legacy_fallback(fallback_stage);
    let snapshot = telemetry.snapshot();
    assert_eq!(snapshot.authority_label, "retained-auto:legacy");
    assert_eq!(
        snapshot.legacy_fallback_stage,
        Some(PaintAuthorityFallbackStage::Prepare)
    );
    viewport.finish_retained_surface_transaction(true);
}

#[test]
fn retained_auto_authority_accepts_deferred_viewport_root() {
    fn assert_compiles(candidate: RecordedArtifactCandidate) {
        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let target = ctx.allocate_target(&mut graph);
        ctx.set_current_target(target);
        assert!(matches!(
            try_compile_recorded_artifact_frame(&mut graph, candidate, &ctx, None),
            PropertyNeutralArtifactAttempt::Compiled { .. }
        ));
    }

    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let (arena, roots) = prepared_safe_leaf();
    let AutoAuthorityDecision::Artifact { candidate, trace } =
        auto_decision(&arena, &roots, &ctx)
    else {
        panic!("property-neutral Element must select Artifact")
    };
    assert!(candidate.eligibility.eligible);
    assert!(trace.rejections.is_empty());
    assert_compiles(candidate);

    let (arena, roots, root) = prepared_deferred_viewport_leaf();
    let (properties, generations) = synced_paint_state(&arena, &roots);
    let clip_id = crate::view::compositor::property_tree::ClipNodeId {
        owner: root,
        role: crate::view::compositor::property_tree::ClipNodeRole::SelfClip,
    };
    let clip_chain = properties
        .clip_snapshot_for(Some(clip_id))
        .expect("deferred viewport root owns an exact clip chain");
    let [clip] = clip_chain.as_slice() else {
        panic!("deferred viewport root must own one clip snapshot")
    };
    assert_eq!(clip.owner, root);
    assert_eq!(clip.parent, None);
    assert_eq!(clip.logical_scissor, [0, 0, 320, 240]);
    assert_eq!(
        clip.behavior,
        crate::view::compositor::property_tree::ClipBehavior::Replace
    );

    let record = |mode| {
        crate::view::paint::record_coverage_manifest(
            &arena,
            &roots,
            false,
            true,
            mode,
            &properties,
            &generations,
        )
    };
    let metadata = record(crate::view::paint::CoverageRecordingMode::MetadataOnly);
    let full = record(crate::view::paint::CoverageRecordingMode::FullArtifact);
    assert!(metadata.validation_errors.is_empty());
    assert!(full.validation_errors.is_empty());
    assert!(crate::view::paint::canonical_manifest_matches_for_test(
        &metadata, &full
    ));
    assert!(metadata.items.iter().all(|item| !matches!(
        item,
        crate::view::paint::PaintCoverageItem::LegacyBoundary { .. }
    )));

    let AutoAuthorityDecision::Artifact { candidate, trace } =
        select_retained_auto_authority(&arena, &roots, &properties, &generations, &ctx, true)
    else {
        panic!("exact deferred viewport root must select Artifact")
    };
    assert!(candidate.eligibility.eligible);
    assert!(trace.rejections.is_empty());
    assert_compiles(candidate);

    for tamper in ["behavior", "generation"] {
        let (arena, roots, root) = prepared_deferred_viewport_leaf();
        let (mut properties, generations) = synced_paint_state(&arena, &roots);
        let clip_id = crate::view::compositor::property_tree::ClipNodeId {
            owner: root,
            role: crate::view::compositor::property_tree::ClipNodeRole::SelfClip,
        };
        let clip = properties.clips.get_mut(&clip_id).expect("self clip");
        match tamper {
            "behavior" => {
                clip.behavior = crate::view::compositor::property_tree::ClipBehavior::Intersect
            }
            "generation" => clip.generation = 0,
            _ => unreachable!(),
        }
        crate::view::paint::take_full_artifact_record_count();
        let AutoAuthorityDecision::Legacy { trace } = select_retained_auto_authority(
            &arena,
            &roots,
            &properties,
            &generations,
            &ctx,
            true,
        ) else {
            panic!("tampered deferred viewport witness must fail closed: {tamper}")
        };
        assert!(
            trace.rejections.iter().any(|rejection| matches!(
                rejection,
                AutoAuthorityRejection::Artifact { eligibility }
                    if eligibility.reasons.contains(
                        &crate::view::paint::FrameArtifactFallbackReason::LegacyBoundary(
                            crate::view::paint::LegacyPaintReason::Deferred,
                        )
                    )
            )),
            "tampered {tamper} rejection labels: {:?}",
            trace
                .rejections
                .iter()
                .map(AutoAuthorityRejection::debug_label)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            crate::view::paint::take_full_artifact_record_count(),
            0,
            "malformed {tamper} must fail in graph-inert metadata preflight"
        );
    }
}
