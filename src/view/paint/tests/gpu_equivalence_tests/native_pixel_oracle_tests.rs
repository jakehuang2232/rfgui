use super::*;

#[test]
#[ignore = "requires native GPU adapter"]
fn native_prepared_image_2x2_fit_sampling_alpha_and_arena_drop_match() -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native GPU initialized");
    let adapter = gpu.label();
    let pixels: Arc<[u8]> = Arc::from([
        255_u8, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 255, 255, 255, 0, 64,
    ]);
    for (fit, sampling, opacity, validate_anchors) in [
        (
            crate::view::ImageFit::Fill,
            crate::view::ImageSampling::Nearest,
            1.0,
            true,
        ),
        (
            crate::view::ImageFit::Contain,
            crate::view::ImageSampling::Linear,
            0.65,
            false,
        ),
        (
            crate::view::ImageFit::Cover,
            crate::view::ImageSampling::Nearest,
            0.4,
            false,
        ),
    ] {
        let legacy = render(
            legacy_image_graph(pixels.clone(), fit, sampling, opacity, false)?,
            &gpu,
        )?;
        let artifact = render(
            artifact_image_graph(pixels.clone(), fit, sampling, opacity, false)?,
            &gpu,
        )?;
        if validate_anchors {
            validate_nearest_fill_image_anchors(&legacy, "legacy", &adapter)?;
            validate_nearest_fill_image_anchors(&artifact, "artifact", &adapter)?;
        }
        compare_pixels(
            &legacy,
            &artifact,
            [0, 0, 47, 31],
            &adapter,
            &format!("prepared-image-{fit:?}-{sampling:?}-{opacity}"),
        )?;
    }

    let legacy = render(
        legacy_image_graph(
            pixels.clone(),
            crate::view::ImageFit::Fill,
            crate::view::ImageSampling::Linear,
            0.65,
            true,
        )?,
        &gpu,
    )?;
    let artifact = render(
        artifact_image_graph(
            pixels,
            crate::view::ImageFit::Fill,
            crate::view::ImageSampling::Linear,
            0.65,
            true,
        )?,
        &gpu,
    )?;
    compare_pixels(
        &legacy,
        &artifact,
        [14, 17, 40, 24],
        &adapter,
        "prepared-image-decorated-fill-linear-0.65",
    )?;
    eprintln!("native PreparedImage parity passed on {adapter}");
    Ok(())
}

#[test]
#[ignore = "requires native GPU adapter"]
fn native_prepared_image_semantics_have_independent_pixel_oracles() -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native GPU initialized");
    let pattern: Arc<[u8]> = Arc::from([
        255_u8, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 255, 255, 255, 0, 64,
    ]);

    let contain = render(
        artifact_image_graph(
            pattern.clone(),
            crate::view::ImageFit::Contain,
            crate::view::ImageSampling::Nearest,
            1.0,
            false,
        )?,
        &gpu,
    )?;
    assert_pixel_near(&contain, 2, 15, [0, 0, 0, 0], 0, "contain letterbox")?;
    for (x, y, expected, name) in [
        (12, 5, [255, 0, 0, 255], "contain top-left"),
        (35, 5, [0, 255, 0, 128], "contain top-right"),
        (12, 25, [0, 0, 255, 255], "contain bottom-left"),
        (35, 25, [255, 255, 0, 64], "contain bottom-right"),
    ] {
        assert_pixel_near(&contain, x, y, expected, 1, name)?;
    }

    let cover = render(
        artifact_image_graph(
            pattern.clone(),
            crate::view::ImageFit::Cover,
            crate::view::ImageSampling::Nearest,
            1.0,
            false,
        )?,
        &gpu,
    )?;
    for (x, y, expected, name) in [
        (5, 4, [255, 0, 0, 255], "cover cropped top-left"),
        (40, 4, [0, 255, 0, 128], "cover cropped top-right"),
        (5, 27, [0, 0, 255, 255], "cover cropped bottom-left"),
        (40, 27, [255, 255, 0, 64], "cover cropped bottom-right"),
    ] {
        assert_pixel_near(&cover, x, y, expected, 1, name)?;
    }

    let half_opacity = render(
        artifact_image_graph(
            Arc::from([
                255_u8, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255,
            ]),
            crate::view::ImageFit::Fill,
            crate::view::ImageSampling::Nearest,
            0.5,
            false,
        )?,
        &gpu,
    )?;
    assert_pixel_near(&half_opacity, 11, 10, [255, 0, 0, 128], 1, "opacity output")?;

    let linear = render(
        artifact_image_graph(
            pattern,
            crate::view::ImageFit::Fill,
            crate::view::ImageSampling::Linear,
            1.0,
            false,
        )?,
        &gpu,
    )?;
    assert_pixel_near(
        &linear,
        23,
        15,
        [128, 128, 64, 176],
        4,
        "linear four-texel interpolation",
    )?;

    let decorated = render(
        artifact_image_graph(
            Arc::from([
                0_u8, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255, 255,
            ]),
            crate::view::ImageFit::Contain,
            crate::view::ImageSampling::Nearest,
            1.0,
            true,
        )?,
        &gpu,
    )?;
    assert_pixel_near(&decorated, 0, 0, [0, 0, 0, 0], 0, "decorated outside")?;
    assert_pixel_near(
        &decorated,
        12,
        28,
        [116, 3, 2, 255],
        2,
        "decorated border interior",
    )?;
    assert_pixel_near(
        &decorated,
        16,
        28,
        [2, 5, 12, 255],
        1,
        "decorated contain letterbox exposes background",
    )?;
    assert_pixel_near(
        &decorated,
        30,
        28,
        [0, 0, 255, 255],
        1,
        "decorated image paints over background",
    )?;
    Ok(())
}

