use super::*;
use crate::style::{Color, Layout, ParsedValue, PropertyId, ScrollDirection, Style};
use crate::view::base_component::{DirtyPassMask, Element, Rect, Size};
use crate::view::compositor::property_tree::{ClipNodeId, ClipNodeRole, ScrollNodeId};
use crate::view::node_arena::{Node, NodeArena};

fn install_geometry(arena: &NodeArena, key: NodeKey, rect: Rect, content: Size) {
    let mut node = arena.get_mut(key).unwrap();
    let element = node.element.as_any_mut().downcast_mut::<Element>().unwrap();
    element.layout_state.layout_position.x = rect.x;
    element.layout_state.layout_position.y = rect.y;
    element.layout_state.layout_size = Size {
        width: rect.width,
        height: rect.height,
    };
    element.layout_state.layout_inner_position.x = rect.x;
    element.layout_state.layout_inner_position.y = rect.y;
    element.layout_state.layout_inner_size = Size {
        width: rect.width,
        height: rect.height,
    };
    element.layout_state.content_size = content;
    element.set_background_color_value(Color::rgb(24, 48, 72));
}

fn fixture() -> (
    NodeArena,
    NodeKey,
    NodeKey,
    NodeKey,
    PropertyTrees,
    PaintGenerationTracker,
) {
    let mut arena = NodeArena::new();
    let outer = arena.insert(Node::new(Box::new(Element::new_with_id(
        0x1250_00, 10.0, 20.0, 100.0, 80.0,
    ))));
    let inner = arena.insert(Node::new(Box::new(Element::new_with_id(
        0x1250_01, 10.0, 20.0, 100.0, 300.0,
    ))));
    let leaf = arena.insert(Node::new(Box::new(Element::new_with_id(
        0x1250_02, 10.0, 20.0, 100.0, 600.0,
    ))));
    arena.set_parent(inner, Some(outer));
    arena.push_child(outer, inner);
    arena.set_parent(leaf, Some(inner));
    arena.push_child(inner, leaf);
    for owner in [outer, inner] {
        let mut style = Style::new();
        style.insert(
            PropertyId::ScrollDirection,
            ParsedValue::ScrollDirection(ScrollDirection::Vertical),
        );
        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        arena
            .get_mut(owner)
            .unwrap()
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .unwrap()
            .apply_style(style);
    }
    install_geometry(
        &arena,
        outer,
        Rect {
            x: 10.0,
            y: 20.0,
            width: 100.0,
            height: 80.0,
        },
        Size {
            width: 100.0,
            height: 300.0,
        },
    );
    install_geometry(
        &arena,
        inner,
        Rect {
            x: 10.0,
            y: 20.0,
            width: 100.0,
            height: 300.0,
        },
        Size {
            width: 100.0,
            height: 600.0,
        },
    );
    install_geometry(
        &arena,
        leaf,
        Rect {
            x: 10.0,
            y: 20.0,
            width: 100.0,
            height: 600.0,
        },
        Size {
            width: 100.0,
            height: 600.0,
        },
    );
    for key in [outer, inner, leaf] {
        arena
            .get_mut(key)
            .unwrap()
            .element
            .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    }
    arena.refresh_subtree_dirty_cache(outer);
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[outer]);
    assert!(properties.validation_errors.is_empty());
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[outer], &properties);
    (arena, outer, inner, leaf, properties, generations)
}

