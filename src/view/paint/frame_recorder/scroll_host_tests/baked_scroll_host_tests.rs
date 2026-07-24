use super::*;

#[test]
fn baked_scroll_host_recorder_preserves_order_properties_and_empty_overlay_parity() {
    let (arena, root, child, properties, generations) = fixture();
    let scroll = ScrollNodeId(root);
    let clip = ClipNodeId {
        owner: root,
        role: ClipNodeRole::ContentsClip,
    };
    let scroll_snapshot = properties.scroll_snapshot_for(scroll).unwrap();
    let witness = PaintBakedScrollHostWitness::new(root, child, scroll_snapshot, clip).unwrap();
    let artifact = record_baked_scroll_host_artifact_for_plan(
        &arena,
        &[root],
        &properties,
        &generations,
        witness,
    )
    .unwrap();

    assert_eq!(artifact.chunks.len(), 3);
    assert_eq!(artifact.chunks[0].owner, root);
    assert_eq!(
        artifact.chunks[0].id.phase,
        super::super::super::PaintNodePhase::BeforeChildren
    );
    assert_eq!(artifact.chunks[0].properties, Default::default());
    assert_eq!(artifact.chunks[1].owner, child);
    assert_eq!(artifact.chunks[1].properties.scroll, Some(scroll));
    assert_eq!(artifact.chunks[1].properties.clip, Some(clip));
    let overlay = &artifact.chunks[2];
    assert_eq!(overlay.owner, root);
    assert_eq!(
        overlay.id.phase,
        super::super::super::PaintNodePhase::AfterChildren
    );
    assert_eq!(
        overlay.id.role,
        super::super::super::PaintChunkRole::ScrollbarOverlay
    );
    assert_eq!(overlay.properties, Default::default());
    assert!(overlay.op_range.is_empty());
    assert_eq!(overlay.op_range.start, artifact.ops.len());
}

#[test]
fn opaque_scrollbar_recorder_freezes_one_exact_legacy_order_overlay() {
    let (arena, root, child, properties, generations) = opaque_fixture();
    let scroll_id = ScrollNodeId(root);
    let clip_id = ClipNodeId {
        owner: root,
        role: ClipNodeRole::ContentsClip,
    };
    let scroll = properties.scroll_snapshot_for(scroll_id).unwrap();
    let contents_clip = properties
        .clip_snapshot_for(Some(clip_id))
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    let witness = PaintBakedScrollHostWitness::new(root, child, scroll, clip_id).unwrap();
    let artifact = record_baked_scroll_host_artifact_for_plan(
        &arena,
        &[root],
        &properties,
        &generations,
        witness,
    )
    .unwrap();

    let overlay_chunk = artifact.chunks.last().unwrap();
    assert_eq!(overlay_chunk.op_range.len(), 1);
    let [PaintOp::PreparedScrollbarOverlay(overlay)] =
        &artifact.ops[overlay_chunk.op_range.clone()]
    else {
        panic!("opaque overlay must remain one typed op");
    };
    assert!(overlay.matches_vertical_witness(scroll.scrollbar_overlay));
    assert_eq!(
        overlay_chunk.payload_identity,
        PaintPayloadIdentity::prepared_scrollbar_overlay(overlay)
    );
    assert_eq!(
        overlay.track_shadow.params.color[3].to_bits(),
        0.5_f32.to_bits()
    );
    assert_eq!(
        overlay.track.params.fill_color[3].to_bits(),
        0.35_f32.to_bits()
    );
    assert_eq!(
        overlay.thumb_shadow.params.color[3].to_bits(),
        0.5_f32.to_bits()
    );
    assert_eq!(
        overlay.thumb.params.fill_color[3].to_bits(),
        0.58_f32.to_bits()
    );
    assert!(
        super::super::super::compiler::validate_baked_scroll_host_artifact(
            &artifact,
            root,
            child,
            scroll,
            contents_clip,
        )
        .is_some()
    );
    let mut wrong_geometry = scroll;
    wrong_geometry
        .scrollbar_overlay
        .vertical_track
        .as_mut()
        .unwrap()
        .x += 1.0;
    assert!(
        super::super::super::compiler::validate_baked_scroll_host_artifact(
            &artifact,
            root,
            child,
            wrong_geometry,
            contents_clip,
        )
        .is_none()
    );

    let validates = |artifact: &PaintArtifact| {
        super::super::super::compiler::validate_baked_scroll_host_artifact(
            artifact,
            root,
            child,
            scroll,
            contents_clip,
        )
        .is_some()
    };
    let mut malicious = artifact.clone();
    let PaintOp::PreparedScrollbarOverlay(overlay) =
        &mut malicious.ops[overlay_chunk.op_range.start]
    else {
        unreachable!()
    };
    overlay.track_shadow.params.blur_radius += 1.0;
    assert!(!validates(&malicious));

    malicious = artifact.clone();
    let PaintOp::PreparedScrollbarOverlay(overlay) =
        &mut malicious.ops[overlay_chunk.op_range.start]
    else {
        unreachable!()
    };
    std::mem::swap(&mut overlay.track.params, &mut overlay.thumb.params);
    assert!(!validates(&malicious));

    malicious = artifact.clone();
    malicious.chunks.last_mut().unwrap().payload_identity =
        PaintPayloadIdentity::prepared_shadows(std::iter::empty());
    assert!(!validates(&malicious));

    malicious = artifact.clone();
    let extra = malicious.ops[overlay_chunk.op_range.start].clone();
    malicious.ops.push(extra);
    malicious.chunks.last_mut().unwrap().op_range.end += 1;
    assert!(!validates(&malicious));

    malicious = artifact.clone();
    malicious.ops.clear();
    malicious.chunks.last_mut().unwrap().op_range = 0..0;
    assert!(!validates(&malicious));

    malicious = artifact.clone();
    let PaintOp::PreparedScrollbarOverlay(overlay) =
        malicious.ops[overlay_chunk.op_range.start].clone()
    else {
        unreachable!()
    };
    malicious.ops[overlay_chunk.op_range.start] = PaintOp::DrawRect(overlay.track);
    assert!(!validates(&malicious));

    for index in 0..artifact.chunks.len() {
        malicious = artifact.clone();
        malicious.chunks[index].id.slot = 1;
        assert!(!validates(&malicious), "chunk {index} slot drift must fail");

        malicious = artifact.clone();
        malicious.chunks[index].id.scope = super::super::super::PaintPropertyScope::Contents;
        assert!(
            !validates(&malicious),
            "chunk {index} scope drift must fail"
        );
    }
}

