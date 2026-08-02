use super::*;

#[test]
fn nested_scroll_success_telemetry_names_segment_topology_depth_and_zero_residency() {
    let (phase, topology, residency) =
        super::super::property_boundary_dag_success_telemetry_grammar(Some(3), 0, 0);
    assert_eq!(phase, "nested-scroll-segment");
    assert_eq!(topology, " topology=linear-scroll-chain chain-depth=3");
    assert_eq!(residency, " residency=zero");

    let (phase, topology, residency) =
        super::super::property_boundary_dag_success_telemetry_grammar(None, 1, 0);
    assert_eq!(phase, "property-boundary-dag");
    assert!(topology.is_empty());
    assert!(residency.is_empty());
}

#[test]
fn retained_auto_nested_scroll_hard_cutover_selects_dag_and_emits_without_parent_target() {
    let ctx = UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    let (arena, roots, properties, generations) = prepared_exact_nested_scroll_scene();
    let outer = roots[0];
    let inner = arena.children_of(outer)[0];
    let decision =
        select_retained_auto_authority(&arena, &roots, &properties, &generations, &ctx, true);
    let AutoAuthorityDecision::PropertyBoundaryDagScene { scene, trace } = decision else {
        panic!("exact S->S->leaf must select the property-boundary DAG nested segment")
    };
    assert_eq!(scene.nested_scroll_chain_depth(), Some(2));
    assert!(scene.is_canonical());
    assert!(trace.rejections.iter().all(|rejection| !matches!(
        rejection,
        AutoAuthorityRejection::NativeScrollForestPlan { .. }
    )));
    let rejection_owner_codes = trace
        .rejections
        .iter()
        .flat_map(|rejection| {
            super::super::selection_rejection_debug_records(
                &super::super::PaintAuthoritySelectionRejection::Auto(rejection.clone()),
            )
        })
        .map(|record| {
            (
                record.owner,
                crate::view::debug::census::fallback_detail_label(&record.detail),
            )
        })
        .collect::<Vec<_>>();
    for (owner, code) in [
        (inner, "property-boundary-dag:scroll-boundary"),
        (inner, "property-boundary-dag:invalid-scroll-host"),
        (
            inner,
            "property-boundary-dag:ancestor-boundary-not-consumed",
        ),
        (
            inner,
            "property-boundary-dag:receiver-ancestor-boundary-not-consumed",
        ),
        (
            inner,
            "property-boundary-dag:receiver-state-cursor-mismatch",
        ),
        (
            outer,
            "property-boundary-dag:root-boundary-schedule-unsupported",
        ),
    ] {
        assert!(
            !rejection_owner_codes.contains(&(Some(owner), code.to_string())),
            "M0 target owner/code must disappear after nested DAG cutover: owner={owner:?} code={code} records={rejection_owner_codes:?}"
        );
    }

    let mut viewport = Viewport::new();
    let owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut graph = FrameGraph::new();
    let prepared = crate::view::paint::prepare_property_boundary_dag_scene_from_pool(
        &mut viewport,
        scene,
        &mut graph,
        ctx,
        [0.0, 0.0, 0.0, 1.0],
        owner,
    )
    .expect("selected nested segment prepares through the generic DAG facade");
    let outcome = crate::view::paint::emit_prepared_property_boundary_dag_scene(prepared);
    let (state, build_trace) = outcome.into_parts();
    assert_eq!(state.opaque_rect_order(), 2);
    assert_eq!(build_trace.root_count, 1);
    assert_eq!(build_trace.generic_surface_count, 0);
    assert_eq!(build_trace.scroll_group_count, 1);
    assert_eq!(build_trace.reraster_count, 1);
    assert_eq!(build_trace.reuse_count, 0);
    assert_eq!(graph.declared_persistent_texture_keys().count(), 2);
    assert_eq!(
        graph
            .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
            .len(),
        2,
        "direct-to-frame segment owns only the frame and cold leaf clears"
    );
    assert_eq!(
        graph
            .test_graphics_passes::<
                crate::view::render_pass::texture_composite_pass::TextureCompositePass,
            >()
            .len(),
        1,
        "persistent leaf composites directly to the frame"
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
        telemetry.final_authority(),
        PaintAuthorityKind::PropertyScene
    );
    assert!(telemetry.fallback_boundary_nodes().is_empty());
    assert!(retained_auto_fallback_overlay_records(&telemetry, &roots).is_empty());
    let mut viewport = Viewport::new();
    viewport.scene.node_arena = arena;
    let capture = viewport.build_retained_auto_debug_capture(&telemetry, &roots, true, true);
    assert_eq!(
        capture.frame.selected_authority,
        crate::view::debug::DebugFramePaintAuthority::PropertyScene
    );
    assert_eq!(
        capture.frame.disposition,
        crate::view::debug::DebugFrameDisposition::Presented
    );
}

