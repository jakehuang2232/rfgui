use super::*;

#[test]
fn nested_scroll_executor_cold_raster_warm_reuse_and_one_stage_lifecycle() {
    let ctx = || UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    let clear = [0.125, 0.25, 0.5, 1.0];
    let mut viewport = Viewport::new();

    let cold_owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut cold_graph = FrameGraph::new();
    let cold_geometry = prepared_nested_scroll_geometry_fixture();
    let expected_leaf_desc = cold_geometry.scene.leaf_stamp.target.color.clone();
    let expected_assembly_desc = cold_geometry.compiled.assembly.color_desc.clone();
    let expected_leaf_key = cold_geometry.scene.leaf_stamp.identity.color_key;
    let expected_depth_key = expected_leaf_key
        .depth_stencil()
        .expect("canonical R1 color key owns one depth key");
    let expected_persistent_keys = [expected_leaf_key, expected_depth_key]
        .into_iter()
        .collect::<FxHashSet<_>>();
    let expected_leaf_to_assembly = cold_geometry.compiled.leaf_to_assembly.params();
    let expected_assembly_to_root = cold_geometry.compiled.assembly_to_root.params();
    let expected_c0 = cold_geometry.compiled.outer_clip.logical_scissor;
    let expected_c01 = cold_geometry.compiled.leaf_to_assembly.scissor;
    let cold = prepare_nested_scroll_scene_from_pool(
        &mut viewport,
        cold_geometry,
        &mut cold_graph,
        ctx(),
        clear,
        cold_owner,
    )
    .unwrap();
    assert!(cold.graph_is_pristine_for_test());
    assert_eq!(cold.transaction_shape_for_test(), [1, 2, 0, 0, 1, 1]);
    assert_eq!(
        cold.action_for_test(),
        RetainedSurfaceCompileAction::Reraster
    );
    assert_eq!(cold.actions.len(), 1);
    assert_eq!(cold.terminals_for_test(), [1, 1, 1, 1, 1]);
    assert_eq!(cold.pool_shape_for_test(), (0, None));
    let cold_outcome = emit_prepared_nested_scroll_scene(cold);
    assert_eq!(cold_outcome.state.opaque_rect_order(), 1);
    assert_eq!(cold_outcome.state.target_pair_count_for_test(), 3);
    assert_eq!(cold_outcome.trace.reraster_count, 1);
    assert_eq!(cold_outcome.trace.reuse_count, 0);
    assert_eq!(
        cold_graph
            .declared_persistent_texture_keys()
            .collect::<FxHashSet<_>>(),
        expected_persistent_keys
    );
    assert!(!viewport.retained_property_scroll_scene_stage_is_available());

    let clears = cold_graph.test_graphics_passes::<ClearPass>();
    assert_eq!(clears.len(), 3, "root + A0 + cold R1 clears");
    assert_eq!(
        clears[0].test_snapshot().color_bits,
        clear.map(f32::to_bits)
    );
    assert_eq!(clears[1].test_snapshot().color_bits, [0.0_f32.to_bits(); 4]);
    assert_eq!(clears[2].test_snapshot().color_bits, [0.0_f32.to_bits(); 4]);
    let composites = cold_graph.test_graphics_passes::<TextureCompositePass>();
    assert_eq!(composites.len(), 2);
    let leaf_composite = composites[0].test_snapshot();
    let assembly_composite = composites[1].test_snapshot();
    assert_eq!(
        leaf_composite.bounds_bits,
        expected_leaf_to_assembly.bounds.map(f32::to_bits)
    );
    assert_eq!(leaf_composite.effective_scissor_rect, Some(expected_c01));
    assert_eq!(
        assembly_composite.bounds_bits,
        expected_assembly_to_root.bounds.map(f32::to_bits)
    );
    assert_eq!(assembly_composite.effective_scissor_rect, Some(expected_c0));
    assert_eq!(
        leaf_composite.output_target,
        assembly_composite.source_handle
    );
    assert_eq!(
        cold_graph
            .texture_desc_for_handle(leaf_composite.source_handle.unwrap())
            .unwrap(),
        &expected_leaf_desc
    );
    assert_eq!(
        cold_graph
            .texture_desc_for_handle(leaf_composite.output_target.unwrap())
            .unwrap(),
        &expected_assembly_desc
    );
    let rects = cold_graph.test_rect_pass_snapshots();
    assert_eq!(rects.len(), 3);
    assert_eq!(rects[0].effective_scissor_rect, None);
    assert_eq!(rects[1].effective_scissor_rect, Some(expected_c0));
    assert_eq!(rects[2].effective_scissor_rect, None);
    assert_eq!(rects[1].output_target, leaf_composite.output_target);
    assert_eq!(rects[2].output_target, leaf_composite.source_handle);
    let root_target = clears[0].test_snapshot().output_target;
    let assembly_target = leaf_composite.output_target;
    let leaf_target = leaf_composite.source_handle;
    let snapshot = cold_graph.test_compile_snapshot().unwrap();
    let payloads = snapshot.pass_payloads();
    assert_eq!(payloads.len(), 8);
    let position = |predicate: &dyn Fn(&FramePassTestPayload) -> bool| {
        payloads.iter().position(predicate).expect("sealed pass")
    };
    let root_clear = position(&|payload| {
        matches!(
            payload,
            FramePassTestPayload::Clear(pass)
                if pass.output_target == root_target
                    && pass.color_bits == clear.map(f32::to_bits)
        )
    });
    let outer_host = position(&|payload| {
        matches!(
            payload,
            FramePassTestPayload::DrawRect(rect)
                if rect.output_target == root_target && rect.effective_scissor_rect.is_none()
        )
    });
    let assembly_clear = position(&|payload| {
        matches!(
            payload,
            FramePassTestPayload::Clear(pass)
                if pass.output_target == assembly_target
                    && pass.color_bits == [0.0_f32.to_bits(); 4]
        )
    });
    let inner_host = position(&|payload| {
        matches!(
            payload,
            FramePassTestPayload::DrawRect(rect)
                if rect.output_target == assembly_target
                    && rect.effective_scissor_rect == Some(expected_c0)
        )
    });
    let leaf_clear = position(&|payload| {
        matches!(
            payload,
            FramePassTestPayload::Clear(pass)
                if pass.output_target == leaf_target
                    && pass.color_bits == [0.0_f32.to_bits(); 4]
        )
    });
    let leaf_draw = position(&|payload| {
        matches!(
            payload,
            FramePassTestPayload::DrawRect(rect)
                if rect.output_target == leaf_target && rect.effective_scissor_rect.is_none()
        )
    });
    let leaf_attach = position(&|payload| {
        matches!(
            payload,
            FramePassTestPayload::TextureComposite(composite)
                if composite.source_handle == leaf_target
                    && composite.output_target == assembly_target
                    && composite.effective_scissor_rect == Some(expected_c01)
        )
    });
    let assembly_attach = position(&|payload| {
        matches!(
            payload,
            FramePassTestPayload::TextureComposite(composite)
                if composite.source_handle == assembly_target
                    && composite.output_target == root_target
                    && composite.effective_scissor_rect == Some(expected_c0)
        )
    });
    assert!(root_clear < outer_host && outer_host < assembly_attach);
    assert!(assembly_clear < inner_host && inner_host < leaf_attach);
    assert!(leaf_clear < leaf_draw && leaf_draw < leaf_attach);
    assert!(leaf_attach < assembly_attach);
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(cold_owner), true));

    let warm_owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut warm_graph = FrameGraph::new();
    let warm_geometry = prepared_nested_scroll_geometry_fixture();
    let mut warm = prepare_nested_scroll_scene_from_pool(
        &mut viewport,
        warm_geometry,
        &mut warm_graph,
        ctx(),
        clear,
        warm_owner,
    )
    .unwrap();
    warm.refresh_action_from_committed_test_pool();
    assert_eq!(warm.action_for_test(), RetainedSurfaceCompileAction::Reuse);
    let warm_outcome = emit_prepared_nested_scroll_scene(warm);
    assert_eq!(warm_outcome.state.opaque_rect_order(), 1);
    assert_eq!(warm_outcome.state.target_pair_count_for_test(), 3);
    assert_eq!(warm_outcome.trace.reraster_count, 0);
    assert_eq!(warm_outcome.trace.reuse_count, 1);
    assert_eq!(
        warm_graph
            .declared_persistent_texture_keys()
            .collect::<FxHashSet<_>>(),
        expected_persistent_keys
    );
    assert_eq!(warm_graph.test_graphics_passes::<ClearPass>().len(), 2);
    assert_eq!(
        warm_graph
            .test_graphics_passes::<TextureCompositePass>()
            .len(),
        2
    );
    assert_eq!(warm_graph.test_rect_pass_snapshots().len(), 2);
    assert!(!viewport.retained_property_scroll_scene_stage_is_available());
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(warm_owner), true));
    assert!(viewport.retained_property_scroll_scene_stage_is_available());
}

