use super::*;
use slotmap::SlotMap;

fn chunk_id(phase: PaintNodePhase, slot: u16) -> PaintChunkId {
    let mut owners = SlotMap::<crate::view::node_arena::NodeKey, ()>::with_key();
    PaintChunkId {
        owner: owners.insert(()),
        scope: crate::view::paint::PaintPropertyScope::Contents,
        phase,
        slot,
        role: crate::view::paint::PaintChunkRole::SelfDecoration,
    }
}

#[test]
fn child_mask_depth_derivation_preserves_carried_scope_and_saturating_phase_rules() {
    let ids = [
        chunk_id(PaintNodePhase::AfterChildren, 0),
        chunk_id(PaintNodePhase::BeforeChildren, RETAINED_CHILD_MASK_SLOT),
        chunk_id(PaintNodePhase::BeforeChildren, RETAINED_CHILD_MASK_SLOT),
        chunk_id(PaintNodePhase::AfterChildren, RETAINED_CHILD_MASK_SLOT),
        chunk_id(PaintNodePhase::AfterChildren, RETAINED_CHILD_MASK_SLOT),
        chunk_id(PaintNodePhase::AfterChildren, RETAINED_CHILD_MASK_SLOT),
    ];
    assert_eq!(artifact_child_mask_max_depth(ids, 0), 2);
    assert_eq!(artifact_child_mask_max_depth(ids, 7), 9);
    assert_eq!(artifact_child_mask_max_depth([], 7), 0);
}

#[test]
fn child_mask_action_is_derived_only_from_reserved_slot_and_phase() {
    assert_eq!(
        ArtifactSurfaceChildMaskAction::from_chunk_id(chunk_id(PaintNodePhase::BeforeChildren, 0,)),
        ArtifactSurfaceChildMaskAction::Unchanged,
    );
    assert_eq!(
        ArtifactSurfaceChildMaskAction::from_chunk_id(chunk_id(
            PaintNodePhase::BeforeChildren,
            RETAINED_CHILD_MASK_SLOT,
        )),
        ArtifactSurfaceChildMaskAction::Push,
    );
    assert_eq!(
        ArtifactSurfaceChildMaskAction::from_chunk_id(chunk_id(
            PaintNodePhase::AfterChildren,
            RETAINED_CHILD_MASK_SLOT,
        )),
        ArtifactSurfaceChildMaskAction::Pop,
    );
}
