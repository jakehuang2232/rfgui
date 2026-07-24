use rustc_hash::FxHashMap;

use super::*;
use crate::view::Viewport;

struct PropertyForestFixture {
    arena: NodeArena,
    root: NodeKey,
    nested: NodeKey,
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

fn property_forest_fixture(
    stable_id_base: u64,
    effect_parent: bool,
    neutral_wrapper: bool,
) -> PropertyForestFixture {
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
    let nested = commit_child(
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
        nested,
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
    if effect_parent {
        crate::view::test_support::get_element_mut::<Element>(&arena, root).set_opacity(0.5);
        crate::view::test_support::get_element_mut::<Element>(&arena, nested)
            .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
                3.0, 2.0, 0.0,
            ))));
    } else {
        crate::view::test_support::get_element_mut::<Element>(&arena, root)
            .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
                3.0, 2.0, 0.0,
            ))));
        crate::view::test_support::get_element_mut::<Element>(&arena, nested).set_opacity(0.5);
    }
    arena.refresh_subtree_dirty_cache(root);
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[root]);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[root], &properties);
    PropertyForestFixture {
        arena,
        root,
        nested,
        properties,
        generations,
    }
}

fn plan(fixture: &PropertyForestFixture) -> FramePaintPlan {
    plan_property_effect_scene_with_context(
        &fixture.arena,
        &[fixture.root],
        &fixture.properties,
        &fixture.generations,
        TransformSurfacePlanContext::new([0.0, 0.0], None),
    )
    .expect("canonical property boundary forest")
}

fn parent_context(graph: &mut FrameGraph, dpr: f32) -> UiBuildContext {
    let mut ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, dpr);
    let parent = ctx.allocate_target(graph);
    ctx.set_current_target(parent);
    ctx
}

fn build(
    viewport: &mut Viewport,
    plan: &FramePaintPlan,
    dpr: f32,
) -> (FrameGraph, RetainedPropertySceneBuildTrace) {
    let mut graph = FrameGraph::new();
    let ctx = parent_context(&mut graph, dpr);
    let outcome =
        build_retained_property_scene_with_forced_pool_for_test(viewport, plan, &mut graph, ctx)
            .expect("property boundary forest preflights and emits");
    let (_, trace) = outcome.into_parts();
    (graph, trace)
}

fn actions(
    trace: &RetainedPropertySceneBuildTrace,
) -> FxHashMap<NodeKey, RetainedSurfaceCompileAction> {
    trace
        .surfaces
        .iter()
        .map(|surface| (surface.boundary_root, surface.action))
        .collect()
}

#[test]
fn direct_effect_transform_dpr1_cold_then_warm_reuses_both_residents() {
    let fixture = property_forest_fixture(0xf4_1100, true, false);
    let plan = plan(&fixture);
    let mut viewport = Viewport::new();
    let (_, cold) = build(&mut viewport, &plan, 1.0);
    assert_eq!((cold.reraster_count, cold.reuse_count), (2, 0));
    viewport.finish_retained_surface_transaction(true);

    let (_, warm) = build(&mut viewport, &plan, 1.0);
    assert_eq!((warm.reraster_count, warm.reuse_count), (0, 2));
    viewport.finish_retained_surface_transaction(false);
}

#[test]
fn neutral_effect_transform_dpr2_descriptors_and_warm_reuse_are_exact() {
    let fixture = property_forest_fixture(0xf4_1200, true, true);
    let plan = plan(&fixture);
    let viewport = Viewport::new();
    let mut dpr1_graph = FrameGraph::new();
    let dpr1_ctx = parent_context(&mut dpr1_graph, 1.0);
    let dpr1 =
        prepare_retained_property_scene_stamps_for_test(&viewport, &plan, &dpr1_graph, &dpr1_ctx)
            .expect("DPR1 descriptors");
    let mut dpr2_graph = FrameGraph::new();
    let dpr2_ctx = parent_context(&mut dpr2_graph, 2.0);
    let dpr2 =
        prepare_retained_property_scene_stamps_for_test(&viewport, &plan, &dpr2_graph, &dpr2_ctx)
            .expect("DPR2 descriptors");
    for (one, two) in dpr1.iter().zip(&dpr2) {
        assert_eq!(two.target.color.width(), one.target.color.width() * 2);
        assert_eq!(two.target.color.height(), one.target.color.height() * 2);
        assert_eq!(two.target.source_bounds_bits, one.target.source_bounds_bits);
    }

    let mut viewport = Viewport::new();
    let (_, cold) = build(&mut viewport, &plan, 2.0);
    assert_eq!((cold.reraster_count, cold.reuse_count), (2, 0));
    viewport.finish_retained_surface_transaction(true);
    let (_, warm) = build(&mut viewport, &plan, 2.0);
    assert_eq!((warm.reraster_count, warm.reuse_count), (0, 2));
    viewport.finish_retained_surface_transaction(false);
}

