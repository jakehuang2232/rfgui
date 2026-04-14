use crate::rfgui::ui::{RsxNode, component, rsx};
use rfgui::{Align, Animation, Animator, Direction, Element, JustifyContent, Keyframe, Layout, Length, Repeat, style, Transform, Scale};

#[component]
pub fn MainScene() -> RsxNode {
    rsx! {
        <Element
            style={{
                width: Length::percent(100.0),
                height: Length::percent(100.0),
                layout: Layout::flex().align(Align::Center).justify_content(JustifyContent::Center),
                font_size: 100.0,
                color: "#fff",
                animator: Animator::new([
                    Animation::new([
                        // Keyframe::new(0.0, style!{font_size: 100.0}),
                        // Keyframe::new(1.0, style!{font_size: 200.0}),
                        Keyframe::new(0.0, style!{transform: Transform::new([Scale::uniform(1.0)])}),
                        Keyframe::new(1.0, style!{transform: Transform::new([Scale::uniform(1.5)])}),
                    ]),
                ]).duration(750)
                .repeat(Repeat::Infinite)
                .direction(Direction::Alternate)
                .ease_in_out()
            }}>
            Hello, world!
        </Element>
    }
}
