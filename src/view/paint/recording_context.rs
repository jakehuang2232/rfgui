//! Recorder-side capability context.
//!
//! Split out of `artifact.rs` so the artifact data model — what the V2
//! layerizer consumes — stays free of the pre-V2 admission types. This module
//! is held to the same rule: a recording capability is either a generic
//! property witness or a recorder-derived behavior flag that coverage
//! recomputes for every node it walks.

use crate::view::compositor::property_tree::{ClipNodeId, ScrollNodeId};
use crate::view::node_arena::NodeKey;
use std::sync::Arc;

use super::artifact::*;

/// Large witnesses that only special recorders or a few owners carry are
/// shared behind `Arc`: the context is cloned for every walked node and every
/// child edge, and most of those witnesses are absent on the hot path.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct PaintRecordingContext {
    pub(crate) inside_text_area: bool,
    /// Path-scoped authority for a single projection-owned Text selection.
    /// The coverage walker derives this independently for every child edge;
    /// it must never be treated as ambient frame state.
    pub(crate) text_area_selection: Option<PaintTextSelectionWitness>,
    /// Path-scoped authority for the single projection-owned `Text` that
    /// contains the active IME preedit. Geometry decorations remain owned by
    /// the TextArea; this witness only proves which child glyph payload may
    /// contain the transient insertion.
    pub(crate) text_area_preedit: Option<Arc<PaintTextPreeditWitness>>,
    /// Set by the coverage walker from the canonical frame root/path, never
    /// inferred from stable ids or arena parent scans.
    pub(crate) is_frame_root: bool,
    /// Canonical owner for this one coverage invocation. Child contexts inherit
    /// by value, so the walker overwrites this before every capability call.
    pub(crate) recording_owner: Option<NodeKey>,
    pub(crate) recording_owner_stable_id: Option<u64>,
    /// The canonical property snapshot proves that `recording_owner` owns this
    /// exact logical `SelfClip / Replace` boundary. It is cleared before every
    /// node and cannot act as ambient custom-host authority.
    pub(crate) authoritative_self_clip: Option<ClipNodeId>,
    /// Recorder-minted proof for the one exact deferred viewport-clipped
    /// native root. Coverage clears and rebinds this frozen replace-scissor
    /// witness after every component context hook.
    pub(crate) deferred_viewport_self_clip: Option<Arc<PaintDeferredViewportSelfClipWitness>>,
    /// Owner-scoped scroll root accepted by the generic Surface DAG recorder.
    /// Coverage binds it only when this owner's descendants state names the
    /// same owner-keyed scroll node, so foreign scroll authority cannot leak
    /// through copied component context.
    pub(crate) surface_dag_scroll: Option<ScrollNodeId>,
    pub(crate) surface_dag_scroll_snapshot:
        Option<Arc<crate::view::compositor::property_tree::ScrollNodeSnapshot>>,
}

impl PaintRecordingContext {
    pub(crate) fn authorizes_deferred_viewport_self_clip_for(&self, stable_id: u64) -> bool {
        matches!(
            (
                self.recording_owner,
                self.recording_owner_stable_id,
                self.deferred_viewport_self_clip.as_deref(),
            ),
            (Some(owner), Some(recording_stable_id), Some(witness))
                if recording_stable_id == stable_id
                    && witness.is_canonical_for(
                        owner,
                        stable_id,
                        self.authoritative_self_clip,
                    )
        )
    }

    /// Generic owner-scoped scroll authority.
    pub(crate) fn authorizes_generic_scroll_host_root(&self, stable_id: u64) -> bool {
        matches!(
            (
                self.recording_owner,
                self.recording_owner_stable_id,
                self.surface_dag_scroll,
            ),
            (Some(owner), Some(recording_stable_id), Some(scroll))
                if recording_stable_id == stable_id && scroll == ScrollNodeId(owner)
        )
    }

    /// Complete frozen host state for command recording, bound from
    /// PropertyTrees after component hooks for the canonical traversal owner.
    pub(crate) fn recorded_scroll_host_snapshot_for_root(
        &self,
        stable_id: u64,
    ) -> Option<crate::view::compositor::property_tree::ScrollNodeSnapshot> {
        let snapshot = *self.surface_dag_scroll_snapshot.as_deref()?;
        (self.authorizes_generic_scroll_host_root(stable_id)
            && Some(snapshot.id) == self.surface_dag_scroll
            && Some(snapshot.owner) == self.recording_owner)
            .then_some(snapshot)
    }

    pub(crate) fn without_text_area_child_authority(&self) -> Self {
        Self {
            text_area_selection: None,
            text_area_preedit: None,
            ..self.clone()
        }
    }
}
