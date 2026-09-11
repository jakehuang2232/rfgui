use super::*;

#[test]
fn retained_auto_scroll_content_budget_overflow_is_a_typed_preparation_rejection() {
    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let (arena, roots, properties, generations) = prepared_exact_scroll_scene();
    for capture_trace in [false, true] {
        let decision = super::super::select_retained_auto_authority_with_artifact_budget_for_test(
            &arena,
            &roots,
            &properties,
            &generations,
            &ctx,
            1,
            capture_trace,
        );
        let AutoAuthorityDecision::Legacy { trace } = decision else {
            panic!("a rejected Artifact budget must not retry an older retained planner")
        };

        if !capture_trace {
            assert!(trace.rejections.is_empty());
            continue;
        }
        assert_eq!(
            trace.rejections.len(),
            1,
            "no compatibility attempts after budget rejection"
        );
        assert!(trace.rejections.iter().any(|rejection| matches!(
            rejection,
            AutoAuthorityRejection::ArtifactPrepare {
                error: RecordedArtifactSurfacePrepareError::RasterPlan(
                    crate::view::paint::ArtifactSurfaceRasterPlanError::TextureBudgetExceeded(_),
                ),
            }
        )));
    }
}
