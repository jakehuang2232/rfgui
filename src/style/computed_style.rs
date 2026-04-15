#![allow(missing_docs)]

//! Computed style data used by layout, rendering, and interaction passes.

use crate::style::color::Color;
use crate::style::parsed_style::{
    Align, Animator, BoxShadow, CrossSize, Cursor, FontSize, Layout, Length, ParsedValue, Position,
    PropertyId, ScrollDirection, Style, TextWrap, Transform, TransformOrigin, Transitions,
};

/// A resolved size value used by computed style.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SizeValue {
    Auto,
    Length(Length),
}

/// A generic top-right-bottom-left edge container.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgeInsets<T> {
    pub top: T,
    pub right: T,
    pub bottom: T,
    pub left: T,
}

/// A generic per-corner radii container.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CornerRadii<T> {
    pub top_left: T,
    pub top_right: T,
    pub bottom_right: T,
    pub bottom_left: T,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ComputedStyle {
    pub layout: Layout,
    pub cross_size: CrossSize,
    pub align: Align,
    pub flex_grow: f32,
    pub flex_shrink: f32,
    pub flex_basis: SizeValue,
    pub position: Position,
    pub width: SizeValue,
    pub height: SizeValue,
    pub min_width: SizeValue,
    pub min_height: SizeValue,
    pub max_width: SizeValue,
    pub max_height: SizeValue,
    pub margin: EdgeInsets<Length>,
    pub padding: EdgeInsets<Length>,
    pub gap: Length,
    pub scroll_direction: ScrollDirection,
    pub cursor: Cursor,
    pub color: Color,
    pub selection_background_color: Color,
    pub background_color: Color,
    pub font_families: Vec<String>,
    pub font_size: f32,
    pub font_weight: u16,
    pub line_height: f32,
    pub text_wrap: TextWrap,
    pub border_radius: f32,
    pub border_radii: CornerRadii<Length>,
    pub border_width: f32,
    pub border_color: Color,
    pub border_widths: EdgeInsets<Length>,
    pub border_colors: EdgeInsets<Color>,
    pub opacity: f32,
    pub box_shadow: Vec<BoxShadow>,
    pub transform: Transform,
    pub transform_origin: TransformOrigin,
    pub transition: Transitions,
    pub animator: Option<Animator>,
}

impl Default for ComputedStyle {
    fn default() -> Self {
        Self {
            layout: Layout::Inline,
            cross_size: CrossSize::Fit,
            align: Align::Start,
            flex_grow: 0.0,
            flex_shrink: 1.0,
            flex_basis: SizeValue::Auto,
            position: Position::static_(),
            width: SizeValue::Auto,
            height: SizeValue::Auto,
            min_width: SizeValue::Length(Length::Px(0.0)),
            min_height: SizeValue::Length(Length::Px(0.0)),
            max_width: SizeValue::Auto,
            max_height: SizeValue::Auto,
            margin: EdgeInsets {
                top: Length::Px(0.0),
                right: Length::Px(0.0),
                bottom: Length::Px(0.0),
                left: Length::Px(0.0),
            },
            padding: EdgeInsets {
                top: Length::Px(0.0),
                right: Length::Px(0.0),
                bottom: Length::Px(0.0),
                left: Length::Px(0.0),
            },
            gap: Length::Px(0.0),
            scroll_direction: ScrollDirection::None,
            cursor: Cursor::Default,
            color: Color::rgb(0, 0, 0),
            selection_background_color: Color::rgba(0, 0, 0, 0),
            background_color: Color::rgba(0, 0, 0, 0),
            font_families: Vec::new(),
            font_size: 16.0,
            font_weight: 400,
            line_height: 1.2,
            text_wrap: TextWrap::Wrap,
            border_radius: 0.0,
            border_radii: CornerRadii {
                top_left: Length::Px(0.0),
                top_right: Length::Px(0.0),
                bottom_right: Length::Px(0.0),
                bottom_left: Length::Px(0.0),
            },
            border_width: 0.0,
            border_color: Color::rgb(0, 0, 0),
            border_widths: EdgeInsets {
                top: Length::Px(0.0),
                right: Length::Px(0.0),
                bottom: Length::Px(0.0),
                left: Length::Px(0.0),
            },
            border_colors: EdgeInsets {
                top: Color::rgb(0, 0, 0),
                right: Color::rgb(0, 0, 0),
                bottom: Color::rgb(0, 0, 0),
                left: Color::rgb(0, 0, 0),
            },
            opacity: 1.0,
            box_shadow: Vec::new(),
            transform: Transform::default(),
            transform_origin: TransformOrigin::center(),
            transition: Transitions::default(),
            animator: None,
        }
    }
}

