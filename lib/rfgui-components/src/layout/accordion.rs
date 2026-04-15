use crate::{ExpandMoreIcon, use_theme};
use rfgui::ui::{
    Binding, RsxComponent, RsxNode, component, on_click, props, rsx, use_state,
};
use rfgui::view::Element;
use rfgui::{
    Align, Angle, Border, ClipMode, Color, Cursor, Layout, Length, Position, Rotate, Transform,
    Transition, TransitionProperty, flex,
};

pub struct Accordion;

#[props]
pub struct AccordionProps {
    pub title: String,
    pub default_expanded: Option<bool>,
    pub expanded_binding: Option<Binding<bool>>,
    pub disabled: Option<bool>,
}

impl RsxComponent<AccordionProps> for Accordion {
    fn render(props: AccordionProps, children: Vec<RsxNode>) -> RsxNode {
        rsx! {
            <AccordionView
                title={props.title}
                default_expanded={props.default_expanded.unwrap_or(false)}
                expanded_binding={props.expanded_binding}
                disabled={props.disabled.unwrap_or(false)}
            >
                {children}
            </AccordionView>
        }
    }
}

impl rfgui::ui::RsxTag for Accordion {
    type Props = __AccordionPropsInit;
    type StrictProps = AccordionProps;
    const ACCEPTS_CHILDREN: bool = true;

    fn into_strict(props: Self::Props) -> Self::StrictProps {
        props.into()
    }

    fn create_node(
        props: Self::StrictProps,
        children: Vec<rfgui::ui::RsxNode>,
        _key: Option<rfgui::ui::RsxKey>,
    ) -> rfgui::ui::RsxNode {
        <Self as RsxComponent<AccordionProps>>::render(props, children)
    }
}

#[component]
fn AccordionView(
    title: String,
    default_expanded: bool,
    expanded_binding: Option<Binding<bool>>,
    disabled: bool,
    children: Vec<RsxNode>,
) -> RsxNode {
    let theme = use_theme().0;
    let fallback_expanded = use_state(|| default_expanded);
    let is_expanded = expanded_binding
        .as_ref()
        .map(Binding::get)
        .unwrap_or_else(|| fallback_expanded.get());
    let expanded_state = expanded_binding.unwrap_or_else(|| fallback_expanded.binding());

    let toggle = on_click(move |_event| {
        if disabled {
            return;
        }
        expanded_state.set(!expanded_state.get());
    });

    let content_border = Border::uniform(Length::px(0.0), theme.color.border.as_ref())
        .top(Some(Length::px(1.0)), Some(theme.color.border.as_ref()));
    rsx! {
        <Element
            style={{
                width: Length::percent(100.0),
                layout: Layout::flow().column().no_wrap(),
                border_radius: theme.component.input.radius,
                border: theme.component.card.border.clone(),
                background: if disabled {
                    theme.color.state.disabled.clone()
                } else {
                    theme.color.layer.surface.clone()
                },
            }}
        >
            <Element
                style={{
                    width: Length::percent(100.0),
                    layout: Layout::flex().align(Align::Center),
                    padding: theme.component.input.padding,
                    cursor: if disabled { Cursor::Default } else { Cursor::Pointer },
                    background: if disabled {
                        theme.color.state.disabled.clone()
                    } else if is_expanded {
                        theme.color.state.hover.clone()
                    } else {
                        Box::new(Color::transparent()) as Box<dyn rfgui::ColorLike>
                    },
                    transition: [
                        Transition::new(
                            TransitionProperty::BackgroundColor,
                            theme.motion.duration.normal,
                        )
                        .ease_in_out(),
                    ],
                    hover: {
                        background: if disabled {None} else {theme.color.state.hover.clone()}
                    }
                }}
                on_click={toggle}
            >
                <Element
                    style={{
                        font_size: theme.typography.size.md,
                        flex: flex().grow(1.0),
                        color: if disabled {
                            theme.color.text.disabled.clone()
                        } else {
                            theme.color.text.primary.clone()
                        },
                    }}
                >
                    {title}
                </Element>
                <Element
                    style={{
                        flex: flex().grow(0.0).shrink(0.0),
                        color: if disabled {
                            theme.color.text.disabled.clone()
                        } else {
                            theme.color.text.secondary.clone()
                        },
                        transition: [
                            Transition::new(
                                TransitionProperty::Transform,
                                theme.motion.duration.normal,
                            )
                            .ease_in_out(),
                        ],
                        transform: if is_expanded {
                            Transform::new([Rotate::z(Angle::deg(0.0))])
                        } else {
                            Transform::new([Rotate::z(Angle::deg(270.0))])
                        },
                    }}
                >
                    <ExpandMoreIcon style={{
                        font_size: theme.typography.size.md,
                        color: if disabled {
                            theme.color.text.disabled.clone()
                        } else {
                            theme.color.text.secondary.clone()
                        },
                    }} />
                </Element>
            </Element>
            <Element
                style={{
                    layout: Layout::flex().column(),
                    gap: theme.spacing.sm,
                    position: Position::static_().clip(ClipMode::Parent),
                    height: if is_expanded { None } else { Length::Zero },
                    padding: theme.component.card.padding,
                    border: if is_expanded { content_border } else { None },
                    background: theme.color.layer.surface.clone(),
                    transition: [
                        Transition::new(
                            TransitionProperty::Height,
                            theme.motion.duration.normal,
                        )
                        .ease_in_out(),
                    ],
                }}
            >
                {children}
            </Element>
        </Element>
    }
}
