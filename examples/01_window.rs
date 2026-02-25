use rfgui::{Transition, TransitionProperty};
use rfgui_components::{
    Button, ButtonVariant, Checkbox, NumberField, Select, Slider, Switch, Window, on_resize,
};
use std::sync::Arc;

use rfgui::ui::host::{Element, Text, TextArea};
use rfgui::ui::{RsxNode, component, globalState, on_click, rsx, take_state_dirty, use_state};
use rfgui::{
    Border, BorderRadius, ClipMode, Collision, CollisionBoundary, Color, Cursor, Display,
    FontFamily, HexColor, Length, Padding, Position, ScrollDirection, Viewport,
};
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{DeviceEvent, ElementState, Ime, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::Key;
use winit::window::{CursorIcon, Window as WinitWindow, WindowId};

#[component]
fn MainScene() -> RsxNode {
    fn select_label(item: &String, _: usize) -> String {
        item.clone()
    }

    let click_count = globalState(|| 0_i32);
    let message =
        use_state(|| String::from("Line 1: multiline=true\nLine 2: keep editing\n中文字測試"));
    let checked = use_state(|| true);
    let number_value = use_state(|| 3.0_f64);
    let selected_value = use_state(|| String::from("Option A"));
    let slider_value = use_state(|| 42.0_f64);
    let switch_on = use_state(|| false);
    let panel_size = globalState(|| String::from("360 x 240"));

    let click_count_value = click_count.get();
    let message_value = message.get();
    let checked_value = checked.get();
    let number_value_value = number_value.get();
    let selected_value_value = selected_value.get();
    let slider_value_value = slider_value.get();
    let switch_on_value = switch_on.get();
    let increment_state = click_count.clone();
    let panel_size_state = panel_size.clone();
    let increment = on_click(move |event| {
        increment_state.update(|v| *v += 1);
        event.meta.stop_propagation();
    });
    let panel_resize = on_resize(move |w, h| {
        panel_size_state.update(|value| *value = format!("{w:.0} x {h:.0}"));
    });

    rsx! {
        <Element style={{
            width: Length::percent(100.0),
            height: Length::percent(100.0),
            display: Display::flow().row().wrap(),
            gap: Length::px(24.0),
            padding: Padding::uniform(Length::px(20.0)),
            scroll_direction: ScrollDirection::Vertical,
            font: FontFamily::new(["Noto Sans CJK TC", "PingFang TC"]),
        }} anchor="root">
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
                display: Display::flow().row().wrap(),
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
                display: Display::flow().row().wrap(),
                gap: Length::px(8.0),
                padding: Padding::uniform(Length::px(8.0)),
            }} >
                <Text font_size=22 style={{ color: "#abb2bf" }} >
                    Button Test
                </Text>
                <Text font_size=14 style={{ color: "#abb2bf" }} >{format!("Click Count: {}", click_count_value)}</Text>
                <Button
                    label="Click\nMe"
                    variant={Some(ButtonVariant::Contained)}
                    on_click={increment}
                />
            </Element>
            <Element style={{
                width: Length::px(150.0),
                height: Length::px(150.0),
                background: "#61afef",
                border: Border::uniform(Length::px(3.0), &Color::hex("#21252b")),
                border_radius: 16,
                opacity: 0.5,
                display: Display::flow().column(),
            }}>
                <Text font_size=16 style={{ color: "#21252b" }} >
                    Opacity Nesting
                </Text>
                <Element style={{ width: Length::px(110.0), height: Length::px(86.0), background: "#e06c75", border: Border::uniform(Length::px(2.0), &Color::hex("#21252b")), border_radius: 12, opacity: 0.6 }}>
                    <Element style={{
                        width: Length::px(72.0), height: Length::px(52.0), background: "#61afef", border: Border::uniform(Length::px(2.0), &Color::hex("#21252b")), border_radius: 8, opacity: 0.5 }}>
                        <Text font_size=12 style={{ color: "#ffffff" }}  opacity=0.9>
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
                display: Display::flow().column(),
            }}>
                <Text font_size=16 style={{ color: "#21252b" }} >
                    Background Opacity Nesting
                </Text>
                <Element style={{ width: Length::px(110.0), height: Length::px(86.0), background: "#e06c75", border: Border::uniform(Length::px(2.0), &Color::hex("#21252b")), border_radius: 12, opacity: 1 }}>
                    <Element style={{
                        width: Length::px(72.0), height: Length::px(52.0), background: "#61afef", border: Border::uniform(Length::px(2.0), &Color::hex("#21252b")), border_radius: 8, opacity: 0.5 }}>
                        <Text font_size=12 style={{ color: "#ffffff" }}  opacity=0.9>
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
                display: Display::flow().column(),
            }}>
                <Text font_size=12 style={{ color: "#111111" }} >Scroll down to see more content 1</Text>
                <Text font_size=12 style={{ color: "#111111" }} >Scroll down to see more content 2</Text>
                <Text font_size=12 style={{ color: "#111111" }} >Scroll down to see more content 3</Text>
                <Text font_size=12 style={{ color: "#111111" }} >Scroll down to see more content 4</Text>
                <Text font_size=12 style={{ color: "#111111" }} >Scroll down to see more content 5</Text>
                <Text font_size=12 style={{ color: "#111111" }} >Scroll down to see more content 6</Text>
                <Text font_size=12 style={{ color: "#111111" }} >Scroll down to see more content 7</Text>
                <Text font_size=12 style={{ color: "#111111" }} >Scroll down to see more content 8</Text>
                <Text font_size=12 style={{ color: "#111111" }} >Scroll down to see more content 9</Text>
                <Text font_size=12 style={{ color: "#111111" }} >Scroll down to see more content 10</Text>
                <Text font_size=12 style={{ color: "#111111" }} >Scroll down to see more content 11</Text>
                <Text font_size=12 style={{ color: "#111111" }} >Scroll down to see more content 12</Text>
                <Text font_size=12 style={{ color: "#111111" }} >Scroll down to see more content 13</Text>

            </Element>
            <Element style={{
                width: Length::px(320.0),
                height: Length::px(170.0),
                background: "#2c313c",
                border: Border::uniform(Length::px(3.0), &Color::hex("#61afef")),
                border_radius: 16,
                display: Display::flow().column().no_wrap(),
            }}>
                <Text font_size=14 style={{ color: "#e5e9f0" }} >TextArea Test</Text>
                <Text font_size=12 style={{ color: "#aab4c6" }} >
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
                display: Display::flow().column().no_wrap(),
                gap: Length::px(8.0),
                padding: Padding::uniform(Length::px(12.0)),
            }}>
                <Text font_size=14 style={{ color: "#e2e8f0" }} >MUI Components Demo</Text>
                <Element style={{
                    display: Display::flow().row().no_wrap(),
                    gap: Length::px(8.0)
                }}>
                    <Button
                        label="Contained"
                        variant={Some(ButtonVariant::Contained)}
                    />
                    <Button
                        label="Outlined"
                        variant={Some(ButtonVariant::Outlined)}
                    />
                    <Button
                        label="Text"
                        variant={Some(ButtonVariant::Text)}
                    />
                </Element>
                <Checkbox
                    label="Enable feature"
                    binding={checked.binding()}
                />
                <Element style={{
                    display: Display::flow().row().no_wrap(),
                    gap: Length::px(8.0),
                }}>
                    <NumberField
                        binding={number_value.binding()}
                        min=0.0
                        max=10.0
                        step=0.5
                    />
                    <Select
                        data={(1..100).collect::<Vec<i32>>()}
                        to_label={|item, index| format!("{} Hello, Very Long Item!", item)}
                        to_value={|item, index| format!("{}", item)}
                        value={selected_value.binding()}
                    />
                </Element>
                <Slider
                    binding={slider_value.binding()}
                    min=0.0
                    max=100.0
                />
                <Switch
                    label="Dark mode"
                    binding={switch_on.binding()}
                />
                <Text font_size=12 style={{ color: "#93c5fd" }} >
                    {format!(
                        "checked={} number={:.1} selected={} slider={:.0} switch={}",
                        checked_value,
                        number_value_value,
                        selected_value_value,
                        slider_value_value,
                        switch_on_value
                    )}
                </Text>
            </Element>
            <Element style={{
                width: Length::px(420.0),
                height: Length::px(340.0),
                background: "#0b1324",
                border: Border::uniform(Length::px(2.0), &Color::hex("#1e40af")),
                border_radius: 14,
                padding: Padding::uniform(Length::px(12.0)),
                display: Display::flow().column().no_wrap(),
                gap: Length::px(8.0),
            }}>
                <Text font_size=14 style={{ color: "#dbeafe" }}>Window Component Demo</Text>
                <Text font_size=12 style={{ color: "#93c5fd" }}>
                    {format!("size: {}", panel_size.get())}
                </Text>
                <Element style={{
                    width: Length::percent(100.0),
                    height: Length::px(270.0),
                    background: "#0f172a",
                    border: Border::uniform(Length::px(1.0), &Color::hex("#334155")),
                    border_radius: 10,
                }}>
                    <Window
                        title="Inspector Panel"
                        draggable=true
                        width=360.0
                        height=240.0
                        on_resize={panel_resize}
                    >
                        <Text font_size=12 style={{ color: "#0f172a" }}>Drag title bar to move</Text>
                        <Text font_size=12 style={{ color: "#334155" }}>Drag bottom-right handle to resize</Text>
                        <Button
                            label="Action"
                            variant={Some(ButtonVariant::Outlined)}
                        />
                    </Window>
                </Element>
            </Element>
            <Element style={{
                width: Length::percent(100.0),
                height: Length::px(300.0),
                background: "#111827",
                border: Border::uniform(Length::px(2.0), &Color::hex("#334155")),
                border_radius: 14,
                padding: Padding::uniform(Length::px(12.0)),
                display: Display::flow().column().no_wrap(),
                gap: Length::px(8.0),
            }} anchor="menu_button">
                <Text font_size=14 style={{ color: "#e2e8f0" }}>Absolute + Anchor + Collision</Text>
                <Text font_size=12 style={{ color: "#94a3b8" }}>
                    parent anchor = "menu_button"
                </Text>
                <Element style={{
                    width: Length::px(120.0),
                    height: Length::px(36.0),
                    background: "#1d4ed8",
                    border_radius: 8,
                }}>
                    <Text font_size=12 style={{ color: "#eff6ff" }}>Menu Button</Text>
                </Element>
                <Element style={{
                    width: Length::percent(100.0),
                    height: Length::px(110.0),
                    display: Display::flow().row().no_wrap(),
                    gap: Length::px(8.0),
                }}>
                    <Element style={{
                        width: Length::px(110.0),
                        height: Length::px(110.0),
                        background: "#1f2937",
                        border: Border::uniform(Length::px(1.0), &Color::hex("#475569")),
                        border_radius: 8,
                        padding: Padding::uniform(Length::px(8.0)),
                    }}>
                        <Text font_size=10 style={{ color: "#cbd5e1" }}>clip=Parent (default)</Text>
                        <Element style={{
                            position: Position::absolute()
                                .top(Length::px(56.0))
                                .left(Length::px(84.0)),
                            width: Length::px(74.0),
                            height: Length::px(24.0),
                            background: "#ef4444",
                            border_radius: 6,
                        }}>
                            <Text font_size=10 style={{ color: "#fef2f2" }}>overflow</Text>
                        </Element>
                    </Element>
                    <Element style={{
                        width: Length::px(110.0),
                        height: Length::px(110.0),
                        background: "#1f2937",
                        border: Border::uniform(Length::px(1.0), &Color::hex("#475569")),
                        border_radius: 8,
                        padding: Padding::uniform(Length::px(8.0)),
                    }}>
                        <Text font_size=10 style={{ color: "#cbd5e1" }}>clip=Viewport</Text>
                        <Element style={{
                            position: Position::absolute()
                                .top(Length::px(56.0))
                                .left(Length::px(84.0))
                                .clip(ClipMode::Viewport),
                            width: Length::px(74.0),
                            height: Length::px(24.0),
                            background: "#f59e0b",
                            border_radius: 6,
                        }}>
                            <Text font_size=10 style={{ color: "#fffbeb" }}>overflow</Text>
                        </Element>
                    </Element>
                    <Element style={{
                        width: Length::px(140.0),
                        height: Length::px(110.0),
                        background: "#1f2937",
                        border: Border::uniform(Length::px(1.0), &Color::hex("#475569")),
                        border_radius: 8,
                        padding: Padding::uniform(Length::px(8.0)),
                        display: Display::flow().column().no_wrap(),
                        gap: Length::px(6.0),
                    }}>
                        <Text font_size=10 style={{ color: "#cbd5e1" }}>clip=AnchorParent</Text>
                        <Element style={{
                            width: Length::px(56.0),
                            height: Length::px(26.0),
                            background: "#1d4ed8",
                            border_radius: 6,
                        }} anchor="abs_anchor_test">
                            <Text font_size=9 style={{ color: "#dbeafe" }}>anchor</Text>
                        </Element>
                        <Element style={{
                            position: Position::absolute()
                                .anchor("abs_anchor_test")
                                .top(Length::px(0.0))
                                .left(Length::px(38.0))
                                .clip(ClipMode::AnchorParent),
                            width: Length::px(82.0),
                            height: Length::px(22.0),
                            background: "#22c55e",
                            border_radius: 6,
                        }}>
                            <Text font_size=9 style={{ color: "#ecfdf5" }}>anchor clip</Text>
                        </Element>
                    </Element>
                </Element>
                <Element style={{
                    position: Position::absolute()
                        .anchor("menu_button")
                        .top(Length::px(48.0))
                        .left(Length::px(132.0))
                        .collision(Collision::FlipFit, CollisionBoundary::Viewport),
                    width: Length::px(150.0),
                    height: Length::px(96.0),
                    background: "#0b1220",
                    border: Border::uniform(Length::px(1.0), &Color::hex("#3b82f6")),
                    border_radius: 10,
                    padding: Padding::uniform(Length::px(8.0)),
                    display: Display::flow().column(),
                    gap: Length::px(6.0),
                }}>
                    <Text font_size=12 style={{ color: "#bfdbfe" }}>Popover (anchor)</Text>
                    <Text font_size=11 style={{ color: "#93c5fd" }}>collision: FlipFit + Viewport</Text>
                    <Text font_size=11 style={{ color: "#93c5fd" }}>try resizing window edge</Text>
                </Element>
                <Element style={{
                    position: Position::absolute()
                        .top(Length::px(10.0))
                        .bottom(Length::px(10.0))
                        .right(Length::px(12.0)),
                    width: Length::px(120.0),
                    height: Length::px(30.0),
                    background: "#166534",
                    border_radius: 8,
                }}>
                    <Text font_size=11 style={{ color: "#dcfce7" }}>fallback parent anchor</Text>
                </Element>
            </Element>
        </Element>
    }
}

