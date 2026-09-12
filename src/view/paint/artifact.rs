#![allow(dead_code)] // Staged artifact authority is exercised by tests until viewport rollout.

use std::ops::Range;
use std::sync::Arc;

use rustc_hash::{FxHashMap, FxHashSet};
use slotmap::Key;

use crate::view::ImageSampling;
use crate::view::base_component::{Rect, ScrollbarOverlayWitness, ScrollbarPaintStateWitness};
use crate::view::compositor::property_tree::PropertyTreeState;
use crate::view::compositor::property_tree::{
    ClipBehavior, ClipNodeId, ClipNodeRole, ClipNodeSnapshot, EffectNodeId, EffectNodeSnapshot,
    LayoutPositionNodeSnapshot, ScrollNodeId, ScrollNodeSnapshot, TransformNodeId,
    TransformNodeSnapshot, VisualOffsetNodeSnapshot,
};
use crate::view::node_arena::NodeKey;
use crate::view::render_pass::draw_rect_pass::{
    GradientKindGpu, GradientPaint, RectPassParams, RectRenderMode,
};
use crate::view::render_pass::shadow_module::{ShadowMesh, ShadowParams};
use crate::view::render_pass::text_pass::{
    TextPassPreparedFragment, TextPassPreparedParams, TextPassPreparedStagingGlyphInput,
};
use crate::view::render_pass::texture_composite_pass::TextureCompositeParams;
use crate::view::sampled_texture::{
    SampledTextureAlphaMode, SampledTextureId, SampledTextureUpload, SvgRasterAssetId,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PaintDeferredViewportSelfClipWitness {
    target_owner: NodeKey,
    stable_id: u64,
    clip: ClipNodeSnapshot,
}

/// Late-phase-only authority for one deferred viewport root whose opacity is
/// owned by a retained effect surface. It binds the exact Replace self clip
/// and isolated effect snapshot so neither can be replayed independently in
/// the normal phase.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PaintDeferredViewportEffectWitness {
    clip: PaintDeferredViewportSelfClipWitness,
    effect: EffectNodeSnapshot,
}

impl PaintDeferredViewportEffectWitness {
    pub(crate) fn new(
        clip: PaintDeferredViewportSelfClipWitness,
        effect: EffectNodeSnapshot,
    ) -> Option<Self> {
        (effect.id.0 == clip.target_owner
            && effect.owner == clip.target_owner
            && effect.parent.is_none()
            && effect.generation != 0
            && effect.opacity.is_finite()
            && (0.0..=1.0).contains(&effect.opacity))
        .then_some(Self { clip, effect })
    }

    pub(super) fn is_canonical_for(
        self,
        owner: NodeKey,
        stable_id: u64,
        authoritative_self_clip: Option<ClipNodeId>,
        effect: EffectNodeId,
    ) -> bool {
        self.effect.id == effect
            && self.effect.owner == owner
            && self.effect.parent.is_none()
            && self.effect.generation != 0
            && self.effect.opacity.is_finite()
            && (0.0..=1.0).contains(&self.effect.opacity)
            && self
                .clip
                .is_canonical_for(owner, stable_id, authoritative_self_clip)
    }
}

impl PaintDeferredViewportSelfClipWitness {
    pub(crate) fn new(
        target_owner: NodeKey,
        stable_id: u64,
        clip: ClipNodeSnapshot,
        logical_scissor: [u32; 4],
    ) -> Option<Self> {
        (stable_id != 0
            && clip.id.owner == target_owner
            && clip.id.role == ClipNodeRole::SelfClip
            && clip.owner == target_owner
            && clip.parent.is_none()
            && clip.logical_scissor == logical_scissor
            && clip.behavior == ClipBehavior::Replace
            && clip.generation != 0)
            .then_some(Self {
                target_owner,
                stable_id,
                clip,
            })
    }

    pub(super) fn is_canonical_for(
        self,
        owner: NodeKey,
        stable_id: u64,
        authoritative_self_clip: Option<ClipNodeId>,
    ) -> bool {
        self.target_owner == owner
            && self.stable_id == stable_id
            && authoritative_self_clip == Some(self.clip.id)
            && self.clip.id.owner == owner
            && self.clip.owner == owner
            && self.clip.id.role == ClipNodeRole::SelfClip
            && self.clip.parent.is_none()
            && self.clip.behavior == ClipBehavior::Replace
            && self.clip.generation != 0
    }
}

/// Typed raster-local generation for a detached contents clip. Live property
/// generations include viewport-space ancestry and therefore cannot
/// participate in a reusable offset-zero content identity.
pub(crate) const DETACHED_LOCAL_CLIP_GENERATION: u64 = 1;

/// One frozen property boundary in root-to-surface order. The snapshot's
/// parent id must name the nearest earlier entry with the same role.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum PropertyForestBoundarySnapshot {
    Transform(TransformNodeSnapshot),
    Effect(EffectNodeSnapshot),
}

impl PropertyForestBoundarySnapshot {
    fn owner(self) -> NodeKey {
        match self {
            Self::Transform(snapshot) => snapshot.owner,
            Self::Effect(snapshot) => snapshot.owner,
        }
    }
}

/// Complete root-to-surface T/E order used by the no-scroll property forest
/// recorder. This owns the entire chain; the Copy token installed in
/// `PaintRecordingContext` can only be minted after the frozen order,
/// same-role parent links and canonical same-owner T -> E edge are rechecked.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ConsumedPropertyForestAncestorChainWitness {
    boundary_owner: NodeKey,
    entries: Vec<PropertyForestBoundarySnapshot>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PropertyForestProjectionToken {
    target_owner: NodeKey,
    expected_transform: Option<TransformNodeId>,
    projected_transform: Option<TransformNodeId>,
    expected_effect: Option<EffectNodeId>,
    projected_effect: Option<EffectNodeId>,
    neutral_effect: Option<EffectNodeId>,
}

impl ConsumedPropertyForestAncestorChainWitness {
    pub(crate) fn new(
        boundary_owner: NodeKey,
        entries: Vec<PropertyForestBoundarySnapshot>,
    ) -> Option<Self> {
        let witness = Self {
            boundary_owner,
            entries,
        };
        witness.is_canonical().then_some(witness)
    }

    fn canonical_leaves(
        &self,
    ) -> Option<(
        Option<TransformNodeId>,
        Option<EffectNodeId>,
        PropertyForestBoundarySnapshot,
    )> {
        if self.boundary_owner.is_null() || self.entries.is_empty() {
            return None;
        }
        let mut latest_transform = None;
        let mut latest_effect = None;
        let mut previous = None;
        for entry in self.entries.iter().copied() {
            if let Some(previous) = previous {
                let alternates = matches!(
                    (previous, entry),
                    (
                        PropertyForestBoundarySnapshot::Transform(_),
                        PropertyForestBoundarySnapshot::Effect(_),
                    ) | (
                        PropertyForestBoundarySnapshot::Effect(_),
                        PropertyForestBoundarySnapshot::Transform(_),
                    )
                );
                let same_owner_is_canonical = previous.owner() != entry.owner()
                    || matches!(
                        (previous, entry),
                        (
                            PropertyForestBoundarySnapshot::Transform(_),
                            PropertyForestBoundarySnapshot::Effect(_),
                        )
                    );
                if !alternates || !same_owner_is_canonical {
                    return None;
                }
            }
            match entry {
                PropertyForestBoundarySnapshot::Transform(snapshot) => {
                    if snapshot.id.0 != snapshot.owner
                        || snapshot.owner.is_null()
                        || snapshot.generation == 0
                        || snapshot.parent != latest_transform
                        || snapshot
                            .owner_viewport_transform
                            .to_cols_array()
                            .into_iter()
                            .any(|value| !value.is_finite())
                    {
                        return None;
                    }
                    latest_transform = Some(snapshot.id);
                }
                PropertyForestBoundarySnapshot::Effect(snapshot) => {
                    if snapshot.id.0 != snapshot.owner
                        || snapshot.owner.is_null()
                        || snapshot.generation == 0
                        || snapshot.parent != latest_effect
                        || !snapshot.opacity.is_finite()
                        || !(0.0..=1.0).contains(&snapshot.opacity)
                    {
                        return None;
                    }
                    latest_effect = Some(snapshot.id);
                }
            }
            previous = Some(entry);
        }
        let leaf = self.entries.last().copied()?;
        (leaf.owner() == self.boundary_owner).then_some((latest_transform, latest_effect, leaf))
    }

    pub(crate) fn is_canonical(&self) -> bool {
        self.canonical_leaves().is_some()
    }

    pub(crate) fn projection_for_target(
        &self,
        target_owner: NodeKey,
    ) -> Option<PropertyForestProjectionToken> {
        if target_owner.is_null() {
            return None;
        }
        let (transform, effect, leaf) = self.canonical_leaves()?;
        Some(match leaf {
            PropertyForestBoundarySnapshot::Transform(_) => PropertyForestProjectionToken {
                target_owner,
                expected_transform: transform,
                projected_transform: transform,
                expected_effect: effect,
                projected_effect: None,
                neutral_effect: effect,
            },
            PropertyForestBoundarySnapshot::Effect(_) => PropertyForestProjectionToken {
                target_owner,
                expected_transform: transform,
                projected_transform: None,
                expected_effect: effect,
                projected_effect: effect,
                neutral_effect: None,
            },
        })
    }
}

impl PropertyForestProjectionToken {
    pub(super) fn project_for(
        self,
        owner: NodeKey,
        live: PropertyTreeState,
        opacity_authority: PaintOpacityAuthority,
    ) -> Option<PropertyTreeState> {
        if self.target_owner != owner
            || live.transform != self.expected_transform
            || live.effect != self.expected_effect
            || self.neutral_effect.is_some_and(|effect| {
                opacity_authority != PaintOpacityAuthority::NeutralRootEffect(effect)
            })
        {
            return None;
        }
        Some(PropertyTreeState {
            transform: self.projected_transform,
            effect: self.projected_effect,
            ..live
        })
    }
}

const MAX_CONSUMED_ANCESTOR_PROPERTIES: usize = 3;

/// Exact bounded projection stack used by the property/scroll receiver
/// recorder.  Entries are applied outer-to-inner and rebound to each current
/// traversal owner by the coverage walker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ConsumedAncestorPropertyStackWitness {
    entries: [Option<ConsumedAncestorProperty>; MAX_CONSUMED_ANCESTOR_PROPERTIES],
    len: u8,
    target_owner: NodeKey,
}

impl ConsumedAncestorPropertyStackWitness {
    pub(crate) fn new(target_owner: NodeKey, entries: &[ConsumedAncestorProperty]) -> Option<Self> {
        if target_owner.is_null()
            || entries.is_empty()
            || entries.len() > MAX_CONSUMED_ANCESTOR_PROPERTIES
        {
            return None;
        }
        let mut transform_seen = false;
        let mut effect_seen = false;
        let mut scroll_seen = false;
        for (index, entry) in entries.iter().enumerate() {
            match entry {
                ConsumedAncestorProperty::Transform(witness) => {
                    if scroll_seen {
                        return None;
                    }
                    if !witness
                        .for_target(target_owner)
                        .is_canonical_for(target_owner)
                    {
                        return None;
                    }
                    if std::mem::replace(&mut transform_seen, true) {
                        return None;
                    }
                }
                ConsumedAncestorProperty::SameOwnerTransformBoundary(_) => return None,
                ConsumedAncestorProperty::SameOwnerEffectBoundary(_) => return None,
                ConsumedAncestorProperty::ScrollContents(witness) => {
                    if !witness
                        .for_target(target_owner)
                        .is_canonical_for(target_owner)
                    {
                        return None;
                    }
                    if std::mem::replace(&mut scroll_seen, true) {
                        return None;
                    }
                    if index + 1 != entries.len() {
                        return None;
                    }
                }
                ConsumedAncestorProperty::Effect(witness) => {
                    if scroll_seen {
                        return None;
                    }
                    if !witness
                        .for_target(target_owner)
                        .is_canonical_for(target_owner)
                    {
                        return None;
                    }
                    if std::mem::replace(&mut effect_seen, true) {
                        return None;
                    }
                }
            }
        }
        let mut sealed = [None; MAX_CONSUMED_ANCESTOR_PROPERTIES];
        for (slot, entry) in sealed.iter_mut().zip(entries.iter().copied()) {
            *slot = Some(entry.for_target(target_owner));
        }
        Some(Self {
            entries: sealed,
            len: entries.len() as u8,
            target_owner,
        })
    }

    /// Seals the only equal-owner stack admitted by the native property DAG:
    /// the owner's outer transform followed by its scroll/contents-clip pair
    /// projected onto the direct content child. The generic constructor keeps
    /// rejecting equal-owner transform entries.
    pub(crate) fn new_same_owner_transform_scroll(
        target_owner: NodeKey,
        transform: ConsumedSameOwnerTransformBoundaryWitness,
        scroll: ConsumedAncestorScrollContentsWitness,
    ) -> Option<Self> {
        if target_owner.is_null()
            || transform.owner != scroll.parent_boundary
            || scroll.child_boundary != target_owner
            || !transform
                .for_target(target_owner)
                .is_canonical_for(target_owner)
            || !scroll
                .for_target(target_owner)
                .is_canonical_for(target_owner)
        {
            return None;
        }
        Some(Self {
            entries: [
                Some(ConsumedAncestorProperty::SameOwnerTransformBoundary(
                    transform.for_target(target_owner),
                )),
                Some(ConsumedAncestorProperty::ScrollContents(
                    scroll.for_target(target_owner),
                )),
                None,
            ],
            len: 2,
            target_owner,
        })
    }

    /// Seals the equal-owner `Effect -> ScrollContents` projection used by
    /// the native E+S compiler. The generic constructor keeps rejecting this
    /// shape so only the typed same-owner planner can neutralize the effect
    /// before recording H/C/O.
    pub(crate) fn new_same_owner_effect_scroll(
        target_owner: NodeKey,
        effect: ConsumedSameOwnerEffectBoundaryWitness,
        scroll: ConsumedAncestorScrollContentsWitness,
    ) -> Option<Self> {
        if target_owner.is_null()
            || effect.owner != scroll.parent_boundary
            || scroll.child_boundary != target_owner
            || !effect
                .for_target(target_owner)
                .is_canonical_for(target_owner)
            || !scroll
                .for_target(target_owner)
                .is_canonical_for(target_owner)
        {
            return None;
        }
        Some(Self {
            entries: [
                Some(ConsumedAncestorProperty::SameOwnerEffectBoundary(
                    effect.for_target(target_owner),
                )),
                Some(ConsumedAncestorProperty::ScrollContents(
                    scroll.for_target(target_owner),
                )),
                None,
            ],
            len: 2,
            target_owner,
        })
    }

    /// Seals the same-owner T+E projection used while recording H/O. Scroll
    /// remains live on the direct content child so `BakedScrollHost` can keep
    /// its established phase split.
    pub(crate) fn new_same_owner_transform_effect_host(
        target_owner: NodeKey,
        transform: ConsumedSameOwnerTransformBoundaryWitness,
        effect: ConsumedSameOwnerEffectBoundaryWitness,
    ) -> Option<Self> {
        if target_owner.is_null()
            || transform.owner != effect.owner
            || transform.owner != target_owner
            || !transform
                .for_target(target_owner)
                .is_canonical_for(target_owner)
            || !effect
                .for_target(target_owner)
                .is_canonical_for(target_owner)
        {
            return None;
        }
        Some(Self {
            entries: [
                Some(ConsumedAncestorProperty::SameOwnerTransformBoundary(
                    transform.for_target(target_owner),
                )),
                Some(ConsumedAncestorProperty::SameOwnerEffectBoundary(
                    effect.for_target(target_owner),
                )),
                None,
            ],
            len: 2,
            target_owner,
        })
    }

    /// Seals the complete same-owner T -> E -> ScrollContents projection onto
    /// the direct detached content child. Generic stacks continue to reject
    /// all equal-owner boundary witnesses.
    pub(crate) fn new_same_owner_transform_effect_scroll(
        target_owner: NodeKey,
        transform: ConsumedSameOwnerTransformBoundaryWitness,
        effect: ConsumedSameOwnerEffectBoundaryWitness,
        scroll: ConsumedAncestorScrollContentsWitness,
    ) -> Option<Self> {
        if target_owner.is_null()
            || transform.owner != effect.owner
            || transform.owner != scroll.parent_boundary
            || scroll.child_boundary != target_owner
            || !transform
                .for_target(target_owner)
                .is_canonical_for(target_owner)
            || !effect
                .for_target(target_owner)
                .is_canonical_for(target_owner)
            || !scroll
                .for_target(target_owner)
                .is_canonical_for(target_owner)
        {
            return None;
        }
        Some(Self {
            entries: [
                Some(ConsumedAncestorProperty::SameOwnerTransformBoundary(
                    transform.for_target(target_owner),
                )),
                Some(ConsumedAncestorProperty::SameOwnerEffectBoundary(
                    effect.for_target(target_owner),
                )),
                Some(ConsumedAncestorProperty::ScrollContents(
                    scroll.for_target(target_owner),
                )),
            ],
            len: 3,
            target_owner,
        })
    }

    pub(crate) fn for_target(self, target_owner: NodeKey) -> Self {
        let mut entries = self.entries;
        for entry in entries.iter_mut().take(self.len as usize) {
            *entry = entry.map(|entry| entry.for_target(target_owner));
        }
        Self {
            entries,
            target_owner,
            ..self
        }
    }

    pub(crate) fn entries(self) -> impl Iterator<Item = ConsumedAncestorProperty> {
        self.entries.into_iter().take(self.len as usize).flatten()
    }

