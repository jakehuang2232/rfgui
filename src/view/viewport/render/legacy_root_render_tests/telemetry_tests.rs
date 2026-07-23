use super::*;

#[test]
fn retained_auto_trace_capture_does_not_change_authority_decision() {
    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);

    let (arena, roots) = prepared_transform_leaf();
    let (properties, generations) = synced_paint_state(&arena, &roots);
    let captured =
        select_retained_auto_authority(&arena, &roots, &properties, &generations, &ctx, true);
    let uncaptured =
        select_retained_auto_authority(&arena, &roots, &properties, &generations, &ctx, false);
    assert_eq!(
        auto_authority_kind(&captured),
        auto_authority_kind(&uncaptured)
    );
    assert_eq!(
        auto_authority_kind(&captured),
        AutoAuthorityKind::PropertyScene
    );
    assert!(auto_authority_trace(&uncaptured).rejections.is_empty());

    let (effect_scroll_arena, effect_scroll_roots, _, _) =
        prepared_transform_scroll_scene(glam::Mat4::IDENTITY);
    crate::view::test_support::get_element_mut::<Element>(
        &effect_scroll_arena,
        effect_scroll_roots[0],
    )
    .set_resolved_transform_for_test(None);
    crate::view::test_support::get_element_mut::<Element>(
        &effect_scroll_arena,
        effect_scroll_roots[0],
    )
    .set_opacity(0.5);
    effect_scroll_arena.refresh_subtree_dirty_cache(effect_scroll_roots[0]);
    let (effect_scroll_properties, effect_scroll_generations) =
        synced_paint_state(&effect_scroll_arena, &effect_scroll_roots);
    let captured = select_retained_auto_authority(
        &effect_scroll_arena,
        &effect_scroll_roots,
        &effect_scroll_properties,
        &effect_scroll_generations,
        &ctx,
        true,
    );
    let uncaptured = select_retained_auto_authority(
        &effect_scroll_arena,
        &effect_scroll_roots,
        &effect_scroll_properties,
        &effect_scroll_generations,
        &ctx,
        false,
    );
    assert!(matches!(
        &captured,
        AutoAuthorityDecision::EffectScrollScene { .. }
    ));
    assert!(matches!(
        &uncaptured,
        AutoAuthorityDecision::EffectScrollScene { .. }
    ));
    assert!(!auto_authority_trace(&captured).rejections.is_empty());
    assert!(auto_authority_trace(&uncaptured).rejections.is_empty());

    let (arena, roots) = prepared_safe_leaf();
    let (properties, generations) = synced_paint_state(&arena, &roots);
    let captured =
        select_retained_auto_authority(&arena, &roots, &properties, &generations, &ctx, true);
    let uncaptured =
        select_retained_auto_authority(&arena, &roots, &properties, &generations, &ctx, false);
    assert_eq!(
        auto_authority_kind(&captured),
        auto_authority_kind(&uncaptured)
    );
    assert_eq!(auto_authority_kind(&captured), AutoAuthorityKind::Artifact);
    assert!(auto_authority_trace(&captured).rejections.is_empty());
    assert!(auto_authority_trace(&uncaptured).rejections.is_empty());
}

