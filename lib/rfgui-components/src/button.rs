use crate::use_theme;
use rfgui::TextAlign::Center;
use rfgui::ui::host::{Element, Text};
use rfgui::ui::{ClickHandlerProp, RsxComponent, RsxNode, props, rsx};
use rfgui::{
    AlignItems, Border, Color, ColorLike, Cursor, Display, JustifyContent, Length, Transition,
    TransitionProperty, Transitions,
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
        let theme = use_theme().get();
        let variant = props.variant.unwrap_or(ButtonVariant::Contained);
        let disabled = props.disabled.unwrap_or(false);
        let (background, border_color, hover_background, text_color): (
            Box<dyn ColorLike>,
            Box<dyn ColorLike>,
            Box<dyn ColorLike>,
            Box<dyn ColorLike>,
        ) = if disabled {
            (
                theme.color.state.disabled.clone(),
                theme.color.state.disabled.clone(),
                theme.color.state.disabled.clone(),
                theme.color.text.disabled.clone(),
            )
        } else {
            match variant {
                ButtonVariant::Contained => (
                    theme.color.primary.base.clone(),
                    theme.color.primary.base.clone(),
                    theme.color.state.active.clone(),
                    theme.color.primary.on.clone(),
                ),
                ButtonVariant::Outlined => (
                    theme.color.layer.surface.clone(),
                    theme.color.primary.base.clone(),
                    theme.color.state.hover.clone(),
                    theme.color.text.primary.clone(),
                ),
                ButtonVariant::Text => (
                    Box::new(Color::transparent()) as Box<dyn ColorLike>,
                    Box::new(Color::transparent()) as Box<dyn ColorLike>,
                    theme.color.state.hover.clone(),
                    theme.color.text.primary.clone(),
                ),
            }
        };
        let transition_duration = theme.motion.duration.normal;
        let mut root = rsx! {
            <Element
                style={{
                    display: Display::flow()
                        .row()
                        .no_wrap()
                        .justify_content(JustifyContent::Center),
                    align_items: AlignItems::Center,
                    padding: theme.component.button.padding,
                    border_radius: theme.component.button.radius,
                    border: Border::uniform(Length::px(1.0), border_color.as_ref()),
                    background: background,
                    transition: Transitions::single(
                        Transition::new(TransitionProperty::BackgroundColor, transition_duration)
                            .ease_in_out(),
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
