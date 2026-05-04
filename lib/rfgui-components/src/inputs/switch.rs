use crate::use_theme;
use rfgui::style::{
    Align, ColorLike, Layout, Length, Operator, Transition, TransitionProperty, darken_color,
};
use rfgui::ui::{
    Binding, PointerEnterHandlerProp, PointerLeaveHandlerProp, RsxComponent, RsxNode, on_click,
    props, rsx, use_state,
};
use rfgui::view::{Element, Text};
use std::rc::Rc;

pub struct Switch;

#[derive(Clone)]
#[props]
pub struct SwitchProps {
    pub label: String,
    pub binding: Option<Binding<bool>>,
    pub checked: Option<bool>,
    pub disabled: Option<bool>,
    pub on_change: Option<Rc<dyn Fn(bool)>>,
}

impl RsxComponent<SwitchProps> for Switch {
    fn render(props: SwitchProps, _children: Vec<RsxNode>) -> RsxNode {
        let checked = props.checked.unwrap_or(false);
        let has_binding = props.binding.is_some();
        let binding = props.binding.unwrap_or_else(|| Binding::new(checked));
        let disabled = props.disabled.unwrap_or(false);
        let label = props.label;
        let theme = use_theme().0;
        let switch_theme = &theme.component.switch;
        let thumb_travel = Length::calc(
            Length::calc(
                Length::calc(
                    switch_theme.track_width,
                    Operator::subtract,
                    switch_theme.track_padding.left,
                ),
                Operator::subtract,
                switch_theme.track_padding.right,
            ),
            Operator::subtract,
            switch_theme.thumb_width,
        );
        let fallback_checked = use_state(|| checked);
        let checked_binding = if has_binding {
            binding
        } else {
            fallback_checked.binding()
        };
        let checked = checked_binding.get();

        let on_change = props.on_change;
        let click = on_click(move |_event| {
            if disabled {
                return;
            }
            let next = !checked_binding.get();
            checked_binding.set(next);
            if let Some(cb) = on_change.as_ref() {
                cb(next);
            }
        });

        let hover_state = use_state(|| false);
        let hover_state_for_enter = hover_state.clone();
        let hover_state_for_leave = hover_state.clone();
        let hovered = hover_state.get();
        let on_pointer_enter =
            PointerEnterHandlerProp::new(move |_event| hover_state_for_enter.set(true));
        let on_pointer_leave =
            PointerLeaveHandlerProp::new(move |_event| hover_state_for_leave.set(false));

        let track_base: Box<dyn ColorLike> = if disabled {
            theme.color.state.disabled.clone()
        } else if checked {
            theme.color.primary.base.clone()
        } else {
            theme.color.border.clone()
        };
        let track_background: Box<dyn ColorLike> = if !disabled && hovered {
            let amount = if checked {
                theme.color.state.hover_darken
            } else {
                -theme.color.state.hover_darken
            };
            Box::new(darken_color(track_base.as_ref(), amount))
        } else {
            track_base
        };

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
                    layout: Layout::flow().row().align(Align::Center).no_wrap(),
                    width: switch_theme.track_width,
                    height: switch_theme.track_height,
                    padding: switch_theme.track_padding,
                    border_radius: switch_theme.track_radius,
                    transition: [
                        Transition::new(
                            TransitionProperty::BackgroundColor,
                            theme.motion.duration.normal,
                        )
                        .ease_in_out(),
                    ],
                    background: track_background,
                }}
                >
                    <Element style={{
                        width: if checked { thumb_travel } else { Length::Zero },
                        height: switch_theme.thumb_height,
                        transition: [
                            Transition::new(TransitionProperty::Width, 180).ease_in_out(),
                        ],
                    }} />
                    <Element style={{
                        width: switch_theme.thumb_width,
                        height: switch_theme.thumb_height,
                        border_radius: switch_theme.thumb_radius,
                        background: if disabled {
                            theme.color.layer.raised.clone()
                        } else {
                            theme.color.layer.surface.clone()
                        },
                    }} />
                </Element>
                <Text
                    font_size={theme.typography.size.sm}
                    style={{ color: if disabled { theme.color.text.disabled.clone() } else { theme.color.text.primary.clone() } }}
                >
                    {label}
                </Text>
            </Element>
        }
    }
}

#[rfgui::ui::component]
impl rfgui::ui::RsxTag for Switch {
    type Props = __SwitchPropsInit;
    type StrictProps = SwitchProps;
    const ACCEPTS_CHILDREN: bool = false;

    fn into_strict(props: Self::Props) -> Self::StrictProps {
        props.into()
    }

    fn create_node(
        props: Self::StrictProps,
        children: Vec<rfgui::ui::RsxNode>,
        _key: Option<rfgui::ui::RsxKey>,
    ) -> rfgui::ui::RsxNode {
        <Self as RsxComponent<SwitchProps>>::render(props, children)
    }
}
