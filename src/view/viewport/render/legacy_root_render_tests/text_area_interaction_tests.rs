use super::*;

#[test]
fn retained_auto_scroll_text_area_subtree_interaction_and_budget_fail_closed() {
    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let (arena, roots, _, _) = prepared_scroll_text_area_scene_with(
        0.0,
        0.0,
        "RetainedAuto must admit bounded internal TextArea scrolling without a ScrollNode",
    );
    let wrapper = arena.children_of(roots[0])[0];
    let text_area = arena.children_of(wrapper)[0];
    {
        let mut node = arena.get_mut(text_area).unwrap();
        let text_area = node
            .element
            .as_any_mut()
            .downcast_mut::<TextArea>()
            .unwrap();
        text_area.is_focused = true;
        text_area.caret_visible = true;
    }
    let (properties, generations) = synced_paint_state(&arena, &roots);
    let decision =
        select_retained_auto_authority(&arena, &roots, &properties, &generations, &ctx, true);
    let scene = match decision {
        AutoAuthorityDecision::PropertyScrollScene { scene, trace } => {
            assert!(matches!(
                trace.rejections.as_slice(),
                [AutoAuthorityRejection::PropertyScrollPlan { .. }]
            ));
            scene
        }
        AutoAuthorityDecision::Legacy { trace } => panic!(
            "focused TextArea must reach C2b/C2c retained authority: {:?}",
            trace.rejections
        ),
        _ => panic!("focused TextArea selected the wrong retained authority"),
    };
    assert!(scene.is_canonical());
    assert!(scene.rejects_synchronized_interactive_caret_width_tamper_for_test());
    assert!(scene.rejects_synchronized_interactive_caret_position_tamper_for_test());
    assert!(scene.rejects_synchronized_interactive_caret_height_tamper_for_test());
    let mut viewport = Viewport::new();
    let frame_owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut graph = FrameGraph::new();
    let graph_before = graph.build_state_snapshot_for_test();
    let prepared = crate::view::paint::prepare_retained_property_scroll_forest_from_pool(
        &mut viewport,
        scene,
        &mut graph,
        UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0),
        [0.0, 0.0, 0.0, 1.0],
        frame_owner,
    )
    .expect("interactive Single backing must prepare without graph mutation");
    assert_eq!(
        prepared.graph_build_state_snapshot_for_test(),
        graph_before,
        "successful interactive prepare must remain graph-inert"
    );
    let outcome = crate::view::paint::emit_prepared_retained_property_scroll_forest(prepared);
    let (state, trace) = outcome.into_parts();
    assert_eq!(state.opaque_rect_order(), 1);
    assert_eq!(
        trace.backing,
        crate::view::paint::ScrollSceneBackingKind::Single
    );
    assert_eq!(trace.tile_count, 1);
    assert_eq!(trace.reraster_count, 1);
    assert_ne!(graph.build_state_snapshot_for_test(), graph_before);
    assert_eq!(
        graph
            .test_graphics_passes::<crate::view::render_pass::texture_composite_pass::TextureCompositePass>()
            .len(),
        1
    );
    let pass_names = graph
        .pass_descriptors()
        .iter()
        .map(|pass| pass.name)
        .collect::<Vec<_>>();
    let composite = pass_names
        .iter()
        .position(|name| name.ends_with("TextureCompositePass"))
        .expect("resident base must composite");
    let caret = pass_names
        .iter()
        .position(|name| name.ends_with("OpaqueRectPass"))
        .expect("visible opaque caret must emit dynamically");
    assert!(composite < caret, "caret must follow resident composite");
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(frame_owner), true));

    let (arena, roots, _, _) = prepared_scroll_text_area_scene();
    let wrapper = arena.children_of(roots[0])[0];
    let text_area = arena.children_of(wrapper)[0];
    {
        let mut node = arena.get_mut(text_area).unwrap();
        let text_area = node
            .element
            .as_any_mut()
            .downcast_mut::<TextArea>()
            .unwrap();
        text_area.on_render_handler = Some(crate::ui::on_text_area_render(|render| {
            render.range(0..1, |_text_area| crate::ui::RsxNode::text("projection"))
        }));
    }
    let (properties, generations) = synced_paint_state(&arena, &roots);
    assert!(matches!(
        select_retained_auto_authority(&arena, &roots, &properties, &generations, &ctx, true,),
        AutoAuthorityDecision::Legacy { .. }
    ));

    for interactive in ["focused-selection", "focused-preedit"] {
        let (mut arena, roots, _, _) = prepared_scroll_text_area_scene();
        let wrapper = arena.children_of(roots[0])[0];
        let text_area = arena.children_of(wrapper)[0];
        {
            let mut node = arena.get_mut(text_area).unwrap();
            let text_area = node
                .element
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .unwrap();
            text_area.is_focused = true;
            match interactive {
                "focused-selection" => {
                    text_area.selection_anchor_char = Some(2);
                    text_area.selection_focus_char = Some(8);
                }
                "focused-preedit" => {
                    text_area.cursor_char = 2;
                    text_area.ime_preedit = "中".to_string();
                    text_area.ime_preedit_cursor = Some((0, "中".len()));
                    text_area.children_dirty = true;
                    text_area.bump_unified_ifc_source_revision();
                }
                _ => unreachable!(),
            }
        }
        if interactive == "focused-preedit" {
            arena.with_element_taken(text_area, |element, arena| {
                element.measure(
                    LayoutConstraints {
                        max_width: 108.0,
                        max_height: 28.0,
                        viewport_width: 320.0,
                        viewport_height: 240.0,
                        percent_base_width: Some(320.0),
                        percent_base_height: Some(240.0),
                    },
                    arena,
                );
                element.place(
                    LayoutPlacement {
                        parent_x: 0.0,
                        parent_y: -20.0,
                        visual_offset_x: 0.0,
                        visual_offset_y: 0.0,
                        available_width: 108.0,
                        available_height: 28.0,
                        viewport_width: 320.0,
                        viewport_height: 240.0,
                        percent_base_width: Some(320.0),
                        percent_base_height: Some(240.0),
                    },
                    arena,
                );
            });
            let mut stack = vec![roots[0]];
            while let Some(owner) = stack.pop() {
                stack.extend(arena.children_of(owner));
                arena
                    .get_mut(owner)
                    .unwrap()
                    .element
                    .clear_local_dirty_flags(DirtyFlags::ALL);
            }
            arena.clear_arena_dirty_subtree(roots[0], DirtyFlags::ALL);
            arena.refresh_subtree_dirty_cache(roots[0]);
        }
        let (properties, generations) = synced_paint_state(&arena, &roots);
        let root_node = arena.get(roots[0]).unwrap();
        let root_element = root_node
            .element
            .as_any()
            .downcast_ref::<Element>()
            .unwrap();
        assert!(
            root_element
                .exact_retained_scroll_interactive_text_area_subtree_admission(
                    roots[0], &arena, 1.0,
                )
                .is_some(),
            "{interactive} fixture must satisfy component admission"
        );
        let decision = select_retained_auto_authority(
            &arena,
            &roots,
            &properties,
            &generations,
            &ctx,
            true,
        );
        let (scene, trace) = match decision {
            AutoAuthorityDecision::PropertyScrollScene { scene, trace } => (scene, trace),
            AutoAuthorityDecision::Legacy { trace } => panic!(
                "{interactive} must reach validated graph-inert authority: {:?}",
                trace.rejections
            ),
            _ => panic!("{interactive} selected wrong retained authority"),
        };
        assert!(matches!(
            trace.rejections.as_slice(),
            [AutoAuthorityRejection::PropertyScrollPlan { .. }]
        ));
        assert!(scene.is_canonical());
    }

    for interaction in ["pointer", "pending-scroll"] {
        let (arena, roots, _, _) = prepared_scroll_text_area_scene();
        let wrapper = arena.children_of(roots[0])[0];
        let text_area = arena.children_of(wrapper)[0];
        {
            let mut node = arena.get_mut(text_area).unwrap();
            let text_area = node
                .element
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .unwrap();
            match interaction {
                "pointer" => text_area.pointer_selecting = true,
                "pending-scroll" => text_area.pending_caret_scroll = true,
                _ => unreachable!(),
            }
        }
        let (properties, generations) = synced_paint_state(&arena, &roots);
        let decision = select_retained_auto_authority(
            &arena,
            &roots,
            &properties,
            &generations,
            &ctx,
            true,
        );
        assert!(
            matches!(decision, AutoAuthorityDecision::PropertyScrollScene { .. }),
            "{interaction} is paint-neutral and must retain scroll-scene authority",
        );
    }

    for interaction in ["caret", "preedit", "preedit-selection"] {
        let (arena, roots, _, _) = prepared_scroll_text_area_scene();
        let wrapper = arena.children_of(roots[0])[0];
        let text_area = arena.children_of(wrapper)[0];
        {
            let mut node = arena.get_mut(text_area).unwrap();
            let text_area = node
                .element
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .unwrap();
            match interaction {
                "caret" => text_area.caret_visible = true,
                "preedit" => text_area.ime_preedit = "中".to_string(),
                "preedit-selection" => {
                    text_area.ime_preedit = "中".to_string();
                    text_area.selection_anchor_char = Some(2);
                    text_area.selection_focus_char = Some(8);
                }
                _ => unreachable!(),
            }
        }
        let (properties, generations) = synced_paint_state(&arena, &roots);
        let AutoAuthorityDecision::Legacy { trace } = select_retained_auto_authority(
            &arena,
            &roots,
            &properties,
            &generations,
            &ctx,
            true,
        ) else {
            panic!("{interaction} TextArea must remain whole-frame Legacy")
        };
        assert!(matches!(
            trace.rejections.first(),
            Some(AutoAuthorityRejection::PropertyScrollPlan { .. })
        ));
    }

    let (arena, roots, properties, generations) = prepared_scroll_text_area_scene();
    let graph = FrameGraph::new();
    let graph_before = graph.build_state_snapshot_for_test();
    let viewport = Viewport::new();
    let residents_before = viewport.compositor.retained_surfaces.clone();
    let budget = crate::view::paint::ScrollSceneSingleTextureBudget::new(4096, 1).unwrap();
    assert_eq!(
        crate::view::paint::plan_and_validate_property_scroll_scene(
            &arena,
            &roots,
            &properties,
            &generations,
            1.0,
            [0.0; 2],
            None,
            crate::time::Instant::now(),
            wgpu::TextureFormat::Bgra8Unorm,
            budget,
        )
        .err(),
        Some(crate::view::paint::PropertyScrollScenePlanError::BackingBudget)
    );
    assert_eq!(
        graph.build_state_snapshot_for_test(),
        graph_before,
        "C1 budget rejection must remain graph-inert"
    );
    assert_eq!(
        viewport.compositor.retained_surfaces, residents_before,
        "C1 budget rejection must remain pool-inert"
    );
}

