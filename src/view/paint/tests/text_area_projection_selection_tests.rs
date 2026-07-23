use super::*;

#[test]
fn text_area_projection_selection_is_path_scoped_ordered_and_matches_legacy() {
    let selected_fixture = || {
        let (arena, roots, root, projection, projected_text) =
            prepared_projection_text_area_tree();
        {
            let mut node = arena.get_mut(root).unwrap();
            let text_area = node
                .element
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .unwrap();
            text_area.selection_anchor_char = Some(8);
            text_area.selection_focus_char = Some(15);
        }
        (arena, roots, root, projection, projected_text)
    };
    assert_whole_frame_structural_parity(
        || {
            let (arena, roots, ..) = selected_fixture();
            (arena, roots)
        },
        PaintParityConfig::default(),
    );

    let (arena, roots, root, projection, projected_text) = selected_fixture();
    let root_context = arena
        .get(root)
        .unwrap()
        .element
        .shadow_paint_recording_context(PaintRecordingContext::default());
    let projection_context = arena
        .get(root)
        .unwrap()
        .element
        .shadow_paint_recording_context_for_child(projection, &arena, root_context);
    let witness = projection_context
        .text_area_selection
        .expect("selected projection edge must own a witness");
    assert_eq!((witness.local_start, witness.local_end), (1, 8));
    assert_eq!(witness.target_owner, projected_text);
    assert_eq!(
        witness.target_stable_id,
        arena.get(projected_text).unwrap().element.stable_id()
    );
    for sibling in arena
        .children_of(root)
        .into_iter()
        .filter(|child| *child != projection)
    {
        let sibling_context = arena
            .get(root)
            .unwrap()
            .element
            .shadow_paint_recording_context_for_child(sibling, &arena, root_context);
        assert_eq!(
            sibling_context.text_area_selection, None,
            "selection authority must not leak to a TextArea sibling"
        );
    }
    let wrapper_context = arena
        .get(projection)
        .unwrap()
        .element
        .shadow_paint_recording_context(projection_context);
    let text_context = arena
        .get(projection)
        .unwrap()
        .element
        .shadow_paint_recording_context_for_child(projected_text, &arena, wrapper_context);
    assert_eq!(text_context.text_area_selection, Some(witness));

    let (properties, generations) = sync_identity(&arena, &roots);
    let (artifact, eligibility) =
        crate::view::base_component::with_text_area_selection_render_context(
            Some(
                crate::view::base_component::TextAreaSelectionRenderContext {
                    start: 0,
                    end: 9,
                    fill: [1.0, 0.0, 0.0, 1.0],
                },
            ),
            || whole_frame_artifact(&arena, &roots, &properties, &generations),
        );
    assert!(eligibility.eligible);
    assert_eq!(
        artifact
            .chunks
            .iter()
            .map(|chunk| (chunk.owner, chunk.id.slot, chunk.id.role))
            .collect::<Vec<_>>(),
        vec![
            (root, 1, PaintChunkRole::TextGlyphs),
            (projected_text, 0, PaintChunkRole::SelectionUnderlay),
            (projected_text, 1, PaintChunkRole::TextGlyphs),
        ]
    );
    let selection_chunk = artifact
        .chunks
        .iter()
        .find(|chunk| chunk.id.role == PaintChunkRole::SelectionUnderlay)
        .unwrap();
    assert!(
        artifact.ops[selection_chunk.op_range.clone()]
            .iter()
            .all(|op| {
                matches!(op, PaintOp::DrawRect(op) if op.params.fill_color == witness.fill)
            })
    );

    let selected_glyph_id = artifact
        .chunks
        .iter()
        .find(|chunk| {
            chunk.owner == projected_text && chunk.id.role == PaintChunkRole::TextGlyphs
        })
        .unwrap()
        .id;
    {
        let mut node = arena.get_mut(root).unwrap();
        let text_area = node
            .element
            .as_any_mut()
            .downcast_mut::<TextArea>()
            .unwrap();
        text_area.selection_anchor_char = None;
        text_area.selection_focus_char = None;
    }
    let (properties, generations) = sync_identity(&arena, &roots);
    let (artifact, eligibility) =
        whole_frame_artifact(&arena, &roots, &properties, &generations);
    assert!(eligibility.eligible);
    assert_eq!(
        artifact
            .chunks
            .iter()
            .find(|chunk| {
                chunk.owner == projected_text && chunk.id.role == PaintChunkRole::TextGlyphs
            })
            .unwrap()
            .id,
        selected_glyph_id,
        "projection Text glyph identity must not move when selection toggles"
    );
}

