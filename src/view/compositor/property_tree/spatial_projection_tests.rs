use super::*;

use crate::style::{
    Anchor, Layout, Length, Padding, ParsedValue, Position, PropertyId, Scale, ScrollDirection,
    Style, Transform, TransformOrigin, Transition, TransitionProperty, Transitions,
};
use crate::view::base_component::{Element, LayoutConstraints, LayoutPlacement};
use crate::view::test_support::{
    commit_child, commit_element, get_element_mut, measure_and_place, new_test_arena,
};

fn constraints() -> LayoutConstraints {
    LayoutConstraints {
        max_width: 400.0,
        max_height: 300.0,
        viewport_width: 400.0,
        viewport_height: 300.0,
        percent_base_width: Some(400.0),
        percent_base_height: Some(300.0),
    }
}

fn placement() -> LayoutPlacement {
    LayoutPlacement {
        parent_x: 0.0,
        parent_y: 0.0,
        visual_offset_x: 0.0,
        visual_offset_y: 0.0,
        available_width: 400.0,
        available_height: 300.0,
        viewport_width: 400.0,
        viewport_height: 300.0,
        percent_base_width: Some(400.0),
        percent_base_height: Some(300.0),
    }
}

fn transformed_element(id: u64, x: f32, y: f32, width: f32, height: f32) -> Element {
    let mut element = Element::new_with_id(id, x, y, width, height);
    let mut style = Style::new();
    style.set_transform(Transform::new([Scale::uniform(1.25)]));
    style.set_transform_origin(TransformOrigin::px(7.0, 11.0));
    element.apply_style(style);
    element
}

fn matrix_bits(matrix: Mat4) -> [u32; 16] {
    matrix.to_cols_array().map(f32::to_bits)
}

fn projection_scroll_node(owner: NodeKey, offset: Vec2) -> ScrollNode {
    ScrollNode {
        owner,
        parent: None,
        offset,
        configured_axis: ScrollAxisSnapshot::Vertical,
        viewport: Rect {
            x: 0.0,
            y: 0.0,
            width: 180.0,
            height: 100.0,
        },
        content_size: Size {
            width: 180.0,
            height: 300.0,
        },
        layout_content_bounds_at_zero: Rect {
            x: 0.0,
            y: 0.0,
            width: 180.0,
            height: 300.0,
        },
        scrollbar_overlay: ScrollbarOverlayWitness {
            vertical_track: None,
            vertical_thumb: None,
            horizontal_track: None,
            horizontal_thumb: None,
            interaction: crate::view::base_component::ScrollbarInteractionWitness {
                hovered: false,
                dragging_axis: None,
                has_interaction_timestamp: false,
            },
            paint_state: ScrollbarPaintStateWitness::NotPaintable,
            sampled_alpha: 0.0,
            shadow_blur_radius: 0.0,
        },
        contents_clip: ScrollContentsClipWitness::ExactRect([0, 0, 180, 100]),
        generation: 1,
    }
}

#[test]
fn transition_visual_offset_changes_only_the_composite_spatial_edge() {
    let mut element = transformed_element(0xb1a0, 40.0, 30.0, 80.0, 50.0);
    let mut style = Style::new();
    style.insert(
        PropertyId::Transition,
        ParsedValue::Transition(Transitions::single(Transition::new(
            TransitionProperty::PositionX,
            200,
        ))),
    );
    element.apply_style(style);

    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(element));
    measure_and_place(&mut arena, root, constraints(), placement());

    let mut trees = PropertyTrees::default();
    trees.sync(&arena, &[root]);
    let transform_id = TransformNodeId(root);
    let position_id = LayoutPositionNodeId(root);
    let visual_id = VisualOffsetNodeId(root);
    let first_transform = trees.transforms[&transform_id];
    let first_position = trees.layout_positions[&position_id];
    let first_visual = trees.visual_offsets[&visual_id];

    get_element_mut::<Element>(&arena, root).set_layout_transition_x(18.0);
    measure_and_place(&mut arena, root, constraints(), placement());
    trees.sync(&arena, &[root]);

    let next_transform = trees.transforms[&transform_id];
    let next_position = trees.layout_positions[&position_id];
    let next_visual = trees.visual_offsets[&visual_id];
    assert_eq!(
        next_position.generation, first_position.generation,
        "transition animation must not dirty scroll-zero layout placement",
    );
    assert_eq!(
        next_position.translation_at_scroll_zero,
        first_position.translation_at_scroll_zero,
    );
    assert!(next_visual.generation > first_visual.generation);
    assert_eq!(next_visual.offset, Vec2::new(18.0, 0.0));
    assert_eq!(
        next_transform.local_generation, first_transform.local_generation,
        "composition-only visual motion must preserve local transform identity",
    );
    assert_eq!(
        matrix_bits(next_transform.local_matrix),
        matrix_bits(first_transform.local_matrix),
    );
    assert_eq!(
        next_transform.local_origin.to_array().map(f32::to_bits),
        first_transform.local_origin.to_array().map(f32::to_bits),
    );
    assert_ne!(
        matrix_bits(
            next_transform
                .derived_projection
                .expect("next derived projection")
                .owner_viewport_transform,
        ),
        matrix_bits(
            first_transform
                .derived_projection
                .expect("first derived projection")
                .owner_viewport_transform,
        ),
        "the derived composite projection follows the animation",
    );
}

