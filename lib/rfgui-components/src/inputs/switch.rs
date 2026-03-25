use crate::use_theme;
use rfgui::ui::host::{Element, Text};
use rfgui::ui::{
    Binding, RsxChildrenPolicy, RsxComponent, RsxNode, component, on_click, props, rsx, use_state,
};
use rfgui::{Align, Layout, Length, Operator, Transition, TransitionProperty};

pub struct Switch;

#[props]
pub struct SwitchProps {
    pub label: String,
    pub binding: Option<Binding<bool>>,
    pub checked: Option<bool>,
    pub disabled: Option<bool>,
}

impl RsxComponent<SwitchProps> for Switch {
    fn render(props: SwitchProps, _children: Vec<RsxNode>) -> RsxNode {
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

impl RsxChildrenPolicy for Switch {
    const ACCEPTS_CHILDREN: bool = false;
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

    let click = on_click(move |_event| {
        if disabled {
            return;
        }
        checked_binding.set(!checked_binding.get());
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
