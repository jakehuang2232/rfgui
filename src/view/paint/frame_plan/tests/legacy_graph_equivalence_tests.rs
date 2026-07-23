use super::*;

#[test]
fn forced_rect_graph_is_strictly_identical_to_legacy_graph() {
    let (mut legacy_arena, legacy_root, _, _) = exact_transform_fixture();
    let mut legacy_graph = FrameGraph::new();
    let (legacy_ctx, legacy_parent) =
        parent_context_with_clear(&mut legacy_graph, 160, 120, 1.0);
    legacy_arena
        .with_element_taken(legacy_root, |element, arena| {
            element.build(&mut legacy_graph, arena, legacy_ctx)
        })
        .expect("legacy transformed rect build");
    legacy_graph
        .add_texture_sink(
            &legacy_parent,
            crate::view::frame_graph::ExternalSinkKind::DebugCapture,
        )
        .expect("legacy parent sink");

    let (forced_arena, forced_root, properties, generations) = exact_transform_fixture();
    let forced_plan = plan_single_root_transform_surface(
        &forced_arena,
        &[forced_root],
        &properties,
        &generations,
    )
    .expect("forced transformed rect plan");
    let mut forced_graph = FrameGraph::new();
    let (forced_ctx, forced_parent) =
        parent_context_with_clear(&mut forced_graph, 160, 120, 1.0);
    let mut viewport = Viewport::new();
    super::super::super::execute_forced_transform_surface_for_test(
        &mut viewport,
        &forced_plan,
        &mut forced_graph,
        forced_ctx,
    )
    .expect("forced transformed rect build");
    forced_graph
        .add_texture_sink(
            &forced_parent,
            crate::view::frame_graph::ExternalSinkKind::DebugCapture,
        )
        .expect("forced parent sink");

    let legacy = legacy_graph
        .test_compile_snapshot()
        .expect("strict legacy graph snapshot");
    let forced = forced_graph
        .test_compile_snapshot()
        .expect("strict forced graph snapshot");
    assert_eq!(forced, legacy);
}

#[test]
fn forced_rect_nonzero_context_graph_is_strictly_identical_to_legacy_graph() {
    let frozen = TransformSurfacePlanContext::new([0.25, -0.25], Some([3, 4, 50, 60]));
    let (mut legacy_arena, legacy_root, _, _) = exact_transform_fixture();
    let mut legacy_graph = FrameGraph::new();
    let (mut legacy_ctx, legacy_parent) =
        parent_context_with_clear(&mut legacy_graph, 160, 120, 2.0);
    legacy_ctx.translate_paint_offset(0.25, -0.25);
    legacy_ctx.push_scissor_rect(Some([3, 4, 50, 60]));
    legacy_arena
        .with_element_taken(legacy_root, |element, arena| {
            element.build(&mut legacy_graph, arena, legacy_ctx)
        })
        .expect("legacy nonzero-context transformed rect build");
    legacy_graph
        .add_texture_sink(
            &legacy_parent,
            crate::view::frame_graph::ExternalSinkKind::DebugCapture,
        )
        .expect("legacy nonzero-context parent sink");

    let (forced_arena, forced_root, properties, generations) = exact_transform_fixture();
    let forced_plan = plan_single_root_transform_surface_with_context(
        &forced_arena,
        &[forced_root],
        &properties,
        &generations,
        frozen,
    )
    .expect("forced nonzero-context transformed rect plan");
    let mut forced_graph = FrameGraph::new();
    let (mut forced_ctx, forced_parent) =
        parent_context_with_clear(&mut forced_graph, 160, 120, 2.0);
    forced_ctx.translate_paint_offset(0.25, -0.25);
    forced_ctx.push_scissor_rect(Some([3, 4, 50, 60]));
    let mut viewport = Viewport::new();
    super::super::super::execute_forced_transform_surface_for_test(
        &mut viewport,
        &forced_plan,
        &mut forced_graph,
        forced_ctx,
    )
    .expect("forced nonzero-context transformed rect build");
    forced_graph
        .add_texture_sink(
            &forced_parent,
            crate::view::frame_graph::ExternalSinkKind::DebugCapture,
        )
        .expect("forced nonzero-context parent sink");

    assert_eq!(
        forced_graph
            .test_compile_snapshot()
            .expect("strict forced nonzero-context snapshot"),
        legacy_graph
            .test_compile_snapshot()
            .expect("strict legacy nonzero-context snapshot")
    );
}

