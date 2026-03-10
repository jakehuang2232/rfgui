use crate::rfgui::ui::host::{Element, Text};
use crate::rfgui::ui::{Binding, RsxNode, rsx};
use crate::rfgui::{Layout, Length, Padding};
use crate::rfgui_components::{
    Button, ButtonVariant, Checkbox, NumberField, Select, Slider, Switch, Theme,
};

pub struct ComponentTestBindings {
    pub count: Binding<i32>,
    pub checked: Binding<bool>,
    pub number: Binding<f64>,
    pub selected: Binding<String>,
    pub slider: Binding<f64>,
    pub switch_state: Binding<bool>,
}

pub struct ComponentTestValues {
    pub count: i32,
    pub checked: bool,
    pub number: f64,
    pub selected: String,
    pub slider: f64,
    pub switch_state: bool,
}

fn select_label(item: &String, _: usize) -> String {
    item.clone()
}

pub fn build(
    theme: &Theme,
    bindings: ComponentTestBindings,
    values: ComponentTestValues,
) -> RsxNode {
    let count_increment = bindings.count.clone();
    let count_reset = bindings.count.clone();
    let options = (1..=1000)
        .map(|index| format!("Option {index}"))
        .collect::<Vec<String>>();

    rsx! {
        <Element style={{
            width: Length::percent(100.0),
            height: Length::percent(100.0),
            layout: Layout::flow().column().no_wrap(),
            gap: theme.spacing.md,
            padding: Padding::uniform(theme.spacing.md),
            color: theme.color.text.secondary.clone(),
            font_size: theme.typography.size.sm,
        }}>
            <Element style={{
                width: Length::percent(100.0),
                layout: Layout::flow().row().wrap(),
                gap: theme.spacing.sm,
            }}>
                <Button
                    label="Count +1"
                    variant={Some(ButtonVariant::Contained)}
                    on_click={move |_| { count_increment.update(|value| *value += 1); }}
                />
                <Button
                    label="Count Reset"
                    variant={Some(ButtonVariant::Outlined)}
                    on_click={move |_| { count_reset.set(0); }}
                />
                <Button
                    label="Nothing"
                    variant={Some(ButtonVariant::Text)}
                />
            </Element>
            <Checkbox
                label="Enable flag"
                binding={bindings.checked}
            />
            <Switch
                label="Switch state"
                binding={bindings.switch_state}
            />
            <NumberField
                binding={bindings.number}
                min=0.0
                max=100.0
                step=1.0
            />
            <Select
                data={options}
                to_label={select_label}
                to_value={|item, _| item.clone()}
                value={bindings.selected}
            />
            <Slider
                binding={bindings.slider}
                min=0.0
                max=100.0
            />
            <Text>
                {format!(
                    "count={} checked={} number={:.0} selected={} slider={:.0} switch={}",
                    values.count,
                    values.checked,
                    values.number,
                    values.selected,
                    values.slider,
                    values.switch_state
                )}
            </Text>
        </Element>
    }
}
