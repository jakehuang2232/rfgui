use crate::use_theme;
use rfgui::ui::{Binding, RsxComponent, RsxNode, on_click, props, rsx, use_state};
use rfgui::view::{Element, Text};
use rfgui::{Align, Layout, Length, Operator, Transition, TransitionProperty};
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

        rsx! {
            <Element style={{
                layout: Layout::flow().row().align(Align::Center).no_wrap(),
                gap: theme.spacing.md,
            }} on_click={click}>
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
                    background: if disabled {
                        theme.color.state.disabled.clone()
                    } else if checked {
                        theme.color.primary.base.clone()
                    } else {
                        theme.color.border.clone()
                    },
                }}>
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
