use super::*;

#[test]
fn plain_text_area_records_one_contents_glyph_chunk_and_transparent_runs() {
    let (arena, roots, root) = prepared_plain_text_area_tree(
        "plain TextArea wraps across a deliberately narrow viewport",
    );
    let (properties, generations) = sync_identity(&arena, &roots);
    take_full_artifact_record_count();
    let (artifact, eligibility) =
        whole_frame_artifact(&arena, &roots, &properties, &generations);
    assert!(eligibility.eligible);
    assert_eq!(artifact.chunks.len(), 1);
    assert_eq!(artifact.chunks[0].owner, root);
    assert_eq!(artifact.chunks[0].id.scope, PaintPropertyScope::Contents);
    assert_eq!(artifact.chunks[0].id.phase, PaintNodePhase::BeforeChildren);
    assert_eq!(artifact.chunks[0].id.slot, 1);
    assert_eq!(artifact.chunks[0].id.role, PaintChunkRole::TextGlyphs);
    assert_eq!(artifact.ops.len(), 1, "children must not duplicate glyphs");
    let PaintOp::PreparedText(op) = &artifact.ops[0] else {
        panic!("plain TextArea must freeze a prepared text op")
    };
    assert_eq!(op.params.scissor_rect, None);
    assert_eq!(op.params.stencil_clip_id, None);
    assert_eq!(take_full_artifact_record_count(), 1);

    let state = properties.node_state_for(root).unwrap();
    assert_eq!(artifact.chunks[0].properties, state.descendants);
    assert_ne!(state.paint.clip, state.descendants.clip);
    let graph = compiled_whole_frame_graph(&artifact);
    let passes = graph
        .test_graphics_passes::<crate::view::render_pass::text_pass::TextPreparedInputPass>();
    assert_eq!(passes.len(), 1);
    assert!(
        passes[0]
            .test_snapshot()
            .pass_context
            .scissor_rect
            .is_some(),
        "ContentsClip must reach the pass while the prepared op keeps scissor None"
    );
}

#[test]
fn plain_text_area_selection_orders_underlay_before_slot_one_glyphs() {
    let record = |anchor, focus| {
        let (arena, roots, root) = prepared_plain_text_area_selection_tree(
            "forward and reverse selection",
            108.0,
            anchor,
            focus,
        );
        let (properties, generations) = sync_identity(&arena, &roots);
        let (artifact, eligibility) =
            whole_frame_artifact(&arena, &roots, &properties, &generations);
        assert!(eligibility.eligible);
        assert_eq!(
            artifact
                .chunks
                .iter()
                .map(|chunk| (chunk.id.role, chunk.id.slot, chunk.id.scope))
                .collect::<Vec<_>>(),
            vec![
                (
                    PaintChunkRole::SelectionUnderlay,
                    0,
                    PaintPropertyScope::Contents,
                ),
                (PaintChunkRole::TextGlyphs, 1, PaintPropertyScope::Contents,),
            ]
        );
        assert!(artifact.chunks[0].op_range.len() >= 1);
        assert_eq!(artifact.chunks[1].op_range.len(), 1);
        assert_eq!(
            artifact
                .ops
                .iter()
                .filter(|op| matches!(op, PaintOp::PreparedText(_)))
                .count(),
            1,
            "Run children must not duplicate glyphs"
        );
        assert!(artifact.ops[..artifact.chunks[0].op_range.end]
            .iter()
            .all(|op| matches!(op, PaintOp::DrawRect(rect) if rect.mode == crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly)));
        assert_eq!(
            artifact.chunks[0].properties,
            properties.node_state_for(root).unwrap().descendants
        );
        (artifact, root)
    };

    let (forward, _) = record(2, 19);
    let (reverse, _) = record(19, 2);
    assert_eq!(
        forward.chunks[0].payload_identity, reverse.chunks[0].payload_identity,
        "selection direction must not perturb ordered geometry"
    );
}

