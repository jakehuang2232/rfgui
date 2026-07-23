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

#[test]
#[ignore = "requires native GPU adapter"]
// Exact closure for the currently admitted production contract: scale=1,
// paint offset=0, and no external scissor. Run explicitly with:
// cargo test -q native_production_transform_and_effect_scroll_match_legacy_and_reuse_two_real_pairs -- --ignored --nocapture
fn native_production_transform_and_effect_scroll_match_legacy_and_reuse_two_real_pairs()
-> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native GPU initialized");
    let adapter = gpu.label();
    let sampled_at = crate::time::Instant::now();
    for (cold_grammar, warm_grammar) in [
        (
            DirectPropertyScrollGpuGrammar::Transform {
                translation: [7.0, 5.0],
            },
            DirectPropertyScrollGpuGrammar::Transform {
                translation: [17.0, 15.0],
            },
        ),
        (
            DirectPropertyScrollGpuGrammar::Effect { opacity: 0.625 },
            DirectPropertyScrollGpuGrammar::Effect { opacity: 0.875 },
        ),
    ] {
        let mut viewport = Viewport::new();
        let (cold_graph, cold_trace, cold_owner, cold_residents) =
            production_direct_property_scroll_graph(&mut viewport, cold_grammar, sampled_at)?;
        if cold_trace.reraster_count != 2 || cold_trace.reuse_count != 0 {
            return Err(format!(
                "cold {} frame did not naturally select R/R on {adapter}: {cold_trace:?}",
                cold_grammar.label()
            ));
        }
        validate_direct_property_scroll_graph_shape(&cold_graph, cold_grammar, true)?;
        if cold_residents
            .iter()
            .any(|(key, desc)| viewport.has_compatible_persistent_render_target_pair(*key, desc))
        {
            return Err(format!(
                "fresh {} viewport unexpectedly has a resident pair",
                cold_grammar.label()
            ));
        }
        let cold_pixels = render_on_viewport(cold_graph, gpu, &mut viewport, 1.0, FORMAT)?;
        if !viewport.finish_retained_surface_transaction_for_frame(Some(cold_owner), true) {
            return Err(format!(
                "cold {} transaction did not commit",
                cold_grammar.label()
            ));
        }
        for (key, desc) in &cold_residents {
            if !viewport.has_compatible_persistent_render_target_pair(*key, desc) {
                return Err(format!(
                    "cold {} frame did not establish pair {key:?} on {adapter}",
                    cold_grammar.label()
                ));
            }
            viewport.forget_retained_surface_pair_witness_for_test(*key);
            if !viewport.has_compatible_persistent_render_target_pair(*key, desc) {
                return Err(format!(
                    "{} pair {key:?} depended only on the test witness",
                    cold_grammar.label()
                ));
            }
        }
        let cold_legacy = render(legacy_direct_property_scroll_graph(cold_grammar)?, gpu)?;
        validate_direct_property_scroll_nonblank_anchor(
            &cold_legacy,
            cold_grammar,
            "cold legacy",
            &adapter,
        )?;
        validate_direct_property_scroll_nonblank_anchor(
            &cold_pixels,
            cold_grammar,
            "cold production",
            &adapter,
        )?;
        compare_pixels(
            &cold_legacy,
            &cold_pixels,
            [0, 0, WIDTH, HEIGHT],
            &adapter,
            &format!("production-{}/cold-r-r", cold_grammar.label()),
        )?;

        let (warm_graph, warm_trace, warm_owner, warm_residents) =
            production_direct_property_scroll_graph(&mut viewport, warm_grammar, sampled_at)?;
        if warm_trace.reraster_count != 0 || warm_trace.reuse_count != 2 {
            return Err(format!(
                "warm {} composite-only frame did not naturally select U/U on {adapter}: {warm_trace:?}",
                warm_grammar.label()
            ));
        }
        if !warm_direct_property_scroll_receiver_matches_cold(&cold_residents, &warm_residents) {
            return Err(format!(
                "{} warm graph did not declare exactly the cold receiver pair",
                warm_grammar.label()
            ));
        }
        validate_direct_property_scroll_graph_shape(&warm_graph, warm_grammar, false)?;
        let warm_pixels = render_on_viewport(warm_graph, gpu, &mut viewport, 1.0, FORMAT)?;
        if !viewport.finish_retained_surface_transaction_for_frame(Some(warm_owner), true) {
            return Err(format!(
                "warm {} transaction did not commit",
                warm_grammar.label()
            ));
        }
        for (key, desc) in &cold_residents {
            if !viewport.has_compatible_persistent_render_target_pair(*key, desc) {
                return Err(format!(
                    "warm {} frame lost cold physical pair {key:?}",
                    warm_grammar.label()
                ));
            }
        }
        let warm_legacy = render(legacy_direct_property_scroll_graph(warm_grammar)?, gpu)?;
        validate_direct_property_scroll_nonblank_anchor(
            &warm_legacy,
            warm_grammar,
            "warm legacy",
            &adapter,
        )?;
        validate_direct_property_scroll_nonblank_anchor(
            &warm_pixels,
            warm_grammar,
            "warm production",
            &adapter,
        )?;
        compare_pixels(
            &warm_legacy,
            &warm_pixels,
            [0, 0, WIDTH, HEIGHT],
            &adapter,
            &format!("production-{}/warm-u-u", warm_grammar.label()),
        )?;
    }
    eprintln!("production T->S/E->S real-pool GPU closure passed on {adapter}");
    Ok(())
}

