mod hover;
mod keyframes;
mod transitions;

use crate::rfgui::ui::{RsxNode, component, rsx};
use hover::StyleTransitions;
use keyframes::Keyframes;
use transitions::TransitionLab;

#[component]
pub fn AnimationPage(animation_on: bool) -> RsxNode {
    rsx! {
        <Keyframes animation_on={animation_on} />
        <StyleTransitions />
        <TransitionLab />
    }
}
