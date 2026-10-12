use super::*;

#[test]
fn custom_wrapper_public_typed_phases_preserve_order_slots_and_compile() {
    let host = CustomWrapperPaintHost::canonical(0x8f60);
    let (arena, root, child, properties, generations) = custom_wrapper_fixture(
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
}
