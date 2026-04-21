use crate::inputs::button::{ButtonColor, ButtonSize, resolve_color_set, size_spec};
use crate::inputs::toggle_button_group::ToggleButtonGroupContext;
use crate::use_theme;
use rfgui::ui::{
    ClickEvent, ClickHandlerProp, RsxComponent, RsxNode, component, props, rsx, use_context,
};
use rfgui::view::Element;
use rfgui::{
    Align, Border, Color, ColorLike, Cursor, JustifyContent, Layout, Length, Transition,
    TransitionProperty, Transitions,
};

pub struct ToggleButton;

#[derive(Clone)]
#[props]
pub struct ToggleButtonProps {
    pub value: Option<String>,
    pub selected: Option<bool>,
    pub size: Option<ButtonSize>,
    pub color: Option<ButtonColor>,
    pub disabled: Option<bool>,
    pub on_click: Option<ClickHandlerProp>,
}

impl RsxComponent<ToggleButtonProps> for ToggleButton {
    fn render(props: ToggleButtonProps, children: Vec<RsxNode>) -> RsxNode {
        rsx! {
            <ToggleButtonView
                value={props.value}
                selected={props.selected}
                size={props.size}
                color={props.color}
                disabled={props.disabled}
                on_click={props.on_click}
            >
                {children}
            </ToggleButtonView>
        }
    }
}

#[rfgui::ui::component]
impl rfgui::ui::RsxTag for ToggleButton {
    type Props = __ToggleButtonPropsInit;
    type StrictProps = ToggleButtonProps;
    const ACCEPTS_CHILDREN: bool = true;

    fn into_strict(props: Self::Props) -> Self::StrictProps {
        props.into()
    }

    fn create_node(
        props: Self::StrictProps,
        children: Vec<rfgui::ui::RsxNode>,
        _key: Option<rfgui::ui::RsxKey>,
    ) -> rfgui::ui::RsxNode {
        <Self as RsxComponent<ToggleButtonProps>>::render(props, children)
    }
}

fn resolve(color: &dyn ColorLike) -> Color {
    let [r, g, b, a] = color.to_rgba_u8();
    Color::rgba(r, g, b, a)
}

#[component]
fn ToggleButtonView(
    value: Option<String>,
    selected: Option<bool>,
    size: Option<ButtonSize>,
    color: Option<ButtonColor>,
    disabled: Option<bool>,
    on_click: Option<ClickHandlerProp>,
    children: Vec<RsxNode>,
) -> RsxNode {
    let theme = use_theme().0;
    let group_ctx = use_context::<ToggleButtonGroupContext>();

    // Group overrides: selected derived from group binding, click wired to
    // group on_change + binding set, size/color/disabled fall back to group.
    let (selected, size, color, disabled, on_click) = match &group_ctx {
        Some(ctx) => {
            let group_selected = match (&value, ctx.value.get()) {
                (Some(v), Some(current)) => *v == current,
                _ => false,
            };
            let effective_disabled = disabled.unwrap_or(false) || ctx.disabled;
            let effective_size = size.or(ctx.size);
            let effective_color = color.or(ctx.color);

            let group_click: Option<ClickHandlerProp> = match &value {
                Some(v) if !effective_disabled => {
                    let binding = ctx.value.clone();
                    let on_change = ctx.on_change.clone();
                    let user_on_click = on_click.clone();
                    let v = v.clone();
                    Some(ClickHandlerProp::new(move |event: &mut ClickEvent| {
                        if let Some(h) = user_on_click.as_ref() {
                            h.call(event);
                        }
                        let next = if binding.get().as_ref() == Some(&v) {
                            None
                        } else {
                            Some(v.clone())
                        };
                        binding.set(next.clone());
                        if let Some(cb) = on_change.as_ref() {
                            cb(event, next);
                        }
                    }))
                }
                _ => None,
            };

            (
                group_selected,
                effective_size.unwrap_or(ButtonSize::Medium),
                effective_color.unwrap_or(ButtonColor::Inherit),
                effective_disabled,
                group_click,
            )
        }
        None => (
            selected.unwrap_or(false),
            size.unwrap_or(ButtonSize::Medium),
            color.unwrap_or(ButtonColor::Inherit),
            disabled.unwrap_or(false),
            on_click,
        ),
    };

    let spec = size_spec(&theme, size);
    let (color_base, _color_on) = resolve_color_set(&theme, color);

    let transparent: Box<dyn ColorLike> = Box::new(Color::transparent());

    let background: Box<dyn ColorLike> = if disabled {
        transparent.clone()
    } else if selected {
        theme.color.state.active.clone()
    } else {
        transparent.clone()
    };
    let hover_background: Box<dyn ColorLike> = if disabled {
        background.clone()
    } else if selected {
        theme.color.state.pressed.clone()
    } else {
        theme.color.state.hover.clone()
    };
    let text_color: Box<dyn ColorLike> = if disabled {
        theme.color.text.disabled.clone()
    } else if selected {
        color_base.clone()
    } else {
        theme.color.text.primary.clone()
    };
    let border_color: Box<dyn ColorLike> = if disabled {
        theme.color.state.disabled.clone()
    } else {
        theme.color.border.clone()
    };

    let in_group = group_ctx.is_some();
    let border = if in_group {
        None
    } else {
        Some(Border::uniform(Length::px(1.0), border_color.as_ref()))
    };
    let border_radius = if in_group {
        None
    } else {
        Some(theme.component.button.toggle_button_radius)
    };

    rsx! {
        <Element
            style={{
                layout: Layout::flow()
                    .row()
                    .no_wrap()
                    .justify_content(JustifyContent::Center)
                    .align(Align::Center),
                gap: spec.icon_gap,
                padding: spec.toggle_button_padding,
                border_radius: border_radius,
                border: border,
                background: resolve(background.as_ref()),
                color: resolve(text_color.as_ref()),
                font_size: spec.font_size,
                transition: Transitions::single(
                    Transition::new(TransitionProperty::BackgroundColor, theme.motion.duration.normal)
                        .ease_in_out(),
                ),
                cursor: if disabled { Cursor::Default } else { Cursor::Pointer },
                hover: {
                    background: resolve(hover_background.as_ref()),
                },
            }}
            on_click={if !disabled { on_click } else { None }}
        >
            {children}
        </Element>
    }
}
