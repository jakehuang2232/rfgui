use crate::style::color::Color;
use crate::style::parsed_style::{
    AlignItems, Display, FlowDirection, FlowWrap, JustifyContent, Length, ParsedValue, Position,
    PropertyId, ScrollDirection, Style, Transitions,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SizeValue {
    Auto,
    Length(Length),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgeInsets<T> {
    pub top: T,
    pub right: T,
    pub bottom: T,
    pub left: T,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CornerRadii<T> {
    pub top_left: T,
    pub top_right: T,
    pub bottom_right: T,
    pub bottom_left: T,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ComputedStyle {
    pub display: Display,
    pub flow_direction: FlowDirection,
    pub flow_wrap: FlowWrap,
    pub justify_content: JustifyContent,
    pub align_items: AlignItems,
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
    pub color: Color,
    pub background_color: Color,
    pub font_families: Vec<String>,
    pub font_size: f32,
    pub font_weight: u16,
    pub line_height: f32,
    pub border_radius: f32,
    pub border_radii: CornerRadii<Length>,
    pub border_width: f32,
    pub border_color: Color,
    pub border_widths: EdgeInsets<Length>,
    pub border_colors: EdgeInsets<Color>,
    pub opacity: f32,
    pub transition: Transitions,
}

impl Default for ComputedStyle {
    fn default() -> Self {
        Self {
            display: Display::Block,
            flow_direction: FlowDirection::Row,
            flow_wrap: FlowWrap::NoWrap,
            justify_content: JustifyContent::Start,
            align_items: AlignItems::Start,
            position: Position::Static,
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
            color: Color::rgb(0, 0, 0),
            background_color: Color::rgba(0, 0, 0, 0),
            font_families: Vec::new(),
            font_size: 16.0,
            font_weight: 400,
            line_height: 1.2,
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
            transition: Transitions::default(),
        }
    }
}

pub fn compute_style(parsed: &Style, parent: Option<&ComputedStyle>) -> ComputedStyle {
    let mut computed = ComputedStyle::default();

    if let Some(parent) = parent {
        computed.color = parent.color;
        computed.font_families = parent.font_families.clone();
        computed.font_size = parent.font_size;
        computed.font_weight = parent.font_weight;
        computed.line_height = parent.line_height;
    }

    for declaration in parsed.declarations() {
        match declaration.property {
            PropertyId::Display => {
                if let ParsedValue::Display(value) = &declaration.value {
                    computed.display = *value;
                }
            }
            PropertyId::FlowDirection => {
                if let ParsedValue::FlowDirection(value) = &declaration.value {
                    computed.flow_direction = *value;
                }
            }
            PropertyId::FlowWrap => {
                if let ParsedValue::FlowWrap(value) = &declaration.value {
                    computed.flow_wrap = *value;
                }
            }
            PropertyId::JustifyContent => {
                if let ParsedValue::JustifyContent(value) = &declaration.value {
                    computed.justify_content = *value;
                }
            }
            PropertyId::AlignItems => {
                if let ParsedValue::AlignItems(value) = &declaration.value {
                    computed.align_items = *value;
                }
            }
            PropertyId::Position => {
                if let ParsedValue::Position(value) = &declaration.value {
                    computed.position = *value;
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
                if let ParsedValue::Length(Length::Px(px)) = &declaration.value {
                    computed.font_size = px.max(0.0);
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
            PropertyId::Transition => {
                if let ParsedValue::Transition(value) = &declaration.value {
                    computed.transition = value.clone();
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
    Some(*raw)
}

fn resolve_length_px(length: Length) -> f32 {
    match length {
        Length::Px(v) => v,
        Length::Percent(v) => v,
        Length::Zero => 0.0,
    }
}

fn max4(a: f32, b: f32, c: f32, d: f32) -> f32 {
    a.max(b).max(c).max(d)
}
