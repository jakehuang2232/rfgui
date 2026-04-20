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
use rfgui::app::{App, AppConfig, AppEvent, WheelConfig};
use rfgui::platform::web_backend::{CanvasCursorSink, InMemoryClipboard};
use rfgui::platform::{
    Clipboard, CursorSink, PlatformImePreedit, PlatformKeyEvent, PlatformPointerButton,
    PlatformPointerEvent, PlatformPointerEventKind, PlatformServices, PlatformTextInput,
    PlatformWheelEvent, PointerType, RedrawRequester,
};
use rfgui::ui::run_due_timers;
use rfgui::view::viewport::{RenderFrameResult, Viewport};
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
    let runner = Runner::new(Box::new(app), config);
    event_loop.spawn_app(runner);
}

/// Lightweight shared cell used by the spawn_local closure to hand a
/// freshly-built `Viewport` back into the runner after surface init
/// completes.
type SharedViewport = Rc<RefCell<Option<Viewport>>>;

struct Runner {
    /// Holds the App until the Viewport is created, then `None`.
    pending_app: Rc<RefCell<Option<Box<dyn App>>>>,
    config: AppConfig,
    window: Option<Arc<Window>>,
    viewport: SharedViewport,
    clipboard: InMemoryClipboard,
    cursor_sink: Option<CanvasCursorSink>,
    redraw: WebRedrawRequester,
    last_mouse_logical: Option<(f32, f32)>,
    cursor_in_window: bool,
    ime_composing: bool,
    boot_overlay_hidden: bool,
    resize_listener: Option<wasm_bindgen::closure::Closure<dyn FnMut()>>,
    /// Set when async viewport init kicks off. Reset once the viewport
    /// actually appears in the shared cell. Prevents re-entry from a
    /// second `resumed` call.
    init_in_flight: bool,
}

