use super::*;

#[test]
fn generic_transition_and_selection_sources_reject_at_owner_before_mutation() {
    let (arena, root, wrapper, text_area) = prepared_atomic_projection_scroll_shell();
    {
        let mut node = arena.get_mut(text_area).unwrap();
        let text_area = node
            .element
            .as_any_mut()
            .downcast_mut::<TextArea>()
            .unwrap();
        text_area.selection_anchor_char = Some(0);
        text_area.selection_focus_char = Some(6);
    }
    let root_node = arena.get(root).unwrap();
    let root_element = root_node
        .element
        .as_any()
        .downcast_ref::<Element>()
        .unwrap();
    let admission = root_element
        .exact_retained_scroll_atomic_projection_selection_text_area_subtree_admission(
            root, &arena, 1.0,
        )
        .expect("selection projection fixture must admit");
    drop(root_node);
    let (properties, generations) = sync_identity(&arena, &[root]);
    let scroll = properties
        .scroll_snapshot_for(crate::view::compositor::property_tree::ScrollNodeId(root))
        .unwrap();
    let outer_clip = *properties
        .clip_snapshot_for(Some(ClipNodeId {
            owner: root,
            role: ClipNodeRole::ContentsClip,
        }))
        .unwrap()
        .last()
        .unwrap();
    let outer = PaintScrollContentWitness::new(root, wrapper, scroll, outer_clip).unwrap();
    assert!(
        super::super::frame_recorder::record_scroll_atomic_projection_selection_text_area_subtree_local_artifact_for_plan(
            &arena,
            &properties,
            &generations,
            &admission,
            outer,
        )
        .is_ok(),
        "baseline generic admission must record before the tamper matrix",
    );

    let mut viewport = viewport_with_committed_atomic_projection_selection_resident();
    let frame_owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let graph = FrameGraph::new();
    let graph_before = graph.build_state_snapshot_for_test();
    let pool_before = viewport.retained_surface_transaction_shape_for_test();
    let expected = vec![
        super::super::frame_recorder::FrameArtifactFallbackReason::PropertyBoundary(text_area),
    ];
    let rejects = |tampered| {
        assert_eq!(
            super::super::frame_recorder::record_scroll_atomic_projection_selection_text_area_subtree_local_artifact_for_plan(
                &arena,
                &properties,
                &generations,
                &tampered,
                outer,
            )
            .err(),
            Some(expected.clone()),
        );
    };

    for transition in [
        admission
            .artifact_space_transition
            .tamper_from_origin_for_test(0),
        admission
            .artifact_space_transition
            .tamper_from_origin_for_test(1),
        admission
            .artifact_space_transition
            .tamper_to_origin_for_test(0),
        admission
            .artifact_space_transition
            .tamper_to_origin_for_test(1),
        admission
            .artifact_space_transition
            .tamper_revision_for_test(),
    ] {
        let mut tampered = admission.clone();
        tampered.artifact_space_transition = transition;
        rejects(tampered);
    }

    let mut start = admission.clone();
    start.selection_source.start_char += 1;
    rejects(start);
    let mut end = admission.clone();
    end.selection_source.end_char += 1;
    rejects(end);
    let mut color = admission.clone();
    color.selection_source.color_rgba_bits[0] ^= 1;
    rejects(color);

    let mut synchronized_transition = admission.clone();
    synchronized_transition.artifact_space_transition = synchronized_transition
        .artifact_space_transition
        .tamper_revision_for_test();
    synchronized_transition
        .paint_grammar
        .atomic_source
        .last_unified_apply_bits
        .2 += 1;
    rejects(synchronized_transition);

    let mut synchronized_selection = admission;
    synchronized_selection.selection_source.end_char += 1;
    let crate::view::base_component::text_area::RetainedTextAreaPaintGrammar::SelectionGlyphs {
        end_char,
        ..
    } = &mut synchronized_selection.paint_grammar.selection
    else {
        panic!("selection fixture must retain selection source")
    };
    *end_char += 1;
    rejects(synchronized_selection);

    assert_eq!(graph.build_state_snapshot_for_test(), graph_before);
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        pool_before
    );
    assert!(viewport.retained_surface_frame_stage_owner_is_active(frame_owner));
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(frame_owner), false));
}