#[test]
fn opaque_scrollbar_reuses_stable_stamp_and_blur_drift_rerasterizes() {
    let prepare = |blur_radius| {
        let (arena, root, _child, properties, generations) =
            fixture_with_scrollbar(true, blur_radius);
        let plan = super::super::super::plan_single_root_scroll_host_surface(
            &arena,
            &[root],
            &properties,
            &generations,
            1.0,
            [0.0; 2],
            None,
        )
        .unwrap();
        let graph = crate::view::frame_graph::FrameGraph::new();
        let ctx = crate::view::base_component::UiBuildContext::new(
            100,
            80,
            wgpu::TextureFormat::Rgba8Unorm,
            1.0,
        );
        super::super::super::prepare_retained_scroll_host_stamp_for_test(&plan, &graph, &ctx)
            .unwrap()
    };
    let baseline = prepare(3.0);
    let drifted = prepare(7.0);
    assert!(super::super::super::retained_surface_raster_stamp_is_canonical(&baseline));
    assert!(super::super::super::retained_surface_raster_stamp_is_canonical(&drifted));
    for index in 0..baseline.chunks.len() {
        let mut malicious = baseline.clone();
        malicious.chunks[index].id.slot = 1;
        let [super::super::super::RetainedSurfaceRasterStepStamp::ArtifactSpan(span)] =
            malicious.ordered_steps.as_mut_slice()
        else {
            panic!("scroll host stamp must have one artifact span");
        };
        span.chunks[index].id.slot = 1;
        assert!(!super::super::super::retained_surface_raster_stamp_is_canonical(&malicious));

        let mut malicious = baseline.clone();
        malicious.chunks[index].id.scope = super::super::super::PaintPropertyScope::Contents;
        let [super::super::super::RetainedSurfaceRasterStepStamp::ArtifactSpan(span)] =
            malicious.ordered_steps.as_mut_slice()
        else {
            panic!("scroll host stamp must have one artifact span");
        };
        span.chunks[index].id.scope = super::super::super::PaintPropertyScope::Contents;
        assert!(!super::super::super::retained_surface_raster_stamp_is_canonical(&malicious));
    }
    assert_eq!(
        crate::view::viewport::retained_surface_compile_action_against_resident_for_test(
            baseline.clone(),
            &baseline,
        ),
        super::super::super::RetainedSurfaceCompileAction::Reuse
    );
    assert_eq!(
        crate::view::viewport::retained_surface_compile_action_against_resident_for_test(
            baseline, &drifted,
        ),
        super::super::super::RetainedSurfaceCompileAction::Reraster
    );
}

#[test]
fn translucent_scrollbar_freezes_exact_sampled_alpha_into_typed_overlay() {
    let (arena, root, child, properties, generations) = translucent_fixture();
    let scroll_id = ScrollNodeId(root);
    let clip_id = ClipNodeId {
        owner: root,
        role: ClipNodeRole::ContentsClip,
    };
    let scroll = properties.scroll_snapshot_for(scroll_id).unwrap();
    let contents_clip = properties
        .clip_snapshot_for(Some(clip_id))
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    let alpha = scroll.scrollbar_overlay.sampled_alpha;
    assert!((0.0..1.0).contains(&alpha));

    let witness = PaintBakedScrollHostWitness::new(root, child, scroll, clip_id).unwrap();
    let artifact = record_baked_scroll_host_artifact_for_plan(
        &arena,
        &[root],
        &properties,
        &generations,
        witness,
    )
    .unwrap();
    let [PaintOp::PreparedScrollbarOverlay(overlay)] =
        &artifact.ops[artifact.chunks.last().unwrap().op_range.clone()]
    else {
        panic!("translucent overlay must remain one typed op");
    };
    assert!(overlay.matches_vertical_witness(scroll.scrollbar_overlay));
    assert_eq!(
        overlay.track_shadow.params.color[3].to_bits(),
        (0.5 * alpha).to_bits()
    );
    assert_eq!(
        overlay.track.params.fill_color[3].to_bits(),
        (0.35 * alpha).to_bits()
    );
    assert_eq!(
        overlay.thumb.params.fill_color[3].to_bits(),
        (0.58 * alpha).to_bits()
    );
    assert!(
        super::super::super::compiler::validate_baked_scroll_host_artifact(
            &artifact,
            root,
            child,
            scroll,
            contents_clip,
        )
        .is_some()
    );
}

#[test]
fn general_recorder_still_rejects_scroll_host_without_owned_witness() {
    let (arena, root, _child, properties, generations) = fixture();
    let error = record_frame_artifact(
        &arena,
        &[root],
        &properties,
        &generations,
        RendererMode::StrictPlan,
    )
    .unwrap_err();
    assert!(error.reasons.iter().any(|reason| matches!(
        reason,
        FrameArtifactFallbackReason::LegacyBoundary(
            LegacyPaintReason::ScrollContainer | LegacyPaintReason::ChildClip
        )
    )));
}
