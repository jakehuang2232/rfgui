use crate::platform::{key_to_string, map_cursor_icon, map_device_button, map_mouse_button};
use crate::rfgui::promotion::ViewportPromotionConfig;
use crate::rfgui::ui::{
    RsxNode, clear_redraw_callback, next_timer_deadline, rsx, run_due_timers,
    set_redraw_callback, take_state_dirty,
};
use crate::rfgui::{ColorLike, Viewport};
use crate::rfgui_components::{Theme, init_theme};
use crate::scene::MainScene;
use crate::state::{
    DEBUG_GEOMETRY_OVERLAY, DEBUG_RENDER_TIME, DEBUG_REUSE_PATH, ENABLE_LAYER_PROMOTION,
    REQUEST_DUMP_FRAME_GRAPH_DOT, THEME_DARK_MODE,
};
use crate::utils::{app_background_color, current_unix_timestamp, should_dispatch_keyboard_text};
use rfd::FileDialog;
use std::fs;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{DeviceEvent, ElementState, Ime, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window as WinitWindow, WindowId};

#[cfg(target_os = "macos")]
use crate::platform::with_shadow;

#[derive(Clone, Copy)]
struct WheelNormalization {
    mouse_line_step: f32,
    touchpad_pixel_scale: f32,
    min_touchpad_delta: f32,
}

impl Default for WheelNormalization {
    fn default() -> Self {
        Self {
            mouse_line_step: 28.0,
            touchpad_pixel_scale: 1.0,
            min_touchpad_delta: 0.5,
        }
    }
}

#[derive(Default)]
pub struct App {
    window: Option<Arc<WinitWindow>>,
    viewport: Option<Viewport>,
    app: Option<RsxNode>,
    app_dirty: bool,
    ime_composing: bool,
    ime_dirty: bool,
    last_ime_focus_id: Option<u64>,
    last_ime_allowed: bool,
    last_ime_area: Option<(i32, i32, u32, u32)>,
    cursor_in_window: bool,
    last_mouse_position_viewport: Option<(f32, f32)>,
    background_color: Box<dyn ColorLike>,
    applied_theme_dark: Option<bool>,
    wheel_normalization: WheelNormalization,
}

impl App {
    fn rebuild_app(&mut self) {
        self.app = Some(rsx! { <MainScene /> });
    }

    fn mark_ime_dirty(&mut self) {
        self.ime_dirty = true;
    }

    fn sync_theme_visuals(&mut self) {
        let theme_dark = THEME_DARK_MODE.load(Ordering::Relaxed);
        if self.applied_theme_dark == Some(theme_dark) {
            return;
        }
        self.applied_theme_dark = Some(theme_dark);
        self.background_color = app_background_color(theme_dark);
        if let Some(viewport) = &mut self.viewport {
            viewport.set_clear_color(self.background_color.clone());
            viewport.request_redraw();
        }
        #[cfg(target_os = "macos")]
        if let Some(window) = &self.window {
            with_shadow(window, !self.background_color.is_transparent());
        }
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
        let next_allowed = focused_id.is_some();

        if self.last_ime_allowed != next_allowed {
            window.set_ime_allowed(next_allowed);
            self.last_ime_allowed = next_allowed;
        }

        if let Some((x, y, w, h)) = next_area {
            if self.last_ime_area != Some((x, y, w, h)) {
                let position = PhysicalPosition::new(x, y);
                let size = PhysicalSize::new(w, h);
                window.set_ime_cursor_area(position, size);
            }
        }
        self.last_ime_area = next_area;
        self.ime_dirty = false;
    }

    fn dump_frame_graph_dot_with_dialog(&mut self) {
        let Some(viewport) = &self.viewport else {
            eprintln!("[warn] no viewport available for frame graph dump");
            return;
        };
        let Some(dot) = viewport.dump_graph() else {
            eprintln!("[warn] no frame graph available for dump");
            return;
        };
        let default_file_name = format!("framegraph-{}.dot", current_unix_timestamp());
        let Some(path) = FileDialog::new()
            .add_filter("Graphviz DOT", &["dot"])
            .set_file_name(&default_file_name)
            .save_file()
        else {
            return;
        };
        if let Err(error) = fs::write(&path, dot.as_bytes()) {
            eprintln!(
                "[warn] failed to dump frame graph DOT to {}: {}",
                path.display(),
                error
            );
            return;
        }
        println!("[info] frame graph DOT dumped to {}", path.display());
    }

