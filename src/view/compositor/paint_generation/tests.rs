use super::*;
use crate::style::{Length, ParsedValue, PropertyId, ScrollDirection, Style, Transform, Translate};
use crate::view::base_component::{
    BoxModelSnapshot, BuildState, DirtyPassMask, Element, EventTarget, LayoutConstraints,
    LayoutPlacement, Layoutable, Renderable, Size, UiBuildContext,
};
use crate::view::frame_graph::FrameGraph;
use crate::view::node_arena::Node;

fn insert_element(arena: &mut NodeArena, id: u64) -> NodeKey {
    arena.insert(Node::new(Box::new(Element::new_with_id(
        id, 0.0, 0.0, 100.0, 100.0,
    ))))
}

fn attach(arena: &mut NodeArena, parent: NodeKey, child: NodeKey) {
    arena.set_parent(child, Some(parent));
    arena.push_child(parent, child);
}

fn sync(
    tracker: &mut PaintGenerationTracker,
    trees: &mut PropertyTrees,
    arena: &NodeArena,
    roots: &[NodeKey],
) {
    trees.sync(arena, roots);
    tracker.sync(arena, roots, trees);
}

fn mutate_element(arena: &NodeArena, key: NodeKey, f: impl FnOnce(&mut Element)) {
    let mut node = arena.get_mut(key).expect("element exists");
    let element = node
        .element
        .as_any_mut()
        .downcast_mut::<Element>()
        .expect("Element");
    f(element);
}

#[test]
fn identical_sync_preserves_all_revisions() {
    let mut arena = NodeArena::new();
    let root = insert_element(&mut arena, 1);
    let mut trees = PropertyTrees::default();
    let mut tracker = PaintGenerationTracker::default();

    sync(&mut tracker, &mut trees, &arena, &[root]);
    let first = tracker.snapshot(root).unwrap();
    assert_eq!(
        first.coverage,
        PaintGenerationCoverage::RetainedSignatureObserved
    );
    sync(&mut tracker, &mut trees, &arena, &[root]);

    assert_eq!(tracker.snapshot(root).unwrap(), first);
}

#[test]
fn unchanged_topology_reuses_observed_children_allocation() {
    let mut arena = NodeArena::new();
    let root = insert_element(&mut arena, 1);
    let child = insert_element(&mut arena, 2);
    attach(&mut arena, root, child);
    let mut trees = PropertyTrees::default();
    let mut tracker = PaintGenerationTracker::default();

    sync(&mut tracker, &mut trees, &arena, &[root]);
    let first_storage = tracker.observed_children_storage(root).unwrap();
    sync(&mut tracker, &mut trees, &arena, &[root]);

    assert_eq!(tracker.observed_children_storage(root), Some(first_storage));
}

#[test]
fn background_signature_change_only_bumps_self_paint() {
    let mut arena = NodeArena::new();
    let root = insert_element(&mut arena, 1);
    let mut trees = PropertyTrees::default();
    let mut tracker = PaintGenerationTracker::default();
    sync(&mut tracker, &mut trees, &arena, &[root]);
    let first = tracker.snapshot(root).unwrap();

    mutate_element(&arena, root, |element| {
        element.set_background_color_value(crate::style::Color::rgba(1, 2, 3, 255));
    });
    sync(&mut tracker, &mut trees, &arena, &[root]);
    let second = tracker.snapshot(root).unwrap();

    assert_ne!(second.self_paint_revision, first.self_paint_revision);
    assert_eq!(second.composite_revision, first.composite_revision);
    assert_eq!(second.topology_revision, first.topology_revision);
}

#[test]
fn opacity_effect_only_bumps_composite() {
    let mut arena = NodeArena::new();
    let root = insert_element(&mut arena, 1);
    let mut trees = PropertyTrees::default();
    let mut tracker = PaintGenerationTracker::default();
    sync(&mut tracker, &mut trees, &arena, &[root]);
    let first = tracker.snapshot(root).unwrap();

    mutate_element(&arena, root, |element| element.set_opacity(0.5));
    sync(&mut tracker, &mut trees, &arena, &[root]);
    let second = tracker.snapshot(root).unwrap();

    assert_eq!(second.self_paint_revision, first.self_paint_revision);
    assert_ne!(second.composite_revision, first.composite_revision);
    assert_eq!(second.topology_revision, first.topology_revision);
}

