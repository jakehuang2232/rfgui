use super::*;

type ExpectedChunk = (
    NodeKey,
    PaintPropertyScope,
    PaintNodePhase,
    u16,
    PaintChunkRole,
    crate::view::compositor::property_tree::PropertyTreeState,
);

fn assert_generic_artifact_contract(
    name: &str,
    arena: &NodeArena,
    roots: &[NodeKey],
    properties: &PropertyTrees,
    artifact: &PaintArtifact,
    expected: &[ExpectedChunk],
) {
    assert_eq!(artifact.chunks.len(), expected.len(), "{name}: chunk count");
    let mut op_cursor = 0;
    for (chunk, (owner, scope, phase, slot, role, properties)) in
        artifact.chunks.iter().zip(expected)
    {
        assert_eq!(
            (
                chunk.owner,
                chunk.id.owner,
                chunk.id.scope,
                chunk.id.phase,
                chunk.id.slot,
                chunk.id.role,
                chunk.properties,
            ),
            (*owner, *owner, *scope, *phase, *slot, *role, *properties,),
            "{name}: generic chunk identity",
        );
        assert_eq!(chunk.op_range.start, op_cursor, "{name}: exact-once start");
        assert!(
            chunk.op_range.end > chunk.op_range.start,
            "{name}: every semantic chunk owns at least one op",
        );
        let ops = &artifact.ops[chunk.op_range.clone()];
        match (chunk.id.role, &chunk.payload_identity) {
            (PaintChunkRole::TextGlyphs, PaintPayloadIdentity::PreparedTexts(texts)) => {
                assert_eq!(texts.len(), ops.len(), "{name}: glyph payload coverage");
                let prepared: Vec<&PreparedTextOp> = ops
                    .iter()
                    .map(|op| match op {
                        PaintOp::PreparedText(prepared) => prepared,
                        _ => panic!("{name}: glyph chunk contains a non-text op"),
                    })
                    .collect();
                assert!(
                    prepared.iter().all(|op| {
                        op.params.staging_input.scale_factor.to_bits() == 1.0_f32.to_bits()
                            && !op.params.staging_input.glyphs.is_empty()
                            && op.params.staging_input.glyphs.iter().all(|glyph| {
                                glyph.raster.font_size.to_bits() == 17.5_f32.to_bits()
                                    && glyph.paint.opacity.to_bits() == 1.0_f32.to_bits()
                                    && glyph.final_paint_pos.iter().all(|value| value.is_finite())
                            })
                    }),
                    "{name}: authored 17.5px glyph values, opacity, scale, and positions",
                );
                let [prepared] = prepared.as_slice() else {
                    panic!("{name}: each Stage A glyph chunk must contain one authored run")
                };
                let first = prepared
                    .params
                    .staging_input
                    .glyphs
                    .first()
                    .expect("Stage A glyph run has a first glyph");
                let (glyph_id, local_pos, final_paint_pos) = match (name, chunk.id.scope) {
                    ("plain", PaintPropertyScope::Contents) => {
                        (68, [0.0_f32, 15.0_f32], [8.0_f32, 27.0_f32])
                    }
                    ("interactive", PaintPropertyScope::Contents) => {
                        (76, [0.0_f32, 15.0_f32], [8.0_f32, 27.0_f32])
                    }
                    (
                        "atomic-projection"
                        | "focused-atomic-projection"
                        | "atomic-projection-selection",
                        PaintPropertyScope::Contents,
                    ) => (69, [0.0_f32, 23.0_f32], [8.0_f32, 35.0_f32]),
                    (
                        "atomic-projection"
                        | "focused-atomic-projection"
                        | "atomic-projection-selection",
                        PaintPropertyScope::SelfPaint,
                    ) => (83, [0.0_f32, 15.0_f32], [62.48242_f32, 27.25_f32]),
                    _ => panic!("{name}: glyph literal contract is not registered"),
                };
                assert_eq!(
                    (
                        first.raster.glyph_id,
                        first.paint.local_pos.map(f32::to_bits),
                        first.final_paint_pos.map(f32::to_bits),
                    ),
                    (
                        glyph_id,
                        local_pos.map(f32::to_bits),
                        final_paint_pos.map(f32::to_bits),
                    ),
                    "{name}: first authored glyph id and coordinates",
                );
            }
            (PaintChunkRole::Caret, PaintPayloadIdentity::PreparedRects(rects)) => {
                assert_eq!(rects.len(), ops.len(), "{name}: caret payload coverage");
                let [PaintOp::DrawRect(caret)] = ops else {
                    panic!("{name}: caret chunk must contain one rect op")
                };
                let (position, size) = match name {
                    "interactive" => ([60.525635_f32, 12.0_f32], [1.0_f32, 22.75_f32]),
                    "focused-atomic-projection" => ([62.48242_f32, 12.0_f32], [1.0_f32, 23.0_f32]),
                    _ => panic!("{name}: caret literal contract is not registered"),
                };
                assert_eq!(
                    (
                        caret.params.position.map(f32::to_bits),
                        caret.params.size.map(f32::to_bits),
                        caret.params.fill_color.map(f32::to_bits),
                        caret.params.opacity.to_bits(),
                        caret.mode,
                    ),
                    (
                        position.map(f32::to_bits),
                        size.map(f32::to_bits),
                        [
                            0.0056053917_f32,
                            0.0056053917_f32,
                            0.0056053917_f32,
                            1.0_f32,
                        ]
                        .map(f32::to_bits),
                        1.0_f32.to_bits(),
                        crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly,
                    ),
                    "{name}: caret op values must match the frozen fixture contract",
                );
            }
            (PaintChunkRole::SelectionUnderlay, PaintPayloadIdentity::PreparedRects(rects)) => {
                assert_eq!(rects.len(), ops.len(), "{name}: selection payload coverage",);
                let [PaintOp::DrawRect(selection)] = ops else {
                    panic!("{name}: selection literal contract expects one rect")
                };
                assert_eq!(
                    (
                        selection.params.position.map(f32::to_bits),
                        selection.params.size.map(f32::to_bits),
                        selection.params.fill_color.map(f32::to_bits),
                        selection.params.opacity.to_bits(),
                        selection.mode,
                    ),
                    (
                        [8.0_f32, 12.0_f32].map(f32::to_bits),
                        [49.62036_f32, 22.75_f32].map(f32::to_bits),
                        [
                            0.06301004_f32,
                            0.23455065_f32,
                            0.8713672_f32,
                            0.34901962_f32,
                        ]
                        .map(f32::to_bits),
                        1.0_f32.to_bits(),
                        crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly,
                    ),
                    "{name}: selection rect values must match the frozen fixture contract",
                );
            }
            _ => panic!("{name}: role and payload identity drifted: {chunk:?}"),
        }
        op_cursor = chunk.op_range.end;
    }
    assert_eq!(op_cursor, artifact.ops.len(), "{name}: exact-once terminal");
    assert_complete_artifact_store_profile(name, arena, roots, properties, artifact, expected);
}

