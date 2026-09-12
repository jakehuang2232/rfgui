use super::*;

#[test]
fn phase_plan_preserves_vector_order_around_dom_children() {
    let mut arena = NodeArena::new();
    let root = insert_plan(&mut arena, PlanHost::recordable(0x8f00, &[0, 1], &[0, 1]));
    let child = insert_plan(&mut arena, PlanHost::recordable(0x8f01, &[0], &[]));
    append(&mut arena, root, child);

    let manifest = record(&arena, &[root], false);
    let sequence = manifest
        .items
        .iter()
        .map(|item| match item {
            PaintCoverageItem::ArtifactChunk { chunk, order, .. } => {
                (chunk.owner, order.phase, order.slot)
            }
            other => panic!("unexpected coverage item: {other:?}"),
        })
        .collect::<Vec<_>>();
    assert_eq!(
        sequence,
        vec![
            (root, PaintNodePhase::BeforeChildren, 0),
            (root, PaintNodePhase::BeforeChildren, 1),
            (child, PaintNodePhase::BeforeChildren, 0),
            (root, PaintNodePhase::AfterChildren, 0),
            (root, PaintNodePhase::AfterChildren, 1),
        ]
    );

    let (properties, generations) = identity(&arena, &[root]);
    let crate::view::paint::FrameArtifactRecordOutcome::Artifact {
        artifact,
        eligibility,
    } = crate::view::paint::record_frame_artifact(
        &arena,
        &[root],
        &properties,
        &generations,
        crate::view::paint::RendererMode::Auto,
    )
    .expect("phase plan recording")
    else {
        panic!("phase plan must be whole-frame eligible")
    };
    assert!(eligibility.eligible);
    assert_eq!(artifact.chunks.len(), 5);
    assert_eq!(
        artifact
            .chunks
            .iter()
            .filter(|chunk| chunk.owner == root)
            .count(),
        4,
        "same owner must retain every distinct phase/slot chunk"
    );
    let mut graph = FrameGraph::new();
    let ctx = UiBuildContext::new(16, 16, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    assert!(
        crate::view::paint::try_compile_artifact(&artifact, &mut graph, ctx).is_ok(),
        "compiler/store must accept same-owner chunks with distinct phase/slot ids"
    );
}

#[test]
fn transparent_parent_and_leaf_are_canonical_coverage_without_chunks() {
    let mut arena = NodeArena::new();
    let parent = insert_plan(&mut arena, PlanHost::transparent(0x8f10));
    let child = insert_plan(&mut arena, PlanHost::recordable(0x8f11, &[0], &[]));
    append(&mut arena, parent, child);
    let leaf = insert_plan(&mut arena, PlanHost::transparent(0x8f12));
    let roots = [parent, leaf];
    let (properties, generations) = identity(&arena, &roots);

    let metadata = record_coverage_manifest(
        &arena,
        &roots,
        false,
        true,
        CoverageRecordingMode::MetadataOnly,
        &properties,
        &generations,
    );
    let full = record_coverage_manifest(
        &arena,
        &roots,
        false,
        true,
        CoverageRecordingMode::FullArtifact,
        &properties,
        &generations,
    );
    assert!(matches!(
        metadata.items.as_slice(),
        [
            PaintCoverageItem::TransparentNode { owner: a, .. },
            PaintCoverageItem::ArtifactChunk { chunk, .. },
            PaintCoverageItem::TransparentNode { owner: b, .. },
        ] if *a == parent && chunk.owner == child && *b == leaf
    ));
    assert!(super::super::super::frame_recorder::canonical_manifest_matches(&metadata, &full));
    let stats = metadata.stats();
    assert_eq!(stats.total_nodes, 3);
    assert_eq!(stats.artifact_nodes, 3);
    assert_eq!(stats.artifact_chunks, 1);
}

#[test]
fn contents_scope_uses_descendants_properties_and_compiles_intersect_clip() {
    let mut arena = NodeArena::new();
    let mut host = PlanHost::recordable(0x8f18, &[0], &[]);
    host.metadata_scope = PaintPropertyScope::Contents;
    host.full_scope = PaintPropertyScope::Contents;
    host.contents_scissor = Some([3, 4, 20, 10]);
    let root = insert_plan(&mut arena, host);
    let (properties, generations) = identity(&arena, &[root]);
    let states = properties.node_state_for(root).expect("property states");
    assert_eq!(states.paint.clip, None);
    assert!(matches!(
        states.descendants.clip,
        Some(crate::view::compositor::property_tree::ClipNodeId {
            owner,
            role: crate::view::compositor::property_tree::ClipNodeRole::ContentsClip,
        }) if owner == root
    ));

    let crate::view::paint::FrameArtifactRecordOutcome::Artifact { artifact, .. } =
        crate::view::paint::record_frame_artifact(
            &arena,
            &[root],
            &properties,
            &generations,
            crate::view::paint::RendererMode::Auto,
        )
        .expect("contents artifact")
    else {
        panic!("contents scope must be artifact eligible")
    };
    assert_eq!(artifact.chunks[0].id.scope, PaintPropertyScope::Contents);
    assert_eq!(artifact.chunks[0].properties, states.descendants);
    let mut graph = FrameGraph::new();
    let ctx = UiBuildContext::new(32, 32, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    assert!(crate::view::paint::try_compile_artifact(&artifact, &mut graph, ctx).is_ok());
}

#[test]
fn deferred_nodes_stay_in_the_late_phase() {
    let mut arena = NodeArena::new();
    let root = insert_plan(&mut arena, PlanHost::recordable(0x8f40, &[0], &[0]));
    let mut deferred_host = PlanHost::recordable(0x8f41, &[0], &[0]);
    deferred_host.deferred = true;
    let deferred = insert_plan(&mut arena, deferred_host);
    append(&mut arena, root, deferred);
    let normal_root = insert_plan(&mut arena, PlanHost::recordable(0x8f42, &[0], &[]));
    let manifest = record(&arena, &[root, normal_root], false);
    let sequence = manifest
        .items
        .iter()
        .filter_map(|item| match item {
            PaintCoverageItem::ArtifactChunk { chunk, .. } => Some((chunk.owner, chunk.id.phase)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        sequence,
        vec![
            (root, PaintNodePhase::BeforeChildren),
            (root, PaintNodePhase::AfterChildren),
            (normal_root, PaintNodePhase::BeforeChildren),
            (deferred, PaintNodePhase::BeforeChildren),
            (deferred, PaintNodePhase::AfterChildren),
        ]
    );
}

#[test]
fn recursive_manifest_preserves_root_order_and_self_before_children() {
    let mut arena = NodeArena::new();
    let a = insert(&mut arena, 1);
    let child = insert(&mut arena, 2);
    append(&mut arena, a, child);
    let b = insert(&mut arena, 3);
    let manifest = record(&arena, &[a, b], false);
    let owners = manifest
        .items
        .iter()
        .filter_map(|item| match item {
            PaintCoverageItem::ArtifactChunk { chunk, .. } => Some(chunk.owner),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(owners, vec![a, child, b]);
    let orders = manifest
        .items
        .iter()
        .filter_map(|item| match item {
            PaintCoverageItem::ArtifactChunk { order, .. } => Some(order.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(orders[0].root_index, 0);
    assert_eq!(orders[1].child_path.as_ref(), &[0]);
    assert_eq!(orders[2].root_index, 1);
}

#[test]
fn recording_is_side_effect_free_and_deterministic() {
    let mut arena = NodeArena::new();
    let root = insert(&mut arena, 50);
    let first = record(&arena, &[root], false);
    let second = record(&arena, &[root], false);
    assert_eq!(format!("{:?}", first.items), format!("{:?}", second.items));
    assert_eq!(first.validation_errors, second.validation_errors);
}