impl ComputedStyle {
    /// Returns `true` when all layout-affecting (measure-pass) fields match.
    ///
    /// Position insets (left/top/right/bottom), colors, opacity, transform,
    /// border-radius, cursor, box-shadow, transition, and animator are excluded
    /// because they only affect the place or paint passes.
    pub fn layout_eq(&self, other: &Self) -> bool {
        self.layout == other.layout
            && self.cross_size == other.cross_size
            && self.align == other.align
            && self.flex_grow == other.flex_grow
            && self.flex_shrink == other.flex_shrink
            && self.flex_basis == other.flex_basis
            && self.position.mode() == other.position.mode()
            && self.width == other.width
            && self.height == other.height
            && self.min_width == other.min_width
            && self.min_height == other.min_height
            && self.max_width == other.max_width
            && self.max_height == other.max_height
            && self.margin == other.margin
            && self.padding == other.padding
            && self.gap == other.gap
            && self.scroll_direction == other.scroll_direction
            && self.font_families == other.font_families
            && self.font_size == other.font_size
            && self.font_weight == other.font_weight
            && self.line_height == other.line_height
            && self.text_wrap == other.text_wrap
            && self.border_widths == other.border_widths
    }

    pub const fn layout_axis_direction(&self) -> crate::FlowDirection {
        match self.layout {
            Layout::Flex { direction, .. } | Layout::Flow { direction, .. } => direction,
            Layout::Inline => crate::FlowDirection::Row,
            _ => crate::FlowDirection::Row,
        }
    }

    pub const fn layout_flow_direction(&self) -> crate::FlowDirection {
        match self.layout {
            Layout::Flow { direction, .. } => direction,
            Layout::Inline => crate::FlowDirection::Row,
            _ => crate::FlowDirection::Row,
        }
    }

    pub const fn layout_flow_wrap(&self) -> crate::FlowWrap {
        match self.layout {
            Layout::Flow { wrap, .. } => wrap,
            Layout::Inline => crate::FlowWrap::Wrap,
            _ => crate::FlowWrap::NoWrap,
        }
    }

    pub const fn layout_axis_justify_content(&self) -> crate::JustifyContent {
        match self.layout {
            Layout::Flex {
                justify_content, ..
            }
            | Layout::Flow {
                justify_content, ..
            } => justify_content,
            Layout::Inline => crate::JustifyContent::Start,
            _ => crate::JustifyContent::Start,
        }
    }

    pub const fn layout_flow_justify_content(&self) -> crate::JustifyContent {
        match self.layout {
            Layout::Flow {
                justify_content, ..
            } => justify_content,
            Layout::Inline => crate::JustifyContent::Start,
            _ => crate::JustifyContent::Start,
        }
    }

    pub const fn layout_axis_cross_size(&self) -> crate::CrossSize {
        match self.layout {
            Layout::Flex { cross_axis, .. } | Layout::Flow { cross_axis, .. } => cross_axis.size,
            Layout::Inline => crate::CrossSize::Fit,
            _ => self.cross_size,
        }
    }

    pub const fn layout_flow_cross_size(&self) -> crate::CrossSize {
        match self.layout {
            Layout::Flow { cross_axis, .. } => cross_axis.size,
            Layout::Inline => crate::CrossSize::Fit,
            _ => self.cross_size,
        }
    }

