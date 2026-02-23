use crate::style::{
    AlignItems, BorderRadius, Color, Display, FlowDirection, Length, Padding, ParsedValue,
    PropertyId, Style, Transition, TransitionProperty, Transitions,
};
use crate::ui::host::{Element, Text};
use crate::ui::{Binding, RsxNode, on_click, rsx};

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

pub fn build_switch_rsx(props: SwitchProps) -> RsxNode {
    let checked = props
        .checked_binding
        .as_ref()
        .map(|v| v.get())
        .unwrap_or(props.checked);

    let binding = props.checked_binding.clone();
    let disabled = props.disabled;
    let click = on_click(move |_event| {
        if disabled {
            return;
        }
        if let Some(binding) = &binding {
            binding.set(!binding.get());
        }
    });

    let label_color = if props.disabled { "#9E9E9E" } else { "#1F2937" };

    rsx! {
        <Element style={switch_root_style()} on_click={click}>
            <Element style={switch_track_style(checked, props.disabled)}>
                <Element style={switch_spacer_style(checked)} />
                <Element style={switch_thumb_style(props.disabled)} />
            </Element>
            <Text x=56 y=8 font_size=14 font="Heiti TC, Noto Sans CJK TC, Roboto" color={label_color}>
                {props.label}
            </Text>
        </Element>
    }
}

fn switch_root_style() -> Style {
    let mut style = Style::new();
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(180.0)));
    style.insert(PropertyId::Height, ParsedValue::Length(Length::px(34.0)));
    style
}

fn switch_track_style(checked: bool, disabled: bool) -> Style {
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
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(44.0)));
    style.insert(PropertyId::Height, ParsedValue::Length(Length::px(24.0)));
    style.set_padding(Padding::uniform(Length::px(2.0)));
    style.set_border_radius(BorderRadius::uniform(Length::px(12.0)));
    style.insert(
        PropertyId::Transition,
        ParsedValue::Transition(Transitions::single(
            Transition::new(TransitionProperty::BackgroundColor, 180).ease_in_out(),
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

fn switch_spacer_style(checked: bool) -> Style {
    let mut style = Style::new();
    style.insert(
        PropertyId::Width,
        ParsedValue::Length(Length::px(if checked { 20.0 } else { 0.0 })),
    );
    style.insert(PropertyId::Height, ParsedValue::Length(Length::px(20.0)));
    style.insert(
        PropertyId::Transition,
        ParsedValue::Transition(Transitions::single(
            Transition::new(TransitionProperty::Width, 180).ease_in_out(),
        )),
    );
    style
}

fn switch_thumb_style(disabled: bool) -> Style {
    let mut style = Style::new();
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(20.0)));
    style.insert(PropertyId::Height, ParsedValue::Length(Length::px(20.0)));
    style.set_border_radius(BorderRadius::uniform(Length::px(10.0)));
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
