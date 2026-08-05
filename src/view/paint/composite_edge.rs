use crate::view::base_component::{Rect, UiBuildContext};
use crate::view::compositor::property_tree::{ClipBehavior, ClipNodeId, PropertyTreeState};
use crate::view::frame_graph::FrameGraph;
use crate::view::node_arena::NodeKey;
use crate::view::render_pass::draw_rect_pass::{DrawRectInput, DrawRectOutput, DrawRectPass};

use super::{
    DrawRectOp, PaintArtifact, PaintArtifactContractRejection, PaintArtifactContractViolation,
    PaintChunk, PaintChunkId, PaintOp, PaintPayloadIdentity,
};

/// One arena-independent primitive drawn at a compositing boundary.
///
/// Presence is per primitive: callers represent a hidden or culled primitive
/// by omitting its edge. No sibling edge may act as its presence authority.
#[derive(Clone, Debug)]
pub(crate) struct PaintCompositeEdge {
    pub(crate) id: PaintChunkId,
    pub(crate) owner: NodeKey,
    pub(crate) bounds_bits: [u32; 4],
    pub(crate) properties: PropertyTreeState,
    pub(crate) logical_scissor: Option<[u32; 4]>,
    pub(crate) payload_identity: PaintPayloadIdentity,
    pub(crate) op: DrawRectOp,
}

impl PartialEq for PaintCompositeEdge {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.owner == other.owner
            && self.bounds_bits == other.bounds_bits
            && self.properties == other.properties
            && self.logical_scissor == other.logical_scissor
            && self.payload_identity == other.payload_identity
            && PaintPayloadIdentity::prepared_rects([&self.op])
                == PaintPayloadIdentity::prepared_rects([&other.op])
    }
}

impl Eq for PaintCompositeEdge {}

impl PaintCompositeEdge {
    pub(crate) fn new_draw_rect(
        id: PaintChunkId,
        owner: NodeKey,
        bounds: Rect,
        properties: PropertyTreeState,
        logical_scissor: Option<[u32; 4]>,
        op: DrawRectOp,
    ) -> Option<Self> {
        let bounds_bits = [bounds.x, bounds.y, bounds.width, bounds.height].map(f32::to_bits);
        let payload_identity = PaintPayloadIdentity::prepared_rects([&op])?;
        let edge = Self {
            id,
            owner,
            bounds_bits,
            properties,
            logical_scissor,
            payload_identity,
            op,
        };
        edge.is_canonical().then_some(edge)
    }

    #[allow(dead_code)]
    pub(crate) fn from_artifact_chunk(
        artifact: &PaintArtifact,
        chunk: &PaintChunk,
    ) -> Option<Self> {
        let [PaintOp::DrawRect(op)] = &artifact.ops[chunk.op_range.clone()] else {
            return None;
        };
        let logical_scissor = resolved_artifact_scissor(artifact, chunk.properties.clip)?;
        let edge = Self::new_draw_rect(
            chunk.id,
            chunk.owner,
            chunk.bounds,
            chunk.properties,
            logical_scissor,
            op.clone(),
        )?;
        (edge.payload_identity == chunk.payload_identity).then_some(edge)
    }

    pub(crate) fn is_canonical(&self) -> bool {
        self.validate().is_ok()
    }

    pub(crate) fn validate(&self) -> Result<(), PaintArtifactContractRejection> {
        let [x, y, width, height] = self.bounds_bits.map(f32::from_bits);
        if self.id.owner != self.owner {
            return Err(PaintArtifactContractRejection {
                owner: self.owner,
                violation: PaintArtifactContractViolation::CompositeOwner,
            });
        }
        if [x, y, width, height]
                .into_iter()
                .any(|value| !value.is_finite())
            || width <= 0.0
            || height <= 0.0
            || !(x + width).is_finite()
            || !(y + height).is_finite()
            || self.op.params.position.map(f32::to_bits)
                != [self.bounds_bits[0], self.bounds_bits[1]]
            || self.op.params.size.map(f32::to_bits) != [self.bounds_bits[2], self.bounds_bits[3]]
        {
            return Err(PaintArtifactContractRejection {
                owner: self.owner,
                violation: PaintArtifactContractViolation::CompositeBounds,
            });
        }
        if PaintPayloadIdentity::prepared_rects([&self.op]).as_ref()
            != Some(&self.payload_identity)
        {
            return Err(PaintArtifactContractRejection {
                owner: self.owner,
                violation: PaintArtifactContractViolation::CompositePayload,
            });
        }
        if !self
            .logical_scissor
            .is_none_or(|[left, top, scissor_width, scissor_height]| {
                let Some(right) = left.checked_add(scissor_width) else {
                    return false;
                };
                let Some(bottom) = top.checked_add(scissor_height) else {
                    return false;
                };
                scissor_width > 0
                    && scissor_height > 0
                    && x < right as f32
                    && x + width > left as f32
                    && y < bottom as f32
                    && y + height > top as f32
            })
        {
            return Err(PaintArtifactContractRejection {
                owner: self.owner,
                violation: PaintArtifactContractViolation::CompositeClip,
            });
        }
        Ok(())
    }

