//! Direct command fixtures for low-level paint tests. This code has no
//! production selector, retained planner, residency, or reuse transaction.
//! Retirement acceptance must use the production Viewport and independent
//! expected pixels, never this direct compiler as its geometry/pixel oracle.
use super::*;
use crate::view::base_component::{AncestorClipContext, BuildState};
use crate::view::render_pass::ClearPass;
use crate::view::render_pass::composite_layer_pass::{
    CompositeLayerInput, CompositeLayerOutput, CompositeLayerParams, CompositeLayerPass, LayerIn,
};

pub(crate) struct ArtifactCompileError {
    kind: ArtifactCompileErrorKind,
    state: BuildState,
}

impl ArtifactCompileError {
    pub(crate) fn kind(&self) -> ArtifactCompileErrorKind {
        self.kind
    }

    fn into_state(self) -> BuildState {
        self.state
    }
}

/// Test-only direct command compilation for payload and clip unit tests.
pub(crate) fn try_compile_artifact(
    artifact: &PaintArtifact,
    graph: &mut FrameGraph,
    mut ctx: UiBuildContext,
) -> Result<BuildState, ArtifactCompileError> {
    let Some(validated) = validate_artifact_store(artifact) else {
        return Err(ArtifactCompileError {
            kind: ArtifactCompileErrorKind::InvalidStore,
            state: ctx.into_state(),
        });
    };
    #[cfg(test)]
    ARTIFACT_COMPILE_COUNT.with(|count| count.set(count.get().saturating_add(1)));
    match validated.target {
        ValidatedArtifactTarget::CurrentTarget => {
            compile_validated_artifact(artifact, validated.resolved_clips, graph, &mut ctx)
        }
        ValidatedArtifactTarget::RootOpacityGroup { root, effect } => compile_root_opacity_group(
            artifact,
            validated.resolved_clips,
            root,
            effect,
            graph,
            &mut ctx,
        ),
    }
    Ok(ctx.into_state())
}

fn compile_root_opacity_group(
    artifact: &PaintArtifact,
    resolved_clips: Vec<ResolvedClip>,
    root: crate::view::node_arena::NodeKey,
    effect: EffectNodeSnapshot,
    graph: &mut FrameGraph,
    ctx: &mut UiBuildContext,
) {
    let parent_target = ctx.current_target().unwrap_or_else(|| {
        let target = ctx.allocate_target(graph);
        ctx.set_current_target(target);
        target
    });
    let mut layer_ctx = UiBuildContext::from_parts(
        ctx.viewport(),
        ctx.layer_subtree_state_with_ancestor_clip(AncestorClipContext::default()),
    );
    let layer_target = layer_ctx.allocate_persistent_full_viewport_target(
        graph,
        crate::view::base_component::root_effect_stable_key(root),
    );
    layer_ctx.set_current_target(layer_target);
    {
        graph.add_graphics_pass(ClearPass::new(
            crate::view::render_pass::clear_pass::ClearParams::new([0.0, 0.0, 0.0, 0.0]),
            crate::view::render_pass::clear_pass::ClearInput {
                pass_context: layer_ctx.graphics_pass_context(),
                clear_depth_stencil: true,
            },
            crate::view::render_pass::clear_pass::ClearOutput {
                render_target: layer_target,
            },
        ));
        compile_validated_artifact(artifact, resolved_clips, graph, &mut layer_ctx);
    }
    let layer_state = layer_ctx.into_state();
    ctx.merge_child_render_state(&layer_state);
    ctx.set_current_target(parent_target);

    let viewport = ctx.viewport();
    let scale = viewport.scale_factor().max(0.0001);
    graph.add_graphics_pass(CompositeLayerPass::new(
        CompositeLayerParams {
            rect_pos: [0.0, 0.0],
            rect_size: [
                viewport.target_width() as f32 / scale,
                viewport.target_height() as f32 / scale,
            ],
            corner_radii: [0.0; 4],
            opacity: effect.opacity,
            scissor_rect: None,
            clear_target: false,
        },
        CompositeLayerInput {
            layer: LayerIn::with_handle(
                layer_target
                    .handle()
                    .expect("persistent root opacity target must have a texture handle"),
            ),
            pass_context: ctx.graphics_pass_context(),
            source_physical_origin: None,
        },
        CompositeLayerOutput {
            render_target: parent_target,
        },
    ));
    ctx.set_current_target(parent_target);
}

