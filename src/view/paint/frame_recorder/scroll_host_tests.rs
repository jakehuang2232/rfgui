use super::*;
use crate::style::{Layout, ParsedValue, PropertyId, ScrollDirection, Style};
use crate::view::base_component::{DirtyPassMask, Element, ElementTrait, EventTarget, Size};
use crate::view::compositor::property_tree::{ClipNodeId, ClipNodeRole, ScrollNodeId};
use crate::view::node_arena::{Node, NodeArena};
use crate::view::paint::{PaintOp, PaintPayloadIdentity};

fn fixture_with_scrollbar(
    hovered: bool,
    shadow_blur_radius: f32,
) -> (
    NodeArena,
    NodeKey,
    NodeKey,
    PropertyTrees,
    PaintGenerationTracker,
) {
    let mut arena = NodeArena::new();
    let root = arena.insert(Node::new(Box::new(Element::new_with_id(
        81_001, 0.0, 0.0, 100.0, 80.0,
    ))));
    let child = arena.insert(Node::new(Box::new(Element::new_with_id(
        81_002, 0.0, -20.0, 100.0, 300.0,
    ))));
    arena.set_parent(child, Some(root));
    arena.push_child(root, child);
    let mut style = Style::new();
    style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(ScrollDirection::Vertical),
    );
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    {
        let mut root_node = arena.get_mut(root).unwrap();
        let root_element = root_node
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .unwrap();
        root_element.apply_style(style);
        root_element.layout_state.content_size = Size {
            width: 100.0,
            height: 300.0,
        };
        root_element.set_scroll_offset((0.0, 20.0));
        root_element.set_scrollbar_shadow_blur_radius(shadow_blur_radius);
        root_element.set_hovered(hovered);
        root_element.clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    }
    arena
        .get_mut(child)
        .unwrap()
        .element
        .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    arena.refresh_subtree_dirty_cache(root);
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[root]);
    assert!(
        properties.validation_errors.is_empty(),
        "unexpected offset fixture property errors: {:?}",
        properties.validation_errors
    );
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[root], &properties);
    (arena, root, child, properties, generations)
}

fn fixture() -> (
    NodeArena,
    NodeKey,
    NodeKey,
    PropertyTrees,
    PaintGenerationTracker,
) {
    fixture_with_scrollbar(false, 3.0)
}

fn opaque_fixture() -> (
    NodeArena,
    NodeKey,
    NodeKey,
    PropertyTrees,
    PaintGenerationTracker,
) {
    fixture_with_scrollbar(true, 3.0)
}

fn translucent_fixture() -> (
    NodeArena,
    NodeKey,
    NodeKey,
    PropertyTrees,
    PaintGenerationTracker,
) {
    let (arena, root, child, _, _) = fixture();
    {
        let mut root_node = arena.get_mut(root).unwrap();
        let root_element = root_node
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .unwrap();
        root_element.set_hovered(true);
        root_element.set_hovered(false);
        let sampled_at = crate::time::Instant::now();
        let _ = root_element.tick_post_layout_animation_frame(sampled_at);
        let _ = root_element.tick_post_layout_animation_frame(
            sampled_at + crate::time::Duration::from_millis(1_000),
        );
        root_element.clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    }
    arena.refresh_subtree_dirty_cache(root);
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[root]);
    assert!(
        properties.validation_errors.is_empty(),
        "unexpected offset fixture property errors: {:?}",
        properties.validation_errors
    );
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[root], &properties);
    (arena, root, child, properties, generations)
}

fn content_witness(
    root: NodeKey,
    child: NodeKey,
    properties: &PropertyTrees,
) -> PaintScrollContentWitness {
    let scroll = properties.scroll_snapshot_for(ScrollNodeId(root)).unwrap();
    let clip_id = ClipNodeId {
        owner: root,
        role: ClipNodeRole::ContentsClip,
    };
    let clip = properties
        .clip_snapshot_for(Some(clip_id))
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    PaintScrollContentWitness::new(root, child, scroll, clip).unwrap()
}

fn fixture_at_offset(
    offset: [f32; 2],
) -> (
    NodeArena,
    NodeKey,
    NodeKey,
    PropertyTrees,
    PaintGenerationTracker,
) {
    let mut arena = NodeArena::new();
    let root = arena.insert(Node::new(Box::new(Element::new_with_id(
        81_101, 0.0, 0.0, 100.0, 80.0,
    ))));
    let child = arena.insert(Node::new(Box::new(Element::new_with_id(
        81_102, -offset[0], -offset[1], 300.0, 300.0,
    ))));
    arena.set_parent(child, Some(root));
    arena.push_child(root, child);
    let mut style = Style::new();
    style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(ScrollDirection::Vertical),
    );
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    {
        let mut root_node = arena.get_mut(root).unwrap();
        let root_element = root_node
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .unwrap();
        root_element.apply_style(style);
        root_element.layout_state.content_size = Size {
            width: 300.0,
            height: 300.0,
        };
        root_element.set_scroll_offset((offset[0], offset[1]));
        root_element.clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    }
    {
        let mut child_node = arena.get_mut(child).unwrap();
        child_node
            .element
            .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    }
    arena.refresh_subtree_dirty_cache(root);
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[root]);
    assert!(
        properties.validation_errors.is_empty(),
        "unexpected offset fixture property errors: {:?}",
        properties.validation_errors
    );
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[root], &properties);
    (arena, root, child, properties, generations)
}

mod baked_scroll_compiler_tests;
mod baked_scroll_host_tests;
mod scroll_content_recorder_tests;
mod scroll_host_planner_tests;
