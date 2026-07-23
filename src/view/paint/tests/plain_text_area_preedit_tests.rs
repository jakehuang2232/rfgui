use super::*;

#[test]
fn plain_text_area_preedit_variants_emit_exact_decoration_and_match_legacy() {
    for (content, width, cursor_char, preedit, preedit_cursor) in [
        ("abcdef", 108.0, 3, "中🙂", None),
        ("abcdef", 108.0, 2, "中🙂", Some((0, "中".len()))),
        ("abcdef", 108.0, 2, "中🙂", Some((0, 1))),
        ("", 108.0, 0, "入力", Some((0, usize::MAX))),
        ("first\nsecond", 108.0, 5, "長い入力", None),
        (
            "preedit wraps across several visual lines in a narrow viewport",
            64.0,
            9,
            "composition",
            Some((0, 6)),
        ),
    ] {
        assert_whole_frame_structural_parity(
            || {
                let (arena, roots, _) = prepared_plain_text_area_preedit_tree(
                    content,
                    width,
                    cursor_char,
                    preedit,
                    preedit_cursor,
                );
                (arena, roots)
            },
            PaintParityConfig::default(),
        );

        let (arena, roots, root) = prepared_plain_text_area_preedit_tree(
            content,
            width,
            cursor_char,
            preedit,
            preedit_cursor,
        );
        let (properties, generations) = sync_identity(&arena, &roots);
        let (artifact, eligibility) =
            whole_frame_artifact(&arena, &roots, &properties, &generations);
        assert!(eligibility.eligible);
        assert_eq!(
            artifact
                .chunks
                .iter()
                .map(|chunk| (chunk.id.phase, chunk.id.slot, chunk.id.role))
                .collect::<Vec<_>>(),
            vec![
                (
                    PaintNodePhase::BeforeChildren,
                    1,
                    PaintChunkRole::TextGlyphs
                ),
                (
                    PaintNodePhase::AfterChildren,
                    0,
                    PaintChunkRole::TextDecoration,
                ),
                (PaintNodePhase::AfterChildren, 1, PaintChunkRole::Caret),
            ]
        );
        let decoration = artifact
            .chunks
            .iter()
            .find(|chunk| chunk.id.role == PaintChunkRole::TextDecoration)
            .unwrap();
        assert!(!decoration.op_range.is_empty());
        for op in &artifact.ops[decoration.op_range.clone()] {
            let PaintOp::DrawRect(op) = op else {
                panic!("preedit decoration must contain only rect ops")
            };
            assert_eq!(
                op.mode,
                crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly
            );
            assert_eq!(op.params.size[1].to_bits(), 1.0_f32.to_bits());
            assert!(op.params.size[0] >= 1.0);
            assert_eq!(op.params.opacity.to_bits(), 1.0_f32.to_bits());
        }
        let transient_runs = arena
            .children_of(root)
            .into_iter()
            .filter(|&key| {
                arena
                    .get(key)
                    .unwrap()
                    .element
                    .as_any()
                    .downcast_ref::<TextAreaTextRun>()
                    .is_some_and(|run| run.is_preedit_run())
            })
            .count();
        assert_eq!(transient_runs, 1);
    }
}

