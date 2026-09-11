use super::*;

pub(crate) struct SingleViewportFrameObservation {
    pub(crate) texture: wgpu::Texture,
    pub(crate) artifact_selected: bool,
    pub(crate) legacy_selected: bool,
    pub(crate) rejection_labels: Vec<String>,
    pub(crate) actions: Vec<crate::view::paint::RetainedSurfaceCompileAction>,
    pub(crate) color_targets: Vec<(
        crate::view::frame_graph::PersistentTextureKey,
        crate::view::frame_graph::TextureDesc,
    )>,
    pub(crate) texture_bytes: u64,
    pub(crate) frame_number: u64,
}

impl Viewport {
    pub(crate) fn edit_scene_arena_for_test(
        &mut self,
        edit: impl FnOnce(&mut crate::view::node_arena::NodeArena),
    ) {
        edit(&mut self.scene.node_arena);
    }

    pub(crate) fn install_single_viewport_scene_for_test(
        &mut self,
        arena: crate::view::node_arena::NodeArena,
        root: crate::view::node_arena::NodeKey,
    ) {
        self.install_single_viewport_forest_for_test(arena, vec![root]);
    }

    pub(crate) fn install_single_viewport_forest_for_test(
        &mut self,
        arena: crate::view::node_arena::NodeArena,
        roots: Vec<crate::view::node_arena::NodeKey>,
    ) {
        self.scene.node_arena = arena;
        self.scene.ui_root_keys = roots;
        self.clear_color = Box::new(crate::style::Color::rgba(0, 0, 0, 0));
    }

    /// Only window-surface acquisition is supplied by begin_offscreen_test_frame.
    /// The production frame entry owns layout, resource preparation, property
    /// observation, selection, graph execution, pool commit, and submission.
    /// The returned texture handle survives submission for independent readback.
    pub(crate) fn render_single_viewport_scene_for_test(
        &mut self,
    ) -> Result<SingleViewportFrameObservation, String> {
        self.render_single_viewport_observed_frame_for_test(None)
    }

    pub(crate) fn render_single_viewport_legacy_recovery_for_test(
        &mut self,
    ) -> Result<SingleViewportFrameObservation, String> {
        if self.retained_auto_terminal_failure != Some(RetainedAutoTerminalFailureStage::Execute) {
            return Err("Legacy recovery requires a latched execute failure".into());
        }
        self.render_single_viewport_observed_frame_for_test(Some(
            PaintAuthorityFallbackStage::Execute,
        ))
    }

    /// A caller must name the expected unavailable-recording reason. This is
    /// distinct from success and execute-failure recovery, so neither can
    /// accidentally pass through a selection rejection.
    pub(crate) fn render_single_viewport_selection_fallback_for_test(
        &mut self,
        expected_rejection: &str,
    ) -> Result<SingleViewportFrameObservation, String> {
        if self.paint_renderer_mode != ViewportPaintRendererMode::RetainedAuto
            || self.retained_auto_terminal_failure.is_some()
        {
            return Err("selection fallback requires Auto without a terminal failure".into());
        }
        let observed = self.render_single_viewport_observed_frame_for_test(Some(
            PaintAuthorityFallbackStage::Selection,
        ))?;
        if !observed
            .rejection_labels
            .iter()
            .any(|label| label == expected_rejection)
        {
            return Err(format!(
                "missing expected rejection {expected_rejection}: {:?}",
                observed.rejection_labels
            ));
        }
        Ok(observed)
    }

    fn render_single_viewport_observed_frame_for_test(
        &mut self,
        expected_fallback: Option<PaintAuthorityFallbackStage>,
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
        if snapshot.legacy_fallback_stage != expected_fallback
            || snapshot.terminal_failure_stage.is_some()
            || (expected_fallback.is_some() && snapshot.selected != PaintAuthorityKind::Legacy)
        {
            return Err(format!(
                "unexpected frame outcome: {snapshot:?}, expected fallback {expected_fallback:?}"
            ));
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
            rejection_labels: snapshot.rejection_labels,
            actions: crate::view::paint::take_last_production_actions_for_test(),
            color_targets,
            texture_bytes,
            frame_number: self.frame.frame_number,
        })
    }

    pub(crate) fn render_single_viewport_budget_fallback_for_test(
        &mut self,
    ) -> Result<SingleViewportFrameObservation, String> {
        let observed = self.render_single_viewport_observed_frame_for_test(Some(
            PaintAuthorityFallbackStage::Prepare,
        ))?;
        if observed.rejection_labels.len() != 1
            || !observed.rejection_labels[0]
                .starts_with("artifact-prepare:RasterPlan(TextureBudgetExceeded(")
            || !observed.actions.is_empty()
            || self.retained_surface_transaction_shape_for_test() != (0, None)
        {
            return Err(format!(
                "budget rejection must bypass compatibility and stage no residents: {:?}",
                observed.rejection_labels
            ));
        }
        Ok(observed)
    }
}

