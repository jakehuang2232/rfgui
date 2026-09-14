use super::*;

#[test]
fn warm_owner_scopes_reject_missing_or_cyclic_live_endpoints_and_recover() {
    for damage in 0..4 {
        let mut arena = new_test_arena();
        let root = commit_element(
            &mut arena,
            Box::new(leaf_element(
                0xfeed_9082,
                Color::rgb(21, 43, 65),
                0.5,
                false,
            )),
        );
        let mut child_element = leaf_element(0xfeed_9083, Color::rgb(65, 43, 21), 1.0, false);
        let mut child_style = Style::new();
        child_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(0.0))
                    .top(Length::px(0.0))
                    .clip(ClipMode::AnchorParent),
            ),
        );
        child_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::rgb(65, 43, 21)),
        );
        child_element.apply_style(child_style);
        let child = commit_child(&mut arena, root, Box::new(child_element));
        let (measure, place) = constraints();
        measure_and_place(&mut arena, root, measure, place);
        let (mut properties, generations) = sync_identity(&arena, &[root]);
        let state = properties.node_state_for(child).unwrap().paint;
        let clip = state.clip.expect("fixture has an explicit self clip");
        let effect = state.effect.expect("fixture has inherited opacity");
        let good_clip = properties.clips[&clip].clone();
        let good_effect = properties.effects[&effect].clone();
        let mut cache = RecordingCache::default();
        for _ in 0..2 {
            let outcome = record_surface_dag_frame_artifact_cached(
                &arena,
                &[root],
                &properties,
                &generations,
                &mut cache,
            )
            .unwrap();
            assert!(
                matches!(outcome, FrameArtifactRecordOutcome::Artifact { .. }),
                "{outcome:?}"
            );
        }
        assert!(
            cache.scope_hits > 0,
            "fixture must reuse immutable owner scopes"
        );
        match damage {
            0 => {
                properties.clips.remove(&clip);
            }
            1 => properties.clips.get_mut(&clip).unwrap().parent = Some(clip),
            2 => {
                properties.effects.remove(&effect);
            }
            3 => properties.effects.get_mut(&effect).unwrap().parent = Some(effect),
            _ => unreachable!(),
        }
        for outcome in [
            record_surface_dag_frame_artifact_cached(
                &arena,
                &[root],
                &properties,
                &generations,
                &mut cache,
            )
            .unwrap(),
            record_surface_dag_frame_artifact(
                &arena,
                &[root],
                &properties,
                &generations,
                RendererMode::Auto,
            )
            .unwrap(),
        ] {
            assert!(
                matches!(
                    outcome,
                    FrameArtifactRecordOutcome::WholeFrameLegacyFallback(_)
                ),
                "warm scope must not hide live endpoint damage {damage}"
            );
        }
        properties.clips.insert(clip, good_clip);
        properties.effects.insert(effect, good_effect);
        assert!(
            matches!(
                record_surface_dag_frame_artifact_cached(
                    &arena,
                    &[root],
                    &properties,
                    &generations,
                    &mut cache,
                )
                .unwrap(),
                FrameArtifactRecordOutcome::Artifact { .. }
            ),
            "repaired endpoints must recover after damage {damage}"
        );
    }
}

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
