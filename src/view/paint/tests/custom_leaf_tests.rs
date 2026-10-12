use super::*;

#[test]
fn custom_leaf_typed_adapter_records_canonical_fill_and_compiles() {
    let host = CustomLeafPaintHost::fill(0x8f10);
    let expected = host.bounds;
    let (arena, root, properties, generations) = custom_leaf_fixture(host);
    let _ = take_full_artifact_record_count();
    let artifact = whole_frame_artifact(&arena, &[root], &properties, &generations);
    assert_eq!(take_full_artifact_record_count(), 1);
    assert_eq!(artifact.chunks.len(), 1);
    assert_eq!(artifact.chunks[0].owner, root);
    assert_eq!(artifact.chunks[0].id.scope, PaintPropertyScope::SelfPaint);
    assert_eq!(artifact.chunks[0].id.phase, PaintNodePhase::BeforeChildren);
    assert_eq!(artifact.chunks[0].id.slot, 0);
    assert_eq!(artifact.chunks[0].id.role, PaintChunkRole::SelfDecoration);
    assert!(matches!(
        artifact.ops.as_slice(),
        [PaintOp::DrawRect(rect)]
            if rect.mode == crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly
    ));
    let graph = compiled_whole_frame_graph(&artifact);
    let rects = graph.test_rect_pass_snapshots();
    let [rect] = rects.as_slice() else {
        panic!("custom fill must compile to exactly one rect pass")
    };
    assert_eq!(
        rect.position_bits,
        [expected.x, expected.y].map(f32::to_bits)
    );
    assert_eq!(
        rect.size_bits,
        [expected.width, expected.height].map(f32::to_bits)
    );
    assert_eq!(rect.opacity_bits, 1.0_f32.to_bits());
    assert_eq!(rect.fill_color_bits[3], 0.75_f32.to_bits());
}

#[test]
fn custom_leaf_cannot_forge_scroll_placement_normalization_capability() {
    let host = CustomLeafPaintHost::fill(0x8f2f);
    assert!(host.retained_scroll_normalized_paint_capability().is_none());
}
