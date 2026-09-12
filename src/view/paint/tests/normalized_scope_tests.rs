use super::*;

#[test]
fn chunks_share_owner_prefixes_but_conflicting_observations_still_reject() {
    let (arena, roots, child) = prepared_plain_tree();
    let (properties, generations) = sync_identity(&arena, &roots);
    let mut manifest = record_coverage_manifest(
        &arena,
        &roots,
        false,
        true,
        CoverageRecordingMode::FullArtifact,
        &properties,
        &generations,
    );
    let scopes = manifest
        .items
        .iter()
        .filter_map(|item| match item {
            PaintCoverageItem::ArtifactChunk {
                chunk, owner_scope, ..
            } => Some((chunk.owner, owner_scope.clone())),
            _ => None,
        })
        .collect::<std::collections::HashMap<_, _>>();
    let root_scope = &scopes[&roots[0]];
    let child_scope = &scopes[&child];
    assert!(
        std::sync::Arc::ptr_eq(root_scope, child_scope.parent.as_ref().unwrap()),
        "descendants must share the actual immutable ancestor observation"
    );
    let accepted = super::super::frame_recorder::materialize_frame_artifact(
        manifest.clone(),
        PaintArtifactTarget::CurrentTarget,
        RendererMode::Auto,
        FrameArtifactEligibility {
            eligible: true,
            ..Default::default()
        },
    )
    .unwrap();
    let FrameArtifactRecordOutcome::Artifact { artifact, .. } = accepted else {
        panic!("valid shared scopes must materialize");
    };
    assert_eq!(
        artifact.owner_nodes.len(),
        3,
        "each owner is stored exactly once"
    );
    assert_eq!(artifact.owner_property_states.len(), 3);

    let PaintCoverageItem::ArtifactChunk { owner_scope, .. } = manifest.items.iter_mut().find(|item|
        matches!(item, PaintCoverageItem::ArtifactChunk { chunk, .. } if chunk.owner == child)
    ).unwrap() else { unreachable!() };
    // A distinct allocation with the same owner is not an already-validated
    // shared prefix. The materializer must compare it against the earlier root.
    let parent = std::sync::Arc::make_mut(owner_scope)
        .parent
        .as_mut()
        .unwrap();
    std::sync::Arc::make_mut(parent).topology.parent = Some(child);
    let outcome = super::super::frame_recorder::materialize_frame_artifact(
        manifest,
        PaintArtifactTarget::CurrentTarget,
        RendererMode::Auto,
        FrameArtifactEligibility {
            eligible: true,
            ..Default::default()
        },
    )
    .unwrap();
    let FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility) = outcome else {
        panic!("same owner with conflicting parent must reject");
    };
    assert!(
        eligibility
            .reasons
            .contains(&FrameArtifactFallbackReason::Validation(
                PaintCoverageValidationError::ConflictingOwnerSnapshot(roots[0])
            ))
    );
}

#[test]
fn shared_effect_chains_do_not_hide_a_separate_conflicting_observation() {
    let (arena, root, properties, generations) =
        prepared_leaf(0xfeed_9081, Color::rgb(21, 43, 65), 0.5, false);
    let mut manifest = record_coverage_manifest(
        &arena,
        &[root],
        false,
        true,
        CoverageRecordingMode::FullArtifact,
        &properties,
        &generations,
    );
    let PaintCoverageItem::ArtifactChunk {
        owner_scope,
        effect_snapshot,
        ..
    } = manifest
        .items
        .iter_mut()
        .find(|item| matches!(item, PaintCoverageItem::ArtifactChunk { .. }))
        .unwrap()
    else {
        unreachable!()
    };
    assert_eq!(effect_snapshot.len(), 1);
    assert!(Arc::ptr_eq(effect_snapshot, &owner_scope.effects[0]));
    let id = effect_snapshot[0].id;
    let scope = Arc::make_mut(owner_scope);
    Arc::make_mut(&mut scope.effects[0])[0].opacity = 0.25;
    let outcome = super::super::frame_recorder::materialize_frame_artifact(
        manifest,
        PaintArtifactTarget::CurrentTarget,
        RendererMode::Auto,
        FrameArtifactEligibility {
            eligible: true,
            ..Default::default()
        },
    )
    .unwrap();
    let FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility) = outcome else {
        panic!("separately allocated conflicting chain must not be skipped");
    };
    assert!(
        eligibility
            .reasons
            .contains(&FrameArtifactFallbackReason::Validation(
                PaintCoverageValidationError::ConflictingEffectSnapshot(id)
            ))
    );
}
