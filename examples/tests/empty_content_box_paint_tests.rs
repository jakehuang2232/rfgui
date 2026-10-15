//! A box whose padding and border take its whole size has no content box,
//! but still paints its background and border, as in CSS.
use rfgui::style::{Border, Color, Layout, Length, Padding};
use rfgui::time::Instant;
use rfgui::ui::rsx;
use rfgui::view::{Element, Viewport};

#[path = "../../lib/rfgui-components/tests/retained_controls/gpu.rs"]
mod gpu;

const SIZE: [u32; 2] = [200, 60];
const RED: Color = Color::rgb(255, 0, 0);
const GREEN: Color = Color::rgb(0, 255, 0);
const BLUE: Color = Color::rgb(0, 0, 255);

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn boxes_without_a_content_box_paint_background_and_border() -> Result<(), String> {
    // Left to right, 20 px apart: padding only (20x20), borders that meet
    // in the middle (40x40), and a 4 px border around 16 px of padding.
    let scene = rsx! {
        <Element style={{
            width: Length::px(200.0),
            height: Length::px(60.0),
            layout: Layout::flow().row(),
            gap: Length::px(20.0),
        }}>
            <Element style={{ padding: Padding::uniform(Length::px(10.0)), background: RED }} />
            <Element style={{
                width: Length::px(40.0),
                height: Length::px(40.0),
                background: RED,
                border: Border::uniform(Length::px(20.0), &BLUE),
            }} />
            <Element style={{
                padding: Padding::uniform(Length::px(16.0)),
                background: GREEN,
                border: Border::uniform(Length::px(4.0), &BLUE),
            }} />
        </Element>
    };
    let gpu = gpu::Gpu::new()?;
    let frame = Viewport::new().render_rsx_offscreen_for_test(
        &scene,
        gpu.device.clone(),
        gpu.queue.clone(),
        SIZE,
        1.0,
        Instant::now(),
    )?;
    let pixels = gpu.read(&frame.texture, SIZE)?;
    let rgb = |x: u32, y: u32| {
        let at = ((y * SIZE[0] + x) * 4) as usize;
        [pixels[at], pixels[at + 1], pixels[at + 2]]
    };

    // Edge antialiasing may leave a unit or two of the neighbouring color.
    let assert_near = |label: &str, at: [u32; 2], expected: [u8; 3]| {
        let actual = rgb(at[0], at[1]);
        let off = (0..3).map(|i| actual[i].abs_diff(expected[i])).max();
        assert!(off <= Some(2), "{label}: {actual:?}, expected {expected:?}");
    };
    assert_near("padding-only background", [10, 10], [255, 0, 0]);
    assert_near("borders meeting in the middle", [60, 20], [0, 0, 255]);
    assert_near("border around padding", [102, 20], [0, 0, 255]);
    assert_near("background inside the border", [120, 20], [0, 255, 0]);
    Ok(())
}
