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
fn retained_auto_nested_scroll_hard_cutover_selects_artifact_and_emits_two_surfaces() {
    let ctx = UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    let (arena, roots, properties, generations) = prepared_exact_nested_scroll_scene();
    let mut viewport = Viewport::new();
    let mut graph = FrameGraph::new();
    let emission = crate::view::viewport::emit_retained_auto_artifact_surface_for_test(
        &mut viewport,
        &arena,
        &roots,
        &properties,
        &generations,
        &mut graph,
        &ctx,
    )
    .expect("selected nested scroll Artifact must emit");
    assert_eq!(emission.surface_count, 2);
    assert_eq!(emission.aggregate_texture_bytes, 1_440_000);
    assert_eq!(
        emission.actions,
        [
            crate::view::paint::RetainedSurfaceCompileAction::Reraster,
            crate::view::paint::RetainedSurfaceCompileAction::Reraster,
        ]
    );
    assert_eq!(graph.declared_persistent_texture_keys().count(), 4);
    assert_eq!(
        graph
            .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
            .len(),
        2,
        "two cold Artifact surfaces must each clear once"
    );
    assert_eq!(
        graph
            .test_graphics_passes::<crate::view::render_pass::composite_layer_pass::CompositeLayerPass>()
            .len(),
        2,
        "inner and outer ScrollContent surfaces each composite once"
    );
    assert!(viewport.finish_retained_surface_transaction_for_frame(
        Some(emission.frame_owner),
        true
    ));

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
        PaintAuthorityKind::Artifact
    );
    assert!(telemetry.fallback_boundary_nodes().is_empty());
    assert!(retained_auto_fallback_overlay_records(&telemetry, &roots).is_empty());
    let mut viewport = Viewport::new();
    viewport.scene.node_arena = arena;
    let capture = viewport.build_retained_auto_debug_capture(&telemetry, &roots, true, true);
    assert_eq!(
        capture.frame.selected_authority,
        crate::view::debug::DebugFramePaintAuthority::Artifact
    );
    assert_eq!(
        capture.frame.disposition,
        crate::view::debug::DebugFrameDisposition::Presented
    );
}

#[test]
fn nested_scroll_census_capture_preserves_artifact_authority() {
    let ctx = UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
    let (arena, roots, properties, generations) = prepared_exact_nested_scroll_scene();
    let captured =
        select_retained_auto_authority(&arena, &roots, &properties, &generations, &ctx, true);
    let uncaptured =
        select_retained_auto_authority(&arena, &roots, &properties, &generations, &ctx, false);
    let AutoAuthorityDecision::Artifact { trace, .. } = captured else {
        panic!("census capture must preserve nested Artifact authority")
    };
    let AutoAuthorityDecision::Artifact {
        trace: uncaptured_trace,
        ..
    } = uncaptured else {
        panic!("capture-off must preserve nested Artifact authority")
    };
    assert!(trace.rejections.is_empty());
    assert!(uncaptured_trace.rejections.is_empty());
}

#[test]
fn retained_auto_depth_three_linear_chain_selects_artifact_before_native_forest() {
    let (arena, roots, properties, generations) = prepared_exact_depth_three_nested_scroll_scene();
    let decision = select_retained_auto_authority(
        &arena,
        &roots,
        &properties,
        &generations,
        &UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
        true,
    );
    let AutoAuthorityDecision::Artifact { trace, .. } = decision else {
        panic!(
            "S->S->S->leaf must be claimed by Artifact before forest selection: {:?}",
            auto_authority_trace(&decision).rejections
        )
    };
    assert!(trace.rejections.is_empty());
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
        assert_eq!(trace.rejections.len(), 1, "must not retry a compatibility planner");
        assert!(matches!(&trace.rejections[0], AutoAuthorityRejection::Artifact { eligibility }
            if !eligibility.eligible && !eligibility.reasons.is_empty()), "{trace:?}");
    }
}

#[test]
fn retained_auto_missing_text_stays_legacy_while_inline_ifc_owned_text_selects_artifact() {
    for kind in [crate::view::paint::NestedTextFallbackKind::MissingPrepared] {
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
        assert_eq!(trace.rejections.len(), 1, "must not retry a compatibility planner");
        assert!(matches!(&trace.rejections[0], AutoAuthorityRejection::Artifact { eligibility }
            if !eligibility.eligible && !eligibility.reasons.is_empty()), "{trace:?}");
    }

    let kind = crate::view::paint::NestedTextFallbackKind::InlineIfcOwned;
    let (arena, outer, properties, generations) =
        crate::view::paint::nested_scroll_unready_text_fixture_for_test(kind);
    let roots = [outer];
    let decision = select_retained_auto_authority(
        &arena,
        &roots,
        &properties,
        &generations,
        &UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
        true,
    );
    assert!(
        matches!(decision, AutoAuthorityDecision::Artifact { .. }),
        "InlineIfcOwned Text must use the generic Artifact executor: {:?}",
        auto_authority_trace(&decision).rejections
    );
}

#[test]
fn retained_auto_exact_scroll_selects_artifact_and_never_baked_host() {
    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let (arena, roots, properties, generations) = prepared_exact_scroll_scene();
    let decision =
        select_retained_auto_authority(&arena, &roots, &properties, &generations, &ctx, true);
    let trace = match decision {
        AutoAuthorityDecision::Artifact { trace, .. } => trace,
        AutoAuthorityDecision::Legacy { trace } => panic!(
            "exact scroll topology rejected Artifact: {:?}",
            trace.rejections
        ),
        _ => panic!("exact scroll topology selected a non-scroll authority"),
    };
    assert!(trace.rejections.is_empty());
    assert_eq!(properties.scrolls.len(), 1);
}
