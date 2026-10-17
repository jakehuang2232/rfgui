mod global_key;

use crate::rfgui::ui::{RsxNode, component, rsx};
use global_key::GlobalKeyMove;

#[component]
pub fn StatePage() -> RsxNode {
    rsx! { <GlobalKeyMove /> }
}
