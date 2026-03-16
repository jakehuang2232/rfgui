use rfgui::ui::host::{Image, ImageFit, ImageSampling};
use crate::rfgui::ui::host::{Element};
use crate::rfgui::ui::{RsxNode, rsx};
use crate::rfgui::{Layout, Length, Padding};
use crate::rfgui_components::{
    Theme,
};
use crate::utils::output_image_source;

pub fn build(
    theme: &Theme,
) -> RsxNode {
    rsx! {
        <Element style={{
            width: Length::percent(100.0),
            height: Length::percent(100.0),
            layout: Layout::flow().column().no_wrap(),
            padding: Padding::uniform(theme.spacing.md),
        }}>
            <Image source={output_image_source("rfgui-logo.png")} sampling={ImageSampling::Linear} fit={ImageFit::Contain}/>
        </Element>
    }
}
