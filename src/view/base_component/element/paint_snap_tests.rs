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
        .prepared_outer_shadow_ops()
        .expect("finite inline fragments use the typed shadow grammar");
    assert_eq!(prepared.len(), 2);
    let min_y = prepared
        .iter()
        .map(|shadow| shadow.shape.y)
        .collect::<Vec<_>>();
    assert_eq!(min_y, vec![0.0, 10.0]);
}
