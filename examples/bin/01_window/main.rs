extern crate rfgui;
extern crate rfgui_components;

mod components;
mod scene;
mod scene_windows;
mod utils;
mod window_manager;

use rfgui::Color;
use rfgui::app::{App, AppConfig, AppContext, WheelConfig};
use rfgui::ui::{RsxNode, rsx};

use crate::scene::MainScene;

struct WindowDemoApp;

impl App for WindowDemoApp {
    fn build(&mut self, _ctx: &mut AppContext<'_>) -> RsxNode {
        rsx! { <MainScene /> }
    }
}

fn make_config() -> AppConfig {
    AppConfig {
        title: String::from("RFGUI Window Demo"),
        initial_size: (1280, 800),
        scale_factor: None,
        transparent: false,
        clear_color: Some(Color::rgb(40, 44, 52)),
        wheel: WheelConfig::default(),
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    scene_windows::particle_demo::register_particle_canvas();
    examples::winit_runner::run(WindowDemoApp, make_config());
}

#[cfg(target_arch = "wasm32")]
fn main() {
    scene_windows::particle_demo::register_particle_canvas();
    examples::web_runner::run(WindowDemoApp, make_config());
}
