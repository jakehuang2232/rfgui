use super::super::{PaintNodePhase, RETAINED_CHILD_MASK_SLOT};
use super::{
    ArtifactSurfaceCompositeGeometryStamp, ArtifactSurfaceRasterOriginProjection,
    ArtifactSurfaceRasterTargetId, ArtifactSurfaceResolvedClip, PreparedArtifactSurfaceFrame,
    PreparedArtifactSurfaceRasterChunk, PreparedArtifactSurfaceRasterPlan,
    PreparedArtifactSurfaceRasterStep, RetainedSurfaceCompileAction,
    SealedArtifactSurfaceResidentSet, SurfaceDagExecutionNodeId,
};
use crate::view::base_component::{AncestorClipContext, BuildState, UiBuildContext};
use crate::view::frame_graph::FrameGraph;
use crate::view::render_pass::clear_pass::{ClearInput, ClearOutput, ClearParams};
use crate::view::render_pass::composite_layer_pass::{
    CompositeLayerInput, CompositeLayerOutput, CompositeLayerParams, CompositeLayerPass, LayerIn,
};
use crate::view::render_pass::draw_rect_pass::{
    DrawRectInput, DrawRectOutput, DrawRectPass, RenderTargetOut,
};
use crate::view::render_pass::render_target::GraphicsPassScissor;
use crate::view::render_pass::texture_composite_pass::{
    TextureCompositeInput, TextureCompositeOutput, TextureCompositeParams, TextureCompositeSourceIn,
};
use crate::view::render_pass::{ClearPass, TextureCompositePass};
use crate::view::viewport::{RetainedSurfaceFrameStageOwner, Viewport};
use rustc_hash::FxHashSet;

#[cfg(any(test, feature = "renderer-test-support"))]
thread_local! {
    static LAST_PRODUCTION_ACTIONS: std::cell::RefCell<Vec<RetainedSurfaceCompileAction>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

#[cfg(any(test, feature = "renderer-test-support"))]
pub(crate) fn take_last_production_actions_for_test() -> Vec<RetainedSurfaceCompileAction> {
    LAST_PRODUCTION_ACTIONS.with(|actions| std::mem::take(&mut *actions.borrow_mut()))
}

/// One child-mask transition sealed in painter order for the artifact executor.
///
/// The paired raster chunk remains the source of the mask draw payload. This
/// value freezes only whether that chunk opens, closes, or leaves the stencil
/// scope unchanged, so the executor never re-derives the decision from slot
/// and phase metadata.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ArtifactSurfaceChildMaskAction {
    Unchanged,
    /// `None` when the mask lies outside the target. It still opens a stencil
    /// scope, but its geometry writes no stencil pixels, so descendants fail
    /// the incremented reference until Pop restores the owner. The incoming
    /// scissor stays as it is.
    Push(Option<GraphicsPassScissor>),
    Pop,
}

impl ArtifactSurfaceChildMaskAction {
    #[cfg(test)]
    fn from_chunk_id(id: super::super::PaintChunkId) -> Self {
        if id.slot != RETAINED_CHILD_MASK_SLOT {
            return Self::Unchanged;
        }
        match id.phase {
            PaintNodePhase::BeforeChildren => {
                Self::Push(Some(GraphicsPassScissor::Logical([0, 0, 1, 1])))
            }
            PaintNodePhase::AfterChildren => Self::Pop,
        }
    }