#[test]
fn plain_text_area_preedit_selection_glyph_underline_caret_order_and_clip_are_exact() {
    let make = |empty_viewport: bool| {
        let (arena, roots, root) = prepared_plain_text_area_preedit_tree(
            "selection composition",
            108.0,
            9,
            "中",
            Some((0, 3)),
        );
        {
            let mut node = arena.get_mut(root).unwrap();
            let text_area = node
                .element
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .unwrap();
            text_area.selection_anchor_char = Some(0);
            text_area.selection_focus_char = Some(4);
            if empty_viewport {
                text_area.viewport_size.height = 0.0;
            }
        }
        (arena, roots, root)
    };

    let (arena, roots, root) = make(false);
    let (properties, generations) = sync_identity(&arena, &roots);
    let (artifact, eligibility) =
        whole_frame_artifact(&arena, &roots, &properties, &generations);
    assert!(eligibility.eligible);
    assert_eq!(
        artifact
            .chunks
            .iter()
            .map(|chunk| (chunk.id.phase, chunk.id.slot, chunk.id.role))
            .collect::<Vec<_>>(),
        vec![
            (
                PaintNodePhase::BeforeChildren,
                0,
                PaintChunkRole::SelectionUnderlay,
            ),
            (
                PaintNodePhase::BeforeChildren,
                1,
                PaintChunkRole::TextGlyphs
            ),
            (
                PaintNodePhase::AfterChildren,
                0,
                PaintChunkRole::TextDecoration,
            ),
            (PaintNodePhase::AfterChildren, 1, PaintChunkRole::Caret),
        ]
    );
    assert!(artifact.chunks.iter().all(|chunk| {
        chunk.id.owner == root
            && chunk.id.scope == PaintPropertyScope::Contents
            && chunk.properties == properties.node_state_for(root).unwrap().descendants
    }));
    let mut graph = compiled_whole_frame_graph(&artifact);
    let snapshot = graph.test_compile_snapshot().unwrap();
    let payloads = snapshot.pass_payloads();
    assert!(
        matches!(
            payloads,
            [
                FramePassTestPayload::Clear(_),
                FramePassTestPayload::DrawRect(selection),
                FramePassTestPayload::PreparedText(glyphs),
                FramePassTestPayload::DrawRect(underline),
                FramePassTestPayload::DrawRect(caret),
            ] if selection.mode == crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly
                && underline.mode == crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly
                && caret.mode == crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly
                && selection.effective_scissor_rect == underline.effective_scissor_rect
                && underline.effective_scissor_rect == caret.effective_scissor_rect
                && glyphs.pass_context.scissor_rect == caret.effective_scissor_rect
                && glyphs.pass_context.stencil_clip_id == caret.pass_context.stencil_clip_id
        ),
        "payloads={payloads:?}"
    );

    let (arena, roots, _) = make(true);
    let (properties, generations) = sync_identity(&arena, &roots);
    let (artifact, eligibility) =
        whole_frame_artifact(&arena, &roots, &properties, &generations);
    assert!(eligibility.eligible);
    assert_eq!(artifact.chunks.len(), 4);
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
fn plain_text_area_bounded_baked_scroll_is_canonical_and_matches_legacy() {
    let fixture = || {
        let (mut arena, roots, root) = prepared_plain_text_area_preedit_tree(
            "selection composition stays aligned while the viewport scrolls",
            108.0,
            9,
            "中",
            Some((0, 3)),
        );
        {
            let mut node = arena.get_mut(root).unwrap();
            let text_area = node
                .element
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .unwrap();
            text_area.selection_anchor_char = Some(0);
            text_area.selection_focus_char = Some(4);
        }
        place_text_area_with_baked_scroll(&mut arena, root, 108.0, 28.0, [0.0, 9.0]);
        (arena, roots)
    };

    assert_whole_frame_structural_parity(fixture, PaintParityConfig::default());

    let (arena, roots) = fixture();
    let root = roots[0];
    let (properties, generations) = sync_identity(&arena, &roots);
    assert!(
        properties.scrolls.is_empty(),
        "TextArea scroll stays baked into paint"
    );
    let root_node = arena.get(root).unwrap();
    let text_area = root_node
        .element
        .as_any()
        .downcast_ref::<TextArea>()
        .unwrap();
    assert!(!text_area.retained_paint_properties().is_scroll_container);
    assert_eq!(
        text_area.shadow_paint_recording_capability(
            &arena,
            false,
            PaintRecordingContext::default(),
        ),
        ShadowPaintRecordingCapability::Recordable
    );
    drop(root_node);

    let metadata = record_coverage_manifest(
        &arena,
        &roots,
        false,
        true,
        CoverageRecordingMode::MetadataOnly,
        &properties,
        &generations,
    );
    let full = record_coverage_manifest(
        &arena,
        &roots,
        false,
        true,
        CoverageRecordingMode::FullArtifact,
        &properties,
        &generations,
    );
    assert!(super::super::frame_recorder::canonical_manifest_matches(
        &metadata, &full
    ));

    let (artifact, eligibility) =
        whole_frame_artifact(&arena, &roots, &properties, &generations);
    assert!(eligibility.eligible);
    assert_eq!(
        artifact
            .chunks
            .iter()
            .map(|chunk| chunk.id.role)
            .collect::<Vec<_>>(),
        vec![
            PaintChunkRole::SelectionUnderlay,
            PaintChunkRole::TextGlyphs,
            PaintChunkRole::TextDecoration,
            PaintChunkRole::Caret,
        ]
    );
    assert!(artifact.chunks.iter().all(|chunk| {
        chunk.id.scope == PaintPropertyScope::Contents
            && chunk.properties == properties.node_state_for(root).unwrap().descendants
    }));
}

#[test]
fn text_area_baked_scroll_changes_self_paint_revision_only_after_exact_replacement() {
    let (mut arena, roots, root) = prepared_plain_text_area_tree(
        "paint identity must change when a bounded internal scroll changes",
    );
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &roots);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &roots, &properties);
    let before = generations.snapshot(root).unwrap();

    place_text_area_with_baked_scroll(&mut arena, root, 108.0, 28.0, [0.0, 7.0]);
    properties.sync(&arena, &roots);
    generations.sync(&arena, &roots, &properties);
    let scrolled = generations.snapshot(root).unwrap();
    assert_ne!(scrolled.self_paint_revision, before.self_paint_revision);
    assert!(properties.scrolls.is_empty());

    properties.sync(&arena, &roots);
    generations.sync(&arena, &roots, &properties);
    assert_eq!(
        generations.snapshot(root).unwrap().self_paint_revision,
        scrolled.self_paint_revision,
        "unchanged baked scroll must keep a deterministic paint identity"
    );
}

