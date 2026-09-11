//! Retirement tripwire, not rendering evidence. The historical denominator at
//! 3c4a86a was 46,828 lines: scroll_scene 25,069, frame_plan 15,267,
//! legacy_recording 4,986, legacy_admission 1,506. One 44-line shared layout
//! witness validator survives in frame_recorder; moving it is not deletion.
//! Behavioral acceptance runs the production artifact path and Legacy against
//! independent geometry/pixels, including the portable scroll forest round trip.
use super::*;

#[test]
fn production_renderer_modes_and_authorities_have_two_explicit_outcomes() {
    fn mode(mode: ViewportPaintRendererMode) -> &'static str {
        match mode {
            ViewportPaintRendererMode::Legacy => "legacy",
            ViewportPaintRendererMode::RetainedAuto => "retained-auto",
        }
    }
    fn authority(decision: &super::super::RetainedAutoDecision) -> &'static str {
        match decision {
            super::super::RetainedAutoDecision::Artifact { .. } => "artifact",
            super::super::RetainedAutoDecision::Legacy { .. } => "legacy",
        }
    }
    assert_eq!(mode(ViewportPaintRendererMode::Legacy), "legacy");
    assert_eq!(
        mode(ViewportPaintRendererMode::RetainedAuto),
        "retained-auto"
    );
    let (arena, roots) = prepared_safe_leaf();
    let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    assert_eq!(authority(&auto_decision(&arena, &roots, &ctx)), "artifact");
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn retired_planner_files_and_compatibility_selector_are_physically_absent() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    for path in [
        "src/view/paint/scroll_scene.rs",
        "src/view/paint/frame_plan.rs",
        "src/view/paint/legacy_recording.rs",
        "src/view/paint/legacy_admission.rs",
        "src/view/paint/retained_surface_executor.rs",
        "src/view/paint/scroll_content.rs",
        "src/view/paint/scroll_tiles.rs",
        "src/view/viewport/render/compatibility_reference.rs",
    ] {
        assert!(
            !root.join(path).exists(),
            "retired implementation reappeared: {path}"
        );
    }
}
