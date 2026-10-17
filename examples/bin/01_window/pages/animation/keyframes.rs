use crate::rfgui::style::{
    Align, Angle, Animation, Animator, Border, BorderRadius, Color, ColorLike, Direction, FillMode,
    JustifyContent, Keyframe, Layout, Length, Opacity, Repeat, Rotate, Scale, Style, Transform,
    Translate,
};
use crate::rfgui::ui::{RsxNode, component, rsx};
use crate::rfgui::view::Element;
use crate::rfgui_components::use_theme;
use crate::showcase::{Demo, demo_animator};
use rfgui::style;

fn keyframe<T: ColorLike + 'static>(
    background: T,
    width: f32,
    height: f32,
    opacity: f32,
    offset_x: f32,
    rotate_deg: f32,
    scale: f32,
) -> Style {
    style! {
        background_color: Box::new(background) as Box<dyn ColorLike>,
        width: Length::px(width),
        height: Length::px(height),
        opacity: Opacity::new(opacity),
        border_radius: BorderRadius::uniform(Length::px(18.0)),
        transform: Transform::new([
            Translate::x(Length::px(offset_x)),
            Rotate::z(Angle::deg(rotate_deg)),
            Scale::xy(scale, scale),
        ]),
    }
}

#[component]
pub fn Keyframes(animation_on: bool) -> RsxNode {
    let (theme, _) = use_theme();
    rsx! {
        <Demo
            title="Keyframe animation"
            description="An Animator plays keyframes of any style: here color, size, opacity and transform at once, repeating forever and alternating direction. The Inspector's animation switch pauses it."
        >
            <Element style={{
                width: Length::px(320.0),
                height: Length::px(140.0),
                background: Color::hex("#08111f"),
                border: Border::uniform(Length::px(1.0), &Color::hex("#1e293b")),
                border_radius: theme.radius.md,
                layout: Layout::flow()
                    .row()
                    .no_wrap()
                    .justify_content(JustifyContent::Center)
                    .align(Align::Center),
            }}>
                <Element style={{
                    width: Length::px(56.0),
                    height: Length::px(56.0),
                    background: Color::hex("#38bdf8"),
                    border_radius: BorderRadius::uniform(Length::px(18.0)),
                    animator: demo_animator(animation_on, Animator::new([
                        Animation::new([
                            Keyframe::new(0.0, keyframe(Color::hex("#38bdf8"), 56.0, 56.0, 0.72, -34.0, -18.0, 0.88)),
                            Keyframe::new(0.45, keyframe(Color::hex("#f97316"), 88.0, 40.0, 1.0, 0.0, 0.0, 1.6)),
                            Keyframe::new(1.0, keyframe(Color::hex("#22c55e"), 52.0, 74.0, 0.82, 34.0, 16.0, 0.92)),
                        ])
                        .duration(2200)
                        .direction(Direction::Alternate),
                    ])
                    .duration(1400)
                    .repeat(Repeat::Infinite)
                    .fill_mode(FillMode::Both)
                    .direction(Direction::Normal)),
                }} />
            </Element>
        </Demo>
    }
}