fn extend_unique<T: Copy + PartialEq>(store: &mut Vec<T>, snapshots: impl IntoIterator<Item = T>) {
    for snapshot in snapshots {
        if !store.contains(&snapshot) {
            store.push(snapshot);
        }
    }
}

/// Store order is contractual: recording appends each chunk-referenced closure
/// first, then the paint/descendants closures for that chunk owner's topology.
/// The five fixtures currently add 0 clip and 0 effect nodes from the endpoint
/// pass, but that measured corpus boundary is not a global completeness claim:
/// a valid artifact may own endpoint-only snapshots no chunk references.
fn assert_complete_artifact_store_profile(
    name: &str,
    arena: &NodeArena,
    roots: &[NodeKey],
    properties: &PropertyTrees,
    artifact: &PaintArtifact,
    expected: &[ExpectedChunk],
) {
    let mut clips = Vec::new();
    let mut effects = Vec::new();
    let mut chunk_clips = Vec::new();
    let mut chunk_effects = Vec::new();
    let mut owners = Vec::new();
    let mut owner_property_states = Vec::new();

    for (owner, _, _, _, _, state) in expected {
        extend_unique(
            &mut clips,
            properties
                .clip_snapshot_for(state.clip)
                .expect("complete clip snapshot closure"),
        );
        extend_unique(
            &mut chunk_clips,
            properties
                .clip_snapshot_for(state.clip)
                .expect("complete chunk-only clip closure"),
        );
        extend_unique(
            &mut effects,
            properties
                .effect_snapshot_for(state.effect)
                .expect("complete effect snapshot closure"),
        );
        extend_unique(
            &mut chunk_effects,
            properties
                .effect_snapshot_for(state.effect)
                .expect("complete chunk-only effect closure"),
        );
        let mut cursor = Some(*owner);
        while let Some(owner) = cursor {
            let parent = (!roots.contains(&owner))
                .then(|| arena.parent_of(owner))
                .flatten();
            extend_unique(&mut owners, [PaintOwnerSnapshot { owner, parent }]);
            let state = properties
                .node_state_for(owner)
                .expect("Stage A owner endpoint state");
            let endpoint = PaintOwnerPropertyStateSnapshot {
                owner,
                paint: state.paint,
                descendants: state.descendants,
            };
            extend_unique(&mut owner_property_states, [endpoint]);
            for state in [endpoint.paint, endpoint.descendants] {
                extend_unique(
                    &mut clips,
                    properties
                        .clip_snapshot_for(state.clip)
                        .expect("complete owner endpoint clip closure"),
                );
                extend_unique(
                    &mut effects,
                    properties
                        .effect_snapshot_for(state.effect)
                        .expect("complete owner endpoint effect closure"),
                );
            }
            cursor = parent;
        }
    }

    assert_eq!(artifact.clip_nodes, clips, "{name}: complete clip store");
    assert_eq!(
        artifact.effect_nodes, effects,
        "{name}: complete effect store"
    );
    // This Stage A artifact-only corpus authors no transform, layout-position,
    // visual-offset, or scroll property nodes. The empty scroll store is an
    // explicit fixture boundary, not a claim of scroll-graph coverage.
    assert!(
        artifact.transform_nodes.is_empty(),
        "{name}: Stage A fixture transform store profile"
    );
    assert!(
        artifact.layout_position_nodes.is_empty(),
        "{name}: Stage A fixture layout-position store profile"
    );
    assert!(
        artifact.visual_offset_nodes.is_empty(),
        "{name}: Stage A fixture visual-offset store profile"
    );
    assert!(
        artifact.scroll_nodes.is_empty(),
        "{name}: Stage A fixture scroll store profile"
    );
    assert_eq!(artifact.owner_nodes, owners, "{name}: complete owner store");
    assert_eq!(
        clips.len(),
        chunk_clips.len(),
        "{name}: five-fixture endpoint clip expansion remains measured at 0"
    );
    assert_eq!(
        effects.len(),
        chunk_effects.len(),
        "{name}: five-fixture endpoint effect expansion remains measured at 0"
    );
    // Expected endpoints use the same admitted chunk-owner ancestor closure as
    // the topology gate. Exact equality is local to that artifact owner set,
    // not proof about owners that the artifact did not admit.
    assert_eq!(
        artifact.owner_property_states, owner_property_states,
        "{name}: complete owner endpoint store"
    );
}

