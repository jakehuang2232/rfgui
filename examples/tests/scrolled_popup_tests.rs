//! A viewport-clipped popup that scrolls, opened inside a scroll container,
//! as a select's menu is.
use rfgui::style::{ClipMode, Color, Layout, Length, Position, ScrollDirection};
use rfgui::time::Instant;
use rfgui::ui::rsx;
use rfgui::view::{Element, Viewport};

#[path = "../../lib/rfgui-components/tests/retained_controls/gpu.rs"]
mod gpu;

/// The popup paints outside the page's scroll raster, since it escapes the
/// page's clip, yet composites with the page's scroll.
#[test]
#[ignore = "requires native hardware graphics adapter"]
fn a_scrolling_popup_paints_inside_a_scroll_container() -> Result<(), String> {
    let gpu = gpu::Gpu::new()?;
    let scene = rsx! {
        <Element style={{
            width: Length::px(200.),
            height: Length::px(200.),
            layout: Layout::flow().column().no_wrap(),
            scroll_direction: ScrollDirection::Vertical,
            background: Color::rgb(40, 40, 40),
        }}>
            <Element style={{ width: Length::px(120.) }}>
                <Element anchor="trigger" style={{
                    width: Length::px(120.),
                    height: Length::px(20.),
                    background: Color::rgb(90, 90, 90),
                }} />
                <Element style={{
                    position: Position::absolute()
                        .anchor("trigger")
                        .top(Length::percent(100.))
                        .left(Length::Zero)
                        .clip(ClipMode::Viewport),
                    width: Length::percent(100.),
                    height: Length::px(80.),
                    scroll_direction: ScrollDirection::Vertical,
                    background: Color::rgb(30, 160, 60),
                }}>
                    <Element style={{ width: Length::px(10.), height: Length::px(400.) }} />
                </Element>
            </Element>
            <Element style={{ width: Length::px(10.), height: Length::px(1000.) }} />
        </Element>
    };
    for dpr in [1_u32, 2] {
        let size = [200 * dpr, 200 * dpr];
        let frame = Viewport::new().render_rsx_offscreen_for_test(
            &scene,
            gpu.device.clone(),
            gpu.queue.clone(),
            size,
            dpr as f32,
            Instant::now(),
        )?;
        let pixels = gpu.read(&frame.texture, size)?;
        let at = |x: u32, y: u32| {
            let i = ((y * dpr) * size[0] + x * dpr) as usize * 4;
            [pixels[i], pixels[i + 1], pixels[i + 2]]
        };
        // The popup spans (0, 20) to (120, 100), below the trigger.
        assert_ne!(at(60, 60), at(160, 60), "dpr {dpr}");
    }
    Ok(())
}