#[test]
fn retained_auto_telemetry_labels_every_selected_authority_without_named_aliases() {
    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let (arena, roots) = prepared_safe_leaf();
    assert_eq!(
        telemetry_for_auto_decision(auto_decision(&arena, &roots, &ctx))
            .snapshot()
            .authority_label,
        "retained-auto:artifact"
    );

    let (arena, roots) = prepared_transform_leaf();
    assert_eq!(
        telemetry_for_auto_decision(auto_decision(&arena, &roots, &ctx))
            .snapshot()
            .authority_label,
        "retained-auto:property-scene"
    );

    let (arena, roots, _) = prepared_nested_transform_tree();
    assert_eq!(
        telemetry_for_auto_decision(auto_decision(&arena, &roots, &ctx))
            .snapshot()
            .authority_label,
        "retained-auto:property-scene"
    );

    let (arena, roots, _, _, _) = prepared_transform_child_isolation_tree();
    assert_eq!(
        telemetry_for_auto_decision(auto_decision(&arena, &roots, &ctx))
            .snapshot()
            .authority_label,
        "retained-auto:property-scene"
    );

    let (arena, roots) = prepared_safe_leaf();
    crate::view::test_support::get_element_mut::<Element>(&arena, roots[0]).set_opacity(0.5);
    let isolation_telemetry = telemetry_for_auto_decision(auto_decision(&arena, &roots, &ctx));
    assert_eq!(
        isolation_telemetry.snapshot().authority_label,
        "retained-auto:property-scene"
    );
    let formatted = isolation_telemetry.format_debug();
    assert!(formatted.contains("retained-auto:property-scene"));
    assert!(!formatted.contains("retained-isolation-canary"));

    let (arena, roots, properties, generations) = prepared_exact_scroll_scene();
    let decision =
        select_retained_auto_authority(&arena, &roots, &properties, &generations, &ctx, true);
    assert_eq!(
        telemetry_for_auto_decision(decision)
            .snapshot()
            .authority_label,
        "retained-auto:property-scene"
    );
}

#[test]
fn true_legacy_unknown_custom_host_has_red_reason_snapshot() {
    let id = 0xd3_b001;
    let mut arena = new_test_arena();
    let root = commit_element(
        &mut arena,
        Box::new(UnknownOverlayHost {
            id,
            bounds: BoxModelSnapshot {
                node_id: id,
                parent_id: None,
                x: 12.0,
                y: 18.0,
                width: 64.0,
                height: 32.0,
                border_radius: 0.0,
                should_render: true,
            },
        }),
    );
    let roots = vec![root];
    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let mut telemetry = telemetry_for_auto_decision(auto_decision(&arena, &roots, &ctx));
    telemetry.note_legacy_fallback(PaintAuthorityFallbackStage::Selection);
    let overlay = retained_auto_fallback_overlay_records(&telemetry, &roots);
    assert_eq!(
        overlay,
        vec![(
            root,
            Some(crate::view::paint::LegacyPaintReason::UnknownHost)
        )]
    );
    assert_eq!(
        retained_auto_overlay_label(
            std::any::type_name::<UnknownOverlayHost>(),
            id,
            overlay[0].1,
        ),
        "UnknownOverlayHost#13873153 fallback=unknown-host"
    );

    let mut viewport = Viewport::new();
    viewport.scene.node_arena = arena;
    let capture = viewport.build_retained_auto_debug_capture(&telemetry, &roots, true, true);
    assert_eq!(
        capture.frame.disposition,
        crate::view::debug::DebugFrameDisposition::FellBackToLegacy
    );
    assert_eq!(
        capture.frame.selected_authority,
        crate::view::debug::DebugFramePaintAuthority::Legacy
    );
    assert_eq!(capture.frame.statistics.fallback_count, 1);
    assert_eq!(capture.frame.fallback_stages.len(), 1);
    assert_eq!(
        capture.frame.fallback_stages[0].detail,
        crate::view::debug::DebugFallbackDetail::Boundary {
            reason: "unknown-host"
        }
    );
}

