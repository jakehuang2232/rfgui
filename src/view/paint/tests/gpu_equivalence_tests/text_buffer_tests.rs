use super::*;
use crate::view::inline_formatting_context::{
    InlineFormattingContext, InlineIfcInput, InlineIfcItem, InlineIfcLayoutOptions,
    InlineIfcSourceId, InlineIfcStyle,
};
use crate::view::render_pass::text_pass::{
    TextInput, TextOutput, TextPassPreparedFragment, TextPassPreparedParams, TextPreparedInputPass,
};

// Release thread-local GPU objects before wgpu's own thread-local teardown,
// including when a pixel assertion panics (same lifetime as other Text gates).
struct TextGpuCleanup;
impl Drop for TextGpuCleanup {
    fn drop(&mut self) {
        crate::view::render_pass::text_pass::clear_text_resources_cache();
    }
}

fn text_params(dpr: u32, row: u32, shift: u32, doubled: bool) -> TextPassPreparedParams {
    let color = if row == 0 {
        [255, 0, 0, 255]
    } else {
        [0, 0, 255, 255]
    };
    let ifc = InlineFormattingContext::build_with_options(
        InlineIfcInput::new(vec![InlineIfcItem::TextSpan {
            source: InlineIfcSourceId(1),
            text: "MMM".into(),
            style: Some(InlineIfcStyle {
                font_size: 12.0,
                line_height: 1.2,
                font_weight: 400,
                brush: color,
                font_families: vec!["sans-serif".into()].into(),
                vertical_align: crate::style::VerticalAlign::Baseline,
            }),
        }]),
        InlineIfcLayoutOptions::new(Some(40.0), true),
    );
    let mut staging_input =
        crate::view::inline_text_pass_adapter::inline_ifc_paint_input_to_text_pass_staging_input(
            &ifc.text_pass_paint_input(),
            [0.0, 0.0],
            dpr as f32,
            0,
            1.0,
        );
    assert!(!staging_input.glyphs.is_empty());
    if doubled {
        let mut second = staging_input.glyphs.clone();
        for glyph in &mut second {
            glyph.paint.fragment_index = 1;
        }
        staging_input.glyphs.extend(second);
    }
    TextPassPreparedParams {
        staging_input,
        fragments: (0..if doubled { 2 } else { 1 })
            .map(|column| TextPassPreparedFragment {
                origin: [(4 + shift + column * 48) as f32, (4 + row * 28) as f32],
                size: [40.0, 24.0],
            })
            .collect(),
        scissor_rect: None,
        stencil_clip_id: None,
    }
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_text_globals_reuse_buffers_without_overwriting_other_passes() -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native graphics context");
    let _cleanup = TextGpuCleanup;
    for dpr in [1_u32, 2] {
        let mut viewport = Viewport::new();
        let mut baseline: Option<Vec<u8>> = None;
        // Each state repeats on one Viewport. Include movement, fragment buffer
        // growth/shrink, and render-target resizing; then prove the next frame
        // reuses real buffer handles while receiving that state's fresh bytes.
        for (stage, (shift, doubled, width)) in [
            (0, false, 96),
            (8, false, 96),
            (8, true, 96),
            (0, true, 112),
            (0, false, 96),
            (0, false, 96), // explicit cache release, followed by recovery
        ]
        .into_iter()
        .enumerate()
        {
            let released = if stage == 5 {
                let previous = viewport.frame_buffers_for_test();
                viewport.release_render_resource_caches();
                assert!(viewport.frame_buffers_for_test().is_empty());
                previous
            } else {
                Vec::new()
            };
            let size = [width * dpr, 64 * dpr];
            let mut cold_buffers = None;
            for warm in [false, true] {
                let (mut graph, ctx, target) =
                    transformed_graph_prelude_with_size(dpr as f32, None, size);
                for row in 0..2 {
                    graph.add_graphics_pass(TextPreparedInputPass::new(
                        text_params(dpr, row, shift, doubled),
                        TextInput {
                            pass_context: ctx.graphics_pass_context(),
                        },
                        TextOutput {
                            render_target: target,
                        },
                    ));
                }
                add_present(&mut graph, &target)?;
                let pixels = render_on_viewport_with_size(
                    graph,
                    gpu,
                    &mut viewport,
                    dpr as f32,
                    FORMAT,
                    size,
                )?;
                // Independent color and region assertions prevent an empty
                // image, or two passes overwriting one another, from becoming
                // a valid temporal reference. Font raster shape itself is not
                // this gate's oracle: translations/duplication below use the
                // first frame of this same path, never Legacy output.
                let mut coverage = [[0; 2]; 2];
                for y in 0..size[1] {
                    for x in 0..size[0] {
                        let at = ((y * size[0] + x) * 4) as usize;
                        let pixel = &pixels[at..at + 4];
                        if pixel[3] == 0 {
                            continue;
                        }
                        let row = if y < 32 * dpr { 0 } else { 1 };
                        let column = if x < (52 + shift) * dpr { 0 } else { 1 };
                        assert!(column == 0 || doubled);
                        let left = (4 + shift + column as u32 * 48) * dpr;
                        let top = (4 + row as u32 * 28) * dpr;
                        assert!(x >= left && x < left + 40 * dpr);
                        assert!(y >= top && y < top + 24 * dpr);
                        assert_eq!(
                            &pixel[..3],
                            if row == 0 { &[255, 0, 0] } else { &[0, 0, 255] }
                        );
                        coverage[row][column] += 1;
                    }
                }
                for row in coverage {
                    assert!(row[0] > 10, "first fragment must actually draw");
                    assert_eq!(row[1] > 10, doubled, "second fragment visibility");
                }
                if let Some(base) = &baseline {
                    let mut expected = vec![0; pixels.len()];
                    for y in 0..64 * dpr {
                        for x in 0..96 * dpr {
                            let at = ((y * 96 * dpr + x) * 4) as usize;
                            if base[at + 3] == 0 {
                                continue;
                            }
                            for column in 0..if doubled { 2 } else { 1 } {
                                let dest_x = x + (shift + column * 48) * dpr;
                                assert!(dest_x < size[0]);
                                let dest = ((y * size[0] + dest_x) * 4) as usize;
                                expected[dest..dest + 4].copy_from_slice(&base[at..at + 4]);
                            }
                        }
                    }
                    assert_eq!(pixels, expected, "stage {stage}, warm {warm}, DPR {dpr}");
                } else {
                    baseline = Some(pixels);
                }
                // Four distinct Text allocations plus Present's uniform.
                // Graph/pass destruction must not destroy these pooled buffers.
                let buffers = viewport.frame_buffers_for_test();
                assert_eq!(buffers.len(), 5);
                assert!(
                    buffers.iter().all(|(_, buffer)| {
                        released.iter().all(|(_, previous)| buffer != previous)
                    }),
                    "explicit release must allocate new physical buffers"
                );
                for (index, (_, buffer)) in buffers.iter().enumerate() {
                    assert!(buffers[..index].iter().all(|(_, other)| other != buffer));
                }
                let storage_sizes = buffers
                    .iter()
                    .filter(|(_, buffer)| buffer.usage().contains(wgpu::BufferUsages::STORAGE))
                    .map(|(_, buffer)| buffer.size())
                    .collect::<Vec<_>>();
                assert_eq!(storage_sizes, vec![if doubled { 64 } else { 32 }; 2]);
                if let Some(cold) = &cold_buffers {
                    assert_eq!(&buffers, cold, "warm frame must reuse real allocations");
                } else {
                    cold_buffers = Some(buffers);
                }
            }
        }
    }
    eprintln!(
        "Text globals: 2 passes, 6 state pairs, DPR 1/2; each warm frame reuses all 4 Text buffers; pixels verified"
    );
    Ok(())
}
