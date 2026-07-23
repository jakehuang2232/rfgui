use super::*;

#[test]
fn focused_atomic_projection_source_seals_visible_hidden_and_culled_caret_without_handler() {
    for (case, caret_visible, parent_paint_offset) in [
        ("visible", true, [0.0, 0.0]),
        ("hidden", false, [0.0, 0.0]),
        // Source authority does not classify clip visibility. A far-away
        // present caret is still sealed and can be culled only later.
        ("culled_source", true, [10_000.0, 10_000.0]),
    ] {
        let (arena, root, projection, _, call_count) = retained_atomic_projection_fixture();
        {
            let mut node = arena.get_mut(root).unwrap();
            let text_area = node
                .element
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .unwrap();
            text_area.is_focused = true;
            text_area.caret_visible = caret_visible;
            text_area.cursor_char = 7;
        }
        let node = arena.get(root).unwrap();
        let text_area = node.element.as_any().downcast_ref::<TextArea>().unwrap();
        let grammar = text_area
            .exact_retained_property_scroll_focused_atomic_projection_glyph_subtree(
                root,
                &arena,
                parent_paint_offset,
            )
            .unwrap_or_else(|| panic!("{case} focused source must admit"));
        assert!(grammar.is_canonical(), "{case}");
        assert_eq!(grammar.atomic_source.projection_owner, projection, "{case}");
        assert_eq!(grammar.caret.cursor_char, 7, "{case}");
        match (&grammar.caret.paint, caret_visible) {
            (super::super::super::FocusedAtomicCaretSourcePaintSeal::Hidden, false) => {}
            (
                super::super::super::FocusedAtomicCaretSourcePaintSeal::Present {
                    bounds_bits,
                    payload_identity,
                },
                true,
            ) => {
                assert_eq!(bounds_bits[2], 1.0_f32.to_bits(), "{case}");
                assert!(matches!(
                    payload_identity,
                    crate::view::paint::PaintPayloadIdentity::PreparedRects(rects)
                        if rects.len() == 1
                ));
            }
            _ => panic!("{case} caret source classification drifted"),
        }
        assert_eq!(
            call_count.get(),
            1,
            "{case}: source oracle must not rerun on_render",
        );
        assert!(
            text_area
                .exact_retained_property_scroll_atomic_projection_subtree(
                    root,
                    &arena,
                    parent_paint_offset,
                )
                .is_none(),
            "the existing non-focused atomic grammar must remain closed",
        );
    }
}

#[test]
fn focused_atomic_projection_source_rejects_interaction_stale_dirty_and_multi_states() {
    for case in [
        "unfocused",
        "selection",
        "collapsed_selection",
        "preedit",
        "ime_cursor",
        "scroll_x",
        "scroll_y",
        "animator",
        "deferred",
        "not_rendered",
        "cursor_oob",
        "children_dirty",
        "stale_package",
        "dirty_projection",
        "multi_projection_source",
    ] {
        let (arena, root, projection, _, call_count) = retained_atomic_projection_fixture();
        {
            let mut node = arena.get_mut(root).unwrap();
            let text_area = node
                .element
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .unwrap();
            text_area.is_focused = case != "unfocused";
            text_area.caret_visible = true;
            text_area.cursor_char = 7;
            match case {
                "selection" => {
                    text_area.selection_anchor_char = Some(0);
                    text_area.selection_focus_char = Some(1);
                }
                "collapsed_selection" => {
                    text_area.selection_anchor_char = Some(1);
                    text_area.selection_focus_char = Some(1);
                }
                "preedit" => text_area.ime_preedit = "x".to_string(),
                "ime_cursor" => text_area.ime_preedit_cursor = Some((0, 0)),
                "scroll_x" => text_area.scroll_x = 1.0,
                "scroll_y" => text_area.scroll_y = 1.0,
                "animator" => text_area.retained_source_test_active_animator = true,
                "deferred" => text_area.retained_source_test_deferred = true,
                "not_rendered" => text_area.layout_state.should_render = false,
                "cursor_oob" => text_area.cursor_char = text_area.content.chars().count() + 1,
                "children_dirty" => text_area.children_dirty = true,
                "stale_package" => text_area
                    .unified_ifc_source_revision
                    .set(text_area.unified_ifc_source_revision.get() + 1),
                "multi_projection_source" => {
                    text_area.tamper_cached_unified_atomic_sources_for_test(true)
                }
                "unfocused" | "dirty_projection" => {}
                _ => unreachable!(),
            }
        }
        if case == "dirty_projection" {
            arena.mark_dirty(projection, DirtyFlags::LAYOUT);
        }
        let node = arena.get(root).unwrap();
        let text_area = node.element.as_any().downcast_ref::<TextArea>().unwrap();
        assert!(
            text_area
                .exact_retained_property_scroll_focused_atomic_projection_glyph_subtree(
                    root,
                    &arena,
                    [0.0, 0.0],
                )
                .is_none(),
            "{case}",
        );
        assert_eq!(call_count.get(), 1, "{case}: callback count drifted");
    }
}

