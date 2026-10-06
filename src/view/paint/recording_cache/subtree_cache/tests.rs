use super::*;
use crate::view::test_support::commit_element;

#[test]
fn replay_checks_transitive_property_endpoints_outside_arena_ancestry() {
    let mut arena = NodeArena::new();
    let owner = commit_element(&mut arena, Box::new(Element::new(0., 0., 10., 10.)));
    let external = commit_element(&mut arena, Box::new(Element::new(0., 0., 10., 10.)));
    let mut trees = PropertyTrees::default();
    trees.effects.insert(
        EffectNodeId(owner),
        EffectNode {
            owner,
            parent: Some(EffectNodeId(external)),
            opacity: 0.5,
            generation: 1,
        },
    );
    trees.effects.insert(
        EffectNodeId(external),
        EffectNode {
            owner: external,
            parent: None,
            opacity: 0.75,
            generation: 1,
        },
    );
    let mut cache = SubtreeCache::default();
    cache.bind(&arena, &[owner]);
    let (key, members) = cache
        .key(
            &arena,
            owner,
            &PaintRecordingContext::default(),
            0,
            &[],
            false,
            &FxHashMap::default(),
        )
        .unwrap();
    let generations = crate::view::compositor::PaintGenerationTracker::default();
    cache.entries.insert(
        owner,
        (
            key.clone(),
            Arc::new(Snapshot {
                owner,
                key: key.clone(),
                completed: Default::default(),
                items: Arc::from([]),
                states: vec![],
                scopes: vec![],
                metadata: vec![],
                properties: property_closure(&trees, &members).into_iter().collect(),
                generations: FxHashMap::default(),
            }),
            false,
            trees.property_store_stamp(),
            generations.generation_store_stamp(),
        ),
    );
    assert!(cache.replay(owner, &key, &trees, &generations).is_some());
    // Neither arena access nor any revision counter announces this change.
    trees
        .effects
        .get_mut(&EffectNodeId(external))
        .unwrap()
        .opacity = 0.25;
    assert!(cache.replay(owner, &key, &trees, &generations).is_none());
    // Malformed cycles terminate; missing/new endpoints remain observations.
    trees
        .effects
        .get_mut(&EffectNodeId(external))
        .unwrap()
        .parent = Some(EffectNodeId(owner));
    assert_eq!(property_closure(&trees, &[owner]).len(), 2);
}

#[test]
fn completed_commands_reject_intervening_native_ancestor_or_lookup_mutation() {
    for mutation in 0..3 {
        let mut arena = NodeArena::new();
        let parent = commit_element(&mut arena, Box::new(Element::new(0., 0., 10., 10.)));
        let owner = crate::view::test_support::commit_child(
            &mut arena,
            parent,
            Box::new(Element::new(0., 0., 5., 5.)),
        );
        let mut cache = SubtreeCache::default();
        cache.bind(&arena, &[parent]);
        let (key, _) = cache
            .key(
                &arena,
                owner,
                &PaintRecordingContext::default(),
                0,
                &[0],
                false,
                &FxHashMap::default(),
            )
            .unwrap();
        // The command contents are irrelevant to this second mutation guard;
        // full recording and state comparisons are covered by integration tests.
        let snapshot = Snapshot {
            owner,
            key,
            completed: Default::default(),
            items: Arc::from([]),
            states: vec![],
            scopes: vec![],
            metadata: vec![],
            properties: FxHashMap::default(),
            generations: FxHashMap::default(),
        };
        let commands: Arc<[PaintCoverageItem]> = Arc::from([]);
        let observed_clock = arena.mutation_clock();
        assert!(
            snapshot
                .completed_commands(&arena, observed_clock)
                .is_none()
        );
        snapshot.remember_completed_commands(commands.clone());
        assert!(Arc::ptr_eq(
            &snapshot.completed_commands(&arena, observed_clock).unwrap(),
            &commands
        ));
        match mutation {
            0 => drop(arena.get_mut(owner)),
            1 => drop(arena.get_mut(parent)),
            _ => {
                commit_element(
                    &mut arena,
                    Box::new(Element::new_with_id(0x9911, 0., 0., 5., 5.)),
                );
            }
        }
        assert!(
            snapshot
                .completed_commands(&arena, observed_clock)
                .is_none()
        );
    }
}

#[test]
fn repeated_native_validation_rechecks_topology_and_external_id_aliases() {
    let mut arena = NodeArena::new();
    let parent = commit_element(
        &mut arena,
        Box::new(Element::new_with_id(0x9980, 0., 0., 10., 10.)),
    );
    let child = crate::view::test_support::commit_child(
        &mut arena,
        parent,
        Box::new(Element::new(0., 0., 5., 5.)),
    );
    let mut cache = SubtreeCache::default();
    cache.bind(&arena, &[parent]);
    assert!(cache.tracked_local(&arena, parent));
    assert!(cache.tracked_local(&arena, parent));
    arena.set_parent(child, None);
    assert!(!cache.tracked_local(&arena, parent));
    arena.set_parent(child, Some(parent));
    assert!(cache.tracked_local(&arena, parent));
    let alias = commit_element(
        &mut arena,
        Box::new(Element::new_with_id(0x9980, 0., 0., 1., 1.)),
    );
    assert!(!cache.tracked_local(&arena, parent));
    arena.remove_subtree(alias);
    arena.refresh_stable_id_index();
    assert!(cache.tracked_local(&arena, parent));
    cache.finish(false);
    assert!(cache.local_validations.is_empty());
}
