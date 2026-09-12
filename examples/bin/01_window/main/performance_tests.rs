use super::*;
use rfgui::time::Instant;
use rfgui::view::Viewport;

/// The real demo tree through the production layout/paint entry. Window
/// acquisition is offscreen; this does not measure host presentation latency.
#[test]
#[ignore = "native hardware benchmark; run alone in release mode"]
fn native_complete_demo_warm_frames() -> Result<(), String> {
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
    let frames = std::env::var("RFGUI_PERF_FRAMES")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(40);
    assert!(frames >= 10);
    let static_scene = std::env::var_os("RFGUI_PERF_STATIC").is_some_and(|v| v == "1");
    let limit = std::env::var("RFGUI_PERF_MAX_BUILD_MS")
        .ok()
        .map(|v| v.parse::<f64>().expect("finite positive CPU budget"));
    assert!(limit.is_none_or(|v| v.is_finite() && v > 0.0));
    let mut over_budget = Vec::new();
    for dpr in [1, 2] {
        let mut viewport = Viewport::new();
        viewport.set_paint_renderer_mode(ViewportPaintRendererMode::RetainedAuto);
        let root = rfgui::ui::rsx_scope(|| rsx! { <MainScene /> });
        let now = Instant::now();
        let mut warm_build = Vec::new();
        let mut warm_total = Vec::new();
        let mut warm_allocations = 0;
        for frame in 0..frames {
            let observation = viewport.render_rsx_offscreen_for_test(
                &root,
                device.clone(),
                queue.clone(),
                [1280 * dpr, 800 * dpr],
                dpr as f32,
                now + std::time::Duration::from_millis(if static_scene { 0 } else { frame * 16 }),
            )?;
            assert!(
                observation.artifact_selected,
                "the real demo must select Artifact"
            );
            if frame >= 5 {
                warm_build.push(observation.cpu_ms[5]);
                warm_total.push(observation.cpu_ms[0]);
                warm_allocations += observation.target_allocations;
            }
            println!(
                "demo-frame dpr={dpr} frame={frame} artifact={} reuse={} reraster={} allocations={} command_replays={} localized_replays={} geometry_replays={} cpu_ms={:?}",
                observation.artifact_selected,
                observation.reuses,
                observation.rerasterizations,
                observation.target_allocations,
                observation.command_replays,
                observation.localized_replays,
                observation.geometry_replays,
                observation.cpu_ms
            );
            device
                .poll(wgpu::PollType::wait_indefinitely())
                .map_err(|e| e.to_string())?;
        }
        warm_build.sort_by(f64::total_cmp);
        warm_total.sort_by(f64::total_cmp);
        let median = |s: &[f64]| {
            if s.len() % 2 == 0 {
                (s[s.len() / 2 - 1] + s[s.len() / 2]) / 2.0
            } else {
                s[s.len() / 2]
            }
        };
        // Nearest-rank p95; first five frames are reported above, not mixed
        // into warm steady-state statistics. No concurrent compile/sampling.
        let p95 = |s: &[f64]| s[(s.len() * 95).div_ceil(100) - 1];
        println!(
            "perf-summary static={static_scene} dpr={dpr} n={} build_median_ms={:.6} build_p95_ms={:.6} total_median_ms={:.6} total_p95_ms={:.6} warm_target_allocations={warm_allocations}",
            warm_build.len(),
            median(&warm_build),
            p95(&warm_build),
            median(&warm_total),
            p95(&warm_total)
        );
        if let Some(limit) = limit {
            if median(&warm_build) > limit || p95(&warm_build) > limit {
                over_budget.push(format!(
                    "DPR {dpr}: median {:.3} / p95 {:.3} ms exceeds {limit} ms",
                    median(&warm_build),
                    p95(&warm_build)
                ));
            }
        }
    }
    if !over_budget.is_empty() {
        return Err(over_budget.join("; "));
    }
    Ok(())
}

