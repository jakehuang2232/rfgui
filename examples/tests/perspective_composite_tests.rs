//! A surface turned in perspective keeps straight lines straight: its
//! texture is interpolated perspective-correctly across the whole quad,
//! not affinely per triangle.
use rfgui::style::{
    Angle, Color, Layout, Length, Padding, Perspective, Rotate, Transform, TransformOrigin,
};
use rfgui::time::Instant;
use rfgui::ui::{RsxNode, rsx};
use rfgui::view::{Element, Viewport};

#[path = "../../lib/rfgui-components/tests/retained_controls/gpu.rs"]
mod gpu;

const SIZE: [u32; 2] = [400, 300];

/// A full-size surface turned 40 degrees about its vertical axis, holding
/// one red bar low and to the left of center, across the quad's diagonal.
fn scene() -> RsxNode {
    rsx! {
        <Element style={{
            width: Length::px(400.0),
            height: Length::px(300.0),
            padding: Padding::uniform(Length::px(40.0)),
            layout: Layout::flow().column(),
            transform_origin: TransformOrigin::center(),
            transform: Transform::new([Perspective::px(600.0), Rotate::y(Angle::deg(40.0))]),
        }}>
            <Element style={{width: Length::px(10.0), height: Length::px(170.0)}} />
            <Element style={{
                width: Length::px(300.0),
                height: Length::px(10.0),
                background: Color::rgb(255, 0, 0),
            }} />
        </Element>
    }
}

/// The largest distance, in physical pixels, of the red bar's center line
/// from the straight line through its two ends.
fn bar_bend(pixels: &[u8], [width, height]: [u32; 2]) -> Option<f32> {
    let centers = (0..width)
        .filter_map(|x| {
            let (mut sum, mut weight) = (0.0, 0.0);
            for y in 0..height {
                let p = &pixels[((y * width + x) * 4) as usize..][..4];
                if p[0] > p[1] && p[0] > p[2] {
                    let w = f32::from(p[0]) / 255.0;
                    sum += w * y as f32;
                    weight += w;
                }
            }
            (weight > 0.0).then(|| (x as f32, sum / weight))
        })
        .collect::<Vec<_>>();
    // Ends are partly covered; measure between columns a few pixels in.
    let inner = centers.get(3..centers.len().checked_sub(3)?)?;
    let (&(x0, y0), &(x1, y1)) = (inner.first()?, inner.last()?);
    Some(
        inner
            .iter()
            .map(|&(x, y)| (y - (y0 + (y1 - y0) * (x - x0) / (x1 - x0))).abs())
            .fold(0.0, f32::max),
    )
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn surface_turned_in_perspective_keeps_lines_straight() -> Result<(), String> {
    let gpu = gpu::Gpu::new()?;
    for dpr in [1_u32, 2] {
        let size = SIZE.map(|n| n * dpr);
        let frame = Viewport::new().render_rsx_offscreen_for_test(
            &scene(),
            gpu.device.clone(),
            gpu.queue.clone(),
            size,
            dpr as f32,
            Instant::now(),
        )?;
        let pixels = gpu.read(&frame.texture, size)?;
        // Rotated edges step a pixel at a time, so a straight bar still
        // wanders by under a pixel; the per-triangle kink bent it by 17.
        let bend = bar_bend(&pixels, size).ok_or("no red bar")?;
        assert!(bend <= 2.0, "DPR {dpr}: the bar bends {bend}px");
    }
    Ok(())
}
