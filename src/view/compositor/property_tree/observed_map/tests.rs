use super::*;

#[test]
fn history_covers_all_mutation_paths_and_independent_readers() {
    let mut map = ObservedMap::<u32, u32>::default();
    let empty = map.stamp();
    map.insert(1, 10);
    map.insert(2, 20);
    let first = map.stamp();
    *map.get_mut(&1).unwrap() = 11;
    map.remove(&2);
    let changes = |stamp: &Stamp| {
        let mut keys = FxHashSet::default();
        map.changed_since(stamp, |key| {
            keys.insert(key);
        })
        .map(|_| keys)
    };
    assert_eq!(changes(&empty), Some([1, 2].into_iter().collect()));
    assert_eq!(changes(&first), Some([1, 2].into_iter().collect()));
    let before_retain = map.stamp();
    map.retain(|_, _| true);
    let mut changed = false;
    map.changed_since(&before_retain, |_| changed = true)
        .unwrap();
    assert!(!changed);
    map.retain(|_, _| false);
    let mut keys = Vec::new();
    map.changed_since(&before_retain, |k| keys.push(k)).unwrap();
    assert_eq!(keys, [1]);
    map.insert(3, 30);
    let before_mut = map.stamp();
    for value in map.values_mut() {
        *value += 1;
    }
    let mut keys = Vec::new();
    map.changed_since(&before_mut, |k| keys.push(k)).unwrap();
    assert_eq!(keys, [3]);
    let before_entry = map.stamp();
    *map.entry(3).or_default() = 99;
    let mut keys = Vec::new();
    map.changed_since(&before_entry, |k| keys.push(k)).unwrap();
    assert_eq!(keys, [3]);
    let before_bulk_edit = map.stamp();
    for (_, value) in map.iter_mut() {
        *value += 1;
    }
    assert!(map.changed_since(&before_bulk_edit, |_| {}).is_none());
    assert!(map.changed_since(&map.stamp(), |_| {}).is_some());
}

#[test]
fn replacement_history_loss_and_saturation_fail_closed() {
    let mut map = ObservedMap::<u32, u32>::default();
    let old = map.stamp();
    let replacement = ObservedMap::<u32, u32>::default();
    assert!(replacement.changed_since(&old, |_| {}).is_none());
    for i in 0..=HISTORY_LIMIT {
        map.insert(i as u32, 0);
    }
    assert_eq!(map.writes.len(), HISTORY_LIMIT);
    assert!(map.changed_since(&old, |_| {}).is_none());
    let fresh = map.stamp();
    map.clock = u64::MAX;
    assert!(map.changed_since(&fresh, |_| {}).is_none());
    assert!(map.changed_since(&map.stamp(), |_| {}).is_none());
}

#[test]
fn query_memo_rechecks_current_stores_and_generation_maps() {
    let mut arena = NodeArena::new();
    let owner = crate::view::test_support::commit_element(
        &mut arena,
        Box::new(crate::view::base_component::Element::new(0., 0., 10., 10.)),
    );
    let mut trees = PropertyTrees::default();
    let before = trees.property_store_stamp();
    trees.states.insert(owner, NodePropertyState::default());
    let first = trees.property_writes_since(&before).unwrap();
    assert!(first.contains(&owner));
    assert!(Arc::ptr_eq(
        &first,
        &trees.property_writes_since(&before).unwrap()
    ));
    let current = trees.property_store_stamp();
    assert!(trees.property_writes_since(&current).unwrap().is_empty());
    trees.states.remove(&owner);
    assert!(
        trees
            .property_writes_since(&current)
            .unwrap()
            .contains(&owner)
    );
    trees.states = Default::default();
    assert!(trees.property_writes_since(&current).is_none());
    let generations = trees.generation_store_stamp();
    *trees
        .effect_generations
        .entry(EffectNodeId(owner))
        .or_default() += 1;
    assert_eq!(
        trees.generation_writes_since(&generations),
        Some([owner].into_iter().collect())
    );
    let generations = trees.generation_store_stamp();
    trees.effect_generations = Default::default();
    assert!(trees.generation_writes_since(&generations).is_none());
}

#[test]
fn equal_native_observations_keep_stores_unchanged_and_repair_owner_corruption() {
    use crate::view::base_component::Element;
    use crate::view::test_support::{commit_element, get_element_mut};
    let mut arena = NodeArena::new();
    let owner = commit_element(&mut arena, Box::new(Element::new(0., 0., 10., 10.)));
    let foreign = commit_element(&mut arena, Box::new(Element::new(0., 0., 10., 10.)));
    get_element_mut::<Element>(&arena, owner).set_opacity(0.5);
    let mut trees = PropertyTrees::default();
    trees.sync(&arena, &[owner]);
    let stamp = trees.property_store_stamp();
    drop(arena.get_mut(owner)); // Force observation without changing any input.
    trees.sync(&arena, &[owner]);
    assert!(trees.observed_nodes > 0);
    assert!(trees.property_writes_since(&stamp).unwrap().is_empty());
    trees.effects.get_mut(&EffectNodeId(owner)).unwrap().owner = foreign;
    drop(arena.get_mut(owner));
    trees.sync(&arena, &[owner]);
    assert_eq!(trees.effects[&EffectNodeId(owner)].owner, owner);
    assert!(
        trees
            .property_writes_since(&stamp)
            .unwrap()
            .contains(&owner)
    );
}
