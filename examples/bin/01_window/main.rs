extern crate rfgui;
extern crate rfgui_components;

#[cfg(target_arch = "wasm32")]
#[global_allocator]
static ALLOCATOR: rlsf::SmallGlobalTlsf = rlsf::SmallGlobalTlsf::new();

mod components;
mod scene;
mod scene_windows;
mod utils;
mod window_manager;

use rfgui::app::{App, AppConfig, AppContext, WheelConfig};
use rfgui::style::Color;
use rfgui::ui::{RsxNode, rsx};
use rfgui::view::viewport::ViewportPaintRendererMode;

use crate::scene::MainScene;

const RETAINED_AUTO_LABEL: &str = "retained-auto";
const LEGACY_LABEL: &str = "legacy";
#[cfg(not(target_arch = "wasm32"))]
const PAINT_RENDERER_ENV: &str = "RFGUI_PAINT_RENDERER";
#[cfg(any(target_arch = "wasm32", test))]
const PAINT_RENDERER_QUERY: &str = "rfgui-paint";

struct WindowDemoApp {
    paint_renderer_mode: ViewportPaintRendererMode,
}

impl App for WindowDemoApp {
    fn build(&mut self, _ctx: &mut AppContext<'_>) -> RsxNode {
        rsx! { <MainScene /> }
    }

    fn on_ready(&mut self, ctx: &mut AppContext<'_>) {
        ctx.viewport
            .set_paint_renderer_mode(self.paint_renderer_mode);
    }
}

fn parse_paint_renderer_mode(value: Option<&str>) -> ViewportPaintRendererMode {
    match value {
        Some(RETAINED_AUTO_LABEL) => ViewportPaintRendererMode::RetainedAuto,
        Some(LEGACY_LABEL) | None => ViewportPaintRendererMode::Legacy,
        Some(_) => ViewportPaintRendererMode::Legacy,
    }
}

#[cfg(any(target_arch = "wasm32", test))]
fn query_parameter<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    let mut matched_count = 0;
    let mut matched_value = None;
    for pair in query.strip_prefix('?').unwrap_or(query).split('&') {
        let (candidate, value) = match pair.split_once('=') {
            Some((candidate, value)) => (candidate, Some(value)),
            None => (pair, None),
        };
        if candidate == key {
            matched_count += 1;
            matched_value = value;
        }
    }
    (matched_count == 1).then_some(matched_value).flatten()
}

#[cfg(not(target_arch = "wasm32"))]
fn configured_paint_renderer_mode() -> ViewportPaintRendererMode {
    let value = std::env::var(PAINT_RENDERER_ENV).ok();
    parse_paint_renderer_mode(value.as_deref())
}

#[cfg(target_arch = "wasm32")]
fn configured_paint_renderer_mode() -> ViewportPaintRendererMode {
    let query = web_sys::window()
        .and_then(|window| js_sys::Reflect::get(window.as_ref(), &"location".into()).ok())
        .and_then(|location| js_sys::Reflect::get(&location, &"search".into()).ok())
        .and_then(|search| search.as_string());
    parse_paint_renderer_mode(
        query
            .as_deref()
            .and_then(|query| query_parameter(query, PAINT_RENDERER_QUERY)),
    )
}

fn make_config() -> AppConfig {
    AppConfig {
        title: String::from("RFGUI Window Demo"),
        initial_size: (1280, 800),
        scale_factor: None,
        transparent: false,
        clear_color: Some(Color::rgb(40, 44, 52)),
        wheel: WheelConfig::default(),
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    examples::winit_runner::run(
        WindowDemoApp {
            paint_renderer_mode: configured_paint_renderer_mode(),
        },
        make_config(),
    );
}

#[cfg(target_arch = "wasm32")]
fn main() {
    examples::web_runner::run(
        WindowDemoApp {
            paint_renderer_mode: configured_paint_renderer_mode(),
        },
        make_config(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paint_renderer_mode_parser_is_opt_in_and_fail_closed() {
        assert_eq!(
            parse_paint_renderer_mode(Some(RETAINED_AUTO_LABEL)),
            ViewportPaintRendererMode::RetainedAuto
        );
        for value in [
            None,
            Some(LEGACY_LABEL),
            Some(""),
            Some("artifact"),
            Some("RETAINED-AUTO"),
        ] {
            assert_eq!(
                parse_paint_renderer_mode(value),
                ViewportPaintRendererMode::Legacy
            );
        }
    }

    #[test]
    fn query_parser_reads_only_the_exact_pilot_key() {
        assert_eq!(
            query_parameter(
                "?unrelated=1&rfgui-paint=retained-auto&after=2",
                PAINT_RENDERER_QUERY,
            ),
            Some(RETAINED_AUTO_LABEL)
        );
        assert_eq!(
            query_parameter("?rfgui-paint=legacy", PAINT_RENDERER_QUERY),
            Some(LEGACY_LABEL)
        );
        assert_eq!(
            query_parameter("?rfgui-painter=retained-auto", PAINT_RENDERER_QUERY),
            None
        );
        assert_eq!(query_parameter("?rfgui-paint", PAINT_RENDERER_QUERY), None);
        assert_eq!(
            query_parameter(
                "?rfgui-paint=retained-auto&rfgui-paint=legacy",
                PAINT_RENDERER_QUERY,
            ),
            None
        );
        assert_eq!(
            query_parameter(
                "?rfgui-paint=legacy&rfgui-paint=retained-auto",
                PAINT_RENDERER_QUERY,
            ),
            None
        );
        assert_eq!(
            query_parameter(
                "?rfgui-paint=retained-auto&rfgui-paint=retained-auto",
                PAINT_RENDERER_QUERY,
            ),
            None
        );
    }
}
