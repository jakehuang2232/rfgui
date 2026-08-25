use super::super::{PaintChunkId, PaintNodePhase, RETAINED_CHILD_MASK_SLOT};
use super::{
    ArtifactSurfaceRasterTargetId, PreparedArtifactSurfaceFrame,
    PreparedArtifactSurfaceRasterChunk, PreparedArtifactSurfaceRasterStep,
    SurfaceDagExecutionNodeId,
};

/// One child-mask transition sealed in painter order for the artifact executor.
///
/// The paired raster chunk remains the source of the mask draw payload. This
/// value freezes only whether that chunk opens, closes, or leaves the stencil
/// scope unchanged, so the executor never re-derives the decision from slot
/// and phase metadata.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // C3b3b1 seals these actions before its executor consumes them.
enum ArtifactSurfaceChildMaskAction {
    Unchanged,
    Push,
    Pop,
}

impl ArtifactSurfaceChildMaskAction {
    #[allow(dead_code)] // C3b3b1 execution sealing is the first production caller.
    fn from_chunk_id(id: PaintChunkId) -> Self {
        if id.slot != RETAINED_CHILD_MASK_SLOT {
            return Self::Unchanged;
        }
        match id.phase {
            PaintNodePhase::BeforeChildren => Self::Push,
            PaintNodePhase::AfterChildren => Self::Pop,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)] // C3b3b1 keeps span and nested steps in one execution stream.
enum ArtifactSurfaceChildMaskStep {
    ArtifactSpan {
        step_index: usize,
        chunk_actions: Vec<ArtifactSurfaceChildMaskAction>,
    },
    NestedSurface(SurfaceDagExecutionNodeId),
}

/// Child-mask program for one scene-root or detached-surface render target.
///
/// Each target starts with stencil depth zero. A parent target retains its
/// scope across `NestedSurface`, while the nested surface owns a separate
/// target and therefore a separate zero-based program.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)] // C3b3b1 is the first production consumer of target programs.
struct ArtifactSurfaceChildMaskTargetProgram {
    target: ArtifactSurfaceRasterTargetId,
    max_mask_depth: usize,
    steps: Vec<ArtifactSurfaceChildMaskStep>,
}

/// Graph-inert execution seal consumed only by the C3b3b1 emitter.
///
/// The compiler-owned frame and its child-mask programs stay together. No
/// public parts accessor exists, so a caller cannot substitute a mask program
/// derived from a different prepared frame.
#[derive(Debug)]
#[allow(dead_code)] // C3b3b1 consumes the sealed frame and programs together.
pub(super) struct PreparedArtifactSurfaceExecution {
    frame: PreparedArtifactSurfaceFrame,
    root_programs: Vec<ArtifactSurfaceChildMaskTargetProgram>,
    node_programs: Vec<ArtifactSurfaceChildMaskTargetProgram>,
}

/// Computes the absolute maximum child-mask depth for one ordered chunk-id
/// stream. `carried_in_depth` preserves the legacy segment emitter's scopes
/// that may span several calls; complete C3 targets pass zero.
pub(super) fn artifact_child_mask_max_depth(
    ids: impl IntoIterator<Item = PaintChunkId>,
    carried_in_depth: usize,
) -> usize {
    let mut depth = carried_in_depth;
    let mut maximum = 0;
    for id in ids {
        if id.slot != RETAINED_CHILD_MASK_SLOT {
            continue;
        }
        match id.phase {
            PaintNodePhase::BeforeChildren => {
                depth = depth.saturating_add(1);
                maximum = maximum.max(depth);
            }
            PaintNodePhase::AfterChildren => depth = depth.saturating_sub(1),
        }
    }
    maximum
}

#[allow(dead_code)] // C3b3b1 execution sealing is the first production caller.
fn seal_target_program(
    target: ArtifactSurfaceRasterTargetId,
    steps: &[PreparedArtifactSurfaceRasterStep],
) -> ArtifactSurfaceChildMaskTargetProgram {
    let max_mask_depth = artifact_child_mask_max_depth(
        steps
            .iter()
            .flat_map(step_chunks)
            .map(|chunk| chunk.source().id),
        0,
    );
    let steps = steps
        .iter()
        .enumerate()
        .map(|(step_index, step)| match step {
            PreparedArtifactSurfaceRasterStep::ArtifactSpan(span) => {
                ArtifactSurfaceChildMaskStep::ArtifactSpan {
                    step_index,
                    chunk_actions: span
                        .chunks()
                        .iter()
                        .map(|chunk| {
                            ArtifactSurfaceChildMaskAction::from_chunk_id(chunk.source().id)
                        })
                        .collect(),
                }
            }
            PreparedArtifactSurfaceRasterStep::NestedSurface(child) => {
                ArtifactSurfaceChildMaskStep::NestedSurface(*child)
            }
        })
        .collect();
    ArtifactSurfaceChildMaskTargetProgram {
        target,
        max_mask_depth,
        steps,
    }
}

#[allow(dead_code)] // C3b3b1 target sealing walks each prepared artifact span.
fn step_chunks(step: &PreparedArtifactSurfaceRasterStep) -> &[PreparedArtifactSurfaceRasterChunk] {
    match step {
        PreparedArtifactSurfaceRasterStep::ArtifactSpan(span) => span.chunks(),
        PreparedArtifactSurfaceRasterStep::NestedSurface(_) => &[],
    }
}

/// Seals only execution facts absent from `PreparedArtifactSurfaceFrame`.
/// Descriptor, resident, graph-key, and compile-action validation remain in
/// their existing owners; graph declared-key collision is checked immediately
/// before target allocation in C3b3b1.
#[allow(dead_code)] // C3b3b1 executor is the first production caller of this seal.
pub(super) fn seal_prepared_artifact_surface_execution(
    frame: PreparedArtifactSurfaceFrame,
) -> PreparedArtifactSurfaceExecution {
    let root_programs = frame
        .raster_plan()
        .roots()
        .iter()
        .map(|root| {
            seal_target_program(
                ArtifactSurfaceRasterTargetId::SceneRoot(root.scene_root()),
                root.steps(),
            )
        })
        .collect();
    let node_programs = frame
        .raster_plan()
        .nodes()
        .iter()
        .map(|node| {
            seal_target_program(
                ArtifactSurfaceRasterTargetId::Surface(node.source()),
                node.steps(),
            )
        })
        .collect();
    PreparedArtifactSurfaceExecution {
        frame,
        root_programs,
        node_programs,
    }
}

#[cfg(test)]
mod tests;
