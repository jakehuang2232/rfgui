use super::*;

#[test]
fn planned_boundary_is_typed_canonical_and_stops_both_recording_passes() {
    let mut arena = NodeArena::new();
    let root = insert_plan(&mut arena, PlanHost::recordable(0x8f08, &[0], &[0]));
    let boundary_root = insert_plan(&mut arena, PlanHost::recordable(0x8f09, &[0], &[0]));
    let hidden_descendant = insert_plan(&mut arena, PlanHost::recordable(0x8f0a, &[0], &[]));
    append(&mut arena, root, boundary_root);
    append(&mut arena, boundary_root, hidden_descendant);
    let (mut properties, generations) = identity(&arena, &[root]);
    properties.transforms.insert(
        TransformNodeId(boundary_root),
        crate::view::compositor::property_tree::TransformNode {
            owner: boundary_root,
            parent: None,
            local_matrix: glam::Mat4::IDENTITY,
            local_origin: glam::Vec3::ZERO,
            local_generation: 1,
            generation: 1,
            derived_projection: Some(
                crate::view::compositor::property_tree::DerivedSpatialProjection {
                    owner_viewport_position: glam::Vec2::ZERO,
                    owner_viewport_transform: glam::Mat4::IDENTITY,
                },
            ),
        },
    );
    let boundary = PlannedBoundary {
        root: boundary_root,
        stable_id: 0x8f09,
        kind: PlannedBoundaryKind::Transform(TransformNodeId(boundary_root)),
    };
    let cutouts = PlannedBoundaryCutoutSet::from_iter([(boundary_root, boundary)]);
    let record = |mode| {
        record_coverage_manifest_with_context(
            &arena,
            &[root],
            false,
            true,
            mode,
            &properties,
            &generations,
            PaintRecordingContext::default(),
            None,
            &cutouts,
        )
    };
    let metadata = record(CoverageRecordingMode::MetadataOnly);
    let full = record(CoverageRecordingMode::FullArtifact);

    for manifest in [&metadata, &full] {
        assert!(matches!(
            manifest.items.as_slice(),
            [
                PaintCoverageItem::ArtifactChunk { chunk: before, .. },
                PaintCoverageItem::PlannedBoundary { boundary: actual, .. },
                PaintCoverageItem::ArtifactChunk { chunk: after, .. },
            ] if before.owner == root
                && before.id.phase == PaintNodePhase::BeforeChildren
                && *actual == boundary
                && after.owner == root
                && after.id.phase == PaintNodePhase::AfterChildren
        ));
        assert!(manifest.items.iter().all(|item| match item {
            PaintCoverageItem::ArtifactChunk { chunk, .. } => {
                chunk.owner != boundary_root && chunk.owner != hidden_descendant
            }
            _ => true,
        }));
    }
    assert!(super::super::super::frame_recorder::canonical_manifest_matches(&metadata, &full));

    let mut mismatched = full.clone();
    let PaintCoverageItem::PlannedBoundary {
        boundary: mismatched_boundary,
        ..
    } = &mut mismatched.items[1]
    else {
        panic!("fixture marker")
    };
    mismatched_boundary.stable_id ^= 1;
    assert!(
        !super::super::super::frame_recorder::canonical_manifest_matches(&metadata, &mismatched)
    );
    mismatched.items.remove(1);
    assert!(
        !super::super::super::frame_recorder::canonical_manifest_matches(&metadata, &mismatched)
    );

    let invalid_boundary = PlannedBoundary {
        stable_id: boundary.stable_id ^ 1,
        ..boundary
    };
    let invalid_cutouts = PlannedBoundaryCutoutSet::from_iter([(boundary_root, invalid_boundary)]);
    let invalid = record_coverage_manifest_with_context(
        &arena,
        &[root],
        false,
        true,
        CoverageRecordingMode::MetadataOnly,
        &properties,
        &generations,
        PaintRecordingContext::default(),
        None,
        &invalid_cutouts,
    );
    assert_eq!(
        invalid.validation_errors,
        vec![PaintCoverageValidationError::InvalidPlannedBoundary(
            boundary_root
        )]
    );

    let isolation_boundary = PlannedBoundary {
        root: boundary_root,
        stable_id: boundary.stable_id,
        kind: PlannedBoundaryKind::Isolation(crate::view::compositor::property_tree::EffectNodeId(
            boundary_root,
        )),
    };
    let isolation_cutouts =
        PlannedBoundaryCutoutSet::from_iter([(boundary_root, isolation_boundary)]);
    let missing_isolation_snapshot = record_coverage_manifest_with_context(
        &arena,
        &[root],
        false,
        true,
        CoverageRecordingMode::MetadataOnly,
        &properties,
        &generations,
        PaintRecordingContext::default(),
        None,
        &isolation_cutouts,
    );
    assert_eq!(
        missing_isolation_snapshot.validation_errors,
        vec![PaintCoverageValidationError::InvalidPlannedBoundary(
            boundary_root
        )],
        "a transform snapshot cannot authorize an isolation marker"
    );
    properties.effects.insert(
        EffectNodeId(boundary_root),
        crate::view::compositor::property_tree::EffectNode {
            owner: boundary_root,
            parent: None,
            opacity: 0.5,
            generation: 1,
        },
    );
    for mode in [
        CoverageRecordingMode::MetadataOnly,
        CoverageRecordingMode::FullArtifact,
    ] {
        let manifest = record_coverage_manifest_with_context(
            &arena,
            &[root],
            false,
            true,
            mode,
            &properties,
            &generations,
            PaintRecordingContext::default(),
            None,
            &isolation_cutouts,
        );
        assert!(matches!(
            manifest.items.as_slice(),
            [
                PaintCoverageItem::ArtifactChunk { chunk: before, .. },
                PaintCoverageItem::PlannedBoundary { boundary: actual, .. },
                PaintCoverageItem::ArtifactChunk { chunk: after, .. },
            ] if before.owner == root
                && *actual == isolation_boundary
                && after.owner == root
        ));
        assert!(manifest.items.iter().all(|item| match item {
            PaintCoverageItem::ArtifactChunk { chunk, .. } => {
                chunk.owner != boundary_root && chunk.owner != hidden_descendant
            }
            _ => true,
        }));
    }

    let wrong_effect_boundary = PlannedBoundary {
        kind: PlannedBoundaryKind::Isolation(crate::view::compositor::property_tree::EffectNodeId(
            root,
        )),
        ..isolation_boundary
    };
    let wrong_effect_cutouts =
        PlannedBoundaryCutoutSet::from_iter([(boundary_root, wrong_effect_boundary)]);
    let invalid = record_coverage_manifest_with_context(
        &arena,
        &[root],
        false,
        true,
        CoverageRecordingMode::MetadataOnly,
        &properties,
        &generations,
        PaintRecordingContext::default(),
        None,
        &wrong_effect_cutouts,
    );
    assert_eq!(
        invalid.validation_errors,
        vec![PaintCoverageValidationError::InvalidPlannedBoundary(
            boundary_root
        )]
    );

    properties
        .transforms
        .remove(&TransformNodeId(boundary_root));
    let missing_transform_snapshot = record_coverage_manifest_with_context(
        &arena,
        &[root],
        false,
        true,
        CoverageRecordingMode::MetadataOnly,
        &properties,
        &generations,
        PaintRecordingContext::default(),
        None,
        &cutouts,
    );
    assert_eq!(
        missing_transform_snapshot.validation_errors,
        vec![PaintCoverageValidationError::InvalidPlannedBoundary(
            boundary_root
        )],
        "an effect snapshot cannot authorize a transform marker"
    );
}