    fn from_chunk(
        chunk: &PreparedArtifactSurfaceRasterChunk,
        raster_origin: Option<ArtifactSurfaceRasterOriginProjection>,
    ) -> Self {
        let id = chunk.source().id;
        if id.slot != RETAINED_CHILD_MASK_SLOT {
            return Self::Unchanged;
        }
        match id.phase {
            PaintNodePhase::BeforeChildren => {
                let scissor = match raster_origin {
                    Some(projection) => projection.target_physical_scissor_for_projected_bounds(
                        chunk.localized_bounds_bits(),
                    ),
                    None => {
                        let [x, y, width, height] =
                            chunk.localized_bounds_bits().map(f32::from_bits);
                        crate::view::base_component::exact_logical_scissor_for_rect(
                            crate::view::base_component::Rect {
                                x,
                                y,
                                width,
                                height,
                            },
                        )
                        .map(GraphicsPassScissor::Logical)
                    }
                };
                Self::Push(scissor)
            }
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

#[derive(Clone, Copy)]
enum PreparedArtifactSurfaceComposite {
    Transform {
        params: TextureCompositeParams,
        resolved_clip: ArtifactSurfaceResolvedClip,
    },
    Layer {
        rect_pos: [f32; 2],
        rect_size: [f32; 2],
        opacity: f32,
        source_physical_origin: [f32; 2],
        resolved_clip: ArtifactSurfaceResolvedClip,
    },
}

fn seal_target_program(
    target: ArtifactSurfaceRasterTargetId,
    steps: &[PreparedArtifactSurfaceRasterStep],
    raster_origin: Option<ArtifactSurfaceRasterOriginProjection>,
) -> ArtifactSurfaceChildMaskTargetProgram {
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
                            ArtifactSurfaceChildMaskAction::from_chunk(chunk, raster_origin)
                        })
                        .collect(),
                }
            }
            PreparedArtifactSurfaceRasterStep::NestedSurface(child) => {
                ArtifactSurfaceChildMaskStep::NestedSurface(*child)
            }
        })
        .collect();
    ArtifactSurfaceChildMaskTargetProgram { target, steps }
}

/// Seals only execution facts absent from `PreparedArtifactSurfaceFrame`.
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
                None,
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
                Some(node.raster_origin),
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
    raster_origin: ArtifactSurfaceRasterOriginProjection,
) -> Option<PreparedArtifactSurfaceComposite> {
    match geometry {
        ArtifactSurfaceCompositeGeometryStamp::Transform {
            source_bounds_bits,
            destination_bounds_bits,
            ..
        } => {
            let source = source_bounds_bits.map(f32::from_bits);
            let destination = destination_bounds_bits.map(f32::from_bits);
            let projected = geometry.transform_quad()?;
            Some(PreparedArtifactSurfaceComposite::Transform {
                params: TextureCompositeParams {
                    bounds: destination,
                    quad_positions: Some(projected),
                    uv_bounds: Some(source),
                    mask_uv_bounds: None,
                    use_mask: false,
                    source_is_premultiplied: true,
                    opacity: 1.0,
                    scissor_rect: None,
                    nine_patch: None,
                },
                resolved_clip: geometry.resolved_receiver_clip(),
            })
        }
        ArtifactSurfaceCompositeGeometryStamp::Effect {
            source_bounds_bits,
            destination_bounds_bits,
            opacity_bits,
            resolved_receiver_clip,
            ..
        } => {
            let [x, y, width, height] = destination_bounds_bits.map(f32::from_bits);
            let opacity = f32::from_bits(opacity_bits);
            let source_physical_origin = raster_origin
                .composite_source_physical_origin(source_bounds_bits, destination_bounds_bits)?;
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
                source_physical_origin,
                resolved_clip: resolved_receiver_clip,
            })
        }
        ArtifactSurfaceCompositeGeometryStamp::ScrollContent {
            source_bounds_bits,
            destination_bounds_bits,
            resolved_receiver_clip,
            ..
        } => {
            let [x, y, width, height] = destination_bounds_bits.map(f32::from_bits);
            let source_physical_origin = raster_origin
                .composite_source_physical_origin(source_bounds_bits, destination_bounds_bits)?;
            ([x, y, width, height].into_iter().all(f32::is_finite) && width > 0.0 && height > 0.0)
                .then_some(PreparedArtifactSurfaceComposite::Layer {
                    rect_pos: [x, y],
                    rect_size: [width, height],
                    opacity: 1.0,
                    source_physical_origin,
                    resolved_clip: resolved_receiver_clip,
                })
        }
    }
}

/// One open child mask: its owner, its stencil value, the scissor it
/// replaced, and the scratch layer it opened when the stencil was full.
struct ChildMaskScope {
    owner: crate::view::node_arena::NodeKey,
    clip_id: u8,
    previous_scissor: Option<GraphicsPassScissor>,
    layer: Option<ChildMaskLayer>,
}

/// A target holds 255 nested masks in its stencil. A deeper mask draws itself
/// and everything it clips into a scratch layer whose stencil starts empty,
/// which composites back where the outer masks pass.
struct ChildMaskLayer {
    layer: RenderTargetOut,
    outer: RenderTargetOut,
    outer_clip_ids: Vec<u8>,
}

