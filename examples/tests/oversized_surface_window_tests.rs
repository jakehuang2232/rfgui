//! Surfaces larger than the texture limit raster only the part their receiver
//! can show: an opacity group taller than the limit at the root, and a rotated
//! bar wider than the limit at the root and inside scrolled content.
use rfgui::style::{
    Angle, Color, Layout, Length, Opacity, Position, Rotate, ScrollDirection, Transform,
};
use rfgui::time::Instant;
use rfgui::ui::{RsxNode, rsx};
use rfgui::view::viewport::set_scroll_offset_by_id;
use rfgui::view::{Element, Viewport};

#[path = "../../lib/rfgui-components/tests/retained_controls/gpu.rs"]
mod gpu;

const SIZE: [u32; 2] = [400, 400];
const BAND_HEIGHT: f32 = 2000.;
const BANDS: [[u8; 3]; 5] = [
    [255, 0, 0],
    [0, 255, 0],
    [0, 0, 255],
    [255, 255, 0],
    [0, 255, 255],
];

/// A 300px wide, 10000px tall opacity group of colored bands, placed `top`
/// pixels from the viewport top.
fn tall_group(top: f32) -> RsxNode {
    rsx! {
        <Element style={{
            position: Position::absolute().left(Length::px(0.)).top(Length::px(top)),
            width: Length::px(300.),
            layout: Layout::flow().column(),
            opacity: Opacity::new(0.5),
        }}>
            {BANDS.iter().map(|&[r, g, b]| rsx! {
                <Element style={{
                    width: Length::px(300.),
                    height: Length::px(BAND_HEIGHT),
                    background: Color::rgb(r, g, b),
                }} />
            }).collect::<Vec<_>>()}
        </Element>
    }
}

/// A 100px tall red bar `width` wide, rotated 10 degrees about its center,
/// which sits at (200, 50) in its 400x100 slot.
fn rotated_bar(width: f32) -> RsxNode {
    rsx! {
        <Element style={{
            position: Position::absolute().left(Length::px(200. - width / 2.)).top(Length::px(0.)),
            width: Length::px(width),
            height: Length::px(100.),
            background: Color::rgb(255, 0, 0),
            transform: Transform::new([Rotate::z(Angle::deg(10.0))]),
        }} />
    }
}

fn bar_at_root(width: f32) -> RsxNode {
    rsx! {
        <Element style={{width: Length::px(400.), height: Length::px(400.)}}>
            <Element style={{
                position: Position::absolute().left(Length::px(0.)).top(Length::px(150.)),
                width: Length::px(400.),
                height: Length::px(100.),
            }}>
                {rotated_bar(width)}
            </Element>
        </Element>
    }
}

/// The bar's slot sits 600px down a scroll container.
fn bar_in_scroll(width: f32) -> RsxNode {
    rsx! {
        <Element style={{
            width: Length::px(400.),
            height: Length::px(400.),
            layout: Layout::flow().column(),
            scroll_direction: ScrollDirection::Vertical,
        }}>
            <Element style={{width: Length::px(400.), height: Length::px(600.)}} />
            <Element style={{width: Length::px(400.), height: Length::px(100.)}}>
                {rotated_bar(width)}
            </Element>
            <Element style={{width: Length::px(400.), height: Length::px(600.)}} />
        </Element>
    }
}

fn render(
    gpu: &gpu::Gpu,
    viewport: &mut Viewport,
    tree: &RsxNode,
    dpr: u32,
) -> Result<Vec<u8>, String> {
    let size = SIZE.map(|n| n * dpr);
    let frame = viewport.render_rsx_offscreen_for_test(
        tree,
        gpu.device.clone(),
        gpu.queue.clone(),
        size,
        dpr as f32,
        Instant::now(),
    )?;
    assert!(frame.artifact_selected);
    gpu.read(&frame.texture, size)
}

fn pixel(pixels: &[u8], dpr: u32, [x, y]: [u32; 2]) -> [u8; 4] {
    let at = ((y * dpr * SIZE[0] * dpr + x * dpr) * 4) as usize;
    pixels[at..at + 4].try_into().unwrap()
}

