//! Reproducible input-to-submit benchmark. This is offscreen CPU latency,
//! not window FPS or process CPU utilization. Run alone, without trace flags.
use rfgui::platform::input::Key;
use rfgui::style::{Color, Layout, Length, ScrollDirection};
use rfgui::time::Instant;
use rfgui::ui::{KeyEventData, RsxNode, rsx};
use rfgui::view::viewport::{ViewportPaintRendererMode, set_scroll_offset_by_id};
use rfgui::view::{Element, Text, TextArea, Viewport};

#[path = "../../lib/rfgui-components/tests/retained_controls/gpu.rs"]
mod gpu;

fn scene() -> RsxNode {
    rsx! {
        <Element style={{width:Length::px(640.), height:Length::px(480.), layout:Layout::flow().column()}}>
            <Element style={{width:Length::px(400.), height:Length::px(60.)}}>
                <TextArea content="Input benchmark" font_size=16 />
            </Element>
            <Element style={{width:Length::px(400.), height:Length::px(400.), layout:Layout::flow().column(), scroll_direction:ScrollDirection::Both}}>
                {(0..512).map(|i| rsx! {
                    <Element style={{width:Length::px(360.), height:Length::px(24.), background:Color::rgb(40,60,80), hover:{background:Color::rgb(160,40,40)}}}>
                        <Text>{format!("Row {i}: immutable text")}</Text>
                    </Element>
                }).collect::<Vec<_>>()}
            </Element>
        </Element>
    }
}

fn percentile(values: &[f64], percent: usize) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    sorted[(sorted.len() * percent).div_ceil(100) - 1]
}

