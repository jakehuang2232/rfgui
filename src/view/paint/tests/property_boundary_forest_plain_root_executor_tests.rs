use rustc_hash::FxHashMap;

use super::*;
use crate::view::Viewport;

struct PlainRootExecutorFixture {
    arena: NodeArena,
    roots: Vec<NodeKey>,
    plain_roots: [NodeKey; 3],
    property_roots: [NodeKey; 2],
    property_children: [NodeKey; 2],
    property_contents: [NodeKey; 2],
    properties: PropertyTrees,
    generations: PaintGenerationTracker,
}

fn element(stable_id: u64, color: Color) -> Element {
    let mut element = Element::new_with_id(stable_id, 0.0, 0.0, 112.0, 84.0);
    let mut style = Style::new();
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    style.insert(PropertyId::BackgroundColor, ParsedValue::color_like(color));
    element.apply_style(style);
    element
}

fn fixture() -> PlainRootExecutorFixture {
    let mut arena = new_test_arena();
    let plain_before = commit_element(
        &mut arena,
        Box::new(element(0xf5_5101, Color::rgb(35, 75, 115))),
    );
    let property_a = commit_element(
        &mut arena,
        Box::new(element(0xf5_5102, Color::rgb(25, 55, 95))),
    );
    let content_a = commit_child(
        &mut arena,
        property_a,
        Box::new(element(0xf5_5103, Color::rgb(45, 125, 85))),
    );
    let child_a = commit_child(
        &mut arena,
        property_a,
        Box::new(element(0xf5_5104, Color::rgb(165, 65, 35))),
    );
    commit_child(
        &mut arena,
        child_a,
        Box::new(element(0xf5_5105, Color::rgb(45, 145, 105))),
    );
    let plain_between = commit_element(
        &mut arena,
        Box::new(element(0xf5_5106, Color::rgb(145, 95, 45))),
    );
    let property_b = commit_element(
        &mut arena,
        Box::new(element(0xf5_5107, Color::rgb(125, 75, 155))),
    );
    let content_b = commit_child(
        &mut arena,
        property_b,
        Box::new(element(0xf5_5108, Color::rgb(75, 125, 65))),
    );
    let child_b = commit_child(
        &mut arena,
        property_b,
        Box::new(element(0xf5_5109, Color::rgb(55, 135, 175))),
    );
    commit_child(
        &mut arena,
        child_b,
        Box::new(element(0xf5_5110, Color::rgb(175, 85, 125))),
    );
    let plain_after = commit_element(
        &mut arena,
        Box::new(element(0xf5_5111, Color::rgb(65, 105, 145))),
    );
    let roots = vec![
        plain_before,
        property_a,
        plain_between,
        property_b,
        plain_after,
    ];
    let (measure, place) = constraints();
    for &root in &roots {
        measure_and_place(&mut arena, root, measure, place);
    }
    crate::view::test_support::get_element_mut::<Element>(&arena, property_a)
        .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
            2.0, 1.0, 0.0,
        ))));
    crate::view::test_support::get_element_mut::<Element>(&arena, child_a).set_opacity(0.57);
    crate::view::test_support::get_element_mut::<Element>(&arena, property_b).set_opacity(0.63);
    crate::view::test_support::get_element_mut::<Element>(&arena, child_b)
        .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
            4.0, 1.0, 0.0,
        ))));
    for &root in &roots {
        arena.refresh_subtree_dirty_cache(root);
    }
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &roots);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &roots, &properties);
    PlainRootExecutorFixture {
        arena,
        roots,
        plain_roots: [plain_before, plain_between, plain_after],
        property_roots: [property_a, property_b],
        property_children: [child_a, child_b],
        property_contents: [content_a, content_b],
        properties,
        generations,
    }
}

fn sync(fixture: &mut PlainRootExecutorFixture) {
    fixture.properties.sync(&fixture.arena, &fixture.roots);
    fixture
        .generations
        .sync(&fixture.arena, &fixture.roots, &fixture.properties);
}

fn plan(fixture: &PlainRootExecutorFixture) -> FramePaintPlan {
    plan_property_effect_scene_with_context(
        &fixture.arena,
        &fixture.roots,
        &fixture.properties,
        &fixture.generations,
        TransformSurfacePlanContext::new([0.0, 0.0], None),
    )
    .unwrap()
}

fn context(graph: &mut FrameGraph, dpr: f32) -> UiBuildContext {
    let mut ctx = UiBuildContext::new(360, 260, wgpu::TextureFormat::Bgra8Unorm, dpr);
    let target = ctx.allocate_target(graph);
    ctx.set_current_target(target);
    ctx
}