#[test]
fn focused_atomic_projection_source_private_identity_rejects_public_tamper() {
    let (arena, root, ..) = retained_atomic_projection_fixture();
    {
        let mut node = arena.get_mut(root).unwrap();
        let text_area = node
            .element
            .as_any_mut()
            .downcast_mut::<TextArea>()
            .unwrap();
        text_area.is_focused = true;
        text_area.caret_visible = true;
        text_area.cursor_char = 7;
    }
    let node = arena.get(root).unwrap();
    let text_area = node.element.as_any().downcast_ref::<TextArea>().unwrap();
    let grammar = text_area
        .exact_retained_property_scroll_focused_atomic_projection_glyph_subtree(
            root,
            &arena,
            [0.0, 0.0],
        )
        .unwrap();

    let mut cursor = grammar.clone();
    cursor.caret.cursor_char += 1;
    assert!(!cursor.is_canonical(), "cursor drift must fail closed");

    let mut style = grammar.clone();
    let changed_color_bits = [
        0.9_f32.to_bits(),
        0.2_f32.to_bits(),
        0.1_f32.to_bits(),
        1.0_f32.to_bits(),
    ];
    let super::super::super::FocusedAtomicCaretSourcePaintSeal::Present {
        bounds_bits,
        payload_identity,
    } = &mut style.caret.paint
    else {
        panic!("visible fixture must seal a present caret")
    };
    let [x, y, width, height] = bounds_bits.map(f32::from_bits);
    let changed_style_op = crate::view::paint::DrawRectOp {
        params: RectPassParams {
            position: [x, y],
            size: [width, height],
            fill_color: changed_color_bits.map(f32::from_bits),
            opacity: 1.0,
            ..Default::default()
        },
        mode: RectRenderMode::FillOnly,
    };
    *payload_identity =
        crate::view::paint::PaintPayloadIdentity::prepared_rects([&changed_style_op]).unwrap();
    style.caret.foreground_color_bits = changed_color_bits;
    assert!(
        !style.is_canonical(),
        "synchronized style and payload drift must fail closed",
    );

    let mut bounds = grammar.clone();
    let unchanged_color = bounds.caret.foreground_color_bits.map(f32::from_bits);
    let super::super::super::FocusedAtomicCaretSourcePaintSeal::Present {
        bounds_bits,
        payload_identity,
    } = &mut bounds.caret.paint
    else {
        panic!("visible fixture must seal a present caret")
    };
    bounds_bits[0] = (f32::from_bits(bounds_bits[0]) + 1.0).to_bits();
    let [x, y, width, height] = bounds_bits.map(f32::from_bits);
    let changed_bounds_op = crate::view::paint::DrawRectOp {
        params: RectPassParams {
            position: [x, y],
            size: [width, height],
            fill_color: unchanged_color,
            opacity: 1.0,
            ..Default::default()
        },
        mode: RectRenderMode::FillOnly,
    };
    *payload_identity =
        crate::view::paint::PaintPayloadIdentity::prepared_rects([&changed_bounds_op]).unwrap();
    assert!(
        !bounds.is_canonical(),
        "synchronized bounds and payload drift must fail closed",
    );

    let mut nested = grammar;
    nested.atomic_source.atomic_line_index += 1;
    assert!(
        !nested.is_canonical(),
        "nested private source identity must remain authoritative",
    );
}

#[test]
fn retained_atomic_projection_selection_source_is_root_owned_and_handler_free() {
    let (arena, root, projection, _, call_count) =
        retained_atomic_projection_fixture_with_selection(
            "before projected after",
            7..16,
            "projected",
            Some((0, 6)),
        );
    let node = arena.get(root).unwrap();
    let text_area = node.element.as_any().downcast_ref::<TextArea>().unwrap();
    let grammar = text_area
        .exact_retained_property_scroll_atomic_projection_selection_subtree(
            root,
            &arena,
            [0.0, 0.0],
        )
        .expect("root-owned selection plus one projection must admit");
    assert!(grammar.is_canonical());
    assert_eq!(grammar.atomic_source.projection_owner, projection);
    assert!(matches!(
        grammar.selection,
        super::super::super::RetainedTextAreaPaintGrammar::SelectionGlyphs {
            start_char: 0,
            end_char: 6,
            ..
        }
    ));
    assert_eq!(call_count.get(), 1, "oracle must not rerun on_render");
    assert!(
        text_area
            .exact_retained_property_scroll_atomic_projection_subtree(root, &arena, [0.0, 0.0],)
            .is_none(),
        "existing atomic glyph grammar must remain selection-free",
    );

    let mut synchronized = grammar.clone();
    synchronized.selection = super::super::super::RetainedTextAreaPaintGrammar::SelectionGlyphs {
        start_char: 1,
        end_char: 6,
        color_rgba_bits: match synchronized.selection {
            super::super::super::RetainedTextAreaPaintGrammar::SelectionGlyphs {
                color_rgba_bits,
                ..
            } => color_rgba_bits,
            _ => unreachable!(),
        },
    };
    assert!(
        !synchronized.is_canonical(),
        "private frozen identity must reject a still-valid synchronized public drift",
    );
}

