//! An outer shadow paints wherever its box is drawn.
use rfgui::style::{BoxShadow, Color, Layout, Length, ScrollDirection};
use rfgui::time::Instant;
use rfgui::ui::rsx;
use rfgui::view::viewport::set_scroll_offset_by_id;
use rfgui::view::{Element, Viewport};

#[path = "../../lib/rfgui-components/tests/retained_controls/gpu.rs"]
mod gpu;

fn shadow() -> Vec<BoxShadow> {
    vec![
        BoxShadow::new()
            .color(Color::rgba(0, 0, 0, 160))
            .offset_y(4.0)
            .blur(12.0),
    ]
}

fn pixel(pixels: &[u8], width: u32, dpr: u32, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * dpr) * width + x * dpr) as usize * 4;
    [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
}

/// A scroll container rasters more than it shows. A box near the bottom of
/// the view sits further down that raster than the view is tall, and its
/// shadow still paints.
#[test]
#[ignore = "requires native hardware graphics adapter"]
fn a_shadow_low_in_a_scrolled_raster_paints() {
    let gpu = gpu::Gpu::new().unwrap();
    let scene = rsx! {
        <Element style={{
            width: Length::px(400.),
            height: Length::px(400.),
            layout: Layout::flow().column().no_wrap(),
            scroll_direction: ScrollDirection::Vertical,
            background: Color::rgb(240, 240, 240),
        }}>
            <Element style={{ width: Length::px(10.), height: Length::px(600.) }} />
            <Element style={{
                width: Length::px(100.),
                height: Length::px(40.),
                background: Color::rgb(60, 120, 220),
                box_shadow: shadow(),
            }} />
            <Element style={{ width: Length::px(10.), height: Length::px(1000.) }} />
        </Element>
    };
    for dpr in [1_u32, 2] {
        let size = [400 * dpr, 400 * dpr];
        let mut viewport = Viewport::new();
        let render = |viewport: &mut Viewport| {
            viewport
                .render_rsx_offscreen_for_test(
                    &scene,
                    gpu.device.clone(),
                    gpu.queue.clone(),
                    size,
                    dpr as f32,
                    Instant::now(),
                )
                .unwrap()
        };
        render(&mut viewport);
        let arena = viewport.node_arena();
        let root = arena.roots()[0];
        let id = arena.get(root).unwrap().element.stable_id();
        assert!(set_scroll_offset_by_id(arena, root, id, (0., 250.)));
        let frame = render(&mut viewport);
        let pixels = gpu.read(&frame.texture, size).unwrap();
        let background = pixel(&pixels, size[0], dpr, 300, 370);
        // The box now spans (0, 350) to (100, 390); its shadow reaches past
        // its right edge.
        assert_ne!(
            pixel(&pixels, size[0], dpr, 104, 372),
            background,
            "dpr {dpr}"
        );
    }
}
