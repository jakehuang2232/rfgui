//! Renderer-neutral retained paint-generation observations.
//!
//! Stable, `NodeKey`-scoped revisions let retained planning validate paint,
//! composite, and topology identity against one coherent live snapshot.

#![allow(dead_code)]

use rustc_hash::{FxHashMap, FxHashSet};

use super::PropertyTrees;
use crate::view::base_component::ElementTrait;
use crate::view::node_arena::{NodeArena, NodeKey};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PaintGenerationCoverage {
    /// A host whose complete retained paint signature is observed.
    RetainedSignatureObserved,
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

/// Why a [`PaintGenerationTracker`] no longer describes the live arena.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LiveSnapshotMismatch {
    /// The node that disagreed, or `None` for a whole-scene mismatch.
    pub(crate) owner: Option<NodeKey>,
    pub(crate) field: LiveSnapshotField,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LiveSnapshotField {
    ObservedRoots,
    RepeatedNode,
    MissingNode,
    MissingRecord,
    InactiveRecord,
    Epoch,
    Parent,
    Children,
    Coverage,
    SelfSignature,
    TransformGeneration,
    EffectGeneration,
    ScrollGeneration,
    UnreachableRecord,
}

impl LiveSnapshotField {
    /// Stable lowercase code for debug reporting.
    pub(crate) fn code(self) -> &'static str {
        match self {
            Self::ObservedRoots => "live-snapshot-observed-roots",
            Self::RepeatedNode => "live-snapshot-repeated-node",
            Self::MissingNode => "live-snapshot-missing-node",
            Self::MissingRecord => "live-snapshot-missing-record",
            Self::InactiveRecord => "live-snapshot-inactive-record",
            Self::Epoch => "live-snapshot-epoch",
            Self::Parent => "live-snapshot-parent",
            Self::Children => "live-snapshot-children",
            Self::Coverage => "live-snapshot-coverage",
            Self::SelfSignature => "live-snapshot-self-signature",
            Self::TransformGeneration => "live-snapshot-transform-generation",
            Self::EffectGeneration => "live-snapshot-effect-generation",
            Self::ScrollGeneration => "live-snapshot-scroll-generation",
            Self::UnreachableRecord => "live-snapshot-unreachable-record",
        }
    }
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
        self.live_snapshot_mismatch(arena, roots, property_trees)
            .is_none()
    }

    /// Every reason this tracker no longer describes the live arena.
    ///
    /// [`Self::live_snapshot_mismatch`] stops at the first disagreement
    /// because the planners that gate on it only need a yes or no, and they
    /// run every frame. Diagnostics want the whole set: a planner that rejects
    /// on this precondition returns before its per-node validation runs at
    /// all, so one drifting node anywhere suppresses every other reason in the
    /// scene. Collecting them turns that into one pass instead of one node per
    /// run.
    ///
    /// Callers pay a full traversal, so this belongs behind a diagnostic
    /// switch rather than on a planning path.
    pub(crate) fn live_snapshot_mismatches(
        &self,
        arena: &NodeArena,
        roots: &[NodeKey],
        property_trees: &PropertyTrees,
    ) -> Vec<LiveSnapshotMismatch> {
        let mut mismatches = Vec::new();
        if self.observed_roots.as_slice() != roots {
            mismatches.push(LiveSnapshotMismatch {
                owner: None,
                field: LiveSnapshotField::ObservedRoots,
            });
        }
        let mut stack = roots.to_vec();
        let mut seen = FxHashSet::default();
        while let Some(key) = stack.pop() {
            if !seen.insert(key) {
                mismatches.push(LiveSnapshotMismatch {
                    owner: Some(key),
                    field: LiveSnapshotField::RepeatedNode,
                });
                continue;
            }
            let Some(node) = arena.get(key) else {
                mismatches.push(LiveSnapshotMismatch {
                    owner: Some(key),
                    field: LiveSnapshotField::MissingNode,
                });
                continue;
            };
            let children = node.children();
            stack.extend(children.iter().copied());
            let Some(record) = self.nodes.get(&key) else {
                mismatches.push(LiveSnapshotMismatch {
                    owner: Some(key),
                    field: LiveSnapshotField::MissingRecord,
                });
                continue;
            };
            let field = if !record.active {
                Some(LiveSnapshotField::InactiveRecord)
            } else if record.last_seen_epoch != self.epoch {
                Some(LiveSnapshotField::Epoch)
            } else if record.observed_parent != node.parent() {
                Some(LiveSnapshotField::Parent)
            } else if record.observed_children.as_slice() != children {
                Some(LiveSnapshotField::Children)
            } else if record.coverage != coverage_for(node.element.as_ref()) {
                Some(LiveSnapshotField::Coverage)
            } else if record.observed_self_signature != node.element.retained_paint_signature() {
                Some(LiveSnapshotField::SelfSignature)
            } else if record.observed_transform_generation
                != property_trees.transform_generation_for_owner(key)
            {
                Some(LiveSnapshotField::TransformGeneration)
            } else if record.observed_effect_generation
                != property_trees.effect_generation_for_owner(key)
            {
                Some(LiveSnapshotField::EffectGeneration)
            } else if record.observed_scroll_generation
                != property_trees.scroll_generation_for_owner(key)
            {
                Some(LiveSnapshotField::ScrollGeneration)
            } else {
                None
            };
            if let Some(field) = field {
                mismatches.push(LiveSnapshotMismatch {
                    owner: Some(key),
                    field,
                });
            }
        }
        let mut unreachable = self
            .nodes
            .iter()
            .filter(|(key, record)| {
                record.active && record.last_seen_epoch == self.epoch && !seen.contains(key)
            })
            .map(|(key, _)| *key)
            .collect::<Vec<_>>();
        // `nodes` is a hash map, so fix an order before reporting.
        unreachable.sort_unstable();
        mismatches.extend(unreachable.into_iter().map(|key| LiveSnapshotMismatch {
            owner: Some(key),
            field: LiveSnapshotField::UnreachableRecord,
        }));
        mismatches
    }

    /// The first reason this tracker no longer describes the live arena, or
    /// `None` when it still does.
    ///
    /// [`Self::matches_live_snapshot`] answers the same question as a boolean
    /// and discards everything else. A whole-tree equality check that fails
    /// somewhere in a few thousand nodes is not actionable on its own, so this
    /// reports the node and the field that first disagreed. Traversal order is
    /// deterministic, so the reported node is stable for a given snapshot.
    pub(crate) fn live_snapshot_mismatch(
        &self,
        arena: &NodeArena,
        roots: &[NodeKey],
        property_trees: &PropertyTrees,
    ) -> Option<LiveSnapshotMismatch> {
        let drift = |owner: Option<NodeKey>, field: LiveSnapshotField| {
            Some(LiveSnapshotMismatch { owner, field })
        };
        if self.observed_roots.as_slice() != roots {
            return drift(None, LiveSnapshotField::ObservedRoots);
        }
        let mut stack = roots.to_vec();
        let mut seen = FxHashSet::default();
        while let Some(key) = stack.pop() {
            if !seen.insert(key) {
                return drift(Some(key), LiveSnapshotField::RepeatedNode);
            }
            let Some(node) = arena.get(key) else {
                return drift(Some(key), LiveSnapshotField::MissingNode);
            };
            let Some(record) = self.nodes.get(&key) else {
                return drift(Some(key), LiveSnapshotField::MissingRecord);
            };
            let children = node.children();
            let field = if !record.active {
                Some(LiveSnapshotField::InactiveRecord)
            } else if record.last_seen_epoch != self.epoch {
                Some(LiveSnapshotField::Epoch)
            } else if record.observed_parent != node.parent() {
                Some(LiveSnapshotField::Parent)
            } else if record.observed_children.as_slice() != children {
                Some(LiveSnapshotField::Children)
            } else if record.coverage != coverage_for(node.element.as_ref()) {
                Some(LiveSnapshotField::Coverage)
            } else if record.observed_self_signature != node.element.retained_paint_signature() {
                Some(LiveSnapshotField::SelfSignature)
            } else if record.observed_transform_generation
                != property_trees.transform_generation_for_owner(key)
            {
                Some(LiveSnapshotField::TransformGeneration)
            } else if record.observed_effect_generation
                != property_trees.effect_generation_for_owner(key)
            {
                Some(LiveSnapshotField::EffectGeneration)
            } else if record.observed_scroll_generation
                != property_trees.scroll_generation_for_owner(key)
            {
                Some(LiveSnapshotField::ScrollGeneration)
            } else {
                None
            };
            if let Some(field) = field {
                return drift(Some(key), field);
            }
            stack.extend(children.iter().copied());
        }
        self.nodes
            .iter()
            .find(|(key, record)| {
                record.active && record.last_seen_epoch == self.epoch && !seen.contains(key)
            })
            .map(|(key, _)| LiveSnapshotMismatch {
                owner: Some(*key),
                field: LiveSnapshotField::UnreachableRecord,
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
        property_trees: &PropertyTrees,
    ) -> LocalPaintGenerations {
        let self_signature = element.retained_paint_signature();
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
        self.observe_node(
            key,
            parent,
            &children,
            node.element.as_ref(),
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
    if element.retained_paint_signature_is_complete() {
        PaintGenerationCoverage::RetainedSignatureObserved
    } else {
        PaintGenerationCoverage::Untracked
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod live_snapshot_mismatch_tests;