#[test]
fn text_area_atomic_projection_disjoint_root_selection_is_ordered_and_matches_legacy() {
    let fixture = || {
        let (arena, roots, root, projection, projected_text) =
            prepared_projection_text_area_tree();
        {
            let mut node = arena.get_mut(root).unwrap();
            let text_area = node
                .element
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .unwrap();
            text_area.selection_anchor_char = Some(0);
            text_area.selection_focus_char = Some(6);
        }
        (arena, roots, root, projection, projected_text)
    };
    assert_whole_frame_structural_parity(
        || {
            let (arena, roots, ..) = fixture();
            (arena, roots)
        },
        PaintParityConfig::default(),
    );

    let (arena, roots, root, projection, projected_text) = fixture();
    let (properties, generations) = sync_identity(&arena, &roots);
    let (artifact, eligibility) =
        whole_frame_artifact(&arena, &roots, &properties, &generations);
    assert!(eligibility.eligible, "{eligibility:?}");
    assert_eq!(
        artifact
            .chunks
            .iter()
            .map(|chunk| (chunk.owner, chunk.id.slot, chunk.id.role))
            .collect::<Vec<_>>(),
        vec![
            (root, 0, PaintChunkRole::SelectionUnderlay),
            (root, 1, PaintChunkRole::TextGlyphs),
            (projected_text, 1, PaintChunkRole::TextGlyphs),
        ],
        "root-owned selection must precede both root and projection glyphs",
    );
    let root_context = arena
        .get(root)
        .unwrap()
        .element
        .shadow_paint_recording_context(PaintRecordingContext::default());
    let projection_context = arena
        .get(root)
        .unwrap()
        .element
        .shadow_paint_recording_context_for_child(projection, &arena, root_context);
    assert_eq!(
        projection_context.text_area_selection, None,
        "disjoint root selection must not mint projection-owned authority",
    );
}

#[test]
fn text_area_selection_crossing_projection_is_split_between_root_and_child() {
    let fixture = || {
        let (arena, roots, root, projection, projected_text) =
            prepared_projection_text_area_tree();
        {
            let mut node = arena.get_mut(root).unwrap();
            let text_area = node
                .element
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .unwrap();
            text_area.selection_anchor_char = Some(0);
            text_area.selection_focus_char = Some(10);
        }
        (arena, roots, root, projection, projected_text)
    };
    assert_whole_frame_structural_parity(
        || {
            let (arena, roots, ..) = fixture();
            (arena, roots)
        },
        PaintParityConfig::default(),
    );

    let (arena, roots, root, projection, projected_text) = fixture();
    let (properties, generations) = sync_identity(&arena, &roots);
    let (artifact, eligibility) =
        whole_frame_artifact(&arena, &roots, &properties, &generations);
    assert!(eligibility.eligible, "{eligibility:?}");
    assert!(artifact.chunks.iter().any(|chunk| {
        chunk.owner == root && chunk.id.role == PaintChunkRole::SelectionUnderlay
    }));
    assert!(artifact.chunks.iter().any(|chunk| {
        chunk.owner == projected_text && chunk.id.role == PaintChunkRole::SelectionUnderlay
    }));

    let root_context = arena
        .get(root)
        .unwrap()
        .element
        .shadow_paint_recording_context(PaintRecordingContext::default());
    let projection_context = arena
        .get(root)
        .unwrap()
        .element
        .shadow_paint_recording_context_for_child(projection, &arena, root_context);
    let witness = projection_context
        .text_area_selection
        .expect("crossing selection must mint projection-local authority");
    assert_eq!(witness.local_start, 0);
    assert_eq!(witness.local_end, 3);
}

