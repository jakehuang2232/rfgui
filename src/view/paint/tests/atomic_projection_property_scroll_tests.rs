use super::*;

#[test]
fn atomic_projection_selection_property_scroll_cold_warm_and_collision_are_closed_loop() {
    let ctx = || UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    let clear = [0.0, 0.0, 0.0, 1.0];
    let mut viewport = crate::view::viewport::Viewport::new();

    let cold_owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut cold_graph = FrameGraph::new();
    let cold_scene = validated_atomic_projection_selection_scroll_scene_at(6);
    assert!(cold_scene.is_canonical());
    assert!(cold_scene.atomic_projection_selection_contract_for_test());
    assert!(cold_scene.atomic_projection_selection_tamper_matrix_for_test());
    assert!(cold_scene.atomic_projection_selection_prepare_failure_matrix_is_atomic_for_test());
    let cold = super::super::scroll_scene::prepare_retained_property_scroll_forest_from_pool(
        &mut viewport,
        cold_scene,
        &mut cold_graph,
        ctx(),
        clear,
        cold_owner,
    )
    .expect("cold selection scene prepares without graph mutation");
    let cold_stamps = cold.scroll_content_stamps_for_test();
    let [cold_stamp] = cold_stamps.as_slice() else {
        panic!("one selection root owns one Single content stamp")
    };
    let cold_stamp = cold_stamp.clone();
    take_artifact_compile_count();
    let cold = super::super::scroll_scene::emit_prepared_retained_property_scroll_forest(cold);
    let (cold_state, cold_trace) = cold.into_parts();
    let cold_passes = cold_graph
        .pass_descriptors()
        .iter()
        .map(|descriptor| descriptor.name.rsplit("::").next().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        cold_passes,
        [
            "ClearPass",
            "DrawRectPass",
            "ClearPass",
            "DrawRectPass",
            "DrawRectPass",
            "DrawRectPass",
            "TextPreparedInputPass",
            "TextPreparedInputPass",
            "DrawRectPass",
            "TextureCompositePass",
        ],
        "root clear -> H -> content clear -> selection/root/projection local raster -> composite -> empty O",
    );
    assert_eq!((cold_trace.reraster_count, cold_trace.reuse_count), (1, 0));
    assert_eq!(take_artifact_compile_count(), 3, "cold emits H/C/O");
    assert_eq!(cold_state.opaque_rect_order(), 0);
    assert_eq!(
        cold_graph
            .test_graphics_passes::<crate::view::render_pass::ClearPass>()
            .len(),
        2
    );
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(cold_owner), true));

    let warm_owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut warm_graph = FrameGraph::new();
    let mut warm = super::super::scroll_scene::prepare_retained_property_scroll_forest_from_pool(
        &mut viewport,
        validated_atomic_projection_selection_scroll_scene_fixture(
            AtomicProjectionScrollFixture::baseline("projected", 44.0),
            6,
        ),
        &mut warm_graph,
        ctx(),
        clear,
        warm_owner,
    )
    .expect("outer-scroll-only selection scene prepares");
    warm.refresh_actions_from_committed_test_pool();
    let warm_stamps = warm.scroll_content_stamps_for_test();
    let [warm_stamp] = warm_stamps.as_slice() else {
        panic!("one warm selection content stamp")
    };
    assert_eq!(
        warm_stamp.identity.resident_key(),
        cold_stamp.identity.resident_key()
    );
    assert_eq!(
        warm_stamp, &cold_stamp,
        "outer scroll is composite-only state"
    );
    take_artifact_compile_count();
    let warm = super::super::scroll_scene::emit_prepared_retained_property_scroll_forest(warm);
    let (warm_state, warm_trace) = warm.into_parts();
    let warm_passes = warm_graph
        .pass_descriptors()
        .iter()
        .map(|descriptor| descriptor.name.rsplit("::").next().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        warm_passes,
        ["ClearPass", "DrawRectPass", "TextureCompositePass"]
    );
    assert_eq!((warm_trace.reraster_count, warm_trace.reuse_count), (0, 1));
    assert_eq!(take_artifact_compile_count(), 2, "reuse emits H/O only");
    assert_eq!(warm_state.opaque_rect_order(), 0);
    assert_eq!(
        warm_graph
            .test_graphics_passes::<crate::view::render_pass::ClearPass>()
            .len(),
        1
    );
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(warm_owner), true));

    let collision_scene = validated_atomic_projection_selection_scroll_scene_at(6);
    let (collision_key, collision_desc) = collision_scene
        .first_single_backing_declaration_for_test()
        .unwrap();
    let collision_owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut collision_graph = FrameGraph::new();
    let _ = collision_graph.declare_persistent_texture_internal::<
        crate::view::render_pass::draw_rect_pass::RenderTargetTag,
    >(collision_desc, collision_key);
    let graph_before = collision_graph.build_state_snapshot_for_test();
    let pool_before = viewport.retained_surface_transaction_shape_for_test();
    take_artifact_compile_count();
    assert_eq!(
        super::super::scroll_scene::prepare_retained_property_scroll_forest_from_pool(
            &mut viewport,
            collision_scene,
            &mut collision_graph,
            ctx(),
            clear,
            collision_owner,
        )
        .err(),
        Some(
            super::super::scroll_scene::RetainedPropertyScrollScenePrepareError::PersistentKeyAlreadyDeclared(
                collision_key,
            ),
        )
    );
    assert_eq!(
        take_artifact_compile_count(),
        0,
        "prepare cannot compile artifacts"
    );
    assert_eq!(
        collision_graph.build_state_snapshot_for_test(),
        graph_before
    );
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        pool_before
    );
    assert!(viewport.retained_surface_frame_stage_owner_is_active(collision_owner));
    assert!(
        viewport.finish_retained_surface_transaction_for_frame(Some(collision_owner), false)
    );

    let recovery_owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut recovery_graph = FrameGraph::new();
    let mut recovery = super::super::scroll_scene::prepare_retained_property_scroll_forest_from_pool(
        &mut viewport,
        validated_atomic_projection_selection_scroll_scene_at(6),
        &mut recovery_graph,
        ctx(),
        clear,
        recovery_owner,
    )
    .expect("collision cannot disturb committed selection resident");
    recovery.refresh_actions_from_committed_test_pool();
    let recovery = super::super::scroll_scene::emit_prepared_retained_property_scroll_forest(recovery);
    let (_, recovery_trace) = recovery.into_parts();
    assert_eq!(
        (recovery_trace.reraster_count, recovery_trace.reuse_count),
        (0, 1)
    );
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(recovery_owner), true));
}

