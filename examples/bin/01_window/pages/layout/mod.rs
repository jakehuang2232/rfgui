mod alignment;
mod flex;
mod inline;
mod positioning;
mod scrolling;
mod sizing;

use crate::rfgui::ui::{RsxNode, component, rsx};
use alignment::FlowAlignment;
use flex::FlexSharing;
use inline::InlineFormatting;
use positioning::{AnchorsAndCollision, ClipModes};
use scrolling::Scrolling;
use sizing::Sizing;

#[component]
pub fn LayoutPage() -> RsxNode {
    rsx! {
        <FlowAlignment />
        <FlexSharing />
        <InlineFormatting />
        <Sizing />
        <AnchorsAndCollision />
        <ClipModes />
        <Scrolling />
    }
}
