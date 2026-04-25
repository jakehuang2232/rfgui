use crate::components::GlobalKeyRenderTestBlock;
use crate::rfgui::ui::{RsxNode, component, on_click, rsx, use_state};
use crate::rfgui::view::{Element, Image, ImageFit, Svg, SvgSource, Text};
use crate::rfgui::{
    Align, Angle, Animation, Animator, Border, BorderRadius, ClipMode, Collision,
    CollisionBoundary, Color, CrossSize, Direction, JustifyContent, Keyframe, Layout, Length,
    Opacity, Padding, ParsedValue, Perspective, Position, PropertyId, Repeat, Rotate, Scale,
    ScrollDirection, Style, Transform, TransformOrigin, Transition, TransitionProperty, Translate,
};
use crate::rfgui_components::{Button, ButtonVariant, Theme};
use crate::utils::output_image_source;
use rfgui::{FillMode, Gradient, SideOrCorner};

fn animator_demo_keyframe<T: crate::rfgui::ColorLike>(
    background: T,
    width: f32,
    height: f32,
    opacity: f32,
    offset_x: f32,
    rotate_deg: f32,
    scale: f32,
) -> Style {
    let mut style = Style::new();
    style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(background),
    );
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(width)));
    style.insert(PropertyId::Height, ParsedValue::Length(Length::px(height)));
    style.insert(
        PropertyId::Opacity,
        ParsedValue::Opacity(Opacity::new(opacity)),
    );
    style.set_border_radius(BorderRadius::uniform(Length::px(18.0)));
    style.set_transform(Transform::new([
        Translate::x(Length::px(offset_x)),
        Rotate::z(Angle::deg(rotate_deg)),
        Scale::xy(scale, scale),
    ]));
    style
}