#[test]
fn atomic_projection_selection_property_scroll_local_output_change_matrix_rerasterizes_same_resident()
 {
    let baseline = AtomicProjectionScrollFixture::baseline("projected", 20.0);
    let mut source = baseline;
    source.content = "source projected after";
    let mut style = baseline;
    style.font_size = 16.0;
    let mut payload = baseline;
    payload.projected_content = "projection";
    let mut geometry = baseline;
    geometry.content_height = 340.0;
    let mut topology = baseline;
    topology.projection_start = 6;
    topology.projection_end = 15;
    let mut local_clip = baseline;
    local_clip.width = 108.0;

    let ctx = || UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    for (name, fixture, selection_end) in [
        ("selection", baseline, 5),
        ("source", source, 6),
        ("style", style, 6),
        ("payload", payload, 6),
        ("geometry", geometry, 6),
        ("topology", topology, 6),
        ("local-clip", local_clip, 6),
    ] {
        let mut viewport = crate::view::viewport::Viewport::new();
        let cold_owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut cold_graph = FrameGraph::new();
        let cold = super::super::scroll_scene::prepare_retained_property_scroll_forest_from_pool(
            &mut viewport,
            validated_atomic_projection_selection_scroll_scene_fixture(baseline, 6),
            &mut cold_graph,
            ctx(),
            [0.0; 4],
            cold_owner,
        )
        .unwrap();
        let cold_stamps = cold.scroll_content_stamps_for_test();
        let [cold_stamp] = cold_stamps.as_slice() else {
            panic!("{name}: baseline owns one selection content stamp")
        };
        let cold_stamp = cold_stamp.clone();
        let _ = super::super::scroll_scene::emit_prepared_retained_property_scroll_forest(cold);
        assert!(viewport.finish_retained_surface_transaction_for_frame(Some(cold_owner), true));

        let changed_owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut changed_graph = FrameGraph::new();
        let mut changed =
            super::super::scroll_scene::prepare_retained_property_scroll_forest_from_pool(
                &mut viewport,
                validated_atomic_projection_selection_scroll_scene_fixture(
                    fixture,
                    selection_end,
                ),
                &mut changed_graph,
                ctx(),
                [0.0; 4],
                changed_owner,
            )
            .unwrap_or_else(|error| panic!("{name}: valid selection variant: {error:?}"));
        changed.refresh_actions_from_committed_test_pool();
        let changed_stamps = changed.scroll_content_stamps_for_test();
        let [changed_stamp] = changed_stamps.as_slice() else {
            panic!("{name}: changed scene owns one selection content stamp")
        };
        assert_eq!(
            changed_stamp.identity.resident_key(),
            cold_stamp.identity.resident_key(),
            "{name}: stable resident allocation identity"
        );
        assert_ne!(
            changed_stamp, &cold_stamp,
            "{name}: full local raster identity"
        );
        if name == "geometry" {
            assert_ne!(
                changed_stamp.target.source_bounds_bits,
                cold_stamp.target.source_bounds_bits
            );
        }
        if name == "local-clip" {
            assert_ne!(changed_stamp.clip_nodes, cold_stamp.clip_nodes);
        }
        take_artifact_compile_count();
        let changed =
            super::super::scroll_scene::emit_prepared_retained_property_scroll_forest(changed);
        let (_, trace) = changed.into_parts();
        assert_eq!((trace.reraster_count, trace.reuse_count), (1, 0), "{name}");
        assert_eq!(take_artifact_compile_count(), 3, "{name}: H/C/O");
        assert_eq!(
            changed_graph
                .test_graphics_passes::<crate::view::render_pass::ClearPass>()
                .len(),
            2,
            "{name}: root and content clear"
        );
        assert!(
            viewport.finish_retained_surface_transaction_for_frame(Some(changed_owner), true)
        );
    }
}

