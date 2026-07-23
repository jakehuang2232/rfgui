use super::*;

#[test]
fn hover_style_updates_color_opacity_and_reverts() {
    let mut el = Element::new(0.0, 0.0, 100.0, 40.0);
    let base_color = Color::rgb(10, 20, 30);
    let hover_color = Color::rgb(200, 150, 100);
    let mut style = Style::new();
    style.set_background(base_color.into());
    style.insert(
        PropertyId::Opacity,
        ParsedValue::Opacity(Opacity::new(0.25)),
    );
    let mut hover_style = Style::new();
    hover_style.set_background(hover_color.into());
    hover_style.insert(
        PropertyId::Opacity,
        ParsedValue::Opacity(Opacity::new(0.75)),
    );
    style.set_hover(hover_style);
    el.apply_style(style);
    el.layout_dirty = false;
    el.clear_local_dirty_flags(DirtyFlags::ALL);

    assert!(el.set_hovered(true));
    let hovered_state = el.debug_render_state();
    assert_eq!(hovered_state.background_rgba, hover_color.to_rgba_u8());
    assert!((hovered_state.opacity - 0.75).abs() < 0.001);
    assert!(!el.layout_dirty);
    assert!(el.local_dirty_flags().contains(DirtyFlags::RUNTIME));

    el.clear_local_dirty_flags(DirtyFlags::ALL);
    el.layout_dirty = false;
    assert!(el.set_hovered(false));
    let base_state = el.debug_render_state();
    assert_eq!(base_state.background_rgba, base_color.to_rgba_u8());
    assert!((base_state.opacity - 0.25).abs() < 0.001);
    assert!(!el.layout_dirty);
    assert!(el.local_dirty_flags().contains(DirtyFlags::RUNTIME));
}

#[test]
fn layout_affecting_hover_style_marks_layout_dirty() {
    let mut el = Element::new(0.0, 0.0, 100.0, 40.0);
    let mut style = Style::new();
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(100.0)));
    let mut hover_style = Style::new();
    hover_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(180.0)));
    style.set_hover(hover_style);
    el.apply_style(style);
    el.layout_dirty = false;
    el.clear_local_dirty_flags(DirtyFlags::ALL);

    assert!(el.set_hovered(true));
    assert!(el.layout_dirty);
    assert!(el.local_dirty_flags().contains(DirtyFlags::LAYOUT));
}

#[test]
fn hover_style_emits_transition_request() {
    let mut el = Element::new(0.0, 0.0, 100.0, 40.0);
    let mut style = Style::new();
    style.insert(PropertyId::Opacity, ParsedValue::Opacity(Opacity::new(0.2)));
    style.insert(
        PropertyId::Transition,
        ParsedValue::Transition(Transitions::single(Transition::new(
            TransitionProperty::Opacity,
            200,
        ))),
    );
    let mut hover_style = Style::new();
    hover_style.insert(PropertyId::Opacity, ParsedValue::Opacity(Opacity::new(0.8)));
    style.set_hover(hover_style);
    el.apply_style(style);

    assert!(el.set_hovered(true));
    let reqs = el.take_style_transition_requests();
    assert_eq!(reqs.len(), 1);
    assert_eq!(reqs[0].field, crate::transition::StyleField::Opacity);
    assert_eq!(reqs[0].from, crate::transition::StyleValue::Scalar(0.2));
    assert_eq!(reqs[0].to, crate::transition::StyleValue::Scalar(0.8));
}