#[test]
fn retained_atomic_projection_selection_source_rejects_outside_bounded_grammar() {
    for case in [
        "empty",
        "projection_owned",
        "crossing_projection",
        "focused",
        "caret",
        "preedit",
        "inner_scroll",
        "duplicate_atomic",
    ] {
        let (arena, root, ..) = retained_atomic_projection_fixture();
        {
            let mut node = arena.get_mut(root).unwrap();
            let text_area = node
                .element
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .unwrap();
            text_area.selection_anchor_char = Some(match case {
                "projection_owned" => 8,
                "crossing_projection" => 5,
                _ => 0,
            });
            text_area.selection_focus_char = Some(match case {
                "empty" => 0,
                "projection_owned" => 15,
                "crossing_projection" => 10,
                _ => 6,
            });
            match case {
                "focused" => text_area.is_focused = true,
                "caret" => text_area.caret_visible = true,
                "preedit" => text_area.ime_preedit = "x".to_string(),
                "inner_scroll" => text_area.scroll_y = 1.0,
                "duplicate_atomic" => {
                    text_area.tamper_cached_unified_atomic_sources_for_test(true)
                }
                _ => {}
            }
        }
        let node = arena.get(root).unwrap();
        let text_area = node.element.as_any().downcast_ref::<TextArea>().unwrap();
        assert!(
            text_area
                .exact_retained_property_scroll_atomic_projection_selection_subtree(
                    root,
                    &arena,
                    [0.0, 0.0],
                )
                .is_none(),
            "{case}",
        );
    }
}

#[test]
fn retained_atomic_projection_grammar_rejects_forged_leaf_constraints_and_geometry() {
    let (arena, root, ..) = retained_atomic_projection_fixture();
    let node = arena.get(root).unwrap();
    let text_area = node.element.as_any().downcast_ref::<TextArea>().unwrap();
    let grammar = text_area
        .exact_retained_property_scroll_atomic_projection_subtree(root, &arena, [0.0, 0.0])
        .unwrap();

    let mut tampered = grammar.clone();
    tampered.projection_text_stable_id = tampered.topology[0].stable_id;
    assert!(!tampered.is_canonical());

    let mut tampered = grammar.clone();
    tampered.measurement_constraints.available_height_bits = Some(1.0_f32.to_bits());
    assert!(!tampered.is_canonical());

    let mut tampered = grammar.clone();
    tampered.projection_text_bounds_bits[0] =
        (f32::from_bits(tampered.projection_text_bounds_bits[0]) + 1.0).to_bits();
    assert!(!tampered.is_canonical());

    let mut tampered = grammar.clone();
    tampered.atomic_insertion_byte += 1;
    assert!(!tampered.is_canonical());

    let mut tampered = grammar;
    tampered.flow_offset_bits[0] =
        (f32::from_bits(tampered.flow_offset_bits[0]) + 1.0).to_bits();
    assert!(!tampered.is_canonical());
}

#[test]
fn retained_atomic_projection_private_source_identity_rejects_synchronized_public_tamper() {
    let (arena, root, ..) = retained_atomic_projection_fixture();
    let node = arena.get(root).unwrap();
    let text_area = node.element.as_any().downcast_ref::<TextArea>().unwrap();
    let grammar = text_area
        .exact_retained_property_scroll_atomic_projection_subtree(root, &arena, [0.0, 0.0])
        .unwrap();

    let mut tampered = grammar.clone();
    tampered.atomic_line_index += 1;
    assert!(
        !tampered.is_canonical(),
        "atomic line is frozen source identity"
    );

    let mut tampered = grammar.clone();
    tampered.vertical_align = crate::style::VerticalAlign::Top;
    assert!(
        !tampered.is_canonical(),
        "vertical-align is frozen source identity"
    );

    let mut tampered = grammar.clone();
    tampered.last_unified_apply_bits.0 =
        (f32::from_bits(tampered.last_unified_apply_bits.0) + 1.0).to_bits();
    assert!(
        !tampered.is_canonical(),
        "finite apply-origin drift is frozen source identity"
    );

    let mut tampered = grammar;
    let topology = Arc::make_mut(&mut tampered.topology);
    assert_eq!(
        topology[0].kind,
        super::super::super::RetainedAtomicProjectionTextAreaTopologyKind::TextRun
    );
    topology[0].kind = super::super::super::RetainedAtomicProjectionTextAreaTopologyKind::LineBreak;
    assert!(
        !tampered.is_canonical(),
        "nonprojection topology kind is frozen source identity"
    );
}
