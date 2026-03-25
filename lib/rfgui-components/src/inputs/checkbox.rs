use crate::use_theme;
use rfgui::ui::host::{Element, Text};
use rfgui::ui::{
    Binding, ClickHandlerProp, MouseEnterHandlerProp, MouseLeaveHandlerProp, RsxChildrenPolicy,
    RsxComponent, RsxNode, component, props, rsx, use_state,
};
use rfgui::{Align, Border, Color, Layout, Transition, TransitionProperty};

pub struct Checkbox;

#[props]
pub struct CheckboxProps {
    pub label: String,
    pub binding: Option<Binding<bool>>,
    pub checked: Option<bool>,
    pub disabled: Option<bool>,
}

impl RsxComponent<CheckboxProps> for Checkbox {
    fn render(props: CheckboxProps, _children: Vec<RsxNode>) -> RsxNode {
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

impl RsxChildrenPolicy for Checkbox {
    const ACCEPTS_CHILDREN: bool = false;
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
    let checkbox_theme = &theme.component.checkbox;
    let fallback_checked = use_state(|| checked);
    let checked_binding = if has_binding {
        binding
    } else {
        fallback_checked.binding()
    };
    let hover_state = use_state(|| false);
    let hover_state_for_enter = hover_state.clone();
    let hover_state_for_leave = hover_state.clone();
    let checked = checked_binding.get();
    let hovered = hover_state.get();
    let click = ClickHandlerProp::new(move |_event| {
        if disabled {
            return;
        }
        checked_binding.set(!checked_binding.get());
    });

    let on_mouse_enter = MouseEnterHandlerProp::new(move |_event| hover_state_for_enter.set(true));
    let on_mouse_leave = MouseLeaveHandlerProp::new(move |_event| hover_state_for_leave.set(false));

    rsx! {
        <Element style={{
            layout: Layout::flow().row().align(Align::Center).no_wrap(),
            gap: theme.spacing.md,
        }}
        on_click={click}
        on_mouse_enter={on_mouse_enter}
        on_mouse_leave={on_mouse_leave}
        >
            <Element style={{
                width: checkbox_theme.size,
                height: checkbox_theme.size,
                border_radius: checkbox_theme.radius,
                background: if disabled {
                    theme.color.state.disabled.clone()
                } else if checked {
                    theme.color.primary.base.clone()
                } else if hovered {
                    theme.color.state.hover.clone()
                } else {
                    None
                },
                border: if disabled {
                    Border::uniform(checkbox_theme.border_width, theme.color.border.as_ref())
                } else if checked {
                    Border::uniform(checkbox_theme.border_width, theme.color.primary.base.as_ref())
                } else {
                    Border::uniform(checkbox_theme.border_width, theme.color.border.as_ref())
                },
                transition: [Transition::new(TransitionProperty::BackgroundColor, 180).timing(theme.motion.easing.standard)],
            }} >
                <Element
                    style={{
                        color: if checked {
                            if disabled { theme.color.text.disabled.clone() } else { theme.color.surface.on.clone() }
                        }else {
                            Color::transparent()
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
                style={{
                    color: if disabled { theme.color.text.disabled.clone() } else { theme.color.text.primary.clone() }
                }}
            >
                {label}
            </Text>
        </Element>
    }
}
