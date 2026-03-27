use crate::components::GlobalKeyRenderTestBlock;
use crate::rfgui::ui::host::{Element, Image, ImageFit, Text, TextArea};
use crate::rfgui::ui::{Binding, ClickHandlerProp, RsxNode, rsx};
use crate::rfgui::{
    Align, Border, BorderRadius, ClipMode, Collision, CollisionBoundary, Color, CrossSize,
    FontFamily, JustifyContent, Layout, Length, Padding, Position, ScrollDirection, Transition,
    TransitionProperty,
};
use crate::rfgui_components::{Button, ButtonVariant, Theme};
use crate::utils::output_image_source;

pub struct RenderTestBindings {
    pub justify_content: Binding<JustifyContent>,
    pub align: Binding<Align>,
    pub cross_size: Binding<CrossSize>,
    pub message: Binding<String>,
}

pub struct RenderTestValues {
    pub click_count: i32,
    pub message: String,
}

pub fn build(
    theme: &Theme,
    bindings: RenderTestBindings,
    values: RenderTestValues,
    increment: ClickHandlerProp,
) -> RsxNode {
    let justify_content_start = bindings.justify_content.clone();
    let justify_content_center = bindings.justify_content.clone();
    let justify_content_end = bindings.justify_content.clone();
    let justify_content_space_between = bindings.justify_content.clone();
    let justify_content_space_around = bindings.justify_content.clone();
    let justify_content_space_evenly = bindings.justify_content.clone();

    let align_start = bindings.align.clone();
    let align_center = bindings.align.clone();
    let align_end = bindings.align.clone();
    let cross_size_fit = bindings.cross_size.clone();
    let cross_size_stretch = bindings.cross_size.clone();
    rsx! {
        <Element style={{
            width: Length::percent(100.0),
            height: Length::percent(100.0),
            background: Color::transparent(),
            layout: Layout::flow()
                .row()
                .wrap()
                .justify_content(bindings.justify_content.get())
                .align(bindings.align.get())
                .cross_size(bindings.cross_size.get()),
            gap: theme.spacing.md,
            padding: Padding::uniform(theme.spacing.xl),
            scroll_direction: ScrollDirection::Vertical,
            font: FontFamily::new(["Noto Sans CJK TC", "PingFang TC"]),
            color: theme.color.text.primary.clone(),
            font_size: theme.typography.size.sm,
        }} anchor="root">
            <Element style={{
                width: Length::percent(100.0),
                layout: Layout::flow().row().wrap(),
                gap: theme.spacing.md,
            }}>
                Justify Content:
                <Button label="Start" on_click={move |_| {justify_content_start.set(JustifyContent::Start);}} />
                <Button label="Center" on_click={move |_| {justify_content_center.set(JustifyContent::Center);}} />
                <Button label="End" on_click={move |_| {justify_content_end.set(JustifyContent::End);}} />
                <Button label="SpaceBetween" on_click={move |_| {justify_content_space_between.set(JustifyContent::SpaceBetween);}} />
                <Button label="SpaceAround" on_click={move |_| {justify_content_space_around.set(JustifyContent::SpaceAround);}} />
                <Button label="SpaceEvenly" on_click={move |_| {justify_content_space_evenly.set(JustifyContent::SpaceEvenly);}} />
            </Element>
            <Element style={{
                width: Length::percent(100.0),
                layout: Layout::flow().row().wrap(),
                gap: theme.spacing.md,
            }}>
                Cross Align:
                <Button label="Start" on_click={move |_| {align_start.set(Align::Start);}} />
                <Button label="Center" on_click={move |_| {align_center.set(Align::Center);}} />
                <Button label="End" on_click={move |_| {align_end.set(Align::End);}} />
                Cross Size:
                <Button label="Fit" on_click={move |_| {cross_size_fit.set(CrossSize::Fit);}} />
                <Button label="Stretch" on_click={move |_| {cross_size_stretch.set(CrossSize::Stretch);}} />
            </Element>
            <Element style={{
                width: Length::px(100.0),
                height: Length::px(100.0),
                background: theme.color.primary.base.clone(),
                color: theme.color.primary.on.clone(),
                padding: Padding::uniform(theme.spacing.md),
            }}>
                Pure Object
            </Element>
            <Element style={{
                width: Length::px(150.0),
                background: theme.color.primary.base.clone(),
                color: theme.color.primary.on.clone(),
                border: Border::uniform(theme.spacing.md, theme.color.border.as_ref()),
                transition: [Transition::new(TransitionProperty::Height, theme.motion.duration.slow).ease_in_out().delay(1000)],
            }}>
                Border + Auto Height + Delay Transition
            </Element>
            <Element style={{
                width: Length::px(150.0),
                height: Length::px(150.0),
                background: theme.color.primary.base.clone(),
                color: theme.color.primary.on.clone(),
                border_radius: BorderRadius::uniform(theme.radius.md)
                    .top_right(Length::px(32.0))
                    .bottom_left(Length::percent(90.0)),
            }}>
                Border Radius
            </Element>
            <Element style={{
                width: Length::percent(50.0),
                height: Length::px(150.0),
                background: theme.color.primary.base.clone(),
                color: theme.color.primary.on.clone(),
                border_radius: BorderRadius::uniform(theme.radius.md)
                    .top_right(Length::px(32.0))
                    .bottom_left(Length::percent(90.0)),
                transition: [
                    Transition::new(TransitionProperty::All, theme.motion.duration.slow),
                ]
            }}>
                Percentage Width
            </Element>
            <Element style={{
                width: Length::px(150.0),
                height: Length::px(150.0),
                background: theme.color.primary.base.clone(),
                color: theme.color.primary.on.clone(),
                border: Border::uniform(Length::px(10.0), &Color::hex("#274f8b"))
                    .top(Some(Length::px(20.0)), Some(&Color::hex("#e06c75")))
                    .left(Some(Length::px(15.0)), Some(&Color::hex("#2db353"))),
                border_radius: BorderRadius::uniform(Length::px(10.0))
                    .top_right(Length::px(10.0))
                    .bottom_left(Length::percent(90.0)),
                box_shadow: vec![
                    theme.shadow.level_3,
                ],
            }}>
                Border Radius + Border + Shadow
            </Element>
            <Element style={{
                width: Length::px(220.0),
                height: Length::px(170.0),
                background: theme.color.layer.surface.clone(),
                border: Border::uniform(Length::px(2.0), theme.color.border.as_ref()),
                border_radius: theme.radius.lg,
                padding: Padding::uniform(theme.spacing.sm),
                layout: Layout::flow().column().no_wrap(),
                gap: theme.spacing.xs,
                color: theme.color.text.primary.clone(),
            }}>
                <Text>Image Test</Text>
                <Image
                    source={output_image_source("rfgui-logo.png")}
                    fit={ImageFit::Contain}
                    style={{
                        width: Length::percent(100.0),
                        height: Length::px(120.0),
                        background: theme.color.layer.raised.clone(),
                        border: Border::uniform(Length::px(1.0), theme.color.divider.as_ref()),
                        border_radius: theme.radius.md,
                    }}
                    loading={rsx! {
                        <Element style={{
                            width: Length::percent(100.0),
                            height: Length::percent(100.0),
                            background: theme.color.layer.raised.clone(),
                            color: theme.color.text.secondary.clone(),
                        }}>
                            <Text>Loading logo...</Text>
                        </Element>
                    }}
                    error={rsx! {
                        <Element style={{
                            width: Length::percent(100.0),
                            height: Length::percent(100.0),
                            background: Color::hex("#b91c1c"),
                            color: theme.color.background.base.clone(),
                            padding: Padding::uniform(theme.spacing.sm),
                        }}>
                            <Text>Logo load failed</Text>
                        </Element>
                    }}
                />
            </Element>
            <Element style={{
                width: Length::px(170.0),
                height: Length::px(170.0),
                background: theme.color.secondary.base.clone(),
                color: theme.color.secondary.on.clone(),
                border: Border::uniform(theme.spacing.xl, theme.color.divider.as_ref()),
                border_radius: theme.radius.lg,
                hover: {
                    border: Border::uniform(theme.spacing.xl, theme.color.primary.base.as_ref()),
                },
                transition: [
                    Transition::new(TransitionProperty::Position, theme.motion.duration.slow).ease_in_out(),
                    Transition::new(TransitionProperty::BorderColor, theme.motion.duration.slow).ease_in_out()
                ],
            }}>
                <Element style={{
                    width: Length::percent(100.0),
                    height: Length::percent(100.0),
                    background: theme.color.primary.base.clone(),
                    color: theme.color.primary.on.clone(),
                    border: Border::uniform(theme.spacing.xl, theme.color.border.as_ref()),
                    border_radius: Length::Zero,
                    hover: {
                        background: theme.color.primary.on.clone(),
                        border: Border::uniform(theme.spacing.xl, theme.color.layer.inverse.as_ref())
                    },
                    transition: [
                        Transition::new(TransitionProperty::All, theme.motion.duration.fast),
                    ],
                }}>
                    Nested + Hover Test + Position Transition
                </Element>
            </Element>
            <Element style={{
                width: Length::px(150.0),
                height: Length::px(150.0),
                background: theme.color.primary.base.clone(),
                color: theme.color.primary.on.clone(),
                border: Border::uniform(theme.spacing.sm, theme.color.border.as_ref()),
                border_radius: 50,
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
                    Clip Test
                </Element>
                <Element style={{ width: Length::px(56.0), height: Length::px(56.0), background: "#61ef9c" }} />
                <Element style={{ width: Length::px(120.0), height: Length::px(64.0), background: "#c678dd", border: Border::uniform(Length::px(4.0), theme.color.state.focus.as_ref()) }} />
            </Element>
            <Element style={{
                width: Length::px(150.0),
                height: Length::px(150.0),
                background: theme.color.layer.surface.clone(),
                border: Border::uniform(Length::px(3.0), theme.color.primary.base.as_ref()),
                border_radius: theme.radius.lg,
                layout: Layout::flow().row(),
                gap: theme.spacing.sm,
                padding: Padding::uniform(theme.spacing.sm),
                color: theme.color.text.primary.clone(),
            }}>
                <Text>
                    Button Test
                </Text>
                <Text>{format!("Click Count: {}", values.click_count)}</Text>
                <Button
                    label="Click\nMe"
                    variant={Some(ButtonVariant::Contained)}
                    on_click={increment}
                />
            </Element>
            <Element style={{
                width: Length::px(150.0),
                height: Length::px(150.0),
                background: theme.color.primary.base.clone(),
                border: Border::uniform(Length::px(3.0), theme.color.border.as_ref()),
                border_radius: theme.radius.lg,
                opacity: 0.5,
                layout: Layout::flow().column(),
                color: theme.color.primary.on.clone(),
            }}>
                <Text>
                    Opacity Nesting
                </Text>
                <Element style={{ width: Length::px(110.0), height: Length::px(86.0), background: "#e06c75", border: Border::uniform(Length::px(2.0), theme.color.border.as_ref()), border_radius: theme.radius.lg, opacity: 0.6 }}>
                    <Element style={{
                        width: Length::px(72.0), height: Length::px(52.0), background: "#61afef", border: Border::uniform(Length::px(2.0), theme.color.border.as_ref()), border_radius: theme.radius.md, opacity: 0.5 }}>
                        <Text opacity=0.9>
                            Alpha
                        </Text>
                    </Element>
                </Element>
            </Element>
            <Element style={{
                width: Length::px(150.0),
                height: Length::px(150.0),
                background: theme.color.primary.base.clone(),
                border: Border::uniform(Length::px(3.0), theme.color.border.as_ref()),
                border_radius: theme.radius.lg,
                layout: Layout::flow().column(),
                color: theme.color.primary.on.clone(),
            }}>
                <Text>
                    Background Opacity Nesting
                </Text>
                <Element style={{ width: Length::px(110.0), height: Length::px(86.0), background: "#e06c75", border: Border::uniform(Length::px(2.0), theme.color.border.as_ref()), border_radius: theme.radius.lg, opacity: 1 }}>
                    <Element style={{
                        width: Length::px(72.0), height: Length::px(52.0), background: "#61afef", border: Border::uniform(Length::px(2.0), theme.color.border.as_ref()), border_radius: theme.radius.md, opacity: 0.5 }}>
                        <Text opacity=0.9>
                            Alpha
                        </Text>
                    </Element>
                </Element>
            </Element>
            <Element style={{
                width: Length::px(150.0),
                height: Length::px(150.0),
                background: theme.color.primary.base.clone(),
                border: Border::uniform(Length::px(3.0), theme.color.border.as_ref()),
                border_radius: theme.radius.lg,
                scroll_direction: ScrollDirection::Vertical,
                layout: Layout::flow().column(),
                color: theme.color.primary.on.clone(),
            }}>
                <Text>Scroll down to see more content 1</Text>
                <Text>Scroll down to see more content 2</Text>
                <Text>Scroll down to see more content 3</Text>
                <Text>Scroll down to see more content 4</Text>
                <Text>Scroll down to see more content 5</Text>
                <Text>Scroll down to see more content 6</Text>
                <Text>Scroll down to see more content 7</Text>
                <Text>Scroll down to see more content 8</Text>
                <Text>Scroll down to see more content 9</Text>
                <Text>Scroll down to see more content 10</Text>
                <Text>Scroll down to see more content 11</Text>
                <Text>Scroll down to see more content 12</Text>
                <Text>Scroll down to see more content 13</Text>
            </Element>
            <Element style={{
                width: Length::px(320.0),
                height: Length::px(170.0),
                background: theme.color.layer.raised.clone(),
                border: Border::uniform(Length::px(3.0), theme.color.primary.base.as_ref()),
                border_radius: theme.radius.lg,
                layout: Layout::flow().column().no_wrap(),
                color: theme.color.text.primary.clone(),
                font_size: theme.typography.size.sm,
            }}>
                <Text>TextArea Test</Text>
                <Text>
                    {format!("Bound chars: {}", values.message.chars().count())}
                </Text>
                <TextArea x=12 y=34 style={{ width: Length::px(296.0), height: Length::px(78.0), color: theme.color.text.primary.clone() }} font_size=13 multiline=true placeholder="Please enter multiline content..." binding={bindings.message} />
                <TextArea x=12 y=98 style={{ width: Length::px(296.0), height: Length::px(26.0), color: theme.color.text.primary.clone() }} font_size=13 multiline=false read_only=true>
                    multiline=false
                    Line breaks should become spaces
                </TextArea>
            </Element>
            <Element style={{
                width: Length::percent(100.0),
                height: Length::px(300.0),
                background: theme.color.layer.surface.clone(),
                border: Border::uniform(Length::px(2.0), theme.color.border.as_ref()),
                border_radius: theme.radius.lg,
                padding: Padding::uniform(theme.spacing.md),
                layout: Layout::flow().column().no_wrap(),
                gap: theme.spacing.sm,
                color: theme.color.text.primary.clone(),
                font_size: theme.typography.size.sm,
            }} anchor="menu_button">
                <Text>Absolute + Anchor + Collision</Text>
                <Text>
                    parent anchor = "menu_button"
                </Text>
                <Element style={{
                    width: Length::px(120.0),
                    height: Length::px(36.0),
                    background: theme.color.primary.base.clone(),
                    color: theme.color.primary.on.clone(),
                    border_radius: theme.radius.md,
                }}>
                    <Text>Menu Button</Text>
                </Element>
                <Element style={{
                    width: Length::percent(100.0),
                    height: Length::px(110.0),
                    layout: Layout::flow().row().no_wrap(),
                    gap: theme.spacing.sm,
                }}>
                    <Element style={{
                        width: Length::px(110.0),
                        height: Length::px(110.0),
                        background: theme.color.layer.raised.clone(),
                        border: Border::uniform(Length::px(1.0), theme.color.border.as_ref()),
                        border_radius: theme.radius.md,
                        padding: Padding::uniform(theme.spacing.sm),
                        color: theme.color.text.primary.clone(),
                    }}>
                        <Text>clip=Parent (default)</Text>
                        <Element style={{
                            position: Position::absolute()
                                .top(Length::px(56.0))
                                .left(Length::px(84.0)),
                            width: Length::px(74.0),
                            height: Length::px(24.0),
                            background: "#ef4444",
                            color: theme.color.secondary.on.clone(),
                            border_radius: theme.radius.sm,
                        }}>
                            <Text>overflow, </Text>
                        </Element>
                    </Element>
                    <Element style={{
                        width: Length::px(110.0),
                        height: Length::px(110.0),
                        background: theme.color.layer.raised.clone(),
                        border: Border::uniform(Length::px(1.0), theme.color.border.as_ref()),
                        border_radius: theme.radius.md,
                        padding: Padding::uniform(theme.spacing.sm),
                        color: theme.color.text.primary.clone(),
                    }}>
                        <Text>clip=Viewport</Text>
                        <Element style={{
                            position: Position::absolute()
                                .top(Length::px(56.0))
                                .left(Length::px(20.0))
                                .clip(ClipMode::Viewport),
                            width: Length::px(74.0),
                            height: Length::px(24.0),
                            background: "#f59e0b",
                            color: theme.color.background.base.clone(),
                            border_radius: theme.radius.sm,
                        }}>
                            <Text>overflow</Text>
                        </Element>
                    </Element>
                    <Element style={{
                        width: Length::px(140.0),
                        height: Length::px(110.0),
                        background: theme.color.layer.raised.clone(),
                        border: Border::uniform(Length::px(1.0), theme.color.border.as_ref()),
                        border_radius: theme.radius.md,
                        padding: Padding::uniform(theme.spacing.sm),
                        layout: Layout::flow().column().no_wrap(),
                        gap: theme.spacing.xs,
                        color: theme.color.text.primary.clone(),
                    }}>
                        <Text>clip=AnchorParent</Text>
                        <Element style={{
                            width: Length::px(56.0),
                            height: Length::px(26.0),
                            background: "#1d4ed8",
                            color: theme.color.primary.on.clone(),
                            border_radius: theme.radius.sm,
                        }} anchor="abs_anchor_test">
                            <Text>anchor</Text>
                        </Element>
                        <Element style={{
                            position: Position::absolute()
                                .anchor("abs_anchor_test")
                                .top(Length::px(0.0))
                                .left(Length::px(38.0))
                                .clip(ClipMode::AnchorParent),
                            width: Length::px(150.0),
                            height: Length::px(22.0),
                            background: "#22c55e",
                            color: theme.color.background.base.clone(),
                            border_radius: theme.radius.sm,
                        }}>
                            <Text>anchor clip</Text>
                        </Element>
                    </Element>
                </Element>
                <Element style={{
                    position: Position::absolute()
                        .anchor("menu_button")
                        .top(Length::px(10.0))
                        .left(Length::percent(50.0))
                        .collision(Collision::FlipFit, CollisionBoundary::Viewport),
                    width: Length::px(150.0),
                    height: Length::px(96.0),
                    background: theme.color.layer.inverse.clone(),
                    border: Border::uniform(Length::px(1.0), theme.color.primary.base.as_ref()),
                    border_radius: theme.radius.md,
                    padding: Padding::uniform(theme.spacing.sm),
                    layout: Layout::flow().column(),
                    gap: theme.spacing.xs,
                    color: theme.color.layer.on_inverse.clone(),
                }}>
                    <Text>Popover (anchor)</Text>
                    <Text>collision: FlipFit + Viewport</Text>
                    <Text>try resizing window edge</Text>
                </Element>
                <Element style={{
                    position: Position::absolute()
                        .top(Length::px(10.0))
                        .bottom(Length::px(10.0))
                        .right(Length::px(12.0)),
                    width: Length::px(120.0),
                    height: Length::px(30.0),
                    background: theme.color.state.focus.clone(),
                    color: theme.color.background.base.clone(),
                    border_radius: theme.radius.md,
                }}>
                    <Text>fallback parent anchor</Text>
                </Element>
            </Element>
            <GlobalKeyRenderTestBlock />
        </Element>
    }
}
