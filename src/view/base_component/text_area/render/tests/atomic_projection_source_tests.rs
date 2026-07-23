use super::*;

#[test]
fn retained_atomic_projection_source_oracle_is_exact_and_never_calls_handler() {
    let (arena, root, projection, projected_text, call_count) =
        retained_atomic_projection_fixture();
    assert_eq!(call_count.get(), 1, "layout realizes the handler once");
    let text_area_node = arena.get(root).unwrap();
    let text_area = text_area_node
        .element
        .as_any()
        .downcast_ref::<TextArea>()
        .unwrap();
    let grammar = text_area
        .exact_retained_property_scroll_atomic_projection_subtree(root, &arena, [0.0, 0.0])
        .expect("single bare Text projection must satisfy C3a source authority");
    assert!(grammar.is_canonical());
    assert_eq!(grammar.projection_owner, projection);
    assert_eq!(grammar.projection_text_owner, projected_text);
    assert_eq!(
        (grammar.projection_start_char, grammar.projection_end_char),
        (7, 16)
    );
    assert_eq!(
        grammar.projection_backing_start_byte,
        grammar.projection_backing_end_byte
    );
    assert_eq!(grammar.topology.len(), 3);
    assert_eq!(
        grammar.topology[grammar.projection_index].kind,
        super::super::super::RetainedAtomicProjectionTextAreaTopologyKind::ProjectionSegment
    );
    assert!(
        !text_area.exact_retained_property_scroll_glyph_subtree(root, &arena, [0.0, 0.0]),
        "C1 must remain projection-free"
    );
    assert!(
        text_area
            .exact_retained_property_scroll_selection_glyph_subtree(root, &arena, [0.0, 0.0])
            .is_none(),
        "C2a must remain projection-free"
    );
    assert!(
        text_area
            .exact_retained_property_scroll_interactive_subtree(root, &arena, [0.0, 0.0])
            .is_none(),
        "C2b/C2c must remain projection-free"
    );
    assert!(
        text_area
            .exact_retained_property_scroll_atomic_projection_subtree(root, &arena, [0.0, 0.0],)
            .is_some()
    );
    assert_eq!(
        call_count.get(),
        1,
        "source admission must never execute the FnMut handler"
    );
}

#[test]
fn retained_atomic_projection_source_oracle_rejects_stateful_and_interactive_states() {
    for case in [
        "focused",
        "caret",
        "selection",
        "preedit",
        "scroll_x",
        "scroll_y",
        "children_dirty",
    ] {
        let (arena, root, ..) = retained_atomic_projection_fixture();
        {
            let mut node = arena.get_mut(root).unwrap();
            let text_area = node
                .element
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .unwrap();
            match case {
                "focused" => text_area.is_focused = true,
                "caret" => text_area.caret_visible = true,
                "selection" => {
                    text_area.selection_anchor_char = Some(0);
                    text_area.selection_focus_char = Some(1);
                }
                "preedit" => text_area.ime_preedit = "x".to_string(),
                "scroll_x" => text_area.scroll_x = 1.0,
                "scroll_y" => text_area.scroll_y = 1.0,
                "children_dirty" => text_area.children_dirty = true,
                _ => unreachable!(),
            }
        }
        let node = arena.get(root).unwrap();
        let text_area = node.element.as_any().downcast_ref::<TextArea>().unwrap();
        assert!(
            text_area
                .exact_retained_property_scroll_atomic_projection_subtree(
                    root,
                    &arena,
                    [0.0, 0.0],
                )
                .is_none(),
            "{case}"
        );
    }
}

#[test]
fn retained_atomic_projection_sources_ignore_paint_neutral_interaction_flags() {
    for flag in ["pointer", "pending"] {
        let (arena, root, ..) = retained_atomic_projection_fixture();
        {
            let mut node = arena.get_mut(root).unwrap();
            let text_area = node
                .element
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .unwrap();
            match flag {
                "pointer" => text_area.pointer_selecting = true,
                "pending" => text_area.pending_caret_scroll = true,
                _ => unreachable!(),
            }
        }
        let node = arena.get(root).unwrap();
        let text_area = node.element.as_any().downcast_ref::<TextArea>().unwrap();
        assert!(
            text_area
                .exact_retained_property_scroll_atomic_projection_subtree(
                    root,
                    &arena,
                    [0.0, 0.0],
                )
                .is_some(),
            "{flag} must not change the realized atomic paint source",
        );
    }

    for flag in ["pointer", "pending"] {
        let (arena, root, ..) = retained_atomic_projection_fixture_with_selection(
            "before projected after",
            7..16,
            "projected",
            Some((0, 6)),
        );
        {
            let mut node = arena.get_mut(root).unwrap();
            let text_area = node
                .element
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .unwrap();
            match flag {
                "pointer" => text_area.pointer_selecting = true,
                "pending" => text_area.pending_caret_scroll = true,
                _ => unreachable!(),
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
                .is_some(),
            "{flag} must preserve the root-owned selection source",
        );
    }

    for flag in ["pointer", "pending"] {
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
            match flag {
                "pointer" => text_area.pointer_selecting = true,
                "pending" => text_area.pending_caret_scroll = true,
                _ => unreachable!(),
            }
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
                .is_some(),
            "{flag} must preserve the focused atomic paint source",
        );
    }
}

