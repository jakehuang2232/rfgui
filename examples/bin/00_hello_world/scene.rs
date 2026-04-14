use crate::rfgui::ui::{RsxNode, component, rsx};
use rfgui::{Align, Element, JustifyContent, Layout, Length};

#[component]
pub fn MainScene() -> RsxNode {
    rsx! {
        <Element
            style={{
                width: Length::percent(100.0),
                height: Length::percent(100.0),
                layout: Layout::flex().align(Align::Center).justify_content(JustifyContent::Center),
                font_size: 100.0,
                color: "#fff",
            }}>
            Hello, world!
        </Element>
    }
}