#[test]
fn forced_zero_blur_shadow_graph_is_strictly_identical_to_legacy_graph() {
    let mut root = Element::new_with_id(0xc2_b100, 20.0, 20.0, 30.0, 18.0);
    let mut style = Style::new();
    style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::rgb(40, 80, 120)),
    );
    style.set_box_shadow(vec![
        BoxShadow::new().offset_x(3.0).offset_y(4.0).spread(2.0),
    ]);
    style.set_transform(Transform::new([Rotate::z(Angle::deg(6.0))]));
    root.apply_style(style);

    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(root));
    measure_and_place(
        &mut arena,
        root,
        LayoutConstraints {
            max_width: 160.0,
            max_height: 120.0,
            viewport_width: 160.0,
            viewport_height: 120.0,
            percent_base_width: Some(160.0),
            percent_base_height: Some(120.0),
        },
        LayoutPlacement {
            parent_x: 20.0,
            parent_y: 20.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 160.0,
            available_height: 120.0,
            viewport_width: 160.0,
            viewport_height: 120.0,
            percent_base_width: Some(160.0),
            percent_base_height: Some(120.0),
        },
    );
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[root]);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[root], &properties);
    let plan = plan_single_root_transform_surface(&arena, &[root], &properties, &generations)
        .expect("zero-blur outer shadow surface plan");
    let [PaintPlanStep::RetainedSurface(surface)] = plan.steps.as_slice() else {
        panic!("one shadow surface")
    };
    assert!(
        only_span(surface)
            .artifact
            .ops
            .iter()
            .any(|op| matches!(op, PaintOp::PreparedShadow(_)))
    );

    let mut legacy_graph = FrameGraph::new();
    let (legacy_ctx, legacy_parent) =
        parent_context_with_clear(&mut legacy_graph, 160, 120, 1.0);
    arena
        .with_element_taken(root, |element, arena| {
            element.build(&mut legacy_graph, arena, legacy_ctx)
        })
        .expect("legacy zero-blur shadow build");
    legacy_graph
        .add_texture_sink(
            &legacy_parent,
            crate::view::frame_graph::ExternalSinkKind::DebugCapture,
        )
        .expect("legacy shadow sink");

    let mut forced_graph = FrameGraph::new();
    let (forced_ctx, forced_parent) =
        parent_context_with_clear(&mut forced_graph, 160, 120, 1.0);
    let mut viewport = Viewport::new();
    super::super::super::execute_forced_transform_surface_for_test(
        &mut viewport,
        &plan,
        &mut forced_graph,
        forced_ctx,
    )
    .expect("forced zero-blur shadow build");
    forced_graph
        .add_texture_sink(
            &forced_parent,
            crate::view::frame_graph::ExternalSinkKind::DebugCapture,
        )
        .expect("forced shadow sink");

    assert_eq!(
        forced_graph
            .test_compile_snapshot()
            .expect("strict forced shadow snapshot"),
        legacy_graph
            .test_compile_snapshot()
            .expect("strict legacy shadow snapshot")
    );
}

#[test]
fn ordinary_recorder_and_compiler_still_reject_the_same_transform_artifact() {
    let (arena, root, properties, generations) = exact_transform_fixture();
    let outcome = super::super::super::record_frame_artifact(
        &arena,
        &[root],
        &properties,
        &generations,
        super::super::super::RendererMode::Auto,
    )
    .expect("auto recorder must return a whole-frame fallback");
    let super::super::super::FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility) =
        outcome
    else {
        panic!("ordinary recorder must not acquire transform surface authority")
    };
    assert!(eligibility.reasons.contains(
        &super::super::super::FrameArtifactFallbackReason::LegacyBoundary(
            super::super::super::LegacyPaintReason::Transform,
        )
    ));

    let plan = plan_single_root_transform_surface(&arena, &[root], &properties, &generations)
        .expect("surface-only planner owns the positive path");
    let PaintPlanStep::RetainedSurface(surface) = &plan.steps[0] else {
        panic!("one retained surface")
    };
    let mut graph = FrameGraph::new();
    let before = graph.build_state_snapshot_for_test();
    let result = super::super::super::try_compile_artifact(
        &only_span(surface).artifact,
        &mut graph,
        UiBuildContext::new(160, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0),
    );
    let error = match result {
        Ok(_) => panic!("general compiler must reject transform chunks"),
        Err(error) => error,
    };
    assert_eq!(
        error.kind(),
        super::super::super::ArtifactCompileErrorKind::InvalidStore
    );
    assert_eq!(graph.build_state_snapshot_for_test(), before);
}

#[test]
fn ambient_or_wrong_owner_transform_witness_cannot_escape_surface_policy() {
    let (arena, root, properties, generations) = exact_transform_fixture();
    let root_stable_id = arena.get(root).expect("root").element.stable_id();
    let ambient = super::super::super::PaintRecordingContext {
        recording_owner: Some(root),
        recording_owner_stable_id: Some(root_stable_id),
        transform_surface: Some(PaintTransformSurfaceWitness::canonical_root(root)),
        ..Default::default()
    };
    let manifest = super::super::super::coverage_manifest::record_coverage_manifest_with_context(
        &arena,
        &[root],
        false,
        true,
        super::super::super::CoverageRecordingMode::MetadataOnly,
        &properties,
        &generations,
        ambient,
        None,
        &Default::default(),
    );
    assert!(matches!(
        manifest.items.as_slice(),
        [super::super::super::PaintCoverageItem::LegacyBoundary {
            reason: super::super::super::LegacyPaintReason::Transform,
            ..
        }]
    ));

    let child = arena
        .get(root)
        .expect("root")
        .element
        .children()
        .first()
        .copied()
        .expect("child");
    let wrong_owner = super::super::super::PaintRecordingContext {
        recording_owner: Some(root),
        recording_owner_stable_id: Some(root_stable_id),
        transform_surface: Some(
            PaintTransformSurfaceWitness::canonical_root(root).for_target(child),
        ),
        ..Default::default()
    };
    assert!(!wrong_owner.authorizes_transform_surface_root(root_stable_id));
    assert!(!wrong_owner.authorizes_transform_surface_owner(Some(TransformNodeId(root))));
}
