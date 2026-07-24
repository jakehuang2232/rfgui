use rustc_hash::FxHashMap;

use super::*;
use crate::view::Viewport;

#[derive(Clone, Copy)]
enum DepthThreeTopology {
    TransformEffectTransform,
    EffectTransformEffect,
}

struct DepthThreeFixture {
    arena: NodeArena,
    root: NodeKey,
    middle: NodeKey,
    leaf: NodeKey,
    sibling: NodeKey,
    properties: PropertyTrees,
    generations: PaintGenerationTracker,
}

fn depth_three_element(id: u64, color: Color) -> Element {
    let mut element = Element::new_with_id(id, 0.0, 0.0, 120.0, 90.0);
    let mut style = Style::new();
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    style.insert(PropertyId::BackgroundColor, ParsedValue::color_like(color));
    element.apply_style(style);
    element
}

fn depth_three_fixture(
    topology: DepthThreeTopology,
    neutral_wrappers: bool,
    stable_id_base: u64,
) -> DepthThreeFixture {
    let mut arena = new_test_arena();
    let root = commit_element(
        &mut arena,
        Box::new(depth_three_element(
            stable_id_base + 1,
            Color::rgb(25, 55, 95),
        )),
    );
    let sibling = commit_child(
        &mut arena,
        root,
        Box::new(depth_three_element(
            stable_id_base + 2,
            Color::rgb(35, 125, 75),
        )),
    );
    let middle_parent = if neutral_wrappers {
        commit_child(
            &mut arena,
            root,
            Box::new(depth_three_element(
                stable_id_base + 3,
                Color::rgb(45, 75, 105),
            )),
        )
    } else {
        root
    };
    let middle = commit_child(
        &mut arena,
        middle_parent,
        Box::new(depth_three_element(
            stable_id_base + 4,
            Color::rgb(165, 65, 35),
        )),
    );
    let leaf_parent = if neutral_wrappers {
        commit_child(
            &mut arena,
            middle,
            Box::new(depth_three_element(
                stable_id_base + 5,
                Color::rgb(70, 85, 115),
            )),
        )
    } else {
        middle
    };
    let leaf = commit_child(
        &mut arena,
        leaf_parent,
        Box::new(depth_three_element(
            stable_id_base + 6,
            Color::rgb(185, 75, 30),
        )),
    );
    commit_child(
        &mut arena,
        leaf,
        Box::new(depth_three_element(
            stable_id_base + 7,
            Color::rgb(30, 155, 105),
        )),
    );
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    let set_transform = |arena: &NodeArena, owner, x| {
        crate::view::test_support::get_element_mut::<Element>(arena, owner)
            .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
                x, 2.0, 0.0,
            ))));
    };
    match topology {
        DepthThreeTopology::TransformEffectTransform => {
            set_transform(&arena, root, 2.0);
            crate::view::test_support::get_element_mut::<Element>(&arena, middle).set_opacity(0.55);
            set_transform(&arena, leaf, 5.0);
        }
        DepthThreeTopology::EffectTransformEffect => {
            crate::view::test_support::get_element_mut::<Element>(&arena, root).set_opacity(0.55);
            set_transform(&arena, middle, 4.0);
            crate::view::test_support::get_element_mut::<Element>(&arena, leaf).set_opacity(0.7);
        }
    }
    arena.refresh_subtree_dirty_cache(root);
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[root]);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[root], &properties);
    DepthThreeFixture {
        arena,
        root,
        middle,
        leaf,
        sibling,
        properties,
        generations,
    }
}

