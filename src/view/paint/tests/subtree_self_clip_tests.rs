use super::super::recording_context::PaintSubtreeSelfClipWitness;
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
fn generic_subtree_self_clip_records_descendants_only_with_matching_phase_order() {
    for anchor_first in [false, true] {
        let (arena, roots, owner, child) = scene(anchor_first);
        let (trees, generations) = sync_identity(&arena, &roots);
        let node = arena.get(owner).unwrap();
        // The previous leaf/deferred route must not acquire subtree authority.
        assert_eq!(
            node.element
                .exact_retained_self_clip_scissor_rect(owner, &arena, false),
            None
        );
        let witness = PaintSubtreeSelfClipWitness::from_live_owner(&arena, owner, &trees, false);
        assert_eq!(witness.is_some(), !anchor_first);
        assert!(
            matches!(
                record_clip_enabled_frame_artifact(
                    &arena,
                    &roots,
                    &trees,
                    &generations,
                    RendererMode::Auto,
                )
                .unwrap(),
                FrameArtifactRecordOutcome::WholeFrameLegacyFallback { .. }
            ),
            "the older recording policy must still reject nonempty self clips"
        );
        let result = record_surface_dag_frame_artifact(
            &arena,
            &roots,
            &trees,
            &generations,
            RendererMode::Auto,
        )
        .unwrap();
        if anchor_first {
            assert!(matches!(
                result,
                FrameArtifactRecordOutcome::WholeFrameLegacyFallback { .. }
            ));
            continue;
        }
        let FrameArtifactRecordOutcome::Artifact {
            artifact,
            eligibility,
        } = result
        else {
            panic!("ordered subtree must record without fallback");
        };
        assert!(eligibility.eligible);
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
        let normal = arena.children_of(roots[0])[0];
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

#[test]
fn generic_subtree_self_clip_rejects_foreign_state_geometry_and_missing_snapshot() {
    let (arena, roots, owner, _) = scene(false);
    let (mut trees, _) = sync_identity(&arena, &roots);
    let node = arena.get(owner).unwrap();
    let stable_id = node.element.stable_id();
    let scissor = node
        .element
        .exact_generic_subtree_self_clip_scissor_rect(owner, &arena, false)
        .unwrap();
    let state = trees.paint_state_for(owner).unwrap();
    let witness =
        PaintSubtreeSelfClipWitness::from_live_owner(&arena, owner, &trees, false).unwrap();
    let recorded = trees.node_state_for(owner).unwrap();
    assert!(witness.matches_recorded_scopes(recorded.paint, recorded.descendants));
    assert!(!witness.matches_recorded_scopes(recorded.paint, PropertyTreeState::default()));
    let context = PaintRecordingContext {
        recording_owner: Some(owner),
        recording_owner_stable_id: Some(stable_id),
        surface_dag: true,
        surface_dag_paint_state: Some(state),
        authoritative_self_clip: state.clip,
        subtree_self_clip: Some(witness),
        ..Default::default()
    };
    assert!(context.authorizes_subtree_self_clip_for(stable_id, scissor));
    for altered in [
        PaintRecordingContext {
            recording_owner: Some(roots[0]),
            ..context
        },
        PaintRecordingContext {
            recording_owner_stable_id: Some(stable_id + 1),
            ..context
        },
        PaintRecordingContext {
            surface_dag: false,
            ..context
        },
        PaintRecordingContext {
            surface_dag_paint_state: Some(Default::default()),
            ..context
        },
        PaintRecordingContext {
            authoritative_self_clip: None,
            ..context
        },
    ] {
        assert!(!altered.authorizes_subtree_self_clip_for(stable_id, scissor));
    }
    let mut wrong_scissor = scissor;
    wrong_scissor[2] += 1;
    assert!(!context.authorizes_subtree_self_clip_for(stable_id, wrong_scissor));
    let clip = trees.clips.get_mut(&state.clip.unwrap()).unwrap();
    clip.geometry =
        crate::view::compositor::property_tree::ClipGeometry::LogicalScissor(wrong_scissor);
    assert!(PaintSubtreeSelfClipWitness::from_live_owner(&arena, owner, &trees, false).is_none());
    trees.clips.remove(&state.clip.unwrap());
    assert!(PaintSubtreeSelfClipWitness::from_live_owner(&arena, owner, &trees, false).is_none());
}

#[test]
fn generic_subtree_self_clip_rejects_malformed_child_topology() {
    for mutation in 0..3 {
        let (mut arena, roots, owner, child) = scene(false);
        let (trees, _) = sync_identity(&arena, &roots);
        match mutation {
            0 => arena.set_parent(child, Some(roots[0])),
            1 => arena.set_children(owner, vec![child, child]),
            2 => arena.set_arena_children_without_mirror_for_test(owner, vec![]),
            _ => unreachable!(),
        }
        assert!(
            PaintSubtreeSelfClipWitness::from_live_owner(&arena, owner, &trees, false).is_none()
        );
    }
}

#[test]
fn generic_subtree_self_clip_requires_descendants_to_preserve_the_exact_scope() {
    use crate::view::compositor::property_tree::{ClipGeometry, ClipNode};
    let (arena, roots, owner, _) = scene(false);
    let (mut trees, _) = sync_identity(&arena, &roots);
    let self_clip = trees.paint_state_for(owner).unwrap().clip.unwrap();
    let contents = ClipNodeId {
        owner,
        role: ClipNodeRole::ContentsClip,
    };
    trees.clips.insert(
        contents,
        ClipNode {
            owner,
            parent: Some(self_clip),
            geometry: ClipGeometry::LogicalScissor([0, 0, 8, 8]),
            behavior: ClipBehavior::Intersect,
            generation: 1,
        },
    );
    trees.states.get_mut(&owner).unwrap().descendants.clip = Some(contents);
    assert!(PaintSubtreeSelfClipWitness::from_live_owner(&arena, owner, &trees, false).is_some());
    trees.clips.get_mut(&contents).unwrap().behavior = ClipBehavior::Replace;
    assert!(PaintSubtreeSelfClipWitness::from_live_owner(&arena, owner, &trees, false).is_none());
    trees.clips.get_mut(&contents).unwrap().behavior = ClipBehavior::Intersect;
    trees.clips.get_mut(&contents).unwrap().parent = None;
    assert!(PaintSubtreeSelfClipWitness::from_live_owner(&arena, owner, &trees, false).is_none());
    trees.states.get_mut(&owner).unwrap().descendants.clip = None;
    assert!(PaintSubtreeSelfClipWitness::from_live_owner(&arena, owner, &trees, false).is_none());
}
