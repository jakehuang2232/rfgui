use super::*;

#[test]
fn small_and_spilled_slot_schedules_match_full_set_duplicate_checks() {
    let mut actual = SeenChunkSlots::default();
    let mut expected = FxHashSet::default();
    for phase in [
        PaintNodePhase::BeforeChildren,
        PaintNodePhase::AfterChildren,
    ] {
        for slot in [0, 1, 2, 3, 4, 5, u16::MAX, 0, 4, u16::MAX, 3] {
            assert_eq!(actual.insert((phase, slot)), expected.insert((phase, slot)));
        }
    }
    // A spill must not forget an earlier inline key or confuse the two phases.
    for key in expected {
        assert!(!actual.insert(key));
    }
}

#[test]
fn chunk_orders_share_only_their_owners_immutable_path() {
    let owner = CoverageOrder::node(2, &[3, 7]);
    let before = owner.for_chunk(PaintNodePhase::BeforeChildren, 0);
    let after = owner.for_chunk(PaintNodePhase::AfterChildren, 1);
    assert!(std::sync::Arc::ptr_eq(
        &before.child_path,
        &after.child_path
    ));
    assert_eq!(
        before,
        CoverageOrder::chunk(2, &[3, 7], PaintNodePhase::BeforeChildren, 0)
    );
    assert_eq!(
        after,
        CoverageOrder::chunk(2, &[3, 7], PaintNodePhase::AfterChildren, 1)
    );
    assert_ne!(
        after,
        CoverageOrder::chunk(2, &[3, 8], PaintNodePhase::AfterChildren, 1)
    );
}
