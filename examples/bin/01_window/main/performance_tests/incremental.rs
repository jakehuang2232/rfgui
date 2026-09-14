use super::*;

/// Run each update mode in its own process, alone, without trace flags for
/// performance acceptance. The idle mode freezes the source revision too.
#[test]
#[ignore = "native GPU benchmark; run alone with RFGUI_PERF_UPDATES=idle|particles|all"]
fn native_incremental_demo_frames() -> Result<(), String> {
    let _cleanup = TextCacheCleanup;
    let updates = std::env::var("RFGUI_PERF_UPDATES").unwrap_or_else(|_| "all".into());
    assert!(matches!(updates.as_str(), "idle" | "particles" | "all"));
    let frames = std::env::var("RFGUI_PERF_FRAMES")
        .ok()
        .map(|s| s.parse::<u64>().unwrap())
        .unwrap_or(330);
    assert!(frames >= 40);
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
    for dpr in [1, 2] {
        let mut viewport = Viewport::new();
        viewport.set_paint_renderer_mode(ViewportPaintRendererMode::RetainedAuto);
        let root = rfgui::ui::rsx_scope(|| rsx! { <MainScene /> });
        let now = Instant::now();
        let mut samples = Vec::new();
        for frame in 0..frames {
            let result = viewport.render_rsx_offscreen_for_test(
                &root,
                device.clone(),
                queue.clone(),
                [1280 * dpr, 800 * dpr],
                dpr as f32,
                now + std::time::Duration::from_millis(frame * 16),
            )?;
            assert!(result.artifact_selected);
            if frame >= 30 {
                samples.push(result.cpu_ms);
                if updates == "idle" {
                    assert_eq!(result.rerasterizations, 0, "idle native rasters stay warm");
                    assert!(
                        !viewport.is_animating(),
                        "idle engine has no running animation"
                    );
                    assert!(
                        viewport
                            .gpu_paint_observations()
                            .iter()
                            .all(|source| source.work
                                != Some(rfgui::view::gpu_paint::GpuPaintWork::Rendered)),
                        "idle source must not submit new producer draws"
                    );
                }
            }
            println!(
                "incremental-frame updates={updates} dpr={dpr} frame={frame} nodes={} reuse={} reraster={} allocations={} record_hits={} cpu_ms={:?}",
                viewport.node_arena().len(),
                result.reuses,
                result.rerasterizations,
                result.target_allocations,
                result.command_replays,
                result.cpu_ms
            );
            device
                .poll(wgpu::PollType::wait_indefinitely())
                .map_err(|e| e.to_string())?;
        }
        let mut medians = [0.0; 10];
        let mut p95s = [0.0; 10];
        for i in 0..10 {
            let mut values = samples.iter().map(|s| s[i]).collect::<Vec<_>>();
            values.sort_by(f64::total_cmp);
            medians[i] = (values[(values.len() - 1) / 2] + values[values.len() / 2]) / 2.0;
            p95s[i] = values[(values.len() * 95).div_ceil(100) - 1];
        }
        println!(
            "incremental-summary updates={updates} dpr={dpr} n={} medians={medians:?} p95s={p95s:?}",
            samples.len()
        );
    }
    Ok(())
}
