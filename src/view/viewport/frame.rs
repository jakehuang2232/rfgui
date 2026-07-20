use super::*;
use std::time::Duration;

pub(super) struct BeginFrameProfile {
    pub total_ms: f64,
    pub acquire_ms: f64,
    pub create_view_ms: f64,
    pub create_encoder_ms: f64,
}

#[derive(Default)]
pub(super) struct EndFrameProfile {
    pub total_ms: f64,
    pub submit_ms: f64,
    pub present_ms: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FrameDisposition {
    SubmitAndPresent,
    Abort,
}

pub(super) struct FrameStats {
    enabled: bool,
    last_report_at: Instant,
    frames: u32,
    total_frame_time: Duration,
}

impl FrameStats {
    pub(super) fn new(enabled: bool) -> Self {
        Self {
            enabled,
            last_report_at: Instant::now(),
            frames: 0,
            total_frame_time: Duration::ZERO,
        }
    }

    pub(super) fn set_enabled(&mut self, enabled: bool) {
        if self.enabled == enabled {
            return;
        }
        self.enabled = enabled;
        self.last_report_at = Instant::now();
        self.frames = 0;
        self.total_frame_time = Duration::ZERO;
    }

    pub(super) fn record_frame(&mut self, frame_time: Duration) {
        if !self.enabled {
            return;
        }

        self.frames += 1;
        self.total_frame_time += frame_time;

        let elapsed = self.last_report_at.elapsed();
        if elapsed < Duration::from_secs(1) {
            return;
        }

        let secs = elapsed.as_secs_f64().max(f64::EPSILON);
        let fps = self.frames as f64 / secs;
        let avg_ms = if self.frames == 0 {
            0.0
        } else {
            (self.total_frame_time.as_secs_f64() * 1000.0) / self.frames as f64
        };

        eprintln!(
            "[perf ] fps={:.1} frame_avg={:.2}ms frames={}",
            fps, avg_ms, self.frames
        );

        self.last_report_at = Instant::now();
        self.frames = 0;
        self.total_frame_time = Duration::ZERO;
    }
}

pub(super) struct FrameState {
    #[cfg(not(test))]
    pub render_texture: wgpu::SurfaceTexture,
    #[cfg(test)]
    pub render_texture: Option<wgpu::SurfaceTexture>,
    #[cfg(test)]
    pub offscreen_texture: Option<wgpu::Texture>,
    pub view: wgpu::TextureView,
    pub resolve_view: Option<wgpu::TextureView>,
    pub encoder: wgpu::CommandEncoder,
    pub depth_view: Option<wgpu::TextureView>,
}

impl FrameState {
    /// Discard an acquired frame without finishing its encoder. Dropping the
    /// unsubmitted encoder abandons every command recorded so far, while the
    /// final `SurfaceTexture` drop releases the acquired image without
    /// presenting it.
    pub(super) fn discard_unsubmitted(self) {
        #[cfg(not(test))]
        let Self {
            render_texture,
            view,
            resolve_view,
            encoder,
            depth_view,
        } = self;
        #[cfg(test)]
        let Self {
            render_texture,
            offscreen_texture,
            view,
            resolve_view,
            encoder,
            depth_view,
        } = self;

        // Encoder first: all views and transient resources may be referenced
        // by commands that must never become a command buffer.
        drop(encoder);
        drop(resolve_view);
        drop(depth_view);
        drop(view);
        #[cfg(test)]
        drop(offscreen_texture);
        // Keep the acquired surface image last so its Drop path can discard it
        // after every unsubmitted reference owned by FrameState is gone.
        drop(render_texture);
    }
}

/// Collects all per-frame profiling timings so they can be passed to trace
/// tree construction without scattering ~20 individual variables.
#[derive(Default)]
pub(super) struct FrameTimings {
    pub begin_frame_ms: f64,
    pub begin_frame_acquire_ms: f64,
    pub begin_frame_create_view_ms: f64,
    pub begin_frame_create_encoder_ms: f64,

    pub layout_ms: f64,
    pub layout_measure_ms: f64,
    pub layout_place_ms: f64,
    pub layout_collect_box_models_ms: f64,
    pub layout_traversal_profile: LayoutTraversalProfile,
    pub layout_text_measure_profile: crate::view::base_component::TextMeasureProfile,
    pub layout_place_profile: crate::view::base_component::LayoutPlaceProfile,

    pub post_layout_transition_ms: f64,

    pub relayout_ms: f64,
    pub relayout_measure_ms: f64,
    pub relayout_place_ms: f64,
    pub relayout_collect_box_models_ms: f64,
    pub relayout_traversal_profile: LayoutTraversalProfile,
    pub relayout_place_profile: crate::view::base_component::LayoutPlaceProfile,

    pub build_graph_ms: f64,

    pub compile_ms: f64,
    pub compile_children: Vec<super::debug::TraceRenderNode>,

    pub execute_ms: f64,
    pub execute_pass_count: usize,
    pub execute_ordered_passes: Vec<(String, f64, usize)>,
    pub execute_detail_ordered_passes: Vec<(String, f64, usize)>,

    pub end_frame_ms: f64,
    pub end_frame_submit_ms: f64,
    pub end_frame_present_ms: f64,

    pub total_ms: f64,

    /// Time spent in `App::build()` producing the RSX tree.  Measured in
    /// `render_frame` and injected before the trace tree is built.
    pub rsx_build_ms: f64,

    pub frame_number: u64,
}

/// Fine-grained traversal timings inside one layout pass.
#[derive(Clone, Copy, Default)]
pub(super) struct LayoutTraversalProfile {
    pub root_count: usize,
    pub sync_registered_elements_ms: f64,
    pub dirty_refresh_before_measure_ms: f64,
    pub measure_roots_ms: f64,
    pub measure_candidate_clean_children: usize,
    pub measure_dirty_children: usize,
    pub dirty_refresh_before_place_ms: f64,
    pub place_roots_ms: f64,
    pub placement_candidate_clean_children: usize,
    pub placement_dirty_children: usize,
    pub skipped_child_place_calls: usize,
    pub collect_box_models_ms: f64,
}

/// Result of a single layout pass (measure → place → collect_box_models).
pub(super) struct LayoutPassResult {
    pub measure_ms: f64,
    pub place_ms: f64,
    pub collect_box_models_ms: f64,
    pub traversal_profile: LayoutTraversalProfile,
    pub text_measure_profile: crate::view::base_component::TextMeasureProfile,
    pub place_profile: crate::view::base_component::LayoutPlaceProfile,
}

pub struct FrameParts<'a> {
    pub encoder: &'a mut wgpu::CommandEncoder,
    pub view: &'a wgpu::TextureView,
    pub resolve_view: Option<&'a wgpu::TextureView>,
    pub depth_view: Option<&'a wgpu::TextureView>,
}

impl<'a> FrameParts<'a> {
    pub fn depth_stencil_attachment(
        &self,
        depth_load: wgpu::LoadOp<f32>,
        stencil_load: wgpu::LoadOp<u32>,
    ) -> Option<wgpu::RenderPassDepthStencilAttachment<'a>> {
        self.depth_view
            .map(|view| wgpu::RenderPassDepthStencilAttachment {
                view,
                depth_ops: Some(wgpu::Operations {
                    load: depth_load,
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: Some(wgpu::Operations {
                    load: stencil_load,
                    store: wgpu::StoreOp::Store,
                }),
            })
    }
}
