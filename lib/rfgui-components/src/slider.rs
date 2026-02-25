use std::cell::Cell;
use std::rc::Rc;

use rfgui::ui::host::{Element, Text};
use rfgui::ui::{
    Binding, RsxComponent, RsxNode, component, on_mouse_down, on_mouse_move, on_mouse_up, props,
    rsx, use_state,
};
use rfgui::{
    BorderRadius, Color, Length, ParsedValue, Position, PropertyId, Style, Transition,
    TransitionProperty, Transitions,
};

pub struct Slider;

#[props]
pub struct SliderProps {
    pub value: Option<f64>,
    pub binding: Option<Binding<f64>>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub disabled: Option<bool>,
}

impl RsxComponent<SliderProps> for Slider {
    fn render(props: SliderProps) -> RsxNode {
        let value = props.value.unwrap_or(30.0);
        let has_binding = props.binding.is_some();
        let binding = props.binding.unwrap_or_else(|| Binding::new(value));

        rsx! {
            <SliderView
                value={value}
                has_binding={has_binding}
                binding={binding}
                min={props.min.unwrap_or(0.0)}
                max={props.max.unwrap_or(100.0)}
                disabled={props.disabled.unwrap_or(false)}
            />
        }
    }
}

#[component]
fn SliderView(
    value: f64,
    has_binding: bool,
    binding: Binding<f64>,
    min: f64,
    max: f64,
    disabled: bool,
) -> RsxNode {
    let width = 240.0_f32;
    let height = 32.0_f32;

    let fallback_value = use_state(|| value);
    let value_binding = if has_binding {
        binding
    } else {
        fallback_value.binding()
    };

    let value = value_binding.get().clamp(min, max);
    let ratio = normalize_ratio(value, min, max);
    let track_y = height * 0.5 - 2.0;
    let thumb_x = (width * ratio as f32).clamp(0.0, width);

    let mut down = None;
    let mut mv = None;
    let mut up = None;
    if !disabled {
        let last_sent_value = Rc::new(Cell::new(None::<f64>));
        let width = width.max(1.0);

        let binding = value_binding.clone();
        let last_sent_value_down = last_sent_value.clone();
        down = Some(on_mouse_down(move |event| {
            let next = value_from_local_x(event.mouse.local_x, width, min, max);
            set_if_changed(&binding, &last_sent_value_down, next);
            event.meta.stop_propagation();
        }));

        let binding = value_binding.clone();
        let last_sent_value_move = last_sent_value.clone();
        mv = Some(on_mouse_move(move |event| {
            if !event.mouse.buttons.left {
                return;
            }
            let next = value_from_local_x(event.mouse.local_x, width, min, max);
            set_if_changed(&binding, &last_sent_value_move, next);
            event.meta.stop_propagation();
        }));

        up = Some(on_mouse_up(move |_event| {
            last_sent_value.set(None);
        }));
    }

    let mut root = rsx! {
        <Element style={slider_root_style(width, height)}>
            <Element style={slider_rail_style(track_y, width, disabled)} />
            <Element style={slider_active_style(track_y, thumb_x, disabled)} />
            <Element style={slider_thumb_style(thumb_x, height, disabled)} />
            <Element style={slider_value_container_style(width, height)}>
                <Text
                    font_size=12
                    line_height=1.0
                    font="Heiti TC, Noto Sans CJK TC, Roboto"
                    style={{ color: if disabled { "#9E9E9E" } else { "#374151" } }}
                >
                    {format!("{value:.0}")}
                </Text>
            </Element>
        </Element>
    };

    if let RsxNode::Element(node) = &mut root {
        if let Some(handler) = down {
            node.props
                .push(("on_mouse_down".to_string(), handler.into()));
        }
        if let Some(handler) = mv {
            node.props
                .push(("on_mouse_move".to_string(), handler.into()));
        }
        if let Some(handler) = up {
            node.props.push(("on_mouse_up".to_string(), handler.into()));
        }
    }

    root
}

fn slider_root_style(width: f32, height: f32) -> Style {
    let mut style = Style::new();
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(width)));
    style.insert(PropertyId::Height, ParsedValue::Length(Length::px(height)));
    style
}

fn slider_rail_style(track_y: f32, width: f32, disabled: bool) -> Style {
    let mut style = Style::new();
    style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .top(Length::px(track_y))
                .left(Length::px(0.0)),
        ),
    );
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(width)));
    style.insert(PropertyId::Height, ParsedValue::Length(Length::px(4.0)));
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

fn slider_active_style(track_y: f32, width: f32, disabled: bool) -> Style {
    let mut style = Style::new();
    style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .top(Length::px(track_y))
                .left(Length::px(0.0)),
        ),
    );
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(width)));
    style.insert(PropertyId::Height, ParsedValue::Length(Length::px(4.0)));
    style.set_border_radius(BorderRadius::uniform(Length::px(2.0)));
    style.insert_color_like(
        PropertyId::BackgroundColor,
        if disabled {
            Color::hex("#B0BEC5")
        } else {
            Color::hex("#1976D2")
        },
    );
    style.insert(
        PropertyId::Transition,
        ParsedValue::Transition(Transitions::single(
            Transition::new(TransitionProperty::Width, 60).ease_out(),
        )),
    );
    style
}

fn slider_thumb_style(thumb_x: f32, height: f32, disabled: bool) -> Style {
    let mut style = Style::new();
    style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .top(Length::px(height * 0.5 - 8.0))
                .left(Length::px((thumb_x - 8.0).max(0.0))),
        ),
    );
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(16.0)));
    style.insert(PropertyId::Height, ParsedValue::Length(Length::px(16.0)));
    style.set_border_radius(BorderRadius::uniform(Length::px(8.0)));
    style.insert_color_like(
        PropertyId::BackgroundColor,
        if disabled {
            Color::hex("#B0BEC5")
        } else {
            Color::hex("#1976D2")
        },
    );
    style.insert(
        PropertyId::Transition,
        ParsedValue::Transition(Transitions::single(
            Transition::new(TransitionProperty::Position, 60).ease_out(),
        )),
    );
    style
}

fn slider_value_container_style(width: f32, height: f32) -> Style {
    let mut style = Style::new();
    style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .top(Length::px(height * 0.5 - 8.0))
                .left(Length::px((width + 10.0).max(0.0))),
        ),
    );
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(28.0)));
    style.insert(PropertyId::Height, ParsedValue::Length(Length::px(16.0)));
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
