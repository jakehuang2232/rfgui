use super::*;

#[test]
fn compiler_rejects_overlapping_ranges_before_emit() {
    let mut artifact = compiler_test_artifact();
    artifact.ops.push(artifact.ops[0].clone());
    let mut overlapping = distinct_chunk(artifact.chunks[0].clone());
    overlapping.op_range = 0..2;
    artifact.chunks.push(overlapping);
    assert_compiler_rejects_before_emit(&artifact, "overlapping ranges");
}

#[test]
fn compiler_rejects_out_of_order_ranges_before_emit() {
    let mut artifact = compiler_test_artifact();
    artifact.ops.push(artifact.ops[0].clone());
    let mut second = distinct_chunk(artifact.chunks[0].clone());
    artifact.chunks[0].op_range = 1..2;
    second.op_range = 0..1;
    artifact.chunks.push(second);
    assert_compiler_rejects_before_emit(&artifact, "out-of-order ranges");
}

#[test]
fn compiler_rejects_internal_gap_before_emit() {
    let mut artifact = compiler_test_artifact();
    artifact.ops.push(artifact.ops[0].clone());
    artifact.ops.push(artifact.ops[0].clone());
    let mut after_gap = distinct_chunk(artifact.chunks[0].clone());
    after_gap.op_range = 2..3;
    artifact.chunks.push(after_gap);
    assert_compiler_rejects_before_emit(&artifact, "internal op gap");
}

#[test]
fn compiler_rejects_trailing_unowned_ops_before_emit() {
    let mut artifact = compiler_test_artifact();
    artifact.ops.push(artifact.ops[0].clone());
    assert_compiler_rejects_before_emit(&artifact, "trailing unowned ops");
}

#[test]
fn compiler_rejects_duplicate_chunk_id_before_emit() {
    let mut artifact = compiler_test_artifact();
    artifact.ops.push(artifact.ops[0].clone());
    let mut duplicate = artifact.chunks[0].clone();
    duplicate.op_range = 1..2;
    artifact.chunks.push(duplicate);
    assert_compiler_rejects_before_emit(&artifact, "duplicate PaintChunkId");
}

#[test]
fn artifact_and_legacy_roots_keep_document_order() {
    let builds = Arc::new(AtomicUsize::new(0));
    let mut arena = new_test_arena();
    let artifact_root = commit_element(
        &mut arena,
        Box::new(leaf_element(50, Color::rgb(255, 0, 0), 1.0, false)),
    );
    let legacy_root = commit_element(
        &mut arena,
        Box::new(RecordingHost {
            id: 51,
            builds: builds.clone(),
            fill: Some([0.0, 0.0, 1.0, 1.0]),
        }),
    );
    let (measure, place) = constraints();
    measure_and_place(&mut arena, artifact_root, measure, place);
    let roots = [artifact_root, legacy_root];
    let (properties, generations) = sync_identity(&arena, &roots);

    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let target = ctx.allocate_target(&mut graph);
    ctx.set_current_target(target);
    for root in roots {
        let child_ctx = UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone());
        let next = match record_root(&arena, root, &properties, &generations) {
            PaintRecordOutcome::Artifact(artifact) => {
                compile_artifact(&artifact, &mut graph, child_ctx)
            }
            PaintRecordOutcome::LegacySubtree(_) => arena
                .with_element_taken(root, |element, arena| {
                    element.build(&mut graph, arena, child_ctx)
                })
                .expect("legacy root builds"),
        };
        ctx.set_state(next);
    }

    let snapshots = graph.test_rect_pass_snapshots();
    assert_eq!(
        snapshots.len(),
        2,
        "passes={:?} builds={}",
        graph
            .pass_descriptors()
            .into_iter()
            .map(|pass| pass.name)
            .collect::<Vec<_>>(),
        builds.load(Ordering::Relaxed)
    );
    assert!(f32::from_bits(snapshots[0].fill_color_bits[0]) > 0.9);
    assert!(f32::from_bits(snapshots[1].fill_color_bits[2]) > 0.9);
    assert_eq!(builds.load(Ordering::Relaxed), 1);
}
