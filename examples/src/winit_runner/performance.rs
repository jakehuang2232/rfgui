use super::*;
use std::fmt::Write;

struct FrameSample {
    start_ms: f64,
    gap_ms: Option<f64>,
    wall_ms: f64,
    cpu_ms: [f64; 10],
    completion_ms: [f64; 6],
    frontend_ms: [f64; 4],
    counts: (u64, u64, u64),
    focused: Option<bool>,
    element_focused: bool,
    occluded: bool,
    requested: bool,
    animating: bool,
    input_events: u64,
    drain_ms: Option<f64>,
}

pub(super) struct WindowPerformance {
    limit: usize,
    samples: Vec<FrameSample>,
    counts: (u64, u64, u64),
    pending_redraw_observation: bool,
    drain_gpu: bool,
    continuous: bool,
    focus_root: bool,
    started: Instant,
    frame_started: Option<Instant>,
    previous_frame_started: Option<Instant>,
    pointer_sequence: u64,
    previous_pointer_sequence: u64,
    input: Vec<(f64, String, Option<(f32, f32)>)>,
    aborts: Vec<(f64, (u64, u64, u64))>,
}
impl WindowPerformance {
    pub(super) fn from_env() -> Option<Self> {
        let limit = std::env::var("RFGUI_WINDOW_PERF_FRAMES")
            .ok()?
            .parse()
            .expect("frame count");
        assert!(limit >= 40);
        Some(Self {
            limit,
            samples: Vec::with_capacity(limit),
            continuous: !std::env::var("RFGUI_WINDOW_PERF_DEMAND").is_ok_and(|v| v == "1"),
            focus_root: std::env::var("RFGUI_WINDOW_PERF_FOCUS_ROOT").as_deref() == Ok("1"),
            started: Instant::now(),
            frame_started: None,
            previous_frame_started: None,
            pointer_sequence: 0,
            previous_pointer_sequence: 0,
            input: Vec::new(),
            aborts: Vec::new(),
            counts: (0, 0, 0),
            pending_redraw_observation: false,
            drain_gpu: std::env::var("RFGUI_WINDOW_PERF_DRAIN_GPU").is_ok_and(|v| v == "1"),
        })
    }
    pub(super) fn continuous(&self) -> bool {
        self.continuous
    }
    pub(super) fn begin_render(&mut self) {
        self.frame_started = Some(Instant::now());
    }
    pub(super) fn pointer_event(&mut self, kind: &str, position: Option<(f32, f32)>) {
        self.pointer_sequence += 1;
        self.input.push((
            self.started.elapsed().as_secs_f64() * 1000.,
            kind.into(),
            position,
        ));
    }
    pub(super) fn observe(
        &mut self,
        viewport: &mut Viewport,
        wall_ms: f64,
        focused: Option<bool>,
        occluded: bool,
    ) {
        let (cpu_ms, completion_ms, counts) = viewport.renderer_performance_sample();
        if counts.2 != self.counts.2 {
            assert_eq!(counts.2, self.counts.2 + 1);
            assert_eq!((counts.0, counts.1), (self.counts.0, self.counts.1));
            // A temporarily unavailable surface aborts without submitting.
            // Preserve the attempt separately so the measurement can exclude
            // affected intervals; never turn it into a successful frame sample.
            self.aborts
                .push((self.started.elapsed().as_secs_f64() * 1000., counts));
            self.counts = counts;
            self.frame_started = None;
            return;
        }
        if counts.0 == self.counts.0 {
            return;
        }
        assert_eq!(counts.0, self.counts.0 + 1);
        assert_eq!(
            counts.1,
            self.counts.1 + 1,
            "submitted window frame must present"
        );
        self.counts = counts;
        if self.samples.is_empty() {
            if self.focus_root {
                viewport.set_focused_node_id(viewport.node_arena().roots().first().copied());
                viewport.request_redraw();
            }
            // One startup marker lets an external CPU sampler align its clock.
            // No formatting or output occurs per frame during measurement.
            println!(
                "window-ready pid={} elapsed_ms={:.6}",
                std::process::id(),
                self.started.elapsed().as_secs_f64() * 1000.
            );
        }
        let start = self.frame_started.take().expect("recorded frame start");
        let gap_ms = self
            .previous_frame_started
            .map(|old| (start - old).as_secs_f64() * 1000.);
        self.previous_frame_started = Some(start);
        let input_events = self.pointer_sequence - self.previous_pointer_sequence;
        self.previous_pointer_sequence = self.pointer_sequence;
        let frontend = viewport.frontend_profile();
        let drain_ms = self.drain_gpu.then(|| {
            let start = Instant::now();
            viewport
                .device()
                .expect("rendered viewport has device")
                .poll(wgpu::PollType::wait_indefinitely())
                .expect("diagnostic GPU drain");
            start.elapsed().as_secs_f64() * 1000.
        });
        self.samples.push(FrameSample {
            start_ms: (start - self.started).as_secs_f64() * 1000.,
            gap_ms,
            wall_ms,
            cpu_ms,
            completion_ms,
            frontend_ms: [
                frontend.total_ms(),
                frontend.state_flush_ms,
                frontend.rsx_build_ms,
                frontend.scene_update_ms,
            ],
            counts,
            focused,
            element_focused: viewport.focused_node_id().is_some(),
            occluded,
            requested: false,
            animating: false,
            input_events,
            drain_ms,
        });
        self.pending_redraw_observation = true;
    }
    pub(super) fn note_redraw(&mut self, requested: bool, animating: bool) {
        if !std::mem::take(&mut self.pending_redraw_observation) {
            return;
        }
        let sample = self.samples.last_mut().unwrap();
        sample.requested = requested;
        sample.animating = animating;
        if self.continuous
            && self.samples.len() > 30
            && std::env::var("RFGUI_PERF_UPDATES").is_ok_and(|mode| mode == "idle")
        {
            assert!(
                !requested && !animating,
                "idle engine must not schedule another frame"
            );
        }
    }
    pub(super) fn complete(&self) -> bool {
        self.samples.len() >= self.limit
    }
}

impl Drop for WindowPerformance {
    fn drop(&mut self) {
        // Flush only after the event loop exits, outside every measured interval.
        let mut output = String::new();
        for (frame, s) in self.samples.iter().enumerate() {
            let _ = writeln!(
                output,
                "window-perf frame={frame} start_ms={:.6} gap_ms={:?} wall_ms={:.6} cpu_ms={:?} frontend_ms={:?} completion_ms={:?} counts={:?} focused={:?} element_focused={} occluded={} requested={} animating={} input_events={} drain_ms={:?}",
                s.start_ms,
                s.gap_ms,
                s.wall_ms,
                s.cpu_ms,
                s.frontend_ms,
                s.completion_ms,
                s.counts,
                s.focused,
                s.element_focused,
                s.occluded,
                s.requested,
                s.animating,
                s.input_events,
                s.drain_ms
            );
        }
        for (at, counts) in &self.aborts {
            let _ = writeln!(output, "window-abort time_ms={at:.6} counts={counts:?}");
        }
        for (at, kind, position) in &self.input {
            let _ = writeln!(
                output,
                "window-input time_ms={at:.6} kind={kind} logical={position:?}"
            );
        }
        if let Ok(path) = std::env::var("RFGUI_WINDOW_PERF_OUTPUT") {
            std::fs::write(path, output).expect("write completed performance capture");
        } else {
            print!("{output}");
        }
    }
}
