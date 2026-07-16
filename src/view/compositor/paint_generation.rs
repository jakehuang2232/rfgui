//! Shadow paint-generation observations.
//!
//! Stable, `NodeKey`-scoped revisions participate in promotion reuse as an
//! additional veto. A layer reuses only when both its legacy signature and
//! generation match; generations never authorize reuse that the signature
//! rejects, so the existing signature path remains the correctness authority.

#![allow(dead_code)]

use rustc_hash::{FxHashMap, FxHashSet};

use super::PropertyTrees;
use crate::view::base_component::ElementTrait;
use crate::view::node_arena::{NodeArena, NodeKey};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PaintGenerationCoverage {
    /// A built-in host whose current promotion signature is observed as the
    /// temporary correctness oracle. This is not mutation-complete coverage.
    SignatureObserved,
    /// An external-resource or custom host without a complete paint identity.
    /// Its local paint revision advances every observed frame.
    Untracked,
}

#[derive(Clone, Debug)]
struct NodeGenerationRecord {
    self_paint_revision: u64,
    composite_revision: u64,
    topology_revision: u64,
    observed_self_signature: u64,
    observed_transform_generation: Option<u64>,
    observed_effect_generation: Option<u64>,
    observed_scroll_generation: Option<u64>,
    observed_parent: Option<NodeKey>,
    observed_children: Vec<NodeKey>,
    coverage: PaintGenerationCoverage,
    active: bool,
    last_seen_epoch: u64,
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PaintGenerationSnapshot {
    pub(crate) self_paint_revision: u64,
    pub(crate) composite_revision: u64,
    pub(crate) topology_revision: u64,
    pub(crate) observed_self_signature: u64,
    pub(crate) observed_transform_generation: Option<u64>,
    pub(crate) observed_effect_generation: Option<u64>,
    pub(crate) observed_scroll_generation: Option<u64>,
    pub(crate) observed_parent: Option<NodeKey>,
    pub(crate) observed_children: Vec<NodeKey>,
    pub(crate) coverage: PaintGenerationCoverage,
    pub(crate) active: bool,
}

#[derive(Default)]
pub(crate) struct PaintGenerationTracker {
    next_revision: u64,
    nodes: FxHashMap<NodeKey, NodeGenerationRecord>,
    observed_roots: Vec<NodeKey>,
    root_topology_revision: u64,
    epoch: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LocalPaintGenerations {
    pub(crate) self_paint_revision: u64,
    pub(crate) composite_revision: u64,
    pub(crate) topology_revision: u64,
}

impl PaintGenerationTracker {
    pub(crate) fn local_generations_for(&self, key: NodeKey) -> Option<LocalPaintGenerations> {
        self.nodes
            .get(&key)
            .filter(|record| record.active)
            .map(|record| LocalPaintGenerations {
                self_paint_revision: record.self_paint_revision,
                composite_revision: record.composite_revision,
                topology_revision: record.topology_revision,
            })
    }

    /// Read-only proof that this tracker and the supplied property trees still
    /// describe the exact live arena snapshot they observed. Retained scene
    /// preparation uses this immediately before recording so a stale
    /// generation map cannot bless freshly changed paint payloads.
    pub(crate) fn matches_live_snapshot(
        &self,
        arena: &NodeArena,
        roots: &[NodeKey],
        property_trees: &PropertyTrees,
    ) -> bool {
        if self.observed_roots.as_slice() != roots {
            return false;
        }
        let mut stack = roots.to_vec();
        let mut seen = FxHashSet::default();
        while let Some(key) = stack.pop() {
            if !seen.insert(key) {
                return false;
            }
            let Some(node) = arena.get(key) else {
                return false;
            };
            let Some(record) = self.nodes.get(&key) else {
                return false;
            };
            let children = node.children();
            if !record.active
                || record.last_seen_epoch != self.epoch
                || record.observed_parent != node.parent()
                || record.observed_children.as_slice() != children
                || record.coverage != coverage_for(node.element.as_ref())
                || record.observed_self_signature != node.element.promotion_self_signature()
                || record.observed_transform_generation
                    != property_trees.transform_generation_for_owner(key)
                || record.observed_effect_generation
                    != property_trees.effect_generation_for_owner(key)
                || record.observed_scroll_generation
                    != property_trees.scroll_generation_for_owner(key)
            {
                return false;
            }
            stack.extend(children.iter().copied());
        }
        self.nodes.iter().all(|(key, record)| {
            !record.active || record.last_seen_epoch != self.epoch || seen.contains(key)
        })
    }