#[test]
fn focused_plain_text_area_records_contents_caret_after_children_and_matches_legacy() {
    let focused_fixture = |content: &str, selection: Option<(usize, usize)>| {
        let (arena, roots, root) = prepared_plain_text_area_tree(content);
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
            if let Some((anchor, focus)) = selection {
                text_area.selection_anchor_char = Some(anchor);
                text_area.selection_focus_char = Some(focus);
            }
        }
        settle_plain_text_area(&arena, root);
        (arena, roots, root)
    };

    for selection in [None, Some((1, 8))] {
        let (arena, roots, root) = focused_fixture("focused caret artifact", selection);
        let (properties, generations) = sync_identity(&arena, &roots);
        let (artifact, eligibility) =
            whole_frame_artifact(&arena, &roots, &properties, &generations);
        assert!(eligibility.eligible);
        let expected_roles = if selection.is_some() {
            vec![
                PaintChunkRole::SelectionUnderlay,
                PaintChunkRole::TextGlyphs,
                PaintChunkRole::Caret,
            ]
        } else {
            vec![PaintChunkRole::TextGlyphs, PaintChunkRole::Caret]
        };
        assert_eq!(
            artifact
                .chunks
                .iter()
                .map(|chunk| chunk.id.role)
                .collect::<Vec<_>>(),
            expected_roles
        );
        let caret = artifact.chunks.last().unwrap();
        assert_eq!(caret.owner, root);
        assert_eq!(caret.id.scope, PaintPropertyScope::Contents);
        assert_eq!(caret.id.phase, PaintNodePhase::AfterChildren);
        assert_eq!(caret.id.slot, 1);
        assert_eq!(caret.op_range.len(), 1);
        let PaintOp::DrawRect(caret_op) = &artifact.ops[caret.op_range.clone()][0] else {
            panic!("caret must freeze one draw rect")
        };
        assert_eq!(
            caret_op.mode,
            crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly
        );
        assert_eq!(caret_op.params.size[0].to_bits(), 1.0_f32.to_bits());
        assert!(caret_op.params.size[1] >= 1.0);
        assert_eq!(caret_op.params.opacity.to_bits(), 1.0_f32.to_bits());
        assert_eq!(
            caret.properties,
            properties.node_state_for(root).unwrap().descendants
        );

        if selection.is_some() {
            let mut graph = compiled_whole_frame_graph(&artifact);
            let snapshot = graph.test_compile_snapshot().unwrap();
            let payloads = snapshot.pass_payloads();
            assert!(
                matches!(payloads, [
                    FramePassTestPayload::Clear(_),
                    FramePassTestPayload::DrawRect(selection),
                    FramePassTestPayload::PreparedText(glyphs),
                    FramePassTestPayload::DrawRect(caret),
                ] if selection.mode == crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly
                    && caret.mode == crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly
                    && selection.effective_scissor_rect == caret.effective_scissor_rect
                    && glyphs.pass_context.scissor_rect == caret.effective_scissor_rect
                    && glyphs.pass_context.stencil_clip_id == caret.pass_context.stencil_clip_id),
                "selection, glyphs, children boundary, and caret must compile in phased order with one Contents clip/stencil authority: {payloads:?}"
            );
        }
    }

    assert_whole_frame_structural_parity(
        || {
            let (arena, roots, _) = focused_fixture("focused caret parity", Some((1, 7)));
            (arena, roots)
        },
        PaintParityConfig::default(),
    );
}

