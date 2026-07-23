use super::*;

#[test]
fn nested_scroll_media_localizer_preserves_source_payload_and_rejects_tampering() {
    for kind in [NestedMediaLeafKind::Image, NestedMediaLeafKind::Svg] {
        let (arena, outer, _inner, _leaf, properties, generations) =
            nested_scroll_media_fixture(kind);
        let scene =
            compile_nested_scroll_fixture_parts(&arena, outer, &properties, &generations);
        let scaffold = scene.plan.nested_scroll_planning_scaffold().unwrap();
        let [outer_boundary, _] = scaffold.boundaries.as_slice() else {
            unreachable!()
        };
        let super::super::super::frame_plan::NestedScrollSceneScheduledStep::ContentReceiver(receiver) =
            &scaffold.schedule.steps[2]
        else {
            unreachable!()
        };
        let RetainedPropertyScrollGenericAuthority::NestedScrollCompiler(contract) =
            &scene.transaction.generic_authority
        else {
            unreachable!()
        };
        let raw = receiver.artifact.artifact();
        let localize = |artifact: &super::super::super::PaintArtifact| {
            validate_nested_scroll_content_artifact(
                artifact,
                scaffold.admission.content_leaf,
                outer_boundary.scroll.id,
                outer_boundary.contents_clip,
                contract.compiled.leaf_recorded_bounds_bits,
                contract.compiled.leaf_source_bounds_bits,
            )
        };
        let localized = localize(raw)
            .unwrap_or_else(|| panic!("{kind:?} exact payload must localize canonically"));
        let localized_artifact = localized.artifact_for_test();

        let raw_rect = raw.ops.iter().find_map(|op| match op {
            super::super::super::PaintOp::DrawRect(rect) => Some(rect),
            _ => None,
        });
        let localized_rect = localized_artifact.ops.iter().find_map(|op| match op {
            super::super::super::PaintOp::DrawRect(rect) => Some(rect),
            _ => None,
        });
        let (raw_rect, localized_rect) = (raw_rect.unwrap(), localized_rect.unwrap());
        let recorded = contract
            .compiled
            .leaf_recorded_bounds_bits
            .map(f32::from_bits);
        let local = contract
            .compiled
            .leaf_source_bounds_bits
            .map(f32::from_bits);
        assert_eq!(
            localized_rect.params.position.map(f32::to_bits),
            [
                raw_rect.params.position[0] + local[0] - recorded[0],
                raw_rect.params.position[1] + local[1] - recorded[1],
            ]
            .map(f32::to_bits),
            "{kind:?} decoration position"
        );

        let (raw_params, raw_upload, localized_params, localized_upload) = match (
            raw.ops.last().unwrap(),
            localized_artifact.ops.last().unwrap(),
        ) {
            (
                super::super::super::PaintOp::PreparedImage(raw),
                super::super::super::PaintOp::PreparedImage(localized),
            ) => (
                &raw.params,
                &raw.upload,
                &localized.params,
                &localized.upload,
            ),
            (
                super::super::super::PaintOp::PreparedSvg(raw),
                super::super::super::PaintOp::PreparedSvg(localized),
            ) => (
                &raw.params,
                &raw.upload,
                &localized.params,
                &localized.upload,
            ),
            _ => panic!("{kind:?} role and prepared op must agree"),
        };
        assert_eq!(
            localized_params.bounds.map(f32::to_bits),
            [
                raw_params.bounds[0] + local[0] - recorded[0],
                raw_params.bounds[1] + local[1] - recorded[1],
                raw_params.bounds[2],
                raw_params.bounds[3],
            ]
            .map(f32::to_bits),
            "{kind:?} destination bounds"
        );
        assert_eq!(
            localized_params.uv_bounds.map(|uv| uv.map(f32::to_bits)),
            raw_params.uv_bounds.map(|uv| uv.map(f32::to_bits)),
            "{kind:?} UVs are source identity"
        );
        assert_eq!(localized_upload.id, raw_upload.id, "{kind:?} namespace");
        assert_eq!(
            localized_upload.generation, raw_upload.generation,
            "{kind:?} generation"
        );
        assert_eq!(
            localized_upload.extent(),
            raw_upload.extent(),
            "{kind:?} extent"
        );
        assert!(
            std::sync::Arc::ptr_eq(&localized_upload.pixels, &raw_upload.pixels),
            "{kind:?} localizer must retain the frozen upload Arc"
        );

        assert!(
            validate_scroll_scene_content_artifact(
                localized_artifact.clone(),
                scaffold.admission.content_leaf,
                contract.compiled.leaf_source_bounds_bits,
            )
            .is_none(),
            "{kind:?} must not widen the single-scroll content authority"
        );
        assert!(
            validate_scroll_scene_host_before_artifact(
                localized_artifact.clone(),
                scaffold.admission.content_leaf,
                contract.compiled.leaf_source_bounds_bits,
            )
            .is_none(),
            "{kind:?} must not widen host authority"
        );

        let mut wrong_slot = raw.clone();
        wrong_slot.chunks[0].id.slot = 1;
        assert!(localize(&wrong_slot).is_none(), "{kind:?} active slot");

        let mut bad_range = raw.clone();
        bad_range.chunks[0].op_range.end = bad_range.ops.len() + 1;
        assert!(localize(&bad_range).is_none(), "{kind:?} op range");

        for tamper in 0..4 {
            let mut invalid = raw.clone();
            let (params, upload) = match invalid.ops.last_mut().unwrap() {
                super::super::super::PaintOp::PreparedImage(op) => (&mut op.params, &mut op.upload),
                super::super::super::PaintOp::PreparedSvg(op) => (&mut op.params, &mut op.upload),
                _ => unreachable!(),
            };
            match tamper {
                0 => upload.generation = upload.generation.saturating_add(1),
                1 => upload.pixels = std::sync::Arc::from(upload.pixels.to_vec()),
                2 => {
                    let uv = params.uv_bounds.get_or_insert([0.0, 0.0, 1.0, 1.0]);
                    uv[0] = 0.25;
                }
                3 => {
                    upload.id = match kind {
                        NestedMediaLeafKind::Image => {
                            crate::view::sampled_texture::SampledTextureId::SvgRaster(
                                crate::view::sampled_texture::SvgRasterAssetId::for_test(91),
                            )
                        }
                        NestedMediaLeafKind::Svg => {
                            crate::view::sampled_texture::SampledTextureId::Image(
                                crate::view::sampled_texture::ImageAssetId::for_test(91),
                            )
                        }
                    };
                }
                _ => unreachable!(),
            }
            assert!(
                localize(&invalid).is_none(),
                "{kind:?} tamper variant {tamper} must fail closed"
            );
        }
    }
}

