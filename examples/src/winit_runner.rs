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

use rfgui::app::{App, AppConfig, AppContext, AppEvent, WheelConfig};
use rfgui::platform::desktop_backend::ArboardClipboard;
use rfgui::platform::{
    CallbackCursorSink, CallbackRedrawRequester, Clipboard, NullClipboard, PlatformImePreedit,
    PlatformKeyEvent, PlatformMouseButton, PlatformMouseEvent, PlatformMouseEventKind,
    PlatformServices, PlatformTextInput, PlatformWheelEvent,
};
use rfgui::ui::{RsxNode, next_timer_deadline, peek_state_dirty, run_due_timers};
use rfgui::view::viewport::{Viewport, ViewportControl};
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
use winit::keyboard::{Key, NamedKey, PhysicalKey};
use winit::window::{Window, WindowId};

/// Run an `App` until the user closes the window.
///
/// Blocks the calling thread. Returns on `WindowEvent::CloseRequested`
/// after calling `App::on_shutdown`.
pub fn run<A: App + 'static>(app: A, config: AppConfig) {
    let event_loop = EventLoop::new().expect("failed to create winit event loop");
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut handler = Runner::new(app, config);
    event_loop
        .run_app(&mut handler)
        .expect("winit event loop exited with error");
}

struct Runner<A: App> {
    app: A,
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
    /// Memoized result of `App::build` from the last rebuild. Reused on
    /// pure-redraw frames (animation tick, hover update, …) so the
    /// viewport receives the *same* `RsxNode` reference and skips its
    /// expensive structural diff. Without this, every animation frame
    /// rebuilt the entire scene and ran orders of magnitude slower than
    /// the old hand-rolled app loop.
    cached_rsx: Option<RsxNode>,
    /// True until the next `App::build` runs. Set on construction (so
    /// the first frame builds) and whenever a state change triggers a
    /// rebuild-class dirty flag.
    needs_rebuild: bool,
    ready_dispatched: bool,
}

