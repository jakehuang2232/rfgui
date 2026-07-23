use super::*;

#[test]
fn compiler_emits_typed_straight_srgb_svg_as_texture_composite() {
    let artifact = compiler_svg_test_artifact(false);
    assert_eq!(artifact.chunks[0].id.role, PaintChunkRole::SvgContent);
    assert_eq!(artifact.chunks[0].properties, PropertyTreeState::default());

    let mut graph = compiled_whole_frame_graph(&artifact);
    let passes =
        graph.test_graphics_passes_mut::<crate::view::render_pass::TextureCompositePass>();
    assert_eq!(passes.len(), 1);
    let snapshot = passes[0].test_snapshot();
    let upload = snapshot
        .sampled_source
        .expect("SVG artifact must retain an owning sampled upload");
    assert!(matches!(
        upload.id,
        crate::view::sampled_texture::SampledTextureId::SvgRaster(_)
    ));
    assert_eq!(upload.format, wgpu::TextureFormat::Rgba8UnormSrgb);
    assert_eq!(
        upload.alpha_mode,
        crate::view::sampled_texture::SampledTextureAlphaMode::Straight
    );
    assert!(!snapshot.source_is_premultiplied);
    assert!(!snapshot.use_mask);
    assert!(snapshot.quad_position_bits.is_none());
    assert!(snapshot.mask_uv_bounds_bits.is_none());
    assert!(snapshot.explicit_scissor_rect.is_none());
    assert!(snapshot.effective_scissor_rect.is_none());
    assert!(snapshot.uv_bounds_bits.is_some());

    // The strict whole-graph adapter already knows TextureComposite; this
    // must remain a complete structural snapshot rather than an unchecked
    // pass hidden behind the common Image/SVG compiler implementation.
    let _ = strict_paint_snapshot(&mut graph, PaintParityConfig::default());
}

#[test]
fn compiler_accepts_only_svg_decoration_then_payload_grammar() {
    let undecorated = compiler_svg_test_artifact(false);
    assert!(undecorated.ops.len() == 1);
    assert!(
        compiled_whole_frame_graph(&undecorated)
            .pass_descriptors()
            .len()
            > 1
    );

    let decorated = compiler_svg_test_artifact(true);
    assert!(matches!(
        decorated.ops.last(),
        Some(PaintOp::PreparedSvg(_))
    ));
    assert!(
        compiled_whole_frame_graph(&decorated)
            .pass_descriptors()
            .len()
            > 1
    );

    let mut fill_only = decorated.clone();
    fill_only.ops.remove(1);
    fill_only.chunks[0].op_range = 0..fill_only.ops.len();
    refresh_svg_standard_draw_rect_identity(&mut fill_only);
    assert!(
        compiled_whole_frame_graph(&fill_only)
            .pass_descriptors()
            .len()
            > 1,
        "fill-only SVG decoration is a valid grammar prefix"
    );

    let mut border_only = decorated.clone();
    let PaintOp::DrawRect(border) = border_only.ops.remove(1) else {
        panic!("decorated fixture second op must be border")
    };
    border_only.ops.remove(0);
    border_only.ops.insert(0, PaintOp::DrawRect(border));
    border_only.chunks[0].op_range = 0..border_only.ops.len();
    assert_eq!(
        compiled_whole_frame_graph(&border_only)
            .pass_descriptors()
            .len(),
        1,
        "border-only SVG decoration is not a valid grammar prefix"
    );

    let mut payload_not_last = compiler_svg_test_artifact(false);
    payload_not_last.ops.push(PaintOp::DrawRect(DrawRectOp {
        params: RectPassParams::default(),
        mode: crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly,
    }));
    payload_not_last.chunks[0].op_range = 0..payload_not_last.ops.len();
    assert_eq!(
        compiled_whole_frame_graph(&payload_not_last)
            .pass_descriptors()
            .len(),
        1,
        "PreparedSvg must be the final and unique content payload"
    );

    let mut wrong_payload_type = compiler_svg_test_artifact(false);
    let PaintOp::PreparedSvg(svg) = wrong_payload_type.ops.remove(0) else {
        unreachable!()
    };
    wrong_payload_type
        .ops
        .push(PaintOp::PreparedImage(PreparedImageOp {
            params: svg.params,
            upload: svg.upload,
        }));
    assert_eq!(
        compiled_whole_frame_graph(&wrong_payload_type)
            .pass_descriptors()
            .len(),
        1,
        "SvgContent must not accept a PreparedImage payload"
    );
}

