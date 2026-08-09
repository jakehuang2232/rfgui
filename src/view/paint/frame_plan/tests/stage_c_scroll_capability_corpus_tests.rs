//! Closed C0c input corpus for C1 classification and C2 reconstruction.
//!
//! The eleven cases are depth-four, nested scroll, branching siblings,
//! heterogeneous roots, co-located properties, clip crossing, interleaved
//! sibling order, named anchor, layout-position reference-scroll,
//! scroll-offset delta, and overlay-after. The first nine prevent a fixed
//! grammar or numeric-depth implementation; the last two prevent topology
//! coverage from standing in for scroll behavior.
//! `DepthFour` deliberately freezes the complete arena-independent property
//! snapshot input rather than claiming the legacy planner already admits it.
//! `NamedAnchor` refers to C0b's artifact-round-tripped
//! `named_anchor_projection_uses_canonical_reference_edge`; C1 must consume
//! that spatial input before it can claim anchor classification coverage.
//! C0c intentionally substitutes real native-forest and co-located planner
//! inputs for the earlier proposed name inventory over nine artifact-scroll
//! regression tests. The C0a name inventory still guards deletion/renaming;
//! these fixtures instead freeze the planner data C1 must actually consume.
//! Interleaved sibling order and named-anchor consumption are explicit corpus
//! additions, not aliases for that substituted inventory.
//!
//! Durable mapping for C2/C3:
//! - transform -> `SurfaceDagNodeKind::Transform`;
//! - effect -> `SurfaceDagNodeKind::Effect`;
//! - scroll -> `SurfaceDagNodeKind::ScrollContent` with host/content/overlay;
//! - clip, layout-position, and visual-offset -> no independent surface.
//!
//! A receiver is always a generic target ID, never a scroll-content grammar
//! variant. A clip crossing a scroll boundary preserves the outer clip and
//! rebases the local clip into detached-content space. `semantic_revision` is
//! non-spatial. The future Stage A transition differential must compute each
//! derived axis as `to[axis] - from[axis]`, in that order at `f32`, before its
//! bitwise comparison; reassociation is not semantic drift.

use super::*;
use crate::view::compositor::property_tree::LayoutPositionNodeId;
use crate::view::paint::scroll_scene::{
    ScrollSceneSingleTextureBudget, ValidatedScrollContentEffectScene,
    emit_prepared_retained_scroll_content_effect_scene,
    prepare_retained_scroll_content_effect_scene_from_pool,
};
use crate::view::paint::{
    PaintChunkRole, PropertyBoundaryDagCompiler, RetainedSurfaceRasterStamp,
    ValidatedPropertyBoundaryDagScene,
};

#[test]
fn stage_c_scroll_capability_case_set_is_closed() {
    assert_eq!(
        STAGE_C_SCROLL_CAPABILITY_CASES.map(StageCScrollCapabilityCase::label),
        [
            "depth-four",
            "nested-scroll",
            "branching-siblings",
            "heterogeneous-roots",
            "co-located-properties",
            "clip-crossing-scroll",
            "interleaved-sibling-order",
            "named-anchor",
            "layout-position-reference-scroll",
            "scroll-offset-delta",
            "overlay-phase",
        ],
    );
}

