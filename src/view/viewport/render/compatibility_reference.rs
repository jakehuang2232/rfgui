//! Historical selector frozen from a500341 for regression scenes and paired cost measurements.
//! This module is test-only. Production RetainedAuto cannot return these planner payloads.
//! Use this selector as a reference for historical selection decisions only.
//! Never use its planner outputs as pixel or geometry oracles; rendering
//! expectations must be derived independently of these retired paths.
use super::*;
mod helpers;
use helpers::*;
pub(super) use helpers::{
    native_scroll_forest_topology_is_branching_or_multi_root, retained_auto_reachable_tree_facts,
};

/// Historical owning decision retained for planner/pool regression tests.
/// The production decision is RetainedAutoDecision, which has only two outcomes.
pub(super) enum CompatibilityAuthorityDecision {
    NativeScrollForest {
        plan: crate::view::paint::FramePaintPlan,
        trace: AutoAuthorityTrace,
    },
    PropertyBoundaryDagScene {
        scene: crate::view::paint::ValidatedPropertyBoundaryDagScene,
        trace: AutoAuthorityTrace,
    },
    DirectScrollTransformScene {
        scene: crate::view::paint::ValidatedDirectScrollTransformTransaction,
        trace: AutoAuthorityTrace,
    },
    PropertyScrollScene {
        scene: crate::view::paint::ValidatedPropertyScrollScene,
        trace: AutoAuthorityTrace,
    },
    FrameRootScrollScene {
        scene: crate::view::paint::ValidatedFrameRootScrollScene,
        trace: AutoAuthorityTrace,
    },
    TransformScrollScene {
        scene: crate::view::paint::ValidatedTransformScrollScene,
        trace: AutoAuthorityTrace,
    },
    EffectScrollScene {
        scene: crate::view::paint::ValidatedEffectScrollSceneCheckpoint,
        trace: AutoAuthorityTrace,
    },
    TransformEffectScrollScene {
        scene: crate::view::paint::ValidatedTransformEffectScrollScene,
        trace: AutoAuthorityTrace,
    },
    PropertyScene {
        plan: crate::view::paint::FramePaintPlan,
        trace: AutoAuthorityTrace,
    },
    Artifact {
        candidate: RecordedArtifactCandidate,
        trace: AutoAuthorityTrace,
    },
    Legacy {
        trace: AutoAuthorityTrace,
    },
}

pub(super) fn select_before_convergence(
    arena: &crate::view::node_arena::NodeArena,
    roots: &[crate::view::node_arena::NodeKey],
    property_trees: &crate::view::compositor::PropertyTrees,
    paint_generations: &crate::view::compositor::PaintGenerationTracker,
    ctx: &crate::view::base_component::UiBuildContext,
    semantic_frame_time: crate::time::Instant,
    scroll_budget: crate::view::paint::ScrollSceneSingleTextureBudget,
    artifact_surface_max_texture_dimension_2d: u32,
    artifact_surface_max_texture_bytes: u64,
    capture_trace: bool,
) -> CompatibilityAuthorityDecision {
    let mut trace = AutoAuthorityTrace::new(capture_trace);
    // Complete recording and the common plan/seal decide whether this frame
    // can execute. Property counts, host families and topology do not gate
    // this attempt. The baseline deliberately retries the historical cascade.
    match record_auto_detached_surface_candidate(
        arena,
        roots,
        property_trees,
        paint_generations,
        artifact_surface_raster_context(
            ctx,
            artifact_surface_max_texture_dimension_2d,
            artifact_surface_max_texture_bytes,
        ),
        RecordedArtifactSurfaceRequirement::General,
    ) {
        Ok(candidate) => return CompatibilityAuthorityDecision::Artifact { candidate, trace },
        Err(RecordedArtifactCandidateRejection::Eligibility(eligibility)) => {
            trace.capture(|| AutoAuthorityRejection::Artifact { eligibility });
        }
        Err(RecordedArtifactCandidateRejection::Prepare(error)) => {
            // Decide from the typed error, never from optional debug telemetry.
            let budget_rejected = matches!(
                error,
                RecordedArtifactSurfacePrepareError::RasterPlan(
                    crate::view::paint::ArtifactSurfaceRasterPlanError::TextureBudgetExceeded(_)
                )
            );
            trace.capture(|| AutoAuthorityRejection::ArtifactPrepare { error });
            if budget_rejected {
                return CompatibilityAuthorityDecision::Legacy { trace };
            }
        }
    }
    select_retained_auto_compatibility_authority_with_semantics(
        arena,
        roots,
        property_trees,
        paint_generations,
        ctx,
        semantic_frame_time,
        scroll_budget,
        artifact_surface_max_texture_dimension_2d,
        artifact_surface_max_texture_bytes,
        trace,
    )
}