#[test]
fn empty_focused_plain_text_area_is_caret_only_and_contents_clip_can_cull_it() {
    let make = |empty_viewport: bool| {
        let (arena, roots, root) = prepared_plain_text_area_tree("");
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
            if empty_viewport {
                text_area.viewport_size.height = 0.0;
            }
        }
        settle_plain_text_area(&arena, root);
        (arena, roots, root)
    };

    let (arena, roots, root) = make(false);
    let (properties, generations) = sync_identity(&arena, &roots);
    let (artifact, eligibility) =
        whole_frame_artifact(&arena, &roots, &properties, &generations);
    assert!(eligibility.eligible);
    assert_eq!(artifact.chunks.len(), 1);
    assert_eq!(artifact.chunks[0].id.role, PaintChunkRole::Caret);
    assert_eq!(artifact.chunks[0].id.phase, PaintNodePhase::AfterChildren);
    assert_eq!(artifact.ops.len(), 1);
    assert!(arena.children_of(root).is_empty());
    assert_eq!(
        compiled_whole_frame_graph(&artifact)
            .test_rect_pass_snapshots()
            .len(),
        1
    );

    let (arena, roots, _) = make(true);
    let (properties, generations) = sync_identity(&arena, &roots);
    let (artifact, eligibility) =
        whole_frame_artifact(&arena, &roots, &properties, &generations);
    assert!(eligibility.eligible);
    assert_eq!(artifact.chunks.len(), 1);
    assert!(
        artifact
            .clip_nodes
            .iter()
            .any(|clip| clip.logical_scissor[2] == 0 || clip.logical_scissor[3] == 0)
    );
    assert!(
        compiled_whole_frame_graph(&artifact)
            .test_rect_pass_snapshots()
            .is_empty()
    );
}

#[test]
fn retained_caret_phase_flip_changes_only_self_paint_generation() {
    let (mut arena, roots, root) = prepared_plain_text_area_tree("generation caret");
    let t0 = crate::time::Instant::now();
    {
        let mut node = arena.get_mut(root).unwrap();
        let text_area = node
            .element
            .as_any_mut()
            .downcast_mut::<TextArea>()
            .unwrap();
        text_area.is_focused = true;
        text_area.caret_visible = true;
        text_area.caret_blink_epoch = Some(t0);
    }
    settle_plain_text_area(&arena, root);

    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &roots);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &roots, &properties);
    let initial = generations.snapshot(root).unwrap();

    assert!(!crate::view::base_component::tick_animation_frames(
        &mut arena,
        &roots,
        t0 + crate::time::Duration::from_millis(529),
    ));
    assert!(
        arena
            .get(root)
            .unwrap()
            .element
            .local_dirty_flags()
            .is_empty()
    );
    properties.sync(&arena, &roots);
    generations.sync(&arena, &roots, &properties);
    assert_eq!(generations.snapshot(root).unwrap(), initial);

    assert!(crate::view::base_component::tick_animation_frames(
        &mut arena,
        &roots,
        t0 + crate::time::Duration::from_millis(530),
    ));
    let dirty = arena.get(root).unwrap().element.local_dirty_flags();
    assert_eq!(dirty, DirtyFlags::PAINT);
    assert_eq!(arena.arena_local_dirty(root), DirtyFlags::PAINT);
    properties.sync(&arena, &roots);
    generations.sync(&arena, &roots, &properties);
    let flipped = generations.snapshot(root).unwrap();
    assert_ne!(flipped.self_paint_revision, initial.self_paint_revision);
    assert_eq!(flipped.composite_revision, initial.composite_revision);
    assert_eq!(flipped.topology_revision, initial.topology_revision);
}

#[test]
fn retained_caret_metadata_full_visibility_drift_is_not_canonical() {
    let (arena, roots, root) = prepared_plain_text_area_tree("caret drift");
    {
        let mut node = arena.get_mut(root).unwrap();
        let text_area = node
            .element
            .as_any_mut()
            .downcast_mut::<TextArea>()
            .unwrap();
        text_area.is_focused = true;
        text_area.caret_visible = true;
    }
    settle_plain_text_area(&arena, root);
    let (properties, generations) = sync_identity(&arena, &roots);
    let metadata = record_coverage_manifest(
        &arena,
        &roots,
        false,
        true,
        CoverageRecordingMode::MetadataOnly,
        &properties,
        &generations,
    );
    arena
        .get_mut(root)
        .unwrap()
        .element
        .as_any_mut()
        .downcast_mut::<TextArea>()
        .unwrap()
        .caret_visible = false;
    let full = record_coverage_manifest(
        &arena,
        &roots,
        false,
        true,
        CoverageRecordingMode::FullArtifact,
        &properties,
        &generations,
    );
    assert!(!super::super::frame_recorder::canonical_manifest_matches(
        &metadata, &full
    ));
}