    pub(super) fn authorizes_scroll_content_local_owner(
        self,
        owner: NodeKey,
        opacity_authority: PaintOpacityAuthority,
    ) -> bool {
        if owner != self.target_owner
            || self.len == 0
            || usize::from(self.len) > MAX_CONSUMED_ANCESTOR_PROPERTIES
            || self.entries[..usize::from(self.len)]
                .iter()
                .any(Option::is_none)
            || self.entries[usize::from(self.len)..]
                .iter()
                .any(Option::is_some)
        {
            return false;
        }
        let mut transform_seen = false;
        let mut effect_seen = false;
        let mut scroll_witness = None;
        for (index, entry) in self.entries().enumerate() {
            match entry {
                ConsumedAncestorProperty::Transform(witness) => {
                    if scroll_witness.is_some()
                        || std::mem::replace(&mut transform_seen, true)
                        || !witness.is_canonical_for(owner)
                    {
                        return false;
                    }
                }
                ConsumedAncestorProperty::SameOwnerTransformBoundary(witness) => {
                    if scroll_witness.is_some()
                        || std::mem::replace(&mut transform_seen, true)
                        || !witness.is_canonical_for(owner)
                    {
                        return false;
                    }
                }
                ConsumedAncestorProperty::SameOwnerEffectBoundary(witness) => {
                    if scroll_witness.is_some()
                        || std::mem::replace(&mut effect_seen, true)
                        || !witness.is_canonical_for(owner)
                        || opacity_authority
                            != PaintOpacityAuthority::NeutralRootEffect(witness.effect.id)
                    {
                        return false;
                    }
                }
                ConsumedAncestorProperty::ScrollContents(witness) => {
                    if !witness.is_canonical_for(owner) || scroll_witness.replace(witness).is_some()
                    {
                        return false;
                    }
                    if index + 1 != usize::from(self.len) {
                        return false;
                    }
                }
                ConsumedAncestorProperty::Effect(witness) => {
                    if scroll_witness.is_some()
                        || std::mem::replace(&mut effect_seen, true)
                        || !witness.is_canonical_for(owner)
                        || opacity_authority
                            != PaintOpacityAuthority::NeutralRootEffect(witness.effect.id)
                    {
                        return false;
                    }
                }
            }
        }
        scroll_witness.is_some()
    }

    pub(super) fn project_for(
        self,
        owner: NodeKey,
        mut live: PropertyTreeState,
        opacity_authority: PaintOpacityAuthority,
    ) -> Option<PropertyTreeState> {
        if owner != self.target_owner || self.len == 0 {
            return None;
        }
        for entry in self.entries() {
            live = match entry {
                ConsumedAncestorProperty::Transform(witness)
                    if witness.is_canonical_for(owner)
                        && live.transform == Some(witness.transform) =>
                {
                    PropertyTreeState {
                        transform: None,
                        ..live
                    }
                }
                ConsumedAncestorProperty::SameOwnerTransformBoundary(witness)
                    if witness.is_canonical_for(owner)
                        && live.transform == Some(witness.transform) =>
                {
                    PropertyTreeState {
                        transform: None,
                        ..live
                    }
                }
                ConsumedAncestorProperty::SameOwnerEffectBoundary(witness)
                    if witness.is_canonical_for(owner)
                        && opacity_authority
                            == PaintOpacityAuthority::NeutralRootEffect(witness.effect.id)
                        && live.effect == witness.expected_before =>
                {
                    PropertyTreeState {
                        effect: witness.projected_after,
                        ..live
                    }
                }
                ConsumedAncestorProperty::ScrollContents(witness)
                    if witness.is_canonical_for(owner)
                        && live.scroll == Some(witness.scroll)
                        && live.clip == Some(witness.contents_clip) =>
                {
                    PropertyTreeState {
                        clip: None,
                        scroll: None,
                        ..live
                    }
                }
                ConsumedAncestorProperty::Effect(witness)
                    if witness.is_canonical_for(owner)
                        && opacity_authority
                            == PaintOpacityAuthority::NeutralRootEffect(witness.effect.id)
                        && live.effect == witness.expected_before =>
                {
                    PropertyTreeState {
                        effect: witness.projected_after,
                        ..live
                    }
                }
                _ => return None,
            };
        }
        Some(live)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ConsumedAncestorProperty {
    Transform(ConsumedAncestorTransformWitness),
    SameOwnerTransformBoundary(ConsumedSameOwnerTransformBoundaryWitness),
    SameOwnerEffectBoundary(ConsumedSameOwnerEffectBoundaryWitness),
    Effect(ConsumedAncestorEffectWitness),
    /// One scroll projection and its owning contents clip are consumed as a
    /// single boundary. Projecting only one half would either double-translate
    /// content or retain a viewport-space clip in the offset-zero raster.
    ScrollContents(ConsumedAncestorScrollContentsWitness),
}

impl ConsumedAncestorProperty {
    pub(crate) fn for_target(self, target_owner: NodeKey) -> Self {
        match self {
            Self::Transform(witness) => Self::Transform(witness.for_target(target_owner)),
            Self::SameOwnerTransformBoundary(witness) => {
                Self::SameOwnerTransformBoundary(witness.for_target(target_owner))
            }
            Self::SameOwnerEffectBoundary(witness) => {
                Self::SameOwnerEffectBoundary(witness.for_target(target_owner))
            }
            Self::Effect(witness) => Self::Effect(witness.for_target(target_owner)),
            Self::ScrollContents(witness) => Self::ScrollContents(witness.for_target(target_owner)),
        }
    }
}

/// Exact projection for an effect boundary and scroll host owned by the same
/// native node. This capability is intentionally separate from
/// `ConsumedAncestorEffectWitness`, whose unequal-owner invariant remains
/// unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ConsumedSameOwnerEffectBoundaryWitness {
    pub(crate) owner: NodeKey,
    pub(crate) effect: EffectNodeSnapshot,
    pub(crate) expected_before: Option<EffectNodeId>,
    pub(crate) projected_after: Option<EffectNodeId>,
    pub(crate) target_owner: NodeKey,
}

impl ConsumedSameOwnerEffectBoundaryWitness {
    pub(crate) fn new(owner: NodeKey, effect: EffectNodeSnapshot) -> Option<Self> {
        let witness = Self {
            owner,
            effect,
            expected_before: Some(effect.id),
            projected_after: effect.parent,
            target_owner: owner,
        };
        witness.is_canonical_for(owner).then_some(witness)
    }

    pub(crate) fn for_target(self, target_owner: NodeKey) -> Self {
        Self {
            target_owner,
            ..self
        }
    }

    pub(super) fn is_canonical_for(self, owner: NodeKey) -> bool {
        self.effect.id.0 == self.owner
            && self.effect.owner == self.owner
            && self.effect.parent.is_none()
            && self.effect.generation != 0
            && self.effect.opacity.is_finite()
            && (0.0..=1.0).contains(&self.effect.opacity)
            && self.expected_before == Some(self.effect.id)
            && self.projected_after == self.effect.parent
            && self.target_owner == owner
    }
}

/// Exact effect-chain projection owned by one already-neutralized receiver.
/// The before/after leaf ids are part of the capability, so removing an
/// arbitrary effect tag or skipping an ancestor cannot pass projection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ConsumedAncestorEffectWitness {
    pub(crate) parent_boundary: NodeKey,
    pub(crate) child_boundary: NodeKey,
    pub(crate) effect: EffectNodeSnapshot,
    pub(crate) expected_before: Option<EffectNodeId>,
    pub(crate) projected_after: Option<EffectNodeId>,
    pub(crate) target_owner: NodeKey,
}

impl ConsumedAncestorEffectWitness {
    pub(crate) fn new(
        parent_boundary: NodeKey,
        child_boundary: NodeKey,
        effect: EffectNodeSnapshot,
        expected_before: Option<EffectNodeId>,
        projected_after: Option<EffectNodeId>,
    ) -> Option<Self> {
        let witness = Self {
            parent_boundary,
            child_boundary,
            effect,
            expected_before,
            projected_after,
            target_owner: child_boundary,
        };
        witness.is_canonical_for(child_boundary).then_some(witness)
    }

    pub(crate) fn for_target(self, target_owner: NodeKey) -> Self {
        Self {
            target_owner,
            ..self
        }
    }

    pub(super) fn is_canonical_for(self, owner: NodeKey) -> bool {
        self.parent_boundary != self.child_boundary
            && self.effect.id.0 == self.parent_boundary
            && self.effect.owner == self.parent_boundary
            && self.effect.generation != 0
            && self.effect.opacity.is_finite()
            && (0.0..=1.0).contains(&self.effect.opacity)
            && self.expected_before == Some(self.effect.id)
            && self.projected_after == self.effect.parent
            && self.target_owner == owner
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ConsumedAncestorScrollContentsWitness {
    pub(crate) parent_boundary: NodeKey,
    pub(crate) child_boundary: NodeKey,
    pub(crate) scroll: ScrollNodeId,
    pub(crate) contents_clip: ClipNodeId,
    pub(crate) target_owner: NodeKey,
}

impl ConsumedAncestorScrollContentsWitness {
    pub(crate) fn new(
        parent_boundary: NodeKey,
        child_boundary: NodeKey,
        scroll: ScrollNodeId,
        contents_clip: ClipNodeId,
    ) -> Option<Self> {
        (parent_boundary != child_boundary
            && scroll.0 == parent_boundary
            && contents_clip.owner == parent_boundary
            && contents_clip.role == ClipNodeRole::ContentsClip)
            .then_some(Self {
                parent_boundary,
                child_boundary,
                scroll,
                contents_clip,
                target_owner: child_boundary,
            })
    }

    pub(crate) fn for_target(self, target_owner: NodeKey) -> Self {
        Self {
            target_owner,
            ..self
        }
    }

    pub(super) fn is_canonical_for(self, owner: NodeKey) -> bool {
        self.parent_boundary != self.child_boundary
            && self.scroll.0 == self.parent_boundary
            && self.contents_clip.owner == self.parent_boundary
            && self.contents_clip.role == ClipNodeRole::ContentsClip
            && self.target_owner == owner
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ConsumedAncestorTransformWitness {
    pub(crate) parent_boundary: NodeKey,
    pub(crate) child_boundary: NodeKey,
    pub(crate) transform: TransformNodeId,
    pub(crate) target_owner: NodeKey,
}

/// Exact projection for a transform boundary already owned by an outer
/// retained surface while an inner effect surface records the same node.
/// This is deliberately separate from `ConsumedAncestorTransformWitness`:
/// equal owners are canonical here and remain forbidden for the ancestor
/// capability.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ConsumedSameOwnerTransformBoundaryWitness {
    pub(crate) owner: NodeKey,
    pub(crate) transform: TransformNodeId,
    pub(crate) target_owner: NodeKey,
}

impl ConsumedSameOwnerTransformBoundaryWitness {
    pub(crate) fn new(owner: NodeKey, transform: TransformNodeId) -> Option<Self> {
        (transform.0 == owner).then_some(Self {
            owner,
            transform,
            target_owner: owner,
        })
    }

    pub(crate) fn for_target(self, target_owner: NodeKey) -> Self {
        Self {
            target_owner,
            ..self
        }
    }

    pub(super) fn is_canonical_for(self, owner: NodeKey) -> bool {
        self.owner == self.transform.0 && self.target_owner == owner
    }
}

impl ConsumedAncestorTransformWitness {
    pub(crate) fn new(
        parent_boundary: NodeKey,
        child_boundary: NodeKey,
        transform: TransformNodeId,
    ) -> Option<Self> {
        (parent_boundary != child_boundary && transform.0 == parent_boundary).then_some(Self {
            parent_boundary,
            child_boundary,
            transform,
            target_owner: child_boundary,
        })
    }

    pub(crate) fn for_target(self, target_owner: NodeKey) -> Self {
        Self {
            target_owner,
            ..self
        }
    }

    pub(super) fn is_canonical_for(self, owner: NodeKey) -> bool {
        self.parent_boundary != self.child_boundary
            && self.transform.0 == self.parent_boundary
            && self.target_owner == owner
    }
}

/// Recorder-owned identity for one detached, offset-zero scroll-content
/// artifact. Both recording passes receive this exact immutable value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PaintScrollContentWitness {
    boundary_root: NodeKey,
    content_root: NodeKey,
    scroll: ScrollNodeSnapshot,
    contents_clip: ClipNodeSnapshot,
    normalization_offset_bits: [u32; 2],
}

/// One boundary-local projection edge in an arbitrary-depth scroll forest.
/// The witness consumes exactly this scroll/contents-clip pair; ancestor
/// edges are represented by the parent target and never accumulated in the
/// bounded ancestor-property stack.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PaintScrollForestEdgeWitness {
    boundary_root: NodeKey,
    content_root: NodeKey,
    scroll: ScrollNodeSnapshot,
    contents_clip: ClipNodeSnapshot,
    normalization_offset_bits: [u32; 2],
}

impl PaintScrollForestEdgeWitness {
    pub(crate) fn new(
        boundary_root: NodeKey,
        content_root: NodeKey,
        scroll: ScrollNodeSnapshot,
        contents_clip: ClipNodeSnapshot,
        parent_scroll: Option<ScrollNodeId>,
        parent_clip: Option<ClipNodeId>,
    ) -> Option<Self> {
        let normalization_offset = [scroll.offset.x, scroll.offset.y];
        (boundary_root != content_root
            && scroll.id.0 == boundary_root
            && scroll.owner == boundary_root
            && scroll.parent == parent_scroll
            && scroll.generation != 0
            && normalization_offset.into_iter().all(f32::is_finite)
            && contents_clip.id.owner == boundary_root
            && contents_clip.id.role == ClipNodeRole::ContentsClip
            && contents_clip.owner == boundary_root
            && contents_clip.parent == parent_clip
            && contents_clip.generation != 0
            && scroll.has_canonical_geometry_with_contents_clip_parent_ids(
                contents_clip,
                parent_scroll,
                parent_clip,
            ))
        .then_some(Self {
            boundary_root,
            content_root,
            scroll,
            contents_clip,
            normalization_offset_bits: normalization_offset.map(f32::to_bits),
        })
    }

    pub(crate) fn boundary_root(self) -> NodeKey {
        self.boundary_root
    }

    pub(crate) fn content_root(self) -> NodeKey {
        self.content_root
    }

    pub(crate) fn scroll_snapshot(self) -> ScrollNodeSnapshot {
        self.scroll
    }

    pub(crate) fn contents_clip_snapshot(self) -> ClipNodeSnapshot {
        self.contents_clip
    }

    pub(crate) fn normalization_paint_offset(self) -> [f32; 2] {
        self.normalization_offset_bits.map(f32::from_bits)
    }

    pub(crate) fn consumed_property(self) -> ConsumedAncestorProperty {
        ConsumedAncestorProperty::ScrollContents(
            ConsumedAncestorScrollContentsWitness::new(
                self.boundary_root,
                self.content_root,
                self.scroll.id,
                self.contents_clip.id,
            )
            .expect("canonical forest edge owns one scroll/clip projection"),
        )
    }

    pub(crate) fn project_host_for(
        self,
        owner: NodeKey,
        live: PropertyTreeState,
    ) -> Option<PropertyTreeState> {
        if owner != self.boundary_root {
            return None;
        }
        let parent = PropertyTreeState {
            clip: self.contents_clip.parent,
            scroll: self.scroll.parent,
            ..PropertyTreeState::default()
        };
        let own = PropertyTreeState {
            clip: Some(self.contents_clip.id),
            scroll: Some(self.scroll.id),
            ..PropertyTreeState::default()
        };
        if live.legacy_boundary_eq(parent) {
            Some(PropertyTreeState {
                transform: None,
                clip: None,
                effect: None,
                scroll: None,
                ..live
            })
        } else if live.legacy_boundary_eq(own) {
            // Host masks are emitted on the parent target around the typed
            // content receiver. The receiver keeps the own S/C edge; storing
            // it again on the mask would double-apply scroll/clip state.
            Some(PropertyTreeState {
                transform: None,
                clip: None,
                effect: None,
                scroll: None,
                ..live
            })
        } else {
            None
        }
    }
}

impl PaintScrollContentWitness {
    pub(crate) fn new(
        boundary_root: NodeKey,
        content_root: NodeKey,
        scroll: ScrollNodeSnapshot,
        contents_clip: ClipNodeSnapshot,
    ) -> Option<Self> {
        let normalization_offset = [scroll.offset.x, scroll.offset.y];
        (boundary_root != content_root
            && scroll.id.0 == boundary_root
            && scroll.owner == boundary_root
            && scroll.parent.is_none()
            && scroll.generation != 0
            && normalization_offset.into_iter().all(f32::is_finite)
            && contents_clip.id.owner == boundary_root
            && contents_clip.id.role == ClipNodeRole::ContentsClip
            && contents_clip.owner == boundary_root
            && contents_clip.parent.is_none()
            && contents_clip.generation != 0
            && scroll.has_canonical_vertical_geometry_with_contents_clip(contents_clip))
        .then_some(Self {
            boundary_root,
            content_root,
            scroll,
            contents_clip,
            normalization_offset_bits: normalization_offset.map(f32::to_bits),
        })
    }

    pub(crate) fn boundary_root(self) -> NodeKey {
        self.boundary_root
    }

    pub(crate) fn content_root(self) -> NodeKey {
        self.content_root
    }

    pub(crate) fn scroll_snapshot(self) -> ScrollNodeSnapshot {
        self.scroll
    }

    pub(crate) fn contents_clip_snapshot(self) -> ClipNodeSnapshot {
        self.contents_clip
    }

    pub(crate) fn normalization_paint_offset(self) -> [f32; 2] {
        self.normalization_offset_bits.map(f32::from_bits)
    }

