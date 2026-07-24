use rustc_hash::FxHashMap;

use super::*;
use crate::view::Viewport;

#[derive(Clone, Copy)]
enum LinearRole {
    Transform,
    Effect,
}

struct LinearExecutorFixture {
    arena: NodeArena,
    root: NodeKey,
    boundaries: Vec<NodeKey>,
    properties: PropertyTrees,
    generations: PaintGenerationTracker,
}

fn linear_element(id: u64, color: Color) -> Element {
    let mut element = Element::new_with_id(id, 0.0, 0.0, 120.0, 90.0);
    let mut style = Style::new();
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    style.insert(PropertyId::BackgroundColor, ParsedValue::color_like(color));
    element.apply_style(style);
    element
}

fn linear_fixture(
    roles: &[LinearRole],
    neutral_wrappers: bool,
    stable_id_base: u64,
) -> LinearExecutorFixture {
    let mut arena = new_test_arena();
    let root = commit_element(
        &mut arena,
        Box::new(linear_element(stable_id_base + 1, Color::rgb(25, 55, 95))),
    );
    let mut boundaries = vec![root];
    let mut parent = root;
    let mut next_id = stable_id_base + 1;
    for ordinal in 1..roles.len() {
        if neutral_wrappers {
            next_id += 1;
            parent = commit_child(
                &mut arena,
                parent,
                Box::new(linear_element(
                    next_id,
                    Color::rgb(45, 75 + ordinal as u8, 105),
                )),
            );
        }
        next_id += 1;
        parent = commit_child(
            &mut arena,
            parent,
            Box::new(linear_element(
                next_id,
                Color::rgb(165, 65 + ordinal as u8, 35),
            )),
        );
        boundaries.push(parent);
    }
    commit_child(
        &mut arena,
        parent,
        Box::new(linear_element(next_id + 1, Color::rgb(30, 155, 105))),
    );
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    for (ordinal, (&owner, role)) in boundaries.iter().zip(roles).enumerate() {
        let mut element = crate::view::test_support::get_element_mut::<Element>(&arena, owner);
        match role {
            LinearRole::Transform => {
                element.set_resolved_transform_for_test(Some(glam::Mat4::from_translation(
                    glam::Vec3::new(2.0 + ordinal as f32, 1.0, 0.0),
                )));
            }
            LinearRole::Effect => element.set_opacity(0.45 + ordinal as f32 * 0.04),
        }
    }
    arena.refresh_subtree_dirty_cache(root);
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[root]);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[root], &properties);
    LinearExecutorFixture {
        arena,
        root,
        boundaries,
        properties,
        generations,
    }
}

fn sync_fixture(fixture: &mut LinearExecutorFixture) {
    fixture.properties.sync(&fixture.arena, &[fixture.root]);
    fixture
        .generations
        .sync(&fixture.arena, &[fixture.root], &fixture.properties);
}

fn linear_plan(fixture: &LinearExecutorFixture) -> FramePaintPlan {
    plan_property_effect_scene_with_context(
        &fixture.arena,
        &[fixture.root],
        &fixture.properties,
        &fixture.generations,
        TransformSurfacePlanContext::new([0.0, 0.0], None),
    )
    .expect("reviewed arbitrary-depth linear property forest")
}

fn parent_context(graph: &mut FrameGraph, dpr: f32) -> UiBuildContext {
    let mut ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, dpr);
    let target = ctx.allocate_target(graph);
    ctx.set_current_target(target);
    ctx
}

fn build(
    viewport: &mut Viewport,
    plan: &FramePaintPlan,
    dpr: f32,
) -> RetainedPropertySceneBuildTrace {
    let mut graph = FrameGraph::new();
    let ctx = parent_context(&mut graph, dpr);
    build_retained_property_scene_with_forced_pool_for_test(viewport, plan, &mut graph, ctx)
        .expect("linear property forest preflights and emits")
        .into_parts()
        .1
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
fn depth_four_tete_and_depth_five_etete_dpr1_dpr2_cold_warm() {
    use LinearRole::{Effect, Transform};
    for (roles, neutral, stable_id_base) in [
        (vec![Transform, Effect, Transform, Effect], false, 0xf4_7100),
        (vec![Transform, Effect, Transform, Effect], true, 0xf4_7200),
        (
            vec![Effect, Transform, Effect, Transform, Effect],
            false,
            0xf4_7300,
        ),
        (
            vec![Effect, Transform, Effect, Transform, Effect],
            true,
            0xf4_7400,
        ),
    ] {
        let fixture = linear_fixture(&roles, neutral, stable_id_base);
        let plan = linear_plan(&fixture);
        for dpr in [1.0, 2.0] {
            let mut viewport = Viewport::new();
            let cold = build(&mut viewport, &plan, dpr);
            assert_eq!((cold.reraster_count, cold.reuse_count), (roles.len(), 0));
            viewport.finish_retained_surface_transaction(true);
            let warm = build(&mut viewport, &plan, dpr);
            assert_eq!((warm.reraster_count, warm.reuse_count), (0, roles.len()));
            assert_eq!(
                viewport.retained_surface_transaction_shape_for_test().0,
                roles.len()
            );
            viewport.finish_retained_surface_transaction(false);
        }
    }
}

#[test]
fn middle_effect_invalidation_propagates_only_to_ancestors() {
    use LinearRole::{Effect, Transform};
    let mut fixture = linear_fixture(
        &[Transform, Effect, Transform, Effect, Transform],
        true,
        0xf4_7500,
    );
    let baseline = linear_plan(&fixture);
    let mut viewport = Viewport::new();
    build(&mut viewport, &baseline, 1.0);
    viewport.finish_retained_surface_transaction(true);

    crate::view::test_support::get_element_mut::<Element>(&fixture.arena, fixture.boundaries[3])
        .set_opacity(0.25);
    sync_fixture(&mut fixture);
    let changed = linear_plan(&fixture);
    let trace = build(&mut viewport, &changed, 1.0);
    let by_owner = actions(&trace);
    for (ordinal, owner) in fixture.boundaries.iter().copied().enumerate() {
        assert_eq!(
            by_owner[&owner],
            if ordinal < 3 {
                RetainedSurfaceCompileAction::Reraster
            } else {
                RetainedSurfaceCompileAction::Reuse
            },
            "boundary ordinal {ordinal}",
        );
    }
    assert_eq!((trace.reraster_count, trace.reuse_count), (3, 2));
    viewport.finish_retained_surface_transaction(false);
}

#[test]
fn depth_five_prepare_tamper_is_atomic() {
    use LinearRole::{Effect, Transform};
    let fixture = linear_fixture(
        &[Effect, Transform, Effect, Transform, Effect],
        true,
        0xf4_7600,
    );
    let plan = linear_plan(&fixture);
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
