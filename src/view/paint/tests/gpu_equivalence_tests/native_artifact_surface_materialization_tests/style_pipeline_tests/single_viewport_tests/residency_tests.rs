use super::*;

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_single_viewport_missing_backing_rerasterizes_then_reuses() -> Result<(), String> {
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
        let mut stable_target = None;
        // Ordered frames: cold, warm, missing backing, recovered warm.
        // No style/layout dirty injection or retained identity invalidation is
        // performed between frames. Only the actual pool pair is released.
        for (frame, expected_action) in [
            RetainedSurfaceCompileAction::Reraster,
            RetainedSurfaceCompileAction::Reuse,
            RetainedSurfaceCompileAction::Reraster,
            RetainedSurfaceCompileAction::Reuse,
        ]
        .into_iter()
        .enumerate()
        {
            let allocations_before = viewport.offscreen_pool_texture_creation_count_for_test();
            viewport.begin_offscreen_test_frame(
                gpu.device.clone(),
                gpu.queue.clone(),
                WIDTH * dpr,
                HEIGHT * dpr,
                FORMAT,
            )?;
            // Acquisition resets DPR each frame; restore it before layout.
            viewport.set_scale_factor(dpr as f32);
            assert_eq!(viewport.scale_factor(), dpr as f32);
            assert_eq!(viewport.logical_size(), (WIDTH as f32, HEIGHT as f32));
            let observed = viewport.render_single_viewport_scene_for_test()?;
            let pixels =
                read_submitted_texture(&observed.texture, gpu, [WIDTH * dpr, HEIGHT * dpr])?;
            // Missing backing must never turn a plausible Reuse decision into
            // transparent output. Check independent pixels before accounting.
            validate_style_pixels(
                &pixels,
                StyleScene::TranslucentFill,
                [9.0, 4.0],
                0.0,
                dpr,
                frame,
            )?;
            assert!(observed.artifact_selected);
            if expected_action == RetainedSurfaceCompileAction::Reuse {
                assert_eq!(
                    viewport.offscreen_pool_texture_creation_count_for_test(),
                    allocations_before,
                    "warm composite must not allocate color or raster depth"
                );
            }
            assert_eq!(observed.frame_number, frame as u64 + 1);
            assert_eq!(
                observed.actions,
                [expected_action],
                "DPR {dpr} frame {frame}"
            );
            assert_eq!(observed.texture_bytes, 20 * 16 * 4 * u64::from(dpr * dpr));
            assert_eq!(observed.color_targets.len(), 1);
            let target = &observed.color_targets[0];
            assert_eq!((target.1.width(), target.1.height()), (20 * dpr, 16 * dpr));
            assert!(viewport.has_compatible_persistent_render_target(target.0, &target.1));
            let (_, depth) = crate::view::base_component::persistent_target_texture_descriptors(
                target.1.clone(),
                target.0,
            );
            assert!(
                !viewport.has_compatible_persistent_render_target(
                    target.0.depth_stencil().unwrap(),
                    &depth
                ),
                "raster depth is transient; warm Reuse must depend only on valid color pixels"
            );
            if let Some(first) = &stable_target {
                // Stable logical identity is intentional across reallocation;
                // it is not evidence that the physical GPU texture survived.
                assert_eq!(target, first);
            } else {
                stable_target = Some(target.clone());
            }
            if frame == 1 {
                // Exercise genuine backing loss via the pool API, not a fake
                // residency witness. This does not test pressure-policy choice.
                assert!(viewport.release_persistent_render_target_pair(target.0));
                assert!(!viewport.has_compatible_persistent_render_target(target.0, &target.1));
                assert!(!viewport.release_persistent_render_target_pair(target.0));
            }
        }
    }
    eprintln!("single Viewport backing recovery passed on {}", gpu.label());
    Ok(())
}
