//! Retained transform / clip / effect / scroll property trees.
//!
//! These trees are not yet render truth.  They retain stable `NodeKey` based
//! identity and classify resolved property changes while the existing render
//! and dirty paths remain authoritative.

#![allow(dead_code)]

use glam::{Mat4, Vec2};
use rustc_hash::{FxHashMap, FxHashSet};
use slotmap::Key;

use crate::view::base_component::{
    Rect, ScrollAxisSnapshot, ScrollContentsClipWitness, ScrollGeometryObservation,
    ScrollGeometrySnapshot, ScrollbarOverlayWitness, ScrollbarPaintStateWitness, Size,
    SpatialPositionReferenceSnapshot, canonical_horizontal_scrollbar_geometry,
    canonical_vertical_scrollbar_geometry, exact_logical_scissor_for_rect,
};
use crate::view::node_arena::{NodeArena, NodeKey};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TransformNodeId(pub(crate) NodeKey);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct EffectNodeId(pub(crate) NodeKey);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ScrollNodeId(pub(crate) NodeKey);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct LayoutPositionNodeId(pub(crate) NodeKey);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct VisualOffsetNodeId(pub(crate) NodeKey);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SpatialPositionReference {
    Viewport,
    LayoutParent(Option<NodeKey>),
    /// Named anchor's visual border-box origin. Projection resolves this from
    /// the anchor's layout-position and visual-offset chains; its transform
    /// and the flattened compatibility viewport position are not inputs.
    ///
    /// C0b deliberately preserves placement compatibility: the anchor and
    /// anchored owner visual chains are evaluated independently, so a shared
    /// visual ancestor contributes once through each chain.
    /// `Element::register_anchor_snapshot` stores the anchor's
    /// `layout_state.layout_position` in `PLACEMENT_RUNTIME.anchors` after
    /// placement has added the inherited and local visual offsets, while the
    /// anchored owner's `LayoutPlacement` carries its visual offsets
    /// independently. Revalidate this compatibility contract if either call
    /// site changes. Anchor transforms and anchor references carrying
    /// `reference_scroll` remain unsupported; both require explicit capability
    /// work before the full Stage C corpus.
    Anchor(NodeKey),
}

/// Composition-invariant placement edge captured before scroll subtraction.
/// The translation is relative to `reference`, not an absolute viewport
/// position reconstructed from `layout_flow_position`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LayoutPositionNode {
    pub(crate) owner: NodeKey,
    pub(crate) reference: SpatialPositionReference,
    /// Scroll edge subtracted from this placement by the referenced parent's
    /// child layout context. If present, the V2 snapshot graph must contain
    /// the exact scroll node rather than inferring it from owner coincidence.
    pub(crate) reference_scroll: Option<ScrollNodeId>,
    pub(crate) translation_at_scroll_zero: Vec2,
    pub(crate) child_reference_offset_at_scroll_zero: Vec2,
    pub(crate) generation: u64,
}

/// Owner-local layout-transition offset. Ancestor visual offsets are carried
/// by their own nodes; this payload is never cumulative and never raster
/// identity.
#[derive(Clone, Copy, Debug)]
pub(crate) struct VisualOffsetNode {
    pub(crate) owner: NodeKey,
    pub(crate) parent: Option<VisualOffsetNodeId>,
    pub(crate) offset: Vec2,
    pub(crate) generation: u64,
}

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
    pub(crate) layout_position: Option<LayoutPositionNodeId>,
    pub(crate) visual_offset: Option<VisualOffsetNodeId>,
}

impl PropertyTreeState {
    /// Explicit compatibility projection for the pre-V2 exact grammar. New
    /// recording/artifact equality remains six-dimensional; only the legacy
    /// boundary recognizer may erase spatial dimensions during migration.
    pub(crate) const fn legacy_boundary_dimensions(self) -> Self {
        Self {
            layout_position: None,
            visual_offset: None,
            ..self
        }
    }

    pub(crate) fn legacy_boundary_eq(self, other: Self) -> bool {
        self.legacy_boundary_dimensions() == other.legacy_boundary_dimensions()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PropertyDimensionTransition<Id> {
    pub(crate) from: Option<Id>,
    pub(crate) to: Option<Id>,
}

impl<Id: Copy + Eq> PropertyDimensionTransition<Id> {
    pub(crate) fn is_changed(self) -> bool {
        self.from != self.to
    }
}

/// Pairwise six-dimensional property-state delta. This classifies identity
/// changes only; B1b resolves descend/ascend/cross semantics from the owning
/// arena-independent snapshot graph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PropertyStateTransition {
    pub(crate) transform: PropertyDimensionTransition<TransformNodeId>,
    pub(crate) clip: PropertyDimensionTransition<ClipNodeId>,
    pub(crate) effect: PropertyDimensionTransition<EffectNodeId>,
    pub(crate) scroll: PropertyDimensionTransition<ScrollNodeId>,
    pub(crate) layout_position: PropertyDimensionTransition<LayoutPositionNodeId>,
    pub(crate) visual_offset: PropertyDimensionTransition<VisualOffsetNodeId>,
}

impl PropertyStateTransition {
    pub(crate) fn between(from: PropertyTreeState, to: PropertyTreeState) -> Self {
        Self {
            transform: PropertyDimensionTransition {
                from: from.transform,
                to: to.transform,
            },
            clip: PropertyDimensionTransition {
                from: from.clip,
                to: to.clip,
            },
            effect: PropertyDimensionTransition {
                from: from.effect,
                to: to.effect,
            },
            scroll: PropertyDimensionTransition {
                from: from.scroll,
                to: to.scroll,
            },
            layout_position: PropertyDimensionTransition {
                from: from.layout_position,
                to: to.layout_position,
            },
            visual_offset: PropertyDimensionTransition {
                from: from.visual_offset,
                to: to.visual_offset,
            },
        }
    }
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
    /// Unconjugated authored transform in the owner's local reference box.
    /// It is not ancestor-composed and contains no layout position, scroll
    /// offset, or transition visual offset. Consumers combine it with
    /// `local_origin` to form the node-local conjugated transform.
    pub(crate) local_matrix: Mat4,
    pub(crate) local_origin: glam::Vec3,
    /// Revision of only `local_matrix` / `local_origin`; unlike `generation`,
    /// it remains stable when the transform is reparented.
    pub(crate) local_generation: u64,
    pub(crate) generation: u64,
    /// B2-derived composition geometry. This cache is rebuilt from the four
    /// spatial source families after every sync and is never an independent
    /// authority. `None` keeps incomplete or oracle-divergent paths closed.
    pub(crate) derived_projection: Option<DerivedSpatialProjection>,
}

/// Arena-independent, owning copy of one transform-tree node.
/// `local_matrix` plus `local_origin` is the position-independent authored
/// source; `owner_viewport_transform` is derived from the complete spatial
/// snapshot graph and is composite-side data only.
#[derive(Clone, Copy, Debug)]
pub(crate) struct TransformNodeSnapshot {
    pub(crate) id: TransformNodeId,
    pub(crate) owner: NodeKey,
    pub(crate) parent: Option<TransformNodeId>,
    pub(crate) local_matrix: Mat4,
    pub(crate) local_origin: glam::Vec3,
    pub(crate) local_generation: u64,
    pub(crate) generation: u64,
    /// Owner position and authored transform derived from local transform,
    /// layout-position, visual-offset, and scroll snapshots. Consumers use
    /// these fields rather than querying a component or arena.
    pub(crate) owner_viewport_position: Vec2,
    pub(crate) owner_viewport_transform: Mat4,
}

impl PartialEq for TransformNodeSnapshot {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.owner == other.owner
            && self.parent == other.parent
            && self.local_matrix.to_cols_array().map(f32::to_bits)
                == other.local_matrix.to_cols_array().map(f32::to_bits)
            && self.local_origin.to_array().map(f32::to_bits)
                == other.local_origin.to_array().map(f32::to_bits)
            && self.local_generation == other.local_generation
            && self.generation == other.generation
            && self.owner_viewport_position.to_array().map(f32::to_bits)
                == other.owner_viewport_position.to_array().map(f32::to_bits)
            && self
                .owner_viewport_transform
                .to_cols_array()
                .map(f32::to_bits)
                == other
                    .owner_viewport_transform
                    .to_cols_array()
                    .map(f32::to_bits)
    }
}

