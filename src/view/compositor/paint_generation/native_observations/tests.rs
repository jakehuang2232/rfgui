use super::*;
use crate::style::{Color, ColorLike, Length, Style, Transform, Translate};
use crate::view::base_component::{DirtyFlags, Element};
use crate::view::test_support::{commit_element, commit_child, get_element_mut};
use std::cell::Cell;
use std::rc::Rc;

fn compare(
    cached: &mut PaintGenerationTracker,
    full: &mut PaintGenerationTracker,
    arena: &NodeArena,
    roots: &[NodeKey],
    trees: &PropertyTrees,
) {
    cached.sync_arena(arena, roots, trees);
    full.sync(arena, roots, trees);
    assert_eq!(
        cached.root_topology_revision(),
        full.root_topology_revision()
    );
    assert_eq!(cached.nodes.len(), full.nodes.len());
    for key in cached.nodes.keys() {
        assert_eq!(cached.snapshot(*key), full.snapshot(*key), "owner {key:?}");
    }
    assert_eq!(
        cached.live_snapshot_mismatch(arena, roots, trees),
        full.live_snapshot_mismatch(arena, roots, trees)
    );
}

#[test]
fn native_observation_replay_matches_full_sync_through_edit_reparent_and_foreign_arena() {
    let mut arena = NodeArena::new();
    let root = commit_element(&mut arena, Box::new(Element::new(0., 0., 100., 100.)));
    let other = commit_element(&mut arena, Box::new(Element::new(0., 0., 100., 100.)));
    let child = commit_child(&mut arena, root, Box::new(Element::new(0., 0., 10., 10.)));
    let mut trees = PropertyTrees::default();
    let mut cached = PaintGenerationTracker::default();
    let mut full = PaintGenerationTracker::default();
    for frame in 0..8 {
        match frame {
            2 => {
                get_element_mut::<Element>(&arena, child)
                    .set_background_color(Color::rgb(1, 20, 3));
                arena.clear_element_dirty_flags(child, DirtyFlags::ALL);
            }
            3 => get_element_mut::<Element>(&arena, root).set_opacity(0.5),
            4 => {
                let mut style = Style::new();
                style.set_transform(Transform::new([Translate::x(Length::px(12.))]));
                get_element_mut::<Element>(&arena, root).apply_style(style);
            }
            5 => {
                arena.set_children(root, vec![]);
                arena.set_children(other, vec![child]);
                arena.set_parent(child, Some(other));
            }
            6 => {
                arena.remove_subtree(child);
            }
            _ => {}
        }
        let roots = if frame == 7 {
            vec![other]
        } else {
            vec![root, other]
        };
        trees.sync(&arena, &roots);
        compare(&mut cached, &mut full, &arena, &roots, &trees);
        if frame == 1 {
            assert_eq!(cached.native_observation_replays, 3);
        }
    }
    let mut foreign = NodeArena::new();
    let foreign_root = commit_element(&mut foreign, Box::new(Element::new(0., 0., 50., 50.)));
    assert_eq!(foreign_root, root);
    trees.sync(&foreign, &[foreign_root]);
    compare(&mut cached, &mut full, &foreign, &[foreign_root], &trees);
    assert_eq!(cached.native_observation_replays, 0);
}

#[derive(Clone)]
struct LiveColor(Rc<Cell<[f32; 4]>>);
impl ColorLike for LiveColor {
    fn box_clone(&self) -> Box<dyn ColorLike> {
        Box::new(self.clone())
    }
    fn to_rgba_f32(&self) -> [f32; 4] {
        self.0.get()
    }
}
#[test]
fn native_observation_replay_keeps_external_color_and_manual_observations_live() {
    let mut arena = NodeArena::new();
    let root = commit_element(&mut arena, Box::new(Element::new(0., 0., 10., 10.)));
    let live = LiveColor(Rc::new(Cell::new([1., 0., 0., 1.])));
    get_element_mut::<Element>(&arena, root).set_background_color(live.clone());
    let mut trees = PropertyTrees::default();
    trees.sync(&arena, &[root]);
    let mut cached = PaintGenerationTracker::default();
    let mut full = PaintGenerationTracker::default();
    compare(&mut cached, &mut full, &arena, &[root], &trees);
    let previous = cached.local_generations_for(root).unwrap();
    let clock = arena.mutation_clock();
    live.0.set([0., 0., 1., 1.]);
    compare(&mut cached, &mut full, &arena, &[root], &trees);
    assert_eq!(arena.mutation_clock(), clock);
    assert_eq!(cached.native_observation_replays, 0);
    assert_ne!(cached.local_generations_for(root).unwrap(), previous);
    get_element_mut::<Element>(&arena, root).set_background_color(Color::rgb(1, 2, 3));
    compare(&mut cached, &mut full, &arena, &[root], &trees);
    let node = arena.get(root).unwrap();
    cached.observe_node(
        root,
        node.parent(),
        node.children(),
        node.element.as_ref(),
        &trees,
    );
    full.observe_node(
        root,
        node.parent(),
        node.children(),
        node.element.as_ref(),
        &trees,
    );
    assert!(cached.nodes[&root].native_observation.is_none());
    compare(&mut cached, &mut full, &arena, &[root], &trees);
    assert_eq!(cached.native_observation_replays, 0);
}

