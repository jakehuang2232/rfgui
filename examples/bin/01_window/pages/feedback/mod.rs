mod alerts;
mod snackbar;
mod tooltip;

use crate::rfgui::ui::{RsxNode, component, rsx};
use alerts::Alerts;
use snackbar::Snackbars;
use tooltip::Tooltips;

#[component]
pub fn FeedbackPage() -> RsxNode {
    rsx! {
        <Tooltips />
        <Alerts />
        <Snackbars />
    }
}