impl Eq for TransformNodeSnapshot {}

impl TransformNodeSnapshot {
    /// Scalar validity shared by fresh graph construction and replay of an
    /// already validated parent graph. Parent membership is a separate proof.
    pub(crate) fn validate_projection_value(&self) -> Result<(), SpatialProjectionError> {
        if self.id.0 != self.owner
            || self.owner.is_null()
            || self.local_generation == 0
            || self.generation == 0
            || self
                .local_matrix
                .to_cols_array()
                .into_iter()
                .chain(self.local_origin.to_array())
                .any(|value| !value.is_finite())
        {
            return Err(SpatialProjectionError::InvalidSnapshot(self.owner));
        }
        Ok(())
    }

    pub(crate) fn has_canonical_derived_projection(self) -> bool {
        let origin = glam::Vec3::new(
            self.owner_viewport_position.x + self.local_origin.x,
            self.owner_viewport_position.y + self.local_origin.y,
            self.local_origin.z,
        );
        crate::view::base_component::compose_transform_about_origin(self.local_matrix, origin)
            .to_cols_array()
            .map(f32::to_bits)
            == self
                .owner_viewport_transform
                .to_cols_array()
                .map(f32::to_bits)
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct LayoutPositionNodeSnapshot {
    pub(crate) id: LayoutPositionNodeId,
    pub(crate) owner: NodeKey,
    pub(crate) reference: SpatialPositionReference,
    pub(crate) reference_scroll: Option<ScrollNodeId>,
    pub(crate) translation_at_scroll_zero: Vec2,
    pub(crate) child_reference_offset_at_scroll_zero: Vec2,
    pub(crate) generation: u64,
}

impl PartialEq for LayoutPositionNodeSnapshot {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.owner == other.owner
            && self.reference == other.reference
            && self.reference_scroll == other.reference_scroll
            && self.translation_at_scroll_zero.to_array().map(f32::to_bits)
                == other
                    .translation_at_scroll_zero
                    .to_array()
                    .map(f32::to_bits)
            && self
                .child_reference_offset_at_scroll_zero
                .to_array()
                .map(f32::to_bits)
                == other
                    .child_reference_offset_at_scroll_zero
                    .to_array()
                    .map(f32::to_bits)
            && self.generation == other.generation
    }
}

impl Eq for LayoutPositionNodeSnapshot {}

#[derive(Clone, Copy, Debug)]
pub(crate) struct VisualOffsetNodeSnapshot {
    pub(crate) id: VisualOffsetNodeId,
    pub(crate) owner: NodeKey,
    pub(crate) parent: Option<VisualOffsetNodeId>,
    pub(crate) offset: Vec2,
    pub(crate) generation: u64,
}

impl PartialEq for VisualOffsetNodeSnapshot {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.owner == other.owner
            && self.parent == other.parent
            && self.offset.to_array().map(f32::to_bits) == other.offset.to_array().map(f32::to_bits)
            && self.generation == other.generation
    }
}

impl Eq for VisualOffsetNodeSnapshot {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SpatialProjectionError {
    DuplicateTransform(TransformNodeId),
    DuplicateLayoutPosition(LayoutPositionNodeId),
    DuplicateVisualOffset(VisualOffsetNodeId),
    DuplicateScroll(ScrollNodeId),
    MissingTransform(TransformNodeId),
    MissingLayoutPosition(LayoutPositionNodeId),
    MissingVisualOffset(VisualOffsetNodeId),
    MissingScroll(ScrollNodeId),
    InvalidSnapshot(NodeKey),
    CyclicTransform(TransformNodeId),
    CyclicLayoutPosition(LayoutPositionNodeId),
    CyclicVisualOffset(VisualOffsetNodeId),
    CyclicScroll(ScrollNodeId),
    InvalidLayoutReference(LayoutPositionNodeId),
}

/// Owner projection derived exclusively from the spatial snapshot graph.
/// `owner_viewport_transform` intentionally contains only the owner's authored
/// transform; ancestor transforms remain separate property-tree edges.
#[derive(Clone, Copy, Debug)]
pub(crate) struct DerivedSpatialProjection {
    pub(crate) owner_viewport_position: Vec2,
    pub(crate) owner_viewport_transform: Mat4,
}

/// Validated, arena-independent view of the four spatial property families.
/// B1b uses this to reproduce the current owner-only viewport transform before
/// B2 migrates individual consumers. Named-anchor placement stays fail-closed
/// until its already-projected anchor origin is decomposed at placement time.
pub(crate) struct SpatialProjectionGraph<'a> {
    transforms: FxHashMap<TransformNodeId, &'a TransformNodeSnapshot>,
    positions: FxHashMap<LayoutPositionNodeId, &'a LayoutPositionNodeSnapshot>,
    visuals: FxHashMap<VisualOffsetNodeId, &'a VisualOffsetNodeSnapshot>,
    scrolls: FxHashMap<ScrollNodeId, &'a ScrollNodeSnapshot>,
    // Successful prefixes belong to these immutable snapshots only. Preserve
    // root-to-leaf floating-point addition order when extending a prefix.
    resolved_positions: std::cell::RefCell<
        ProjectionPrefixCache<'a, LayoutPositionNodeId, LayoutPositionNodeSnapshot>,
    >,
    resolved_visuals:
        std::cell::RefCell<ProjectionPrefixCache<'a, VisualOffsetNodeId, VisualOffsetNodeSnapshot>>,
}

