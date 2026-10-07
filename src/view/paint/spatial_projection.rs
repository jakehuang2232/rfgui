//! The compiler's single relative-to-absolute conversion. Property snapshots
//! carry only relative spatial edges, local transforms and owner-local clip
//! geometry, and recorded chunks are owner-local and unsnapped; owner
//! transforms, clip scissors, chunk placement and every owner's pixel snap
//! are derived here, once per artifact, from one spatial projection graph.
use glam::{Mat4, Vec2};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::view::base_component::paint_offset_after_owner_snap;
use crate::view::compositor::property_tree::{
    ClipNodeSnapshot, DerivedSpatialProjection, SpatialProjectionError, SpatialProjectionGraph,
    TransformNodeId,
};
use crate::view::node_arena::NodeKey;

use super::{PaintArtifact, PaintOwnerSnap};

#[derive(Debug)]
pub(crate) struct ArtifactSpatialProjection {
    transforms: FxHashMap<TransformNodeId, DerivedSpatialProjection>,
    /// Origins of the layout frames owner-local clips and chunks are placed in.
    frames: FxHashMap<NodeKey, Vec2>,
    owner_snap_points: std::sync::Arc<[OwnerSnapPoints]>,
    /// Every owner's snapped paint offset for a zero host offset.
    owner_paint_offsets: FxHashMap<NodeKey, [f32; 2]>,
}

/// The viewport points where one owner meets the pixel grid, in order; each
/// snaps the paint offset reaching it. An owner without points paints at its
/// parent's paint offset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OwnerSnapPoints {
    pub(crate) owner: NodeKey,
    pub(crate) parent: Option<NodeKey>,
    pub(crate) points_bits: [Option<[u32; 2]>; 2],
}

impl OwnerSnapPoints {
    /// Places the owner's frame-relative snap points at their frame's origin.
    fn try_new(
        graph: &SpatialProjectionGraph<'_>,
        owner: NodeKey,
        parent: Option<NodeKey>,
        snap: PaintOwnerSnap,
    ) -> Result<Self, SpatialProjectionError> {
        let origin = match snap.frame {
            Some(frame) if snap.points_bits != [None, None] => {
                graph.derive_owner_frame_origin(frame.0)?
            }
            _ => Vec2::ZERO,
        };
        let mut points_bits = [None; 2];
        for (placed, point) in points_bits.iter_mut().zip(snap.points_bits) {
            let Some(point) = point else {
                continue;
            };
            let [x, y] = point.map(f32::from_bits);
            let viewport = [origin.x + x, origin.y + y];
            if viewport.iter().any(|value| !value.is_finite()) {
                return Err(SpatialProjectionError::InvalidSnapshot(owner));
            }
            *placed = Some(viewport.map(f32::to_bits));
        }
        Ok(Self {
            owner,
            parent,
            points_bits,
        })
    }
}

/// Every owner's paint offset after its ancestors' snaps and its own,
/// starting from `start` at the owner roots. This is the legacy renderer's
/// owner snap chain, so placement reproduces its pixel positions.
pub(crate) fn snapped_owner_paint_offsets(
    owners: &[OwnerSnapPoints],
    start: [f32; 2],
) -> Result<FxHashMap<NodeKey, [f32; 2]>, SpatialProjectionError> {
    let mut by_owner = FxHashMap::with_capacity_and_hasher(owners.len(), Default::default());
    for owner in owners {
        if by_owner.insert(owner.owner, *owner).is_some() {
            return Err(SpatialProjectionError::InvalidSnapshot(owner.owner));
        }
    }
    let mut offsets: FxHashMap<NodeKey, [f32; 2]> =
        FxHashMap::with_capacity_and_hasher(owners.len(), Default::default());
    let mut chain = Vec::new();
    let mut seen = FxHashSet::default();
    for owner in owners {
        if offsets.contains_key(&owner.owner) {
            continue;
        }
        chain.clear();
        seen.clear();
        let mut cursor = owner.owner;
        // Artifact owner stores preserve canonical traversal order, not a
        // parent-first topological order. Resolve the bounded ancestry
        // explicitly so a child-first store cannot change snap semantics.
        let mut paint_offset = loop {
            if let Some(offset) = offsets.get(&cursor) {
                break *offset;
            }
            if chain.len() >= usize::from(u8::MAX) || !seen.insert(cursor) {
                return Err(SpatialProjectionError::InvalidSnapshot(cursor));
            }
            let current = by_owner
                .get(&cursor)
                .copied()
                .ok_or(SpatialProjectionError::InvalidSnapshot(cursor))?;
            chain.push(current);
            match current.parent {
                Some(parent) => cursor = parent,
                None => break start,
            }
        };
        for current in chain.drain(..).rev() {
            for point in current.points_bits.into_iter().flatten() {
                paint_offset =
                    paint_offset_after_owner_snap(point.map(f32::from_bits), paint_offset)
                        .ok_or(SpatialProjectionError::InvalidSnapshot(current.owner))?;
            }
            offsets.insert(current.owner, paint_offset);
        }
    }
    Ok(offsets)
}

