#![allow(dead_code)] // Staged artifact authority is exercised by tests until viewport rollout.

use std::ops::Range;
use std::sync::Arc;

use slotmap::Key;

use crate::view::ImageSampling;
use crate::view::base_component::{Rect, ScrollbarOverlayWitness, ScrollbarPaintStateWitness};
use crate::view::compositor::property_tree::PropertyTreeState;
use crate::view::compositor::property_tree::{
    ClipBehavior, ClipNodeId, ClipNodeRole, ClipNodeSnapshot, EffectNodeId, EffectNodeSnapshot,
    ScrollNodeId, ScrollNodeSnapshot, TransformNodeId,
};
use crate::view::node_arena::NodeKey;
use crate::view::render_pass::draw_rect_pass::{
    GradientKindGpu, GradientPaint, RectPassParams, RectRenderMode,
};
use crate::view::render_pass::shadow_module::{ShadowMesh, ShadowParams};
use crate::view::render_pass::text_pass::TextPassPreparedParams;
use crate::view::render_pass::texture_composite_pass::TextureCompositeParams;
use crate::view::sampled_texture::{
    SampledTextureAlphaMode, SampledTextureId, SampledTextureUpload, SvgRasterAssetId,
};

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
    /// Owner-scoped proof that this coverage invocation belongs to one
    /// canonically planned transform surface. The normal frame recorder never
    /// installs this witness; the surface recorder clears and rebinds it for
    /// every canonical traversal owner.
    pub(crate) transform_surface: Option<PaintTransformSurfaceWitness>,
    /// Recorder-owned authority for the one exact M10E1A root/child path.
    /// Coverage clears and rebinds this after every component hook.
    pub(crate) baked_scroll_host: Option<PaintBakedScrollHostWitness>,
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
    /// Exact bounded `S0 -> S1 -> leaf` content projection.  This is kept
    /// separate from the generic ancestor stack: two scroll boundaries may
    /// not be smuggled through the single-scroll stack invariant.
    pub(crate) nested_scroll_content: Option<PaintNestedScrollContentWitness>,
    /// Inner-host half of the bounded nested scene.  It projects S0/C0 from
    /// H1/O1 self paint while preserving S1/C1 on the receiver edge.
    pub(crate) nested_scroll_host: Option<PaintNestedScrollContentWitness>,
    /// C1-only proof which consumes one outer Scroll/ContentsClip pair while
    /// preserving the exact TextArea-local ContentsClip in detached geometry.
    pub(crate) scroll_text_area_subtree: Option<PaintScrollTextAreaSubtreeWitness>,
    /// Host-side half of the same C1 proof.  Unlike
    /// `scroll_text_area_subtree`, this only authorizes the wrapper's exact
    /// child clip and never projects live outer properties.
    pub(crate) baked_scroll_text_area_subtree: Option<PaintScrollTextAreaSubtreeWitness>,
    /// C3a-only projection of the same outer scroll/clip pair.  It is a
    /// separate capability because its source grammar is non-Copy and is
    /// revalidated by the closed recorder before and after coverage.
    pub(crate) scroll_atomic_projection_text_area_subtree:
        Option<PaintScrollAtomicProjectionTextAreaRecorderWitness>,
    pub(crate) baked_scroll_atomic_projection_text_area_subtree:
        Option<PaintScrollAtomicProjectionTextAreaRecorderWitness>,
    /// C2b/C2c resident-base authority. It uses the same exact property
    /// projection as C1/C2a while authorizing source-level caret suppression.
    pub(crate) scroll_interactive_text_area_subtree:
        Option<PaintScrollInteractiveTextAreaSubtreeWitness>,
    pub(crate) baked_scroll_interactive_text_area_subtree:
        Option<PaintScrollInteractiveTextAreaSubtreeWitness>,
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
                    && witness.boundary_root == owner
                    && witness.target_owner == owner
        )
    }

    pub(crate) fn baked_scroll_host_snapshot_for_root(
        self,
        stable_id: u64,
    ) -> Option<crate::view::compositor::property_tree::ScrollNodeSnapshot> {
        self.authorizes_baked_scroll_host_root(stable_id).then(|| {
            self.baked_scroll_host
                .expect("authority requires witness")
                .scroll
        })
    }

    pub(crate) fn without_text_area_selection(mut self) -> Self {
        self.text_area_selection = None;
        self
    }

    pub(crate) fn without_text_area_preedit(mut self) -> Self {
        self.text_area_preedit = None;
        self
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
        if let Some(witness) = self.nested_scroll_content {
            return witness.project_for(self.recording_owner?, live);
        }
        if let Some(witness) = self.nested_scroll_host {
            return witness.project_host_for(self.recording_owner?, live);
        }
        if let Some(witness) = self.scroll_text_area_subtree {
            return witness.project_for(self.recording_owner?, live);
        }
        if let Some(witness) = self.scroll_atomic_projection_text_area_subtree {
            return witness.project_for(self.recording_owner?, live);
        }
        if let Some(witness) = self.scroll_interactive_text_area_subtree {
            return witness.project_for(self.recording_owner?, live);
        }
        if let Some(stack) = self.consumed_ancestor_property_stack {
            return stack.project_for(self.recording_owner?, live, self.opacity_authority);
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
        self.nested_scroll_content
            .is_some_and(|witness| witness.is_canonical_for(owner))
            || matches!(
                self.consumed_ancestor_property,
                Some(ConsumedAncestorProperty::ScrollContents(witness))
                    if witness.is_canonical_for(owner)
            )
            || self.consumed_ancestor_property_stack.is_some_and(|stack| {
                stack.authorizes_scroll_content_local_owner(owner, self.opacity_authority)
            })
            || self
                .scroll_text_area_subtree
                .is_some_and(|witness| witness.is_canonical_for(owner))
            || self
                .scroll_atomic_projection_text_area_subtree
                .is_some_and(|witness| witness.is_canonical_for(owner))
            || self
                .scroll_interactive_text_area_subtree
                .is_some_and(|witness| witness.is_canonical_for(owner))
    }

    /// The C1 wrapper is the only Element allowed to carry a descendant
    /// contents clip inside the exact scroll artifact.  The TextArea witness
    /// freezes that clip and is rebound by coverage to the current owner, so
    /// this cannot authorize an unrelated child-clip topology.
    pub(crate) fn authorizes_scroll_text_area_content_wrapper(self, stable_id: u64) -> bool {
        let stable_witness = self
            .baked_scroll_text_area_subtree
            .or(self.scroll_text_area_subtree);
        let interactive_witness = self
            .baked_scroll_interactive_text_area_subtree
            .or(self.scroll_interactive_text_area_subtree);
        let atomic_witness = self
            .baked_scroll_atomic_projection_text_area_subtree
            .or(self.scroll_atomic_projection_text_area_subtree);
        matches!(
            (
                self.recording_owner,
                self.recording_owner_stable_id,
                stable_witness,
            ),
            (Some(owner), Some(recording_stable_id), Some(witness))
                if recording_stable_id == stable_id
                    && witness.outer().content_root() == owner
                    && witness.is_canonical_for(owner)
        ) || matches!(
            (
                self.recording_owner,
                self.recording_owner_stable_id,
                interactive_witness,
            ),
            (Some(owner), Some(recording_stable_id), Some(witness))
                if recording_stable_id == stable_id
                    && witness.outer().content_root() == owner
                    && witness.is_canonical_for(owner)
        ) || matches!(
            (
                self.recording_owner,
                self.recording_owner_stable_id,
                atomic_witness,
            ),
            (Some(owner), Some(recording_stable_id), Some(witness))
                if recording_stable_id == stable_id
                    && witness.outer().content_root() == owner
                    && witness.is_canonical_for(owner)
        )
    }

    pub(crate) fn suppresses_interactive_text_area_caret(self, owner: NodeKey) -> bool {
        self.recording_owner == Some(owner)
            && self
                .scroll_interactive_text_area_subtree
                .or(self.baked_scroll_interactive_text_area_subtree)
                .is_some_and(|witness| {
                    witness.text_area_root() == owner && witness.is_canonical_for(owner)
                })
    }

    /// Exact media-leaf exception for the bounded nested-scroll recorder. The
    /// leaf artifact has consumed S1/C1 but deliberately retains outer S0/C0
    /// until the R1 -> A0 composite. No generic/single-scroll authority can
    /// satisfy this predicate.
    pub(crate) fn authorizes_nested_scroll_content_properties(
        self,
        owner: NodeKey,
        properties: PropertyTreeState,
    ) -> bool {
        self.recording_owner == Some(owner)
            && self.nested_scroll_content.is_some_and(|witness| {
                witness.is_canonical_for(owner)
                    && properties
                        == (PropertyTreeState {
                            clip: Some(witness.outer_contents_clip()),
                            scroll: Some(witness.outer_scroll()),
                            ..PropertyTreeState::default()
                        })
            })
    }
}

