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
// cargo test -q native_forced_transform_surface_matches_legacy_pixels -- --ignored --nocapture
fn native_forced_transform_surface_matches_legacy_pixels() -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native GPU initialized");
    let adapter = gpu.label();
    for (case, scale_factor, outer_scissor) in [
        ("scale-1", 1.0, None),
        ("scale-2-outer-scissor", 2.0, Some([14, 10, 22, 18])),
    ] {
        let legacy = render_with_config(
            legacy_transformed_rect_graph(scale_factor, outer_scissor)?,
            gpu,
            scale_factor,
            FORMAT,
        )?;
        let forced = render_with_config(
            forced_transformed_rect_graph(scale_factor, outer_scissor)?,
            gpu,
            scale_factor,
            FORMAT,
        )?;
        let legacy_covered = legacy
            .chunks_exact(BYTES_PER_PIXEL as usize)
            .filter(|pixel| pixel[3] != 0)
            .count();
        let forced_covered = forced
            .chunks_exact(BYTES_PER_PIXEL as usize)
            .filter(|pixel| pixel[3] != 0)
            .count();
        if legacy_covered == 0 || forced_covered == 0 {
            return Err(format!(
                "{case}: transform parity fixture rendered blank on {adapter}: legacy={legacy_covered}, forced={forced_covered}"
            ));
        }
        compare_pixels(&legacy, &forced, [0, 0, WIDTH, HEIGHT], &adapter, case)?;
        eprintln!(
            "forced transform native parity {case} passed on {adapter}: covered={forced_covered}"
        );
    }
    Ok(())
}

#[test]
#[ignore = "requires native GPU adapter"]
// Run explicitly with:
// cargo test -q native_forced_nested_transform_surfaces_match_legacy_pixels -- --ignored --nocapture
fn native_forced_nested_transform_surfaces_match_legacy_pixels() -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native GPU initialized");
    let adapter = gpu.label();
    for (case, scale_factor, outer_scissor) in [
        ("nested-scale-1", 1.0, None),
        ("nested-scale-2-outer-scissor", 2.0, Some([8, 8, 42, 38])),
    ] {
        let legacy = render_with_config(
            legacy_nested_transformed_rect_graph(scale_factor, outer_scissor)?,
            gpu,
            scale_factor,
            FORMAT,
        )?;
        let forced = render_with_config(
            forced_nested_transformed_rect_graph(scale_factor, outer_scissor)?,
            gpu,
            scale_factor,
            FORMAT,
        )?;
        let legacy_covered = legacy
            .chunks_exact(BYTES_PER_PIXEL as usize)
            .filter(|pixel| pixel[3] != 0)
            .count();
        let forced_covered = forced
            .chunks_exact(BYTES_PER_PIXEL as usize)
            .filter(|pixel| pixel[3] != 0)
            .count();
        if legacy_covered == 0 || forced_covered == 0 {
            return Err(format!(
                "{case}: nested transform parity fixture rendered blank on {adapter}: legacy={legacy_covered}, forced={forced_covered}"
            ));
        }
        compare_pixels(&legacy, &forced, [0, 0, WIDTH, HEIGHT], &adapter, case)?;
        eprintln!(
            "forced nested transform native parity {case} passed on {adapter}: covered={forced_covered}"
        );
    }
    Ok(())
}

#[test]
#[ignore = "requires native GPU adapter"]
// Run explicitly with:
// cargo test -q native_forced_nested_r_u_and_u_u_frames_match_legacy_pixels -- --ignored --nocapture
fn native_forced_nested_r_u_and_u_u_frames_match_legacy_pixels() -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native GPU initialized");
    let adapter = gpu.label();
    let mut viewport = Viewport::new();

    let baseline =
        forced_nested_transformed_rect_graph_on_viewport(&mut viewport, 1.0, None, 7.0, 5.0)?;
    let _ = render_on_viewport(baseline, gpu, &mut viewport, 1.0, FORMAT)?;
    viewport.finish_retained_surface_transaction(true);

    let child_transform_only =
        forced_nested_transformed_rect_graph_on_viewport(&mut viewport, 1.0, None, 7.0, 8.0)?;
    if child_transform_only
        .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
        .len()
        != 2
        || child_transform_only.test_rect_pass_snapshots().len() != 3
        || child_transform_only
            .test_graphics_passes::<crate::view::render_pass::TextureCompositePass>()
            .len()
            != 2
    {
        return Err("native nested child transform-only frame did not select R/U".to_string());
    }
    let r_u = render_on_viewport(child_transform_only, gpu, &mut viewport, 1.0, FORMAT)?;
    viewport.finish_retained_surface_transaction(true);
    let r_u_legacy = render(
        legacy_nested_transformed_rect_graph_with_transforms(1.0, None, 7.0, 8.0)?,
        gpu,
    )?;
    compare_pixels(
        &r_u_legacy,
        &r_u,
        [0, 0, WIDTH, HEIGHT],
        &adapter,
        "nested-child-transform-only-r-u",
    )?;

    let parent_transform_only =
        forced_nested_transformed_rect_graph_on_viewport(&mut viewport, 1.0, None, 10.0, 8.0)?;
    if parent_transform_only
        .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
        .len()
        != 1
        || !parent_transform_only.test_rect_pass_snapshots().is_empty()
        || parent_transform_only
            .test_graphics_passes::<crate::view::render_pass::TextureCompositePass>()
            .len()
            != 1
    {
        return Err("native nested parent transform-only frame did not select U/U".to_string());
    }
    let u_u = render_on_viewport(parent_transform_only, gpu, &mut viewport, 1.0, FORMAT)?;
    viewport.finish_retained_surface_transaction(true);
    let u_u_legacy = render(
        legacy_nested_transformed_rect_graph_with_transforms(1.0, None, 10.0, 8.0)?,
        gpu,
    )?;
    compare_pixels(
        &u_u_legacy,
        &u_u,
        [0, 0, WIDTH, HEIGHT],
        &adapter,
        "nested-parent-transform-only-u-u",
    )?;
    eprintln!("nested R/U and U/U native parity passed on {adapter}");
    Ok(())
}

