//! Production selection performs one complete attempt, with capture-independent rejection.
use super::super::{RetainedAutoDecision, attempts};
use super::*;

#[derive(Clone, Copy, Debug)]
enum Case {
    Accepted,
    EarlyRecording,
    LatePlanning,
    Budget,
}

fn fixture(
    case: Case,
) -> (
    NodeArena,
    Vec<NodeKey>,
    PropertyTrees,
    PaintGenerationTracker,
    UiBuildContext,
    u64,
) {
    let (arena, roots, mut properties, generations) = if matches!(case, Case::EarlyRecording) {
        let (arena, roots) = prepared_safe_leaf();
        let bounds = arena.get(roots[0]).unwrap().element.box_model_snapshot();
        *arena.get_mut(roots[0]).unwrap().element = Box::new(UnknownOverlayHost {
            id: bounds.node_id,
            bounds,
        });
        let (properties, generations) = synced_paint_state(&arena, &roots);
        (arena, roots, properties, generations)
    } else {
        crate::view::paint::native_scroll_forest_plan_fixture()
    };
    let ctx = UiBuildContext::new(800, 700, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let mut budget = ARTIFACT_SURFACE_AGGREGATE_BUDGET_BYTES;
    match case {
        Case::Accepted | Case::EarlyRecording => {}
        // A zero generation survives metadata recording, but the Surface DAG
        // planner rejects it. This is deliberately a non-budget late failure.
        Case::LatePlanning => properties.scrolls.values_mut().next().unwrap().generation = 0,
        Case::Budget => budget = 1,
    }
    (arena, roots, properties, generations, ctx, budget)
}

fn select(
    f: &(
        NodeArena,
        Vec<NodeKey>,
        PropertyTrees,
        PaintGenerationTracker,
        UiBuildContext,
        u64,
    ),
    before: bool,
    capture: bool,
) -> AutoAuthorityDecision {
    let (arena, roots, properties, generations, ctx, budget) = f;
    let _ = before;
    super::super::select_retained_auto_frame(
        arena,
        roots,
        properties,
        generations,
        ctx,
        8192,
        *budget,
        capture,
        None,
        None,
    )
}

#[test]
fn retained_auto_rejects_once_with_capture_independent_stage() {
    for case in [
        Case::Accepted,
        Case::EarlyRecording,
        Case::LatePlanning,
        Case::Budget,
    ] {
        for capture in [false, true] {
            let f = fixture(case);
            for _ in 0..2 {
                // Match the production enum exhaustively: no compatibility payload
                // can be returned even if a historical planner would accept it.
                let (decision, counts) = attempts::observe(|| {
                    super::super::select_retained_auto_frame(
                        &f.0, &f.1, &f.2, &f.3, &f.4, 8192, f.5, capture, None, None,
                    )
                });
                let trace = match decision {
                    RetainedAutoDecision::Artifact { candidate, trace } => {
                        assert!(matches!(case, Case::Accepted));
                        assert!(matches!(
                            candidate.payload,
                            RecordedArtifactPayload::ArtifactSurface(_)
                        ));
                        assert_eq!(counts.get("resident-seal"), Some(&1));
                        trace
                    }
                    RetainedAutoDecision::Legacy { trace } => {
                        assert!(!matches!(case, Case::Accepted));
                        assert_eq!(
                            auto_artifact_legacy_fallback_stage(&trace),
                            if matches!(case, Case::EarlyRecording) {
                                PaintAuthorityFallbackStage::Selection
                            } else {
                                PaintAuthorityFallbackStage::Prepare
                            }
                        );
                        assert_eq!(trace.rejections.len(), usize::from(capture));
                        if capture {
                            match case {
                                Case::EarlyRecording => assert!(matches!(trace.rejections[0], AutoAuthorityRejection::Artifact { .. })),
                                Case::LatePlanning => assert!(matches!(trace.rejections[0], AutoAuthorityRejection::ArtifactPrepare { error }
                                    if !matches!(error, RecordedArtifactSurfacePrepareError::RasterPlan(crate::view::paint::ArtifactSurfaceRasterPlanError::TextureBudgetExceeded(_))))),
                                Case::Budget => assert!(matches!(trace.rejections[0], AutoAuthorityRejection::ArtifactPrepare { error: RecordedArtifactSurfacePrepareError::RasterPlan(crate::view::paint::ArtifactSurfaceRasterPlanError::TextureBudgetExceeded(_)) })),
                                Case::Accepted => unreachable!(),
                            }
                        }
                        trace
                    }
                };
                if !capture {
                    assert!(trace.rejections.is_empty());
                }
                assert_eq!(counts.get("generic-record"), Some(&1));
                assert_eq!(
                    counts.get("raster-plan").copied().unwrap_or(0),
                    usize::from(!matches!(case, Case::EarlyRecording))
                );
                assert_eq!(
                    counts.get("resident-seal").copied().unwrap_or(0),
                    usize::from(matches!(case, Case::Accepted))
                );
                assert!(!counts.contains_key("compatibility-cascade"));
                assert!(!counts.contains_key("compatibility-record"));
                assert!(
                    counts
                        .keys()
                        .all(|kind| ["generic-record", "raster-plan", "resident-seal"]
                            .contains(kind))
                );
            }
        }
    }
}

#[test]
fn retained_auto_incomplete_recording_cannot_gain_compatibility_authority() {
    let (arena, roots, properties, generations) = prepared_scroll_text_area_scene();
    let wrapper = arena.children_of(roots[0])[0];
    let text_area = arena.children_of(wrapper)[0];
    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    // This lightweight historical fixture is intentionally not upgraded here.
    // Its old planner accepted a TextArea owner the complete recorder cannot certify.
    // Keep both facts visible instead of disguising a selection change as a rename.
    // The former planner accepted this incomplete fixture. That historical
    // result is documented in c535e87; the removed planner is not an oracle.
    for capture in [false, true] {
        let (after, counts) = attempts::observe(|| {
            super::super::select_retained_auto_frame(
                &arena,
                &roots,
                &properties,
                &generations,
                &ctx,
                8192,
                ARTIFACT_SURFACE_AGGREGATE_BUDGET_BYTES,
                capture,
                None,
                None,
            )
        });
        let RetainedAutoDecision::Legacy { trace } = after else {
            panic!("complete recording rejection must select whole-frame Legacy");
        };
        assert_eq!(
            auto_artifact_legacy_fallback_stage(&trace),
            PaintAuthorityFallbackStage::Selection
        );
        if capture {
            assert!(
                matches!(trace.rejections.as_slice(), [AutoAuthorityRejection::Artifact { eligibility }]
                if eligibility.reasons.contains(&crate::view::paint::FrameArtifactFallbackReason::PropertyBoundary(text_area)))
            );
        } else {
            assert!(trace.rejections.is_empty());
        }
        assert_eq!(
            counts.into_iter().collect::<Vec<_>>(),
            vec![("generic-record", 1)]
        );
    }
}
