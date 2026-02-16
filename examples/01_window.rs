use rust_gui::{Transition, TransitionProperty};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

use rust_gui::ui::host::{Element, Text, TextArea};
use rust_gui::ui::{RsxNode, component, on_click, rsx};
use rust_gui::{
    Border, BorderRadius, Color, Display, FlowDirection, FlowWrap, HexColor, Length, Padding,
    ScrollDirection, Viewport,
};
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, Ime, MouseScrollDelta, WindowEvent};
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
            flow_direction: FlowDirection::Row,
            flow_wrap: FlowWrap::Wrap,
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
                width: Length::px(150.0),
                height: Length::px(150.0),
                background: "#61afef",
                border_radius: BorderRadius::uniform(Length::px(10.0))
                    .top_right(Length::px(32.0))
                    .bottom_left(Length::percent(90.0)),
            }}>
                圓角
            </Element>
            <Element style={{
                width: Length::percent(50.0),
                height: Length::px(150.0),
                background: "#61afef",
                border_radius: BorderRadius::uniform(Length::px(10.0))
                    .top_right(Length::px(32.0))
                    .bottom_left(Length::percent(90.0)),
                transition: [
                    Transition::new(TransitionProperty::All, 1000),
                ]
            }}>
                百分比寬度
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
                    Transition::new(TransitionProperty::BorderColor, 1000).ease_in_out()
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
                flow_direction: FlowDirection::Row,
                flow_wrap: FlowWrap::Wrap,
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
            <Element style={{
                width: Length::px(150.0),
                height: Length::px(150.0),
                background: "#61afef7f",
                border: Border::uniform(Length::px(3.0), &Color::hex("#21252b")),
                border_radius: 16,
                scroll_direction: ScrollDirection::Vertical,
            }}>
                <Text x=10 y=10 font_size=16 color="#21252b" font="Noto Sans CJK TC">
                    背景透明度嵌套
                </Text>
                <Element style={{ width: Length::px(110.0), height: Length::px(86.0), background: "#e06c75", border: Border::uniform(Length::px(2.0), &Color::hex("#21252b")), border_radius: 12, opacity: 1 }}>
                    <Element style={{
                        width: Length::px(72.0), height: Length::px(52.0), background: "#61afef", border: Border::uniform(Length::px(2.0), &Color::hex("#21252b")), border_radius: 8, opacity: 0.5 }}>
                        <Text x=8 y=16 font_size=12 color="#ffffff" font="Noto Sans CJK TC" opacity=0.9>
                            Alpha
                        </Text>
                    </Element>
                </Element>
            </Element>
            <Element style={{
                width: Length::px(150.0),
                height: Length::px(150.0),
                background: "#61afef",
                border: Border::uniform(Length::px(3.0), &Color::hex("#21252b")),
                border_radius: 16,
                scroll_direction: ScrollDirection::Vertical,
                display: Display::Flow,
                flow_direction: FlowDirection::Column,
            }}>
                <Text font_size=12 color="#111111" font="Noto Sans CJK TC">向下滾動可看到更多內容 1</Text>
                <Text font_size=12 color="#111111" font="Noto Sans CJK TC">向下滾動可看到更多內容 2</Text>
                <Text font_size=12 color="#111111" font="Noto Sans CJK TC">向下滾動可看到更多內容 3</Text>
                <Text font_size=12 color="#111111" font="Noto Sans CJK TC">向下滾動可看到更多內容 4</Text>
                <Text font_size=12 color="#111111" font="Noto Sans CJK TC">向下滾動可看到更多內容 5</Text>
                <Text font_size=12 color="#111111" font="Noto Sans CJK TC">向下滾動可看到更多內容 6</Text>
                <Text font_size=12 color="#111111" font="Noto Sans CJK TC">向下滾動可看到更多內容 7</Text>
                <Text font_size=12 color="#111111" font="Noto Sans CJK TC">向下滾動可看到更多內容 8</Text>
                <Text font_size=12 color="#111111" font="Noto Sans CJK TC">向下滾動可看到更多內容 9</Text>
                <Text font_size=12 color="#111111" font="Noto Sans CJK TC">向下滾動可看到更多內容 10</Text>
                <Text font_size=12 color="#111111" font="Noto Sans CJK TC">向下滾動可看到更多內容 11</Text>
                <Text font_size=12 color="#111111" font="Noto Sans CJK TC">向下滾動可看到更多內容 12</Text>
                <Text font_size=12 color="#111111" font="Noto Sans CJK TC">向下滾動可看到更多內容 13</Text>

            </Element>
            <Element style={{ width: Length::px(320.0), height: Length::px(170.0), background: "#2c313c", border: Border::uniform(Length::px(3.0), &Color::hex("#61afef")), border_radius: 16 }}>
                <Text x=12 y=10 font_size=14 color="#e5e9f0" font="Noto Sans CJK TC">TextArea 測試</Text>
                <TextArea x=12 y=34 width=296 height=54 font_size=13 color="#c8d0e0" font="Noto Sans CJK TC" multiline=true placeholder="請輸入多行內容...">
                    第一行: multiline=true
                    第二行: 保留換行
                    這是一行很長的內容test test test test test test test test test test test test test
                </TextArea>
                <TextArea x=12 y=98 width=296 height=26 font_size=13 color="#c8d0e0" font="Noto Sans CJK TC" multiline=false read_only=true>
                    multiline=false
                    換行應該會被轉成空白
                </TextArea>
            </Element>
        </Element>
    }
}