fn build(
    viewport: &mut Viewport,
    plan: &FramePaintPlan,
    dpr: f32,
) -> (FrameGraph, RetainedPropertySceneBuildTrace) {
    let mut graph = FrameGraph::new();
    let ctx = context(&mut graph, dpr);
    let trace =
        build_retained_property_scene_with_forced_pool_for_test(viewport, plan, &mut graph, ctx)
            .unwrap()
            .into_parts()
            .1;
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

fn repaint(arena: &NodeArena, owner: NodeKey, color: Color) {
    let mut style = Style::new();
    style.insert(PropertyId::BackgroundColor, ParsedValue::color_like(color));
    crate::view::test_support::get_element_mut::<Element>(arena, owner).apply_style(style);
}

#[test]
fn plain_roots_add_no_residents_to_cold_warm_joint_transactions() {
    let fixture = fixture();
    let plan = plan(&fixture);
    for dpr in [1.0, 2.0] {
        let mut viewport = Viewport::new();
        let (_, cold) = build(&mut viewport, &plan, dpr);
        assert_eq!((cold.root_count, cold.surface_count), (5, 4));
        assert_eq!((cold.reraster_count, cold.reuse_count), (4, 0));
        viewport.finish_retained_surface_transaction(true);
        assert_eq!(viewport.retained_surface_transaction_shape_for_test().0, 4);
        let (_, warm) = build(&mut viewport, &plan, dpr);
        assert_eq!((warm.reraster_count, warm.reuse_count), (0, 4));
        viewport.finish_retained_surface_transaction(false);
    }
}

#[test]
fn plain_root_invalidation_keeps_all_property_residents_reusable() {
    let mut fixture = fixture();
    let mut viewport = Viewport::new();
    build(&mut viewport, &plan(&fixture), 1.0);
    viewport.finish_retained_surface_transaction(true);

    repaint(
        &fixture.arena,
        fixture.plain_roots[1],
        Color::rgb(225, 45, 95),
    );
    sync(&mut fixture);
    let (_, trace) = build(&mut viewport, &plan(&fixture), 1.0);
    assert_eq!((trace.reraster_count, trace.reuse_count), (0, 4));
    viewport.finish_retained_surface_transaction(false);
}

#[test]
fn property_root_invalidation_does_not_pollute_plain_or_other_property_root() {
    let mut fixture = fixture();
    let mut viewport = Viewport::new();
    build(&mut viewport, &plan(&fixture), 1.0);
    viewport.finish_retained_surface_transaction(true);

    repaint(
        &fixture.arena,
        fixture.property_contents[0],
        Color::rgb(215, 35, 85),
    );
    sync(&mut fixture);
    let (_, trace) = build(&mut viewport, &plan(&fixture), 1.0);
    let actions = actions(&trace);
    assert_eq!(
        actions[&fixture.property_roots[0]],
        RetainedSurfaceCompileAction::Reraster
    );
    assert_eq!(
        actions[&fixture.property_children[0]],
        RetainedSurfaceCompileAction::Reuse
    );
    assert_eq!(
        actions[&fixture.property_roots[1]],
        RetainedSurfaceCompileAction::Reuse
    );
    assert_eq!(
        actions[&fixture.property_children[1]],
        RetainedSurfaceCompileAction::Reuse
    );
    assert_eq!((trace.reraster_count, trace.reuse_count), (1, 3));
    viewport.finish_retained_surface_transaction(false);
}

#[test]
fn reorder_and_plain_then_property_removal_commit_exact_smaller_full_sets() {
    let mut fixture = fixture();
    let property_a = fixture.property_roots[0];
    let mut viewport = Viewport::new();
    build(&mut viewport, &plan(&fixture), 1.0);
    viewport.finish_retained_surface_transaction(true);

    fixture.roots.reverse();
    sync(&mut fixture);
    let (_, reordered) = build(&mut viewport, &plan(&fixture), 1.0);
    assert_eq!((reordered.reraster_count, reordered.reuse_count), (0, 4));
    viewport.finish_retained_surface_transaction(true);
    assert_eq!(viewport.retained_surface_transaction_shape_for_test().0, 4);

    let removed_plain = fixture.plain_roots[1];
    assert_eq!(fixture.arena.remove_subtree(removed_plain), 1);
    fixture.roots.retain(|root| *root != removed_plain);
    sync(&mut fixture);
    let (_, plain_removed) = build(&mut viewport, &plan(&fixture), 1.0);
    assert_eq!(
        (plain_removed.reraster_count, plain_removed.reuse_count),
        (0, 4)
    );
    viewport.finish_retained_surface_transaction(true);
    assert_eq!(viewport.retained_surface_transaction_shape_for_test().0, 4);

    assert_eq!(fixture.arena.remove_subtree(property_a), 4);
    fixture.roots.retain(|root| *root != property_a);
    sync(&mut fixture);
    let (_, property_removed) = build(&mut viewport, &plan(&fixture), 1.0);
    assert_eq!(
        (
            property_removed.reraster_count,
            property_removed.reuse_count
        ),
        (0, 2)
    );
    viewport.finish_retained_surface_transaction(true);
    assert_eq!(viewport.retained_surface_transaction_shape_for_test().0, 2);
}

#[test]
fn plain_root_joint_stamp_and_action_tampers_reject_before_mutation() {
    let fixture = fixture();
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
        let ctx = context(&mut graph, 1.0);
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
