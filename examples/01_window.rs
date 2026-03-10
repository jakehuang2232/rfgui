extern crate rfgui;
extern crate rfgui_components;

#[path = "01_window/app.rs"]
mod app;
#[path = "01_window/components.rs"]
mod components;
#[path = "01_window/platform.rs"]
mod platform;
#[path = "01_window/scene.rs"]
mod scene;
#[path = "01_window/scene_windows/mod.rs"]
mod scene_windows;
#[path = "01_window/state.rs"]
mod state;
#[path = "01_window/utils.rs"]
mod utils;
#[path = "01_window/window_manager.rs"]
mod window_manager;

fn main() {
    app::run();
}
