use rfgui::ui::host::{Element, Text};
use rfgui::ui::{Binding, BlurHandlerProp, ClickHandlerProp, KeyDownHandlerProp, MouseDownHandlerProp, RsxComponent, RsxNode, component, rsx, use_state, props};
use rfgui::{
    AlignItems, Border, BorderRadius, ClipMode, Collision, CollisionBoundary, Color, Display,
    JustifyContent, Length, Padding, ParsedValue, Position, PropertyId, Style,
};

pub struct Select;

#[props]
pub struct SelectProps {
    pub options: Vec<String>,
    pub binding: Option<Binding<usize>>,
    pub selected_index: Option<i64>,
    pub disabled: Option<bool>,
}

impl RsxComponent for Select {
    type Props = SelectProps;

    fn render(props: Self::Props) -> RsxNode {
        let selected_index = props.selected_index.unwrap_or(0);
        if selected_index < 0 {
            panic!("rsx build error on <Select>. prop `selected_index` expects non-negative value");
        }
        let has_binding = props.binding.is_some();
        let binding = props
            .binding
            .unwrap_or_else(|| Binding::new(selected_index as usize));

        rsx! {
            <SelectView
                options={props.options}
                selected_index={selected_index}
                selected_binding={binding}
                has_selected_binding={has_binding}
                disabled={props.disabled.unwrap_or(false)}
            />
        }
    }
}

#[component]
fn SelectView(
    options: Vec<String>,
    selected_index: i64,
    selected_binding: Binding<usize>,
    has_selected_binding: bool,
    disabled: bool,
) -> RsxNode {
    const SELECT_TRIGGER_ANCHOR: &str = "__rfgui_select_trigger_anchor";

    let width = 220.0_f32;
    let height = 40.0_f32;

    let fallback_selected = use_state(|| selected_index as usize);
    let selected_binding = if has_selected_binding {
        selected_binding
    } else {
        fallback_selected.binding()
    };

    let fallback_open = use_state(|| false);
    let open_binding = fallback_open.binding();
    let menu_pointer_down = use_state(|| false).binding();
    let selected_index = selected_binding.get();
    let option_text = resolve_option_text(&options, selected_index);
    let is_open = !disabled && open_binding.get();

    let trigger_click = {
        let open_binding = open_binding.clone();
        ClickHandlerProp::new(move |event| {
            if disabled {
                return;
            }
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
        <Element style={select_root_style(width)}>
            <Element
                style={select_trigger_style(width, height, disabled)}
                anchor={SELECT_TRIGGER_ANCHOR}
                on_click={trigger_click}
                on_blur={trigger_blur}
                on_key_down={trigger_key_down}
            >
                <Text
                    style={{ color: if disabled { "#9E9E9E" } else { "#111827" } }}
                >
                    {option_text}
                </Text>
                <Text
                    style={{ color: if disabled { "#BDBDBD" } else { "#6B7280" } }}
                >
                    {if is_open { "▴" } else { "▾" }}
                </Text>
            </Element>
        </Element>
    };

    if is_open && let RsxNode::Element(root_node) = &mut root {
        root_node.children.push(build_menu_node(
            &options,
            width,
            selected_index,
            selected_binding,
            open_binding,
            menu_pointer_down,
            height,
            SELECT_TRIGGER_ANCHOR,
        ));
    }

    root
}

fn select_root_style(width: f32) -> Style {
    let mut style = Style::new();
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(width)));
    style.insert(
        PropertyId::Display,
        ParsedValue::Display(Display::flow().column().no_wrap()),
    );
    style
}

fn select_trigger_style(width: f32, height: f32, disabled: bool) -> Style {
    let mut style = Style::new();
    style.insert(
        PropertyId::Display,
        ParsedValue::Display(Display::flow().row().no_wrap()),
    );
    style.insert(
        PropertyId::AlignItems,
        ParsedValue::AlignItems(AlignItems::Center),
    );
    style.insert(
        PropertyId::JustifyContent,
        ParsedValue::JustifyContent(JustifyContent::SpaceBetween),
    );
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(width)));
    style.insert(PropertyId::Height, ParsedValue::Length(Length::px(height)));
    style.set_border_radius(BorderRadius::uniform(Length::px(8.0)));
    style.set_border(Border::uniform(Length::px(1.0), &Color::hex("#B0BEC5")));
    style.insert_color_like(
        PropertyId::BackgroundColor,
        if disabled {
            Color::hex("#F5F5F5")
        } else {
            Color::hex("#FFFFFF")
        },
    );
    style.set_padding(Padding::uniform(Length::px(0.0)).x(Length::px(12.0)));
    let mut hover = Style::new();
    hover.insert_color_like(PropertyId::BackgroundColor, Color::hex("#FAFAFA"));
    style.set_hover(hover);
    style
}