#[test]
fn plain_caret_artifact_honours_soft_wrap_affinity_and_matches_legacy() {
    fn fixture(upstream: bool) -> (NodeArena, Vec<NodeKey>, NodeKey) {
        let content = "甲乙丙丁戊己庚辛壬癸子丑寅卯辰巳午未申酉戌亥";
        let (arena, roots, root) =
            prepared_plain_text_area_tree_with(content, "", 80.0, [7.25, 11.75]);
        let boundary = {
            let node = arena.get(root).unwrap();
            let text_area = node.element.as_any().downcast_ref::<TextArea>().unwrap();
            (0..=content.chars().count())
                .find(|&char_index| {
                    let upstream =
                        crate::view::base_component::text_area::caret_map_probe_with_affinity(
                            text_area, &arena, char_index, true,
                        );
                    let downstream =
                        crate::view::base_component::text_area::caret_map_probe_with_affinity(
                            text_area, &arena, char_index, false,
                        );
                    upstream
                        .zip(downstream)
                        .is_some_and(|(up, down)| (up.2 - down.2).abs() > 0.5)
                })
                .expect("narrow fixture must expose a soft-wrap affinity boundary")
        };
        {
            let mut node = arena.get_mut(root).unwrap();
            let text_area = node
                .element
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .unwrap();
            text_area.cursor_char = boundary;
            crate::view::base_component::text_area::set_caret_affinity_probe(
                text_area, upstream,
            );
            text_area.is_focused = true;
            text_area.caret_visible = true;
            text_area.caret_blink_epoch = None;
        }
        settle_plain_text_area(&arena, root);
        (arena, roots, root)
    }

    let caret_position = |upstream| {
        let (arena, roots, _) = fixture(upstream);
        let (properties, generations) = sync_identity(&arena, &roots);
        let (artifact, eligibility) =
            whole_frame_artifact(&arena, &roots, &properties, &generations);
        assert!(eligibility.eligible);
        let caret = artifact
            .chunks
            .iter()
            .find(|chunk| chunk.id.role == PaintChunkRole::Caret)
            .unwrap();
        let PaintOp::DrawRect(op) = &artifact.ops[caret.op_range.clone()][0] else {
            panic!("caret artifact must contain one rect")
        };
        op.params.position
    };
    let upstream = caret_position(true);
    let downstream = caret_position(false);
    assert!(
        upstream[1] < downstream[1],
        "upstream caret must remain on the upper visual line: up={upstream:?}, down={downstream:?}"
    );

    for upstream in [true, false] {
        assert_whole_frame_structural_parity(
            || {
                let (arena, roots, _) = fixture(upstream);
                (arena, roots)
            },
            PaintParityConfig::default(),
        );
    }
}

#[test]
fn plain_text_area_selection_multiline_wrapped_and_clamped_cases_match_legacy() {
    for (content, width, anchor, focus) in [
        ("first line\nsecond line", 108.0, 2, 19),
        (
            "selection wraps across multiple visual lines in a narrow viewport",
            64.0,
            3,
            54,
        ),
        ("clamp this selection", 108.0, 0, usize::MAX),
        ("aé中🙂z", 108.0, 1, 4),
    ] {
        assert_whole_frame_structural_parity(
            || {
                let (arena, roots, _) =
                    prepared_plain_text_area_selection_tree(content, width, anchor, focus);
                (arena, roots)
            },
            PaintParityConfig::default(),
        );

        let (arena, roots, _) =
            prepared_plain_text_area_selection_tree(content, width, anchor, focus);
        let (properties, generations) = sync_identity(&arena, &roots);
        let (artifact, eligibility) =
            whole_frame_artifact(&arena, &roots, &properties, &generations);
        assert!(eligibility.eligible);
        assert_eq!(
            artifact.chunks[0].id.role,
            PaintChunkRole::SelectionUnderlay
        );
        if content.contains('\n') || width < 100.0 {
            assert!(artifact.chunks[0].op_range.len() >= 2);
        }
    }

    for (anchor, focus) in [(3, 3), (usize::MAX, usize::MAX)] {
        let (arena, roots, _) = prepared_plain_text_area_selection_tree(
            "collapsed selection",
            108.0,
            anchor,
            focus,
        );
        let (properties, generations) = sync_identity(&arena, &roots);
        let (artifact, eligibility) =
            whole_frame_artifact(&arena, &roots, &properties, &generations);
        assert!(eligibility.eligible);
        assert_eq!(artifact.chunks.len(), 1);
        assert_eq!(artifact.chunks[0].id.role, PaintChunkRole::TextGlyphs);
        assert_eq!(artifact.chunks[0].id.slot, 1);
    }
}