#[test]
fn scroll_generation_conservatively_bumps_self_paint() {
    let mut arena = NodeArena::new();
    let root = insert_element(&mut arena, 1);
    let mut trees = PropertyTrees::default();
    let mut tracker = PaintGenerationTracker::default();
    sync(&mut tracker, &mut trees, &arena, &[root]);
    let first = tracker.snapshot(root).unwrap();

    let mut style = Style::new();
    style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(ScrollDirection::Vertical),
    );
    mutate_element(&arena, root, |element| element.apply_style(style));
    let child = insert_element(&mut arena, 2);
    attach(&mut arena, root, child);
    mutate_element(&arena, root, |element| {
        element.layout_state.content_size = Size {
            width: 100.0,
            height: 300.0,
        };
        element.clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    });
    arena
        .get_mut(child)
        .expect("child")
        .element
        .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
    arena.refresh_subtree_dirty_cache(root);
    sync(&mut tracker, &mut trees, &arena, &[root]);
    let second = tracker.snapshot(root).unwrap();

    assert_ne!(second.self_paint_revision, first.self_paint_revision);
    assert_eq!(second.composite_revision, first.composite_revision);
    assert_ne!(
        second.observed_scroll_generation,
        first.observed_scroll_generation
    );
}

#[test]
fn transform_generation_conservatively_bumps_only_self_paint() {
    let mut arena = NodeArena::new();
    let root = insert_element(&mut arena, 1);
    let mut trees = PropertyTrees::default();
    let mut tracker = PaintGenerationTracker::default();
    sync(&mut tracker, &mut trees, &arena, &[root]);
    let first = tracker.snapshot(root).unwrap();

    let mut style = Style::new();
    style.set_transform(Transform::new([Translate::x(Length::px(12.0))]));
    mutate_element(&arena, root, |element| element.apply_style(style));
    sync(&mut tracker, &mut trees, &arena, &[root]);
    let second = tracker.snapshot(root).unwrap();

    assert_ne!(second.self_paint_revision, first.self_paint_revision);
    assert_eq!(second.composite_revision, first.composite_revision);
    assert_eq!(second.topology_revision, first.topology_revision);
    assert_ne!(
        second.observed_transform_generation,
        first.observed_transform_generation
    );

    sync(&mut tracker, &mut trees, &arena, &[root]);
    assert_eq!(tracker.snapshot(root).unwrap(), second);
}

#[test]
fn reorder_and_reparent_bump_local_topology_records() {
    let mut arena = NodeArena::new();
    let left = insert_element(&mut arena, 1);
    let right = insert_element(&mut arena, 2);
    let a = insert_element(&mut arena, 3);
    let b = insert_element(&mut arena, 4);
    attach(&mut arena, left, a);
    attach(&mut arena, left, b);
    let mut trees = PropertyTrees::default();
    let mut tracker = PaintGenerationTracker::default();
    sync(&mut tracker, &mut trees, &arena, &[left, right]);
    let left_first = tracker.snapshot(left).unwrap();
    let a_first = tracker.snapshot(a).unwrap();

    arena.set_children(left, vec![b, a]);
    sync(&mut tracker, &mut trees, &arena, &[left, right]);
    let left_reordered = tracker.snapshot(left).unwrap();
    assert_ne!(
        left_reordered.topology_revision,
        left_first.topology_revision
    );

    arena.set_children(left, vec![b]);
    arena.set_parent(a, Some(right));
    arena.push_child(right, a);
    let right_before = tracker.snapshot(right).unwrap();
    sync(&mut tracker, &mut trees, &arena, &[left, right]);

    assert_ne!(
        tracker.snapshot(left).unwrap().topology_revision,
        left_reordered.topology_revision
    );
    assert_ne!(
        tracker.snapshot(right).unwrap().topology_revision,
        right_before.topology_revision
    );
    assert_ne!(
        tracker.snapshot(a).unwrap().topology_revision,
        a_first.topology_revision
    );
}

