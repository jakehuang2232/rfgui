//! Full offscreen frames, including state publication, RSX construction, real
//! layout/paint/submit and a separate GPU completion wait. No window present.
#![cfg(feature = "renderer-test-support")]
use rfgui::style::{Color, Layout, Length, ScrollDirection};
use rfgui::time::Instant;
use rfgui::ui::{
    Binding, RsxNode, component, flush_state_updates, profile_ui_work, rsx, rsx_scope,
};
use rfgui::view::viewport::{ViewportPaintRendererMode, set_scroll_offset_by_id};
use rfgui::view::{Element, Text, Viewport};
#[path = "frame_pipeline_bench/diagnostics.rs"]
mod diagnostics;
#[path = "frame_pipeline_bench/gpu.rs"]
mod gpu;
const SIZE: [u32; 2] = [640, 480];

#[component]
fn Row(label: String, red: bool, wide: bool) -> RsxNode {
    rsx! {
        <Element style={{width:Length::px(if wide {360.0} else {320.0}),height:Length::px(24.0),background:if red {Color::rgb(180,40,40)} else {Color::rgb(40,60,80)}}}>
            <Text>{label}</Text>
        </Element>
    }
}
fn scene(case: &str, value: bool, rows: usize) -> RsxNode {
    let layout: Layout = if std::env::var_os("RFGUI_BENCH_FLEX").is_some() {
        Layout::flex().column().into()
    } else {
        Layout::flow().column().into()
    };
    rsx_scope(|| {
        rsx! {
            <Element style={{width:Length::px(400.0),height:Length::px(400.0),layout:layout,scroll_direction:ScrollDirection::Both}}>
                {(0..rows).map(|i| rsx! {
                    <Row label={if i==0 && case=="text" && value {"Edited text".to_owned()} else {format!("Row {i}")}} red={i==0 && case=="color" && value} wide={i==0 && case=="size" && value}/>
                }).collect::<Vec<_>>()}
            </Element>
        }
    })
}
fn p50(mut values: Vec<f64>) -> f64 {
    values.sort_by(f64::total_cmp);
    (values[(values.len() - 1) / 2] + values[values.len() / 2]) * 0.5
}

#[test]
#[ignore = "native full-frame benchmark; run alone with --ignored --nocapture --test-threads=1"]
fn state_to_completed_offscreen_frame() -> Result<(), String> {
    let gpu = gpu::Gpu::new()?;
    let samples = std::env::var("RFGUI_BENCH_SAMPLES")
        .ok()
        .map(|n| n.parse::<usize>().unwrap())
        .unwrap_or(60);
    assert!(samples > 0);
    let diagnostics_enabled = std::env::var_os("RFGUI_BENCH_DIAGNOSTICS").is_some();
    let mut cases = vec!["color", "text", "scroll", "size"];
    if std::env::var_os("RFGUI_BENCH_IDLE").is_some() {
        cases.push("idle");
    }
    for rows in [128, 512] {
        for &case in &cases {
            let mut reference_pixels: Vec<Vec<u8>> = Vec::new();
            for mode in [
                ViewportPaintRendererMode::Legacy,
                ViewportPaintRendererMode::RetainedAuto,
            ] {
                let state = Binding::new(false);
                let mut viewport = Viewport::new();
                viewport.set_paint_renderer_mode(mode);
                viewport.set_renderer_diagnostics_for_test(diagnostics_enabled);
                let mut diagnostics = diagnostics::Samples::default();
                let now = Instant::now();
                let mut cpu = Vec::new();
                let mut completed = Vec::new();
                let mut phases: [Vec<f64>; 10] = Default::default();
                let mut counts = [0usize; 9];
                for frame in 0..20 + samples {
                    let start = Instant::now();
                    let (result, p) = profile_ui_work(|| {
                        state.set(frame % 2 != 0);
                        flush_state_updates();
                        let value = state.snapshot().get();
                        if case == "scroll" && frame > 0 {
                            let arena = viewport.node_arena();
                            let root = arena.roots()[0];
                            let id = arena.get(root).unwrap().element.stable_id();
                            assert!(set_scroll_offset_by_id(
                                arena,
                                root,
                                id,
                                (0., if value { 12. } else { 0. })
                            ));
                        }
                        let tree = scene(case, value, rows);
                        viewport.render_rsx_offscreen_for_test(
                            &tree,
                            gpu.device.clone(),
                            gpu.queue.clone(),
                            SIZE,
                            1.,
                            now,
                        )
                    });
                    let frame_cpu = start.elapsed().as_secs_f64() * 1000.;
                    let out = result.map_err(|e| format!("{rows} {case} {mode:?}: {e}"))?;
                    gpu.device
                        .poll(wgpu::PollType::wait_indefinitely())
                        .map_err(|e| e.to_string())?;
                    let frame_completed = start.elapsed().as_secs_f64() * 1000.;
                    if frame == 18 || frame == 19 {
                        let pixels = gpu.read(&out.texture, SIZE)?;
                        if let Some(dir) = std::env::var_os("RFGUI_BENCH_PIXELS") {
                            let dir = std::path::PathBuf::from(dir);
                            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                            std::fs::write(
                                dir.join(format!("{rows}-{case}-{mode:?}-{}.rgba", frame - 18)),
                                &pixels,
                            )
                            .map_err(|e| e.to_string())?;
                        }
                        assert!(pixels.chunks_exact(4).any(|p| p[3] > 0), "empty output");
                        if mode == ViewportPaintRendererMode::Legacy {
                            reference_pixels.push(pixels);
                        } else {
                            let differences = pixels
                                .iter()
                                .zip(&reference_pixels[frame - 18])
                                .filter(|(a, b)| a != b)
                                .count();
                            assert_eq!(differences, 0, "renderer pixel mismatch: {case}");
                        }
                    }
                    assert_eq!(out.diagnostics.is_some(), diagnostics_enabled);
                    if frame >= 20 {
                        if let Some(d) = out.diagnostics {
                            diagnostics.push(d);
                        }
                        if case == "color" {
                            assert_eq!(p.measure_calls, 0, "paint-only update remeasured");
                        }
                        cpu.push(frame_cpu);
                        completed.push(frame_completed);
                        for (values, ms) in phases.iter_mut().zip(out.cpu_ms) {
                            values.push(ms);
                        }
                        for (sum, n) in counts.iter_mut().zip([
                            p.dirty_observations,
                            p.dirty_subtree_reuses,
                            p.measure_calls,
                            p.place_calls,
                            p.box_model_reads,
                            p.box_model_reused_snapshots,
                            p.dirty_clear_visits,
                            p.render_change_observations,
                            p.measure_reuses,
                        ]) {
                            *sum += n;
                        }
                    }
                }
                assert!(
                    (reference_pixels[0] != reference_pixels[1]) == (case != "idle"),
                    "update didn't change pixels: {case}"
                );
                diagnostics.print(&format!("{mode:?}"), case, rows);
                println!(
                    "full-frame mode={mode:?} case={case} rows={rows} n={samples} cpu_p50_ms={:.6} completed_p50_ms={:.6} phases_p50_ms={:?} counts_sum={counts:?}",
                    p50(cpu),
                    p50(completed),
                    phases.map(p50)
                );
            }
        }
    }
    Ok(())
}
