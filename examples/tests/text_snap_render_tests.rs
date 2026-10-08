//! Glyphs snap to the pixel grid the same way at every position, so text
//! moved by whole physical pixels renders rigidly, also where it crosses the
//! viewport's top-left edge.
use rfgui::style::{Color, Length, Padding, Position};
use rfgui::time::Instant;
use rfgui::ui::{RsxNode, rsx};
use rfgui::view::viewport::ViewportPaintRendererMode;
use rfgui::view::{Element, Text, Viewport};

#[path = "../../lib/rfgui-components/tests/retained_controls/gpu.rs"]
mod gpu;

const SIZE: [u32; 2] = [160, 48];

/// Text in an element at `[left, top]`. The element's origin snaps to the
/// pixel grid; its fractional padding keeps the glyphs off the grid.
fn text_at([left, top]: [f32; 2]) -> RsxNode {
    rsx! {
        <Element style={{
            position: Position::absolute().left(Length::px(left)).top(Length::px(top)),
            padding: Padding::new().left(Length::px(0.4)).top(Length::px(0.6)),
        }}>
            <Text style={{ color: Color::rgb(20, 20, 20) }}>Hello, glyphs</Text>
        </Element>
    }
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn text_moved_by_whole_pixels_renders_rigidly_across_the_viewport_edge() -> Result<(), String> {
    const SHIFT: u32 = 10;
    // The first glyphs start above and left of the viewport.
    const AT: [f32; 2] = [-4.0, -6.0];
    let gpu = gpu::Gpu::new()?;
    for mode in [
        ViewportPaintRendererMode::Legacy,
        ViewportPaintRendererMode::RetainedAuto,
    ] {
        for dpr in [1_u32, 2] {
            let size = [SIZE[0] * dpr, SIZE[1] * dpr];
            let render = |at: [f32; 2]| -> Result<Vec<u8>, String> {
                let mut viewport = Viewport::new();
                viewport.set_paint_renderer_mode(mode);
                let frame = viewport.render_rsx_offscreen_for_test(
                    &text_at(at),
                    gpu.device.clone(),
                    gpu.queue.clone(),
                    size,
                    dpr as f32,
                    Instant::now(),
                )?;
                gpu.read(&frame.texture, size)
            };
            let clipped = render(AT)?;
            let inside = render([AT[0] + SHIFT as f32, AT[1] + SHIFT as f32])?;
            let shift = SHIFT * dpr;
            let pixel = |pixels: &[u8], x: u32, y: u32| {
                let at = ((y * size[0] + x) * 4) as usize;
                <[u8; 4]>::try_from(&pixels[at..at + 4]).unwrap()
            };
            let background = pixel(&clipped, size[0] - 1, size[1] - 1);
            let (mut inked, mut mismatched) = (0, Vec::new());
            for y in 0..size[1] - shift {
                for x in 0..size[0] - shift {
                    let expected = pixel(&inside, x + shift, y + shift);
                    inked += usize::from(expected != background);
                    if pixel(&clipped, x, y) != expected {
                        mismatched.push([x, y]);
                    }
                }
            }
            assert!(inked > 0, "{mode:?} DPR {dpr}: no text rendered");
            assert!(
                mismatched.is_empty(),
                "{mode:?} DPR {dpr}: {} pixels moved apart from the rest, first {:?}",
                mismatched.len(),
                &mismatched[..mismatched.len().min(4)]
            );
        }
    }
    Ok(())
}