#[derive(Default)]
struct App {
    window: Option<Arc<WinitWindow>>,
    viewport: Option<Viewport>,
    app: Option<RsxNode>,
    ime_composing: bool,
    ime_dirty: bool,
    last_ime_focus_id: Option<u64>,
    last_ime_allowed: bool,
    last_ime_area: Option<(i32, i32, u32, u32)>,
    cursor_in_window: bool,
    last_mouse_position_viewport: Option<(f32, f32)>,
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
            next_area = Some(viewport.logical_to_physical_rect(x, y, w, h));
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
                .create_window(WinitWindow::default_attributes())
                .unwrap(),
        );
        let mut viewport = Viewport::new();
        viewport.set_scale_factor(window.scale_factor() as f32);
        let size = window.inner_size();
        viewport.set_size(size.width, size.height);
        viewport.set_clear_color(Box::new(HexColor::new("#282c34")));
        let cursor_window = window.clone();
        viewport.set_cursor_handler(move |cursor| {
            cursor_window.set_cursor(map_cursor_icon(cursor));
        });
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
        self.cursor_in_window = false;
        self.last_mouse_position_viewport = None;
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
                self.cursor_in_window = true;
                if let Some(viewport) = &mut self.viewport {
                    let (logical_x, logical_y) =
                        viewport.physical_to_logical_point(position.x as f32, position.y as f32);
                    self.last_mouse_position_viewport = Some((logical_x, logical_y));
                    viewport.set_mouse_position_viewport(logical_x, logical_y);
                    viewport.dispatch_mouse_move_event();
                }
            }
            WindowEvent::CursorLeft { .. } => {
                self.cursor_in_window = false;
                if let Some(viewport) = &mut self.viewport {
                    viewport.clear_mouse_position_viewport();
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if let Some(viewport) = &mut self.viewport {
                    let (dx, dy) = match delta {
                        MouseScrollDelta::LineDelta(x, y) => (x * 24.0, y * 24.0),
                        MouseScrollDelta::PixelDelta(pos) => {
                            viewport.physical_to_logical_point(pos.x as f32, pos.y as f32)
                        }
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

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: winit::event::DeviceId,
        event: DeviceEvent,
    ) {
        if self.cursor_in_window {
            return;
        }
        let Some(viewport) = &mut self.viewport else {
            return;
        };
        if !viewport.has_viewport_mouse_listeners() {
            return;
        }

        match event {
            DeviceEvent::MouseMotion { delta } => {
                let Some((x, y)) = self.last_mouse_position_viewport else {
                    return;
                };
                let (dx, dy) = viewport.physical_to_logical_point(delta.0 as f32, delta.1 as f32);
                let next = (x + dx, y + dy);
                self.last_mouse_position_viewport = Some(next);
                viewport.set_mouse_position_viewport(next.0, next.1);
                viewport.dispatch_mouse_move_event();
            }
            DeviceEvent::Button { button, state } => {
                if state != ElementState::Released {
                    return;
                }
                let Some(mapped_button) = map_device_button(button) else {
                    return;
                };
                if let Some((x, y)) = self.last_mouse_position_viewport {
                    viewport.set_mouse_position_viewport(x, y);
                }
                viewport.set_mouse_button_pressed(mapped_button, false);
                viewport.dispatch_mouse_up_event(mapped_button);
            }
            _ => {}
        }
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

fn map_device_button(button: u32) -> Option<rfgui::MouseButton> {
    match button {
        1 => Some(rfgui::MouseButton::Left),
        2 => Some(rfgui::MouseButton::Right),
        3 => Some(rfgui::MouseButton::Middle),
        4 => Some(rfgui::MouseButton::Back),
        5 => Some(rfgui::MouseButton::Forward),
        _ => None,
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

fn map_cursor_icon(cursor: Cursor) -> CursorIcon {
    match cursor {
        Cursor::Default => CursorIcon::Default,
        Cursor::ContextMenu => CursorIcon::ContextMenu,
        Cursor::Help => CursorIcon::Help,
        Cursor::Pointer => CursorIcon::Pointer,
        Cursor::Progress => CursorIcon::Progress,
        Cursor::Wait => CursorIcon::Wait,
        Cursor::Cell => CursorIcon::Cell,
        Cursor::Crosshair => CursorIcon::Crosshair,
        Cursor::Text => CursorIcon::Text,
        Cursor::VerticalText => CursorIcon::VerticalText,
        Cursor::Alias => CursorIcon::Alias,
        Cursor::Copy => CursorIcon::Copy,
        Cursor::Move => CursorIcon::Move,
        Cursor::NoDrop => CursorIcon::NoDrop,
        Cursor::NotAllowed => CursorIcon::NotAllowed,
        Cursor::Grab => CursorIcon::Grab,
        Cursor::Grabbing => CursorIcon::Grabbing,
        Cursor::EResize => CursorIcon::EResize,
        Cursor::NResize => CursorIcon::NResize,
        Cursor::NeResize => CursorIcon::NeResize,
        Cursor::NwResize => CursorIcon::NwResize,
        Cursor::SResize => CursorIcon::SResize,
        Cursor::SeResize => CursorIcon::SeResize,
        Cursor::SwResize => CursorIcon::SwResize,
        Cursor::WResize => CursorIcon::WResize,
        Cursor::EwResize => CursorIcon::EwResize,
        Cursor::NsResize => CursorIcon::NsResize,
        Cursor::NeswResize => CursorIcon::NeswResize,
        Cursor::NwseResize => CursorIcon::NwseResize,
        Cursor::ColResize => CursorIcon::ColResize,
        Cursor::RowResize => CursorIcon::RowResize,
        Cursor::AllScroll => CursorIcon::AllScroll,
        Cursor::ZoomIn => CursorIcon::ZoomIn,
        Cursor::ZoomOut => CursorIcon::ZoomOut,
        Cursor::DndAsk => CursorIcon::DndAsk,
        Cursor::AllResize => CursorIcon::AllResize,
    }
}

fn main() {
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App::default();
    event_loop.run_app(&mut app).unwrap();
}
