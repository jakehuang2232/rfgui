//! Typed retained transform-surface preparation and emission.
//!
//! Preparation validates the complete plan, artifact, descriptor pair, parent
//! target, and persistent-key contract without mutating the frame graph. Only
//! the opaque token returned here can reach emission, so emission has no late
//! rejection path.

use crate::view::base_component::{
    AncestorClipContext, BuildState, UiBuildContext, persistent_target_texture_descriptors,
    texture_desc_for_logical_bounds, transformed_layer_stable_key,
};
use crate::view::frame_graph::texture_resource::TextureDesc;
use crate::view::frame_graph::{FrameGraph, PersistentTextureKey};
use crate::view::render_pass::composite_layer_pass::{
    CompositeLayerInput, CompositeLayerOutput, CompositeLayerParams, CompositeLayerPass, LayerIn,
};
use crate::view::render_pass::draw_rect_pass::RenderTargetOut;
use crate::view::render_pass::texture_composite_pass::{
    TextureCompositeInput, TextureCompositeOutput, TextureCompositeSourceIn,
};
use crate::view::render_pass::{ClearPass, TextureCompositePass};
use crate::view::viewport::Viewport;
use rustc_hash::{FxHashMap, FxHashSet};
use slotmap::Key;

use super::compiler::{
    ValidatedBakedScrollHostArtifact, ValidatedEffectPropertySurfaceArtifact,
    ValidatedIsolationSurfaceArtifact, ValidatedTransformSurfaceArtifact,
    emit_validated_baked_scroll_host_artifact, emit_validated_isolation_surface_artifact,
    emit_validated_transform_surface_artifact, validate_baked_scroll_host_artifact,
    validate_isolation_surface_artifact, validate_transform_surface_artifact,
};
use super::frame_plan::opaque_order_count;
use super::{
    ArtifactSpanPlan, FramePaintPlan, NestedSurfaceRasterDependency, PaintArtifactTarget,
    PaintPlanStep, RetainedScrollHostRasterDependency, RetainedSurfaceCompileAction,
    RetainedSurfaceCompositeGeometryStamp, RetainedSurfacePlan, RetainedSurfaceRasterInputs,
    RetainedSurfaceRasterRole, RetainedSurfaceRasterStamp, RetainedSurfaceRasterStepStamp,
    SurfaceKind, retained_isolation_composite_geometry_stamp,
    retained_nested_isolation_composite_geometry_stamp, retained_surface_composite_geometry_stamp,
    validated_isolation_surface_artifact_span_stamp,
    validated_retained_surface_artifact_span_stamp, validated_retained_surface_tree_raster_stamp,
    validated_scroll_host_artifact_span_stamp, validated_scroll_host_raster_stamp,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RetainedSurfacePrepareError {
    PlanShape,
    NestedSurface,
    BoundaryIdentity,
    GeometryContract,
    ContextMismatch,
    OpaqueSpan,
    ArtifactTarget,
    ArtifactStore,
    ParentTarget,
    PersistentKeyAlreadyDeclared(PersistentTextureKey),
    DescriptorPair,
    DuplicateSurfaceIdentity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RetainedSurfaceTreePolicy {
    Singleton,
    ScrollHostSingleton,
    TransformChildTransform,
    TransformChildNestedIsolation,
    PropertyScene,
    #[cfg(test)]
    ForcedDepthTwo,
}

impl RetainedSurfaceTreePolicy {
    fn accepts_surface(self, kind: &SurfaceKind, depth: usize) -> bool {
        match self {
            Self::Singleton => depth == 0 && !matches!(kind, SurfaceKind::NestedIsolation(_)),
            Self::ScrollHostSingleton => depth == 0 && matches!(kind, SurfaceKind::ScrollHost(_)),
            Self::TransformChildTransform => match depth {
                0 | 1 => matches!(kind, SurfaceKind::Transform(_)),
                _ => false,
            },
            Self::TransformChildNestedIsolation => match depth {
                0 => matches!(kind, SurfaceKind::Transform(_)),
                1 => matches!(kind, SurfaceKind::NestedIsolation(_)),
                _ => false,
            },
            Self::PropertyScene => matches!(
                kind,
                SurfaceKind::Transform(_)
                    | SurfaceKind::NestedIsolation(super::frame_plan::NestedIsolationSurfacePlan {
                        property_scene: Some(_),
                        ..
                    })
            ),
            #[cfg(test)]
            Self::ForcedDepthTwo => match depth {
                0 => !matches!(kind, SurfaceKind::NestedIsolation(_)),
                1 => matches!(
                    kind,
                    SurfaceKind::Transform(_) | SurfaceKind::NestedIsolation(_)
                ),
                _ => false,
            },
        }
    }
}

/// Opaque authority token. Its fields stay private so no caller can fabricate
/// a validated artifact, descriptor, or raster stamp.
struct PreparedFramePaintPlan<'a> {
    root: PreparedSurface<'a>,
    parent_target: Option<RenderTargetOut>,
}

struct PreparedPropertyScene<'a> {
    steps: Vec<PreparedPropertySceneStep<'a>>,
    transaction_witness: super::frame_plan::PropertySceneTransactionWitness,
    parent_target: Option<RenderTargetOut>,
}

enum PreparedPropertySceneStep<'a> {
    ArtifactSpan(PreparedPropertySceneArtifactSpan<'a>),
    RetainedSurface(PreparedSurface<'a>),
}

struct PreparedPropertySceneArtifactSpan<'a> {
    plan: &'a ArtifactSpanPlan,
    artifact: super::compiler::ValidatedPropertySceneArtifact<'a>,
}

struct PreparedSurface<'a> {
    surface: &'a RetainedSurfacePlan,
    raster_steps: Vec<PreparedSurfaceStep<'a>>,
    color_key: PersistentTextureKey,
    color_desc: TextureDesc,
    stamp: RetainedSurfaceRasterStamp,
    action: Option<RetainedSurfaceCompileAction>,
}

enum PreparedSurfaceStep<'a> {
    ArtifactSpan(PreparedArtifactSpan<'a>),
    RetainedSurface(PreparedNestedSurface<'a>),
}

struct PreparedNestedSurface<'a> {
    child: Box<PreparedSurface<'a>>,
    parent_opaque_order_before: u32,
    parent_opaque_order_after: u32,
}

struct PreparedArtifactSpan<'a> {
    plan: &'a ArtifactSpanPlan,
    artifact: PreparedValidatedArtifact<'a>,
}

enum PreparedValidatedArtifact<'a> {
    Transform(ValidatedTransformSurfaceArtifact<'a>),
    TransformProperty(super::compiler::ValidatedTransformPropertySurfaceArtifact<'a>),
    Isolation(ValidatedIsolationSurfaceArtifact<'a>),
    EffectProperty(ValidatedEffectPropertySurfaceArtifact<'a>),
    ScrollHost(ValidatedBakedScrollHostArtifact<'a>),
}

impl PreparedFramePaintPlan<'_> {
    fn freeze_actions(
        &mut self,
        mut actions: FxHashMap<super::RetainedSurfaceResidentKey, RetainedSurfaceCompileAction>,
    ) {
        self.root.freeze_actions(&mut actions);
        assert!(
            actions.is_empty(),
            "the viewport action provider must cover the prepared full set exactly"
        );
    }

    fn stamps(&self) -> Vec<&RetainedSurfaceRasterStamp> {
        let mut stamps = Vec::new();
        self.root.collect_stamps(&mut stamps);
        stamps
    }
}

impl PreparedPropertyScene<'_> {
    fn recomputed_aggregate_opaque_order_span(&self) -> std::ops::Range<u32> {
        let mut cursor = 0_u32;
        for step in &self.steps {
            match step {
                PreparedPropertySceneStep::ArtifactSpan(span) => {
                    cursor = span.plan.opaque_order_span().end;
                }
                PreparedPropertySceneStep::RetainedSurface(surface) => {
                    if !surface_is_property_effect(surface.surface) {
                        cursor = cursor.max(surface.stamp.opaque_order_span.end);
                    }
                }
            }
        }
        0..cursor
    }

    fn freeze_actions(
        &mut self,
        mut actions: FxHashMap<super::RetainedSurfaceResidentKey, RetainedSurfaceCompileAction>,
    ) {
        for step in &mut self.steps {
            if let PreparedPropertySceneStep::RetainedSurface(surface) = step {
                surface.freeze_actions(&mut actions);
            }
        }
        assert!(
            actions.is_empty(),
            "the viewport action provider must cover the prepared property scene exactly"
        );
    }

    fn stamps(&self) -> Vec<&RetainedSurfaceRasterStamp> {
        let mut stamps = Vec::new();
        for step in &self.steps {
            if let PreparedPropertySceneStep::RetainedSurface(surface) = step {
                surface.collect_stamps(&mut stamps);
            }
        }
        stamps
    }

    fn collect_traces(&self) -> Vec<RetainedSurfaceBuildTrace> {
        let mut traces = Vec::new();
        for step in &self.steps {
            if let PreparedPropertySceneStep::RetainedSurface(surface) = step {
                surface.collect_traces(&mut traces);
            }
        }
        traces
    }
}

impl PreparedSurface<'_> {
    fn stamp(&self) -> &RetainedSurfaceRasterStamp {
        &self.stamp
    }

    fn action(&self) -> RetainedSurfaceCompileAction {
        self.action
            .expect("prepared surface actions are frozen before graph mutation")
    }

    fn collect_stamps<'a>(&'a self, stamps: &mut Vec<&'a RetainedSurfaceRasterStamp>) {
        stamps.push(&self.stamp);
        for step in &self.raster_steps {
            if let PreparedSurfaceStep::RetainedSurface(nested) = step {
                nested.child.collect_stamps(stamps);
            }
        }
    }

    fn freeze_actions(
        &mut self,
        actions: &mut FxHashMap<super::RetainedSurfaceResidentKey, RetainedSurfaceCompileAction>,
    ) {
        assert!(
            self.action.is_none(),
            "surface action can only be frozen once"
        );
        self.action = Some(
            actions
                .remove(&self.stamp.identity.resident_key())
                .expect("viewport action provider omitted a prepared surface"),
        );
        for step in &mut self.raster_steps {
            if let PreparedSurfaceStep::RetainedSurface(nested) = step {
                nested.child.freeze_actions(actions);
            }
        }
    }

    fn collect_traces(&self, traces: &mut Vec<RetainedSurfaceBuildTrace>) {
        traces.push(RetainedSurfaceBuildTrace::from_prepared(self));
        for step in &self.raster_steps {
            if let PreparedSurfaceStep::RetainedSurface(nested) = step {
                nested.child.collect_traces(traces);
            }
        }
    }
}

#[cfg(test)]
pub(crate) fn prepare_forced_retained_surface_stamp_for_test(
    plan: &FramePaintPlan,
    graph: &FrameGraph,
    ctx: &UiBuildContext,
) -> Result<RetainedSurfaceRasterStamp, RetainedSurfacePrepareError> {
    Ok(prepare_frame_paint_plan_forced(plan, graph, ctx)?
        .root
        .stamp)
}

#[cfg(test)]
pub(crate) fn prepare_retained_scroll_host_stamp_for_test(
    plan: &FramePaintPlan,
    graph: &FrameGraph,
    ctx: &UiBuildContext,
) -> Result<RetainedSurfaceRasterStamp, RetainedSurfacePrepareError> {
    Ok(prepare_retained_scroll_host_surface(plan, graph, ctx)?
        .root
        .stamp)
}

fn prepare_retained_surface<'a>(
    plan: &'a FramePaintPlan,
    graph: &FrameGraph,
    ctx: &UiBuildContext,
) -> Result<PreparedFramePaintPlan<'a>, RetainedSurfacePrepareError> {
    let [PaintPlanStep::RetainedSurface(root)] = plan.steps() else {
        return Err(RetainedSurfacePrepareError::PlanShape);
    };
    if !matches!(root.kind(), SurfaceKind::Transform(_)) {
        return Err(RetainedSurfacePrepareError::PlanShape);
    }
    prepare_frame_paint_plan(plan, graph, ctx, RetainedSurfaceTreePolicy::Singleton)
}

