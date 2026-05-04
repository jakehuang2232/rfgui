use crate::rfgui::style::{Layout, Length};
use crate::rfgui::ui::{RsxNode, component, rsx, use_state};
use crate::rfgui::view::{Element, Text};
use crate::rfgui_components::{
    Alert, AlertSeverity, Button, ButtonColor, Snackbar, SnackbarHorizontal, SnackbarOrigin,
    SnackbarVertical, Theme, snackbar_close_binding,
};
use rfgui_components::Accordion;
use std::time::Duration;

#[component]
pub fn SnackbarSection(theme: Theme) -> RsxNode {
    let open = use_state(|| false);
    let severity = use_state(|| AlertSeverity::Info);
    let message = use_state(|| String::new());

    let make_show = |sev: AlertSeverity, text: &'static str| {
        let open = open.clone();
        let severity = severity.clone();
        let message = message.clone();
        move |_: &mut crate::rfgui::ui::ClickEvent| {
            severity.set(sev);
            message.set(text.to_string());
            open.set(true);
        }
    };

    let show_info = make_show(AlertSeverity::Info, "Draft saved");
    let show_success = make_show(AlertSeverity::Success, "Operation succeeded");
    let show_warning = make_show(AlertSeverity::Warning, "Connection unstable");
    let show_error = make_show(AlertSeverity::Error, "Save failed");

    let on_close = snackbar_close_binding(open.binding());
    let alert_close = {
        let open = open.clone();
        std::rc::Rc::new(move || open.set(false)) as std::rc::Rc<dyn Fn()>
    };

    rsx! {
        <Accordion title="Snackbar">
            <Text style={{ color: theme.color.text.secondary.clone() }}>
                "Trigger snackbar with severity (auto-dismiss 4s, click X to close)"
            </Text>
            <Element style={{
                width: Length::percent(100.0),
                layout: Layout::flow().row().wrap().align(rfgui::style::Align::Center),
                gap: theme.spacing.lg,
                padding: rfgui::style::Padding::uniform(theme.spacing.md),
            }}>
                <Button on_click={show_info}>Info</Button>
                <Button
                    color={Some(ButtonColor::Success)}
                    on_click={show_success}
                >Success</Button>
                <Button
                    color={Some(ButtonColor::Warning)}
                    on_click={show_warning}
                >Warning</Button>
                <Button
                    color={Some(ButtonColor::Error)}
                    on_click={show_error}
                >Error</Button>
            </Element>

            <Snackbar
                open={open.get()}
                auto_hide_duration={Some(Duration::from_secs(4))}
                anchor_origin={Some(SnackbarOrigin::new(
                    SnackbarVertical::Bottom,
                    SnackbarHorizontal::Left,
                ))}
                on_close={Some(on_close)}
            >
                <Alert severity={severity.get()} on_close={Some(alert_close)}>
                    {message.get()}
                </Alert>
            </Snackbar>
        </Accordion>
    }
}