#[derive(Default)]
struct App {
    window: Option<Arc<Window>>,
    viewport: Option<Viewport>,
    app: Option<RsxNode>,
    ime_dirty: bool,
    last_ime_focus_id: Option<u64>,
    last_ime_allowed: bool,
    last_ime_area: Option<(i32, i32, u32, u32)>,
}

impl App {
    fn rebuild_app(&mut self) {
        self.app = Some(rsx! { <MainScene /> });
    }

    fn mark_ime_dirty(&mut self) {
        self.ime_dirty = true;
    }

    fn sync_ime_state(&mut self, force: bool) {
        let (Some(window), Some(viewport)) = (&self.window, &self.viewport) else {
            return;
        };

        let focused_id = viewport.focused_node_id();
        if !force && !self.ime_dirty && focused_id == self.last_ime_focus_id {
            return;
        }
        self.last_ime_focus_id = focused_id;

        let mut next_area = None;
        if let Some((x, y, w, h)) = viewport.focused_ime_cursor_rect() {
            next_area = Some((
                x.max(0.0).round() as i32,
                y.max(0.0).round() as i32,
                w.max(1.0).ceil() as u32,
                h.max(1.0).ceil() as u32,
            ));
        }
        let next_allowed = next_area.is_some();

        if self.last_ime_allowed != next_allowed {
            window.set_ime_allowed(next_allowed);
            self.last_ime_allowed = next_allowed;
        }

        if let Some((x, y, w, h)) = next_area {
            if self.last_ime_area != Some((x, y, w, h)) {
                let pos = PhysicalPosition::new(x, y);
                let size = PhysicalSize::new(w, h);
                window.set_ime_cursor_area(pos, size);
            }
        }
        self.last_ime_area = next_area;
        self.ime_dirty = false;
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
        window.set_ime_allowed(false);

        self.window = Some(window);
        self.viewport = Some(viewport);
        self.ime_dirty = true;
        self.last_ime_focus_id = None;
        self.last_ime_allowed = false;
        self.last_ime_area = None;
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
                self.mark_ime_dirty();
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                if let (Some(window), Some(viewport)) = (&self.window, &mut self.viewport) {
                    viewport.set_scale_factor(scale_factor as f32);
                    let size: PhysicalSize<u32> = window.inner_size();
                    viewport.set_size(size.width, size.height);
                    viewport.request_redraw();
                }
                self.mark_ime_dirty();
            }
            WindowEvent::RedrawRequested => {
                if let (Some(viewport), Some(app)) = (&mut self.viewport, &self.app) {
                    let _ = viewport.render_rsx(&app);
                }
                self.mark_ime_dirty();
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
                self.mark_ime_dirty();
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
                self.mark_ime_dirty();
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
                self.mark_ime_dirty();
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                if let Some(viewport) = &mut self.viewport {
                    viewport.dispatch_text_input_event(text);
                }
                self.mark_ime_dirty();
            }
            WindowEvent::Ime(Ime::Preedit(text, cursor)) => {
                if let Some(viewport) = &mut self.viewport {
                    viewport.dispatch_ime_preedit_event(text, cursor);
                }
                self.mark_ime_dirty();
            }
            WindowEvent::Ime(Ime::Disabled) => {
                if let Some(viewport) = &mut self.viewport {
                    viewport.dispatch_ime_preedit_event(String::new(), None);
                }
                self.mark_ime_dirty();
            }
            WindowEvent::Focused(false) => {
                if let Some(viewport) = &mut self.viewport {
                    viewport.clear_input_state();
                }
                self.mark_ime_dirty();
            }
            _ => (),
        }

        if let (Some(window), Some(viewport)) = (&self.window, &mut self.viewport) {
            if viewport.take_redraw_request() {
                window.request_redraw();
            }
        }
        self.sync_ime_state(false);
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
