use super::super::{PaintChunkId, PaintNodePhase, RETAINED_CHILD_MASK_SLOT};
use super::{
    ArtifactSurfaceCompositeGeometryStamp, ArtifactSurfaceRasterTargetId,
    PreparedArtifactSurfaceFrame, PreparedArtifactSurfaceRasterChunk,
    PreparedArtifactSurfaceRasterPlan, PreparedArtifactSurfaceRasterStep, ResolvedClip,
    RetainedSurfaceCompileAction, SealedArtifactSurfaceResidentSet, SurfaceDagExecutionNodeId,
};
use crate::view::base_component::{AncestorClipContext, BuildState, UiBuildContext};
use crate::view::frame_graph::{FrameGraph, PersistentTextureKey};
use crate::view::render_pass::clear_pass::{ClearInput, ClearOutput, ClearParams};
use crate::view::render_pass::composite_layer_pass::{
    CompositeLayerInput, CompositeLayerOutput, CompositeLayerParams, CompositeLayerPass, LayerIn,
};
use crate::view::render_pass::draw_rect_pass::{
    DrawRectInput, DrawRectOutput, DrawRectPass, RenderTargetOut,
};
use crate::view::render_pass::texture_composite_pass::{
    TextureCompositeInput, TextureCompositeOutput, TextureCompositeParams, TextureCompositeSourceIn,
};
use crate::view::render_pass::{ClearPass, TextureCompositePass};
use crate::view::viewport::{RetainedSurfaceFrameStageOwner, Viewport};
use glam::{Mat4, Vec3};
use rustc_hash::FxHashSet;

/// One child-mask transition sealed in painter order for the artifact executor.
///
/// The paired raster chunk remains the source of the mask draw payload. This
/// value freezes only whether that chunk opens, closes, or leaves the stencil
/// scope unchanged, so the executor never re-derives the decision from slot
/// and phase metadata.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ArtifactSurfaceChildMaskAction {
    Unchanged,
    Push,
    Pop,
}

