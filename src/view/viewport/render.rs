mod surface_acquisition;
#[cfg(test)]
mod attempts;
#[cfg(test)]
mod tests;

use super::*;

#[cfg(test)]
mod single_viewport_frame_test_support;
#[cfg(test)]
pub(crate) use single_viewport_frame_test_support::SingleViewportFrameObservation;

fn frame_disposition(compiled: bool, executed: bool) -> FrameDisposition {
    if compiled && executed {
        FrameDisposition::SubmitAndPresent
    } else {
        FrameDisposition::Abort
    }
}

fn should_store_compile_cache(compiled: bool, executed: bool) -> bool {
    compiled && executed
}

// What the surface cache may keep across frames: color + depth payload of
// GPU source outputs and surface rasters (128 MiB), counted with real physical
// descriptors after DPR and content-envelope resolution. Past it a surface is
// still drawn, every frame, into a frame-local target. This is not a cap on
// total GPU residency or sampled resources.
const ARTIFACT_SURFACE_AGGREGATE_BUDGET_BYTES: u64 = 128 * 1024 * 1024;

fn artifact_surface_raster_context(
    ctx: &crate::view::base_component::UiBuildContext,
    max_texture_dimension_2d: u32,
    max_texture_bytes: u64,
) -> crate::view::paint::ArtifactSurfaceRasterContext {
    let viewport = ctx.viewport();
    crate::view::paint::ArtifactSurfaceRasterContext::new(
        viewport.scale_factor(),
        viewport.target_format(),
        ctx.paint_offset(),
        ctx.graphics_pass_context().logical_scissor_rect(),
        [viewport.target_width(), viewport.target_height()],
        max_texture_dimension_2d,
        max_texture_bytes,
    )
    .expect("production artifact surface raster context is canonical")
}

/// One recorded frame, planned and sealed.
struct RecordedFrame {
    frame: crate::view::paint::PreparedArtifactSurfaceFrame,
    chunk_count: usize,
    op_count: usize,
    /// The nodes left out of the frame, each with its subtree, because their
    /// paint inputs are incomplete.
    left_out: FxHashSet<crate::view::node_arena::NodeKey>,
}

/// Why a recorded artifact did not become a frame.
enum FramePlanError {
    Plan(crate::view::paint::ArtifactSurfaceRasterPlanError),
    Seal(crate::view::paint::ArtifactSurfaceResidentSealError),
}

fn plan_recorded_frame(
    artifact: crate::view::paint::PaintArtifact,
    raster_context: crate::view::paint::ArtifactSurfaceRasterContext,
    planning_cache: Option<&mut crate::view::paint::PlanningCache>,
) -> Result<RecordedFrame, FramePlanError> {
    let chunk_count = artifact.chunks.len();
    let op_count = artifact.ops.len();
    #[cfg(test)]
    attempts::record("raster-plan");
    let plan = match planning_cache {
        Some(cache) => crate::view::paint::prepare_artifact_surface_raster_plan_cached(
            artifact,
            raster_context,
            cache,
        ),
        None => crate::view::paint::prepare_artifact_surface_raster_plan(artifact, raster_context),
    }
    .map_err(FramePlanError::Plan)?;
    #[cfg(test)]
    attempts::record("resident-seal");
    Ok(RecordedFrame {
        frame: crate::view::paint::seal_prepared_artifact_surface_frame(plan)
            .map_err(FramePlanError::Seal)?,
        chunk_count,
        op_count,
        left_out: FxHashSet::default(),
    })
}

/// Records the frame's paint and plans its surfaces. The caches only reuse
/// work: whatever they cannot replay is recorded and planned again. A node
/// whose paint cannot be recorded or planned is left out with its subtree
/// and the rest of the frame still draws; `None` only when the plan fails
/// without naming a node.
fn record_frame(
    arena: &crate::view::node_arena::NodeArena,
    roots: &[crate::view::node_arena::NodeKey],
    property_trees: &crate::view::compositor::PropertyTrees,
    paint_generations: &crate::view::compositor::PaintGenerationTracker,
    raster_context: crate::view::paint::ArtifactSurfaceRasterContext,
    mut recording_cache: Option<&mut crate::view::paint::RecordingCache>,
    mut planning_cache: Option<&mut crate::view::paint::PlanningCache>,
) -> Option<RecordedFrame> {
    #[cfg(test)]
    attempts::record("generic-record");
    let mut left_out = FxHashSet::default();
    let mut artifact = crate::view::paint::record_frame_artifact(
        arena,
        roots,
        property_trees,
        paint_generations,
        recording_cache.as_deref_mut(),
        &mut left_out,
    );
    if let Some(cache) = planning_cache.as_deref_mut() {
        cache.observe_property_changes(property_trees);
        cache.observe_recording_changes(arena);
    }
    loop {
        let error =
            match plan_recorded_frame(artifact, raster_context, planning_cache.as_deref_mut()) {
                Ok(mut recorded) => {
                    recorded.left_out = left_out;
                    return Some(recorded);
                }
                Err(error) => error,
            };
        let owner = match error {
            FramePlanError::Plan(error) => error.owner(),
            FramePlanError::Seal(_) => None,
        };
        // Each pass leaves out one more node, so this ends.
        if !owner.is_some_and(|owner| left_out.insert(owner)) {
            match error {
                FramePlanError::Plan(error) => eprintln!("[warn] frame plan failed: {error:?}"),
                FramePlanError::Seal(error) => eprintln!("[warn] frame seal failed: {error:?}"),
            }
            return None;
        }
        artifact = crate::view::paint::record_frame_artifact(
            arena,
            roots,
            property_trees,
            paint_generations,
            recording_cache.as_deref_mut(),
            &mut left_out,
        );
    }
}

#[cfg(test)]
fn record_frame_for_test(
    arena: &crate::view::node_arena::NodeArena,
    roots: &[crate::view::node_arena::NodeKey],
    property_trees: &crate::view::compositor::PropertyTrees,
    paint_generations: &crate::view::compositor::PaintGenerationTracker,
    ctx: &crate::view::base_component::UiBuildContext,
) -> RecordedFrame {
    record_frame(
        arena,
        roots,
        property_trees,
        paint_generations,
        artifact_surface_raster_context(
            ctx,
            wgpu::Limits::default().max_texture_dimension_2d,
            ARTIFACT_SURFACE_AGGREGATE_BUDGET_BYTES,
        ),
        None,
        None,
    )
    .expect("the test frame plans")
}

/// Emits the recorded frame. Without `owner` the surface cache is unavailable
/// and every surface is drawn without being kept.
fn compile_recorded_frame(
    viewport: &mut Viewport,
    owner: Option<crate::view::viewport::RetainedSurfaceFrameStageOwner>,
    graph: &mut FrameGraph,
    frame: crate::view::paint::PreparedArtifactSurfaceFrame,
    ctx: &crate::view::base_component::UiBuildContext,
) -> crate::view::base_component::BuildState {
    let artifact_ctx =
        crate::view::base_component::UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone());
    crate::view::paint::emit_prepared_artifact_surface_frame_from_pool(
        viewport,
        owner,
        frame,
        graph,
        artifact_ctx,
    )
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ArtifactSurfaceIntermediateReadbackForTest {
    pub(crate) color_key: crate::view::frame_graph::PersistentTextureKey,
    pub(crate) width: u32,
    pub(crate) height: u32,
    /// Destination-space physical coordinate occupied by intermediate texel
    /// `(0, 0)`. This is the same sealed sampling fact consumed by the
    /// composite pass, not a test-side reconstruction.
    pub(crate) source_physical_origin: [f32; 2],
}

#[cfg(test)]
pub(crate) struct AutoArtifactSurfaceEmissionForTest {
    pub(crate) frame_owner: crate::view::viewport::RetainedSurfaceFrameStageOwner,
    pub(crate) surface_count: usize,
    pub(crate) aggregate_texture_bytes: u64,
    pub(crate) actions: Vec<crate::view::paint::RetainedSurfaceCompileAction>,
    pub(crate) intermediate_surfaces: Vec<ArtifactSurfaceIntermediateReadbackForTest>,
}

/// Native Stage C gate seam for both zero-resident and detached artifact
/// surface frames. Recording and dispatch both use the production functions;
/// each named gate owns the surface-count contract for its fixture, while only
/// the compact observations are test-only.
#[cfg(test)]
pub(crate) fn emit_retained_auto_artifact_surface_for_test(
    viewport: &mut Viewport,
    arena: &crate::view::node_arena::NodeArena,
    roots: &[crate::view::node_arena::NodeKey],
    property_trees: &crate::view::compositor::PropertyTrees,
    paint_generations: &crate::view::compositor::PaintGenerationTracker,
    graph: &mut FrameGraph,
    ctx: &crate::view::base_component::UiBuildContext,
) -> Result<AutoArtifactSurfaceEmissionForTest, String> {
    let RecordedFrame { frame, .. } =
        record_frame_for_test(arena, roots, property_trees, paint_generations, ctx);
    let surface_count = frame.raster_plan().nodes().len();
    let aggregate_texture_bytes = frame
        .raster_plan()
        .nodes()
        .iter()
        .try_fold(0_u64, |total, node| {
            let color = crate::view::raster_cost::texture_desc_payload_bytes(&node.target().color);
            let depth = crate::view::raster_cost::texture_desc_payload_bytes(&node.target().depth);
            total
                .checked_add(color.bytes)
                .and_then(|bytes| bytes.checked_add(depth.bytes))
        })
        .ok_or_else(|| "artifact surface descriptor byte total overflowed".to_owned())?;
    let intermediate_surfaces = frame
        .raster_plan()
        .nodes()
        .iter()
        .filter_map(|node| {
            let (color_key, width, height, source_physical_origin) =
                node.intermediate_readback_observation_for_test()?;
            Some(ArtifactSurfaceIntermediateReadbackForTest {
                color_key,
                width,
                height,
                source_physical_origin,
            })
        })
        .collect();
    let owner = viewport
        .begin_retained_surface_frame_stage()
        .ok_or_else(|| "artifact surface frame stage is already active".to_owned())?;
    let _ = crate::view::paint::take_last_production_actions_for_test();
    compile_recorded_frame(viewport, Some(owner), graph, frame, ctx);
    let actions = crate::view::paint::take_last_production_actions_for_test();
    if actions.len() != surface_count {
        return Err(format!(
            "artifact action count drifted: surfaces={surface_count}, actions={}",
            actions.len()
        ));
    }
    Ok(AutoArtifactSurfaceEmissionForTest {
        frame_owner: owner,
        surface_count,
        aggregate_texture_bytes,
        actions,
        intermediate_surfaces,
    })
}

fn finish_frame_dirty_lifecycle(
    arena: &mut crate::view::node_arena::NodeArena,
    root_keys: &[crate::view::node_arena::NodeKey],
    compiled: bool,
    executed: bool,
) {
    if !compiled || !executed {
        return;
    }

    let consumed = crate::view::node_arena::NodeArena::render_consumption_mask();
    for &root_key in root_keys {
        crate::view::viewport::scene_helpers::clear_subtree_dirty_flags_with_arena_dirty(
            arena, root_key, consumed,
        );
    }
}

