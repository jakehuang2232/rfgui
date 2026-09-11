use super::*;

#[test]
fn planning_corpus_rejects_missing_duplicate_and_crossed_scroll_masks() {
    use crate::view::paint::{
        LayerizationPolicy, SurfaceDagError, derive_artifact_surface_transition_requests,
    };
    let artifact = record(&fixture(Scene::ClipScopes));
    let masks = artifact
        .chunks
        .iter()
        .enumerate()
        .filter(|(_, chunk)| chunk.id.slot == crate::view::paint::RETAINED_CHILD_MASK_SLOT)
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    assert_eq!(
        masks.len(),
        4,
        "outer begin, inner begin, inner end, outer end"
    );
    let inner = artifact.chunks[masks[1]].owner;
    let outer = artifact.chunks[masks[0]].owner;
    for (case, expected_owner) in [(0, inner), (1, inner), (2, outer)] {
        let mut damaged = artifact.clone();
        match case {
            0 => {
                damaged.chunks.remove(masks[2]);
            }
            1 => {
                damaged
                    .chunks
                    .insert(masks[1], damaged.chunks[masks[1]].clone());
            }
            2 => {
                damaged.chunks.swap(masks[2], masks[3]);
            }
            _ => unreachable!(),
        }
        let error = derive_artifact_surface_transition_requests(
            &damaged,
            LayerizationPolicy::ResolveMaterializedTargets,
        )
        .unwrap_err();
        // Missing inner close is discovered when the outer scope tries to close.
        let expected_owner = if case == 0 { outer } else { expected_owner };
        assert!(
            matches!(error, SurfaceDagError::InvalidScrollMaskScope { owner } if owner == expected_owner),
            "case {case}: {error:?}"
        );
    }
}

fn flatten(
    plan: &PreparedArtifactSurfaceRasterPlan,
    steps: &[PreparedArtifactSurfaceRasterStep],
    chunks: &mut Vec<(PaintChunkId, std::ops::Range<usize>)>,
    seen: &mut std::collections::BTreeSet<usize>,
    receiver: SurfaceDagExecutionTargetId,
) {
    for step in steps {
        match step {
            PreparedArtifactSurfaceRasterStep::ArtifactSpan(span) => {
                let mut cursor = span.op_range().start;
                for chunk in span.chunks() {
                    let end = cursor + chunk.localized_ops().len();
                    chunks.push((chunk.source().id, cursor..end));
                    cursor = end;
                }
                assert_eq!(
                    cursor,
                    span.op_range().end,
                    "localized ops cover the sealed span"
                );
            }
            PreparedArtifactSurfaceRasterStep::NestedSurface(id) => {
                assert!(
                    seen.insert(id.index()),
                    "each surface referenced exactly once; no cycles"
                );
                let node = &plan.nodes()[id.index()];
                assert_eq!(
                    node.receiver(),
                    receiver,
                    "nested edge names its actual receiver"
                );
                flatten(
                    plan,
                    node.steps(),
                    chunks,
                    seen,
                    SurfaceDagExecutionTargetId::Surface(*id),
                );
            }
        }
    }
}

#[test]
fn planning_corpus_preserves_command_order_and_exact_once_ownership() {
    for scene in Scene::ALL {
        eprintln!("planning contract {scene:?}");
        let fixture = fixture(scene);
        let artifact = record(&fixture);
        // Mask begin/end are structural commands, not fixture color owners;
        // the complete flatten comparison below still checks them exactly.
        assert_eq!(
            artifact
                .chunks
                .iter()
                .filter(|c| !c.op_range.is_empty()
                    && c.id.slot != crate::view::paint::RETAINED_CHILD_MASK_SLOT)
                .map(|c| c.owner)
                .collect::<Vec<_>>(),
            fixture.paint_owners,
            "paint order {scene:?}"
        );
        let expected = artifact
            .chunks
            .iter()
            .map(|c| (c.id, c.op_range.clone()))
            .collect::<Vec<_>>();
        let op_count = artifact.ops.len();
        if matches!(scene, Scene::DeepForest) {
            assert_eq!(fixture.roots.len(), 2);
            let mut depth = 0;
            let mut current = fixture.paint_owners[0];
            while let Some(parent) = fixture.arena.parent_of(current) {
                depth += 1;
                current = parent;
            }
            assert!(
                depth >= 5,
                "source has at least four nested boundary owners"
            );
        }
        drop(fixture.arena);
        for reverse in [false, true] {
            let mut artifact = artifact.clone();
            if reverse {
                // Parentless registry order defines scene-root paint order.
                // Permute descendants, preserving that explicit root contract.
                let mut descendants = artifact
                    .owner_nodes
                    .iter()
                    .filter(|s| s.parent.is_some())
                    .copied()
                    .rev()
                    .collect::<Vec<_>>()
                    .into_iter();
                for slot in &mut artifact.owner_nodes {
                    if slot.parent.is_some() {
                        *slot = descendants.next().unwrap();
                    }
                }
                artifact.owner_property_states.reverse();
                artifact.transform_nodes.reverse();
                artifact.effect_nodes.reverse();
                artifact.scroll_nodes.reverse();
                artifact.clip_nodes.reverse();
                artifact.layout_position_nodes.reverse();
                artifact.visual_offset_nodes.reverse();
            }
            for dpr in [1.0, 2.0] {
                let plan = prepare(artifact.clone(), dpr, [7.0, 5.0]);
                for node in plan.nodes() {
                    if let crate::view::paint::compiler::ArtifactSurfaceCompositeGeometryStamp::Transform { receiver_transform_bits, .. } = node.geometry() {
                        let snapshot = artifact.transform_nodes.iter().find(|s| s.owner == node.identity().boundary_root).unwrap();
                        assert_eq!(receiver_transform_bits, snapshot.owner_viewport_transform.to_cols_array().map(f32::to_bits), "an owner-only projection must not cancel the receiver's separately applied transform");
                    }
                }
                if matches!(
                    scene,
                    Scene::Scale | Scene::QuarterTurn | Scene::ObliqueTurn | Scene::NegativeOrigin
                ) {
                    assert_eq!(
                        plan.nodes().len(),
                        1,
                        "non-translation content retains its raster target"
                    );
                    let target = plan.nodes()[0].target();
                    assert_eq!(
                        [target.color.width(), target.color.height()],
                        [20 * dpr as u32, 16 * dpr as u32],
                        "scale/rotation changes composite geometry, not local raster resolution"
                    );
                }
                let mut actual = Vec::new();
                let mut seen = std::collections::BTreeSet::new();
                for root in plan.roots() {
                    flatten(
                        &plan,
                        root.steps(),
                        &mut actual,
                        &mut seen,
                        SurfaceDagExecutionTargetId::SceneRoot(root.scene_root()),
                    );
                }
                assert_eq!(
                    actual, expected,
                    "whole command order after recursive reconstruction: {scene:?}"
                );
                assert_eq!(
                    actual
                        .iter()
                        .flat_map(|(_, range)| range.clone())
                        .collect::<Vec<_>>(),
                    (0..op_count).collect::<Vec<_>>(),
                    "every source op index occurs once, in original order"
                );
                assert_eq!(seen.len(), plan.nodes().len());
                seal_prepared_artifact_surface_frame(plan).expect("strict execution seal");
            }
        }
    }
}