/// Recorder-owned C3a property capability.  The full source grammar is kept
/// out of this Copy token: the recorder owns it and reruns the live oracle on
/// both sides of metadata/full recording.  This token can therefore authorize
/// only the exact property projection, never source topology or raster facts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PaintScrollAtomicProjectionTextAreaSubtreeWitness {
    outer: PaintScrollContentWitness,
    text_area_root: NodeKey,
    live_contents_clip: ClipNodeSnapshot,
    local_contents_clip: ClipNodeSnapshot,
    target_owner: NodeKey,
}

impl PaintScrollAtomicProjectionTextAreaSubtreeWitness {
    pub(crate) fn new(
        outer: PaintScrollContentWitness,
        text_area_root: NodeKey,
        live_contents_clip: ClipNodeSnapshot,
        local_logical_scissor: [u32; 4],
        paint_grammar: &crate::view::base_component::text_area::RetainedAtomicProjectionTextAreaPaintGrammar,
    ) -> Option<Self> {
        let outer_clip = outer.contents_clip_snapshot();
        let local_contents_clip = ClipNodeSnapshot {
            parent: None,
            logical_scissor: local_logical_scissor,
            generation: RETAINED_TEXT_AREA_LOCAL_CLIP_GENERATION,
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
            && paint_grammar.is_canonical())
        .then_some(Self {
            outer,
            text_area_root,
            live_contents_clip,
            local_contents_clip,
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

    fn is_canonical_for(self, owner: NodeKey) -> bool {
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
            && self.local_contents_clip.generation == RETAINED_TEXT_AREA_LOCAL_CLIP_GENERATION
    }

    fn project_for(self, owner: NodeKey, live: PropertyTreeState) -> Option<PropertyTreeState> {
        if !self.is_canonical_for(owner)
            || live.transform.is_some()
            || live.effect.is_some()
            || live.scroll != Some(self.outer.scroll_snapshot().id)
        {
            return None;
        }
        if live.clip == Some(self.outer.contents_clip_snapshot().id) {
            Some(PropertyTreeState::default())
        } else if live.clip == Some(self.live_contents_clip.id) {
            Some(PropertyTreeState {
                clip: Some(self.local_contents_clip.id),
                ..Default::default()
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
pub(crate) struct PaintScrollAtomicProjectionSelectionTextAreaSubtreeWitness {
    property: PaintScrollAtomicProjectionTextAreaSubtreeWitness,
    pub(crate) selection: crate::view::base_component::text_area::RetainedTextAreaPaintGrammar,
}

impl PaintScrollAtomicProjectionSelectionTextAreaSubtreeWitness {
    pub(crate) fn new(
        outer: PaintScrollContentWitness,
        text_area_root: NodeKey,
        live_contents_clip: ClipNodeSnapshot,
        local_logical_scissor: [u32; 4],
        grammar: &crate::view::base_component::text_area::RetainedAtomicProjectionSelectionTextAreaPaintGrammar,
    ) -> Option<Self> {
        if !matches!(
            grammar.selection,
            crate::view::base_component::text_area::RetainedTextAreaPaintGrammar::SelectionGlyphs { .. }
        ) || !grammar.selection.is_canonical()
        {
            return None;
        }
        Some(Self {
            property: PaintScrollAtomicProjectionTextAreaSubtreeWitness::new(
                outer,
                text_area_root,
                live_contents_clip,
                local_logical_scissor,
                &grammar.atomic_source,
            )?,
            selection: grammar.selection,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PaintScrollAtomicProjectionTextAreaRecorderWitness {
    ExistingAtomicGlyph(PaintScrollAtomicProjectionTextAreaSubtreeWitness),
    AtomicProjectionSelection(PaintScrollAtomicProjectionSelectionTextAreaSubtreeWitness),
}

impl PaintScrollAtomicProjectionTextAreaRecorderWitness {
    pub(crate) fn property(self) -> PaintScrollAtomicProjectionTextAreaSubtreeWitness {
        match self {
            Self::ExistingAtomicGlyph(witness) => witness,
            Self::AtomicProjectionSelection(witness) => witness.property,
        }
    }

    fn is_canonical_for(self, owner: NodeKey) -> bool {
        self.property().is_canonical_for(owner)
            && match self {
                Self::ExistingAtomicGlyph(_) => true,
                Self::AtomicProjectionSelection(witness) => {
                    matches!(
                        witness.selection,
                        crate::view::base_component::text_area::RetainedTextAreaPaintGrammar::SelectionGlyphs { .. }
                    ) && witness.selection.is_canonical()
                }
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
        let mut previous_rank = None;
        let mut transform_seen = false;
        let mut effect_seen = false;
        let mut scroll_seen = false;
        for entry in entries {
            let rank = match entry {
                ConsumedAncestorProperty::Transform(witness) => {
                    if !witness
                        .for_target(target_owner)
                        .is_canonical_for(target_owner)
                    {
                        return None;
                    }
                    if std::mem::replace(&mut transform_seen, true) {
                        return None;
                    }
                    0_u8
                }
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
                    2_u8
                }
                ConsumedAncestorProperty::Effect(witness) => {
                    if !witness
                        .for_target(target_owner)
                        .is_canonical_for(target_owner)
                    {
                        return None;
                    }
                    if std::mem::replace(&mut effect_seen, true) {
                        return None;
                    }
                    1_u8
                }
            };
            if previous_rank.is_some_and(|previous| previous >= rank) {
                return None;
            }
            previous_rank = Some(rank);
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

    fn authorizes_scroll_content_local_owner(
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
        let mut previous_rank = None;
        let mut scroll_witness = None;
        for entry in self.entries() {
            let rank = match entry {
                ConsumedAncestorProperty::Transform(witness) => {
                    if !witness.is_canonical_for(owner) {
                        return false;
                    }
                    0_u8
                }
                ConsumedAncestorProperty::ScrollContents(witness) => {
                    if !witness.is_canonical_for(owner) || scroll_witness.replace(witness).is_some()
                    {
                        return false;
                    }
                    2_u8
                }
                ConsumedAncestorProperty::Effect(witness) => {
                    if !witness.is_canonical_for(owner)
                        || opacity_authority
                            != PaintOpacityAuthority::NeutralRootEffect(witness.effect.id)
                    {
                        return false;
                    }
                    1_u8
                }
            };
            if previous_rank.is_some_and(|previous| previous >= rank) {
                return false;
            }
            previous_rank = Some(rank);
        }
        scroll_witness.is_some()
    }

    fn project_for(
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
            Self::Effect(witness) => Self::Effect(witness.for_target(target_owner)),
            Self::ScrollContents(witness) => Self::ScrollContents(witness.for_target(target_owner)),
        }
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

    fn is_canonical_for(self, owner: NodeKey) -> bool {
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

    fn is_canonical_for(self, owner: NodeKey) -> bool {
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

    fn is_canonical_for(self, owner: NodeKey) -> bool {
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
    paint_grammar: crate::view::base_component::text_area::RetainedTextAreaPaintGrammar,
    target_owner: NodeKey,
}

/// Typed raster-local generation for C1/C2a's detached TextArea contents clip.
/// Live property generations include viewport-space ancestry and therefore
/// cannot participate in a reusable offset-zero content identity.
pub(crate) const RETAINED_TEXT_AREA_LOCAL_CLIP_GENERATION: u64 = 1;

impl PaintScrollTextAreaSubtreeWitness {
    pub(crate) fn new(
        outer: PaintScrollContentWitness,
        text_area_root: NodeKey,
        live_contents_clip: ClipNodeSnapshot,
        local_logical_scissor: [u32; 4],
        paint_grammar: crate::view::base_component::text_area::RetainedTextAreaPaintGrammar,
    ) -> Option<Self> {
        let outer_clip = outer.contents_clip_snapshot();
        let local_contents_clip = ClipNodeSnapshot {
            parent: None,
            logical_scissor: local_logical_scissor,
            generation: RETAINED_TEXT_AREA_LOCAL_CLIP_GENERATION,
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
            && paint_grammar.is_canonical())
        .then_some(Self {
            outer,
            text_area_root,
            live_contents_clip,
            local_contents_clip,
            paint_grammar,
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

    pub(crate) fn paint_grammar(
        self,
    ) -> crate::view::base_component::text_area::RetainedTextAreaPaintGrammar {
        self.paint_grammar
    }

    pub(crate) fn for_target(self, target_owner: NodeKey) -> Self {
        Self {
            target_owner,
            ..self
        }
    }

    fn is_canonical_for(self, owner: NodeKey) -> bool {
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
            && self.local_contents_clip.generation == RETAINED_TEXT_AREA_LOCAL_CLIP_GENERATION
            && self.paint_grammar.is_canonical()
    }

    fn project_for(self, owner: NodeKey, live: PropertyTreeState) -> Option<PropertyTreeState> {
        if !self.is_canonical_for(owner)
            || live.transform.is_some()
            || live.effect.is_some()
            || live.scroll != Some(self.outer.scroll_snapshot().id)
        {
            return None;
        }
        if live.clip == Some(self.outer.contents_clip_snapshot().id) {
            Some(PropertyTreeState::default())
        } else if live.clip == Some(self.live_contents_clip.id) {
            Some(PropertyTreeState {
                clip: Some(self.local_contents_clip.id),
                ..Default::default()
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
    paint_grammar: crate::view::base_component::text_area::RetainedInteractiveTextAreaPaintGrammar,
    target_owner: NodeKey,
}

impl PaintScrollInteractiveTextAreaSubtreeWitness {
    pub(crate) fn new(
        outer: PaintScrollContentWitness,
        text_area_root: NodeKey,
        live_contents_clip: ClipNodeSnapshot,
        local_logical_scissor: [u32; 4],
        paint_grammar: crate::view::base_component::text_area::RetainedInteractiveTextAreaPaintGrammar,
    ) -> Option<Self> {
        let outer_clip = outer.contents_clip_snapshot();
        let local_contents_clip = ClipNodeSnapshot {
            parent: None,
            logical_scissor: local_logical_scissor,
            generation: RETAINED_TEXT_AREA_LOCAL_CLIP_GENERATION,
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
            && paint_grammar.is_canonical())
        .then_some(Self {
            outer,
            text_area_root,
            live_contents_clip,
            local_contents_clip,
            paint_grammar,
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

    pub(crate) fn paint_grammar(
        self,
    ) -> crate::view::base_component::text_area::RetainedInteractiveTextAreaPaintGrammar {
        self.paint_grammar
    }

    pub(crate) fn for_target(self, target_owner: NodeKey) -> Self {
        Self {
            target_owner,
            ..self
        }
    }

    fn is_canonical_for(self, owner: NodeKey) -> bool {
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
            && self.local_contents_clip.generation == RETAINED_TEXT_AREA_LOCAL_CLIP_GENERATION
            && self.paint_grammar.is_canonical()
    }

    fn project_for(self, owner: NodeKey, live: PropertyTreeState) -> Option<PropertyTreeState> {
        if !self.is_canonical_for(owner)
            || live.transform.is_some()
            || live.effect.is_some()
            || live.scroll != Some(self.outer.scroll_snapshot().id)
        {
            return None;
        }
        if live.clip == Some(self.outer.contents_clip_snapshot().id) {
            Some(PropertyTreeState::default())
        } else if live.clip == Some(self.live_contents_clip.id) {
            Some(PropertyTreeState {
                clip: Some(self.local_contents_clip.id),
                ..Default::default()
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

/// Recorder-owned witness for the leaf of exactly two nested scroll hosts.
/// The inner boundary is consumed in this recorder scope and projects to the
/// still-live outer boundary.  A separate outer scope consumes that remaining
/// boundary, preserving the generic stack's one-scroll invariant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PaintNestedScrollContentWitness {
    outer_boundary_root: NodeKey,
    inner_boundary_root: NodeKey,
    content_root: NodeKey,
    outer_scroll: ScrollNodeId,
    outer_contents_clip: ClipNodeId,
    inner_scroll: ScrollNodeId,
    inner_contents_clip: ClipNodeId,
    normalization_offset_bits: [u32; 2],
}

impl PaintNestedScrollContentWitness {
    #[cfg(test)]
    pub(crate) fn for_layerizer_test(
        outer_boundary_root: NodeKey,
        inner_boundary_root: NodeKey,
        content_root: NodeKey,
    ) -> Option<Self> {
        (outer_boundary_root != inner_boundary_root
            && outer_boundary_root != content_root
            && inner_boundary_root != content_root)
            .then_some(Self {
                outer_boundary_root,
                inner_boundary_root,
                content_root,
                outer_scroll: ScrollNodeId(outer_boundary_root),
                outer_contents_clip: ClipNodeId {
                    owner: outer_boundary_root,
                    role: ClipNodeRole::ContentsClip,
                },
                inner_scroll: ScrollNodeId(inner_boundary_root),
                inner_contents_clip: ClipNodeId {
                    owner: inner_boundary_root,
                    role: ClipNodeRole::ContentsClip,
                },
                normalization_offset_bits: [0.0_f32.to_bits(); 2],
            })
    }

    pub(crate) fn new(
        outer_boundary_root: NodeKey,
        inner_boundary_root: NodeKey,
        content_root: NodeKey,
        outer_scroll: ScrollNodeSnapshot,
        outer_contents_clip: ClipNodeSnapshot,
        inner_scroll: ScrollNodeSnapshot,
        inner_contents_clip: ClipNodeSnapshot,
    ) -> Option<Self> {
        let outer_clip = outer_contents_clip.id;
        let inner_clip = inner_contents_clip.id;
        let normalization_offset = [inner_scroll.offset.x, inner_scroll.offset.y];
        (outer_boundary_root != inner_boundary_root
            && outer_boundary_root != content_root
            && inner_boundary_root != content_root
            && outer_scroll.id == ScrollNodeId(outer_boundary_root)
            && outer_scroll.owner == outer_boundary_root
            && outer_scroll.parent.is_none()
            && outer_scroll.generation != 0
            && outer_clip.owner == outer_boundary_root
            && outer_clip.role == ClipNodeRole::ContentsClip
            && outer_contents_clip.owner == outer_boundary_root
            && outer_contents_clip.parent.is_none()
            && outer_contents_clip.generation != 0
            && inner_scroll.id == ScrollNodeId(inner_boundary_root)
            && inner_scroll.owner == inner_boundary_root
            && inner_scroll.parent == Some(outer_scroll.id)
            && inner_scroll.generation != 0
            && inner_clip.owner == inner_boundary_root
            && inner_clip.role == ClipNodeRole::ContentsClip
            && inner_contents_clip.owner == inner_boundary_root
            && inner_contents_clip.parent == Some(outer_clip)
            && inner_contents_clip.generation != 0
            && outer_scroll.has_canonical_vertical_geometry_with_contents_clip(outer_contents_clip)
            && inner_scroll.has_canonical_nested_vertical_geometry_with_contents_clip(
                inner_contents_clip,
                outer_scroll,
                outer_contents_clip,
            )
            && normalization_offset.into_iter().all(f32::is_finite))
        .then_some(Self {
            outer_boundary_root,
            inner_boundary_root,
            content_root,
            outer_scroll: outer_scroll.id,
            outer_contents_clip: outer_clip,
            inner_scroll: inner_scroll.id,
            inner_contents_clip: inner_clip,
            normalization_offset_bits: normalization_offset.map(f32::to_bits),
        })
    }

    pub(crate) fn boundary_root(self) -> NodeKey {
        self.inner_boundary_root
    }

    pub(crate) fn outer_boundary_root(self) -> NodeKey {
        self.outer_boundary_root
    }

    pub(crate) fn content_root(self) -> NodeKey {
        self.content_root
    }

    pub(crate) fn inner_scroll(self) -> ScrollNodeId {
        self.inner_scroll
    }

    pub(crate) fn outer_scroll(self) -> ScrollNodeId {
        self.outer_scroll
    }

    pub(crate) fn outer_contents_clip(self) -> ClipNodeId {
        self.outer_contents_clip
    }

    pub(crate) fn inner_contents_clip(self) -> ClipNodeId {
        self.inner_contents_clip
    }

    pub(crate) fn normalization_paint_offset(self) -> [f32; 2] {
        self.normalization_offset_bits.map(f32::from_bits)
    }

    fn is_canonical_for(self, owner: NodeKey) -> bool {
        self.outer_boundary_root != self.inner_boundary_root
            && self.outer_boundary_root != self.content_root
            && self.inner_boundary_root != self.content_root
            && owner == self.content_root
            && self.outer_scroll == ScrollNodeId(self.outer_boundary_root)
            && self.outer_contents_clip.owner == self.outer_boundary_root
            && self.outer_contents_clip.role == ClipNodeRole::ContentsClip
            && self.inner_scroll == ScrollNodeId(self.inner_boundary_root)
            && self.inner_contents_clip.owner == self.inner_boundary_root
            && self.inner_contents_clip.role == ClipNodeRole::ContentsClip
    }

    fn project_for(self, owner: NodeKey, live: PropertyTreeState) -> Option<PropertyTreeState> {
        (self.is_canonical_for(owner)
            && live
                == (PropertyTreeState {
                    clip: Some(self.inner_contents_clip),
                    scroll: Some(self.inner_scroll),
                    ..PropertyTreeState::default()
                }))
        .then_some(PropertyTreeState {
            clip: Some(self.outer_contents_clip),
            scroll: Some(self.outer_scroll),
            ..PropertyTreeState::default()
        })
    }

    fn project_host_for(
        self,
        owner: NodeKey,
        live: PropertyTreeState,
    ) -> Option<PropertyTreeState> {
        if owner != self.inner_boundary_root {
            return None;
        }
        let outer = PropertyTreeState {
            clip: Some(self.outer_contents_clip),
            scroll: Some(self.outer_scroll),
            ..PropertyTreeState::default()
        };
        let inner = PropertyTreeState {
            clip: Some(self.inner_contents_clip),
            scroll: Some(self.inner_scroll),
            ..PropertyTreeState::default()
        };
        if live == outer {
            Some(PropertyTreeState::default())
        } else if live == inner {
            Some(inner)
        } else {
            None
        }
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
    /// leaf referenced by `chunks`.
    pub(crate) clip_nodes: Vec<ClipNodeSnapshot>,
    /// Complete, arena-independent transitive effect snapshot for every
    /// effect leaf referenced by `chunks`. M6B validates this store but keeps
    /// the existing per-op baked opacity as visual authority.
    pub(crate) effect_nodes: Vec<EffectNodeSnapshot>,
    /// Canonical frame-traversal ownership topology for every chunk owner and
    /// its transitive ancestors. Roots are explicitly parentless even if the
    /// same arena node has an out-of-scope parent.
    pub(crate) owner_nodes: Vec<PaintOwnerSnapshot>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PaintOwnerSnapshot {
    pub(crate) owner: NodeKey,
    pub(crate) parent: Option<NodeKey>,
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
    SelectionUnderlay,
    TextDecoration,
    Caret,
    ScrollbarOverlay,
}

#[derive(Clone, Debug)]
pub(crate) enum PaintOp {
    DrawRect(DrawRectOp),
    PreparedInlineIfcDecoration(PreparedInlineIfcDecorationOp),
    PreparedShadow(PreparedShadowOp),
    PreparedScrollbarOverlay(PreparedScrollbarOverlayOp),
    PreparedText(PreparedTextOp),
    PreparedImage(PreparedImageOp),
    PreparedSvg(PreparedSvgOp),
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
    identity: PreparedScrollbarOverlayIdentity,
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
    pub(crate) fn from_vertical_witness(witness: ScrollbarOverlayWitness) -> Option<Self> {
        let alpha = witness.sampled_alpha;
        if !matches!(
            witness.paint_state,
            ScrollbarPaintStateWitness::OpaqueNow | ScrollbarPaintStateWitness::TranslucentNow
        ) || witness.horizontal_track.is_some()
            || witness.horizontal_thumb.is_some()
            || !alpha.is_finite()
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
        let (track, thumb) = witness.vertical_track.zip(witness.vertical_thumb)?;
        let track_shadow = Self::shadow(track, witness.shadow_blur_radius, 0.5 * alpha)?;
        let track = Self::fill(track, [0.95, 0.95, 0.95, 0.35 * alpha])?;
        let thumb_shadow = Self::shadow(thumb, witness.shadow_blur_radius, 0.5 * alpha)?;
        let thumb = Self::fill(thumb, [0.95, 0.95, 0.95, 0.58 * alpha])?;
        let identity = PreparedScrollbarOverlayIdentity::from_parts(
            &track_shadow,
            &track,
            &thumb_shadow,
            &thumb,
        )?;
        Some(Self {
            track_shadow,
            track,
            thumb_shadow,
            thumb,
            identity,
        })
    }

    fn shadow(rect: Rect, blur_radius: f32, alpha: f32) -> Option<PreparedScrollbarShadowOp> {
        let radius = (rect.width * 0.5).max(0.0);
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
        params.set_border_radius((rect.width * 0.5).max(0.0));
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
        )
        .as_ref()
            == Some(&self.identity)
    }

    pub(crate) fn matches_vertical_witness(&self, witness: ScrollbarOverlayWitness) -> bool {
        self.has_canonical_identity()
            && Self::from_vertical_witness(witness)
                .is_some_and(|expected| expected.identity == self.identity)
    }

    pub(crate) fn frozen_identity(&self) -> PreparedScrollbarOverlayIdentity {
        self.identity.clone()
    }

    pub(crate) fn has_baked_opacity(&self, expected_bits: u32) -> bool {
        self.track_shadow.params.opacity.to_bits() == expected_bits
            && self.track.params.opacity.to_bits() == expected_bits
            && self.thumb_shadow.params.opacity.to_bits() == expected_bits
            && self.thumb.params.opacity.to_bits() == expected_bits
    }
}

impl PreparedScrollbarOverlayIdentity {
    fn from_parts(
        track_shadow: &PreparedScrollbarShadowOp,
        track: &DrawRectOp,
        thumb_shadow: &PreparedScrollbarShadowOp,
        thumb: &DrawRectOp,
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
            || params.blur_radius.to_bits() != 0.0_f32.to_bits()
            || params
                .color
                .iter()
                .any(|channel| !channel.is_finite() || !(0.0..=1.0).contains(channel))
            || !params.opacity.is_finite()
            || !(0.0..=1.0).contains(&params.opacity)
            || params.spread.to_bits() != 0.0_f32.to_bits()
            || params.clip_to_geometry
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
    pub(crate) params: TextPassPreparedParams,
    identity: PreparedTextIdentity,
}

impl PreparedTextOp {
    pub(crate) fn new(params: TextPassPreparedParams) -> Option<Self> {
        let identity = PreparedTextIdentity::from_params(&params)?;
        Some(Self { params, identity })
    }

    pub(crate) fn has_canonical_identity(&self) -> bool {
        PreparedTextIdentity::from_params(&self.params).as_ref() == Some(&self.identity)
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

/// Exact semantic and raster identity for C2a's selection underlay.
///
/// The character range is intentionally duplicated beside the exact ordered
/// rectangle identities. That makes the payload itself prove which admitted
/// selection produced it; changing only the grammar cannot leave a canonical
/// retained stamp behind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RetainedTextAreaSelectionRasterSeal {
    pub(crate) start_char: usize,
    pub(crate) end_char: usize,
    pub(crate) color_rgba_bits: [u32; 4],
    pub(crate) rects: Arc<[PreparedDrawRectIdentity]>,
}

impl RetainedTextAreaSelectionRasterSeal {
    pub(crate) fn is_canonical_for_text_area(
        &self,
        grammar: crate::view::base_component::text_area::RetainedTextAreaPaintGrammar,
    ) -> bool {
        let crate::view::base_component::text_area::RetainedTextAreaPaintGrammar::SelectionGlyphs {
            start_char,
            end_char,
            color_rgba_bits,
        } = grammar
        else {
            return false;
        };
        self.start_char == start_char
            && self.end_char == end_char
            && self.color_rgba_bits == color_rgba_bits
            && !self.rects.is_empty()
            && self.rects.iter().all(|rect| {
                rect.mode == RectRenderMode::FillOnly
                    && rect.params.fill_color_bits == color_rgba_bits
                    && rect.params.opacity_bits == 1.0_f32.to_bits()
            })
    }

    pub(crate) fn is_canonical_for_interactive(
        &self,
        grammar: crate::view::base_component::text_area::RetainedInteractiveTextAreaPaintGrammar,
    ) -> bool {
        let crate::view::base_component::text_area::RetainedInteractiveTextAreaPaintGrammar::FocusedSelectionGlyphs {
            start_char,
            end_char,
            color_rgba_bits,
        } = grammar
        else {
            return false;
        };
        self.is_canonical_for_text_area(
            crate::view::base_component::text_area::RetainedTextAreaPaintGrammar::SelectionGlyphs {
                start_char,
                end_char,
                color_rgba_bits,
            },
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RetainedInteractiveTextAreaResidentRasterSeal {
    FocusedGlyphs,
    FocusedSelectionGlyphs(RetainedTextAreaSelectionRasterSeal),
    FocusedPreeditGlyphs(RetainedTextAreaPreeditRasterSeal),
}

impl RetainedInteractiveTextAreaResidentRasterSeal {
    pub(crate) fn paint_grammar(
        &self,
    ) -> crate::view::base_component::text_area::RetainedInteractiveTextAreaPaintGrammar {
        match self {
            Self::FocusedGlyphs => {
                crate::view::base_component::text_area::RetainedInteractiveTextAreaPaintGrammar::FocusedGlyphs
            }
            Self::FocusedSelectionGlyphs(seal) => {
                crate::view::base_component::text_area::RetainedInteractiveTextAreaPaintGrammar::FocusedSelectionGlyphs {
                    start_char: seal.start_char,
                    end_char: seal.end_char,
                    color_rgba_bits: seal.color_rgba_bits,
                }
            }
            Self::FocusedPreeditGlyphs(_) => {
                crate::view::base_component::text_area::RetainedInteractiveTextAreaPaintGrammar::FocusedPreeditGlyphs
            }
        }
    }

    pub(crate) fn is_canonical_for(
        &self,
        grammar: crate::view::base_component::text_area::RetainedInteractiveTextAreaPaintGrammar,
    ) -> bool {
        match (self, grammar) {
            (
                Self::FocusedGlyphs,
                crate::view::base_component::text_area::RetainedInteractiveTextAreaPaintGrammar::FocusedGlyphs,
            ) => true,
            (Self::FocusedSelectionGlyphs(seal), grammar) => {
                seal.is_canonical_for_interactive(grammar)
            }
            (
                Self::FocusedPreeditGlyphs(seal),
                crate::view::base_component::text_area::RetainedInteractiveTextAreaPaintGrammar::FocusedPreeditGlyphs,
            ) => seal.is_canonical(),
            _ => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RetainedAtomicProjectionTextAreaChunkRasterSeal {
    pub(crate) id: PaintChunkId,
    pub(crate) owner: NodeKey,
    pub(crate) bounds_bits: [u32; 4],
    pub(crate) payload_identity: PaintPayloadIdentity,
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
    Svg(PreparedSvgIdentity, Arc<[PreparedDrawRectIdentity]>),
    PreparedShadows(
        Arc<[PreparedShadowIdentity]>,
        Arc<[PreparedDrawRectIdentity]>,
    ),
    PreparedTexts(Arc<[PreparedTextIdentity]>),
    PreparedRects(Arc<[PreparedDrawRectIdentity]>),
    RetainedTextAreaSelection(RetainedTextAreaSelectionRasterSeal),
    PreparedScrollbarOverlay(PreparedScrollbarOverlayIdentity),
    InlineIfcDecorations(Arc<[PreparedInlineIfcDecorationIdentity]>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RetainedTextAreaGeneratedNodeKind {
    TextRun,
    PreeditRun,
    LineBreak,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RetainedTextAreaGeneratedNodeSeal {
    pub(crate) topology_index: usize,
    pub(crate) owner: NodeKey,
    pub(crate) parent: NodeKey,
    pub(crate) stable_id: u64,
    pub(crate) source_id: u64,
    pub(crate) kind: RetainedTextAreaGeneratedNodeKind,
    pub(crate) char_range: Range<usize>,
    pub(crate) backing_byte_range: Range<usize>,
    pub(crate) preedit_backing_byte_range: Option<Range<usize>>,
    pub(crate) preedit_caret_backing_byte: Option<usize>,
    pub(crate) text: Arc<str>,
    pub(crate) preedit_cursor: Option<(usize, usize)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RetainedTextAreaPreeditRasterSeal {
    pub(crate) text_area_root: NodeKey,
    pub(crate) paint_grammar:
        crate::view::base_component::text_area::RetainedInteractiveTextAreaPaintGrammar,
    pub(crate) content: Arc<str>,
    pub(crate) backing_text: Arc<str>,
    pub(crate) ime_preedit: Arc<str>,
    pub(crate) ime_preedit_cursor: Option<(usize, usize)>,
    pub(crate) cursor_char: usize,
    pub(crate) cursor_affinity: crate::view::base_component::text_area::CaretAffinity,
    pub(crate) unified_ifc_source_revision: u64,
    pub(crate) last_unified_apply_bits: Option<(u32, u32, u64)>,
    pub(crate) generated_topology: Arc<[RetainedTextAreaGeneratedNodeSeal]>,
    pub(crate) foreground_color_bits: [u32; 4],
    pub(crate) glyph_bounds_bits: [u32; 4],
    pub(crate) underline_bounds_bits: [u32; 4],
    pub(crate) glyph_identity: PaintPayloadIdentity,
    pub(crate) underline_identity: PaintPayloadIdentity,
}

impl RetainedTextAreaPreeditRasterSeal {
    pub(crate) fn is_canonical(&self) -> bool {
        if self.paint_grammar
            != crate::view::base_component::text_area::RetainedInteractiveTextAreaPaintGrammar::FocusedPreeditGlyphs
            || self.ime_preedit.is_empty()
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
            || !self
                .last_unified_apply_bits
                .is_some_and(|(x, y, revision)| {
                    f32::from_bits(x).is_finite()
                        && f32::from_bits(y).is_finite()
                        && revision == self.unified_ifc_source_revision
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
                || entry.parent != self.text_area_root
                || entry.owner == self.text_area_root
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
                RetainedTextAreaGeneratedNodeKind::PreeditRun => {
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
                RetainedTextAreaGeneratedNodeKind::TextRun => {
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
                RetainedTextAreaGeneratedNodeKind::LineBreak => {
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

fn preedit_glyph_identity_is_exact(
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

fn preedit_underline_identity_is_exact(
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RetainedTextAreaCaretOverlayPaintIdentity {
    Hidden,
    Culled {
        bounds_bits: [u32; 4],
        payload_identity: PaintPayloadIdentity,
    },
    Visible {
        bounds_bits: [u32; 4],
        payload_identity: PaintPayloadIdentity,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RetainedTextAreaCaretOverlayIdentity {
    pub(crate) owner: NodeKey,
    pub(crate) stable_id: u64,
    pub(crate) focused: bool,
    pub(crate) should_render: bool,
    pub(crate) caret_visible: bool,
    pub(crate) foreground_color_bits: [u32; 4],
    pub(crate) cursor_char: usize,
    pub(crate) cursor_affinity: crate::view::base_component::text_area::CaretAffinity,
    pub(crate) ime_preedit_cursor: Option<(usize, usize)>,
    pub(crate) local_scroll_bits: [u32; 2],
    pub(crate) unified_ifc_source_revision: u64,
    pub(crate) last_unified_apply_bits: Option<(u32, u32, u64)>,
    /// Independently recomputed by the source caret-map oracle. Paint/op
    /// identity must agree with it; clipping cannot redefine geometry.
    pub(crate) oracle_bounds_bits: Option<[u32; 4]>,
    pub(crate) text_area_clip: ClipNodeSnapshot,
    pub(crate) outer_clip: ClipNodeSnapshot,
    pub(crate) paint: RetainedTextAreaCaretOverlayPaintIdentity,
}

#[derive(Clone, Debug)]
pub(crate) struct RecordedRetainedTextAreaCaretOverlay {
    pub(crate) identity: RetainedTextAreaCaretOverlayIdentity,
    pub(crate) op: Option<DrawRectOp>,
}

impl RecordedRetainedTextAreaCaretOverlay {
    pub(crate) fn is_canonical(&self) -> bool {
        let identity = &self.identity;
        if identity.stable_id == 0
            || !identity.focused
            || !identity.should_render
            || identity
                .local_scroll_bits
                .map(f32::from_bits)
                .into_iter()
                .any(|v| !v.is_finite())
            || identity.unified_ifc_source_revision == 0
            || identity
                .last_unified_apply_bits
                .is_none_or(|(x, y, revision)| {
                    !f32::from_bits(x).is_finite()
                        || !f32::from_bits(y).is_finite()
                        || revision != identity.unified_ifc_source_revision
                })
            || identity.text_area_clip.id.owner != identity.owner
            || identity.text_area_clip.owner != identity.owner
            || identity.text_area_clip.id.role != ClipNodeRole::ContentsClip
            || identity.text_area_clip.parent != Some(identity.outer_clip.id)
            || identity.outer_clip.id.owner != identity.outer_clip.owner
            || identity.outer_clip.id.role != ClipNodeRole::ContentsClip
            || identity.outer_clip.owner == identity.owner
            || identity.outer_clip.id == identity.text_area_clip.id
            || identity.outer_clip.parent.is_some()
            || identity.text_area_clip.behavior != ClipBehavior::Intersect
            || identity.outer_clip.behavior != ClipBehavior::Intersect
            || identity.text_area_clip.generation == 0
            || identity.outer_clip.generation == 0
            || identity
                .foreground_color_bits
                .map(f32::from_bits)
                .into_iter()
                .any(|channel| !channel.is_finite() || !(0.0..=1.0).contains(&channel))
            || identity.text_area_clip.logical_scissor[0]
                .checked_add(identity.text_area_clip.logical_scissor[2])
                .is_none()
            || identity.text_area_clip.logical_scissor[1]
                .checked_add(identity.text_area_clip.logical_scissor[3])
                .is_none()
            || identity.outer_clip.logical_scissor[0]
                .checked_add(identity.outer_clip.logical_scissor[2])
                .is_none()
            || identity.outer_clip.logical_scissor[1]
                .checked_add(identity.outer_clip.logical_scissor[3])
                .is_none()
        {
            return false;
        }
        match (&identity.paint, &self.op, identity.oracle_bounds_bits) {
            (RetainedTextAreaCaretOverlayPaintIdentity::Hidden, None, None) => {
                !identity.caret_visible
            }
            (
                RetainedTextAreaCaretOverlayPaintIdentity::Culled {
                    bounds_bits,
                    payload_identity,
                },
                None,
                Some(oracle_bounds_bits),
            ) => {
                identity.caret_visible
                    && *bounds_bits == oracle_bounds_bits
                    && caret_payload_identity_is_exact(
                        *bounds_bits,
                        payload_identity,
                        identity.foreground_color_bits,
                    )
                    && !caret_bounds_intersect_live_clip_chain(
                        *bounds_bits,
                        identity.text_area_clip.logical_scissor,
                        identity.outer_clip.logical_scissor,
                    )
            }
            (
                RetainedTextAreaCaretOverlayPaintIdentity::Visible {
                    bounds_bits,
                    payload_identity,
                },
                Some(op),
                Some(oracle_bounds_bits),
            ) => {
                identity.caret_visible
                    && *bounds_bits == oracle_bounds_bits
                    && op.mode == RectRenderMode::FillOnly
                    && op.params.size[0].to_bits() == 1.0_f32.to_bits()
                    && op.params.size[1].is_finite()
                    && op.params.size[1] > 0.0
                    && op.params.opacity.to_bits() == 1.0_f32.to_bits()
                    && op.params.fill_color.map(f32::to_bits) == identity.foreground_color_bits
                    && *bounds_bits
                        == [
                            op.params.position[0],
                            op.params.position[1],
                            op.params.size[0],
                            op.params.size[1],
                        ]
                        .map(f32::to_bits)
                    && PaintPayloadIdentity::prepared_rects([op]).as_ref() == Some(payload_identity)
                    && caret_bounds_intersect_live_clip_chain(
                        *bounds_bits,
                        identity.text_area_clip.logical_scissor,
                        identity.outer_clip.logical_scissor,
                    )
            }
            _ => false,
        }
    }

    pub(crate) fn bitwise_eq(&self, other: &Self) -> bool {
        self.is_canonical()
            && other.is_canonical()
            && self.identity == other.identity
            && self
                .op
                .as_ref()
                .and_then(|op| PaintPayloadIdentity::prepared_rects([op]))
                == other
                    .op
                    .as_ref()
                    .and_then(|op| PaintPayloadIdentity::prepared_rects([op]))
    }
}

fn caret_payload_identity_is_exact(
    bounds_bits: [u32; 4],
    payload_identity: &PaintPayloadIdentity,
    foreground_color_bits: [u32; 4],
) -> bool {
    let PaintPayloadIdentity::PreparedRects(rects) = payload_identity else {
        return false;
    };
    let [rect] = rects.as_ref() else {
        return false;
    };
    rect.mode == RectRenderMode::FillOnly
        && rect.params.position_bits == [bounds_bits[0], bounds_bits[1]]
        && rect.params.size_bits == [bounds_bits[2], bounds_bits[3]]
        && bounds_bits[2] == 1.0_f32.to_bits()
        && f32::from_bits(bounds_bits[3]).is_finite()
        && f32::from_bits(bounds_bits[3]) > 0.0
        && rect.params.opacity_bits == 1.0_f32.to_bits()
        && rect.params.fill_color_bits == foreground_color_bits
}

fn caret_bounds_intersect_live_clip_chain(
    bounds_bits: [u32; 4],
    text_area_scissor: [u32; 4],
    outer_scissor: [u32; 4],
) -> bool {
    let [x, y, width, height] = bounds_bits.map(f32::from_bits);
    let left = text_area_scissor[0].max(outer_scissor[0]) as f32;
    let top = text_area_scissor[1].max(outer_scissor[1]) as f32;
    let (Some(text_right), Some(outer_right), Some(text_bottom), Some(outer_bottom)) = (
        text_area_scissor[0].checked_add(text_area_scissor[2]),
        outer_scissor[0].checked_add(outer_scissor[2]),
        text_area_scissor[1].checked_add(text_area_scissor[3]),
        outer_scissor[1].checked_add(outer_scissor[3]),
    ) else {
        return false;
    };
    let right = text_right.min(outer_right) as f32;
    let bottom = text_bottom.min(outer_bottom) as f32;
    [x, y, width, height].into_iter().all(f32::is_finite)
        && width > 0.0
        && height > 0.0
        && x < right
        && x + width > left
        && y < bottom
        && y + height > top
}

impl PaintPayloadIdentity {
    pub(crate) fn retained_text_area_selection_seal(
        &self,
    ) -> Option<RetainedTextAreaSelectionRasterSeal> {
        let Self::RetainedTextAreaSelection(seal) = self else {
            return None;
        };
        Some(seal.clone())
    }

    pub(crate) fn retained_text_area_selection_grammar(
        &self,
    ) -> Option<crate::view::base_component::text_area::RetainedTextAreaPaintGrammar> {
        let Self::RetainedTextAreaSelection(seal) = self else {
            return None;
        };
        let grammar =
            crate::view::base_component::text_area::RetainedTextAreaPaintGrammar::SelectionGlyphs {
                start_char: seal.start_char,
                end_char: seal.end_char,
                color_rgba_bits: seal.color_rgba_bits,
            };
        grammar.is_canonical().then_some(grammar)
    }

    pub(crate) fn matches_exact_text_area_selection_ops<'a>(
        &self,
        rects: impl IntoIterator<Item = &'a DrawRectOp>,
    ) -> bool {
        self.retained_text_area_selection_grammar()
            .and_then(|grammar| Self::prepared_text_area_selection(grammar, rects))
            .as_ref()
            == Some(self)
    }

    pub(crate) fn prepared_text_area_selection<'a>(
        grammar: crate::view::base_component::text_area::RetainedTextAreaPaintGrammar,
        rects: impl IntoIterator<Item = &'a DrawRectOp>,
    ) -> Option<Self> {
        let crate::view::base_component::text_area::RetainedTextAreaPaintGrammar::SelectionGlyphs {
            start_char,
            end_char,
            color_rgba_bits,
        } = grammar
        else {
            return None;
        };
        if !grammar.is_canonical() {
            return None;
        }
        let rects = Self::draw_rect_identities(rects)?;
        if rects.is_empty()
            || rects.iter().any(|rect| {
                rect.mode != RectRenderMode::FillOnly
                    || rect.params.fill_color_bits != color_rgba_bits
                    || rect.params.opacity_bits != 1.0_f32.to_bits()
            })
        {
            return None;
        }
        Some(Self::RetainedTextAreaSelection(
            RetainedTextAreaSelectionRasterSeal {
                start_char,
                end_char,
                color_rgba_bits,
                rects,
            },
        ))
    }

    pub(crate) fn matches_exact_text_area_selection(
        &self,
        grammar: crate::view::base_component::text_area::RetainedTextAreaPaintGrammar,
        op_count: usize,
        bounds_bits: [u32; 4],
    ) -> bool {
        let Self::RetainedTextAreaSelection(seal) = self else {
            return false;
        };
        let crate::view::base_component::text_area::RetainedTextAreaPaintGrammar::SelectionGlyphs {
            start_char,
            end_char,
            color_rgba_bits,
        } = grammar
        else {
            return false;
        };
        if !grammar.is_canonical()
            || seal.start_char != start_char
            || seal.end_char != end_char
            || seal.color_rgba_bits != color_rgba_bits
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
        Self::PreparedScrollbarOverlay(op.frozen_identity())
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

    pub(crate) fn svg_with_decoration<'a>(
        svg: PreparedSvgIdentity,
        decoration: impl IntoIterator<Item = &'a DrawRectOp>,
    ) -> Option<Self> {
        Some(Self::Svg(svg, Self::draw_rect_identities(decoration)?))
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
        Self::InlineIfcDecorations(
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

impl PreparedTextIdentity {
    fn from_params(params: &TextPassPreparedParams) -> Option<Self> {
        let scale_factor = params.staging_input.scale_factor;
        if !scale_factor.is_finite()
            || scale_factor <= 0.0
            || params.staging_input.glyphs.is_empty()
            || params.fragments.is_empty()
            || params
                .scissor_rect
                .is_some_and(|[_, _, width, height]| width == 0 || height == 0)
        {
            return None;
        }

        let fragments = params
            .fragments
            .iter()
            .map(|fragment| {
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
            })
            .collect::<Option<Vec<_>>>()?;

        let glyphs = params
            .staging_input
            .glyphs
            .iter()
            .map(|glyph| {
                let font_data = glyph.raster.font_data.as_ref()?;
                if font_data.data.id() != glyph.raster.font_data_id
                    || font_data.index != glyph.raster.font_index
                    || glyph.raster.glyph_id > u16::MAX as u32
                    || !glyph.raster.font_size.is_finite()
                    || glyph.raster.font_size <= 0.0
                    || glyph.paint.fragment_index as usize >= params.fragments.len()
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
                let fragment = params.fragments[glyph.paint.fragment_index as usize];
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
            })
            .collect::<Option<Vec<_>>>()?;

        Some(Self {
            scale_factor_bits: scale_factor.to_bits(),
            glyphs: glyphs.into(),
            fragments: fragments.into(),
            scissor_rect: params.scissor_rect,
            stencil_clip_id: params.stencil_clip_id,
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
mod consumed_ancestor_property_tests {
    use super::*;
    use crate::view::base_component::Element;
    use crate::view::test_support::{commit_child, commit_element, new_test_arena};
    use slotmap::SlotMap;

    fn keys() -> (NodeKey, NodeKey, NodeKey) {
        let mut arena = new_test_arena();
        let parent = commit_element(
            &mut arena,
            Box::new(Element::new_with_id(0xd1_1000, 0.0, 0.0, 10.0, 10.0)),
        );
        let child = commit_child(
            &mut arena,
            parent,
            Box::new(Element::new_with_id(0xd1_1001, 0.0, 0.0, 8.0, 8.0)),
        );
        let descendant = commit_child(
            &mut arena,
            child,
            Box::new(Element::new_with_id(0xd1_1002, 0.0, 0.0, 4.0, 4.0)),
        );
        (parent, child, descendant)
    }

    #[test]
    fn property_effect_contract_detaches_the_local_clip_root_parent() {
        let (ancestor, boundary, _) = keys();
        let effect = EffectNodeSnapshot {
            id: EffectNodeId(boundary),
            owner: boundary,
            parent: None,
            opacity: 0.5,
            generation: 7,
        };
        let ancestor_clip = ClipNodeSnapshot {
            id: ClipNodeId {
                owner: ancestor,
                role: ClipNodeRole::ContentsClip,
            },
            owner: ancestor,
            parent: None,
            behavior: crate::view::compositor::property_tree::ClipBehavior::Intersect,
            logical_scissor: [0, 0, 20, 20],
            generation: 8,
        };
        let local_clip = ClipNodeSnapshot {
            id: ClipNodeId {
                owner: boundary,
                role: ClipNodeRole::SelfClip,
            },
            owner: boundary,
            parent: Some(ancestor_clip.id),
            behavior: crate::view::compositor::property_tree::ClipBehavior::Replace,
            logical_scissor: [2, 2, 10, 10],
            generation: 9,
        };
        let contract = EffectPropertySurfaceArtifactContract::new(
            boundary,
            0xd1_2000,
            effect,
            vec![effect],
            Vec::new(),
            vec![local_clip],
            vec![ancestor_clip],
            vec![EffectPropertyContentWitness {
                owner: boundary,
                stable_id: 0xd1_2000,
                parent: None,
                self_paint_revision: 10,
                topology_revision: 11,
            }],
        )
        .expect("canonical clipped effect contract");
        let detached = contract
            .detach_clip_snapshot(&[local_clip, ancestor_clip])
            .expect("exact ancestor suffix detaches");
        assert_eq!(detached.len(), 1);
        assert_eq!(detached[0].id, local_clip.id);
        assert_eq!(detached[0].parent, None);
        assert_eq!(contract.isolated_local_raster_clips(), detached);
    }

    #[test]
    fn consumed_transform_projection_is_owner_bound_and_preserves_other_properties() {
        let (parent, child, descendant) = keys();
        let transform = TransformNodeId(parent);
        let witness = ConsumedAncestorTransformWitness::new(parent, child, transform)
            .expect("canonical direct-boundary identity");
        let effect = EffectNodeId(child);
        let live = PropertyTreeState {
            transform: Some(transform),
            effect: Some(effect),
            ..Default::default()
        };
        let context = PaintRecordingContext {
            recording_owner: Some(descendant),
            consumed_ancestor_property: Some(ConsumedAncestorProperty::Transform(
                witness.for_target(descendant),
            )),
            ..Default::default()
        };
        assert_eq!(
            context.project_consumed_ancestor_property(live),
            Some(PropertyTreeState {
                transform: None,
                effect: Some(effect),
                ..Default::default()
            })
        );
    }

    #[test]
    fn consumed_scroll_contents_projection_is_atomic_owner_bound_and_preserves_other_properties() {
        let (parent, child, descendant) = keys();
        let scroll = ScrollNodeId(parent);
        let contents_clip = ClipNodeId {
            owner: parent,
            role: ClipNodeRole::ContentsClip,
        };
        let witness =
            ConsumedAncestorScrollContentsWitness::new(parent, child, scroll, contents_clip)
                .unwrap();
        let effect = EffectNodeId(child);
        let transform = TransformNodeId(child);
        let live = PropertyTreeState {
            transform: Some(transform),
            clip: Some(contents_clip),
            effect: Some(effect),
            scroll: Some(scroll),
        };
        let context = PaintRecordingContext {
            recording_owner: Some(descendant),
            consumed_ancestor_property: Some(ConsumedAncestorProperty::ScrollContents(
                witness.for_target(descendant),
            )),
            ..Default::default()
        };
        assert_eq!(
            context.project_consumed_ancestor_property(live),
            Some(PropertyTreeState {
                transform: Some(transform),
                effect: Some(effect),
                ..Default::default()
            })
        );

        for mismatch in [
            PropertyTreeState {
                scroll: None,
                ..live
            },
            PropertyTreeState { clip: None, ..live },
        ] {
            assert_eq!(context.project_consumed_ancestor_property(mismatch), None);
        }
        let wrong_target = PaintRecordingContext {
            recording_owner: Some(child),
            consumed_ancestor_property: context.consumed_ancestor_property,
            ..Default::default()
        };
        assert_eq!(wrong_target.project_consumed_ancestor_property(live), None);
        assert!(
            ConsumedAncestorScrollContentsWitness::new(parent, parent, scroll, contents_clip)
                .is_none()
        );
        assert!(
            ConsumedAncestorScrollContentsWitness::new(
                parent,
                child,
                ScrollNodeId(child),
                contents_clip,
            )
            .is_none()
        );
    }

    #[test]
    fn consumed_property_stack_projects_transform_then_scroll_atomically() {
        let (transform_owner, scroll_owner, content_owner) = keys();
        let transform = TransformNodeId(transform_owner);
        let scroll = ScrollNodeId(scroll_owner);
        let contents_clip = ClipNodeId {
            owner: scroll_owner,
            role: ClipNodeRole::ContentsClip,
        };
        let transform_witness =
            ConsumedAncestorTransformWitness::new(transform_owner, scroll_owner, transform)
                .unwrap();
        let scroll_witness = ConsumedAncestorScrollContentsWitness::new(
            scroll_owner,
            content_owner,
            scroll,
            contents_clip,
        )
        .unwrap();
        let stack = ConsumedAncestorPropertyStackWitness::new(
            content_owner,
            &[
                ConsumedAncestorProperty::Transform(transform_witness),
                ConsumedAncestorProperty::ScrollContents(scroll_witness),
            ],
        )
        .unwrap();
        let live = PropertyTreeState {
            transform: Some(transform),
            clip: Some(contents_clip),
            scroll: Some(scroll),
            ..Default::default()
        };
        let context = PaintRecordingContext {
            recording_owner: Some(content_owner),
            consumed_ancestor_property_stack: Some(stack),
            ..Default::default()
        };
        assert_eq!(
            context.project_consumed_ancestor_property(live),
            Some(PropertyTreeState::default())
        );
        assert_eq!(
            context.project_consumed_ancestor_property(PropertyTreeState {
                transform: None,
                ..live
            }),
            None
        );
        let retargeted = PaintRecordingContext {
            recording_owner: Some(scroll_owner),
            consumed_ancestor_property_stack: Some(stack),
            ..Default::default()
        };
        assert_eq!(retargeted.project_consumed_ancestor_property(live), None);
        let reversed = ConsumedAncestorPropertyStackWitness::new(
            content_owner,
            &[
                ConsumedAncestorProperty::ScrollContents(scroll_witness),
                ConsumedAncestorProperty::Transform(transform_witness),
            ],
        );
        assert!(
            reversed.is_none(),
            "planner order is part of the capability"
        );
    }

    #[test]
    fn consumed_effect_scroll_stack_requires_exact_chain_and_neutral_authority() {
        let (effect_owner, scroll_owner, content_owner) = keys();
        let effect = EffectNodeSnapshot {
            id: EffectNodeId(effect_owner),
            owner: effect_owner,
            parent: None,
            opacity: 0.5,
            generation: 7,
        };
        let effect_witness = ConsumedAncestorEffectWitness::new(
            effect_owner,
            scroll_owner,
            effect,
            Some(effect.id),
            None,
        )
        .unwrap();
        let contents_clip = ClipNodeId {
            owner: scroll_owner,
            role: ClipNodeRole::ContentsClip,
        };
        let scroll_witness = ConsumedAncestorScrollContentsWitness::new(
            scroll_owner,
            content_owner,
            ScrollNodeId(scroll_owner),
            contents_clip,
        )
        .unwrap();
        let stack = ConsumedAncestorPropertyStackWitness::new(
            content_owner,
            &[
                ConsumedAncestorProperty::Effect(effect_witness),
                ConsumedAncestorProperty::ScrollContents(scroll_witness),
            ],
        )
        .unwrap();
        let live = PropertyTreeState {
            clip: Some(contents_clip),
            effect: Some(effect.id),
            scroll: Some(ScrollNodeId(scroll_owner)),
            ..Default::default()
        };
        let neutral = PaintRecordingContext {
            recording_owner: Some(content_owner),
            consumed_ancestor_property_stack: Some(stack),
            opacity_authority: PaintOpacityAuthority::NeutralRootEffect(effect.id),
            ..Default::default()
        };
        assert_eq!(
            neutral.project_consumed_ancestor_property(live),
            Some(PropertyTreeState::default())
        );
        assert!(neutral.authorizes_scroll_content_local_owner(content_owner));

        let baked = PaintRecordingContext {
            opacity_authority: PaintOpacityAuthority::Baked,
            ..neutral
        };
        assert_eq!(baked.project_consumed_ancestor_property(live), None);
        assert!(!baked.authorizes_scroll_content_local_owner(content_owner));

        let mut wrong_chain = effect_witness;
        wrong_chain.projected_after = Some(EffectNodeId(scroll_owner));
        assert!(
            ConsumedAncestorPropertyStackWitness::new(
                content_owner,
                &[
                    ConsumedAncestorProperty::Effect(wrong_chain),
                    ConsumedAncestorProperty::ScrollContents(scroll_witness),
                ],
            )
            .is_none()
        );
        assert!(
            ConsumedAncestorPropertyStackWitness::new(
                content_owner,
                &[
                    ConsumedAncestorProperty::ScrollContents(scroll_witness),
                    ConsumedAncestorProperty::Effect(effect_witness),
                ],
            )
            .is_none(),
            "projection capability is sealed outer-to-inner"
        );
    }

    #[test]
    fn consumed_transform_effect_scroll_stack_projects_all_three_layers_exactly() {
        let mut keys = SlotMap::<NodeKey, ()>::with_key();
        let transform_owner = keys.insert(());
        let effect_owner = keys.insert(());
        let scroll_owner = keys.insert(());
        let content_owner = keys.insert(());
        let transform = TransformNodeId(transform_owner);
        let effect = EffectNodeSnapshot {
            id: EffectNodeId(effect_owner),
            owner: effect_owner,
            parent: None,
            opacity: 0.5,
            generation: 9,
        };
        let transform_witness =
            ConsumedAncestorTransformWitness::new(transform_owner, effect_owner, transform)
                .unwrap();
        let effect_witness = ConsumedAncestorEffectWitness::new(
            effect_owner,
            scroll_owner,
            effect,
            Some(effect.id),
            None,
        )
        .unwrap();
        let clip = ClipNodeId {
            owner: scroll_owner,
            role: ClipNodeRole::ContentsClip,
        };
        let scroll_witness = ConsumedAncestorScrollContentsWitness::new(
            scroll_owner,
            content_owner,
            ScrollNodeId(scroll_owner),
            clip,
        )
        .unwrap();
        let stack = ConsumedAncestorPropertyStackWitness::new(
            content_owner,
            &[
                ConsumedAncestorProperty::Transform(transform_witness),
                ConsumedAncestorProperty::Effect(effect_witness),
                ConsumedAncestorProperty::ScrollContents(scroll_witness),
            ],
        )
        .unwrap();
        let live = PropertyTreeState {
            transform: Some(transform),
            effect: Some(effect.id),
            scroll: Some(ScrollNodeId(scroll_owner)),
            clip: Some(clip),
        };
        let context = PaintRecordingContext {
            recording_owner: Some(content_owner),
            consumed_ancestor_property_stack: Some(stack),
            opacity_authority: PaintOpacityAuthority::NeutralRootEffect(effect.id),
            ..Default::default()
        };
        assert_eq!(
            context.project_consumed_ancestor_property(live),
            Some(PropertyTreeState::default())
        );
        assert!(context.authorizes_scroll_content_local_owner(content_owner));
        for invalid in [
            [
                ConsumedAncestorProperty::Effect(effect_witness),
                ConsumedAncestorProperty::Transform(transform_witness),
                ConsumedAncestorProperty::ScrollContents(scroll_witness),
            ],
            [
                ConsumedAncestorProperty::Transform(transform_witness),
                ConsumedAncestorProperty::ScrollContents(scroll_witness),
                ConsumedAncestorProperty::Effect(effect_witness),
            ],
        ] {
            assert!(ConsumedAncestorPropertyStackWitness::new(content_owner, &invalid).is_none());
        }
    }

    #[test]
    fn scroll_content_local_authority_accepts_only_exact_canonical_stack() {
        let (transform_owner, scroll_owner, content_owner) = keys();
        let transform_witness = ConsumedAncestorTransformWitness::new(
            transform_owner,
            scroll_owner,
            TransformNodeId(transform_owner),
        )
        .unwrap();
        let scroll_witness = ConsumedAncestorScrollContentsWitness::new(
            scroll_owner,
            content_owner,
            ScrollNodeId(scroll_owner),
            ClipNodeId {
                owner: scroll_owner,
                role: ClipNodeRole::ContentsClip,
            },
        )
        .unwrap();
        let stack = ConsumedAncestorPropertyStackWitness::new(
            content_owner,
            &[
                ConsumedAncestorProperty::Transform(transform_witness),
                ConsumedAncestorProperty::ScrollContents(scroll_witness),
            ],
        )
        .unwrap();
        let context = PaintRecordingContext {
            recording_owner: Some(content_owner),
            consumed_ancestor_property_stack: Some(stack),
            ..Default::default()
        };
        assert!(context.authorizes_scroll_content_local_owner(content_owner));

        let wrong_owner = PaintRecordingContext {
            recording_owner: Some(scroll_owner),
            ..context
        };
        assert!(!wrong_owner.authorizes_scroll_content_local_owner(scroll_owner));

        let transform_only = ConsumedAncestorPropertyStackWitness::new(
            content_owner,
            &[ConsumedAncestorProperty::Transform(transform_witness)],
        )
        .unwrap();
        assert!(
            !PaintRecordingContext {
                recording_owner: Some(content_owner),
                consumed_ancestor_property_stack: Some(transform_only),
                ..Default::default()
            }
            .authorizes_scroll_content_local_owner(content_owner)
        );

        let duplicate_scroll = ConsumedAncestorPropertyStackWitness {
            entries: [
                Some(ConsumedAncestorProperty::ScrollContents(scroll_witness)),
                Some(ConsumedAncestorProperty::ScrollContents(scroll_witness)),
                None,
            ],
            len: 2,
            target_owner: content_owner,
        };
        assert!(
            !PaintRecordingContext {
                recording_owner: Some(content_owner),
                consumed_ancestor_property_stack: Some(duplicate_scroll),
                ..Default::default()
            }
            .authorizes_scroll_content_local_owner(content_owner)
        );

        let mut noncanonical = stack;
        noncanonical.entries[1] = Some(ConsumedAncestorProperty::ScrollContents(
            scroll_witness.for_target(scroll_owner),
        ));
        assert!(
            !PaintRecordingContext {
                recording_owner: Some(content_owner),
                consumed_ancestor_property_stack: Some(noncanonical),
                ..Default::default()
            }
            .authorizes_scroll_content_local_owner(content_owner)
        );
    }

    #[test]
    fn wrong_child_boundary_retarget_or_live_transform_cannot_project() {
        let (parent, child, descendant) = keys();
        let transform = TransformNodeId(parent);
        assert!(ConsumedAncestorTransformWitness::new(parent, parent, transform).is_none());
        let witness = ConsumedAncestorTransformWitness::new(parent, child, transform).unwrap();
        let live = PropertyTreeState {
            transform: Some(transform),
            ..Default::default()
        };
        let wrong_target = PaintRecordingContext {
            recording_owner: Some(child),
            consumed_ancestor_property: Some(ConsumedAncestorProperty::Transform(
                witness.for_target(descendant),
            )),
            ..Default::default()
        };
        assert_eq!(wrong_target.project_consumed_ancestor_property(live), None);

        let mismatch = PaintRecordingContext {
            recording_owner: Some(child),
            consumed_ancestor_property: Some(ConsumedAncestorProperty::Transform(
                witness.for_target(child),
            )),
            ..Default::default()
        };
        assert_eq!(
            mismatch.project_consumed_ancestor_property(PropertyTreeState::default()),
            None
        );
    }
}