fn record_artifact(arena: &NodeArena, roots: &[NodeKey]) -> (PaintArtifact, PropertyTrees) {
    let (properties, generations) = sync_identity(arena, roots);
    let (artifact, eligibility) = whole_frame_artifact(arena, roots, &properties, &generations);
    assert!(
        eligibility.eligible,
        "Stage A corpus must record completely"
    );
    (artifact, properties)
}

fn set_text_area_interaction(arena: &NodeArena, root: NodeKey, focused: bool, selection: bool) {
    {
        let mut node = arena.get_mut(root).unwrap();
        let text_area = node
            .element
            .as_any_mut()
            .downcast_mut::<TextArea>()
            .unwrap();
        text_area.is_focused = focused;
        text_area.caret_visible = focused;
        text_area.caret_blink_epoch = None;
        text_area.cursor_char = 7;
        if selection {
            text_area.selection_anchor_char = Some(0);
            text_area.selection_focus_char = Some(6);
        }
    }
    settle_plain_text_area(arena, root);
}

#[test]
fn stage_a_text_area_artifact_contract_preserves_generic_chunk_semantics() {
    let (plain_arena, plain_roots, plain_root) = prepared_plain_text_area_tree("artifact contract");
    let (plain_artifact, plain_properties) = record_artifact(&plain_arena, &plain_roots);
    let plain_state = plain_properties
        .node_state_for(plain_root)
        .unwrap()
        .descendants;
    assert_generic_artifact_contract(
        "plain",
        &plain_arena,
        &plain_roots,
        &plain_properties,
        &plain_artifact,
        &[(
            plain_root,
            PaintPropertyScope::Contents,
            PaintNodePhase::BeforeChildren,
            1,
            PaintChunkRole::TextGlyphs,
            plain_state,
        )],
    );

    let (interactive_arena, interactive_roots, interactive_root) =
        prepared_plain_text_area_tree("interactive artifact contract");
    set_text_area_interaction(&interactive_arena, interactive_root, true, false);
    let (interactive_artifact, interactive_properties) =
        record_artifact(&interactive_arena, &interactive_roots);
    let interactive_state = interactive_properties
        .node_state_for(interactive_root)
        .unwrap()
        .descendants;
    assert_generic_artifact_contract(
        "interactive",
        &interactive_arena,
        &interactive_roots,
        &interactive_properties,
        &interactive_artifact,
        &[
            (
                interactive_root,
                PaintPropertyScope::Contents,
                PaintNodePhase::BeforeChildren,
                1,
                PaintChunkRole::TextGlyphs,
                interactive_state,
            ),
            (
                interactive_root,
                PaintPropertyScope::Contents,
                PaintNodePhase::AfterChildren,
                1,
                PaintChunkRole::Caret,
                interactive_state,
            ),
        ],
    );

    for (name, focused, selection) in [
        ("atomic-projection", false, false),
        ("focused-atomic-projection", true, false),
        ("atomic-projection-selection", false, true),
    ] {
        let (arena, roots, root, _, projected_text) = prepared_projection_text_area_tree();
        set_text_area_interaction(&arena, root, focused, selection);
        let (artifact, properties) = record_artifact(&arena, &roots);
        let root_state = properties.node_state_for(root).unwrap().descendants;
        let projection_state = properties.node_state_for(projected_text).unwrap().paint;
        let mut expected = Vec::new();
        if selection {
            expected.push((
                root,
                PaintPropertyScope::Contents,
                PaintNodePhase::BeforeChildren,
                0,
                PaintChunkRole::SelectionUnderlay,
                root_state,
            ));
        }
        expected.extend([
            (
                root,
                PaintPropertyScope::Contents,
                PaintNodePhase::BeforeChildren,
                1,
                PaintChunkRole::TextGlyphs,
                root_state,
            ),
            (
                projected_text,
                PaintPropertyScope::SelfPaint,
                PaintNodePhase::BeforeChildren,
                1,
                PaintChunkRole::TextGlyphs,
                projection_state,
            ),
        ]);
        if focused {
            expected.push((
                root,
                PaintPropertyScope::Contents,
                PaintNodePhase::AfterChildren,
                1,
                PaintChunkRole::Caret,
                root_state,
            ));
        }
        assert_generic_artifact_contract(name, &arena, &roots, &properties, &artifact, &expected);
    }
}

#[test]
fn stage_a_owner_property_endpoints_preserve_distinct_paint_and_descendants() {
    let (arena, roots, root) = prepared_plain_text_area_tree("owner endpoint contract");
    let (artifact, properties) = record_artifact(&arena, &roots);
    let expected = properties
        .node_state_for(root)
        .expect("plain TextArea property endpoints");
    let [actual] = artifact.owner_property_states.as_slice() else {
        panic!("plain TextArea must record exactly one owner endpoint pair")
    };

    assert_eq!(actual.owner, root);
    assert_eq!(actual.paint, expected.paint);
    assert_eq!(actual.descendants, expected.descendants);
    assert_ne!(
        actual.paint, actual.descendants,
        "producer must not collapse the two endpoint roles"
    );
}