#[test]
fn artifact_legacy_artifact_sequence_reports_unprepared_text_boundary() {
    let mut arena = NodeArena::new();
    let root = insert(&mut arena, 14);
    let left = insert(&mut arena, 15);
    let unknown = arena.insert(Node::new(Box::new(Text::new(0.0, 0.0, 10.0, 10.0, "text"))));
    let right = insert(&mut arena, 16);
    append(&mut arena, root, left);
    append(&mut arena, root, unknown);
    append(&mut arena, root, right);
    let manifest = record(&arena, &[root], false);
    assert!(matches!(manifest.items.as_slice(), [
        PaintCoverageItem::ArtifactChunk { chunk: root_chunk, .. },
        PaintCoverageItem::ArtifactChunk { chunk: left_chunk, .. },
        PaintCoverageItem::LegacyBoundary { root: legacy_root, reason: LegacyPaintReason::MissingPreparedText, .. },
        PaintCoverageItem::ArtifactChunk { chunk: right_chunk, .. },
    ] if root_chunk.owner == root && left_chunk.owner == left && *legacy_root == unknown && right_chunk.owner == right));
}

#[test]
fn coverage_stats_count_entire_legacy_subtree_once() {
    let mut arena = NodeArena::new();
    let mut transformed = Element::new_with_id(70, 0.0, 0.0, 10.0, 10.0);
    let mut style = Style::new();
    style.set_transform(Transform::new([Rotate::z(Angle::deg(10.0))]));
    transformed.apply_style(style);
    let root = arena.insert(Node::new(Box::new(transformed)));
    let mut parent = root;
    for id in 71..80 {
        let child = insert(&mut arena, id);
        append(&mut arena, parent, child);
        parent = child;
    }
    let leaf = insert(&mut arena, 80);
    append(&mut arena, parent, leaf);
    let manifest = record(&arena, &[root], false);
    let stats = manifest.stats();
    assert_eq!(stats.total_nodes, 11);
    assert_eq!(stats.legacy_covered_nodes, 11);
}

#[test]
fn fallback_boundary_does_not_record_descendant_and_exact_deferred_root_records_late() {
    let mut arena = NodeArena::new();
    let mut transformed = Element::new_with_id(20, 0.0, 0.0, 10.0, 10.0);
    let mut style = Style::new();
    style.set_transform(Transform::new([Rotate::z(Angle::deg(10.0))]));
    transformed.apply_style(style);
    let root = arena.insert(Node::new(Box::new(transformed)));
    let child = insert(&mut arena, 21);
    append(&mut arena, root, child);
    let manifest = record(&arena, &[root], false);
    assert_eq!(manifest.items.len(), 1);
    assert!(
        matches!(manifest.items[0], PaintCoverageItem::LegacyBoundary { root: boundary, reason: LegacyPaintReason::Transform, .. } if boundary == root)
    );

    let mut deferred = Element::new_with_id(30, 0.0, 0.0, 10.0, 10.0);
    let mut deferred_style = Style::new();
    deferred_style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .left(Length::px(0.0))
                .clip(ClipMode::Viewport),
        ),
    );
    deferred.apply_style(deferred_style);
    let deferred_root = arena.insert(Node::new(Box::new(deferred)));
    let deferred_manifest = record(&arena, &[deferred_root], false);
    assert!(
        matches!(
            deferred_manifest.items.as_slice(),
            [PaintCoverageItem::ArtifactChunk { order, chunk, clip_snapshot, .. }]
                if order.root_index == 1
                    && order.child_path.as_ref() == [0]
                    && chunk.owner == deferred_root
                    && clip_snapshot.len() == 1
                    && clip_snapshot[0].owner == deferred_root
                    && clip_snapshot[0].behavior
                        == crate::view::compositor::property_tree::ClipBehavior::Replace
        ),
        "{:#?}",
        deferred_manifest.items
    );
}
