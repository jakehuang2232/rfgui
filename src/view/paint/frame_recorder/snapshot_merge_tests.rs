use super::*;

#[test]
fn snapshot_merge_rejects_conflicting_duplicate_identity() {
    let mut store = FxHashMap::default();
    assert_eq!(
        merge_snapshot(&mut store, 7_u64, 11_u64),
        SnapshotMerge::Inserted
    );
    assert_eq!(merge_snapshot(&mut store, 7, 11), SnapshotMerge::Identical);
    assert_eq!(merge_snapshot(&mut store, 7, 12), SnapshotMerge::Conflict);
    assert_eq!(
        store[&7], 11,
        "conflict must not replace the canonical first snapshot"
    );
}
