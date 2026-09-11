use super::*;

struct RetainedAutoPropertyForestFixture {
    arena: NodeArena,
    roots: Vec<NodeKey>,
    properties: PropertyTrees,
    generations: PaintGenerationTracker,
}

fn styled_element(id: u64, x: f32, y: f32, width: f32, height: f32, color: Color) -> Element {
    let mut element = Element::new_with_id(id, x, y, width, height);
    let mut style = Style::new();
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    style.insert(PropertyId::BackgroundColor, ParsedValue::color_like(color));
    element.apply_style(style);
    element
}

fn effect_transform_fixture(
    stable_id_base: u64,
    neutral_wrapper: bool,
) -> RetainedAutoPropertyForestFixture {
    let mut arena = new_test_arena();
    let root = commit_element(
        &mut arena,
        Box::new(styled_element(
            stable_id_base + 1,
            0.0,
            0.0,
            120.0,
            90.0,
            Color::rgb(30, 50, 90),
        )),
    );
    let nested_parent = if neutral_wrapper {
        commit_child(
            &mut arena,
            root,
            Box::new(styled_element(
                stable_id_base + 2,
                4.0,
                3.0,
                80.0,
                58.0,
                Color::rgb(40, 70, 100),
            )),
        )
    } else {
        root
    };
    let transform = commit_child(
        &mut arena,
        nested_parent,
        Box::new(styled_element(
            stable_id_base + 3,
            8.0,
            7.0,
            44.0,
            30.0,
            Color::rgb(180, 70, 30),
        )),
    );
    commit_child(
        &mut arena,
        transform,
        Box::new(styled_element(
            stable_id_base + 4,
            2.0,
            2.0,
            12.0,
            9.0,
            Color::rgb(20, 160, 100),
        )),
    );
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    crate::view::test_support::get_element_mut::<Element>(&arena, root).set_opacity(0.5);
    crate::view::test_support::get_element_mut::<Element>(&arena, transform)
        .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
            3.0, 2.0, 0.0,
        ))));
    arena.refresh_subtree_dirty_cache(root);
    let roots = vec![root];
    let (properties, generations) = synced_paint_state(&arena, &roots);
    RetainedAutoPropertyForestFixture {
        arena,
        roots,
        properties,
        generations,
    }
}

fn selection_context(dpr: f32) -> UiBuildContext {
    UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, dpr)
}

fn selected_artifact(
    fixture: &RetainedAutoPropertyForestFixture,
    dpr: f32,
) -> (RecordedArtifactCandidate, AutoAuthorityTrace, usize) {
    selected_artifact_surface(
        "E -> T forest",
        &fixture.arena,
        &fixture.roots,
        &fixture.properties,
        &fixture.generations,
        &selection_context(dpr),
    )
}

fn build_selected(
    viewport: &mut Viewport,
    candidate: RecordedArtifactCandidate,
    dpr: f32,
) -> usize {
    emit_selected_artifact_surface("E -> T forest", viewport, candidate, selection_context(dpr))
}

#[test]
fn retained_auto_direct_and_neutral_effect_transform_select_artifact() {
    for neutral_wrapper in [false, true] {
        let stable_id_base = if neutral_wrapper {
            0xf4_2110
        } else {
            0xf4_2100
        };
        let fixture = effect_transform_fixture(stable_id_base, neutral_wrapper);
        let (_, trace, _) = selected_artifact(&fixture, 1.0);
        assert!(
            !trace.rejections.iter().any(|rejection| matches!(
                rejection,
                AutoAuthorityRejection::ArtifactPrepare { .. }
            )),
            "selected authority cannot reject itself: {trace:?}",
        );
    }
}

