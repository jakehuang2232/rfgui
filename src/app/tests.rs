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
