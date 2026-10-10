//! Production redraw admission, with acquisition deferred until surface execution.
use rfgui::style::{Color, Length, Transition, TransitionProperty};
use rfgui::time::{Duration, Instant};
use rfgui::ui::{RsxNode, rsx};
use rfgui::view::viewport::ViewportPaintRendererMode;
use rfgui::view::{Element, TextArea, Viewport};

#[path = "../../lib/rfgui-components/tests/retained_controls/gpu.rs"]
mod gpu;

fn redraw(
    gpu: &gpu::Gpu,
    viewport: &mut Viewport,
    root: &RsxNode,
    size: [u32; 2],
    dpr: f32,
    now: Instant,
) -> Result<Option<Vec<u8>>, String> {
    viewport
        .render_rsx_redraw_offscreen_for_test(
            root,
            gpu.device.clone(),
            gpu.queue.clone(),
            size,
            dpr,
            now,
        )?
        .map(|texture| gpu.read(&texture, size))
        .transpose()
}

const RED: [u8; 4] = [255, 0, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];

/// The pixel at logical point `[x, y]` of a frame rendered at `dpr`.
fn pixel_at(pixels: &[u8], size: [u32; 2], dpr: u32, [x, y]: [u32; 2]) -> [u8; 4] {
    let at = ((y * dpr * size[0] + x * dpr) * 4) as usize;
    pixels[at..at + 4].try_into().unwrap()
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn late_surface_acquisition_aborts_without_submit_and_retries_the_same_renderer()
-> Result<(), String> {
    let gpu = gpu::Gpu::new()?;
    for dpr in [1, 2] {
        for mode in [
            ViewportPaintRendererMode::Legacy,
            ViewportPaintRendererMode::RetainedAuto,
        ] {
            let mut viewport = Viewport::new();
            viewport.set_paint_renderer_mode(mode);
            let root = rsx! { <Element style={{
                width: Length::px(40.), height: Length::px(40.),
                background_color: Color::hex("#ff0000"),
            }} /> };
            let size = [64 * dpr, 64 * dpr];
            let now = Instant::now();
            viewport.fail_next_surface_acquisition_for_test();
            let (failed, work) = rfgui::ui::profile_ui_work(|| {
                redraw(&gpu, &mut viewport, &root, size, dpr as f32, now)
            });
            assert!(failed?.is_none());
            assert!(
                work.graphics_passes_recorded > 0,
                "offscreen passes precede acquisition"
            );
            assert_eq!(viewport.frame_acquisition_count_for_test(), 1);
            assert_eq!(
                viewport.renderer_performance_sample().2.0,
                0,
                "aborted encoder never submits"
            );

            // Admission retains the obligation, and the Auto circuit breaker
            // stays unlatched. The helper also verifies actual selected authority.
            let recovered = redraw(&gpu, &mut viewport, &root, size, dpr as f32, now)?.unwrap();
            assert_eq!(viewport.frame_acquisition_count_for_test(), 2);
            assert_eq!(viewport.renderer_performance_sample().2.0, 1);
            assert_eq!(
                pixel_at(&recovered, size, dpr, [20, 20]),
                RED,
                "recovered frame {mode:?} DPR {dpr}"
            );
            assert!(redraw(&gpu, &mut viewport, &root, size, dpr as f32, now)?.is_none());
            assert_eq!(viewport.frame_acquisition_count_for_test(), 2);
        }
    }
    Ok(())
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn unchanged_redraws_do_not_acquire_or_submit_and_changes_preserve_pixels() -> Result<(), String> {
    let gpu = gpu::Gpu::new()?;
    for dpr in [1, 2] {
        for mode in [
            ViewportPaintRendererMode::Legacy,
            ViewportPaintRendererMode::RetainedAuto,
        ] {
            let mut viewport = Viewport::new();
            viewport.set_paint_renderer_mode(mode);
            viewport.set_clear_color(Box::new(Color::rgba(0, 0, 0, 0)));
            let root = rsx! { <Element style={{
                width: Length::px(40.), height: Length::px(40.),
                background_color: Color::hex("#ff0000"),
                hover: { background_color: Color::hex("#0000ff") },
            }} /> };
            let now = Instant::now();
            let mut size = [64 * dpr, 64 * dpr];
            let first = redraw(&gpu, &mut viewport, &root, size, dpr as f32, now)?.unwrap();
            assert_eq!(
                pixel_at(&first, size, dpr, [20, 20]),
                RED,
                "{mode:?} DPR {dpr}"
            );
            let before = viewport.renderer_performance_sample().2;
            let acquires = viewport.frame_acquisition_count_for_test();
            for _ in 0..20 {
                viewport.request_redraw();
                viewport.drain_platform_requests();
                let (frame, work) = rfgui::ui::profile_ui_work(|| {
                    redraw(&gpu, &mut viewport, &root, size, dpr as f32, now)
                });
                assert!(frame?.is_none());
                assert_eq!(
                    work.animation_request_observations, 1,
                    "one scheduling observation per clean root per redraw attempt"
                );
            }
            assert_eq!(viewport.renderer_performance_sample().2, before);
            assert_eq!(viewport.frame_acquisition_count_for_test(), acquires);

            viewport.set_pointer_position_viewport(10., 10.);
            viewport.dispatch_pointer_move_event();
            let hover = redraw(&gpu, &mut viewport, &root, size, dpr as f32, now)?.unwrap();
            assert_eq!(
                pixel_at(&hover, size, dpr, [20, 20]),
                BLUE,
                "{mode:?} DPR {dpr}"
            );
            viewport.clear_pointer_position_viewport();
            let leave = redraw(&gpu, &mut viewport, &root, size, dpr as f32, now)?.unwrap();
            assert_eq!(first, leave);
            size = [80 * dpr, 80 * dpr];
            let resize = redraw(&gpu, &mut viewport, &root, size, dpr as f32, now)?.unwrap();
            assert!(redraw(&gpu, &mut viewport, &root, size, dpr as f32, now)?.is_none());
            viewport.set_clear_color(Box::new(Color::rgb(0, 255, 0)));
            let clear = redraw(&gpu, &mut viewport, &root, size, dpr as f32, now)?.unwrap();
            assert_ne!(resize, clear);
            assert_eq!(
                pixel_at(&clear, size, dpr, [60, 60]),
                [0, 255, 0, 255],
                "{mode:?} DPR {dpr}"
            );
        }
    }
    Ok(())
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn caret_deadline_survives_skipped_redraws() -> Result<(), String> {
    let gpu = gpu::Gpu::new()?;
    for mode in [
        ViewportPaintRendererMode::Legacy,
        ViewportPaintRendererMode::RetainedAuto,
    ] {
        let root = rsx! { <TextArea content={"M".to_string()} /> };
        let mut viewport = Viewport::new();
        viewport.set_paint_renderer_mode(mode);
        let now = Instant::now();
        let size = [120, 60];
        redraw(&gpu, &mut viewport, &root, size, 1., now)?.unwrap();
        viewport.set_focused_node_id(Some(viewport.node_arena().roots()[0]));
        let visible = redraw(&gpu, &mut viewport, &root, size, 1., now)?.unwrap();
        for millis in [1, 100, 529] {
            assert!(
                redraw(
                    &gpu,
                    &mut viewport,
                    &root,
                    size,
                    1.,
                    now + Duration::from_millis(millis)
                )?
                .is_none()
            );
            assert_eq!(
                rfgui::ui::next_timer_deadline(),
                Some(now + Duration::from_millis(530))
            );
        }
        let hidden = redraw(
            &gpu,
            &mut viewport,
            &root,
            size,
            1.,
            now + Duration::from_millis(530),
        )?
        .unwrap();
        assert_ne!(visible, hidden, "{mode:?}");
    }
    Ok(())
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn asynchronous_resource_completion_wakes_a_clean_scene() -> Result<(), String> {
    let gpu = gpu::Gpu::new()?;
    for (index, mode) in [
        ViewportPaintRendererMode::Legacy,
        ViewportPaintRendererMode::RetainedAuto,
    ]
    .into_iter()
    .enumerate()
    {
        let root = rsx! { <Element style={{width: Length::px(40.), height: Length::px(40.)}} /> };
        let mut viewport = Viewport::new();
        viewport.set_paint_renderer_mode(mode);
        let now = Instant::now();
        let first = redraw(&gpu, &mut viewport, &root, [64, 64], 1., now)?.unwrap();
        assert!(redraw(&gpu, &mut viewport, &root, [64, 64], 1., now)?.is_none());
        // Start a real loader after the scene is clean. Completion must render
        // even without an RSX patch or an element-local dirty notification.
        let path = std::env::temp_dir().join(format!(
            "rfgui-demand-resource-{}-{index}.png",
            std::process::id()
        ));
        image::RgbaImage::from_pixel(32, 32, image::Rgba([255_u8, 0, 0, 255]))
            .save(&path)
            .unwrap();
        let resource = rfgui::view::base_component::Image::new_with_id(
            900_000 + index as u64,
            rfgui::view::ImageSource::Path(path.clone()),
        );
        let mut completed = None;
        for _ in 0..100 {
            std::thread::sleep(Duration::from_millis(5));
            completed = redraw(&gpu, &mut viewport, &root, [64, 64], 1., now)?;
            if completed.is_some() {
                break;
            }
        }
        assert_eq!(completed.as_ref(), Some(&first));
        assert!(redraw(&gpu, &mut viewport, &root, [64, 64], 1., now)?.is_none());
        drop(resource);
        std::fs::remove_file(path).unwrap();
    }
    Ok(())
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn transition_final_sample_renders_then_settles() -> Result<(), String> {
    let gpu = gpu::Gpu::new()?;
    for mode in [
        ViewportPaintRendererMode::Legacy,
        ViewportPaintRendererMode::RetainedAuto,
    ] {
        let root = rsx! { <Element style={{
            width: Length::px(40.), height: Length::px(40.),
            background_color: Color::hex("#ff0000"),
            transition: [Transition::new(TransitionProperty::BackgroundColor, 100)],
            hover: { background_color: Color::hex("#0000ff") },
        }} /> };
        let mut viewport = Viewport::new();
        viewport.set_paint_renderer_mode(mode);
        viewport.set_clear_color(Box::new(Color::rgba(0, 0, 0, 0)));
        let now = Instant::now();
        let size = [64, 64];
        let idle = redraw(&gpu, &mut viewport, &root, size, 1., now)?.unwrap();
        // Warm past cold-commit wall-clock initialization before deterministic samples.
        let now = now + Duration::from_secs(1);
        let _ = redraw(&gpu, &mut viewport, &root, size, 1., now)?;
        viewport.set_pointer_position_viewport(10., 10.);
        viewport.dispatch_pointer_move_event();
        let mut frames = vec![idle];
        for millis in [1, 25, 50, 101] {
            frames.push(
                redraw(
                    &gpu,
                    &mut viewport,
                    &root,
                    size,
                    1.,
                    now + Duration::from_millis(millis),
                )?
                .unwrap(),
            );
        }
        assert!(!viewport.is_animating());
        // The box goes from red to blue and never back toward red.
        let probes = frames
            .iter()
            .map(|pixels| pixel_at(pixels, size, 1, [20, 20]))
            .collect::<Vec<_>>();
        assert_eq!(probes.first(), Some(&RED), "{mode:?}: {probes:?}");
        assert_eq!(probes.last(), Some(&BLUE), "{mode:?}: {probes:?}");
        assert!(
            probes
                .windows(2)
                .all(|pair| pair[1][0] <= pair[0][0] && pair[1][2] >= pair[0][2]),
            "{mode:?}: {probes:?}"
        );
        if let Ok(path) = std::env::var("RFGUI_TRANSITION_PIXELS") {
            std::fs::create_dir_all(&path).unwrap();
            for (index, pixels) in frames.iter().enumerate() {
                std::fs::write(
                    std::path::Path::new(&path).join(format!("{mode:?}-{index}.rgba")),
                    pixels,
                )
                .unwrap();
            }
        }
        assert!(
            redraw(
                &gpu,
                &mut viewport,
                &root,
                size,
                1.,
                now + Duration::from_millis(200)
            )?
            .is_none()
        );
    }
    Ok(())
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn explicit_redraw_and_geometry_overlay_submit_once_on_a_clean_scene() -> Result<(), String> {
    let gpu = gpu::Gpu::new()?;
    for dpr in [1, 2] {
        for mode in [
            ViewportPaintRendererMode::Legacy,
            ViewportPaintRendererMode::RetainedAuto,
        ] {
            let root = rsx! { <Element style={{width: Length::px(40.), height: Length::px(40.), background_color: Color::hex("#ff0000")}} /> };
            let now = Instant::now();
            let size = [64 * dpr, 64 * dpr];
            let mut viewport = Viewport::new();
            viewport.set_paint_renderer_mode(mode);
            let first = redraw(&gpu, &mut viewport, &root, size, dpr as f32, now)?.unwrap();
            assert_eq!(
                pixel_at(&first, size, dpr, [20, 20]),
                RED,
                "{mode:?} DPR {dpr}"
            );
            assert!(redraw(&gpu, &mut viewport, &root, size, dpr as f32, now)?.is_none());
            let before = viewport.renderer_performance_sample().2;
            rfgui::ui::ViewportHandle.request_redraw();
            let explicit = redraw(&gpu, &mut viewport, &root, size, dpr as f32, now)?.unwrap();
            assert_eq!(first, explicit);
            assert!(redraw(&gpu, &mut viewport, &root, size, dpr as f32, now)?.is_none());
            rfgui::ui::ViewportHandle.set_debug_geometry_overlay(true);
            let overlay = redraw(&gpu, &mut viewport, &root, size, dpr as f32, now)?.unwrap();
            assert_ne!(first, overlay);
            assert!(redraw(&gpu, &mut viewport, &root, size, dpr as f32, now)?.is_none());
            rfgui::ui::ViewportHandle.set_debug_geometry_overlay(true);
            assert!(redraw(&gpu, &mut viewport, &root, size, dpr as f32, now)?.is_none());
            rfgui::ui::ViewportHandle.set_debug_geometry_overlay(false);
            let restored = redraw(&gpu, &mut viewport, &root, size, dpr as f32, now)?.unwrap();
            assert_eq!(first, restored);
            assert!(redraw(&gpu, &mut viewport, &root, size, dpr as f32, now)?.is_none());
            assert_eq!(viewport.renderer_performance_sample().2.0, before.0 + 3);
        }
    }
    Ok(())
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn hover_without_visual_styles_or_scrollbars_does_not_submit() -> Result<(), String> {
    let gpu = gpu::Gpu::new()?;
    for mode in [
        ViewportPaintRendererMode::Legacy,
        ViewportPaintRendererMode::RetainedAuto,
    ] {
        let root = rsx! { <Element style={{width: Length::px(40.), height: Length::px(40.), background_color: Color::hex("#ff0000")}} /> };
        let mut viewport = Viewport::new();
        viewport.set_paint_renderer_mode(mode);
        let now = Instant::now();
        let first = redraw(&gpu, &mut viewport, &root, [64, 64], 1., now)?.unwrap();
        assert_eq!(pixel_at(&first, [64, 64], 1, [20, 20]), RED, "{mode:?}");
        let counts = viewport.renderer_performance_sample().2;
        let acquired = viewport.frame_acquisition_count_for_test();
        for i in 0..40 {
            let x = if i % 2 == 0 { 10. } else { 50. };
            viewport.set_pointer_position_viewport(x, 10.);
            viewport.dispatch_pointer_move_event();
            assert!(
                redraw(
                    &gpu,
                    &mut viewport,
                    &root,
                    [64, 64],
                    1.,
                    now + Duration::from_millis(i * 50)
                )?
                .is_none()
            );
        }
        assert_eq!(viewport.renderer_performance_sample().2, counts);
        assert_eq!(viewport.frame_acquisition_count_for_test(), acquired);
    }
    Ok(())
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn presentation_binding_reuses_uniforms_and_preserves_resize_pixels() -> Result<(), String> {
    let gpu = gpu::Gpu::new()?;
    for dpr in [1, 2] {
        for mode in [
            ViewportPaintRendererMode::Legacy,
            ViewportPaintRendererMode::RetainedAuto,
        ] {
            let root = rsx! { <Element style={{width: Length::px(40.), height: Length::px(40.), background_color: Color::hex("#ff0000")}} /> };
            let mut viewport = Viewport::new();
            viewport.set_paint_renderer_mode(mode);
            let now = Instant::now();
            let mut frames = Vec::new();
            for side in [64, 96, 128, 64] {
                let size = [side * dpr, side * dpr];
                let mut stable = None;
                for frame in 0..6 {
                    rfgui::ui::ViewportHandle.request_redraw();
                    let (pixels, work) = rfgui::ui::profile_ui_work(|| {
                        redraw(&gpu, &mut viewport, &root, size, dpr as f32, now)
                    });
                    let pixels = pixels?.unwrap();
                    let red_pixels = pixels
                        .chunks_exact(4)
                        .filter(|pixel| {
                            pixel[0] == 255 && pixel[1] == 0 && pixel[2] == 0 && pixel[3] == 255
                        })
                        .count();
                    assert!(
                        ((38 * dpr * 38 * dpr) as usize..=(40 * dpr * 40 * dpr) as usize)
                            .contains(&red_pixels),
                        "presentation UVs must preserve the rectangle extent after resize (excluding antialiased edges): {red_pixels}, {mode:?}, DPR {dpr}, size {side}"
                    );
                    for y in 0..size[1] {
                        for x in 0..size[0] {
                            if x >= 40 * dpr || y >= 40 * dpr {
                                let offset = ((y * size[0] + x) * 4) as usize;
                                assert_ne!(
                                    &pixels[offset..offset + 4],
                                    &[255, 0, 0, 255],
                                    "red rectangle must not grow with the pooled target"
                                );
                            }
                        }
                    }
                    assert_eq!(
                        work.box_model_reads, 0,
                        "normal rendering must not collect diagnostic boxes"
                    );
                    assert_eq!(
                        work.box_model_reused_snapshots, 0,
                        "normal rendering must not clone diagnostic boxes"
                    );
                    if frame >= 2 {
                        assert_eq!(
                            work.present_bind_group_creations, 0,
                            "steady presentation must reuse binding and uniform"
                        );
                    }
                    if let Some(ref previous) = stable {
                        assert_eq!(&pixels, previous);
                    }
                    stable = Some(pixels);
                }
                let (boxes, reads) =
                    rfgui::ui::profile_ui_work(|| viewport.frame_box_models().len());
                assert_eq!(boxes, 1);
                assert!(reads.box_model_reads + reads.box_model_reused_snapshots > 0);
                let (_, cached) = rfgui::ui::profile_ui_work(|| viewport.frame_box_models());
                assert_eq!(
                    cached.box_model_reads + cached.box_model_reused_snapshots,
                    0
                );
                frames.push(stable.unwrap());
            }
            // Resizing back to the first size restores its frame exactly.
            assert_eq!(frames[0], frames[3], "{mode:?} DPR {dpr}");
        }
    }
    Ok(())
}