fn assert_near(actual: [u8; 4], expected: [u8; 4], case: &str) {
    assert!(
        actual.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 1),
        "{case}: actual {actual:?}, expected {expected:?}"
    );
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn root_opacity_group_taller_than_the_texture_limit_shows_the_bands_in_view() -> Result<(), String>
{
    let gpu = gpu::Gpu::new()?;
    for dpr in [1_u32, 2] {
        let mut viewport = Viewport::new();
        // The same viewport moves the group, so the second frame re-windows.
        for top in [-3900., -1900.] {
            let pixels = render(&gpu, &mut viewport, &tall_group(top), dpr)?;
            let band = |y: f32| BANDS[((y - top) / BAND_HEIGHT) as usize];
            for y in [50, 250, 395] {
                let [r, g, b] = band(y as f32);
                assert_near(
                    pixel(&pixels, dpr, [150, y]),
                    [r, g, b, 128],
                    &format!("DPR {dpr} top {top} y {y}"),
                );
            }
            assert_eq!(pixel(&pixels, dpr, [350, 200]), [0, 0, 0, 0]);
        }
    }
    Ok(())
}

/// Whether the pixel at `index` borders both painted and empty pixels.
fn on_edge(pixels: &[u8], width: u32, index: usize) -> bool {
    let (x, y) = ((index as u32 % width) as i64, (index as u32 / width) as i64);
    let height = (pixels.len() / 4) as i64 / i64::from(width);
    let alphas = (-1..=1).flat_map(|dy| (-1..=1).map(move |dx| (x + dx, y + dy)));
    let alphas = alphas
        .filter(|&(x, y)| x >= 0 && y >= 0 && x < i64::from(width) && y < height)
        .map(|(x, y)| pixels[((y * i64::from(width) + x) * 4 + 3) as usize])
        .collect::<Vec<_>>();
    alphas.contains(&0) && alphas.iter().any(|&a| a > 0)
}

/// A bar wider than the texture limit must paint, inside the viewport, what a
/// bar with the same center that fits the limit paints. The composite quad
/// has hard edges built from different corners, so a pixel centered on an
/// edge may land on either side; only such edge pixels may differ.
fn assert_wide_bar_matches_narrow(
    scene: fn(f32) -> RsxNode,
    scroll: Option<f32>,
    center: [u32; 2],
) -> Result<(), String> {
    let gpu = gpu::Gpu::new()?;
    for dpr in [1_u32, 2] {
        let mut frames = Vec::new();
        for width in [9000., 1000.] {
            let mut viewport = Viewport::new();
            let mut pixels = render(&gpu, &mut viewport, &scene(width), dpr)?;
            if let Some(offset) = scroll {
                let arena = viewport.node_arena();
                let root = arena.roots()[0];
                let id = arena.get(root).unwrap().element.stable_id();
                assert!(set_scroll_offset_by_id(arena, root, id, (0., offset)));
                pixels = render(&gpu, &mut viewport, &scene(width), dpr)?;
            }
            assert_eq!(
                pixel(&pixels, dpr, center),
                [255, 0, 0, 255],
                "width {width} DPR {dpr}: the bar center"
            );
            frames.push(pixels);
        }
        let differing = frames[0]
            .chunks_exact(4)
            .zip(frames[1].chunks_exact(4))
            .enumerate()
            .filter(|(_, (a, b))| a.iter().zip(b.iter()).any(|(a, b)| a.abs_diff(*b) > 2))
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        let width = SIZE[0] * dpr;
        assert!(
            differing
                .iter()
                .all(|&index| on_edge(&frames[1], width, index)),
            "wide and narrow bars differ inside the bar at DPR {dpr}: {differing:?}"
        );
        assert!(
            differing.len() <= 4,
            "too many edge pixels differ at DPR {dpr}: {differing:?}"
        );
    }
    Ok(())
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn rotated_bar_wider_than_the_texture_limit_paints_like_a_narrow_one() -> Result<(), String> {
    assert_wide_bar_matches_narrow(bar_at_root, None, [200, 200])
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn rotated_bar_wider_than_the_texture_limit_in_scrolled_content_paints_like_a_narrow_one()
-> Result<(), String> {
    assert_wide_bar_matches_narrow(bar_in_scroll, Some(500.), [200, 150])
}