#[test]
fn stage_c_native_scroll_topologies_are_real_planner_inputs() {
    let (arena, roots, properties, generations) = native_scroll_forest_plan_fixture();
    let plan = plan_native_scroll_forest_scaffold_with_context(
        &arena,
        &roots,
        &properties,
        &generations,
        1.0,
        TransformSurfacePlanContext::default(),
    )
    .expect("C0c native scroll topology corpus");
    let forest = plan
        .native_scroll_forest_planning_scaffold()
        .expect("C0c native scroll forest");

    assert_eq!(forest.roots.len(), 2, "heterogeneous roots are one scene");
    let nested_depth = |mut boundary: NativeScrollBoundaryId| {
        let mut depth = 1usize;
        while let Some(parent) = forest.boundaries[boundary.0 as usize].parent {
            depth += 1;
            boundary = parent;
        }
        depth
    };
    assert!(
        forest
            .boundaries
            .iter()
            .map(|boundary| nested_depth(boundary.id))
            .max()
            .is_some_and(|depth| depth >= 3),
        "nested scroll is represented as data",
    );
    let children_of_one = forest
        .boundaries
        .iter()
        .filter(|boundary| boundary.parent == Some(NativeScrollBoundaryId(1)))
        .map(|boundary| boundary.id)
        .collect::<Vec<_>>();
    assert_eq!(
        children_of_one,
        [NativeScrollBoundaryId(2), NativeScrollBoundaryId(3)],
        "branching siblings remain ordered",
    );

    let parent = &forest.boundaries[1];
    let crossing = &forest.boundaries[2];
    assert_eq!(
        crossing.contents_clip.parent,
        Some(parent.contents_clip.id),
        "the outer clip remains explicit across the scroll boundary",
    );
    assert_eq!(
        crossing.projection.live_input.clip,
        Some(crossing.contents_clip.id)
    );
    assert_eq!(
        crossing.projection.projected_output, parent.projection.live_input,
        "the detached child projects into its parent scroll-content space",
    );

    let branch_program = &forest.programs[1];
    let child_positions = branch_program
        .content_steps
        .iter()
        .enumerate()
        .filter_map(|(index, step)| {
            matches!(step, NativeScrollForestContentProgramStep::ChildBoundary(_)).then_some(index)
        })
        .collect::<Vec<_>>();
    assert_eq!(child_positions.len(), 2);
    assert!(
        branch_program.content_steps[child_positions[0] + 1..child_positions[1]]
            .iter()
            .any(|step| matches!(step, NativeScrollForestContentProgramStep::Artifact(_))),
        "an ordinary sibling artifact remains ordered between detached child surfaces",
    );

    let (arena, root, properties, generations) = same_owner_transform_effect_scroll_roles_fixture();
    let plan = plan_property_scroll_interleave_scaffold_with_context(
        &arena,
        &[root],
        &properties,
        &generations,
        TransformSurfacePlanContext::default(),
    )
    .expect("C0c co-located T/E/S fixture");
    let scaffold = plan
        .property_scroll_planning_scaffold()
        .expect("C0c co-located scaffold");
    assert_eq!(
        scaffold.same_owner_transform_effect_scroll_insertions.len(),
        1,
        "co-located property families remain one explicit topology case",
    );
}

#[test]
fn stage_c_depth_four_scroll_chain_is_an_arena_independent_snapshot_fixture() {
    let (arena, _, wrapper, fourth, properties, _) = stage_c_depth_four_scroll_fixture();
    let fourth_admission = crate::view::test_support::get_element::<Element>(&arena, fourth)
        .exact_retained_scroll_forest_host_admission(fourth, &arena, 1.0)
        .expect("C0c fourth scroll host admission");
    let third = arena
        .find_by_stable_id(0x12f0_05)
        .expect("C0c third scroll owner");
    assert_eq!(arena.children_of(third), [wrapper]);
    assert_eq!(
        properties
            .node_state_for(third)
            .and_then(|state| state.descendants.scroll),
        Some(ScrollNodeId(third)),
        "the third host must remain a scroll boundary",
    );
    assert_eq!(
        properties
            .node_state_for(wrapper)
            .and_then(|state| state.descendants.scroll),
        Some(ScrollNodeId(third)),
        "the neutral wrapper must inherit the third scroll edge",
    );
    assert_eq!(
        properties
            .node_state_for(fourth)
            .and_then(|state| state.paint.scroll),
        Some(ScrollNodeId(third)),
        "the fourth host must receive the inherited scroll edge",
    );
    assert!(
        fourth_admission.matches_scroll_node(
            properties
                .scroll_snapshot_for(ScrollNodeId(fourth))
                .expect("C0c fourth scroll snapshot"),
        )
    );
    let chain = properties
        .scroll_snapshot_chain_for(Some(ScrollNodeId(fourth)))
        .expect("C0c complete depth-four scroll snapshot chain");
    assert_eq!(chain.len(), 4, "depth is data, not a grammar variant");
    assert_eq!(chain[0].id, ScrollNodeId(fourth));
    assert!(
        chain
            .windows(2)
            .all(|pair| pair[0].parent == Some(pair[1].id))
    );
    assert!(chain.last().is_some_and(|root| root.parent.is_none()));
}

fn plan_scroll_effect(
    arena: &NodeArena,
    root: NodeKey,
    properties: &PropertyTrees,
    generations: &PaintGenerationTracker,
) -> ValidatedScrollContentEffectScene {
    let scene = PropertyBoundaryDagCompiler::plan_and_validate(
        arena,
        &[root],
        properties,
        generations,
        1.0,
        [0.0; 2],
        None,
        crate::time::Instant::now(),
        wgpu::TextureFormat::Bgra8UnormSrgb,
        ScrollSceneSingleTextureBudget::new(8192, 128 * 1024 * 1024)
            .expect("C0c non-zero scroll budget"),
    )
    .expect("C0c scroll-effect capability fixture");
    match scene {
        ValidatedPropertyBoundaryDagScene::ScrollEffect(scene) => scene,
        _ => panic!("C0c offset differential requires one S -> E root"),
    }
}

