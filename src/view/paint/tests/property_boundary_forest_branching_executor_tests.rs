use rustc_hash::FxHashMap;

use super::*;
use crate::view::Viewport;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BranchRole {
    Transform,
    Effect,
}

struct BranchExecutorFixture {
    arena: NodeArena,
    root: NodeKey,
    root_content: NodeKey,
    branches: Vec<NodeKey>,
    branch_contents: Vec<NodeKey>,
    root_role: BranchRole,
    child_role: BranchRole,
    next_stable_id: u64,
    properties: PropertyTrees,
    generations: PaintGenerationTracker,
}

fn branch_element(id: u64, color: Color) -> Element {
    let mut element = Element::new_with_id(id, 0.0, 0.0, 120.0, 90.0);
    let mut style = Style::new();
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    style.insert(PropertyId::BackgroundColor, ParsedValue::color_like(color));
    element.apply_style(style);
    element
}

fn apply_role(arena: &NodeArena, owner: NodeKey, role: BranchRole, ordinal: usize) {
    let mut element = crate::view::test_support::get_element_mut::<Element>(arena, owner);
    match role {
        BranchRole::Transform => {
            element.set_resolved_transform_for_test(Some(glam::Mat4::from_translation(
                glam::Vec3::new(2.0 + ordinal as f32, 1.0, 0.0),
            )));
        }
        BranchRole::Effect => element.set_opacity(0.48 + ordinal as f32 * 0.08),
    }
}

fn branch_fixture(
    root_role: BranchRole,
    child_role: BranchRole,
    neutral_wrappers: bool,
    stable_id_base: u64,
) -> BranchExecutorFixture {
    let mut arena = new_test_arena();
    let root = commit_element(
        &mut arena,
        Box::new(branch_element(stable_id_base + 1, Color::rgb(25, 55, 95))),
    );
    let root_content = commit_child(
        &mut arena,
        root,
        Box::new(branch_element(stable_id_base + 2, Color::rgb(35, 125, 75))),
    );
    let mut branches = Vec::new();
    let mut branch_contents = Vec::new();
    let mut next_stable_id = stable_id_base + 2;
    for ordinal in 0..2 {
        let parent = if neutral_wrappers {
            next_stable_id += 1;
            commit_child(
                &mut arena,
                root,
                Box::new(branch_element(
                    next_stable_id,
                    Color::rgb(45, 85 + ordinal as u8 * 10, 115),
                )),
            )
        } else {
            root
        };
        next_stable_id += 1;
        let branch = commit_child(
            &mut arena,
            parent,
            Box::new(branch_element(
                next_stable_id,
                Color::rgb(165, 65 + ordinal as u8 * 10, 35),
            )),
        );
        next_stable_id += 1;
        let content = commit_child(
            &mut arena,
            branch,
            Box::new(branch_element(
                next_stable_id,
                Color::rgb(30, 145 + ordinal as u8 * 10, 105),
            )),
        );
        branches.push(branch);
        branch_contents.push(content);
    }
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    apply_role(&arena, root, root_role, 0);
    for (ordinal, branch) in branches.iter().copied().enumerate() {
        apply_role(&arena, branch, child_role, ordinal + 1);
    }
    arena.refresh_subtree_dirty_cache(root);
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[root]);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[root], &properties);
    BranchExecutorFixture {
        arena,
        root,
        root_content,
        branches,
        branch_contents,
        root_role,
        child_role,
        next_stable_id,
        properties,
        generations,
    }
}

fn sync_fixture(fixture: &mut BranchExecutorFixture) {
    fixture.properties.sync(&fixture.arena, &[fixture.root]);
    fixture
        .generations
        .sync(&fixture.arena, &[fixture.root], &fixture.properties);
}

fn relayout_and_reapply_roles(fixture: &mut BranchExecutorFixture) {
    let (measure, place) = constraints();
    measure_and_place(&mut fixture.arena, fixture.root, measure, place);
    apply_role(&fixture.arena, fixture.root, fixture.root_role, 0);
    for (ordinal, branch) in fixture.branches.iter().copied().enumerate() {
        apply_role(&fixture.arena, branch, fixture.child_role, ordinal + 1);
    }
    fixture.arena.refresh_subtree_dirty_cache(fixture.root);
    sync_fixture(fixture);
}

