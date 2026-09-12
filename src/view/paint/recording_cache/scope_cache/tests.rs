use super::*;
use crate::view::paint::{FrameArtifactRecordOutcome, record_surface_dag_frame_artifact_cached};

#[test]
fn effect_store_patch_requires_consistent_values_at_every_occurrence() {
    for case in 0..5 {
        let (arena, root, properties, generations) = crate::view::paint::tests::prepared_leaf(
            0xfeed_a001,
            crate::style::Color::rgb(255, 0, 0),
            0.5,
            false,
        );
        let mut cache = RecordingCache::default();
        let FrameArtifactRecordOutcome::Artifact { artifact, .. } =
            record_surface_dag_frame_artifact_cached(
                &arena,
                &[root],
                &properties,
                &generations,
                &mut cache,
            )
            .unwrap()
        else {
            panic!("fixture must record an artifact");
        };
        let old = cache.scope_store.as_ref().unwrap().key.0[0].clone();
        // Duplicate observations model an owner's before/after paint phases.
        // The same owner key is insufficient proof that both values agree.
        cache.remember_scope_store(ScopeStoreKey(vec![old.clone(), old.clone()]), &artifact);
        let metadata = cache.entries[&root].metadata.before_children[0].clone();
        let edited = |opacity| {
            let mut owner = old.0.clone();
            for effects in &mut Arc::make_mut(&mut owner).effects {
                let mut values = effects.to_vec();
                assert!(!values.is_empty());
                for effect in &mut values {
                    effect.opacity = opacity;
                    effect.generation += 1;
                }
                *effects = values.into();
            }
            (owner.clone(), old.1.clone(), owner.effects[0].clone())
        };
        let changed = edited(0.25);
        let observation = |(owner_scope, clip_snapshot, effect_snapshot)| {
            crate::view::paint::coverage_manifest::PaintCoverageItem::ArtifactChunk {
                order: Default::default(),
                chunk: metadata.clone(),
                owner_scope,
                clip_snapshot,
                effect_snapshot,
                ops: None,
            }
        };
        let second = match case {
            0 => changed.clone(),
            1 => old.clone(),
            2 => edited(0.75),
            3 => (old.0.clone(), old.1.clone(), changed.2.clone()),
            4 => {
                let mut changed = changed.clone();
                Arc::make_mut(&mut changed.0).parent = Some(old.0.clone());
                changed
            }
            _ => unreachable!(),
        };
        let mut manifest = PaintCoverageManifest::default();
        manifest.items = vec![observation(changed), observation(second)];
        let mut output = PaintArtifact::default();
        assert_eq!(
            cache.replay_scope_store(&manifest, &mut output),
            case == 0,
            "case {case}"
        );
        if case == 0 {
            assert_eq!(output.effect_nodes[0].opacity, 0.25);
            assert_eq!(cache.scope_store_effect_updates, 1);
        } else {
            assert!(
                output.effect_nodes.is_empty(),
                "failed proof publishes no partial store"
            );
            assert_eq!(cache.scope_store_effect_updates, 0);
        }
    }
}
