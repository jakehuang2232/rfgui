use super::*;

#[test]
#[ignore = "requires native GPU adapter"]
// Run explicitly with:
// cargo test -q native_production_isolation_reuses_real_pool_on_opacity_only_frame -- --ignored --nocapture
fn native_production_isolation_reuses_real_pool_on_opacity_only_frame() -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native GPU initialized");
    let adapter = gpu.label();
    let mut viewport = Viewport::new();
    let (first_graph, first_trace) = production_isolation_graph(&mut viewport, 0.5)?;
    if first_trace.action != RetainedSurfaceCompileAction::Reraster {
        return Err(format!("first isolation frame was not R on {adapter}"));
    }
    let _ = render_on_viewport(first_graph, gpu, &mut viewport, 1.0, FORMAT)?;
    viewport.finish_retained_surface_transaction(true);

    let (second_graph, second_trace) = production_isolation_graph(&mut viewport, 0.25)?;
    if second_trace.action != RetainedSurfaceCompileAction::Reuse
        || second_graph
            .test_graphics_passes::<crate::view::frame_graph::ClearPass>()
            .len()
            != 1
        || !second_graph.test_rect_pass_snapshots().is_empty()
        || second_graph
            .test_graphics_passes::<crate::view::render_pass::composite_layer_pass::CompositeLayerPass>()
            .len()
            != 1
    {
        return Err(format!(
            "opacity-only isolation frame did not select real-pool U on {adapter}: {:?}",
            second_trace.action
        ));
    }
    let reused = render_on_viewport(second_graph, gpu, &mut viewport, 1.0, FORMAT)?;
    viewport.finish_retained_surface_transaction(true);

    let mut fresh_viewport = Viewport::new();
    let (fresh_graph, fresh_trace) = production_isolation_graph(&mut fresh_viewport, 0.25)?;
    if fresh_trace.action != RetainedSurfaceCompileAction::Reraster {
        return Err("fresh isolation oracle was not R".to_string());
    }
    let fresh = render_on_viewport(fresh_graph, gpu, &mut fresh_viewport, 1.0, FORMAT)?;
    fresh_viewport.finish_retained_surface_transaction(true);
    compare_pixels(
        &fresh,
        &reused,
        [0, 0, WIDTH, HEIGHT],
        &adapter,
        "production-isolation/opacity-only-real-pool-reuse",
    )?;
    eprintln!("production isolation real-pool opacity-only reuse passed on {adapter}");
    Ok(())
}

#[test]
#[ignore = "requires native GPU adapter"]
// Run explicitly with:
// cargo test -q native_root_group_opacity_matches_explicit_offscreen_overlap_oracle -- --ignored --nocapture
fn native_root_group_opacity_matches_explicit_offscreen_overlap_oracle() -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native GPU initialized");
    let adapter = gpu.label();
    let anchors = [(10, 10), (24, 20), (45, 20)];

    for opacity in [0.0_f32, 0.5, 1.0] {
        let artifact = render(artifact_root_group_overlap_graph(opacity)?, gpu)?;
        let explicit = render(explicit_root_group_overlap_graph(opacity)?, gpu)?;
        let case = format!("root-group-overlap-opacity-{opacity}");
        compare_pixels(&explicit, &artifact, [21, 17, 16, 16], &adapter, &case)?;
        let expected_anchors = root_group_anchor_oracle(opacity);
        for (anchor_index, &(x, y)) in anchors.iter().enumerate() {
            assert_pixel_near(
                &artifact,
                x,
                y,
                expected_anchors[anchor_index],
                1,
                &format!("{case} independent CPU source-over on {adapter}"),
            )?;
        }
    }
    eprintln!("root group explicit offscreen oracle passed on {adapter}");
    Ok(())
}

