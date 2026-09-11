use super::*;

#[test]
#[ignore = "requires native GPU adapter"]
// Run explicitly with:
// cargo test -q native_offscreen_legacy_and_artifact_pixels_match -- --ignored --nocapture
fn native_offscreen_legacy_and_artifact_pixels_match() -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native GPU initialized");
    let adapter = gpu.label();
    for (case, with_border) in [("solid-fill", false), ("solid-fill-border", true)] {
        let legacy = render(legacy_graph(with_border)?, &gpu)?;
        let artifact = render(artifact_graph(with_border)?, &gpu)?;
        validate_color_anchors(&legacy, with_border, &format!("{case}/legacy"), &adapter)?;
        validate_color_anchors(
            &artifact,
            with_border,
            &format!("{case}/artifact"),
            &adapter,
        )?;
        compare_pixels(&legacy, &artifact, [12, 12, 24, 12], &adapter, case)?;
    }
    let legacy = render(legacy_self_clip_graph()?, &gpu)?;
    let artifact = render(artifact_self_clip_graph()?, &gpu)?;
    let expected_clipped = rgba8_unorm(Color::rgb(220, 40, 30));
    for (path, pixels) in [("legacy", &legacy), ("artifact", &artifact)] {
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
    }
    compare_pixels(&legacy, &artifact, [30, 8, 20, 16], &adapter, "self-clip")?;
    eprintln!("native pixel parity passed on {adapter}");
    Ok(())
}

#[test]
#[ignore = "requires native GPU adapter"]
// Run explicitly with:
// cargo test -q native_zero_surface_v2_matches_legacy_pixels -- --ignored --nocapture
fn native_zero_surface_v2_matches_legacy_pixels() -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native GPU initialized");
    let adapter = gpu.label();
    for (case, with_border) in [("solid-fill", false), ("solid-fill-border", true)] {
        let legacy = render(legacy_graph(with_border)?, &gpu)?;
        let v2 = render(zero_surface_v2_graph(with_border)?, &gpu)?;
        validate_color_anchors(&legacy, with_border, &format!("{case}/legacy"), &adapter)?;
        validate_color_anchors(&v2, with_border, &format!("{case}/v2"), &adapter)?;
        compare_pixels(&legacy, &v2, [12, 12, 24, 12], &adapter, case)?;
    }
    let legacy = render(legacy_self_clip_graph()?, &gpu)?;
    let v2 = render(zero_surface_v2_self_clip_graph()?, &gpu)?;
    let expected_clipped = rgba8_unorm(Color::rgb(220, 40, 30));
    for (path, pixels) in [("legacy", &legacy), ("v2", &v2)] {
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
    }
    compare_pixels(&legacy, &v2, [30, 8, 20, 16], &adapter, "self-clip")?;
    eprintln!("native zero-surface V2 pixel parity passed on {adapter}");
    Ok(())
}