#[test]
fn atomic_projection_property_scroll_cold_warm_reuse_and_collision_are_closed_loop() {
    let ctx = || UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    let clear = [0.0, 0.0, 0.0, 1.0];
    let mut viewport = crate::view::viewport::Viewport::new();

    let cold_owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut cold_graph = FrameGraph::new();
    let cold = super::super::scroll_scene::prepare_retained_property_scroll_forest_from_pool(
        &mut viewport,
        validated_atomic_projection_scroll_scene_at("projected", 20.0),
        &mut cold_graph,
        ctx(),
        clear,
        cold_owner,
    )
    .expect("cold C3a scene prepares before graph mutation");
    let cold_stamps = cold.scroll_content_stamps_for_test();
    let [cold_stamp] = cold_stamps.as_slice() else {
        panic!("one C3a scroll root owns one Single content stamp")
    };
    let cold_stamp = cold_stamp.clone();
    take_artifact_compile_count();
    let cold = super::super::scroll_scene::emit_prepared_retained_property_scroll_forest(cold);
    let (cold_state, cold_trace) = cold.into_parts();
    let cold_passes = cold_graph
        .pass_descriptors()
        .iter()
        .map(|descriptor| descriptor.name.rsplit("::").next().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        cold_passes,
        [
            "ClearPass",
            "DrawRectPass",
            "ClearPass",
            "DrawRectPass",
            "DrawRectPass",
            "TextPreparedInputPass",
            "TextPreparedInputPass",
            "DrawRectPass",
            "TextureCompositePass",
        ],
        "root clear -> host -> detached wrapper/mask/content -> composite; the empty overlay still consumes the compiler token after composite",
    );
    assert_eq!((cold_trace.reraster_count, cold_trace.reuse_count), (1, 0));
    assert_eq!(take_artifact_compile_count(), 3, "cold emits H/C/O");
    assert_eq!(cold_state.opaque_rect_order(), 0);
    assert_eq!(
        cold_graph
            .test_graphics_passes::<crate::view::render_pass::ClearPass>()
            .len(),
        2,
        "root clear plus detached-content clear"
    );
    assert_eq!(
        cold_graph
            .test_graphics_passes::<crate::view::render_pass::TextureCompositePass>()
            .len(),
        1
    );
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(cold_owner), true));

    let warm_owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut warm_graph = FrameGraph::new();
    let mut warm = super::super::scroll_scene::prepare_retained_property_scroll_forest_from_pool(
        &mut viewport,
        validated_atomic_projection_scroll_scene_at("projected", 44.0),
        &mut warm_graph,
        ctx(),
        clear,
        warm_owner,
    )
    .expect("outer-scroll-only C3a scene prepares");
    warm.refresh_actions_from_committed_test_pool();
    let warm_stamps = warm.scroll_content_stamps_for_test();
    let [warm_stamp] = warm_stamps.as_slice() else {
        panic!("one warm C3a content stamp")
    };
    assert_eq!(
        warm_stamp.identity.resident_key(),
        cold_stamp.identity.resident_key()
    );
    assert_eq!(
        warm_stamp, &cold_stamp,
        "outer scroll is composite-only state"
    );
    take_artifact_compile_count();
    let warm = super::super::scroll_scene::emit_prepared_retained_property_scroll_forest(warm);
    let (warm_state, warm_trace) = warm.into_parts();
    let warm_passes = warm_graph
        .pass_descriptors()
        .iter()
        .map(|descriptor| descriptor.name.rsplit("::").next().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        warm_passes,
        ["ClearPass", "DrawRectPass", "TextureCompositePass"],
        "reuse keeps host/composite order and emits no detached-content passes",
    );
    assert_eq!((warm_trace.reraster_count, warm_trace.reuse_count), (0, 1));
    assert_eq!(
        take_artifact_compile_count(),
        2,
        "reuse emits H/O and replays the detached content cursor"
    );
    assert_eq!(warm_state.opaque_rect_order(), 0);
    assert_eq!(
        warm_graph
            .test_graphics_passes::<crate::view::render_pass::ClearPass>()
            .len(),
        1,
        "reuse has only the root clear"
    );
    assert_eq!(
        warm_graph
            .test_graphics_passes::<crate::view::render_pass::TextureCompositePass>()
            .len(),
        1
    );
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(warm_owner), true));

    let collision_scene = validated_atomic_projection_scroll_scene_at("projected", 44.0);
    let (collision_key, collision_desc) = collision_scene
        .first_single_backing_declaration_for_test()
        .unwrap();
    let collision_owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut collision_graph = FrameGraph::new();
    let _ = collision_graph.declare_persistent_texture_internal::<
        crate::view::render_pass::draw_rect_pass::RenderTargetTag,
    >(collision_desc, collision_key);
    let graph_before = collision_graph.build_state_snapshot_for_test();
    let pool_before = viewport.retained_surface_transaction_shape_for_test();
    assert_eq!(
        super::super::scroll_scene::prepare_retained_property_scroll_forest_from_pool(
            &mut viewport,
            collision_scene,
            &mut collision_graph,
            ctx(),
            clear,
            collision_owner,
        )
        .err(),
        Some(
            super::super::scroll_scene::RetainedPropertyScrollScenePrepareError::PersistentKeyAlreadyDeclared(
                collision_key,
            ),
        )
    );
    assert_eq!(
        collision_graph.build_state_snapshot_for_test(),
        graph_before
    );
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        pool_before
    );
    assert!(viewport.retained_surface_frame_stage_owner_is_active(collision_owner));
    assert!(
        viewport.finish_retained_surface_transaction_for_frame(Some(collision_owner), false)
    );

    let recovery_owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut recovery_graph = FrameGraph::new();
    let mut recovery = super::super::scroll_scene::prepare_retained_property_scroll_forest_from_pool(
        &mut viewport,
        validated_atomic_projection_scroll_scene_at("projected", 44.0),
        &mut recovery_graph,
        ctx(),
        clear,
        recovery_owner,
    )
    .expect("collision cannot disturb the committed resident");
    recovery.refresh_actions_from_committed_test_pool();
    let recovery = super::super::scroll_scene::emit_prepared_retained_property_scroll_forest(recovery);
    let (_, recovery_trace) = recovery.into_parts();
    assert_eq!(
        (recovery_trace.reraster_count, recovery_trace.reuse_count),
        (0, 1)
    );
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(recovery_owner), true));
}

