use super::*;

#[test]
fn outer_shadow_artifact_owns_ordered_fractional_payload_and_strict_pass_sequence() {
    let (arena, root, properties, generations) =
        prepared_shadow_leaf(0x6d10, 1.0, two_outer_shadows(), true);
    let (artifact, eligibility) =
        whole_frame_artifact(&arena, &[root], &properties, &generations);
    assert!(eligibility.eligible);
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
    assert_eq!(shadows[0].mesh.vertices[0], [10.0, 21.0]);
    assert!(shadows.iter().all(|shadow| shadow.has_canonical_identity()));

    drop(arena);
    let mut graph = compiled_whole_frame_graph(&artifact);
    let snapshot = graph.test_compile_snapshot().unwrap();
    let payloads = snapshot.pass_payloads();
    assert!(
        matches!(payloads, [
        FramePassTestPayload::Clear(_),
        FramePassTestPayload::ShadowFill(_),
        FramePassTestPayload::Clear(_),
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
    assert_eq!(
        shadow_fills[0].color_bits,
        Color::rgb(220, 30, 20).to_rgba_f32().map(f32::to_bits)
    );
    assert_eq!(
        shadow_fills[1].color_bits,
        Color::rgb(20, 40, 220).to_rgba_f32().map(f32::to_bits)
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
fn outer_shadow_owner_with_two_children_records_before_children_and_matches_legacy() {
    let (arena, root, first, second, properties, generations) =
        prepared_shadow_owner_tree(0x6d30, 1.0);
    let metadata = record_coverage_manifest(
        &arena,
        &[root],
        false,
        true,
        CoverageRecordingMode::MetadataOnly,
        &properties,
        &generations,
    );
    let full = record_coverage_manifest(
        &arena,
        &[root],
        false,
        true,
        CoverageRecordingMode::FullArtifact,
        &properties,
        &generations,
    );
    assert!(metadata.validation_errors.is_empty());
    assert!(full.validation_errors.is_empty());
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

    let (artifact, eligibility) =
        whole_frame_artifact(&arena, &[root], &properties, &generations);
    assert!(eligibility.eligible);
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

    let (legacy_arena, legacy_root, _, _, _, _) = prepared_shadow_owner_tree(0x6d30, 1.0);
    assert_eq!(
        compiled_whole_frame_graph(&artifact).pass_descriptors(),
        legacy_roots_graph(legacy_arena, &[legacy_root]).pass_descriptors()
    );
}

#[test]
fn outer_shadow_artifact_respects_baked_and_root_group_opacity_authority() {
    let (arena, root, properties, generations) =
        prepared_shadow_leaf(0x6d11, 0.4, two_outer_shadows(), false);
    let (baked, _) = whole_frame_artifact(&arena, &[root], &properties, &generations);
    assert!(
        baked
            .ops
            .iter()
            .filter_map(|op| match op {
                PaintOp::PreparedShadow(shadow) => Some(shadow.params.opacity),
                _ => None,
            })
            .all(|opacity| opacity.to_bits() == 0.4_f32.to_bits())
    );

    let (group, _) = root_group_artifact(&arena, &[root], &properties, &generations);
    group.ops.iter().for_each(assert_neutral_opacity);
    let mut graph = compiled_whole_frame_graph(&group);
    let snapshot = graph.test_compile_snapshot().unwrap();
    assert!(snapshot.pass_payloads().iter().any(|payload| {
        matches!(payload, FramePassTestPayload::CompositeLayer(composite)
            if composite.opacity_bits == 0.4_f32.to_bits())
    }));
    assert!(
        snapshot
            .pass_payloads()
            .iter()
            .filter_map(|payload| match payload {
                FramePassTestPayload::ShadowFill(fill) => Some(fill.color_bits[3]),
                _ => None,
            })
            .all(|alpha| alpha == 1.0_f32.to_bits())
    );
}

#[test]
fn outer_shadow_owner_opacity_is_applied_once_and_metadata_detects_shadow_drift() {
    let (arena, root, _, _, properties, generations) = prepared_shadow_owner_tree(0x6d33, 0.4);
    let (baked, _) = whole_frame_artifact(&arena, &[root], &properties, &generations);
    assert!(
        baked
            .ops
            .iter()
            .filter_map(|op| match op {
                PaintOp::PreparedShadow(shadow) => Some(shadow.params.opacity),
                _ => None,
            })
            .all(|opacity| opacity.to_bits() == 0.4_f32.to_bits())
    );

    let (group, _) = root_group_artifact(&arena, &[root], &properties, &generations);
    group.ops.iter().for_each(assert_neutral_opacity);
    let snapshot = compiled_whole_frame_graph(&group)
        .test_compile_snapshot()
        .unwrap();
    assert_eq!(
        snapshot
            .pass_payloads()
            .iter()
            .filter(|payload| matches!(
                payload,
                FramePassTestPayload::CompositeLayer(composite)
                    if composite.opacity_bits == 0.4_f32.to_bits()
            ))
            .count(),
        1
    );

    let metadata = record_coverage_manifest(
        &arena,
        &[root],
        false,
        true,
        CoverageRecordingMode::MetadataOnly,
        &properties,
        &generations,
    );
    arena
        .get_mut(root)
        .unwrap()
        .element
        .as_any_mut()
        .downcast_mut::<Element>()
        .unwrap()
        .set_box_shadows(vec![
            BoxShadow::new()
                .color(Color::rgb(20, 40, 220))
                .offset_x(-9.0)
                .offset_y(4.5),
            BoxShadow::new()
                .color(Color::rgb(220, 30, 20))
                .offset_x(1.5)
                .offset_y(-2.25),
        ]);
    let full = record_coverage_manifest(
        &arena,
        &[root],
        false,
        true,
        CoverageRecordingMode::FullArtifact,
        &properties,
        &generations,
    );
    assert!(!frame_recorder::canonical_manifest_matches(
        &metadata, &full
    ));
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
        let outcome = record_clip_enabled_frame_artifact(
            &arena,
            &[root],
            &properties,
            &generations,
            RendererMode::Auto,
        )
        .unwrap();
        match boundary {
            ChildBoundary::MalformedTextAreaRun => {
                let FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility) = outcome
                else {
                    panic!("standalone TextArea internals must fail closed")
                };
                assert!(eligibility.reasons.contains(
                    &FrameArtifactFallbackReason::LegacyBoundary(
                        LegacyPaintReason::UnknownHost,
                    ),
                ));
                assert_eq!(take_full_artifact_record_count(), 0);
            }
            ChildBoundary::Deferred => {
                let FrameArtifactRecordOutcome::Artifact {
                    artifact,
                    eligibility,
                } = outcome
                else {
                    panic!("supported deferred native children must remain retained")
                };
                assert!(eligibility.eligible);
                assert!(eligibility.reasons.is_empty());
                assert_eq!(
                    artifact
                        .ops
                        .iter()
                        .filter(|op| matches!(op, PaintOp::PreparedShadow(_)))
                        .count(),
                    2,
                    "outer shadows remain owned by the retained parent",
                );
                assert_eq!(
                    take_full_artifact_record_count(),
                    2,
                    "the frame root and deferred child each record one sealed artifact",
                );
            }
        }
    }
}

#[test]
fn nonzero_blur_outer_shadow_is_auto_recordable_and_matches_legacy_graph() {
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
        let outcome = record_clip_enabled_frame_artifact(
            &arena,
            &[root],
            &properties,
            &generations,
            RendererMode::Auto,
        )
        .expect("automatic artifact recording never forces");
        let FrameArtifactRecordOutcome::Artifact {
            artifact,
            eligibility,
        } = outcome
        else {
            panic!("finite non-inset blur shadow must remain on retained auto")
        };
        assert!(eligibility.eligible);
        let prepared = artifact
            .ops
            .iter()
            .find_map(|op| match op {
                PaintOp::PreparedShadow(shadow) => Some(shadow),
                _ => None,
            })
            .expect("blur shadow must be frozen into the artifact");
        assert_eq!(prepared.params.blur_radius.to_bits(), blur.to_bits());
        assert!(prepared.has_canonical_identity());

        let mut tampered = artifact.clone();
        let PaintOp::PreparedShadow(tampered_shadow) = tampered
            .ops
            .iter_mut()
            .find(|op| matches!(op, PaintOp::PreparedShadow(_)))
            .unwrap()
        else {
            unreachable!()
        };
        tampered_shadow.params.blur_radius += 1.0;
        assert_eq!(
            compiled_whole_frame_graph(&tampered)
                .pass_descriptors()
                .len(),
            1,
            "blur drift must fail closed before artifact pass emission"
        );
        drop(arena);

        let artifact_graph = compiled_whole_frame_graph(&artifact);
        let (legacy_arena, legacy_root, _, _) =
            prepared_shadow_leaf(id, 1.0, vec![shadow()], true);
        let legacy_graph = legacy_roots_graph(legacy_arena, &[legacy_root]);
        assert_eq!(
            artifact_graph.pass_descriptors(),
            legacy_graph.pass_descriptors()
        );
        assert_eq!(
            artifact_graph
                .pass_descriptors()
                .iter()
                .filter(|pass| pass.name.ends_with("blur_module::BlurStagePass"))
                .count(),
            expected_blur_stages,
            "retained and legacy must share the physical blur threshold"
        );
    }
}

#[test]
fn inset_blur_shadow_is_auto_recordable_and_matches_legacy_mask_graph() {
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
    let outcome = record_clip_enabled_frame_artifact(
        &arena,
        &[root],
        &properties,
        &generations,
        RendererMode::Auto,
    )
    .expect("automatic artifact recording never forces");
    let FrameArtifactRecordOutcome::Artifact {
        artifact,
        eligibility,
    } = outcome
    else {
        panic!("finite inset blur shadow must remain on retained auto")
    };
    assert!(eligibility.eligible);
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
    assert!(prepared.has_canonical_identity());

    let mut tampered = artifact.clone();
    let PaintOp::PreparedShadow(tampered_shadow) = tampered
        .ops
        .iter_mut()
        .find(|op| matches!(op, PaintOp::PreparedShadow(_)))
        .unwrap()
    else {
        unreachable!()
    };
    tampered_shadow.params.clip_to_geometry = false;
    assert_eq!(
        compiled_whole_frame_graph(&tampered)
            .pass_descriptors()
            .len(),
        1,
        "clip-to-geometry drift must fail closed before artifact pass emission"
    );
    drop(arena);

    let artifact_graph = compiled_whole_frame_graph(&artifact);
    let (legacy_arena, legacy_root, _, _) =
        prepared_shadow_leaf(0x6d21, 1.0, vec![shadow()], true);
    let legacy_graph = legacy_roots_graph(legacy_arena, &[legacy_root]);
    assert_eq!(
        artifact_graph.pass_descriptors(),
        legacy_graph.pass_descriptors()
    );
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

#[test]
fn outer_shadow_artifact_compiler_fails_closed() {
    let (arena, root, properties, generations) =
        prepared_shadow_leaf(0x6d22, 1.0, two_outer_shadows(), false);
    let (mut artifact, _) = whole_frame_artifact(&arena, &[root], &properties, &generations);
    let PaintOp::PreparedShadow(late) = artifact
        .ops
        .iter_mut()
        .rev()
        .find(|op| matches!(op, PaintOp::PreparedShadow(_)))
        .unwrap()
    else {
        unreachable!()
    };
    late.mesh.indices[0] = u32::MAX;
    assert_eq!(
        compiled_whole_frame_graph(&artifact)
            .pass_descriptors()
            .len(),
        1,
        "late invalid shadow must emit zero artifact passes"
    );

    let (arena, root, properties, generations) =
        prepared_shadow_leaf(0x6d23, 1.0, two_outer_shadows(), false);
    let (mut reordered, _) = whole_frame_artifact(&arena, &[root], &properties, &generations);
    let fill_index = reordered
        .ops
        .iter()
        .position(|op| matches!(op, PaintOp::DrawRect(_)))
        .unwrap();
    reordered.ops.swap(0, fill_index);
    assert_eq!(
        compiled_whole_frame_graph(&reordered)
            .pass_descriptors()
            .len(),
        1,
        "shadow after FillOnly must reject before the first artifact pass"
    );
}

#[test]
fn outer_shadow_artifact_preflight_and_exact_clip_cases() {
    for (case, shadow) in [
        ("nan-blur", BoxShadow::new().blur(f32::NAN)),
        ("infinite-offset", BoxShadow::new().offset_x(f32::INFINITY)),
    ] {
        let (arena, root, properties, generations) =
            prepared_shadow_leaf(0x6d24, 1.0, vec![shadow], false);
        let _ = take_full_artifact_record_count();
        let error = record_property_neutral_frame_artifact(
            &arena,
            &[root],
            &properties,
            &generations,
            RendererMode::ForcedForTests,
        )
        .unwrap_err();
        assert!(
            error
                .reasons
                .contains(&FrameArtifactFallbackReason::LegacyBoundary(
                    LegacyPaintReason::BoxShadow
                )),
            "{case}: {error:?}"
        );
        assert_eq!(take_full_artifact_record_count(), 0, "{case}");
    }
    let mut clipped = Element::new_with_id(0x6d23, 10.25, 20.75, 80.0, 40.0);
    let mut style = Style::new();
    style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::rgb(40, 80, 160)),
    );
    style.set_box_shadow(two_outer_shadows());
    style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .left(Length::px(10.0))
                .top(Length::px(20.0))
                .clip(ClipMode::AnchorParent),
        ),
    );
    clipped.apply_style(style);
    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(clipped));
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    let (properties, generations) = sync_identity(&arena, &[root]);
    let _ = take_full_artifact_record_count();
    let outcome = record_frame_artifact(
        &arena,
        &[root],
        &properties,
        &generations,
        RendererMode::ForcedForTests,
    )
    .expect("exact single-owner self clip + canonical outer shadow records");
    let FrameArtifactRecordOutcome::Artifact {
        artifact,
        eligibility,
    } = outcome
    else {
        panic!("exact clipped outer shadow must not fall back")
    };
    assert!(eligibility.eligible);
    assert_eq!(artifact.chunks.len(), 1);
    assert!(matches!(
        artifact.ops.first(),
        Some(PaintOp::PreparedShadow(_))
    ));
    assert!(
        compiled_whole_frame_graph(&artifact)
            .pass_descriptors()
            .len()
            > 1
    );
    assert_eq!(take_full_artifact_record_count(), 1);

    let (mut arena, root, _, _) = prepared_shadow_leaf(0x6d24, 1.0, two_outer_shadows(), false);
    let mut rounded = Style::new();
    rounded.set_border_radius(BorderRadius::uniform(Length::px(12.0)));
    arena
        .get_mut(root)
        .unwrap()
        .element
        .as_any_mut()
        .downcast_mut::<Element>()
        .unwrap()
        .apply_style(rounded);
    commit_child(
        &mut arena,
        root,
        Box::new(leaf_element(0x6d25, Color::rgb(20, 180, 40), 1.0, false)),
    );
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    let (properties, generations) = sync_identity(&arena, &[root]);
    let _ = take_full_artifact_record_count();
    let outcome = record_property_neutral_frame_artifact(
        &arena,
        &[root],
        &properties,
        &generations,
        RendererMode::ForcedForTests,
    )
    .expect("rounded child clip records as a retained mask scope");
    let FrameArtifactRecordOutcome::Artifact {
        artifact,
        eligibility,
    } = outcome
    else {
        panic!("rounded child clip must not fall back")
    };
    assert!(eligibility.eligible);
    let mask_chunks = artifact
        .chunks
        .iter()
        .filter(|chunk| chunk.id.slot == RETAINED_CHILD_MASK_SLOT)
        .collect::<Vec<_>>();
    assert_eq!(mask_chunks.len(), 2);
    assert_eq!(mask_chunks[0].id.phase, PaintNodePhase::BeforeChildren);
    assert_eq!(mask_chunks[1].id.phase, PaintNodePhase::AfterChildren);
    assert_eq!(artifact.chunks[0].owner, root);
    assert_eq!(artifact.chunks[1].id.slot, RETAINED_CHILD_MASK_SLOT);
    assert_eq!(
        artifact.chunks[2].owner,
        arena.get(root).unwrap().element.children()[0]
    );
    assert_eq!(artifact.chunks[3].id.slot, RETAINED_CHILD_MASK_SLOT);

    use crate::view::render_pass::draw_rect_pass::RectStencilModeTestSnapshot;
    let snapshots = compiled_whole_frame_graph(&artifact).test_rect_pass_snapshots();
    assert!(snapshots.iter().any(|snapshot| {
        snapshot.stencil_mode == RectStencilModeTestSnapshot::Increment { clip_id: 0 }
            && !snapshot.color_write_enabled
    }));
    assert!(snapshots.iter().any(|snapshot| {
        snapshot.pass_context.stencil_clip_id == Some(1) && snapshot.color_write_enabled
    }));
    assert!(snapshots.iter().any(|snapshot| {
        snapshot.stencil_mode == RectStencilModeTestSnapshot::Decrement { clip_id: 1 }
            && !snapshot.color_write_enabled
    }));
    assert_eq!(take_full_artifact_record_count(), 2);
}
