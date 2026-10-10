use super::*;

/// Each corner texel of the 2x2 pattern is sampled alone near its image
/// corner, so even linear filtering reads it unblended there, and owner
/// opacity scales its alpha. The pattern's color channels are 0 or 255, so no
/// color-space conversion applies. Compares in the premultiplied domain the
/// RGBA8 targets quantize in, where low alpha cannot amplify rounding.
fn validate_opacity_corner_anchors(
    pixels: &[u8],
    anchors: [[u32; 2]; 4],
    opacity: f32,
    case: &str,
    adapter: &str,
) -> Result<(), String> {
    let texels = [
        [255_u8, 0, 0, 255],
        [0, 255, 0, 128],
        [0, 0, 255, 255],
        [255, 255, 0, 64],
    ];
    let premultiply = |[r, g, b, a]: [u8; 4]| {
        [r, g, b].map(|channel| (f32::from(channel) * f32::from(a) / 255.0).round() as u8)
    };
    for ([x, y], texel) in anchors.into_iter().zip(texels) {
        let alpha = (f32::from(texel[3]) * opacity).round() as u8;
        let expected = [texel[0], texel[1], texel[2], alpha];
        let actual = pixel_at(pixels, x, y)?;
        let near = actual[3].abs_diff(alpha) <= 1
            && premultiply(actual)
                .iter()
                .zip(premultiply(expected))
                .all(|(actual, expected)| actual.abs_diff(expected) <= 1);
        if !near {
            return Err(format!(
                "{case} corner ({x},{y}) is wrong on {adapter}: actual={actual:?}, expected={expected:?}"
            ));
        }
    }
    Ok(())
}

#[test]
#[ignore = "requires native GPU adapter"]
fn native_prepared_image_2x2_fit_sampling_alpha_and_arena_drop_match_oracles() -> Result<(), String>
{
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native GPU initialized");
    let adapter = gpu.label();
    let pixels: Arc<[u8]> = Arc::from([
        255_u8, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 255, 255, 255, 0, 64,
    ]);
    let fill = render(
        artifact_image_graph(
            pixels.clone(),
            crate::view::ImageFit::Fill,
            crate::view::ImageSampling::Nearest,
            1.0,
            false,
        )?,
        &gpu,
    )?;
    validate_nearest_fill_image_anchors(&fill, "artifact", &adapter)?;
    // Contain centers a 31-pixel square in the 47x31 box, leaving letterbox
    // bars; Cover scales to 47 pixels and crops eight rows top and bottom.
    for (fit, sampling, opacity, anchors, letterbox) in [
        (
            crate::view::ImageFit::Contain,
            crate::view::ImageSampling::Linear,
            0.65,
            [[12, 5], [35, 5], [12, 25], [35, 25]],
            Some([2, 15]),
        ),
        (
            crate::view::ImageFit::Cover,
            crate::view::ImageSampling::Nearest,
            0.4,
            [[5, 4], [40, 4], [5, 27], [40, 27]],
            None,
        ),
    ] {
        let case = format!("prepared-image-{fit:?}-{sampling:?}-{opacity}");
        let artifact = render(
            artifact_image_graph(pixels.clone(), fit, sampling, opacity, false)?,
            &gpu,
        )?;
        validate_opacity_corner_anchors(&artifact, anchors, opacity, &case, &adapter)?;
        if let Some([x, y]) = letterbox {
            assert_pixel_near(
                &artifact,
                x,
                y,
                [0, 0, 0, 0],
                0,
                &format!("{case} letterbox"),
            )?;
        }
    }

    // Independent of either renderer: opaque blue replaces the background
    // inside the group's content box, then owner opacity 0.65 gives alpha166.
    // Per-op opacity would leave background color and alpha above166 here.
    for (path, graph) in [
        (
            "legacy",
            legacy_image_graph(
                Arc::from([0, 0, 255, 255].repeat(4)),
                crate::view::ImageFit::Fill,
                crate::view::ImageSampling::Nearest,
                0.65,
                true,
            )?,
        ),
        (
            "group artifact",
            artifact_image_graph(
                Arc::from([0, 0, 255, 255].repeat(4)),
                crate::view::ImageFit::Fill,
                crate::view::ImageSampling::Nearest,
                0.65,
                true,
            )?,
        ),
    ] {
        let output = render(graph, gpu)?;
        assert_pixel_near(&output, 30, 28, [0, 0, 255, 166], 1, path)?;
        assert_pixel_near(&output, 0, 0, [0; 4], 0, path)?;
    }
    eprintln!("native PreparedImage oracles passed on {adapter}");
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
