use crate::rfgui::style::{
    Border, ClipMode, Collision, CollisionBoundary, Layout, Length, Padding, Position,
};
use crate::rfgui::ui::{RsxNode, component, rsx};
use crate::rfgui::view::{Element, Text};
use crate::rfgui_components::use_theme;
use crate::showcase::Demo;

#[component]
pub fn AnchorsAndCollision() -> RsxNode {
    let (theme, _) = use_theme();
    rsx! {
        <Demo
            title="Anchors and collision"
            description="An absolutely positioned element is placed against a named anchor, or against its parent when it names none. Collision handling flips and fits it to stay inside the viewport: scroll the page or resize the window and watch the popover."
        >
            <Element style={{
                width: Length::percent(100.0),
                height: Length::px(200.0),
                background: theme.color.layer.raised.clone(),
                border: Border::uniform(Length::px(1.0), theme.color.divider.as_ref()),
                border_radius: theme.radius.md,
                padding: Padding::uniform(theme.spacing.md),
                layout: Layout::flow().column().no_wrap(),
                gap: theme.spacing.sm,
            }} anchor="popover_anchor">
                <Text>{"This box is the anchor \"popover_anchor\""}</Text>
                <Element style={{
                    position: Position::absolute()
                        .anchor("popover_anchor")
                        .top(Length::px(10.0))
                        .left(Length::percent(50.0))
                        .collision(Collision::FlipFit, CollisionBoundary::Viewport),
                    width: Length::px(180.0),
                    height: Length::px(96.0),
                    background: theme.color.layer.inverse.clone(),
                    border: Border::uniform(Length::px(1.0), theme.color.primary.base.as_ref()),
                    border_radius: theme.radius.md,
                    padding: Padding::uniform(theme.spacing.sm),
                    layout: Layout::flow().column(),
                    gap: theme.spacing.xs,
                    color: theme.color.layer.on_inverse.clone(),
                }}>
                    <Text>Popover</Text>
                    <Text>collision: FlipFit + Viewport</Text>
                </Element>
                <Element style={{
                    position: Position::absolute()
                        .bottom(Length::px(12.0))
                        .right(Length::px(12.0)),
                    padding: Padding::uniform(theme.spacing.sm),
                    background: theme.color.state.focus.clone(),
                    color: theme.color.background.base.clone(),
                    border_radius: theme.radius.md,
                }}>
                    <Text>No anchor: placed against its parent</Text>
                </Element>
            </Element>
        </Demo>
    }
}

#[component]
pub fn ClipModes() -> RsxNode {
    let (theme, _) = use_theme();
    rsx! {
        <Demo
            title="Clip modes"
            description="A positioned element that overflows is clipped by its parent by default. ClipMode::Viewport escapes every ancestor clip; ClipMode::AnchorParent clips it to its anchor's parent instead."
        >
            <Element style={{
                width: Length::percent(100.0),
                layout: Layout::flow().row().wrap(),
                gap: theme.spacing.xl,
                padding: Padding::uniform(Length::Zero).bottom(theme.spacing.lg),
            }}>
                <Element style={{
                    width: Length::px(130.0),
                    height: Length::px(110.0),
                    background: theme.color.layer.raised.clone(),
                    border: Border::uniform(Length::px(1.0), theme.color.border.as_ref()),
                    border_radius: theme.radius.md,
                    padding: Padding::uniform(theme.spacing.sm),
                    layout: Layout::flow().column().no_wrap(),
                    gap: theme.spacing.xs,
                }}>
                    <Text>clip=Parent (default)</Text>
                    <Element style={{
                        position: Position::absolute()
                            .top(Length::px(56.0))
                            .left(Length::px(84.0)),
                        padding: Padding::uniform(theme.spacing.xs),
                        background: "#ef4444",
                        color: theme.color.secondary.on.clone(),
                        border_radius: theme.radius.sm,
                    }}>
                        <Text>clipped here</Text>
                    </Element>
                </Element>
                <Element style={{
                    width: Length::px(130.0),
                    height: Length::px(110.0),
                    background: theme.color.layer.raised.clone(),
                    border: Border::uniform(Length::px(1.0), theme.color.border.as_ref()),
                    border_radius: theme.radius.md,
                    padding: Padding::uniform(theme.spacing.sm),
                    layout: Layout::flow().column().no_wrap(),
                    gap: theme.spacing.xs,
                }}>
                    <Text>clip=Viewport</Text>
                    <Element style={{
                        position: Position::absolute()
                            .top(Length::px(56.0))
                            .left(Length::px(84.0))
                            .clip(ClipMode::Viewport),
                        padding: Padding::uniform(theme.spacing.xs),
                        background: "#f59e0b",
                        color: theme.color.background.base.clone(),
                        border_radius: theme.radius.sm,
                    }}>
                        <Text>escapes the box</Text>
                    </Element>
                </Element>
                <Element style={{
                    width: Length::px(150.0),
                    height: Length::px(110.0),
                    background: theme.color.layer.raised.clone(),
                    border: Border::uniform(Length::px(1.0), theme.color.border.as_ref()),
                    border_radius: theme.radius.md,
                    padding: Padding::uniform(theme.spacing.sm),
                    layout: Layout::flow().column().no_wrap(),
                    gap: theme.spacing.xs,
                }}>
                    <Text>clip=AnchorParent</Text>
                    <Element style={{
                        padding: Padding::uniform(theme.spacing.xs),
                        background: "#1d4ed8",
                        color: theme.color.primary.on.clone(),
                        border_radius: theme.radius.sm,
                    }} anchor="clip_anchor">
                        <Text>anchor</Text>
                    </Element>
                    <Element style={{
                        position: Position::absolute()
                            .anchor("clip_anchor")
                            .top(Length::px(0.0))
                            .left(Length::px(48.0))
                            .clip(ClipMode::AnchorParent),
                        padding: Padding::uniform(theme.spacing.xs),
                        background: "#22c55e",
                        color: theme.color.background.base.clone(),
                        border_radius: theme.radius.sm,
                    }}>
                        <Text>{"clipped to the anchor's parent"}</Text>
                    </Element>
                </Element>
            </Element>
        </Demo>
    }
}
