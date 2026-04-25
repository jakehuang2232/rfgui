use std::any::Any;
use std::rc::Rc;

use crate::{ExpandMoreIcon, use_theme};
use rfgui::ui::{
    Binding, BlurHandlerProp, ClickHandlerProp, FocusHandlerProp, KeyDownHandlerProp,
    PointerDownHandlerProp, RsxComponent, RsxNode, component, props, rsx, use_state,
};
use rfgui::view::{Element, Text};
use rfgui::{
    Align, Angle, ClipMode, Collision, CollisionBoundary, Color, ColorLike, CrossSize, Layout,
    Length, Operator, Position, Rotate, ScrollDirection, Transform, Transition, TransitionProperty,
    flex,
};

pub struct Select<DataType = (), ValueType = ()>(std::marker::PhantomData<(DataType, ValueType)>)
where
    ValueType: 'static;

#[derive(Clone)]
#[props]
pub struct SelectProps<DataType, ValueType: 'static> {
    pub data: Vec<DataType>,
    pub to_label: fn(&DataType, usize) -> String,
    pub to_value: Option<fn(&DataType, usize) -> ValueType>,
    pub to_disabled: Option<fn(&DataType, usize) -> bool>,
    pub value: Binding<ValueType>,
}

#[derive(Clone)]
struct SelectMenuItem {
    key: usize,
    label: String,
    selected: bool,
    disabled: bool,
    on_select: ClickHandlerProp,
}

impl<DataType, ValueType> RsxComponent<SelectProps<DataType, ValueType>>
    for Select<DataType, ValueType>
where
    DataType: Clone + 'static,
    ValueType: Clone + PartialEq + 'static,
{
    fn render(props: SelectProps<DataType, ValueType>, _children: Vec<RsxNode>) -> RsxNode {
        let selected_value = props.value.get();
        let selected_index =
            resolve_selected_index(&props.data, &selected_value, props.to_value, props.to_label);
        let selected_label = resolve_option_text(&props.data, selected_index, props.to_label);

        let menu_items: Vec<SelectMenuItem> = props
            .data
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let label = (props.to_label)(item, index);
                let value = value_of_item(item, index, props.to_label, props.to_value);
                let disabled = props
                    .to_disabled
                    .map(|resolver| resolver(item, index))
                    .unwrap_or(false);
                let selected = value == selected_value;
                let value_binding = props.value.clone();
                let on_select = ClickHandlerProp::new(move |event| {
                    if disabled {
                        return;
                    }
                    value_binding.set(value.clone());
                    event.meta.stop_propagation();
                });

                SelectMenuItem {
                    key: index,
                    label,
                    selected,
                    disabled,
                    on_select,
                }
            })
            .collect();

        rsx! {
            <SelectView
                selected_label={selected_label}
                menu_items={menu_items}
            />
        }
    }
}

#[rfgui::ui::component]
impl<DataType, ValueType> rfgui::ui::RsxTag for Select<DataType, ValueType>
where
    DataType: Clone + 'static,
    ValueType: Clone + PartialEq + 'static,
{
    type Props = __SelectPropsInit<DataType, ValueType>;
    type StrictProps = SelectProps<DataType, ValueType>;
    const ACCEPTS_CHILDREN: bool = false;

    fn into_strict(props: Self::Props) -> Self::StrictProps {
        props.into()
    }

    fn create_node(
        props: Self::StrictProps,
        children: Vec<rfgui::ui::RsxNode>,
        _key: Option<rfgui::ui::RsxKey>,
    ) -> rfgui::ui::RsxNode {
        <Self as RsxComponent<SelectProps<DataType, ValueType>>>::render(props, children)
    }
}

