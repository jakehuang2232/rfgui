mod accordion;
mod tree_view;
mod windows;

use crate::rfgui::ui::{RsxNode, component, rsx};
use accordion::Accordions;
use tree_view::TreeViewDemo;
use windows::Windows;

#[component]
pub fn ContainersPage() -> RsxNode {
    rsx! {
        <Windows />
        <Accordions />
        <TreeViewDemo />
    }
}
