use super::*;

#[test]
fn generic_rect_phase_roles_compile_in_frozen_chunk_and_op_order() {
    let mut artifact = compiler_test_artifact();
    let template = artifact.chunks[0].clone();
    let rects = [
        rect_phase_op(1.0, [1.0, 0.0, 0.0, 1.0]),
        rect_phase_op(2.0, [0.0, 1.0, 0.0, 1.0]),
        rect_phase_op(3.0, [0.0, 0.0, 1.0, 1.0]),
        rect_phase_op(4.0, [1.0, 1.0, 0.0, 1.0]),
        rect_phase_op(5.0, [1.0, 0.0, 1.0, 1.0]),
    ];
    artifact.ops = rects.iter().cloned().map(PaintOp::DrawRect).collect();
    artifact.chunks = [
        (
            PaintChunkRole::SelectionUnderlay,
            PaintNodePhase::BeforeChildren,
            0,
            0..2,
        ),
        (
            PaintChunkRole::TextDecoration,
            PaintNodePhase::AfterChildren,
            0,
            2..4,
        ),
        (
            PaintChunkRole::Caret,
            PaintNodePhase::AfterChildren,
            1,
            4..5,
        ),
    ]
    .into_iter()
    .map(|(role, phase, slot, range)| {
        let mut chunk = template.clone();
        chunk.id.role = role;
        chunk.id.phase = phase;
        chunk.id.slot = slot;
        chunk.op_range = range.clone();
        chunk.payload_identity = PaintPayloadIdentity::prepared_rects(
            artifact.ops[range].iter().filter_map(|op| match op {
                PaintOp::DrawRect(rect) => Some(rect),
                _ => None,
            }),
        )
        .unwrap();
        chunk
    })
    .collect();

    assert_eq!(
        artifact
            .chunks
            .iter()
            .map(|chunk| (chunk.id.role, chunk.id.phase, chunk.id.slot))
            .collect::<Vec<_>>(),
        vec![
            (
                PaintChunkRole::SelectionUnderlay,
                PaintNodePhase::BeforeChildren,
                0,
            ),
            (
                PaintChunkRole::TextDecoration,
                PaintNodePhase::AfterChildren,
                0,
            ),
            (PaintChunkRole::Caret, PaintNodePhase::AfterChildren, 1,),
        ]
    );
    let graph = compiled_whole_frame_graph(&artifact);
    let snapshots = graph.test_rect_pass_snapshots();
    assert_eq!(snapshots.len(), 5);
    assert_eq!(
        snapshots
            .iter()
            .map(|snapshot| snapshot.fill_color_bits)
            .collect::<Vec<_>>(),
        rects
            .iter()
            .map(|rect| rect.params.fill_color.map(f32::to_bits))
            .collect::<Vec<_>>()
    );
}