#[component]
fn SelectView(selected_label: String, menu_items: Vec<SelectMenuItem>) -> RsxNode {
    const SELECT_TRIGGER_ANCHOR: &str = "__rfgui_select_trigger_anchor";

    let fallback_open = use_state(|| false);
    let open_binding = fallback_open.binding();
    let fallback_focused = use_state(|| false);
    let focused_binding = fallback_focused.binding();
    let was_focused_on_pointer_down = use_state(|| false);
    let was_focused_on_pointer_down_binding = was_focused_on_pointer_down.binding();
    let is_open = open_binding.get();
    let is_focused = focused_binding.get();
    let theme = use_theme().0;

    let pseudo_focus = {
        let open_binding = open_binding.clone();
        let focused_binding = focused_binding.clone();
        FocusHandlerProp::new(move |event| {
            focused_binding.set(true);
            open_binding.set(true);
            event.meta.stop_propagation();
        })
    };
    let pseudo_blur = {
        let open_binding = open_binding.clone();
        let focused_binding = focused_binding.clone();
        BlurHandlerProp::new(move |_| {
            focused_binding.set(false);
            open_binding.set(false);
        })
    };
    let pseudo_key_down = {
        let open_binding = open_binding.clone();
        KeyDownHandlerProp::new(move |event| {
            use rfgui::platform::Key;
            let key = event.key.key;
            if key == Key::Escape {
                event.meta.viewport().set_focus(None);
                event.meta.stop_propagation();
                return;
            }
            if key == Key::Enter || key == Key::NumberPadEnter {
                open_binding.set(!open_binding.get());
                event.meta.stop_propagation();
                return;
            }
            if key == Key::Tab {
                open_binding.set(false);
            }
        })
    };
    let pseudo_mouse_down = {
        let was_focused_on_pointer_down_binding = was_focused_on_pointer_down_binding.clone();
        PointerDownHandlerProp::new(move |event| {
            was_focused_on_pointer_down_binding.set(is_focused);
            if event.meta.focus_change_suppressed() {
                return;
            }
            event
                .viewport
                .set_focus(Some(event.meta.current_target_id()));
        })
    };
    let trigger_click = {
        let was_focused_on_pointer_down_binding = was_focused_on_pointer_down_binding.clone();
        let open_binding = open_binding.clone();
        ClickHandlerProp::new(move |event| {
            if was_focused_on_pointer_down_binding.get() {
                event.meta.viewport().set_focus(None);
            } else {
                open_binding.set(true);
            }
            event.meta.stop_propagation();
        })
    };

    let mut root = rsx! {
        <Element
            style={{
                max_width: Length::percent(100.0),
                font_size: theme.typography.size.sm,
            }}
            on_pointer_down={pseudo_mouse_down}
            on_focus={pseudo_focus}
            on_blur={pseudo_blur}
            on_key_down={pseudo_key_down}
        >
            <Element
                style={{
                    color: theme.color.background.on,
                    max_width: Length::percent(100.0),
                    layout: Layout::flex()
                        .row()
                        .align(Align::Center),
                    border_radius: theme.component.input.radius,
                    border: theme.component.input.border.clone(),
                    background: theme.color.background.base,
                    padding: theme.component.input.padding,
                    hover: {
                        background: theme.component.select.trigger_hover_background.clone(),
                    }
                }}
                anchor={SELECT_TRIGGER_ANCHOR}
                on_click={trigger_click}
            >
                <Element style={{
                    flex: flex().grow(1.0),
                    width: Length::calc(Length::percent(100.0), Operator::subtract, Length::px(24.0)),
                }}>
                    {selected_label}
                </Element>
                <Element style={{
                    flex: flex().grow(0.0).shrink(0.0),
                    color: theme.color.text.secondary.clone(),
                    transition: [
                        Transition::new(
                            TransitionProperty::Transform,
                            theme.motion.duration.normal,
                        )
                        .ease_in_out(),
                    ],
                    transform: if is_open {
                        Transform::new([Rotate::z(Angle::deg(0.0))])
                    } else {
                        Transform::new([Rotate::z(Angle::deg(270.0))])
                    },
                }}>
                    <ExpandMoreIcon style={{
                        font_size: theme.typography.size.md,
                        color: theme.color.text.secondary.clone(),
                    }} />
                </Element>
            </Element>
        </Element>
    };

    if is_open && let RsxNode::Element(root_node) = &mut root {
        std::rc::Rc::make_mut(root_node)
            .children
            .push(build_menu_node(&menu_items, SELECT_TRIGGER_ANCHOR));
    }

    root
}

