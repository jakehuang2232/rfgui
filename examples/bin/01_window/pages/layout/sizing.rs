use crate::rfgui::style::{Layout, Length, Padding, Transition, TransitionProperty};
use crate::rfgui::ui::{RsxNode, component, rsx};
use crate::rfgui::view::{Element, Text};
use crate::rfgui_components::use_theme;
use crate::showcase::{Caption, Demo, Sample, SampleRow};

#[component]
pub fn Sizing() -> RsxNode {
    let (theme, _) = use_theme();
    rsx! {
        <Demo
            title="Sizing"
            description="Width and height are fixed lengths, percentages of the parent, or left out to fit the content. Percentages follow the parent as it resizes; with a transition, they ease to the new size."
        >
            <SampleRow>
                <Sample label="100 × 100 px">
                    <Element style={{
                        width: Length::px(100.0),
                        height: Length::px(100.0),
                        background: theme.color.primary.base.clone(),
                    }} />
                </Sample>
                <Sample label="Fits its content">
                    <Element style={{
                        padding: Padding::uniform(theme.spacing.md),
                        background: theme.color.primary.base.clone(),
                        color: theme.color.primary.on.clone(),
                    }}>
                        <Text>Content-sized</Text>
                    </Element>
                </Sample>
            </SampleRow>
            <Caption text="50% and 25% of the card, easing as you resize the window" />
            <Element style={{
                width: Length::percent(100.0),
                layout: Layout::flow().column().no_wrap(),
                gap: theme.spacing.sm,
                padding: Padding::uniform(theme.spacing.sm),
                background: theme.color.layer.raised.clone(),
                border_radius: theme.radius.md,
            }}>
                <Element style={{
                    width: Length::percent(50.0),
                    padding: Padding::uniform(theme.spacing.sm),
                    background: theme.color.primary.base.clone(),
                    color: theme.color.primary.on.clone(),
                    border_radius: theme.radius.sm,
                    transition: [Transition::new(TransitionProperty::Width, theme.motion.duration.slow).ease_in_out()],
                }}>
                    <Text>50%</Text>
                </Element>
                <Element style={{
                    width: Length::percent(25.0),
                    padding: Padding::uniform(theme.spacing.sm),
                    background: theme.color.secondary.base.clone(),
                    color: theme.color.secondary.on.clone(),
                    border_radius: theme.radius.sm,
                    transition: [Transition::new(TransitionProperty::Width, theme.motion.duration.slow).ease_in_out()],
                }}>
                    <Text>25%</Text>
                </Element>
            </Element>
        </Demo>
    }
}
