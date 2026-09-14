use super::*;
use std::cell::Cell;
use std::rc::Rc;

#[derive(Debug)]
struct Counted(u32, Rc<Cell<usize>>);
impl Clone for Counted {
    fn clone(&self) -> Self {
        self.1.set(self.1.get() + 1);
        Self(self.0, self.1.clone())
    }
}

#[test]
fn shared_readers_do_not_copy_and_mutation_detaches_old_snapshots() {
    let copies = Rc::new(Cell::new(0));
    let block = |values: Range<u32>| {
        values
            .map(|v| Counted(v, copies.clone()))
            .collect::<Vec<_>>()
            .into()
    };
    let mut sequence = SharedSequence::new();
    sequence.append_shared(block(0..3));
    sequence.append_shared(block(3..6));
    let mut copy = sequence.clone();
    assert_eq!(
        sequence
            .get(1..3)
            .unwrap()
            .iter()
            .map(|v| v.0)
            .collect::<Vec<_>>(),
        [1, 2]
    );
    let mut iter = sequence.iter();
    assert_eq!(iter.len(), 6);
    assert_eq!(iter.next().unwrap().0, 0);
    assert_eq!(iter.next_back().unwrap().0, 5);
    assert_eq!(iter.len(), 4);
    assert_eq!(
        copies.get(),
        0,
        "shared clone, iteration and contained ranges stay borrowed"
    );
    assert!(sequence.flattened.get().is_none());
    copy[2].0 = 99;
    assert_eq!(copy[2].0, 99);
    assert_eq!(sequence[2].0, 2);
    assert_eq!(copies.get(), 6);
    assert!(sequence.get(6).is_none());
    assert!(sequence.get(6..6).unwrap().is_empty());
    assert!(sequence.get(4..3).is_none());
    assert!(sequence.get(..=usize::MAX).is_none());
    assert_eq!(
        sequence
            .get(2..5)
            .unwrap()
            .iter()
            .map(|v| v.0)
            .collect::<Vec<_>>(),
        [2, 3, 4]
    );
    assert_eq!(
        sequence[2].0, 2,
        "flattening a read never adopts another copy's edits"
    );
}

#[test]
fn equality_skips_only_shared_blocks_and_preserves_edited_or_repartitioned_values() {
    let mut first = SharedSequence::from(Arc::<[u32]>::from([1, 2, 3]));
    first.append_shared(Arc::from([4, 5]));
    let mut second = first.clone();
    let calls = Cell::new(0);
    assert!(first.equivalent_with(&second, |a, b| {
        calls.set(calls.get() + 1);
        a == b
    }));
    assert_eq!(calls.get(), 0);
    second[3] = 8;
    assert_ne!(first, second);
    let mut partitioned = SharedSequence::from(Arc::<[u32]>::from([1, 2]));
    partitioned.append_shared(Arc::from([3, 4, 5]));
    assert_eq!(first, partitioned);
    assert_eq!(first.into_iter().collect::<Vec<_>>(), [1, 2, 3, 4, 5]);
}

#[test]
fn append_after_snapshot_detaches_schedule_without_copying_payloads() {
    let copies = Rc::new(Cell::new(0));
    let mut original = SharedSequence::new();
    for value in 0..8 {
        original.append_shared(vec![Counted(value, copies.clone())].into());
    }
    let mut appended = original.clone();
    let (Storage::Shared(old), Storage::Shared(new)) = (&original.storage, &appended.storage)
    else {
        panic!("shared snapshots");
    };
    assert!(Arc::ptr_eq(old, new));
    appended.append_shared(vec![Counted(8, copies.clone()), Counted(9, copies.clone())].into());
    assert_eq!(copies.get(), 0);
    assert_eq!(original.len(), 8);
    assert!(original.get(8).is_none());
    assert_eq!(appended.len(), 10);
    assert_eq!(
        appended
            .get(8..10)
            .unwrap()
            .iter()
            .map(|v| v.0)
            .collect::<Vec<_>>(),
        [8, 9]
    );
    assert_eq!(original.into_blocks().len(), 8);
    assert_eq!(
        appended
            .get(0..2)
            .unwrap()
            .iter()
            .map(|v| v.0)
            .collect::<Vec<_>>(),
        [0, 1]
    );
    appended.clear();
    assert!(appended.iter().next().is_none());
}

#[test]
fn block_range_view_never_flattens_or_clones_payloads() {
    let copies = Rc::new(Cell::new(0));
    let mut sequence = SharedSequence::new();
    for n in 0..4 {
        sequence.append_shared(vec![Counted(n, copies.clone())].into());
    }
    let view = sequence.view(1..4).unwrap();
    assert_eq!(view.iter().map(|v| v.0).collect::<Vec<_>>(), [1, 2, 3]);
    assert_eq!(
        view.iter().rev().map(|v| v.0).collect::<Vec<_>>(),
        [3, 2, 1]
    );
    assert_eq!(view.first().unwrap().0, 1);
    assert_eq!(view.last().unwrap().0, 3);
    assert_eq!(copies.get(), 0);
    assert!(sequence.flattened.get().is_none());
    assert!(sequence.view(4..3).is_none());
    assert!(sequence.view(0..5).is_none());
}

#[test]
fn range_snapshot_retains_only_intersecting_blocks_and_survives_cow() {
    let first: Arc<[u32]> = vec![0, 1, 2].into();
    let second: Arc<[u32]> = vec![3, 4, 5].into();
    let third: Arc<[u32]> = vec![6, 7, 8].into();
    let mut sequence = SharedSequence::new();
    for block in [&first, &second, &third] {
        sequence.append_shared(block.clone());
    }
    let snapshot = sequence.view(1..5).unwrap().snapshot();
    assert_eq!(snapshot.iter().copied().collect::<Vec<_>>(), [1, 2, 3, 4]);
    assert_eq!(snapshot.blocks.len(), 2);
    assert_eq!(
        Arc::strong_count(&third),
        2,
        "unrelated block is not retained"
    );
    assert!(snapshot.shares_with(&sequence.view(1..5).unwrap()));
    assert!(!snapshot.shares_with(&sequence.view(2..6).unwrap()));
    let mut edited = sequence.clone();
    edited[2] = 99;
    assert!(!snapshot.shares_with(&edited.view(1..5).unwrap()));
    assert_eq!(snapshot.iter().copied().collect::<Vec<_>>(), [1, 2, 3, 4]);
}
