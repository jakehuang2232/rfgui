use super::*;
use crate::view::render_pass::buffer_bindings::take_counts_for_test;
mod buffered_grid;

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_rect_bindings_preserve_solid_and_gradient_layout_switches() -> Result<(), String> {
    use crate::view::render_pass::draw_rect_pass::{GradientPaint, GradientStopGpu};
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native graphics context");
    let red = [1.0, 0.0, 0.0, 1.0];
    let blue = [0.0, 0.0, 1.0, 1.0];
    let gradient = GradientPaint {
        axis: [0.0, 0.0, 24.0, 0.0],
        stops: [(0.0, red), (0.5, red), (0.5, blue), (1.0, blue)]
            .map(|(position, color)| GradientStopGpu {
                color,
                pos: [position, 0.0, 0.0, 0.0],
            })
            .into(),
        ..Default::default()
    };
    for dpr in [1_u32, 2] {
        let mut viewport = Viewport::new();
        // Reuse one Viewport and its layout-keyed bind-group cache. Returning
        // to solid after each gradient must not reuse a gradient layout.
        for kind in [0, 1, 0, 2, 0] {
            let size = [WIDTH * dpr, HEIGHT * dpr];
            let (mut graph, mut ctx, target) =
                transformed_graph_prelude_with_size(dpr as f32, None, size);
            let mut params = RectPassParams {
                position: [8.0, 8.0],
                size: [24.0, 24.0],
                fill_color: [0.0, 1.0, 0.0, 1.0],
                opacity: 1.0,
                ..Default::default()
            };
            if kind == 1 {
                params.gradient = Some(gradient.clone());
            }
            if kind == 2 {
                params.border_widths = [4.0; 4];
                params.border_color = [1.0; 4];
                params.border_gradient = Some(gradient.clone());
            }
            ctx.emit_draw_rect_pass(
                &mut graph,
                DrawRectPass::new(params, DrawRectInput::default(), DrawRectOutput::default()),
            );
            add_present(&mut graph, &target)?;
            let pixels =
                render_on_viewport_with_size(graph, gpu, &mut viewport, dpr as f32, FORMAT, size)?;
            let probes = match kind {
                1 => [(14, 20, [255, 0, 0, 255]), (26, 20, [0, 0, 255, 255])],
                2 => [(10, 20, [255, 0, 0, 255]), (30, 20, [0, 0, 255, 255])],
                _ => [(14, 20, [0, 255, 0, 255]), (26, 20, [0, 255, 0, 255])],
            };
            for (x, y, expected) in probes.into_iter().chain([(4, 20, [0; 4])]) {
                let start = (((y * dpr) * size[0] + x * dpr) * 4) as usize;
                assert_eq!(
                    &pixels[start..start + 4],
                    expected,
                    "kind {kind}, DPR {dpr}, ({x},{y})"
                );
            }
            if kind != 0 {
                assert!(
                    viewport.has_gradient_stops_buffer_for_test(),
                    "gradient variants still require real storage"
                );
            }
        }
    }
    Ok(())
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_graphics_group_elides_buffers_without_losing_per_draw_uniforms() -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native graphics context");
    for buffered in [false, true] {
        let mut viewport = Viewport::new();
        for dpr in [1_u32, 2] {
            let mesh =
                buffered.then(|| std::sync::Arc::new(buffered_grid::Resources::new(&gpu.device)));
            let size = [WIDTH * dpr, HEIGHT * dpr];
            let (mut graph, mut ctx, target) =
                transformed_graph_prelude_with_size(dpr as f32, None, size);
            for row in 0..16 {
                for column in 0..16 {
                    if let Some(resources) = &mesh {
                        graph.add_graphics_pass(buffered_grid::Pass {
                            resources: resources.clone(),
                            target,
                            row,
                            column,
                            dpr,
                        });
                        continue;
                    }
                    // Procedural rectangles, distinct positions and alternating colors. A
                    // skipped dynamic-offset update changes independently known
                    // pixels even when vertex/index accounting looks plausible.
                    let color = if (row + column) % 2 == 0 {
                        [1.0, 0.0, 0.0, 1.0]
                    } else {
                        [0.0, 0.0, 1.0, 1.0]
                    };
                    let mut pass = DrawRectPass::new(
                        RectPassParams {
                            position: [column as f32 * 4.0, row as f32 * 4.0],
                            size: [4.0, 4.0],
                            fill_color: color,
                            opacity: 1.0,
                            ..Default::default()
                        },
                        DrawRectInput::default(),
                        DrawRectOutput::default(),
                    );
                    pass.set_render_mode(crate::view::render_pass::RectRenderMode::FillOnly);
                    ctx.emit_draw_rect_pass(&mut graph, pass);
                }
            }
            add_present(&mut graph, &target)?;
            let _ = take_counts_for_test();
            let pixels =
                render_on_viewport_with_size(graph, gpu, &mut viewport, dpr as f32, FORMAT, size)?;
            let groups = take_counts_for_test();
            assert!(
                !viewport.has_gradient_stops_buffer_for_test(),
                "solid-only draws must neither allocate nor bind gradient storage"
            );
            let mut counts = [[0; 2]; 2];
            for group in &groups {
                for kind in 0..2 {
                    for counter in 0..2 {
                        counts[kind][counter] += group[kind][counter];
                    }
                }
            }
            for row in 0..16 {
                for column in 0..16 {
                    let x = (column * 4 + 2) * dpr;
                    let y = (row * 4 + 2) * dpr;
                    let start = ((y * size[0] + x) * 4) as usize;
                    let expected = if (row + column) % 2 == 0 {
                        [255, 0, 0, 255]
                    } else {
                        [0, 0, 255, 255]
                    };
                    assert_eq!(
                        &pixels[start..start + 4],
                        expected,
                        "cell ({column}, {row}), DPR {dpr}"
                    );
                }
            }
            if !buffered {
                assert!(
                    groups.is_empty(),
                    "procedural rectangles must issue zero vertex/index bindings: {groups:?}"
                );
                eprintln!(
                    "procedural rectangles DPR {dpr}: 256 draws, zero vertex/index bindings, no gradient storage"
                );
                continue;
            }
            eprintln!("graphics buffer binding groups DPR {dpr}: {groups:?}");
            // A render-pass boundary requires re-establishing the bindings. The
            // expected saving is relative to actual groups, not one global bind.
            assert!(!groups.is_empty());
            assert!(
                groups.len() <= 4,
                "fixture must still group its rectangle draws"
            );
            for group in &groups {
                assert_eq!(
                    group[0][1], 1,
                    "one shared vertex binding per group: {groups:?}"
                );
                assert_eq!(
                    group[1][1], 1,
                    "one shared index binding per group: {groups:?}"
                );
            }
            for (name, [requested, emitted]) in ["vertex", "index"].into_iter().zip(counts) {
                assert!(
                    requested >= 256,
                    "all rectangle draws must reach {name} binding: {counts:?}"
                );
                assert!(
                    emitted == groups.len(),
                    "only render-pass boundaries require a fresh {name} binding: {counts:?}"
                );
                eprintln!(
                    "graphics buffer bindings DPR {dpr} {name}: requested={requested}, emitted={emitted}, skipped={}",
                    requested - emitted
                );
            }
        }
    }
    eprintln!("graphics binding pixel evidence on {}", gpu.label());
    Ok(())
}
