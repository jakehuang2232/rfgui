use rfgui::{Transition, TransitionProperty};
use std::sync::Arc;

use rfgui::ui::host::{
    Button, Checkbox, Element, NumberField, Select, Slider, Switch, Text, TextArea,
};
use rfgui::ui::{RsxNode, component, globalState, on_click, rsx, take_state_dirty, use_state};
use rfgui::{
    Border, BorderRadius, Color, Display, FlowDirection, FlowWrap, FontFamily, HexColor, Length,
    Padding, ScrollDirection, Viewport,
};
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, Ime, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::Key;
use winit::window::{Window, WindowId};

#[component]
fn MainScene() -> RsxNode {
    let click_count = globalState(|| 0_i32);
    let message = use_state(|| String::from("Line 1: multiline=true\nLine 2: keep editing\n中文字測試"));
    let checked = use_state(|| true);
    let number_value = use_state(|| 3.0_f64);
    let selected_index = use_state(|| 0_usize);
    let slider_value = use_state(|| 42.0_f64);
    let switch_on = use_state(|| false);

    let click_count_value = click_count.get();
    let message_value = message.get();
    let checked_value = checked.get();
    let number_value_value = number_value.get();
    let selected_index_value = selected_index.get();
    let slider_value_value = slider_value.get();
    let switch_on_value = switch_on.get();
    let increment_state = click_count.clone();
    let increment = on_click(move |event| {
        increment_state.update(|v| *v += 1);
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
            font: FontFamily::new(["Heiti TC", "Noto Sans CJK TC", "PingFang TC"]),
        }}>
            <Element style={{
                width: Length::px(150.0),
                height: Length::px(150.0),
                background: "#61afef",
                padding: Padding::uniform(Length::px(10.0)),
            }}>
                Pure Object
            </Element>
            <Element style={{
                width: Length::px(150.0),
                height: Length::px(150.0),
                background: "#61afef",
                border: Border::uniform(Length::px(10.0), &Color::hex("#21252b")),
            }}>
                Border
            </Element>
            <Element style={{
                width: Length::px(150.0),
                height: Length::px(150.0),
                background: "#61afef",
                border_radius: BorderRadius::uniform(Length::px(10.0))
                    .top_right(Length::px(32.0))
                    .bottom_left(Length::percent(90.0)),
            }}>
                Border Radius
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
                Percentage Width
            </Element>
            <Element style={{
                width: Length::px(150.0),
                height: Length::px(150.0),
                background: "#61afef",
                border: Border::uniform(Length::px(10.0), &Color::hex("#21252b"))
                    .top(Some(Length::px(20.0)), None),
                border_radius: BorderRadius::uniform(Length::px(10.0))
                    .top_right(Length::px(10.0))
                    .bottom_left(Length::percent(90.0)),
            }}>
                Border Radius + Border
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
                    Nested + Hover Test
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
                    Clip Test
                </Element>
                <Element style={{ width: Length::px(56.0), height: Length::px(56.0), background: "#61afef" }} />
                <Element style={{ width: Length::px(120.0), height: Length::px(64.0), background: "#c678dd", border: Border::uniform(Length::px(4.0), &Color::hex("#56b6c2")) }} />
            </Element>
            <Element style={{
                width: Length::px(150.0),
                height: Length::px(150.0),
                background: "#282c34",
                border: Border::uniform(Length::px(3.0), &Color::hex("#61afef")),
                border_radius: 16,
                display: Display::Flow,
                flow_wrap: FlowWrap::Wrap,
                gap: Length::px(8.0),
                padding: Padding::uniform(Length::px(8.0)),
            }} >
                <Text font_size=22 color="#abb2bf" >
                    Button Test
                </Text>
                <Text font_size=14 color="#abb2bf" >{format!("Click Count: {}", click_count_value)}</Text>
                <Button
                    label="Click Me"
                    variant="contained"
                    on_click={increment}
                />
            </Element>
            <Element style={{ width: Length::px(150.0), height: Length::px(150.0), background: "#61afef", border: Border::uniform(Length::px(3.0), &Color::hex("#21252b")), border_radius: 16, opacity: 0.5 }}>
                <Text x=10 y=10 font_size=16 color="#21252b" >
                    Opacity Nesting
                </Text>
                <Element style={{ width: Length::px(110.0), height: Length::px(86.0), background: "#e06c75", border: Border::uniform(Length::px(2.0), &Color::hex("#21252b")), border_radius: 12, opacity: 0.6 }}>
                    <Element style={{
                        width: Length::px(72.0), height: Length::px(52.0), background: "#61afef", border: Border::uniform(Length::px(2.0), &Color::hex("#21252b")), border_radius: 8, opacity: 0.5 }}>
                        <Text x=8 y=16 font_size=12 color="#ffffff"  opacity=0.9>
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
                <Text x=10 y=10 font_size=16 color="#21252b" >
                    Background Opacity Nesting
                </Text>
                <Element style={{ width: Length::px(110.0), height: Length::px(86.0), background: "#e06c75", border: Border::uniform(Length::px(2.0), &Color::hex("#21252b")), border_radius: 12, opacity: 1 }}>
                    <Element style={{
                        width: Length::px(72.0), height: Length::px(52.0), background: "#61afef", border: Border::uniform(Length::px(2.0), &Color::hex("#21252b")), border_radius: 8, opacity: 0.5 }}>
                        <Text x=8 y=16 font_size=12 color="#ffffff"  opacity=0.9>
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
                <Text font_size=12 color="#111111" >Scroll down to see more content 1</Text>
                <Text font_size=12 color="#111111" >Scroll down to see more content 2</Text>
                <Text font_size=12 color="#111111" >Scroll down to see more content 3</Text>
                <Text font_size=12 color="#111111" >Scroll down to see more content 4</Text>
                <Text font_size=12 color="#111111" >Scroll down to see more content 5</Text>
                <Text font_size=12 color="#111111" >Scroll down to see more content 6</Text>
                <Text font_size=12 color="#111111" >Scroll down to see more content 7</Text>
                <Text font_size=12 color="#111111" >Scroll down to see more content 8</Text>
                <Text font_size=12 color="#111111" >Scroll down to see more content 9</Text>
                <Text font_size=12 color="#111111" >Scroll down to see more content 10</Text>
                <Text font_size=12 color="#111111" >Scroll down to see more content 11</Text>
                <Text font_size=12 color="#111111" >Scroll down to see more content 12</Text>
                <Text font_size=12 color="#111111" >Scroll down to see more content 13</Text>

            </Element>
            <Element style={{ width: Length::px(320.0), height: Length::px(170.0), background: "#2c313c", border: Border::uniform(Length::px(3.0), &Color::hex("#61afef")), border_radius: 16 }}>
                <Text x=12 y=10 font_size=14 color="#e5e9f0" >TextArea Test</Text>
                <Text x=12 y=118 font_size=12 color="#aab4c6" >
                    {format!("Bound chars: {}", message_value.chars().count())}
                </Text>
                <TextArea x=12 y=34 width=296 height=78 font_size=13 color="#c8d0e0"  multiline=true placeholder="Please enter multiline content..." binding={message.binding()} />
                <TextArea x=12 y=98 width=296 height=26 font_size=13 color="#c8d0e0"  multiline=false read_only=true>
                    multiline=false
                    Line breaks should become spaces
                </TextArea>
            </Element>
            <Element style={{
                width: Length::px(390.0),
                height: Length::px(290.0),
                background: "#0f172a",
                border: Border::uniform(Length::px(2.0), &Color::hex("#1d4ed8")),
                border_radius: 14,
                display: Display::Flow,
                flow_direction: FlowDirection::Column,
                flow_wrap: FlowWrap::NoWrap,
                gap: Length::px(8.0),
                padding: Padding::uniform(Length::px(12.0)),
            }}>
                <Text font_size=14 color="#e2e8f0" >MUI Components Demo</Text>
                <Element style={{
                    display: Display::Flow,
                    flow_direction: FlowDirection::Row,
                    flow_wrap: FlowWrap::NoWrap,
                    gap: Length::px(8.0)
                }}>
                    <Button label="Contained" width=98 height=34 variant="contained" />
                    <Button label="Outlined" width=98 height=34 variant="outlined" />
                    <Button label="Text" width=70 height=34 variant="text" />
                </Element>
                <Checkbox label="Enable feature" binding={checked.binding()} width=180 height=30 />
                <Element style={{
                    width: Length::px(300.0),
                    height: Length::px(36.0),
                    display: Display::Flow,
                    flow_direction: FlowDirection::Row,
                    flow_wrap: FlowWrap::NoWrap,
                    gap: Length::px(8.0)
                }}>
                    <NumberField binding={number_value.binding()} min=0.0 max=10.0 step=0.5 width=136 height=36 />
                    <Select
                        options={vec![
                            String::from("Option A"),
                            String::from("Option B"),
                            String::from("Option C"),
                        ]}
                        binding={selected_index.binding()}
                        width=140
                        height=36
                    />
                </Element>
                <Slider binding={slider_value.binding()} min=0.0 max=100.0 width=180 height=30 />
                <Switch label="Dark mode" binding={switch_on.binding()} />
                <Text font_size=12 color="#93c5fd" >
                    {format!(
                        "checked={} number={:.1} selected={} slider={:.0} switch={}",
                        checked_value,
                        number_value_value,
                        selected_index_value,
                        slider_value_value,
                        switch_on_value
                    )}
                </Text>
            </Element>
        </Element>
    }
}

