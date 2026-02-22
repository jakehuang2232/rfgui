use crate::style::{
    AlignItems, Border, BorderRadius, Color, Display, FlowDirection, JustifyContent, Length,
    ParsedValue, PropertyId, Style,
};
use crate::view::base_component::{Element, Text};
use crate::HexColor;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonVariant {
    Contained,
    Outlined,
    Text,
}

pub struct ButtonProps {
    pub label: String,
    pub width: f32,
    pub height: f32,
    pub variant: ButtonVariant,
    pub disabled: bool,
}

impl ButtonProps {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            width: 124.0,
            height: 36.0,
            variant: ButtonVariant::Contained,
            disabled: false,
        }
    }
}

pub fn build_button(props: ButtonProps) -> Element {
    let mut root = Element::new(0.0, 0.0, props.width, props.height);
    root.apply_style(button_style(props.variant, props.disabled, props.width, props.height));

    let mut label = Text::from_content(props.label);
    label.set_font("Roboto, Noto Sans CJK TC");
    label.set_font_size(14.0);
    label.set_position(16.0, 9.0);
    label.set_color(button_text_color(props.variant, props.disabled));
    root.add_child(Box::new(label));

    root
}

fn button_style(variant: ButtonVariant, disabled: bool, width: f32, height: f32) -> Style {
    let mut style = Style::new();
    style.insert(PropertyId::Display, ParsedValue::Display(Display::Flow));
    style.insert(
        PropertyId::FlowDirection,
        ParsedValue::FlowDirection(FlowDirection::Row),
    );
    style.insert(
        PropertyId::JustifyContent,
        ParsedValue::JustifyContent(JustifyContent::Center),
    );
    style.insert(
        PropertyId::AlignItems,
        ParsedValue::AlignItems(AlignItems::Center),
    );
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(width)));
    style.insert(PropertyId::Height, ParsedValue::Length(Length::px(height)));
    style.set_border_radius(BorderRadius::uniform(Length::px(8.0)));

    if disabled {
        style.insert_color_like(PropertyId::BackgroundColor, Color::hex("#E0E0E0"));
        style.set_border(Border::uniform(Length::px(1.0), &Color::hex("#E0E0E0")));
        return style;
    }

    match variant {
        ButtonVariant::Contained => {
            style.insert_color_like(PropertyId::BackgroundColor, Color::hex("#1976D2"));
            style.set_border(Border::uniform(Length::px(1.0), &Color::hex("#1976D2")));
            let mut hover = Style::new();
            hover.insert_color_like(PropertyId::BackgroundColor, Color::hex("#1565C0"));
            style.set_hover(hover);
        }
        ButtonVariant::Outlined => {
            style.insert_color_like(PropertyId::BackgroundColor, Color::hex("#FFFFFF"));
            style.set_border(Border::uniform(Length::px(1.0), &Color::hex("#1976D2")));
            let mut hover = Style::new();
            hover.insert_color_like(PropertyId::BackgroundColor, Color::hex("#E3F2FD"));
            style.set_hover(hover);
        }
        ButtonVariant::Text => {
            style.insert_color_like(PropertyId::BackgroundColor, Color::hex("#FFFFFF"));
            style.set_border(Border::uniform(Length::px(1.0), &Color::hex("#FFFFFF")));
            let mut hover = Style::new();
            hover.insert_color_like(PropertyId::BackgroundColor, Color::hex("#E3F2FD"));
            style.set_hover(hover);
        }
    }

    style
}

fn button_text_color(variant: ButtonVariant, disabled: bool) -> HexColor<'static> {
    if disabled {
        return HexColor::new("#9E9E9E");
    }
    match variant {
        ButtonVariant::Contained => HexColor::new("#FFFFFF"),
        ButtonVariant::Outlined | ButtonVariant::Text => HexColor::new("#1976D2"),
    }
}