#[test]
fn nested_scroll_media_executor_cold_raster_and_warm_reuse_emit_the_closed_corpus() {
    let ctx = || UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    for kind in [NestedMediaLeafKind::Image, NestedMediaLeafKind::Svg] {
        let (arena, outer, _inner, _leaf, properties, generations) =
            nested_scroll_media_fixture(kind);
        let geometry = prepare_nested_scroll_receiver_geometry(
            compile_nested_scroll_fixture_parts(&arena, outer, &properties, &generations),
            wgpu::TextureFormat::Bgra8UnormSrgb,
            generous_budget(),
        )
        .unwrap_or_else(|error| panic!("{kind:?} executable geometry rejected: {error:?}"));
        let warm_geometry = geometry.clone();
        let mut viewport = Viewport::new();

        let cold_owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut cold_graph = FrameGraph::new();
        let cold = prepare_nested_scroll_scene_from_pool(
            &mut viewport,
            geometry,
            &mut cold_graph,
            ctx(),
            [0.0, 0.0, 0.0, 1.0],
            cold_owner,
        )
        .unwrap();
        assert_eq!(
            cold.action_for_test(),
            RetainedSurfaceCompileAction::Reraster
        );
        let cold = emit_prepared_nested_scroll_scene(cold);
        assert_eq!((cold.trace.reraster_count, cold.trace.reuse_count), (1, 0));
        assert_eq!(cold_graph.test_graphics_passes::<ClearPass>().len(), 3);
        assert_eq!(
            cold_graph
                .test_graphics_passes::<TextureCompositePass>()
                .len(),
            3,
            "{kind:?}: media draw plus R1->A0 and A0->root"
        );
        assert_eq!(
            cold_graph.test_rect_pass_snapshots().len(),
            3,
            "{kind:?}: two hosts plus one media decoration"
        );
        assert!(viewport.finish_retained_surface_transaction_for_frame(Some(cold_owner), true));

        let warm_owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut warm_graph = FrameGraph::new();
        let mut warm = prepare_nested_scroll_scene_from_pool(
            &mut viewport,
            warm_geometry,
            &mut warm_graph,
            ctx(),
            [0.0, 0.0, 0.0, 1.0],
            warm_owner,
        )
        .unwrap();
        warm.refresh_action_from_committed_test_pool();
        assert_eq!(warm.action_for_test(), RetainedSurfaceCompileAction::Reuse);
        let warm = emit_prepared_nested_scroll_scene(warm);
        assert_eq!((warm.trace.reraster_count, warm.trace.reuse_count), (0, 1));
        assert_eq!(warm_graph.test_graphics_passes::<ClearPass>().len(), 2);
        assert_eq!(
            warm_graph
                .test_graphics_passes::<TextureCompositePass>()
                .len(),
            2,
            "{kind:?}: warm leaf skips the media raster pass"
        );
        assert_eq!(warm_graph.test_rect_pass_snapshots().len(), 2);
        assert!(viewport.finish_retained_surface_transaction_for_frame(Some(warm_owner), true));
    }
}

