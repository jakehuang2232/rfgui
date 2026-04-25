use crate::use_theme;

use rfgui::ui::{
    Binding, RsxComponent, RsxNode, on_pointer_down, on_pointer_move, on_pointer_up, props, rsx,
    use_state,
};
use rfgui::view::{Element, Text};
use rfgui::{
    Align, Cursor, JustifyContent, Layout, Length, Operator, Position, TextWrap, Transition,
    TransitionProperty, flex,
};

pub struct Slider;

#[derive(Clone)]
#[props]
pub struct SliderProps {
    pub value: Option<f64>,
    pub binding: Option<Binding<f64>>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub option_count: Option<usize>,
    pub disabled: Option<bool>,
    pub label: Option<String>,
}

impl RsxComponent<SliderProps> for Slider {
    fn render(props: SliderProps, _children: Vec<RsxNode>) -> RsxNode {
        const HORIZONTAL_PADDING: f32 = 8.0;

        let value = props.value.unwrap_or(30.0);
        let has_binding = props.binding.is_some();
        let binding = props.binding.unwrap_or_else(|| Binding::new(value));
        let min = props.min.unwrap_or(0.0);
        let max = props.max.unwrap_or(100.0);
        let step_count = resolve_option_count(min, max, props.option_count);
        let disabled = props.disabled.unwrap_or(false);
        let label = props.label;
        let theme = use_theme().0;
        let slider_theme = &theme.component.slider;
        let height = slider_theme.height.max(1.0);
        let grab_padding = slider_theme.grab_padding.max(0.0).min(height * 0.5);
        let thumb_width = slider_theme.grab_width.max(1.0).min(height);

        let fallback_value = use_state(|| value);
        let value_binding = if has_binding {
            binding
        } else {
            fallback_value.binding()
        };
        let dragging = use_state(|| false);
        let dragging_binding = dragging.binding();

        let value = value_binding.get().clamp(min, max);
        let ratio = value_ratio(value, min, max);
        let thumb_left_percent = ratio as f32 * 100.0;
        let is_dragging = dragging_binding.get();

        let grab_background = if disabled {
            slider_theme.grab_disabled_background.clone()
        } else if is_dragging {
            slider_theme.grab_active_background.clone()
        } else {
            slider_theme.grab_background.clone()
        };

        let mouse_down = if disabled {
            None
        } else {
            let binding = value_binding.clone();
            let dragging_binding = dragging_binding.clone();
            Some(on_pointer_down(move |event| {
                let next = value_from_drag_position(
                    event.pointer.local_x,
                    event.meta.current_target().bounds.width,
                    HORIZONTAL_PADDING,
                    min,
                    max,
                    step_count,
                );
                binding.set(next);
                dragging_binding.set(true);
                event.meta.request_pointer_capture();
                event.meta.stop_propagation();
            }))
        };

        let mouse_move = if disabled {
            None
        } else {
            let binding = value_binding.clone();
            let dragging_binding = dragging_binding.clone();
            Some(on_pointer_move(move |event| {
                if !dragging_binding.get() || !event.pointer.buttons.left {
                    return;
                }

                let next = value_from_drag_position(
                    event.pointer.local_x,
                    event.meta.current_target().bounds.width,
                    HORIZONTAL_PADDING,
                    min,
                    max,
                    step_count,
                );
                binding.set(next);
                event.meta.stop_propagation();
            }))
        };

        let mouse_up = if disabled {
            None
        } else {
            let dragging_binding = dragging_binding.clone();
            Some(on_pointer_up(move |_event| {
                dragging_binding.set(false);
            }))
        };

        rsx! {
            <Element style={{
                layout: Layout::flex().row().align(Align::Center),
                width: Length::percent(100.0),
                gap: Length::px(4.0),
            }}>
                <Element style={{
                    border_radius: slider_theme.frame_radius.clone(),
                    border: theme.component.input.border.clone(),
                    flex: flex().grow(3.0).shrink(1.0),
                    min_width: Length::Zero,
                    height: Length::px(height),
                    layout: Layout::flow()
                        .row()
                        .no_wrap()
                        .justify_content(JustifyContent::Center)
                        .align(Align::Center),
                    cursor: if disabled {
                        Cursor::Default
                    } else if is_dragging {
                        Cursor::Grabbing
                    } else {
                        Cursor::Grab
                    },
                    background: if disabled {
                        theme.color.state.disabled.clone()
                    } else if is_dragging {
                        slider_theme.frame_active_background.clone()
                    } else {
                        slider_theme.frame_background.clone()
                    },
                }}
                on_pointer_down={mouse_down}
                on_pointer_move={mouse_move}
                on_pointer_up={mouse_up}
                >
                    <Element style={{
                        position: Position::absolute()
                            .top(Length::px(grab_padding))
                            .bottom(Length::px(grab_padding))
                            .left(Length::calc(
                                Length::percent(thumb_left_percent),
                                Operator::subtract,
                                Length::px(thumb_width * 0.5),
                            )),
                        width: Length::px(thumb_width),
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
                        style={{
                            color: if disabled { theme.color.text.disabled.clone() } else { theme.color.text.primary.clone() }
                        }}
                    >
                        {format!("{value:.0}")}
                    </Text>
                </Element>
                <Element style={{
                    flex: flex().grow(1.0).shrink(1.0).basis(theme.component.input.label_width_basis.clone()),
                    max_width: theme.component.input.label_max_width.clone(),
                    text_wrap: TextWrap::NoWrap,
                }}>{label.unwrap_or_default()}</Element>
            </Element>
        }
    }
}

#[rfgui::ui::component]
impl rfgui::ui::RsxTag for Slider {
    type Props = __SliderPropsInit;
    type StrictProps = SliderProps;
    const ACCEPTS_CHILDREN: bool = false;

    fn into_strict(props: Self::Props) -> Self::StrictProps {
        props.into()
    }

    fn create_node(
        props: Self::StrictProps,
        children: Vec<rfgui::ui::RsxNode>,
        _key: Option<rfgui::ui::RsxKey>,
    ) -> rfgui::ui::RsxNode {
        <Self as RsxComponent<SliderProps>>::render(props, children)
    }
}

fn resolve_option_count(min: f64, max: f64, configured: Option<usize>) -> usize {
    if let Some(count) = configured {
        return count.max(1);
    }

    ((max - min).abs().round() as usize + 1).max(1)
}

fn value_ratio(value: f64, min: f64, max: f64) -> f64 {
    if (max - min).abs() <= f64::EPSILON {
        return 0.0;
    }
    ((value - min) / (max - min)).clamp(0.0, 1.0)
}

fn value_from_drag_position(
    local_x: f32,
    target_width: f32,
    horizontal_padding: f32,
    min: f64,
    max: f64,
    step_count: usize,
) -> f64 {
    let inner_width = (target_width - horizontal_padding * 2.0).max(1.0);
    let inner_x = (local_x - horizontal_padding).clamp(0.0, inner_width);
    let ratio = inner_x as f64 / inner_width as f64;

    if step_count <= 1 || (max - min).abs() <= f64::EPSILON {
        return min;
    }

    let snapped_index = (ratio * (step_count - 1) as f64).round() as usize;
    let snapped_ratio = snapped_index as f64 / (step_count - 1) as f64;
    min + (max - min) * snapped_ratio
}