impl ChildMaskLayer {
    fn open(graph: &mut FrameGraph, ctx: &mut UiBuildContext) -> Self {
        let outer = ctx
            .current_target()
            .expect("artifact child masks draw into a target");
        let layer = ctx.allocate_target_like(graph, outer);
        let outer_clip_ids = ctx.take_clip_ids();
        ctx.set_current_target(layer);
        graph.add_graphics_pass(ClearPass::new(
            ClearParams::new([0.0, 0.0, 0.0, 0.0]),
            ClearInput {
                pass_context: ctx.graphics_pass_context(),
                clear_depth_stencil: true,
            },
            ClearOutput {
                render_target: layer,
            },
        ));
        Self {
            layer,
            outer,
            outer_clip_ids,
        }
    }

    fn close(self, graph: &mut FrameGraph, ctx: &mut UiBuildContext) {
        ctx.restore_clip_ids(self.outer_clip_ids);
        ctx.set_current_target(self.outer);
        // Empty bounds cover the whole target: the layer matches it texel for
        // texel, and the outer masks' stencil test admits it.
        graph.add_graphics_pass(TextureCompositePass::new(
            TextureCompositeParams {
                source_is_premultiplied: true,
                ..Default::default()
            },
            TextureCompositeInput::from_render_target(
                TextureCompositeSourceIn::with_handle(
                    self.layer
                        .handle()
                        .expect("a scratch layer owns a texture handle"),
                ),
                Default::default(),
                ctx.graphics_pass_context(),
            ),
            TextureCompositeOutput {
                render_target: self.outer,
            },
        ));
    }
}

fn emit_child_mask_chunk(
    action: ArtifactSurfaceChildMaskAction,
    chunk: &PreparedArtifactSurfaceRasterChunk,
    graph: &mut FrameGraph,
    ctx: &mut UiBuildContext,
    scopes: &mut Vec<ChildMaskScope>,
) {
    let [super::PaintOp::DrawRect(mask)] = chunk.localized_ops() else {
        unreachable!("sealed child-mask chunk owns one localized rect")
    };
    match action {
        ArtifactSurfaceChildMaskAction::Push(scissor) => {
            let layer =
                (ctx.current_clip_id() == u8::MAX).then(|| ChildMaskLayer::open(graph, ctx));
            let parent_clip_id = ctx.current_clip_id();
            let child_clip_id = ctx
                .push_clip_id()
                .expect("a full stencil opens a scratch layer");
            let previous_scissor = ctx.push_graphics_pass_scissor(scissor);
            let mut pass = DrawRectPass::new(
                mask.params.clone(),
                DrawRectInput::default(),
                DrawRectOutput::default(),
            );
            pass.set_render_mode(mask.mode);
            pass.set_stencil_increment(parent_clip_id);
            pass.set_color_write_enabled(false);
            ctx.emit_draw_rect_pass(graph, pass);
            scopes.push(ChildMaskScope {
                owner: chunk.source().owner,
                clip_id: child_clip_id,
                previous_scissor,
                layer,
            });
        }
        ArtifactSurfaceChildMaskAction::Pop => {
            let ChildMaskScope {
                owner,
                clip_id: child_clip_id,
                previous_scissor,
                layer,
            } = scopes
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
            ctx.restore_graphics_pass_scissor(previous_scissor);
            if let Some(layer) = layer {
                layer.close(graph, ctx);
            }
        }
        ArtifactSurfaceChildMaskAction::Unchanged => {
            unreachable!("ordinary chunks never enter the child-mask emitter")
        }
    }
}

