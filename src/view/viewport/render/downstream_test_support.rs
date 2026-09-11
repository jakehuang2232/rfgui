//! Opt-in downstream integration-test observation. No selector overrides or
//! fallback exemptions: controls run their real RSX reconciliation and renderer.
use super::*;

pub struct RendererTestFrame {
    pub texture: wgpu::Texture,
    pub artifact_selected: bool,
    pub rerasterizations: usize,
    pub reuses: usize,
    pub resident_pairs: usize,
    pub texture_bytes: u64,
    pub persistent_targets: Vec<(String, [u32; 2])>,
}

impl Viewport {
    /// Acquire an offscreen test target and use the normal RSX frame pipeline.
    /// `now` is one deterministic semantic animation time, not a profiling clock.
    /// Panics/errors fail the test; callers must not continue a failed frame.
    #[doc(hidden)]
    pub fn render_rsx_offscreen_for_test(
        &mut self,
        root: &RsxNode,
        device: wgpu::Device,
        queue: wgpu::Queue,
        size: [u32; 2],
        dpr: f32,
        now: crate::time::Instant,
    ) -> Result<RendererTestFrame, String> {
        if !dpr.is_finite() || dpr <= 0.0 || size.contains(&0) {
            return Err("invalid test viewport".into());
        }
        self.begin_offscreen_test_frame(
            device,
            queue,
            size[0],
            size[1],
            wgpu::TextureFormat::Rgba8Unorm,
        )?;
        // Acquisition resets scale; this order is required on every frame.
        self.set_scale_factor(dpr);
        self.clear_color = Box::new(crate::style::Color::rgba(0, 0, 0, 0));
        let texture = self
            .frame
            .frame_state
            .as_ref()
            .unwrap()
            .offscreen_texture
            .clone()
            .unwrap();
        let _capture = enable_paint_authority_test_capture();
        let _ = crate::view::paint::take_last_production_actions_for_test();
        let before = self.frame_completion_counts_for_test();
        self.render_rsx_at(root, now)?;
        let after = self.frame_completion_counts_for_test();
        if self.frame.frame_state.is_some() || after != (before.0 + 1, before.1, before.2) {
            return Err(format!(
                "frame did not submit cleanly: {before:?} -> {after:?}"
            ));
        }
        let snapshot = take_paint_authority_test_snapshot().ok_or("missing authority")?;
        if snapshot.legacy_fallback_stage.is_some() || snapshot.terminal_failure_stage.is_some() {
            return Err(format!("unexpected fallback/failure: {snapshot:?}"));
        }
        let artifact_selected = snapshot.selected == PaintAuthorityKind::Artifact;
        if artifact_selected
            != (self.paint_renderer_mode() == ViewportPaintRendererMode::RetainedAuto)
        {
            return Err(format!("unexpected authority: {snapshot:?}"));
        }
        let actions = crate::view::paint::take_last_production_actions_for_test();
        let mut resident_pairs = 0;
        let mut texture_bytes = 0;
        let mut persistent_targets = Vec::new();
        let graph = self
            .frame
            .last_frame_graph
            .as_ref()
            .ok_or("missing executed graph")?;
        for (key, desc) in graph.declared_persistent_textures() {
            texture_bytes += crate::view::raster_cost::texture_desc_payload_bytes(desc).bytes;
            if key.depth_stencil().is_some() {
                if !self.has_compatible_persistent_render_target_pair(key, desc) {
                    return Err("missing resident pair".into());
                }
                resident_pairs += 1;
                persistent_targets.push((format!("{key:?}"), [desc.width(), desc.height()]));
            }
        }
        persistent_targets.sort();
        Ok(RendererTestFrame {
            texture,
            artifact_selected,
            resident_pairs,
            texture_bytes,
            persistent_targets,
            rerasterizations: actions
                .iter()
                .filter(|a| {
                    matches!(
                        a,
                        crate::view::paint::RetainedSurfaceCompileAction::Reraster
                    )
                })
                .count(),
            reuses: actions
                .iter()
                .filter(|a| matches!(a, crate::view::paint::RetainedSurfaceCompileAction::Reuse))
                .count(),
        })
    }
}
