use crate::rfgui::ui::{RsxNode, component, rsx, use_state};
use crate::rfgui_components::{NumberField, Theme};
use rfgui_components::Accordion;

#[component]
pub fn NumberFieldSection(theme: Theme) -> RsxNode {
    let _ = theme;
    let int_number = use_state(|| 0);
    let float_number = use_state(|| 0.0);

    rsx! {
        <Accordion title="Number Field">
            <NumberField::<i32>
                binding={int_number.binding()}
                min=0
                max=100
                step=1
                label="I32 Number"
            />
            <NumberField::<f64>
                binding={float_number.binding()}
                min=0.0
                max=100.0
                step=0.1
                label="F32 Number"
            />
        </Accordion>
    }
}
