use rfgui::ScrollDirection;
use crate::rfgui::view::{Element, Text};
use crate::rfgui::ui::{Binding, RsxNode, rsx};
use crate::rfgui::{Layout, Length, Padding};
use crate::rfgui_components::{
    Button, ButtonVariant, Checkbox, NumberField, Select, Slider, Switch, Theme,
};
use rfgui::ui::use_state;
use rfgui_components::Accordion;

pub struct ComponentTestBindings {
    pub checked: Binding<bool>,
    pub selected: Binding<String>,
    pub slider: Binding<f64>,
    pub switch_state: Binding<bool>,
}

pub struct ComponentTestValues {
    pub checked: bool,
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
    let options = (1..=1000)
        .map(|index| format!("Option {index}"))
        .collect::<Vec<String>>();

    let count = use_state(|| 0);
    let count_for_increment = count.clone();
    let count_increment = move |_event: &mut crate::rfgui::ui::ClickEvent| {
        count_for_increment.update(|value| *value += 1)
    };

    let int_number = use_state(|| 0);
    let float_number = use_state(|| 0.0);

    rsx! {
        <Element style={{
            width: Length::percent(100.0),
            height: Length::percent(100.0),
            layout: Layout::flow().column().no_wrap(),
            gap: theme.spacing.sm,
            padding: Padding::uniform(theme.spacing.md),
            color: theme.color.text.primary.clone(),
            font_size: theme.typography.size.sm,
            scroll_direction: ScrollDirection::Vertical,
        }}>
            <Accordion title="Button">
                <Element style={{
                    width: Length::percent(100.0),
                    layout: Layout::flow().row().wrap(),
                    gap: theme.spacing.sm,
                }}>
                    <Button
                        label="Contained"
                        variant={Some(ButtonVariant::Contained)}
                    />
                    <Button
                        label="Outlined"
                        variant={Some(ButtonVariant::Outlined)}
                    />
                    <Button
                        label="Text"
                        variant={Some(ButtonVariant::Text)}
                    />
                </Element>
                <Element style={{
                            width: Length::percent(100.0),
                            layout: Layout::flow().row().wrap(),
                            gap: theme.spacing.sm,
                        }}>
                    <Button
                        label="Contained"
                        variant={Some(ButtonVariant::Contained)}
                        disabled
                    />
                    <Button
                        label="Outlined"
                        variant={Some(ButtonVariant::Outlined)}
                        disabled
                    />
                    <Button
                        label="Text"
                        variant={Some(ButtonVariant::Text)}
                        disabled
                    />
                </Element>
                <Element style={{
                    width: Length::percent(100.0),
                    layout: Layout::flow().row(),
                    gap: theme.spacing.sm,
                }}>
                    <Button
                        label="Click Me"
                        variant={Some(ButtonVariant::Contained)}
                        on_click={count_increment.clone()}
                    />
                    <Button
                        label="Hold to Repeat"
                        variant={Some(ButtonVariant::Contained)}
                        repeat
                        on_click={count_increment.clone()}
                    />
                    <Text>{format!("Count: {}", count.get())}</Text>
                </Element>
            </Accordion>
            <Accordion title="Number Field">
                 <NumberField
                    binding={int_number.binding()}
                    min=0
                    max=100
                    step=1
                    label="I32 Number"
                />
                <NumberField 
                    binding={float_number.binding()}
                    min=0.0
                    max=100.0
                    step=0.1
                    label="F32 Number"
                />
            </Accordion>
            <Checkbox
                label="Enable flag"
                binding={bindings.checked}
            />
            <Switch
                label="Switch state"
                binding={bindings.switch_state}
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
                    "checked={} selected={} slider={:.0} switch={}",
                    values.checked,
                    values.selected,
                    values.slider,
                    values.switch_state
                )}
            </Text>
        </Element>
    }
}
