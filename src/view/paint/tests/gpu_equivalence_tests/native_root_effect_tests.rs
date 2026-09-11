use super::*;

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
                        pixel[channel] =
                            (f32::from(pixel[channel]) * f32::from(pixel[3]) / 255.0).round() as u8;
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
    let valid = |artifact: &PaintArtifact| {
        let (mut graph, ctx, _) = graph_prelude();
        try_compile_artifact(artifact, &mut graph, ctx).is_ok()
    };
    assert!(valid(&artifact));
    let PaintOp::DrawRect(rect) = &mut artifact.ops[0] else {
        panic!("rectangle");
    };
    rect.params.fill_color = [0.0, 1.0, 0.0, 1.0];
    assert!(
        !valid(&artifact),
        "changing payload without updating its identity must stay rejected"
    );
}

#[test]
fn root_group_oracle_quantizes_each_rgba8_target_before_unpremultiplying() {
    // [0.12,0.9,0.2,0.7] writes [21,161,36,178] into the layer;
    // opacity 0.4 writes [8,64,14,71] into the receiver, giving this
    // straight readback. Unquantized RGB would incorrectly give red=31.
    assert_eq!(
        root_group_raster_readback(premultiply([0.12, 0.9, 0.2, 0.7]), 0.4),
        [29, 230, 50, 71]
    );
}
