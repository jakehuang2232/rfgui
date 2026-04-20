use crate::{CheckIcon, use_theme};
use rfgui::ui::{
    Binding, ClickHandlerProp, PointerEnterHandlerProp, PointerLeaveHandlerProp,
    RsxComponent, RsxNode, props, rsx, use_state,
};
use rfgui::view::{Element, Text};
use rfgui::{Align, Border, Color, JustifyContent, Layout, Transition, TransitionProperty};
use std::rc::Rc;

pub struct Checkbox;

#[props]
pub struct CheckboxProps {
    pub label: String,
    pub binding: Option<Binding<bool>>,
    pub checked: Option<bool>,
    pub disabled: Option<bool>,
    pub on_change: Option<Rc<dyn Fn(bool)>>,
}

impl RsxComponent<CheckboxProps> for Checkbox {
    fn render(props: CheckboxProps, _children: Vec<RsxNode>) -> RsxNode {
        let checked = props.checked.unwrap_or(false);
        let has_binding = props.binding.is_some();
        let binding = props.binding.unwrap_or_else(|| Binding::new(checked));
        let disabled = props.disabled.unwrap_or(false);
        let on_change = props.on_change;
        let label = props.label;
        let theme = use_theme().0;
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
            let next = !checked_binding.get();
            checked_binding.set(next);
            if let Some(cb) = &on_change {
                cb(next);
            }
        });

        let on_pointer_enter =
            PointerEnterHandlerProp::new(move |_event| hover_state_for_enter.set(true));
        let on_pointer_leave =
            PointerLeaveHandlerProp::new(move |_event| hover_state_for_leave.set(false));

        rsx! {
            <Element style={{
                layout: Layout::flow().row().align(Align::Center).no_wrap(),
                gap: theme.spacing.md,
            }}
            on_click={click}
            on_pointer_enter={on_pointer_enter}
            on_pointer_leave={on_pointer_leave}
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
                    layout: Layout::flex().justify_content(JustifyContent::Center).align(Align::Center),
                }} >
                    <CheckIcon style={{
                        color: if checked {
                            if disabled { theme.color.text.disabled.clone() } else { theme.color.surface.on.clone() }
                        }else {
                            Color::transparent()
                        },
                        font_size: theme.typography.size.md,
                        transition: [Transition::new(TransitionProperty::Color, 180).timing(theme.motion.easing.standard)]
                    }}/>
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
}

impl rfgui::ui::RsxTag for Checkbox {
    type Props = __CheckboxPropsInit;
    type StrictProps = CheckboxProps;
    const ACCEPTS_CHILDREN: bool = false;

    fn into_strict(props: Self::Props) -> Self::StrictProps {
        props.into()
    }

    fn create_node(
        props: Self::StrictProps,
        children: Vec<rfgui::ui::RsxNode>,
        _key: Option<rfgui::ui::RsxKey>,
    ) -> rfgui::ui::RsxNode {
        <Self as RsxComponent<CheckboxProps>>::render(props, children)
    }
}
