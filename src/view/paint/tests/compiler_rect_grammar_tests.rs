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
        chunk.payload_identity =
            PaintPayloadIdentity::prepared_rects(artifact.ops[range].iter().filter_map(|op| {
                match op {
                    PaintOp::DrawRect(rect) => Some(rect),
                    _ => None,
                }
            }))
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

#[test]
fn compiler_rejects_every_invalid_generic_rect_phase_before_emit() {
    let mut empty = compiler_rect_phase_artifact(
        PaintChunkRole::SelectionUnderlay,
        vec![rect_phase_op(1.0, [1.0, 0.0, 0.0, 1.0])],
    );
    empty.ops.clear();
    empty.chunks[0].op_range = 0..0;
    empty.chunks[0].payload_identity =
        PaintPayloadIdentity::prepared_rects(std::iter::empty()).unwrap();
    assert_compiler_rejects_before_emit(&empty, "empty generic rect phase");

    let caret_multi = compiler_rect_phase_artifact(
        PaintChunkRole::Caret,
        vec![
            rect_phase_op(1.0, [1.0, 0.0, 0.0, 1.0]),
            rect_phase_op(2.0, [0.0, 1.0, 0.0, 1.0]),
        ],
    );
    assert_compiler_rejects_before_emit(&caret_multi, "multi-rect caret");

    let mut wrong_mode = compiler_rect_phase_artifact(
        PaintChunkRole::TextDecoration,
        vec![rect_phase_op(1.0, [1.0, 0.0, 0.0, 1.0])],
    );
    let PaintOp::DrawRect(rect) = &mut wrong_mode.ops[0] else {
        unreachable!()
    };
    rect.mode = crate::view::render_pass::draw_rect_pass::RectRenderMode::BorderOnly;
    refresh_rect_phase_identity(&mut wrong_mode);
    assert_compiler_rejects_before_emit(&wrong_mode, "non-FillOnly rect phase");

    let mut mixed = compiler_rect_phase_artifact(
        PaintChunkRole::SelectionUnderlay,
        vec![
            rect_phase_op(1.0, [1.0, 0.0, 0.0, 1.0]),
            rect_phase_op(2.0, [0.0, 1.0, 0.0, 1.0]),
        ],
    );
    let image = compiler_image_test_artifact(false);
    mixed.ops[1] = image.ops[0].clone();
    assert_compiler_rejects_before_emit(&mixed, "mixed rect and non-rect phase");

    let mut reordered = compiler_rect_phase_artifact(
        PaintChunkRole::TextDecoration,
        vec![
            rect_phase_op(1.0, [1.0, 0.0, 0.0, 1.0]),
            rect_phase_op(2.0, [0.0, 1.0, 0.0, 1.0]),
        ],
    );
    reordered.chunks[0].payload_identity = PaintPayloadIdentity::prepared_rects(
        reordered.ops.iter().rev().filter_map(|op| match op {
            PaintOp::DrawRect(rect) => Some(rect),
            _ => None,
        }),
    )
    .unwrap();
    assert_compiler_rejects_before_emit(&reordered, "reordered rect identity");

    let mut tampered = compiler_rect_phase_artifact(
        PaintChunkRole::SelectionUnderlay,
        vec![rect_phase_op(1.0, [1.0, 0.0, 0.0, 1.0])],
    );
    let PaintOp::DrawRect(rect) = &mut tampered.ops[0] else {
        unreachable!()
    };
    rect.params.position[0] = f32::from_bits(rect.params.position[0].to_bits() ^ 1);
    assert_compiler_rejects_before_emit(&tampered, "tampered rect params");
}

#[test]
fn compiler_rejects_duplicate_owner_phase_slot_even_when_roles_differ() {
    let mut artifact = compiler_test_artifact();
    let mut duplicate_slot = artifact.chunks[0].clone();
    duplicate_slot.id.role = PaintChunkRole::TextGlyphs;
    duplicate_slot.op_range = artifact.ops.len()..artifact.ops.len();
    duplicate_slot.payload_identity =
        PaintPayloadIdentity::prepared_texts(std::iter::empty::<&PreparedTextOp>());
    artifact.chunks.push(duplicate_slot);

    assert_compiler_rejects_before_emit(
        &artifact,
        "duplicate owner/phase/slot with a distinct role",
    );
}

#[test]
fn compiler_rejects_border_only_image_grammar_before_emit() {
    let mut artifact = compiler_image_test_artifact(true);
    assert!(matches!(artifact.ops[0], PaintOp::DrawRect(_)));
    let PaintOp::DrawRect(border) = artifact.ops.remove(1) else {
        panic!("fixture second op must be border")
    };
    artifact.ops.remove(0);
    artifact.ops.insert(0, PaintOp::DrawRect(border));
    artifact.chunks[0].op_range = 0..artifact.ops.len();
    let graph = compiled_whole_frame_graph(&artifact);
    assert_eq!(graph.pass_descriptors().len(), 1, "only clear may remain");
}

#[test]
fn compiler_rejects_late_invalid_prepared_image_before_any_artifact_emit() {
    let mut artifact = compiler_image_test_artifact(false);
    let PaintOp::PreparedImage(mut invalid_op) = artifact.ops[0].clone() else {
        panic!("bare image fixture")
    };
    invalid_op.upload.pixels = Arc::from([1_u8, 2, 3]);
    let identity = PaintPayloadIdentity::image_with_decoration(
        PreparedImageIdentity::from_op(&invalid_op),
        std::iter::empty(),
    )
    .unwrap();
    let mut key_arena = NodeArena::new();
    let second_owner = key_arena.insert(Node::new(Box::new(Element::new_with_id(
        999, 0.0, 0.0, 1.0, 1.0,
    ))));
    let start = artifact.ops.len();
    artifact.ops.push(PaintOp::PreparedImage(invalid_op));
    let mut late = artifact.chunks[0].clone();
    late.id.owner = second_owner;
    late.owner = second_owner;
    late.op_range = start..artifact.ops.len();
    late.payload_identity = identity;
    artifact.chunks.push(late);

    let graph = compiled_whole_frame_graph(&artifact);
    assert_eq!(graph.pass_descriptors().len(), 1, "only clear may remain");
}

