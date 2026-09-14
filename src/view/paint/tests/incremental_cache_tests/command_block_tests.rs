use super::*;

#[test]
fn complete_blocks_reuse_commands_but_mutation_and_current_opacity_fail_closed() {
    let (arena, root, properties, generations) =
        prepared_leaf(0xfeed_9401, Color::rgb(255, 0, 0), 0.5, false);
    let mut recording = RecordingCache::default();
    let mut planning = PlanningCache::default();
    let context = ArtifactSurfaceRasterContext::new(
        1.,
        wgpu::TextureFormat::Rgba8Unorm,
        [0., 0.],
        None,
        8192,
        128 * 1024 * 1024,
    )
    .unwrap();
    let mut last = None;
    for _ in 0..4 {
        let input = artifact(
            record_surface_dag_frame_artifact_cached(
                &arena,
                &[root],
                &properties,
                &generations,
                &mut recording,
            )
            .unwrap(),
        );
        let actual =
            prepare_artifact_surface_raster_plan_cached(input.clone(), context, &mut planning)
                .unwrap();
        let fresh = prepare_artifact_surface_raster_plan(input.clone(), context).unwrap();
        assert_eq!(format!("{actual:?}"), format!("{fresh:?}"));
        last = Some(input);
    }
    let input = last.unwrap();
    assert!(planning.command_validation_counts().0 > 0);
    assert_eq!(planning.command_validation_counts().1, 0);

    // Same complete metadata allocation, changed operations. COW must revoke
    // command proof even when geometry and payload identity appear unchanged.
    let mut damaged = input.clone();
    let PaintOp::DrawRect(rect) = &mut damaged.ops[0] else {
        panic!("rectangle fixture");
    };
    rect.params.opacity = f32::NAN;
    assert!(prepare_artifact_surface_raster_plan_cached(damaged, context, &mut planning).is_err());
    prepare_artifact_surface_raster_plan_cached(input.clone(), context, &mut planning).unwrap();
    assert!(
        planning.command_validation_counts().1 > 0,
        "failed frames discard proofs"
    );
    prepare_artifact_surface_raster_plan_cached(input.clone(), context, &mut planning).unwrap();
    assert!(planning.command_validation_counts().0 > 0);

    // Current property inputs remain authoritative even on a complete block hit.
    let mut damaged = input.clone();
    assert!(!damaged.effect_nodes.is_empty());
    damaged.effect_nodes[0].opacity = 0.25;
    assert!(prepare_artifact_surface_raster_plan_cached(damaged, context, &mut planning).is_err());
    prepare_artifact_surface_raster_plan_cached(input.clone(), context, &mut planning).unwrap();

    let mut damaged = input.clone();
    damaged.chunks[0].payload_identity = PaintPayloadIdentity::None;
    assert!(prepare_artifact_surface_raster_plan_cached(damaged, context, &mut planning).is_err());
    // The original block is immutable and remains independently valid.
    prepare_artifact_surface_raster_plan(input, context).unwrap();
}

#[test]
fn cached_blocks_still_check_cross_block_masks_and_duplicate_slots() {
    use std::sync::Arc;
    let mut arena = new_test_arena();
    let mut element = leaf_element(0xfeed_9402, Color::rgb(30, 80, 150), 1., false);
    let mut style = Style::new();
    style.set_border_radius(BorderRadius::uniform(Length::px(12.)));
    element.apply_style(style);
    let root = commit_element(&mut arena, Box::new(element));
    commit_child(
        &mut arena,
        root,
        Box::new(leaf_element(0xfeed_9403, Color::rgb(20, 50, 90), 1., false)),
    );
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    let (properties, generations) = sync_identity(&arena, &[root]);
    let mut input = artifact(
        record_surface_dag_frame_artifact(
            &arena,
            &[root],
            &properties,
            &generations,
            RendererMode::Auto,
        )
        .unwrap(),
    );
    let mut chunks = Vec::<Arc<[PaintChunk]>>::new();
    let mut ops = Vec::<Arc<[PaintOp]>>::new();
    for chunk in &input.chunks {
        chunks.push(vec![chunk.clone()].into());
        ops.push(input.ops[chunk.op_range.clone()].to_vec().into());
    }
    let compose = |chunks: &[Arc<[PaintChunk]>], ops: &[Arc<[PaintOp]>]| {
        let mut result = input.clone();
        result.chunks.clear();
        result.ops.clear();
        for chunk in chunks {
            result.chunks.append_shared(chunk.clone());
        }
        for block in ops {
            result.ops.append_shared(block.clone());
        }
        result
    };
    let context = ArtifactSurfaceRasterContext::new(
        1.,
        wgpu::TextureFormat::Rgba8Unorm,
        [0., 0.],
        None,
        8192,
        128 * 1024 * 1024,
    )
    .unwrap();
    let mut planning = PlanningCache::default();
    for _ in 0..2 {
        prepare_artifact_surface_raster_plan_cached(compose(&chunks, &ops), context, &mut planning)
            .unwrap();
    }
    assert_eq!(planning.command_validation_counts(), (chunks.len(), 0));
    let closing = chunks
        .iter()
        .position(|block| {
            block[0].id.slot == RETAINED_CHILD_MASK_SLOT
                && block[0].id.phase == PaintNodePhase::AfterChildren
        })
        .unwrap();
    let mut bad_chunks = chunks.clone();
    let mut bad_ops = ops.clone();
    let chunk = &mut Arc::make_mut(&mut bad_chunks[closing])[0];
    chunk.bounds.x += 1.;
    let PaintOp::DrawRect(rect) = &mut Arc::make_mut(&mut bad_ops[closing])[0] else {
        unreachable!();
    };
    rect.params.position[0] += 1.;
    chunk.payload_identity = PaintPayloadIdentity::prepared_rects([&*rect]).unwrap();
    let bad = compose(&bad_chunks, &bad_ops);
    assert!(prepare_artifact_surface_raster_plan(bad.clone(), context).is_err());
    assert!(prepare_artifact_surface_raster_plan_cached(bad, context, &mut planning).is_err());
    prepare_artifact_surface_raster_plan_cached(compose(&chunks, &ops), context, &mut planning)
        .unwrap();
    input = compose(&chunks, &ops);
    let mut duplicate = input.chunks[0].clone();
    let duplicate_ops = input.ops[duplicate.op_range.clone()].to_vec();
    duplicate.op_range = input.ops.len()..input.ops.len() + duplicate_ops.len();
    input.chunks.append_shared(vec![duplicate].into());
    input.ops.append_shared(duplicate_ops.into());
    assert!(prepare_artifact_surface_raster_plan_cached(input, context, &mut planning).is_err());
}
