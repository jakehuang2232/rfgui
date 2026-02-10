use std::sync::Arc;

use rust_gui::Viewport;
use rust_gui::ui::host::{Element, Text};
use rust_gui::ui::{RsxNode, rsx};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

#[derive(Default)]
struct App {
    window: Option<Arc<Window>>,
    viewport: Option<Viewport>,
    app: Option<RsxNode>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = Arc::new(
            event_loop
                .create_window(Window::default_attributes())
                .unwrap(),
        );
        let mut viewport = Viewport::new();
        let size = window.inner_size();
        viewport.set_size(size.width, size.height);
        pollster::block_on(viewport.set_window(window.clone()));
        pollster::block_on(viewport.create_surface());

        self.window = Some(window);
        self.viewport = Some(viewport);
        self.app = Some(rsx! {
            <Element x=40 y=40 width=240 height=140 background="#4CC9F0" border_color="#1D3557" border_width=8 border_radius=10>
                <Element x=24 y=24 width=72 height=48 background="#FFD166" border_color="#EF476F" border_width=3 />
                <Element x=180 y=80 width=120 height=80 background="#F72585" border_color="#B5179E" border_width=4 />
                <Text x=16 y=16 font_size=26 font="Noto Sans CJK TC">Hello Rust GUI Text 測試</Text>
            </Element>
        });
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
                self.window = None;
            }
            WindowEvent::Resized(size) => {
                if let (Some(window), Some(viewport)) = (&self.window, &mut self.viewport) {
                    viewport.set_size(size.width, size.height);
                    window.request_redraw();
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                if let (Some(window), Some(viewport)) = (&self.window, &mut self.viewport) {
                    viewport.set_scale_factor(scale_factor as f32);
                    let size: PhysicalSize<u32> = window.inner_size();
                    viewport.set_size(size.width, size.height);
                    window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => {
                if let (Some(viewport), Some(app)) =
                    (&mut self.viewport, &self.app)
                {
                    let _ = viewport.render_rsx(&app);
                }
            }
            _ => (),
        }
    }
}

fn main() {
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App::default();
    event_loop.run_app(&mut app).unwrap();
}
