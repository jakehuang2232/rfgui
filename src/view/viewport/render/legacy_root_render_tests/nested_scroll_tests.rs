use super::*;

#[test]
fn retained_auto_nested_scroll_selects_and_emits_one_atomic_cold_scene() {
    let ctx = UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    let (arena, roots, properties, generations) = prepared_exact_nested_scroll_scene();
    let decision =
        select_retained_auto_authority(&arena, &roots, &properties, &generations, &ctx, true);
    let AutoAuthorityDecision::NestedScrollScene { prepared, trace } = decision else {
        panic!("exact S0->S1->leaf must select the dedicated nested authority")
    };
    assert!(prepared.is_canonical());
    assert!(trace.rejections.is_empty());

    let mut viewport = Viewport::new();
    let owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut graph = FrameGraph::new();
    let graph_before = graph.build_state_snapshot_for_test();
    let pool_before = viewport.retained_surface_transaction_shape_for_test();
    assert_eq!(graph.build_state_snapshot_for_test(), graph_before);
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        pool_before
    );
    let (selection, outcome) = preflight_nested_scroll_selection(
        &mut viewport,
        &mut graph,
        ctx,
        [0.0, 0.0, 0.0, 1.0],
        Some(owner),
        RetainedTransformCanarySelection::NestedScrollScenePlanned(prepared),
    );
    assert!(matches!(
        selection,
        RetainedTransformCanarySelection::NestedScrollScenePrepared
    ));
    let (state, build_trace) = outcome.unwrap().into_parts();
    assert_eq!(state.opaque_rect_order(), 1);
    assert_eq!(build_trace.root_count, 1);
    assert_eq!(build_trace.generic_surface_count, 0);
    assert_eq!(build_trace.scroll_group_count, 1);
    assert_eq!(build_trace.reraster_count, 1);
    assert_eq!(build_trace.reuse_count, 0);
    assert!(nested_scroll_success_trace(&build_trace).contains("topology=S0->S1->leaf"));
    assert!(nested_scroll_success_trace(&build_trace).contains("a0=transient-keyless"));
    assert_eq!(graph.declared_persistent_texture_keys().count(), 2);
    let clears = graph.test_graphics_passes::<crate::view::frame_graph::ClearPass>();
    assert_eq!(clears.len(), 3, "root + A0 + cold R1 clears");
    let root_target = clears[0].test_snapshot().output_target;
    assert_eq!(
        clears
            .iter()
            .filter(|clear| clear.test_snapshot().output_target == root_target)
            .count(),
        1,
        "nested emit owns the root clear exactly once"
    );
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));

    let (arena, roots, properties, generations) = prepared_exact_nested_scroll_scene();
    let telemetry = telemetry_for_auto_decision(select_retained_auto_authority(
        &arena,
        &roots,
        &properties,
        &generations,
        &UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
        true,
    ));
    assert_eq!(
        telemetry.snapshot().authority_label,
        "retained-auto:property-scene"
    );
}

#[test]
fn retained_auto_loading_image_nested_leaf_uses_typed_active_wrapper() {
    let (arena, outer, properties, generations) =
        crate::view::paint::nested_scroll_unready_media_fixture_for_test(
            crate::view::paint::NestedMediaLeafKind::Image,
        );
    let roots = [outer];
    assert_eq!(properties.scrolls.len(), 2);
    let decision = select_retained_auto_authority(
        &arena,
        &roots,
        &properties,
        &generations,
        &UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
        true,
    );
    assert!(
        matches!(decision, AutoAuthorityDecision::NestedScrollScene { .. }),
        "a legal loading Image wrapper must stay retained"
    );
}

#[test]
fn retained_auto_loading_svg_nested_leaf_uses_typed_active_wrapper() {
    let (arena, outer, properties, generations) =
        crate::view::paint::nested_scroll_unready_media_fixture_for_test(
            crate::view::paint::NestedMediaLeafKind::Svg,
        );
    let roots = [outer];
    assert_eq!(properties.scrolls.len(), 2);
    let decision = select_retained_auto_authority(
        &arena,
        &roots,
        &properties,
        &generations,
        &UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
        true,
    );
    assert!(
        matches!(decision, AutoAuthorityDecision::NestedScrollScene { .. }),
        "a source transition with no exact SVG raster is a legal loading wrapper"
    );
}

#[test]
fn retained_auto_missing_and_inline_owned_text_nested_leafs_stay_whole_frame_legacy() {
    for kind in [
        crate::view::paint::NestedTextFallbackKind::MissingPrepared,
        crate::view::paint::NestedTextFallbackKind::InlineIfcOwned,
    ] {
        let (arena, outer, properties, generations) =
            crate::view::paint::nested_scroll_unready_text_fixture_for_test(kind);
        let roots = [outer];
        assert_eq!(properties.scrolls.len(), 2, "{kind:?} keeps exact topology");

        let viewport = Viewport::new();
        let graph = FrameGraph::new();
        let graph_before = graph.build_state_snapshot_for_test();
        let pool_before = viewport.retained_surface_transaction_shape_for_test();
        let AutoAuthorityDecision::Legacy { trace } = select_retained_auto_authority(
            &arena,
            &roots,
            &properties,
            &generations,
            &UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
            true,
        ) else {
            panic!("{kind:?} Text must remain whole-frame legacy")
        };
        assert!(matches!(
            trace.rejections.first(),
            Some(AutoAuthorityRejection::NestedScrollPlan { .. })
        ));
        assert_eq!(
            graph.build_state_snapshot_for_test(),
            graph_before,
            "{kind:?}"
        );
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            pool_before,
            "{kind:?}"
        );
        assert!(viewport.retained_property_scroll_scene_stage_is_available());
    }
}