#[test]
fn nested_scroll_m6_census_capture_preserves_authority_residency_and_actions() {
    let ctx = UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    let (arena, roots, properties, generations) = prepared_exact_nested_scroll_scene();
    let captured =
        select_retained_auto_authority(&arena, &roots, &properties, &generations, &ctx, true);
    let uncaptured =
        select_retained_auto_authority(&arena, &roots, &properties, &generations, &ctx, false);
    let AutoAuthorityDecision::PropertyBoundaryDagScene {
        scene: captured_scene,
        ..
    } = captured
    else {
        panic!("census capture must preserve nested DAG authority")
    };
    let AutoAuthorityDecision::PropertyBoundaryDagScene {
        scene: uncaptured_scene,
        trace: uncaptured_trace,
    } = uncaptured
    else {
        panic!("capture-off must preserve nested DAG authority")
    };
    assert!(uncaptured_trace.rejections.is_empty());
    assert_eq!(captured_scene.nested_scroll_chain_depth(), Some(2));
    assert_eq!(uncaptured_scene.nested_scroll_chain_depth(), Some(2));
    assert_eq!(
        captured_scene.nested_scroll_persistent_leaf_target_for_test(),
        uncaptured_scene.nested_scroll_persistent_leaf_target_for_test(),
        "observational capture cannot change nested raster identity or descriptors"
    );

    let build = |scene| {
        let mut viewport = Viewport::new();
        let owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut graph = FrameGraph::new();
        let prepared = crate::view::paint::prepare_property_boundary_dag_scene_from_pool(
            &mut viewport,
            scene,
            &mut graph,
            UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
            [0.0, 0.0, 0.0, 1.0],
            owner,
        )
        .expect("capture-neutral nested scene prepares");
        let outcome = crate::view::paint::emit_prepared_property_boundary_dag_scene(prepared);
        let (state, trace) = outcome.into_parts();
        assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));
        (
            state.opaque_rect_order(),
            trace.reraster_count,
            trace.reuse_count,
            graph.test_graphics_passes::<crate::view::frame_graph::ClearPass>().len(),
            graph
                .test_graphics_passes::<
                    crate::view::render_pass::texture_composite_pass::TextureCompositePass,
                >()
                .len(),
            graph.declared_persistent_texture_keys().count(),
        )
    };
    assert_eq!(build(captured_scene), build(uncaptured_scene));
}

#[test]
fn retained_auto_depth_three_linear_chain_selects_dag_before_native_forest() {
    let (arena, roots, properties, generations) = prepared_exact_depth_three_nested_scroll_scene();
    let decision = select_retained_auto_authority(
        &arena,
        &roots,
        &properties,
        &generations,
        &UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
        true,
    );
    let AutoAuthorityDecision::PropertyBoundaryDagScene { scene, trace } = decision else {
        panic!(
            "S->S->S->leaf must be claimed by the DAG before forest selection: {:?}",
            auto_authority_trace(&decision).rejections
        )
    };
    assert_eq!(scene.nested_scroll_chain_depth(), Some(3));
    assert!(trace.rejections.iter().all(|rejection| !matches!(
        rejection,
        AutoAuthorityRejection::NativeScrollForestPlan { .. }
    )));
}

#[test]
fn retained_auto_unready_nested_media_does_not_retry_the_retired_executor() {
    for kind in [
        crate::view::paint::NestedMediaLeafKind::Image,
        crate::view::paint::NestedMediaLeafKind::Svg,
    ] {
        let (arena, outer, properties, generations) =
            crate::view::paint::nested_scroll_unready_media_fixture_for_test(kind);
        let roots = [outer];
        assert!(
            !super::super::native_scroll_forest_topology_is_branching_or_multi_root(
                &roots,
                &properties,
            )
        );
        let decision = select_retained_auto_authority(
            &arena,
            &roots,
            &properties,
            &generations,
            &UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
            true,
        );
        let AutoAuthorityDecision::Legacy { trace } = decision else {
            panic!(
                "unready {kind:?} must fail closed after the hard cutover: {:?}",
                auto_authority_trace(&decision).rejections
            )
        };
        assert!(trace.rejections.iter().any(|rejection| matches!(
            rejection,
            AutoAuthorityRejection::PropertyBoundaryDagPlan { .. }
        )));
    }
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
        assert!(
            !super::super::native_scroll_forest_topology_is_branching_or_multi_root(
                &roots,
                &properties,
            )
        );
        let decision = select_retained_auto_authority(
            &arena,
            &roots,
            &properties,
            &generations,
            &UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
            true,
        );
        let AutoAuthorityDecision::Legacy { trace } = decision else {
            panic!(
                "{kind:?} Text must remain whole-frame legacy: {:?}",
                auto_authority_trace(&decision).rejections
            )
        };
        assert!(trace.rejections.iter().any(|rejection| matches!(
            rejection,
            AutoAuthorityRejection::PropertyBoundaryDagPlan { .. }
        )));
    }
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
        [AutoAuthorityRejection::FrameRootScrollPlan { .. }]
    ));
    assert_eq!(properties.scrolls.len(), 1);
}
