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
fn stage_c_depth_four_scroll_chain_is_an_arena_independent_snapshot_fixture() {
    let (arena, _, wrapper, fourth, properties, _) = stage_c_depth_four_scroll_fixture();
    let crate::view::base_component::ScrollGeometryObservation::Exact(fourth_geometry) =
        crate::view::test_support::get_element::<Element>(&arena, fourth)
            .scroll_geometry_observation(fourth, &arena)
    else {
        panic!("fourth fixture host must have live scroll geometry");
    };
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
    assert!(scroll_geometry_snapshot_matches_scroll_node(
        fourth_geometry,
        properties
            .scroll_snapshot_for(ScrollNodeId(fourth))
            .expect("C0c fourth scroll snapshot"),
    ));
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
