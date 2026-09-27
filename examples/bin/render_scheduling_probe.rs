//! Small native workload for demand rendering and transition measurements.
use rfgui::app::{App, AppConfig, AppContext};
use rfgui::style::{Color, Length, Transition, TransitionProperty};
use rfgui::ui::{RsxNode, rsx};
use rfgui::view::viewport::ViewportPaintRendererMode;
use rfgui::view::{Element, TextArea};

struct Probe(RsxNode);

impl App for Probe {
    fn build(&mut self, _: &mut AppContext<'_>) -> RsxNode {
        self.0.clone()
    }

    fn on_ready(&mut self, ctx: &mut AppContext<'_>) {
        let mode = if std::env::var("RFGUI_PROBE_RENDERER").as_deref() == Ok("legacy") {
            ViewportPaintRendererMode::Legacy
        } else {
            ViewportPaintRendererMode::RetainedAuto
        };
        ctx.viewport.set_paint_renderer_mode(mode);
    }
}

fn main() {
    let scene = if std::env::var("RFGUI_PROBE_SCENE").as_deref() == Ok("transition") {
        rsx! {
            <Element style={{
                width: Length::px(300.), height: Length::px(180.),
                background_color: Color::hex("#ff0000"),
                transition: [Transition::new(TransitionProperty::BackgroundColor, 60_000)],
                hover: { background_color: Color::hex("#0000ff") },
            }} />
        }
    } else {
        rsx! {
            <TextArea content={"Focused caret measurement".to_string()} style={{
                width: Length::px(300.), height: Length::px(180.),
                background_color: Color::hex("#ffffff"),
            }} />
        }
    };
    examples::winit_runner::run(
        Probe(scene),
        AppConfig {
            title: "RFGUI scheduling probe".into(),
            initial_size: (360, 240),
            clear_color: Some(Color::rgb(240, 240, 240)),
            ..Default::default()
        },
    );
}
