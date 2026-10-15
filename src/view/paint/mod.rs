//! Complete paint recording, generic surface planning, and retained execution.

mod artifact;
mod compiler;
mod composite_edge;
mod coverage_manifest;
pub(crate) mod coverage_walk;

mod frame_recorder;

mod property_transition;
mod recorder;
mod recording_context;
mod shared_sequence;
mod spatial_projection;
pub(crate) use spatial_projection::{
    ArtifactSpatialProjection, OwnerSnapPoints, snapped_owner_paint_offsets,
};

mod surface_dag;

#[allow(unused_imports)]
pub(crate) use artifact::{
    DrawRectOp, PaintArtifact, PaintArtifactContractRejection, PaintArtifactContractViolation,
    PaintArtifactSpaceTransition, PaintAtomicProjectionArtifactSource, PaintChunk, PaintChunkFrame,
    PaintChunkId, PaintChunkMetadata, PaintChunkRasterIdentity, PaintChunkRole,
    PaintContentRevision, PaintDeferredViewportSelfClipWitness, PaintNodePhase, PaintNodePlan,
    PaintOp, PaintOwnerPropertyStateSnapshot, PaintOwnerSnap, PaintOwnerSnapshot,
    PaintPayloadIdentity, PaintPropertyScope, PaintTextContentSource, PaintTextPreeditWitness,
    PaintTextSelectionSource, PaintTextSelectionWitness, PreparedImageIdentity, PreparedImageOp,
    PreparedInlineIfcDecorationDescriptor, PreparedInlineIfcDecorationIdentity,
    PreparedInlineIfcDecorationOp, PreparedScrollbarOverlayIdentity, PreparedScrollbarOverlayOp,
    PreparedShadowIdentity, PreparedShadowOp, PreparedSvgIdentity, PreparedSvgOp,
    PreparedTextIdentity, PreparedTextOp, RETAINED_CHILD_MASK_SLOT, RetainedChildMaskPlan,
    bounds_with_shadow_ink, preedit_glyph_identity_is_exact, preedit_underline_identity_is_exact,
};
#[cfg(any(test, feature = "renderer-test-support"))]
pub(crate) use compiler::take_last_production_actions_for_test;
#[allow(unused_imports)]
pub(crate) use compiler::{
    ArtifactSurfaceCompositeGeometryStamp, ArtifactSurfaceLocalizationError,
    ArtifactSurfacePaintOpKind, ArtifactSurfaceRasterContext, ArtifactSurfaceRasterPlanError,
    ArtifactSurfaceRasterTargetId, ArtifactSurfaceResidentSealError, PlanningCache,
    PreparedArtifactSurfaceFrame, PreparedArtifactSurfaceRasterChunk,
    PreparedArtifactSurfaceRasterNode, PreparedArtifactSurfaceRasterPlan,
    PreparedArtifactSurfaceRasterRoot, PreparedArtifactSurfaceRasterSpan,
    PreparedArtifactSurfaceRasterStep, ResolvedClip, RetainedSurfaceChunkStamp,
    RetainedSurfaceCompileAction, RetainedSurfaceRasterIdentity, RetainedSurfaceRasterInputs,
    RetainedSurfaceRasterRole, RetainedSurfaceRasterStamp, RetainedSurfaceResidentKey,
    SealedArtifactSurfaceResidentEntry, SealedArtifactSurfaceResidentSet,
    SingleTargetSurfaceDagPrepareError, emit_prepared_artifact_surface_frame_from_pool,
    localize_artifact_surface_op, prepare_artifact_surface_raster_plan,
    prepare_artifact_surface_raster_plan_cached, seal_prepared_artifact_surface_frame,
};
#[cfg(test)]
pub(crate) use compiler::{
    artifact_surface_op_corresponds_to_source_for_test,
    artifact_surface_op_has_baked_opacity_for_test, neutralize_artifact_surface_opacity_for_test,
    resolve_artifact_surface_clip_for_test,
};
#[cfg(test)]
pub(crate) use compiler::{compile_artifact, take_artifact_compile_count, try_compile_artifact};
#[cfg(test)]
pub(crate) use composite_edge::PaintCompositeEdge;
#[cfg(test)]
pub(crate) use coverage_manifest::record_coverage_manifest;
#[allow(unused_imports)]
pub(crate) use coverage_manifest::{
    CoverageOrder, CoverageRecordingMode, PaintCoverageItem, PaintCoverageManifest,
};

pub(crate) use frame_recorder::record_frame_artifact;
#[cfg(test)]
pub(crate) use frame_recorder::{
    record_surface_dag_frame_artifact, record_surface_dag_frame_artifact_cached,
};

#[allow(unused_imports)]
pub(crate) use property_transition::{
    ArtifactCursor, ArtifactOwnerGraph, ArtifactSceneTarget, ArtifactTransitionRequest,
    ClassifiedTransitionEvent, OwnerPropertyStateEndpoint, PropertySnapshotGraph,
    PropertyStateReferenceError, TransitionError, artifact_cursors,
    classify_artifact_transition_sequence, classify_property_transition,
};
pub(crate) use recording_context::PaintRecordingContext;
#[cfg(test)]
pub(crate) use surface_dag::derive_artifact_surface_candidates;
#[allow(unused_imports)]
pub(crate) use surface_dag::{
    ArtifactSurfaceCandidate, ArtifactSurfaceCoverageForest, ArtifactSurfaceCoverageNode,
    ArtifactSurfaceCoverageRoot, ArtifactSurfaceCoverageSpan, ArtifactSurfaceCoverageStep,
    LayerizationPolicy, SurfaceDag, SurfaceDagClipClosureProjection, SurfaceDagClipRebase,
    SurfaceDagError, SurfaceDagExecutionNode, SurfaceDagExecutionNodeId, SurfaceDagExecutionOrder,
    SurfaceDagExecutionRoot, SurfaceDagExecutionTargetId, SurfaceDagNode, SurfaceDagNodeId,
    SurfaceDagNodeKind, SurfaceDagSceneRoot, SurfaceDagSceneRootId, SurfaceDagTargetId,
    SurfaceMaterializationDecision, SurfaceMaterializationOutcome,
};
#[cfg(test)]
pub(crate) fn canonical_manifest_matches_for_test(
    metadata: &PaintCoverageManifest,
    full: &PaintCoverageManifest,
) -> bool {
    frame_recorder::canonical_manifest_matches(metadata, full)
}
#[cfg(test)]
pub(crate) use recorder::{note_full_artifact_record, take_full_artifact_record_count};

#[cfg(test)]
pub(crate) mod tests;

#[cfg(test)]
pub(crate) mod planning_tests;
#[cfg(test)]
pub(crate) use planning_tests::{native_scroll_forest_plan_fixture, nested_scroll_plan_fixture};

pub(crate) use artifact::PreparedGpuOp;
pub(crate) use artifact::{artifact_plan_in_layout_frame, metadata_plan_in_layout_frame};

pub(crate) mod work_profile;

mod recording_cache;
pub(crate) use recording_cache::RecordingCache;

#[cfg(test)]
pub(crate) use surface_dag::derive_artifact_surface_coverage_forest;

#[cfg(test)]
pub(crate) use surface_dag::derive_artifact_surface_transition_requests;

#[cfg(test)]
pub(crate) use surface_dag::reconstruct_surface_dag;