fn prepare_retained_isolation_surface<'a>(
    plan: &'a FramePaintPlan,
    graph: &FrameGraph,
    ctx: &UiBuildContext,
) -> Result<PreparedFramePaintPlan<'a>, RetainedSurfacePrepareError> {
    let [PaintPlanStep::RetainedSurface(root)] = plan.steps() else {
        return Err(RetainedSurfacePrepareError::PlanShape);
    };
    if !matches!(
        (root.kind(), root.raster_steps()),
        (SurfaceKind::Isolation(_), [PaintPlanStep::ArtifactSpan(_)])
    ) {
        return Err(RetainedSurfacePrepareError::PlanShape);
    }
    prepare_frame_paint_plan(plan, graph, ctx, RetainedSurfaceTreePolicy::Singleton)
}

fn prepare_retained_scroll_host_surface<'a>(
    plan: &'a FramePaintPlan,
    graph: &FrameGraph,
    ctx: &UiBuildContext,
) -> Result<PreparedFramePaintPlan<'a>, RetainedSurfacePrepareError> {
    let [PaintPlanStep::RetainedSurface(root)] = plan.steps() else {
        return Err(RetainedSurfacePrepareError::PlanShape);
    };
    if !matches!(
        (root.kind(), root.raster_steps()),
        (SurfaceKind::ScrollHost(_), [PaintPlanStep::ArtifactSpan(_)])
    ) {
        return Err(RetainedSurfacePrepareError::PlanShape);
    }
    prepare_frame_paint_plan(
        plan,
        graph,
        ctx,
        RetainedSurfaceTreePolicy::ScrollHostSingleton,
    )
}

fn prepare_retained_surface_tree<'a>(
    plan: &'a FramePaintPlan,
    graph: &FrameGraph,
    ctx: &UiBuildContext,
) -> Result<PreparedFramePaintPlan<'a>, RetainedSurfacePrepareError> {
    if !is_exact_depth_two_tree_shape(plan) {
        return Err(RetainedSurfacePrepareError::PlanShape);
    }
    prepare_frame_paint_plan(
        plan,
        graph,
        ctx,
        RetainedSurfaceTreePolicy::TransformChildTransform,
    )
}

fn is_exact_depth_two_tree_shape(plan: &FramePaintPlan) -> bool {
    let [PaintPlanStep::RetainedSurface(root)] = plan.steps() else {
        return false;
    };
    if !matches!(root.kind(), SurfaceKind::Transform(_)) {
        return false;
    }
    let mut children = root.raster_steps().iter().filter_map(|step| match step {
        PaintPlanStep::RetainedSurface(child) => Some(child.as_ref()),
        PaintPlanStep::ArtifactSpan(_) => None,
    });
    let Some(child) = children.next() else {
        return false;
    };
    children.next().is_none()
        && matches!(child.kind(), SurfaceKind::Transform(_))
        && child.parent_surface() == Some(root.boundary_root())
        && matches!(child.raster_steps(), [PaintPlanStep::ArtifactSpan(_)])
}

fn prepare_retained_effect_tree<'a>(
    plan: &'a FramePaintPlan,
    graph: &FrameGraph,
    ctx: &UiBuildContext,
) -> Result<PreparedFramePaintPlan<'a>, RetainedSurfacePrepareError> {
    if !is_exact_transform_child_nested_isolation_shape(plan) {
        return Err(RetainedSurfacePrepareError::PlanShape);
    }
    prepare_frame_paint_plan(
        plan,
        graph,
        ctx,
        RetainedSurfaceTreePolicy::TransformChildNestedIsolation,
    )
}

fn is_exact_transform_child_nested_isolation_shape(plan: &FramePaintPlan) -> bool {
    let [PaintPlanStep::RetainedSurface(root)] = plan.steps() else {
        return false;
    };
    if !matches!(root.kind(), SurfaceKind::Transform(_)) {
        return false;
    }
    let mut children = root.raster_steps().iter().filter_map(|step| match step {
        PaintPlanStep::RetainedSurface(child) => Some(child.as_ref()),
        PaintPlanStep::ArtifactSpan(_) => None,
    });
    let Some(child) = children.next() else {
        return false;
    };
    children.next().is_none()
        && matches!(child.kind(), SurfaceKind::NestedIsolation(_))
        && child.parent_surface() == Some(root.boundary_root())
        && matches!(child.raster_steps(), [PaintPlanStep::ArtifactSpan(_)])
}

#[cfg(test)]
fn prepare_frame_paint_plan_forced<'a>(
    plan: &'a FramePaintPlan,
    graph: &FrameGraph,
    ctx: &UiBuildContext,
) -> Result<PreparedFramePaintPlan<'a>, RetainedSurfacePrepareError> {
    prepare_frame_paint_plan(plan, graph, ctx, RetainedSurfaceTreePolicy::ForcedDepthTwo)
}

fn prepare_frame_paint_plan<'a>(
    plan: &'a FramePaintPlan,
    graph: &FrameGraph,
    ctx: &UiBuildContext,
    policy: RetainedSurfaceTreePolicy,
) -> Result<PreparedFramePaintPlan<'a>, RetainedSurfacePrepareError> {
    let [PaintPlanStep::RetainedSurface(surface)] = plan.steps() else {
        return Err(RetainedSurfacePrepareError::PlanShape);
    };
    if surface.parent_surface().is_some() {
        return Err(RetainedSurfacePrepareError::NestedSurface);
    }
    if !policy.accepts_surface(surface.kind(), 0) {
        return Err(RetainedSurfacePrepareError::PlanShape);
    }
    if matches!(
        policy,
        RetainedSurfaceTreePolicy::Singleton | RetainedSurfaceTreePolicy::ScrollHostSingleton
    ) && !matches!(surface.raster_steps(), [PaintPlanStep::ArtifactSpan(_)])
    {
        return Err(RetainedSurfacePrepareError::NestedSurface);
    }
    validate_parent_target(graph, ctx)?;
    let mut seen_roots = FxHashSet::default();
    let mut seen_stable_ids = FxHashSet::default();
    let mut seen_persistent_keys = FxHashSet::default();
    let root = prepare_surface(
        surface,
        graph,
        ctx,
        0,
        None,
        policy,
        &mut seen_roots,
        &mut seen_stable_ids,
        &mut seen_persistent_keys,
    )?;
    Ok(PreparedFramePaintPlan {
        root,
        parent_target: ctx.current_target(),
    })
}

fn prepare_property_scene<'a>(
    plan: &'a FramePaintPlan,
    graph: &FrameGraph,
    ctx: &UiBuildContext,
) -> Result<PreparedPropertyScene<'a>, RetainedSurfacePrepareError> {
    let transaction_witness = plan
        .property_scene_transaction_witness()
        .ok_or(RetainedSurfacePrepareError::PlanShape)?;
    if plan
        .property_scene_context()
        .is_none_or(|context| !context.matches_ui_context(ctx))
    {
        return Err(RetainedSurfacePrepareError::ContextMismatch);
    }
    validate_parent_target(graph, ctx)?;
    let mut seen_roots = FxHashSet::default();
    let mut seen_stable_ids = FxHashSet::default();
    let mut seen_persistent_keys = FxHashSet::default();
    let mut cursor = 0_u32;
    let mut steps = Vec::with_capacity(plan.steps().len());
    for step in plan.steps() {
        match step {
            PaintPlanStep::ArtifactSpan(span) => {
                if span.opaque_order_span().start != cursor {
                    return Err(RetainedSurfacePrepareError::OpaqueSpan);
                }
                let expected_end = cursor
                    .checked_add(opaque_order_count(span.artifact()))
                    .ok_or(RetainedSurfacePrepareError::OpaqueSpan)?;
                if span.opaque_order_span().end != expected_end {
                    return Err(RetainedSurfacePrepareError::OpaqueSpan);
                }
                let artifact = super::compiler::validate_property_scene_artifact(span.artifact())
                    .ok_or(RetainedSurfacePrepareError::ArtifactStore)?;
                cursor = expected_end;
                steps.push(PreparedPropertySceneStep::ArtifactSpan(
                    PreparedPropertySceneArtifactSpan {
                        plan: span,
                        artifact,
                    },
                ));
            }
            PaintPlanStep::RetainedSurface(surface) => {
                if surface.parent_surface().is_some() {
                    return Err(RetainedSurfacePrepareError::NestedSurface);
                }
                let prepared = prepare_surface(
                    surface,
                    graph,
                    ctx,
                    0,
                    None,
                    RetainedSurfaceTreePolicy::PropertyScene,
                    &mut seen_roots,
                    &mut seen_stable_ids,
                    &mut seen_persistent_keys,
                )?;
                if !surface_is_property_effect(prepared.surface) {
                    cursor = cursor.max(prepared.stamp.opaque_order_span.end);
                }
                steps.push(PreparedPropertySceneStep::RetainedSurface(prepared));
            }
        }
    }
    if transaction_witness.aggregate_opaque_order_span != (0..cursor)
        || seen_roots.len() != transaction_witness.surfaces.len()
    {
        return Err(RetainedSurfacePrepareError::PlanShape);
    }
    Ok(PreparedPropertyScene {
        steps,
        transaction_witness,
        parent_target: ctx.current_target(),
    })
}

fn validate_parent_target(
    graph: &FrameGraph,
    ctx: &UiBuildContext,
) -> Result<(), RetainedSurfacePrepareError> {
    let viewport = ctx.viewport();
    if let Some(handle) = ctx.current_target().and_then(|target| target.handle()) {
        let Some(desc) = graph.texture_desc_for_handle(handle) else {
            return Err(RetainedSurfacePrepareError::ParentTarget);
        };
        if !graph.contains_texture_handle(handle)
            || desc.width() == 0
            || desc.height() == 0
            || desc.format() != viewport.target_format()
            || desc.dimension() != wgpu::TextureDimension::D2
            || desc.sample_count() != 1
            || !desc
                .usage()
                .contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
        {
            return Err(RetainedSurfacePrepareError::ParentTarget);
        }
    }
    Ok(())
}

fn surface_is_property_effect(surface: &RetainedSurfacePlan) -> bool {
    matches!(
        surface.kind(),
        SurfaceKind::NestedIsolation(super::frame_plan::NestedIsolationSurfacePlan {
            property_scene: Some(_),
            ..
        })
    )
}

fn property_effect_composite_basis_stamp(
    basis: super::frame_plan::PropertyIsolationCompositeBasis,
) -> super::compiler::PropertyEffectCompositeBasisStamp {
    match basis {
        super::frame_plan::PropertyIsolationCompositeBasis::FrameRoot => {
            super::compiler::PropertyEffectCompositeBasisStamp::FrameRoot
        }
        super::frame_plan::PropertyIsolationCompositeBasis::ParentEffect(effect) => {
            super::compiler::PropertyEffectCompositeBasisStamp::ParentEffect(effect)
        }
        super::frame_plan::PropertyIsolationCompositeBasis::ParentTransform {
            transform,
            viewport_matrix_bits,
        } => super::compiler::PropertyEffectCompositeBasisStamp::ParentTransform {
            transform,
            viewport_matrix_bits,
        },
    }
}