#[test]
fn compiler_rejects_standard_draw_rect_composite_identity_drift_before_emit() {
    let (arena, root, properties, generations) =
        prepared_leaf(0x6a70, Color::rgb(20, 80, 160), 1.0, true);
    let (element, _) = whole_frame_artifact(&arena, &[root], &properties, &generations);
    assert!(
        compiled_whole_frame_graph(&element)
            .pass_descriptors()
            .len()
            > 1
    );

    let mut fill_drift = element.clone();
    let fill = fill_drift
        .ops
        .iter_mut()
        .find_map(|op| match op {
            PaintOp::DrawRect(rect)
                if rect.mode
                    == crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly =>
            {
                Some(rect)
            }
            _ => None,
        })
        .unwrap();
    fill.params.fill_color[0] = f32::from_bits(fill.params.fill_color[0].to_bits() ^ 1);
    assert_compiler_rejects_before_emit(&fill_drift, "Element fill params drift");

    let mut border_drift = element.clone();
    let border = border_drift
        .ops
        .iter_mut()
        .find_map(|op| match op {
            PaintOp::DrawRect(rect)
                if rect.mode
                    == crate::view::render_pass::draw_rect_pass::RectRenderMode::BorderOnly =>
            {
                Some(rect)
            }
            _ => None,
        })
        .unwrap();
    border.params.border_widths[0] =
        f32::from_bits(border.params.border_widths[0].to_bits() ^ 1);
    assert_compiler_rejects_before_emit(&border_drift, "Element border params drift");

    let image = compiler_image_test_artifact(true);
    assert!(compiled_whole_frame_graph(&image).pass_descriptors().len() > 1);
    let mut image_drift = image.clone();
    let PaintOp::DrawRect(rect) = &mut image_drift.ops[0] else {
        panic!("decorated Image must start with DrawRect")
    };
    rect.params.opacity = f32::from_bits(rect.params.opacity.to_bits() ^ 1);
    assert_compiler_rejects_before_emit(&image_drift, "Image decoration drift");

    let svg = compiler_svg_test_artifact(true);
    assert!(compiled_whole_frame_graph(&svg).pass_descriptors().len() > 1);
    let mut svg_drift = svg.clone();
    let PaintOp::DrawRect(rect) = &mut svg_drift.ops[0] else {
        panic!("decorated SVG must start with DrawRect")
    };
    rect.params.position[0] = f32::from_bits(rect.params.position[0].to_bits() ^ 1);
    assert_compiler_rejects_before_emit(&svg_drift, "SVG decoration drift");

    let mut missing = element.clone();
    missing.chunks[0].payload_identity = PaintPayloadIdentity::None;
    assert_compiler_rejects_before_emit(&missing, "missing DrawRect composite identity");

    let mut missing_decoration = element.clone();
    missing_decoration.chunks[0].payload_identity =
        PaintPayloadIdentity::prepared_shadows(std::iter::empty());
    assert_compiler_rejects_before_emit(
        &missing_decoration,
        "content-only identity missing DrawRect composite",
    );

    let mut wrong = element;
    wrong.chunks[0].payload_identity = image.chunks[0].payload_identity.clone();
    assert_compiler_rejects_before_emit(&wrong, "wrong composite identity variant");
}

#[test]
fn standard_draw_rect_identity_accepts_and_freezes_fill_and_border_gradients() {
    let mut arena = new_test_arena();
    let mut element = Element::new_with_id(0x6a71, 0.0, 0.0, 96.0, 48.0);
    apply_gradient_style(&mut element, "#ff0000", "#0000ff", "#ffffff", "#000000");
    let root = commit_element(&mut arena, Box::new(element));
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    let (properties, generations) = sync_identity(&arena, &[root]);
    let (artifact, _) = whole_frame_artifact(&arena, &[root], &properties, &generations);
    assert!(
        compiled_whole_frame_graph(&artifact)
            .pass_descriptors()
            .len()
            > 1
    );

    let mut fill_axis_drift = artifact.clone();
    let fill = fill_axis_drift
        .ops
        .iter_mut()
        .find_map(|op| match op {
            PaintOp::DrawRect(rect)
                if rect.mode
                    == crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly =>
            {
                Some(rect)
            }
            _ => None,
        })
        .unwrap();
    let gradient = fill.params.gradient.as_mut().expect("fill gradient");
    gradient.axis[0] = f32::from_bits(gradient.axis[0].to_bits() ^ 1);
    assert_compiler_rejects_before_emit(&fill_axis_drift, "fill gradient axis drift");

    let mut border_stop_drift = artifact;
    let border = border_stop_drift
        .ops
        .iter_mut()
        .find_map(|op| match op {
            PaintOp::DrawRect(rect)
                if rect.mode
                    == crate::view::render_pass::draw_rect_pass::RectRenderMode::BorderOnly =>
            {
                Some(rect)
            }
            _ => None,
        })
        .unwrap();
    let gradient = border
        .params
        .border_gradient
        .as_mut()
        .expect("border gradient");
    let stops = Arc::make_mut(&mut gradient.stops);
    stops[0].color[0] = f32::from_bits(stops[0].color[0].to_bits() ^ 1);
    assert_compiler_rejects_before_emit(&border_stop_drift, "border gradient stop drift");
}
