use crate::rfgui::style::{Border, Length};
use crate::rfgui::ui::{RsxNode, component, rsx};
use crate::rfgui::view::{ImageFit, Svg, SvgSource};
use crate::rfgui_components::use_theme;
use crate::showcase::{Demo, Sample, SampleRow};

const PICTURE: &str = r##"<svg width="160" height="120" viewBox="0 0 160 120" xmlns="http://www.w3.org/2000/svg">
<rect x="8" y="8" width="144" height="104" rx="20" fill="#0f766e"/>
<circle cx="48" cy="42" r="18" fill="#99f6e4"/>
<path d="M38 84 L72 58 L98 80 L122 46" fill="none" stroke="#f0fdfa" stroke-width="10" stroke-linecap="round" stroke-linejoin="round"/>
<rect x="94" y="72" width="34" height="24" rx="8" fill="#f59e0b"/>
</svg>"##;

#[component]
pub fn SvgDemo() -> RsxNode {
    let (theme, _) = use_theme();
    let svg = |width: f32, height: f32| {
        rsx! {
            <Svg
                source={SvgSource::Content(PICTURE.to_string())}
                fit={ImageFit::Contain}
                style={{
                    width: Length::px(width),
                    height: Length::px(height),
                    background: theme.color.layer.surface.clone(),
                    border: Border::uniform(Length::px(1.0), theme.color.divider.as_ref()),
                    border_radius: theme.radius.md,
                }}
            />
        }
    };
    rsx! {
        <Demo
            title="SVG"
            description="SVG documents are rasterized at the size and scale they are shown at, so they stay sharp when enlarged."
        >
            <SampleRow>
                <Sample label="80 × 60">{svg(80.0, 60.0)}</Sample>
                <Sample label="180 × 120">{svg(180.0, 120.0)}</Sample>
                <Sample label="320 × 200">{svg(320.0, 200.0)}</Sample>
            </SampleRow>
        </Demo>
    }
}