#[test]
fn nested_scroll_media_two_frame_reuse_and_targeted_invalidation_matrix() {
    for (index, kind) in [NestedMediaLeafKind::Image, NestedMediaLeafKind::Svg]
        .into_iter()
        .enumerate()
    {
        let (mut arena, outer, _inner, leaf, _, _) = nested_scroll_media_fixture(kind);
        let baseline_scene = sync_nested_media_scene(&mut arena, outer, leaf, 3);
        let baseline_payload = nested_media_payload_identity(&baseline_scene);
        let baseline_stamp = baseline_scene.leaf_stamp.clone();
        let baseline_geometry = prepare_nested_scroll_receiver_geometry(
            baseline_scene,
            wgpu::TextureFormat::Bgra8UnormSrgb,
            generous_budget(),
        )
        .expect("shadow media baseline geometry");
        let mut viewport = Viewport::new();
        execute_nested_media_frame(
            &mut viewport,
            baseline_geometry.clone(),
            RetainedSurfaceCompileAction::Reraster,
        );
        execute_nested_media_frame(
            &mut viewport,
            baseline_geometry,
            RetainedSurfaceCompileAction::Reuse,
        );

        match kind {
            NestedMediaLeafKind::Image => {
                let crate::view::sampled_texture::SampledTextureId::Image(asset_id) =
                    baseline_payload.0
                else {
                    panic!("Image fixture must retain Image texture namespace")
                };
                crate::view::image_resource::replace_ready_image_for_test(
                    asset_id,
                    2,
                    2,
                    std::sync::Arc::from([0x31_u8; 16]),
                );
            }
            NestedMediaLeafKind::Svg => {
                arena
                    .get_mut(leaf)
                    .unwrap()
                    .element
                    .as_any_mut()
                    .downcast_mut::<Svg>()
                    .unwrap()
                    .replace_active_raster_generation_for_test(0x42);
            }
        }
        let generation_scene = sync_nested_media_scene(&mut arena, outer, leaf, 4);
        let generation_payload = nested_media_payload_identity(&generation_scene);
        assert_eq!(generation_payload.0, baseline_payload.0, "{kind:?}");
        assert_ne!(generation_payload.1, baseline_payload.1, "{kind:?}");
        assert_ne!(generation_payload.3, baseline_payload.3, "{kind:?}");
        assert_ne!(generation_scene.leaf_stamp, baseline_stamp, "{kind:?}");
        assert_eq!(
            generation_scene.leaf_stamp.identity.resident_key(),
            baseline_stamp.identity.resident_key(),
            "{kind:?}: generation drift invalidates the existing media resident"
        );
        execute_nested_media_frame(
            &mut viewport,
            prepare_nested_scroll_receiver_geometry(
                generation_scene,
                wgpu::TextureFormat::Bgra8UnormSrgb,
                generous_budget(),
            )
            .expect("generation-mutated media geometry"),
            RetainedSurfaceCompileAction::Reraster,
        );

        match kind {
            NestedMediaLeafKind::Image => {
                let crate::view::sampled_texture::SampledTextureId::Image(asset_id) =
                    generation_payload.0
                else {
                    unreachable!()
                };
                crate::view::image_resource::set_image_loading_for_test(asset_id);
            }
            NestedMediaLeafKind::Svg => {
                arena
                    .get_mut(leaf)
                    .unwrap()
                    .element
                    .as_any_mut()
                    .downcast_mut::<Svg>()
                    .unwrap()
                    .set_source(crate::view::SvgSource::Content(format!(
                        r##"<svg width="100" height="600" xmlns="http://www.w3.org/2000/svg"><desc>retained-slot-switch-{index}</desc></svg>"##
                    )));
            }
        }
        let loading_scene = sync_nested_media_scene(&mut arena, outer, leaf, 5);
        let scaffold = loading_scene
            .plan
            .nested_scroll_planning_scaffold()
            .unwrap();
        let super::super::super::frame_plan::NestedScrollSceneScheduledStep::ContentReceiver(receiver) =
            &scaffold.schedule.steps[2]
        else {
            unreachable!()
        };
        assert_eq!(
            receiver.artifact.artifact().chunks[0].id.role,
            PaintChunkRole::SelfDecoration,
            "{kind:?}: loading slot switches to the retained wrapper payload"
        );
        execute_nested_media_frame(
            &mut viewport,
            prepare_nested_scroll_receiver_geometry(
                loading_scene,
                wgpu::TextureFormat::Bgra8UnormSrgb,
                generous_budget(),
            )
            .expect("loading-wrapper media geometry"),
            RetainedSurfaceCompileAction::Reraster,
        );
    }
}

