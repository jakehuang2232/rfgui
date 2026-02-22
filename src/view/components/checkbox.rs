use crate::style::{
    AlignItems, Border, BorderRadius, Color, Display, FlowDirection, Length, Padding, ParsedValue,
    PropertyId, Style,
};
use crate::ui::Binding;
use crate::view::base_component::{Element, Text};

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

pub fn build_checkbox(props: CheckboxProps) -> Element {
    let checked = props
        .checked_binding
        .as_ref()
        .map(|v| v.get())
        .unwrap_or(props.checked);

    let mut root = Element::new(0.0, 0.0, props.width, props.height);
    root.apply_style(checkbox_row_style(props.width, props.height));

    if !props.disabled {
        if let Some(binding) = props.checked_binding.clone() {
            root.on_click(move |_event, _control| {
                binding.set(!binding.get());
            });
        }
    }

    let mut box_el = Element::new(0.0, 0.0, 18.0, 18.0);
    box_el.apply_style(checkbox_box_style(checked, props.disabled));
    if checked {
        let mut check = Text::from_content("✓");
        check.set_position(2.0, -1.0);
        check.set_font_size(16.0);
        check.set_font("Roboto, Noto Sans CJK TC");
        check.set_color(if props.disabled {
            Color::hex("#9E9E9E")
        } else {
            Color::hex("#FFFFFF")
        });
        box_el.add_child(Box::new(check));
    }

    let mut label = Text::from_content(props.label);
    label.set_position(28.0, 7.0);
    label.set_font("Roboto, Noto Sans CJK TC");
    label.set_font_size(14.0);
    label.set_color(if props.disabled {
        Color::hex("#9E9E9E")
    } else {
        Color::hex("#1F2937")
    });

    root.add_child(Box::new(box_el));
    root.add_child(Box::new(label));
    root
}

fn checkbox_row_style(width: f32, height: f32) -> Style {
    let mut style = Style::new();
    style.insert(PropertyId::Display, ParsedValue::Display(Display::Flow));
    style.insert(
        PropertyId::FlowDirection,
        ParsedValue::FlowDirection(FlowDirection::Row),
    );
    style.insert(
        PropertyId::AlignItems,
        ParsedValue::AlignItems(AlignItems::Center),
    );
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

