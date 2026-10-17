use crate::rfgui::style::{Border, Layout, Length, Padding, Transition, TransitionProperty};
use crate::rfgui::ui::{ClickEvent, RsxNode, component, rsx, use_state};
use crate::rfgui::view::{Element, Text};
use crate::rfgui_components::{Button, ButtonSize, use_theme};
use crate::showcase::{Demo, Sample, SampleRow};

const SHORT: &str = "A short note.";
const LONG: &str = "A longer note that needs several lines, so the box grows taller. Its height eases to the new size after a one second delay.";

#[component]
pub fn StyleTransitions() -> RsxNode {
    let (theme, _) = use_theme();
    let long = use_state(|| false);
    let spacious = use_state(|| false);
    let toggle_long = {
        let long = long.clone();
        move |_: &mut ClickEvent| long.update(|value| *value = !*value)
    };
    let toggle_spacious = {
        let spacious = spacious.clone();
        move |_: &mut ClickEvent| spacious.update(|value| *value = !*value)
    };
    let spacing = if spacious.get() {
        (theme.spacing.xl, Length::px(6.0), theme.spacing.lg)
    } else {
        (theme.spacing.sm, Length::px(1.0), theme.spacing.xs)
    };

    rsx! {
        <Demo
            title="Hover and style transitions"
            description="Hover styles, and any style that changes, ease from the old value to the new one: colors, sizes from content, padding, border widths and gaps."
        >
            <SampleRow>
                <Sample label="Nested hover">
                    <Element style={{
                        width: Length::px(170.0),
                        height: Length::px(170.0),
                        background: theme.color.secondary.base.clone(),
                        border: Border::uniform(theme.spacing.xl, theme.color.divider.as_ref()),
                        border_radius: theme.radius.lg,
                        hover: {
                            border: Border::uniform(theme.spacing.xl, theme.color.primary.base.as_ref()),
                        },
                        transition: [
                            Transition::new(TransitionProperty::BorderColor, theme.motion.duration.slow).ease_in_out(),
                        ],
                    }}>
                        <Element style={{
                            width: Length::percent(100.0),
                            height: Length::percent(100.0),
                            padding: Padding::uniform(theme.spacing.xs),
                            background: theme.color.primary.base.clone(),
                            color: theme.color.primary.on.clone(),
                            hover: {
                                background: theme.color.primary.on.clone(),
                                color: theme.color.primary.base.clone(),
                            },
                            transition: [
                                Transition::new(TransitionProperty::All, theme.motion.duration.fast),
                            ],
                        }}>
                            <Text>Hover the frame, then the center</Text>
                        </Element>
                    </Element>
                </Sample>
                <Sample label="Height from content, delayed">
                    <Button size={Some(ButtonSize::Small)} on_click={toggle_long}>
                        {if long.get() { "Shorter" } else { "Longer" }}
                    </Button>
                    <Element style={{
                        width: Length::px(200.0),
                        padding: Padding::uniform(theme.spacing.sm),
                        background: theme.color.primary.base.clone(),
                        color: theme.color.primary.on.clone(),
                        border_radius: theme.radius.sm,
                        transition: [
                            Transition::new(TransitionProperty::Height, theme.motion.duration.slow)
                                .ease_in_out()
                                .delay(1000),
                        ],
                    }}>
                        <Text>{if long.get() { LONG } else { SHORT }}</Text>
                    </Element>
                </Sample>
                <Sample label="Padding, border width and gap">
                    <Button size={Some(ButtonSize::Small)} on_click={toggle_spacious}>
                        {if spacious.get() { "Compact" } else { "Spacious" }}
                    </Button>
                    <Element style={{
                        layout: Layout::flow().row().no_wrap(),
                        padding: Padding::uniform(spacing.0),
                        border: Border::uniform(spacing.1, theme.color.primary.base.as_ref()),
                        gap: spacing.2,
                        border_radius: theme.radius.md,
                        transition: [
                            Transition::new(TransitionProperty::Padding, theme.motion.duration.slow).ease_in_out(),
                            Transition::new(TransitionProperty::BorderWidth, theme.motion.duration.slow).ease_in_out(),
                            Transition::new(TransitionProperty::Gap, theme.motion.duration.slow).ease_in_out(),
                        ],
                    }}>
                        <Element style={{ width: Length::px(32.0), height: Length::px(32.0), background: theme.color.secondary.base.clone(), border_radius: theme.radius.sm }} />
                        <Element style={{ width: Length::px(32.0), height: Length::px(32.0), background: theme.color.secondary.base.clone(), border_radius: theme.radius.sm }} />
                        <Element style={{ width: Length::px(32.0), height: Length::px(32.0), background: theme.color.secondary.base.clone(), border_radius: theme.radius.sm }} />
                    </Element>
                </Sample>
            </SampleRow>
        </Demo>
    }
}