#[test]
fn scroll_changes_viewport_projection_without_dirtying_the_child_layout_edge() {
    let mut root_element = Element::new_with_id(0xb1b0, 0.0, 0.0, 160.0, 90.0);
    let mut root_style = Style::new();
    root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    root_style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(ScrollDirection::Vertical),
    );
    root_element.apply_style(root_style);

    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(root_element));
    let child = commit_child(
        &mut arena,
        root,
        Box::new(transformed_element(0xb1b1, 9.0, 70.0, 120.0, 180.0)),
    );
    measure_and_place(&mut arena, root, constraints(), placement());

    let source_spatial = arena
        .get(child)
        .expect("child")
        .element
        .compositor_spatial_placement_snapshot()
        .expect("child spatial placement");
    assert_eq!(
        source_spatial.reference(),
        SpatialPositionReferenceSnapshot::LayoutParent(None),
    );

    let mut trees = PropertyTrees::default();
    trees.sync(&arena, &[root]);
    trees.scrolls.insert(
        ScrollNodeId(root),
        projection_scroll_node(root, Vec2::ZERO),
    );
    trees.refresh_derived_spatial_projections();
    let position_id = LayoutPositionNodeId(child);
    let transform_id = TransformNodeId(child);
    let first_position = trees.layout_positions[&position_id];
    assert_eq!(
        first_position.reference,
        SpatialPositionReference::LayoutParent(Some(root)),
    );
    let first_transform = trees.transforms[&transform_id];

    arena
        .get_mut(root)
        .expect("scroll root")
        .element
        .set_scroll_offset((0.0, 24.0));
    measure_and_place(&mut arena, root, constraints(), placement());
    trees.sync(&arena, &[root]);
    trees.scrolls.insert(
        ScrollNodeId(root),
        projection_scroll_node(root, Vec2::new(0.0, 24.0)),
    );
    trees.refresh_derived_spatial_projections();

    let next_position = trees.layout_positions[&position_id];
    let next_transform = trees.transforms[&transform_id];
    assert_eq!(next_position.generation, first_position.generation);
    assert_eq!(
        next_position.translation_at_scroll_zero, first_position.translation_at_scroll_zero,
        "the edge must come from the pre-scroll relative target, not layout_flow_position",
    );
    assert_eq!(
        next_transform.local_generation,
        first_transform.local_generation,
    );
    assert_ne!(
        matrix_bits(
            next_transform
                .derived_projection
                .expect("next derived projection")
                .owner_viewport_transform,
        ),
        matrix_bits(
            first_transform
                .derived_projection
                .expect("first derived projection")
                .owner_viewport_transform,
        ),
    );
}

