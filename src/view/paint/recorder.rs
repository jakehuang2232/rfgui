#![allow(dead_code)] // Compatibility recorder remains available for the later authority switch.

use crate::view::base_component::Element;
use crate::view::compositor::{PaintGenerationTracker, PropertyTrees};
use crate::view::node_arena::{NodeArena, NodeKey};

#[cfg(test)]
use super::PaintChunkMetadata;
use super::{PaintArtifact, PaintOwnerSnapshot};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum LegacyPaintReason {
    UnknownHost,
    HasChildren,
    Promoted,
    Transform,
    BoxShadow,
    InlineIfc,
    ScrollContainer,
    SelfClip,
    ChildClip,
    Deferred,
    LayoutTransition,
    StatefulPaint,
    MissingPaintIdentity,
    MissingPreparedInlineDecoration,
    MissingPreparedInlineRoot,
    MissingPreparedText,
    MissingPreparedImage,
    MissingPreparedSvg,
    TextAreaSelection,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LegacySubtree {
    pub(crate) root: NodeKey,
    pub(crate) stable_id: u64,
    pub(crate) reason: LegacyPaintReason,
}

#[derive(Clone, Debug)]
pub(crate) enum PaintRecordOutcome {
    Artifact(PaintArtifact),
    LegacySubtree(LegacySubtree),
}

#[cfg(test)]
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub(crate) enum PaintMetadataOutcome {
    Artifact(Vec<PaintChunkMetadata>),
    LegacySubtree(LegacySubtree),
}

#[cfg(test)]
thread_local! {
    static FULL_ARTIFACT_RECORDS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

pub(crate) fn record_root(
    arena: &NodeArena,
    root: NodeKey,
    promoted: bool,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
) -> PaintRecordOutcome {
    let Some(node) = arena.get(root) else {
        return legacy(root, 0, LegacyPaintReason::MissingPaintIdentity);
    };
    let stable_id = node.element.stable_id();
    if promoted {
        return legacy(root, stable_id, LegacyPaintReason::Promoted);
    }
    let Some(properties) = property_trees.paint_state_for(root) else {
        return legacy(root, stable_id, LegacyPaintReason::MissingPaintIdentity);
    };
    let Some(generations) = paint_generations.local_generations_for(root) else {
        return legacy(root, stable_id, LegacyPaintReason::MissingPaintIdentity);
    };
    let Some(element) = node.element.as_any().downcast_ref::<Element>() else {
        return legacy(root, stable_id, LegacyPaintReason::UnknownHost);
    };
    let content_revision = super::PaintContentRevision {
        self_paint_revision: generations.self_paint_revision,
        composite_revision: generations.composite_revision,
        topology_revision: generations.topology_revision,
    };
    #[cfg(test)]
    FULL_ARTIFACT_RECORDS.with(|count| count.set(count.get().saturating_add(1)));
    match element.record_safe_leaf_paint_artifact(root, properties, content_revision) {
        Ok(mut artifact) => {
            let Some(clip_nodes) = property_trees.clip_snapshot_for(properties.clip) else {
                return legacy(root, stable_id, LegacyPaintReason::MissingPaintIdentity);
            };
            let Some(effect_nodes) = property_trees.effect_snapshot_for(properties.effect) else {
                return legacy(root, stable_id, LegacyPaintReason::MissingPaintIdentity);
            };
            artifact.clip_nodes = clip_nodes;
            artifact.effect_nodes = effect_nodes;
            artifact.owner_nodes = vec![PaintOwnerSnapshot {
                owner: root,
                parent: None,
            }];
            PaintRecordOutcome::Artifact(artifact)
        }
        Err(reason) => legacy(root, stable_id, reason),
    }
}

#[cfg(test)]
pub(crate) fn record_root_metadata(
    arena: &NodeArena,
    root: NodeKey,
    promoted: bool,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
) -> PaintMetadataOutcome {
    let Some(node) = arena.get(root) else {
        return metadata_legacy(root, 0, LegacyPaintReason::MissingPaintIdentity);
    };
    let stable_id = node.element.stable_id();
    if promoted {
        return metadata_legacy(root, stable_id, LegacyPaintReason::Promoted);
    }
    let Some(properties) = property_trees.paint_state_for(root) else {
        return metadata_legacy(root, stable_id, LegacyPaintReason::MissingPaintIdentity);
    };
    let Some(generations) = paint_generations.local_generations_for(root) else {
        return metadata_legacy(root, stable_id, LegacyPaintReason::MissingPaintIdentity);
    };
    let Some(element) = node.element.as_any().downcast_ref::<Element>() else {
        return metadata_legacy(root, stable_id, LegacyPaintReason::UnknownHost);
    };
    let content_revision = super::PaintContentRevision {
        self_paint_revision: generations.self_paint_revision,
        composite_revision: generations.composite_revision,
        topology_revision: generations.topology_revision,
    };
    match element.record_safe_leaf_paint_metadata(root, properties, content_revision) {
        Ok(chunk) => PaintMetadataOutcome::Artifact(vec![chunk]),
        Err(reason) => metadata_legacy(root, stable_id, reason),
    }
}

fn legacy(root: NodeKey, stable_id: u64, reason: LegacyPaintReason) -> PaintRecordOutcome {
    PaintRecordOutcome::LegacySubtree(LegacySubtree {
        root,
        stable_id,
        reason,
    })
}

#[cfg(test)]
fn metadata_legacy(
    root: NodeKey,
    stable_id: u64,
    reason: LegacyPaintReason,
) -> PaintMetadataOutcome {
    PaintMetadataOutcome::LegacySubtree(LegacySubtree {
        root,
        stable_id,
        reason,
    })
}

#[cfg(test)]
pub(crate) fn take_full_artifact_record_count() -> usize {
    FULL_ARTIFACT_RECORDS.with(|count| count.replace(0))
}

#[cfg(test)]
pub(crate) fn note_full_artifact_record() {
    FULL_ARTIFACT_RECORDS.with(|count| count.set(count.get().saturating_add(1)));
}
