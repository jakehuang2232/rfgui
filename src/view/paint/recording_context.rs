//! Recorder-side capability context.
//!
//! Split out of `artifact.rs` so the artifact data model — what the V2
//! layerizer consumes — stays free of the pre-V2 admission types. This module
//! is held to the same rule: a recording capability is either a generic
//! property witness or a recorder-derived behavior flag that coverage
//! recomputes for every node it walks.

use crate::view::compositor::property_tree::{
    ClipNodeId, ClipNodeRole, EffectNodeId, PropertyTreeState, ScrollNodeId, TransformNodeId,
};
use crate::view::node_arena::NodeKey;

use super::artifact::*;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct PaintRecordingContext {
    pub(crate) paint_offset: [f32; 2],
    pub(crate) inside_text_area: bool,
    /// Path-scoped authority for a single projection-owned Text selection.
    /// The coverage walker derives this independently for every child edge;
    /// it must never be treated as ambient frame state.
    pub(crate) text_area_selection: Option<PaintTextSelectionWitness>,
    /// Path-scoped authority for the single projection-owned `Text` that
    /// contains the active IME preedit. Geometry decorations remain owned by
    /// the TextArea; this witness only proves which child glyph payload may
    /// contain the transient insertion.
    pub(crate) text_area_preedit: Option<PaintTextPreeditWitness>,
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
    pub(crate) deferred_viewport_self_clip: Option<PaintDeferredViewportSelfClipWitness>,
    /// Bound only for the deferred root's late-phase coverage invocation when
    /// the same root owns the active effect-surface contract.
    pub(crate) deferred_viewport_effect: Option<PaintDeferredViewportEffectWitness>,
    /// Owner-scoped proof that this coverage invocation belongs to one
    /// canonically planned transform surface. The normal frame recorder never
    /// installs this witness; the surface recorder clears and rebinds it for
    /// every canonical traversal owner.
    pub(crate) transform_surface: Option<PaintTransformSurfaceWitness>,
    /// Generic C3b Surface DAG policy bit. Coverage copies it from the
    /// recorder and rebinds its owner-scoped property authorities from the
    /// current owner's state after every component hook. Artifact validation
    /// remains responsible for proving the resulting snapshot store.
    pub(crate) surface_dag: bool,
    /// Owner-scoped transform accepted by the generic Surface DAG
    /// recorder. Coverage overwrites it before every node paints, so it cannot
    /// become ambient authority inherited from a component context hook.
    pub(crate) surface_dag_transform: Option<TransformNodeId>,
    /// Owner-scoped scroll root accepted by the generic Surface DAG recorder.
    /// Coverage binds it only when this owner's descendants state names the
    /// same owner-keyed scroll node, so foreign scroll authority cannot leak
    /// through copied component context.
    pub(crate) surface_dag_scroll: Option<ScrollNodeId>,
    /// Recorder-owned authority for the one exact M10E1A root/child path.
    /// Coverage clears and rebinds this after every component hook.
    pub(crate) baked_scroll_host: Option<PaintBakedScrollHostWitness>,
    /// Narrow frame-root receiver authority to encode the scroll host's
    /// retained child mask around a detached content marker. Older baked-host
    /// recorders keep their established H/C/O grammar and leave this false.
    pub(crate) frame_root_scroll_host_child_mask: bool,
    /// Owner-bound proof that one ancestor property is already represented by
    /// the parent retained surface.  Recording may project only that exact
    /// property out of the artifact view; live property-tree state remains
    /// untouched and every other property family is preserved verbatim.
    pub(crate) consumed_ancestor_property: Option<ConsumedAncestorProperty>,
    /// B4 receiver recording may have to project more than one already-owned
    /// ancestor boundary (for example transform + scroll contents).  This is
    /// a fixed-capacity, planner-sealed stack so component hooks cannot append,
    /// reorder, or retarget capabilities while coverage walks the subtree.
    pub(crate) consumed_ancestor_property_stack: Option<ConsumedAncestorPropertyStackWitness>,
    /// Recorder-bound projection token minted from a complete, immutable
    /// root-to-surface Transform/Effect boundary chain. The owning chain
    /// witness remains with coverage; component hooks only receive this
    /// target-scoped, Copy projection capability.
    pub(crate) property_forest_projection: Option<PropertyForestProjectionToken>,
    /// Boundary-local arbitrary-depth host projection. It projects the parent
    /// S/C pair from host self paint while preserving this boundary's own S/C
    /// pair on descendants.
    pub(crate) scroll_forest_host: Option<PaintScrollForestEdgeWitness>,
    /// Recorder-derived proof that this exact node's self paint is recorded on
    /// a detached local basis, so it consumes `paint_offset`. Coverage clears
    /// and recomputes it after every component context hook; a component that
    /// sets it is overwritten before its own paint runs.
    pub(crate) scroll_content_local_owner: bool,
    /// Recorder-derived proof that this exact node may keep one descendant
    /// contents clip inside the recording. Like the flag above it is per-node
    /// and never inherited: a child inherits the value by copy and coverage
    /// overwrites it before the child paints.
    pub(crate) descendant_contents_clip: bool,
    /// Recorder-derived proof that a resident raster already owns this node's
    /// caret, so the recording must not paint a second one. Same per-node
    /// lifetime as the two flags above.
    pub(crate) resident_caret_suppressed: bool,
    /// Recorder-owned post-hook paint offset required by the bounded detached
    /// scroll-content canary. Coverage rebinds this after every component hook
    /// and compares bitwise; all other recording policies leave it absent.
    pub(crate) required_scroll_content_paint_offset_bits: Option<[u32; 2]>,
    pub(crate) opacity_authority: PaintOpacityAuthority,
}

