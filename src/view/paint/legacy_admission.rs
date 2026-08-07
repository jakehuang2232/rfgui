//! Legacy retained admission types.
//!
//! Every item in this module serves only the pre-V2 exact-shape retained
//! middle layer. It carries no durable paint semantics: the generic artifact,
//! payload identity, composite edges, and artifact-space transition already
//! express everything the V2 layerizer consumes.
//!
//! This whole file is deleted in the Stage C hard-cutover change set that
//! removes the old retained middle layer. Do not add new items here, and do
//! not let anything outside the legacy admission/planner path depend on it.

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PaintChunkRasterIdentity {
    pub(crate) id: PaintChunkId,
    pub(crate) owner: NodeKey,
    pub(crate) bounds_bits: [u32; 4],
    pub(crate) payload_identity: PaintPayloadIdentity,
}