fn build_menu_node(menu_items: &[SelectMenuItem], anchor_name: &str) -> RsxNode {
    let theme = use_theme().0;
    let option_nodes: Vec<RsxNode> = menu_items
        .iter()
        .map(|item| {
            let mouse_down = PointerDownHandlerProp::new(move |event| {
                event.meta.suppress_focus_change();
                event.meta.stop_propagation();
            });
            let option_disabled = item.disabled;
            let on_select = item.on_select.clone();
            let click = ClickHandlerProp::new(move |event| {
                if option_disabled {
                    return;
                }
                on_select.call(event);
                event.meta.viewport().set_focus(None);
                event.meta.stop_propagation();
            });

            rsx! {
                <Element
                    key={item.key}
                    style={{
                        layout: Layout::flex().row(),
                        width: Length::percent(100.0),
                        padding: theme.component.input.padding,
                        background: if item.disabled {
                            theme.component.select.option_disabled_background.clone()
                        } else if item.selected {
                            theme.component.select.option_selected_background.clone()
                        } else {
                            Box::new(Color::transparent()) as Box<dyn ColorLike>
                        },
                        hover: {
                            background: theme.component.select.option_hover_background.clone(),
                        }
                    }}
                    on_pointer_down={mouse_down}
                    on_click={click}
                >
                    <Text
                        style={{
                            color: if item.disabled {
                                theme.component.select.option_disabled_text.clone()
                            } else if item.selected {
                                theme.component.select.option_selected_text.clone()
                            } else {
                                theme.color.background.on.clone()
                            }
                        }}
                    >
                        {item.label.clone()}
                    </Text>
                </Element>
            }
        })
        .collect();

    rsx! {
        <Element
            style={{
                position: Position::absolute()
                    .anchor(anchor_name)
                    .top(Length::calc(Length::percent(100.0), Operator::subtract, Length::px(1.0)))
                    .left(Length::px(0.0))
                    .collision(Collision::FlipFit, CollisionBoundary::Viewport)
                    .clip(ClipMode::Viewport),
                max_height: Length::vh(50.0),
                width: Length::percent(100.0),
                layout: Layout::flow()
                    .column()
                    .no_wrap()
                    .cross_size(CrossSize::Stretch),
                border_radius: theme.component.input.radius,
                border: theme.component.input.border.clone(),
                background: theme.color.background.base,
                scroll_direction: ScrollDirection::Vertical,
            }}
        >
            {option_nodes}
        </Element>
    }
}

fn resolve_option_text<DataType>(
    data: &[DataType],
    selected_index: usize,
    to_label: fn(&DataType, usize) -> String,
) -> String {
    if data.is_empty() {
        return String::new();
    }
    data.get(selected_index)
        .map(|item| to_label(item, selected_index))
        .unwrap_or_else(|| to_label(&data[0], 0))
}

fn resolve_selected_index<DataType, ValueType>(
    data: &[DataType],
    selected_value: &ValueType,
    to_value: Option<fn(&DataType, usize) -> ValueType>,
    to_label: fn(&DataType, usize) -> String,
) -> usize
where
    ValueType: Clone + PartialEq + 'static,
{
    if data.is_empty() {
        return 0;
    }
    data.iter()
        .enumerate()
        .position(|(index, item)| value_of_item(item, index, to_label, to_value) == *selected_value)
        .unwrap_or(0)
}

fn value_of_item<DataType, ValueType>(
    item: &DataType,
    index: usize,
    to_label: fn(&DataType, usize) -> String,
    to_value: Option<fn(&DataType, usize) -> ValueType>,
) -> ValueType
where
    ValueType: Clone + 'static,
{
    if let Some(to_value) = to_value {
        return to_value(item, index);
    }
    let label = to_label(item, index);
    let erased: Rc<dyn Any> = Rc::new(label);
    if let Ok(v) = Rc::downcast::<ValueType>(erased) {
        return (*v).clone();
    }
    panic!("Select prop `to_value` is required when ValueType is not String");
}