#[allow(clippy::too_many_arguments)]
fn prepare_surface<'a>(
    surface: &'a RetainedSurfacePlan,
    graph: &FrameGraph,
    ctx: &UiBuildContext,
    depth: usize,
    expected_parent: Option<crate::view::node_arena::NodeKey>,
    policy: RetainedSurfaceTreePolicy,
    seen_roots: &mut FxHashSet<crate::view::node_arena::NodeKey>,
    seen_stable_ids: &mut FxHashSet<u64>,
    seen_persistent_keys: &mut FxHashSet<PersistentTextureKey>,
) -> Result<PreparedSurface<'a>, RetainedSurfacePrepareError> {
    if (policy != RetainedSurfaceTreePolicy::PropertyScene && depth > 1)
        || surface.parent_surface() != expected_parent
        || !policy.accepts_surface(surface.kind(), depth)
    {
        return Err(RetainedSurfacePrepareError::NestedSurface);
    }
    if surface.stable_id() == 0 {
        return Err(RetainedSurfacePrepareError::BoundaryIdentity);
    }
    let viewport = ctx.viewport();
    let (source_bounds, descriptor_transform) = match surface.kind() {
        SurfaceKind::Transform(plan) => {
            if plan.transform.0 != surface.boundary_root()
                || surface.persistent_color_key()
                    != transformed_layer_stable_key(surface.stable_id())
            {
                return Err(RetainedSurfacePrepareError::BoundaryIdentity);
            }
            if !surface.matches_frozen_witness()
                || !plan.geometry.matches_rebuilt_contract()
                || plan.geometry.outer_scissor_rect != plan.context.outer_scissor_rect()
            {
                return Err(RetainedSurfacePrepareError::GeometryContract);
            }
            if policy != RetainedSurfaceTreePolicy::PropertyScene
                && depth == 0
                && !plan.context.matches_ui_context(ctx)
            {
                return Err(RetainedSurfacePrepareError::ContextMismatch);
            }
            (plan.geometry.source_bounds, ctx.current_render_transform())
        }
        SurfaceKind::Isolation(plan) => {
            let expected_width = viewport.target_width() as f32 / viewport.scale_factor();
            let expected_height = viewport.target_height() as f32 / viewport.scale_factor();
            if plan.effect.id.0 != surface.boundary_root()
                || plan.effect.owner != surface.boundary_root()
                || surface.persistent_color_key()
                    != crate::view::base_component::isolation_layer_stable_key(surface.stable_id())
            {
                return Err(RetainedSurfacePrepareError::BoundaryIdentity);
            }
            if depth != 0
                || plan.effect.parent.is_some()
                || plan.effect.generation == 0
                || !plan.effect.opacity.is_finite()
                || !(0.0..=1.0).contains(&plan.effect.opacity)
                || !surface.matches_frozen_witness()
                || plan.geometry.outer_scissor_rect.is_some()
                || plan.geometry.source_bounds.x.to_bits() != 0.0_f32.to_bits()
                || plan.geometry.source_bounds.y.to_bits() != 0.0_f32.to_bits()
                || plan.geometry.logical_size[0].to_bits() != expected_width.to_bits()
                || plan.geometry.logical_size[1].to_bits() != expected_height.to_bits()
                || ctx.graphics_pass_context().scissor_rect.is_some()
            {
                return Err(RetainedSurfacePrepareError::GeometryContract);
            }
            (plan.geometry.source_bounds, None)
        }
        SurfaceKind::NestedIsolation(plan) => {
            let bounds = plan.geometry.source_bounds;
            if plan.effect.id.0 != surface.boundary_root()
                || plan.effect.owner != surface.boundary_root()
                || surface.persistent_color_key()
                    != crate::view::base_component::isolation_layer_stable_key(surface.stable_id())
            {
                return Err(RetainedSurfacePrepareError::BoundaryIdentity);
            }
            let property_contract = plan.property_scene.as_ref();
            let property_artifact_contract = plan.property_scene_artifact.as_ref();
            let property_scene_surface = policy == RetainedSurfaceTreePolicy::PropertyScene;
            if property_scene_surface != property_contract.is_some()
                || property_scene_surface != property_artifact_contract.is_some()
                || (!property_scene_surface && depth != 1)
                || plan.effect.parent.is_some()
                || plan.effect.generation == 0
                || !plan.effect.opacity.is_finite()
                || !(0.0..=1.0).contains(&plan.effect.opacity)
                || !surface.matches_frozen_witness()
                || [bounds.x, bounds.y, bounds.width, bounds.height]
                    .iter()
                    .any(|value| !value.is_finite())
                || bounds.x < 0.0
                || bounds.y < 0.0
                || bounds.width <= 0.0
                || bounds.height <= 0.0
                || bounds.corner_radii.map(f32::to_bits) != [0.0_f32.to_bits(); 4]
                || (!property_scene_surface && ctx.graphics_pass_context().scissor_rect.is_some())
            {
                return Err(RetainedSurfacePrepareError::GeometryContract);
            }
            if let Some(contract) = property_contract {
                let source_bounds_bits =
                    [bounds.x, bounds.y, bounds.width, bounds.height].map(f32::to_bits);
                if contract.effect_chain.isolated_leaf != plan.effect
                    || contract.effect_chain.isolated_leaf.id != plan.effect.id
                    || contract.effect_chain.isolated_leaf.owner != surface.boundary_root()
                    || contract.effect_chain.isolated_leaf.parent.is_some()
                    || contract.raster_space.source_bounds_bits != source_bounds_bits
                    || contract.composite.rect_bits != source_bounds_bits
                    || contract.composite.opacity_bits != plan.effect.opacity.to_bits()
                    || contract.composite.effect_generation != plan.effect.generation
                    || contract.raster_identity.boundary != plan.effect.id
                    || contract.raster_identity.stable_id != surface.stable_id()
                    || contract.raster_identity.raster_space != contract.raster_space
                    || contract.raster_identity.local_raster_clips != contract.local_raster_clips
                    || contract.raster_identity.nested_dependencies != contract.nested_dependencies
                    || contract.parent_opaque_cursor_delta != 0
                {
                    return Err(RetainedSurfacePrepareError::GeometryContract);
                }
                let artifact_contract =
                    property_artifact_contract.expect("property-scene presence pair checked above");
                let content_matches = artifact_contract
                    .content()
                    .iter()
                    .zip(&contract.raster_identity.content)
                    .all(|(artifact, planning)| {
                        artifact.owner == planning.owner
                            && artifact.stable_id == planning.stable_id
                            && artifact.parent == planning.parent
                            && artifact.self_paint_revision == planning.self_paint_revision
                            && artifact.topology_revision == planning.topology_revision
                    });
                if artifact_contract.boundary_root() != surface.boundary_root()
                    || artifact_contract.stable_id() != surface.stable_id()
                    || artifact_contract.isolated_leaf() != contract.effect_chain.isolated_leaf
                    || artifact_contract.live_effect_chain()
                        != contract.effect_chain.live_leaf_to_root
                    || artifact_contract.detached_ancestors()
                        != contract.effect_chain.detached_ancestors
                    || artifact_contract.local_raster_clips() != contract.local_raster_clips
                    || artifact_contract.detached_ancestor_clips()
                        != contract.ancestor_composite_clips
                    || artifact_contract.content().len() != contract.raster_identity.content.len()
                    || !content_matches
                {
                    return Err(RetainedSurfacePrepareError::GeometryContract);
                }
            }
            (bounds, None)
        }
        SurfaceKind::ScrollHost(plan) => {
            let bounds = plan.admission.source_bounds;
            if depth != 0
                || plan.admission.boundary_root != surface.boundary_root()
                || plan.admission.stable_id != surface.stable_id()
                || plan.admission.child_stable_id == 0
                || surface.persistent_color_key()
                    != crate::view::base_component::scroll_host_layer_stable_key(
                        surface.stable_id(),
                    )
                || !surface.matches_frozen_witness()
                || !plan.admission.matches_scroll_node(plan.scroll)
                || !matches!(
                    plan.admission.scroll.scrollbar_overlay.paint_state,
                    crate::view::base_component::ScrollbarPaintStateWitness::HiddenNow
                        | crate::view::base_component::ScrollbarPaintStateWitness::NotPaintable
                        | crate::view::base_component::ScrollbarPaintStateWitness::OpaqueNow
                        | crate::view::base_component::ScrollbarPaintStateWitness::TranslucentNow
                )
                || !matches!(
                    plan.scroll.scrollbar_overlay.paint_state,
                    crate::view::base_component::ScrollbarPaintStateWitness::HiddenNow
                        | crate::view::base_component::ScrollbarPaintStateWitness::NotPaintable
                        | crate::view::base_component::ScrollbarPaintStateWitness::OpaqueNow
                        | crate::view::base_component::ScrollbarPaintStateWitness::TranslucentNow
                )
                || !plan
                    .scroll
                    .has_canonical_vertical_geometry_with_contents_clip(plan.contents_clip)
                || plan.scroll.id.0 != surface.boundary_root()
                || plan.scroll.owner != surface.boundary_root()
                || plan.scroll.parent.is_some()
                || plan.scroll.generation == 0
                || plan.contents_clip.id.owner != surface.boundary_root()
                || plan.contents_clip.id.role
                    != crate::view::compositor::property_tree::ClipNodeRole::ContentsClip
                || plan.contents_clip.owner != surface.boundary_root()
                || plan.contents_clip.parent.is_some()
                || plan.contents_clip.behavior
                    != crate::view::compositor::property_tree::ClipBehavior::Intersect
                || plan.contents_clip.generation == 0
                || plan.scroll.contents_clip
                    != crate::view::base_component::ScrollContentsClipWitness::ExactRect(
                        plan.contents_clip.logical_scissor,
                    )
                || viewport.scale_factor().to_bits() != 1.0_f32.to_bits()
                || ctx.paint_offset().map(f32::to_bits) != [0.0_f32.to_bits(); 2]
                || ctx.graphics_pass_context().scissor_rect.is_some()
            {
                return Err(RetainedSurfacePrepareError::GeometryContract);
            }
            (bounds, None)
        }
    };
    if source_bounds.x < 0.0 || source_bounds.y < 0.0 {
        return Err(RetainedSurfacePrepareError::GeometryContract);
    }
    let color_key = surface.persistent_color_key();
    let depth_key = color_key
        .depth_stencil()
        .ok_or(RetainedSurfacePrepareError::DescriptorPair)?;
    if !seen_roots.insert(surface.boundary_root())
        || !seen_stable_ids.insert(surface.stable_id())
        || !seen_persistent_keys.insert(color_key)
        || !seen_persistent_keys.insert(depth_key)
    {
        return Err(RetainedSurfacePrepareError::DuplicateSurfaceIdentity);
    }
    for key in [color_key, depth_key] {
        if graph
            .declared_persistent_texture_keys()
            .any(|declared| declared == key)
        {
            return Err(RetainedSurfacePrepareError::PersistentKeyAlreadyDeclared(
                key,
            ));
        }
    }

    let color_desc = texture_desc_for_logical_bounds(
        source_bounds,
        viewport.scale_factor(),
        descriptor_transform,
        viewport.target_format(),
    );
    let (color_desc, depth_desc) = persistent_target_texture_descriptors(color_desc, color_key);
    let color_usage = wgpu::TextureUsages::RENDER_ATTACHMENT
        | wgpu::TextureUsages::TEXTURE_BINDING
        | wgpu::TextureUsages::COPY_SRC
        | wgpu::TextureUsages::COPY_DST;
    if color_desc.format() != viewport.target_format()
        || color_desc.dimension() != wgpu::TextureDimension::D2
        || color_desc.sample_count() != 1
        || color_desc.usage() != color_usage
        || depth_desc.width() != color_desc.width()
        || depth_desc.height() != color_desc.height()
        || depth_desc.format() != wgpu::TextureFormat::Depth24PlusStencil8
        || depth_desc.dimension() != wgpu::TextureDimension::D2
        || depth_desc.sample_count() != color_desc.sample_count()
        || depth_desc.usage() != wgpu::TextureUsages::RENDER_ATTACHMENT
    {
        return Err(RetainedSurfacePrepareError::DescriptorPair);
    }
    let target = RetainedSurfaceRasterInputs {
        color: color_desc.clone(),
        depth: depth_desc,
        scale_factor_bits: viewport.scale_factor().to_bits(),
        source_bounds_bits: [
            source_bounds.x.to_bits(),
            source_bounds.y.to_bits(),
            source_bounds.width.to_bits(),
            source_bounds.height.to_bits(),
        ],
    };

    let mut cursor = 0_u32;
    let mut prepared_steps = Vec::with_capacity(surface.raster_steps().len());
    let mut stamp_steps = Vec::with_capacity(surface.raster_steps().len());
    for (step_index, step) in surface.raster_steps().iter().enumerate() {
        match step {
            PaintPlanStep::ArtifactSpan(span) => {
                let target_matches = match surface.kind() {
                    SurfaceKind::Transform(_) => {
                        span.artifact().target == PaintArtifactTarget::CurrentTarget
                    }
                    SurfaceKind::Isolation(plan) => matches!(
                        span.artifact().target,
                        PaintArtifactTarget::RootOpacityGroup { root, effect }
                            if root == surface.boundary_root() && effect == plan.effect.id
                    ),
                    SurfaceKind::NestedIsolation(plan) => {
                        if plan.property_scene.is_some() {
                            span.artifact().target == PaintArtifactTarget::CurrentTarget
                        } else {
                            matches!(
                                span.artifact().target,
                                PaintArtifactTarget::RootOpacityGroup { root, effect }
                                    if root == surface.boundary_root() && effect == plan.effect.id
                            )
                        }
                    }
                    SurfaceKind::ScrollHost(_) => {
                        span.artifact().target == PaintArtifactTarget::CurrentTarget
                    }
                };
                if !target_matches || span.opaque_order_span().start != cursor {
                    return Err(if !target_matches {
                        RetainedSurfacePrepareError::ArtifactTarget
                    } else {
                        RetainedSurfacePrepareError::OpaqueSpan
                    });
                }
                let expected_end = cursor
                    .checked_add(opaque_order_count(span.artifact()))
                    .ok_or(RetainedSurfacePrepareError::OpaqueSpan)?;
                if span.opaque_order_span().end != expected_end {
                    return Err(RetainedSurfacePrepareError::OpaqueSpan);
                }
                let (artifact, span_stamp) = match surface.kind() {
                    SurfaceKind::Transform(plan)
                        if policy == RetainedSurfaceTreePolicy::PropertyScene =>
                    {
                        let artifact =
                            super::compiler::validate_transform_property_surface_artifact(
                                span.artifact(),
                                surface.boundary_root(),
                                plan.transform,
                            )
                            .ok_or(RetainedSurfacePrepareError::ArtifactStore)?;
                        let span_stamp = super::compiler::validated_transform_property_surface_artifact_span_stamp(
                            &artifact,
                            step_index,
                            span.opaque_order_span().clone(),
                        )
                        .ok_or(RetainedSurfacePrepareError::ArtifactStore)?;
                        (
                            PreparedValidatedArtifact::TransformProperty(artifact),
                            span_stamp,
                        )
                    }
                    SurfaceKind::Transform(plan) => (
                        PreparedValidatedArtifact::Transform(
                            validate_transform_surface_artifact(
                                span.artifact(),
                                surface.boundary_root(),
                                plan.transform,
                            )
                            .ok_or(RetainedSurfacePrepareError::ArtifactStore)?,
                        ),
                        validated_retained_surface_artifact_span_stamp(
                            span.artifact(),
                            surface.boundary_root(),
                            plan.transform,
                            step_index,
                            span.opaque_order_span().clone(),
                        )
                        .ok_or(RetainedSurfacePrepareError::ArtifactStore)?,
                    ),
                    SurfaceKind::Isolation(plan) => (
                        PreparedValidatedArtifact::Isolation(
                            validate_isolation_surface_artifact(
                                span.artifact(),
                                surface.boundary_root(),
                                plan.effect.id,
                            )
                            .ok_or(RetainedSurfacePrepareError::ArtifactStore)?,
                        ),
                        validated_isolation_surface_artifact_span_stamp(
                            span.artifact(),
                            surface.boundary_root(),
                            plan.effect.id,
                            step_index,
                            span.opaque_order_span().clone(),
                        )
                        .ok_or(RetainedSurfacePrepareError::ArtifactStore)?,
                    ),
                    SurfaceKind::NestedIsolation(plan) if plan.property_scene.is_some() => {
                        let artifact = super::compiler::validate_effect_property_surface_artifact(
                            span.artifact(),
                            plan.property_scene_artifact
                                .as_ref()
                                .ok_or(RetainedSurfacePrepareError::ArtifactStore)?,
                        )
                        .ok_or(RetainedSurfacePrepareError::ArtifactStore)?;
                        let span_stamp =
                            super::compiler::validated_effect_property_surface_artifact_span_stamp(
                                &artifact,
                                step_index,
                                span.opaque_order_span().clone(),
                            )
                            .ok_or(RetainedSurfacePrepareError::ArtifactStore)?;
                        (
                            PreparedValidatedArtifact::EffectProperty(artifact),
                            span_stamp,
                        )
                    }
                    SurfaceKind::NestedIsolation(plan) => (
                        PreparedValidatedArtifact::Isolation(
                            validate_isolation_surface_artifact(
                                span.artifact(),
                                surface.boundary_root(),
                                plan.effect.id,
                            )
                            .ok_or(RetainedSurfacePrepareError::ArtifactStore)?,
                        ),
                        validated_isolation_surface_artifact_span_stamp(
                            span.artifact(),
                            surface.boundary_root(),
                            plan.effect.id,
                            step_index,
                            span.opaque_order_span().clone(),
                        )
                        .ok_or(RetainedSurfacePrepareError::ArtifactStore)?,
                    ),
                    SurfaceKind::ScrollHost(plan) => (
                        PreparedValidatedArtifact::ScrollHost(
                            validate_baked_scroll_host_artifact(
                                span.artifact(),
                                surface.boundary_root(),
                                plan.admission.child,
                                plan.scroll,
                                plan.contents_clip,
                            )
                            .ok_or(RetainedSurfacePrepareError::ArtifactStore)?,
                        ),
                        validated_scroll_host_artifact_span_stamp(
                            span.artifact(),
                            surface.boundary_root(),
                            plan.admission.child,
                            plan.scroll,
                            plan.contents_clip,
                            step_index,
                            span.opaque_order_span().clone(),
                        )
                        .ok_or(RetainedSurfacePrepareError::ArtifactStore)?,
                    ),
                };
                cursor = expected_end;
                prepared_steps.push(PreparedSurfaceStep::ArtifactSpan(PreparedArtifactSpan {
                    plan: span,
                    artifact,
                }));
                stamp_steps.push(RetainedSurfaceRasterStepStamp::ArtifactSpan(span_stamp));
            }
            PaintPlanStep::RetainedSurface(child) => {
                if (policy != RetainedSurfaceTreePolicy::PropertyScene && depth >= 1)
                    || !policy.accepts_surface(child.kind(), depth + 1)
                {
                    return Err(RetainedSurfacePrepareError::NestedSurface);
                }
                let child = prepare_surface(
                    child,
                    graph,
                    ctx,
                    depth + 1,
                    Some(surface.boundary_root()),
                    policy,
                    seen_roots,
                    seen_stable_ids,
                    seen_persistent_keys,
                )?;
                let parent_before = cursor;
                let parent_after = if surface_is_property_effect(child.surface) {
                    cursor
                } else {
                    cursor.max(child.stamp.opaque_order_span.end)
                };
                let child_composite_geometry = match child.surface.kind() {
                    SurfaceKind::Transform(plan) => {
                        retained_surface_composite_geometry_stamp(plan.geometry)
                    }
                    SurfaceKind::Isolation(plan) => retained_isolation_composite_geometry_stamp(
                        plan.geometry.source_bounds,
                        plan.geometry.logical_size,
                        plan.effect.opacity,
                        plan.geometry.outer_scissor_rect,
                    ),
                    SurfaceKind::NestedIsolation(plan) => {
                        if let Some(contract) = &plan.property_scene {
                            super::compiler::retained_property_effect_composite_geometry_stamp(
                                plan.geometry.source_bounds,
                                plan.effect.opacity,
                                plan.effect.generation,
                                property_effect_composite_basis_stamp(contract.composite.basis),
                                contract.composite.resolved_scissor,
                                contract.ancestor_composite_clips.clone(),
                            )
                        } else {
                            retained_nested_isolation_composite_geometry_stamp(
                                plan.geometry.source_bounds,
                                plan.effect.opacity,
                            )
                        }
                    }
                    SurfaceKind::ScrollHost(_) => {
                        return Err(RetainedSurfacePrepareError::NestedSurface);
                    }
                }
                .ok_or(RetainedSurfacePrepareError::GeometryContract)?;
                stamp_steps.push(RetainedSurfaceRasterStepStamp::NestedSurface(
                    NestedSurfaceRasterDependency {
                        step_index,
                        child_stamp: Box::new(child.stamp.clone()),
                        child_composite_geometry,
                        parent_opaque_order_before: parent_before,
                        parent_opaque_order_after: parent_after,
                    },
                ));
                cursor = parent_after;
                prepared_steps.push(PreparedSurfaceStep::RetainedSurface(
                    PreparedNestedSurface {
                        child: Box::new(child),
                        parent_opaque_order_before: parent_before,
                        parent_opaque_order_after: parent_after,
                    },
                ));
            }
        }
    }
    if surface.aggregate_opaque_order_span() != &(0..cursor) {
        return Err(RetainedSurfacePrepareError::OpaqueSpan);
    }
    let stamp = match surface.kind() {
        SurfaceKind::ScrollHost(plan) => validated_scroll_host_raster_stamp(
            surface.boundary_root(),
            surface.stable_id(),
            surface.persistent_color_key(),
            target,
            stamp_steps,
            surface.aggregate_opaque_order_span().clone(),
            RetainedScrollHostRasterDependency {
                scroll: plan.scroll,
                contents_clip: plan.contents_clip,
            },
        ),
        SurfaceKind::Transform(_) if policy == RetainedSurfaceTreePolicy::PropertyScene => {
            if surface.raster_steps().iter().any(|step| {
                matches!(
                    step,
                    PaintPlanStep::RetainedSurface(child)
                        if surface_is_property_effect(child)
                )
            }) {
                validated_mixed_property_transform_raster_stamp(
                    surface.boundary_root(),
                    surface.stable_id(),
                    surface.persistent_color_key(),
                    depth,
                    target,
                    stamp_steps,
                    surface.aggregate_opaque_order_span().clone(),
                )
            } else {
                super::compiler::validated_property_scene_surface_raster_stamp(
                    surface.boundary_root(),
                    surface.stable_id(),
                    surface.persistent_color_key(),
                    depth,
                    target,
                    stamp_steps,
                    surface.aggregate_opaque_order_span().clone(),
                )
            }
        }
        SurfaceKind::NestedIsolation(plan)
            if policy == RetainedSurfaceTreePolicy::PropertyScene =>
        {
            super::compiler::validated_property_effect_surface_raster_stamp(
                plan.property_scene_artifact
                    .as_ref()
                    .ok_or(RetainedSurfacePrepareError::ArtifactStore)?,
                depth,
                target,
                stamp_steps,
                surface.aggregate_opaque_order_span().clone(),
            )
        }
        kind => validated_retained_surface_tree_raster_stamp(
            surface.boundary_root(),
            surface.stable_id(),
            surface.persistent_color_key(),
            match kind {
                SurfaceKind::Transform(_) => RetainedSurfaceRasterRole::Transform,
                SurfaceKind::Isolation(_) => RetainedSurfaceRasterRole::RootIsolation,
                SurfaceKind::NestedIsolation(_) => RetainedSurfaceRasterRole::NestedIsolation,
                SurfaceKind::ScrollHost(_) => unreachable!(),
            },
            depth,
            target,
            stamp_steps,
            surface.aggregate_opaque_order_span().clone(),
        ),
    }
    .ok_or(RetainedSurfacePrepareError::ArtifactStore)?;
    Ok(PreparedSurface {
        surface,
        raster_steps: prepared_steps,
        color_key,
        color_desc,
        stamp,
        action: None,
    })
}

