use crate::platform::{key_to_string, map_cursor_icon, map_device_button, map_mouse_button};
use crate::rfgui::promotion::ViewportPromotionConfig;
use crate::rfgui::time::Instant;
use crate::rfgui::ui::{
    RsxNode, UiDirtyState, clear_redraw_callback, next_timer_deadline, rsx, run_due_timers,
    set_redraw_callback, take_state_dirty,
};
use crate::rfgui::{ColorLike, Viewport};
#[cfg(target_arch = "wasm32")]
use crate::rfgui::view::load_default_web_cjk_font;
use crate::rfgui_components::{Theme, init_theme};
use crate::scene::MainScene;
use crate::state::{
    DEBUG_GEOMETRY_OVERLAY, DEBUG_RENDER_TIME, DEBUG_REUSE_PATH, ENABLE_LAYER_PROMOTION,
    REQUEST_DUMP_FRAME_GRAPH_DOT, THEME_DARK_MODE,
};
#[cfg(not(target_arch = "wasm32"))]
use crate::utils::current_unix_timestamp;
use crate::utils::{app_background_color, should_dispatch_keyboard_text};
#[cfg(not(target_arch = "wasm32"))]
use rfd::FileDialog;
use std::cell::RefCell;
#[cfg(not(target_arch = "wasm32"))]
use std::fs;
#[cfg(target_arch = "wasm32")]
use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::Ordering;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::{JsCast, closure::Closure};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition, PhysicalSize};
use winit::event::{DeviceEvent, ElementState, Ime, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
#[cfg(not(target_arch = "wasm32"))]
use winit::event_loop::EventLoop;
#[cfg(target_arch = "wasm32")]
use winit::platform::web::WindowAttributesExtWebSys;
use winit::window::{Window as WinitWindow, WindowId};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::spawn_local;
#[cfg(target_arch = "wasm32")]
use web_sys::{HtmlCanvasElement, KeyboardEvent, PointerEvent, WheelEvent, Window};

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
    viewport: Rc<RefCell<Option<Viewport>>>,
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
    pending_ui_dirty: UiDirtyState,
}

impl App {
    fn viewport_ready(&self) -> bool {
        self.viewport.borrow().is_some()
    }

    fn with_viewport_mut<R>(&self, f: impl FnOnce(&mut Viewport) -> R) -> Option<R> {
        let mut viewport = self.viewport.borrow_mut();
        viewport.as_mut().map(f)
    }

    fn make_window_attributes() -> winit::window::WindowAttributes {
        let attributes = WinitWindow::default_attributes()
            .with_transparent(true)
            .with_title("RFGUI Example")
            .with_inner_size(LogicalSize::new(1280.0, 800.0));
        #[cfg(target_arch = "wasm32")]
        let attributes = attributes.with_append(true);
        attributes
    }

    fn configure_viewport(
        window: Arc<WinitWindow>,
        background_color: Box<dyn ColorLike>,
    ) -> Viewport {
        let mut viewport = Viewport::new();
        #[cfg(target_arch = "wasm32")]
        viewport.set_msaa_sample_count(1);
        viewport.set_scale_factor(window.scale_factor() as f32);
        let size = window.inner_size();
        viewport.set_size(size.width, size.height);
        viewport.set_clear_color(background_color);
        let cursor_window = window.clone();
        viewport.set_cursor_handler(move |cursor| {
            cursor_window.set_cursor(map_cursor_icon(cursor));
        });
        viewport
    }

    #[cfg(target_arch = "wasm32")]
    fn configure_canvas_viewport(
        canvas: &HtmlCanvasElement,
        background_color: Box<dyn ColorLike>,
    ) -> Viewport {
        let mut viewport = Viewport::new();
        viewport.set_msaa_sample_count(1);
        viewport.set_clear_color(background_color);
        let canvas = canvas.clone();
        viewport.set_cursor_handler(move |cursor| {
            let _ = canvas.style().set_property("cursor", web_cursor_name(cursor));
        });
        viewport
    }

    fn rebuild_app(&mut self) {
        self.app = Some(rsx! { <MainScene /> });
    }

    fn mark_ime_dirty(&mut self) {
        self.ime_dirty = true;
    }

