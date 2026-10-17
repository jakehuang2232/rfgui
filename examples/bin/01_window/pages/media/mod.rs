mod icons;
mod image;
mod svg;

use crate::rfgui::ui::{RsxNode, component, rsx};
use icons::MaterialSymbols;
use image::ImageDemo;
use svg::SvgDemo;

#[component]
pub fn MediaPage(animation_on: bool) -> RsxNode {
    rsx! {
        <ImageDemo />
        <SvgDemo />
        <MaterialSymbols animation_on={animation_on} />
    }
}