    fn normalize_wheel_delta(
        config: WheelNormalization,
        viewport: &Viewport,
        delta: MouseScrollDelta,
    ) -> Option<(f32, f32)> {
        let normalized = match delta {
            MouseScrollDelta::LineDelta(x, y) => (x * config.mouse_line_step, y * config.mouse_line_step),
            MouseScrollDelta::PixelDelta(position) => {
                let (dx, dy) =
                    viewport.physical_to_logical_point(position.x as f32, position.y as f32);
                let dx = dx * config.touchpad_pixel_scale;
                let dy = dy * config.touchpad_pixel_scale;
                let dx = if dx.abs() < config.min_touchpad_delta { 0.0 } else { dx };
                let dy = if dy.abs() < config.min_touchpad_delta { 0.0 } else { dy };
                (dx, dy)
            }
        };

        if normalized.0.abs() <= f32::EPSILON && normalized.1.abs() <= f32::EPSILON {
            return None;
        }
        Some((-normalized.0, -normalized.1))
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = Arc::new(
            event_loop
                .create_window(
                    WinitWindow::default_attributes()
                        .with_transparent(true)
                        .with_title("RFGUI Example"),
                )
                .unwrap(),
        );
        let mut viewport = Viewport::new();
        viewport.set_scale_factor(window.scale_factor() as f32);
        let size = window.inner_size();
        viewport.set_size(size.width, size.height);
        viewport.set_clear_color(self.background_color.clone());
        let cursor_window = window.clone();
        viewport.set_cursor_handler(move |cursor| {
            cursor_window.set_cursor(map_cursor_icon(cursor));
        });
        pollster::block_on(viewport.set_window(window.clone()));
        pollster::block_on(viewport.create_surface());
        window.set_ime_allowed(false);

        #[cfg(target_os = "macos")]
        with_shadow(&window, !self.background_color.is_transparent());

        self.window = Some(window);
        self.viewport = Some(viewport);
        if let Some(window) = &self.window {
            let redraw_window = window.clone();
            set_redraw_callback(move || {
                redraw_window.request_redraw();
            });
        }
        self.ime_composing = false;
        self.ime_dirty = true;
        self.last_ime_focus_id = None;
        self.last_ime_allowed = false;
        self.last_ime_area = None;
        self.cursor_in_window = false;
        self.last_mouse_position_viewport = None;
        self.applied_theme_dark = None;
        self.sync_theme_visuals();
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
                clear_redraw_callback();
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
                if self.app_dirty || take_state_dirty() {
                    self.rebuild_app();
                    self.app_dirty = false;
                }
                self.sync_theme_visuals();
                if let (Some(viewport), Some(app)) = (&mut self.viewport, &self.app) {
                    viewport
                        .set_debug_geometry_overlay(DEBUG_GEOMETRY_OVERLAY.load(Ordering::Relaxed));
                    viewport.set_debug_trace_render_time(DEBUG_RENDER_TIME.load(Ordering::Relaxed));
                    viewport.set_debug_trace_reuse_path(DEBUG_REUSE_PATH.load(Ordering::Relaxed));
                    let mut promotion_config = viewport.promotion_config();
                    let enable_layer_promotion = ENABLE_LAYER_PROMOTION.load(Ordering::Relaxed);
                    promotion_config.enabled = enable_layer_promotion;
                    promotion_config.base_threshold = if enable_layer_promotion {
                        ViewportPromotionConfig::default().base_threshold
                    } else {
                        1000
                    };
                    viewport.set_promotion_config(promotion_config);
                    let _ = viewport.render_rsx(app);
                }
                if REQUEST_DUMP_FRAME_GRAPH_DOT.swap(false, Ordering::AcqRel) {
                    self.dump_frame_graph_dot_with_dialog();
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
                let wheel_normalization = self.wheel_normalization;
                if let Some(viewport) = &mut self.viewport {
                    if let Some((dx, dy)) =
                        Self::normalize_wheel_delta(wheel_normalization, viewport, delta)
                    {
                        viewport.dispatch_mouse_wheel_event(dx, dy);
                    }
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
            self.app_dirty = true;
            if let Some(viewport) = &mut self.viewport {
                viewport.request_redraw();
            }
        }

        if let (Some(window), Some(viewport)) = (&self.window, &mut self.viewport) {
            if REQUEST_DUMP_FRAME_GRAPH_DOT.load(Ordering::Acquire) {
                viewport.request_redraw();
            }
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

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        run_due_timers(std::time::Instant::now());

        if let Some(deadline) = next_timer_deadline() {
            event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
        } else {
            event_loop.set_control_flow(ControlFlow::Wait);
        }
    }
}

pub fn run() {
    init_theme(Theme::dark());
    THEME_DARK_MODE.store(true, Ordering::Relaxed);
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App::default();
    app.background_color = app_background_color(true);
    app.applied_theme_dark = Some(true);
    event_loop.run_app(&mut app).unwrap();
}
