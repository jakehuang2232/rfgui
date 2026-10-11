use super::*;
use crate::view::paint::PaintChunkId;
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
fn child_mask_action_is_derived_only_from_reserved_slot_and_phase() {
    assert_eq!(
        ArtifactSurfaceChildMaskAction::from_chunk_id(chunk_id(PaintNodePhase::BeforeChildren, 0,)),
        ArtifactSurfaceChildMaskAction::Unchanged,
    );
    assert!(matches!(
        ArtifactSurfaceChildMaskAction::from_chunk_id(chunk_id(
            PaintNodePhase::BeforeChildren,
            RETAINED_CHILD_MASK_SLOT,
        )),
        ArtifactSurfaceChildMaskAction::Push(Some(GraphicsPassScissor::Logical([0, 0, 1, 1]))),
    ));
    assert_eq!(
        ArtifactSurfaceChildMaskAction::from_chunk_id(chunk_id(
            PaintNodePhase::AfterChildren,
            RETAINED_CHILD_MASK_SLOT,
        )),
        ArtifactSurfaceChildMaskAction::Pop,
    );
}
