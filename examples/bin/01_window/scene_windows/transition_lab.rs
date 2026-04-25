use crate::rfgui::ui::{RsxNode, component, rsx, use_state};
use crate::rfgui::view::{Element, Text};
use crate::rfgui::{
    Angle, Border, Color, JustifyContent, Layout, Length, Padding, Perspective, Rotate, Scale,
    ScrollDirection, Transform, TransformOrigin, Transition, TransitionProperty, Translate,
};
use crate::rfgui_components::{Button, Theme};

#[component]
pub fn TransitionLab(theme: Theme) -> RsxNode {
    let style_enabled = use_state(|| true);
    let style_target_alt = use_state(|| false);
    let transform_enabled = use_state(|| true);
    let transform_target_alt = use_state(|| false);
    let layout_enabled = use_state(|| true);
    let layout_expanded = use_state(|| false);
    let visual_enabled = use_state(|| true);
    let visual_at_end = use_state(|| false);

    let style_enabled_value = style_enabled.get();
    let style_target_alt_value = style_target_alt.get();
    let transform_enabled_value = transform_enabled.get();
    let transform_target_alt_value = transform_target_alt.get();
    let layout_enabled_value = layout_enabled.get();
    let layout_expanded_value = layout_expanded.get();
    let visual_enabled_value = visual_enabled.get();
    let visual_at_end_value = visual_at_end.get();

    let style_start = style_enabled.binding();
    let style_toggle_target = style_target_alt.binding();
    let style_remove = style_enabled.binding();
    let style_reset_enable = style_enabled.binding();
    let style_reset_target = style_target_alt.binding();
    let transform_start = transform_enabled.binding();
    let transform_toggle_target = transform_target_alt.binding();
    let transform_remove = transform_enabled.binding();
    let transform_reset_enable = transform_enabled.binding();
    let transform_reset_target = transform_target_alt.binding();
    let transform_idle_border = Color::hex("#334155");
    let transform_active_border = Color::hex("#67e8f9");
    let layout_start_enable = layout_enabled.binding();
    let layout_toggle_size = layout_expanded.binding();
    let layout_remove = layout_enabled.binding();
    let layout_reset_enable = layout_enabled.binding();
    let layout_reset_size = layout_expanded.binding();
    let visual_start_enable = visual_enabled.binding();
    let visual_toggle_pos = visual_at_end.binding();
    let visual_remove = visual_enabled.binding();
    let visual_reset_enable = visual_enabled.binding();
    let visual_reset_pos = visual_at_end.binding();

    rsx! {
        <Element style={{
            width: Length::percent(100.0),
            layout: Layout::flow().column().no_wrap(),
            gap: theme.spacing.md,
            padding: Padding::uniform(theme.spacing.md),
            color: theme.color.text.primary.clone(),
            font_size: theme.typography.size.sm,
        }}>
            <Text>Transition Plugins Test</Text>
            <Text>
                {"How to verify: click Start Animation first, then click Remove Transition during playback. Expected: jump to the end value immediately."}
            </Text>
            <Element style={{
                layout: Layout::flow().row().wrap(),
                gap: theme.spacing.md,
                width: Length::percent(100.0),
            }}>
                <Element style={{
                    width: Length::px(220.0),
                    background: theme.color.layer.surface.clone(),
                    border: Border::uniform(Length::px(1.0), theme.color.border.as_ref()),
                    border_radius: theme.radius.md,
                    padding: Padding::uniform(theme.spacing.sm),
                    layout: Layout::flow().column().no_wrap(),
                    gap: theme.spacing.xs,
                }}>
                    <Text>StyleTransitionPlugin</Text>
                    <Text>
                        {format!("transition={} target={}", style_enabled_value, style_target_alt_value)}
                    </Text>
                    <Element style={{
                        width: Length::px(180.0),
                        height: Length::px(56.0),
                        background: if style_target_alt_value { Color::hex("#f97316") } else { Color::hex("#22c55e") },
                        border_radius: theme.radius.md,
                        transition: if style_enabled_value {
                            vec![Transition::new(TransitionProperty::BackgroundColor, theme.motion.duration.slow).ease_in_out()]
                        } else {
                            Vec::<Transition>::new()
                        },
                    }} />
                    <Element style={{ layout: Layout::flow().row().wrap(), gap: theme.spacing.xs }}>
                        <Button  on_click={move |_| { style_start.set(true); style_toggle_target.update(|value| *value = !*value); }}>Start Animation</Button>
                        <Button  on_click={move |_| { style_remove.set(false); }}>Remove Transition</Button>
                        <Button  on_click={move |_| { style_reset_enable.set(true); style_reset_target.set(false); }}>Reset</Button>
                    </Element>
                </Element>
                <Element style={{
                    width: Length::px(220.0),
                    background: theme.color.layer.surface.clone(),
                    border: Border::uniform(Length::px(1.0), theme.color.border.as_ref()),
                    border_radius: theme.radius.md,
                    padding: Padding::uniform(theme.spacing.sm),
                    layout: Layout::flow().column().no_wrap(),
                    gap: theme.spacing.xs,
                }}>
                    <Text>TransformTransitionPlugin</Text>
                    <Text>
                        {format!("transition={} target={}", transform_enabled_value, transform_target_alt_value)}
                    </Text>
                    <Element style={{
                        width: Length::px(180.0),
                        height: Length::px(116.0),
                        background: Color::hex("#08111f"),
                        border: Border::uniform(Length::px(1.0), &Color::hex("#1e293b")),
                        border_radius: theme.radius.md,
                        layout: Layout::flow().row().no_wrap().justify_content(JustifyContent::Center),
                        padding: Padding::uniform(theme.spacing.xs),
                    }}>
                        <Element style={{
                            width: Length::px(124.0),
                            height: Length::px(82.0),
                            background: if transform_target_alt_value { Color::hex("#0f172a") } else { Color::hex("#020617") },
                            border: Border::uniform(
                                Length::px(1.0),
                                if transform_target_alt_value { &transform_active_border } else { &transform_idle_border }
                            ),
                            border_radius: theme.radius.md,
                            box_shadow: vec![
                                if transform_target_alt_value {
                                    theme.shadow.level_3
                                        .color(Color::rgba(34, 211, 238, 110))
                                        .offset_x(-10.0)
                                        .offset_y(26.0)
                                        .blur(42.0)
                                        .spread(2.0)
                                } else {
                                    theme.shadow.level_3
                                        .color(Color::rgba(15, 23, 42, 180))
                                        .offset_x(0.0)
                                        .offset_y(12.0)
                                        .blur(26.0)
                                        .spread(0.0)
                                },
                            ],
                            transform_origin: if transform_target_alt_value {
                                TransformOrigin::percent(84.0, 12.0).with_z(48.0)
                            } else {
                                TransformOrigin::percent(16.0, 82.0).with_z(18.0)
                            },
                            transform: if transform_target_alt_value {
                                Transform::new([
                                    Perspective::px(960.0),
                                    Translate::x(Length::px(26.0)).with_y(Length::px(-10.0)),
                                    Rotate::x(Angle::deg(24.0)).y(Angle::deg(-30.0)).z(Angle::deg(-10.0)),
                                    Scale::xy(1.14, 1.14),
                                ])
                            } else {
                                Transform::new([
                                    Perspective::px(960.0),
                                    Translate::x(Length::px(-8.0)).with_y(Length::px(6.0)),
                                    Rotate::x(Angle::deg(-10.0)).y(Angle::deg(14.0)).z(Angle::deg(6.0)),
                                    Scale::xy(0.96, 0.96),
                                ])
                            },
                            transition: if transform_enabled_value {
                                vec![
                                    Transition::new(TransitionProperty::Transform, theme.motion.duration.slow).ease_in_out(),
                                    Transition::new(TransitionProperty::BoxShadow, theme.motion.duration.slow).ease_in_out(),
                                ]
                            } else {
                                Vec::<Transition>::new()
                            },
                            layout: Layout::flow().column().no_wrap(),
                            gap: theme.spacing.xs,
                            padding: Padding::uniform(Length::px(10.0)),
                        }}>
                            <Element style={{
                                width: Length::percent(100.0),
                                height: Length::px(8.0),
                                background: if transform_target_alt_value { Color::hex("#67e8f9") } else { Color::hex("#2563eb") },
                                border_radius: theme.radius.md,
                            }} />
                            <Element style={{
                                width: Length::percent(100.0),
                                layout: Layout::flow().row().no_wrap().justify_content(JustifyContent::SpaceBetween),
                            }}>
                                <Text style={{ color: Color::hex("#e2e8f0") }}>Transform</Text>
                                <Text style={{ color: Color::hex("#7dd3fc") }}>3D</Text>
                            </Element>
                            <Text style={{ color: Color::hex("#94a3b8") }}>
                                {if transform_target_alt_value { "tilt / lift / spin" } else { "armed / waiting" }}
                            </Text>
                        </Element>
                    </Element>
                    <Element style={{ layout: Layout::flow().row().wrap(), gap: theme.spacing.xs }}>
                        <Button  on_click={move |_| { transform_start.set(true); transform_toggle_target.update(|value| *value = !*value); }}>Start Animation</Button>
                        <Button  on_click={move |_| { transform_remove.set(false); }}>Remove Transition</Button>
                        <Button  on_click={move |_| { transform_reset_enable.set(true); transform_reset_target.set(false); }}>Reset</Button>
                    </Element>
                </Element>
                <Element style={{
                    width: Length::px(220.0),
                    background: theme.color.layer.surface.clone(),
                    border: Border::uniform(Length::px(1.0), theme.color.border.as_ref()),
                    border_radius: theme.radius.md,
                    padding: Padding::uniform(theme.spacing.sm),
                    layout: Layout::flow().column().no_wrap(),
                    gap: theme.spacing.xs,
                }}>
                    <Text>LayoutTransitionPlugin</Text>
                    <Text>
                        {format!("transition={} expanded={}", layout_enabled_value, layout_expanded_value)}
                    </Text>
                    <Element style={{
                        width: Length::px(180.0),
                        height: Length::px(58.0),
                        background: "#1f2937",
                        border_radius: theme.radius.md,
                    }}>
                        <Element style={{
                            width: if layout_expanded_value { Length::px(180.0) } else { Length::px(34.0) },
                            height: if layout_expanded_value { Length::px(58.0) } else { Length::px(34.0) },
                            background: "#38bdf8",
                            border_radius: theme.radius.md,
                            transition: if layout_enabled_value {
                                vec![
                                    Transition::new(TransitionProperty::Width, theme.motion.duration.slow).ease_in_out(),
                                    Transition::new(TransitionProperty::Height, theme.motion.duration.slow).ease_in_out(),
                                ]
                            } else {
                                Vec::<Transition>::new()
                            },
                        }} />
                    </Element>
                    <Element style={{ layout: Layout::flow().row().wrap(), gap: theme.spacing.xs }}>
                        <Button  on_click={move |_| { layout_start_enable.set(true); layout_toggle_size.update(|value| *value = !*value); }}>Start Animation</Button>
                        <Button  on_click={move |_| { layout_remove.set(false); }}>Remove Transition</Button>
                        <Button  on_click={move |_| { layout_reset_enable.set(true); layout_reset_size.set(false); }}>Reset</Button>
                    </Element>
                </Element>
                <Element style={{
                    width: Length::px(220.0),
                    background: theme.color.layer.surface.clone(),
                    border: Border::uniform(Length::px(1.0), theme.color.border.as_ref()),
                    border_radius: theme.radius.md,
                    padding: Padding::uniform(theme.spacing.sm),
                    layout: Layout::flow().column().no_wrap(),
                    gap: theme.spacing.xs,
                }}>
                    <Text>VisualTransitionPlugin</Text>
                    <Text>
                        {format!("transition={} at_end={}", visual_enabled_value, visual_at_end_value)}
                    </Text>
                    <Element style={{
                        width: Length::px(180.0),
                        height: Length::px(58.0),
                        background: "#1f2937",
                        border_radius: theme.radius.md,
                        layout: Layout::flow().row().no_wrap().justify_content(if visual_at_end_value { JustifyContent::End } else { JustifyContent::Start }),
                        padding: Padding::uniform(theme.spacing.xs),
                    }}>
                        <Element style={{
                            width: Length::px(42.0),
                            height: Length::px(42.0),
                            background: theme.color.secondary.base.clone(),
                            border_radius: theme.radius.md,
                            transition: if visual_enabled_value {
                                vec![Transition::new(TransitionProperty::Position, theme.motion.duration.slow).ease_in_out()]
                            } else {
                                Vec::<Transition>::new()
                            },
                        }} />
                    </Element>
                    <Element style={{ layout: Layout::flow().row().wrap(), gap: theme.spacing.xs }}>
                        <Button  on_click={move |_| { visual_start_enable.set(true); visual_toggle_pos.update(|value| *value = !*value); }}>Start Animation</Button>
                        <Button  on_click={move |_| { visual_remove.set(false); }}>Remove Transition</Button>
                        <Button  on_click={move |_| { visual_reset_enable.set(true); visual_reset_pos.set(false); }}>Reset</Button>
                    </Element>
                </Element>
            </Element>
            <Element style={{
                width: Length::percent(100.0),
                height: Length::px(176.0),
                background: theme.color.layer.surface.clone(),
                border: Border::uniform(Length::px(1.0), theme.color.border.as_ref()),
                border_radius: theme.radius.md,
                padding: Padding::uniform(theme.spacing.sm),
                layout: Layout::flow().column().no_wrap(),
                gap: theme.spacing.xs,
            }}>
                <Text>ScrollTransitionPlugin</Text>
                <Text>
                    {"Use the mouse wheel to scroll this area and observe inertia/interpolation. This plugin is not controlled by style.transition."}
                </Text>
                <Element style={{
                    width: Length::percent(100.0),
                    height: Length::px(120.0),
                    background: theme.color.layer.inverse.clone(),
                    border: Border::uniform(Length::px(1.0), theme.color.border.as_ref()),
                    border_radius: theme.radius.md,
                    scroll_direction: ScrollDirection::Vertical,
                    layout: Layout::flow().column().no_wrap(),
                    gap: theme.spacing.xs,
                    padding: Padding::uniform(theme.spacing.sm),
                    color: theme.color.layer.on_inverse.clone(),
                }}>
                    <Text>Scroll row 01</Text>
                    <Text>Scroll row 02</Text>
                    <Text>Scroll row 03</Text>
                    <Text>Scroll row 04</Text>
                    <Text>Scroll row 05</Text>
                    <Text>Scroll row 06</Text>
                    <Text>Scroll row 07</Text>
                    <Text>Scroll row 08</Text>
                    <Text>Scroll row 09</Text>
                    <Text>Scroll row 10</Text>
                    <Text>Scroll row 11</Text>
                    <Text>Scroll row 12</Text>
                </Element>
            </Element>
        </Element>
    }
}
