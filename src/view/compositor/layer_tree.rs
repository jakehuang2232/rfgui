//! Retained shadow compositor topology.
//!
//! This module deliberately does not own rendering or GPU resources.  It
//! mirrors the current promotion decision and paint-recording boundaries so
//! topology identity can be validated before the legacy renderer is replaced.

#![allow(dead_code)]

use rustc_hash::{FxHashMap, FxHashSet};

use crate::view::base_component::Rect;
use crate::view::node_arena::NodeKey;
use crate::view::paint::{
    LegacyPaintReason, PaintChunkId, PaintContentRevision, PaintCoverageValidationError,
};
use crate::view::promotion::PromotionHardReason;

use super::property_tree::PropertyTreeState;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum LayerId {
    SceneRoot,
    Promoted(NodeKey),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CompositingReason {
    SceneRoot,
    Hard(PromotionHardReason),
    Heuristic { score: i32, threshold: i32 },
}

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct PaintOrderKey {
    pub(crate) root_index: usize,
    pub(crate) child_path: Vec<usize>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct LayerBounds {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
    pub(crate) corner_radii: [f32; 4],
}

impl LayerBounds {
    pub(crate) fn scene() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
            corner_radii: [0.0; 4],
        }
    }
}

impl PartialEq for LayerBounds {
    fn eq(&self, other: &Self) -> bool {
        self.x.to_bits() == other.x.to_bits()
            && self.y.to_bits() == other.y.to_bits()
            && self.width.to_bits() == other.width.to_bits()
            && self.height.to_bits() == other.height.to_bits()
            && self
                .corner_radii
                .iter()
                .zip(other.corner_radii.iter())
                .all(|(left, right)| left.to_bits() == right.to_bits())
    }
}

impl Eq for LayerBounds {}

#[derive(Clone, Debug)]
pub(crate) struct RetainedPaintChunk {
    pub(crate) id: PaintChunkId,
    pub(crate) owner: NodeKey,
    pub(crate) bounds: Rect,
    pub(crate) properties: PropertyTreeState,
    pub(crate) content_revision: PaintContentRevision,
    pub(crate) payload_identity: crate::view::paint::PaintPayloadIdentity,
}

impl PartialEq for RetainedPaintChunk {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.owner == other.owner
            && rect_eq(self.bounds, other.bounds)
            && self.properties == other.properties
            && self.content_revision == other.content_revision
            && self.payload_identity == other.payload_identity
    }
}