fn commit_scroll_effect_baseline(
    viewport: &mut Viewport,
    scene: ValidatedScrollContentEffectScene,
) -> Vec<RetainedSurfaceRasterStamp> {
    let owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut graph = FrameGraph::new();
    let prepared = prepare_retained_scroll_content_effect_scene_from_pool(
        viewport,
        scene,
        &mut graph,
        UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
        [0.0; 4],
        owner,
    )
    .expect("C0c baseline prepare");
    let observations = prepared.effect_content_observations_for_test();
    assert_eq!(observations.len(), 1, "C0c fixture has one retained root");
    let stamps = observations[0]
        .iter()
        .map(|(_, stamp)| stamp.clone())
        .collect::<Vec<_>>();
    let _ = emit_prepared_retained_scroll_content_effect_scene(prepared);
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));
    stamps
}

#[test]
fn stage_c_layout_position_reference_scroll_is_a_real_placement_edge() {
    let (arena, root, child, properties, _) = stage_c_layout_position_reference_scroll_fixture();
    {
        let node = arena.get(root).expect("C0c reference-scroll root");
        assert_eq!(node.children(), node.element.children());
        let observation = node.element.scroll_geometry_observation(root, &arena);
        let snapshot = node.element.box_model_snapshot();
        let element = node
            .element
            .as_any()
            .downcast_ref::<Element>()
            .expect("C0c reference-scroll Element");
        let layout = &element.layout_state;
        assert!(
            matches!(
                observation,
                crate::view::base_component::ScrollGeometryObservation::Exact(_)
            ),
            "C0c reference-scroll geometry: {observation:?}; box={snapshot:?}; layout={layout:?}",
        );
    }

    assert_eq!(
        properties
            .layout_positions
            .get(&LayoutPositionNodeId(child))
            .expect("C0c child layout-position node")
            .reference_scroll,
        Some(ScrollNodeId(root)),
        "the production placement edge must name the scroll it subtracts",
    );
    assert_eq!(
        properties
            .scroll_snapshot_for(ScrollNodeId(root))
            .expect("C0c reference scroll snapshot")
            .offset,
        glam::Vec2::new(0.0, 20.0),
    );
}

/// C0c's defining scroll differential uses the same arena and resident pool.
/// A live offset delta must preserve both detached raster stamps and select
/// `Reuse`; an independent in-place paint mutation must change the content
/// stamp and select `Reraster`.
#[test]
fn stage_c_scroll_offset_delta_is_composition_only_and_not_content_change() {
    let (arena, root, mut properties, mut generations) =
        scroll_content_effect_interleave_fixture(false, true);
    let baseline = plan_scroll_effect(&arena, root, &properties, &generations);
    let scroll = root;
    let mut viewport = Viewport::new();
    let baseline_stamps = commit_scroll_effect_baseline(&mut viewport, baseline);
    let baseline_offset =
        apply_stage_c_scroll_offset_delta(&arena, scroll, &mut properties, &mut generations, 7.0);

    let moved = plan_scroll_effect(&arena, root, &properties, &generations);
    assert_ne!(
        properties
            .scroll_snapshot_for(ScrollNodeId(scroll))
            .expect("C0c moved scroll snapshot")
            .offset,
        baseline_offset,
    );

    let owner = viewport.begin_retained_surface_frame_stage().unwrap();
    let mut graph = FrameGraph::new();
    let mut prepared = prepare_retained_scroll_content_effect_scene_from_pool(
        &mut viewport,
        moved,
        &mut graph,
        UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
        [0.0; 4],
        owner,
    )
    .expect("C0c moved prepare");
    let moved_observations =
        prepared.refresh_effect_content_observations_from_committed_pool_for_test();
    let [moved_root] = moved_observations.as_slice() else {
        panic!("C0c moved fixture must retain one root")
    };
    assert_eq!(
        moved_root[0],
        (
            RetainedSurfaceCompileAction::Reuse,
            baseline_stamps[0].clone()
        ),
        "scroll offset must preserve and reuse the effect raster stamp",
    );
    assert_eq!(
        moved_root[1],
        (
            RetainedSurfaceCompileAction::Reuse,
            baseline_stamps[1].clone()
        ),
        "scroll offset must preserve and reuse the detached content raster stamp",
    );
    let _ = emit_prepared_retained_scroll_content_effect_scene(prepared);
    assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));

    let (content_arena, content_root, mut content_properties, mut content_generations) =
        scroll_content_effect_interleave_fixture(false, true);
    let content_baseline = plan_scroll_effect(
        &content_arena,
        content_root,
        &content_properties,
        &content_generations,
    );
    let mut content_viewport = Viewport::new();
    let content_baseline_stamps =
        commit_scroll_effect_baseline(&mut content_viewport, content_baseline);
    let wrapper = content_arena
        .find_by_stable_id(0xb4_3011)
        .expect("C0c content wrapper");
    crate::view::test_support::get_element_mut::<Element>(&content_arena, wrapper)
        .set_background_color_value(Color::rgb(36, 12, 24));
    content_arena.refresh_subtree_dirty_cache(content_root);
    content_properties.sync(&content_arena, &[content_root]);
    content_generations.sync(&content_arena, &[content_root], &content_properties);
    let changed = plan_scroll_effect(
        &content_arena,
        content_root,
        &content_properties,
        &content_generations,
    );
    let owner = content_viewport
        .begin_retained_surface_frame_stage()
        .unwrap();
    let mut graph = FrameGraph::new();
    let mut prepared = prepare_retained_scroll_content_effect_scene_from_pool(
        &mut content_viewport,
        changed,
        &mut graph,
        UiBuildContext::new(640, 480, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
        [0.0; 4],
        owner,
    )
    .expect("C0c content-change prepare");
    let changed_observations =
        prepared.refresh_effect_content_observations_from_committed_pool_for_test();
    let [changed_root] = changed_observations.as_slice() else {
        panic!("C0c content-change fixture must retain one root")
    };
    assert_eq!(
        changed_root[0],
        (
            RetainedSurfaceCompileAction::Reuse,
            content_baseline_stamps[0].clone(),
        ),
        "an in-place content mutation must not invalidate the effect surface",
    );
    assert_ne!(changed_root[1].1, content_baseline_stamps[1]);
    assert_eq!(
        changed_root[1].1.identity, content_baseline_stamps[1].identity,
        "content mutation preserves the resident key while changing raster input",
    );
    assert_eq!(
        changed_root[1].0,
        RetainedSurfaceCompileAction::Reraster,
        "content mutation must reraster the detached content surface",
    );
    let _ = emit_prepared_retained_scroll_content_effect_scene(prepared);
    assert!(content_viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));
}

