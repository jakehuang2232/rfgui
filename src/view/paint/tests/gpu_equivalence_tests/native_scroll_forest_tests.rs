use super::*;

#[test]
#[ignore = "requires native GPU adapter"]
// Run explicitly with:
// cargo test -q native_focused_atomic_projection_scroll_forest_matches_legacy_and_reuses_real_pair -- --ignored --nocapture
fn native_focused_atomic_projection_scroll_forest_matches_legacy_and_reuses_real_pair()
-> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native GPU initialized");
    let adapter = gpu.label();

    for (case, caret_visible, preedit) in [
        ("caret-visible", true, None),
        ("caret-hidden", false, None),
        (
            "preedit-caret-visible",
            true,
            Some(("中", Some((0, "中".len())))),
        ),
    ] {
        let mut viewport = Viewport::new();
        let (cold_graph, cold_trace, cold_owner, cold_residents) =
            retained_focused_atomic_projection_scroll_graph(&mut viewport, caret_visible, preedit)?;
        if cold_trace.reraster_count != 1
            || cold_trace.reuse_count != 0
            || cold_trace.tile_count != 1
            || cold_residents.len() != 1
        {
            return Err(format!(
                "cold focused atomic projection frame did not select one R pair on {adapter}: {case}, trace={cold_trace:?}, residents={cold_residents:?}"
            ));
        }
        let (resident_key, resident_desc) = cold_residents[0].clone();
        if viewport.has_compatible_persistent_render_target_pair(resident_key, &resident_desc) {
            return Err(format!(
                "fresh focused atomic projection viewport unexpectedly had resident pair on {adapter}: {case}"
            ));
        }
        let cold_pixels = render_on_viewport(cold_graph, gpu, &mut viewport, 1.0, FORMAT)?;
        if !viewport.finish_retained_surface_transaction_for_frame(Some(cold_owner), true) {
            return Err(format!(
                "cold focused atomic projection transaction did not commit on {adapter}: {case}"
            ));
        }
        if !viewport.has_compatible_persistent_render_target_pair(resident_key, &resident_desc) {
            return Err(format!(
                "cold focused atomic projection frame did not establish real residency on {adapter}: {case}"
            ));
        }

        let legacy_pixels = render(
            legacy_focused_atomic_projection_scroll_graph(caret_visible, preedit)?,
            gpu,
        )?;
        compare_pixels(
            &legacy_pixels,
            &cold_pixels,
            [0, 0, WIDTH, HEIGHT],
            &adapter,
            &format!("focused-atomic-projection/cold-r/{case}"),
        )?;

        let (warm_graph, warm_trace, warm_owner, warm_residents) =
            retained_focused_atomic_projection_scroll_graph(&mut viewport, caret_visible, preedit)?;
        if warm_residents.as_slice() != cold_residents.as_slice()
            || warm_trace.reraster_count != 0
            || warm_trace.reuse_count != 1
            || warm_trace.tile_count != 1
        {
            return Err(format!(
                "warm focused atomic projection frame did not reuse the same resident pair on {adapter}: {case}, cold={cold_trace:?}/{cold_residents:?}, warm={warm_trace:?}/{warm_residents:?}"
            ));
        }
        let warm_pixels = render_on_viewport(warm_graph, gpu, &mut viewport, 1.0, FORMAT)?;
        if !viewport.finish_retained_surface_transaction_for_frame(Some(warm_owner), true) {
            return Err(format!(
                "warm focused atomic projection transaction did not commit on {adapter}: {case}"
            ));
        }
        compare_pixels(
            &legacy_pixels,
            &warm_pixels,
            [0, 0, WIDTH, HEIGHT],
            &adapter,
            &format!("focused-atomic-projection/warm-u/{case}"),
        )?;
        compare_pixels(
            &cold_pixels,
            &warm_pixels,
            [0, 0, WIDTH, HEIGHT],
            &adapter,
            &format!("focused-atomic-projection/cold-warm/{case}"),
        )?;
    }
    Ok(())
}

