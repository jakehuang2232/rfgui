pub mod particle_demo;

use crate::rfgui::style::{Border, Length};
use crate::rfgui::ui::{Binding, RsxNode, component, rsx};
use crate::rfgui::view::Element;
use crate::rfgui_components::use_theme;
use crate::showcase::Demo;
use particle_demo::ParticleDemo;

#[component]
pub fn CustomGpuPage(animation_on: &Binding<bool>) -> RsxNode {
    let (theme, _) = use_theme();
    rsx! {
        <Demo
            title="Particles in a WGSL shader"
            description="ParticleCanvas is a host element of its own. It simulates a thousand particles on the CPU and draws them with one WGSL draw, which the engine renders into a texture and composites with the rest of the page. Hold the left button over it to pull the particles in."
        >
            <Element style={{
                width: Length::percent(100.0),
                height: Length::px(420.0),
                background: "#05070d",
                border: Border::uniform(Length::px(1.0), theme.color.divider.as_ref()),
                border_radius: theme.radius.md,
            }}>
                <ParticleDemo animation_on={animation_on.clone()} />
            </Element>
        </Demo>
    }
}
