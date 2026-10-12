//! A transformed element rasters into its own surface. Its outer shadow paints
//! past its border box, so the surface must cover the shadow too.
use rfgui::style::{BorderRadius, BoxShadow, Color, Length, Position, Scale, Transform};
use rfgui::time::Instant;
use rfgui::ui::{RsxNode, rsx};
use rfgui::view::{Element, Viewport};

#[path = "../../lib/rfgui-components/tests/retained_controls/gpu.rs"]
mod gpu;

/// A 36px dot centered at (100, 100) on a 200px root, with a 28px glow.
fn scene(scale: f32) -> RsxNode {
    rsx! {
        <Element style={{
            width: Length::px(200.),
            height: Length::px(200.),
            background: Color::rgb(40, 30, 50),
        }}>
            <Element style={{
                position: Position::absolute().left(Length::px(82.)).top(Length::px(82.)),
                width: Length::px(36.),
                height: Length::px(36.),
                border_radius: BorderRadius::uniform(Length::px(18.)),
                background: Color::rgb(220, 110, 80),
                transform: Transform::new([Scale::uniform(scale)]),
                box_shadow: vec![BoxShadow::new().color(Color::rgba(220, 110, 80, 200)).blur(28.0)],
            }} />
        </Element>
    }
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn transformed_outer_shadow_paints_past_the_border_box() {
    let gpu = gpu::Gpu::new().unwrap();
    for scale in [1.0, 1.2] {
        for dpr in [1_u32, 2] {
            let size = [200 * dpr, 200 * dpr];
            let frame = Viewport::new()
                .render_rsx_offscreen_for_test(
                    &scene(scale),
                    gpu.device.clone(),
                    gpu.queue.clone(),
                    size,
                    dpr as f32,
                    Instant::now(),
                )
                .unwrap();
            let pixels = gpu.read(&frame.texture, size).unwrap();
            let at = |x: u32, y: u32| {
                let i = ((y * dpr) * size[0] + x * dpr) as usize * 4;
                [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
            };
            let background = at(2, 2);
            // 30px from center lies outside even the scaled border box
            // (21.6px) and well inside the glow.
            assert_ne!(at(130, 100), background, "scale {scale} dpr {dpr}");
            assert_ne!(at(100, 130), background, "scale {scale} dpr {dpr}");
        }
    }
}
