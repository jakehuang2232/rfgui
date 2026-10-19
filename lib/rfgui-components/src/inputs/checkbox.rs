use crate::material_symbol::CheckIcon;
use crate::use_theme;
use rfgui::style::{Align, Border, Color, JustifyContent, Layout, Transition, TransitionProperty};
use rfgui::ui::{
    Binding, ClickHandlerProp, PointerEnterHandlerProp, PointerLeaveHandlerProp, RsxComponent,
    RsxNode, props, rsx, use_state,
};
use rfgui::view::{Element, Text};
use std::rc::Rc;

/// A box that shows a checkmark when checked, with a label beside it.
/// Clicking the box or the label toggles it. A label that does not fit the
/// width the checkbox gets wraps beside the box.
pub struct Checkbox;

#[derive(Clone)]
#[props]
pub struct CheckboxProps {
    /// Shown to the right of the box.
    pub label: String,
    /// Holds the checked state: the checkbox reads it and writes each toggle
    /// to it.
    pub binding: Option<Binding<bool>>,
    /// The initial state when there is no `binding`; later changes are
    /// ignored, so use `binding` to set the state from outside.
    pub checked: Option<bool>,
    /// Ignores clicks and uses the disabled colors.
    pub disabled: Option<bool>,
    /// Called with the new state after each toggle.
    pub on_change: Option<Rc<dyn Fn(bool)>>,
}

impl RsxComponent<CheckboxProps> for Checkbox {
    fn render(props: &CheckboxProps, _children: Vec<RsxNode>) -> RsxNode {
        let checked = props.checked.unwrap_or(false);
        let has_binding = props.binding.is_some();
        let binding = props
            .binding
            .clone()
            .unwrap_or_else(|| Binding::new(checked));
        let disabled = props.disabled.unwrap_or(false);
        let on_change = props.on_change.clone();
        let label = props.label.clone();
        let theme = use_theme().0;
        let checkbox_theme = &theme.component.checkbox;
        let fallback_checked = use_state(|| checked);
        let checked_binding = if has_binding {
            binding.snapshot()
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
            let next = !checked_binding.get_committed();
            checked_binding.set(next);
            if let Some(cb) = &on_change {
                cb(next);
            }
        });

        let on_pointer_enter =
            PointerEnterHandlerProp::new(move |_event| hover_state_for_enter.set(true));
        let on_pointer_leave =
            PointerLeaveHandlerProp::new(move |_event| hover_state_for_leave.set(false));

        // A flex row, not a flow row: a flow row measures the label against
        // the whole row, so a long one would push past the available width
        // instead of wrapping beside the box.
        rsx! {
            <Element style={{
                layout: Layout::flex().row().align(Align::Center),
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
                    style={{
                        font_size: theme.typography.size.sm,
                        color: if disabled { theme.color.text.disabled.clone() } else { theme.color.text.primary.clone() }
                    }}
                >
                    {label}
                </Text>
            </Element>
        }
    }
}

#[rfgui::ui::component]
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
        <Self as RsxComponent<CheckboxProps>>::render(&props, children)
    }
}
