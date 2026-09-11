use super::super::style_pipeline_tests::read_submitted_texture;
use super::*;
use crate::view::viewport::ViewportPaintRendererMode;

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_single_viewport_planning_corpus_artifact_selection_and_reuse() -> Result<(), String> {
    run(ViewportPaintRendererMode::RetainedAuto)
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_single_viewport_planning_corpus_legacy_pixels() -> Result<(), String> {
    run(ViewportPaintRendererMode::Legacy)
}

fn run(mode: ViewportPaintRendererMode) -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native GPU");
    for scene in Scene::ALL {
        for dpr in [1_u32, 2] {
            // No prior layout or recorder run: the production frame must
            // derive everything from the installed styles in this Viewport.
            let fixture = unlaid_out_fixture(scene);
            let mut viewport = Viewport::new();
            viewport.set_paint_renderer_mode(mode);
            viewport.install_single_viewport_forest_for_test(fixture.arena, fixture.roots);
            let mut first_targets = None;
            for frame in 0..3 {
                eprintln!("single Viewport corpus {scene:?} {mode:?} DPR={dpr} frame={frame}");
                viewport.begin_offscreen_test_frame(
                    gpu.device.clone(),
                    gpu.queue.clone(),
                    EXTENT[0] * dpr,
                    EXTENT[1] * dpr,
                    FORMAT,
                )?;
                viewport.set_scale_factor(dpr as f32);
                assert_eq!(viewport.scale_factor(), dpr as f32);
                assert_eq!(
                    viewport.logical_size(),
                    (EXTENT[0] as f32, EXTENT[1] as f32)
                );
                let observed = viewport.render_single_viewport_scene_for_test()?;
                let pixels =
                    read_submitted_texture(&observed.texture, gpu, EXTENT.map(|v| v * dpr))?;
                for (name, [x, y], expected) in &fixture.probes {
                    let at = ((y * dpr * EXTENT[0] * dpr + x * dpr) * 4) as usize;
                    assert!(
                        pixels[at..at + 4]
                            .iter()
                            .zip(expected)
                            .all(|(a, b)| a.abs_diff(*b) <= 1),
                        "{scene:?} {mode:?} DPR={dpr} frame={frame} {name}: actual={:?} expected={expected:?}",
                        &pixels[at..at + 4]
                    );
                }
                assert_eq!(observed.frame_number, frame + 1);
                if mode == ViewportPaintRendererMode::Legacy {
                    assert!(observed.legacy_selected);
                    continue;
                }
                assert!(
                    observed.artifact_selected,
                    "the valid corpus must select Artifact"
                );
                assert!(
                    !observed.actions.is_empty(),
                    "must use the generic retained pool executor"
                );
                assert_eq!(observed.actions.len(), observed.color_targets.len());
                assert!(
                    observed.actions.iter().all(|action| *action
                        == if frame == 0 {
                            RetainedSurfaceCompileAction::Reraster
                        } else {
                            RetainedSurfaceCompileAction::Reuse
                        }),
                    "{scene:?} frame={frame}: {:?}",
                    observed.actions
                );
                let mut bytes = 0;
                for (key, desc) in &observed.color_targets {
                    assert!(viewport.has_compatible_persistent_render_target_pair(*key, desc));
                    bytes += u64::from(desc.width()) * u64::from(desc.height()) * 12;
                }
                assert_eq!(
                    observed.texture_bytes, bytes,
                    "RGBA8 + Depth32FloatStencil8 for every real pair"
                );
                if let Some(first) = &first_targets {
                    assert_eq!(
                        &observed.color_targets, first,
                        "warm frames preserve every target identity and descriptor"
                    );
                } else {
                    first_targets = Some(observed.color_targets);
                }
            }
        }
    }
    Ok(())
}