    pub(crate) fn consumed_property(self) -> ConsumedAncestorProperty {
        ConsumedAncestorProperty::ScrollContents(
            ConsumedAncestorScrollContentsWitness::new(
                self.boundary_root,
                self.content_root,
                self.scroll.id,
                self.contents_clip.id,
            )
            .expect("validated scroll-content witness has canonical property identities"),
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PaintTransformSurfaceWitness {
    pub(crate) boundary_owner: NodeKey,
    pub(crate) transform: TransformNodeId,
    pub(crate) target_owner: NodeKey,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PaintBakedScrollHostWitness {
    boundary_root: NodeKey,
    child: NodeKey,
    scroll: crate::view::compositor::property_tree::ScrollNodeSnapshot,
    contents_clip: ClipNodeId,
    target_owner: NodeKey,
}

impl PaintBakedScrollHostWitness {
    pub(crate) fn new(
        boundary_root: NodeKey,
        child: NodeKey,
        scroll: crate::view::compositor::property_tree::ScrollNodeSnapshot,
        contents_clip: ClipNodeId,
    ) -> Option<Self> {
        (boundary_root != child
            && scroll.id.0 == boundary_root
            && scroll.owner == boundary_root
            && contents_clip.owner == boundary_root
            && contents_clip.role == ClipNodeRole::ContentsClip)
            .then_some(Self {
                boundary_root,
                child,
                scroll,
                contents_clip,
                target_owner: boundary_root,
            })
    }

    pub(crate) fn for_target(self, target_owner: NodeKey) -> Self {
        Self {
            target_owner,
            ..self
        }
    }

    pub(crate) fn boundary_root(self) -> NodeKey {
        self.boundary_root
    }

    pub(crate) fn child(self) -> NodeKey {
        self.child
    }

    pub(crate) fn scroll(self) -> crate::view::compositor::property_tree::ScrollNodeId {
        self.scroll.id
    }

    pub(crate) fn scroll_snapshot(
        self,
    ) -> crate::view::compositor::property_tree::ScrollNodeSnapshot {
        self.scroll
    }

    pub(crate) fn contents_clip(self) -> ClipNodeId {
        self.contents_clip
    }

    pub(crate) fn target_owner(self) -> NodeKey {
        self.target_owner
    }
}

impl PaintTransformSurfaceWitness {
    pub(crate) fn canonical_root(root: NodeKey) -> Self {
        Self {
            boundary_owner: root,
            transform: TransformNodeId(root),
            target_owner: root,
        }
    }

    pub(crate) fn for_target(self, target_owner: NodeKey) -> Self {
        Self {
            target_owner,
            ..self
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PaintTextPreeditWitness {
    pub(crate) projection_owner: NodeKey,
    pub(crate) target_owner: NodeKey,
    pub(crate) target_stable_id: u64,
    pub(crate) local_start_char: usize,
    pub(crate) local_end_char: usize,
    pub(crate) target_start_byte: usize,
    pub(crate) target_end_byte: usize,
    pub(crate) target_caret_byte: usize,
    pub(crate) target_caret_char: usize,
}

impl PaintTextPreeditWitness {
    pub(crate) fn is_canonical_for(self, owner: NodeKey, stable_id: u64) -> bool {
        self.target_owner == owner
            && self.target_stable_id == stable_id
            && self.local_start_char < self.local_end_char
            && self.target_start_byte < self.target_end_byte
            && self.target_caret_byte >= self.target_start_byte
            && self.target_caret_byte <= self.target_end_byte
            && self.target_caret_char >= self.local_start_char
            && self.target_caret_char <= self.local_end_char
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PaintTextSelectionWitness {
    pub(crate) target_owner: NodeKey,
    pub(crate) target_stable_id: u64,
    pub(crate) local_start: usize,
    pub(crate) local_end: usize,
    pub(crate) fill: [f32; 4],
}

impl PaintTextSelectionWitness {
    pub(crate) fn is_canonical_for(self, owner: NodeKey, stable_id: u64) -> bool {
        self.target_owner == owner
            && self.target_stable_id == stable_id
            && self.local_start < self.local_end
            && self
                .fill
                .iter()
                .all(|channel| channel.is_finite() && (0.0..=1.0).contains(channel))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum PaintOpacityAuthority {
    #[default]
    Baked,
    NeutralRootEffect(EffectNodeId),
}

/// One owner in the canonical, cutout-aware content topology of a retained
/// effect raster.  Composite revisions are deliberately absent: opacity and
/// effect generations are composite authority and must not enter the raster
/// identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct EffectPropertyContentWitness {
    pub(crate) owner: NodeKey,
    pub(crate) stable_id: u64,
    pub(crate) parent: Option<NodeKey>,
    pub(crate) self_paint_revision: u64,
    pub(crate) topology_revision: u64,
}

/// Opaque, owning compiler/recorder authority materialized from the canonical
/// effect scaffold.  It freezes both sides of detachment: the exact live
/// effect/clip chains and the surface-local view that may enter the artifact.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EffectPropertySurfaceArtifactContract {
    boundary_root: NodeKey,
    stable_id: u64,
    isolated_leaf: EffectNodeSnapshot,
    live_effect_chain: Vec<EffectNodeSnapshot>,
    detached_ancestors: Vec<EffectNodeSnapshot>,
    local_raster_clips: Vec<crate::view::compositor::property_tree::ClipNodeSnapshot>,
    detached_ancestor_clips: Vec<crate::view::compositor::property_tree::ClipNodeSnapshot>,
    content: Vec<EffectPropertyContentWitness>,
}

impl EffectPropertySurfaceArtifactContract {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        boundary_root: NodeKey,
        stable_id: u64,
        isolated_leaf: EffectNodeSnapshot,
        live_effect_chain: Vec<EffectNodeSnapshot>,
        detached_ancestors: Vec<EffectNodeSnapshot>,
        local_raster_clips: Vec<crate::view::compositor::property_tree::ClipNodeSnapshot>,
        detached_ancestor_clips: Vec<crate::view::compositor::property_tree::ClipNodeSnapshot>,
        content: Vec<EffectPropertyContentWitness>,
    ) -> Option<Self> {
        let contract = Self {
            boundary_root,
            stable_id,
            isolated_leaf,
            live_effect_chain,
            detached_ancestors,
            local_raster_clips,
            detached_ancestor_clips,
            content,
        };
        contract.is_canonical().then_some(contract)
    }

    pub(crate) fn boundary_root(&self) -> NodeKey {
        self.boundary_root
    }

    pub(crate) fn stable_id(&self) -> u64 {
        self.stable_id
    }

    pub(crate) fn isolated_leaf(&self) -> EffectNodeSnapshot {
        self.isolated_leaf
    }

    pub(crate) fn live_effect_chain(&self) -> &[EffectNodeSnapshot] {
        &self.live_effect_chain
    }

    pub(crate) fn detached_ancestors(&self) -> &[EffectNodeSnapshot] {
        &self.detached_ancestors
    }

    pub(crate) fn local_raster_clips(
        &self,
    ) -> &[crate::view::compositor::property_tree::ClipNodeSnapshot] {
        &self.local_raster_clips
    }

    pub(crate) fn detached_ancestor_clips(
        &self,
    ) -> &[crate::view::compositor::property_tree::ClipNodeSnapshot] {
        &self.detached_ancestor_clips
    }

    pub(crate) fn isolated_local_raster_clips(
        &self,
    ) -> Vec<crate::view::compositor::property_tree::ClipNodeSnapshot> {
        let mut clips = self.local_raster_clips.clone();
        if let Some(root) = clips.last_mut() {
            root.parent = None;
        }
        clips
    }

    pub(crate) fn content(&self) -> &[EffectPropertyContentWitness] {
        &self.content
    }

    pub(crate) fn is_canonical(&self) -> bool {
        if self.boundary_root.is_null()
            || self.stable_id == 0
            || self.live_effect_chain.first()
                != Some(&EffectNodeSnapshot {
                    parent: self.live_effect_chain.first().and_then(|leaf| leaf.parent),
                    ..self.isolated_leaf
                })
            || self.isolated_leaf.id != EffectNodeId(self.boundary_root)
            || self.isolated_leaf.owner != self.boundary_root
            || self.isolated_leaf.parent.is_some()
            || self.detached_ancestors != self.live_effect_chain[1..]
            || self.content.is_empty()
        {
            return false;
        }
        let mut effects = rustc_hash::FxHashSet::default();
        for (index, effect) in self.live_effect_chain.iter().enumerate() {
            if effect.id.0 != effect.owner
                || effect.generation == 0
                || !effect.opacity.is_finite()
                || !(0.0..=1.0).contains(&effect.opacity)
                || !effects.insert(effect.id)
                || effect.parent != self.live_effect_chain.get(index + 1).map(|next| next.id)
            {
                return false;
            }
        }

        let mut clips = rustc_hash::FxHashSet::default();
        let full_clips = self
            .local_raster_clips
            .iter()
            .chain(&self.detached_ancestor_clips)
            .collect::<Vec<_>>();
        for (index, clip) in full_clips.iter().enumerate() {
            if clip.id.owner != clip.owner
                || clip.generation == 0
                || !matches!(
                    (clip.id.role, clip.behavior),
                    (
                        ClipNodeRole::SelfClip,
                        crate::view::compositor::property_tree::ClipBehavior::Replace
                    ) | (
                        ClipNodeRole::ContentsClip,
                        crate::view::compositor::property_tree::ClipBehavior::Intersect
                    )
                )
                || !clips.insert(clip.id)
                || clip.parent != full_clips.get(index + 1).map(|next| next.id)
            {
                return false;
            }
        }

        let mut owners = rustc_hash::FxHashSet::default();
        let mut stable_ids = rustc_hash::FxHashSet::default();
        for (index, witness) in self.content.iter().enumerate() {
            if witness.stable_id == 0
                || witness.self_paint_revision == 0
                || witness.topology_revision == 0
                || !stable_ids.insert(witness.stable_id)
                || (index == 0
                    && (witness.owner != self.boundary_root
                        || witness.stable_id != self.stable_id
                        || witness.parent.is_some()))
                || (index != 0
                    && witness
                        .parent
                        .is_none_or(|parent| parent == witness.owner || !owners.contains(&parent)))
                || !owners.insert(witness.owner)
            {
                return false;
            }
        }
        true
    }

    pub(crate) fn detach_effect_snapshot(
        &self,
        leaf: Option<EffectNodeId>,
        live: &[EffectNodeSnapshot],
    ) -> Option<Vec<EffectNodeSnapshot>> {
        (self.is_canonical()
            && leaf == Some(self.isolated_leaf.id)
            && live == self.live_effect_chain)
            .then(|| vec![self.isolated_leaf])
    }

    /// Removes only the exact frozen ancestor suffix. Descendant-local clips
    /// remain intact, while the boundary's own local chain must stay present.
    pub(crate) fn detach_clip_snapshot(
        &self,
        live: &[crate::view::compositor::property_tree::ClipNodeSnapshot],
    ) -> Option<Vec<crate::view::compositor::property_tree::ClipNodeSnapshot>> {
        if !self.is_canonical() {
            return None;
        }
        // Coverage already validated the full live chain before projecting an
        // ancestor-only leaf to `None`. Snapshot collection then observes an
        // empty projected chain, which is the exact local artifact view.
        if live.is_empty() && self.local_raster_clips.is_empty() {
            return Some(Vec::new());
        }
        if live.len() < self.detached_ancestor_clips.len()
            || live[live.len() - self.detached_ancestor_clips.len()..]
                != self.detached_ancestor_clips
        {
            return None;
        }
        let mut local = live[..live.len() - self.detached_ancestor_clips.len()].to_vec();
        if local.len() < self.local_raster_clips.len()
            || local[local.len() - self.local_raster_clips.len()..] != self.local_raster_clips
        {
            return None;
        }
        if let Some(root) = local.last_mut() {
            root.parent = None;
        }
        Some(local)
    }

    pub(crate) fn project_clip_leaf(
        &self,
        live_leaf: Option<ClipNodeId>,
        live: &[crate::view::compositor::property_tree::ClipNodeSnapshot],
    ) -> Option<Option<ClipNodeId>> {
        if live.first().map(|clip| clip.id) != live_leaf {
            return None;
        }
        self.detach_clip_snapshot(live)
            .map(|local| local.first().map(|clip| clip.id))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum PaintArtifactTarget {
    #[default]
    CurrentTarget,
    RootOpacityGroup {
        root: NodeKey,
        effect: EffectNodeId,
    },
}

#[derive(Clone, Debug, Default)]
pub(crate) struct PaintArtifact {
    pub(crate) target: PaintArtifactTarget,
    pub(crate) chunks: Vec<PaintChunk>,
    pub(crate) ops: Vec<PaintOp>,
    /// Complete, arena-independent transitive clip snapshot for every clip
    /// leaf referenced by `chunks` or `owner_property_states`.
    pub(crate) clip_nodes: Vec<ClipNodeSnapshot>,
    /// Complete, arena-independent transitive effect snapshot for every
    /// effect leaf referenced by `chunks` or `owner_property_states`. M6B
    /// validates this store but keeps the existing per-op baked opacity as
    /// visual authority.
    pub(crate) effect_nodes: Vec<EffectNodeSnapshot>,
    /// Complete arena-independent transform graph referenced by `chunks` or
    /// `owner_property_states`, including canonical derived owner projection.
    pub(crate) transform_nodes: Vec<TransformNodeSnapshot>,
    /// Complete transitive layout-position graph referenced by `chunks` or
    /// `owner_property_states`.
    pub(crate) layout_position_nodes: Vec<LayoutPositionNodeSnapshot>,
    /// Complete transitive owner-local visual-offset graph referenced by
    /// `chunks` or `owner_property_states`; parent links preserve ancestor
    /// animation composition.
    pub(crate) visual_offset_nodes: Vec<VisualOffsetNodeSnapshot>,
    /// Complete transitive scroll graph referenced by `chunks` or
    /// `owner_property_states`.
    pub(crate) scroll_nodes: Vec<ScrollNodeSnapshot>,
    /// Canonical frame-traversal ownership topology for every chunk owner and
    /// its transitive ancestors. Roots are explicitly parentless even if the
    /// same arena node has an out-of-scope parent.
    pub(crate) owner_nodes: Vec<PaintOwnerSnapshot>,
    /// Projected paint/descendants property endpoints for exactly the owners
    /// in `owner_nodes`. The producer derives both stores from the same
    /// chunk-owner ancestor closure; this exact key-set rule does not prove
    /// completeness for owners outside the artifact scene.
    pub(crate) owner_property_states: Vec<PaintOwnerPropertyStateSnapshot>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PaintOwnerSnapshot {
    pub(crate) owner: NodeKey,
    pub(crate) parent: Option<NodeKey>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PaintOwnerPropertyStateSnapshot {
    pub(crate) owner: NodeKey,
    /// Persistent identity for this owner across frames. This lives beside
    /// the owner's property endpoints rather than on [`PaintOwnerSnapshot`]
    /// so the lightweight topology store does not become a second identity
    /// authority. A retained surface key still combines this value with its
    /// surface role; `stable_id` alone is not a complete persistent key.
    pub(crate) stable_id: u64,
    pub(crate) paint: PropertyTreeState,
    pub(crate) descendants: PropertyTreeState,
}

/// Restricts the two stores already materialized by coverage recording to the
/// clip/effect closure of `target` chunks and owner endpoints. Spatial stores
/// remain the separate Stage C preflight responsibility; this artifact-local
/// projection is not a global property-tree completeness claim.
pub(super) fn project_clip_effect_snapshot_closure(
    target: &mut PaintArtifact,
    source: &PaintArtifact,
) -> Option<()> {
    let states = target
        .chunks
        .iter()
        .map(|chunk| chunk.properties)
        .chain(
            target
                .owner_property_states
                .iter()
                .flat_map(|snapshot| [snapshot.paint, snapshot.descendants]),
        )
        .collect::<Vec<_>>();

    let clip_nodes = source
        .clip_nodes
        .iter()
        .map(|snapshot| (snapshot.id, *snapshot))
        .collect::<FxHashMap<_, _>>();
    let effect_nodes = source
        .effect_nodes
        .iter()
        .map(|snapshot| (snapshot.id, *snapshot))
        .collect::<FxHashMap<_, _>>();
    let mut clips = FxHashSet::default();
    let mut effects = FxHashSet::default();
    let mut pending_clips = states
        .iter()
        .filter_map(|state| state.clip)
        .collect::<Vec<_>>();
    let mut pending_effects = states
        .iter()
        .filter_map(|state| state.effect)
        .collect::<Vec<_>>();
    while let Some(id) = pending_clips.pop() {
        if clips.insert(id) {
            if let Some(parent) = clip_nodes.get(&id)?.parent {
                pending_clips.push(parent);
            }
        }
    }
    while let Some(id) = pending_effects.pop() {
        if effects.insert(id) {
            if let Some(parent) = effect_nodes.get(&id)?.parent {
                pending_effects.push(parent);
            }
        }
    }
    target.clip_nodes = source
        .clip_nodes
        .iter()
        .copied()
        .filter(|snapshot| clips.contains(&snapshot.id))
        .collect();
    target.effect_nodes = source
        .effect_nodes
        .iter()
        .copied()
        .filter(|snapshot| effects.contains(&snapshot.id))
        .collect();
    Some(())
}

/// Returns only the clip/effect snapshots that participate in chunk raster
/// semantics. Owner endpoints may legitimately extend the artifact stores,
/// but those endpoint-only snapshots are transition data rather than raster
/// identity. Source order is preserved for the frozen identity callers.
pub(super) fn chunk_raster_property_snapshot_closure(
    artifact: &PaintArtifact,
) -> Option<(Vec<ClipNodeSnapshot>, Vec<EffectNodeSnapshot>)> {
    let clip_nodes = artifact
        .clip_nodes
        .iter()
        .map(|snapshot| (snapshot.id, *snapshot))
        .collect::<FxHashMap<_, _>>();
    let effect_nodes = artifact
        .effect_nodes
        .iter()
        .map(|snapshot| (snapshot.id, *snapshot))
        .collect::<FxHashMap<_, _>>();
    let mut clips = FxHashSet::default();
    let mut effects = FxHashSet::default();
    let mut pending_clips = artifact
        .chunks
        .iter()
        .filter_map(|chunk| chunk.properties.clip)
        .collect::<Vec<_>>();
    let mut pending_effects = artifact
        .chunks
        .iter()
        .filter_map(|chunk| chunk.properties.effect)
        .collect::<Vec<_>>();
    while let Some(id) = pending_clips.pop() {
        if clips.insert(id) {
            if let Some(parent) = clip_nodes.get(&id).and_then(|snapshot| snapshot.parent) {
                pending_clips.push(parent);
            }
        }
    }
    while let Some(id) = pending_effects.pop() {
        if effects.insert(id) {
            if let Some(parent) = effect_nodes.get(&id).and_then(|snapshot| snapshot.parent) {
                pending_effects.push(parent);
            }
        }
    }
    Some((
        artifact
            .clip_nodes
            .iter()
            .copied()
            .filter(|snapshot| clips.contains(&snapshot.id))
            .collect(),
        artifact
            .effect_nodes
            .iter()
            .copied()
            .filter(|snapshot| effects.contains(&snapshot.id))
            .collect(),
    ))
}

#[derive(Clone, Debug)]
pub(crate) struct PaintChunk {
    pub(crate) id: PaintChunkId,
    pub(crate) owner: NodeKey,
    pub(crate) op_range: Range<usize>,
    pub(crate) bounds: Rect,
    /// Observational identity only in this slice. Property trees do not drive
    /// rendering until transform/clip/effect coverage is complete.
    pub(crate) properties: PropertyTreeState,
    /// Everything currently baked into this chunk's raster content. Opacity
    /// remains baked in Phase 4, so composite/topology revisions are part of
    /// the content key even though the eventual compositor will split them.
    pub(crate) content_revision: PaintContentRevision,
    pub(crate) payload_identity: PaintPayloadIdentity,
}

/// Artifact-space identity for one raster-bearing chunk.
///
/// This deliberately carries no retained-policy or component grammar. Durable
/// consumers use it to freeze the generic chunk facts that must survive
/// preparation without retaining the complete artifact.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PaintChunkRasterIdentity {
    pub(crate) id: PaintChunkId,
    pub(crate) owner: NodeKey,
    pub(crate) bounds_bits: [u32; 4],
    pub(crate) payload_identity: PaintPayloadIdentity,
}

#[derive(Clone, Debug)]
pub(crate) struct PaintChunkMetadata {
    pub(crate) id: PaintChunkId,
    pub(crate) owner: NodeKey,
    pub(crate) bounds: Rect,
    pub(crate) properties: PropertyTreeState,
    pub(crate) content_revision: PaintContentRevision,
    pub(crate) payload_identity: PaintPayloadIdentity,
}

pub(crate) fn has_canonical_paint_bounds(bounds: Rect) -> bool {
    bounds.x.is_finite()
        && bounds.y.is_finite()
        && bounds.width.is_finite()
        && bounds.height.is_finite()
        && bounds.width >= 0.0
        && bounds.height >= 0.0
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct PaintContentRevision {
    pub(crate) self_paint_revision: u64,
    pub(crate) composite_revision: u64,
    pub(crate) topology_revision: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct PaintChunkId {
    pub(crate) owner: NodeKey,
    pub(crate) scope: PaintPropertyScope,
    pub(crate) phase: PaintNodePhase,
    pub(crate) slot: u16,
    pub(crate) role: PaintChunkRole,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) enum PaintPropertyScope {
    #[default]
    SelfPaint,
    Contents,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum PaintNodePhase {
    #[default]
    BeforeChildren,
    AfterChildren,
}

/// Reserved `SelfDecoration` slot for the typed child-mask scope.
/// The phase distinguishes the begin and end chunks.
pub(crate) const RETAINED_CHILD_MASK_SLOT: u16 = u16::MAX;

#[derive(Clone, Debug)]
pub(crate) struct RetainedChildMaskPlan {
    bounds: Rect,
    logical_scissor: [u32; 4],
    op: DrawRectOp,
    payload_identity: PaintPayloadIdentity,
    in_scope_children: Arc<[NodeKey]>,
    overflow_children: Arc<[NodeKey]>,
}

impl RetainedChildMaskPlan {
    pub(crate) fn new(
        bounds: Rect,
        logical_scissor: [u32; 4],
        op: DrawRectOp,
        children: &[NodeKey],
        in_scope_children: Vec<NodeKey>,
        overflow_children: Vec<NodeKey>,
    ) -> Option<Self> {
        if crate::view::base_component::exact_logical_scissor_for_rect(bounds)
            != Some(logical_scissor)
            || op.mode != crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly
            || op.params.position != [bounds.x, bounds.y]
            || op.params.size != [bounds.width, bounds.height]
            || op
                .params
                .size
                .iter()
                .any(|value| !value.is_finite() || *value <= 0.0)
        {
            return None;
        }
        let mut reconstructed = Vec::with_capacity(children.len());
        let mut in_scope = in_scope_children.iter().copied();
        let mut overflow = overflow_children.iter().copied();
        let mut next_in_scope = in_scope.next();
        let mut next_overflow = overflow.next();
        for &child in children {
            if next_in_scope == Some(child) {
                reconstructed.push(child);
                next_in_scope = in_scope.next();
            } else if next_overflow == Some(child) {
                reconstructed.push(child);
                next_overflow = overflow.next();
            } else {
                return None;
            }
        }
        if next_in_scope.is_some()
            || next_overflow.is_some()
            || reconstructed.as_slice() != children
        {
            return None;
        }
        let payload_identity = PaintPayloadIdentity::prepared_rects([&op])?;
        Some(Self {
            bounds,
            logical_scissor,
            op,
            payload_identity,
            in_scope_children: in_scope_children.into(),
            overflow_children: overflow_children.into(),
        })
    }

    pub(crate) fn is_canonical_for_children(&self, children: &[NodeKey]) -> bool {
        let mut expected_in_scope = self.in_scope_children.iter().copied();
        let mut expected_overflow = self.overflow_children.iter().copied();
        let mut next_in_scope = expected_in_scope.next();
        let mut next_overflow = expected_overflow.next();
        for &child in children {
            if next_in_scope == Some(child) {
                next_in_scope = expected_in_scope.next();
            } else if next_overflow == Some(child) {
                next_overflow = expected_overflow.next();
            } else {
                return false;
            }
        }
        next_in_scope.is_none() && next_overflow.is_none()
    }

    pub(crate) fn in_scope_children(&self) -> &[NodeKey] {
        &self.in_scope_children
    }

    pub(crate) fn overflow_children(&self) -> &[NodeKey] {
        &self.overflow_children
    }

    pub(crate) fn metadata(
        &self,
        owner: NodeKey,
        phase: PaintNodePhase,
        properties: PropertyTreeState,
        content_revision: PaintContentRevision,
    ) -> PaintChunkMetadata {
        PaintChunkMetadata {
            id: PaintChunkId {
                owner,
                scope: PaintPropertyScope::Contents,
                phase,
                slot: RETAINED_CHILD_MASK_SLOT,
                role: PaintChunkRole::SelfDecoration,
            },
            owner,
            bounds: self.bounds,
            properties,
            content_revision,
            payload_identity: self.payload_identity.clone(),
        }
    }

    pub(crate) fn artifact(
        &self,
        owner: NodeKey,
        phase: PaintNodePhase,
        properties: PropertyTreeState,
        content_revision: PaintContentRevision,
    ) -> PaintArtifact {
        PaintArtifact {
            target: Default::default(),
            chunks: vec![PaintChunk {
                id: PaintChunkId {
                    owner,
                    scope: PaintPropertyScope::Contents,
                    phase,
                    slot: RETAINED_CHILD_MASK_SLOT,
                    role: PaintChunkRole::SelfDecoration,
                },
                owner,
                op_range: 0..1,
                bounds: self.bounds,
                properties,
                content_revision,
                payload_identity: self.payload_identity.clone(),
            }],
            ops: vec![PaintOp::DrawRect(self.op.clone())],
            clip_nodes: Vec::new(),
            effect_nodes: Vec::new(),
            transform_nodes: Vec::new(),
            layout_position_nodes: Vec::new(),
            visual_offset_nodes: Vec::new(),
            scroll_nodes: Vec::new(),
            owner_property_states: Vec::new(),
            owner_nodes: vec![PaintOwnerSnapshot {
                owner,
                parent: None,
            }],
        }
    }

    pub(crate) fn logical_scissor(&self) -> [u32; 4] {
        self.logical_scissor
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct PaintNodePlan<T> {
    pub(crate) before_children: Vec<T>,
    pub(crate) after_children: Vec<T>,
}

impl<T> PaintNodePlan<T> {
    pub(crate) fn single_before(item: T) -> Self {
        Self {
            before_children: vec![item],
            after_children: Vec::new(),
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.before_children.is_empty() && self.after_children.is_empty()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum PaintChunkRole {
    SelfDecoration,
    TextGlyphs,
    ImageContent,
    SvgContent,
    GpuContent,
    SelectionUnderlay,
    TextDecoration,
    Caret,
    ScrollbarOverlay,
}

#[derive(Clone, Debug)]
pub(crate) enum PaintOp {
    DrawRect(DrawRectOp),
    PreparedInlineIfcDecoration(Arc<PreparedInlineIfcDecorationOp>),
    PreparedShadow(PreparedShadowOp),
    PreparedScrollbarOverlay(Arc<PreparedScrollbarOverlayOp>),
    PreparedText(PreparedTextOp),
    PreparedImage(PreparedImageOp),
    PreparedSvg(PreparedSvgOp),
    PreparedGpu(PreparedGpuOp),
}

impl PaintOp {
    pub(crate) fn inline_decoration(op: PreparedInlineIfcDecorationOp) -> Self {
        Self::PreparedInlineIfcDecoration(Arc::new(op))
    }
    pub(crate) fn scrollbar_overlay(op: PreparedScrollbarOverlayOp) -> Self {
        Self::PreparedScrollbarOverlay(Arc::new(op))
    }
}

#[derive(Clone, Debug)]
pub(crate) struct DrawRectOp {
    pub(crate) params: RectPassParams,
    pub(crate) mode: RectRenderMode,
}

#[derive(Clone, Debug)]
pub(crate) struct PreparedInlineIfcDecorationOp {
    pub(crate) descriptor: PreparedInlineIfcDecorationDescriptor,
    pub(crate) fill: RectPassParams,
    pub(crate) border: Option<RectPassParams>,
    identity: PreparedInlineIfcDecorationIdentity,
}

#[derive(Clone, Debug)]
pub(crate) struct PreparedInlineIfcDecorationDescriptor {
    pub(crate) source: u64,
    pub(crate) line_index: usize,
    pub(crate) range: Range<usize>,
    pub(crate) style_key: [u8; 4],
    pub(crate) slice_insets: [f32; 4],
    pub(crate) is_first_for_source: bool,
    pub(crate) is_last_for_source: bool,
}

impl PreparedInlineIfcDecorationOp {
    pub(crate) fn new(
        descriptor: PreparedInlineIfcDecorationDescriptor,
        fill: RectPassParams,
        border: Option<RectPassParams>,
    ) -> Option<Self> {
        let identity =
            PreparedInlineIfcDecorationIdentity::from_parts(&descriptor, &fill, border.as_ref())?;
        Some(Self {
            descriptor,
            fill,
            border,
            identity,
        })
    }

    pub(crate) fn has_canonical_identity(&self) -> bool {
        PreparedInlineIfcDecorationIdentity::from_parts(
            &self.descriptor,
            &self.fill,
            self.border.as_ref(),
        )
        .as_ref()
            == Some(&self.identity)
    }

    pub(crate) fn frozen_identity(&self) -> PreparedInlineIfcDecorationIdentity {
        self.identity.clone()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PreparedInlineIfcDecorationIdentity {
    source: u64,
    line_index: usize,
    range: Range<usize>,
    style_key: [u8; 4],
    slice_insets_bits: [u32; 4],
    is_first_for_source: bool,
    is_last_for_source: bool,
    fill: PreparedInlineIfcRectIdentity,
    border: Option<PreparedInlineIfcRectIdentity>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PreparedInlineIfcRectIdentity {
    position_bits: [u32; 2],
    size_bits: [u32; 2],
    fill_color_bits: [u32; 4],
    opacity_bits: u32,
    border_width_bits: [u32; 4],
    border_radius_bits: [[u32; 2]; 4],
    border_color_bits: [u32; 4],
    border_side_color_bits: [[u32; 4]; 4],
    use_border_side_colors: bool,
    depth_bits: u32,
}

impl PreparedInlineIfcDecorationIdentity {
    fn from_parts(
        descriptor: &PreparedInlineIfcDecorationDescriptor,
        fill: &RectPassParams,
        border: Option<&RectPassParams>,
    ) -> Option<Self> {
        if descriptor.range.start >= descriptor.range.end
            || descriptor
                .slice_insets
                .iter()
                .any(|value| !value.is_finite() || *value < 0.0)
        {
            return None;
        }
        let fill_identity = PreparedInlineIfcRectIdentity::from_params(fill)?;
        let border_identity = match border {
            Some(border) => Some(PreparedInlineIfcRectIdentity::from_params(border)?),
            None => None,
        };
        let has_border = fill.border_widths.iter().any(|width| *width > 0.0);
        if has_border != border.is_some() {
            return None;
        }
        if let Some(border) = border {
            if fill.position.map(f32::to_bits) != border.position.map(f32::to_bits)
                || fill.size.map(f32::to_bits) != border.size.map(f32::to_bits)
                || fill.opacity.to_bits() != border.opacity.to_bits()
                || fill.border_widths.map(f32::to_bits) != border.border_widths.map(f32::to_bits)
                || fill.border_radii.map(|radius| radius.map(f32::to_bits))
                    != border.border_radii.map(|radius| radius.map(f32::to_bits))
                || fill.border_color.map(f32::to_bits) != border.border_color.map(f32::to_bits)
                || fill.border_side_colors.map(|color| color.map(f32::to_bits))
                    != border
                        .border_side_colors
                        .map(|color| color.map(f32::to_bits))
                || fill.use_border_side_colors != border.use_border_side_colors
                || fill.depth.to_bits() != border.depth.to_bits()
                || border.fill_color.map(f32::to_bits) != [0.0_f32.to_bits(); 4]
                || !border.use_border_side_colors
            {
                return None;
            }
        }
        Some(Self {
            source: descriptor.source,
            line_index: descriptor.line_index,
            range: descriptor.range.clone(),
            style_key: descriptor.style_key,
            slice_insets_bits: descriptor.slice_insets.map(f32::to_bits),
            is_first_for_source: descriptor.is_first_for_source,
            is_last_for_source: descriptor.is_last_for_source,
            fill: fill_identity,
            border: border_identity,
        })
    }
}

impl PreparedInlineIfcRectIdentity {
    fn from_params(params: &RectPassParams) -> Option<Self> {
        let colors_are_valid = params
            .fill_color
            .iter()
            .chain(params.border_color.iter())
            .chain(params.border_side_colors.iter().flatten())
            .all(|channel| channel.is_finite() && (0.0..=1.0).contains(channel));
        if params.position.iter().any(|value| !value.is_finite())
            || params
                .size
                .iter()
                .any(|value| !value.is_finite() || *value <= 0.0)
            || !((params.position[0] + params.size[0]).is_finite()
                && (params.position[1] + params.size[1]).is_finite())
            || !colors_are_valid
            || !params.opacity.is_finite()
            || !(0.0..=1.0).contains(&params.opacity)
            || params
                .border_widths
                .iter()
                .any(|value| !value.is_finite() || *value < 0.0)
            || params
                .border_radii
                .iter()
                .flatten()
                .any(|value| !value.is_finite() || *value < 0.0)
            || !params.depth.is_finite()
            || params.gradient.is_some()
            || params.border_gradient.is_some()
        {
            return None;
        }
        Some(Self {
            position_bits: params.position.map(f32::to_bits),
            size_bits: params.size.map(f32::to_bits),
            fill_color_bits: params.fill_color.map(f32::to_bits),
            opacity_bits: params.opacity.to_bits(),
            border_width_bits: params.border_widths.map(f32::to_bits),
            border_radius_bits: params.border_radii.map(|radius| radius.map(f32::to_bits)),
            border_color_bits: params.border_color.map(f32::to_bits),
            border_side_color_bits: params
                .border_side_colors
                .map(|color| color.map(f32::to_bits)),
            use_border_side_colors: params.use_border_side_colors,
            depth_bits: params.depth.to_bits(),
        })
    }
}

#[derive(Clone, Debug)]
pub(crate) struct PreparedShadowOp {
    pub(crate) mesh: ShadowMesh,
    pub(crate) params: ShadowParams,
    pub(crate) identity: PreparedShadowIdentity,
}

impl PreparedShadowOp {
    pub(crate) fn new(mesh: ShadowMesh, params: ShadowParams) -> Option<Self> {
        let identity = PreparedShadowIdentity::from_parts(&mesh, params)?;
        Some(Self {
            mesh,
            params,
            identity,
        })
    }

    pub(crate) fn has_canonical_identity(&self) -> bool {
        PreparedShadowIdentity::from_parts(&self.mesh, self.params).as_ref() == Some(&self.identity)
    }

    pub(crate) fn frozen_identity(&self) -> PreparedShadowIdentity {
        self.identity.clone()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PreparedShadowIdentity {
    vertices_bits: Vec<[u32; 2]>,
    indices: Vec<u32>,
    offset_bits: [u32; 2],
    blur_radius_bits: u32,
    color_bits: [u32; 4],
    opacity_bits: u32,
    spread_bits: u32,
    clip_to_geometry: bool,
}

/// One indivisible legacy-order scrollbar overlay. Keeping the two shadows
/// and two fills behind one typed op prevents generic shadow/rect grammar from
/// being widened for the retained scroll-host canary.
#[derive(Clone, Debug)]
pub(crate) struct PreparedScrollbarOverlayOp {
    pub(crate) track_shadow: PreparedScrollbarShadowOp,
    pub(crate) track: DrawRectOp,
    pub(crate) thumb_shadow: PreparedScrollbarShadowOp,
    pub(crate) thumb: DrawRectOp,
    secondary: Option<Box<PreparedScrollbarAxisOp>>,
    identity: PreparedScrollbarOverlayIdentity,
}

#[derive(Clone, Debug)]
struct PreparedScrollbarAxisOp {
    track_shadow: PreparedScrollbarShadowOp,
    track: DrawRectOp,
    thumb_shadow: PreparedScrollbarShadowOp,
    thumb: DrawRectOp,
}

#[derive(Clone, Debug)]
pub(crate) struct PreparedScrollbarShadowOp {
    pub(crate) mesh: ShadowMesh,
    pub(crate) params: ShadowParams,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PreparedScrollbarOverlayIdentity {
    track_shadow: PreparedScrollbarShadowIdentity,
    track: PreparedDrawRectIdentity,
    thumb_shadow: PreparedScrollbarShadowIdentity,
    thumb: PreparedDrawRectIdentity,
    secondary: Option<Box<PreparedScrollbarAxisIdentity>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PreparedScrollbarAxisIdentity {
    track_shadow: PreparedScrollbarShadowIdentity,
    track: PreparedDrawRectIdentity,
    thumb_shadow: PreparedScrollbarShadowIdentity,
    thumb: PreparedDrawRectIdentity,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PreparedScrollbarShadowIdentity {
    vertices_bits: Vec<[u32; 2]>,
    indices: Vec<u32>,
    offset_bits: [u32; 2],
    blur_radius_bits: u32,
    color_bits: [u32; 4],
    opacity_bits: u32,
    spread_bits: u32,
    clip_to_geometry: bool,
}

impl PreparedScrollbarOverlayOp {
    pub(crate) fn from_witness(witness: ScrollbarOverlayWitness) -> Option<Self> {
        let alpha = witness.sampled_alpha;
        if !matches!(
            witness.paint_state,
            ScrollbarPaintStateWitness::OpaqueNow | ScrollbarPaintStateWitness::TranslucentNow
        ) || !alpha.is_finite()
            || alpha <= 0.0
            || alpha > 1.0
            || (witness.paint_state == ScrollbarPaintStateWitness::OpaqueNow
                && alpha.to_bits() != 1.0_f32.to_bits())
            || (witness.paint_state == ScrollbarPaintStateWitness::TranslucentNow
                && (alpha.to_bits() == 0.0_f32.to_bits() || alpha.to_bits() == 1.0_f32.to_bits()))
            || !witness.shadow_blur_radius.is_finite()
            || witness.shadow_blur_radius < 0.0
        {
            return None;
        }
        let mut axes = Vec::with_capacity(2);
        if let Some((track, thumb)) = witness.vertical_track.zip(witness.vertical_thumb) {
            axes.push(Self::axis(track, thumb, witness.shadow_blur_radius, alpha)?);
        } else if witness.vertical_track.is_some() || witness.vertical_thumb.is_some() {
            return None;
        }
        if let Some((track, thumb)) = witness.horizontal_track.zip(witness.horizontal_thumb) {
            axes.push(Self::axis(track, thumb, witness.shadow_blur_radius, alpha)?);
        } else if witness.horizontal_track.is_some() || witness.horizontal_thumb.is_some() {
            return None;
        }
        if axes.is_empty() {
            return None;
        }
        let first = axes.remove(0);
        let secondary = axes.pop().map(Box::new);
        let identity = PreparedScrollbarOverlayIdentity::from_parts(
            &first.track_shadow,
            &first.track,
            &first.thumb_shadow,
            &first.thumb,
            secondary.as_deref(),
        )?;
        Some(Self {
            track_shadow: first.track_shadow,
            track: first.track,
            thumb_shadow: first.thumb_shadow,
            thumb: first.thumb,
            secondary,
            identity,
        })
    }

    pub(crate) fn from_vertical_witness(witness: ScrollbarOverlayWitness) -> Option<Self> {
        if witness.horizontal_track.is_some() || witness.horizontal_thumb.is_some() {
            return None;
        }
        Self::from_witness(witness)
    }

    fn axis(
        track: Rect,
        thumb: Rect,
        shadow_blur_radius: f32,
        alpha: f32,
    ) -> Option<PreparedScrollbarAxisOp> {
        let track_shadow = Self::shadow(track, shadow_blur_radius, 0.5 * alpha)?;
        let track = Self::fill(track, [0.95, 0.95, 0.95, 0.35 * alpha])?;
        let thumb_shadow = Self::shadow(thumb, shadow_blur_radius, 0.5 * alpha)?;
        let thumb = Self::fill(thumb, [0.95, 0.95, 0.95, 0.58 * alpha])?;
        Some(PreparedScrollbarAxisOp {
            track_shadow,
            track,
            thumb_shadow,
            thumb,
        })
    }

    fn shadow(rect: Rect, blur_radius: f32, alpha: f32) -> Option<PreparedScrollbarShadowOp> {
        let radius = (rect.width.min(rect.height) * 0.5).max(0.0);
        let shadow = PreparedScrollbarShadowOp {
            mesh: ShadowMesh::rounded_rect(
                rect.x,
                rect.y,
                rect.width.max(0.0),
                rect.height.max(0.0),
                radius,
            ),
            params: ShadowParams {
                offset_x: 1.0,
                offset_y: 1.0,
                blur_radius,
                color: [0.0, 0.0, 0.0, alpha],
                opacity: 1.0,
                spread: 0.0,
                clip_to_geometry: true,
            },
        };
        PreparedScrollbarShadowIdentity::from_parts(&shadow.mesh, shadow.params)?;
        Some(shadow)
    }

    fn fill(rect: Rect, color: [f32; 4]) -> Option<DrawRectOp> {
        let mut params = RectPassParams {
            position: [rect.x, rect.y],
            size: [rect.width, rect.height],
            fill_color: color,
            opacity: 1.0,
            ..Default::default()
        };
        params.set_border_width(0.0);
        params.set_border_radius((rect.width.min(rect.height) * 0.5).max(0.0));
        let op = DrawRectOp {
            params,
            mode: RectRenderMode::FillOnly,
        };
        PreparedDrawRectIdentity::from_op(&op)?;
        Some(op)
    }

    pub(crate) fn has_canonical_identity(&self) -> bool {
        PreparedScrollbarOverlayIdentity::from_parts(
            &self.track_shadow,
            &self.track,
            &self.thumb_shadow,
            &self.thumb,
            self.secondary.as_deref(),
        )
        .as_ref()
            == Some(&self.identity)
    }

    pub(crate) fn matches_vertical_witness(&self, witness: ScrollbarOverlayWitness) -> bool {
        self.has_canonical_identity()
            && Self::from_vertical_witness(witness)
                .is_some_and(|expected| expected.identity == self.identity)
    }

    pub(crate) fn matches_witness(&self, witness: ScrollbarOverlayWitness) -> bool {
        self.has_canonical_identity()
            && Self::from_witness(witness)
                .is_some_and(|expected| expected.identity == self.identity)
    }

    pub(crate) fn secondary_axis(
        &self,
    ) -> Option<(
        &PreparedScrollbarShadowOp,
        &DrawRectOp,
        &PreparedScrollbarShadowOp,
        &DrawRectOp,
    )> {
        self.secondary.as_deref().map(|axis| {
            (
                &axis.track_shadow,
                &axis.track,
                &axis.thumb_shadow,
                &axis.thumb,
            )
        })
    }

    pub(crate) fn frozen_identity(&self) -> PreparedScrollbarOverlayIdentity {
        self.identity.clone()
    }

    /// Rebuilds every spatial primitive in one indivisible overlay while
    /// retaining the axis order and semantic witness frozen by this type.
    pub(crate) fn translated_by(&self, delta: [f32; 2]) -> Option<Self> {
        if delta.into_iter().any(|value| !value.is_finite()) {
            return None;
        }
        let mut translated = self.clone();
        let translate_shadow = |shadow: &mut PreparedScrollbarShadowOp| -> Option<()> {
            for vertex in &mut shadow.mesh.vertices {
                vertex[0] += delta[0];
                vertex[1] += delta[1];
                if vertex.iter().any(|value| !value.is_finite()) {
                    return None;
                }
            }
            Some(())
        };
        let translate_rect = |rect: &mut DrawRectOp| -> Option<()> {
            rect.params.position[0] += delta[0];
            rect.params.position[1] += delta[1];
            rect.params
                .position
                .iter()
                .all(|value| value.is_finite())
                .then_some(())
        };
        translate_shadow(&mut translated.track_shadow)?;
        translate_rect(&mut translated.track)?;
        translate_shadow(&mut translated.thumb_shadow)?;
        translate_rect(&mut translated.thumb)?;
        if let Some(axis) = translated.secondary.as_deref_mut() {
            translate_shadow(&mut axis.track_shadow)?;
            translate_rect(&mut axis.track)?;
            translate_shadow(&mut axis.thumb_shadow)?;
            translate_rect(&mut axis.thumb)?;
        }
        translated.identity = PreparedScrollbarOverlayIdentity::from_parts(
            &translated.track_shadow,
            &translated.track,
            &translated.thumb_shadow,
            &translated.thumb,
            translated.secondary.as_deref(),
        )?;
        Some(translated)
    }

    pub(crate) fn has_baked_opacity(&self, expected_bits: u32) -> bool {
        self.track_shadow.params.opacity.to_bits() == expected_bits
            && self.track.params.opacity.to_bits() == expected_bits
            && self.thumb_shadow.params.opacity.to_bits() == expected_bits
            && self.thumb.params.opacity.to_bits() == expected_bits
            && self.secondary.as_deref().is_none_or(|axis| {
                axis.track_shadow.params.opacity.to_bits() == expected_bits
                    && axis.track.params.opacity.to_bits() == expected_bits
                    && axis.thumb_shadow.params.opacity.to_bits() == expected_bits
                    && axis.thumb.params.opacity.to_bits() == expected_bits
            })
    }

    /// Rebuilds the frozen overlay identity after the surface raster planner
    /// removes the owner-local opacity that will be applied by the composite
    /// edge. Every primitive participates; a partially neutralized overlay is
    /// not representable.
    pub(crate) fn with_baked_opacity(&self, opacity: f32) -> Option<Self> {
        if !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {
            return None;
        }
        let mut rebuilt = self.clone();
        let set_axis = |track_shadow: &mut PreparedScrollbarShadowOp,
                        track: &mut DrawRectOp,
                        thumb_shadow: &mut PreparedScrollbarShadowOp,
                        thumb: &mut DrawRectOp| {
            track_shadow.params.opacity = opacity;
            track.params.opacity = opacity;
            thumb_shadow.params.opacity = opacity;
            thumb.params.opacity = opacity;
        };
        set_axis(
            &mut rebuilt.track_shadow,
            &mut rebuilt.track,
            &mut rebuilt.thumb_shadow,
            &mut rebuilt.thumb,
        );
        if let Some(axis) = rebuilt.secondary.as_deref_mut() {
            set_axis(
                &mut axis.track_shadow,
                &mut axis.track,
                &mut axis.thumb_shadow,
                &mut axis.thumb,
            );
        }
        rebuilt.identity = PreparedScrollbarOverlayIdentity::from_parts(
            &rebuilt.track_shadow,
            &rebuilt.track,
            &rebuilt.thumb_shadow,
            &rebuilt.thumb,
            rebuilt.secondary.as_deref(),
        )?;
        Some(rebuilt)
    }

    #[cfg(test)]
    pub(crate) fn axis_geometry_bits_for_test(&self) -> Vec<([u32; 4], [u32; 4])> {
        let rect_bits = |op: &DrawRectOp| {
            (
                [
                    op.params.position[0].to_bits(),
                    op.params.position[1].to_bits(),
                    op.params.size[0].to_bits(),
                    op.params.size[1].to_bits(),
                ],
                op.params.fill_color.map(f32::to_bits),
            )
        };
        let mut axes = vec![(rect_bits(&self.track).0, rect_bits(&self.thumb).0)];
        if let Some(axis) = self.secondary.as_deref() {
            axes.push((rect_bits(&axis.track).0, rect_bits(&axis.thumb).0));
        }
        axes
    }

    #[cfg(test)]
    pub(crate) fn tamper_track_position_for_test(&mut self) {
        self.track.params.position[0] += 1.0;
    }

    #[cfg(test)]
    pub(crate) fn tamper_primary_axis_for_test(&mut self) {
        self.track.params.position.swap(0, 1);
        self.track.params.size.swap(0, 1);
        self.thumb.params.position.swap(0, 1);
        self.thumb.params.size.swap(0, 1);
    }

    #[cfg(test)]
    pub(crate) fn tamper_thumb_size_for_test(&mut self) {
        self.thumb.params.size[0] += 1.0;
    }

    #[cfg(test)]
    pub(crate) fn tamper_alpha_for_test(&mut self) {
        self.track.params.fill_color[3] *= 0.5;
    }

    #[cfg(test)]
    pub(crate) fn tamper_axis_order_for_test(&mut self) -> bool {
        let Some(secondary) = self.secondary.as_deref_mut() else {
            return false;
        };
        std::mem::swap(&mut self.track_shadow, &mut secondary.track_shadow);
        std::mem::swap(&mut self.track, &mut secondary.track);
        std::mem::swap(&mut self.thumb_shadow, &mut secondary.thumb_shadow);
        std::mem::swap(&mut self.thumb, &mut secondary.thumb);
        true
    }
}

impl PreparedScrollbarOverlayIdentity {
    fn from_parts(
        track_shadow: &PreparedScrollbarShadowOp,
        track: &DrawRectOp,
        thumb_shadow: &PreparedScrollbarShadowOp,
        thumb: &DrawRectOp,
        secondary: Option<&PreparedScrollbarAxisOp>,
    ) -> Option<Self> {
        Some(Self {
            track_shadow: PreparedScrollbarShadowIdentity::from_parts(
                &track_shadow.mesh,
                track_shadow.params,
            )?,
            track: PreparedDrawRectIdentity::from_op(track)?,
            thumb_shadow: PreparedScrollbarShadowIdentity::from_parts(
                &thumb_shadow.mesh,
                thumb_shadow.params,
            )?,
            thumb: PreparedDrawRectIdentity::from_op(thumb)?,
            secondary: match secondary {
                Some(axis) => Some(Box::new(PreparedScrollbarAxisIdentity::from_axis(axis)?)),
                None => None,
            },
        })
    }
}

impl PreparedScrollbarAxisIdentity {
    fn from_axis(axis: &PreparedScrollbarAxisOp) -> Option<Self> {
        Some(Self {
            track_shadow: PreparedScrollbarShadowIdentity::from_parts(
                &axis.track_shadow.mesh,
                axis.track_shadow.params,
            )?,
            track: PreparedDrawRectIdentity::from_op(&axis.track)?,
            thumb_shadow: PreparedScrollbarShadowIdentity::from_parts(
                &axis.thumb_shadow.mesh,
                axis.thumb_shadow.params,
            )?,
            thumb: PreparedDrawRectIdentity::from_op(&axis.thumb)?,
        })
    }
}

impl PreparedScrollbarShadowIdentity {
    fn from_parts(mesh: &ShadowMesh, params: ShadowParams) -> Option<Self> {
        if mesh.vertices.is_empty()
            || mesh.indices.is_empty()
            || mesh.indices.len() % 3 != 0
            || mesh
                .vertices
                .iter()
                .flatten()
                .any(|coordinate| !coordinate.is_finite())
            || mesh
                .indices
                .iter()
                .any(|&index| index as usize >= mesh.vertices.len())
            || !params.offset_x.is_finite()
            || !params.offset_y.is_finite()
            || !params.blur_radius.is_finite()
            || params.blur_radius < 0.0
            || params
                .color
                .iter()
                .any(|channel| !channel.is_finite() || !(0.0..=1.0).contains(channel))
            || !params.opacity.is_finite()
            || !(0.0..=1.0).contains(&params.opacity)
            || !params.spread.is_finite()
        {
            return None;
        }
        Some(Self {
            vertices_bits: mesh
                .vertices
                .iter()
                .map(|vertex| vertex.map(f32::to_bits))
                .collect(),
            indices: mesh.indices.clone(),
            offset_bits: [params.offset_x.to_bits(), params.offset_y.to_bits()],
            blur_radius_bits: params.blur_radius.to_bits(),
            color_bits: params.color.map(f32::to_bits),
            opacity_bits: params.opacity.to_bits(),
            spread_bits: params.spread.to_bits(),
            clip_to_geometry: params.clip_to_geometry,
        })
    }
}

impl PreparedShadowIdentity {
    fn from_parts(mesh: &ShadowMesh, params: ShadowParams) -> Option<Self> {
        if mesh.vertices.is_empty()
            || mesh.indices.is_empty()
            || mesh.indices.len() % 3 != 0
            || mesh
                .vertices
                .iter()
                .flatten()
                .any(|coordinate| !coordinate.is_finite())
            || mesh
                .indices
                .iter()
                .any(|&index| index as usize >= mesh.vertices.len())
            || !params.offset_x.is_finite()
            || !params.offset_y.is_finite()
            || !params.blur_radius.is_finite()
            || params.blur_radius.to_bits() != params.blur_radius.max(0.0).to_bits()
            || params
                .color
                .iter()
                .any(|channel| !channel.is_finite() || !(0.0..=1.0).contains(channel))
            || !params.opacity.is_finite()
            || !(0.0..=1.0).contains(&params.opacity)
            || params.spread.to_bits() != 0.0_f32.to_bits()
        {
            return None;
        }
        Some(Self {
            vertices_bits: mesh
                .vertices
                .iter()
                .map(|vertex| vertex.map(f32::to_bits))
                .collect(),
            indices: mesh.indices.clone(),
            offset_bits: [params.offset_x.to_bits(), params.offset_y.to_bits()],
            blur_radius_bits: params.blur_radius.to_bits(),
            color_bits: params.color.map(f32::to_bits),
            opacity_bits: params.opacity.to_bits(),
            spread_bits: params.spread.to_bits(),
            clip_to_geometry: params.clip_to_geometry,
        })
    }
}

#[derive(Clone, Debug)]
pub(crate) struct PreparedTextOp {
    pub(crate) params: Arc<TextPassPreparedParams>,
    // Sharing immutable validated input makes clone/replay and revalidation
    // constant-time. A changed allocation still goes through full validation.
    validated_params: Arc<TextPassPreparedParams>,
    identity: PreparedTextIdentity,
}

impl PreparedTextOp {
    /// Validate the unclipped source used by Text capability, with the same
    /// glyph/fragment rules as `new`, without allocating an op or identity.
    /// Recorded ops still validate their own scissor through `new`.
    pub(crate) fn validate_unclipped_glyph_stream(
        scale_factor: f32,
        fragments: &[TextPassPreparedFragment],
        mut glyphs: impl ExactSizeIterator<Item = TextPassPreparedStagingGlyphInput>,
    ) -> bool {
        PreparedTextIdentity::valid_header(scale_factor, glyphs.len(), fragments, None)
            && fragments
                .iter()
                .all(|fragment| PreparedTextFragmentIdentity::from_fragment(fragment).is_some())
            && glyphs
                .all(|glyph| PreparedTextGlyphIdentity::from_glyph(&glyph, fragments).is_some())
    }

    pub(crate) fn new(params: impl Into<Arc<TextPassPreparedParams>>) -> Option<Self> {
        let params = params.into();
        #[cfg(test)]
        prepared_text_identity_tests::note_construction();
        let identity = PreparedTextIdentity::from_params(&params)?;
        Some(Self {
            validated_params: params.clone(),
            params,
            identity,
        })
    }

    pub(crate) fn has_canonical_identity(&self) -> bool {
        Arc::ptr_eq(&self.params, &self.validated_params)
            || self.identity.matches_params(&self.params)
    }

    #[cfg(test)]
    pub(crate) fn construction_count_for_test() -> usize {
        prepared_text_identity_tests::construction_count()
    }

    pub(crate) fn frozen_identity(&self) -> PreparedTextIdentity {
        self.identity.clone()
    }
}

#[derive(Clone, Debug)]
pub(crate) struct PreparedImageOp {
    pub(crate) params: TextureCompositeParams,
    pub(crate) upload: SampledTextureUpload,
}

/// Arena- and registry-independent SVG raster payload frozen for one paint
/// artifact. SVG keeps a distinct op/identity from Image even though both
/// currently compile to `TextureCompositePass`; the typed boundary prevents
/// the two asset namespaces from becoming interchangeable by accident.
#[derive(Clone, Debug)]
pub(crate) struct PreparedSvgOp {
    pub(crate) params: TextureCompositeParams,
    pub(crate) upload: SampledTextureUpload,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PreparedDrawRectIdentity {
    mode: RectRenderMode,
    params: PreparedDrawRectParamsIdentity,
}

/// Component-independent semantic and raster identity for a text selection
/// underlay.
///
/// The character range is intentionally duplicated beside the exact ordered
/// rectangle identities. The payload validates its own range, color, and draw
/// operations; no component-owned grammar is needed to make it canonical.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TextSelectionPayloadIdentity {
    pub(crate) start_char: usize,
    pub(crate) end_char: usize,
    pub(crate) color_rgba_bits: [u32; 4],
    pub(crate) rects: Arc<[PreparedDrawRectIdentity]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PaintArtifactContractViolation {
    CompositeOwner,
    CompositeBounds,
    CompositeClip,
    CompositePayload,
    CompositePhaseOrder,
    CompositeSourceParity,
    SelectionRange,
    SelectionColor,
    SelectionRectIdentity,
    SelectionSourceParity,
    TransitionOrigin,
    TransitionRevision,
    TransitionRevisionParity,
    TransitionSourceParity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PaintArtifactContractRejection {
    pub(crate) owner: NodeKey,
    pub(crate) violation: PaintArtifactContractViolation,
}

/// Component-independent source facts for one text-selection underlay.
///
/// Geometry and prepared draw identities are deliberately absent here: the
/// recorder must obtain those from the emitted artifact and bind them through
/// [`TextSelectionPayloadIdentity`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PaintTextSelectionSource {
    pub(crate) start_char: usize,
    pub(crate) end_char: usize,
    pub(crate) color_rgba_bits: [u32; 4],
}

impl PaintTextSelectionSource {
    pub(crate) fn is_canonical(self) -> bool {
        self.start_char < self.end_char
            && self
                .color_rgba_bits
                .map(f32::from_bits)
                .into_iter()
                .all(|channel| channel.is_finite() && (0.0..=1.0).contains(&channel))
    }

    pub(crate) fn matches_payload(self, payload: &TextSelectionPayloadIdentity) -> bool {
        self.is_canonical()
            && payload.matches_source(self.start_char, self.end_char, self.color_rgba_bits)
    }

    pub(crate) fn validate_payload_for_owner(
        self,
        owner: NodeKey,
        payload: &TextSelectionPayloadIdentity,
    ) -> Result<(), PaintArtifactContractRejection> {
        payload.validate_for_owner(owner)?;
        self.matches_payload(payload)
            .then_some(())
            .ok_or(PaintArtifactContractRejection {
                owner,
                violation: PaintArtifactContractViolation::SelectionSourceParity,
            })
    }
}

/// Generic semantic source for a resident text payload. These cases describe
/// emitted text primitives, not a component or surface grammar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PaintTextContentSource {
    Glyphs,
    Selection(PaintTextSelectionSource),
    Preedit,
}

impl PaintTextContentSource {
    pub(crate) fn is_canonical(self) -> bool {
        match self {
            Self::Glyphs | Self::Preedit => true,
            Self::Selection(selection) => selection.is_canonical(),
        }
    }

    pub(crate) fn selection(self) -> Option<PaintTextSelectionSource> {
        match self {
            Self::Selection(selection) => Some(selection),
            Self::Glyphs | Self::Preedit => None,
        }
    }

    pub(crate) fn has_preedit(self) -> bool {
        matches!(self, Self::Preedit)
    }
}

/// Artifact-facing facts for one admitted atomic projection.
///
/// This intentionally contains neither the component's exact-shape grammar
/// nor layout/measure internals. The exact admission remains a transient
/// selector predicate; recorder and compiler consume only the owner topology
/// and projection geometry that the emitted artifact must reproduce.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PaintAtomicProjectionArtifactSource {
    pub(crate) projection_text_owner: NodeKey,
    pub(crate) projection_text_bounds_bits: [u32; 4],
    pub(crate) descendant_owner_topology: Arc<[PaintOwnerSnapshot]>,
}

impl PaintAtomicProjectionArtifactSource {
    pub(crate) fn is_canonical_for(&self, text_area_root: NodeKey) -> bool {
        let [x, y, width, height] = self.projection_text_bounds_bits.map(f32::from_bits);
        if self.projection_text_owner == text_area_root
            || [x, y, width, height]
                .into_iter()
                .any(|value| !value.is_finite())
            || width <= 0.0
            || height <= 0.0
            || !(x + width).is_finite()
            || !(y + height).is_finite()
            || self.descendant_owner_topology.is_empty()
        {
            return false;
        }
        let mut parents = FxHashMap::default();
        if self
            .descendant_owner_topology
            .iter()
            .any(|owner| parents.insert(owner.owner, owner.parent).is_some())
        {
            return false;
        }
        let Some(Some(projection_root)) = parents.get(&self.projection_text_owner).copied() else {
            return false;
        };
        projection_root != text_area_root
            && parents.get(&projection_root) == Some(&Some(text_area_root))
            && self.descendant_owner_topology.iter().all(|owner| {
                owner.owner != text_area_root
                    && owner.parent.is_some_and(|parent| {
                        parent == text_area_root || parents.contains_key(&parent)
                    })
            })
    }
}

impl TextSelectionPayloadIdentity {
    fn validate_parts(&self) -> Result<(), PaintArtifactContractViolation> {
        if self.start_char >= self.end_char {
            return Err(PaintArtifactContractViolation::SelectionRange);
        }
        if self
            .color_rgba_bits
            .map(f32::from_bits)
            .into_iter()
            .any(|channel| !channel.is_finite() || !(0.0..=1.0).contains(&channel))
        {
            return Err(PaintArtifactContractViolation::SelectionColor);
        }
        if self.rects.is_empty()
            || self.rects.iter().any(|rect| {
                rect.mode != RectRenderMode::FillOnly
                    || rect.params.fill_color_bits != self.color_rgba_bits
                    || rect.params.opacity_bits != 1.0_f32.to_bits()
            })
        {
            return Err(PaintArtifactContractViolation::SelectionRectIdentity);
        }
        Ok(())
    }

    pub(crate) fn validate_for_owner(
        &self,
        owner: NodeKey,
    ) -> Result<(), PaintArtifactContractRejection> {
        self.validate_parts()
            .map_err(|violation| PaintArtifactContractRejection { owner, violation })
    }

    pub(crate) fn validate_exact_ops_for_owner(
        &self,
        owner: NodeKey,
        rects: &[DrawRectOp],
    ) -> Result<(), PaintArtifactContractRejection> {
        self.validate_for_owner(owner)?;
        let exact = PaintPayloadIdentity::prepared_text_selection(
            self.start_char,
            self.end_char,
            self.color_rgba_bits,
            rects,
        );
        (exact.as_ref() == Some(&PaintPayloadIdentity::TextSelection(self.clone())))
            .then_some(())
            .ok_or(PaintArtifactContractRejection {
                owner,
                violation: PaintArtifactContractViolation::SelectionRectIdentity,
            })
    }

    pub(crate) fn is_canonical(&self) -> bool {
        self.validate_parts().is_ok()
    }

    pub(crate) fn matches_source(
        &self,
        start_char: usize,
        end_char: usize,
        color_rgba_bits: [u32; 4],
    ) -> bool {
        self.is_canonical()
            && self.start_char == start_char
            && self.end_char == end_char
            && self.color_rgba_bits == color_rgba_bits
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PreparedDrawRectParamsIdentity {
    position_bits: [u32; 2],
    size_bits: [u32; 2],
    fill_color_bits: [u32; 4],
    opacity_bits: u32,
    border_width_bits: [u32; 4],
    border_radius_bits: [[u32; 2]; 4],
    border_color_bits: [u32; 4],
    border_side_color_bits: [[u32; 4]; 4],
    use_border_side_colors: bool,
    depth_bits: u32,
    gradient: Option<PreparedGradientIdentity>,
    border_gradient: Option<PreparedGradientIdentity>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PreparedGradientIdentity {
    kind: GradientKindGpu,
    axis_bits: [u32; 4],
    repeating: bool,
    stops: Arc<[PreparedGradientStopIdentity]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PreparedGradientStopIdentity {
    color_bits: [u32; 4],
    position_bits: [u32; 4],
}

impl PreparedDrawRectIdentity {
    fn from_op(op: &DrawRectOp) -> Option<Self> {
        Some(Self {
            mode: op.mode,
            params: PreparedDrawRectParamsIdentity::from_params(&op.params)?,
        })
    }
}

impl PreparedDrawRectParamsIdentity {
    fn from_params(params: &RectPassParams) -> Option<Self> {
        let colors_are_valid = params
            .fill_color
            .iter()
            .chain(params.border_color.iter())
            .chain(params.border_side_colors.iter().flatten())
            .all(|channel| channel.is_finite() && (0.0..=1.0).contains(channel));
        if params.position.iter().any(|value| !value.is_finite())
            || params
                .size
                .iter()
                .any(|value| !value.is_finite() || *value <= 0.0)
            || !((params.position[0] + params.size[0]).is_finite()
                && (params.position[1] + params.size[1]).is_finite())
            || !colors_are_valid
            || !params.opacity.is_finite()
            || !(0.0..=1.0).contains(&params.opacity)
            || params
                .border_widths
                .iter()
                .any(|value| !value.is_finite() || *value < 0.0)
            || params
                .border_radii
                .iter()
                .flatten()
                .any(|value| !value.is_finite() || *value < 0.0)
            || !params.depth.is_finite()
        {
            return None;
        }
        Some(Self {
            position_bits: params.position.map(f32::to_bits),
            size_bits: params.size.map(f32::to_bits),
            fill_color_bits: params.fill_color.map(f32::to_bits),
            opacity_bits: params.opacity.to_bits(),
            border_width_bits: params.border_widths.map(f32::to_bits),
            border_radius_bits: params.border_radii.map(|radius| radius.map(f32::to_bits)),
            border_color_bits: params.border_color.map(f32::to_bits),
            border_side_color_bits: params
                .border_side_colors
                .map(|color| color.map(f32::to_bits)),
            use_border_side_colors: params.use_border_side_colors,
            depth_bits: params.depth.to_bits(),
            gradient: match params.gradient.as_ref() {
                Some(gradient) => Some(PreparedGradientIdentity::from_paint(gradient)?),
                None => None,
            },
            border_gradient: match params.border_gradient.as_ref() {
                Some(gradient) => Some(PreparedGradientIdentity::from_paint(gradient)?),
                None => None,
            },
        })
    }
}

impl PreparedGradientIdentity {
    fn from_paint(paint: &GradientPaint) -> Option<Self> {
        if paint.axis.iter().any(|value| !value.is_finite()) || paint.stops.is_empty() {
            return None;
        }
        let stops = paint
            .stops
            .iter()
            .map(|stop| {
                if stop
                    .color
                    .iter()
                    .any(|value| !value.is_finite() || !(0.0..=1.0).contains(value))
                    || stop.pos.iter().any(|value| !value.is_finite())
                {
                    return None;
                }
                Some(PreparedGradientStopIdentity {
                    color_bits: stop.color.map(f32::to_bits),
                    position_bits: stop.pos.map(f32::to_bits),
                })
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Self {
            kind: paint.kind,
            axis_bits: paint.axis.map(f32::to_bits),
            repeating: paint.repeating,
            stops: stops.into(),
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum PaintPayloadIdentity {
    #[default]
    None,
    Image(PreparedImageIdentity, Arc<[PreparedDrawRectIdentity]>),
    ImageWithShadows(
        PreparedImageIdentity,
        Arc<[PreparedShadowIdentity]>,
        Arc<[PreparedDrawRectIdentity]>,
    ),
    Gpu(PreparedGpuIdentity),
    Svg(PreparedSvgIdentity, Arc<[PreparedDrawRectIdentity]>),
    SvgWithShadows(
        PreparedSvgIdentity,
        Arc<[PreparedShadowIdentity]>,
        Arc<[PreparedDrawRectIdentity]>,
    ),
    PreparedShadows(
        Arc<[PreparedShadowIdentity]>,
        Arc<[PreparedDrawRectIdentity]>,
    ),
    PreparedTexts(Arc<[PreparedTextIdentity]>),
    PreparedRects(Arc<[PreparedDrawRectIdentity]>),
    TextSelection(TextSelectionPayloadIdentity),
    PreparedScrollbarOverlay(Arc<PreparedScrollbarOverlayIdentity>),
    InlineIfcDecorations(
        Arc<[PreparedShadowIdentity]>,
        Arc<[PreparedInlineIfcDecorationIdentity]>,
    ),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextPayloadNodeKind {
    TextRun,
    PreeditRun,
    LineBreak,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TextPayloadNodeIdentity {
    pub(crate) topology_index: usize,
    pub(crate) owner: NodeKey,
    pub(crate) parent: NodeKey,
    pub(crate) stable_id: u64,
    pub(crate) source_id: u64,
    pub(crate) kind: TextPayloadNodeKind,
    pub(crate) char_range: Range<usize>,
    pub(crate) backing_byte_range: Range<usize>,
    pub(crate) preedit_backing_byte_range: Option<Range<usize>>,
    pub(crate) preedit_caret_backing_byte: Option<usize>,
    pub(crate) text: Arc<str>,
    pub(crate) preedit_cursor: Option<(usize, usize)>,
}

/// Exact source record for a projection between two artifact coordinate spaces.
///
/// This is geometry/composite-side data. It validates how an already-recorded
/// host artifact projects into another artifact space, and must not be folded
/// into persistent texture keys or resident raster identity. Structural
/// equality includes the source origins and semantic revision; it is not a
/// derived spatial-transition comparison.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PaintArtifactSpaceTransition {
    from_origin_bits: [u32; 2],
    to_origin_bits: [u32; 2],
    semantic_revision: u64,
}

/// Bitwise result of evaluating `to - from` at `f32` on both spatial axes.
///
/// Reassociated source origins may produce this same value. Semantic revision
/// is deliberately absent and must be compared independently.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PaintArtifactSpaceTranslationBits([u32; 2]);

impl PaintArtifactSpaceTranslationBits {
    pub(crate) fn into_bits(self) -> [u32; 2] {
        self.0
    }
}

impl PaintArtifactSpaceTransition {
    pub(crate) fn from_bits(
        from_origin_bits: [u32; 2],
        to_origin_bits: [u32; 2],
        semantic_revision: u64,
    ) -> Option<Self> {
        let transition = Self {
            from_origin_bits,
            to_origin_bits,
            semantic_revision,
        };
        transition.is_canonical().then_some(transition)
    }

    pub(crate) fn is_canonical(self) -> bool {
        self.semantic_revision != 0
            && self
                .from_origin_bits
                .into_iter()
                .chain(self.to_origin_bits)
                .map(f32::from_bits)
                .all(f32::is_finite)
            && self.translation().is_some()
    }

    pub(crate) fn validate_for_owner(
        self,
        owner: NodeKey,
    ) -> Result<(), PaintArtifactContractRejection> {
        if self.semantic_revision == 0 {
            return Err(PaintArtifactContractRejection {
                owner,
                violation: PaintArtifactContractViolation::TransitionRevision,
            });
        }
        if self
            .from_origin_bits
            .into_iter()
            .chain(self.to_origin_bits)
            .map(f32::from_bits)
            .any(|value| !value.is_finite())
            || self.translation().is_none()
        {
            return Err(PaintArtifactContractRejection {
                owner,
                violation: PaintArtifactContractViolation::TransitionOrigin,
            });
        }
        Ok(())
    }

    pub(crate) fn validate_expected_for_owner(
        self,
        owner: NodeKey,
        expected: Self,
    ) -> Result<(), PaintArtifactContractRejection> {
        self.validate_for_owner(owner)?;
        expected.validate_for_owner(owner)?;
        if !self.source_bits_eq(expected) {
            return Err(PaintArtifactContractRejection {
                owner,
                violation: PaintArtifactContractViolation::TransitionSourceParity,
            });
        }
        self.semantic_revision_eq(expected)
            .then_some(())
            .ok_or(PaintArtifactContractRejection {
                owner,
                violation: PaintArtifactContractViolation::TransitionRevisionParity,
            })
    }

    #[cfg(test)]
    pub(crate) fn tamper_from_origin_for_test(mut self, axis: usize) -> Self {
        self.from_origin_bits[axis] ^= 1;
        self
    }

    #[cfg(test)]
    pub(crate) fn tamper_to_origin_for_test(mut self, axis: usize) -> Self {
        self.to_origin_bits[axis] ^= 1;
        self
    }

    #[cfg(test)]
    pub(crate) fn tamper_revision_for_test(mut self) -> Self {
        self.semantic_revision = self.semantic_revision.saturating_add(1);
        self
    }

    pub(crate) fn semantic_revision(self) -> u64 {
        self.semantic_revision
    }

    /// Exact parity of both recorded origin bit pairs. This is stricter than
    /// equality of their derived translation.
    pub(crate) fn source_bits_eq(self, other: Self) -> bool {
        self.from_origin_bits == other.from_origin_bits
            && self.to_origin_bits == other.to_origin_bits
    }

    pub(crate) fn semantic_revision_eq(self, other: Self) -> bool {
        self.semantic_revision == other.semantic_revision
    }

    fn derived_translation(self) -> Option<[f32; 2]> {
        let from = self.from_origin_bits.map(f32::from_bits);
        let to = self.to_origin_bits.map(f32::from_bits);
        let translation = [to[0] - from[0], to[1] - from[1]];
        translation
            .into_iter()
            .all(f32::is_finite)
            .then_some(translation)
    }

    /// Artifact-only seam for a later cross-source spatial differential.
    /// Production admission must continue comparing exact source bits and the
    /// semantic revision independently.
    pub(crate) fn translation_bits(self) -> Option<PaintArtifactSpaceTranslationBits> {
        self.derived_translation()
            .map(|translation| PaintArtifactSpaceTranslationBits(translation.map(f32::to_bits)))
    }

    pub(crate) fn translation(self) -> Option<[f32; 2]> {
        self.derived_translation()
    }

    pub(crate) fn project_bounds_bits(self, bounds_bits: [u32; 4]) -> Option<[u32; 4]> {
        let [delta_x, delta_y] = self.translation()?;
        let x = f32::from_bits(bounds_bits[0]) + delta_x;
        let y = f32::from_bits(bounds_bits[1]) + delta_y;
        let width = f32::from_bits(bounds_bits[2]);
        let height = f32::from_bits(bounds_bits[3]);
        [x, y, width, height]
            .into_iter()
            .all(f32::is_finite)
            .then_some([x, y, width, height].map(f32::to_bits))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TextPreeditPayloadIdentity {
    pub(crate) owner: NodeKey,
    pub(crate) content: Arc<str>,
    pub(crate) backing_text: Arc<str>,
    pub(crate) ime_preedit: Arc<str>,
    pub(crate) ime_preedit_cursor: Option<(usize, usize)>,
    pub(crate) cursor_char: usize,
    pub(crate) unified_ifc_source_revision: u64,
    pub(crate) artifact_space_transition: Option<PaintArtifactSpaceTransition>,
    pub(crate) generated_topology: Arc<[TextPayloadNodeIdentity]>,
    pub(crate) foreground_color_bits: [u32; 4],
    pub(crate) glyph_bounds_bits: [u32; 4],
    pub(crate) underline_bounds_bits: [u32; 4],
    pub(crate) glyph_identity: PaintPayloadIdentity,
    pub(crate) underline_identity: PaintPayloadIdentity,
}

impl TextPreeditPayloadIdentity {
    pub(crate) fn is_canonical(&self) -> bool {
        if self.ime_preedit.is_empty()
            || self
                .foreground_color_bits
                .map(f32::from_bits)
                .into_iter()
                .any(|channel| !channel.is_finite() || !(0.0..=1.0).contains(&channel))
            || self.cursor_char > self.content.chars().count()
            || !self.ime_preedit_cursor.is_none_or(|(start, end)| {
                start <= end
                    && end <= self.ime_preedit.len()
                    && self.ime_preedit.is_char_boundary(start)
                    && self.ime_preedit.is_char_boundary(end)
            })
            || self.unified_ifc_source_revision == 0
            || !self.artifact_space_transition.is_some_and(|transition| {
                transition.is_canonical()
                    && transition.semantic_revision() == self.unified_ifc_source_revision
            })
            || self.generated_topology.is_empty()
            || !preedit_glyph_identity_is_exact(
                &self.glyph_identity,
                self.glyph_bounds_bits,
                self.foreground_color_bits,
            )
            || !preedit_underline_identity_is_exact(
                &self.underline_identity,
                self.underline_bounds_bits,
                self.foreground_color_bits,
            )
        {
            return false;
        }
        let insert_byte = self
            .content
            .char_indices()
            .nth(self.cursor_char)
            .map(|(index, _)| index)
            .unwrap_or(self.content.len());
        let mut expected_backing =
            String::with_capacity(self.content.len().saturating_add(self.ime_preedit.len()));
        expected_backing.push_str(&self.content[..insert_byte]);
        expected_backing.push_str(&self.ime_preedit);
        expected_backing.push_str(&self.content[insert_byte..]);
        if expected_backing.as_str() != self.backing_text.as_ref() {
            return false;
        }
        let expected_preedit_caret = self
            .ime_preedit_cursor
            .map(|(_, end)| end)
            .unwrap_or(self.ime_preedit.len());
        let mut backing_cursor = 0usize;
        let mut committed_chars = 0usize;
        let mut committed = String::with_capacity(self.content.len());
        let mut preedit_count = 0usize;
        for (index, entry) in self.generated_topology.iter().enumerate() {
            if entry.topology_index != index
                || entry.stable_id == 0
                || entry.parent != self.owner
                || entry.owner == self.owner
                || entry.source_id != entry.stable_id
                || entry.char_range.start > entry.char_range.end
                || entry.backing_byte_range.start != backing_cursor
                || entry.backing_byte_range.start > entry.backing_byte_range.end
                || entry.backing_byte_range.end > self.backing_text.len()
                || !self
                    .backing_text
                    .is_char_boundary(entry.backing_byte_range.start)
                || !self
                    .backing_text
                    .is_char_boundary(entry.backing_byte_range.end)
                || self.generated_topology[..index]
                    .iter()
                    .any(|previous| previous.owner == entry.owner)
            {
                return false;
            }
            let backing = &self.backing_text[entry.backing_byte_range.clone()];
            match entry.kind {
                TextPayloadNodeKind::PreeditRun => {
                    preedit_count += 1;
                    let expected_range = entry.backing_byte_range.clone();
                    if preedit_count != 1
                        || entry.char_range != (self.cursor_char..self.cursor_char)
                        || entry.text.as_ref() != self.ime_preedit.as_ref()
                        || backing != self.ime_preedit.as_ref()
                        || entry.preedit_cursor != self.ime_preedit_cursor
                        || entry.preedit_backing_byte_range != Some(expected_range.clone())
                        || entry.preedit_caret_backing_byte
                            != Some(expected_range.start + expected_preedit_caret)
                    {
                        return false;
                    }
                }
                TextPayloadNodeKind::TextRun => {
                    let char_len = entry.text.chars().count();
                    if entry.char_range
                        != (committed_chars..committed_chars.saturating_add(char_len))
                        || backing != entry.text.as_ref()
                        || entry.preedit_cursor.is_some()
                        || entry.preedit_backing_byte_range.is_some()
                        || entry.preedit_caret_backing_byte.is_some()
                    {
                        return false;
                    }
                    committed_chars = entry.char_range.end;
                    committed.push_str(&entry.text);
                }
                TextPayloadNodeKind::LineBreak => {
                    if entry.char_range != (committed_chars..committed_chars.saturating_add(1))
                        || backing != "\n"
                        || !entry.text.is_empty()
                        || entry.preedit_cursor.is_some()
                        || entry.preedit_backing_byte_range.is_some()
                        || entry.preedit_caret_backing_byte.is_some()
                    {
                        return false;
                    }
                    committed_chars = entry.char_range.end;
                    committed.push('\n');
                }
            }
            backing_cursor = entry.backing_byte_range.end;
        }
        preedit_count == 1
            && backing_cursor == self.backing_text.len()
            && committed_chars == self.content.chars().count()
            && committed == self.content.as_ref()
    }
}

pub(crate) fn preedit_glyph_identity_is_exact(
    identity: &PaintPayloadIdentity,
    bounds_bits: [u32; 4],
    foreground_color_bits: [u32; 4],
) -> bool {
    let PaintPayloadIdentity::PreparedTexts(texts) = identity else {
        return false;
    };
    let [text] = texts.as_ref() else {
        return false;
    };
    if text.glyphs.is_empty()
        || text.fragments.is_empty()
        || text.glyphs.iter().any(|glyph| {
            glyph.color_bits != foreground_color_bits || glyph.opacity_bits != 1.0_f32.to_bits()
        })
    {
        return false;
    }
    let mut left = f32::INFINITY;
    let mut top = f32::INFINITY;
    let mut right = f32::NEG_INFINITY;
    let mut bottom = f32::NEG_INFINITY;
    for fragment in text.fragments.iter() {
        let [x, y] = fragment.origin_bits.map(f32::from_bits);
        let [width, height] = fragment.size_bits.map(f32::from_bits);
        left = left.min(x);
        top = top.min(y);
        right = right.max(x + width);
        bottom = bottom.max(y + height);
    }
    [left, top, right - left, bottom - top].map(f32::to_bits) == bounds_bits
}

pub(crate) fn preedit_underline_identity_is_exact(
    identity: &PaintPayloadIdentity,
    bounds_bits: [u32; 4],
    foreground_color_bits: [u32; 4],
) -> bool {
    let PaintPayloadIdentity::PreparedRects(rects) = identity else {
        return false;
    };
    if rects.is_empty() {
        return false;
    }
    let mut left = f32::INFINITY;
    let mut top = f32::INFINITY;
    let mut right = f32::NEG_INFINITY;
    let mut bottom = f32::NEG_INFINITY;
    for rect in rects.iter() {
        if rect.mode != RectRenderMode::FillOnly
            || rect.params.fill_color_bits != foreground_color_bits
            || rect.params.opacity_bits != 1.0_f32.to_bits()
            || rect.params.size_bits[1] != 1.0_f32.to_bits()
        {
            return false;
        }
        let [x, y] = rect.params.position_bits.map(f32::from_bits);
        let [width, height] = rect.params.size_bits.map(f32::from_bits);
        left = left.min(x);
        top = top.min(y);
        right = right.max(x + width);
        bottom = bottom.max(y + height);
    }
    [left, top, right - left, bottom - top].map(f32::to_bits) == bounds_bits
}

impl PaintPayloadIdentity {
    /// Rebuilds the same semantic payload variant from already-localized ops.
    /// Spatial bits may change; asset/source identity and typed payload shape
    /// must remain the variant and cardinality frozen by `self`.
    pub(crate) fn rebuild_from_localized_ops(&self, ops: &[PaintOp]) -> Option<Self> {
        match self {
            Self::Gpu(expected) => {
                let [PaintOp::PreparedGpu(op)] = ops else {
                    return None;
                };
                let identity = op.identity()?;
                (identity.source == expected.source).then_some(Self::Gpu(identity))
            }

            Self::None => ops.is_empty().then_some(Self::None),
            Self::PreparedRects(expected) => {
                let rects = ops
                    .iter()
                    .map(|op| match op {
                        PaintOp::DrawRect(rect) => Some(rect),
                        _ => None,
                    })
                    .collect::<Option<Vec<_>>>()?;
                if rects.len() != expected.len() {
                    return None;
                }
                Self::prepared_rects(rects)
            }
            Self::TextSelection(selection) => {
                let rects = ops
                    .iter()
                    .map(|op| match op {
                        PaintOp::DrawRect(rect) => Some(rect),
                        _ => None,
                    })
                    .collect::<Option<Vec<_>>>()?;
                (rects.len() == selection.rects.len()).then(|| {
                    Self::prepared_text_selection(
                        selection.start_char,
                        selection.end_char,
                        selection.color_rgba_bits,
                        rects,
                    )
                })?
            }
            Self::PreparedShadows(expected_shadows, expected_rects) => {
                let (shadows, rects) = ops.split_at(expected_shadows.len());
                let shadows = shadows
                    .iter()
                    .map(|op| match op {
                        PaintOp::PreparedShadow(shadow) => Some(shadow),
                        _ => None,
                    })
                    .collect::<Option<Vec<_>>>()?;
                let rects = rects
                    .iter()
                    .map(|op| match op {
                        PaintOp::DrawRect(rect) => Some(rect),
                        _ => None,
                    })
                    .collect::<Option<Vec<_>>>()?;
                if rects.len() != expected_rects.len() {
                    return None;
                }
                Self::prepared_shadows_with_decoration(shadows, rects)
            }
            Self::PreparedTexts(expected) => {
                let texts = ops
                    .iter()
                    .map(|op| match op {
                        PaintOp::PreparedText(text) => Some(text),
                        _ => None,
                    })
                    .collect::<Option<Vec<_>>>()?;
                (texts.len() == expected.len()).then(|| Self::prepared_texts(texts))
            }
            Self::PreparedScrollbarOverlay(_) => match ops {
                [PaintOp::PreparedScrollbarOverlay(overlay)] => {
                    Some(Self::prepared_scrollbar_overlay(overlay))
                }
                _ => None,
            },
            Self::InlineIfcDecorations(expected_shadows, expected_decorations) => {
                let (shadows, decorations) = ops.split_at(expected_shadows.len());
                let shadows = shadows
                    .iter()
                    .map(|op| match op {
                        PaintOp::PreparedShadow(shadow) => Some(shadow),
                        _ => None,
                    })
                    .collect::<Option<Vec<_>>>()?;
                let decorations = decorations
                    .iter()
                    .map(|op| match op {
                        PaintOp::PreparedInlineIfcDecoration(decoration) => {
                            Some(decoration.as_ref())
                        }
                        _ => None,
                    })
                    .collect::<Option<Vec<_>>>()?;
                (decorations.len() == expected_decorations.len())
                    .then(|| Self::inline_ifc_decorations_with_shadows(shadows, decorations))
            }
            Self::Image(_, expected_rects) | Self::ImageWithShadows(_, _, expected_rects) => {
                let (image, prefix) = ops.split_last()?;
                let PaintOp::PreparedImage(image) = image else {
                    return None;
                };
                let shadow_count = match self {
                    Self::ImageWithShadows(_, shadows, _) => shadows.len(),
                    Self::Image(_, _) => 0,
                    _ => unreachable!(),
                };
                let (shadows, rects) = prefix.split_at(shadow_count);
                let shadows = shadows
                    .iter()
                    .map(|op| match op {
                        PaintOp::PreparedShadow(shadow) => Some(shadow),
                        _ => None,
                    })
                    .collect::<Option<Vec<_>>>()?;
                let rects = rects
                    .iter()
                    .map(|op| match op {
                        PaintOp::DrawRect(rect) => Some(rect),
                        _ => None,
                    })
                    .collect::<Option<Vec<_>>>()?;
                (rects.len() == expected_rects.len()).then(|| {
                    Self::image_with_shadows_and_decoration(
                        PreparedImageIdentity::from_op(image),
                        shadows,
                        rects,
                    )
                })?
            }
            Self::Svg(_, expected_rects) | Self::SvgWithShadows(_, _, expected_rects) => {
                let (svg, prefix) = ops.split_last()?;
                let PaintOp::PreparedSvg(svg) = svg else {
                    return None;
                };
                let shadow_count = match self {
                    Self::SvgWithShadows(_, shadows, _) => shadows.len(),
                    Self::Svg(_, _) => 0,
                    _ => unreachable!(),
                };
                let (shadows, rects) = prefix.split_at(shadow_count);
                let shadows = shadows
                    .iter()
                    .map(|op| match op {
                        PaintOp::PreparedShadow(shadow) => Some(shadow),
                        _ => None,
                    })
                    .collect::<Option<Vec<_>>>()?;
                let rects = rects
                    .iter()
                    .map(|op| match op {
                        PaintOp::DrawRect(rect) => Some(rect),
                        _ => None,
                    })
                    .collect::<Option<Vec<_>>>()?;
                (rects.len() == expected_rects.len()).then(|| {
                    Self::svg_with_shadows_and_decoration(
                        PreparedSvgIdentity::from_op(svg)?,
                        shadows,
                        rects,
                    )
                })?
            }
        }
    }

    pub(crate) fn text_selection_identity(&self) -> Option<TextSelectionPayloadIdentity> {
        let Self::TextSelection(seal) = self else {
            return None;
        };
        Some(seal.clone())
    }

    pub(crate) fn matches_text_selection_source(
        &self,
        start_char: usize,
        end_char: usize,
        color_rgba_bits: [u32; 4],
    ) -> bool {
        matches!(self, Self::TextSelection(selection)
            if selection.matches_source(start_char, end_char, color_rgba_bits))
    }

    pub(crate) fn matches_exact_text_selection_ops<'a>(
        &self,
        rects: impl IntoIterator<Item = &'a DrawRectOp>,
    ) -> bool {
        let Self::TextSelection(selection) = self else {
            return false;
        };
        Self::prepared_text_selection(
            selection.start_char,
            selection.end_char,
            selection.color_rgba_bits,
            rects,
        )
        .as_ref()
            == Some(self)
    }

    pub(crate) fn exact_fill_rect_ops(&self) -> Option<Vec<DrawRectOp>> {
        let Self::PreparedRects(rects) = self else {
            return None;
        };
        let ops = rects
            .iter()
            .map(|rect| {
                (rect.mode == RectRenderMode::FillOnly
                    && rect.params.border_width_bits == [0.0_f32.to_bits(); 4]
                    && rect.params.border_radius_bits == [[0.0_f32.to_bits(); 2]; 4]
                    && rect.params.border_color_bits == [0.0_f32.to_bits(); 4]
                    && rect.params.border_side_color_bits == [[0.0_f32.to_bits(); 4]; 4]
                    && !rect.params.use_border_side_colors
                    && rect.params.depth_bits == 0.0_f32.to_bits()
                    && rect.params.gradient.is_none()
                    && rect.params.border_gradient.is_none())
                .then(|| DrawRectOp {
                    params: RectPassParams {
                        position: rect.params.position_bits.map(f32::from_bits),
                        size: rect.params.size_bits.map(f32::from_bits),
                        fill_color: rect.params.fill_color_bits.map(f32::from_bits),
                        opacity: f32::from_bits(rect.params.opacity_bits),
                        ..Default::default()
                    },
                    mode: rect.mode,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        (Self::prepared_rects(ops.iter()).as_ref() == Some(self)).then_some(ops)
    }

    pub(crate) fn prepared_text_selection<'a>(
        start_char: usize,
        end_char: usize,
        color_rgba_bits: [u32; 4],
        rects: impl IntoIterator<Item = &'a DrawRectOp>,
    ) -> Option<Self> {
        let rects = Self::draw_rect_identities(rects)?;
        let selection = TextSelectionPayloadIdentity {
            start_char,
            end_char,
            color_rgba_bits,
            rects,
        };
        selection
            .is_canonical()
            .then_some(Self::TextSelection(selection))
    }

    pub(crate) fn matches_exact_text_selection(
        &self,
        start_char: usize,
        end_char: usize,
        color_rgba_bits: [u32; 4],
        op_count: usize,
        bounds_bits: [u32; 4],
    ) -> bool {
        let Self::TextSelection(seal) = self else {
            return false;
        };
        if !seal.matches_source(start_char, end_char, color_rgba_bits)
            || seal.rects.len() != op_count
        {
            return false;
        }
        Self::exact_fill_rect_bounds_bits(&seal.rects, color_rgba_bits) == Some(bounds_bits)
    }

    pub(crate) fn matches_exact_fill_rects(
        &self,
        op_count: usize,
        fill_color_bits: [u32; 4],
        bounds_bits: [u32; 4],
    ) -> bool {
        let Self::PreparedRects(rects) = self else {
            return false;
        };
        if rects.len() != op_count {
            return false;
        }
        Self::exact_fill_rect_bounds_bits(rects, fill_color_bits) == Some(bounds_bits)
    }

    fn exact_fill_rect_bounds_bits(
        rects: &[PreparedDrawRectIdentity],
        fill_color_bits: [u32; 4],
    ) -> Option<[u32; 4]> {
        if rects.is_empty()
            || rects.iter().any(|rect| {
                rect.mode != RectRenderMode::FillOnly
                    || rect.params.fill_color_bits != fill_color_bits
                    || rect.params.opacity_bits != 1.0_f32.to_bits()
            })
        {
            return None;
        }
        let mut left = f32::INFINITY;
        let mut top = f32::INFINITY;
        let mut right = f32::NEG_INFINITY;
        let mut bottom = f32::NEG_INFINITY;
        for rect in rects.iter() {
            let [x, y] = rect.params.position_bits.map(f32::from_bits);
            let [width, height] = rect.params.size_bits.map(f32::from_bits);
            left = left.min(x);
            top = top.min(y);
            right = right.max(x + width);
            bottom = bottom.max(y + height);
        }
        [left, top, right, bottom]
            .into_iter()
            .all(f32::is_finite)
            .then(|| [left, top, right - left, bottom - top].map(f32::to_bits))
    }

    /// Canonical ordered identity for generic rectangle-only paint phases.
    /// Cardinality and role-specific grammar remain compiler authority.
    pub(crate) fn prepared_rects<'a>(
        rects: impl IntoIterator<Item = &'a DrawRectOp>,
    ) -> Option<Self> {
        Some(Self::PreparedRects(Self::draw_rect_identities(rects)?))
    }

    pub(crate) fn prepared_shadows<'a>(
        shadows: impl IntoIterator<Item = &'a PreparedShadowOp>,
    ) -> Self {
        Self::PreparedShadows(
            shadows
                .into_iter()
                .map(PreparedShadowOp::frozen_identity)
                .collect::<Vec<_>>()
                .into(),
            Arc::from([]),
        )
    }

    pub(crate) fn prepared_scrollbar_overlay(op: &PreparedScrollbarOverlayOp) -> Self {
        Self::PreparedScrollbarOverlay(Arc::new(op.frozen_identity()))
    }

    pub(crate) fn prepared_shadows_with_decoration<'a, 'b>(
        shadows: impl IntoIterator<Item = &'a PreparedShadowOp>,
        decoration: impl IntoIterator<Item = &'b DrawRectOp>,
    ) -> Option<Self> {
        Some(Self::PreparedShadows(
            shadows
                .into_iter()
                .map(PreparedShadowOp::frozen_identity)
                .collect::<Vec<_>>()
                .into(),
            Self::draw_rect_identities(decoration)?,
        ))
    }

    pub(crate) fn image_with_decoration<'a>(
        image: PreparedImageIdentity,
        decoration: impl IntoIterator<Item = &'a DrawRectOp>,
    ) -> Option<Self> {
        Some(Self::Image(image, Self::draw_rect_identities(decoration)?))
    }

    pub(crate) fn image_with_shadows_and_decoration<'a, 'b>(
        image: PreparedImageIdentity,
        shadows: impl IntoIterator<Item = &'a PreparedShadowOp>,
        decoration: impl IntoIterator<Item = &'b DrawRectOp>,
    ) -> Option<Self> {
        let shadows = shadows
            .into_iter()
            .map(PreparedShadowOp::frozen_identity)
            .collect::<Vec<_>>();
        let decoration = Self::draw_rect_identities(decoration)?;
        if shadows.is_empty() {
            Some(Self::Image(image, decoration))
        } else {
            Some(Self::ImageWithShadows(image, shadows.into(), decoration))
        }
    }

    pub(crate) fn svg_with_decoration<'a>(
        svg: PreparedSvgIdentity,
        decoration: impl IntoIterator<Item = &'a DrawRectOp>,
    ) -> Option<Self> {
        Some(Self::Svg(svg, Self::draw_rect_identities(decoration)?))
    }

    pub(crate) fn svg_with_shadows_and_decoration<'a, 'b>(
        svg: PreparedSvgIdentity,
        shadows: impl IntoIterator<Item = &'a PreparedShadowOp>,
        decoration: impl IntoIterator<Item = &'b DrawRectOp>,
    ) -> Option<Self> {
        let shadows = shadows
            .into_iter()
            .map(PreparedShadowOp::frozen_identity)
            .collect::<Vec<_>>();
        let decoration = Self::draw_rect_identities(decoration)?;
        if shadows.is_empty() {
            Some(Self::Svg(svg, decoration))
        } else {
            Some(Self::SvgWithShadows(svg, shadows.into(), decoration))
        }
    }

    fn draw_rect_identities<'a>(
        decoration: impl IntoIterator<Item = &'a DrawRectOp>,
    ) -> Option<Arc<[PreparedDrawRectIdentity]>> {
        Some(
            decoration
                .into_iter()
                .map(PreparedDrawRectIdentity::from_op)
                .collect::<Option<Vec<_>>>()?
                .into(),
        )
    }

    pub(crate) fn prepared_texts<'a>(texts: impl IntoIterator<Item = &'a PreparedTextOp>) -> Self {
        Self::PreparedTexts(
            texts
                .into_iter()
                .map(PreparedTextOp::frozen_identity)
                .collect::<Vec<_>>()
                .into(),
        )
    }

    pub(crate) fn inline_ifc_decorations<'a>(
        decorations: impl IntoIterator<Item = &'a PreparedInlineIfcDecorationOp>,
    ) -> Self {
        Self::inline_ifc_decorations_with_shadows(std::iter::empty(), decorations)
    }

    pub(crate) fn inline_ifc_decorations_with_shadows<'a, 'b>(
        shadows: impl IntoIterator<Item = &'a PreparedShadowOp>,
        decorations: impl IntoIterator<Item = &'b PreparedInlineIfcDecorationOp>,
    ) -> Self {
        Self::InlineIfcDecorations(
            shadows
                .into_iter()
                .map(PreparedShadowOp::frozen_identity)
                .collect::<Vec<_>>()
                .into(),
            decorations
                .into_iter()
                .map(PreparedInlineIfcDecorationOp::frozen_identity)
                .collect::<Vec<_>>()
                .into(),
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PreparedTextIdentity {
    scale_factor_bits: u32,
    glyphs: Arc<[PreparedTextGlyphIdentity]>,
    fragments: Arc<[PreparedTextFragmentIdentity]>,
    scissor_rect: Option<[u32; 4]>,
    stencil_clip_id: Option<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PreparedTextGlyphIdentity {
    glyph_id: u32,
    font_size_bits: u32,
    font_data_id: u64,
    font_index: u32,
    normalized_coords_hash: u64,
    local_pos_bits: [u32; 2],
    color_bits: [u32; 4],
    opacity_bits: u32,
    fragment_index: u32,
    final_paint_pos_bits: [u32; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PreparedTextFragmentIdentity {
    origin_bits: [u32; 2],
    size_bits: [u32; 2],
}

impl PreparedTextFragmentIdentity {
    fn from_fragment(fragment: &TextPassPreparedFragment) -> Option<Self> {
        if fragment.origin.iter().any(|value| !value.is_finite())
            || fragment
                .size
                .iter()
                .any(|value| !value.is_finite() || *value <= 0.0)
        {
            return None;
        }
        Some(PreparedTextFragmentIdentity {
            origin_bits: fragment.origin.map(f32::to_bits),
            size_bits: fragment.size.map(f32::to_bits),
        })
    }
}

impl PreparedTextGlyphIdentity {
    fn from_glyph(
        glyph: &TextPassPreparedStagingGlyphInput,
        fragments: &[TextPassPreparedFragment],
    ) -> Option<Self> {
        let font_data = glyph.raster.font_data.as_ref()?;
        if font_data.data.id() != glyph.raster.font_data_id
            || font_data.index != glyph.raster.font_index
            || glyph.raster.glyph_id > u16::MAX as u32
            || !glyph.raster.font_size.is_finite()
            || glyph.raster.font_size <= 0.0
            || glyph.paint.fragment_index as usize >= fragments.len()
            || glyph.paint.local_pos.iter().any(|value| !value.is_finite())
            || glyph
                .paint
                .color
                .iter()
                .any(|value| !value.is_finite() || !(0.0..=1.0).contains(value))
            || !glyph.paint.opacity.is_finite()
            || !(0.0..=1.0).contains(&glyph.paint.opacity)
            || glyph.final_paint_pos.iter().any(|value| !value.is_finite())
        {
            return None;
        }
        let fragment = fragments[glyph.paint.fragment_index as usize];
        let expected_final = [
            fragment.origin[0] + glyph.paint.local_pos[0],
            fragment.origin[1] + glyph.paint.local_pos[1],
        ];
        if glyph.final_paint_pos.map(f32::to_bits) != expected_final.map(f32::to_bits) {
            return None;
        }
        Some(PreparedTextGlyphIdentity {
            glyph_id: glyph.raster.glyph_id,
            font_size_bits: glyph.raster.font_size.to_bits(),
            font_data_id: glyph.raster.font_data_id,
            font_index: glyph.raster.font_index,
            normalized_coords_hash: glyph.raster.normalized_coords_hash,
            local_pos_bits: glyph.paint.local_pos.map(f32::to_bits),
            color_bits: glyph.paint.color.map(f32::to_bits),
            opacity_bits: glyph.paint.opacity.to_bits(),
            fragment_index: glyph.paint.fragment_index,
            final_paint_pos_bits: glyph.final_paint_pos.map(f32::to_bits),
        })
    }
}

impl PreparedTextIdentity {
    fn valid_header(
        scale_factor: f32,
        glyph_count: usize,
        fragments: &[TextPassPreparedFragment],
        scissor_rect: Option<[u32; 4]>,
    ) -> bool {
        scale_factor.is_finite()
            && scale_factor > 0.0
            && glyph_count != 0
            && !fragments.is_empty()
            && !scissor_rect.is_some_and(|[_, _, width, height]| width == 0 || height == 0)
    }

    fn from_params(params: &TextPassPreparedParams) -> Option<Self> {
        if !Self::valid_header(
            params.staging_input.scale_factor,
            params.staging_input.glyphs.len(),
            &params.fragments,
            params.scissor_rect,
        ) {
            return None;
        }
        let fragments = params
            .fragments
            .iter()
            .map(PreparedTextFragmentIdentity::from_fragment)
            .collect::<Option<Vec<_>>>()?;
        // The input length is exact. Avoid repeated growth through an
        // Option-collect adapter when preparing a long text run.
        let mut glyphs = Vec::with_capacity(params.staging_input.glyphs.len());
        for glyph in &params.staging_input.glyphs {
            glyphs.push(PreparedTextGlyphIdentity::from_glyph(
                glyph,
                &params.fragments,
            )?);
        }
        Some(Self {
            scale_factor_bits: params.staging_input.scale_factor.to_bits(),
            glyphs: glyphs.into(),
            fragments: fragments.into(),
            scissor_rect: params.scissor_rect,
            stencil_clip_id: params.stencil_clip_id,
        })
    }

    fn matches_params(&self, params: &TextPassPreparedParams) -> bool {
        Self::valid_header(
            params.staging_input.scale_factor,
            params.staging_input.glyphs.len(),
            &params.fragments,
            params.scissor_rect,
        ) && self.scale_factor_bits == params.staging_input.scale_factor.to_bits()
            && self.scissor_rect == params.scissor_rect
            && self.stencil_clip_id == params.stencil_clip_id
            && self.fragments.len() == params.fragments.len()
            && self.glyphs.len() == params.staging_input.glyphs.len()
            && self
                .fragments
                .iter()
                .zip(&params.fragments)
                .all(|(expected, actual)| {
                    PreparedTextFragmentIdentity::from_fragment(actual).as_ref() == Some(expected)
                })
            && self
                .glyphs
                .iter()
                .zip(&params.staging_input.glyphs)
                .all(|(expected, actual)| {
                    PreparedTextGlyphIdentity::from_glyph(actual, &params.fragments).as_ref()
                        == Some(expected)
                })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PreparedImageIdentity {
    pub(crate) sampled_texture_id: SampledTextureId,
    pub(crate) generation: u64,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) format: wgpu::TextureFormat,
    pub(crate) alpha_mode: SampledTextureAlphaMode,
    pub(crate) sampling: ImageSampling,
    pub(crate) pixel_len: usize,
    /// Identity of the immutable, frame-frozen pixel allocation. Together
    /// with generation, dimensions, and length this detects upload Arc
    /// replacement between metadata and full recording.
    pub(crate) pixel_ptr: usize,
    pub(crate) bounds_bits: [u32; 4],
    pub(crate) uv_bounds_bits: Option<[u32; 4]>,
    pub(crate) opacity_bits: u32,
    pub(crate) source_is_premultiplied: bool,
}

impl PreparedImageIdentity {
    pub(crate) fn from_op(op: &PreparedImageOp) -> Self {
        Self {
            sampled_texture_id: op.upload.id,
            generation: op.upload.generation,
            width: op.upload.width,
            height: op.upload.height,
            format: op.upload.format,
            alpha_mode: op.upload.alpha_mode,
            sampling: op.upload.sampling,
            pixel_len: op.upload.pixels.len(),
            pixel_ptr: op.upload.pixels.as_ptr() as usize,
            bounds_bits: op.params.bounds.map(f32::to_bits),
            uv_bounds_bits: op.params.uv_bounds.map(|bounds| bounds.map(f32::to_bits)),
            opacity_bits: op.params.opacity.to_bits(),
            source_is_premultiplied: op.params.source_is_premultiplied,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PreparedSvgIdentity {
    pub(crate) svg_raster_asset_id: SvgRasterAssetId,
    pub(crate) generation: u64,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) format: wgpu::TextureFormat,
    pub(crate) alpha_mode: SampledTextureAlphaMode,
    pub(crate) sampling: ImageSampling,
    pub(crate) pixel_len: usize,
    /// Identity of the immutable, frame-frozen raster allocation.
    pub(crate) pixel_ptr: usize,
    pub(crate) bounds_bits: [u32; 4],
    pub(crate) uv_bounds_bits: Option<[u32; 4]>,
    pub(crate) opacity_bits: u32,
    pub(crate) source_is_premultiplied: bool,
}

impl PreparedSvgIdentity {
    pub(crate) fn from_op(op: &PreparedSvgOp) -> Option<Self> {
        let SampledTextureId::SvgRaster(svg_raster_asset_id) = op.upload.id else {
            return None;
        };
        Some(Self {
            svg_raster_asset_id,
            generation: op.upload.generation,
            width: op.upload.width,
            height: op.upload.height,
            format: op.upload.format,
            alpha_mode: op.upload.alpha_mode,
            sampling: op.upload.sampling,
            pixel_len: op.upload.pixels.len(),
            pixel_ptr: op.upload.pixels.as_ptr() as usize,
            bounds_bits: op.params.bounds.map(f32::to_bits),
            uv_bounds_bits: op.params.uv_bounds.map(|bounds| bounds.map(f32::to_bits)),
            opacity_bits: op.params.opacity.to_bits(),
            source_is_premultiplied: op.params.source_is_premultiplied,
        })
    }
}

#[cfg(test)]
mod consumed_ancestor_property_tests;
#[cfg(test)]
mod paint_artifact_contract_violation_tests;
#[cfg(test)]
mod paint_artifact_space_transition_tests;
#[cfg(test)]
mod text_selection_payload_identity_tests;

#[cfg(test)]
mod prepared_text_identity_tests;

#[derive(Clone, Debug)]
pub(crate) struct PreparedGpuOp {
    pub(crate) params: TextureCompositeParams,
    pub(crate) source: crate::view::gpu_paint::GpuPaintSource,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PreparedGpuIdentity {
    source: crate::view::gpu_paint::GpuPaintSource,
    bounds: [u32; 4],
    opacity: u32,
}
impl PreparedGpuOp {
    pub(crate) fn identity(&self) -> Option<PreparedGpuIdentity> {
        let p = &self.params;
        (p.bounds.iter().all(|v| v.is_finite())
            && p.bounds[2] > 0.0
            && p.bounds[3] > 0.0
            && p.opacity.is_finite()
            && (0.0..=1.0).contains(&p.opacity)
            && p.source_is_premultiplied
            && !p.use_mask
            && p.quad_positions.is_none()
            && p.uv_bounds.is_none()
            && p.mask_uv_bounds.is_none()
            && p.scissor_rect.is_none())
        .then(|| PreparedGpuIdentity {
            source: self.source.clone(),
            bounds: p.bounds.map(f32::to_bits),
            opacity: p.opacity.to_bits(),
        })
    }
}
