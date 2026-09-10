use super::*;

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_single_viewport_execute_failure_legacy_recovery_and_explicit_retry() -> Result<(), String>
{
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native hardware graphics context");
    for dpr in [1_u32, 2] {
        let mut viewport = Viewport::new();
        viewport.set_paint_renderer_mode(ViewportPaintRendererMode::RetainedAuto);
        let (root, mut style) = install_translucent_scene(&mut viewport);
        style.set_transform(Transform::new([Translate::xy(
            Length::px(9.0),
            Length::px(4.0),
        )]));
        get_element_mut::<Element>(viewport.node_arena(), root).apply_style(style);
        let mut target = None;
        // Ordered sequence: cold, warm, execute failure, two Legacy frames,
        // explicit circuit reset, warm retry. Scene content never changes.
        for frame in 0..7 {
            if frame == 5 {
                viewport.set_paint_renderer_mode(ViewportPaintRendererMode::RetainedAuto);
            }
            viewport.begin_offscreen_test_frame(
                gpu.device.clone(),
                gpu.queue.clone(),
                WIDTH * dpr,
                HEIGHT * dpr,
                FORMAT,
            )?;
            // Frame acquisition resets DPR, so restore it before layout.
            viewport.set_scale_factor(dpr as f32);
            assert_eq!(viewport.scale_factor(), dpr as f32);
            assert_eq!(viewport.logical_size(), (WIDTH as f32, HEIGHT as f32));
            if frame == 2 {
                let fault = crate::view::frame_graph::execution_failure_test_support::arm();
                viewport.render_single_viewport_execution_failure_for_test()?;
                assert!(fault.fired(), "must fail after a real execution step");
                let (key, desc) = target.as_ref().expect("cold target");
                assert!(!viewport.has_compatible_persistent_render_target_pair(*key, desc));
                continue;
            }
            let observed = if frame == 3 || frame == 4 {
                viewport.render_single_viewport_legacy_recovery_for_test()?
            } else {
                viewport.render_single_viewport_scene_for_test()?
            };
            let pixels =
                read_submitted_texture(&observed.texture, gpu, [WIDTH * dpr, HEIGHT * dpr])?;
            validate_style_pixels(
                &pixels,
                StyleScene::TranslucentFill,
                [9.0, 4.0],
                0.0,
                dpr,
                frame,
            )?;
            // The requested mode remains Auto while its latched breaker selects
            // Legacy. A successful Legacy frame must not automatically retry.
            assert_eq!(
                viewport.paint_renderer_mode(),
                ViewportPaintRendererMode::RetainedAuto
            );
            if frame == 3 || frame == 4 {
                assert!(observed.legacy_selected);
                continue;
            }
            assert!(observed.artifact_selected);
            assert_eq!(
                observed.actions,
                [if frame == 0 || frame == 5 {
                    RetainedSurfaceCompileAction::Reraster
                } else {
                    RetainedSurfaceCompileAction::Reuse
                }]
            );
            assert_eq!(observed.texture_bytes, 20 * 16 * 12 * u64::from(dpr * dpr));
            assert_eq!(observed.color_targets.len(), 1);
            let current = &observed.color_targets[0];
            assert!(viewport.has_compatible_persistent_render_target_pair(current.0, &current.1));
            if let Some(first) = &target {
                assert_eq!(
                    current, first,
                    "logical identity survives, physical backing need not"
                );
            } else {
                target = Some(current.clone());
            }
        }
    }
    eprintln!(
        "execution failure and explicit retry passed on {}",
        gpu.label()
    );
    Ok(())
}