impl PaintRecordingContext {
    pub(crate) fn authorizes_self_clip_for(self, stable_id: u64) -> bool {
        matches!(
            (
                self.recording_owner,
                self.recording_owner_stable_id,
                self.authoritative_self_clip,
            ),
            (Some(owner), Some(recording_stable_id), Some(clip))
                if recording_stable_id == stable_id
                    && clip.owner == owner
                    && clip.role == ClipNodeRole::SelfClip
        )
    }

    pub(crate) fn authorizes_deferred_viewport_self_clip_for(self, stable_id: u64) -> bool {
        matches!(
            (
                self.recording_owner,
                self.recording_owner_stable_id,
                self.deferred_viewport_self_clip,
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

    pub(crate) fn authorizes_deferred_viewport_effect_for(
        self,
        stable_id: u64,
        effect: EffectNodeId,
    ) -> bool {
        matches!(
            (
                self.recording_owner,
                self.recording_owner_stable_id,
                self.deferred_viewport_effect,
            ),
            (Some(owner), Some(recording_stable_id), Some(witness))
                if recording_stable_id == stable_id
                    && witness.is_canonical_for(
                        owner,
                        stable_id,
                        self.authoritative_self_clip,
                        effect,
                    )
        )
    }

    pub(crate) fn authorizes_transform_surface_owner(
        self,
        transform: Option<TransformNodeId>,
    ) -> bool {
        matches!(
            (self.recording_owner, self.transform_surface, transform),
            (Some(owner), Some(witness), Some(transform))
                if witness.target_owner == owner
                    && witness.transform == transform
                    && witness.transform.0 == witness.boundary_owner
        ) || matches!(
            (
                self.recording_owner,
                self.surface_dag,
                self.surface_dag_transform,
                transform,
            ),
            (Some(_), true, Some(expected), Some(actual)) if expected == actual
        )
    }

    pub(crate) fn authorizes_transform_surface_root(self, stable_id: u64) -> bool {
        matches!(
            (
                self.recording_owner,
                self.recording_owner_stable_id,
                self.transform_surface,
            ),
            (Some(owner), Some(recording_stable_id), Some(witness))
                if recording_stable_id == stable_id
                    && witness.target_owner == owner
                    && witness.boundary_owner == owner
                    && witness.transform == TransformNodeId(owner)
        ) || matches!(
            (
                self.recording_owner,
                self.recording_owner_stable_id,
                self.surface_dag,
                self.surface_dag_transform,
            ),
            (Some(owner), Some(recording_stable_id), true, Some(transform))
                if recording_stable_id == stable_id && transform == TransformNodeId(owner)
        )
    }

    pub(crate) fn authorizes_baked_scroll_host_root(self, stable_id: u64) -> bool {
        matches!(
            (
                self.recording_owner,
                self.recording_owner_stable_id,
                self.baked_scroll_host,
            ),
            (Some(owner), Some(recording_stable_id), Some(witness))
                if recording_stable_id == stable_id
                    && witness.boundary_root() == owner
                    && witness.target_owner() == owner
        )
    }

    /// Generic owner-scoped scroll authority. This is deliberately separate
    /// from the exact baked-scroll witness: callers may admit the generic
    /// Surface DAG path without making the infallible exact snapshot accessor
    /// observe an authority that carries no witness.
    pub(crate) fn authorizes_generic_scroll_host_root(self, stable_id: u64) -> bool {
        matches!(
            (
                self.recording_owner,
                self.recording_owner_stable_id,
                self.surface_dag,
                self.surface_dag_scroll,
            ),
            (Some(owner), Some(recording_stable_id), true, Some(scroll))
                if recording_stable_id == stable_id && scroll == ScrollNodeId(owner)
        )
    }

    pub(crate) fn authorizes_frame_root_scroll_host_child_mask(self, stable_id: u64) -> bool {
        self.frame_root_scroll_host_child_mask && self.authorizes_baked_scroll_host_root(stable_id)
    }

    pub(crate) fn baked_scroll_host_snapshot_for_root(
        self,
        stable_id: u64,
    ) -> Option<crate::view::compositor::property_tree::ScrollNodeSnapshot> {
        self.authorizes_baked_scroll_host_root(stable_id).then(|| {
            self.baked_scroll_host
                .expect("authority requires witness")
                .scroll_snapshot()
        })
    }

    pub(crate) fn without_text_area_child_authority(mut self) -> Self {
        self.text_area_selection = None;
        self.text_area_preedit = None;
        self
    }

    pub(crate) fn paint_opacity(self, baked_opacity: f32) -> f32 {
        match self.opacity_authority {
            PaintOpacityAuthority::Baked => baked_opacity.clamp(0.0, 1.0),
            PaintOpacityAuthority::NeutralRootEffect(_) => 1.0,
        }
    }

    pub(crate) fn project_consumed_ancestor_property(
        self,
        live: PropertyTreeState,
    ) -> Option<PropertyTreeState> {
        if let Some(witness) = self.scroll_forest_host {
            return witness.project_host_for(self.recording_owner?, live);
        }
        if let Some(stack) = self.consumed_ancestor_property_stack {
            return stack.project_for(self.recording_owner?, live, self.opacity_authority);
        }
        if let Some(projection) = self.property_forest_projection {
            return projection.project_for(self.recording_owner?, live, self.opacity_authority);
        }
        match self.consumed_ancestor_property {
            None => Some(live),
            Some(ConsumedAncestorProperty::Transform(witness)) => {
                if witness.is_canonical_for(self.recording_owner?)
                    && live.transform == Some(witness.transform)
                {
                    Some(PropertyTreeState {
                        transform: None,
                        ..live
                    })
                } else {
                    None
                }
            }
            Some(ConsumedAncestorProperty::SameOwnerTransformBoundary(witness)) => {
                if witness.is_canonical_for(self.recording_owner?)
                    && live.transform == Some(witness.transform)
                {
                    Some(PropertyTreeState {
                        transform: None,
                        ..live
                    })
                } else {
                    None
                }
            }
            Some(ConsumedAncestorProperty::SameOwnerEffectBoundary(witness)) => {
                if witness.is_canonical_for(self.recording_owner?)
                    && self.opacity_authority
                        == PaintOpacityAuthority::NeutralRootEffect(witness.effect.id)
                    && live.effect == witness.expected_before
                {
                    Some(PropertyTreeState {
                        effect: witness.projected_after,
                        ..live
                    })
                } else {
                    None
                }
            }
            Some(ConsumedAncestorProperty::ScrollContents(witness)) => {
                if witness.is_canonical_for(self.recording_owner?)
                    && live.scroll == Some(witness.scroll)
                    && live.clip == Some(witness.contents_clip)
                {
                    Some(PropertyTreeState {
                        clip: None,
                        scroll: None,
                        ..live
                    })
                } else {
                    None
                }
            }
            Some(ConsumedAncestorProperty::Effect(witness)) => {
                if witness.is_canonical_for(self.recording_owner?)
                    && self.opacity_authority
                        == PaintOpacityAuthority::NeutralRootEffect(witness.effect.id)
                    && live.effect == witness.expected_before
                {
                    Some(PropertyTreeState {
                        effect: witness.projected_after,
                        ..live
                    })
                } else {
                    None
                }
            }
        }
    }

    pub(crate) fn authorizes_scroll_content_local_owner(self, owner: NodeKey) -> bool {
        if self.recording_owner != Some(owner) {
            return false;
        }
        matches!(
            self.consumed_ancestor_property,
            Some(ConsumedAncestorProperty::ScrollContents(witness))
                if witness.is_canonical_for(owner)
        ) || self.consumed_ancestor_property_stack.is_some_and(|stack| {
            stack.authorizes_scroll_content_local_owner(owner, self.opacity_authority)
        }) || self.scroll_content_local_owner
    }

    /// One node inside a recording may keep a descendant contents clip. The
    /// authority is minted per node by coverage, so a component hook cannot
    /// carry it to a sibling or reuse it for an unrelated child-clip topology.
    pub(crate) fn authorizes_descendant_contents_clip(self, stable_id: u64) -> bool {
        self.descendant_contents_clip
            && self.recording_owner.is_some()
            && self.recording_owner_stable_id == Some(stable_id)
    }

    pub(crate) fn suppresses_resident_caret(self, owner: NodeKey) -> bool {
        self.resident_caret_suppressed && self.recording_owner == Some(owner)
    }
}