fn emit_prepared_retained_surface(
    prepared: PreparedFramePaintPlan<'_>,
    graph: &mut FrameGraph,
    mut ctx: UiBuildContext,
) -> (BuildState, Vec<RetainedSurfaceRasterStamp>) {
    let parent_target = prepared.parent_target.unwrap_or_else(|| {
        let target = ctx.allocate_target(graph);
        ctx.set_current_target(target);
        target
    });
    ctx.set_current_target(parent_target);
    let mut stamps = Vec::new();
    let state = emit_prepared_surface(prepared.root, graph, ctx, &mut stamps, true);
    (state, stamps)
}

fn emit_prepared_property_scene_graph(
    prepared: PreparedPropertyScene<'_>,
    graph: &mut FrameGraph,
    mut ctx: UiBuildContext,
) -> (BuildState, Vec<RetainedSurfaceRasterStamp>) {
    let parent_target = prepared
        .parent_target
        .or_else(|| ctx.current_target())
        .unwrap_or_else(|| {
            let target = ctx.allocate_target(graph);
            ctx.set_current_target(target);
            target
        });
    ctx.set_current_target(parent_target);
    let expected_terminal = prepared.transaction_witness.aggregate_opaque_order_span.end;
    let mut stamps = Vec::new();
    for step in prepared.steps {
        match step {
            PreparedPropertySceneStep::ArtifactSpan(span) => {
                assert_eq!(
                    ctx.opaque_rect_order(),
                    span.plan.opaque_order_span().start,
                    "prepared property-scene cursor must match before artifact emit"
                );
                super::compiler::emit_validated_property_scene_artifact(
                    span.artifact,
                    graph,
                    &mut ctx,
                );
                assert_eq!(
                    ctx.opaque_rect_order(),
                    span.plan.opaque_order_span().end,
                    "validated property-scene artifact must reach its sealed end"
                );
            }
            PreparedPropertySceneStep::RetainedSurface(surface) => {
                let before = ctx.opaque_rect_order();
                let child_terminal = surface.stamp.opaque_order_span.end;
                let preserves_parent_cursor = surface_is_property_effect(surface.surface);
                let viewport = ctx.viewport();
                let state = emit_prepared_surface(surface, graph, ctx, &mut stamps, true);
                ctx = UiBuildContext::from_parts(viewport, state);
                assert_eq!(
                    ctx.opaque_rect_order(),
                    if preserves_parent_cursor {
                        before
                    } else {
                        before.max(child_terminal)
                    },
                    "property-scene child must apply its prepared opaque cursor policy"
                );
            }
        }
    }
    assert_eq!(
        ctx.opaque_rect_order(),
        expected_terminal,
        "property-scene emitted terminal must equal its sealed aggregate"
    );
    (ctx.into_state(), stamps)
}

