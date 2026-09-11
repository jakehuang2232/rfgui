use super::*;

// C-1.3: enter the generic recorder directly. No legacy TextArea coverage
// authority, property projection, or resident-caret suppression is supplied.
fn record_generic(arena: &NodeArena, roots: &[NodeKey]) -> PaintArtifact {
    let (properties, generations) = sync_identity(arena, roots);
    for (&owner, states) in &properties.states {
        for state in [states.paint, states.descendants] {
            assert!(
                properties
                    .layout_position_snapshot_chain_for(state.layout_position)
                    .is_some(),
                "position {owner:?}: {state:?}"
            );
            assert!(
                properties
                    .visual_offset_snapshot_chain_for(state.visual_offset)
                    .is_some(),
                "visual {owner:?}: {state:?}"
            );
            assert!(
                properties.clip_snapshot_for(state.clip).is_some(),
                "clip {owner:?}: {state:?}"
            );
            assert!(
                properties.scroll_snapshot_chain_for(state.scroll).is_some(),
                "scroll {owner:?}: {state:?}"
            );
        }
    }
    let outcome = record_surface_dag_frame_artifact(
        arena,
        roots,
        &properties,
        &generations,
        RendererMode::ForcedForTests,
    )
    .expect("TextArea must record complete state through the generic entry");
    let FrameArtifactRecordOutcome::Artifact {
        artifact,
        eligibility,
    } = outcome
    else {
        panic!("generic TextArea recording must not fall back");
    };
    assert!(eligibility.eligible);
    artifact
}

#[test]
fn generic_text_area_records_interactive_commands_without_legacy_authority() {
    for projection in [false, true] {
        let (arena, roots, root, projected) = if projection {
            let (arena, roots, root, _, child) =
                prepared_projection_text_area_preedit_tree(8, "中🙂", Some((0, 7)));
            (arena, roots, root, Some(child))
        } else {
            let (arena, roots, root) = prepared_plain_text_area_preedit_tree(
                "complete commands",
                108.0,
                3,
                "中🙂",
                Some((0, 7)),
            );
            (arena, roots, root, None)
        };
        let artifact = record_generic(&arena, &roots);
        let mut expected = vec![(root, PaintChunkRole::TextGlyphs)];
        if let Some(child) = projected {
            expected.push((child, PaintChunkRole::TextGlyphs));
        }
        expected.extend([
            (root, PaintChunkRole::TextDecoration),
            (root, PaintChunkRole::Caret),
        ]);
        assert_eq!(
            artifact
                .chunks
                .iter()
                .map(|c| (c.owner, c.id.role))
                .collect::<Vec<_>>(),
            expected
        );
        assert!(
            artifact
                .ops
                .iter()
                .any(|op| matches!(op, PaintOp::PreparedText(_)))
        );
        assert!(
            !artifact.clip_nodes.is_empty(),
            "contents clip must remain frozen"
        );
    }
}

#[test]
fn generic_text_area_selection_retains_contents_state_and_caret() {
    let (arena, roots, root) =
        prepared_plain_text_area_selection_tree("selected content", 108.0, 1, 8);
    {
        let mut node = arena.get_mut(root).unwrap();
        let text_area = node
            .element
            .as_any_mut()
            .downcast_mut::<TextArea>()
            .unwrap();
        text_area.is_focused = true;
        text_area.caret_visible = true;
        text_area.caret_blink_epoch = None;
    }
    settle_plain_text_area(&arena, root);
    let artifact = record_generic(&arena, &roots);
    assert_eq!(
        artifact
            .chunks
            .iter()
            .map(|c| c.id.role)
            .collect::<Vec<_>>(),
        vec![
            PaintChunkRole::SelectionUnderlay,
            PaintChunkRole::TextGlyphs,
            PaintChunkRole::Caret,
        ]
    );
    let (properties, _) = sync_identity(&arena, &roots);
    let contents = properties.node_state_for(root).unwrap().descendants;
    assert!(artifact.chunks.iter().all(|c| c.properties == contents));
}

#[test]
fn generic_text_area_projection_spatial_chain_matches_layout_after_internal_scroll() {
    use crate::view::compositor::property_tree::SpatialProjectionGraph;
    let (mut arena, roots, root, projection, text) = prepared_projection_text_area_tree();
    for scroll in [0.0, 4.0, 0.0] {
        place_text_area_with_baked_scroll(&mut arena, root, 132.0, 8.0, [0.0, scroll]);
        let artifact = record_generic(&arena, &roots);
        let graph = SpatialProjectionGraph::try_new(
            &artifact.transform_nodes,
            &artifact.layout_position_nodes,
            &artifact.visual_offset_nodes,
            &artifact.scroll_nodes,
        )
        .unwrap();
        for owner in [root, projection, text] {
            let resolved = graph
                .derive_optional_owner_viewport_position(owner)
                .unwrap();
            let snapshot = arena.get(owner).unwrap().element.box_model_snapshot();
            assert!(
                (resolved.x - snapshot.x).abs() < 0.001 && (resolved.y - snapshot.y).abs() < 0.001,
                "owner {owner:?}, internal scroll {scroll}: {resolved:?} vs ({},{})",
                snapshot.x,
                snapshot.y
            );
        }
    }
}

#[test]
fn generic_text_area_projection_rejects_a_missing_frozen_spatial_parent() {
    let (arena, roots, _, projection, _) =
        prepared_projection_text_area_preedit_tree(8, "中🙂", Some((0, 7)));
    let artifact = record_generic(&arena, &roots);
    drop(arena);
    let context = ArtifactSurfaceRasterContext::new(
        1.0,
        wgpu::TextureFormat::Rgba8Unorm,
        [0.0, 0.0],
        None,
        8192,
        128 * 1024 * 1024,
    )
    .unwrap();
    prepare_artifact_surface_raster_plan(artifact.clone(), context)
        .expect("complete frozen parent chain");
    let mut damaged = artifact;
    let before = damaged.layout_position_nodes.len();
    damaged
        .layout_position_nodes
        .retain(|snapshot| snapshot.owner != projection);
    assert_eq!(damaged.layout_position_nodes.len() + 1, before);
    assert!(
        prepare_artifact_surface_raster_plan(damaged, context).is_err(),
        "missing parent must reject, not substitute an absolute or zero position"
    );
}