fn depth_three_plan(fixture: &DepthThreeFixture) -> FramePaintPlan {
    plan_property_effect_scene_with_context(
        &fixture.arena,
        &[fixture.root],
        &fixture.properties,
        &fixture.generations,
        TransformSurfacePlanContext::new([0.0, 0.0], None),
    )
    .expect("reviewed depth-three property forest")
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
            .expect("depth-three property forest preflights and emits");
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

fn sync_fixture(fixture: &mut DepthThreeFixture) {
    fixture.properties.sync(&fixture.arena, &[fixture.root]);
    fixture
        .generations
        .sync(&fixture.arena, &[fixture.root], &fixture.properties);
}

fn assert_actions(
    fixture: &DepthThreeFixture,
    trace: &RetainedPropertySceneBuildTrace,
    expected: [(NodeKey, RetainedSurfaceCompileAction); 3],
) {
    let by_owner = actions(trace);
    assert_eq!(by_owner.len(), 3);
    for (owner, action) in expected {
        assert_eq!(by_owner[&owner], action);
    }
    assert_eq!(
        trace.reraster_count + trace.reuse_count,
        3,
        "fixture owners: {:?}",
        [fixture.root, fixture.middle, fixture.leaf],
    );
}

fn cold_then_warm(
    fixture: &DepthThreeFixture,
    dpr: f32,
) -> (FrameGraph, RetainedPropertySceneBuildTrace) {
    let plan = depth_three_plan(fixture);
    let mut viewport = Viewport::new();
    let (cold_graph, cold) = build(&mut viewport, &plan, dpr);
    assert_eq!((cold.reraster_count, cold.reuse_count), (3, 0));
    viewport.finish_retained_surface_transaction(true);
    let (_, warm) = build(&mut viewport, &plan, dpr);
    assert_eq!((warm.reraster_count, warm.reuse_count), (0, 3));
    viewport.finish_retained_surface_transaction(false);
    (cold_graph, warm)
}

#[test]
fn transform_effect_transform_direct_and_neutral_dpr1_dpr2_cold_warm() {
    for (neutral, stable_id_base) in [(false, 0xf4_4100), (true, 0xf4_4200)] {
        let fixture = depth_three_fixture(
            DepthThreeTopology::TransformEffectTransform,
            neutral,
            stable_id_base,
        );
        for dpr in [1.0, 2.0] {
            let (graph, _) = cold_then_warm(&fixture, dpr);
            assert_eq!(
                graph
                    .test_graphics_passes::<crate::view::render_pass::TextureCompositePass>()
                    .len(),
                2
            );
            assert_eq!(
                graph
                    .test_graphics_passes::<
                        crate::view::render_pass::composite_layer_pass::CompositeLayerPass,
                    >()
                    .len(),
                1
            );
        }
    }
}

#[test]
fn effect_transform_effect_direct_and_neutral_dpr1_dpr2_cold_warm() {
    for (neutral, stable_id_base) in [(false, 0xf4_4300), (true, 0xf4_4400)] {
        let fixture = depth_three_fixture(
            DepthThreeTopology::EffectTransformEffect,
            neutral,
            stable_id_base,
        );
        for dpr in [1.0, 2.0] {
            let (graph, _) = cold_then_warm(&fixture, dpr);
            assert_eq!(
                graph
                    .test_graphics_passes::<crate::view::render_pass::TextureCompositePass>()
                    .len(),
                1
            );
            assert_eq!(
                graph
                    .test_graphics_passes::<
                        crate::view::render_pass::composite_layer_pass::CompositeLayerPass,
                    >()
                    .len(),
                2
            );
        }
    }
}

#[test]
fn transform_effect_transform_composite_invalidation_is_ancestor_scoped() {
    let mut top = depth_three_fixture(
        DepthThreeTopology::TransformEffectTransform,
        false,
        0xf4_4500,
    );
    let baseline = depth_three_plan(&top);
    let mut viewport = Viewport::new();
    build(&mut viewport, &baseline, 1.0);
    viewport.finish_retained_surface_transaction(true);
    crate::view::test_support::get_element_mut::<Element>(&top.arena, top.root)
        .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
            11.0, 3.0, 0.0,
        ))));
    sync_fixture(&mut top);
    let (_, trace) = build(&mut viewport, &depth_three_plan(&top), 1.0);
    assert_actions(
        &top,
        &trace,
        [
            (top.root, RetainedSurfaceCompileAction::Reraster),
            (top.middle, RetainedSurfaceCompileAction::Reuse),
            (top.leaf, RetainedSurfaceCompileAction::Reuse),
        ],
    );
    viewport.finish_retained_surface_transaction(false);

    let mut middle = depth_three_fixture(
        DepthThreeTopology::TransformEffectTransform,
        true,
        0xf4_4600,
    );
    let baseline = depth_three_plan(&middle);
    let mut viewport = Viewport::new();
    build(&mut viewport, &baseline, 1.0);
    viewport.finish_retained_surface_transaction(true);
    crate::view::test_support::get_element_mut::<Element>(&middle.arena, middle.middle)
        .set_opacity(0.25);
    sync_fixture(&mut middle);
    let (_, trace) = build(&mut viewport, &depth_three_plan(&middle), 1.0);
    assert_actions(
        &middle,
        &trace,
        [
            (middle.root, RetainedSurfaceCompileAction::Reraster),
            (middle.middle, RetainedSurfaceCompileAction::Reuse),
            (middle.leaf, RetainedSurfaceCompileAction::Reuse),
        ],
    );
    viewport.finish_retained_surface_transaction(false);
}

