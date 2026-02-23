use crate::style::{Border, BorderRadius, Color, Length, ParsedValue, PropertyId, Style};
use crate::ui::Binding;
use crate::view::base_component::{Element, Text};

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

pub fn build_select(props: SelectProps) -> Element {
    build_select_with_ids(props, 0, 0, 0)
}

pub fn build_select_with_ids(
    props: SelectProps,
    root_id: u64,
    text_id: u64,
    icon_id: u64,
) -> Element {
    let selected_index = props
        .selected_binding
        .as_ref()
        .map(|v| v.get())
        .unwrap_or(props.selected_index);
    let option_text = resolve_option_text(&props.options, selected_index);

    let mut root = Element::new_with_id(root_id, 0.0, 0.0, props.width, props.height);
    root.apply_style(select_style(props.width, props.height, props.disabled));

    if !props.disabled {
        if let Some(binding) = props.selected_binding.clone() {
            let len = props.options.len();
            root.on_click(move |_event, _control| {
                if len == 0 {
                    return;
                }
                binding.set((binding.get() + 1) % len);
            });
        }
    }

    let mut text = Text::from_content_with_id(text_id, option_text);
    text.set_position(12.0, props.height * 0.5 - 8.0);
    text.set_font_size(14.0);
    text.set_font("Heiti TC, Noto Sans CJK TC, Roboto");
    text.set_color(if props.disabled {
        Color::hex("#9E9E9E")
    } else {
        Color::hex("#111827")
    });

    let mut icon = Text::from_content_with_id(icon_id, "▾");
    icon.set_position((props.width - 20.0).max(0.0), props.height * 0.5 - 8.0);
    icon.set_font_size(14.0);
    icon.set_font("Heiti TC, Noto Sans CJK TC, Roboto");
    icon.set_color(if props.disabled {
        Color::hex("#BDBDBD")
    } else {
        Color::hex("#6B7280")
    });

    root.add_child(Box::new(text));
    root.add_child(Box::new(icon));
    root
}

fn select_style(width: f32, height: f32, disabled: bool) -> Style {
    let mut style = Style::new();
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
    let mut hover = Style::new();
    hover.insert_color_like(PropertyId::BackgroundColor, Color::hex("#FAFAFA"));
    style.set_hover(hover);
    style
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
