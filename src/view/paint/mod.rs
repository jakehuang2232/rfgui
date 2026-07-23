//! Retained paint recording scaffold.
//!
//! The first slice deliberately records only side-effect-free leaf Element
//! decoration. Every other host remains on the existing Renderable path.

mod artifact;
mod compiler;
mod coverage_manifest;
mod frame_plan;
mod frame_recorder;
mod recorder;
mod retained_surface_executor;
mod scroll_content;
mod scroll_scene;
mod scroll_tiles;

#[allow(unused_imports)]
pub(crate) use artifact::{
    ConsumedAncestorEffectWitness, ConsumedAncestorProperty, ConsumedAncestorPropertyStackWitness,
    ConsumedAncestorScrollContentsWitness, ConsumedAncestorTransformWitness,
    ConsumedSameOwnerEffectBoundaryWitness, ConsumedSameOwnerTransformBoundaryWitness, DrawRectOp,
    EffectPropertyContentWitness, EffectPropertySurfaceArtifactContract, PaintArtifact,
    PaintArtifactTarget, PaintBakedScrollHostWitness, PaintChunk, PaintChunkId, PaintChunkMetadata,
    PaintChunkRole, PaintContentRevision, PaintDeferredViewportEffectWitness,
    PaintDeferredViewportSelfClipWitness, PaintNestedScrollContentWitness, PaintNodePhase,
    PaintNodePlan, PaintOp, PaintOpacityAuthority, PaintOwnerSnapshot, PaintPayloadIdentity,
    PaintPropertyScope, PaintRecordingContext,
    PaintScrollAtomicProjectionSelectionTextAreaSubtreeWitness,
    PaintScrollAtomicProjectionTextAreaRecorderWitness,
    PaintScrollAtomicProjectionTextAreaSubtreeWitness, PaintScrollContentWitness,
    PaintScrollFocusedAtomicProjectionTextAreaSubtreeWitness, PaintScrollForestEdgeWitness,
    PaintScrollInteractiveTextAreaSubtreeWitness, PaintScrollTextAreaSubtreeWitness,
    PaintTextPreeditWitness, PaintTextSelectionWitness, PaintTransformSurfaceWitness,
    PreparedImageIdentity, PreparedImageOp, PreparedInlineIfcDecorationDescriptor,
    PreparedInlineIfcDecorationIdentity, PreparedInlineIfcDecorationOp,
    PreparedScrollbarOverlayIdentity, PreparedScrollbarOverlayOp, PreparedShadowIdentity,
    PreparedShadowOp, PreparedSvgIdentity, PreparedSvgOp, PreparedTextIdentity, PreparedTextOp,
    RETAINED_CHILD_MASK_SLOT, RecordedRetainedTextAreaCaretOverlay,
    RetainedAtomicProjectionTextAreaChunkRasterSeal, RetainedChildMaskPlan,
    RetainedTextAreaCaretOverlayIdentity, RetainedTextAreaCaretOverlayPaintIdentity,
    RetainedTextAreaGeneratedNodeKind, RetainedTextAreaGeneratedNodeSeal,
    RetainedTextAreaPreeditRasterSeal, has_canonical_paint_bounds, preedit_glyph_identity_is_exact,
    preedit_underline_identity_is_exact,
};
#[cfg(test)]
pub(crate) use compiler::validate_media_content_artifact_for_test;
#[allow(unused_imports)]
// C3 state/lifecycle landed before the C4 producer consumes every stamp type.
pub(crate) use compiler::{
    ArtifactCompileErrorKind, NestedSurfaceRasterDependency, PropertyEffectCompositeBasisStamp,
    PropertyEffectRasterIdentityInputs, RetainedAtomicProjectionTextAreaResidentRasterSeal,
    RetainedPropertySceneTransactionStamp, RetainedScrollHostRasterDependency,
    RetainedSurfaceArtifactSpanStamp, RetainedSurfaceChunkStamp, RetainedSurfaceCompileAction,
    RetainedSurfaceCompositeGeometryStamp, RetainedSurfaceRasterIdentity,
    RetainedSurfaceRasterInputs, RetainedSurfaceRasterRole, RetainedSurfaceRasterStamp,
    RetainedSurfaceRasterStepStamp, RetainedSurfaceResidentKey, RootEffectCompileAction,
    RootEffectRasterInputs, RootEffectRasterStamp, ValidatedEffectPropertySurfaceArtifact,
    emit_validated_effect_property_surface_artifact,
    property_effect_composite_geometry_stamp_is_canonical,
    property_effect_surface_raster_stamp_is_canonical_at_depth,
    property_effect_surface_raster_stamp_validates_contract_at_depth,
    retained_isolation_composite_geometry_stamp,
    retained_nested_isolation_composite_geometry_stamp,
    retained_property_effect_composite_geometry_stamp, retained_surface_composite_geometry_stamp,
    retained_surface_raster_stamp_is_canonical,
    retained_surface_raster_stamp_is_canonical_at_depth, try_compile_artifact,
    try_compile_root_effect_artifact, validate_effect_property_surface_artifact,
    validated_effect_property_surface_artifact_span_stamp,
    validated_isolation_surface_artifact_span_stamp,
    validated_property_effect_surface_raster_stamp, validated_retained_surface_artifact_span_stamp,
    validated_retained_surface_raster_stamp, validated_retained_surface_tree_raster_stamp,
    validated_root_effect_raster_stamp, validated_scroll_content_raster_stamp,
    validated_scroll_content_tile_raster_stamp, validated_scroll_host_artifact_span_stamp,
    validated_scroll_host_raster_stamp, validated_scroll_text_area_content_raster_stamp,
};
#[cfg(test)]
pub(crate) use compiler::{compile_artifact, take_artifact_compile_count};
#[allow(unused_imports)]
pub(crate) use coverage_manifest::{
    CoverageRecordingMode, PaintCoverageItem, PaintCoverageManifest, PaintCoverageStats,
    PaintCoverageValidationError, PlannedBoundary, PlannedBoundaryCutoutSet, PlannedBoundaryKind,
    record_coverage_manifest,
};
#[cfg(test)]
pub(crate) use frame_plan::tests::{native_scroll_forest_plan_fixture, nested_scroll_plan_fixture};
#[allow(unused_imports)]
pub(crate) use frame_plan::{
    ArtifactSpanPlan, FramePaintPlan, FramePaintPlanError, FramePaintPlanRejection, PaintPlanStep,
    RetainedSurfacePlan, SurfaceKind, TransformSurfacePlanContext,
    plan_native_scroll_forest_scaffold_with_context,
    plan_nested_scroll_scene_scaffold_with_context, plan_property_effect_scene_with_context,
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
    RendererMode, record_clip_enabled_frame_artifact, record_frame_artifact,
    record_property_neutral_frame_artifact, record_root_group_opacity_frame_artifact,
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
    ForcedTransformSurfaceError, build_retained_property_scene_with_forced_pool_for_test,
    execute_forced_transform_surface_for_test, prepare_forced_retained_surface_stamp_for_test,
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
    PreparedFrameRootScrollScene, PreparedNestedScrollReceiverGeometry,
    PreparedPropertyBoundaryDagScene, PreparedRetainedPropertyScrollForest,
    PropertyBoundaryDagCompiler, PropertyScrollScenePlan, PropertyScrollScenePlanError,
    RetainedPropertyScrollGroupSignature, RetainedPropertyScrollResidentGroup,
    RetainedPropertyScrollSceneBuildOutcome, RetainedPropertyScrollSceneBuildTrace,
    RetainedPropertyScrollScenePrepareError, RetainedPropertyScrollSceneTransaction,
    ScrollSceneBackingKind, ScrollSceneBuildOutcome, ScrollSceneBuildTrace,
    ScrollSceneFromLiveError, ScrollSceneSingleTextureBudget,
    ValidatedDirectScrollTransformTransaction, ValidatedEffectScrollSceneCheckpoint,
    ValidatedEffectTransformScrollScene, ValidatedFrameRootScrollScene,
    ValidatedPropertyBoundaryDagScene, ValidatedPropertyScrollScene,
    ValidatedTransformEffectScrollScene, ValidatedTransformScrollScene,
    build_scroll_scene_from_pool, emit_prepared_direct_scroll_transform_scene,
    emit_prepared_frame_root_scroll_scene, emit_prepared_native_scroll_forest_transaction,
    emit_prepared_nested_scroll_scene, emit_prepared_property_boundary_dag_scene,
    emit_prepared_retained_effect_scroll_scene,
    emit_prepared_retained_effect_transform_scroll_scene,
    emit_prepared_retained_property_scroll_forest,
    emit_prepared_retained_transform_effect_scroll_scene,
    emit_prepared_retained_transform_scroll_scene, plan_and_prepare_nested_scroll_scene,
    plan_and_validate_direct_scroll_transform_scene,
    plan_and_validate_effect_scroll_scene_checkpoint, plan_and_validate_frame_root_scroll_scene,
    plan_and_validate_property_scroll_scene, plan_and_validate_transform_effect_scroll_scene,
    plan_and_validate_transform_scroll_scene, plan_property_scroll_scene_scaffold,
    prepare_direct_scroll_transform_scene_from_pool, prepare_frame_root_scroll_scene,
    prepare_native_scroll_forest_transaction_from_pool, prepare_nested_scroll_scene_from_pool,
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
