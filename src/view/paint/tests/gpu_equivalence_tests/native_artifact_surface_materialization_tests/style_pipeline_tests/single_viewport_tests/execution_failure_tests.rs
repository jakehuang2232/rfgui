use super::*;

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_single_viewport_execute_failure_retries_next_frame() -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native hardware graphics context");
    for dpr in [1_u32, 2] {
        let mut viewport = Viewport::new();
        let (root, mut style) = install_translucent_scene(&mut viewport);
        style.set_transform(Transform::new([Translate::xy(
            Length::px(9.0),
            Length::px(4.0),
        )]));
        get_element_mut::<Element>(viewport.node_arena(), root).apply_style(style);
        let mut target = None;
        // Ordered sequence: cold, warm, execute failure, cold retry, warm.
        // Scene content never changes.
        for frame in 0..5 {
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
                assert!(!viewport.has_compatible_persistent_render_target(*key, desc));
                continue;
            }
            let observed = viewport.render_single_viewport_scene_for_test()?;
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
            assert_eq!(
                observed.actions,
                [if frame == 0 || frame == 3 {
                    RetainedSurfaceCompileAction::Reraster
                } else {
                    RetainedSurfaceCompileAction::Reuse
                }]
            );
            assert_eq!(observed.texture_bytes, 20 * 16 * 4 * u64::from(dpr * dpr));
            assert_eq!(observed.color_targets.len(), 1);
            let current = &observed.color_targets[0];
            assert!(viewport.has_compatible_persistent_render_target(current.0, &current.1));
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
    eprintln!("execution failure and retry passed on {}", gpu.label());
    Ok(())
}
