use super::*;

#[test]
fn image_and_svg_compile_with_validated_inherited_contents_clip() {
    for (name, mut artifact) in [
        ("image", compiler_image_test_artifact(false)),
        ("svg", compiler_svg_test_artifact(false)),
    ] {
        add_inherited_contents_clip(&mut artifact, [2, 3, 20, 10]);
        let mut graph = compiled_whole_frame_graph(&artifact);
        let _ = strict_paint_snapshot(&mut graph, PaintParityConfig::default());
        let passes =
            graph.test_graphics_passes_mut::<crate::view::render_pass::TextureCompositePass>();
        assert_eq!(passes.len(), 1, "{name} inherited clip must compile");
        let snapshot = passes[0].test_snapshot();
        assert!(snapshot.explicit_scissor_rect.is_none());
        assert_eq!(snapshot.effective_scissor_rect, Some([2, 3, 20, 10]));
    }
}

#[test]
fn contents_clip_intersects_ancestor_replace_and_explicit_empty_culls() {
    let mut artifact = compiler_image_test_artifact(false);
    let contents = add_inherited_contents_clip(&mut artifact, [20, 30, 80, 80]);
    let outer_owner = unique_synthetic_owner(&artifact);
    artifact
        .owner_nodes
        .iter_mut()
        .find(|snapshot| snapshot.owner == contents.owner)
        .expect("contents owner")
        .parent = Some(outer_owner);
    artifact.owner_nodes.push(PaintOwnerSnapshot {
        owner: outer_owner,
        parent: None,
    });
    let outer = ClipNodeId {
        owner: outer_owner,
        role: ClipNodeRole::SelfClip,
    };
    artifact
        .clip_nodes
        .iter_mut()
        .find(|snapshot| snapshot.id == contents)
        .expect("contents clip")
        .parent = Some(outer);
    artifact.clip_nodes.push(ClipNodeSnapshot {
        id: outer,
        owner: outer_owner,
        parent: None,
        logical_scissor: [0, 0, 50, 60],
        behavior: ClipBehavior::Replace,
        generation: 1,
    });

    let mut graph = compiled_whole_frame_graph(&artifact);
    let _ = strict_paint_snapshot(&mut graph, PaintParityConfig::default());
    let passes =
        graph.test_graphics_passes_mut::<crate::view::render_pass::TextureCompositePass>();
    assert_eq!(passes.len(), 1);
    assert_eq!(
        passes[0].test_snapshot().effective_scissor_rect,
        Some([20, 30, 30, 30])
    );

    let mut empty = artifact;
    empty
        .clip_nodes
        .iter_mut()
        .find(|snapshot| snapshot.id == contents)
        .expect("contents clip")
        .logical_scissor = [20, 30, 0, 0];
    let mut graph = compiled_whole_frame_graph(&empty);
    assert!(
        graph
            .test_graphics_passes_mut::<crate::view::render_pass::TextureCompositePass>()
            .is_empty(),
        "explicit empty contents clip must suppress the clipped pass"
    );
}

#[test]
fn nested_self_replace_escapes_ancestor_contents_intersection() {
    let mut artifact = compiler_image_test_artifact(false);
    let leaf = artifact.chunks[0].owner;
    let outer_owner = unique_synthetic_owner(&artifact);
    artifact
        .owner_nodes
        .iter_mut()
        .find(|snapshot| snapshot.owner == leaf)
        .unwrap()
        .parent = Some(outer_owner);
    artifact.owner_nodes.push(PaintOwnerSnapshot {
        owner: outer_owner,
        parent: None,
    });
    let contents = ClipNodeId {
        owner: outer_owner,
        role: ClipNodeRole::ContentsClip,
    };
    let own = ClipNodeId {
        owner: leaf,
        role: ClipNodeRole::SelfClip,
    };
    artifact.clip_nodes.extend([
        ClipNodeSnapshot {
            id: contents,
            owner: outer_owner,
            parent: None,
            logical_scissor: [20, 30, 10, 10],
            behavior: ClipBehavior::Intersect,
            generation: 1,
        },
        ClipNodeSnapshot {
            id: own,
            owner: leaf,
            parent: Some(contents),
            logical_scissor: [5, 6, 80, 70],
            behavior: ClipBehavior::Replace,
            generation: 1,
        },
    ]);
    artifact.chunks[0].properties.clip = Some(own);

    let mut graph = compiled_whole_frame_graph(&artifact);
    let _ = strict_paint_snapshot(&mut graph, PaintParityConfig::default());
    let passes =
        graph.test_graphics_passes_mut::<crate::view::render_pass::TextureCompositePass>();
    assert_eq!(passes.len(), 1);
    assert_eq!(
        passes[0].test_snapshot().effective_scissor_rect,
        Some([5, 6, 80, 70])
    );
}

