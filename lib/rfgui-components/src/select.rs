use std::any::Any;
use std::rc::Rc;

use crate::use_theme;
use rfgui::ui::host::{Element, Text};
use rfgui::ui::{
    Binding, BlurHandlerProp, ClickHandlerProp, FocusHandlerProp, KeyDownHandlerProp,
    MouseDownHandlerProp,
    RsxComponent, RsxNode, component, props, rsx, use_state,
};
use rfgui::{
    AlignItems, ClipMode, Collision, CollisionBoundary, Color, Display, ColorLike,
    JustifyContent, Length, Operator, Position, ScrollDirection,
};

pub struct Select;

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
    label: String,
    selected: bool,
    disabled: bool,
    on_select: ClickHandlerProp,
}

impl<DataType, ValueType> RsxComponent<SelectProps<DataType, ValueType>> for Select
where
    DataType: Clone + 'static,
    ValueType: Clone + PartialEq + 'static,
{
    fn render(props: SelectProps<DataType, ValueType>) -> RsxNode {
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

#[component]
fn SelectView(selected_label: String, menu_items: Vec<SelectMenuItem>) -> RsxNode {
    const SELECT_TRIGGER_ANCHOR: &str = "__rfgui_select_trigger_anchor";

    let fallback_open = use_state(|| false);
    let open_binding = fallback_open.binding();
    let is_open = open_binding.get();
    let theme = use_theme().get();

    let pseudo_focus = {
        let open_binding = open_binding.clone();
        FocusHandlerProp::new(move |event| {
            open_binding.set(true);
            event.meta.stop_propagation();
        })
    };
    let pseudo_blur = {
        let open_binding = open_binding.clone();
        BlurHandlerProp::new(move |_| {
            open_binding.set(false);
        })
    };
    let pseudo_key_down = {
        let open_binding = open_binding.clone();
        KeyDownHandlerProp::new(move |event| {
            let key = event.key.key.as_str();
            let code = event.key.code.as_str();
            if key_matches(key, code, "Escape") {
                event.meta.viewport().set_focus(None);
                event.meta.stop_propagation();
                return;
            }
            if key_matches(key, code, "Enter") {
                open_binding.set(!open_binding.get());
                event.meta.stop_propagation();
                return;
            }
            if key_matches(key, code, "Tab") {
                open_binding.set(false);
            }
        })
    };
    let pseudo_mouse_down = MouseDownHandlerProp::new(move |event| {
        if event.meta.keep_focus_requested() {
            return;
        }
        event
            .viewport
            .set_focus(Some(event.meta.current_target_id()));
    });

    let mut root = rsx! {
        <Element
            style={{
                max_width: Length::percent(100.0),
                font_size: theme.typography.size.sm,
            }}
            on_mouse_down={pseudo_mouse_down}
            on_focus={pseudo_focus}
            on_blur={pseudo_blur}
            on_key_down={pseudo_key_down}
        >
            <Element
                style={{
                    color: theme.color.background.on,
                    max_width: Length::percent(100.0),
                    display: Display::flow()
                        .row()
                        .no_wrap()
                        .justify_content(JustifyContent::SpaceBetween),
                    align_items: AlignItems::Center,
                    border_radius: theme.component.input.radius,
                    border: theme.component.input.border.clone(),
                    background: theme.color.background.base,
                    padding: theme.component.input.padding,
                    hover: {
                        background: theme.component.select.trigger_hover_background.clone(),
                    }
                }}
                anchor={SELECT_TRIGGER_ANCHOR}
            >
                <Element style={{
                    width: Length::calc(Length::percent(100.0), Operator::subtract, Length::px(24.0)),
                }}>
                    {selected_label}
                </Element>
                <Element>
                    {if is_open { "▴" } else { "▾" }}
                </Element>
            </Element>
        </Element>
    };

    if is_open && let RsxNode::Element(root_node) = &mut root {
        root_node
            .children
            .push(build_menu_node(&menu_items, open_binding, SELECT_TRIGGER_ANCHOR));
    }

    root
}

fn build_menu_node(
    menu_items: &[SelectMenuItem],
    menu_open: Binding<bool>,
    anchor_name: &str,
) -> RsxNode {
    let theme = use_theme().get();
    let option_nodes: Vec<RsxNode> = menu_items
        .iter()
        .map(|item| {
            let menu_open = menu_open.clone();
            let mouse_down = MouseDownHandlerProp::new(move |event| {
                event.meta.request_keep_focus();
                event.meta.stop_propagation();
            });
            let option_disabled = item.disabled;
            let on_select = item.on_select.clone();
            let click = ClickHandlerProp::new(move |event| {
                if option_disabled {
                    return;
                }
                on_select.call(event);
                menu_open.set(false);
                event.meta.stop_propagation();
            });

            rsx! {
                <Element
                    style={{
                        display: Display::flow().row().no_wrap(),
                        align_items: AlignItems::Center,
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
                    on_mouse_down={mouse_down}
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
                display: Display::flow().column().no_wrap(),
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

fn key_matches(key: &str, code: &str, token: &str) -> bool {
    key.eq_ignore_ascii_case(token)
        || key == format!("Named({token})")
        || code == format!("Code({token})")
}