fn build_layout_pass_trace_children(
    traversal_profile: &super::frame::LayoutTraversalProfile,
    measure_ms: f64,
    measure_children: Vec<TraceRenderNode>,
    place_ms: f64,
    place_profile: &crate::view::base_component::LayoutPlaceProfile,
    invalidate_box_models_ms: f64,
) -> Vec<TraceRenderNode> {
    vec![
        TraceRenderNode::new(
            format!(
                "layout_work (dirty_reads={}, subtree_reuses={}, measure_calls={}, measure_reuses={}, place_calls={}, box_reads={}, box_reuses={})",
                traversal_profile.work.dirty_observations,
                traversal_profile.work.dirty_subtree_reuses,
                traversal_profile.work.measure_calls,
                traversal_profile.work.measure_reuses,
                traversal_profile.work.place_calls,
                traversal_profile.work.box_model_reads,
                traversal_profile.work.box_model_reused_snapshots
            ),
            0.0,
        ),
        TraceRenderNode::new(
            "sync_registered_elements".to_string(),
            traversal_profile.sync_registered_elements_ms,
        ),
        TraceRenderNode::new(
            format!(
                "dirty_refresh_before_measure (roots={})",
                traversal_profile.root_count
            ),
            traversal_profile.dirty_refresh_before_measure_ms,
        ),
        TraceRenderNode::with_children("measure", measure_ms, measure_children),
        TraceRenderNode::new(
            format!(
                "measure_clean_child_candidates (clean={}, dirty={})",
                traversal_profile.measure_candidate_clean_children,
                traversal_profile.measure_dirty_children
            ),
            0.0,
        ),
        TraceRenderNode::new(
            format!(
                "dirty_refresh_before_place (roots={})",
                traversal_profile.root_count
            ),
            traversal_profile.dirty_refresh_before_place_ms,
        ),
        TraceRenderNode::with_children(
            "place",
            place_ms,
            build_layout_place_trace_nodes(place_profile),
        ),
        TraceRenderNode::new(
            format!(
                "placement_clean_child_candidates (clean={}, dirty={})",
                traversal_profile.placement_candidate_clean_children,
                traversal_profile.placement_dirty_children
            ),
            0.0,
        ),
        TraceRenderNode::new(
            format!(
                "skipped_child_place_calls (count={})",
                traversal_profile.skipped_child_place_calls
            ),
            0.0,
        ),
        TraceRenderNode::new(
            format!(
                "invalidate_box_models (roots={})",
                traversal_profile.root_count
            ),
            invalidate_box_models_ms,
        ),
    ]
}

impl Viewport {
    /// Run a single layout pass: measure → place → invalidate_box_models.
    /// Returns profiling data for the pass.
    #[cfg(test)]
    pub(super) fn run_layout_pass(&mut self) -> LayoutPassResult {
        self.run_layout_pass_with_registered_sync(true)
    }

    /// A transition-triggered second layout belongs to the same rendered
    /// frame. Resource-backed hosts were already frozen by the first pass, so
    /// repeating the arena sync here could mix two async resource generations
    /// (and even two child-slot topologies) in one frame.
    fn run_relayout_pass(&mut self) -> LayoutPassResult {
        self.run_layout_pass_with_registered_sync(false)
    }

    fn run_layout_pass_with_registered_sync(
        &mut self,
        sync_registered_elements: bool,
    ) -> LayoutPassResult {
        let work_before = crate::ui::work_profile::snapshot();
        self.compositor.frame_box_models.take();
        crate::view::base_component::reset_text_measure_profile();
        crate::view::base_component::reset_layout_place_profile();
        crate::view::base_component::reset_layout_gate_candidate_profile();

        // Take the arena out of the scene so we can pass it by &mut into
        // layout without aliasing the viewport; restore at the end.
        let mut arena = std::mem::take(&mut self.scene.node_arena);
        let root_keys = self.scene.ui_root_keys.clone();
        let mut traversal_profile = super::frame::LayoutTraversalProfile {
            root_count: root_keys.len(),
            ..Default::default()
        };

        let measure_started_at = Instant::now();
        let constraints = crate::view::base_component::LayoutConstraints {
            max_width: self.logical_width,
            max_height: self.logical_height,
            viewport_width: self.logical_width,
            viewport_height: self.logical_height,
            percent_base_width: Some(self.logical_width),
            percent_base_height: Some(self.logical_height),
        };
        // Flush deferred arena mutations for explicitly registered hosts.
        // Must run before measure so layout sees their current arena state.
        if sync_registered_elements {
            let sync_registered_elements_started_at = Instant::now();
            arena.sync_registered_elements();
            traversal_profile.sync_registered_elements_ms =
                sync_registered_elements_started_at.elapsed().as_secs_f64() * 1000.0;
        }
        // Refresh the per-node subtree-dirty cache once at the top of the
        // measure pass so every Element::measure / place can read
        // subtree_dirty_flags via an O(1) cache lookup instead of walking
        // its entire subtree (an O(N²) trap pre-cache).
        let dirty_refresh_before_measure_started_at = Instant::now();
        let measure_dirty_roots = root_keys
            .iter()
            .map(|&root_key| {
                arena
                    .refresh_subtree_dirty_cache(root_key)
                    .intersects(crate::view::base_component::DirtyFlags::LAYOUT)
            })
            .collect::<Vec<_>>();
        traversal_profile.dirty_refresh_before_measure_ms = dirty_refresh_before_measure_started_at
            .elapsed()
            .as_secs_f64()
            * 1000.0;
        let measure_roots_started_at = Instant::now();
        for &root_key in &root_keys {
            if arena.get(root_key).is_some_and(|node| {
                node.element
                    .as_any()
                    .downcast_ref::<crate::view::base_component::Element>()
                    .is_some_and(|element| element.measure_is_noop(constraints, &arena))
            }) {
                continue;
            }
            arena.with_element_taken(root_key, |root, arena| {
                root.measure(constraints, arena);
            });
        }
        for (&root_key, was_layout_dirty) in root_keys.iter().zip(measure_dirty_roots) {
            if was_layout_dirty {
                arena.clear_cached_arena_dirty_subtree(
                    root_key,
                    crate::view::base_component::DirtyFlags::LAYOUT,
                );
            }
        }
        traversal_profile.measure_roots_ms =
            measure_roots_started_at.elapsed().as_secs_f64() * 1000.0;
        let measure_ms = measure_started_at.elapsed().as_secs_f64() * 1000.0;
        let text_measure_profile = crate::view::base_component::take_text_measure_profile();

        let place_started_at = Instant::now();
        let placement = crate::view::base_component::LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: self.logical_width,
            available_height: self.logical_height,
            viewport_width: self.logical_width,
            viewport_height: self.logical_height,
            percent_base_width: Some(self.logical_width),
            percent_base_height: Some(self.logical_height),
        };
        // Measure mutated per-node dirty bits, so refresh the cache again
        // before place so `Element::place` can read it in O(1).
        let dirty_refresh_before_place_started_at = Instant::now();
        let place_dirty_roots = root_keys
            .iter()
            .map(|&root_key| {
                arena
                    .refresh_subtree_dirty_cache(root_key)
                    .intersects(crate::view::base_component::DirtyFlags::PLACE)
            })
            .collect::<Vec<_>>();
        traversal_profile.dirty_refresh_before_place_ms = dirty_refresh_before_place_started_at
            .elapsed()
            .as_secs_f64()
            * 1000.0;
        let place_roots_started_at = Instant::now();
        for &root_key in &root_keys {
            if arena.get(root_key).is_some_and(|node| {
                node.element
                    .as_any()
                    .downcast_ref::<crate::view::base_component::Element>()
                    .is_some_and(|element| element.place_is_noop(placement, &arena))
            }) {
                continue;
            }
            arena.with_element_taken(root_key, |root, arena| {
                root.place(placement, arena);
            });
        }
        for (&root_key, was_place_dirty) in root_keys.iter().zip(place_dirty_roots) {
            if was_place_dirty {
                arena.clear_cached_arena_dirty_subtree(
                    root_key,
                    crate::view::base_component::DirtyFlags::PLACE,
                );
            }
        }
        traversal_profile.place_roots_ms = place_roots_started_at.elapsed().as_secs_f64() * 1000.0;
        let place_ms = place_started_at.elapsed().as_secs_f64() * 1000.0;
        let gate_profile = crate::view::base_component::take_layout_gate_candidate_profile();
        traversal_profile.measure_candidate_clean_children =
            gate_profile.measure_candidate_clean_children;
        traversal_profile.measure_dirty_children = gate_profile.measure_dirty_children;
        traversal_profile.placement_candidate_clean_children =
            gate_profile.placement_candidate_clean_children;
        traversal_profile.placement_dirty_children = gate_profile.placement_dirty_children;

        self.scene.node_arena = arena;
        let collect_started_at = Instant::now();
        self.invalidate_frame_box_models();
        let invalidate_box_models_ms = collect_started_at.elapsed().as_secs_f64() * 1000.0;
        traversal_profile.invalidate_box_models_ms = invalidate_box_models_ms;
        let place_profile = crate::view::base_component::take_layout_place_profile();
        traversal_profile.skipped_child_place_calls = place_profile.skipped_child_place_calls;

        traversal_profile.work = crate::ui::work_profile::snapshot().since(work_before);

