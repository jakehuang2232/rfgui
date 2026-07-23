use super::*;

#[test]
#[ignore = "requires native GPU adapter"]
// Run explicitly with:
// cargo test -q native_svg_straight_srgb_alpha_expected_pixel -- --ignored --nocapture
fn native_svg_straight_srgb_alpha_expected_pixel() -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native GPU initialized");
    let straight_srgb = [200_u8, 100, 50, 128];
    let prepared = crate::view::base_component::prepare_svg_fixture_for_test(
        r##"<svg width="4" height="4" xmlns="http://www.w3.org/2000/svg"><rect width="4" height="4" fill="#c86432" fill-opacity="0.5"/></svg>"##,
        crate::view::ImageFit::Fill,
        (4.0, 4.0),
        [8.0, 8.0, 24.0, 24.0],
        1.0,
    )?;
    let pixels = render(
        direct_sampled_image_graph(prepared.upload, prepared.params, FORMAT, false)?,
        gpu,
    )?;
    let expected = [
        // PresentSurface converts the internal premultiplied target back to
        // straight surface RGB. Therefore the observable RGB is the linear
        // decode of the straight sRGB source, while alpha remains independent.
        srgb_byte_to_linear_surface_byte(straight_srgb[0]),
        srgb_byte_to_linear_surface_byte(straight_srgb[1]),
        srgb_byte_to_linear_surface_byte(straight_srgb[2]),
        straight_srgb[3],
    ];
    assert_pixel_near(
        &pixels,
        16,
        16,
        expected,
        2,
        &format!("svg straight-sRGB alpha on {}", gpu.label()),
    )?;
    assert_pixel_near(&pixels, 0, 0, [0, 0, 0, 0], 0, "svg outside")?;
    Ok(())
}

#[test]
#[ignore = "requires native GPU adapter"]
// Run explicitly with:
// cargo test -q native_svg_fit_and_dpr2_expected_pixels -- --ignored --nocapture
fn native_svg_fit_and_dpr2_expected_pixels() -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native GPU initialized");
    let wide = r##"<svg width="101" height="37" viewBox="0 0 101 37" xmlns="http://www.w3.org/2000/svg"><rect width="25" height="37" fill="#ff0000"/><rect x="25" width="51" height="37" fill="#00ff00"/><rect x="76" width="25" height="37" fill="#0000ff"/></svg>"##;
    let tall = r##"<svg width="37" height="101" viewBox="0 0 37 101" xmlns="http://www.w3.org/2000/svg"><rect width="37" height="25" fill="#ff0000"/><rect y="25" width="37" height="51" fill="#00ff00"/><rect y="76" width="37" height="25" fill="#0000ff"/></svg>"##;
    let destination = [4.0, 4.0, 20.0, 20.0];
    for (shape, source, intrinsic) in [("wide", wide, (101.0, 37.0)), ("tall", tall, (37.0, 101.0))]
    {
        for fit in [
            crate::view::ImageFit::Contain,
            crate::view::ImageFit::Cover,
            crate::view::ImageFit::Fill,
        ] {
            let prepared = crate::view::base_component::prepare_svg_fixture_for_test(
                source,
                fit,
                intrinsic,
                destination,
                2.0,
            )?;
            let expected_extent = match (shape, fit) {
                ("wide", crate::view::ImageFit::Contain) => (64, 24),
                ("tall", crate::view::ImageFit::Contain) => (24, 64),
                ("wide", crate::view::ImageFit::Cover) => (128, 47),
                ("tall", crate::view::ImageFit::Cover) => (47, 128),
                (_, crate::view::ImageFit::Fill) => (64, 64),
                _ => unreachable!(),
            };
            if prepared.upload.extent() != expected_extent {
                return Err(format!(
                    "{shape}/{fit:?} DPR2 extent wrong: actual={:?}, expected={expected_extent:?}",
                    prepared.upload.extent()
                ));
            }
            let pixels = render_with_config(
                direct_sampled_image_graph(prepared.upload, prepared.params, FORMAT, false)?,
                gpu,
                2.0,
                FORMAT,
            )?;
            let case = format!("svg {shape}/{fit:?}/DPR2 on {}", gpu.label());
            match (shape, fit) {
                ("wide", crate::view::ImageFit::Contain | crate::view::ImageFit::Fill) => {
                    assert_pixel_near(&pixels, 12, 28, [255, 0, 0, 255], 1, &case)?;
                    assert_pixel_near(&pixels, 28, 28, [0, 255, 0, 255], 1, &case)?;
                    assert_pixel_near(&pixels, 44, 28, [0, 0, 255, 255], 1, &case)?;
                    if fit == crate::view::ImageFit::Contain {
                        assert_pixel_near(&pixels, 28, 12, [0, 0, 0, 0], 0, &case)?;
                    }
                }
                ("tall", crate::view::ImageFit::Contain | crate::view::ImageFit::Fill) => {
                    assert_pixel_near(&pixels, 28, 12, [255, 0, 0, 255], 1, &case)?;
                    assert_pixel_near(&pixels, 28, 28, [0, 255, 0, 255], 1, &case)?;
                    assert_pixel_near(&pixels, 28, 44, [0, 0, 255, 255], 1, &case)?;
                    if fit == crate::view::ImageFit::Contain {
                        assert_pixel_near(&pixels, 12, 28, [0, 0, 0, 0], 0, &case)?;
                    }
                }
                (_, crate::view::ImageFit::Cover) => {
                    for (x, y) in [(12, 12), (28, 28), (44, 44)] {
                        assert_pixel_near(&pixels, x, y, [0, 255, 0, 255], 1, &case)?;
                    }
                }
                _ => unreachable!(),
            }
        }
    }
    Ok(())
}