#[test]
fn nested_scroll_text_localizer_preserves_glyph_sources_and_rejects_resealed_tampering() {
    let (arena, outer, _inner, _leaf, properties, generations) = nested_scroll_text_fixture();
    let scene = compile_nested_scroll_fixture_parts(&arena, outer, &properties, &generations);
    let scaffold = scene.plan.nested_scroll_planning_scaffold().unwrap();
    let [outer_boundary, _] = scaffold.boundaries.as_slice() else {
        unreachable!()
    };
    let super::super::super::frame_plan::NestedScrollSceneScheduledStep::ContentReceiver(receiver) =
        &scaffold.schedule.steps[2]
    else {
        unreachable!()
    };
    let RetainedPropertyScrollGenericAuthority::NestedScrollCompiler(contract) =
        &scene.transaction.generic_authority
    else {
        unreachable!()
    };
    let raw = receiver.artifact.artifact();
    let localize = |artifact: &super::super::super::PaintArtifact| {
        validate_nested_scroll_content_artifact(
            artifact,
            scaffold.admission.content_leaf,
            outer_boundary.scroll.id,
            outer_boundary.contents_clip,
            contract.compiled.leaf_recorded_bounds_bits,
            contract.compiled.leaf_source_bounds_bits,
        )
    };
    let localized = localize(raw).expect("exact standalone Text localizes");
    let localized_artifact = localized.artifact_for_test();
    let [super::super::super::PaintOp::PreparedText(raw_text)] = raw.ops.as_slice() else {
        unreachable!()
    };
    let [super::super::super::PaintOp::PreparedText(localized_text)] =
        localized_artifact.ops.as_slice()
    else {
        unreachable!()
    };
    let raw_fragment = raw_text.params.fragments[0];
    let localized_fragment = localized_text.params.fragments[0];
    let local_bounds = contract
        .compiled
        .leaf_source_bounds_bits
        .map(f32::from_bits);
    assert_eq!(
        localized_fragment.origin.map(f32::to_bits),
        [local_bounds[0], local_bounds[1]].map(f32::to_bits)
    );
    assert_eq!(
        localized_fragment.size.map(f32::to_bits),
        raw_fragment.size.map(f32::to_bits)
    );
    assert_eq!(
        localized_text.params.staging_input.scale_factor.to_bits(),
        raw_text.params.staging_input.scale_factor.to_bits()
    );
    assert_eq!(
        localized_text.params.staging_input.glyphs.len(),
        raw_text.params.staging_input.glyphs.len()
    );
    for (raw_glyph, localized_glyph) in raw_text
        .params
        .staging_input
        .glyphs
        .iter()
        .zip(&localized_text.params.staging_input.glyphs)
    {
        assert_eq!(localized_glyph.raster.glyph_id, raw_glyph.raster.glyph_id);
        assert_eq!(
            localized_glyph.raster.font_size.to_bits(),
            raw_glyph.raster.font_size.to_bits()
        );
        assert_eq!(
            localized_glyph.raster.font_data_id,
            raw_glyph.raster.font_data_id
        );
        assert_eq!(
            localized_glyph.raster.font_index,
            raw_glyph.raster.font_index
        );
        assert_eq!(
            localized_glyph.raster.normalized_coords_hash,
            raw_glyph.raster.normalized_coords_hash
        );
        assert_eq!(
            localized_glyph.paint.local_pos.map(f32::to_bits),
            raw_glyph.paint.local_pos.map(f32::to_bits)
        );
        assert_eq!(
            localized_glyph.paint.color.map(f32::to_bits),
            raw_glyph.paint.color.map(f32::to_bits)
        );
        assert_eq!(
            localized_glyph.paint.opacity.to_bits(),
            raw_glyph.paint.opacity.to_bits()
        );
        assert_eq!(localized_glyph.paint.fragment_index, 0);
        assert_eq!(
            localized_glyph.final_paint_pos.map(f32::to_bits),
            [
                localized_fragment.origin[0] + localized_glyph.paint.local_pos[0],
                localized_fragment.origin[1] + localized_glyph.paint.local_pos[1],
            ]
            .map(f32::to_bits)
        );
    }

    assert!(
        validate_scroll_scene_content_artifact(
            localized_artifact.clone(),
            scaffold.admission.content_leaf,
            contract.compiled.leaf_source_bounds_bits,
        )
        .is_none(),
        "Text must not widen the single-scroll content authority"
    );
    assert!(
        validate_scroll_scene_host_before_artifact(
            localized_artifact.clone(),
            scaffold.admission.content_leaf,
            contract.compiled.leaf_source_bounds_bits,
        )
        .is_none(),
        "Text must not widen host authority"
    );

    let reseal = |artifact: &mut super::super::super::PaintArtifact,
                  mutate: &dyn Fn(
        &mut crate::view::render_pass::text_pass::TextPassPreparedParams,
    )| {
        let super::super::super::PaintOp::PreparedText(prepared) = &artifact.ops[0] else {
            unreachable!()
        };
        let mut params = prepared.params.clone();
        mutate(&mut params);
        let prepared = super::super::super::PreparedTextOp::new(params)
            .expect("tamper remains a globally canonical PreparedText");
        artifact.ops[0] = super::super::super::PaintOp::PreparedText(prepared);
        let super::super::super::PaintOp::PreparedText(prepared) = &artifact.ops[0] else {
            unreachable!()
        };
        artifact.chunks[0].payload_identity =
            super::super::super::PaintPayloadIdentity::prepared_texts([prepared]);
    };

    let mut origin = raw.clone();
    reseal(&mut origin, &|params| {
        params.fragments[0].origin[0] += 0.25;
        let origin = params.fragments[0].origin;
        for glyph in &mut params.staging_input.glyphs {
            glyph.final_paint_pos = [
                origin[0] + glyph.paint.local_pos[0],
                origin[1] + glyph.paint.local_pos[1],
            ];
        }
    });
    assert!(localize(&origin).is_none(), "resealed origin/final drift");

    let mut size = raw.clone();
    reseal(&mut size, &|params| params.fragments[0].size[0] += 1.0);
    assert!(localize(&size).is_none(), "resealed fragment size drift");

    let mut scale = raw.clone();
    reseal(&mut scale, &|params| {
        params.staging_input.scale_factor = 2.0
    });
    assert!(localize(&scale).is_none(), "resealed scale drift");

    let mut zero_ops = raw.clone();
    zero_ops.ops.clear();
    zero_ops.chunks[0].op_range = 0..0;
    zero_ops.chunks[0].payload_identity =
        super::super::super::PaintPayloadIdentity::prepared_texts(std::iter::empty());
    assert!(localize(&zero_ops).is_none(), "zero Text ops");

    let mut multiple_ops = raw.clone();
    multiple_ops.ops.push(multiple_ops.ops[0].clone());
    multiple_ops.chunks[0].op_range = 0..2;
    multiple_ops.chunks[0].payload_identity =
        super::super::super::PaintPayloadIdentity::prepared_texts(multiple_ops.ops.iter().filter_map(
            |op| match op {
                super::super::super::PaintOp::PreparedText(text) => Some(text),
                _ => None,
            },
        ));
    assert!(localize(&multiple_ops).is_none(), "multiple Text ops");

    let mut multiple_fragments = raw.clone();
    reseal(&mut multiple_fragments, &|params| {
        params.fragments.push(params.fragments[0]);
    });
    assert!(
        localize(&multiple_fragments).is_none(),
        "multiple fragments"
    );

    let mut zero_fragments = raw.clone();
    let super::super::super::PaintOp::PreparedText(text) = &mut zero_fragments.ops[0] else {
        unreachable!()
    };
    text.params.fragments.clear();
    assert!(localize(&zero_fragments).is_none(), "zero fragments");

    let mut illegal_fragment = raw.clone();
    let super::super::super::PaintOp::PreparedText(text) = &mut illegal_fragment.ops[0] else {
        unreachable!()
    };
    text.params.staging_input.glyphs[0].paint.fragment_index = 1;
    assert!(
        localize(&illegal_fragment).is_none(),
        "illegal fragment index"
    );

    for tamper in 0..4 {
        let mut invalid = raw.clone();
        let super::super::super::PaintOp::PreparedText(text) = &mut invalid.ops[0] else {
            unreachable!()
        };
        match tamper {
            0 => text.params.fragments[0].origin[0] += 1.0,
            1 => text.params.staging_input.glyphs[0].final_paint_pos[1] += 1.0,
            2 => text.params.scissor_rect = Some([0, 0, 10, 10]),
            3 => text.params.stencil_clip_id = Some(1),
            _ => unreachable!(),
        }
        assert!(localize(&invalid).is_none(), "tamper variant {tamper}");
    }
}

