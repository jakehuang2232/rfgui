extern crate rfgui;
extern crate rfgui_components;

#[path = "../app.rs"]
mod app;
#[path = "../components.rs"]
mod components;
#[path = "../platform.rs"]
mod platform;
#[path = "../scene.rs"]
mod scene;
#[path = "../scene_windows/mod.rs"]
mod scene_windows;
#[path = "../state.rs"]
mod state;
#[path = "../utils.rs"]
mod utils;
#[path = "../window_manager.rs"]
mod window_manager;

fn main() {
    app::run();
}
