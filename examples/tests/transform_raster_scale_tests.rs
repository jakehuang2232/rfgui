//! A scaled surface rasters at the resolution it is displayed at: content
//! scaled far down past the texture limit still shows, and content scaled up
//! keeps sharp edges instead of stretching a smaller raster.
use rfgui::style::{Color, Layout, Length, Position, Scale, Transform};
use rfgui::time::Instant;
use rfgui::ui::{RsxNode, rsx};
use rfgui::view::{Element, Viewport};

#[path = "../../lib/rfgui-components/tests/retained_controls/gpu.rs"]
mod gpu;

const SIZE: [u32; 2] = [400, 400];
const BANDS: [[u8; 3]; 5] = [
    [255, 0, 0],
    [0, 255, 0],
    [0, 0, 255],
    [255, 255, 0],
    [0, 255, 255],
];

/// `bands` side by side, each `band` wide and `height` tall, scaled by
/// `scale` about their common center, which sits at the viewport center.
fn scaled_bands(bands: &[[u8; 3]], band: f32, height: f32, scale: f32) -> RsxNode {
    let width = band * bands.len() as f32;
    rsx! {
        <Element style={{width: Length::px(400.), height: Length::px(400.)}}>
            <Element style={{
                position: Position::absolute()
                    .left(Length::px(200. - width / 2.))
                    .top(Length::px(200. - height / 2.)),
                width: Length::px(width),
                height: Length::px(height),
                layout: Layout::flow().row(),
                transform: Transform::new([Scale::uniform(scale)]),
            }}>
                {bands.iter().map(|&[r, g, b]| rsx! {
                    <Element style={{
                        width: Length::px(band),
                        height: Length::px(height),
                        background: Color::rgb(r, g, b),
                    }} />
                }).collect::<Vec<_>>()}
            </Element>
        </Element>
    }
}

fn render(gpu: &gpu::Gpu, tree: &RsxNode, dpr: u32) -> Result<Vec<u8>, String> {
    let size = SIZE.map(|n| n * dpr);
    let frame = Viewport::new().render_rsx_offscreen_for_test(
        tree,
        gpu.device.clone(),
        gpu.queue.clone(),
        size,
        dpr as f32,
        Instant::now(),
    )?;
    gpu.read(&frame.texture, size)
}

/// The pixel at physical `[x, y]` of a frame rendered at `dpr`.
fn physical_pixel(pixels: &[u8], dpr: u32, [x, y]: [u32; 2]) -> [u8; 4] {
    let at = ((y * SIZE[0] * dpr + x) * 4) as usize;
    pixels[at..at + 4].try_into().unwrap()
}

/// Five 10000px wide, 40000px tall bands scaled to 1% show as five 100px
/// bands, centered: band `k` spans `[-50 + 100k, 50 + 100k)`. The content
/// rasters at 1% of the device scale, far inside the texture limit.
#[test]
#[ignore = "requires native hardware graphics adapter"]
fn content_scaled_far_down_past_the_texture_limit_shows_its_bands() -> Result<(), String> {
    let gpu = gpu::Gpu::new()?;
    for dpr in [1_u32, 2] {
        let pixels = render(&gpu, &scaled_bands(&BANDS, 10000., 40000., 0.01), dpr)?;
        for (band, x) in [(0, 10), (1, 100), (2, 200), (3, 300), (4, 390)] {
            let [r, g, b] = BANDS[band];
            assert_eq!(
                physical_pixel(&pixels, dpr, [x * dpr, 200 * dpr]),
                [r, g, b, 255],
                "DPR {dpr}: band {band} at x {x}"
            );
        }
    }
    Ok(())
}

/// A 100px square, red then blue halves, scaled 3x paints what the same
/// halves laid out 3x larger paint: edges one device pixel wide, as at their
/// own size. A raster at the device scale stretched 3x would smear each edge
/// across three.
#[test]
#[ignore = "requires native hardware graphics adapter"]
fn content_scaled_up_paints_like_content_laid_out_that_large() -> Result<(), String> {
    let gpu = gpu::Gpu::new()?;
    let halves = [[255, 0, 0], [0, 0, 255]];
    for dpr in [1_u32, 2] {
        let scaled = render(&gpu, &scaled_bands(&halves, 50., 100., 3.), dpr)?;
        let laid_out = render(&gpu, &scaled_bands(&halves, 150., 300., 1.), dpr)?;
        let differing = scaled
            .chunks_exact(4)
            .zip(laid_out.chunks_exact(4))
            .filter(|(a, b)| a.iter().zip(b.iter()).any(|(a, b)| a.abs_diff(*b) > 1))
            .count();
        assert_eq!(differing, 0, "DPR {dpr}");
        assert_eq!(
            physical_pixel(&scaled, dpr, [200 * dpr - 2, 200 * dpr]),
            [255, 0, 0, 255]
        );
        assert_eq!(
            physical_pixel(&scaled, dpr, [200 * dpr + 1, 200 * dpr]),
            [0, 0, 255, 255]
        );
    }
    Ok(())
}
