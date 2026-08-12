//! Legacy retained admission types.
//!
//! Every item in this module serves only the pre-V2 exact-shape retained
//! middle layer. It carries no durable paint semantics: the generic artifact,
//! payload identity, composite edges, and artifact-space transition already
//! express everything the V2 layerizer consumes.
//!
//! This whole file is deleted in the Stage C hard-cutover change set that
//! removes the old retained middle layer, and nothing outside the legacy
//! admission/planner path may depend on it.
//!
//! Adding items follows one rule, not a blanket ban. A **new** legacy
//! capability is forbidden — the answer to a missing capability is a generic
//! one, or none. Moving an **existing** exact proof in is allowed, and only
//! for one reason: to break a coupling that would otherwise keep exact-shape
//! grammar alive inside a durable module. Anything moved in must join the
//! closed set and the Stage C compile-time deletion inventory in the same
//! change, so the file can never grow a member the cutover would miss.

use crate::view::compositor::property_tree::{
    ClipBehavior, ClipNodeRole, ClipNodeSnapshot, PropertyTreeState,
};
use crate::view::node_arena::NodeKey;

use super::artifact::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PaintScrollAtomicProjectionSelectionTextAreaSubtreeWitness {
    property: PaintScrollDetachedProjectionSubtreeWitness,
    pub(crate) selection: PaintTextSelectionSource,
}

