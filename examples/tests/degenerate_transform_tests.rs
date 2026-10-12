//! A transformed plane whose corner reaches the eye plane has no 2D quad. It
//! is not composited, and the frame around it still renders through the
//! retained path instead of being rejected.
use rfgui::style::{Angle, Color, Length, Perspective, Position, Rotate, Transform};
use rfgui::time::Instant;
use rfgui::ui::{RsxNode, rsx};
use rfgui::view::{Element, Viewport};

#[path = "../../lib/rfgui-components/tests/retained_controls/gpu.rs"]
mod gpu;

/// A 100px plane with a child, tilted about X behind a 50px perspective, on a
/// green 200px root.
fn scene(degrees: f32) -> RsxNode {
    rsx! {
        <Element style={{
            width: Length::px(200.),
            height: Length::px(200.),
            background: Color::rgb(0, 255, 0),
        }}>
            <Element style={{
                position: Position::absolute().left(Length::px(50.)).top(Length::px(50.)),
                width: Length::px(100.),
                height: Length::px(100.),
                background: Color::rgb(255, 0, 0),
                transform: Transform::new([Perspective::px(50.), Rotate::x(Angle::deg(degrees))]),
            }}>
                <Element style={{
                    width: Length::px(40.),
                    height: Length::px(40.),
                    background: Color::rgb(0, 0, 255),
                }} />
            </Element>
        </Element>
    }
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn plane_reaching_the_eye_plane_is_not_composited() {
    let gpu = gpu::Gpu::new().unwrap();
    for dpr in [1_u32, 2] {
        let size = [200 * dpr, 200 * dpr];
        let frame = Viewport::new()
            .render_rsx_offscreen_for_test(
                &scene(89.99),
                gpu.device.clone(),
                gpu.queue.clone(),
                size,
                dpr as f32,
                Instant::now(),
            )
            .unwrap();
        let pixels = gpu.read(&frame.texture, size).unwrap();
        assert!(
            pixels
                .chunks_exact(4)
                .all(|p| p[0] == 0 && p[1] == 255 && p[2] == 0),
            "dpr {dpr}: only the root background is drawn"
        );
    }
}
