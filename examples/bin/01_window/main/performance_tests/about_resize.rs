use super::*;
use crate::scene_windows::about_panel::Build as AboutPanel;
use rfgui::view::viewport::PointerButton;
use rfgui_components::{Theme, Window};

#[test]
#[ignore = "native hardware resize benchmark; run alone in release mode"]
fn native_about_window_resize() -> Result<(), String> {
    let (device, queue) = pollster::block_on(async {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .map_err(|e| e.to_string())?;
        adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .map_err(|e| e.to_string())
    })?;
    let frames = std::env::var("RFGUI_ABOUT_RESIZE_FRAMES")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(22);
    assert!(frames >= 7);
    let budget = std::env::var("RFGUI_ABOUT_RESIZE_MAX_LAYOUT_MS")
        .ok()
        .map(|s| s.parse::<f64>().expect("layout budget in milliseconds"));
    assert!(budget.is_none_or(|ms| ms.is_finite() && ms > 0.0));
    for mode in [
        ViewportPaintRendererMode::Legacy,
        ViewportPaintRendererMode::RetainedAuto,
    ] {
        let mut layout_times = Vec::new();
        let mut frame_times = Vec::new();
        let mut viewport = Viewport::new();
        viewport.set_paint_renderer_mode(mode);
        let resized = std::rc::Rc::new(std::cell::Cell::new((360.0, 280.0)));
        let on_resize = rfgui_components::on_resize({
            let resized = resized.clone();
            move |width, height| resized.set((width, height))
        });
        let now = Instant::now();
        for frame in 0..frames {
            // Widen for ten frames, then narrow. Revisit evicted wrap widths too.
            let drag_frame = frame.saturating_sub(2) % 20;
            let step = if frame < 2 {
                0
            } else if drag_frame < 10 {
                drag_frame + 1
            } else {
                19 - drag_frame
            };
            let expected_width = 360.0 + step as f32 * 4.0;
            if frame == 2 {
                viewport.set_pointer_position_viewport(439.0, 180.0);
                viewport.dispatch_pointer_move_event();
                viewport.set_pointer_button_pressed(PointerButton::Left, true);
                viewport.dispatch_pointer_down_event(PointerButton::Left);
            }
            if frame >= 2 {
                viewport.set_pointer_position_viewport(439.0 + step as f32 * 4.0, 180.0);
                viewport.dispatch_pointer_move_event();
                assert_eq!(resized.get(), (expected_width, 280.0));
            }
            let root = rfgui::ui::rsx_scope(|| {
                rsx! {
                    <Window title="About" width=360.0 height=280.0 position={(80.0,40.0)} on_resize={on_resize.clone()}>
                        <AboutPanel theme={Theme::dark()} />
                    </Window>
                }
            });
            let observation = viewport.render_rsx_offscreen_for_test(
                &root,
                device.clone(),
                queue.clone(),
                [960, 720],
                1.0,
                now + std::time::Duration::from_millis(frame * 16),
            )?;
            assert!(
                viewport.node_arena().roots().iter().any(|key| {
                    let node = viewport.node_arena().get(*key).unwrap();
                    let bounds = node.element.box_model_snapshot();
                    (bounds.width - expected_width).abs() < 0.01
                        && (bounds.height - 280.0).abs() < 0.01
                }),
                "rendered window must follow the drag, mode={mode:?} frame={frame}"
            );
            if frame >= 2 {
                layout_times.push(observation.cpu_ms[2]);
                frame_times.push(observation.cpu_ms[0]);
            }
            println!(
                "about-resize mode={mode:?} frame={frame} width={expected_width} cpu_ms={:?}",
                observation.cpu_ms
            );
            device
                .poll(wgpu::PollType::wait_indefinitely())
                .map_err(|e| e.to_string())?;
        }
        viewport.set_pointer_button_pressed(PointerButton::Left, false);
        viewport.dispatch_pointer_up_event(PointerButton::Left);
        layout_times.sort_by(f64::total_cmp);
        let n = layout_times.len();
        let median = (layout_times[(n - 1) / 2] + layout_times[n / 2]) / 2.0;
        let p95 = layout_times[(layout_times.len() * 95).div_ceil(100) - 1];
        frame_times.sort_by(f64::total_cmp);
        let frame_median = (frame_times[(n - 1) / 2] + frame_times[n / 2]) / 2.0;
        let frame_p95 = frame_times[(n * 95).div_ceil(100) - 1];
        println!(
            "about-resize-summary mode={mode:?} n={} layout_median_ms={median:.3} layout_p95_ms={p95:.3} frame_median_ms={frame_median:.3} frame_p95_ms={frame_p95:.3}",
            layout_times.len()
        );
        if let Some(budget) = budget {
            assert!(
                p95 <= budget,
                "{mode:?}: layout p95 {p95:.3} ms exceeds {budget} ms"
            );
        }
    }
    Ok(())
}
