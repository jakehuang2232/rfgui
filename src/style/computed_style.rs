#![allow(missing_docs)]

//! Computed style data used by layout, rendering, and interaction passes.

use crate::style::color::Color;
use crate::style::gradient::Gradient;
use crate::style::parsed_style::{
    Align, Animator, BoxShadow, CrossSize, Cursor, FontSize, Layout, Length, ParsedValue, Position,
    PropertyId, ScrollDirection, Style, TextWrap, Transform, TransformOrigin, Transitions,
    VerticalAlign,
};
use crate::style::style_props::apply_inherited_properties;

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
    pub background_image: Option<Gradient>,
    pub border_image: Option<Gradient>,
    pub font_families: Vec<String>,
    pub font_size: f32,
    pub font_weight: u16,
    pub line_height: f32,
    pub text_wrap: TextWrap,
    /// Cross-axis alignment within the inline line box. Initial
    /// `Baseline`; inherited (see `docs/design/inline-baseline.md` D5).
    /// Non-inline containers don't read this — they only pass it down
    /// the cascade.
    pub vertical_align: VerticalAlign,
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
            background_image: None,
            border_image: None,
            font_families: Vec::new(),
            font_size: 16.0,
            font_weight: 400,
            line_height: 1.2,
            text_wrap: TextWrap::Wrap,
            vertical_align: VerticalAlign::Baseline,
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
            && self.vertical_align == other.vertical_align
            && self.border_widths == other.border_widths
    }

    pub const fn layout_axis_direction(&self) -> crate::style::FlowDirection {
        match self.layout {
            Layout::Flex { direction, .. } | Layout::Flow { direction, .. } => direction,
            Layout::Inline => crate::style::FlowDirection::Row,
            _ => crate::style::FlowDirection::Row,
        }
    }

    pub const fn layout_flow_direction(&self) -> crate::style::FlowDirection {
        match self.layout {
            Layout::Flow { direction, .. } => direction,
            Layout::Inline => crate::style::FlowDirection::Row,
            _ => crate::style::FlowDirection::Row,
        }
    }

    pub const fn layout_flow_wrap(&self) -> crate::style::FlowWrap {
        match self.layout {
            Layout::Flow { wrap, .. } => wrap,
            Layout::Inline => crate::style::FlowWrap::Wrap,
            _ => crate::style::FlowWrap::NoWrap,
        }
    }

    pub const fn layout_axis_justify_content(&self) -> crate::style::JustifyContent {
        match self.layout {
            Layout::Flex {
                justify_content, ..
            }
            | Layout::Flow {
                justify_content, ..
            } => justify_content,
            Layout::Inline => crate::style::JustifyContent::Start,
            _ => crate::style::JustifyContent::Start,
        }
    }

    pub const fn layout_flow_justify_content(&self) -> crate::style::JustifyContent {
        match self.layout {
            Layout::Flow {
                justify_content, ..
            } => justify_content,
            Layout::Inline => crate::style::JustifyContent::Start,
            _ => crate::style::JustifyContent::Start,
        }
    }

    pub const fn layout_axis_cross_size(&self) -> crate::style::CrossSize {
        match self.layout {
            Layout::Flex { cross_axis, .. } | Layout::Flow { cross_axis, .. } => cross_axis.size,
            Layout::Inline => crate::style::CrossSize::Fit,
            _ => self.cross_size,
        }
    }

    pub const fn layout_flow_cross_size(&self) -> crate::style::CrossSize {
        match self.layout {
            Layout::Flow { cross_axis, .. } => cross_axis.size,
            Layout::Inline => crate::style::CrossSize::Fit,
            _ => self.cross_size,
        }
    }

    pub const fn layout_axis_align(&self) -> crate::style::Align {
        match self.layout {
            Layout::Flex { cross_axis, .. } | Layout::Flow { cross_axis, .. } => cross_axis.align,
            Layout::Inline => crate::style::Align::Start,
            _ => self.align,
        }
    }

    pub const fn layout_flow_align(&self) -> crate::style::Align {
        match self.layout {
            Layout::Flow { cross_axis, .. } => cross_axis.align,
            Layout::Inline => crate::style::Align::Start,
            _ => self.align,
        }
    }
}

pub fn compute_style(parsed: &Style, parent: Option<&ComputedStyle>) -> ComputedStyle {
    compute_style_with_context(
        parsed,
        StyleComputeContext {
            parent,
            viewport_width: 0.0,
            viewport_height: 0.0,
            root_font_size: 16.0,
            hovered: false,
        },
    )
}

/// Runtime context for style computation.
///
/// Font-size relative units are resolved from this context. Hovered state
/// selects the authored hover style before declarations are computed.
#[derive(Debug, Clone, Copy)]
pub struct StyleComputeContext<'a> {
    pub parent: Option<&'a ComputedStyle>,
    pub viewport_width: f32,
    pub viewport_height: f32,
    pub root_font_size: f32,
    pub hovered: bool,
}

pub fn compute_style_with_context(parsed: &Style, ctx: StyleComputeContext<'_>) -> ComputedStyle {
    let effective_style = ctx
        .hovered
        .then(|| parsed.hover().map(|hover| parsed.clone() + hover.clone()))
        .flatten();
    let parsed = effective_style.as_ref().unwrap_or(parsed);

    let mut computed = ComputedStyle::default();
    let mut has_explicit_cross_size = false;
    let mut has_explicit_align = false;

    if let Some(parent) = ctx.parent {
        apply_inherited_properties(parent, &mut computed);
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
            PropertyId::BackgroundImage => {
                if let ParsedValue::Gradient(value) = &declaration.value {
                    computed.background_image = Some(value.clone());
                }
            }
            PropertyId::BorderImage => {
                if let ParsedValue::Gradient(value) = &declaration.value {
                    computed.border_image = Some(value.clone());
                }
            }
            PropertyId::FontFamily => {
                if let ParsedValue::FontFamily(value) = &declaration.value {
                    computed.font_families = value.as_slice().to_vec();
                }
            }
            PropertyId::FontSize => {
                if let ParsedValue::FontSize(value) = &declaration.value {
                    computed.font_size = resolve_font_size_px(*value, computed.font_size, ctx);
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
            PropertyId::VerticalAlign => {
                if let ParsedValue::VerticalAlign(value) = &declaration.value {
                    computed.vertical_align = *value;
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

fn resolve_font_size_px(
    font_size: FontSize,
    parent_font_size: f32,
    ctx: StyleComputeContext<'_>,
) -> f32 {
    font_size.resolve_px(
        parent_font_size,
        ctx.root_font_size,
        ctx.viewport_width,
        ctx.viewport_height,
    )
}

fn max4(a: f32, b: f32, c: f32, d: f32) -> f32 {
    a.max(b).max(c).max(d)
}

#[cfg(test)]
mod tests {
    use super::{StyleComputeContext, compute_style, compute_style_with_context};
    use crate::style::{
        Align, CrossAxis, CrossSize, FlowDirection, FlowWrap, JustifyContent, Layout, Length,
    };
    use crate::style::{
        BoxShadow, Color, FontSize, Opacity, ParsedValue, PropertyId, SelectionStyle, SizeValue,
        Style, TextWrap,
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
}
