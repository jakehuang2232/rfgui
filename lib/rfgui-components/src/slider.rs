use crate::use_theme;
use std::cell::Cell;
use std::rc::Rc;

use rfgui::ui::host::{Element, Text};
use rfgui::ui::{
    Binding, RsxComponent, RsxNode, component, on_mouse_down, on_mouse_move, on_mouse_up, props,
    rsx, use_state,
};
use rfgui::{BorderRadius, Length, Position, Transition, TransitionProperty};

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
    let theme = use_theme().get();
    let width = 240.0_f32;
    let height = 18.0_f32;

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
        <Element style={{
            width: Length::px(width),
            height: Length::px(height),
        }}>
            <Element style={{
                position: Position::absolute()
                    .top(Length::px(0.0))
                    .left(Length::px(0.0)),
                width: Length::px(width),
                height: Length::px(height),
                border_radius: BorderRadius::uniform(Length::px(2.0)),
                background: if disabled {
                    theme.color.state.disabled.clone()
                } else {
                    theme.color.divider.clone()
                },
            }} />
            <Element style={{
                position: Position::absolute()
                    .top(Length::px(0.0))
                    .left(Length::px(0.0)),
                width: Length::px(thumb_x),
                height: Length::px(height),
                border_radius: BorderRadius::uniform(Length::px(2.0)),
                background: if disabled {
                    theme.color.border.clone()
                } else {
                    theme.color.primary.base.clone()
                },
                transition: [
                    Transition::new(TransitionProperty::Width, theme.motion.duration.fast)
                        .ease_out(),
                ],
            }} />
            <Element style={{
                position: Position::absolute()
                    .top(Length::px(height * 0.5 - 8.0))
                    .left(Length::px((thumb_x - 8.0).max(0.0))),
                width: Length::px(16.0),
                height: Length::px(16.0),
                border_radius: BorderRadius::uniform(Length::px(8.0)),
                background: if disabled {
                    theme.color.border.clone()
                } else {
                    theme.color.primary.base.clone()
                },
                transition: [
                    Transition::new(TransitionProperty::Position, theme.motion.duration.fast)
                        .ease_out(),
                ],
            }} />
            <Element style={{
                position: Position::absolute()
                    .top(Length::px(height * 0.5 - 8.0))
                    .left(Length::px((width + 10.0).max(0.0))),
                width: Length::px(28.0),
                height: Length::px(16.0),
            }}>
                <Text
                    font_size={theme.typography.size.xs}
                    line_height=1.0
                    font={theme.typography.font_family.clone()}
                    style={{ color: if disabled { theme.color.text.disabled.clone() } else { theme.color.text.secondary.clone() } }}
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
