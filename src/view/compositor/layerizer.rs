//! Observational layerization over the current authoritative promotion state.

#![allow(dead_code)]

use rustc_hash::{FxHashMap, FxHashSet};

use crate::view::node_arena::{NodeArena, NodeKey};
use crate::view::paint::{
    CoverageRecordingMode, PaintCoverageItem, PaintCoverageManifest, PaintCoverageStats,
    record_coverage_manifest,
};
use crate::view::promotion::{PromotedLayerUpdate, PromotionDecision, PromotionState};

use super::layer_tree::{
    CompositingReason, LayerBounds, LayerId, LayerItem, LayerTreeValidationError,
    OpaqueLegacyRevision, PaintOrderKey, PendingLayer, RetainedPaintChunk,
};
use super::{PaintGenerationTracker, PropertyTrees};

pub(crate) struct LayerizationResult {
    pub(crate) layers: Vec<PendingLayer>,
    pub(crate) validation_errors: Vec<LayerTreeValidationError>,
    pub(crate) coverage_stats: PaintCoverageStats,
}

struct NodePosition {
    order: PaintOrderKey,
    ancestors: Vec<NodeKey>,
    stable_id: u64,
    ownership_ancestor_start: usize,
}

pub(crate) fn layerize_shadow_tree(
    arena: &NodeArena,
    roots: &[NodeKey],
    promotion_state: &PromotionState,
    updates: &[PromotedLayerUpdate],
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    volatile_epoch: u64,
) -> LayerizationResult {
    let positions = collect_positions(arena, roots);
    let mut stable_keys: FxHashMap<u64, Vec<NodeKey>> = FxHashMap::default();
    for (&key, position) in &positions {
        stable_keys.entry(position.stable_id).or_default().push(key);
    }

    let mut errors = Vec::new();
    let mut requested_keys = FxHashMap::default();
    for &stable_id in &promotion_state.promoted_node_ids {
        match stable_keys.get(&stable_id).map(Vec::as_slice) {
            None | Some([]) => errors.push(LayerTreeValidationError::MissingStableId { stable_id }),
            Some([key]) => {
                requested_keys.insert(stable_id, *key);
            }
            Some(_) => errors.push(LayerTreeValidationError::DuplicateStableId { stable_id }),
        }
    }

    let decisions = promotion_state
        .decisions
        .iter()
        .filter(|decision| decision.should_promote)
        .map(|decision| (decision.node_id, decision))
        .collect::<FxHashMap<_, _>>();
    let updates = updates
        .iter()
        .map(|update| (update.node_id, update))
        .collect::<FxHashMap<_, _>>();

    let mut candidates = requested_keys.into_iter().collect::<Vec<_>>();
    candidates.sort_by_key(|(_, key)| positions[key].ancestors.len());
    let requested_key_set = candidates
        .iter()
        .map(|(_, key)| *key)
        .collect::<FxHashSet<_>>();
    let mut promoted_keys = FxHashMap::default();
    let mut promoted_key_set = FxHashSet::default();
    for (stable_id, key) in candidates {
        let decision_valid = decisions.contains_key(&stable_id);
        let update_valid = updates.contains_key(&stable_id);
        if !decision_valid {
            errors.push(LayerTreeValidationError::MissingPromotionDecision { stable_id });
        }
        if !update_valid {
            errors.push(LayerTreeValidationError::MissingPromotionUpdate { stable_id });
        }
        if !decision_valid || !update_valid {
            continue;
        }
        if let Some(invalid_ancestor) = positions[&key].ancestors
            [positions[&key].ownership_ancestor_start..]
            .iter()
            .rev()
            .copied()
            .find(|ancestor| {
                requested_key_set.contains(ancestor) && !promoted_key_set.contains(ancestor)
            })
        {
            errors.push(LayerTreeValidationError::InvalidPromotedAncestor {
                stable_id,
                ancestor_stable_id: positions[&invalid_ancestor].stable_id,
            });
            continue;
        }
        promoted_keys.insert(stable_id, key);
        promoted_key_set.insert(key);
    }

    let mut parent_layers = FxHashMap::default();
    let mut composition_paths = FxHashMap::default();
    for &key in &promoted_key_set {
        let position = &positions[&key];
        let nearest_index = position.ancestors[position.ownership_ancestor_start..]
            .iter()
            .rposition(|ancestor| promoted_key_set.contains(ancestor))
            .map(|relative| relative + position.ownership_ancestor_start);
        let parent = nearest_index
            .map(|index| LayerId::Promoted(position.ancestors[index]))
            .unwrap_or(LayerId::SceneRoot);
        let path_start = nearest_index
            .map(|index| index + 1)
            .unwrap_or(position.ownership_ancestor_start);
        parent_layers.insert(key, parent);
        composition_paths.insert(key, position.ancestors[path_start..].to_vec());
    }

    let mut seen_chunks = FxHashSet::default();
    let mut scene_items = Vec::new();
    let valid_promoted_ids = promoted_keys.keys().copied().collect::<FxHashSet<_>>();
    let scene_manifest = record_coverage_manifest(
        arena,
        roots,
        &valid_promoted_ids,
        None,
        true,
        true,
        CoverageRecordingMode::MetadataOnly,
        property_trees,
        paint_generations,
    );
    let mut coverage_stats = scene_manifest.stats();
    append_manifest_items(
        &mut scene_items,
        scene_manifest,
        OpaqueLegacyRevision(volatile_epoch),
        LayerId::SceneRoot,
        &parent_layers,
        &mut seen_chunks,
        &mut errors,
    );

    let scene_revision = paint_generations.root_topology_revision_value();
    let mut layers = vec![PendingLayer {
        id: LayerId::SceneRoot,
        owner: None,
        stable_id: None,
        parent: None,
        paint_order: PaintOrderKey::default(),
        composition_path: Vec::new(),
        reason: CompositingReason::SceneRoot,
        bounds: LayerBounds::scene(),
        properties: Default::default(),
        raster_revision: scene_revision,
        composite_revision: 0,
        topology_revision: scene_revision,
        items: scene_items,
    }];

    let mut promoted = promoted_keys
        .iter()
        .map(|(&stable_id, &key)| (stable_id, key))
        .collect::<Vec<_>>();
    promoted.sort_by(|(_, left), (_, right)| positions[left].order.cmp(&positions[right].order));
    for (stable_id, key) in promoted {
        let decision = decisions[&stable_id];
        let update = updates[&stable_id];
        let Some(node) = arena.get(key) else {
            errors.push(LayerTreeValidationError::MissingStableId { stable_id });
            continue;
        };
        let bounds = node.element.promotion_composite_bounds();
        let scoped_promoted_ids = promoted_keys
            .iter()
            .filter_map(|(&candidate_id, &candidate_key)| {
                (candidate_key == key
                    || positions[&candidate_key].ancestors
                        [positions[&candidate_key].ownership_ancestor_start..]
                        .iter()
                        .any(|ancestor| *ancestor == key))
                .then_some(candidate_id)
            })
            .collect::<FxHashSet<_>>();
        let manifest = record_coverage_manifest(
            arena,
            &[key],
            &scoped_promoted_ids,
            Some(key),
            true,
            false,
            CoverageRecordingMode::MetadataOnly,
            property_trees,
            paint_generations,
        );
        coverage_stats.merge(manifest.stats());
        let mut items = Vec::new();
        append_manifest_items(
            &mut items,
            manifest,
            OpaqueLegacyRevision(update.base_generation),
            LayerId::Promoted(key),
            &parent_layers,
            &mut seen_chunks,
            &mut errors,
        );
        let generations = paint_generations.local_generations_for(key);
        layers.push(PendingLayer {
            id: LayerId::Promoted(key),
            owner: Some(key),
            stable_id: Some(stable_id),
            parent: Some(parent_layers[&key]),
            paint_order: positions[&key].order.clone(),
            composition_path: composition_paths.remove(&key).unwrap_or_default(),
            reason: reason_for(decision),
            bounds: LayerBounds {
                x: bounds.x,
                y: bounds.y,
                width: bounds.width,
                height: bounds.height,
                corner_radii: bounds.corner_radii,
            },
            properties: property_trees.paint_state_for(key).unwrap_or_default(),
            raster_revision: update.base_generation,
            composite_revision: update.composition_generation,
            topology_revision: generations
                .map(|generation| generation.topology_revision)
                .unwrap_or_default(),
            items,
        });
    }

    LayerizationResult {
        layers,
        validation_errors: errors,
        coverage_stats,
    }
}

