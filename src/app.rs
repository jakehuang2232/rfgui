//! `App` trait and supporting types — the contract between user code and
//! any backend runner.
//!
//! Phase 5 of the viewport-decoupling work. The rfgui engine itself ships
//! no runner; actual event-loop glue (winit, web, custom) lives in the
//! host crate and calls back into this trait. The trait intentionally
//! knows nothing about `winit`, `web_sys`, or any specific platform.
//!
//! Flow per frame / event batch:
//!   1. Host drains its platform events, wraps each in an `AppEvent`, and
//!      calls `App::on_event(...)`.
//!   2. Host calls `App::build(...)` to get a fresh `RsxNode` tree.
//!   3. Host hands the tree to `Viewport::render(...)`.
//!   4. Host drains `Viewport::drain_platform_requests()` and applies the
//!      cursor / clipboard / redraw requests to the real host window.

use crate::Color;
use crate::platform::{
    PlatformImePreedit, PlatformKeyEvent, PlatformPointerEvent, PlatformServices,
    PlatformTextInput, PlatformWheelEvent,
};
use crate::ui::RsxNode;
use crate::view::viewport::ViewportControl;

/// Host window theme. Pushed via [`AppEvent::ThemeChanged`] when the OS
/// setting flips. Apps typically use this to re-pick a colour palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowTheme {
    Light,
    Dark,
}

/// Platform-neutral application event. Wraps the `Platform*Event` types
/// from `crate::platform::input`; hosts translate their own event shapes
/// (winit `WindowEvent`, DOM events, …) into one of these before calling
/// `App::on_event`.
#[derive(Debug, Clone, PartialEq)]
pub enum AppEvent {
    Pointer(PlatformPointerEvent),
    Wheel(PlatformWheelEvent),
    Key(PlatformKeyEvent),
    TextInput(PlatformTextInput),
    ImePreedit(PlatformImePreedit),
    /// Logical surface size changed, in physical pixels. The host is
    /// expected to also call `Viewport::set_size`; this event just lets
    /// the `App` react (e.g. layout hints).
    Resized {
        width: u32,
        height: u32,
        /// Scale factor in effect at the time of this event. Sent
        /// alongside the size so the `App` can compute logical px
        /// without racing a separate `ScaleFactorChanged`.
        scale: f32,
    },
    /// DPI / scale factor changed. Host also pushes it to the viewport.
    /// `suggested_size` is the new physical size the host recommends
    /// adopting (mirrors winit 0.29+ callback semantics). `None` when the
    /// backend cannot suggest a size.
    ScaleFactorChanged {
        scale: f32,
        suggested_size: Option<(u32, u32)>,
    },
    /// Host window moved (top-left corner, physical pixels).
    Moved {
        x: i32,
        y: i32,
    },
    /// Host window minimized.
    Minimized,
    /// Host window maximized.
    Maximized,
    /// Host window restored (unmaximized or de-minimized).
    Restored,
    /// Host surface occluded / unoccluded. When `true`, the app should
    /// skip rendering to save work; unchanged state still gets a single
    /// event.
    Occluded(bool),
    /// System / app theme changed (Light ↔ Dark).
    ThemeChanged(WindowTheme),
    /// File drag-and-drop hover started over the host surface. Paths
    /// preview the payload so the app can highlight the drop target.
    FilesHovered(Vec<std::path::PathBuf>),
    /// Drag-hover ended without a drop.
    FilesHoverCancelled,
    /// Files were dropped on the host surface.
    FilesDropped(Vec<std::path::PathBuf>),
    /// Host window / tab gained or lost focus. Distinct from per-element
    /// focus dispatched via `Viewport::dispatch_focus_event`.
    HostFocus(bool),
    /// Host is about to shut down; last chance for the `App` to persist
    /// state.
    CloseRequested,
}

/// View into the world that `App` methods are allowed to mutate.
///
/// Composes a `ViewportControl` (scoped access to the viewport) with
/// `PlatformServices` (clipboard, cursor sink, redraw requester) so the
/// `App` never needs a direct reference to either the concrete `Viewport`
/// or the host's platform backend types.
pub struct AppContext<'a> {
    pub viewport: ViewportControl<'a>,
    pub services: PlatformServices<'a>,
}

