//! Wasm runner that mirrors `winit_runner` for browser hosts.
//!
//! winit on wasm uses the same `ApplicationHandler` shape as native, so
//! the bulk of the event-translation logic is identical. The two
//! differences this file owns:
//!
//! 1. Viewport setup is unavoidably async on wasm (wgpu adapter +
//!    surface). We use `wasm_bindgen_futures::spawn_local` and store the
//!    viewport behind `Rc<RefCell<...>>` so the future can write it back
//!    after `resumed` returns.
//! 2. There is no host clipboard or `arboard`; everything lives in an
//!    in-memory shim. Cursor goes through `CanvasCursorSink` on the
//!    canvas's CSS style.
//!
//! Existing `index.html` keeps a `<canvas id="app-canvas">` element; we
//! pass it to winit via `WindowAttributesExtWebSys::with_canvas` so
//! winit and our scene paint into the same canvas the bootstrap script
//! sized.

#![cfg(target_arch = "wasm32")]

use rfgui::SurfaceFormatPreference;
use rfgui::app::{App, AppConfig, AppContext, AppEvent, WheelConfig};
use rfgui::platform::web_backend::{CanvasCursorSink, InMemoryClipboard};
use rfgui::platform::{
    Clipboard, CursorSink, PlatformImePreedit, PlatformKeyEvent, PlatformMouseButton,
    PlatformMouseEvent, PlatformMouseEventKind, PlatformServices, PlatformTextInput,
    PlatformWheelEvent, RedrawRequester,
};
use rfgui::ui::{RsxNode, peek_state_dirty, run_due_timers};
use rfgui::view::viewport::{Viewport, ViewportControl};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;
use web_sys::{HtmlCanvasElement, window};
use web_time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{
    ElementState, Ime, KeyEvent, MouseButton as WinitMouseButton, MouseScrollDelta, WindowEvent,
};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey, PhysicalKey};
use winit::platform::web::{EventLoopExtWebSys, WindowAttributesExtWebSys};
use winit::window::{Window, WindowId};

/// Spawn the rfgui `App` on the browser event loop.
///
/// Non-blocking — winit's wasm backend hooks into the JS event loop
/// rather than running its own. `console_error_panic_hook` is installed
/// so Rust panics surface as a readable JS error.
pub fn run<A: App + 'static>(app: A, mut config: AppConfig) {
    console_error_panic_hook::set_once();
    let _ = &mut config; // currently no wasm-only mutation; reserved.
    let event_loop = EventLoop::new().expect("failed to create winit event loop");
    event_loop.set_control_flow(ControlFlow::Wait);
    let runner = Runner::new(app, config);
    event_loop.spawn_app(runner);
}

/// Lightweight shared cell used by the spawn_local closure to hand a
/// freshly-built `Viewport` back into the runner after surface init
/// completes.
type SharedViewport = Rc<RefCell<Option<Viewport>>>;

struct Runner<A: App> {
    app: A,
    config: AppConfig,
    window: Option<Arc<Window>>,
    viewport: SharedViewport,
    clipboard: InMemoryClipboard,
    cursor_sink: Option<CanvasCursorSink>,
    redraw: WebRedrawRequester,
    last_mouse_logical: Option<(f32, f32)>,
    cursor_in_window: bool,
    ime_composing: bool,
    cached_rsx: Option<RsxNode>,
    needs_rebuild: bool,
    ready_dispatched: bool,
    boot_overlay_hidden: bool,
    /// Set when async viewport init kicks off. Reset once the viewport
    /// actually appears in the shared cell. Prevents re-entry from a
    /// second `resumed` call.
    init_in_flight: bool,
}