#[test]
fn retained_auto_nested_scroll_preflight_failures_are_atomic() {
    let select = || {
        let (arena, roots, properties, generations) = prepared_exact_nested_scroll_scene();
        let decision = select_retained_auto_authority(
            &arena,
            &roots,
            &properties,
            &generations,
            &UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
            true,
        );
        let AutoAuthorityDecision::NestedScrollScene { prepared, .. } = decision else {
            panic!("exact nested fixture selects dedicated authority")
        };
        prepared
    };

    let mut stage_viewport = Viewport::new();
    let mut stage_graph = FrameGraph::new();
    let stage_graph_before = stage_graph.build_state_snapshot_for_test();
    let stage_pool_before = stage_viewport.retained_surface_transaction_shape_for_test();
    let (selection, outcome) = preflight_nested_scroll_selection(
        &mut stage_viewport,
        &mut stage_graph,
        UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
        [0.0; 4],
        None,
        RetainedTransformCanarySelection::NestedScrollScenePlanned(select()),
    );
    assert!(outcome.is_none());
    assert!(matches!(
        selection,
        RetainedTransformCanarySelection::NestedScrollScenePrepareRejected(
            crate::view::paint::RetainedPropertyScrollScenePrepareError::StageUnavailable
        )
    ));
    assert_eq!(
        stage_graph.build_state_snapshot_for_test(),
        stage_graph_before
    );
    assert_eq!(
        stage_viewport.retained_surface_transaction_shape_for_test(),
        stage_pool_before
    );

    let mut context_viewport = Viewport::new();
    let context_owner = context_viewport
        .begin_retained_surface_frame_stage()
        .unwrap();
    let mut context_graph = FrameGraph::new();
    let context_graph_before = context_graph.build_state_snapshot_for_test();
    let context_pool_before = context_viewport.retained_surface_transaction_shape_for_test();
    let mut bad_ctx = UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    bad_ctx.push_scissor_rect(Some([1, 2, 3, 4]));
    let (selection, outcome) = preflight_nested_scroll_selection(
        &mut context_viewport,
        &mut context_graph,
        bad_ctx,
        [0.0; 4],
        Some(context_owner),
        RetainedTransformCanarySelection::NestedScrollScenePlanned(select()),
    );
    assert!(outcome.is_none());
    assert!(matches!(
        selection,
        RetainedTransformCanarySelection::NestedScrollScenePrepareRejected(
            crate::view::paint::RetainedPropertyScrollScenePrepareError::ContextMismatch
        )
    ));
    assert_eq!(
        context_graph.build_state_snapshot_for_test(),
        context_graph_before
    );
    assert_eq!(
        context_viewport.retained_surface_transaction_shape_for_test(),
        context_pool_before
    );
    assert!(context_viewport.retained_surface_frame_stage_owner_is_active(context_owner));
    assert!(
        context_viewport
            .finish_retained_surface_transaction_for_frame(Some(context_owner), false,)
    );

    let mut collision_viewport = Viewport::new();
    let collision_owner = collision_viewport
        .begin_retained_surface_frame_stage()
        .unwrap();
    let collision_prepared = select();
    let (collision_key, collision_desc) = collision_prepared.leaf_target_for_test();
    let mut collision_graph = FrameGraph::new();
    let mut declaring_ctx =
        UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    let _ = declaring_ctx.allocate_persistent_target_with_desc(
        &mut collision_graph,
        collision_desc,
        collision_key,
    );
    let collision_graph_before = collision_graph.build_state_snapshot_for_test();
    let collision_pool_before =
        collision_viewport.retained_surface_transaction_shape_for_test();
    let (selection, outcome) = preflight_nested_scroll_selection(
        &mut collision_viewport,
        &mut collision_graph,
        UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
        [0.0; 4],
        Some(collision_owner),
        RetainedTransformCanarySelection::NestedScrollScenePlanned(collision_prepared),
    );
    assert!(outcome.is_none());
    assert!(matches!(
        selection,
        RetainedTransformCanarySelection::NestedScrollScenePrepareRejected(
            crate::view::paint::RetainedPropertyScrollScenePrepareError::PersistentKeyAlreadyDeclared(key)
        ) if key == collision_key
    ));
    assert_eq!(
        collision_graph.build_state_snapshot_for_test(),
        collision_graph_before
    );
    assert_eq!(
        collision_viewport.retained_surface_transaction_shape_for_test(),
        collision_pool_before
    );
    assert!(
        collision_viewport
            .finish_retained_surface_transaction_for_frame(Some(collision_owner), false,)
    );

    let (fallback, trace) = nested_scroll_prepare_rejection_dispatch(
        &crate::view::paint::RetainedPropertyScrollScenePrepareError::StageUnavailable,
    );
    assert!(fallback);
    assert!(trace.contains("nested-scroll-prepare-rejected=StageUnavailable"));
    assert_eq!(
        nested_scroll_prepare_rejection_fallback_stage(),
        PaintAuthorityFallbackStage::Prepare
    );
}

#[test]
fn retained_auto_exact_scroll_selects_scene_and_never_baked_host() {
    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let (arena, roots, properties, generations) = prepared_exact_scroll_scene();
    let decision =
        select_retained_auto_authority(&arena, &roots, &properties, &generations, &ctx, true);
    let trace = match decision {
        AutoAuthorityDecision::PropertyScrollScene { trace, .. } => trace,
        AutoAuthorityDecision::Legacy { trace } => panic!(
            "exact scroll topology rejected PropertyScene: {:?}",
            trace.rejections
        ),
        _ => panic!("exact scroll topology selected a non-scroll authority"),
    };
    assert!(matches!(
        trace.rejections.as_slice(),
        [AutoAuthorityRejection::PropertyScrollPlan { .. }]
    ));
    assert_eq!(properties.scrolls.len(), 1);
}
