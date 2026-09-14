use super::*;

pub(super) struct WindowPerformance {
    limit: usize,
    frames: usize,
    counts: (u64, u64, u64),
    pending_redraw_observation: bool,
    drain_gpu: bool,
    continuous: bool,
    started: Instant,
    frame_started: Option<Instant>,
    previous_frame_started: Option<Instant>,
    pointer_sequence: u64,
    previous_pointer_sequence: u64,
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
            continuous: !std::env::var("RFGUI_WINDOW_PERF_DEMAND").is_ok_and(|v| v == "1"),
            started: Instant::now(),
            frame_started: None,
            previous_frame_started: None,
            pointer_sequence: 0,
            previous_pointer_sequence: 0,
            frames: 0,
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
        if self.continuous {
            return;
        }
        self.pointer_sequence += 1;
        println!(
            "window-input event={} time_ms={:.6} kind={kind} logical={position:?}",
            self.pointer_sequence,
            self.started.elapsed().as_secs_f64() * 1000.
        );
    }
    pub(super) fn observe(
        &mut self,
        viewport: &Viewport,
        wall_ms: f64,
        focused: Option<bool>,
        occluded: bool,
    ) {
        let (cpu, completion, counts) = viewport.renderer_performance_sample();
        assert_eq!(counts.2, self.counts.2, "window frame aborted");
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
        let frame_started = self.frame_started.take().expect("recorded frame start");
        let frame_start_ms = frame_started.duration_since(self.started).as_secs_f64() * 1000.;
        let frame_gap_ms = self
            .previous_frame_started
            .map(|old| frame_started.duration_since(old).as_secs_f64() * 1000.);
        let input_events = self.pointer_sequence - self.previous_pointer_sequence;
        self.previous_frame_started = Some(frame_started);
        self.previous_pointer_sequence = self.pointer_sequence;
        if !self.continuous {
            println!(
                "window-frame-input frame={} start_ms={frame_start_ms:.6} gap_ms={frame_gap_ms:?} event={} input_events={input_events}",
                self.frames, self.pointer_sequence
            );
        }
        println!(
            "window-perf frame={} nodes={} logical={:?} physical={:?} wall_ms={wall_ms:.6} cpu_ms={cpu:?} completion_ms={completion:?} counts={counts:?} focused={focused:?} occluded={occluded}",
            self.frames,
            viewport.node_arena().len(),
            viewport.logical_size(),
            viewport.surface_size()
        );
        if self.drain_gpu {
            // Diagnostic control matching the offscreen harness's post-sample
            // drain. Report its cost separately: this is not a production
            // optimization and must not be used for normal frame acceptance.
            let start = rfgui::time::Instant::now();
            viewport
                .device()
                .expect("rendered viewport has a device")
                .poll(wgpu::PollType::wait_indefinitely())
                .expect("diagnostic GPU drain");
            println!(
                "window-gpu-drain frame={} wait_ms={:.6}",
                self.frames,
                start.elapsed().as_secs_f64() * 1000.
            );
        }
        self.frames += 1;
        self.pending_redraw_observation = true;
    }
    pub(super) fn note_redraw(&mut self, requested: bool, animating: bool) {
        if !std::mem::take(&mut self.pending_redraw_observation) {
            return;
        }
        println!(
            "window-demand frame={} requested={requested} animating={animating}",
            self.frames - 1
        );
        if self.continuous
            && self.frames > 30
            && std::env::var("RFGUI_PERF_UPDATES").is_ok_and(|mode| mode == "idle")
        {
            assert!(
                !requested && !animating,
                "idle engine must not schedule another frame"
            );
        }
    }
    pub(super) fn complete(&self) -> bool {
        self.frames >= self.limit
    }
}
