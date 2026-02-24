use rfgui::ui::host::{Element, Text};
use rfgui::ui::{
    Binding, ClickHandlerProp, RsxComponent, RsxNode, component, props, rsx, use_state,
};
use rfgui::{
    AlignItems, Border, BorderRadius, Color, Display, JustifyContent, Length, ParsedValue,
    PropertyId, Style,
};

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

impl RsxComponent for NumberField {
    type Props = NumberFieldProps;

    fn render(props: Self::Props) -> RsxNode {
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
    let width = 140.0_f32;
    let height = 40.0_f32;

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
        <Element style={number_field_style(width, height, disabled)}>
            <Element style={stepper_style(button_size, disabled, true)}>
                <Text
                    font_size=18
                    line_height=1.0
                    font="Heiti TC, Noto Sans CJK TC, Roboto"
                    style={{ color: if disabled { "#BDBDBD" } else { "#374151" } }}
                >
                    {"−"}
                </Text>
            </Element>
            <Element style={value_style(value_width, button_size)}>
                <Text
                    font_size=14
                    line_height=1.0
                    font="Heiti TC, Noto Sans CJK TC, Roboto"
                    style={{ color: if disabled { "#9E9E9E" } else { "#111827" } }}
                >
                    {format_number(current)}
                </Text>
            </Element>
            <Element style={stepper_style(button_size, disabled, false)}>
                <Text
                    font_size=16
                    line_height=1.0
                    font="Heiti TC, Noto Sans CJK TC, Roboto"
                    style={{ color: if disabled { "#BDBDBD" } else { "#374151" } }}
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

fn number_field_style(width: f32, height: f32, disabled: bool) -> Style {
    let mut style = Style::new();
    style.insert(
        PropertyId::Display,
        ParsedValue::Display(Display::flow().row().no_wrap()),
    );
    style.insert(
        PropertyId::AlignItems,
        ParsedValue::AlignItems(AlignItems::Center),
    );
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(width)));
    style.insert(PropertyId::Height, ParsedValue::Length(Length::px(height)));
    style.set_border_radius(BorderRadius::uniform(Length::px(8.0)));
    style.set_border(Border::uniform(Length::px(1.0), &Color::hex("#B0BEC5")));
    style.insert_color_like(
        PropertyId::BackgroundColor,
        if disabled {
            Color::hex("#F5F5F5")
        } else {
            Color::hex("#FFFFFF")
        },
    );
    style
}

fn stepper_style(size: f32, disabled: bool, is_left: bool) -> Style {
    let mut style = Style::new();
    style.insert(
        PropertyId::Display,
        ParsedValue::Display(Display::flow().row().no_wrap()),
    );
    style.insert(
        PropertyId::AlignItems,
        ParsedValue::AlignItems(AlignItems::Center),
    );
    style.insert(
        PropertyId::JustifyContent,
        ParsedValue::JustifyContent(JustifyContent::Center),
    );
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(size)));
    style.insert(PropertyId::Height, ParsedValue::Length(Length::px(size)));
    style.insert_color_like(
        PropertyId::BackgroundColor,
        if disabled {
            Color::hex("#F5F5F5")
        } else {
            Color::hex("#F8FAFC")
        },
    );
    let mut border = Border::uniform(Length::px(1.0), &Color::hex("#CFD8DC"));
    border = if is_left {
        border.right(Some(Length::px(1.0)), Some(&Color::hex("#CFD8DC")))
    } else {
        border.left(Some(Length::px(1.0)), Some(&Color::hex("#CFD8DC")))
    };
    style.set_border(border);
    style
}

fn value_style(width: f32, height: f32) -> Style {
    let mut style = Style::new();
    style.insert(
        PropertyId::Display,
        ParsedValue::Display(Display::flow().row().no_wrap()),
    );
    style.insert(
        PropertyId::AlignItems,
        ParsedValue::AlignItems(AlignItems::Center),
    );
    style.insert(
        PropertyId::JustifyContent,
        ParsedValue::JustifyContent(JustifyContent::Center),
    );
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(width)));
    style.insert(PropertyId::Height, ParsedValue::Length(Length::px(height)));
    style
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
