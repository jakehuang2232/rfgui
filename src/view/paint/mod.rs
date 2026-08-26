//! Retained paint recording scaffold.
//!
//! The first slice deliberately records only side-effect-free leaf Element
//! decoration. Every other host remains on the existing Renderable path.

mod artifact;
mod compiler;
mod composite_edge;
mod coverage_manifest;
mod frame_plan;
mod frame_recorder;
mod legacy_admission;
mod legacy_recording;
mod property_transition;
mod recorder;
mod recording_context;
mod retained_surface_executor;
mod scroll_content;
mod scroll_scene;
mod scroll_tiles;
mod surface_dag;

#[allow(unused_imports)]
pub(crate) use artifact::{
    ConsumedAncestorEffectWitness, ConsumedAncestorProperty, ConsumedAncestorPropertyStackWitness,
    ConsumedAncestorScrollContentsWitness, ConsumedAncestorTransformWitness,
    ConsumedPropertyForestAncestorChainWitness, ConsumedSameOwnerEffectBoundaryWitness,
    ConsumedSameOwnerTransformBoundaryWitness, DrawRectOp, EffectPropertyContentWitness,
    EffectPropertySurfaceArtifactContract, PaintArtifact, PaintArtifactContractRejection,
    PaintArtifactContractViolation, PaintArtifactSpaceTransition,
    PaintArtifactSpaceTranslationBits, PaintArtifactTarget, PaintAtomicProjectionArtifactSource,
    PaintBakedScrollHostWitness, PaintChunk, PaintChunkId, PaintChunkMetadata,
    PaintChunkRasterIdentity, PaintChunkRole, PaintContentRevision,
    PaintDeferredViewportEffectWitness, PaintDeferredViewportSelfClipWitness, PaintNodePhase,
    PaintNodePlan, PaintOp, PaintOpacityAuthority, PaintOwnerPropertyStateSnapshot,
    PaintOwnerSnapshot, PaintPayloadIdentity, PaintPropertyScope, PaintScrollContentWitness,
    PaintScrollForestEdgeWitness, PaintTextContentSource, PaintTextPreeditWitness,
    PaintTextSelectionSource, PaintTextSelectionWitness, PaintTransformSurfaceWitness,
    PreparedImageIdentity, PreparedImageOp, PreparedInlineIfcDecorationDescriptor,
    PreparedInlineIfcDecorationIdentity, PreparedInlineIfcDecorationOp,
    PreparedScrollbarOverlayIdentity, PreparedScrollbarOverlayOp, PreparedShadowIdentity,
    PreparedShadowOp, PreparedSvgIdentity, PreparedSvgOp, PreparedTextIdentity, PreparedTextOp,
    PropertyForestBoundarySnapshot, RETAINED_CHILD_MASK_SLOT, RetainedChildMaskPlan,
    TextPayloadNodeIdentity, TextPayloadNodeKind, TextPreeditPayloadIdentity,
    has_canonical_paint_bounds, preedit_glyph_identity_is_exact,
    preedit_underline_identity_is_exact,
};
#[allow(unused_imports)]
// C3 state/lifecycle landed before the C4 producer consumes every stamp type.
pub(crate) use compiler::{
    ArtifactCompileErrorKind, ArtifactSurfaceCompositeGeometryStamp, ArtifactSurfaceExecutionError,
    ArtifactSurfaceLocalizationError, ArtifactSurfacePaintOpKind, ArtifactSurfaceRasterContext,
    ArtifactSurfaceRasterPlanError, ArtifactSurfaceRasterTargetId,
    ArtifactSurfaceResidentSealError, NestedSurfaceRasterDependency, PreparedArtifactSurfaceFrame,
    PreparedArtifactSurfaceRasterChunk, PreparedArtifactSurfaceRasterNode,
    PreparedArtifactSurfaceRasterPlan, PreparedArtifactSurfaceRasterRoot,
    PreparedArtifactSurfaceRasterSpan, PreparedArtifactSurfaceRasterStep,
    PropertyEffectCompositeBasisStamp, PropertyEffectRasterIdentityInputs, ResolvedClip,
    RetainedAtomicProjectionTextAreaResidentRasterSeal, RetainedPropertySceneTransactionStamp,
    RetainedScrollHostRasterDependency, RetainedSurfaceArtifactSpanStamp,
    RetainedSurfaceChunkStamp, RetainedSurfaceCompileAction, RetainedSurfaceCompositeGeometryStamp,
    RetainedSurfaceRasterIdentity, RetainedSurfaceRasterInputs, RetainedSurfaceRasterRole,
    RetainedSurfaceRasterStamp, RetainedSurfaceRasterStampParts, RetainedSurfaceRasterStepStamp,
    RetainedSurfaceResidentKey, RootEffectCompileAction, RootEffectRasterInputs,
    RootEffectRasterStamp, SealedArtifactSurfaceResidentEntry, SealedArtifactSurfaceResidentSet,
    SingleTargetSurfaceDagPrepareError, ValidatedEffectPropertySurfaceArtifact,
    ValidatedSingleTargetSurfaceDagFrame, emit_prepared_artifact_surface_frame_from_pool,
    emit_single_target_surface_dag_frame, emit_validated_effect_property_surface_artifact,
    localize_artifact_surface_op, prepare_artifact_surface_raster_plan,
    prepare_single_target_surface_dag_frame, property_effect_composite_geometry_stamp_is_canonical,
    property_effect_surface_raster_stamp_is_canonical_at_depth,
    property_effect_surface_raster_stamp_validates_contract_at_depth,
    retained_isolation_composite_geometry_stamp,
    retained_nested_isolation_composite_geometry_stamp,
    retained_property_effect_composite_geometry_stamp, retained_surface_composite_geometry_stamp,
    retained_surface_raster_stamp_is_canonical,
    retained_surface_raster_stamp_is_canonical_at_depth, seal_prepared_artifact_surface_frame,
    try_compile_artifact, try_compile_root_effect_artifact,
    validate_effect_property_surface_artifact,
    validated_effect_property_surface_artifact_span_stamp,
    validated_isolation_surface_artifact_span_stamp,
    validated_property_effect_surface_raster_stamp, validated_retained_surface_artifact_span_stamp,
    validated_retained_surface_raster_stamp, validated_retained_surface_tree_raster_stamp,
    validated_root_effect_raster_stamp, validated_scroll_content_raster_stamp,
    validated_scroll_content_tile_raster_stamp, validated_scroll_host_artifact_span_stamp,
    validated_scroll_host_raster_stamp, validated_scroll_text_area_content_raster_stamp,
};
#[cfg(test)]
pub(crate) use compiler::{
    artifact_surface_op_corresponds_to_source_for_test,
    artifact_surface_op_has_baked_opacity_for_test, neutralize_artifact_surface_opacity_for_test,
    resolve_artifact_surface_clip_for_test, validate_media_content_artifact_for_test,
};
#[cfg(test)]
pub(crate) use compiler::{compile_artifact, take_artifact_compile_count};
pub(crate) use composite_edge::{
    PaintCompositeEdge, emit_paint_composite_edges, intersect_logical_scissors,
    paint_composite_edge_opaque_delta,
};
#[allow(unused_imports)]
pub(crate) use coverage_manifest::{
    CoverageOrder, CoverageRecordingMode, PaintCoverageItem, PaintCoverageManifest,
    PaintCoverageStats, PaintCoverageValidationError, PlannedBoundary, PlannedBoundaryCutoutSet,
    PlannedBoundaryKind, record_coverage_manifest,
};
#[cfg(test)]
pub(crate) use frame_plan::tests::nested_scroll_fixture as nested_scroll_fixture_for_test;
#[cfg(test)]
pub(crate) use frame_plan::tests::{native_scroll_forest_plan_fixture, nested_scroll_plan_fixture};
#[allow(unused_imports)]
pub(crate) use frame_plan::{
    ArtifactSpanPlan, FramePaintPlan, FramePaintPlanError, FramePaintPlanRejection, PaintPlanStep,
    RetainedSurfacePlan, SurfaceKind, TransformSurfacePlanContext,
    plan_native_scroll_forest_scaffold_with_context, plan_property_effect_scene_with_context,
    plan_single_root_isolation_surface, plan_single_root_scroll_host_surface,
    plan_single_root_transform_child_isolation_surface,
    plan_single_root_transform_child_isolation_surface_with_context,
    plan_single_root_transform_surface, plan_single_root_transform_surface_with_context,
    plan_transform_property_scene_with_context,
};
#[cfg(test)]
pub(crate) use frame_plan::{
    PropertySceneTopLevelSurfaceWitness, PropertySceneTransactionRootWitness,
    PropertySceneTransactionSurfaceKind, PropertySceneTransactionSurfaceWitness,
    PropertySceneTransactionWitness,
};
#[allow(unused_imports)]
pub(crate) use frame_recorder::{
    ForcedFrameArtifactError, FrameArtifactDebugBoundary, FrameArtifactDebugBoundaryKind,
    FrameArtifactEligibility, FrameArtifactFallbackReason, FrameArtifactRecordOutcome,
    RendererMode, record_clip_enabled_frame_artifact, record_closed_single_target_frame_artifact,
    record_frame_artifact, record_property_neutral_frame_artifact,
    record_root_group_opacity_frame_artifact,
};
/// Legacy retained admission surface. Deleted whole in the Stage C hard
/// cutover; see `legacy_admission`.
#[allow(unused_imports)]
pub(crate) use legacy_admission::{
    LegacyTextAreaProjection, PaintLegacyTextAreaCoverageAuthority,
    PaintScrollAtomicProjectionSelectionTextAreaSubtreeWitness,
    PaintScrollAtomicProjectionTextAreaRecorderWitness,
    PaintScrollDetachedProjectionSubtreeWitness,
    PaintScrollFocusedAtomicProjectionTextAreaSubtreeWitness,
    PaintScrollInteractiveTextAreaSubtreeWitness, PaintScrollTextAreaSubtreeWitness,
    RetainedInteractiveTextAreaResidentRasterSeal,
    RetainedScrollAtomicProjectionSelectionTextAreaSubtreeAdmissionSnapshot,
    RetainedScrollAtomicProjectionTextAreaSubtreeAdmissionSnapshot,
    RetainedScrollFocusedAtomicProjectionTextAreaSubtreeAdmissionSnapshot,
    RetainedScrollInteractiveTextAreaSubtreeAdmissionSnapshot,
    RetainedScrollTextAreaSubtreeAdmissionSnapshot,
    exact_retained_property_scroll_text_area_paint_source,
    exact_retained_scroll_atomic_projection_selection_text_area_subtree_admission,
    exact_retained_scroll_atomic_projection_text_area_subtree_admission,
    exact_retained_scroll_focused_atomic_projection_text_area_subtree_admission,
    exact_retained_scroll_interactive_text_area_subtree_admission,
    exact_retained_scroll_text_area_subtree_admission,
};
#[allow(unused_imports)]
pub(crate) use property_transition::{
    ArtifactCursor, ArtifactOwnerGraph, ArtifactSceneTarget, ArtifactTransitionRequest,
    ClassifiedTransitionEvent, OwnerPropertyStateEndpoint, PropertySnapshotGraph,
    PropertyStateReferenceError, TransitionError, artifact_cursors,
    classify_artifact_transition_sequence, classify_property_transition,
};
pub(crate) use recording_context::PaintRecordingContext;
#[allow(unused_imports)]
pub(crate) use surface_dag::{
    ArtifactSurfaceCandidate, ArtifactSurfaceCoverageForest, ArtifactSurfaceCoverageNode,
    ArtifactSurfaceCoverageRoot, ArtifactSurfaceCoverageSpan, ArtifactSurfaceCoverageStep,
    LayerizationPolicy, SurfaceDag, SurfaceDagClipClosureProjection, SurfaceDagClipRebase,
    SurfaceDagError, SurfaceDagExecutionNode, SurfaceDagExecutionNodeId, SurfaceDagExecutionOrder,
    SurfaceDagExecutionRoot, SurfaceDagExecutionTargetId, SurfaceDagNode, SurfaceDagNodeId,
    SurfaceDagNodeKind, SurfaceDagSceneRoot, SurfaceDagSceneRootId, SurfaceDagTargetId,
    derive_artifact_surface_candidates, derive_artifact_surface_coverage_forest,
    derive_artifact_surface_transition_requests, reconstruct_surface_dag,
};
#[cfg(test)]
pub(crate) fn canonical_manifest_matches_for_test(
    metadata: &PaintCoverageManifest,
    full: &PaintCoverageManifest,
) -> bool {
    frame_recorder::canonical_manifest_matches(metadata, full)
}
#[allow(unused_imports)]
pub(crate) use recorder::{LegacyPaintReason, PaintRecordOutcome, record_root};
#[cfg(test)]
pub(crate) use recorder::{note_full_artifact_record, take_full_artifact_record_count};
#[cfg(test)]
#[allow(unused_imports)]
pub(crate) use retained_surface_executor::{
    ForcedTransformSurfaceError, PropertyBoundaryForestPrepareTamper,
    build_retained_property_scene_with_forced_pool_for_test,
    execute_forced_transform_surface_for_test, prepare_forced_retained_surface_stamp_for_test,
    prepare_property_boundary_forest_with_tamper_for_test,
    prepare_retained_property_scene_stamps_for_test, prepare_retained_scroll_host_stamp_for_test,
};
#[allow(unused_imports)]
pub(crate) use retained_surface_executor::{
    PreparedRetainedPropertyScene, RetainedPropertySceneBuildOutcome,
    RetainedPropertySceneBuildTrace, RetainedPropertySceneTransaction, RetainedSurfaceBuildOutcome,
    RetainedSurfaceBuildTrace, RetainedSurfacePrepareError, RetainedSurfaceTreeBuildOutcome,
    build_retained_effect_tree_from_pool, build_retained_isolation_surface_from_pool,
    build_retained_scroll_host_surface_from_pool, build_retained_surface_from_pool,
    build_retained_surface_tree_from_pool, emit_prepared_retained_property_scene,
    prepare_retained_property_scene_from_pool,
};
#[allow(unused_imports)]
pub(crate) use scroll_content::{
    PreparedScrollContentCompositeGeometry, PreparedScrollContentTileCompositeGeometry,
    PreparedScrollTransformContentCompositeGeometry,
};
#[cfg(test)]
pub(crate) use scroll_scene::{
    NestedMediaLeafKind, NestedTextFallbackKind, build_scroll_scene_from_pool_with_budget_for_test,
    nested_scroll_unready_media_fixture_for_test, nested_scroll_unready_text_fixture_for_test,
    prepare_native_scroll_forest_transaction_with_forced_pool_for_test,
    retained_auto_scroll_content_effect_fixture,
};
#[allow(unused_imports)]
pub(crate) use scroll_scene::{
    PreparedFrameRootScrollScene, PreparedPropertyBoundaryDagScene,
    PreparedRetainedPropertyScrollForest, PropertyBoundaryDagCompiler, PropertyScrollScenePlan,
    PropertyScrollScenePlanError, RetainedPropertyScrollGroupSignature,
    RetainedPropertyScrollResidentGroup, RetainedPropertyScrollSceneBuildOutcome,
    RetainedPropertyScrollSceneBuildTrace, RetainedPropertyScrollSceneEmptyReplacement,
    RetainedPropertyScrollScenePrepareError, RetainedPropertyScrollSceneTransaction,
    ScrollSceneBackingKind, ScrollSceneBuildOutcome, ScrollSceneBuildTrace,
    ScrollSceneFromLiveError, ScrollSceneSingleTextureBudget,
    ValidatedDirectScrollTransformTransaction, ValidatedEffectScrollSceneCheckpoint,
    ValidatedEffectTransformScrollScene, ValidatedFrameRootScrollScene,
    ValidatedPropertyBoundaryDagScene, ValidatedPropertyScrollScene,
    ValidatedTransformEffectScrollScene, ValidatedTransformScrollScene,
    build_scroll_scene_from_pool, emit_prepared_direct_scroll_transform_scene,
    emit_prepared_frame_root_scroll_scene, emit_prepared_native_scroll_forest_transaction,
    emit_prepared_property_boundary_dag_scene, emit_prepared_retained_effect_scroll_scene,
    emit_prepared_retained_effect_transform_scroll_scene,
    emit_prepared_retained_property_scroll_forest,
    emit_prepared_retained_transform_effect_scroll_scene,
    emit_prepared_retained_transform_scroll_scene, plan_and_validate_direct_scroll_transform_scene,
    plan_and_validate_effect_scroll_scene_checkpoint, plan_and_validate_frame_root_scroll_scene,
    plan_and_validate_property_scroll_scene, plan_and_validate_transform_effect_scroll_scene,
    plan_and_validate_transform_scroll_scene, plan_property_scroll_scene_scaffold,
    prepare_direct_scroll_transform_scene_from_pool, prepare_frame_root_scroll_scene,
    prepare_native_scroll_forest_transaction_from_pool,
    prepare_property_boundary_dag_scene_from_pool, prepare_retained_effect_scroll_scene_from_pool,
    prepare_retained_effect_transform_scroll_scene_from_pool,
    prepare_retained_property_scroll_forest_from_pool,
    prepare_retained_transform_effect_scroll_scene_from_pool,
    prepare_retained_transform_scroll_scene_from_pool, production_single_texture_budget,
};
#[allow(unused_imports)]
pub(crate) use scroll_tiles::{
    ScrollContentActiveTileManifest, ScrollContentTileBounds, ScrollContentTileIndex,
    ScrollContentTileRasterIdentity, ScrollContentTileSetTransactionStamp,
    plan_active_scroll_content_tiles_dpr1,
};

#[cfg(test)]
pub(crate) mod tests;
