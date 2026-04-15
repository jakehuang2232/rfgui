extern crate rfgui;
extern crate rfgui_components;

mod scene;

use rfgui::Color;
use rfgui::app::{App, AppConfig, AppContext, WheelConfig};
use rfgui::ui::{RsxNode, rsx};

use crate::scene::MainScene;

struct HelloWorldApp;

impl App for HelloWorldApp {
    fn build(&mut self, _ctx: &mut AppContext<'_>) -> RsxNode {
        rsx! { <MainScene /> }
    }
}

fn make_config() -> AppConfig {
    AppConfig {
        title: String::from("RFGUI Example"),
        initial_size: (1280, 800),
        scale_factor: None,
        transparent: false,
        clear_color: Some(Color::rgb(40, 44, 52)),
        wheel: WheelConfig::default(),
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    examples::winit_runner::run(HelloWorldApp, make_config());
}

#[cfg(target_arch = "wasm32")]
fn main() {
    examples::web_runner::run(HelloWorldApp, make_config());
}
