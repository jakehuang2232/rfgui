use super::{StyleComputeContext, compute_style, compute_style_with_context};
use crate::style::{
    Align, CrossAxis, CrossSize, FlowDirection, FlowWrap, JustifyContent, Layout, Length,
};
use crate::style::{
    BoxShadow, Color, FontSize, Opacity, ParsedValue, PropertyId, SelectionStyle, SizeValue, Style,
    TextWrap,
};

#[test]
fn compute_style_applies_box_shadow_list() {
    let mut style = Style::new();
    style.set_box_shadow(vec![
        BoxShadow::new()
            .color(Color::hex("#112233"))
            .offset_x(2.0)
            .offset_y(3.0)
            .blur(4.0)
            .spread(5.0),
        BoxShadow::new().color(Color::hex("#445566")).offset(-1.5),
    ]);

    let computed = compute_style(&style, None);
    assert_eq!(computed.box_shadow.len(), 2);
    assert_eq!(computed.box_shadow[0].offset_x, 2.0);
    assert_eq!(computed.box_shadow[0].offset_y, 3.0);
    assert_eq!(computed.box_shadow[0].blur, 4.0);
    assert_eq!(computed.box_shadow[0].spread, 5.0);
    assert_eq!(computed.box_shadow[1].offset_x, -1.5);
    assert_eq!(computed.box_shadow[1].offset_y, -1.5);
}

#[test]
fn compute_style_resolves_font_size_relative_to_parent() {
    let mut parent_style = Style::new();
    parent_style.insert(
        PropertyId::FontSize,
        ParsedValue::FontSize(FontSize::px(20.0)),
    );
    let parent = compute_style(&parent_style, None);

    let mut child_style = Style::new();
    child_style.insert(
        PropertyId::FontSize,
        ParsedValue::FontSize(FontSize::em(1.5)),
    );
    let child = compute_style(&child_style, Some(&parent));
    assert_eq!(child.font_size, 30.0);
}

#[test]
fn compute_style_with_context_matches_legacy_parent_inheritance() {
    let mut parent_style = Style::new();
    parent_style.insert(
        PropertyId::Color,
        ParsedValue::Color(Color::rgb(0x33, 0x66, 0x99).into()),
    );
    parent_style.insert(
        PropertyId::FontSize,
        ParsedValue::FontSize(FontSize::px(22.0)),
    );
    parent_style.insert(
        PropertyId::LineHeight,
        ParsedValue::LineHeight(crate::style::LineHeight::new(1.6)),
    );
    parent_style.insert(
        PropertyId::TextWrap,
        ParsedValue::TextWrap(TextWrap::NoWrap),
    );
    let parent = compute_style(&parent_style, None);

    let mut child_style = Style::new();
    child_style.insert(
        PropertyId::FontSize,
        ParsedValue::FontSize(FontSize::percent(150.0)),
    );
    child_style.insert(
        PropertyId::Opacity,
        ParsedValue::Opacity(crate::style::Opacity::new(0.5)),
    );

    let legacy = compute_style(&child_style, Some(&parent));
    let with_context = compute_style_with_context(
        &child_style,
        StyleComputeContext {
            parent: Some(&parent),
            viewport_width: 640.0,
            viewport_height: 480.0,
            root_font_size: 24.0,
            hovered: true,
        },
    );

    assert_eq!(with_context, legacy);
    assert_eq!(with_context.color, parent.color);
    assert_eq!(with_context.font_size, 33.0);
    assert_eq!(with_context.line_height, parent.line_height);
    assert_eq!(with_context.text_wrap, parent.text_wrap);
}

#[test]
fn compute_style_with_context_applies_hover_style_when_hovered() {
    let mut style = Style::new();
    style.insert(
        PropertyId::Color,
        ParsedValue::Color(Color::rgb(0x10, 0x20, 0x30).into()),
    );

    let mut hover = Style::new();
    hover.insert(
        PropertyId::Color,
        ParsedValue::Color(Color::rgb(0x44, 0x55, 0x66).into()),
    );
    hover.insert(PropertyId::Opacity, ParsedValue::Opacity(Opacity::new(0.4)));
    style.set_hover(hover);

    let computed = compute_style_with_context(
        &style,
        StyleComputeContext {
            parent: None,
            viewport_width: 0.0,
            viewport_height: 0.0,
            root_font_size: 16.0,
            hovered: true,
        },
    );

    assert_eq!(computed.color, Color::rgb(0x44, 0x55, 0x66));
    assert_eq!(computed.opacity, 0.4);
}

