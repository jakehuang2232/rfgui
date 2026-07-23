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
            .shadow_paint_recording_capability(
                &arena,
                false,
                PaintRecordingContext::default(),
            ),
        ShadowPaintRecordingCapability::CulledSubtree
    );
    let FrameArtifactRecordOutcome::Artifact {
        artifact,
        eligibility,
    } = record_frame_artifact(
        &arena,
        &[root],
        &properties,
        &generations,
        RendererMode::ForcedForTests,
    )
    .expect("effect-neutral hidden subtree is fully covered")
    else {
        panic!("forced recording cannot silently fall back")
    };
    assert!(eligibility.eligible, "{eligibility:?}");
    assert!(artifact.chunks.is_empty());
    assert!(artifact.ops.is_empty());

    let manifest = record_coverage_manifest(
        &arena,
        &[root],
        false,
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
    let FrameArtifactRecordOutcome::Artifact {
        artifact,
        eligibility,
    } = record_clip_enabled_frame_artifact(
        &arena,
        &roots,
        &properties,
        &generations,
        RendererMode::ForcedForTests,
    )
    .expect("effect-neutral multi-root frame is recordable")
    else {
        panic!("forced recording cannot silently fall back")
    };
    assert!(eligibility.eligible, "{eligibility:?}");
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
        record_coverage_manifest(&arena, &[root], false, true, mode, properties, generations)
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
        false,
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
        false,
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
fn culled_subtree_keeps_root_effect_and_deferred_fail_closed() {
    let (arena, root, _) = hidden_element_subtree(156, 157);
    let mut transform_style = Style::new();
    transform_style.set_transform(Transform::new([Translate::x(Length::px(3.0))]));
    arena
        .get_mut(root)
        .unwrap()
        .element
        .as_any_mut()
        .downcast_mut::<Element>()
        .unwrap()
        .apply_style(transform_style);
    assert_eq!(
        arena
            .get(root)
            .unwrap()
            .element
            .shadow_paint_recording_capability(
                &arena,
                false,
                PaintRecordingContext::default(),
            ),
        ShadowPaintRecordingCapability::Legacy(ShadowPaintBlocker::Transform)
    );

    let (arena, root, _) = hidden_element_subtree(158, 159);
    let mut scroll_style = Style::new();
    scroll_style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(crate::style::ScrollDirection::Vertical),
    );
    arena
        .get_mut(root)
        .unwrap()
        .element
        .as_any_mut()
        .downcast_mut::<Element>()
        .unwrap()
        .apply_style(scroll_style);
    assert_eq!(
        arena
            .get(root)
            .unwrap()
            .element
            .shadow_paint_recording_capability(
                &arena,
                false,
                PaintRecordingContext::default(),
            ),
        ShadowPaintRecordingCapability::Legacy(ShadowPaintBlocker::ScrollContainer)
    );

    let (arena, root, child) = hidden_element_subtree(166, 167);
    arena
        .get_mut(child)
        .unwrap()
        .element
        .as_any_mut()
        .downcast_mut::<Element>()
        .unwrap()
        .set_opacity(0.5);
    let (properties, generations) = sync_identity(&arena, &[root]);
    take_full_artifact_record_count();
    let child_effect = record_frame_artifact(
        &arena,
        &[root],
        &properties,
        &generations,
        RendererMode::ForcedForTests,
    )
    .unwrap_err();
    assert!(child_effect.reasons.iter().any(|reason| matches!(
        reason,
        FrameArtifactFallbackReason::LegacyBoundary(LegacyPaintReason::StatefulPaint)
    )));
    assert_eq!(take_full_artifact_record_count(), 0);

    let (arena, root, child) = hidden_element_subtree(162, 163);
    let mut deferred_style = Style::new();
    deferred_style.insert(
        PropertyId::Position,
        ParsedValue::Position(Position::absolute().clip(ClipMode::Viewport)),
    );
    arena
        .get_mut(child)
        .unwrap()
        .element
        .as_any_mut()
        .downcast_mut::<Element>()
        .unwrap()
        .apply_style(deferred_style);
    let (properties, generations) = sync_identity(&arena, &[root]);
    take_full_artifact_record_count();
    let deferred = record_clip_enabled_frame_artifact(
        &arena,
        &[root],
        &properties,
        &generations,
        RendererMode::ForcedForTests,
    )
    .unwrap_err();
    assert!(deferred.reasons.iter().any(|reason| matches!(
        reason,
        FrameArtifactFallbackReason::LegacyBoundary(LegacyPaintReason::Deferred)
    )));
    assert_eq!(take_full_artifact_record_count(), 0);

    let (arena, root, _) = hidden_element_subtree(164, 165);
    arena
        .get_mut(root)
        .unwrap()
        .element
        .as_any_mut()
        .downcast_mut::<Element>()
        .unwrap()
        .set_opacity(0.5);
    let (properties, generations) = sync_identity(&arena, &[root]);
    take_full_artifact_record_count();
    let effect = record_root_group_opacity_frame_artifact(
        &arena,
        &[root],
        &properties,
        &generations,
        RendererMode::ForcedForTests,
    )
    .unwrap_err();
    assert!(effect.reasons.iter().any(|reason| matches!(
        reason,
        FrameArtifactFallbackReason::LegacyBoundary(LegacyPaintReason::StatefulPaint)
            | FrameArtifactFallbackReason::PropertyBoundary(_)
    )));
    assert_eq!(take_full_artifact_record_count(), 0);
}
