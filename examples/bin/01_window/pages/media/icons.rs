use crate::rfgui::style::{Angle, Layout, Length, Rotate, Transform};
use crate::rfgui::ui::{RsxNode, component, rsx};
use crate::rfgui::view::{Element, Text};
use crate::rfgui_components::material_symbol::{
    CloseIcon, FavoriteIcon, HomeIcon, RocketLaunchIcon, SettingsIcon,
};
use crate::rfgui_components::use_theme;
use crate::showcase::{Demo, demo_animator};
use rfgui::style::Repeat::Infinite;
use rfgui::style::{Animation, Animator, FillMode, Keyframe};

#[component]
pub fn MaterialSymbols(animation_on: bool) -> RsxNode {
    let (theme, _) = use_theme();
    rsx! {
        <Demo
            title="Material Symbols"
            description="Every Material Symbol is an icon component, drawn from its own single-glyph font so an app carries only the icons it uses. Icons are text: they take a color, a size and animations."
        >
            <Element style={{
                width: Length::percent(100.0),
                layout: Layout::flow().row().wrap(),
                gap: theme.spacing.md,
                color: theme.color.text.primary.clone(),
                font_size: theme.typography.size.sm,
            }}>
                <Element style={{
                    layout: Layout::flow().column().no_wrap(),
                    gap: theme.spacing.xs,
                }}>
                    <Text>Default</Text>
                    <CloseIcon style={{ font_size: 24.0 }} />
                </Element>
                <Element style={{
                    layout: Layout::flow().column().no_wrap(),
                    gap: theme.spacing.xs,
                }}>
                    <Text>Colored</Text>
                    <CloseIcon style={{
                        font_size: 24.0,
                        color: theme.color.secondary.base.clone(),
                    }} />
                </Element>
                <Element style={{
                    layout: Layout::flow().column().no_wrap(),
                    gap: theme.spacing.xs,
                }}>
                    <Text>Rotating</Text>
                    <CloseIcon style={{
                        font_size: 24.0,
                        color: theme.color.primary.base.clone(),
                        animator: demo_animator(animation_on, Animator::new([
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
                        .duration(theme.motion.duration.slow)),
                    }} />
                </Element>
                <Element style={{
                    layout: Layout::flow().column().no_wrap(),
                    gap: theme.spacing.xs,
                }}>
                    <Text>Sized by font size</Text>
                    <Element style={{
                        layout: Layout::flow().row().no_wrap(),
                        gap: theme.spacing.sm,
                        font_size: 32.0,
                    }}>
                        <HomeIcon />
                        <SettingsIcon />
                        <FavoriteIcon style={{ color: theme.color.error.base.clone() }} />
                        <RocketLaunchIcon style={{ color: theme.color.warning.base.clone() }} />
                    </Element>
                </Element>
            </Element>
        </Demo>
    }
}
