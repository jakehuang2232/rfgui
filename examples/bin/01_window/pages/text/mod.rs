mod text_area;
mod typography;

use crate::rfgui::ui::{RsxNode, component, rsx};
use text_area::TextAreaDemo;
use typography::Typography;

#[component]
pub fn TextPage() -> RsxNode {
    rsx! {
        <Typography />
        <TextAreaDemo />
    }
}
