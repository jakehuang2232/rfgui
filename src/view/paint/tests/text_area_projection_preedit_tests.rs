use super::*;

#[test]
fn text_area_projection_baked_scroll_translates_root_and_absolute_child_once() {
    fn fixture(scroll_y: f32) -> (NodeArena, Vec<NodeKey>, NodeKey, NodeKey) {
        let (mut arena, roots, root, _, projected_text) = prepared_projection_text_area_tree();
        place_text_area_with_baked_scroll(&mut arena, root, 132.0, 8.0, [0.0, scroll_y]);
        (arena, roots, root, projected_text)
    }

    assert_whole_frame_structural_parity(
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
    let (unscrolled, eligibility) =
        whole_frame_artifact(&arena, &roots, &properties, &generations);
    assert!(eligibility.eligible);
    let root_before = first_glyph_position(&unscrolled, root);
    let projected_before = first_glyph_position(&unscrolled, projected_text);

    let (arena, roots, root, projected_text) = fixture(4.0);
    let (properties, generations) = sync_identity(&arena, &roots);
    let (scrolled, eligibility) =
        whole_frame_artifact(&arena, &roots, &properties, &generations);
    assert!(eligibility.eligible);
    let root_after = first_glyph_position(&scrolled, root);
    let projected_after = first_glyph_position(&scrolled, projected_text);
    assert_eq!(root_after[1], root_before[1] - 4.0);
    assert_eq!(projected_after[1], projected_before[1] - 4.0);
}

#[test]
fn text_area_projection_preedit_direct_text_is_path_scoped_ordered_and_matches_legacy() {
    assert_whole_frame_structural_parity(
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
        .shadow_paint_recording_context(PaintRecordingContext::default());
    let projection_context = arena
        .get(root)
        .unwrap()
        .element
        .shadow_paint_recording_context_for_child(projection, &arena, root_context);
    let witness = projection_context
        .text_area_preedit
        .expect("target projection edge must carry preedit authority");
    assert_eq!((witness.local_start_char, witness.local_end_char), (1, 3));
    let text_context = arena
        .get(projection)
        .unwrap()
        .element
        .shadow_paint_recording_context_for_child(projected_text, &arena, projection_context);
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
                .shadow_paint_recording_context_for_child(sibling, &arena, root_context,)
                .text_area_preedit,
            None
        );
    }

    let (properties, generations) = sync_identity(&arena, &roots);
    let (artifact, eligibility) =
        whole_frame_artifact(&arena, &roots, &properties, &generations);
    assert!(eligibility.eligible);
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
        let (artifact, eligibility) =
            whole_frame_artifact(&arena, &roots, &properties, &generations);
        assert!(eligibility.eligible);
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
        .shadow_paint_recording_context(PaintRecordingContext::default());
    assert_eq!(
        arena
            .get(root)
            .unwrap()
            .element
            .shadow_paint_recording_context_for_child(projection, &arena, root_context,)
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
    let (artifact, eligibility) =
        whole_frame_artifact(&arena, &roots, &properties, &generations);
    assert!(eligibility.eligible);
    assert!(artifact.chunks.iter().any(|chunk| {
        chunk.owner == projected_text && chunk.id.role == PaintChunkRole::TextGlyphs
    }));
    assert!(artifact.chunks.iter().any(|chunk| {
        chunk.owner == root && chunk.id.role == PaintChunkRole::TextDecoration
    }));
}

#[test]
fn text_area_projection_atomic_witness_tamper_fails_before_full_hooks() {
    for case in [
        "live_width",
        "flow_offset",
        "range",
        "source",
        "backing",
        "atomic_missing",
        "atomic_duplicate",
        "measurement_constraint",
        "measurement_size",
        "insertion",
        "orphan",
        "dirty",
        "scroll",
    ] {
        let (mut arena, roots, root, projection, _) = prepared_projection_text_area_tree();
        let projection_index = arena
            .children_of(root)
            .iter()
            .position(|key| *key == projection)
            .unwrap();
        match case {
            "live_width" => {
                let width = arena
                    .get(projection)
                    .unwrap()
                    .element
                    .box_model_snapshot()
                    .width;
                arena.with_element_taken(projection, |element, _arena| {
                    element.set_layout_width(width + 1.0);
                });
            }
            "flow_offset" => {
                arena.with_element_taken(projection, |element, _arena| {
                    element.set_layout_offset(999.0, 0.0);
                });
            }
            "range" => {
                arena
                    .get_mut(projection)
                    .unwrap()
                    .element
                    .as_any_mut()
                    .downcast_mut::<TextAreaProjectionSegment>()
                    .unwrap()
                    .set_char_range(0..1);
            }
            "source" => {
                arena
                    .get(root)
                    .unwrap()
                    .element
                    .as_any()
                    .downcast_ref::<TextArea>()
                    .unwrap()
                    .tamper_cached_unified_segment_source_for_test(projection_index);
            }
            "backing" => {
                arena
                    .get(root)
                    .unwrap()
                    .element
                    .as_any()
                    .downcast_ref::<TextArea>()
                    .unwrap()
                    .tamper_cached_unified_segment_backing_range_for_test(
                        projection_index,
                        0..1,
                    );
            }
            "atomic_missing" | "atomic_duplicate" => {
                arena
                    .get(root)
                    .unwrap()
                    .element
                    .as_any()
                    .downcast_ref::<TextArea>()
                    .unwrap()
                    .tamper_cached_unified_atomic_sources_for_test(case == "atomic_duplicate");
            }
            "measurement_constraint" | "measurement_size" => {
                arena
                    .get(root)
                    .unwrap()
                    .element
                    .as_any()
                    .downcast_ref::<TextArea>()
                    .unwrap()
                    .tamper_cached_unified_atomic_measurement_for_test(
                        projection_index,
                        case == "measurement_constraint",
                    );
            }
            "insertion" => {
                arena
                    .get(root)
                    .unwrap()
                    .element
                    .as_any()
                    .downcast_ref::<TextArea>()
                    .unwrap()
                    .tamper_cached_unified_atomic_insertion_for_test(projection_index);
            }
            "orphan" => arena.set_parent(projection, None),
            "dirty" => arena.mark_dirty(projection, DirtyFlags::LAYOUT),
            "scroll" => {
                arena
                    .get_mut(root)
                    .unwrap()
                    .element
                    .as_any_mut()
                    .downcast_mut::<TextArea>()
                    .unwrap()
                    .scroll_x = 1.0;
            }
            _ => unreachable!(),
        }
        let eligibility = assert_text_area_fallback_before_full(&arena, &roots);
        assert!(!eligibility.eligible, "{case}");
    }
}

#[test]
fn text_area_projection_preedit_topology_and_witness_tamper_fail_closed() {
    let (mut arena, roots, _root, projection, _) =
        prepared_projection_text_area_preedit_tree(8, "中", Some((0, 3)));
    commit_child(
        &mut arena,
        projection,
        Box::new(Text::from_content_with_id(0x7e98, "duplicate")),
    );
    assert_text_area_fallback_before_full(&arena, &roots);

    let (mut arena, roots, _root, projection, projected_text) =
        prepared_projection_text_area_preedit_tree(8, "中", Some((0, 3)));
    let wrapper = commit_child(
        &mut arena,
        projection,
        Box::new(Element::new_with_id(0x7e97, 0.0, 0.0, 1.0, 1.0)),
    );
    arena.set_parent(projected_text, Some(wrapper));
    arena.set_children(wrapper, vec![projected_text]);
    arena.with_element_taken(wrapper, |element, _| {
        element.sync_children_mirror(&[projected_text]);
    });
    arena.set_children(projection, vec![wrapper]);
    arena.with_element_taken(projection, |element, _| {
        element.sync_children_mirror(&[wrapper]);
    });
    assert_text_area_fallback_before_full(&arena, &roots);

    let (arena, _roots, root, projection, projected_text) =
        prepared_projection_text_area_preedit_tree(8, "中", Some((0, 3)));
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
    let mut text_context = arena
        .get(projection)
        .unwrap()
        .element
        .shadow_paint_recording_context_for_child(projected_text, &arena, projection_context);
    text_context
        .text_area_preedit
        .as_mut()
        .unwrap()
        .target_caret_byte = usize::MAX;
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
                text_context,
            )
            .is_none()
    );
}

