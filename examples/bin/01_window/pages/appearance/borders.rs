use crate::rfgui::style::{Border, BorderRadius, Color, Gradient, Length, Padding, SideOrCorner};
use crate::rfgui::ui::{RsxNode, component, rsx};
use crate::rfgui::view::{Element, Text};
use crate::rfgui_components::use_theme;
use crate::showcase::{Demo, Sample, SampleRow};

#[component]
pub fn BackgroundsAndBorders() -> RsxNode {
    let (theme, _) = use_theme();
    rsx! {
        <Demo
            title="Backgrounds and borders"
            description="Solid colors and gradients, borders set per side, and a radius per corner in pixels or percent."
        >
            <SampleRow>
                <Sample label="Radius per corner">
                    <Element style={{
                        width: Length::px(150.0),
                        height: Length::px(150.0),
                        background: theme.color.primary.base.clone(),
                        border_radius: BorderRadius::uniform(theme.radius.md)
                            .top_right(Length::px(32.0))
                            .bottom_left(Length::percent(90.0)),
                    }} />
                </Sample>
                <Sample label="Gradient, borders per side, shadow">
                    <Element style={{
                        width: Length::px(150.0),
                        height: Length::px(150.0),
                        background: Gradient::rainbow(SideOrCorner::Right),
                        border: Border::uniform(Length::px(10.0), &Color::hex("#274f8b"))
                            .top(Some(Length::px(20.0)), Some(&Color::hex("#e06c75")))
                            .left(Some(Length::px(15.0)), Some(&Color::hex("#2db353"))),
                        border_radius: BorderRadius::uniform(Length::px(10.0))
                            .bottom_left(Length::percent(90.0)),
                        box_shadow: vec![theme.shadow.level_3],
                    }} />
                </Sample>
                <Sample label="Thick border, content-sized height">
                    <Element style={{
                        width: Length::px(150.0),
                        padding: Padding::uniform(theme.spacing.sm),
                        background: theme.color.primary.base.clone(),
                        color: theme.color.primary.on.clone(),
                        border: Border::uniform(theme.spacing.md, theme.color.border.as_ref()),
                    }}>
                        <Text>The height follows the text inside the border.</Text>
                    </Element>
                </Sample>
            </SampleRow>
        </Demo>
    }
}