/// Preserve one production viewport across physical-size/DPR changes. The first
/// frame of each step is reported separately; only frames 5+ enter its warm
/// distribution. These CPU/allocation observations are not a pixel oracle or
/// window-presentation measurement.
#[test]
#[ignore = "native hardware benchmark; run alone in release mode"]
fn native_complete_demo_viewport_transitions() -> Result<(), String> {
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
    let frames = std::env::var("RFGUI_PERF_FRAMES")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(40);
    assert!(frames >= 10);
    let static_scene = std::env::var_os("RFGUI_PERF_STATIC").is_some_and(|v| v == "1");
    let limit = std::env::var("RFGUI_PERF_MAX_BUILD_MS")
        .ok()
        .map(|v| v.parse::<f64>().expect("finite positive CPU budget"));
    assert!(limit.is_none_or(|v| v.is_finite() && v > 0.0));
    let mut viewport = Viewport::new();
    viewport.set_paint_renderer_mode(ViewportPaintRendererMode::RetainedAuto);
    let root = rfgui::ui::rsx_scope(|| rsx! { <MainScene /> });
    let now = Instant::now();
    let mut over_budget = Vec::new();
    // Order is intentional: logical resize, same logical size at DPR 2, then
    // return to DPR 1. Each transition starts from the previous step's backing.
    for (step, (label, logical, dpr)) in [
        ("initial", [1280, 800], 1),
        ("resize", [960, 600], 1),
        ("dpr-up", [960, 600], 2),
        ("dpr-down", [960, 600], 1),
    ]
    .into_iter()
    .enumerate()
    {
        let mut warm_build = Vec::new();
        let mut warm_total = Vec::new();
        let mut warm_allocations = 0;
        for frame in 0..frames {
            let tick = step as u64 * frames + frame;
            let observation = viewport.render_rsx_offscreen_for_test(
                &root,
                device.clone(),
                queue.clone(),
                [logical[0] * dpr, logical[1] * dpr],
                dpr as f32,
                now + std::time::Duration::from_millis(if static_scene { 0 } else { tick * 16 }),
            )?;
            assert!(
                observation.artifact_selected,
                "{label} frame {frame} must select Artifact"
            );
            assert_eq!(viewport.scale_factor(), dpr as f32);
            assert_eq!(
                viewport.logical_size(),
                (logical[0] as f32, logical[1] as f32)
            );
            println!(
                "transition-frame step={label} dpr={dpr} frame={frame} reuse={} reraster={} allocations={} cpu_ms={:?}",
                observation.reuses,
                observation.rerasterizations,
                observation.target_allocations,
                observation.cpu_ms
            );
            if frame >= 5 {
                warm_build.push(observation.cpu_ms[5]);
                warm_total.push(observation.cpu_ms[0]);
                warm_allocations += observation.target_allocations;
            }
            device
                .poll(wgpu::PollType::wait_indefinitely())
                .map_err(|e| e.to_string())?;
        }
        warm_build.sort_by(f64::total_cmp);
        warm_total.sort_by(f64::total_cmp);
        let median = |s: &[f64]| {
            if s.len() % 2 == 0 {
                (s[s.len() / 2 - 1] + s[s.len() / 2]) / 2.0
            } else {
                s[s.len() / 2]
            }
        };
        let p95 = |s: &[f64]| s[(s.len() * 95).div_ceil(100) - 1];
        println!(
            "transition-summary static={static_scene} step={label} dpr={dpr} n={} build_median_ms={:.6} build_p95_ms={:.6} total_median_ms={:.6} total_p95_ms={:.6} warm_target_allocations={warm_allocations}",
            warm_build.len(),
            median(&warm_build),
            p95(&warm_build),
            median(&warm_total),
            p95(&warm_total)
        );
        if limit.is_some_and(|limit| median(&warm_build) > limit || p95(&warm_build) > limit) {
            over_budget.push(format!(
                "{label}: median {:.3} / p95 {:.3} ms",
                median(&warm_build),
                p95(&warm_build)
            ));
        }
    }
    if over_budget.is_empty() {
        Ok(())
    } else {
        Err(over_budget.join("; "))
    }
}
