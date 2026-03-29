use crate::use_theme;
use rfgui::TextAlign::Center;
use rfgui::ui::{
    ClickEvent, ClickHandlerProp, EventMeta, MouseButton, MouseDownHandlerProp,
    MouseEnterHandlerProp, MouseEventData, MouseLeaveHandlerProp, RsxChildrenPolicy, RsxComponent,
    RsxNode, component, props, rsx, use_interval, use_state,
};
use rfgui::view::{Element, Text};
use rfgui::{
    Align, Border, BorderRadius, Color, ColorLike, Cursor, JustifyContent, Layout, Length, Padding,
    Transition, TransitionProperty, Transitions,
};
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonVariant {
    Contained,
    Outlined,
    Text,
}

impl From<&str> for ButtonVariant {
    fn from(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "contained" => ButtonVariant::Contained,
            "outlined" => ButtonVariant::Outlined,
            "text" => ButtonVariant::Text,
            other => panic!("rsx build error on <Button>. unknown Button variant `{other}`"),
        }
    }
}

impl From<String> for ButtonVariant {
    fn from(value: String) -> Self {
        ButtonVariant::from(value.as_str())
    }
}

pub struct Button;

#[props]
pub struct ButtonProps {
    pub label: String,
    pub variant: Option<ButtonVariant>,
    pub disabled: Option<bool>,
    pub repeat: Option<bool>,
    pub style: Option<ButtonStyleSlot>,
    pub on_click: Option<ClickHandlerProp>,
}

#[props]
#[derive(Clone)]
pub struct ButtonStyleSlot {
    pub color: Option<Color>,
    pub background: Option<Color>,
    pub padding: Option<Padding>,
    pub border: Option<Border>,
    pub border_radius: Option<BorderRadius>,
    pub hover: Option<ButtonHoverStyleSlot>,
}

#[props]
#[derive(Clone)]
pub struct ButtonHoverStyleSlot {
    pub color: Option<Color>,
    pub background: Option<Color>,
    pub border: Option<Border>,
}

impl RsxComponent<ButtonProps> for Button {
    fn render(props: ButtonProps, _children: Vec<RsxNode>) -> RsxNode {
        rsx! {
            <ButtonView
                label={props.label}
                variant={props.variant}
                disabled={props.disabled}
                repeat={props.repeat}
                style={props.style}
                on_click={props.on_click}
            />
        }
    }
}

impl RsxChildrenPolicy for Button {
    const ACCEPTS_CHILDREN: bool = false;
}

#[derive(Clone, Default)]
struct ButtonRepeatState {
    pressed: bool,
    hovered: bool,
    repeating_started: bool,
    remaining_until_fire: Option<Duration>,
    trigger: Option<ButtonRepeatTrigger>,
}

#[derive(Clone)]
struct ButtonRepeatTrigger {
    target_id: u64,
    mouse: MouseEventData,
}

fn trigger_click(handler: &ClickHandlerProp, trigger: &ButtonRepeatTrigger) {
    let mut event = ClickEvent {
        meta: EventMeta::new(trigger.target_id),
        mouse: trigger.mouse.clone(),
    };
    handler.call(&mut event);
}

fn resolve_color(color: &dyn ColorLike) -> Color {
    let [r, g, b, a] = color.to_rgba_u8();
    Color::rgba(r, g, b, a)
}