#[test]
fn plain_text_area_preedit_tampered_state_run_and_package_fail_before_full_hooks() {
    for case in [
        "ime",
        "run_text",
        "run_range",
        "run_cursor",
        "missing_run",
        "duplicate_run",
        "backing_range",
        "preedit_range",
        "caret_byte",
        "source",
    ] {
        let (arena, roots, root) =
            prepared_plain_text_area_preedit_tree("abcdef", 108.0, 3, "中🙂", Some((0, 3)));
        let (preedit_index, preedit_key) = arena
            .children_of(root)
            .into_iter()
            .enumerate()
            .find(|(_, key)| {
                arena
                    .get(*key)
                    .unwrap()
                    .element
                    .as_any()
                    .downcast_ref::<TextAreaTextRun>()
                    .is_some_and(|run| run.is_preedit_run())
            })
            .expect("fixture must contain one transient preedit Run");
        match case {
            "ime" => {
                arena
                    .get_mut(root)
                    .unwrap()
                    .element
                    .as_any_mut()
                    .downcast_mut::<TextArea>()
                    .unwrap()
                    .ime_preedit
                    .push('!');
            }
            "run_text" => {
                arena
                    .get_mut(preedit_key)
                    .unwrap()
                    .element
                    .as_any_mut()
                    .downcast_mut::<TextAreaTextRun>()
                    .unwrap()
                    .text
                    .push('!');
            }
            "run_range" => {
                arena
                    .get_mut(preedit_key)
                    .unwrap()
                    .element
                    .as_any_mut()
                    .downcast_mut::<TextAreaTextRun>()
                    .unwrap()
                    .char_range = 2..2;
            }
            "run_cursor" => {
                arena
                    .get_mut(preedit_key)
                    .unwrap()
                    .element
                    .as_any_mut()
                    .downcast_mut::<TextAreaTextRun>()
                    .unwrap()
                    .preedit_cursor = None;
            }
            "missing_run" => {
                arena
                    .get_mut(preedit_key)
                    .unwrap()
                    .element
                    .as_any_mut()
                    .downcast_mut::<TextAreaTextRun>()
                    .unwrap()
                    .is_preedit_run = false;
            }
            "duplicate_run" => {
                let other = arena
                    .children_of(root)
                    .into_iter()
                    .find(|key| *key != preedit_key)
                    .unwrap();
                arena
                    .get_mut(other)
                    .unwrap()
                    .element
                    .as_any_mut()
                    .downcast_mut::<TextAreaTextRun>()
                    .unwrap()
                    .is_preedit_run = true;
            }
            "backing_range" => {
                arena
                    .get(root)
                    .unwrap()
                    .element
                    .as_any()
                    .downcast_ref::<TextArea>()
                    .unwrap()
                    .tamper_cached_unified_segment_backing_range_for_test(preedit_index, 1..1);
            }
            "preedit_range" => {
                arena
                    .get(root)
                    .unwrap()
                    .element
                    .as_any()
                    .downcast_ref::<TextArea>()
                    .unwrap()
                    .tamper_cached_unified_segment_preedit_range_for_test(preedit_index, None);
            }
            "caret_byte" => {
                arena
                    .get(root)
                    .unwrap()
                    .element
                    .as_any()
                    .downcast_ref::<TextArea>()
                    .unwrap()
                    .tamper_cached_unified_segment_preedit_caret_for_test(
                        preedit_index,
                        Some(usize::MAX),
                    );
            }
            "source" => {
                arena
                    .get(root)
                    .unwrap()
                    .element
                    .as_any()
                    .downcast_ref::<TextArea>()
                    .unwrap()
                    .tamper_cached_unified_segment_source_for_test(preedit_index);
            }
            _ => unreachable!(),
        }
        let eligibility = assert_text_area_fallback_before_full(&arena, &roots);
        assert!(!eligibility.eligible, "{case}");
    }
}