impl PaintScrollAtomicProjectionSelectionTextAreaSubtreeWitness {
    pub(crate) fn new(
        outer: PaintScrollContentWitness,
        text_area_root: NodeKey,
        live_contents_clip: ClipNodeSnapshot,
        local_logical_scissor: [u32; 4],
        selection: PaintTextSelectionSource,
    ) -> Option<Self> {
        if !selection.is_canonical() {
            return None;
        }
        Some(Self {
            property: PaintScrollDetachedProjectionSubtreeWitness::new(
                outer,
                text_area_root,
                live_contents_clip,
                local_logical_scissor,
            )?,
            selection,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PaintScrollFocusedAtomicProjectionTextAreaSubtreeWitness {
    property: PaintScrollDetachedProjectionSubtreeWitness,
}

impl PaintScrollFocusedAtomicProjectionTextAreaSubtreeWitness {
    pub(crate) fn new(
        outer: PaintScrollContentWitness,
        text_area_root: NodeKey,
        live_contents_clip: ClipNodeSnapshot,
        local_logical_scissor: [u32; 4],
    ) -> Option<Self> {
        Some(Self {
            property: PaintScrollDetachedProjectionSubtreeWitness::new(
                outer, text_area_root, live_contents_clip, local_logical_scissor,
            )?,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PaintScrollAtomicProjectionTextAreaRecorderWitness {
    ExistingAtomicGlyph(PaintScrollDetachedProjectionSubtreeWitness),
    AtomicProjectionSelection(PaintScrollAtomicProjectionSelectionTextAreaSubtreeWitness),
    FocusedAtomicProjectionGlyph(PaintScrollFocusedAtomicProjectionTextAreaSubtreeWitness),
}

impl PaintScrollAtomicProjectionTextAreaRecorderWitness {
    pub(crate) fn property(self) -> PaintScrollDetachedProjectionSubtreeWitness {
        match self {
            Self::ExistingAtomicGlyph(witness) => witness,
            Self::AtomicProjectionSelection(witness) => witness.property,
            Self::FocusedAtomicProjectionGlyph(witness) => witness.property,
        }
    }

    pub(super) fn is_canonical_for(self, owner: NodeKey) -> bool {
        self.property().is_canonical_for(owner)
            && match self {
                Self::ExistingAtomicGlyph(_) => true,
                Self::AtomicProjectionSelection(witness) => {
                    witness.selection.is_canonical()
                }
                Self::FocusedAtomicProjectionGlyph(_) => true,
            }
    }

    pub(crate) fn outer(self) -> PaintScrollContentWitness {
        self.property().outer()
    }

    pub(crate) fn live_contents_clip(self) -> ClipNodeSnapshot {
        self.property().live_contents_clip()
    }

    pub(crate) fn local_contents_clip(self) -> ClipNodeSnapshot {
        self.property().local_contents_clip()
    }

    pub(crate) fn detach_clip_snapshot(
        self,
        live: &[ClipNodeSnapshot],
    ) -> Option<Vec<ClipNodeSnapshot>> {
        self.property().detach_clip_snapshot(live)
    }

    pub(crate) fn for_target(self, target_owner: NodeKey) -> Self {
        match self {
            Self::ExistingAtomicGlyph(witness) => {
                Self::ExistingAtomicGlyph(witness.for_target(target_owner))
            }
            Self::AtomicProjectionSelection(witness) => Self::AtomicProjectionSelection(
                PaintScrollAtomicProjectionSelectionTextAreaSubtreeWitness {
                    property: witness.property.for_target(target_owner),
                    ..witness
                },
            ),
            Self::FocusedAtomicProjectionGlyph(witness) => Self::FocusedAtomicProjectionGlyph(
                PaintScrollFocusedAtomicProjectionTextAreaSubtreeWitness {
                    property: witness.property.for_target(target_owner),
                },
            ),
        }
    }

    pub(crate) fn project_for(
        self,
        owner: NodeKey,
        live: PropertyTreeState,
    ) -> Option<PropertyTreeState> {
        self.property().project_for(owner, live)
    }
}

/// Recorder-owned C1/C2a projection for `ScrollContents -> Element -> TextArea`.
/// The live TextArea clip is frozen together with its detached replacement, so
/// coverage can consume only the exact outer suffix and cannot reconstruct clip
/// geometry from an arbitrary component hook.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PaintScrollTextAreaSubtreeWitness {
    outer: PaintScrollContentWitness,
    text_area_root: NodeKey,
    live_contents_clip: ClipNodeSnapshot,
    local_contents_clip: ClipNodeSnapshot,
    paint_source: PaintTextContentSource,
    target_owner: NodeKey,
}

impl PaintScrollTextAreaSubtreeWitness {
    pub(crate) fn new(
        outer: PaintScrollContentWitness,
        text_area_root: NodeKey,
        live_contents_clip: ClipNodeSnapshot,
        local_logical_scissor: [u32; 4],
        paint_source: PaintTextContentSource,
    ) -> Option<Self> {
        let outer_clip = outer.contents_clip_snapshot();
        let local_contents_clip = ClipNodeSnapshot {
            parent: None,
            logical_scissor: local_logical_scissor,
            generation: DETACHED_LOCAL_CLIP_GENERATION,
            ..live_contents_clip
        };
        (text_area_root != outer.boundary_root()
            && text_area_root != outer.content_root()
            && live_contents_clip.id.owner == text_area_root
            && live_contents_clip.id.role == ClipNodeRole::ContentsClip
            && live_contents_clip.owner == text_area_root
            && live_contents_clip.parent == Some(outer_clip.id)
            && live_contents_clip.behavior == ClipBehavior::Intersect
            && live_contents_clip.generation != 0
            && paint_source.is_canonical())
        .then_some(Self {
            outer,
            text_area_root,
            live_contents_clip,
            local_contents_clip,
            paint_source,
            target_owner: outer.content_root(),
        })
    }

    pub(crate) fn outer(self) -> PaintScrollContentWitness {
        self.outer
    }

    pub(crate) fn text_area_root(self) -> NodeKey {
        self.text_area_root
    }

    pub(crate) fn live_contents_clip(self) -> ClipNodeSnapshot {
        self.live_contents_clip
    }

    pub(crate) fn local_contents_clip(self) -> ClipNodeSnapshot {
        self.local_contents_clip
    }

    pub(crate) fn paint_source(self) -> PaintTextContentSource {
        self.paint_source
    }

    pub(crate) fn for_target(self, target_owner: NodeKey) -> Self {
        Self {
            target_owner,
            ..self
        }
    }

    pub(super) fn is_canonical_for(self, owner: NodeKey) -> bool {
        self.target_owner == owner
            && self.live_contents_clip.id.owner == self.text_area_root
            && self.live_contents_clip.owner == self.text_area_root
            && self.live_contents_clip.id.role == ClipNodeRole::ContentsClip
            && self.live_contents_clip.parent == Some(self.outer.contents_clip_snapshot().id)
            && self.live_contents_clip.behavior == ClipBehavior::Intersect
            && self.live_contents_clip.generation != 0
            && self.local_contents_clip.id == self.live_contents_clip.id
            && self.local_contents_clip.owner == self.live_contents_clip.owner
            && self.local_contents_clip.parent.is_none()
            && self.local_contents_clip.behavior == self.live_contents_clip.behavior
            && self.local_contents_clip.generation == DETACHED_LOCAL_CLIP_GENERATION
            && self.paint_source.is_canonical()
    }

    pub(super) fn project_for(self, owner: NodeKey, live: PropertyTreeState) -> Option<PropertyTreeState> {
        if !self.is_canonical_for(owner)
            || live.transform.is_some()
            || live.effect.is_some()
            || live.scroll != Some(self.outer.scroll_snapshot().id)
        {
            return None;
        }
        if live.clip == Some(self.outer.contents_clip_snapshot().id) {
            Some(PropertyTreeState {
                transform: None,
                clip: None,
                effect: None,
                scroll: None,
                ..live
            })
        } else if live.clip == Some(self.live_contents_clip.id) {
            Some(PropertyTreeState {
                transform: None,
                clip: Some(self.local_contents_clip.id),
                effect: None,
                scroll: None,
                ..live
            })
        } else {
            None
        }
    }

    pub(crate) fn detach_clip_snapshot(
        self,
        live: &[ClipNodeSnapshot],
    ) -> Option<Vec<ClipNodeSnapshot>> {
        if live.is_empty() {
            return Some(Vec::new());
        }
        (live == [self.live_contents_clip, self.outer.contents_clip_snapshot()])
            .then(|| vec![self.local_contents_clip])
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PaintScrollInteractiveTextAreaSubtreeWitness {
    outer: PaintScrollContentWitness,
    text_area_root: NodeKey,
    live_contents_clip: ClipNodeSnapshot,
    local_contents_clip: ClipNodeSnapshot,
    paint_source: PaintTextContentSource,
    target_owner: NodeKey,
}

impl PaintScrollInteractiveTextAreaSubtreeWitness {
    pub(crate) fn new(
        outer: PaintScrollContentWitness,
        text_area_root: NodeKey,
        live_contents_clip: ClipNodeSnapshot,
        local_logical_scissor: [u32; 4],
        paint_source: PaintTextContentSource,
    ) -> Option<Self> {
        let outer_clip = outer.contents_clip_snapshot();
        let local_contents_clip = ClipNodeSnapshot {
            parent: None,
            logical_scissor: local_logical_scissor,
            generation: DETACHED_LOCAL_CLIP_GENERATION,
            ..live_contents_clip
        };
        (text_area_root != outer.boundary_root()
            && text_area_root != outer.content_root()
            && live_contents_clip.id.owner == text_area_root
            && live_contents_clip.id.role == ClipNodeRole::ContentsClip
            && live_contents_clip.owner == text_area_root
            && live_contents_clip.parent == Some(outer_clip.id)
            && live_contents_clip.behavior == ClipBehavior::Intersect
            && live_contents_clip.generation != 0
            && paint_source.is_canonical())
        .then_some(Self {
            outer,
            text_area_root,
            live_contents_clip,
            local_contents_clip,
            paint_source,
            target_owner: outer.content_root(),
        })
    }

    pub(crate) fn outer(self) -> PaintScrollContentWitness {
        self.outer
    }

    pub(crate) fn text_area_root(self) -> NodeKey {
        self.text_area_root
    }

    pub(crate) fn live_contents_clip(self) -> ClipNodeSnapshot {
        self.live_contents_clip
    }

    pub(crate) fn local_contents_clip(self) -> ClipNodeSnapshot {
        self.local_contents_clip
    }

    pub(crate) fn for_target(self, target_owner: NodeKey) -> Self {
        Self {
            target_owner,
            ..self
        }
    }

    pub(super) fn is_canonical_for(self, owner: NodeKey) -> bool {
        self.target_owner == owner
            && self.live_contents_clip.id.owner == self.text_area_root
            && self.live_contents_clip.owner == self.text_area_root
            && self.live_contents_clip.id.role == ClipNodeRole::ContentsClip
            && self.live_contents_clip.parent == Some(self.outer.contents_clip_snapshot().id)
            && self.live_contents_clip.behavior == ClipBehavior::Intersect
            && self.live_contents_clip.generation != 0
            && self.local_contents_clip.id == self.live_contents_clip.id
            && self.local_contents_clip.owner == self.live_contents_clip.owner
            && self.local_contents_clip.parent.is_none()
            && self.local_contents_clip.behavior == self.live_contents_clip.behavior
            && self.local_contents_clip.generation == DETACHED_LOCAL_CLIP_GENERATION
            && self.paint_source.is_canonical()
    }

    pub(super) fn project_for(self, owner: NodeKey, live: PropertyTreeState) -> Option<PropertyTreeState> {
        if !self.is_canonical_for(owner)
            || live.transform.is_some()
            || live.effect.is_some()
            || live.scroll != Some(self.outer.scroll_snapshot().id)
        {
            return None;
        }
        if live.clip == Some(self.outer.contents_clip_snapshot().id) {
            Some(PropertyTreeState {
                transform: None,
                clip: None,
                effect: None,
                scroll: None,
                ..live
            })
        } else if live.clip == Some(self.live_contents_clip.id) {
            Some(PropertyTreeState {
                transform: None,
                clip: Some(self.local_contents_clip.id),
                effect: None,
                scroll: None,
                ..live
            })
        } else {
            None
        }
    }

    pub(crate) fn detach_clip_snapshot(
        self,
        live: &[ClipNodeSnapshot],
    ) -> Option<Vec<ClipNodeSnapshot>> {
        if live.is_empty() {
            return Some(Vec::new());
        }
        (live == [self.live_contents_clip, self.outer.contents_clip_snapshot()])
            .then(|| vec![self.local_contents_clip])
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RetainedInteractiveTextAreaResidentRasterSeal {
    FocusedGlyphs,
    FocusedSelectionGlyphs(TextSelectionPayloadIdentity),
    FocusedPreeditGlyphs(TextPreeditPayloadIdentity),
}

impl RetainedInteractiveTextAreaResidentRasterSeal {
    pub(crate) fn paint_source(&self) -> PaintTextContentSource {
        match self {
            Self::FocusedGlyphs => PaintTextContentSource::Glyphs,
            Self::FocusedSelectionGlyphs(seal) => PaintTextContentSource::Selection(
                PaintTextSelectionSource {
                    start_char: seal.start_char,
                    end_char: seal.end_char,
                    color_rgba_bits: seal.color_rgba_bits,
                },
            ),
            Self::FocusedPreeditGlyphs(_) => PaintTextContentSource::Preedit,
        }
    }

    pub(crate) fn is_canonical_for(
        &self,
        source: PaintTextContentSource,
    ) -> bool {
        match (self, source) {
            (Self::FocusedGlyphs, PaintTextContentSource::Glyphs) => true,
            (Self::FocusedSelectionGlyphs(seal), PaintTextContentSource::Selection(source)) => {
                source.matches_payload(seal)
            }
            (Self::FocusedPreeditGlyphs(seal), PaintTextContentSource::Preedit) => {
                seal.is_canonical()
            }
            _ => false,
        }
    }
}


// ---- legacy exact-shape projection tokens (moved from artifact.rs) ----

/// Component-independent proof that one detached scroll-content projection has
/// a canonical local clip: `local_contents_clip` is the live contents clip
/// re-based onto the detached surface. The token authorizes only that property
/// projection — never source topology, component semantics, or raster facts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PaintScrollDetachedProjectionSubtreeWitness {
    outer: PaintScrollContentWitness,
    projection_root: NodeKey,
    live_contents_clip: ClipNodeSnapshot,
    local_contents_clip: ClipNodeSnapshot,
    target_owner: NodeKey,
}

impl PaintScrollDetachedProjectionSubtreeWitness {
    pub(crate) fn new(
        outer: PaintScrollContentWitness,
        projection_root: NodeKey,
        live_contents_clip: ClipNodeSnapshot,
        local_logical_scissor: [u32; 4],
    ) -> Option<Self> {
        let outer_clip = outer.contents_clip_snapshot();
        let local_contents_clip = ClipNodeSnapshot {
            parent: None,
            logical_scissor: local_logical_scissor,
            generation: DETACHED_LOCAL_CLIP_GENERATION,
            ..live_contents_clip
        };
        (projection_root != outer.boundary_root()
            && projection_root != outer.content_root()
            && live_contents_clip.id.owner == projection_root
            && live_contents_clip.id.role == ClipNodeRole::ContentsClip
            && live_contents_clip.owner == projection_root
            && live_contents_clip.parent == Some(outer_clip.id)
            && live_contents_clip.behavior == ClipBehavior::Intersect
            && live_contents_clip.generation != 0)
        .then_some(Self {
            outer,
            projection_root,
            live_contents_clip,
            local_contents_clip,
            target_owner: outer.content_root(),
        })
    }

    pub(crate) fn outer(self) -> PaintScrollContentWitness {
        self.outer
    }
    pub(crate) fn projection_root(self) -> NodeKey {
        self.projection_root
    }
    pub(crate) fn live_contents_clip(self) -> ClipNodeSnapshot {
        self.live_contents_clip
    }
    pub(crate) fn local_contents_clip(self) -> ClipNodeSnapshot {
        self.local_contents_clip
    }
    pub(crate) fn for_target(self, target_owner: NodeKey) -> Self {
        Self {
            target_owner,
            ..self
        }
    }

    pub(super) fn is_canonical_for(self, owner: NodeKey) -> bool {
        self.target_owner == owner
            && self.live_contents_clip.id.owner == self.projection_root
            && self.live_contents_clip.owner == self.projection_root
            && self.live_contents_clip.id.role == ClipNodeRole::ContentsClip
            && self.live_contents_clip.parent == Some(self.outer.contents_clip_snapshot().id)
            && self.live_contents_clip.behavior == ClipBehavior::Intersect
            && self.live_contents_clip.generation != 0
            && self.local_contents_clip.id == self.live_contents_clip.id
            && self.local_contents_clip.owner == self.live_contents_clip.owner
            && self.local_contents_clip.parent.is_none()
            && self.local_contents_clip.behavior == self.live_contents_clip.behavior
            && self.local_contents_clip.generation == DETACHED_LOCAL_CLIP_GENERATION
    }

    pub(super) fn project_for(self, owner: NodeKey, live: PropertyTreeState) -> Option<PropertyTreeState> {
        if !self.is_canonical_for(owner)
            || live.transform.is_some()
            || live.effect.is_some()
            || live.scroll != Some(self.outer.scroll_snapshot().id)
        {
            return None;
        }
        if live.clip == Some(self.outer.contents_clip_snapshot().id) {
            Some(PropertyTreeState {
                transform: None,
                clip: None,
                effect: None,
                scroll: None,
                ..live
            })
        } else if live.clip == Some(self.live_contents_clip.id) {
            Some(PropertyTreeState {
                transform: None,
                clip: Some(self.local_contents_clip.id),
                effect: None,
                scroll: None,
                ..live
            })
        } else {
            None
        }
    }

    pub(crate) fn detach_clip_snapshot(
        self,
        live: &[ClipNodeSnapshot],
    ) -> Option<Vec<ClipNodeSnapshot>> {
        if live.is_empty() {
            return Some(Vec::new());
        }
        (live == [self.live_contents_clip, self.outer.contents_clip_snapshot()])
            .then(|| vec![self.local_contents_clip])
    }
}

/// Coverage-side authority for the one exact detached subtree a legacy
/// recording may carry.
///
/// The six pre-V2 admission capabilities are mutually exclusive — a recording
/// is either a baked host or a detached local, and either the stable, the
/// atomic-projection, or the interactive grammar — so coverage carries this one
/// value instead of six parallel capability fields. It never reaches
/// `PaintRecordingContext`, the artifact, or any payload identity.
///
/// Two kinds of accessor, with different guarantees. Among the node-facing ones
/// — `project_for` and the three `authorizes_*` / `suppresses_*` predicates —
/// any branch that grants a projected state or a behavior revalidates the
/// witness against the walked node; a variant the branch does not apply to
/// returns `NoAuthority` or `false` without granting anything, so none of them
/// can act as ambient subtree state either way. The structural ones — `outer`,
/// `detaches_clip_snapshot`, `detach_clip_snapshot` — revalidate nothing: they
/// read the witness's own frozen fields, which are private, immutable, and
/// could only have been produced by a canonical mint. `detach_clip_snapshot`
/// still fails closed, but on the recorded clip chain rather than on the node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PaintLegacyTextAreaCoverageAuthority {
    Local(PaintScrollTextAreaSubtreeWitness),
    BakedHost(PaintScrollTextAreaSubtreeWitness),
    AtomicProjectionLocal(PaintScrollAtomicProjectionTextAreaRecorderWitness),
    AtomicProjectionBakedHost(PaintScrollAtomicProjectionTextAreaRecorderWitness),
    InteractiveLocal(PaintScrollInteractiveTextAreaSubtreeWitness),
    InteractiveBakedHost(PaintScrollInteractiveTextAreaSubtreeWitness),
}

/// Outcome of asking a legacy authority to project one live property state.
///
/// `NoAuthority` and `Rejected` must stay distinguishable: the first means the
/// generic projection still owns this node, the second means a validated legacy
/// projection failed and the node has to fall back. Both are pure — a rejection
/// consumes nothing and mutates nothing. Collapsing them into `Option` would
/// let a node whose exact authority failed be admitted by the weaker generic
/// projection instead, bypassing the failure.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum LegacyTextAreaProjection {
    NoAuthority,
    Projected(PropertyTreeState),
    Rejected,
}

impl PaintLegacyTextAreaCoverageAuthority {
    pub(crate) fn for_target(self, target_owner: NodeKey) -> Self {
        match self {
            Self::Local(witness) => Self::Local(witness.for_target(target_owner)),
            Self::BakedHost(witness) => Self::BakedHost(witness.for_target(target_owner)),
            Self::AtomicProjectionLocal(witness) => {
                Self::AtomicProjectionLocal(witness.for_target(target_owner))
            }
            Self::AtomicProjectionBakedHost(witness) => {
                Self::AtomicProjectionBakedHost(witness.for_target(target_owner))
            }
            Self::InteractiveLocal(witness) => {
                Self::InteractiveLocal(witness.for_target(target_owner))
            }
            Self::InteractiveBakedHost(witness) => {
                Self::InteractiveBakedHost(witness.for_target(target_owner))
            }
        }
    }

    pub(crate) fn outer(self) -> PaintScrollContentWitness {
        match self {
            Self::Local(witness) | Self::BakedHost(witness) => witness.outer(),
            Self::AtomicProjectionLocal(witness) | Self::AtomicProjectionBakedHost(witness) => {
                witness.outer()
            }
            Self::InteractiveLocal(witness) | Self::InteractiveBakedHost(witness) => {
                witness.outer()
            }
        }
    }

    fn is_canonical_for(self, owner: NodeKey) -> bool {
        match self {
            Self::Local(witness) | Self::BakedHost(witness) => witness.is_canonical_for(owner),
            Self::AtomicProjectionLocal(witness) | Self::AtomicProjectionBakedHost(witness) => {
                witness.is_canonical_for(owner)
            }
            Self::InteractiveLocal(witness) | Self::InteractiveBakedHost(witness) => {
                witness.is_canonical_for(owner)
            }
        }
    }

    /// Only the detached local half consumes live ancestor properties. A baked
    /// host records against unprojected live state, so it must leave the
    /// generic projection in charge.
    fn detached_local(self) -> bool {
        matches!(
            self,
            Self::Local(_) | Self::AtomicProjectionLocal(_) | Self::InteractiveLocal(_)
        )
    }

    pub(crate) fn project_for(
        self,
        owner: NodeKey,
        live: PropertyTreeState,
    ) -> LegacyTextAreaProjection {
        if !self.detached_local() {
            return LegacyTextAreaProjection::NoAuthority;
        }
        let projected = match self {
            Self::Local(witness) => witness.project_for(owner, live),
            Self::AtomicProjectionLocal(witness) => witness.project_for(owner, live),
            Self::InteractiveLocal(witness) => witness.project_for(owner, live),
            _ => None,
        };
        match projected {
            Some(projected) => LegacyTextAreaProjection::Projected(projected),
            None => LegacyTextAreaProjection::Rejected,
        }
    }

    /// Mirrors `project_for`: the recorded clip chain is rebased onto the
    /// detached surface only for the local half.
    pub(crate) fn detaches_clip_snapshot(self) -> bool {
        self.detached_local()
    }

    pub(crate) fn detach_clip_snapshot(
        self,
        live: &[ClipNodeSnapshot],
    ) -> Option<Vec<ClipNodeSnapshot>> {
        match self {
            Self::Local(witness) => witness.detach_clip_snapshot(live),
            Self::AtomicProjectionLocal(witness) => witness.detach_clip_snapshot(live),
            Self::InteractiveLocal(witness) => witness.detach_clip_snapshot(live),
            _ => None,
        }
    }

    /// Self paint of the detached content root is recorded on the local basis,
    /// so it consumes the recorder's normalization offset.
    pub(crate) fn authorizes_scroll_content_local_owner(self, owner: NodeKey) -> bool {
        self.detached_local() && self.is_canonical_for(owner)
    }

    /// The content root is the one node allowed to keep a descendant contents
    /// clip inside the recording; the clip itself stays frozen in the witness.
    pub(crate) fn authorizes_descendant_contents_clip(self, owner: NodeKey) -> bool {
        self.outer().content_root() == owner && self.is_canonical_for(owner)
    }

    /// The resident raster already owns the caret for these two grammars, so
    /// the recording must not paint a second, blinking one.
    pub(crate) fn suppresses_resident_caret(self, owner: NodeKey) -> bool {
        match self {
            Self::AtomicProjectionLocal(witness) | Self::AtomicProjectionBakedHost(witness) => {
                matches!(
                    witness,
                    PaintScrollAtomicProjectionTextAreaRecorderWitness::FocusedAtomicProjectionGlyph(
                        _
                    )
                ) && witness.property().projection_root() == owner
                    && witness.is_canonical_for(owner)
            }
            Self::InteractiveLocal(witness) | Self::InteractiveBakedHost(witness) => {
                witness.text_area_root() == owner && witness.is_canonical_for(owner)
            }
            Self::Local(_) | Self::BakedHost(_) => false,
        }
    }
}

// ---- exact TextArea admission proofs and their minting selectors ----
//
// Moved out of `element/mod.rs` so the built-in `Element` host owns no
// component-specific admission grammar. Everything below proves the pre-V2
// exact shapes only: the V2 layerizer consumes the generic source, transition,
// selection source, composite edges and property snapshots that these tokens
// hand on, never the tokens themselves.

use crate::view::base_component::text_area::{
    FocusedAtomicCaretSourceSeal, FocusedAtomicPreeditSourceSeal,
    RetainedAtomicProjectionSelectionTextAreaPaintGrammar,
    RetainedAtomicProjectionTextAreaPaintGrammar, RetainedFocusedAtomicProjectionTextAreaPaintGrammar,
    RetainedTextAreaPaintGrammar,
};
use crate::view::base_component::{
    Element, RetainedSurfaceBounds, Rect, ScrollGeometrySnapshot, ScrollbarOverlayWitness, TextArea,
};
use crate::view::node_arena::NodeArena;
use crate::view::base_component::ElementTrait;

// Short-lived duplicates of the `element/mod.rs` bitwise oracles. Copying them
// keeps the durable `Element` API from widening for a layer that is deleted
// whole in Stage C; they go with this file.
fn composite_bounds_bitwise_equal(
    left: RetainedSurfaceBounds,
    right: RetainedSurfaceBounds,
) -> bool {
    [left.x, left.y, left.width, left.height].map(f32::to_bits)
        == [right.x, right.y, right.width, right.height].map(f32::to_bits)
        && left.corner_radii.map(f32::to_bits) == right.corner_radii.map(f32::to_bits)
}

fn scroll_geometry_snapshot_matches_scroll_node(
    live: ScrollGeometrySnapshot,
    snapshot: crate::view::compositor::property_tree::ScrollNodeSnapshot,
) -> bool {
    live.configured_axis == snapshot.configured_axis
        && live.offset.map(f32::to_bits)
            == [snapshot.offset.x.to_bits(), snapshot.offset.y.to_bits()]
        && rects_bitwise_equal(live.scrollport_rect, snapshot.viewport)
        && live.content_size.map(f32::to_bits)
            == [
                snapshot.content_size.width.to_bits(),
                snapshot.content_size.height.to_bits(),
            ]
        && rects_bitwise_equal(
            live.layout_content_bounds_at_zero,
            snapshot.layout_content_bounds_at_zero,
        )
        && live.contents_clip == snapshot.contents_clip
        && scrollbar_overlays_bitwise_equal(live.scrollbar_overlay, snapshot.scrollbar_overlay)
}

fn scroll_geometry_snapshots_bitwise_equal(
    lhs: ScrollGeometrySnapshot,
    rhs: ScrollGeometrySnapshot,
) -> bool {
    lhs.configured_axis == rhs.configured_axis
        && lhs.offset.map(f32::to_bits) == rhs.offset.map(f32::to_bits)
        && rects_bitwise_equal(lhs.scrollport_rect, rhs.scrollport_rect)
        && lhs.content_size.map(f32::to_bits) == rhs.content_size.map(f32::to_bits)
        && rects_bitwise_equal(
            lhs.layout_content_bounds_at_zero,
            rhs.layout_content_bounds_at_zero,
        )
        && lhs.contents_clip == rhs.contents_clip
        && scrollbar_overlays_bitwise_equal(lhs.scrollbar_overlay, rhs.scrollbar_overlay)
}

fn optional_rects_bitwise_equal(lhs: Option<Rect>, rhs: Option<Rect>) -> bool {
    match (lhs, rhs) {
        (None, None) => true,
        (Some(lhs), Some(rhs)) => rects_bitwise_equal(lhs, rhs),
        _ => false,
    }
}

fn rects_bitwise_equal(lhs: Rect, rhs: Rect) -> bool {
    [lhs.x, lhs.y, lhs.width, lhs.height].map(f32::to_bits)
        == [rhs.x, rhs.y, rhs.width, rhs.height].map(f32::to_bits)
}

fn scrollbar_overlays_bitwise_equal(
    lhs: ScrollbarOverlayWitness,
    rhs: ScrollbarOverlayWitness,
) -> bool {
    optional_rects_bitwise_equal(lhs.vertical_track, rhs.vertical_track)
        && optional_rects_bitwise_equal(lhs.vertical_thumb, rhs.vertical_thumb)
        && optional_rects_bitwise_equal(lhs.horizontal_track, rhs.horizontal_track)
        && optional_rects_bitwise_equal(lhs.horizontal_thumb, rhs.horizontal_thumb)
        && lhs.interaction == rhs.interaction
        && lhs.paint_state == rhs.paint_state
        && lhs.sampled_alpha.to_bits() == rhs.sampled_alpha.to_bits()
        && lhs.shadow_blur_radius.to_bits() == rhs.shadow_blur_radius.to_bits()
}

/// Exact sibling admission for the first property-scroll TextArea subtree.
///
/// This deliberately does not widen `RetainedScrollHostAdmissionSnapshot` or
/// its direct-leaf oracle.  The admitted grammar is one scroll host, one
/// otherwise leaf-equivalent Element content wrapper, and one plain TextArea
/// subtree rooted at `text_area_root`. The frozen paint grammar distinguishes
/// C1 glyph-only content from C2a selection-underlay plus glyph content.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RetainedScrollTextAreaSubtreeAdmissionSnapshot {
    pub(crate) boundary_root: NodeKey,
    pub(crate) stable_id: u64,
    pub(crate) content_wrapper: NodeKey,
    pub(crate) content_wrapper_stable_id: u64,
    pub(crate) text_area_root: NodeKey,
    pub(crate) text_area_stable_id: u64,
    pub(crate) paint_source: PaintTextContentSource,
    pub(crate) source_bounds: RetainedSurfaceBounds,
    pub(crate) scroll: ScrollGeometrySnapshot,
}

impl RetainedScrollTextAreaSubtreeAdmissionSnapshot {
    pub(crate) fn bitwise_eq(self, other: Self) -> bool {
        self.boundary_root == other.boundary_root
            && self.stable_id == other.stable_id
            && self.content_wrapper == other.content_wrapper
            && self.content_wrapper_stable_id == other.content_wrapper_stable_id
            && self.text_area_root == other.text_area_root
            && self.text_area_stable_id == other.text_area_stable_id
            && self.paint_source.is_canonical()
            && other.paint_source.is_canonical()
            && self.paint_source == other.paint_source
            && scroll_geometry_snapshots_bitwise_equal(self.scroll, other.scroll)
            && composite_bounds_bitwise_equal(self.source_bounds, other.source_bounds)
    }

    pub(crate) fn matches_scroll_node(
        self,
        snapshot: crate::view::compositor::property_tree::ScrollNodeSnapshot,
    ) -> bool {
        scroll_geometry_snapshot_matches_scroll_node(self.scroll, snapshot)
    }

    pub(crate) fn matches_live_source(
        self,
        text_area: &TextArea,
        arena: &NodeArena,
        recording_offset: [f32; 2],
    ) -> bool {
        let live = if text_area.exact_retained_property_scroll_glyph_subtree(
            self.text_area_root,
            arena,
            recording_offset,
        ) {
            Some(RetainedTextAreaPaintGrammar::GlyphOnly)
        } else {
            text_area.exact_retained_property_scroll_selection_glyph_subtree(
                self.text_area_root,
                arena,
                recording_offset,
            )
        };
        live.and_then(|grammar| grammar.artifact_content_source()) == Some(self.paint_source)
    }
}

/// Exact sibling admission for one realized atomic TextArea projection whose
/// user subtree is exactly one bare static Text leaf. The recorder reruns the
/// live component oracle before and after recording, while the artifact path
/// consumes only the generic source and spatial transition facts below.
#[derive(Clone, Debug)]
pub(crate) struct RetainedScrollAtomicProjectionTextAreaSubtreeAdmissionSnapshot {
    pub(crate) boundary_root: NodeKey,
    pub(crate) stable_id: u64,
    pub(crate) content_wrapper: NodeKey,
    pub(crate) content_wrapper_stable_id: u64,
    pub(crate) text_area_root: NodeKey,
    pub(crate) text_area_stable_id: u64,
    paint_grammar: RetainedAtomicProjectionTextAreaPaintGrammar,
    pub(crate) artifact_source: PaintAtomicProjectionArtifactSource,
    pub(crate) artifact_space_transition: PaintArtifactSpaceTransition,
    pub(crate) source_bounds: RetainedSurfaceBounds,
    pub(crate) scroll: ScrollGeometrySnapshot,
}

impl RetainedScrollAtomicProjectionTextAreaSubtreeAdmissionSnapshot {
    pub(crate) fn bitwise_eq(&self, other: &Self) -> bool {
        self.boundary_root == other.boundary_root
            && self.stable_id == other.stable_id
            && self.content_wrapper == other.content_wrapper
            && self.content_wrapper_stable_id == other.content_wrapper_stable_id
            && self.text_area_root == other.text_area_root
            && self.text_area_stable_id == other.text_area_stable_id
            && self.paint_grammar.is_canonical()
            && other.paint_grammar.is_canonical()
            && self.paint_grammar == other.paint_grammar
            && self.artifact_source == other.artifact_source
            && self
                .artifact_space_transition
                .source_bits_eq(other.artifact_space_transition)
            && self
                .artifact_space_transition
                .semantic_revision_eq(other.artifact_space_transition)
            && scroll_geometry_snapshots_bitwise_equal(self.scroll, other.scroll)
            && composite_bounds_bitwise_equal(self.source_bounds, other.source_bounds)
    }

    pub(crate) fn matches_scroll_node(
        &self,
        snapshot: crate::view::compositor::property_tree::ScrollNodeSnapshot,
    ) -> bool {
        scroll_geometry_snapshot_matches_scroll_node(self.scroll, snapshot)
    }

    /// Narrow tamper seam for the frozen-replay matrices.
    ///
    /// The grammar is private proof; a test must be able to drift it without
    /// the field becoming writable to the whole crate.
    #[cfg(test)]
    pub(crate) fn tamper_projection_text_stable_id_for_test(&mut self) {
        self.paint_grammar.projection_text_stable_id ^= 1;
    }

    /// Read-only grammar access for the frozen-replay tests. Production reads
    /// the generic source, transition and selection facts instead.
    #[cfg(test)]
    pub(crate) fn paint_grammar_for_test(&self) -> &RetainedAtomicProjectionTextAreaPaintGrammar {
        &self.paint_grammar
    }


    pub(crate) fn matches_live_source(
        &self,
        text_area: &TextArea,
        arena: &NodeArena,
        recording_offset: [f32; 2],
    ) -> bool {
        text_area
            .exact_retained_property_scroll_atomic_projection_subtree(
                self.text_area_root,
                arena,
                recording_offset,
            )
            .is_some_and(|grammar| {
                grammar == self.paint_grammar
                    && grammar.artifact_source(self.text_area_root).as_ref()
                        == Some(&self.artifact_source)
                    && grammar.artifact_space_transition().is_some_and(|expected| {
                        self.artifact_space_transition
                            .validate_expected_for_owner(self.text_area_root, expected)
                            .is_ok()
                    })
            })
    }
}

/// Exact sibling admission for one root-owned nonempty TextArea selection and
/// one realized atomic projection.
#[derive(Clone, Debug)]
pub(crate) struct RetainedScrollAtomicProjectionSelectionTextAreaSubtreeAdmissionSnapshot {
    pub(crate) boundary_root: NodeKey,
    pub(crate) stable_id: u64,
    pub(crate) content_wrapper: NodeKey,
    pub(crate) content_wrapper_stable_id: u64,
    pub(crate) text_area_root: NodeKey,
    pub(crate) text_area_stable_id: u64,
    paint_grammar:
        RetainedAtomicProjectionSelectionTextAreaPaintGrammar,
    pub(crate) artifact_source: PaintAtomicProjectionArtifactSource,
    pub(crate) selection_source: PaintTextSelectionSource,
    pub(crate) artifact_space_transition: PaintArtifactSpaceTransition,
    pub(crate) source_bounds: RetainedSurfaceBounds,
    pub(crate) scroll: ScrollGeometrySnapshot,
}

impl RetainedScrollAtomicProjectionSelectionTextAreaSubtreeAdmissionSnapshot {
    pub(crate) fn bitwise_eq(&self, other: &Self) -> bool {
        self.boundary_root == other.boundary_root
            && self.stable_id == other.stable_id
            && self.content_wrapper == other.content_wrapper
            && self.content_wrapper_stable_id == other.content_wrapper_stable_id
            && self.text_area_root == other.text_area_root
            && self.text_area_stable_id == other.text_area_stable_id
            && self.paint_grammar.is_canonical()
            && other.paint_grammar.is_canonical()
            && self.paint_grammar == other.paint_grammar
            && self.artifact_source == other.artifact_source
            && self.selection_source == other.selection_source
            && self
                .artifact_space_transition
                .source_bits_eq(other.artifact_space_transition)
            && self
                .artifact_space_transition
                .semantic_revision_eq(other.artifact_space_transition)
            && scroll_geometry_snapshots_bitwise_equal(self.scroll, other.scroll)
            && composite_bounds_bitwise_equal(self.source_bounds, other.source_bounds)
    }

    pub(crate) fn matches_scroll_node(
        &self,
        snapshot: crate::view::compositor::property_tree::ScrollNodeSnapshot,
    ) -> bool {
        scroll_geometry_snapshot_matches_scroll_node(self.scroll, snapshot)
    }
    /// Narrow tamper seam for the frozen-replay matrices. See the sibling
    /// token's seam: the grammar stays private proof.
    #[cfg(test)]
    pub(crate) fn tamper_projection_text_stable_id_for_test(&mut self) {
        self.paint_grammar.atomic_source.projection_text_stable_id ^= 1;
    }

    /// Drifts the frozen IFC apply revision without touching the generic
    /// transition, so a test can prove the two are checked independently.
    #[cfg(test)]
    pub(crate) fn tamper_unified_apply_revision_for_test(&mut self) {
        self.paint_grammar.atomic_source.last_unified_apply_bits.2 += 1;
    }

    /// Drifts the frozen selection extent without touching
    /// `selection_source`, so a test can prove the same for the selection.
    #[cfg(test)]
    pub(crate) fn tamper_selection_end_char_for_test(&mut self) {
        let RetainedTextAreaPaintGrammar::SelectionGlyphs { end_char, .. } =
            &mut self.paint_grammar.selection
        else {
            panic!("selection grammar must retain a selection source")
        };
        *end_char += 1;
    }

    /// Read-only grammar access for the frozen-replay tests. Production reads
    /// the generic source, transition and selection facts instead.
    #[cfg(test)]
    pub(crate) fn paint_grammar_for_test(&self) -> &RetainedAtomicProjectionSelectionTextAreaPaintGrammar {
        &self.paint_grammar
    }


    pub(crate) fn matches_live_source(
        &self,
        text_area: &TextArea,
        arena: &NodeArena,
        recording_offset: [f32; 2],
    ) -> bool {
        text_area
            .exact_retained_property_scroll_atomic_projection_selection_subtree(
                self.text_area_root,
                arena,
                recording_offset,
            )
            .is_some_and(|grammar| {
                grammar == self.paint_grammar
                    && grammar.artifact_source(self.text_area_root).as_ref()
                        == Some(&self.artifact_source)
                    && grammar.artifact_selection_source() == Some(self.selection_source)
                    && grammar.artifact_space_transition().is_some_and(|expected| {
                        self.artifact_space_transition
                            .validate_expected_for_owner(self.text_area_root, expected)
                            .is_ok()
                    })
            })
    }
}

/// Exact focused-glyph sibling admission for one realized atomic projection.
/// Caret and preedit sources remain post-composite facts and are excluded from
/// the resident raster identity.
#[derive(Clone, Debug)]
pub(crate) struct RetainedScrollFocusedAtomicProjectionTextAreaSubtreeAdmissionSnapshot {
    pub(crate) boundary_root: NodeKey,
    pub(crate) stable_id: u64,
    pub(crate) content_wrapper: NodeKey,
    pub(crate) content_wrapper_stable_id: u64,
    pub(crate) text_area_root: NodeKey,
    pub(crate) text_area_stable_id: u64,
    paint_grammar: RetainedFocusedAtomicProjectionTextAreaPaintGrammar,
    pub(crate) artifact_source: PaintAtomicProjectionArtifactSource,
    pub(crate) artifact_space_transition: PaintArtifactSpaceTransition,
    pub(crate) source_bounds: RetainedSurfaceBounds,
    pub(crate) scroll: ScrollGeometrySnapshot,
}

impl RetainedScrollFocusedAtomicProjectionTextAreaSubtreeAdmissionSnapshot {
    pub(crate) fn bitwise_eq(&self, other: &Self) -> bool {
        self.boundary_root == other.boundary_root
            && self.stable_id == other.stable_id
            && self.content_wrapper == other.content_wrapper
            && self.content_wrapper_stable_id == other.content_wrapper_stable_id
            && self.text_area_root == other.text_area_root
            && self.text_area_stable_id == other.text_area_stable_id
            && self.paint_grammar.is_canonical()
            && other.paint_grammar.is_canonical()
            && self.paint_grammar == other.paint_grammar
            && self.artifact_source == other.artifact_source
            && self
                .artifact_space_transition
                .source_bits_eq(other.artifact_space_transition)
            && self
                .artifact_space_transition
                .semantic_revision_eq(other.artifact_space_transition)
            && scroll_geometry_snapshots_bitwise_equal(self.scroll, other.scroll)
            && composite_bounds_bitwise_equal(self.source_bounds, other.source_bounds)
    }

    pub(crate) fn matches_scroll_node(
        &self,
        snapshot: crate::view::compositor::property_tree::ScrollNodeSnapshot,
    ) -> bool {
        scroll_geometry_snapshot_matches_scroll_node(self.scroll, snapshot)
    }
    pub(crate) fn matches_live_source(
        &self,
        text_area: &TextArea,
        arena: &NodeArena,
        recording_offset: [f32; 2],
    ) -> bool {
        text_area
            .exact_retained_property_scroll_focused_atomic_projection_glyph_subtree(
                self.text_area_root,
                arena,
                recording_offset,
            )
            .is_some_and(|grammar| {
                grammar == self.paint_grammar
                    && grammar.artifact_source(self.text_area_root).as_ref()
                        == Some(&self.artifact_source)
                    && grammar.artifact_space_transition().is_some_and(|expected| {
                        self.artifact_space_transition
                            .validate_expected_for_owner(self.text_area_root, expected)
                            .is_ok()
                    })
            })
    }

    /// Read-only grammar access for the frozen-replay tests. Production reads
    /// the generic source, transition and selection facts instead.
    #[cfg(test)]
    pub(crate) fn paint_grammar_for_test(&self) -> &RetainedFocusedAtomicProjectionTextAreaPaintGrammar {
        &self.paint_grammar
    }

    pub(crate) fn caret_source(
        &self,
    ) -> &FocusedAtomicCaretSourceSeal {
        &self.paint_grammar.caret
    }

    pub(crate) fn preedit_source(
        &self,
    ) -> Option<&FocusedAtomicPreeditSourceSeal> {
        self.paint_grammar.preedit.as_ref()
    }
}

/// Exact sibling admission for focused plain TextArea retention. Its resident
/// base grammar excludes caret paint; the dynamic caret overlay is sealed by
/// the recorder/compiler chain.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RetainedScrollInteractiveTextAreaSubtreeAdmissionSnapshot {
    pub(crate) boundary_root: NodeKey,
    pub(crate) stable_id: u64,
    pub(crate) content_wrapper: NodeKey,
    pub(crate) content_wrapper_stable_id: u64,
    pub(crate) text_area_root: NodeKey,
    pub(crate) text_area_stable_id: u64,
    pub(crate) paint_source: PaintTextContentSource,
    /// Independent source-oracle geometry. `None` is the exact hidden-caret
    /// result; `Some` is the caret-map-derived live bounds before clipping.
    pub(crate) caret_oracle_bounds_bits: Option<[u32; 4]>,
    pub(crate) source_bounds: RetainedSurfaceBounds,
    pub(crate) scroll: ScrollGeometrySnapshot,
}

impl RetainedScrollInteractiveTextAreaSubtreeAdmissionSnapshot {
    pub(crate) fn bitwise_eq(self, other: Self) -> bool {
        self.boundary_root == other.boundary_root
            && self.stable_id == other.stable_id
            && self.content_wrapper == other.content_wrapper
            && self.content_wrapper_stable_id == other.content_wrapper_stable_id
            && self.text_area_root == other.text_area_root
            && self.text_area_stable_id == other.text_area_stable_id
            && self.paint_source.is_canonical()
            && other.paint_source.is_canonical()
            && self.paint_source == other.paint_source
            && self.caret_oracle_bounds_bits == other.caret_oracle_bounds_bits
            && scroll_geometry_snapshots_bitwise_equal(self.scroll, other.scroll)
            && composite_bounds_bitwise_equal(self.source_bounds, other.source_bounds)
    }

    pub(crate) fn matches_scroll_node(
        self,
        snapshot: crate::view::compositor::property_tree::ScrollNodeSnapshot,
    ) -> bool {
        scroll_geometry_snapshot_matches_scroll_node(self.scroll, snapshot)
    }
    pub(crate) fn matches_live_source(
        self,
        text_area: &TextArea,
        arena: &NodeArena,
        recording_offset: [f32; 2],
    ) -> bool {
        text_area
            .exact_retained_property_scroll_interactive_subtree(
                self.text_area_root,
                arena,
                recording_offset,
            )
            .and_then(|grammar| grammar.artifact_content_source())
            == Some(self.paint_source)
    }
}

/// Closed C1/C2a sibling of the direct-leaf scroll admission. Keeping this
/// separate makes the original B0 admission continue to prove that its
/// content child has no descendants.
pub(crate) fn exact_retained_scroll_text_area_subtree_admission(
    host: &Element,
    owner: NodeKey,
    arena: &NodeArena,
    scale_factor: f32,
) -> Option<RetainedScrollTextAreaSubtreeAdmissionSnapshot> {
    let (
        source_bounds,
        scroll,
        content_wrapper,
        content_wrapper_stable_id,
        text_area_root,
        wrapper_recording_offset,
        _live_wrapper_offset,
    ) = host
        .legacy_retained_scroll_single_child_content_shell(owner, arena, scale_factor)?
        .into_parts();
    let text_area_node = arena.get(text_area_root)?;
    let text_area = text_area_node
        .element
        .as_any()
        .downcast_ref::<TextArea>()?;
    let paint_grammar = if text_area.exact_retained_property_scroll_glyph_subtree(
        text_area_root,
        arena,
        wrapper_recording_offset,
    ) {
        RetainedTextAreaPaintGrammar::GlyphOnly
    } else {
        text_area.exact_retained_property_scroll_selection_glyph_subtree(
            text_area_root,
            arena,
            wrapper_recording_offset,
        )?
    };
    let paint_source = paint_grammar.artifact_content_source()?;
    Some(RetainedScrollTextAreaSubtreeAdmissionSnapshot {
        boundary_root: owner,
        stable_id: host.stable_id(),
        content_wrapper,
        content_wrapper_stable_id,
        text_area_root,
        text_area_stable_id: text_area.stable_id(),
        paint_source,
        source_bounds,
        scroll,
    })
}

/// Exact atomic-projection sibling admission. The component oracle stays
/// local to this selector; downstream recording receives generic artifact
/// facts from the resulting snapshot.
pub(crate) fn exact_retained_scroll_atomic_projection_text_area_subtree_admission(
    host: &Element,
    owner: NodeKey,
    arena: &NodeArena,
    scale_factor: f32,
) -> Option<RetainedScrollAtomicProjectionTextAreaSubtreeAdmissionSnapshot> {
    let (
        source_bounds,
        scroll,
        content_wrapper,
        content_wrapper_stable_id,
        text_area_root,
        wrapper_recording_offset,
        _live_wrapper_offset,
    ) = host
        .legacy_retained_scroll_single_child_content_shell(owner, arena, scale_factor)?
        .into_parts();
    let text_area_node = arena.get(text_area_root)?;
    let text_area = text_area_node
        .element
        .as_any()
        .downcast_ref::<TextArea>()?;
    let paint_grammar = text_area.exact_retained_property_scroll_atomic_projection_subtree(
        text_area_root,
        arena,
        wrapper_recording_offset,
    )?;
    let artifact_space_transition = paint_grammar.artifact_space_transition()?;
    let artifact_source = paint_grammar.artifact_source(text_area_root)?;
    Some(
        RetainedScrollAtomicProjectionTextAreaSubtreeAdmissionSnapshot {
            boundary_root: owner,
            stable_id: host.stable_id(),
            content_wrapper,
            content_wrapper_stable_id,
            text_area_root,
            text_area_stable_id: text_area.stable_id(),
            paint_grammar,
            artifact_source,
            artifact_space_transition,
            source_bounds,
            scroll,
        },
    )
}

/// Exact root-owned selection plus one realized atomic projection. The
/// component oracle is rerun by recorders around artifact production.
pub(crate) fn exact_retained_scroll_atomic_projection_selection_text_area_subtree_admission(
    host: &Element,
    owner: NodeKey,
    arena: &NodeArena,
    scale_factor: f32,
) -> Option<RetainedScrollAtomicProjectionSelectionTextAreaSubtreeAdmissionSnapshot> {
    let (
        source_bounds,
        scroll,
        content_wrapper,
        content_wrapper_stable_id,
        text_area_root,
        wrapper_recording_offset,
        _live_wrapper_offset,
    ) = host
        .legacy_retained_scroll_single_child_content_shell(owner, arena, scale_factor)?
        .into_parts();
    let text_area_node = arena.get(text_area_root)?;
    let text_area = text_area_node
        .element
        .as_any()
        .downcast_ref::<TextArea>()?;
    let paint_grammar = text_area
        .exact_retained_property_scroll_atomic_projection_selection_subtree(
            text_area_root,
            arena,
            wrapper_recording_offset,
        )?;
    let artifact_space_transition = paint_grammar.artifact_space_transition()?;
    let artifact_source = paint_grammar.artifact_source(text_area_root)?;
    let selection_source = paint_grammar.artifact_selection_source()?;
    Some(
        RetainedScrollAtomicProjectionSelectionTextAreaSubtreeAdmissionSnapshot {
            boundary_root: owner,
            stable_id: host.stable_id(),
            content_wrapper,
            content_wrapper_stable_id,
            text_area_root,
            text_area_stable_id: text_area.stable_id(),
            paint_grammar,
            artifact_source,
            selection_source,
            artifact_space_transition,
            source_bounds,
            scroll,
        },
    )
}

/// Exact focused-glyph sibling for one atomic projection. Resident and
/// post-composite source facts are frozen independently.
pub(crate) fn exact_retained_scroll_focused_atomic_projection_text_area_subtree_admission(
    host: &Element,
    owner: NodeKey,
    arena: &NodeArena,
    scale_factor: f32,
) -> Option<RetainedScrollFocusedAtomicProjectionTextAreaSubtreeAdmissionSnapshot> {
    let (
        source_bounds,
        scroll,
        content_wrapper,
        content_wrapper_stable_id,
        text_area_root,
        wrapper_recording_offset,
        _live_wrapper_offset,
    ) = host
        .legacy_retained_scroll_single_child_content_shell(owner, arena, scale_factor)?
        .into_parts();
    let text_area_node = arena.get(text_area_root)?;
    let text_area = text_area_node
        .element
        .as_any()
        .downcast_ref::<TextArea>()?;
    let paint_grammar = text_area
        .exact_retained_property_scroll_focused_atomic_projection_glyph_subtree(
            text_area_root,
            arena,
            wrapper_recording_offset,
        )?;
    let artifact_space_transition = paint_grammar.artifact_space_transition()?;
    let artifact_source = paint_grammar.artifact_source(text_area_root)?;
    Some(
        RetainedScrollFocusedAtomicProjectionTextAreaSubtreeAdmissionSnapshot {
            boundary_root: owner,
            stable_id: host.stable_id(),
            content_wrapper,
            content_wrapper_stable_id,
            text_area_root,
            text_area_stable_id: text_area.stable_id(),
            paint_grammar,
            artifact_source,
            artifact_space_transition,
            source_bounds,
            scroll,
        },
    )
}

pub(crate) fn exact_retained_scroll_interactive_text_area_subtree_admission(
    host: &Element,
    owner: NodeKey,
    arena: &NodeArena,
    scale_factor: f32,
) -> Option<RetainedScrollInteractiveTextAreaSubtreeAdmissionSnapshot> {
    let (
        source_bounds,
        scroll,
        content_wrapper,
        content_wrapper_stable_id,
        text_area_root,
        wrapper_recording_offset,
        live_wrapper_offset,
    ) = host
        .legacy_retained_scroll_single_child_content_shell(owner, arena, scale_factor)?
        .into_parts();
    let text_area_node = arena.get(text_area_root)?;
    let text_area = text_area_node
        .element
        .as_any()
        .downcast_ref::<TextArea>()?;
    let paint_grammar = text_area.exact_retained_property_scroll_interactive_subtree(
        text_area_root,
        arena,
        wrapper_recording_offset,
    )?;
    let paint_source = paint_grammar.artifact_content_source()?;
    let caret_oracle_bounds_bits = text_area.retained_interactive_caret_oracle_bounds_bits(
        text_area_root,
        arena,
        wrapper_recording_offset,
        live_wrapper_offset,
        paint_grammar,
    )?;
    Some(RetainedScrollInteractiveTextAreaSubtreeAdmissionSnapshot {
        boundary_root: owner,
        stable_id: host.stable_id(),
        content_wrapper,
        content_wrapper_stable_id,
        text_area_root,
        text_area_stable_id: text_area.stable_id(),
        paint_source,
        caret_oracle_bounds_bits,
        source_bounds,
        scroll,
    })
}

/// Generic paint source for the one frame-root scroll TextArea subtree.
///
/// The frame-root planner needs the content source and nothing else, so the
/// exact grammar qualification stays here and only `PaintTextContentSource`
/// crosses back out.
pub(crate) fn exact_retained_property_scroll_text_area_paint_source(
    text_area: &TextArea,
    text_area_root: NodeKey,
    arena: &NodeArena,
    recording_offset: [f32; 2],
) -> Option<PaintTextContentSource> {
    let grammar = if text_area.exact_retained_property_scroll_glyph_subtree(
        text_area_root,
        arena,
        recording_offset,
    ) {
        RetainedTextAreaPaintGrammar::GlyphOnly
    } else {
        text_area.exact_retained_property_scroll_selection_glyph_subtree(
            text_area_root,
            arena,
            recording_offset,
        )?
    };
    grammar.artifact_content_source()
}