#[test]
fn nested_scroll_leaf_localizer_translates_draw_ops_and_shadow_mesh_vertices() {
    let scene = compiled_nested_scroll_fixture();
    let scaffold = scene.plan.nested_scroll_planning_scaffold().unwrap();
    let [outer_boundary, _] = scaffold.boundaries.as_slice() else {
        unreachable!()
    };
    let super::super::super::frame_plan::NestedScrollSceneScheduledStep::ContentReceiver(receiver) =
        &scaffold.schedule.steps[2]
    else {
        unreachable!()
    };
    let mut raw = receiver.artifact.artifact().clone();
    let shadow = super::super::super::PreparedShadowOp::new(
        crate::view::render_pass::shadow_module::ShadowMesh::rounded_rect(
            raw.chunks[0].bounds.x,
            raw.chunks[0].bounds.y,
            raw.chunks[0].bounds.width,
            raw.chunks[0].bounds.height,
            0.0,
        ),
        crate::view::render_pass::shadow_module::ShadowParams {
            offset_x: 4.0,
            offset_y: 5.0,
            ..Default::default()
        },
    )
    .unwrap();
    raw.ops
        .insert(0, super::super::super::PaintOp::PreparedShadow(shadow));
    raw.chunks[0].op_range = 0..raw.ops.len();
    let shadow = match &raw.ops[0] {
        super::super::super::PaintOp::PreparedShadow(shadow) => shadow,
        _ => unreachable!(),
    };
    let rect = match &raw.ops[1] {
        super::super::super::PaintOp::DrawRect(rect) => rect,
        _ => unreachable!(),
    };
    raw.chunks[0].payload_identity =
        super::super::super::PaintPayloadIdentity::prepared_shadows_with_decoration([shadow], [rect])
            .unwrap();
    let RetainedPropertyScrollGenericAuthority::NestedScrollCompiler(contract) =
        &scene.transaction.generic_authority
    else {
        unreachable!()
    };
    let recorded = contract
        .compiled
        .leaf_recorded_bounds_bits
        .map(f32::from_bits);
    let local = contract
        .compiled
        .leaf_source_bounds_bits
        .map(f32::from_bits);
    let delta = [local[0] - recorded[0], local[1] - recorded[1]];
    let localized = validate_nested_scroll_content_artifact(
        &raw,
        scaffold.admission.content_leaf,
        outer_boundary.scroll.id,
        outer_boundary.contents_clip,
        contract.compiled.leaf_recorded_bounds_bits,
        contract.compiled.leaf_source_bounds_bits,
    )
    .expect("supported rect/shadow leaf localizes into executable content");
    let localized = localized.artifact_for_test();
    let raw_rect = raw
        .ops
        .iter()
        .find_map(|op| match op {
            super::super::super::PaintOp::DrawRect(rect) => Some(&rect.params),
            _ => None,
        })
        .unwrap();
    let localized_rect = localized
        .ops
        .iter()
        .find_map(|op| match op {
            super::super::super::PaintOp::DrawRect(rect) => Some(&rect.params),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        localized_rect.position.map(f32::to_bits),
        [
            raw_rect.position[0] + delta[0],
            raw_rect.position[1] + delta[1],
        ]
        .map(f32::to_bits)
    );
    assert_eq!(
        localized_rect.position.map(f32::to_bits),
        [0.0; 2].map(f32::to_bits)
    );

    let raw_shadow = raw
        .ops
        .iter()
        .find_map(|op| match op {
            super::super::super::PaintOp::PreparedShadow(shadow) => Some(shadow),
            _ => None,
        })
        .expect("fixture records one exact prepared shadow");
    let localized_shadow = localized
        .ops
        .iter()
        .find_map(|op| match op {
            super::super::super::PaintOp::PreparedShadow(shadow) => Some(shadow),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        localized_shadow.mesh.vertices.len(),
        raw_shadow.mesh.vertices.len()
    );
    for (actual, recorded) in localized_shadow
        .mesh
        .vertices
        .iter()
        .zip(&raw_shadow.mesh.vertices)
    {
        assert_eq!(
            actual.map(f32::to_bits),
            [recorded[0] + delta[0], recorded[1] + delta[1]].map(f32::to_bits)
        );
    }
    assert!(localized.clip_nodes.is_empty());
    assert_eq!(localized.chunks[0].properties, PropertyTreeState::default());
    assert_eq!(
        [
            localized.chunks[0].bounds.x,
            localized.chunks[0].bounds.y,
            localized.chunks[0].bounds.width,
            localized.chunks[0].bounds.height,
        ]
        .map(f32::to_bits),
        contract.compiled.leaf_source_bounds_bits
    );
}

#[test]
fn nested_scroll_unsupported_text_leaf_localizer_fails_closed_without_graph_or_pool_mutation() {
    let scene = compiled_nested_scroll_fixture();
    let scaffold = scene.plan.nested_scroll_planning_scaffold().unwrap();
    let [outer_boundary, _] = scaffold.boundaries.as_slice() else {
        unreachable!()
    };
    let super::super::super::frame_plan::NestedScrollSceneScheduledStep::ContentReceiver(receiver) =
        &scaffold.schedule.steps[2]
    else {
        unreachable!()
    };
    let mut unsupported = receiver.artifact.artifact().clone();
    unsupported.chunks[0].payload_identity =
        super::super::super::PaintPayloadIdentity::PreparedTexts(std::sync::Arc::from([]));
    let RetainedPropertyScrollGenericAuthority::NestedScrollCompiler(contract) =
        &scene.transaction.generic_authority
    else {
        unreachable!()
    };
    let viewport = Viewport::new();
    let graph = FrameGraph::new();
    let graph_before = graph.build_state_snapshot_for_test();
    let pool_before = viewport.retained_surface_transaction_shape_for_test();

    assert!(
        validate_nested_scroll_content_artifact(
            &unsupported,
            scaffold.admission.content_leaf,
            outer_boundary.scroll.id,
            outer_boundary.contents_clip,
            contract.compiled.leaf_recorded_bounds_bits,
            contract.compiled.leaf_source_bounds_bits,
        )
        .is_none()
    );
    assert_eq!(graph.build_state_snapshot_for_test(), graph_before);
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        pool_before
    );
}
