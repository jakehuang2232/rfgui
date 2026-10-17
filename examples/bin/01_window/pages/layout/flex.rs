use crate::rfgui::style::{Align, Layout, Length, Padding, flex};
use crate::rfgui::ui::{RsxNode, component, rsx};
use crate::rfgui::view::{Element, Text};
use crate::rfgui_components::{Button, ButtonSize, use_theme};
use crate::showcase::{Caption, Demo};

#[component]
pub fn FlexSharing() -> RsxNode {
    let (theme, _) = use_theme();
    let grown = [1.0_f32, 2.0, 1.0]
        .iter()
        .enumerate()
        .map(|(index, &grow)| {
            rsx! {
                <Element key={index} style={{
                    flex: flex().grow(grow).shrink(1.0),
                    min_width: Length::Zero,
                    padding: Padding::uniform(theme.spacing.sm),
                    background: theme.color.secondary.base.clone(),
                    color: theme.color.secondary.on.clone(),
                    border_radius: theme.radius.sm,
                }}>
                    <Text>{format!("grow {grow}")}</Text>
                </Element>
            }
        })
        .collect::<Vec<_>>();

    rsx! {
        <Demo
            title="Flex"
            description="A flex row keeps its children on one line and shares the room between them by their grow and shrink factors."
        >
            <Caption text="Grow 1 : 2 : 1" />
            <Element style={{
                width: Length::percent(100.0),
                layout: Layout::flex().row(),
                gap: theme.spacing.sm,
            }}>
                {grown}
            </Element>
            <Caption text="Text beside fixed items wraps in the room they leave" />
            <Element style={{
                width: Length::percent(100.0),
                layout: Layout::flex().row().align(Align::Center),
                gap: theme.spacing.md,
                padding: Padding::uniform(theme.spacing.md),
                background: theme.color.layer.raised.clone(),
                border_radius: theme.radius.md,
            }}>
                <Element style={{
                    width: Length::px(40.0),
                    height: Length::px(40.0),
                    background: theme.color.primary.base.clone(),
                    border_radius: Length::px(20.0),
                }} />
                <Text>
                    A long message next to an avatar and a button. Narrow the window and this text wraps onto more lines, while the avatar and the button keep their size.
                </Text>
                <Button size={Some(ButtonSize::Small)}>Reply</Button>
            </Element>
        </Demo>
    }
}
