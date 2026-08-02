//! Authority-level baseline for nested scroll.
//!
//! The planner-level counterpart lives in
//! `paint::frame_plan::tests::property_scroll_interleave_tests`. This half
//! pins what `RetainedAuto` does with the same shape: which candidate reports
//! it, and that the frame ends up whole-frame Legacy.
//!
//! See `docs/design/nested-scroll-property-interleave.md`.

use super::*;
use crate::view::debug::census::fallback_detail_label;
use crate::view::paint::{FrameArtifactFallbackReason, FramePaintPlanRejection, LegacyPaintReason};

fn nested_scroll_scene() -> (
    NodeArena,
    Vec<NodeKey>,
    NodeKey,
    PropertyTrees,
    PaintGenerationTracker,
) {
    let (arena, root, inner, properties, generations) =
        crate::view::paint::nested_scroll_fixture_for_test();
    (arena, vec![root], inner, properties, generations)
}

#[test]
fn nested_scroll_falls_back_to_whole_frame_legacy() {
    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let (arena, roots, _inner, properties, generations) = nested_scroll_scene();

    let decision =
        select_retained_auto_authority(&arena, &roots, &properties, &generations, &ctx, true);

    let AutoAuthorityDecision::Legacy { trace } = decision else {
        panic!("nested scroll must not select a retained authority today")
    };
    assert!(
        !trace.rejections.is_empty(),
        "the fallback is explained by candidate rejections"
    );
}

#[test]
fn the_property_boundary_dag_candidate_reaches_artifact_preflight() {
    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let (arena, roots, inner, properties, generations) = nested_scroll_scene();

    let AutoAuthorityDecision::Legacy { trace } =
        select_retained_auto_authority(&arena, &roots, &properties, &generations, &ctx, true)
    else {
        panic!("nested scroll must not select a retained authority today")
    };

    let dag = trace
        .rejections
        .iter()
        .find_map(|rejection| match rejection {
            AutoAuthorityRejection::PropertyBoundaryDagPlan { error } => Some(error),
            _ => None,
        })
        .expect("the property boundary DAG candidate is the general path and must report");

    let crate::view::paint::PropertyScrollScenePlanError::Frame(frame) = dag else {
        panic!("M4 should pass DAG grammar classification: {dag:?}")
    };
    assert!(
        frame.reasons.iter().any(|reason| matches!(
            reason,
            FramePaintPlanRejection::Coverage(FrameArtifactFallbackReason::LegacyBoundary(
                LegacyPaintReason::MissingPreparedInlineRoot
            ))
        )),
        "the lightweight M0 fixture now stops at artifact readiness, not nested grammar: {frame:?}"
    );
    assert_ne!(inner, roots[0], "fixture still contains the nested host");
}

#[test]
fn the_reported_codes_are_the_ones_a_census_would_show() {
    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let (arena, roots, _inner, properties, generations) = nested_scroll_scene();

    let AutoAuthorityDecision::Legacy { trace } =
        select_retained_auto_authority(&arena, &roots, &properties, &generations, &ctx, true)
    else {
        panic!("nested scroll must not select a retained authority today")
    };

    let mut codes = trace
        .rejections
        .iter()
        .flat_map(|rejection| {
            super::super::selection_rejection_debug_records(
                &super::super::PaintAuthoritySelectionRejection::Auto(rejection.clone()),
            )
        })
        .map(|record| fallback_detail_label(&record.detail))
        .collect::<Vec<_>>();
    codes.sort();
    codes.dedup();

    assert!(
        codes.contains(&"missing-inline-root".to_string()),
        "M4 reaches artifact preflight for the lightweight M0 fixture: {codes:?}"
    );
    assert!(
        !codes.contains(&"property-boundary-dag:property-boundary-dag-plan".to_string()),
        "the nested DAG compiler grammar must no longer be the rejection: {codes:?}"
    );
    assert!(
        codes.iter().all(|code| !matches!(
            code.as_str(),
            "property-boundary-dag:scroll-boundary"
                | "property-boundary-dag:invalid-scroll-host"
                | "property-boundary-dag:ancestor-boundary-not-consumed"
                | "property-boundary-dag:receiver-ancestor-boundary-not-consumed"
                | "property-boundary-dag:receiver-state-cursor-mismatch"
        )),
        "M0 planner consequences must disappear once M3 seals the typed DAG: {codes:?}"
    );
}