#[test]
fn alternating_child_composites_into_parent_before_top_level_composite() {
    let effect_transform = property_forest_fixture(0xf4_1300, true, true);
    let mut viewport = Viewport::new();
    let (graph, _) = build(&mut viewport, &plan(&effect_transform), 1.0);
    let transforms = graph.test_graphics_passes::<crate::view::render_pass::TextureCompositePass>();
    let effects = graph
        .test_graphics_passes::<crate::view::render_pass::composite_layer_pass::CompositeLayerPass>(
        );
    assert_eq!((transforms.len(), effects.len()), (1, 1));
    assert_eq!(
        transforms[0].test_snapshot().output_target,
        effects[0].test_snapshot().layer_handle,
        "T child must composite into the E resident before E reaches the frame",
    );
    assert_eq!(effects[0].test_snapshot().opacity_bits, 0.5_f32.to_bits());
    viewport.finish_retained_surface_transaction(false);

    let transform_effect = property_forest_fixture(0xf4_1400, false, false);
    let mut viewport = Viewport::new();
    let (graph, _) = build(&mut viewport, &plan(&transform_effect), 1.0);
    let transforms = graph.test_graphics_passes::<crate::view::render_pass::TextureCompositePass>();
    let effects = graph
        .test_graphics_passes::<crate::view::render_pass::composite_layer_pass::CompositeLayerPass>(
        );
    assert_eq!((transforms.len(), effects.len()), (1, 1));
    assert_eq!(
        effects[0].test_snapshot().output_target,
        transforms[0].test_snapshot().source_handle,
        "existing T -> E path still composites E into T before T reaches the frame",
    );
    viewport.finish_retained_surface_transaction(false);
}

#[test]
fn top_level_effect_opacity_is_composite_only_and_reuses_both_residents() {
    let mut fixture = property_forest_fixture(0xf4_1500, true, false);
    let baseline = plan(&fixture);
    let mut viewport = Viewport::new();
    let mut stamp_graph = FrameGraph::new();
    let stamp_ctx = parent_context(&mut stamp_graph, 1.0);
    let baseline_stamps = prepare_retained_property_scene_stamps_for_test(
        &viewport,
        &baseline,
        &stamp_graph,
        &stamp_ctx,
    )
    .unwrap();
    build(&mut viewport, &baseline, 1.0);
    viewport.finish_retained_surface_transaction(true);

    crate::view::test_support::get_element_mut::<Element>(&fixture.arena, fixture.root)
        .set_opacity(0.25);
    fixture.properties.sync(&fixture.arena, &[fixture.root]);
    fixture
        .generations
        .sync(&fixture.arena, &[fixture.root], &fixture.properties);
    let changed = plan(&fixture);
    let mut changed_stamp_graph = FrameGraph::new();
    let changed_stamp_ctx = parent_context(&mut changed_stamp_graph, 1.0);
    let changed_stamps = prepare_retained_property_scene_stamps_for_test(
        &viewport,
        &changed,
        &changed_stamp_graph,
        &changed_stamp_ctx,
    )
    .unwrap();
    assert_eq!(changed_stamps, baseline_stamps);
    let (graph, trace) = build(&mut viewport, &changed, 1.0);
    assert_eq!((trace.reraster_count, trace.reuse_count), (0, 2));
    let effects = graph
        .test_graphics_passes::<crate::view::render_pass::composite_layer_pass::CompositeLayerPass>(
        );
    assert_eq!(effects.len(), 1);
    assert_eq!(effects[0].test_snapshot().opacity_bits, 0.25_f32.to_bits());
    viewport.finish_retained_surface_transaction(false);
}