impl Runner {
    fn new(app: Box<dyn App>, config: AppConfig) -> Self {
        Self {
            pending_app: Rc::new(RefCell::new(Some(app))),
            config,
            window: None,
            viewport: Rc::new(RefCell::new(None)),
            clipboard: InMemoryClipboard::default(),
            cursor_sink: None,
            redraw: WebRedrawRequester::default(),
            last_mouse_logical: None,
            cursor_in_window: false,
            ime_composing: false,
            boot_overlay_hidden: false,
            resize_listener: None,
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
        let (physical_size, scale) = sync_canvas_size(lookup_app_canvas().as_ref(), window.scale_factor() as f32);
        let clear_color = self.config.clear_color;
        let viewport_slot = self.viewport.clone();
        let pending_app = self.pending_app.clone();
        spawn_local(async move {
            let mut viewport = Viewport::new();
            if let Some(app) = pending_app.borrow_mut().take() {
                viewport.set_app(app);
            }
            viewport.set_msaa_sample_count(1);
            viewport.set_scale_factor(scale);
            viewport.set_size(physical_size.0, physical_size.1);
            viewport.set_surface_format_preference(SurfaceFormatPreference::PreferSrgb);
            if let Some(color) = clear_color {
                viewport.set_clear_color(Box::new(color));
            }
            viewport.attach(window.clone()).await;
            *viewport_slot.borrow_mut() = Some(viewport);
            window.request_redraw();
        });
    }

    fn install_resize_listener(&mut self) {
        if self.resize_listener.is_some() {
            return;
        }
        let Some(window) = self.window.clone() else {
            return;
        };
        let Some(web_window) = web_sys::window() else {
            return;
        };
        let viewport_slot = self.viewport.clone();
        let closure = wasm_bindgen::closure::Closure::wrap(Box::new(move || {
            let canvas = lookup_app_canvas();
            let (size, scale) =
                sync_canvas_size(canvas.as_ref(), window.scale_factor() as f32);
            if let Some(viewport) = viewport_slot.borrow_mut().as_mut() {
                viewport.set_scale_factor(scale);
                viewport.set_size(size.0, size.1);
            }
            window.request_redraw();
        }) as Box<dyn FnMut()>);
        let _ = web_window.add_event_listener_with_callback(
            "resize",
            closure.as_ref().unchecked_ref(),
        );
        self.resize_listener = Some(closure);
    }

    fn ensure_ready(&mut self) {
        if let Some(viewport) = self.viewport.borrow_mut().as_mut() {
            let cursor_sink: &mut dyn CursorSink = match self.cursor_sink.as_mut() {
                Some(sink) => sink,
                None => &mut NoopCursorSink,
            };
            viewport.app_on_ready(PlatformServices {
                clipboard: &mut self.clipboard,
                cursor: cursor_sink,
                redraw: &self.redraw,
            });
        }
    }

    fn render_once(&mut self) {
        self.ensure_ready();
        let result = {
            let mut viewport_borrow = self.viewport.borrow_mut();
            let Some(viewport) = viewport_borrow.as_mut() else {
                return;
            };
            let cursor_sink: &mut dyn CursorSink = match self.cursor_sink.as_mut() {
                Some(sink) => sink,
                None => &mut NoopCursorSink,
            };
            viewport.render_frame(PlatformServices {
                clipboard: &mut self.clipboard,
                cursor: cursor_sink,
                redraw: &self.redraw,
            })
        };
        let needs_retry = matches!(result, RenderFrameResult::NeedsRetry);
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
        {
            let mut vp = self.viewport.borrow_mut();
            if let Some(viewport) = vp.as_mut() {
                let cursor_sink: &mut dyn CursorSink = match self.cursor_sink.as_mut() {
                    Some(sink) => sink,
                    None => &mut NoopCursorSink,
                };
                viewport.dispatch_app_event(&app_event, PlatformServices {
                    clipboard: &mut self.clipboard,
                    cursor: cursor_sink,
                    redraw: &self.redraw,
                });
                let _ = viewport.dispatch_platform_key_event(&platform_event);
            }
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
                        let mut vp = self.viewport.borrow_mut();
                        if let Some(viewport) = vp.as_mut() {
                            let cursor_sink: &mut dyn CursorSink = match self.cursor_sink.as_mut() {
                                Some(sink) => sink,
                                None => &mut NoopCursorSink,
                            };
                            viewport.dispatch_app_event(&ti_event, PlatformServices {
                                clipboard: &mut self.clipboard,
                                cursor: cursor_sink,
                                redraw: &self.redraw,
                            });
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
                let mut vp = self.viewport.borrow_mut();
                if let Some(viewport) = vp.as_mut() {
                    let cursor_sink: &mut dyn CursorSink = match self.cursor_sink.as_mut() {
                        Some(sink) => sink,
                        None => &mut NoopCursorSink,
                    };
                    viewport.dispatch_app_event(&app_event, PlatformServices {
                        clipboard: &mut self.clipboard,
                        cursor: cursor_sink,
                        redraw: &self.redraw,
                    });
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
                let mut vp = self.viewport.borrow_mut();
                if let Some(viewport) = vp.as_mut() {
                    let cursor_sink: &mut dyn CursorSink = match self.cursor_sink.as_mut() {
                        Some(sink) => sink,
                        None => &mut NoopCursorSink,
                    };
                    viewport.dispatch_app_event(&app_event, PlatformServices {
                        clipboard: &mut self.clipboard,
                        cursor: cursor_sink,
                        redraw: &self.redraw,
                    });
                    let _ = viewport.dispatch_platform_text_input(&ti);
                }
            }
        }
    }
}

impl ApplicationHandler for Runner {
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
        self.install_resize_listener();
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
                {
                    let mut vp = self.viewport.borrow_mut();
                    if let Some(viewport) = vp.as_mut() {
                        let cursor_sink: &mut dyn CursorSink = match self.cursor_sink.as_mut() {
                            Some(sink) => sink,
                            None => &mut NoopCursorSink,
                        };
                        let close = AppEvent::CloseRequested;
                        viewport.dispatch_app_event(&close, PlatformServices {
                            clipboard: &mut self.clipboard,
                            cursor: cursor_sink,
                            redraw: &self.redraw,
                        });
                        let cursor_sink: &mut dyn CursorSink = match self.cursor_sink.as_mut() {
                            Some(sink) => sink,
                            None => &mut NoopCursorSink,
                        };
                        viewport.app_on_shutdown(PlatformServices {
                            clipboard: &mut self.clipboard,
                            cursor: cursor_sink,
                            redraw: &self.redraw,
                        });
                    }
                }
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                let mut vp = self.viewport.borrow_mut();
                if let Some(viewport) = vp.as_mut() {
                    viewport.set_size(size.width, size.height);
                    let ev = AppEvent::Resized {
                        width: size.width,
                        height: size.height,
                    };
                    let cursor_sink: &mut dyn CursorSink = match self.cursor_sink.as_mut() {
                        Some(sink) => sink,
                        None => &mut NoopCursorSink,
                    };
                    viewport.dispatch_app_event(&ev, PlatformServices {
                        clipboard: &mut self.clipboard,
                        cursor: cursor_sink,
                        redraw: &self.redraw,
                    });
                }
                drop(vp);
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                let mut vp = self.viewport.borrow_mut();
                if let Some(viewport) = vp.as_mut() {
                    viewport.set_scale_factor(scale_factor as f32);
                    let ev = AppEvent::ScaleFactorChanged {
                        scale: scale_factor as f32,
                    };
                    let cursor_sink: &mut dyn CursorSink = match self.cursor_sink.as_mut() {
                        Some(sink) => sink,
                        None => &mut NoopCursorSink,
                    };
                    viewport.dispatch_app_event(&ev, PlatformServices {
                        clipboard: &mut self.clipboard,
                        cursor: cursor_sink,
                        redraw: &self.redraw,
                    });
                }
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
                let move_event = PlatformPointerEvent {
                    kind: PlatformPointerEventKind::Move {
                        x: logical.0,
                        y: logical.1,
                    },
                    pointer_id: 0,
                    pointer_type: PointerType::Mouse,
                    pressure: 0.0,
                };
                let ev = AppEvent::Pointer(move_event);
                let mut vp = self.viewport.borrow_mut();
                if let Some(viewport) = vp.as_mut() {
                    let cursor_sink: &mut dyn CursorSink = match self.cursor_sink.as_mut() {
                        Some(sink) => sink,
                        None => &mut NoopCursorSink,
                    };
                    viewport.dispatch_app_event(&ev, PlatformServices {
                        clipboard: &mut self.clipboard,
                        cursor: cursor_sink,
                        redraw: &self.redraw,
                    });
                    let _ = viewport.dispatch_platform_pointer_event(&move_event);
                }
            }
            WindowEvent::CursorLeft { .. } => {
                self.cursor_in_window = false;
                if let Some(viewport) = self.viewport.borrow_mut().as_mut() {
                    viewport.clear_pointer_position_viewport();
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let Some(mapped) = winit_button_to_platform(button) else {
                    return;
                };
                let pressed = matches!(state, ElementState::Pressed);
                {
                    let mut vp = self.viewport.borrow_mut();
                    if let Some(viewport) = vp.as_mut() {
                        viewport
                            .set_pointer_button_pressed(platform_button_to_viewport(mapped), pressed);
                        let kind = if pressed {
                            PlatformPointerEventKind::Down(mapped)
                        } else {
                            PlatformPointerEventKind::Up(mapped)
                        };
                        let pressure = if pressed { 0.5 } else { 0.0 };
                        let ev = AppEvent::Pointer(PlatformPointerEvent {
                            kind,
                            pointer_id: 0,
                            pointer_type: PointerType::Mouse,
                            pressure,
                        });
                        let cursor_sink: &mut dyn CursorSink = match self.cursor_sink.as_mut() {
                            Some(sink) => sink,
                            None => &mut NoopCursorSink,
                        };
                        viewport.dispatch_app_event(&ev, PlatformServices {
                            clipboard: &mut self.clipboard,
                            cursor: cursor_sink,
                            redraw: &self.redraw,
                        });
                        let _ = viewport.dispatch_platform_pointer_event(&PlatformPointerEvent {
                            kind,
                            pointer_id: 0,
                            pointer_type: PointerType::Mouse,
                            pressure,
                        });
                        if !pressed {
                            let click = PlatformPointerEvent {
                                kind: PlatformPointerEventKind::Click(mapped),
                                pointer_id: 0,
                                pointer_type: PointerType::Mouse,
                                pressure: 0.0,
                            };
                            let click_ev = AppEvent::Pointer(click);
                            let cursor_sink: &mut dyn CursorSink = match self.cursor_sink.as_mut() {
                                Some(sink) => sink,
                                None => &mut NoopCursorSink,
                            };
                            viewport.dispatch_app_event(&click_ev, PlatformServices {
                                clipboard: &mut self.clipboard,
                                cursor: cursor_sink,
                                redraw: &self.redraw,
                            });
                            let _ = viewport.dispatch_platform_pointer_event(&click);
                        }
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
                let mut vp = self.viewport.borrow_mut();
                if let Some(viewport) = vp.as_mut() {
                    let cursor_sink: &mut dyn CursorSink = match self.cursor_sink.as_mut() {
                        Some(sink) => sink,
                        None => &mut NoopCursorSink,
                    };
                    viewport.dispatch_app_event(&ev, PlatformServices {
                        clipboard: &mut self.clipboard,
                        cursor: cursor_sink,
                        redraw: &self.redraw,
                    });
                    let _ = viewport.dispatch_platform_wheel_event(&PlatformWheelEvent {
                        delta_x: -dx,
                        delta_y: -dy,
                    });
                }
            }
            WindowEvent::Focused(focused) => {
                let ev = AppEvent::HostFocus(focused);
                {
                    let mut vp = self.viewport.borrow_mut();
                    if let Some(viewport) = vp.as_mut() {
                        let cursor_sink: &mut dyn CursorSink = match self.cursor_sink.as_mut() {
                            Some(sink) => sink,
                            None => &mut NoopCursorSink,
                        };
                        viewport.dispatch_app_event(&ev, PlatformServices {
                            clipboard: &mut self.clipboard,
                            cursor: cursor_sink,
                            redraw: &self.redraw,
                        });
                    }
                }
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

/// Read the canvas's CSS box, multiply by the device pixel ratio, and write
/// the result back into `canvas.width` / `canvas.height` so the wgpu surface
/// is configured at the same physical resolution the browser is composing.
/// Returns `(physical_size, scale_factor)` for the caller to push into the
/// viewport.
fn sync_canvas_size(
    canvas: Option<&HtmlCanvasElement>,
    fallback_scale: f32,
) -> ((u32, u32), f32) {
    let Some(canvas) = canvas else {
        return ((1, 1), fallback_scale);
    };
    let dpr = window()
        .map(|w| w.device_pixel_ratio() as f32)
        .filter(|v| *v > 0.0)
        .unwrap_or(fallback_scale);
    let client_w = canvas.client_width().max(1) as f32;
    let client_h = canvas.client_height().max(1) as f32;
    let physical_w = (client_w * dpr).round().max(1.0) as u32;
    let physical_h = (client_h * dpr).round().max(1.0) as u32;
    if canvas.width() != physical_w {
        canvas.set_width(physical_w);
    }
    if canvas.height() != physical_h {
        canvas.set_height(physical_h);
    }
    ((physical_w, physical_h), dpr)
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

fn winit_button_to_platform(button: WinitMouseButton) -> Option<PlatformPointerButton> {
    Some(match button {
        WinitMouseButton::Left => PlatformPointerButton::Left,
        WinitMouseButton::Right => PlatformPointerButton::Right,
        WinitMouseButton::Middle => PlatformPointerButton::Middle,
        WinitMouseButton::Back => PlatformPointerButton::Back,
        WinitMouseButton::Forward => PlatformPointerButton::Forward,
        WinitMouseButton::Other(code) => PlatformPointerButton::Other(code),
    })
}

fn platform_button_to_viewport(
    button: PlatformPointerButton,
) -> rfgui::view::viewport::PointerButton {
    use rfgui::view::viewport::PointerButton as Vb;
    match button {
        PlatformPointerButton::Left => Vb::Left,
        PlatformPointerButton::Right => Vb::Right,
        PlatformPointerButton::Middle => Vb::Middle,
        PlatformPointerButton::Back => Vb::Back,
        PlatformPointerButton::Forward => Vb::Forward,
        PlatformPointerButton::Other(code) => Vb::Other(code),
    }
}
