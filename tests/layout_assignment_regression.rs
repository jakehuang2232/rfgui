//! Incremental assignment reuse must match a freshly built scene, in both renderers.
#![cfg(feature = "renderer-test-support")]
use rfgui::style::{Color, CrossSize, Layout, Length, ScrollDirection};
use rfgui::time::Instant;
use rfgui::ui::{RsxNode, render_root, rsx};
use rfgui::view::viewport::ViewportPaintRendererMode;
use rfgui::view::{Element, Text, Viewport};
#[path = "frame_pipeline_bench/gpu.rs"]
mod gpu;
const SIZE: [u32; 2] = [640, 480];

fn scene(axis: usize, case: &str, step: usize) -> RsxNode {
    let changed = step % 2 == 1;
    let cross = if case == "stretch" && changed {
        CrossSize::Fit
    } else {
        CrossSize::Stretch
    };
    let layout: Layout = match axis {
        0 => Layout::flow().column().cross_size(cross).into(),
        1 => Layout::flow().row().cross_size(cross).into(),
        2 => Layout::flex().column().cross_size(cross).into(),
        _ => Layout::flex().row().cross_size(cross).into(),
    };
    render_root(|| {
        rsx! {
            <Element style={{layout:layout, width:Length::px(if case=="parent" && changed {340.} else {400.}),
                height:Length::px(400.), gap:Length::px(if changed {7.} else {3.}),scroll_direction:ScrollDirection::Both}}>
                {(0..if case=="structure" && changed {3} else {4}).map(|i| {
                    let label = if i==0 && (case=="text" || case=="wrap") {
                        if case=="wrap" && changed {format!("new {step} words that wrap onto several separate lines")} else {format!("fresh {step}")}
                    } else {format!("Row {i}")};
                    let color = if case=="paint" && changed && i==1 {Color::rgb(180,40,40)} else {Color::rgb(40,60,80)};
                    let font = if case=="font" && changed {20.} else {16.};
                    if case=="stretch" {
                        rsx! { <Element style={{background:color}}><Text style={{font_size:font}}>{label}</Text></Element> }
                    } else {
                        rsx! { <Element style={{width:Length::percent(if case=="wrap" {16.} else {20.}), background:color}}><Text style={{font_size:font}}>{label}</Text></Element> }
                    }
                }).collect::<Vec<_>>()}
            </Element>
        }
    })
}

#[test]
#[ignore = "native Metal pixel regression; run alone"]
fn incremental_assignment_matches_cold_geometry_and_pixels() -> Result<(), String> {
    let gpu = gpu::Gpu::new()?;
    let now = Instant::now();
    let mut frames = 0;
    for dpr in [1., 2.] {
        for axis in 0..4 {
            for case in [
                "text",
                "wrap",
                "parent",
                "font",
                "paint",
                "structure",
                "stretch",
            ] {
                for mode in [
                    ViewportPaintRendererMode::Legacy,
                    ViewportPaintRendererMode::RetainedAuto,
                ] {
                    let mut incremental = Viewport::new();
                    incremental.set_paint_renderer_mode(mode);
                    for step in 0..4 {
                        let tree = scene(axis, case, step);
                        let out = incremental.render_rsx_offscreen_for_test(
                            &tree,
                            gpu.device.clone(),
                            gpu.queue.clone(),
                            SIZE,
                            dpr,
                            now,
                        )?;
                        let pixels = gpu.read(&out.texture, SIZE)?;
                        let mut cold = Viewport::new();
                        cold.set_paint_renderer_mode(mode);
                        let fresh = cold.render_rsx_offscreen_for_test(
                            &tree,
                            gpu.device.clone(),
                            gpu.queue.clone(),
                            SIZE,
                            dpr,
                            now,
                        )?;
                        let expected = gpu.read(&fresh.texture, SIZE)?;
                        assert!(pixels.chunks_exact(4).any(|p| p[3] > 0));
                        assert_eq!(
                            pixels.iter().zip(&expected).filter(|(a, b)| a != b).count(),
                            0,
                            "incremental/cold {mode:?} axis={axis} case={case} step={step} dpr={dpr}"
                        );
                        frames += 2;
                    }
                }
            }
        }
    }
    println!("assignment regression: {frames} GPU frames; incremental/cold byte-exact");
    Ok(())
}