#[test]
fn nested_transform_matrix_reuses_child_and_rerasterizes_effect_parent() {
    let mut fixture = property_forest_fixture(0xf4_1600, true, true);
    let baseline = plan(&fixture);
    let mut viewport = Viewport::new();
    let mut stamp_graph = FrameGraph::new();
    let stamp_ctx = parent_context(&mut stamp_graph, 1.0);
    let baseline_stamps = prepare_retained_property_scene_stamps_for_test(
        &viewport,
        &baseline,
        &stamp_graph,
        &stamp_ctx,
    )
    .unwrap();
    build(&mut viewport, &baseline, 1.0);
    viewport.finish_retained_surface_transaction(true);

    crate::view::test_support::get_element_mut::<Element>(&fixture.arena, fixture.nested)
        .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
            9.0, 4.0, 0.0,
        ))));
    fixture.properties.sync(&fixture.arena, &[fixture.root]);
    fixture
        .generations
        .sync(&fixture.arena, &[fixture.root], &fixture.properties);
    let changed = plan(&fixture);
    let mut changed_stamp_graph = FrameGraph::new();
    let changed_stamp_ctx = parent_context(&mut changed_stamp_graph, 1.0);
    let changed_stamps = prepare_retained_property_scene_stamps_for_test(
        &viewport,
        &changed,
        &changed_stamp_graph,
        &changed_stamp_ctx,
    )
    .unwrap();
    assert_ne!(changed_stamps[0], baseline_stamps[0]);
    assert_eq!(changed_stamps[1], baseline_stamps[1]);
    let (graph, trace) = build(&mut viewport, &changed, 1.0);
    let by_owner = actions(&trace);
    assert_eq!(
        by_owner[&fixture.root],
        RetainedSurfaceCompileAction::Reraster
    );
    assert_eq!(
        by_owner[&fixture.nested],
        RetainedSurfaceCompileAction::Reuse
    );
    assert_eq!((trace.reraster_count, trace.reuse_count), (1, 1));
    assert_eq!(
        graph
            .test_graphics_passes::<crate::view::render_pass::TextureCompositePass>()
            .len(),
        1,
    );
    viewport.finish_retained_surface_transaction(false);
}

#[test]
fn descriptor_action_and_forest_tamper_reject_before_graph_pool_or_stage_mutation() {
    let fixture = property_forest_fixture(0xf4_1700, true, true);
    let plan = plan(&fixture);
    let mut viewport = Viewport::new();
    build(&mut viewport, &plan, 1.0);
    viewport.finish_retained_surface_transaction(true);
    let pool_before = viewport.retained_surface_transaction_shape_for_test();

    for (tamper, expected) in [
        (
            PropertyBoundaryForestPrepareTamper::Descriptor,
            RetainedSurfacePrepareError::ArtifactStore,
        ),
        (
            PropertyBoundaryForestPrepareTamper::OrderedStamp,
            RetainedSurfacePrepareError::ArtifactStore,
        ),
        (
            PropertyBoundaryForestPrepareTamper::Receiver,
            RetainedSurfacePrepareError::ArtifactStore,
        ),
        (
            PropertyBoundaryForestPrepareTamper::OmittedAction,
            RetainedSurfacePrepareError::ActionSet,
        ),
    ] {
        let mut graph = FrameGraph::new();
        let ctx = parent_context(&mut graph, 1.0);
        let graph_before = graph.build_state_snapshot_for_test();
        let error = prepare_property_boundary_forest_with_tamper_for_test(
            &viewport, &plan, &graph, &ctx, tamper,
        )
        .expect_err("tampered prepare authority");
        assert_eq!(error, expected);
        assert_eq!(graph.build_state_snapshot_for_test(), graph_before);
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            pool_before
        );
    }
}