// Scratch sets/vectors are reused across roots of the same immutable graph.
// This retains the exact cycle checks and arithmetic order without allocating
// one transient hash set for every owner on every resolution.
struct ProjectionPrefixCache<'a, Id, Snapshot> {
    resolved: FxHashMap<Id, Vec2>,
    chain: Vec<&'a Snapshot>,
    seen: FxHashSet<Id>,
}
impl<Id, Snapshot> Default for ProjectionPrefixCache<'_, Id, Snapshot> {
    fn default() -> Self {
        Self {
            resolved: FxHashMap::default(),
            chain: Vec::new(),
            seen: FxHashSet::default(),
        }
    }
}

impl<'a> SpatialProjectionGraph<'a> {
    pub(crate) fn try_new(
        transforms: &'a [TransformNodeSnapshot],
        positions: &'a [LayoutPositionNodeSnapshot],
        visuals: &'a [VisualOffsetNodeSnapshot],
        scrolls: &'a [ScrollNodeSnapshot],
    ) -> Result<Self, SpatialProjectionError> {
        let mut graph = Self {
            transforms: FxHashMap::with_capacity_and_hasher(transforms.len(), Default::default()),
            positions: FxHashMap::with_capacity_and_hasher(positions.len(), Default::default()),
            visuals: FxHashMap::with_capacity_and_hasher(visuals.len(), Default::default()),
            scrolls: FxHashMap::with_capacity_and_hasher(scrolls.len(), Default::default()),
            resolved_positions: Default::default(),
            resolved_visuals: Default::default(),
        };

        for snapshot in transforms {
            if graph.transforms.insert(snapshot.id, snapshot).is_some() {
                return Err(SpatialProjectionError::DuplicateTransform(snapshot.id));
            }
            snapshot.validate_projection_value()?;
        }
        for snapshot in positions {
            if graph.positions.insert(snapshot.id, snapshot).is_some() {
                return Err(SpatialProjectionError::DuplicateLayoutPosition(snapshot.id));
            }
            snapshot.validate_projection_value()?;
        }
        for snapshot in visuals {
            if graph.visuals.insert(snapshot.id, snapshot).is_some() {
                return Err(SpatialProjectionError::DuplicateVisualOffset(snapshot.id));
            }
            snapshot.validate_projection_value()?;
        }
        for snapshot in scrolls {
            if graph.scrolls.insert(snapshot.id, snapshot).is_some() {
                return Err(SpatialProjectionError::DuplicateScroll(snapshot.id));
            }
            snapshot.validate_projection_value()?;
        }

        for snapshot in transforms {
            if let Some(parent) = snapshot.parent
                && !graph.transforms.contains_key(&parent)
            {
                return Err(SpatialProjectionError::MissingTransform(parent));
            }
        }
        for snapshot in positions {
            let parent = match snapshot.reference {
                SpatialPositionReference::Viewport
                | SpatialPositionReference::LayoutParent(None) => None,
                SpatialPositionReference::LayoutParent(Some(parent))
                | SpatialPositionReference::Anchor(parent) => Some(LayoutPositionNodeId(parent)),
            };
            if let Some(parent) = parent
                && !graph.positions.contains_key(&parent)
            {
                return Err(SpatialProjectionError::MissingLayoutPosition(parent));
            }
            if let Some(scroll) = snapshot.reference_scroll
                && !graph.scrolls.contains_key(&scroll)
            {
                return Err(SpatialProjectionError::MissingScroll(scroll));
            }
        }
        for snapshot in visuals {
            if let Some(parent) = snapshot.parent
                && !graph.visuals.contains_key(&parent)
            {
                return Err(SpatialProjectionError::MissingVisualOffset(parent));
            }
        }
        for snapshot in scrolls {
            if let Some(parent) = snapshot.parent
                && !graph.scrolls.contains_key(&parent)
            {
                return Err(SpatialProjectionError::MissingScroll(parent));
            }
        }
        validate_parent_forest(
            &graph.transforms,
            transforms.iter().map(|snapshot| snapshot.id),
            |snapshot| snapshot.parent,
        )
        .map_err(SpatialProjectionError::CyclicTransform)?;
        validate_parent_forest(
            &graph.positions,
            positions.iter().map(|snapshot| snapshot.id),
            |snapshot| match snapshot.reference {
                SpatialPositionReference::Viewport
                | SpatialPositionReference::LayoutParent(None) => None,
                SpatialPositionReference::LayoutParent(Some(parent))
                | SpatialPositionReference::Anchor(parent) => Some(LayoutPositionNodeId(parent)),
            },
        )
        .map_err(SpatialProjectionError::CyclicLayoutPosition)?;
        validate_parent_forest(
            &graph.visuals,
            visuals.iter().map(|snapshot| snapshot.id),
            |snapshot| snapshot.parent,
        )
        .map_err(SpatialProjectionError::CyclicVisualOffset)?;
        validate_parent_forest(
            &graph.scrolls,
            scrolls.iter().map(|snapshot| snapshot.id),
            |snapshot| snapshot.parent,
        )
        .map_err(SpatialProjectionError::CyclicScroll)?;

        Ok(graph)
    }

    pub(crate) fn derive_owner_viewport_transform(
        &self,
        id: TransformNodeId,
    ) -> Result<DerivedSpatialProjection, SpatialProjectionError> {
        let transform = self
            .transforms
            .get(&id)
            .copied()
            .ok_or(SpatialProjectionError::MissingTransform(id))?;
        let owner_viewport_position = self.layout_flow_position(transform.owner)?
            + self.cumulative_visual_offset(transform.owner)?;
        let origin = glam::Vec3::new(
            owner_viewport_position.x + transform.local_origin.x,
            owner_viewport_position.y + transform.local_origin.y,
            transform.local_origin.z,
        );
        let owner_viewport_transform = crate::view::base_component::compose_transform_about_origin(
            transform.local_matrix,
            origin,
        );
        if owner_viewport_transform
            .to_cols_array()
            .into_iter()
            .any(|value| !value.is_finite())
        {
            return Err(SpatialProjectionError::InvalidSnapshot(transform.owner));
        }
        Ok(DerivedSpatialProjection {
            owner_viewport_position,
            owner_viewport_transform,
        })
    }

