use crate::rfgui::style::{Layout, Length};
use crate::rfgui::ui::{RsxNode, component, rsx};
use crate::rfgui::view::{Element, Text};
use crate::rfgui_components::{Accordion, use_theme};
use crate::showcase::Demo;

#[component]
pub fn Accordions() -> RsxNode {
    let (theme, _) = use_theme();
    rsx! {
        <Demo
            title="Accordion"
            description="A titled section that expands and collapses its content, animating its height."
        >
            <Element style={{
                width: Length::percent(100.0),
                layout: Layout::flow().column().no_wrap(),
                gap: theme.spacing.sm,
            }}>
                <Accordion title="Expanded at first" default_expanded={true}>
                    <Text>Content of the first section.</Text>
                </Accordion>
                <Accordion title="Collapsed at first">
                    <Text>Content of the second section, shown once it is expanded.</Text>
                </Accordion>
                <Accordion title="Disabled" disabled={true}>
                    <Text>This content cannot be shown.</Text>
                </Accordion>
            </Element>
        </Demo>
    }
}