#[test]
fn root_reorder_bumps_forest_topology_without_changing_node_topology() {
    let mut arena = NodeArena::new();
    let first_root = insert_element(&mut arena, 1);
    let second_root = insert_element(&mut arena, 2);
    let mut trees = PropertyTrees::default();
    let mut tracker = PaintGenerationTracker::default();
    sync(&mut tracker, &mut trees, &arena, &[first_root, second_root]);
    let first_forest_revision = tracker.root_topology_revision();
    let first_node = tracker.snapshot(first_root).unwrap();
    let second_node = tracker.snapshot(second_root).unwrap();

    sync(&mut tracker, &mut trees, &arena, &[second_root, first_root]);

    assert_ne!(tracker.root_topology_revision(), first_forest_revision);
    assert_eq!(tracker.snapshot(first_root).unwrap(), first_node);
    assert_eq!(tracker.snapshot(second_root).unwrap(), second_node);
}

#[test]
fn live_detach_retains_record_and_remove_prunes_without_key_aliasing() {
    let mut arena = NodeArena::new();
    let root = insert_element(&mut arena, 1);
    let child = insert_element(&mut arena, 2);
    attach(&mut arena, root, child);
    let mut trees = PropertyTrees::default();
    let mut tracker = PaintGenerationTracker::default();
    sync(&mut tracker, &mut trees, &arena, &[root]);
    let first = tracker.snapshot(child).unwrap();

    arena.set_children(root, Vec::new());
    arena.set_parent(child, None);
    sync(&mut tracker, &mut trees, &arena, &[root]);
    let inactive = tracker.snapshot(child).unwrap();
    assert!(!inactive.active);
    assert_ne!(inactive.topology_revision, first.topology_revision);
    assert_eq!(inactive.self_paint_revision, first.self_paint_revision);
    assert_eq!(inactive.composite_revision, first.composite_revision);

    arena.set_parent(child, Some(root));
    arena.push_child(root, child);
    sync(&mut tracker, &mut trees, &arena, &[root]);
    let reattached = tracker.snapshot(child).unwrap();
    assert!(reattached.active);
    assert_ne!(reattached.topology_revision, inactive.topology_revision);
    assert_eq!(reattached.self_paint_revision, first.self_paint_revision);
    assert_eq!(reattached.composite_revision, first.composite_revision);

    arena.remove_subtree(child);
    sync(&mut tracker, &mut trees, &arena, &[root]);
    assert!(tracker.snapshot(child).is_none());

    let replacement = insert_element(&mut arena, 3);
    assert_ne!(replacement, child);
    attach(&mut arena, root, replacement);
    sync(&mut tracker, &mut trees, &arena, &[root]);
    assert_ne!(
        tracker.snapshot(replacement).unwrap().self_paint_revision,
        reattached.self_paint_revision
    );
}

#[derive(Default)]
struct CustomHost;

impl Layoutable for CustomHost {
    fn measure(&mut self, _constraints: LayoutConstraints, _arena: &mut NodeArena) {}
    fn place(&mut self, _placement: LayoutPlacement, _arena: &mut NodeArena) {}
    fn measured_size(&self) -> (f32, f32) {
        (0.0, 0.0)
    }
    fn set_layout_width(&mut self, _width: f32) {}
    fn set_layout_height(&mut self, _height: f32) {}
}

impl EventTarget for CustomHost {}

impl Renderable for CustomHost {
    fn build(
        &mut self,
        _graph: &mut FrameGraph,
        _arena: &mut NodeArena,
        ctx: UiBuildContext,
    ) -> BuildState {
        ctx.into_state()
    }
}

impl ElementTrait for CustomHost {
    fn stable_id(&self) -> u64 {
        99
    }

    fn box_model_snapshot(&self) -> BoxModelSnapshot {
        BoxModelSnapshot {
            node_id: 99,
            parent_id: None,
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
            border_radius: 0.0,
            should_render: false,
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[test]
fn generic_tracker_tracks_untracked_custom_hosts() {
    let mut arena = NodeArena::new();
    let root = arena.insert(Node::new(Box::new(CustomHost::default())));
    let mut trees = PropertyTrees::default();
    let mut tracker = PaintGenerationTracker::default();
    sync(&mut tracker, &mut trees, &arena, &[root]);
    let first = tracker.snapshot(root).unwrap();
    sync(&mut tracker, &mut trees, &arena, &[root]);
    let second = tracker.snapshot(root).unwrap();

    assert_eq!(first.coverage, PaintGenerationCoverage::Untracked);
    assert_ne!(first.self_paint_revision, second.self_paint_revision);
    assert_eq!(first.composite_revision, second.composite_revision);
    assert_eq!(first.topology_revision, second.topology_revision);
    assert!(tracker.matches_live_snapshot(&arena, &[root], &trees));
}