#[test]
fn retained_auto_dpr1_and_dpr2_stage_stable_artifact_residents_and_never_red() {
    for (dpr, neutral_wrapper) in [(1.0, false), (2.0, true)] {
        let stable_id_base = if neutral_wrapper {
            0xf4_2210
        } else {
            0xf4_2200
        };
        let fixture = effect_transform_fixture(stable_id_base, neutral_wrapper);
        let (telemetry_candidate, trace, surface_count) = selected_artifact(&fixture, dpr);
        let telemetry = telemetry_for_auto_decision(AutoAuthorityDecision::Artifact {
            candidate: telemetry_candidate,
            trace,
        });
        assert_eq!(telemetry.final_authority(), PaintAuthorityKind::Artifact);
        assert!(telemetry.fallback_boundary_nodes().is_empty());
        assert!(retained_auto_fallback_overlay_records(&telemetry, &fixture.roots).is_empty());

        let mut viewport = Viewport::new();
        let (cold, _, _) = selected_artifact(&fixture, dpr);
        assert_eq!(build_selected(&mut viewport, cold, dpr), surface_count);
        let (warm, _, _) = selected_artifact(&fixture, dpr);
        assert_eq!(build_selected(&mut viewport, warm, dpr), surface_count);

        viewport.scene.node_arena = fixture.arena;
        let capture =
            viewport.build_retained_auto_debug_capture(&telemetry, &fixture.roots, true, true);
        assert_eq!(
            capture.frame.selected_authority,
            crate::view::debug::DebugFramePaintAuthority::Artifact
        );
        assert_eq!(
            capture.frame.disposition,
            crate::view::debug::DebugFrameDisposition::Presented
        );
        assert!(capture.frame.fallback_stages.is_empty());
        assert_eq!(capture.frame.statistics.fallback_count, 0);
        assert!(capture.nodes.iter().all(|node| node.fallbacks.is_empty()));
    }
}

#[test]
fn property_forest_prepare_rejection_cannot_report_retained_success() {
    let fixture = effect_transform_fixture(0xf4_2300, true);
    // Named component-only oracle: this is the sole b2 test that bypasses the
    // production selector to retain coverage of the legacy planner rejection.
    let plan_context = crate::view::paint::TransformSurfacePlanContext::new([0.0; 2], None);
    let plan = crate::view::paint::plan_property_effect_scene_with_context(
        &fixture.arena,
        &fixture.roots,
        &fixture.properties,
        &fixture.generations,
        plan_context,
    )
    .expect("legacy property forest component fixture plans");
    let trace = AutoAuthorityTrace {
        artifact_prepare_rejected: false,
        capture_rejections: true,
        rejections: Vec::new(),
    };
    let viewport = Viewport::new();
    let mut graph = FrameGraph::new();
    let mut ctx = selection_context(1.0);
    let target = ctx.allocate_target(&mut graph);
    ctx.set_current_target(target);
    let graph_before = graph.build_state_snapshot_for_test();
    let error = crate::view::paint::prepare_property_boundary_forest_with_tamper_for_test(
        &viewport,
        &plan,
        &graph,
        &ctx,
        crate::view::paint::PropertyBoundaryForestPrepareTamper::OmittedAction,
    )
    .expect_err("omitted action must reject before graph mutation");
    assert_eq!(
        error,
        crate::view::paint::RetainedSurfacePrepareError::ActionSet
    );
    assert_eq!(graph.build_state_snapshot_for_test(), graph_before);
    assert_eq!(
        viewport.retained_surface_transaction_shape_for_test(),
        (0, None)
    );

    let selection = RetainedTransformCanarySelection::PropertyScenePrepareRejected(error);
    let mut telemetry = PaintAuthorityTelemetry::from_selection(
        ViewportPaintRendererMode::RetainedAuto,
        &selection,
        Some((AutoAuthorityKind::PropertyScene, trace)),
    );
    telemetry.note_legacy_fallback(PaintAuthorityFallbackStage::Prepare);
    assert_eq!(telemetry.final_authority(), PaintAuthorityKind::Legacy);

    let mut viewport = Viewport::new();
    viewport.scene.node_arena = fixture.arena;
    let capture =
        viewport.build_retained_auto_debug_capture(&telemetry, &fixture.roots, true, true);
    assert_eq!(
        capture.frame.selected_authority,
        crate::view::debug::DebugFramePaintAuthority::Legacy
    );
    assert_eq!(
        capture.frame.disposition,
        crate::view::debug::DebugFrameDisposition::FellBackToLegacy
    );
    assert_eq!(capture.frame.statistics.retained_surfaces, 0);
    assert_eq!(capture.frame.statistics.fallback_count, 1);
    assert_eq!(capture.frame.fallback_stages.len(), 1);
}
