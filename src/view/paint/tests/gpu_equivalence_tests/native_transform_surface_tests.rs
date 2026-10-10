use super::*;

/// The AnchorParent rect replaces the ancestor scissor at [30, 8, 20, 16];
/// the sibling drawn after it must not inherit that scissor.
fn validate_self_clip_pixels(pixels: &[u8], path: &str, adapter: &str) -> Result<(), String> {
    let expected_clipped = rgba8_unorm(Color::rgb(220, 40, 30));
    let escaped = pixel_at(pixels, 35, 12)?;
    if escaped != expected_clipped {
        return Err(format!(
            "self-clip/{path} AnchorParent replace anchor is wrong on {adapter}: actual={escaped:?}, expected={expected_clipped:?}"
        ));
    }
    let restored = pixel_at(pixels, 35, 40)?;
    if restored != [0, 0, 0, 0] {
        return Err(format!(
            "self-clip/{path} restored sibling anchor is wrong on {adapter}: actual={restored:?}, expected=[0, 0, 0, 0]"
        ));
    }
    for y in 9..23 {
        for x in 31..49 {
            let actual = pixel_at(pixels, x, y)?;
            if actual != expected_clipped {
                return Err(format!(
                    "self-clip/{path} interior ({x},{y}) is wrong on {adapter}: actual={actual:?}, expected={expected_clipped:?}"
                ));
            }
        }
    }
    Ok(())
}

#[test]
#[ignore = "requires native GPU adapter"]
// Run explicitly with:
// cargo test -q native_offscreen_artifact_pixels_match_geometry -- --ignored --nocapture
fn native_offscreen_artifact_pixels_match_geometry() -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native GPU initialized");
    let adapter = gpu.label();
    for (case, with_border) in [("solid-fill", false), ("solid-fill-border", true)] {
        let artifact = render(artifact_graph(with_border)?, &gpu)?;
        let path = format!("{case}/artifact");
        validate_color_anchors(&artifact, with_border, &path, &adapter)?;
        assert_fixture_geometry(&artifact, with_border, &adapter, &path)?;
    }
    let artifact = render(artifact_self_clip_graph()?, &gpu)?;
    validate_self_clip_pixels(&artifact, "artifact", &adapter)?;
    eprintln!("native artifact pixel geometry passed on {adapter}");
    Ok(())
}

#[test]
#[ignore = "requires native GPU adapter"]
// Run explicitly with:
// cargo test -q native_zero_surface_v2_pixels_match_geometry -- --ignored --nocapture
fn native_zero_surface_v2_pixels_match_geometry() -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native GPU initialized");
    let adapter = gpu.label();
    for (case, with_border) in [("solid-fill", false), ("solid-fill-border", true)] {
        let v2 = render(zero_surface_v2_graph(with_border)?, &gpu)?;
        let path = format!("{case}/v2");
        validate_color_anchors(&v2, with_border, &path, &adapter)?;
        assert_fixture_geometry(&v2, with_border, &adapter, &path)?;
    }
    let v2 = render(zero_surface_v2_self_clip_graph()?, &gpu)?;
    validate_self_clip_pixels(&v2, "v2", &adapter)?;
    eprintln!("native zero-surface V2 pixel geometry passed on {adapter}");
    Ok(())
}
