//! Headless CPU benchmark for pending state -> App::build -> arena commit.
//! No surface is acquired: these numbers exclude layout, raster, and GPU work.
//! Run alone with --ignored --nocapture --test-threads=1, without trace/features.
use rfgui::app::{App, AppContext};
use rfgui::platform::{HeadlessBackend, PlatformServices};
use rfgui::time::Instant;
use rfgui::ui::{Binding, RsxNode, component, rsx};
use rfgui::view::Element;
use rfgui::view::viewport::Viewport;

#[component]
fn Row(value: usize) -> RsxNode {
    rsx! { <Element>{value.to_string()}</Element> }
}
struct BenchApp {
    state: Binding<usize>,
    rows: usize,
}
impl App for BenchApp {
    fn build(&mut self, _: &mut AppContext<'_>) -> RsxNode {
        let current = self.state.snapshot().get();
        rsx! {
            <Element>
                {(0..self.rows).map(|i| rsx! { <Row value={if i + 1 == self.rows { current } else { i }} /> }).collect::<Vec<_>>()}
            </Element>
        }
    }
}

#[test]
#[ignore = "CPU benchmark; run alone without tracing or renderer-test-support"]
fn state_build_commit_cpu_baseline() {
    for rows in [128, 512, 2048] {
        let state = Binding::new(0usize);
        let mut viewport = Viewport::new();
        viewport.set_app(Box::new(BenchApp {
            state: state.clone(),
            rows,
        }));
        let mut host = HeadlessBackend::default();
        let mut samples = Vec::new();
        for frame in 0..230 {
            let start = Instant::now();
            state.set(frame + 1);
            viewport.render_frame(PlatformServices {
                clipboard: &mut host.clipboard,
                cursor: &mut host.cursor,
                redraw: &host.redraw,
            });
            let ms = start.elapsed().as_secs_f64() * 1000.0;
            assert_eq!(state.snapshot().get(), frame + 1);
            if frame >= 30 {
                samples.push(ms);
            }
        }
        samples.sort_by(f64::total_cmp);
        println!(
            "frontend-cpu rows={rows} n={} p50_ms={:.6} p95_ms={:.6}",
            samples.len(),
            (samples[99] + samples[100]) / 2.0,
            samples[189]
        );
    }
}
