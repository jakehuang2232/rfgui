use crate::{Button, use_theme};
use rfgui::ui::{
    Binding, BlurHandlerProp, ClickHandlerProp, RsxChildrenPolicy, RsxComponent, RsxNode,
    TextChangeHandlerProp, props, rsx, use_state,
};
use rfgui::view::{Element, TextArea};
use rfgui::{Align, Layout, Length, Padding, TextWrap, flex};

pub struct NumberField;

pub trait NumberFieldValue: Copy + PartialEq + PartialOrd + 'static {
    fn zero() -> Self;
    fn one() -> Self;
    fn parse_input(raw: &str) -> Option<Self>;
    fn is_intermediate_input(raw: &str) -> bool;
    fn increment(value: Self, step: Self) -> Self;
    fn decrement(value: Self, step: Self) -> Self;
    fn format_value(value: &Self) -> String;
}

#[props]
pub struct NumberFieldProps<T: NumberFieldValue> {
    pub value: Option<T>,
    pub binding: Option<Binding<T>>,
    pub min: Option<T>,
    pub max: Option<T>,
    pub step: Option<T>,
    pub disabled: Option<bool>,
    pub label: Option<String>,
}

impl<T> RsxComponent<NumberFieldProps<T>> for NumberField
where
    T: NumberFieldValue,
{
    fn render(props: NumberFieldProps<T>, _children: Vec<RsxNode>) -> RsxNode {
        let value = props.value.unwrap_or_else(T::zero);
        let has_binding = props.binding.is_some();
        let binding = props.binding.unwrap_or_else(|| Binding::new(value));
        let theme = use_theme().get();
        let label = props.label;

        let fallback_value = use_state(|| value);
        let value_binding = if has_binding {
            binding
        } else {
            fallback_value.binding()
        };
        let min = props.min;
        let max = props.max;
        let step = props.step.unwrap_or_else(T::one);
        let disabled = props.disabled.unwrap_or(false);
        let current = value_binding.get();
        let number_string = use_state(|| T::format_value(&current));

        let minus_click = if disabled {
            None
        } else {
            Some(step_handler(
                value_binding.clone(),
                number_string.binding(),
                step,
                min,
                max,
                true,
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
                false,
            ))
        };

        let text_change = if disabled {
            None
        } else {
            let value_binding = value_binding.clone();
            Some(TextChangeHandlerProp::new(
                move |event: &mut rfgui::ui::TextChangeEvent| {
                    let raw = event.value.trim();
                    if raw.is_empty() || T::is_intermediate_input(raw) {
                        return;
                    }
                    let Some(parsed) = T::parse_input(raw) else {
                        return;
                    };

                    let next = clamp_number(parsed, min, max);
                    if value_binding.get() != next {
                        value_binding.set(next);
                    }
                },
            ))
        };
        let blur = if disabled {
            None
        } else {
            let value_binding = value_binding.clone();
            let number_string = number_string.binding();
            Some(BlurHandlerProp::new(move |_event| {
                let draft = number_string.get();
                let current = value_binding.get();
                let (next, display) = commit_text_input::<T>(&draft, current, min, max);
                if current != next {
                    value_binding.set(next);
                }
                if draft != display {
                    number_string.set(display);
                }
            }))
        };

        rsx! {
            <Element style={{
                layout: Layout::flex().row().align(Align::Center),
                width: Length::percent(100.0),
                gap: Length::px(4.0),
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
                        on_blur={blur}
                        on_focus={|event| event.target.select_all()}
                    />
                </Element>
                <Button
                    label="-"
                    style={
                        padding: Padding::new().x(Length::px(5.0)),
                    }
                    repeat on_click={minus_click}
                    disabled={disabled}/>
                <Button
                    label="+"
                    style={
                        padding: Padding::new().x(Length::px(5.0)),
                    }
                    repeat on_click={plus_click}
                    disabled={disabled}
                />
                <Element style={{
                    flex: flex().grow(1.0).shrink(1.0).basis(theme.component.input.label_width_basis.clone()),
                    max_width: theme.component.input.label_max_width.clone(),
                    text_wrap: TextWrap::NoWrap,
                }}>{label.unwrap_or_default()}</Element>
            </Element>
        }
    }
}

impl RsxChildrenPolicy for NumberField {
    const ACCEPTS_CHILDREN: bool = false;
}

fn step_handler<T: NumberFieldValue>(
    binding: Binding<T>,
    text_binding: Binding<String>,
    step: T,
    min: Option<T>,
    max: Option<T>,
    subtract: bool,
) -> ClickHandlerProp {
    ClickHandlerProp::new(move |_event| {
        let current = binding.get();
        let stepped = if subtract {
            T::decrement(current, step)
        } else {
            T::increment(current, step)
        };
        let next = clamp_number(stepped, min, max);
        binding.set(next);
        text_binding.set(T::format_value(&next));
    })
}

fn clamp_number<T: NumberFieldValue>(value: T, min: Option<T>, max: Option<T>) -> T {
    let mut next = value;
    if let Some(min) = min {
        if next < min {
            next = min;
        }
    }
    if let Some(max) = max {
        if next > max {
            next = max;
        }
    }
    next
}

fn commit_text_input<T: NumberFieldValue>(
    raw: &str,
    current: T,
    min: Option<T>,
    max: Option<T>,
) -> (T, String) {
    let trimmed = raw.trim();
    let next = if trimmed.is_empty() || T::is_intermediate_input(trimmed) {
        current
    } else if let Some(parsed) = T::parse_input(trimmed) {
        clamp_number(parsed, min, max)
    } else {
        current
    };
    (next, T::format_value(&clamp_number(next, min, max)))
}

fn is_incomplete_float(raw: &str) -> bool {
    if matches!(raw, "+" | "-" | "." | "+." | "-.") {
        return true;
    }

    if let Some(prefix) = raw.strip_suffix('e').or_else(|| raw.strip_suffix('E')) {
        return !prefix.is_empty() && prefix.parse::<f64>().is_ok();
    }
    if let Some(prefix) = raw.strip_suffix("e+").or_else(|| raw.strip_suffix("E+")) {
        return !prefix.is_empty() && prefix.parse::<f64>().is_ok();
    }
    if let Some(prefix) = raw.strip_suffix("e-").or_else(|| raw.strip_suffix("E-")) {
        return !prefix.is_empty() && prefix.parse::<f64>().is_ok();
    }

    false
}

macro_rules! impl_integer_number_field_value {
    ($($ty:ty),* $(,)?) => {
        $(
            impl NumberFieldValue for $ty {
                fn zero() -> Self { 0 }
                fn one() -> Self { 1 }
                fn parse_input(raw: &str) -> Option<Self> {
                    raw.parse::<Self>().ok()
                }
                fn is_intermediate_input(raw: &str) -> bool {
                    raw == "+"
                }
                fn increment(value: Self, step: Self) -> Self {
                    value.saturating_add(step)
                }
                fn decrement(value: Self, step: Self) -> Self {
                    value.saturating_sub(step)
                }
                fn format_value(value: &Self) -> String {
                    value.to_string()
                }
            }
        )*
    };
}

macro_rules! impl_signed_number_field_value {
    ($($ty:ty),* $(,)?) => {
        $(
            impl NumberFieldValue for $ty {
                fn zero() -> Self { 0 as $ty }
                fn one() -> Self { 1 as $ty }
                fn parse_input(raw: &str) -> Option<Self> {
                    raw.parse::<Self>().ok()
                }
                fn is_intermediate_input(raw: &str) -> bool {
                    matches!(raw, "+" | "-")
                }
                fn increment(value: Self, step: Self) -> Self {
                    value.saturating_add(step)
                }
                fn decrement(value: Self, step: Self) -> Self {
                    value.saturating_sub(step)
                }
                fn format_value(value: &Self) -> String {
                    value.to_string()
                }
            }
        )*
    };
}

macro_rules! impl_float_number_field_value {
    ($($ty:ty),* $(,)?) => {
        $(
            impl NumberFieldValue for $ty {
                fn zero() -> Self { 0.0 }
                fn one() -> Self { 1.0 }
                fn parse_input(raw: &str) -> Option<Self> {
                    raw.parse::<Self>().ok()
                }
                fn is_intermediate_input(raw: &str) -> bool {
                    is_incomplete_float(raw)
                }
                fn increment(value: Self, step: Self) -> Self {
                    value + step
                }
                fn decrement(value: Self, step: Self) -> Self {
                    value - step
                }
                fn format_value(value: &Self) -> String {
                    let rounded = (*value * 1000.0).round() / 1000.0;
                    if rounded.fract().abs() < 0.0001 {
                        format!("{}", rounded as i64)
                    } else {
                        format!("{rounded:.3}")
                            .trim_end_matches('0')
                            .trim_end_matches('.')
                            .to_string()
                    }
                }
            }

        )*
    };
}

impl_integer_number_field_value!(u8, u16, u32, u64, usize);
impl_signed_number_field_value!(i8, i16, i32, i64, isize);
impl_float_number_field_value!(f32, f64);

#[cfg(test)]
mod tests {
    use super::{NumberFieldValue, clamp_number, commit_text_input};

    #[test]
    fn formats_integer_without_decimal() {
        assert_eq!(i32::format_value(&42), "42");
    }

    #[test]
    fn formats_float_with_trimmed_fraction() {
        assert_eq!(f64::format_value(&1.5), "1.5");
        assert_eq!(f64::format_value(&2.0), "2");
    }

    #[test]
    fn clamps_generic_values() {
        assert_eq!(clamp_number(10_i32, Some(0), Some(5)), 5);
        assert_eq!(clamp_number(1.5_f64, Some(2.0), Some(5.0)), 2.0);
    }

    #[test]
    fn signed_integer_supports_intermediate_minus() {
        assert!(i32::is_intermediate_input("-"));
        assert!(!usize::is_intermediate_input("-"));
    }

    #[test]
    fn float_supports_incomplete_exponent_intermediate() {
        assert!(f64::is_intermediate_input("1e"));
        assert!(f64::is_intermediate_input("1e-"));
        assert!(!f64::is_intermediate_input("1.5"));
    }

    #[test]
    fn blur_commit_restores_current_value_for_intermediate_input() {
        assert_eq!(
            commit_text_input::<i32>("-", 7, Some(0), Some(10)),
            (7, "7".to_string())
        );
    }

    #[test]
    fn blur_commit_clamps_and_formats_value() {
        assert_eq!(
            commit_text_input::<f64>("12.5", 0.0, Some(0.0), Some(10.0)),
            (10.0, "10".to_string())
        );
    }
}
