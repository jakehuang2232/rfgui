//! Minimal winit runner for the rfgui `App` trait.
//!
//! Native-only (the wasm examples still roll their own DOM glue). This is
//! intentionally small: it stands up a winit `EventLoop` + window, attaches
//! a `Viewport`, translates a core subset of `WindowEvent`s into
//! `Platform*Event`, drives `App::build`, and drains the viewport's
//! pending platform requests after each batch.
//!
//! Existing examples still use their hand-written `ApplicationHandler`
//! impls for features the runner doesn't cover yet (IME, theme sync,
//! custom mouse button mapping, …). Once the runner gains parity, those
//! examples can migrate incrementally.

#![cfg(not(target_arch = "wasm32"))]

use crate::winit_key_map::{physical_key_to_rf, winit_modifiers_to_rf};
use rfgui::app::{App, AppConfig, AppEvent, WheelConfig};
use rfgui::platform::desktop_backend::ArboardClipboard;
use rfgui::platform::{
    CallbackCursorSink, CallbackRedrawRequester, Clipboard, NullClipboard, PlatformImePreedit,
    PlatformKeyEvent, PlatformPointerButton, PlatformPointerEvent, PlatformPointerEventKind,
    PlatformServices, PlatformTextInput, PlatformWheelEvent, PointerType,
};
use rfgui::ui::{next_timer_deadline, run_due_timers};
use rfgui::view::viewport::{RenderFrameResult, Viewport};
use smol_str::SmolStr;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition, PhysicalSize};
use winit::event::{
    DeviceEvent, DeviceId, ElementState, Ime, KeyEvent, MouseButton as WinitMouseButton,
    MouseScrollDelta, WindowEvent,
};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

/// Run an `App` until the user closes the window.
///
/// Blocks the calling thread. Returns on `WindowEvent::CloseRequested`
/// after calling `App::on_shutdown`.
pub fn run<A: App + 'static>(app: A, config: AppConfig) {
    let event_loop = EventLoop::new().expect("failed to create winit event loop");
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut handler = Runner::new(Box::new(app), config);
    event_loop
        .run_app(&mut handler)
        .expect("winit event loop exited with error");
}

struct Runner {
    /// Holds the App until the Viewport is created, then `None`.
    pending_app: Option<Box<dyn App>>,
    config: AppConfig,
    window: Option<Arc<Window>>,
    viewport: Option<Viewport>,
    /// Backend clipboard. Falls back to `NullClipboard` on machines where
    /// `arboard` cannot bind (headless CI, some sandboxes).
    clipboard: Box<dyn Clipboard + Send>,
    /// Cursor applied to the winit window via `set_cursor` through a
    /// callback sink so the viewport never sees `winit::Window`.
    cursor: CallbackCursorSink,
    redraw: CallbackRedrawRequester,
    /// Shared cell that the redraw closure writes into. Picked up each
    /// tick and converted into `Window::request_redraw`.
    redraw_flag: Arc<Mutex<bool>>,
    last_mouse: Option<PhysicalPosition<f64>>,
    /// Last cursor position in logical (scale-factor-adjusted) viewport
    /// coordinates. Used by `DeviceEvent::MouseMotion` to keep drag
    /// tracking alive when the cursor leaves the window.
    last_mouse_logical: Option<(f32, f32)>,
    /// Whether the cursor is currently inside the window. `DeviceEvent`
    /// drag forwarding only activates when this is false.
    cursor_in_window: bool,
    /// True while the IME is composing a preedit string. Suppresses the
    /// fallback `event.text` text-input dispatch so the same character
    /// is not inserted twice.
    ime_composing: bool,
    last_ime_rect: Option<(i32, i32, u32, u32)>,
}

impl Runner {
    fn new(app: Box<dyn App>, config: AppConfig) -> Self {
        let clipboard: Box<dyn Clipboard + Send> = match ArboardClipboard::new() {
            Some(c) => Box::new(c),
            None => Box::new(NullClipboard::default()),
        };
        let cursor = CallbackCursorSink::new(|_| {});
        let redraw_flag = Arc::new(Mutex::new(false));
        let redraw_flag_write = redraw_flag.clone();
        let redraw = CallbackRedrawRequester::new(move || {
            *redraw_flag_write.lock().unwrap() = true;
        });
        Self {
            pending_app: Some(app),
            config,
            window: None,
            viewport: None,
            clipboard,
            cursor,
            redraw,
            redraw_flag,
            last_mouse: None,
            last_mouse_logical: None,
            cursor_in_window: false,
            ime_composing: false,
            last_ime_rect: None,
        }
    }

