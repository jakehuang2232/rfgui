use crate::rfgui::style::{Layout, Length};
use crate::rfgui::ui::{RsxNode, component, rsx, use_state};
use crate::rfgui::view::Element;
use crate::rfgui_components::{Checkbox, Select, Slider, Switch, use_theme};
use crate::showcase::{Caption, Demo};

fn select_value_identity(item: &String, _: usize) -> String {
    item.clone()
}

fn select_label(item: &String, _: usize) -> String {
    item.clone()
}

#[component]
pub fn SelectionInputs() -> RsxNode {
    let (theme, _) = use_theme();
    let options = (1..=1000)
        .map(|index| format!("Option {index}"))
        .collect::<Vec<String>>();

    let checked = use_state(|| false);
    let selected = use_state(|| String::from("Option 1"));
    let slider = use_state(|| 25.0_f64);
    let switch_state = use_state(|| true);

    rsx! {
        <Demo
            title="Checkbox, Switch, Select and Slider"
            description="Each control reads and writes a Binding, so the state lives in the component that owns it. The select scrolls through a thousand options."
        >
            <Element style={{
                width: Length::percent(100.0),
                layout: Layout::flow().row().wrap(),
                gap: theme.spacing.lg,
            }}>
                <Checkbox
                    label="Enable flag"
                    binding={checked.binding()}
                />
                <Switch
                    label="Switch state"
                    binding={switch_state.binding()}
                />
            </Element>
            <Element style={{
                width: Length::percent(100.0),
                layout: Layout::flow().column().no_wrap(),
                gap: theme.spacing.sm,
            }}>
                <Element style={{ width: Length::px(240.0) }}>
                    <Select::<String, String>
                        data={options}
                        to_label={select_label as fn(&String, usize) -> String}
                        to_value={select_value_identity as fn(&String, usize) -> String}
                        value={selected.binding()}
                    />
                </Element>
                <Element style={{ width: Length::px(320.0) }}>
                    <Slider
                        binding={slider.binding()}
                        min=0.0
                        max=100.0
                        label="Slider"
                    />
                </Element>
            </Element>
            <Caption text={format!(
                "checked={} switch={} selected={} slider={:.0}",
                checked.get(),
                switch_state.get(),
                selected.get(),
                slider.get(),
            )} />
        </Demo>
    }
}
