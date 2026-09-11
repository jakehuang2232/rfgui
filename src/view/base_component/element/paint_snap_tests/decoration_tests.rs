use super::*;

#[test]
fn self_decoration_ops_use_fixed_fill_and_border_slots() {
    let mut element = Element::new(0.0, 0.0, 40.0, 20.0);

    let fill_only = element.self_decoration_paint_ops(1.0, [0.0, 0.0]);
    assert_eq!(fill_only.test_len(), 1);
    let mut fill_only = fill_only.into_iter();
    assert!(matches!(
        fill_only.next().map(|op| op.mode),
        Some(RectRenderMode::FillOnly)
    ));
    assert!(fill_only.next().is_none());

    element.border_widths.left = 1.0;
    let fill_and_border = element.self_decoration_paint_ops(1.0, [0.0, 0.0]);
    assert_eq!(fill_and_border.test_len(), 2);
    let mut fill_and_border = fill_and_border.into_iter();
    assert!(matches!(
        fill_and_border.next().map(|op| op.mode),
        Some(RectRenderMode::FillOnly)
    ));
    assert!(matches!(
        fill_and_border.next().map(|op| op.mode),
        Some(RectRenderMode::BorderOnly)
    ));
    assert!(fill_and_border.next().is_none());
}

#[test]
fn box_shadow_mesh_origin_applies_paint_offset_without_changing_geometry() {
    let mut ctx = UiBuildContext::new(100, 100, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    ctx.translate_paint_offset(0.4, -0.6);

    let fragment = Rect {
        x: 10.25,
        y: 20.75,
        width: 30.5,
        height: 40.25,
    };
    let spread = 2.5;
    let [shadow_x, shadow_y] = ctx.paint_point(fragment.x - spread, fragment.y - spread);

    assert!((shadow_x - 8.15).abs() < 0.001);
    assert!((shadow_y - 17.65).abs() < 0.001);
    assert!((fragment.width + spread * 2.0 - 35.5).abs() < 0.001);
    assert!((fragment.height + spread * 2.0 - 45.25).abs() < 0.001);
}

#[test]
fn fragmented_inline_outer_shadow_prepares_one_op_per_fragment_in_order() {
    let mut element = Element::new(0.0, 0.0, 80.0, 20.0);
    let mut style = crate::style::Style::new();
    style.insert(
        crate::style::PropertyId::Layout,
        crate::style::ParsedValue::Layout(crate::style::Layout::Inline),
    );
    style.insert(
        crate::style::PropertyId::Width,
        crate::style::ParsedValue::Auto,
    );
    style.insert(
        crate::style::PropertyId::Height,
        crate::style::ParsedValue::Auto,
    );
    style.set_box_shadow(vec![crate::style::BoxShadow::new().offset_x(1.0)]);
    element.apply_style(style);
    element.inline_paint_fragments = vec![
        Rect {
            x: 0.0,
            y: 0.0,
            width: 40.0,
            height: 10.0,
        },
        Rect {
            x: 0.0,
            y: 10.0,
            width: 30.0,
            height: 10.0,
        },
    ];

    let prepared = element
        .prepared_outer_shadow_ops(crate::view::paint::PaintRecordingContext::default())
        .expect("finite inline fragments use the typed shadow grammar");
    assert_eq!(prepared.len(), 2);
    let min_y = prepared
        .iter()
        .map(|shadow| {
            shadow
                .mesh
                .vertices
                .iter()
                .map(|vertex| vertex[1])
                .fold(f32::INFINITY, f32::min)
        })
        .collect::<Vec<_>>();
    assert_eq!(min_y, vec![0.0, 10.0]);
}
