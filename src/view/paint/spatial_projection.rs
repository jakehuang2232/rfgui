//! The compiler's single relative-to-absolute conversion. Property snapshots
//! carry only relative spatial edges and local transforms; owner viewport
//! positions and owner transforms are derived here, once per artifact, from
//! one spatial projection graph.
use glam::{Mat4, Vec2};
use rustc_hash::FxHashMap;

use crate::view::compositor::property_tree::{
    DerivedSpatialProjection, SpatialProjectionError, SpatialProjectionGraph, TransformNodeId,
};
use crate::view::node_arena::NodeKey;

use super::PaintArtifact;

#[derive(Debug)]
pub(crate) struct ArtifactSpatialProjection {
    transforms: FxHashMap<TransformNodeId, DerivedSpatialProjection>,
    owners: FxHashMap<NodeKey, Vec2>,
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
        Ok(Self { transforms, owners })
    }

    /// The owner's authored transform conjugated about its viewport origin.
    /// Ancestor transforms remain separate property-tree edges.
    pub(crate) fn owner_viewport_transform(&self, id: TransformNodeId) -> Option<Mat4> {
        self.transforms
            .get(&id)
            .map(|derived| derived.owner_viewport_transform)
    }

    /// Scroll-zero viewport position of a paint owner of this artifact.
    pub(crate) fn owner_viewport_position(&self, owner: NodeKey) -> Option<Vec2> {
        self.owners.get(&owner).copied()
    }
}
