extern crate rfgui;
extern crate rfgui_components;

mod app;
mod components;
mod platform;
mod scene;
mod scene_windows;
mod state;
mod utils;
mod window_manager;

fn main() {
    app::run();
}