    pub const fn layout_axis_align(&self) -> crate::Align {
        match self.layout {
            Layout::Flex { cross_axis, .. } | Layout::Flow { cross_axis, .. } => cross_axis.align,
            Layout::Inline => crate::Align::Start,
            _ => self.align,
        }
    }

    pub const fn layout_flow_align(&self) -> crate::Align {
        match self.layout {
            Layout::Flow { cross_axis, .. } => cross_axis.align,
            Layout::Inline => crate::Align::Start,
            _ => self.align,
        }
    }
}

pub fn compute_style(parsed: &Style, parent: Option<&ComputedStyle>) -> ComputedStyle {
    let mut computed = ComputedStyle::default();
    let mut has_explicit_cross_size = false;
    let mut has_explicit_align = false;

    if let Some(parent) = parent {
        computed.color = parent.color;
        computed.font_families = parent.font_families.clone();
        computed.font_size = parent.font_size;
        computed.font_weight = parent.font_weight;
        computed.line_height = parent.line_height;
    }

    if let Some(selection) = parsed.selection()
        && let Some(background) = selection.background_color()
    {
        computed.selection_background_color = background.to_color();
    }

    for declaration in parsed.declarations() {
        match declaration.property {
            PropertyId::Layout => {
                if let ParsedValue::Layout(value) = &declaration.value {
                    computed.layout = *value;
                }
            }
            PropertyId::CrossSize => {
                if let ParsedValue::CrossSize(value) = &declaration.value {
                    computed.cross_size = *value;
                    has_explicit_cross_size = true;
                }
            }
            PropertyId::Align => {
                if let ParsedValue::Align(value) = &declaration.value {
                    computed.align = *value;
                    has_explicit_align = true;
                }
            }
            PropertyId::Flex => {
                if let ParsedValue::Flex(value) = &declaration.value {
                    computed.flex_grow = value.grow_value().max(0.0);
                    computed.flex_shrink = value.shrink_value().max(0.0);
                    computed.flex_basis = value
                        .basis_value()
                        .map(SizeValue::Length)
                        .unwrap_or(SizeValue::Auto);
                }
            }
            PropertyId::Position => {
                if let ParsedValue::Position(value) = &declaration.value {
                    computed.position = value.clone();
                }
            }
            PropertyId::Width => {
                if let Some(value) = parse_size_value(&declaration.value) {
                    computed.width = value;
                }
            }
            PropertyId::Height => {
                if let Some(value) = parse_size_value(&declaration.value) {
                    computed.height = value;
                }
            }
            PropertyId::MinWidth => {
                if let Some(value) = parse_size_value(&declaration.value) {
                    computed.min_width = value;
                }
            }
            PropertyId::MinHeight => {
                if let Some(value) = parse_size_value(&declaration.value) {
                    computed.min_height = value;
                }
            }
            PropertyId::MaxWidth => {
                if let Some(value) = parse_size_value(&declaration.value) {
                    computed.max_width = value;
                }
            }
            PropertyId::MaxHeight => {
                if let Some(value) = parse_size_value(&declaration.value) {
                    computed.max_height = value;
                }
            }
            PropertyId::MarginTop => {
                computed.margin.top = parse_length(&declaration.value, computed.margin.top)
            }
            PropertyId::MarginRight => {
                computed.margin.right = parse_length(&declaration.value, computed.margin.right)
            }
            PropertyId::MarginBottom => {
                computed.margin.bottom = parse_length(&declaration.value, computed.margin.bottom)
            }
            PropertyId::MarginLeft => {
                computed.margin.left = parse_length(&declaration.value, computed.margin.left)
            }
            PropertyId::PaddingTop => {
                computed.padding.top = parse_length(&declaration.value, computed.padding.top)
            }
            PropertyId::PaddingRight => {
                computed.padding.right = parse_length(&declaration.value, computed.padding.right)
            }
            PropertyId::PaddingBottom => {
                computed.padding.bottom = parse_length(&declaration.value, computed.padding.bottom)
            }
            PropertyId::PaddingLeft => {
                computed.padding.left = parse_length(&declaration.value, computed.padding.left)
            }
            PropertyId::Gap => computed.gap = parse_length(&declaration.value, computed.gap),
            PropertyId::ScrollDirection => {
                if let ParsedValue::ScrollDirection(value) = &declaration.value {
                    computed.scroll_direction = *value;
                }
            }
            PropertyId::Cursor => {
                if let ParsedValue::Cursor(value) = &declaration.value {
                    computed.cursor = *value;
                }
            }
            PropertyId::Color => {
                computed.color = parse_color(&declaration.value).unwrap_or(computed.color)
            }
            PropertyId::BackgroundColor => {
                computed.background_color =
                    parse_color(&declaration.value).unwrap_or(computed.background_color)
            }
            PropertyId::FontFamily => {
                if let ParsedValue::FontFamily(value) = &declaration.value {
                    computed.font_families = value.as_slice().to_vec();
                }
            }
            PropertyId::FontSize => {
                if let ParsedValue::FontSize(value) = &declaration.value {
                    computed.font_size = resolve_font_size_px(*value, computed.font_size);
                }
            }
            PropertyId::FontWeight => {
                if let ParsedValue::FontWeight(value) = &declaration.value {
                    computed.font_weight = value.value().clamp(100, 900);
                }
            }
            PropertyId::LineHeight => {
                if let ParsedValue::LineHeight(value) = &declaration.value {
                    computed.line_height = value.value().max(0.0);
                }
            }
            PropertyId::TextWrap => {
                if let ParsedValue::TextWrap(value) = &declaration.value {
                    computed.text_wrap = *value;
                }
            }
            PropertyId::BorderRadius => {
                let length = parse_length(&declaration.value, Length::Px(computed.border_radius));
                computed.border_radii.top_left = length;
                computed.border_radii.top_right = length;
                computed.border_radii.bottom_right = length;
                computed.border_radii.bottom_left = length;
            }
            PropertyId::BorderTopLeftRadius => {
                computed.border_radii.top_left =
                    parse_length(&declaration.value, computed.border_radii.top_left)
            }
            PropertyId::BorderTopRightRadius => {
                computed.border_radii.top_right =
                    parse_length(&declaration.value, computed.border_radii.top_right)
            }
            PropertyId::BorderBottomRightRadius => {
                computed.border_radii.bottom_right =
                    parse_length(&declaration.value, computed.border_radii.bottom_right)
            }
            PropertyId::BorderBottomLeftRadius => {
                computed.border_radii.bottom_left =
                    parse_length(&declaration.value, computed.border_radii.bottom_left)
            }
            PropertyId::BorderWidth => {
                let length = parse_length(&declaration.value, Length::Px(computed.border_width));
                computed.border_width = resolve_length_px(length).max(0.0);
                computed.border_widths.top = length;
                computed.border_widths.right = length;
                computed.border_widths.bottom = length;
                computed.border_widths.left = length;
            }
            PropertyId::BorderColor => {
                let color = parse_color(&declaration.value).unwrap_or(computed.border_color);
                computed.border_color = color;
                computed.border_colors.top = color;
                computed.border_colors.right = color;
                computed.border_colors.bottom = color;
                computed.border_colors.left = color;
            }
            PropertyId::BorderTopWidth => {
                computed.border_widths.top =
                    parse_length(&declaration.value, computed.border_widths.top)
            }
            PropertyId::BorderRightWidth => {
                computed.border_widths.right =
                    parse_length(&declaration.value, computed.border_widths.right)
            }
            PropertyId::BorderBottomWidth => {
                computed.border_widths.bottom =
                    parse_length(&declaration.value, computed.border_widths.bottom)
            }
            PropertyId::BorderLeftWidth => {
                computed.border_widths.left =
                    parse_length(&declaration.value, computed.border_widths.left)
            }
            PropertyId::BorderTopColor => {
                computed.border_colors.top =
                    parse_color(&declaration.value).unwrap_or(computed.border_colors.top)
            }
            PropertyId::BorderRightColor => {
                computed.border_colors.right =
                    parse_color(&declaration.value).unwrap_or(computed.border_colors.right)
            }
            PropertyId::BorderBottomColor => {
                computed.border_colors.bottom =
                    parse_color(&declaration.value).unwrap_or(computed.border_colors.bottom)
            }
            PropertyId::BorderLeftColor => {
                computed.border_colors.left =
                    parse_color(&declaration.value).unwrap_or(computed.border_colors.left)
            }
            PropertyId::Opacity => {
                if let ParsedValue::Opacity(value) = &declaration.value {
                    computed.opacity = value.value().clamp(0.0, 1.0);
                }
            }
            PropertyId::BoxShadow => {
                if let ParsedValue::BoxShadow(value) = &declaration.value {
                    computed.box_shadow = value.clone();
                }
            }
            PropertyId::Transform => {
                if let ParsedValue::Transform(value) = &declaration.value {
                    computed.transform = value.clone();
                }
            }
            PropertyId::TransformOrigin => {
                if let ParsedValue::TransformOrigin(value) = &declaration.value {
                    computed.transform_origin = *value;
                }
            }
            PropertyId::Transition => {
                if let ParsedValue::Transition(value) = &declaration.value {
                    computed.transition = value.clone();
                }
            }
            PropertyId::Animator => {
                if let ParsedValue::Animator(value) = &declaration.value {
                    computed.animator = Some(value.clone());
                }
            }
        }
    }

    computed.border_width = max4(
        resolve_length_px(computed.border_widths.top),
        resolve_length_px(computed.border_widths.right),
        resolve_length_px(computed.border_widths.bottom),
        resolve_length_px(computed.border_widths.left),
    );
    computed.border_color = computed.border_colors.top;
    computed.border_radius = max4(
        resolve_length_px(computed.border_radii.top_left),
        resolve_length_px(computed.border_radii.top_right),
        resolve_length_px(computed.border_radii.bottom_right),
        resolve_length_px(computed.border_radii.bottom_left),
    )
    .max(0.0);
    if !has_explicit_cross_size {
        computed.cross_size = computed.layout_axis_cross_size();
    }
    if !has_explicit_align {
        computed.align = computed.layout_axis_align();
    }

    computed
}

