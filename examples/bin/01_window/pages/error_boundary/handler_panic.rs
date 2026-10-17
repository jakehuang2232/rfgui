use crate::rfgui::style::{Layout, Length};
use crate::rfgui::ui::{ClickEvent, RsxNode, component, rsx, use_state};
use crate::rfgui::view::Element;
use crate::rfgui_components::{Button, ButtonColor, use_theme};
use crate::showcase::Demo;

#[component]
pub fn HandlerPanic() -> RsxNode {
    let (theme, _) = use_theme();
    let clicks = use_state(|| 0_u32);
    let panic_in_handler = |_: &mut ClickEvent| panic!("this click handler panicked");
    let count = {
        let clicks = clicks.clone();
        move |_: &mut ClickEvent| clicks.update(|count| *count += 1)
    };
    rsx! {
        <Demo
            title="A panic in an event handler"
            description="A panic in a handler, a timer or a state updater cuts that callback short and is reported in the terminal; no boundary is involved, and the rest of the app does not notice."
        >
            <Element style={{
                width: Length::percent(100.0),
                layout: Layout::flow().row().wrap(),
                gap: theme.spacing.md,
            }}>
                <Button color={Some(ButtonColor::Error)} on_click={panic_in_handler}>Panic on click</Button>
                <Button on_click={count}>{format!("Still responsive: {} clicks", clicks.get())}</Button>
            </Element>
        </Demo>
    }
}