fn emit_prepared_surface(
    prepared: PreparedSurface<'_>,
    graph: &mut FrameGraph,
    mut parent_ctx: UiBuildContext,
    stamps: &mut Vec<RetainedSurfaceRasterStamp>,
    composite_to_parent: bool,
) -> BuildState {
    let PreparedSurface {
        surface,
        raster_steps,
        color_key,
        color_desc,
        stamp,
        action,
    } = prepared;
    let action = action.expect("prepared surface actions are frozen before graph mutation");
    let parent_target = parent_ctx
        .current_target()
        .expect("prepared surface always has a parent target before mutation");
    let mut layer_ctx = UiBuildContext::from_parts(
        parent_ctx.viewport(),
        parent_ctx.layer_subtree_state_with_ancestor_clip(AncestorClipContext::default()),
    );
    layer_ctx.set_current_render_transform(match surface.kind() {
        SurfaceKind::Transform(_) => parent_ctx.current_render_transform(),
        SurfaceKind::Isolation(_) => None,
        SurfaceKind::NestedIsolation(_) => parent_ctx.current_render_transform(),
        SurfaceKind::ScrollHost(_) => None,
    });
    let layer_target = layer_ctx.allocate_persistent_target_with_desc(graph, color_desc, color_key);
    layer_ctx.set_current_target(layer_target);
    if action == RetainedSurfaceCompileAction::Reraster {
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
        for step in raster_steps {
            match step {
                PreparedSurfaceStep::ArtifactSpan(span) => {
                    assert_eq!(
                        layer_ctx.opaque_rect_order(),
                        span.plan.opaque_order_span().start,
                        "prepared local opaque cursor must match before artifact emit"
                    );
                    match span.artifact {
                        PreparedValidatedArtifact::Transform(artifact) => {
                            emit_validated_transform_surface_artifact(
                                artifact,
                                graph,
                                &mut layer_ctx,
                            );
                        }
                        PreparedValidatedArtifact::TransformProperty(artifact) => {
                            super::compiler::emit_validated_transform_property_surface_artifact(
                                artifact,
                                graph,
                                &mut layer_ctx,
                            );
                        }
                        PreparedValidatedArtifact::Isolation(artifact) => {
                            emit_validated_isolation_surface_artifact(
                                artifact,
                                graph,
                                &mut layer_ctx,
                            );
                        }
                        PreparedValidatedArtifact::EffectProperty(artifact) => {
                            super::compiler::emit_validated_effect_property_surface_artifact(
                                artifact,
                                graph,
                                &mut layer_ctx,
                            );
                        }
                        PreparedValidatedArtifact::ScrollHost(artifact) => {
                            emit_validated_baked_scroll_host_artifact(
                                artifact,
                                graph,
                                &mut layer_ctx,
                            );
                        }
                    }
                    assert_eq!(
                        layer_ctx.opaque_rect_order(),
                        span.plan.opaque_order_span().end,
                        "validated artifact opaque count must reach its prepared local end"
                    );
                }
                PreparedSurfaceStep::RetainedSurface(nested) => {
                    assert_eq!(
                        layer_ctx.opaque_rect_order(),
                        nested.parent_opaque_order_before,
                        "nested surface must start at its prepared parent cursor"
                    );
                    let viewport = layer_ctx.viewport();
                    let child_state =
                        emit_prepared_surface(*nested.child, graph, layer_ctx, stamps, true);
                    layer_ctx = UiBuildContext::from_parts(viewport, child_state);
                    assert_eq!(
                        layer_ctx.opaque_rect_order(),
                        nested.parent_opaque_order_after,
                        "nested surface must reach its prepared parent cursor"
                    );
                }
            }
        }
        assert_eq!(
            layer_ctx.opaque_rect_order(),
            surface.aggregate_opaque_order_span().end,
            "prepared surface terminal must equal emitted local terminal"
        );
    } else {
        for step in raster_steps {
            let PreparedSurfaceStep::RetainedSurface(nested) = step else {
                continue;
            };
            let viewport = layer_ctx.viewport();
            let mut detached_parent_ctx = UiBuildContext::from_parts(
                viewport,
                layer_ctx.layer_subtree_state_with_ancestor_clip(AncestorClipContext::default()),
            );
            detached_parent_ctx.set_current_render_transform(layer_ctx.current_render_transform());
            detached_parent_ctx.set_current_target(layer_target);
            let child_terminal = nested.child.stamp.opaque_order_span.end;
            let expected_replayed_terminal = if surface_is_property_effect(nested.child.surface) {
                0
            } else {
                child_terminal
            };
            let child_state =
                emit_prepared_surface(*nested.child, graph, detached_parent_ctx, stamps, false);
            assert_eq!(
                child_state.opaque_rect_order(),
                expected_replayed_terminal,
                "detached reused-subtree child must apply its prepared opaque cursor policy"
            );
            layer_ctx.merge_child_target_pairs(&child_state);
        }
        layer_ctx.replay_opaque_rect_order_exact(0, surface.aggregate_opaque_order_span().end);
    }
    let layer_state = layer_ctx.into_state();
    let parent_before = parent_ctx.opaque_rect_order();
    let child_terminal = surface.aggregate_opaque_order_span().end;
    let property_effect = surface_is_property_effect(surface);
    if property_effect {
        parent_ctx.merge_child_target_pairs(&layer_state);
        assert_eq!(
            parent_ctx.opaque_rect_order(),
            parent_before,
            "property-effect composite must not advance its parent opaque cursor"
        );
    } else {
        let parent_after = parent_before.max(child_terminal);
        parent_ctx.merge_child_render_state_exact(
            &layer_state,
            parent_before,
            child_terminal,
            parent_after,
        );
    }
    parent_ctx.set_current_target(parent_target);
    if composite_to_parent {
        match surface.kind() {
            SurfaceKind::Transform(plan) => {
                graph.add_graphics_pass(TextureCompositePass::new(
                    plan.geometry.texture_composite_params(),
                    TextureCompositeInput::from_render_target(
                        TextureCompositeSourceIn::with_handle(
                            layer_target
                                .handle()
                                .expect("prepared persistent surface target must have a handle"),
                        ),
                        Default::default(),
                        parent_ctx.graphics_pass_context(),
                    ),
                    TextureCompositeOutput {
                        render_target: parent_target,
                    },
                ));
            }
            SurfaceKind::Isolation(plan) => {
                graph.add_graphics_pass(CompositeLayerPass::new(
                    CompositeLayerParams {
                        rect_pos: [0.0, 0.0],
                        rect_size: plan.geometry.logical_size,
                        corner_radii: [0.0; 4],
                        opacity: plan.effect.opacity,
                        scissor_rect: plan.geometry.outer_scissor_rect,
                        clear_target: false,
                    },
                    CompositeLayerInput {
                        layer: LayerIn::with_handle(
                            layer_target
                                .handle()
                                .expect("prepared isolation target must have a handle"),
                        ),
                        pass_context: parent_ctx.graphics_pass_context(),
                    },
                    CompositeLayerOutput {
                        render_target: parent_target,
                    },
                ));
            }
            SurfaceKind::NestedIsolation(plan) => {
                let bounds = plan.geometry.source_bounds;
                let scissor_rect = plan
                    .property_scene
                    .as_ref()
                    .and_then(|contract| contract.composite.resolved_scissor);
                graph.add_graphics_pass(CompositeLayerPass::new(
                    CompositeLayerParams {
                        rect_pos: [bounds.x, bounds.y],
                        rect_size: [bounds.width, bounds.height],
                        corner_radii: [0.0; 4],
                        opacity: plan.effect.opacity,
                        scissor_rect,
                        clear_target: false,
                    },
                    CompositeLayerInput {
                        layer: LayerIn::with_handle(
                            layer_target
                                .handle()
                                .expect("prepared nested isolation target must have a handle"),
                        ),
                        pass_context: parent_ctx.graphics_pass_context(),
                    },
                    CompositeLayerOutput {
                        render_target: parent_target,
                    },
                ));
            }
            SurfaceKind::ScrollHost(plan) => {
                let bounds = plan.admission.source_bounds;
                graph.add_graphics_pass(CompositeLayerPass::new(
                    CompositeLayerParams {
                        rect_pos: [bounds.x, bounds.y],
                        rect_size: [bounds.width, bounds.height],
                        corner_radii: [0.0; 4],
                        opacity: 1.0,
                        scissor_rect: None,
                        clear_target: false,
                    },
                    CompositeLayerInput {
                        layer: LayerIn::with_handle(
                            layer_target
                                .handle()
                                .expect("prepared scroll-host target must have a handle"),
                        ),
                        pass_context: parent_ctx.graphics_pass_context(),
                    },
                    CompositeLayerOutput {
                        render_target: parent_target,
                    },
                ));
            }
        }
    } else if action == RetainedSurfaceCompileAction::Reraster {
        graph
            .add_texture_sink(
                &layer_target,
                crate::view::frame_graph::ExternalSinkKind::PersistentMaterialization,
            )
            .expect("prepared persistent reraster target must support materialization sinks");
    }
    parent_ctx.set_current_target(parent_target);
    stamps.push(stamp);
    parent_ctx.into_state()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RetainedSurfaceBuildTrace {
    pub(crate) action: RetainedSurfaceCompileAction,
    pub(crate) boundary_root: crate::view::node_arena::NodeKey,
    pub(crate) descriptor_size: [u32; 2],
    pub(crate) chunk_count: usize,
    pub(crate) op_count: usize,
}

impl RetainedSurfaceBuildTrace {
    fn from_prepared(prepared: &PreparedSurface<'_>) -> Self {
        let stamp = prepared.stamp();
        Self {
            action: prepared.action(),
            boundary_root: stamp.identity.boundary_root,
            descriptor_size: [stamp.target.color.width(), stamp.target.color.height()],
            chunk_count: stamp.chunks.len(),
            op_count: stamp.op_count,
        }
    }
}

pub(crate) struct RetainedSurfaceBuildOutcome {
    state: BuildState,
    trace: RetainedSurfaceBuildTrace,
}

impl RetainedSurfaceBuildOutcome {
    pub(crate) fn into_parts(self) -> (BuildState, RetainedSurfaceBuildTrace) {
        (self.state, self.trace)
    }
}

pub(crate) struct RetainedSurfaceTreeBuildOutcome {
    state: BuildState,
    traces: Vec<RetainedSurfaceBuildTrace>,
}

#[derive(Clone, Debug)]
pub(crate) struct RetainedPropertySceneBuildTrace {
    pub(crate) root_count: usize,
    pub(crate) surface_count: usize,
    pub(crate) reraster_count: usize,
    pub(crate) reuse_count: usize,
    pub(crate) surfaces: Vec<RetainedSurfaceBuildTrace>,
}

pub(crate) struct RetainedPropertySceneBuildOutcome {
    state: BuildState,
    trace: RetainedPropertySceneBuildTrace,
}

/// Executor-owned property-scene transaction capability. The transform-only
/// variant preserves the existing compiler seal. Effect scenes use a sibling
/// executor seal so their arbitrary-depth role contract does not weaken the
/// generic retained-surface or transform-only property-scene canonicalizers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RetainedPropertySceneTransaction {
    Transform(super::compiler::RetainedPropertySceneTransactionStamp),
    Effect(RetainedPropertyEffectSceneTransactionStamp),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RetainedPropertyEffectSceneTransactionStamp {
    witness: super::frame_plan::PropertySceneTransactionWitness,
    ordered_stamps: Vec<RetainedSurfaceRasterStamp>,
    plan_step_count: usize,
    aggregate_opaque_order_span: std::ops::Range<u32>,
}

impl From<super::compiler::RetainedPropertySceneTransactionStamp>
    for RetainedPropertySceneTransaction
{
    fn from(transaction: super::compiler::RetainedPropertySceneTransactionStamp) -> Self {
        Self::Transform(transaction)
    }
}

impl RetainedPropertySceneTransaction {
    pub(crate) fn is_canonical(&self) -> bool {
        match self {
            Self::Transform(transaction) => transaction.is_canonical(),
            Self::Effect(transaction) => transaction.is_canonical(),
        }
    }

    pub(crate) fn validates_surface_stamps(&self, stamps: &[RetainedSurfaceRasterStamp]) -> bool {
        match self {
            Self::Transform(transaction) => transaction.validates_surface_stamps(stamps),
            Self::Effect(transaction) => transaction.validates_surface_stamps(stamps),
        }
    }
}

impl RetainedPropertyEffectSceneTransactionStamp {
    fn new(
        witness: super::frame_plan::PropertySceneTransactionWitness,
        ordered_stamps: &[RetainedSurfaceRasterStamp],
        plan_step_count: usize,
        aggregate_opaque_order_span: std::ops::Range<u32>,
    ) -> Option<Self> {
        property_effect_scene_transaction_is_canonical(
            &witness,
            ordered_stamps,
            plan_step_count,
            &aggregate_opaque_order_span,
        )
        .then(|| Self {
            witness,
            ordered_stamps: ordered_stamps.to_vec(),
            plan_step_count,
            aggregate_opaque_order_span,
        })
    }

    #[cfg(test)]
    pub(crate) fn new_for_test(
        witness: super::frame_plan::PropertySceneTransactionWitness,
        ordered_stamps: &[RetainedSurfaceRasterStamp],
        plan_step_count: usize,
        aggregate_opaque_order_span: std::ops::Range<u32>,
    ) -> Option<Self> {
        Self::new(
            witness,
            ordered_stamps,
            plan_step_count,
            aggregate_opaque_order_span,
        )
    }

    fn is_canonical(&self) -> bool {
        property_effect_scene_transaction_is_canonical(
            &self.witness,
            &self.ordered_stamps,
            self.plan_step_count,
            &self.aggregate_opaque_order_span,
        )
    }

    fn validates_surface_stamps(&self, stamps: &[RetainedSurfaceRasterStamp]) -> bool {
        self.is_canonical() && self.ordered_stamps == stamps
    }
}

fn property_effect_scene_surface_stamp_is_canonical(
    stamp: &RetainedSurfaceRasterStamp,
    depth: usize,
) -> bool {
    if stamp.identity.role == RetainedSurfaceRasterRole::PropertyEffect {
        return super::compiler::property_effect_surface_raster_stamp_is_canonical_at_depth(
            stamp, depth,
        );
    }
    fn validate(
        stamp: &RetainedSurfaceRasterStamp,
        depth: usize,
        identities: &mut FxHashSet<super::RetainedSurfaceResidentKey>,
        owners: &mut FxHashSet<crate::view::node_arena::NodeKey>,
    ) -> bool {
        if depth >= usize::from(u8::MAX)
            || stamp.identity.scroll_content_tile.is_some()
            || stamp.scroll_host.is_some()
            || stamp.text_area_paint_grammar.is_some()
            || stamp.interactive_text_area_resident.is_some()
            || stamp.atomic_projection_text_area_resident.is_some()
            || stamp.identity.stable_id == 0
            || !stamp
                .target
                .has_canonical_descriptor_pair_for(stamp.identity)
            || stamp.opaque_order_span.start != 0
            || !identities.insert(stamp.identity.resident_key())
            || !owners.insert(stamp.identity.boundary_root)
        {
            return false;
        }
        match (stamp.identity.role, stamp.property_effect.as_ref()) {
            (RetainedSurfaceRasterRole::Transform, None) => {
                if stamp.identity.color_key
                    != transformed_layer_stable_key(stamp.identity.stable_id)
                {
                    return false;
                }
            }
            (RetainedSurfaceRasterRole::PropertyEffect, Some(effect)) => {
                if stamp.identity.color_key
                    != crate::view::base_component::isolation_layer_stable_key(
                        stamp.identity.stable_id,
                    )
                    || effect.content.is_empty()
                    || effect.content[0].owner != stamp.identity.boundary_root
                    || effect.content[0].parent.is_some()
                {
                    return false;
                }
                let mut content_owners = FxHashSet::default();
                let mut stable_ids = FxHashSet::default();
                for (index, content) in effect.content.iter().enumerate() {
                    if content.owner.is_null()
                        || content.stable_id == 0
                        || content.self_paint_revision == 0
                        || content.topology_revision == 0
                        || !content_owners.insert(content.owner)
                        || !stable_ids.insert(content.stable_id)
                        || (index > 0
                            && content
                                .parent
                                .is_none_or(|parent| !content_owners.contains(&parent)))
                    {
                        return false;
                    }
                }
                let mut clips = FxHashSet::default();
                for clip in &effect.local_raster_clips {
                    if clip.id.owner != clip.owner
                        || clip.generation == 0
                        || !content_owners.contains(&clip.owner)
                        || !matches!(
                            (clip.id.role, clip.behavior),
                            (
                                crate::view::compositor::property_tree::ClipNodeRole::SelfClip,
                                crate::view::compositor::property_tree::ClipBehavior::Replace
                            ) | (
                                crate::view::compositor::property_tree::ClipNodeRole::ContentsClip,
                                crate::view::compositor::property_tree::ClipBehavior::Intersect
                            )
                        )
                        || !clips.insert(clip.id)
                    {
                        return false;
                    }
                }
            }
            _ => return false,
        }

        let mut cursor = 0_u32;
        let mut owner_topology = Vec::new();
        let mut clip_nodes = Vec::new();
        let mut chunks = Vec::new();
        let mut op_count = 0usize;
        for (expected_index, step) in stamp.ordered_steps.iter().enumerate() {
            match step {
                RetainedSurfaceRasterStepStamp::ArtifactSpan(span) => {
                    let calculated_ops = span
                        .chunks
                        .iter()
                        .try_fold(0usize, |count, chunk| count.checked_add(chunk.op_count));
                    if span.step_index != expected_index
                        || span.opaque_order_span.start != cursor
                        || span.opaque_order_span.end < span.opaque_order_span.start
                        || calculated_ops != Some(span.op_count)
                    {
                        return false;
                    }
                    cursor = span.opaque_order_span.end;
                    owner_topology.extend(span.owner_topology.iter().copied());
                    clip_nodes.extend(span.clip_nodes.iter().copied());
                    chunks.extend(span.chunks.iter().cloned());
                    op_count = match op_count.checked_add(span.op_count) {
                        Some(value) => value,
                        None => return false,
                    };
                }
                RetainedSurfaceRasterStepStamp::NestedSurface(dependency) => {
                    let child_is_canonical = if dependency.child_stamp.identity.role
                        == RetainedSurfaceRasterRole::PropertyEffect
                    {
                        super::compiler::property_effect_surface_raster_stamp_is_canonical_at_depth(
                            &dependency.child_stamp,
                            depth.saturating_add(1),
                        )
                    } else {
                        validate(
                            &dependency.child_stamp,
                            depth.saturating_add(1),
                            identities,
                            owners,
                        )
                    };
                    if dependency.step_index != expected_index
                        || dependency.parent_opaque_order_before != cursor
                        || !child_is_canonical
                    {
                        return false;
                    }
                    let source_matches = match (
                        dependency.child_stamp.identity.role,
                        &dependency.child_composite_geometry,
                    ) {
                        (
                            RetainedSurfaceRasterRole::Transform,
                            super::RetainedSurfaceCompositeGeometryStamp::Transform {
                                source_bounds_bits,
                                ..
                            },
                        ) => {
                            *source_bounds_bits == dependency.child_stamp.target.source_bounds_bits
                        }
                        (
                            RetainedSurfaceRasterRole::PropertyEffect,
                            super::RetainedSurfaceCompositeGeometryStamp::PropertyEffect {
                                source_bounds_bits,
                                ..
                            },
                        ) => {
                            *source_bounds_bits == dependency.child_stamp.target.source_bounds_bits
                                && super::property_effect_composite_geometry_stamp_is_canonical(
                                    &dependency.child_composite_geometry,
                                )
                        }
                        _ => false,
                    };
                    let expected_after = if dependency.child_stamp.identity.role
                        == RetainedSurfaceRasterRole::PropertyEffect
                    {
                        cursor
                    } else {
                        cursor.max(dependency.child_stamp.opaque_order_span.end)
                    };
                    if !source_matches || dependency.parent_opaque_order_after != expected_after {
                        return false;
                    }
                    cursor = expected_after;
                }
                RetainedSurfaceRasterStepStamp::TransformEffectScrollChild(_)
                | RetainedSurfaceRasterStepStamp::ScrollBoundary(_)
                | RetainedSurfaceRasterStepStamp::EffectScrollBoundary(_) => return false,
            }
        }
        stamp.opaque_order_span == (0..cursor)
            && stamp.owner_topology == owner_topology
            && stamp.clip_nodes == clip_nodes
            && stamp.chunks == chunks
            && stamp.op_count == op_count
    }

    validate(
        stamp,
        depth,
        &mut FxHashSet::default(),
        &mut FxHashSet::default(),
    )
}

#[cfg(test)]
pub(crate) fn legacy_property_executor_rejects_effect_scroll_boundary_for_test(
    stamp: &RetainedSurfaceRasterStamp,
) -> bool {
    !property_effect_scene_surface_stamp_is_canonical(stamp, 0)
}

#[cfg(test)]
pub(crate) fn legacy_property_executor_rejects_transform_effect_scroll_child_for_test(
    stamp: &RetainedSurfaceRasterStamp,
) -> bool {
    !property_effect_scene_surface_stamp_is_canonical(stamp, 0)
}

fn validated_mixed_property_transform_raster_stamp(
    boundary_root: crate::view::node_arena::NodeKey,
    stable_id: u64,
    color_key: PersistentTextureKey,
    depth: usize,
    target: RetainedSurfaceRasterInputs,
    ordered_steps: Vec<RetainedSurfaceRasterStepStamp>,
    aggregate_opaque_order_span: std::ops::Range<u32>,
) -> Option<RetainedSurfaceRasterStamp> {
    if stable_id == 0
        || color_key != transformed_layer_stable_key(stable_id)
        || aggregate_opaque_order_span.start != 0
        || !ordered_steps.iter().any(|step| {
            matches!(
                step,
                RetainedSurfaceRasterStepStamp::NestedSurface(dependency)
                    if dependency.child_stamp.identity.role
                        == RetainedSurfaceRasterRole::PropertyEffect
            )
        })
    {
        return None;
    }
    let identity = super::RetainedSurfaceRasterIdentity {
        boundary_root,
        stable_id,
        color_key,
        role: RetainedSurfaceRasterRole::Transform,
        scroll_content_tile: None,
    };
    if !target.has_canonical_descriptor_pair_for(identity) {
        return None;
    }
    let mut cursor = 0_u32;
    let mut owner_topology = Vec::new();
    let mut clip_nodes = Vec::new();
    let mut chunks = Vec::new();
    let mut op_count = 0usize;
    for (expected_index, step) in ordered_steps.iter().enumerate() {
        match step {
            RetainedSurfaceRasterStepStamp::ArtifactSpan(span) => {
                if span.step_index != expected_index || span.opaque_order_span.start != cursor {
                    return None;
                }
                cursor = span.opaque_order_span.end;
                owner_topology.extend(span.owner_topology.iter().copied());
                clip_nodes.extend(span.clip_nodes.iter().copied());
                chunks.extend(span.chunks.iter().cloned());
                op_count = op_count.checked_add(span.op_count)?;
            }
            RetainedSurfaceRasterStepStamp::NestedSurface(dependency) => {
                if dependency.step_index != expected_index
                    || dependency.parent_opaque_order_before != cursor
                    || dependency.parent_opaque_order_after != cursor
                    || dependency.child_stamp.identity.role
                        != RetainedSurfaceRasterRole::PropertyEffect
                {
                    return None;
                }
            }
            RetainedSurfaceRasterStepStamp::TransformEffectScrollChild(_)
            | RetainedSurfaceRasterStepStamp::ScrollBoundary(_)
            | RetainedSurfaceRasterStepStamp::EffectScrollBoundary(_) => return None,
        }
    }
    if aggregate_opaque_order_span != (0..cursor) {
        return None;
    }
    let stamp = RetainedSurfaceRasterStamp {
        identity,
        target,
        owner_topology,
        clip_nodes,
        chunks,
        op_count,
        opaque_order_span: aggregate_opaque_order_span,
        ordered_steps,
        text_area_paint_grammar: None,
        interactive_text_area_resident: None,
        atomic_projection_text_area_resident: None,
        scroll_host: None,
        property_effect: None,
    };
    property_effect_scene_surface_stamp_is_canonical(&stamp, depth).then_some(stamp)
}

fn property_effect_scene_transaction_is_canonical(
    witness: &super::frame_plan::PropertySceneTransactionWitness,
    ordered_stamps: &[RetainedSurfaceRasterStamp],
    plan_step_count: usize,
    aggregate_opaque_order_span: &std::ops::Range<u32>,
) -> bool {
    use super::frame_plan::PropertySceneTransactionSurfaceKind;

    if ordered_stamps.is_empty()
        || witness.roots.is_empty()
        || witness.surfaces.len() != ordered_stamps.len()
        || witness.aggregate_opaque_order_span.start != 0
        || witness.aggregate_opaque_order_span != *aggregate_opaque_order_span
        || !witness
            .surfaces
            .iter()
            .any(|surface| matches!(surface.kind, PropertySceneTransactionSurfaceKind::Effect(_)))
    {
        return false;
    }
    let mut next_root_step = 0usize;
    let mut roots = FxHashMap::default();
    let mut root_stable_ids = FxHashSet::default();
    for (ordinal, root) in witness.roots.iter().enumerate() {
        if root.ordinal as usize != ordinal
            || root.stable_id == 0
            || root.top_level_step_span.start != next_root_step
            || root.top_level_step_span.end < root.top_level_step_span.start
            || roots.insert(root.root, ordinal as u32).is_some()
            || !root_stable_ids.insert(root.stable_id)
        {
            return false;
        }
        next_root_step = root.top_level_step_span.end;
    }
    if next_root_step != plan_step_count {
        return false;
    }
    let pure_effect_forest = witness
        .surfaces
        .iter()
        .all(|surface| matches!(surface.kind, PropertySceneTransactionSurfaceKind::Effect(_)));
    let exact_mixed_scene = witness.roots.len() == 1
        && witness.surfaces.len() == 2
        && matches!(
            witness.surfaces[0].kind,
            PropertySceneTransactionSurfaceKind::Transform(_)
        )
        && matches!(
            witness.surfaces[1].kind,
            PropertySceneTransactionSurfaceKind::Effect(_)
        )
        && witness.surfaces[0].parent_surface.is_none()
        && witness.surfaces[0].scene_root == witness.roots[0].root
        && witness.surfaces[0].boundary_root == witness.roots[0].root
        && witness.surfaces[1].parent_surface == Some(witness.surfaces[0].boundary_root)
        && witness.surfaces[1].scene_root == witness.roots[0].root;
    if !pure_effect_forest && !exact_mixed_scene {
        return false;
    }
    let mut surface_by_owner = FxHashMap::default();
    let mut stable_ids = FxHashSet::default();
    let mut resident_keys = FxHashSet::default();
    let mut depths = Vec::with_capacity(witness.surfaces.len());
    for (ordinal, (surface, stamp)) in witness.surfaces.iter().zip(ordered_stamps).enumerate() {
        let kind_matches = match surface.kind {
            PropertySceneTransactionSurfaceKind::Transform(id) => {
                id.0 == surface.boundary_root
                    && stamp.identity.role == RetainedSurfaceRasterRole::Transform
                    && surface.effect_composite.is_none()
                    && surface
                        .transform_viewport_matrix_bits
                        .is_some_and(|matrix| {
                            matrix.into_iter().map(f32::from_bits).all(f32::is_finite)
                        })
                    && surface.persistent_color_key
                        == transformed_layer_stable_key(surface.stable_id)
            }
            PropertySceneTransactionSurfaceKind::Effect(id) => {
                id.0 == surface.boundary_root
                    && stamp.identity.role == RetainedSurfaceRasterRole::PropertyEffect
                    && surface.transform_viewport_matrix_bits.is_none()
                    && surface.effect_composite.is_some()
                    && surface.persistent_color_key
                        == crate::view::base_component::isolation_layer_stable_key(
                            surface.stable_id,
                        )
            }
        };
        if surface.ordinal as usize != ordinal
            || !kind_matches
            || stamp.identity.boundary_root != surface.boundary_root
            || stamp.identity.stable_id != surface.stable_id
            || stamp.identity.color_key != surface.persistent_color_key
            || !roots.contains_key(&surface.scene_root)
            || surface_by_owner
                .insert(surface.boundary_root, ordinal)
                .is_some()
            || !stable_ids.insert(surface.stable_id)
            || !resident_keys.insert(stamp.identity.resident_key())
        {
            return false;
        }
        let depth = match surface.parent_surface {
            None => 0,
            Some(parent) => {
                let Some(&parent_ordinal) = surface_by_owner.get(&parent) else {
                    return false;
                };
                if witness.surfaces[parent_ordinal].scene_root != surface.scene_root {
                    return false;
                }
                depths[parent_ordinal] + 1
            }
        };
        if let PropertySceneTransactionSurfaceKind::Effect(_) = surface.kind {
            let Some(effect) = surface.effect_composite.as_ref() else {
                return false;
            };
            let expected_basis = match surface.parent_surface {
                None => super::frame_plan::PropertyIsolationCompositeBasis::FrameRoot,
                Some(parent) => {
                    let Some(&parent_ordinal) = surface_by_owner.get(&parent) else {
                        return false;
                    };
                    match witness.surfaces[parent_ordinal].kind {
                        PropertySceneTransactionSurfaceKind::Transform(transform) => {
                            let Some(viewport_matrix_bits) =
                                witness.surfaces[parent_ordinal].transform_viewport_matrix_bits
                            else {
                                return false;
                            };
                            super::frame_plan::PropertyIsolationCompositeBasis::ParentTransform {
                                transform,
                                viewport_matrix_bits,
                            }
                        }
                        PropertySceneTransactionSurfaceKind::Effect(effect) => {
                            super::frame_plan::PropertyIsolationCompositeBasis::ParentEffect(effect)
                        }
                    }
                }
            };
            if effect.mapping.basis != expected_basis
                || effect.mapping.rect_bits != stamp.target.source_bounds_bits
                || effect.mapping.effect_generation == 0
                || !(0.0..=1.0).contains(&f32::from_bits(effect.mapping.opacity_bits))
                || super::frame_plan::resolve_composite_scissor(
                    witness.outer_scissor_rect,
                    &effect.ancestor_composite_clips,
                )
                .ok()
                    != Some(effect.mapping.resolved_scissor)
            {
                return false;
            }
        }
        if !property_effect_scene_surface_stamp_is_canonical(stamp, depth) {
            return false;
        }
        depths.push(depth);
    }
    let mut nested_children = FxHashSet::default();
    for (parent_ordinal, stamp) in ordered_stamps.iter().enumerate() {
        for step in &stamp.ordered_steps {
            let RetainedSurfaceRasterStepStamp::NestedSurface(dependency) = step else {
                continue;
            };
            let Some(&child_ordinal) =
                surface_by_owner.get(&dependency.child_stamp.identity.boundary_root)
            else {
                return false;
            };
            if witness.surfaces[child_ordinal].parent_surface
                != Some(witness.surfaces[parent_ordinal].boundary_root)
                || dependency.child_stamp.as_ref() != &ordered_stamps[child_ordinal]
                || !nested_children.insert(child_ordinal)
            {
                return false;
            }
            if dependency.child_stamp.identity.role == RetainedSurfaceRasterRole::PropertyEffect {
                let Some(expected) = witness.surfaces[child_ordinal].effect_composite.as_ref()
                else {
                    return false;
                };
                let RetainedSurfaceCompositeGeometryStamp::PropertyEffect {
                    source_bounds_bits,
                    opacity_bits,
                    effect_generation,
                    basis,
                    resolved_scissor,
                    ancestor_composite_clips,
                } = &dependency.child_composite_geometry
                else {
                    return false;
                };
                let expected_basis = match expected.mapping.basis {
                    super::frame_plan::PropertyIsolationCompositeBasis::FrameRoot => {
                        return false;
                    }
                    super::frame_plan::PropertyIsolationCompositeBasis::ParentEffect(effect) => {
                        super::compiler::PropertyEffectCompositeBasisStamp::ParentEffect(effect)
                    }
                    super::frame_plan::PropertyIsolationCompositeBasis::ParentTransform {
                        transform,
                        viewport_matrix_bits,
                    } => super::compiler::PropertyEffectCompositeBasisStamp::ParentTransform {
                        transform,
                        viewport_matrix_bits,
                    },
                };
                if *source_bounds_bits != expected.mapping.rect_bits
                    || *opacity_bits != expected.mapping.opacity_bits
                    || *effect_generation != expected.mapping.effect_generation
                    || *basis != expected_basis
                    || *resolved_scissor != expected.mapping.resolved_scissor
                    || *ancestor_composite_clips != expected.ancestor_composite_clips
                {
                    return false;
                }
            }
        }
    }
    if witness
        .surfaces
        .iter()
        .enumerate()
        .any(|(ordinal, surface)| {
            surface.parent_surface.is_some() != nested_children.contains(&ordinal)
        })
    {
        return false;
    }
    let mut top_level = FxHashSet::default();
    let mut previous_step = None;
    for top in &witness.top_level_surfaces {
        let Some(surface) = witness.surfaces.get(top.surface_ordinal as usize) else {
            return false;
        };
        let Some(root) = witness.roots.get(top.scene_root_ordinal as usize) else {
            return false;
        };
        if surface.parent_surface.is_some()
            || surface.scene_root != root.root
            || !root.top_level_step_span.contains(&top.step_index)
            || previous_step.is_some_and(|previous| previous >= top.step_index)
            || !top_level.insert(top.surface_ordinal as usize)
        {
            return false;
        }
        previous_step = Some(top.step_index);
    }
    witness
        .surfaces
        .iter()
        .enumerate()
        .all(|(ordinal, surface)| surface.parent_surface.is_some() || top_level.contains(&ordinal))
}

/// Opaque preflight capability for one property scene. Construction freezes
/// every artifact proof, descriptor pair, pool action, and the scene
/// transaction while the frame graph is still untouched; the viewport can
/// only consume it through the infallible emission entry point below.
pub(crate) struct PreparedRetainedPropertyScene<'a> {
    prepared: PreparedPropertyScene<'a>,
    transaction: RetainedPropertySceneTransaction,
    ordered_stamps: Vec<RetainedSurfaceRasterStamp>,
    trace: RetainedPropertySceneBuildTrace,
}

