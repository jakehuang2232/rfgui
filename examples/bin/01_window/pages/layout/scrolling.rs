use crate::rfgui::style::{Border, Layout, Length, Padding, ScrollDirection};
use crate::rfgui::ui::{RsxNode, component, rsx};
use crate::rfgui::view::{Element, Text};
use crate::rfgui_components::use_theme;
use crate::showcase::{Demo, Sample, SampleRow};

#[component]
pub fn Scrolling() -> RsxNode {
    let (theme, _) = use_theme();
    let rows = (1..=16)
        .map(|index| rsx! { <Text key={index}>{format!("Row {index}")}</Text> })
        .collect::<Vec<_>>();
    let chips = (1..=12)
        .map(|index| {
            rsx! {
                <Element key={index} style={{
                    width: Length::px(64.0),
                    height: Length::px(64.0),
                    background: theme.color.secondary.base.clone(),
                    color: theme.color.secondary.on.clone(),
                    border_radius: theme.radius.md,
                    padding: Padding::uniform(theme.spacing.sm),
                }}>
                    <Text>{format!("{index}")}</Text>
                </Element>
            }
        })
        .collect::<Vec<_>>();

    rsx! {
        <Demo
            title="Scrolling"
            description="A scroll container clips its content and shows a scrollbar that fades in while you scroll. Its thumb can be dragged and its track clicked; wheel scrolling eases between positions."
        >
            <SampleRow>
                <Sample label="Vertical">
                    <Element style={{
                        width: Length::px(220.0),
                        height: Length::px(160.0),
                        background: theme.color.layer.surface.clone(),
                        border: Border::uniform(Length::px(1.0), theme.color.divider.as_ref()),
                        border_radius: theme.radius.md,
                        padding: Padding::uniform(theme.spacing.sm),
                        layout: Layout::flow().column().no_wrap(),
                        gap: theme.spacing.xs,
                        scroll_direction: ScrollDirection::Vertical,
                    }}>
                        {rows}
                    </Element>
                </Sample>
                <Sample label="Horizontal">
                    <Element style={{
                        width: Length::px(300.0),
                        height: Length::px(100.0),
                        background: theme.color.layer.surface.clone(),
                        border: Border::uniform(Length::px(1.0), theme.color.divider.as_ref()),
                        border_radius: theme.radius.md,
                        padding: Padding::uniform(theme.spacing.sm),
                        layout: Layout::flow().row().no_wrap(),
                        gap: theme.spacing.sm,
                        scroll_direction: ScrollDirection::Horizontal,
                    }}>
                        {chips}
                    </Element>
                </Sample>
            </SampleRow>
        </Demo>
    }
}
