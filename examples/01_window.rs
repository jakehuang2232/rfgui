use rust_gui::{Transition, TransitionProperty};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

use rust_gui::ui::host::{Element, Text};
use rust_gui::ui::{RsxNode, component, on_click, rsx};
use rust_gui::{
    Border, BorderRadius, Color, Display, FlexDirection, FlexWrap, HexColor, Length, Padding,
    ScrollDirection, Viewport,
};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::Key;
use winit::window::{Window, WindowId};

#[component]
fn MainScene() -> RsxNode {
    let click_count = MAIN_SCENE_CLICK_COUNT.load(Ordering::Relaxed);
    let increment = on_click(|event| {
        MAIN_SCENE_CLICK_COUNT.fetch_add(1, Ordering::Relaxed);
        MAIN_SCENE_DIRTY.store(true, Ordering::Release);
        event.meta.stop_propagation();
    });
    rsx! {
        <Element style={{
            width: Length::percent(100.0),
            height: Length::percent(100.0),
            display: Display::Flow,
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::Wrap,
            gap: Length::px(24.0),
            padding: Padding::uniform(Length::px(20.0)),
            scroll_direction: ScrollDirection::Vertical,
        }}>
            <Element style={{
                width: Length::px(150.0),
                height: Length::px(150.0),
                background: "#61afef",
                padding: Padding::uniform(Length::px(10.0)),
            }}>
                純物件
            </Element>
            <Element style={{ width: Length::px(150.0), height: Length::px(150.0), background: "#61afef", border: Border::uniform(Length::px(10.0), &Color::hex("#21252b")) }}>
                外框
            </Element>
            <Element style={{
                width: Length::px(100.0),
                height: Length::px(100.0),
                background: "#61afef",
                border_radius: BorderRadius::uniform(Length::px(10.0))
                    .top_right(Length::px(32.0))
                    .bottom_left(Length::percent(90.0)),
            }}>
                圓角
            </Element>
            <Element style={{
                width: Length::px(150.0),
                height: Length::px(150.0),
                background: "#61afef",
                border: Border::uniform(Length::px(10.0), &Color::hex("#21252b"))
                    .top(Some(Length::px(20.0)), None),
                border_radius: 16 }}>
                圓角 + 外框
            </Element>
            <Element style={{
                width: Length::px(170.0),
                height: Length::px(170.0),
                background: "#e06c75",
                border: Border::uniform(Length::px(20.0), &Color::hex("#21252b")),
                border_radius: 16,
                hover: {
                    border: Border::uniform(Length::px(20.0), &Color::hex("#61afef")),
                },
                transition: [
                    Transition::new(TransitionProperty::Position, 1000).ease_in_out(),
                ],
            }}>
                <Element style={{
                    width: Length::percent(100.0),
                    height: Length::percent(100.0),
                    background: "#61afef",
                    border: Border::uniform(Length::px(20.0), &Color::hex("#3e4451")),
                    border_radius: 0,
                    hover: {
                        background: "#7bc1ff",
                        border: Border::uniform(Length::px(20.0), &Color::hex("#1f2937"))
                    },
                    transition: [
                        Transition::new(TransitionProperty::All, 200),
                    ],
                }}>
                    嵌套 + hover 測試
                </Element>
            </Element>
            <Element style={{
                width: Length::px(150.0),
                height: Length::px(150.0),
                background: "#61afef",
                border: Border::uniform(Length::px(8.0), &Color::hex("#21252b")),
                border_radius: 50,
                display: Display::Flow,
                flex_direction: FlexDirection::Row,
                flex_wrap: FlexWrap::Wrap,
                gap: Length::px(8.0),
                padding: Padding::uniform(Length::px(8.0)),
            }}>
                <Element style={{ width: Length::px(72.0), height: Length::px(48.0), background: "#d19a66", border: Border::uniform(Length::px(3.0), &Color::hex("#e06c75")) }}>
                    Clip 測試
                </Element>
                <Element style={{ width: Length::px(56.0), height: Length::px(56.0), background: "#61afef" }} />
                <Element style={{ width: Length::px(120.0), height: Length::px(64.0), background: "#c678dd", border: Border::uniform(Length::px(4.0), &Color::hex("#56b6c2")) }} />
            </Element>
            <Element style={{ width: Length::px(150.0), height: Length::px(150.0), background: "#282c34", border: Border::uniform(Length::px(3.0), &Color::hex("#61afef")), border_radius: 16 }} >
                <Text x=12 y=10 font_size=22 color="#abb2bf" font="Noto Sans CJK TC">
                    按鈕測試
                </Text>
                <Text x=12 y=48 font_size=14 color="#abb2bf" font="Noto Sans CJK TC">{format!("Click 計數: {}", click_count)}</Text>
                <Element style={{ width: Length::px(48.0), height: Length::px(48.0), background: "#98c379", border: Border::uniform(Length::px(2.0), &Color::hex("#21252b")), border_radius: 20 }} on_click={increment}>
                    Click Me
                </Element>
            </Element>
            <Element style={{ width: Length::px(150.0), height: Length::px(150.0), background: "#61afef", border: Border::uniform(Length::px(3.0), &Color::hex("#21252b")), border_radius: 16, opacity: 0.5 }}>
                <Text x=10 y=10 font_size=16 color="#21252b" font="Noto Sans CJK TC">
                    透明度嵌套
                </Text>
                <Element style={{ width: Length::px(110.0), height: Length::px(86.0), background: "#e06c75", border: Border::uniform(Length::px(2.0), &Color::hex("#21252b")), border_radius: 12, opacity: 0.6 }}>
                    <Element style={{
                        width: Length::px(72.0), height: Length::px(52.0), background: "#61afef", border: Border::uniform(Length::px(2.0), &Color::hex("#21252b")), border_radius: 8, opacity: 0.5 }}>
                        <Text x=8 y=16 font_size=12 color="#ffffff" font="Noto Sans CJK TC" opacity=0.9>
                            Alpha
                        </Text>
                    </Element>
                </Element>
            </Element>
            <Element style={{ width: Length::px(150.0), height: Length::px(150.0), background: "#61afef7f", border: Border::uniform(Length::px(3.0), &Color::hex("#21252b")), border_radius: 16, scroll_direction: ScrollDirection::Vertical }}>
                <Text x=10 y=10 font_size=16 color="#21252b" font="Noto Sans CJK TC">
                    背景透明度嵌套
                </Text>
                <Text x=10 y=34 font_size=12 color="#111111" font="Noto Sans CJK TC">向下滾動可看到更多內容 1</Text>
                <Text x=10 y=52 font_size=12 color="#111111" font="Noto Sans CJK TC">向下滾動可看到更多內容 2</Text>
                <Text x=10 y=70 font_size=12 color="#111111" font="Noto Sans CJK TC">向下滾動可看到更多內容 3</Text>
                <Text x=10 y=88 font_size=12 color="#111111" font="Noto Sans CJK TC">向下滾動可看到更多內容 4</Text>
                <Text x=10 y=106 font_size=12 color="#111111" font="Noto Sans CJK TC">向下滾動可看到更多內容 5</Text>
                <Text x=10 y=124 font_size=12 color="#111111" font="Noto Sans CJK TC">向下滾動可看到更多內容 6</Text>
                <Text x=10 y=142 font_size=12 color="#111111" font="Noto Sans CJK TC">向下滾動可看到更多內容 7</Text>
                <Element style={{ width: Length::px(110.0), height: Length::px(86.0), background: "#e06c75", border: Border::uniform(Length::px(2.0), &Color::hex("#21252b")), border_radius: 12, opacity: 1 }}>
                    <Element style={{
                        width: Length::px(72.0), height: Length::px(52.0), background: "#61afef", border: Border::uniform(Length::px(2.0), &Color::hex("#21252b")), border_radius: 8, opacity: 0.5 }}>
                        <Text x=8 y=16 font_size=12 color="#ffffff" font="Noto Sans CJK TC" opacity=0.9>
                            Alpha
                        </Text>
                    </Element>
                </Element>
            </Element>
        </Element>
    }
}