impl RetainedPropertySceneBuildOutcome {
    pub(crate) fn into_parts(self) -> (BuildState, RetainedPropertySceneBuildTrace) {
        (self.state, self.trace)
    }
}

impl RetainedSurfaceTreeBuildOutcome {
    pub(crate) fn into_parts(self) -> (BuildState, Vec<RetainedSurfaceBuildTrace>) {
        (self.state, self.traces)
    }
}

/// The only production capability that can emit a retained transform surface.
/// Action selection is fixed to the viewport's real GPU-pool witness, and the
/// canonical full-set transaction is staged immediately after infallible emit.
pub(crate) fn build_retained_surface_from_pool(
    viewport: &mut Viewport,
    plan: &FramePaintPlan,
    graph: &mut FrameGraph,
    ctx: UiBuildContext,
) -> Result<RetainedSurfaceBuildOutcome, RetainedSurfacePrepareError> {
    let mut prepared = prepare_retained_surface(plan, graph, &ctx)?;
    let actions = viewport.retained_surface_compile_actions_from_pool(prepared.stamps());
    prepared.freeze_actions(actions);
    let trace = RetainedSurfaceBuildTrace::from_prepared(&prepared.root);
    let (state, stamps) = emit_prepared_retained_surface(prepared, graph, ctx);
    let [stamp] = <Vec<_> as TryInto<[RetainedSurfaceRasterStamp; 1]>>::try_into(stamps)
        .expect("production singleton preparation emits exactly one surface stamp");
    assert!(
        viewport.stage_retained_surface_full_set([stamp]),
        "prepared single-surface stamp must stage as a canonical full set"
    );
    Ok(RetainedSurfaceBuildOutcome { state, trace })
}

