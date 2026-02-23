use rfgui::ui::host::{Element, Text};
use rfgui::ui::{ClickHandlerProp, RsxNode, rsx};
use rfgui::{
    AlignItems, Border, BorderRadius, Color, Display, FlowDirection, JustifyContent, Length,
    ParsedValue, PropertyId, Style, Transition, TransitionProperty, Transitions,
};

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
    pub on_click: Option<ClickHandlerProp>,
}

impl ButtonProps {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            width: 124.0,
            height: 36.0,
            variant: ButtonVariant::Contained,
            disabled: false,
            on_click: None,
        }
    }
}

pub fn build_button_rsx(props: ButtonProps) -> RsxNode {
    let click = props.on_click.clone();
    let disabled = props.disabled;
    let mut root = rsx! {
        <Element style={button_style(
            props.variant,
            props.disabled,
            props.width,
            props.height,
        )}>
            <Text
                font_size=14
                line_height=1.0
                font="Heiti TC, Noto Sans CJK TC, Roboto"
                color={button_text_color_hex(props.variant, props.disabled)}
            >
                {props.label}
            </Text>
        </Element>
    };

    if !disabled
        && let Some(handler) = click
        && let RsxNode::Element(node) = &mut root
    {
        node.props.push(("on_click".to_string(), handler.into()));
    }

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
    style.insert(
        PropertyId::Transition,
        ParsedValue::Transition(Transitions::single(
            Transition::new(TransitionProperty::BackgroundColor, 180).ease_in_out(),
        )),
    );

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

fn button_text_color_hex(variant: ButtonVariant, disabled: bool) -> &'static str {
    if disabled {
        return "#9E9E9E";
    }
    match variant {
        ButtonVariant::Contained => "#FFFFFF",
        ButtonVariant::Outlined | ButtonVariant::Text => "#1976D2",
    }
}
