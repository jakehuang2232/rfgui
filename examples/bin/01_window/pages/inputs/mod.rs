mod buttons;
mod number_field;
mod selection;
mod toggle_buttons;

use crate::rfgui::ui::{RsxNode, component, rsx};
use buttons::Buttons;
use number_field::NumberFields;
use selection::SelectionInputs;
use toggle_buttons::ToggleButtons;

#[component]
pub fn InputsPage() -> RsxNode {
    rsx! {
        <Buttons />
        <ToggleButtons />
        <SelectionInputs />
        <NumberFields />
    }
}
