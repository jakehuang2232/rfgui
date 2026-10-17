use crate::rfgui::style::{Border, Layout, Length};
use crate::rfgui::ui::{RsxNode, component, rsx};
use crate::rfgui::view::{Element, Text};
use crate::rfgui_components::use_theme;
use crate::showcase::{Demo, Sample, SampleRow};

#[component]
pub fn OpacityNesting() -> RsxNode {
    let (theme, _) = use_theme();
    rsx! {
        <Demo
            title="Opacity"
            description="Opacity applies to an element and everything inside it as one group, so nested opacities multiply instead of showing through each other."
        >
            <SampleRow>
                <Sample label="0.5 around 0.6 around 0.5">
                    <Element style={{
                        width: Length::px(150.0),
                        height: Length::px(130.0),
                        background: theme.color.primary.base.clone(),
                        border: Border::uniform(Length::px(3.0), theme.color.border.as_ref()),
                        border_radius: theme.radius.lg,
                        opacity: 0.5,
                        layout: Layout::flow().column(),
                        color: theme.color.primary.on.clone(),
                    }}>
                        <Element style={{ width: Length::px(110.0), height: Length::px(86.0), background: "#e06c75", border: Border::uniform(Length::px(2.0), theme.color.border.as_ref()), border_radius: theme.radius.lg, opacity: 0.6 }}>
                            <Element style={{ width: Length::px(72.0), height: Length::px(52.0), background: "#61afef", border: Border::uniform(Length::px(2.0), theme.color.border.as_ref()), border_radius: theme.radius.md, opacity: 0.5 }}>
                                <Text style={{ opacity: 0.9 }}>Alpha</Text>
                            </Element>
                        </Element>
                    </Element>
                </Sample>
                <Sample label="Only the inner box at 0.5">
                    <Element style={{
                        width: Length::px(150.0),
                        height: Length::px(130.0),
                        background: theme.color.primary.base.clone(),
                        border: Border::uniform(Length::px(3.0), theme.color.border.as_ref()),
                        border_radius: theme.radius.lg,
                        layout: Layout::flow().column(),
                        color: theme.color.primary.on.clone(),
                    }}>
                        <Element style={{ width: Length::px(110.0), height: Length::px(86.0), background: "#e06c75", border: Border::uniform(Length::px(2.0), theme.color.border.as_ref()), border_radius: theme.radius.lg, opacity: 1 }}>
                            <Element style={{ width: Length::px(72.0), height: Length::px(52.0), background: "#61afef", border: Border::uniform(Length::px(2.0), theme.color.border.as_ref()), border_radius: theme.radius.md, opacity: 0.5 }}>
                                <Text style={{ opacity: 0.9 }}>Alpha</Text>
                            </Element>
                        </Element>
                    </Element>
                </Sample>
            </SampleRow>
        </Demo>
    }
}
