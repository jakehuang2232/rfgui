use super::*;

#[test]
fn metadata_full_phase_and_slot_drift_or_duplicates_are_rejected() {
    fn manifests(host: PlanHost) -> (PaintCoverageManifest, PaintCoverageManifest) {
        let mut arena = NodeArena::new();
        let root = insert_plan(&mut arena, host);
        let (properties, generations) = identity(&arena, &[root]);
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
        (metadata, full)
    }

    let mut missing = PlanHost::recordable(0x8f20, &[0, 1], &[]);
    missing.full = PlanShape::new(&[0], &[]);
    let (metadata, full) = manifests(missing);
    assert!(!super::super::super::frame_recorder::canonical_manifest_matches(&metadata, &full));

    let mut swapped = PlanHost::recordable(0x8f21, &[0, 1], &[]);
    swapped.full.before.swap(0, 1);
    let (metadata, full) = manifests(swapped);
    assert!(!super::super::super::frame_recorder::canonical_manifest_matches(&metadata, &full));

    let mut wrong_phase = PlanHost::recordable(0x8f22, &[0], &[]);
    wrong_phase.full.before[0].0 = PaintNodePhase::AfterChildren;
    let (_, full) = manifests(wrong_phase);
    assert!(full.validation_errors.iter().any(|error| matches!(
        error,
        PaintCoverageValidationError::InvalidChunkPhase {
            expected: PaintNodePhase::BeforeChildren,
            actual: PaintNodePhase::AfterChildren,
            ..
        }
    )));

    let mut duplicate = PlanHost::recordable(0x8f23, &[0], &[]);
    duplicate
        .full
        .before
        .push((PaintNodePhase::BeforeChildren, 0));
    let (_, full) = manifests(duplicate);
    assert!(full.validation_errors.iter().any(|error| matches!(
        error,
        PaintCoverageValidationError::DuplicateChunkSlot {
            phase: PaintNodePhase::BeforeChildren,
            slot: 0,
            ..
        }
    )));

    let mut scope_drift = PlanHost::recordable(0x8f24, &[0], &[]);
    scope_drift.contents_scissor = Some([0, 0, 10, 10]);
    scope_drift.full_scope = PaintPropertyScope::Contents;
    let (metadata, full) = manifests(scope_drift);
    assert!(!super::super::super::frame_recorder::canonical_manifest_matches(&metadata, &full));
}

#[test]
fn metadata_mode_never_records_or_retains_full_ops() {
    let mut arena = NodeArena::new();
    let root = insert(&mut arena, 4);
    let (properties, generations) = identity(&arena, &[root]);
    crate::view::paint::take_full_artifact_record_count();

    let metadata = record_coverage_manifest(
        &arena,
        &[root],
        false,
        true,
        CoverageRecordingMode::MetadataOnly,
        &properties,
        &generations,
    );
    assert_eq!(crate::view::paint::take_full_artifact_record_count(), 0);
    assert!(matches!(
        metadata.items.as_slice(),
        [PaintCoverageItem::ArtifactChunk { ops: None, .. }]
    ));

    let full = record_coverage_manifest(
        &arena,
        &[root],
        false,
        true,
        CoverageRecordingMode::FullArtifact,
        &properties,
        &generations,
    );
    assert_eq!(crate::view::paint::take_full_artifact_record_count(), 1);
    assert!(matches!(
        full.items.as_slice(),
        [PaintCoverageItem::ArtifactChunk { ops: Some(_), .. }]
    ));
}

#[test]
fn validation_reports_missing_duplicate_key_and_stable_id() {
    let mut arena = NodeArena::new();
    let a = insert(&mut arena, 40);
    let b = insert(&mut arena, 40);
    let manifest = record_coverage_manifest(
        &arena,
        &[a, a, b, NodeKey::null()],
        false,
        true,
        CoverageRecordingMode::MetadataOnly,
        &PropertyTrees::default(),
        &PaintGenerationTracker::default(),
    );
    assert!(
        manifest
            .validation_errors
            .contains(&PaintCoverageValidationError::DuplicateNodeKey(a))
    );
    assert!(
        manifest
            .validation_errors
            .contains(&PaintCoverageValidationError::DuplicateStableId(40))
    );
    assert!(
        manifest
            .validation_errors
            .contains(&PaintCoverageValidationError::MissingNode(NodeKey::null()))
    );
    assert_eq!(manifest.stats().total_nodes, 2);
}