    pub(crate) fn begin_frame(&mut self, roots: &[NodeKey]) {
        self.epoch = self.epoch.wrapping_add(1);
        if self.observed_roots != roots {
            self.root_topology_revision = self.allocate_revision();
            self.observed_roots.clear();
            self.observed_roots.extend_from_slice(roots);
        }
    }

    pub(crate) fn observe_node(
        &mut self,
        key: NodeKey,
        parent: Option<NodeKey>,
        children: &[NodeKey],
        element: &dyn ElementTrait,
        self_signature: u64,
        property_trees: &PropertyTrees,
    ) -> LocalPaintGenerations {
        let coverage = coverage_for(element);
        let transform_generation = property_trees.transform_generation_for_owner(key);
        let effect_generation = property_trees.effect_generation_for_owner(key);
        let scroll_generation = property_trees.scroll_generation_for_owner(key);

        let (self_paint_revision, composite_revision, topology_revision) =
            if self.nodes.contains_key(&key) {
                let (
                    self_changed,
                    composite_changed,
                    topology_changed,
                    previous_self_paint_revision,
                    previous_composite_revision,
                    previous_topology_revision,
                ) = {
                    let previous = &self.nodes[&key];
                    (
                        coverage == PaintGenerationCoverage::Untracked
                            || previous.coverage != coverage
                            || previous.observed_self_signature != self_signature
                            || previous.observed_transform_generation != transform_generation
                            || previous.observed_scroll_generation != scroll_generation,
                        previous.coverage != coverage
                            || previous.observed_effect_generation != effect_generation,
                        !previous.active
                            || previous.observed_parent != parent
                            || previous.observed_children != children,
                        previous.self_paint_revision,
                        previous.composite_revision,
                        previous.topology_revision,
                    )
                };

                (
                    if self_changed {
                        self.allocate_revision()
                    } else {
                        previous_self_paint_revision
                    },
                    if composite_changed {
                        self.allocate_revision()
                    } else {
                        previous_composite_revision
                    },
                    if topology_changed {
                        self.allocate_revision()
                    } else {
                        previous_topology_revision
                    },
                )
            } else {
                (
                    self.allocate_revision(),
                    self.allocate_revision(),
                    self.allocate_revision(),
                )
            };

        if let Some(record) = self.nodes.get_mut(&key) {
            record.self_paint_revision = self_paint_revision;
            record.composite_revision = composite_revision;
            record.topology_revision = topology_revision;
            record.observed_self_signature = self_signature;
            record.observed_transform_generation = transform_generation;
            record.observed_effect_generation = effect_generation;
            record.observed_scroll_generation = scroll_generation;
            record.observed_parent = parent;
            if record.observed_children.as_slice() != children {
                record.observed_children.clear();
                record.observed_children.extend_from_slice(children);
            }
            record.coverage = coverage;
            record.active = true;
            record.last_seen_epoch = self.epoch;
        } else {
            self.nodes.insert(
                key,
                NodeGenerationRecord {
                    self_paint_revision,
                    composite_revision,
                    topology_revision,
                    observed_self_signature: self_signature,
                    observed_transform_generation: transform_generation,
                    observed_effect_generation: effect_generation,
                    observed_scroll_generation: scroll_generation,
                    observed_parent: parent,
                    observed_children: children.to_vec(),
                    coverage,
                    active: true,
                    last_seen_epoch: self.epoch,
                },
            );
        }

        LocalPaintGenerations {
            self_paint_revision,
            composite_revision,
            topology_revision,
        }
    }

