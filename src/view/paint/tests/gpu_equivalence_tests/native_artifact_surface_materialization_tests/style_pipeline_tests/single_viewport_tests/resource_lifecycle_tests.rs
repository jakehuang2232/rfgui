use super::*;
use crate::view::base_component::Image;
use crate::view::image_resource::{
    acquire_image_resource, replace_ready_image_for_test, set_image_error_for_test,
    set_image_loading_for_test,
};
use crate::view::sampled_texture::{ImageAssetId, SampledTextureId};
use crate::view::{ImageSampling, ImageSource};
use std::sync::Arc;

mod pressure_tests;
mod svg_resource_tests;
mod slot_content_tests;
mod source_transition_tests;

struct ResourceScene {
    _handle: crate::view::image_resource::ImageHandle,
    asset: ImageAssetId,
}

fn install_resource_scene(viewport: &mut Viewport) -> ResourceScene {
    let source = ImageSource::Rgba {
        width: 2,
        height: 2,
        pixels: Arc::from([255, 0, 0, 255].repeat(4)),
    };
    let handle = acquire_image_resource(&source);
    let asset = handle.asset_id();
    let mut arena = NodeArena::new();
    let mut root = Element::new_with_id(0xc4_7001, 0.0, 0.0, 20.0, 16.0);
    let mut style = sized_grid(20.0, 16.0);
    style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::rgba(0, 0, 0, 0)),
    );
    style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(ScrollDirection::Vertical),
    );
    root.apply_style(style);
    let root = commit_element(&mut arena, Box::new(root));
    let mut image = Image::new_with_id(0xc4_7002, source);
    let mut style = sized_grid(20.0, 32.0);
    style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::rgb(0, 255, 0)),
    );
    image.apply_style(style);
    image.set_fit(crate::view::ImageFit::Fill);
    image.set_sampling(ImageSampling::Nearest);
    commit_child(&mut arena, root, Box::new(image));
    viewport.install_single_viewport_scene_for_test(arena, root);
    ResourceScene {
        _handle: handle,
        asset,
    }
}

fn begin_resource_frame(viewport: &mut Viewport, gpu: &NativeGpu, dpr: u32) -> Result<(), String> {
    viewport.begin_offscreen_test_frame(
        gpu.device.clone(),
        gpu.queue.clone(),
        WIDTH * dpr,
        HEIGHT * dpr,
        FORMAT,
    )?;
    // Frame acquisition resets scale; restore it before production layout.
    viewport.set_scale_factor(dpr as f32);
    assert_eq!(viewport.scale_factor(), dpr as f32);
    assert_eq!(viewport.logical_size(), (WIDTH as f32, HEIGHT as f32));
    Ok(())
}

fn check_resource_pixels(
    pixels: &[u8],
    dpr: u32,
    expected: [u8; 4],
    label: &str,
) -> Result<(), String> {
    // Uniform image covers [0,20)x[0,32), clipped to 20x16. Loading/error reveal its opaque green background.
    // All expectations are absolute; neither renderer is oracle.
    for (x, y, color) in [
        (4, 4, expected),
        (18, 14, expected),
        (22, 4, [0; 4]),
        (4, 18, [0; 4]),
    ] {
        let i = ((y * dpr * WIDTH * dpr + x * dpr) * 4) as usize;
        let actual: [u8; 4] = pixels[i..i + 4].try_into().unwrap();
        if actual
            .into_iter()
            .zip(color)
            .any(|(a, e)| a.abs_diff(e) > 1)
        {
            return Err(format!(
                "{label} DPR {dpr} @({x},{y}) {actual:?} expected {color:?}"
            ));
        }
    }
    Ok(())
}

fn check_resource_retention(
    viewport: &Viewport,
    observed: &crate::view::viewport::SingleViewportFrameObservation,
    dpr: u32,
    action: RetainedSurfaceCompileAction,
) {
    assert!(observed.artifact_selected);
    assert_eq!(observed.actions, [action]);
    assert_eq!(observed.texture_bytes, 20 * 32 * 12 * u64::from(dpr * dpr));
    assert_eq!(observed.color_targets.len(), 1);
    let (key, desc) = &observed.color_targets[0];
    assert_eq!((desc.width(), desc.height()), (20 * dpr, 32 * dpr));
    assert!(viewport.has_compatible_persistent_render_target_pair(*key, desc));
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_single_viewport_resource_completion_and_generation_invalidation() -> Result<(), String> {
    run_resource_transitions(ViewportPaintRendererMode::RetainedAuto)
}
#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_single_viewport_legacy_resource_completion_and_generation_pixels() -> Result<(), String> {
    run_resource_transitions(ViewportPaintRendererMode::Legacy)
}

