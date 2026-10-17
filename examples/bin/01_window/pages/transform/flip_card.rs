use crate::rfgui::style::{
    Align, Angle, BackfaceVisibility, Cursor, JustifyContent, Layout, Length, Perspective,
    Position, Rotate, Transform, TransformOrigin, Transition, TransitionProperty,
};
use crate::rfgui::ui::{RsxNode, component, rsx, use_state};
use crate::rfgui::view::{Element, Text};
use crate::rfgui_components::{Theme, use_theme};
use crate::showcase::Demo;

const WIDTH: f32 = 220.0;
const HEIGHT: f32 = 132.0;

/// One face of the card, turned by `degrees` around its vertical axis and
/// hidden while it faces away.
fn face(theme: &Theme, label: &str, degrees: f32, front: bool) -> RsxNode {
    let (background, color) = if front {
        (
            theme.color.primary.base.clone(),
            theme.color.primary.on.clone(),
        )
    } else {
        (
            theme.color.secondary.base.clone(),
            theme.color.secondary.on.clone(),
        )
    };
    rsx! {
        <Element style={{
            position: Position::absolute().top(Length::Zero).left(Length::Zero),
            width: Length::px(WIDTH),
            height: Length::px(HEIGHT),
            layout: Layout::flex().row().justify_content(JustifyContent::Center).align(Align::Center),
            background: background,
            color: color,
            font_size: theme.typography.size.lg,
            border_radius: theme.radius.lg,
            transform: Transform::new([Perspective::px(800.0), Rotate::y(Angle::deg(degrees))]),
            transform_origin: TransformOrigin::center(),
            backface_visibility: BackfaceVisibility::Hidden,
            transition: [
                Transition::new(TransitionProperty::Transform, theme.motion.duration.slow).ease_in_out(),
            ],
        }}>
            <Text>{label.to_string()}</Text>
        </Element>
    }
}

#[component]
pub fn FlipCard() -> RsxNode {
    let (theme, _) = use_theme();
    let flipped = use_state(|| false);
    let turn = if flipped.get() { 180.0 } else { 0.0 };
    let flip = {
        let flipped = flipped.clone();
        move |_: &mut crate::rfgui::ui::ClickEvent| flipped.update(|value| *value = !*value)
    };
    rsx! {
        <Demo
            title="Back faces"
            description="With backface_visibility: Hidden, a face disappears while it is turned away from the viewer. Two faces back to back make a card: click it to flip."
        >
            <Element
                style={{
                    width: Length::px(WIDTH),
                    height: Length::px(HEIGHT),
                    cursor: Cursor::Pointer,
                }}
                on_click={flip}
            >
                {face(&theme, "Front", turn, true)}
                {face(&theme, "Back", turn - 180.0, false)}
            </Element>
        </Demo>
    }
}
