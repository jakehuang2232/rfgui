use std::any::Any;
use std::rc::Rc;

use rfgui::ui::host::{Element, Text};
use rfgui::ui::{
    Binding, BlurHandlerProp, ClickHandlerProp, KeyDownHandlerProp, MouseDownHandlerProp,
    RsxComponent, RsxNode, component, props, rsx, use_state,
};
use rfgui::{
    AlignItems, Border, BorderRadius, ClipMode, Collision, CollisionBoundary, Color, Display,
    JustifyContent, Length, Padding, Position, ScrollDirection,
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

    let width = 220.0_f32;
    let height = 40.0_f32;

    let fallback_open = use_state(|| false);
    let open_binding = fallback_open.binding();
    let menu_pointer_down = use_state(|| false).binding();
    let is_open = open_binding.get();

    let trigger_click = {
        let open_binding = open_binding.clone();
        ClickHandlerProp::new(move |event| {
            open_binding.set(!open_binding.get());
            event.meta.stop_propagation();
        })
    };
    let trigger_blur = {
        let open_binding = open_binding.clone();
        let menu_pointer_down = menu_pointer_down.clone();
        BlurHandlerProp::new(move |_| {
            // Keep menu alive during option press so click can complete selection.
            if menu_pointer_down.get() {
                menu_pointer_down.set(false);
                return;
            }
            open_binding.set(false);
        })
    };
    let trigger_key_down = {
        let open_binding = open_binding.clone();
        KeyDownHandlerProp::new(move |event| {
            let key = event.key.key.as_str();
            let code = event.key.code.as_str();
            if key_matches(key, code, "Escape") {
                open_binding.set(false);
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

    let mut root = rsx! {
        <Element
            style={{
                width: Length::px(width),
                display: Display::flow().column().no_wrap(),
            }}
        >
            <Element
                style={{
                    display: Display::flow().row().no_wrap(),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::SpaceBetween,
                    width: Length::px(width),
                    height: Length::px(height),
                    border_radius: BorderRadius::uniform(Length::px(8.0)),
                    border: Border::uniform(Length::px(1.0), &Color::hex("#B0BEC5")),
                    background: Color::hex("#FFFFFF"),
                    padding: Padding::uniform(Length::px(0.0)).x(Length::px(12.0)),
                    hover: {
                        background: Color::hex("#FAFAFA"),
                    }
                }}
                anchor={SELECT_TRIGGER_ANCHOR}
                on_click={trigger_click}
                on_blur={trigger_blur}
                on_key_down={trigger_key_down}
            >
                <Text style={{ color: Color::hex("#111827") }}>
                    {selected_label}
                </Text>
                <Text style={{ color: Color::hex("#6B7280") }}>
                    {if is_open { "▴" } else { "▾" }}
                </Text>
            </Element>
        </Element>
    };

    if is_open && let RsxNode::Element(root_node) = &mut root {
        root_node.children.push(build_menu_node(
            &menu_items,
            width,
            open_binding,
            menu_pointer_down,
            height,
            SELECT_TRIGGER_ANCHOR,
        ));
    }

    root
}

fn build_menu_node(
    menu_items: &[SelectMenuItem],
    width: f32,
    menu_open: Binding<bool>,
    menu_pointer_down: Binding<bool>,
    trigger_height: f32,
    anchor_name: &str,
) -> RsxNode {
    let option_nodes: Vec<RsxNode> = menu_items
        .iter()
        .map(|item| {
            let menu_open = menu_open.clone();
            let menu_pointer_down_for_mouse_down = menu_pointer_down.clone();
            let option_disabled = item.disabled;
            let mouse_down = MouseDownHandlerProp::new(move |event| {
                if option_disabled {
                    return;
                }
                menu_pointer_down_for_mouse_down.set(true);
                event.meta.stop_propagation();
            });
            let menu_pointer_down_for_click = menu_pointer_down.clone();
            let option_disabled = item.disabled;
            let on_select = item.on_select.clone();
            let click = ClickHandlerProp::new(move |event| {
                if option_disabled {
                    return;
                }
                on_select.call(event);
                menu_open.set(false);
                menu_pointer_down_for_click.set(false);
                event.meta.stop_propagation();
            });

            rsx! {
                <Element
                    style={{
                        width: Length::px(width),
                        height: Length::px(32.0),
                        display: Display::flow().row().no_wrap(),
                        align_items: AlignItems::Center,
                        padding: Padding::uniform(Length::px(0.0)).x(Length::px(12.0)),
                        background: if item.disabled {
                            Color::hex("#F9FAFB9E")
                        } else if item.selected {
                            Color::hex("#EEF3FE9E")
                        } else {
                            Color::hex("#00000000")
                        },
                        hover: {
                            background: Color::hex("#F5F7FA"),
                        }
                    }}
                    on_mouse_down={mouse_down}
                    on_click={click}
                >
                    <Text
                        font_size=13
                        line_height=1.0
                        font="Heiti TC, Noto Sans CJK TC, Roboto"
                        style={{ color: if item.disabled { Color::hex("#9CA3AF") } else if item.selected { Color::hex("#1D4ED8") } else { Color::hex("#111827") } }}
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
                    .top(Length::px(trigger_height + 4.0))
                    .left(Length::px(0.0))
                    .collision(Collision::FlipFit, CollisionBoundary::Viewport)
                    .clip(ClipMode::Viewport),
                width: Length::px(width),
                max_height: Length::vh(50.0),
                display: Display::flow().column().no_wrap(),
                border_radius: BorderRadius::uniform(Length::px(8.0)),
                border: Border::uniform(Length::px(1.0), &Color::hex("#B0BEC5")),
                background: Color::hex("#FFFFFF"),
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
