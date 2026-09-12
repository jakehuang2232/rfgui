use super::*;
use crate::view::sampled_texture::{SampledTextureAlphaMode, SampledTextureUpload};

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_single_viewport_real_sampled_pressure_preserves_raster_and_recovers_source()
-> Result<(), String> {
    run_pressure(ViewportPaintRendererMode::RetainedAuto)
}
#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_single_viewport_legacy_real_sampled_pressure_pixels() -> Result<(), String> {
    run_pressure(ViewportPaintRendererMode::Legacy)
}

fn run_pressure(mode: ViewportPaintRendererMode) -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native hardware graphics context");
    let (threshold, evict_to, _) = Viewport::sampled_cache_policy_for_test();
    const SIDE: u32 = 2048;
    let bytes = u64::from(SIDE) * u64::from(SIDE) * 4;
    let count = threshold / bytes + 1;
    let pixels: Arc<[u8]> = Arc::from(vec![0; bytes as usize]);
    // Real uploads cross the production budget. No byte-size/age mutation or
    // direct cache eviction is used; ensure_sampled_texture runs the policy.
    let pressure: Vec<_> = (0..count)
        .map(|i| SampledTextureUpload {
            id: SampledTextureId::Image(ImageAssetId::for_test(0xf000_0000 + i)),
            generation: 1,
            width: SIDE,
            height: SIDE,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            alpha_mode: SampledTextureAlphaMode::Straight,
            pixels: pixels.clone(),
            sampling: ImageSampling::Nearest,
        })
        .collect();
    for dpr in [1, 2] {
        let mut viewport = Viewport::new();
        viewport.set_paint_renderer_mode(mode);
        let scene = install_resource_scene(&mut viewport);
        let source_id = SampledTextureId::Image(scene.asset);
        let mut initial_source = None;
        let mut target = None;
        for frame in 0..4 {
            begin_resource_frame(&mut viewport, gpu, dpr)?;
            let observed = if frame == 1 {
                let pressure = pressure.clone();
                viewport.render_single_viewport_after_freeze_for_test(move |viewport| {
                    for upload in &pressure {
                        assert!(viewport.ensure_sampled_texture(upload));
                    }
                    assert_eq!(
                        viewport.sampled_cache_observation_for_test(source_id).0,
                        None,
                        "inactive source evicted by real pressure"
                    );
                    for upload in &pressure {
                        assert!(
                            viewport.sampled_texture_view(upload.id).is_some(),
                            "current-frame uploads pinned"
                        );
                    }
                    assert_eq!(
                        viewport.sampled_cache_observation_for_test(source_id).2,
                        count * bytes
                    );
                    assert!(
                        count * bytes > threshold,
                        "pinned active working set may exceed budget"
                    );
                })?
            } else {
                viewport.render_single_viewport_scene_for_test()?
            };
            let readback =
                read_submitted_texture(&observed.texture, gpu, [WIDTH * dpr, HEIGHT * dpr])?;
            check_resource_pixels(
                &readback,
                dpr,
                [255, 0, 0, 255],
                &format!("pressure {mode:?} frame {frame}"),
            )?;
            let (source, uploads, total) = viewport.sampled_cache_observation_for_test(source_id);
            if frame == 0 {
                initial_source = source;
                assert!(source.is_some());
            }
            if mode == ViewportPaintRendererMode::RetainedAuto {
                check_resource_retention(
                    &viewport,
                    &observed,
                    dpr,
                    if frame == 0 || frame == 2 {
                        RetainedSurfaceCompileAction::Reraster
                    } else {
                        RetainedSurfaceCompileAction::Reuse
                    },
                );
                if frame == 1 {
                    assert!(
                        source.is_none(),
                        "raster reuse does not need evicted source upload"
                    );
                    assert_eq!(uploads, count + 1);
                    let (key, desc) = &observed.color_targets[0];
                    assert!(viewport.release_persistent_render_target_pair(*key));
                    assert!(!viewport.has_compatible_persistent_render_target(*key, desc));
                }
                if let Some(first) = &target {
                    assert_eq!(&observed.color_targets[0], first);
                } else {
                    target = Some(observed.color_targets[0].clone());
                }
                if frame >= 2 {
                    assert_eq!(
                        source, initial_source,
                        "restore identical source generation after eviction"
                    );
                    assert_eq!(
                        uploads,
                        count + 2,
                        "one recovery upload, none on warm reuse"
                    );
                    assert!(
                        total <= evict_to,
                        "old pressure textures become eligible next frame"
                    );
                }
            } else {
                assert!(observed.legacy_selected);
                assert_eq!(source, initial_source);
                if frame >= 1 {
                    assert_eq!(uploads, count + 2);
                }
                // The current frame pins all uploads in frame 1, including the
                // reloaded source. In frame 2 old filler uploads become evictable.
                if frame >= 2 {
                    assert!(total <= evict_to);
                }
            }
        }
    }
    eprintln!("real sampled pressure {mode:?} passed on {}", gpu.label());
    Ok(())
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_single_viewport_sampled_idle_eviction_and_active_legacy_protection() -> Result<(), String>
{
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native hardware graphics context");
    let (_, _, stale_frames) = Viewport::sampled_cache_policy_for_test();
    let sentinel = SampledTextureUpload {
        id: SampledTextureId::Image(ImageAssetId::for_test(0xf100_0000)),
        generation: 1,
        width: 1,
        height: 1,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        alpha_mode: SampledTextureAlphaMode::Straight,
        pixels: Arc::from([0; 4]),
        sampling: ImageSampling::Nearest,
    };
    for mode in [
        ViewportPaintRendererMode::RetainedAuto,
        ViewportPaintRendererMode::Legacy,
    ] {
        for dpr in [1, 2] {
            let mut viewport = Viewport::new();
            viewport.set_paint_renderer_mode(mode);
            let scene = install_resource_scene(&mut viewport);
            let id = SampledTextureId::Image(scene.asset);
            let expire = stale_frames + 1;
            // Advance genuine submitted frames, not a fabricated cache age.
            // Equality to the stale threshold is retained; the next frame is
            // evicted if unsampled. Legacy samples each frame and stays pinned.
            for frame in 0..=expire + 2 {
                begin_resource_frame(&mut viewport, gpu, dpr)?;
                let observed = if frame == stale_frames || frame == expire {
                    let sentinel = sentinel.clone();
                    viewport.render_single_viewport_after_freeze_for_test(move |viewport| {
                        assert!(viewport.ensure_sampled_texture(&sentinel));
                        let present = viewport.sampled_texture_view(id).is_some();
                        assert_eq!(
                            present,
                            frame == stale_frames || mode == ViewportPaintRendererMode::Legacy
                        );
                    })?
                } else {
                    viewport.render_single_viewport_scene_for_test()?
                };
                let pixels =
                    read_submitted_texture(&observed.texture, gpu, [WIDTH * dpr, HEIGHT * dpr])?;
                check_resource_pixels(
                    &pixels,
                    dpr,
                    [255, 0, 0, 255],
                    &format!("idle {mode:?} frame {frame}"),
                )?;
                if mode == ViewportPaintRendererMode::RetainedAuto {
                    check_resource_retention(
                        &viewport,
                        &observed,
                        dpr,
                        if frame == 0 || frame == expire + 1 {
                            RetainedSurfaceCompileAction::Reraster
                        } else {
                            RetainedSurfaceCompileAction::Reuse
                        },
                    );
                    if frame == expire {
                        assert!(viewport.sampled_texture_view(id).is_none());
                        assert!(
                            viewport
                                .release_persistent_render_target_pair(observed.color_targets[0].0)
                        );
                    }
                } else {
                    assert!(observed.legacy_selected);
                }
                let (source, uploads, _) = viewport.sampled_cache_observation_for_test(id);
                if frame > expire {
                    assert!(source.is_some());
                    assert_eq!(
                        uploads,
                        if mode == ViewportPaintRendererMode::Legacy {
                            2
                        } else {
                            3
                        }
                    );
                }
            }
        }
    }
    eprintln!("sampled idle policy passed on {}", gpu.label());
    Ok(())
}