#[test]
fn retained_auto_scroll_text_area_selection_is_exact_reusable_and_invalidating() {
    let select = |arena: &NodeArena,
                  roots: &[NodeKey],
                  properties: &PropertyTrees,
                  generations: &PaintGenerationTracker| {
        match select_retained_auto_authority(
            arena,
            roots,
            properties,
            generations,
            &UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0),
            true,
        ) {
            AutoAuthorityDecision::PropertyScrollScene { scene, trace } => {
                assert!(matches!(
                    trace.rejections.as_slice(),
                    [AutoAuthorityRejection::PropertyScrollPlan { .. }]
                ));
                scene
            }
            AutoAuthorityDecision::Legacy { trace } => {
                panic!("exact C2a selection rejected: {:?}", trace.rejections)
            }
            _ => panic!("exact C2a selection chose the wrong authority"),
        }
    };
    let prepare_emit =
        |viewport: &mut Viewport, scene: crate::view::paint::ValidatedPropertyScrollScene| {
            let owner = viewport.begin_retained_surface_frame_stage().unwrap();
            let mut graph = FrameGraph::new();
            let mut prepared =
                crate::view::paint::prepare_retained_property_scroll_forest_from_pool(
                    viewport,
                    scene,
                    &mut graph,
                    UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0),
                    [0.0, 0.0, 0.0, 1.0],
                    owner,
                )
                .expect("C2a scene prepares atomically");
            prepared.refresh_actions_from_committed_test_pool();
            let stamps = prepared.scroll_content_stamps_for_test();
            let [stamp] = stamps.as_slice() else {
                panic!("C2a single backing seals one stamp")
            };
            let stamp = stamp.clone();
            let outcome =
                crate::view::paint::emit_prepared_retained_property_scroll_forest(prepared);
            let (_state, trace) = outcome.into_parts();
            assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));
            (stamp, trace)
        };

    let (mut arena, roots, mut properties, mut generations) = prepared_scroll_text_area_scene();
    update_prepared_scroll_text_area_selection(
        &arena,
        &roots,
        &mut properties,
        &mut generations,
        (Some(2), Some(18)),
        None,
    );
    let wrapper = arena.children_of(roots[0])[0];
    let text_area = arena.children_of(wrapper)[0];
    let clip_id = crate::view::compositor::property_tree::ClipNodeId {
        owner: text_area,
        role: crate::view::compositor::property_tree::ClipNodeRole::ContentsClip,
    };
    let baseline_clip_generation = properties
        .clip_snapshot_for(Some(clip_id))
        .expect("C2a clip chain")[0]
        .generation;
    let baseline_self_revision = generations.snapshot(text_area).unwrap().self_paint_revision;
    let mut viewport = Viewport::new();
    let (baseline_stamp, baseline) = prepare_emit(
        &mut viewport,
        select(&arena, &roots, &properties, &generations),
    );
    assert_eq!((baseline.reraster_count, baseline.reuse_count), (1, 0));
    assert!(matches!(
        baseline_stamp.text_area_paint_grammar,
        Some(crate::view::base_component::text_area::RetainedTextAreaPaintGrammar::SelectionGlyphs {
            start_char: 2,
            end_char: 18,
            ..
        })
    ));
    assert_eq!(
        baseline_stamp
            .chunks
            .iter()
            .map(|chunk| chunk.id.role)
            .collect::<Vec<_>>(),
        vec![
            crate::view::paint::PaintChunkRole::SelfDecoration,
            crate::view::paint::PaintChunkRole::SelfDecoration,
            crate::view::paint::PaintChunkRole::SelectionUnderlay,
            crate::view::paint::PaintChunkRole::TextGlyphs,
            crate::view::paint::PaintChunkRole::SelfDecoration,
        ]
    );
    assert!(crate::view::paint::retained_surface_raster_stamp_is_canonical(&baseline_stamp));

    let pool_before_tamper = viewport.compositor.retained_surfaces.clone();
    let mut role = baseline_stamp.clone();
    role.chunks[1].id.role = crate::view::paint::PaintChunkRole::TextDecoration;
    let mut slot = baseline_stamp.clone();
    slot.chunks[1].id.slot = 1;
    let mut order = baseline_stamp.clone();
    order.chunks.swap(1, 2);
    let mut op_count = baseline_stamp.clone();
    op_count.chunks[1].op_count = 0;
    let mut payload = baseline_stamp.clone();
    payload.chunks[1].payload_identity = Default::default();
    let mut grammar_legal_range = baseline_stamp.clone();
    let Some(
        crate::view::base_component::text_area::RetainedTextAreaPaintGrammar::SelectionGlyphs {
            start_char,
            end_char,
            ..
        },
    ) = grammar_legal_range.text_area_paint_grammar.as_mut()
    else {
        unreachable!()
    };
    *start_char = 3;
    *end_char = 17;
    let legal_range_grammar = grammar_legal_range.text_area_paint_grammar.unwrap();
    let [crate::view::paint::RetainedSurfaceRasterStepStamp::ArtifactSpan(artifact_span)] =
        baseline_stamp.ordered_steps.as_slice()
    else {
        unreachable!()
    };
    assert!(
        crate::view::paint::validated_scroll_text_area_content_raster_stamp(
            baseline_stamp.identity.boundary_root,
            baseline_stamp.identity.stable_id,
            baseline_stamp.target.clone(),
            artifact_span.clone(),
            baseline_stamp.opaque_order_span.clone(),
            legal_range_grammar,
        )
        .is_none(),
        "the constructor seam must reject a legal range that does not match the sealed payload"
    );
    let mut grammar_range = baseline_stamp.clone();
    let Some(
        crate::view::base_component::text_area::RetainedTextAreaPaintGrammar::SelectionGlyphs {
            start_char,
            end_char,
            ..
        },
    ) = grammar_range.text_area_paint_grammar.as_mut()
    else {
        unreachable!()
    };
    *start_char = *end_char;
    let mut grammar_kind = baseline_stamp.clone();
    grammar_kind.text_area_paint_grammar =
        Some(crate::view::base_component::text_area::RetainedTextAreaPaintGrammar::GlyphOnly);
    let mut grammar_nan = baseline_stamp.clone();
    let Some(
        crate::view::base_component::text_area::RetainedTextAreaPaintGrammar::SelectionGlyphs {
            color_rgba_bits,
            ..
        },
    ) = grammar_nan.text_area_paint_grammar.as_mut()
    else {
        unreachable!()
    };
    color_rgba_bits[0] = f32::NAN.to_bits();
    let mut grammar_out_of_range = baseline_stamp.clone();
    let Some(
        crate::view::base_component::text_area::RetainedTextAreaPaintGrammar::SelectionGlyphs {
            color_rgba_bits,
            ..
        },
    ) = grammar_out_of_range.text_area_paint_grammar.as_mut()
    else {
        unreachable!()
    };
    color_rgba_bits[3] = 1.5_f32.to_bits();
    for tampered in [
        role,
        slot,
        order,
        op_count,
        payload,
        grammar_legal_range,
        grammar_range,
        grammar_kind,
        grammar_nan,
        grammar_out_of_range,
    ] {
        assert!(!crate::view::paint::retained_surface_raster_stamp_is_canonical(&tampered));
    }
    assert_eq!(viewport.compositor.retained_surfaces, pool_before_tamper);

    update_prepared_scroll_text_area_scene(
        &mut arena,
        &roots,
        &mut properties,
        &mut generations,
        30.0,
        9.0,
    );
    assert_ne!(
        properties
            .clip_snapshot_for(Some(clip_id))
            .expect("moved C2a clip chain")[0]
            .generation,
        baseline_clip_generation
    );
    assert_ne!(
        generations.snapshot(text_area).unwrap().self_paint_revision,
        baseline_self_revision
    );
    let (outer_stamp, outer) = prepare_emit(
        &mut viewport,
        select(&arena, &roots, &properties, &generations),
    );
    assert!(
        outer_stamp == baseline_stamp,
        "outer-only raw generation drift must preserve the retained raster stamp"
    );
    assert_eq!((outer.reraster_count, outer.reuse_count), (0, 1));

    update_prepared_scroll_text_area_selection(
        &arena,
        &roots,
        &mut properties,
        &mut generations,
        (Some(5), Some(24)),
        None,
    );
    let (range_stamp, range) = prepare_emit(
        &mut viewport,
        select(&arena, &roots, &properties, &generations),
    );
    assert!(
        range_stamp != outer_stamp,
        "selection-range drift must invalidate the retained raster stamp"
    );
    assert_eq!((range.reraster_count, range.reuse_count), (1, 0));
    assert_eq!(
        range_stamp
            .chunks
            .iter()
            .filter(|chunk| {
                chunk.id.role != crate::view::paint::PaintChunkRole::SelectionUnderlay
            })
            .map(|chunk| {
                (
                    chunk.id,
                    chunk.owner,
                    chunk.bounds_bits,
                    chunk.clip,
                    chunk.payload_identity.clone(),
                    chunk.op_count,
                )
            })
            .collect::<Vec<_>>(),
        outer_stamp
            .chunks
            .iter()
            .filter(|chunk| {
                chunk.id.role != crate::view::paint::PaintChunkRole::SelectionUnderlay
            })
            .map(|chunk| {
                (
                    chunk.id,
                    chunk.owner,
                    chunk.bounds_bits,
                    chunk.clip,
                    chunk.payload_identity.clone(),
                    chunk.op_count,
                )
            })
            .collect::<Vec<_>>(),
        "selection-range drift must preserve wrapper and glyph paint payloads",
    );

    update_prepared_scroll_text_area_selection(
        &arena,
        &roots,
        &mut properties,
        &mut generations,
        (Some(5), Some(24)),
        Some(Color::rgba(12, 34, 56, 128)),
    );
    let (color_stamp, color) = prepare_emit(
        &mut viewport,
        select(&arena, &roots, &properties, &generations),
    );
    assert!(
        color_stamp != range_stamp,
        "selection-color drift must invalidate the retained raster stamp"
    );
    assert_eq!((color.reraster_count, color.reuse_count), (1, 0));
    assert_eq!(
        color_stamp
            .chunks
            .iter()
            .filter(|chunk| {
                chunk.id.role != crate::view::paint::PaintChunkRole::SelectionUnderlay
            })
            .map(|chunk| {
                (
                    chunk.id,
                    chunk.owner,
                    chunk.bounds_bits,
                    chunk.clip,
                    chunk.payload_identity.clone(),
                    chunk.op_count,
                )
            })
            .collect::<Vec<_>>(),
        range_stamp
            .chunks
            .iter()
            .filter(|chunk| {
                chunk.id.role != crate::view::paint::PaintChunkRole::SelectionUnderlay
            })
            .map(|chunk| {
                (
                    chunk.id,
                    chunk.owner,
                    chunk.bounds_bits,
                    chunk.clip,
                    chunk.payload_identity.clone(),
                    chunk.op_count,
                )
            })
            .collect::<Vec<_>>(),
        "selection-color drift must preserve wrapper and glyph paint payloads",
    );

    update_prepared_scroll_text_area_scene(
        &mut arena,
        &roots,
        &mut properties,
        &mut generations,
        30.0,
        12.0,
    );
    let (local_scroll_stamp, local_scroll) = prepare_emit(
        &mut viewport,
        select(&arena, &roots, &properties, &generations),
    );
    assert!(matches!(
        local_scroll_stamp.text_area_paint_grammar,
        Some(
            crate::view::base_component::text_area::RetainedTextAreaPaintGrammar::SelectionGlyphs {
                start_char: 5,
                end_char: 24,
                ..
            }
        )
    ));
    assert!(
        local_scroll_stamp != color_stamp,
        "C2a local TextArea scroll must invalidate the selection resident stamp"
    );
    assert_eq!(
        (local_scroll.reraster_count, local_scroll.reuse_count),
        (1, 0)
    );
}