#[test]
fn atomic_projection_property_scroll_local_output_change_matrix_rerasterizes_same_resident() {
    let baseline = AtomicProjectionScrollFixture::baseline("projected", 20.0);
    let mut source = baseline;
    source.content = "source projected after";
    let mut style = baseline;
    style.font_size = 16.0;
    let mut payload = baseline;
    payload.projected_content = "projection";
    let mut geometry = baseline;
    geometry.content_height = 340.0;
    let mut topology = baseline;
    topology.projection_start = 6;
    topology.projection_end = 15;
    let mut local_clip = baseline;
    local_clip.width = 108.0;

    let ctx = || UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    for (name, variant) in [
        ("source", source),
        ("style", style),
        ("payload", payload),
        ("geometry", geometry),
        ("topology", topology),
        ("local-clip", local_clip),
    ] {
        let mut viewport = crate::view::viewport::Viewport::new();
        let cold_owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut cold_graph = FrameGraph::new();
        let cold = super::super::scroll_scene::prepare_retained_property_scroll_forest_from_pool(
            &mut viewport,
            validated_atomic_projection_scroll_scene_fixture(baseline),
            &mut cold_graph,
            ctx(),
            [0.0; 4],
            cold_owner,
        )
        .unwrap();
        let cold_stamps = cold.scroll_content_stamps_for_test();
        let [cold_stamp] = cold_stamps.as_slice() else {
            panic!("{name}: baseline owns one content stamp")
        };
        let cold_stamp = cold_stamp.clone();
        let _ = super::super::scroll_scene::emit_prepared_retained_property_scroll_forest(cold);
        assert!(viewport.finish_retained_surface_transaction_for_frame(Some(cold_owner), true));

        let changed_owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut changed_graph = FrameGraph::new();
        let mut changed =
            super::super::scroll_scene::prepare_retained_property_scroll_forest_from_pool(
                &mut viewport,
                validated_atomic_projection_scroll_scene_fixture(variant),
                &mut changed_graph,
                ctx(),
                [0.0; 4],
                changed_owner,
            )
            .unwrap_or_else(|error| panic!("{name}: valid C3a variant: {error:?}"));
        changed.refresh_actions_from_committed_test_pool();
        let changed_stamps = changed.scroll_content_stamps_for_test();
        let [changed_stamp] = changed_stamps.as_slice() else {
            panic!("{name}: changed scene owns one content stamp")
        };
        assert_eq!(
            changed_stamp.identity.resident_key(),
            cold_stamp.identity.resident_key(),
            "{name}: resident identity"
        );
        assert_ne!(changed_stamp, &cold_stamp, "{name}: local raster output");
        if name == "geometry" {
            assert_ne!(
                changed_stamp.target.source_bounds_bits,
                cold_stamp.target.source_bounds_bits
            );
        }
        if name == "local-clip" {
            assert_ne!(changed_stamp.clip_nodes, cold_stamp.clip_nodes);
        }
        take_artifact_compile_count();
        let changed =
            super::super::scroll_scene::emit_prepared_retained_property_scroll_forest(changed);
        let (_, trace) = changed.into_parts();
        assert_eq!((trace.reraster_count, trace.reuse_count), (1, 0), "{name}");
        assert_eq!(take_artifact_compile_count(), 3, "{name}: H/C/O");
        assert_eq!(
            changed_graph
                .test_graphics_passes::<crate::view::render_pass::ClearPass>()
                .len(),
            2,
            "{name}: root and content clear"
        );
        assert!(
            viewport.finish_retained_surface_transaction_for_frame(Some(changed_owner), true)
        );
    }
}