// Historical retained planners remain reachable only after the general
// attempt rejects. Keep this entry explicit so their existing mutation and
// atomicity tests do not pretend that a valid frame still selects a bridge.
pub(super) fn select_retained_auto_compatibility_authority_with_semantics(
    arena: &crate::view::node_arena::NodeArena,
    roots: &[crate::view::node_arena::NodeKey],
    property_trees: &crate::view::compositor::PropertyTrees,
    paint_generations: &crate::view::compositor::PaintGenerationTracker,
    ctx: &crate::view::base_component::UiBuildContext,
    semantic_frame_time: crate::time::Instant,
    scroll_budget: crate::view::paint::ScrollSceneSingleTextureBudget,
    artifact_surface_max_texture_dimension_2d: u32,
    artifact_surface_max_texture_bytes: u64,
    mut trace: AutoAuthorityTrace,
) -> CompatibilityAuthorityDecision {
    attempts::record("compatibility-cascade");
    let transforms = property_trees.transforms.len();
    let effects = property_trees.effects.len();
    let scrolls = property_trees.scrolls.len();
    let reachable_tree_facts = retained_auto_reachable_tree_facts(arena, roots);

    if scrolls != 0 || reachable_tree_facts.has_scroll_container {
        let viewport = ctx.viewport();
        // Compatibility-only admission: successful complete recordings have
        // already selected the generic executor. Preserve these historical
        // restrictions for the historical baseline and direct planner regressions.
        // General TextArea authority is exercised by the native single-
        // Viewport caret/selection/IME gates, including an outer scroll scope.
        let scroll_content_artifact_admitted = transforms == 0
            && effects == 0
            && !reachable_tree_facts.has_text_area_paint_family
            && !native_scroll_forest_topology_is_branching_or_multi_root(roots, property_trees);
        if scroll_content_artifact_admitted
            && let Some(candidate) = try_select_auto_detached_surface_candidate(
                arena,
                roots,
                property_trees,
                paint_generations,
                artifact_surface_raster_context(
                    ctx,
                    artifact_surface_max_texture_dimension_2d,
                    artifact_surface_max_texture_bytes,
                ),
                RecordedArtifactSurfaceRequirement::ScrollContentOnly,
                &mut trace,
            )
        {
            return CompatibilityAuthorityDecision::Artifact { candidate, trace };
        }
        attempts::record("plan_and_validate_frame_root_scroll_scene");
        match crate::view::paint::plan_and_validate_frame_root_scroll_scene(
            arena,
            roots,
            property_trees,
            paint_generations,
            viewport.scale_factor(),
            ctx.paint_offset(),
            ctx.graphics_pass_context().logical_scissor_rect(),
            viewport.target_format(),
        ) {
            Ok(scene) => {
                return CompatibilityAuthorityDecision::FrameRootScrollScene { scene, trace };
            }
            Err(error) => {
                trace.capture(|| AutoAuthorityRejection::FrameRootScrollPlan { error });
            }
        }
        attempts::record("plan_and_validate_property_scroll_scene");
        match crate::view::paint::plan_and_validate_property_scroll_scene(
            arena,
            roots,
            property_trees,
            paint_generations,
            viewport.scale_factor(),
            ctx.paint_offset(),
            ctx.graphics_pass_context().logical_scissor_rect(),
            semantic_frame_time,
            viewport.target_format(),
            scroll_budget,
        ) {
            Ok(scene) => {
                return CompatibilityAuthorityDecision::PropertyScrollScene { scene, trace };
            }
            Err(error) => {
                trace.capture(|| AutoAuthorityRejection::PropertyScrollPlan { error });
            }
        }
        attempts::record("plan_and_validate_transform_scroll_scene");
        match crate::view::paint::plan_and_validate_transform_scroll_scene(
            arena,
            roots,
            property_trees,
            paint_generations,
            viewport.scale_factor(),
            ctx.paint_offset(),
            ctx.graphics_pass_context().logical_scissor_rect(),
            semantic_frame_time,
            viewport.target_format(),
            scroll_budget,
        ) {
            Ok(scene) => {
                return CompatibilityAuthorityDecision::TransformScrollScene { scene, trace };
            }
            Err(error) => {
                trace.capture(|| AutoAuthorityRejection::TransformScrollPlan { error });
            }
        }
        attempts::record("plan_and_validate_effect_scroll_scene_checkpoint");
        match crate::view::paint::plan_and_validate_effect_scroll_scene_checkpoint(
            arena,
            roots,
            property_trees,
            paint_generations,
            viewport.scale_factor(),
            ctx.paint_offset(),
            ctx.graphics_pass_context().logical_scissor_rect(),
            semantic_frame_time,
            viewport.target_format(),
            scroll_budget,
        ) {
            Ok(scene) => return CompatibilityAuthorityDecision::EffectScrollScene { scene, trace },
            Err(error) => {
                trace.capture(|| AutoAuthorityRejection::EffectScrollPlan { error });
            }
        }
        attempts::record("plan_and_validate_transform_effect_scroll_scene");
        match crate::view::paint::plan_and_validate_transform_effect_scroll_scene(
            arena,
            roots,
            property_trees,
            paint_generations,
            viewport.scale_factor(),
            ctx.paint_offset(),
            ctx.graphics_pass_context().logical_scissor_rect(),
            semantic_frame_time,
            viewport.target_format(),
            scroll_budget,
        ) {
            Ok(scene) => {
                return CompatibilityAuthorityDecision::TransformEffectScrollScene { scene, trace };
            }
            Err(error) => {
                trace.capture(|| AutoAuthorityRejection::TransformEffectScrollPlan { error });
            }
        }
        let forest_topology = scrolls >= 2
            && native_scroll_forest_topology_is_branching_or_multi_root(roots, property_trees);
        if forest_topology {
            let plan_context = crate::view::paint::TransformSurfacePlanContext::new(
                ctx.paint_offset(),
                ctx.graphics_pass_context().logical_scissor_rect(),
            );
            attempts::record("plan_native_scroll_forest_scaffold_with_context");
            match crate::view::paint::plan_native_scroll_forest_scaffold_with_context(
                arena,
                roots,
                property_trees,
                paint_generations,
                viewport.scale_factor(),
                plan_context,
            ) {
                Ok(plan) => {
                    return CompatibilityAuthorityDecision::NativeScrollForest { plan, trace };
                }
                Err(error) => {
                    trace.capture(|| AutoAuthorityRejection::NativeScrollForestPlan { error });
                }
            }
        }
        attempts::record("plan_property_boundary_dag");
        let boundary_dag =
            PropertyBoundaryDagCompiler::plan_and_validate_after_fixed_grammar_cascade(
                arena,
                roots,
                property_trees,
                paint_generations,
                viewport.scale_factor(),
                ctx.paint_offset(),
                ctx.graphics_pass_context().logical_scissor_rect(),
                semantic_frame_time,
                viewport.target_format(),
                scroll_budget,
            );
        match boundary_dag {
            Ok(Some(scene)) => {
                return CompatibilityAuthorityDecision::PropertyBoundaryDagScene { scene, trace };
            }
            Ok(None) => {}
            Err(error) => {
                trace.capture(|| AutoAuthorityRejection::PropertyBoundaryDagPlan { error });
            }
        }
        if scrolls >= 2 && !forest_topology {
            trace.capture(|| AutoAuthorityRejection::NativeScrollForestPlan {
                error: crate::view::paint::FramePaintPlanError {
                    reasons: vec![
                        crate::view::paint::FramePaintPlanRejection::InvalidPropertyScene(
                            "native-scroll-forest-linear-chain",
                        ),
                    ],
                },
            });
        }
        attempts::record("plan_and_validate_direct_scroll_transform_scene");
        return match crate::view::paint::plan_and_validate_direct_scroll_transform_scene(
            arena,
            roots,
            property_trees,
            paint_generations,
            viewport.scale_factor(),
            ctx.paint_offset(),
            ctx.graphics_pass_context().logical_scissor_rect(),
            viewport.target_format(),
            scroll_budget,
        ) {
            Ok(scene) => {
                CompatibilityAuthorityDecision::DirectScrollTransformScene { scene, trace }
            }
            Err(error) => {
                trace.capture(|| AutoAuthorityRejection::DirectScrollTransformPlan { error });
                CompatibilityAuthorityDecision::Legacy { trace }
            }
        };
    }

    if effects != 0 {
        // Native hosts explicitly admitted by ElementTrait use the existing
        // host-generic root-opacity artifact grammar. The full tree/property
        // witness and metadata/full-artifact pair remain authoritative, so a
        // resource, topology, property, or generation drift still fails
        // closed before emission.
        if is_exact_native_root_opacity_artifact(arena, roots, property_trees) {
            match record_auto_artifact_candidate(
                arena,
                roots,
                property_trees,
                paint_generations,
                artifact_surface_raster_context(
                    ctx,
                    artifact_surface_max_texture_dimension_2d,
                    artifact_surface_max_texture_bytes,
                ),
            ) {
                Ok(candidate) => {
                    return CompatibilityAuthorityDecision::Artifact { candidate, trace };
                }
                Err(RecordedArtifactCandidateRejection::Eligibility(eligibility)) => {
                    trace.capture(|| AutoAuthorityRejection::Artifact { eligibility });
                }
                Err(RecordedArtifactCandidateRejection::Prepare(error)) => {
                    trace.capture(|| AutoAuthorityRejection::ArtifactPrepare { error });
                }
            }
        }
        if let Some(candidate) = try_select_auto_detached_surface_candidate(
            arena,
            roots,
            property_trees,
            paint_generations,
            artifact_surface_raster_context(
                ctx,
                artifact_surface_max_texture_dimension_2d,
                artifact_surface_max_texture_bytes,
            ),
            RecordedArtifactSurfaceRequirement::Detached,
            &mut trace,
        ) {
            return CompatibilityAuthorityDecision::Artifact { candidate, trace };
        }
        let plan_context = crate::view::paint::TransformSurfacePlanContext::new(
            ctx.paint_offset(),
            ctx.graphics_pass_context().logical_scissor_rect(),
        );
        attempts::record("plan_property_effect_scene_with_context");
        return match crate::view::paint::plan_property_effect_scene_with_context(
            arena,
            roots,
            property_trees,
            paint_generations,
            plan_context,
        ) {
            Ok(plan) => CompatibilityAuthorityDecision::PropertyScene { plan, trace },
            Err(error) => {
                trace.capture(|| AutoAuthorityRejection::Plan {
                    authority: AutoAuthorityKind::PropertyScene,
                    error,
                });
                CompatibilityAuthorityDecision::Legacy { trace }
            }
        };
    }

    if transforms != 0 && effects == 0 {
        if let Some(candidate) = try_select_auto_detached_surface_candidate(
            arena,
            roots,
            property_trees,
            paint_generations,
            artifact_surface_raster_context(
                ctx,
                artifact_surface_max_texture_dimension_2d,
                artifact_surface_max_texture_bytes,
            ),
            RecordedArtifactSurfaceRequirement::Detached,
            &mut trace,
        ) {
            return CompatibilityAuthorityDecision::Artifact { candidate, trace };
        }
        let plan_context = crate::view::paint::TransformSurfacePlanContext::new(
            ctx.paint_offset(),
            ctx.graphics_pass_context().logical_scissor_rect(),
        );
        attempts::record("plan_transform_property_scene_with_context");
        return match crate::view::paint::plan_transform_property_scene_with_context(
            arena,
            roots,
            property_trees,
            paint_generations,
            plan_context,
        ) {
            Ok(plan) => CompatibilityAuthorityDecision::PropertyScene { plan, trace },
            Err(error) => {
                trace.capture(|| AutoAuthorityRejection::Plan {
                    authority: AutoAuthorityKind::PropertyScene,
                    error,
                });
                CompatibilityAuthorityDecision::Legacy { trace }
            }
        };
    }

    match record_auto_artifact_candidate(
        arena,
        roots,
        property_trees,
        paint_generations,
        artifact_surface_raster_context(
            ctx,
            artifact_surface_max_texture_dimension_2d,
            artifact_surface_max_texture_bytes,
        ),
    ) {
        Ok(candidate) => CompatibilityAuthorityDecision::Artifact { candidate, trace },
        Err(RecordedArtifactCandidateRejection::Eligibility(eligibility)) => {
            trace.capture(|| AutoAuthorityRejection::Artifact { eligibility });
            CompatibilityAuthorityDecision::Legacy { trace }
        }
        Err(RecordedArtifactCandidateRejection::Prepare(error)) => {
            trace.capture(|| AutoAuthorityRejection::ArtifactPrepare { error });
            CompatibilityAuthorityDecision::Legacy { trace }
        }
    }
}

impl From<RetainedAutoDecision> for CompatibilityAuthorityDecision {
    fn from(decision: RetainedAutoDecision) -> Self {
        match decision {
            RetainedAutoDecision::Artifact { candidate, trace } => {
                Self::Artifact { candidate, trace }
            }
            RetainedAutoDecision::Legacy { trace } => Self::Legacy { trace },
        }
    }
}