pub(crate) fn build_retained_isolation_surface_from_pool(
    viewport: &mut Viewport,
    plan: &FramePaintPlan,
    graph: &mut FrameGraph,
    ctx: UiBuildContext,
) -> Result<RetainedSurfaceBuildOutcome, RetainedSurfacePrepareError> {
    let mut prepared = prepare_retained_isolation_surface(plan, graph, &ctx)?;
    let actions = viewport.retained_surface_compile_actions_from_pool(prepared.stamps());
    prepared.freeze_actions(actions);
    let trace = RetainedSurfaceBuildTrace::from_prepared(&prepared.root);
    let (state, stamps) = emit_prepared_retained_surface(prepared, graph, ctx);
    let [stamp] = <Vec<_> as TryInto<[RetainedSurfaceRasterStamp; 1]>>::try_into(stamps)
        .expect("production isolation preparation emits exactly one surface stamp");
    assert!(
        viewport.stage_retained_surface_full_set([stamp]),
        "prepared isolation stamp must stage as a canonical full set"
    );
    Ok(RetainedSurfaceBuildOutcome { state, trace })
}

/// Production capability for the exact single-root baked scroll-host canary.
/// The complete scroll dependency is frozen before graph mutation, and pool
/// action selection treats offset-only changes as reraster inputs.
pub(crate) fn build_retained_scroll_host_surface_from_pool(
    viewport: &mut Viewport,
    plan: &FramePaintPlan,
    graph: &mut FrameGraph,
    ctx: UiBuildContext,
) -> Result<RetainedSurfaceBuildOutcome, RetainedSurfacePrepareError> {
    let mut prepared = prepare_retained_scroll_host_surface(plan, graph, &ctx)?;
    let actions = viewport.retained_surface_compile_actions_from_pool(prepared.stamps());
    prepared.freeze_actions(actions);
    let trace = RetainedSurfaceBuildTrace::from_prepared(&prepared.root);
    let (state, stamps) = emit_prepared_retained_surface(prepared, graph, ctx);
    let [stamp] = <Vec<_> as TryInto<[RetainedSurfaceRasterStamp; 1]>>::try_into(stamps)
        .expect("production scroll-host preparation emits exactly one surface stamp");
    assert!(
        viewport.stage_retained_surface_full_set([stamp]),
        "prepared scroll-host stamp must stage as a canonical full set"
    );
    Ok(RetainedSurfaceBuildOutcome { state, trace })
}