fn reason_for(decision: &PromotionDecision) -> CompositingReason {
    decision
        .hard_reason
        .map(CompositingReason::Hard)
        .unwrap_or(CompositingReason::Heuristic {
            score: decision.score,
            threshold: decision.threshold,
        })
}

fn collect_positions(arena: &NodeArena, roots: &[NodeKey]) -> FxHashMap<NodeKey, NodePosition> {
    fn walk(
        arena: &NodeArena,
        key: NodeKey,
        root_index: usize,
        path: &mut Vec<usize>,
        ancestors: &mut Vec<NodeKey>,
        ownership_ancestor_start: usize,
        seen: &mut FxHashSet<NodeKey>,
        out: &mut FxHashMap<NodeKey, NodePosition>,
    ) {
        if !seen.insert(key) {
            return;
        }
        let Some(node) = arena.get(key) else {
            return;
        };
        let is_deferred = node.element.is_deferred_to_root_viewport_render();
        out.insert(
            key,
            NodePosition {
                order: PaintOrderKey {
                    root_index,
                    child_path: path.clone(),
                },
                ancestors: ancestors.clone(),
                stable_id: node.element.stable_id(),
                ownership_ancestor_start: if is_deferred {
                    ancestors.len()
                } else {
                    ownership_ancestor_start
                },
            },
        );
        let children = node.children().to_vec();
        drop(node);
        ancestors.push(key);
        let child_ownership_start = if is_deferred {
            ancestors.len().saturating_sub(1)
        } else {
            ownership_ancestor_start
        };
        for (index, child) in children.into_iter().enumerate() {
            path.push(index);
            walk(
                arena,
                child,
                root_index,
                path,
                ancestors,
                child_ownership_start,
                seen,
                out,
            );
            path.pop();
        }
        ancestors.pop();
    }

    let mut out = FxHashMap::default();
    let mut seen = FxHashSet::default();
    for (root_index, &root) in roots.iter().enumerate() {
        walk(
            arena,
            root,
            root_index,
            &mut Vec::new(),
            &mut Vec::new(),
            0,
            &mut seen,
            &mut out,
        );
    }
    out
}

