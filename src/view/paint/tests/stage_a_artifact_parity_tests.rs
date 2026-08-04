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
                assert!(ops.iter().all(|op| matches!(op, PaintOp::PreparedText(_))));
            }
            (PaintChunkRole::Caret, PaintPayloadIdentity::PreparedRects(rects)) => {
                assert_eq!(rects.len(), ops.len(), "{name}: caret payload coverage");
                assert!(ops.iter().all(|op| matches!(op, PaintOp::DrawRect(_))));
            }
            (PaintChunkRole::SelectionUnderlay, PaintPayloadIdentity::PreparedRects(rects)) => {
                assert_eq!(rects.len(), ops.len(), "{name}: selection payload coverage",);
                assert!(ops.iter().all(|op| matches!(op, PaintOp::DrawRect(_))));
            }
            _ => panic!("{name}: role and payload identity drifted: {chunk:?}"),
        }
        op_cursor = chunk.op_range.end;
    }
    assert_eq!(op_cursor, artifact.ops.len(), "{name}: exact-once terminal");
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
    let (plain_arena, plain_roots, plain_root) = prepared_plain_text_area_tree("artifact parity");
    let (plain_artifact, plain_properties) = record_artifact(&plain_arena, &plain_roots);
    let plain_state = plain_properties
        .node_state_for(plain_root)
        .unwrap()
        .descendants;
    assert_generic_artifact_contract(
        "plain",
        &plain_artifact,
        &[((
            plain_root,
            PaintPropertyScope::Contents,
            PaintNodePhase::BeforeChildren,
            1,
            PaintChunkRole::TextGlyphs,
            plain_state,
        ))],
    );

    let (interactive_arena, interactive_roots, interactive_root) =
        prepared_plain_text_area_tree("interactive artifact parity");
    set_text_area_interaction(&interactive_arena, interactive_root, true, false);
    let (interactive_artifact, interactive_properties) =
        record_artifact(&interactive_arena, &interactive_roots);
    let interactive_state = interactive_properties
        .node_state_for(interactive_root)
        .unwrap()
        .descendants;
    assert_generic_artifact_contract(
        "interactive",
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
        assert_generic_artifact_contract(name, &artifact, &expected);
    }
}
