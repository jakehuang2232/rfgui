use crate::rfgui::style::{Layout, Length};
use crate::rfgui::ui::{RsxNode, component, rsx, use_state};
use crate::rfgui::view::Element;
use crate::rfgui_components::{NumberField, use_theme};
use crate::showcase::Demo;

#[component]
pub fn NumberFields() -> RsxNode {
    let (theme, _) = use_theme();
    let int_number = use_state(|| 0);
    let float_number = use_state(|| 0.0);

    rsx! {
        <Demo
            title="NumberField"
            description="A typed numeric field with a range and a step, for integers or floats. Type a value, or hold the minus and plus buttons to step it."
        >
            <Element style={{
                width: Length::px(320.0),
                layout: Layout::flow().column().no_wrap(),
                gap: theme.spacing.sm,
            }}>
                <NumberField::<i32>
                    binding={int_number.binding()}
                    min=0
                    max=100
                    step=1
                    label="i32"
                />
                <NumberField::<f64>
                    binding={float_number.binding()}
                    min=0.0
                    max=100.0
                    step=0.1
                    label="f64"
                />
            </Element>
        </Demo>
    }
}
