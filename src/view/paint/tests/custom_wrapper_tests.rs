use super::*;

#[test]
fn custom_wrapper_public_typed_phases_preserve_order_slots_and_compile() {
    let host = CustomWrapperPaintHost::canonical(0x8f60);
    let (mut arena, root, child, properties, generations) = custom_wrapper_fixture(
        host,
        Box::new(leaf_element(0x8f61, Color::rgb(1, 2, 3), 1.0, false)),
    );

    let _ = take_full_artifact_record_count();
    let (artifact, eligibility) =
        whole_frame_artifact(&arena, &[root], &properties, &generations);
    assert!(eligibility.eligible);
    let order = artifact
        .chunks
        .iter()
        .map(|chunk| (chunk.owner, chunk.id.phase, chunk.id.slot))
        .collect::<Vec<_>>();
    assert_eq!(
        order,
        vec![
            (root, PaintNodePhase::BeforeChildren, 0),
            (root, PaintNodePhase::BeforeChildren, 1),
            (child, PaintNodePhase::BeforeChildren, 0),
            (root, PaintNodePhase::AfterChildren, 0),
            (root, PaintNodePhase::AfterChildren, 1),
        ]
    );
    assert!(
        artifact
            .chunks
            .iter()
            .filter(|chunk| chunk.owner == root)
            .all(|chunk| chunk.id.scope == PaintPropertyScope::SelfPaint
                && chunk.id.role == PaintChunkRole::SelfDecoration)
    );
    assert!(
        artifact
            .chunks
            .iter()
            .filter(|chunk| chunk.owner == root)
            .all(|chunk| chunk.op_range.len() == 1)
    );

    let compiled_graph = compiled_whole_frame_graph(&artifact);
    let compiled_rects = compiled_graph.test_rect_pass_snapshots();
    assert_eq!(compiled_rects.len(), 5);
    assert_eq!(compiled_rects[1].opacity_bits, 1.0_f32.to_bits());
    assert_eq!(compiled_rects[1].fill_color_bits[3], 0.5_f32.to_bits());
    assert_eq!(compiled_rects[4].opacity_bits, 1.0_f32.to_bits());
    assert_eq!(compiled_rects[4].fill_color_bits[3], 0.25_f32.to_bits());

    let mut legacy_graph = FrameGraph::new();
    let legacy_ctx = UiBuildContext::new(64, 64, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    arena
        .with_element_taken(root, |element, arena| {
            element.build(&mut legacy_graph, arena, legacy_ctx)
        })
        .expect("wrapper root");
    let legacy_rects = legacy_graph.test_rect_pass_snapshots();
    assert_eq!(legacy_rects.len(), compiled_rects.len());
    for (index, (compiled, legacy)) in compiled_rects.iter().zip(&legacy_rects).enumerate() {
        assert_eq!(compiled.position_bits, legacy.position_bits);
        assert_eq!(compiled.size_bits, legacy.size_bits);
        assert_eq!(compiled.fill_color_bits, legacy.fill_color_bits);
        assert_eq!(compiled.opacity_bits, legacy.opacity_bits);
        assert_eq!(compiled.border_width_bits, legacy.border_width_bits);
        assert_eq!(compiled.border_radius_bits, legacy.border_radius_bits);
        assert_eq!(compiled.border_color_bits, legacy.border_color_bits);
        // The built-in child uses the visually equivalent zero-border
        // `Combined` legacy mode while its artifact canonicalizes to
        // `FillOnly`. Wrapper commands on either side must match exactly.
        if index != 2 {
            assert_eq!(compiled.mode, legacy.mode);
        }
    }
}

#[test]
fn custom_wrapper_legacy_build_traverses_child_exactly_once_between_phases() {
    let builds = Arc::new(AtomicUsize::new(0));
    let (mut arena, root, _, _, _) = custom_wrapper_fixture(
        CustomWrapperPaintHost::canonical(0x8f62),
        Box::new(RecordingHost {
            id: 0x8f63,
            builds: builds.clone(),
            fill: Some([0.2, 0.3, 0.4, 1.0]),
        }),
    );
    let mut graph = FrameGraph::new();
    let ctx = UiBuildContext::new(64, 64, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    arena
        .with_element_taken(root, |element, arena| element.build(&mut graph, arena, ctx))
        .expect("wrapper root");
    assert_eq!(builds.load(Ordering::Relaxed), 1);
    let rects = graph.test_rect_pass_snapshots();
    assert_eq!(rects.len(), 5);
    assert_eq!(
        rects[0].fill_color_bits,
        [0.8, 0.0, 0.0, 1.0].map(f32::to_bits)
    );
    assert_eq!(
        rects[1].fill_color_bits,
        [0.0, 0.8, 0.0, 0.5].map(f32::to_bits)
    );
    assert_eq!(
        rects[2].fill_color_bits,
        [0.2, 0.3, 0.4, 1.0].map(f32::to_bits)
    );
    assert_eq!(
        rects[3].fill_color_bits,
        [0.0, 0.0, 0.8, 1.0].map(f32::to_bits)
    );
    assert_eq!(
        rects[4].fill_color_bits,
        [0.8, 0.8, 0.0, 0.25].map(f32::to_bits)
    );
}

#[test]
fn custom_wrapper_drift_forces_whole_frame_fallback_after_one_full_plan() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut host = CustomWrapperPaintHost::canonical(0x8f64);
    host.mode = CustomWrapperRecordMode::Drift {
        calls: calls.clone(),
    };
    let (arena, root, _, properties, generations) = custom_wrapper_fixture(
        host,
        Box::new(leaf_element(0x8f65, Color::rgb(1, 2, 3), 1.0, false)),
    );
    let _ = take_full_artifact_record_count();
    assert!(matches!(
        record_frame_artifact(
            &arena,
            &[root],
            &properties,
            &generations,
            RendererMode::Auto,
        ),
        Ok(FrameArtifactRecordOutcome::WholeFrameLegacyFallback(_))
    ));
    assert_eq!(calls.load(Ordering::Relaxed), 4);
    assert!(take_full_artifact_record_count() >= 1);
}

#[test]
fn custom_wrapper_invalid_empty_and_slot_overflow_fail_before_full_recording() {
    for (id, mode) in [
        (0x8f66, CustomWrapperRecordMode::InvalidBounds),
        (0x8f68, CustomWrapperRecordMode::Empty),
        (0x8f6a, CustomWrapperRecordMode::Overflow),
    ] {
        let mut host = CustomWrapperPaintHost::canonical(id);
        host.mode = mode;
        let (arena, root, _, properties, generations) = custom_wrapper_fixture(
            host,
            Box::new(leaf_element(id + 1, Color::rgb(1, 2, 3), 1.0, false)),
        );
        let _ = take_full_artifact_record_count();
        assert!(matches!(
            record_frame_artifact(
                &arena,
                &[root],
                &properties,
                &generations,
                RendererMode::Auto,
            ),
            Ok(FrameArtifactRecordOutcome::WholeFrameLegacyFallback(_))
        ));
        assert_eq!(take_full_artifact_record_count(), 0);
    }
}

#[test]
fn custom_wrapper_topology_properties_and_unknown_child_fail_closed() {
    let (mut topology_arena, topology_root, _, _, _) = custom_wrapper_fixture(
        CustomWrapperPaintHost::canonical(0x8f70),
        Box::new(leaf_element(0x8f71, Color::rgb(1, 2, 3), 1.0, false)),
    );
    topology_arena.set_children(topology_root, Vec::new());
    assert_eq!(
        topology_arena
            .get(topology_root)
            .unwrap()
            .element
            .shadow_paint_recording_capability(
                &topology_arena,
                false,
                PaintRecordingContext::default(),
            ),
        ShadowPaintRecordingCapability::Unsupported
    );

    for contents in [false, true] {
        let (arena, root, _, mut properties, generations) = custom_wrapper_fixture(
            CustomWrapperPaintHost::canonical(0x8f72 + u64::from(contents)),
            Box::new(leaf_element(
                0x8f74 + u64::from(contents),
                Color::rgb(1, 2, 3),
                1.0,
                false,
            )),
        );
        let state = properties.states.get_mut(&root).unwrap();
        if contents {
            state.descendants.transform = Some(TransformNodeId(root));
        } else {
            state.paint.transform = Some(TransformNodeId(root));
        }
        let _ = take_full_artifact_record_count();
        assert!(matches!(
            record_frame_artifact(
                &arena,
                &[root],
                &properties,
                &generations,
                RendererMode::Auto,
            ),
            Ok(FrameArtifactRecordOutcome::WholeFrameLegacyFallback(_))
        ));
        assert_eq!(take_full_artifact_record_count(), 0);
    }

    let builds = Arc::new(AtomicUsize::new(0));
    let (arena, root, _, properties, generations) = custom_wrapper_fixture(
        CustomWrapperPaintHost::canonical(0x8f76),
        Box::new(RecordingHost {
            id: 0x8f77,
            builds,
            fill: None,
        }),
    );
    let _ = take_full_artifact_record_count();
    assert!(matches!(
        record_frame_artifact(
            &arena,
            &[root],
            &properties,
            &generations,
            RendererMode::Auto,
        ),
        Ok(FrameArtifactRecordOutcome::WholeFrameLegacyFallback(_))
    ));
    assert_eq!(take_full_artifact_record_count(), 0);
}
