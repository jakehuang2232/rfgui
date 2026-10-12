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
        snap: crate::view::paint::PaintOwnerSnap::INHERIT,
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
        geometry: crate::view::compositor::property_tree::ClipGeometry::Viewport([0, 0, 50, 60]),
        behavior: ClipBehavior::Replace,
        generation: 1,
    });

    let mut graph = compiled_whole_frame_graph(&artifact);
    let _ = strict_paint_snapshot(&mut graph, PaintParityConfig::default());
    let passes = graph.test_graphics_passes_mut::<crate::view::render_pass::TextureCompositePass>();
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
        .geometry = crate::view::compositor::property_tree::ClipGeometry::Viewport([20, 30, 0, 0]);
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
        snap: crate::view::paint::PaintOwnerSnap::INHERIT,
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
            geometry: crate::view::compositor::property_tree::ClipGeometry::Viewport([
                20, 30, 10, 10,
            ]),
            behavior: ClipBehavior::Intersect,
            generation: 1,
        },
        ClipNodeSnapshot {
            id: own,
            owner: leaf,
            parent: Some(contents),
            geometry: crate::view::compositor::property_tree::ClipGeometry::Viewport([
                5, 6, 80, 70,
            ]),
            behavior: ClipBehavior::Replace,
            generation: 1,
        },
    ]);
    artifact.chunks[0].properties.clip = Some(own);

    let mut graph = compiled_whole_frame_graph(&artifact);
    let _ = strict_paint_snapshot(&mut graph, PaintParityConfig::default());
    let passes = graph.test_graphics_passes_mut::<crate::view::render_pass::TextureCompositePass>();
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
fn empty_clip_emits_nothing_and_does_not_consume_opaque_order() {
    let mut artifact = compiler_clip_test_artifact();
    artifact.clip_nodes[0].geometry = artifact.clip_nodes[0].geometry.emptied_for_test();

    let graph = compiled_whole_frame_graph(&artifact);
    let snapshots = graph.test_rect_pass_snapshots();
    assert_eq!(snapshots.len(), 1, "only the unclipped sibling should emit");
    assert!(snapshots[0].opaque);
    assert_eq!(snapshots[0].opaque_depth_order, Some(0));
}
