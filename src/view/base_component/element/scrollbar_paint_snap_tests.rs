use super::*;

#[test]
fn scrollbar_rect_position_applies_paint_offset_without_changing_size() {
    let mut ctx = UiBuildContext::new(100, 100, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    ctx.translate_paint_offset(-0.3, 0.45);

    let rect = Rect {
        x: 91.25,
        y: 4.5,
        width: 7.0,
        height: 80.25,
    };
    let position = ctx.paint_point(rect.x, rect.y);

    assert!((position[0] - 90.95).abs() < 0.001);
    assert!((position[1] - 4.95).abs() < 0.001);
    assert!((rect.width - 7.0).abs() < 0.001);
    assert!((rect.height - 80.25).abs() < 0.001);
}
