use glam::{Mat4, Vec2, Vec3};

use super::*;
use crate::view::{
    base_component::{
        Element, Rect, ScrollAxisSnapshot, ScrollContentsClipWitness, ScrollbarInteractionWitness,
        ScrollbarOverlayWitness, ScrollbarPaintStateWitness, Size,
    },
    compositor::property_tree::{
        ClipNodeSnapshot, EffectNodeSnapshot, LayoutPositionNodeSnapshot, ScrollNodeSnapshot,
        SpatialPositionReference, TransformNodeSnapshot, VisualOffsetNodeSnapshot,
    },
    node_arena::{Node, NodeArena},
    render_pass::draw_rect_pass::{RectPassParams, RectRenderMode},
};

use crate::view::paint::{
    DrawRectOp, PaintChunk, PaintChunkId, PaintChunkRole, PaintContentRevision, PaintNodePhase,
    PaintOp, PaintOwnerSnapshot, PaintPayloadIdentity, PaintPropertyScope,
};

fn insert_owner(arena: &mut NodeArena, stable_id: u64) -> NodeKey {
    arena.insert(Node::new(Box::new(Element::new_with_id(
        stable_id, 0.0, 0.0, 10.0, 10.0,
    ))))
}

fn chunk(owner: NodeKey, phase: PaintNodePhase, op_range: std::ops::Range<usize>) -> PaintChunk {
    PaintChunk {
        id: PaintChunkId {
            owner,
            scope: PaintPropertyScope::SelfPaint,
            phase,
            slot: 0,
            role: PaintChunkRole::SelfDecoration,
        },
        owner,
        op_range,
        bounds: Rect {
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
        },
        properties: PropertyTreeState::default(),
        content_revision: PaintContentRevision {
            self_paint_revision: 1,
            composite_revision: 1,
            topology_revision: 1,
        },
        payload_identity: PaintPayloadIdentity::None,
    }
}

