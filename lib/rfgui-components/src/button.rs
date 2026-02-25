use rfgui::TextAlign::Center;
use rfgui::ui::host::{Element, Text};
use rfgui::ui::{ClickHandlerProp, RsxComponent, RsxNode, props, rsx};
use rfgui::{
    AlignItems, Border, BorderRadius, Color, Cursor, Display, FlowDirection, JustifyContent,
    Length, Padding, Transition, TransitionProperty, Transitions,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonVariant {
    Contained,
    Outlined,
    Text,
}

impl From<&str> for ButtonVariant {
    fn from(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "contained" => ButtonVariant::Contained,
            "outlined" => ButtonVariant::Outlined,
            "text" => ButtonVariant::Text,
            other => panic!("rsx build error on <Button>. unknown Button variant `{other}`"),
        }
    }
}

impl From<String> for ButtonVariant {
    fn from(value: String) -> Self {
        ButtonVariant::from(value.as_str())
    }
}

pub struct Button;

#[props]
pub struct ButtonProps {
    pub label: String,
    pub variant: Option<ButtonVariant>,
    pub disabled: Option<bool>,
    pub on_click: Option<ClickHandlerProp>,
}

impl RsxComponent<ButtonProps> for Button {
    fn render(props: ButtonProps) -> RsxNode {
        let variant = props.variant.unwrap_or(ButtonVariant::Contained);
        let disabled = props.disabled.unwrap_or(false);
        let (background, border_color, hover_background, text_color) = if disabled {
            (
                Color::hex("#E0E0E0"),
                Color::hex("#E0E0E0"),
                Color::hex("#E0E0E0"),
                "#9E9E9E",
            )
        } else {
            match variant {
                ButtonVariant::Contained => (
                    Color::hex("#1976D2"),
                    Color::hex("#1976D2"),
                    Color::hex("#1565C0"),
                    "#FFFFFF",
                ),
                ButtonVariant::Outlined => (
                    Color::hex("#FFFFFF"),
                    Color::hex("#1976D2"),
                    Color::hex("#E3F2FD"),
                    "#1976D2",
                ),
                ButtonVariant::Text => (
                    Color::hex("#FFFFFF"),
                    Color::hex("#FFFFFF"),
                    Color::hex("#E3F2FD"),
                    "#1976D2",
                ),
            }
        };
        let mut root = rsx! {
            <Element
                style={{
                    display: Display::flow().row().no_wrap(),
                    flow_direction: FlowDirection::Row,
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    padding: Padding::uniform(Length::px(8.0)).x(Length::px(16.0)),
                    border_radius: BorderRadius::uniform(Length::px(8.0)),
                    border: Border::uniform(Length::px(1.0), &border_color),
                    background: background,
                    transition: Transitions::single(
                        Transition::new(TransitionProperty::BackgroundColor, 180).ease_in_out(),
                    ),
                    cursor: Cursor::Pointer,
                    hover: {
                        background: hover_background,
                    },
                }}
            >
                <Text
                    align={Center}
                    style={{ color: text_color }}
                >
                    {props.label}
                </Text>
            </Element>
        };

        if !disabled
            && let Some(handler) = props.on_click
            && let RsxNode::Element(node) = &mut root
        {
            node.props.push(("on_click".to_string(), handler.into()));
        }

        root
    }
}