    fn ensure_viewport(&mut self) {
        if self.viewport.is_some() {
            return;
        }
        let Some(window) = &self.window else { return };
        let mut viewport = Viewport::new();
        if let Some(app) = self.pending_app.take() {
            viewport.set_app(app);
        }
        viewport.set_scale_factor(window.scale_factor() as f32);
        let size = window.inner_size();
        viewport.set_size(size.width, size.height);
        if let Some(color) = self.config.clear_color {
            viewport.set_clear_color(Box::new(color));
        }
        pollster::block_on(viewport.attach(window.clone()));
        self.viewport = Some(viewport);
        // Kick the first frame. Winit does not emit RedrawRequested on
        // window creation, so without this the App never renders until
        // something else pokes the event loop.
        window.request_redraw();
    }

    fn handle_keyboard(&mut self, event: KeyEvent) {
        // Snapshot ingest time first — winit does not carry a hardware event
        // timestamp, so we record the earliest moment the runner observes the
        // event. Taken before any decode work so queued events keep ordering.
        let timestamp = rfgui::time::Instant::now();
        let pressed = matches!(event.state, ElementState::Pressed);
        let rf_key = physical_key_to_rf(&event.physical_key);
        let characters: Option<SmolStr> = event
            .text
            .as_ref()
            .filter(|t| !t.is_empty())
            .map(|t| SmolStr::new(t.as_str()));
        let modifiers = self
            .viewport
            .as_ref()
            .map(|v| v.modifiers())
            .unwrap_or_default();
        let platform_event = PlatformKeyEvent {
            key: rf_key,
            characters,
            modifiers,
            repeat: event.repeat,
            is_composing: self.ime_composing,
            pressed,
            timestamp,
        };
        let app_event = AppEvent::Key(platform_event.clone());
        if let Some(viewport) = self.viewport.as_mut() {
            viewport.dispatch_app_event(
                &app_event,
                PlatformServices {
                    clipboard: self.clipboard.as_mut(),
                    cursor: &mut self.cursor,
                    redraw: &self.redraw,
                },
            );
            let _ = viewport.dispatch_platform_key_event(&platform_event);
        }
        // Clipboard shortcuts: Cmd/Ctrl+C/X/V on key-down. Fire the
        // semantic event in addition to the raw key event so apps can
        // react either way. Skip during IME composition to avoid
        // stepping on conversion gestures.
        if pressed && !self.ime_composing && modifiers.command() {
            use rfgui::platform::input::Key;
            match rf_key {
                Key::KeyC => {
                    if let Some(viewport) = self.viewport.as_mut() {
                        let _ = viewport.dispatch_copy_event();
                    }
                }
                Key::KeyX => {
                    if let Some(viewport) = self.viewport.as_mut() {
                        let _ = viewport.dispatch_cut_event();
                    }
                }
                Key::KeyV => {
                    if let Some(text) = self.clipboard.get() {
                        if !text.is_empty() {
                            if let Some(viewport) = self.viewport.as_mut() {
                                let _ = viewport.dispatch_paste_event(text);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        if pressed && !self.ime_composing {
            if let Some(text) = event.text.as_ref() {
                if !text.is_empty()
                    && self
                        .viewport
                        .as_ref()
                        .map(|v| should_dispatch_text(v, text))
                        .unwrap_or(true)
                {
                    let ti = PlatformTextInput {
                        text: text.to_string(),
                        input_type: rfgui::platform::PlatformInputType::Typing,
                        is_composing: false,
                    };
                    let ti_event = AppEvent::TextInput(ti.clone());
                    if let Some(viewport) = self.viewport.as_mut() {
                        viewport.dispatch_app_event(
                            &ti_event,
                            PlatformServices {
                                clipboard: self.clipboard.as_mut(),
                                cursor: &mut self.cursor,
                                redraw: &self.redraw,
                            },
                        );
                        let _ = viewport.dispatch_platform_text_input(&ti);
                    }
                }
            }
        }
    }

    fn handle_ime(&mut self, ime: Ime) {
        match ime {
            Ime::Enabled => {
                self.ime_composing = false;
                if let Some(viewport) = self.viewport.as_mut() {
                    let _ = viewport.dispatch_ime_enabled_event();
                }
            }
            Ime::Disabled => {
                self.ime_composing = false;
                let preedit = PlatformImePreedit {
                    text: String::new(),
                    cursor_start: None,
                    cursor_end: None,
                    selection_start: None,
                    selection_end: None,
                    attributes: Vec::new(),
                };
                if let Some(viewport) = self.viewport.as_mut() {
                    let _ = viewport.dispatch_platform_ime_preedit(&preedit);
                    let _ = viewport.dispatch_ime_disabled_event();
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
                    selection_start: None,
                    selection_end: None,
                    attributes: Vec::new(),
                };
                let app_event = AppEvent::ImePreedit(preedit.clone());
                if let Some(viewport) = self.viewport.as_mut() {
                    viewport.dispatch_app_event(
                        &app_event,
                        PlatformServices {
                            clipboard: self.clipboard.as_mut(),
                            cursor: &mut self.cursor,
                            redraw: &self.redraw,
                        },
                    );
                    let _ = viewport.dispatch_platform_ime_preedit(&preedit);
                }
            }
            Ime::Commit(text) => {
                self.ime_composing = false;
                if text.is_empty() {
                    return;
                }
                let ti = PlatformTextInput {
                    text: text.clone(),
                    input_type: rfgui::platform::PlatformInputType::ImeCommit,
                    is_composing: false,
                };
                let app_event = AppEvent::TextInput(ti.clone());
                if let Some(viewport) = self.viewport.as_mut() {
                    viewport.dispatch_app_event(
                        &app_event,
                        PlatformServices {
                            clipboard: self.clipboard.as_mut(),
                            cursor: &mut self.cursor,
                            redraw: &self.redraw,
                        },
                    );
                    // Fire the dedicated lifecycle event first so observers see
                    // the commit before the text-input insertion path runs.
                    let _ = viewport.dispatch_ime_commit_event(text);
                    let _ = viewport.dispatch_platform_text_input(&ti);
                }
            }
        }
    }

    /// Normalize a winit wheel delta into logical-pixel (dx, dy) using
    /// `config.wheel`. Returns `None` when the result falls inside the
    /// trackpad deadzone on both axes, so the caller can skip emitting.
    ///
    /// - `LineDelta` (mouse wheel ticks) → multiply by `mouse_line_step`
    ///   on both axes. No deadzone — line ticks are always intentional.
    /// - `PixelDelta` (trackpad scroll) → convert physical to logical via
    ///   `Viewport::physical_to_logical_point`, then scale and deadzone.
    fn normalize_wheel(&mut self, delta: MouseScrollDelta) -> Option<(f32, f32)> {
        let cfg: WheelConfig = self.config.wheel;
        let (dx, dy) = match delta {
            MouseScrollDelta::LineDelta(x, y) => (x * cfg.mouse_line_step, y * cfg.mouse_line_step),
            MouseScrollDelta::PixelDelta(pos) => {
                // Trackpad deltas come in physical pixels; fold the viewport
                // scale factor in so downstream logic sees logical pixels.
                let viewport = self.viewport.as_ref()?;
                let (lx, ly) = viewport.physical_to_logical_point(pos.x as f32, pos.y as f32);
                let lx = lx * cfg.touchpad_pixel_scale;
                let ly = ly * cfg.touchpad_pixel_scale;
                let lx = if lx.abs() < cfg.touchpad_deadzone {
                    0.0
                } else {
                    lx
                };
                let ly = if ly.abs() < cfg.touchpad_deadzone {
                    0.0
                } else {
                    ly
                };
                (lx, ly)
            }
        };
        if dx.abs() <= f32::EPSILON && dy.abs() <= f32::EPSILON {
            return None;
        }
        Some((dx, dy))
    }

    fn ensure_ready(&mut self) {
        if let Some(viewport) = self.viewport.as_mut() {
            viewport.app_on_ready(PlatformServices {
                clipboard: self.clipboard.as_mut(),
                cursor: &mut self.cursor,
                redraw: &self.redraw,
            });
        }
    }

    /// Run one build+render cycle. Called on `RedrawRequested` and also
    /// once synchronously at the end of `resumed` so the very first frame
    /// paints without waiting for a user event — winit does not queue a
    /// `RedrawRequested` at window creation on every platform.
    fn render_once(&mut self) {
        self.ensure_ready();
        if let Some(viewport) = self.viewport.as_mut() {
            let result = viewport.render_frame(PlatformServices {
                clipboard: self.clipboard.as_mut(),
                cursor: &mut self.cursor,
                redraw: &self.redraw,
            });
            if matches!(result, RenderFrameResult::NeedsRetry) {
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
        }
        self.sync_ime_cursor_area();
        self.drain_and_apply();
    }

    /// Push the focused element's IME cursor rect to winit so the system
    /// IME candidate window docks next to the caret. Deduped against the
    /// last rect the runner sent so we only call into winit on real
    /// changes.
    fn sync_ime_cursor_area(&mut self) {
        let Some(viewport) = self.viewport.as_ref() else {
            return;
        };
        let Some(rect) = viewport.focused_ime_cursor_rect() else {
            return;
        };
        let (x, y, w, h) = viewport.logical_to_physical_rect(rect.0, rect.1, rect.2, rect.3);
        let next = (x, y, w, h);
        if self.last_ime_rect == Some(next) {
            return;
        }
        self.last_ime_rect = Some(next);
        if let Some(window) = &self.window {
            window.set_ime_cursor_area(PhysicalPosition::new(x, y), PhysicalSize::new(w, h));
        }
    }

    fn drain_and_apply(&mut self) {
        let Some(viewport) = self.viewport.as_mut() else {
            return;
        };
        let requests = viewport.drain_platform_requests();
        let want_redraw = requests.request_redraw || *self.redraw_flag.lock().unwrap();
        if let Some(window) = &self.window {
            if let Some(cursor) = requests.cursor {
                window.set_cursor(winit_cursor_from(cursor));
            }
            if want_redraw {
                *self.redraw_flag.lock().unwrap() = false;
                window.request_redraw();
            }
            for cmd in &requests.window_commands {
                apply_window_command(window, cmd);
            }
            for cmd in &requests.ime_commands {
                apply_ime_command(window, cmd);
            }
        }
        if let Some(text) = requests.clipboard_write {
            self.clipboard.set(&text);
        }
        if requests.request_paste {
            if let Some(text) = self.clipboard.get() {
                if !text.is_empty() {
                    if let Some(viewport) = self.viewport.as_mut() {
                        let _ = viewport.dispatch_paste_event(text);
                    }
                }
            }
        }
        // `pending_drags` still unhandled — Sprint 8b wires drag state
        // machine & OS drag bridge.
        let _ = requests.pending_drags;
    }
}

fn apply_window_command(window: &Window, cmd: &rfgui::platform::WindowCommand) {
    use rfgui::platform::WindowCommand;
    match cmd {
        WindowCommand::Close => {
            // winit 0.30 has no explicit close method on `Window`; the
            // runner needs an `ActiveEventLoop` to call `exit()`. Caller
            // should send a close via an app channel instead. We swallow
            // for now so handlers can at least express intent.
        }
        WindowCommand::Minimize => {
            window.set_minimized(true);
        }
        WindowCommand::Maximize => {
            window.set_maximized(true);
        }
        WindowCommand::Restore => {
            window.set_minimized(false);
            window.set_maximized(false);
        }
        WindowCommand::SetFullscreen(enable) => {
            use winit::window::Fullscreen;
            window.set_fullscreen(if *enable {
                Some(Fullscreen::Borderless(None))
            } else {
                None
            });
        }
        WindowCommand::SetTitle(title) => {
            window.set_title(title);
        }
    }
}

fn apply_ime_command(window: &Window, cmd: &rfgui::platform::ImeCommand) {
    use rfgui::platform::ImeCommand;
    match cmd {
        ImeCommand::Enable => {
            window.set_ime_allowed(true);
        }
        ImeCommand::Disable => {
            window.set_ime_allowed(false);
        }
        ImeCommand::SetCursorRect(x, y, w, h) => {
            use winit::dpi::LogicalPosition;
            use winit::dpi::LogicalSize;
            window.set_ime_cursor_area(
                LogicalPosition::new(*x as f64, *y as f64),
                LogicalSize::new(*w as f64, *h as f64),
            );
        }
    }
}

impl ApplicationHandler for Runner {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title(&self.config.title)
            .with_transparent(self.config.transparent)
            .with_inner_size(LogicalSize::new(
                self.config.initial_size.0 as f64,
                self.config.initial_size.1 as f64,
            ));
        let window = Arc::new(
            event_loop
                .create_window(attrs)
                .expect("failed to create winit window"),
        );
        window.set_ime_allowed(true);
        apply_macos_shadow(&window, !self.config.transparent);
        self.window = Some(window);
        self.ensure_viewport();
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        self.ensure_ready();
        match event {
            WindowEvent::CloseRequested => {
                if let Some(viewport) = self.viewport.as_mut() {
                    let close = AppEvent::CloseRequested;
                    viewport.dispatch_app_event(
                        &close,
                        PlatformServices {
                            clipboard: self.clipboard.as_mut(),
                            cursor: &mut self.cursor,
                            redraw: &self.redraw,
                        },
                    );
                    viewport.app_on_shutdown(PlatformServices {
                        clipboard: self.clipboard.as_mut(),
                        cursor: &mut self.cursor,
                        redraw: &self.redraw,
                    });
                }
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                if let Some(viewport) = self.viewport.as_mut() {
                    viewport.set_size(size.width, size.height);
                    let scale = viewport.scale_factor();
                    let ev = AppEvent::Resized {
                        width: size.width,
                        height: size.height,
                        scale,
                    };
                    viewport.dispatch_app_event(
                        &ev,
                        PlatformServices {
                            clipboard: self.clipboard.as_mut(),
                            cursor: &mut self.cursor,
                            redraw: &self.redraw,
                        },
                    );
                }
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                if let Some(viewport) = self.viewport.as_mut() {
                    viewport.set_scale_factor(scale_factor as f32);
                    let ev = AppEvent::ScaleFactorChanged {
                        scale: scale_factor as f32,
                        suggested_size: None,
                    };
                    viewport.dispatch_app_event(
                        &ev,
                        PlatformServices {
                            clipboard: self.clipboard.as_mut(),
                            cursor: &mut self.cursor,
                            redraw: &self.redraw,
                        },
                    );
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.last_mouse = Some(position);
                self.cursor_in_window = true;
                let (logical_x, logical_y) = self
                    .viewport
                    .as_ref()
                    .map(|v| v.physical_to_logical_point(position.x as f32, position.y as f32))
                    .unwrap_or((position.x as f32, position.y as f32));
                self.last_mouse_logical = Some((logical_x, logical_y));
                let move_event = PlatformPointerEvent {
                    kind: PlatformPointerEventKind::Move {
                        x: logical_x,
                        y: logical_y,
                    },
                    pointer_id: 0,
                    pointer_type: PointerType::Mouse,
                    pressure: 0.0,
                };
                let ev = AppEvent::Pointer(move_event);
                if let Some(viewport) = self.viewport.as_mut() {
                    viewport.dispatch_app_event(
                        &ev,
                        PlatformServices {
                            clipboard: self.clipboard.as_mut(),
                            cursor: &mut self.cursor,
                            redraw: &self.redraw,
                        },
                    );
                    let _ = viewport.dispatch_platform_pointer_event(&move_event);
                }
            }
            WindowEvent::CursorLeft { .. } => {
                self.cursor_in_window = false;
                if let Some(viewport) = self.viewport.as_mut() {
                    viewport.clear_pointer_position_viewport();
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let Some(mapped) = winit_button_to_platform(button) else {
                    return;
                };
                let pressed = matches!(state, ElementState::Pressed);
                if let Some(viewport) = self.viewport.as_mut() {
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
                    viewport.dispatch_app_event(
                        &ev,
                        PlatformServices {
                            clipboard: self.clipboard.as_mut(),
                            cursor: &mut self.cursor,
                            redraw: &self.redraw,
                        },
                    );
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
                        viewport.dispatch_app_event(
                            &click_ev,
                            PlatformServices {
                                clipboard: self.clipboard.as_mut(),
                                cursor: &mut self.cursor,
                                redraw: &self.redraw,
                            },
                        );
                        let _ = viewport.dispatch_platform_pointer_event(&click);
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let Some((dx, dy)) = self.normalize_wheel(delta) else {
                    return;
                };
                let position = self.last_mouse_logical.unwrap_or((0.0, 0.0));
                let wheel = PlatformWheelEvent {
                    delta_x: -dx,
                    delta_y: -dy,
                    position,
                    modifiers: rfgui::platform::Modifiers::empty(),
                    delta_mode: rfgui::platform::WheelDeltaMode::Pixel,
                    phase: rfgui::platform::WheelPhase::Changed,
                    timestamp: rfgui::time::Instant::now(),
                };
                let ev = AppEvent::Wheel(wheel);
                if let Some(viewport) = self.viewport.as_mut() {
                    viewport.dispatch_app_event(
                        &ev,
                        PlatformServices {
                            clipboard: self.clipboard.as_mut(),
                            cursor: &mut self.cursor,
                            redraw: &self.redraw,
                        },
                    );
                    let _ = viewport.dispatch_platform_wheel_event(&wheel);
                }
            }
            WindowEvent::Focused(focused) => {
                let ev = AppEvent::HostFocus(focused);
                if let Some(viewport) = self.viewport.as_mut() {
                    viewport.dispatch_app_event(
                        &ev,
                        PlatformServices {
                            clipboard: self.clipboard.as_mut(),
                            cursor: &mut self.cursor,
                            redraw: &self.redraw,
                        },
                    );
                }
                if !focused {
                    self.ime_composing = false;
                    if let Some(viewport) = self.viewport.as_mut() {
                        viewport.clear_input_state();
                    }
                }
            }
            WindowEvent::ModifiersChanged(mods) => {
                if let Some(viewport) = self.viewport.as_mut() {
                    viewport.set_modifiers(winit_modifiers_to_rf(mods.state()));
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
            WindowEvent::Moved(pos) => {
                let ev = AppEvent::Moved { x: pos.x, y: pos.y };
                if let Some(viewport) = self.viewport.as_mut() {
                    viewport.dispatch_app_event(
                        &ev,
                        PlatformServices {
                            clipboard: self.clipboard.as_mut(),
                            cursor: &mut self.cursor,
                            redraw: &self.redraw,
                        },
                    );
                }
            }
            WindowEvent::Occluded(occluded) => {
                let ev = AppEvent::Occluded(occluded);
                if let Some(viewport) = self.viewport.as_mut() {
                    viewport.dispatch_app_event(
                        &ev,
                        PlatformServices {
                            clipboard: self.clipboard.as_mut(),
                            cursor: &mut self.cursor,
                            redraw: &self.redraw,
                        },
                    );
                }
            }
            WindowEvent::ThemeChanged(theme) => {
                let mapped = match theme {
                    winit::window::Theme::Light => rfgui::app::WindowTheme::Light,
                    winit::window::Theme::Dark => rfgui::app::WindowTheme::Dark,
                };
                let ev = AppEvent::ThemeChanged(mapped);
                if let Some(viewport) = self.viewport.as_mut() {
                    viewport.dispatch_app_event(
                        &ev,
                        PlatformServices {
                            clipboard: self.clipboard.as_mut(),
                            cursor: &mut self.cursor,
                            redraw: &self.redraw,
                        },
                    );
                }
            }
            WindowEvent::HoveredFile(path) => {
                // winit delivers one path per event; coalesce into a single
                // `FilesHovered` call so downstream handlers see a batch.
                let ev = AppEvent::FilesHovered(vec![path]);
                if let Some(viewport) = self.viewport.as_mut() {
                    viewport.dispatch_app_event(
                        &ev,
                        PlatformServices {
                            clipboard: self.clipboard.as_mut(),
                            cursor: &mut self.cursor,
                            redraw: &self.redraw,
                        },
                    );
                }
            }
            WindowEvent::HoveredFileCancelled => {
                let ev = AppEvent::FilesHoverCancelled;
                if let Some(viewport) = self.viewport.as_mut() {
                    viewport.dispatch_app_event(
                        &ev,
                        PlatformServices {
                            clipboard: self.clipboard.as_mut(),
                            cursor: &mut self.cursor,
                            redraw: &self.redraw,
                        },
                    );
                }
            }
            WindowEvent::DroppedFile(path) => {
                let ev = AppEvent::FilesDropped(vec![path]);
                if let Some(viewport) = self.viewport.as_mut() {
                    viewport.dispatch_app_event(
                        &ev,
                        PlatformServices {
                            clipboard: self.clipboard.as_mut(),
                            cursor: &mut self.cursor,
                            redraw: &self.redraw,
                        },
                    );
                }
            }
            _ => {}
        }
        self.drain_and_apply();
    }

    fn device_event(&mut self, _event_loop: &ActiveEventLoop, _id: DeviceId, event: DeviceEvent) {
        // Device events are the only drag channel that keeps firing after
        // the cursor leaves the window. We only consume them during an
        // active in-progress drag — indicated by an existing viewport
        // mouse move listener — so we do not double-dispatch otherwise.
        if self.cursor_in_window {
            return;
        }
        let has_listener = self
            .viewport
            .as_ref()
            .map(|v| v.has_viewport_pointer_listeners())
            .unwrap_or(false);
        if !has_listener {
            return;
        }
        match event {
            DeviceEvent::MouseMotion { delta } => {
                let Some((last_x, last_y)) = self.last_mouse_logical else {
                    return;
                };
                let Some(viewport) = self.viewport.as_mut() else {
                    return;
                };
                let (dx, dy) = viewport.physical_to_logical_point(delta.0 as f32, delta.1 as f32);
                let next = (last_x + dx, last_y + dy);
                viewport.set_pointer_position_viewport(next.0, next.1);
                let _ = viewport.dispatch_platform_pointer_event(&PlatformPointerEvent {
                    kind: PlatformPointerEventKind::Move {
                        x: next.0,
                        y: next.1,
                    },
                    pointer_id: 0,
                    pointer_type: PointerType::Mouse,
                    pressure: 0.0,
                });
                self.last_mouse_logical = Some(next);
            }
            DeviceEvent::Button { button, state } => {
                if !matches!(state, ElementState::Released) {
                    return;
                }
                let Some(mapped) = device_button_to_platform(button) else {
                    return;
                };
                if let Some(viewport) = self.viewport.as_mut() {
                    if let Some((x, y)) = self.last_mouse_logical {
                        viewport.set_pointer_position_viewport(x, y);
                    }
                    viewport.set_pointer_button_pressed(platform_button_to_viewport(mapped), false);
                    let _ = viewport.dispatch_platform_pointer_event(&PlatformPointerEvent {
                        kind: PlatformPointerEventKind::Up(mapped),
                        pointer_id: 0,
                        pointer_type: PointerType::Mouse,
                        pressure: 0.0,
                    });
                    let _ = viewport.dispatch_platform_pointer_event(&PlatformPointerEvent {
                        kind: PlatformPointerEventKind::Click(mapped),
                        pointer_id: 0,
                        pointer_type: PointerType::Mouse,
                        pressure: 0.0,
                    });
                }
            }
            _ => {}
        }
        self.drain_and_apply();
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // Drive component timers (use_timeout, use_interval). Viewport
        // transition/animation plugins tick inside render_rsx and report
        // their state via `viewport.is_animating()` below, so they don't
        // go through this path.
        let now = Instant::now();
        run_due_timers(now);
        if *self.redraw_flag.lock().unwrap() {
            *self.redraw_flag.lock().unwrap() = false;
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
        // Schedule the next wake-up:
        // - viewport reports active transitions → Poll so the loop
        //   iterates and the freshly queued RedrawRequested fires
        // - timer pending → WaitUntil(deadline)
        // - otherwise idle until the next user event
        let animating = self
            .viewport
            .as_ref()
            .map(|v| v.is_animating())
            .unwrap_or(false);
        if animating {
            event_loop.set_control_flow(ControlFlow::Poll);
        } else {
            match next_timer_deadline() {
                Some(deadline) => event_loop.set_control_flow(ControlFlow::WaitUntil(deadline)),
                None => event_loop.set_control_flow(ControlFlow::Wait),
            }
        }
    }
}

/// Toggle the macOS native window drop-shadow.
///
/// Transparent rfgui windows otherwise show a ghost shadow around the
/// fully-transparent regions; disabling the shadow keeps transparent
/// surfaces visually clean. No-op on non-macOS targets.
#[cfg(target_os = "macos")]
fn apply_macos_shadow(window: &Window, has_shadow: bool) {
    use winit::platform::macos::WindowExtMacOS;
    window.set_has_shadow(has_shadow);
}

#[cfg(not(target_os = "macos"))]
fn apply_macos_shadow(_window: &Window, _has_shadow: bool) {}

/// Return true when a `KeyEvent::text` payload should also be sent down
/// the text-input path. Drops named-key sentinel chars, control
/// characters, and any character typed with a modifier combo so shortcut
/// chords don't insert their literal key as text.
fn should_dispatch_text(viewport: &Viewport, text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    let mut chars = text.chars();
    let Some(ch) = chars.next() else {
        return false;
    };
    if chars.next().is_some() {
        return false;
    }
    // Winit named-key sentinels live in the Unicode PUA range; skip them.
    if matches!(ch as u32, 0xF700..=0xF8FF) {
        return false;
    }
    if ch.is_control() {
        return false;
    }
    // Shift is a text-producing modifier (uppercase, symbols); only
    // Ctrl/Alt/Meta combos should suppress typing.
    let m = viewport.modifiers();
    !(m.ctrl() || m.alt() || m.meta())
}

/// Map winit's `DeviceEvent::Button` numeric code onto the platform enum
/// used by the rest of the pipeline. Only the standard five-button
/// mouse layout is covered — extra side buttons are ignored.
fn device_button_to_platform(button: u32) -> Option<PlatformPointerButton> {
    Some(match button {
        1 => PlatformPointerButton::Left,
        2 => PlatformPointerButton::Right,
        3 => PlatformPointerButton::Middle,
        4 => PlatformPointerButton::Back,
        5 => PlatformPointerButton::Forward,
        _ => return None,
    })
}

/// Bridge from the engine's `PlatformPointerButton` to the viewport's
/// internal `PointerButton`. Kept separate so the platform enum does not
/// leak implementation details of `view::viewport::PointerButton`.
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

fn winit_cursor_from(cursor: rfgui::Cursor) -> winit::window::Cursor {
    use rfgui::Cursor as C;
    use winit::window::CursorIcon;
    let icon = match cursor {
        C::Default => CursorIcon::Default,
        C::ContextMenu => CursorIcon::ContextMenu,
        C::Help => CursorIcon::Help,
        C::Pointer => CursorIcon::Pointer,
        C::Progress => CursorIcon::Progress,
        C::Wait => CursorIcon::Wait,
        C::Cell => CursorIcon::Cell,
        C::Crosshair => CursorIcon::Crosshair,
        C::Text => CursorIcon::Text,
        C::VerticalText => CursorIcon::VerticalText,
        C::Alias => CursorIcon::Alias,
        C::Copy => CursorIcon::Copy,
        C::Move => CursorIcon::Move,
        C::NoDrop => CursorIcon::NoDrop,
        C::NotAllowed => CursorIcon::NotAllowed,
        C::Grab => CursorIcon::Grab,
        C::Grabbing => CursorIcon::Grabbing,
        C::EResize => CursorIcon::EResize,
        C::NResize => CursorIcon::NResize,
        C::NeResize => CursorIcon::NeResize,
        C::NwResize => CursorIcon::NwResize,
        C::SResize => CursorIcon::SResize,
        C::SeResize => CursorIcon::SeResize,
        C::SwResize => CursorIcon::SwResize,
        C::WResize => CursorIcon::WResize,
        C::EwResize => CursorIcon::EwResize,
        C::NsResize => CursorIcon::NsResize,
        C::NeswResize => CursorIcon::NeswResize,
        C::NwseResize => CursorIcon::NwseResize,
        C::ColResize => CursorIcon::ColResize,
        C::RowResize => CursorIcon::RowResize,
        C::AllScroll => CursorIcon::AllScroll,
        C::ZoomIn => CursorIcon::ZoomIn,
        C::ZoomOut => CursorIcon::ZoomOut,
        C::DndAsk => CursorIcon::Alias,
        C::AllResize => CursorIcon::Move,
    };
    winit::window::Cursor::Icon(icon)
}
