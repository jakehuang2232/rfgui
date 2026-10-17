mod basics;
mod flip_card;
mod hit_testing;
mod tilt_card;

use crate::rfgui::ui::{RsxNode, component, rsx};
use basics::TransformBasics;
use flip_card::FlipCard;
use hit_testing::TransformHitTesting;
use tilt_card::TiltCard;

#[component]
pub fn TransformPage() -> RsxNode {
    rsx! {
        <TransformBasics />
        <TransformHitTesting />
        <TiltCard />
        <FlipCard />
    }
}
