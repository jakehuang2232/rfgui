use super::*;

#[test]
fn current_order_changes_remain_visible_and_unseen_storage_is_released() {
    let (_, owner, _, _) = crate::view::paint::tests::prepared_leaf(
        0xfeed_a002,
        crate::style::Color::rgb(255, 0, 0),
        1.0,
        false,
    );
    let mut cache = RecordingCache::default();
    cache.begin();
    let first = cache.intern_order_path(owner, &[0, 2]);
    cache.finish(true);
    cache.begin();
    let warm = cache.intern_order_path(owner, &[0, 2]);
    assert!(Arc::ptr_eq(&first, &warm));
    let changed = cache.intern_order_path(owner, &[2, 0]);
    assert_ne!(
        first, changed,
        "a second recording still observes order drift"
    );
    assert_eq!(
        first.as_ref(),
        &[0, 2],
        "old observations must stay immutable"
    );
    assert_eq!(changed.as_ref(), &[2, 0]);
    let retained = Arc::downgrade(&changed);
    drop(changed);
    cache.finish(true);
    assert!(retained.upgrade().is_some());
    cache.begin();
    cache.finish(true);
    assert!(
        retained.upgrade().is_none(),
        "an unseen owner releases its path"
    );
    let rejected = cache.intern_order_path(owner, &[1]);
    let weak = Arc::downgrade(&rejected);
    drop(rejected);
    cache.finish(false);
    assert!(weak.upgrade().is_none());
}
