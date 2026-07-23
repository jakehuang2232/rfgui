use super::*;

#[test]
fn retained_auto_interactive_text_area_reuses_dynamic_caret_and_invalidates_resident_base() {
    let make_scene = |kind: &str| {
        let (outer_scroll, local_scroll) = if kind == "culled" {
            (20.0, 9.0)
        } else {
            (0.0, 0.0)
        };
        let (mut arena, roots, _, _) = prepared_scroll_text_area_scene_with(
            outer_scroll,
            local_scroll,
            "Interactive TextArea resident identity separates caret from base raster",
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
            match kind {
                "visible" => text_area.caret_visible = true,
                "culled" | "outer-scrollbar" => text_area.caret_visible = true,
                "transparent" => {
                    text_area.caret_visible = true;
                    text_area.color = Color::rgba(0, 0, 0, 0);
                    text_area.children_dirty = true;
                    text_area.bump_unified_ifc_source_revision();
                }
                "hidden" => text_area.caret_visible = false,
                "cursor" => {
                    text_area.caret_visible = true;
                    text_area.cursor_char = 1;
                }
                "selection" => {
                    text_area.caret_visible = false;
                    text_area.selection_anchor_char = Some(0);
                    text_area.selection_focus_char = Some(2);
                }
                "preedit" => {
                    text_area.caret_visible = false;
                    text_area.cursor_char = 1;
                    text_area.ime_preedit = "中".to_string();
                    text_area.ime_preedit_cursor = Some((0, "中".len()));
                    text_area.children_dirty = true;
                    text_area.bump_unified_ifc_source_revision();
                }
                _ => unreachable!(),
            }
        }
        if kind == "outer-scrollbar" {
            crate::view::test_support::get_element_mut::<Element>(&arena, roots[0])
                .set_sampled_scrollbar_alpha_for_test(1.0);
        }
        if matches!(kind, "preedit" | "transparent") {
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
                        parent_y: 0.0,
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
        match select_retained_auto_authority(
            &arena,
            &roots,
            &properties,
            &generations,
            &UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0),
            true,
        ) {
            AutoAuthorityDecision::PropertyScrollScene { scene, .. } => scene,
            AutoAuthorityDecision::Legacy { trace } => {
                panic!(
                    "interactive {kind} fixture rejected: {:?}",
                    trace.rejections
                )
            }
            _ => panic!("interactive {kind} selected wrong authority"),
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
                .unwrap();
            prepared.refresh_actions_from_committed_test_pool();
            let stamps = prepared.scroll_content_stamps_for_test();
            let [stamp] = stamps.as_slice() else {
                panic!("interactive Single backing must have one resident stamp")
            };
            let stamp = stamp.clone();
            let outcome =
                crate::view::paint::emit_prepared_retained_property_scroll_forest(prepared);
            let (state, trace) = outcome.into_parts();
            let pass_names = graph
                .pass_descriptors()
                .iter()
                .map(|pass| pass.name)
                .collect::<Vec<_>>();
            assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));
            (stamp, state.opaque_rect_order(), trace, pass_names)
        };

    let (dynamic_arena, dynamic_roots, _, _) = prepared_scroll_text_area_scene_with(
        0.0,
        0.0,
        "Interactive TextArea resident identity separates caret from base raster",
    );
    let dynamic_wrapper = dynamic_arena.children_of(dynamic_roots[0])[0];
    let dynamic_text_area = dynamic_arena.children_of(dynamic_wrapper)[0];
    let select_dynamic = |arena: &NodeArena, roots: &[NodeKey]| {
        let (properties, generations) = synced_paint_state(arena, roots);
        match select_retained_auto_authority(
            arena,
            roots,
            &properties,
            &generations,
            &UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0),
            true,
        ) {
            AutoAuthorityDecision::PropertyScrollScene { scene, .. } => scene,
            AutoAuthorityDecision::Legacy { trace } => {
                panic!(
                    "dynamic interactive fixture rejected: {:?}",
                    trace.rejections
                )
            }
            _ => panic!("dynamic interactive fixture selected wrong authority"),
        }
    };
    {
        let mut node = dynamic_arena.get_mut(dynamic_text_area).unwrap();
        let text_area = node
            .element
            .as_any_mut()
            .downcast_mut::<TextArea>()
            .unwrap();
        text_area.is_focused = true;
        text_area.caret_visible = true;
    }
    let mut viewport = Viewport::new();
    let visible_scene = select_dynamic(&dynamic_arena, &dynamic_roots);
    assert_eq!(
        visible_scene.interactive_post_composite_opaque_delta_for_test(),
        Some(1)
    );
    let (base_stamp, visible_order, visible, visible_passes) =
        prepare_emit(&mut viewport, visible_scene);
    let synchronized_chunk_tamper =
        |stamp: &mut crate::view::paint::RetainedSurfaceRasterStamp,
         tamper: fn(&mut Vec<crate::view::paint::RetainedSurfaceChunkStamp>)| {
            tamper(&mut stamp.chunks);
            stamp.op_count = stamp.chunks.iter().map(|chunk| chunk.op_count).sum();
            let [crate::view::paint::RetainedSurfaceRasterStepStamp::ArtifactSpan(span)] =
                stamp.ordered_steps.as_mut_slice()
            else {
                panic!("interactive TextArea stamp must contain one artifact span")
            };
            tamper(&mut span.chunks);
            span.op_count = span.chunks.iter().map(|chunk| chunk.op_count).sum();
        };
    assert_eq!(
        base_stamp.chunks[1].id.slot,
        crate::view::paint::RETAINED_CHILD_MASK_SLOT
    );
    assert_eq!(
        base_stamp.chunks[3].id.slot,
        crate::view::paint::RETAINED_CHILD_MASK_SLOT
    );
    let mut missing_mask = base_stamp.clone();
    synchronized_chunk_tamper(&mut missing_mask, |chunks| {
        chunks.remove(3);
    });
    assert!(!crate::view::paint::retained_surface_raster_stamp_is_canonical(&missing_mask));
    let mut reordered_mask = base_stamp.clone();
    synchronized_chunk_tamper(&mut reordered_mask, |chunks| {
        chunks.swap(1, 3);
    });
    assert!(!crate::view::paint::retained_surface_raster_stamp_is_canonical(&reordered_mask));
    let mut wrong_mask_slot = base_stamp.clone();
    synchronized_chunk_tamper(&mut wrong_mask_slot, |chunks| {
        chunks[3].id.slot = 0;
    });
    assert!(!crate::view::paint::retained_surface_raster_stamp_is_canonical(&wrong_mask_slot));
    let mut wrong_mask_payload_pair = base_stamp.clone();
    synchronized_chunk_tamper(&mut wrong_mask_payload_pair, |chunks| {
        chunks[3].payload_identity = crate::view::paint::PaintPayloadIdentity::None;
    });
    assert!(
        !crate::view::paint::retained_surface_raster_stamp_is_canonical(
            &wrong_mask_payload_pair
        )
    );
    assert_eq!((visible.reraster_count, visible.reuse_count), (1, 0));
    assert_eq!(visible_order, 1);
    assert!(
        visible_passes
            .iter()
            .any(|name| name.ends_with("OpaqueRectPass"))
    );

    {
        let mut node = dynamic_arena.get_mut(dynamic_text_area).unwrap();
        node.element
            .as_any_mut()
            .downcast_mut::<TextArea>()
            .unwrap()
            .caret_visible = false;
    }
    let (hidden_stamp, hidden_order, hidden, hidden_passes) = prepare_emit(
        &mut viewport,
        select_dynamic(&dynamic_arena, &dynamic_roots),
    );
    assert_eq!(hidden_stamp, base_stamp);
    assert_eq!((hidden.reraster_count, hidden.reuse_count), (0, 1));
    assert_eq!(hidden_order, 0);
    assert!(
        !hidden_passes
            .iter()
            .any(|name| name.ends_with("OpaqueRectPass"))
    );

    {
        let mut node = dynamic_arena.get_mut(dynamic_text_area).unwrap();
        let text_area = node
            .element
            .as_any_mut()
            .downcast_mut::<TextArea>()
            .unwrap();
        text_area.caret_visible = true;
        text_area.cursor_char = 1;
    }
    let (cursor_stamp, cursor_order, cursor, _) = prepare_emit(
        &mut viewport,
        select_dynamic(&dynamic_arena, &dynamic_roots),
    );
    assert_eq!(cursor_stamp, base_stamp);
    assert_eq!((cursor.reraster_count, cursor.reuse_count), (0, 1));
    assert_eq!(cursor_order, 1);

    let (_, neutral_before_generations) = synced_paint_state(&dynamic_arena, &dynamic_roots);
    let neutral_before_revision = neutral_before_generations
        .snapshot(dynamic_text_area)
        .expect("interactive TextArea generation")
        .self_paint_revision;
    {
        let mut node = dynamic_arena.get_mut(dynamic_text_area).unwrap();
        node.element
            .as_any_mut()
            .downcast_mut::<TextArea>()
            .unwrap()
            .pointer_selecting = true;
    }
    let (_, pointer_generations) = synced_paint_state(&dynamic_arena, &dynamic_roots);
    assert_eq!(
        pointer_generations
            .snapshot(dynamic_text_area)
            .expect("pointer TextArea generation")
            .self_paint_revision,
        neutral_before_revision,
        "pointer capture state is not a paint revision input",
    );
    let (pointer_stamp, pointer_order, pointer, _) = prepare_emit(
        &mut viewport,
        select_dynamic(&dynamic_arena, &dynamic_roots),
    );
    assert_eq!(pointer_stamp, base_stamp);
    assert_eq!((pointer.reraster_count, pointer.reuse_count), (0, 1));
    assert_eq!(pointer_order, 1);

    {
        let mut node = dynamic_arena.get_mut(dynamic_text_area).unwrap();
        let text_area = node
            .element
            .as_any_mut()
            .downcast_mut::<TextArea>()
            .unwrap();
        text_area.pointer_selecting = false;
        text_area.pending_caret_scroll = true;
    }
    let (_, pending_generations) = synced_paint_state(&dynamic_arena, &dynamic_roots);
    assert_eq!(
        pending_generations
            .snapshot(dynamic_text_area)
            .expect("pending-scroll TextArea generation")
            .self_paint_revision,
        neutral_before_revision,
        "caret-follow scheduling is not a paint revision input",
    );
    let (pending_stamp, pending_order, pending, _) = prepare_emit(
        &mut viewport,
        select_dynamic(&dynamic_arena, &dynamic_roots),
    );
    assert_eq!(pending_stamp, base_stamp);
    assert_eq!((pending.reraster_count, pending.reuse_count), (0, 1));
    assert_eq!(pending_order, 1);

    let (selection_stamp, _, selection, _) =
        prepare_emit(&mut viewport, make_scene("selection"));
    assert_ne!(
        selection_stamp.interactive_text_area_resident,
        base_stamp.interactive_text_area_resident
    );
    assert_eq!((selection.reraster_count, selection.reuse_count), (1, 0));
    let base_glyph = base_stamp
        .chunks
        .iter()
        .find(|chunk| chunk.id.role == crate::view::paint::PaintChunkRole::TextGlyphs)
        .expect("focused base glyph chunk");
    let selection_glyph = selection_stamp
        .chunks
        .iter()
        .find(|chunk| chunk.id.role == crate::view::paint::PaintChunkRole::TextGlyphs)
        .expect("focused selection glyph chunk");
    assert_eq!(selection_glyph.id, base_glyph.id);
    assert_eq!(selection_glyph.owner, base_glyph.owner);
    assert_eq!(selection_glyph.bounds_bits, base_glyph.bounds_bits);
    assert_eq!(selection_glyph.clip, base_glyph.clip);
    assert_eq!(
        selection_glyph.payload_identity, base_glyph.payload_identity,
        "selection must preserve the exact prepared glyph payload",
    );
    assert_eq!(selection_glyph.op_count, base_glyph.op_count);

    let (preedit_stamp, _, preedit, _) = prepare_emit(&mut viewport, make_scene("preedit"));
    assert_ne!(
        preedit_stamp.interactive_text_area_resident,
        selection_stamp.interactive_text_area_resident
    );
    assert_eq!((preedit.reraster_count, preedit.reuse_count), (1, 0));
    let base_wrapper = base_stamp
        .chunks
        .iter()
        .find(|chunk| chunk.id.role == crate::view::paint::PaintChunkRole::SelfDecoration)
        .expect("interactive wrapper chunk");
    let preedit_wrapper = preedit_stamp
        .chunks
        .iter()
        .find(|chunk| chunk.id.role == crate::view::paint::PaintChunkRole::SelfDecoration)
        .expect("preedit wrapper chunk");
    assert_eq!(preedit_wrapper.id, base_wrapper.id);
    assert_eq!(preedit_wrapper.owner, base_wrapper.owner);
    assert_eq!(preedit_wrapper.bounds_bits, base_wrapper.bounds_bits);
    assert_eq!(preedit_wrapper.clip, base_wrapper.clip);
    assert_eq!(
        preedit_wrapper.payload_identity, base_wrapper.payload_identity,
        "preedit must invalidate text/decorations without perturbing wrapper paint",
    );
    assert_eq!(preedit_wrapper.op_count, base_wrapper.op_count);
    assert!(
        preedit_stamp.chunks.iter().any(|chunk| {
            chunk.id.role == crate::view::paint::PaintChunkRole::TextDecoration
        })
    );

    let culled_scene = make_scene("culled");
    assert!(culled_scene.interactive_caret_is_culled_for_test());
    assert_eq!(
        culled_scene.interactive_post_composite_opaque_delta_for_test(),
        Some(0)
    );
    let (_, culled_order, _, culled_passes) = prepare_emit(&mut viewport, culled_scene);
    assert_eq!(culled_order, 0);
    assert!(
        !culled_passes
            .iter()
            .any(|name| name.ends_with("OpaqueRectPass"))
    );

    let transparent_scene = make_scene("transparent");
    assert_eq!(
        transparent_scene.interactive_post_composite_opaque_delta_for_test(),
        Some(0)
    );
    let (_, transparent_order, _, transparent_passes) =
        prepare_emit(&mut viewport, transparent_scene);
    assert_eq!(transparent_order, 0);
    let transparent_composite = transparent_passes
        .iter()
        .position(|name| name.ends_with("TextureCompositePass"))
        .unwrap();
    let transparent_caret = transparent_passes
        .iter()
        .rposition(|name| name.ends_with("DrawRectPass"))
        .unwrap();
    assert!(transparent_composite < transparent_caret);

    let (_, _, _, scrollbar_passes) =
        prepare_emit(&mut viewport, make_scene("outer-scrollbar"));
    let composite = scrollbar_passes
        .iter()
        .position(|name| name.ends_with("TextureCompositePass"))
        .unwrap();
    let caret = scrollbar_passes
        .iter()
        .position(|name| name.ends_with("OpaqueRectPass"))
        .unwrap();
    let overlay = scrollbar_passes
        .iter()
        .rposition(|name| name.ends_with("DrawRectPass"))
        .unwrap();
    assert!(composite < caret && caret < overlay);

    let collision_scene = make_scene("visible");
    let (collision_key, collision_desc) = collision_scene
        .first_single_backing_declaration_for_test()
        .expect("interactive content must be Single-backed");
    let mut collision_viewport = Viewport::new();
    let collision_owner = collision_viewport
        .begin_retained_surface_frame_stage()
        .unwrap();
    let mut collision_graph = FrameGraph::new();
    let mut declaring_ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let _ = declaring_ctx.allocate_persistent_target_with_desc(
        &mut collision_graph,
        collision_desc,
        collision_key,
    );
    let graph_before = collision_graph.build_state_snapshot_for_test();
    let pool_before = collision_viewport.retained_surface_transaction_shape_for_test();
    let result = crate::view::paint::prepare_retained_property_scroll_forest_from_pool(
        &mut collision_viewport,
        collision_scene,
        &mut collision_graph,
        UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0),
        [0.0, 0.0, 0.0, 1.0],
        collision_owner,
    );
    assert_eq!(
        result.err(),
        Some(
            crate::view::paint::RetainedPropertyScrollScenePrepareError::PersistentKeyAlreadyDeclared(
                collision_key,
            ),
        )
    );
    assert_eq!(
        collision_graph.build_state_snapshot_for_test(),
        graph_before
    );
    assert_eq!(
        collision_viewport.retained_surface_transaction_shape_for_test(),
        pool_before
    );
    assert!(
        collision_viewport
            .finish_retained_surface_transaction_for_frame(Some(collision_owner), false,)
    );
}