    /// Reconstruct an ordinary paint owner's viewport placement without a
    /// live arena. A missing layout-position or visual-offset family denotes
    /// that family's neutral value for ordinary owners; transform projection
    /// deliberately retains its stricter requirement for both snapshots.
    pub(crate) fn derive_optional_owner_viewport_position(
        &self,
        owner: NodeKey,
    ) -> Result<Vec2, SpatialProjectionError> {
        let layout = if self.positions.contains_key(&LayoutPositionNodeId(owner)) {
            self.layout_flow_position(owner)?
        } else {
            Vec2::ZERO
        };
        let visual = if self.visuals.contains_key(&VisualOffsetNodeId(owner)) {
            self.cumulative_visual_offset(owner)?
        } else {
            Vec2::ZERO
        };
        Ok(layout + visual)
    }

    fn layout_flow_position(&self, owner: NodeKey) -> Result<Vec2, SpatialProjectionError> {
        let mut cache = self.resolved_positions.borrow_mut();
        let ProjectionPrefixCache {
            resolved,
            chain,
            seen,
        } = &mut *cache;
        chain.clear();
        seen.clear();
        let mut cursor = LayoutPositionNodeId(owner);
        let mut position = loop {
            if let Some(position) = resolved.get(&cursor) {
                break *position;
            }
            if !seen.insert(cursor) {
                return Err(SpatialProjectionError::CyclicLayoutPosition(cursor));
            }
            let snapshot = self
                .positions
                .get(&cursor)
                .copied()
                .ok_or(SpatialProjectionError::MissingLayoutPosition(cursor))?;
            chain.push(snapshot);
            cursor = match snapshot.reference {
                SpatialPositionReference::Viewport
                | SpatialPositionReference::LayoutParent(None) => break Vec2::ZERO,
                SpatialPositionReference::LayoutParent(Some(parent)) => {
                    LayoutPositionNodeId(parent)
                }
                SpatialPositionReference::Anchor(anchor) => LayoutPositionNodeId(anchor),
            };
        };
        for snapshot in chain.drain(..).rev() {
            match snapshot.reference {
                SpatialPositionReference::Viewport
                | SpatialPositionReference::LayoutParent(None) => {
                    if snapshot.reference_scroll.is_some() {
                        return Err(SpatialProjectionError::InvalidLayoutReference(snapshot.id));
                    }
                    position = snapshot.translation_at_scroll_zero;
                }
                SpatialPositionReference::LayoutParent(Some(parent)) => {
                    let reference = self
                        .positions
                        .get(&LayoutPositionNodeId(parent))
                        .ok_or(SpatialProjectionError::InvalidLayoutReference(snapshot.id))?;
                    position += reference.child_reference_offset_at_scroll_zero;
                    if let Some(scroll_id) = snapshot.reference_scroll {
                        if scroll_id.0 != parent {
                            return Err(SpatialProjectionError::InvalidLayoutReference(
                                snapshot.id,
                            ));
                        }
                        let scroll = self
                            .scrolls
                            .get(&scroll_id)
                            .copied()
                            .ok_or(SpatialProjectionError::MissingScroll(scroll_id))?;
                        position -= scroll.offset;
                    }
                    position += snapshot.translation_at_scroll_zero;
                }
                SpatialPositionReference::Anchor(anchor) => {
                    if snapshot.reference_scroll.is_some() {
                        return Err(SpatialProjectionError::InvalidLayoutReference(snapshot.id));
                    }
                    // Keep the authored anchor edge and its visual origin;
                    // cached layout prefixes never substitute flattened host geometry.
                    position += self.cumulative_visual_offset(anchor)?;
                    position += snapshot.translation_at_scroll_zero;
                }
            }
            resolved.insert(snapshot.id, position);
        }
        Ok(position)
    }

    fn cumulative_visual_offset(&self, owner: NodeKey) -> Result<Vec2, SpatialProjectionError> {
        let mut cache = self.resolved_visuals.borrow_mut();
        let ProjectionPrefixCache {
            resolved,
            chain,
            seen,
        } = &mut *cache;
        chain.clear();
        seen.clear();
        let mut cursor = VisualOffsetNodeId(owner);
        let mut offset = loop {
            if let Some(offset) = resolved.get(&cursor) {
                break *offset;
            }
            if !seen.insert(cursor) {
                return Err(SpatialProjectionError::CyclicVisualOffset(cursor));
            }
            let snapshot = self
                .visuals
                .get(&cursor)
                .copied()
                .ok_or(SpatialProjectionError::MissingVisualOffset(cursor))?;
            chain.push(snapshot);
            let Some(parent) = snapshot.parent else {
                break Vec2::ZERO;
            };
            cursor = parent;
        };
        for snapshot in chain.drain(..).rev() {
            offset += snapshot.offset;
            resolved.insert(snapshot.id, offset);
        }
        Ok(offset)
    }
}

fn validate_parent_forest<K, V, F>(
    nodes: &FxHashMap<K, &V>,
    starts: impl IntoIterator<Item = K>,
    parent: F,
) -> Result<(), K>
where
    K: Copy + Eq + std::hash::Hash,
    F: Fn(&V) -> Option<K>,
{
    let mut complete = FxHashSet::with_capacity_and_hasher(nodes.len(), Default::default());
    let mut path = FxHashSet::default();
    for start in starts {
        if complete.contains(&start) {
            continue;
        }
        path.clear();
        let mut cursor = Some(start);
        while let Some(id) = cursor {
            if complete.contains(&id) {
                break;
            }
            if !path.insert(id) {
                return Err(id);
            }
            cursor = nodes.get(&id).and_then(|snapshot| parent(snapshot));
        }
        complete.extend(path.drain());
    }
    Ok(())
}

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

