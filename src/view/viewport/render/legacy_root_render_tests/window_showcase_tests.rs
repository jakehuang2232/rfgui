use super::*;

#[test]
fn retained_auto_text_area_zero_and_bounded_scroll_select_artifact_and_invalid_states_legacy() {
    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    for scroll_y in [0.0, 9.0] {
        let (arena, roots, root) = prepared_auto_text_area(scroll_y, false);
        let (properties, _) = synced_paint_state(&arena, &roots);
        assert!(properties.scrolls.is_empty());
        let state = properties.node_state_for(root).unwrap();
        assert_ne!(state.paint.clip, state.descendants.clip);
        let AutoAuthorityDecision::Artifact { candidate, trace } =
            auto_decision(&arena, &roots, &ctx)
        else {
            panic!("bounded TextArea scroll {scroll_y} must select Artifact")
        };
        assert!(candidate.eligibility.eligible);
        assert!(trace.rejections.is_empty());
    }

    let (arena, roots, _) = prepared_auto_text_area(f32::NAN, false);
    {
        let AutoAuthorityDecision::Legacy { trace } = auto_decision(&arena, &roots, &ctx)
        else {
            panic!("invalid TextArea scroll state must select Legacy")
        };
        assert!(matches!(
            trace.rejections.first(),
            Some(AutoAuthorityRejection::Artifact { eligibility })
                if eligibility.reasons.contains(
                    &crate::view::paint::FrameArtifactFallbackReason::LegacyBoundary(
                        crate::view::paint::LegacyPaintReason::StatefulPaint,
                    )
                )
        ));
    }

    let (arena, roots, _) = prepared_auto_text_area(0.0, true);
    let AutoAuthorityDecision::Artifact { candidate, trace } =
        auto_decision(&arena, &roots, &ctx)
    else {
        panic!("pending caret-follow is paint-neutral and must select Artifact")
    };
    assert!(candidate.eligibility.eligible);
    assert!(trace.rejections.is_empty());
}