#[test]
fn effect_transform_effect_composite_invalidation_is_ancestor_scoped() {
    let mut top = depth_three_fixture(DepthThreeTopology::EffectTransformEffect, false, 0xf4_4700);
    let baseline = depth_three_plan(&top);
    let mut viewport = Viewport::new();
    build(&mut viewport, &baseline, 1.0);
    viewport.finish_retained_surface_transaction(true);
    crate::view::test_support::get_element_mut::<Element>(&top.arena, top.root).set_opacity(0.3);
    sync_fixture(&mut top);
    let (_, trace) = build(&mut viewport, &depth_three_plan(&top), 1.0);
    assert_eq!((trace.reraster_count, trace.reuse_count), (0, 3));
    viewport.finish_retained_surface_transaction(false);

    let mut middle =
        depth_three_fixture(DepthThreeTopology::EffectTransformEffect, true, 0xf4_4800);
    let baseline = depth_three_plan(&middle);
    let mut viewport = Viewport::new();
    build(&mut viewport, &baseline, 1.0);
    viewport.finish_retained_surface_transaction(true);
    crate::view::test_support::get_element_mut::<Element>(&middle.arena, middle.middle)
        .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
            13.0, 4.0, 0.0,
        ))));
    sync_fixture(&mut middle);
    let (_, trace) = build(&mut viewport, &depth_three_plan(&middle), 1.0);
    assert_actions(
        &middle,
        &trace,
        [
            (middle.root, RetainedSurfaceCompileAction::Reraster),
            (middle.middle, RetainedSurfaceCompileAction::Reraster),
            (middle.leaf, RetainedSurfaceCompileAction::Reuse),
        ],
    );
    viewport.finish_retained_surface_transaction(false);
}

#[test]
fn depth_three_plain_sibling_isolation_and_role_aware_opaque_cursor_are_exact() {
    for (topology, stable_id_base) in [
        (DepthThreeTopology::TransformEffectTransform, 0xf4_4900),
        (DepthThreeTopology::EffectTransformEffect, 0xf4_4a00),
    ] {
        let mut fixture = depth_three_fixture(topology, true, stable_id_base);
        let baseline = depth_three_plan(&fixture);
        let mut viewport = Viewport::new();
        let mut graph = FrameGraph::new();
        let ctx = parent_context(&mut graph, 1.0);
        let stamps =
            prepare_retained_property_scene_stamps_for_test(&viewport, &baseline, &graph, &ctx)
                .expect("depth-three stamps");
        assert_eq!(stamps.len(), 3);
        for parent in &stamps[..2] {
            let dependency = parent
                .ordered_steps
                .iter()
                .find_map(|step| match step {
                    RetainedSurfaceRasterStepStamp::NestedSurface(dependency) => Some(dependency),
                    _ => None,
                })
                .expect("each non-leaf embeds its direct child");
            let expected_after = match dependency.child_stamp.identity.role {
                RetainedSurfaceRasterRole::Transform => dependency
                    .parent_opaque_order_before
                    .max(dependency.child_stamp.opaque_order_span.end),
                RetainedSurfaceRasterRole::PropertyEffect => dependency.parent_opaque_order_before,
                _ => unreachable!(),
            };
            assert_eq!(dependency.parent_opaque_order_after, expected_after);
        }
        build(&mut viewport, &baseline, 1.0);
        viewport.finish_retained_surface_transaction(true);

        let mut style = Style::new();
        style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::rgb(215, 35, 85)),
        );
        crate::view::test_support::get_element_mut::<Element>(&fixture.arena, fixture.sibling)
            .apply_style(style);
        sync_fixture(&mut fixture);
        let (_, trace) = build(&mut viewport, &depth_three_plan(&fixture), 1.0);
        assert_actions(
            &fixture,
            &trace,
            [
                (fixture.root, RetainedSurfaceCompileAction::Reraster),
                (fixture.middle, RetainedSurfaceCompileAction::Reuse),
                (fixture.leaf, RetainedSurfaceCompileAction::Reuse),
            ],
        );
        viewport.finish_retained_surface_transaction(false);
    }
}

#[test]
fn depth_three_tamper_rejects_before_graph_pool_or_stage_mutation() {
    let fixture = depth_three_fixture(
        DepthThreeTopology::TransformEffectTransform,
        true,
        0xf4_4b00,
    );
    let plan = depth_three_plan(&fixture);
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
        assert_eq!(
            prepare_property_boundary_forest_with_tamper_for_test(
                &viewport, &plan, &graph, &ctx, tamper,
            ),
            Err(expected),
        );
        assert_eq!(graph.build_state_snapshot_for_test(), graph_before);
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            pool_before
        );
    }
}
