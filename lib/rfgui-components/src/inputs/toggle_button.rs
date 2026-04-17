use crate::inputs::button::{ButtonColor, ButtonSize, resolve_color_set, size_spec};
use crate::use_theme;
use rfgui::ui::{
    ClickHandlerProp, RsxComponent, RsxNode, component, props, rsx,
};
use rfgui::view::Element;
use rfgui::{
    Align, Border, Color, ColorLike, Cursor, JustifyContent, Layout, Length, Transition,
    TransitionProperty, Transitions,
};

pub struct ToggleButton;

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
    let _value = value;
    let theme = use_theme().0;
    let selected = selected.unwrap_or(false);
    let size = size.unwrap_or(ButtonSize::Medium);
    let color = color.unwrap_or(ButtonColor::Inherit);
    let disabled = disabled.unwrap_or(false);

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
                border_radius: theme.component.button.toggle_button_radius,
                border: Border::uniform(Length::px(1.0), border_color.as_ref()),
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