#[test]
fn paint_authority_telemetry_keeps_rejections_stages_and_scroll_costs_structured() {
    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let (arena, roots) = prepared_mixed_eligibility_roots();
    let mut telemetry = telemetry_for_auto_decision(auto_decision(&arena, &roots, &ctx));
    telemetry.note_legacy_fallback(PaintAuthorityFallbackStage::Selection);
    let snapshot = telemetry.snapshot();
    assert_eq!(snapshot.authority_label, "retained-auto:legacy");
    assert_eq!(snapshot.selected, PaintAuthorityKind::Legacy);
    assert_eq!(
        snapshot.legacy_fallback_stage,
        Some(PaintAuthorityFallbackStage::Selection)
    );
    telemetry.note_artifact_rejection(crate::view::paint::FrameArtifactEligibility {
        reasons: vec![
            crate::view::paint::FrameArtifactFallbackReason::LegacyBoundary(
                crate::view::paint::LegacyPaintReason::UnknownHost,
            ),
        ],
        debug_boundaries: vec![crate::view::paint::FrameArtifactDebugBoundary {
            owner: roots[0],
            kind: crate::view::paint::FrameArtifactDebugBoundaryKind::Legacy(
                crate::view::paint::LegacyPaintReason::UnknownHost,
            ),
        }],
        ..Default::default()
    });
    let fallback_nodes = telemetry.fallback_boundary_nodes();
    assert!(fallback_nodes.contains(&roots[0]));
    assert_eq!(
        fallback_nodes
            .iter()
            .filter(|&&owner| owner == roots[0])
            .count(),
        1,
        "structured fallback owners must be deduplicated",
    );
    assert_eq!(
        debug_legacy_fallback(crate::view::paint::LegacyPaintReason::UnknownHost).0,
        crate::view::debug::DebugFallbackCategory::UnsupportedHost,
    );
    assert_eq!(
        debug_legacy_fallback(crate::view::paint::LegacyPaintReason::BoxShadow).1,
        crate::view::debug::DebugFallbackDetail::Boundary {
            reason: "box-shadow",
        },
    );
    assert_eq!(
        retained_auto_overlay_label(
            "rfgui::view::base_component::Element",
            42,
            Some(crate::view::paint::LegacyPaintReason::ChildClip),
        ),
        "Element#42 fallback=child-clip",
    );

    let (arena, roots) = prepared_transform_leaf();
    let mut prepare_failure = telemetry_for_auto_decision(auto_decision(&arena, &roots, &ctx));
    prepare_failure.note_legacy_fallback(PaintAuthorityFallbackStage::Prepare);
    assert_eq!(
        prepare_failure.snapshot().legacy_fallback_stage,
        Some(PaintAuthorityFallbackStage::Prepare)
    );
    let mut build_failure = prepare_failure.clone();
    build_failure.note_legacy_fallback(PaintAuthorityFallbackStage::Build);
    assert_eq!(
        build_failure.snapshot().legacy_fallback_stage,
        Some(PaintAuthorityFallbackStage::Build)
    );
    let mut compile_failure = prepare_failure.clone();
    compile_failure.note_legacy_fallback(PaintAuthorityFallbackStage::Compile);
    assert_eq!(
        compile_failure.snapshot().legacy_fallback_stage,
        Some(PaintAuthorityFallbackStage::Compile)
    );
    let mut terminal_failure = prepare_failure;
    terminal_failure.note_terminal_failure(PaintAuthorityFallbackStage::Execute);
    assert_eq!(
        terminal_failure.snapshot().terminal_failure_stage,
        Some(PaintAuthorityFallbackStage::Execute)
    );

    let mut scroll = telemetry_for_auto_decision(auto_decision(&arena, &roots, &ctx));
    scroll.note_scroll_content(crate::view::paint::ScrollSceneBuildTrace {
        backing: crate::view::paint::ScrollSceneBackingKind::Single,
        action: crate::view::paint::RetainedSurfaceCompileAction::Reuse,
        content_root: roots[0],
        descriptor_size: [64, 128],
        content_chunk_count: 2,
        content_op_count: 3,
        content_pair_bytes: 65_536,
        tile_count: 1,
        reraster_count: 0,
        reuse_count: 1,
    });
    let single = scroll.snapshot().scroll_content.expect("single telemetry");
    assert_eq!(
        single.backing,
        crate::view::paint::ScrollSceneBackingKind::Single
    );
    assert_eq!(single.tile_count, 1);
    assert_eq!(single.pair_bytes, 65_536);
    assert_eq!(scroll.snapshot().resident_release_count, None);

    scroll.note_scroll_content(crate::view::paint::ScrollSceneBuildTrace {
        backing: crate::view::paint::ScrollSceneBackingKind::Tiled,
        action: crate::view::paint::RetainedSurfaceCompileAction::Reraster,
        content_root: roots[0],
        descriptor_size: [64, 64],
        content_chunk_count: 2,
        content_op_count: 3,
        content_pair_bytes: 131_072,
        tile_count: 3,
        reraster_count: 2,
        reuse_count: 1,
    });
    let tiled = scroll.snapshot().scroll_content.expect("tiled telemetry");
    assert_eq!(
        tiled.backing,
        crate::view::paint::ScrollSceneBackingKind::Tiled
    );
    assert_eq!(
        (tiled.tile_count, tiled.reraster_count, tiled.reuse_count),
        (3, 2, 1)
    );
    assert_eq!(tiled.pair_bytes, 131_072);
    let tiled_debug = scroll.format_debug();
    assert!(tiled_debug.contains("pair-bytes=131072"));
    assert!(tiled_debug.contains("resident-releases=unavailable"));
    assert!(!tiled_debug.contains("resident-bytes"));
}

