use crate::inputs::button::{ButtonColor, ButtonSize, resolve_color_set, size_spec};
use crate::use_theme;
use rfgui::ui::{ClickHandlerProp, RsxComponent, RsxNode, component, props, rsx};
use rfgui::view::Element;
use rfgui::{
    Align, Color, ColorLike, Cursor, JustifyContent, Layout, Transition, TransitionProperty,
    Transitions,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IconButtonEdge {
    Start,
    End,
    None,
}

impl From<&str> for IconButtonEdge {
    fn from(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "start" => IconButtonEdge::Start,
            "end" => IconButtonEdge::End,
            "none" | "" => IconButtonEdge::None,
            other => panic!("rsx build error on <IconButton>. unknown edge `{other}`"),
        }
    }
}

impl From<String> for IconButtonEdge {
    fn from(value: String) -> Self {
        IconButtonEdge::from(value.as_str())
    }
}

impl rfgui::ui::IntoOptionalProp<IconButtonEdge> for &str {
    fn into_optional_prop(self) -> Option<IconButtonEdge> {
        Some(IconButtonEdge::from(self))
    }
}

impl rfgui::ui::IntoOptionalProp<IconButtonEdge> for String {
    fn into_optional_prop(self) -> Option<IconButtonEdge> {
        Some(IconButtonEdge::from(self))
    }
}

pub struct IconButton;

#[derive(Clone)]
#[props]
pub struct IconButtonProps {
    pub size: Option<ButtonSize>,
    pub color: Option<ButtonColor>,
    pub disabled: Option<bool>,
    pub edge: Option<IconButtonEdge>,
    pub on_click: Option<ClickHandlerProp>,
}

impl RsxComponent<IconButtonProps> for IconButton {
    fn render(props: IconButtonProps, children: Vec<RsxNode>) -> RsxNode {
        rsx! {
            <IconButtonView
                size={props.size}
                color={props.color}
                disabled={props.disabled}
                edge={props.edge}
                on_click={props.on_click}
            >
                {children}
            </IconButtonView>
        }
    }
}

#[rfgui::ui::component]
impl rfgui::ui::RsxTag for IconButton {
    type Props = __IconButtonPropsInit;
    type StrictProps = IconButtonProps;
    const ACCEPTS_CHILDREN: bool = true;

    fn into_strict(props: Self::Props) -> Self::StrictProps {
        props.into()
    }

    fn create_node(
        props: Self::StrictProps,
        children: Vec<rfgui::ui::RsxNode>,
        _key: Option<rfgui::ui::RsxKey>,
    ) -> rfgui::ui::RsxNode {
        <Self as RsxComponent<IconButtonProps>>::render(props, children)
    }
}

fn resolve(color: &dyn ColorLike) -> Color {
    let [r, g, b, a] = color.to_rgba_u8();
    Color::rgba(r, g, b, a)
}

#[component]
fn IconButtonView(
    size: Option<ButtonSize>,
    color: Option<ButtonColor>,
    disabled: Option<bool>,
    edge: Option<IconButtonEdge>,
    on_click: Option<ClickHandlerProp>,
    children: Vec<RsxNode>,
) -> RsxNode {
    let theme = use_theme().0;
    let size = size.unwrap_or(ButtonSize::Medium);
    let color = color.unwrap_or(ButtonColor::Inherit);
    let disabled = disabled.unwrap_or(false);
    let _edge = edge.unwrap_or(IconButtonEdge::None);

    let spec = size_spec(&theme, size);
    let (color_base, _color_on) = resolve_color_set(&theme, color);

    let transparent: Box<dyn ColorLike> = Box::new(Color::transparent());
    let text_color: Box<dyn ColorLike> = if disabled {
        theme.color.text.disabled.clone()
    } else {
        color_base.clone()
    };
    let hover_background: Box<dyn ColorLike> = if disabled {
        transparent.clone()
    } else {
        theme.color.state.hover.clone()
    };

    rsx! {
        <Element
            style={{
                width: spec.icon_button_size,
                height: spec.icon_button_size,
                layout: Layout::flow()
                    .row()
                    .no_wrap()
                    .justify_content(JustifyContent::Center)
                    .align(Align::Center),
                padding: spec.icon_button_padding,
                border_radius: theme.component.button.icon_button_radius,
                background: resolve(transparent.as_ref()),
                color: resolve(text_color.as_ref()),
                font_size: spec.icon_size,
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
