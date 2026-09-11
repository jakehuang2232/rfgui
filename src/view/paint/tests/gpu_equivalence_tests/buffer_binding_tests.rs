use super::*;
use crate::view::render_pass::buffer_bindings::take_counts_for_test;

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_graphics_group_elides_buffers_without_losing_per_draw_uniforms() -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native graphics context");
    let mut viewport = Viewport::new();
    for dpr in [1_u32, 2] {
        let size = [WIDTH * dpr, HEIGHT * dpr];
        let (mut graph, mut ctx, target) =
            transformed_graph_prelude_with_size(dpr as f32, None, size);
        for row in 0..16 {
            for column in 0..16 {
                // Shared mesh, distinct positions and alternating colors. A
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
    eprintln!("graphics binding pixel evidence on {}", gpu.label());
    Ok(())
}
