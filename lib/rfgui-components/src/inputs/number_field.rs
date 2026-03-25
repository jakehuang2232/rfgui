use crate::{Button, use_theme};
use rfgui::ui::host::{Element, TextArea};
use rfgui::ui::{
    Binding, ClickHandlerProp, RsxChildrenPolicy, RsxComponent, RsxNode, TextChangeHandlerProp,
    component, props, rsx, use_state,
};
use rfgui::{Align, Layout, Length, TextWrap, flex, Padding};

pub struct NumberField;

#[props]
pub struct NumberFieldProps {
    pub value: Option<f64>,
    pub binding: Option<Binding<f64>>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub step: Option<f64>,
    pub disabled: Option<bool>,
    pub label: Option<String>,
}

impl RsxComponent<NumberFieldProps> for NumberField {
    fn render(props: NumberFieldProps, _children: Vec<RsxNode>) -> RsxNode {
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
                label={props.label}
            />
        }
    }
}

impl RsxChildrenPolicy for NumberField {
    const ACCEPTS_CHILDREN: bool = false;
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
    label: Option<String>,
) -> RsxNode {
    let theme = use_theme().get();
    let _ = label;

    let fallback_value = use_state(|| value);
    let value_binding = if has_binding {
        binding
    } else {
        fallback_value.binding()
    };
    let min = if has_min { Some(min_value) } else { None };
    let max = if has_max { Some(max_value) } else { None };
    let current = value_binding.get();
    let number_string = use_state(|| format_number(current));

    let minus_click = if disabled {
        None
    } else {
        Some(step_handler(
            value_binding.clone(),
            number_string.binding(),
            -step,
            min,
            max,
        ))
    };

    let plus_click = if disabled {
        None
    } else {
        Some(step_handler(
            value_binding.clone(),
            number_string.binding(),
            step,
            min,
            max,
        ))
    };

    let text_change = if disabled {
        None
    } else {
        let value_binding = value_binding.clone();
        let number_string = number_string.binding();
        Some(TextChangeHandlerProp::new(
            move |event: &mut rfgui::ui::TextChangeEvent| {
                let raw = event.value.trim();
                if raw.is_empty() {
                    return;
                }
                let Ok(parsed) = raw.parse::<f64>() else {
                    return;
                };

                let next = clamp_number(parsed, min, max);
                if value_binding.get() != next {
                    value_binding.set(next);
                }

                if next != parsed {
                    number_string.set(format_number(next));
                }
            },
        ))
    };

    let root = rsx! {
        <Element style={{
            layout: Layout::flex().row().align(Align::Center),
            width: Length::percent(100.0),
            gap: Length::px(2.0),
        }}>
            <Element style={{
                border_radius: theme.component.input.radius,
                border: theme.component.input.border.clone(),
                padding: Padding::new().x(Length::px(2.0)),
                flex: flex().grow(3.0).shrink(1.0),
                min_width: Length::Zero,
                background: if disabled {
                    theme.color.state.disabled.clone()
                } else {
                    theme.color.layer.surface.clone()
                },
                selection: {
                    background: theme.color.text.primary_selection_background.clone(),
                }
            }}>
                <TextArea
                    style={{width: Length::percent(100.0)}}
                    multiline={false}
                    read_only={disabled}
                    binding={number_string.binding()}
                    on_change={text_change}
                    on_focus={|event| event.target.select_all()}
                />
            </Element>
            <Button label="-" repeat on_click={minus_click} disabled={disabled} />
            <Button label="+" repeat on_click={plus_click} disabled={disabled} />
            <Element style={{
                flex: flex().grow(1.0).shrink(1.0).basis(theme.component.input.label_width_basis.clone()),
                max_width: theme.component.input.label_max_width.clone(),
                text_wrap: TextWrap::NoWrap,
            }}>{label.unwrap_or_default()}</Element>
        </Element>
    };

    root
}

fn step_handler(
    binding: Binding<f64>,
    text_binding: Binding<String>,
    delta: f64,
    min: Option<f64>,
    max: Option<f64>,
) -> ClickHandlerProp {
    ClickHandlerProp::new(move |_event| {
        let next = clamp_number(binding.get() + delta, min, max);
        binding.set(next);
        text_binding.set(format_number(next));
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

fn clamp_number(value: f64, min: Option<f64>, max: Option<f64>) -> f64 {
    let mut next = value;
    if let Some(min) = min {
        next = next.max(min);
    }
    if let Some(max) = max {
        next = next.min(max);
    }
    next
}
