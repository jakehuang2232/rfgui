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
use crate::view::paint::FramePaintPlanRejection;

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
fn the_property_boundary_dag_candidate_reports_the_nested_host() {
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
        panic!("the DAG candidate delegates to the frame planner: {dag:?}")
    };
    assert!(
        frame.reasons.iter().any(|reason| matches!(
            reason,
            FramePaintPlanRejection::ScrollBoundary(owner) if *owner == inner
        )),
        "the nested host is named: {:?}",
        frame.reasons
    );
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

    for expected in [
        "property-boundary-dag:scroll-boundary",
        "property-boundary-dag:invalid-scroll-host",
        "property-boundary-dag:ancestor-boundary-not-consumed",
        "property-boundary-dag:receiver-ancestor-boundary-not-consumed",
        "property-boundary-dag:receiver-state-cursor-mismatch",
    ] {
        assert!(
            codes.contains(&expected.to_string()),
            "missing {expected} in {codes:?}"
        );
    }
}