#[test]
fn nested_scroll_recorders_seal_h0_h1_receiver_o1_o0_and_two_scope_projection() {
    let (arena, outer, inner, leaf, properties, generations) = fixture();
    let admission = arena
        .get(outer)
        .unwrap()
        .element
        .as_any()
        .downcast_ref::<Element>()
        .unwrap()
        .exact_retained_nested_scroll_scene_admission(outer, &arena, 1.0)
        .expect("exact nested admission");
    assert_eq!(admission.inner_boundary_root, inner);
    assert_eq!(admission.content_leaf, leaf);

    let outer_scroll = properties.scroll_snapshot_for(ScrollNodeId(outer)).unwrap();
    let inner_scroll = properties.scroll_snapshot_for(ScrollNodeId(inner)).unwrap();
    let outer_clip_id = ClipNodeId {
        owner: outer,
        role: ClipNodeRole::ContentsClip,
    };
    let inner_clip_id = ClipNodeId {
        owner: inner,
        role: ClipNodeRole::ContentsClip,
    };
    let outer_clip = properties.clip_snapshot_for(Some(outer_clip_id)).unwrap()[0];
    let inner_clip = properties.clip_snapshot_for(Some(inner_clip_id)).unwrap()[0];
    let outer_state = crate::view::compositor::property_tree::PropertyTreeState {
        clip: Some(outer_clip_id),
        scroll: Some(outer_scroll.id),
        ..Default::default()
    };
    let inner_state = crate::view::compositor::property_tree::PropertyTreeState {
        clip: Some(inner_clip_id),
        scroll: Some(inner_scroll.id),
        ..Default::default()
    };
    assert_eq!(properties.states[&inner].paint, outer_state);
    assert_eq!(properties.states[&leaf].paint, inner_state);

    let outer_host =
        PaintBakedScrollHostWitness::new(outer, inner, outer_scroll, outer_clip_id).unwrap();
    let inner_cutout = super::super::PlannedBoundary {
        root: inner,
        stable_id: admission.inner_stable_id,
        kind: super::super::PlannedBoundaryKind::Scroll(inner_scroll.id),
    };
    let outer_steps = record_nested_scroll_outer_host_steps_for_plan(
        &arena,
        outer,
        &properties,
        &generations,
        outer_host,
        inner_cutout,
    )
    .expect("H0-S1-O0");
    assert!(matches!(
        outer_steps.as_slice(),
        [
            RecordedTransformSurfaceStep::Artifact(_),
            RecordedTransformSurfaceStep::Boundary(found),
            RecordedTransformSurfaceStep::Artifact(_),
        ] if *found == inner_cutout
    ));

    let outer_content =
        PaintScrollContentWitness::new(outer, inner, outer_scroll, outer_clip).unwrap();
    let inner_host =
        PaintBakedScrollHostWitness::new(inner, leaf, inner_scroll, inner_clip_id).unwrap();
    let content = PaintNestedScrollContentWitness::new(
        outer,
        inner,
        leaf,
        outer_scroll,
        outer_clip,
        inner_scroll,
        inner_clip,
    )
    .unwrap();
    assert!(
        PaintNestedScrollContentWitness::new(
            outer,
            inner,
            outer,
            outer_scroll,
            outer_clip,
            inner_scroll,
            inner_clip,
        )
        .is_none(),
        "outer/content alias must not mint a nested chain witness"
    );
    let inner_steps = record_nested_scroll_inner_host_steps_for_plan(
        &arena,
        inner,
        &properties,
        &generations,
        inner_host,
        outer_content,
        admission.content_leaf_stable_id,
        content,
    )
    .expect("H1-receiver-O1");
    assert!(matches!(
        inner_steps.as_slice(),
        [
            RecordedNestedScrollHostStep::Artifact(_),
            RecordedNestedScrollHostStep::ContentReceiver(_),
            RecordedNestedScrollHostStep::Artifact(_),
        ]
    ));

    let artifact =
        record_nested_scroll_content_artifact_for_plan(&arena, &properties, &generations, content)
            .expect("S1/C1 projects to S0/C0 in the leaf scope");
    assert!(!artifact.chunks.is_empty());
    assert!(
        artifact
            .chunks
            .iter()
            .all(|chunk| chunk.owner == leaf && chunk.properties == outer_state)
    );
}