/// The corpus names all three scroll phases, so the visible overlay case must
/// measure an actual terminal overlay artifact rather than metadata alone.
#[test]
fn stage_c_visible_overlay_is_terminal_after_detached_content() {
    let (arena, roots, properties, generations) = stage_c_visible_overlay_fixture();
    let scroll = arena
        .find_by_stable_id(0x12f0_01)
        .expect("C0c visible overlay scroll host");
    let plan = plan_native_scroll_forest_scaffold_with_context(
        &arena,
        &roots,
        &properties,
        &generations,
        1.0,
        TransformSurfacePlanContext::default(),
    )
    .expect("C0c visible overlay plan");
    let forest = plan
        .native_scroll_forest_planning_scaffold()
        .expect("C0c visible overlay forest");
    let boundary = forest
        .boundaries
        .iter()
        .find(|boundary| boundary.boundary_root == scroll)
        .expect("C0c visible overlay boundary")
        .id;
    let overlay = forest.programs[boundary.0 as usize]
        .overlay_after
        .artifact();
    assert!(!overlay.ops.is_empty(), "visible overlay must paint");
    assert!(
        overlay
            .chunks
            .iter()
            .all(|chunk| chunk.id.phase == PaintNodePhase::AfterChildren),
        "overlay artifact must contain only after-children chunks",
    );
    assert!(
        overlay
            .chunks
            .iter()
            .any(|chunk| chunk.id.role == PaintChunkRole::ScrollbarOverlay),
        "visible overlay must contain a real scrollbar chunk",
    );
    assert_eq!(
        overlay.chunks.last().map(|chunk| chunk.op_range.end),
        Some(overlay.ops.len()),
        "terminal overlay chunk must consume the artifact exactly",
    );

    let host_index = forest
        .schedule
        .steps
        .iter()
        .position(|step| {
            matches!(
                step,
                NativeScrollForestScheduledStep::Artifact {
                    boundary: owner,
                    phase: NativeScrollArtifactPhase::HostBefore,
                } if *owner == boundary
            )
        })
        .expect("C0c host phase");
    let overlay_index = forest
        .schedule
        .steps
        .iter()
        .position(|step| {
            matches!(
                step,
                NativeScrollForestScheduledStep::Artifact {
                    boundary: owner,
                    phase: NativeScrollArtifactPhase::OverlayAfter,
                } if *owner == boundary
            )
        })
        .expect("C0c overlay phase");
    assert!(
        forest.schedule.steps[host_index + 1..overlay_index]
            .iter()
            .any(|step| matches!(
                step,
                NativeScrollForestScheduledStep::ChildBoundary {
                    parent: Some(owner),
                    ..
                } if *owner == boundary
            )),
        "detached content traversal must remain between host and overlay",
    );
    assert!(
        forest.schedule.steps[overlay_index + 1..]
            .first()
            .is_none_or(|step| matches!(
                step,
                NativeScrollForestScheduledStep::ChildBoundary { parent: None, .. }
            )),
        "overlay must close the boundary before the next scene root",
    );
}