    pub(crate) fn finish_frame(&mut self, arena: &NodeArena) {
        let newly_inactive = self
            .nodes
            .iter()
            .filter_map(|(&key, record)| {
                (record.active && record.last_seen_epoch != self.epoch).then_some(key)
            })
            .collect::<Vec<_>>();
        for key in newly_inactive {
            let revision = self.allocate_revision();
            if let Some(record) = self.nodes.get_mut(&key) {
                record.active = false;
                record.topology_revision = revision;
            }
        }

        self.nodes.retain(|key, _| arena.contains_key(*key));
    }

    pub(crate) fn root_topology_revision_value(&self) -> u64 {
        self.root_topology_revision
    }

    #[cfg(test)]
    pub(crate) fn sync(
        &mut self,
        arena: &NodeArena,
        roots: &[NodeKey],
        property_trees: &PropertyTrees,
    ) {
        self.begin_frame(roots);

        let mut seen = FxHashSet::default();
        for &root in roots {
            self.sync_subtree(arena, root, property_trees, &mut seen);
        }

        self.finish_frame(arena);
    }

    fn sync_subtree(
        &mut self,
        arena: &NodeArena,
        key: NodeKey,
        property_trees: &PropertyTrees,
        seen: &mut FxHashSet<NodeKey>,
    ) {
        if !seen.insert(key) {
            return;
        }
        let Some(node) = arena.get(key) else {
            return;
        };

        let parent = node.parent();
        let children = node.children().to_vec();
        let self_signature = node.element.promotion_self_signature();
        self.observe_node(
            key,
            parent,
            &children,
            node.element.as_ref(),
            self_signature,
            property_trees,
        );
        drop(node);

        for child in children {
            self.sync_subtree(arena, child, property_trees, seen);
        }
    }

    fn allocate_revision(&mut self) -> u64 {
        self.next_revision = self.next_revision.wrapping_add(1);
        if self.next_revision == 0 {
            // Equality is the only semantic requirement. Starting a fresh
            // epoch is safer than allowing the wrapped value to alias an
            // ancient live record.
            self.nodes.clear();
            self.next_revision = 1;
        }
        self.next_revision
    }

    #[cfg(test)]
    pub(crate) fn snapshot(&self, key: NodeKey) -> Option<PaintGenerationSnapshot> {
        self.nodes.get(&key).map(|record| PaintGenerationSnapshot {
            self_paint_revision: record.self_paint_revision,
            composite_revision: record.composite_revision,
            topology_revision: record.topology_revision,
            observed_self_signature: record.observed_self_signature,
            observed_transform_generation: record.observed_transform_generation,
            observed_effect_generation: record.observed_effect_generation,
            observed_scroll_generation: record.observed_scroll_generation,
            observed_parent: record.observed_parent,
            observed_children: record.observed_children.clone(),
            coverage: record.coverage,
            active: record.active,
        })
    }

    #[cfg(test)]
    pub(crate) fn epoch(&self) -> u64 {
        self.epoch
    }

    #[cfg(test)]
    pub(crate) fn root_topology_revision(&self) -> u64 {
        self.root_topology_revision
    }

    #[cfg(test)]
    fn observed_children_storage(&self, key: NodeKey) -> Option<(usize, usize)> {
        self.nodes.get(&key).map(|record| {
            (
                record.observed_children.as_ptr() as usize,
                record.observed_children.capacity(),
            )
        })
    }
}

fn coverage_for(element: &dyn ElementTrait) -> PaintGenerationCoverage {
    if element.promotion_signature_is_complete() {
        PaintGenerationCoverage::SignatureObserved
    } else {
        PaintGenerationCoverage::Untracked
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::{
        Length, ParsedValue, PropertyId, ScrollDirection, Style, Transform, Translate,
    };
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
    fn custom_default_is_untracked_and_never_looks_stable() {
        let mut arena = NodeArena::new();
        let root = arena.insert(Node::new(Box::new(CustomHost)));
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
    }
}
