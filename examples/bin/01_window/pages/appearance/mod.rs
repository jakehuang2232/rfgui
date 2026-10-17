mod borders;
mod clipping;
mod opacity;
mod shadows;

use crate::rfgui::ui::{RsxNode, component, rsx};
use borders::BackgroundsAndBorders;
use clipping::RoundedClipping;
use opacity::OpacityNesting;
use shadows::Shadows;

#[component]
pub fn AppearancePage() -> RsxNode {
    rsx! {
        <BackgroundsAndBorders />
        <Shadows />
        <OpacityNesting />
        <RoundedClipping />
    }
}
