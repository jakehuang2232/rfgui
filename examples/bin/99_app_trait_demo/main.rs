//! Minimal end-to-end demo of the phase-5 `App` trait + phase-6 winit
//! runner. Exists to prove the stack wires up and give new examples a
//! copy-paste starting point. Does nothing beyond rendering a centered
//! "Hello from App trait" label on a red background.

extern crate rfgui;
extern crate rfgui_components;

use rfgui::app::{App, AppConfig, AppContext, WheelConfig};
use rfgui::ui::{RsxNode, component, rsx};
use rfgui::{Align, Element, JustifyContent, Layout, Length};

#[component]
fn DemoScene() -> RsxNode {
    rsx! {
        <Element
            style={{
                width: Length::percent(100.0),
                height: Length::percent(100.0),
                layout: Layout::flex().align(Align::Center).justify_content(JustifyContent::Center),
                background_color: "#b33",
                color: "#fff",
                font_size: 48.0,
            }}>
            Hello from App trait
        </Element>
    }
}

struct DemoApp;

impl App for DemoApp {
    fn build(&mut self, _ctx: &mut AppContext<'_>) -> RsxNode {
        rsx! { <DemoScene /> }
    }
}

fn make_config() -> AppConfig {
    AppConfig {
        title: String::from("rfgui App trait demo"),
        initial_size: (800, 600),
        scale_factor: None,
        transparent: false,
        clear_color: None,
        wheel: WheelConfig::default(),
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    examples::winit_runner::run(DemoApp, make_config());
}

#[cfg(target_arch = "wasm32")]
fn main() {
    examples::web_runner::run(DemoApp, make_config());
}