#[test]
#[ignore = "requires native GPU adapter"]
fn native_sampled_texture_srgb_scale_generation_eviction_and_reset_have_pixel_oracles()
-> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native GPU initialized");
    let id = crate::view::sampled_texture::SampledTextureId::Image(
        crate::view::sampled_texture::ImageAssetId::for_test(0x4d34),
    );
    let params = direct_sampled_params([2.0, 2.0, 10.0, 10.0]);
    let unorm = render_with_config(
        direct_sampled_image_graph(
            solid_upload(id, 1, [128, 64, 32, 255]),
            params,
            wgpu::TextureFormat::Rgba8Unorm,
            false,
        )?,
        &gpu,
        1.0,
        wgpu::TextureFormat::Rgba8Unorm,
    )?;
    assert_pixel_near(&unorm, 5, 5, [55, 13, 4, 255], 2, "sRGB decode into Unorm")?;

    let srgb = render_with_config(
        direct_sampled_image_graph(
            solid_upload(id, 2, [128, 64, 32, 255]),
            params,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            false,
        )?,
        &gpu,
        2.0,
        wgpu::TextureFormat::Rgba8UnormSrgb,
    )?;
    assert_pixel_near(&srgb, 5, 5, [128, 64, 32, 255], 2, "sRGB target encode")?;
    assert_pixel_near(&srgb, 2, 2, [0, 0, 0, 0], 0, "scale-two bounds origin")?;

    let mut viewport = Viewport::new();
    let bounds = direct_sampled_params([0.0, 0.0, 12.0, 12.0]);
    let red = render_on_viewport(
        direct_sampled_image_graph(
            solid_upload(id, 10, [255, 0, 0, 255]),
            bounds,
            FORMAT,
            false,
        )?,
        &gpu,
        &mut viewport,
        1.0,
        FORMAT,
    )?;
    assert_pixel_near(&red, 5, 5, [255, 0, 0, 255], 0, "generation one")?;

    let blue = render_on_viewport(
        direct_sampled_image_graph(
            solid_upload(id, 11, [0, 0, 255, 255]),
            bounds,
            FORMAT,
            false,
        )?,
        &gpu,
        &mut viewport,
        1.0,
        FORMAT,
    )?;
    assert_pixel_near(&blue, 5, 5, [0, 0, 255, 255], 0, "generation reupload")?;

    viewport.evict_sampled_texture_for_test(id);
    let green = render_on_viewport(
        direct_sampled_image_graph(
            solid_upload(id, 11, [0, 255, 0, 255]),
            bounds,
            FORMAT,
            false,
        )?,
        &gpu,
        &mut viewport,
        1.0,
        FORMAT,
    )?;
    assert_pixel_near(&green, 5, 5, [0, 255, 0, 255], 0, "eviction reupload")?;

    viewport.release_render_resource_caches();
    let yellow = render_on_viewport(
        direct_sampled_image_graph(
            solid_upload(id, 11, [255, 255, 0, 255]),
            bounds,
            FORMAT,
            false,
        )?,
        &gpu,
        &mut viewport,
        1.0,
        FORMAT,
    )?;
    assert_pixel_near(&yellow, 5, 5, [255, 255, 0, 255], 0, "cache reset reupload")?;
    Ok(())
}

#[test]
#[ignore = "requires native GPU adapter"]
fn native_prepared_image_forced_transient_geometry_matches_prepared_buffers_at_scale_two()
-> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native GPU initialized");
    let adapter = gpu.label();
    let pixels: Arc<[u8]> = Arc::from([
        255_u8, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 255, 255, 255, 0, 64,
    ]);
    let normal = artifact_image_graph(
        pixels.clone(),
        crate::view::ImageFit::Cover,
        crate::view::ImageSampling::Nearest,
        0.7,
        false,
    )?;
    let mut forced = artifact_image_graph(
        pixels,
        crate::view::ImageFit::Cover,
        crate::view::ImageSampling::Nearest,
        0.7,
        false,
    )?;
    let mut passes =
        forced.test_graphics_passes_mut::<crate::view::render_pass::TextureCompositePass>();
    if passes.len() != 1 {
        return Err(format!(
            "forced fallback fixture expected one TextureComposite pass, got {}",
            passes.len()
        ));
    }
    passes[0].force_transient_geometry_fallback_for_test();

    let normal = render_with_config(normal, &gpu, 2.0, FORMAT)?;
    let forced = render_with_config(forced, &gpu, 2.0, FORMAT)?;
    compare_pixels(
        &normal,
        &forced,
        [0, 0, WIDTH, HEIGHT],
        &adapter,
        "prepared-image-forced-transient-scale-two-cover",
    )?;
    Ok(())
}