impl Viewport {
    /// Observe an intentionally failed production frame without relaxing the
    /// success harness. The caller must arm a scoped execution fault first.
    pub(crate) fn render_single_viewport_execution_failure_for_test(
        &mut self,
    ) -> Result<(), String> {
        if self.frame.frame_state.is_none() {
            return Err("failure test requires an acquired frame".into());
        }
        let _capture = enable_paint_authority_test_capture();
        let before = self.frame_completion_counts_for_test();
        let _ = self.render_render_tree(0.0, 0.0, crate::time::Instant::now());
        let after = self.frame_completion_counts_for_test();
        if after != (before.0, before.1, before.2 + 1) || self.frame.frame_state.is_some() {
            return Err(format!(
                "failed frame must abort without submission: {before:?} -> {after:?}"
            ));
        }
        let snapshot =
            take_paint_authority_test_snapshot().ok_or("missing failed-frame authority")?;
        if snapshot.selected != PaintAuthorityKind::Artifact
            || snapshot.terminal_failure_stage != Some(PaintAuthorityFallbackStage::Execute)
            || snapshot.legacy_fallback_stage.is_some()
        {
            return Err(format!("unexpected failed-frame authority: {snapshot:?}"));
        }
        if self.retained_auto_terminal_failure != Some(RetainedAutoTerminalFailureStage::Execute)
            || self.frame.compile_cache.is_some()
            || self.retained_surface_transaction_shape_for_test() != (0, None)
        {
            return Err(
                "failed frame left a cache/transaction or did not latch execute failure".into(),
            );
        }
        Ok(())
    }
}

impl Viewport {
    pub(crate) fn sampled_cache_observation_for_test(
        &self,
        id: crate::view::sampled_texture::SampledTextureId,
    ) -> (Option<(u64, u32, u32)>, u64, u64) {
        (
            self.frame
                .sampled_texture_cache
                .get(&id)
                .map(|entry| (entry.generation, entry.width, entry.height)),
            self.frame.sampled_texture_upload_count,
            self.frame
                .sampled_texture_cache
                .values()
                .map(|entry| entry.byte_size)
                .sum(),
        )
    }
    pub(crate) fn sampled_cache_policy_for_test() -> (u64, u64, u64) {
        (
            Self::SAMPLED_TEXTURE_PRESSURE_BYTES,
            Self::SAMPLED_TEXTURE_EVICT_TO_BYTES,
            Self::SAMPLED_TEXTURE_STALE_FRAMES,
        )
    }
}

// Scoped to the current test thread; callbacks cannot leak on early return or
// unwind. This seam runs after the production frame counter and resource freeze,
// so resource uploads use the current frame's real pinning epoch.
thread_local! {
    static AFTER_FREEZE: std::cell::RefCell<Option<Box<dyn FnOnce(&mut Viewport)>>> = const { std::cell::RefCell::new(None) };
}
struct AfterFreezeGuard;
impl Drop for AfterFreezeGuard {
    fn drop(&mut self) {
        AFTER_FREEZE.with(|slot| {
            slot.borrow_mut().take();
        });
    }
}
pub(super) fn run_after_resource_freeze(viewport: &mut Viewport) {
    let action = AFTER_FREEZE.with(|slot| slot.borrow_mut().take());
    if let Some(action) = action {
        action(viewport);
    }
}
impl Viewport {
    pub(crate) fn render_single_viewport_after_freeze_for_test(
        &mut self,
        action: impl FnOnce(&mut Viewport) + 'static,
    ) -> Result<SingleViewportFrameObservation, String> {
        AFTER_FREEZE.with(|slot| {
            assert!(
                slot.borrow().is_none(),
                "resource-freeze callback already active"
            );
            *slot.borrow_mut() = Some(Box::new(action));
        });
        let _guard = AfterFreezeGuard;
        let observed = self.render_single_viewport_scene_for_test()?;
        if AFTER_FREEZE.with(|slot| slot.borrow().is_some()) {
            return Err("production resource-freeze callback did not run".into());
        }
        Ok(observed)
    }
}
