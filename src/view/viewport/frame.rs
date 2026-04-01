use super::*;
use std::time::Duration;

pub(super) struct BeginFrameProfile {
    pub total_ms: f64,
    pub acquire_ms: f64,
    pub create_view_ms: f64,
    pub create_encoder_ms: f64,
}

pub(super) struct EndFrameProfile {
    pub total_ms: f64,
    pub submit_ms: f64,
    pub present_ms: f64,
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
    pub render_texture: wgpu::SurfaceTexture,
    pub view: wgpu::TextureView,
    pub resolve_view: Option<wgpu::TextureView>,
    pub encoder: wgpu::CommandEncoder,
    pub depth_view: Option<wgpu::TextureView>,
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