#[test]
fn retained_auto_routes_nested_effects_and_reports_typed_plan_rejection_for_interleave() {
    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let (arena, roots, _, child, _) = prepared_nested_opacity_tree();
    let decision = auto_decision(&arena, &roots, &ctx);
    assert_eq!(
        auto_authority_kind(&decision),
        AutoAuthorityKind::PropertyScene
    );
    assert!(auto_authority_trace(&decision).rejections.is_empty());
    assert_eq!(
        telemetry_for_auto_decision(decision)
            .snapshot()
            .authority_label,
        "retained-auto:property-scene"
    );

    crate::view::test_support::get_element_mut::<Element>(&arena, child)
        .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
            3.0, 0.0, 0.0,
        ))));
    let rejected = auto_decision(&arena, &roots, &ctx);
    assert_eq!(auto_authority_kind(&rejected), AutoAuthorityKind::Legacy);
    let [AutoAuthorityRejection::Plan { authority, error }] =
        auto_authority_trace(&rejected).rejections.as_slice()
    else {
        panic!("rejected effect/transform interleave has one typed plan rejection")
    };
    assert_eq!(*authority, AutoAuthorityKind::PropertyScene);
    assert!(error.reasons.iter().any(|reason| matches!(
        reason,
        crate::view::paint::FramePaintPlanRejection::CoLocatedTransformEffect(_)
            | crate::view::paint::FramePaintPlanRejection::UnsupportedPropertyInterleave(_)
    )));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn window_like_native_showcase_selects_non_legacy_retained_auto_authority() {
    let (arena, roots) = crate::view::paint::tests::window_like_native_showcase_fixture();
    let ctx = UiBuildContext::new(800, 600, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let (properties, generations) = synced_paint_state(&arena, &roots);
    if let Err(error) = crate::view::paint::plan_and_validate_frame_root_scroll_scene(
        &arena,
        &roots,
        &properties,
        &generations,
        1.0,
        [0.0; 2],
        None,
        wgpu::TextureFormat::Bgra8Unorm,
    ) {
        panic!("direct FrameRootScrollScene planning failed: {error:?}");
    }
    let decision =
        select_retained_auto_authority(&arena, &roots, &properties, &generations, &ctx, true);
    let (scene, trace) = match decision {
        AutoAuthorityDecision::FrameRootScrollScene { scene, trace } => (scene, trace),
        AutoAuthorityDecision::Legacy { trace } => panic!(
            "native Window-like showcase must not select Legacy; rejections={:?}; topology=transforms:{} effects:{} clips:{} scrolls:{} states:{} validation={:?} clip_nodes={:?}",
            trace.rejections,
            properties.transforms.len(),
            properties.effects.len(),
            properties.clips.len(),
            properties.scrolls.len(),
            properties.states.len(),
            properties.validation_errors,
            properties
                .clips
                .iter()
                .map(|(id, clip)| (*id, clip.owner, clip.parent, clip.geometry, clip.behavior))
                .collect::<Vec<_>>(),
        ),
        _ => panic!("native Window-like showcase selected an unexpected retained authority"),
    };
    assert!(scene.is_canonical());
    assert_eq!(scene.receiver_roots_for_test(), roots);
    assert!(scene.local_text_area_clip_tampering_is_rejected_for_test());

    let mut viewport = Viewport::new();
    let frame_owner = viewport
        .begin_retained_surface_frame_stage()
        .expect("retained frame stage");
    let mut graph = FrameGraph::new();
    let prepared = crate::view::paint::prepare_frame_root_scroll_scene(
        &mut viewport,
        scene,
        &mut graph,
        ctx,
        [0.0, 0.0, 0.0, 1.0],
        frame_owner,
    )
    .unwrap_or_else(|error| {
        panic!(
            "native Window-like FrameRootScrollScene prepare failed: {error:?}; earlier_rejections={:?}",
            trace.rejections
        )
    });
    let stamps = prepared.scroll_content_stamps_for_test();
    let [stamp] = stamps.as_slice() else {
        panic!("native Window-like fixture must own one scroll-content stamp")
    };
    assert_eq!(
        stamp
            .chunks
            .iter()
            .map(|chunk| chunk.id.role)
            .collect::<Vec<_>>(),
        vec![
            crate::view::paint::PaintChunkRole::SelfDecoration,
            crate::view::paint::PaintChunkRole::SelfDecoration,
            crate::view::paint::PaintChunkRole::TextGlyphs,
            crate::view::paint::PaintChunkRole::ImageContent,
            crate::view::paint::PaintChunkRole::SvgContent,
            crate::view::paint::PaintChunkRole::TextGlyphs,
            crate::view::paint::PaintChunkRole::SelfDecoration,
        ],
        "the exact child-mask pair must enclose the ordered non-TextArea siblings and TextArea"
    );
    let mut mask_after_first_sibling = stamp.clone();
    mask_after_first_sibling.chunks.swap(1, 2);
    let [crate::view::paint::RetainedSurfaceRasterStepStamp::ArtifactSpan(span)] =
        mask_after_first_sibling.ordered_steps.as_mut_slice()
    else {
        panic!("native Window-like stamp must contain one artifact span")
    };
    span.chunks.swap(1, 2);
    assert!(
        !crate::view::paint::retained_surface_raster_stamp_is_canonical(
            &mask_after_first_sibling
        ),
        "moving mask begin after the first sibling must fail closed"
    );
    let outcome = crate::view::paint::emit_prepared_frame_root_scroll_scene(prepared);
    let (_state, build_trace) = outcome.into_parts();
    assert!(
        !graph.pass_descriptors().is_empty(),
        "retained property scene must emit a complete frame"
    );
    assert!(build_trace.root_count > 0);
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(frame_owner), true));

    let second_ctx = UiBuildContext::new(800, 600, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let second_decision = select_retained_auto_authority(
        &arena,
        &roots,
        &properties,
        &generations,
        &second_ctx,
        true,
    );
    let second_scene = match second_decision {
        AutoAuthorityDecision::FrameRootScrollScene { scene, .. } => scene,
        _ => panic!("stable native Window-like scene must retain FrameRootScrollScene"),
    };
    let second_owner = viewport
        .begin_retained_surface_frame_stage()
        .expect("second retained frame stage");
    let mut second_graph = FrameGraph::new();
    let mut second_prepared = crate::view::paint::prepare_frame_root_scroll_scene(
        &mut viewport,
        second_scene,
        &mut second_graph,
        second_ctx,
        [0.0, 0.0, 0.0, 1.0],
        second_owner,
    )
    .expect("stable native Window-like scene prepares a second frame");
    second_prepared.refresh_actions_from_committed_test_pool();
    let second_actions = second_prepared.actions_for_test();
    assert!(!second_actions.is_empty());
    assert!(
        second_actions.iter().all(|action| {
            *action == crate::view::paint::RetainedSurfaceCompileAction::Reuse
        })
    );
    let second_outcome =
        crate::view::paint::emit_prepared_frame_root_scroll_scene(second_prepared);
    let (_second_state, second_trace) = second_outcome.into_parts();
    assert_eq!(second_trace.reraster_count, 0);
    assert_eq!(second_trace.reuse_count, second_actions.len());
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(second_owner), false,));
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        (0, None),
        "aborting the warm frame releases committed and pending residents"
    );

    let reversed_roots = roots.iter().copied().rev().collect::<Vec<_>>();
    let (reversed_properties, reversed_generations) =
        synced_paint_state(&arena, &reversed_roots);
    let reversed_ctx = UiBuildContext::new(800, 600, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let reversed_decision = select_retained_auto_authority(
        &arena,
        &reversed_roots,
        &reversed_properties,
        &reversed_generations,
        &reversed_ctx,
        true,
    );
    let reversed_scene = match reversed_decision {
        AutoAuthorityDecision::FrameRootScrollScene { scene, .. } => scene,
        _ => panic!("plain-before-scroll root order must retain FrameRootScrollScene"),
    };
    assert_eq!(reversed_scene.receiver_roots_for_test(), reversed_roots);
    let reversed_owner = viewport
        .begin_retained_surface_frame_stage()
        .expect("reversed retained frame stage");
    let mut reversed_graph = FrameGraph::new();
    let reversed_prepared = crate::view::paint::prepare_frame_root_scroll_scene(
        &mut viewport,
        reversed_scene,
        &mut reversed_graph,
        reversed_ctx,
        [0.0, 0.0, 0.0, 1.0],
        reversed_owner,
    )
    .expect("plain-before-scroll scene prepares");
    let reversed_outcome =
        crate::view::paint::emit_prepared_frame_root_scroll_scene(reversed_prepared);
    let (_reversed_state, reversed_trace) = reversed_outcome.into_parts();
    assert_eq!(reversed_trace.root_count, 2);
    assert_eq!(reversed_trace.scroll_group_count, 1);
    assert!(
        viewport.finish_retained_surface_transaction_for_frame(Some(reversed_owner), true,)
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn window_like_retained_final_keeps_candidate_rejection_off_fallback_overlay() {
    let (arena, roots) = crate::view::paint::tests::window_like_native_showcase_fixture();
    let ctx = UiBuildContext::new(800, 600, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let (properties, generations) = synced_paint_state(&arena, &roots);
    crate::view::paint::take_full_artifact_record_count();
    let captured =
        select_retained_auto_authority(&arena, &roots, &properties, &generations, &ctx, true);
    let captured_artifact_records = crate::view::paint::take_full_artifact_record_count();
    let uncaptured =
        select_retained_auto_authority(&arena, &roots, &properties, &generations, &ctx, false);
    let uncaptured_artifact_records = crate::view::paint::take_full_artifact_record_count();
    assert_eq!(
        auto_authority_kind(&captured),
        AutoAuthorityKind::PropertyScene
    );
    assert_eq!(
        auto_authority_kind(&captured),
        auto_authority_kind(&uncaptured)
    );
    assert_eq!(captured_artifact_records, uncaptured_artifact_records);

    let captured_scene = match captured {
        AutoAuthorityDecision::FrameRootScrollScene { scene, .. } => scene,
        _ => panic!("captured Window-like scene must retain FrameRootScrollScene"),
    };
    let uncaptured_scene = match uncaptured {
        AutoAuthorityDecision::FrameRootScrollScene { scene, .. } => scene,
        _ => panic!("uncaptured Window-like scene must retain FrameRootScrollScene"),
    };
    let mut action_snapshots = Vec::new();
    for scene in [captured_scene, uncaptured_scene] {
        let mut viewport = Viewport::new();
        let owner = viewport
            .begin_retained_surface_frame_stage()
            .expect("Window debug invariance frame stage");
        let mut graph = FrameGraph::new();
        let prepared = crate::view::paint::prepare_frame_root_scroll_scene(
            &mut viewport,
            scene,
            &mut graph,
            UiBuildContext::new(800, 600, wgpu::TextureFormat::Bgra8Unorm, 1.0),
            [0.0, 0.0, 0.0, 1.0],
            owner,
        )
        .expect("Window debug invariance prepare");
        action_snapshots.push(prepared.actions_for_test());
        assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));
    }
    assert_eq!(action_snapshots[0], action_snapshots[1]);

    let owner = roots[0];
    let trace = AutoAuthorityTrace {
        capture_rejections: true,
        rejections: vec![AutoAuthorityRejection::Artifact {
            eligibility: crate::view::paint::FrameArtifactEligibility {
                reasons: vec![
                    crate::view::paint::FrameArtifactFallbackReason::LegacyBoundary(
                        crate::view::paint::LegacyPaintReason::ChildClip,
                    ),
                ],
                debug_boundaries: vec![crate::view::paint::FrameArtifactDebugBoundary {
                    owner,
                    kind: crate::view::paint::FrameArtifactDebugBoundaryKind::Legacy(
                        crate::view::paint::LegacyPaintReason::ChildClip,
                    ),
                }],
                ..Default::default()
            },
        }],
    };
    let telemetry = PaintAuthorityTelemetry::from_selection(
        ViewportPaintRendererMode::RetainedAuto,
        &RetainedTransformCanarySelection::PropertyScenePrepared,
        Some((AutoAuthorityKind::PropertyScene, trace)),
    );
    assert_eq!(
        telemetry.final_authority(),
        PaintAuthorityKind::PropertyScene
    );
    assert_eq!(telemetry.selection_rejections.len(), 1);
    assert!(telemetry.fallback_boundary_nodes().is_empty());
    assert!(retained_auto_fallback_overlay_records(&telemetry, &roots).is_empty());
    assert!(
        telemetry
            .format_debug()
            .contains("candidate-rejections=[artifact:")
    );

    let mut viewport = Viewport::new();
    viewport.scene.node_arena = arena;
    let capture = viewport.build_retained_auto_debug_capture(&telemetry, &roots, true, true);
    assert_eq!(
        capture.frame.disposition,
        crate::view::debug::DebugFrameDisposition::Presented
    );
    assert_eq!(
        capture.frame.selected_authority,
        crate::view::debug::DebugFramePaintAuthority::PropertyScene
    );
    assert!(capture.frame.fallback_stages.is_empty());
    assert_eq!(capture.frame.statistics.fallback_count, 0);
    assert!(capture.nodes.iter().all(|node| node.fallbacks.is_empty()));
}
