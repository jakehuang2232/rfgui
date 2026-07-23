//! Retained transform / clip / effect / scroll property trees.
//!
//! These trees are not yet render truth.  They retain stable `NodeKey` based
//! identity and classify resolved property changes while the existing render
//! and dirty paths remain authoritative.

#![allow(dead_code)]

use glam::{Mat4, Vec2};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::view::base_component::{
    Rect, ScrollAxisSnapshot, ScrollContentsClipWitness, ScrollGeometryObservation,
    ScrollGeometrySnapshot, ScrollbarOverlayWitness, ScrollbarPaintStateWitness, Size,
    canonical_horizontal_scrollbar_geometry, canonical_vertical_scrollbar_geometry,
    exact_logical_scissor_for_rect,
};
use crate::view::node_arena::{NodeArena, NodeKey};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TransformNodeId(pub(crate) NodeKey);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct EffectNodeId(pub(crate) NodeKey);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ScrollNodeId(pub(crate) NodeKey);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ClipNodeRole {
    SelfClip,
    ContentsClip,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ClipNodeId {
    pub(crate) owner: NodeKey,
    pub(crate) role: ClipNodeRole,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PropertyTreeState {
    pub(crate) transform: Option<TransformNodeId>,
    pub(crate) clip: Option<ClipNodeId>,
    pub(crate) effect: Option<EffectNodeId>,
    pub(crate) scroll: Option<ScrollNodeId>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct NodePropertyState {
    /// Properties that apply while painting the node itself.
    pub(crate) paint: PropertyTreeState,
    /// Properties inherited by the node's authoritative arena children.
    pub(crate) descendants: PropertyTreeState,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct TransformNode {
    pub(crate) owner: NodeKey,
    pub(crate) parent: Option<TransformNodeId>,
    /// Current Element transforms are resolved in viewport space.  Keeping
    /// that fact in the name prevents future callers from parent-multiplying
    /// this value as though it were local-space.
    pub(crate) viewport_matrix: Mat4,
    pub(crate) generation: u64,
}

/// Arena-independent, owning copy of one transform-tree node.  The matrix is
/// already expressed in the engine's shared viewport-basis paint space; a
/// retained-surface planner must apply it exactly once at the owning
/// composite edge and must not parent-multiply it while freezing the plan.
#[derive(Clone, Copy, Debug)]
pub(crate) struct TransformNodeSnapshot {
    pub(crate) id: TransformNodeId,
    pub(crate) owner: NodeKey,
    pub(crate) parent: Option<TransformNodeId>,
    pub(crate) viewport_matrix: Mat4,
    pub(crate) generation: u64,
}

impl PartialEq for TransformNodeSnapshot {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.owner == other.owner
            && self.parent == other.parent
            && self.viewport_matrix.to_cols_array().map(f32::to_bits)
                == other.viewport_matrix.to_cols_array().map(f32::to_bits)
            && self.generation == other.generation
    }
}

impl Eq for TransformNodeSnapshot {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PropertyTreeValidationError {
    NonFiniteTransform(NodeKey),
    /// The host is a legitimate scroll container, but the narrow M10E0 slice
    /// cannot yet prove an exact legacy rectangular contents clip.
    ScrollContractUnavailable(NodeKey),
    /// A host returned a snapshot, but its fields do not form one complete,
    /// internally consistent scroll observation.
    InvalidScrollGeometrySnapshot(NodeKey),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ClipBehavior {
    Intersect,
    Replace,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum ClipGeometry {
    Rect(Rect),
    RoundedRect {
        rect: Rect,
        radii: [f32; 4],
    },
    /// Already-resolved logical scissor from the legacy layout path. This is
    /// intentionally stored verbatim; property sync must not repeat the
    /// floor/ceil conversion and risk drifting from legacy paint.
    LogicalScissor([u32; 4]),
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ClipNode {
    pub(crate) owner: NodeKey,
    pub(crate) parent: Option<ClipNodeId>,
    pub(crate) geometry: ClipGeometry,
    pub(crate) behavior: ClipBehavior,
    pub(crate) generation: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ClipNodeSnapshot {
    pub(crate) id: ClipNodeId,
    pub(crate) owner: NodeKey,
    pub(crate) parent: Option<ClipNodeId>,
    pub(crate) logical_scissor: [u32; 4],
    pub(crate) behavior: ClipBehavior,
    pub(crate) generation: u64,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct EffectNode {
    pub(crate) owner: NodeKey,
    pub(crate) parent: Option<EffectNodeId>,
    pub(crate) opacity: f32,
    pub(crate) generation: u64,
}

/// Arena-independent, owning copy of one effect-tree node. Opacity remains
/// baked into paint ops in M6B; this snapshot is strict identity/topology
/// evidence only and does not make the effect tree render authority.
#[derive(Clone, Copy, Debug)]
pub(crate) struct EffectNodeSnapshot {
    pub(crate) id: EffectNodeId,
    pub(crate) owner: NodeKey,
    pub(crate) parent: Option<EffectNodeId>,
    pub(crate) opacity: f32,
    pub(crate) generation: u64,
}

impl PartialEq for EffectNodeSnapshot {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.owner == other.owner
            && self.parent == other.parent
            && self.opacity.to_bits() == other.opacity.to_bits()
            && self.generation == other.generation
    }
}

impl Eq for EffectNodeSnapshot {}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ScrollNode {
    pub(crate) owner: NodeKey,
    pub(crate) parent: Option<ScrollNodeId>,
    pub(crate) offset: Vec2,
    /// Configured input/scrollbar axes, not a translation mask. Consumers
    /// must project the complete 2D `offset`.
    pub(crate) configured_axis: ScrollAxisSnapshot,
    pub(crate) viewport: Rect,
    pub(crate) content_size: Size,
    /// Layout extent at offset zero, not paint overflow or raster bounds.
    pub(crate) layout_content_bounds_at_zero: Rect,
    pub(crate) scrollbar_overlay: ScrollbarOverlayWitness,
    pub(crate) contents_clip: ScrollContentsClipWitness,
    pub(crate) generation: u64,
}

/// Arena-independent, owning snapshot of one exact M10E0 scroll node.
/// Configured axes remain interaction/scrollbar metadata only; consumers must
/// always preserve the complete two-dimensional baked offset.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ScrollNodeSnapshot {
    pub(crate) id: ScrollNodeId,
    pub(crate) owner: NodeKey,
    pub(crate) parent: Option<ScrollNodeId>,
    pub(crate) offset: Vec2,
    pub(crate) configured_axis: ScrollAxisSnapshot,
    pub(crate) viewport: Rect,
    pub(crate) content_size: Size,
    pub(crate) layout_content_bounds_at_zero: Rect,
    pub(crate) scrollbar_overlay: ScrollbarOverlayWitness,
    pub(crate) contents_clip: ScrollContentsClipWitness,
    pub(crate) generation: u64,
}

impl PartialEq for ScrollNodeSnapshot {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.owner == other.owner
            && self.parent == other.parent
            && f32_bits_equal(self.offset.x, other.offset.x)
            && f32_bits_equal(self.offset.y, other.offset.y)
            && self.configured_axis == other.configured_axis
            && rect_bits_equal(self.viewport, other.viewport)
            && f32_bits_equal(self.content_size.width, other.content_size.width)
            && f32_bits_equal(self.content_size.height, other.content_size.height)
            && rect_bits_equal(
                self.layout_content_bounds_at_zero,
                other.layout_content_bounds_at_zero,
            )
            && scrollbar_overlay_bits_equal(self.scrollbar_overlay, other.scrollbar_overlay)
            && self.contents_clip == other.contents_clip
            && self.generation == other.generation
    }
}

impl Eq for ScrollNodeSnapshot {}

impl ScrollNodeSnapshot {
    /// Exact synchronized geometry gate for a scroll boundary nested below
    /// an independently-owned ancestor clip chain. The contents clip remains
    /// the scroll node's own authority; its parent is deliberately preserved
    /// for the final composite instead of being required to be absent.
    pub(crate) fn is_canonical_with_ancestor_contents_clip(self, clip: ClipNodeSnapshot) -> bool {
        let geometry = ScrollGeometrySnapshot {
            configured_axis: self.configured_axis,
            offset: [self.offset.x, self.offset.y],
            scrollport_rect: self.viewport,
            content_size: [self.content_size.width, self.content_size.height],
            layout_content_bounds_at_zero: self.layout_content_bounds_at_zero,
            contents_clip: self.contents_clip,
            scrollbar_overlay: self.scrollbar_overlay,
        };
        self.id.0 == self.owner
            && self.parent.is_none()
            && self.generation != 0
            && clip.id.owner == self.owner
            && clip.id.role == ClipNodeRole::ContentsClip
            && clip.owner == self.owner
            && clip.behavior == ClipBehavior::Intersect
            && clip.generation != 0
            && self.contents_clip == ScrollContentsClipWitness::ExactRect(clip.logical_scissor)
            && scroll_geometry_snapshot_is_valid(geometry)
    }

    /// Geometry half of the compiler gate for one exact 2D baked-scroll host.
    /// `configured_axis` controls input and scrollbar overlay geometry only;
    /// both synchronized offset components and content extents remain raster
    /// authority for Vertical, Horizontal, and Both hosts.
    pub(crate) fn has_canonical_geometry_with_contents_clip(self, clip: ClipNodeSnapshot) -> bool {
        self.has_canonical_geometry_with_contents_clip_and_parents(clip, None, None)
    }

    /// Exact parent-chain gate for the bounded `S0 -> S1` foundation slice.
    /// The parent must itself satisfy the unchanged parentless B0 geometry
    /// contract; this method only admits the direct child scroll/clip pair.
    pub(crate) fn has_canonical_nested_geometry_with_contents_clip(
        self,
        clip: ClipNodeSnapshot,
        parent_scroll: ScrollNodeSnapshot,
        parent_clip: ClipNodeSnapshot,
    ) -> bool {
        self.owner != parent_scroll.owner
            && parent_scroll.has_canonical_geometry_with_contents_clip(parent_clip)
            && self.has_canonical_geometry_with_contents_clip_and_parents(
                clip,
                Some(parent_scroll.id),
                Some(parent_clip.id),
            )
    }

    /// Boundary-local parent edge gate for an arbitrary-depth scroll forest.
    /// The complete ancestor chain is sealed by the forest planner, so this
    /// validates exactly one edge instead of requiring the immediate parent
    /// to be a parentless B0 root.
    pub(crate) fn has_canonical_geometry_with_contents_clip_parent_ids(
        self,
        clip: ClipNodeSnapshot,
        expected_scroll_parent: Option<ScrollNodeId>,
        expected_clip_parent: Option<ClipNodeId>,
    ) -> bool {
        self.has_canonical_geometry_with_contents_clip_and_parents(
            clip,
            expected_scroll_parent,
            expected_clip_parent,
        )
    }

    fn has_canonical_geometry_with_contents_clip_and_parents(
        self,
        clip: ClipNodeSnapshot,
        expected_scroll_parent: Option<ScrollNodeId>,
        expected_clip_parent: Option<ClipNodeId>,
    ) -> bool {
        let overlay = self.scrollbar_overlay;
        let can_scroll_x = matches!(
            self.configured_axis,
            ScrollAxisSnapshot::Horizontal | ScrollAxisSnapshot::Both
        ) && self.content_size.width > self.viewport.width;
        let can_scroll_y = matches!(
            self.configured_axis,
            ScrollAxisSnapshot::Vertical | ScrollAxisSnapshot::Both
        ) && self.content_size.height > self.viewport.height;
        let expected_vertical = can_scroll_y
            .then(|| {
                canonical_vertical_scrollbar_geometry(
                    self.viewport,
                    self.content_size.height,
                    self.offset.y,
                    can_scroll_x,
                )
            })
            .flatten();
        let expected_horizontal = can_scroll_x
            .then(|| {
                canonical_horizontal_scrollbar_geometry(
                    self.viewport,
                    self.content_size.width,
                    self.offset.x,
                    can_scroll_y,
                )
            })
            .flatten();
        let overlay_is_exact = scrollbar_geometry_pair_bits_equal(
            expected_vertical,
            overlay.vertical_track,
            overlay.vertical_thumb,
        ) && scrollbar_geometry_pair_bits_equal(
            expected_horizontal,
            overlay.horizontal_track,
            overlay.horizontal_thumb,
        );

        self.has_canonical_base_geometry_with_contents_clip(
            clip,
            expected_scroll_parent,
            expected_clip_parent,
        ) && overlay_is_exact
    }

    /// M10E1A hidden-overlay compiler gate.
    /// Geometry validation reuses the same M10E0 snapshot validator used by
    /// property-tree synchronization; callers cannot substitute planner-only
    /// assumptions for compiler authority.
    pub(crate) fn is_canonical_with_contents_clip(self, clip: ClipNodeSnapshot) -> bool {
        self.has_canonical_geometry_with_contents_clip(clip)
            && matches!(
                self.scrollbar_overlay.paint_state,
                ScrollbarPaintStateWitness::HiddenNow | ScrollbarPaintStateWitness::NotPaintable
            )
    }

    pub(crate) fn is_canonical_painted_with_contents_clip(self, clip: ClipNodeSnapshot) -> bool {
        self.has_canonical_geometry_with_contents_clip(clip)
            && matches!(
                self.scrollbar_overlay.paint_state,
                ScrollbarPaintStateWitness::OpaqueNow | ScrollbarPaintStateWitness::TranslucentNow
            )
    }

    fn has_canonical_base_geometry_with_contents_clip(
        self,
        clip: ClipNodeSnapshot,
        expected_scroll_parent: Option<ScrollNodeId>,
        expected_clip_parent: Option<ClipNodeId>,
    ) -> bool {
        let geometry = ScrollGeometrySnapshot {
            configured_axis: self.configured_axis,
            offset: [self.offset.x, self.offset.y],
            scrollport_rect: self.viewport,
            content_size: [self.content_size.width, self.content_size.height],
            layout_content_bounds_at_zero: self.layout_content_bounds_at_zero,
            contents_clip: self.contents_clip,
            scrollbar_overlay: self.scrollbar_overlay,
        };
        let geometry_is_non_negative = [
            self.offset.x,
            self.offset.y,
            self.viewport.x,
            self.viewport.y,
            self.viewport.width,
            self.viewport.height,
            self.content_size.width,
            self.content_size.height,
            self.layout_content_bounds_at_zero.x,
            self.layout_content_bounds_at_zero.y,
            self.layout_content_bounds_at_zero.width,
            self.layout_content_bounds_at_zero.height,
        ]
        .into_iter()
        .all(|value| value.is_finite() && value >= 0.0);
        geometry_is_non_negative
            && self.id.0 == self.owner
            && self.parent == expected_scroll_parent
            && self.generation != 0
            && clip.id.owner == self.owner
            && clip.id.role == ClipNodeRole::ContentsClip
            && clip.owner == self.owner
            && clip.parent == expected_clip_parent
            && clip.behavior == ClipBehavior::Intersect
            && clip.generation != 0
            && self.contents_clip == ScrollContentsClipWitness::ExactRect(clip.logical_scissor)
            && scroll_geometry_snapshot_is_valid(geometry)
    }

    /// Compatibility spelling for retained-scroll consumers that have not
    /// yet migrated their API names. Semantics are the complete 2D contract.
    pub(crate) fn has_canonical_vertical_geometry_with_contents_clip(
        self,
        clip: ClipNodeSnapshot,
    ) -> bool {
        self.has_canonical_geometry_with_contents_clip(clip)
    }

    /// Compatibility spelling for nested consumers. The configured axes of
    /// each node are independent interaction/overlay metadata.
    pub(crate) fn has_canonical_nested_vertical_geometry_with_contents_clip(
        self,
        clip: ClipNodeSnapshot,
        parent_scroll: ScrollNodeSnapshot,
        parent_clip: ClipNodeSnapshot,
    ) -> bool {
        self.has_canonical_nested_geometry_with_contents_clip(clip, parent_scroll, parent_clip)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PropertyChangeFlags(u8);

impl PropertyChangeFlags {
    pub(crate) const NONE: Self = Self(0);
    pub(crate) const TRANSFORM: Self = Self(1 << 0);
    pub(crate) const CLIP: Self = Self(1 << 1);
    pub(crate) const EFFECT: Self = Self(1 << 2);
    pub(crate) const SCROLL: Self = Self(1 << 3);
    pub(crate) const TOPOLOGY: Self = Self(1 << 4);

    pub(crate) const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

/// Retained shadow property trees owned by the viewport compositor.
///
/// Transform nodes remain observational. Clip authority is intentionally
/// limited to the exact legacy-resolved `AnchorParent` scissor of a leaf
/// `Element`; broader self/contents/deferred scopes remain legacy. Effect
/// opacity and scroll identity continue to mirror the existing contracts.
#[derive(Default)]
pub(crate) struct PropertyTrees {
    pub(crate) transforms: FxHashMap<TransformNodeId, TransformNode>,
    pub(crate) clips: FxHashMap<ClipNodeId, ClipNode>,
    pub(crate) effects: FxHashMap<EffectNodeId, EffectNode>,
    pub(crate) scrolls: FxHashMap<ScrollNodeId, ScrollNode>,
    transform_generations: FxHashMap<TransformNodeId, u64>,
    clip_generations: FxHashMap<ClipNodeId, u64>,
    effect_generations: FxHashMap<EffectNodeId, u64>,
    scroll_generations: FxHashMap<ScrollNodeId, u64>,
    pub(crate) states: FxHashMap<NodeKey, NodePropertyState>,
    pub(crate) changes: FxHashMap<NodeKey, PropertyChangeFlags>,
    pub(crate) validation_errors: Vec<PropertyTreeValidationError>,
    pub(crate) epoch: u64,
}

impl PropertyTrees {
    #[cfg(test)]
    pub(crate) fn epoch(&self) -> u64 {
        self.epoch
    }

    pub(super) fn effect_generation_for_owner(&self, owner: NodeKey) -> Option<u64> {
        self.effect_generations.get(&EffectNodeId(owner)).copied()
    }

    pub(super) fn transform_generation_for_owner(&self, owner: NodeKey) -> Option<u64> {
        self.transform_generations
            .get(&TransformNodeId(owner))
            .copied()
    }

    pub(crate) fn transform_snapshot_for(
        &self,
        id: TransformNodeId,
    ) -> Option<TransformNodeSnapshot> {
        let node = self.transforms.get(&id)?;
        Some(TransformNodeSnapshot {
            id,
            owner: node.owner,
            parent: node.parent,
            viewport_matrix: node.viewport_matrix,
            generation: node.generation,
        })
    }

    pub(super) fn scroll_generation_for_owner(&self, owner: NodeKey) -> Option<u64> {
        self.scroll_generations.get(&ScrollNodeId(owner)).copied()
    }

    pub(crate) fn scroll_snapshot_for(&self, id: ScrollNodeId) -> Option<ScrollNodeSnapshot> {
        let node = self.scrolls.get(&id)?;
        Some(ScrollNodeSnapshot {
            id,
            owner: node.owner,
            parent: node.parent,
            offset: node.offset,
            configured_axis: node.configured_axis,
            viewport: node.viewport,
            content_size: node.content_size,
            layout_content_bounds_at_zero: node.layout_content_bounds_at_zero,
            scrollbar_overlay: node.scrollbar_overlay,
            contents_clip: node.contents_clip,
            generation: node.generation,
        })
    }

    pub(crate) fn paint_state_for(&self, owner: NodeKey) -> Option<PropertyTreeState> {
        self.states.get(&owner).map(|state| state.paint)
    }

    pub(crate) fn authoritative_self_clip_for_owner(
        &self,
        owner: NodeKey,
        properties: PropertyTreeState,
    ) -> Option<ClipNodeId> {
        let id = ClipNodeId {
            owner,
            role: ClipNodeRole::SelfClip,
        };
        (properties.clip == Some(id)
            && self.clips.get(&id).is_some_and(|clip| {
                clip.owner == owner
                    && clip.behavior == ClipBehavior::Replace
                    && matches!(clip.geometry, ClipGeometry::LogicalScissor(_))
                    && clip.generation != 0
            }))
        .then_some(id)
    }

    pub(crate) fn node_state_for(&self, owner: NodeKey) -> Option<NodePropertyState> {
        self.states.get(&owner).copied()
    }

    pub(crate) fn clip_snapshot_for(
        &self,
        leaf: Option<ClipNodeId>,
    ) -> Option<Vec<ClipNodeSnapshot>> {
        let mut snapshots = Vec::new();
        let mut seen = FxHashSet::default();
        let mut cursor = leaf;
        while let Some(id) = cursor {
            if !seen.insert(id) || snapshots.len() >= usize::from(u8::MAX) {
                return None;
            }
            let node = self.clips.get(&id)?;
            let ClipGeometry::LogicalScissor(logical_scissor) = node.geometry else {
                return None;
            };
            snapshots.push(ClipNodeSnapshot {
                id,
                owner: node.owner,
                parent: node.parent,
                logical_scissor,
                behavior: node.behavior,
                generation: node.generation,
            });
            cursor = node.parent;
        }
        Some(snapshots)
    }

    /// Returns the complete leaf-to-root effect chain. `None` means the
    /// retained tree is incomplete, cyclic, or exceeds the bounded depth and
    /// therefore cannot be captured safely into an owning artifact.
    pub(crate) fn effect_snapshot_for(
        &self,
        leaf: Option<EffectNodeId>,
    ) -> Option<Vec<EffectNodeSnapshot>> {
        let mut snapshots = Vec::new();
        let mut seen = FxHashSet::default();
        let mut cursor = leaf;
        while let Some(id) = cursor {
            if !seen.insert(id) || snapshots.len() >= usize::from(u8::MAX) {
                return None;
            }
            let node = self.effects.get(&id)?;
            snapshots.push(EffectNodeSnapshot {
                id,
                owner: node.owner,
                parent: node.parent,
                opacity: node.opacity,
                generation: node.generation,
            });
            cursor = node.parent;
        }
        Some(snapshots)
    }

    pub(crate) fn sync(&mut self, arena: &NodeArena, roots: &[NodeKey]) {
        self.epoch = self.epoch.wrapping_add(1);
        self.changes.clear();
        self.validation_errors.clear();
        let mut seen = FxHashSet::default();
        for &root in roots {
            self.sync_subtree(arena, root, PropertyTreeState::default(), true, &mut seen);
        }
        self.prune_unseen(arena, &seen);
    }

    fn sync_subtree(
        &mut self,
        arena: &NodeArena,
        key: NodeKey,
        inherited: PropertyTreeState,
        is_frame_root: bool,
        seen: &mut FxHashSet<NodeKey>,
    ) {
        let Some(node) = arena.get(key) else {
            return;
        };
        if !seen.insert(key) {
            return;
        }

        let transform = if let Some(snapshot) =
            node.element.compositor_viewport_transform_snapshot()
        {
            let id = TransformNodeId(key);
            let previous = self.transforms.get(&id).copied();
            let parent_changed = previous.is_some_and(|node| node.parent != inherited.transform);
            let matrix = snapshot.to_cols_array();
            let matrix_changed =
                previous.is_none_or(|node| !matrix_bits_equal(node.viewport_matrix, matrix));
            let changed = parent_changed || matrix_changed;
            let generation = if changed {
                self.bump_transform_generation(id)
            } else {
                previous
                    .map(|node| node.generation)
                    .unwrap_or_else(|| self.bump_transform_generation(id))
            };
            if matrix.iter().any(|value| !value.is_finite()) {
                self.validation_errors
                    .push(PropertyTreeValidationError::NonFiniteTransform(key));
            }
            self.transforms.insert(
                id,
                TransformNode {
                    owner: key,
                    parent: inherited.transform,
                    viewport_matrix: Mat4::from_cols_array(&matrix),
                    generation,
                },
            );
            if changed {
                self.mark_change(key, PropertyChangeFlags::TRANSFORM);
                if previous.is_none() || parent_changed {
                    self.mark_change(key, PropertyChangeFlags::TOPOLOGY);
                }
            }
            Some(id)
        } else {
            let id = TransformNodeId(key);
            if self.transforms.remove(&id).is_some() {
                self.bump_transform_generation(id);
                self.mark_change(
                    key,
                    PropertyChangeFlags::TRANSFORM.union(PropertyChangeFlags::TOPOLOGY),
                );
            }
            inherited.transform
        };

        let properties = node.element.retained_paint_properties();
        let scroll_snapshot = if properties.is_scroll_container {
            match node.element.scroll_geometry_observation(key, arena) {
                ScrollGeometryObservation::Exact(snapshot)
                    if scroll_geometry_snapshot_is_valid(snapshot) =>
                {
                    Some(snapshot)
                }
                ScrollGeometryObservation::Exact(_) => {
                    self.validation_errors.push(
                        PropertyTreeValidationError::InvalidScrollGeometrySnapshot(key),
                    );
                    None
                }
                ScrollGeometryObservation::Unsupported => {
                    self.validation_errors
                        .push(PropertyTreeValidationError::ScrollContractUnavailable(key));
                    None
                }
                ScrollGeometryObservation::Inactive => None,
            }
        } else {
            None
        };
        let opacity = properties.opacity.clamp(0.0, 1.0);
        let effect = if opacity.to_bits() == 1.0_f32.to_bits() {
            if self.effects.remove(&EffectNodeId(key)).is_some() {
                self.bump_effect_generation(EffectNodeId(key));
                self.mark_change(
                    key,
                    PropertyChangeFlags::EFFECT.union(PropertyChangeFlags::TOPOLOGY),
                );
            }
            inherited.effect
        } else {
            let id = EffectNodeId(key);
            let previous = self.effects.get(&id).copied();
            let parent_changed = previous.is_some_and(|node| node.parent != inherited.effect);
            let changed = match previous {
                Some(previous) => parent_changed || previous.opacity.to_bits() != opacity.to_bits(),
                None => true,
            };
            let generation = if changed {
                self.bump_effect_generation(id)
            } else {
                previous
                    .map(|node| node.generation)
                    .unwrap_or_else(|| self.bump_effect_generation(id))
            };
            self.effects.insert(
                id,
                EffectNode {
                    owner: key,
                    parent: inherited.effect,
                    opacity,
                    generation,
                },
            );
            if changed {
                self.mark_change(key, PropertyChangeFlags::EFFECT);
                if previous.is_none() || parent_changed {
                    self.mark_change(key, PropertyChangeFlags::TOPOLOGY);
                }
            }
            Some(id)
        };

        let clip = node
            .element
            .exact_retained_self_clip_scissor_rect(key, arena, is_frame_root);
        let clip = if let Some(logical_scissor) = clip {
            let id = ClipNodeId {
                owner: key,
                role: ClipNodeRole::SelfClip,
            };
            let previous = self.clips.get(&id).copied();
            let geometry = ClipGeometry::LogicalScissor(logical_scissor);
            let parent_changed = previous.is_some_and(|node| node.parent != inherited.clip);
            let changed = previous.is_none_or(|node| {
                parent_changed
                    || !matches!(
                        node.geometry,
                        ClipGeometry::LogicalScissor(previous) if previous == logical_scissor
                    )
                    || node.behavior != ClipBehavior::Replace
            });
            let generation = if changed {
                self.bump_clip_generation(id)
            } else {
                previous
                    .map(|node| node.generation)
                    .unwrap_or_else(|| self.bump_clip_generation(id))
            };
            self.clips.insert(
                id,
                ClipNode {
                    owner: key,
                    parent: inherited.clip,
                    geometry,
                    behavior: ClipBehavior::Replace,
                    generation,
                },
            );
            if changed {
                self.mark_change(key, PropertyChangeFlags::CLIP);
            }
            if previous.is_none() || parent_changed {
                self.mark_change(key, PropertyChangeFlags::TOPOLOGY);
            }
            Some(id)
        } else {
            let id = ClipNodeId {
                owner: key,
                role: ClipNodeRole::SelfClip,
            };
            if self.clips.remove(&id).is_some() {
                self.bump_clip_generation(id);
                self.mark_change(
                    key,
                    PropertyChangeFlags::CLIP.union(PropertyChangeFlags::TOPOLOGY),
                );
            }
            inherited.clip
        };

        let paint = PropertyTreeState {
            transform,
            clip,
            effect,
            ..inherited
        };
        // A declared scroll container is atomic: its clip comes only from the
        // same validated owning snapshot as its scroll node. Never combine a
        // malformed/missing scroll snapshot with the generic clip hook.
        let contents_clip = if properties.is_scroll_container {
            scroll_snapshot.map(|snapshot| match snapshot.contents_clip {
                ScrollContentsClipWitness::ExactRect(scissor) => scissor,
            })
        } else {
            node.element.contents_logical_scissor()
        };
        let contents_clip = if let Some(logical_scissor) = contents_clip {
            let id = ClipNodeId {
                owner: key,
                role: ClipNodeRole::ContentsClip,
            };
            let previous = self.clips.get(&id).copied();
            let geometry = ClipGeometry::LogicalScissor(logical_scissor);
            let parent_changed = previous.is_some_and(|node| node.parent != paint.clip);
            let changed = previous.is_none_or(|node| {
                parent_changed
                    || !matches!(
                        node.geometry,
                        ClipGeometry::LogicalScissor(previous) if previous == logical_scissor
                    )
                    || node.behavior != ClipBehavior::Intersect
            });
            let generation = if changed {
                self.bump_clip_generation(id)
            } else {
                previous
                    .map(|node| node.generation)
                    .unwrap_or_else(|| self.bump_clip_generation(id))
            };
            self.clips.insert(
                id,
                ClipNode {
                    owner: key,
                    parent: paint.clip,
                    geometry,
                    behavior: ClipBehavior::Intersect,
                    generation,
                },
            );
            if changed {
                self.mark_change(key, PropertyChangeFlags::CLIP);
            }
            if previous.is_none() || parent_changed {
                self.mark_change(key, PropertyChangeFlags::TOPOLOGY);
            }
            Some(id)
        } else {
            let id = ClipNodeId {
                owner: key,
                role: ClipNodeRole::ContentsClip,
            };
            if self.clips.remove(&id).is_some() {
                self.bump_clip_generation(id);
                self.mark_change(
                    key,
                    PropertyChangeFlags::CLIP.union(PropertyChangeFlags::TOPOLOGY),
                );
            }
            paint.clip
        };
        let scroll = if let Some(snapshot) = scroll_snapshot {
            let id = ScrollNodeId(key);
            let offset = Vec2::from_array(snapshot.offset);
            let prior = self.scrolls.get(&id).copied();
            let parent_changed = prior.is_some_and(|previous| previous.parent != inherited.scroll);
            let changed = prior.is_none_or(|previous| {
                parent_changed || !scroll_node_payload_equal(previous, snapshot)
            });
            let generation = if changed {
                self.bump_scroll_generation(id)
            } else {
                prior
                    .map(|node| node.generation)
                    .unwrap_or_else(|| self.bump_scroll_generation(id))
            };
            self.scrolls.insert(
                id,
                ScrollNode {
                    owner: key,
                    parent: inherited.scroll,
                    offset,
                    configured_axis: snapshot.configured_axis,
                    viewport: snapshot.scrollport_rect,
                    content_size: Size {
                        width: snapshot.content_size[0],
                        height: snapshot.content_size[1],
                    },
                    layout_content_bounds_at_zero: snapshot.layout_content_bounds_at_zero,
                    scrollbar_overlay: snapshot.scrollbar_overlay,
                    contents_clip: snapshot.contents_clip,
                    generation,
                },
            );
            if changed {
                self.mark_change(key, PropertyChangeFlags::SCROLL);
            }
            if parent_changed {
                self.mark_change(key, PropertyChangeFlags::TOPOLOGY);
            }
            Some(id)
        } else {
            if self.scrolls.remove(&ScrollNodeId(key)).is_some() {
                self.bump_scroll_generation(ScrollNodeId(key));
                self.mark_change(
                    key,
                    PropertyChangeFlags::SCROLL.union(PropertyChangeFlags::TOPOLOGY),
                );
            }
            inherited.scroll
        };
        let descendants = PropertyTreeState {
            clip: contents_clip,
            scroll,
            ..paint
        };
        let next_state = NodePropertyState { paint, descendants };
        if self
            .states
            .get(&key)
            .is_none_or(|previous| *previous != next_state)
        {
            self.mark_change(key, PropertyChangeFlags::TOPOLOGY);
        }
        self.states.insert(key, next_state);

        let children = node.children().to_vec();
        drop(node);
        for child in children {
            self.sync_subtree(arena, child, descendants, false, seen);
        }
    }

    fn mark_change(&mut self, key: NodeKey, flags: PropertyChangeFlags) {
        self.changes
            .entry(key)
            .and_modify(|current| *current = current.union(flags))
            .or_insert(flags);
    }

    fn bump_effect_generation(&mut self, id: EffectNodeId) -> u64 {
        let generation = self.effect_generations.entry(id).or_insert(0);
        *generation = generation.saturating_add(1);
        *generation
    }

    fn bump_transform_generation(&mut self, id: TransformNodeId) -> u64 {
        let generation = self.transform_generations.entry(id).or_insert(0);
        *generation = generation.saturating_add(1);
        *generation
    }

    fn bump_clip_generation(&mut self, id: ClipNodeId) -> u64 {
        let generation = self.clip_generations.entry(id).or_insert(0);
        *generation = generation.saturating_add(1);
        *generation
    }

    fn bump_scroll_generation(&mut self, id: ScrollNodeId) -> u64 {
        let generation = self.scroll_generations.entry(id).or_insert(0);
        *generation = generation.saturating_add(1);
        *generation
    }

    fn prune_unseen(&mut self, arena: &NodeArena, seen: &FxHashSet<NodeKey>) {
        self.transforms.retain(|id, _| seen.contains(&id.0));
        self.clips.retain(|id, _| seen.contains(&id.owner));
        self.effects.retain(|id, _| seen.contains(&id.0));
        self.scrolls.retain(|id, _| seen.contains(&id.0));
        // Active property state follows the current roots, but tombstone
        // counters follow the owner's generational arena lifetime. A node can
        // temporarily leave the active root set and later reattach with the
        // same NodeKey; its generations must remain monotonic across that gap.
        self.transform_generations
            .retain(|id, _| arena.contains_key(id.0));
        self.effect_generations
            .retain(|id, _| arena.contains_key(id.0));
        self.clip_generations
            .retain(|id, _| arena.contains_key(id.owner));
        self.scroll_generations
            .retain(|id, _| arena.contains_key(id.0));
        self.states.retain(|key, _| seen.contains(key));
    }

    #[cfg(test)]
    fn changes_for(&self, key: NodeKey) -> PropertyChangeFlags {
        self.changes
            .get(&key)
            .copied()
            .unwrap_or(PropertyChangeFlags::NONE)
    }
}

fn matrix_bits_equal(matrix: Mat4, snapshot: [f32; 16]) -> bool {
    matrix
        .to_cols_array()
        .into_iter()
        .zip(snapshot)
        .all(|(left, right)| left.to_bits() == right.to_bits())
}

fn f32_bits_equal(left: f32, right: f32) -> bool {
    left.to_bits() == right.to_bits()
}

fn rect_bits_equal(left: Rect, right: Rect) -> bool {
    f32_bits_equal(left.x, right.x)
        && f32_bits_equal(left.y, right.y)
        && f32_bits_equal(left.width, right.width)
        && f32_bits_equal(left.height, right.height)
}

fn optional_rect_bits_equal(left: Option<Rect>, right: Option<Rect>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => rect_bits_equal(left, right),
        (None, None) => true,
        _ => false,
    }
}

fn scrollbar_geometry_pair_bits_equal(
    expected: Option<(Rect, Rect)>,
    actual_track: Option<Rect>,
    actual_thumb: Option<Rect>,
) -> bool {
    match (expected, actual_track, actual_thumb) {
        (Some((expected_track, expected_thumb)), Some(actual_track), Some(actual_thumb)) => {
            rect_bits_equal(expected_track, actual_track)
                && rect_bits_equal(expected_thumb, actual_thumb)
        }
        (None, None, None) => true,
        _ => false,
    }
}

fn scroll_node_payload_equal(node: ScrollNode, snapshot: ScrollGeometrySnapshot) -> bool {
    node.configured_axis == snapshot.configured_axis
        && f32_bits_equal(node.offset.x, snapshot.offset[0])
        && f32_bits_equal(node.offset.y, snapshot.offset[1])
        && rect_bits_equal(node.viewport, snapshot.scrollport_rect)
        && f32_bits_equal(node.content_size.width, snapshot.content_size[0])
        && f32_bits_equal(node.content_size.height, snapshot.content_size[1])
        && rect_bits_equal(
            node.layout_content_bounds_at_zero,
            snapshot.layout_content_bounds_at_zero,
        )
        && scrollbar_overlay_bits_equal(node.scrollbar_overlay, snapshot.scrollbar_overlay)
        && node.contents_clip == snapshot.contents_clip
}

fn scrollbar_overlay_bits_equal(
    left: ScrollbarOverlayWitness,
    right: ScrollbarOverlayWitness,
) -> bool {
    optional_rect_bits_equal(left.vertical_track, right.vertical_track)
        && optional_rect_bits_equal(left.vertical_thumb, right.vertical_thumb)
        && optional_rect_bits_equal(left.horizontal_track, right.horizontal_track)
        && optional_rect_bits_equal(left.horizontal_thumb, right.horizontal_thumb)
        && left.interaction == right.interaction
        && left.paint_state == right.paint_state
        && f32_bits_equal(left.sampled_alpha, right.sampled_alpha)
        && f32_bits_equal(left.shadow_blur_radius, right.shadow_blur_radius)
}

fn rect_is_finite_non_negative(rect: Rect) -> bool {
    rect.x.is_finite()
        && rect.y.is_finite()
        && rect.width.is_finite()
        && rect.height.is_finite()
        && rect.width >= 0.0
        && rect.height >= 0.0
        && (rect.x + rect.width).is_finite()
        && (rect.y + rect.height).is_finite()
}

fn rect_contains_rect(outer: Rect, inner: Rect) -> bool {
    rect_is_finite_non_negative(outer)
        && rect_is_finite_non_negative(inner)
        && inner.x >= outer.x
        && inner.y >= outer.y
        && inner.x + inner.width <= outer.x + outer.width
        && inner.y + inner.height <= outer.y + outer.height
}

fn scroll_geometry_snapshot_is_valid(snapshot: ScrollGeometrySnapshot) -> bool {
    let viewport = snapshot.scrollport_rect;
    let bounds = snapshot.layout_content_bounds_at_zero;
    if !rect_is_finite_non_negative(viewport)
        || viewport.width <= 0.0
        || viewport.height <= 0.0
        || !rect_is_finite_non_negative(bounds)
        || snapshot.content_size.iter().any(|value| !value.is_finite())
        || snapshot.content_size[0] < viewport.width
        || snapshot.content_size[1] < viewport.height
        || !f32_bits_equal(bounds.x, viewport.x)
        || !f32_bits_equal(bounds.y, viewport.y)
        || !f32_bits_equal(bounds.width, snapshot.content_size[0])
        || !f32_bits_equal(bounds.height, snapshot.content_size[1])
        || snapshot
            .offset
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0)
    {
        return false;
    }

    let max_x = (snapshot.content_size[0] - viewport.width).max(0.0);
    let max_y = (snapshot.content_size[1] - viewport.height).max(0.0);
    if snapshot.offset[0] > max_x || snapshot.offset[1] > max_y {
        return false;
    }

    let ScrollContentsClipWitness::ExactRect(scissor) = snapshot.contents_clip;
    // The witness remains clip authority. This shared helper is only an
    // internal-consistency mirror of the exact legacy conversion.
    if exact_logical_scissor_for_rect(viewport) != Some(scissor) {
        return false;
    }

    let overlay = snapshot.scrollbar_overlay;
    if !overlay.shadow_blur_radius.is_finite()
        || overlay.shadow_blur_radius < 0.0
        || !overlay.sampled_alpha.is_finite()
        || !(0.0..=1.0).contains(&overlay.sampled_alpha)
        || overlay.vertical_track.is_some() != overlay.vertical_thumb.is_some()
        || overlay.horizontal_track.is_some() != overlay.horizontal_thumb.is_some()
        || overlay
            .vertical_track
            .zip(overlay.vertical_thumb)
            .is_some_and(|(track, thumb)| !rect_contains_rect(track, thumb))
        || overlay
            .horizontal_track
            .zip(overlay.horizontal_thumb)
            .is_some_and(|(track, thumb)| !rect_contains_rect(track, thumb))
        || (overlay.vertical_track.is_some()
            && !matches!(
                snapshot.configured_axis,
                ScrollAxisSnapshot::Vertical | ScrollAxisSnapshot::Both
            ))
        || (overlay.horizontal_track.is_some()
            && !matches!(
                snapshot.configured_axis,
                ScrollAxisSnapshot::Horizontal | ScrollAxisSnapshot::Both
            ))
    {
        return false;
    }
    let has_geometry = overlay.vertical_track.is_some() || overlay.horizontal_track.is_some();
    if matches!(
        overlay.interaction.dragging_axis,
        Some(ScrollAxisSnapshot::Both)
    ) || (matches!(
        overlay.interaction.dragging_axis,
        Some(ScrollAxisSnapshot::Vertical)
    ) && overlay.vertical_track.is_none())
        || (matches!(
            overlay.interaction.dragging_axis,
            Some(ScrollAxisSnapshot::Horizontal)
        ) && overlay.horizontal_track.is_none())
    {
        return false;
    }
    let alpha_is_hidden = overlay.sampled_alpha.to_bits() == 0.0_f32.to_bits();
    let alpha_is_opaque = overlay.sampled_alpha.to_bits() == 1.0_f32.to_bits();
    let forced_opaque = overlay.interaction.hovered || overlay.interaction.dragging_axis.is_some();
    if (has_geometry && forced_opaque && !alpha_is_opaque)
        || (has_geometry
            && !forced_opaque
            && overlay.interaction.has_interaction_timestamp != !alpha_is_hidden)
    {
        return false;
    }
    let expected_paint_state = if !has_geometry {
        ScrollbarPaintStateWitness::NotPaintable
    } else if alpha_is_opaque {
        ScrollbarPaintStateWitness::OpaqueNow
    } else if alpha_is_hidden {
        ScrollbarPaintStateWitness::HiddenNow
    } else {
        ScrollbarPaintStateWitness::TranslucentNow
    };
    overlay.paint_state == expected_paint_state && (has_geometry || alpha_is_hidden)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::{
        ClipMode, Layout, Length, Opacity, ParsedValue, Position, PropertyId, ScrollDirection,
        Style, Transform, TransformEntry, Translate,
    };
    use crate::view::base_component::text_area::{
        TextAreaLineBreak, TextAreaProjectionSegment, TextAreaTextRun,
    };
    use crate::view::base_component::{
        DirtyFlags, DirtyPassMask, Element, ElementTrait, Image, ScrollbarPaintStateWitness, Svg,
        Text, TextArea,
    };
    use crate::view::node_arena::Node;
    use crate::view::{ImageSource, SvgSource};
    use std::sync::Arc;

    use crate::view::test_support::{commit_element, measure_and_place, new_test_arena};

    struct NeutralCustomHost;

    impl crate::view::base_component::Layoutable for NeutralCustomHost {
        fn measure(
            &mut self,
            _constraints: crate::view::base_component::LayoutConstraints,
            _arena: &mut NodeArena,
        ) {
        }

        fn place(
            &mut self,
            _placement: crate::view::base_component::LayoutPlacement,
            _arena: &mut NodeArena,
        ) {
        }

        fn measured_size(&self) -> (f32, f32) {
            (0.0, 0.0)
        }

        fn set_layout_width(&mut self, _width: f32) {}

        fn set_layout_height(&mut self, _height: f32) {}
    }

    impl crate::view::base_component::EventTarget for NeutralCustomHost {}

    impl crate::view::base_component::Renderable for NeutralCustomHost {
        fn build(
            &mut self,
            _graph: &mut crate::view::frame_graph::FrameGraph,
            _arena: &mut NodeArena,
            ctx: crate::view::base_component::UiBuildContext,
        ) -> crate::view::base_component::BuildState {
            ctx.into_state()
        }
    }

    impl crate::view::base_component::ElementTrait for NeutralCustomHost {
        fn stable_id(&self) -> u64 {
            99
        }

        fn box_model_snapshot(&self) -> crate::view::base_component::BoxModelSnapshot {
            crate::view::base_component::BoxModelSnapshot {
                node_id: 99,
                parent_id: None,
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 0.0,
                border_radius: 0.0,
                should_render: false,
            }
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }

        fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
            self
        }
    }

    struct ContentsClipHost {
        id: u64,
        scissor: Option<[u32; 4]>,
        children: Vec<NodeKey>,
        declares_scroll: bool,
    }

    impl crate::view::base_component::Layoutable for ContentsClipHost {
        fn measure(
            &mut self,
            _constraints: crate::view::base_component::LayoutConstraints,
            _arena: &mut NodeArena,
        ) {
        }

        fn place(
            &mut self,
            _placement: crate::view::base_component::LayoutPlacement,
            _arena: &mut NodeArena,
        ) {
        }

        fn measured_size(&self) -> (f32, f32) {
            (1.0, 1.0)
        }

        fn set_layout_width(&mut self, _width: f32) {}
        fn set_layout_height(&mut self, _height: f32) {}
    }

    impl crate::view::base_component::EventTarget for ContentsClipHost {}

    impl crate::view::base_component::Renderable for ContentsClipHost {
        fn build(
            &mut self,
            _graph: &mut crate::view::frame_graph::FrameGraph,
            _arena: &mut NodeArena,
            ctx: crate::view::base_component::UiBuildContext,
        ) -> crate::view::base_component::BuildState {
            ctx.into_state()
        }
    }

    impl crate::view::base_component::ElementTrait for ContentsClipHost {
        fn stable_id(&self) -> u64 {
            self.id
        }

        fn box_model_snapshot(&self) -> crate::view::base_component::BoxModelSnapshot {
            crate::view::base_component::BoxModelSnapshot {
                node_id: self.id,
                parent_id: None,
                x: 0.0,
                y: 0.0,
                width: 1.0,
                height: 1.0,
                border_radius: 0.0,
                should_render: true,
            }
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }

        fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
            self
        }

        fn contents_logical_scissor(&self) -> Option<[u32; 4]> {
            self.scissor
        }

        fn retained_paint_properties(
            &self,
        ) -> crate::view::base_component::RetainedPaintProperties {
            crate::view::base_component::RetainedPaintProperties {
                is_scroll_container: self.declares_scroll,
                ..Default::default()
            }
        }

        fn children(&self) -> &[NodeKey] {
            &self.children
        }

        fn sync_children_mirror(&mut self, children: &[NodeKey]) {
            self.children.clear();
            self.children.extend_from_slice(children);
        }
    }

    fn insert_contents_clip_host(
        arena: &mut NodeArena,
        id: u64,
        scissor: Option<[u32; 4]>,
    ) -> NodeKey {
        arena.insert(Node::new(Box::new(ContentsClipHost {
            id,
            scissor,
            children: Vec::new(),
            declares_scroll: false,
        })))
    }

    fn insert_missing_scroll_contract_host(arena: &mut NodeArena, id: u64) -> NodeKey {
        arena.insert(Node::new(Box::new(ContentsClipHost {
            id,
            scissor: Some([1, 2, 30, 40]),
            children: Vec::new(),
            declares_scroll: true,
        })))
    }

    fn insert_element(arena: &mut NodeArena, id: u64) -> NodeKey {
        arena.insert(Node::new(Box::new(Element::new_with_id(
            id, 0.0, 0.0, 100.0, 100.0,
        ))))
    }

    fn append_child(arena: &mut NodeArena, parent: NodeKey, child: NodeKey) {
        arena.set_parent(child, Some(parent));
        arena.push_child(parent, child);
    }

    fn set_opacity(arena: &NodeArena, key: NodeKey, opacity: f32) {
        arena
            .get_mut(key)
            .expect("element exists")
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .expect("Element")
            .set_opacity(opacity);
    }

    fn set_scroll_direction(arena: &NodeArena, key: NodeKey, direction: ScrollDirection) {
        let mut style = Style::new();
        style.insert(
            PropertyId::ScrollDirection,
            ParsedValue::ScrollDirection(direction),
        );
        arena
            .get_mut(key)
            .expect("element exists")
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .expect("Element")
            .apply_style(style);
    }

    fn install_scroll_layout_geometry(
        arena: &NodeArena,
        key: NodeKey,
        viewport: Rect,
        content_size: [f32; 2],
    ) {
        let mut node = arena.get_mut(key).expect("scroll element exists");
        let element = node
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .expect("Element");
        element.layout_state.layout_position.x = viewport.x;
        element.layout_state.layout_position.y = viewport.y;
        element.layout_state.layout_size.width = viewport.width;
        element.layout_state.layout_size.height = viewport.height;
        element.layout_state.layout_inner_position.x = viewport.x;
        element.layout_state.layout_inner_position.y = viewport.y;
        element.layout_state.layout_inner_size.width = viewport.width;
        element.layout_state.layout_inner_size.height = viewport.height;
        element.layout_state.content_size = Size {
            width: content_size[0],
            height: content_size[1],
        };
    }

    fn make_vertical_scroll_fixture(
        arena: &mut NodeArena,
        root_id: u64,
        child_id: u64,
    ) -> (NodeKey, NodeKey) {
        let root = insert_element(arena, root_id);
        let child = insert_element(arena, child_id);
        append_child(arena, root, child);
        set_scroll_direction(arena, root, ScrollDirection::Vertical);
        install_scroll_layout_geometry(
            arena,
            root,
            Rect {
                x: 10.0,
                y: 20.0,
                width: 100.0,
                height: 80.0,
            },
            [100.0, 300.0],
        );
        clear_layout_dirty_for_subtree(arena, root);
        (root, child)
    }

    fn make_nested_vertical_scroll_fixture(
        arena: &mut NodeArena,
        outer_id: u64,
        inner_id: u64,
        leaf_id: u64,
    ) -> (NodeKey, NodeKey, NodeKey) {
        let outer = insert_element(arena, outer_id);
        let inner = insert_element(arena, inner_id);
        let leaf = insert_element(arena, leaf_id);
        append_child(arena, outer, inner);
        append_child(arena, inner, leaf);
        set_scroll_direction(arena, outer, ScrollDirection::Vertical);
        set_scroll_direction(arena, inner, ScrollDirection::Vertical);
        for owner in [outer, inner] {
            let mut style = Style::new();
            style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
            arena
                .get_mut(owner)
                .unwrap()
                .element
                .as_any_mut()
                .downcast_mut::<Element>()
                .unwrap()
                .apply_style(style);
        }
        install_scroll_layout_geometry(
            arena,
            outer,
            Rect {
                x: 10.0,
                y: 20.0,
                width: 100.0,
                height: 80.0,
            },
            [100.0, 300.0],
        );
        install_scroll_layout_geometry(
            arena,
            inner,
            Rect {
                x: 10.0,
                y: 20.0,
                width: 100.0,
                height: 300.0,
            },
            [100.0, 600.0],
        );
        install_scroll_layout_geometry(
            arena,
            leaf,
            Rect {
                x: 10.0,
                y: 20.0,
                width: 100.0,
                height: 600.0,
            },
            [100.0, 600.0],
        );
        clear_layout_dirty_for_subtree(arena, outer);
        (outer, inner, leaf)
    }

    fn clear_layout_dirty_for_subtree(arena: &NodeArena, root: NodeKey) {
        fn walk(arena: &NodeArena, key: NodeKey, flags: DirtyFlags) {
            let children = arena
                .get(key)
                .map(|node| node.children().to_vec())
                .unwrap_or_default();
            if let Some(mut node) = arena.get_mut(key) {
                node.element.clear_local_dirty_flags(flags);
            }
            for child in children {
                walk(arena, child, flags);
            }
        }
        let flags = DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT);
        walk(arena, root, flags);
        arena.refresh_subtree_dirty_cache(root);
    }

    fn set_transform(arena: &NodeArena, key: NodeKey, transform: Transform) {
        let mut style = Style::new();
        style.set_transform(transform);
        arena
            .get_mut(key)
            .expect("element exists")
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .expect("Element")
            .apply_style(style);
    }

    fn translate_x(value: f32) -> Transform {
        Transform::new([Translate::x(Length::px(value))])
    }

    fn matrix_bits(matrix: Mat4) -> [u32; 16] {
        matrix.to_cols_array().map(f32::to_bits)
    }

    fn opacity_style(opacity: f32) -> Style {
        let mut style = Style::new();
        style.insert(
            PropertyId::Opacity,
            ParsedValue::Opacity(Opacity::new(opacity)),
        );
        style
    }

    fn anchor_parent_clip_fixture(viewport_width: f32) -> (NodeArena, NodeKey) {
        let mut element = Element::new_with_id(701, 10.25, 12.75, 80.0, 40.0);
        element.set_background_color_value(crate::style::Color::rgb(220, 40, 30));
        let mut style = Style::new();
        style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(10.25))
                    .top(Length::px(12.75))
                    .clip(ClipMode::AnchorParent),
            ),
        );
        element.apply_style(style);
        let mut arena = new_test_arena();
        let key = commit_element(&mut arena, Box::new(element));
        measure_and_place(
            &mut arena,
            key,
            crate::view::base_component::LayoutConstraints {
                max_width: viewport_width,
                max_height: 240.0,
                viewport_width,
                viewport_height: 240.0,
                percent_base_width: Some(viewport_width),
                percent_base_height: Some(240.0),
            },
            crate::view::base_component::LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: viewport_width,
                available_height: 240.0,
                viewport_width,
                viewport_height: 240.0,
                percent_base_width: Some(viewport_width),
                percent_base_height: Some(240.0),
            },
        );
        (arena, key)
    }

    fn nested_anchor_parent_fixture(anchor_first: bool) -> (NodeArena, NodeKey, NodeKey, NodeKey) {
        let mut arena = new_test_arena();
        let mut parent = Element::new_with_id(0x8d00, 0.0, 0.0, 320.0, 240.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        parent.apply_style(parent_style);
        let parent = commit_element(&mut arena, Box::new(parent));

        let child = |id, mode| {
            let mut child = Element::new_with_id(id, 0.0, 0.0, 40.0, 30.0);
            let mut style = Style::new();
            style.insert(
                PropertyId::Position,
                ParsedValue::Position(
                    Position::absolute()
                        .left(Length::px(8.0))
                        .top(Length::px(9.0))
                        .clip(mode),
                ),
            );
            child.apply_style(style);
            child
        };
        let (normal, anchor) = if anchor_first {
            let anchor = arena.insert(Node::with_parent(
                Box::new(child(0x8d02, ClipMode::AnchorParent)),
                Some(parent),
            ));
            let normal = arena.insert(Node::with_parent(
                Box::new(child(0x8d01, ClipMode::Parent)),
                Some(parent),
            ));
            arena.set_children(parent, vec![anchor, normal]);
            (normal, anchor)
        } else {
            let normal = arena.insert(Node::with_parent(
                Box::new(child(0x8d01, ClipMode::Parent)),
                Some(parent),
            ));
            let anchor = arena.insert(Node::with_parent(
                Box::new(child(0x8d02, ClipMode::AnchorParent)),
                Some(parent),
            ));
            arena.set_children(parent, vec![normal, anchor]);
            (normal, anchor)
        };
        let constraints = crate::view::base_component::LayoutConstraints {
            max_width: 320.0,
            max_height: 240.0,
            viewport_width: 320.0,
            viewport_height: 240.0,
            percent_base_width: Some(320.0),
            percent_base_height: Some(240.0),
        };
        let placement = crate::view::base_component::LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 320.0,
            available_height: 240.0,
            viewport_width: 320.0,
            viewport_height: 240.0,
            percent_base_width: Some(320.0),
            percent_base_height: Some(240.0),
        };
        measure_and_place(&mut arena, parent, constraints, placement);
        (arena, parent, normal, anchor)
    }

    fn set_clip_mode(arena: &NodeArena, key: NodeKey, mode: ClipMode) {
        let mut style = Style::new();
        style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(10.25))
                    .top(Length::px(12.75))
                    .clip(mode),
            ),
        );
        arena
            .get_mut(key)
            .expect("element exists")
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .expect("Element")
            .apply_style(style);
    }

    #[test]
    fn unchanged_sync_preserves_effect_identity_and_generation() {
        let mut arena = NodeArena::new();
        let root = insert_element(&mut arena, 1);
        set_opacity(&arena, root, 0.5);
        let mut trees = PropertyTrees::default();
        trees.sync(&arena, &[root]);
        let first = trees.effects[&EffectNodeId(root)];

        trees.sync(&arena, &[root]);

        assert_eq!(trees.states[&root].paint.effect, Some(EffectNodeId(root)));
        assert_eq!(
            trees.effects[&EffectNodeId(root)].generation,
            first.generation
        );
        assert_eq!(trees.changes_for(root), PropertyChangeFlags::NONE);
    }

    #[test]
    fn transform_state_applies_to_self_and_descendants_without_parent_multiplication() {
        let mut arena = NodeArena::new();
        let root = insert_element(&mut arena, 0x8d00);
        let child = insert_element(&mut arena, 0x8d01);
        append_child(&mut arena, root, child);
        set_transform(&arena, root, translate_x(12.0));
        set_transform(&arena, child, translate_x(7.0));
        let root_source = arena
            .get(root)
            .unwrap()
            .element
            .compositor_viewport_transform_snapshot()
            .unwrap()
            .to_cols_array()
            .map(f32::to_bits);
        let child_source = arena
            .get(child)
            .unwrap()
            .element
            .compositor_viewport_transform_snapshot()
            .unwrap()
            .to_cols_array()
            .map(f32::to_bits);

        let mut trees = PropertyTrees::default();
        trees.sync(&arena, &[root]);

        let root_id = TransformNodeId(root);
        let child_id = TransformNodeId(child);
        assert_eq!(trees.states[&root].paint.transform, Some(root_id));
        assert_eq!(trees.states[&root].descendants.transform, Some(root_id));
        assert_eq!(trees.states[&child].paint.transform, Some(child_id));
        assert_eq!(trees.states[&child].descendants.transform, Some(child_id));
        assert_eq!(trees.transforms[&root_id].parent, None);
        assert_eq!(trees.transforms[&child_id].parent, Some(root_id));
        assert_eq!(
            matrix_bits(trees.transforms[&root_id].viewport_matrix),
            root_source
        );
        assert_eq!(
            matrix_bits(trees.transforms[&child_id].viewport_matrix),
            child_source,
            "resolved viewport matrices must not be parent-multiplied during sync",
        );
    }

    #[test]
    fn transform_generation_is_bitwise_stable_and_matrix_change_is_not_topology() {
        let mut arena = NodeArena::new();
        let root = insert_element(&mut arena, 0x8d10);
        set_transform(&arena, root, translate_x(4.0));
        let id = TransformNodeId(root);
        let mut trees = PropertyTrees::default();
        trees.sync(&arena, &[root]);
        let first = trees.transforms[&id].generation;

        trees.sync(&arena, &[root]);
        assert_eq!(trees.transforms[&id].generation, first);
        assert_eq!(trees.changes_for(root), PropertyChangeFlags::NONE);

        set_transform(&arena, root, translate_x(5.0));
        trees.sync(&arena, &[root]);
        assert_eq!(trees.transforms[&id].generation, first + 1);
        let changes = trees.changes_for(root);
        assert!(changes.contains(PropertyChangeFlags::TRANSFORM));
        assert!(!changes.contains(PropertyChangeFlags::TOPOLOGY));
        assert!(!changes.contains(PropertyChangeFlags::CLIP));
        assert!(!changes.contains(PropertyChangeFlags::EFFECT));
        assert!(!changes.contains(PropertyChangeFlags::SCROLL));
    }

    #[test]
    fn transform_reparent_preserves_id_and_updates_parent_topology() {
        let mut arena = NodeArena::new();
        let left = insert_element(&mut arena, 0x8d20);
        let right = insert_element(&mut arena, 0x8d21);
        let child = insert_element(&mut arena, 0x8d22);
        set_transform(&arena, left, translate_x(1.0));
        set_transform(&arena, right, translate_x(2.0));
        set_transform(&arena, child, translate_x(3.0));
        append_child(&mut arena, left, child);
        let child_id = TransformNodeId(child);
        let mut trees = PropertyTrees::default();
        trees.sync(&arena, &[left, right]);
        let first_generation = trees.transforms[&child_id].generation;

        arena.set_children(left, Vec::new());
        arena.set_parent(child, Some(right));
        arena.push_child(right, child);
        trees.sync(&arena, &[left, right]);

        assert_eq!(
            trees.transforms[&child_id].parent,
            Some(TransformNodeId(right))
        );
        assert_eq!(trees.transforms[&child_id].generation, first_generation + 1);
        let changes = trees.changes_for(child);
        assert!(changes.contains(PropertyChangeFlags::TRANSFORM));
        assert!(changes.contains(PropertyChangeFlags::TOPOLOGY));
    }

    #[test]
    fn transform_tombstone_is_monotonic_across_remove_reinsert_and_inactive_roots() {
        let mut arena = NodeArena::new();
        let root = insert_element(&mut arena, 0x8d30);
        set_transform(&arena, root, translate_x(1.0));
        let id = TransformNodeId(root);
        let mut trees = PropertyTrees::default();
        trees.sync(&arena, &[root]);
        let first = trees.transforms[&id].generation;

        trees.sync(&arena, &[]);
        assert!(!trees.transforms.contains_key(&id));
        assert!(trees.transform_generations.contains_key(&id));
        trees.sync(&arena, &[root]);
        assert!(trees.transforms[&id].generation > first);

        set_transform(&arena, root, Transform::default());
        trees.sync(&arena, &[root]);
        let removed_generation = trees.transform_generations[&id];
        assert!(!trees.transforms.contains_key(&id));
        set_transform(&arena, root, translate_x(1.0));
        trees.sync(&arena, &[root]);
        assert!(trees.transforms[&id].generation > removed_generation);

        arena.remove(root);
        trees.sync(&arena, &[root]);
        assert!(!trees.transform_generations.contains_key(&id));
    }

    #[test]
    fn non_finite_transform_remains_a_property_boundary_and_reports_validation() {
        let mut arena = NodeArena::new();
        let root = insert_element(&mut arena, 0x8d40);
        let mut matrix = Mat4::IDENTITY.to_cols_array();
        matrix[5] = f32::NAN;
        set_transform(
            &arena,
            root,
            Transform::new([TransformEntry::from_matrix(matrix)]),
        );
        let mut trees = PropertyTrees::default();
        trees.sync(&arena, &[root]);

        assert_eq!(
            trees.states[&root].paint.transform,
            Some(TransformNodeId(root)),
            "invalid numeric payloads must not collapse to neutral",
        );
        assert_eq!(
            trees.validation_errors,
            vec![PropertyTreeValidationError::NonFiniteTransform(root)]
        );
    }

    #[test]
    fn image_and_svg_delegate_their_element_transform_snapshot() {
        let mut arena = NodeArena::new();
        let mut image = Image::new_with_id(
            0x8d50,
            crate::view::ImageSource::Rgba {
                width: 1,
                height: 1,
                pixels: Arc::from([255, 255, 255, 255]),
            },
        );
        let mut image_style = Style::new();
        image_style.set_transform(translate_x(8.0));
        image.apply_style(image_style);
        let image_key = arena.insert(Node::new(Box::new(image)));

        let mut svg = Svg::new_with_id(
            0x8d51,
            crate::view::SvgSource::Content("<svg xmlns=\"http://www.w3.org/2000/svg\"/>".into()),
        );
        let mut svg_style = Style::new();
        svg_style.set_transform(translate_x(9.0));
        svg.apply_style(svg_style);
        let svg_key = arena.insert(Node::new(Box::new(svg)));

        let mut trees = PropertyTrees::default();
        trees.sync(&arena, &[image_key, svg_key]);

        for key in [image_key, svg_key] {
            assert_eq!(
                trees.states[&key].paint.transform,
                Some(TransformNodeId(key))
            );
            let expected = arena
                .get(key)
                .unwrap()
                .element
                .compositor_viewport_transform_snapshot()
                .unwrap()
                .to_cols_array()
                .map(f32::to_bits);
            assert_eq!(
                matrix_bits(trees.transforms[&TransformNodeId(key)].viewport_matrix),
                expected
            );
        }
    }

    #[test]
    fn built_in_non_wrapper_hosts_are_explicitly_transform_neutral() {
        // These hosts do not own an `Element` and their prop/style schemas do
        // not accept CSS transforms. Keeping the inventory here prevents a new
        // transform-capable built-in from silently inheriting the neutral
        // default instead of delegating an authoritative snapshot.
        let hosts: Vec<Box<dyn ElementTrait>> = vec![
            Box::new(Text::new(0.0, 0.0, 10.0, 10.0, "text")),
            Box::new(TextArea::new()),
            Box::new(TextAreaProjectionSegment::new()),
            Box::new(TextAreaTextRun::new("run".to_string(), 0..3)),
            Box::new(TextAreaLineBreak::new(3..4)),
        ];

        for host in hosts {
            assert!(
                host.compositor_viewport_transform_snapshot().is_none(),
                "{} unexpectedly became transform-capable without an explicit delegate",
                host.element_type_name(),
            );
        }
    }

    #[test]
    fn effect_snapshot_owns_complete_leaf_to_root_chain_bit_exactly() {
        let mut arena = NodeArena::new();
        let root = insert_element(&mut arena, 10);
        let child = insert_element(&mut arena, 11);
        append_child(&mut arena, root, child);
        set_opacity(&arena, root, 0.5);
        set_opacity(&arena, child, 0.25);
        let mut trees = PropertyTrees::default();
        trees.sync(&arena, &[root]);

        let snapshots = trees
            .effect_snapshot_for(Some(EffectNodeId(child)))
            .expect("complete effect chain must snapshot");
        assert_eq!(snapshots.len(), 2);
        assert_eq!(snapshots[0].id, EffectNodeId(child));
        assert_eq!(snapshots[0].owner, child);
        assert_eq!(snapshots[0].parent, Some(EffectNodeId(root)));
        assert_eq!(snapshots[0].opacity.to_bits(), 0.25_f32.to_bits());
        assert!(snapshots[0].generation > 0);
        assert_eq!(snapshots[1].id, EffectNodeId(root));
        assert_eq!(snapshots[1].owner, root);
        assert_eq!(snapshots[1].parent, None);
        assert_eq!(snapshots[1].opacity.to_bits(), 0.5_f32.to_bits());
        assert!(snapshots[1].generation > 0);
        assert_eq!(trees.effect_snapshot_for(None), Some(Vec::new()));

        trees.effects.remove(&EffectNodeId(root));
        assert!(
            trees
                .effect_snapshot_for(Some(EffectNodeId(child)))
                .is_none()
        );
    }

    #[test]
    fn anchor_parent_self_clip_is_stable_replace_and_generation_is_monotonic() {
        let (arena, root) = anchor_parent_clip_fixture(320.0);
        let id = ClipNodeId {
            owner: root,
            role: ClipNodeRole::SelfClip,
        };
        let mut trees = PropertyTrees::default();
        trees.sync(&arena, &[root]);
        let first = trees.clips[&id];
        assert_eq!(trees.states[&root].paint.clip, Some(id));
        assert_eq!(first.owner, root);
        assert_eq!(first.parent, None);
        assert_eq!(first.behavior, ClipBehavior::Replace);
        assert!(matches!(
            first.geometry,
            ClipGeometry::LogicalScissor([0, 0, 320, 240])
        ));
        assert!(trees.changes_for(root).contains(PropertyChangeFlags::CLIP));
        assert!(
            trees
                .changes_for(root)
                .contains(PropertyChangeFlags::TOPOLOGY)
        );

        trees.sync(&arena, &[root]);
        assert_eq!(trees.clips[&id].generation, first.generation);
        assert_eq!(trees.changes_for(root), PropertyChangeFlags::NONE);

        set_clip_mode(&arena, root, ClipMode::Parent);
        trees.sync(&arena, &[root]);
        assert!(!trees.clips.contains_key(&id));
        assert!(trees.changes_for(root).contains(PropertyChangeFlags::CLIP));
        assert!(
            trees
                .changes_for(root)
                .contains(PropertyChangeFlags::TOPOLOGY)
        );

        set_clip_mode(&arena, root, ClipMode::AnchorParent);
        trees.sync(&arena, &[root]);
        assert!(trees.clips[&id].generation > first.generation);
    }

    #[test]
    fn nested_anchor_parent_leaf_is_exact_only_after_normal_siblings() {
        let (arena, root, normal, anchor) = nested_anchor_parent_fixture(false);
        let id = ClipNodeId {
            owner: anchor,
            role: ClipNodeRole::SelfClip,
        };
        let mut trees = PropertyTrees::default();
        trees.sync(&arena, &[root]);

        let clip = trees.clips[&id];
        assert_eq!(clip.parent, None);
        assert_eq!(clip.behavior, ClipBehavior::Replace);
        assert!(matches!(clip.geometry, ClipGeometry::LogicalScissor(_)));
        assert_eq!(trees.states[&normal].paint.clip, None);
        assert_eq!(trees.states[&anchor].paint.clip, Some(id));
        assert_eq!(trees.states[&anchor].descendants.clip, Some(id));
        assert_eq!(
            trees.authoritative_self_clip_for_owner(anchor, trees.states[&anchor].paint),
            Some(id)
        );

        trees.sync(&arena, &[root]);
        assert_eq!(trees.clips[&id].generation, clip.generation);

        let (arena, root, _, anchor) = nested_anchor_parent_fixture(true);
        let mut trees = PropertyTrees::default();
        trees.sync(&arena, &[root]);
        assert!(!trees.clips.contains_key(&ClipNodeId {
            owner: anchor,
            role: ClipNodeRole::SelfClip,
        }));
        assert_eq!(trees.states[&anchor].paint.clip, None);

        let (arena, root, normal, anchor) = nested_anchor_parent_fixture(false);
        set_clip_mode(&arena, normal, ClipMode::Viewport);
        let mut trees = PropertyTrees::default();
        trees.sync(&arena, &[root]);
        assert!(
            !trees.clips.contains_key(&ClipNodeId {
                owner: anchor,
                role: ClipNodeRole::SelfClip,
            }),
            "a deferred Viewport sibling invalidates the normal frame ordering witness"
        );
    }

    #[test]
    fn nested_anchor_parent_replace_escapes_ancestor_contents_intersection() {
        let (mut arena, parent, _, anchor) = nested_anchor_parent_fixture(false);
        let outer = insert_contents_clip_host(&mut arena, 0x8d10, Some([12, 14, 20, 18]));
        arena.set_parent(parent, Some(outer));
        arena.set_children(outer, vec![parent]);

        let contents = ClipNodeId {
            owner: outer,
            role: ClipNodeRole::ContentsClip,
        };
        let own = ClipNodeId {
            owner: anchor,
            role: ClipNodeRole::SelfClip,
        };
        let mut trees = PropertyTrees::default();
        trees.sync(&arena, &[outer]);

        assert_eq!(trees.states[&parent].paint.clip, Some(contents));
        assert_eq!(trees.clips[&own].parent, Some(contents));
        assert_eq!(trees.clips[&own].behavior, ClipBehavior::Replace);
        assert_eq!(trees.states[&anchor].paint.clip, Some(own));
        assert_eq!(
            trees
                .clip_snapshot_for(Some(own))
                .unwrap()
                .iter()
                .map(|clip| (clip.id, clip.behavior))
                .collect::<Vec<_>>(),
            vec![
                (own, ClipBehavior::Replace),
                (contents, ClipBehavior::Intersect),
            ]
        );
    }

    #[test]
    fn anchor_parent_clip_tombstone_survives_inactive_roots_and_prunes_removed_owner() {
        let (mut arena, root) = anchor_parent_clip_fixture(320.0);
        let id = ClipNodeId {
            owner: root,
            role: ClipNodeRole::SelfClip,
        };
        let mut trees = PropertyTrees::default();
        trees.sync(&arena, &[root]);
        let first_generation = trees.clips[&id].generation;

        trees.sync(&arena, &[]);
        assert!(!trees.clips.contains_key(&id));
        assert!(!trees.states.contains_key(&root));
        assert!(trees.clip_generations.contains_key(&id));

        trees.sync(&arena, &[root]);
        assert!(trees.clips[&id].generation > first_generation);

        arena.remove(root);
        trees.sync(&arena, &[root]);
        assert!(!trees.clips.contains_key(&id));
        assert!(!trees.states.contains_key(&root));
        assert!(!trees.clip_generations.contains_key(&id));
    }

    #[test]
    fn contents_clip_applies_only_to_descendants_and_is_inherited() {
        let mut arena = NodeArena::new();
        let root = insert_contents_clip_host(&mut arena, 0x8c00, Some([10, 20, 80, 40]));
        let child = insert_element(&mut arena, 0x8c01);
        append_child(&mut arena, root, child);
        let id = ClipNodeId {
            owner: root,
            role: ClipNodeRole::ContentsClip,
        };

        let mut trees = PropertyTrees::default();
        trees.sync(&arena, &[root]);

        assert_eq!(trees.states[&root].paint.clip, None);
        assert_eq!(trees.states[&root].descendants.clip, Some(id));
        assert_eq!(trees.states[&child].paint.clip, Some(id));
        let clip = trees.clips[&id];
        assert_eq!(clip.owner, root);
        assert_eq!(clip.parent, None);
        assert_eq!(clip.behavior, ClipBehavior::Intersect);
        assert!(matches!(
            clip.geometry,
            ClipGeometry::LogicalScissor([10, 20, 80, 40])
        ));
    }

    #[test]
    fn nested_contents_clips_intersect_in_owner_order_and_preserve_explicit_empty() {
        let mut arena = NodeArena::new();
        let outer = insert_contents_clip_host(&mut arena, 0x8c10, Some([0, 0, 100, 100]));
        let inner = insert_contents_clip_host(&mut arena, 0x8c11, Some([20, 30, 0, 0]));
        let leaf = insert_element(&mut arena, 0x8c12);
        append_child(&mut arena, outer, inner);
        append_child(&mut arena, inner, leaf);
        let outer_id = ClipNodeId {
            owner: outer,
            role: ClipNodeRole::ContentsClip,
        };
        let inner_id = ClipNodeId {
            owner: inner,
            role: ClipNodeRole::ContentsClip,
        };

        let mut trees = PropertyTrees::default();
        trees.sync(&arena, &[outer]);

        assert_eq!(trees.states[&inner].paint.clip, Some(outer_id));
        assert_eq!(trees.states[&inner].descendants.clip, Some(inner_id));
        assert_eq!(trees.states[&leaf].paint.clip, Some(inner_id));
        assert_eq!(trees.clips[&inner_id].parent, Some(outer_id));
        assert!(matches!(
            trees.clips[&inner_id].geometry,
            ClipGeometry::LogicalScissor([20, 30, 0, 0])
        ));
        assert_eq!(
            trees
                .clip_snapshot_for(Some(inner_id))
                .expect("complete nested clip chain")
                .iter()
                .map(|snapshot| snapshot.id)
                .collect::<Vec<_>>(),
            vec![inner_id, outer_id]
        );
    }

    #[test]
    fn contents_clip_removal_and_reinsert_bump_generation_and_topology() {
        let mut arena = NodeArena::new();
        let root = insert_contents_clip_host(&mut arena, 0x8c20, Some([1, 2, 30, 40]));
        let id = ClipNodeId {
            owner: root,
            role: ClipNodeRole::ContentsClip,
        };
        let mut trees = PropertyTrees::default();
        trees.sync(&arena, &[root]);
        let first = trees.clips[&id].generation;

        arena
            .get_mut(root)
            .expect("contents host")
            .element
            .as_any_mut()
            .downcast_mut::<ContentsClipHost>()
            .expect("contents host")
            .scissor = None;
        trees.sync(&arena, &[root]);
        assert!(!trees.clips.contains_key(&id));
        assert!(trees.changes_for(root).contains(PropertyChangeFlags::CLIP));
        assert!(
            trees
                .changes_for(root)
                .contains(PropertyChangeFlags::TOPOLOGY)
        );

        arena
            .get_mut(root)
            .expect("contents host")
            .element
            .as_any_mut()
            .downcast_mut::<ContentsClipHost>()
            .expect("contents host")
            .scissor = Some([1, 2, 30, 40]);
        trees.sync(&arena, &[root]);
        assert!(trees.clips[&id].generation > first);
    }

    #[test]
    fn opacity_change_marks_effect_without_other_property_changes() {
        let mut arena = NodeArena::new();
        let root = insert_element(&mut arena, 1);
        set_opacity(&arena, root, 0.5);
        let mut trees = PropertyTrees::default();
        trees.sync(&arena, &[root]);
        let generation = trees.effects[&EffectNodeId(root)].generation;
        trees.sync(&arena, &[root]);

        set_opacity(&arena, root, 0.25);
        trees.sync(&arena, &[root]);

        let changes = trees.changes_for(root);
        assert!(changes.contains(PropertyChangeFlags::EFFECT));
        assert!(!changes.contains(PropertyChangeFlags::TRANSFORM));
        assert!(!changes.contains(PropertyChangeFlags::CLIP));
        assert!(!changes.contains(PropertyChangeFlags::SCROLL));
        assert_eq!(
            trees.effects[&EffectNodeId(root)].generation,
            generation + 1
        );
    }

    #[test]
    fn scroll_state_applies_to_descendants_not_owner_paint() {
        let mut arena = NodeArena::new();
        let (root, child) = make_vertical_scroll_fixture(&mut arena, 1, 2);
        let mut trees = PropertyTrees::default();
        trees.sync(&arena, &[root]);
        trees.sync(&arena, &[root]);

        arena
            .get_mut(root)
            .expect("root exists")
            .element
            .set_scroll_offset((0.0, 24.0));
        // A newly written offset invalidates placement; property sync must not
        // combine it with the old placed geometry.
        trees.sync(&arena, &[root]);
        assert!(!trees.scrolls.contains_key(&ScrollNodeId(root)));
        assert!(trees.validation_errors.contains(
            &PropertyTreeValidationError::ScrollContractUnavailable(root)
        ));
        clear_layout_dirty_for_subtree(&arena, root);
        trees.sync(&arena, &[root]);

        assert_eq!(trees.states[&root].paint.scroll, None);
        assert_eq!(
            trees.states[&root].descendants.scroll,
            Some(ScrollNodeId(root))
        );
        assert_eq!(trees.states[&child].paint.scroll, Some(ScrollNodeId(root)));
        assert!(
            trees
                .changes_for(root)
                .contains(PropertyChangeFlags::SCROLL)
        );
        assert_eq!(
            trees.scrolls[&ScrollNodeId(root)].offset,
            Vec2::new(0.0, 24.0)
        );
        let scroll = trees.scrolls[&ScrollNodeId(root)];
        assert_eq!(scroll.configured_axis, ScrollAxisSnapshot::Vertical);
        assert!(rect_bits_equal(
            scroll.viewport,
            Rect {
                x: 10.0,
                y: 20.0,
                width: 100.0,
                height: 80.0,
            }
        ));
        assert_eq!(
            [scroll.content_size.width, scroll.content_size.height],
            [100.0, 300.0]
        );
    }

    #[test]
    fn canonical_scroll_geometry_and_element_admission_preserve_full_2d_state_for_all_axes() {
        let cases = [
            (
                ScrollDirection::Vertical,
                ScrollAxisSnapshot::Vertical,
                false,
                true,
            ),
            (
                ScrollDirection::Horizontal,
                ScrollAxisSnapshot::Horizontal,
                true,
                false,
            ),
            (ScrollDirection::Both, ScrollAxisSnapshot::Both, true, true),
        ];
        let mut both = None;
        for (index, (direction, expected_axis, expect_horizontal, expect_vertical)) in
            cases.into_iter().enumerate()
        {
            let mut arena = NodeArena::new();
            let root = insert_element(&mut arena, 20_000 + index as u64 * 2);
            let child = insert_element(&mut arena, 20_001 + index as u64 * 2);
            append_child(&mut arena, root, child);
            set_scroll_direction(&arena, root, direction);
            for owner in [root, child] {
                let mut style = Style::new();
                style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
                arena
                    .get_mut(owner)
                    .unwrap()
                    .element
                    .as_any_mut()
                    .downcast_mut::<Element>()
                    .unwrap()
                    .apply_style(style);
            }
            arena
                .get_mut(child)
                .unwrap()
                .element
                .as_any_mut()
                .downcast_mut::<Element>()
                .unwrap()
                .set_background_color_value(crate::style::Color::rgb(24, 48, 72));
            let viewport = Rect {
                x: 10.0,
                y: 20.0,
                width: 100.0,
                height: 80.0,
            };
            let content_size = [280.0, 260.0];
            let offset = [37.0, 41.0];
            install_scroll_layout_geometry(&arena, root, viewport, content_size);
            install_scroll_layout_geometry(
                &arena,
                child,
                Rect {
                    x: viewport.x - offset[0],
                    y: viewport.y - offset[1],
                    width: content_size[0],
                    height: content_size[1],
                },
                content_size,
            );
            arena
                .get_mut(root)
                .unwrap()
                .element
                .set_scroll_offset((offset[0], offset[1]));
            clear_layout_dirty_for_subtree(&arena, root);

            let mut trees = PropertyTrees::default();
            trees.sync(&arena, &[root]);
            assert!(trees.validation_errors.is_empty(), "{direction:?}");
            let snapshot = trees.scroll_snapshot_for(ScrollNodeId(root)).unwrap();
            let clip_id = ClipNodeId {
                owner: root,
                role: ClipNodeRole::ContentsClip,
            };
            let clip = trees.clip_snapshot_for(Some(clip_id)).unwrap()[0];
            assert_eq!(snapshot.configured_axis, expected_axis);
            assert_eq!(snapshot.offset, Vec2::new(offset[0], offset[1]));
            assert_eq!(
                snapshot.scrollbar_overlay.horizontal_track.is_some(),
                expect_horizontal
            );
            assert_eq!(
                snapshot.scrollbar_overlay.vertical_track.is_some(),
                expect_vertical
            );
            assert!(snapshot.has_canonical_geometry_with_contents_clip(clip));
            assert!(snapshot.has_canonical_vertical_geometry_with_contents_clip(clip));

            let admission = arena
                .get(root)
                .unwrap()
                .element
                .as_any()
                .downcast_ref::<Element>()
                .unwrap()
                .exact_retained_scroll_host_admission(root, &arena, 1.0)
                .unwrap_or_else(|| panic!("{direction:?} must pass exact Element admission"));
            assert_eq!(admission.scroll.configured_axis, expected_axis);
            assert_eq!(
                admission.scroll.offset.map(f32::to_bits),
                offset.map(f32::to_bits)
            );
            let child_bounds = arena.get(child).unwrap().element.box_model_snapshot();
            assert_eq!(
                (child_bounds.x + admission.scroll.offset[0]).to_bits(),
                admission.scroll.layout_content_bounds_at_zero.x.to_bits()
            );
            assert_eq!(
                (child_bounds.y + admission.scroll.offset[1]).to_bits(),
                admission.scroll.layout_content_bounds_at_zero.y.to_bits()
            );

            if expected_axis == ScrollAxisSnapshot::Both {
                both = Some((snapshot, clip));
            }
        }

        let (both, clip) = both.expect("Both fixture");
        let mut offset_tampered = both;
        offset_tampered.offset.x += 1.0;
        assert!(!offset_tampered.has_canonical_geometry_with_contents_clip(clip));

        let mut bounds_tampered = both;
        bounds_tampered.layout_content_bounds_at_zero.width += 1.0;
        assert!(!bounds_tampered.has_canonical_geometry_with_contents_clip(clip));

        let mut axis_tampered = both;
        axis_tampered.configured_axis = ScrollAxisSnapshot::Vertical;
        assert!(!axis_tampered.has_canonical_geometry_with_contents_clip(clip));

        let mut overlay_tampered = both;
        overlay_tampered
            .scrollbar_overlay
            .horizontal_thumb
            .as_mut()
            .unwrap()
            .x += 1.0;
        assert!(!overlay_tampered.has_canonical_geometry_with_contents_clip(clip));
    }

    #[test]
    fn scroll_snapshot_generations_track_axis_viewport_content_clip_and_overlay_fields() {
        let mut arena = NodeArena::new();
        let (root, _child) = make_vertical_scroll_fixture(&mut arena, 10, 11);
        let scroll_id = ScrollNodeId(root);
        let clip_id = ClipNodeId {
            owner: root,
            role: ClipNodeRole::ContentsClip,
        };
        let mut trees = PropertyTrees::default();
        trees.sync(&arena, &[root]);
        let stable_scroll = trees.scrolls[&scroll_id].generation;
        let stable_clip = trees.clips[&clip_id].generation;

        trees.sync(&arena, &[root]);
        assert_eq!(trees.scrolls[&scroll_id].generation, stable_scroll);
        assert_eq!(trees.clips[&clip_id].generation, stable_clip);
        assert_eq!(trees.changes_for(root), PropertyChangeFlags::NONE);

        install_scroll_layout_geometry(
            &arena,
            root,
            Rect {
                x: 10.0,
                y: 20.0,
                width: 100.0,
                height: 80.0,
            },
            [100.0, 360.0],
        );
        trees.sync(&arena, &[root]);
        let content_generation = trees.scrolls[&scroll_id].generation;
        assert!(content_generation > stable_scroll);
        assert_eq!(trees.clips[&clip_id].generation, stable_clip);
        assert!(
            trees
                .changes_for(root)
                .contains(PropertyChangeFlags::SCROLL)
        );
        assert!(!trees.changes_for(root).contains(PropertyChangeFlags::CLIP));

        install_scroll_layout_geometry(
            &arena,
            root,
            Rect {
                x: 11.25,
                y: 21.5,
                width: 96.5,
                height: 76.25,
            },
            [180.0, 360.0],
        );
        set_scroll_direction(&arena, root, ScrollDirection::Both);
        clear_layout_dirty_for_subtree(&arena, root);
        trees.sync(&arena, &[root]);
        let geometry_generation = trees.scrolls[&scroll_id].generation;
        assert!(geometry_generation > content_generation);
        assert_eq!(
            trees.scrolls[&scroll_id].configured_axis,
            ScrollAxisSnapshot::Both
        );
        assert!(trees.clips[&clip_id].generation > stable_clip);
        assert!(matches!(
            trees.clips[&clip_id].geometry,
            ClipGeometry::LogicalScissor([11, 21, 97, 77])
        ));
        assert!(
            trees
                .changes_for(root)
                .contains(PropertyChangeFlags::SCROLL)
        );
        assert!(trees.changes_for(root).contains(PropertyChangeFlags::CLIP));

        arena
            .get_mut(root)
            .expect("root")
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .expect("Element")
            .set_scrollbar_shadow_blur_radius_for_test(7.0);
        trees.sync(&arena, &[root]);
        assert!(trees.scrolls[&scroll_id].generation > geometry_generation);
        assert_eq!(
            trees.scrolls[&scroll_id]
                .scrollbar_overlay
                .shadow_blur_radius,
            7.0
        );
    }

    #[test]
    fn nested_scroll_scene_admission_is_a_strict_sibling_of_the_b0_oracle() {
        let mut arena = NodeArena::new();
        let (outer, inner, leaf) =
            make_nested_vertical_scroll_fixture(&mut arena, 12_100, 12_101, 12_102);
        let outer_node = arena.get(outer).unwrap();
        let outer_element = outer_node
            .element
            .as_any()
            .downcast_ref::<Element>()
            .unwrap();
        assert!(
            outer_element
                .exact_retained_scroll_host_admission(outer, &arena, 1.0)
                .is_none(),
            "the original B0 oracle must remain leaf-only"
        );
        let admission = outer_element
            .exact_retained_nested_scroll_scene_admission(outer, &arena, 1.0)
            .expect("exact S0 -> S1 -> leaf fixture");
        assert_eq!(admission.outer_boundary_root, outer);
        assert_eq!(admission.inner_boundary_root, inner);
        assert_eq!(admission.content_leaf, leaf);
        assert!(admission.bitwise_eq(admission));

        let mut trees = PropertyTrees::default();
        trees.sync(&arena, &[outer]);
        let outer_scroll = trees.scroll_snapshot_for(ScrollNodeId(outer)).unwrap();
        let inner_scroll = trees.scroll_snapshot_for(ScrollNodeId(inner)).unwrap();
        assert!(admission.matches_scroll_nodes(outer_scroll, inner_scroll));

        let mut stable_id_drift = admission;
        stable_id_drift.inner_stable_id = stable_id_drift.inner_stable_id.saturating_add(1);
        assert!(!admission.bitwise_eq(stable_id_drift));
        let mut source_bounds_drift = admission;
        source_bounds_drift.inner_source_bounds.x += 1.0;
        assert!(!admission.bitwise_eq(source_bounds_drift));
        let mut scroll_geometry_drift = admission;
        scroll_geometry_drift.inner_scroll.offset[1] += 0.25;
        assert!(!admission.bitwise_eq(scroll_geometry_drift));

        let foreign = {
            let mut foreign_arena = NodeArena::new();
            insert_element(&mut foreign_arena, 12_103);
            insert_element(&mut foreign_arena, 12_104);
            insert_element(&mut foreign_arena, 12_105);
            insert_element(&mut foreign_arena, 12_106)
        };
        assert_ne!(foreign, outer);
        assert_ne!(foreign, inner);
        let mut foreign_outer = outer_scroll;
        foreign_outer.id = ScrollNodeId(foreign);
        foreign_outer.owner = foreign;
        assert!(
            !admission.matches_scroll_nodes(foreign_outer, inner_scroll),
            "same geometry under a foreign outer identity must fail closed"
        );
        let mut foreign_inner = inner_scroll;
        foreign_inner.id = ScrollNodeId(foreign);
        foreign_inner.owner = foreign;
        assert!(
            !admission.matches_scroll_nodes(outer_scroll, foreign_inner),
            "same geometry under a foreign inner identity must fail closed"
        );

        let dpr2_admission = outer_element
            .exact_retained_nested_scroll_scene_admission(outer, &arena, 2.0)
            .expect("device-aligned nested scroll geometry remains exact at DPR2");
        assert!(admission.bitwise_eq(dpr2_admission));
        let device_aligned = |value: f32| {
            let device = value * 2.0;
            device.is_finite() && device.fract().to_bits() == 0.0_f32.to_bits()
        };
        let bounds_edges = |bounds: crate::view::base_component::RetainedSurfaceBounds| {
            [
                bounds.x,
                bounds.y,
                bounds.x + bounds.width,
                bounds.y + bounds.height,
            ]
        };
        let rect_edges = |rect: Rect| [rect.x, rect.y, rect.x + rect.width, rect.y + rect.height];
        assert!(
            bounds_edges(dpr2_admission.outer_source_bounds)
                .into_iter()
                .chain(bounds_edges(dpr2_admission.inner_source_bounds))
                .chain(rect_edges(dpr2_admission.outer_scroll.scrollport_rect))
                .chain(rect_edges(dpr2_admission.inner_scroll.scrollport_rect))
                .all(device_aligned)
        );
        assert!(
            outer_element
                .exact_retained_nested_scroll_scene_admission(outer, &arena, 0.0)
                .is_none()
        );
        assert!(
            outer_element
                .exact_retained_nested_scroll_scene_admission(outer, &arena, f32::NAN)
                .is_none()
        );
        drop(outer_node);
        arena
            .get_mut(outer)
            .unwrap()
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .unwrap()
            .layout_state
            .layout_position
            .x += 0.25;
        let outer_node = arena.get(outer).unwrap();
        let outer_element = outer_node
            .element
            .as_any()
            .downcast_ref::<Element>()
            .unwrap();
        assert!(
            outer_element
                .exact_retained_nested_scroll_scene_admission(outer, &arena, 2.0)
                .is_none()
        );

        let (mut sibling_arena, sibling_outer, _, _) = {
            let mut arena = NodeArena::new();
            let (outer, inner, leaf) =
                make_nested_vertical_scroll_fixture(&mut arena, 12_110, 12_111, 12_112);
            (arena, outer, inner, leaf)
        };
        let sibling = insert_element(&mut sibling_arena, 12_113);
        append_child(&mut sibling_arena, sibling_outer, sibling);
        assert!(
            sibling_arena
                .get(sibling_outer)
                .unwrap()
                .element
                .as_any()
                .downcast_ref::<Element>()
                .unwrap()
                .exact_retained_nested_scroll_scene_admission(sibling_outer, &sibling_arena, 1.0,)
                .is_none(),
            "outer siblings must fail closed"
        );

        let mut detached_arena = NodeArena::new();
        let (detached_outer, detached_inner, _) =
            make_nested_vertical_scroll_fixture(&mut detached_arena, 12_120, 12_121, 12_122);
        detached_arena.set_parent(detached_inner, None);
        assert!(
            detached_arena
                .get(detached_outer)
                .unwrap()
                .element
                .as_any()
                .downcast_ref::<Element>()
                .unwrap()
                .exact_retained_nested_scroll_scene_admission(detached_outer, &detached_arena, 1.0,)
                .is_none(),
            "the direct S0/S1 arena edge is part of admission"
        );

        let mut detached_leaf_arena = NodeArena::new();
        let (detached_leaf_outer, _, detached_leaf) =
            make_nested_vertical_scroll_fixture(&mut detached_leaf_arena, 12_125, 12_126, 12_127);
        detached_leaf_arena.set_parent(detached_leaf, None);
        assert!(
            detached_leaf_arena
                .get(detached_leaf_outer)
                .unwrap()
                .element
                .as_any()
                .downcast_ref::<Element>()
                .unwrap()
                .exact_retained_nested_scroll_scene_admission(
                    detached_leaf_outer,
                    &detached_leaf_arena,
                    1.0,
                )
                .is_none(),
            "the direct S1/leaf arena edge is part of admission"
        );

        let mut styled_arena = NodeArena::new();
        let (styled_outer, styled_inner, _) =
            make_nested_vertical_scroll_fixture(&mut styled_arena, 12_130, 12_131, 12_132);
        set_opacity(&styled_arena, styled_inner, 0.5);
        assert!(
            styled_arena
                .get(styled_outer)
                .unwrap()
                .element
                .as_any()
                .downcast_ref::<Element>()
                .unwrap()
                .exact_retained_nested_scroll_scene_admission(styled_outer, &styled_arena, 1.0,)
                .is_none(),
            "inner effects remain outside the exact nested-scroll slice"
        );
    }

    #[test]
    fn nested_scroll_geometry_validator_binds_scroll_and_clip_parent_edges() {
        let mut arena = NodeArena::new();
        let (outer, inner, leaf) =
            make_nested_vertical_scroll_fixture(&mut arena, 12_200, 12_201, 12_202);
        let mut trees = PropertyTrees::default();
        trees.sync(&arena, &[outer]);
        assert!(trees.validation_errors.is_empty());

        let outer_scroll = trees.scroll_snapshot_for(ScrollNodeId(outer)).unwrap();
        let inner_scroll = trees.scroll_snapshot_for(ScrollNodeId(inner)).unwrap();
        let outer_clip_id = ClipNodeId {
            owner: outer,
            role: ClipNodeRole::ContentsClip,
        };
        let inner_clip_id = ClipNodeId {
            owner: inner,
            role: ClipNodeRole::ContentsClip,
        };
        let outer_clip = trees.clip_snapshot_for(Some(outer_clip_id)).unwrap()[0];
        let inner_clips = trees.clip_snapshot_for(Some(inner_clip_id)).unwrap();
        assert_eq!(inner_clips.len(), 2);
        let inner_clip = inner_clips[0];
        assert_eq!(inner_scroll.parent, Some(outer_scroll.id));
        assert_eq!(inner_clip.parent, Some(outer_clip.id));
        assert!(
            inner_scroll.has_canonical_nested_vertical_geometry_with_contents_clip(
                inner_clip,
                outer_scroll,
                outer_clip,
            )
        );
        assert!(
            !inner_scroll.has_canonical_vertical_geometry_with_contents_clip(inner_clip),
            "the parentless B0 validator must remain unchanged"
        );

        let outer_state = PropertyTreeState {
            clip: Some(outer_clip.id),
            scroll: Some(outer_scroll.id),
            ..Default::default()
        };
        let inner_state = PropertyTreeState {
            clip: Some(inner_clip.id),
            scroll: Some(inner_scroll.id),
            ..Default::default()
        };
        assert_eq!(trees.states[&outer].paint, PropertyTreeState::default());
        assert_eq!(trees.states[&outer].descendants, outer_state);
        assert_eq!(trees.states[&inner].paint, outer_state);
        assert_eq!(trees.states[&inner].descendants, inner_state);
        assert_eq!(trees.states[&leaf].paint, inner_state);

        let mut wrong_scroll_parent = inner_scroll;
        wrong_scroll_parent.parent = None;
        assert!(
            !wrong_scroll_parent.has_canonical_nested_vertical_geometry_with_contents_clip(
                inner_clip,
                outer_scroll,
                outer_clip,
            )
        );
        let mut wrong_clip_parent = inner_clip;
        wrong_clip_parent.parent = None;
        assert!(
            !inner_scroll.has_canonical_nested_vertical_geometry_with_contents_clip(
                wrong_clip_parent,
                outer_scroll,
                outer_clip,
            )
        );
        let mut non_root_parent = outer_scroll;
        non_root_parent.parent = Some(inner_scroll.id);
        assert!(
            !inner_scroll.has_canonical_nested_vertical_geometry_with_contents_clip(
                inner_clip,
                non_root_parent,
                outer_clip,
            )
        );
    }

    #[test]
    fn scroll_contents_clip_is_owned_by_scroll_host_and_inherits_parent_clip() {
        let mut arena = NodeArena::new();
        let parent = insert_contents_clip_host(&mut arena, 20, Some([2, 3, 140, 120]));
        let (root, child) = make_vertical_scroll_fixture(&mut arena, 21, 22);
        append_child(&mut arena, parent, root);
        clear_layout_dirty_for_subtree(&arena, root);
        let parent_clip = ClipNodeId {
            owner: parent,
            role: ClipNodeRole::ContentsClip,
        };
        let scroll_clip = ClipNodeId {
            owner: root,
            role: ClipNodeRole::ContentsClip,
        };
        let mut trees = PropertyTrees::default();
        trees.sync(&arena, &[parent]);

        assert_eq!(trees.states[&root].paint.clip, Some(parent_clip));
        assert_eq!(trees.states[&root].descendants.clip, Some(scroll_clip));
        assert_eq!(trees.states[&child].paint.clip, Some(scroll_clip));
        let clip = trees.clips[&scroll_clip];
        assert_eq!(clip.owner, root);
        assert_eq!(clip.parent, Some(parent_clip));
        assert_eq!(clip.behavior, ClipBehavior::Intersect);
        assert!(matches!(
            clip.geometry,
            ClipGeometry::LogicalScissor([10, 20, 100, 80])
        ));
    }

    #[test]
    fn inactive_unsupported_and_invalid_scroll_observations_are_distinct_and_fail_closed() {
        let mut arena = NodeArena::new();
        let inactive = insert_element(&mut arena, 30);
        set_scroll_direction(&arena, inactive, ScrollDirection::Vertical);
        clear_layout_dirty_for_subtree(&arena, inactive);
        let mut trees = PropertyTrees::default();
        trees.sync(&arena, &[inactive]);
        assert!(!trees.scrolls.contains_key(&ScrollNodeId(inactive)));
        assert!(trees.validation_errors.is_empty());

        let missing = insert_missing_scroll_contract_host(&mut arena, 33);
        trees.sync(&arena, &[missing]);
        assert!(!trees.scrolls.contains_key(&ScrollNodeId(missing)));
        assert!(!trees.clips.contains_key(&ClipNodeId {
            owner: missing,
            role: ClipNodeRole::ContentsClip,
        }));
        assert!(trees.validation_errors.contains(
            &PropertyTreeValidationError::ScrollContractUnavailable(missing)
        ));

        let (unsupported, child) = make_vertical_scroll_fixture(&mut arena, 31, 32);
        arena
            .get_mut(child)
            .expect("child")
            .element
            .set_layout_width(120.0);
        arena.refresh_subtree_dirty_cache(unsupported);
        trees.sync(&arena, &[unsupported]);
        assert!(!trees.scrolls.contains_key(&ScrollNodeId(unsupported)));
        assert!(trees.validation_errors.contains(
            &PropertyTreeValidationError::ScrollContractUnavailable(unsupported)
        ));

        clear_layout_dirty_for_subtree(&arena, unsupported);
        arena
            .get_mut(unsupported)
            .expect("root")
            .element
            .set_scroll_offset((f32::NAN, 0.0));
        clear_layout_dirty_for_subtree(&arena, unsupported);
        trees.sync(&arena, &[unsupported]);
        assert!(!trees.scrolls.contains_key(&ScrollNodeId(unsupported)));
        assert!(trees.validation_errors.contains(
            &PropertyTreeValidationError::InvalidScrollGeometrySnapshot(unsupported)
        ));
        assert!(!trees.clips.contains_key(&ClipNodeId {
            owner: unsupported,
            role: ClipNodeRole::ContentsClip,
        }));
    }

    #[test]
    fn effect_generation_stays_monotonic_across_inactive_and_readded_state() {
        let mut arena = NodeArena::new();
        let root = insert_element(&mut arena, 1);
        set_opacity(&arena, root, 0.5);
        let mut trees = PropertyTrees::default();
        trees.sync(&arena, &[root]);
        let first_generation = trees.effects[&EffectNodeId(root)].generation;

        set_opacity(&arena, root, 1.0);
        trees.sync(&arena, &[root]);
        assert!(!trees.effects.contains_key(&EffectNodeId(root)));

        set_opacity(&arena, root, 0.5);
        trees.sync(&arena, &[root]);
        assert!(trees.effects[&EffectNodeId(root)].generation > first_generation);
    }

    #[test]
    fn effect_generation_survives_temporary_removal_from_active_roots() {
        let mut arena = NodeArena::new();
        let root = insert_element(&mut arena, 1);
        set_opacity(&arena, root, 0.5);
        let mut trees = PropertyTrees::default();
        trees.sync(&arena, &[root]);
        let first_generation = trees.effects[&EffectNodeId(root)].generation;

        trees.sync(&arena, &[]);
        assert!(!trees.effects.contains_key(&EffectNodeId(root)));
        assert!(!trees.states.contains_key(&root));

        trees.sync(&arena, &[root]);
        assert!(trees.effects[&EffectNodeId(root)].generation > first_generation);
    }

    #[test]
    fn scroll_generation_stays_monotonic_across_off_and_on_state() {
        let mut arena = NodeArena::new();
        let (root, _child) = make_vertical_scroll_fixture(&mut arena, 1, 2);
        let mut trees = PropertyTrees::default();
        trees.sync(&arena, &[root]);
        let first_generation = trees.scrolls[&ScrollNodeId(root)].generation;

        set_scroll_direction(&arena, root, ScrollDirection::None);
        clear_layout_dirty_for_subtree(&arena, root);
        trees.sync(&arena, &[root]);
        assert!(!trees.scrolls.contains_key(&ScrollNodeId(root)));

        set_scroll_direction(&arena, root, ScrollDirection::Vertical);
        clear_layout_dirty_for_subtree(&arena, root);
        trees.sync(&arena, &[root]);
        assert!(trees.scrolls[&ScrollNodeId(root)].generation > first_generation);
    }

    #[test]
    fn scroll_snapshot_equality_owns_exact_scrollbar_paint_state_and_sampled_alpha() {
        let mut arena = NodeArena::new();
        let (root, _child) = make_vertical_scroll_fixture(&mut arena, 31, 32);
        let mut trees = PropertyTrees::default();
        trees.sync(&arena, &[root]);
        let baseline = trees.scroll_snapshot_for(ScrollNodeId(root)).unwrap();

        let mut changed_state = baseline;
        changed_state.scrollbar_overlay.paint_state = ScrollbarPaintStateWitness::OpaqueNow;
        assert_ne!(baseline, changed_state);

        let mut changed_alpha = baseline;
        changed_alpha.scrollbar_overlay.sampled_alpha = 0.5;
        assert_ne!(baseline, changed_alpha);
    }

    #[test]
    fn scroll_generation_survives_temporary_removal_from_active_roots() {
        let mut arena = NodeArena::new();
        let (root, _child) = make_vertical_scroll_fixture(&mut arena, 1, 2);
        let mut trees = PropertyTrees::default();
        trees.sync(&arena, &[root]);
        let first_generation = trees.scrolls[&ScrollNodeId(root)].generation;

        trees.sync(&arena, &[]);
        assert!(!trees.scrolls.contains_key(&ScrollNodeId(root)));
        assert!(!trees.states.contains_key(&root));

        trees.sync(&arena, &[root]);
        assert!(trees.scrolls[&ScrollNodeId(root)].generation > first_generation);
    }

    #[test]
    fn text_image_and_svg_effect_snapshots_use_the_trait_contract() {
        let mut arena = NodeArena::new();

        let mut text = Text::new_with_id(1, 0.0, 0.0, 80.0, 20.0, "text");
        text.set_opacity(0.25);
        let text_key = arena.insert(Node::new(Box::new(text)));

        let mut image = Image::new_with_id(
            2,
            ImageSource::Rgba {
                width: 1,
                height: 1,
                pixels: Arc::from([255, 255, 255, 255]),
            },
        );
        image.apply_style(opacity_style(0.5));
        let image_key = arena.insert(Node::new(Box::new(image)));

        let mut svg = Svg::new_with_id(
            3,
            SvgSource::Content("<svg xmlns=\"http://www.w3.org/2000/svg\"/>".into()),
        );
        svg.apply_style(opacity_style(0.75));
        let svg_key = arena.insert(Node::new(Box::new(svg)));

        let mut trees = PropertyTrees::default();
        trees.sync(&arena, &[text_key, image_key, svg_key]);

        assert_eq!(trees.effects[&EffectNodeId(text_key)].opacity, 0.25);
        assert_eq!(trees.effects[&EffectNodeId(image_key)].opacity, 0.5);
        assert_eq!(trees.effects[&EffectNodeId(svg_key)].opacity, 0.75);
    }

    #[test]
    fn reparent_preserves_effect_id_and_updates_parent_topology() {
        let mut arena = NodeArena::new();
        let left = insert_element(&mut arena, 1);
        let right = insert_element(&mut arena, 2);
        let child = insert_element(&mut arena, 3);
        set_opacity(&arena, left, 0.8);
        set_opacity(&arena, right, 0.6);
        set_opacity(&arena, child, 0.4);
        append_child(&mut arena, left, child);
        let mut trees = PropertyTrees::default();
        trees.sync(&arena, &[left, right]);
        let generation = trees.effects[&EffectNodeId(child)].generation;

        arena.set_children(left, Vec::new());
        arena.set_parent(child, Some(right));
        arena.push_child(right, child);
        trees.sync(&arena, &[left, right]);

        let effect = trees.effects[&EffectNodeId(child)];
        assert_eq!(effect.parent, Some(EffectNodeId(right)));
        assert_eq!(effect.generation, generation + 1);
        assert!(
            trees
                .changes_for(child)
                .contains(PropertyChangeFlags::TOPOLOGY)
        );
    }

    #[test]
    fn sync_prunes_removed_nodes_and_generational_keys_do_not_alias() {
        let mut arena = NodeArena::new();
        let old = insert_element(&mut arena, 1);
        set_opacity(&arena, old, 0.5);
        let mut trees = PropertyTrees::default();
        trees.sync(&arena, &[old]);
        assert!(trees.states.contains_key(&old));

        arena.remove(old);
        // A caller may retain a stale generational root key until its own
        // root list is compacted; it must not keep shadow entries alive.
        trees.sync(&arena, &[old]);
        assert!(!trees.states.contains_key(&old));
        assert!(!trees.effects.contains_key(&EffectNodeId(old)));

        let new = insert_element(&mut arena, 2);
        assert_ne!(old, new);
        trees.sync(&arena, &[new]);
        assert!(!trees.states.contains_key(&old));
        assert!(trees.states.contains_key(&new));
    }

    #[test]
    fn neutral_component_does_not_invent_transform_clip_effect_or_scroll_nodes() {
        let mut arena = NodeArena::new();
        let key = arena.insert(Node::new(Box::new(NeutralCustomHost)));
        let mut trees = PropertyTrees::default();
        trees.sync(&arena, &[key]);

        assert_eq!(trees.states[&key], NodePropertyState::default());
        assert!(
            trees
                .changes_for(key)
                .contains(PropertyChangeFlags::TOPOLOGY)
        );
        assert!(trees.transforms.is_empty());
        assert!(trees.clips.is_empty());
        assert!(trees.effects.is_empty());
        assert!(trees.scrolls.is_empty());

        trees.sync(&arena, &[]);
        assert!(!trees.states.contains_key(&key));
        trees.sync(&arena, &[key]);
        assert_eq!(trees.states[&key], NodePropertyState::default());
        assert!(
            trees
                .changes_for(key)
                .contains(PropertyChangeFlags::TOPOLOGY)
        );
    }
}