#[test]
fn compiler_rejects_invalid_contents_clip_role_behavior_and_ancestry() {
    let mut valid = compiler_image_test_artifact(false);
    let contents = add_inherited_contents_clip(&mut valid, [1, 2, 3, 4]);

    let mut wrong_behavior = valid.clone();
    wrong_behavior.clip_nodes[0].behavior = ClipBehavior::Replace;
    assert_compiler_rejects_before_emit(&wrong_behavior, "contents clip replace behavior");

    let mut wrong_role = valid.clone();
    wrong_role.clip_nodes[0].id.role = ClipNodeRole::SelfClip;
    wrong_role.chunks[0].properties.clip = Some(wrong_role.clip_nodes[0].id);
    assert_compiler_rejects_before_emit(&wrong_role, "intersect self-clip role");

    let mut wrong_owner = valid;
    let unrelated = unique_synthetic_owner(&wrong_owner);
    wrong_owner.clip_nodes[0].id.owner = unrelated;
    wrong_owner.clip_nodes[0].owner = unrelated;
    wrong_owner.chunks[0].properties.clip = Some(wrong_owner.clip_nodes[0].id);
    assert_ne!(unrelated, contents.owner);
    assert_compiler_rejects_before_emit(&wrong_owner, "clip owner outside chunk ancestry");
}

#[test]
fn compiler_rejects_every_invalid_clip_snapshot_before_emit() {
    let valid = compiler_clip_test_artifact();

    let mut missing_leaf = valid.clone();
    missing_leaf.clip_nodes.clear();
    assert_compiler_rejects_before_emit(&missing_leaf, "missing clip leaf");

    let mut dangling_parent = valid.clone();
    dangling_parent.clip_nodes[0].parent = Some(ClipNodeId {
        owner: NodeKey::null(),
        role: ClipNodeRole::SelfClip,
    });
    assert_compiler_rejects_before_emit(&dangling_parent, "dangling clip parent");

    let mut cycle = valid.clone();
    cycle.clip_nodes[0].parent = Some(cycle.clip_nodes[0].id);
    assert_compiler_rejects_before_emit(&cycle, "clip cycle");

    let mut invalid_owner = valid.clone();
    invalid_owner.clip_nodes[0].owner = NodeKey::null();
    assert_compiler_rejects_before_emit(&invalid_owner, "invalid clip owner");

    let mut invalid_role = valid.clone();
    invalid_role.clip_nodes[0].id.role = ClipNodeRole::ContentsClip;
    invalid_role.chunks[0].properties.clip = Some(invalid_role.clip_nodes[0].id);
    assert_compiler_rejects_before_emit(&invalid_role, "invalid clip role");

    let mut invalid_generation = valid.clone();
    invalid_generation.clip_nodes[0].generation = 0;
    assert_compiler_rejects_before_emit(&invalid_generation, "invalid clip generation");

    let mut excessive_depth = valid;
    let leaf_id = excessive_depth.clip_nodes[0].id;
    let mut key_arena = NodeArena::new();
    let mut ids = Vec::new();
    while ids.len() < usize::from(u8::MAX) {
        let key = key_arena.insert(Node::new(Box::new(Element::new_with_id(
            10_000 + ids.len() as u64,
            0.0,
            0.0,
            1.0,
            1.0,
        ))));
        let id = ClipNodeId {
            owner: key,
            role: ClipNodeRole::SelfClip,
        };
        if id != leaf_id {
            ids.push(id);
        }
    }
    excessive_depth.clip_nodes[0].parent = Some(ids[0]);
    for (index, id) in ids.iter().copied().enumerate() {
        excessive_depth.clip_nodes.push(ClipNodeSnapshot {
            id,
            owner: id.owner,
            parent: ids.get(index + 1).copied(),
            logical_scissor: [0, 0, 320, 240],
            behavior: ClipBehavior::Replace,
            generation: 1,
        });
    }
    assert_compiler_rejects_before_emit(&excessive_depth, "clip depth above 255");
}

#[test]
fn empty_clip_emits_nothing_and_does_not_consume_opaque_order() {
    let mut artifact = compiler_clip_test_artifact();
    artifact.clip_nodes[0].logical_scissor[2] = 0;

    let graph = compiled_whole_frame_graph(&artifact);
    let snapshots = graph.test_rect_pass_snapshots();
    assert_eq!(snapshots.len(), 1, "only the unclipped sibling should emit");
    assert!(snapshots[0].opaque);
    assert_eq!(snapshots[0].opaque_depth_order, Some(0));
}
