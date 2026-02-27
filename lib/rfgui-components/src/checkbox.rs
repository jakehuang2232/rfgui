use crate::use_theme;
use rfgui::ui::host::{Element, Text};
use rfgui::ui::{
    Binding, ClickHandlerProp, RsxComponent, RsxNode, component, props, rsx, use_state,
};
use rfgui::{
    AlignItems, Border, BorderRadius, Color, Display, Length, Padding, Transition,
    TransitionProperty,
};

pub struct Checkbox;

#[props]
pub struct CheckboxProps {
    pub label: String,
    pub binding: Option<Binding<bool>>,
    pub checked: Option<bool>,
    pub disabled: Option<bool>,
}

impl RsxComponent<CheckboxProps> for Checkbox {
    fn render(props: CheckboxProps) -> RsxNode {
        let checked = props.checked.unwrap_or(false);
        let has_binding = props.binding.is_some();
        let binding = props.binding.unwrap_or_else(|| Binding::new(checked));

        rsx! {
            <CheckboxView
                label={props.label}
                checked={checked}
                has_binding={has_binding}
                binding={binding}
                disabled={props.disabled.unwrap_or(false)}
            />
        }
    }
}

#[component]
fn CheckboxView(
    label: String,
    checked: bool,
    has_binding: bool,
    binding: Binding<bool>,
    disabled: bool,
) -> RsxNode {
    let theme = use_theme().get();
    let fallback_checked = use_state(|| checked);
    let checked_binding = if has_binding {
        binding
    } else {
        fallback_checked.binding()
    };
    let checked = checked_binding.get();

    let click = if disabled {
        None
    } else {
        Some(ClickHandlerProp::new(move |_event| {
            checked_binding.set(!checked_binding.get())
        }))
    };

    let mut root = rsx! {
        <Element style={{
            display: Display::flow().row().no_wrap(),
            align_items: AlignItems::Center,
            gap: theme.spacing.md,
        }}>
            <Element style={{
                width: Length::px(18.0),
                height: Length::px(18.0),
                border_radius: BorderRadius::uniform(theme.radius.sm),
                background: if disabled {
                    theme.color.state.disabled.clone()
                } else if checked {
                    theme.color.primary.base.clone()
                } else {
                    theme.color.layer.surface.clone()
                },
                border: if disabled {
                    Border::uniform(Length::px(1.0), theme.color.border.as_ref())
                } else if checked {
                    Border::uniform(Length::px(1.0), theme.color.primary.base.as_ref())
                } else {
                    Border::uniform(Length::px(1.0), theme.color.border.as_ref())
                },
                transition: [Transition::new(TransitionProperty::BackgroundColor, 180).timing(theme.motion.easing.standard)]
            }} >
                <Element
                    style={{
                        color: if checked {
                            if disabled { theme.color.text.disabled.clone() } else { theme.color.surface.on.clone() }
                        }else {
                            Box::new(Color::transparent())
                        },
                        font_size: theme.typography.size.md,
                        transition: [Transition::new(TransitionProperty::Color, 180).timing(theme.motion.easing.standard)]
                    }}
                >
                    {"✓"}
                </Element>
            </Element>
            <Text
                font_size={theme.typography.size.sm}
                style={{ color: if disabled { theme.color.text.disabled.clone() } else { theme.color.text.primary.clone() } }}
            >
                {label}
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
