use crate::rfgui::style::{
    Angle, Border, Length, Perspective, Rotate, Scale, Transform, TransformOrigin, Transition,
    TransitionProperty, Translate,
};
use crate::rfgui::ui::{RsxNode, component, rsx, use_state};
use crate::rfgui::view::{Element, Text};
use crate::rfgui_components::use_theme;
use crate::showcase::{Caption, Demo};

#[component]
pub fn TransformHitTesting() -> RsxNode {
    let (theme, _) = use_theme();
    let status = use_state(|| String::from("Move the pointer over the card"));
    let enter = status.binding();
    let leave = status.binding();
    let moved = status.binding();
    let down = status.binding();
    let up = status.binding();
    let click = status.binding();
    rsx! {
        <Demo
            title="Pointer events through transforms"
            description="Hit testing follows the transform, so a tilted card receives exactly the pointer events over its visible shape, with positions in its own coordinates."
        >
            <Caption text={status.get()} />
            <Element style={{ width: Length::px(240.0), height: Length::px(150.0) }}>
                <Element
                    style={{
                        width: Length::px(160.0),
                        height: Length::px(110.0),
                        background: "#dc2626",
                        color: theme.color.background.base.clone(),
                        border_radius: theme.radius.md,
                        transform: Transform::new([
                            Perspective::px(720.0),
                            Translate::x(Length::px(20.0)).with_y(Length::px(16.0)),
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
                    on_pointer_enter={move |_| enter.set("pointer enter".to_string())}
                    on_pointer_leave={move |_| leave.set("pointer leave".to_string())}
                    on_pointer_move={move |event| {
                        moved.set(format!(
                            "pointer move at ({:.1}, {:.1})",
                            event.pointer.local_x, event.pointer.local_y
                        ))
                    }}
                    on_pointer_down={move |event| {
                        down.set(format!(
                            "pointer down at ({:.1}, {:.1})",
                            event.pointer.local_x, event.pointer.local_y
                        ))
                    }}
                    on_pointer_up={move |event| {
                        up.set(format!(
                            "pointer up at ({:.1}, {:.1})",
                            event.pointer.local_x, event.pointer.local_y
                        ))
                    }}
                    on_click={move |event| {
                        click.set(format!(
                            "click at ({:.1}, {:.1})",
                            event.pointer.local_x, event.pointer.local_y
                        ))
                    }}
                >
                    <Text>hover, move, click</Text>
                </Element>
            </Element>
        </Demo>
    }
}
