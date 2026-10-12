use super::*;

#[test]
fn text_area_projection_baked_scroll_translates_root_and_absolute_child_once() {
    fn fixture(scroll_y: f32) -> (NodeArena, Vec<NodeKey>, NodeKey, NodeKey) {
        let (mut arena, roots, root, _, projected_text) = prepared_projection_text_area_tree();
        place_text_area_with_baked_scroll(&mut arena, root, 132.0, 8.0, [0.0, scroll_y]);
        (arena, roots, root, projected_text)
    }

    eligible_whole_frame_rects(
        || {
            let (arena, roots, ..) = fixture(4.0);
            (arena, roots)
        },
        PaintParityConfig::default(),
    );

    let first_glyph_position = |artifact: &PaintArtifact, owner: NodeKey| {
        let chunk = artifact
            .chunks
            .iter()
            .find(|chunk| chunk.owner == owner && chunk.id.role == PaintChunkRole::TextGlyphs)
            .expect("owner must retain one glyph chunk");
        let PaintOp::PreparedText(op) = &artifact.ops[chunk.op_range.start] else {
            panic!("glyph chunk must reference a prepared text op")
        };
        op.params.staging_input.glyphs[0].final_paint_pos
    };

    let (arena, roots, root, projected_text) = fixture(0.0);
    let (properties, generations) = sync_identity(&arena, &roots);
    let unscrolled = whole_frame_artifact(&arena, &roots, &properties, &generations);
    let root_before = first_glyph_position(&unscrolled, root);
    let projected_before = first_glyph_position(&unscrolled, projected_text);

    let (arena, roots, root, projected_text) = fixture(4.0);
    let (properties, generations) = sync_identity(&arena, &roots);
    let scrolled = whole_frame_artifact(&arena, &roots, &properties, &generations);
    let root_after = first_glyph_position(&scrolled, root);
    let projected_after = first_glyph_position(&scrolled, projected_text);
    assert_eq!(root_after[1], root_before[1] - 4.0);
    assert_eq!(projected_after[1], projected_before[1] - 4.0);
}

#[test]
fn text_area_projection_preedit_direct_text_is_path_scoped_and_ordered() {
    eligible_whole_frame_rects(
        || {
            let (arena, roots, ..) =
                prepared_projection_text_area_preedit_tree(8, "中🙂", Some((0, 7)));
            (arena, roots)
        },
        PaintParityConfig::default(),
    );

    let (arena, roots, root, projection, projected_text) =
        prepared_projection_text_area_preedit_tree(8, "中🙂", Some((0, 7)));
    let root_context = arena
        .get(root)
        .unwrap()
        .element
        .shadow_paint_recording_context(&PaintRecordingContext::default());
    let projection_context = arena
        .get(root)
        .unwrap()
        .element
        .shadow_paint_recording_context_for_child(projection, &arena, &root_context);
    let witness = projection_context
        .text_area_preedit
        .clone()
        .expect("target projection edge must carry preedit authority");
    assert_eq!((witness.local_start_char, witness.local_end_char), (1, 3));
    let text_context = arena
        .get(projection)
        .unwrap()
        .element
        .shadow_paint_recording_context_for_child(projected_text, &arena, &projection_context);
    assert_eq!(text_context.text_area_preedit, Some(witness));
    for sibling in arena
        .children_of(root)
        .into_iter()
        .filter(|key| *key != projection)
    {
        assert_eq!(
            arena
                .get(root)
                .unwrap()
                .element
                .shadow_paint_recording_context_for_child(sibling, &arena, &root_context,)
                .text_area_preedit,
            None
        );
    }

    let (properties, generations) = sync_identity(&arena, &roots);
    let artifact = whole_frame_artifact(&arena, &roots, &properties, &generations);
    assert_eq!(
        artifact
            .chunks
            .iter()
            .map(|chunk| (chunk.owner, chunk.id.phase, chunk.id.slot, chunk.id.role))
            .collect::<Vec<_>>(),
        vec![
            (
                root,
                PaintNodePhase::BeforeChildren,
                1,
                PaintChunkRole::TextGlyphs
            ),
            (
                projected_text,
                PaintNodePhase::BeforeChildren,
                1,
                PaintChunkRole::TextGlyphs,
            ),
            (
                root,
                PaintNodePhase::AfterChildren,
                0,
                PaintChunkRole::TextDecoration
            ),
            (
                root,
                PaintNodePhase::AfterChildren,
                1,
                PaintChunkRole::Caret
            ),
        ]
    );
    let decoration = artifact
        .chunks
        .iter()
        .find(|chunk| chunk.id.role == PaintChunkRole::TextDecoration)
        .unwrap();
    assert!(artifact.ops[decoration.op_range.clone()].iter().all(
        |op| matches!(op, PaintOp::DrawRect(op) if op.params.size[1].to_bits() == 1.0_f32.to_bits())
    ));
}

#[test]
fn text_area_projection_preedit_utf8_cursor_clamps_to_prior_boundary() {
    let caret_position = |cursor| {
        let (arena, roots, ..) = prepared_projection_text_area_preedit_tree(8, "中🙂", cursor);
        let (properties, generations) = sync_identity(&arena, &roots);
        let artifact = whole_frame_artifact(&arena, &roots, &properties, &generations);
        let chunk = artifact
            .chunks
            .iter()
            .find(|chunk| chunk.id.role == PaintChunkRole::Caret)
            .unwrap();
        let PaintOp::DrawRect(op) = &artifact.ops[chunk.op_range.start] else {
            panic!("caret must be a rect")
        };
        op.params.position
    };
    let start = caret_position(Some((0, 0)));
    assert_eq!(caret_position(Some((0, 1))), start);
    let after_cjk = caret_position(Some((0, 3)));
    assert!(after_cjk[0] > start[0]);
    assert_eq!(caret_position(Some((0, 4))), after_cjk);
    let end = caret_position(None);
    assert!(end[0] > after_cjk[0] || end[1] > after_cjk[1]);
}

#[test]
fn mixed_projection_with_plain_transient_preedit_remains_eligible() {
    let (arena, roots, root, projection, projected_text) =
        prepared_projection_text_area_preedit_tree(2, "中", Some((0, 3)));
    let root_context = arena
        .get(root)
        .unwrap()
        .element
        .shadow_paint_recording_context(&PaintRecordingContext::default());
    assert_eq!(
        arena
            .get(root)
            .unwrap()
            .element
            .shadow_paint_recording_context_for_child(projection, &arena, &root_context,)
            .text_area_preedit,
        None
    );
    assert!(arena.children_of(root).into_iter().any(|key| {
        arena.get(key).is_some_and(|node| {
            node.element
                .as_any()
                .downcast_ref::<TextAreaTextRun>()
                .is_some_and(|run| run.is_preedit_run())
        })
    }));
    let (properties, generations) = sync_identity(&arena, &roots);
    let artifact = whole_frame_artifact(&arena, &roots, &properties, &generations);
    assert!(artifact.chunks.iter().any(|chunk| {
        chunk.owner == projected_text && chunk.id.role == PaintChunkRole::TextGlyphs
    }));
    assert!(
        artifact.chunks.iter().any(|chunk| {
            chunk.owner == root && chunk.id.role == PaintChunkRole::TextDecoration
        })
    );
}