/// Configuration the runner uses to stand up its host window.
///
/// Kept small and data-only on purpose — new fields get added here rather
/// than becoming trait methods, so existing `App` implementations stay
/// source-compatible when a new option appears.
/// Wheel / trackpad input normalization parameters.
///
/// Host wheel events come in two shapes: discrete mouse "line" ticks and
/// continuous trackpad "pixel" deltas. Without normalization the former
/// feels too slow and the latter too fast and jittery around zero. These
/// knobs are plain data so the engine can own them without depending on
/// any host event type.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WheelConfig {
    /// Logical-pixel distance one mouse-wheel line tick scrolls.
    pub mouse_line_step: f32,
    /// Multiplier applied to trackpad pixel deltas after they've been
    /// converted to logical pixels via the viewport scale factor.
    pub touchpad_pixel_scale: f32,
    /// Absolute trackpad delta (in logical pixels, per axis) below which
    /// the event is dropped. Kills sub-pixel jitter at rest.
    pub touchpad_deadzone: f32,
}

impl Default for WheelConfig {
    fn default() -> Self {
        Self {
            mouse_line_step: 28.0,
            touchpad_pixel_scale: 1.0,
            touchpad_deadzone: 0.5,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub title: String,
    pub initial_size: (u32, u32),
    pub scale_factor: Option<f32>,
    /// Request a transparent host surface. Runners pass this to their
    /// window-attribute builder and, on platforms that need it (macOS),
    /// also drop the native drop-shadow so the transparent region stays
    /// visually clean. No-op on platforms without transparency support.
    pub transparent: bool,
    /// Initial viewport clear color. `None` means the runner leaves the
    /// viewport's built-in default (opaque black) untouched. Typical
    /// transparent apps set `Color::transparent()` here.
    pub clear_color: Option<Color>,
    /// Mouse wheel / trackpad normalization. Runners use this to convert
    /// raw host wheel events into logical-pixel deltas.
    pub wheel: WheelConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            title: String::from("rfgui"),
            initial_size: (1280, 800),
            scale_factor: None,
            transparent: false,
            clear_color: None,
            wheel: WheelConfig::default(),
        }
    }
}

/// Contract an application implements to run under a backend runner.
///
/// Implementors own their UI state and return a fresh `RsxNode` from
/// `build` on every render. `on_event` is called before `build`; event
/// handlers may mutate self and request a redraw through
/// `ctx.viewport.request_redraw()`.
///
/// Default method impls let simple apps only override `build`.
pub trait App {
    fn build(&mut self, ctx: &mut AppContext<'_>) -> RsxNode;

    fn on_event(&mut self, _event: &AppEvent, _ctx: &mut AppContext<'_>) {}

    /// Called once after the surface is created and the viewport is ready
    /// to render the first frame. Hooks that need the GPU (texture upload,
    /// font preloading, …) go here.
    fn on_ready(&mut self, _ctx: &mut AppContext<'_>) {}

    /// Called when the runner is about to exit. Return value is currently
    /// unused; reserved for future "request shutdown cancellation" semantics.
    fn on_shutdown(&mut self, _ctx: &mut AppContext<'_>) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::{
        CallbackCursorSink, CallbackRedrawRequester, NullClipboard, PlatformServices,
    };
    use crate::ui::RsxNode;
    use crate::view::viewport::{Viewport, ViewportControl};

    struct DummyApp {
        frames: u32,
    }

    impl App for DummyApp {
        fn build(&mut self, _ctx: &mut AppContext<'_>) -> RsxNode {
            self.frames += 1;
            RsxNode::text("")
        }

        fn on_event(&mut self, _event: &AppEvent, _ctx: &mut AppContext<'_>) {}
    }

    #[test]
    fn dummy_app_builds_against_live_context() {
        let mut viewport = Viewport::new();
        let mut clipboard = NullClipboard::default();
        let mut cursor = CallbackCursorSink::new(|_| {});
        let redraw = CallbackRedrawRequester::new(|| {});
        let mut app = DummyApp { frames: 0 };

        let mut ctx = AppContext {
            viewport: ViewportControl::new(&mut viewport),
            services: PlatformServices {
                clipboard: &mut clipboard,
                cursor: &mut cursor,
                redraw: &redraw,
            },
        };
        let _ = app.build(&mut ctx);
        assert_eq!(app.frames, 1);
    }

    #[test]
    fn app_config_default_values_match_docs() {
        let cfg = AppConfig::default();
        assert_eq!(cfg.title, "rfgui");
        assert_eq!(cfg.initial_size, (1280, 800));
        assert_eq!(cfg.scale_factor, None);
        assert!(!cfg.transparent);
        assert!(cfg.clear_color.is_none());
        assert_eq!(cfg.wheel.mouse_line_step, 28.0);
        assert_eq!(cfg.wheel.touchpad_pixel_scale, 1.0);
        assert_eq!(cfg.wheel.touchpad_deadzone, 0.5);
    }
}
