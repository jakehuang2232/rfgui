use crate::style::{Border, BorderRadius, Color, Length, ParsedValue, PropertyId, Style};
use crate::ui::Binding;
use crate::view::base_component::{Element, Text};

pub struct NumberFieldProps {
    pub value: f64,
    pub value_binding: Option<Binding<f64>>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub step: f64,
    pub width: f32,
    pub height: f32,
    pub disabled: bool,
}

impl NumberFieldProps {
    pub fn new() -> Self {
        Self {
            value: 0.0,
            value_binding: None,
            min: None,
            max: None,
            step: 1.0,
            width: 140.0,
            height: 40.0,
            disabled: false,
        }
    }
}

pub fn build_number_field(props: NumberFieldProps) -> Element {
    build_number_field_with_ids(props, 0, 0, 0, 0)
}

pub fn build_number_field_with_ids(
    props: NumberFieldProps,
    root_id: u64,
    minus_id: u64,
    plus_id: u64,
    value_text_id: u64,
) -> Element {
    let current = props
        .value_binding
        .as_ref()
        .map(|v| v.get())
        .unwrap_or(props.value);

    let mut root = Element::new_with_id(root_id, 0.0, 0.0, props.width, props.height);
    root.apply_style(number_field_style(
        props.width,
        props.height,
        props.disabled,
    ));

    let button_size = props.height - 2.0;
    let value_width = (props.width - button_size * 2.0).max(0.0);

    let mut minus = Element::new_with_id(minus_id, 1.0, 1.0, button_size, button_size);
    minus.apply_style(stepper_style(props.disabled, true));
    if !props.disabled {
        if let Some(binding) = props.value_binding.clone() {
            let step = props.step;
            let min = props.min;
            let max = props.max;
            minus.on_click(move |_event, _control| {
                let mut next = binding.get() - step;
                if let Some(min) = min {
                    next = next.max(min);
                }
                if let Some(max) = max {
                    next = next.min(max);
                }
                binding.set(next);
            });
        }
    }
    let mut minus_text = Text::from_content("−");
    minus_text.set_position(button_size * 0.5 - 4.0, button_size * 0.5 - 10.0);
    minus_text.set_font_size(18.0);
    minus_text.set_font("Heiti TC, Noto Sans CJK TC, Roboto");
    minus_text.set_color(if props.disabled {
        Color::hex("#BDBDBD")
    } else {
        Color::hex("#374151")
    });
    minus.add_child(Box::new(minus_text));

    let mut plus = Element::new_with_id(
        plus_id,
        1.0 + button_size + value_width,
        1.0,
        button_size,
        button_size,
    );
    plus.apply_style(stepper_style(props.disabled, false));
    if !props.disabled {
        if let Some(binding) = props.value_binding.clone() {
            let step = props.step;
            let min = props.min;
            let max = props.max;
            plus.on_click(move |_event, _control| {
                let mut next = binding.get() + step;
                if let Some(min) = min {
                    next = next.max(min);
                }
                if let Some(max) = max {
                    next = next.min(max);
                }
                binding.set(next);
            });
        }
    }
    let mut plus_text = Text::from_content("+");
    plus_text.set_position(button_size * 0.5 - 4.0, button_size * 0.5 - 9.0);
    plus_text.set_font_size(16.0);
    plus_text.set_font("Heiti TC, Noto Sans CJK TC, Roboto");
    plus_text.set_color(if props.disabled {
        Color::hex("#BDBDBD")
    } else {
        Color::hex("#374151")
    });
    plus.add_child(Box::new(plus_text));

    let mut value_text = Text::from_content_with_id(value_text_id, format_number(current));
    value_text.set_position(1.0 + button_size + 10.0, props.height * 0.5 - 8.0);
    value_text.set_font_size(14.0);
    value_text.set_font("Heiti TC, Noto Sans CJK TC, Roboto");
    value_text.set_color(if props.disabled {
        Color::hex("#9E9E9E")
    } else {
        Color::hex("#111827")
    });

    root.add_child(Box::new(minus));
    root.add_child(Box::new(plus));
    root.add_child(Box::new(value_text));
    root
}

fn number_field_style(width: f32, height: f32, disabled: bool) -> Style {
    let mut style = Style::new();
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

fn stepper_style(disabled: bool, is_left: bool) -> Style {
    let mut style = Style::new();
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