#[test]
fn apply_style_syncs_box_shadow_into_element_state() {
    let mut el = Element::new(0.0, 0.0, 100.0, 40.0);
    let mut style = Style::new();
    style.set_box_shadow(vec![
        BoxShadow::new()
            .color(Color::hex("#223344"))
            .offset_x(3.0)
            .offset_y(5.0)
            .blur(8.0)
            .spread(2.0),
        BoxShadow::new().offset(-1.0),
    ]);
    el.apply_style(style);

    assert_eq!(el.computed_style.box_shadow.len(), 2);
    assert_eq!(el.box_shadows.len(), 2);
    assert_eq!(el.box_shadows[0].offset_x, 3.0);
    assert_eq!(el.box_shadows[0].offset_y, 5.0);
    assert_eq!(el.box_shadows[0].blur, 8.0);
    assert_eq!(el.box_shadows[0].spread, 2.0);
    assert!(!el.box_shadows[0].inset);
    assert_eq!(el.box_shadows[1].offset_x, -1.0);
    assert_eq!(el.box_shadows[1].offset_y, -1.0);
    assert!(!el.box_shadows[1].inset);
}

#[test]
fn apply_style_syncs_background_border_and_opacity_into_element_state() {
    let mut el = Element::new(0.0, 0.0, 100.0, 40.0);
    let background = Color::rgb(18, 52, 86);
    let border_color = Color::rgb(171, 205, 239);
    let mut style = Style::new();
    style.set_background(background.into());
    style.set_border(Border::uniform(Length::px(3.0), &border_color));
    style.insert(
        PropertyId::Opacity,
        ParsedValue::Opacity(Opacity::new(0.42)),
    );

    el.apply_style(style);

    let render_state = el.debug_render_state();
    assert_eq!(render_state.background_rgba, background.to_rgba_u8());
    assert_eq!(render_state.border_top_rgba, border_color.to_rgba_u8());
    assert_eq!(render_state.border_right_rgba, border_color.to_rgba_u8());
    assert_eq!(render_state.border_bottom_rgba, border_color.to_rgba_u8());
    assert_eq!(render_state.border_left_rgba, border_color.to_rgba_u8());
    assert!((el.border_widths.left - 3.0).abs() < 0.001);
    assert!((el.border_widths.right - 3.0).abs() < 0.001);
    assert!((el.border_widths.top - 3.0).abs() < 0.001);
    assert!((el.border_widths.bottom - 3.0).abs() < 0.001);
    assert!((render_state.opacity - 0.42).abs() < 0.001);
}

#[test]
fn computed_style_consumer_syncs_element_render_state() {
    let mut el = Element::new(0.0, 0.0, 100.0, 40.0);
    el.clear_local_dirty_flags(DirtyFlags::ALL);
    let background = Color::rgb(9, 18, 27);
    let border_color = Color::rgb(36, 45, 54);
    let mut computed = ComputedStyle::default();
    computed.background_color = background;
    computed.border_colors = crate::style::EdgeInsets {
        top: border_color,
        right: border_color,
        bottom: border_color,
        left: border_color,
    };
    computed.border_widths = crate::style::EdgeInsets {
        top: Length::px(2.0),
        right: Length::px(2.0),
        bottom: Length::px(2.0),
        left: Length::px(2.0),
    };
    computed.opacity = 0.35;

    ComputedStyleConsumer::apply_computed_style(&mut el, computed, None);

    let render_state = el.debug_render_state();
    assert_eq!(render_state.background_rgba, background.to_rgba_u8());
    assert_eq!(render_state.border_top_rgba, border_color.to_rgba_u8());
    assert_eq!(render_state.border_right_rgba, border_color.to_rgba_u8());
    assert_eq!(render_state.border_bottom_rgba, border_color.to_rgba_u8());
    assert_eq!(render_state.border_left_rgba, border_color.to_rgba_u8());
    assert!((el.border_widths.left - 2.0).abs() < 0.001);
    assert!((el.border_widths.right - 2.0).abs() < 0.001);
    assert!((el.border_widths.top - 2.0).abs() < 0.001);
    assert!((el.border_widths.bottom - 2.0).abs() < 0.001);
    assert!((render_state.opacity - 0.35).abs() < 0.001);
    assert!(el.local_dirty_flags().contains(DirtyFlags::PAINT));
    assert!(el.local_dirty_flags().contains(DirtyFlags::COMPOSITE));
}