#[test]
fn paint_authority_test_capture_is_explicit_and_thread_local() {
    assert!(!paint_authority_test_capture_enabled());
    assert!(take_paint_authority_test_snapshot().is_none());

    {
        let _guard = enable_paint_authority_test_capture();
        assert!(paint_authority_test_capture_enabled());

        let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let (arena, roots) = prepared_transform_leaf();
        let telemetry = telemetry_for_auto_decision(auto_decision(&arena, &roots, &ctx));
        store_paint_authority_test_snapshot(&telemetry);

        let snapshot = take_paint_authority_test_snapshot().expect("captured snapshot");
        assert_eq!(snapshot.selected, PaintAuthorityKind::PropertyScene);

        store_paint_authority_test_snapshot(&telemetry);
    }

    assert!(!paint_authority_test_capture_enabled());
    assert!(
        take_paint_authority_test_snapshot().is_none(),
        "dropping the capture guard must discard its last snapshot"
    );
}

#[test]
fn failed_begin_frame_attempt_cannot_reuse_previous_authority_snapshot() {
    let _guard = enable_paint_authority_test_capture();
    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let (arena, roots) = prepared_transform_leaf();
    let telemetry = telemetry_for_auto_decision(auto_decision(&arena, &roots, &ctx));

    // Model a completed frame whose snapshot has not yet been consumed,
    // followed by a render attempt that returns from `begin_frame`.
    store_paint_authority_test_snapshot(&telemetry);
    begin_paint_authority_telemetry_attempt();
    assert!(take_paint_authority_test_snapshot().is_none());

    // The same failed-attempt boundary remains empty when the successful
    // frame snapshot was already consumed by the test.
    store_paint_authority_test_snapshot(&telemetry);
    assert!(take_paint_authority_test_snapshot().is_some());
    begin_paint_authority_telemetry_attempt();
    assert!(take_paint_authority_test_snapshot().is_none());
}

#[test]
fn resident_release_telemetry_reports_each_frame_delta() {
    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let (arena, roots) = prepared_transform_leaf();

    let mut first_frame = telemetry_for_auto_decision(auto_decision(&arena, &roots, &ctx));
    first_frame.note_resident_release_delta(4, 7);

    let mut second_frame = telemetry_for_auto_decision(auto_decision(&arena, &roots, &ctx));
    second_frame.note_resident_release_delta(7, 7);

    assert_eq!(first_frame.snapshot().resident_release_count, Some(3));
    assert_eq!(second_frame.snapshot().resident_release_count, Some(0));
}
