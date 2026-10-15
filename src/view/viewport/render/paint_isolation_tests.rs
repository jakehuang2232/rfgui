//! A node whose paint inputs are incomplete is left out of the frame with
//! its subtree, and the rest of the frame still paints.

use rustc_hash::FxHashSet;

use crate::style::{Color, Layout, Length, ParsedValue, PropertyId, Style};
use crate::view::base_component::{Element, LayoutConstraints, LayoutPlacement, UiBuildContext};
use crate::view::compositor::property_tree::LayoutPositionNodeId;
use crate::view::compositor::{PaintGenerationTracker, PropertyTrees};
use crate::view::node_arena::{NodeArena, NodeKey};
use crate::view::paint::{PlanningCache, RecordingCache};
use crate::view::test_support::{
    commit_child, commit_element, get_element_mut, measure_and_place, new_test_arena,
};

use super::{
    ARTIFACT_SURFACE_AGGREGATE_BUDGET_BYTES, RecordedFrame, artifact_surface_raster_context,
    record_frame,
};

fn sized(id: u64, size: f32, background: Option<Color>) -> Element {
    let mut element = Element::new_with_id(id, 0.0, 0.0, size, size);
    let mut style = Style::new();
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(size)));
    style.insert(PropertyId::Height, ParsedValue::Length(Length::px(size)));
    style.insert(
        PropertyId::Layout,
        ParsedValue::Layout(Layout::flow().column().into()),
    );
    if let Some(color) = background {
        style.insert(PropertyId::BackgroundColor, ParsedValue::color_like(color));
    }
    element.apply_style(style);
    element
}

/// A root without paint of its own over four painted boxes: `a`, `b` holding
/// `c`, and `d`. Returns the root and `[a, b, c, d]`.
fn scene(arena: &mut NodeArena) -> (NodeKey, [NodeKey; 4]) {
    let root = commit_element(arena, Box::new(sized(1, 300.0, None)));
    let color = Some(Color::rgb(30, 90, 200));
    let a = commit_child(arena, root, Box::new(sized(2, 40.0, color)));
    let b = commit_child(arena, root, Box::new(sized(3, 40.0, color)));
    let c = commit_child(arena, b, Box::new(sized(4, 20.0, color)));
    let d = commit_child(arena, root, Box::new(sized(5, 40.0, color)));
    (root, [a, b, c, d])
}

fn lay_out(arena: &mut NodeArena, root: NodeKey) {
    let constraints = LayoutConstraints {
        max_width: 320.0,
        max_height: 320.0,
        viewport_width: 320.0,
        viewport_height: 320.0,
        percent_base_width: Some(320.0),
        percent_base_height: Some(320.0),
    };
    let placement = LayoutPlacement {
        parent_x: 0.0,
        parent_y: 0.0,
        visual_offset_x: 0.0,
        visual_offset_y: 0.0,
        available_width: 320.0,
        available_height: 320.0,
        viewport_width: 320.0,
        viewport_height: 320.0,
        percent_base_width: Some(320.0),
        percent_base_height: Some(320.0),
    };
    measure_and_place(arena, root, constraints, placement);
}

fn synced(arena: &NodeArena, root: NodeKey) -> (PropertyTrees, PaintGenerationTracker) {
    let mut properties = PropertyTrees::default();
    properties.sync(arena, &[root]);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(arena, &[root], &properties);
    (properties, generations)
}

/// The caches a viewport keeps across frames.
#[derive(Default)]
struct Caches {
    recording: RecordingCache,
    planning: PlanningCache,
}

fn record(
    arena: &NodeArena,
    root: NodeKey,
    properties: &PropertyTrees,
    generations: &PaintGenerationTracker,
    caches: &mut Caches,
) -> RecordedFrame {
    let ctx = UiBuildContext::new(320, 320, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let raster_context = artifact_surface_raster_context(
        &ctx,
        wgpu::Limits::default().max_texture_dimension_2d,
        ARTIFACT_SURFACE_AGGREGATE_BUDGET_BYTES,
    );
    record_frame(
        arena,
        &[root],
        properties,
        generations,
        raster_context,
        Some(&mut caches.recording),
        Some(&mut caches.planning),
    )
    .expect("the frame plans")
}

fn assert_left_out(recorded: &RecordedFrame, expected: &[NodeKey]) {
    assert_eq!(
        recorded.left_out,
        expected.iter().copied().collect::<FxHashSet<_>>()
    );
    // The root, `a` and `d` still paint.
    assert_eq!(recorded.chunk_count, 3);
}

#[test]
fn a_transform_without_a_finite_matrix_leaves_out_only_its_subtree() {
    let mut arena = new_test_arena();
    let (root, [_, b, c, d]) = scene(&mut arena);
    lay_out(&mut arena, root);
    get_element_mut::<Element>(&arena, b)
        .set_resolved_transform_for_test(Some(glam::Mat4::from_scale(glam::Vec3::NAN)));
    // `d` keeps its own transform's projection.
    get_element_mut::<Element>(&arena, d).set_resolved_transform_for_test(Some(
        glam::Mat4::from_translation(glam::Vec3::new(5.0, 0.0, 0.0)),
    ));
    let (properties, generations) = synced(&arena, root);
    assert_left_out(
        &record(
            &arena,
            root,
            &properties,
            &generations,
            &mut Caches::default(),
        ),
        &[b, c],
    );
}

#[test]
fn a_node_the_property_sync_missed_is_left_out_with_its_subtree() {
    let mut arena = new_test_arena();
    let (root, [_, b, _, _]) = scene(&mut arena);
    lay_out(&mut arena, root);
    let (mut properties, generations) = synced(&arena, root);
    properties.states.remove(&b);
    assert_left_out(
        &record(
            &arena,
            root,
            &properties,
            &generations,
            &mut Caches::default(),
        ),
        &[b],
    );
}

#[test]
fn a_node_the_plan_cannot_place_is_left_out_with_its_subtree() {
    let mut arena = new_test_arena();
    let (root, [_, b, _, _]) = scene(&mut arena);
    lay_out(&mut arena, root);
    let (mut properties, generations) = synced(&arena, root);
    // The recording takes the placement as it is; planning rejects it.
    properties
        .layout_positions
        .get_mut(&LayoutPositionNodeId(b))
        .expect("b is placed")
        .translation_at_scroll_zero = glam::Vec2::NAN;
    assert_left_out(
        &record(
            &arena,
            root,
            &properties,
            &generations,
            &mut Caches::default(),
        ),
        &[b],
    );
}

#[test]
fn a_left_out_node_paints_again_once_its_inputs_are_fixed() {
    let mut arena = new_test_arena();
    let (root, [_, b, c, _]) = scene(&mut arena);
    lay_out(&mut arena, root);
    let set_transform = |arena: &NodeArena, matrix| {
        get_element_mut::<Element>(arena, b).set_resolved_transform_for_test(Some(matrix));
    };
    let mut caches = Caches::default();
    set_transform(&arena, glam::Mat4::from_scale(glam::Vec3::NAN));
    for _ in 0..2 {
        let (properties, generations) = synced(&arena, root);
        assert_left_out(
            &record(&arena, root, &properties, &generations, &mut caches),
            &[b, c],
        );
    }
    // The second frame reused what the first one recorded.
    assert!(caches.recording.hits > 0);
    set_transform(&arena, glam::Mat4::IDENTITY);
    let (properties, generations) = synced(&arena, root);
    let recorded = record(&arena, root, &properties, &generations, &mut caches);
    assert!(recorded.left_out.is_empty());
    assert_eq!(recorded.chunk_count, 5);
}
