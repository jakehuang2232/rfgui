use super::*;

#[test]
fn receiver_clip_keeps_owner_scope_without_reintroducing_consumed_ancestors() {
    let mut keys = slotmap::SlotMap::<NodeKey, ()>::with_key();
    let owner = keys.insert(());
    let descendant = keys.insert(());
    let outer = ClipNodeId {
        owner,
        role: ClipNodeRole::SelfClip,
    };
    let inner = ClipNodeId {
        owner: descendant,
        role: ClipNodeRole::SelfClip,
    };
    let mut clips = FxHashMap::default();
    for (id, parent) in [(outer, None), (inner, Some(outer))] {
        clips.insert(
            id,
            ClipNodeSnapshot {
                id,
                owner: id.owner,
                parent,
                logical_scissor: [0, 0, 20, 16],
                behavior: ClipBehavior::Replace,
                generation: 1,
            },
        );
    }
    assert_eq!(
        artifact_surface_receiver_clip(Some(outer), Some(inner), &clips),
        Some(Some(outer))
    );
    assert_eq!(
        artifact_surface_receiver_clip(None, Some(inner), &clips),
        Some(None),
        "a descendant self clip cannot become the ancestor surface's composite clip"
    );
    assert_eq!(
        artifact_surface_receiver_clip(Some(outer), None, &clips),
        Some(None),
        "an ancestor clip already consumed by detachment must stay consumed"
    );
    assert_eq!(
        artifact_surface_receiver_clip(Some(inner), Some(inner), &clips),
        Some(Some(inner))
    );
    clips.get_mut(&outer).unwrap().parent = Some(inner);
    assert_eq!(
        artifact_surface_receiver_clip(Some(outer), Some(inner), &clips),
        None
    );
    clips.remove(&outer);
    assert_eq!(
        artifact_surface_receiver_clip(Some(outer), Some(inner), &clips),
        None
    );
}