#[test]
#[ignore = "requires native GPU adapter"]
// Exact closure for the currently admitted production contract: scale=1,
// paint offset=0 and no external scissor. Run with:
// cargo test -q native_production_transform_effect_scroll_matches_legacy_and_reuses_three_real_pairs -- --ignored --nocapture
fn native_production_transform_effect_scroll_matches_legacy_and_reuses_three_real_pairs()
-> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native GPU initialized");
    let adapter = gpu.label();
    let sampled_at = crate::time::Instant::now();
    let cold_frame = TransformEffectScrollGpuFrame {
        translation: [7.0, 3.0],
    };
    let mut viewport = Viewport::new();
    let (cold_graph, cold_trace, cold_owner, cold_residents) =
        production_transform_effect_scroll_graph(&mut viewport, cold_frame, sampled_at)?;
    if cold_trace.reraster_count != 3 || cold_trace.reuse_count != 0 {
        return Err(format!(
            "cold T->E->S frame did not naturally select R/R/R on {adapter}: {cold_trace:?}"
        ));
    }
    validate_transform_effect_scroll_graph_shape(&cold_graph, true)?;
    if !transform_effect_scroll_resident_roles_are_exact(&cold_residents, true)
        || cold_residents
            .iter()
            .any(|(key, desc)| viewport.has_compatible_persistent_render_target_pair(*key, desc))
    {
        return Err("fresh T->E->S viewport has an invalid cold residency shape".to_string());
    }
    let cold_pixels = render_on_viewport(cold_graph, gpu, &mut viewport, 1.0, FORMAT)?;
    if !viewport.finish_retained_surface_transaction_for_frame(Some(cold_owner), true) {
        return Err("cold T->E->S transaction did not commit".to_string());
    }
    for (key, desc) in &cold_residents {
        if !viewport.has_compatible_persistent_render_target_pair(*key, desc) {
            return Err(format!(
                "cold T->E->S frame did not establish pair {key:?} on {adapter}"
            ));
        }
        viewport.forget_retained_surface_pair_witness_for_test(*key);
        if !viewport.has_compatible_persistent_render_target_pair(*key, desc) {
            return Err(format!(
                "T->E->S pair {key:?} depended only on the test witness"
            ));
        }
    }
    let cold_legacy = render(legacy_transform_effect_scroll_graph(cold_frame)?, gpu)?;
    validate_transform_effect_scroll_nonblank_anchor(
        &cold_legacy,
        cold_frame,
        "cold legacy",
        &adapter,
    )?;
    validate_transform_effect_scroll_nonblank_anchor(
        &cold_pixels,
        cold_frame,
        "cold production",
        &adapter,
    )?;
    compare_pixels(
        &cold_legacy,
        &cold_pixels,
        [0, 0, WIDTH, HEIGHT],
        &adapter,
        "production-transform-effect-scroll/cold-r-r-r",
    )?;

    for (case, warm_frame) in [
        ("identical", cold_frame),
        (
            "translation-only",
            TransformEffectScrollGpuFrame {
                translation: [19.0, 11.0],
            },
        ),
    ] {
        let (warm_graph, warm_trace, warm_owner, warm_residents) =
            production_transform_effect_scroll_graph(&mut viewport, warm_frame, sampled_at)?;
        if warm_trace.reraster_count != 0 || warm_trace.reuse_count != 3 {
            return Err(format!(
                "T->E->S {case} warm frame did not naturally select U/U/U on {adapter}: {warm_trace:?}"
            ));
        }
        if !transform_effect_scroll_warm_declarations_match_cold(&cold_residents, &warm_residents) {
            return Err(format!(
                "T->E->S {case} warm declarations are not the cold T/E subset: cold={cold_residents:?}, warm={warm_residents:?}"
            ));
        }
        validate_transform_effect_scroll_graph_shape(&warm_graph, false)?;
        let warm_pixels = render_on_viewport(warm_graph, gpu, &mut viewport, 1.0, FORMAT)?;
        if !viewport.finish_retained_surface_transaction_for_frame(Some(warm_owner), true) {
            return Err(format!("T->E->S {case} warm transaction did not commit"));
        }
        for (key, desc) in &cold_residents {
            if !viewport.has_compatible_persistent_render_target_pair(*key, desc) {
                return Err(format!(
                    "T->E->S {case} warm frame lost cold physical pair {key:?}"
                ));
            }
        }
        let warm_legacy = render(legacy_transform_effect_scroll_graph(warm_frame)?, gpu)?;
        validate_transform_effect_scroll_nonblank_anchor(
            &warm_legacy,
            warm_frame,
            &format!("{case} warm legacy"),
            &adapter,
        )?;
        validate_transform_effect_scroll_nonblank_anchor(
            &warm_pixels,
            warm_frame,
            &format!("{case} warm production"),
            &adapter,
        )?;
        compare_pixels(
            &warm_legacy,
            &warm_pixels,
            [0, 0, WIDTH, HEIGHT],
            &adapter,
            &format!("production-transform-effect-scroll/{case}-warm-u-u-u"),
        )?;
    }
    eprintln!("production T->E->S real-pool GPU closure passed on {adapter}");
    Ok(())
}
