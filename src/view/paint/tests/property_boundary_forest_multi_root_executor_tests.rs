use rustc_hash::FxHashMap;

use super::*;
use crate::view::Viewport;

struct MultiRootExecutorFixture {
    arena: NodeArena,
    roots: Vec<NodeKey>,
    children: [NodeKey; 2],
    root_contents: [NodeKey; 2],
    properties: PropertyTrees,
    generations: PaintGenerationTracker,
}

fn root_element(stable_id: u64, color: Color) -> Element {
    let mut element = Element::new_with_id(stable_id, 0.0, 0.0, 118.0, 88.0);
    let mut style = Style::new();
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    style.insert(PropertyId::BackgroundColor, ParsedValue::color_like(color));
    element.apply_style(style);
    element
}

fn apply_transform(arena: &NodeArena, owner: NodeKey, ordinal: usize) {
    crate::view::test_support::get_element_mut::<Element>(arena, owner)
        .set_resolved_transform_for_test(Some(glam::Mat4::from_translation(glam::Vec3::new(
            2.0 + ordinal as f32,
            1.0,
            0.0,
        ))));
}

fn apply_effect(arena: &NodeArena, owner: NodeKey, ordinal: usize) {
    crate::view::test_support::get_element_mut::<Element>(arena, owner)
        .set_opacity(0.52 + ordinal as f32 * 0.07);
}

fn multi_root_fixture() -> MultiRootExecutorFixture {
    let mut arena = new_test_arena();
    let root_a = commit_element(
        &mut arena,
        Box::new(root_element(0xf5_1101, Color::rgb(25, 55, 95))),
    );
    let root_content_a = commit_child(
        &mut arena,
        root_a,
        Box::new(root_element(0xf5_1102, Color::rgb(35, 125, 75))),
    );
    let child_a = commit_child(
        &mut arena,
        root_a,
        Box::new(root_element(0xf5_1103, Color::rgb(165, 65, 35))),
    );
    commit_child(
        &mut arena,
        child_a,
        Box::new(root_element(0xf5_1104, Color::rgb(45, 145, 105))),
    );

    let root_b = commit_element(
        &mut arena,
        Box::new(root_element(0xf5_1105, Color::rgb(35, 75, 115))),
    );
    let root_content_b = commit_child(
        &mut arena,
        root_b,
        Box::new(root_element(0xf5_1106, Color::rgb(125, 95, 45))),
    );
    let child_b = commit_child(
        &mut arena,
        root_b,
        Box::new(root_element(0xf5_1107, Color::rgb(135, 75, 155))),
    );
    commit_child(
        &mut arena,
        child_b,
        Box::new(root_element(0xf5_1108, Color::rgb(55, 135, 175))),
    );
    let (measure, place) = constraints();
    for root in [root_a, root_b] {
        measure_and_place(&mut arena, root, measure, place);
    }
    apply_transform(&arena, root_a, 0);
    apply_effect(&arena, child_a, 1);
    apply_effect(&arena, root_b, 2);
    apply_transform(&arena, child_b, 3);
    for root in [root_a, root_b] {
        arena.refresh_subtree_dirty_cache(root);
    }
    let roots = vec![root_a, root_b];
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &roots);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &roots, &properties);
    MultiRootExecutorFixture {
        arena,
        roots,
        children: [child_a, child_b],
        root_contents: [root_content_a, root_content_b],
        properties,
        generations,
    }
}

fn sync_fixture(fixture: &mut MultiRootExecutorFixture) {
    fixture.properties.sync(&fixture.arena, &fixture.roots);
    fixture
        .generations
        .sync(&fixture.arena, &fixture.roots, &fixture.properties);
}

fn plan(fixture: &MultiRootExecutorFixture) -> FramePaintPlan {
    plan_property_effect_scene_with_context(
        &fixture.arena,
        &fixture.roots,
        &fixture.properties,
        &fixture.generations,
        TransformSurfacePlanContext::new([0.0, 0.0], None),
    )
    .expect("heterogeneous multi-root property scene")
}

