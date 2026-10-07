//! The compiler's single relative-to-absolute conversion. Property snapshots
//! carry only relative spatial edges, local transforms and owner-local clip
//! geometry, and recorded chunks are owner-local; owner viewport positions,
//! owner transforms, clip scissors and chunk placement are derived here, once
//! per artifact, from one spatial projection graph.
use glam::{Mat4, Vec2};
use rustc_hash::FxHashMap;

use crate::view::compositor::property_tree::{
    ClipNodeSnapshot, DerivedSpatialProjection, SpatialProjectionError, SpatialProjectionGraph,
    TransformNodeId,
};
use crate::view::node_arena::NodeKey;

use super::PaintArtifact;

#[derive(Debug)]
pub(crate) struct ArtifactSpatialProjection {
    transforms: FxHashMap<TransformNodeId, DerivedSpatialProjection>,
    owners: FxHashMap<NodeKey, Vec2>,
    /// Origins of the layout frames owner-local clips and chunks are placed in.
    frames: FxHashMap<NodeKey, Vec2>,
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
        let owners = artifact
            .owner_nodes
            .iter()
            .map(|snapshot| {
                Ok((
                    snapshot.owner,
                    graph.derive_optional_owner_viewport_position(snapshot.owner)?,
                ))
            })
            .collect::<Result<_, SpatialProjectionError>>()?;
        let mut frames = FxHashMap::default();
        for owner in owner_local_geometry_owners(artifact) {
            if let std::collections::hash_map::Entry::Vacant(frame) = frames.entry(owner) {
                frame.insert(graph.derive_owner_frame_origin(owner)?);
            }
        }
        Ok(Self {
            transforms,
            owners,
            frames,
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

    /// Viewport position of a paint owner of this artifact.
    pub(crate) fn owner_viewport_position(&self, owner: NodeKey) -> Option<Vec2> {
        self.owners.get(&owner).copied()
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
