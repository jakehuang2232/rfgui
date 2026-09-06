use super::*;

pub(crate) struct SingleViewportFrameObservation {
    pub(crate) texture: wgpu::Texture,
    pub(crate) artifact_selected: bool,
    pub(crate) legacy_selected: bool,
    pub(crate) actions: Vec<crate::view::paint::RetainedSurfaceCompileAction>,
    pub(crate) color_targets: Vec<(
        crate::view::frame_graph::PersistentTextureKey,
        crate::view::frame_graph::TextureDesc,
    )>,
    pub(crate) texture_bytes: u64,
    pub(crate) frame_number: u64,
}

impl Viewport {
    pub(crate) fn install_single_viewport_scene_for_test(
        &mut self,
        arena: crate::view::node_arena::NodeArena,
        root: crate::view::node_arena::NodeKey,
    ) {
        self.scene.node_arena = arena;
        self.scene.ui_root_keys = vec![root];
        self.clear_color = Box::new(crate::style::Color::rgba(0, 0, 0, 0));
    }

    /// Only window-surface acquisition is supplied by begin_offscreen_test_frame.
    /// The production frame entry owns layout, resource preparation, property
    /// observation, selection, graph execution, pool commit, and submission.
    /// The returned texture handle survives submission for independent readback.
    pub(crate) fn render_single_viewport_scene_for_test(
        &mut self,
    ) -> Result<SingleViewportFrameObservation, String> {
        let texture = self
            .frame
            .frame_state
            .as_ref()
            .and_then(|frame| frame.offscreen_texture.clone())
            .ok_or("single-viewport test requires an active offscreen frame")?;
        let _capture = enable_paint_authority_test_capture();
        let _ = crate::view::paint::take_last_production_actions_for_test();
        let before = self.frame_completion_counts_for_test();
        // The bool reports transition/animation redraw demand, not render
        // success. Submission/abort counts and authority telemetry below
        // establish success; these static fixtures need no redraw assertion.
        let _ = self.render_render_tree(0.0, 0.0, crate::time::Instant::now());
        let after = self.frame_completion_counts_for_test();
        if self.frame.frame_state.is_some() || after.0 != before.0 + 1 || after.2 != before.2 {
            return Err(format!(
                "frame did not submit cleanly: before={before:?}, after={after:?}"
            ));
        }
        let snapshot = take_paint_authority_test_snapshot().ok_or("missing frame authority")?;
        if snapshot.legacy_fallback_stage.is_some() || snapshot.terminal_failure_stage.is_some() {
            return Err(format!("frame failed or fell back: {snapshot:?}"));
        }
        let graph = self
            .frame
            .last_frame_graph
            .as_ref()
            .ok_or("missing executed frame graph")?;
        let mut texture_bytes = 0_u64;
        let mut color_targets = Vec::new();
        for (key, desc) in graph.declared_persistent_textures() {
            texture_bytes = texture_bytes
                .checked_add(crate::view::raster_cost::texture_desc_payload_bytes(desc).bytes)
                .ok_or("frame texture accounting overflow")?;
            if key.depth_stencil().is_some() {
                color_targets.push((key, desc.clone()));
            }
        }
        Ok(SingleViewportFrameObservation {
            texture,
            artifact_selected: snapshot.selected == PaintAuthorityKind::Artifact,
            legacy_selected: snapshot.selected == PaintAuthorityKind::Legacy,
            actions: crate::view::paint::take_last_production_actions_for_test(),
            color_targets,
            texture_bytes,
            frame_number: self.frame.frame_number,
        })
    }
}
