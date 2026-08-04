use super::*;

use crate::style::{Layout, ParsedValue, PropertyId, Scale, ScrollDirection, Style, Transform};
use crate::view::base_component::{
    Element, LayoutConstraints, LayoutPlacement, Rect, ScrollAxisSnapshot,
    ScrollContentsClipWitness, ScrollbarInteractionWitness, ScrollbarOverlayWitness,
    ScrollbarPaintStateWitness, Size,
};
use crate::view::compositor::property_tree::{
    LayoutPositionNodeId, ScrollNode, ScrollNodeId, TransformNodeId, VisualOffsetNodeId,
};
use crate::view::paint::{
    PaintChunkId, PaintChunkRole, PaintContentRevision, PaintNodePhase, PaintPayloadIdentity,
    PaintPropertyScope,
};
use crate::view::test_support::{
    commit_child, commit_element, get_element_mut, measure_and_place, new_test_arena,
};

#[test]
fn snapshot_merge_rejects_conflicting_duplicate_identity() {
    let mut store = FxHashMap::default();
    assert_eq!(
        merge_snapshot(&mut store, 7_u64, 11_u64),
        SnapshotMerge::Inserted
    );
    assert_eq!(merge_snapshot(&mut store, 7, 11), SnapshotMerge::Identical);
    assert_eq!(merge_snapshot(&mut store, 7, 12), SnapshotMerge::Conflict);
    assert_eq!(
        store[&7], 11,
        "conflict must not replace the canonical first snapshot"
    );
}

#[test]
fn artifact_snapshots_complete_spatial_ancestor_graphs_without_arena_queries() {
    let mut root_element = Element::new_with_id(0xb1f0, 0.0, 0.0, 180.0, 100.0);
    let mut root_style = Style::new();
    root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    root_style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(ScrollDirection::Vertical),
    );
    root_element.apply_style(root_style);

    let mut child_element = Element::new_with_id(0xb1f1, 4.0, 70.0, 80.0, 140.0);
    let mut child_style = Style::new();
    child_style.set_transform(Transform::new([Scale::uniform(1.1)]));
    child_element.apply_style(child_style);

    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(root_element));
    let child = commit_child(&mut arena, root, Box::new(child_element));
    measure_and_place(
        &mut arena,
        root,
        LayoutConstraints {
            max_width: 400.0,
            max_height: 300.0,
            viewport_width: 400.0,
            viewport_height: 300.0,
            percent_base_width: Some(400.0),
            percent_base_height: Some(300.0),
        },
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
        },
    );
    get_element_mut::<Element>(&arena, root)
        .layout_state
        .content_size
        .height = 300.0;
    let mut trees = PropertyTrees::default();
    trees.sync(&arena, &[root]);
    trees.scrolls.insert(
        ScrollNodeId(root),
        ScrollNode {
            owner: root,
            parent: None,
            offset: glam::Vec2::ZERO,
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
                interaction: ScrollbarInteractionWitness {
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
        },
    );
    trees.refresh_derived_spatial_projections_for_test();

    let mut artifact = PaintArtifact {
        chunks: vec![PaintChunk {
            id: PaintChunkId {
                owner: child,
                scope: PaintPropertyScope::SelfPaint,
                phase: PaintNodePhase::BeforeChildren,
                slot: 0,
                role: PaintChunkRole::SelfDecoration,
            },
            owner: child,
            op_range: 0..0,
            bounds: Rect {
                x: 0.0,
                y: 0.0,
                width: 1.0,
                height: 1.0,
            },
            properties: trees.states[&child].paint,
            content_revision: PaintContentRevision {
                self_paint_revision: 1,
                composite_revision: 1,
                topology_revision: 1,
            },
            payload_identity: PaintPayloadIdentity::None,
        }],
        ..PaintArtifact::default()
    };
    assert!(
        trees
            .transform_snapshot_chain_for(artifact.chunks[0].properties.transform)
            .is_some(),
        "transform snapshot chain: {:?}",
        trees.spatial_validation_errors
    );
    assert!(
        trees
            .layout_position_snapshot_chain_for(
                artifact.chunks[0].properties.layout_position,
            )
            .is_some(),
        "layout-position snapshot chain"
    );
    assert!(
        trees
            .visual_offset_snapshot_chain_for(artifact.chunks[0].properties.visual_offset)
            .is_some(),
        "visual-offset snapshot chain"
    );
    populate_referenced_property_snapshots(&mut artifact, &trees).expect("snapshot graph");

    assert_eq!(
        artifact
            .transform_nodes
            .iter()
            .map(|snapshot| snapshot.id)
            .collect::<Vec<_>>(),
        vec![TransformNodeId(child)],
    );
    assert_eq!(
        artifact
            .layout_position_nodes
            .iter()
            .map(|snapshot| snapshot.id)
            .collect::<Vec<_>>(),
        vec![LayoutPositionNodeId(child), LayoutPositionNodeId(root)],
    );
    assert_eq!(
        artifact
            .visual_offset_nodes
            .iter()
            .map(|snapshot| snapshot.id)
            .collect::<Vec<_>>(),
        vec![VisualOffsetNodeId(child), VisualOffsetNodeId(root)],
    );
    assert_eq!(
        artifact
            .scroll_nodes
            .iter()
            .map(|snapshot| snapshot.id)
            .collect::<Vec<_>>(),
        vec![ScrollNodeId(root)],
    );
}