impl ArtifactSurfaceChildMaskAction {
    fn from_chunk_id(id: PaintChunkId) -> Self {
        if id.slot != RETAINED_CHILD_MASK_SLOT {
            return Self::Unchanged;
        }
        match id.phase {
            PaintNodePhase::BeforeChildren => Self::Push,
            PaintNodePhase::AfterChildren => Self::Pop,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ArtifactSurfaceChildMaskStep {
    ArtifactSpan {
        step_index: usize,
        chunk_actions: Vec<ArtifactSurfaceChildMaskAction>,
    },
    NestedSurface(SurfaceDagExecutionNodeId),
}

/// Child-mask program for one scene-root or detached-surface render target.
///
/// Each target starts with stencil depth zero. A parent target retains its
/// scope across `NestedSurface`, while the nested surface owns a separate
/// target and therefore a separate zero-based program.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ArtifactSurfaceChildMaskTargetProgram {
    target: ArtifactSurfaceRasterTargetId,
    max_mask_depth: usize,
    steps: Vec<ArtifactSurfaceChildMaskStep>,
}

/// Graph-inert execution seal consumed only by the C3b3b1 emitter.
///
/// The compiler-owned frame and its child-mask programs stay together. No
/// public parts accessor exists, so a caller cannot substitute a mask program
/// derived from a different prepared frame.
#[derive(Debug)]
pub(super) struct PreparedArtifactSurfaceExecution {
    frame: PreparedArtifactSurfaceFrame,
    root_programs: Vec<ArtifactSurfaceChildMaskTargetProgram>,
    node_programs: Vec<ArtifactSurfaceChildMaskTargetProgram>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ArtifactSurfaceExecutionError {
    InactiveFrameStageOwner,
    ChildMaskDepthOverflow {
        target: ArtifactSurfaceRasterTargetId,
        incoming_depth: u8,
        max_mask_depth: usize,
    },
    PersistentKeyAlreadyDeclared(PersistentTextureKey),
}

#[derive(Clone, Copy)]
enum PreparedArtifactSurfaceComposite {
    Transform {
        params: TextureCompositeParams,
        resolved_clip: ResolvedClip,
    },
    Layer {
        rect_pos: [f32; 2],
        rect_size: [f32; 2],
        opacity: f32,
        resolved_clip: ResolvedClip,
    },
}

/// Computes the absolute maximum child-mask depth for one ordered chunk-id
/// stream. `carried_in_depth` preserves the legacy segment emitter's scopes
/// that may span several calls; complete C3 targets pass zero.
pub(super) fn artifact_child_mask_max_depth(
    ids: impl IntoIterator<Item = PaintChunkId>,
    carried_in_depth: usize,
) -> usize {
    let mut depth = carried_in_depth;
    let mut maximum = 0;
    for id in ids {
        if id.slot != RETAINED_CHILD_MASK_SLOT {
            continue;
        }
        match id.phase {
            PaintNodePhase::BeforeChildren => {
                depth = depth.saturating_add(1);
                maximum = maximum.max(depth);
            }
            PaintNodePhase::AfterChildren => depth = depth.saturating_sub(1),
        }
    }
    maximum
}

fn seal_target_program(
    target: ArtifactSurfaceRasterTargetId,
    steps: &[PreparedArtifactSurfaceRasterStep],
) -> ArtifactSurfaceChildMaskTargetProgram {
    let max_mask_depth = artifact_child_mask_max_depth(
        steps
            .iter()
            .flat_map(step_chunks)
            .map(|chunk| chunk.source().id),
        0,
    );
    let steps = steps
        .iter()
        .enumerate()
        .map(|(step_index, step)| match step {
            PreparedArtifactSurfaceRasterStep::ArtifactSpan(span) => {
                ArtifactSurfaceChildMaskStep::ArtifactSpan {
                    step_index,
                    chunk_actions: span
                        .chunks()
                        .iter()
                        .map(|chunk| {
                            ArtifactSurfaceChildMaskAction::from_chunk_id(chunk.source().id)
                        })
                        .collect(),
                }
            }
            PreparedArtifactSurfaceRasterStep::NestedSurface(child) => {
                ArtifactSurfaceChildMaskStep::NestedSurface(*child)
            }
        })
        .collect();
    ArtifactSurfaceChildMaskTargetProgram {
        target,
        max_mask_depth,
        steps,
    }
}

fn step_chunks(step: &PreparedArtifactSurfaceRasterStep) -> &[PreparedArtifactSurfaceRasterChunk] {
    match step {
        PreparedArtifactSurfaceRasterStep::ArtifactSpan(span) => span.chunks(),
        PreparedArtifactSurfaceRasterStep::NestedSurface(_) => &[],
    }
}

/// Seals only execution facts absent from `PreparedArtifactSurfaceFrame`.
/// Descriptor, resident, graph-key, and compile-action validation remain in
/// their existing owners; graph declared-key collision is checked immediately
/// before target allocation in C3b3b1.
pub(super) fn seal_prepared_artifact_surface_execution(
    frame: PreparedArtifactSurfaceFrame,
) -> PreparedArtifactSurfaceExecution {
    let root_programs = frame
        .raster_plan()
        .roots()
        .iter()
        .map(|root| {
            seal_target_program(
                ArtifactSurfaceRasterTargetId::SceneRoot(root.scene_root()),
                root.steps(),
            )
        })
        .collect();
    let node_programs = frame
        .raster_plan()
        .nodes()
        .iter()
        .map(|node| {
            seal_target_program(
                ArtifactSurfaceRasterTargetId::Surface(node.source()),
                node.steps(),
            )
        })
        .collect();
    PreparedArtifactSurfaceExecution {
        frame,
        root_programs,
        node_programs,
    }
}

fn prepare_composite_geometry(
    geometry: ArtifactSurfaceCompositeGeometryStamp,
) -> Option<PreparedArtifactSurfaceComposite> {
    match geometry {
        ArtifactSurfaceCompositeGeometryStamp::Transform {
            source_bounds_bits,
            destination_bounds_bits,
            receiver_transform_bits,
            resolved_receiver_clip,
            ..
        } => {
            let source = source_bounds_bits.map(f32::from_bits);
            let destination = destination_bounds_bits.map(f32::from_bits);
            let transform = Mat4::from_cols_array(&receiver_transform_bits.map(f32::from_bits));
            if source
                .into_iter()
                .chain(destination)
                .any(|value| !value.is_finite())
                || source[2] <= 0.0
                || source[3] <= 0.0
                || destination[2] <= 0.0
                || destination[3] <= 0.0
                || !transform.is_finite()
            {
                return None;
            }
            let corners = [
                Vec3::new(source[0], source[1] + source[3], 0.0),
                Vec3::new(source[0] + source[2], source[1] + source[3], 0.0),
                Vec3::new(source[0] + source[2], source[1], 0.0),
                Vec3::new(source[0], source[1], 0.0),
            ];
            let mut projected = [[0.0; 2]; 4];
            let mut min_x = f32::INFINITY;
            let mut min_y = f32::INFINITY;
            let mut max_x = f32::NEG_INFINITY;
            let mut max_y = f32::NEG_INFINITY;
            for (index, corner) in corners.into_iter().enumerate() {
                let point = transform * corner.extend(1.0);
                if !point.is_finite() || point.w.abs() <= 0.000_001 {
                    return None;
                }
                let point = [point.x / point.w, point.y / point.w];
                if point.into_iter().any(|value| !value.is_finite()) {
                    return None;
                }
                min_x = min_x.min(point[0]);
                min_y = min_y.min(point[1]);
                max_x = max_x.max(point[0]);
                max_y = max_y.max(point[1]);
                projected[index] = point;
            }
            let raw_bounds = [min_x, min_y, max_x - min_x, max_y - min_y];
            if raw_bounds[2].to_bits() != destination[2].to_bits()
                || raw_bounds[3].to_bits() != destination[3].to_bits()
            {
                return None;
            }
            let delta = [destination[0] - min_x, destination[1] - min_y];
            if delta.into_iter().any(|value| !value.is_finite()) {
                return None;
            }
            for point in &mut projected {
                point[0] += delta[0];
                point[1] += delta[1];
            }
            Some(PreparedArtifactSurfaceComposite::Transform {
                params: TextureCompositeParams {
                    bounds: destination,
                    quad_positions: Some(projected),
                    uv_bounds: Some(source),
                    mask_uv_bounds: None,
                    use_mask: false,
                    source_is_premultiplied: true,
                    opacity: 1.0,
                    scissor_rect: match resolved_receiver_clip {
                        ResolvedClip::Scissor(scissor) => Some(scissor),
                        ResolvedClip::Unclipped | ResolvedClip::Empty => None,
                    },
                },
                resolved_clip: resolved_receiver_clip,
            })
        }
        ArtifactSurfaceCompositeGeometryStamp::Effect {
            destination_bounds_bits,
            opacity_bits,
            resolved_receiver_clip,
            ..
        } => {
            let [x, y, width, height] = destination_bounds_bits.map(f32::from_bits);
            let opacity = f32::from_bits(opacity_bits);
            ([x, y, width, height, opacity]
                .into_iter()
                .all(f32::is_finite)
                && width > 0.0
                && height > 0.0
                && (0.0..=1.0).contains(&opacity))
            .then_some(PreparedArtifactSurfaceComposite::Layer {
                rect_pos: [x, y],
                rect_size: [width, height],
                opacity,
                resolved_clip: resolved_receiver_clip,
            })
        }
        ArtifactSurfaceCompositeGeometryStamp::ScrollContent {
            destination_bounds_bits,
            resolved_receiver_clip,
            ..
        } => {
            let [x, y, width, height] = destination_bounds_bits.map(f32::from_bits);
            ([x, y, width, height].into_iter().all(f32::is_finite) && width > 0.0 && height > 0.0)
                .then_some(PreparedArtifactSurfaceComposite::Layer {
                    rect_pos: [x, y],
                    rect_size: [width, height],
                    opacity: 1.0,
                    resolved_clip: resolved_receiver_clip,
                })
        }
    }
}

fn emit_child_mask_chunk(
    action: ArtifactSurfaceChildMaskAction,
    chunk: &PreparedArtifactSurfaceRasterChunk,
    graph: &mut FrameGraph,
    ctx: &mut UiBuildContext,
    scopes: &mut Vec<(crate::view::node_arena::NodeKey, u8, Option<[u32; 4]>)>,
) {
    let [super::PaintOp::DrawRect(mask)] = chunk.localized_ops() else {
        unreachable!("sealed child-mask chunk owns one localized rect")
    };
    match action {
        ArtifactSurfaceChildMaskAction::Push => {
            let parent_clip_id = ctx.current_clip_id();
            let child_clip_id = ctx
                .push_clip_id()
                .expect("preflighted artifact target child-mask depth");
            let [x, y, width, height] = chunk.localized_bounds_bits().map(f32::from_bits);
            let logical_scissor = crate::view::base_component::exact_logical_scissor_for_rect(
                crate::view::base_component::Rect {
                    x,
                    y,
                    width,
                    height,
                },
            )
            .expect("sealed child-mask chunk has an exact scissor");
            let previous_scissor = ctx.push_scissor_rect(Some(logical_scissor));
            let mut pass = DrawRectPass::new(
                mask.params.clone(),
                DrawRectInput::default(),
                DrawRectOutput::default(),
            );
            pass.set_render_mode(mask.mode);
            pass.set_stencil_increment(parent_clip_id);
            pass.set_color_write_enabled(false);
            ctx.emit_draw_rect_pass(graph, pass);
            scopes.push((chunk.source().owner, child_clip_id, previous_scissor));
        }
        ArtifactSurfaceChildMaskAction::Pop => {
            let (owner, child_clip_id, previous_scissor) = scopes
                .pop()
                .expect("sealed child-mask target program is balanced");
            assert_eq!(owner, chunk.source().owner);
            let mut pass = DrawRectPass::new(
                mask.params.clone(),
                DrawRectInput::default(),
                DrawRectOutput::default(),
            );
            pass.set_render_mode(mask.mode);
            pass.set_stencil_decrement(child_clip_id);
            pass.set_color_write_enabled(false);
            ctx.emit_draw_rect_pass(graph, pass);
            ctx.pop_clip_id();
            ctx.restore_scissor_rect(previous_scissor);
        }
        ArtifactSurfaceChildMaskAction::Unchanged => {
            unreachable!("ordinary chunks never enter the child-mask emitter")
        }
    }
}

fn emit_chunk_ops(
    chunk: &PreparedArtifactSurfaceRasterChunk,
    graph: &mut FrameGraph,
    ctx: &mut UiBuildContext,
) {
    let emit = |ops: &[super::PaintOp], graph: &mut FrameGraph, ctx: &mut UiBuildContext| {
        for op in ops {
            super::emit_artifact_surface_paint_op(op, graph, ctx);
        }
    };
    match chunk.clip_schedule {
        super::ArtifactSurfaceChunkClipSchedule::WholeChunk(ResolvedClip::Unclipped) => {
            emit(chunk.localized_ops(), graph, ctx)
        }
        super::ArtifactSurfaceChunkClipSchedule::WholeChunk(ResolvedClip::Scissor(scissor)) => {
            let previous = ctx.replace_scissor_rect(Some(scissor));
            emit(chunk.localized_ops(), graph, ctx);
            ctx.restore_scissor_rect(previous);
        }
        super::ArtifactSurfaceChunkClipSchedule::WholeChunk(ResolvedClip::Empty) => {}
        super::ArtifactSurfaceChunkClipSchedule::AfterShadowPrefix {
            prefix_op_count,
            suffix_clip,
        } => {
            let (prefix, suffix) = chunk.localized_ops().split_at(prefix_op_count);
            emit(prefix, graph, ctx);
            match suffix_clip {
                ResolvedClip::Unclipped => emit(suffix, graph, ctx),
                ResolvedClip::Scissor(scissor) => {
                    let previous = ctx.replace_scissor_rect(Some(scissor));
                    emit(suffix, graph, ctx);
                    ctx.restore_scissor_rect(previous);
                }
                ResolvedClip::Empty => {}
            }
        }
    }
}

fn emit_artifact_span(
    span: &super::PreparedArtifactSurfaceRasterSpan,
    actions: &[ArtifactSurfaceChildMaskAction],
    graph: &mut FrameGraph,
    ctx: &mut UiBuildContext,
    scopes: &mut Vec<(crate::view::node_arena::NodeKey, u8, Option<[u32; 4]>)>,
) {
    assert_eq!(span.chunks().len(), actions.len());
    let before = ctx.opaque_rect_order();
    for (chunk, action) in span.chunks().iter().zip(actions.iter().copied()) {
        match action {
            ArtifactSurfaceChildMaskAction::Unchanged => emit_chunk_ops(chunk, graph, ctx),
            ArtifactSurfaceChildMaskAction::Push | ArtifactSurfaceChildMaskAction::Pop => {
                emit_child_mask_chunk(action, chunk, graph, ctx, scopes)
            }
        }
    }
    assert_eq!(
        ctx.opaque_rect_order(),
        before.saturating_add(span.opaque_order_count()),
        "artifact span must reach its sealed opaque terminal",
    );
}

fn emit_composite(
    composite: PreparedArtifactSurfaceComposite,
    layer_target: RenderTargetOut,
    parent_target: RenderTargetOut,
    graph: &mut FrameGraph,
    parent_ctx: &mut UiBuildContext,
) {
    match composite {
        PreparedArtifactSurfaceComposite::Transform {
            params,
            resolved_clip,
        } => {
            if resolved_clip == ResolvedClip::Empty {
                return;
            }
            graph.add_graphics_pass(TextureCompositePass::new(
                params,
                TextureCompositeInput::from_render_target(
                    TextureCompositeSourceIn::with_handle(
                        layer_target
                            .handle()
                            .expect("artifact surface target owns a texture handle"),
                    ),
                    Default::default(),
                    parent_ctx.graphics_pass_context(),
                ),
                TextureCompositeOutput {
                    render_target: parent_target,
                },
            ));
        }
        PreparedArtifactSurfaceComposite::Layer {
            rect_pos,
            rect_size,
            opacity,
            resolved_clip,
        } => {
            if resolved_clip == ResolvedClip::Empty {
                return;
            }
            graph.add_graphics_pass(CompositeLayerPass::new(
                CompositeLayerParams {
                    rect_pos,
                    rect_size,
                    corner_radii: [0.0; 4],
                    opacity,
                    scissor_rect: match resolved_clip {
                        ResolvedClip::Scissor(scissor) => Some(scissor),
                        ResolvedClip::Unclipped | ResolvedClip::Empty => None,
                    },
                    clear_target: false,
                },
                CompositeLayerInput {
                    layer: LayerIn::with_handle(
                        layer_target
                            .handle()
                            .expect("artifact surface target owns a texture handle"),
                    ),
                    pass_context: parent_ctx.graphics_pass_context(),
                },
                CompositeLayerOutput {
                    render_target: parent_target,
                },
            ));
        }
    }
    parent_ctx.set_current_target(parent_target);
}

struct ArtifactSurfaceEmitter<'a> {
    plan: &'a PreparedArtifactSurfaceRasterPlan,
    residents: &'a SealedArtifactSurfaceResidentSet,
    programs: &'a [ArtifactSurfaceChildMaskTargetProgram],
    composites: &'a [PreparedArtifactSurfaceComposite],
    actions: &'a [RetainedSurfaceCompileAction],
}

impl ArtifactSurfaceEmitter<'_> {
    fn emit_node(
        &self,
        child: SurfaceDagExecutionNodeId,
        graph: &mut FrameGraph,
        mut parent_ctx: UiBuildContext,
        composite_to_parent: bool,
    ) -> BuildState {
        let node = self
            .plan
            .nodes()
            .get(child.index())
            .filter(|node| node.execution_id() == child)
            .expect("sealed execution node id");
        let resident = self
            .residents
            .ordered_entries()
            .get(child.index())
            .expect("sealed resident execution id");
        let program = self
            .programs
            .get(child.index())
            .filter(|program| {
                program.target == ArtifactSurfaceRasterTargetId::Surface(node.source())
            })
            .expect("sealed target program id");
        let action = self.actions[child.index()];
        let parent_target = parent_ctx
            .current_target()
            .expect("artifact surface has a receiver target");
        let mut layer_ctx = UiBuildContext::from_parts(
            parent_ctx.viewport(),
            parent_ctx.layer_subtree_state_with_ancestor_clip(AncestorClipContext::default()),
        );
        let layer_target = layer_ctx.allocate_persistent_target_with_desc(
            graph,
            node.target().color.clone(),
            node.identity().color_key,
        );
        layer_ctx.set_current_target(layer_target);
        if action == RetainedSurfaceCompileAction::Reraster {
            graph.add_graphics_pass(ClearPass::new(
                ClearParams::new([0.0, 0.0, 0.0, 0.0]),
                ClearInput {
                    pass_context: layer_ctx.graphics_pass_context(),
                    clear_depth_stencil: true,
                },
                ClearOutput {
                    render_target: layer_target,
                },
            ));
            let mut scopes = Vec::new();
            self.emit_steps(program, node.steps(), graph, &mut layer_ctx, &mut scopes);
            assert!(scopes.is_empty(), "sealed target child masks must balance");
        } else {
            for step in &program.steps {
                let ArtifactSurfaceChildMaskStep::NestedSurface(child) = step else {
                    continue;
                };
                let viewport = layer_ctx.viewport();
                let mut detached_parent = UiBuildContext::from_parts(
                    viewport.clone(),
                    layer_ctx
                        .layer_subtree_state_with_ancestor_clip(AncestorClipContext::default()),
                );
                detached_parent.set_current_target(layer_target);
                let child_state = self.emit_node(*child, graph, detached_parent, false);
                layer_ctx.merge_child_target_pairs(&child_state);
            }
            layer_ctx.replay_opaque_rect_order_exact(0, resident.stamp().opaque_order_span.end);
        }
        assert_eq!(
            layer_ctx.opaque_rect_order(),
            resident.stamp().opaque_order_span.end,
            "artifact target must reach its sealed opaque terminal",
        );
        let layer_state = layer_ctx.into_state();
        let parent_before = parent_ctx.opaque_rect_order();
        let child_terminal = resident.stamp().opaque_order_span.end;
        let parent_after = if node.identity().role
            == super::RetainedSurfaceRasterRole::PropertyEffect
            || node.geometry().resolved_receiver_clip() == ResolvedClip::Empty
        {
            parent_before
        } else {
            parent_before.max(child_terminal)
        };
        parent_ctx.merge_child_target_pairs(&layer_state);
        if composite_to_parent {
            parent_ctx.replay_opaque_rect_order_exact(parent_before, parent_after);
            parent_ctx.set_current_target(parent_target);
            emit_composite(
                self.composites[child.index()],
                layer_target,
                parent_target,
                graph,
                &mut parent_ctx,
            );
        } else if action == RetainedSurfaceCompileAction::Reraster {
            graph
                .add_texture_sink(
                    &layer_target,
                    crate::view::frame_graph::ExternalSinkKind::PersistentMaterialization,
                )
                .expect("artifact reraster target supports persistent materialization");
        }
        parent_ctx.into_state()
    }

    fn emit_steps(
        &self,
        program: &ArtifactSurfaceChildMaskTargetProgram,
        steps: &[PreparedArtifactSurfaceRasterStep],
        graph: &mut FrameGraph,
        ctx: &mut UiBuildContext,
        scopes: &mut Vec<(crate::view::node_arena::NodeKey, u8, Option<[u32; 4]>)>,
    ) {
        assert_eq!(program.steps.len(), steps.len());
        for program_step in &program.steps {
            match program_step {
                ArtifactSurfaceChildMaskStep::ArtifactSpan {
                    step_index,
                    chunk_actions,
                } => {
                    let PreparedArtifactSurfaceRasterStep::ArtifactSpan(span) = &steps[*step_index]
                    else {
                        unreachable!("sealed span step kind")
                    };
                    emit_artifact_span(span, chunk_actions, graph, ctx, scopes);
                }
                ArtifactSurfaceChildMaskStep::NestedSurface(child) => {
                    let viewport = ctx.viewport();
                    let state = self.emit_node(
                        *child,
                        graph,
                        UiBuildContext::from_parts(viewport.clone(), ctx.state_clone()),
                        true,
                    );
                    ctx.set_state(state);
                }
            }
        }
    }
}

/// Emits one fully sealed artifact Surface DAG frame and atomically stages the
/// same resident set. Pool canonicality must be established before target
/// allocation because persistent color allocation otherwise contains a
/// depth-key `expect`. Graph-key collision is checked last in preflight so no
/// claimed graph state can go stale before the first append-only mutation.
pub(crate) fn emit_prepared_artifact_surface_frame_from_pool(
    viewport: &mut Viewport,
    owner: RetainedSurfaceFrameStageOwner,
    frame: PreparedArtifactSurfaceFrame,
    graph: &mut FrameGraph,
    ctx: UiBuildContext,
) -> Result<BuildState, ArtifactSurfaceExecutionError> {
    emit_prepared_artifact_surface_frame(viewport, owner, frame, graph, ctx, false)
        .map(|(state, _)| state)
}

fn emit_prepared_artifact_surface_frame(
    viewport: &mut Viewport,
    owner: RetainedSurfaceFrameStageOwner,
    frame: PreparedArtifactSurfaceFrame,
    graph: &mut FrameGraph,
    mut ctx: UiBuildContext,
    allow_forced_pair_witness: bool,
) -> Result<(BuildState, Vec<RetainedSurfaceCompileAction>), ArtifactSurfaceExecutionError> {
    if !viewport.retained_surface_frame_stage_owner_is_active(owner) {
        return Err(ArtifactSurfaceExecutionError::InactiveFrameStageOwner);
    }
    let execution = seal_prepared_artifact_surface_execution(frame);
    let PreparedArtifactSurfaceExecution {
        frame,
        root_programs,
        node_programs,
    } = execution;
    let (plan, residents) = frame.into_parts();

    // This canonicalizer validates every color key's depth role before the
    // infallible allocator reaches its depth-key `expect`.
    #[cfg(test)]
    let pool_emission = if allow_forced_pair_witness {
        viewport
            .prepare_artifact_surface_pool_emission_for_forced_test(residents)
            .expect("compiler-sealed artifact residents are pool canonical")
    } else {
        viewport
            .prepare_artifact_surface_pool_emission_from_pool(residents)
            .expect("compiler-sealed artifact residents are pool canonical")
    };
    #[cfg(not(test))]
    let pool_emission = {
        let _ = allow_forced_pair_witness;
        viewport
            .prepare_artifact_surface_pool_emission_from_pool(residents)
            .expect("compiler-sealed artifact residents are pool canonical")
    };
    let residents = pool_emission.residents();
    let ordered_actions = pool_emission.ordered_actions();
    assert_eq!(ordered_actions.len(), residents.len());
    let actions = ordered_actions
        .iter()
        .zip(residents.ordered_entries())
        .map(|((key, action), resident)| {
            assert_eq!(*key, resident.resident_key());
            *action
        })
        .collect::<Vec<_>>();
    let composites = plan
        .nodes()
        .iter()
        .map(|node| {
            // Raster-plan construction makes this projection total: Transform
            // destinations pass `transform_destination_bounds`, which rejects
            // a non-positive projected AABB, while Effect and ScrollContent
            // preserve source extents already rejected when
            // `has_canonical_descriptor_pair_for` seals their target.
            prepare_composite_geometry(node.geometry())
                .expect("prepared artifact geometry has a typed render-pass projection")
        })
        .collect::<Vec<_>>();

    let incoming_depth = ctx.current_clip_id();
    for program in root_programs.iter().chain(&node_programs) {
        let target_incoming = match program.target {
            ArtifactSurfaceRasterTargetId::SceneRoot(_) => incoming_depth,
            ArtifactSurfaceRasterTargetId::Surface(_) => 0,
        };
        if usize::from(target_incoming) + program.max_mask_depth > usize::from(u8::MAX) {
            return Err(ArtifactSurfaceExecutionError::ChildMaskDepthOverflow {
                target: program.target,
                incoming_depth: target_incoming,
                max_mask_depth: program.max_mask_depth,
            });
        }
    }

    // Keep this as the final preflight read. No graph-derived bool or key-set
    // snapshot is stored in the execution capability.
    let declared = graph
        .declared_persistent_texture_keys()
        .collect::<FxHashSet<_>>();
    for resident in residents.ordered_entries() {
        let color = resident.stamp().identity.color_key;
        let depth = color
            .depth_stencil()
            .expect("pool-canonical artifact color key owns a depth role");
        for key in [color, depth] {
            if declared.contains(&key) {
                return Err(ArtifactSurfaceExecutionError::PersistentKeyAlreadyDeclared(
                    key,
                ));
            }
        }
    }

    let parent_target = ctx.current_target().unwrap_or_else(|| {
        let target = ctx.allocate_target(graph);
        ctx.set_current_target(target);
        target
    });
    ctx.set_current_target(parent_target);
    let emitter = ArtifactSurfaceEmitter {
        plan: &plan,
        residents: &residents,
        programs: &node_programs,
        composites: &composites,
        actions: &actions,
    };
    for (root, program) in plan.roots().iter().zip(&root_programs) {
        assert_eq!(
            program.target,
            ArtifactSurfaceRasterTargetId::SceneRoot(root.scene_root())
        );
        let mut scopes = Vec::new();
        emitter.emit_steps(program, root.steps(), graph, &mut ctx, &mut scopes);
        assert!(scopes.is_empty(), "sealed root child masks must balance");
    }

    // `UiBuildContext` owns only a by-value `ViewportContext`; borrow checking
    // does not protect the staging slot across emission. Pool canonicality is
    // already carried by the linear capability, so this assertion now guards
    // only that the owner stayed active and the pending slot stayed empty.
    let residents = pool_emission.into_canonical_residents();
    assert!(
        viewport.stage_artifact_surface_resident_set(owner, residents),
        "preflighted artifact resident transaction must stage after emission"
    );
    Ok((ctx.into_state(), actions))
}

#[cfg(test)]
fn emit_prepared_artifact_surface_frame_for_forced_test(
    viewport: &mut Viewport,
    owner: RetainedSurfaceFrameStageOwner,
    frame: PreparedArtifactSurfaceFrame,
    graph: &mut FrameGraph,
    ctx: UiBuildContext,
) -> Result<(BuildState, Vec<RetainedSurfaceCompileAction>), ArtifactSurfaceExecutionError> {
    emit_prepared_artifact_surface_frame(viewport, owner, frame, graph, ctx, true)
}

#[cfg(test)]
mod tests;
