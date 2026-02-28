use crate::use_theme;
use std::cell::Cell;
use std::rc::Rc;

use rfgui::ui::host::{Element, Text};
use rfgui::ui::{
    Binding, RsxComponent, RsxNode, component, on_mouse_down, on_mouse_move, on_mouse_up, props,
    rsx, use_state,
};
use rfgui::{
    AlignItems, Cursor, Display, JustifyContent, Length, Position, Transition, TransitionProperty,
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
    let theme = use_theme().get();
    let slider_theme = &theme.component.slider;
    let width = slider_theme.width.max(1.0);
    let height = slider_theme.height.max(1.0);
    let grab_width = slider_theme.grab_width.max(1.0).min(width);
    let grab_padding = slider_theme.grab_padding.max(0.0).min(height * 0.5);
    let grab_height = (height - grab_padding * 2.0).max(1.0);

    let fallback_value = use_state(|| value);
    let value_binding = if has_binding {
        binding
    } else {
        fallback_value.binding()
    };
    let dragging = use_state(|| false);
    let dragging_binding = dragging.binding();

    let value = value_binding.get().clamp(min, max);
    let ratio = normalize_ratio(value, min, max);
    let thumb_x = ((width - grab_width) * ratio as f32).clamp(0.0, (width - grab_width).max(0.0));
    let is_dragging = dragging_binding.get();

    let mut down = None;
    let mut mv = None;
    let mut up = None;
    if !disabled {
        let last_sent_value = Rc::new(Cell::new(None::<f64>));
        let width = width.max(1.0);

        let binding = value_binding.clone();
        let last_sent_value_down = last_sent_value.clone();
        let dragging_binding_down = dragging_binding.clone();
        down = Some(on_mouse_down(move |event| {
            let next = value_from_local_x(event.mouse.local_x, width, min, max);
            set_if_changed(&binding, &last_sent_value_down, next);
            dragging_binding_down.set(true);
            event.meta.request_pointer_capture();
            event.meta.stop_propagation();
        }));

        let binding = value_binding.clone();
        let last_sent_value_move = last_sent_value.clone();
        let dragging_binding_move = dragging_binding.clone();
        mv = Some(on_mouse_move(move |event| {
            if !dragging_binding_move.get() || !event.mouse.buttons.left {
                return;
            }
            let next = value_from_local_x(event.mouse.local_x, width, min, max);
            set_if_changed(&binding, &last_sent_value_move, next);
            event.meta.stop_propagation();
        }));

        let dragging_binding_up = dragging_binding.clone();
        up = Some(on_mouse_up(move |_event| {
            last_sent_value.set(None);
            dragging_binding_up.set(false);
        }));
    }

    let grab_background = if disabled {
        slider_theme.grab_disabled_background.clone()
    } else if is_dragging {
        slider_theme.grab_active_background.clone()
    } else {
        slider_theme.grab_background.clone()
    };

    let mut root = rsx! {
        <Element style={{
            width: Length::px(width),
            height: Length::px(height),
            display: Display::flow().row().no_wrap().justify_content(JustifyContent::Center),
            align_items: AlignItems::Center,
            cursor: if disabled {
                Cursor::Default
            } else if is_dragging {
                Cursor::Grabbing
            } else {
                Cursor::Grab
            },
            border_radius: slider_theme.frame_radius.clone(),
            background: if disabled {
                slider_theme.frame_disabled_background.clone()
            } else if is_dragging {
                slider_theme.frame_active_background.clone()
            } else {
                slider_theme.frame_background.clone()
            },
            hover: {
                background: if disabled || is_dragging {
                    if disabled {
                        slider_theme.frame_disabled_background.clone()
                    } else {
                        slider_theme.frame_active_background.clone()
                    }
                } else {
                    slider_theme.frame_hover_background.clone()
                },
            }
        }}>
            <Element style={{
                position: Position::absolute()
                    .top(Length::px(grab_padding))
                    .left(Length::px(thumb_x)),
                width: Length::px(grab_width),
                height: Length::px(grab_height),
                border_radius: slider_theme.grab_radius.clone(),
                background: grab_background,
                transition: [
                    Transition::new(TransitionProperty::Position, theme.motion.duration.fast)
                        .ease_out(),
                    Transition::new(TransitionProperty::BackgroundColor, theme.motion.duration.fast)
                        .ease_in_out(),
                ],
                hover: {
                    background: if disabled {
                        slider_theme.grab_disabled_background.clone()
                    } else if is_dragging {
                        slider_theme.grab_active_background.clone()
                    } else {
                        slider_theme.grab_hover_background.clone()
                    },
                }
            }} />
            <Text
                font_size={theme.typography.size.xs}
                line_height=1.0
                font={theme.typography.font_family.clone()}
                style={{ color: if disabled { theme.color.text.disabled.clone() } else { theme.color.text.primary.clone() } }}
            >
                {format!("{value:.0}")}
            </Text>
        </Element>
    };

    if let RsxNode::Element(node) = &mut root {
        if let Some(handler) = down {
            node.props.push(("on_mouse_down".to_string(), handler.into()));
        }
        if let Some(handler) = mv {
            node.props.push(("on_mouse_move".to_string(), handler.into()));
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