impl ArtifactSpatialProjection {
    pub(crate) fn try_new(artifact: &PaintArtifact) -> Result<Self, SpatialProjectionError> {
        let graph = SpatialProjectionGraph::try_new(
            &artifact.transform_nodes,
            &artifact.layout_position_nodes,
            &artifact.visual_offset_nodes,
            &artifact.scroll_nodes,
        )?;
        let transforms = artifact
            .transform_nodes
            .iter()
            .map(|snapshot| {
                Ok((
                    snapshot.id,
                    graph.derive_owner_viewport_transform(snapshot.id)?,
                ))
            })
            .collect::<Result<_, SpatialProjectionError>>()?;
        let mut frames = FxHashMap::default();
        for owner in owner_local_geometry_owners(artifact) {
            if let std::collections::hash_map::Entry::Vacant(frame) = frames.entry(owner) {
                frame.insert(graph.derive_owner_frame_origin(owner)?);
            }
        }
        let owner_snap_points = artifact
            .owner_nodes
            .iter()
            .map(|snapshot| {
                OwnerSnapPoints::try_new(&graph, snapshot.owner, snapshot.parent, snapshot.snap)
            })
            .collect::<Result<std::sync::Arc<[_]>, _>>()?;
        let owner_paint_offsets = snapped_owner_paint_offsets(&owner_snap_points, [0.0, 0.0])?;
        Ok(Self {
            transforms,
            frames,
            owner_snap_points,
            owner_paint_offsets,
        })
    }

    /// Whether this projection holds the layout frame of every owner of
    /// owner-local geometry in `artifact`. Frame origins depend only on the
    /// spatial snapshots.
    pub(crate) fn frames_owner_geometry(&self, artifact: &PaintArtifact) -> bool {
        owner_local_geometry_owners(artifact).all(|owner| self.frames.contains_key(&owner))
    }

    /// Viewport origin of `frame`'s own layout frame, where owner-local clips
    /// of `frame` and layout-frame chunks naming it are placed.
    pub(crate) fn frame_origin(&self, frame: NodeKey) -> Option<[f32; 2]> {
        self.frames.get(&frame).map(|origin| origin.to_array())
    }

    /// Places every owner-local clip at its owner's layout frame, leaving each
    /// clip a viewport scissor.
    pub(crate) fn place_clips(
        &self,
        clips: &mut [ClipNodeSnapshot],
    ) -> Result<(), SpatialProjectionError> {
        for clip in clips {
            let origin = if clip.geometry.is_owner_local() {
                *self
                    .frames
                    .get(&clip.owner)
                    .ok_or(SpatialProjectionError::InvalidClip(clip.id))?
            } else {
                Vec2::ZERO
            };
            clip.geometry = clip
                .geometry
                .placed_at(origin)
                .ok_or(SpatialProjectionError::InvalidClip(clip.id))?;
        }
        Ok(())
    }

    /// The owner's authored transform conjugated about its viewport origin.
    /// Ancestor transforms remain separate property-tree edges.
    pub(crate) fn owner_viewport_transform(&self, id: TransformNodeId) -> Option<Mat4> {
        self.transforms
            .get(&id)
            .map(|derived| derived.owner_viewport_transform)
    }

    /// The snap points of every owner of this artifact.
    pub(crate) fn owner_snap_points(&self) -> &std::sync::Arc<[OwnerSnapPoints]> {
        &self.owner_snap_points
    }

    /// The paint offset an owner of this artifact paints at for a zero host
    /// offset: the sum of its ancestors' pixel snaps and its own.
    pub(crate) fn owner_paint_offset(&self, owner: NodeKey) -> Option<[f32; 2]> {
        self.owner_paint_offsets.get(&owner).copied()
    }
}

fn owner_local_geometry_owners(artifact: &PaintArtifact) -> impl Iterator<Item = NodeKey> + '_ {
    artifact
        .clip_nodes
        .iter()
        .filter(|clip| clip.geometry.is_owner_local())
        .map(|clip| clip.owner)
        .chain(
            artifact
                .chunks
                .iter()
                .filter(|chunk| chunk.frame == super::PaintChunkFrame::Layout)
                .filter_map(|chunk| chunk.properties.layout_position)
                .map(|frame| frame.0),
        )
}
