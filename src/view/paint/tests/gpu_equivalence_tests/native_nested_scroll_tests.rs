use super::*;

#[test]
#[ignore = "requires native GPU adapter"]
// SVG here proves parity for the exact prepared/frozen raster payload. It is
// deliberately not an SVG parser or rasterizer end-to-end test.
// Run explicitly with:
// cargo test -q native_production_nested_scroll_image_svg_text_frozen_payloads_match_legacy_and_reuse_real_r1 -- --ignored --nocapture
fn native_production_nested_scroll_image_svg_text_frozen_payloads_match_legacy_and_reuse_real_r1()
-> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native GPU initialized");
    let adapter = gpu.label();
    let outer_offset_y = 2.0;
    let inner_offset_y = 3.0;

    for kind in NestedScrollGpuLeafKind::GPU_CLOSURE {
        let mut viewport = Viewport::new();
        let (cold_graph, cold_trace, cold_owner, leaf_key, leaf_desc) =
            production_nested_scroll_leaf_graph(
                &mut viewport,
                kind,
                outer_offset_y,
                inner_offset_y,
                None,
            )?;
        if cold_trace.reraster_count != 1 || cold_trace.reuse_count != 0 {
            return Err(format!(
                "cold nested-scroll {} frame did not select R on {adapter}: {cold_trace:?}",
                kind.label()
            ));
        }
        validate_nested_scroll_leaf_graph_shape(&cold_graph, kind, true)?;
        if viewport.has_compatible_persistent_render_target_pair(leaf_key, &leaf_desc) {
            return Err(format!(
                "fresh nested-scroll {} viewport unexpectedly had R1 residency",
                kind.label()
            ));
        }
        let cold_pixels = render_on_viewport(cold_graph, gpu, &mut viewport, 1.0, FORMAT)?;
        if !viewport.finish_retained_surface_transaction_for_frame(Some(cold_owner), true) {
            return Err(format!(
                "cold nested-scroll {} transaction did not commit",
                kind.label()
            ));
        }
        if !viewport.has_compatible_persistent_render_target_pair(leaf_key, &leaf_desc) {
            return Err(format!(
                "cold nested-scroll {} frame did not establish real R1 residency on {adapter}",
                kind.label()
            ));
        }
        viewport.forget_retained_surface_pair_witness_for_test(leaf_key);
        if !viewport.has_compatible_persistent_render_target_pair(leaf_key, &leaf_desc) {
            return Err(format!(
                "nested-scroll {} R1 residency depended only on the test witness",
                kind.label()
            ));
        }

        let legacy_graph =
            legacy_nested_scroll_leaf_graph(kind, outer_offset_y, inner_offset_y, None)?;
        validate_nested_scroll_legacy_leaf_graph_shape(&legacy_graph, kind)?;
        let legacy_pixels = render(legacy_graph, gpu)?;
        validate_nested_scroll_leaf_anchor(&legacy_pixels, kind)?;
        validate_nested_scroll_leaf_anchor(&cold_pixels, kind)?;
        compare_pixels(
            &legacy_pixels,
            &cold_pixels,
            [0, 0, WIDTH, HEIGHT],
            &adapter,
            &format!("production-nested-scroll-{}/cold-r", kind.label()),
        )?;

        let (warm_graph, warm_trace, warm_owner, warm_key, warm_desc) =
            production_nested_scroll_leaf_graph(
                &mut viewport,
                kind,
                outer_offset_y,
                inner_offset_y,
                None,
            )?;
        if warm_key != leaf_key || warm_desc != leaf_desc {
            return Err(format!(
                "nested-scroll {} R1 identity drifted between identical frames",
                kind.label()
            ));
        }
        if warm_trace.reraster_count != 0 || warm_trace.reuse_count != 1 {
            return Err(format!(
                "warm nested-scroll {} frame did not naturally select U on {adapter}: {warm_trace:?}",
                kind.label()
            ));
        }
        validate_nested_scroll_leaf_graph_shape(&warm_graph, kind, false)?;
        let warm_pixels = render_on_viewport(warm_graph, gpu, &mut viewport, 1.0, FORMAT)?;
        if !viewport.finish_retained_surface_transaction_for_frame(Some(warm_owner), true) {
            return Err(format!(
                "warm nested-scroll {} transaction did not commit",
                kind.label()
            ));
        }
        if !viewport.has_compatible_persistent_render_target_pair(leaf_key, &leaf_desc) {
            return Err(format!(
                "warm nested-scroll {} frame lost real R1 residency",
                kind.label()
            ));
        }
        compare_pixels(
            &legacy_pixels,
            &warm_pixels,
            [0, 0, WIDTH, HEIGHT],
            &adapter,
            &format!("production-nested-scroll-{}/warm-u", kind.label()),
        )?;
        compare_pixels(
            &cold_pixels,
            &warm_pixels,
            [0, 0, WIDTH, HEIGHT],
            &adapter,
            &format!("production-nested-scroll-{}/cold-warm", kind.label()),
        )?;
    }
    eprintln!(
        "production nested-scroll Image/SVG(frozen raster)/Text GPU closure passed on {adapter}"
    );
    Ok(())
}