fn branch_plan(fixture: &BranchExecutorFixture) -> FramePaintPlan {
    plan_property_effect_scene_with_context(
        &fixture.arena,
        &[fixture.root],
        &fixture.properties,
        &fixture.generations,
        TransformSurfacePlanContext::new([0.0, 0.0], None),
    )
    .expect("reviewed branch property scene")
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
) -> (FrameGraph, RetainedPropertySceneBuildTrace) {
    let mut graph = FrameGraph::new();
    let ctx = parent_context(&mut graph, dpr);
    let outcome =
        build_retained_property_scene_with_forced_pool_for_test(viewport, plan, &mut graph, ctx)
            .expect("branch property scene preflights and emits");
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

fn add_branch(fixture: &mut BranchExecutorFixture) -> NodeKey {
    fixture.next_stable_id += 1;
    let branch = commit_child(
        &mut fixture.arena,
        fixture.root,
        Box::new(branch_element(
            fixture.next_stable_id,
            Color::rgb(185, 85, 45),
        )),
    );
    fixture.next_stable_id += 1;
    let content = commit_child(
        &mut fixture.arena,
        branch,
        Box::new(branch_element(
            fixture.next_stable_id,
            Color::rgb(45, 165, 115),
        )),
    );
    fixture.branches.push(branch);
    fixture.branch_contents.push(content);
    relayout_and_reapply_roles(fixture);
    branch
}

#[test]
fn transform_and_effect_branch_forests_dpr1_dpr2_cold_warm_and_cursor_stamps() {
    use BranchRole::{Effect, Transform};
    for (root_role, child_role, neutral, stable_id_base) in [
        (Transform, Effect, false, 0xf4_8100),
        (Transform, Effect, true, 0xf4_8200),
        (Effect, Transform, false, 0xf4_8300),
        (Effect, Transform, true, 0xf4_8400),
    ] {
        let fixture = branch_fixture(root_role, child_role, neutral, stable_id_base);
        let plan = branch_plan(&fixture);
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
            .expect("branch stamps");
            assert_eq!(stamps.len(), 3);
            let dependencies = stamps[0]
                .ordered_steps
                .iter()
                .filter_map(|step| match step {
                    RetainedSurfaceRasterStepStamp::NestedSurface(dependency) => Some(dependency),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(dependencies.len(), 2);
            for dependency in dependencies {
                let expected_after = match dependency.child_stamp.identity.role {
                    RetainedSurfaceRasterRole::Transform => dependency
                        .parent_opaque_order_before
                        .max(dependency.child_stamp.opaque_order_span.end),
                    RetainedSurfaceRasterRole::PropertyEffect => {
                        dependency.parent_opaque_order_before
                    }
                    _ => unreachable!(),
                };
                assert_eq!(dependency.parent_opaque_order_after, expected_after);
            }

            let (_, cold) = build(&mut viewport, &plan, dpr);
            assert_eq!((cold.reraster_count, cold.reuse_count), (3, 0));
            viewport.finish_retained_surface_transaction(true);
            let (_, warm) = build(&mut viewport, &plan, dpr);
            assert_eq!((warm.reraster_count, warm.reuse_count), (0, 3));
            assert_eq!(viewport.retained_surface_transaction_shape_for_test().0, 3);
            viewport.finish_retained_surface_transaction(false);
        }
    }
}

#[test]
fn branch_content_invalidation_rerasterizes_ancestor_and_changed_branch_only() {
    use BranchRole::{Effect, Transform};
    let mut fixture = branch_fixture(Transform, Effect, true, 0xf4_8500);
    let baseline = branch_plan(&fixture);
    let mut viewport = Viewport::new();
    build(&mut viewport, &baseline, 1.0);
    viewport.finish_retained_surface_transaction(true);

    repaint(
        &fixture.arena,
        fixture.branch_contents[0],
        Color::rgb(215, 35, 85),
    );
    sync_fixture(&mut fixture);
    let (_, trace) = build(&mut viewport, &branch_plan(&fixture), 1.0);
    let by_owner = actions(&trace);
    assert_eq!(
        by_owner[&fixture.root],
        RetainedSurfaceCompileAction::Reraster
    );
    assert_eq!(
        by_owner[&fixture.branches[0]],
        RetainedSurfaceCompileAction::Reraster
    );
    assert_eq!(
        by_owner[&fixture.branches[1]],
        RetainedSurfaceCompileAction::Reuse
    );
    assert_eq!((trace.reraster_count, trace.reuse_count), (2, 1));
    viewport.finish_retained_surface_transaction(false);
}

#[test]
fn parent_content_change_rerasterizes_parent_while_both_branches_reuse() {
    use BranchRole::{Effect, Transform};
    let mut fixture = branch_fixture(Effect, Transform, true, 0xf4_8600);
    assert_eq!(fixture.root_role, Effect);
    let baseline = branch_plan(&fixture);
    let mut viewport = Viewport::new();
    build(&mut viewport, &baseline, 1.0);
    viewport.finish_retained_surface_transaction(true);

    repaint(
        &fixture.arena,
        fixture.root_content,
        Color::rgb(225, 45, 95),
    );
    sync_fixture(&mut fixture);
    let (_, trace) = build(&mut viewport, &branch_plan(&fixture), 1.0);
    let by_owner = actions(&trace);
    assert_eq!(
        by_owner[&fixture.root],
        RetainedSurfaceCompileAction::Reraster
    );
    for branch in &fixture.branches {
        assert_eq!(
            by_owner[branch],
            RetainedSurfaceCompileAction::Reuse,
            "unchanged sibling resident",
        );
    }
    assert_eq!((trace.reraster_count, trace.reuse_count), (1, 2));
    viewport.finish_retained_surface_transaction(false);
}

#[test]
fn adding_and_removing_a_branch_updates_one_atomic_full_set() {
    use BranchRole::{Effect, Transform};
    let mut fixture = branch_fixture(Transform, Effect, false, 0xf4_8700);
    let mut viewport = Viewport::new();
    build(&mut viewport, &branch_plan(&fixture), 1.0);
    viewport.finish_retained_surface_transaction(true);
    assert_eq!(viewport.retained_surface_transaction_shape_for_test().0, 3);

    let added = add_branch(&mut fixture);
    let (_, added_trace) = build(&mut viewport, &branch_plan(&fixture), 1.0);
    let added_actions = actions(&added_trace);
    assert_eq!(
        added_actions[&fixture.root],
        RetainedSurfaceCompileAction::Reraster
    );
    assert_eq!(
        added_actions[&added],
        RetainedSurfaceCompileAction::Reraster
    );
    assert_eq!(
        added_actions[&fixture.branches[0]],
        RetainedSurfaceCompileAction::Reuse
    );
    assert_eq!(
        added_actions[&fixture.branches[1]],
        RetainedSurfaceCompileAction::Reuse
    );
    viewport.finish_retained_surface_transaction(true);
    assert_eq!(viewport.retained_surface_transaction_shape_for_test().0, 4);

    assert_eq!(
        fixture.arena.remove_subtree(added),
        2,
        "branch removal includes its content and detaches the parent edge",
    );
    fixture.branches.pop();
    fixture.branch_contents.pop();
    relayout_and_reapply_roles(&mut fixture);
    let (_, removed_trace) = build(&mut viewport, &branch_plan(&fixture), 1.0);
    let removed_actions = actions(&removed_trace);
    assert_eq!(
        removed_actions[&fixture.root],
        RetainedSurfaceCompileAction::Reraster
    );
    for branch in &fixture.branches {
        assert_eq!(removed_actions[branch], RetainedSurfaceCompileAction::Reuse);
    }
    viewport.finish_retained_surface_transaction(true);
    assert_eq!(viewport.retained_surface_transaction_shape_for_test().0, 3);
}

#[test]
fn branch_prepare_tamper_rejects_before_graph_pool_or_stage_mutation() {
    use BranchRole::{Effect, Transform};
    let fixture = branch_fixture(Transform, Effect, true, 0xf4_8800);
    let plan = branch_plan(&fixture);
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