#[component]
fn ButtonView(
    label: String,
    variant: Option<ButtonVariant>,
    disabled: Option<bool>,
    repeat: Option<bool>,
    style: Option<ButtonStyleSlot>,
    on_click: Option<ClickHandlerProp>,
) -> RsxNode {
    const REPEAT_DELAY: Duration = Duration::from_millis(400);
    const REPEAT_INTERVAL: Duration = Duration::from_millis(75);
    const REPEAT_TICK: Duration = Duration::from_millis(25);

    let theme = use_theme().get();
    let variant = variant.unwrap_or(ButtonVariant::Contained);
    let disabled = disabled.unwrap_or(false);
    let repeat_enabled = repeat.unwrap_or(false) && !disabled && on_click.is_some();
    let repeat_state = use_state(ButtonRepeatState::default);
    let repeat_snapshot = repeat_state.get();

    if repeat_enabled {
        let interval_state = repeat_state.clone();
        let interval_click = on_click.clone();
        use_interval(repeat_snapshot.pressed, REPEAT_TICK, move || {
            let Some(handler) = interval_click.as_ref() else {
                return;
            };
            let snapshot = interval_state.get();
            if !snapshot.pressed || !snapshot.hovered {
                return;
            }
            let Some(remaining_until_fire) = snapshot.remaining_until_fire else {
                return;
            };
            if remaining_until_fire > REPEAT_TICK {
                interval_state.update(|state| {
                    state.remaining_until_fire =
                        Some(remaining_until_fire.saturating_sub(REPEAT_TICK));
                });
                return;
            }
            let Some(trigger) = snapshot.trigger else {
                return;
            };
            interval_state.update(|state| {
                state.repeating_started = true;
                state.remaining_until_fire = Some(REPEAT_INTERVAL);
            });
            trigger_click(handler, &trigger);
        });
    } else {
        use_interval(false, REPEAT_TICK, || {});
    }

    let transparent = Box::new(Color::transparent()) as Box<dyn ColorLike>;
    let border = if disabled {
        match variant {
            ButtonVariant::Contained => {
                Border::uniform(Length::px(0.5), theme.color.state.disabled.as_ref())
            }
            ButtonVariant::Outlined => {
                Border::uniform(Length::px(0.5), theme.color.state.disabled.as_ref())
            }
            ButtonVariant::Text => Border::uniform(Length::px(0.5), transparent.as_ref()),
        }
    } else {
        match variant {
            ButtonVariant::Contained => {
                Border::uniform(Length::px(0.5), theme.color.primary.base.as_ref())
            }
            ButtonVariant::Outlined => {
                Border::uniform(Length::px(0.5), theme.color.primary.base.as_ref())
            }
            ButtonVariant::Text => Border::uniform(Length::px(0.5), transparent.as_ref()),
        }
    };
    let background: Box<dyn ColorLike> = if disabled {
        match variant {
            ButtonVariant::Contained => theme.color.state.disabled.clone(),
            ButtonVariant::Outlined | ButtonVariant::Text => {
                Box::new(Color::transparent()) as Box<dyn ColorLike>
            }
        }
    } else {
        match variant {
            ButtonVariant::Contained => theme.color.primary.base.clone(),
            ButtonVariant::Outlined => Box::new(Color::transparent()) as Box<dyn ColorLike>,
            ButtonVariant::Text => Box::new(Color::transparent()) as Box<dyn ColorLike>,
        }
    };
    let hover_background: Box<dyn ColorLike> = if disabled {
        background.clone()
    } else {
        match variant {
            ButtonVariant::Contained => theme.color.state.active.clone(),
            ButtonVariant::Outlined => theme.color.state.hover.clone(),
            ButtonVariant::Text => theme.color.state.hover.clone(),
        }
    };
    let text_color = if disabled {
        theme.color.text.disabled.clone()
    } else {
        match variant {
            ButtonVariant::Contained => theme.color.primary.on.clone(),
            ButtonVariant::Outlined => theme.color.text.primary.clone(),
            ButtonVariant::Text => theme.color.text.primary.clone(),
        }
    };
    let style_slot = style.as_ref();
    let root_background = style_slot
        .and_then(|slot| slot.background)
        .unwrap_or_else(|| resolve_color(background.as_ref()));
    let root_padding = style_slot
        .and_then(|slot| slot.padding)
        .unwrap_or(theme.component.button.padding);
    let root_border = style_slot
        .and_then(|slot| slot.border.clone())
        .unwrap_or(border);
    let root_border_radius = style_slot
        .and_then(|slot| slot.border_radius)
        .unwrap_or(theme.component.button.radius);
    let root_hover_background = if disabled {
        root_background
    } else {
        style_slot
            .and_then(|slot| slot.hover.as_ref())
            .and_then(|hover| hover.background)
            .unwrap_or_else(|| resolve_color(hover_background.as_ref()))
    };
    let root_hover_border = if disabled {
        root_border.clone()
    } else {
        style_slot
            .and_then(|slot| slot.hover.as_ref())
            .and_then(|hover| hover.border.clone())
            .unwrap_or_else(|| root_border.clone())
    };
    let resolved_text_color = style_slot
        .and_then(|slot| slot.color)
        .unwrap_or_else(|| resolve_color(text_color.as_ref()));
    let text_hover_color = if disabled {
        resolved_text_color
    } else {
        style_slot
            .and_then(|slot| slot.hover.as_ref())
            .and_then(|hover| hover.color)
            .unwrap_or(resolved_text_color)
    };

    let mouse_down = if repeat_enabled {
        let repeat_state = repeat_state.binding();
        let on_click = on_click.clone();
        Some(MouseDownHandlerProp::new(move |event| {
            if event.mouse.button != Some(MouseButton::Left) {
                return;
            }
            let Some(handler) = on_click.as_ref() else {
                return;
            };
            let trigger = ButtonRepeatTrigger {
                target_id: event.meta.current_target_id(),
                mouse: event.mouse.clone(),
            };
            repeat_state.set(ButtonRepeatState {
                pressed: true,
                hovered: true,
                repeating_started: false,
                remaining_until_fire: Some(REPEAT_DELAY),
                trigger: Some(trigger.clone()),
            });
            trigger_click(handler, &trigger);

            let button_target_id = trigger.target_id;
            let repeat_state_for_move = repeat_state.clone();
            let move_listener = event.viewport.add_mouse_move_listener(move |move_event| {
                if move_event.meta.target_id() == 0 && move_event.meta.current_target_id() == 0 {
                    repeat_state_for_move.update(|state| {
                        if state.pressed {
                            state.hovered = false;
                        }
                    });
                } else if move_event.meta.current_target_id() == button_target_id {
                    repeat_state_for_move.update(|state| {
                        if state.pressed {
                            state.hovered = true;
                        }
                    });
                }
            });

            let repeat_state_for_up = repeat_state.clone();
            event.viewport.add_mouse_up_listener_until(move |up_event| {
                up_event.viewport.remove_listener(move_listener);
                repeat_state_for_up.update(|state| {
                    state.pressed = false;
                    state.hovered = false;
                    state.repeating_started = false;
                    state.remaining_until_fire = None;
                    state.trigger = None;
                });
                true
            });
        }))
    } else {
        None
    };

    let mouse_enter = if repeat_enabled {
        let repeat_state = repeat_state.binding();
        Some(MouseEnterHandlerProp::new(move |_event| {
            repeat_state.update(|state| {
                if state.pressed {
                    state.hovered = true;
                }
            });
        }))
    } else {
        None
    };

    let mouse_leave = if repeat_enabled {
        let repeat_state = repeat_state.binding();
        Some(MouseLeaveHandlerProp::new(move |_event| {
            repeat_state.update(|state| {
                if state.pressed {
                    state.hovered = false;
                }
            });
        }))
    } else {
        None
    };

    rsx! {
        <Element
            style={{
                layout: Layout::flow()
                    .row()
                    .no_wrap()
                    .justify_content(JustifyContent::Center)
                    .align(Align::Center),
                padding: root_padding,
                border_radius: root_border_radius,
                border: root_border,
                background: root_background,
                transition: Transitions::single(
                    Transition::new(TransitionProperty::BackgroundColor, theme.motion.duration.normal)
                        .ease_in_out(),
                ),
                cursor: if disabled { Cursor::Default } else { Cursor::Pointer },
                hover: {
                    background: root_hover_background,
                    border: root_hover_border,
                },
            }}
            on_mouse_down={mouse_down}
            on_mouse_enter={mouse_enter}
            on_mouse_leave={mouse_leave}
            on_click={if !disabled && !repeat_enabled { on_click } else { None }}
        >
            <Text
                font_size={theme.typography.size.sm}
                align={Center}
                style={{
                    color: resolved_text_color,
                    hover: {
                        color: text_hover_color,
                    }
                }}
            >
                {label}
            </Text>
        </Element>
    }
}
