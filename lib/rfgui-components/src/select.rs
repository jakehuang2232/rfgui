use rfgui::ui::host::{Element, Text};
use rfgui::ui::{Binding, ClickHandlerProp, RsxNode, component, rsx, use_state};
use rfgui::{
    AlignItems, Border, BorderRadius, ClipMode, Collision, CollisionBoundary, Color, Display,
    JustifyContent, Length, Padding, ParsedValue, Position, PropertyId, Style,
};

pub struct SelectProps {
    pub options: Vec<String>,
    pub selected_index: usize,
    pub selected_binding: Option<Binding<usize>>,
    pub width: f32,
    pub height: f32,
    pub disabled: bool,
}

impl SelectProps {
    pub fn new(options: Vec<String>) -> Self {
        Self {
            options,
            selected_index: 0,
            selected_binding: None,
            width: 220.0,
            height: 40.0,
            disabled: false,
        }
    }
}

pub fn build_select_rsx(props: SelectProps) -> RsxNode {
    let has_selected_binding = props.selected_binding.is_some();
    let selected_binding = props
        .selected_binding
        .unwrap_or_else(|| Binding::new(props.selected_index));
    rsx! {
        <SelectComponent
            options={props.options}
            selected_index={props.selected_index as i64}
            selected_binding={selected_binding}
            has_selected_binding={has_selected_binding}
            width={props.width}
            height={props.height}
            disabled={props.disabled}
        />
    }
}

#[component]
fn SelectComponent(
    options: Vec<String>,
    selected_index: i64,
    selected_binding: Binding<usize>,
    has_selected_binding: bool,
    width: f32,
    height: f32,
    disabled: bool,
) -> RsxNode {
    const SELECT_TRIGGER_ANCHOR: &str = "__rfgui_select_trigger_anchor";
    let fallback_selected = use_state(|| selected_index.max(0) as usize);
    let selected_binding = if has_selected_binding {
        selected_binding
    } else {
        fallback_selected.binding()
    };
    let fallback_open = use_state(|| false);
    let open_binding = fallback_open.binding();
    let selected_index = selected_binding.get();
    let option_text = resolve_option_text(&options, selected_index);
    let is_open = !disabled && open_binding.get();

    let trigger_click = {
        let open_binding = open_binding.clone();
        let disabled = disabled;
        ClickHandlerProp::new(move |event| {
            if disabled {
                return;
            }
            open_binding.set(!open_binding.get());
            event.meta.stop_propagation();
        })
    };

    let mut root = rsx! {
        <Element style={select_root_style(width)}>
            <Element
                style={select_trigger_style(width, height, disabled)}
                anchor={SELECT_TRIGGER_ANCHOR}
                on_click={trigger_click}
            >
                <Text
                    font_size=14
                    line_height=1.0
                    font="Heiti TC, Noto Sans CJK TC, Roboto"
                    color={if disabled { "#9E9E9E" } else { "#111827" }}
                >
                    {option_text}
                </Text>
                <Text
                    font_size=14
                    line_height=1.0
                    font="Heiti TC, Noto Sans CJK TC, Roboto"
                    color={if disabled { "#BDBDBD" } else { "#6B7280" }}
                >
                    {if is_open { "▴" } else { "▾" }}
                </Text>
            </Element>
        </Element>
    };

    if is_open
            && let RsxNode::Element(root_node) = &mut root
    {
        root_node.children.push(build_menu_node(
            &options,
            width,
            selected_index,
            selected_binding,
            open_binding,
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
    // style.insert(PropertyId::Display, ParsedValue::Display(Display::flow().column()));
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
            let click = ClickHandlerProp::new(move |event| {
                binding.set(index);
                menu_open.set(false);
                event.meta.stop_propagation();
            });

            rsx! {
                <Element style={select_option_style(width, selected_index == index)} on_click={click}>
                    <Text
                        font_size=13
                        line_height=1.0
                        font="Heiti TC, Noto Sans CJK TC, Roboto"
                        color={if selected_index == index { "#1D4ED8" } else { "#111827" }}
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
