//! Helpers used only by unit tests.

use super::*;

impl PaintCompositeEdge {
    pub(crate) fn validate_schedule(
        &self,
        owner: NodeKey,
        phase: super::super::PaintNodePhase,
        slot: u16,
        role: super::super::PaintChunkRole,
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

    pub(super) fn draw_pass(&self) -> Option<DrawRectPass> {
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

use crate::view::base_component::UiBuildContext;
use crate::view::frame_graph::FrameGraph;
use crate::view::render_pass::draw_rect_pass::{DrawRectInput, DrawRectOutput, DrawRectPass};
