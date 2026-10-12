use super::*;

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_resize_preserves_sources_and_valid_residents() -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("hardware adapter");
    let mut viewport = Viewport::new();
    let scene = install_resource_scene(&mut viewport);
    let mut first_uploads = None;
    let mut first_target = None;
    // Grow, shrink, and return to the original size. The fixed-size image
    // and scroll content are unaffected by these viewport-only resizes.
    for [width, height] in [[67, 64], [91, 80], [40, 36], [67, 64]] {
        viewport.begin_offscreen_test_frame(
            gpu.device.clone(),
            gpu.queue.clone(),
            width,
            height,
            FORMAT,
        )?;
        viewport.finish_offscreen_reconfigure_for_test();
        let observed = viewport.render_single_viewport_scene_for_test()?;
        let (_, uploads, _) =
            viewport.sampled_cache_observation_for_test(SampledTextureId::Image(scene.asset));
        if let Some(first) = first_uploads {
            assert_eq!(uploads, first, "resize must retain the uploaded source");
        } else {
            assert!(uploads > 0);
            first_uploads = Some(uploads);
        }
        assert_eq!(
            observed.actions,
            [if first_target.is_none() {
                RetainedSurfaceCompileAction::Reraster
            } else {
                RetainedSurfaceCompileAction::Reuse
            }]
        );
        let target = observed.color_targets[0].clone();
        assert!(viewport.has_compatible_persistent_render_target(target.0, &target.1));
        if let Some(first) = &first_target {
            assert_eq!(&target, first);
        }
        first_target = Some(target);
        let pixels = read_submitted_texture(&observed.texture, gpu, [width, height])?;
        for y in 0..height {
            for x in 0..width {
                let at = ((y * width + x) * 4) as usize;
                let expected = if x < 20 && y < 16 {
                    [255, 0, 0, 255]
                } else {
                    [0, 0, 0, 0]
                };
                assert_eq!(&pixels[at..at + 4], &expected, "{width}x{height} ({x},{y})");
            }
        }
    }
    eprintln!(
        "resize source/resident reuse and pixels passed on {}",
        gpu.label()
    );
    Ok(())
}
