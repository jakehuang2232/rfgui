use crate::use_theme;
use rfgui::TextAlign::Center;
use rfgui::ui::host::{Element, Text};
use rfgui::ui::{ClickHandlerProp, RsxComponent, RsxNode, props, rsx};
use rfgui::{
    AlignItems, Border, Color, ColorLike, Cursor, JustifyContent, Layout, Length, Transition,
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
        let mut root = rsx! {
            <Element
                style={{
                    layout: Layout::flow()
                        .row()
                        .no_wrap()
                        .justify_content(JustifyContent::Center)
                        .align_items(AlignItems::Center),
                    padding: theme.component.button.padding,
                    border_radius: theme.component.button.radius,
                    border: if disabled {
                        Border::uniform(Length::px(0.5), theme.color.state.disabled.as_ref())
                    } else {
                        match variant {
                            ButtonVariant::Contained => Border::uniform(Length::px(0.5), theme.color.primary.base.as_ref()),
                            ButtonVariant::Outlined => Border::uniform(Length::px(0.5), theme.color.primary.base.as_ref()),
                            ButtonVariant::Text => Border::uniform(
                                Length::px(0.5),
                                (Box::new(Color::transparent()) as Box<dyn ColorLike>).as_ref(),
                            ),
                        }
                    },
                    background: if disabled {
                        theme.color.state.disabled.clone()
                    } else {
                        match variant {
                            ButtonVariant::Contained => theme.color.primary.base.clone(),
                            ButtonVariant::Outlined => None,
                            ButtonVariant::Text => Box::new(Color::transparent()) as Box<dyn ColorLike>,
                        }
                    },
                    transition: Transitions::single(
                        Transition::new(TransitionProperty::BackgroundColor, theme.motion.duration.normal)
                            .ease_in_out(),
                    ),
                    cursor: Cursor::Pointer,
                    hover: {
                        background: if disabled {
                            theme.color.state.disabled.clone()
                        } else {
                            match variant {
                                ButtonVariant::Contained => theme.color.state.active.clone(),
                                ButtonVariant::Outlined => theme.color.state.hover.clone(),
                                ButtonVariant::Text => theme.color.state.hover.clone(),
                            }
                        },
                    },
                }}
            >
                <Text
                    font_size={theme.typography.size.sm}
                    align={Center}
                    style={{
                        color: if disabled {
                            theme.color.text.disabled.clone()
                        } else {
                            match variant {
                                ButtonVariant::Contained => theme.color.primary.on.clone(),
                                ButtonVariant::Outlined => theme.color.text.primary.clone(),
                                ButtonVariant::Text => theme.color.text.primary.clone(),
                            }
                        }
                    }}
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