impl<A: App> Runner<A> {
    fn new(app: A, config: AppConfig) -> Self {
        Self {
            app,
            config,
            window: None,
            viewport: Rc::new(RefCell::new(None)),
            clipboard: InMemoryClipboard::default(),
            cursor_sink: None,
            redraw: WebRedrawRequester::default(),
            last_mouse_logical: None,
            cursor_in_window: false,
            ime_composing: false,
            cached_rsx: None,
            needs_rebuild: true,
            ready_dispatched: false,
            boot_overlay_hidden: false,
            init_in_flight: false,
        }
    }

    fn ensure_viewport(&mut self) {
        if self.viewport.borrow().is_some() || self.init_in_flight {
            return;
        }
        let Some(window) = self.window.clone() else {
            return;
        };
        self.init_in_flight = true;
        let scale = window.scale_factor() as f32;
        let size = window.inner_size();
        let clear_color = self.config.clear_color;
        let viewport_slot = self.viewport.clone();
        spawn_local(async move {
            let mut viewport = Viewport::new();
            viewport.set_msaa_sample_count(1);
            viewport.set_scale_factor(scale);
            viewport.set_size(size.width, size.height);
            viewport.set_surface_format_preference(SurfaceFormatPreference::PreferNonSrgb);
            if let Some(color) = clear_color {
                viewport.set_clear_color(Box::new(color));
            }
            viewport.attach(window.clone()).await;
            viewport.create_surface().await;
            *viewport_slot.borrow_mut() = Some(viewport);
            // Wake the event loop so the first frame paints.
            window.request_redraw();
        });
    }

    fn ensure_ready(&mut self) {
        if self.ready_dispatched {
            return;
        }
        if self.viewport.borrow().is_none() {
            return;
        }
        self.with_ctx(|app, ctx| app.on_ready(ctx));
        self.ready_dispatched = true;
    }