#[test]
fn local_transform_composes_after_layout_and_visual_position() {
    let mut element = transformed_element(0xb1c0, 25.0, 35.0, 80.0, 50.0);
    let mut style = Style::new();
    style.insert(
        PropertyId::Transition,
        ParsedValue::Transition(Transitions::single(Transition::new(
            TransitionProperty::Position,
            200,
        ))),
    );
    element.apply_style(style);

    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(element));
    measure_and_place(&mut arena, root, constraints(), placement());
    get_element_mut::<Element>(&arena, root).set_layout_transition_x(13.0);
    measure_and_place(&mut arena, root, constraints(), placement());

    let mut trees = PropertyTrees::default();
    trees.sync(&arena, &[root]);
    let transform = trees.transforms[&TransformNodeId(root)];
    let spatial = arena
        .get(root)
        .expect("root")
        .element
        .compositor_spatial_placement_snapshot()
        .expect("spatial placement");
    let viewport_position = spatial.compatibility_viewport_position();
    let origin_world = glam::Vec3::new(
        viewport_position[0] + transform.local_origin.x,
        viewport_position[1] + transform.local_origin.y,
        transform.local_origin.z,
    );
    let recomposed = crate::view::base_component::compose_transform_about_origin(
        transform.local_matrix,
        origin_world,
    );
    assert_eq!(
        matrix_bits(recomposed),
        matrix_bits(
            transform
                .derived_projection
                .expect("derived projection")
                .owner_viewport_transform,
        ),
        "visual position is applied after layout placement and before transform conjugation",
    );

    let mut viewport_child = transformed_element(0xb1c1, 0.0, 0.0, 30.0, 20.0);
    let mut viewport_style = Style::new();
    viewport_style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .anchor(Anchor::Viewport)
                .left(Length::px(12.0))
                .top(Length::px(14.0)),
        ),
    );
    viewport_child.apply_style(viewport_style);
    let child = commit_child(&mut arena, root, Box::new(viewport_child));
    measure_and_place(&mut arena, root, constraints(), placement());
    trees.sync(&arena, &[root]);
    assert_eq!(
        trees.layout_positions[&LayoutPositionNodeId(child)].reference,
        SpatialPositionReference::Viewport,
    );
    assert_eq!(
        trees.visual_offsets[&VisualOffsetNodeId(child)].parent,
        Some(VisualOffsetNodeId(root)),
        "viewport layout anchoring must not erase arena-parent visual inheritance",
    );
    let state = trees.states[&child].paint;
    let transforms = trees
        .transform_snapshot_chain_for(state.transform)
        .expect("transform snapshots");
    let positions = trees
        .layout_position_snapshot_chain_for(state.layout_position)
        .expect("layout-position snapshots");
    let visuals = trees
        .visual_offset_snapshot_chain_for(state.visual_offset)
        .expect("visual-offset snapshots");
    let graph = SpatialProjectionGraph::try_new(&transforms, &positions, &visuals, &[])
        .expect("viewport-anchored graph");
    let child_transform = transforms
        .iter()
        .find(|snapshot| snapshot.id == TransformNodeId(child))
        .expect("child transform");
    assert_eq!(
        matrix_bits(
            graph
                .derive_owner_viewport_transform(child_transform.id)
                .expect("viewport-anchored projection")
                .owner_viewport_transform,
        ),
        matrix_bits(child_transform.owner_viewport_transform),
    );
}

#[test]
fn property_state_transition_keeps_layout_and_visual_dimensions_distinct() {
    let mut arena = new_test_arena();
    let from_owner = commit_element(
        &mut arena,
        Box::new(Element::new_with_id(0xb1d0, 0.0, 0.0, 10.0, 10.0)),
    );
    let to_owner = commit_element(
        &mut arena,
        Box::new(Element::new_with_id(0xb1d1, 0.0, 0.0, 10.0, 10.0)),
    );
    let from = PropertyTreeState {
        layout_position: Some(LayoutPositionNodeId(from_owner)),
        visual_offset: Some(VisualOffsetNodeId(from_owner)),
        ..PropertyTreeState::default()
    };
    let to = PropertyTreeState {
        layout_position: Some(LayoutPositionNodeId(to_owner)),
        visual_offset: Some(VisualOffsetNodeId(from_owner)),
        ..PropertyTreeState::default()
    };

    let transition = PropertyStateTransition::between(from, to);
    assert!(transition.layout_position.is_changed());
    assert!(!transition.visual_offset.is_changed());
    assert!(!transition.transform.is_changed());
    assert_eq!(
        from.legacy_boundary_dimensions(),
        to.legacy_boundary_dimensions(),
        "only the explicitly named legacy grammar projection may erase spatial identities",
    );
}