fn scroll_snapshot(id: ScrollNodeId, parent: Option<ScrollNodeId>) -> ScrollNodeSnapshot {
    ScrollNodeSnapshot {
        id,
        owner: id.0,
        parent,
        offset: Vec2::ZERO,
        configured_axis: ScrollAxisSnapshot::Vertical,
        viewport: Rect {
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
        },
        content_size: Size {
            width: 10.0,
            height: 20.0,
        },
        layout_content_bounds_at_zero: Rect {
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 20.0,
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
        contents_clip: ScrollContentsClipWitness::ExactRect([0, 0, 10, 10]),
        generation: 1,
    }
}

fn complete_artifact() -> (
    PaintArtifact,
    PropertyTreeState,
    PropertyTreeState,
    NodeKey,
    NodeKey,
) {
    let mut arena = NodeArena::new();
    let root = insert_owner(&mut arena, 0xc1_0001);
    let child = insert_owner(&mut arena, 0xc1_0002);
    let missing = insert_owner(&mut arena, 0xc1_00ff);

    let root_transform = TransformNodeId(root);
    let child_transform = TransformNodeId(child);
    let root_clip = ClipNodeId {
        owner: root,
        role: ClipNodeRole::ContentsClip,
    };
    let child_clip = ClipNodeId {
        owner: child,
        role: ClipNodeRole::SelfClip,
    };
    let root_effect = EffectNodeId(root);
    let child_effect = EffectNodeId(child);
    let root_scroll = ScrollNodeId(root);
    let child_scroll = ScrollNodeId(child);
    let root_position = LayoutPositionNodeId(root);
    let child_position = LayoutPositionNodeId(child);
    let root_visual = VisualOffsetNodeId(root);
    let child_visual = VisualOffsetNodeId(child);

    let artifact = PaintArtifact {
        chunks: vec![
            chunk(root, PaintNodePhase::BeforeChildren, 0..0),
            chunk(child, PaintNodePhase::AfterChildren, 0..0),
        ],
        clip_nodes: vec![
            ClipNodeSnapshot {
                id: child_clip,
                owner: child,
                parent: Some(root_clip),
                logical_scissor: [1, 1, 8, 8],
                behavior: ClipBehavior::Replace,
                generation: 1,
            },
            ClipNodeSnapshot {
                id: root_clip,
                owner: root,
                parent: None,
                logical_scissor: [0, 0, 10, 10],
                behavior: ClipBehavior::Intersect,
                generation: 1,
            },
        ],
        effect_nodes: vec![
            EffectNodeSnapshot {
                id: child_effect,
                owner: child,
                parent: Some(root_effect),
                opacity: 0.5,
                generation: 1,
            },
            EffectNodeSnapshot {
                id: root_effect,
                owner: root,
                parent: None,
                opacity: 0.75,
                generation: 1,
            },
        ],
        transform_nodes: vec![
            TransformNodeSnapshot {
                id: child_transform,
                owner: child,
                parent: Some(root_transform),
                local_matrix: Mat4::IDENTITY,
                local_origin: Vec3::ZERO,
                local_generation: 1,
                generation: 1,
                owner_viewport_position: Vec2::ZERO,
                owner_viewport_transform: Mat4::IDENTITY,
            },
            TransformNodeSnapshot {
                id: root_transform,
                owner: root,
                parent: None,
                local_matrix: Mat4::IDENTITY,
                local_origin: Vec3::ZERO,
                local_generation: 1,
                generation: 1,
                owner_viewport_position: Vec2::ZERO,
                owner_viewport_transform: Mat4::IDENTITY,
            },
        ],
        layout_position_nodes: vec![
            LayoutPositionNodeSnapshot {
                id: child_position,
                owner: child,
                reference: SpatialPositionReference::LayoutParent(Some(root)),
                reference_scroll: Some(root_scroll),
                translation_at_scroll_zero: Vec2::ZERO,
                child_reference_offset_at_scroll_zero: Vec2::ZERO,
                generation: 1,
            },
            LayoutPositionNodeSnapshot {
                id: root_position,
                owner: root,
                reference: SpatialPositionReference::Viewport,
                reference_scroll: None,
                translation_at_scroll_zero: Vec2::ZERO,
                child_reference_offset_at_scroll_zero: Vec2::ZERO,
                generation: 1,
            },
        ],
        visual_offset_nodes: vec![
            VisualOffsetNodeSnapshot {
                id: child_visual,
                owner: child,
                parent: Some(root_visual),
                offset: Vec2::ZERO,
                generation: 1,
            },
            VisualOffsetNodeSnapshot {
                id: root_visual,
                owner: root,
                parent: None,
                offset: Vec2::ZERO,
                generation: 1,
            },
        ],
        scroll_nodes: vec![
            scroll_snapshot(child_scroll, Some(root_scroll)),
            scroll_snapshot(root_scroll, None),
        ],
        owner_nodes: vec![
            PaintOwnerSnapshot {
                owner: root,
                parent: None,
            },
            PaintOwnerSnapshot {
                owner: child,
                parent: Some(root),
            },
        ],
        ..PaintArtifact::default()
    };
    let from = PropertyTreeState {
        transform: Some(child_transform),
        clip: Some(child_clip),
        effect: Some(child_effect),
        scroll: Some(child_scroll),
        layout_position: Some(child_position),
        visual_offset: Some(child_visual),
    };
    let to = PropertyTreeState {
        transform: Some(root_transform),
        clip: Some(root_clip),
        effect: Some(root_effect),
        scroll: Some(root_scroll),
        layout_position: Some(root_position),
        visual_offset: Some(root_visual),
    };
    (artifact, from, to, child, missing)
}

#[test]
fn classifier_preserves_all_six_dimensions_without_receiver_grammar() {
    let (artifact, from, to, target, _) = complete_artifact();
    let snapshots = PropertySnapshotGraph::try_from_artifact(&artifact).expect("closed snapshots");
    assert_eq!(
        snapshots.clip_parent(from.clip.expect("child clip")),
        Ok(to.clip),
    );
    let transition =
        classify_property_transition(from, to, &snapshots).expect("six-dimensional transition");
    assert_eq!(transition, PropertyStateTransition::between(from, to));

    let cursors = artifact_cursors(&artifact).expect("artifact traversal");
    let owners = ArtifactOwnerGraph::try_from_artifact(&artifact).expect("owner graph");
    assert_eq!(
        owners.parent(target),
        Ok(Some(artifact.owner_nodes[0].owner))
    );
    let scene_target = owners.scene_target(target).expect("rooted target");
    let event = ClassifiedTransitionEvent::new(scene_target, cursors[1], from, to, &snapshots)
        .expect("classified event");
    assert_eq!(event.scene_root_ordinal(), 0);
    assert_eq!(event.cursor().chunk_index(), 1);
    assert_eq!(event.cursor().op_index(), 0);
    assert_eq!(event.target(), target);
    assert_eq!(event.transition(), transition);
}

#[test]
fn classifier_rejects_every_missing_dimension_with_a_typed_owner() {
    let (artifact, _, _, _, missing) = complete_artifact();
    let snapshots = PropertySnapshotGraph::try_from_artifact(&artifact).expect("closed snapshots");
    let cases = [
        (
            PropertyTreeState {
                transform: Some(TransformNodeId(missing)),
                ..Default::default()
            },
            TransitionError::UnknownTransformReference(TransformNodeId(missing)),
        ),
        (
            PropertyTreeState {
                clip: Some(ClipNodeId {
                    owner: missing,
                    role: ClipNodeRole::SelfClip,
                }),
                ..Default::default()
            },
            TransitionError::UnknownClipReference(ClipNodeId {
                owner: missing,
                role: ClipNodeRole::SelfClip,
            }),
        ),
        (
            PropertyTreeState {
                effect: Some(EffectNodeId(missing)),
                ..Default::default()
            },
            TransitionError::UnknownEffectReference(EffectNodeId(missing)),
        ),
        (
            PropertyTreeState {
                scroll: Some(ScrollNodeId(missing)),
                ..Default::default()
            },
            TransitionError::UnknownScrollReference(ScrollNodeId(missing)),
        ),
        (
            PropertyTreeState {
                layout_position: Some(LayoutPositionNodeId(missing)),
                ..Default::default()
            },
            TransitionError::UnknownLayoutPositionReference(LayoutPositionNodeId(missing)),
        ),
        (
            PropertyTreeState {
                visual_offset: Some(VisualOffsetNodeId(missing)),
                ..Default::default()
            },
            TransitionError::UnknownVisualOffsetReference(VisualOffsetNodeId(missing)),
        ),
    ];
    for (state, expected) in cases {
        assert_eq!(
            classify_property_transition(state, PropertyTreeState::default(), &snapshots),
            Err(expected),
        );
    }
}

#[test]
fn snapshot_topology_and_artifact_cursor_fail_closed() {
    let (mut artifact, _, _, _, _) = complete_artifact();
    let child_clip = artifact.clip_nodes[0].id;
    artifact.clip_nodes[1].parent = Some(child_clip);
    assert_eq!(
        PropertySnapshotGraph::try_from_artifact(&artifact).err(),
        Some(TransitionError::CyclicClip(child_clip)),
    );

    let (mut artifact, _, _, _, _) = complete_artifact();
    artifact.chunks[1].op_range = 1..1;
    assert_eq!(
        artifact_cursors(&artifact),
        Err(TransitionError::InvalidArtifactCursor(1)),
    );

    let (mut artifact, _, _, _, _) = complete_artifact();
    artifact.ops.push(PaintOp::DrawRect(DrawRectOp {
        params: RectPassParams::default(),
        mode: RectRenderMode::FillOnly,
    }));
    assert_eq!(
        artifact_cursors(&artifact),
        Err(TransitionError::NonTerminalArtifactCursor(0)),
    );
}

#[test]
fn spatial_cycle_owner_is_deterministic_in_artifact_store_order() {
    let (mut artifact, _, _, _, _) = complete_artifact();
    let child = artifact.transform_nodes[0].id;
    artifact.transform_nodes[1].parent = Some(child);
    assert_eq!(
        PropertySnapshotGraph::try_from_artifact(&artifact).err(),
        Some(TransitionError::SpatialSnapshot(
            SpatialProjectionError::CyclicTransform(child),
        )),
    );

    let (mut artifact, _, _, _, _) = complete_artifact();
    let child = artifact.layout_position_nodes[0].id;
    artifact.layout_position_nodes[1].reference =
        SpatialPositionReference::LayoutParent(Some(child.0));
    assert_eq!(
        PropertySnapshotGraph::try_from_artifact(&artifact).err(),
        Some(TransitionError::SpatialSnapshot(
            SpatialProjectionError::CyclicLayoutPosition(child),
        )),
    );

    let (mut artifact, _, _, _, _) = complete_artifact();
    let child = artifact.visual_offset_nodes[0].id;
    artifact.visual_offset_nodes[1].parent = Some(child);
    assert_eq!(
        PropertySnapshotGraph::try_from_artifact(&artifact).err(),
        Some(TransitionError::SpatialSnapshot(
            SpatialProjectionError::CyclicVisualOffset(child),
        )),
    );

    let (mut artifact, _, _, _, _) = complete_artifact();
    let child = artifact.scroll_nodes[0].id;
    artifact.scroll_nodes[1].parent = Some(child);
    assert_eq!(
        PropertySnapshotGraph::try_from_artifact(&artifact).err(),
        Some(TransitionError::SpatialSnapshot(
            SpatialProjectionError::CyclicScroll(child),
        )),
    );
}

#[test]
fn scene_root_ordinal_is_derived_from_owner_store_order() {
    let (mut artifact, _, _, child, second_root) = complete_artifact();
    artifact
        .chunks
        .push(chunk(second_root, PaintNodePhase::BeforeChildren, 0..0));
    artifact.owner_nodes.push(PaintOwnerSnapshot {
        owner: second_root,
        parent: None,
    });

    let owners = ArtifactOwnerGraph::try_from_artifact(&artifact).expect("two-root owner graph");
    let first = owners.scene_target(child).expect("first rooted target");
    let second = owners
        .scene_target(second_root)
        .expect("second rooted target");
    assert_eq!(first.scene_root_ordinal, 0);
    assert_eq!(second.scene_root_ordinal, 1);
}

#[test]
fn transition_error_taxonomy_is_exhaustive() {
    fn spatial_reason(error: SpatialProjectionError) -> &'static str {
        match error {
            SpatialProjectionError::DuplicateTransform(_) => "duplicate-transform",
            SpatialProjectionError::DuplicateLayoutPosition(_) => "duplicate-layout-position",
            SpatialProjectionError::DuplicateVisualOffset(_) => "duplicate-visual-offset",
            SpatialProjectionError::DuplicateScroll(_) => "duplicate-scroll",
            SpatialProjectionError::MissingTransform(_) => "missing-transform",
            SpatialProjectionError::MissingLayoutPosition(_) => "missing-layout-position",
            SpatialProjectionError::MissingVisualOffset(_) => "missing-visual-offset",
            SpatialProjectionError::MissingScroll(_) => "missing-scroll",
            SpatialProjectionError::InvalidSnapshot(_) => "invalid-snapshot",
            SpatialProjectionError::CyclicTransform(_) => "cyclic-transform",
            SpatialProjectionError::CyclicLayoutPosition(_) => "cyclic-layout-position",
            SpatialProjectionError::CyclicVisualOffset(_) => "cyclic-visual-offset",
            SpatialProjectionError::CyclicScroll(_) => "cyclic-scroll",
            SpatialProjectionError::InvalidLayoutReference(_) => "invalid-layout-reference",
        }
    }

    fn reason(error: TransitionError) -> &'static str {
        match error {
            TransitionError::SpatialSnapshot(error) => spatial_reason(error),
            TransitionError::DuplicateClip(_) => "duplicate-clip",
            TransitionError::InvalidClip(_) => "invalid-clip",
            TransitionError::MissingClip(_) => "missing-clip",
            TransitionError::CyclicClip(_) => "cyclic-clip",
            TransitionError::DuplicateEffect(_) => "duplicate-effect",
            TransitionError::InvalidEffect(_) => "invalid-effect",
            TransitionError::MissingEffect(_) => "missing-effect",
            TransitionError::CyclicEffect(_) => "cyclic-effect",
            TransitionError::UnknownTransformReference(_) => "unknown-transform-reference",
            TransitionError::UnknownClipReference(_) => "unknown-clip-reference",
            TransitionError::UnknownEffectReference(_) => "unknown-effect-reference",
            TransitionError::UnknownScrollReference(_) => "unknown-scroll-reference",
            TransitionError::UnknownLayoutPositionReference(_) => {
                "unknown-layout-position-reference"
            }
            TransitionError::UnknownVisualOffsetReference(_) => "unknown-visual-offset-reference",
            TransitionError::DuplicateOwner(_) => "duplicate-owner",
            TransitionError::InvalidOwner(_) => "invalid-owner",
            TransitionError::MissingOwnerParent(_) => "missing-owner-parent",
            TransitionError::CyclicOwner(_) => "cyclic-owner",
            TransitionError::UnknownTarget(_) => "unknown-target",
            TransitionError::UnreferencedOwner(_) => "unreferenced-owner",
            TransitionError::SceneRootOrdinalOverflow(_) => "scene-root-ordinal-overflow",
            TransitionError::InvalidArtifactCursor(_) => "invalid-artifact-cursor",
            TransitionError::NonTerminalArtifactCursor(_) => "non-terminal-artifact-cursor",
        }
    }

    let (artifact, _, _, _, _) = complete_artifact();
    let mut duplicate = artifact.clone();
    duplicate.transform_nodes.push(duplicate.transform_nodes[0]);
    let error = PropertySnapshotGraph::try_from_artifact(&duplicate)
        .err()
        .expect("duplicate transform must reject");
    assert_eq!(reason(error), "duplicate-transform");
}
