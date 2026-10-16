use super::*;

#[test]
fn outer_shadow_artifact_owns_ordered_fractional_payload_and_strict_pass_sequence() {
    let (arena, root, properties, generations) =
        prepared_shadow_leaf(0x6d10, 1.0, two_outer_shadows(), true);
    let artifact = whole_frame_artifact(&arena, &[root], &properties, &generations);
    let shadows = artifact
        .ops
        .iter()
        .filter_map(|op| match op {
            PaintOp::PreparedShadow(shadow) => Some(shadow),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(shadows.len(), 2);
    assert_eq!(
        shadows[0].params.color,
        Color::rgb(220, 30, 20).to_rgba_f32()
    );
    assert_eq!(
        shadows[1].params.color,
        Color::rgb(20, 40, 220).to_rgba_f32()
    );
    assert_eq!([shadows[0].shape.x, shadows[0].shape.y], [10.0, 21.0]);

    drop(arena);
    let mut graph = compiled_whole_frame_graph(&artifact);
    let snapshot = graph.test_compile_snapshot().unwrap();
    let payloads = snapshot.pass_payloads();
    assert!(
        matches!(payloads, [
        FramePassTestPayload::ShadowFill(_),
        FramePassTestPayload::ShadowFill(_),
        FramePassTestPayload::Clear(_),
        FramePassTestPayload::TextureComposite(_),
        FramePassTestPayload::TextureComposite(_),
        FramePassTestPayload::DrawRect(fill),
        FramePassTestPayload::DrawRect(border),
    ] if fill.mode == crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly
        && border.mode == crate::view::render_pass::draw_rect_pass::RectRenderMode::BorderOnly),
        "payloads={payloads:?}"
    );
    let shadow_fills = payloads
        .iter()
        .filter_map(|payload| match payload {
            FramePassTestPayload::ShadowFill(fill) => Some(fill),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(shadow_fills.len(), 2);
    let mut cleared_fills = 0;
    for pass in &graph.compiled_graph().unwrap().passes {
        if pass.descriptor.name.contains("ShadowFillPass") {
            let crate::view::frame_graph::PassDetails::Graphics(graphics) =
                &pass.descriptor.details
            else {
                panic!("shadow fill must be graphics");
            };
            assert_eq!(graphics.color_attachments.len(), 1);
            assert_eq!(
                graphics.color_attachments[0].load_op,
                crate::view::frame_graph::AttachmentLoadOp::Clear([0.; 4])
            );
            cleared_fills += 1;
        }
    }
    assert_eq!(
        cleared_fills, 2,
        "each fill clears its own template attachment"
    );
    // Templates hold color-independent coverage; each shadow tints its own.
    assert!(
        shadow_fills
            .iter()
            .all(|fill| fill.color_bits == [1.0_f32; 4].map(f32::to_bits))
    );
    let tints = payloads
        .iter()
        .filter_map(|payload| match payload {
            FramePassTestPayload::TextureComposite(composite) => composite.nine_patch_tint_bits,
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        tints,
        vec![
            Color::rgb(220, 30, 20).to_rgba_f32().map(f32::to_bits),
            Color::rgb(20, 40, 220).to_rgba_f32().map(f32::to_bits),
        ]
    );
    let first_rect = payloads
        .iter()
        .position(|payload| matches!(payload, FramePassTestPayload::DrawRect(_)))
        .unwrap();
    assert!(
        payloads[..first_rect]
            .iter()
            .any(|payload| { matches!(payload, FramePassTestPayload::TextureComposite(_)) })
    );
    let rect_modes = payloads
        .iter()
        .filter_map(|payload| match payload {
            FramePassTestPayload::DrawRect(rect) => Some(rect.mode),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        rect_modes,
        vec![
            crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly,
            crate::view::render_pass::draw_rect_pass::RectRenderMode::BorderOnly,
        ]
    );
}

#[test]
fn outer_shadow_owner_with_two_children_records_before_children() {
    let (arena, root, first, second, properties, generations) =
        prepared_shadow_owner_tree(0x6d30, 1.0);
    let metadata = record_coverage_manifest(
        &arena,
        &[root],
        true,
        CoverageRecordingMode::MetadataOnly,
        &properties,
        &generations,
    );
    let full = record_coverage_manifest(
        &arena,
        &[root],
        true,
        CoverageRecordingMode::FullArtifact,
        &properties,
        &generations,
    );
    assert!(frame_recorder::canonical_manifest_matches(&metadata, &full));
    assert_eq!(
        metadata
            .items
            .iter()
            .map(|item| match item {
                PaintCoverageItem::ArtifactChunk { chunk, .. } => {
                    (chunk.owner, chunk.id.role, chunk.id.phase, chunk.id.slot)
                }
                other => panic!("unexpected owner-tree coverage: {other:?}"),
            })
            .collect::<Vec<_>>(),
        vec![
            (
                root,
                PaintChunkRole::SelfDecoration,
                PaintNodePhase::BeforeChildren,
                0,
            ),
            (
                first,
                PaintChunkRole::SelfDecoration,
                PaintNodePhase::BeforeChildren,
                0,
            ),
            (
                second,
                PaintChunkRole::SelfDecoration,
                PaintNodePhase::BeforeChildren,
                0,
            ),
        ]
    );
    let PaintCoverageItem::ArtifactChunk {
        chunk: owner_chunk, ..
    } = &metadata.items[0]
    else {
        unreachable!()
    };
    assert!(matches!(
        &owner_chunk.payload_identity,
        PaintPayloadIdentity::PreparedShadows(shadows, decoration)
            if shadows.len() == 2 && decoration.len() == 2
    ));

    let artifact = whole_frame_artifact(&arena, &[root], &properties, &generations);
    assert_eq!(
        artifact
            .chunks
            .iter()
            .map(|chunk| chunk.owner)
            .collect::<Vec<_>>(),
        vec![root, first, second]
    );
    let first_non_shadow = artifact
        .ops
        .iter()
        .position(|op| !matches!(op, PaintOp::PreparedShadow(_)))
        .unwrap();
    assert_eq!(first_non_shadow, 2);
}

#[test]
fn outer_shadow_owner_native_child_boundaries_record_retained_artifact() {
    #[derive(Clone, Copy)]
    enum ChildBoundary {
        MalformedTextAreaRun,
        Deferred,
    }

    for (index, boundary) in [ChildBoundary::MalformedTextAreaRun, ChildBoundary::Deferred]
        .into_iter()
        .enumerate()
    {
        let id = 0x6d40 + index as u64 * 0x10;
        let (mut arena, root, _, _) = prepared_shadow_leaf(id, 1.0, two_outer_shadows(), false);
        let _child = match boundary {
            ChildBoundary::MalformedTextAreaRun => commit_child(
                &mut arena,
                root,
                Box::new(TextAreaTextRun::new("native".to_string(), 0..6)),
            ),
            ChildBoundary::Deferred => {
                let mut child = leaf_element(id + 1, Color::rgb(20, 180, 40), 1.0, false);
                let mut style = Style::new();
                style.insert(
                    PropertyId::Position,
                    ParsedValue::Position(Position::absolute().clip(ClipMode::Viewport)),
                );
                child.apply_style(style);
                commit_child(&mut arena, root, Box::new(child))
            }
        };
        let (measure, place) = constraints();
        measure_and_place(&mut arena, root, measure, place);
        let (properties, generations) = sync_identity(&arena, &[root]);
        take_full_artifact_record_count();
        let artifact =
            record_surface_dag_frame_artifact(&arena, &[root], &properties, &generations);
        assert_eq!(
            artifact
                .ops
                .iter()
                .filter(|op| matches!(op, PaintOp::PreparedShadow(_)))
                .count(),
            2,
            "outer shadows remain owned by the retained parent",
        );
        if matches!(boundary, ChildBoundary::Deferred) {
            assert_eq!(
                take_full_artifact_record_count(),
                2,
                "the frame root and deferred child each record one sealed artifact",
            );
        }
    }
}

#[test]
fn nonzero_blur_outer_shadow_is_auto_recordable() {
    for (index, blur, expected_blur_stages) in [(0_u64, 0.000_5_f32, 0_usize), (1, 8.5, 2)] {
        let shadow = || {
            BoxShadow::new()
                .color(Color::rgb(30, 90, 210))
                .offset_x(2.5)
                .offset_y(-1.25)
                .blur(blur)
                .spread(1.75)
        };
        let id = 0x6d1f + index;
        let (arena, root, properties, generations) =
            prepared_shadow_leaf(id, 1.0, vec![shadow()], true);
        let outcome = record_surface_dag_frame_artifact(&arena, &[root], &properties, &generations);
        let artifact = outcome;
        let prepared = artifact
            .ops
            .iter()
            .find_map(|op| match op {
                PaintOp::PreparedShadow(shadow) => Some(shadow),
                _ => None,
            })
            .expect("blur shadow must be frozen into the artifact");
        assert_eq!(prepared.params.blur_radius.to_bits(), blur.to_bits());

        drop(arena);

        let artifact_graph = compiled_whole_frame_graph(&artifact);
        assert_eq!(
            artifact_graph
                .pass_descriptors()
                .iter()
                .filter(|pass| pass.name.ends_with("blur_module::BlurStagePass"))
                .count(),
            expected_blur_stages,
            "blur stages follow the physical blur threshold"
        );
    }
}

#[test]
fn inset_blur_shadow_is_auto_recordable_with_a_mask_fill() {
    let shadow = || {
        BoxShadow::new()
            .color(Color::rgb(180, 40, 90))
            .offset_x(-2.0)
            .offset_y(3.0)
            .blur(7.25)
            .spread(1.5)
            .inset(true)
    };
    let (arena, root, properties, generations) =
        prepared_shadow_leaf(0x6d21, 1.0, vec![shadow()], true);
    let outcome = record_surface_dag_frame_artifact(&arena, &[root], &properties, &generations);
    let artifact = outcome;
    let prepared = artifact
        .ops
        .iter()
        .find_map(|op| match op {
            PaintOp::PreparedShadow(shadow) => Some(shadow),
            _ => None,
        })
        .expect("inset blur shadow must be frozen into the artifact");
    assert!(prepared.params.clip_to_geometry);
    assert_eq!(prepared.params.blur_radius.to_bits(), 7.25_f32.to_bits());

    drop(arena);

    let artifact_graph = compiled_whole_frame_graph(&artifact);
    assert_eq!(
        artifact_graph
            .test_graphics_passes::<crate::view::render_pass::shadow_module::ShadowFillPass>()
            .len(),
        2,
        "inset shadow requires one colored shadow fill and one geometry-mask fill"
    );
    assert_eq!(
        artifact_graph
            .pass_descriptors()
            .iter()
            .filter(|pass| pass.name.ends_with("blur_module::BlurStagePass"))
            .count(),
        2
    );
}