/// V2-only spatial decomposition failures. These are intentionally separate
/// from `validation_errors`: B1a is observational and must not change the
/// authority decision of the existing retained path before cutover.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SpatialTreeValidationError {
    MissingLocalTransform(NodeKey),
    NonFiniteLocalTransform(NodeKey),
    InvalidSpatialPlacement(NodeKey),
    MissingSpatialReference(NodeKey),
    Projection(SpatialProjectionError),
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
    pub(crate) const POSITION: Self = Self(1 << 5);
    pub(crate) const VISUAL_OFFSET: Self = Self(1 << 6);

    pub(crate) const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    pub(crate) const fn is_empty(self) -> bool {
        self.0 == 0
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
    pub(crate) layout_positions: FxHashMap<LayoutPositionNodeId, LayoutPositionNode>,
    pub(crate) visual_offsets: FxHashMap<VisualOffsetNodeId, VisualOffsetNode>,
    pub(crate) clips: FxHashMap<ClipNodeId, ClipNode>,
    pub(crate) effects: FxHashMap<EffectNodeId, EffectNode>,
    pub(crate) scrolls: FxHashMap<ScrollNodeId, ScrollNode>,
    // Rebuilt every sync from live observations, not inferred from an absent
    // ScrollNode (which could instead mean an invalid/missing contract).
    inactive_scroll_owners: FxHashSet<NodeKey>,
    transform_generations: FxHashMap<TransformNodeId, u64>,
    local_transform_generations: FxHashMap<TransformNodeId, u64>,
    layout_position_generations: FxHashMap<LayoutPositionNodeId, u64>,
    visual_offset_generations: FxHashMap<VisualOffsetNodeId, u64>,
    clip_generations: FxHashMap<ClipNodeId, u64>,
    effect_generations: FxHashMap<EffectNodeId, u64>,
    scroll_generations: FxHashMap<ScrollNodeId, u64>,
    pub(crate) states: FxHashMap<NodeKey, NodePropertyState>,
    pub(crate) changes: FxHashMap<NodeKey, PropertyChangeFlags>,
    pub(crate) validation_errors: Vec<PropertyTreeValidationError>,
    pub(crate) spatial_validation_errors: Vec<SpatialTreeValidationError>,
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
        let derived = node.derived_projection?;
        let mut snapshot = TransformNodeSnapshot {
            id,
            owner: node.owner,
            parent: node.parent,
            local_matrix: node.local_matrix,
            local_origin: node.local_origin,
            local_generation: node.local_generation,
            generation: node.generation,
            owner_viewport_position: derived.owner_viewport_position,
            owner_viewport_transform: derived.owner_viewport_transform,
        };
        if !snapshot.has_canonical_derived_projection() {
            // Preserve owner attribution for fail-closed planners while
            // making the already-required nonzero generation proof fail.
            // Returning `None` would make a corrupted existing node
            // indistinguishable from a missing property-tree node.
            snapshot.generation = 0;
        }
        Some(snapshot)
    }

    pub(crate) fn layout_position_snapshot_for(
        &self,
        id: LayoutPositionNodeId,
    ) -> Option<LayoutPositionNodeSnapshot> {
        let node = self.layout_positions.get(&id)?;
        Some(LayoutPositionNodeSnapshot {
            id,
            owner: node.owner,
            reference: node.reference,
            reference_scroll: node.reference_scroll,
            translation_at_scroll_zero: node.translation_at_scroll_zero,
            child_reference_offset_at_scroll_zero: node.child_reference_offset_at_scroll_zero,
            generation: node.generation,
        })
    }

    pub(crate) fn visual_offset_snapshot_for(
        &self,
        id: VisualOffsetNodeId,
    ) -> Option<VisualOffsetNodeSnapshot> {
        let node = self.visual_offsets.get(&id)?;
        Some(VisualOffsetNodeSnapshot {
            id,
            owner: node.owner,
            parent: node.parent,
            offset: node.offset,
            generation: node.generation,
        })
    }

    pub(crate) fn transform_snapshot_chain_for(
        &self,
        leaf: Option<TransformNodeId>,
    ) -> Option<Vec<TransformNodeSnapshot>> {
        let mut snapshots = Vec::new();
        let mut seen = FxHashSet::default();
        let mut cursor = leaf;
        while let Some(id) = cursor {
            if !seen.insert(id) {
                return None;
            }
            let snapshot = self.transform_snapshot_for(id)?;
            cursor = snapshot.parent;
            snapshots.push(snapshot);
        }
        Some(snapshots)
    }

    pub(crate) fn layout_position_snapshot_chain_for(
        &self,
        leaf: Option<LayoutPositionNodeId>,
    ) -> Option<Vec<LayoutPositionNodeSnapshot>> {
        let mut snapshots = Vec::new();
        let mut seen = FxHashSet::default();
        let mut cursor = leaf;
        while let Some(id) = cursor {
            if !seen.insert(id) {
                return None;
            }
            let snapshot = self.layout_position_snapshot_for(id)?;
            cursor = match snapshot.reference {
                SpatialPositionReference::Viewport
                | SpatialPositionReference::LayoutParent(None) => None,
                SpatialPositionReference::LayoutParent(Some(parent))
                | SpatialPositionReference::Anchor(parent) => Some(LayoutPositionNodeId(parent)),
            };
            snapshots.push(snapshot);
        }
        Some(snapshots)
    }

    pub(crate) fn visual_offset_snapshot_chain_for(
        &self,
        leaf: Option<VisualOffsetNodeId>,
    ) -> Option<Vec<VisualOffsetNodeSnapshot>> {
        self.visual_offset_id_chain_for(leaf)?
            .into_iter()
            .map(|id| self.visual_offset_snapshot_for(id))
            .collect()
    }

    fn visual_offset_id_chain_for(
        &self,
        leaf: Option<VisualOffsetNodeId>,
    ) -> Option<Vec<VisualOffsetNodeId>> {
        let mut ids = Vec::new();
        let mut seen = FxHashSet::default();
        let mut cursor = leaf;
        while let Some(id) = cursor {
            if !seen.insert(id) {
                return None;
            }
            let node = self.visual_offsets.get(&id)?;
            cursor = node.parent;
            ids.push(id);
        }
        Some(ids)
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

    pub(crate) fn scroll_snapshot_chain_for(
        &self,
        leaf: Option<ScrollNodeId>,
    ) -> Option<Vec<ScrollNodeSnapshot>> {
        let mut snapshots = Vec::new();
        let mut seen = FxHashSet::default();
        let mut cursor = leaf;
        while let Some(id) = cursor {
            if !seen.insert(id) {
                return None;
            }
            let snapshot = self.scroll_snapshot_for(id)?;
            cursor = snapshot.parent;
            snapshots.push(snapshot);
        }
        Some(snapshots)
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

    pub(crate) fn clip_node_snapshot_for(&self, id: ClipNodeId) -> Option<ClipNodeSnapshot> {
        let node = self.clips.get(&id)?;
        let ClipGeometry::LogicalScissor(logical_scissor) = node.geometry else {
            return None;
        };
        Some(ClipNodeSnapshot {
            id,
            owner: node.owner,
            parent: node.parent,
            logical_scissor,
            behavior: node.behavior,
            generation: node.generation,
        })
    }

    pub(crate) fn effect_node_snapshot_for(&self, id: EffectNodeId) -> Option<EffectNodeSnapshot> {
        let node = self.effects.get(&id)?;
        Some(EffectNodeSnapshot {
            id,
            owner: node.owner,
            parent: node.parent,
            opacity: node.opacity,
            generation: node.generation,
        })
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
            let snapshot = self.clip_node_snapshot_for(id)?;
            cursor = snapshot.parent;
            snapshots.push(snapshot);
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
            let snapshot = self.effect_node_snapshot_for(id)?;
            cursor = snapshot.parent;
            snapshots.push(snapshot);
        }
        Some(snapshots)
    }

    /// Rebuilds the composition cache from the four canonical spatial source
    /// families. No component-owned viewport projection participates in this
    /// derivation.
    fn refresh_derived_spatial_projections(&mut self) {
        for node in self.transforms.values_mut() {
            node.derived_projection = None;
        }

        let transforms = self
            .transforms
            .iter()
            .map(|(&id, node)| TransformNodeSnapshot {
                id,
                owner: node.owner,
                parent: node.parent,
                local_matrix: node.local_matrix,
                local_origin: node.local_origin,
                local_generation: node.local_generation,
                generation: node.generation,
                // `SpatialProjectionGraph` reads only the canonical source
                // fields above. These placeholders cannot become inputs.
                owner_viewport_position: Vec2::ZERO,
                owner_viewport_transform: Mat4::IDENTITY,
            })
            .collect::<Vec<_>>();
        let mut required_positions = FxHashSet::default();
        let mut required_visuals = FxHashSet::default();
        let mut required_scrolls = FxHashSet::default();
        for transform in &transforms {
            let mut position_chain = Vec::new();
            let mut position_seen = FxHashSet::default();
            let mut position_cursor = Some(LayoutPositionNodeId(transform.owner));
            let mut position_chain_is_complete = true;
            while let Some(id) = position_cursor {
                if !position_seen.insert(id) {
                    position_chain_is_complete = false;
                    break;
                }
                let Some(node) = self.layout_positions.get(&id) else {
                    position_chain_is_complete = false;
                    break;
                };
                position_chain.push(id);
                if let Some(scroll) = node.reference_scroll {
                    let mut scroll_chain = Vec::new();
                    let mut scroll_seen = FxHashSet::default();
                    let mut scroll_cursor = Some(scroll);
                    while let Some(scroll_id) = scroll_cursor {
                        if !scroll_seen.insert(scroll_id) {
                            position_chain_is_complete = false;
                            break;
                        }
                        let Some(scroll_node) = self.scrolls.get(&scroll_id) else {
                            position_chain_is_complete = false;
                            break;
                        };
                        scroll_chain.push(scroll_id);
                        scroll_cursor = scroll_node.parent;
                    }
                    if !position_chain_is_complete {
                        break;
                    }
                    required_scrolls.extend(scroll_chain);
                }
                if let SpatialPositionReference::Anchor(anchor) = node.reference {
                    let Some(anchor_visual_chain) =
                        self.visual_offset_id_chain_for(Some(VisualOffsetNodeId(anchor)))
                    else {
                        position_chain_is_complete = false;
                        break;
                    };
                    required_visuals.extend(anchor_visual_chain);
                }
                position_cursor = match node.reference {
                    SpatialPositionReference::Viewport
                    | SpatialPositionReference::LayoutParent(None) => None,
                    SpatialPositionReference::LayoutParent(Some(parent))
                    | SpatialPositionReference::Anchor(parent) => {
                        Some(LayoutPositionNodeId(parent))
                    }
                };
            }
            if position_chain_is_complete {
                required_positions.extend(position_chain);
            }

            if let Some(visual_chain) =
                self.visual_offset_id_chain_for(Some(VisualOffsetNodeId(transform.owner)))
            {
                required_visuals.extend(visual_chain);
            }
        }

        let positions = self
            .layout_positions
            .iter()
            .filter(|(id, _)| required_positions.contains(id))
            .map(|(&id, node)| LayoutPositionNodeSnapshot {
                id,
                owner: node.owner,
                reference: node.reference,
                reference_scroll: node.reference_scroll,
                translation_at_scroll_zero: node.translation_at_scroll_zero,
                child_reference_offset_at_scroll_zero: node.child_reference_offset_at_scroll_zero,
                generation: node.generation,
            })
            .collect::<Vec<_>>();
        let visuals = self
            .visual_offsets
            .iter()
            .filter(|(id, _)| required_visuals.contains(id))
            .map(|(&id, node)| VisualOffsetNodeSnapshot {
                id,
                owner: node.owner,
                parent: node.parent,
                offset: node.offset,
                generation: node.generation,
            })
            .collect::<Vec<_>>();
        let scrolls = self
            .scrolls
            .iter()
            .filter(|(id, _)| required_scrolls.contains(id))
            .map(|(&id, node)| ScrollNodeSnapshot {
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
            .collect::<Vec<_>>();

        let graph =
            match SpatialProjectionGraph::try_new(&transforms, &positions, &visuals, &scrolls) {
                Ok(graph) => graph,
                Err(error) => {
                    self.spatial_validation_errors
                        .push(SpatialTreeValidationError::Projection(error));
                    return;
                }
            };

        for snapshot in &transforms {
            let derived = match graph.derive_owner_viewport_transform(snapshot.id) {
                Ok(derived) => derived,
                Err(error) => {
                    self.spatial_validation_errors
                        .push(SpatialTreeValidationError::Projection(error));
                    continue;
                }
            };
            if let Some(node) = self.transforms.get_mut(&snapshot.id) {
                node.derived_projection = Some(derived);
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn refresh_derived_spatial_projections_for_test(&mut self) {
        self.spatial_validation_errors.clear();
        self.refresh_derived_spatial_projections();
    }

    pub(crate) fn sync(&mut self, arena: &NodeArena, roots: &[NodeKey]) {
        self.epoch = self.epoch.wrapping_add(1);
        self.inactive_scroll_owners.clear();
        self.changes.clear();
        self.validation_errors.clear();
        self.spatial_validation_errors.clear();
        let mut seen = FxHashSet::default();
        for &root in roots {
            self.sync_subtree(arena, root, PropertyTreeState::default(), true, &mut seen);
        }
        self.prune_unseen(arena, &seen);
        self.refresh_derived_spatial_projections();
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

        self.sync_spatial_nodes(arena, key, node.element.as_ref());
        let layout_position = self
            .layout_positions
            .contains_key(&LayoutPositionNodeId(key))
            .then_some(LayoutPositionNodeId(key))
            .or(inherited.layout_position);
        let visual_offset = self
            .visual_offsets
            .contains_key(&VisualOffsetNodeId(key))
            .then_some(VisualOffsetNodeId(key))
            .or(inherited.visual_offset);

        let local_snapshot = node.element.compositor_local_transform_snapshot();
        if local_snapshot.is_none() && node.element.has_retained_transform_surface() {
            self.spatial_validation_errors
                .push(SpatialTreeValidationError::MissingLocalTransform(key));
        }
        let transform = if let Some(local_snapshot) = local_snapshot {
            let id = TransformNodeId(key);
            let previous = self.transforms.get(&id).copied();
            let parent_changed = previous.is_some_and(|node| node.parent != inherited.transform);
            let local_matrix = local_snapshot.to_cols_array();
            let local_origin = local_snapshot.origin();
            if local_matrix.iter().any(|value| !value.is_finite())
                || local_origin.iter().any(|value| !value.is_finite())
            {
                self.validation_errors
                    .push(PropertyTreeValidationError::NonFiniteTransform(key));
                self.spatial_validation_errors
                    .push(SpatialTreeValidationError::NonFiniteLocalTransform(key));
            }
            let local_changed = previous.is_none_or(|node| {
                !matrix_bits_equal(node.local_matrix, local_matrix)
                    || !vec3_bits_equal(node.local_origin, local_origin)
            });
            let changed = parent_changed || local_changed;
            let generation = if changed {
                self.bump_transform_generation(id)
            } else {
                previous
                    .map(|node| node.generation)
                    .unwrap_or_else(|| self.bump_transform_generation(id))
            };
            let local_generation = if local_changed {
                self.bump_local_transform_generation(id)
            } else {
                previous
                    .map(|node| node.local_generation)
                    .unwrap_or_else(|| self.bump_local_transform_generation(id))
            };
            self.transforms.insert(
                id,
                TransformNode {
                    owner: key,
                    parent: inherited.transform,
                    local_matrix: Mat4::from_cols_array(&local_matrix),
                    local_origin: glam::Vec3::from_array(local_origin),
                    local_generation,
                    generation,
                    derived_projection: None,
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
                ScrollGeometryObservation::Inactive => {
                    self.inactive_scroll_owners.insert(key);
                    None
                }
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
            .exact_retained_self_clip_scissor_rect(key, arena, is_frame_root)
            .or_else(|| {
                node.element
                    .exact_generic_subtree_self_clip_scissor_rect(key, arena, is_frame_root)
            });
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
            layout_position,
            visual_offset,
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

    fn sync_spatial_nodes(
        &mut self,
        arena: &NodeArena,
        key: NodeKey,
        element: &dyn crate::view::base_component::ElementTrait,
    ) {
        let position_id = LayoutPositionNodeId(key);
        let visual_id = VisualOffsetNodeId(key);
        let Some(snapshot) = element.compositor_spatial_placement_snapshot() else {
            self.layout_positions.remove(&position_id);
            self.visual_offsets.remove(&visual_id);
            return;
        };

        let translation = snapshot.translation_at_scroll_zero();
        let viewport_translation = snapshot.viewport_translation_at_scroll_zero();
        let child_reference_offset = snapshot.child_reference_offset_at_scroll_zero();
        let visual_offset = snapshot.visual_offset();
        let viewport_position = snapshot.compatibility_viewport_position();
        if translation
            .iter()
            .chain(viewport_translation.iter())
            .chain(child_reference_offset.iter())
            .chain(visual_offset.iter())
            .chain(viewport_position.iter())
            .any(|value| !value.is_finite())
        {
            self.spatial_validation_errors
                .push(SpatialTreeValidationError::InvalidSpatialPlacement(key));
            self.layout_positions.remove(&position_id);
            self.visual_offsets.remove(&visual_id);
            return;
        }

        let reference = match snapshot.reference() {
            SpatialPositionReferenceSnapshot::Viewport => Some(SpatialPositionReference::Viewport),
            SpatialPositionReferenceSnapshot::LayoutParent(None) => {
                Some(SpatialPositionReference::LayoutParent(arena.parent_of(key)))
            }
            SpatialPositionReferenceSnapshot::LayoutParent(Some(stable_id)) => arena
                .find_by_stable_id(stable_id)
                .filter(|parent| Some(*parent) == arena.parent_of(key))
                .map(|parent| SpatialPositionReference::LayoutParent(Some(parent))),
            SpatialPositionReferenceSnapshot::Anchor(stable_id) => arena
                .find_by_stable_id(stable_id)
                .map(SpatialPositionReference::Anchor),
        };
        let Some(reference) = reference else {
            self.spatial_validation_errors
                .push(SpatialTreeValidationError::MissingSpatialReference(key));
            self.layout_positions.remove(&position_id);
            self.visual_offsets.remove(&visual_id);
            return;
        };

        let translation_at_scroll_zero = Vec2::from_array(match reference {
            SpatialPositionReference::LayoutParent(None) => viewport_translation,
            SpatialPositionReference::Viewport
            | SpatialPositionReference::LayoutParent(Some(_))
            | SpatialPositionReference::Anchor(_) => translation,
        });
        let child_reference_offset_at_scroll_zero = Vec2::from_array(child_reference_offset);
        let reference_scroll = match reference {
            SpatialPositionReference::LayoutParent(Some(parent)) => {
                let parent_node = arena.get(parent);
                let declared_scroll = parent_node.as_ref().is_some_and(|node| {
                    node.element.retained_paint_properties().is_scroll_container
                });
                let applied_scroll = parent_node.as_ref().is_some_and(|node| {
                    let (x, y) = node.element.get_scroll_offset();
                    x.to_bits() != 0.0_f32.to_bits() || y.to_bits() != 0.0_f32.to_bits()
                });
                // Parent-first sync has already resolved this owner's actual
                // scroll observation. An inactive declaration with zero offset
                // contributes no subtraction to the child's spatial edge.
                // Nonzero offsets and invalid contracts still require a node.
                ((declared_scroll && !self.inactive_scroll_owners.contains(&parent))
                    || applied_scroll
                    || self.scrolls.contains_key(&ScrollNodeId(parent)))
                .then_some(ScrollNodeId(parent))
            }
            SpatialPositionReference::Viewport
            | SpatialPositionReference::LayoutParent(None)
            | SpatialPositionReference::Anchor(_) => None,
        };
        let previous = self.layout_positions.get(&position_id).copied();
        let position_changed = previous.is_none_or(|previous| {
            previous.reference != reference
                || previous.reference_scroll != reference_scroll
                || !vec2_bits_equal(
                    previous.translation_at_scroll_zero,
                    translation_at_scroll_zero.to_array(),
                )
                || !vec2_bits_equal(
                    previous.child_reference_offset_at_scroll_zero,
                    child_reference_offset_at_scroll_zero.to_array(),
                )
        });
        let generation = if position_changed {
            self.bump_layout_position_generation(position_id)
        } else {
            previous
                .map(|node| node.generation)
                .unwrap_or_else(|| self.bump_layout_position_generation(position_id))
        };
        self.layout_positions.insert(
            position_id,
            LayoutPositionNode {
                owner: key,
                reference,
                reference_scroll,
                translation_at_scroll_zero,
                child_reference_offset_at_scroll_zero,
                generation,
            },
        );
        if position_changed {
            self.mark_change(key, PropertyChangeFlags::POSITION);
        }

        let offset = Vec2::from_array(visual_offset);
        // Layout placement always receives the immediate arena parent's
        // cumulative visual offset, even when an absolute node selects a
        // viewport or named-anchor position reference. Keep that inheritance
        // distinct from the layout-position reference graph.
        let parent = arena.parent_of(key).and_then(|parent| {
            #[cfg(test)]
            if !self
                .visual_offsets
                .contains_key(&VisualOffsetNodeId(parent))
            {
                return None;
            }
            Some(VisualOffsetNodeId(parent))
        });
        let previous = self.visual_offsets.get(&visual_id).copied();
        let visual_changed = previous.is_none_or(|previous| {
            previous.parent != parent || !vec2_bits_equal(previous.offset, offset.to_array())
        });
        let generation = if visual_changed {
            self.bump_visual_offset_generation(visual_id)
        } else {
            previous
                .map(|node| node.generation)
                .unwrap_or_else(|| self.bump_visual_offset_generation(visual_id))
        };
        self.visual_offsets.insert(
            visual_id,
            VisualOffsetNode {
                owner: key,
                parent,
                offset,
                generation,
            },
        );
        if visual_changed {
            self.mark_change(key, PropertyChangeFlags::VISUAL_OFFSET);
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

    fn bump_local_transform_generation(&mut self, id: TransformNodeId) -> u64 {
        let generation = self.local_transform_generations.entry(id).or_insert(0);
        *generation = generation.saturating_add(1);
        *generation
    }

    fn bump_layout_position_generation(&mut self, id: LayoutPositionNodeId) -> u64 {
        let generation = self.layout_position_generations.entry(id).or_insert(0);
        *generation = generation.saturating_add(1);
        *generation
    }

    fn bump_visual_offset_generation(&mut self, id: VisualOffsetNodeId) -> u64 {
        let generation = self.visual_offset_generations.entry(id).or_insert(0);
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
        self.layout_positions.retain(|id, _| seen.contains(&id.0));
        self.visual_offsets.retain(|id, _| seen.contains(&id.0));
        self.clips.retain(|id, _| seen.contains(&id.owner));
        self.effects.retain(|id, _| seen.contains(&id.0));
        self.scrolls.retain(|id, _| seen.contains(&id.0));
        // Active property state follows the current roots, but tombstone
        // counters follow the owner's generational arena lifetime. A node can
        // temporarily leave the active root set and later reattach with the
        // same NodeKey; its generations must remain monotonic across that gap.
        self.transform_generations
            .retain(|id, _| arena.contains_key(id.0));
        self.local_transform_generations
            .retain(|id, _| arena.contains_key(id.0));
        self.layout_position_generations
            .retain(|id, _| arena.contains_key(id.0));
        self.visual_offset_generations
            .retain(|id, _| arena.contains_key(id.0));
        self.effect_generations
            .retain(|id, _| arena.contains_key(id.0));
        self.clip_generations
            .retain(|id, _| arena.contains_key(id.owner));
        self.scroll_generations
            .retain(|id, _| arena.contains_key(id.0));
        self.states.retain(|key, _| seen.contains(key));
    }

    /// Final observed changes survive layout's consumption of LAYOUT/PLACE.
    /// These are work-selection hints, not evidence that an absent flag proves
    /// a complete native command payload or a resident GPU allocation valid.
    pub(crate) fn changes_for(&self, key: NodeKey) -> PropertyChangeFlags {
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

fn vec2_bits_equal(vector: Vec2, snapshot: [f32; 2]) -> bool {
    vector
        .to_array()
        .into_iter()
        .zip(snapshot)
        .all(|(left, right)| left.to_bits() == right.to_bits())
}

fn vec3_bits_equal(vector: glam::Vec3, snapshot: [f32; 3]) -> bool {
    vector
        .to_array()
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
mod tests;

#[cfg(test)]
mod spatial_projection_tests;

#[cfg(test)]
mod spatial_prefix_tests;

impl LayoutPositionNodeSnapshot {
    pub(crate) fn validate_projection_value(&self) -> Result<(), SpatialProjectionError> {
        if self.id.0 != self.owner
            || self.owner.is_null()
            || self.generation == 0
            || self
                .translation_at_scroll_zero
                .to_array()
                .into_iter()
                .chain(self.child_reference_offset_at_scroll_zero.to_array())
                .any(|value| !value.is_finite())
        {
            return Err(SpatialProjectionError::InvalidSnapshot(self.owner));
        }
        Ok(())
    }
}

impl VisualOffsetNodeSnapshot {
    pub(crate) fn validate_projection_value(&self) -> Result<(), SpatialProjectionError> {
        if self.id.0 != self.owner
            || self.owner.is_null()
            || self.generation == 0
            || self
                .offset
                .to_array()
                .into_iter()
                .any(|value| !value.is_finite())
        {
            return Err(SpatialProjectionError::InvalidSnapshot(self.owner));
        }
        Ok(())
    }
}

impl ScrollNodeSnapshot {
    pub(crate) fn validate_projection_value(&self) -> Result<(), SpatialProjectionError> {
        if self.id.0 != self.owner
            || self.owner.is_null()
            || self.generation == 0
            || self
                .offset
                .to_array()
                .into_iter()
                .any(|value| !value.is_finite())
        {
            return Err(SpatialProjectionError::InvalidSnapshot(self.owner));
        }
        Ok(())
    }
}
