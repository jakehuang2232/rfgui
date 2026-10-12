//! Surfaces larger than the texture limit raster only the part their receiver
//! can show: an opacity group taller than the limit at the root, and a rotated
//! bar wider than the limit at the root, inside scrolled content, and inside
//! such an opacity group, as is another group as tall.
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

/// The colored bands, each `width` wide.
fn bands(width: f32) -> Vec<RsxNode> {
    BANDS
        .iter()
        .map(|&[r, g, b]| {
            rsx! {
                <Element style={{
                    width: Length::px(width),
                    height: Length::px(BAND_HEIGHT),
                    background: Color::rgb(r, g, b),
                }} />
            }
        })
        .collect()
}

/// A 300px wide opacity group of `content`, placed `top` pixels from the
/// viewport top.
fn group_at(top: f32, content: Vec<RsxNode>) -> RsxNode {
    rsx! {
        <Element style={{
            position: Position::absolute().left(Length::px(0.)).top(Length::px(top)),
            width: Length::px(300.),
            layout: Layout::flow().column(),
            opacity: Opacity::new(0.5),
        }}>
            {content}
        </Element>
    }
}

/// A 10000px tall opacity group of colored bands.
fn tall_group(top: f32) -> RsxNode {
    group_at(top, bands(300.))
}

/// The bands in an opacity group of their own, inside the outer group.
fn nested_tall_groups(top: f32) -> RsxNode {
    group_at(
        top,
        vec![rsx! {
            <Element style={{
                width: Length::px(300.),
                layout: Layout::flow().column(),
                opacity: Opacity::new(0.5),
            }}>
                {bands(300.)}
            </Element>
        }],
    )
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

/// The bar's slot at the top of a 10000px tall opacity group.
fn bar_in_tall_group(width: f32) -> RsxNode {
    group_at(
        -150.,
        vec![
            rsx! {<Element style={{width: Length::px(300.), height: Length::px(300.)}} />},
            rsx! {
                <Element style={{width: Length::px(400.), height: Length::px(100.)}}>
                    {rotated_bar(width)}
                </Element>
            },
            rsx! {<Element style={{width: Length::px(300.), height: Length::px(9600.)}} />},
        ],
    )
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
    render_counting_rasters(gpu, viewport, tree, dpr).map(|(pixels, _)| pixels)
}

/// The frame's pixels and how many resident rasters it composited.
fn render_counting_rasters(
    gpu: &gpu::Gpu,
    viewport: &mut Viewport,
    tree: &RsxNode,
    dpr: u32,
) -> Result<(Vec<u8>, usize), String> {
    let size = SIZE.map(|n| n * dpr);
    let frame = viewport.render_rsx_offscreen_for_test(
        tree,
        gpu.device.clone(),
        gpu.queue.clone(),
        size,
        dpr as f32,
        Instant::now(),
    )?;
    Ok((gpu.read(&frame.texture, size)?, frame.resident_rasters))
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

/// `scene` at two placements in one viewport, so the second frame re-windows,
/// shows the bands in view at `alpha` from `rasters` resident rasters.
fn assert_bands_in_view(
    scene: fn(f32) -> RsxNode,
    alpha: u8,
    rasters: usize,
) -> Result<(), String> {
    let gpu = gpu::Gpu::new()?;
    for dpr in [1_u32, 2] {
        let mut viewport = Viewport::new();
        for top in [-3900., -1900.] {
            let (pixels, resident) =
                render_counting_rasters(&gpu, &mut viewport, &scene(top), dpr)?;
            assert_eq!(resident, rasters, "DPR {dpr} top {top}");
            let band = |y: f32| BANDS[((y - top) / BAND_HEIGHT) as usize];
            for y in [50, 250, 395] {
                let [r, g, b] = band(y as f32);
                assert_near(
                    pixel(&pixels, dpr, [150, y]),
                    [r, g, b, alpha],
                    &format!("DPR {dpr} top {top} y {y}"),
                );
            }
            assert_eq!(pixel(&pixels, dpr, [350, 200]), [0, 0, 0, 0]);
        }
    }
    Ok(())
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn root_opacity_group_taller_than_the_texture_limit_shows_the_bands_in_view() -> Result<(), String>
{
    assert_bands_in_view(tall_group, 128, 1)
}

/// The inner group's receiver is the outer group with no clip between them:
/// it shows only what the outer group shows.
#[test]
#[ignore = "requires native hardware graphics adapter"]
fn opacity_group_nested_in_one_as_tall_shows_the_bands_in_view() -> Result<(), String> {
    assert_bands_in_view(nested_tall_groups, 64, 2)
}

/// Two of the bands' groups side by side at the top of a scroll container's
/// content. Two nested surfaces keep the content raster, so each group's
/// receiver is that raster, with no clip between them.
fn groups_in_scroll() -> RsxNode {
    let group = || {
        rsx! {
            <Element style={{
                width: Length::px(150.),
                layout: Layout::flow().column(),
                opacity: Opacity::new(0.5),
            }}>
                {bands(150.)}
            </Element>
        }
    };
    rsx! {
        <Element style={{
            width: Length::px(400.),
            height: Length::px(400.),
            layout: Layout::flow().column(),
            scroll_direction: ScrollDirection::Vertical,
        }}>
            <Element style={{width: Length::px(300.), layout: Layout::flow().row()}}>
                {group()}
                {group()}
            </Element>
        </Element>
    }
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn opacity_group_taller_than_the_texture_limit_in_scrolled_content_shows_the_bands_in_view()
-> Result<(), String> {
    let gpu = gpu::Gpu::new()?;
    for dpr in [1_u32, 2] {
        let mut viewport = Viewport::new();
        render(&gpu, &mut viewport, &groups_in_scroll(), dpr)?;
        for offset in [3900., 1900.] {
            let arena = viewport.node_arena();
            let root = arena.roots()[0];
            let id = arena.get(root).unwrap().element.stable_id();
            assert!(set_scroll_offset_by_id(arena, root, id, (0., offset)));
            let pixels = render(&gpu, &mut viewport, &groups_in_scroll(), dpr)?;
            let band = |y: f32| BANDS[((y + offset) / BAND_HEIGHT) as usize];
            for y in [50, 250, 395] {
                let [r, g, b] = band(y as f32);
                for x in [75, 225] {
                    assert_near(
                        pixel(&pixels, dpr, [x, y]),
                        [r, g, b, 128],
                        &format!("DPR {dpr} offset {offset} ({x}, {y})"),
                    );
                }
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
    center_color: [u8; 4],
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
            assert_near(
                pixel(&pixels, dpr, center),
                center_color,
                &format!("width {width} DPR {dpr}: the bar center"),
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
    assert_wide_bar_matches_narrow(bar_at_root, None, [200, 200], [255, 0, 0, 255])
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn rotated_bar_wider_than_the_texture_limit_in_scrolled_content_paints_like_a_narrow_one()
-> Result<(), String> {
    assert_wide_bar_matches_narrow(bar_in_scroll, Some(500.), [200, 150], [255, 0, 0, 255])
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn rotated_bar_wider_than_the_texture_limit_in_a_tall_opacity_group_paints_like_a_narrow_one()
-> Result<(), String> {
    assert_wide_bar_matches_narrow(bar_in_tall_group, None, [200, 200], [255, 0, 0, 128])
}