#[test]
fn compute_style_with_context_ignores_hover_style_when_not_hovered() {
    let mut style = Style::new();
    style.insert(
        PropertyId::Color,
        ParsedValue::Color(Color::rgb(0x10, 0x20, 0x30).into()),
    );

    let mut hover = Style::new();
    hover.insert(
        PropertyId::Color,
        ParsedValue::Color(Color::rgb(0x44, 0x55, 0x66).into()),
    );
    hover.insert(PropertyId::Opacity, ParsedValue::Opacity(Opacity::new(0.4)));
    style.set_hover(hover);

    let computed = compute_style_with_context(
        &style,
        StyleComputeContext {
            parent: None,
            viewport_width: 0.0,
            viewport_height: 0.0,
            root_font_size: 16.0,
            hovered: false,
        },
    );

    assert_eq!(computed.color, Color::rgb(0x10, 0x20, 0x30));
    assert_eq!(computed.opacity, 1.0);
}

#[test]
fn hover_style_overrides_base_declarations() {
    let mut style = Style::new();
    style.insert(PropertyId::Opacity, ParsedValue::Opacity(Opacity::new(0.2)));

    let mut hover = Style::new();
    hover.insert(PropertyId::Opacity, ParsedValue::Opacity(Opacity::new(0.8)));
    style.set_hover(hover);

    let computed = compute_style_with_context(
        &style,
        StyleComputeContext {
            parent: None,
            viewport_width: 0.0,
            viewport_height: 0.0,
            root_font_size: 16.0,
            hovered: true,
        },
    );

    assert_eq!(computed.opacity, 0.8);
}

#[test]
fn legacy_compute_style_does_not_apply_hover_style() {
    let mut style = Style::new();
    style.insert(PropertyId::Opacity, ParsedValue::Opacity(Opacity::new(0.2)));

    let mut hover = Style::new();
    hover.insert(PropertyId::Opacity, ParsedValue::Opacity(Opacity::new(0.8)));
    style.set_hover(hover);

    let computed = compute_style(&style, None);

    assert_eq!(computed.opacity, 0.2);
}

#[test]
fn hovered_effective_style_uses_merged_selection() {
    let mut style = Style::new();
    let mut base_selection = SelectionStyle::new();
    base_selection.set_background(Color::rgb(0x11, 0x22, 0x33));
    style.set_selection(base_selection);

    let mut hover = Style::new();
    let mut hover_selection = SelectionStyle::new();
    hover_selection.set_background(Color::rgb(0xaa, 0xbb, 0xcc));
    hover.set_selection(hover_selection);
    style.set_hover(hover);

    let computed = compute_style_with_context(
        &style,
        StyleComputeContext {
            parent: None,
            viewport_width: 0.0,
            viewport_height: 0.0,
            root_font_size: 16.0,
            hovered: true,
        },
    );

    assert_eq!(
        computed.selection_background_color,
        Color::rgb(0xaa, 0xbb, 0xcc)
    );
}

#[test]
fn legacy_compute_style_keeps_default_root_font_size_for_rem() {
    let mut style = Style::new();
    style.insert(
        PropertyId::FontSize,
        ParsedValue::FontSize(FontSize::rem(2.0)),
    );

    let legacy = compute_style(&style, None);

    assert_eq!(legacy.font_size, 32.0);
}

#[test]
fn compute_style_with_context_resolves_rem_from_root_font_size() {
    let mut style = Style::new();
    style.insert(
        PropertyId::FontSize,
        ParsedValue::FontSize(FontSize::rem(2.0)),
    );

    let computed = compute_style_with_context(
        &style,
        StyleComputeContext {
            parent: None,
            viewport_width: 800.0,
            viewport_height: 600.0,
            root_font_size: 20.0,
            hovered: false,
        },
    );

    assert_eq!(computed.font_size, 40.0);
}

#[test]
fn compute_style_with_context_resolves_viewport_font_sizes() {
    let mut vw_style = Style::new();
    vw_style.insert(
        PropertyId::FontSize,
        ParsedValue::FontSize(FontSize::vw(10.0)),
    );
    let vw = compute_style_with_context(
        &vw_style,
        StyleComputeContext {
            parent: None,
            viewport_width: 800.0,
            viewport_height: 600.0,
            root_font_size: 16.0,
            hovered: false,
        },
    );

    let mut vh_style = Style::new();
    vh_style.insert(
        PropertyId::FontSize,
        ParsedValue::FontSize(FontSize::vh(10.0)),
    );
    let vh = compute_style_with_context(
        &vh_style,
        StyleComputeContext {
            parent: None,
            viewport_width: 800.0,
            viewport_height: 600.0,
            root_font_size: 16.0,
            hovered: false,
        },
    );

    assert_eq!(vw.font_size, 80.0);
    assert_eq!(vh.font_size, 60.0);
}

