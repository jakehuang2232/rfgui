use super::*;

#[test]
fn custom_wrapper_public_typed_phases_preserve_order_slots_and_compile() {
    let host = CustomWrapperPaintHost::canonical(0x8f60);
    let (mut arena, root, child, properties, generations) = custom_wrapper_fixture(
        host,
        Box::new(leaf_element(0x8f61, Color::rgb(1, 2, 3), 1.0, false)),
    );

    let _ = take_full_artifact_record_count();
    let artifact = whole_frame_artifact(&arena, &[root], &properties, &generations);
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
