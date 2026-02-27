use crate::use_theme;
use rfgui::ui::host::{Element, Text};
use rfgui::ui::{
    Binding, ClickHandlerProp, RsxComponent, RsxNode, component, props, rsx, use_state,
};
use rfgui::{AlignItems, Border, Display, JustifyContent, Length};

pub struct NumberField;

#[props]
pub struct NumberFieldProps {
    pub value: Option<f64>,
    pub binding: Option<Binding<f64>>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub step: Option<f64>,
    pub disabled: Option<bool>,
}

impl RsxComponent<NumberFieldProps> for NumberField {
    fn render(props: NumberFieldProps) -> RsxNode {
        let value = props.value.unwrap_or(0.0);
        let has_binding = props.binding.is_some();
        let binding = props.binding.unwrap_or_else(|| Binding::new(value));

        rsx! {
            <NumberFieldView
                value={value}
                has_binding={has_binding}
                binding={binding}
                has_min={props.min.is_some()}
                min_value={props.min.unwrap_or(0.0)}
                has_max={props.max.is_some()}
                max_value={props.max.unwrap_or(0.0)}
                step={props.step.unwrap_or(1.0)}
                disabled={props.disabled.unwrap_or(false)}
            />
        }
    }
}

#[component]
fn NumberFieldView(
    value: f64,
    has_binding: bool,
    binding: Binding<f64>,
    has_min: bool,
    min_value: f64,
    has_max: bool,
    max_value: f64,
    step: f64,
    disabled: bool,
) -> RsxNode {
    let theme = use_theme().get();
    let width = 120.0_f32;
    let height = 18.0_f32;

    let fallback_value = use_state(|| value);
    let value_binding = if has_binding {
        binding
    } else {
        fallback_value.binding()
    };
    let min = if has_min { Some(min_value) } else { None };
    let max = if has_max { Some(max_value) } else { None };
    let current = value_binding.get();

    let button_size = (height - 2.0).max(0.0);
    let value_width = (width - button_size * 2.0).max(0.0);
    let left_border =
        Border::uniform(Length::px(1.0), theme.color.border.as_ref())
            .right(Some(Length::px(1.0)), Some(theme.color.border.as_ref()));
    let right_border =
        Border::uniform(Length::px(1.0), theme.color.border.as_ref())
            .left(Some(Length::px(1.0)), Some(theme.color.border.as_ref()));

    let minus_click = if disabled {
        None
    } else {
        Some(step_handler(value_binding.clone(), -step, min, max))
    };

    let plus_click = if disabled {
        None
    } else {
        Some(step_handler(value_binding.clone(), step, min, max))
    };

    let mut root = rsx! {
        <Element style={{
            display: Display::flow().row().no_wrap(),
            align_items: AlignItems::Center,
            width: Length::px(width),
            height: Length::px(height),
            border_radius: theme.component.input.radius,
            border: theme.component.input.border.clone(),
            background: if disabled {
                theme.color.state.disabled.clone()
            } else {
                theme.color.layer.surface.clone()
            },
        }}>
            <Element style={{
                display: Display::flow()
                    .row()
                    .no_wrap()
                    .justify_content(JustifyContent::Center),
                align_items: AlignItems::Center,
                width: Length::px(button_size),
                height: Length::px(button_size),
                background: if disabled {
                    theme.color.state.disabled.clone()
                } else {
                    theme.color.layer.raised.clone()
                },
                border: left_border,
            }}>
                <Text
                    font_size={theme.typography.size.lg}
                    line_height=1.0
                    font={theme.typography.font_family.clone()}
                    style={{ color: if disabled { theme.color.text.disabled.clone() } else { theme.color.text.secondary.clone() } }}
                >
                    {"−"}
                </Text>
            </Element>
            <Element style={{
                display: Display::flow()
                    .row()
                    .no_wrap()
                    .justify_content(JustifyContent::Center),
                align_items: AlignItems::Center,
                width: Length::px(value_width),
                height: Length::px(button_size),
            }}>
                <Text
                    font_size={theme.typography.size.sm}
                    line_height=1.0
                    font={theme.typography.font_family.clone()}
                    style={{ color: if disabled { theme.color.text.disabled.clone() } else { theme.color.text.primary.clone() } }}
                >
                    {format_number(current)}
                </Text>
            </Element>
            <Element style={{
                display: Display::flow()
                    .row()
                    .no_wrap()
                    .justify_content(JustifyContent::Center),
                align_items: AlignItems::Center,
                width: Length::px(button_size),
                height: Length::px(button_size),
                background: if disabled {
                    theme.color.state.disabled.clone()
                } else {
                    theme.color.layer.raised.clone()
                },
                border: right_border,
            }}>
                <Text
                    font_size={theme.typography.size.md}
                    line_height=1.0
                    font={theme.typography.font_family.clone()}
                    style={{ color: if disabled { theme.color.text.disabled.clone() } else { theme.color.text.secondary.clone() } }}
                >
                    {"+"}
                </Text>
            </Element>
        </Element>
    };

    if let RsxNode::Element(root_node) = &mut root {
        if let Some(handler) = minus_click
            && let Some(RsxNode::Element(minus_node)) = root_node.children.get_mut(0)
        {
            minus_node
                .props
                .push(("on_click".to_string(), handler.into()));
        }
        if let Some(handler) = plus_click
            && let Some(RsxNode::Element(plus_node)) = root_node.children.get_mut(2)
        {
            plus_node
                .props
                .push(("on_click".to_string(), handler.into()));
        }
    }

    root
}

fn step_handler(
    binding: Binding<f64>,
    delta: f64,
    min: Option<f64>,
    max: Option<f64>,
) -> ClickHandlerProp {
    ClickHandlerProp::new(move |_event| {
        let mut next = binding.get() + delta;
        if let Some(min) = min {
            next = next.max(min);
        }
        if let Some(max) = max {
            next = next.min(max);
        }
        binding.set(next);
    })
}

fn format_number(value: f64) -> String {
    let rounded = (value * 1000.0).round() / 1000.0;
    if (rounded.fract()).abs() < 0.0001 {
        format!("{}", rounded as i64)
    } else {
        format!("{rounded:.3}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    }
}
