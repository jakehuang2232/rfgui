use super::super::style_pipeline_tests::read_submitted_texture;
use super::*;
use crate::style::{Border, FontSize, Length, Padding, VerticalAlign};
use crate::view::test_support::{commit_element, get_element};
use crate::view::viewport::ViewportPaintRendererMode;

const SIZE: [u32; 2] = [342, 240];
const CONTENT: &str = "First line with a long value that can wrap when auto wrap is enabled.{{API_HOST}}/v1/users/{{USER_ID}}/activity/with/a/very/long/path\nTail line";

fn scene(align: VerticalAlign, wrap: bool) -> (Viewport, NodeKey) {
    let mut area = TextArea::new();
    area.content = CONTENT.into();
    area.font_size = 14.0;
    area.line_height = 1.25;
    area.color = Color::rgb(255, 0, 0);
    area.vertical_align = align;
    area.auto_wrap = wrap;
    area.on_render_handler = Some(crate::ui::on_text_area_render(|render| {
        for token in ["{{API_HOST}}", "{{USER_ID}}"] {
            let start = CONTENT.find(token).unwrap();
            render.range(start..start + token.len(), move |_| {
                crate::ui::rsx! {
                    <crate::view::Element style={{
                        font_size: FontSize::Px(24.0),
                        color: Color::rgb(255, 255, 255),
                        background: Color::rgb(0, 0, 128),
                        padding: Padding::uniform(Length::px(0.0)).x(Length::px(20.0)),
                        border: Border::uniform(Length::px(1.0), &Color::rgb(0, 0, 255)),
                    }}>
                        <crate::view::Text>{token}</crate::view::Text>
                    </crate::view::Element>
                }
            });
        }
    }));
    let mut arena = NodeArena::new();
    let root = commit_element(&mut arena, Box::new(area));
    arena.with_element_taken(root, |el, _| {
        el.as_any_mut()
            .downcast_mut::<TextArea>()
            .unwrap()
            .set_self_node_key(root)
    });
    let mut viewport = Viewport::new();
    viewport.install_single_viewport_scene_for_test(arena, root);
    (viewport, root)
}

fn assert_pixels(
    viewport: &Viewport,
    owner: NodeKey,
    pixels: &[u8],
    align: VerticalAlign,
    dpr: u32,
) {
    let area = get_element::<TextArea>(viewport.node_arena(), owner);
    let package = area
        .unified_inline_ifc_render_package(viewport.node_arena())
        .unwrap();
    let lines = package.visual_line_rects();
    let mut expected = Vec::new();
    for segment in &package.source_segments {
        for (index, text) in package.ifc.source_text_line_rects(segment.source) {
            let line = lines[index];
            let y = match align {
                VerticalAlign::Top => line.y,
                VerticalAlign::Middle => line.y + (line.height - text.height) / 2.0,
                VerticalAlign::Bottom => line.y + line.height - text.height,
                VerticalAlign::Baseline => {
                    let snapshot = package.ifc.text_layout_snapshot_ref();
                    let offset = snapshot
                        .lines
                        .iter()
                        .map(|line| line.y)
                        .fold(0.0_f32, f32::min);
                    text.y - offset
                }
            };
            expected.push(crate::ui::Rect::new(text.x, y, text.width, text.height));
        }
    }
    let mut counts = vec![0; expected.len()];
    for (i, pixel) in pixels.chunks_exact(4).enumerate() {
        if pixel[0] < 200 || pixel[1] > 20 || pixel[2] > 20 || pixel[3] < 100 {
            continue;
        }
        let x = (i as u32 % (SIZE[0] * dpr)) as f32 / dpr as f32;
        let y = (i as u32 / (SIZE[0] * dpr)) as f32 / dpr as f32;
        let mut covered = false;
        for (rect, count) in expected.iter().zip(&mut counts) {
            // Permit one physical pixel of snapping/antialiasing at each edge.
            let epsilon = 1.0 / dpr as f32;
            if x >= rect.x - epsilon
                && x <= rect.x + rect.width + epsilon
                && y >= rect.y - epsilon
                && y <= rect.y + rect.height + epsilon
            {
                *count += 1;
                covered = true;
            }
        }
        assert!(
            covered,
            "{align:?} DPR={dpr}: glyph pixel ({x},{y}) escaped its aligned text bands: {expected:?}"
        );
    }
    for (rect, count) in expected.iter().zip(counts) {
        if rect.x >= 0.0 && rect.x + rect.width <= SIZE[0] as f32 && rect.width > 10.0 {
            assert!(
                count > 5,
                "{align:?} DPR={dpr}: expected glyphs in {rect:?}, found {count}"
            );
        }
    }
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_projected_text_area_vertical_alignment_has_independent_pixel_bands() -> Result<(), String>
{
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().unwrap();
    let _cleanup = TextGpuCleanup;
    for mode in [
        ViewportPaintRendererMode::Legacy,
        ViewportPaintRendererMode::RetainedAuto,
    ] {
        for dpr in [1, 2] {
            for align in [
                VerticalAlign::Baseline,
                VerticalAlign::Top,
                VerticalAlign::Middle,
                VerticalAlign::Bottom,
            ] {
                for wrap in [true, false] {
                    let (mut viewport, owner) = scene(align, wrap);
                    viewport.set_paint_renderer_mode(mode);
                    let mut previous = None;
                    for frame in 0..2 {
                        viewport.begin_offscreen_test_frame(
                            gpu.device.clone(),
                            gpu.queue.clone(),
                            SIZE[0] * dpr,
                            SIZE[1] * dpr,
                            FORMAT,
                        )?;
                        viewport.set_scale_factor(dpr as f32);
                        let observed = viewport.render_single_viewport_scene_for_test()
                            .map_err(|error| format!("{mode:?} {align:?} DPR={dpr} wrap={wrap} frame={frame}: {error}"))?;
                        let pixels =
                            read_submitted_texture(&observed.texture, gpu, SIZE.map(|v| v * dpr))?;
                        assert_pixels(&viewport, owner, &pixels, align, dpr);
                        if let Some(previous) = previous {
                            assert_eq!(pixels, previous, "stable aligned frame");
                        }
                        previous = Some(pixels);
                    }
                }
            }
        }
    }
    Ok(())
}