    fn render_frame(&mut self) {
        if !self.viewport_ready() {
            return;
        }
        let pending_ui_dirty = std::mem::take(&mut self.pending_ui_dirty);
        if self.app_dirty || pending_ui_dirty.has_any() {
            self.rebuild_app();
            self.app_dirty = false;
            let _ = take_state_dirty();
        }
        self.sync_theme_visuals();
        if let Some(app) = &self.app {
            self.with_viewport_mut(|viewport| {
                viewport.set_debug_geometry_overlay(DEBUG_GEOMETRY_OVERLAY.load(Ordering::Relaxed));
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
                let _ = viewport.render_rsx_with_dirty(app, pending_ui_dirty);
            });
        }
        if REQUEST_DUMP_FRAME_GRAPH_DOT.swap(false, Ordering::AcqRel) {
            self.dump_frame_graph_dot_with_dialog();
        }
        self.mark_ime_dirty();
    }

    fn finalize_frame_updates(&mut self) -> bool {
        let state_dirty = take_state_dirty();
        self.pending_ui_dirty = self.pending_ui_dirty.union(state_dirty);
        if state_dirty.needs_rebuild() {
            self.app_dirty = true;
        }
        if self.pending_ui_dirty.needs_redraw() {
            self.with_viewport_mut(|viewport| {
                viewport.request_redraw();
            });
        }

        let mut should_schedule_redraw = false;
        self.with_viewport_mut(|viewport| {
            if REQUEST_DUMP_FRAME_GRAPH_DOT.load(Ordering::Acquire) {
                viewport.request_redraw();
            }
            if viewport.take_redraw_request() {
                should_schedule_redraw = true;
            }
        });
        if let Some(window) = &self.window {
            if should_schedule_redraw {
                window.request_redraw();
            }
            should_schedule_redraw = false;
        }
        self.sync_ime_state(false);
        should_schedule_redraw
    }

    fn sync_theme_visuals(&mut self) {
        let theme_dark = THEME_DARK_MODE.load(Ordering::Relaxed);
        if self.applied_theme_dark == Some(theme_dark) {
            return;
        }
        self.applied_theme_dark = Some(theme_dark);
        self.background_color = app_background_color(theme_dark);
        self.with_viewport_mut(|viewport| {
            viewport.set_clear_color(self.background_color.clone());
            viewport.request_redraw();
        });
        #[cfg(target_os = "macos")]
        if let Some(window) = &self.window {
            with_shadow(window, !self.background_color.is_transparent());
        }
    }