    pub(crate) fn validate_schedule(
        &self,
        owner: NodeKey,
        phase: super::PaintNodePhase,
        slot: u16,
        role: super::PaintChunkRole,
    ) -> Result<(), PaintArtifactContractRejection> {
        self.validate()?;
        if self.owner != owner
            || self.id.owner != owner
            || self.id.phase != phase
            || self.id.slot != slot
            || self.id.role != role
        {
            return Err(PaintArtifactContractRejection {
                owner,
                violation: PaintArtifactContractViolation::CompositePhaseOrder,
            });
        }
        Ok(())
    }

    pub(crate) fn validate_source_parity(
        &self,
        expected: &Self,
    ) -> Result<(), PaintArtifactContractRejection> {
        self.validate()?;
        expected.validate()?;
        (self == expected)
            .then_some(())
            .ok_or(PaintArtifactContractRejection {
                owner: expected.owner,
                violation: PaintArtifactContractViolation::CompositeSourceParity,
            })
    }

    fn draw_pass(&self) -> Option<DrawRectPass> {
        self.is_canonical().then(|| {
            let mut pass = DrawRectPass::new(
                self.op.params.clone(),
                DrawRectInput::default(),
                DrawRectOutput::default(),
            );
            pass.set_render_mode(self.op.mode);
            pass
        })
    }
}

pub(crate) fn paint_composite_edge_opaque_delta(edges: &[PaintCompositeEdge]) -> Option<u32> {
    edges.iter().try_fold(0u32, |count, edge| {
        count.checked_add(u32::from(edge.draw_pass()?.is_opaque_candidate()))
    })
}

pub(crate) fn emit_paint_composite_edges(
    edges: &[PaintCompositeEdge],
    graph: &mut FrameGraph,
    ctx: &mut UiBuildContext,
) {
    for edge in edges {
        let pass = edge
            .draw_pass()
            .expect("prepared composite edge must remain canonical");
        let previous = ctx.replace_scissor_rect(edge.logical_scissor);
        ctx.emit_draw_rect_pass(graph, pass);
        ctx.replace_scissor_rect(previous);
    }
}

fn resolved_artifact_scissor(
    artifact: &PaintArtifact,
    leaf: Option<ClipNodeId>,
) -> Option<Option<[u32; 4]>> {
    let Some(mut cursor) = leaf else {
        return Some(None);
    };
    let mut seen = Vec::new();
    let mut intersection: Option<[u32; 4]> = None;
    loop {
        if seen.contains(&cursor) {
            return None;
        }
        seen.push(cursor);
        let clip = artifact.clip_nodes.iter().find(|clip| clip.id == cursor)?;
        if clip.behavior != ClipBehavior::Intersect {
            return None;
        }
        intersection = Some(match intersection {
            Some(current) => intersect_logical_scissors(current, clip.logical_scissor)?,
            None => clip.logical_scissor,
        });
        let Some(parent) = clip.parent else {
            break;
        };
        cursor = parent;
    }
    Some(intersection)
}

pub(crate) fn intersect_logical_scissors(a: [u32; 4], b: [u32; 4]) -> Option<[u32; 4]> {
    let left = a[0].max(b[0]);
    let top = a[1].max(b[1]);
    let right = a[0].checked_add(a[2])?.min(b[0].checked_add(b[2])?);
    let bottom = a[1].checked_add(a[3])?.min(b[1].checked_add(b[3])?);
    (right > left && bottom > top).then_some([left, top, right - left, bottom - top])
}
