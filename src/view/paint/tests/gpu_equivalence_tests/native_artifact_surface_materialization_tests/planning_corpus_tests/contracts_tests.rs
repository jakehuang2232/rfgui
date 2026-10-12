use super::*;

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
                        let projection = crate::view::paint::ArtifactSpatialProjection::try_new(&artifact).unwrap();
                        assert_eq!(receiver_transform_bits, projection.owner_viewport_transform(snapshot.id).unwrap().to_cols_array().map(f32::to_bits), "an owner-only projection must not cancel the receiver's separately applied transform");
                    }
                }
                if matches!(
                    scene,
                    Scene::Scale
                        | Scene::QuarterTurn
                        | Scene::ObliqueTurn
                        | Scene::NegativeOrigin
                        | Scene::NegativeOriginContent
                        | Scene::NegativeOriginVerticalContent
                ) {
                    assert_eq!(
                        plan.nodes().len(),
                        1,
                        "non-translation content retains its raster target"
                    );
                    // The 2x scale rasters at its displayed resolution; a
                    // rotation keeps the local one.
                    let shown = if matches!(scene, Scene::QuarterTurn | Scene::ObliqueTurn) {
                        1
                    } else {
                        2
                    };
                    let target = plan.nodes()[0].target();
                    assert_eq!(
                        [target.color.width(), target.color.height()],
                        [20 * dpr as u32 * shown, 16 * dpr as u32 * shown],
                        "a transform rasters at the resolution it shows content at"
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

#[test]
fn decorated_inline_cross_feature_records_fragments_and_glyphs() {
    let f = fixture(Scene::DecoratedInlineClip);
    // Viewport-space geometry, as the compiler reads it.
    let artifact = crate::view::paint::compiler::with_placed_chunks(record(&f), 1.0);
    let decorations = artifact
        .ops
        .iter()
        .filter_map(|op| match op {
            PaintOp::PreparedInlineIfcDecoration(d) => Some(d),
            _ => None,
        })
        .collect::<Vec<_>>();
    // Inline padding expands the fragment above its line box (y=8, while
    // the scrollport starts at y=16), so its top border is clipped away.
    // Use the first fragment's left border/padding, both inside the clip.
    let first = decorations.first().unwrap();
    assert_eq!(first.fill.position, [16.0, 8.0]);
    assert_eq!(first.fill.border_widths, [6.0, 0.0, 6.0, 6.0]);
    assert!(first.fill.size[1] > 12.0);
    assert!(
        decorations
            .iter()
            .any(|d| d.fill.position[1] >= 52.0 && d.fill.size[0] > 12.0),
        "bottom clear probe must cover real inline content beyond the scrollport"
    );
    assert!(
        artifact
            .ops
            .iter()
            .any(|op| matches!(op, PaintOp::PreparedText(_)))
    );
    assert!(
        artifact
            .ops
            .iter()
            .filter(|op| matches!(op, PaintOp::PreparedInlineIfcDecoration(_)))
            .count()
            >= 2
    );
}
