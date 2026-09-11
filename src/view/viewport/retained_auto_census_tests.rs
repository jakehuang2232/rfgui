//! Viewport-level wiring for [`crate::view::debug::census`].
//!
//! The census aggregation itself is covered in `view::debug::census::tests`;
//! these cases cover the path from viewport debug options through
//! [`Viewport::capture_retained_auto_census`].

use super::Viewport;
use crate::view::debug::census::UNATTRIBUTED_ELEMENT_TYPE;
use crate::view::debug::{
    DebugFallbackCategory, DebugFallbackDetail, DebugFallbackStage, DebugFrameDisposition,
    DebugFramePaintAuthority, DebugPaintRequestedMode, DebugRetainedAutoCaptureInput,
    DebugRetainedAutoFallbackCaptureInput, DebugRetainedAutoFrameCaptureInput,
    DebugRetainedAutoStatistics,
};

fn boundary(reason: &'static str) -> DebugRetainedAutoFallbackCaptureInput {
    DebugRetainedAutoFallbackCaptureInput {
        stage: DebugFallbackStage::Selection,
        category: DebugFallbackCategory::UnsupportedHost,
        detail: DebugFallbackDetail::Boundary { reason },
        owner: None,
        stable_id: None,
        element_type: None,
        bounds: None,
    }
}

fn attempt(
    selected_authority: DebugFramePaintAuthority,
    disposition: DebugFrameDisposition,
    fallback_stages: Vec<DebugRetainedAutoFallbackCaptureInput>,
) -> DebugRetainedAutoCaptureInput {
    DebugRetainedAutoCaptureInput {
        frame: DebugRetainedAutoFrameCaptureInput {
            attempt_id: 4,
            requested_mode: DebugPaintRequestedMode::RetainedAuto,
            selected_authority,
            disposition,
            fallback_stages,
            statistics: DebugRetainedAutoStatistics::default(),
        },
        nodes: Vec::new(),
        surfaces: Vec::new(),
    }
}

#[test]
fn census_is_none_until_an_authority_attempt_is_captured() {
    let viewport = Viewport::new();

    assert!(viewport.capture_retained_auto_census().is_none());
}

#[test]
fn census_aggregates_the_last_captured_attempt() {
    let mut viewport = Viewport::new();
    viewport.frame.last_retained_auto_debug = Some(attempt(
        DebugFramePaintAuthority::Legacy,
        DebugFrameDisposition::FellBackToLegacy,
        vec![
            boundary("child-clip"),
            boundary("child-clip"),
            boundary("transform"),
        ],
    ));

    let census = viewport
        .capture_retained_auto_census()
        .expect("captured attempt should produce a census");

    assert_eq!(census.attempt_id, 4);
    assert!(!census.is_retained_success());
    assert_eq!(census.total_fallbacks(), 3);
    assert_eq!(census.entries.len(), 2);
    assert_eq!(census.entries[0].count, 2);
    assert_eq!(
        census.total_for_element_type(UNATTRIBUTED_ELEMENT_TYPE),
        3,
        "boundaries whose owner identity did not resolve stay counted"
    );
}

#[test]
fn census_reports_retained_success_without_inspecting_candidate_rejections() {
    // Contract 1.2: earlier candidate rejections may coexist with a retained
    // final authority, so authority plus disposition decide success.
    let mut viewport = Viewport::new();
    viewport.frame.last_retained_auto_debug = Some(attempt(
        DebugFramePaintAuthority::Artifact,
        DebugFrameDisposition::Presented,
        vec![boundary("transform")],
    ));

    let census = viewport
        .capture_retained_auto_census()
        .expect("captured attempt should produce a census");

    assert!(census.is_retained_success());
    assert_eq!(census.total_fallbacks(), 1);
}

#[test]
fn census_capture_flag_does_not_enable_the_debug_overlay() {
    // Debug capture is observational. The census-only flag exists so a scene
    // can be censused without the overlay painting over it.
    let mut viewport = Viewport::new();
    let mut options = viewport.debug_options();
    options.retained_auto_census = true;
    viewport.set_debug_options(options);

    assert!(!viewport.debug_overlay_enabled());
    assert!(viewport.debug_options().retained_auto_census);
}

#[test]
fn a_planner_named_rejection_is_attributed_instead_of_the_whole_frame_record() {
    // Several ladder paths return Legacy without ever reaching the artifact
    // candidate, and only artifact rejections populate
    // `legacy_debug_boundaries`. Before plan rejections were mapped, a
    // scroll-heavy scene censused as one unattributed
    // `whole-frame-legacy-fallback` that named no component.
    let mut viewport = Viewport::new();
    viewport.frame.last_retained_auto_debug = Some(attempt(
        DebugFramePaintAuthority::Legacy,
        DebugFrameDisposition::FellBackToLegacy,
        vec![DebugRetainedAutoFallbackCaptureInput {
            stage: DebugFallbackStage::Planning,
            category: DebugFallbackCategory::PropertyTopology,
            detail: DebugFallbackDetail::Code {
                code: "scroll-boundary",
            },
            owner: None,
            stable_id: None,
            element_type: None,
            bounds: None,
        }],
    ));

    let census = viewport
        .capture_retained_auto_census()
        .expect("captured attempt should produce a census");

    assert_eq!(census.entries.len(), 1);
    assert_eq!(
        crate::view::debug::census::fallback_detail_label(&census.entries[0].detail),
        "scroll-boundary",
    );
}