/// Apply a sealed terminal clip without re-deriving its relationship to the
/// incoming scissor. Preparation has already resolved the complete clip chain,
/// including whether Replace severs that external input.
fn replace_sealed_terminal_scissor(
    ctx: &mut UiBuildContext,
    scissor: GraphicsPassScissor,
) -> Option<GraphicsPassScissor> {
    ctx.replace_graphics_pass_scissor(Some(scissor))
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
        super::ArtifactSurfaceChunkClipSchedule::WholeChunk(
            ArtifactSurfaceResolvedClip::Unclipped,
        ) => emit(chunk.localized_ops(), graph, ctx),
        super::ArtifactSurfaceChunkClipSchedule::WholeChunk(
            ArtifactSurfaceResolvedClip::Scissor(scissor),
        ) => {
            let previous = replace_sealed_terminal_scissor(ctx, scissor);
            emit(chunk.localized_ops(), graph, ctx);
            ctx.restore_graphics_pass_scissor(previous);
        }
        super::ArtifactSurfaceChunkClipSchedule::WholeChunk(ArtifactSurfaceResolvedClip::Empty) => {
        }
        super::ArtifactSurfaceChunkClipSchedule::AfterShadowPrefix {
            prefix_op_count,
            suffix_clip,
        } => {
            let (prefix, suffix) = chunk.localized_ops().split_at(prefix_op_count);
            emit(prefix, graph, ctx);
            match suffix_clip {
                ArtifactSurfaceResolvedClip::Unclipped => emit(suffix, graph, ctx),
                ArtifactSurfaceResolvedClip::Scissor(scissor) => {
                    let previous = replace_sealed_terminal_scissor(ctx, scissor);
                    emit(suffix, graph, ctx);
                    ctx.restore_graphics_pass_scissor(previous);
                }
                ArtifactSurfaceResolvedClip::Empty => {}
            }
        }
    }
}

