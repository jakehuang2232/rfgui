use super::*;
use crate::view::debug::{DebugCaptureOptions, DebugSurfaceAction, DebugSurfaceKind};

fn surfaces(viewport: &Viewport) -> Vec<(DebugSurfaceKind, DebugSurfaceAction, (u32, u32))> {
    viewport
        .capture_debug(DebugCaptureOptions {
            include_retained_auto: true,
            ..Default::default()
        })
        .document()
        .viewport
        .retained_auto
        .as_ref()
        .expect("the frame recorded its surfaces")
        .surfaces
        .iter()
        .map(|surface| (surface.kind, surface.action, surface.size))
        .collect()
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_single_viewport_debug_capture_reports_each_surface_action() -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native hardware graphics context");
    let dpr = 2;
    let mut viewport = Viewport::new();
    let (root, mut style) = install_translucent_scene(&mut viewport);
    style.set_transform(Transform::new([Translate::xy(
        Length::px(9.0),
        Length::px(4.0),
    )]));
    get_element_mut::<Element>(viewport.node_arena(), root).apply_style(style);
    let mut options = viewport.debug_options();
    options.retained_auto_overlay = true;
    viewport.set_debug_options(options);
    for action in [DebugSurfaceAction::Reraster, DebugSurfaceAction::Reuse] {
        viewport.begin_offscreen_test_frame(
            gpu.device.clone(),
            gpu.queue.clone(),
            WIDTH * dpr,
            HEIGHT * dpr,
            FORMAT,
        )?;
        viewport.set_scale_factor(dpr as f32);
        viewport.render_single_viewport_scene_for_test()?;
        assert_eq!(
            surfaces(&viewport),
            [(DebugSurfaceKind::Effect, action, (20 * dpr, 16 * dpr))]
        );
    }

    // With the overlay off, an F9 report still records the next frame.
    options.retained_auto_overlay = false;
    viewport.set_debug_options(options);
    viewport.request_retained_auto_report();
    viewport.begin_offscreen_test_frame(
        gpu.device.clone(),
        gpu.queue.clone(),
        WIDTH * dpr,
        HEIGHT * dpr,
        FORMAT,
    )?;
    viewport.set_scale_factor(dpr as f32);
    viewport.render_single_viewport_scene_for_test()?;
    assert_eq!(
        surfaces(&viewport),
        [(
            DebugSurfaceKind::Effect,
            DebugSurfaceAction::Reuse,
            (20 * dpr, 16 * dpr)
        )]
    );
    assert!(!viewport.surface_diagnostics_requested());
    Ok(())
}
