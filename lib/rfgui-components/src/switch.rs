use rfgui::{
    AlignItems, BorderRadius, Color, Display, Length, Padding, ParsedValue, PropertyId, Style,
    Transition, TransitionProperty, Transitions,
};
use rfgui::ui::host::{Element, Text};
use rfgui::ui::{Binding, RsxNode, component, on_click, rsx, use_state};

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
    let has_binding = props.checked_binding.is_some();
    let binding = props
        .checked_binding
        .unwrap_or_else(|| Binding::new(props.checked));
    rsx! {
        <SwitchComponent
            label={props.label}
            checked={props.checked}
            has_binding={has_binding}
            binding={binding}
            disabled={props.disabled}
        />
    }
}

#[component]
fn SwitchComponent(
    label: String,
    checked: bool,
    has_binding: bool,
    binding: Binding<bool>,
    disabled: bool,
) -> RsxNode {
    let fallback_checked = use_state(|| checked);
    let checked_binding = if has_binding {
        binding
    } else {
        fallback_checked.binding()
    };
    let checked = checked_binding.get();

    let click = on_click(move |_event| {
        if disabled {
            return;
        }
        checked_binding.set(!checked_binding.get());
    });

    let label_color = if disabled { "#9E9E9E" } else { "#1F2937" };

    rsx! {
        <Element style={switch_root_style()} on_click={click}>
            <Element style={switch_track_style(checked, disabled)}>
                <Element style={switch_spacer_style(checked)} />
                <Element style={switch_thumb_style(disabled)} />
            </Element>
            <Text font_size=14 font="Heiti TC, Noto Sans CJK TC, Roboto" color={label_color}>
                {label}
            </Text>
        </Element>
    }
}

fn switch_root_style() -> Style {
    let mut style = Style::new();
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(180.0)));
    style.insert(PropertyId::Height, ParsedValue::Length(Length::px(34.0)));
    style.insert(
        PropertyId::Display,
        ParsedValue::Display(Display::flow().row().no_wrap()),
    );
    style.insert(
        PropertyId::AlignItems,
        ParsedValue::AlignItems(AlignItems::Center),
    );
    style.insert(PropertyId::Gap, ParsedValue::Length(Length::px(12.0)));
    style
}

fn switch_track_style(checked: bool, disabled: bool) -> Style {
    let mut style = Style::new();
    style.insert(
        PropertyId::Display,
        ParsedValue::Display(Display::flow().row().no_wrap()),
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
