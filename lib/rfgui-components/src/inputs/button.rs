use crate::use_theme;
use rfgui::TextAlign::Center;
use rfgui::ui::host::{Element, Text};
use rfgui::ui::{ClickHandlerProp, RsxChildrenPolicy, RsxComponent, RsxNode, props, rsx};
use rfgui::{
    Align, Border, Color, ColorLike, Cursor, JustifyContent, Layout, Length, Transition,
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
    fn render(props: ButtonProps, _children: Vec<RsxNode>) -> RsxNode {
        let theme = use_theme().get();
        let variant = props.variant.unwrap_or(ButtonVariant::Contained);
        let disabled = props.disabled.unwrap_or(false);
        let transparent = Box::new(Color::transparent()) as Box<dyn ColorLike>;
        let border = if disabled {
            match variant {
                ButtonVariant::Contained => {
                    Border::uniform(Length::px(0.5), theme.color.state.disabled.as_ref())
                }
                ButtonVariant::Outlined => {
                    Border::uniform(Length::px(0.5), theme.color.state.disabled.as_ref())
                }
                ButtonVariant::Text => Border::uniform(Length::px(0.5), transparent.as_ref()),
            }
        } else {
            match variant {
                ButtonVariant::Contained => {
                    Border::uniform(Length::px(0.5), theme.color.primary.base.as_ref())
                }
                ButtonVariant::Outlined => {
                    Border::uniform(Length::px(0.5), theme.color.primary.base.as_ref())
                }
                ButtonVariant::Text => Border::uniform(Length::px(0.5), transparent.as_ref()),
            }
        };
        let background: Box<dyn ColorLike> = if disabled {
            match variant {
                ButtonVariant::Contained => theme.color.state.disabled.clone(),
                ButtonVariant::Outlined | ButtonVariant::Text => {
                    Box::new(Color::transparent()) as Box<dyn ColorLike>
                }
            }
        } else {
            match variant {
                ButtonVariant::Contained => theme.color.primary.base.clone(),
                ButtonVariant::Outlined => Box::new(Color::transparent()) as Box<dyn ColorLike>,
                ButtonVariant::Text => Box::new(Color::transparent()) as Box<dyn ColorLike>,
            }
        };
        let hover_background: Box<dyn ColorLike> = if disabled {
            background.clone()
        } else {
            match variant {
                ButtonVariant::Contained => theme.color.state.active.clone(),
                ButtonVariant::Outlined => theme.color.state.hover.clone(),
                ButtonVariant::Text => theme.color.state.hover.clone(),
            }
        };
        let text_color = if disabled {
            theme.color.text.disabled.clone()
        } else {
            match variant {
                ButtonVariant::Contained => theme.color.primary.on.clone(),
                ButtonVariant::Outlined => theme.color.text.primary.clone(),
                ButtonVariant::Text => theme.color.text.primary.clone(),
            }
        };
        let mut root = rsx! {
            <Element
                style={{
                    layout: Layout::flow()
                        .row()
                        .no_wrap()
                        .justify_content(JustifyContent::Center)
                        .align(Align::Center),
                    padding: theme.component.button.padding,
                    border_radius: theme.component.button.radius,
                    border: border,
                    background: background,
                    transition: Transitions::single(
                        Transition::new(TransitionProperty::BackgroundColor, theme.motion.duration.normal)
                            .ease_in_out(),
                    ),
                    cursor: if disabled { Cursor::Default } else { Cursor::Pointer },
                    hover: {
                        background: hover_background,
                    },
                }}
            >
                <Text
                    font_size={theme.typography.size.sm}
                    align={Center}
                    style={{
                        color: text_color
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

impl RsxChildrenPolicy for Button {
    const ACCEPTS_CHILDREN: bool = false;
}