#[cfg(test)]
pub(crate) fn compile_artifact(
    artifact: &PaintArtifact,
    graph: &mut FrameGraph,
    ctx: UiBuildContext,
) -> BuildState {
    match try_compile_artifact(artifact, graph, ctx) {
        Ok(state) => state,
        Err(error) => error.into_state(),
    }
}

fn compile_validated_artifact(
    artifact: &PaintArtifact,
    resolved_clips: Vec<ResolvedClip>,
    graph: &mut FrameGraph,
    ctx: &mut UiBuildContext,
) {
    let mut child_mask_scopes = Vec::new();
    compile_validated_artifact_segment(
        artifact,
        resolved_clips,
        graph,
        ctx,
        &mut child_mask_scopes,
    );
    debug_assert!(child_mask_scopes.is_empty());
}

fn compile_validated_artifact_segment(
    artifact: &PaintArtifact,
    resolved_clips: Vec<ResolvedClip>,
    graph: &mut FrameGraph,
    ctx: &mut UiBuildContext,
    child_mask_scopes: &mut Vec<(crate::view::node_arena::NodeKey, u8, Option<[u32; 4]>)>,
) {
    let max_mask_depth = artifact_surface_executor::artifact_child_mask_max_depth(
        artifact.chunks.iter().map(|chunk| chunk.id),
        child_mask_scopes.len(),
    );
    if ctx.current_clip_id() as usize + max_mask_depth > u8::MAX as usize {
        return;
    }
    for (chunk, resolved_clip) in artifact.chunks.iter().zip(resolved_clips) {
        let _observed_identity = (&chunk.bounds, chunk.properties, chunk.content_revision);
        if chunk.id.slot == super::super::RETAINED_CHILD_MASK_SLOT {
            let [PaintOp::DrawRect(mask)] = &artifact.ops[chunk.op_range.clone()] else {
                unreachable!("validated retained child-mask chunk owns one rect")
            };
            match chunk.id.phase {
                super::super::PaintNodePhase::BeforeChildren => {
                    let parent_clip_id = ctx.current_clip_id();
                    let child_clip_id = ctx
                        .push_clip_id()
                        .expect("validated retained child-mask depth");
                    let logical_scissor =
                        crate::view::base_component::exact_logical_scissor_for_rect(chunk.bounds)
                            .expect("validated retained child-mask scissor");
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
                    child_mask_scopes.push((chunk.owner, child_clip_id, previous_scissor));
                }
                super::super::PaintNodePhase::AfterChildren => {
                    let (owner, child_clip_id, previous_scissor) = child_mask_scopes
                        .pop()
                        .expect("validated retained child-mask pairing");
                    debug_assert_eq!(owner, chunk.owner);
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
            }
            continue;
        }
        let shadow_prefix_len = exact_self_clip_shadow_prefix_len(artifact, chunk).unwrap_or(0);
        let previous_scissor = match (shadow_prefix_len, resolved_clip) {
            (0, ResolvedClip::Unclipped) => None,
            (0, ResolvedClip::Scissor(scissor)) => Some(ctx.replace_scissor_rect(Some(scissor))),
            // Do not enter a graphics scope for an empty clip: in particular,
            // opaque rectangles must not consume DFS depth order.
            (0, ResolvedClip::Empty) => continue,
            // Exact self-clip shadow grammar emits the outer-shadow prefix
            // against the incoming parent scissor. The owner's Replace clip
            // begins only at decoration/media.
            (_, _) => None,
        };
        let mut split_previous_scissor = None;
        for (op_index, op) in artifact.ops[chunk.op_range.clone()].iter().enumerate() {
            if shadow_prefix_len != 0 && op_index == shadow_prefix_len {
                match resolved_clip {
                    ResolvedClip::Unclipped => {}
                    ResolvedClip::Scissor(scissor) => {
                        split_previous_scissor = Some(ctx.replace_scissor_rect(Some(scissor)));
                    }
                    ResolvedClip::Empty => break,
                }
            }
            emit_artifact_surface_paint_op(op, graph, ctx);
        }
        if let Some(previous) = split_previous_scissor.or(previous_scissor) {
            ctx.restore_scissor_rect(previous);
        }
    }
}

fn validate_artifact_store(artifact: &PaintArtifact) -> Option<ValidatedArtifact> {
    validate_artifact_store_with_policy(artifact, ArtifactStoreValidationPolicy::General)
}