#[test]
fn retained_atomic_projection_source_oracle_rejects_package_geometry_and_topology_drift() {
    for case in [
        "segment_width",
        "flow_offset",
        "range",
        "source",
        "backing",
        "atomic_missing",
        "atomic_duplicate",
        "measurement_constraint",
        "measurement_size",
        "insertion",
        "orphan_projection",
        "dirty_projection",
        "extra_projection_child",
        "leaf_geometry",
        "leaf_invisible",
        "leaf_unprepared",
    ] {
        let (mut arena, root, projection, projection_text, _) =
            retained_atomic_projection_fixture();
        let projection_index = arena
            .children_of(root)
            .iter()
            .position(|child| *child == projection)
            .unwrap();
        match case {
            "segment_width" => {
                let width = arena
                    .get(projection)
                    .unwrap()
                    .element
                    .box_model_snapshot()
                    .width;
                arena.with_element_taken(projection, |element, _| {
                    element.set_layout_width(width + 1.0);
                });
            }
            "flow_offset" => {
                arena.with_element_taken(projection, |element, _| {
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
            "source" => arena
                .get(root)
                .unwrap()
                .element
                .as_any()
                .downcast_ref::<TextArea>()
                .unwrap()
                .tamper_cached_unified_segment_source_for_test(projection_index),
            "backing" => arena
                .get(root)
                .unwrap()
                .element
                .as_any()
                .downcast_ref::<TextArea>()
                .unwrap()
                .tamper_cached_unified_segment_backing_range_for_test(projection_index, 0..1),
            "atomic_missing" | "atomic_duplicate" => arena
                .get(root)
                .unwrap()
                .element
                .as_any()
                .downcast_ref::<TextArea>()
                .unwrap()
                .tamper_cached_unified_atomic_sources_for_test(case == "atomic_duplicate"),
            "measurement_constraint" | "measurement_size" => arena
                .get(root)
                .unwrap()
                .element
                .as_any()
                .downcast_ref::<TextArea>()
                .unwrap()
                .tamper_cached_unified_atomic_measurement_for_test(
                    projection_index,
                    case == "measurement_constraint",
                ),
            "insertion" => arena
                .get(root)
                .unwrap()
                .element
                .as_any()
                .downcast_ref::<TextArea>()
                .unwrap()
                .tamper_cached_unified_atomic_insertion_for_test(projection_index),
            "orphan_projection" => arena.set_parent(projection, None),
            "dirty_projection" => arena.mark_dirty(projection, DirtyFlags::LAYOUT),
            "extra_projection_child" => {
                let extra = crate::view::test_support::commit_element(
                    &mut arena,
                    Box::new(crate::view::base_component::Element::new_with_id(
                        0xc3a_1001, 0.0, 0.0, 1.0, 1.0,
                    )),
                );
                arena.set_parent(extra, Some(projection));
                arena.set_children(projection, vec![projection_text, extra]);
            }
            "leaf_geometry" => arena
                .get_mut(projection_text)
                .unwrap()
                .element
                .as_any_mut()
                .downcast_mut::<Text>()
                .unwrap()
                .tamper_layout_position_for_test(1.0, 0.0),
            "leaf_invisible" => arena
                .get_mut(projection_text)
                .unwrap()
                .element
                .as_any_mut()
                .downcast_mut::<Text>()
                .unwrap()
                .set_should_render_for_test(false),
            "leaf_unprepared" => arena
                .get_mut(projection_text)
                .unwrap()
                .element
                .as_any_mut()
                .downcast_mut::<Text>()
                .unwrap()
                .clear_prepared_standalone_text_for_test(),
            _ => unreachable!(),
        }
        let node = arena.get(root).unwrap();
        let text_area = node.element.as_any().downcast_ref::<TextArea>().unwrap();
        assert!(
            text_area
                .exact_retained_property_scroll_atomic_projection_subtree(
                    root,
                    &arena,
                    [0.0, 0.0],
                )
                .is_none(),
            "{case}"
        );
    }
}

#[test]
fn retained_atomic_projection_source_oracle_keeps_outside_realized_grammars_legacy() {
    let (arena, root, ..) =
        retained_atomic_projection_fixture_with("projected", 0..9, "projected");
    let node = arena.get(root).unwrap();
    assert!(
        node.element
            .as_any()
            .downcast_ref::<TextArea>()
            .unwrap()
            .exact_retained_property_scroll_atomic_projection_subtree(root, &arena, [0.0, 0.0],)
            .is_none(),
        "whole-content projection without a root glyph is a later grammar"
    );

    let (arena, root, ..) =
        retained_atomic_projection_fixture_with("before projected after", 7..16, "");
    let node = arena.get(root).unwrap();
    assert!(
        node.element
            .as_any()
            .downcast_ref::<TextArea>()
            .unwrap()
            .exact_retained_property_scroll_atomic_projection_subtree(root, &arena, [0.0, 0.0],)
            .is_none(),
        "zero-op projection Text is outside C3a"
    );

    let (arena, root) = projection_fixture(0, true);
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
    let node = arena.get(root).unwrap();
    assert!(
        node.element
            .as_any()
            .downcast_ref::<TextArea>()
            .unwrap()
            .exact_retained_property_scroll_atomic_projection_subtree(root, &arena, [0.0, 0.0],)
            .is_none(),
        "Element -> Text projection remains Legacy"
    );

    for case in ["zero", "multiple"] {
        let (mut arena, root, ..) = retained_atomic_projection_fixture();
        arena.with_element_taken(root, |element, _| {
            let text_area = element.as_any_mut().downcast_mut::<TextArea>().unwrap();
            text_area.on_render_handler = Some(if case == "zero" {
                crate::ui::on_text_area_render(|_render| {})
            } else {
                crate::ui::on_text_area_render(|render| {
                    render.range(0..6, |_text_area| RsxNode::text("before"));
                    render.range(17..22, |_text_area| RsxNode::text("after"));
                })
            });
            text_area.children_dirty = true;
            text_area.bump_unified_ifc_source_revision();
            text_area.dirty_flags = DirtyFlags::ALL;
        });
        crate::view::test_support::measure_and_place(
            &mut arena,
            root,
            LayoutConstraints {
                max_width: 132.0,
                max_height: 240.0,
                viewport_width: 320.0,
                viewport_height: 240.0,
                percent_base_width: Some(320.0),
                percent_base_height: Some(240.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 132.0,
                available_height: 240.0,
                viewport_width: 320.0,
                viewport_height: 240.0,
                percent_base_width: Some(320.0),
                percent_base_height: Some(240.0),
            },
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
        let node = arena.get(root).unwrap();
        assert!(
            node.element
                .as_any()
                .downcast_ref::<TextArea>()
                .unwrap()
                .exact_retained_property_scroll_atomic_projection_subtree(
                    root,
                    &arena,
                    [0.0, 0.0],
                )
                .is_none(),
            "{case} projection handler"
        );
    }
}

#[test]
fn retained_atomic_projection_scroll_admission_is_graph_inert_and_exact() {
    let (arena, root, wrapper, text_area) = retained_atomic_projection_scroll_shell();
    let root_node = arena.get(root).unwrap();
    let root_element = root_node
        .element
        .as_any()
        .downcast_ref::<Element>()
        .unwrap();
    let admission = root_element
        .exact_retained_scroll_atomic_projection_text_area_subtree_admission(root, &arena, 1.0)
        .expect("C3a source shell must admit the exact sibling snapshot");
    assert_eq!(admission.boundary_root, root);
    assert_eq!(admission.content_wrapper, wrapper);
    assert_eq!(admission.text_area_root, text_area);
    assert!(admission.paint_grammar.is_canonical());
    assert!(
        root_element
            .exact_retained_scroll_text_area_subtree_admission(root, &arena, 1.0)
            .is_none(),
        "C1/C2 admission must not inherit C3a semantics"
    );
    let dpr2_admission = root_element
        .exact_retained_scroll_atomic_projection_text_area_subtree_admission(root, &arena, 2.0)
        .expect("device-aligned DPR2 geometry keeps the exact sibling descriptor");
    assert!(admission.bitwise_eq(&dpr2_admission));
    let device_aligned = |value: f32| {
        let device = value * 2.0;
        device.is_finite() && device.fract().to_bits() == 0.0_f32.to_bits()
    };
    assert!(
        [
            dpr2_admission.source_bounds.x,
            dpr2_admission.source_bounds.y,
            dpr2_admission.source_bounds.x + dpr2_admission.source_bounds.width,
            dpr2_admission.source_bounds.y + dpr2_admission.source_bounds.height,
            dpr2_admission.scroll.scrollport_rect.x,
            dpr2_admission.scroll.scrollport_rect.y,
            dpr2_admission.scroll.scrollport_rect.x
                + dpr2_admission.scroll.scrollport_rect.width,
            dpr2_admission.scroll.scrollport_rect.y
                + dpr2_admission.scroll.scrollport_rect.height,
        ]
        .into_iter()
        .all(device_aligned)
    );
    assert!(
        root_element
            .exact_retained_scroll_atomic_projection_text_area_subtree_admission(
                root, &arena, 0.0,
            )
            .is_none()
    );
    assert!(
        root_element
            .exact_retained_scroll_atomic_projection_text_area_subtree_admission(
                root,
                &arena,
                f32::NAN,
            )
            .is_none()
    );
    drop(root_node);

    arena
        .get_mut(root)
        .unwrap()
        .element
        .as_any_mut()
        .downcast_mut::<Element>()
        .unwrap()
        .layout_state
        .layout_position
        .x += 0.25;
    let root_node = arena.get(root).unwrap();
    let root_element = root_node
        .element
        .as_any()
        .downcast_ref::<Element>()
        .unwrap();
    assert!(
        root_element
            .exact_retained_scroll_atomic_projection_text_area_subtree_admission(
                root, &arena, 2.0,
            )
            .is_none()
    );
}