#[test]
fn spatial_snapshots_are_bitwise_and_chains_are_transitively_complete() {
    let mut root_element = transformed_element(0xb1e0, 0.0, 0.0, 180.0, 100.0);
    let mut root_style = Style::new();
    root_style.set_transform(Transform::new([Scale::uniform(1.1)]));
    root_style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(ScrollDirection::Vertical),
    );
    root_element.apply_style(root_style);

    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(root_element));
    let child = commit_child(
        &mut arena,
        root,
        Box::new(transformed_element(0xb1e1, 5.0, 6.0, 80.0, 140.0)),
    );
    let grandchild = commit_child(
        &mut arena,
        child,
        Box::new(Element::new_with_id(0xb1e2, 2.0, 3.0, 20.0, 20.0)),
    );
    measure_and_place(&mut arena, root, constraints(), placement());
    get_element_mut::<Element>(&arena, root)
        .layout_state
        .content_size
        .height = 300.0;

    let mut trees = PropertyTrees::default();
    trees.sync(&arena, &[root]);
    trees.scrolls.insert(
        ScrollNodeId(root),
        projection_scroll_node(root, Vec2::ZERO),
    );
    trees.refresh_derived_spatial_projections();
    let state = trees.states[&grandchild].paint;
    assert_eq!(
        trees
            .transform_snapshot_chain_for(state.transform)
            .unwrap_or_else(|| {
                panic!(
                    "transform snapshot chain: {:?}",
                    trees.spatial_validation_errors
                )
            })
            .iter()
            .map(|snapshot| snapshot.id)
            .collect::<Vec<_>>(),
        vec![TransformNodeId(child), TransformNodeId(root)],
    );
    assert_eq!(
        trees
            .layout_position_snapshot_chain_for(state.layout_position)
            .expect("layout-position snapshot chain")
            .len(),
        3,
    );
    assert_eq!(
        trees
            .visual_offset_snapshot_chain_for(state.visual_offset)
            .expect("visual-offset snapshot chain")
            .len(),
        3,
    );
    let position = trees
        .layout_position_snapshot_for(LayoutPositionNodeId(grandchild))
        .expect("layout-position snapshot");
    let mut negative_zero_position = position;
    negative_zero_position.translation_at_scroll_zero.x = -0.0;
    let mut positive_zero_position = position;
    positive_zero_position.translation_at_scroll_zero.x = 0.0;
    assert_ne!(positive_zero_position, negative_zero_position);

    let transform = trees
        .transform_snapshot_for(TransformNodeId(child))
        .expect("transform snapshot");
    let mut negative_zero_transform = transform;
    negative_zero_transform.local_origin.z = -0.0;
    let mut positive_zero_transform = transform;
    positive_zero_transform.local_origin.z = 0.0;
    assert_ne!(positive_zero_transform, negative_zero_transform);
}

#[test]
fn interleaved_projection_matches_owner_compatibility_matrix() {
    let mut root_element = transformed_element(0xb1f0, 20.0, 18.0, 180.0, 100.0);
    let mut root_style = Style::new();
    root_style.set_transform(Transform::new([Scale::uniform(1.1)]));
    root_style.set_padding(Padding::uniform(Length::px(8.0)));
    root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    root_style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(ScrollDirection::Vertical),
    );
    root_style.insert(
        PropertyId::Transition,
        ParsedValue::Transition(Transitions::single(Transition::new(
            TransitionProperty::PositionX,
            200,
        ))),
    );
    root_element.apply_style(root_style);

    let mut child_element = transformed_element(0xb1f1, 9.0, 70.0, 120.0, 180.0);
    let mut child_style = Style::new();
    child_style.insert(
        PropertyId::Transition,
        ParsedValue::Transition(Transitions::single(Transition::new(
            TransitionProperty::PositionY,
            200,
        ))),
    );
    child_element.apply_style(child_style);

    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(root_element));
    let child = commit_child(&mut arena, root, Box::new(child_element));
    measure_and_place(&mut arena, root, constraints(), placement());
    get_element_mut::<Element>(&arena, root).set_layout_transition_x(6.0);
    get_element_mut::<Element>(&arena, child).set_layout_transition_y(4.0);
    arena
        .get_mut(root)
        .expect("scroll root")
        .element
        .set_scroll_offset((0.0, 24.0));
    measure_and_place(&mut arena, root, constraints(), placement());

    let mut trees = PropertyTrees::default();
    trees.sync(&arena, &[root]);
    trees.scrolls.insert(
        ScrollNodeId(root),
        projection_scroll_node(root, Vec2::new(0.0, 24.0)),
    );
    trees.refresh_derived_spatial_projections();
    let state = trees.states[&child].paint;
    let transforms = trees
        .transform_snapshot_chain_for(state.transform)
        .unwrap_or_else(|| {
            panic!(
                "transform snapshots: {:?}",
                trees.spatial_validation_errors
            )
        });
    let positions = trees
        .layout_position_snapshot_chain_for(state.layout_position)
        .expect("layout-position snapshots");
    let visuals = trees
        .visual_offset_snapshot_chain_for(state.visual_offset)
        .expect("visual-offset snapshots");
    let scrolls = trees
        .scroll_snapshot_chain_for(Some(ScrollNodeId(root)))
        .expect("scroll snapshots");
    let graph = SpatialProjectionGraph::try_new(&transforms, &positions, &visuals, &scrolls)
        .expect("complete spatial graph");

    assert_eq!(
        positions
            .iter()
            .find(|snapshot| snapshot.owner == root)
            .expect("root position")
            .child_reference_offset_at_scroll_zero,
        Vec2::splat(8.0),
        "the parent content reference must be frozen explicitly",
    );
    for snapshot in &transforms {
        let derived = graph
            .derive_owner_viewport_transform(snapshot.id)
            .expect("derived owner projection");
        assert_eq!(
            matrix_bits(derived.owner_viewport_transform),
            matrix_bits(snapshot.owner_viewport_transform),
            "the arena-independent graph must reproduce the sealed owner projection bitwise: owner={:?} derived_position={:?} scrolls={:?}",
            snapshot.owner,
            derived.owner_viewport_position,
            scrolls
                .iter()
                .map(|scroll| (scroll.owner, scroll.offset))
                .collect::<Vec<_>>(),
        );
        let owner = arena.get(snapshot.owner).expect("projection owner");
        let element = owner
            .element
            .as_any()
            .downcast_ref::<Element>()
            .expect("Element projection owner");
        assert_eq!(
            derived.owner_viewport_position.to_array().map(f32::to_bits),
            [
                element.layout_state.layout_position.x.to_bits(),
                element.layout_state.layout_position.y.to_bits(),
            ],
        );
    }
}