impl Eq for RetainedPaintChunk {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OpaqueLegacyRevision(pub(crate) u64);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum LayerItem {
    PaintChunk {
        artifact_root: NodeKey,
        chunk: RetainedPaintChunk,
    },
    LegacySpan {
        boundary_root: NodeKey,
        stable_id: u64,
        reason: LegacyPaintReason,
        span_index: usize,
        before_cutout: Option<LayerId>,
        after_cutout: Option<LayerId>,
        revision: OpaqueLegacyRevision,
    },
    PromotedChild(LayerId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PendingLayer {
    pub(crate) id: LayerId,
    pub(crate) owner: Option<NodeKey>,
    pub(crate) stable_id: Option<u64>,
    pub(crate) parent: Option<LayerId>,
    pub(crate) paint_order: PaintOrderKey,
    pub(crate) composition_path: Vec<NodeKey>,
    pub(crate) reason: CompositingReason,
    pub(crate) bounds: LayerBounds,
    pub(crate) properties: PropertyTreeState,
    pub(crate) raster_revision: u64,
    pub(crate) composite_revision: u64,
    pub(crate) topology_revision: u64,
    pub(crate) items: Vec<LayerItem>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CompositorLayer {
    pub(crate) id: LayerId,
    pub(crate) owner: Option<NodeKey>,
    pub(crate) stable_id: Option<u64>,
    pub(crate) parent: Option<LayerId>,
    pub(crate) paint_order: PaintOrderKey,
    pub(crate) composition_path: Vec<NodeKey>,
    pub(crate) reason: CompositingReason,
    pub(crate) bounds: LayerBounds,
    pub(crate) properties: PropertyTreeState,
    pub(crate) raster_revision: u64,
    pub(crate) composite_revision: u64,
    pub(crate) topology_revision: u64,
    pub(crate) items: Vec<LayerItem>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct LayerTreeDiff {
    pub(crate) added: Vec<LayerId>,
    pub(crate) removed: Vec<LayerId>,
    pub(crate) reparented: Vec<LayerId>,
    pub(crate) reordered: Vec<LayerId>,
    pub(crate) raster_changed: Vec<LayerId>,
    pub(crate) composite_changed: Vec<LayerId>,
    pub(crate) topology_changed: Vec<LayerId>,
    pub(crate) metadata_changed: Vec<LayerId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum LayerTreeValidationError {
    MissingStableId {
        stable_id: u64,
    },
    DuplicateStableId {
        stable_id: u64,
    },
    MissingPromotionDecision {
        stable_id: u64,
    },
    MissingPromotionUpdate {
        stable_id: u64,
    },
    DuplicatePaintChunk {
        id: PaintChunkId,
    },
    InvalidPromotedAncestor {
        stable_id: u64,
        ancestor_stable_id: u64,
    },
    MissingParentLayer {
        layer: LayerId,
        parent: LayerId,
    },
    PaintCoverage(PaintCoverageValidationError),
    InvalidManifestCutoutParent {
        cutout: LayerId,
        manifest_parent: LayerId,
        actual_parent: Option<LayerId>,
    },
}

#[derive(Default)]
pub(crate) struct LayerTree {
    pub(crate) epoch: u64,
    pub(crate) layers: FxHashMap<LayerId, CompositorLayer>,
    pub(crate) last_diff: LayerTreeDiff,
    pub(crate) validation_errors: Vec<LayerTreeValidationError>,
}

impl LayerTree {
    pub(crate) fn reconcile(
        &mut self,
        pending: Vec<PendingLayer>,
        mut validation_errors: Vec<LayerTreeValidationError>,
    ) {
        self.epoch = self.epoch.wrapping_add(1);
        let mut next = pending
            .into_iter()
            .map(|layer| {
                (
                    layer.id,
                    CompositorLayer {
                        id: layer.id,
                        owner: layer.owner,
                        stable_id: layer.stable_id,
                        parent: layer.parent,
                        paint_order: layer.paint_order,
                        composition_path: layer.composition_path,
                        reason: layer.reason,
                        bounds: layer.bounds,
                        properties: layer.properties,
                        raster_revision: layer.raster_revision,
                        composite_revision: layer.composite_revision,
                        topology_revision: layer.topology_revision,
                        items: layer.items,
                    },
                )
            })
            .collect::<FxHashMap<_, _>>();

        loop {
            let invalid = next
                .iter()
                .filter_map(|(&id, layer)| {
                    if id == LayerId::SceneRoot {
                        return None;
                    }
                    let parent = layer.parent.unwrap_or(LayerId::SceneRoot);
                    (!next.contains_key(&parent)).then_some((id, parent))
                })
                .collect::<Vec<_>>();
            if invalid.is_empty() {
                break;
            }
            for (layer, parent) in invalid {
                next.remove(&layer);
                validation_errors
                    .push(LayerTreeValidationError::MissingParentLayer { layer, parent });
            }
        }

        let parents = next
            .iter()
            .map(|(&id, layer)| (id, layer.parent))
            .collect::<FxHashMap<_, _>>();
        for (&parent_id, layer) in &mut next {
            layer.items.retain(|item| match item {
                LayerItem::PromotedChild(child) => {
                    parents.get(child).copied().flatten() == Some(parent_id)
                }
                _ => true,
            });
        }

        self.last_diff = diff_layers(&self.layers, &next);
        self.layers = next;
        self.validation_errors = validation_errors;
    }

    #[cfg(test)]
    pub(crate) fn layer(&self, id: LayerId) -> Option<&CompositorLayer> {
        self.layers.get(&id)
    }

    pub(crate) fn root_children(&self) -> Vec<LayerId> {
        child_sequence(&self.layers, LayerId::SceneRoot)
    }
}

fn diff_layers(
    previous: &FxHashMap<LayerId, CompositorLayer>,
    next: &FxHashMap<LayerId, CompositorLayer>,
) -> LayerTreeDiff {
    let previous_ids = previous.keys().copied().collect::<FxHashSet<_>>();
    let next_ids = next.keys().copied().collect::<FxHashSet<_>>();
    let mut diff = LayerTreeDiff {
        added: next_ids.difference(&previous_ids).copied().collect(),
        removed: previous_ids.difference(&next_ids).copied().collect(),
        ..LayerTreeDiff::default()
    };
    for id in previous_ids.intersection(&next_ids).copied() {
        let old = &previous[&id];
        let new = &next[&id];
        if old.parent != new.parent {
            diff.reparented.push(id);
        }
        if old.raster_revision != new.raster_revision || !raster_items_eq(&old.items, &new.items) {
            diff.raster_changed.push(id);
        }
        if old.composite_revision != new.composite_revision
            || old.properties != new.properties
            || old.bounds != new.bounds
        {
            diff.composite_changed.push(id);
        }
        if old.topology_revision != new.topology_revision
            || old.composition_path != new.composition_path
            || !topology_items_eq(&old.items, &new.items)
        {
            diff.topology_changed.push(id);
        }
        if old.owner != new.owner
            || old.stable_id != new.stable_id
            || old.reason != new.reason
            || !metadata_items_eq(&old.items, &new.items)
        {
            diff.metadata_changed.push(id);
        }
    }
    collect_relative_reorders(previous, next, &mut diff.reordered);
    sort_diff(&mut diff);
    diff
}

fn sort_diff(diff: &mut LayerTreeDiff) {
    fn key(id: &LayerId) -> String {
        format!("{id:?}")
    }
    diff.added.sort_by_key(key);
    diff.removed.sort_by_key(key);
    diff.reparented.sort_by_key(key);
    diff.reordered.sort_by_key(key);
    diff.raster_changed.sort_by_key(key);
    diff.composite_changed.sort_by_key(key);
    diff.topology_changed.sort_by_key(key);
    diff.metadata_changed.sort_by_key(key);
}

fn collect_relative_reorders(
    previous: &FxHashMap<LayerId, CompositorLayer>,
    next: &FxHashMap<LayerId, CompositorLayer>,
    out: &mut Vec<LayerId>,
) {
    for &parent in previous.keys().filter(|id| next.contains_key(id)) {
        let old = child_sequence(previous, parent);
        let new = child_sequence(next, parent);
        let old_set = old.iter().copied().collect::<FxHashSet<_>>();
        let new_set = new.iter().copied().collect::<FxHashSet<_>>();
        let old_retained = old
            .into_iter()
            .filter(|id| new_set.contains(id))
            .collect::<Vec<_>>();
        let new_retained = new
            .into_iter()
            .filter(|id| old_set.contains(id))
            .collect::<Vec<_>>();
        if old_retained != new_retained {
            out.extend(old_retained);
        }
    }
    out.sort_by_key(|id| format!("{id:?}"));
    out.dedup();
}

fn child_sequence(layers: &FxHashMap<LayerId, CompositorLayer>, parent: LayerId) -> Vec<LayerId> {
    layers
        .get(&parent)
        .into_iter()
        .flat_map(|layer| &layer.items)
        .filter_map(|item| match item {
            LayerItem::PromotedChild(id) => Some(*id),
            _ => None,
        })
        .collect()
}

fn raster_items_eq(left: &[LayerItem], right: &[LayerItem]) -> bool {
    fn eq(left: &LayerItem, right: &LayerItem) -> bool {
        match (left, right) {
            (
                LayerItem::PaintChunk { chunk: left, .. },
                LayerItem::PaintChunk { chunk: right, .. },
            ) => left == right,
            (
                LayerItem::LegacySpan {
                    boundary_root: left_root,
                    span_index: left_span,
                    revision: left_revision,
                    ..
                },
                LayerItem::LegacySpan {
                    boundary_root: right_root,
                    span_index: right_span,
                    revision: right_revision,
                    ..
                },
            ) => {
                left_root == right_root
                    && left_span == right_span
                    && left_revision == right_revision
            }
            _ => false,
        }
    }
    let left = left
        .iter()
        .filter(|item| !matches!(item, LayerItem::PromotedChild(_)))
        .collect::<Vec<_>>();
    let right = right
        .iter()
        .filter(|item| !matches!(item, LayerItem::PromotedChild(_)))
        .collect::<Vec<_>>();
    left.len() == right.len()
        && left
            .into_iter()
            .zip(right)
            .all(|(left, right)| eq(left, right))
}

fn topology_items_eq(left: &[LayerItem], right: &[LayerItem]) -> bool {
    fn eq(left: &LayerItem, right: &LayerItem) -> bool {
        match (left, right) {
            (
                LayerItem::PaintChunk {
                    artifact_root: left_root,
                    chunk: left_chunk,
                },
                LayerItem::PaintChunk {
                    artifact_root: right_root,
                    chunk: right_chunk,
                },
            ) => left_root == right_root && left_chunk.id == right_chunk.id,
            (
                LayerItem::LegacySpan {
                    boundary_root: left_root,
                    span_index: left_span,
                    before_cutout: left_before,
                    after_cutout: left_after,
                    ..
                },
                LayerItem::LegacySpan {
                    boundary_root: right_root,
                    span_index: right_span,
                    before_cutout: right_before,
                    after_cutout: right_after,
                    ..
                },
            ) => {
                left_root == right_root
                    && left_span == right_span
                    && left_before == right_before
                    && left_after == right_after
            }
            (LayerItem::PromotedChild(left), LayerItem::PromotedChild(right)) => left == right,
            _ => false,
        }
    }
    left.len() == right.len() && left.iter().zip(right).all(|(left, right)| eq(left, right))
}

fn metadata_items_eq(left: &[LayerItem], right: &[LayerItem]) -> bool {
    let left = left.iter().filter_map(|item| match item {
        LayerItem::LegacySpan {
            stable_id, reason, ..
        } => Some((*stable_id, *reason)),
        _ => None,
    });
    let right = right.iter().filter_map(|item| match item {
        LayerItem::LegacySpan {
            stable_id, reason, ..
        } => Some((*stable_id, *reason)),
        _ => None,
    });
    left.eq(right)
}

pub(super) fn rect_eq(left: Rect, right: Rect) -> bool {
    left.x.to_bits() == right.x.to_bits()
        && left.y.to_bits() == right.y.to_bits()
        && left.width.to_bits() == right.width.to_bits()
        && left.height.to_bits() == right.height.to_bits()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::base_component::Element;
    use crate::view::node_arena::{Node, NodeArena};

    fn key(arena: &mut NodeArena, stable_id: u64) -> NodeKey {
        arena.insert(Node::new(Box::new(Element::new_with_id(
            stable_id, 0.0, 0.0, 10.0, 10.0,
        ))))
    }

    fn pending(id: LayerId, parent: LayerId, order: &[usize], revision: u64) -> PendingLayer {
        let owner = match id {
            LayerId::SceneRoot => None,
            LayerId::Promoted(key) => Some(key),
        };
        PendingLayer {
            id,
            owner,
            stable_id: Some(revision),
            parent: (id != LayerId::SceneRoot).then_some(parent),
            paint_order: PaintOrderKey {
                root_index: 0,
                child_path: order.to_vec(),
            },
            composition_path: Vec::new(),
            reason: if id == LayerId::SceneRoot {
                CompositingReason::SceneRoot
            } else {
                CompositingReason::Heuristic {
                    score: 50,
                    threshold: 35,
                }
            },
            bounds: LayerBounds::scene(),
            properties: PropertyTreeState::default(),
            raster_revision: revision,
            composite_revision: revision,
            topology_revision: revision,
            items: Vec::new(),
        }
    }

    fn scene(children: &[LayerId], revision: u64) -> PendingLayer {
        let mut layer = pending(LayerId::SceneRoot, LayerId::SceneRoot, &[], revision);
        layer.items = children
            .iter()
            .copied()
            .map(LayerItem::PromotedChild)
            .collect();
        layer
    }

    #[test]
    fn reconcile_reports_local_changes_without_clearing_unrelated_layers() {
        let mut arena = NodeArena::new();
        let a = key(&mut arena, 1);
        let b = key(&mut arena, 2);
        let c = key(&mut arena, 3);
        let mut tree = LayerTree::default();
        tree.reconcile(
            vec![
                scene(&[LayerId::Promoted(a), LayerId::Promoted(b)], 1),
                pending(LayerId::Promoted(a), LayerId::SceneRoot, &[0], 1),
                pending(LayerId::Promoted(b), LayerId::SceneRoot, &[1], 1),
            ],
            Vec::new(),
        );
        tree.reconcile(
            vec![
                scene(&[LayerId::Promoted(a)], 1),
                pending(LayerId::Promoted(a), LayerId::SceneRoot, &[1], 1),
                pending(LayerId::Promoted(c), LayerId::Promoted(a), &[1, 0], 2),
            ],
            Vec::new(),
        );
        assert_eq!(tree.last_diff.added, vec![LayerId::Promoted(c)]);
        assert_eq!(tree.last_diff.removed, vec![LayerId::Promoted(b)]);
        assert!(tree.last_diff.reordered.is_empty());
        assert!(!tree.last_diff.removed.contains(&LayerId::Promoted(a)));
    }

    #[test]
    fn identical_reconcile_has_no_diff_and_reparent_is_separate_from_reorder() {
        let mut arena = NodeArena::new();
        let a = key(&mut arena, 1);
        let b = key(&mut arena, 2);
        let layers = vec![
            scene(&[LayerId::Promoted(a), LayerId::Promoted(b)], 1),
            pending(LayerId::Promoted(a), LayerId::SceneRoot, &[0], 1),
            pending(LayerId::Promoted(b), LayerId::SceneRoot, &[0, 0], 1),
        ];
        let mut tree = LayerTree::default();
        tree.reconcile(layers.clone(), Vec::new());
        tree.reconcile(layers, Vec::new());
        assert_eq!(tree.last_diff, LayerTreeDiff::default());

        tree.reconcile(
            vec![
                scene(&[LayerId::Promoted(a)], 1),
                {
                    let mut layer = pending(LayerId::Promoted(a), LayerId::SceneRoot, &[0], 1);
                    layer
                        .items
                        .push(LayerItem::PromotedChild(LayerId::Promoted(b)));
                    layer
                },
                pending(LayerId::Promoted(b), LayerId::Promoted(a), &[0, 0], 1),
            ],
            Vec::new(),
        );
        assert_eq!(tree.last_diff.reparented, vec![LayerId::Promoted(b)]);
        assert!(tree.last_diff.reordered.is_empty());
    }

    #[test]
    fn node_key_replacement_with_same_stable_id_is_remove_plus_add() {
        let mut arena = NodeArena::new();
        let old = key(&mut arena, 7);
        let mut tree = LayerTree::default();
        tree.reconcile(
            vec![
                scene(&[LayerId::Promoted(old)], 1),
                pending(LayerId::Promoted(old), LayerId::SceneRoot, &[0], 7),
            ],
            Vec::new(),
        );
        arena.remove(old);
        let replacement = key(&mut arena, 7);
        tree.reconcile(
            vec![
                scene(&[LayerId::Promoted(replacement)], 1),
                pending(LayerId::Promoted(replacement), LayerId::SceneRoot, &[0], 7),
            ],
            Vec::new(),
        );
        assert_eq!(tree.last_diff.removed, vec![LayerId::Promoted(old)]);
        assert_eq!(tree.last_diff.added, vec![LayerId::Promoted(replacement)]);
    }

    #[test]
    fn absolute_path_shift_does_not_reorder_but_promoted_sibling_swap_does() {
        let mut arena = NodeArena::new();
        let a = key(&mut arena, 1);
        let b = key(&mut arena, 2);
        let mut tree = LayerTree::default();
        tree.reconcile(
            vec![
                scene(&[LayerId::Promoted(a), LayerId::Promoted(b)], 1),
                pending(LayerId::Promoted(a), LayerId::SceneRoot, &[1], 1),
                pending(LayerId::Promoted(b), LayerId::SceneRoot, &[3], 1),
            ],
            Vec::new(),
        );
        tree.reconcile(
            vec![
                scene(&[LayerId::Promoted(a), LayerId::Promoted(b)], 1),
                pending(LayerId::Promoted(a), LayerId::SceneRoot, &[2], 1),
                pending(LayerId::Promoted(b), LayerId::SceneRoot, &[4], 1),
            ],
            Vec::new(),
        );
        assert!(tree.last_diff.reordered.is_empty());

        tree.reconcile(
            vec![
                scene(&[LayerId::Promoted(b), LayerId::Promoted(a)], 1),
                pending(LayerId::Promoted(a), LayerId::SceneRoot, &[4], 1),
                pending(LayerId::Promoted(b), LayerId::SceneRoot, &[2], 1),
            ],
            Vec::new(),
        );
        assert_eq!(
            tree.last_diff
                .reordered
                .iter()
                .copied()
                .collect::<FxHashSet<_>>(),
            FxHashSet::from_iter([LayerId::Promoted(a), LayerId::Promoted(b)])
        );
    }

    #[test]
    fn reason_change_is_metadata_and_missing_parent_is_not_normalized_to_root() {
        let mut arena = NodeArena::new();
        let a = key(&mut arena, 1);
        let missing = key(&mut arena, 2);
        let mut tree = LayerTree::default();
        tree.reconcile(
            vec![
                scene(&[LayerId::Promoted(a)], 1),
                pending(LayerId::Promoted(a), LayerId::SceneRoot, &[0], 1),
            ],
            Vec::new(),
        );
        let mut changed = pending(LayerId::Promoted(a), LayerId::SceneRoot, &[0], 1);
        changed.reason = CompositingReason::Hard(PromotionHardReason::ActiveOpacityAnimation);
        let orphan = pending(
            LayerId::Promoted(missing),
            LayerId::Promoted(key(&mut arena, 3)),
            &[1],
            1,
        );
        let orphan_id = orphan.id;
        let orphan_parent = orphan.parent.unwrap();
        tree.reconcile(
            vec![scene(&[LayerId::Promoted(a)], 1), changed, orphan],
            Vec::new(),
        );
        assert_eq!(tree.last_diff.metadata_changed, vec![LayerId::Promoted(a)]);
        assert!(!tree.layers.contains_key(&orphan_id));
        assert!(
            tree.validation_errors
                .contains(&LayerTreeValidationError::MissingParentLayer {
                    layer: orphan_id,
                    parent: orphan_parent,
                })
        );
    }
}
