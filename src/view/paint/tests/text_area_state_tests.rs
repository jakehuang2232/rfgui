use super::*;

#[test]
fn plain_text_area_placeholder_newline_fractional_and_empty_cases_match_legacy() {
    for (content, placeholder, width, origin) in [
        ("", "placeholder text", 108.0, [7.25, 11.75]),
        ("first\nsecond", "", 108.0, [7.25, 11.75]),
        (
            "soft wrapping text across several visual lines",
            "",
            64.0,
            [13.375, 17.625],
        ),
    ] {
        let config = PaintParityConfig::default();
        assert_whole_frame_structural_parity(
            || {
                let (arena, roots, _) =
                    prepared_plain_text_area_tree_with(content, placeholder, width, origin);
                (arena, roots)
            },
            config,
        );
    }

    let (arena, roots, root) = prepared_plain_text_area_tree("");
    let (properties, generations) = sync_identity(&arena, &roots);
    take_full_artifact_record_count();
    let (artifact, eligibility) =
        whole_frame_artifact(&arena, &roots, &properties, &generations);
    assert!(eligibility.eligible);
    assert!(artifact.chunks.is_empty());
    assert!(artifact.ops.is_empty());
    assert_eq!(take_full_artifact_record_count(), 0);
    assert!(arena.children_of(root).is_empty());
}

#[test]
fn plain_text_area_unsafe_stateful_states_fail_before_full_hooks() {
    for case in ["selection_mixed", "preedit", "scroll"] {
        let (arena, roots, root) = prepared_plain_text_area_tree("state matrix");
        {
            let mut node = arena.get_mut(root).unwrap();
            let text_area = node
                .element
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .unwrap();
            match case {
                "selection_mixed" => {
                    text_area.selection_anchor_char = Some(0);
                    text_area.selection_focus_char = None;
                }
                "preedit" => text_area.ime_preedit = "x".to_string(),
                "scroll" => text_area.scroll_x = -0.0,
                _ => unreachable!(),
            }
        }
        let eligibility = assert_text_area_fallback_before_full(&arena, &roots);
        assert!(!eligibility.eligible, "{case}");
    }
}

#[test]
fn plain_text_area_paint_neutral_transient_states_remain_recordable() {
    for case in [
        "pointer",
        "pending_scroll",
        "realized_zero_projection_handler",
    ] {
        let (arena, roots, root) = prepared_plain_text_area_tree("state matrix");
        {
            let mut node = arena.get_mut(root).unwrap();
            let text_area = node
                .element
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .unwrap();
            match case {
                "pointer" => text_area.pointer_selecting = true,
                "pending_scroll" => text_area.pending_caret_scroll = true,
                "realized_zero_projection_handler" => {
                    text_area.on_render_handler =
                        Some(crate::ui::on_text_area_render(|_render| {}));
                }
                _ => unreachable!(),
            }
        }
        let (properties, generations) = sync_identity(&arena, &roots);
        let (artifact, eligibility) =
            whole_frame_artifact(&arena, &roots, &properties, &generations);
        assert!(eligibility.eligible, "{case}: {eligibility:?}");
        assert!(artifact.chunks.iter().any(|chunk| {
            chunk.owner == root && chunk.id.role == PaintChunkRole::TextGlyphs
        }));
    }
}