#[test]
fn interleaved_projection_fails_closed_for_incomplete_and_anchor_graphs() {
    let mut arena = new_test_arena();
    let root = commit_element(
        &mut arena,
        Box::new(transformed_element(0xb200, 0.0, 0.0, 80.0, 50.0)),
    );
    let child = commit_child(
        &mut arena,
        root,
        Box::new(transformed_element(0xb201, 3.0, 4.0, 30.0, 20.0)),
    );
    measure_and_place(&mut arena, root, constraints(), placement());

    let mut trees = PropertyTrees::default();
    trees.sync(&arena, &[root]);
    let state = trees.states[&child].paint;
    let transforms = trees
        .transform_snapshot_chain_for(state.transform)
        .expect("transform snapshots");
    let mut positions = trees
        .layout_position_snapshot_chain_for(state.layout_position)
        .expect("layout-position snapshots");
    let visuals = trees
        .visual_offset_snapshot_chain_for(state.visual_offset)
        .expect("visual-offset snapshots");

    let incomplete = SpatialProjectionGraph::try_new(&transforms, &positions[..1], &visuals, &[]);
    assert_eq!(
        incomplete.err(),
        Some(SpatialProjectionError::MissingLayoutPosition(
            LayoutPositionNodeId(root),
        )),
    );

    positions[0].reference_scroll = Some(ScrollNodeId(root));
    let missing_scroll = SpatialProjectionGraph::try_new(&transforms, &positions, &visuals, &[]);
    assert_eq!(
        missing_scroll.err(),
        Some(SpatialProjectionError::MissingScroll(ScrollNodeId(root))),
        "an applied scroll edge may not disappear from the owning snapshot graph",
    );

    positions[0].reference_scroll = None;
    let mut cyclic_visuals = visuals.clone();
    cyclic_visuals
        .iter_mut()
        .find(|snapshot| snapshot.id == VisualOffsetNodeId(root))
        .expect("root visual")
        .parent = Some(VisualOffsetNodeId(child));
    let cyclic = SpatialProjectionGraph::try_new(&transforms, &positions, &cyclic_visuals, &[]);
    assert!(matches!(
        cyclic,
        Err(SpatialProjectionError::CyclicVisualOffset(_))
    ));

    positions[0].reference = SpatialPositionReference::Anchor(root);
    let graph = SpatialProjectionGraph::try_new(&transforms, &positions, &visuals, &[])
        .expect("anchor identity graph is complete");
    assert_eq!(
        graph
            .derive_owner_viewport_transform(TransformNodeId(child))
            .err(),
        Some(SpatialProjectionError::UnsupportedAnchorReference(
            LayoutPositionNodeId(child),
        )),
    );
}