fn parse_size_value(input: &ParsedValue) -> Option<SizeValue> {
    if let ParsedValue::Auto = input {
        return Some(SizeValue::Auto);
    }
    match input {
        ParsedValue::Length(value) => Some(SizeValue::Length(*value)),
        _ => None,
    }
}

fn parse_length(input: &ParsedValue, fallback: Length) -> Length {
    match input {
        ParsedValue::Length(value) => *value,
        _ => fallback,
    }
}

fn parse_color(input: &ParsedValue) -> Option<Color> {
    let ParsedValue::Color(raw) = input else {
        return None;
    };
    Some(raw.to_color())
}

fn resolve_length_px(length: Length) -> f32 {
    length.resolve_without_percent_base(0.0, 0.0)
}

fn resolve_font_size_px(font_size: FontSize, parent_font_size: f32) -> f32 {
    font_size.resolve_px(parent_font_size, 16.0, 0.0, 0.0)
}

fn max4(a: f32, b: f32, c: f32, d: f32) -> f32 {
    a.max(b).max(c).max(d)
}

#[cfg(test)]
mod tests {
    use super::compute_style;
    use crate::style::{
        BoxShadow, Color, FontSize, ParsedValue, PropertyId, SizeValue, Style, TextWrap,
    };
    use crate::{
        Align, CrossAxis, CrossSize, FlowDirection, FlowWrap, JustifyContent, Layout, Length,
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
            ParsedValue::Flex(crate::flex().grow(2.0).shrink(0.0).basis(Length::px(80.0))),
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
}