#[test]
fn compiler_rejects_svg_identity_drift_and_wrong_asset_namespace() {
    let valid = compiler_svg_test_artifact(false);

    let mut drift = valid.clone();
    let PaintPayloadIdentity::Svg(mut identity, decoration) =
        drift.chunks[0].payload_identity.clone()
    else {
        panic!("SVG fixture identity")
    };
    identity.opacity_bits ^= 1;
    drift.chunks[0].payload_identity = PaintPayloadIdentity::Svg(identity, decoration);
    assert_eq!(
        compiled_whole_frame_graph(&drift).pass_descriptors().len(),
        1
    );

    let mut wrong_namespace = valid;
    let PaintOp::PreparedSvg(prepared) = &mut wrong_namespace.ops[0] else {
        panic!("SVG fixture payload")
    };
    prepared.upload.id = crate::view::sampled_texture::SampledTextureId::Image(
        crate::view::sampled_texture::ImageAssetId::for_test(77),
    );
    assert_eq!(
        compiled_whole_frame_graph(&wrong_namespace)
            .pass_descriptors()
            .len(),
        1,
        "SVG content must never accept an Image asset id"
    );
}

#[test]
fn compiler_rejects_every_unsupported_svg_composite_input_and_property() {
    let valid = compiler_svg_test_artifact(false);
    let mut cases = Vec::new();

    let mut invalid = valid.clone();
    let PaintOp::PreparedSvg(op) = &mut invalid.ops[0] else {
        unreachable!()
    };
    op.params.source_is_premultiplied = true;
    cases.push(("premultiplied source", invalid));

    let mut invalid = valid.clone();
    let PaintOp::PreparedSvg(op) = &mut invalid.ops[0] else {
        unreachable!()
    };
    op.params.use_mask = true;
    cases.push(("mask", invalid));

    let mut invalid = valid.clone();
    let PaintOp::PreparedSvg(op) = &mut invalid.ops[0] else {
        unreachable!()
    };
    op.params.mask_uv_bounds = Some([0.0, 0.0, 1.0, 1.0]);
    cases.push(("mask UV", invalid));

    let mut invalid = valid.clone();
    let PaintOp::PreparedSvg(op) = &mut invalid.ops[0] else {
        unreachable!()
    };
    op.params.quad_positions = Some([[0.0; 2]; 4]);
    cases.push(("quad", invalid));

    let mut invalid = valid.clone();
    let PaintOp::PreparedSvg(op) = &mut invalid.ops[0] else {
        unreachable!()
    };
    op.params.scissor_rect = Some([0, 0, 1, 1]);
    cases.push(("op scissor", invalid));

    let mut invalid = valid.clone();
    let PaintOp::PreparedSvg(op) = &mut invalid.ops[0] else {
        unreachable!()
    };
    op.params.uv_bounds = None;
    cases.push(("missing source UV", invalid));

    let mut invalid = valid.clone();
    let PaintOp::PreparedSvg(op) = &mut invalid.ops[0] else {
        unreachable!()
    };
    op.upload.format = wgpu::TextureFormat::Rgba8Unorm;
    cases.push(("non-sRGB upload", invalid));

    let mut invalid = valid.clone();
    invalid.chunks[0].properties.transform = Some(TransformNodeId(NodeKey::null()));
    cases.push(("transform property", invalid));

    let mut invalid = valid.clone();
    invalid.chunks[0].properties.clip = Some(ClipNodeId {
        owner: invalid.chunks[0].owner,
        role: ClipNodeRole::SelfClip,
    });
    cases.push(("clip property", invalid));

    let mut invalid = valid;
    invalid.chunks[0].properties.scroll = Some(
        crate::view::compositor::property_tree::ScrollNodeId(NodeKey::null()),
    );
    cases.push(("scroll property", invalid));

    for (name, invalid) in cases {
        assert_eq!(
            compiled_whole_frame_graph(&invalid)
                .pass_descriptors()
                .len(),
            1,
            "unsupported SVG {name} must fail closed"
        );
    }
}

#[test]
fn compiler_prevalidates_late_invalid_svg_before_emitting_earlier_svg() {
    let mut artifact = compiler_svg_test_artifact(false);
    let PaintOp::PreparedSvg(mut invalid_op) = artifact.ops[0].clone() else {
        panic!("SVG fixture payload")
    };
    invalid_op.upload.pixels = Arc::from([1_u8, 2, 3]);
    let mut key_arena = NodeArena::new();
    let second_owner = key_arena.insert(Node::new(Box::new(Element::new_with_id(
        1001, 0.0, 0.0, 1.0, 1.0,
    ))));
    let start = artifact.ops.len();
    artifact.ops.push(PaintOp::PreparedSvg(invalid_op));
    let mut late = artifact.chunks[0].clone();
    late.id.owner = second_owner;
    late.owner = second_owner;
    late.op_range = start..artifact.ops.len();
    artifact.chunks.push(late);

    assert_eq!(
        compiled_whole_frame_graph(&artifact)
            .pass_descriptors()
            .len(),
        1,
        "late invalid SVG must leave only the pre-existing clear pass"
    );
}
