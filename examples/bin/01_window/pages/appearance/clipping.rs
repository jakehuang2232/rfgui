use crate::rfgui::style::{Border, Layout, Length, Padding};
use crate::rfgui::ui::{RsxNode, component, rsx};
use crate::rfgui::view::{Element, Text};
use crate::rfgui_components::use_theme;
use crate::showcase::{Demo, Sample, SampleRow};

#[component]
pub fn RoundedClipping() -> RsxNode {
    let (theme, _) = use_theme();
    rsx! {
        <Demo
            title="Rounded clipping"
            description="Children that overflow are clipped to the parent's rounded corners and border, including the corners of a circle."
        >
            <SampleRow>
                <Sample label="Children of a circle">
                    <Element style={{
                        width: Length::px(150.0),
                        height: Length::px(150.0),
                        background: theme.color.primary.base.clone(),
                        border: Border::uniform(theme.spacing.sm, theme.color.border.as_ref()),
                        border_radius: 75,
                        layout: Layout::flow().row().wrap(),
                        gap: theme.spacing.sm,
                        padding: Padding::uniform(theme.spacing.sm),
                    }}>
                        <Element style={{
                            width: Length::px(72.0),
                            height: Length::px(48.0),
                            background: "#d19a66",
                            border: Border::uniform(theme.spacing.sm, theme.color.state.active.as_ref())
                        }}>
                            <Text>Clipped</Text>
                        </Element>
                        <Element style={{ width: Length::px(56.0), height: Length::px(56.0), background: "#61ef9c" }} />
                        <Element style={{ width: Length::px(120.0), height: Length::px(64.0), background: "#c678dd", border: Border::uniform(Length::px(4.0), theme.color.state.focus.as_ref()) }} />
                    </Element>
                </Sample>
            </SampleRow>
        </Demo>
    }
}