        LayoutPassResult {
            measure_ms,
            place_ms,
            invalidate_box_models_ms,
            traversal_profile,
            text_measure_profile,
            place_profile,
        }
    }

    /// Outlines each surface's owner node, colored by what the surface cache
    /// did with it: green reuse, orange reraster, red transient. Labels stack
    /// when one node owns several surfaces.
    fn push_retained_auto_debug_overlay(&mut self) {
        use crate::view::paint::RetainedSurfaceCompileAction as Action;
        if !self.debug_options.retained_auto_overlay {
            return;
        }
        let scale = self.scale_factor.max(0.0001);
        let screen_w = self.gpu.surface_config.width.max(1) as f32;
        let screen_h = self.gpu.surface_config.height.max(1) as f32;
        let mut labels_per_owner = FxHashMap::default();
        let mut geometry = Vec::new();
        for entry in &self.frame.surface_diagnostics {
            let (shown, color) = match entry.action {
                Action::Reuse => (
                    self.debug_options.retained_auto_show_reuse,
                    [38.0 / 255.0, 242.0 / 255.0, 90.0 / 255.0, 242.0 / 255.0],
                ),
                Action::Reraster => (
                    self.debug_options.retained_auto_show_reraster,
                    [1.0, 115.0 / 255.0, 26.0 / 255.0, 242.0 / 255.0],
                ),
                Action::Transient => (
                    self.debug_options.retained_auto_show_transient,
                    [1.0, 59.0 / 255.0, 48.0 / 255.0, 242.0 / 255.0],
                ),
            };
            if !shown {
                continue;
            }
            let Some(surface) = self.retained_auto_debug_surface(entry) else {
                continue;
            };
            let Some(node) = self.scene.node_arena.get(entry.owner) else {
                continue;
            };
            let snapshot = node.element.box_model_snapshot();
            if !snapshot.should_render {
                continue;
            }
            let row = labels_per_owner.entry(entry.owner).or_insert(0_usize);
            geometry.push(build_debug_overlay_geometry(
                &snapshot,
                scale,
                screen_w,
                screen_h,
                color,
                Some((&surface.label(), *row)),
            ));
            *row += 1;
        }
        for (vertices, indices) in geometry {
            self.push_debug_overlay_geometry(&vertices, &indices);
        }
    }

    /// One recorded surface resolved against its owner node.
    fn retained_auto_debug_surface(
        &self,
        entry: &SurfaceDiagnostic,
    ) -> Option<crate::view::debug::DebugRetainedAutoSurfaceSnapshot> {
        let input = self.retained_auto_debug_surface_input(entry)?;
        Some(crate::view::debug::DebugRetainedAutoSurfaceSnapshot {
            owner: None,
            stable_id: input.stable_id,
            element_type: input.element_type,
            bounds: input.bounds,
            kind: input.kind,
            action: input.action,
            size: input.size,
            scale_factor: input.scale_factor,
        })
    }

    fn retained_auto_debug_surface_input(
        &self,
        entry: &SurfaceDiagnostic,
    ) -> Option<crate::view::debug::DebugRetainedAutoSurfaceCaptureInput> {
        use crate::view::debug::DebugSurfaceAction;
        use crate::view::paint::RetainedSurfaceCompileAction as Action;
        let node = self.scene.node_arena.get(entry.owner)?;
        let bounds = node.element.box_model_snapshot();
        Some(crate::view::debug::DebugRetainedAutoSurfaceCaptureInput {
            owner: Some(entry.owner),
            stable_id: Some(node.element.stable_id()),
            element_type: node.element.element_type_name(),
            bounds: Some(crate::view::debug::DebugRect {
                x: bounds.x,
                y: bounds.y,
                width: bounds.width,
                height: bounds.height,
            }),
            kind: entry.kind,
            action: match entry.action {
                Action::Reuse => DebugSurfaceAction::Reuse,
                Action::Reraster => DebugSurfaceAction::Reraster,
                Action::Transient => DebugSurfaceAction::Transient,
            },
            size: (entry.desc.width(), entry.desc.height()),
            scale_factor: entry.scale_factor,
        })
    }

    fn build_retained_auto_debug_capture(
        &self,
        presented: bool,
        paint_chunks: usize,
        paint_ops: usize,
    ) -> crate::view::debug::DebugRetainedAutoCaptureInput {
        use crate::view::debug::{DebugFrameDisposition, DebugSurfaceAction, DebugSurfaceKind};
        let surfaces = self
            .frame
            .surface_diagnostics
            .iter()
            .filter_map(|entry| self.retained_auto_debug_surface_input(entry))
            .collect::<Vec<_>>();
        let count =
            |keep: &dyn Fn(&crate::view::debug::DebugRetainedAutoSurfaceCaptureInput) -> bool| {
                surfaces.iter().filter(|surface| keep(surface)).count() as u64
            };
        let statistics = crate::view::debug::DebugRetainedAutoStatistics {
            paint_chunks: paint_chunks as u64,
            paint_ops: paint_ops as u64,
            surfaces: count(&|surface| surface.kind != DebugSurfaceKind::GpuSource),
            gpu_sources: count(&|surface| surface.kind == DebugSurfaceKind::GpuSource),
            reuses: count(&|surface| surface.action == DebugSurfaceAction::Reuse),
            rerasterizations: count(&|surface| surface.action == DebugSurfaceAction::Reraster),
            transients: count(&|surface| surface.action == DebugSurfaceAction::Transient),
            transform_nodes: self.compositor.property_trees.transforms.len() as u64,
            effect_nodes: self.compositor.property_trees.effects.len() as u64,
            scroll_nodes: self.compositor.property_trees.scrolls.len() as u64,
        };
        crate::view::debug::DebugRetainedAutoCaptureInput {
            frame: crate::view::debug::DebugRetainedAutoFrameSnapshot {
                frame_number: self.frame.frame_number,
                disposition: if presented {
                    DebugFrameDisposition::Presented
                } else {
                    DebugFrameDisposition::Aborted
                },
                statistics,
            },
            surfaces,
        }
    }

    /// Asks for a text report of the next frame's surfaces, printed once that
    /// frame finishes. Bound to F9 by the example runner.
    pub fn request_retained_auto_report(&mut self) {
        self.frame.retained_auto_report_pending = true;
        self.request_redraw();
    }

    /// Build the hierarchical trace tree from collected frame timings.
    fn build_frame_trace_tree(t: &FrameTimings, opts: &ViewportDebugOptions) -> TraceRenderNode {
        let any_detail =
            opts.trace_layout_detail || opts.trace_compile_detail || opts.trace_execute_detail;
        let layout_with_transition_ms = t.layout_total_ms;

        // --- begin_frame (expand when any detail flag is on) ---
        let begin_frame = if any_detail {
            TraceRenderNode::with_children(
                "begin_frame",
                t.begin_frame_ms,
                vec![
                    TraceRenderNode::new("create_command_encoder", t.begin_frame_create_encoder_ms),
                ],
            )
        } else {
            TraceRenderNode::new("begin_frame", t.begin_frame_ms)
        };

        // --- layout ---
        let layout = if opts.trace_layout_detail {
            let layout_measure_children =
                build_text_measure_trace_nodes(&t.layout_text_measure_profile);
            let layout_traversal_children = build_layout_pass_trace_children(
                &t.layout_traversal_profile,
                t.layout_measure_ms,
                layout_measure_children,
                t.layout_place_ms,
                &t.layout_place_profile,
                t.layout_invalidate_box_models_ms,
            );
            let relayout_traversal_children = build_layout_pass_trace_children(
                &t.relayout_traversal_profile,
                t.relayout_measure_ms,
                Vec::new(),
                t.relayout_place_ms,
                &t.relayout_place_profile,
                t.relayout_invalidate_box_models_ms,
            );
            TraceRenderNode::with_children(
                "layout",
                layout_with_transition_ms,
                vec![
                    TraceRenderNode::with_children(
                        "layout_traversal",
                        t.layout_ms,
                        layout_traversal_children,
                    ),
                    TraceRenderNode::new("post_layout_transition", t.post_layout_transition_ms),
                    TraceRenderNode::with_children(
                        "relayout_after_transition",
                        t.relayout_ms,
                        vec![TraceRenderNode::with_children(
                            "layout_traversal",
                            t.relayout_measure_ms
                                + t.relayout_place_ms
                                + t.relayout_invalidate_box_models_ms,
                            relayout_traversal_children,
                        )],
                    ),
                ],
            )
        } else {
            TraceRenderNode::new("layout", layout_with_transition_ms)
        };

        // --- compile ---
        let compile = if opts.trace_compile_detail {
            TraceRenderNode::with_children("compile", t.compile_ms, t.compile_children.clone())
        } else {
            TraceRenderNode::new("compile", t.compile_ms)
        };

        // --- execute ---
        let execute = if opts.trace_execute_detail {
            let mut execute_children = if t.execute_ordered_passes.is_empty() {
                vec![TraceRenderNode::new(
                    format!("passes ({})", t.execute_pass_count),
                    0.0,
                )]
            } else {
                build_execute_detail_trace_nodes(t.execute_ordered_passes.clone())
            };
            execute_children.push(TraceRenderNode::new(
                "acquire_surface_texture", t.surface_acquire_ms,
            ));
            execute_children.push(TraceRenderNode::new(
                "create_surface_view", t.surface_create_view_ms,
            ));
            if !t.execute_detail_ordered_passes.is_empty() {
                let detail_total_ms: f64 = t
                    .execute_detail_ordered_passes
                    .iter()
                    .map(|(_, ms, _)| *ms)
                    .sum();
                let detail_children =
                    build_execute_detail_trace_nodes(t.execute_detail_ordered_passes.clone());
                execute_children.push(TraceRenderNode::with_children(
                    "execute_detail",
                    detail_total_ms,
                    detail_children,
                ));
            }
            TraceRenderNode::with_children(
                format!("execute (passes={})", t.execute_pass_count),
                t.execute_ms,
                t.execute_profile_ms
                    .map(|ms| {
                        vec![TraceRenderNode::with_children(
                            "execute_graph",
                            ms,
                            execute_children,
                        )]
                    })
                    .unwrap_or_default(),
            )
        } else {
            TraceRenderNode::new(
                format!("execute (passes={})", t.execute_pass_count),
                t.execute_ms,
            )
        };

        // --- end_frame (expand when any detail flag is on) ---
        let end_frame = if any_detail {
            TraceRenderNode::with_children(
                "end_frame",
                t.end_frame_ms,
                vec![
                    TraceRenderNode::new("staging_finish", t.end_frame.staging_finish_ms),
                    TraceRenderNode::new("encoder_finish", t.end_frame.encoder_finish_ms),
                    TraceRenderNode::new("queue_submit", t.end_frame.submit_ms),
                    TraceRenderNode::new("resource_cleanup", t.end_frame.resource_cleanup_ms),
                    TraceRenderNode::new("present", t.end_frame.present_ms),
                    TraceRenderNode::new(
                        format!("gpu_wait (waited={})", t.end_frame.gpu_waited),
                        t.end_frame.gpu_wait_ms,
                    ),
                    TraceRenderNode::new("abort_cleanup", t.end_frame.abort_cleanup_ms),
                ],
            )
        } else {
            TraceRenderNode::new("end_frame", t.end_frame_ms)
        };

        // Each first-level phase spans consecutive wall-clock boundaries,
        // including caller bookkeeping and failed attempts. Nested profiles
        // retain their narrower diagnostic scopes. Front-end intervals are
        // outside the renderer-only total_ms; unwrap nests inside RSX build.
        TraceRenderNode::with_children(
            format!("render_frame #{}", t.frame_number),
            t.frontend.total_ms() + t.total_ms,
            vec![
                TraceRenderNode::new(
                    format!(
                        "state_frame targets={} actions={} changed={} memo_visits={} memo_invalidated={}",
                        t.frontend.work.state_targets,
                        t.frontend.work.state_actions,
                        t.frontend.work.changed_targets,
                        t.frontend.work.memo_invalidation_visits,
                        t.frontend.work.memo_invalidations
                    ),
                    t.frontend.state_flush_ms,
                ),
                TraceRenderNode::with_children(
                    "rsx_build",
                    t.frontend.rsx_build_ms,
                    vec![TraceRenderNode::new(
                        format!(
                            "unwrap nodes={} components={} memo_hits={}",
                            t.frontend.work.unwrap_nodes,
                            t.frontend.work.component_renders,
                            t.frontend.work.memo_hits
                        ),
                        t.frontend.work.unwrap_ms,
                    )],
                ),
                TraceRenderNode::with_children(
                    "scene_update",
                    t.frontend.scene_update_ms,
                    vec![
                        TraceRenderNode::new(
                            format!(
                                "reconcile calls={} nodes={} shared={} patches={}",
                                t.frontend.work.reconcile_calls,
                                t.frontend.work.reconciled_nodes,
                                t.frontend.work.shared_subtree_hits,
                                t.frontend.work.patches
                            ),
                            t.frontend.work.reconcile_ms,
                        ),
                        TraceRenderNode::new("placement", t.frontend.work.placement_ms),
                        TraceRenderNode::new(
                            format!("translate works={}", t.frontend.work.fiber_works),
                            t.frontend.work.translate_ms,
                        ),
                        TraceRenderNode::new(
                            format!(
                                "incremental_commit scroll_save={} scroll_restore={}",
                                t.frontend.work.scroll_save_nodes,
                                t.frontend.work.scroll_restore_nodes
                            ),
                            t.frontend.work.incremental_commit_ms,
                        ),
                        TraceRenderNode::new("cold_commit", t.frontend.work.cold_commit_ms),
                    ],
                ),
                begin_frame,
                layout,
                TraceRenderNode::new("prepare_paint", t.prepare_paint_ms),
                TraceRenderNode::new("sync_properties", t.sync_properties_ms),
                TraceRenderNode::with_children(
                    "build_graph",
                    t.build_graph_ms,
                    vec![TraceRenderNode::new("paint_select", t.paint_select_ms)],
                ),
                compile,
                execute,
                TraceRenderNode::new("finish_render", t.finish_render_ms),
                end_frame,
            ],
        )
    }

    fn render_render_tree(
        &mut self,
        dt: f32,
        now_seconds: f64,
        semantic_now: crate::time::Instant,
        frontend_changed: bool,
    ) -> bool {
        // Registered hosts may hold deferred arena edits. Observe them before
        // proving the tree clean, and do not sync them again during layout.
        self.scene.node_arena.sync_registered_elements();
        let dirty = self.scene.ui_root_keys.iter().any(|&key| {
            !self
                .scene
                .node_arena
                .refresh_subtree_dirty_cache(key)
                .is_empty()
        });
        use crate::view::base_component::AnimationFrameRequest;
        // The preceding final-state request already owns this wake-up. New
        // state/scene/resource changes independently invalidate admission.
        // Resample only after this frame's final layout/resource preparation,
        // so scrollbar hold/fade and caret deadlines use the finished state.
        let animation_due = match self.scheduled_animation_request {
            AnimationFrameRequest::None => false,
            AnimationFrameRequest::NextFrame => true,
            AnimationFrameRequest::At(at) => at <= semantic_now,
        };
        if !frontend_changed
            && !dirty
            && !animation_due
            && !self.frame.render_required
            && !self.needs_reconfigure
            && self.frame.frame_state.is_none()
            && self.transitions.transition_claims.is_empty()
            && !crate::view::base_component::transition_requests_pending()
        {
            return false;
        }
        // Keep consumed frontend/resource invalidation alive across failed
        // acquisition or execution. Only submission discharges this obligation.
        self.frame.render_required = true;
        // Profiling is deliberately a separate clock read. It may only feed
        // elapsed-time diagnostics; retained frame semantics use the sample
        // captured once by `render_rsx`.
        let profile_start = Instant::now();
        let mut phase_clock = super::frame::FramePhaseClock::new(profile_start);
        self.frame.frame_number = self.frame.frame_number.saturating_add(1);
        let frame_number = self.frame.frame_number;
        let begin_frame_profile = match self.begin_frame() {
            Some(profile) => profile,
            None => {
                return false;
            }
        };

        let mut timings = FrameTimings {
            begin_frame_ms: phase_clock.checkpoint_ms(),
            begin_frame_create_encoder_ms: begin_frame_profile.create_encoder_ms,
            frontend: self.frame.frontend,
            frame_number,
            ..Default::default()
        };

        // --- Layout ---
        crate::view::base_component::set_text_measure_profile_enabled(
            self.debug_options.trace_render_time,
        );
        let diagnostic_enabled = {
            #[cfg(feature = "renderer-test-support")]
            {
                self.frame.diagnostics_enabled
            }
            #[cfg(not(feature = "renderer-test-support"))]
            {
                false
            }
        };
        let _layout_profile = crate::view::base_component::enable_layout_profile_scoped(
            self.debug_options.trace_render_time || diagnostic_enabled,
        );
        let layout_started_at = Instant::now();
        let layout_result = self.run_layout_pass_with_registered_sync(false);
        timings.layout_measure_ms = layout_result.measure_ms;
        timings.layout_place_ms = layout_result.place_ms;
        timings.layout_invalidate_box_models_ms = layout_result.invalidate_box_models_ms;
        timings.layout_traversal_profile = layout_result.traversal_profile;
        timings.layout_text_measure_profile = layout_result.text_measure_profile;
        timings.layout_place_profile = layout_result.place_profile;
        timings.layout_ms = layout_started_at.elapsed().as_secs_f64() * 1000.0;

        // After layout is resolved for this frame, immediately run visual/style/scroll transitions
        // so their updated endpoints are visible in the same frame.
        let post_layout_transition_started_at = Instant::now();
        let post_layout_transition = self.run_post_layout_transitions(dt, now_seconds);
        timings.post_layout_transition_ms =
            post_layout_transition_started_at.elapsed().as_secs_f64() * 1000.0;

        // --- Relayout after transition (if needed) ---
        let relayout_started_at = Instant::now();
        if post_layout_transition.relayout_required {
            let relayout_result = self.run_relayout_pass();
            timings.relayout_measure_ms = relayout_result.measure_ms;
            timings.relayout_place_ms = relayout_result.place_ms;
            timings.relayout_invalidate_box_models_ms = relayout_result.invalidate_box_models_ms;
            timings.relayout_traversal_profile = relayout_result.traversal_profile;
            timings.relayout_place_profile = relayout_result.place_profile;
        }
        timings.relayout_ms = relayout_started_at.elapsed().as_secs_f64() * 1000.0;

        timings.layout_total_ms = phase_clock.checkpoint_ms();
        // Layout-affecting transitions (scroll, layout) can move elements
        // under a stationary pointer. Hover is hit-tested again once this
        // frame is painted, so PointerEnter/PointerLeave fire without a real
        // PointerMove. This frame's style transitions have already run: a
        // hover style applied now would paint its end value for one frame
        // before its transition restarts from the old value.
        if post_layout_transition.relayout_required {
            self.frame.pointer_hover_resync_pending = true;
        }

        // Scrollbar visibility depends on final scroll geometry. Resolve it
        // only after layout/relayout, using the viewport entry's sole
        // semantic time sample, before any property or paint observation.
        let post_layout_animation_changed = {
            let mut arena = std::mem::take(&mut self.scene.node_arena);
            let root_keys = self.scene.ui_root_keys.clone();
            let changed = crate::view::base_component::tick_post_layout_animation_frames(
                &mut arena,
                &root_keys,
                semantic_now,
            );
            self.scene.node_arena = arena;
            changed
        };

        // Final layout is now stable. Freeze resource-backed paint payloads
        // exactly once for this frame before property-tree observation and
        // paint recording. This pass cannot mutate arena
        // topology; slot changes caused by async completion wait until the
        // next frame's pre-layout sync.
        self.scene.node_arena.prepare_registered_paint_resources(
            crate::view::base_component::PaintResourcePreparationContext {
                frame_number,
                device_scale: self.scale_factor,
                now: semantic_now,
            },
        );

        #[cfg(test)]
        single_viewport_frame_test_support::run_after_resource_freeze(self);
        self.frame.surface_diagnostics.clear();
        self.prune_gpu_paint_sources();
        timings.prepare_paint_ms = phase_clock.checkpoint_ms();

        // Freeze the final state after transition sampling and relayout.
        // Property changes survive layout dirty consumption and select CPU
        // planning work; complete recorded inputs still certify cache reuse.
        self.sync_compositor_property_trees();
        let render_changes = self.scene.node_arena.capture_render_changes();
        timings.sync_properties_ms = phase_clock.checkpoint_ms();

        // --- Build frame graph ---
        crate::view::paint::work_profile::begin();
        self.clear_debug_overlay_geometry();
        let mut graph = FrameGraph::new();
        graph.set_max_texture_dimension_2d(self.max_texture_dimension_2d());
        graph.set_resident_shadow_templates(self.resident_shadow_templates());
        let mut ctx = crate::view::base_component::UiBuildContext::new(
            self.gpu.surface_config.width,
            self.gpu.surface_config.height,
            self.offscreen_format(),
            self.scale_factor,
        );
        let retained_surface_frame_owner = self.begin_retained_surface_frame_stage();
        let root_keys_for_build = self.scene.ui_root_keys.clone();
        self.scene
            .node_arena
            .sync_popup_stack(&mut self.scene.popup_stack);
        let paint_select_started = self.debug_options.trace_render_time.then(Instant::now);
        let raster_context = artifact_surface_raster_context(
            &ctx,
            self.max_texture_dimension_2d(),
            ARTIFACT_SURFACE_AGGREGATE_BUDGET_BYTES,
        );
        let recorded = record_frame(
            &self.scene.node_arena,
            &root_keys_for_build,
            &self.compositor.property_trees,
            &self.compositor.paint_generations,
            raster_context,
            Some(&mut self.compositor.recording_cache),
            Some(&mut self.compositor.planning_cache),
        );
        if let Some(started) = paint_select_started {
            timings.paint_select_ms = started.elapsed().as_secs_f64() * 1000.0;
        }
        if let Some(recorded) = &recorded {
            self.report_left_out_paint(&recorded.left_out);
        }
        let (recorded_chunks, recorded_ops) = recorded
            .as_ref()
            .map_or((0, 0), |recorded| (recorded.chunk_count, recorded.op_count));
        let clear_uses_premultiplied_alpha = matches!(
            self.gpu.surface_config.alpha_mode,
            wgpu::CompositeAlphaMode::PostMultiplied | wgpu::CompositeAlphaMode::PreMultiplied
        );
        let mut clear_rgba = self.clear_color.to_rgba_f32();
        if clear_uses_premultiplied_alpha {
            let a = clear_rgba[3].clamp(0.0, 1.0);
            clear_rgba[0] *= a;
            clear_rgba[1] *= a;
            clear_rgba[2] *= a;
            clear_rgba[3] = a;
        }
        {
            let output = ctx.allocate_target(&mut graph);
            let output_handle = output.handle();
            ctx.set_current_target(output.clone());
            let clear_pass = crate::view::frame_graph::ClearPass::new(
                crate::view::render_pass::clear_pass::ClearParams::new(clear_rgba),
                crate::view::render_pass::clear_pass::ClearInput {
                    pass_context: ctx.graphics_pass_context(),
                    clear_depth_stencil: true,
                },
                crate::view::render_pass::clear_pass::ClearOutput {
                    render_target: output.clone(),
                    ..Default::default()
                },
            );
            if let Some(handle) = output_handle {
                ctx.set_color_target(Some(handle));
            }
            graph.add_graphics_pass(clear_pass);
            ctx.set_current_target(output);
        }
        // A missing owner means another transaction owns the slot: draw
        // without the surface cache. A frame whose plan failed is aborted
        // below, and the next frame records again.
        let frame_planned = recorded.is_some();
        if let Some(recorded) = recorded {
            let state = compile_recorded_frame(
                self,
                retained_surface_frame_owner,
                &mut graph,
                recorded.frame,
                &ctx,
            );
            ctx.set_state(state);
        }
        self.push_retained_auto_debug_overlay();
        let dependency_handle = ctx.current_target().and_then(|target| target.handle());
        if let Some(dep_handle) = dependency_handle {
            let present_pass =
                crate::view::render_pass::present_surface_pass::PresentSurfacePass::new(
                    crate::view::render_pass::present_surface_pass::PresentSurfaceParams,
                    crate::view::render_pass::present_surface_pass::PresentSurfaceInput {
                        source:
                            crate::view::render_pass::draw_rect_pass::RenderTargetIn::with_handle(
                                dep_handle,
                            ),
                        ..Default::default()
                    },
                    crate::view::render_pass::present_surface_pass::PresentSurfaceOutput::default(),
                );
            let present_handle = graph.add_graphics_pass(present_pass);
            graph
                .add_pass_sink(
                    present_handle,
                    crate::view::frame_graph::ExternalSinkKind::SurfacePresent,
                )
                .expect("surface present sink should register");
        }
        crate::view::paint::work_profile::count(
            "owner_scope_replays",
            self.compositor.recording_cache.scope_hits,
        );
        crate::view::paint::work_profile::count(
            "scope_store_replays",
            self.compositor.recording_cache.scope_store_hits,
        );
        crate::view::paint::work_profile::count(
            "scope_store_effect_updates",
            self.compositor.recording_cache.scope_store_effect_updates,
        );
        crate::view::paint::work_profile::count(
            "property_closure_replays",
            self.compositor.recording_cache.property_closure_hits,
        );
        crate::view::paint::work_profile::count(
            "record_replays",
            self.compositor.recording_cache.hits,
        );
        crate::view::paint::work_profile::count(
            "record_misses",
            self.compositor.recording_cache.misses,
        );
        crate::view::paint::work_profile::count(
            "graph_replays",
            self.compositor.planning_cache.graph_hits,
        );
        crate::view::paint::work_profile::count(
            "coverage_replays",
            self.compositor.planning_cache.coverage_hits,
        );
        crate::view::paint::work_profile::count(
            "placement_replays",
            self.compositor.planning_cache.placement_hits,
        );
        crate::view::paint::work_profile::count(
            "raster_span_replays",
            self.compositor.planning_cache.raster_span_hits(),
        );
        crate::view::paint::work_profile::count(
            "geometry_replays",
            self.compositor.planning_cache.geometry_hits,
        );
        crate::view::paint::work_profile::count(
            "localized_replays",
            self.compositor.planning_cache.localized_hits,
        );
        crate::view::paint::work_profile::count(
            "localized_misses",
            self.compositor.planning_cache.localized_misses,
        );
        timings.build_graph_ms = phase_clock.checkpoint_ms();
        crate::view::paint::work_profile::finish(frame_number as u64, timings.build_graph_ms);

        // --- Compile ---
        // Take the cache out (moves ownership) so we can pass self mutably to compile.
        // On cache hit the graph is reused in-place; on miss it is dropped. Either way
        // the returned compiled_graph is stored back for the next frame.
        let prior_cache = self
            .frame
            .compile_cache
            .take()
            .map(|c| (c.topology_key, c.graph));
        let mut compiled_topology_key = None;
        let compiled = frame_planned
            && match graph.compile_with_upload_cached(self, prior_cache) {
                Ok((profile, topology_key)) => {
                    if self.debug_options.trace_render_time {
                        timings.compile_children = build_compile_trace_nodes(
                            &profile,
                            self.debug_options.trace_compile_detail,
                        );
                    }
                    compiled_topology_key = Some(topology_key);
                    true
                }
                Err(err) => {
                    eprintln!("[warn] frame graph compile failed: {:?}", err);
                    // compile_cache already cleared by take() above
                    false
                }
            };

        // Include cache transfer, diagnostic construction and errors, even
        // when compilation fails without returning an internal profile.
        timings.compile_ms = phase_clock.checkpoint_ms();

        // --- Execute ---
        let mut executed = false;
        if compiled {
            match graph.execute_profiled(self, self.debug_options.trace_render_time) {
                Ok(profile) => {
                    timings.execute_profile_ms = Some(profile.total_ms);
                    timings.execute_pass_count = profile.pass_count;
                    timings.execute_ordered_passes = profile.ordered_passes;
                    timings.execute_detail_ordered_passes = profile.detail_ordered;
                    executed = true;
                }
                Err(crate::view::frame_graph::FrameGraphError::SurfaceUnavailable) => {}
                Err(error) => {
                    eprintln!("[warn] frame graph execution failed: {error:?}");
                }
            }
        }
        // Failed execution still consumes this phase; a missing profile
        // must not turn the time spent into a zero-duration attempt.
        timings.execute_ms = phase_clock.checkpoint_ms();
        if let Some(frame) = &self.frame.frame_state {
            timings.surface_acquire_ms = frame.surface_acquire_ms;
            timings.surface_create_view_ms = frame.surface_create_view_ms;
        }
        let root_keys = self.scene.ui_root_keys.clone();
        finish_frame_dirty_lifecycle(&mut self.scene.node_arena, &root_keys, compiled, executed);
        if compiled && executed {
            self.scene.node_arena.commit_render_changes(render_changes);
        }
        self.finish_retained_surface_transaction_for_frame(
            retained_surface_frame_owner,
            compiled && executed,
        );
        if self.surface_diagnostics_requested() {
            self.frame.last_retained_auto_debug = Some(self.build_retained_auto_debug_capture(
                compiled && executed,
                recorded_chunks,
                recorded_ops,
            ));
        }
        if std::mem::take(&mut self.frame.retained_auto_report_pending) {
            let capture = self.capture_debug(crate::view::debug::DebugCaptureOptions {
                include_retained_auto: true,
                ..Default::default()
            });
            if let Some(report) = &capture.document().viewport.retained_auto {
                print!("{report}");
            }
        }

        // Never retain topology from a terminal frame. In particular, an
        // execute failure has a compiled graph but it is not a successful
        // cross-frame cache seed; the next frame must retry cleanly.
        self.frame.compile_cache = None;
        if should_store_compile_cache(compiled, executed)
            && let Some(topology_key) = compiled_topology_key
        {
            if let Some(compiled_graph) = graph.take_compiled_graph() {
                self.frame.compile_cache = Some(CachedCompiledGraph {
                    topology_key,
                    graph: compiled_graph,
                });
            }
        }

        timings.finish_render_ms = phase_clock.checkpoint_ms();

        // --- Complete frame ---
        // Transaction rollback above must settle before the acquired frame is
        // either submitted or discarded. A terminal compile/execute failure
        // never submits a partially recorded encoder and never presents its
        // surface image.
        let end_frame_profile = self.complete_frame(frame_disposition(compiled, executed));
        timings.end_frame_ms = phase_clock.checkpoint_ms();
        timings.end_frame = end_frame_profile;
        timings.total_ms = phase_clock.total_ms();
        #[cfg(any(test, feature = "renderer-test-support"))]
        {
            self.frame.last_cpu_phases = [
                timings.total_ms,
                timings.begin_frame_ms,
                timings.layout_total_ms,
                timings.prepare_paint_ms,
                timings.sync_properties_ms,
                timings.build_graph_ms,
                timings.compile_ms,
                timings.execute_ms,
                timings.finish_render_ms,
                timings.end_frame_ms,
            ];
            self.frame.last_completion_phases = [
                timings.end_frame.staging_finish_ms,
                timings.end_frame.encoder_finish_ms,
                timings.end_frame.submit_ms,
                timings.end_frame.resource_cleanup_ms,
                timings.end_frame.present_ms,
                timings.end_frame.gpu_wait_ms,
            ];
        }

        #[cfg(test)]
        frame_timing_tests::assert_frame_accounting(&timings);

        #[cfg(feature = "renderer-test-support")]
        {
            self.frame.last_diagnostics = diagnostic_enabled.then(|| {
                downstream_test_support::RendererTestDiagnostics::capture(
                    &timings,
                    &self.compositor,
                    crate::view::base_component::take_layout_place_profile(),
                )
            });
        }

        // --- Trace output ---
        if self.debug_options.trace_render_time {
            println!("paint chunks={recorded_chunks} ops={recorded_ops}");
            if !self.frame.gpu_paint_sources.is_empty() {
                println!(
                    "gpu-paint frame={} sources={:?}",
                    frame_number,
                    self.gpu_paint_observations()
                );
            }
            self.print_retained_raster_diagnostics(frame_number);
            let trace_root = Self::build_frame_trace_tree(&timings, &self.debug_options);
            println!("{}", format_trace_render_tree(&trace_root));
        }
        crate::view::base_component::set_text_measure_profile_enabled(false);
        self.frame.frame_stats.record_frame(profile_start.elapsed());
        // Only persist the graph when compile succeeded; a failed compile
        // leaves the graph in an inconsistent state.
        self.frame.last_frame_graph = if compiled { Some(graph) } else { None };
        // This frame's text layout is done; the reused segmenter state must
        // not keep the last laid-out text alive until the next frame.
        crate::platform::word_segmenter::release_cached_text();
        // Animation dirtiness has already been painted in this frame. Only
        // retry it if submission failed; future samples use the deadline API.
        post_layout_transition.redraw_changed
            || (post_layout_animation_changed && !self.frame.frame_presented)
    }

    /// Reports a render pass the frame graph left out, once per message.
    pub(crate) fn report_skipped_render_pass(&mut self, message: String) {
        if !self.frame.skipped_render_passes.contains(&message) {
            eprintln!("[error] {message}");
            self.frame.skipped_render_passes.insert(message);
        }
    }

    #[cfg(test)]
    pub(crate) fn skipped_render_passes_for_test(&self) -> usize {
        self.frame.skipped_render_passes.len()
    }

    /// Warns once about each node a frame leaves out, until it paints again.
    fn report_left_out_paint(&mut self, left_out: &FxHashSet<crate::view::node_arena::NodeKey>) {
        for key in left_out {
            if !self.frame.left_out_paint.contains(key) {
                eprintln!(
                    "[warn] node {key:?} and its subtree are not painted: their paint inputs are incomplete"
                );
            }
        }
        self.frame.left_out_paint.clone_from(left_out);
    }

    fn update_animation_frame_schedule(&mut self, now: Instant) {
        use crate::view::base_component::{AnimationFrameRequest, animation_frame_request};

        let request =
            animation_frame_request(&self.scene.node_arena, &self.scene.ui_root_keys, now);
        self.scheduled_animation_request = request;
        // A new frame supersedes any outstanding animation wake-up.
        self.animation_redraw_pending.set(false);
        if let Some(timer) = &self.animation_timer {
            timer.cancel();
        }
        match request {
            AnimationFrameRequest::None => {}
            AnimationFrameRequest::At(at) if at > now => {
                let pending = self.animation_redraw_pending.clone();
                let timer = self.animation_timer.get_or_insert_with(|| {
                    crate::time::timers::Timer::new(move || {
                        pending.set(true);
                        crate::ui::request_timer_redraw();
                    })
                });
                timer.schedule(at, None);
            }
            AnimationFrameRequest::NextFrame | AnimationFrameRequest::At(_) => {
                self.request_redraw();
            }
        }
    }

    /// Commits and renders a resolved tree. Trees holding user components
    /// must come from [`crate::ui::render_root`]; `rsx!` alone defers them.
    pub fn render_rsx(&mut self, root: &RsxNode) -> Result<(), String> {
        // The sole semantic engine-time sample for this viewport frame. Every
        // retained animation tick and paint-resource freeze observes this
        // exact value; profiling clocks below remain observational only.
        self.render_rsx_at(root, crate::time::Instant::now(), None)
    }

    fn render_rsx_at(
        &mut self,
        root: &RsxNode,
        semantic_now: crate::time::Instant,
        app_profile: Option<(FrontendProfile, crate::ui::UiWorkProfile, Instant)>,
    ) -> Result<(), String> {
        #[cfg(any(test, feature = "renderer-test-support"))]
        {
            self.frame.last_cpu_phases = [0.; 10];
            self.frame.last_completion_phases = [0.; 6];
        }
        let _capture = crate::ui::work_profile::capture(
            self.debug_options.trace_render_time
                || cfg!(any(test, feature = "renderer-test-support")),
        );
        let before = app_profile
            .as_ref()
            .map_or_else(crate::ui::work_profile::snapshot, |(_, before, _)| *before);
        self.frame.frontend = FrontendProfile::default();
        let flush_start = Instant::now();
        let _state_frame = crate::ui::begin_state_frame();
        let (mut frontend, scene_start) = if let Some((frontend, _, start)) = app_profile {
            (frontend, start)
        } else {
            let scene_start = Instant::now();
            (
                FrontendProfile {
                    state_flush_ms: (scene_start - flush_start).as_secs_f64() * 1000.0,
                    ..Default::default()
                },
                scene_start,
            )
        };
        let state_dirty = take_state_dirty();
        // Apply any viewport mutations that component event handlers
        // enqueued via `use_viewport()` during the previous tick. Must
        // run before dirty evaluation so toggles like trace_render_time
        // take effect on the upcoming frame.
        self.apply_pending_viewport_actions();
        // Reset the animation flag — transition plugins below will set
        // it back to true if any of them still want more frames.
        let was_animating = std::mem::replace(&mut self.is_animating, false);
        let resource_dirty = crate::view::image_resource::take_image_redraw_dirty()
            | crate::view::svg_resource::take_svg_redraw_dirty();
        // Pointer equality certifies an unchanged tree. Otherwise reconcile
        // once, without an earlier structural equality walk. Placement and the
        // general translator consume the same root-relative patch schedule.
        let rooted_patches = self.scene.last_rsx_root.as_ref().map(|previous| {
            if RsxNode::ptr_eq(previous, root) {
                return Vec::new();
            }
            crate::ui::reconcile_multi(Some(&unpack_root_set(previous)), &unpack_root_set(root))
        });
        crate::ui::work_profile::count(|p| {
            p.patches += rooted_patches.as_ref().map_or(0, Vec::len)
        });
        let root_changed = rooted_patches
            .as_ref()
            .is_none_or(|patches| !patches.is_empty());
        if !root_changed {
            // A newly allocated but equal tree is now the accepted snapshot.
            // Subsequent redraws can use pointer equality instead of diffing
            // it again against the older allocation.
            self.scene.last_rsx_root = Some(root.clone());
        }
        let mut needs_rebuild = state_dirty.needs_rebuild() || root_changed;
        if root_changed
            && let Some(patches) = &rooted_patches
            && self.try_apply_placement_updates(root, patches)?
        {
            needs_rebuild = false;
        }
        let input_targets = self.input_target_ids();
        // Keep pre-commit scroll state until a possibly partial failed batch
        // has completed its cold recovery too.
        let mut incremental_scroll_offsets = FxHashMap::default();
        // Incremental Fiber-commit path.
        //
        // Only engaged when ALL of:
        //   - incremental commit is enabled (default true),
        //   - a previous `last_rsx_root` exists (not a cold start),
        //   - the full-rebuild path below would otherwise run,
        //   - we have at least one arena root,
        //   - every reconcile patch can be translated into FiberWork
        //     and applied safely.
        //
        // Any failure leaves `needs_rebuild` untouched and falls
        // through to the full-rebuild path.
        if needs_rebuild
            && self.scene.use_incremental_commit
            && self.scene.last_rsx_root.is_some()
            && !self.scene.ui_root_keys.is_empty()
        {
            let previous_root = self.scene.last_rsx_root.as_ref().unwrap();
            // 軌 1 #4 Fragment-at-root: unpack Fragment root into its
            // children so `reconcile_multi` sees the same arity that
            // the arena stores (Fragment root → N arena roots).
            let old_roots = unpack_root_set(previous_root);
            let new_roots = unpack_root_set(root);
            let translate_profile =
                crate::ui::work_profile::scope(crate::ui::work_profile::Phase::Translate);
            let descriptor_ctx = crate::view::fiber_work::DescriptorContext {
                new_rsx_root: root,
                // 軌 1 #6: pass the previous tree so the translator
                // can identity-validate parent_path walks for
                // InsertChild patches.
                old_rsx_root: Some(previous_root),
                inherited_style: &self.style,
                viewport_width: self.logical_width,
                viewport_height: self.logical_height,
            };
            let translated = crate::view::fiber_work::translate_rooted_patches_all_or_nothing(
                rooted_patches.unwrap_or_default(),
                self.scene.node_arena.stable_id_index(),
                &self.scene.node_arena,
                &self.scene.ui_root_keys,
                &old_roots,
                &new_roots,
                Some(&descriptor_ctx),
            );
            drop(translate_profile);
            if let Some(works) = translated {
                crate::ui::work_profile::count(|p| p.fiber_works += works.len());
                let all_committable = works
                    .iter()
                    .all(|w| w.is_committable(&self.scene.node_arena));
                if all_committable {
                    let _commit_profile = crate::ui::work_profile::scope(
                        crate::ui::work_profile::Phase::IncrementalCommit,
                    );
                    incremental_scroll_offsets =
                        Self::save_replaced_scroll_states(&self.scene.node_arena, &works);
                    let apply_ctx = crate::view::fiber_work::ApplyContext {
                        viewport_style: &self.style,
                        viewport_width: self.logical_width,
                        viewport_height: self.logical_height,
                    };
                    let incremental_result = crate::view::fiber_work::apply_fiber_works(
                        &mut self.scene.node_arena,
                        apply_ctx,
                        works,
                    );
                    match incremental_result {
                        Ok(()) => {
                            // Keep the arena roots view in lockstep: ReplaceRoot
                            // mints a new root NodeKey, so always refresh from
                            // the arena after a committed batch.
                            let refreshed_roots = self.scene.node_arena.roots().to_vec();
                            self.scene.ui_root_keys = refreshed_roots;
                            Self::restore_replaced_scroll_states(
                                &self.scene.node_arena,
                                &incremental_scroll_offsets,
                            );
                            self.scene.last_rsx_root = Some(root.clone());
                            needs_rebuild = false;
                        }
                        Err(error) => {
                            // Earlier work in this non-transactional batch may
                            // already have rewritten the arena root set. The
                            // cold path below must remove the current roots as
                            // well as any still-live roots from the stale
                            // viewport mirror, or newly-created subtrees and
                            // their stable-id/sync registrations would leak.
                            self.scene
                                .refresh_roots_for_cold_rebuild_after_incremental_failure();
                            eprintln!(
                                "[render_rsx] incremental apply failed; cold rebuild: {error:?}"
                            );
                        }
                    }
                }
            }
        }
        if needs_rebuild {
            let _commit_profile =
                crate::ui::work_profile::scope(crate::ui::work_profile::Phase::ColdCommit);
            // Clear and save current scroll states
            self.scene.scroll_offsets.clear();
            Self::save_scroll_states(
                &self.scene.node_arena,
                &self.scene.ui_root_keys,
                &mut self.scene.scroll_offsets,
            );
            self.scene.scroll_offsets.extend(incremental_scroll_offsets);
            let layout_snapshots =
                crate::view::viewport::transitions_tick::collect_layout_transition_snapshots(
                    &self.scene.node_arena,
                    &self.scene.ui_root_keys,
                );
            let (converted_descriptors, conversion_errors) =
                crate::view::renderer_adapter::rsx_to_descriptors_with_context(
                    root,
                    &self.style,
                    self.logical_width,
                    self.logical_height,
                );
            if !conversion_errors.is_empty() {
                eprintln!(
                    "[render_rsx] skipped {} invalid node(s):\n{}",
                    conversion_errors.len(),
                    conversion_errors.join("\n")
                );
            }
            // An empty tree, such as one a render panic unmounted, still
            // replaces the previous one.
            if converted_descriptors.is_empty() && !conversion_errors.is_empty() {
                eprintln!("[render_rsx] no valid root nodes converted; keep previous render tree");
                self.scene.last_rsx_root = Some(root.clone());
                drop(_commit_profile);
                frontend.scene_update_ms = scene_start.elapsed().as_secs_f64() * 1000.0;
                frontend.work = crate::ui::work_profile::snapshot().since(before);
                self.frame.frontend = frontend;
                return Ok(());
            }
            // Approach-C: drop the previous arena subtree and commit the
            // freshly-built descriptor trees as new arena roots. `ui_roots`
            // (the legacy boxed mirror) stays empty — arena is the source
            // of truth; render and layout below walk the arena via root keys
            // instead.
            for old_key in std::mem::take(&mut self.scene.ui_root_keys) {
                self.scene.node_arena.remove_subtree(old_key);
            }
            let mut new_root_keys = Vec::with_capacity(converted_descriptors.len());
            for desc in converted_descriptors {
                let key = crate::view::renderer_adapter::commit_descriptor_tree(
                    &mut self.scene.node_arena,
                    None,
                    desc,
                );
                new_root_keys.push(key);
            }
            self.scene.ui_root_keys = new_root_keys.clone();
            self.scene.node_arena.set_roots(new_root_keys);
            self.scene.last_rsx_root = Some(root.clone());

            // Restore scroll states into new elements
            Self::restore_scroll_states(
                &self.scene.node_arena,
                &self.scene.ui_root_keys,
                &self.scene.scroll_offsets,
            );
            {
                let mut arena = std::mem::take(&mut self.scene.node_arena);
                let root_keys = self.scene.ui_root_keys.clone();
                crate::view::viewport::transitions_tick::seed_layout_transition_snapshots(
                    &mut arena,
                    &root_keys,
                    &layout_snapshots,
                );
                self.scene.node_arena = arena;
            }
            // Drop tracks for channels the rebuilt tree no longer declares
            // before applying in-flight samples — otherwise a removed
            // transition would re-stamp the stale interpolated value over
            // the freshly synced target.
            let _ = self.cancel_disallowed_transition_tracks();
            let has_inflight_transition = self.sync_inflight_transition_state();
            if has_inflight_transition {
                self.request_redraw();
            }
        }
        self.fix_up_input_targets(input_targets);
        self.sync_focus_dispatch();
        let animation_changed = {
            let mut arena = std::mem::take(&mut self.scene.node_arena);
            let root_keys = self.scene.ui_root_keys.clone();
            let changed = crate::view::base_component::tick_animation_frames(
                &mut arena,
                &root_keys,
                semantic_now,
            );
            self.scene.node_arena = arena;
            changed
        };
        let canceled_tracks = self.cancel_disallowed_transition_tracks();
        // Reconciling runtime transition state is a whole-tree walk; when
        // no claims are active now AND none were active last frame there
        // is no per-node state left to clear, so the walk is a no-op.
        let claims_empty = self.transitions.transition_claims.is_empty();
        let reconcile_skippable = claims_empty && self.transitions.claims_were_empty;
        self.transitions.claims_were_empty = claims_empty;
        let reconciled_transition_state = if reconcile_skippable {
            false
        } else {
            let mut arena = std::mem::take(&mut self.scene.node_arena);
            let root_keys = self.scene.ui_root_keys.clone();
            let result =
                crate::view::viewport::transitions_tick::reconcile_transition_runtime_state(
                    &mut arena,
                    &root_keys,
                    &active_channels_by_node(&self.transitions.transition_claims),
                );
            self.scene.node_arena = arena;
            result
        };
        let (dt, now_seconds) = self.transition_timing(semantic_now);
        let transition_changed_before_render = canceled_tracks
            || reconciled_transition_state
            || self.run_pre_layout_transitions(dt, now_seconds);
        frontend.scene_update_ms = scene_start.elapsed().as_secs_f64() * 1000.0;
        frontend.work = crate::ui::work_profile::snapshot().since(before);
        self.frame.frontend = frontend;
        let hover_changed_before_render = self.sync_pointer_hover_visual(needs_rebuild);
        // An empty tree still renders, so the frame it replaced is cleared.
        let transition_changed_after_layout = self.render_render_tree(
            dt,
            now_seconds,
            semantic_now,
            state_dirty.has_any()
                || root_changed
                || resource_dirty
                || animation_changed
                || transition_changed_before_render
                || was_animating
                || hover_changed_before_render,
        );
        let hover_resynced = std::mem::take(&mut self.frame.pointer_hover_resync_pending)
            && self.resync_pointer_hover();
        let hover_changed = self.frame.frame_presented && self.sync_pointer_hover_visual(false);
        if self.frame.render_required
            || hover_resynced
            || hover_changed
            || (animation_changed && !self.frame.frame_presented)
            || transition_changed_before_render
            || transition_changed_after_layout
        {
            self.request_redraw();
        }
        self.update_animation_frame_schedule(semantic_now);
        if std::mem::take(&mut self.frame.frame_presented) {
            self.notify_cursor_handler();
        }
        Ok(())
    }

    fn sync_pointer_hover_visual(&mut self, rebuilt: bool) -> bool {
        let next_hover_target = self.pointer_position_viewport().and_then(|(x, y)| {
            Self::hit_test_pointer_target(
                &self.scene.node_arena,
                &self.scene.popup_stack,
                &self.scene.ui_root_keys,
                x,
                y,
            )
            .map(|(_, t)| t)
        });
        // Re-applying hover flags is a whole-tree walk; skip it when the
        // hover target is unchanged and the arena was not rebuilt this
        // frame (a rebuild drops the per-node hover flags).
        if next_hover_target == self.input_state.hovered_node_id && !rebuilt {
            false
        } else {
            let mut arena = std::mem::take(&mut self.scene.node_arena);
            let root_keys = self.scene.ui_root_keys.clone();
            let result = Self::sync_hover_visual_only(
                &mut arena,
                &root_keys,
                &mut self.input_state.hovered_node_id,
                next_hover_target,
                &mut self.input_state.hover_text_recascade,
            );
            self.scene.node_arena = arena;
            self.recascade_hover_text();
            result
        }
    }

    /// Build RSX (if dirty) and render a frame in one call.
    ///
    /// Requires a live `App` set via `set_app`. Checks global dirty
    /// state, runs `App::build` through [`crate::ui::render_root`] when a
    /// rebuild is needed, then delegates to `render_rsx` for the GPU work.
    pub fn render_frame(
        &mut self,
        services: crate::platform::PlatformServices<'_>,
    ) -> super::RenderFrameResult {
        let _capture = crate::ui::work_profile::capture(
            self.debug_options.trace_render_time
                || cfg!(any(test, feature = "renderer-test-support")),
        );
        let before = crate::ui::work_profile::snapshot();
        let flush_start = Instant::now();
        let _state_frame = crate::ui::begin_state_frame();
        let build_start = Instant::now();
        let mut frontend = FrontendProfile {
            state_flush_ms: (build_start - flush_start).as_secs_f64() * 1000.0,
            ..Default::default()
        };
        self.frame.frontend = FrontendProfile::default();
        if self.app.is_none() {
            return super::RenderFrameResult::Ok;
        }

        if peek_state_dirty().needs_rebuild() {
            self.needs_rebuild = true;
        }

        let mut scene_start = build_start;
        if self.needs_rebuild || self.cached_rsx.is_none() {
            let rsx = self.with_app(services, |app, ctx| {
                crate::ui::render_root(|| app.build(ctx))
            });
            self.cached_rsx = Some(rsx);
            self.needs_rebuild = false;
            scene_start = Instant::now();
            frontend.rsx_build_ms = (scene_start - build_start).as_secs_f64() * 1000.0;
        }

        if let Some(rsx) = self.cached_rsx.clone() {
            let _ = self.render_rsx_at(
                &rsx,
                crate::time::Instant::now(),
                Some((frontend, before, scene_start)),
            );
        }

        // Only inspect roots for layout readiness. Diagnostic box models are
        // collected on demand; an intentionally zero-sized root is valid.
        let has_layout_root = self.scene.ui_root_keys.iter().any(|&key| {
            self.scene.node_arena.get(key).is_some_and(|node| {
                let (width, height) = node.element.measured_size();
                width.is_finite() && height.is_finite() && width >= 0. && height >= 0.
            })
        });
        // A tree that renders nothing, such as one a render panic unmounted,
        // has no layout to wait for.
        let renders_something = self.cached_rsx.as_ref().is_some_and(
            |rsx| !matches!(rsx, crate::ui::RsxNode::Fragment(f) if f.children.is_empty()),
        );
        if renders_something && !has_layout_root {
            super::RenderFrameResult::NeedsRetry
        } else {
            super::RenderFrameResult::Ok
        }
    }

    /// Front-end CPU observations from the latest render attempt. Detailed
    /// work counts require tracing or `ui::profile_ui_work`; failed attempts
    /// without a completed scene update report a cleared sample.
    pub fn frontend_profile(&self) -> FrontendProfile {
        self.frame.frontend
    }

    /// Forward an `AppEvent` to the held `App::on_event`.
    pub fn dispatch_app_event(
        &mut self,
        event: &crate::app::AppEvent,
        services: crate::platform::PlatformServices<'_>,
    ) {
        let _batch = crate::ui::begin_state_batch();
        self.with_app(services, |app, ctx| {
            crate::ui::run_callback("App::on_event", || app.on_event(event, ctx))
        });
    }

    /// Call `App::on_ready` exactly once (subsequent calls are no-ops).
    pub fn app_on_ready(&mut self, services: crate::platform::PlatformServices<'_>) {
        if self.ready_dispatched {
            return;
        }
        self.ready_dispatched = true;
        self.with_app(services, |app, ctx| app.on_ready(ctx));
    }

    /// Call `App::on_shutdown`.
    pub fn app_on_shutdown(&mut self, services: crate::platform::PlatformServices<'_>) {
        if self.app.is_none() {
            return;
        }
        self.with_app(services, |app, ctx| app.on_shutdown(ctx));
    }

    /// Temporarily extract the App, build an AppContext, call the
    /// closure, then put the App back. This sidesteps the borrow-checker
    /// conflict between `&mut self` (for `ViewportControl`) and
    /// `&mut self.app`.
    ///
    /// The reborrowing of `services` fields breaks the invariant lifetime
    /// binding that `&'a mut` references carry, allowing the compiler to
    /// pick a shorter, block-scoped lifetime for the `AppContext`.
    fn with_app<R>(
        &mut self,
        services: crate::platform::PlatformServices<'_>,
        f: impl FnOnce(&mut dyn crate::app::App, &mut crate::app::AppContext<'_>) -> R,
    ) -> R {
        let mut app = self.app.take().expect("no app set");
        let result = {
            let mut ctx = crate::app::AppContext {
                viewport: super::ViewportControl::new(self),
                services: crate::platform::PlatformServices {
                    clipboard: &mut *services.clipboard,
                    cursor: &mut *services.cursor,
                    redraw: services.redraw,
                },
            };
            f(&mut *app, &mut ctx)
        };
        self.app = Some(app);
        result
    }

    /// Drain the thread-local queue populated by `ui::use_viewport()` and
    /// apply each action to this viewport. Called at the top of
    /// `render_rsx` so event handlers from the prior frame land
    /// before dirty flags are read.
    fn apply_pending_viewport_actions(&mut self) {
        let actions = crate::ui::drain_viewport_actions();
        if actions.is_empty() {
            return;
        }
        let mut options = self.debug_options;
        for action in actions {
            match action {
                crate::ui::ViewportAction::SetDebugTraceFps(on) => {
                    options.trace_fps = on;
                }
                crate::ui::ViewportAction::SetDebugTraceRenderTime(on) => {
                    options.trace_render_time = on;
                }
                crate::ui::ViewportAction::SetDebugTraceLayoutDetail(on) => {
                    options.trace_layout_detail = on;
                }
                crate::ui::ViewportAction::SetDebugTraceCompileDetail(on) => {
                    options.trace_compile_detail = on;
                }
                crate::ui::ViewportAction::SetDebugTraceExecuteDetail(on) => {
                    options.trace_execute_detail = on;
                }
                crate::ui::ViewportAction::SetDebugGeometryOverlay(on) => {
                    options.geometry_overlay = on;
                }
                crate::ui::ViewportAction::SetDebugRetainedAutoOverlay(on) => {
                    options.retained_auto_overlay = on;
                }
                crate::ui::ViewportAction::SetDebugRetainedAutoShowReuse(on) => {
                    options.retained_auto_show_reuse = on;
                }
                crate::ui::ViewportAction::SetDebugRetainedAutoShowReraster(on) => {
                    options.retained_auto_show_reraster = on;
                }
                crate::ui::ViewportAction::SetDebugRetainedAutoShowTransient(on) => {
                    options.retained_auto_show_transient = on;
                }
                crate::ui::ViewportAction::SetClearColor(color) => {
                    self.set_clear_color(Box::new(color));
                }
                crate::ui::ViewportAction::SetCursor(cursor) => {
                    self.set_cursor(cursor);
                }
                crate::ui::ViewportAction::RequestRedraw => {
                    self.frame.render_required = true;
                    self.request_redraw();
                }
            }
        }
        if options != self.debug_options {
            self.set_debug_options(options);
        }
    }

    fn begin_frame(&mut self) -> Option<BeginFrameProfile> {
        // If a frame is already in progress (e.g. recursive render call),
        // return a zero-cost profile so the caller proceeds with the
        // existing encoder rather than skipping the frame entirely.
        if self.frame.frame_state.is_some() {
            return Some(BeginFrameProfile {
                create_encoder_ms: 0.0,
            });
        }
        #[cfg(feature = "renderer-test-support")]
        if self.frame.offscreen_redraw_target.is_some() {
            return self.prepare_offscreen_redraw_for_test();
        }
        if !self.apply_pending_reconfigure() {
            return None;
        }
        self.frame.offscreen_render_target_pool.begin_frame();
        self.reclaim_idle_frame_gpu_pools();
        self.reset_rect_instance_frame();
        self.reset_text_globals_frame();
        crate::view::render_pass::draw_rect_pass::begin_draw_rect_resources_frame();
        crate::view::render_pass::composite_layer_pass::begin_composite_layer_resources_frame(
            self.render_resource_scope_id(),
        );
        crate::view::render_pass::texture_composite_pass::begin_texture_composite_resources_frame(
            self.render_resource_scope_id(),
        );
        crate::view::render_pass::shadow_module::begin_shadow_resources_frame();
        crate::view::render_pass::text_pass::begin_text_resources_frame_for_scope(
            self.render_resource_scope_id(),
        );

        self.gpu.surface.as_ref()?;
        let device = self.gpu.device.as_ref()?;

        let create_encoder_started_at = Instant::now();
        let encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        let create_encoder_ms = create_encoder_started_at.elapsed().as_secs_f64() * 1000.0;

        self.frame.frame_state = Some(FrameState {
            render_texture: None,
            #[cfg(any(test, feature = "renderer-test-support"))]
            offscreen_texture: None,
            view: None,
            surface_acquire_ms: 0.,
            surface_create_view_ms: 0.,
            resolve_view: None,
            encoder,
            depth_view: self.gpu.depth_view.clone(),
        });
        Some(BeginFrameProfile { create_encoder_ms })
    }

    #[cfg(any(test, feature = "renderer-test-support"))]
    pub(super) fn prepare_offscreen_test_frame(
        &mut self,
        device: wgpu::Device,
        queue: wgpu::Queue,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) -> Result<(), String> {
        if self.frame.frame_state.is_some() {
            return Err("an offscreen test frame is already active".to_string());
        }
        let width = width.max(1);
        let height = height.max(1);
        self.gpu.device = Some(device.clone());
        self.gpu.queue = Some(queue);
        self.gpu.surface = None;
        self.gpu.surface_config.width = width;
        self.gpu.surface_config.height = height;
        self.gpu.surface_config.format = format;
        self.gpu.surface_config.usage =
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC;
        self.gpu.surface_config.view_formats.clear();
        self.gpu.surface_target_format = format;
        self.gpu.msaa_sample_count = 1;
        self.gpu.depth_texture = None;
        self.gpu.depth_view = None;
        self.scale_factor = 1.0;
        self.logical_width = width as f32;
        self.logical_height = height as f32;

        self.frame.offscreen_render_target_pool.begin_frame();
        self.reset_rect_instance_frame();
        self.reset_text_globals_frame();
        crate::view::render_pass::draw_rect_pass::begin_draw_rect_resources_frame();
        crate::view::render_pass::composite_layer_pass::begin_composite_layer_resources_frame(
            self.render_resource_scope_id(),
        );
        crate::view::render_pass::texture_composite_pass::begin_texture_composite_resources_frame(
            self.render_resource_scope_id(),
        );
        crate::view::render_pass::shadow_module::begin_shadow_resources_frame();
        crate::view::render_pass::text_pass::begin_text_resources_frame_for_scope(
            self.render_resource_scope_id(),
        );

        let encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        self.frame.frame_state = Some(FrameState {
            render_texture: None,
            offscreen_texture: None,
            view: None,
            surface_acquire_ms: 0.,
            surface_create_view_ms: 0.,
            resolve_view: None,
            encoder,
            depth_view: None,
        });
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn encode_offscreen_test_readback(
        &mut self,
        buffer: &wgpu::Buffer,
        padded_bytes_per_row: u32,
        width: u32,
        height: u32,
    ) -> Result<(), String> {
        let frame = self
            .frame
            .frame_state
            .as_mut()
            .ok_or_else(|| "no active offscreen test frame".to_string())?;
        let texture = frame
            .offscreen_texture
            .as_ref()
            .ok_or_else(|| "active test frame has no offscreen texture".to_string())?;
        frame.encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bytes_per_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn end_offscreen_test_frame(&mut self) -> Result<(), String> {
        if self.frame.frame_state.is_none() {
            return Err("no active offscreen test frame".to_string());
        }
        let _ = self.submit_and_present_frame();
        Ok(())
    }

    fn complete_frame(&mut self, disposition: FrameDisposition) -> EndFrameProfile {
        match disposition {
            FrameDisposition::SubmitAndPresent => self.submit_and_present_frame(),
            FrameDisposition::Abort => self.abort_frame(),
        }
    }

    fn abort_frame(&mut self) -> EndFrameProfile {
        let started_at = Instant::now();
        self.frame.frame_presented = false;
        let Some(frame) = self.frame.frame_state.take() else {
            return EndFrameProfile::default();
        };

        frame.discard_unsubmitted();
        self.finish_gpu_paint_frame(false);
        self.finish_shadow_template_frame(false);
        self.frame.offscreen_render_target_pool.finish_frame();

        #[cfg(not(target_arch = "wasm32"))]
        {
            // StagingBelt has no abort/reset operation. `recall()` is only
            // valid after every encoder containing its copies was submitted,
            // so abandon the belt and lazily recreate it on the next upload.
            self.gpu.upload_staging_belt = None;
        }
        #[cfg(target_arch = "wasm32")]
        crate::view::render_pass::destroy_frame_transient_buffers();

        #[cfg(any(test, feature = "renderer-test-support"))]
        {
            self.frame.completion_counts.aborts =
                self.frame.completion_counts.aborts.saturating_add(1);
        }

        EndFrameProfile {
            abort_cleanup_ms: started_at.elapsed().as_secs_f64() * 1000.0,
            ..Default::default()
        }
    }

    fn submit_and_present_frame(&mut self) -> EndFrameProfile {
        let mut clock = super::frame::FramePhaseClock::new(Instant::now());
        let frame = match self.frame.frame_state.take() {
            Some(frame) => frame,
            None => return EndFrameProfile::default(),
        };
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(staging_belt) = self.gpu.upload_staging_belt.as_mut() {
            staging_belt.finish();
        }

        let mut profile = EndFrameProfile {
            staging_finish_ms: clock.checkpoint_ms(),
            ..Default::default()
        };
        let command_buffer = frame.encoder.finish();
        profile.encoder_finish_ms = clock.checkpoint_ms();
        let queue = self.gpu.queue.as_ref().unwrap();
        let _submission_index = queue.submit(Some(command_buffer));
        profile.submit_ms = clock.checkpoint_ms();
        #[cfg(any(test, feature = "renderer-test-support"))]
        {
            self.frame.completion_counts.submits =
                self.frame.completion_counts.submits.saturating_add(1);
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(staging_belt) = self.gpu.upload_staging_belt.as_mut() {
            staging_belt.recall();
        }
        #[cfg(target_arch = "wasm32")]
        crate::view::render_pass::destroy_frame_transient_buffers();
        self.frame.offscreen_render_target_pool.finish_frame();
        profile.resource_cleanup_ms = clock.checkpoint_ms();

        if let Some(render_texture) = frame.render_texture {
            queue.present(render_texture);
            #[cfg(any(test, feature = "renderer-test-support"))]
            {
                self.frame.completion_counts.presents =
                    self.frame.completion_counts.presents.saturating_add(1);
            }
        }
        profile.present_ms = clock.checkpoint_ms();
        self.finish_gpu_paint_frame(true);
        self.finish_shadow_template_frame(true);
        self.retire_stale_frame_buffers();
        profile.resource_cleanup_ms += clock.checkpoint_ms();
        #[cfg(not(target_arch = "wasm32"))]
        {
            // Surface latency limits acquired swapchain images, but it does not
            // bound every resource referenced by submitted command buffers.
            // Keep a small native submission pipeline and wait only when the
            // oldest frame falls outside it, so per-frame buffers and bind
            // groups can be retired instead of accumulating indefinitely.
            const MAX_IN_FLIGHT_SUBMISSIONS: usize = 2;
            self.gpu.in_flight_submissions.push_back(_submission_index);
            if self.gpu.in_flight_submissions.len() > MAX_IN_FLIGHT_SUBMISSIONS {
                let oldest = self
                    .gpu
                    .in_flight_submissions
                    .pop_front()
                    .expect("submission queue exceeded its non-zero limit");
                if let Some(device) = self.gpu.device.as_ref() {
                    let wait_started_at = Instant::now();
                    let _ = device.poll(wgpu::PollType::Wait {
                        submission_index: Some(oldest),
                        timeout: None,
                    });
                    profile.gpu_wait_ms = wait_started_at.elapsed().as_secs_f64() * 1000.0;
                    profile.gpu_waited = true;
                }
            }
        }
        self.frame.frame_presented = true;
        self.frame.render_required = false;
        profile
    }

    #[cfg(any(test, feature = "renderer-test-support"))]
    fn frame_completion_counts_for_test(&self) -> (u64, u64, u64) {
        let counts = self.frame.completion_counts;
        (counts.submits, counts.presents, counts.aborts)
    }
}

#[cfg(test)]
mod frame_recording_tests;
#[cfg(test)]
mod frame_timing_tests;
#[cfg(test)]
mod incremental_layout_tests;
#[cfg(test)]
mod paint_isolation_tests;

/// Flatten a Fragment-at-root into its children so multi-root reconcile
/// sees the same arity as the arena (Fragment root → N arena roots).
/// Non-Fragment roots pass through as a single-element slice.
fn unpack_root_set(root: &crate::ui::RsxNode) -> Vec<&crate::ui::RsxNode> {
    match root {
        crate::ui::RsxNode::Fragment(frag) => frag.children.iter().collect(),
        other => vec![other],
    }
}

#[cfg(feature = "renderer-test-support")]
pub(super) mod downstream_test_support;
