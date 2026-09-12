use super::*;
#[test]
fn shares_only_completed_suffix_and_rejects_cycles_and_missing_nodes() {
    let mut known = FxHashMap::default();
    let nodes = FxHashMap::from_iter([(1, (1, None)), (2, (2, Some(1))), (3, (3, Some(2)))]);
    let first = unseen_chain(Some(2), &known, |id| nodes.get(&id).copied(), |s| s.1).unwrap();
    assert_eq!(first, vec![(2, Some(1)), (1, None)]);
    known.extend(first.iter().copied().map(|s| (s.0, s)));
    let mut reads = Vec::new();
    let next = unseen_chain(
        Some(3),
        &known,
        |id| {
            reads.push(id);
            nodes.get(&id).copied()
        },
        |s| s.1,
    )
    .unwrap();
    assert_eq!(next, vec![(3, Some(2))]);
    assert_eq!(reads, vec![3]);
    assert!(unseen_chain(Some(9), &known, |id| nodes.get(&id).copied(), |s| s.1).is_none());
    assert!(unseen_chain(Some(9), &known, |id| Some((id, Some(id))), |s| s.1).is_none());
}