#[test]
fn current_property_generations_are_read_without_relying_on_dirty_hints() {
    let mut arena = NodeArena::new();
    let root = commit_element(&mut arena, Box::new(Element::new(0., 0., 10., 10.)));
    get_element_mut::<Element>(&arena, root).set_opacity(0.5);
    let mut trees = PropertyTrees::default();
    trees.sync(&arena, &[root]);
    let mut cached = PaintGenerationTracker::default();
    let mut full = PaintGenerationTracker::default();
    compare(&mut cached, &mut full, &arena, &[root], &trees);
    compare(&mut cached, &mut full, &arena, &[root], &trees);
    assert_eq!(cached.native_observation_replays, 1);
    // A different complete property observation is supplied while the native
    // arena and its mutation revision stay identical; stale hints cannot bless it.
    let different = PropertyTrees::default();
    compare(&mut cached, &mut full, &arena, &[root], &different);
    assert_eq!(cached.native_observation_replays, 0);
}

#[test]
fn sparse_updates_reuse_reachability_and_keep_full_observation_revision_order() {
    let mut arena = NodeArena::new();
    let root = commit_element(&mut arena, Box::new(Element::new(0., 0., 100., 100.)));
    let children = (0..12)
        .map(|_| commit_child(&mut arena, root, Box::new(Element::new(0., 0., 10., 10.))))
        .collect::<Vec<_>>();
    let mut trees = PropertyTrees::default();
    let mut cached = PaintGenerationTracker::default();
    let mut full = PaintGenerationTracker::default();
    trees.sync(&arena, &[root]);
    compare(&mut cached, &mut full, &arena, &[root], &trees);
    let epoch = cached.epoch();
    for frame in 0..80 {
        // Write in reverse traversal order; revision allocation must still use
        // the original walk, including simultaneous paint and effect changes.
        for index in [9, 3] {
            let mut element = get_element_mut::<Element>(&arena, children[index]);
            element.set_opacity(if frame % 2 == 0 { 0.5 } else { 0.75 });
            element.set_background_color(Color::rgb(frame, index as u8, 30));
        }
        trees.sync(&arena, &[root]);
        compare(&mut cached, &mut full, &arena, &[root], &trees);
        assert_eq!(
            cached.epoch(),
            epoch,
            "unchanged reachability needs no sweep"
        );
        assert_eq!(cached.native_observation_replays, 11);
    }
}

#[test]
fn lost_history_rebuilds_and_removed_unreachable_records_do_not_accumulate() {
    let mut arena = NodeArena::new();
    let root = commit_element(&mut arena, Box::new(Element::new(0., 0., 100., 100.)));
    let other = commit_element(&mut arena, Box::new(Element::new(0., 0., 10., 10.)));
    let mut trees = PropertyTrees::default();
    let mut cached = PaintGenerationTracker::default();
    let mut full = PaintGenerationTracker::default();
    for roots in [&[root, other][..], &[root][..]] {
        trees.sync(&arena, roots);
        compare(&mut cached, &mut full, &arena, roots, &trees);
    }
    assert!(!cached.snapshot(other).unwrap().active);
    arena.remove(other);
    compare(&mut cached, &mut full, &arena, &[root], &trees);
    assert!(cached.snapshot(other).is_none());
    let epoch = cached.epoch();
    for _ in 0..4100 {
        drop(arena.get_mut(root));
    }
    trees.sync(&arena, &[root]);
    compare(&mut cached, &mut full, &arena, &[root], &trees);
    assert_ne!(
        cached.epoch(),
        epoch,
        "missing history requires the full walk"
    );
    assert!(
        cached.native_scene.is_some(),
        "a successful full walk can reseal"
    );
}