fn append_manifest_items(
    items: &mut Vec<LayerItem>,
    manifest: PaintCoverageManifest,
    revision: OpaqueLegacyRevision,
    manifest_parent: LayerId,
    parent_layers: &FxHashMap<NodeKey, LayerId>,
    seen_chunks: &mut FxHashSet<crate::view::paint::PaintChunkId>,
    errors: &mut Vec<LayerTreeValidationError>,
) {
    let mut invalid_cutouts = FxHashSet::default();
    for item in &manifest.items {
        if let PaintCoverageItem::PromotedBoundary { root, .. } = item {
            let actual_parent = parent_layers.get(root).copied();
            if actual_parent != Some(manifest_parent) {
                invalid_cutouts.insert(*root);
                errors.push(LayerTreeValidationError::InvalidManifestCutoutParent {
                    cutout: LayerId::Promoted(*root),
                    manifest_parent,
                    actual_parent,
                });
            }
        }
    }
    errors.extend(
        manifest
            .validation_errors
            .iter()
            .cloned()
            .map(LayerTreeValidationError::PaintCoverage),
    );
    for item in manifest.items {
        match item {
            PaintCoverageItem::ArtifactChunk {
                order,
                chunk,
                ops: _,
                ..
            } => {
                let _ = order;
                if !seen_chunks.insert(chunk.id) {
                    errors.push(LayerTreeValidationError::DuplicatePaintChunk { id: chunk.id });
                    continue;
                }
                items.push(LayerItem::PaintChunk {
                    artifact_root: chunk.id.owner,
                    chunk: RetainedPaintChunk {
                        id: chunk.id,
                        owner: chunk.owner,
                        bounds: chunk.bounds,
                        properties: chunk.properties,
                        content_revision: chunk.content_revision,
                        payload_identity: chunk.payload_identity,
                    },
                });
            }
            PaintCoverageItem::TransparentNode { order, .. } => {
                let _ = order;
            }
            PaintCoverageItem::CulledSubtree { order, .. } => {
                let _ = order;
            }
            PaintCoverageItem::LegacyBoundary {
                root,
                stable_id,
                reason,
                span_index,
                before_promoted,
                after_promoted,
                order,
            } => {
                let _ = order;
                items.push(LayerItem::LegacySpan {
                    boundary_root: root,
                    stable_id,
                    reason,
                    span_index,
                    before_cutout: before_promoted
                        .filter(|key| !invalid_cutouts.contains(key))
                        .map(LayerId::Promoted),
                    after_cutout: after_promoted
                        .filter(|key| !invalid_cutouts.contains(key))
                        .map(LayerId::Promoted),
                    revision,
                });
            }
            PaintCoverageItem::PromotedBoundary {
                order,
                root,
                stable_id,
            } => {
                let _ = (order, stable_id);
                if !invalid_cutouts.contains(&root) {
                    items.push(LayerItem::PromotedChild(LayerId::Promoted(root)));
                }
            }
            PaintCoverageItem::PlannedBoundary { .. }
            | PaintCoverageItem::NestedScrollContentReceiver { .. } => {
                errors.push(LayerTreeValidationError::PaintCoverage(
                    crate::view::paint::PaintCoverageValidationError::RecordingPassMismatch,
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::{ClipMode, Color, Length, ParsedValue, Position, PropertyId, Style};
    use crate::view::base_component::Element;
    use crate::view::compositor::{PaintGenerationTracker, PropertyTrees};
    use crate::view::node_arena::{Node, NodeArena};
    use crate::view::paint::{PaintMetadataOutcome, record_root_metadata};
    use crate::view::promotion::{
        PromotedLayerUpdateKind, PromotionHardReason, PromotionScoreBreakdown, PromotionState,
    };
    use slotmap::Key;

    fn element(id: u64) -> Element {
        let mut element = Element::new_with_id(id, 0.0, 0.0, 20.0, 20.0);
        let mut style = Style::new();
        style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::rgb(id as u8, 0, 0)),
        );
        element.apply_style(style);
        element
    }

    fn insert(arena: &mut NodeArena, id: u64) -> NodeKey {
        arena.insert(Node::new(Box::new(element(id))))
    }

    fn child(arena: &mut NodeArena, parent: NodeKey, id: u64) -> NodeKey {
        let key = insert(arena, id);
        arena.set_parent(key, Some(parent));
        arena.push_child(parent, key);
        key
    }

    fn deferred_child(arena: &mut NodeArena, parent: NodeKey, id: u64) -> NodeKey {
        let mut element = element(id);
        let mut style = Style::new();
        style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(0.0))
                    .clip(ClipMode::Viewport),
            ),
        );
        element.apply_style(style);
        let key = arena.insert(Node::new(Box::new(element)));
        arena.set_parent(key, Some(parent));
        arena.push_child(parent, key);
        key
    }

    fn identity(arena: &NodeArena, roots: &[NodeKey]) -> (PropertyTrees, PaintGenerationTracker) {
        let mut properties = PropertyTrees::default();
        properties.sync(arena, roots);
        let mut generations = PaintGenerationTracker::default();
        generations.sync(arena, roots, &properties);
        (properties, generations)
    }

    fn promotion(ids: &[(u64, Option<u64>)]) -> (PromotionState, Vec<PromotedLayerUpdate>) {
        let mut state = PromotionState::default();
        let mut updates = Vec::new();
        for (index, &(node_id, parent_id)) in ids.iter().enumerate() {
            state.promoted_node_ids.insert(node_id);
            state.decisions.push(PromotionDecision {
                node_id,
                parent_id,
                score: 50,
                threshold: 35,
                should_promote: true,
                hard_reason: None,
                budget_rejection: None,
                breakdown: PromotionScoreBreakdown::default(),
                subtree_node_count: 1,
                estimated_pass_count: 1,
                visible_area_ratio: 1.0,
                viewport_coverage: 0.1,
                distance_to_viewport: 0.0,
                estimated_memory_bytes: 64,
            });
            updates.push(PromotedLayerUpdate {
                node_id,
                parent_id,
                kind: PromotedLayerUpdateKind::Reraster,
                base_signature: index as u64 + 1,
                previous_base_signature: None,
                composition_kind: PromotedLayerUpdateKind::Reraster,
                composition_signature: index as u64 + 2,
                previous_composition_signature: None,
                base_generation: index as u64 + 10,
                previous_base_generation: None,
                composition_generation: index as u64 + 20,
                previous_composition_generation: None,
            });
        }
        (state, updates)
    }

    #[test]
    fn safe_leaf_roots_keep_artifact_content_and_root_order() {
        let mut arena = NodeArena::new();
        let a = insert(&mut arena, 1);
        let b = insert(&mut arena, 2);
        let roots = [a, b];
        let (properties, generations) = identity(&arena, &roots);
        let result = layerize_shadow_tree(
            &arena,
            &roots,
            &PromotionState::default(),
            &[],
            &properties,
            &generations,
            1,
        );
        let scene = result
            .layers
            .iter()
            .find(|layer| layer.id == LayerId::SceneRoot)
            .unwrap();
        assert_eq!(scene.items.len(), 2);
        assert!(
            scene
                .items
                .iter()
                .all(|content| matches!(content, LayerItem::PaintChunk { .. }))
        );
        assert!(result.validation_errors.is_empty());
    }

    #[test]
    fn metadata_layerization_matches_full_chunk_without_recording_ops() {
        let mut arena = NodeArena::new();
        let root = insert(&mut arena, 1);
        let roots = [root];
        let (properties, generations) = identity(&arena, &roots);
        crate::view::paint::take_full_artifact_record_count();
        let result = layerize_shadow_tree(
            &arena,
            &roots,
            &PromotionState::default(),
            &[],
            &properties,
            &generations,
            1,
        );
        assert_eq!(crate::view::paint::take_full_artifact_record_count(), 0);
        let LayerItem::PaintChunk {
            chunk: metadata, ..
        } = &result.layers[0].items[0]
        else {
            panic!("safe leaf should expose paint metadata");
        };
        let PaintMetadataOutcome::Artifact(metadata_outcome) =
            record_root_metadata(&arena, root, false, &properties, &generations)
        else {
            panic!("metadata recorder should keep the safe leaf recordable");
        };
        let crate::view::paint::PaintRecordOutcome::Artifact(full) =
            crate::view::paint::record_root(&arena, root, false, &properties, &generations)
        else {
            panic!("full recorder should keep the safe leaf recordable");
        };
        assert_eq!(crate::view::paint::take_full_artifact_record_count(), 1);
        assert_eq!(metadata.id, metadata_outcome[0].id);
        assert_eq!(metadata.id, full.chunks[0].id);
        assert_eq!(metadata.content_revision, full.chunks[0].content_revision);
        assert_eq!(metadata.properties, full.chunks[0].properties);
        assert!(super::super::layer_tree::rect_eq(
            metadata.bounds,
            full.chunks[0].bounds
        ));
    }

    #[test]
    fn standard_layerizer_rejects_dedicated_nested_scroll_receiver() {
        let mut arena = NodeArena::new();
        let outer = insert(&mut arena, 0x1251_00);
        let inner = insert(&mut arena, 0x1251_01);
        let content = insert(&mut arena, 0x1251_02);
        let manifest = crate::view::paint::nested_scroll_receiver_manifest_for_layerizer_test(
            outer, inner, content, 0x1251_02,
        );
        let mut items = Vec::new();
        let mut seen_chunks = FxHashSet::default();
        let mut errors = Vec::new();
        append_manifest_items(
            &mut items,
            manifest,
            OpaqueLegacyRevision(1),
            LayerId::SceneRoot,
            &FxHashMap::default(),
            &mut seen_chunks,
            &mut errors,
        );
        assert!(items.is_empty());
        assert_eq!(
            errors,
            vec![LayerTreeValidationError::PaintCoverage(
                crate::view::paint::PaintCoverageValidationError::RecordingPassMismatch,
            )]
        );
    }

    #[test]
    fn promoted_root_uses_authoritative_reason_and_opaque_content() {
        let mut arena = NodeArena::new();
        let root = insert(&mut arena, 1);
        let roots = [root];
        let (properties, generations) = identity(&arena, &roots);
        let (mut state, updates) = promotion(&[(1, None)]);
        state.decisions[0].hard_reason = Some(PromotionHardReason::ActiveOpacityAnimation);
        let result = layerize_shadow_tree(
            &arena,
            &roots,
            &state,
            &updates,
            &properties,
            &generations,
            1,
        );
        let scene = result
            .layers
            .iter()
            .find(|layer| layer.id == LayerId::SceneRoot)
            .unwrap();
        let promoted = result
            .layers
            .iter()
            .find(|layer| layer.id == LayerId::Promoted(root))
            .unwrap();
        assert_eq!(
            scene.items,
            vec![LayerItem::PromotedChild(LayerId::Promoted(root))]
        );
        assert_eq!(promoted.parent, Some(LayerId::SceneRoot));
        assert_eq!(
            promoted.reason,
            CompositingReason::Hard(PromotionHardReason::ActiveOpacityAnimation)
        );
        assert!(matches!(
            promoted.items.as_slice(),
            [LayerItem::LegacySpan {
                reason: crate::view::paint::LegacyPaintReason::Promoted,
                ..
            }]
        ));
        assert!(result.validation_errors.is_empty());
    }

    #[test]
    fn nested_promotions_use_nearest_layer_parent_and_preserve_host_path() {
        let mut arena = NodeArena::new();
        let root = insert(&mut arena, 1);
        let promoted_parent = child(&mut arena, root, 2);
        let host = child(&mut arena, promoted_parent, 3);
        let promoted_child = child(&mut arena, host, 4);
        let roots = [root];
        let (properties, generations) = identity(&arena, &roots);
        let (state, updates) = promotion(&[(2, Some(1)), (4, Some(3))]);
        let result = layerize_shadow_tree(
            &arena,
            &roots,
            &state,
            &updates,
            &properties,
            &generations,
            1,
        );
        let parent = result
            .layers
            .iter()
            .find(|layer| layer.id == LayerId::Promoted(promoted_parent))
            .unwrap();
        let nested = result
            .layers
            .iter()
            .find(|layer| layer.id == LayerId::Promoted(promoted_child))
            .unwrap();
        assert_eq!(nested.parent, Some(parent.id));
        assert_eq!(nested.composition_path, vec![host]);
        assert!(matches!(
            parent.items.as_slice(),
            [
                LayerItem::LegacySpan {
                    before_cutout: None,
                    after_cutout: Some(id),
                    ..
                },
                LayerItem::PromotedChild(child),
                LayerItem::LegacySpan {
                    before_cutout: Some(id2),
                    after_cutout: None,
                    ..
                }
            ] if *id == nested.id && *child == nested.id && *id2 == nested.id
        ));
        assert!(result.validation_errors.is_empty());
    }

    #[test]
    fn unpromoted_legacy_root_lists_only_direct_promoted_cutouts() {
        let mut arena = NodeArena::new();
        let root = insert(&mut arena, 1);
        let promoted_parent = child(&mut arena, root, 2);
        let promoted_child = child(&mut arena, promoted_parent, 3);
        let roots = [root];
        let (properties, generations) = identity(&arena, &roots);
        let (state, updates) = promotion(&[(2, Some(1)), (3, Some(2))]);
        let result = layerize_shadow_tree(
            &arena,
            &roots,
            &state,
            &updates,
            &properties,
            &generations,
            9,
        );
        let scene = &result.layers[0];
        assert!(matches!(
            scene.items.as_slice(),
            [
                LayerItem::LegacySpan {
                    revision: OpaqueLegacyRevision(9),
                    before_cutout: None,
                    after_cutout: Some(parent),
                    ..
                },
                LayerItem::PromotedChild(parent2),
                LayerItem::LegacySpan {
                    revision: OpaqueLegacyRevision(9),
                    before_cutout: Some(parent3),
                    after_cutout: None,
                    ..
                }
            ] if *parent == LayerId::Promoted(promoted_parent)
                && *parent2 == LayerId::Promoted(promoted_parent)
                && *parent3 == LayerId::Promoted(promoted_parent)
        ));
        assert!(
            !scene
                .items
                .contains(&LayerItem::PromotedChild(LayerId::Promoted(promoted_child)))
        );
    }

    #[test]
    fn unresolved_or_duplicate_stable_ids_are_validation_errors_not_guesses() {
        let mut arena = NodeArena::new();
        let a = insert(&mut arena, 7);
        let b = insert(&mut arena, 7);
        let roots = [a, b];
        let (properties, generations) = identity(&arena, &roots);
        let (mut state, updates) = promotion(&[(7, None)]);
        state.promoted_node_ids.insert(99);
        let result = layerize_shadow_tree(
            &arena,
            &roots,
            &state,
            &updates,
            &properties,
            &generations,
            1,
        );
        assert!(
            result
                .validation_errors
                .contains(&LayerTreeValidationError::DuplicateStableId { stable_id: 7 })
        );
        assert!(
            result
                .validation_errors
                .contains(&LayerTreeValidationError::MissingStableId { stable_id: 99 })
        );
        assert_eq!(result.layers.len(), 1);
    }

    #[test]
    fn multiple_direct_cutouts_form_ordered_n_plus_one_legacy_spans() {
        let mut arena = NodeArena::new();
        let root = insert(&mut arena, 1);
        let _a = child(&mut arena, root, 2);
        let promoted_p = child(&mut arena, root, 3);
        let _b = child(&mut arena, root, 4);
        let promoted_q = child(&mut arena, root, 5);
        let _c = child(&mut arena, root, 6);
        let nested = child(&mut arena, promoted_p, 7);
        let roots = [root];
        let (properties, generations) = identity(&arena, &roots);
        let (state, updates) = promotion(&[(3, Some(1)), (5, Some(1)), (7, Some(3))]);
        let result = layerize_shadow_tree(
            &arena,
            &roots,
            &state,
            &updates,
            &properties,
            &generations,
            1,
        );
        let scene = result
            .layers
            .iter()
            .find(|layer| layer.id == LayerId::SceneRoot)
            .unwrap();
        let p = LayerId::Promoted(promoted_p);
        let q = LayerId::Promoted(promoted_q);
        assert!(matches!(
            scene.items.as_slice(),
            [
                LayerItem::LegacySpan { span_index: 0, before_cutout: None, after_cutout: Some(p0), .. },
                LayerItem::PromotedChild(p1),
                LayerItem::LegacySpan { span_index: 1, before_cutout: Some(p2), after_cutout: Some(q0), .. },
                LayerItem::PromotedChild(q1),
                LayerItem::LegacySpan { span_index: 2, before_cutout: Some(q2), after_cutout: None, .. },
            ] if *p0 == p && *p1 == p && *p2 == p && *q0 == q && *q1 == q && *q2 == q
        ));
        assert!(
            !scene
                .items
                .contains(&LayerItem::PromotedChild(LayerId::Promoted(nested)))
        );
        assert!(result.validation_errors.is_empty());
    }

    #[test]
    fn manifest_validation_maps_duplicate_and_missing_root_keys_into_layer_errors() {
        let mut arena = NodeArena::new();
        let root = insert(&mut arena, 88);
        let roots = [root, root, NodeKey::null()];
        let (properties, generations) = identity(&arena, &roots);
        let result = layerize_shadow_tree(
            &arena,
            &roots,
            &PromotionState::default(),
            &[],
            &properties,
            &generations,
            1,
        );
        assert!(
            result
                .validation_errors
                .contains(&LayerTreeValidationError::PaintCoverage(
                    crate::view::paint::PaintCoverageValidationError::DuplicateNodeKey(root)
                ))
        );
        assert!(
            result
                .validation_errors
                .contains(&LayerTreeValidationError::PaintCoverage(
                    crate::view::paint::PaintCoverageValidationError::MissingNode(NodeKey::null())
                ))
        );
    }

    #[test]
    fn deferred_boundary_resets_promoted_parent_and_emits_late_after_normal_content() {
        let mut arena = NodeArena::new();
        let a = insert(&mut arena, 100);
        let deferred = deferred_child(&mut arena, a, 101);
        let p = child(&mut arena, deferred, 102);
        let _q = child(&mut arena, a, 103);
        let roots = [a];
        let (properties, generations) = identity(&arena, &roots);
        let (state, updates) = promotion(&[(100, None), (102, Some(101))]);
        let result = layerize_shadow_tree(
            &arena,
            &roots,
            &state,
            &updates,
            &properties,
            &generations,
            1,
        );
        let p_layer = result
            .layers
            .iter()
            .find(|layer| layer.id == LayerId::Promoted(p))
            .expect("deferred promoted P layer");
        assert_eq!(p_layer.parent, Some(LayerId::SceneRoot));
        assert_eq!(p_layer.composition_path, vec![deferred]);
        let scene = &result.layers[0];
        let a_index = scene
            .items
            .iter()
            .position(|item| *item == LayerItem::PromotedChild(LayerId::Promoted(a)))
            .expect("A boundary");
        let p_index = scene
            .items
            .iter()
            .position(|item| *item == LayerItem::PromotedChild(LayerId::Promoted(p)))
            .expect("late P boundary");
        assert!(a_index < p_index, "normal A/Q content must precede late P");
        assert_eq!(
            scene
                .items
                .iter()
                .filter(|item| matches!(item, LayerItem::LegacySpan { boundary_root, span_index: 0, .. } if *boundary_root == deferred))
                .count(),
            1
        );
        let a_layer = result
            .layers
            .iter()
            .find(|layer| layer.id == LayerId::Promoted(a))
            .unwrap();
        assert!(
            !a_layer
                .items
                .contains(&LayerItem::PromotedChild(LayerId::Promoted(p)))
        );
        assert!(result.validation_errors.is_empty());
    }

    #[test]
    fn deferred_boundary_preserves_promoted_nesting_inside_new_scene_scope() {
        let mut arena = NodeArena::new();
        let a = insert(&mut arena, 110);
        let deferred = deferred_child(&mut arena, a, 111);
        let r = child(&mut arena, deferred, 112);
        let p = child(&mut arena, r, 113);
        let roots = [a];
        let (properties, generations) = identity(&arena, &roots);
        let (state, updates) = promotion(&[(110, None), (112, Some(111)), (113, Some(112))]);
        let result = layerize_shadow_tree(
            &arena,
            &roots,
            &state,
            &updates,
            &properties,
            &generations,
            1,
        );
        let r_layer = result
            .layers
            .iter()
            .find(|layer| layer.id == LayerId::Promoted(r))
            .unwrap();
        let p_layer = result
            .layers
            .iter()
            .find(|layer| layer.id == LayerId::Promoted(p))
            .unwrap();
        assert_eq!(r_layer.parent, Some(LayerId::SceneRoot));
        assert_eq!(r_layer.composition_path, vec![deferred]);
        assert_eq!(p_layer.parent, Some(LayerId::Promoted(r)));
        assert!(p_layer.composition_path.is_empty());
        assert!(result.validation_errors.is_empty());
    }

    #[test]
    fn promoted_deferred_root_owns_nested_promotions_in_the_new_scope() {
        let mut arena = NodeArena::new();
        let a = insert(&mut arena, 120);
        let deferred = deferred_child(&mut arena, a, 121);
        let r = child(&mut arena, deferred, 122);
        let p = child(&mut arena, r, 123);
        let roots = [a];
        let (properties, generations) = identity(&arena, &roots);
        let (state, updates) = promotion(&[(121, Some(120)), (122, Some(121)), (123, Some(122))]);
        let result = layerize_shadow_tree(
            &arena,
            &roots,
            &state,
            &updates,
            &properties,
            &generations,
            1,
        );
        let d_layer = result
            .layers
            .iter()
            .find(|layer| layer.id == LayerId::Promoted(deferred))
            .expect("promoted deferred D");
        let r_layer = result
            .layers
            .iter()
            .find(|layer| layer.id == LayerId::Promoted(r))
            .expect("promoted R");
        let p_layer = result
            .layers
            .iter()
            .find(|layer| layer.id == LayerId::Promoted(p))
            .expect("promoted P");
        assert_eq!(d_layer.parent, Some(LayerId::SceneRoot));
        assert!(d_layer.composition_path.is_empty());
        assert_eq!(r_layer.parent, Some(LayerId::Promoted(deferred)));
        assert!(r_layer.composition_path.is_empty());
        assert_eq!(p_layer.parent, Some(LayerId::Promoted(r)));
        assert!(p_layer.composition_path.is_empty());
        assert_eq!(
            d_layer
                .items
                .iter()
                .filter(|item| **item == LayerItem::PromotedChild(LayerId::Promoted(r)))
                .count(),
            1
        );
        assert_eq!(
            r_layer
                .items
                .iter()
                .filter(|item| **item == LayerItem::PromotedChild(LayerId::Promoted(p)))
                .count(),
            1
        );
        assert!(result.validation_errors.is_empty());
    }

    #[test]
    fn promoted_reuse_keeps_legacy_span_revision_stable() {
        let mut arena = NodeArena::new();
        let root = insert(&mut arena, 1);
        let roots = [root];
        let (properties, generations) = identity(&arena, &roots);
        let (state, updates) = promotion(&[(1, None)]);
        let mut tree = crate::view::compositor::layer_tree::LayerTree::default();
        let first = layerize_shadow_tree(
            &arena,
            &roots,
            &state,
            &updates,
            &properties,
            &generations,
            1,
        );
        tree.reconcile(first.layers, first.validation_errors);
        let mut reused_updates = updates.clone();
        reused_updates[0].kind = PromotedLayerUpdateKind::Reuse;
        reused_updates[0].composition_kind = PromotedLayerUpdateKind::Reuse;
        let second = layerize_shadow_tree(
            &arena,
            &roots,
            &state,
            &reused_updates,
            &properties,
            &generations,
            2,
        );
        tree.reconcile(second.layers, second.validation_errors);
        assert!(
            !tree
                .last_diff
                .raster_changed
                .contains(&LayerId::Promoted(root))
        );
    }

    #[test]
    fn invalid_promoted_ancestor_rejects_its_promoted_subtree() {
        let mut arena = NodeArena::new();
        let root = insert(&mut arena, 1);
        let promoted_parent = child(&mut arena, root, 2);
        let promoted_child = child(&mut arena, promoted_parent, 3);
        let roots = [root];
        let (properties, generations) = identity(&arena, &roots);
        let (mut state, mut updates) = promotion(&[(2, Some(1)), (3, Some(2))]);
        state.decisions.retain(|decision| decision.node_id != 2);
        updates.retain(|update| update.node_id != 2);
        let result = layerize_shadow_tree(
            &arena,
            &roots,
            &state,
            &updates,
            &properties,
            &generations,
            1,
        );
        assert!(
            !result
                .layers
                .iter()
                .any(|layer| layer.id == LayerId::Promoted(promoted_parent))
        );
        assert!(
            !result
                .layers
                .iter()
                .any(|layer| layer.id == LayerId::Promoted(promoted_child))
        );
        assert!(result.validation_errors.contains(
            &LayerTreeValidationError::InvalidPromotedAncestor {
                stable_id: 3,
                ancestor_stable_id: 2,
            }
        ));
    }
}