#[test]
#[ignore = "requires native GPU adapter"]
// Run explicitly with:
// cargo test -q native_production_transform_surface_reuses_real_pool_on_second_frame -- --ignored --nocapture
fn native_production_transform_surface_reuses_real_pool_on_second_frame() -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native GPU initialized");
    let adapter = gpu.label();
    let mut viewport = Viewport::new();

    let (first_graph, first_trace) = production_transformed_rect_graph(&mut viewport, 1.0, None)?;
    if first_trace.action != RetainedSurfaceCompileAction::Reraster {
        return Err(format!(
            "first production transform frame unexpectedly reused a non-resident pair on {adapter}: {:?}",
            first_trace.action
        ));
    }
    let first_pixels = render_on_viewport(first_graph, gpu, &mut viewport, 1.0, FORMAT)?;
    viewport.finish_retained_surface_transaction(true);

    let (second_graph, second_trace) = production_transformed_rect_graph(&mut viewport, 1.0, None)?;
    if second_trace.action != RetainedSurfaceCompileAction::Reuse {
        return Err(format!(
            "second production transform frame did not reuse the real resident GPU pair on {adapter}: {:?}",
            second_trace.action
        ));
    }
    let clear_count = second_graph
        .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
        .len();
    let raster_count = second_graph.test_graphics_passes::<DrawRectPass>().len();
    let composite_count = second_graph
        .test_graphics_passes::<crate::view::render_pass::texture_composite_pass::TextureCompositePass>()
        .len();
    if clear_count != 1 || raster_count != 0 || composite_count != 1 {
        return Err(format!(
            "second production transform frame emitted raster work on {adapter}: clears={clear_count}, rects={raster_count}, composites={composite_count}"
        ));
    }
    let second_pixels = render_on_viewport(second_graph, gpu, &mut viewport, 1.0, FORMAT)?;
    viewport.finish_retained_surface_transaction(true);
    compare_pixels(
        &first_pixels,
        &second_pixels,
        [0, 0, WIDTH, HEIGHT],
        &adapter,
        "production-transform/frame-2-real-pool-reuse",
    )?;
    eprintln!("production transform real-pool second-frame reuse passed on {adapter}");
    Ok(())
}

#[test]
#[ignore = "requires native GPU adapter"]
// Run explicitly with:
// cargo test -q native_production_retained_surface_tree_reuses_real_pool_on_second_frame -- --ignored --nocapture
fn native_production_retained_surface_tree_reuses_real_pool_on_second_frame() -> Result<(), String>
{
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native GPU initialized");
    let adapter = gpu.label();
    let mut viewport = Viewport::new();

    let (first_graph, first_traces) =
        production_nested_transformed_rect_graph(&mut viewport, 1.0, None)?;
    if first_traces.len() != 2
        || first_traces
            .iter()
            .any(|trace| trace.action != RetainedSurfaceCompileAction::Reraster)
    {
        return Err(format!(
            "first production tree frame did not select R/R on {adapter}: {first_traces:?}"
        ));
    }
    let first_pixels = render_on_viewport(first_graph, gpu, &mut viewport, 1.0, FORMAT)?;
    viewport.finish_retained_surface_transaction(true);

    let (second_graph, second_traces) =
        production_nested_transformed_rect_graph(&mut viewport, 1.0, None)?;
    if second_traces.len() != 2
        || second_traces
            .iter()
            .any(|trace| trace.action != RetainedSurfaceCompileAction::Reuse)
    {
        return Err(format!(
            "second production tree frame did not select U/U from the real pool on {adapter}: {second_traces:?}"
        ));
    }
    let clear_count = second_graph
        .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
        .len();
    let raster_count = second_graph.test_graphics_passes::<DrawRectPass>().len();
    let composite_count = second_graph
        .test_graphics_passes::<crate::view::render_pass::TextureCompositePass>()
        .len();
    if clear_count != 1 || raster_count != 0 || composite_count != 1 {
        return Err(format!(
            "second production tree frame emitted raster/child-composite work on {adapter}: clears={clear_count}, rects={raster_count}, composites={composite_count}"
        ));
    }
    let second_pixels = render_on_viewport(second_graph, gpu, &mut viewport, 1.0, FORMAT)?;
    viewport.finish_retained_surface_transaction(true);
    let legacy_pixels = render(legacy_nested_transformed_rect_graph(1.0, None)?, gpu)?;
    compare_pixels(
        &legacy_pixels,
        &first_pixels,
        [0, 0, WIDTH, HEIGHT],
        &adapter,
        "production-tree/frame-1-r-r",
    )?;
    compare_pixels(
        &first_pixels,
        &second_pixels,
        [0, 0, WIDTH, HEIGHT],
        &adapter,
        "production-tree/frame-2-real-pool-u-u",
    )?;
    eprintln!("production retained-surface tree real-pool reuse passed on {adapter}");
    Ok(())
}
