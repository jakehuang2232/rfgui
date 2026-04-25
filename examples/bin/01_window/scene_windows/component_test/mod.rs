mod button_section;
mod material_symbols_section;
mod number_field_section;
mod tooltip_section;
mod tree_view_section;

use crate::rfgui::ui::{RsxNode, component, rsx, use_state};
use crate::rfgui::view::{Element, Text};
use crate::rfgui::{Layout, Length, Padding};
use crate::rfgui_components::{Checkbox, Select, Slider, Switch, Theme};

use button_section::ButtonSection;
use material_symbols_section::MaterialSymbolsSection;
use number_field_section::NumberFieldSection;
use tooltip_section::TooltipSection;
use tree_view_section::TreeViewSection;

fn select_value_identity(item: &String, _: usize) -> String {
    item.clone()
}

fn select_label(item: &String, _: usize) -> String {
    item.clone()
}

#[component]
pub fn ComponentTest(theme: Theme) -> RsxNode {
    let options = (1..=1000)
        .map(|index| format!("Option {index}"))
        .collect::<Vec<String>>();

    let checked = use_state(|| false);
    let selected = use_state(|| String::from("Option A"));
    let slider = use_state(|| 25.0_f64);
    let switch_state = use_state(|| true);

    rsx! {
        <Element style={{
            width: Length::percent(100.0),
            layout: Layout::flow().column().no_wrap(),
            gap: theme.spacing.sm,
            padding: Padding::uniform(theme.spacing.md),
            color: theme.color.text.primary.clone(),
            font_size: theme.typography.size.sm,
        }}>
            <ButtonSection theme={theme.clone()} />
            <TooltipSection theme={theme.clone()} />
            <NumberFieldSection theme={theme.clone()} />
            <MaterialSymbolsSection theme={theme.clone()} />
            <TreeViewSection theme={theme.clone()} />
            <Checkbox
                label="Enable flag"
                binding={checked.binding()}
            />
            <Switch
                label="Switch state"
                binding={switch_state.binding()}
            />

            <Select::<String, String>
                data={options}
                to_label={select_label as fn(&String, usize) -> String}
                to_value={select_value_identity as fn(&String, usize) -> String}
                value={selected.binding()}
            />
            <Slider
                binding={slider.binding()}
                min=0.0
                max=100.0
                label="Slider"
            />
            <Text>
                {format!(
                    "checked={} selected={} slider={:.0} switch={}",
                    checked.get(),
                    selected.get(),
                    slider.get(),
                    switch_state.get()
                )}
            </Text>
        </Element>
    }
}