fn parent_context(graph: &mut FrameGraph, dpr: f32) -> UiBuildContext {
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
    let ctx = parent_context(&mut graph, dpr);
    let outcome =
        build_retained_property_scene_with_forced_pool_for_test(viewport, plan, &mut graph, ctx)
            .expect("multi-root scene preflights and emits");
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

fn repaint(arena: &NodeArena, owner: NodeKey, color: Color) {
    let mut style = Style::new();
    style.insert(PropertyId::BackgroundColor, ParsedValue::color_like(color));
    crate::view::test_support::get_element_mut::<Element>(arena, owner).apply_style(style);
}

#[test]
fn heterogeneous_roots_are_one_cold_warm_joint_transaction_at_dpr1_and_dpr2() {
    let fixture = multi_root_fixture();
    let plan = plan(&fixture);
    for dpr in [1.0, 2.0] {
        let mut viewport = Viewport::new();
        let mut preflight_graph = FrameGraph::new();
        let preflight_ctx = parent_context(&mut preflight_graph, dpr);
        let stamps = prepare_retained_property_scene_stamps_for_test(
            &viewport,
            &plan,
            &preflight_graph,
            &preflight_ctx,
        )
        .expect("ordered multi-root stamps");
        assert_eq!(stamps.len(), 4);
        assert_eq!(stamps[0].identity.boundary_root, fixture.roots[0]);
        assert_eq!(stamps[1].identity.boundary_root, fixture.children[0]);
        assert_eq!(stamps[2].identity.boundary_root, fixture.roots[1]);
        assert_eq!(stamps[3].identity.boundary_root, fixture.children[1]);

        let (_, cold) = build(&mut viewport, &plan, dpr);
        assert_eq!((cold.root_count, cold.surface_count), (2, 4));
        assert_eq!((cold.reraster_count, cold.reuse_count), (4, 0));
        viewport.finish_retained_surface_transaction(true);
        assert_eq!(viewport.retained_surface_transaction_shape_for_test().0, 4);
        let (_, warm) = build(&mut viewport, &plan, dpr);
        assert_eq!((warm.reraster_count, warm.reuse_count), (0, 4));
        viewport.finish_retained_surface_transaction(false);
    }
}

#[test]
fn root_a_content_mutation_does_not_pollute_root_b_residents() {
    let mut fixture = multi_root_fixture();
    let baseline = plan(&fixture);
    let mut viewport = Viewport::new();
    build(&mut viewport, &baseline, 1.0);
    viewport.finish_retained_surface_transaction(true);

    repaint(
        &fixture.arena,
        fixture.root_contents[0],
        Color::rgb(225, 45, 95),
    );
    sync_fixture(&mut fixture);
    let (_, trace) = build(&mut viewport, &plan(&fixture), 1.0);
    let actions = actions(&trace);
    assert_eq!(
        actions[&fixture.roots[0]],
        RetainedSurfaceCompileAction::Reraster
    );
    assert_eq!(
        actions[&fixture.children[0]],
        RetainedSurfaceCompileAction::Reuse
    );
    assert_eq!(
        actions[&fixture.roots[1]],
        RetainedSurfaceCompileAction::Reuse
    );
    assert_eq!(
        actions[&fixture.children[1]],
        RetainedSurfaceCompileAction::Reuse
    );
    assert_eq!((trace.reraster_count, trace.reuse_count), (1, 3));
    viewport.finish_retained_surface_transaction(false);
}

#[test]
fn root_reorder_preserves_residents_but_stages_the_new_exact_joint_order() {
    let mut fixture = multi_root_fixture();
    let mut viewport = Viewport::new();
    build(&mut viewport, &plan(&fixture), 1.0);
    viewport.finish_retained_surface_transaction(true);

    fixture.roots.swap(0, 1);
    sync_fixture(&mut fixture);
    let reordered = plan(&fixture);
    let (_, trace) = build(&mut viewport, &reordered, 1.0);
    assert_eq!((trace.reraster_count, trace.reuse_count), (0, 4));
    assert_eq!(trace.surfaces[0].boundary_root, fixture.roots[0]);
    assert_eq!(trace.surfaces[2].boundary_root, fixture.roots[1]);
    viewport.finish_retained_surface_transaction(true);
    assert_eq!(viewport.retained_surface_transaction_shape_for_test().0, 4);
}

#[test]
fn removing_one_root_commits_one_smaller_atomic_full_set() {
    let mut fixture = multi_root_fixture();
    let removed_root = fixture.roots[0];
    let survivor_root = fixture.roots[1];
    let survivor_child = fixture.children[1];
    let mut viewport = Viewport::new();
    build(&mut viewport, &plan(&fixture), 1.0);
    viewport.finish_retained_surface_transaction(true);
    assert_eq!(viewport.retained_surface_transaction_shape_for_test().0, 4);

    assert_eq!(fixture.arena.remove_subtree(removed_root), 4);
    fixture.roots.remove(0);
    sync_fixture(&mut fixture);
    let (_, trace) = build(&mut viewport, &plan(&fixture), 1.0);
    let actions = actions(&trace);
    assert_eq!(actions.len(), 2);
    assert_eq!(actions[&survivor_root], RetainedSurfaceCompileAction::Reuse);
    assert_eq!(
        actions[&survivor_child],
        RetainedSurfaceCompileAction::Reuse
    );
    viewport.finish_retained_surface_transaction(true);
    assert_eq!(viewport.retained_surface_transaction_shape_for_test().0, 2);
}

#[test]
fn multi_root_prepare_tampers_reject_before_graph_pool_or_stage_mutation() {
    let fixture = multi_root_fixture();
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
