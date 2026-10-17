use crate::rfgui::style::{Layout, Length};
use crate::rfgui::ui::{RsxNode, component, rsx};
use crate::rfgui::view::Element;
use crate::rfgui_components::{Alert, AlertSeverity, AlertVariant, Button, ButtonSize, use_theme};
use crate::showcase::{Caption, Demo};

#[component]
pub fn Alerts() -> RsxNode {
    let (theme, _) = use_theme();
    let column = |variant: AlertVariant| {
        rsx! {
            <Element style={{
                width: Length::percent(100.0),
                layout: Layout::flow().column().no_wrap(),
                gap: theme.spacing.sm,
            }}>
                <Alert severity={AlertSeverity::Info} variant={variant}>
                    A new version is available.
                </Alert>
                <Alert severity={AlertSeverity::Success} variant={variant}>
                    The file was saved.
                </Alert>
                <Alert severity={AlertSeverity::Warning} variant={variant}>
                    The connection is unstable.
                </Alert>
                <Alert
                    severity={AlertSeverity::Error}
                    variant={variant}
                    action={rsx! { <Button size={Some(ButtonSize::Small)}>Retry</Button> }}
                >
                    The upload failed.
                </Alert>
            </Element>
        }
    };
    rsx! {
        <Demo
            title="Alert"
            description="Four severities in three variants. An alert can hold an action, and a close button when given on_close."
        >
            <Caption text="Standard" />
            {column(AlertVariant::Standard)}
            <Caption text="Filled" />
            {column(AlertVariant::Filled)}
            <Caption text="Outlined" />
            {column(AlertVariant::Outlined)}
        </Demo>
    }
}