#[test]
fn plain_text_area_selection_contents_clip_handles_explicit_empty_viewport() {
    let (arena, roots, root) =
        prepared_plain_text_area_selection_tree("clipped selection", 108.0, 0, 7);
    arena
        .get_mut(root)
        .unwrap()
        .element
        .as_any_mut()
        .downcast_mut::<TextArea>()
        .unwrap()
        .viewport_size
        .height = 0.0;
    let (properties, generations) = sync_identity(&arena, &roots);
    let (artifact, eligibility) =
        whole_frame_artifact(&arena, &roots, &properties, &generations);
    assert!(eligibility.eligible);
    assert_eq!(artifact.chunks.len(), 2);
    assert!(
        artifact
            .clip_nodes
            .iter()
            .any(|clip| clip.logical_scissor[2] == 0 || clip.logical_scissor[3] == 0),
        "clips={:?}",
        artifact.clip_nodes
    );
    let graph = compiled_whole_frame_graph(&artifact);
    assert!(graph.test_rect_pass_snapshots().is_empty());
    assert!(
        graph
            .test_graphics_passes::<crate::view::render_pass::text_pass::TextPreparedInputPass>(
            )
            .is_empty()
    );
}

#[test]
fn plain_text_area_selection_metadata_and_full_hooks_reread_live_color_and_range() {
    let (arena, roots, root) =
        prepared_plain_text_area_selection_tree("metadata drift", 108.0, 1, 10);
    let (properties, generations) = sync_identity(&arena, &roots);
    let state = properties.node_state_for(root).unwrap();
    let generation = generations.local_generations_for(root).unwrap();
    let revision = PaintContentRevision {
        self_paint_revision: generation.self_paint_revision,
        composite_revision: generation.composite_revision,
        topology_revision: generation.topology_revision,
    };
    let metadata = arena
        .get(root)
        .unwrap()
        .element
        .record_shadow_paint_metadata_plan(
            root,
            state.paint,
            state.descendants,
            revision,
            &arena,
            PaintRecordingContext::default(),
        )
        .unwrap();
    assert_eq!(metadata.before_children.len(), 2);
    let old_selection_identity = metadata.before_children[0].payload_identity.clone();

    {
        let mut node = arena.get_mut(root).unwrap();
        node.element
            .as_any_mut()
            .downcast_mut::<TextArea>()
            .unwrap()
            .selection_background_color = Color::rgba(240, 32, 80, 128);
    }
    let full = arena
        .get(root)
        .unwrap()
        .element
        .record_shadow_paint_artifact_plan(
            root,
            state.paint,
            state.descendants,
            revision,
            &arena,
            PaintRecordingContext::default(),
        )
        .unwrap();
    assert_eq!(full.before_children.len(), 2);
    assert_ne!(
        old_selection_identity, full.before_children[0].chunks[0].payload_identity,
        "full recording must freeze the live selection color"
    );

    {
        let mut node = arena.get_mut(root).unwrap();
        let text_area = node
            .element
            .as_any_mut()
            .downcast_mut::<TextArea>()
            .unwrap();
        text_area.selection_anchor_char = Some(4);
        text_area.selection_focus_char = Some(4);
    }
    let collapsed = arena
        .get(root)
        .unwrap()
        .element
        .record_shadow_paint_metadata_plan(
            root,
            state.paint,
            state.descendants,
            revision,
            &arena,
            PaintRecordingContext::default(),
        )
        .unwrap();
    assert_eq!(collapsed.before_children.len(), 1);
    assert_eq!(
        collapsed.before_children[0].id.role,
        PaintChunkRole::TextGlyphs
    );
    assert_eq!(collapsed.before_children[0].id.slot, 1);
}