#[test]
#[ignore = "requires native GPU adapter"]
// Run explicitly with:
// cargo test -q native_production_nested_scroll_matches_legacy_and_reuses_real_r1 -- --ignored --nocapture
fn native_production_nested_scroll_matches_legacy_and_reuses_real_r1() -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native GPU initialized");
    let adapter = gpu.label();
    let mut viewport = Viewport::new();
    let outer_offset_y = 13.0;
    let inner_offset_y = 9.0;
    let outer_scissor = None;

    let (cold_graph, cold_trace, cold_owner, leaf_key, leaf_desc) = production_nested_scroll_graph(
        &mut viewport,
        outer_offset_y,
        inner_offset_y,
        outer_scissor,
    )?;
    if cold_trace.reraster_count != 1 || cold_trace.reuse_count != 0 {
        return Err(format!(
            "cold nested-scroll frame did not select R on {adapter}: {cold_trace:?}"
        ));
    }
    if viewport.has_compatible_persistent_render_target_pair(leaf_key, &leaf_desc) {
        return Err("fresh nested-scroll viewport unexpectedly had a resident R1 pair".to_string());
    }
    let cold_clear_count = cold_graph
        .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
        .len();
    if cold_clear_count != 3 {
        return Err(format!(
            "cold nested-scroll graph must clear root, transient A0, and R1: {cold_clear_count}"
        ));
    }
    let cold_pixels = render_on_viewport(cold_graph, gpu, &mut viewport, 1.0, FORMAT)?;
    if !viewport.finish_retained_surface_transaction_for_frame(Some(cold_owner), true) {
        return Err("cold nested-scroll transaction owner was not committed".to_string());
    }
    if !viewport.has_compatible_persistent_render_target_pair(leaf_key, &leaf_desc) {
        return Err(format!(
            "cold nested-scroll compile/execute did not establish real R1 residency on {adapter}"
        ));
    }
    viewport.forget_retained_surface_pair_witness_for_test(leaf_key);
    if !viewport.has_compatible_persistent_render_target_pair(leaf_key, &leaf_desc) {
        return Err("removing the test witness must not remove real R1 residency".to_string());
    }

    let legacy_pixels = render(
        legacy_nested_scroll_graph(outer_offset_y, inner_offset_y, outer_scissor)?,
        gpu,
    )?;
    compare_pixels(
        &legacy_pixels,
        &cold_pixels,
        [0, 0, WIDTH, HEIGHT],
        &adapter,
        "production-nested-scroll/frame-1-r1",
    )?;

    let (warm_graph, warm_trace, warm_owner, warm_leaf_key, warm_leaf_desc) =
        production_nested_scroll_graph(
            &mut viewport,
            outer_offset_y,
            inner_offset_y,
            outer_scissor,
        )?;
    if warm_leaf_key != leaf_key || warm_leaf_desc != leaf_desc {
        return Err("nested-scroll R1 identity drifted between identical frames".to_string());
    }
    if warm_trace.reraster_count != 0 || warm_trace.reuse_count != 1 {
        return Err(format!(
            "warm nested-scroll frame did not naturally select U from the real pool on {adapter}: {warm_trace:?}"
        ));
    }
    let warm_clear_count = warm_graph
        .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
        .len();
    if warm_clear_count != 2 {
        return Err(format!(
            "warm nested-scroll graph must clear only root and transient A0, not R1: {warm_clear_count}"
        ));
    }
    let warm_pixels = render_on_viewport(warm_graph, gpu, &mut viewport, 1.0, FORMAT)?;
    if !viewport.finish_retained_surface_transaction_for_frame(Some(warm_owner), true) {
        return Err("warm nested-scroll transaction owner was not committed".to_string());
    }
    if !viewport.has_compatible_persistent_render_target_pair(leaf_key, &leaf_desc) {
        return Err("warm nested-scroll frame lost real R1 residency".to_string());
    }
    compare_pixels(
        &cold_pixels,
        &warm_pixels,
        [0, 0, WIDTH, HEIGHT],
        &adapter,
        "production-nested-scroll/frame-2-real-pool-u",
    )?;
    eprintln!("production nested-scroll real-pool parity passed on {adapter}");
    Ok(())
}
