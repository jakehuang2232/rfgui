use super::*;

#[test]
fn malformed_metadata_properties_and_revision_fail_preflight_without_full_hooks() {
    for (malformed, expected) in [
        (
            MalformedChunk::MetadataProperties,
            PaintCoverageValidationError::InvalidChunkProperties
                as fn(NodeKey) -> PaintCoverageValidationError,
        ),
        (
            MalformedChunk::MetadataRevision,
            PaintCoverageValidationError::InvalidChunkRevision,
        ),
    ] {
        let (arena, root, full_records, properties, generations) = malformed_host(malformed);
        let error = record_frame_artifact(
            &arena,
            &[root],
            &properties,
            &generations,
            RendererMode::ForcedForTests,
        )
        .expect_err("malformed metadata must fail preflight");
        assert_eq!(full_records.load(Ordering::Relaxed), 0);
        assert!(
            error
                .reasons
                .contains(&FrameArtifactFallbackReason::Validation(expected(root)))
        );
    }
}

#[test]
fn malformed_metadata_bounds_fail_preflight_without_full_hooks() {
    for malformed in [
        MalformedChunk::MetadataNaNBounds,
        MalformedChunk::MetadataNegativeBounds,
    ] {
        let (arena, root, full_records, properties, generations) = malformed_host(malformed);
        let error = record_frame_artifact(
            &arena,
            &[root],
            &properties,
            &generations,
            RendererMode::ForcedForTests,
        )
        .expect_err("non-canonical metadata bounds must fail preflight");
        assert_eq!(full_records.load(Ordering::Relaxed), 0);
        assert!(
            error
                .reasons
                .contains(&FrameArtifactFallbackReason::Validation(
                    PaintCoverageValidationError::InvalidChunkBounds(root)
                ))
        );
    }
}

#[test]
fn malformed_full_owner_properties_revision_and_range_fail_closed() {
    for (malformed, expected) in [
        (
            MalformedChunk::FullOwner,
            PaintCoverageValidationError::InvalidChunkIdOwner
                as fn(NodeKey) -> PaintCoverageValidationError,
        ),
        (
            MalformedChunk::FullChunkOwner,
            PaintCoverageValidationError::InvalidChunkOwner,
        ),
        (
            MalformedChunk::FullProperties,
            PaintCoverageValidationError::InvalidChunkProperties,
        ),
        (
            MalformedChunk::FullRevision,
            PaintCoverageValidationError::InvalidChunkRevision,
        ),
    ] {
        let (arena, root, full_records, properties, generations) = malformed_host(malformed);
        let error = record_frame_artifact(
            &arena,
            &[root],
            &properties,
            &generations,
            RendererMode::ForcedForTests,
        )
        .expect_err("malformed full chunk must fail closed");
        assert_eq!(full_records.load(Ordering::Relaxed), 1);
        assert!(
            error
                .reasons
                .contains(&FrameArtifactFallbackReason::Validation(expected(root)))
        );
    }

    let (arena, root, full_records, properties, generations) =
        malformed_host(MalformedChunk::FullRange);
    let error = record_frame_artifact(
        &arena,
        &[root],
        &properties,
        &generations,
        RendererMode::ForcedForTests,
    )
    .expect_err("malformed range must fail closed");
    assert_eq!(full_records.load(Ordering::Relaxed), 1);
    assert!(
        error
            .reasons
            .contains(&FrameArtifactFallbackReason::Validation(
                PaintCoverageValidationError::InvalidArtifactOpRange {
                    node: root,
                    start: 1,
                    end: 0,
                    op_count: 0,
                }
            ))
    );
}

#[test]
fn canonical_preflight_full_mismatch_fails_closed() {
    let (arena, root, full_records, properties, generations) =
        malformed_host(MalformedChunk::FullBounds);
    let error = record_frame_artifact(
        &arena,
        &[root],
        &properties,
        &generations,
        RendererMode::ForcedForTests,
    )
    .expect_err("canonical identity drift must fail closed");
    assert_eq!(full_records.load(Ordering::Relaxed), 1);
    assert_eq!(
        error.reasons,
        vec![FrameArtifactFallbackReason::Validation(
            PaintCoverageValidationError::RecordingPassMismatch
        )]
    );
}

#[test]
fn compiler_validates_entire_store_before_emitting_any_pass() {
    let (arena, root, properties, generations) =
        prepared_leaf(46, Color::rgb(20, 30, 40), 1.0, false);
    let PaintRecordOutcome::Artifact(mut artifact) =
        record_root(&arena, root, &properties, &generations)
    else {
        panic!("safe leaf must record")
    };
    let mut malformed_late_chunk = artifact.chunks[0].clone();
    malformed_late_chunk.op_range = 2..1;
    artifact.chunks.push(malformed_late_chunk);

    let graph = compiled_whole_frame_graph(&artifact);
    assert!(
        graph.test_rect_pass_snapshots().is_empty(),
        "a malformed later chunk must not leave an earlier partial pass"
    );
}
