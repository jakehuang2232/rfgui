use crate::rfgui::style::{BoxShadow, Color, Layout, Length, Padding};
use crate::rfgui::ui::{RsxNode, component, rsx};
use crate::rfgui::view::{Element, Text};
use crate::rfgui_components::use_theme;
use crate::showcase::Demo;

#[component]
pub fn Shadows() -> RsxNode {
    let (theme, _) = use_theme();
    let tile = |label: &str, shadow: BoxShadow| {
        rsx! {
            <Element style={{
                width: Length::px(120.0),
                height: Length::px(80.0),
                padding: Padding::uniform(theme.spacing.sm),
                background: theme.color.layer.raised.clone(),
                color: theme.color.text.secondary.clone(),
                border_radius: theme.radius.md,
                box_shadow: vec![shadow],
            }}>
                <Text>{label.to_string()}</Text>
            </Element>
        }
    };
    rsx! {
        <Demo
            title="Shadows"
            description="Box shadows with an offset, a blur and a spread, in any color. Each blur is drawn once and reused as a nine-patch, so shadows stay cheap as their boxes resize."
        >
            <Element style={{
                width: Length::percent(100.0),
                layout: Layout::flow().row().wrap(),
                gap: theme.spacing.xl,
                padding: Padding::uniform(theme.spacing.md),
            }}>
                {tile("level 1", theme.shadow.level_1)}
                {tile("level 2", theme.shadow.level_2)}
                {tile("level 3", theme.shadow.level_3)}
                {tile(
                    "colored glow",
                    theme.shadow.level_3
                        .color(Color::rgba(34, 211, 238, 140))
                        .offset_y(0.0)
                        .blur(28.0)
                        .spread(4.0),
                )}
            </Element>
        </Demo>
    }
}