#[test]
#[ignore = "requires native GPU adapter"]
// Run explicitly with:
// cargo test -q native_retained_root_effect_reuses_raster_across_opacity_only_frame -- --ignored --nocapture
fn native_retained_root_effect_reuses_raster_across_opacity_only_frame() -> Result<(), String> {
    const FIRST_OPACITY: f32 = 0.8;
    const SECOND_OPACITY: f32 = 0.4;
    const REPAINTED_FIRST_COLOR: [f32; 4] = [0.12, 0.9, 0.2, 0.7];

    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native GPU initialized");
    let adapter = gpu.label();
    let mut viewport = Viewport::new();

    // Frame 1 has no resident witness, so it must clear and raster the
    // persistent layer before compositing it into the frame output.
    let first_artifact = root_group_overlap_artifact(FIRST_OPACITY);
    let (first_stamp, first_key, first_desc) = retained_root_effect_witness(&first_artifact)?;
    let first_action =
        viewport.test_root_effect_compile_action(&first_stamp, first_key, &first_desc);
    if first_action != RootEffectCompileAction::Reraster {
        return Err(format!(
            "frame 1 unexpectedly reused a non-resident root layer on {adapter}: {first_action:?}"
        ));
    }
    let first_graph = retained_root_group_graph(&first_artifact, first_action)?;
    assert_retained_root_effect_graph_shape(&first_graph, 2, 2, "frame 1/reraster")?;
    let first_pixels = render_on_viewport(first_graph, gpu, &mut viewport, 1.0, FORMAT)?;
    viewport.test_commit_root_effect_transaction(first_stamp.clone(), first_key, first_action);
    let first_reference = render(explicit_root_group_overlap_graph(FIRST_OPACITY)?, gpu)?;
    compare_pixels(
        &first_reference,
        &first_pixels,
        [21, 17, 16, 16],
        &adapter,
        "retained-root/frame-1-reraster",
    )?;

    // Frame 2 changes only root opacity. Root opacity and its composite
    // revision are intentionally outside the raster stamp, while the pool
    // must still contain an exact compatible color/depth pair.
    let second_artifact = root_group_overlap_artifact(SECOND_OPACITY);
    let (second_stamp, second_key, second_desc) = retained_root_effect_witness(&second_artifact)?;
    if second_stamp != first_stamp || second_key != first_key {
        return Err("opacity-only frame changed the retained root raster witness".to_string());
    }
    let second_action =
        viewport.test_root_effect_compile_action(&second_stamp, second_key, &second_desc);
    if second_action != RootEffectCompileAction::Reuse {
        return Err(format!(
            "frame 2 failed to reuse the compatible resident root layer on {adapter}: {second_action:?}"
        ));
    }
    let second_graph = retained_root_group_graph(&second_artifact, second_action)?;
    assert_retained_root_effect_graph_shape(&second_graph, 1, 0, "frame 2/reuse")?;
    let second_pixels = render_on_viewport(second_graph, gpu, &mut viewport, 1.0, FORMAT)?;
    viewport.test_commit_root_effect_transaction(
        second_stamp.clone(),
        second_key,
        second_action,
    );
    let second_reference = render(explicit_root_group_overlap_graph(SECOND_OPACITY)?, gpu)?;
    compare_pixels(
        &second_reference,
        &second_pixels,
        [21, 17, 16, 16],
        &adapter,
        "retained-root/frame-2-opacity-only-reuse",
    )?;

    // Frame 3 changes raster content and advances its self-paint revision.
    // The committed witness must reject reuse, and the changed first-only
    // anchor proves that the persistent texture was actually rerastered.
    let mut repainted_artifact = root_group_overlap_artifact(SECOND_OPACITY);
    let PaintOp::DrawRect(first_rect) = &mut repainted_artifact.ops[0] else {
        return Err("frame 3 first paint op is not a rectangle".to_string());
    };
    first_rect.params.fill_color = REPAINTED_FIRST_COLOR;
    repainted_artifact.chunks[0].payload_identity =
        PaintPayloadIdentity::prepared_shadows_with_decoration(
            std::iter::empty(),
            [&*first_rect],
        )
        .expect("repainted decoration carries its current exact payload identity");
    repainted_artifact.chunks[0]
        .content_revision
        .self_paint_revision = repainted_artifact.chunks[0]
        .content_revision
        .self_paint_revision
        .saturating_add(1);
    let (repainted_stamp, repainted_key, repainted_desc) =
        retained_root_effect_witness(&repainted_artifact)?;
    if repainted_key != second_key || repainted_desc != second_desc {
        return Err(
            "frame 3 changed the retained target identity instead of only raster content"
                .to_string(),
        );
    }
    if repainted_stamp == second_stamp {
        return Err(
            "raster-affecting frame did not change the root raster witness".to_string(),
        );
    }
    let repainted_action = viewport.test_root_effect_compile_action(
        &repainted_stamp,
        repainted_key,
        &repainted_desc,
    );
    if repainted_action != RootEffectCompileAction::Reraster {
        return Err(format!(
            "frame 3 reused stale root pixels after a paint revision on {adapter}: {repainted_action:?}"
        ));
    }
    let repainted_graph = retained_root_group_graph(&repainted_artifact, repainted_action)?;
    assert_retained_root_effect_graph_shape(&repainted_graph, 2, 2, "frame 3/reraster")?;
    let repainted_pixels =
        render_on_viewport(repainted_graph, gpu, &mut viewport, 1.0, FORMAT)?;
    viewport.test_commit_root_effect_transaction(
        repainted_stamp,
        repainted_key,
        repainted_action,
    );
    assert_pixel_near(
        &repainted_pixels,
        10,
        10,
        root_group_raster_readback(premultiply(REPAINTED_FIRST_COLOR), SECOND_OPACITY),
        1,
        &format!("retained-root/frame-3-reraster anchor on {adapter}"),
    )?;
    if pixel_at(&repainted_pixels, 10, 10)? == pixel_at(&second_pixels, 10, 10)? {
        return Err(format!(
            "frame 3 retained the stale first-only anchor after reraster on {adapter}"
        ));
    }

    eprintln!("retained root-effect two-frame reuse oracle passed on {adapter}");
    Ok(())
}