#[test]
fn text_area_projection_preedit_metadata_full_drift_is_detected() {
    for drift_cursor in [false, true] {
        let (arena, roots, root, _projection, projected_text) =
            prepared_projection_text_area_preedit_tree(8, "中🙂", Some((0, 7)));
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
        if drift_cursor {
            arena
                .get_mut(root)
                .unwrap()
                .element
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .unwrap()
                .ime_preedit_cursor = Some((0, 3));
        } else {
            arena
                .get_mut(projected_text)
                .unwrap()
                .element
                .as_any_mut()
                .downcast_mut::<Text>()
                .unwrap()
                .set_text("p中🙂rojected!");
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
        assert!(!super::super::frame_recorder::canonical_manifest_matches(
            &metadata, &full
        ));
    }
}

#[test]
fn text_area_projection_preedit_state_boundaries_fail_closed() {
    for case in ["selection", "scroll"] {
        let (arena, roots, root, ..) =
            prepared_projection_text_area_preedit_tree(8, "中", Some((0, 3)));
        {
            let mut node = arena.get_mut(root).unwrap();
            let text_area = node
                .element
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .unwrap();
            match case {
                "selection" => {
                    text_area.selection_anchor_char = Some(7);
                    text_area.selection_focus_char = Some(8);
                }
                "scroll" => text_area.scroll_x = 1.0,
                _ => unreachable!(),
            }
        }
        assert_text_area_fallback_before_full(&arena, &roots);
    }

    let (arena, _roots, _root, projection, _) =
        prepared_projection_text_area_preedit_tree(8, "中", Some((0, 3)));
    let projection_node = arena.get(projection).unwrap();
    assert_eq!(
        projection_node.element.shadow_paint_recording_capability(
            &arena,
            true,
            PaintRecordingContext {
                inside_text_area: true,
                ..Default::default()
            },
        ),
        ShadowPaintRecordingCapability::Legacy(ShadowPaintBlocker::Deferred)
    );
    drop(projection_node);
}
