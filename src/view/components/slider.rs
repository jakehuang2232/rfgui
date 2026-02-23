use std::cell::Cell;
use std::rc::Rc;

use crate::style::{BorderRadius, Color, Length, ParsedValue, PropertyId, Style};
use crate::ui::Binding;
use crate::view::base_component::{Element, Text};

pub struct SliderProps {
    pub value: f64,
    pub value_binding: Option<Binding<f64>>,
    pub min: f64,
    pub max: f64,
    pub width: f32,
    pub height: f32,
    pub disabled: bool,
}

impl SliderProps {
    pub fn new() -> Self {
        Self {
            value: 30.0,
            value_binding: None,
            min: 0.0,
            max: 100.0,
            width: 240.0,
            height: 32.0,
            disabled: false,
        }
    }
}

pub fn build_slider(props: SliderProps) -> Element {
    build_slider_with_ids(props, 0, 0, 0, 0, 0)
}

pub fn build_slider_with_ids(
    props: SliderProps,
    root_id: u64,
    rail_id: u64,
    active_id: u64,
    thumb_id: u64,
    value_text_id: u64,
) -> Element {
    let value = props
        .value_binding
        .as_ref()
        .map(|v| v.get())
        .unwrap_or(props.value)
        .clamp(props.min, props.max);
    let ratio = normalize_ratio(value, props.min, props.max);

    let mut root = Element::new_with_id(root_id, 0.0, 0.0, props.width, props.height);
    root.apply_style(slider_root_style(props.width, props.height));

    if !props.disabled {
        let last_sent_value = Rc::new(Cell::new(None::<f64>));
        if let Some(binding) = props.value_binding.clone() {
            let min = props.min;
            let max = props.max;
            let width = props.width.max(1.0);
            let last_sent_value = last_sent_value.clone();
            root.on_mouse_down(move |event, _control| {
                _control.set_pointer_capture(event.meta.current_target_id());
                let next = value_from_local_x(event.mouse.local_x, width, min, max);
                set_if_changed(&binding, &last_sent_value, next);
                event.meta.stop_propagation();
            });
        }
        if let Some(binding) = props.value_binding.clone() {
            let min = props.min;
            let max = props.max;
            let width = props.width.max(1.0);
            let last_sent_value = last_sent_value.clone();
            root.on_mouse_move(move |event, _control| {
                if !event.mouse.buttons.left {
                    return;
                }
                let next = value_from_local_x(event.mouse.local_x, width, min, max);
                set_if_changed(&binding, &last_sent_value, next);
                event.meta.stop_propagation();
            });
        }
        let last_sent_value = last_sent_value.clone();
        root.on_mouse_up(move |event, control| {
            control.release_pointer_capture(event.meta.current_target_id());
            last_sent_value.set(None);
        });
    }

    let track_y = props.height * 0.5 - 2.0;
    let thumb_x = (props.width * ratio as f32).clamp(0.0, props.width);

    let mut rail = Element::new_with_id(rail_id, 0.0, track_y, props.width, 4.0);
    rail.apply_style(slider_rail_style(props.disabled));

    let mut active = Element::new_with_id(active_id, 0.0, track_y, thumb_x, 4.0);
    active.apply_style(slider_active_style(props.disabled));

    let mut thumb = Element::new_with_id(
        thumb_id,
        (thumb_x - 8.0).max(0.0),
        props.height * 0.5 - 8.0,
        16.0,
        16.0,
    );
    thumb.apply_style(slider_thumb_style(props.disabled));

    let mut value_text = Text::from_content_with_id(value_text_id, format!("{value:.0}"));
    value_text.set_position((props.width + 10.0).max(0.0), props.height * 0.5 - 8.0);
    value_text.set_font_size(12.0);
    value_text.set_font("Heiti TC, Noto Sans CJK TC, Roboto");
    value_text.set_color(if props.disabled {
        Color::hex("#9E9E9E")
    } else {
        Color::hex("#374151")
    });

    root.add_child(Box::new(rail));
    root.add_child(Box::new(active));
    root.add_child(Box::new(thumb));
    root.add_child(Box::new(value_text));
    root
}

fn slider_root_style(width: f32, height: f32) -> Style {
    let mut style = Style::new();
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(width)));
    style.insert(PropertyId::Height, ParsedValue::Length(Length::px(height)));
    style
}

fn slider_rail_style(disabled: bool) -> Style {
    let mut style = Style::new();
    style.set_border_radius(BorderRadius::uniform(Length::px(2.0)));
    style.insert_color_like(
        PropertyId::BackgroundColor,
        if disabled {
            Color::hex("#E5E7EB")
        } else {
            Color::hex("#CFD8DC")
        },
    );
    style
}

fn slider_active_style(disabled: bool) -> Style {
    let mut style = Style::new();
    style.set_border_radius(BorderRadius::uniform(Length::px(2.0)));
    style.insert_color_like(
        PropertyId::BackgroundColor,
        if disabled {
            Color::hex("#B0BEC5")
        } else {
            Color::hex("#1976D2")
        },
    );
    style
}

fn slider_thumb_style(disabled: bool) -> Style {
    let mut style = Style::new();
    style.set_border_radius(BorderRadius::uniform(Length::px(8.0)));
    style.insert_color_like(
        PropertyId::BackgroundColor,
        if disabled {
            Color::hex("#B0BEC5")
        } else {
            Color::hex("#1976D2")
        },
    );
    style
}

fn normalize_ratio(value: f64, min: f64, max: f64) -> f64 {
    let span = (max - min).abs();
    if span <= f64::EPSILON {
        return 0.0;
    }
    ((value - min) / (max - min)).clamp(0.0, 1.0)
}

fn value_from_local_x(local_x: f32, width: f32, min: f64, max: f64) -> f64 {
    let ratio = (local_x / width).clamp(0.0, 1.0) as f64;
    min + (max - min) * ratio
}

fn set_if_changed(binding: &Binding<f64>, last_sent_value: &Cell<Option<f64>>, next: f64) {
    const EPS: f64 = 0.0001;
    let current = last_sent_value.get().unwrap_or_else(|| binding.get());
    if (current - next).abs() <= EPS {
        return;
    }
    binding.set(next);
    last_sent_value.set(Some(next));
}
