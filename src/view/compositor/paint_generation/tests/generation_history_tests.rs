use super::*;

#[test]
fn generation_history_covers_raw_writes_removal_and_cache_refresh() {
    let mut arena = NodeArena::new();
    let root = insert_element(&mut arena, 0xfeed_9501);
    let other = insert_element(&mut arena, 0xfeed_9502);
    let mut trees = PropertyTrees::default();
    let mut tracker = PaintGenerationTracker::default();
    sync(&mut tracker, &mut trees, &arena, &[root, other]);
    let before = tracker.generation_store_stamp();
    assert!(tracker.generation_writes_since(&before).unwrap().is_empty());
    tracker.nodes.get_mut(&root).unwrap().self_paint_revision += 1;
    let changed = tracker.generation_writes_since(&before).unwrap();
    assert_eq!(changed.iter().copied().collect::<Vec<_>>(), [root]);
    assert!(std::sync::Arc::ptr_eq(
        &changed,
        &tracker.generation_writes_since(&before).unwrap()
    ));
    tracker.nodes.get_mut(&other).unwrap().active = false;
    assert_eq!(tracker.generation_writes_since(&before).unwrap().len(), 2);
    assert!(tracker.local_generations_for(other).is_none());
    let before_remove = tracker.generation_store_stamp();
    tracker.nodes.remove(&root);
    assert!(
        tracker
            .generation_writes_since(&before_remove)
            .unwrap()
            .contains(&root)
    );
    assert!(tracker.local_generations_for(root).is_none());
}

#[test]
fn foreign_lost_and_bulk_generation_history_cannot_certify_no_change() {
    let mut arena = NodeArena::new();
    let root = insert_element(&mut arena, 0xfeed_9503);
    let mut trees = PropertyTrees::default();
    let mut tracker = PaintGenerationTracker::default();
    sync(&mut tracker, &mut trees, &arena, &[root]);
    let before = tracker.generation_store_stamp();
    assert!(
        PaintGenerationTracker::default()
            .generation_writes_since(&before)
            .is_none()
    );
    for _ in 0..4097 {
        tracker.nodes.get_mut(&root).unwrap().composite_revision += 1;
    }
    assert!(tracker.generation_writes_since(&before).is_none());
    let before_bulk = tracker.generation_store_stamp();
    std::ops::DerefMut::deref_mut(&mut tracker.nodes).clear();
    assert!(tracker.generation_writes_since(&before_bulk).is_none());
    assert!(tracker.local_generations_for(root).is_none());
}

#[test]
fn observation_bookkeeping_does_not_dirty_local_generations() {
    let mut arena = NodeArena::new();
    let root = insert_element(&mut arena, 0xfeed_9504);
    let mut trees = PropertyTrees::default();
    let mut tracker = PaintGenerationTracker::default();
    sync(&mut tracker, &mut trees, &arena, &[root]);
    let before = tracker.generation_store_stamp();
    for epoch in 0..5000 {
        tracker.nodes.get_mut(&root).unwrap().last_seen_epoch = epoch;
    }
    assert!(tracker.generation_writes_since(&before).unwrap().is_empty());
    tracker.nodes.get_mut(&root).unwrap().topology_revision += 1;
    assert!(
        tracker
            .generation_writes_since(&before)
            .unwrap()
            .contains(&root)
    );
}
