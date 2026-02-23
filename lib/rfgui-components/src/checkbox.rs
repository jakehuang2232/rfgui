use rfgui::ui::host::{Element, Text};
use rfgui::ui::{Binding, ClickHandlerProp, RsxNode, rsx};
use rfgui::{
    AlignItems, Border, BorderRadius, Color, Display, FlowDirection, Length, Padding, ParsedValue,
    PropertyId, Style,
};

pub struct CheckboxProps {
    pub label: String,
    pub checked: bool,
    pub checked_binding: Option<Binding<bool>>,
    pub width: f32,
    pub height: f32,
    pub disabled: bool,
}

impl CheckboxProps {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            checked: false,
            checked_binding: None,
            width: 220.0,
            height: 32.0,
            disabled: false,
        }
    }
}

pub fn build_checkbox_rsx(props: CheckboxProps) -> RsxNode {
    let checked = props
        .checked_binding
        .as_ref()
        .map(|v| v.get())
        .unwrap_or(props.checked);
    let click = if props.disabled {
        None
    } else {
        props.checked_binding
            .clone()
            .map(|binding| ClickHandlerProp::new(move |_event| binding.set(!binding.get())))
    };

    let mut root = rsx! {
        <Element style={checkbox_row_style(props.width, props.height)}>
            <Element style={checkbox_box_style(checked, props.disabled)}>
                {if checked {
                    rsx! {
                        <Text
                            x=2
                            y=-1
                            font_size=16
                            font="Heiti TC, Noto Sans CJK TC, Roboto"
                            color={if props.disabled { "#9E9E9E" } else { "#FFFFFF" }}
                        >
                            {"✓"}
                        </Text>
                    }
                } else {
                    RsxNode::fragment(Vec::new())
                }}
            </Element>
            <Text
                x=28
                y=7
                font_size=14
                font="Heiti TC, Noto Sans CJK TC, Roboto"
                color={if props.disabled { "#9E9E9E" } else { "#1F2937" }}
            >
                {props.label}
            </Text>
        </Element>
    };

    if let Some(handler) = click
        && let RsxNode::Element(node) = &mut root
    {
        node.props.push(("on_click".to_string(), handler.into()));
    }

    root
}

fn checkbox_row_style(width: f32, height: f32) -> Style {
    let mut style = Style::new();
    style.insert(PropertyId::Display, ParsedValue::Display(Display::Flow));
    style.insert(
        PropertyId::FlowDirection,
        ParsedValue::FlowDirection(FlowDirection::Row),
    );
    style.insert(
        PropertyId::AlignItems,
        ParsedValue::AlignItems(AlignItems::Center),
    );
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(width)));
    style.insert(PropertyId::Height, ParsedValue::Length(Length::px(height)));
    style.set_padding(Padding::uniform(Length::px(0.0)));
    style
}

fn checkbox_box_style(checked: bool, disabled: bool) -> Style {
    let mut style = Style::new();
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(18.0)));
    style.insert(PropertyId::Height, ParsedValue::Length(Length::px(18.0)));
    style.set_border_radius(BorderRadius::uniform(Length::px(4.0)));
    if disabled {
        style.insert_color_like(PropertyId::BackgroundColor, Color::hex("#F5F5F5"));
        style.set_border(Border::uniform(Length::px(1.0), &Color::hex("#BDBDBD")));
    } else if checked {
        style.insert_color_like(PropertyId::BackgroundColor, Color::hex("#1976D2"));
        style.set_border(Border::uniform(Length::px(1.0), &Color::hex("#1976D2")));
    } else {
        style.insert_color_like(PropertyId::BackgroundColor, Color::hex("#FFFFFF"));
        style.set_border(Border::uniform(Length::px(1.0), &Color::hex("#6B7280")));
    }
    style
}