#[test]
fn plain_text_area_stale_dirty_topology_and_range_drift_fail_closed() {
    let (arena, roots, root) = prepared_plain_text_area_tree("stale package");
    arena
        .get_mut(root)
        .unwrap()
        .element
        .as_any_mut()
        .downcast_mut::<TextArea>()
        .unwrap()
        .bump_unified_ifc_source_revision();
    assert_text_area_fallback_before_full(&arena, &roots);

    let (arena, roots, root) = prepared_plain_text_area_tree("direct mutation");
    let child = arena.children_of(root)[0];
    arena
        .get_mut(child)
        .unwrap()
        .element
        .as_any_mut()
        .downcast_mut::<TextAreaTextRun>()
        .unwrap()
        .text
        .push('!');
    assert_text_area_fallback_before_full(&arena, &roots);

    let (arena, roots, root) = prepared_plain_text_area_tree("range drift");
    let child = arena.children_of(root)[0];
    let wrong = 1..12;
    {
        let mut child_node = arena.get_mut(child).unwrap();
        child_node
            .element
            .as_any_mut()
            .downcast_mut::<TextAreaTextRun>()
            .unwrap()
            .char_range = wrong.clone();
    }
    {
        let mut root_node = arena.get_mut(root).unwrap();
        let text_area = root_node
            .element
            .as_any_mut()
            .downcast_mut::<TextArea>()
            .unwrap();
        text_area.child_char_ranges[0] = wrong.clone();
        text_area.tamper_cached_unified_segment_char_range_for_test(0, wrong);
    }
    assert_text_area_fallback_before_full(&arena, &roots);

    let (arena, roots, root) = prepared_plain_text_area_tree("dirty child");
    let child = arena.children_of(root)[0];
    arena.mark_dirty(child, DirtyFlags::LAYOUT);
    assert_text_area_fallback_before_full(&arena, &roots);

    let (mut arena, roots, root) = prepared_plain_text_area_tree("orphan child");
    let child = arena.children_of(root)[0];
    arena.set_parent(child, None);
    assert_text_area_fallback_before_full(&arena, &roots);

    let (mut arena, roots, root) = prepared_plain_text_area_tree("wrong parent");
    let child = arena.children_of(root)[0];
    let wrong_parent = commit_element(
        &mut arena,
        Box::new(Element::new_with_id(0x7eff, 0.0, 0.0, 10.0, 10.0)),
    );
    arena.set_parent(child, Some(wrong_parent));
    assert_text_area_fallback_before_full(&arena, &roots);
}

#[test]
fn plain_text_area_live_empty_ignores_stale_package_and_apply_authority() {
    let (mut arena, roots, root) = prepared_plain_text_area_tree("clear me");
    arena.with_element_taken(root, |element, _arena| {
        element
            .as_any_mut()
            .downcast_mut::<TextArea>()
            .unwrap()
            .set_text(String::new());
    });
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    settle_plain_text_area(&arena, root);
    let text_area_node = arena.get(root).unwrap();
    let text_area = text_area_node
        .element
        .as_any()
        .downcast_ref::<TextArea>()
        .unwrap();
    assert!(text_area.last_unified_apply.get().is_some());
    let capability = text_area.shadow_paint_recording_capability(
        &arena,
        false,
        PaintRecordingContext::default(),
    );
    assert_eq!(
        capability,
        ShadowPaintRecordingCapability::Transparent,
        "children={:?} children_dirty={} local={:?} arena={:?} pending={}",
        text_area.children,
        text_area.children_dirty,
        text_area.dirty_flags,
        arena.arena_local_dirty(root),
        text_area.pending_caret_scroll,
    );
    drop(text_area_node);

    let (properties, generations) = sync_identity(&arena, &roots);
    take_full_artifact_record_count();
    let (artifact, eligibility) =
        whole_frame_artifact(&arena, &roots, &properties, &generations);
    assert!(eligibility.eligible);
    assert!(artifact.chunks.is_empty());
    assert!(artifact.ops.is_empty());
    assert_eq!(take_full_artifact_record_count(), 0);
}

#[test]
fn text_area_leaf_deferred_or_wrong_context_never_turns_transparent() {
    let (arena, _roots, root) = prepared_plain_text_area_tree("boundary");
    let root_node = arena.get(root).unwrap();
    let text_area = root_node
        .element
        .as_any()
        .downcast_ref::<TextArea>()
        .unwrap();
    assert_eq!(
        text_area.shadow_paint_recording_capability(
            &arena,
            true,
            PaintRecordingContext::default(),
        ),
        ShadowPaintRecordingCapability::Legacy(ShadowPaintBlocker::Deferred)
    );
    drop(root_node);

    let mut standalone = new_test_arena();
    let run = commit_element(
        &mut standalone,
        Box::new(TextAreaTextRun::new("orphan".to_string(), 0..6)),
    );
    let run_node = standalone.get(run).unwrap();
    assert_eq!(
        run_node.element.shadow_paint_recording_capability(
            &standalone,
            false,
            PaintRecordingContext::default(),
        ),
        ShadowPaintRecordingCapability::Unsupported
    );
}