#[derive(Default)]
struct App {
    window: Option<Arc<Window>>,
    viewport: Option<Viewport>,
    app: Option<RsxNode>,
    ime_composing: bool,
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
        // Enable IME as long as there is a focused node; cursor area can arrive later.
        let next_allowed = focused_id.is_some();

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
        self.ime_composing = false;
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
                if let Some(viewport) = &mut self.viewport {
                    let button = map_mouse_button(button);
                    viewport.set_mouse_button_pressed(button, state == ElementState::Pressed);
                    if state == ElementState::Pressed {
                        viewport.dispatch_mouse_down_event(button);
                    } else {
                        viewport.dispatch_mouse_up_event(button);
                        viewport.dispatch_click_event(button);
                    }
                }
                self.mark_ime_dirty();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let Some(viewport) = &mut self.viewport {
                    let key = key_to_string(&event.logical_key);
                    let pressed = event.state == ElementState::Pressed;
                    viewport.set_key_pressed(key.clone(), pressed);
                    let code = format!("{:?}", event.physical_key);
                    if pressed {
                        viewport.dispatch_key_down_event(key, code, event.repeat);
                        if !self.ime_composing {
                            if let Some(text) = event.text.as_deref() {
                                if should_dispatch_keyboard_text(viewport, text) {
                                    viewport.dispatch_text_input_event(text.to_string());
                                }
                            }
                        }
                    } else {
                        viewport.dispatch_key_up_event(key, code, event.repeat);
                    }
                }
                self.mark_ime_dirty();
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                self.ime_composing = false;
                if let Some(viewport) = &mut self.viewport {
                    viewport.dispatch_text_input_event(text);
                }
                self.mark_ime_dirty();
            }
            WindowEvent::Ime(Ime::Preedit(text, cursor)) => {
                self.ime_composing = !text.is_empty();
                if let Some(viewport) = &mut self.viewport {
                    viewport.dispatch_ime_preedit_event(text, cursor);
                }
                self.mark_ime_dirty();
            }
            WindowEvent::Ime(Ime::Disabled) => {
                self.ime_composing = false;
                if let Some(viewport) = &mut self.viewport {
                    viewport.dispatch_ime_preedit_event(String::new(), None);
                }
                self.mark_ime_dirty();
            }
            WindowEvent::Focused(false) => {
                self.ime_composing = false;
                if let Some(viewport) = &mut self.viewport {
                    viewport.clear_input_state();
                }
                self.mark_ime_dirty();
            }
            _ => (),
        }

        if take_state_dirty() {
            self.rebuild_app();
            if let Some(viewport) = &mut self.viewport {
                viewport.request_redraw();
            }
        }

        if let (Some(window), Some(viewport)) = (&self.window, &mut self.viewport) {
            if viewport.take_redraw_request() {
                window.request_redraw();
            }
        }
        self.sync_ime_state(false);
    }
}