fn emit_artifact_span(
    span: &super::PreparedArtifactSurfaceRasterSpan,
    actions: &[ArtifactSurfaceChildMaskAction],
    graph: &mut FrameGraph,
    ctx: &mut UiBuildContext,
    scopes: &mut Vec<ChildMaskScope>,
) {
    assert_eq!(span.chunks().len(), actions.len());
    let before = ctx.opaque_rect_order();
    for (chunk, action) in span.chunks().iter().zip(actions.iter().copied()) {
        match action {
            ArtifactSurfaceChildMaskAction::Unchanged => emit_chunk_ops(chunk, graph, ctx),
            ArtifactSurfaceChildMaskAction::Push(_) | ArtifactSurfaceChildMaskAction::Pop => {
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
            if resolved_clip == ArtifactSurfaceResolvedClip::Empty {
                return;
            }
            let previous_scissor = match resolved_clip {
                ArtifactSurfaceResolvedClip::Scissor(scissor) => {
                    Some(replace_sealed_terminal_scissor(parent_ctx, scissor))
                }
                ArtifactSurfaceResolvedClip::Unclipped | ArtifactSurfaceResolvedClip::Empty => None,
            };
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
            if let Some(previous_scissor) = previous_scissor {
                parent_ctx.restore_graphics_pass_scissor(previous_scissor);
            }
        }
        PreparedArtifactSurfaceComposite::Layer {
            rect_pos,
            rect_size,
            opacity,
            source_physical_origin,
            resolved_clip,
        } => {
            if resolved_clip == ArtifactSurfaceResolvedClip::Empty {
                return;
            }
            let previous_scissor = match resolved_clip {
                ArtifactSurfaceResolvedClip::Scissor(scissor) => {
                    Some(replace_sealed_terminal_scissor(parent_ctx, scissor))
                }
                ArtifactSurfaceResolvedClip::Unclipped | ArtifactSurfaceResolvedClip::Empty => None,
            };
            graph.add_graphics_pass(CompositeLayerPass::new(
                CompositeLayerParams {
                    rect_pos,
                    rect_size,
                    corner_radii: [0.0; 4],
                    opacity,
                    scissor_rect: None,
                    clear_target: false,
                },
                CompositeLayerInput {
                    layer: LayerIn::with_handle(
                        layer_target
                            .handle()
                            .expect("artifact surface target owns a texture handle"),
                    ),
                    pass_context: parent_ctx.graphics_pass_context(),
                    source_physical_origin: Some(source_physical_origin),
                },
                CompositeLayerOutput {
                    render_target: parent_target,
                },
            ));
            if let Some(previous_scissor) = previous_scissor {
                parent_ctx.restore_graphics_pass_scissor(previous_scissor);
            }
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
        // Under a reused parent only a kept raster needs refreshing.
        if !composite_to_parent && action == RetainedSurfaceCompileAction::Transient {
            return parent_ctx.into_state();
        }
        let parent_target = parent_ctx
            .current_target()
            .expect("artifact surface has a receiver target");
        // The surface rasters at its own resolution; its commands draw there.
        let mut layer_ctx = UiBuildContext::from_parts(
            parent_ctx
                .viewport()
                .with_scale_factor(f32::from_bits(node.target().scale_factor_bits)),
            parent_ctx.layer_subtree_state_with_ancestor_clip(AncestorClipContext::default()),
        );
        let layer_target = match action {
            RetainedSurfaceCompileAction::Transient => {
                layer_ctx.allocate_transient_raster_target(graph, node.target().color.clone())
            }
            RetainedSurfaceCompileAction::Reraster | RetainedSurfaceCompileAction::Reuse => {
                layer_ctx.allocate_retained_raster_target(
                    graph,
                    node.target().color.clone(),
                    node.identity().color_key,
                    action == RetainedSurfaceCompileAction::Reraster,
                )
            }
        };
        layer_ctx.set_current_target(layer_target);
        if action != RetainedSurfaceCompileAction::Reuse {
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
            || node.geometry().resolved_receiver_clip() == ArtifactSurfaceResolvedClip::Empty
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
        scopes: &mut Vec<ChildMaskScope>,
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
/// resident set the surface cache keeps. Pool canonicality must be established
/// before target allocation because persistent color allocation otherwise
/// contains a depth-key `expect`.
///
/// Without an active frame owner the surface cache is unavailable: every
/// surface is drawn into a frame-local target and nothing is staged. A surface
/// whose persistent key the graph already declared is drawn the same way.
pub(crate) fn emit_prepared_artifact_surface_frame_from_pool(
    viewport: &mut Viewport,
    owner: Option<RetainedSurfaceFrameStageOwner>,
    frame: PreparedArtifactSurfaceFrame,
    graph: &mut FrameGraph,
    ctx: UiBuildContext,
) -> BuildState {
    emit_prepared_artifact_surface_frame(viewport, owner, frame, graph, ctx, false).0
}

fn emit_prepared_artifact_surface_frame(
    viewport: &mut Viewport,
    owner: Option<RetainedSurfaceFrameStageOwner>,
    frame: PreparedArtifactSurfaceFrame,
    graph: &mut FrameGraph,
    mut ctx: UiBuildContext,
    allow_forced_pair_witness: bool,
) -> (BuildState, Vec<RetainedSurfaceCompileAction>) {
    let _profile = crate::view::paint::work_profile::scope("emit_prepared_artifact_surface_frame");
    let owner = owner.filter(|owner| viewport.retained_surface_frame_stage_owner_is_active(*owner));
    let execution = seal_prepared_artifact_surface_execution(frame);
    let PreparedArtifactSurfaceExecution {
        frame,
        root_programs,
        node_programs,
    } = execution;
    let (plan, residents) = frame.into_parts();

    #[cfg(test)]
    let pool_emission = if allow_forced_pair_witness {
        viewport.prepare_artifact_surface_pool_emission_for_forced_test(residents)
    } else {
        viewport.prepare_artifact_surface_pool_emission_from_pool(residents)
    };
    #[cfg(not(test))]
    let pool_emission = {
        let _ = allow_forced_pair_witness;
        viewport.prepare_artifact_surface_pool_emission_from_pool(residents)
    };
    let residents = pool_emission.residents();
    let ordered_actions = pool_emission.ordered_actions();
    assert_eq!(ordered_actions.len(), residents.len());
    let declared = graph
        .declared_persistent_texture_keys()
        .collect::<FxHashSet<_>>();
    let actions = ordered_actions
        .iter()
        .zip(residents.ordered_entries())
        .zip(plan.nodes())
        .map(|(((key, action), resident), node)| {
            assert_eq!(*key, resident.resident_key());
            let color = resident.stamp().identity.color_key;
            // A key without a depth role cannot be kept: the surface draws
            // for this frame only.
            let cached = owner.is_some()
                && node.retained()
                && !declared.contains(&color)
                && color
                    .depth_stencil()
                    .is_some_and(|depth| !declared.contains(&depth));
            if cached {
                *action
            } else {
                RetainedSurfaceCompileAction::Transient
            }
        })
        .collect::<Vec<_>>();
    #[cfg(any(test, feature = "renderer-test-support"))]
    LAST_PRODUCTION_ACTIONS.with(|observed| observed.replace(actions.clone()));
    let composites = plan
        .nodes()
        .iter()
        .map(|node| {
            // Transform nodes composite their frozen quad. Do not re-evaluate
            // a matrix against normalized source coordinates here.
            prepare_composite_geometry(node.geometry(), node.raster_origin)
                .expect("prepared artifact geometry has a typed render-pass projection")
        })
        .collect::<Vec<_>>();

    graph.set_transient_gpu_paint_sources(plan.transient_gpu_sources().clone());
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
    // does not protect the staging slot across emission. The owner must stay
    // active and the pending slot empty.
    let mut transient = actions
        .iter()
        .map(|action| *action == RetainedSurfaceCompileAction::Transient);
    let residents = pool_emission
        .into_residents()
        .retaining(|_| !transient.next().expect("one action per resident"));
    if viewport.surface_diagnostics_requested() {
        let diagnostics = surface_diagnostics(viewport, &plan, &actions);
        viewport.record_surface_diagnostics(diagnostics);
    }

    if let Some(owner) = owner {
        assert!(
            viewport.stage_artifact_surface_resident_set(owner, residents),
            "preflighted artifact resident transaction must stage after emission"
        );
    }
    (ctx.into_state(), actions)
}

/// Each surface with its action, then each GPU source that paints this frame
/// with the action its preparation will take. Sources inside a reused surface
/// do not run and are left out.
fn surface_diagnostics(
    viewport: &Viewport,
    plan: &PreparedArtifactSurfaceRasterPlan,
    actions: &[RetainedSurfaceCompileAction],
) -> Vec<crate::view::viewport::SurfaceDiagnostic> {
    use crate::view::debug::DebugSurfaceKind;
    let mut diagnostics = plan
        .nodes()
        .iter()
        .zip(actions)
        .map(|(node, action)| {
            let identity = node.identity();
            crate::view::viewport::SurfaceDiagnostic {
                owner: identity.boundary_root,
                kind: match identity.role {
                    super::RetainedSurfaceRasterRole::Transform => DebugSurfaceKind::Transform,
                    super::RetainedSurfaceRasterRole::PropertyEffect => DebugSurfaceKind::Effect,
                    super::RetainedSurfaceRasterRole::ScrollContent => {
                        DebugSurfaceKind::ScrollContent
                    }
                },
                action: *action,
                key: identity.color_key,
                desc: node.target().color.clone(),
                scale_factor: f32::from_bits(node.target().scale_factor_bits),
            }
        })
        .collect::<Vec<_>>();
    let painted_steps = plan.roots().iter().map(|root| root.steps()).chain(
        plan.nodes()
            .iter()
            .zip(actions)
            .filter(|(_, action)| **action != RetainedSurfaceCompileAction::Reuse)
            .map(|(node, _)| node.steps()),
    );
    let mut seen = FxHashSet::default();
    for step in painted_steps.flatten() {
        let PreparedArtifactSurfaceRasterStep::ArtifactSpan(span) = step else {
            continue;
        };
        for chunk in span.chunks() {
            for op in chunk.localized_ops() {
                let super::super::PaintOp::PreparedGpu(op) = op else {
                    continue;
                };
                let id = op.source.id.get();
                if !seen.insert(id) {
                    continue;
                }
                let retain = !plan.transient_gpu_sources().contains(&id);
                diagnostics.push(crate::view::viewport::SurfaceDiagnostic {
                    owner: chunk.source().owner,
                    kind: DebugSurfaceKind::GpuSource,
                    action: viewport.planned_gpu_paint_source_action(&op.source, retain),
                    key: op.source.key(),
                    desc: op.source.descriptor(viewport.max_texture_dimension_2d()),
                    scale_factor: viewport.scale_factor(),
                });
            }
        }
    }
    diagnostics
}

#[cfg(test)]
fn emit_prepared_artifact_surface_frame_for_forced_test(
    viewport: &mut Viewport,
    owner: Option<RetainedSurfaceFrameStageOwner>,
    frame: PreparedArtifactSurfaceFrame,
    graph: &mut FrameGraph,
    ctx: UiBuildContext,
) -> (BuildState, Vec<RetainedSurfaceCompileAction>) {
    emit_prepared_artifact_surface_frame(viewport, owner, frame, graph, ctx, true)
}

#[cfg(test)]
mod tests;