impl<A: App> Runner<A> {
    fn new(app: A, config: AppConfig) -> Self {
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
            app,
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
            cached_rsx: None,
            needs_rebuild: true,
            ready_dispatched: false,
        }
    }

    fn ensure_viewport(&mut self) {
        if self.viewport.is_some() {
            return;
        }
        let Some(window) = &self.window else { return };
        let mut viewport = Viewport::new();
        viewport.set_scale_factor(window.scale_factor() as f32);
        let size = window.inner_size();
        viewport.set_size(size.width, size.height);
        if let Some(color) = self.config.clear_color {
            viewport.set_clear_color(Box::new(color));
        }
        pollster::block_on(viewport.attach(window.clone()));
        pollster::block_on(viewport.create_surface());
        self.viewport = Some(viewport);
        // Kick the first frame. Winit does not emit RedrawRequested on
        // window creation, so without this the App never renders until
        // something else pokes the event loop.
        window.request_redraw();
    }

    fn with_ctx<R>(&mut self, f: impl FnOnce(&mut A, &mut AppContext<'_>) -> R) -> Option<R> {
        let viewport = self.viewport.as_mut()?;
        // Actual cursor application happens in drain_and_apply via the
        // winit Window; the sink on self is a stub because the viewport
        // records pending requests and the runner applies them.
        let mut ctx = AppContext {
            viewport: ViewportControl::new(viewport),
            services: PlatformServices {
                clipboard: self.clipboard.as_mut(),
                cursor: &mut self.cursor,
                redraw: &self.redraw,
            },
        };
        Some(f(&mut self.app, &mut ctx))
    }

    fn handle_keyboard(&mut self, event: KeyEvent) {
        let key_str = key_to_string(&event.logical_key);
        let code_str = physical_key_to_string(&event.physical_key);
        let pressed = matches!(event.state, ElementState::Pressed);
        // Update the viewport's pressed-key set so `is_key_pressed`
        // lookups (used by shortcut handlers and the text-input filter
        // below) stay in sync with the host keyboard state.
        if let Some(viewport) = self.viewport.as_mut() {
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
        if let Some(viewport) = self.viewport.as_mut() {
            let _ = viewport.dispatch_platform_key_event(&platform_event);
        }
        // Winit delivers committed text on key-press events as `event.text`
        // when no IME is in use. Suppress during IME composition to avoid
        // double-inserting the character, and filter out shortcut chords
        // so `Ctrl+C` doesn't insert the literal "c".
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
                    };
                    let ti_event = AppEvent::TextInput(ti.clone());
                    self.with_ctx(|app, ctx| app.on_event(&ti_event, ctx));
                    if let Some(viewport) = self.viewport.as_mut() {
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
            }
            Ime::Disabled => {
                self.ime_composing = false;
                // Clear any stale preedit state on the viewport so the
                // next character doesn't start with leftover composition.
                let preedit = PlatformImePreedit {
                    text: String::new(),
                    cursor_start: None,
                    cursor_end: None,
                };
                if let Some(viewport) = self.viewport.as_mut() {
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
                if let Some(viewport) = self.viewport.as_mut() {
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
                if let Some(viewport) = self.viewport.as_mut() {
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

    fn ensure_ready(&mut self) {
        if self.ready_dispatched {
            return;
        }
        self.with_ctx(|app, ctx| app.on_ready(ctx));
        self.ready_dispatched = true;
    }

    /// Run one build+render cycle. Called on `RedrawRequested` and also
    /// once synchronously at the end of `resumed` so the very first frame
    /// paints without waiting for a user event — winit does not queue a
    /// `RedrawRequested` at window creation on every platform.
    fn render_once(&mut self) {
        self.ensure_ready();
        // Promote any dirty-rebuild signal from the last event batch into
        // a real rebuild request so the cached RSX gets refreshed.
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
        if let (Some(rsx), Some(viewport)) =
            (self.cached_rsx.as_ref(), self.viewport.as_mut())
        {
            let _ = viewport.render_rsx(rsx);
        }
        self.sync_ime_cursor_area();
        // The first frame after window creation can hit
        // `SurfaceTexture::Occluded` on macOS while the NSWindow is still
        // becoming visible. `begin_frame` then silently returns and no
        // geometry is laid out, so the screen stays blank until something
        // else (a manual resize, an animation tick) pokes the loop. Detect
        // that case and queue another redraw so the next tick retries.
        let needs_retry = self
            .viewport
            .as_ref()
            .zip(self.cached_rsx.as_ref())
            .map(|(v, _)| v.frame_box_models().is_empty())
            .unwrap_or(false);
        if needs_retry {
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
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
            window.set_ime_cursor_area(
                PhysicalPosition::new(x, y),
                PhysicalSize::new(w, h),
            );
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
        }
        if let Some(text) = requests.clipboard_write {
            self.clipboard.set(&text);
        }
    }
}

impl<A: App + 'static> ApplicationHandler for Runner<A> {
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

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _id: WindowId,
        event: WindowEvent,
    ) {
        self.ensure_ready();
        match event {
            WindowEvent::CloseRequested => {
                let close = AppEvent::CloseRequested;
                self.with_ctx(|app, ctx| app.on_event(&close, ctx));
                self.with_ctx(|app, ctx| app.on_shutdown(ctx));
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                if let Some(viewport) = self.viewport.as_mut() {
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
                if let Some(viewport) = self.viewport.as_mut() {
                    viewport.set_scale_factor(scale_factor as f32);
                }
                let ev = AppEvent::ScaleFactorChanged {
                    scale: scale_factor as f32,
                };
                self.with_ctx(|app, ctx| app.on_event(&ev, ctx));
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.last_mouse = Some(position);
                self.cursor_in_window = true;
                let (logical_x, logical_y) = self
                    .viewport
                    .as_ref()
                    .map(|v| {
                        v.physical_to_logical_point(position.x as f32, position.y as f32)
                    })
                    .unwrap_or((position.x as f32, position.y as f32));
                self.last_mouse_logical = Some((logical_x, logical_y));
                let move_event = PlatformMouseEvent {
                    kind: PlatformMouseEventKind::Move {
                        x: logical_x,
                        y: logical_y,
                    },
                };
                let ev = AppEvent::Mouse(move_event);
                self.with_ctx(|app, ctx| app.on_event(&ev, ctx));
                if let Some(viewport) = self.viewport.as_mut() {
                    let _ = viewport.dispatch_platform_mouse_event(&move_event);
                }
            }
            WindowEvent::CursorLeft { .. } => {
                self.cursor_in_window = false;
                if let Some(viewport) = self.viewport.as_mut() {
                    viewport.clear_mouse_position_viewport();
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let Some(mapped) = winit_button_to_platform(button) else {
                    return;
                };
                let pressed = matches!(state, ElementState::Pressed);
                if let Some(viewport) = self.viewport.as_mut() {
                    viewport.set_mouse_button_pressed(
                        platform_button_to_viewport(mapped),
                        pressed,
                    );
                }
                let kind = if pressed {
                    PlatformMouseEventKind::Down(mapped)
                } else {
                    PlatformMouseEventKind::Up(mapped)
                };
                let ev = AppEvent::Mouse(PlatformMouseEvent { kind });
                self.with_ctx(|app, ctx| app.on_event(&ev, ctx));
                if let Some(viewport) = self.viewport.as_mut() {
                    let _ = viewport.dispatch_platform_mouse_event(&PlatformMouseEvent { kind });
                    if !pressed {
                        let click = PlatformMouseEvent {
                            kind: PlatformMouseEventKind::Click(mapped),
                        };
                        let click_ev = AppEvent::Mouse(click);
                        self.with_ctx(|app, ctx| app.on_event(&click_ev, ctx));
                        if let Some(viewport) = self.viewport.as_mut() {
                            let _ = viewport.dispatch_platform_mouse_event(&click);
                        }
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let Some((dx, dy)) = self.normalize_wheel(delta) else {
                    return;
                };
                // Old runner dispatched negated deltas so a forward wheel
                // scrolls content *up* — match that so scroll direction
                // feels right on both mouse wheels and trackpads.
                let ev = AppEvent::Wheel(PlatformWheelEvent {
                    delta_x: -dx,
                    delta_y: -dy,
                });
                self.with_ctx(|app, ctx| app.on_event(&ev, ctx));
                if let Some(viewport) = self.viewport.as_mut() {
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
                    // Purge any stuck hover / pressed-key / pressed-button
                    // state left over from the outgoing focus session.
                    self.ime_composing = false;
                    if let Some(viewport) = self.viewport.as_mut() {
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

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _id: DeviceId,
        event: DeviceEvent,
    ) {
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
            .map(|v| v.has_viewport_mouse_listeners())
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
                let (dx, dy) =
                    viewport.physical_to_logical_point(delta.0 as f32, delta.1 as f32);
                let next = (last_x + dx, last_y + dy);
                viewport.set_mouse_position_viewport(next.0, next.1);
                let _ = viewport.dispatch_platform_mouse_event(&PlatformMouseEvent {
                    kind: PlatformMouseEventKind::Move {
                        x: next.0,
                        y: next.1,
                    },
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
                        viewport.set_mouse_position_viewport(x, y);
                    }
                    viewport.set_mouse_button_pressed(
                        platform_button_to_viewport(mapped),
                        false,
                    );
                    let _ = viewport.dispatch_platform_mouse_event(&PlatformMouseEvent {
                        kind: PlatformMouseEventKind::Up(mapped),
                    });
                    let _ = viewport.dispatch_platform_mouse_event(&PlatformMouseEvent {
                        kind: PlatformMouseEventKind::Click(mapped),
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

/// Serialize a winit logical key into the canonical string used by
/// `PlatformKeyEvent::key`. Named keys map to their DOM-style names
/// ("Enter", "ArrowLeft", "Backspace", …); character keys pass through as
/// their text.
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
    let has_alt = viewport.is_key_pressed("Alt");
    let has_ctrl = viewport.is_key_pressed("Control");
    let has_meta = viewport.is_key_pressed("Meta") || viewport.is_key_pressed("Super");
    !(has_alt || has_ctrl || has_meta)
}

/// Map winit's `DeviceEvent::Button` numeric code onto the platform enum
/// used by the rest of the pipeline. Only the standard five-button
/// mouse layout is covered — extra side buttons are ignored.
fn device_button_to_platform(button: u32) -> Option<PlatformMouseButton> {
    Some(match button {
        1 => PlatformMouseButton::Left,
        2 => PlatformMouseButton::Right,
        3 => PlatformMouseButton::Middle,
        4 => PlatformMouseButton::Back,
        5 => PlatformMouseButton::Forward,
        _ => return None,
    })
}

/// Bridge from the engine's `PlatformMouseButton` to the viewport's
/// internal `MouseButton`. Kept separate so the platform enum does not
/// leak implementation details of `view::viewport::MouseButton`.
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

