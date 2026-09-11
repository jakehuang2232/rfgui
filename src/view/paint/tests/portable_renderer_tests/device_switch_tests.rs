use super::*;

pub(super) async fn run(
    instance: &wgpu::Instance,
    first: &Gpu,
) -> Result<(usize, wgpu::AdapterInfo), String> {
    // WebGPU consumes an adapter on requestDevice; acquire a fresh adapter
    // using the same hardware validation as the initial device.
    let adapter = request_adapter(instance).await?;
    let second = request_gpu(&adapter).await?;
    assert_ne!(
        first.device, second.device,
        "must exercise distinct devices"
    );
    let mut frames = 0;
    // Initialize fresh Viewports on A, B, then A again, including the resource
    // release that production create_surface performs. This covers initial
    // device installation, not alternating already initialized Viewports on
    // different devices or interleaving their prepare/execute phases.
    for (step, gpu) in [first, &second, first].into_iter().enumerate() {
        for mode in [
            ViewportPaintRendererMode::RetainedAuto,
            ViewportPaintRendererMode::Legacy,
        ] {
            for scene in [
                Scene::DecoratedInlineClip,
                Scene::ShadowClip,
                Scene::DeferredOverlay,
            ] {
                for dpr in [1, 2] {
                    let fixture = unlaid_out_fixture(scene);
                    let mut viewport = Viewport::new();
                    viewport.set_paint_renderer_mode(mode);
                    viewport.install_single_viewport_forest_for_test(fixture.arena, fixture.roots);
                    let label = format!("device switch {step} {mode:?} {scene:?} DPR={dpr}");
                    progress(&label);
                    begin(&mut viewport, gpu, EXTENT, dpr)?;
                    let observed = viewport
                        .render_single_viewport_scene_for_test()
                        .map_err(|error| format!("{label}: {error}"))?;
                    let pixels =
                        read_pixels(gpu, &observed.texture, EXTENT.map(|v| v * dpr)).await?;
                    for (name, at, expected) in &fixture.probes {
                        probe(&pixels, EXTENT, dpr, name, *at, *expected)
                            .map_err(|error| format!("{label}: {error}"))?;
                    }
                    if mode == ViewportPaintRendererMode::RetainedAuto {
                        assert!(observed.artifact_selected);
                        assert!(!observed.actions.is_empty());
                        assert!(
                            observed
                                .actions
                                .iter()
                                .all(|a| *a == RetainedSurfaceCompileAction::Reraster)
                        );
                        for (key, desc) in &observed.color_targets {
                            assert!(
                                viewport.has_compatible_persistent_render_target_pair(*key, desc)
                            );
                        }
                    } else {
                        assert!(observed.legacy_selected);
                    }
                    frames += 1;
                }
            }
        }
    }
    assert_eq!(frames, 36);
    Ok((frames, adapter.get_info()))
}
