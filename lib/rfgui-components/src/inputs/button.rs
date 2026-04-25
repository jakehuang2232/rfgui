use crate::{ButtonSizeSpec, Theme, use_theme};
use rfgui::ui::{
    ClickEvent, ClickHandlerProp, EventMeta, NodeId, PointerButton, PointerDownHandlerProp,
    PointerEnterHandlerProp, PointerEventData, PointerLeaveHandlerProp, RsxComponent, RsxNode,
    component, props, rsx, use_interval, use_state,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonSize {
    Small,
    Medium,
    Large,
}

impl From<&str> for ButtonSize {
    fn from(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "small" => ButtonSize::Small,
            "medium" => ButtonSize::Medium,
            "large" => ButtonSize::Large,
            other => panic!("rsx build error on <Button>. unknown Button size `{other}`"),
        }
    }
}

impl From<String> for ButtonSize {
    fn from(value: String) -> Self {
        ButtonSize::from(value.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonColor {
    Primary,
    Secondary,
    Error,
    Warning,
    Info,
    Success,
    Inherit,
}

impl From<&str> for ButtonColor {
    fn from(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "primary" => ButtonColor::Primary,
            "secondary" => ButtonColor::Secondary,
            "error" => ButtonColor::Error,
            "warning" => ButtonColor::Warning,
            "info" => ButtonColor::Info,
            "success" => ButtonColor::Success,
            "inherit" => ButtonColor::Inherit,
            other => panic!("rsx build error on <Button>. unknown Button color `{other}`"),
        }
    }
}

impl From<String> for ButtonColor {
    fn from(value: String) -> Self {
        ButtonColor::from(value.as_str())
    }
}

impl rfgui::ui::IntoOptionalProp<ButtonVariant> for &str {
    fn into_optional_prop(self) -> Option<ButtonVariant> {
        Some(ButtonVariant::from(self))
    }
}

impl rfgui::ui::IntoOptionalProp<ButtonVariant> for String {
    fn into_optional_prop(self) -> Option<ButtonVariant> {
        Some(ButtonVariant::from(self))
    }
}

impl rfgui::ui::IntoOptionalProp<ButtonSize> for &str {
    fn into_optional_prop(self) -> Option<ButtonSize> {
        Some(ButtonSize::from(self))
    }
}

impl rfgui::ui::IntoOptionalProp<ButtonSize> for String {
    fn into_optional_prop(self) -> Option<ButtonSize> {
        Some(ButtonSize::from(self))
    }
}

impl rfgui::ui::IntoOptionalProp<ButtonColor> for &str {
    fn into_optional_prop(self) -> Option<ButtonColor> {
        Some(ButtonColor::from(self))
    }
}

impl rfgui::ui::IntoOptionalProp<ButtonColor> for String {
    fn into_optional_prop(self) -> Option<ButtonColor> {
        Some(ButtonColor::from(self))
    }
}

/// Resolve a `ButtonColor` to a `(base, on)` pair from the current theme.
/// `Inherit` returns the primary text color as base and keeps `on` equal so
/// text-style buttons pick up the ambient foreground.
pub(crate) fn resolve_color_set(
    theme: &Theme,
    color: ButtonColor,
) -> (Box<dyn ColorLike>, Box<dyn ColorLike>) {
    match color {
        ButtonColor::Primary => (
            theme.color.primary.base.clone(),
            theme.color.primary.on.clone(),
        ),
        ButtonColor::Secondary => (
            theme.color.secondary.base.clone(),
            theme.color.secondary.on.clone(),
        ),
        ButtonColor::Error => (theme.color.error.base.clone(), theme.color.error.on.clone()),
        ButtonColor::Warning => (
            theme.color.warning.base.clone(),
            theme.color.warning.on.clone(),
        ),
        ButtonColor::Info => (theme.color.info.base.clone(), theme.color.info.on.clone()),
        ButtonColor::Success => (
            theme.color.success.base.clone(),
            theme.color.success.on.clone(),
        ),
        ButtonColor::Inherit => (
            theme.color.text.primary.clone(),
            theme.color.text.primary.clone(),
        ),
    }
}

pub(crate) fn size_spec(theme: &Theme, size: ButtonSize) -> ButtonSizeSpec {
    match size {
        ButtonSize::Small => theme.component.button.size.small.clone(),
        ButtonSize::Medium => theme.component.button.size.medium.clone(),
        ButtonSize::Large => theme.component.button.size.large.clone(),
    }
}

pub struct Button;

#[derive(Clone)]
#[props]
pub struct ButtonProps {
    pub variant: Option<ButtonVariant>,
    pub size: Option<ButtonSize>,
    pub color: Option<ButtonColor>,
    pub disabled: Option<bool>,
    pub repeat: Option<bool>,
    pub full_width: Option<bool>,
    pub start_icon: Option<RsxNode>,
    pub end_icon: Option<RsxNode>,
    pub on_click: Option<ClickHandlerProp>,
    pub tooltip: Option<RsxNode>,
}

impl RsxComponent<ButtonProps> for Button {
    fn render(props: ButtonProps, children: Vec<RsxNode>) -> RsxNode {
        rsx! {
            <ButtonView
                variant={props.variant}
                size={props.size}
                color={props.color}
                disabled={props.disabled}
                repeat={props.repeat}
                full_width={props.full_width}
                start_icon={props.start_icon}
                end_icon={props.end_icon}
                on_click={props.on_click}
                tooltip={props.tooltip}
            >
                {children}
            </ButtonView>
        }
    }
}

#[rfgui::ui::component]
impl rfgui::ui::RsxTag for Button {
    type Props = __ButtonPropsInit;
    type StrictProps = ButtonProps;
    const ACCEPTS_CHILDREN: bool = true;

    fn into_strict(props: Self::Props) -> Self::StrictProps {
        props.into()
    }

    fn create_node(
        props: Self::StrictProps,
        children: Vec<RsxNode>,
        _key: Option<rfgui::ui::RsxKey>,
    ) -> RsxNode {
        <Self as RsxComponent<ButtonProps>>::render(props, children)
    }
}

#[derive(Clone, Default, PartialEq)]
struct ButtonRepeatState {
    pressed: bool,
    hovered: bool,
    repeating_started: bool,
    remaining_until_fire: Option<Duration>,
    trigger: Option<ButtonRepeatTrigger>,
}

#[derive(Clone, PartialEq)]
struct ButtonRepeatTrigger {
    target_id: NodeId,
    pointer: PointerEventData,
}

fn trigger_click(handler: &ClickHandlerProp, trigger: &ButtonRepeatTrigger) {
    let mut event = ClickEvent {
        meta: EventMeta::new(trigger.target_id),
        pointer: trigger.pointer.clone(),
        click_count: 1,
    };
    handler.call(&mut event);
}

fn resolve_color(color: &dyn ColorLike) -> Color {
    let [r, g, b, a] = color.to_rgba_u8();
    Color::rgba(r, g, b, a)
}

#[component]
fn ButtonView(
    variant: Option<ButtonVariant>,
    size: Option<ButtonSize>,
    color: Option<ButtonColor>,
    disabled: Option<bool>,
    repeat: Option<bool>,
    full_width: Option<bool>,
    start_icon: Option<RsxNode>,
    end_icon: Option<RsxNode>,
    on_click: Option<ClickHandlerProp>,
    tooltip: Option<RsxNode>,
    children: Vec<RsxNode>,
) -> RsxNode {
    const REPEAT_DELAY: Duration = Duration::from_millis(400);
    const REPEAT_INTERVAL: Duration = Duration::from_millis(75);
    const REPEAT_TICK: Duration = Duration::from_millis(25);

    let theme = use_theme().0;
    let variant = variant.unwrap_or(ButtonVariant::Contained);
    let size = size.unwrap_or(ButtonSize::Medium);
    let color = color.unwrap_or(ButtonColor::Primary);
    let disabled = disabled.unwrap_or(false);
    let full_width = full_width.unwrap_or(false);
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

    let spec = size_spec(&theme, size);
    let (color_base, color_on) = resolve_color_set(&theme, color);
    let transparent: Box<dyn ColorLike> = Box::new(Color::transparent());

    let border: Border = if disabled {
        match variant {
            ButtonVariant::Contained | ButtonVariant::Outlined => {
                Border::uniform(Length::px(0.5), theme.color.state.disabled.as_ref())
            }
            ButtonVariant::Text => Border::uniform(Length::px(0.5), transparent.as_ref()),
        }
    } else {
        match variant {
            ButtonVariant::Contained | ButtonVariant::Outlined => {
                Border::uniform(Length::px(0.5), color_base.as_ref())
            }
            ButtonVariant::Text => Border::uniform(Length::px(0.5), transparent.as_ref()),
        }
    };

    let background: Box<dyn ColorLike> = if disabled {
        match variant {
            ButtonVariant::Contained => theme.color.state.disabled.clone(),
            ButtonVariant::Outlined | ButtonVariant::Text => transparent.clone(),
        }
    } else {
        match variant {
            ButtonVariant::Contained => color_base.clone(),
            ButtonVariant::Outlined | ButtonVariant::Text => transparent.clone(),
        }
    };

    let hover_background: Box<dyn ColorLike> = if disabled {
        background.clone()
    } else {
        match variant {
            ButtonVariant::Contained => theme.color.state.active.clone(),
            ButtonVariant::Outlined | ButtonVariant::Text => theme.color.state.hover.clone(),
        }
    };

    let text_color: Box<dyn ColorLike> = if disabled {
        theme.color.text.disabled.clone()
    } else {
        match variant {
            ButtonVariant::Contained => color_on.clone(),
            ButtonVariant::Outlined | ButtonVariant::Text => color_base.clone(),
        }
    };

    let resolved_background = resolve_color(background.as_ref());
    let resolved_hover_background = resolve_color(hover_background.as_ref());
    let resolved_text_color = resolve_color(text_color.as_ref());

    let mouse_down = if repeat_enabled {
        let repeat_state = repeat_state.binding();
        let on_click = on_click.clone();
        Some(PointerDownHandlerProp::new(move |event| {
            if event.pointer.button != Some(PointerButton::Left) {
                return;
            }
            let Some(handler) = on_click.as_ref() else {
                return;
            };
            let trigger = ButtonRepeatTrigger {
                target_id: event.meta.current_target_id(),
                pointer: event.pointer.clone(),
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
            let move_listener = event.viewport.add_pointer_move_listener(move |move_event| {
                if move_event.meta.target_id() == NodeId::default()
                    && move_event.meta.current_target_id() == NodeId::default()
                {
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
            event
                .viewport
                .add_pointer_up_listener_until(move |up_event| {
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

    let tooltip_present = tooltip.is_some();
    let tooltip_hover = use_state(|| false);
    let tooltip_hovered = tooltip_present && tooltip_hover.get();

    let mouse_enter = if repeat_enabled || tooltip_present {
        let repeat_binding = if repeat_enabled {
            Some(repeat_state.binding())
        } else {
            None
        };
        let tooltip_binding = if tooltip_present {
            Some(tooltip_hover.binding())
        } else {
            None
        };
        Some(PointerEnterHandlerProp::new(move |_event| {
            if let Some(rs) = repeat_binding.as_ref() {
                rs.update(|state| {
                    if state.pressed {
                        state.hovered = true;
                    }
                });
            }
            if let Some(tb) = tooltip_binding.as_ref() {
                tb.set(true);
            }
        }))
    } else {
        None
    };

    let mouse_leave = if repeat_enabled || tooltip_present {
        let repeat_binding = if repeat_enabled {
            Some(repeat_state.binding())
        } else {
            None
        };
        let tooltip_binding = if tooltip_present {
            Some(tooltip_hover.binding())
        } else {
            None
        };
        Some(PointerLeaveHandlerProp::new(move |_event| {
            if let Some(rs) = repeat_binding.as_ref() {
                rs.update(|state| {
                    if state.pressed {
                        state.hovered = false;
                    }
                });
            }
            if let Some(tb) = tooltip_binding.as_ref() {
                tb.set(false);
            }
        }))
    } else {
        None
    };

    let root_padding: Padding = spec.padding;
    let root_border_radius: BorderRadius = theme.component.button.radius;
    let icon_gap = spec.icon_gap;

    let width = if full_width {
        Some(Length::percent(100.0))
    } else {
        None
    };

    rsx! {
        <Element
            style={{
                width: width,
                layout: Layout::flow()
                    .row()
                    .no_wrap()
                    .justify_content(JustifyContent::Center)
                    .align(Align::Center),
                gap: icon_gap,
                color: resolved_text_color,
                padding: root_padding,
                border_radius: root_border_radius,
                border: border,
                background: resolved_background,
                transition: Transitions::single(
                    Transition::new(TransitionProperty::BackgroundColor, theme.motion.duration.normal)
                        .ease_in_out(),
                ),
                cursor: if disabled { Cursor::Default } else { Cursor::Pointer },
                hover: {
                    background: resolved_hover_background,
                },
            }}
            on_pointer_down={mouse_down}
            on_pointer_enter={mouse_enter}
            on_pointer_leave={mouse_leave}
            on_click={if !disabled && !repeat_enabled { on_click } else { None }}
        >
            {start_icon}
            <Text font_size={spec.font_size}>
                {children}
            </Text>
            {end_icon}
            {if tooltip_hovered { tooltip } else { None }}
        </Element>
    }
}