fn select_menu_style(width: f32, trigger_height: f32, anchor_name: &str) -> Style {
    let mut style = Style::new();
    style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .anchor(anchor_name)
                .top(Length::px(trigger_height + 4.0))
                .left(Length::px(0.0))
                .collision(Collision::FlipFit, CollisionBoundary::Viewport)
                .clip(ClipMode::Viewport),
        ),
    );
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(width)));
    style.insert(
        PropertyId::Display,
        ParsedValue::Display(Display::flow().column().no_wrap()),
    );
    style.set_border_radius(BorderRadius::uniform(Length::px(8.0)));
    style.set_border(Border::uniform(Length::px(1.0), &Color::hex("#B0BEC5")));
    style.insert_color_like(PropertyId::BackgroundColor, Color::hex("#FFFFFF"));
    style
}

fn select_option_style(width: f32, selected: bool) -> Style {
    let mut style = Style::new();
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(width)));
    style.insert(PropertyId::Height, ParsedValue::Length(Length::px(32.0)));
    style.insert(
        PropertyId::Display,
        ParsedValue::Display(Display::flow().row().no_wrap()),
    );
    style.insert(
        PropertyId::AlignItems,
        ParsedValue::AlignItems(AlignItems::Center),
    );
    style.set_padding(Padding::uniform(Length::px(0.0)).x(Length::px(12.0)));
    style.insert_color_like(
        PropertyId::BackgroundColor,
        if selected {
            Color::hex("#EEF3FE")
        } else {
            Color::hex("#FFFFFF")
        },
    );
    let mut hover = Style::new();
    hover.insert_color_like(PropertyId::BackgroundColor, Color::hex("#F5F7FA"));
    style.set_hover(hover);
    style
}

fn build_menu_node(
    options: &[String],
    width: f32,
    selected_index: usize,
    selected_binding: Binding<usize>,
    menu_open: Binding<bool>,
    menu_pointer_down: Binding<bool>,
    trigger_height: f32,
    anchor_name: &str,
) -> RsxNode {
    let option_nodes: Vec<RsxNode> = options
        .iter()
        .enumerate()
        .map(|(index, option)| {
            let option_text = option.clone();
            let binding = selected_binding.clone();
            let menu_open = menu_open.clone();
            let menu_pointer_down_for_mouse_down = menu_pointer_down.clone();
            let mouse_down = MouseDownHandlerProp::new(move |_| {
                menu_pointer_down_for_mouse_down.set(true);
            });
            let menu_pointer_down_for_click = menu_pointer_down.clone();
            let click = ClickHandlerProp::new(move |event| {
                binding.set(index);
                menu_open.set(false);
                menu_pointer_down_for_click.set(false);
                event.meta.stop_propagation();
            });

            rsx! {
                <Element
                    style={select_option_style(width, selected_index == index)}
                    on_mouse_down={mouse_down}
                    on_click={click}
                >
                    <Text
                        font_size=13
                        line_height=1.0
                        font="Heiti TC, Noto Sans CJK TC, Roboto"
                        style={{ color: if selected_index == index { "#1D4ED8" } else { "#111827" } }}
                    >
                        {option_text}
                    </Text>
                </Element>
            }
        })
        .collect();

    rsx! {
        <Element style={select_menu_style(width, trigger_height, anchor_name)}>
            {option_nodes}
        </Element>
    }
}

fn resolve_option_text(options: &[String], selected_index: usize) -> String {
    if options.is_empty() {
        return String::new();
    }
    options
        .get(selected_index)
        .cloned()
        .unwrap_or_else(|| options[0].clone())
}

fn key_matches(key: &str, code: &str, token: &str) -> bool {
    key.eq_ignore_ascii_case(token)
        || key == format!("Named({token})")
        || code == format!("Code({token})")
}