#[test]
#[ignore = "native GPU benchmark; run alone in release with --ignored --nocapture --test-threads=1"]
fn input_to_submitted_frame() -> Result<(), String> {
    let gpu = gpu::Gpu::new()?;
    let samples = std::env::var("RFGUI_BENCH_SAMPLES")
        .ok()
        .map(|s| s.parse::<usize>().unwrap())
        .unwrap_or(180);
    assert!(samples > 0);
    let diagnostics = std::env::var_os("RFGUI_BENCH_DIAGNOSTICS").is_some();
    for case in ["idle", "hover", "scroll", "text-input"] {
        for mode in [
            ViewportPaintRendererMode::Legacy,
            ViewportPaintRendererMode::RetainedAuto,
        ] {
            let root = scene();
            let mut viewport = Viewport::new();
            viewport.set_paint_renderer_mode(mode);
            // Freeze animation time to isolate each requested input. Scheduling
            // and caret frequency are measured separately in the window runner.
            let now = Instant::now();
            let mut cpu = Vec::new();
            let mut input = Vec::new();
            let mut phases: [Vec<f64>; 10] = Default::default();
            let mut work = [0_u64; 3];
            let mut sampled = Vec::new();
            for frame in 0..30 + samples {
                let start = Instant::now();
                if frame > 0 {
                    match case {
                        "hover" => {
                            viewport.set_pointer_position_viewport(
                                20.,
                                if frame % 2 == 0 { 80. } else { 104. },
                            );
                            viewport.dispatch_pointer_move_event();
                        }
                        "scroll" => {
                            // Match the existing full-frame benchmark: apply a
                            // settled offset, excluding wheel inertia/timers.
                            let arena = viewport.node_arena();
                            let root = arena.roots()[0];
                            let host = arena.get(root).unwrap().children()[1];
                            let id = arena.get(host).unwrap().element.stable_id();
                            assert!(set_scroll_offset_by_id(
                                arena,
                                root,
                                id,
                                (0., if frame % 2 == 0 { 0. } else { 12. })
                            ));
                        }
                        "text-input" => {
                            if frame == 1 {
                                let arena = viewport.node_arena();
                                let root = arena.roots()[0];
                                let wrapper = arena.get(root).unwrap().children()[0];
                                let editor = arena.get(wrapper).unwrap().children()[0];
                                viewport.set_focused_node_id(Some(editor));
                            }
                            if frame % 2 != 0 {
                                assert!(viewport.dispatch_text_input_event("x".into()));
                            } else {
                                assert!(viewport.dispatch_key_down_event(KeyEventData {
                                    key: Key::Backspace,
                                    characters: None,
                                    modifiers: Default::default(),
                                    repeat: false,
                                    is_composing: false,
                                    location: Default::default(),
                                    timestamp: now,
                                }));
                            }
                        }
                        _ => {}
                    }
                }
                let input_ms = start.elapsed().as_secs_f64() * 1000.;
                let render = |viewport: &mut Viewport| {
                    viewport.render_rsx_offscreen_for_test(
                        &root,
                        gpu.device.clone(),
                        gpu.queue.clone(),
                        [640, 480],
                        1.,
                        now,
                    )
                };
                // Work counters are opt-in and observe one unmeasured frame.
                let output = if diagnostics && frame == 29 {
                    let (output, work) = rfgui::ui::profile_ui_work(|| render(&mut viewport));
                    println!(
                        "interaction-rect-work mode={mode:?} case={case} rect_draw_calls={} rect_instances={} rect_instance_uploads={} graphics_passes={}",
                        work.rect_draw_calls,
                        work.rect_instances,
                        work.rect_instance_uploads,
                        work.graphics_passes_recorded
                    );
                    output
                } else {
                    render(&mut viewport)
                }?;
                let cpu_ms = start.elapsed().as_secs_f64() * 1000.;
                // Keep GPU completion outside CPU sample and prevent queued
                // work from an earlier frame distorting the next sample.
                gpu.device
                    .poll(wgpu::PollType::wait_indefinitely())
                    .map_err(|e| e.to_string())?;
                if frame == 28 || frame == 29 {
                    if std::env::var_os("RFGUI_BENCH_DIAGNOSTICS").is_some() {
                        println!(
                            "interaction-targets mode={mode:?} case={case} frame={frame} targets={:?}",
                            output.persistent_targets
                        );
                    }
                    let pixels = gpu.read(&output.texture, [640, 480])?;
                    assert!(pixels.chunks_exact(4).any(|p| p[3] > 0));
                    if let Some(dir) = std::env::var_os("RFGUI_BENCH_PIXELS") {
                        let dir = std::path::PathBuf::from(dir);
                        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                        std::fs::write(
                            dir.join(format!("{case}-{mode:?}-{}.rgba", frame - 28)),
                            &pixels,
                        )
                        .map_err(|e| e.to_string())?;
                    }
                    // Compare the same renderer before/after an optimization.
                    if let Some(dir) = std::env::var_os("RFGUI_BENCH_REFERENCE_PIXELS") {
                        let expected = std::fs::read(
                            std::path::PathBuf::from(dir)
                                .join(format!("{case}-{mode:?}-{}.rgba", frame - 28)),
                        )
                        .map_err(|e| e.to_string())?;
                        let differences =
                            pixels.iter().zip(&expected).filter(|(a, b)| a != b).count();
                        assert_eq!(pixels.len(), expected.len());
                        assert_eq!(
                            differences, 0,
                            "before/after pixel mismatch: {case} {mode:?}"
                        );
                    }
                    sampled.push(pixels);
                }
                if frame >= 30 {
                    work[0] += output.rerasterizations as u64;
                    work[1] += output.reuses as u64;
                    work[2] += output.target_allocations;
                    cpu.push(cpu_ms);
                    input.push(input_ms);
                    for (values, ms) in phases.iter_mut().zip(output.cpu_ms) {
                        values.push(ms);
                    }
                }
            }
            assert_eq!(
                sampled[0] == sampled[1],
                case == "idle",
                "input must change pixels: {case} {mode:?}"
            );
            println!(
                "interaction mode={mode:?} case={case} nodes={} n={samples} cpu_p50_ms={:.6} cpu_p95_ms={:.6} input_p50_ms={:.6} phases_p50_ms={:?} work_sum_reraster_reuse_alloc={work:?}",
                viewport.node_arena().len(),
                percentile(&cpu, 50),
                percentile(&cpu, 95),
                percentile(&input, 50),
                phases.map(|v| percentile(&v, 50))
            );
        }
    }
    Ok(())
}
