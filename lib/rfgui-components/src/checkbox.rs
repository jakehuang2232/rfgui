use rfgui::ui::host::{Element, Text};
use rfgui::ui::{Binding, ClickHandlerProp, RsxNode, component, rsx, use_state};
use rfgui::{
    AlignItems, Border, BorderRadius, Color, Display, Length, Padding, ParsedValue, PropertyId,
    Style,
};

pub struct CheckboxProps {
    pub label: String,
    pub checked: bool,
    pub checked_binding: Option<Binding<bool>>,
    pub width: f32,
    pub height: f32,
    pub disabled: bool,
}

impl CheckboxProps {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            checked: false,
            checked_binding: None,
            width: 220.0,
            height: 32.0,
            disabled: false,
        }
    }
}

pub fn build_checkbox_rsx(props: CheckboxProps) -> RsxNode {
    let has_binding = props.checked_binding.is_some();
    let binding = props
        .checked_binding
        .unwrap_or_else(|| Binding::new(props.checked));
    rsx! {
        <CheckboxComponent
            label={props.label}
            checked={props.checked}
            has_binding={has_binding}
            binding={binding}
            width={props.width}
            height={props.height}
            disabled={props.disabled}
        />
    }
}

#[component]
fn CheckboxComponent(
    label: String,
    checked: bool,
    has_binding: bool,
    binding: Binding<bool>,
    width: f32,
    height: f32,
    disabled: bool,
) -> RsxNode {
    let fallback_checked = use_state(|| checked);
    let checked_binding = if has_binding {
        binding
    } else {
        fallback_checked.binding()
    };
    let checked = checked_binding.get();
    let click = if disabled {
        None
    } else {
        Some(ClickHandlerProp::new(move |_event| {
            checked_binding.set(!checked_binding.get())
        }))
    };

    let mut box_node = rsx! {
        <Element style={checkbox_box_style(checked, disabled)} />
    };
    if checked && let RsxNode::Element(node) = &mut box_node {
        node.children.push(rsx! {
            <Text
                font_size=16
                font="Heiti TC, Noto Sans CJK TC, Roboto"
                color={if disabled { "#9E9E9E" } else { "#FFFFFF" }}
            >
                {"✓"}
            </Text>
        });
    }

    let mut root = rsx! {
        <Element style={checkbox_row_style(width, height)}>
            {box_node}
            <Text
                font_size=14
                font="Heiti TC, Noto Sans CJK TC, Roboto"
                color={if disabled { "#9E9E9E" } else { "#1F2937" }}
            >
                {label}
            </Text>
        </Element>
    };

    if let Some(handler) = click
        && let RsxNode::Element(node) = &mut root
    {
        node.props.push(("on_click".to_string(), handler.into()));
    }

    root
}

fn checkbox_row_style(width: f32, height: f32) -> Style {
    let mut style = Style::new();
    style.insert(
        PropertyId::Display,
        ParsedValue::Display(Display::flow().row().no_wrap()),
    );
    style.insert(
        PropertyId::AlignItems,
        ParsedValue::AlignItems(AlignItems::Center),
    );
    style.insert(PropertyId::Gap, ParsedValue::Length(Length::px(10.0)));
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(width)));
    style.insert(PropertyId::Height, ParsedValue::Length(Length::px(height)));
    style.set_padding(Padding::uniform(Length::px(0.0)));
    style
}

fn checkbox_box_style(checked: bool, disabled: bool) -> Style {
    let mut style = Style::new();
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(18.0)));
    style.insert(PropertyId::Height, ParsedValue::Length(Length::px(18.0)));
    style.set_border_radius(BorderRadius::uniform(Length::px(4.0)));
    if disabled {
        style.insert_color_like(PropertyId::BackgroundColor, Color::hex("#F5F5F5"));
        style.set_border(Border::uniform(Length::px(1.0), &Color::hex("#BDBDBD")));
    } else if checked {
        style.insert_color_like(PropertyId::BackgroundColor, Color::hex("#1976D2"));
        style.set_border(Border::uniform(Length::px(1.0), &Color::hex("#1976D2")));
    } else {
        style.insert_color_like(PropertyId::BackgroundColor, Color::hex("#FFFFFF"));
        style.set_border(Border::uniform(Length::px(1.0), &Color::hex("#6B7280")));
    }
    style
}