#[test]
fn nested_scroll_receiver_and_parent_chain_tamper_fail_closed() {
    let (mut arena, outer, inner, leaf, mut properties, generations) = fixture();
    let outer_scroll = properties.scroll_snapshot_for(ScrollNodeId(outer)).unwrap();
    let inner_scroll = properties.scroll_snapshot_for(ScrollNodeId(inner)).unwrap();
    let outer_clip_id = ClipNodeId {
        owner: outer,
        role: ClipNodeRole::ContentsClip,
    };
    let inner_clip_id = ClipNodeId {
        owner: inner,
        role: ClipNodeRole::ContentsClip,
    };
    let outer_clip = properties.clip_snapshot_for(Some(outer_clip_id)).unwrap()[0];
    let inner_clip = properties.clip_snapshot_for(Some(inner_clip_id)).unwrap()[0];
    let content = PaintNestedScrollContentWitness::new(
        outer,
        inner,
        leaf,
        outer_scroll,
        outer_clip,
        inner_scroll,
        inner_clip,
    )
    .unwrap();
    let inner_host =
        PaintBakedScrollHostWitness::new(inner, leaf, inner_scroll, inner_clip_id).unwrap();
    let outer_content =
        PaintScrollContentWitness::new(outer, inner, outer_scroll, outer_clip).unwrap();

    assert!(
        record_nested_scroll_inner_host_steps_for_plan(
            &arena,
            inner,
            &properties,
            &generations,
            inner_host,
            outer_content,
            0,
            content,
        )
        .is_err()
    );

    arena.set_parent(leaf, Some(outer));
    assert!(
        record_nested_scroll_inner_host_steps_for_plan(
            &arena,
            inner,
            &properties,
            &generations,
            inner_host,
            outer_content,
            arena.get(leaf).unwrap().element.stable_id(),
            content,
        )
        .is_err()
    );
    arena.set_parent(leaf, Some(inner));

    properties
        .scrolls
        .get_mut(&ScrollNodeId(inner))
        .unwrap()
        .parent = None;
    assert!(
        record_nested_scroll_content_artifact_for_plan(&arena, &properties, &generations, content,)
            .is_err()
    );

    for drift in 0..5 {
        let (arena, outer, inner, leaf, mut properties, generations) = fixture();
        let outer_scroll = properties.scroll_snapshot_for(ScrollNodeId(outer)).unwrap();
        let inner_scroll = properties.scroll_snapshot_for(ScrollNodeId(inner)).unwrap();
        let outer_clip_id = ClipNodeId {
            owner: outer,
            role: ClipNodeRole::ContentsClip,
        };
        let inner_clip_id = ClipNodeId {
            owner: inner,
            role: ClipNodeRole::ContentsClip,
        };
        let outer_clip = properties.clip_snapshot_for(Some(outer_clip_id)).unwrap()[0];
        let inner_clip = properties.clip_snapshot_for(Some(inner_clip_id)).unwrap()[0];
        let content = PaintNestedScrollContentWitness::new(
            outer,
            inner,
            leaf,
            outer_scroll,
            outer_clip,
            inner_scroll,
            inner_clip,
        )
        .unwrap();
        match drift {
            0 => properties.clips.get_mut(&inner_clip_id).unwrap().parent = None,
            1 => properties.clips.get_mut(&inner_clip_id).unwrap().owner = outer,
            2 => properties.clips.get_mut(&inner_clip_id).unwrap().generation = 0,
            3 => {
                properties
                    .scrolls
                    .get_mut(&ScrollNodeId(inner))
                    .unwrap()
                    .owner = outer
            }
            4 => {
                properties
                    .scrolls
                    .get_mut(&ScrollNodeId(inner))
                    .unwrap()
                    .generation = 0
            }
            _ => unreachable!(),
        }
        assert!(
            record_nested_scroll_content_artifact_for_plan(
                &arena,
                &properties,
                &generations,
                content,
            )
            .is_err(),
            "live nested property drift case {drift} must fail closed"
        );
    }
}
