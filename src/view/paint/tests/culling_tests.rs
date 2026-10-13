use super::*;

#[test]
fn whole_frame_should_render_false_culls_the_complete_subtree() {
    let (arena, root, child) = hidden_element_subtree(130, 131);
    let (properties, generations) = sync_identity(&arena, &[root]);
    assert_eq!(
        arena
            .get(root)
            .unwrap()
            .element
            .shadow_paint_recording_capability(&arena, false, &PaintRecordingContext::default(),),
        ShadowPaintRecordingCapability::CulledSubtree
    );
    let artifact = record_surface_dag_frame_artifact(&arena, &[root], &properties, &generations);
    assert!(artifact.chunks.is_empty());
    assert!(artifact.ops.is_empty());

    let manifest = record_coverage_manifest(
        &arena,
        &[root],
        true,
        CoverageRecordingMode::MetadataOnly,
        &properties,
        &generations,
    );
    assert!(matches!(
        manifest.items.as_slice(),
        [PaintCoverageItem::CulledSubtree { owner, .. }] if *owner == root
    ));
    assert!(manifest.items.iter().all(
        |item| !matches!(item, PaintCoverageItem::ArtifactChunk { chunk, .. } if chunk.owner == child)
    ));
    let stats = manifest.stats();
    assert_eq!(stats.culled_subtrees, 1);
    assert_eq!(stats.artifact_chunks, 0);
}

#[test]
fn culled_subtree_multi_root_keeps_only_the_visible_root_artifact() {
    let (mut arena, hidden, hidden_child) = hidden_element_subtree(140, 141);
    let visible = commit_element(
        &mut arena,
        Box::new(leaf_element(142, Color::rgb(20, 90, 220), 1.0, false)),
    );
    let (measure, place) = constraints();
    measure_and_place(&mut arena, visible, measure, place);
    let roots = [hidden, visible];
    let (properties, generations) = sync_identity(&arena, &roots);
    let artifact = record_surface_dag_frame_artifact(&arena, &roots, &properties, &generations);
    assert!(!artifact.chunks.is_empty());
    assert!(artifact.chunks.iter().all(|chunk| chunk.owner == visible));
    assert!(
        artifact
            .chunks
            .iter()
            .all(|chunk| chunk.owner != hidden && chunk.owner != hidden_child)
    );
}

#[test]
fn culled_subtree_metadata_full_parity_and_topology_revision_are_canonical() {
    let (mut arena, root, child) = hidden_element_subtree(150, 151);
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[root]);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[root], &properties);
    let record = |mode, properties: &PropertyTrees, generations: &PaintGenerationTracker| {
        record_coverage_manifest(&arena, &[root], true, mode, properties, generations)
    };
    let metadata = record(
        CoverageRecordingMode::MetadataOnly,
        &properties,
        &generations,
    );
    let mut full = record(
        CoverageRecordingMode::FullArtifact,
        &properties,
        &generations,
    );
    assert!(super::super::frame_recorder::canonical_manifest_matches(
        &metadata, &full
    ));
    let baseline_topology = match &metadata.items[0] {
        PaintCoverageItem::CulledSubtree {
            content_revision, ..
        } => content_revision.topology_revision,
        item => panic!("expected culled subtree, got {item:?}"),
    };
    let PaintCoverageItem::CulledSubtree {
        content_revision, ..
    } = &mut full.items[0]
    else {
        unreachable!()
    };
    content_revision.topology_revision = content_revision.topology_revision.wrapping_add(1);
    assert!(!super::super::frame_recorder::canonical_manifest_matches(
        &metadata, &full
    ));

    arena
        .get_mut(child)
        .unwrap()
        .element
        .as_any_mut()
        .downcast_mut::<Element>()
        .unwrap()
        .set_background_color_value(Color::rgb(10, 240, 80));
    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);
    let child_mutated = record_coverage_manifest(
        &arena,
        &[root],
        true,
        CoverageRecordingMode::MetadataOnly,
        &properties,
        &generations,
    );
    assert!(matches!(
        child_mutated.items.as_slice(),
        [PaintCoverageItem::CulledSubtree { owner, .. }] if *owner == root
    ));

    let added = commit_child(
        &mut arena,
        root,
        Box::new(leaf_element(152, Color::rgb(180, 30, 160), 1.0, false)),
    );
    let (measure, place) = constraints();
    measure_and_place(&mut arena, added, measure, place);
    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);
    let topology_changed = record_coverage_manifest(
        &arena,
        &[root],
        true,
        CoverageRecordingMode::MetadataOnly,
        &properties,
        &generations,
    );
    let changed_topology = match &topology_changed.items[0] {
        PaintCoverageItem::CulledSubtree {
            content_revision, ..
        } => content_revision.topology_revision,
        item => panic!("expected culled subtree, got {item:?}"),
    };
    assert_ne!(changed_topology, baseline_topology);
}

#[test]
fn generic_culled_subtree_with_nested_effects_records_no_paint() {
    let (arena, root, child) = hidden_element_subtree(0x7e30, 0x7e31);
    for owner in [root, child] {
        arena
            .get_mut(owner)
            .unwrap()
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .unwrap()
            .set_opacity(0.5);
    }
    let (properties, generations) = sync_identity(&arena, &[root]);
    assert!(
        properties
            .node_state_for(child)
            .unwrap()
            .paint
            .effect
            .is_some()
    );
    let artifact = record_surface_dag_frame_artifact(&arena, &[root], &properties, &generations);
    assert!(artifact.chunks.is_empty());
    assert!(
        artifact.ops.is_empty(),
        "ancestor and child opacity cannot make a culled subtree paint"
    );
}

#[test]
fn deferred_viewport_child_records_beneath_a_culled_subtree() {
    // The culled root is observed, its child is never walked, and the
    // grandchild paints late against the viewport.
    let (mut arena, root, middle) = hidden_element_subtree(160, 161);
    let mut late = leaf_element(162, Color::rgb(30, 160, 90), 1.0, false);
    let mut style = Style::new();
    style.insert(
        PropertyId::Position,
        ParsedValue::Position(Position::absolute().clip(ClipMode::Viewport)),
    );
    late.apply_style(style);
    let late = commit_child(&mut arena, middle, Box::new(late));
    let (measure, place) = constraints();
    measure_and_place(&mut arena, late, measure, place);
    let (properties, generations) = sync_identity(&arena, &[root]);

    let artifact = record_surface_dag_frame_artifact(&arena, &[root], &properties, &generations);
    assert!(artifact.chunks.iter().any(|chunk| chunk.owner == late));
    assert!(
        artifact
            .chunks
            .iter()
            .all(|chunk| chunk.owner != root && chunk.owner != middle)
    );
}