#[derive(Default)]
struct App {
    window: Option<Arc<Window>>,
    viewport: Option<Viewport>,
    app: Option<RsxNode>,
}

impl App {
    fn rebuild_app(&mut self) {
        self.app = Some(rsx! { <MainScene /> });
    }
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
        viewport.set_clear_color(Box::new(HexColor::new("#282c34")));
        pollster::block_on(viewport.set_window(window.clone()));
        pollster::block_on(viewport.create_surface());

        self.window = Some(window);
        self.viewport = Some(viewport);
        self.rebuild_app();
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
                if let Some(viewport) = &mut self.viewport {
                    viewport.set_size(size.width, size.height);
                    viewport.request_redraw();
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                if let (Some(window), Some(viewport)) = (&self.window, &mut self.viewport) {
                    viewport.set_scale_factor(scale_factor as f32);
                    let size: PhysicalSize<u32> = window.inner_size();
                    viewport.set_size(size.width, size.height);
                    viewport.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => {
                if let (Some(viewport), Some(app)) = (&mut self.viewport, &self.app) {
                    let _ = viewport.render_rsx(&app);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                if let Some(viewport) = &mut self.viewport {
                    viewport.set_mouse_position_viewport(position.x as f32, position.y as f32);
                    viewport.dispatch_mouse_move_event();
                }
            }
            WindowEvent::CursorLeft { .. } => {
                if let Some(viewport) = &mut self.viewport {
                    viewport.clear_mouse_position_viewport();
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if let Some(viewport) = &mut self.viewport {
                    let (dx, dy) = match delta {
                        MouseScrollDelta::LineDelta(x, y) => (x * 24.0, y * 24.0),
                        MouseScrollDelta::PixelDelta(pos) => (pos.x as f32, pos.y as f32),
                    };
                    viewport.dispatch_mouse_wheel_event(-dx, -dy);
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let mut should_rebuild = false;
                if let Some(viewport) = &mut self.viewport {
                    let button = map_mouse_button(button);
                    viewport.set_mouse_button_pressed(button, state == ElementState::Pressed);
                    if state == ElementState::Pressed {
                        viewport.dispatch_mouse_down_event(button);
                    } else {
                        viewport.dispatch_mouse_up_event(button);
                        viewport.dispatch_click_event(button);
                        if MAIN_SCENE_DIRTY.swap(false, Ordering::AcqRel) {
                            should_rebuild = true;
                        }
                    }
                }
                if should_rebuild {
                    self.rebuild_app();
                    if let Some(viewport) = &mut self.viewport {
                        viewport.request_redraw();
                    }
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let Some(viewport) = &mut self.viewport {
                    let key = key_to_string(&event.logical_key);
                    let pressed = event.state == ElementState::Pressed;
                    viewport.set_key_pressed(key.clone(), pressed);
                    let code = format!("{:?}", event.physical_key);
                    if pressed {
                        viewport.dispatch_key_down_event(key, code, event.repeat);
                    } else {
                        viewport.dispatch_key_up_event(key, code, event.repeat);
                    }
                }
            }
            WindowEvent::Focused(false) => {
                if let Some(viewport) = &mut self.viewport {
                    viewport.clear_input_state();
                }
            }
            _ => (),
        }

        if let (Some(window), Some(viewport)) = (&self.window, &mut self.viewport) {
            if viewport.take_redraw_request() {
                window.request_redraw();
            }
        }
    }
}

fn map_mouse_button(button: winit::event::MouseButton) -> rust_gui::MouseButton {
    match button {
        winit::event::MouseButton::Left => rust_gui::MouseButton::Left,
        winit::event::MouseButton::Right => rust_gui::MouseButton::Right,
        winit::event::MouseButton::Middle => rust_gui::MouseButton::Middle,
        winit::event::MouseButton::Back => rust_gui::MouseButton::Back,
        winit::event::MouseButton::Forward => rust_gui::MouseButton::Forward,
        winit::event::MouseButton::Other(v) => rust_gui::MouseButton::Other(v),
    }
}

fn key_to_string(key: &Key) -> String {
    match key {
        Key::Character(text) => text.to_string(),
        _ => format!("{key:?}"),
    }
}

fn main() {
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App::default();
    event_loop.run_app(&mut app).unwrap();
}

static MAIN_SCENE_CLICK_COUNT: AtomicI32 = AtomicI32::new(0);
static MAIN_SCENE_DIRTY: AtomicBool = AtomicBool::new(false);