#[component]
pub fn RenderTest(theme: Theme) -> RsxNode {
    let click_count = use_state(|| 0_i32);
    let transform_event_status = use_state(|| String::from("Move over the transform cards"));
    let justify_content = use_state(|| JustifyContent::Start);
    let align = use_state(|| Align::Start);
    let cross_size = use_state(|| CrossSize::Fit);

    let justify_content_start = justify_content.binding();
    let justify_content_center = justify_content.binding();
    let justify_content_end = justify_content.binding();
    let justify_content_space_between = justify_content.binding();
    let justify_content_space_around = justify_content.binding();
    let justify_content_space_evenly = justify_content.binding();

    let align_start = align.binding();
    let align_center = align.binding();
    let align_end = align.binding();
    let cross_size_fit = cross_size.binding();
    let cross_size_stretch = cross_size.binding();
    let transform_enter = transform_event_status.binding();
    let transform_leave = transform_event_status.binding();
    let transform_move = transform_event_status.binding();
    let transform_down = transform_event_status.binding();
    let transform_up = transform_event_status.binding();
    let transform_click = transform_event_status.binding();
    let increment_state = click_count.clone();
    let increment = on_click(move |event| {
        increment_state.update(|value| *value += 1);
        event.meta.stop_propagation();
    });
    rsx! {
            <Element style={{
                width: Length::percent(100.0),
                background: Color::transparent(),
                layout: Layout::flow()
                    .row()
                    .wrap()
                    .justify_content(justify_content.get())
                    .align(align.get())
                    .cross_size(cross_size.get()),
                gap: theme.spacing.md,
                padding: Padding::uniform(theme.spacing.xl),
                font: theme.typography.font_family.clone(),
                color: theme.color.text.primary.clone(),
                font_size: theme.typography.size.sm,
            }} anchor="root">
                <Element style={{
                    width: Length::percent(100.0),
                    layout: Layout::flow().row().wrap(),
                    gap: theme.spacing.md,
                }}>
                    Justify Content:
                    <Button  on_click={move |_| {justify_content_start.set(JustifyContent::Start);}}>Start</Button>
                    <Button  on_click={move |_| {justify_content_center.set(JustifyContent::Center);}}>Center</Button>
                    <Button  on_click={move |_| {justify_content_end.set(JustifyContent::End);}}>End</Button>
                    <Button  on_click={move |_| {justify_content_space_between.set(JustifyContent::SpaceBetween);}}>SpaceBetween</Button>
                    <Button  on_click={move |_| {justify_content_space_around.set(JustifyContent::SpaceAround);}}>SpaceAround</Button>
                    <Button  on_click={move |_| {justify_content_space_evenly.set(JustifyContent::SpaceEvenly);}}>SpaceEvenly</Button>
                </Element>
                <Element style={{
                    width: Length::percent(100.0),
                    layout: Layout::flow().row().wrap(),
                    gap: theme.spacing.md,
                }}>
                    Cross Align:
                    <Button  on_click={move |_| {align_start.set(Align::Start);}}>Start</Button>
                    <Button  on_click={move |_| {align_center.set(Align::Center);}}>Center</Button>
                    <Button  on_click={move |_| {align_end.set(Align::End);}}>End</Button>
                    Cross Size:
                    <Button  on_click={move |_| {cross_size_fit.set(CrossSize::Fit);}}>Fit</Button>
                    <Button  on_click={move |_| {cross_size_stretch.set(CrossSize::Stretch);}}>Stretch</Button>
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
                    background: Gradient::rainbow(SideOrCorner::Right),
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
                    <Text>Svg Test</Text>
                    <Svg
                        source={SvgSource::Content(
                            r##"<svg width="160" height="120" viewBox="0 0 160 120" xmlns="http://www.w3.org/2000/svg">
<rect x="8" y="8" width="144" height="104" rx="20" fill="#0f766e"/>
<circle cx="48" cy="42" r="18" fill="#99f6e4"/>
<path d="M38 84 L72 58 L98 80 L122 46" fill="none" stroke="#f0fdfa" stroke-width="10" stroke-linecap="round" stroke-linejoin="round"/>
<rect x="94" y="72" width="34" height="24" rx="8" fill="#f59e0b"/>
</svg>"##
                                .to_string()
                        )}
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
                                <Text>Loading svg...</Text>
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
                                <Text>Svg load failed</Text>
                            </Element>
                        }}
                    />
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
                    <Text>Animator Test</Text>
                    <Element style={{
                        width: Length::percent(100.0),
                        height: Length::px(120.0),
                        background: Color::hex("#08111f"),
                        border: Border::uniform(Length::px(1.0), &Color::hex("#1e293b")),
                        border_radius: theme.radius.md,
                        layout: Layout::flow().row().no_wrap().justify_content(JustifyContent::Center),
                        align: Align::Center,
                    }}>
                        <Element style={{
                            width: Length::px(56.0),
                            height: Length::px(56.0),
                            background: Color::hex("#38bdf8"),
                            border_radius: BorderRadius::uniform(Length::px(18.0)),
                            animator: Animator::new([
                                Animation::new([
                                    Keyframe::new(0.0, animator_demo_keyframe(Color::hex("#38bdf8"), 56.0, 56.0, 0.72, -34.0, -18.0, 0.88)),
                                    Keyframe::new(0.45, animator_demo_keyframe(Color::hex("#f97316"), 88.0, 40.0, 1.0, 0.0, 0.0, 3.0)),
                                    Keyframe::new(1.0, animator_demo_keyframe(Color::hex("#22c55e"), 52.0, 74.0, 0.82, 34.0, 16.0, 0.92)),
                                ])
                                .duration(2200)
                                .direction(Direction::Alternate),
                            ])
                            .duration(1400)
                            .repeat(Repeat::Infinite)
                            .fill_mode(FillMode::Both)
                            .direction(Direction::Normal),
                        }}>
                            <Text style={{ color: theme.color.text.secondary.clone() }}>
                                {"Animator::new + Animation::new + Keyframe::new"}
                            </Text>
                        </Element>
                    </Element>
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
                    layout: Layout::flow().row().wrap(),
                    gap: theme.spacing.sm,
                    padding: Padding::uniform(theme.spacing.sm),
                    color: theme.color.text.primary.clone(),
                }}>
                    <Text>
                        Button Test
                    </Text>
                    <Text>{format!("Click Count: {}", click_count.get())}</Text>
                    <Button

                        variant={Some(ButtonVariant::Contained)}
                        on_click={increment}
    >{"Click\nMe"}</Button>
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
                <Element style={{
                    width: Length::percent(100.0),
                    background: theme.color.layer.surface.clone(),
                    border: Border::uniform(Length::px(2.0), theme.color.primary.base.as_ref()),
                    border_radius: theme.radius.lg,
                    padding: Padding::uniform(theme.spacing.md),
                    layout: Layout::flow().column().no_wrap(),
                    gap: theme.spacing.sm,
                    color: theme.color.text.primary.clone(),
                }}>
                    <Text>Transform Test</Text>
                    <Text>{transform_event_status.get()}</Text>
                    <Element style={{
                        width: Length::percent(100.0),
                        layout: Layout::flow().row().wrap(),
                        gap: theme.spacing.lg,
                        cross_size: CrossSize::Fit,
                    }}>
                        <Element style={{
                            width: Length::px(210.0),
                            height: Length::px(170.0),
                            background: theme.color.layer.raised.clone(),
                            border: Border::uniform(Length::px(1.0), theme.color.border.as_ref()),
                            border_radius: theme.radius.md,
                            padding: Padding::uniform(theme.spacing.sm),
                            color: theme.color.text.secondary.clone(),
                            layout: Layout::flow().column().no_wrap(),
                            gap: theme.spacing.xs,
                        }}>
                            <Text>translate + scale</Text>
                            <Element style={{
                                width: Length::px(136.0),
                                height: Length::px(96.0),
                                background: theme.color.primary.base.clone(),
                                color: theme.color.primary.on.clone(),
                                border_radius: theme.radius.md,
                                transform: Transform::new([
                                    Translate::x(Length::px(10.0)).with_y(Length::px(8.0)),
                                    Scale::xy(1.1, 0.9),
                                ]),
                                transform_origin: TransformOrigin::center(),
                            }}>
                                <Text>translate + scale</Text>
                            </Element>
                        </Element>
                        <Element style={{
                            width: Length::px(210.0),
                            height: Length::px(170.0),
                            background: theme.color.layer.raised.clone(),
                            border: Border::uniform(Length::px(1.0), theme.color.border.as_ref()),
                            border_radius: theme.radius.md,
                            padding: Padding::uniform(theme.spacing.sm),
                            color: theme.color.text.secondary.clone(),
                            layout: Layout::flow().column().no_wrap(),
                            gap: theme.spacing.xs,
                        }}>
                            <Text>rotate z</Text>
                            <Element style={{
                                width: Length::px(132.0),
                                height: Length::px(92.0),
                                background: "#0f766e",
                                color: theme.color.background.base.clone(),
                                border_radius: theme.radius.md,
                                transform: Transform::new([
                                    Rotate::z(Angle::deg(-15.0)),
                                ]),
                                transform_origin: TransformOrigin::center(),
                            }}>
                                <Text>rotate z</Text>
                            </Element>
                        </Element>
                        <Element style={{
                            width: Length::px(210.0),
                            height: Length::px(170.0),
                            background: theme.color.layer.raised.clone(),
                            border: Border::uniform(Length::px(1.0), theme.color.border.as_ref()),
                            border_radius: theme.radius.md,
                            padding: Padding::uniform(theme.spacing.sm),
                            color: theme.color.text.secondary.clone(),
                            layout: Layout::flow().column().no_wrap(),
                            gap: theme.spacing.xs,
                        }}>
                            <Text>perspective + rotate x/y</Text>
                            <Element style={{
                                width: Length::px(132.0),
                                height: Length::px(92.0),
                                background: "#7c3aed",
                                color: theme.color.background.base.clone(),
                                border_radius: theme.radius.md,
                                transform: Transform::new([
                                    Perspective::px(560.0),
                                    Rotate::x(Angle::deg(26.0)).y(Angle::deg(-18.0)).z(Angle::deg(8.0)),
                                ]),
                                transform_origin: TransformOrigin::center().with_z(12.0),
                            }}>
                                <Text>3D card</Text>
                            </Element>
                        </Element>
                        <Element style={{
                            width: Length::px(210.0),
                            height: Length::px(170.0),
                            background: theme.color.layer.raised.clone(),
                            border: Border::uniform(Length::px(1.0), theme.color.border.as_ref()),
                            border_radius: theme.radius.md,
                            padding: Padding::uniform(theme.spacing.sm),
                            color: theme.color.text.secondary.clone(),
                            layout: Layout::flow().column().no_wrap(),
                            gap: theme.spacing.xs,
                        }}>
                            <Text>interactive transform</Text>
                            <Element
                                style={{
                                    width: Length::px(136.0),
                                    height: Length::px(96.0),
                                    background: "#dc2626",
                                    color: theme.color.background.base.clone(),
                                    border_radius: theme.radius.md,
                                    transform: Transform::new([
                                        Perspective::px(720.0),
                                        Translate::x(Length::px(6.0)).with_y(Length::px(6.0)),
                                        Rotate::x(Angle::deg(18.0)).y(Angle::deg(18.0)).z(Angle::deg(-10.0)),
                                        Scale::xy(1.04, 1.04),
                                    ]),
                                    transform_origin: TransformOrigin::percent(50.0, 50.0).with_z(18.0),
                                    hover: {
                                        background: "#f97316",
                                        border: Border::uniform(Length::px(2.0), theme.color.background.base.as_ref()),
                                    },
                                    transition: [
                                        Transition::new(TransitionProperty::All, theme.motion.duration.fast).ease_in_out(),
                                    ],
                                }}
                                on_pointer_enter={move |_| {
                                    transform_enter.set("transform target: mouse enter".to_string());
                                }}
                                on_pointer_leave={move |_| {
                                    transform_leave.set("transform target: mouse leave".to_string());
                                }}
                                on_pointer_move={move |event| {
                                    transform_move.set(format!(
                                        "transform target: move local=({:.1}, {:.1})",
                                        event.pointer.local_x,
                                        event.pointer.local_y
                                    ));
                                }}
                                on_pointer_down={move |event| {
                                    transform_down.set(format!(
                                        "transform target: mouse down local=({:.1}, {:.1})",
                                        event.pointer.local_x,
                                        event.pointer.local_y
                                    ));
                                }}
                                on_pointer_up={move |event| {
                                    transform_up.set(format!(
                                        "transform target: mouse up local=({:.1}, {:.1})",
                                        event.pointer.local_x,
                                        event.pointer.local_y
                                    ));
                                }}
                                on_click={move |event| {
                                    transform_click.set(format!(
                                        "transform target: click local=({:.1}, {:.1})",
                                        event.pointer.local_x,
                                        event.pointer.local_y
                                    ));
                                }}
                            >
                                <Text>hover / enter / move / click</Text>
                            </Element>
                        </Element>
                        <Element style={{
                            width: Length::px(250.0),
                            height: Length::px(210.0),
                            background: Color::hex("#0b1220"),
                            border: Border::uniform(Length::px(1.0), &Color::hex("#24324a")),
                            border_radius: theme.radius.lg,
                            padding: Padding::uniform(theme.spacing.sm),
                            color: Color::hex("#9fb3c8"),
                            layout: Layout::flow().column().no_wrap(),
                            gap: theme.spacing.xs,
                        }}>
                            <Text>hover transform showcase</Text>
                            <Element
                                style={{
                                    width: Length::px(210.0),
                                    height: Length::px(150.0),
                                    background: Color::hex("#050816"),
                                    border: Border::uniform(Length::px(1.0), &Color::hex("#1e293b")),
                                    border_radius: BorderRadius::uniform(Length::px(22.0)),
                                    layout: Layout::flow().column().no_wrap(),
                                    padding: Padding::uniform(Length::px(14.0)),
                                    gap: Length::px(8.0),
                                    transform: Transform::new([
                                        Perspective::px(1200.0),
                                        Rotate::x(Angle::deg(0.0)).y(Angle::deg(0.0)).z(Angle::deg(0.0)),
                                        Scale::xy(1.0, 1.0),
                                    ]),
                                    transform_origin: TransformOrigin::percent(18.0, 22.0).with_z(26.0),
                                    box_shadow: vec![
                                        theme.shadow.level_3.color(Color::rgba(7, 12, 24, 210)).offset_x(0.0).offset_y(18.0).blur(36.0).spread(0.0),
                                    ],
                                    hover: {
                                        background: Color::hex("#0f172a"),
                                        border: Border::uniform(Length::px(1.0), &Color::hex("#7dd3fc")),
                                        transform: Transform::new([
                                            Perspective::px(1200.0),
                                            Translate::x(Length::percent(5.0)).with_y(Length::px(-10.0)),
                                            Rotate::x(Angle::deg(20.0)).y(Angle::deg(-28.0)).z(Angle::deg(-8.0)),
                                            Scale::xy(1.06, 1.06),
                                        ]),
                                        transform_origin: TransformOrigin::percent(82.0, 16.0).with_z(52.0),
                                        box_shadow: vec![
                                            theme.shadow.level_3.color(Color::rgba(34, 211, 238, 110)).offset_x(-10.0).offset_y(28.0).blur(44.0).spread(2.0),
                                        ],
                                    },
                                    transition: [
                                        Transition::new(TransitionProperty::Transform, theme.motion.duration.slow).ease_in_out(),
                                        Transition::new(TransitionProperty::TransformOrigin, theme.motion.duration.slow).ease_in_out(),
                                        Transition::new(TransitionProperty::BackgroundColor, theme.motion.duration.fast).ease_in_out(),
                                        Transition::new(TransitionProperty::BorderColor, theme.motion.duration.fast).ease_in_out(),
                                        Transition::new(TransitionProperty::BoxShadow, theme.motion.duration.slow).ease_in_out(),
                                    ],
                                }}
                            >
                                <Element style={{
                                    width: Length::percent(100.0),
                                    height: Length::px(10.0),
                                    background: Color::hex("#22d3ee"),
                                    border_radius: BorderRadius::uniform(Length::px(999.0)),
                                    opacity: 0.85,
                                }} />
                                <Element style={{
                                    width: Length::percent(100.0),
                                    layout: Layout::flow()
                                        .row()
                                        .no_wrap()
                                        .justify_content(JustifyContent::SpaceBetween)
                                        .align(Align::Center),
                                }}>
                                    <Text style={{ color: Color::hex("#e2e8f0"), font_size: theme.typography.size.lg }}>
                                        Neon Tilt Card
                                    </Text>
                                    <Element style={{
                                        width: Length::px(44.0),
                                        height: Length::px(44.0),
                                        background: Color::hex("#1d4ed8"),
                                        border_radius: BorderRadius::uniform(Length::px(14.0)),
                                        transform: Transform::new([
                                            Rotate::z(Angle::deg(-12.0)),
                                        ]),
                                    }}>
                                        <Text style={{ color: Color::hex("#dbeafe") }}>RF</Text>
                                    </Element>
                                </Element>
                                <Text style={{ color: Color::hex("#7dd3fc") }}>
                                    hover me
                                </Text>
                                <Text style={{ color: Color::hex("#94a3b8") }}>
                                    transform + transform-origin + perspective
                                </Text>
                                <Element style={{
                                    width: Length::percent(100.0),
                                    height: Length::px(44.0),
                                    background: Color::hex("#111c34"),
                                    border_radius: BorderRadius::uniform(Length::px(14.0)),
                                    layout: Layout::flow()
                                        .row()
                                        .no_wrap()
                                        .justify_content(JustifyContent::SpaceBetween)
                                        .align(Align::Center),
                                    padding: Padding::uniform(Length::px(0.0))
                                        .xy(Length::px(12.0), Length::px(10.0)),
                                    color: Color::hex("#cbd5e1"),
                                }}>
                                    <Text>origin swings across the card</Text>
                                    <Text style={{ color: Color::hex("#f8fafc") }}>LIVE</Text>
                                </Element>
                            </Element>
                        </Element>
                    </Element>
                </Element>
                <GlobalKeyRenderTestBlock />
            </Element>
        }
}
