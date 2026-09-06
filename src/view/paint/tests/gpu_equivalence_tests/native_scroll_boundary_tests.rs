use super::*;

#[test]
#[ignore = "requires native GPU adapter"]
// Exact closure for the currently admitted production contract: scale=1,
// paint offset=0, and no external scissor. Run explicitly with:
// cargo test -q native_production_direct_scroll_transform_matches_legacy_and_reuses_real_pair -- --ignored --nocapture
fn native_production_direct_scroll_transform_matches_legacy_and_reuses_real_pair()
-> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native GPU initialized");
    let adapter = gpu.label();
    let mut viewport = Viewport::new();

    let (cold_graph, cold_trace, cold_owner, resident, cold_composite) =
        production_direct_scroll_transform_graph(
            &mut viewport,
            DirectScrollTransformGpuCase::BASELINE,
        )?;
    validate_direct_scroll_transform_graph_shape(&cold_graph, cold_trace, true, "cold baseline")?;
    if viewport.has_compatible_persistent_render_target_pair(resident.0, &resident.1) {
        return Err("fresh direct S->T viewport unexpectedly had a resident T pair".to_string());
    }
    let cold_pixels = render_on_viewport(cold_graph, gpu, &mut viewport, 1.0, FORMAT)?;
    if !viewport.finish_retained_surface_transaction_for_frame(Some(cold_owner), true) {
        return Err("cold direct S->T transaction did not commit".to_string());
    }
    if !viewport.has_compatible_persistent_render_target_pair(resident.0, &resident.1) {
        return Err(format!(
            "cold direct S->T frame did not establish real T residency on {adapter}"
        ));
    }
    viewport.forget_retained_surface_pair_witness_for_test(resident.0);
    if !viewport.has_compatible_persistent_render_target_pair(resident.0, &resident.1) {
        return Err("direct S->T T pair depended only on the test witness".to_string());
    }
    let cold_legacy = render(
        legacy_direct_scroll_transform_graph(DirectScrollTransformGpuCase::BASELINE)?,
        gpu,
    )?;
    validate_direct_scroll_transform_gradient_coverage(
        &cold_legacy,
        DirectScrollTransformGpuCase::BASELINE,
        "cold legacy",
        &adapter,
    )?;
    let baseline_coverage = validate_direct_scroll_transform_gradient_coverage(
        &cold_pixels,
        DirectScrollTransformGpuCase::BASELINE,
        "cold production",
        &adapter,
    )?;
    compare_pixels(
        &cold_legacy,
        &cold_pixels,
        [0, 0, WIDTH, HEIGHT],
        &adapter,
        "production-direct-s-t/cold-r",
    )?;

    let (
        identical_graph,
        identical_trace,
        identical_owner,
        identical_resident,
        identical_composite,
    ) = production_direct_scroll_transform_graph(
        &mut viewport,
        DirectScrollTransformGpuCase::BASELINE,
    )?;
    if identical_resident != resident {
        return Err("direct S->T resident identity drifted on identical warm frame".to_string());
    }
    validate_direct_scroll_transform_graph_shape(
        &identical_graph,
        identical_trace,
        false,
        "identical warm",
    )?;
    if identical_composite != cold_composite {
        return Err(format!(
            "direct S->T identical warm composite drifted: cold={cold_composite:?}, warm={identical_composite:?}"
        ));
    }
    let identical_pixels = render_on_viewport(identical_graph, gpu, &mut viewport, 1.0, FORMAT)?;
    if !viewport.finish_retained_surface_transaction_for_frame(Some(identical_owner), true) {
        return Err("identical warm direct S->T transaction did not commit".to_string());
    }
    let identical_legacy = render(
        legacy_direct_scroll_transform_graph(DirectScrollTransformGpuCase::BASELINE)?,
        gpu,
    )?;
    validate_direct_scroll_transform_gradient_coverage(
        &identical_pixels,
        DirectScrollTransformGpuCase::BASELINE,
        "identical warm production",
        &adapter,
    )?;
    compare_pixels(
        &identical_legacy,
        &identical_pixels,
        [0, 0, WIDTH, HEIGHT],
        &adapter,
        "production-direct-s-t/identical-u",
    )?;

    let (scroll_graph, scroll_trace, scroll_owner, scroll_resident, scroll_composite) =
        production_direct_scroll_transform_graph(
            &mut viewport,
            DirectScrollTransformGpuCase::SCROLL_ONLY,
        )?;
    if scroll_resident != resident {
        return Err("direct S->T resident identity drifted on scroll-only frame".to_string());
    }
    validate_direct_scroll_transform_graph_shape(
        &scroll_graph,
        scroll_trace,
        false,
        "scroll-only warm",
    )?;
    validate_direct_scroll_transform_composite_delta(
        identical_composite,
        scroll_composite,
        [0.0, -8.0],
        "scroll-only",
    )?;
    let scroll_pixels = render_on_viewport(scroll_graph, gpu, &mut viewport, 1.0, FORMAT)?;
    if !viewport.finish_retained_surface_transaction_for_frame(Some(scroll_owner), true) {
        return Err("scroll-only direct S->T transaction did not commit".to_string());
    }
    let scroll_legacy = render(
        legacy_direct_scroll_transform_graph(DirectScrollTransformGpuCase::SCROLL_ONLY)?,
        gpu,
    )?;
    let scroll_coverage = validate_direct_scroll_transform_gradient_coverage(
        &scroll_pixels,
        DirectScrollTransformGpuCase::SCROLL_ONLY,
        "scroll-only production",
        &adapter,
    )?;
    validate_direct_scroll_transform_gradient_coverage(
        &scroll_legacy,
        DirectScrollTransformGpuCase::SCROLL_ONLY,
        "scroll-only legacy",
        &adapter,
    )?;
    if scroll_coverage.red >= baseline_coverage.red
        || scroll_coverage.blue <= baseline_coverage.blue
    {
        return Err(format!(
            "direct S->T scroll-only frame did not move sharp-gradient coverage: baseline={baseline_coverage:?}, scrolled={scroll_coverage:?}"
        ));
    }
    compare_pixels(
        &scroll_legacy,
        &scroll_pixels,
        [0, 0, WIDTH, HEIGHT],
        &adapter,
        "production-direct-s-t/scroll-only-u",
    )?;

    let (
        transform_graph,
        transform_trace,
        transform_owner,
        transform_resident,
        transform_composite,
    ) = production_direct_scroll_transform_graph(
        &mut viewport,
        DirectScrollTransformGpuCase::TRANSFORM_ONLY,
    )?;
    if transform_resident != resident {
        return Err("direct S->T resident identity drifted on transform-only frame".to_string());
    }
    validate_direct_scroll_transform_graph_shape(
        &transform_graph,
        transform_trace,
        false,
        "transform-only warm",
    )?;
    validate_direct_scroll_transform_composite_delta(
        scroll_composite,
        transform_composite,
        [6.0, 4.0],
        "transform-only",
    )?;
    let transform_pixels = render_on_viewport(transform_graph, gpu, &mut viewport, 1.0, FORMAT)?;
    if !viewport.finish_retained_surface_transaction_for_frame(Some(transform_owner), true) {
        return Err("transform-only direct S->T transaction did not commit".to_string());
    }
    if !viewport.has_compatible_persistent_render_target_pair(resident.0, &resident.1) {
        return Err("direct S->T mutation frames lost real T residency".to_string());
    }
    let transform_legacy = render(
        legacy_direct_scroll_transform_graph(DirectScrollTransformGpuCase::TRANSFORM_ONLY)?,
        gpu,
    )?;
    validate_direct_scroll_transform_gradient_coverage(
        &transform_pixels,
        DirectScrollTransformGpuCase::TRANSFORM_ONLY,
        "transform-only production",
        &adapter,
    )?;
    validate_direct_scroll_transform_gradient_coverage(
        &transform_legacy,
        DirectScrollTransformGpuCase::TRANSFORM_ONLY,
        "transform-only legacy",
        &adapter,
    )?;
    compare_pixels(
        &transform_legacy,
        &transform_pixels,
        [0, 0, WIDTH, HEIGHT],
        &adapter,
        "production-direct-s-t/transform-only-u",
    )?;
    eprintln!("production direct S->T real-pool GPU closure passed on {adapter}");
    Ok(())
}
