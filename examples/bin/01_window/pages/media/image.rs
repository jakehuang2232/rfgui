use crate::rfgui::style::{Border, Color, Length, Padding};
use crate::rfgui::ui::{RsxNode, component, rsx};
use crate::rfgui::view::{Element, Image, ImageFit, Text};
use crate::rfgui_components::use_theme;
use crate::showcase::{Demo, Sample, SampleRow};
use crate::utils::output_image_source;

#[component]
pub fn ImageDemo() -> RsxNode {
    let (theme, _) = use_theme();
    let image = |file_name: &str, fit: ImageFit| {
        rsx! {
            <Image
                source={output_image_source(file_name)}
                fit={fit}
                style={{
                    width: Length::px(180.0),
                    height: Length::px(120.0),
                    background: theme.color.layer.surface.clone(),
                    border: Border::uniform(Length::px(1.0), theme.color.divider.as_ref()),
                    border_radius: theme.radius.md,
                }}
                loading={rsx! {
                    <Element style={{
                        width: Length::percent(100.0),
                        height: Length::percent(100.0),
                        color: theme.color.text.secondary.clone(),
                        padding: Padding::uniform(theme.spacing.sm),
                    }}>
                        <Text>{"Loading…"}</Text>
                    </Element>
                }}
                error={rsx! {
                    <Element style={{
                        width: Length::percent(100.0),
                        height: Length::percent(100.0),
                        background: Color::hex("#b91c1c"),
                        color: theme.color.background.base.clone(),
                        padding: Padding::uniform(theme.spacing.sm),
                    }}>
                        <Text>Failed to load</Text>
                    </Element>
                }}
            />
        }
    };
    rsx! {
        <Demo
            title="Image"
            description="PNG and JPEG images load in the background and fit their box. Until then, or if loading fails, the loading and error slots render instead."
        >
            <SampleRow>
                <Sample label="fit = Contain">{image("rfgui-logo.png", ImageFit::Contain)}</Sample>
                <Sample label="fit = Cover">{image("rfgui-logo.png", ImageFit::Cover)}</Sample>
                <Sample label="A missing file shows the error slot">{image("missing.png", ImageFit::Contain)}</Sample>
            </SampleRow>
        </Demo>
    }
}