fn map_mouse_button(button: winit::event::MouseButton) -> rfgui::MouseButton {
    match button {
        winit::event::MouseButton::Left => rfgui::MouseButton::Left,
        winit::event::MouseButton::Right => rfgui::MouseButton::Right,
        winit::event::MouseButton::Middle => rfgui::MouseButton::Middle,
        winit::event::MouseButton::Back => rfgui::MouseButton::Back,
        winit::event::MouseButton::Forward => rfgui::MouseButton::Forward,
        winit::event::MouseButton::Other(v) => rfgui::MouseButton::Other(v),
    }
}

fn key_to_string(key: &Key) -> String {
    match key {
        Key::Character(text) => text.to_string(),
        _ => format!("{key:?}"),
    }
}

fn should_dispatch_keyboard_text(viewport: &Viewport, text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    if text.chars().any(|ch| ch.is_control()) {
        return false;
    }
    // Keep shortcuts (Ctrl/Alt/Cmd + key) out of text-input path.
    let has_alt = viewport.is_key_pressed("Named(Alt)")
        || viewport.is_key_pressed("Named(AltGraph)")
        || viewport.is_key_pressed("Code(AltLeft)")
        || viewport.is_key_pressed("Code(AltRight)");
    let has_ctrl = viewport.is_key_pressed("Named(Control)")
        || viewport.is_key_pressed("Code(ControlLeft)")
        || viewport.is_key_pressed("Code(ControlRight)");
    let has_meta = viewport.is_key_pressed("Named(Super)")
        || viewport.is_key_pressed("Named(Meta)")
        || viewport.is_key_pressed("Code(SuperLeft)")
        || viewport.is_key_pressed("Code(SuperRight)")
        || viewport.is_key_pressed("Code(MetaLeft)")
        || viewport.is_key_pressed("Code(MetaRight)");
    !(has_alt || has_ctrl || has_meta)
}

fn main() {
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App::default();
    event_loop.run_app(&mut app).unwrap();
}