/// Production capability for the exact depth-two retained-surface tree.
/// Preparation validates the whole tree before mutation, actions come only
/// from real persistent-pool residency, and all stamps stage atomically.
pub(crate) fn build_retained_surface_tree_from_pool(
    viewport: &mut Viewport,
    plan: &FramePaintPlan,
    graph: &mut FrameGraph,
    ctx: UiBuildContext,
) -> Result<RetainedSurfaceTreeBuildOutcome, RetainedSurfacePrepareError> {
    let mut prepared = prepare_retained_surface_tree(plan, graph, &ctx)?;
    let actions = viewport.retained_surface_compile_actions_from_pool(prepared.stamps());
    prepared.freeze_actions(actions);
    let mut traces = Vec::new();
    prepared.root.collect_traces(&mut traces);
    let (state, stamps) = emit_prepared_retained_surface(prepared, graph, ctx);
    assert_eq!(
        stamps.len(),
        2,
        "production depth-two preparation emits exactly two surface stamps"
    );
    assert!(
        viewport.stage_retained_surface_full_set(stamps),
        "prepared retained-surface tree must stage as one canonical full set"
    );
    Ok(RetainedSurfaceTreeBuildOutcome { state, traces })
}

/// Production capability for the one exact mixed retained tree. Preparation
/// validates the complete Transform -> NestedIsolation shape before pool
/// actions are frozen or the graph is mutated, then stages exactly two stamps
/// as one atomic full set.
pub(crate) fn build_retained_effect_tree_from_pool(
    viewport: &mut Viewport,
    plan: &FramePaintPlan,
    graph: &mut FrameGraph,
    ctx: UiBuildContext,
) -> Result<RetainedSurfaceTreeBuildOutcome, RetainedSurfacePrepareError> {
    let mut prepared = prepare_retained_effect_tree(plan, graph, &ctx)?;
    let actions = viewport.retained_surface_compile_actions_from_pool(prepared.stamps());
    prepared.freeze_actions(actions);
    let mut traces = Vec::new();
    prepared.root.collect_traces(&mut traces);
    let (state, stamps) = emit_prepared_retained_surface(prepared, graph, ctx);
    assert_eq!(
        stamps.len(),
        2,
        "production mixed preparation emits exactly two surface stamps"
    );
    assert!(
        viewport.stage_retained_surface_full_set(stamps),
        "prepared mixed effect tree must stage as one canonical full set"
    );
    Ok(RetainedSurfaceTreeBuildOutcome { state, traces })
}

#[cfg(test)]
pub(crate) fn build_retained_property_scene_with_forced_pool_for_test(
    viewport: &mut Viewport,
    plan: &FramePaintPlan,
    graph: &mut FrameGraph,
    ctx: UiBuildContext,
) -> Result<RetainedPropertySceneBuildOutcome, RetainedSurfacePrepareError> {
    let prepared =
        prepare_retained_property_scene_with_pool_policy(viewport, plan, graph, &ctx, true)?;
    Ok(emit_prepared_retained_property_scene(
        viewport, prepared, graph, ctx,
    ))
}

pub(crate) fn prepare_retained_property_scene_from_pool<'a>(
    viewport: &Viewport,
    plan: &'a FramePaintPlan,
    graph: &FrameGraph,
    ctx: &UiBuildContext,
) -> Result<PreparedRetainedPropertyScene<'a>, RetainedSurfacePrepareError> {
    prepare_retained_property_scene_with_pool_policy(viewport, plan, graph, ctx, false)
}

#[cfg(test)]
pub(crate) fn prepare_retained_property_scene_stamps_for_test(
    viewport: &Viewport,
    plan: &FramePaintPlan,
    graph: &FrameGraph,
    ctx: &UiBuildContext,
) -> Result<Vec<RetainedSurfaceRasterStamp>, RetainedSurfacePrepareError> {
    Ok(
        prepare_retained_property_scene_with_pool_policy(viewport, plan, graph, ctx, false)?
            .ordered_stamps,
    )
}

fn prepare_retained_property_scene_with_pool_policy<'a>(
    viewport: &Viewport,
    plan: &'a FramePaintPlan,
    graph: &FrameGraph,
    ctx: &UiBuildContext,
    allow_forced_pair_witness: bool,
) -> Result<PreparedRetainedPropertyScene<'a>, RetainedSurfacePrepareError> {
    let mut prepared = prepare_property_scene(plan, graph, ctx)?;
    let ordered_stamps = prepared.stamps().into_iter().cloned().collect::<Vec<_>>();
    let plan_step_count = prepared.steps.len();
    let aggregate_opaque_order_span = prepared.recomputed_aggregate_opaque_order_span();
    let transaction = if prepared.transaction_witness.surfaces.iter().any(|surface| {
        matches!(
            surface.kind,
            super::frame_plan::PropertySceneTransactionSurfaceKind::Effect(_)
        )
    }) {
        RetainedPropertySceneTransaction::Effect(
            RetainedPropertyEffectSceneTransactionStamp::new(
                prepared.transaction_witness.clone(),
                &ordered_stamps,
                plan_step_count,
                aggregate_opaque_order_span,
            )
            .ok_or(RetainedSurfacePrepareError::ArtifactStore)?,
        )
    } else {
        RetainedPropertySceneTransaction::Transform(
            super::compiler::RetainedPropertySceneTransactionStamp::new(
                prepared.transaction_witness.clone(),
                &ordered_stamps,
            )
            .ok_or(RetainedSurfacePrepareError::ArtifactStore)?,
        )
    };
    let actions = if allow_forced_pair_witness {
        #[cfg(test)]
        {
            viewport.retained_surface_compile_actions_for_forced_test(prepared.stamps())
        }
        #[cfg(not(test))]
        unreachable!("forced property-scene pool policy is test-only")
    } else {
        viewport.retained_surface_compile_actions_from_pool(prepared.stamps())
    };
    prepared.freeze_actions(actions);
    let surfaces = prepared.collect_traces();
    let trace = RetainedPropertySceneBuildTrace {
        root_count: prepared.transaction_witness.roots.len(),
        surface_count: surfaces.len(),
        reraster_count: surfaces
            .iter()
            .filter(|surface| surface.action == RetainedSurfaceCompileAction::Reraster)
            .count(),
        reuse_count: surfaces
            .iter()
            .filter(|surface| surface.action == RetainedSurfaceCompileAction::Reuse)
            .count(),
        surfaces,
    };
    Ok(PreparedRetainedPropertyScene {
        prepared,
        transaction,
        ordered_stamps,
        trace,
    })
}

pub(crate) fn emit_prepared_retained_property_scene(
    viewport: &mut Viewport,
    prepared: PreparedRetainedPropertyScene<'_>,
    graph: &mut FrameGraph,
    ctx: UiBuildContext,
) -> RetainedPropertySceneBuildOutcome {
    let PreparedRetainedPropertyScene {
        prepared,
        transaction,
        ordered_stamps,
        trace,
    } = prepared;
    let (state, emitted_stamps) = emit_prepared_property_scene_graph(prepared, graph, ctx);
    let expected = ordered_stamps
        .iter()
        .map(|stamp| (stamp.identity.resident_key(), stamp))
        .collect::<FxHashMap<_, _>>();
    let emitted = emitted_stamps
        .iter()
        .map(|stamp| (stamp.identity.resident_key(), stamp))
        .collect::<FxHashMap<_, _>>();
    assert_eq!(
        emitted, expected,
        "property-scene emission must cover the preflight-frozen surface set exactly"
    );
    assert!(
        viewport.stage_retained_property_scene(transaction, ordered_stamps),
        "preflight-sealed property-scene transaction must stage atomically"
    );
    RetainedPropertySceneBuildOutcome { state, trace }
}

#[cfg(test)]
pub(crate) use RetainedSurfacePrepareError as ForcedTransformSurfaceError;

#[cfg(test)]
pub(crate) fn execute_forced_transform_surface_for_test(
    viewport: &mut Viewport,
    plan: &FramePaintPlan,
    graph: &mut FrameGraph,
    ctx: UiBuildContext,
) -> Result<BuildState, ForcedTransformSurfaceError> {
    let mut prepared = prepare_frame_paint_plan_forced(plan, graph, &ctx)?;
    let actions = viewport.retained_surface_compile_actions_for_forced_test(prepared.stamps());
    prepared.freeze_actions(actions);
    let (state, stamps) = emit_prepared_retained_surface(prepared, graph, ctx);
    assert!(
        viewport.stage_retained_surface_full_set(stamps),
        "prepared full tree must stage as one canonical full set"
    );
    Ok(state)
}