#[test]
fn text_area_projection_selection_utf8_local_range_and_metadata_full_identity_are_exact() {
    let utf8_fixture = || {
        let (arena, roots, root, projection, projected_text) =
            prepared_projection_text_area_tree_with("前🙂投影中文後", 2..6, "投影中文");
        {
            let mut node = arena.get_mut(root).unwrap();
            let text_area = node
                .element
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .unwrap();
            text_area.selection_anchor_char = Some(3);
            text_area.selection_focus_char = Some(5);
        }
        (arena, roots, root, projection, projected_text)
    };
    assert_whole_frame_structural_parity(
        || {
            let (arena, roots, ..) = utf8_fixture();
            (arena, roots)
        },
        PaintParityConfig::default(),
    );
    let (arena, _roots, root, projection, _) = utf8_fixture();
    let root_context = arena
        .get(root)
        .unwrap()
        .element
        .shadow_paint_recording_context(PaintRecordingContext::default());
    let witness = arena
        .get(root)
        .unwrap()
        .element
        .shadow_paint_recording_context_for_child(projection, &arena, root_context)
        .text_area_selection
        .unwrap();
    assert_eq!((witness.local_start, witness.local_end), (1, 3));

    for mutate_fill in [false, true] {
        let (arena, roots, root, ..) = utf8_fixture();
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
        {
            let mut node = arena.get_mut(root).unwrap();
            let text_area = node
                .element
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .unwrap();
            if mutate_fill {
                text_area.selection_background_color = Color::rgba(240, 32, 80, 128);
            } else {
                text_area.selection_anchor_char = Some(2);
                text_area.selection_focus_char = Some(4);
            }
        }
        let full = record_coverage_manifest(
            &arena,
            &roots,
            false,
            true,
            CoverageRecordingMode::FullArtifact,
            &properties,
            &generations,
        );
        assert!(
            !super::super::frame_recorder::canonical_manifest_matches(&metadata, &full),
            "metadata/full must detect {} drift",
            if mutate_fill { "fill" } else { "local range" }
        );
    }
}

#[test]
fn text_area_projection_selection_ambiguous_owner_and_witness_tamper_fail_closed() {
    let (mut arena, roots, root, projection, _) = prepared_projection_text_area_tree();
    {
        let mut node = arena.get_mut(root).unwrap();
        let text_area = node
            .element
            .as_any_mut()
            .downcast_mut::<TextArea>()
            .unwrap();
        text_area.selection_anchor_char = Some(8);
        text_area.selection_focus_char = Some(15);
    }
    commit_child(
        &mut arena,
        projection,
        Box::new(Text::from_content_with_id(0x7e99, "projected")),
    );
    let mut stack = vec![root];
    while let Some(key) = stack.pop() {
        stack.extend(arena.children_of(key));
        arena
            .get_mut(key)
            .unwrap()
            .element
            .clear_local_dirty_flags(DirtyFlags::ALL);
    }
    arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
    arena.refresh_subtree_dirty_cache(root);
    assert_text_area_fallback_before_full(&arena, &roots);

    let (arena, _roots, root, projection, projected_text) =
        prepared_projection_text_area_tree();
    {
        let mut node = arena.get_mut(root).unwrap();
        let text_area = node
            .element
            .as_any_mut()
            .downcast_mut::<TextArea>()
            .unwrap();
        text_area.selection_anchor_char = Some(8);
        text_area.selection_focus_char = Some(15);
    }
    let root_context = arena
        .get(root)
        .unwrap()
        .element
        .shadow_paint_recording_context(PaintRecordingContext::default());
    let projection_context = arena
        .get(root)
        .unwrap()
        .element
        .shadow_paint_recording_context_for_child(projection, &arena, root_context);
    let wrapper_context = arena
        .get(projection)
        .unwrap()
        .element
        .shadow_paint_recording_context(projection_context);
    let text_context = arena
        .get(projection)
        .unwrap()
        .element
        .shadow_paint_recording_context_for_child(projected_text, &arena, wrapper_context);
    for tamper_stable_id in [false, true] {
        let mut tampered = text_context;
        let witness = tampered.text_area_selection.as_mut().unwrap();
        if tamper_stable_id {
            witness.target_stable_id = witness.target_stable_id.wrapping_add(1);
        } else {
            witness.local_end = usize::MAX;
        }
        assert!(
            arena
                .get(projected_text)
                .unwrap()
                .element
                .record_shadow_paint_metadata_plan(
                    projected_text,
                    Default::default(),
                    Default::default(),
                    PaintContentRevision {
                        self_paint_revision: 1,
                        composite_revision: 1,
                        topology_revision: 1,
                    },
                    &arena,
                    tampered,
                )
                .is_none(),
            "tampered projection selection witness must fail closed"
        );
    }
}

