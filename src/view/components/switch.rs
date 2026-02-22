use crate::style::{
    BorderRadius, Color, Length, ParsedValue, PropertyId, Style, Transition, TransitionProperty,
    Transitions,
};
use crate::ui::Binding;
use crate::view::base_component::{Element, Text};

pub struct SwitchProps {
    pub label: String,
    pub checked: bool,
    pub checked_binding: Option<Binding<bool>>,
    pub disabled: bool,
}

impl SwitchProps {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            checked: false,
            checked_binding: None,
            disabled: false,
        }
    }
}

pub fn build_switch(props: SwitchProps) -> Element {
    build_switch_with_ids(props, 0, 0, 0, 0)
}

pub fn build_switch_with_ids(
    props: SwitchProps,
    root_id: u64,
    track_id: u64,
    thumb_id: u64,
    label_id: u64,
) -> Element {
    let checked = props
        .checked_binding
        .as_ref()
        .map(|v| v.get())
        .unwrap_or(props.checked);

    let mut root = Element::new_with_id(root_id, 0.0, 0.0, 180.0, 34.0);
    root.apply_style(switch_root_style());

    if !props.disabled {
        if let Some(binding) = props.checked_binding.clone() {
            root.on_click(move |_event, _control| {
                binding.set(!binding.get());
            });
        }
    }

    let mut track = Element::new_with_id(track_id, 0.0, 4.0, 44.0, 24.0);
    track.apply_style(switch_track_style(checked, props.disabled));

    let thumb_x = if checked { 22.0 } else { 2.0 };
    let mut thumb = Element::new_with_id(thumb_id, thumb_x, 2.0, 20.0, 20.0);
    thumb.apply_style(switch_thumb_style(props.disabled));
    track.add_child(Box::new(thumb));

    let mut label = Text::from_content_with_id(label_id, props.label);
    label.set_position(56.0, 8.0);
    label.set_font_size(14.0);
    label.set_font("Roboto, Noto Sans CJK TC");
    label.set_color(if props.disabled {
        Color::hex("#9E9E9E")
    } else {
        Color::hex("#1F2937")
    });

    root.add_child(Box::new(track));
    root.add_child(Box::new(label));
    root
}

fn switch_root_style() -> Style {
    let mut style = Style::new();
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(180.0)));
    style.insert(PropertyId::Height, ParsedValue::Length(Length::px(34.0)));
    style
}

fn switch_track_style(checked: bool, disabled: bool) -> Style {
    let mut style = Style::new();
    style.set_border_radius(BorderRadius::uniform(Length::px(12.0)));
    style.insert(
        PropertyId::Transition,
        ParsedValue::Transition(Transitions::single(
            Transition::new(TransitionProperty::BackgroundColor, 1800).ease_in_out(),
        )),
    );
    style.insert_color_like(
        PropertyId::BackgroundColor,
        if disabled {
            Color::hex("#E0E0E0")
        } else if checked {
            Color::hex("#1976D2")
        } else {
            Color::hex("#B0BEC5")
        },
    );
    style
}

fn switch_thumb_style(disabled: bool) -> Style {
    let mut style = Style::new();
    style.set_border_radius(BorderRadius::uniform(Length::px(10.0)));
    style.insert(
        PropertyId::Transition,
        ParsedValue::Transition(Transitions::single(
            Transition::new(TransitionProperty::PositionX, 1800).ease_in_out(),
        )),
    );
    style.insert_color_like(
        PropertyId::BackgroundColor,
        if disabled {
            Color::hex("#EEEEEE")
        } else {
            Color::hex("#FFFFFF")
        },
    );
    style
}