#[test]
#[ignore = "requires native GPU adapter"]
// Run explicitly with:
// cargo test -q native_outer_shadow_artifact_matches_independent_anchor_oracle -- --ignored --nocapture
fn native_outer_shadow_artifact_matches_independent_anchor_oracle() -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native GPU initialized");
    for opacity in [0.0_f32, 0.5, 1.0] {
        for (path, graph) in [
            ("artifact", artifact_outer_shadow_graph(opacity)?),
            ("legacy", legacy_outer_shadow_graph(opacity)?),
        ] {
            let pixels = render(graph, gpu)?;
            let measured = if path == "legacy" {
                // Legacy applies opacity before the f16 shadow target, while
                // the artifact root group applies it after an RGBA8 target.
                // A one-code alpha rounding difference amplifies straight RGB;
                // additionally check Legacy in the actual premultiplied
                // compositing domain, with the same one-code tolerance.
                let mut premultiplied = pixels.clone();
                for pixel in premultiplied.chunks_exact_mut(4) {
                    for channel in 0..3 {
                        pixel[channel] = (f32::from(pixel[channel]) * f32::from(pixel[3])
                            / 255.0)
                            .round() as u8;
                    }
                }
                premultiplied
            } else {
                pixels.clone()
            };
            let mut expected = outer_shadow_anchor_oracle(opacity);
            if path == "legacy" {
                for channel in 0..3 {
                    expected[channel] = (f32::from(expected[channel]) * f32::from(expected[3])
                        / 255.0)
                        .round() as u8;
                }
            }
            assert_pixel_near(
                &measured,
                7,
                30,
                expected,
                1,
                &format!(
                    "{path} outer-shadow independent linear/quantized oracle opacity={opacity} on {}",
                    gpu.label()
                ),
            )?;
            assert_pixel_near(&pixels, 2, 30, [0, 0, 0, 0], 0, "outer-shadow outside")?;
        }
    }
    Ok(())
}

#[test]
fn shadow_oracle_accounts_for_linear_premultiplied_readback() {
    assert_eq!(outer_shadow_anchor_oracle(0.0), [0, 0, 0, 0]);
    assert_eq!(outer_shadow_anchor_oracle(0.5), [10, 33, 152, 77]);
    assert_eq!(outer_shadow_anchor_oracle(1.0), [8, 33, 153, 153]);
}

#[test]
fn root_group_fixture_requires_current_exact_decoration_identity() {
    let mut artifact = root_group_overlap_artifact(0.5);
    assert!(retained_root_effect_witness(&artifact).is_ok());
    let PaintOp::DrawRect(rect) = &mut artifact.ops[0] else {
        panic!("rectangle");
    };
    rect.params.fill_color = [0.0, 1.0, 0.0, 1.0];
    assert!(
        retained_root_effect_witness(&artifact).is_err(),
        "changing payload without updating its identity must stay rejected"
    );
}

#[test]
fn root_group_oracle_quantizes_each_rgba8_target_before_unpremultiplying() {
    // [0.12,0.9,0.2,0.7] writes [21,161,36,178] into the layer;
    // opacity 0.4 writes [8,64,14,71] into the receiver, giving this
    // straight readback. Unquantized RGB would incorrectly give red=31.
    assert_eq!(root_group_raster_readback(premultiply([0.12,0.9,0.2,0.7]), 0.4),
        [29,230,50,71]);
}
