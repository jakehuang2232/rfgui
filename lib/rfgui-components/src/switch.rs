use crate::use_theme;
use rfgui::ui::host::{Element, Text};
use rfgui::ui::{Binding, RsxComponent, RsxNode, component, on_click, props, rsx, use_state};
use rfgui::{
    AlignItems, BorderRadius, Display, Length, Padding, Transition, TransitionProperty,
};

pub struct Switch;

#[props]
pub struct SwitchProps {
    pub label: String,
    pub binding: Option<Binding<bool>>,
    pub checked: Option<bool>,
    pub disabled: Option<bool>,
}

impl RsxComponent<SwitchProps> for Switch {
    fn render(props: SwitchProps) -> RsxNode {
        let checked = props.checked.unwrap_or(false);
        let has_binding = props.binding.is_some();
        let binding = props.binding.unwrap_or_else(|| Binding::new(checked));

        rsx! {
            <SwitchView
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
fn SwitchView(
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

    let click = on_click(move |_event| {
        if disabled {
            return;
        }
        checked_binding.set(!checked_binding.get());
    });

    rsx! {
        <Element style={{
            display: Display::flow().row().no_wrap(),
            align_items: AlignItems::Center,
            gap: theme.spacing.md,
        }} on_click={click}>
            <Element style={{
                display: Display::flow().row().no_wrap(),
                align_items: AlignItems::Center,
                width: Length::px(44.0),
                height: Length::px(18.0),
                padding: Padding::uniform(Length::px(2.0)),
                border_radius: BorderRadius::uniform(Length::px(8.0)),
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
                    width: Length::px(if checked { 20.0 } else { 0.0 }),
                    height: Length::px(14.0),
                    transition: [
                        Transition::new(TransitionProperty::Width, 180).ease_in_out(),
                    ],
                }} />
                <Element style={{
                    width: Length::px(20.0),
                    height: Length::px(14.0),
                    border_radius: BorderRadius::uniform(Length::px(10.0)),
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
