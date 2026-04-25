use crate::rfgui::ui::{RsxNode, component, rsx};
use crate::rfgui::view::{Element, Text};
use crate::rfgui::{Angle, Layout, Length, Rotate, Transform};
use crate::rfgui_components::{CloseIcon, Theme};
use rfgui::Repeat::Infinite;
use rfgui::{Animation, Animator, FillMode, Keyframe};
use rfgui_components::Accordion;

#[component]
pub fn MaterialSymbolsSection(theme: Theme) -> RsxNode {
    rsx! {
        <Accordion title="Material Symbols">
            <Element style={{
                width: Length::percent(100.0),
                layout: Layout::flow().row().wrap(),
                gap: theme.spacing.md,
                color: theme.color.text.primary.clone(),
            }}>
                <Element style={{
                    layout: Layout::flow().column().no_wrap(),
                    gap: theme.spacing.xs,
                }}>
                    <Text>Default</Text>
                    <CloseIcon />
                </Element>
                <Element style={{
                    layout: Layout::flow().column().no_wrap(),
                    gap: theme.spacing.xs,
                }}>
                    <Text>Colored</Text>
                    <CloseIcon style={{
                        color: theme.color.secondary.base.clone(),
                        font_size: theme.typography.size.xl,
                    }} />
                </Element>
                <Element style={{
                    layout: Layout::flow().column().no_wrap(),
                    gap: theme.spacing.xs,
                }}>
                    <Text>Rotating</Text>
                    <CloseIcon style={{
                        color: theme.color.primary.base.clone(),
                        animator: Animator::new([
                            Animation::new([
                                Keyframe::new(0.0, rfgui::style! {
                                    transform: Transform::new([Rotate::z(Angle::deg(0.0))]),
                                }),
                                Keyframe::new(1.0, rfgui::style! {
                                    transform: Transform::new([Rotate::z(Angle::deg(360.0))]),
                                }),
                            ]),
                        ]).fill_mode(FillMode::Forwards)
                        .repeat(Infinite)
                        .duration(theme.motion.duration.slow),
                    }} />
                </Element>
            </Element>
        </Accordion>
    }
}