#[test]
fn nested_scroll_text_executor_cold_raster_and_warm_reuse_gate_the_text_pass() {
    let ctx = || UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    let (arena, outer, _inner, leaf, mut properties, mut generations) =
        nested_scroll_text_fixture();
    let geometry = prepare_nested_scroll_receiver_geometry(
        compile_nested_scroll_fixture_parts(&arena, outer, &properties, &generations),
        wgpu::TextureFormat::Bgra8UnormSrgb,
        generous_budget(),
    )
    .expect("standalone Text executable geometry");
    let baseline_leaf_stamp = geometry.scene.leaf_stamp.clone();
    let warm_geometry = geometry.clone();
    let mut viewport = Viewport::new();

    let cold_owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut cold_graph = FrameGraph::new();
    let cold = prepare_nested_scroll_scene_from_pool(
        &mut viewport,
        geometry,
        &mut cold_graph,
        ctx(),
        [0.0, 0.0, 0.0, 1.0],
        cold_owner,
    )
    .unwrap();
    assert_eq!(
        cold.action_for_test(),
        RetainedSurfaceCompileAction::Reraster
    );
    let cold = emit_prepared_nested_scroll_scene(cold);
    assert_eq!((cold.trace.reraster_count, cold.trace.reuse_count), (1, 0));
    assert_eq!(
        cold_graph
            .test_graphics_passes::<crate::view::render_pass::text_pass::TextPreparedInputPass>(
            )
            .len(),
        1
    );
    assert_eq!(
        cold_graph
            .test_graphics_passes::<TextureCompositePass>()
            .len(),
        2,
        "cold Text plus R1->A0 and A0->root composites"
    );
    assert_eq!(cold_graph.test_graphics_passes::<ClearPass>().len(), 3);
    assert_eq!(cold_graph.test_rect_pass_snapshots().len(), 2);
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(cold_owner), true));

    let warm_owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut warm_graph = FrameGraph::new();
    let mut warm = prepare_nested_scroll_scene_from_pool(
        &mut viewport,
        warm_geometry,
        &mut warm_graph,
        ctx(),
        [0.0, 0.0, 0.0, 1.0],
        warm_owner,
    )
    .unwrap();
    warm.refresh_action_from_committed_test_pool();
    assert_eq!(warm.action_for_test(), RetainedSurfaceCompileAction::Reuse);
    let warm = emit_prepared_nested_scroll_scene(warm);
    assert_eq!((warm.trace.reraster_count, warm.trace.reuse_count), (0, 1));
    assert_eq!(
        warm_graph
            .test_graphics_passes::<crate::view::render_pass::text_pass::TextPreparedInputPass>(
            )
            .len(),
        0
    );
    assert_eq!(
        warm_graph
            .test_graphics_passes::<TextureCompositePass>()
            .len(),
        2
    );
    assert_eq!(warm_graph.test_graphics_passes::<ClearPass>().len(), 2);
    assert_eq!(warm_graph.test_rect_pass_snapshots().len(), 2);
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(warm_owner), true));

    arena
        .get_mut(leaf)
        .unwrap()
        .element
        .as_any_mut()
        .downcast_mut::<Text>()
        .unwrap()
        .set_color(Color::rgb(210, 47, 83));
    arena.refresh_subtree_dirty_cache(outer);
    properties.sync(&arena, &[outer]);
    generations.sync(&arena, &[outer], &properties);
    let changed = prepare_nested_scroll_receiver_geometry(
        compile_nested_scroll_fixture_parts(&arena, outer, &properties, &generations),
        wgpu::TextureFormat::Bgra8UnormSrgb,
        generous_budget(),
    )
    .expect("Text color mutation keeps exact executable geometry");
    assert_ne!(changed.scene.leaf_stamp, baseline_leaf_stamp);

    let changed_owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut changed_graph = FrameGraph::new();
    let mut changed = prepare_nested_scroll_scene_from_pool(
        &mut viewport,
        changed,
        &mut changed_graph,
        ctx(),
        [0.0, 0.0, 0.0, 1.0],
        changed_owner,
    )
    .unwrap();
    changed.refresh_action_from_committed_test_pool();
    assert_eq!(
        changed.action_for_test(),
        RetainedSurfaceCompileAction::Reraster,
        "legitimate Text paint mutation must not reuse the old R1"
    );
    let changed = emit_prepared_nested_scroll_scene(changed);
    assert_eq!(
        (changed.trace.reraster_count, changed.trace.reuse_count),
        (1, 0)
    );
    assert_eq!(
        changed_graph
            .test_graphics_passes::<crate::view::render_pass::text_pass::TextPreparedInputPass>(
            )
            .len(),
        1
    );
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(changed_owner), true));
}