    fn with_ctx<R>(&mut self, f: impl FnOnce(&mut A, &mut AppContext<'_>) -> R) -> Option<R> {
        let mut viewport_borrow = self.viewport.borrow_mut();
        let viewport = viewport_borrow.as_mut()?;
        let cursor_sink: &mut dyn CursorSink = match self.cursor_sink.as_mut() {
            Some(sink) => sink,
            None => &mut NoopCursorSink,
        };
        let mut ctx = AppContext {
            viewport: ViewportControl::new(viewport),
            services: PlatformServices {
                clipboard: &mut self.clipboard,
                cursor: cursor_sink,
                redraw: &self.redraw,
            },
        };
        Some(f(&mut self.app, &mut ctx))
    }

    fn render_once(&mut self) {
        self.ensure_ready();
        if self.viewport.borrow().is_none() {
            return;
        }
        if peek_state_dirty().needs_rebuild() {
            self.needs_rebuild = true;
        }
        if self.needs_rebuild || self.cached_rsx.is_none() {
            let rsx = self.with_ctx(|app, ctx| app.build(ctx));
            if let Some(rsx) = rsx {
                self.cached_rsx = Some(rsx);
            }
            self.needs_rebuild = false;
        }
        if let Some(rsx) = self.cached_rsx.clone() {
            if let Some(viewport) = self.viewport.borrow_mut().as_mut() {
                let _ = viewport.render_rsx(&rsx);
            }
        }
        // Retry on the next tick when the canvas surface is briefly
        // occluded right after creation — `begin_frame` then silently
        // bails and produces no geometry, leaving the canvas blank until
        // an unrelated event pokes the loop.
        let needs_retry = self.cached_rsx.is_some()
            && self
                .viewport
                .borrow()
                .as_ref()
                .map(|v| v.frame_box_models().is_empty())
                .unwrap_or(false);
        if needs_retry {
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
        self.drain_and_apply();
        if !self.boot_overlay_hidden && !needs_retry {
            hide_boot_overlay();
            self.boot_overlay_hidden = true;
        }
    }

    fn drain_and_apply(&mut self) {
        let mut want_redraw = self.redraw.take();
        if let Some(viewport) = self.viewport.borrow_mut().as_mut() {
            let requests = viewport.drain_platform_requests();
            if let Some(cursor) = requests.cursor {
                if let Some(sink) = self.cursor_sink.as_mut() {
                    sink.set_cursor(cursor);
                }
            }
            if requests.request_redraw {
                want_redraw = true;
            }
            if let Some(text) = requests.clipboard_write {
                self.clipboard.set(&text);
            }
        }
        if want_redraw {
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
    }

    fn normalize_wheel(&self, delta: MouseScrollDelta) -> Option<(f32, f32)> {
        let cfg: WheelConfig = self.config.wheel;
        let viewport_borrow = self.viewport.borrow();
        let viewport = viewport_borrow.as_ref()?;
        let (dx, dy) = match delta {
            MouseScrollDelta::LineDelta(x, y) => (x * cfg.mouse_line_step, y * cfg.mouse_line_step),
            MouseScrollDelta::PixelDelta(pos) => {
                let (lx, ly) =
                    viewport.physical_to_logical_point(pos.x as f32, pos.y as f32);
                let lx = lx * cfg.touchpad_pixel_scale;
                let ly = ly * cfg.touchpad_pixel_scale;
                let lx = if lx.abs() < cfg.touchpad_deadzone { 0.0 } else { lx };
                let ly = if ly.abs() < cfg.touchpad_deadzone { 0.0 } else { ly };
                (lx, ly)
            }
        };
        if dx.abs() <= f32::EPSILON && dy.abs() <= f32::EPSILON {
            return None;
        }
        Some((dx, dy))
    }

    fn handle_keyboard(&mut self, event: KeyEvent) {
        let key_str = key_to_string(&event.logical_key);
        let code_str = physical_key_to_string(&event.physical_key);
        let pressed = matches!(event.state, ElementState::Pressed);
        if let Some(viewport) = self.viewport.borrow_mut().as_mut() {
            viewport.set_key_pressed(key_str.clone(), pressed);
        }
        let platform_event = PlatformKeyEvent {
            key: key_str,
            code: code_str,
            repeat: event.repeat,
            pressed,
        };
        let app_event = AppEvent::Key(platform_event.clone());
        self.with_ctx(|app, ctx| app.on_event(&app_event, ctx));
        if let Some(viewport) = self.viewport.borrow_mut().as_mut() {
            let _ = viewport.dispatch_platform_key_event(&platform_event);
        }
        if pressed && !self.ime_composing {
            if let Some(text) = event.text.as_ref() {
                if !text.is_empty() {
                    let allow = self
                        .viewport
                        .borrow()
                        .as_ref()
                        .map(|v| {
                            let m_alt = v.is_key_pressed("Alt");
                            let m_ctrl = v.is_key_pressed("Control");
                            let m_meta = v.is_key_pressed("Meta") || v.is_key_pressed("Super");
                            !(m_alt || m_ctrl || m_meta)
                        })
                        .unwrap_or(true);
                    if allow {
                        let ti = PlatformTextInput {
                            text: text.to_string(),
                        };
                        let ti_event = AppEvent::TextInput(ti.clone());
                        self.with_ctx(|app, ctx| app.on_event(&ti_event, ctx));
                        if let Some(viewport) = self.viewport.borrow_mut().as_mut() {
                            let _ = viewport.dispatch_platform_text_input(&ti);
                        }
                    }
                }
            }
        }
    }

    fn handle_ime(&mut self, ime: Ime) {
        match ime {
            Ime::Enabled => {
                self.ime_composing = false;
            }
            Ime::Disabled => {
                self.ime_composing = false;
                let preedit = PlatformImePreedit {
                    text: String::new(),
                    cursor_start: None,
                    cursor_end: None,
                };
                if let Some(viewport) = self.viewport.borrow_mut().as_mut() {
                    let _ = viewport.dispatch_platform_ime_preedit(&preedit);
                }
            }
            Ime::Preedit(text, cursor) => {
                self.ime_composing = !text.is_empty();
                let (start, end) = match cursor {
                    Some((s, e)) => (Some(s), Some(e)),
                    None => (None, None),
                };
                let preedit = PlatformImePreedit {
                    text,
                    cursor_start: start,
                    cursor_end: end,
                };
                let app_event = AppEvent::ImePreedit(preedit.clone());
                self.with_ctx(|app, ctx| app.on_event(&app_event, ctx));
                if let Some(viewport) = self.viewport.borrow_mut().as_mut() {
                    let _ = viewport.dispatch_platform_ime_preedit(&preedit);
                }
            }
            Ime::Commit(text) => {
                self.ime_composing = false;
                if text.is_empty() {
                    return;
                }
                let ti = PlatformTextInput { text };
                let app_event = AppEvent::TextInput(ti.clone());
                self.with_ctx(|app, ctx| app.on_event(&app_event, ctx));
                if let Some(viewport) = self.viewport.borrow_mut().as_mut() {
                    let _ = viewport.dispatch_platform_text_input(&ti);
                }
            }
        }
    }
}

impl<A: App + 'static> ApplicationHandler for Runner<A> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let canvas = lookup_app_canvas();
        let mut attrs = Window::default_attributes()
            .with_title(&self.config.title)
            .with_transparent(self.config.transparent);
        attrs = attrs.with_canvas(canvas.clone());
        if canvas.is_none() {
            attrs = attrs.with_append(true);
        }
        let window = Arc::new(
            event_loop
                .create_window(attrs)
                .expect("failed to create winit window"),
        );
        window.set_ime_allowed(true);
        if let Some(canvas) = canvas {
            self.cursor_sink = Some(CanvasCursorSink::new(canvas));
        }
        self.window = Some(window);
        self.ensure_viewport();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _id: WindowId,
        event: WindowEvent,
    ) {
        // Viewport may not exist yet on the very first events while
        // async surface init is still in flight; ensure_viewport keeps
        // trying so the next event will have it.
        if self.viewport.borrow().is_none() {
            self.ensure_viewport();
        }
        self.ensure_ready();
        match event {
            WindowEvent::CloseRequested => {
                let close = AppEvent::CloseRequested;
                self.with_ctx(|app, ctx| app.on_event(&close, ctx));
                self.with_ctx(|app, ctx| app.on_shutdown(ctx));
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                if let Some(viewport) = self.viewport.borrow_mut().as_mut() {
                    viewport.set_size(size.width, size.height);
                }
                let ev = AppEvent::Resized {
                    width: size.width,
                    height: size.height,
                };
                self.with_ctx(|app, ctx| app.on_event(&ev, ctx));
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                if let Some(viewport) = self.viewport.borrow_mut().as_mut() {
                    viewport.set_scale_factor(scale_factor as f32);
                }
                let ev = AppEvent::ScaleFactorChanged {
                    scale: scale_factor as f32,
                };
                self.with_ctx(|app, ctx| app.on_event(&ev, ctx));
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor_in_window = true;
                let logical = self
                    .viewport
                    .borrow()
                    .as_ref()
                    .map(|v| v.physical_to_logical_point(position.x as f32, position.y as f32))
                    .unwrap_or((position.x as f32, position.y as f32));
                self.last_mouse_logical = Some(logical);
                let move_event = PlatformMouseEvent {
                    kind: PlatformMouseEventKind::Move {
                        x: logical.0,
                        y: logical.1,
                    },
                };
                let ev = AppEvent::Mouse(move_event);
                self.with_ctx(|app, ctx| app.on_event(&ev, ctx));
                if let Some(viewport) = self.viewport.borrow_mut().as_mut() {
                    let _ = viewport.dispatch_platform_mouse_event(&move_event);
                }
            }
            WindowEvent::CursorLeft { .. } => {
                self.cursor_in_window = false;
                if let Some(viewport) = self.viewport.borrow_mut().as_mut() {
                    viewport.clear_mouse_position_viewport();
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let Some(mapped) = winit_button_to_platform(button) else {
                    return;
                };
                let pressed = matches!(state, ElementState::Pressed);
                if let Some(viewport) = self.viewport.borrow_mut().as_mut() {
                    viewport
                        .set_mouse_button_pressed(platform_button_to_viewport(mapped), pressed);
                }
                let kind = if pressed {
                    PlatformMouseEventKind::Down(mapped)
                } else {
                    PlatformMouseEventKind::Up(mapped)
                };
                let ev = AppEvent::Mouse(PlatformMouseEvent { kind });
                self.with_ctx(|app, ctx| app.on_event(&ev, ctx));
                if let Some(viewport) = self.viewport.borrow_mut().as_mut() {
                    let _ = viewport.dispatch_platform_mouse_event(&PlatformMouseEvent { kind });
                }
                if !pressed {
                    let click = PlatformMouseEvent {
                        kind: PlatformMouseEventKind::Click(mapped),
                    };
                    let click_ev = AppEvent::Mouse(click);
                    self.with_ctx(|app, ctx| app.on_event(&click_ev, ctx));
                    if let Some(viewport) = self.viewport.borrow_mut().as_mut() {
                        let _ = viewport.dispatch_platform_mouse_event(&click);
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let Some((dx, dy)) = self.normalize_wheel(delta) else {
                    return;
                };
                let ev = AppEvent::Wheel(PlatformWheelEvent {
                    delta_x: -dx,
                    delta_y: -dy,
                });
                self.with_ctx(|app, ctx| app.on_event(&ev, ctx));
                if let Some(viewport) = self.viewport.borrow_mut().as_mut() {
                    let _ = viewport.dispatch_platform_wheel_event(&PlatformWheelEvent {
                        delta_x: -dx,
                        delta_y: -dy,
                    });
                }
            }
            WindowEvent::Focused(focused) => {
                let ev = AppEvent::HostFocus(focused);
                self.with_ctx(|app, ctx| app.on_event(&ev, ctx));
                if !focused {
                    self.ime_composing = false;
                    if let Some(viewport) = self.viewport.borrow_mut().as_mut() {
                        viewport.clear_input_state();
                    }
                }
            }
            WindowEvent::KeyboardInput {
                event: key_event, ..
            } => {
                self.handle_keyboard(key_event);
            }
            WindowEvent::Ime(ime) => {
                self.handle_ime(ime);
            }
            WindowEvent::RedrawRequested => {
                self.render_once();
            }
            _ => {}
        }
        self.drain_and_apply();
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        run_due_timers(now.into());
        if self.redraw.take() {
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
        // Browser event loops drive themselves via requestAnimationFrame
        // when the viewport requests another redraw, so we never hold the
        // loop in a tight `Poll`. winit's wasm backend treats `Wait` as
        // "yield to the JS scheduler"; the queued `RedrawRequested` will
        // dispatch on the next animation frame regardless of the
        // ControlFlow setting we choose here.
        event_loop.set_control_flow(ControlFlow::Wait);
    }
}

/// `RedrawRequester` impl that records into a per-runner `Cell` instead
/// of going through `Send + Sync`. Wasm is single-threaded so the bound
/// is unnecessary friction.
#[derive(Default)]
struct WebRedrawRequester {
    flag: Rc<std::cell::Cell<bool>>,
}

impl WebRedrawRequester {
    fn take(&self) -> bool {
        let v = self.flag.get();
        self.flag.set(false);
        v
    }
}

impl RedrawRequester for WebRedrawRequester {
    fn request_redraw(&self) {
        self.flag.set(true);
    }
}

/// Stand-in cursor sink used while the canvas isn't available yet.
struct NoopCursorSink;
impl CursorSink for NoopCursorSink {
    fn set_cursor(&mut self, _cursor: rfgui::Cursor) {}
}

fn lookup_app_canvas() -> Option<HtmlCanvasElement> {
    use wasm_bindgen::JsCast;
    let document = window()?.document()?;
    let element = document.get_element_by_id("app-canvas")?;
    element.dyn_into::<HtmlCanvasElement>().ok()
}

fn key_to_string(key: &Key) -> String {
    match key {
        Key::Character(c) => c.to_string(),
        Key::Named(named) => named_key_to_string(*named).to_string(),
        Key::Unidentified(_) => String::from("Unidentified"),
        Key::Dead(_) => String::from("Dead"),
    }
}

fn named_key_to_string(key: NamedKey) -> &'static str {
    match key {
        NamedKey::Enter => "Enter",
        NamedKey::Tab => "Tab",
        NamedKey::Space => " ",
        NamedKey::Backspace => "Backspace",
        NamedKey::Delete => "Delete",
        NamedKey::Escape => "Escape",
        NamedKey::ArrowLeft => "ArrowLeft",
        NamedKey::ArrowRight => "ArrowRight",
        NamedKey::ArrowUp => "ArrowUp",
        NamedKey::ArrowDown => "ArrowDown",
        NamedKey::Home => "Home",
        NamedKey::End => "End",
        NamedKey::PageUp => "PageUp",
        NamedKey::PageDown => "PageDown",
        NamedKey::Shift => "Shift",
        NamedKey::Control => "Control",
        NamedKey::Alt => "Alt",
        NamedKey::Super => "Meta",
        NamedKey::CapsLock => "CapsLock",
        NamedKey::F1 => "F1",
        NamedKey::F2 => "F2",
        NamedKey::F3 => "F3",
        NamedKey::F4 => "F4",
        NamedKey::F5 => "F5",
        NamedKey::F6 => "F6",
        NamedKey::F7 => "F7",
        NamedKey::F8 => "F8",
        NamedKey::F9 => "F9",
        NamedKey::F10 => "F10",
        NamedKey::F11 => "F11",
        NamedKey::F12 => "F12",
        _ => "Unidentified",
    }
}

fn physical_key_to_string(key: &PhysicalKey) -> String {
    match key {
        PhysicalKey::Code(code) => format!("{code:?}"),
        PhysicalKey::Unidentified(_) => String::from("Unidentified"),
    }
}

fn hide_boot_overlay() {
    let Some(win) = window() else { return };
    let boot = match js_sys::Reflect::get(&win, &wasm_bindgen::JsValue::from_str("__RFGUI_BOOT__")) {
        Ok(v) if !v.is_undefined() && !v.is_null() => v,
        _ => return,
    };
    let func = match js_sys::Reflect::get(&boot, &wasm_bindgen::JsValue::from_str("hideBootOverlay")) {
        Ok(v) => v,
        Err(_) => return,
    };
    if let Ok(func) = func.dyn_into::<js_sys::Function>() {
        let _ = func.call0(&boot);
    }
}

fn winit_button_to_platform(button: WinitMouseButton) -> Option<PlatformMouseButton> {
    Some(match button {
        WinitMouseButton::Left => PlatformMouseButton::Left,
        WinitMouseButton::Right => PlatformMouseButton::Right,
        WinitMouseButton::Middle => PlatformMouseButton::Middle,
        WinitMouseButton::Back => PlatformMouseButton::Back,
        WinitMouseButton::Forward => PlatformMouseButton::Forward,
        WinitMouseButton::Other(code) => PlatformMouseButton::Other(code),
    })
}

fn platform_button_to_viewport(
    button: PlatformMouseButton,
) -> rfgui::view::viewport::MouseButton {
    use rfgui::view::viewport::MouseButton as Vb;
    match button {
        PlatformMouseButton::Left => Vb::Left,
        PlatformMouseButton::Right => Vb::Right,
        PlatformMouseButton::Middle => Vb::Middle,
        PlatformMouseButton::Back => Vb::Back,
        PlatformMouseButton::Forward => Vb::Forward,
        PlatformMouseButton::Other(code) => Vb::Other(code),
    }
}