#[test]
fn compute_style_with_context_resolves_em_and_percent_from_parent_font_size() {
    let mut parent_style = Style::new();
    parent_style.insert(
        PropertyId::FontSize,
        ParsedValue::FontSize(FontSize::px(20.0)),
    );
    let parent = compute_style(&parent_style, None);

    let mut em_style = Style::new();
    em_style.insert(
        PropertyId::FontSize,
        ParsedValue::FontSize(FontSize::em(1.5)),
    );
    let em = compute_style_with_context(
        &em_style,
        StyleComputeContext {
            parent: Some(&parent),
            viewport_width: 800.0,
            viewport_height: 600.0,
            root_font_size: 24.0,
            hovered: false,
        },
    );

    let mut percent_style = Style::new();
    percent_style.insert(
        PropertyId::FontSize,
        ParsedValue::FontSize(FontSize::percent(150.0)),
    );
    let percent = compute_style_with_context(
        &percent_style,
        StyleComputeContext {
            parent: Some(&parent),
            viewport_width: 800.0,
            viewport_height: 600.0,
            root_font_size: 24.0,
            hovered: false,
        },
    );

    assert_eq!(em.font_size, 30.0);
    assert_eq!(percent.font_size, 30.0);
}

#[test]
fn compute_style_reads_justify_content_from_layput_flow() {
    let mut style = Style::new();
    style.insert(
        PropertyId::Layout,
        ParsedValue::Layout(
            Layout::flow()
                .column()
                .wrap()
                .justify_content(JustifyContent::SpaceEvenly)
                .align(Align::Center)
                .cross_size(CrossSize::Stretch)
                .into(),
        ),
    );

    let computed = compute_style(&style, None);
    assert_eq!(
        computed.layout,
        Layout::Flow {
            direction: FlowDirection::Column,
            wrap: FlowWrap::Wrap,
            justify_content: JustifyContent::SpaceEvenly,
            cross_axis: CrossAxis::new(CrossSize::Stretch, Align::Center),
        }
    );
    assert_eq!(computed.align, Align::Center);
    assert_eq!(computed.cross_size, CrossSize::Stretch);
}

#[test]
fn explicit_cross_axis_overrides_flow_cross_axis() {
    let mut style = Style::new();
    style.insert(
        PropertyId::Layout,
        ParsedValue::Layout(
            Layout::flow()
                .align(Align::End)
                .cross_size(CrossSize::Fit)
                .into(),
        ),
    );
    style.insert(PropertyId::Align, ParsedValue::Align(Align::Center));
    style.insert(
        PropertyId::CrossSize,
        ParsedValue::CrossSize(CrossSize::Stretch),
    );

    let computed = compute_style(&style, None);
    assert_eq!(computed.align, Align::Center);
    assert_eq!(computed.cross_size, CrossSize::Stretch);
}

#[test]
fn compute_style_reads_text_wrap() {
    let mut style = Style::new();
    style.insert(
        PropertyId::TextWrap,
        ParsedValue::TextWrap(TextWrap::NoWrap),
    );

    let computed = compute_style(&style, None);
    assert_eq!(computed.text_wrap, TextWrap::NoWrap);
}

#[test]
fn compute_style_reads_flex_container_and_item_fields() {
    let mut style = Style::new();
    style.insert(
        PropertyId::Layout,
        ParsedValue::Layout(
            Layout::flex()
                .column()
                .justify_content(JustifyContent::Center)
                .align(Align::End)
                .cross_size(CrossSize::Stretch)
                .into(),
        ),
    );
    style.insert(
        PropertyId::Flex,
        ParsedValue::Flex(
            crate::style::flex()
                .grow(2.0)
                .shrink(0.0)
                .basis(Length::px(80.0)),
        ),
    );

    let computed = compute_style(&style, None);
    assert_eq!(computed.layout_axis_direction(), FlowDirection::Column);
    assert_eq!(
        computed.layout_axis_justify_content(),
        JustifyContent::Center
    );
    assert_eq!(computed.layout_axis_align(), Align::End);
    assert_eq!(computed.layout_axis_cross_size(), CrossSize::Stretch);
    assert_eq!(computed.flex_grow, 2.0);
    assert_eq!(computed.flex_shrink, 0.0);
    assert_eq!(computed.flex_basis, SizeValue::Length(Length::px(80.0)));
}

#[test]
fn inline_layout_uses_row_wrap_defaults() {
    let mut style = Style::new();
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));

    let computed = compute_style(&style, None);
    assert_eq!(computed.layout_axis_direction(), FlowDirection::Row);
    assert_eq!(computed.layout_flow_wrap(), FlowWrap::Wrap);
    assert_eq!(computed.layout_axis_align(), Align::Start);
    assert_eq!(computed.layout_axis_cross_size(), CrossSize::Fit);
}