#[test]
fn plain_text_area_preedit_commit_and_cancel_return_to_plain_slice() {
    let relayout = |arena: &mut NodeArena, root: NodeKey| {
        let measure = LayoutConstraints {
            max_width: 108.0,
            max_height: 240.0,
            viewport_width: 320.0,
            viewport_height: 240.0,
            percent_base_width: Some(320.0),
            percent_base_height: Some(240.0),
        };
        let place = LayoutPlacement {
            parent_x: 7.25,
            parent_y: 11.75,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 108.0,
            available_height: 240.0,
            viewport_width: 320.0,
            viewport_height: 240.0,
            percent_base_width: Some(320.0),
            percent_base_height: Some(240.0),
        };
        measure_and_place(arena, root, measure, place);
        arena
            .get_mut(root)
            .unwrap()
            .element
            .as_any_mut()
            .downcast_mut::<TextArea>()
            .unwrap()
            .pending_caret_scroll = false;
        settle_plain_text_area(arena, root);
    };

    for commit in [false, true] {
        let (mut arena, roots, root) =
            prepared_plain_text_area_preedit_tree("abcd", 108.0, 2, "中", Some((0, 3)));
        arena.with_element_taken(root, |element, _arena| {
            let text_area = element.as_any_mut().downcast_mut::<TextArea>().unwrap();
            if commit {
                assert!(text_area.commit_preedit_for_paint_test());
            } else {
                assert!(text_area.clear_preedit_for_paint_test());
            }
        });
        relayout(&mut arena, root);

        let root_node = arena.get(root).unwrap();
        let text_area = root_node
            .element
            .as_any()
            .downcast_ref::<TextArea>()
            .unwrap();
        assert!(text_area.ime_preedit.is_empty());
        assert_eq!(text_area.ime_preedit_cursor, None);
        assert_eq!(text_area.content, if commit { "ab中cd" } else { "abcd" });
        drop(root_node);
        assert!(arena.children_of(root).into_iter().all(|key| {
            arena
                .get(key)
                .unwrap()
                .element
                .as_any()
                .downcast_ref::<TextAreaTextRun>()
                .is_none_or(|run| !run.is_preedit_run())
        }));

        let (properties, generations) = sync_identity(&arena, &roots);
        let (artifact, eligibility) =
            whole_frame_artifact(&arena, &roots, &properties, &generations);
        assert!(eligibility.eligible);
        assert!(
            artifact
                .chunks
                .iter()
                .all(|chunk| chunk.id.role != PaintChunkRole::TextDecoration)
        );
    }
}

#[test]
fn plain_text_area_preedit_metadata_full_drift_and_boundaries_fail_closed() {
    let (arena, roots, root) =
        prepared_plain_text_area_preedit_tree("drift", 108.0, 2, "中", Some((0, 3)));
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
        .ime_preedit
        .push('!');
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

    let (arena, roots, root) =
        prepared_plain_text_area_preedit_tree("scroll", 108.0, 2, "中", None);
    arena
        .get_mut(root)
        .unwrap()
        .element
        .as_any_mut()
        .downcast_mut::<TextArea>()
        .unwrap()
        .scroll_y = 1.0;
    assert_text_area_fallback_before_full(&arena, &roots);

    let (arena, _roots, root) =
        prepared_plain_text_area_preedit_tree("deferred", 108.0, 2, "中", None);
    let node = arena.get(root).unwrap();
    let text_area = node.element.as_any().downcast_ref::<TextArea>().unwrap();
    assert_eq!(
        text_area.shadow_paint_recording_capability(
            &arena,
            true,
            PaintRecordingContext::default(),
        ),
        ShadowPaintRecordingCapability::Legacy(ShadowPaintBlocker::Deferred)
    );
    drop(node);
}
