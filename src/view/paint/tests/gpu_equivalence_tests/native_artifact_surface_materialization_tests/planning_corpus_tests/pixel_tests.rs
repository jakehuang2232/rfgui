use super::*;

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_planning_corpus_absolute_coordinates_and_reuse() -> Result<(), String> {
    let _text_cleanup = crate::view::paint::tests::gpu_equivalence_tests::native_artifact_scroll_content_tests::NativeArtifactTextThreadCacheCleanup;
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().unwrap();
    for scene in Scene::ALL {
        for dpr in [1_u32, 2] {
            for offset in [[0.0, 0.0], [7.0, 5.0]] {
                eprintln!("planning pixels {scene:?}, dpr={dpr}, offset={offset:?}");
                let fixture = fixture(scene);
                let artifact = record(&fixture);
                let mut viewport = Viewport::new();
                drop(fixture.arena);
                for frame in 0..2 {
                    let (mut graph, mut ctx, target) = transformed_graph_prelude_with_size(
                        dpr as f32,
                        None,
                        EXTENT.map(|v| v * dpr),
                    );
                    ctx.translate_paint_offset(offset[0], offset[1]);
                    let plan = prepare(artifact.clone(), dpr as f32, offset);
                    let plan = seal_prepared_artifact_surface_frame(plan)
                        .map_err(|e| format!("{scene:?} seal: {e:?}"))?;
                    let owner = viewport.begin_retained_surface_frame_stage().unwrap();
                    let _ = take_last_production_actions_for_test();
                    emit_prepared_artifact_surface_frame_from_pool(
                        &mut viewport,
                        Some(owner),
                        plan,
                        &mut graph,
                        ctx,
                    );
                    let actions = take_last_production_actions_for_test();
                    add_present(&mut graph, &target)?;
                    let pixels = render_on_viewport_with_size(
                        graph,
                        gpu,
                        &mut viewport,
                        dpr as f32,
                        FORMAT,
                        EXTENT.map(|v| v * dpr),
                    )?;
                    for (name, [x, y], expected) in &fixture.probes {
                        let x = (*x as f32 + offset[0]) as u32 * dpr;
                        let y = (*y as f32 + offset[1]) as u32 * dpr;
                        assert!(x < EXTENT[0] * dpr && y < EXTENT[1] * dpr);
                        let at = ((y * EXTENT[0] * dpr + x) * 4) as usize;
                        assert!(
                            pixels[at..at + 4]
                                .iter()
                                .zip(expected)
                                .all(|(a, e)| a.abs_diff(*e) <= 1),
                            "{scene:?} {name} DPR={dpr} offset={offset:?} frame={frame}: at ({x},{y}) actual={:?}, expected={expected:?}",
                            &pixels[at..at + 4]
                        );
                    }
                    assert!(!actions.is_empty());
                    assert!(
                        actions.iter().all(|a| *a
                            == if frame == 0 {
                                RetainedSurfaceCompileAction::Reraster
                            } else {
                                RetainedSurfaceCompileAction::Reuse
                            }),
                        "{scene:?}: {actions:?}"
                    );
                    assert!(
                        viewport.finish_retained_surface_transaction_for_frame(Some(owner), true)
                    );
                }
            }
        }
    }
    Ok(())
}