#[test]
fn text_area_projection_selection_visibility_gate_prevents_artifact_only_underlay() {
    for case in ["should_render", "opacity_zero"] {
        let fixture = || {
            let (arena, roots, root, _projection, projected_text) =
                prepared_projection_text_area_tree();
            {
                let mut node = arena.get_mut(root).unwrap();
                let text_area = node
                    .element
                    .as_any_mut()
                    .downcast_mut::<TextArea>()
                    .unwrap();
                text_area.selection_anchor_char = Some(8);
                text_area.selection_focus_char = Some(15);
            }
            {
                let mut node = arena.get_mut(projected_text).unwrap();
                let text = node.element.as_any_mut().downcast_mut::<Text>().unwrap();
                match case {
                    "should_render" => text.set_should_render_for_test(false),
                    "opacity_zero" => text.set_opacity(0.0),
                    _ => unreachable!(),
                }
            }
            (arena, roots, root, projected_text)
        };
        assert_whole_frame_structural_parity(
            || {
                let (arena, roots, ..) = fixture();
                (arena, roots)
            },
            PaintParityConfig::default(),
        );
        let (arena, roots, _root, projected_text) = fixture();
        let (properties, generations) = sync_identity(&arena, &roots);
        let (artifact, eligibility) =
            whole_frame_artifact(&arena, &roots, &properties, &generations);
        assert!(eligibility.eligible, "{case}");
        assert!(
            artifact.chunks.iter().all(|chunk| {
                chunk.owner != projected_text
                    && chunk.id.role != PaintChunkRole::SelectionUnderlay
            }),
            "{case} must not emit a projected Text glyph or selection underlay"
        );
    }

    let (arena, roots, standalone_text) = prepared_text_tree(false);
    arena
        .get_mut(standalone_text)
        .unwrap()
        .element
        .as_any_mut()
        .downcast_mut::<Text>()
        .unwrap()
        .set_should_render_for_test(false);
    assert_eq!(
        arena
            .get(standalone_text)
            .unwrap()
            .element
            .shadow_paint_recording_capability(
                &arena,
                false,
                PaintRecordingContext::default(),
            ),
        ShadowPaintRecordingCapability::Transparent,
        "standalone invisible Text must close as transparent coverage"
    );
    let (properties, generations) = sync_identity(&arena, &roots);
    let (artifact, eligibility) =
        whole_frame_artifact(&arena, &roots, &properties, &generations);
    assert!(eligibility.eligible);
    assert!(
        artifact.chunks.is_empty(),
        "standalone invisible Text must not emit a typed zero-op chunk"
    );

    let (arena, roots, root, projection, projected_text) = prepared_projection_text_area_tree();
    {
        let mut node = arena.get_mut(root).unwrap();
        let text_area = node
            .element
            .as_any_mut()
            .downcast_mut::<TextArea>()
            .unwrap();
        text_area.selection_anchor_char = Some(8);
        text_area.selection_focus_char = Some(15);
    }
    let root_context = arena
        .get(root)
        .unwrap()
        .element
        .shadow_paint_recording_context(PaintRecordingContext::default());
    let projection_context = arena
        .get(root)
        .unwrap()
        .element
        .shadow_paint_recording_context_for_child(projection, &arena, root_context);
    let wrapper_context = arena
        .get(projection)
        .unwrap()
        .element
        .shadow_paint_recording_context(projection_context);
    let text_context = arena
        .get(projection)
        .unwrap()
        .element
        .shadow_paint_recording_context_for_child(projected_text, &arena, wrapper_context);
    arena
        .get_mut(projected_text)
        .unwrap()
        .element
        .as_any_mut()
        .downcast_mut::<Text>()
        .unwrap()
        .set_text("");
    assert_eq!(
        arena
            .get(projected_text)
            .unwrap()
            .element
            .shadow_paint_recording_capability(&arena, false, text_context),
        ShadowPaintRecordingCapability::Transparent,
        "empty content must close the shared Text paint gate before selection"
    );
    assert_text_area_fallback_before_full(&arena, &roots);
}

#[test]
fn text_area_projection_deferred_and_invalid_scroll_boundaries_remain_fail_closed() {
    let (arena, _roots, root, projection, _) = prepared_projection_text_area_tree();
    {
        let mut node = arena.get_mut(root).unwrap();
        let text_area = node
            .element
            .as_any_mut()
            .downcast_mut::<TextArea>()
            .unwrap();
        text_area.selection_anchor_char = Some(8);
        text_area.selection_focus_char = Some(15);
    }
    let node = arena.get(projection).unwrap();
    assert_eq!(
        node.element.shadow_paint_recording_capability(
            &arena,
            true,
            PaintRecordingContext {
                inside_text_area: true,
                ..Default::default()
            },
        ),
        ShadowPaintRecordingCapability::Legacy(ShadowPaintBlocker::Deferred)
    );
    drop(node);

    let (arena, roots, root, ..) = prepared_projection_text_area_tree();
    {
        let mut node = arena.get_mut(root).unwrap();
        let text_area = node
            .element
            .as_any_mut()
            .downcast_mut::<TextArea>()
            .unwrap();
        text_area.selection_anchor_char = Some(8);
        text_area.selection_focus_char = Some(15);
        text_area.scroll_x = 1.0;
    }
    assert_text_area_fallback_before_full(&arena, &roots);
}