    fn sync_ime_state(&mut self, force: bool) {
        let Some(window) = &self.window else {
            return;
        };
        let Some((focused_id, next_area)) = self.with_viewport_mut(|viewport| {
            let focused_id = viewport.focused_node_id();
            let next_area = viewport
                .focused_ime_cursor_rect()
                .map(|(x, y, w, h)| viewport.logical_to_physical_rect(x, y, w, h));
            (focused_id, next_area)
        }) else {
            return;
        };
        if !force && !self.ime_dirty && focused_id == self.last_ime_focus_id {
            return;
        }
        self.last_ime_focus_id = focused_id;
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

    #[cfg(not(target_arch = "wasm32"))]
    fn dump_frame_graph_dot_with_dialog(&mut self) {
        let Some(dot) = self
            .with_viewport_mut(|viewport| viewport.dump_graph())
            .flatten()
        else {
            eprintln!("[warn] no viewport available for frame graph dump");
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

    #[cfg(target_arch = "wasm32")]
    fn dump_frame_graph_dot_with_dialog(&mut self) {
        eprintln!("[warn] frame graph DOT dump is not supported on web");
    }

    fn normalize_wheel_delta(
        config: WheelNormalization,
        viewport: &Viewport,
        delta: MouseScrollDelta,
    ) -> Option<(f32, f32)> {
        let normalized = match delta {
            MouseScrollDelta::LineDelta(x, y) => {
                (x * config.mouse_line_step, y * config.mouse_line_step)
            }
            MouseScrollDelta::PixelDelta(position) => {
                let (dx, dy) =
                    viewport.physical_to_logical_point(position.x as f32, position.y as f32);
                let dx = dx * config.touchpad_pixel_scale;
                let dy = dy * config.touchpad_pixel_scale;
                let dx = if dx.abs() < config.min_touchpad_delta {
                    0.0
                } else {
                    dx
                };
                let dy = if dy.abs() < config.min_touchpad_delta {
                    0.0
                } else {
                    dy
                };
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
                .create_window(Self::make_window_attributes())
                .unwrap(),
        );
        window.set_ime_allowed(false);

        #[cfg(target_os = "macos")]
        with_shadow(&window, !self.background_color.is_transparent());

        let redraw_window = window.clone();
        set_redraw_callback(move || {
            redraw_window.request_redraw();
        });

        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut viewport =
                Self::configure_viewport(window.clone(), self.background_color.clone());
            pollster::block_on(viewport.set_window(window.clone()));
            pollster::block_on(viewport.create_surface());
            viewport.request_redraw();
            self.viewport.borrow_mut().replace(viewport);
        }

        #[cfg(target_arch = "wasm32")]
        {
            let viewport_slot = self.viewport.clone();
            let background_color = self.background_color.clone();
            let async_window = window.clone();
            spawn_local(async move {
                let mut viewport = App::configure_viewport(async_window.clone(), background_color);
                viewport.set_window(async_window.clone()).await;
                viewport.create_surface().await;
                viewport.request_redraw();
                viewport_slot.borrow_mut().replace(viewport);
                async_window.request_redraw();
            });
        }
        self.window = Some(window);
        self.ime_composing = false;
        self.ime_dirty = true;
        self.last_ime_focus_id = None;
        self.last_ime_allowed = false;
        self.last_ime_area = None;
        self.cursor_in_window = false;
        self.last_mouse_position_viewport = None;
        self.applied_theme_dark = None;
        self.pending_ui_dirty = UiDirtyState::NONE;
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
                self.viewport.borrow_mut().take();
            }
            WindowEvent::Resized(size) => {
                self.with_viewport_mut(|viewport| {
                    viewport.set_size(size.width, size.height);
                    viewport.request_redraw();
                });
                self.mark_ime_dirty();
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                if let Some(window) = &self.window {
                    self.with_viewport_mut(|viewport| {
                        viewport.set_scale_factor(scale_factor as f32);
                        let size: PhysicalSize<u32> = window.inner_size();
                        viewport.set_size(size.width, size.height);
                        viewport.request_redraw();
                    });
                }
                self.mark_ime_dirty();
            }
            WindowEvent::RedrawRequested => {
                self.render_frame();
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor_in_window = true;
                if let Some((logical_x, logical_y)) = self.with_viewport_mut(|viewport| {
                    let (logical_x, logical_y) =
                        viewport.physical_to_logical_point(position.x as f32, position.y as f32);
                    viewport.set_mouse_position_viewport(logical_x, logical_y);
                    viewport.dispatch_mouse_move_event();
                    (logical_x, logical_y)
                }) {
                    self.last_mouse_position_viewport = Some((logical_x, logical_y));
                }
            }
            WindowEvent::CursorLeft { .. } => {
                self.cursor_in_window = false;
                self.with_viewport_mut(|viewport| {
                    viewport.clear_mouse_position_viewport();
                });
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let wheel_normalization = self.wheel_normalization;
                self.with_viewport_mut(|viewport| {
                    if let Some((dx, dy)) =
                        Self::normalize_wheel_delta(wheel_normalization, viewport, delta)
                    {
                        viewport.dispatch_mouse_wheel_event(dx, dy);
                    }
                });
                self.mark_ime_dirty();
            }
            WindowEvent::MouseInput { state, button, .. } => {
                self.with_viewport_mut(|viewport| {
                    let button = map_mouse_button(button);
                    viewport.set_mouse_button_pressed(button, state == ElementState::Pressed);
                    if state == ElementState::Pressed {
                        viewport.dispatch_mouse_down_event(button);
                    } else {
                        viewport.dispatch_mouse_up_event(button);
                        viewport.dispatch_click_event(button);
                    }
                });
                self.mark_ime_dirty();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let ime_composing = self.ime_composing;
                self.with_viewport_mut(|viewport| {
                    let key = key_to_string(&event.logical_key);
                    let pressed = event.state == ElementState::Pressed;
                    viewport.set_key_pressed(key.clone(), pressed);
                    let code = format!("{:?}", event.physical_key);
                    if pressed {
                        viewport.dispatch_key_down_event(key, code, event.repeat);
                        if !ime_composing {
                            if let Some(text) = event.text.as_deref() {
                                if should_dispatch_keyboard_text(viewport, text) {
                                    viewport.dispatch_text_input_event(text.to_string());
                                }
                            }
                        }
                    } else {
                        viewport.dispatch_key_up_event(key, code, event.repeat);
                    }
                });
                self.mark_ime_dirty();
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                self.ime_composing = false;
                self.with_viewport_mut(|viewport| {
                    viewport.dispatch_text_input_event(text);
                });
                self.mark_ime_dirty();
            }
            WindowEvent::Ime(Ime::Preedit(text, cursor)) => {
                self.ime_composing = !text.is_empty();
                self.with_viewport_mut(|viewport| {
                    viewport.dispatch_ime_preedit_event(text, cursor);
                });
                self.mark_ime_dirty();
            }
            WindowEvent::Ime(Ime::Disabled) => {
                self.ime_composing = false;
                self.with_viewport_mut(|viewport| {
                    viewport.dispatch_ime_preedit_event(String::new(), None);
                });
                self.mark_ime_dirty();
            }
            WindowEvent::Focused(false) => {
                self.ime_composing = false;
                self.with_viewport_mut(|viewport| {
                    viewport.clear_input_state();
                });
                self.mark_ime_dirty();
            }
            _ => (),
        }

        let _ = self.finalize_frame_updates();
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
        let has_viewport_mouse_listeners = self
            .with_viewport_mut(|viewport| viewport.has_viewport_mouse_listeners())
            .unwrap_or(false);
        if !has_viewport_mouse_listeners {
            return;
        }

        match event {
            DeviceEvent::MouseMotion { delta } => {
                let Some((x, y)) = self.last_mouse_position_viewport else {
                    return;
                };
                if let Some(next) = self.with_viewport_mut(|viewport| {
                    let (dx, dy) =
                        viewport.physical_to_logical_point(delta.0 as f32, delta.1 as f32);
                    let next = (x + dx, y + dy);
                    viewport.set_mouse_position_viewport(next.0, next.1);
                    viewport.dispatch_mouse_move_event();
                    next
                }) {
                    self.last_mouse_position_viewport = Some(next);
                }
            }
            DeviceEvent::Button { button, state } => {
                if state != ElementState::Released {
                    return;
                }
                let Some(mapped_button) = map_device_button(button) else {
                    return;
                };
                let last_mouse_position_viewport = self.last_mouse_position_viewport;
                self.with_viewport_mut(|viewport| {
                    if let Some((x, y)) = last_mouse_position_viewport {
                        viewport.set_mouse_position_viewport(x, y);
                    }
                    viewport.set_mouse_button_pressed(mapped_button, false);
                    viewport.dispatch_mouse_up_event(mapped_button);
                });
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        #[cfg(target_arch = "wasm32")]
        if !self.viewport_ready() {
            event_loop.set_control_flow(ControlFlow::Poll);
            return;
        }

        run_due_timers(Instant::now());

        if let Some(deadline) = next_timer_deadline() {
            event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
        } else {
            event_loop.set_control_flow(ControlFlow::Wait);
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn web_cursor_name(cursor: crate::rfgui::Cursor) -> &'static str {
    match cursor {
        crate::rfgui::Cursor::Default => "default",
        crate::rfgui::Cursor::ContextMenu => "context-menu",
        crate::rfgui::Cursor::Help => "help",
        crate::rfgui::Cursor::Pointer => "pointer",
        crate::rfgui::Cursor::Progress => "progress",
        crate::rfgui::Cursor::Wait => "wait",
        crate::rfgui::Cursor::Cell => "cell",
        crate::rfgui::Cursor::Crosshair => "crosshair",
        crate::rfgui::Cursor::Text => "text",
        crate::rfgui::Cursor::VerticalText => "vertical-text",
        crate::rfgui::Cursor::Alias => "alias",
        crate::rfgui::Cursor::Copy => "copy",
        crate::rfgui::Cursor::Move => "move",
        crate::rfgui::Cursor::NoDrop => "no-drop",
        crate::rfgui::Cursor::NotAllowed => "not-allowed",
        crate::rfgui::Cursor::Grab => "grab",
        crate::rfgui::Cursor::Grabbing => "grabbing",
        crate::rfgui::Cursor::EResize => "e-resize",
        crate::rfgui::Cursor::NResize => "n-resize",
        crate::rfgui::Cursor::NeResize => "ne-resize",
        crate::rfgui::Cursor::NwResize => "nw-resize",
        crate::rfgui::Cursor::SResize => "s-resize",
        crate::rfgui::Cursor::SeResize => "se-resize",
        crate::rfgui::Cursor::SwResize => "sw-resize",
        crate::rfgui::Cursor::WResize => "w-resize",
        crate::rfgui::Cursor::EwResize => "ew-resize",
        crate::rfgui::Cursor::NsResize => "ns-resize",
        crate::rfgui::Cursor::NeswResize => "nesw-resize",
        crate::rfgui::Cursor::NwseResize => "nwse-resize",
        crate::rfgui::Cursor::ColResize => "col-resize",
        crate::rfgui::Cursor::RowResize => "row-resize",
        crate::rfgui::Cursor::AllScroll => "all-scroll",
        crate::rfgui::Cursor::ZoomIn => "zoom-in",
        crate::rfgui::Cursor::ZoomOut => "zoom-out",
        crate::rfgui::Cursor::DndAsk => "alias",
        crate::rfgui::Cursor::AllResize => "move",
    }
}

#[cfg(target_arch = "wasm32")]
fn web_mouse_button(button: i16) -> Option<crate::rfgui::view::viewport::MouseButton> {
    match button {
        0 => Some(crate::rfgui::view::viewport::MouseButton::Left),
        1 => Some(crate::rfgui::view::viewport::MouseButton::Middle),
        2 => Some(crate::rfgui::view::viewport::MouseButton::Right),
        3 => Some(crate::rfgui::view::viewport::MouseButton::Back),
        4 => Some(crate::rfgui::view::viewport::MouseButton::Forward),
        _ => None,
    }
}

#[cfg(target_arch = "wasm32")]
fn web_window() -> Window {
    web_sys::window().expect("window unavailable")
}

#[cfg(target_arch = "wasm32")]
fn web_document(window: &Window) -> web_sys::Document {
    window.document().expect("document unavailable")
}

#[cfg(target_arch = "wasm32")]
fn web_canvas(document: &web_sys::Document) -> HtmlCanvasElement {
    document
        .get_element_by_id("app-canvas")
        .expect("missing #app-canvas")
        .dyn_into::<HtmlCanvasElement>()
        .expect("#app-canvas is not a canvas")
}

#[cfg(target_arch = "wasm32")]
fn web_sync_canvas_size(app: &mut App, canvas: &HtmlCanvasElement, window: &Window) {
    let dpr = window.device_pixel_ratio() as f32;
    let client_width = canvas.client_width().max(1) as u32;
    let client_height = canvas.client_height().max(1) as u32;
    let width = ((client_width as f32) * dpr).round().max(1.0) as u32;
    let height = ((client_height as f32) * dpr).round().max(1.0) as u32;
    if canvas.width() != width {
        canvas.set_width(width);
    }
    if canvas.height() != height {
        canvas.set_height(height);
    }
    app.with_viewport_mut(|viewport| {
        viewport.set_scale_factor(dpr);
        viewport.set_size(width, height);
        viewport.request_redraw();
    });
}

#[cfg(target_arch = "wasm32")]
fn web_schedule_redraw(
    window: &Window,
    raf_pending: &Rc<Cell<bool>>,
    raf_callback: &Rc<RefCell<Option<Closure<dyn FnMut(f64)>>>>,
) {
    if raf_pending.replace(true) {
        return;
    }
    let callback = raf_callback.borrow();
    let callback = callback.as_ref().expect("raf callback should be initialized");
    let _ = window.request_animation_frame(callback.as_ref().unchecked_ref());
}

#[cfg(target_arch = "wasm32")]
fn web_event_position(
    canvas: &HtmlCanvasElement,
    event: &PointerEvent,
    scale_factor: f32,
) -> (f32, f32) {
    let rect = canvas.get_bounding_client_rect();
    let physical_x = ((event.client_x() as f64 - rect.left()) as f32 * scale_factor).max(0.0);
    let physical_y = ((event.client_y() as f64 - rect.top()) as f32 * scale_factor).max(0.0);
    (physical_x, physical_y)
}

#[cfg(target_arch = "wasm32")]
fn run_web() {
    console_error_panic_hook::set_once();

    init_theme(Theme::dark());
    THEME_DARK_MODE.store(true, Ordering::Relaxed);

    let window = web_window();
    let document = web_document(&window);
    let canvas = web_canvas(&document);

    let mut app = App::default();
    app.background_color = app_background_color(true);
    app.applied_theme_dark = Some(true);

    let app = Rc::new(RefCell::new(app));
    let raf_pending = Rc::new(Cell::new(false));
    let raf_callback: Rc<RefCell<Option<Closure<dyn FnMut(f64)>>>> = Rc::new(RefCell::new(None));

    {
        let app = app.clone();
        let window = window.clone();
        let raf_pending = raf_pending.clone();
        let raf_callback_ref = raf_callback.clone();
        *raf_callback.borrow_mut() = Some(Closure::wrap(Box::new(move |_ts: f64| {
            raf_pending.set(false);
            let Ok(mut app) = app.try_borrow_mut() else {
                return;
            };
            let should_schedule = {
                run_due_timers(Instant::now());
                app.render_frame();
                app.finalize_frame_updates()
            };
            if should_schedule {
                web_schedule_redraw(&window, &raf_pending, &raf_callback_ref);
            }
        }) as Box<dyn FnMut(f64)>));
    }

    {
        let mut app_mut = app.borrow_mut();
        let viewport = App::configure_canvas_viewport(&canvas, app_mut.background_color.clone());
        app_mut.viewport.borrow_mut().replace(viewport);
        web_sync_canvas_size(&mut app_mut, &canvas, &window);
    }

    {
        let app = app.clone();
        let canvas = canvas.clone();
        let window = window.clone();
        let raf_pending = raf_pending.clone();
        let raf_callback_ref = raf_callback.clone();
        let redraw_window = window.clone();
        let redraw_pending = raf_pending.clone();
        let redraw_callback_ref = raf_callback_ref.clone();
        set_redraw_callback(move || {
            web_schedule_redraw(
                &redraw_window,
                &redraw_pending,
                &redraw_callback_ref,
            );
        });
        let mut app_mut = app.borrow_mut();
        app_mut.sync_theme_visuals();
        let should_schedule = app_mut.finalize_frame_updates();
        drop(app_mut);
        if should_schedule {
            web_schedule_redraw(&window, &raf_pending, &raf_callback_ref);
        }
        let _ = canvas;
    }

    {
        let app = app.clone();
        let canvas = canvas.clone();
        let window = window.clone();
        let raf_pending = raf_pending.clone();
        let raf_callback_ref = raf_callback.clone();
        spawn_local(async move {
            let _ = load_default_web_cjk_font().await;
            let mut viewport = {
                let app = app.borrow_mut();
                app.viewport.borrow_mut().take().expect("viewport should exist")
            };
            viewport.set_canvas(canvas.clone()).await;
            viewport.create_surface().await;
            viewport.request_redraw();
            {
                let mut app = app.borrow_mut();
                app.rebuild_app();
                app.viewport.borrow_mut().replace(viewport);
            }
            web_schedule_redraw(&window, &raf_pending, &raf_callback_ref);
        });
    }

    {
        let app = app.clone();
        let canvas = canvas.clone();
        let window = window.clone();
        let raf_pending = raf_pending.clone();
        let raf_callback_ref = raf_callback.clone();
        let resize_window = window.clone();
        let resize_schedule_window = window.clone();
        let closure = Closure::wrap(Box::new(move |_event: web_sys::Event| {
            let Ok(mut app) = app.try_borrow_mut() else {
                return;
            };
            let should_schedule = {
                web_sync_canvas_size(&mut app, &canvas, &resize_window);
                app.finalize_frame_updates()
            };
            if should_schedule {
                web_schedule_redraw(
                    &resize_schedule_window,
                    &raf_pending,
                    &raf_callback_ref,
                );
            }
        }) as Box<dyn FnMut(web_sys::Event)>);
        let _ = window.add_event_listener_with_callback("resize", closure.as_ref().unchecked_ref());
        closure.forget();
    }

    {
        let app = app.clone();
        let canvas = canvas.clone();
        let window = window.clone();
        let raf_pending = raf_pending.clone();
        let raf_callback_ref = raf_callback.clone();
        let pointermove_canvas = canvas.clone();
        let closure = Closure::wrap(Box::new(move |event: web_sys::Event| {
            let Ok(event) = event.dyn_into::<PointerEvent>() else {
                return;
            };
            let Ok(mut app) = app.try_borrow_mut() else {
                return;
            };
            let should_schedule = {
                let scale_factor = app
                    .with_viewport_mut(|viewport| viewport.scale_factor())
                    .unwrap_or(1.0);
                let (physical_x, physical_y) =
                    web_event_position(&pointermove_canvas, &event, scale_factor);
                app.cursor_in_window = true;
                if let Some((logical_x, logical_y)) = app.with_viewport_mut(|viewport| {
                    let next = viewport.physical_to_logical_point(physical_x, physical_y);
                    viewport.set_mouse_position_viewport(next.0, next.1);
                    viewport.dispatch_mouse_move_event();
                    next
                }) {
                    app.last_mouse_position_viewport = Some((logical_x, logical_y));
                }
                app.finalize_frame_updates()
            };
            if should_schedule {
                web_schedule_redraw(&window, &raf_pending, &raf_callback_ref);
            }
        }) as Box<dyn FnMut(web_sys::Event)>);
        let _ = canvas.add_event_listener_with_callback("pointermove", closure.as_ref().unchecked_ref());
        closure.forget();
    }

    {
        let app = app.clone();
        let window = window.clone();
        let raf_pending = raf_pending.clone();
        let raf_callback_ref = raf_callback.clone();
        let closure = Closure::wrap(Box::new(move |_event: web_sys::Event| {
            let Ok(mut app) = app.try_borrow_mut() else {
                return;
            };
            let should_schedule = {
                app.cursor_in_window = false;
                app.with_viewport_mut(|viewport| {
                    viewport.clear_mouse_position_viewport();
                });
                app.finalize_frame_updates()
            };
            if should_schedule {
                web_schedule_redraw(&window, &raf_pending, &raf_callback_ref);
            }
        }) as Box<dyn FnMut(web_sys::Event)>);
        let _ = canvas.add_event_listener_with_callback("pointerleave", closure.as_ref().unchecked_ref());
        closure.forget();
    }

    {
        let app = app.clone();
        let canvas = canvas.clone();
        let window = window.clone();
        let raf_pending = raf_pending.clone();
        let raf_callback_ref = raf_callback.clone();
        let pointerdown_canvas = canvas.clone();
        let closure = Closure::wrap(Box::new(move |event: web_sys::Event| {
            let Ok(event) = event.dyn_into::<PointerEvent>() else {
                return;
            };
            let Some(button) = web_mouse_button(event.button() as i16) else {
                return;
            };
            let Ok(mut app) = app.try_borrow_mut() else {
                return;
            };
            if let Some(element) = pointerdown_canvas.dyn_ref::<web_sys::HtmlElement>() {
                let _ = element.focus();
            }
            let should_schedule = {
                let scale_factor = app
                    .with_viewport_mut(|viewport| viewport.scale_factor())
                    .unwrap_or(1.0);
                let (physical_x, physical_y) =
                    web_event_position(&pointerdown_canvas, &event, scale_factor);
                let _ = app.with_viewport_mut(|viewport| {
                    let next = viewport.physical_to_logical_point(physical_x, physical_y);
                    viewport.set_mouse_position_viewport(next.0, next.1);
                    viewport.set_mouse_button_pressed(button, true);
                    viewport.dispatch_mouse_down_event(button);
                    next
                });
                app.finalize_frame_updates()
            };
            if should_schedule {
                web_schedule_redraw(&window, &raf_pending, &raf_callback_ref);
            }
            event.prevent_default();
        }) as Box<dyn FnMut(web_sys::Event)>);
        let _ = canvas.add_event_listener_with_callback("pointerdown", closure.as_ref().unchecked_ref());
        closure.forget();
    }

    {
        let app = app.clone();
        let canvas = canvas.clone();
        let window = window.clone();
        let raf_pending = raf_pending.clone();
        let raf_callback_ref = raf_callback.clone();
        let pointerup_window = window.clone();
        let closure = Closure::wrap(Box::new(move |event: web_sys::Event| {
            let Ok(event) = event.dyn_into::<PointerEvent>() else {
                return;
            };
            let Some(button) = web_mouse_button(event.button() as i16) else {
                return;
            };
            let Ok(mut app) = app.try_borrow_mut() else {
                return;
            };
            let should_schedule = {
                let scale_factor = app
                    .with_viewport_mut(|viewport| viewport.scale_factor())
                    .unwrap_or(1.0);
                let (physical_x, physical_y) = web_event_position(&canvas, &event, scale_factor);
                let _ = app.with_viewport_mut(|viewport| {
                    let next = viewport.physical_to_logical_point(physical_x, physical_y);
                    viewport.set_mouse_position_viewport(next.0, next.1);
                    viewport.set_mouse_button_pressed(button, false);
                    viewport.dispatch_mouse_up_event(button);
                    viewport.dispatch_click_event(button);
                });
                app.finalize_frame_updates()
            };
            if should_schedule {
                web_schedule_redraw(&pointerup_window, &raf_pending, &raf_callback_ref);
            }
            event.prevent_default();
        }) as Box<dyn FnMut(web_sys::Event)>);
        let _ = window.add_event_listener_with_callback("pointerup", closure.as_ref().unchecked_ref());
        closure.forget();
    }

    {
        let app = app.clone();
        let window = window.clone();
        let raf_pending = raf_pending.clone();
        let raf_callback_ref = raf_callback.clone();
        let closure = Closure::wrap(Box::new(move |event: web_sys::Event| {
            let Ok(event) = event.dyn_into::<WheelEvent>() else {
                return;
            };
            let Ok(mut app) = app.try_borrow_mut() else {
                return;
            };
            let should_schedule = {
                let wheel_normalization = app.wheel_normalization;
                app.with_viewport_mut(|viewport| {
                    let delta = match event.delta_mode() {
                        WheelEvent::DOM_DELTA_LINE => {
                            MouseScrollDelta::LineDelta(event.delta_x() as f32, event.delta_y() as f32)
                        }
                        _ => MouseScrollDelta::PixelDelta(PhysicalPosition::new(
                            event.delta_x(),
                            event.delta_y(),
                        )),
                    };
                    if let Some((dx, dy)) =
                        App::normalize_wheel_delta(wheel_normalization, viewport, delta)
                    {
                        viewport.dispatch_mouse_wheel_event(dx, dy);
                    }
                });
                app.finalize_frame_updates()
            };
            if should_schedule {
                web_schedule_redraw(&window, &raf_pending, &raf_callback_ref);
            }
            event.prevent_default();
        }) as Box<dyn FnMut(web_sys::Event)>);
        let _ = canvas.add_event_listener_with_callback("wheel", closure.as_ref().unchecked_ref());
        closure.forget();
    }

    {
        let app = app.clone();
        let window = window.clone();
        let raf_pending = raf_pending.clone();
        let raf_callback_ref = raf_callback.clone();
        let closure = Closure::wrap(Box::new(move |event: web_sys::Event| {
            let Ok(event) = event.dyn_into::<KeyboardEvent>() else {
                return;
            };
            let Ok(mut app) = app.try_borrow_mut() else {
                return;
            };
            let should_schedule = {
                let ime_composing = app.ime_composing;
                app.with_viewport_mut(|viewport| {
                    let key = event.key();
                    let code = event.code();
                    viewport.set_key_pressed(key.clone(), true);
                    viewport.dispatch_key_down_event(key.clone(), code, event.repeat());
                    if !ime_composing && should_dispatch_keyboard_text(viewport, &key) {
                        viewport.dispatch_text_input_event(key);
                    }
                });
                app.finalize_frame_updates()
            };
            if should_schedule {
                web_schedule_redraw(&window, &raf_pending, &raf_callback_ref);
            }
        }) as Box<dyn FnMut(web_sys::Event)>);
        let _ = canvas.add_event_listener_with_callback("keydown", closure.as_ref().unchecked_ref());
        closure.forget();
    }

    {
        let app = app.clone();
        let window = window.clone();
        let raf_pending = raf_pending.clone();
        let raf_callback_ref = raf_callback.clone();
        let closure = Closure::wrap(Box::new(move |event: web_sys::Event| {
            let Ok(event) = event.dyn_into::<KeyboardEvent>() else {
                return;
            };
            let Ok(mut app) = app.try_borrow_mut() else {
                return;
            };
            let should_schedule = {
                app.with_viewport_mut(|viewport| {
                    let key = event.key();
                    viewport.set_key_pressed(key.clone(), false);
                    viewport.dispatch_key_up_event(key, event.code(), event.repeat());
                });
                app.finalize_frame_updates()
            };
            if should_schedule {
                web_schedule_redraw(&window, &raf_pending, &raf_callback_ref);
            }
        }) as Box<dyn FnMut(web_sys::Event)>);
        let _ = canvas.add_event_listener_with_callback("keyup", closure.as_ref().unchecked_ref());
        closure.forget();
    }

    {
        let app = app.clone();
        let window = window.clone();
        let raf_pending = raf_pending.clone();
        let raf_callback_ref = raf_callback.clone();
        let closure = Closure::wrap(Box::new(move |_event: web_sys::Event| {
            let Ok(mut app) = app.try_borrow_mut() else {
                return;
            };
            let should_schedule = {
                app.ime_composing = false;
                app.with_viewport_mut(|viewport| {
                    viewport.clear_input_state();
                });
                app.finalize_frame_updates()
            };
            if should_schedule {
                web_schedule_redraw(&window, &raf_pending, &raf_callback_ref);
            }
        }) as Box<dyn FnMut(web_sys::Event)>);
        let _ = canvas.add_event_listener_with_callback("blur", closure.as_ref().unchecked_ref());
        closure.forget();
    }
}

pub fn run() {
    #[cfg(target_arch = "wasm32")]
    {
        run_web();
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
    init_theme(Theme::dark());
    THEME_DARK_MODE.store(true, Ordering::Relaxed);
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App::default();
    app.background_color = app_background_color(true);
    app.applied_theme_dark = Some(true);
    event_loop.run_app(&mut app).unwrap();
    }
}
