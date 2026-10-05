use super::*;
use rfgui::view::viewport::PointerButton;

const CPU_PHASES: [&str; 10] = [
    "total", "begin", "layout", "prepare", "sync", "build", "compile", "execute", "finish", "end",
];

fn median(values: &mut [f64]) -> f64 {
    values.sort_by(f64::total_cmp);
    let n = values.len();
    if n % 2 == 0 {
        (values[n / 2 - 1] + values[n / 2]) / 2.0
    } else {
        values[n / 2]
    }
}

/// Drag the top demo window by its title bar through the production input,
/// App rebuild and offscreen render path. One pointer move per frame, as the
/// window runner coalesces moves to frame boundaries. Offscreen CPU only; no
/// host presentation latency.
#[test]
#[ignore = "native hardware benchmark; run alone in release mode"]
fn native_demo_window_drag() -> Result<(), String> {
    let _text_cache_cleanup = TextCacheCleanup;
    let (device, queue) = pollster::block_on(async {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            flags: wgpu::InstanceFlags::empty(),
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .map_err(|e| e.to_string())?;
        adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .map_err(|e| e.to_string())
    })?;
    let frames = std::env::var("RFGUI_DRAG_FRAMES")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(120);
    let dpr = std::env::var("RFGUI_DRAG_DPR")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(2);
    let modes: Vec<_> = match std::env::var("RFGUI_DRAG_RENDERER").as_deref() {
        Ok("legacy") => vec![ViewportPaintRendererMode::Legacy],
        Ok("retained-auto") => vec![ViewportPaintRendererMode::RetainedAuto],
        _ => vec![
            ViewportPaintRendererMode::RetainedAuto,
            ViewportPaintRendererMode::Legacy,
        ],
    };
    const WARM: u64 = 10;
    // The last pushed window ("About") starts on top at (384, 384);
    // `RFGUI_DRAG_GRAB=x,y` grabs another window's title bar instead.
    let grab = std::env::var("RFGUI_DRAG_GRAB")
        .ok()
        .and_then(|v| {
            let (x, y) = v.split_once(',')?;
            Some((x.parse::<f32>().ok()?, y.parse::<f32>().ok()?))
        })
        .unwrap_or((384.0 + 150.0, 384.0 + 12.0));
    for mode in modes {
        let mut viewport = Viewport::new();
        viewport.set_paint_renderer_mode(mode);
        let now = Instant::now();
        let mut input_ms = Vec::new();
        let mut app_build_ms = Vec::new();
        let mut scene_ms = Vec::new();
        let mut frame_ms = Vec::new();
        let mut phases: [Vec<f64>; 10] = Default::default();
        let mut rerasters = 0;
        let mut fallback_frames = Vec::new();
        let mut fallback_reasons = std::collections::BTreeSet::new();
        for frame in 0..WARM + frames {
            let started = Instant::now();
            if frame == WARM {
                viewport.set_pointer_position_viewport(grab.0, grab.1);
                viewport.dispatch_pointer_move_event();
                viewport.set_pointer_button_pressed(PointerButton::Left, true);
                viewport.dispatch_pointer_down_event(PointerButton::Left);
            }
            if frame > WARM {
                // Sweep back and forth so the window stays inside the viewport.
                let step = ((frame - WARM) % 80) as f32;
                let offset = if step < 40.0 { step } else { 80.0 - step } * 4.0;
                viewport.set_pointer_position_viewport(grab.0 - offset, grab.1 - offset * 0.5);
                viewport.dispatch_pointer_move_event();
            }
            let input_done = Instant::now();
            // Production render_frame rebuilds App only when state is dirty.
            let root = rfgui::ui::rsx_scope(|| rsx! { <MainScene /> });
            let build_done = Instant::now();
            // The harness rejects whole-frame fallback after the frame has
            // submitted; record it instead so the drag can be characterized.
            let observation = viewport.render_rsx_offscreen_for_test(
                &root,
                device.clone(),
                queue.clone(),
                [1280 * dpr, 800 * dpr],
                dpr as f32,
                now + std::time::Duration::from_millis(frame * 16),
            );
            let finished = Instant::now();
            let (cpu_ms, frame_rerasters) = match observation {
                Ok(observation) => {
                    if std::env::var_os("RFGUI_DRAG_DIAG").is_some()
                        && (WARM - 1..=WARM + 3).contains(&frame)
                    {
                        println!(
                            "drag-diag frame={frame} reraster={} reuse={} allocations={} targets={:?}",
                            observation.rerasterizations,
                            observation.reuses,
                            observation.target_allocations,
                            observation.persistent_targets
                        );
                    }
                    (observation.cpu_ms, observation.rerasterizations)
                }
                Err(error) if error.starts_with("unexpected fallback") => {
                    if fallback_frames.is_empty() || frame > WARM {
                        fallback_reasons.insert(error);
                    }
                    fallback_frames.push(frame);
                    (viewport.renderer_performance_sample().0, 0)
                }
                Err(error) => return Err(error),
            };
            device
                .poll(wgpu::PollType::wait_indefinitely())
                .map_err(|e| e.to_string())?;
            if std::env::var_os("RFGUI_DRAG_TRACE").is_some() {
                println!(
                    "drag-frame frame={frame} fallback={} reraster={frame_rerasters} total={:.2} layout={:.2} sync={:.2} build={:.2} compile={:.2} end={:.2}",
                    fallback_frames.last() == Some(&frame),
                    cpu_ms[0],
                    cpu_ms[2],
                    cpu_ms[4],
                    cpu_ms[5],
                    cpu_ms[6],
                    cpu_ms[9]
                );
            }
            if std::env::var_os("RFGUI_DRAG_WORK").is_some()
                && (WARM - 3..WARM + 4).contains(&frame)
            {
                let profile = viewport.frontend_profile();
                let w = &profile.work;
                println!(
                    "drag-work frame={frame} scene_ms={:.2} unwrap_ms={:.2} reconcile_ms={:.2} translate_ms={:.2} commit_ms={:.2} renders={} memo_hits={} unwrap_nodes={} reconciled={} shared_hits={} patches={} fiber_works={}",
                    profile.scene_update_ms,
                    w.unwrap_ms,
                    w.reconcile_ms,
                    w.translate_ms,
                    w.incremental_commit_ms,
                    w.component_renders,
                    w.memo_hits,
                    w.unwrap_nodes,
                    w.reconciled_nodes,
                    w.shared_subtree_hits,
                    w.patches,
                    w.fiber_works
                );
            }
            if frame > WARM {
                input_ms.push((input_done - started).as_secs_f64() * 1000.0);
                app_build_ms.push((build_done - input_done).as_secs_f64() * 1000.0);
                scene_ms.push(viewport.frontend_profile().scene_update_ms);
                frame_ms.push((finished - started).as_secs_f64() * 1000.0);
                for (values, ms) in phases.iter_mut().zip(cpu_ms) {
                    values.push(ms);
                }
                rerasters += frame_rerasters;
            }
        }
        let phase_medians = phases
            .iter_mut()
            .zip(CPU_PHASES)
            .map(|(values, name)| format!("{name}={:.3}", median(values)))
            .collect::<Vec<_>>()
            .join(" ");
        println!(
            "window-drag mode={mode:?} dpr={dpr} nodes={} n={frames} frame_p50_ms={:.3} input_p50_ms={:.3} app_build_p50_ms={:.3} scene_update_p50_ms={:.3} render[{phase_medians}] rerasterizations={rerasters} whole_frame_fallbacks={} first_fallback_frames={:?} fallback_reasons={fallback_reasons:?}",
            viewport.node_arena().len(),
            median(&mut frame_ms),
            median(&mut input_ms),
            median(&mut app_build_ms),
            median(&mut scene_ms),
            fallback_frames.len(),
            &fallback_frames[..fallback_frames.len().min(8)],
        );
    }
    Ok(())
}
