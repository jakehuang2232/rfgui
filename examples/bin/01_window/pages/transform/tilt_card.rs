use crate::rfgui::style::{
    Align, Angle, Border, BorderRadius, Color, JustifyContent, Layout, Length, Padding,
    Perspective, Rotate, Scale, Transform, TransformOrigin, Transition, TransitionProperty,
    Translate,
};
use crate::rfgui::ui::{RsxNode, component, rsx};
use crate::rfgui::view::{Element, Text};
use crate::rfgui_components::use_theme;
use crate::showcase::Demo;

#[component]
pub fn TiltCard() -> RsxNode {
    let (theme, _) = use_theme();
    rsx! {
        <Demo
            title="Hover transitions in 3D"
            description="On hover the card lifts and turns in perspective while its origin swings across it. Transform, origin, colors and shadow each transition on their own timing."
        >
            <Element style={{
                width: Length::px(272.0),
                background: Color::hex("#0b1220"),
                border: Border::uniform(Length::px(1.0), &Color::hex("#24324a")),
                border_radius: theme.radius.lg,
                padding: Padding::uniform(theme.spacing.lg),
                color: Color::hex("#9fb3c8"),
            }}>
                <Element
                    style={{
                        width: Length::px(240.0),
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
                        layout: Layout::flex()
                            .row()
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
                        min_height: Length::px(44.0),
                        background: Color::hex("#111c34"),
                        border_radius: BorderRadius::uniform(Length::px(14.0)),
                        layout: Layout::flex()
                            .row()
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
        </Demo>
    }
}
