use super::*;

fn scene(anchor_first: bool) -> (NodeArena, Vec<NodeKey>, NodeKey, NodeKey) {
    let (mut arena, roots, owner) = nested_anchor_parent_mixed_siblings(anchor_first);
    let child = commit_child(
        &mut arena,
        owner,
        Box::new(leaf_element(0xc1_3001, Color::rgb(0, 255, 0), 1.0, false)),
    );
    let mut viewport = crate::view::viewport::Viewport::new();
    crate::view::viewport::layout_artifact_style_scene_for_test(
        &mut viewport,
        &mut arena,
        roots[0],
        [320.0, 240.0],
    );
    (arena, roots, owner, child)
}

#[test]
fn generic_subtree_self_clip_records_in_phase_order_for_either_arena_order() {
    for anchor_first in [false, true] {
        let (arena, roots, owner, child) = scene(anchor_first);
        let (trees, generations) = sync_identity(&arena, &roots);
        let node = arena.get(owner).unwrap();
        // The previous leaf/deferred route must not acquire subtree authority.
        assert_eq!(
            node.element
                .exact_retained_self_clip_geometry(owner, &arena, false),
            None
        );
        let result = record_surface_dag_frame_artifact(&arena, &roots, &trees, &generations);
        let artifact = result;
        let clip = ClipNodeId {
            owner,
            role: ClipNodeRole::SelfClip,
        };
        assert_eq!(trees.node_state_for(owner).unwrap().paint.clip, Some(clip));
        assert_eq!(
            trees.node_state_for(owner).unwrap().descendants.clip,
            Some(clip)
        );
        assert_eq!(trees.paint_state_for(child).unwrap().clip, Some(clip));
        let owners: Vec<_> = artifact
            .chunks
            .iter()
            // Scope markers can create empty chunks with the same owner.
            // Count actual colored commands, including repeats, instead.
            .flat_map(|chunk| {
                artifact.ops[chunk.op_range.clone()]
                    .iter()
                    .filter_map(move |op| {
                        matches!(op, PaintOp::DrawRect(rect) if rect.params.fill_color[3] > 0.0)
                            .then_some(chunk.owner)
                    })
            })
            .collect();
        let normal = arena
            .children_of(roots[0])
            .iter()
            .copied()
            .find(|&sibling| sibling != owner)
            .unwrap();
        assert_eq!(
            owners,
            vec![normal, owner, child],
            "normal sibling, overflow owner and its subtree each record once in phase order"
        );
        assert_eq!(
            artifact
                .chunks
                .iter()
                .find(|chunk| chunk.owner == child)
                .unwrap()
                .properties
                .clip,
            Some(clip)
        );
    }
}