fn run_resource_transitions(mode: ViewportPaintRendererMode) -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native hardware graphics context");
    for dpr in [1, 2] {
        let mut viewport = Viewport::new();
        viewport.set_paint_renderer_mode(mode);
        let scene = install_resource_scene(&mut viewport);
        set_image_loading_for_test(scene.asset);
        // Each pair is transition then unchanged. A worker publishes registry
        // state only; no component mutation or manual dirty clearing occurs.
        // This controls completion timing, not image decoding or host wakeups.
        let final_pixels: Arc<[u8]> = Arc::from([255, 0, 0, 255].repeat(6));
        for frame in 0..12 {
            let asset = scene.asset;
            if frame == 2 || frame == 4 || frame == 6 || frame == 8 || frame == 10 {
                let final_pixels = final_pixels.clone();
                std::thread::spawn(move || match frame {
                    2 => {
                        replace_ready_image_for_test(
                            asset,
                            2,
                            2,
                            Arc::from([255, 0, 0, 255].repeat(4)),
                        );
                    }
                    4 => {
                        replace_ready_image_for_test(
                            asset,
                            2,
                            2,
                            Arc::from([0, 0, 255, 255].repeat(4)),
                        );
                    }
                    6 => set_image_error_for_test(asset, "controlled completion error"),
                    8 | 10 => {
                        // Frame 10 republishes the SAME Arc, dimensions and
                        // bytes as frame 8. Generation is the only difference.
                        if frame == 10 {
                            let Some(crate::view::image_resource::ImageSnapshot::Ready(previous)) =
                                crate::view::image_resource::snapshot_image(asset)
                            else {
                                panic!("previous ready image");
                            };
                            assert!(Arc::ptr_eq(&previous.pixels, &final_pixels));
                            assert_eq!((previous.width, previous.height), (3, 2));
                            let generation =
                                replace_ready_image_for_test(asset, 3, 2, final_pixels);
                            assert_ne!(generation, previous.generation);
                        } else {
                            replace_ready_image_for_test(asset, 3, 2, final_pixels);
                        }
                    }
                    _ => unreachable!(),
                })
                .join()
                .expect("resource completion worker");
            }
            begin_resource_frame(&mut viewport, gpu, dpr)?;
            let observed = viewport.render_single_viewport_scene_for_test()?;
            let pixels =
                read_submitted_texture(&observed.texture, gpu, [WIDTH * dpr, HEIGHT * dpr])?;
            let expected = match frame {
                0 | 1 | 6 | 7 => [0, 255, 0, 255],
                4 | 5 => [0, 0, 255, 255],
                _ => [255, 0, 0, 255],
            };
            check_resource_pixels(&pixels, dpr, expected, &format!("{mode:?} frame {frame}"))?;
            let (_, uploads, _) =
                viewport.sampled_cache_observation_for_test(SampledTextureId::Image(scene.asset));
            assert_eq!(
                uploads,
                match frame {
                    0 | 1 => 0,
                    2 | 3 => 1,
                    4..=7 => 2,
                    8 | 9 => 3,
                    _ => 4,
                },
                "upload only on first ready/new generation/new extent"
            );
            if mode == ViewportPaintRendererMode::RetainedAuto {
                check_resource_retention(
                    &viewport,
                    &observed,
                    dpr,
                    if frame % 2 == 0 {
                        RetainedSurfaceCompileAction::Reraster
                    } else {
                        RetainedSurfaceCompileAction::Reuse
                    },
                );
            } else {
                assert!(observed.legacy_selected);
            }
        }
    }
    eprintln!("resource completion {mode:?} passed on {}", gpu.label());
    Ok(())
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_single_viewport_completion_after_freeze_waits_until_next_frame() -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native hardware graphics context");
    for mode in [
        ViewportPaintRendererMode::RetainedAuto,
        ViewportPaintRendererMode::Legacy,
    ] {
        for dpr in [1, 2] {
            let mut viewport = Viewport::new();
            viewport.set_paint_renderer_mode(mode);
            let scene = install_resource_scene(&mut viewport);
            for frame in 0..6 {
                begin_resource_frame(&mut viewport, gpu, dpr)?;
                let asset = scene.asset;
                let observed = if frame == 1 || frame == 3 {
                    viewport.render_single_viewport_after_freeze_for_test(move |_| {
                        // Publish from another thread AFTER freeze but BEFORE
                        // metadata/full recording. Neither may see newer truth.
                        std::thread::spawn(move || {
                            if frame == 1 {
                                replace_ready_image_for_test(
                                    asset,
                                    2,
                                    2,
                                    Arc::from([0, 0, 255, 255].repeat(4)),
                                );
                            } else {
                                set_image_error_for_test(asset, "late completion error");
                            }
                        })
                        .join()
                        .expect("late resource completion worker");
                    })?
                } else {
                    viewport.render_single_viewport_scene_for_test()?
                };
                let pixels =
                    read_submitted_texture(&observed.texture, gpu, [WIDTH * dpr, HEIGHT * dpr])?;
                let expected = if frame < 2 {
                    [255, 0, 0, 255]
                } else if frame < 4 {
                    [0, 0, 255, 255]
                } else {
                    [0, 255, 0, 255]
                };
                check_resource_pixels(
                    &pixels,
                    dpr,
                    expected,
                    &format!("post-freeze {mode:?} frame {frame}"),
                )?;
                let (_, uploads, _) =
                    viewport.sampled_cache_observation_for_test(SampledTextureId::Image(asset));
                assert_eq!(
                    uploads,
                    if frame < 2 { 1 } else { 2 },
                    "late completion cannot leak a newer upload into this frame"
                );
                if mode == ViewportPaintRendererMode::RetainedAuto {
                    check_resource_retention(
                        &viewport,
                        &observed,
                        dpr,
                        if frame % 2 == 0 {
                            RetainedSurfaceCompileAction::Reraster
                        } else {
                            RetainedSurfaceCompileAction::Reuse
                        },
                    );
                } else {
                    assert!(observed.legacy_selected);
                }
            }
        }
    }
    eprintln!("post-freeze completion passed on {}", gpu.label());
    Ok(())
}

mod wrapper_effect_tests;