#[test]
#[ignore = "requires native GPU adapter"]
// The two GPU roots are intentionally disjoint, so this pixel closure does
// not claim overlap-order coverage. The existing B4 CPU global-partition
// schedule test owns the exact root-order proof.
// Run explicitly with:
// cargo test -q native_production_multi_root_scroll_forest_matches_legacy_and_reuses_real_pool -- --ignored --nocapture
fn native_production_multi_root_scroll_forest_matches_legacy_and_reuses_real_pool()
-> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native GPU initialized");
    let adapter = gpu.label();
    let semantic_frame_time = crate::time::Instant::now();
    let mut viewport = Viewport::new();

    let (cold_graph, cold_trace, cold_owner, cold_residents) = production_scroll_forest_graph(
        &mut viewport,
        ScrollForestContentVersion::Baseline,
        semantic_frame_time,
    )?;
    validate_scroll_forest_graph_shape(&cold_graph, &cold_trace, cold_trace.tile_count, 0)?;
    if cold_residents
        .iter()
        .any(|(key, desc)| viewport.has_compatible_persistent_render_target_pair(*key, desc))
    {
        return Err("fresh scroll-forest viewport unexpectedly has a resident pair".to_string());
    }
    if viewport.retained_surface_transaction_shape_for_test() != (0, Some(cold_trace.tile_count)) {
        return Err(format!(
            "cold scroll-forest did not stage one exact joint transaction: {:?}",
            viewport.retained_surface_transaction_shape_for_test()
        ));
    }
    let cold_pixels = render_on_viewport(cold_graph, gpu, &mut viewport, 1.0, FORMAT)?;
    if !viewport.finish_retained_surface_transaction_for_frame(Some(cold_owner), true) {
        return Err("cold scroll-forest transaction did not commit".to_string());
    }
    for (key, desc) in &cold_residents {
        if !viewport.has_compatible_persistent_render_target_pair(*key, desc) {
            return Err(format!(
                "cold scroll-forest did not establish pair {key:?} on {adapter}"
            ));
        }
        viewport.forget_retained_surface_pair_witness_for_test(*key);
        if !viewport.has_compatible_persistent_render_target_pair(*key, desc) {
            return Err(format!(
                "scroll-forest pair {key:?} depended only on the test witness"
            ));
        }
    }
    let cold_legacy = render(
        legacy_scroll_forest_graph(ScrollForestContentVersion::Baseline)?,
        gpu,
    )?;
    validate_scroll_forest_anchors(
        &cold_legacy,
        ScrollForestContentVersion::Baseline,
        &adapter,
        "cold legacy",
    )?;
    validate_scroll_forest_anchors(
        &cold_pixels,
        ScrollForestContentVersion::Baseline,
        &adapter,
        "cold production",
    )?;
    compare_scroll_forest_pixels(
        &cold_legacy,
        &cold_pixels,
        &adapter,
        "multi-root-scroll-forest/cold",
    )?;

    let (warm_graph, warm_trace, warm_owner, warm_residents) = production_scroll_forest_graph(
        &mut viewport,
        ScrollForestContentVersion::Baseline,
        semantic_frame_time,
    )?;
    validate_scroll_forest_graph_shape(&warm_graph, &warm_trace, 0, warm_trace.tile_count)?;
    if warm_trace.tile_count != cold_trace.tile_count
        || !same_scroll_forest_residents(&cold_residents, &warm_residents)
    {
        return Err(format!(
            "warm scroll-forest resident identities drifted: cold={cold_residents:?}, warm={warm_residents:?}"
        ));
    }
    let warm_pixels = render_on_viewport(warm_graph, gpu, &mut viewport, 1.0, FORMAT)?;
    if !viewport.finish_retained_surface_transaction_for_frame(Some(warm_owner), true) {
        return Err("warm scroll-forest transaction did not commit".to_string());
    }
    for (key, desc) in &cold_residents {
        if !viewport.has_compatible_persistent_render_target_pair(*key, desc) {
            return Err(format!("warm scroll-forest lost pair {key:?}"));
        }
    }
    let warm_legacy = render(
        legacy_scroll_forest_graph(ScrollForestContentVersion::Baseline)?,
        gpu,
    )?;
    validate_scroll_forest_anchors(
        &warm_pixels,
        ScrollForestContentVersion::Baseline,
        &adapter,
        "warm production",
    )?;
    compare_scroll_forest_pixels(
        &warm_legacy,
        &warm_pixels,
        &adapter,
        "multi-root-scroll-forest/warm",
    )?;
    compare_pixels(
        &cold_pixels,
        &warm_pixels,
        [0, 0, WIDTH, HEIGHT],
        &adapter,
        "multi-root-scroll-forest/cold-warm-stability",
    )?;

    let (mixed_graph, mixed_trace, mixed_owner, mixed_residents) = production_scroll_forest_graph(
        &mut viewport,
        ScrollForestContentVersion::FirstRootMutated,
        semantic_frame_time,
    )?;
    validate_scroll_forest_graph_shape(&mixed_graph, &mixed_trace, 1, mixed_trace.tile_count - 1)?;
    if mixed_trace.tile_count != cold_trace.tile_count
        || !same_scroll_forest_residents(&cold_residents, &mixed_residents)
    {
        return Err(format!(
            "mixed scroll-forest resident identities drifted: cold={cold_residents:?}, mixed={mixed_residents:?}"
        ));
    }
    let mixed_pixels = render_on_viewport(mixed_graph, gpu, &mut viewport, 1.0, FORMAT)?;
    if !viewport.finish_retained_surface_transaction_for_frame(Some(mixed_owner), true) {
        return Err("mixed scroll-forest transaction did not commit".to_string());
    }
    let mixed_legacy = render(
        legacy_scroll_forest_graph(ScrollForestContentVersion::FirstRootMutated)?,
        gpu,
    )?;
    validate_scroll_forest_anchors(
        &mixed_pixels,
        ScrollForestContentVersion::FirstRootMutated,
        &adapter,
        "mixed production",
    )?;
    compare_scroll_forest_pixels(
        &mixed_legacy,
        &mixed_pixels,
        &adapter,
        "multi-root-scroll-forest/mixed-first-root-reraster",
    )?;
    validate_scroll_forest_right_root_unchanged(&warm_pixels, &mixed_pixels, &adapter)?;
    if pixel_at(&warm_pixels, 12, 16)? == pixel_at(&mixed_pixels, 12, 16)? {
        return Err("mixed scroll-forest frame did not visibly update the first root".to_string());
    }
    for (key, desc) in &cold_residents {
        if !viewport.has_compatible_persistent_render_target_pair(*key, desc) {
            return Err(format!("mixed scroll-forest lost pair {key:?}"));
        }
    }
    eprintln!(
        "production multi-root scroll-forest GPU closure passed on {adapter} (disjoint roots; B4 CPU seal owns overlap ordering)"
    );
    Ok(())
}
