use super::*;

#[test]
fn text_area_projection_selection_is_path_scoped_and_ordered() {
    let selected_fixture = || {
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
        (arena, roots, root, projection, projected_text)
    };
    eligible_whole_frame_rects(
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
        .shadow_paint_recording_context(&PaintRecordingContext::default());
    let projection_context = arena
        .get(root)
        .unwrap()
        .element
        .shadow_paint_recording_context_for_child(projection, &arena, &root_context);
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
            .shadow_paint_recording_context_for_child(sibling, &arena, &root_context);
        assert_eq!(
            sibling_context.text_area_selection, None,
            "selection authority must not leak to a TextArea sibling"
        );
    }
    let wrapper_context = arena
        .get(projection)
        .unwrap()
        .element
        .shadow_paint_recording_context(&projection_context);
    let text_context = arena
        .get(projection)
        .unwrap()
        .element
        .shadow_paint_recording_context_for_child(projected_text, &arena, &wrapper_context);
    assert_eq!(text_context.text_area_selection, Some(witness));

    let (properties, generations) = sync_identity(&arena, &roots);
    let artifact = whole_frame_artifact(&arena, &roots, &properties, &generations);
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
        .find(|chunk| chunk.owner == projected_text && chunk.id.role == PaintChunkRole::TextGlyphs)
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
    let artifact = whole_frame_artifact(&arena, &roots, &properties, &generations);
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

/// A projection may wrap its Text, as a badge does. The selection reaches
/// that Text through the wrapper, and no other child of it.
#[test]
fn text_area_projection_selection_reaches_a_wrapped_text() {
    let (arena, roots, root, projection, projected_text) =
        prepared_projection_text_area_tree_with("before projected after", 7..16, || {
            crate::ui::rsx! {
                <crate::view::Element>
                    <crate::view::Element />
                    <crate::view::Text>projected</crate::view::Text>
                </crate::view::Element>
            }
        });
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
    let (properties, generations) = sync_identity(&arena, &roots);
    let artifact = whole_frame_artifact(&arena, &roots, &properties, &generations);
    assert!(artifact.chunks.iter().any(|chunk| {
        chunk.owner == projected_text && chunk.id.role == PaintChunkRole::SelectionUnderlay
    }));

    let context_for_child = |parent: NodeKey, child: NodeKey, context: &PaintRecordingContext| {
        let node = arena.get(parent).unwrap();
        let own = node.element.shadow_paint_recording_context(context);
        node.element
            .shadow_paint_recording_context_for_child(child, &arena, &own)
    };
    let [wrapper] = arena.children_of(projection)[..] else {
        panic!("the projection renders one wrapper");
    };
    let projection_context = context_for_child(root, projection, &PaintRecordingContext::default());
    let wrapper_context = context_for_child(projection, wrapper, &projection_context);
    let witness = wrapper_context
        .text_area_selection
        .expect("the wrapper leads to the selected Text");
    assert_eq!(witness.target_owner, projected_text);
    for child in arena.children_of(wrapper) {
        let expected = (child == projected_text).then_some(witness);
        assert_eq!(
            context_for_child(wrapper, child, &wrapper_context).text_area_selection,
            expected
        );
    }
}

#[test]
fn text_area_atomic_projection_disjoint_root_selection_is_ordered() {
    let fixture = || {
        let (arena, roots, root, projection, projected_text) = prepared_projection_text_area_tree();
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
    eligible_whole_frame_rects(
        || {
            let (arena, roots, ..) = fixture();
            (arena, roots)
        },
        PaintParityConfig::default(),
    );

    let (arena, roots, root, projection, projected_text) = fixture();
    let (properties, generations) = sync_identity(&arena, &roots);
    let artifact = whole_frame_artifact(&arena, &roots, &properties, &generations);
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
        .shadow_paint_recording_context(&PaintRecordingContext::default());
    let projection_context = arena
        .get(root)
        .unwrap()
        .element
        .shadow_paint_recording_context_for_child(projection, &arena, &root_context);
    assert_eq!(
        projection_context.text_area_selection, None,
        "disjoint root selection must not mint projection-owned authority",
    );
}

#[test]
fn text_area_selection_crossing_projection_is_split_between_root_and_child() {
    let fixture = || {
        let (arena, roots, root, projection, projected_text) = prepared_projection_text_area_tree();
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
    eligible_whole_frame_rects(
        || {
            let (arena, roots, ..) = fixture();
            (arena, roots)
        },
        PaintParityConfig::default(),
    );

    let (arena, roots, root, projection, projected_text) = fixture();
    let (properties, generations) = sync_identity(&arena, &roots);
    let artifact = whole_frame_artifact(&arena, &roots, &properties, &generations);
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
        .shadow_paint_recording_context(&PaintRecordingContext::default());
    let projection_context = arena
        .get(root)
        .unwrap()
        .element
        .shadow_paint_recording_context_for_child(projection, &arena, &root_context);
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
            prepared_projection_text_area_tree_with("前🙂投影中文後", 2..6, || {
                crate::ui::RsxNode::text("投影中文")
            });
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
    eligible_whole_frame_rects(
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
        .shadow_paint_recording_context(&PaintRecordingContext::default());
    let witness = arena
        .get(root)
        .unwrap()
        .element
        .shadow_paint_recording_context_for_child(projection, &arena, &root_context)
        .text_area_selection
        .unwrap();
    assert_eq!((witness.local_start, witness.local_end), (1, 3));

    for mutate_fill in [false, true] {
        let (arena, roots, root, ..) = utf8_fixture();
        let (properties, generations) = sync_identity(&arena, &roots);
        let metadata = record_coverage_manifest(
            &arena,
            &roots,
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
        eligible_whole_frame_rects(
            || {
                let (arena, roots, ..) = fixture();
                (arena, roots)
            },
            PaintParityConfig::default(),
        );
        let (arena, roots, _root, projected_text) = fixture();
        let (properties, generations) = sync_identity(&arena, &roots);
        let artifact = whole_frame_artifact(&arena, &roots, &properties, &generations);
        assert!(
            artifact.chunks.iter().all(|chunk| {
                chunk.owner != projected_text && chunk.id.role != PaintChunkRole::SelectionUnderlay
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
            .shadow_paint_recording_capability(&arena, false, &PaintRecordingContext::default(),),
        ShadowPaintRecordingCapability::Transparent,
        "standalone invisible Text must close as transparent coverage"
    );
    let (properties, generations) = sync_identity(&arena, &roots);
    let artifact = whole_frame_artifact(&arena, &roots, &properties, &generations);
    assert!(
        artifact.chunks.is_empty(),
        "standalone invisible Text must not emit a typed zero-op chunk"
    );

    let (arena, _, root, projection, projected_text) = prepared_projection_text_area_tree();
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
        .shadow_paint_recording_context(&PaintRecordingContext::default());
    let projection_context = arena
        .get(root)
        .unwrap()
        .element
        .shadow_paint_recording_context_for_child(projection, &arena, &root_context);
    let wrapper_context = arena
        .get(projection)
        .unwrap()
        .element
        .shadow_paint_recording_context(&projection_context);
    let text_context = arena
        .get(projection)
        .unwrap()
        .element
        .shadow_paint_recording_context_for_child(projected_text, &arena, &wrapper_context);
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
            .shadow_paint_recording_capability(&arena, false, &text_context),
        ShadowPaintRecordingCapability::Transparent,
        "empty content must close the shared Text paint gate before selection"
    );
}
