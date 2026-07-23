use std::{ops::Range, sync::Arc};

use rustc_hash::{FxHashMap, FxHashSet};
use slotmap::Key;

use crate::view::base_component::{
    AncestorClipContext, BuildState,
    RetainedScrollAtomicProjectionSelectionTextAreaSubtreeAdmissionSnapshot,
    RetainedScrollAtomicProjectionTextAreaSubtreeAdmissionSnapshot,
    RetainedScrollFocusedAtomicProjectionTextAreaSubtreeAdmissionSnapshot,
    RetainedScrollHostAdmissionSnapshot, RetainedScrollInteractiveTextAreaSubtreeAdmissionSnapshot,
    RetainedScrollTextAreaSubtreeAdmissionSnapshot, RetainedScrollTransformHostAdmissionSnapshot,
    RetainedSurfaceBounds, UiBuildContext, persistent_target_texture_descriptors,
    scroll_content_layer_stable_key, text_area::FocusedAtomicCaretSourcePaintSeal,
    texture_desc_for_logical_bounds, transient_target_texture_descriptors,
};
use crate::view::compositor::property_tree::{
    ClipBehavior, ClipNodeId, ClipNodeRole, ClipNodeSnapshot, EffectNodeSnapshot,
    PropertyTreeState, ScrollNodeId, ScrollNodeSnapshot, TransformNodeId, TransformNodeSnapshot,
};
use crate::view::compositor::{PaintGenerationTracker, PropertyTrees};
use crate::view::frame_graph::texture_resource::TextureDesc;
use crate::view::frame_graph::{FrameGraph, PersistentTextureKey};
use crate::view::node_arena::{NodeArena, NodeKey};
use crate::view::render_pass::ClearPass;
use crate::view::render_pass::composite_layer_pass::{
    CompositeLayerInput, CompositeLayerOutput, CompositeLayerParams, CompositeLayerPass, LayerIn,
};
use crate::view::render_pass::draw_rect_pass::{
    DrawRectInput, DrawRectOutput, DrawRectPass, RenderTargetOut,
};
use crate::view::render_pass::texture_composite_pass::{
    TextureCompositeInput, TextureCompositeOutput, TextureCompositeParams, TextureCompositePass,
    TextureCompositeSourceIn,
};
use crate::view::viewport::{RetainedSurfaceFrameStageOwner, Viewport};

use super::artifact::RetainedInteractiveTextAreaResidentRasterSeal;
use super::compiler::{
    AtomicProjectionSelectionTextAreaPlanIdentity, AtomicProjectionTextAreaPlanIdentity,
    RetainedAtomicProjectionTextAreaResidentRasterSeal,
    ValidatedScrollSceneAtomicProjectionSelectionTextAreaHostEmission,
    ValidatedScrollSceneAtomicProjectionSelectionTextAreaPlanParts,
    ValidatedScrollSceneAtomicProjectionTextAreaHostEmission,
    ValidatedScrollSceneAtomicProjectionTextAreaPlanParts, ValidatedScrollSceneContentArtifact,
    ValidatedScrollSceneFocusedAtomicProjectionTextAreaPlanParts,
    ValidatedScrollSceneHostBeforeArtifact, ValidatedScrollSceneOverlayArtifact,
    emit_validated_scroll_scene_atomic_projection_selection_text_area_content,
    emit_validated_scroll_scene_atomic_projection_selection_text_area_host,
    emit_validated_scroll_scene_atomic_projection_selection_text_area_overlay,
    emit_validated_scroll_scene_atomic_projection_text_area_content,
    emit_validated_scroll_scene_atomic_projection_text_area_host,
    emit_validated_scroll_scene_atomic_projection_text_area_overlay,
    emit_validated_scroll_scene_content_artifact, emit_validated_scroll_scene_host_before_artifact,
    emit_validated_scroll_scene_overlay_artifact,
    prepare_validated_scroll_scene_atomic_projection_selection_text_area_emission,
    prepare_validated_scroll_scene_atomic_projection_text_area_emission,
    reuse_validated_scroll_scene_atomic_projection_selection_text_area_content,
    reuse_validated_scroll_scene_atomic_projection_text_area_content,
    validate_nested_scroll_content_artifact, validate_scroll_scene_content_artifact,
    validate_scroll_scene_host_before_artifact,
    validate_scroll_scene_interactive_text_area_content_artifact,
    validate_scroll_scene_overlay_artifact, validate_scroll_scene_text_area_content_artifact,
    validated_scroll_atomic_projection_selection_text_area_content_raster_stamp,
    validated_scroll_atomic_projection_text_area_content_raster_stamp,
    validated_scroll_content_artifact_span_stamp,
    validated_scroll_interactive_text_area_content_raster_stamp,
};
use super::frame_plan::opaque_order_count;
use super::{
    FramePaintPlanError, FramePaintPlanRejection, PaintArtifact, PaintArtifactTarget,
    PaintOwnerSnapshot, PaintScrollContentWitness, PaintScrollTextAreaSubtreeWitness,
    PreparedScrollContentCompositeGeometry, RecordedRetainedTextAreaCaretOverlay,
    RetainedSurfaceCompileAction, RetainedSurfaceRasterInputs, RetainedSurfaceRasterRole,
    RetainedSurfaceRasterStamp, validated_scroll_content_raster_stamp,
    validated_scroll_text_area_content_raster_stamp,
};

/// Planner-private union over the two exact single-scroll host corpora.  The
/// direct variant continues to carry the direct-leaf admission type; C1 never
/// constructs or impersonates that witness for its non-leaf wrapper.
#[derive(Clone, Debug)]
enum PropertyScrollHostAdmissionKind {
    DirectLeaf(RetainedScrollHostAdmissionSnapshot),
    TextAreaSubtree(RetainedScrollTextAreaSubtreeAdmissionSnapshot),
    InteractiveTextAreaSubtree(RetainedScrollInteractiveTextAreaSubtreeAdmissionSnapshot),
    AtomicProjectionTextAreaSubtree(RetainedScrollAtomicProjectionTextAreaSubtreeAdmissionSnapshot),
    FocusedAtomicProjectionTextAreaSubtree(
        Box<RetainedScrollFocusedAtomicProjectionTextAreaSubtreeAdmissionSnapshot>,
    ),
    AtomicProjectionSelectionTextAreaSubtree(
        RetainedScrollAtomicProjectionSelectionTextAreaSubtreeAdmissionSnapshot,
    ),
}

#[derive(Clone, Debug)]
struct PropertyScrollInteractiveTextAreaCaretSeal {
    recorded: RecordedRetainedTextAreaCaretOverlay,
}

impl PropertyScrollInteractiveTextAreaCaretSeal {
    fn from_recorded(recorded: &RecordedRetainedTextAreaCaretOverlay) -> Option<Self> {
        recorded.is_canonical().then(|| Self {
            recorded: recorded.clone(),
        })
    }

    fn is_canonical(&self) -> bool {
        self.recorded.is_canonical()
    }
}

impl PartialEq for PropertyScrollInteractiveTextAreaCaretSeal {
    fn eq(&self, other: &Self) -> bool {
        self.is_canonical()
            && other.is_canonical()
            && self.recorded.identity == other.recorded.identity
            && self
                .recorded
                .op
                .as_ref()
                .and_then(|op| super::PaintPayloadIdentity::prepared_rects([op]))
                == other
                    .recorded
                    .op
                    .as_ref()
                    .and_then(|op| super::PaintPayloadIdentity::prepared_rects([op]))
    }
}

impl Eq for PropertyScrollInteractiveTextAreaCaretSeal {}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PropertyScrollFocusedAtomicProjectionSidecarSeal {
    caret: crate::view::base_component::text_area::FocusedAtomicCaretSourceSeal,
    preedit: Option<crate::view::base_component::text_area::FocusedAtomicPreeditSourceSeal>,
    text_area_clip: ClipNodeSnapshot,
    outer_clip: ClipNodeSnapshot,
}

impl PropertyScrollFocusedAtomicProjectionSidecarSeal {
    fn new(
        caret: &crate::view::base_component::text_area::FocusedAtomicCaretSourceSeal,
        preedit: Option<&crate::view::base_component::text_area::FocusedAtomicPreeditSourceSeal>,
        text_area_clip: ClipNodeSnapshot,
        outer_clip: ClipNodeSnapshot,
    ) -> Option<Self> {
        let seal = Self {
            caret: caret.clone(),
            preedit: preedit.cloned(),
            text_area_clip,
            outer_clip,
        };
        seal.is_canonical().then_some(seal)
    }

    fn is_canonical(&self) -> bool {
        self.caret.is_canonical()
            && self.preedit.as_ref().is_none_or(|preedit| {
                preedit.is_canonical()
                    && preedit.owner == self.caret.owner
                    && preedit.stable_id == self.caret.stable_id
                    && preedit.cursor_char == self.caret.cursor_char
                    && preedit.cursor_affinity == self.caret.cursor_affinity
                    && preedit.ime_preedit_cursor == self.caret.ime_preedit_cursor
                    && preedit.foreground_color_bits == self.caret.foreground_color_bits
                    && preedit.unified_ifc_source_revision == self.caret.unified_ifc_source_revision
                    && preedit.last_unified_apply_bits == self.caret.last_unified_apply_bits
            })
            && self.text_area_clip.id.owner == self.caret.owner
            && self.text_area_clip.owner == self.caret.owner
            && self.text_area_clip.id.role == ClipNodeRole::ContentsClip
            && self.text_area_clip.parent == Some(self.outer_clip.id)
            && self.text_area_clip.behavior == ClipBehavior::Intersect
            && self.text_area_clip.generation != 0
            && self.outer_clip.id.owner == self.outer_clip.owner
            && self.outer_clip.id.role == ClipNodeRole::ContentsClip
            && self.outer_clip.owner != self.caret.owner
            && self.outer_clip.parent.is_none()
            && self.outer_clip.behavior == ClipBehavior::Intersect
            && self.outer_clip.generation != 0
            && self.text_area_clip.logical_scissor[0]
                .checked_add(self.text_area_clip.logical_scissor[2])
                .is_some()
            && self.text_area_clip.logical_scissor[1]
                .checked_add(self.text_area_clip.logical_scissor[3])
                .is_some()
            && self.outer_clip.logical_scissor[0]
                .checked_add(self.outer_clip.logical_scissor[2])
                .is_some()
            && self.outer_clip.logical_scissor[1]
                .checked_add(self.outer_clip.logical_scissor[3])
                .is_some()
    }

    fn draw_op(&self) -> Option<super::DrawRectOp> {
        if !self.is_canonical() {
            return None;
        }
        let FocusedAtomicCaretSourcePaintSeal::Present {
            bounds_bits,
            payload_identity,
        } = &self.caret.paint
        else {
            return None;
        };
        let [x, y, width, height] = bounds_bits.map(f32::from_bits);
        let op = super::DrawRectOp {
            params: crate::view::render_pass::draw_rect_pass::RectPassParams {
                position: [x, y],
                size: [width, height],
                fill_color: self.caret.foreground_color_bits.map(f32::from_bits),
                opacity: 1.0,
                ..Default::default()
            },
            mode: crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly,
        };
        (super::PaintPayloadIdentity::prepared_rects([&op]).as_ref() == Some(payload_identity))
            .then_some(op)
    }

    fn preedit_draw_op(&self) -> Option<super::DrawRectOp> {
        let preedit = self.preedit.as_ref()?;
        if !self.is_canonical() {
            return None;
        }
        let [x, y, width, height] = preedit.underline_bounds_bits.map(f32::from_bits);
        let op = super::DrawRectOp {
            params: crate::view::render_pass::draw_rect_pass::RectPassParams {
                position: [x, y],
                size: [width, height],
                fill_color: preedit.foreground_color_bits.map(f32::from_bits),
                opacity: 1.0,
                ..Default::default()
            },
            mode: crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly,
        };
        (super::PaintPayloadIdentity::prepared_rects([&op]).as_ref()
            == Some(&preedit.underline_identity))
        .then_some(op)
    }
}

/// Closed schedule attached to the content-composite boundary. Existing
/// grammars have no post-composite work; TextArea variants carry only
/// compiler-sealed sidecars such as caret and IME preedit underline. The forest
/// prepare path freezes them before mutation and the emitter consumes them only
/// at the content-composite edge, keeping resident content raster seals free of
/// focus adornments.
#[derive(Clone, Debug, PartialEq, Eq)]
enum PropertyScrollPostCompositeSchedule {
    NoneForExistingGrammar,
    InteractiveTextAreaCaret(PropertyScrollInteractiveTextAreaCaretSeal),
    FocusedAtomicProjectionSidecars(Box<PropertyScrollFocusedAtomicProjectionSidecarSeal>),
}

impl PropertyScrollPostCompositeSchedule {
    fn clip_intersection(text_area: [u32; 4], outer: [u32; 4]) -> Option<[u32; 4]> {
        let left = text_area[0].max(outer[0]);
        let top = text_area[1].max(outer[1]);
        let right = text_area[0]
            .checked_add(text_area[2])?
            .min(outer[0].checked_add(outer[2])?);
        let bottom = text_area[1]
            .checked_add(text_area[3])?
            .min(outer[1].checked_add(outer[3])?);
        (right > left && bottom > top).then_some([left, top, right - left, bottom - top])
    }

    fn opaque_order_delta(&self) -> Option<u32> {
        match self {
            Self::NoneForExistingGrammar => Some(0),
            Self::InteractiveTextAreaCaret(caret) => {
                if !caret.is_canonical() {
                    return None;
                }
                let Some(op) = caret.recorded.op.as_ref() else {
                    return Some(0);
                };
                if Self::clip_intersection(
                    caret.recorded.identity.text_area_clip.logical_scissor,
                    caret.recorded.identity.outer_clip.logical_scissor,
                )
                .is_none()
                {
                    return Some(0);
                }
                let mut pass = DrawRectPass::new(
                    op.params.clone(),
                    DrawRectInput::default(),
                    DrawRectOutput::default(),
                );
                pass.set_render_mode(op.mode);
                Some(u32::from(pass.is_opaque_candidate()))
            }
            Self::FocusedAtomicProjectionSidecars(sidecars) => {
                if Self::clip_intersection(
                    sidecars.text_area_clip.logical_scissor,
                    sidecars.outer_clip.logical_scissor,
                )
                .is_none()
                {
                    return Some(0);
                }
                let mut count = 0u32;
                for op in [sidecars.preedit_draw_op(), sidecars.draw_op()]
                    .into_iter()
                    .flatten()
                {
                    let mut pass = DrawRectPass::new(
                        op.params,
                        DrawRectInput::default(),
                        DrawRectOutput::default(),
                    );
                    pass.set_render_mode(op.mode);
                    count = count.checked_add(u32::from(pass.is_opaque_candidate()))?;
                }
                sidecars.is_canonical().then_some(count)
            }
        }
    }

    fn emit(self, graph: &mut FrameGraph, ctx: &mut UiBuildContext) {
        let (ops, text_area, outer) = match self {
            Self::NoneForExistingGrammar => return,
            Self::InteractiveTextAreaCaret(caret) => {
                assert!(caret.is_canonical());
                let Some(op) = caret.recorded.op else {
                    return;
                };
                (
                    vec![op],
                    caret.recorded.identity.text_area_clip.logical_scissor,
                    caret.recorded.identity.outer_clip.logical_scissor,
                )
            }
            Self::FocusedAtomicProjectionSidecars(sidecars) => {
                assert!(sidecars.is_canonical());
                let Some(op) = sidecars.draw_op() else {
                    return;
                };
                let mut ops = Vec::new();
                if let Some(preedit) = sidecars.preedit_draw_op() {
                    ops.push(preedit);
                }
                ops.push(op);
                (
                    ops,
                    sidecars.text_area_clip.logical_scissor,
                    sidecars.outer_clip.logical_scissor,
                )
            }
        };
        let Some(scissor) = Self::clip_intersection(text_area, outer) else {
            return;
        };
        let previous = ctx.replace_scissor_rect(Some(scissor));
        for op in ops {
            let mut pass = DrawRectPass::new(
                op.params,
                DrawRectInput::default(),
                DrawRectOutput::default(),
            );
            pass.set_render_mode(op.mode);
            ctx.emit_draw_rect_pass(graph, pass);
        }
        ctx.replace_scissor_rect(previous);
    }
}

#[derive(Clone, Debug)]
struct PropertyScrollHostAdmission {
    boundary_root: NodeKey,
    stable_id: u64,
    child: NodeKey,
    child_stable_id: u64,
    source_bounds: RetainedSurfaceBounds,
    kind: PropertyScrollHostAdmissionKind,
}

impl PropertyScrollHostAdmission {
    fn direct_leaf(admission: RetainedScrollHostAdmissionSnapshot) -> Self {
        Self {
            boundary_root: admission.boundary_root,
            stable_id: admission.stable_id,
            child: admission.child,
            child_stable_id: admission.child_stable_id,
            source_bounds: admission.source_bounds,
            kind: PropertyScrollHostAdmissionKind::DirectLeaf(admission),
        }
    }

    fn text_area_subtree(admission: RetainedScrollTextAreaSubtreeAdmissionSnapshot) -> Self {
        Self {
            boundary_root: admission.boundary_root,
            stable_id: admission.stable_id,
            child: admission.content_wrapper,
            child_stable_id: admission.content_wrapper_stable_id,
            source_bounds: admission.source_bounds,
            kind: PropertyScrollHostAdmissionKind::TextAreaSubtree(admission),
        }
    }

    fn interactive_text_area_subtree(
        admission: RetainedScrollInteractiveTextAreaSubtreeAdmissionSnapshot,
    ) -> Self {
        Self {
            boundary_root: admission.boundary_root,
            stable_id: admission.stable_id,
            child: admission.content_wrapper,
            child_stable_id: admission.content_wrapper_stable_id,
            source_bounds: admission.source_bounds,
            kind: PropertyScrollHostAdmissionKind::InteractiveTextAreaSubtree(admission),
        }
    }

    fn atomic_projection_text_area_subtree(
        admission: RetainedScrollAtomicProjectionTextAreaSubtreeAdmissionSnapshot,
    ) -> Self {
        Self {
            boundary_root: admission.boundary_root,
            stable_id: admission.stable_id,
            child: admission.content_wrapper,
            child_stable_id: admission.content_wrapper_stable_id,
            source_bounds: admission.source_bounds,
            kind: PropertyScrollHostAdmissionKind::AtomicProjectionTextAreaSubtree(admission),
        }
    }

    fn atomic_projection_selection_text_area_subtree(
        admission: RetainedScrollAtomicProjectionSelectionTextAreaSubtreeAdmissionSnapshot,
    ) -> Self {
        Self {
            boundary_root: admission.boundary_root,
            stable_id: admission.stable_id,
            child: admission.content_wrapper,
            child_stable_id: admission.content_wrapper_stable_id,
            source_bounds: admission.source_bounds,
            kind: PropertyScrollHostAdmissionKind::AtomicProjectionSelectionTextAreaSubtree(
                admission,
            ),
        }
    }

    fn focused_atomic_projection_text_area_subtree(
        admission: RetainedScrollFocusedAtomicProjectionTextAreaSubtreeAdmissionSnapshot,
    ) -> Self {
        Self {
            boundary_root: admission.boundary_root,
            stable_id: admission.stable_id,
            child: admission.content_wrapper,
            child_stable_id: admission.content_wrapper_stable_id,
            source_bounds: admission.source_bounds,
            kind: PropertyScrollHostAdmissionKind::FocusedAtomicProjectionTextAreaSubtree(
                Box::new(admission),
            ),
        }
    }

    fn matches_scroll_node(&self, scroll: ScrollNodeSnapshot) -> bool {
        match &self.kind {
            PropertyScrollHostAdmissionKind::DirectLeaf(admission) => {
                (*admission).matches_scroll_node(scroll)
            }
            PropertyScrollHostAdmissionKind::TextAreaSubtree(admission) => {
                (*admission).matches_scroll_node(scroll)
            }
            PropertyScrollHostAdmissionKind::InteractiveTextAreaSubtree(admission) => {
                (*admission).matches_scroll_node(scroll)
            }
            PropertyScrollHostAdmissionKind::AtomicProjectionTextAreaSubtree(admission) => {
                admission.matches_scroll_node(scroll)
            }
            PropertyScrollHostAdmissionKind::FocusedAtomicProjectionTextAreaSubtree(admission) => {
                admission.matches_scroll_node(scroll)
            }
            PropertyScrollHostAdmissionKind::AtomicProjectionSelectionTextAreaSubtree(
                admission,
            ) => admission.matches_scroll_node(scroll),
        }
    }

    fn outer_bitwise_eq(&self, other: &Self) -> bool {
        self.boundary_root == other.boundary_root
            && self.stable_id == other.stable_id
            && self.child == other.child
            && self.child_stable_id == other.child_stable_id
            && [
                self.source_bounds.x,
                self.source_bounds.y,
                self.source_bounds.width,
                self.source_bounds.height,
            ]
            .map(f32::to_bits)
                == [
                    other.source_bounds.x,
                    other.source_bounds.y,
                    other.source_bounds.width,
                    other.source_bounds.height,
                ]
                .map(f32::to_bits)
            && self.source_bounds.corner_radii.map(f32::to_bits)
                == other.source_bounds.corner_radii.map(f32::to_bits)
    }

    fn has_exact_kind_projection(&self) -> bool {
        let projected = match &self.kind {
            PropertyScrollHostAdmissionKind::DirectLeaf(admission) => Self::direct_leaf(*admission),
            PropertyScrollHostAdmissionKind::TextAreaSubtree(admission) => {
                Self::text_area_subtree(*admission)
            }
            PropertyScrollHostAdmissionKind::InteractiveTextAreaSubtree(admission) => {
                Self::interactive_text_area_subtree(*admission)
            }
            PropertyScrollHostAdmissionKind::AtomicProjectionTextAreaSubtree(admission) => {
                Self::atomic_projection_text_area_subtree(admission.clone())
            }
            PropertyScrollHostAdmissionKind::FocusedAtomicProjectionTextAreaSubtree(admission) => {
                Self::focused_atomic_projection_text_area_subtree((**admission).clone())
            }
            PropertyScrollHostAdmissionKind::AtomicProjectionSelectionTextAreaSubtree(
                admission,
            ) => Self::atomic_projection_selection_text_area_subtree(admission.clone()),
        };
        self.outer_bitwise_eq(&projected)
    }

    fn exactly_corresponds_to(
        &self,
        sidecar: Option<RetainedScrollTextAreaSubtreeAdmissionSnapshot>,
        interactive_sidecar: Option<RetainedScrollInteractiveTextAreaSubtreeAdmissionSnapshot>,
        post_composite: &PropertyScrollPostCompositeSchedule,
    ) -> bool {
        self.exactly_corresponds_to_with_atomic(
            sidecar,
            interactive_sidecar,
            None,
            None,
            post_composite,
        )
    }

    fn exactly_corresponds_to_with_atomic(
        &self,
        sidecar: Option<RetainedScrollTextAreaSubtreeAdmissionSnapshot>,
        interactive_sidecar: Option<RetainedScrollInteractiveTextAreaSubtreeAdmissionSnapshot>,
        atomic_sidecar: Option<&RetainedScrollAtomicProjectionTextAreaSubtreeAdmissionSnapshot>,
        focused_atomic_sidecar: Option<
            &RetainedScrollFocusedAtomicProjectionTextAreaSubtreeAdmissionSnapshot,
        >,
        post_composite: &PropertyScrollPostCompositeSchedule,
    ) -> bool {
        self.has_exact_kind_projection()
            && match (
                &self.kind,
                sidecar,
                interactive_sidecar,
                atomic_sidecar,
                focused_atomic_sidecar,
                post_composite,
            ) {
                (
                    PropertyScrollHostAdmissionKind::DirectLeaf(_),
                    None,
                    None,
                    None,
                    None,
                    PropertyScrollPostCompositeSchedule::NoneForExistingGrammar,
                ) => true,
                (
                    PropertyScrollHostAdmissionKind::TextAreaSubtree(inline),
                    Some(sidecar),
                    None,
                    None,
                    None,
                    PropertyScrollPostCompositeSchedule::NoneForExistingGrammar,
                ) => (*inline).bitwise_eq(sidecar),
                (
                    PropertyScrollHostAdmissionKind::InteractiveTextAreaSubtree(inline),
                    None,
                    Some(sidecar),
                    None,
                    None,
                    PropertyScrollPostCompositeSchedule::InteractiveTextAreaCaret(caret),
                ) => {
                    (*inline).bitwise_eq(sidecar)
                        && caret.is_canonical()
                        && caret.recorded.identity.owner == inline.text_area_root
                        && caret.recorded.identity.stable_id == inline.text_area_stable_id
                        && caret.recorded.identity.oracle_bounds_bits
                            == inline.caret_oracle_bounds_bits
                }
                (
                    PropertyScrollHostAdmissionKind::AtomicProjectionTextAreaSubtree(inline),
                    None,
                    None,
                    Some(sidecar),
                    None,
                    PropertyScrollPostCompositeSchedule::NoneForExistingGrammar,
                ) => inline.bitwise_eq(sidecar),
                (
                    PropertyScrollHostAdmissionKind::FocusedAtomicProjectionTextAreaSubtree(inline),
                    None,
                    None,
                    None,
                    Some(sidecar),
                    PropertyScrollPostCompositeSchedule::FocusedAtomicProjectionSidecars(caret),
                ) => {
                    inline.bitwise_eq(sidecar)
                        && caret.is_canonical()
                        && caret.caret.owner == inline.text_area_root
                        && caret.caret.stable_id == inline.text_area_stable_id
                        && caret.text_area_clip.id.owner == inline.text_area_root
                        && caret.text_area_clip.owner == inline.text_area_root
                }
                (
                    PropertyScrollHostAdmissionKind::AtomicProjectionSelectionTextAreaSubtree(_),
                    None,
                    None,
                    None,
                    None,
                    PropertyScrollPostCompositeSchedule::NoneForExistingGrammar,
                ) => true,
                _ => false,
            }
    }

    fn text_area_subtree_snapshot(&self) -> Option<RetainedScrollTextAreaSubtreeAdmissionSnapshot> {
        self.has_exact_kind_projection().then_some(())?;
        match &self.kind {
            PropertyScrollHostAdmissionKind::TextAreaSubtree(admission) => Some(*admission),
            PropertyScrollHostAdmissionKind::DirectLeaf(_)
            | PropertyScrollHostAdmissionKind::InteractiveTextAreaSubtree(_)
            | PropertyScrollHostAdmissionKind::AtomicProjectionTextAreaSubtree(_)
            | PropertyScrollHostAdmissionKind::FocusedAtomicProjectionTextAreaSubtree(_)
            | PropertyScrollHostAdmissionKind::AtomicProjectionSelectionTextAreaSubtree(_) => None,
        }
    }

    fn interactive_text_area_subtree_snapshot(
        &self,
    ) -> Option<RetainedScrollInteractiveTextAreaSubtreeAdmissionSnapshot> {
        self.has_exact_kind_projection().then_some(())?;
        match &self.kind {
            PropertyScrollHostAdmissionKind::InteractiveTextAreaSubtree(admission) => {
                Some(*admission)
            }
            PropertyScrollHostAdmissionKind::DirectLeaf(_)
            | PropertyScrollHostAdmissionKind::TextAreaSubtree(_)
            | PropertyScrollHostAdmissionKind::AtomicProjectionTextAreaSubtree(_)
            | PropertyScrollHostAdmissionKind::FocusedAtomicProjectionTextAreaSubtree(_)
            | PropertyScrollHostAdmissionKind::AtomicProjectionSelectionTextAreaSubtree(_) => None,
        }
    }

    fn atomic_projection_text_area_subtree_snapshot(
        &self,
    ) -> Option<&RetainedScrollAtomicProjectionTextAreaSubtreeAdmissionSnapshot> {
        self.has_exact_kind_projection().then_some(())?;
        match &self.kind {
            PropertyScrollHostAdmissionKind::AtomicProjectionTextAreaSubtree(admission) => {
                Some(admission)
            }
            PropertyScrollHostAdmissionKind::DirectLeaf(_)
            | PropertyScrollHostAdmissionKind::TextAreaSubtree(_)
            | PropertyScrollHostAdmissionKind::InteractiveTextAreaSubtree(_)
            | PropertyScrollHostAdmissionKind::FocusedAtomicProjectionTextAreaSubtree(_)
            | PropertyScrollHostAdmissionKind::AtomicProjectionSelectionTextAreaSubtree(_) => None,
        }
    }

    fn focused_atomic_projection_text_area_subtree_snapshot(
        &self,
    ) -> Option<&RetainedScrollFocusedAtomicProjectionTextAreaSubtreeAdmissionSnapshot> {
        self.has_exact_kind_projection().then_some(())?;
        match &self.kind {
            PropertyScrollHostAdmissionKind::FocusedAtomicProjectionTextAreaSubtree(admission) => {
                Some(admission)
            }
            PropertyScrollHostAdmissionKind::DirectLeaf(_)
            | PropertyScrollHostAdmissionKind::TextAreaSubtree(_)
            | PropertyScrollHostAdmissionKind::InteractiveTextAreaSubtree(_)
            | PropertyScrollHostAdmissionKind::AtomicProjectionTextAreaSubtree(_)
            | PropertyScrollHostAdmissionKind::AtomicProjectionSelectionTextAreaSubtree(_) => None,
        }
    }

    fn atomic_projection_selection_text_area_subtree_snapshot(
        &self,
    ) -> Option<&RetainedScrollAtomicProjectionSelectionTextAreaSubtreeAdmissionSnapshot> {
        self.has_exact_kind_projection().then_some(())?;
        match &self.kind {
            PropertyScrollHostAdmissionKind::AtomicProjectionSelectionTextAreaSubtree(
                admission,
            ) => Some(admission),
            PropertyScrollHostAdmissionKind::DirectLeaf(_)
            | PropertyScrollHostAdmissionKind::TextAreaSubtree(_)
            | PropertyScrollHostAdmissionKind::InteractiveTextAreaSubtree(_)
            | PropertyScrollHostAdmissionKind::AtomicProjectionTextAreaSubtree(_)
            | PropertyScrollHostAdmissionKind::FocusedAtomicProjectionTextAreaSubtree(_) => None,
        }
    }

    #[cfg(test)]
    fn exactly_corresponds_to_resident(
        &self,
        resident: Option<&RetainedInteractiveTextAreaResidentRasterSeal>,
    ) -> bool {
        self.exactly_corresponds_to_resident_with_atomic(resident, None)
    }

    fn exactly_corresponds_to_resident_with_atomic(
        &self,
        interactive_resident: Option<&RetainedInteractiveTextAreaResidentRasterSeal>,
        atomic_resident: Option<&RetainedAtomicProjectionTextAreaResidentRasterSeal>,
    ) -> bool {
        self.has_exact_kind_projection()
            && match (&self.kind, interactive_resident, atomic_resident) {
                (PropertyScrollHostAdmissionKind::DirectLeaf(_), None, None)
                | (PropertyScrollHostAdmissionKind::TextAreaSubtree(_), None, None) => true,
                (
                    PropertyScrollHostAdmissionKind::InteractiveTextAreaSubtree(admission),
                    Some(resident),
                    None,
                ) => resident.is_canonical_for(admission.paint_grammar),
                (
                    PropertyScrollHostAdmissionKind::AtomicProjectionTextAreaSubtree(admission),
                    None,
                    Some(resident),
                ) => {
                    resident.is_canonical()
                        && resident.content_root == admission.content_wrapper
                        && resident.text_area_root == admission.text_area_root
                        && resident.source_grammar == admission.paint_grammar
                }
                (
                    PropertyScrollHostAdmissionKind::FocusedAtomicProjectionTextAreaSubtree(
                        admission,
                    ),
                    None,
                    Some(resident),
                ) => {
                    resident.is_canonical()
                        && resident.content_root == admission.content_wrapper
                        && resident.text_area_root == admission.text_area_root
                        && resident.source_grammar == admission.paint_grammar.atomic_source
                }
                (
                    PropertyScrollHostAdmissionKind::AtomicProjectionSelectionTextAreaSubtree(_),
                    None,
                    None,
                ) => true,
                _ => false,
            }
    }

    fn bitwise_eq(&self, other: &Self) -> bool {
        self.has_exact_kind_projection()
            && other.has_exact_kind_projection()
            && self.outer_bitwise_eq(other)
            && match (&self.kind, &other.kind) {
                (
                    PropertyScrollHostAdmissionKind::DirectLeaf(left),
                    PropertyScrollHostAdmissionKind::DirectLeaf(right),
                ) => (*left).bitwise_eq(*right),
                (
                    PropertyScrollHostAdmissionKind::TextAreaSubtree(left),
                    PropertyScrollHostAdmissionKind::TextAreaSubtree(right),
                ) => (*left).bitwise_eq(*right),
                (
                    PropertyScrollHostAdmissionKind::InteractiveTextAreaSubtree(left),
                    PropertyScrollHostAdmissionKind::InteractiveTextAreaSubtree(right),
                ) => (*left).bitwise_eq(*right),
                (
                    PropertyScrollHostAdmissionKind::AtomicProjectionTextAreaSubtree(left),
                    PropertyScrollHostAdmissionKind::AtomicProjectionTextAreaSubtree(right),
                ) => left.bitwise_eq(right),
                (
                    PropertyScrollHostAdmissionKind::FocusedAtomicProjectionTextAreaSubtree(left),
                    PropertyScrollHostAdmissionKind::FocusedAtomicProjectionTextAreaSubtree(right),
                ) => left.bitwise_eq(right),
                (
                    PropertyScrollHostAdmissionKind::AtomicProjectionSelectionTextAreaSubtree(left),
                    PropertyScrollHostAdmissionKind::AtomicProjectionSelectionTextAreaSubtree(
                        right,
                    ),
                ) => left.bitwise_eq(right),
                _ => false,
            }
    }
}

#[derive(Clone, Debug)]
struct ScrollScenePlan {
    boundary_root: NodeKey,
    root_stable_id: u64,
    content_root: NodeKey,
    content_stable_id: u64,
    admission: PropertyScrollHostAdmission,
    text_area_subtree_admission: Option<RetainedScrollTextAreaSubtreeAdmissionSnapshot>,
    interactive_text_area_subtree_admission:
        Option<RetainedScrollInteractiveTextAreaSubtreeAdmissionSnapshot>,
    atomic_projection_text_area_subtree_admission:
        Option<RetainedScrollAtomicProjectionTextAreaSubtreeAdmissionSnapshot>,
    focused_atomic_projection_text_area_subtree_admission:
        Option<RetainedScrollFocusedAtomicProjectionTextAreaSubtreeAdmissionSnapshot>,
    post_composite: PropertyScrollPostCompositeSchedule,
    interactive_resident: Option<RetainedInteractiveTextAreaResidentRasterSeal>,
    atomic_projection_resident: Option<RetainedAtomicProjectionTextAreaResidentRasterSeal>,
    scroll: ScrollNodeSnapshot,
    contents_clip: ClipNodeSnapshot,
    planned_admission_witness: PropertyScrollHostAdmission,
    planned_text_area_subtree_admission: Option<RetainedScrollTextAreaSubtreeAdmissionSnapshot>,
    planned_interactive_text_area_subtree_admission:
        Option<RetainedScrollInteractiveTextAreaSubtreeAdmissionSnapshot>,
    planned_atomic_projection_text_area_subtree_admission:
        Option<RetainedScrollAtomicProjectionTextAreaSubtreeAdmissionSnapshot>,
    planned_focused_atomic_projection_text_area_subtree_admission:
        Option<RetainedScrollFocusedAtomicProjectionTextAreaSubtreeAdmissionSnapshot>,
    planned_post_composite: PropertyScrollPostCompositeSchedule,
    planned_interactive_resident: Option<RetainedInteractiveTextAreaResidentRasterSeal>,
    planned_atomic_projection_resident: Option<RetainedAtomicProjectionTextAreaResidentRasterSeal>,
    planned_scroll_witness: ScrollNodeSnapshot,
    planned_clip_witness: ClipNodeSnapshot,
    recorded: ScrollSceneRecordedAuthority,
}

#[derive(Clone, Debug)]
enum ScrollSceneRecordedAuthority {
    Existing {
        host_before: PaintArtifact,
        content_local: PaintArtifact,
        overlay: PaintArtifact,
        host_parent_span: Range<u32>,
        content_local_span: Range<u32>,
        overlay_parent_span: Range<u32>,
    },
    AtomicProjectionTextArea(ValidatedScrollSceneAtomicProjectionTextAreaPlanParts),
    AtomicProjectionSelectionTextArea(
        ValidatedScrollSceneAtomicProjectionSelectionTextAreaPlanParts,
    ),
    FocusedAtomicProjectionTextArea(
        Box<ValidatedScrollSceneFocusedAtomicProjectionTextAreaPlanParts>,
    ),
}

#[cfg(test)]
impl ScrollScenePlan {
    fn existing_recorded_artifacts(&self) -> (&PaintArtifact, &PaintArtifact, &PaintArtifact) {
        let ScrollSceneRecordedAuthority::Existing {
            host_before,
            content_local,
            overlay,
            ..
        } = &self.recorded
        else {
            panic!("existing artifact test helper cannot inspect atomic authority")
        };
        (host_before, content_local, overlay)
    }

    fn existing_recorded_artifacts_mut(
        &mut self,
    ) -> (&mut PaintArtifact, &mut PaintArtifact, &mut PaintArtifact) {
        let ScrollSceneRecordedAuthority::Existing {
            host_before,
            content_local,
            overlay,
            ..
        } = &mut self.recorded
        else {
            panic!("existing artifact test helper cannot mutate atomic authority")
        };
        (host_before, content_local, overlay)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DirectScrollTransformScheduledStep {
    ScrollContents {
        owner: NodeKey,
        scroll: ScrollNodeId,
    },
    TransformContent {
        owner: NodeKey,
        transform: TransformNodeId,
    },
}

/// S1 compiler-owned seal for the only reverse property edge admitted by the
/// first slice: one parentless scroll host followed by its sole direct
/// translation surface.  It is intentionally separate from the generic
/// property-scroll scaffold, which continues to reject S->T.
#[derive(Clone, Debug)]
pub(crate) struct DirectScrollTransformSceneScaffold {
    schedule: [DirectScrollTransformScheduledStep; 2],
    admission: RetainedScrollTransformHostAdmissionSnapshot,
    scroll: ScrollNodeSnapshot,
    contents_clip: ClipNodeSnapshot,
    transform: TransformNodeSnapshot,
    insertion: super::PlannedBoundary,
    host_steps: Vec<super::frame_recorder::RecordedTransformSurfaceStep>,
    content_steps: Vec<super::frame_recorder::RecordedTransformSurfaceStep>,
    host_before_identity: PropertyScrollPhaseArtifactIdentity,
    overlay_after_identity: PropertyScrollPhaseArtifactIdentity,
    content_identity: super::frame_plan::PropertyScrollReceiverArtifactIdentity,
    planned_admission: RetainedScrollTransformHostAdmissionSnapshot,
    planned_scroll: ScrollNodeSnapshot,
    planned_contents_clip: ClipNodeSnapshot,
    planned_transform: TransformNodeSnapshot,
    planned_insertion: super::PlannedBoundary,
    planned_host_before_identity: PropertyScrollPhaseArtifactIdentity,
    planned_overlay_after_identity: PropertyScrollPhaseArtifactIdentity,
    planned_content_identity: super::frame_plan::PropertyScrollReceiverArtifactIdentity,
    scale_factor_bits: u32,
}

impl DirectScrollTransformSceneScaffold {
    pub(crate) fn is_canonical(&self) -> bool {
        let scale_factor = f32::from_bits(self.scale_factor_bits);
        let [
            DirectScrollTransformScheduledStep::ScrollContents { owner, scroll },
            DirectScrollTransformScheduledStep::TransformContent {
                owner: transform_owner,
                transform,
            },
        ] = self.schedule
        else {
            return false;
        };
        let [
            super::frame_recorder::RecordedTransformSurfaceStep::Artifact(host_before),
            super::frame_recorder::RecordedTransformSurfaceStep::Boundary(marker),
            super::frame_recorder::RecordedTransformSurfaceStep::Artifact(overlay_after),
        ] = self.host_steps.as_slice()
        else {
            return false;
        };
        let [super::frame_recorder::RecordedTransformSurfaceStep::Artifact(content)] =
            self.content_steps.as_slice()
        else {
            return false;
        };
        scale_factor.is_finite()
            && scale_factor > 0.0
            && self.admission.bitwise_eq(self.planned_admission)
            && self.scroll == self.planned_scroll
            && self.contents_clip == self.planned_contents_clip
            && self.transform == self.planned_transform
            && self.insertion == self.planned_insertion
            && owner == self.admission.boundary_root
            && scroll == self.scroll.id
            && transform_owner == self.admission.transform_content
            && transform == self.transform.id
            && self.insertion.root == transform_owner
            && self.insertion.stable_id == self.admission.transform_content_stable_id
            && self.insertion.kind == super::PlannedBoundaryKind::Transform(transform)
            && *marker == self.insertion
            && !host_before.chunks.is_empty()
            && !overlay_after.chunks.is_empty()
            && !content.chunks.is_empty()
            && content.clip_nodes.is_empty()
            && content.effect_nodes.is_empty()
            && PropertyScrollPhaseArtifactIdentity::from_artifact(host_before).as_ref()
                == Some(&self.host_before_identity)
            && PropertyScrollPhaseArtifactIdentity::from_artifact(overlay_after).as_ref()
                == Some(&self.overlay_after_identity)
            && super::frame_plan::property_scroll_receiver_artifact_identity(content).as_ref()
                == Some(&self.content_identity)
            && self.host_before_identity == self.planned_host_before_identity
            && self.overlay_after_identity == self.planned_overlay_after_identity
            && self.content_identity == self.planned_content_identity
            && (!matches!(
                self.scroll.scrollbar_overlay.paint_state,
                crate::view::base_component::ScrollbarPaintStateWitness::HiddenNow
                    | crate::view::base_component::ScrollbarPaintStateWitness::NotPaintable
            ) || self.overlay_after_identity.op_count == 0)
            && super::compiler::direct_translation_bits(self.transform.viewport_matrix).is_some()
    }

    #[cfg(test)]
    pub(crate) fn tamper_content_artifact_bounds_for_test(&mut self) {
        if let Some(super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact)) =
            self.content_steps.first_mut()
            && let Some(chunk) = artifact.chunks.first_mut()
        {
            chunk.bounds.x += 1.0;
        }
    }

    #[cfg(test)]
    pub(crate) fn tamper_host_artifact_bounds_for_test(&mut self) {
        if let Some(super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact)) =
            self.host_steps.first_mut()
            && let Some(chunk) = artifact.chunks.first_mut()
        {
            chunk.bounds.y += 1.0;
        }
    }

    #[cfg(test)]
    pub(crate) fn overlay_op_count_for_test(&self) -> usize {
        self.overlay_after_identity.op_count
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DirectScrollTransformSingleBackingPlan {
    color_key: PersistentTextureKey,
    color_desc: TextureDesc,
    depth_desc: TextureDesc,
    pair_bytes: u64,
    source_bounds_bits: [u32; 4],
    max_dimension_2d: u32,
    max_pair_bytes: u64,
}

#[derive(Clone, Debug)]
pub(crate) struct DirectScrollTransformGeometryPlan {
    scaffold: DirectScrollTransformSceneScaffold,
    raster_bounds: RetainedSurfaceBounds,
    geometry: super::PreparedScrollTransformContentCompositeGeometry,
    backing: DirectScrollTransformSingleBackingPlan,
    planned_raster_bounds_bits: [u32; 4],
    planned_geometry: super::PreparedScrollTransformContentCompositeGeometry,
    planned_backing: DirectScrollTransformSingleBackingPlan,
}

impl DirectScrollTransformGeometryPlan {
    pub(crate) fn is_canonical(&self) -> bool {
        self.scaffold.is_canonical()
            && bounds_bits(self.raster_bounds) == self.planned_raster_bounds_bits
            && self.geometry.bitwise_eq(self.planned_geometry)
            && self.backing == self.planned_backing
            && self.geometry.source_bounds_bits() == self.planned_raster_bounds_bits
            && self.geometry.matches_inputs(
                self.scaffold.transform,
                self.scaffold.scroll,
                self.scaffold.contents_clip,
            )
            && self.backing.source_bounds_bits == self.planned_raster_bounds_bits
            && self.backing.color_key
                == crate::view::base_component::transformed_layer_stable_key(
                    self.scaffold.admission.transform_content_stable_id,
                )
    }

    #[cfg(test)]
    pub(crate) fn composite_params(&self) -> crate::view::render_pass::TextureCompositeParams {
        self.geometry.params()
    }

    #[cfg(test)]
    pub(crate) fn raster_bounds(&self) -> RetainedSurfaceBounds {
        self.raster_bounds
    }

    #[cfg(test)]
    pub(crate) fn tamper_geometry_seal_for_test(&mut self) {
        self.planned_raster_bounds_bits[0] ^= 1;
    }

    #[cfg(test)]
    pub(crate) fn tamper_backing_seal_for_test(&mut self) {
        self.backing.pair_bytes = self.backing.pair_bytes.saturating_add(1);
    }
}

/// M12B0 scene-local property-boundary identity.  The ordinal is the
/// authoritative ordering key; `owner` and `kind` make co-located future
/// property boundaries structurally distinct without weakening the older
/// `PropertySurfaceId`/named-canary contracts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct SceneBoundaryId {
    ordinal: u32,
    owner: NodeKey,
    kind: SceneBoundaryKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[allow(dead_code)] // Future kinds are spec-only in B0; construction remains sealed.
enum SceneBoundaryKind {
    Transform,
    Effect,
    ScrollContents,
}

/// The target whose logical coordinate space receives one scroll-content
/// composite.  B0 admits only `FrameRoot`; the other variants reserve the
/// typed contract needed by later transform/effect/nested-scroll slices.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // Only FrameRoot is admitted by the B0 constructor.
enum ScrollCompositeBasis {
    FrameRoot,
    Transform(SceneBoundaryId),
    Effect(SceneBoundaryId),
    ScrollContent(SceneBoundaryId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PropertyScrollPhaseArtifactIdentity {
    owner_topology: Vec<PaintOwnerSnapshot>,
    clip_nodes: Vec<ClipNodeSnapshot>,
    effect_nodes: Vec<crate::view::compositor::property_tree::EffectNodeSnapshot>,
    chunks: Vec<PropertyScrollPhaseChunkIdentity>,
    op_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PropertyScrollPhaseChunkIdentity {
    id: super::PaintChunkId,
    owner: NodeKey,
    bounds_bits: [u32; 4],
    properties: crate::view::compositor::property_tree::PropertyTreeState,
    content_revision: super::PaintContentRevision,
    payload_identity: super::PaintPayloadIdentity,
    op_count: usize,
}

impl PropertyScrollPhaseArtifactIdentity {
    fn from_artifact(artifact: &PaintArtifact) -> Option<Self> {
        let mut cursor = 0usize;
        let mut chunks = Vec::with_capacity(artifact.chunks.len());
        for chunk in &artifact.chunks {
            if chunk.op_range.start != cursor || chunk.op_range.end > artifact.ops.len() {
                return None;
            }
            cursor = chunk.op_range.end;
            chunks.push(PropertyScrollPhaseChunkIdentity {
                id: chunk.id,
                owner: chunk.owner,
                bounds_bits: [
                    chunk.bounds.x.to_bits(),
                    chunk.bounds.y.to_bits(),
                    chunk.bounds.width.to_bits(),
                    chunk.bounds.height.to_bits(),
                ],
                properties: chunk.properties,
                content_revision: chunk.content_revision,
                payload_identity: chunk.payload_identity.clone(),
                op_count: chunk.op_range.len(),
            });
        }
        (cursor == artifact.ops.len()).then(|| Self {
            owner_topology: artifact.owner_nodes.clone(),
            clip_nodes: artifact.clip_nodes.clone(),
            effect_nodes: artifact.effect_nodes.clone(),
            chunks,
            op_count: artifact.ops.len(),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PropertyScrollSemanticFrameWitness {
    sampled_at: crate::time::Instant,
    sampled_alpha_bits: u32,
    paint_state: crate::view::base_component::ScrollbarPaintStateWitness,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PropertyScrollClipSplitWitness {
    local_raster_clips: Vec<ClipNodeSnapshot>,
    own_contents_clip: ClipNodeSnapshot,
    ancestor_composite_clips: Vec<ClipNodeSnapshot>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PropertyScrollCompositeDependency {
    basis: ScrollCompositeBasis,
    source_bounds_bits: [u32; 4],
    offset_bits: [u32; 2],
    contents_clip: [u32; 4],
}

/// Content-only raster identity.  Own scroll offset/generation, contents clip,
/// scrollbar state, sampled alpha, and semantic time are absent by design.
#[derive(Clone, Debug, PartialEq, Eq)]
struct PropertyScrollContentRasterIdentity {
    content_root: NodeKey,
    content_stable_id: u64,
    source_bounds_bits: [u32; 4],
    artifact_span: super::RetainedSurfaceArtifactSpanStamp,
    local_opaque_span: Range<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PropertyScrollBackingBudget {
    max_dimension_2d: u32,
    max_active_pair_bytes: u64,
    tile_edge: u32,
    gutter: u32,
    overscan: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PropertyScrollSingleBackingPlan {
    color_key: PersistentTextureKey,
    color_desc: TextureDesc,
    depth_desc: TextureDesc,
    pair_bytes: u64,
    budget: PropertyScrollBackingBudget,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PropertyScrollTilePlan {
    index: super::ScrollContentTileIndex,
    bounds: super::ScrollContentTileBounds,
    color_key: PersistentTextureKey,
    color_desc: TextureDesc,
    depth_desc: TextureDesc,
    pair_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PropertyScrollTiledBackingPlan {
    content_bounds: [u32; 4],
    tile_edge: u32,
    gutter: u32,
    overscan: u32,
    tiles: Vec<PropertyScrollTilePlan>,
    total_pair_bytes: u64,
    budget: PropertyScrollBackingBudget,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PropertyScrollBackingPlan {
    Single(PropertyScrollSingleBackingPlan),
    Tiled(PropertyScrollTiledBackingPlan),
}

#[derive(Clone, Debug)]
enum ScrollBoundaryStep {
    HostBefore {
        artifact: PaintArtifact,
        identity: PropertyScrollPhaseArtifactIdentity,
        parent_span: Range<u32>,
    },
    ContentComposite {
        boundary: SceneBoundaryId,
        artifact: PaintArtifact,
        content: PropertyScrollContentRasterIdentity,
        composite: PropertyScrollCompositeDependency,
        clip_split: PropertyScrollClipSplitWitness,
        backing: PropertyScrollBackingPlan,
        post_composite: PropertyScrollPostCompositeSchedule,
        parent_before: u32,
        parent_after: u32,
    },
    OverlayAfter {
        artifact: PaintArtifact,
        identity: PropertyScrollPhaseArtifactIdentity,
        parent_span: Range<u32>,
    },
    AtomicProjectionHostBefore {
        authority: Arc<ValidatedScrollSceneAtomicProjectionTextAreaPlanParts>,
        identity: AtomicProjectionTextAreaPlanIdentity,
        parent_span: Range<u32>,
    },
    AtomicProjectionContentComposite {
        boundary: SceneBoundaryId,
        authority: Arc<ValidatedScrollSceneAtomicProjectionTextAreaPlanParts>,
        identity: AtomicProjectionTextAreaPlanIdentity,
        content: PropertyScrollContentRasterIdentity,
        composite: PropertyScrollCompositeDependency,
        clip_split: PropertyScrollClipSplitWitness,
        backing: PropertyScrollBackingPlan,
        post_composite: PropertyScrollPostCompositeSchedule,
        parent_before: u32,
        parent_after: u32,
    },
    AtomicProjectionOverlayAfter {
        authority: Arc<ValidatedScrollSceneAtomicProjectionTextAreaPlanParts>,
        identity: AtomicProjectionTextAreaPlanIdentity,
        parent_span: Range<u32>,
    },
    AtomicProjectionSelectionHostBefore {
        authority: Arc<ValidatedScrollSceneAtomicProjectionSelectionTextAreaPlanParts>,
        identity: AtomicProjectionSelectionTextAreaPlanIdentity,
        parent_span: Range<u32>,
    },
    AtomicProjectionSelectionContentComposite {
        boundary: SceneBoundaryId,
        authority: Arc<ValidatedScrollSceneAtomicProjectionSelectionTextAreaPlanParts>,
        identity: AtomicProjectionSelectionTextAreaPlanIdentity,
        content: PropertyScrollContentRasterIdentity,
        composite: PropertyScrollCompositeDependency,
        clip_split: PropertyScrollClipSplitWitness,
        backing: PropertyScrollBackingPlan,
        post_composite: PropertyScrollPostCompositeSchedule,
        parent_before: u32,
        parent_after: u32,
    },
    AtomicProjectionSelectionOverlayAfter {
        authority: Arc<ValidatedScrollSceneAtomicProjectionSelectionTextAreaPlanParts>,
        identity: AtomicProjectionSelectionTextAreaPlanIdentity,
        parent_span: Range<u32>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PropertySceneJointRootPlanningWitness {
    ordinal: u32,
    root: NodeKey,
    stable_id: u64,
    boundary_span: Range<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PropertySceneGenericResourcePlanningWitness {
    boundary: SceneBoundaryId,
    resident_key: super::RetainedSurfaceResidentKey,
    color_key: PersistentTextureKey,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PropertySceneScrollGroupPlanningWitness {
    boundary: SceneBoundaryId,
    content: PropertyScrollContentRasterIdentity,
    backing: PropertyScrollBackingPlan,
}

/// Future joint transaction shape.  It can own generic resources plus many
/// scroll groups, but the private B0 constructor below accepts exactly one
/// root/scroll group and zero generic resources.  No production transaction
/// capability can be obtained from this planning witness.
#[derive(Clone, Debug, PartialEq, Eq)]
struct PropertySceneJointTransactionPlanningWitness {
    roots: Vec<PropertySceneJointRootPlanningWitness>,
    ordered_boundaries: Vec<SceneBoundaryId>,
    generic_full_set: Vec<PropertySceneGenericResourcePlanningWitness>,
    scroll_groups: Vec<PropertySceneScrollGroupPlanningWitness>,
}

#[derive(Clone, Debug)]
struct PropertyScrollScenePlanSeal {
    scene_root: NodeKey,
    scene_root_stable_id: u64,
    boundary: SceneBoundaryId,
    scroll: ScrollNodeSnapshot,
    contents_clip: ClipNodeSnapshot,
    admission: PropertyScrollHostAdmission,
    text_area_subtree_admission: Option<RetainedScrollTextAreaSubtreeAdmissionSnapshot>,
    interactive_text_area_subtree_admission:
        Option<RetainedScrollInteractiveTextAreaSubtreeAdmissionSnapshot>,
    atomic_projection_text_area_subtree_admission:
        Option<RetainedScrollAtomicProjectionTextAreaSubtreeAdmissionSnapshot>,
    focused_atomic_projection_text_area_subtree_admission:
        Option<RetainedScrollFocusedAtomicProjectionTextAreaSubtreeAdmissionSnapshot>,
    post_composite: PropertyScrollPostCompositeSchedule,
    interactive_resident: Option<RetainedInteractiveTextAreaResidentRasterSeal>,
    atomic_projection_resident: Option<RetainedAtomicProjectionTextAreaResidentRasterSeal>,
    semantic: PropertyScrollSemanticFrameWitness,
    steps_identity: Vec<PropertyScrollStepIdentity>,
    joint_transaction: PropertySceneJointTransactionPlanningWitness,
    planned_scroll: ScrollNodeSnapshot,
    planned_contents_clip: ClipNodeSnapshot,
    planned_admission: PropertyScrollHostAdmission,
    planned_text_area_subtree_admission: Option<RetainedScrollTextAreaSubtreeAdmissionSnapshot>,
    planned_interactive_text_area_subtree_admission:
        Option<RetainedScrollInteractiveTextAreaSubtreeAdmissionSnapshot>,
    planned_atomic_projection_text_area_subtree_admission:
        Option<RetainedScrollAtomicProjectionTextAreaSubtreeAdmissionSnapshot>,
    planned_focused_atomic_projection_text_area_subtree_admission:
        Option<RetainedScrollFocusedAtomicProjectionTextAreaSubtreeAdmissionSnapshot>,
    planned_post_composite: PropertyScrollPostCompositeSchedule,
    planned_interactive_resident: Option<RetainedInteractiveTextAreaResidentRasterSeal>,
    planned_atomic_projection_resident: Option<RetainedAtomicProjectionTextAreaResidentRasterSeal>,
    planned_semantic: PropertyScrollSemanticFrameWitness,
    planned_steps_identity: Vec<PropertyScrollStepIdentity>,
    planned_joint_transaction: PropertySceneJointTransactionPlanningWitness,
    scale_factor_bits: u32,
    target_format: wgpu::TextureFormat,
    budget: PropertyScrollBackingBudget,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PropertyScrollStepIdentity {
    HostBefore {
        identity: PropertyScrollPhaseArtifactIdentity,
        parent_span: Range<u32>,
    },
    ContentComposite {
        boundary: SceneBoundaryId,
        content: PropertyScrollContentRasterIdentity,
        composite: PropertyScrollCompositeDependency,
        clip_split: PropertyScrollClipSplitWitness,
        backing: PropertyScrollBackingPlan,
        post_composite: PropertyScrollPostCompositeSchedule,
        parent_before: u32,
        parent_after: u32,
    },
    OverlayAfter {
        identity: PropertyScrollPhaseArtifactIdentity,
        parent_span: Range<u32>,
    },
    AtomicProjectionHostBefore {
        identity: AtomicProjectionTextAreaPlanIdentity,
        parent_span: Range<u32>,
    },
    AtomicProjectionContentComposite {
        boundary: SceneBoundaryId,
        identity: AtomicProjectionTextAreaPlanIdentity,
        content: PropertyScrollContentRasterIdentity,
        composite: PropertyScrollCompositeDependency,
        clip_split: PropertyScrollClipSplitWitness,
        backing: PropertyScrollBackingPlan,
        post_composite: PropertyScrollPostCompositeSchedule,
        parent_before: u32,
        parent_after: u32,
    },
    AtomicProjectionOverlayAfter {
        identity: AtomicProjectionTextAreaPlanIdentity,
        parent_span: Range<u32>,
    },
    AtomicProjectionSelectionHostBefore {
        identity: AtomicProjectionSelectionTextAreaPlanIdentity,
        parent_span: Range<u32>,
    },
    AtomicProjectionSelectionContentComposite {
        boundary: SceneBoundaryId,
        identity: AtomicProjectionSelectionTextAreaPlanIdentity,
        content: PropertyScrollContentRasterIdentity,
        composite: PropertyScrollCompositeDependency,
        clip_split: PropertyScrollClipSplitWitness,
        backing: PropertyScrollBackingPlan,
        post_composite: PropertyScrollPostCompositeSchedule,
        parent_before: u32,
        parent_after: u32,
    },
    AtomicProjectionSelectionOverlayAfter {
        identity: AtomicProjectionSelectionTextAreaPlanIdentity,
        parent_span: Range<u32>,
    },
}

/// M12B0 planning-only capability.  There is deliberately no executor, pool
/// action, viewport dispatch, or public transaction constructor attached to
/// this type.
#[derive(Clone, Debug)]
pub(crate) struct PropertyScrollScenePlan {
    steps: Vec<ScrollBoundaryStep>,
    seal: PropertyScrollScenePlanSeal,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PropertyScrollScenePlanError {
    LiveSnapshotDrift,
    Frame(FramePaintPlanError),
    InvalidContract,
    BackingBudget,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PropertyScrollSingleCompileStamp {
    content: PropertyScrollContentRasterIdentity,
    color_key: PersistentTextureKey,
    color_desc: TextureDesc,
    depth_desc: TextureDesc,
    pair_bytes: u64,
    budget: PropertyScrollBackingBudget,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PropertyScrollTileCompileStamp {
    content: PropertyScrollContentRasterIdentity,
    index: super::ScrollContentTileIndex,
    bounds: super::ScrollContentTileBounds,
    color_key: PersistentTextureKey,
    color_desc: TextureDesc,
    depth_desc: TextureDesc,
    pair_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PropertyScrollTiledCompileStamp {
    content_bounds: [u32; 4],
    tile_edge: u32,
    gutter: u32,
    overscan: u32,
    tiles: Vec<PropertyScrollTileCompileStamp>,
    total_pair_bytes: u64,
    budget: PropertyScrollBackingBudget,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PropertyScrollContentBackingCompileStamp {
    Single(PropertyScrollSingleCompileStamp),
    Tiled(PropertyScrollTiledCompileStamp),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PropertyScrollContentCompileStamp {
    content: PropertyScrollContentRasterIdentity,
    local_raster_clips: Vec<ClipNodeSnapshot>,
    local_opaque_terminal: u32,
    backing: PropertyScrollContentBackingCompileStamp,
    interactive_resident: Option<RetainedInteractiveTextAreaResidentRasterSeal>,
    atomic_projection_resident: Option<RetainedAtomicProjectionTextAreaResidentRasterSeal>,
}

#[derive(Clone, Debug)]
enum PropertyScrollCompiledStep {
    HostBefore {
        artifact: PaintArtifact,
        dependency: PropertyScrollPhaseArtifactIdentity,
        parent_span: Range<u32>,
    },
    DetachedContent {
        boundary: SceneBoundaryId,
        artifact: PaintArtifact,
        stamp: PropertyScrollContentCompileStamp,
        composite: PropertyScrollCompositeDependency,
        clip_split: PropertyScrollClipSplitWitness,
        post_composite: PropertyScrollPostCompositeSchedule,
        parent_before: u32,
        parent_after: u32,
    },
    OverlayAfter {
        artifact: PaintArtifact,
        dependency: PropertyScrollPhaseArtifactIdentity,
        parent_span: Range<u32>,
    },
    AtomicProjectionHostBefore {
        authority: Arc<ValidatedScrollSceneAtomicProjectionTextAreaPlanParts>,
        dependency: AtomicProjectionTextAreaPlanIdentity,
        parent_span: Range<u32>,
    },
    AtomicProjectionDetachedContent {
        boundary: SceneBoundaryId,
        authority: Arc<ValidatedScrollSceneAtomicProjectionTextAreaPlanParts>,
        dependency: AtomicProjectionTextAreaPlanIdentity,
        stamp: PropertyScrollContentCompileStamp,
        composite: PropertyScrollCompositeDependency,
        clip_split: PropertyScrollClipSplitWitness,
        post_composite: PropertyScrollPostCompositeSchedule,
        parent_before: u32,
        parent_after: u32,
    },
    AtomicProjectionOverlayAfter {
        authority: Arc<ValidatedScrollSceneAtomicProjectionTextAreaPlanParts>,
        dependency: AtomicProjectionTextAreaPlanIdentity,
        parent_span: Range<u32>,
    },
    AtomicProjectionSelectionHostBefore {
        authority: Arc<ValidatedScrollSceneAtomicProjectionSelectionTextAreaPlanParts>,
        dependency: AtomicProjectionSelectionTextAreaPlanIdentity,
        parent_span: Range<u32>,
    },
    AtomicProjectionSelectionDetachedContent {
        boundary: SceneBoundaryId,
        authority: Arc<ValidatedScrollSceneAtomicProjectionSelectionTextAreaPlanParts>,
        dependency: AtomicProjectionSelectionTextAreaPlanIdentity,
        stamp: PropertyScrollContentCompileStamp,
        composite: PropertyScrollCompositeDependency,
        clip_split: PropertyScrollClipSplitWitness,
        post_composite: PropertyScrollPostCompositeSchedule,
        parent_before: u32,
        parent_after: u32,
    },
    AtomicProjectionSelectionOverlayAfter {
        authority: Arc<ValidatedScrollSceneAtomicProjectionSelectionTextAreaPlanParts>,
        dependency: AtomicProjectionSelectionTextAreaPlanIdentity,
        parent_span: Range<u32>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PropertyScrollCompiledStepIdentity {
    HostBefore {
        dependency: PropertyScrollPhaseArtifactIdentity,
        parent_span: Range<u32>,
    },
    DetachedContent {
        boundary: SceneBoundaryId,
        stamp: PropertyScrollContentCompileStamp,
        composite: PropertyScrollCompositeDependency,
        clip_split: PropertyScrollClipSplitWitness,
        post_composite: PropertyScrollPostCompositeSchedule,
        parent_before: u32,
        parent_after: u32,
    },
    OverlayAfter {
        dependency: PropertyScrollPhaseArtifactIdentity,
        parent_span: Range<u32>,
    },
    AtomicProjectionHostBefore {
        dependency: AtomicProjectionTextAreaPlanIdentity,
        parent_span: Range<u32>,
    },
    AtomicProjectionDetachedContent {
        boundary: SceneBoundaryId,
        dependency: AtomicProjectionTextAreaPlanIdentity,
        stamp: PropertyScrollContentCompileStamp,
        composite: PropertyScrollCompositeDependency,
        clip_split: PropertyScrollClipSplitWitness,
        post_composite: PropertyScrollPostCompositeSchedule,
        parent_before: u32,
        parent_after: u32,
    },
    AtomicProjectionOverlayAfter {
        dependency: AtomicProjectionTextAreaPlanIdentity,
        parent_span: Range<u32>,
    },
    AtomicProjectionSelectionHostBefore {
        dependency: AtomicProjectionSelectionTextAreaPlanIdentity,
        parent_span: Range<u32>,
    },
    AtomicProjectionSelectionDetachedContent {
        boundary: SceneBoundaryId,
        dependency: AtomicProjectionSelectionTextAreaPlanIdentity,
        stamp: PropertyScrollContentCompileStamp,
        composite: PropertyScrollCompositeDependency,
        clip_split: PropertyScrollClipSplitWitness,
        post_composite: PropertyScrollPostCompositeSchedule,
        parent_before: u32,
        parent_after: u32,
    },
    AtomicProjectionSelectionOverlayAfter {
        dependency: AtomicProjectionSelectionTextAreaPlanIdentity,
        parent_span: Range<u32>,
    },
}

#[derive(Clone, Debug)]
struct PropertyScrollBoundaryCompilerWitness {
    scene_root: NodeKey,
    scene_root_stable_id: u64,
    boundary: SceneBoundaryId,
    admission: PropertyScrollHostAdmission,
    text_area_subtree_admission: Option<RetainedScrollTextAreaSubtreeAdmissionSnapshot>,
    interactive_text_area_subtree_admission:
        Option<RetainedScrollInteractiveTextAreaSubtreeAdmissionSnapshot>,
    atomic_projection_text_area_subtree_admission:
        Option<RetainedScrollAtomicProjectionTextAreaSubtreeAdmissionSnapshot>,
    focused_atomic_projection_text_area_subtree_admission:
        Option<RetainedScrollFocusedAtomicProjectionTextAreaSubtreeAdmissionSnapshot>,
    post_composite: PropertyScrollPostCompositeSchedule,
    interactive_resident: Option<RetainedInteractiveTextAreaResidentRasterSeal>,
    atomic_projection_resident: Option<RetainedAtomicProjectionTextAreaResidentRasterSeal>,
    scroll: ScrollNodeSnapshot,
    contents_clip: ClipNodeSnapshot,
    semantic: PropertyScrollSemanticFrameWitness,
    target_format: wgpu::TextureFormat,
    budget: PropertyScrollBackingBudget,
    steps: Vec<PropertyScrollCompiledStepIdentity>,
}

impl PartialEq for PropertyScrollBoundaryCompilerWitness {
    fn eq(&self, other: &Self) -> bool {
        self.admission.exactly_corresponds_to_with_atomic(
            self.text_area_subtree_admission,
            self.interactive_text_area_subtree_admission,
            self.atomic_projection_text_area_subtree_admission.as_ref(),
            self.focused_atomic_projection_text_area_subtree_admission
                .as_ref(),
            &self.post_composite,
        ) && other.admission.exactly_corresponds_to_with_atomic(
            other.text_area_subtree_admission,
            other.interactive_text_area_subtree_admission,
            other.atomic_projection_text_area_subtree_admission.as_ref(),
            other
                .focused_atomic_projection_text_area_subtree_admission
                .as_ref(),
            &other.post_composite,
        ) && self.admission.exactly_corresponds_to_resident_with_atomic(
            self.interactive_resident.as_ref(),
            self.atomic_projection_resident.as_ref(),
        ) && other.admission.exactly_corresponds_to_resident_with_atomic(
            other.interactive_resident.as_ref(),
            other.atomic_projection_resident.as_ref(),
        ) && self.scene_root == other.scene_root
            && self.scene_root_stable_id == other.scene_root_stable_id
            && self.boundary == other.boundary
            && self.admission.bitwise_eq(&other.admission)
            && match (
                self.text_area_subtree_admission,
                other.text_area_subtree_admission,
            ) {
                (None, None) => true,
                (Some(left), Some(right)) => left.bitwise_eq(right),
                _ => false,
            }
            && match (
                self.interactive_text_area_subtree_admission,
                other.interactive_text_area_subtree_admission,
            ) {
                (None, None) => true,
                (Some(left), Some(right)) => left.bitwise_eq(right),
                _ => false,
            }
            && match (
                self.atomic_projection_text_area_subtree_admission.as_ref(),
                other.atomic_projection_text_area_subtree_admission.as_ref(),
            ) {
                (None, None) => true,
                (Some(left), Some(right)) => left.bitwise_eq(right),
                _ => false,
            }
            && match (
                self.focused_atomic_projection_text_area_subtree_admission
                    .as_ref(),
                other
                    .focused_atomic_projection_text_area_subtree_admission
                    .as_ref(),
            ) {
                (None, None) => true,
                (Some(left), Some(right)) => left.bitwise_eq(right),
                _ => false,
            }
            && self.post_composite == other.post_composite
            && self.interactive_resident == other.interactive_resident
            && self.atomic_projection_resident == other.atomic_projection_resident
            && self.scroll == other.scroll
            && self.contents_clip == other.contents_clip
            && self.semantic == other.semantic
            && self.target_format == other.target_format
            && self.budget == other.budget
            && self.steps == other.steps
    }
}

impl Eq for PropertyScrollBoundaryCompilerWitness {}

#[derive(Clone, Debug)]
struct PropertyScrollBoundaryCompilerSeal {
    planner: PropertyScrollBoundaryCompilerWitness,
    compiler: PropertyScrollBoundaryCompilerWitness,
}

/// M12B1 compiler-sealed, graph-inert authority for exactly one B0 property
/// scroll boundary.  Private fields and the lack of any emission/pool API keep
/// this token from becoming a production execution capability.
#[derive(Debug)]
#[cfg_attr(test, derive(Clone))]
struct ValidatedPropertyScrollBoundary {
    planner: PropertyScrollScenePlan,
    steps: Vec<PropertyScrollCompiledStep>,
    seal: PropertyScrollBoundaryCompilerSeal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PropertyScrollScenePhase {
    HostBefore,
    DetachedContent,
    OverlayAfter,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PropertyScrollSceneScheduleStamp {
    boundary: SceneBoundaryId,
    phase: PropertyScrollScenePhase,
    parent_span: Range<u32>,
    local_span: Range<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ValidatedPropertyScrollSceneSeal {
    roots: Vec<PropertySceneJointRootPlanningWitness>,
    ordered_boundaries: Vec<SceneBoundaryId>,
    schedule: Vec<PropertyScrollSceneScheduleStamp>,
    semantic_frame_time: crate::time::Instant,
    target_format: wgpu::TextureFormat,
    budget: PropertyScrollBackingBudget,
    aggregate_pair_bytes: u64,
}

/// B4-1 authority for an all-or-nothing forest of exact top-level scroll
/// roots. Each child compiler token remains boundary-local; this outer seal is
/// the only source of global root, boundary and parent-cursor order.
pub(crate) struct ValidatedPropertyScrollScene {
    boundaries: Vec<ValidatedPropertyScrollBoundary>,
    seal: ValidatedPropertyScrollSceneSeal,
}

#[derive(Debug)]
struct ValidatedFrameRootScrollRoot {
    scene_root_ordinal: u32,
    receiver_root: NodeKey,
    receiver_stable_id: u64,
    insertion: super::frame_plan::PropertyFrameScrollReceiverInsertionContract,
    receiver_identity_steps: Vec<super::frame_recorder::RecordedTransformSurfaceStep>,
    receiver_compiler: super::compiler::ValidatedFrameRootScrollReceiver,
    boundary: super::frame_plan::PropertyScrollBoundaryContract,
    content_root: NodeKey,
    content_stable_id: u64,
    text_area_witness: Option<PaintScrollTextAreaSubtreeWitness>,
    content_compiler: super::compiler::ValidatedFrameRootScrollContent,
    required_paint_offset_bits: [u32; 2],
}

#[derive(Debug)]
struct ValidatedFrameRootPlainRoot {
    scene_root_ordinal: u32,
    receiver_root: NodeKey,
    receiver_stable_id: u64,
    receiver_compiler: super::compiler::ValidatedFrameRootScrollReceiver,
}

#[derive(Debug)]
enum ValidatedFrameRootSceneRoot {
    Plain(ValidatedFrameRootPlainRoot),
    Scroll(ValidatedFrameRootScrollRoot),
}

/// Compiler-sealed frame-target authority for one descendant scroll cutout
/// per scene root. Receiver artifacts remain on the frame target; detached
/// content alone is rastered in offset-zero space and composited through the
/// frozen own/ancestor clip split.
pub(crate) struct ValidatedFrameRootScrollScene {
    roots: Vec<ValidatedFrameRootSceneRoot>,
    scale_factor_bits: u32,
    target_format: wgpu::TextureFormat,
}

impl ValidatedFrameRootScrollScene {
    pub(crate) fn is_canonical(&self) -> bool {
        let scale_factor = f32::from_bits(self.scale_factor_bits);
        if self.roots.is_empty() || !scale_factor.is_finite() || scale_factor <= 0.0 {
            return false;
        }
        let mut roots = FxHashSet::default();
        let mut boundaries = FxHashSet::default();
        let mut stable_ids = FxHashSet::default();
        self.roots
            .iter()
            .enumerate()
            .all(|(ordinal, root)| match root {
                ValidatedFrameRootSceneRoot::Plain(root) => {
                    root.scene_root_ordinal == u32::try_from(ordinal).unwrap_or(u32::MAX)
                        && roots.insert(root.receiver_root)
                        && stable_ids.insert(root.receiver_stable_id)
                }
                ValidatedFrameRootSceneRoot::Scroll(root) => {
                    root.scene_root_ordinal == u32::try_from(ordinal).unwrap_or(u32::MAX)
                        && roots.insert(root.receiver_root)
                        && boundaries.insert(root.boundary.scroll.owner)
                        && stable_ids.insert(root.receiver_stable_id)
                        && stable_ids.insert(root.content_stable_id)
                        && root.receiver_root != root.boundary.scroll.owner
                        && root.boundary.scroll.owner != root.content_root
                        && root.insertion.receiver_root == root.receiver_root
                        && root.insertion.receiver_stable_id == root.receiver_stable_id
                        && root.insertion.scroll_boundary_ordinal == root.boundary.ordinal
                        && root
                            .insertion
                            .validates_recorded_steps(&root.receiver_identity_steps)
                        && match root.text_area_witness {
                            Some(witness) => {
                                witness.outer().boundary_root() == root.boundary.scroll.owner
                                    && witness.outer().content_root() == root.content_root
                                    && witness.outer().scroll_snapshot() == root.boundary.scroll
                                    && witness.outer().contents_clip_snapshot()
                                        == root.boundary.contents_clip
                                    && root.boundary.local_content_clips
                                        == [witness.live_contents_clip()]
                                    && super::compiler::frame_root_scroll_content_matches(
                                        &root.content_compiler,
                                        root.content_root,
                                        root.text_area_witness,
                                    )
                            }
                            None => {
                                root.boundary.local_content_clips.is_empty()
                                    && super::compiler::frame_root_scroll_content_matches(
                                        &root.content_compiler,
                                        root.content_root,
                                        None,
                                    )
                            }
                        }
                        && root
                            .required_paint_offset_bits
                            .map(f32::from_bits)
                            .iter()
                            .all(|v| v.is_finite())
                }
            })
    }

    #[cfg(test)]
    pub(crate) fn local_text_area_clip_tampering_is_rejected_for_test(&self) -> bool {
        let mut witnessed = 0usize;
        self.roots.iter().all(|root| match root {
            ValidatedFrameRootSceneRoot::Plain(_) => true,
            ValidatedFrameRootSceneRoot::Scroll(root) => match root.text_area_witness {
                Some(_) => {
                    witnessed += 1;
                    super::compiler::frame_root_scroll_content_local_clip_tampering_is_rejected(
                        &root.content_compiler,
                    )
                }
                None => true,
            },
        }) && witnessed > 0
    }

    #[cfg(test)]
    pub(crate) fn scroll_host_phase_order_and_store_tampering_are_sealed_for_test(&self) -> bool {
        let mut scroll_roots = 0usize;
        self.roots.iter().all(|root| match root {
            ValidatedFrameRootSceneRoot::Plain(_) => true,
            ValidatedFrameRootSceneRoot::Scroll(root) => {
                scroll_roots += 1;
                root.receiver_compiler.has_sealed_scroll_host_phase_order()
                    && root.receiver_compiler.rejects_scroll_host_store_tampering()
            }
        }) && scroll_roots > 0
    }

    #[cfg(test)]
    pub(crate) fn scrollbar_overlay_axis_geometry_for_test(
        &self,
    ) -> Vec<Vec<([u32; 4], [u32; 4])>> {
        self.roots
            .iter()
            .filter_map(|root| match root {
                ValidatedFrameRootSceneRoot::Plain(_) => None,
                ValidatedFrameRootSceneRoot::Scroll(root) => root
                    .receiver_compiler
                    .scrollbar_axis_geometry_bits_for_test(),
            })
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn scrollbar_overlay_tampering_is_rejected_for_test(&self) -> bool {
        let mut painted_scroll_roots = 0usize;
        self.roots.iter().all(|root| match root {
            ValidatedFrameRootSceneRoot::Plain(_) => true,
            ValidatedFrameRootSceneRoot::Scroll(root) => {
                if root
                    .receiver_compiler
                    .scrollbar_axis_geometry_bits_for_test()
                    .is_none()
                {
                    true
                } else {
                    painted_scroll_roots += 1;
                    root.receiver_compiler
                        .rejects_scrollbar_overlay_tampering_for_test()
                }
            }
        }) && painted_scroll_roots > 0
    }

    #[cfg(test)]
    pub(crate) fn receiver_roots_for_test(&self) -> Vec<NodeKey> {
        self.roots
            .iter()
            .map(|root| match root {
                ValidatedFrameRootSceneRoot::Plain(root) => root.receiver_root,
                ValidatedFrameRootSceneRoot::Scroll(root) => root.receiver_root,
            })
            .collect()
    }
}

#[derive(Debug)]
struct ValidatedTransformScrollRoot {
    scene_root_ordinal: u32,
    receiver_root: NodeKey,
    receiver_stable_id: u64,
    receiver: TransformNodeSnapshot,
    geometry: crate::view::base_component::TransformSurfaceGeometrySnapshot,
    insertion: super::frame_plan::PropertyScrollReceiverInsertionContract,
    same_owner_insertion:
        Option<super::frame_plan::PropertySameOwnerTransformScrollReceiverInsertionContract>,
    receiver_steps: Vec<super::frame_recorder::RecordedTransformSurfaceStep>,
    boundary: ValidatedPropertyScrollBoundary,
}

/// Compiler-sealed B4-2B planning authority for an exact translation-only
/// `Transform -> ScrollContents` forest.  It is intentionally graph-inert;
/// prepare must still freeze the joint pool transaction before emission may
/// mutate the frame graph.
pub(crate) struct ValidatedTransformScrollScene {
    roots: Vec<ValidatedTransformScrollRoot>,
    scale_factor_bits: u32,
    semantic_frame_time: crate::time::Instant,
    target_format: wgpu::TextureFormat,
    budget: PropertyScrollBackingBudget,
}

#[derive(Debug)]
#[cfg_attr(test, derive(Clone))]
struct ValidatedEffectScrollRootCheckpoint {
    scene_root_ordinal: u32,
    receiver_root: NodeKey,
    receiver_stable_id: u64,
    receiver: EffectNodeSnapshot,
    insertion: super::frame_plan::PropertyEffectScrollReceiverInsertionContract,
    same_owner_insertion:
        Option<super::frame_plan::PropertySameOwnerEffectScrollReceiverInsertionContract>,
    composite: EffectScrollCompositeWitness,
    receiver_steps: Vec<super::frame_recorder::RecordedTransformSurfaceStep>,
    boundary: ValidatedPropertyScrollBoundary,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EffectScrollCompositeWitness {
    source_bounds_bits: [u32; 4],
    opacity_bits: u32,
    effect_generation: u64,
}

impl EffectScrollCompositeWitness {
    fn new(
        insertion: &super::frame_plan::PropertyEffectScrollReceiverInsertionContract,
        receiver: EffectNodeSnapshot,
    ) -> Option<Self> {
        let witness = Self {
            source_bounds_bits: insertion.raster_bounds_bits,
            opacity_bits: receiver.opacity.to_bits(),
            effect_generation: receiver.generation,
        };
        witness.is_canonical(insertion, receiver).then_some(witness)
    }

    fn is_canonical(
        self,
        insertion: &super::frame_plan::PropertyEffectScrollReceiverInsertionContract,
        receiver: EffectNodeSnapshot,
    ) -> bool {
        self.source_bounds_bits == insertion.raster_bounds_bits && self.matches_receiver(receiver)
    }

    fn matches_receiver(self, receiver: EffectNodeSnapshot) -> bool {
        self.opacity_bits == receiver.opacity.to_bits()
            && self.effect_generation == receiver.generation
            && self.effect_generation != 0
            && f32::from_bits(self.opacity_bits).is_finite()
            && (0.0..=1.0).contains(&f32::from_bits(self.opacity_bits))
            && self
                .source_bounds_bits
                .map(f32::from_bits)
                .iter()
                .all(|value| value.is_finite())
            && f32::from_bits(self.source_bounds_bits[2]) > 0.0
            && f32::from_bits(self.source_bounds_bits[3]) > 0.0
    }
}

/// Graph-inert B4-2C compiler authority. Prepare must still freeze the joint
/// pool transaction before emission may mutate the frame graph.
#[cfg_attr(test, derive(Clone))]
pub(crate) struct ValidatedEffectScrollSceneCheckpoint {
    roots: Vec<ValidatedEffectScrollRootCheckpoint>,
    scale_factor_bits: u32,
    semantic_frame_time: crate::time::Instant,
    target_format: wgpu::TextureFormat,
    budget: PropertyScrollBackingBudget,
}

#[derive(Debug)]
#[cfg_attr(test, derive(Clone))]
struct ValidatedTransformEffectScrollRoot {
    scene_root_ordinal: u32,
    outer_receiver: TransformNodeSnapshot,
    outer_stable_id: u64,
    outer_geometry: crate::view::base_component::TransformSurfaceGeometrySnapshot,
    outer_steps: Vec<super::frame_recorder::RecordedTransformSurfaceStep>,
    insertion: super::frame_plan::PropertyTransformEffectScrollReceiverInsertionContract,
    inner_steps: Vec<super::frame_recorder::RecordedTransformSurfaceStep>,
    composite: EffectScrollCompositeWitness,
    boundary: ValidatedPropertyScrollBoundary,
}

/// Graph-inert compiler authority for the exact T -> E -> Scroll grammar.
/// Both retained targets and the detached content boundary are frozen under
/// one root; no direct T->S or E->S token can be projected from this value.
#[cfg_attr(test, derive(Clone))]
pub(crate) struct ValidatedTransformEffectScrollScene {
    roots: Vec<ValidatedTransformEffectScrollRoot>,
    scale_factor_bits: u32,
    semantic_frame_time: crate::time::Instant,
    target_format: wgpu::TextureFormat,
    budget: PropertyScrollBackingBudget,
}

#[derive(Debug)]
#[cfg_attr(test, derive(Clone))]
struct ValidatedEffectTransformScrollRoot {
    scene_root_ordinal: u32,
    insertion: super::frame_plan::PropertyEffectTransformScrollReceiverInsertionContract,
    outer_steps: Vec<super::frame_recorder::RecordedTransformSurfaceStep>,
    inner_steps: Vec<super::frame_recorder::RecordedTransformSurfaceStep>,
    outer_composite: EffectScrollCompositeWitness,
    boundary: ValidatedPropertyScrollBoundary,
}

/// Graph-inert authority for exact `Effect -> Transform -> ScrollContents`.
/// E remains final-composite authority; T owns the scroll H/C/O raster and
/// its texture is inserted into the E raster exactly once.
#[cfg_attr(test, derive(Clone))]
pub(crate) struct ValidatedEffectTransformScrollScene {
    roots: Vec<ValidatedEffectTransformScrollRoot>,
    scale_factor_bits: u32,
    semantic_frame_time: crate::time::Instant,
    target_format: wgpu::TextureFormat,
    budget: PropertyScrollBackingBudget,
}

#[derive(Debug)]
#[allow(dead_code)] // Consumed by the Phase3 pool freezer in the next cutover slice.
struct ValidatedScrollContentEffectRoot {
    scene_root_ordinal: u32,
    scene_root: NodeKey,
    scene_stable_id: u64,
    outer_steps: Option<Vec<super::frame_recorder::RecordedTransformSurfaceStep>>,
    outer_program: Option<super::compiler::ValidatedFrameRootScrollReceiver>,
    scroll_content_marker: super::PlannedBoundary,
    scroll_host_steps: Vec<super::frame_recorder::RecordedTransformSurfaceStep>,
    scroll_host_program: super::compiler::ValidatedFrameRootScrollReceiver,
    insertion: super::frame_plan::PropertyScrollContentEffectInsertionContract,
    receiver_steps: Vec<super::frame_recorder::RecordedTransformSurfaceStep>,
    receiver_program: super::compiler::ValidatedScrollContentEffectReceiverProgram,
    effect_steps: Vec<super::frame_recorder::RecordedTransformSurfaceStep>,
    effect_normalized_owners: Vec<super::compiler::ScrollContentEffectNormalizedOwnerWitness>,
    boundary: super::frame_plan::PropertyScrollBoundaryContract,
}

/// Graph-inert Phase3 authority for exact `S -> E` or `T -> S -> E`.
/// ScrollContent owns the receiver program; E remains a typed child cutout and
/// the optional outer transform is projected before ScrollContents.
#[allow(dead_code)] // Pool/graph authority is added only after this token is sealed.
pub(crate) struct ValidatedScrollContentEffectScene {
    roots: Vec<ValidatedScrollContentEffectRoot>,
    outer_transform: bool,
    scale_factor_bits: u32,
    semantic_frame_time: crate::time::Instant,
    target_format: wgpu::TextureFormat,
    budget: PropertyScrollBackingBudget,
}

impl ValidatedScrollContentEffectScene {
    pub(crate) fn is_canonical(&self) -> bool {
        let scale_factor = f32::from_bits(self.scale_factor_bits);
        if self.roots.is_empty() || !scale_factor.is_finite() || scale_factor <= 0.0 {
            return false;
        }
        let mut scene_roots = FxHashSet::default();
        let mut boundaries = FxHashSet::default();
        let mut effects = FxHashSet::default();
        self.roots.iter().enumerate().all(|(ordinal, root)| {
            let receiver_marker_count = root
                .receiver_steps
                .iter()
                .filter(|step| {
                    matches!(
                        step,
                        super::frame_recorder::RecordedTransformSurfaceStep::Boundary(marker)
                            if *marker == root.insertion.effect_cutout
                    )
                })
                .count();
            let receiver_artifacts_are_neutral = root.receiver_steps.iter().all(|step| match step {
                super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact) => {
                    matches!(artifact.target, super::PaintArtifactTarget::CurrentTarget)
                        && artifact.chunks.iter().all(|chunk| {
                            chunk.properties == crate::view::compositor::property_tree::PropertyTreeState::default()
                        })
                        && artifact.effect_nodes.is_empty()
                }
                super::frame_recorder::RecordedTransformSurfaceStep::Boundary(marker) => {
                    *marker == root.insertion.effect_cutout
                }
            });
            let effect_artifacts_are_canonical = !root.effect_steps.is_empty()
                && root.effect_steps.iter().all(|step| match step {
                    super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact) => {
                        super::compiler::validate_effect_property_surface_artifact(
                            artifact,
                            &root.insertion.artifact_contract,
                        )
                        .is_some()
                    }
                    super::frame_recorder::RecordedTransformSurfaceStep::Boundary(_) => false,
                });
            let transform_is_canonical = match (
                self.outer_transform,
                root.insertion.outer_transform.as_ref(),
                root.outer_steps.as_ref(),
                root.outer_program.as_ref(),
            ) {
                (false, None, None, None) => root.insertion.consumed_transform.is_none(),
                (true, Some(outer), Some(steps), Some(_)) => {
                    let transform = outer.receiver.receiver;
                    transform.owner == root.scene_root
                        && transform.id.0 == root.scene_root
                        && transform.parent.is_none()
                        && root.insertion.consumed_transform
                            == super::ConsumedAncestorTransformWitness::new(
                                transform.owner,
                                root.boundary.scroll.owner,
                                transform.id,
                            )
                        && outer.receiver.validates_recorded_steps(steps)
                }
                _ => false,
            };
            root.scene_root_ordinal == u32::try_from(ordinal).unwrap_or(u32::MAX)
                && scene_roots.insert(root.scene_root)
                && boundaries.insert(root.boundary.scroll.owner)
                && effects.insert(root.insertion.effect.owner)
                && root.scene_stable_id != 0
                && root.boundary.is_canonical()
                && root.boundary.scene_root_ordinal == root.scene_root_ordinal
                && root.insertion.scene_root_ordinal == root.scene_root_ordinal
                && root.insertion.scroll_boundary_ordinal == root.boundary.ordinal
                && root.insertion.content_root != root.boundary.scroll.owner
                && root.insertion.content_stable_id != 0
                && root.insertion.effect_cutout.root == root.insertion.effect.owner
                && root.insertion.effect_cutout.kind
                    == super::PlannedBoundaryKind::Isolation(root.insertion.effect.id)
                && root.insertion.artifact_contract.boundary_root()
                    == root.insertion.effect.owner
                && root.insertion.artifact_contract.is_canonical()
                && root.scroll_content_marker.root == root.boundary.scroll.owner
                && root.scroll_content_marker.kind
                    == super::PlannedBoundaryKind::Scroll(root.boundary.scroll.id)
                && !root.scroll_host_steps.is_empty()
                && transform_is_canonical
                && receiver_marker_count == 1
                && receiver_artifacts_are_neutral
                && effect_artifacts_are_canonical
        })
    }
}

/// Graph-inert Phase1 authority selected from the typed boundary DAG. Each
/// variant owns the pre-existing exact compiler token; the facade cannot
/// construct a token for a grammar that the Phase0 DAG classifier rejects.
#[allow(dead_code)] // Phase1 authority remains graph-inert until an explicit production cutover.
pub(crate) enum ValidatedPropertyBoundaryDagScene {
    FrameRootScroll(ValidatedFrameRootScrollScene),
    TransformScroll(ValidatedTransformScrollScene),
    EffectScroll(ValidatedEffectScrollSceneCheckpoint),
    TransformEffectScroll(ValidatedTransformEffectScrollScene),
    EffectTransformScroll(ValidatedEffectTransformScrollScene),
    ScrollEffect(ValidatedScrollContentEffectScene),
    TransformScrollEffect(ValidatedScrollContentEffectScene),
}

#[allow(dead_code)]
impl ValidatedPropertyBoundaryDagScene {
    pub(crate) fn is_canonical(&self) -> bool {
        match self {
            Self::FrameRootScroll(scene) => scene.is_canonical(),
            Self::TransformScroll(scene) => scene.is_canonical(),
            Self::EffectScroll(scene) => scene.is_canonical(),
            Self::TransformEffectScroll(scene) => scene.is_canonical(),
            Self::EffectTransformScroll(scene) => scene.is_canonical(),
            Self::ScrollEffect(scene) | Self::TransformScrollEffect(scene) => scene.is_canonical(),
        }
    }
}

/// Phase1 entry point. Classification is DAG-driven, while each leaf compiler
/// remains a compiler-sealed fixed-grammar implementation. This performs no
/// pool access; the Phase3 variants now admit the exact S->E and T->S->E
/// ScrollContent receiver edges without widening any legacy leaf compiler.
#[allow(dead_code)] // Exercised by the Phase1 parity suite before production selection changes.
pub(crate) struct PropertyBoundaryDagCompiler;

#[allow(dead_code)]
impl PropertyBoundaryDagCompiler {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn plan_and_validate(
        arena: &NodeArena,
        roots: &[NodeKey],
        property_trees: &PropertyTrees,
        paint_generations: &PaintGenerationTracker,
        scale_factor: f32,
        incoming_paint_offset: [f32; 2],
        outer_scissor_rect: Option<[u32; 4]>,
        semantic_frame_time: crate::time::Instant,
        target_format: wgpu::TextureFormat,
        budget: ScrollSceneSingleTextureBudget,
    ) -> Result<ValidatedPropertyBoundaryDagScene, PropertyScrollScenePlanError> {
        let context = super::TransformSurfacePlanContext::new(incoming_paint_offset, None);
        let plan = super::frame_plan::plan_property_scroll_interleave_scaffold_with_context(
            arena,
            roots,
            property_trees,
            paint_generations,
            context,
        )
        .map_err(PropertyScrollScenePlanError::Frame)?;
        let grammar = plan
            .property_scroll_planning_scaffold()
            .and_then(|scaffold| scaffold.boundary_dag.existing_grammar())
            .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        match grammar {
            super::frame_plan::PropertyBoundaryDagGrammar::FrameRootScroll => {
                plan_and_validate_frame_root_scroll_scene(
                    arena,
                    roots,
                    property_trees,
                    paint_generations,
                    scale_factor,
                    incoming_paint_offset,
                    outer_scissor_rect,
                    target_format,
                )
                .map(ValidatedPropertyBoundaryDagScene::FrameRootScroll)
            }
            super::frame_plan::PropertyBoundaryDagGrammar::TransformScroll => {
                plan_and_validate_transform_scroll_scene(
                    arena,
                    roots,
                    property_trees,
                    paint_generations,
                    scale_factor,
                    incoming_paint_offset,
                    outer_scissor_rect,
                    semantic_frame_time,
                    target_format,
                    budget,
                )
                .map(ValidatedPropertyBoundaryDagScene::TransformScroll)
            }
            super::frame_plan::PropertyBoundaryDagGrammar::EffectScroll => {
                plan_and_validate_effect_scroll_scene_checkpoint(
                    arena,
                    roots,
                    property_trees,
                    paint_generations,
                    scale_factor,
                    incoming_paint_offset,
                    outer_scissor_rect,
                    semantic_frame_time,
                    target_format,
                    budget,
                )
                .map(ValidatedPropertyBoundaryDagScene::EffectScroll)
            }
            super::frame_plan::PropertyBoundaryDagGrammar::TransformEffectScroll => {
                plan_and_validate_transform_effect_scroll_scene(
                    arena,
                    roots,
                    property_trees,
                    paint_generations,
                    scale_factor,
                    incoming_paint_offset,
                    outer_scissor_rect,
                    semantic_frame_time,
                    target_format,
                    budget,
                )
                .map(ValidatedPropertyBoundaryDagScene::TransformEffectScroll)
            }
            super::frame_plan::PropertyBoundaryDagGrammar::EffectTransformScroll => {
                plan_and_validate_effect_transform_scroll_scene(
                    arena,
                    roots,
                    property_trees,
                    paint_generations,
                    scale_factor,
                    incoming_paint_offset,
                    outer_scissor_rect,
                    semantic_frame_time,
                    target_format,
                    budget,
                )
                .map(ValidatedPropertyBoundaryDagScene::EffectTransformScroll)
            }
            super::frame_plan::PropertyBoundaryDagGrammar::ScrollEffect => {
                plan_and_validate_scroll_content_effect_scene(
                    arena,
                    roots,
                    property_trees,
                    paint_generations,
                    scale_factor,
                    incoming_paint_offset,
                    outer_scissor_rect,
                    semantic_frame_time,
                    target_format,
                    budget,
                    false,
                )
                .map(ValidatedPropertyBoundaryDagScene::ScrollEffect)
            }
            super::frame_plan::PropertyBoundaryDagGrammar::TransformScrollEffect => {
                plan_and_validate_scroll_content_effect_scene(
                    arena,
                    roots,
                    property_trees,
                    paint_generations,
                    scale_factor,
                    incoming_paint_offset,
                    outer_scissor_rect,
                    semantic_frame_time,
                    target_format,
                    budget,
                    true,
                )
                .map(ValidatedPropertyBoundaryDagScene::TransformScrollEffect)
            }
        }
    }
}

fn phase3_normalized_owner_witnesses(
    arena: &NodeArena,
    paint_generations: &PaintGenerationTracker,
    owners: impl IntoIterator<Item = NodeKey>,
) -> Option<Vec<super::compiler::ScrollContentEffectNormalizedOwnerWitness>> {
    let mut seen = FxHashSet::default();
    owners
        .into_iter()
        .filter(|owner| seen.insert(*owner))
        .map(|owner| {
            let node = arena.get(owner)?;
            let stable_id = node.element.stable_id();
            let kind = node
                .element
                .retained_scroll_normalized_paint_capability()?
                .kind();
            let topology_revision = paint_generations
                .local_generations_for(owner)?
                .topology_revision;
            super::compiler::scroll_content_effect_normalized_owner_witness(
                owner,
                stable_id,
                topology_revision,
                kind,
            )
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn plan_and_validate_scroll_content_effect_scene(
    arena: &NodeArena,
    roots: &[NodeKey],
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    scale_factor: f32,
    incoming_paint_offset: [f32; 2],
    outer_scissor_rect: Option<[u32; 4]>,
    semantic_frame_time: crate::time::Instant,
    target_format: wgpu::TextureFormat,
    budget: ScrollSceneSingleTextureBudget,
    outer_transform: bool,
) -> Result<ValidatedScrollContentEffectScene, PropertyScrollScenePlanError> {
    if roots.is_empty()
        || !scale_factor.is_finite()
        || scale_factor <= 0.0
        || incoming_paint_offset.map(f32::to_bits) != [0.0_f32.to_bits(); 2]
        || outer_scissor_rect.is_some()
        || !property_trees.validation_errors.is_empty()
        || !paint_generations.matches_live_snapshot(arena, roots, property_trees)
    {
        return Err(PropertyScrollScenePlanError::InvalidContract);
    }
    let context = super::TransformSurfacePlanContext::new(incoming_paint_offset, None);
    let frame_plan = super::frame_plan::plan_property_scroll_interleave_scaffold_with_context(
        arena,
        roots,
        property_trees,
        paint_generations,
        context,
    )
    .map_err(PropertyScrollScenePlanError::Frame)?;
    let scaffold = frame_plan
        .property_scroll_planning_scaffold()
        .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
    if scaffold.roots.len() != roots.len()
        || scaffold.boundaries.len() != roots.len()
        || scaffold.scroll_content_effect_insertions.len() != roots.len()
        || !scaffold.effect_receiver_insertions.is_empty()
        || !scaffold.transform_effect_receiver_insertions.is_empty()
        || !scaffold.effect_transform_receiver_insertions.is_empty()
    {
        return Err(PropertyScrollScenePlanError::InvalidContract);
    }
    let coverage_error = |fallbacks: Vec<super::FrameArtifactFallbackReason>| {
        PropertyScrollScenePlanError::Frame(FramePaintPlanError {
            reasons: fallbacks
                .into_iter()
                .map(FramePaintPlanRejection::Coverage)
                .collect(),
        })
    };
    let mut validated_roots = Vec::with_capacity(roots.len());
    for root in &scaffold.roots {
        let root_steps = &scaffold.schedule.steps[root.step_span.clone()];
        let (boundary_ordinal, scheduled_transform) = match root_steps {
            [
                super::frame_plan::PropertySceneScheduledStep::ScrollBoundary {
                    boundary_ordinal,
                    basis: super::frame_plan::ScrollCompositeBasis::FrameRoot,
                    ..
                },
                super::frame_plan::PropertySceneScheduledStep::ScrollContentSurface {
                    boundary: super::frame_plan::PropertyScheduledSurfaceBoundary::Effect(_),
                    ..
                },
            ] if !outer_transform => (*boundary_ordinal, None),
            [
                super::frame_plan::PropertySceneScheduledStep::RetainedSurface {
                    boundary:
                        super::frame_plan::PropertyScheduledSurfaceBoundary::Transform(transform),
                    parent: None,
                },
                super::frame_plan::PropertySceneScheduledStep::ScrollBoundary {
                    boundary_ordinal,
                    basis: super::frame_plan::ScrollCompositeBasis::Transform(basis),
                    ..
                },
                super::frame_plan::PropertySceneScheduledStep::ScrollContentSurface {
                    boundary: super::frame_plan::PropertyScheduledSurfaceBoundary::Effect(_),
                    ..
                },
            ] if outer_transform && transform == basis => (*boundary_ordinal, Some(*transform)),
            _ => return Err(PropertyScrollScenePlanError::InvalidContract),
        };
        let boundary = scaffold
            .boundaries
            .get(boundary_ordinal as usize)
            .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        let insertion = scaffold
            .scroll_content_effect_insertions
            .iter()
            .find(|insertion| insertion.scene_root_ordinal == root.ordinal)
            .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        if insertion.scroll_boundary_ordinal != boundary.ordinal
            || scheduled_transform.is_some() != insertion.consumed_transform.is_some()
            || scheduled_transform
                != insertion
                    .outer_transform
                    .as_ref()
                    .map(|outer| outer.receiver.receiver)
        {
            return Err(PropertyScrollScenePlanError::InvalidContract);
        }
        let content_witness = PaintScrollContentWitness::new(
            boundary.scroll.owner,
            insertion.content_root,
            boundary.scroll,
            boundary.contents_clip,
        )
        .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        let receiver_steps =
            super::frame_recorder::record_scroll_content_effect_receiver_steps_for_plan(
                arena,
                property_trees,
                paint_generations,
                content_witness,
                insertion.effect_cutout,
                insertion.consumed_transform,
            )
            .map_err(coverage_error)?;
        let effect_steps =
            super::frame_recorder::record_scroll_content_effect_surface_steps_for_plan(
                arena,
                property_trees,
                paint_generations,
                content_witness,
                &insertion.artifact_contract,
                insertion.consumed_transform,
            )
            .map_err(coverage_error)?;
        let content_normalized_owners = phase3_normalized_owner_witnesses(
            arena,
            paint_generations,
            receiver_steps.iter().flat_map(|step| match step {
                super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact) => artifact
                    .owner_nodes
                    .iter()
                    .map(|owner| owner.owner)
                    .collect::<Vec<_>>(),
                super::frame_recorder::RecordedTransformSurfaceStep::Boundary(_) => Vec::new(),
            }),
        )
        .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        let effect_normalized_owners = phase3_normalized_owner_witnesses(
            arena,
            paint_generations,
            insertion
                .artifact_contract
                .content()
                .iter()
                .map(|content| content.owner),
        )
        .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        let receiver_program = super::compiler::validate_scroll_content_effect_receiver_steps(
            receiver_steps.clone(),
            insertion.content_root,
            insertion.content_stable_id,
            insertion.effect_cutout,
            &insertion.artifact_contract,
            content_normalized_owners,
        )
        .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        let scroll_content_marker = super::PlannedBoundary {
            root: boundary.scroll.owner,
            stable_id: arena
                .get(boundary.scroll.owner)
                .map(|node| node.element.stable_id())
                .filter(|stable_id| *stable_id != 0)
                .ok_or(PropertyScrollScenePlanError::InvalidContract)?,
            kind: super::PlannedBoundaryKind::Scroll(boundary.scroll.id),
        };
        let scroll_host_steps =
            super::frame_recorder::record_frame_root_scroll_host_steps_for_plan(
                arena,
                boundary.scroll.owner,
                insertion.content_root,
                property_trees,
                paint_generations,
                boundary.scroll,
                boundary.contents_clip,
                incoming_paint_offset,
                scroll_content_marker,
                insertion.consumed_transform,
            )
            .map_err(coverage_error)?;
        let scroll_host_program = super::compiler::validate_frame_root_scroll_receiver_steps(
            scroll_host_steps.clone(),
            scroll_content_marker,
            boundary.scroll.owner,
            boundary.scroll,
        )
        .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        let outer_steps = insertion
            .outer_transform
            .as_ref()
            .map(|outer| {
                super::frame_recorder::record_property_scroll_receiver_steps_for_plan(
                    arena,
                    outer.receiver.receiver.owner,
                    property_trees,
                    paint_generations,
                    super::PaintTransformSurfaceWitness::canonical_root(
                        outer.receiver.receiver.owner,
                    ),
                    incoming_paint_offset,
                    outer.receiver.scroll_cutout,
                )
                .map_err(coverage_error)
            })
            .transpose()?;
        if outer_steps
            .as_ref()
            .zip(insertion.outer_transform.as_ref())
            .is_some_and(|(steps, outer)| !outer.receiver.validates_recorded_steps(steps))
        {
            return Err(PropertyScrollScenePlanError::InvalidContract);
        }
        let outer_program = outer_steps
            .as_ref()
            .zip(insertion.outer_transform.as_ref())
            .map(|(steps, outer)| {
                super::compiler::validate_effect_transform_scroll_inner_steps(
                    steps.clone(),
                    outer.receiver.scroll_cutout,
                    outer.receiver.receiver.owner,
                    outer.receiver.receiver.id,
                )
                .ok_or(PropertyScrollScenePlanError::InvalidContract)
            })
            .transpose()?;
        validated_roots.push(ValidatedScrollContentEffectRoot {
            scene_root_ordinal: root.ordinal,
            scene_root: root.root,
            scene_stable_id: root.stable_id,
            outer_steps,
            outer_program,
            scroll_content_marker,
            scroll_host_steps,
            scroll_host_program,
            insertion: insertion.clone(),
            receiver_steps,
            receiver_program,
            effect_steps,
            effect_normalized_owners,
            boundary: boundary.clone(),
        });
    }
    if !paint_generations.matches_live_snapshot(arena, roots, property_trees) {
        return Err(PropertyScrollScenePlanError::LiveSnapshotDrift);
    }
    let scene = ValidatedScrollContentEffectScene {
        roots: validated_roots,
        outer_transform,
        scale_factor_bits: scale_factor.to_bits(),
        semantic_frame_time,
        target_format,
        budget: property_scroll_budget(budget),
    };
    scene
        .is_canonical()
        .then_some(scene)
        .ok_or(PropertyScrollScenePlanError::InvalidContract)
}

#[allow(dead_code)] // Used by the Phase3 pool freezer after the graph-inert checkpoint.
fn freeze_scroll_content_effect_stamp_pair(
    root: &ValidatedScrollContentEffectRoot,
    scale_factor_bits: u32,
    target_format: wgpu::TextureFormat,
) -> Option<(RetainedSurfaceRasterStamp, RetainedSurfaceRasterStamp)> {
    let scale_factor = f32::from_bits(scale_factor_bits);
    if !scale_factor.is_finite() || scale_factor <= 0.0 {
        return None;
    }
    let effect_bounds_values = root.insertion.effect_raster_bounds_bits.map(f32::from_bits);
    let effect_bounds = RetainedSurfaceBounds {
        x: effect_bounds_values[0],
        y: effect_bounds_values[1],
        width: effect_bounds_values[2],
        height: effect_bounds_values[3],
        corner_radii: [0.0; 4],
    };
    let effect_color_key =
        crate::view::base_component::isolation_layer_stable_key(root.insertion.effect_stable_id);
    let effect_color =
        texture_desc_for_logical_bounds(effect_bounds, scale_factor, None, target_format);
    let (effect_color_desc, effect_depth_desc) =
        persistent_target_texture_descriptors(effect_color, effect_color_key);
    let mut effect_cursor = 0_u32;
    let mut effect_stamp_steps = Vec::with_capacity(root.effect_steps.len());
    for (step_index, step) in root.effect_steps.iter().enumerate() {
        let super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact) = step else {
            return None;
        };
        let validated = super::compiler::validate_effect_property_surface_artifact(
            artifact,
            &root.insertion.artifact_contract,
        )?;
        let end =
            effect_cursor.checked_add(checked_property_scroll_opaque_order_count(artifact)?)?;
        let span = super::compiler::validated_effect_property_surface_artifact_span_stamp(
            &validated,
            step_index,
            effect_cursor..end,
        )?;
        effect_stamp_steps.push(super::RetainedSurfaceRasterStepStamp::ArtifactSpan(span));
        effect_cursor = end;
    }
    let effect_stamp = super::compiler::validated_property_effect_surface_raster_stamp(
        &root.insertion.artifact_contract,
        0,
        RetainedSurfaceRasterInputs {
            color: effect_color_desc,
            depth: effect_depth_desc,
            scale_factor_bits,
            source_bounds_bits: root.insertion.effect_raster_bounds_bits,
        },
        effect_stamp_steps,
        0..effect_cursor,
    )?;
    let effect_stamp = super::compiler::normalize_scroll_content_effect_surface_raster_stamp(
        effect_stamp,
        &root.insertion.artifact_contract,
        &root.effect_normalized_owners,
    )?;

    let content_bounds = content_zero_bounds(root.boundary.scroll);
    let content_color_key = scroll_content_layer_stable_key(root.insertion.content_stable_id);
    let content_color =
        texture_desc_for_logical_bounds(content_bounds, scale_factor, None, target_format);
    let (content_color_desc, content_depth_desc) =
        persistent_target_texture_descriptors(content_color, content_color_key);
    let content_stamp = super::compiler::validated_scroll_content_effect_receiver_raster_stamp(
        &root.receiver_program,
        RetainedSurfaceRasterInputs {
            color: content_color_desc,
            depth: content_depth_desc,
            scale_factor_bits,
            source_bounds_bits: bounds_bits(content_bounds),
        },
        effect_stamp.clone(),
    )?;
    Some((effect_stamp, content_stamp))
}

#[derive(Clone)]
struct FrozenScrollContentEffectRoot {
    effect_stamp: RetainedSurfaceRasterStamp,
    content_stamp: RetainedSurfaceRasterStamp,
    outer_stamp: Option<RetainedSurfaceRasterStamp>,
}

fn freeze_scroll_content_effect_transaction(
    scene: &ValidatedScrollContentEffectScene,
) -> Option<(
    RetainedPropertyScrollSceneTransaction,
    Vec<FrozenScrollContentEffectRoot>,
)> {
    if !scene.is_canonical() {
        return None;
    }
    let scale_factor = f32::from_bits(scene.scale_factor_bits);
    let mut frozen = Vec::with_capacity(scene.roots.len());
    let mut joint_roots = Vec::with_capacity(scene.roots.len());
    let mut boundaries = Vec::with_capacity(scene.roots.len());
    let mut generic_bindings = Vec::new();
    let mut groups = Vec::with_capacity(scene.roots.len());
    let mut generic_stamps = Vec::new();
    let mut effect_contracts = Vec::with_capacity(scene.roots.len());
    let mut transform_contracts = Vec::with_capacity(scene.roots.len());
    for root in &scene.roots {
        let (effect_stamp, content_stamp) = freeze_scroll_content_effect_stamp_pair(
            root,
            scene.scale_factor_bits,
            scene.target_format,
        )?;
        let boundary = SceneBoundaryId {
            ordinal: root.boundary.ordinal,
            owner: root.boundary.scroll.owner,
            kind: SceneBoundaryKind::ScrollContents,
        };
        let content_bounds = exact_u32_bounds_from_bits(content_stamp.target.source_bounds_bits)?;
        let group = RetainedPropertyScrollResidentGroup {
            boundary,
            content_root: root.insertion.content_root,
            content_stable_id: root.insertion.content_stable_id,
            signature: RetainedPropertyScrollGroupSignature {
                content_bounds,
                tile_edge: SCROLL_CONTENT_TILE_EDGE,
                gutter: SCROLL_CONTENT_TILE_GUTTER,
                overscan: 0,
                scale_factor_bits: scene.scale_factor_bits,
                color_format: scene.target_format,
            },
            backing: RetainedPropertyScrollResidentBacking::Single(content_stamp.clone()),
        };
        if !group
            .is_scroll_content_effect_canonical(&effect_stamp, &root.insertion.artifact_contract)
        {
            return None;
        }
        let outer_stamp = match (
            root.insertion.outer_transform.as_ref(),
            root.outer_program.as_ref(),
        ) {
            (None, None) => None,
            (Some(outer), Some(outer_program)) => {
                let outer_receiver = &outer.receiver;
                let color_key = crate::view::base_component::transformed_layer_stable_key(
                    outer_receiver.receiver_stable_id,
                );
                let color = texture_desc_for_logical_bounds(
                    outer.geometry.source_bounds,
                    scale_factor,
                    None,
                    scene.target_format,
                );
                let (color, depth) = persistent_target_texture_descriptors(color, color_key);
                let stamp = super::compiler::validated_transform_scroll_content_effect_receiver_program_raster_stamp(
                    outer_program,
                    &root.scroll_host_program,
                    outer,
                    root.scene_root_ordinal,
                    root.boundary.ordinal,
                    root.scroll_content_marker,
                    root.insertion.content_root,
                    root.insertion.content_stable_id,
                    root.boundary.scroll,
                    root.boundary.contents_clip,
                    RetainedSurfaceRasterInputs {
                        color,
                        depth,
                        scale_factor_bits: scene.scale_factor_bits,
                        source_bounds_bits: bounds_bits(outer.geometry.source_bounds),
                    },
                    content_stamp.clone(),
                    &effect_stamp,
                    &root.insertion.artifact_contract,
                )?;
                transform_contracts.push(TransformScrollContentEffectCompilerContract {
                    outer_transform: outer_receiver.receiver.id,
                    outer_geometry: outer.geometry,
                    effect: root.insertion.artifact_contract.clone(),
                });
                Some(stamp)
            }
            _ => return None,
        };

        joint_roots.push(RetainedPropertyScrollJointRootStamp {
            ordinal: root.scene_root_ordinal,
            root: root.scene_root,
            stable_id: root.scene_stable_id,
            boundary_span: root.boundary.ordinal..root.boundary.ordinal.checked_add(1)?,
        });
        boundaries.push(boundary);
        if let Some(outer) = &outer_stamp {
            for stamp in [outer, &effect_stamp] {
                generic_bindings.push(RetainedPropertyScrollGenericBindingStamp {
                    boundary,
                    resident_key: stamp.identity.resident_key(),
                    color_key: stamp.identity.color_key,
                });
                generic_stamps.push(stamp.clone());
            }
        } else {
            generic_bindings.push(RetainedPropertyScrollGenericBindingStamp {
                boundary,
                resident_key: effect_stamp.identity.resident_key(),
                color_key: effect_stamp.identity.color_key,
            });
            generic_stamps.push(effect_stamp.clone());
            effect_contracts.push(root.insertion.artifact_contract.clone());
        }
        groups.push(group);
        frozen.push(FrozenScrollContentEffectRoot {
            effect_stamp,
            content_stamp,
            outer_stamp,
        });
    }
    let authority = if scene.outer_transform {
        if !effect_contracts.is_empty() || transform_contracts.len() != scene.roots.len() {
            return None;
        }
        RetainedPropertyScrollGenericAuthority::TransformScrollContentEffectCompiler(
            transform_contracts,
        )
    } else {
        if !transform_contracts.is_empty() || effect_contracts.len() != scene.roots.len() {
            return None;
        }
        RetainedPropertyScrollGenericAuthority::ScrollContentEffectCompiler(effect_contracts)
    };
    let scroll_bindings = groups
        .iter()
        .map(|group| RetainedPropertyScrollGroupBindingStamp {
            boundary: group.boundary,
            content_root: group.content_root,
            content_stable_id: group.content_stable_id,
            backing_rank: group.backing_rank(),
            ordered_resident_keys: group.active_resident_keys(),
        })
        .collect();
    let transaction = RetainedPropertyScrollSceneTransaction {
        seal: RetainedPropertyScrollJointSeal {
            roots: joint_roots,
            ordered_boundaries: boundaries,
            generic_bindings,
            scroll_bindings,
        },
        generic_authority: authority,
        generic_full_set: generic_stamps,
        scroll_groups: groups,
    };
    transaction.is_canonical().then_some((transaction, frozen))
}

impl ValidatedEffectTransformScrollScene {
    pub(crate) fn is_canonical(&self) -> bool {
        let scale_factor = f32::from_bits(self.scale_factor_bits);
        if self.roots.is_empty() || !scale_factor.is_finite() || scale_factor <= 0.0 {
            return false;
        }
        let mut owners = FxHashSet::default();
        let mut stable_ids = FxHashSet::default();
        self.roots.iter().enumerate().all(|(ordinal, root)| {
            let insertion = &root.insertion;
            let inner = &insertion.inner;
            let scroll = &root.boundary.planner.seal;
            root.scene_root_ordinal == u32::try_from(ordinal).unwrap_or(u32::MAX)
                && insertion.scene_root_ordinal == root.scene_root_ordinal
                && insertion.outer_receiver.owner == insertion.outer_receiver.id.0
                && insertion.outer_receiver.parent.is_none()
                && insertion.outer_receiver.generation != 0
                && insertion.outer_stable_id != 0
                && insertion.outer_artifact_contract.is_canonical()
                && insertion.outer_artifact_contract.isolated_leaf()
                    == insertion.outer_receiver
                && insertion.validates_outer_recorded_steps(&root.outer_steps)
                && inner.scene_root_ordinal == root.scene_root_ordinal
                && inner.receiver.parent.is_none()
                && inner.receiver.generation != 0
                && super::compiler::direct_translation_bits(inner.receiver.viewport_matrix)
                    .is_some()
                && insertion.inner_geometry.matches_rebuilt_contract()
                && insertion
                    .inner_geometry
                    .viewport_transform
                    .to_cols_array()
                    .map(f32::to_bits)
                    == inner.receiver.viewport_matrix.to_cols_array().map(f32::to_bits)
                && inner.validates_recorded_steps(&root.inner_steps)
                && root.outer_composite.source_bounds_bits == insertion.outer_raster_bounds_bits
                && root.outer_composite.matches_receiver(insertion.outer_receiver)
                && inner.scroll_boundary_ordinal as usize == ordinal
                && inner.scroll_cutout.root == scroll.scene_root
                && inner.scroll_cutout.stable_id == scroll.scene_root_stable_id
                && matches!(inner.scroll_cutout.kind, super::PlannedBoundaryKind::Scroll(id) if id == scroll.scroll.id)
                && root.boundary.is_canonical()
                && scroll.semantic.sampled_at == self.semantic_frame_time
                && scroll.target_format == self.target_format
                && scroll.budget == self.budget
                && scroll.scale_factor_bits == self.scale_factor_bits
                && owners.insert(insertion.outer_receiver.owner)
                && owners.insert(inner.receiver.owner)
                && owners.insert(scroll.scene_root)
                && owners.insert(scroll.admission.child)
                && stable_ids.insert(insertion.outer_stable_id)
                && stable_ids.insert(inner.receiver_stable_id)
                && stable_ids.insert(scroll.scene_root_stable_id)
                && stable_ids.insert(scroll.admission.child_stable_id)
        })
    }
}

impl ValidatedTransformEffectScrollScene {
    pub(crate) fn is_canonical(&self) -> bool {
        let scale_factor = f32::from_bits(self.scale_factor_bits);
        if self.roots.is_empty() || !scale_factor.is_finite() || scale_factor <= 0.0 {
            return false;
        }
        let mut transform_owners = FxHashSet::default();
        let mut effect_owners = FxHashSet::default();
        let mut scroll_owners = FxHashSet::default();
        let mut content_owners = FxHashSet::default();
        let mut transform_stable_ids = FxHashSet::default();
        let mut effect_stable_ids = FxHashSet::default();
        let mut scroll_stable_ids = FxHashSet::default();
        let mut content_stable_ids = FxHashSet::default();
        self.roots.iter().enumerate().all(|(ordinal, root)| {
            let inner = &root.insertion.inner;
            let scroll = &root.boundary.planner.seal;
            root.scene_root_ordinal == u32::try_from(ordinal).unwrap_or(u32::MAX)
                && root.outer_receiver.owner == root.outer_receiver.id.0
                && root.outer_receiver.parent.is_none()
                && root.outer_receiver.generation != 0
                && super::compiler::direct_translation_bits(root.outer_receiver.viewport_matrix)
                    .is_some()
                && root.outer_stable_id != 0
                && root.outer_geometry.matches_rebuilt_contract()
                && root
                    .outer_geometry
                    .viewport_transform
                    .to_cols_array()
                    .map(f32::to_bits)
                    == root
                        .outer_receiver
                        .viewport_matrix
                        .to_cols_array()
                        .map(f32::to_bits)
                && root.insertion.scene_root_ordinal == root.scene_root_ordinal
                && root.insertion.outer_receiver == root.outer_receiver
                && root.insertion.outer_stable_id == root.outer_stable_id
                && root
                    .insertion
                    .outer_geometry
                    .bitwise_eq(root.outer_geometry)
                && root.insertion.effect_cutout.root == inner.receiver.owner
                && root.insertion.effect_cutout.stable_id == inner.receiver_stable_id
                && matches!(
                    root.insertion.effect_cutout.kind,
                    super::PlannedBoundaryKind::Isolation(effect) if effect == inner.receiver.id
                )
                && root
                    .insertion
                    .validates_outer_recorded_steps(&root.outer_steps)
                && inner.scene_root_ordinal == root.scene_root_ordinal
                && inner.receiver.parent.is_none()
                && inner.receiver.generation != 0
                && inner.validates_recorded_steps(&root.inner_steps)
                && inner.scroll_cutout.root == scroll.scene_root
                && inner.scroll_cutout.stable_id == scroll.scene_root_stable_id
                && matches!(
                    inner.scroll_cutout.kind,
                    super::PlannedBoundaryKind::Scroll(scroll_id) if scroll_id == scroll.scroll.id
                )
                && root.composite.is_canonical(inner, inner.receiver)
                && inner.scroll_boundary_ordinal as usize == ordinal
                && root.boundary.is_canonical()
                && scroll.semantic.sampled_at == self.semantic_frame_time
                && scroll.target_format == self.target_format
                && scroll.budget == self.budget
                && scroll.scale_factor_bits == self.scale_factor_bits
                && scroll.scene_root != inner.receiver.owner
                && transform_owners.insert(root.outer_receiver.owner)
                && effect_owners.insert(inner.receiver.owner)
                && scroll_owners.insert(scroll.scene_root)
                && content_owners.insert(scroll.admission.child)
                && transform_stable_ids.insert(root.outer_stable_id)
                && effect_stable_ids.insert(inner.receiver_stable_id)
                && scroll_stable_ids.insert(scroll.scene_root_stable_id)
                && content_stable_ids.insert(scroll.admission.child_stable_id)
        })
    }
}

impl ValidatedEffectScrollSceneCheckpoint {
    #[allow(dead_code)]
    pub(crate) fn is_canonical(&self) -> bool {
        let scale_factor = f32::from_bits(self.scale_factor_bits);
        if self.roots.is_empty() || !scale_factor.is_finite() || scale_factor <= 0.0 {
            return false;
        }
        let mut owners = FxHashSet::default();
        let mut stable_ids = FxHashSet::default();
        self.roots.iter().enumerate().all(|(ordinal, root)| {
            let scroll = &root.boundary.planner.seal;
            let same_owner_is_canonical = match &root.same_owner_insertion {
                None => scroll.scene_root != root.receiver_root,
                Some(insertion) => {
                    insertion.is_canonical()
                        && insertion.receiver == root.insertion
                        && insertion.owner == root.receiver_root
                        && insertion.stable_id == root.receiver_stable_id
                        && insertion.effect == root.receiver
                        && insertion.scroll == scroll.scroll
                        && insertion.contents_clip == scroll.contents_clip
                        && insertion.content_root == scroll.admission.child
                        && insertion.content_stable_id == scroll.admission.child_stable_id
                        && scroll.scene_root == root.receiver_root
                        && scroll.scene_root_stable_id == root.receiver_stable_id
                }
            };
            root.scene_root_ordinal == u32::try_from(ordinal).unwrap_or(u32::MAX)
                && root.receiver_root == root.receiver.owner
                && root.receiver.id.0 == root.receiver_root
                && root.receiver.parent.is_none()
                && root.receiver.generation != 0
                && root.receiver.opacity.is_finite()
                && (0.0..=1.0).contains(&root.receiver.opacity)
                && root.receiver_stable_id != 0
                && root.insertion.scene_root_ordinal == root.scene_root_ordinal
                && root.insertion.receiver == root.receiver
                && root.insertion.receiver_stable_id == root.receiver_stable_id
                && root.composite.is_canonical(&root.insertion, root.receiver)
                && root.insertion.scroll_boundary_ordinal as usize == ordinal
                && root.insertion.before_span == (0..root.insertion.insertion_index)
                && root.insertion.after_span
                    == (root.insertion.insertion_index + 1..root.receiver_steps.len())
                && root
                    .insertion
                    .validates_recorded_steps(&root.receiver_steps)
                && root.insertion.raster_bounds_bits
                    == transform_scroll_receiver_raster_bounds(
                        &root.receiver_steps,
                        scroll.admission.source_bounds,
                    )
                    .map(bounds_bits)
                    .unwrap_or([u32::MAX; 4])
                && root.boundary.is_canonical()
                && scroll.semantic.sampled_at == self.semantic_frame_time
                && scroll.target_format == self.target_format
                && scroll.budget == self.budget
                && scroll.scale_factor_bits == self.scale_factor_bits
                && same_owner_is_canonical
                && owners.insert(root.receiver_root)
                && (scroll.scene_root == root.receiver_root || owners.insert(scroll.scene_root))
                && owners.insert(scroll.admission.child)
                && stable_ids.insert(root.receiver_stable_id)
                && (scroll.scene_root_stable_id == root.receiver_stable_id
                    || stable_ids.insert(scroll.scene_root_stable_id))
                && stable_ids.insert(scroll.admission.child_stable_id)
        })
    }
}

impl ValidatedTransformScrollScene {
    pub(crate) fn is_canonical(&self) -> bool {
        let scale_factor = f32::from_bits(self.scale_factor_bits);
        if self.roots.is_empty()
            || !scale_factor.is_finite()
            || scale_factor <= 0.0
            || self.budget.max_dimension_2d == 0
            || self.budget.max_active_pair_bytes == 0
        {
            return false;
        }
        let mut owners = FxHashSet::default();
        let mut stable_ids = FxHashSet::default();
        let mut persistent_keys = FxHashSet::default();
        self.roots.iter().enumerate().all(|(ordinal, root)| {
            let scroll_seal = &root.boundary.planner.seal;
            let same_owner_is_canonical = match &root.same_owner_insertion {
                None => scroll_seal.scene_root != root.receiver_root,
                Some(insertion) => {
                    insertion.is_canonical()
                        && insertion.receiver == root.insertion
                        && insertion.owner == root.receiver_root
                        && insertion.stable_id == root.receiver_stable_id
                        && insertion.transform == root.receiver
                        && insertion.scroll == scroll_seal.scroll
                        && insertion.contents_clip == scroll_seal.contents_clip
                        && insertion.content_root == scroll_seal.admission.child
                        && insertion.content_stable_id == scroll_seal.admission.child_stable_id
                        && scroll_seal.scene_root == root.receiver_root
                        && scroll_seal.scene_root_stable_id == root.receiver_stable_id
                }
            };
            let backing = match root.boundary.planner.steps.get(1) {
                Some(ScrollBoundaryStep::ContentComposite { backing, .. }) => backing,
                _ => return false,
            };
            let keys_are_unique = property_scroll_backing_color_keys(backing).all(|color_key| {
                color_key.depth_stencil().is_some_and(|depth_key| {
                    persistent_keys.insert(color_key) && persistent_keys.insert(depth_key)
                })
            });
            root.scene_root_ordinal == u32::try_from(ordinal).unwrap_or(u32::MAX)
                && root.receiver_root == root.receiver.owner
                && root.receiver.id.0 == root.receiver_root
                && root.receiver_stable_id != 0
                && root.receiver.generation != 0
                && root.receiver.parent.is_none()
                && super::compiler::direct_translation_bits(root.receiver.viewport_matrix).is_some()
                && root
                    .geometry
                    .viewport_transform
                    .to_cols_array()
                    .map(f32::to_bits)
                    == root
                        .receiver
                        .viewport_matrix
                        .to_cols_array()
                        .map(f32::to_bits)
                && root.geometry.outer_scissor_rect.is_none()
                && root.geometry.matches_rebuilt_contract()
                && transform_scroll_receiver_raster_bounds(
                    &root.receiver_steps,
                    scroll_seal.admission.source_bounds,
                )
                .is_some_and(|bounds| {
                    bounds_bits(bounds) == bounds_bits(root.geometry.source_bounds)
                })
                && root.insertion.scene_root_ordinal == root.scene_root_ordinal
                && root.insertion.receiver == root.receiver
                && root.insertion.receiver_stable_id == root.receiver_stable_id
                && root.insertion.scroll_boundary_ordinal as usize == ordinal
                && root.insertion.scroll_cutout.root == scroll_seal.scene_root
                && root.insertion.scroll_cutout.stable_id == scroll_seal.scene_root_stable_id
                && matches!(
                    root.insertion.scroll_cutout.kind,
                    super::PlannedBoundaryKind::Scroll(scroll) if scroll == scroll_seal.scroll.id
                )
                && root.insertion.before_span == (0..root.insertion.insertion_index)
                && root.insertion.after_span
                    == (root.insertion.insertion_index + 1..root.receiver_steps.len())
                && root
                    .insertion
                    .validates_recorded_steps(&root.receiver_steps)
                && root.boundary.is_canonical()
                && scroll_seal.semantic.sampled_at == self.semantic_frame_time
                && scroll_seal.target_format == self.target_format
                && scroll_seal.budget == self.budget
                && scroll_seal.scale_factor_bits == self.scale_factor_bits
                && same_owner_is_canonical
                && owners.insert(root.receiver_root)
                && (scroll_seal.scene_root == root.receiver_root
                    || owners.insert(scroll_seal.scene_root))
                && owners.insert(scroll_seal.admission.child)
                && stable_ids.insert(root.receiver_stable_id)
                && (scroll_seal.scene_root_stable_id == root.receiver_stable_id
                    || stable_ids.insert(scroll_seal.scene_root_stable_id))
                && stable_ids.insert(scroll_seal.admission.child_stable_id)
                && keys_are_unique
        })
    }
}

/// Structural resident contract for one scroll-content group.  The identity
/// (`content_root`, `content_stable_id`) lives on the group; this value freezes
/// the allocation/raster policy whose drift invalidates only that group.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RetainedPropertyScrollGroupSignature {
    content_bounds: [u32; 4],
    tile_edge: u32,
    gutter: u32,
    overscan: u32,
    scale_factor_bits: u32,
    color_format: wgpu::TextureFormat,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum RetainedPropertyScrollResidentBacking {
    Single(RetainedSurfaceRasterStamp),
    Tiled(Vec<RetainedSurfaceRasterStamp>),
}

/// One executor-sealed active scroll group.  Even the single-texture form is
/// kept out of the generic surface full-set so a joint transaction can replace
/// generic residents without evicting scroll residency (and vice versa).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RetainedPropertyScrollResidentGroup {
    boundary: SceneBoundaryId,
    content_root: NodeKey,
    content_stable_id: u64,
    signature: RetainedPropertyScrollGroupSignature,
    backing: RetainedPropertyScrollResidentBacking,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RetainedPropertyScrollJointRootStamp {
    ordinal: u32,
    root: NodeKey,
    stable_id: u64,
    boundary_span: Range<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RetainedPropertyScrollGenericBindingStamp {
    boundary: SceneBoundaryId,
    resident_key: super::RetainedSurfaceResidentKey,
    color_key: PersistentTextureKey,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RetainedPropertyScrollGroupBindingStamp {
    boundary: SceneBoundaryId,
    content_root: NodeKey,
    content_stable_id: u64,
    backing_rank: u8,
    ordered_resident_keys: Vec<super::RetainedSurfaceResidentKey>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RetainedPropertyScrollJointSeal {
    roots: Vec<RetainedPropertyScrollJointRootStamp>,
    ordered_boundaries: Vec<SceneBoundaryId>,
    generic_bindings: Vec<RetainedPropertyScrollGenericBindingStamp>,
    scroll_bindings: Vec<RetainedPropertyScrollGroupBindingStamp>,
}

/// Opaque full-scene resident transaction.  It owns the generic full-set and
/// every active scroll group; staging therefore has exactly one input and
/// cannot be assembled from the older generic/tile staging APIs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RetainedPropertyScrollSceneTransaction {
    seal: RetainedPropertyScrollJointSeal,
    generic_authority: RetainedPropertyScrollGenericAuthority,
    generic_full_set: Vec<RetainedSurfaceRasterStamp>,
    scroll_groups: Vec<RetainedPropertyScrollResidentGroup>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)] // Generic compiler authority is consumed by the next joint-scene admission slice.
enum RetainedPropertyScrollGenericAuthority {
    Empty,
    NativeScrollForest,
    /// The receiver is emitted directly into the frame target. Only detached
    /// scroll-content residents participate in the pool transaction.
    FrameRootReceiver,
    Compiler(super::RetainedPropertySceneTransaction),
    TransformScrollCompiler,
    EffectScrollCompiler(Vec<super::EffectPropertySurfaceArtifactContract>),
    TransformEffectScrollCompiler(Vec<TransformEffectScrollCompilerContract>),
    EffectTransformScrollCompiler(Vec<EffectTransformScrollCompilerContract>),
    ScrollContentEffectCompiler(Vec<super::EffectPropertySurfaceArtifactContract>),
    TransformScrollContentEffectCompiler(Vec<TransformScrollContentEffectCompilerContract>),
    ScrollTransformDirectCompiler(ScrollTransformDirectCompilerContract),
    NestedScrollCompiler(NestedScrollCompilerContract),
    #[cfg(test)]
    CanonicalTest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NestedScrollCompilerPhase {
    OuterHostBefore,
    InnerHostBefore,
    LeafContent,
    InnerOverlayAfter,
    OuterOverlayAfter,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct NestedScrollCompilerStepContract {
    phase: NestedScrollCompilerPhase,
    artifact: PropertyScrollPhaseArtifactIdentity,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct NestedScrollCompilerBoundaryContract {
    boundary: SceneBoundaryId,
    parent: Option<SceneBoundaryId>,
    stable_id: u64,
    scroll: ScrollNodeSnapshot,
    contents_clip: ClipNodeSnapshot,
    source_bounds_bits: [u32; 4],
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct NestedScrollAssemblyBindingContract {
    outer: SceneBoundaryId,
    child: SceneBoundaryId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct NestedScrollResidentBindingContract {
    boundary: SceneBoundaryId,
    content_root: NodeKey,
    content_stable_id: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct NestedScrollCompilerWitness {
    scene_root: NodeKey,
    scene_root_stable_id: u64,
    boundaries: Vec<NestedScrollCompilerBoundaryContract>,
    assembly_binding: NestedScrollAssemblyBindingContract,
    resident_binding: NestedScrollResidentBindingContract,
    steps: Vec<NestedScrollCompilerStepContract>,
    leaf_recorded_bounds_bits: [u32; 4],
    leaf_source_bounds_bits: [u32; 4],
    leaf_artifact_span: super::RetainedSurfaceArtifactSpanStamp,
    leaf_stamp: RetainedSurfaceRasterStamp,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct NestedScrollCompilerContract {
    compiled: NestedScrollCompilerWitness,
    planned: NestedScrollCompilerWitness,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ScrollTransformDirectCompilerContract {
    boundary: SceneBoundaryId,
    scene_root: NodeKey,
    scene_root_stable_id: u64,
    transform: TransformNodeId,
    transform_stable_id: u64,
    source_bounds_bits: [u32; 4],
    artifact_span: super::RetainedSurfaceArtifactSpanStamp,
    planned_scene_root: NodeKey,
    planned_scene_root_stable_id: u64,
    planned_transform: TransformNodeId,
    planned_transform_stable_id: u64,
    planned_source_bounds_bits: [u32; 4],
    planned_artifact_span: super::RetainedSurfaceArtifactSpanStamp,
}

impl ScrollTransformDirectCompilerContract {
    fn validates_stamp(&self, stamp: &RetainedSurfaceRasterStamp) -> bool {
        self.boundary.kind == SceneBoundaryKind::ScrollContents
            && self.scene_root == self.planned_scene_root
            && self.scene_root_stable_id == self.planned_scene_root_stable_id
            && self.transform == self.planned_transform
            && self.transform_stable_id == self.planned_transform_stable_id
            && self.source_bounds_bits == self.planned_source_bounds_bits
            && self.artifact_span == self.planned_artifact_span
            && self.boundary.owner == self.scene_root
            && self.scene_root_stable_id != 0
            && self.transform.0 == stamp.identity.boundary_root
            && self.transform_stable_id == stamp.identity.stable_id
            && self.source_bounds_bits == stamp.target.source_bounds_bits
            && stamp.identity.role == RetainedSurfaceRasterRole::Transform
            && stamp.identity.scroll_content_tile.is_none()
            && stamp.scroll_host.is_none()
            && stamp.property_effect.is_none()
            && stamp.ordered_steps.as_slice()
                == [super::RetainedSurfaceRasterStepStamp::ArtifactSpan(
                    self.artifact_span.clone(),
                )]
            && stamp.opaque_order_span == self.artifact_span.opaque_order_span
            && super::retained_surface_raster_stamp_is_canonical(stamp)
    }
}

impl NestedScrollCompilerContract {
    fn validates_group(&self, group: &RetainedPropertyScrollResidentGroup) -> bool {
        let witness = &self.compiled;
        let [outer, inner] = witness.boundaries.as_slice() else {
            return false;
        };
        let [h0, h1, leaf, o1, o0] = witness.steps.as_slice() else {
            return false;
        };
        let [inner_x, inner_y, inner_width, inner_height] =
            inner.source_bounds_bits.map(f32::from_bits);
        let inner_source = RetainedSurfaceBounds {
            x: inner_x,
            y: inner_y,
            width: inner_width,
            height: inner_height,
            corner_radii: [0.0; 4],
        };
        let expected_local_bounds =
            nested_receiver_local_content_bounds(inner_source, inner.scroll).map(bounds_bits);
        let expected_outer_state = crate::view::compositor::property_tree::PropertyTreeState {
            clip: Some(outer.contents_clip.id),
            scroll: Some(outer.scroll.id),
            ..Default::default()
        };
        let step_is = |step: &NestedScrollCompilerStepContract,
                       phase,
                       owner,
                       properties,
                       clips: &[ClipNodeSnapshot],
                       bounds_bits,
                       chunk_phase,
                       role| {
            let [chunk] = step.artifact.chunks.as_slice() else {
                return false;
            };
            step.phase == phase
                && step.artifact.owner_topology
                    == [PaintOwnerSnapshot {
                        owner,
                        parent: None,
                    }]
                && step.artifact.clip_nodes == clips
                && step.artifact.effect_nodes.is_empty()
                && step.artifact.op_count == chunk.op_count
                && chunk.id.owner == owner
                && chunk.owner == owner
                && chunk.id.scope == super::PaintPropertyScope::SelfPaint
                && chunk.id.phase == chunk_phase
                && chunk.id.slot == 0
                && chunk.id.role == role
                && chunk.properties == properties
                && chunk.bounds_bits == bounds_bits
        };
        self.compiled == self.planned
            && witness.scene_root == outer.boundary.owner
            && witness.scene_root_stable_id == outer.stable_id
            && witness.scene_root_stable_id != 0
            && outer.boundary.ordinal == 0
            && outer.boundary.kind == SceneBoundaryKind::ScrollContents
            && outer.parent.is_none()
            && outer.scroll.id.0 == outer.boundary.owner
            && outer.scroll.owner == outer.boundary.owner
            && outer.scroll.parent.is_none()
            && outer.scroll.generation != 0
            && outer.contents_clip.id.owner == outer.boundary.owner
            && outer.contents_clip.parent.is_none()
            && outer.contents_clip.generation != 0
            && outer
                .scroll
                .has_canonical_vertical_geometry_with_contents_clip(outer.contents_clip)
            && inner.boundary.ordinal == 1
            && inner.boundary.kind == SceneBoundaryKind::ScrollContents
            && inner.parent == Some(outer.boundary)
            && inner.stable_id != 0
            && inner.scroll.id.0 == inner.boundary.owner
            && inner.scroll.owner == inner.boundary.owner
            && inner.scroll.parent == Some(outer.scroll.id)
            && inner.scroll.generation != 0
            && inner.contents_clip.id.owner == inner.boundary.owner
            && inner.contents_clip.parent == Some(outer.contents_clip.id)
            && inner.contents_clip.generation != 0
            && inner
                .scroll
                .has_canonical_nested_vertical_geometry_with_contents_clip(
                    inner.contents_clip,
                    outer.scroll,
                    outer.contents_clip,
                )
            && witness.assembly_binding
                == (NestedScrollAssemblyBindingContract {
                    outer: outer.boundary,
                    child: inner.boundary,
                })
            && witness.resident_binding.boundary == inner.boundary
            && witness.resident_binding.content_root == group.content_root
            && witness.resident_binding.content_stable_id == group.content_stable_id
            && group.boundary == inner.boundary
            && group.ordered_stamps() == [witness.leaf_stamp.clone()]
            && witness.leaf_recorded_bounds_bits == bounds_bits(content_zero_bounds(inner.scroll))
            && expected_local_bounds == Some(witness.leaf_source_bounds_bits)
            && witness.leaf_stamp.target.source_bounds_bits == witness.leaf_source_bounds_bits
            && witness.leaf_artifact_span.clip_nodes.is_empty()
            && matches!(witness.leaf_artifact_span.chunks.as_slice(), [chunk]
                if chunk.bounds_bits == witness.leaf_source_bounds_bits && chunk.clip.is_none())
            && witness.leaf_stamp.ordered_steps.as_slice()
                == [super::RetainedSurfaceRasterStepStamp::ArtifactSpan(
                    witness.leaf_artifact_span.clone(),
                )]
            && super::retained_surface_raster_stamp_is_canonical(&witness.leaf_stamp)
            && step_is(
                h0,
                NestedScrollCompilerPhase::OuterHostBefore,
                outer.boundary.owner,
                Default::default(),
                &[],
                outer.source_bounds_bits,
                super::PaintNodePhase::BeforeChildren,
                super::PaintChunkRole::SelfDecoration,
            )
            && step_is(
                h1,
                NestedScrollCompilerPhase::InnerHostBefore,
                inner.boundary.owner,
                Default::default(),
                &[],
                inner.source_bounds_bits,
                super::PaintNodePhase::BeforeChildren,
                super::PaintChunkRole::SelfDecoration,
            )
            && step_is(
                leaf,
                NestedScrollCompilerPhase::LeafContent,
                witness.resident_binding.content_root,
                expected_outer_state,
                &[outer.contents_clip],
                witness.leaf_recorded_bounds_bits,
                super::PaintNodePhase::BeforeChildren,
                match leaf.artifact.chunks.as_slice() {
                    [chunk]
                        if matches!(
                            chunk.id.role,
                            super::PaintChunkRole::SelfDecoration
                                | super::PaintChunkRole::ImageContent
                                | super::PaintChunkRole::SvgContent
                                | super::PaintChunkRole::TextGlyphs
                        ) =>
                    {
                        chunk.id.role
                    }
                    _ => return false,
                },
            )
            && step_is(
                o1,
                NestedScrollCompilerPhase::InnerOverlayAfter,
                inner.boundary.owner,
                Default::default(),
                &[],
                inner.source_bounds_bits,
                super::PaintNodePhase::AfterChildren,
                super::PaintChunkRole::ScrollbarOverlay,
            )
            && step_is(
                o0,
                NestedScrollCompilerPhase::OuterOverlayAfter,
                outer.boundary.owner,
                Default::default(),
                &[],
                outer.source_bounds_bits,
                super::PaintNodePhase::AfterChildren,
                super::PaintChunkRole::ScrollbarOverlay,
            )
    }
}

#[allow(dead_code)] // Consumed through the validated token before prepare lands.
fn nested_scroll_compiler_witness_matches_scaffold(
    witness: &NestedScrollCompilerWitness,
    scaffold: &super::frame_plan::NestedScrollSceneScaffold,
) -> bool {
    let [outer, inner] = scaffold.boundaries.as_slice() else {
        return false;
    };
    let [
        super::frame_plan::NestedScrollSceneScheduledStep::HostBefore { artifact: h0, .. },
        super::frame_plan::NestedScrollSceneScheduledStep::HostBefore { artifact: h1, .. },
        super::frame_plan::NestedScrollSceneScheduledStep::ContentReceiver(receiver),
        super::frame_plan::NestedScrollSceneScheduledStep::OverlayAfter { artifact: o1, .. },
        super::frame_plan::NestedScrollSceneScheduledStep::OverlayAfter { artifact: o0, .. },
    ] = scaffold.schedule.steps.as_slice()
    else {
        return false;
    };
    let make_step = |phase, artifact: &PaintArtifact| {
        Some(NestedScrollCompilerStepContract {
            phase,
            artifact: PropertyScrollPhaseArtifactIdentity::from_artifact(artifact)?,
        })
    };
    let expected_steps = [
        make_step(NestedScrollCompilerPhase::OuterHostBefore, h0.artifact()),
        make_step(NestedScrollCompilerPhase::InnerHostBefore, h1.artifact()),
        make_step(
            NestedScrollCompilerPhase::LeafContent,
            receiver.artifact.artifact(),
        ),
        make_step(NestedScrollCompilerPhase::InnerOverlayAfter, o1.artifact()),
        make_step(NestedScrollCompilerPhase::OuterOverlayAfter, o0.artifact()),
    ]
    .into_iter()
    .collect::<Option<Vec<_>>>();
    let admission = scaffold.admission;
    let [compiled_outer, compiled_inner] = witness.boundaries.as_slice() else {
        return false;
    };
    witness.scene_root == admission.outer_boundary_root
        && witness.scene_root_stable_id == admission.outer_stable_id
        && compiled_outer.boundary.owner == admission.outer_boundary_root
        && compiled_outer.stable_id == admission.outer_stable_id
        && compiled_outer.scroll == outer.scroll
        && compiled_outer.contents_clip == outer.contents_clip
        && compiled_outer.source_bounds_bits == bounds_bits(admission.outer_source_bounds)
        && compiled_inner.boundary.owner == admission.inner_boundary_root
        && compiled_inner.stable_id == admission.inner_stable_id
        && compiled_inner.scroll == inner.scroll
        && compiled_inner.contents_clip == inner.contents_clip
        && compiled_inner.source_bounds_bits == bounds_bits(admission.inner_source_bounds)
        && witness.resident_binding.content_root == admission.content_leaf
        && witness.resident_binding.content_stable_id == admission.content_leaf_stable_id
        && expected_steps.as_ref() == Some(&witness.steps)
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TransformEffectScrollCompilerContract {
    outer_transform: TransformNodeId,
    child: super::EffectPropertySurfaceArtifactContract,
}

#[derive(Clone, Debug)]
struct EffectTransformScrollCompilerContract {
    outer: super::EffectPropertySurfaceArtifactContract,
    child_transform: TransformNodeId,
    child_geometry: crate::view::base_component::TransformSurfaceGeometrySnapshot,
}

#[derive(Clone, Debug)]
struct TransformScrollContentEffectCompilerContract {
    outer_transform: TransformNodeId,
    outer_geometry: crate::view::base_component::TransformSurfaceGeometrySnapshot,
    effect: super::EffectPropertySurfaceArtifactContract,
}

impl PartialEq for TransformScrollContentEffectCompilerContract {
    fn eq(&self, other: &Self) -> bool {
        self.outer_transform == other.outer_transform
            && self.outer_geometry.bitwise_eq(other.outer_geometry)
            && self.effect == other.effect
    }
}

impl Eq for TransformScrollContentEffectCompilerContract {}

impl PartialEq for EffectTransformScrollCompilerContract {
    fn eq(&self, other: &Self) -> bool {
        self.outer == other.outer
            && self.child_transform == other.child_transform
            && self.child_geometry.bitwise_eq(other.child_geometry)
    }
}

impl Eq for EffectTransformScrollCompilerContract {}

impl RetainedPropertyScrollResidentGroup {
    pub(crate) fn content_root(&self) -> NodeKey {
        self.content_root
    }

    pub(crate) fn content_stable_id(&self) -> u64 {
        self.content_stable_id
    }

    pub(crate) fn signature(&self) -> &RetainedPropertyScrollGroupSignature {
        &self.signature
    }

    pub(crate) fn backing_rank(&self) -> u8 {
        match self.backing {
            RetainedPropertyScrollResidentBacking::Single(_) => 0,
            RetainedPropertyScrollResidentBacking::Tiled(_) => 1,
        }
    }

    pub(crate) fn ordered_stamps(&self) -> &[RetainedSurfaceRasterStamp] {
        match &self.backing {
            RetainedPropertyScrollResidentBacking::Single(stamp) => std::slice::from_ref(stamp),
            RetainedPropertyScrollResidentBacking::Tiled(stamps) => stamps,
        }
    }

    pub(crate) fn active_resident_keys(&self) -> Vec<super::RetainedSurfaceResidentKey> {
        self.ordered_stamps()
            .iter()
            .map(|stamp| stamp.identity.resident_key())
            .collect()
    }

    fn is_canonical(&self) -> bool {
        self.is_canonical_with(|stamp| super::retained_surface_raster_stamp_is_canonical(stamp))
    }

    fn is_native_scroll_forest_canonical(&self) -> bool {
        self.is_canonical_with(|stamp| {
            super::compiler::native_scroll_forest_content_raster_stamp_is_canonical(stamp)
        })
    }

    fn is_scroll_content_effect_canonical(
        &self,
        effect_stamp: &RetainedSurfaceRasterStamp,
        effect_contract: &super::EffectPropertySurfaceArtifactContract,
    ) -> bool {
        self.is_canonical_with(|stamp| {
            super::compiler::scroll_content_effect_receiver_raster_stamp_validates_contract(
                stamp,
                self.content_root,
                self.content_stable_id,
                effect_contract,
            ) && stamp
                .ordered_steps
                .iter()
                .filter_map(|step| match step {
                    super::RetainedSurfaceRasterStepStamp::ScrollContentEffectChild(dependency) => {
                        Some(dependency.child_stamp.as_ref())
                    }
                    _ => None,
                })
                .eq(std::iter::once(effect_stamp))
        })
    }

    fn is_canonical_with(
        &self,
        mut stamp_is_canonical: impl FnMut(&RetainedSurfaceRasterStamp) -> bool,
    ) -> bool {
        let scale_factor = f32::from_bits(self.signature.scale_factor_bits);
        if self.content_root.is_null()
            || self.content_stable_id == 0
            || self.boundary.kind != SceneBoundaryKind::ScrollContents
            || !scale_factor.is_finite()
            || scale_factor <= 0.0
            || self.signature.tile_edge == 0
            || self.signature.gutter != 1
        {
            return false;
        }
        let stamps = self.ordered_stamps();
        if stamps.is_empty() {
            return false;
        }
        let mut resident_keys = FxHashSet::default();
        let mut color_keys = FxHashSet::default();
        let common = stamps.iter().all(|stamp| {
            stamp.identity.role == RetainedSurfaceRasterRole::ScrollContent
                && stamp.identity.boundary_root == self.content_root
                && stamp.identity.stable_id == self.content_stable_id
                && stamp.target.scale_factor_bits == self.signature.scale_factor_bits
                && stamp.target.color.format() == self.signature.color_format
                && stamp
                    .target
                    .source_bounds_bits
                    .iter()
                    .all(|bits| f32::from_bits(*bits).is_finite())
                && stamp
                    .target
                    .has_canonical_descriptor_pair_for(stamp.identity)
                && stamp_is_canonical(stamp)
                && resident_keys.insert(stamp.identity.resident_key())
                && color_keys.insert(stamp.identity.color_key)
        });
        if !common {
            return false;
        }
        match &self.backing {
            RetainedPropertyScrollResidentBacking::Single(stamp) => {
                stamp.identity.scroll_content_tile.is_none()
                    && stamp.identity.color_key
                        == scroll_content_layer_stable_key(self.content_stable_id)
                    && stamp.target.source_bounds_bits
                        == self
                            .signature
                            .content_bounds
                            .map(|value| (value as f32).to_bits())
            }
            RetainedPropertyScrollResidentBacking::Tiled(stamps) => {
                let mut previous = None;
                stamps.iter().all(|stamp| {
                    let Some(tile) = stamp.identity.scroll_content_tile else {
                        return false;
                    };
                    let ordered = previous.is_none_or(|previous| previous < tile.index);
                    previous = Some(tile.index);
                    ordered
                        && tile.content_bounds == self.signature.content_bounds
                        && tile.tile_edge == self.signature.tile_edge
                        && tile.gutter == self.signature.gutter
                        && tile.is_canonical()
                })
            }
        }
    }
}

impl RetainedPropertyScrollSceneTransaction {
    pub(crate) fn is_canonical(&self) -> bool {
        let is_direct_scroll_transform = matches!(
            self.generic_authority,
            RetainedPropertyScrollGenericAuthority::ScrollTransformDirectCompiler(_)
        );
        if self.seal.roots.is_empty()
            || self.seal.ordered_boundaries.is_empty()
            || (!is_direct_scroll_transform && self.scroll_groups.is_empty())
            || self.seal.generic_bindings.len() != self.generic_full_set.len()
            || self.seal.scroll_bindings.len() != self.scroll_groups.len()
        {
            return false;
        }
        match &self.generic_authority {
            RetainedPropertyScrollGenericAuthority::Empty => {
                if !self.generic_full_set.is_empty() {
                    return false;
                }
            }
            RetainedPropertyScrollGenericAuthority::NativeScrollForest => {
                if !self.generic_full_set.is_empty()
                    || self.scroll_groups.is_empty()
                    || self.scroll_groups.iter().any(|group| {
                        !group.is_native_scroll_forest_canonical()
                    })
                {
                    return false;
                }
            }
            RetainedPropertyScrollGenericAuthority::FrameRootReceiver => {
                if !self.generic_full_set.is_empty() {
                    return false;
                }
            }
            RetainedPropertyScrollGenericAuthority::Compiler(authority) => {
                if self.generic_full_set.is_empty()
                    || !authority.is_canonical()
                    || !authority.validates_surface_stamps(&self.generic_full_set)
                {
                    return false;
                }
            }
            RetainedPropertyScrollGenericAuthority::TransformScrollCompiler => {
                if self.generic_full_set.is_empty()
                    || self.generic_full_set.iter().any(|stamp| {
                        !super::compiler::transform_scroll_receiver_raster_stamp_is_canonical(stamp)
                    })
                {
                    return false;
                }
            }
            RetainedPropertyScrollGenericAuthority::EffectScrollCompiler(contracts) => {
                if contracts.len() != self.generic_full_set.len()
                    || contracts.is_empty()
                    || self
                        .generic_full_set
                        .iter()
                        .zip(contracts)
                        .any(|(stamp, contract)| {
                            !super::compiler::effect_scroll_receiver_raster_stamp_validates_contract(
                                stamp, contract,
                            )
                        })
                {
                    return false;
                }
            }
            RetainedPropertyScrollGenericAuthority::TransformEffectScrollCompiler(contracts) => {
                if contracts.is_empty()
                    || self.generic_full_set.len() != contracts.len().saturating_mul(2)
                    || self
                        .generic_full_set
                        .chunks_exact(2)
                        .zip(contracts)
                        .any(|(pair, contract)| {
                            !super::compiler::effect_scroll_receiver_raster_stamp_validates_contract(
                                &pair[1],
                                &contract.child,
                            ) || !super::compiler::transform_effect_scroll_outer_raster_stamp_validates_contract(
                                &pair[0],
                                contract.outer_transform,
                                &contract.child,
                            ) || !pair[0].ordered_steps.iter().any(|step| {
                                matches!(
                                    step,
                                    super::RetainedSurfaceRasterStepStamp::TransformEffectScrollChild(dependency)
                                        if dependency.child_stamp.as_ref() == &pair[1]
                                )
                            })
                        })
                {
                    return false;
                }
            }
            RetainedPropertyScrollGenericAuthority::EffectTransformScrollCompiler(contracts) => {
                if contracts.is_empty()
                    || self.generic_full_set.len() != contracts.len().saturating_mul(2)
                    || self
                        .generic_full_set
                        .chunks_exact(2)
                        .zip(contracts)
                        .any(|(pair, contract)| {
                            !super::compiler::transform_scroll_receiver_raster_stamp_is_canonical(
                                &pair[1],
                            ) || !super::compiler::effect_transform_scroll_outer_raster_stamp_validates_contract(
                                &pair[0],
                                &contract.outer,
                                contract.child_transform,
                                contract.child_geometry,
                            ) || !pair[0].ordered_steps.iter().any(|step| {
                                matches!(
                                    step,
                                    super::RetainedSurfaceRasterStepStamp::EffectTransformScrollChild(dependency)
                                        if dependency.child_stamp.as_ref() == &pair[1]
                                )
                            })
                        })
                {
                    return false;
                }
            }
            RetainedPropertyScrollGenericAuthority::ScrollContentEffectCompiler(contracts) => {
                if contracts.is_empty()
                    || contracts.len() != self.generic_full_set.len()
                    || contracts.len() != self.scroll_groups.len()
                    || contracts.len() != self.seal.roots.len()
                    || self
                        .generic_full_set
                        .iter()
                        .zip(contracts)
                        .any(|(effect, contract)| {
                            !super::compiler::scroll_content_effect_surface_raster_stamp_validates_contract(
                                effect,
                                contract,
                            )
                        })
                {
                    return false;
                }
            }
            RetainedPropertyScrollGenericAuthority::TransformScrollContentEffectCompiler(
                contracts,
            ) => {
                if contracts.is_empty()
                    || self.generic_full_set.len() != contracts.len().saturating_mul(2)
                    || self.scroll_groups.len() != contracts.len()
                    || self.seal.roots.len() != contracts.len()
                    || self
                        .generic_full_set
                        .chunks_exact(2)
                        .zip(contracts)
                        .any(|(pair, contract)| {
                            !super::compiler::transform_scroll_content_effect_receiver_raster_stamp_validates_contract(
                                &pair[0],
                                &pair[1],
                                &contract.effect,
                            ) || !super::compiler::scroll_content_effect_surface_raster_stamp_validates_contract(
                                &pair[1],
                                &contract.effect,
                            ) || pair[0].identity.boundary_root != contract.outer_transform.0
                                || pair[0].target.source_bounds_bits
                                    != bounds_bits(contract.outer_geometry.source_bounds)
                                || !contract.outer_geometry.matches_rebuilt_contract()
                        })
                {
                    return false;
                }
            }
            RetainedPropertyScrollGenericAuthority::ScrollTransformDirectCompiler(contract) => {
                let [stamp] = self.generic_full_set.as_slice() else {
                    return false;
                };
                if !self.scroll_groups.is_empty()
                    || !self.seal.scroll_bindings.is_empty()
                    || !contract.validates_stamp(stamp)
                {
                    return false;
                }
            }
            RetainedPropertyScrollGenericAuthority::NestedScrollCompiler(contract) => {
                let [group] = self.scroll_groups.as_slice() else {
                    return false;
                };
                if !self.generic_full_set.is_empty() || !contract.validates_group(group) {
                    return false;
                }
            }
            #[cfg(test)]
            RetainedPropertyScrollGenericAuthority::CanonicalTest => {
                if self.generic_full_set.is_empty()
                    || self.generic_full_set.iter().any(|stamp| {
                        stamp.identity.scroll_content_tile.is_some()
                            || stamp.identity.role == RetainedSurfaceRasterRole::ScrollContent
                            || !super::retained_surface_raster_stamp_is_canonical(stamp)
                    })
                {
                    return false;
                }
            }
        }
        let mut boundary_cursor = 0_u32;
        for (ordinal, root) in self.seal.roots.iter().enumerate() {
            let Ok(ordinal) = u32::try_from(ordinal) else {
                return false;
            };
            if root.ordinal != ordinal
                || root.root.is_null()
                || root.stable_id == 0
                || root.boundary_span.start != boundary_cursor
                || root.boundary_span.end < root.boundary_span.start
            {
                return false;
            }
            boundary_cursor = root.boundary_span.end;
        }
        if usize::try_from(boundary_cursor).ok() != Some(self.seal.ordered_boundaries.len()) {
            return false;
        }
        for root in &self.seal.roots {
            let (Ok(start), Ok(end)) = (
                usize::try_from(root.boundary_span.start),
                usize::try_from(root.boundary_span.end),
            ) else {
                return false;
            };
            for boundary in &self.seal.ordered_boundaries[start..end] {
                let canonical_owner =
                    if let RetainedPropertyScrollGenericAuthority::NestedScrollCompiler(contract) =
                        &self.generic_authority
                    {
                        contract.compiled.scene_root == root.root
                            && contract
                                .compiled
                                .boundaries
                                .iter()
                                .any(|candidate| candidate.boundary == *boundary)
                    } else if matches!(
                        &self.generic_authority,
                        RetainedPropertyScrollGenericAuthority::TransformEffectScrollCompiler(_)
                    ) {
                        self.generic_full_set.iter().any(|stamp| {
                            stamp.identity.boundary_root == root.root
                                && stamp.identity.stable_id == root.stable_id
                                && stamp.ordered_steps.iter().any(|step| {
                                    matches!(
                                        step,
                                        super::RetainedSurfaceRasterStepStamp::TransformEffectScrollChild(dependency)
                                            if dependency.child_stamp.ordered_steps.iter().any(|child_step| {
                                                matches!(
                                                    child_step,
                                                    super::RetainedSurfaceRasterStepStamp::EffectScrollBoundary(scroll)
                                                        if scroll.boundary_root == boundary.owner
                                                            && scroll.scene_root_ordinal == root.ordinal
                                                )
                                            })
                                    )
                                })
                        })
                    } else if matches!(
                        &self.generic_authority,
                        RetainedPropertyScrollGenericAuthority::EffectTransformScrollCompiler(_)
                    ) {
                        self.generic_full_set.iter().any(|stamp| {
                            stamp.identity.boundary_root == root.root
                                && stamp.identity.stable_id == root.stable_id
                                && stamp.ordered_steps.iter().any(|step| {
                                    matches!(
                                        step,
                                        super::RetainedSurfaceRasterStepStamp::EffectTransformScrollChild(dependency)
                                            if dependency.child_stamp.ordered_steps.iter().any(|child_step| {
                                                matches!(
                                                    child_step,
                                                    super::RetainedSurfaceRasterStepStamp::ScrollBoundary(scroll)
                                                        if scroll.boundary_root == boundary.owner
                                                            && scroll.scene_root_ordinal == root.ordinal
                                                )
                                            })
                                    )
                                })
                        })
                    } else if matches!(
                        &self.generic_authority,
                        RetainedPropertyScrollGenericAuthority::TransformScrollCompiler
                            | RetainedPropertyScrollGenericAuthority::EffectScrollCompiler(_)
                    ) {
                        self.seal
                            .generic_bindings
                            .iter()
                            .zip(&self.generic_full_set)
                            .find(|(binding, _)| binding.boundary == *boundary)
                            .is_some_and(|(_, stamp)| {
                                stamp.identity.boundary_root == root.root
                                    && stamp.identity.stable_id == root.stable_id
                                    && stamp.ordered_steps.iter().any(|step| {
                                        match step {
                                    super::RetainedSurfaceRasterStepStamp::ScrollBoundary(
                                        dependency,
                                    ) => {
                                        dependency.boundary_root == boundary.owner
                                            && dependency.scene_root_ordinal == root.ordinal
                                    }
                                    super::RetainedSurfaceRasterStepStamp::EffectScrollBoundary(
                                        dependency,
                                    ) => {
                                        dependency.boundary_root == boundary.owner
                                            && dependency.scene_root_ordinal == root.ordinal
                                    }
                                    _ => false,
                                }
                                    })
                            })
                    } else if matches!(
                        &self.generic_authority,
                        RetainedPropertyScrollGenericAuthority::TransformScrollContentEffectCompiler(_)
                    ) {
                        self.generic_full_set.chunks_exact(2).any(|pair| {
                            pair[0].identity.boundary_root == root.root
                                && pair[0].identity.stable_id == root.stable_id
                                && pair[0].ordered_steps.iter().any(|step| {
                                    matches!(
                                        step,
                                        super::RetainedSurfaceRasterStepStamp::ScrollBoundary(dependency)
                                            if dependency.boundary_root == boundary.owner
                                                && dependency.scroll_boundary_ordinal == boundary.ordinal
                                                && dependency.scene_root_ordinal == root.ordinal
                                    )
                                })
                        })
                    } else if matches!(
                        &self.generic_authority,
                        RetainedPropertyScrollGenericAuthority::FrameRootReceiver
                            | RetainedPropertyScrollGenericAuthority::NativeScrollForest
                    ) {
                        self.scroll_groups
                            .iter()
                            .any(|group| group.boundary == *boundary)
                    } else {
                        boundary.owner == root.root
                    };
                if !canonical_owner {
                    return false;
                }
            }
        }
        let mut boundary_set = FxHashSet::default();
        if self
            .seal
            .ordered_boundaries
            .iter()
            .enumerate()
            .any(|(ordinal, boundary)| {
                u32::try_from(ordinal).ok() != Some(boundary.ordinal)
                    || boundary.owner.is_null()
                    || !boundary_set.insert(*boundary)
            })
        {
            return false;
        }
        let generic_ok = self
            .seal
            .generic_bindings
            .iter()
            .zip(&self.generic_full_set)
            .all(|(binding, stamp)| {
                boundary_set.contains(&binding.boundary)
                    && binding.resident_key == stamp.identity.resident_key()
                    && binding.color_key == stamp.identity.color_key
                    && stamp.identity.scroll_content_tile.is_none()
                    && stamp.identity.role != RetainedSurfaceRasterRole::ScrollContent
                    && stamp
                        .target
                        .has_canonical_descriptor_pair_for(stamp.identity)
            });
        if !generic_ok {
            return false;
        }
        let mut scroll_binding_boundaries = FxHashSet::default();
        let groups_ok = self
            .seal
            .scroll_bindings
            .iter()
            .zip(&self.scroll_groups)
            .enumerate()
            .all(|(index, (binding, group))| {
                let group_is_canonical = match &self.generic_authority {
                    RetainedPropertyScrollGenericAuthority::ScrollContentEffectCompiler(
                        contracts,
                    ) => self
                        .generic_full_set
                        .get(index)
                        .zip(contracts.get(index))
                        .is_some_and(|(effect, contract)| {
                            group.is_scroll_content_effect_canonical(effect, contract)
                        }),
                    RetainedPropertyScrollGenericAuthority::TransformScrollContentEffectCompiler(
                        contracts,
                    ) => self
                        .generic_full_set
                        .get(index.saturating_mul(2).saturating_add(1))
                        .zip(contracts.get(index))
                        .is_some_and(|(effect, contract)| {
                            group.is_scroll_content_effect_canonical(effect, &contract.effect)
                        }),
                    RetainedPropertyScrollGenericAuthority::NativeScrollForest => {
                        group.is_native_scroll_forest_canonical()
                    }
                    _ => group.is_canonical(),
                };
                boundary_set.contains(&binding.boundary)
                    && scroll_binding_boundaries.insert(binding.boundary)
                    && binding.boundary == group.boundary
                    && binding.content_root == group.content_root
                    && binding.content_stable_id == group.content_stable_id
                    && binding.backing_rank == group.backing_rank()
                    && binding.ordered_resident_keys == group.active_resident_keys()
                    && group_is_canonical
            });
        if !groups_ok {
            return false;
        }
        if let RetainedPropertyScrollGenericAuthority::ScrollTransformDirectCompiler(contract) =
            &self.generic_authority
        {
            let ([root], [boundary], [binding], [stamp]) = (
                self.seal.roots.as_slice(),
                self.seal.ordered_boundaries.as_slice(),
                self.seal.generic_bindings.as_slice(),
                self.generic_full_set.as_slice(),
            ) else {
                return false;
            };
            if root.ordinal != 0
                || root.boundary_span != (0..1)
                || root.root != contract.scene_root
                || root.stable_id != contract.scene_root_stable_id
                || *boundary != contract.boundary
                || boundary.ordinal != 0
                || boundary.owner != root.root
                || boundary.kind != SceneBoundaryKind::ScrollContents
                || binding.boundary != *boundary
                || binding.resident_key != stamp.identity.resident_key()
                || binding.color_key != stamp.identity.color_key
                || contract.transform.0 != stamp.identity.boundary_root
                || contract.transform_stable_id != stamp.identity.stable_id
                || contract.source_bounds_bits != stamp.target.source_bounds_bits
            {
                return false;
            }
        }
        if let RetainedPropertyScrollGenericAuthority::NestedScrollCompiler(contract) =
            &self.generic_authority
        {
            let ([root], [outer, inner], [], [binding], [group]) = (
                self.seal.roots.as_slice(),
                self.seal.ordered_boundaries.as_slice(),
                self.seal.generic_bindings.as_slice(),
                self.seal.scroll_bindings.as_slice(),
                self.scroll_groups.as_slice(),
            ) else {
                return false;
            };
            let [contract_outer, contract_inner] = contract.compiled.boundaries.as_slice() else {
                return false;
            };
            if root.ordinal != 0
                || root.boundary_span != (0..2)
                || root.root != contract.compiled.scene_root
                || root.stable_id != contract.compiled.scene_root_stable_id
                || *outer != contract_outer.boundary
                || *inner != contract_inner.boundary
                || contract_outer.parent.is_some()
                || contract_inner.parent != Some(*outer)
                || contract.compiled.assembly_binding.outer != *outer
                || contract.compiled.assembly_binding.child != *inner
                || contract.compiled.resident_binding.boundary != *inner
                || binding.boundary != *inner
                || binding.content_root != contract.compiled.resident_binding.content_root
                || binding.content_stable_id != contract.compiled.resident_binding.content_stable_id
                || group.boundary != *inner
                || group.content_root != binding.content_root
                || group.content_stable_id != binding.content_stable_id
            {
                return false;
            }
        }
        if matches!(
            &self.generic_authority,
            RetainedPropertyScrollGenericAuthority::Empty
                | RetainedPropertyScrollGenericAuthority::FrameRootReceiver
        ) && (self.seal.scroll_bindings.len() != self.seal.ordered_boundaries.len()
            || scroll_binding_boundaries != boundary_set)
        {
            return false;
        }
        if matches!(
            &self.generic_authority,
            RetainedPropertyScrollGenericAuthority::TransformScrollCompiler
                | RetainedPropertyScrollGenericAuthority::EffectScrollCompiler(_)
        ) {
            if self.generic_full_set.len() != self.scroll_groups.len()
                || self.generic_full_set.len() != self.seal.roots.len()
            {
                return false;
            }
            let mut bound_groups = FxHashSet::default();
            for (binding, receiver) in self
                .seal
                .generic_bindings
                .iter()
                .zip(&self.generic_full_set)
            {
                let dependencies = receiver
                    .ordered_steps
                    .iter()
                    .filter_map(|step| match step {
                        super::RetainedSurfaceRasterStepStamp::ScrollBoundary(dependency) => {
                            Some((
                                dependency.boundary_root,
                                dependency.scroll_boundary_ordinal,
                                dependency.content_root,
                                dependency.content_stable_id,
                                dependency.content_stamps.as_slice(),
                            ))
                        }
                        super::RetainedSurfaceRasterStepStamp::EffectScrollBoundary(dependency) => {
                            Some((
                                dependency.boundary_root,
                                dependency.scroll_boundary_ordinal,
                                dependency.content_root,
                                dependency.content_stable_id,
                                dependency.content_stamps.as_slice(),
                            ))
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                let [dependency] = dependencies.as_slice() else {
                    return false;
                };
                let Some((group_index, group)) = self
                    .scroll_groups
                    .iter()
                    .enumerate()
                    .find(|(_, group)| group.boundary == binding.boundary)
                else {
                    return false;
                };
                if !bound_groups.insert(group_index)
                    || dependency.0 != binding.boundary.owner
                    || dependency.1 != binding.boundary.ordinal
                    || dependency.2 != group.content_root
                    || dependency.3 != group.content_stable_id
                    || dependency.4 != group.ordered_stamps()
                {
                    return false;
                }
            }
            if bound_groups.len() != self.scroll_groups.len() {
                return false;
            }
        }
        if matches!(
            &self.generic_authority,
            RetainedPropertyScrollGenericAuthority::TransformEffectScrollCompiler(_)
        ) {
            if self.generic_full_set.len() != self.seal.roots.len().saturating_mul(2)
                || self.scroll_groups.len() != self.seal.roots.len()
            {
                return false;
            }
            let mut bound_groups = FxHashSet::default();
            for pair in self.generic_full_set.chunks_exact(2) {
                let outer = &pair[0];
                let inner = &pair[1];
                let Some(outer_binding) = self
                    .seal
                    .generic_bindings
                    .iter()
                    .find(|binding| binding.resident_key == outer.identity.resident_key())
                else {
                    return false;
                };
                let Some(inner_binding) = self
                    .seal
                    .generic_bindings
                    .iter()
                    .find(|binding| binding.resident_key == inner.identity.resident_key())
                else {
                    return false;
                };
                if outer_binding.boundary != inner_binding.boundary {
                    return false;
                }
                let dependencies = inner
                    .ordered_steps
                    .iter()
                    .filter_map(|step| match step {
                        super::RetainedSurfaceRasterStepStamp::EffectScrollBoundary(dependency) => {
                            Some(dependency)
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                let [dependency] = dependencies.as_slice() else {
                    return false;
                };
                let Some((group_index, group)) = self
                    .scroll_groups
                    .iter()
                    .enumerate()
                    .find(|(_, group)| group.boundary == inner_binding.boundary)
                else {
                    return false;
                };
                if !bound_groups.insert(group_index)
                    || dependency.boundary_root != inner_binding.boundary.owner
                    || dependency.scroll_boundary_ordinal != inner_binding.boundary.ordinal
                    || dependency.content_root != group.content_root
                    || dependency.content_stable_id != group.content_stable_id
                    || dependency.content_stamps != group.ordered_stamps()
                {
                    return false;
                }
            }
            if bound_groups.len() != self.scroll_groups.len() {
                return false;
            }
        }
        if matches!(
            &self.generic_authority,
            RetainedPropertyScrollGenericAuthority::EffectTransformScrollCompiler(_)
        ) {
            if self.generic_full_set.len() != self.seal.roots.len().saturating_mul(2)
                || self.scroll_groups.len() != self.seal.roots.len()
            {
                return false;
            }
            let mut bound_groups = FxHashSet::default();
            for pair in self.generic_full_set.chunks_exact(2) {
                let outer = &pair[0];
                let inner = &pair[1];
                let Some(outer_binding) = self
                    .seal
                    .generic_bindings
                    .iter()
                    .find(|binding| binding.resident_key == outer.identity.resident_key())
                else {
                    return false;
                };
                let Some(inner_binding) = self
                    .seal
                    .generic_bindings
                    .iter()
                    .find(|binding| binding.resident_key == inner.identity.resident_key())
                else {
                    return false;
                };
                if outer_binding.boundary != inner_binding.boundary {
                    return false;
                }
                let dependencies = inner
                    .ordered_steps
                    .iter()
                    .filter_map(|step| match step {
                        super::RetainedSurfaceRasterStepStamp::ScrollBoundary(dependency) => {
                            Some(dependency)
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                let [dependency] = dependencies.as_slice() else {
                    return false;
                };
                let Some((group_index, group)) = self
                    .scroll_groups
                    .iter()
                    .enumerate()
                    .find(|(_, group)| group.boundary == inner_binding.boundary)
                else {
                    return false;
                };
                if !bound_groups.insert(group_index)
                    || dependency.boundary_root != inner_binding.boundary.owner
                    || dependency.scroll_boundary_ordinal != inner_binding.boundary.ordinal
                    || dependency.content_root != group.content_root
                    || dependency.content_stable_id != group.content_stable_id
                    || dependency.content_stamps != group.ordered_stamps()
                {
                    return false;
                }
            }
            if bound_groups.len() != self.scroll_groups.len() {
                return false;
            }
        }
        let mut residents = FxHashSet::default();
        let mut gpu_keys = FxHashSet::default();
        self.ordered_stamps().into_iter().all(|stamp| {
            let Some(depth_key) = stamp.identity.color_key.depth_stencil() else {
                return false;
            };
            residents.insert(stamp.identity.resident_key())
                && gpu_keys.insert(stamp.identity.color_key)
                && gpu_keys.insert(depth_key)
        })
    }

    pub(crate) fn generic_stamps(&self) -> &[RetainedSurfaceRasterStamp] {
        &self.generic_full_set
    }

    pub(crate) fn scroll_groups(&self) -> &[RetainedPropertyScrollResidentGroup] {
        &self.scroll_groups
    }

    pub(crate) fn ordered_stamps(&self) -> Vec<&RetainedSurfaceRasterStamp> {
        self.generic_full_set
            .iter()
            .chain(
                self.scroll_groups
                    .iter()
                    .flat_map(RetainedPropertyScrollResidentGroup::ordered_stamps),
            )
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn new_for_pool_test(
        generic_full_set: Vec<RetainedSurfaceRasterStamp>,
        scroll_stamp_groups: Vec<Vec<RetainedSurfaceRasterStamp>>,
    ) -> Option<Self> {
        if scroll_stamp_groups.is_empty() {
            return None;
        }
        let mut roots = Vec::with_capacity(scroll_stamp_groups.len());
        let mut ordered_boundaries = Vec::with_capacity(scroll_stamp_groups.len());
        let mut scroll_groups = Vec::with_capacity(scroll_stamp_groups.len());
        for (ordinal, stamps) in scroll_stamp_groups.into_iter().enumerate() {
            let ordinal = u32::try_from(ordinal).ok()?;
            let first = stamps.first()?;
            let content_root = first.identity.boundary_root;
            let content_stable_id = first.identity.stable_id;
            let boundary = SceneBoundaryId {
                ordinal,
                owner: content_root,
                kind: SceneBoundaryKind::ScrollContents,
            };
            let signature = if stamps.len() == 1 && first.identity.scroll_content_tile.is_none() {
                RetainedPropertyScrollGroupSignature {
                    content_bounds: exact_u32_bounds_from_bits(first.target.source_bounds_bits)?,
                    tile_edge: SCROLL_CONTENT_TILE_EDGE,
                    gutter: SCROLL_CONTENT_TILE_GUTTER,
                    overscan: 0,
                    scale_factor_bits: first.target.scale_factor_bits,
                    color_format: first.target.color.format(),
                }
            } else {
                let tile = first.identity.scroll_content_tile?;
                if stamps.iter().any(|stamp| {
                    stamp.identity.scroll_content_tile.is_none_or(|candidate| {
                        candidate.content_bounds != tile.content_bounds
                            || candidate.tile_edge != tile.tile_edge
                            || candidate.gutter != tile.gutter
                    })
                }) {
                    return None;
                }
                RetainedPropertyScrollGroupSignature {
                    content_bounds: tile.content_bounds,
                    tile_edge: tile.tile_edge,
                    gutter: tile.gutter,
                    overscan: 0,
                    scale_factor_bits: first.target.scale_factor_bits,
                    color_format: first.target.color.format(),
                }
            };
            let backing = if stamps.len() == 1 && first.identity.scroll_content_tile.is_none() {
                RetainedPropertyScrollResidentBacking::Single(first.clone())
            } else {
                RetainedPropertyScrollResidentBacking::Tiled(stamps)
            };
            roots.push(RetainedPropertyScrollJointRootStamp {
                ordinal,
                root: content_root,
                stable_id: content_stable_id,
                boundary_span: ordinal..ordinal.checked_add(1)?,
            });
            ordered_boundaries.push(boundary);
            scroll_groups.push(RetainedPropertyScrollResidentGroup {
                boundary,
                content_root,
                content_stable_id,
                signature,
                backing,
            });
        }
        let generic_boundary = *ordered_boundaries.first()?;
        let generic_bindings = generic_full_set
            .iter()
            .map(|stamp| RetainedPropertyScrollGenericBindingStamp {
                boundary: generic_boundary,
                resident_key: stamp.identity.resident_key(),
                color_key: stamp.identity.color_key,
            })
            .collect();
        let scroll_bindings = scroll_groups
            .iter()
            .map(|group| RetainedPropertyScrollGroupBindingStamp {
                boundary: group.boundary,
                content_root: group.content_root,
                content_stable_id: group.content_stable_id,
                backing_rank: group.backing_rank(),
                ordered_resident_keys: group.active_resident_keys(),
            })
            .collect();
        let transaction = Self {
            seal: RetainedPropertyScrollJointSeal {
                roots,
                ordered_boundaries,
                generic_bindings,
                scroll_bindings,
            },
            generic_authority: if generic_full_set.is_empty() {
                RetainedPropertyScrollGenericAuthority::Empty
            } else {
                RetainedPropertyScrollGenericAuthority::CanonicalTest
            },
            generic_full_set,
            scroll_groups,
        };
        transaction.is_canonical().then_some(transaction)
    }

    #[cfg(test)]
    pub(crate) fn invalid_for_pool_test(&self) -> Self {
        let mut invalid = self.clone();
        if let Some(binding) = invalid.seal.scroll_bindings.first_mut() {
            binding.backing_rank ^= 1;
        }
        invalid
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RetainedPropertyScrollScenePrepareError {
    BoundaryDrift,
    ContextMismatch,
    ParentTarget,
    PersistentKeyAlreadyDeclared(PersistentTextureKey),
    DescriptorPair,
    GeometryContract,
    PoolContract,
    StageUnavailable,
}

#[derive(Clone, Debug)]
struct PreparedRetainedPropertyScrollTile {
    stamp: RetainedSurfaceRasterStamp,
    color_key: PersistentTextureKey,
    color_desc: TextureDesc,
    geometry: super::PreparedScrollContentTileCompositeGeometry,
}

#[derive(Clone, Debug)]
enum PreparedRetainedPropertyScrollBacking {
    Single {
        stamp: RetainedSurfaceRasterStamp,
        color_key: PersistentTextureKey,
        color_desc: TextureDesc,
        geometry: PreparedScrollContentCompositeGeometry,
        pair_bytes: u64,
    },
    Tiled {
        tiles: Vec<PreparedRetainedPropertyScrollTile>,
        total_pair_bytes: u64,
    },
}

/// Executor-owned pre-clear capability.  All descriptors, resident actions,
/// geometry, ordered artifacts, transaction stamps and trace data are frozen
/// before this value can exist; emission below is deliberately infallible.
#[cfg(test)]
pub(crate) struct PreparedRetainedPropertyScrollScene<'a> {
    /// Exclusive leases are stronger than replaying an epoch stamp: while the
    /// token exists, neither the resident pool nor the graph can change and a
    /// different viewport/graph cannot be substituted at emission time.
    viewport: &'a mut Viewport,
    graph: &'a mut FrameGraph,
    parent_ctx: UiBuildContext,
    host_before: ValidatedScrollSceneHostBeforeArtifact,
    content: ValidatedScrollSceneContentArtifact,
    overlay: ValidatedScrollSceneOverlayArtifact,
    backing: PreparedRetainedPropertyScrollBacking,
    actions: FxHashMap<super::RetainedSurfaceResidentKey, RetainedSurfaceCompileAction>,
    transaction: RetainedPropertyScrollSceneTransaction,
    host_parent_terminal: u32,
    content_local_terminal: u32,
    parent_terminal: u32,
    trace: RetainedPropertyScrollSceneBuildTrace,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PropertyScrollRootTargetPolicy {
    ContextRootTarget,
}

/// B4-1 prepared authority. Clear, root-target policy, frame owner, all
/// boundary artifacts, all resource actions and the one joint transaction are
/// sealed before graph mutation begins.
pub(crate) struct PreparedRetainedPropertyScrollForest<'a> {
    viewport: &'a mut Viewport,
    graph: &'a mut FrameGraph,
    parent_ctx: UiBuildContext,
    boundaries: Vec<PreparedRetainedPropertyScrollBoundaryParts>,
    actions: FxHashMap<super::RetainedSurfaceResidentKey, RetainedSurfaceCompileAction>,
    transaction: RetainedPropertyScrollSceneTransaction,
    schedule: Vec<PropertyScrollSceneScheduleStamp>,
    clear_rgba_bits: [u32; 4],
    target_policy: PropertyScrollRootTargetPolicy,
    frame_owner: RetainedSurfaceFrameStageOwner,
    budget: PropertyScrollBackingBudget,
    aggregate_pair_bytes: u64,
    parent_terminal: u32,
    trace: RetainedPropertyScrollSceneBuildTrace,
}

/// Graph-inert native S-forest pool transaction. No target declaration,
/// graph pass, or resident staging occurs before the complete forest and its
/// post-order action closure have been validated.
pub(crate) struct PreparedNativeScrollForestTransaction {
    transaction: RetainedPropertyScrollSceneTransaction,
    stamps: Vec<RetainedSurfaceRasterStamp>,
    actions: FxHashMap<super::RetainedSurfaceResidentKey, RetainedSurfaceCompileAction>,
    programs: Vec<super::compiler::ValidatedNativeScrollForestBoundaryProgram>,
    geometries: Vec<PreparedScrollContentCompositeGeometry>,
    root_boundaries: Vec<super::frame_plan::NativeScrollBoundaryId>,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NativeScrollForestEmissionTraceStep {
    Host(super::frame_plan::NativeScrollBoundaryId),
    BeginContentRaster(super::frame_plan::NativeScrollBoundaryId),
    ContentArtifact(super::frame_plan::NativeScrollBoundaryId),
    ReuseContent(super::frame_plan::NativeScrollBoundaryId),
    CompositeContent(super::frame_plan::NativeScrollBoundaryId),
    Overlay(super::frame_plan::NativeScrollBoundaryId),
}

#[cfg(test)]
impl PreparedNativeScrollForestTransaction {
    pub(crate) fn stamps_for_test(&self) -> &[RetainedSurfaceRasterStamp] {
        &self.stamps
    }

    pub(crate) fn actions_for_test(
        &self,
    ) -> &FxHashMap<super::RetainedSurfaceResidentKey, RetainedSurfaceCompileAction> {
        &self.actions
    }

    pub(crate) fn transaction_is_canonical_for_test(&self) -> bool {
        self.transaction.is_canonical()
    }

    pub(crate) fn program_shapes_for_test(&self) -> Vec<[usize; 4]> {
        self.programs
            .iter()
            .map(|program| program.shape_for_test())
            .collect()
    }

    pub(super) fn emission_trace_for_test(&self) -> Vec<NativeScrollForestEmissionTraceStep> {
        fn emit(
            boundary: super::frame_plan::NativeScrollBoundaryId,
            prepared: &PreparedNativeScrollForestTransaction,
            trace: &mut Vec<NativeScrollForestEmissionTraceStep>,
        ) {
            let program = &prepared.programs[boundary.0 as usize];
            let key = prepared.stamps[boundary.0 as usize].identity.resident_key();
            trace.push(NativeScrollForestEmissionTraceStep::Host(boundary));
            match prepared.actions[&key] {
                RetainedSurfaceCompileAction::Reraster => {
                    trace.push(NativeScrollForestEmissionTraceStep::BeginContentRaster(
                        boundary,
                    ));
                    for step in program.content_step_kinds() {
                        match step {
                            super::compiler::ValidatedNativeScrollForestContentStepKind::Artifact => {
                                trace.push(NativeScrollForestEmissionTraceStep::ContentArtifact(
                                    boundary,
                                ));
                            }
                            super::compiler::ValidatedNativeScrollForestContentStepKind::ChildBoundary(child) => {
                                emit(child, prepared, trace);
                            }
                        }
                    }
                }
                RetainedSurfaceCompileAction::Reuse => {
                    trace.push(NativeScrollForestEmissionTraceStep::ReuseContent(boundary));
                }
            }
            trace.push(NativeScrollForestEmissionTraceStep::CompositeContent(
                boundary,
            ));
            trace.push(NativeScrollForestEmissionTraceStep::Overlay(boundary));
        }
        let mut trace = Vec::new();
        for root in &self.root_boundaries {
            emit(*root, self, &mut trace);
        }
        trace
    }
}

struct PreparedFrameRootScrollRoot {
    receiver_compiler: super::compiler::ValidatedFrameRootScrollReceiver,
    scroll_cutout: super::PlannedBoundary,
    content_compiler: super::compiler::ValidatedFrameRootScrollContent,
    backing: PreparedRetainedPropertyScrollBacking,
    content_local_terminal: u32,
}

enum PreparedFrameRootSceneRoot {
    Plain {
        receiver_compiler: super::compiler::ValidatedFrameRootScrollReceiver,
    },
    Scroll(PreparedFrameRootScrollRoot),
}

pub(crate) struct PreparedFrameRootScrollScene<'a> {
    viewport: &'a mut Viewport,
    graph: &'a mut FrameGraph,
    parent_ctx: UiBuildContext,
    roots: Vec<PreparedFrameRootSceneRoot>,
    actions: FxHashMap<super::RetainedSurfaceResidentKey, RetainedSurfaceCompileAction>,
    transaction: RetainedPropertyScrollSceneTransaction,
    clear_rgba_bits: [u32; 4],
    frame_owner: RetainedSurfaceFrameStageOwner,
    trace: RetainedPropertyScrollSceneBuildTrace,
}

#[cfg(test)]
impl PreparedFrameRootScrollScene<'_> {
    pub(crate) fn scroll_content_stamps_for_test(&self) -> Vec<RetainedSurfaceRasterStamp> {
        self.roots
            .iter()
            .filter_map(|root| match root {
                PreparedFrameRootSceneRoot::Plain { .. } => None,
                PreparedFrameRootSceneRoot::Scroll(root) => match &root.backing {
                    PreparedRetainedPropertyScrollBacking::Single { stamp, .. } => {
                        Some(stamp.clone())
                    }
                    PreparedRetainedPropertyScrollBacking::Tiled { .. } => None,
                },
            })
            .collect()
    }

    pub(crate) fn refresh_actions_from_committed_test_pool(&mut self) {
        self.actions = self
            .viewport
            .freeze_retained_property_scroll_scene_compile_actions_for_forced_test(
                &self.transaction,
            )
            .expect("prepared frame-root scroll scene remains canonical for the test pool");
        self.trace.reraster_count = self
            .actions
            .values()
            .filter(|action| **action == RetainedSurfaceCompileAction::Reraster)
            .count();
        self.trace.reuse_count = self.actions.len() - self.trace.reraster_count;
    }

    pub(crate) fn actions_for_test(&self) -> Vec<RetainedSurfaceCompileAction> {
        self.actions.values().copied().collect()
    }
}

/// S3b exclusive pre-clear lease for the direct S->T transaction.  Every
/// artifact, descriptor, action, context and staging capability is frozen
/// before this value borrows the mutable graph.
pub(crate) struct PreparedDirectScrollTransformScene<'a> {
    viewport: &'a mut Viewport,
    graph: &'a mut FrameGraph,
    parent_ctx: UiBuildContext,
    host_before: PaintArtifact,
    overlay_after: PaintArtifact,
    content: PaintArtifact,
    boundary_root: NodeKey,
    transform_content: NodeKey,
    transform: TransformNodeId,
    scroll: ScrollNodeSnapshot,
    host_bounds_bits: [u32; 4],
    geometry: super::PreparedScrollTransformContentCompositeGeometry,
    stamp: RetainedSurfaceRasterStamp,
    color_key: PersistentTextureKey,
    color_desc: TextureDesc,
    action: RetainedSurfaceCompileAction,
    transaction: RetainedPropertyScrollSceneTransaction,
    host_terminal: u32,
    content_terminal: u32,
    overlay_terminal: u32,
    clear_rgba_bits: [u32; 4],
    frame_owner: RetainedSurfaceFrameStageOwner,
    trace: RetainedPropertyScrollSceneBuildTrace,
}

#[cfg(test)]
impl PreparedDirectScrollTransformScene<'_> {
    pub(crate) fn action_for_test(&self) -> RetainedSurfaceCompileAction {
        self.action
    }

    pub(crate) fn transaction_shape_for_test(&self) -> [usize; 2] {
        [
            self.transaction.generic_full_set.len(),
            self.transaction.scroll_groups.len(),
        ]
    }

    pub(crate) fn graph_declared_key_count_for_test(&self) -> usize {
        self.graph.declared_persistent_texture_keys().count()
    }

    pub(crate) fn parent_terminal_for_test(&self) -> u32 {
        self.overlay_terminal
    }

    pub(crate) fn composite_params_for_test(
        &self,
    ) -> crate::view::render_pass::TextureCompositeParams {
        self.geometry.params()
    }

    pub(crate) fn refresh_action_from_committed_test_pool(&mut self) {
        let actions = self
            .viewport
            .freeze_retained_property_scroll_scene_compile_actions_for_forced_test(
                &self.transaction,
            )
            .expect("prepared direct S->T transaction remains canonical for the test pool");
        assert_eq!(actions.len(), 1);
        self.action = actions[&self.stamp.identity.resident_key()];
        self.trace.reraster_count =
            usize::from(self.action == RetainedSurfaceCompileAction::Reraster);
        self.trace.reuse_count = usize::from(self.action == RetainedSurfaceCompileAction::Reuse);
    }
}

struct PreparedTransformScrollRoot {
    receiver: TransformNodeSnapshot,
    receiver_stable_id: u64,
    geometry: crate::view::base_component::TransformSurfaceGeometrySnapshot,
    receiver_steps: Vec<super::frame_recorder::RecordedTransformSurfaceStep>,
    boundary: PreparedRetainedPropertyScrollBoundaryParts,
    receiver_stamp: RetainedSurfaceRasterStamp,
    receiver_color_key: PersistentTextureKey,
    receiver_color_desc: TextureDesc,
    receiver_opaque_terminal: u32,
}

/// Exclusive pre-clear B4-2B capability. The receiver and detached-content
/// actions plus their cross-bound joint transaction are frozen before this
/// token can borrow the viewport or graph for emission.
pub(crate) struct PreparedRetainedTransformScrollScene<'a> {
    viewport: &'a mut Viewport,
    graph: &'a mut FrameGraph,
    parent_ctx: UiBuildContext,
    roots: Vec<PreparedTransformScrollRoot>,
    actions: FxHashMap<super::RetainedSurfaceResidentKey, RetainedSurfaceCompileAction>,
    transaction: RetainedPropertyScrollSceneTransaction,
    clear_rgba_bits: [u32; 4],
    frame_owner: RetainedSurfaceFrameStageOwner,
    trace: RetainedPropertyScrollSceneBuildTrace,
}

struct PreparedEffectScrollRoot {
    receiver: EffectNodeSnapshot,
    receiver_stable_id: u64,
    artifact_contract: super::EffectPropertySurfaceArtifactContract,
    composite: EffectScrollCompositeWitness,
    receiver_steps: Vec<super::frame_recorder::RecordedTransformSurfaceStep>,
    boundary: PreparedRetainedPropertyScrollBoundaryParts,
    receiver_stamp: RetainedSurfaceRasterStamp,
    receiver_color_key: PersistentTextureKey,
    receiver_color_desc: TextureDesc,
    receiver_opaque_terminal: u32,
}

/// Exclusive pre-clear B4-2C capability. Receiver geometry, final opacity,
/// pool actions, aggregate budget, frame owner and the joint transaction are
/// all frozen before graph mutation begins.
pub(crate) struct PreparedRetainedEffectScrollScene<'a> {
    viewport: &'a mut Viewport,
    graph: &'a mut FrameGraph,
    parent_ctx: UiBuildContext,
    roots: Vec<PreparedEffectScrollRoot>,
    actions: FxHashMap<super::RetainedSurfaceResidentKey, RetainedSurfaceCompileAction>,
    transaction: RetainedPropertyScrollSceneTransaction,
    clear_rgba_bits: [u32; 4],
    frame_owner: RetainedSurfaceFrameStageOwner,
    trace: RetainedPropertyScrollSceneBuildTrace,
}

struct PreparedTransformEffectScrollRoot {
    outer_receiver: TransformNodeSnapshot,
    outer_stable_id: u64,
    outer_geometry: crate::view::base_component::TransformSurfaceGeometrySnapshot,
    outer_steps: Vec<super::frame_recorder::RecordedTransformSurfaceStep>,
    outer_stamp: RetainedSurfaceRasterStamp,
    outer_color_key: PersistentTextureKey,
    outer_color_desc: TextureDesc,
    outer_opaque_terminal: u32,
    inner: PreparedEffectScrollRoot,
}

/// Exclusive pre-clear capability for the three-target T/E/content
/// transaction. All stamps, actions, descriptors and the aggregate budget are
/// frozen before this value borrows the graph for emission.
pub(crate) struct PreparedRetainedTransformEffectScrollScene<'a> {
    viewport: &'a mut Viewport,
    graph: &'a mut FrameGraph,
    parent_ctx: UiBuildContext,
    roots: Vec<PreparedTransformEffectScrollRoot>,
    actions: FxHashMap<super::RetainedSurfaceResidentKey, RetainedSurfaceCompileAction>,
    transaction: RetainedPropertyScrollSceneTransaction,
    clear_rgba_bits: [u32; 4],
    frame_owner: RetainedSurfaceFrameStageOwner,
    trace: RetainedPropertyScrollSceneBuildTrace,
}

struct PreparedEffectTransformScrollRoot {
    outer_receiver: EffectNodeSnapshot,
    outer_stable_id: u64,
    outer_artifact_contract: super::EffectPropertySurfaceArtifactContract,
    outer_composite: EffectScrollCompositeWitness,
    outer_steps: Vec<super::frame_recorder::RecordedTransformSurfaceStep>,
    outer_stamp: RetainedSurfaceRasterStamp,
    outer_color_key: PersistentTextureKey,
    outer_color_desc: TextureDesc,
    outer_opaque_terminal: u32,
    validated_inner_steps: super::compiler::ValidatedFrameRootScrollReceiver,
    inner: PreparedTransformScrollRoot,
}

pub(crate) struct PreparedRetainedEffectTransformScrollScene<'a> {
    viewport: &'a mut Viewport,
    graph: &'a mut FrameGraph,
    parent_ctx: UiBuildContext,
    roots: Vec<PreparedEffectTransformScrollRoot>,
    actions: FxHashMap<super::RetainedSurfaceResidentKey, RetainedSurfaceCompileAction>,
    transaction: RetainedPropertyScrollSceneTransaction,
    clear_rgba_bits: [u32; 4],
    frame_owner: RetainedSurfaceFrameStageOwner,
    trace: RetainedPropertyScrollSceneBuildTrace,
}

struct PreparedScrollContentEffectRoot {
    validated: ValidatedScrollContentEffectRoot,
    frozen: FrozenScrollContentEffectRoot,
    content_geometry: PreparedScrollContentCompositeGeometry,
}

fn upgrade_scroll_content_effect_actions(
    roots: &[FrozenScrollContentEffectRoot],
    actions: &mut FxHashMap<super::RetainedSurfaceResidentKey, RetainedSurfaceCompileAction>,
) -> Option<()> {
    for root in roots {
        let effect_key = root.effect_stamp.identity.resident_key();
        let content_key = root.content_stamp.identity.resident_key();
        if actions.get(&effect_key).copied() == Some(RetainedSurfaceCompileAction::Reraster) {
            *actions.get_mut(&content_key)? = RetainedSurfaceCompileAction::Reraster;
        }
        if actions.get(&content_key).copied() == Some(RetainedSurfaceCompileAction::Reraster) {
            if let Some(outer) = &root.outer_stamp {
                *actions.get_mut(&outer.identity.resident_key())? =
                    RetainedSurfaceCompileAction::Reraster;
            }
        }
    }
    Some(())
}

pub(crate) struct PreparedRetainedScrollContentEffectScene<'a> {
    viewport: &'a mut Viewport,
    graph: &'a mut FrameGraph,
    parent_ctx: UiBuildContext,
    roots: Vec<PreparedScrollContentEffectRoot>,
    actions: FxHashMap<super::RetainedSurfaceResidentKey, RetainedSurfaceCompileAction>,
    transaction: RetainedPropertyScrollSceneTransaction,
    clear_rgba_bits: [u32; 4],
    frame_owner: RetainedSurfaceFrameStageOwner,
    trace: RetainedPropertyScrollSceneBuildTrace,
}

#[cfg(test)]
impl PreparedRetainedScrollContentEffectScene<'_> {
    fn refresh_actions_from_committed_test_pool(&mut self) {
        self.actions = self
            .viewport
            .freeze_retained_property_scroll_scene_compile_actions_for_forced_test(
                &self.transaction,
            )
            .expect("prepared Phase3 scene remains canonical for the test pool");
        let frozen = self
            .roots
            .iter()
            .map(|root| root.frozen.clone())
            .collect::<Vec<_>>();
        upgrade_scroll_content_effect_actions(&frozen, &mut self.actions)
            .expect("prepared Phase3 action set remains complete");
        self.trace.reraster_count = self
            .actions
            .values()
            .filter(|action| **action == RetainedSurfaceCompileAction::Reraster)
            .count();
        self.trace.reuse_count = self.actions.len() - self.trace.reraster_count;
    }
}

/// One joint pre-clear lease selected by `PropertyBoundaryDagCompiler`.
/// Holding the concrete legacy prepared token inside the enum preserves the
/// existing all-or-nothing pool transaction and graph-mutation boundary.
#[allow(dead_code)] // Phase1 joint lease is test-sealed before production selection changes.
pub(crate) enum PreparedPropertyBoundaryDagScene<'a> {
    FrameRootScroll(PreparedFrameRootScrollScene<'a>),
    TransformScroll(PreparedRetainedTransformScrollScene<'a>),
    EffectScroll(PreparedRetainedEffectScrollScene<'a>),
    TransformEffectScroll(PreparedRetainedTransformEffectScrollScene<'a>),
    EffectTransformScroll(PreparedRetainedEffectTransformScrollScene<'a>),
    ScrollEffect(PreparedRetainedScrollContentEffectScene<'a>),
    TransformScrollEffect(PreparedRetainedScrollContentEffectScene<'a>),
}

#[cfg(test)]
impl PreparedPropertyBoundaryDagScene<'_> {
    fn graph_build_state_snapshot_for_test(&self) -> crate::view::frame_graph::TopologySignature {
        match self {
            Self::FrameRootScroll(prepared) => prepared.graph.build_state_snapshot_for_test(),
            Self::TransformScroll(prepared) => prepared.graph.build_state_snapshot_for_test(),
            Self::EffectScroll(prepared) => prepared.graph.build_state_snapshot_for_test(),
            Self::TransformEffectScroll(prepared) => prepared.graph.build_state_snapshot_for_test(),
            Self::EffectTransformScroll(prepared) => prepared.graph.build_state_snapshot_for_test(),
            Self::ScrollEffect(prepared) | Self::TransformScrollEffect(prepared) => {
                prepared.graph.build_state_snapshot_for_test()
            }
        }
    }

    fn refresh_actions_from_committed_test_pool(&mut self) {
        match self {
            Self::FrameRootScroll(prepared) => prepared.refresh_actions_from_committed_test_pool(),
            Self::TransformScroll(prepared) => prepared.refresh_actions_from_committed_test_pool(),
            Self::EffectScroll(prepared) => prepared.refresh_actions_from_committed_test_pool(),
            Self::TransformEffectScroll(prepared) => {
                prepared.refresh_actions_from_committed_test_pool()
            }
            Self::EffectTransformScroll(prepared) => {
                prepared.refresh_actions_from_committed_test_pool()
            }
            Self::ScrollEffect(prepared) | Self::TransformScrollEffect(prepared) => {
                prepared.refresh_actions_from_committed_test_pool()
            }
        }
    }

    fn action_set_is_exact_for_test(&self) -> bool {
        let (actions, transaction) = match self {
            Self::FrameRootScroll(prepared) => (&prepared.actions, &prepared.transaction),
            Self::TransformScroll(prepared) => (&prepared.actions, &prepared.transaction),
            Self::EffectScroll(prepared) => (&prepared.actions, &prepared.transaction),
            Self::TransformEffectScroll(prepared) => (&prepared.actions, &prepared.transaction),
            Self::EffectTransformScroll(prepared) => (&prepared.actions, &prepared.transaction),
            Self::ScrollEffect(prepared) | Self::TransformScrollEffect(prepared) => {
                (&prepared.actions, &prepared.transaction)
            }
        };
        let expected = transaction
            .ordered_stamps()
            .into_iter()
            .map(|stamp| stamp.identity.resident_key())
            .collect::<FxHashSet<_>>();
        let actual = actions.keys().copied().collect::<FxHashSet<_>>();
        actions.len() == expected.len() && actual == expected
    }

    fn rejects_action_set_mismatch_for_test(&self) -> bool {
        let (actions, transaction) = match self {
            Self::FrameRootScroll(prepared) => (&prepared.actions, &prepared.transaction),
            Self::TransformScroll(prepared) => (&prepared.actions, &prepared.transaction),
            Self::EffectScroll(prepared) => (&prepared.actions, &prepared.transaction),
            Self::TransformEffectScroll(prepared) => (&prepared.actions, &prepared.transaction),
            Self::EffectTransformScroll(prepared) => (&prepared.actions, &prepared.transaction),
            Self::ScrollEffect(prepared) | Self::TransformScrollEffect(prepared) => {
                (&prepared.actions, &prepared.transaction)
            }
        };
        let mut tampered = actions.clone();
        let Some(key) = tampered.keys().next().copied() else {
            return false;
        };
        tampered.remove(&key);
        let expected = transaction
            .ordered_stamps()
            .into_iter()
            .map(|stamp| stamp.identity.resident_key())
            .collect::<FxHashSet<_>>();
        let actual = tampered.keys().copied().collect::<FxHashSet<_>>();
        tampered.len() != expected.len() || actual != expected
    }
}

#[cfg(test)]
impl PreparedRetainedPropertyScrollForest<'_> {
    pub(crate) fn graph_build_state_snapshot_for_test(
        &self,
    ) -> crate::view::frame_graph::TopologySignature {
        self.graph.build_state_snapshot_for_test()
    }

    pub(crate) fn scroll_content_stamps_for_test(&self) -> Vec<RetainedSurfaceRasterStamp> {
        self.boundaries
            .iter()
            .flat_map(|boundary| match &boundary.backing {
                PreparedRetainedPropertyScrollBacking::Single { stamp, .. } => {
                    vec![stamp.clone()]
                }
                PreparedRetainedPropertyScrollBacking::Tiled { tiles, .. } => {
                    tiles.iter().map(|tile| tile.stamp.clone()).collect()
                }
            })
            .collect()
    }

    pub(crate) fn refresh_actions_from_committed_test_pool(&mut self) {
        self.actions = self
            .viewport
            .freeze_retained_property_scroll_scene_compile_actions_for_forced_test(
                &self.transaction,
            )
            .expect("prepared forest remains canonical for the committed test pool");
        self.trace.reraster_count = self
            .actions
            .values()
            .filter(|action| **action == RetainedSurfaceCompileAction::Reraster)
            .count();
        self.trace.reuse_count = self.actions.len() - self.trace.reraster_count;
    }
}

#[cfg(test)]
impl PreparedRetainedPropertyScrollScene<'_> {
    fn refresh_actions_from_committed_test_pool(&mut self) {
        self.actions = self
            .viewport
            .freeze_retained_property_scroll_scene_compile_actions_for_forced_test(
                &self.transaction,
            )
            .expect("prepared transaction remains canonical for the test pool");
        self.trace.reraster_count = self
            .actions
            .values()
            .filter(|action| **action == RetainedSurfaceCompileAction::Reraster)
            .count();
        self.trace.reuse_count = self.actions.len() - self.trace.reraster_count;
    }
}

#[cfg(test)]
impl PreparedRetainedTransformScrollScene<'_> {
    fn refresh_actions_from_committed_test_pool(&mut self) {
        self.actions = self
            .viewport
            .freeze_retained_property_scroll_scene_compile_actions_for_forced_test(
                &self.transaction,
            )
            .expect("prepared transform-scroll scene remains canonical for the test pool");
        for root in &self.roots {
            let content_requires_raster =
                root.boundary
                    .group
                    .active_resident_keys()
                    .iter()
                    .any(|key| {
                        self.actions.get(key).copied()
                            == Some(RetainedSurfaceCompileAction::Reraster)
                    });
            if content_requires_raster {
                let receiver_action = self
                    .actions
                    .get_mut(&root.receiver_stamp.identity.resident_key())
                    .expect("prepared receiver action remains present");
                if *receiver_action == RetainedSurfaceCompileAction::Reuse {
                    *receiver_action = RetainedSurfaceCompileAction::Reraster;
                }
            }
        }
        self.trace.reraster_count = self
            .actions
            .values()
            .filter(|action| **action == RetainedSurfaceCompileAction::Reraster)
            .count();
        self.trace.reuse_count = self.actions.len() - self.trace.reraster_count;
    }
}

#[cfg(test)]
impl PreparedRetainedEffectScrollScene<'_> {
    fn refresh_actions_from_committed_test_pool(&mut self) {
        self.actions = self
            .viewport
            .freeze_retained_property_scroll_scene_compile_actions_for_forced_test(
                &self.transaction,
            )
            .expect("prepared effect-scroll scene remains canonical for the test pool");
        for root in &self.roots {
            let content_requires_raster =
                root.boundary
                    .group
                    .active_resident_keys()
                    .iter()
                    .any(|key| {
                        self.actions.get(key).copied()
                            == Some(RetainedSurfaceCompileAction::Reraster)
                    });
            if content_requires_raster {
                let receiver_action = self
                    .actions
                    .get_mut(&root.receiver_stamp.identity.resident_key())
                    .expect("prepared effect receiver action remains present");
                if *receiver_action == RetainedSurfaceCompileAction::Reuse {
                    *receiver_action = RetainedSurfaceCompileAction::Reraster;
                }
            }
        }
        self.trace.reraster_count = self
            .actions
            .values()
            .filter(|action| **action == RetainedSurfaceCompileAction::Reraster)
            .count();
        self.trace.reuse_count = self.actions.len() - self.trace.reraster_count;
    }
}

#[cfg(test)]
impl PreparedRetainedTransformEffectScrollScene<'_> {
    fn refresh_actions_from_committed_test_pool(&mut self) {
        self.actions = self
            .viewport
            .freeze_retained_property_scroll_scene_compile_actions_for_forced_test(
                &self.transaction,
            )
            .expect("prepared transform-effect-scroll scene remains canonical for the test pool");
        for root in &self.roots {
            let content_requires_raster = root
                .inner
                .boundary
                .group
                .active_resident_keys()
                .iter()
                .any(|key| {
                    self.actions.get(key).copied() == Some(RetainedSurfaceCompileAction::Reraster)
                });
            if content_requires_raster {
                let inner_action = self
                    .actions
                    .get_mut(&root.inner.receiver_stamp.identity.resident_key())
                    .expect("prepared inner effect action remains present");
                if *inner_action == RetainedSurfaceCompileAction::Reuse {
                    *inner_action = RetainedSurfaceCompileAction::Reraster;
                }
            }
        }
        self.trace.reraster_count = self
            .actions
            .values()
            .filter(|action| **action == RetainedSurfaceCompileAction::Reraster)
            .count();
        self.trace.reuse_count = self.actions.len() - self.trace.reraster_count;
    }
}

#[cfg(test)]
impl PreparedRetainedEffectTransformScrollScene<'_> {
    fn refresh_actions_from_committed_test_pool(&mut self) {
        self.actions = self
            .viewport
            .freeze_retained_property_scroll_scene_compile_actions_for_forced_test(
                &self.transaction,
            )
            .expect("prepared effect-transform-scroll scene remains canonical for the test pool");
        for root in &self.roots {
            let content_requires_raster = root
                .inner
                .boundary
                .group
                .active_resident_keys()
                .iter()
                .any(|key| {
                    self.actions.get(key).copied() == Some(RetainedSurfaceCompileAction::Reraster)
                });
            if content_requires_raster {
                let inner_action = self
                    .actions
                    .get_mut(&root.inner.receiver_stamp.identity.resident_key())
                    .expect("prepared inner transform action remains present");
                if *inner_action == RetainedSurfaceCompileAction::Reuse {
                    *inner_action = RetainedSurfaceCompileAction::Reraster;
                }
            }
        }
        self.trace.reraster_count = self
            .actions
            .values()
            .filter(|action| **action == RetainedSurfaceCompileAction::Reraster)
            .count();
        self.trace.reuse_count = self.actions.len() - self.trace.reraster_count;
    }
}

#[derive(Clone, Debug)]
#[allow(dead_code)] // Production dispatch/telemetry wiring lands after the B2 pool/executor slice.
pub(crate) struct RetainedPropertyScrollSceneBuildTrace {
    pub(crate) root_count: usize,
    pub(crate) generic_surface_count: usize,
    pub(crate) effect_surface_count: usize,
    pub(crate) scroll_group_count: usize,
    pub(crate) backing: ScrollSceneBackingKind,
    pub(crate) tile_count: usize,
    pub(crate) reraster_count: usize,
    pub(crate) reuse_count: usize,
    pub(crate) content_pair_bytes: u64,
}

pub(crate) struct RetainedPropertyScrollSceneBuildOutcome {
    state: BuildState,
    trace: RetainedPropertyScrollSceneBuildTrace,
}

impl RetainedPropertyScrollSceneBuildOutcome {
    pub(crate) fn into_parts(self) -> (BuildState, RetainedPropertyScrollSceneBuildTrace) {
        (self.state, self.trace)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // Singleton B1 validation remains as a test oracle after B4 forest cutover.
enum PropertyScrollBoundaryValidationError {
    PlannerDrift,
    LiveSnapshotDrift,
    ArtifactContract,
    StampContract,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ScrollSceneSingleTextureBudget {
    max_dimension_2d: u32,
    max_pair_bytes: u64,
}

impl ScrollSceneSingleTextureBudget {
    pub(crate) fn new(max_dimension_2d: u32, max_pair_bytes: u64) -> Option<Self> {
        (max_dimension_2d > 0 && max_pair_bytes > 0).then_some(Self {
            max_dimension_2d,
            max_pair_bytes,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ScrollScenePrepareError {
    PlanShape,
    FrozenWitness,
    ArtifactStore,
    ContextMismatch,
    ParentTarget,
    PersistentKeyAlreadyDeclared(PersistentTextureKey),
    DescriptorPair,
    GeometryContract,
    SingleTextureLimit,
    RasterCostUnknown,
    RasterCostOverflow,
    ActiveTileBudget,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SingleTextureAdmissionError {
    DimensionExceeded,
    PairBudgetExceeded,
}

#[derive(Debug)]
#[allow(dead_code)] // Typed payloads are surfaced through the canary's Debug trace.
pub(crate) enum ScrollSceneFromLiveError {
    LiveSnapshotDrift,
    Plan(FramePaintPlanError),
    Prepare(ScrollScenePrepareError),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ScrollSceneBackingKind {
    Single,
    Tiled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ScrollSceneBuildTrace {
    pub(crate) backing: ScrollSceneBackingKind,
    pub(crate) action: RetainedSurfaceCompileAction,
    pub(crate) content_root: NodeKey,
    pub(crate) descriptor_size: [u32; 2],
    pub(crate) content_chunk_count: usize,
    pub(crate) content_op_count: usize,
    pub(crate) content_pair_bytes: u64,
    pub(crate) tile_count: usize,
    pub(crate) reraster_count: usize,
    pub(crate) reuse_count: usize,
}

pub(crate) struct ScrollSceneBuildOutcome {
    state: BuildState,
    trace: ScrollSceneBuildTrace,
}

impl ScrollSceneBuildOutcome {
    pub(crate) fn into_parts(self) -> (BuildState, ScrollSceneBuildTrace) {
        (self.state, self.trace)
    }
}

/// Fully validated, graph-inert A2 token. The content action is deliberately
/// absent until the viewport pool freezes it into the A3 typestate.
struct PreparedScrollScene {
    host_before: ValidatedScrollSceneHostBeforeArtifact,
    content: ValidatedScrollSceneContentArtifact,
    overlay: ValidatedScrollSceneOverlayArtifact,
    content_backing: PreparedScrollContentBacking,
    parent_target: Option<RenderTargetOut>,
    host_parent_terminal: u32,
    content_local_terminal: u32,
    parent_terminal: u32,
}

struct PreparedScrollTile {
    stamp: RetainedSurfaceRasterStamp,
    color_key: PersistentTextureKey,
    color_desc: TextureDesc,
    geometry: super::PreparedScrollContentTileCompositeGeometry,
}

enum PreparedScrollContentBacking {
    Single {
        stamp: RetainedSurfaceRasterStamp,
        color_key: PersistentTextureKey,
        color_desc: TextureDesc,
        geometry: PreparedScrollContentCompositeGeometry,
        pair_bytes: u64,
    },
    Tiled {
        manifest: super::ScrollContentTileSetTransactionStamp,
        tiles: Vec<PreparedScrollTile>,
        total_pair_bytes: u64,
    },
}

struct FrozenPreparedScrollScene {
    prepared: PreparedScrollScene,
    content_actions: FrozenScrollContentActions,
}

enum FrozenScrollContentActions {
    Single(RetainedSurfaceCompileAction),
    Tiled(Vec<RetainedSurfaceCompileAction>),
}

struct PreparedScrollSceneEmission {
    host_before: ValidatedScrollSceneHostBeforeArtifact,
    content: ValidatedScrollSceneContentArtifact,
    overlay: ValidatedScrollSceneOverlayArtifact,
    content_backing: PreparedScrollContentBacking,
    parent_target: Option<RenderTargetOut>,
    host_parent_terminal: u32,
    content_local_terminal: u32,
    parent_terminal: u32,
    content_actions: FrozenScrollContentActions,
}

#[cfg(test)]
impl PreparedScrollScene {
    fn content_stamp(&self) -> &RetainedSurfaceRasterStamp {
        match &self.content_backing {
            PreparedScrollContentBacking::Single { stamp, .. } => stamp,
            PreparedScrollContentBacking::Tiled { .. } => {
                panic!("single content stamp requested from tiled scene")
            }
        }
    }

    fn content_key(&self) -> PersistentTextureKey {
        self.content_stamp().identity.color_key
    }

    fn content_geometry(&self) -> PreparedScrollContentCompositeGeometry {
        match &self.content_backing {
            PreparedScrollContentBacking::Single { geometry, .. } => *geometry,
            PreparedScrollContentBacking::Tiled { .. } => {
                panic!("single content geometry requested from tiled scene")
            }
        }
    }

    fn content_pair_bytes(&self) -> u64 {
        match &self.content_backing {
            PreparedScrollContentBacking::Single { pair_bytes, .. } => *pair_bytes,
            PreparedScrollContentBacking::Tiled {
                total_pair_bytes, ..
            } => *total_pair_bytes,
        }
    }

    fn tile_stamps(&self) -> Option<Vec<RetainedSurfaceRasterStamp>> {
        match &self.content_backing {
            PreparedScrollContentBacking::Tiled { tiles, .. } => {
                Some(tiles.iter().map(|tile| tile.stamp.clone()).collect())
            }
            PreparedScrollContentBacking::Single { .. } => None,
        }
    }
}

#[cfg(test)]
impl PreparedScrollSceneEmission {
    fn content_stamp(&self) -> &RetainedSurfaceRasterStamp {
        match &self.content_backing {
            PreparedScrollContentBacking::Single { stamp, .. } => stamp,
            PreparedScrollContentBacking::Tiled { .. } => {
                panic!("single content stamp requested from tiled emission")
            }
        }
    }

    fn content_action(&self) -> RetainedSurfaceCompileAction {
        match &self.content_actions {
            FrozenScrollContentActions::Single(action) => *action,
            FrozenScrollContentActions::Tiled(_) => {
                panic!("single content action requested from tiled emission")
            }
        }
    }
}

impl PreparedScrollScene {
    fn freeze_content_action(
        self,
        content_action: RetainedSurfaceCompileAction,
    ) -> FrozenPreparedScrollScene {
        assert!(matches!(
            &self.content_backing,
            PreparedScrollContentBacking::Single { .. }
        ));
        FrozenPreparedScrollScene {
            prepared: self,
            content_actions: FrozenScrollContentActions::Single(content_action),
        }
    }

    fn freeze_tile_actions(
        self,
        actions: Vec<RetainedSurfaceCompileAction>,
    ) -> Option<FrozenPreparedScrollScene> {
        let PreparedScrollContentBacking::Tiled { tiles, .. } = &self.content_backing else {
            return None;
        };
        (actions.len() == tiles.len()).then_some(FrozenPreparedScrollScene {
            prepared: self,
            content_actions: FrozenScrollContentActions::Tiled(actions),
        })
    }
}

impl FrozenPreparedScrollScene {
    #[cfg(test)]
    fn content_action(&self) -> RetainedSurfaceCompileAction {
        match &self.content_actions {
            FrozenScrollContentActions::Single(action) => *action,
            FrozenScrollContentActions::Tiled(actions) => {
                if actions.contains(&RetainedSurfaceCompileAction::Reraster) {
                    RetainedSurfaceCompileAction::Reraster
                } else {
                    RetainedSurfaceCompileAction::Reuse
                }
            }
        }
    }

    fn into_emission_parts(self) -> PreparedScrollSceneEmission {
        let PreparedScrollScene {
            host_before,
            content,
            overlay,
            content_backing,
            parent_target,
            host_parent_terminal,
            content_local_terminal,
            parent_terminal,
        } = self.prepared;
        PreparedScrollSceneEmission {
            host_before,
            content,
            overlay,
            content_backing,
            parent_target,
            host_parent_terminal,
            content_local_terminal,
            parent_terminal,
            content_actions: self.content_actions,
        }
    }
}

fn bounds_bits(bounds: RetainedSurfaceBounds) -> [u32; 4] {
    [
        bounds.x.to_bits(),
        bounds.y.to_bits(),
        bounds.width.to_bits(),
        bounds.height.to_bits(),
    ]
}

fn content_zero_bounds(scroll: ScrollNodeSnapshot) -> RetainedSurfaceBounds {
    RetainedSurfaceBounds {
        x: scroll.layout_content_bounds_at_zero.x,
        y: scroll.layout_content_bounds_at_zero.y,
        width: scroll.layout_content_bounds_at_zero.width,
        height: scroll.layout_content_bounds_at_zero.height,
        corner_radii: [0.0; 4],
    }
}

fn nested_receiver_local_content_bounds(
    inner_source: RetainedSurfaceBounds,
    inner_scroll: ScrollNodeSnapshot,
) -> Option<RetainedSurfaceBounds> {
    let world = content_zero_bounds(inner_scroll);
    let local = RetainedSurfaceBounds {
        x: world.x - inner_source.x,
        y: world.y - inner_source.y,
        width: world.width,
        height: world.height,
        corner_radii: [0.0; 4],
    };
    exact_dpr1_u32_bounds(local).map(|_| local)
}

const SCROLL_CONTENT_TILE_EDGE: u32 = 1024;
const SCROLL_CONTENT_TILE_GUTTER: u32 = 1;
const SCROLL_CONTENT_TILE_OVERSCAN: u32 = 256;

fn exact_dpr1_u32_bounds(bounds: RetainedSurfaceBounds) -> Option<[u32; 4]> {
    let values = [bounds.x, bounds.y, bounds.width, bounds.height];
    if values.iter().any(|value| {
        !value.is_finite()
            || *value < 0.0
            || value.fract().to_bits() != 0.0_f32.to_bits()
            || *value > u32::MAX as f32
    }) || bounds.width <= 0.0
        || bounds.height <= 0.0
    {
        return None;
    }
    let result = values.map(|value| value as u32);
    result[0].checked_add(result[2])?;
    result[1].checked_add(result[3])?;
    Some(result)
}

fn property_scroll_budget(budget: ScrollSceneSingleTextureBudget) -> PropertyScrollBackingBudget {
    PropertyScrollBackingBudget {
        max_dimension_2d: budget.max_dimension_2d,
        max_active_pair_bytes: budget.max_pair_bytes,
        tile_edge: SCROLL_CONTENT_TILE_EDGE,
        gutter: SCROLL_CONTENT_TILE_GUTTER,
        overscan: SCROLL_CONTENT_TILE_OVERSCAN,
    }
}

fn canonical_pair_bytes(color: &TextureDesc, depth: &TextureDesc) -> Option<u64> {
    let color = crate::view::raster_cost::texture_desc_payload_bytes(color);
    let depth = crate::view::raster_cost::texture_desc_payload_bytes(depth);
    if !color.confidence.budget_usable() || !depth.confidence.budget_usable() {
        return None;
    }
    color.bytes.checked_add(depth.bytes)
}

fn plan_property_scroll_backing(
    content_stable_id: u64,
    scroll: ScrollNodeSnapshot,
    contents_clip: ClipNodeSnapshot,
    scale_factor: f32,
    target_format: wgpu::TextureFormat,
    budget: PropertyScrollBackingBudget,
) -> Option<PropertyScrollBackingPlan> {
    if content_stable_id == 0
        || budget.max_dimension_2d == 0
        || budget.max_active_pair_bytes == 0
        || budget.tile_edge == 0
        || budget.gutter != 1
    {
        return None;
    }
    let content_bounds = content_zero_bounds(scroll);
    let content_bounds_u32 = exact_dpr1_u32_bounds(content_bounds)?;
    if !scale_factor.is_finite() || scale_factor <= 0.0 {
        return None;
    }
    let color_key = scroll_content_layer_stable_key(content_stable_id);
    let color = texture_desc_for_logical_bounds(content_bounds, scale_factor, None, target_format);
    let (color_desc, depth_desc) = persistent_target_texture_descriptors(color, color_key);
    let pair_bytes = canonical_pair_bytes(&color_desc, &depth_desc)?;
    let single_fits = [
        color_desc.width(),
        color_desc.height(),
        depth_desc.width(),
        depth_desc.height(),
    ]
    .into_iter()
    .all(|dimension| dimension <= budget.max_dimension_2d)
        && pair_bytes <= budget.max_active_pair_bytes;
    if single_fits {
        return Some(PropertyScrollBackingPlan::Single(
            PropertyScrollSingleBackingPlan {
                color_key,
                color_desc,
                depth_desc,
                pair_bytes,
                budget,
            },
        ));
    }

    let manifest = super::plan_active_scroll_content_tiles_dpr1(
        content_bounds_u32,
        [scroll.offset.x, scroll.offset.y],
        contents_clip.logical_scissor,
        budget.tile_edge,
        budget.gutter,
        budget.overscan,
    )?;
    let mut previous = None;
    let mut total_pair_bytes = 0_u64;
    let mut color_keys = FxHashSet::default();
    let mut tiles = Vec::with_capacity(manifest.tiles().len());
    for &(index, bounds) in manifest.tiles() {
        if previous.is_some_and(|previous| previous >= index)
            || !bounds.is_canonical_for(content_bounds_u32, budget.tile_edge, budget.gutter, index)
        {
            return None;
        }
        previous = Some(index);
        let color_key = crate::view::base_component::scroll_content_tile_layer_stable_key(
            content_stable_id,
            index.column,
            index.row,
        )?;
        if !color_keys.insert(color_key) {
            return None;
        }
        let [x, y, width, height] = bounds.raster;
        let color = texture_desc_for_logical_bounds(
            RetainedSurfaceBounds {
                x: x as f32,
                y: y as f32,
                width: width as f32,
                height: height as f32,
                corner_radii: [0.0; 4],
            },
            scale_factor,
            None,
            target_format,
        );
        let (color_desc, depth_desc) = persistent_target_texture_descriptors(color, color_key);
        if [
            color_desc.width(),
            color_desc.height(),
            depth_desc.width(),
            depth_desc.height(),
        ]
        .into_iter()
        .any(|dimension| dimension > budget.max_dimension_2d)
        {
            return None;
        }
        let pair_bytes = canonical_pair_bytes(&color_desc, &depth_desc)?;
        total_pair_bytes = total_pair_bytes.checked_add(pair_bytes)?;
        tiles.push(PropertyScrollTilePlan {
            index,
            bounds,
            color_key,
            color_desc,
            depth_desc,
            pair_bytes,
        });
    }
    if tiles.is_empty() || total_pair_bytes > budget.max_active_pair_bytes {
        return None;
    }
    Some(PropertyScrollBackingPlan::Tiled(
        PropertyScrollTiledBackingPlan {
            content_bounds: content_bounds_u32,
            tile_edge: manifest.tile_edge(),
            gutter: manifest.gutter(),
            overscan: manifest.overscan(),
            tiles,
            total_pair_bytes,
            budget,
        },
    ))
}

fn property_scroll_step_identities(
    steps: &[ScrollBoundaryStep],
) -> Option<Vec<PropertyScrollStepIdentity>> {
    steps
        .iter()
        .map(|step| match step {
            ScrollBoundaryStep::HostBefore {
                artifact,
                identity,
                parent_span,
            } => (PropertyScrollPhaseArtifactIdentity::from_artifact(artifact)? == *identity).then(
                || PropertyScrollStepIdentity::HostBefore {
                    identity: identity.clone(),
                    parent_span: parent_span.clone(),
                },
            ),
            ScrollBoundaryStep::ContentComposite {
                boundary,
                artifact: _,
                content,
                composite,
                clip_split,
                backing,
                post_composite,
                parent_before,
                parent_after,
            } => Some(PropertyScrollStepIdentity::ContentComposite {
                boundary: *boundary,
                content: content.clone(),
                composite: *composite,
                clip_split: clip_split.clone(),
                backing: backing.clone(),
                post_composite: post_composite.clone(),
                parent_before: *parent_before,
                parent_after: *parent_after,
            }),
            ScrollBoundaryStep::OverlayAfter {
                artifact,
                identity,
                parent_span,
            } => (PropertyScrollPhaseArtifactIdentity::from_artifact(artifact)? == *identity).then(
                || PropertyScrollStepIdentity::OverlayAfter {
                    identity: identity.clone(),
                    parent_span: parent_span.clone(),
                },
            ),
            ScrollBoundaryStep::AtomicProjectionHostBefore {
                authority,
                identity,
                parent_span,
            } => (authority.is_canonical() && authority.identity() == *identity).then(|| {
                PropertyScrollStepIdentity::AtomicProjectionHostBefore {
                    identity: identity.clone(),
                    parent_span: parent_span.clone(),
                }
            }),
            ScrollBoundaryStep::AtomicProjectionContentComposite {
                boundary,
                authority,
                identity,
                content,
                composite,
                clip_split,
                backing,
                post_composite,
                parent_before,
                parent_after,
            } => (authority.is_canonical() && authority.identity() == *identity).then(|| {
                PropertyScrollStepIdentity::AtomicProjectionContentComposite {
                    boundary: *boundary,
                    identity: identity.clone(),
                    content: content.clone(),
                    composite: *composite,
                    clip_split: clip_split.clone(),
                    backing: backing.clone(),
                    post_composite: post_composite.clone(),
                    parent_before: *parent_before,
                    parent_after: *parent_after,
                }
            }),
            ScrollBoundaryStep::AtomicProjectionOverlayAfter {
                authority,
                identity,
                parent_span,
            } => (authority.is_canonical() && authority.identity() == *identity).then(|| {
                PropertyScrollStepIdentity::AtomicProjectionOverlayAfter {
                    identity: identity.clone(),
                    parent_span: parent_span.clone(),
                }
            }),
            ScrollBoundaryStep::AtomicProjectionSelectionHostBefore {
                authority,
                identity,
                parent_span,
            } => (authority.is_canonical() && authority.identity() == *identity).then(|| {
                PropertyScrollStepIdentity::AtomicProjectionSelectionHostBefore {
                    identity: identity.clone(),
                    parent_span: parent_span.clone(),
                }
            }),
            ScrollBoundaryStep::AtomicProjectionSelectionContentComposite {
                boundary,
                authority,
                identity,
                content,
                composite,
                clip_split,
                backing,
                post_composite,
                parent_before,
                parent_after,
            } => (authority.is_canonical() && authority.identity() == *identity).then(|| {
                PropertyScrollStepIdentity::AtomicProjectionSelectionContentComposite {
                    boundary: *boundary,
                    identity: identity.clone(),
                    content: content.clone(),
                    composite: *composite,
                    clip_split: clip_split.clone(),
                    backing: backing.clone(),
                    post_composite: post_composite.clone(),
                    parent_before: *parent_before,
                    parent_after: *parent_after,
                }
            }),
            ScrollBoundaryStep::AtomicProjectionSelectionOverlayAfter {
                authority,
                identity,
                parent_span,
            } => (authority.is_canonical() && authority.identity() == *identity).then(|| {
                PropertyScrollStepIdentity::AtomicProjectionSelectionOverlayAfter {
                    identity: identity.clone(),
                    parent_span: parent_span.clone(),
                }
            }),
        })
        .collect()
}

fn property_scroll_plan_is_canonical(plan: &PropertyScrollScenePlan) -> bool {
    let seal = &plan.seal;
    let scale_factor = f32::from_bits(seal.scale_factor_bits);
    if !scale_factor.is_finite()
        || scale_factor <= 0.0
        || seal.boundary
            != (SceneBoundaryId {
                ordinal: 0,
                owner: seal.scene_root,
                kind: SceneBoundaryKind::ScrollContents,
            })
        || seal.scene_root_stable_id == 0
        || seal.scroll != seal.planned_scroll
        || seal.contents_clip != seal.planned_contents_clip
        || !seal.admission.bitwise_eq(&seal.planned_admission)
        || !seal.admission.exactly_corresponds_to_with_atomic(
            seal.text_area_subtree_admission,
            seal.interactive_text_area_subtree_admission,
            seal.atomic_projection_text_area_subtree_admission.as_ref(),
            seal.focused_atomic_projection_text_area_subtree_admission
                .as_ref(),
            &seal.post_composite,
        )
        || !seal.planned_admission.exactly_corresponds_to_with_atomic(
            seal.planned_text_area_subtree_admission,
            seal.planned_interactive_text_area_subtree_admission,
            seal.planned_atomic_projection_text_area_subtree_admission
                .as_ref(),
            seal.planned_focused_atomic_projection_text_area_subtree_admission
                .as_ref(),
            &seal.planned_post_composite,
        )
        || !seal.admission.exactly_corresponds_to_resident_with_atomic(
            seal.interactive_resident.as_ref(),
            seal.atomic_projection_resident.as_ref(),
        )
        || !seal
            .planned_admission
            .exactly_corresponds_to_resident_with_atomic(
                seal.planned_interactive_resident.as_ref(),
                seal.planned_atomic_projection_resident.as_ref(),
            )
        || match (
            seal.text_area_subtree_admission,
            seal.planned_text_area_subtree_admission,
        ) {
            (None, None) => false,
            (Some(live), Some(planned)) => !live.bitwise_eq(planned),
            _ => true,
        }
        || match (
            seal.interactive_text_area_subtree_admission,
            seal.planned_interactive_text_area_subtree_admission,
        ) {
            (None, None) => false,
            (Some(live), Some(planned)) => !live.bitwise_eq(planned),
            _ => true,
        }
        || match (
            seal.atomic_projection_text_area_subtree_admission.as_ref(),
            seal.planned_atomic_projection_text_area_subtree_admission
                .as_ref(),
        ) {
            (None, None) => false,
            (Some(live), Some(planned)) => !live.bitwise_eq(planned),
            _ => true,
        }
        || match (
            seal.focused_atomic_projection_text_area_subtree_admission
                .as_ref(),
            seal.planned_focused_atomic_projection_text_area_subtree_admission
                .as_ref(),
        ) {
            (None, None) => false,
            (Some(live), Some(planned)) => !live.bitwise_eq(planned),
            _ => true,
        }
        || seal.post_composite != seal.planned_post_composite
        || seal.interactive_resident != seal.planned_interactive_resident
        || seal.atomic_projection_resident != seal.planned_atomic_projection_resident
        || seal.semantic != seal.planned_semantic
        || seal.steps_identity != seal.planned_steps_identity
        || seal.joint_transaction != seal.planned_joint_transaction
        || seal.semantic.sampled_alpha_bits != seal.scroll.scrollbar_overlay.sampled_alpha.to_bits()
        || seal.semantic.paint_state != seal.scroll.scrollbar_overlay.paint_state
        || seal.admission.boundary_root != seal.scene_root
        || seal.admission.stable_id != seal.scene_root_stable_id
        || !seal.admission.matches_scroll_node(seal.scroll)
    {
        return false;
    }
    if let [
        ScrollBoundaryStep::AtomicProjectionSelectionHostBefore {
            authority: host_authority,
            identity: host_identity,
            parent_span: host_span,
        },
        ScrollBoundaryStep::AtomicProjectionSelectionContentComposite {
            boundary,
            authority: content_authority,
            identity: content_plan_identity,
            content,
            composite,
            clip_split,
            backing,
            post_composite,
            parent_before,
            parent_after,
        },
        ScrollBoundaryStep::AtomicProjectionSelectionOverlayAfter {
            authority: overlay_authority,
            identity: overlay_identity,
            parent_span: overlay_span,
        },
    ] = plan.steps.as_slice()
    {
        let Some(host_terminal) = host_authority.host_before_opaque_order_count() else {
            return false;
        };
        let Some(content_terminal) = content_authority.content_opaque_order_count() else {
            return false;
        };
        let Some(overlay_count) = overlay_authority.overlay_opaque_order_count() else {
            return false;
        };
        let Some(parent_terminal) = host_terminal.checked_add(overlay_count) else {
            return false;
        };
        let Some(expected_span) =
            content_authority.content_artifact_span_stamp(0, 0..content_terminal)
        else {
            return false;
        };
        let Some(local_clips) = content_authority.local_clip_snapshots() else {
            return false;
        };
        let expected_content_bounds = bounds_bits(content_zero_bounds(seal.scroll));
        if *boundary != seal.boundary
            || !matches!(
                seal.admission.kind,
                PropertyScrollHostAdmissionKind::AtomicProjectionSelectionTextAreaSubtree(_)
            )
            || seal.text_area_subtree_admission.is_some()
            || seal.interactive_text_area_subtree_admission.is_some()
            || seal.atomic_projection_text_area_subtree_admission.is_some()
            || seal.interactive_resident.is_some()
            || seal.atomic_projection_resident.is_some()
            || !host_authority.same_authority(content_authority)
            || !host_authority.same_authority(overlay_authority)
            || !host_authority
                .matches_admission_geometry(bounds_bits(seal.admission.source_bounds), seal.scroll)
            || host_identity != content_plan_identity
            || host_identity != overlay_identity
            || host_identity != &host_authority.identity()
            || host_span != &(0..host_terminal)
            || *parent_before != host_terminal
            || *parent_after != host_terminal
            || overlay_span != &(host_terminal..parent_terminal)
            || !matches!(
                post_composite,
                PropertyScrollPostCompositeSchedule::NoneForExistingGrammar
            )
            || !matches!(backing, PropertyScrollBackingPlan::Single(_))
            || content.content_root != seal.admission.child
            || content.content_stable_id != seal.admission.child_stable_id
            || content.source_bounds_bits != expected_content_bounds
            || content.local_opaque_span != (0..content_terminal)
            || content.artifact_span != expected_span
            || local_clips != [host_authority.resident().contents_clip]
            || clip_split.local_raster_clips != local_clips
            || clip_split.own_contents_clip != seal.contents_clip
            || !clip_split.ancestor_composite_clips.is_empty()
            || !matches!(composite.basis, ScrollCompositeBasis::FrameRoot)
            || composite.source_bounds_bits != expected_content_bounds
            || composite.offset_bits
                != [
                    seal.scroll.offset.x.to_bits(),
                    seal.scroll.offset.y.to_bits(),
                ]
            || composite.contents_clip != seal.contents_clip.logical_scissor
            || property_scroll_step_identities(&plan.steps).as_ref() != Some(&seal.steps_identity)
            || plan_property_scroll_backing(
                seal.admission.child_stable_id,
                seal.scroll,
                seal.contents_clip,
                f32::from_bits(seal.scale_factor_bits),
                seal.target_format,
                seal.budget,
            ) != Some(backing.clone())
        {
            return false;
        }
        let joint = &seal.joint_transaction;
        return joint.roots.as_slice()
            == [PropertySceneJointRootPlanningWitness {
                ordinal: 0,
                root: seal.scene_root,
                stable_id: seal.scene_root_stable_id,
                boundary_span: 0..1,
            }]
            && joint.ordered_boundaries.as_slice() == [seal.boundary]
            && joint.generic_full_set.is_empty()
            && joint.scroll_groups.as_slice()
                == [PropertySceneScrollGroupPlanningWitness {
                    boundary: seal.boundary,
                    content: content.clone(),
                    backing: backing.clone(),
                }];
    }
    if let [
        ScrollBoundaryStep::AtomicProjectionHostBefore {
            authority: host_authority,
            identity: host_identity,
            parent_span: host_span,
        },
        ScrollBoundaryStep::AtomicProjectionContentComposite {
            boundary,
            authority: content_authority,
            identity: content_plan_identity,
            content,
            composite,
            clip_split,
            backing,
            post_composite,
            parent_before,
            parent_after,
        },
        ScrollBoundaryStep::AtomicProjectionOverlayAfter {
            authority: overlay_authority,
            identity: overlay_identity,
            parent_span: overlay_span,
        },
    ] = plan.steps.as_slice()
    {
        let atomic_admission = seal
            .admission
            .atomic_projection_text_area_subtree_snapshot();
        let focused_admission = seal
            .admission
            .focused_atomic_projection_text_area_subtree_snapshot();
        let (admission_content_wrapper, admission_text_area_root, admission_paint_grammar) =
            if let Some(admission) = atomic_admission {
                (
                    admission.content_wrapper,
                    admission.text_area_root,
                    &admission.paint_grammar,
                )
            } else if let Some(admission) = focused_admission {
                (
                    admission.content_wrapper,
                    admission.text_area_root,
                    &admission.paint_grammar.atomic_source,
                )
            } else {
                return false;
            };
        let Some(resident) = seal.atomic_projection_resident.as_ref() else {
            return false;
        };
        let Some(host_terminal) = host_authority.host_before_opaque_order_count() else {
            return false;
        };
        let Some(content_terminal) = content_authority.content_opaque_order_count() else {
            return false;
        };
        let Some(overlay_count) = overlay_authority.overlay_opaque_order_count() else {
            return false;
        };
        let Some(post_composite_delta) = post_composite.opaque_order_delta() else {
            return false;
        };
        let Some(parent_after_expected) = host_terminal.checked_add(post_composite_delta) else {
            return false;
        };
        let Some(parent_terminal) = parent_after_expected.checked_add(overlay_count) else {
            return false;
        };
        let Some(expected_span) =
            content_authority.content_artifact_span_stamp(0, 0..content_terminal)
        else {
            return false;
        };
        let Some(local_clips) = content_authority.local_clip_snapshots() else {
            return false;
        };
        let projection_sidecars_are_exact = match (atomic_admission, focused_admission) {
            (Some(admission), None) => {
                seal.atomic_projection_text_area_subtree_admission
                    .as_ref()
                    .is_some_and(|sidecar| admission.bitwise_eq(sidecar))
                    && seal
                        .focused_atomic_projection_text_area_subtree_admission
                        .is_none()
                    && matches!(
                        post_composite,
                        PropertyScrollPostCompositeSchedule::NoneForExistingGrammar
                    )
            }
            (None, Some(admission)) => {
                seal.focused_atomic_projection_text_area_subtree_admission
                    .as_ref()
                    .is_some_and(|sidecar| admission.bitwise_eq(sidecar))
                    && seal.atomic_projection_text_area_subtree_admission.is_none()
                    && matches!(
                        post_composite,
                        PropertyScrollPostCompositeSchedule::FocusedAtomicProjectionSidecars(caret)
                            if caret.is_canonical()
                                && caret.caret.owner == admission.text_area_root
                                && caret.caret.stable_id == admission.text_area_stable_id
                                && caret.outer_clip == seal.contents_clip
                    )
            }
            _ => false,
        };
        let expected_content_bounds = bounds_bits(content_zero_bounds(seal.scroll));
        if *boundary != seal.boundary
            || !projection_sidecars_are_exact
            || seal.text_area_subtree_admission.is_some()
            || seal.interactive_text_area_subtree_admission.is_some()
            || seal.interactive_resident.is_some()
            || !host_authority.same_authority(content_authority)
            || !host_authority.same_authority(overlay_authority)
            || !host_authority
                .matches_admission_geometry(bounds_bits(seal.admission.source_bounds), seal.scroll)
            || !content_authority
                .matches_admission_geometry(bounds_bits(seal.admission.source_bounds), seal.scroll)
            || !overlay_authority
                .matches_admission_geometry(bounds_bits(seal.admission.source_bounds), seal.scroll)
            || host_identity != content_plan_identity
            || host_identity != overlay_identity
            || host_identity != &host_authority.identity()
            || host_span != &(0..host_terminal)
            || *parent_before != host_terminal
            || *parent_after != parent_after_expected
            || overlay_span != &(parent_after_expected..parent_terminal)
            || !matches!(backing, PropertyScrollBackingPlan::Single(_))
            || content.content_root != seal.admission.child
            || content.content_stable_id != seal.admission.child_stable_id
            || content.source_bounds_bits != expected_content_bounds
            || content.local_opaque_span != (0..content_terminal)
            || content.artifact_span != expected_span
            || local_clips != [resident.contents_clip]
            || clip_split.local_raster_clips != local_clips
            || clip_split.own_contents_clip != seal.contents_clip
            || !clip_split.ancestor_composite_clips.is_empty()
            || !matches!(composite.basis, ScrollCompositeBasis::FrameRoot)
            || composite.source_bounds_bits != expected_content_bounds
            || composite.offset_bits
                != [
                    seal.scroll.offset.x.to_bits(),
                    seal.scroll.offset.y.to_bits(),
                ]
            || composite.contents_clip != seal.contents_clip.logical_scissor
            || admission_content_wrapper != content.content_root
            || admission_text_area_root != resident.text_area_root
            || admission_paint_grammar != &resident.source_grammar
            || property_scroll_step_identities(&plan.steps).as_ref() != Some(&seal.steps_identity)
            || plan_property_scroll_backing(
                seal.admission.child_stable_id,
                seal.scroll,
                seal.contents_clip,
                f32::from_bits(seal.scale_factor_bits),
                seal.target_format,
                seal.budget,
            ) != Some(backing.clone())
        {
            return false;
        }
        let joint = &seal.joint_transaction;
        return joint.roots.as_slice()
            == [PropertySceneJointRootPlanningWitness {
                ordinal: 0,
                root: seal.scene_root,
                stable_id: seal.scene_root_stable_id,
                boundary_span: 0..1,
            }]
            && joint.ordered_boundaries.as_slice() == [seal.boundary]
            && joint.generic_full_set.is_empty()
            && joint.scroll_groups.as_slice()
                == [PropertySceneScrollGroupPlanningWitness {
                    boundary: seal.boundary,
                    content: content.clone(),
                    backing: backing.clone(),
                }];
    }
    let [host, content, overlay] = plan.steps.as_slice() else {
        return false;
    };
    let host_bounds_bits = bounds_bits(seal.admission.source_bounds);
    let content_bounds_bits = bounds_bits(content_zero_bounds(seal.scroll));
    let (
        ScrollBoundaryStep::HostBefore {
            artifact: host_artifact,
            identity: host_identity,
            parent_span: host_span,
        },
        ScrollBoundaryStep::ContentComposite {
            boundary,
            artifact: content_artifact,
            content: content_identity,
            composite,
            clip_split,
            backing,
            post_composite,
            parent_before,
            parent_after,
        },
        ScrollBoundaryStep::OverlayAfter {
            artifact: overlay_artifact,
            identity: overlay_identity,
            parent_span: overlay_span,
        },
    ) = (host, content, overlay)
    else {
        return false;
    };
    if *boundary != seal.boundary
        || !matches!(composite.basis, ScrollCompositeBasis::FrameRoot)
        || composite.source_bounds_bits != content_bounds_bits
        || composite.offset_bits
            != [
                seal.scroll.offset.x.to_bits(),
                seal.scroll.offset.y.to_bits(),
            ]
        || composite.contents_clip != seal.contents_clip.logical_scissor
        || clip_split.local_raster_clips != content_artifact.clip_nodes
        || clip_split.own_contents_clip != seal.contents_clip
        || !clip_split.ancestor_composite_clips.is_empty()
        || content_identity.content_root != seal.admission.child
        || content_identity.content_stable_id != seal.admission.child_stable_id
        || content_identity.source_bounds_bits != content_bounds_bits
        || content_identity.local_opaque_span.start != 0
        || post_composite != &seal.post_composite
        || *parent_before != host_span.end
        || parent_before.checked_add(post_composite.opaque_order_delta().unwrap_or(u32::MAX))
            != Some(*parent_after)
        || overlay_span.start != *parent_after
        || host_span.start != 0
        || PropertyScrollPhaseArtifactIdentity::from_artifact(host_artifact).as_ref()
            != Some(host_identity)
        || PropertyScrollPhaseArtifactIdentity::from_artifact(overlay_artifact).as_ref()
            != Some(overlay_identity)
    {
        return false;
    }
    if let Some(text_area) = seal.admission.text_area_subtree_snapshot() {
        let [local_clip] = clip_split.local_raster_clips.as_slice() else {
            return false;
        };
        if text_area.boundary_root != seal.scene_root
            || text_area.content_wrapper != seal.admission.child
            || text_area.content_wrapper_stable_id != seal.admission.child_stable_id
            || text_area.stable_id != seal.scene_root_stable_id
            || !text_area.matches_scroll_node(seal.scroll)
            || local_clip.id.owner != text_area.text_area_root
            || local_clip.owner != text_area.text_area_root
            || local_clip.id.role != ClipNodeRole::ContentsClip
            || local_clip.parent.is_some()
            || local_clip.behavior != ClipBehavior::Intersect
            || local_clip.generation == 0
            || !matches!(backing, PropertyScrollBackingPlan::Single(_))
        {
            return false;
        }
    } else if let Some(text_area) = seal.admission.interactive_text_area_subtree_snapshot() {
        let [local_clip] = clip_split.local_raster_clips.as_slice() else {
            return false;
        };
        if text_area.boundary_root != seal.scene_root
            || text_area.content_wrapper != seal.admission.child
            || text_area.content_wrapper_stable_id != seal.admission.child_stable_id
            || text_area.stable_id != seal.scene_root_stable_id
            || !text_area.matches_scroll_node(seal.scroll)
            || local_clip.id.owner != text_area.text_area_root
            || local_clip.owner != text_area.text_area_root
            || local_clip.id.role != ClipNodeRole::ContentsClip
            || local_clip.parent.is_some()
            || local_clip.behavior != ClipBehavior::Intersect
            || local_clip.generation == 0
            || !matches!(backing, PropertyScrollBackingPlan::Single(_))
        {
            return false;
        }
    } else if !clip_split.local_raster_clips.is_empty() {
        return false;
    }
    let Some(_validated_host) = validate_scroll_scene_host_before_artifact(
        host_artifact.clone(),
        seal.scene_root,
        host_bounds_bits,
    ) else {
        return false;
    };
    let validated_content = if let Some(text_area) = seal.admission.text_area_subtree_snapshot() {
        let [local_clip] = content_artifact.clip_nodes.as_slice() else {
            return false;
        };
        validate_scroll_scene_text_area_content_artifact(
            content_artifact.clone(),
            seal.admission.child,
            text_area.text_area_root,
            text_area.paint_grammar,
            *local_clip,
            content_bounds_bits,
        )
    } else if let Some(text_area) = seal.admission.interactive_text_area_subtree_snapshot() {
        let [local_clip] = content_artifact.clip_nodes.as_slice() else {
            return false;
        };
        let Some(expected_resident) = seal.interactive_resident.clone() else {
            return false;
        };
        let preedit = match expected_resident {
            RetainedInteractiveTextAreaResidentRasterSeal::FocusedPreeditGlyphs(ref seal) => {
                Some(seal.clone())
            }
            _ => None,
        };
        let Some(validated) = validate_scroll_scene_interactive_text_area_content_artifact(
            content_artifact.clone(),
            seal.admission.child,
            text_area.text_area_root,
            text_area.paint_grammar,
            preedit,
            *local_clip,
            content_bounds_bits,
        ) else {
            return false;
        };
        let (content, resident) = validated.into_parts();
        if resident != expected_resident {
            return false;
        }
        Some(content)
    } else {
        validate_scroll_scene_content_artifact(
            content_artifact.clone(),
            seal.admission.child,
            content_bounds_bits,
        )
    };
    let Some(validated_content) = validated_content else {
        return false;
    };
    let Some(_validated_overlay) = validate_scroll_scene_overlay_artifact(
        overlay_artifact.clone(),
        seal.scene_root,
        seal.scroll,
        host_bounds_bits,
    ) else {
        return false;
    };
    if validated_scroll_content_artifact_span_stamp(
        &validated_content,
        0,
        content_identity.local_opaque_span.clone(),
    ) != Some(content_identity.artifact_span.clone())
        || plan_property_scroll_backing(
            seal.admission.child_stable_id,
            seal.scroll,
            seal.contents_clip,
            f32::from_bits(seal.scale_factor_bits),
            seal.target_format,
            seal.budget,
        ) != Some(backing.clone())
        || property_scroll_step_identities(&plan.steps).as_ref() != Some(&seal.steps_identity)
    {
        return false;
    }
    let joint = &seal.joint_transaction;
    joint.roots.as_slice()
        == [PropertySceneJointRootPlanningWitness {
            ordinal: 0,
            root: seal.scene_root,
            stable_id: seal.scene_root_stable_id,
            boundary_span: 0..1,
        }]
        && joint.ordered_boundaries.as_slice() == [seal.boundary]
        && joint.generic_full_set.is_empty()
        && joint.scroll_groups.as_slice()
            == [PropertySceneScrollGroupPlanningWitness {
                boundary: seal.boundary,
                content: content_identity.clone(),
                backing: backing.clone(),
            }]
}

#[allow(clippy::too_many_arguments)]
#[allow(dead_code)] // Singleton live matcher remains a focused-test oracle.
fn property_scroll_plan_matches_exact_live_inputs(
    plan: &PropertyScrollScenePlan,
    arena: &NodeArena,
    roots: &[NodeKey],
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    semantic_frame_time: crate::time::Instant,
) -> bool {
    if !plan.is_canonical()
        || semantic_frame_time != plan.seal.semantic.sampled_at
        || !paint_generations.matches_live_snapshot(arena, roots, property_trees)
        || !property_trees.validation_errors.is_empty()
        || !property_trees.transforms.is_empty()
        || !property_trees.effects.is_empty()
        || property_trees.scrolls.len() != 1
        || !matches!(property_trees.clips.len(), 1 | 2)
    {
        return false;
    }
    let [root] = roots else {
        return false;
    };
    let Some(node) = arena.get(*root) else {
        return false;
    };
    let Some(element) = node
        .element
        .as_any()
        .downcast_ref::<crate::view::base_component::Element>()
    else {
        return false;
    };
    let direct_admission = element.exact_retained_scroll_host_admission(*root, arena, 1.0);
    let text_area_subtree_admission = direct_admission
        .is_none()
        .then(|| element.exact_retained_scroll_text_area_subtree_admission(*root, arena, 1.0))
        .flatten();
    let interactive_text_area_subtree_admission = direct_admission
        .is_none()
        .then(|| {
            element.exact_retained_scroll_interactive_text_area_subtree_admission(*root, arena, 1.0)
        })
        .flatten();
    let atomic_projection_text_area_subtree_admission = direct_admission
        .is_none()
        .then(|| {
            element.exact_retained_scroll_atomic_projection_text_area_subtree_admission(
                *root, arena, 1.0,
            )
        })
        .flatten();
    let focused_atomic_projection_text_area_subtree_admission = direct_admission
        .is_none()
        .then(|| {
            element.exact_retained_scroll_focused_atomic_projection_text_area_subtree_admission(
                *root, arena, 1.0,
            )
        })
        .flatten();
    let atomic_projection_selection_text_area_subtree_admission = direct_admission
        .is_none()
        .then(|| {
            element.exact_retained_scroll_atomic_projection_selection_text_area_subtree_admission(
                *root, arena, 1.0,
            )
        })
        .flatten();
    let admission = match (
        direct_admission,
        text_area_subtree_admission,
        interactive_text_area_subtree_admission,
        atomic_projection_text_area_subtree_admission.as_ref(),
        focused_atomic_projection_text_area_subtree_admission.as_ref(),
        atomic_projection_selection_text_area_subtree_admission.as_ref(),
    ) {
        (Some(admission), None, None, None, None, None) => {
            PropertyScrollHostAdmission::direct_leaf(admission)
        }
        (None, Some(admission), None, None, None, None) => {
            PropertyScrollHostAdmission::text_area_subtree(admission)
        }
        (None, None, Some(admission), None, None, None) => {
            PropertyScrollHostAdmission::interactive_text_area_subtree(admission)
        }
        (None, None, None, Some(admission), None, None) => {
            PropertyScrollHostAdmission::atomic_projection_text_area_subtree(admission.clone())
        }
        (None, None, None, None, Some(admission), None) => {
            PropertyScrollHostAdmission::focused_atomic_projection_text_area_subtree(
                admission.clone(),
            )
        }
        (None, None, None, None, None, Some(admission)) => {
            PropertyScrollHostAdmission::atomic_projection_selection_text_area_subtree(
                admission.clone(),
            )
        }
        _ => return false,
    };
    if !admission.exactly_corresponds_to_with_atomic(
        text_area_subtree_admission,
        interactive_text_area_subtree_admission,
        atomic_projection_text_area_subtree_admission.as_ref(),
        focused_atomic_projection_text_area_subtree_admission.as_ref(),
        &plan.seal.post_composite,
    ) {
        return false;
    }
    let scroll_id = crate::view::compositor::property_tree::ScrollNodeId(*root);
    let clip_id = crate::view::compositor::property_tree::ClipNodeId {
        owner: *root,
        role: crate::view::compositor::property_tree::ClipNodeRole::ContentsClip,
    };
    let Some(scroll) = property_trees.scroll_snapshot_for(scroll_id) else {
        return false;
    };
    let Some(contents_clip) = property_trees
        .clip_snapshot_for(Some(clip_id))
        .and_then(|chain| (chain.len() == 1).then(|| chain[0]))
    else {
        return false;
    };
    let expected_contents = crate::view::compositor::property_tree::PropertyTreeState {
        clip: Some(clip_id),
        scroll: Some(scroll_id),
        ..Default::default()
    };
    let root_state = property_trees.states.get(root).copied();
    let child_state = property_trees.states.get(&admission.child).copied();
    *root == plan.seal.scene_root
        && node.element.stable_id() == plan.seal.scene_root_stable_id
        && admission.bitwise_eq(&plan.seal.admission)
        && plan.seal.admission.exactly_corresponds_to_with_atomic(
            plan.seal.text_area_subtree_admission,
            plan.seal.interactive_text_area_subtree_admission,
            plan.seal
                .atomic_projection_text_area_subtree_admission
                .as_ref(),
            plan.seal
                .focused_atomic_projection_text_area_subtree_admission
                .as_ref(),
            &plan.seal.post_composite,
        )
        && match (
            text_area_subtree_admission,
            plan.seal.text_area_subtree_admission,
        ) {
            (None, None) => true,
            (Some(live), Some(planned)) => live.bitwise_eq(planned),
            _ => false,
        }
        && match (
            interactive_text_area_subtree_admission,
            plan.seal.interactive_text_area_subtree_admission,
        ) {
            (None, None) => true,
            (Some(live), Some(planned)) => live.bitwise_eq(planned),
            _ => false,
        }
        && match (
            atomic_projection_text_area_subtree_admission.as_ref(),
            plan.seal
                .atomic_projection_text_area_subtree_admission
                .as_ref(),
        ) {
            (None, None) => true,
            (Some(live), Some(planned)) => live.bitwise_eq(planned),
            _ => false,
        }
        && match (
            focused_atomic_projection_text_area_subtree_admission.as_ref(),
            plan.seal
                .focused_atomic_projection_text_area_subtree_admission
                .as_ref(),
        ) {
            (None, None) => true,
            (Some(live), Some(planned)) => live.bitwise_eq(planned),
            _ => false,
        }
        && scroll == plan.seal.scroll
        && contents_clip == plan.seal.contents_clip
        && admission.matches_scroll_node(scroll)
        && scroll.owner == *root
        && scroll.parent.is_none()
        && scroll.generation != 0
        && scroll.has_canonical_vertical_geometry_with_contents_clip(contents_clip)
        && contents_clip.id == clip_id
        && contents_clip.owner == *root
        && contents_clip.parent.is_none()
        && contents_clip.generation != 0
        && contents_clip.behavior == crate::view::compositor::property_tree::ClipBehavior::Intersect
        && root_state.is_some_and(|state| {
            state.paint == Default::default() && state.descendants == expected_contents
        })
        && child_state.is_some_and(|state| {
            state.paint == expected_contents && state.descendants == expected_contents
        })
}

impl PropertyScrollScenePlan {
    pub(crate) fn is_canonical(&self) -> bool {
        property_scroll_plan_is_canonical(self)
    }

    #[allow(clippy::too_many_arguments)]
    #[allow(dead_code)]
    pub(crate) fn matches_live_inputs(
        &self,
        arena: &NodeArena,
        roots: &[NodeKey],
        property_trees: &PropertyTrees,
        paint_generations: &PaintGenerationTracker,
        semantic_frame_time: crate::time::Instant,
    ) -> bool {
        property_scroll_plan_matches_exact_live_inputs(
            self,
            arena,
            roots,
            property_trees,
            paint_generations,
            semantic_frame_time,
        )
    }

    #[cfg(test)]
    fn content_identity(&self) -> &PropertyScrollContentRasterIdentity {
        let ScrollBoundaryStep::ContentComposite { content, .. } = &self.steps[1] else {
            unreachable!("canonical B0 plan has one content step")
        };
        content
    }

    #[cfg(test)]
    fn composite_dependency(&self) -> PropertyScrollCompositeDependency {
        let ScrollBoundaryStep::ContentComposite { composite, .. } = &self.steps[1] else {
            unreachable!("canonical B0 plan has one content step")
        };
        *composite
    }

    #[cfg(test)]
    fn overlay_identity(&self) -> &PropertyScrollPhaseArtifactIdentity {
        let ScrollBoundaryStep::OverlayAfter { identity, .. } = &self.steps[2] else {
            unreachable!("canonical B0 plan has one overlay step")
        };
        identity
    }

    #[cfg(test)]
    pub(crate) fn atomic_projection_contract_for_test(&self) -> bool {
        let [
            ScrollBoundaryStep::AtomicProjectionHostBefore {
                authority: host,
                parent_span: host_span,
                ..
            },
            ScrollBoundaryStep::AtomicProjectionContentComposite {
                authority: content,
                backing,
                post_composite,
                parent_before,
                parent_after,
                ..
            },
            ScrollBoundaryStep::AtomicProjectionOverlayAfter {
                authority: overlay,
                parent_span: overlay_span,
                ..
            },
        ] = self.steps.as_slice()
        else {
            return false;
        };
        self.is_canonical()
            && host.same_authority(content)
            && host.same_authority(overlay)
            && host.chunk_counts_for_test() == (1, 5, 1)
            && matches!(backing, PropertyScrollBackingPlan::Single(_))
            && matches!(
                post_composite,
                PropertyScrollPostCompositeSchedule::NoneForExistingGrammar
            )
            && *parent_before == host_span.end
            && parent_before == parent_after
            && overlay_span.start == *parent_after
            && self.seal.text_area_subtree_admission.is_none()
            && self.seal.interactive_text_area_subtree_admission.is_none()
            && self
                .seal
                .atomic_projection_text_area_subtree_admission
                .is_some()
            && self.seal.interactive_resident.is_none()
            && self.seal.atomic_projection_resident.is_some()
    }

    #[cfg(test)]
    pub(crate) fn atomic_projection_tamper_matrix_for_test(&self) -> bool {
        if !self.atomic_projection_contract_for_test() {
            return false;
        }
        let mut missing_sidecar = self.clone();
        missing_sidecar
            .seal
            .planned_atomic_projection_text_area_subtree_admission = None;
        let mut drifted_sidecar = self.clone();
        let Some(sidecar) = drifted_sidecar
            .seal
            .atomic_projection_text_area_subtree_admission
            .as_mut()
        else {
            return false;
        };
        sidecar.paint_grammar.projection_text_stable_id ^= 1;
        let mut reordered = self.clone();
        reordered.steps.swap(0, 2);
        let mut geometry = self.clone();
        geometry.seal.admission.source_bounds.x += 1.0;
        geometry.seal.planned_admission.source_bounds.x += 1.0;
        let PropertyScrollHostAdmissionKind::AtomicProjectionTextAreaSubtree(live_kind) =
            &mut geometry.seal.admission.kind
        else {
            return false;
        };
        live_kind.source_bounds.x += 1.0;
        let PropertyScrollHostAdmissionKind::AtomicProjectionTextAreaSubtree(planned_kind) =
            &mut geometry.seal.planned_admission.kind
        else {
            return false;
        };
        planned_kind.source_bounds.x += 1.0;
        let (Some(live_sidecar), Some(planned_sidecar)) = (
            geometry
                .seal
                .atomic_projection_text_area_subtree_admission
                .as_mut(),
            geometry
                .seal
                .planned_atomic_projection_text_area_subtree_admission
                .as_mut(),
        ) else {
            return false;
        };
        live_sidecar.source_bounds.x += 1.0;
        planned_sidecar.source_bounds.x += 1.0;
        let mut cursor = self.clone();
        let ScrollBoundaryStep::AtomicProjectionContentComposite { parent_after, .. } =
            &mut cursor.steps[1]
        else {
            return false;
        };
        *parent_after = parent_after.saturating_add(1);
        let mut synchronized_source_grammar = self.clone();
        let PropertyScrollHostAdmissionKind::AtomicProjectionTextAreaSubtree(live_kind) =
            &mut synchronized_source_grammar.seal.admission.kind
        else {
            return false;
        };
        live_kind.paint_grammar.projection_text_stable_id ^= 1;
        let PropertyScrollHostAdmissionKind::AtomicProjectionTextAreaSubtree(planned_kind) =
            &mut synchronized_source_grammar.seal.planned_admission.kind
        else {
            return false;
        };
        planned_kind.paint_grammar.projection_text_stable_id ^= 1;
        let (Some(live_sidecar), Some(planned_sidecar)) = (
            synchronized_source_grammar
                .seal
                .atomic_projection_text_area_subtree_admission
                .as_mut(),
            synchronized_source_grammar
                .seal
                .planned_atomic_projection_text_area_subtree_admission
                .as_mut(),
        ) else {
            return false;
        };
        live_sidecar.paint_grammar.projection_text_stable_id ^= 1;
        planned_sidecar.paint_grammar.projection_text_stable_id ^= 1;
        let (Some(live_resident), Some(planned_resident)) = (
            synchronized_source_grammar
                .seal
                .atomic_projection_resident
                .as_mut(),
            synchronized_source_grammar
                .seal
                .planned_atomic_projection_resident
                .as_mut(),
        ) else {
            return false;
        };
        live_resident.source_grammar.projection_text_stable_id ^= 1;
        planned_resident.source_grammar.projection_text_stable_id ^= 1;
        !missing_sidecar.is_canonical()
            && !drifted_sidecar.is_canonical()
            && !reordered.is_canonical()
            && !geometry.is_canonical()
            && !cursor.is_canonical()
            && !synchronized_source_grammar.is_canonical()
    }
}

#[allow(clippy::too_many_arguments)]
#[allow(dead_code)] // Superseded in production by the atomic forest constructor.
pub(crate) fn plan_property_scroll_scene_scaffold(
    arena: &NodeArena,
    roots: &[NodeKey],
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    scale_factor: f32,
    incoming_paint_offset: [f32; 2],
    outer_scissor_rect: Option<[u32; 4]>,
    semantic_frame_time: crate::time::Instant,
    target_format: wgpu::TextureFormat,
    budget: ScrollSceneSingleTextureBudget,
) -> Result<PropertyScrollScenePlan, PropertyScrollScenePlanError> {
    if scale_factor.to_bits() != 1.0_f32.to_bits()
        || incoming_paint_offset.map(f32::to_bits) != [0.0_f32.to_bits(); 2]
        || outer_scissor_rect.is_some()
        || roots.len() != 1
    {
        return Err(PropertyScrollScenePlanError::InvalidContract);
    }
    if !paint_generations.matches_live_snapshot(arena, roots, property_trees) {
        return Err(PropertyScrollScenePlanError::LiveSnapshotDrift);
    }
    let scene = plan_single_root_scroll_scene(
        arena,
        roots,
        property_trees,
        paint_generations,
        scale_factor,
        incoming_paint_offset,
        outer_scissor_rect,
    )
    .map_err(PropertyScrollScenePlanError::Frame)?;
    if !paint_generations.matches_live_snapshot(arena, roots, property_trees) {
        return Err(PropertyScrollScenePlanError::LiveSnapshotDrift);
    }
    property_scroll_plan_from_exact_scene(
        scene,
        scale_factor,
        semantic_frame_time,
        target_format,
        budget,
    )
}

fn property_scroll_plan_from_exact_scene(
    scene: ScrollScenePlan,
    scale_factor: f32,
    semantic_frame_time: crate::time::Instant,
    target_format: wgpu::TextureFormat,
    budget: ScrollSceneSingleTextureBudget,
) -> Result<PropertyScrollScenePlan, PropertyScrollScenePlanError> {
    if !scale_factor.is_finite() || scale_factor <= 0.0 {
        return Err(PropertyScrollScenePlanError::InvalidContract);
    }
    let ScrollScenePlan {
        boundary_root,
        root_stable_id,
        content_root,
        content_stable_id,
        admission,
        text_area_subtree_admission,
        interactive_text_area_subtree_admission,
        atomic_projection_text_area_subtree_admission,
        focused_atomic_projection_text_area_subtree_admission,
        post_composite,
        interactive_resident,
        atomic_projection_resident,
        scroll,
        contents_clip,
        planned_admission_witness,
        planned_text_area_subtree_admission,
        planned_interactive_text_area_subtree_admission,
        planned_atomic_projection_text_area_subtree_admission,
        planned_focused_atomic_projection_text_area_subtree_admission,
        planned_post_composite,
        planned_interactive_resident,
        planned_atomic_projection_resident,
        planned_scroll_witness,
        planned_clip_witness,
        recorded,
    } = scene;
    if !admission.exactly_corresponds_to_with_atomic(
        text_area_subtree_admission,
        interactive_text_area_subtree_admission,
        atomic_projection_text_area_subtree_admission.as_ref(),
        focused_atomic_projection_text_area_subtree_admission.as_ref(),
        &post_composite,
    ) || !planned_admission_witness.exactly_corresponds_to_with_atomic(
        planned_text_area_subtree_admission,
        planned_interactive_text_area_subtree_admission,
        planned_atomic_projection_text_area_subtree_admission.as_ref(),
        planned_focused_atomic_projection_text_area_subtree_admission.as_ref(),
        &planned_post_composite,
    ) || !admission.exactly_corresponds_to_resident_with_atomic(
        interactive_resident.as_ref(),
        atomic_projection_resident.as_ref(),
    ) || !planned_admission_witness.exactly_corresponds_to_resident_with_atomic(
        planned_interactive_resident.as_ref(),
        planned_atomic_projection_resident.as_ref(),
    ) || !admission.bitwise_eq(&planned_admission_witness)
        || !text_area_subtree_admission
            .zip(planned_text_area_subtree_admission)
            .is_none_or(|(live, planned)| live.bitwise_eq(planned))
        || text_area_subtree_admission.is_some() != planned_text_area_subtree_admission.is_some()
        || !interactive_text_area_subtree_admission
            .zip(planned_interactive_text_area_subtree_admission)
            .is_none_or(|(live, planned)| live.bitwise_eq(planned))
        || interactive_text_area_subtree_admission.is_some()
            != planned_interactive_text_area_subtree_admission.is_some()
        || !atomic_projection_text_area_subtree_admission
            .as_ref()
            .zip(planned_atomic_projection_text_area_subtree_admission.as_ref())
            .is_none_or(|(live, planned)| live.bitwise_eq(planned))
        || atomic_projection_text_area_subtree_admission.is_some()
            != planned_atomic_projection_text_area_subtree_admission.is_some()
        || post_composite != planned_post_composite
        || interactive_resident != planned_interactive_resident
        || atomic_projection_resident != planned_atomic_projection_resident
        || scroll != planned_scroll_witness
        || contents_clip != planned_clip_witness
    {
        return Err(PropertyScrollScenePlanError::InvalidContract);
    }
    if let ScrollSceneRecordedAuthority::AtomicProjectionSelectionTextArea(parts) = recorded {
        if !parts.is_canonical()
            || parts.boundary_root() != boundary_root
            || parts.content_root() != content_root
            || parts.outer_scroll() != scroll
            || parts.outer_contents_clip() != contents_clip
            || !parts.matches_admission_geometry(bounds_bits(admission.source_bounds), scroll)
            || !matches!(
                post_composite,
                PropertyScrollPostCompositeSchedule::NoneForExistingGrammar
            )
            || interactive_resident.is_some()
            || atomic_projection_resident.is_some()
            || !matches!(
                admission.kind,
                PropertyScrollHostAdmissionKind::AtomicProjectionSelectionTextAreaSubtree(_)
            )
            || text_area_subtree_admission.is_some()
            || interactive_text_area_subtree_admission.is_some()
            || atomic_projection_text_area_subtree_admission.is_some()
        {
            return Err(PropertyScrollScenePlanError::InvalidContract);
        }
        let host_terminal = parts
            .host_before_opaque_order_count()
            .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        let content_terminal = parts
            .content_opaque_order_count()
            .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        let overlay_count = parts
            .overlay_opaque_order_count()
            .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        let content_local_span = 0..content_terminal;
        let artifact_span = parts
            .content_artifact_span_stamp(0, content_local_span.clone())
            .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        let content_bounds_bits = bounds_bits(content_zero_bounds(scroll));
        let content_identity = PropertyScrollContentRasterIdentity {
            content_root,
            content_stable_id,
            source_bounds_bits: content_bounds_bits,
            artifact_span,
            local_opaque_span: content_local_span,
        };
        let budget = property_scroll_budget(budget);
        let backing = plan_property_scroll_backing(
            content_stable_id,
            scroll,
            contents_clip,
            scale_factor,
            target_format,
            budget,
        )
        .ok_or(PropertyScrollScenePlanError::BackingBudget)?;
        if !matches!(backing, PropertyScrollBackingPlan::Single(_)) {
            return Err(PropertyScrollScenePlanError::BackingBudget);
        }
        let local_clips = parts
            .local_clip_snapshots()
            .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        if local_clips != [parts.resident().contents_clip] {
            return Err(PropertyScrollScenePlanError::InvalidContract);
        }
        let boundary = SceneBoundaryId {
            ordinal: 0,
            owner: boundary_root,
            kind: SceneBoundaryKind::ScrollContents,
        };
        let composite = PropertyScrollCompositeDependency {
            basis: ScrollCompositeBasis::FrameRoot,
            source_bounds_bits: content_bounds_bits,
            offset_bits: [scroll.offset.x.to_bits(), scroll.offset.y.to_bits()],
            contents_clip: contents_clip.logical_scissor,
        };
        let clip_split = PropertyScrollClipSplitWitness {
            local_raster_clips: local_clips.to_vec(),
            own_contents_clip: contents_clip,
            ancestor_composite_clips: Vec::new(),
        };
        let semantic = PropertyScrollSemanticFrameWitness {
            sampled_at: semantic_frame_time,
            sampled_alpha_bits: scroll.scrollbar_overlay.sampled_alpha.to_bits(),
            paint_state: scroll.scrollbar_overlay.paint_state,
        };
        let identity = parts.identity();
        let authority = Arc::new(parts);
        let parent_terminal = host_terminal
            .checked_add(overlay_count)
            .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        let steps = vec![
            ScrollBoundaryStep::AtomicProjectionSelectionHostBefore {
                authority: Arc::clone(&authority),
                identity: identity.clone(),
                parent_span: 0..host_terminal,
            },
            ScrollBoundaryStep::AtomicProjectionSelectionContentComposite {
                boundary,
                authority: Arc::clone(&authority),
                identity: identity.clone(),
                content: content_identity.clone(),
                composite,
                clip_split,
                backing: backing.clone(),
                post_composite: PropertyScrollPostCompositeSchedule::NoneForExistingGrammar,
                parent_before: host_terminal,
                parent_after: host_terminal,
            },
            ScrollBoundaryStep::AtomicProjectionSelectionOverlayAfter {
                authority,
                identity,
                parent_span: host_terminal..parent_terminal,
            },
        ];
        let steps_identity = property_scroll_step_identities(&steps)
            .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        let joint_transaction = PropertySceneJointTransactionPlanningWitness {
            roots: vec![PropertySceneJointRootPlanningWitness {
                ordinal: 0,
                root: boundary_root,
                stable_id: root_stable_id,
                boundary_span: 0..1,
            }],
            ordered_boundaries: vec![boundary],
            generic_full_set: Vec::new(),
            scroll_groups: vec![PropertySceneScrollGroupPlanningWitness {
                boundary,
                content: content_identity,
                backing,
            }],
        };
        let seal = PropertyScrollScenePlanSeal {
            scene_root: boundary_root,
            scene_root_stable_id: root_stable_id,
            boundary,
            scroll,
            contents_clip,
            admission: admission.clone(),
            text_area_subtree_admission: None,
            interactive_text_area_subtree_admission: None,
            atomic_projection_text_area_subtree_admission: None,
            focused_atomic_projection_text_area_subtree_admission: None,
            post_composite: post_composite.clone(),
            interactive_resident: None,
            atomic_projection_resident: None,
            semantic,
            steps_identity: steps_identity.clone(),
            joint_transaction: joint_transaction.clone(),
            planned_scroll: scroll,
            planned_contents_clip: contents_clip,
            planned_admission: planned_admission_witness,
            planned_text_area_subtree_admission: None,
            planned_interactive_text_area_subtree_admission: None,
            planned_atomic_projection_text_area_subtree_admission: None,
            planned_focused_atomic_projection_text_area_subtree_admission: None,
            planned_post_composite,
            planned_interactive_resident: None,
            planned_atomic_projection_resident: None,
            planned_semantic: semantic,
            planned_steps_identity: steps_identity,
            planned_joint_transaction: joint_transaction,
            scale_factor_bits: scale_factor.to_bits(),
            target_format,
            budget,
        };
        let plan = PropertyScrollScenePlan { steps, seal };
        return plan
            .is_canonical()
            .then_some(plan)
            .ok_or(PropertyScrollScenePlanError::InvalidContract);
    }
    if let ScrollSceneRecordedAuthority::FocusedAtomicProjectionTextArea(parts) = recorded {
        if !parts.is_canonical()
            || parts.boundary_root() != boundary_root
            || parts.content_root() != content_root
            || parts.outer_scroll() != scroll
            || parts.outer_contents_clip() != contents_clip
            || !parts.matches_admission_geometry(bounds_bits(admission.source_bounds), scroll)
            || !matches!(
                post_composite,
                PropertyScrollPostCompositeSchedule::FocusedAtomicProjectionSidecars(_)
            )
            || interactive_resident.is_some()
        {
            return Err(PropertyScrollScenePlanError::InvalidContract);
        }
        let Some(focused_admission) =
            focused_atomic_projection_text_area_subtree_admission.as_ref()
        else {
            return Err(PropertyScrollScenePlanError::InvalidContract);
        };
        let Some(resident) = atomic_projection_resident.as_ref() else {
            return Err(PropertyScrollScenePlanError::InvalidContract);
        };
        if parts.text_area_root() != focused_admission.text_area_root
            || parts.resident() != resident
        {
            return Err(PropertyScrollScenePlanError::InvalidContract);
        }
        let base_parts = parts.atomic_projection_base_for_scene_steps();
        let host_terminal = base_parts
            .host_before_opaque_order_count()
            .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        let content_terminal = base_parts
            .content_opaque_order_count()
            .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        let overlay_count = base_parts
            .overlay_opaque_order_count()
            .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        let post_composite_delta = post_composite
            .opaque_order_delta()
            .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        let content_local_span = 0..content_terminal;
        let artifact_span = base_parts
            .content_artifact_span_stamp(0, content_local_span.clone())
            .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        let content_bounds_bits = bounds_bits(content_zero_bounds(scroll));
        let content_identity = PropertyScrollContentRasterIdentity {
            content_root,
            content_stable_id,
            source_bounds_bits: content_bounds_bits,
            artifact_span,
            local_opaque_span: content_local_span,
        };
        let budget = property_scroll_budget(budget);
        let backing = plan_property_scroll_backing(
            content_stable_id,
            scroll,
            contents_clip,
            scale_factor,
            target_format,
            budget,
        )
        .ok_or(PropertyScrollScenePlanError::BackingBudget)?;
        if !matches!(backing, PropertyScrollBackingPlan::Single(_)) {
            return Err(PropertyScrollScenePlanError::BackingBudget);
        }
        let local_clips = base_parts
            .local_clip_snapshots()
            .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        if local_clips != [resident.contents_clip] {
            return Err(PropertyScrollScenePlanError::InvalidContract);
        }
        let boundary = SceneBoundaryId {
            ordinal: 0,
            owner: boundary_root,
            kind: SceneBoundaryKind::ScrollContents,
        };
        let composite = PropertyScrollCompositeDependency {
            basis: ScrollCompositeBasis::FrameRoot,
            source_bounds_bits: content_bounds_bits,
            offset_bits: [scroll.offset.x.to_bits(), scroll.offset.y.to_bits()],
            contents_clip: contents_clip.logical_scissor,
        };
        let clip_split = PropertyScrollClipSplitWitness {
            local_raster_clips: local_clips.to_vec(),
            own_contents_clip: contents_clip,
            ancestor_composite_clips: Vec::new(),
        };
        let semantic = PropertyScrollSemanticFrameWitness {
            sampled_at: semantic_frame_time,
            sampled_alpha_bits: scroll.scrollbar_overlay.sampled_alpha.to_bits(),
            paint_state: scroll.scrollbar_overlay.paint_state,
        };
        let identity = base_parts.identity();
        let authority = Arc::new(base_parts);
        let overlay_start = host_terminal
            .checked_add(post_composite_delta)
            .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        let parent_terminal = overlay_start
            .checked_add(overlay_count)
            .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        let steps = vec![
            ScrollBoundaryStep::AtomicProjectionHostBefore {
                authority: Arc::clone(&authority),
                identity: identity.clone(),
                parent_span: 0..host_terminal,
            },
            ScrollBoundaryStep::AtomicProjectionContentComposite {
                boundary,
                authority: Arc::clone(&authority),
                identity: identity.clone(),
                content: content_identity.clone(),
                composite,
                clip_split,
                backing: backing.clone(),
                post_composite: post_composite.clone(),
                parent_before: host_terminal,
                parent_after: overlay_start,
            },
            ScrollBoundaryStep::AtomicProjectionOverlayAfter {
                authority,
                identity,
                parent_span: overlay_start..parent_terminal,
            },
        ];
        let steps_identity = property_scroll_step_identities(&steps)
            .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        let joint_transaction = PropertySceneJointTransactionPlanningWitness {
            roots: vec![PropertySceneJointRootPlanningWitness {
                ordinal: 0,
                root: boundary_root,
                stable_id: root_stable_id,
                boundary_span: 0..1,
            }],
            ordered_boundaries: vec![boundary],
            generic_full_set: Vec::new(),
            scroll_groups: vec![PropertySceneScrollGroupPlanningWitness {
                boundary,
                content: content_identity,
                backing,
            }],
        };
        let seal = PropertyScrollScenePlanSeal {
            scene_root: boundary_root,
            scene_root_stable_id: root_stable_id,
            boundary,
            scroll,
            contents_clip,
            admission: admission.clone(),
            text_area_subtree_admission: None,
            interactive_text_area_subtree_admission: None,
            atomic_projection_text_area_subtree_admission: None,
            focused_atomic_projection_text_area_subtree_admission:
                focused_atomic_projection_text_area_subtree_admission.clone(),
            post_composite: post_composite.clone(),
            interactive_resident: None,
            atomic_projection_resident: atomic_projection_resident.clone(),
            semantic,
            steps_identity: steps_identity.clone(),
            joint_transaction: joint_transaction.clone(),
            planned_scroll: scroll,
            planned_contents_clip: contents_clip,
            planned_admission: planned_admission_witness,
            planned_text_area_subtree_admission: None,
            planned_interactive_text_area_subtree_admission: None,
            planned_atomic_projection_text_area_subtree_admission: None,
            planned_focused_atomic_projection_text_area_subtree_admission,
            planned_post_composite,
            planned_interactive_resident: None,
            planned_atomic_projection_resident,
            planned_semantic: semantic,
            planned_steps_identity: steps_identity,
            planned_joint_transaction: joint_transaction,
            scale_factor_bits: scale_factor.to_bits(),
            target_format,
            budget,
        };
        let plan = PropertyScrollScenePlan { steps, seal };
        return plan
            .is_canonical()
            .then_some(plan)
            .ok_or(PropertyScrollScenePlanError::InvalidContract);
    }
    if let ScrollSceneRecordedAuthority::AtomicProjectionTextArea(parts) = recorded {
        if !parts.is_canonical()
            || parts.boundary_root() != boundary_root
            || parts.content_root() != content_root
            || parts.outer_scroll() != scroll
            || parts.outer_contents_clip() != contents_clip
            || !parts.matches_admission_geometry(bounds_bits(admission.source_bounds), scroll)
            || !matches!(
                post_composite,
                PropertyScrollPostCompositeSchedule::NoneForExistingGrammar
            )
            || interactive_resident.is_some()
        {
            return Err(PropertyScrollScenePlanError::InvalidContract);
        }
        let Some(atomic_admission) = atomic_projection_text_area_subtree_admission.as_ref() else {
            return Err(PropertyScrollScenePlanError::InvalidContract);
        };
        let Some(resident) = atomic_projection_resident.as_ref() else {
            return Err(PropertyScrollScenePlanError::InvalidContract);
        };
        if parts.text_area_root() != atomic_admission.text_area_root || parts.resident() != resident
        {
            return Err(PropertyScrollScenePlanError::InvalidContract);
        }
        let host_terminal = parts
            .host_before_opaque_order_count()
            .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        let content_terminal = parts
            .content_opaque_order_count()
            .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        let overlay_count = parts
            .overlay_opaque_order_count()
            .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        let content_local_span = 0..content_terminal;
        let artifact_span = parts
            .content_artifact_span_stamp(0, content_local_span.clone())
            .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        let content_bounds_bits = bounds_bits(content_zero_bounds(scroll));
        let content_identity = PropertyScrollContentRasterIdentity {
            content_root,
            content_stable_id,
            source_bounds_bits: content_bounds_bits,
            artifact_span,
            local_opaque_span: content_local_span,
        };
        let budget = property_scroll_budget(budget);
        let backing = plan_property_scroll_backing(
            content_stable_id,
            scroll,
            contents_clip,
            scale_factor,
            target_format,
            budget,
        )
        .ok_or(PropertyScrollScenePlanError::BackingBudget)?;
        if !matches!(backing, PropertyScrollBackingPlan::Single(_)) {
            return Err(PropertyScrollScenePlanError::BackingBudget);
        }
        let local_clips = parts
            .local_clip_snapshots()
            .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        if local_clips != [resident.contents_clip] {
            return Err(PropertyScrollScenePlanError::InvalidContract);
        }
        let boundary = SceneBoundaryId {
            ordinal: 0,
            owner: boundary_root,
            kind: SceneBoundaryKind::ScrollContents,
        };
        let composite = PropertyScrollCompositeDependency {
            basis: ScrollCompositeBasis::FrameRoot,
            source_bounds_bits: content_bounds_bits,
            offset_bits: [scroll.offset.x.to_bits(), scroll.offset.y.to_bits()],
            contents_clip: contents_clip.logical_scissor,
        };
        let clip_split = PropertyScrollClipSplitWitness {
            local_raster_clips: local_clips.to_vec(),
            own_contents_clip: contents_clip,
            ancestor_composite_clips: Vec::new(),
        };
        let semantic = PropertyScrollSemanticFrameWitness {
            sampled_at: semantic_frame_time,
            sampled_alpha_bits: scroll.scrollbar_overlay.sampled_alpha.to_bits(),
            paint_state: scroll.scrollbar_overlay.paint_state,
        };
        let identity = parts.identity();
        let authority = Arc::new(parts);
        let parent_terminal = host_terminal
            .checked_add(overlay_count)
            .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        let steps = vec![
            ScrollBoundaryStep::AtomicProjectionHostBefore {
                authority: Arc::clone(&authority),
                identity: identity.clone(),
                parent_span: 0..host_terminal,
            },
            ScrollBoundaryStep::AtomicProjectionContentComposite {
                boundary,
                authority: Arc::clone(&authority),
                identity: identity.clone(),
                content: content_identity.clone(),
                composite,
                clip_split,
                backing: backing.clone(),
                post_composite: PropertyScrollPostCompositeSchedule::NoneForExistingGrammar,
                parent_before: host_terminal,
                parent_after: host_terminal,
            },
            ScrollBoundaryStep::AtomicProjectionOverlayAfter {
                authority,
                identity,
                parent_span: host_terminal..parent_terminal,
            },
        ];
        let steps_identity = property_scroll_step_identities(&steps)
            .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        let joint_transaction = PropertySceneJointTransactionPlanningWitness {
            roots: vec![PropertySceneJointRootPlanningWitness {
                ordinal: 0,
                root: boundary_root,
                stable_id: root_stable_id,
                boundary_span: 0..1,
            }],
            ordered_boundaries: vec![boundary],
            generic_full_set: Vec::new(),
            scroll_groups: vec![PropertySceneScrollGroupPlanningWitness {
                boundary,
                content: content_identity,
                backing,
            }],
        };
        let seal = PropertyScrollScenePlanSeal {
            scene_root: boundary_root,
            scene_root_stable_id: root_stable_id,
            boundary,
            scroll,
            contents_clip,
            admission: admission.clone(),
            text_area_subtree_admission,
            interactive_text_area_subtree_admission,
            atomic_projection_text_area_subtree_admission:
                atomic_projection_text_area_subtree_admission.clone(),
            focused_atomic_projection_text_area_subtree_admission:
                focused_atomic_projection_text_area_subtree_admission.clone(),
            post_composite: post_composite.clone(),
            interactive_resident: None,
            atomic_projection_resident: atomic_projection_resident.clone(),
            semantic,
            steps_identity: steps_identity.clone(),
            joint_transaction: joint_transaction.clone(),
            planned_scroll: scroll,
            planned_contents_clip: contents_clip,
            planned_admission: planned_admission_witness,
            planned_text_area_subtree_admission,
            planned_interactive_text_area_subtree_admission,
            planned_atomic_projection_text_area_subtree_admission,
            planned_focused_atomic_projection_text_area_subtree_admission,
            planned_post_composite,
            planned_interactive_resident: None,
            planned_atomic_projection_resident,
            planned_semantic: semantic,
            planned_steps_identity: steps_identity,
            planned_joint_transaction: joint_transaction,
            scale_factor_bits: scale_factor.to_bits(),
            target_format,
            budget,
        };
        let plan = PropertyScrollScenePlan { steps, seal };
        return plan
            .is_canonical()
            .then_some(plan)
            .ok_or(PropertyScrollScenePlanError::InvalidContract);
    }
    let ScrollSceneRecordedAuthority::Existing {
        host_before,
        content_local,
        overlay,
        host_parent_span,
        content_local_span,
        overlay_parent_span,
    } = recorded
    else {
        unreachable!("typed atomic authority returned above")
    };
    let host_bounds_bits = bounds_bits(admission.source_bounds);
    let content_bounds_bits = bounds_bits(content_zero_bounds(scroll));
    let validated_host = validate_scroll_scene_host_before_artifact(
        host_before.clone(),
        boundary_root,
        host_bounds_bits,
    )
    .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
    let validated_content = if let Some(text_area_admission) =
        admission.text_area_subtree_snapshot()
    {
        let [local_clip] = content_local.clip_nodes.as_slice() else {
            return Err(PropertyScrollScenePlanError::InvalidContract);
        };
        validate_scroll_scene_text_area_content_artifact(
            content_local.clone(),
            content_root,
            text_area_admission.text_area_root,
            text_area_admission.paint_grammar,
            *local_clip,
            content_bounds_bits,
        )
    } else if let Some(text_area_admission) = admission.interactive_text_area_subtree_snapshot() {
        let [local_clip] = content_local.clip_nodes.as_slice() else {
            return Err(PropertyScrollScenePlanError::InvalidContract);
        };
        let preedit = match interactive_resident.as_ref() {
            Some(RetainedInteractiveTextAreaResidentRasterSeal::FocusedPreeditGlyphs(seal)) => {
                Some(seal.clone())
            }
            _ => None,
        };
        let validated = validate_scroll_scene_interactive_text_area_content_artifact(
            content_local.clone(),
            content_root,
            text_area_admission.text_area_root,
            text_area_admission.paint_grammar,
            preedit,
            *local_clip,
            content_bounds_bits,
        )
        .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        let (content, resident) = validated.into_parts();
        (Some(&resident) == interactive_resident.as_ref()).then_some(content)
    } else {
        validate_scroll_scene_content_artifact(
            content_local.clone(),
            content_root,
            content_bounds_bits,
        )
    }
    .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
    let validated_overlay = validate_scroll_scene_overlay_artifact(
        overlay.clone(),
        boundary_root,
        scroll,
        host_bounds_bits,
    )
    .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
    let _ = (validated_host, validated_overlay);
    let artifact_span = validated_scroll_content_artifact_span_stamp(
        &validated_content,
        0,
        content_local_span.clone(),
    )
    .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
    let content_identity = PropertyScrollContentRasterIdentity {
        content_root,
        content_stable_id,
        source_bounds_bits: content_bounds_bits,
        artifact_span,
        local_opaque_span: content_local_span,
    };
    let budget = property_scroll_budget(budget);
    let backing = plan_property_scroll_backing(
        content_stable_id,
        scroll,
        contents_clip,
        scale_factor,
        target_format,
        budget,
    )
    .ok_or(PropertyScrollScenePlanError::BackingBudget)?;
    if (admission.text_area_subtree_snapshot().is_some()
        || admission.interactive_text_area_subtree_snapshot().is_some()
        || admission
            .atomic_projection_text_area_subtree_snapshot()
            .is_some())
        && !matches!(backing, PropertyScrollBackingPlan::Single(_))
    {
        return Err(PropertyScrollScenePlanError::BackingBudget);
    }
    let boundary = SceneBoundaryId {
        ordinal: 0,
        owner: boundary_root,
        kind: SceneBoundaryKind::ScrollContents,
    };
    let host_identity = PropertyScrollPhaseArtifactIdentity::from_artifact(&host_before)
        .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
    let overlay_identity = PropertyScrollPhaseArtifactIdentity::from_artifact(&overlay)
        .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
    let composite = PropertyScrollCompositeDependency {
        basis: ScrollCompositeBasis::FrameRoot,
        source_bounds_bits: content_bounds_bits,
        offset_bits: [scroll.offset.x.to_bits(), scroll.offset.y.to_bits()],
        contents_clip: contents_clip.logical_scissor,
    };
    let clip_split = PropertyScrollClipSplitWitness {
        local_raster_clips: content_local.clip_nodes.clone(),
        own_contents_clip: contents_clip,
        ancestor_composite_clips: Vec::new(),
    };
    let semantic = PropertyScrollSemanticFrameWitness {
        sampled_at: semantic_frame_time,
        sampled_alpha_bits: scroll.scrollbar_overlay.sampled_alpha.to_bits(),
        paint_state: scroll.scrollbar_overlay.paint_state,
    };
    let steps = vec![
        ScrollBoundaryStep::HostBefore {
            artifact: host_before,
            identity: host_identity,
            parent_span: host_parent_span.clone(),
        },
        ScrollBoundaryStep::ContentComposite {
            boundary,
            artifact: content_local,
            content: content_identity.clone(),
            composite,
            clip_split,
            backing: backing.clone(),
            post_composite: post_composite.clone(),
            parent_before: host_parent_span.end,
            parent_after: overlay_parent_span.start,
        },
        ScrollBoundaryStep::OverlayAfter {
            artifact: overlay,
            identity: overlay_identity,
            parent_span: overlay_parent_span,
        },
    ];
    let steps_identity = property_scroll_step_identities(&steps)
        .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
    let joint_transaction = PropertySceneJointTransactionPlanningWitness {
        roots: vec![PropertySceneJointRootPlanningWitness {
            ordinal: 0,
            root: boundary_root,
            stable_id: root_stable_id,
            boundary_span: 0..1,
        }],
        ordered_boundaries: vec![boundary],
        generic_full_set: Vec::new(),
        scroll_groups: vec![PropertySceneScrollGroupPlanningWitness {
            boundary,
            content: content_identity,
            backing,
        }],
    };
    let seal = PropertyScrollScenePlanSeal {
        scene_root: boundary_root,
        scene_root_stable_id: root_stable_id,
        boundary,
        scroll,
        contents_clip,
        admission: admission.clone(),
        text_area_subtree_admission,
        interactive_text_area_subtree_admission,
        atomic_projection_text_area_subtree_admission:
            atomic_projection_text_area_subtree_admission.clone(),
        focused_atomic_projection_text_area_subtree_admission:
            focused_atomic_projection_text_area_subtree_admission.clone(),
        post_composite: post_composite.clone(),
        interactive_resident: interactive_resident.clone(),
        atomic_projection_resident: atomic_projection_resident.clone(),
        semantic,
        steps_identity: steps_identity.clone(),
        joint_transaction: joint_transaction.clone(),
        planned_scroll: scroll,
        planned_contents_clip: contents_clip,
        planned_admission: admission,
        planned_text_area_subtree_admission: text_area_subtree_admission,
        planned_interactive_text_area_subtree_admission: interactive_text_area_subtree_admission,
        planned_atomic_projection_text_area_subtree_admission:
            atomic_projection_text_area_subtree_admission,
        planned_focused_atomic_projection_text_area_subtree_admission:
            focused_atomic_projection_text_area_subtree_admission,
        planned_post_composite: post_composite,
        planned_interactive_resident: interactive_resident,
        planned_atomic_projection_resident: atomic_projection_resident,
        planned_semantic: semantic,
        planned_steps_identity: steps_identity,
        planned_joint_transaction: joint_transaction,
        scale_factor_bits: scale_factor.to_bits(),
        target_format,
        budget,
    };
    let plan = PropertyScrollScenePlan { steps, seal };
    plan.is_canonical()
        .then_some(plan)
        .ok_or(PropertyScrollScenePlanError::InvalidContract)
}

fn property_scroll_compile_backing_stamp(
    content: &PropertyScrollContentRasterIdentity,
    backing: &PropertyScrollBackingPlan,
) -> PropertyScrollContentBackingCompileStamp {
    match backing {
        PropertyScrollBackingPlan::Single(single) => {
            PropertyScrollContentBackingCompileStamp::Single(PropertyScrollSingleCompileStamp {
                content: content.clone(),
                color_key: single.color_key,
                color_desc: single.color_desc.clone(),
                depth_desc: single.depth_desc.clone(),
                pair_bytes: single.pair_bytes,
                budget: single.budget,
            })
        }
        PropertyScrollBackingPlan::Tiled(tiled) => {
            PropertyScrollContentBackingCompileStamp::Tiled(PropertyScrollTiledCompileStamp {
                content_bounds: tiled.content_bounds,
                tile_edge: tiled.tile_edge,
                gutter: tiled.gutter,
                overscan: tiled.overscan,
                tiles: tiled
                    .tiles
                    .iter()
                    .map(|tile| PropertyScrollTileCompileStamp {
                        content: content.clone(),
                        index: tile.index,
                        bounds: tile.bounds,
                        color_key: tile.color_key,
                        color_desc: tile.color_desc.clone(),
                        depth_desc: tile.depth_desc.clone(),
                        pair_bytes: tile.pair_bytes,
                    })
                    .collect(),
                total_pair_bytes: tiled.total_pair_bytes,
                budget: tiled.budget,
            })
        }
    }
}

fn property_scroll_compile_stamp(
    content: &PropertyScrollContentRasterIdentity,
    clip_split: &PropertyScrollClipSplitWitness,
    backing: &PropertyScrollBackingPlan,
    interactive_resident: Option<&RetainedInteractiveTextAreaResidentRasterSeal>,
    atomic_projection_resident: Option<&RetainedAtomicProjectionTextAreaResidentRasterSeal>,
) -> PropertyScrollContentCompileStamp {
    PropertyScrollContentCompileStamp {
        content: content.clone(),
        local_raster_clips: clip_split.local_raster_clips.clone(),
        local_opaque_terminal: content.local_opaque_span.end,
        backing: property_scroll_compile_backing_stamp(content, backing),
        interactive_resident: interactive_resident.cloned(),
        atomic_projection_resident: atomic_projection_resident.cloned(),
    }
}

fn property_scroll_planned_compiler_step_identities(
    plan: &PropertyScrollScenePlan,
) -> Option<Vec<PropertyScrollCompiledStepIdentity>> {
    plan.steps
        .iter()
        .map(|step| match step {
            ScrollBoundaryStep::HostBefore {
                artifact,
                identity,
                parent_span,
            } => (PropertyScrollPhaseArtifactIdentity::from_artifact(artifact)? == *identity).then(
                || PropertyScrollCompiledStepIdentity::HostBefore {
                    dependency: identity.clone(),
                    parent_span: parent_span.clone(),
                },
            ),
            ScrollBoundaryStep::ContentComposite {
                boundary,
                artifact: _,
                content,
                composite,
                clip_split,
                backing,
                post_composite,
                parent_before,
                parent_after,
            } => Some(PropertyScrollCompiledStepIdentity::DetachedContent {
                boundary: *boundary,
                stamp: property_scroll_compile_stamp(
                    content,
                    clip_split,
                    backing,
                    plan.seal.interactive_resident.as_ref(),
                    plan.seal.atomic_projection_resident.as_ref(),
                ),
                composite: *composite,
                clip_split: clip_split.clone(),
                post_composite: post_composite.clone(),
                parent_before: *parent_before,
                parent_after: *parent_after,
            }),
            ScrollBoundaryStep::OverlayAfter {
                artifact,
                identity,
                parent_span,
            } => (PropertyScrollPhaseArtifactIdentity::from_artifact(artifact)? == *identity).then(
                || PropertyScrollCompiledStepIdentity::OverlayAfter {
                    dependency: identity.clone(),
                    parent_span: parent_span.clone(),
                },
            ),
            ScrollBoundaryStep::AtomicProjectionHostBefore {
                authority,
                identity,
                parent_span,
            } => (authority.is_canonical() && authority.identity() == *identity).then(|| {
                PropertyScrollCompiledStepIdentity::AtomicProjectionHostBefore {
                    dependency: identity.clone(),
                    parent_span: parent_span.clone(),
                }
            }),
            ScrollBoundaryStep::AtomicProjectionSelectionHostBefore {
                authority,
                identity,
                parent_span,
            } => (authority.is_canonical() && authority.identity() == *identity).then(|| {
                PropertyScrollCompiledStepIdentity::AtomicProjectionSelectionHostBefore {
                    dependency: identity.clone(),
                    parent_span: parent_span.clone(),
                }
            }),
            ScrollBoundaryStep::AtomicProjectionSelectionContentComposite {
                boundary,
                authority,
                identity,
                content,
                composite,
                clip_split,
                backing,
                post_composite,
                parent_before,
                parent_after,
            } => (authority.is_canonical() && authority.identity() == *identity).then(|| {
                PropertyScrollCompiledStepIdentity::AtomicProjectionSelectionDetachedContent {
                    boundary: *boundary,
                    dependency: identity.clone(),
                    stamp: property_scroll_compile_stamp(content, clip_split, backing, None, None),
                    composite: *composite,
                    clip_split: clip_split.clone(),
                    post_composite: post_composite.clone(),
                    parent_before: *parent_before,
                    parent_after: *parent_after,
                }
            }),
            ScrollBoundaryStep::AtomicProjectionSelectionOverlayAfter {
                authority,
                identity,
                parent_span,
            } => (authority.is_canonical() && authority.identity() == *identity).then(|| {
                PropertyScrollCompiledStepIdentity::AtomicProjectionSelectionOverlayAfter {
                    dependency: identity.clone(),
                    parent_span: parent_span.clone(),
                }
            }),
            ScrollBoundaryStep::AtomicProjectionContentComposite {
                boundary,
                authority,
                identity,
                content,
                composite,
                clip_split,
                backing,
                post_composite,
                parent_before,
                parent_after,
            } => (authority.is_canonical() && authority.identity() == *identity).then(|| {
                PropertyScrollCompiledStepIdentity::AtomicProjectionDetachedContent {
                    boundary: *boundary,
                    dependency: identity.clone(),
                    stamp: property_scroll_compile_stamp(
                        content,
                        clip_split,
                        backing,
                        plan.seal.interactive_resident.as_ref(),
                        plan.seal.atomic_projection_resident.as_ref(),
                    ),
                    composite: *composite,
                    clip_split: clip_split.clone(),
                    post_composite: post_composite.clone(),
                    parent_before: *parent_before,
                    parent_after: *parent_after,
                }
            }),
            ScrollBoundaryStep::AtomicProjectionOverlayAfter {
                authority,
                identity,
                parent_span,
            } => (authority.is_canonical() && authority.identity() == *identity).then(|| {
                PropertyScrollCompiledStepIdentity::AtomicProjectionOverlayAfter {
                    dependency: identity.clone(),
                    parent_span: parent_span.clone(),
                }
            }),
        })
        .collect()
}

fn property_scroll_compiled_step_identities(
    steps: &[PropertyScrollCompiledStep],
) -> Option<Vec<PropertyScrollCompiledStepIdentity>> {
    steps
        .iter()
        .map(|step| match step {
            PropertyScrollCompiledStep::HostBefore {
                artifact,
                dependency,
                parent_span,
            } => (PropertyScrollPhaseArtifactIdentity::from_artifact(artifact)? == *dependency)
                .then(|| PropertyScrollCompiledStepIdentity::HostBefore {
                    dependency: dependency.clone(),
                    parent_span: parent_span.clone(),
                }),
            PropertyScrollCompiledStep::DetachedContent {
                boundary,
                artifact: _,
                stamp,
                composite,
                clip_split,
                post_composite,
                parent_before,
                parent_after,
            } => Some(PropertyScrollCompiledStepIdentity::DetachedContent {
                boundary: *boundary,
                stamp: stamp.clone(),
                composite: *composite,
                clip_split: clip_split.clone(),
                post_composite: post_composite.clone(),
                parent_before: *parent_before,
                parent_after: *parent_after,
            }),
            PropertyScrollCompiledStep::OverlayAfter {
                artifact,
                dependency,
                parent_span,
            } => (PropertyScrollPhaseArtifactIdentity::from_artifact(artifact)? == *dependency)
                .then(|| PropertyScrollCompiledStepIdentity::OverlayAfter {
                    dependency: dependency.clone(),
                    parent_span: parent_span.clone(),
                }),
            PropertyScrollCompiledStep::AtomicProjectionHostBefore {
                authority,
                dependency,
                parent_span,
            } => (authority.is_canonical() && authority.identity() == *dependency).then(|| {
                PropertyScrollCompiledStepIdentity::AtomicProjectionHostBefore {
                    dependency: dependency.clone(),
                    parent_span: parent_span.clone(),
                }
            }),
            PropertyScrollCompiledStep::AtomicProjectionDetachedContent {
                boundary,
                authority,
                dependency,
                stamp,
                composite,
                clip_split,
                post_composite,
                parent_before,
                parent_after,
            } => (authority.is_canonical() && authority.identity() == *dependency).then(|| {
                PropertyScrollCompiledStepIdentity::AtomicProjectionDetachedContent {
                    boundary: *boundary,
                    dependency: dependency.clone(),
                    stamp: stamp.clone(),
                    composite: *composite,
                    clip_split: clip_split.clone(),
                    post_composite: post_composite.clone(),
                    parent_before: *parent_before,
                    parent_after: *parent_after,
                }
            }),
            PropertyScrollCompiledStep::AtomicProjectionOverlayAfter {
                authority,
                dependency,
                parent_span,
            } => (authority.is_canonical() && authority.identity() == *dependency).then(|| {
                PropertyScrollCompiledStepIdentity::AtomicProjectionOverlayAfter {
                    dependency: dependency.clone(),
                    parent_span: parent_span.clone(),
                }
            }),
            PropertyScrollCompiledStep::AtomicProjectionSelectionHostBefore {
                authority,
                dependency,
                parent_span,
            } => (authority.is_canonical() && authority.identity() == *dependency).then(|| {
                PropertyScrollCompiledStepIdentity::AtomicProjectionSelectionHostBefore {
                    dependency: dependency.clone(),
                    parent_span: parent_span.clone(),
                }
            }),
            PropertyScrollCompiledStep::AtomicProjectionSelectionDetachedContent {
                boundary,
                authority,
                dependency,
                stamp,
                composite,
                clip_split,
                post_composite,
                parent_before,
                parent_after,
            } => (authority.is_canonical() && authority.identity() == *dependency).then(|| {
                PropertyScrollCompiledStepIdentity::AtomicProjectionSelectionDetachedContent {
                    boundary: *boundary,
                    dependency: dependency.clone(),
                    stamp: stamp.clone(),
                    composite: *composite,
                    clip_split: clip_split.clone(),
                    post_composite: post_composite.clone(),
                    parent_before: *parent_before,
                    parent_after: *parent_after,
                }
            }),
            PropertyScrollCompiledStep::AtomicProjectionSelectionOverlayAfter {
                authority,
                dependency,
                parent_span,
            } => (authority.is_canonical() && authority.identity() == *dependency).then(|| {
                PropertyScrollCompiledStepIdentity::AtomicProjectionSelectionOverlayAfter {
                    dependency: dependency.clone(),
                    parent_span: parent_span.clone(),
                }
            }),
        })
        .collect()
}

fn property_scroll_compiler_witness(
    plan: &PropertyScrollScenePlan,
    steps: Vec<PropertyScrollCompiledStepIdentity>,
) -> PropertyScrollBoundaryCompilerWitness {
    PropertyScrollBoundaryCompilerWitness {
        scene_root: plan.seal.scene_root,
        scene_root_stable_id: plan.seal.scene_root_stable_id,
        boundary: plan.seal.boundary,
        admission: plan.seal.admission.clone(),
        text_area_subtree_admission: plan.seal.text_area_subtree_admission,
        interactive_text_area_subtree_admission: plan.seal.interactive_text_area_subtree_admission,
        atomic_projection_text_area_subtree_admission: plan
            .seal
            .atomic_projection_text_area_subtree_admission
            .clone(),
        focused_atomic_projection_text_area_subtree_admission: plan
            .seal
            .focused_atomic_projection_text_area_subtree_admission
            .clone(),
        post_composite: plan.seal.post_composite.clone(),
        interactive_resident: plan.seal.interactive_resident.clone(),
        atomic_projection_resident: plan.seal.atomic_projection_resident.clone(),
        scroll: plan.seal.scroll,
        contents_clip: plan.seal.contents_clip,
        semantic: plan.seal.semantic,
        target_format: plan.seal.target_format,
        budget: plan.seal.budget,
        steps,
    }
}

fn property_scroll_compiled_steps_from_plan(
    plan: &PropertyScrollScenePlan,
) -> Option<Vec<PropertyScrollCompiledStep>> {
    if let [
        ScrollBoundaryStep::AtomicProjectionSelectionHostBefore {
            authority: host_authority,
            identity: host_identity,
            parent_span: host_span,
        },
        ScrollBoundaryStep::AtomicProjectionSelectionContentComposite {
            boundary,
            authority: content_authority,
            identity: content_identity,
            content,
            composite,
            clip_split,
            backing,
            post_composite,
            parent_before,
            parent_after,
        },
        ScrollBoundaryStep::AtomicProjectionSelectionOverlayAfter {
            authority: overlay_authority,
            identity: overlay_identity,
            parent_span: overlay_span,
        },
    ] = plan.steps.as_slice()
    {
        if !host_authority.same_authority(content_authority)
            || !host_authority.same_authority(overlay_authority)
            || host_identity != content_identity
            || host_identity != overlay_identity
            || !matches!(backing, PropertyScrollBackingPlan::Single(_))
            || !matches!(
                post_composite,
                PropertyScrollPostCompositeSchedule::NoneForExistingGrammar
            )
            || parent_before != parent_after
        {
            return None;
        }
        return Some(vec![
            PropertyScrollCompiledStep::AtomicProjectionSelectionHostBefore {
                authority: Arc::clone(host_authority),
                dependency: host_identity.clone(),
                parent_span: host_span.clone(),
            },
            PropertyScrollCompiledStep::AtomicProjectionSelectionDetachedContent {
                boundary: *boundary,
                authority: Arc::clone(content_authority),
                dependency: content_identity.clone(),
                stamp: property_scroll_compile_stamp(content, clip_split, backing, None, None),
                composite: *composite,
                clip_split: clip_split.clone(),
                post_composite: post_composite.clone(),
                parent_before: *parent_before,
                parent_after: *parent_after,
            },
            PropertyScrollCompiledStep::AtomicProjectionSelectionOverlayAfter {
                authority: Arc::clone(overlay_authority),
                dependency: overlay_identity.clone(),
                parent_span: overlay_span.clone(),
            },
        ]);
    }
    if let [
        ScrollBoundaryStep::AtomicProjectionHostBefore {
            authority: host_authority,
            identity: host_identity,
            parent_span: host_span,
        },
        ScrollBoundaryStep::AtomicProjectionContentComposite {
            boundary,
            authority: content_authority,
            identity: content_identity,
            content,
            composite,
            clip_split,
            backing,
            post_composite,
            parent_before,
            parent_after,
        },
        ScrollBoundaryStep::AtomicProjectionOverlayAfter {
            authority: overlay_authority,
            identity: overlay_identity,
            parent_span: overlay_span,
        },
    ] = plan.steps.as_slice()
    {
        if !host_authority.same_authority(content_authority)
            || !host_authority.same_authority(overlay_authority)
            || host_identity != content_identity
            || host_identity != overlay_identity
        {
            return None;
        }
        return Some(vec![
            PropertyScrollCompiledStep::AtomicProjectionHostBefore {
                authority: Arc::clone(host_authority),
                dependency: host_identity.clone(),
                parent_span: host_span.clone(),
            },
            PropertyScrollCompiledStep::AtomicProjectionDetachedContent {
                boundary: *boundary,
                authority: Arc::clone(content_authority),
                dependency: content_identity.clone(),
                stamp: property_scroll_compile_stamp(
                    content,
                    clip_split,
                    backing,
                    plan.seal.interactive_resident.as_ref(),
                    plan.seal.atomic_projection_resident.as_ref(),
                ),
                composite: *composite,
                clip_split: clip_split.clone(),
                post_composite: post_composite.clone(),
                parent_before: *parent_before,
                parent_after: *parent_after,
            },
            PropertyScrollCompiledStep::AtomicProjectionOverlayAfter {
                authority: Arc::clone(overlay_authority),
                dependency: overlay_identity.clone(),
                parent_span: overlay_span.clone(),
            },
        ]);
    }
    let [
        ScrollBoundaryStep::HostBefore {
            artifact: host_artifact,
            identity: host_dependency,
            parent_span: host_span,
        },
        ScrollBoundaryStep::ContentComposite {
            boundary,
            artifact: content_artifact,
            content,
            composite,
            clip_split,
            backing,
            post_composite,
            parent_before,
            parent_after,
        },
        ScrollBoundaryStep::OverlayAfter {
            artifact: overlay_artifact,
            identity: overlay_dependency,
            parent_span: overlay_span,
        },
    ] = plan.steps.as_slice()
    else {
        return None;
    };
    Some(vec![
        PropertyScrollCompiledStep::HostBefore {
            artifact: host_artifact.clone(),
            dependency: host_dependency.clone(),
            parent_span: host_span.clone(),
        },
        PropertyScrollCompiledStep::DetachedContent {
            boundary: *boundary,
            artifact: content_artifact.clone(),
            stamp: property_scroll_compile_stamp(
                content,
                clip_split,
                backing,
                plan.seal.interactive_resident.as_ref(),
                plan.seal.atomic_projection_resident.as_ref(),
            ),
            composite: *composite,
            clip_split: clip_split.clone(),
            post_composite: post_composite.clone(),
            parent_before: *parent_before,
            parent_after: *parent_after,
        },
        PropertyScrollCompiledStep::OverlayAfter {
            artifact: overlay_artifact.clone(),
            dependency: overlay_dependency.clone(),
            parent_span: overlay_span.clone(),
        },
    ])
}

fn checked_property_scroll_opaque_order_count(artifact: &PaintArtifact) -> Option<u32> {
    let count = opaque_order_count(artifact);
    (count != u32::MAX).then_some(count)
}

fn property_scroll_boundary_is_canonical(boundary: &ValidatedPropertyScrollBoundary) -> bool {
    if !boundary.planner.is_canonical() {
        return false;
    }
    let Some(planned_steps) = property_scroll_planned_compiler_step_identities(&boundary.planner)
    else {
        return false;
    };
    let Some(compiled_steps) = property_scroll_compiled_step_identities(&boundary.steps) else {
        return false;
    };
    let planned_witness = property_scroll_compiler_witness(&boundary.planner, planned_steps);
    let compiled_witness = property_scroll_compiler_witness(&boundary.planner, compiled_steps);
    if !planned_witness
        .admission
        .exactly_corresponds_to_with_atomic(
            planned_witness.text_area_subtree_admission,
            planned_witness.interactive_text_area_subtree_admission,
            planned_witness
                .atomic_projection_text_area_subtree_admission
                .as_ref(),
            planned_witness
                .focused_atomic_projection_text_area_subtree_admission
                .as_ref(),
            &planned_witness.post_composite,
        )
        || !compiled_witness
            .admission
            .exactly_corresponds_to_with_atomic(
                compiled_witness.text_area_subtree_admission,
                compiled_witness.interactive_text_area_subtree_admission,
                compiled_witness
                    .atomic_projection_text_area_subtree_admission
                    .as_ref(),
                compiled_witness
                    .focused_atomic_projection_text_area_subtree_admission
                    .as_ref(),
                &compiled_witness.post_composite,
            )
        || !boundary
            .seal
            .planner
            .admission
            .exactly_corresponds_to_with_atomic(
                boundary.seal.planner.text_area_subtree_admission,
                boundary
                    .seal
                    .planner
                    .interactive_text_area_subtree_admission,
                boundary
                    .seal
                    .planner
                    .atomic_projection_text_area_subtree_admission
                    .as_ref(),
                boundary
                    .seal
                    .planner
                    .focused_atomic_projection_text_area_subtree_admission
                    .as_ref(),
                &boundary.seal.planner.post_composite,
            )
        || !boundary
            .seal
            .compiler
            .admission
            .exactly_corresponds_to_with_atomic(
                boundary.seal.compiler.text_area_subtree_admission,
                boundary
                    .seal
                    .compiler
                    .interactive_text_area_subtree_admission,
                boundary
                    .seal
                    .compiler
                    .atomic_projection_text_area_subtree_admission
                    .as_ref(),
                boundary
                    .seal
                    .compiler
                    .focused_atomic_projection_text_area_subtree_admission
                    .as_ref(),
                &boundary.seal.compiler.post_composite,
            )
        || boundary.seal.planner != planned_witness
        || boundary.seal.compiler != compiled_witness
        || boundary.seal.planner != boundary.seal.compiler
    {
        return false;
    }
    if let [
        PropertyScrollCompiledStep::AtomicProjectionSelectionHostBefore {
            authority: host_authority,
            dependency: host_dependency,
            parent_span: host_span,
        },
        PropertyScrollCompiledStep::AtomicProjectionSelectionDetachedContent {
            boundary: compiled_boundary,
            authority: content_authority,
            dependency: content_dependency,
            stamp,
            composite,
            clip_split,
            post_composite,
            parent_before,
            parent_after,
        },
        PropertyScrollCompiledStep::AtomicProjectionSelectionOverlayAfter {
            authority: overlay_authority,
            dependency: overlay_dependency,
            parent_span: overlay_span,
        },
    ] = boundary.steps.as_slice()
    {
        let [
            ScrollBoundaryStep::AtomicProjectionSelectionHostBefore {
                authority: planned_host,
                identity: planned_host_identity,
                parent_span: planned_host_span,
            },
            ScrollBoundaryStep::AtomicProjectionSelectionContentComposite {
                boundary: planned_boundary,
                authority: planned_content,
                identity: planned_content_identity,
                content,
                composite: planned_composite,
                clip_split: planned_clip_split,
                backing,
                post_composite: planned_post_composite,
                parent_before: planned_parent_before,
                parent_after: planned_parent_after,
            },
            ScrollBoundaryStep::AtomicProjectionSelectionOverlayAfter {
                authority: planned_overlay,
                identity: planned_overlay_identity,
                parent_span: planned_overlay_span,
            },
        ] = boundary.planner.steps.as_slice()
        else {
            return false;
        };
        let PropertyScrollContentBackingCompileStamp::Single(single) = &stamp.backing else {
            return false;
        };
        let Some(raster_stamp) =
            validated_scroll_atomic_projection_selection_text_area_content_raster_stamp(
                content.content_root,
                content.content_stable_id,
                RetainedSurfaceRasterInputs {
                    color: single.color_desc.clone(),
                    depth: single.depth_desc.clone(),
                    scale_factor_bits: 1.0_f32.to_bits(),
                    source_bounds_bits: content.source_bounds_bits,
                },
                content.artifact_span.clone(),
                content.local_opaque_span.clone(),
                host_authority.resident().clone(),
            )
        else {
            return false;
        };
        return host_authority.same_authority(content_authority)
            && host_authority.same_authority(overlay_authority)
            && host_authority.same_authority(planned_host)
            && host_authority.same_authority(planned_content)
            && host_authority.same_authority(planned_overlay)
            && host_dependency == content_dependency
            && host_dependency == overlay_dependency
            && host_dependency == planned_host_identity
            && host_dependency == planned_content_identity
            && host_dependency == planned_overlay_identity
            && compiled_boundary == planned_boundary
            && host_span == planned_host_span
            && overlay_span == planned_overlay_span
            && composite == planned_composite
            && clip_split == planned_clip_split
            && post_composite == planned_post_composite
            && matches!(
                post_composite,
                PropertyScrollPostCompositeSchedule::NoneForExistingGrammar
            )
            && parent_before == parent_after
            && parent_before == planned_parent_before
            && parent_after == planned_parent_after
            && stamp
                == &property_scroll_compile_stamp(
                    content,
                    planned_clip_split,
                    backing,
                    None,
                    None,
                )
            && host_authority.matches_atomic_raster_stamp(&raster_stamp);
    }
    if let [
        PropertyScrollCompiledStep::AtomicProjectionHostBefore {
            authority: host_authority,
            dependency: host_dependency,
            parent_span: host_span,
        },
        PropertyScrollCompiledStep::AtomicProjectionDetachedContent {
            boundary: compiled_boundary,
            authority: content_authority,
            dependency: content_dependency,
            stamp,
            composite,
            clip_split,
            post_composite,
            parent_before,
            parent_after,
        },
        PropertyScrollCompiledStep::AtomicProjectionOverlayAfter {
            authority: overlay_authority,
            dependency: overlay_dependency,
            parent_span: overlay_span,
        },
    ] = boundary.steps.as_slice()
    {
        let [
            ScrollBoundaryStep::AtomicProjectionHostBefore {
                authority: planned_host,
                identity: planned_host_identity,
                parent_span: planned_host_span,
            },
            ScrollBoundaryStep::AtomicProjectionContentComposite {
                boundary: planned_boundary,
                authority: planned_content,
                identity: planned_content_identity,
                content,
                composite: planned_composite,
                clip_split: planned_clip_split,
                backing,
                post_composite: planned_post_composite,
                parent_before: planned_parent_before,
                parent_after: planned_parent_after,
            },
            ScrollBoundaryStep::AtomicProjectionOverlayAfter {
                authority: planned_overlay,
                identity: planned_overlay_identity,
                parent_span: planned_overlay_span,
            },
        ] = boundary.planner.steps.as_slice()
        else {
            return false;
        };
        return host_authority.same_authority(content_authority)
            && host_authority.same_authority(overlay_authority)
            && host_authority.same_authority(planned_host)
            && host_authority.same_authority(planned_content)
            && host_authority.same_authority(planned_overlay)
            && host_dependency == content_dependency
            && host_dependency == overlay_dependency
            && host_dependency == planned_host_identity
            && host_dependency == planned_content_identity
            && host_dependency == planned_overlay_identity
            && compiled_boundary == planned_boundary
            && host_span == planned_host_span
            && overlay_span == planned_overlay_span
            && composite == planned_composite
            && clip_split == planned_clip_split
            && post_composite == planned_post_composite
            && parent_before == planned_parent_before
            && parent_after == planned_parent_after
            && stamp
                == &property_scroll_compile_stamp(
                    content,
                    planned_clip_split,
                    backing,
                    boundary.planner.seal.interactive_resident.as_ref(),
                    boundary.planner.seal.atomic_projection_resident.as_ref(),
                );
    }
    let [host, content, overlay] = boundary.steps.as_slice() else {
        return false;
    };
    let (
        PropertyScrollCompiledStep::HostBefore {
            artifact: host_artifact,
            dependency: host_dependency,
            parent_span: host_span,
        },
        PropertyScrollCompiledStep::DetachedContent {
            boundary: compiled_boundary,
            artifact: content_artifact,
            stamp,
            composite,
            clip_split,
            post_composite,
            parent_before,
            parent_after,
        },
        PropertyScrollCompiledStep::OverlayAfter {
            artifact: overlay_artifact,
            dependency: overlay_dependency,
            parent_span: overlay_span,
        },
    ) = (host, content, overlay)
    else {
        return false;
    };
    let plan = &boundary.planner;
    let host_bounds_bits = bounds_bits(plan.seal.admission.source_bounds);
    let content_bounds_bits = bounds_bits(content_zero_bounds(plan.seal.scroll));
    let Some(_host_authority) = validate_scroll_scene_host_before_artifact(
        host_artifact.clone(),
        plan.seal.scene_root,
        host_bounds_bits,
    ) else {
        return false;
    };
    let content_authority = if let Some(text_area) =
        plan.seal.admission.text_area_subtree_snapshot()
    {
        let [local_clip] = content_artifact.clip_nodes.as_slice() else {
            return false;
        };
        validate_scroll_scene_text_area_content_artifact(
            content_artifact.clone(),
            plan.seal.admission.child,
            text_area.text_area_root,
            text_area.paint_grammar,
            *local_clip,
            content_bounds_bits,
        )
    } else if let Some(text_area) = plan.seal.admission.interactive_text_area_subtree_snapshot() {
        let [local_clip] = content_artifact.clip_nodes.as_slice() else {
            return false;
        };
        let preedit = match plan.seal.interactive_resident.as_ref() {
            Some(RetainedInteractiveTextAreaResidentRasterSeal::FocusedPreeditGlyphs(seal)) => {
                Some(seal.clone())
            }
            _ => None,
        };
        let Some(validated) = validate_scroll_scene_interactive_text_area_content_artifact(
            content_artifact.clone(),
            plan.seal.admission.child,
            text_area.text_area_root,
            text_area.paint_grammar,
            preedit,
            *local_clip,
            content_bounds_bits,
        ) else {
            return false;
        };
        let (content, resident) = validated.into_parts();
        if Some(&resident) != plan.seal.interactive_resident.as_ref() {
            return false;
        }
        Some(content)
    } else {
        validate_scroll_scene_content_artifact(
            content_artifact.clone(),
            plan.seal.admission.child,
            content_bounds_bits,
        )
    };
    let Some(content_authority) = content_authority else {
        return false;
    };
    let Some(_overlay_authority) = validate_scroll_scene_overlay_artifact(
        overlay_artifact.clone(),
        plan.seal.scene_root,
        plan.seal.scroll,
        host_bounds_bits,
    ) else {
        return false;
    };
    let Some(host_dependency_from_artifact) =
        PropertyScrollPhaseArtifactIdentity::from_artifact(host_artifact)
    else {
        return false;
    };
    let Some(overlay_dependency_from_artifact) =
        PropertyScrollPhaseArtifactIdentity::from_artifact(overlay_artifact)
    else {
        return false;
    };
    let Some(host_terminal) = checked_property_scroll_opaque_order_count(host_artifact) else {
        return false;
    };
    let Some(content_terminal) = checked_property_scroll_opaque_order_count(content_artifact)
    else {
        return false;
    };
    let Some(overlay_count) = checked_property_scroll_opaque_order_count(overlay_artifact) else {
        return false;
    };
    let Some(post_composite_delta) = post_composite.opaque_order_delta() else {
        return false;
    };
    let Some(overlay_start) = host_terminal.checked_add(post_composite_delta) else {
        return false;
    };
    let Some(parent_terminal) = overlay_start.checked_add(overlay_count) else {
        return false;
    };
    let [
        _,
        ScrollBoundaryStep::ContentComposite {
            content: planned_content,
            backing: planned_backing,
            ..
        },
        _,
    ] = plan.steps.as_slice()
    else {
        return false;
    };
    let expected_content_span = validated_scroll_content_artifact_span_stamp(
        &content_authority,
        0,
        stamp.content.local_opaque_span.clone(),
    );
    *compiled_boundary == plan.seal.boundary
        && host_span == &(0..host_terminal)
        && *parent_before == host_span.end
        && *parent_after == overlay_start
        && overlay_span == &(overlay_start..parent_terminal)
        && host_dependency == &host_dependency_from_artifact
        && overlay_dependency == &overlay_dependency_from_artifact
        && expected_content_span.as_ref() == Some(&stamp.content.artifact_span)
        && stamp.content == *planned_content
        && stamp.content.local_opaque_span == (0..content_terminal)
        && stamp.content.content_root == plan.seal.admission.child
        && stamp.content.content_stable_id == plan.seal.admission.child_stable_id
        && stamp.content.source_bounds_bits == content_bounds_bits
        && stamp.local_raster_clips == content_artifact.clip_nodes
        && stamp.local_raster_clips == clip_split.local_raster_clips
        && post_composite == &plan.seal.post_composite
        && stamp.interactive_resident == plan.seal.interactive_resident
        && stamp.local_opaque_terminal == content_terminal
        && stamp.backing == property_scroll_compile_backing_stamp(planned_content, planned_backing)
        && clip_split.own_contents_clip == plan.seal.contents_clip
        && clip_split.ancestor_composite_clips.is_empty()
        && matches!(composite.basis, ScrollCompositeBasis::FrameRoot)
        && composite.offset_bits
            == [
                plan.seal.scroll.offset.x.to_bits(),
                plan.seal.scroll.offset.y.to_bits(),
            ]
        && composite.contents_clip == plan.seal.contents_clip.logical_scissor
}

impl ValidatedPropertyScrollBoundary {
    pub(crate) fn is_canonical(&self) -> bool {
        property_scroll_boundary_is_canonical(self)
    }

    #[allow(clippy::too_many_arguments)]
    #[allow(dead_code)]
    pub(crate) fn matches_live_inputs(
        &self,
        arena: &NodeArena,
        roots: &[NodeKey],
        property_trees: &PropertyTrees,
        paint_generations: &PaintGenerationTracker,
        semantic_frame_time: crate::time::Instant,
    ) -> bool {
        self.is_canonical()
            && self.planner.matches_live_inputs(
                arena,
                roots,
                property_trees,
                paint_generations,
                semantic_frame_time,
            )
    }
}

#[allow(clippy::too_many_arguments)]
#[allow(dead_code)] // Singleton compiler entry remains a focused-test oracle.
#[cfg(test)]
fn validate_property_scroll_boundary(
    plan: PropertyScrollScenePlan,
    arena: &NodeArena,
    roots: &[NodeKey],
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    semantic_frame_time: crate::time::Instant,
) -> Result<ValidatedPropertyScrollBoundary, PropertyScrollBoundaryValidationError> {
    if !plan.is_canonical() {
        return Err(PropertyScrollBoundaryValidationError::PlannerDrift);
    }
    if !plan.matches_live_inputs(
        arena,
        roots,
        property_trees,
        paint_generations,
        semantic_frame_time,
    ) {
        return Err(PropertyScrollBoundaryValidationError::LiveSnapshotDrift);
    }
    let boundary = validate_property_scroll_boundary_from_frozen_plan(plan)?;
    if !boundary.matches_live_inputs(
        arena,
        roots,
        property_trees,
        paint_generations,
        semantic_frame_time,
    ) {
        return Err(PropertyScrollBoundaryValidationError::LiveSnapshotDrift);
    }
    Ok(boundary)
}

fn validate_property_scroll_boundary_from_frozen_plan(
    plan: PropertyScrollScenePlan,
) -> Result<ValidatedPropertyScrollBoundary, PropertyScrollBoundaryValidationError> {
    if !plan.is_canonical() {
        return Err(PropertyScrollBoundaryValidationError::PlannerDrift);
    }
    let planned_steps = property_scroll_planned_compiler_step_identities(&plan)
        .ok_or(PropertyScrollBoundaryValidationError::ArtifactContract)?;
    let steps = property_scroll_compiled_steps_from_plan(&plan)
        .ok_or(PropertyScrollBoundaryValidationError::ArtifactContract)?;
    let compiled_steps = property_scroll_compiled_step_identities(&steps)
        .ok_or(PropertyScrollBoundaryValidationError::ArtifactContract)?;
    let boundary = ValidatedPropertyScrollBoundary {
        seal: PropertyScrollBoundaryCompilerSeal {
            planner: property_scroll_compiler_witness(&plan, planned_steps),
            compiler: property_scroll_compiler_witness(&plan, compiled_steps),
        },
        planner: plan,
        steps,
    };
    boundary
        .is_canonical()
        .then_some(boundary)
        .ok_or(PropertyScrollBoundaryValidationError::StampContract)
}

fn property_scroll_backing_pair_bytes(backing: &PropertyScrollBackingPlan) -> u64 {
    match backing {
        PropertyScrollBackingPlan::Single(single) => single.pair_bytes,
        PropertyScrollBackingPlan::Tiled(tiled) => tiled.total_pair_bytes,
    }
}

fn property_scroll_backing_color_keys(
    backing: &PropertyScrollBackingPlan,
) -> impl Iterator<Item = PersistentTextureKey> + '_ {
    let single = match backing {
        PropertyScrollBackingPlan::Single(single) => Some(single.color_key),
        PropertyScrollBackingPlan::Tiled(_) => None,
    };
    let tiled = match backing {
        PropertyScrollBackingPlan::Single(_) => [].as_slice(),
        PropertyScrollBackingPlan::Tiled(tiled) => tiled.tiles.as_slice(),
    };
    single
        .into_iter()
        .chain(tiled.iter().map(|tile| tile.color_key))
}

fn property_scroll_scene_seal_from_boundaries(
    boundaries: &[ValidatedPropertyScrollBoundary],
) -> Option<ValidatedPropertyScrollSceneSeal> {
    let first = boundaries.first()?;
    let semantic_frame_time = first.planner.seal.semantic.sampled_at;
    let target_format = first.planner.seal.target_format;
    let budget = first.planner.seal.budget;
    let mut roots = Vec::with_capacity(boundaries.len());
    let mut ordered_boundaries = Vec::with_capacity(boundaries.len());
    let mut schedule = Vec::with_capacity(boundaries.len().checked_mul(3)?);
    let mut aggregate_pair_bytes = 0_u64;
    let mut parent_cursor = 0_u32;
    let mut seen_roots = FxHashSet::default();
    let mut seen_stable_ids = FxHashSet::default();
    let mut seen_contents = FxHashSet::default();
    let mut seen_gpu_keys = FxHashSet::default();

    for (index, boundary) in boundaries.iter().enumerate() {
        if !boundary.is_canonical()
            || boundary.planner.seal.semantic.sampled_at != semantic_frame_time
            || boundary.planner.seal.target_format != target_format
            || boundary.planner.seal.budget != budget
            || !seen_roots.insert(boundary.planner.seal.scene_root)
            || !seen_stable_ids.insert(boundary.planner.seal.scene_root_stable_id)
            || !seen_contents.insert(boundary.planner.seal.admission.child)
            || !seen_stable_ids.insert(boundary.planner.seal.admission.child_stable_id)
        {
            return None;
        }
        let ordinal = u32::try_from(index).ok()?;
        let global_boundary = SceneBoundaryId {
            ordinal,
            owner: boundary.planner.seal.scene_root,
            kind: SceneBoundaryKind::ScrollContents,
        };
        let [host, content, overlay] = boundary.steps.as_slice() else {
            return None;
        };
        let (host_count, content_count, overlay_count, stamp) = match (host, content, overlay) {
            (
                PropertyScrollCompiledStep::HostBefore {
                    artifact: host_artifact,
                    ..
                },
                PropertyScrollCompiledStep::DetachedContent {
                    artifact: content_artifact,
                    stamp,
                    ..
                },
                PropertyScrollCompiledStep::OverlayAfter {
                    artifact: overlay_artifact,
                    ..
                },
            ) => (
                checked_property_scroll_opaque_order_count(host_artifact)?,
                checked_property_scroll_opaque_order_count(content_artifact)?,
                checked_property_scroll_opaque_order_count(overlay_artifact)?,
                stamp,
            ),
            (
                PropertyScrollCompiledStep::AtomicProjectionHostBefore {
                    authority: host, ..
                },
                PropertyScrollCompiledStep::AtomicProjectionDetachedContent {
                    authority: content,
                    stamp,
                    ..
                },
                PropertyScrollCompiledStep::AtomicProjectionOverlayAfter {
                    authority: overlay, ..
                },
            ) if host.same_authority(content) && host.same_authority(overlay) => (
                host.host_before_opaque_order_count()?,
                content.content_opaque_order_count()?,
                overlay.overlay_opaque_order_count()?,
                stamp,
            ),
            (
                PropertyScrollCompiledStep::AtomicProjectionSelectionHostBefore {
                    authority: host,
                    ..
                },
                PropertyScrollCompiledStep::AtomicProjectionSelectionDetachedContent {
                    authority: content,
                    stamp,
                    ..
                },
                PropertyScrollCompiledStep::AtomicProjectionSelectionOverlayAfter {
                    authority: overlay,
                    ..
                },
            ) if host.same_authority(content) && host.same_authority(overlay) => (
                host.host_before_opaque_order_count()?,
                content.content_opaque_order_count()?,
                overlay.overlay_opaque_order_count()?,
                stamp,
            ),
            _ => return None,
        };
        let post_composite_delta = boundary.planner.seal.post_composite.opaque_order_delta()?;
        let host_end = parent_cursor.checked_add(host_count)?;
        let post_composite_end = host_end.checked_add(post_composite_delta)?;
        let parent_end = post_composite_end.checked_add(overlay_count)?;
        schedule.push(PropertyScrollSceneScheduleStamp {
            boundary: global_boundary,
            phase: PropertyScrollScenePhase::HostBefore,
            parent_span: parent_cursor..host_end,
            local_span: 0..host_count,
        });
        schedule.push(PropertyScrollSceneScheduleStamp {
            boundary: global_boundary,
            phase: PropertyScrollScenePhase::DetachedContent,
            parent_span: host_end..post_composite_end,
            local_span: 0..content_count,
        });
        schedule.push(PropertyScrollSceneScheduleStamp {
            boundary: global_boundary,
            phase: PropertyScrollScenePhase::OverlayAfter,
            parent_span: post_composite_end..parent_end,
            local_span: host_count.checked_add(post_composite_delta)?
                ..host_count
                    .checked_add(post_composite_delta)?
                    .checked_add(overlay_count)?,
        });
        parent_cursor = parent_end;

        let backing = &stamp.backing;
        let pair_bytes = match backing {
            PropertyScrollContentBackingCompileStamp::Single(single) => single.pair_bytes,
            PropertyScrollContentBackingCompileStamp::Tiled(tiled) => tiled.total_pair_bytes,
        };
        aggregate_pair_bytes = aggregate_pair_bytes.checked_add(pair_bytes)?;
        let planned_backing = match &boundary.planner.steps[1] {
            ScrollBoundaryStep::ContentComposite { backing, .. }
            | ScrollBoundaryStep::AtomicProjectionContentComposite { backing, .. }
            | ScrollBoundaryStep::AtomicProjectionSelectionContentComposite { backing, .. } => {
                backing
            }
            _ => return None,
        };
        if property_scroll_backing_pair_bytes(planned_backing) != pair_bytes {
            return None;
        }
        for color_key in property_scroll_backing_color_keys(planned_backing) {
            let depth_key = color_key.depth_stencil()?;
            if !seen_gpu_keys.insert(color_key) || !seen_gpu_keys.insert(depth_key) {
                return None;
            }
        }
        roots.push(PropertySceneJointRootPlanningWitness {
            ordinal,
            root: boundary.planner.seal.scene_root,
            stable_id: boundary.planner.seal.scene_root_stable_id,
            boundary_span: ordinal..ordinal.checked_add(1)?,
        });
        ordered_boundaries.push(global_boundary);
    }
    if aggregate_pair_bytes > budget.max_active_pair_bytes {
        return None;
    }
    Some(ValidatedPropertyScrollSceneSeal {
        roots,
        ordered_boundaries,
        schedule,
        semantic_frame_time,
        target_format,
        budget,
        aggregate_pair_bytes,
    })
}

impl ValidatedPropertyScrollScene {
    pub(crate) fn is_canonical(&self) -> bool {
        !self.boundaries.is_empty()
            && property_scroll_scene_seal_from_boundaries(&self.boundaries).as_ref()
                == Some(&self.seal)
    }

    #[cfg(test)]
    pub(crate) fn boundary_count(&self) -> usize {
        self.boundaries.len()
    }

    #[cfg(test)]
    pub(crate) fn interactive_caret_is_culled_for_test(&self) -> bool {
        self.boundaries.first().is_some_and(|boundary| {
            matches!(
                &boundary.planner.seal.post_composite,
                PropertyScrollPostCompositeSchedule::InteractiveTextAreaCaret(caret)
                    if matches!(
                        caret.recorded.identity.paint,
                        super::RetainedTextAreaCaretOverlayPaintIdentity::Culled { .. }
                    )
            )
        })
    }

    #[cfg(test)]
    pub(crate) fn interactive_post_composite_opaque_delta_for_test(&self) -> Option<u32> {
        self.boundaries
            .first()?
            .planner
            .seal
            .post_composite
            .opaque_order_delta()
    }

    #[cfg(test)]
    pub(crate) fn atomic_projection_prepare_and_collision_are_atomic_for_test(&self) -> bool {
        let (Some(boundary), Some(global_boundary)) = (
            self.boundaries.first().cloned(),
            self.seal.ordered_boundaries.first().copied(),
        ) else {
            return false;
        };
        if !matches!(
            boundary.steps.as_slice(),
            [
                PropertyScrollCompiledStep::AtomicProjectionHostBefore { .. },
                PropertyScrollCompiledStep::AtomicProjectionDetachedContent { .. },
                PropertyScrollCompiledStep::AtomicProjectionOverlayAfter { .. },
            ]
        ) {
            return false;
        }
        let Some((color_key, _)) = self.first_single_backing_declaration_for_test() else {
            return false;
        };
        let mut successful_declarations = FxHashSet::default();
        let success = prepare_retained_property_scroll_boundary_parts(
            boundary.clone(),
            global_boundary,
            1.0_f32.to_bits(),
            &FxHashSet::default(),
            &mut successful_declarations,
        );
        let mut colliding_graph_keys = FxHashSet::default();
        colliding_graph_keys.insert(color_key);
        let mut rejected_declarations = FxHashSet::default();
        let collision = prepare_retained_property_scroll_boundary_parts(
            boundary,
            global_boundary,
            1.0_f32.to_bits(),
            &colliding_graph_keys,
            &mut rejected_declarations,
        );
        matches!(
            success,
            Ok(PreparedRetainedPropertyScrollBoundaryParts {
                authority:
                    PreparedRetainedPropertyScrollBoundaryAuthority::AtomicProjectionTextArea { .. },
                backing: PreparedRetainedPropertyScrollBacking::Single { .. },
                ..
            })
        ) && successful_declarations.len() == 2
            && successful_declarations.contains(&color_key)
            && matches!(
                collision,
                Err(
                    RetainedPropertyScrollScenePrepareError::PersistentKeyAlreadyDeclared(key)
                ) if key == color_key
            )
            && rejected_declarations.is_empty()
    }

    #[cfg(test)]
    pub(crate) fn atomic_projection_selection_contract_for_test(&self) -> bool {
        let Some(boundary) = self.boundaries.first() else {
            return false;
        };
        let [
            PropertyScrollCompiledStep::AtomicProjectionSelectionHostBefore {
                authority: host,
                dependency: host_dependency,
                parent_span: host_span,
            },
            PropertyScrollCompiledStep::AtomicProjectionSelectionDetachedContent {
                authority: content,
                dependency: content_dependency,
                stamp,
                post_composite,
                parent_before,
                parent_after,
                ..
            },
            PropertyScrollCompiledStep::AtomicProjectionSelectionOverlayAfter {
                authority: overlay,
                dependency: overlay_dependency,
                parent_span: overlay_span,
            },
        ] = boundary.steps.as_slice()
        else {
            return false;
        };
        matches!(
            boundary.planner.seal.admission.kind,
            PropertyScrollHostAdmissionKind::AtomicProjectionSelectionTextAreaSubtree(_)
        ) && boundary.planner.seal.text_area_subtree_admission.is_none()
            && boundary
                .planner
                .seal
                .interactive_text_area_subtree_admission
                .is_none()
            && boundary
                .planner
                .seal
                .atomic_projection_text_area_subtree_admission
                .is_none()
            && boundary.planner.seal.interactive_resident.is_none()
            && boundary.planner.seal.atomic_projection_resident.is_none()
            && host.same_authority(content)
            && host.same_authority(overlay)
            && host_dependency == content_dependency
            && host_dependency == overlay_dependency
            && stamp.interactive_resident.is_none()
            && stamp.atomic_projection_resident.is_none()
            && matches!(
                stamp.backing,
                PropertyScrollContentBackingCompileStamp::Single(_)
            )
            && matches!(
                post_composite,
                PropertyScrollPostCompositeSchedule::NoneForExistingGrammar
            )
            && parent_before == parent_after
            && host_span.end == *parent_before
            && overlay_span.start == *parent_after
    }

    #[cfg(test)]
    pub(crate) fn atomic_projection_selection_tamper_matrix_for_test(&self) -> bool {
        if !self.atomic_projection_selection_contract_for_test() {
            return false;
        }
        let Some(source_boundary) = self.boundaries.first() else {
            return false;
        };
        let ScrollBoundaryStep::AtomicProjectionSelectionHostBefore {
            authority: source_authority,
            ..
        } = &source_boundary.planner.steps[0]
        else {
            return false;
        };
        let source_authority = source_authority.as_ref().clone();

        let rejects_authority =
            |tampered: ValidatedScrollSceneAtomicProjectionSelectionTextAreaPlanParts| {
                let mut boundary = source_boundary.clone();
                let authority = Arc::new(tampered);
                let identity = authority.identity();
                for step in &mut boundary.planner.steps {
                    match step {
                        ScrollBoundaryStep::AtomicProjectionSelectionHostBefore {
                            authority: step_authority,
                            identity: step_identity,
                            ..
                        }
                        | ScrollBoundaryStep::AtomicProjectionSelectionContentComposite {
                            authority: step_authority,
                            identity: step_identity,
                            ..
                        }
                        | ScrollBoundaryStep::AtomicProjectionSelectionOverlayAfter {
                            authority: step_authority,
                            identity: step_identity,
                            ..
                        } => {
                            *step_authority = Arc::clone(&authority);
                            *step_identity = identity.clone();
                        }
                        _ => return false,
                    }
                }
                for step in &mut boundary.steps {
                    match step {
                        PropertyScrollCompiledStep::AtomicProjectionSelectionHostBefore {
                            authority: step_authority,
                            dependency,
                            ..
                        }
                        | PropertyScrollCompiledStep::AtomicProjectionSelectionDetachedContent {
                            authority: step_authority,
                            dependency,
                            ..
                        }
                        | PropertyScrollCompiledStep::AtomicProjectionSelectionOverlayAfter {
                            authority: step_authority,
                            dependency,
                            ..
                        } => {
                            *step_authority = Arc::clone(&authority);
                            *dependency = identity.clone();
                        }
                        _ => return false,
                    }
                }
                !boundary.is_canonical()
                    && property_scroll_scene_seal_from_boundaries(&[boundary]).is_none()
            };

        let authority_tampers_reject = [
            source_authority.clone().tamper_host_for_test(),
            source_authority.clone().tamper_content_order_for_test(),
            source_authority.clone().tamper_geometry_for_test(),
            source_authority.clone().tamper_topology_for_test(),
            source_authority.tamper_selection_synchronized_for_test(),
        ]
        .into_iter()
        .all(rejects_authority);

        let mut reordered = source_boundary.clone();
        reordered.planner.steps.swap(0, 2);
        reordered.steps.swap(0, 2);
        let reordered_rejects = !reordered.is_canonical()
            && property_scroll_scene_seal_from_boundaries(&[reordered]).is_none();

        let mut kind_drift = source_boundary.clone();
        for admission in [
            &mut kind_drift.planner.seal.admission,
            &mut kind_drift.planner.seal.planned_admission,
            &mut kind_drift.seal.planner.admission,
            &mut kind_drift.seal.compiler.admission,
        ] {
            let PropertyScrollHostAdmissionKind::AtomicProjectionSelectionTextAreaSubtree(
                selection,
            ) = &mut admission.kind
            else {
                return false;
            };
            selection
                .paint_grammar
                .atomic_source
                .projection_text_stable_id ^= 1;
        }
        let kind_drift_rejects = !kind_drift.is_canonical()
            && property_scroll_scene_seal_from_boundaries(&[kind_drift]).is_none();

        authority_tampers_reject && reordered_rejects && kind_drift_rejects
    }

    #[cfg(test)]
    pub(crate) fn atomic_projection_selection_prepare_failure_matrix_is_atomic_for_test(
        &self,
    ) -> bool {
        let (Some(source), Some(global_boundary)) = (
            self.boundaries.first().cloned(),
            self.seal.ordered_boundaries.first().copied(),
        ) else {
            return false;
        };
        let Some((color_key, _)) = self.first_single_backing_declaration_for_test() else {
            return false;
        };
        let rejects_without_declaration = |boundary| {
            let mut declared = FxHashSet::default();
            let result = prepare_retained_property_scroll_boundary_parts(
                boundary,
                global_boundary,
                1.0_f32.to_bits(),
                &FxHashSet::default(),
                &mut declared,
            );
            matches!(
                result,
                Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift)
            ) && declared.is_empty()
        };

        let mut invalid = source.clone();
        let PropertyScrollCompiledStep::AtomicProjectionSelectionDetachedContent {
            parent_after,
            ..
        } = &mut invalid.steps[1]
        else {
            return false;
        };
        *parent_after = parent_after.saturating_add(1);

        let mut reordered = source.clone();
        reordered.steps.swap(0, 2);

        let mut hybrid_phase = source.clone();
        hybrid_phase.steps[2] = hybrid_phase.steps[0].clone();

        let mut tiled = source.clone();
        let ScrollBoundaryStep::AtomicProjectionSelectionContentComposite {
            content, backing, ..
        } = &mut tiled.planner.steps[1]
        else {
            return false;
        };
        let PropertyScrollBackingPlan::Single(single) = backing.clone() else {
            return false;
        };
        let Some(content_bounds) = exact_u32_bounds_from_bits(content.source_bounds_bits) else {
            return false;
        };
        let tile_edge = content_bounds[2].max(content_bounds[3]);
        let index = super::ScrollContentTileIndex { column: 0, row: 0 };
        let Some(bounds) =
            super::ScrollContentTileBounds::for_index(content_bounds, tile_edge, 0, index)
        else {
            return false;
        };
        *backing = PropertyScrollBackingPlan::Tiled(PropertyScrollTiledBackingPlan {
            content_bounds,
            tile_edge,
            gutter: 0,
            overscan: 0,
            tiles: vec![PropertyScrollTilePlan {
                index,
                bounds,
                color_key: single.color_key,
                color_desc: single.color_desc,
                depth_desc: single.depth_desc,
                pair_bytes: single.pair_bytes,
            }],
            total_pair_bytes: single.pair_bytes,
            budget: single.budget,
        });

        let invalid_rejects = rejects_without_declaration(invalid);
        let reordered_rejects = rejects_without_declaration(reordered);
        let hybrid_rejects = rejects_without_declaration(hybrid_phase);
        let tiled_rejects = rejects_without_declaration(tiled);

        let mut collision_keys = FxHashSet::default();
        collision_keys.insert(color_key);
        let mut collision_declared = FxHashSet::default();
        let collision = prepare_retained_property_scroll_boundary_parts(
            source.clone(),
            global_boundary,
            1.0_f32.to_bits(),
            &collision_keys,
            &mut collision_declared,
        );

        let mut success_declared = FxHashSet::default();
        let success = prepare_retained_property_scroll_boundary_parts(
            source,
            global_boundary,
            1.0_f32.to_bits(),
            &FxHashSet::default(),
            &mut success_declared,
        );

        invalid_rejects
            && reordered_rejects
            && hybrid_rejects
            && tiled_rejects
            && matches!(
                collision,
                Err(
                    RetainedPropertyScrollScenePrepareError::PersistentKeyAlreadyDeclared(key)
                ) if key == color_key
            )
            && collision_declared.is_empty()
            && matches!(
                success,
                Ok(PreparedRetainedPropertyScrollBoundaryParts {
                    authority:
                        PreparedRetainedPropertyScrollBoundaryAuthority::AtomicProjectionSelectionTextArea {
                            ..
                        },
                    backing: PreparedRetainedPropertyScrollBacking::Single { .. },
                    ..
                })
            )
            && success_declared.len() == 2
            && success_declared.contains(&color_key)
    }

    #[cfg(test)]
    pub(crate) fn first_single_backing_declaration_for_test(
        &self,
    ) -> Option<(PersistentTextureKey, TextureDesc)> {
        let boundary = self.boundaries.first()?;
        let single = match boundary.planner.steps.get(1)? {
            ScrollBoundaryStep::ContentComposite {
                backing: PropertyScrollBackingPlan::Single(single),
                ..
            }
            | ScrollBoundaryStep::AtomicProjectionContentComposite {
                backing: PropertyScrollBackingPlan::Single(single),
                ..
            }
            | ScrollBoundaryStep::AtomicProjectionSelectionContentComposite {
                backing: PropertyScrollBackingPlan::Single(single),
                ..
            } => single,
            _ => return None,
        };
        Some((single.color_key, single.color_desc.clone()))
    }

    #[cfg(test)]
    pub(crate) fn rejects_synchronized_interactive_caret_width_tamper_for_test(&self) -> bool {
        let Some(boundary) = self.boundaries.first() else {
            return false;
        };
        let PropertyScrollPostCompositeSchedule::InteractiveTextAreaCaret(caret) =
            &boundary.planner.seal.post_composite
        else {
            return false;
        };
        let mut tampered = caret.clone();
        let Some(op) = tampered.recorded.op.as_mut() else {
            return false;
        };
        op.params.size[0] = 2.0;
        let bounds_bits = [
            op.params.position[0],
            op.params.position[1],
            op.params.size[0],
            op.params.size[1],
        ]
        .map(f32::to_bits);
        let Some(payload_identity) = super::PaintPayloadIdentity::prepared_rects([&*op]) else {
            return false;
        };
        tampered.recorded.identity.paint =
            super::RetainedTextAreaCaretOverlayPaintIdentity::Visible {
                bounds_bits,
                payload_identity,
            };
        !tampered.is_canonical()
    }

    #[cfg(test)]
    pub(crate) fn rejects_synchronized_interactive_caret_position_tamper_for_test(&self) -> bool {
        self.rejects_synchronized_interactive_caret_geometry_tamper_for_test(|op| {
            op.params.position[0] += 1.0;
        })
    }

    #[cfg(test)]
    pub(crate) fn rejects_synchronized_interactive_caret_height_tamper_for_test(&self) -> bool {
        self.rejects_synchronized_interactive_caret_geometry_tamper_for_test(|op| {
            op.params.size[1] += 1.0;
        })
    }

    #[cfg(test)]
    fn rejects_synchronized_interactive_caret_geometry_tamper_for_test(
        &self,
        mutate: impl FnOnce(&mut super::DrawRectOp),
    ) -> bool {
        let Some(boundary) = self.boundaries.first() else {
            return false;
        };
        let PropertyScrollPostCompositeSchedule::InteractiveTextAreaCaret(caret) =
            &boundary.planner.seal.post_composite
        else {
            return false;
        };
        let mut tampered = caret.clone();
        let Some(op) = tampered.recorded.op.as_mut() else {
            return false;
        };
        mutate(op);
        let bounds_bits = [
            op.params.position[0],
            op.params.position[1],
            op.params.size[0],
            op.params.size[1],
        ]
        .map(f32::to_bits);
        let Some(payload_identity) = super::PaintPayloadIdentity::prepared_rects([&*op]) else {
            return false;
        };
        tampered.recorded.identity.paint =
            super::RetainedTextAreaCaretOverlayPaintIdentity::Visible {
                bounds_bits,
                payload_identity,
            };
        !tampered.is_canonical()
    }
}

fn collect_exact_property_scroll_reachable_owners(
    arena: &NodeArena,
    root: NodeKey,
    owners: &mut FxHashSet<NodeKey>,
    stable_ids: &mut FxHashSet<u64>,
) -> bool {
    let mut stack = vec![root];
    while let Some(owner) = stack.pop() {
        let Some(node) = arena.get(owner) else {
            return false;
        };
        let stable_id = node.element.stable_id();
        if stable_id == 0
            || !owners.insert(owner)
            || !stable_ids.insert(stable_id)
            || node.element.children() != arena.children_of(owner)
        {
            return false;
        }
        stack.extend(node.element.children().iter().copied());
    }
    true
}

/// Plans and compiler-seals the B4-1 exact forest as one atomic authority.
/// Any non-exact root rejects the complete frame before pool or graph access.
#[allow(clippy::too_many_arguments)]
pub(crate) fn plan_and_validate_property_scroll_scene(
    arena: &NodeArena,
    roots: &[NodeKey],
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    scale_factor: f32,
    incoming_paint_offset: [f32; 2],
    outer_scissor_rect: Option<[u32; 4]>,
    semantic_frame_time: crate::time::Instant,
    target_format: wgpu::TextureFormat,
    budget: ScrollSceneSingleTextureBudget,
) -> Result<ValidatedPropertyScrollScene, PropertyScrollScenePlanError> {
    if roots.is_empty()
        || !scale_factor.is_finite()
        || scale_factor <= 0.0
        || incoming_paint_offset.map(f32::to_bits) != [0.0_f32.to_bits(); 2]
        || outer_scissor_rect.is_some()
        || !property_trees.validation_errors.is_empty()
        || !property_trees.transforms.is_empty()
        || !property_trees.effects.is_empty()
        || property_trees.scrolls.is_empty()
        || property_trees.scrolls.len() > roots.len()
        || property_trees.clips.len() < roots.len()
        || property_trees.clips.len() > roots.len().checked_mul(2).unwrap_or(usize::MAX)
        || property_trees.states.len() < roots.len().checked_mul(2).unwrap_or(usize::MAX)
    {
        return Err(PropertyScrollScenePlanError::InvalidContract);
    }
    if !paint_generations.matches_live_snapshot(arena, roots, property_trees) {
        return Err(PropertyScrollScenePlanError::LiveSnapshotDrift);
    }
    let mut seen_roots = FxHashSet::default();
    let mut reachable_owners = FxHashSet::default();
    let mut reachable_stable_ids = FxHashSet::default();
    let mut expected_clip_count = 0usize;
    let mut boundaries = Vec::with_capacity(roots.len());
    for &root in roots {
        if !seen_roots.insert(root) || arena.parent_of(root).is_some() {
            return Err(PropertyScrollScenePlanError::InvalidContract);
        }
        let scene = plan_exact_root_scroll_scene(
            arena,
            root,
            property_trees,
            paint_generations,
            scale_factor,
            incoming_paint_offset,
            outer_scissor_rect,
        )
        .map_err(PropertyScrollScenePlanError::Frame)?;
        expected_clip_count = expected_clip_count
            .checked_add(
                if scene.admission.text_area_subtree_snapshot().is_some()
                    || scene
                        .admission
                        .interactive_text_area_subtree_snapshot()
                        .is_some()
                    || scene
                        .admission
                        .atomic_projection_text_area_subtree_snapshot()
                        .is_some()
                    || scene
                        .admission
                        .focused_atomic_projection_text_area_subtree_snapshot()
                        .is_some()
                    || scene
                        .admission
                        .atomic_projection_selection_text_area_subtree_snapshot()
                        .is_some()
                {
                    2
                } else {
                    1
                },
            )
            .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        if !collect_exact_property_scroll_reachable_owners(
            arena,
            root,
            &mut reachable_owners,
            &mut reachable_stable_ids,
        ) {
            return Err(PropertyScrollScenePlanError::InvalidContract);
        }
        let plan = property_scroll_plan_from_exact_scene(
            scene,
            scale_factor,
            semantic_frame_time,
            target_format,
            budget,
        )?;
        boundaries.push(
            validate_property_scroll_boundary_from_frozen_plan(plan)
                .map_err(|_| PropertyScrollScenePlanError::InvalidContract)?,
        );
    }
    if property_trees.clips.len() != expected_clip_count
        || property_trees.states.len() != reachable_owners.len()
        || property_trees
            .states
            .keys()
            .any(|owner| !reachable_owners.contains(owner))
    {
        return Err(PropertyScrollScenePlanError::InvalidContract);
    }
    if !paint_generations.matches_live_snapshot(arena, roots, property_trees) {
        return Err(PropertyScrollScenePlanError::LiveSnapshotDrift);
    }
    let seal = property_scroll_scene_seal_from_boundaries(&boundaries)
        .ok_or(PropertyScrollScenePlanError::BackingBudget)?;
    let scene = ValidatedPropertyScrollScene { boundaries, seal };
    scene
        .is_canonical()
        .then_some(scene)
        .ok_or(PropertyScrollScenePlanError::InvalidContract)
}

#[allow(clippy::too_many_arguments)]
fn plan_exact_transform_scroll_boundary(
    arena: &NodeArena,
    receiver: TransformNodeSnapshot,
    boundary: &super::frame_plan::PropertyScrollBoundaryContract,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    scale_factor: f32,
    consumed_effect: Option<super::ConsumedAncestorEffectWitness>,
) -> Result<ScrollScenePlan, FramePaintPlanError> {
    let boundary_root = boundary.scroll.owner;
    let invalid = || FramePaintPlanError {
        reasons: vec![FramePaintPlanRejection::InvalidScrollHost(boundary_root)],
    };
    let node = arena.get(boundary_root).ok_or_else(invalid)?;
    let element = node
        .element
        .as_any()
        .downcast_ref::<crate::view::base_component::Element>()
        .ok_or_else(invalid)?;
    let admission = element
        .exact_retained_transform_scroll_host_admission(
            boundary_root,
            arena.parent_of(boundary_root).ok_or_else(invalid)?,
            arena,
            scale_factor,
        )
        .ok_or_else(invalid)?;
    let scroll = boundary.scroll;
    let contents_clip = boundary.contents_clip;
    if receiver.owner == boundary_root
        || scroll.id.0 != boundary_root
        || scroll.owner != boundary_root
        || scroll.parent.is_some()
        || scroll.generation == 0
        || contents_clip.id.owner != boundary_root
        || contents_clip.id.role != ClipNodeRole::ContentsClip
        || contents_clip.owner != boundary_root
        || contents_clip.parent.is_some()
        || contents_clip.behavior != ClipBehavior::Intersect
        || contents_clip.generation == 0
        || !admission.matches_scroll_node(scroll)
    {
        return Err(invalid());
    }
    let transform =
        super::ConsumedAncestorTransformWitness::new(receiver.owner, boundary_root, receiver.id)
            .ok_or_else(invalid)?;
    let mut host_properties = Vec::with_capacity(2);
    if let Some(effect) = consumed_effect {
        host_properties.push(super::ConsumedAncestorProperty::Effect(
            effect.for_target(boundary_root),
        ));
    }
    host_properties.push(super::ConsumedAncestorProperty::Transform(transform));
    let host_stack =
        super::ConsumedAncestorPropertyStackWitness::new(boundary_root, &host_properties)
            .ok_or_else(invalid)?;
    let baked_witness = super::PaintBakedScrollHostWitness::new(
        boundary_root,
        admission.child,
        scroll,
        contents_clip.id,
    )
    .ok_or_else(invalid)?;
    let baked_artifact = if let Some(effect) = consumed_effect {
        super::frame_recorder::record_effect_baked_scroll_host_artifact_with_stack_for_plan(
            arena,
            &[boundary_root],
            property_trees,
            paint_generations,
            baked_witness,
            host_stack,
            effect.effect,
        )
    } else {
        super::frame_recorder::record_baked_scroll_host_artifact_with_stack_for_plan(
            arena,
            &[boundary_root],
            property_trees,
            paint_generations,
            baked_witness,
            host_stack,
        )
    }
    .map_err(|fallbacks| FramePaintPlanError {
        reasons: fallbacks
            .into_iter()
            .map(FramePaintPlanRejection::Coverage)
            .collect(),
    })?;
    if !super::compiler::validate_baked_scroll_host_artifact_for_plan(
        &baked_artifact,
        boundary_root,
        admission.child,
        scroll,
        contents_clip,
    ) {
        return Err(invalid());
    }
    let [host_chunk, content_chunk, overlay_chunk] = baked_artifact.chunks.as_slice() else {
        return Err(invalid());
    };
    if host_chunk.owner != boundary_root
        || content_chunk.owner != admission.child
        || overlay_chunk.owner != boundary_root
    {
        return Err(invalid());
    }
    let host_before =
        extract_root_scene_chunk(&baked_artifact, 0, boundary_root).ok_or_else(invalid)?;
    let overlay =
        extract_root_scene_chunk(&baked_artifact, 2, boundary_root).ok_or_else(invalid)?;
    let content_witness =
        PaintScrollContentWitness::new(boundary_root, admission.child, scroll, contents_clip)
            .ok_or_else(invalid)?;
    let mut content_properties = Vec::with_capacity(3);
    if let Some(effect) = consumed_effect {
        content_properties.push(super::ConsumedAncestorProperty::Effect(
            effect.for_target(admission.child),
        ));
    }
    content_properties.push(super::ConsumedAncestorProperty::Transform(
        transform.for_target(admission.child),
    ));
    content_properties.push(content_witness.consumed_property());
    let content_stack =
        super::ConsumedAncestorPropertyStackWitness::new(admission.child, &content_properties)
            .ok_or_else(invalid)?;
    let content_local = if let Some(effect) = consumed_effect {
        super::frame_recorder::record_effect_scroll_content_local_artifact_with_stack_for_plan(
            arena,
            property_trees,
            paint_generations,
            content_witness,
            content_stack,
            effect.effect,
        )
    } else {
        super::frame_recorder::record_scroll_content_local_artifact_with_stack_for_plan(
            arena,
            property_trees,
            paint_generations,
            content_witness,
            content_stack,
        )
    }
    .map_err(|fallbacks| FramePaintPlanError {
        reasons: fallbacks
            .into_iter()
            .map(FramePaintPlanRejection::Coverage)
            .collect(),
    })?;
    let host_terminal = opaque_order_count(&host_before);
    let content_terminal = opaque_order_count(&content_local);
    let parent_terminal = host_terminal
        .checked_add(opaque_order_count(&overlay))
        .ok_or_else(invalid)?;
    let admission = PropertyScrollHostAdmission::direct_leaf(admission);
    Ok(ScrollScenePlan {
        boundary_root,
        root_stable_id: admission.stable_id,
        content_root: admission.child,
        content_stable_id: admission.child_stable_id,
        admission: admission.clone(),
        text_area_subtree_admission: None,
        interactive_text_area_subtree_admission: None,
        atomic_projection_text_area_subtree_admission: None,
        focused_atomic_projection_text_area_subtree_admission: None,
        post_composite: PropertyScrollPostCompositeSchedule::NoneForExistingGrammar,
        interactive_resident: None,
        atomic_projection_resident: None,
        scroll,
        contents_clip,
        planned_admission_witness: admission,
        planned_text_area_subtree_admission: None,
        planned_interactive_text_area_subtree_admission: None,
        planned_atomic_projection_text_area_subtree_admission: None,
        planned_focused_atomic_projection_text_area_subtree_admission: None,
        planned_post_composite: PropertyScrollPostCompositeSchedule::NoneForExistingGrammar,
        planned_interactive_resident: None,
        planned_atomic_projection_resident: None,
        planned_scroll_witness: scroll,
        planned_clip_witness: contents_clip,
        recorded: ScrollSceneRecordedAuthority::Existing {
            host_before,
            content_local,
            overlay,
            host_parent_span: 0..host_terminal,
            content_local_span: 0..content_terminal,
            overlay_parent_span: host_terminal..parent_terminal,
        },
    })
}

#[allow(clippy::too_many_arguments)]
fn plan_exact_same_owner_transform_scroll_boundary(
    arena: &NodeArena,
    receiver: TransformNodeSnapshot,
    boundary: &super::frame_plan::PropertyScrollBoundaryContract,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    scale_factor: f32,
) -> Result<ScrollScenePlan, FramePaintPlanError> {
    let boundary_root = boundary.scroll.owner;
    let invalid = || FramePaintPlanError {
        reasons: vec![FramePaintPlanRejection::InvalidScrollHost(boundary_root)],
    };
    let node = arena.get(boundary_root).ok_or_else(invalid)?;
    let element = node
        .element
        .as_any()
        .downcast_ref::<crate::view::base_component::Element>()
        .ok_or_else(invalid)?;
    let admission = element
        .exact_retained_same_owner_transform_scroll_host_admission(
            boundary_root,
            arena,
            scale_factor,
        )
        .ok_or_else(invalid)?;
    let scroll = boundary.scroll;
    let contents_clip = boundary.contents_clip;
    if receiver.owner != boundary_root
        || receiver.id.0 != boundary_root
        || receiver.parent.is_some()
        || receiver.generation == 0
        || scroll.id.0 != boundary_root
        || scroll.owner != boundary_root
        || scroll.parent.is_some()
        || scroll.generation == 0
        || contents_clip.id.owner != boundary_root
        || contents_clip.id.role != ClipNodeRole::ContentsClip
        || contents_clip.owner != boundary_root
        || contents_clip.parent.is_some()
        || contents_clip.behavior != ClipBehavior::Intersect
        || contents_clip.generation == 0
        || !admission.matches_scroll_node(scroll)
    {
        return Err(invalid());
    }
    let consumed_transform =
        super::ConsumedSameOwnerTransformBoundaryWitness::new(receiver.owner, receiver.id)
            .ok_or_else(invalid)?;
    let baked_witness = super::PaintBakedScrollHostWitness::new(
        boundary_root,
        admission.child,
        scroll,
        contents_clip.id,
    )
    .ok_or_else(invalid)?;
    let baked_artifact =
        super::frame_recorder::record_same_owner_transform_scroll_host_artifact_for_plan(
            arena,
            &[boundary_root],
            property_trees,
            paint_generations,
            baked_witness,
            consumed_transform,
        )
        .map_err(|fallbacks| FramePaintPlanError {
            reasons: fallbacks
                .into_iter()
                .map(FramePaintPlanRejection::Coverage)
                .collect(),
        })?;
    if !super::compiler::validate_baked_scroll_host_artifact_for_plan(
        &baked_artifact,
        boundary_root,
        admission.child,
        scroll,
        contents_clip,
    ) {
        return Err(invalid());
    }
    let [host_chunk, content_chunk, overlay_chunk] = baked_artifact.chunks.as_slice() else {
        return Err(invalid());
    };
    if host_chunk.owner != boundary_root
        || content_chunk.owner != admission.child
        || overlay_chunk.owner != boundary_root
    {
        return Err(invalid());
    }
    let host_before =
        extract_root_scene_chunk(&baked_artifact, 0, boundary_root).ok_or_else(invalid)?;
    let overlay =
        extract_root_scene_chunk(&baked_artifact, 2, boundary_root).ok_or_else(invalid)?;
    let content_witness =
        PaintScrollContentWitness::new(boundary_root, admission.child, scroll, contents_clip)
            .ok_or_else(invalid)?;
    let consumed_scroll = super::ConsumedAncestorScrollContentsWitness::new(
        boundary_root,
        admission.child,
        scroll.id,
        contents_clip.id,
    )
    .ok_or_else(invalid)?;
    let content_stack =
        super::ConsumedAncestorPropertyStackWitness::new_same_owner_transform_scroll(
            admission.child,
            consumed_transform,
            consumed_scroll,
        )
        .ok_or_else(invalid)?;
    let content_local =
        super::frame_recorder::record_scroll_content_local_artifact_with_stack_for_plan(
            arena,
            property_trees,
            paint_generations,
            content_witness,
            content_stack,
        )
        .map_err(|fallbacks| FramePaintPlanError {
            reasons: fallbacks
                .into_iter()
                .map(FramePaintPlanRejection::Coverage)
                .collect(),
        })?;
    let host_terminal = opaque_order_count(&host_before);
    let content_terminal = opaque_order_count(&content_local);
    let parent_terminal = host_terminal
        .checked_add(opaque_order_count(&overlay))
        .ok_or_else(invalid)?;
    let admission = PropertyScrollHostAdmission::direct_leaf(admission);
    Ok(ScrollScenePlan {
        boundary_root,
        root_stable_id: admission.stable_id,
        content_root: admission.child,
        content_stable_id: admission.child_stable_id,
        admission: admission.clone(),
        text_area_subtree_admission: None,
        interactive_text_area_subtree_admission: None,
        atomic_projection_text_area_subtree_admission: None,
        focused_atomic_projection_text_area_subtree_admission: None,
        post_composite: PropertyScrollPostCompositeSchedule::NoneForExistingGrammar,
        interactive_resident: None,
        atomic_projection_resident: None,
        scroll,
        contents_clip,
        planned_admission_witness: admission,
        planned_text_area_subtree_admission: None,
        planned_interactive_text_area_subtree_admission: None,
        planned_atomic_projection_text_area_subtree_admission: None,
        planned_focused_atomic_projection_text_area_subtree_admission: None,
        planned_post_composite: PropertyScrollPostCompositeSchedule::NoneForExistingGrammar,
        planned_interactive_resident: None,
        planned_atomic_projection_resident: None,
        planned_scroll_witness: scroll,
        planned_clip_witness: contents_clip,
        recorded: ScrollSceneRecordedAuthority::Existing {
            host_before,
            content_local,
            overlay,
            host_parent_span: 0..host_terminal,
            content_local_span: 0..content_terminal,
            overlay_parent_span: host_terminal..parent_terminal,
        },
    })
}

#[allow(clippy::too_many_arguments)]
fn plan_exact_same_owner_effect_scroll_boundary(
    arena: &NodeArena,
    insertion: &super::frame_plan::PropertySameOwnerEffectScrollReceiverInsertionContract,
    boundary: &super::frame_plan::PropertyScrollBoundaryContract,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    scale_factor: f32,
) -> Result<ScrollScenePlan, FramePaintPlanError> {
    let receiver = insertion.effect;
    let boundary_root = boundary.scroll.owner;
    let invalid = || FramePaintPlanError {
        reasons: vec![FramePaintPlanRejection::InvalidScrollHost(boundary_root)],
    };
    let node = arena.get(boundary_root).ok_or_else(invalid)?;
    let element = node
        .element
        .as_any()
        .downcast_ref::<crate::view::base_component::Element>()
        .ok_or_else(invalid)?;
    let admission = element
        .exact_retained_same_owner_effect_scroll_host_admission(boundary_root, arena, scale_factor)
        .ok_or_else(invalid)?;
    let scroll = boundary.scroll;
    let contents_clip = boundary.contents_clip;
    if !insertion.is_canonical()
        || insertion.owner != boundary_root
        || insertion.scroll != scroll
        || insertion.contents_clip != contents_clip
        || insertion.content_root != admission.child
        || insertion.content_stable_id != admission.child_stable_id
        || receiver.owner != boundary_root
        || receiver.id.0 != boundary_root
        || receiver.parent.is_some()
        || receiver.generation == 0
        || !receiver.opacity.is_finite()
        || !(0.0..=1.0).contains(&receiver.opacity)
        || scroll.id.0 != boundary_root
        || scroll.owner != boundary_root
        || scroll.parent.is_some()
        || scroll.generation == 0
        || contents_clip.id.owner != boundary_root
        || contents_clip.id.role != ClipNodeRole::ContentsClip
        || contents_clip.owner != boundary_root
        || contents_clip.parent.is_some()
        || contents_clip.behavior != ClipBehavior::Intersect
        || contents_clip.generation == 0
        || !admission.matches_scroll_node(scroll)
    {
        return Err(invalid());
    }
    let consumed_effect =
        super::ConsumedSameOwnerEffectBoundaryWitness::new(receiver.owner, receiver)
            .ok_or_else(invalid)?;
    let baked_witness = super::PaintBakedScrollHostWitness::new(
        boundary_root,
        admission.child,
        scroll,
        contents_clip.id,
    )
    .ok_or_else(invalid)?;
    let baked_artifact =
        super::frame_recorder::record_same_owner_effect_scroll_host_artifact_for_plan(
            arena,
            &[boundary_root],
            property_trees,
            paint_generations,
            baked_witness,
            &insertion.receiver.artifact_contract,
            consumed_effect,
        )
        .map_err(|fallbacks| FramePaintPlanError {
            reasons: fallbacks
                .into_iter()
                .map(FramePaintPlanRejection::Coverage)
                .collect(),
        })?;
    if !super::compiler::validate_baked_scroll_host_artifact_for_plan(
        &baked_artifact,
        boundary_root,
        admission.child,
        scroll,
        contents_clip,
    ) {
        return Err(invalid());
    }
    let [host_chunk, content_chunk, overlay_chunk] = baked_artifact.chunks.as_slice() else {
        return Err(invalid());
    };
    if host_chunk.owner != boundary_root
        || content_chunk.owner != admission.child
        || overlay_chunk.owner != boundary_root
    {
        return Err(invalid());
    }
    let host_before =
        extract_root_scene_chunk(&baked_artifact, 0, boundary_root).ok_or_else(invalid)?;
    let overlay =
        extract_root_scene_chunk(&baked_artifact, 2, boundary_root).ok_or_else(invalid)?;
    let content_witness =
        PaintScrollContentWitness::new(boundary_root, admission.child, scroll, contents_clip)
            .ok_or_else(invalid)?;
    let consumed_scroll = super::ConsumedAncestorScrollContentsWitness::new(
        boundary_root,
        admission.child,
        scroll.id,
        contents_clip.id,
    )
    .ok_or_else(invalid)?;
    let content_stack = super::ConsumedAncestorPropertyStackWitness::new_same_owner_effect_scroll(
        admission.child,
        consumed_effect,
        consumed_scroll,
    )
    .ok_or_else(invalid)?;
    let content_local =
        super::frame_recorder::record_effect_scroll_content_local_artifact_with_stack_for_plan(
            arena,
            property_trees,
            paint_generations,
            content_witness,
            content_stack,
            receiver,
        )
        .map_err(|fallbacks| FramePaintPlanError {
            reasons: fallbacks
                .into_iter()
                .map(FramePaintPlanRejection::Coverage)
                .collect(),
        })?;
    let host_terminal = opaque_order_count(&host_before);
    let content_terminal = opaque_order_count(&content_local);
    let parent_terminal = host_terminal
        .checked_add(opaque_order_count(&overlay))
        .ok_or_else(invalid)?;
    let admission = PropertyScrollHostAdmission::direct_leaf(admission);
    Ok(ScrollScenePlan {
        boundary_root,
        root_stable_id: admission.stable_id,
        content_root: admission.child,
        content_stable_id: admission.child_stable_id,
        admission: admission.clone(),
        text_area_subtree_admission: None,
        interactive_text_area_subtree_admission: None,
        atomic_projection_text_area_subtree_admission: None,
        focused_atomic_projection_text_area_subtree_admission: None,
        post_composite: PropertyScrollPostCompositeSchedule::NoneForExistingGrammar,
        interactive_resident: None,
        atomic_projection_resident: None,
        scroll,
        contents_clip,
        planned_admission_witness: admission,
        planned_text_area_subtree_admission: None,
        planned_interactive_text_area_subtree_admission: None,
        planned_atomic_projection_text_area_subtree_admission: None,
        planned_focused_atomic_projection_text_area_subtree_admission: None,
        planned_post_composite: PropertyScrollPostCompositeSchedule::NoneForExistingGrammar,
        planned_interactive_resident: None,
        planned_atomic_projection_resident: None,
        planned_scroll_witness: scroll,
        planned_clip_witness: contents_clip,
        recorded: ScrollSceneRecordedAuthority::Existing {
            host_before,
            content_local,
            overlay,
            host_parent_span: 0..host_terminal,
            content_local_span: 0..content_terminal,
            overlay_parent_span: host_terminal..parent_terminal,
        },
    })
}

#[allow(clippy::too_many_arguments)]
#[allow(dead_code)]
fn plan_exact_effect_scroll_boundary_checkpoint(
    arena: &NodeArena,
    receiver: EffectNodeSnapshot,
    boundary: &super::frame_plan::PropertyScrollBoundaryContract,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    scale_factor: f32,
    consumed_transform: Option<super::ConsumedAncestorTransformWitness>,
) -> Result<ScrollScenePlan, FramePaintPlanError> {
    let boundary_root = boundary.scroll.owner;
    let invalid = || FramePaintPlanError {
        reasons: vec![FramePaintPlanRejection::InvalidScrollHost(boundary_root)],
    };
    let node = arena.get(boundary_root).ok_or_else(invalid)?;
    let element = node
        .element
        .as_any()
        .downcast_ref::<crate::view::base_component::Element>()
        .ok_or_else(invalid)?;
    let admission = element
        .exact_retained_transform_scroll_host_admission(
            boundary_root,
            receiver.owner,
            arena,
            scale_factor,
        )
        .ok_or_else(invalid)?;
    let scroll = boundary.scroll;
    let contents_clip = boundary.contents_clip;
    if receiver.id.0 != receiver.owner
        || receiver.parent.is_some()
        || receiver.generation == 0
        || !receiver.opacity.is_finite()
        || !(0.0..=1.0).contains(&receiver.opacity)
        || receiver.owner == boundary_root
        || arena.parent_of(boundary_root) != Some(receiver.owner)
        || arena.children_of(receiver.owner) != [boundary_root]
        || !admission.matches_scroll_node(scroll)
    {
        return Err(invalid());
    }
    let effect = super::ConsumedAncestorEffectWitness::new(
        receiver.owner,
        boundary_root,
        receiver,
        Some(receiver.id),
        receiver.parent,
    )
    .ok_or_else(invalid)?;
    let mut host_properties = Vec::with_capacity(2);
    if let Some(transform) = consumed_transform {
        host_properties.push(super::ConsumedAncestorProperty::Transform(
            transform.for_target(boundary_root),
        ));
    }
    host_properties.push(super::ConsumedAncestorProperty::Effect(effect));
    let host_stack =
        super::ConsumedAncestorPropertyStackWitness::new(boundary_root, &host_properties)
            .ok_or_else(invalid)?;
    let baked_witness = super::PaintBakedScrollHostWitness::new(
        boundary_root,
        admission.child,
        scroll,
        contents_clip.id,
    )
    .ok_or_else(invalid)?;
    let baked_artifact =
        super::frame_recorder::record_effect_baked_scroll_host_artifact_with_stack_for_plan(
            arena,
            &[boundary_root],
            property_trees,
            paint_generations,
            baked_witness,
            host_stack,
            receiver,
        )
        .map_err(|fallbacks| FramePaintPlanError {
            reasons: fallbacks
                .into_iter()
                .map(FramePaintPlanRejection::Coverage)
                .collect(),
        })?;
    if !super::compiler::validate_baked_scroll_host_artifact_for_plan(
        &baked_artifact,
        boundary_root,
        admission.child,
        scroll,
        contents_clip,
    ) {
        return Err(invalid());
    }
    let [host_chunk, content_chunk, overlay_chunk] = baked_artifact.chunks.as_slice() else {
        return Err(invalid());
    };
    if host_chunk.owner != boundary_root
        || content_chunk.owner != admission.child
        || overlay_chunk.owner != boundary_root
    {
        return Err(invalid());
    }
    let host_before =
        extract_root_scene_chunk(&baked_artifact, 0, boundary_root).ok_or_else(invalid)?;
    let overlay =
        extract_root_scene_chunk(&baked_artifact, 2, boundary_root).ok_or_else(invalid)?;
    let content_witness =
        PaintScrollContentWitness::new(boundary_root, admission.child, scroll, contents_clip)
            .ok_or_else(invalid)?;
    let mut content_properties = Vec::with_capacity(3);
    if let Some(transform) = consumed_transform {
        content_properties.push(super::ConsumedAncestorProperty::Transform(
            transform.for_target(admission.child),
        ));
    }
    content_properties.push(super::ConsumedAncestorProperty::Effect(
        effect.for_target(admission.child),
    ));
    content_properties.push(content_witness.consumed_property());
    let content_stack =
        super::ConsumedAncestorPropertyStackWitness::new(admission.child, &content_properties)
            .ok_or_else(invalid)?;
    let content_local =
        super::frame_recorder::record_effect_scroll_content_local_artifact_with_stack_for_plan(
            arena,
            property_trees,
            paint_generations,
            content_witness,
            content_stack,
            receiver,
        )
        .map_err(|fallbacks| FramePaintPlanError {
            reasons: fallbacks
                .into_iter()
                .map(FramePaintPlanRejection::Coverage)
                .collect(),
        })?;
    let host_terminal = opaque_order_count(&host_before);
    let content_terminal = opaque_order_count(&content_local);
    let parent_terminal = host_terminal
        .checked_add(opaque_order_count(&overlay))
        .ok_or_else(invalid)?;
    let admission = PropertyScrollHostAdmission::direct_leaf(admission);
    Ok(ScrollScenePlan {
        boundary_root,
        root_stable_id: admission.stable_id,
        content_root: admission.child,
        content_stable_id: admission.child_stable_id,
        admission: admission.clone(),
        text_area_subtree_admission: None,
        interactive_text_area_subtree_admission: None,
        atomic_projection_text_area_subtree_admission: None,
        focused_atomic_projection_text_area_subtree_admission: None,
        post_composite: PropertyScrollPostCompositeSchedule::NoneForExistingGrammar,
        interactive_resident: None,
        atomic_projection_resident: None,
        scroll,
        contents_clip,
        planned_admission_witness: admission,
        planned_text_area_subtree_admission: None,
        planned_interactive_text_area_subtree_admission: None,
        planned_atomic_projection_text_area_subtree_admission: None,
        planned_focused_atomic_projection_text_area_subtree_admission: None,
        planned_post_composite: PropertyScrollPostCompositeSchedule::NoneForExistingGrammar,
        planned_interactive_resident: None,
        planned_atomic_projection_resident: None,
        planned_scroll_witness: scroll,
        planned_clip_witness: contents_clip,
        recorded: ScrollSceneRecordedAuthority::Existing {
            host_before,
            content_local,
            overlay,
            host_parent_span: 0..host_terminal,
            content_local_span: 0..content_terminal,
            overlay_parent_span: host_terminal..parent_terminal,
        },
    })
}

fn transform_scroll_receiver_raster_bounds(
    receiver_steps: &[super::frame_recorder::RecordedTransformSurfaceStep],
    scroll_host_bounds: RetainedSurfaceBounds,
) -> Option<RetainedSurfaceBounds> {
    let mut min_x = scroll_host_bounds.x;
    let mut min_y = scroll_host_bounds.y;
    let mut max_x = scroll_host_bounds.x + scroll_host_bounds.width;
    let mut max_y = scroll_host_bounds.y + scroll_host_bounds.height;
    for step in receiver_steps {
        let super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact) = step else {
            continue;
        };
        for chunk in &artifact.chunks {
            let bounds = chunk.bounds;
            let right = bounds.x + bounds.width;
            let bottom = bounds.y + bounds.height;
            if [bounds.x, bounds.y, right, bottom]
                .iter()
                .any(|value| !value.is_finite())
                || bounds.width < 0.0
                || bounds.height < 0.0
            {
                return None;
            }
            min_x = min_x.min(bounds.x);
            min_y = min_y.min(bounds.y);
            max_x = max_x.max(right);
            max_y = max_y.max(bottom);
        }
    }
    let bounds = RetainedSurfaceBounds {
        x: min_x,
        y: min_y,
        width: max_x - min_x,
        height: max_y - min_y,
        corner_radii: [0.0; 4],
    };
    (bounds.x.is_finite()
        && bounds.y.is_finite()
        && bounds.width.is_finite()
        && bounds.height.is_finite()
        && bounds.x >= 0.0
        && bounds.y >= 0.0
        && bounds.width > 0.0
        && bounds.height > 0.0)
        .then_some(bounds)
}

/// Plans and recorder-seals the exact direct `ScrollContents -> Transform`
/// schedule.  This S1 scaffold owns no backing or transaction yet; later
/// phases may proceed only from this canonical frozen result.
#[allow(clippy::too_many_arguments)]
pub(crate) fn plan_direct_scroll_transform_scene_scaffold(
    arena: &NodeArena,
    roots: &[NodeKey],
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    scale_factor: f32,
    incoming_paint_offset: [f32; 2],
    outer_scissor_rect: Option<[u32; 4]>,
) -> Result<DirectScrollTransformSceneScaffold, PropertyScrollScenePlanError> {
    let [root] = roots else {
        return Err(PropertyScrollScenePlanError::InvalidContract);
    };
    if !scale_factor.is_finite()
        || scale_factor <= 0.0
        || incoming_paint_offset.map(f32::to_bits) != [0.0_f32.to_bits(); 2]
        || outer_scissor_rect.is_some()
        || !property_trees.validation_errors.is_empty()
        || !property_trees.effects.is_empty()
        || property_trees.transforms.len() != 1
        || property_trees.scrolls.len() != 1
        || property_trees.clips.len() != 1
        || property_trees.states.len() != 2
        || arena.parent_of(*root).is_some()
        || !paint_generations.matches_live_snapshot(arena, roots, property_trees)
    {
        return Err(PropertyScrollScenePlanError::InvalidContract);
    }
    let root_node = arena
        .get(*root)
        .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
    let root_element = root_node
        .element
        .as_any()
        .downcast_ref::<crate::view::base_component::Element>()
        .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
    let admission = root_element
        .exact_retained_scroll_transform_host_admission(*root, arena, scale_factor)
        .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
    let child = admission.transform_content;
    let scroll_id = ScrollNodeId(*root);
    let clip_id = ClipNodeId {
        owner: *root,
        role: ClipNodeRole::ContentsClip,
    };
    let transform_id = TransformNodeId(child);
    let scroll = property_trees
        .scroll_snapshot_for(scroll_id)
        .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
    let contents_clip = property_trees
        .clip_snapshot_for(Some(clip_id))
        .and_then(|chain| (chain.len() == 1).then(|| chain[0]))
        .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
    let transform = property_trees
        .transform_snapshot_for(transform_id)
        .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
    let scroll_contents = PropertyTreeState {
        clip: Some(clip_id),
        scroll: Some(scroll_id),
        ..Default::default()
    };
    let transformed_contents = PropertyTreeState {
        transform: Some(transform_id),
        ..scroll_contents
    };
    if !admission.matches_scroll_node(scroll)
        || scroll.owner != *root
        || scroll.parent.is_some()
        || scroll.generation == 0
        || !scroll.has_canonical_vertical_geometry_with_contents_clip(contents_clip)
        || contents_clip.id != clip_id
        || contents_clip.owner != *root
        || contents_clip.parent.is_some()
        || contents_clip.generation == 0
        || contents_clip.behavior != ClipBehavior::Intersect
        || transform.id != transform_id
        || transform.owner != child
        || transform.parent.is_some()
        || transform.generation == 0
        || super::compiler::direct_translation_bits(transform.viewport_matrix).is_none()
        || property_trees.states.get(root).is_none_or(|state| {
            state.paint != PropertyTreeState::default() || state.descendants != scroll_contents
        })
        || property_trees.states.get(&child).is_none_or(|state| {
            state.paint != transformed_contents || state.descendants != transformed_contents
        })
    {
        return Err(PropertyScrollScenePlanError::InvalidContract);
    }
    let insertion = super::PlannedBoundary {
        root: child,
        stable_id: admission.transform_content_stable_id,
        kind: super::PlannedBoundaryKind::Transform(transform_id),
    };
    let host_witness =
        super::PaintBakedScrollHostWitness::new(*root, child, scroll, contents_clip.id)
            .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
    let host_steps = super::frame_recorder::record_scroll_transform_host_steps_for_plan(
        arena,
        *root,
        property_trees,
        paint_generations,
        host_witness,
        incoming_paint_offset,
        insertion,
    )
    .map_err(|fallbacks| {
        PropertyScrollScenePlanError::Frame(FramePaintPlanError {
            reasons: fallbacks
                .into_iter()
                .map(FramePaintPlanRejection::Coverage)
                .collect(),
        })
    })?;
    let content_witness = PaintScrollContentWitness::new(*root, child, scroll, contents_clip)
        .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
    let content_steps = super::frame_recorder::record_scroll_transform_content_steps_for_plan(
        arena,
        child,
        property_trees,
        paint_generations,
        super::PaintTransformSurfaceWitness::canonical_root(child),
        content_witness,
    )
    .map_err(|fallbacks| {
        PropertyScrollScenePlanError::Frame(FramePaintPlanError {
            reasons: fallbacks
                .into_iter()
                .map(FramePaintPlanRejection::Coverage)
                .collect(),
        })
    })?;
    if !paint_generations.matches_live_snapshot(arena, roots, property_trees) {
        return Err(PropertyScrollScenePlanError::LiveSnapshotDrift);
    }
    let [
        super::frame_recorder::RecordedTransformSurfaceStep::Artifact(host_before),
        super::frame_recorder::RecordedTransformSurfaceStep::Boundary(_),
        super::frame_recorder::RecordedTransformSurfaceStep::Artifact(overlay_after),
    ] = host_steps.as_slice()
    else {
        return Err(PropertyScrollScenePlanError::InvalidContract);
    };
    let [super::frame_recorder::RecordedTransformSurfaceStep::Artifact(content_artifact)] =
        content_steps.as_slice()
    else {
        return Err(PropertyScrollScenePlanError::InvalidContract);
    };
    let host_before_identity = PropertyScrollPhaseArtifactIdentity::from_artifact(host_before)
        .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
    let overlay_after_identity = PropertyScrollPhaseArtifactIdentity::from_artifact(overlay_after)
        .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
    let content_identity =
        super::frame_plan::property_scroll_receiver_artifact_identity(content_artifact)
            .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
    let scaffold = DirectScrollTransformSceneScaffold {
        schedule: [
            DirectScrollTransformScheduledStep::ScrollContents {
                owner: *root,
                scroll: scroll_id,
            },
            DirectScrollTransformScheduledStep::TransformContent {
                owner: child,
                transform: transform_id,
            },
        ],
        admission: admission.clone(),
        scroll,
        contents_clip,
        transform,
        insertion,
        host_steps,
        content_steps,
        host_before_identity: host_before_identity.clone(),
        overlay_after_identity: overlay_after_identity.clone(),
        content_identity: content_identity.clone(),
        planned_admission: admission,
        planned_scroll: scroll,
        planned_contents_clip: contents_clip,
        planned_transform: transform,
        planned_insertion: insertion,
        planned_host_before_identity: host_before_identity,
        planned_overlay_after_identity: overlay_after_identity,
        planned_content_identity: content_identity,
        scale_factor_bits: scale_factor.to_bits(),
    };
    scaffold
        .is_canonical()
        .then_some(scaffold)
        .ok_or(PropertyScrollScenePlanError::InvalidContract)
}

fn direct_scroll_transform_artifact_raster_bounds(
    artifact: &PaintArtifact,
    transform: TransformNodeId,
) -> Option<RetainedSurfaceBounds> {
    let first = artifact.chunks.first()?;
    if artifact.chunks.iter().any(|chunk| {
        chunk.properties.transform != Some(transform)
            || chunk.properties.scroll.is_some()
            || chunk.properties.clip.is_some()
            || chunk.properties.effect.is_some()
    }) {
        return None;
    }
    let mut min_x = first.bounds.x;
    let mut min_y = first.bounds.y;
    let mut max_x = first.bounds.x + first.bounds.width;
    let mut max_y = first.bounds.y + first.bounds.height;
    for chunk in &artifact.chunks {
        let right = chunk.bounds.x + chunk.bounds.width;
        let bottom = chunk.bounds.y + chunk.bounds.height;
        if [chunk.bounds.x, chunk.bounds.y, right, bottom]
            .into_iter()
            .any(|value| !value.is_finite())
            || chunk.bounds.width < 0.0
            || chunk.bounds.height < 0.0
        {
            return None;
        }
        min_x = min_x.min(chunk.bounds.x);
        min_y = min_y.min(chunk.bounds.y);
        max_x = max_x.max(right);
        max_y = max_y.max(bottom);
    }
    let bounds = RetainedSurfaceBounds {
        x: min_x,
        y: min_y,
        width: max_x - min_x,
        height: max_y - min_y,
        corner_radii: [0.0; 4],
    };
    (bounds.x.is_finite()
        && bounds.y.is_finite()
        && bounds.width.is_finite()
        && bounds.height.is_finite()
        && bounds.x >= 0.0
        && bounds.y >= 0.0
        && bounds.width > 0.0
        && bounds.height > 0.0)
        .then_some(bounds)
}

/// S2 freezes offset-zero raster bounds, direct quad/UV/scissor geometry,
/// and one non-tiled persistent descriptor pair.  Oversize scenes fail
/// closed instead of falling through to the existing scroll tiler.
pub(crate) fn plan_direct_scroll_transform_geometry(
    arena: &NodeArena,
    scaffold: DirectScrollTransformSceneScaffold,
    target_format: wgpu::TextureFormat,
    budget: ScrollSceneSingleTextureBudget,
) -> Result<DirectScrollTransformGeometryPlan, PropertyScrollScenePlanError> {
    if !scaffold.is_canonical()
        || budget.max_dimension_2d == 0
        || budget.max_pair_bytes == 0
        || scaffold.admission.transform_content_stable_id == 0
    {
        return Err(PropertyScrollScenePlanError::InvalidContract);
    }
    let [super::frame_recorder::RecordedTransformSurfaceStep::Artifact(content_artifact)] =
        scaffold.content_steps.as_slice()
    else {
        return Err(PropertyScrollScenePlanError::InvalidContract);
    };
    if super::compiler::validate_transform_property_surface_artifact_for_plan(
        content_artifact,
        scaffold.admission.transform_content,
        scaffold.transform.id,
    )
    .is_none()
    {
        return Err(PropertyScrollScenePlanError::InvalidContract);
    }
    let raster_bounds =
        direct_scroll_transform_artifact_raster_bounds(content_artifact, scaffold.transform.id)
            .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
    let expected_bounds = content_zero_bounds(scaffold.scroll);
    if bounds_bits(raster_bounds) != bounds_bits(expected_bounds)
        || exact_dpr1_u32_bounds(raster_bounds).is_none()
    {
        return Err(PropertyScrollScenePlanError::InvalidContract);
    }
    let content_node = arena
        .get(scaffold.admission.transform_content)
        .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
    let content_element = content_node
        .element
        .as_any()
        .downcast_ref::<crate::view::base_component::Element>()
        .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
    if content_element.stable_id() != scaffold.admission.transform_content_stable_id {
        return Err(PropertyScrollScenePlanError::LiveSnapshotDrift);
    }
    let transform_geometry = content_element
        .exact_transform_receiver_geometry_snapshot_for_presnapped_raster_bounds(
            raster_bounds,
            None,
        )
        .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
    if transform_geometry
        .viewport_transform
        .to_cols_array()
        .map(f32::to_bits)
        != scaffold
            .transform
            .viewport_matrix
            .to_cols_array()
            .map(f32::to_bits)
    {
        return Err(PropertyScrollScenePlanError::LiveSnapshotDrift);
    }
    let geometry = super::PreparedScrollTransformContentCompositeGeometry::new(
        raster_bounds,
        transform_geometry,
        scaffold.transform,
        scaffold.scroll,
        scaffold.contents_clip,
    )
    .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
    let color_key = crate::view::base_component::transformed_layer_stable_key(
        scaffold.admission.transform_content_stable_id,
    );
    let color = texture_desc_for_logical_bounds(
        raster_bounds,
        f32::from_bits(scaffold.scale_factor_bits),
        None,
        target_format,
    );
    let (color_desc, depth_desc) = persistent_target_texture_descriptors(color, color_key);
    let pair_bytes = canonical_pair_bytes(&color_desc, &depth_desc)
        .ok_or(PropertyScrollScenePlanError::BackingBudget)?;
    if [
        color_desc.width(),
        color_desc.height(),
        depth_desc.width(),
        depth_desc.height(),
    ]
    .into_iter()
    .any(|dimension| dimension > budget.max_dimension_2d)
        || pair_bytes > budget.max_pair_bytes
    {
        return Err(PropertyScrollScenePlanError::BackingBudget);
    }
    let source_bounds_bits = bounds_bits(raster_bounds);
    let backing = DirectScrollTransformSingleBackingPlan {
        color_key,
        color_desc,
        depth_desc,
        pair_bytes,
        source_bounds_bits,
        max_dimension_2d: budget.max_dimension_2d,
        max_pair_bytes: budget.max_pair_bytes,
    };
    let plan = DirectScrollTransformGeometryPlan {
        scaffold,
        raster_bounds,
        geometry,
        backing: backing.clone(),
        planned_raster_bounds_bits: source_bounds_bits,
        planned_geometry: geometry,
        planned_backing: backing,
    };
    plan.is_canonical()
        .then_some(plan)
        .ok_or(PropertyScrollScenePlanError::InvalidContract)
}

#[derive(Clone, Debug)]
pub(crate) struct ValidatedDirectScrollTransformTransaction {
    plan: DirectScrollTransformGeometryPlan,
    stamp: RetainedSurfaceRasterStamp,
    transaction: RetainedPropertyScrollSceneTransaction,
    content_opaque_terminal: u32,
}

impl ValidatedDirectScrollTransformTransaction {
    pub(crate) fn is_canonical(&self) -> bool {
        let RetainedPropertyScrollGenericAuthority::ScrollTransformDirectCompiler(contract) =
            &self.transaction.generic_authority
        else {
            return false;
        };
        self.plan.is_canonical()
            && self.transaction.is_canonical()
            && contract.scene_root == self.plan.scaffold.admission.boundary_root
            && contract.scene_root_stable_id == self.plan.scaffold.admission.stable_id
            && contract.transform == self.plan.scaffold.transform.id
            && contract.transform_stable_id
                == self.plan.scaffold.admission.transform_content_stable_id
            && contract.source_bounds_bits == self.plan.backing.source_bounds_bits
            && self.stamp.target.color == self.plan.backing.color_desc
            && self.stamp.target.depth == self.plan.backing.depth_desc
            && self.stamp.target.scale_factor_bits == self.plan.scaffold.scale_factor_bits
            && self.stamp.identity.color_key == self.plan.backing.color_key
            && self.transaction.generic_stamps() == [self.stamp.clone()]
            && self.transaction.scroll_groups().is_empty()
            && self.stamp.opaque_order_span == (0..self.content_opaque_terminal)
    }

    #[cfg(test)]
    pub(crate) fn transaction_shape_for_test(&self) -> [usize; 6] {
        [
            self.transaction.seal.roots.len(),
            self.transaction.seal.ordered_boundaries.len(),
            self.transaction.generic_full_set.len(),
            self.transaction.seal.generic_bindings.len(),
            self.transaction.scroll_groups.len(),
            self.transaction.seal.scroll_bindings.len(),
        ]
    }

    #[cfg(test)]
    pub(crate) fn inner_transaction_is_canonical_for_test(&self) -> bool {
        self.transaction.is_canonical()
    }

    #[cfg(test)]
    pub(crate) fn stamp_for_test(&self) -> &RetainedSurfaceRasterStamp {
        &self.stamp
    }

    #[cfg(test)]
    pub(crate) fn backing_for_test(&self) -> (PersistentTextureKey, TextureDesc, TextureDesc) {
        (
            self.plan.backing.color_key,
            self.plan.backing.color_desc.clone(),
            self.plan.backing.depth_desc.clone(),
        )
    }

    #[cfg(test)]
    pub(crate) fn tamper_synchronized_backing_budget_for_test(&mut self, pair: bool) {
        if pair {
            self.plan.backing.max_pair_bytes = self.plan.backing.pair_bytes.saturating_sub(1);
            self.plan.planned_backing.max_pair_bytes = self.plan.backing.max_pair_bytes;
        } else {
            let max_dimension = self
                .plan
                .backing
                .color_desc
                .width()
                .max(self.plan.backing.color_desc.height())
                .saturating_sub(1);
            self.plan.backing.max_dimension_2d = max_dimension;
            self.plan.planned_backing.max_dimension_2d = max_dimension;
        }
    }

    #[cfg(test)]
    pub(crate) fn tamper_transaction_binding_for_test(&mut self) {
        if let Some(binding) = self.transaction.seal.generic_bindings.first_mut() {
            binding.color_key = scroll_content_layer_stable_key(u64::MAX);
        }
    }

    #[cfg(test)]
    pub(crate) fn tamper_synchronized_root_contract_for_test(&mut self) {
        if let Some(root) = self.transaction.seal.roots.first_mut() {
            root.stable_id = root.stable_id.saturating_add(1);
            if let RetainedPropertyScrollGenericAuthority::ScrollTransformDirectCompiler(contract) =
                &mut self.transaction.generic_authority
            {
                contract.scene_root_stable_id = root.stable_id;
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn tamper_authority_for_test(&mut self, variant: u8) {
        self.transaction.generic_authority = match variant {
            0 => RetainedPropertyScrollGenericAuthority::Empty,
            1 => RetainedPropertyScrollGenericAuthority::TransformScrollCompiler,
            2 => RetainedPropertyScrollGenericAuthority::EffectScrollCompiler(Vec::new()),
            _ => RetainedPropertyScrollGenericAuthority::TransformEffectScrollCompiler(Vec::new()),
        };
    }

    #[cfg(test)]
    pub(crate) fn tamper_boundary_for_test(&mut self, variant: u8) {
        if let Some(boundary) = self.transaction.seal.ordered_boundaries.first_mut() {
            match variant {
                0 => boundary.ordinal = 1,
                1 => boundary.owner = self.plan.scaffold.admission.transform_content,
                _ => boundary.kind = SceneBoundaryKind::Transform,
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn tamper_root_owner_for_test(&mut self) {
        if let Some(root) = self.transaction.seal.roots.first_mut() {
            root.root = self.plan.scaffold.admission.transform_content;
        }
    }

    #[cfg(test)]
    pub(crate) fn tamper_synchronized_source_bounds_for_test(&mut self) {
        self.stamp.target.source_bounds_bits[0] ^= 1;
        if let Some(stamp) = self.transaction.generic_full_set.first_mut() {
            stamp.target.source_bounds_bits = self.stamp.target.source_bounds_bits;
        }
        if let RetainedPropertyScrollGenericAuthority::ScrollTransformDirectCompiler(contract) =
            &mut self.transaction.generic_authority
        {
            contract.source_bounds_bits = self.stamp.target.source_bounds_bits;
        }
    }

    #[cfg(test)]
    pub(crate) fn tamper_synchronized_descriptor_for_test(&mut self) {
        let tampered = TextureDesc::new(
            self.stamp.target.color.width().saturating_add(1),
            self.stamp.target.color.height(),
            self.stamp.target.color.format(),
            wgpu::TextureDimension::D2,
        );
        self.stamp.target.color = tampered.clone();
        if let Some(stamp) = self.transaction.generic_full_set.first_mut() {
            stamp.target.color = tampered;
        }
    }

    #[cfg(test)]
    pub(crate) fn tamper_synchronized_descriptor_origin_for_test(&mut self) {
        let (origin_x, origin_y) = self.stamp.target.color.origin();
        let tampered = self
            .stamp
            .target
            .color
            .clone()
            .with_origin(origin_x.saturating_add(1), origin_y);
        self.stamp.target.color = tampered.clone();
        if let Some(stamp) = self.transaction.generic_full_set.first_mut() {
            stamp.target.color = tampered;
        }
    }

    #[cfg(test)]
    pub(crate) fn tamper_synchronized_scale_for_test(&mut self) {
        self.stamp.target.scale_factor_bits = 1.0_f32.to_bits();
        if let Some(stamp) = self.transaction.generic_full_set.first_mut() {
            stamp.target.scale_factor_bits = 1.0_f32.to_bits();
        }
    }

    #[cfg(test)]
    pub(crate) fn tamper_synchronized_artifact_span_for_test(&mut self) {
        let tampered = if let Some(super::RetainedSurfaceRasterStepStamp::ArtifactSpan(span)) =
            self.stamp.ordered_steps.first_mut()
        {
            span.op_count = span.op_count.saturating_add(1);
            Some(span.clone())
        } else {
            None
        };
        if let Some(tampered) = tampered {
            if let Some(stamp) = self.transaction.generic_full_set.first_mut() {
                stamp.ordered_steps = self.stamp.ordered_steps.clone();
            }
            if let RetainedPropertyScrollGenericAuthority::ScrollTransformDirectCompiler(contract) =
                &mut self.transaction.generic_authority
            {
                contract.artifact_span = tampered;
            }
        }
    }
}

#[derive(Clone, Debug)]
struct ValidatedNestedScrollScene {
    plan: super::frame_plan::FramePaintPlan,
    leaf_stamp: RetainedSurfaceRasterStamp,
    transaction: RetainedPropertyScrollSceneTransaction,
}

impl ValidatedNestedScrollScene {
    fn is_canonical(&self) -> bool {
        let Some(scaffold) = self.plan.nested_scroll_planning_scaffold() else {
            return false;
        };
        let RetainedPropertyScrollGenericAuthority::NestedScrollCompiler(contract) =
            &self.transaction.generic_authority
        else {
            return false;
        };
        self.transaction.is_canonical()
            && nested_scroll_compiler_witness_matches_scaffold(&contract.compiled, scaffold)
            && contract.compiled.leaf_stamp == self.leaf_stamp
            && self.transaction.generic_stamps().is_empty()
            && self.transaction.scroll_groups().len() == 1
            && self.transaction.ordered_stamps() == [&self.leaf_stamp]
            && scaffold.boundaries.len() == 2
    }

    #[cfg(test)]
    fn transaction_shape_for_test(&self) -> [usize; 6] {
        [
            self.transaction.seal.roots.len(),
            self.transaction.seal.ordered_boundaries.len(),
            self.transaction.generic_full_set.len(),
            self.transaction.seal.generic_bindings.len(),
            self.transaction.scroll_groups.len(),
            self.transaction.seal.scroll_bindings.len(),
        ]
    }

    #[cfg(test)]
    fn action_keys_for_test(&self) -> FxHashSet<super::RetainedSurfaceResidentKey> {
        self.transaction
            .ordered_stamps()
            .into_iter()
            .map(|stamp| stamp.identity.resident_key())
            .collect()
    }
}

/// One exact world-space placement replayed into the frame-local A0 target.
/// Draw and text passes already subtract the target descriptor origin, so the
/// compiler must not manually apply S0 a second time.
#[derive(Clone, Debug, PartialEq, Eq)]
struct NestedScrollHostPlacementGeometry {
    source_bounds_bits: [u32; 4],
    target_origin: [u32; 2],
    scissor: [u32; 4],
}

/// Bitwise-frozen texture composite geometry. Bounds are in the destination
/// target's global logical coordinate space; TextureDesc origins perform the
/// world-to-target localization at record time.
#[derive(Clone, Debug, PartialEq, Eq)]
struct NestedScrollCompositeGeometry {
    source_bounds_bits: [u32; 4],
    destination_bounds_bits: [u32; 4],
    uv_bounds_bits: [u32; 4],
    scissor: [u32; 4],
}

impl NestedScrollCompositeGeometry {
    fn params(&self) -> TextureCompositeParams {
        TextureCompositeParams {
            bounds: self.destination_bounds_bits.map(f32::from_bits),
            quad_positions: None,
            uv_bounds: Some(self.uv_bounds_bits.map(f32::from_bits)),
            mask_uv_bounds: None,
            use_mask: false,
            source_is_premultiplied: true,
            opacity: 1.0,
            scissor_rect: Some(self.scissor),
        }
    }
}

/// Sized, keyless A0 descriptor pair. This token is graph-inert: it budgets
/// the descriptors allocated during emission, but owns no handle, stable key,
/// pool action or frame-stage capability.
#[derive(Clone, Debug, PartialEq, Eq)]
struct NestedScrollAssemblyTargetGeometry {
    visible_world_bounds_bits: [u32; 4],
    color_desc: TextureDesc,
    depth_desc: TextureDesc,
    pair_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct NestedScrollReceiverGeometryWitness {
    outer_scroll: ScrollNodeSnapshot,
    outer_clip: ClipNodeSnapshot,
    inner_scroll: ScrollNodeSnapshot,
    inner_clip: ClipNodeSnapshot,
    receiver_world_bounds_bits: [u32; 4],
    leaf_local_bounds_bits: [u32; 4],
    assembly: NestedScrollAssemblyTargetGeometry,
    inner_host_before: NestedScrollHostPlacementGeometry,
    inner_overlay_after: NestedScrollHostPlacementGeometry,
    leaf_to_assembly: NestedScrollCompositeGeometry,
    assembly_to_root: NestedScrollCompositeGeometry,
    resident_pair_bytes: u64,
    aggregate_pair_bytes: u64,
    budget: PropertyScrollBackingBudget,
}

/// Graph-inert prepare checkpoint for the exact S0 -> S1 -> leaf compiler.
/// The production executor borrows the viewport and graph only after this
/// complete geometry witness has been validated.
#[derive(Clone, Debug)]
pub(crate) struct PreparedNestedScrollReceiverGeometry {
    scene: ValidatedNestedScrollScene,
    compiled: NestedScrollReceiverGeometryWitness,
    planned: NestedScrollReceiverGeometryWitness,
}

/// Exclusive pre-mutation lease for the exact `S0 -> S1 -> leaf` executor.
/// Every artifact, descriptor, action, cursor and staging capability is
/// validated before this value can borrow the viewport and graph.
pub(crate) struct PreparedNestedScrollScene<'a> {
    viewport: &'a mut Viewport,
    graph: &'a mut FrameGraph,
    parent_ctx: UiBuildContext,
    outer_host_before: ValidatedScrollSceneHostBeforeArtifact,
    inner_host_before: ValidatedScrollSceneHostBeforeArtifact,
    leaf: ValidatedScrollSceneContentArtifact,
    inner_overlay_after: ValidatedScrollSceneOverlayArtifact,
    outer_overlay_after: ValidatedScrollSceneOverlayArtifact,
    geometry: NestedScrollReceiverGeometryWitness,
    leaf_stamp: RetainedSurfaceRasterStamp,
    actions: FxHashMap<super::RetainedSurfaceResidentKey, RetainedSurfaceCompileAction>,
    transaction: RetainedPropertyScrollSceneTransaction,
    outer_host_terminal: u32,
    inner_host_terminal: u32,
    leaf_terminal: u32,
    inner_terminal: u32,
    parent_terminal: u32,
    clear_rgba_bits: [u32; 4],
    frame_owner: RetainedSurfaceFrameStageOwner,
    trace: RetainedPropertyScrollSceneBuildTrace,
}

#[cfg(test)]
impl PreparedNestedScrollScene<'_> {
    fn action_for_test(&self) -> RetainedSurfaceCompileAction {
        self.actions[&self.leaf_stamp.identity.resident_key()]
    }

    fn transaction_shape_for_test(&self) -> [usize; 6] {
        [
            self.transaction.seal.roots.len(),
            self.transaction.seal.ordered_boundaries.len(),
            self.transaction.generic_full_set.len(),
            self.transaction.seal.generic_bindings.len(),
            self.transaction.scroll_groups.len(),
            self.transaction.seal.scroll_bindings.len(),
        ]
    }

    fn terminals_for_test(&self) -> [u32; 5] {
        [
            self.outer_host_terminal,
            self.inner_host_terminal,
            self.leaf_terminal,
            self.inner_terminal,
            self.parent_terminal,
        ]
    }

    fn graph_is_pristine_for_test(&self) -> bool {
        self.graph.pass_descriptors().is_empty()
            && self
                .graph
                .declared_persistent_texture_keys()
                .next()
                .is_none()
    }

    fn pool_shape_for_test(&self) -> (usize, Option<usize>) {
        self.viewport.retained_surface_transaction_shape_for_test()
    }

    fn refresh_action_from_committed_test_pool(&mut self) {
        self.actions = self
            .viewport
            .freeze_retained_property_scroll_scene_compile_actions_for_forced_test(
                &self.transaction,
            )
            .expect("prepared nested transaction remains canonical for the test pool");
        assert_eq!(self.actions.len(), 1);
        let action = self.action_for_test();
        self.trace.reraster_count = usize::from(action == RetainedSurfaceCompileAction::Reraster);
        self.trace.reuse_count = usize::from(action == RetainedSurfaceCompileAction::Reuse);
    }
}

impl PreparedNestedScrollReceiverGeometry {
    pub(crate) fn is_canonical(&self) -> bool {
        self.scene.is_canonical()
            && self.compiled == self.planned
            && nested_scroll_receiver_geometry_witness(
                &self.scene,
                self.compiled.assembly.color_desc.format(),
                ScrollSceneSingleTextureBudget {
                    max_dimension_2d: self.compiled.budget.max_dimension_2d,
                    max_pair_bytes: self.compiled.budget.max_active_pair_bytes,
                },
            )
            .as_ref()
                == Some(&self.compiled)
    }

    #[cfg(test)]
    fn composite_params_for_test(&self) -> [TextureCompositeParams; 2] {
        [
            self.compiled.leaf_to_assembly.params(),
            self.compiled.assembly_to_root.params(),
        ]
    }

    #[cfg(test)]
    fn descriptor_shape_for_test(&self) -> [(u32, u32); 3] {
        [
            self.compiled.assembly.color_desc.origin(),
            (
                self.compiled.assembly.color_desc.width(),
                self.compiled.assembly.color_desc.height(),
            ),
            self.compiled.assembly.depth_desc.origin(),
        ]
    }

    #[cfg(test)]
    fn pair_bytes_for_test(&self) -> [u64; 3] {
        [
            self.compiled.resident_pair_bytes,
            self.compiled.assembly.pair_bytes,
            self.compiled.aggregate_pair_bytes,
        ]
    }

    #[cfg(test)]
    pub(crate) fn leaf_target_for_test(&self) -> (PersistentTextureKey, TextureDesc) {
        (
            self.scene.leaf_stamp.identity.color_key,
            self.scene.leaf_stamp.target.color.clone(),
        )
    }
}

fn finite_positive_rect(bits: [u32; 4]) -> Option<[f32; 4]> {
    let rect = bits.map(f32::from_bits);
    (rect.into_iter().all(f32::is_finite) && rect[2] > 0.0 && rect[3] > 0.0).then_some(rect)
}

fn intersect_continuous_rect_with_scissor(bounds: [f32; 4], scissor: [u32; 4]) -> Option<[f32; 4]> {
    let right = bounds[0] + bounds[2];
    let bottom = bounds[1] + bounds[3];
    let scissor_right = scissor[0].checked_add(scissor[2])? as f32;
    let scissor_bottom = scissor[1].checked_add(scissor[3])? as f32;
    let left = bounds[0].max(scissor[0] as f32);
    let top = bounds[1].max(scissor[1] as f32);
    let right = right.min(scissor_right);
    let bottom = bottom.min(scissor_bottom);
    let result = [left, top, right - left, bottom - top];
    (result.into_iter().all(f32::is_finite) && result[2] > 0.0 && result[3] > 0.0).then_some(result)
}

fn intersect_nonempty_scissors(left: [u32; 4], right: [u32; 4]) -> Option<[u32; 4]> {
    let left_right = left[0].checked_add(left[2])?;
    let left_bottom = left[1].checked_add(left[3])?;
    let right_right = right[0].checked_add(right[2])?;
    let right_bottom = right[1].checked_add(right[3])?;
    let x = left[0].max(right[0]);
    let y = left[1].max(right[1]);
    let max_x = left_right.min(right_right);
    let max_y = left_bottom.min(right_bottom);
    (max_x > x && max_y > y).then_some([x, y, max_x - x, max_y - y])
}

fn nested_scroll_receiver_geometry_witness(
    scene: &ValidatedNestedScrollScene,
    target_format: wgpu::TextureFormat,
    budget: ScrollSceneSingleTextureBudget,
) -> Option<NestedScrollReceiverGeometryWitness> {
    scene.is_canonical().then_some(())?;
    let scaffold = scene.plan.nested_scroll_planning_scaffold()?;
    let [outer, inner] = scaffold.boundaries.as_slice() else {
        return None;
    };
    let contract = match &scene.transaction.generic_authority {
        RetainedPropertyScrollGenericAuthority::NestedScrollCompiler(contract) => contract,
        _ => return None,
    };
    let admission = scaffold.admission;
    let receiver_world = finite_positive_rect(bounds_bits(admission.inner_source_bounds))?;
    let outer_zero = outer.scroll.layout_content_bounds_at_zero;
    let derived_receiver_world = [
        outer_zero.x - outer.scroll.offset.x,
        outer_zero.y - outer.scroll.offset.y,
        outer_zero.width,
        outer_zero.height,
    ];
    if derived_receiver_world.map(f32::to_bits) != receiver_world.map(f32::to_bits)
        || [
            outer.scroll.offset.x,
            outer.scroll.offset.y,
            inner.scroll.offset.x,
            inner.scroll.offset.y,
        ]
        .into_iter()
        .any(|value| !value.is_finite())
    {
        return None;
    }

    let leaf_local = finite_positive_rect(contract.compiled.leaf_source_bounds_bits)?;
    if scene.leaf_stamp.target.source_bounds_bits != leaf_local.map(f32::to_bits)
        || inner.scroll.layout_content_bounds_at_zero.width.to_bits() != leaf_local[2].to_bits()
        || inner.scroll.layout_content_bounds_at_zero.height.to_bits() != leaf_local[3].to_bits()
        || (receiver_world[0] + leaf_local[0]).to_bits()
            != inner.scroll.layout_content_bounds_at_zero.x.to_bits()
        || (receiver_world[1] + leaf_local[1]).to_bits()
            != inner.scroll.layout_content_bounds_at_zero.y.to_bits()
    {
        return None;
    }

    let c0 = outer.contents_clip.logical_scissor;
    let c1 = inner.contents_clip.logical_scissor;
    let resolved_leaf_clip = intersect_nonempty_scissors(c0, c1)?;
    let visible_world = intersect_continuous_rect_with_scissor(receiver_world, c0)?;
    let visible_world_bits = visible_world.map(f32::to_bits);
    let assembly_color = texture_desc_for_logical_bounds(
        RetainedSurfaceBounds {
            x: visible_world[0],
            y: visible_world[1],
            width: visible_world[2],
            height: visible_world[3],
            corner_radii: [0.0; 4],
        },
        1.0,
        None,
        target_format,
    )
    .with_label("Nested Scroll Assembly A0");
    let (assembly_color, assembly_depth) = transient_target_texture_descriptors(assembly_color);
    let assembly_pair_bytes = canonical_pair_bytes(&assembly_color, &assembly_depth)?;
    let resident_pair_bytes = canonical_pair_bytes(
        &scene.leaf_stamp.target.color,
        &scene.leaf_stamp.target.depth,
    )?;
    let aggregate_pair_bytes = resident_pair_bytes.checked_add(assembly_pair_bytes)?;
    let budget = property_scroll_budget(budget);
    if scene.leaf_stamp.target.color.format() != target_format
        || [
            scene.leaf_stamp.target.color.width(),
            scene.leaf_stamp.target.color.height(),
            scene.leaf_stamp.target.depth.width(),
            scene.leaf_stamp.target.depth.height(),
            assembly_color.width(),
            assembly_color.height(),
            assembly_depth.width(),
            assembly_depth.height(),
        ]
        .into_iter()
        .any(|dimension| dimension == 0 || dimension > budget.max_dimension_2d)
        || aggregate_pair_bytes > budget.max_active_pair_bytes
        || assembly_color.sample_count() != 1
        || assembly_depth.sample_count() != 1
        || assembly_color.origin()
            != (
                visible_world[0].floor().max(0.0) as u32,
                visible_world[1].floor().max(0.0) as u32,
            )
        || assembly_depth.origin() != (0, 0)
    {
        return None;
    }

    let leaf_destination = [
        receiver_world[0] + leaf_local[0] - inner.scroll.offset.x,
        receiver_world[1] + leaf_local[1] - inner.scroll.offset.y,
        leaf_local[2],
        leaf_local[3],
    ];
    if leaf_destination.into_iter().any(|value| !value.is_finite()) {
        return None;
    }
    let target_origin = assembly_color.origin();
    Some(NestedScrollReceiverGeometryWitness {
        outer_scroll: outer.scroll,
        outer_clip: outer.contents_clip,
        inner_scroll: inner.scroll,
        inner_clip: inner.contents_clip,
        receiver_world_bounds_bits: receiver_world.map(f32::to_bits),
        leaf_local_bounds_bits: leaf_local.map(f32::to_bits),
        assembly: NestedScrollAssemblyTargetGeometry {
            visible_world_bounds_bits: visible_world_bits,
            color_desc: assembly_color,
            depth_desc: assembly_depth,
            pair_bytes: assembly_pair_bytes,
        },
        inner_host_before: NestedScrollHostPlacementGeometry {
            source_bounds_bits: receiver_world.map(f32::to_bits),
            target_origin: [target_origin.0, target_origin.1],
            scissor: c0,
        },
        inner_overlay_after: NestedScrollHostPlacementGeometry {
            source_bounds_bits: receiver_world.map(f32::to_bits),
            target_origin: [target_origin.0, target_origin.1],
            scissor: c0,
        },
        leaf_to_assembly: NestedScrollCompositeGeometry {
            source_bounds_bits: leaf_local.map(f32::to_bits),
            destination_bounds_bits: leaf_destination.map(f32::to_bits),
            uv_bounds_bits: leaf_local.map(f32::to_bits),
            scissor: resolved_leaf_clip,
        },
        assembly_to_root: NestedScrollCompositeGeometry {
            source_bounds_bits: visible_world_bits,
            destination_bounds_bits: visible_world_bits,
            uv_bounds_bits: visible_world_bits,
            scissor: c0,
        },
        resident_pair_bytes,
        aggregate_pair_bytes,
        budget,
    })
}

/// Freezes nested receiver-local placement and the sized transient A0 pair.
/// No graph or viewport is accepted, so successful preparation cannot allocate
/// a target, mutate the pool or stage a resident set.
fn prepare_nested_scroll_receiver_geometry(
    scene: ValidatedNestedScrollScene,
    target_format: wgpu::TextureFormat,
    budget: ScrollSceneSingleTextureBudget,
) -> Result<PreparedNestedScrollReceiverGeometry, PropertyScrollScenePlanError> {
    let witness = match nested_scroll_receiver_geometry_witness(&scene, target_format, budget) {
        Some(witness) => witness,
        None => {
            let unconstrained = nested_scroll_receiver_geometry_witness(
                &scene,
                target_format,
                ScrollSceneSingleTextureBudget::new(u32::MAX, u64::MAX)
                    .expect("maximal nested geometry budget is non-zero"),
            );
            return Err(if unconstrained.is_some() {
                PropertyScrollScenePlanError::BackingBudget
            } else {
                PropertyScrollScenePlanError::InvalidContract
            });
        }
    };
    let prepared = PreparedNestedScrollReceiverGeometry {
        scene,
        compiled: witness.clone(),
        planned: witness,
    };
    prepared
        .is_canonical()
        .then_some(prepared)
        .ok_or(PropertyScrollScenePlanError::InvalidContract)
}

/// Dedicated graph-inert RetainedAuto candidate for exactly
/// `S0 -> S1 -> leaf`. Returning only the receiver-geometry token prevents
/// callers from routing the nested `FramePaintPlan` through the standard
/// property-scene executor or layerizer.
#[allow(clippy::too_many_arguments)]
pub(crate) fn plan_and_prepare_nested_scroll_scene(
    arena: &NodeArena,
    roots: &[NodeKey],
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    scale_factor: f32,
    incoming_paint_offset: [f32; 2],
    outer_scissor_rect: Option<[u32; 4]>,
    target_format: wgpu::TextureFormat,
    budget: ScrollSceneSingleTextureBudget,
) -> Result<PreparedNestedScrollReceiverGeometry, PropertyScrollScenePlanError> {
    let plan = super::frame_plan::plan_nested_scroll_scene_scaffold_with_context(
        arena,
        roots,
        property_trees,
        paint_generations,
        scale_factor,
        super::TransformSurfacePlanContext::new(incoming_paint_offset, outer_scissor_rect),
    )
    .map_err(PropertyScrollScenePlanError::Frame)?;
    let scene = compile_nested_scroll_transaction(plan, target_format, budget)?;
    prepare_nested_scroll_receiver_geometry(scene, target_format, budget)
}

/// Completes every fallible nested-scroll check and freezes the sole R1 pool
/// action before the graph can be mutated.
pub(crate) fn prepare_nested_scroll_scene_from_pool<'a>(
    viewport: &'a mut Viewport,
    prepared_geometry: PreparedNestedScrollReceiverGeometry,
    graph: &'a mut FrameGraph,
    ctx: UiBuildContext,
    clear_rgba: [f32; 4],
    frame_owner: RetainedSurfaceFrameStageOwner,
) -> Result<PreparedNestedScrollScene<'a>, RetainedPropertyScrollScenePrepareError> {
    if !prepared_geometry.is_canonical() {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    }
    validate_parent_target(graph, &ctx)
        .map_err(|_| RetainedPropertyScrollScenePrepareError::ParentTarget)?;
    let target_format = prepared_geometry.compiled.assembly.color_desc.format();
    if ctx.viewport().scale_factor().to_bits() != 1.0_f32.to_bits()
        || ctx.paint_offset().map(f32::to_bits) != [0.0_f32.to_bits(); 2]
        || ctx.graphics_pass_context().scissor_rect.is_some()
        || ctx.opaque_rect_order() != 0
        || ctx.current_render_transform().is_some()
        || ctx.viewport().target_format() != target_format
        || prepared_geometry.scene.leaf_stamp.target.color.format() != target_format
        || clear_rgba.iter().any(|component| !component.is_finite())
    {
        return Err(RetainedPropertyScrollScenePrepareError::ContextMismatch);
    }
    if !viewport.retained_property_scroll_scene_stage_is_available()
        || !viewport.retained_surface_frame_stage_owner_is_active(frame_owner)
    {
        return Err(RetainedPropertyScrollScenePrepareError::StageUnavailable);
    }

    let leaf_stamp = &prepared_geometry.scene.leaf_stamp;
    let color_key = leaf_stamp.identity.color_key;
    let depth_key = color_key
        .depth_stencil()
        .ok_or(RetainedPropertyScrollScenePrepareError::DescriptorPair)?;
    if graph
        .declared_persistent_texture_keys()
        .any(|key| key == color_key || key == depth_key)
    {
        return Err(
            RetainedPropertyScrollScenePrepareError::PersistentKeyAlreadyDeclared(color_key),
        );
    }
    let expected_leaf_pair =
        persistent_target_texture_descriptors(leaf_stamp.target.color.clone(), color_key);
    let expected_assembly_pair = transient_target_texture_descriptors(
        prepared_geometry.compiled.assembly.color_desc.clone(),
    );
    if expected_leaf_pair.0 != leaf_stamp.target.color
        || expected_leaf_pair.1 != leaf_stamp.target.depth
        || expected_assembly_pair.0 != prepared_geometry.compiled.assembly.color_desc
        || expected_assembly_pair.1 != prepared_geometry.compiled.assembly.depth_desc
        || canonical_pair_bytes(&leaf_stamp.target.color, &leaf_stamp.target.depth)
            != Some(prepared_geometry.compiled.resident_pair_bytes)
        || canonical_pair_bytes(&expected_assembly_pair.0, &expected_assembly_pair.1)
            != Some(prepared_geometry.compiled.assembly.pair_bytes)
        || prepared_geometry.compiled.aggregate_pair_bytes
            != prepared_geometry
                .compiled
                .resident_pair_bytes
                .checked_add(prepared_geometry.compiled.assembly.pair_bytes)
                .ok_or(RetainedPropertyScrollScenePrepareError::DescriptorPair)?
        || prepared_geometry.compiled.aggregate_pair_bytes
            > prepared_geometry.compiled.budget.max_active_pair_bytes
        || [
            leaf_stamp.target.color.width(),
            leaf_stamp.target.color.height(),
            leaf_stamp.target.depth.width(),
            leaf_stamp.target.depth.height(),
            expected_assembly_pair.0.width(),
            expected_assembly_pair.0.height(),
            expected_assembly_pair.1.width(),
            expected_assembly_pair.1.height(),
        ]
        .into_iter()
        .any(|dimension| {
            dimension == 0 || dimension > prepared_geometry.compiled.budget.max_dimension_2d
        })
    {
        return Err(RetainedPropertyScrollScenePrepareError::DescriptorPair);
    }

    let scaffold = prepared_geometry
        .scene
        .plan
        .nested_scroll_planning_scaffold()
        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
    let [outer, inner] = scaffold.boundaries.as_slice() else {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    };
    let RetainedPropertyScrollGenericAuthority::NestedScrollCompiler(compiler_contract) =
        &prepared_geometry.scene.transaction.generic_authority
    else {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    };
    let [
        super::frame_plan::NestedScrollSceneScheduledStep::HostBefore {
            boundary: super::frame_plan::NestedScrollBoundarySlot::Outer,
            artifact: outer_host,
        },
        super::frame_plan::NestedScrollSceneScheduledStep::HostBefore {
            boundary: super::frame_plan::NestedScrollBoundarySlot::Inner,
            artifact: inner_host,
        },
        super::frame_plan::NestedScrollSceneScheduledStep::ContentReceiver(receiver),
        super::frame_plan::NestedScrollSceneScheduledStep::OverlayAfter {
            boundary: super::frame_plan::NestedScrollBoundarySlot::Inner,
            artifact: inner_overlay,
        },
        super::frame_plan::NestedScrollSceneScheduledStep::OverlayAfter {
            boundary: super::frame_plan::NestedScrollBoundarySlot::Outer,
            artifact: outer_overlay,
        },
    ] = scaffold.schedule.steps.as_slice()
    else {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    };
    let outer_bounds_bits = bounds_bits(scaffold.admission.outer_source_bounds);
    let inner_bounds_bits = bounds_bits(scaffold.admission.inner_source_bounds);
    if inner_bounds_bits != prepared_geometry.compiled.receiver_world_bounds_bits
        || outer.scroll != prepared_geometry.compiled.outer_scroll
        || outer.contents_clip != prepared_geometry.compiled.outer_clip
        || inner.scroll != prepared_geometry.compiled.inner_scroll
        || inner.contents_clip != prepared_geometry.compiled.inner_clip
    {
        return Err(RetainedPropertyScrollScenePrepareError::GeometryContract);
    }
    let outer_host_artifact = outer_host.artifact().clone();
    let inner_host_artifact = inner_host.artifact().clone();
    let leaf_artifact = receiver.artifact.artifact();
    let inner_overlay_artifact = inner_overlay.artifact().clone();
    let outer_overlay_artifact = outer_overlay.artifact().clone();
    let outer_host_terminal = checked_property_scroll_opaque_order_count(&outer_host_artifact)
        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
    let inner_host_terminal = checked_property_scroll_opaque_order_count(&inner_host_artifact)
        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
    let leaf_terminal = checked_property_scroll_opaque_order_count(leaf_artifact)
        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
    let inner_overlay_count = checked_property_scroll_opaque_order_count(&inner_overlay_artifact)
        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
    let outer_overlay_count = checked_property_scroll_opaque_order_count(&outer_overlay_artifact)
        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
    let inner_terminal = inner_host_terminal
        .checked_add(inner_overlay_count)
        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
    let parent_terminal = outer_host_terminal
        .checked_add(outer_overlay_count)
        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;

    let outer_host_before = validate_scroll_scene_host_before_artifact(
        outer_host_artifact,
        scaffold.admission.outer_boundary_root,
        outer_bounds_bits,
    )
    .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
    let inner_host_before = validate_scroll_scene_host_before_artifact(
        inner_host_artifact,
        scaffold.admission.inner_boundary_root,
        inner_bounds_bits,
    )
    .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
    let leaf = validate_nested_scroll_content_artifact(
        leaf_artifact,
        scaffold.admission.content_leaf,
        outer.scroll.id,
        outer.contents_clip,
        compiler_contract.compiled.leaf_recorded_bounds_bits,
        prepared_geometry.compiled.leaf_local_bounds_bits,
    )
    .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
    let localized_span = validated_scroll_content_artifact_span_stamp(&leaf, 0, 0..leaf_terminal)
        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
    if leaf_stamp.ordered_steps.as_slice()
        != [super::RetainedSurfaceRasterStepStamp::ArtifactSpan(
            localized_span,
        )]
        || leaf_stamp.opaque_order_span != (0..leaf_terminal)
    {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    }
    let inner_overlay_after = validate_scroll_scene_overlay_artifact(
        inner_overlay_artifact,
        scaffold.admission.inner_boundary_root,
        inner.scroll,
        inner_bounds_bits,
    )
    .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
    let outer_overlay_after = validate_scroll_scene_overlay_artifact(
        outer_overlay_artifact,
        scaffold.admission.outer_boundary_root,
        outer.scroll,
        outer_bounds_bits,
    )
    .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;

    let actions = viewport
        .freeze_retained_property_scroll_scene_compile_actions_from_pool(
            &prepared_geometry.scene.transaction,
        )
        .ok_or(RetainedPropertyScrollScenePrepareError::PoolContract)?;
    let expected_key = leaf_stamp.identity.resident_key();
    if actions.len() != 1 || actions.keys().copied().next() != Some(expected_key) {
        return Err(RetainedPropertyScrollScenePrepareError::PoolContract);
    }
    let action = actions[&expected_key];
    let trace = RetainedPropertyScrollSceneBuildTrace {
        root_count: 1,
        generic_surface_count: 0,
        effect_surface_count: 0,
        scroll_group_count: 1,
        backing: ScrollSceneBackingKind::Single,
        tile_count: 1,
        reraster_count: usize::from(action == RetainedSurfaceCompileAction::Reraster),
        reuse_count: usize::from(action == RetainedSurfaceCompileAction::Reuse),
        content_pair_bytes: prepared_geometry.compiled.aggregate_pair_bytes,
    };
    Ok(PreparedNestedScrollScene {
        viewport,
        graph,
        parent_ctx: ctx,
        outer_host_before,
        inner_host_before,
        leaf,
        inner_overlay_after,
        outer_overlay_after,
        geometry: prepared_geometry.compiled,
        leaf_stamp: prepared_geometry.scene.leaf_stamp,
        actions,
        transaction: prepared_geometry.scene.transaction,
        outer_host_terminal,
        inner_host_terminal,
        leaf_terminal,
        inner_terminal,
        parent_terminal,
        clear_rgba_bits: clear_rgba.map(f32::to_bits),
        frame_owner,
        trace,
    })
}

/// Emits the compiler-sealed nested scene in its fixed three-cursor order.
/// The prepared lease makes every operation below infallible.
pub(crate) fn emit_prepared_nested_scroll_scene(
    prepared: PreparedNestedScrollScene<'_>,
) -> RetainedPropertyScrollSceneBuildOutcome {
    let PreparedNestedScrollScene {
        viewport,
        graph,
        mut parent_ctx,
        outer_host_before,
        inner_host_before,
        leaf,
        inner_overlay_after,
        outer_overlay_after,
        geometry,
        leaf_stamp,
        mut actions,
        transaction,
        outer_host_terminal,
        inner_host_terminal,
        leaf_terminal,
        inner_terminal,
        parent_terminal,
        clear_rgba_bits,
        frame_owner,
        trace,
    } = prepared;
    assert!(viewport.retained_surface_frame_stage_owner_is_active(frame_owner));
    assert!(viewport.retained_property_scroll_scene_stage_is_available());
    assert_eq!(parent_ctx.opaque_rect_order(), 0);

    let parent_target = parent_ctx
        .current_target()
        .unwrap_or_else(|| parent_ctx.allocate_target(graph));
    parent_ctx.set_current_target(parent_target);
    graph.add_graphics_pass(ClearPass::new(
        crate::view::render_pass::clear_pass::ClearParams::new(clear_rgba_bits.map(f32::from_bits)),
        crate::view::render_pass::clear_pass::ClearInput {
            pass_context: parent_ctx.graphics_pass_context(),
            clear_depth_stencil: true,
        },
        crate::view::render_pass::clear_pass::ClearOutput {
            render_target: parent_target,
        },
    ));
    if let Some(handle) = parent_target.handle() {
        parent_ctx.set_color_target(Some(handle));
    }
    emit_validated_scroll_scene_host_before_artifact(outer_host_before, graph, &mut parent_ctx);
    assert_eq!(parent_ctx.opaque_rect_order(), outer_host_terminal);

    let mut assembly_ctx = UiBuildContext::from_parts(
        parent_ctx.viewport(),
        parent_ctx.layer_subtree_state_with_ancestor_clip(AncestorClipContext::default()),
    );
    let assembly_target = assembly_ctx
        .allocate_transient_target_with_desc(graph, geometry.assembly.color_desc.clone());
    assembly_ctx.set_current_target(assembly_target);
    graph.add_graphics_pass(ClearPass::new(
        crate::view::render_pass::clear_pass::ClearParams::new([0.0; 4]),
        crate::view::render_pass::clear_pass::ClearInput {
            pass_context: assembly_ctx.graphics_pass_context(),
            clear_depth_stencil: true,
        },
        crate::view::render_pass::clear_pass::ClearOutput {
            render_target: assembly_target,
        },
    ));
    assert_eq!(
        assembly_ctx.push_scissor_rect(Some(geometry.outer_clip.logical_scissor)),
        None
    );
    emit_validated_scroll_scene_host_before_artifact(inner_host_before, graph, &mut assembly_ctx);
    assert_eq!(assembly_ctx.opaque_rect_order(), inner_host_terminal);

    let mut leaf_ctx = UiBuildContext::from_parts(
        parent_ctx.viewport(),
        parent_ctx.layer_subtree_state_with_ancestor_clip(AncestorClipContext::default()),
    );
    let leaf_target = leaf_ctx.allocate_persistent_target_with_desc(
        graph,
        leaf_stamp.target.color.clone(),
        leaf_stamp.identity.color_key,
    );
    leaf_ctx.set_current_target(leaf_target);
    let action = actions
        .remove(&leaf_stamp.identity.resident_key())
        .expect("prepared nested leaf action is frozen");
    match action {
        RetainedSurfaceCompileAction::Reraster => {
            graph.add_graphics_pass(ClearPass::new(
                crate::view::render_pass::clear_pass::ClearParams::new([0.0; 4]),
                crate::view::render_pass::clear_pass::ClearInput {
                    pass_context: leaf_ctx.graphics_pass_context(),
                    clear_depth_stencil: true,
                },
                crate::view::render_pass::clear_pass::ClearOutput {
                    render_target: leaf_target,
                },
            ));
            emit_validated_scroll_scene_content_artifact(&leaf, graph, &mut leaf_ctx);
        }
        RetainedSurfaceCompileAction::Reuse => {
            leaf_ctx.replay_opaque_rect_order_exact(0, leaf_terminal);
        }
    }
    assert_eq!(leaf_ctx.graphics_pass_context().scissor_rect, None);
    assert_eq!(leaf_ctx.opaque_rect_order(), leaf_terminal);
    assembly_ctx.merge_child_target_pairs(&leaf_ctx.into_state());
    assert_eq!(assembly_ctx.opaque_rect_order(), inner_host_terminal);
    assembly_ctx.set_current_target(assembly_target);
    graph.add_graphics_pass(TextureCompositePass::new(
        geometry.leaf_to_assembly.params(),
        TextureCompositeInput::from_render_target(
            TextureCompositeSourceIn::with_handle(
                leaf_target
                    .handle()
                    .expect("prepared nested leaf target has a handle"),
            ),
            Default::default(),
            assembly_ctx.graphics_pass_context(),
        ),
        TextureCompositeOutput {
            render_target: assembly_target,
        },
    ));
    emit_validated_scroll_scene_overlay_artifact(inner_overlay_after, graph, &mut assembly_ctx);
    assert_eq!(assembly_ctx.opaque_rect_order(), inner_terminal);

    parent_ctx.merge_child_target_pairs(&assembly_ctx.into_state());
    assert_eq!(parent_ctx.opaque_rect_order(), outer_host_terminal);
    parent_ctx.set_current_target(parent_target);
    graph.add_graphics_pass(TextureCompositePass::new(
        geometry.assembly_to_root.params(),
        TextureCompositeInput::from_render_target(
            TextureCompositeSourceIn::with_handle(
                assembly_target
                    .handle()
                    .expect("prepared nested assembly target has a handle"),
            ),
            Default::default(),
            parent_ctx.graphics_pass_context(),
        ),
        TextureCompositeOutput {
            render_target: parent_target,
        },
    ));
    emit_validated_scroll_scene_overlay_artifact(outer_overlay_after, graph, &mut parent_ctx);
    assert_eq!(parent_ctx.opaque_rect_order(), parent_terminal);
    assert!(actions.is_empty());
    assert!(
        viewport.stage_retained_property_scroll_scene(transaction),
        "prepared nested scene stages its joint transaction exactly once"
    );
    RetainedPropertyScrollSceneBuildOutcome {
        state: parent_ctx.into_state(),
        trace,
    }
}

/// S3a compiles the S2 plan into the existing joint residency transaction
/// vocabulary, but with an authority-specific exact shape: one S boundary,
/// one generic T resident, and no scroll group.
pub(crate) fn compile_direct_scroll_transform_transaction(
    plan: DirectScrollTransformGeometryPlan,
) -> Result<ValidatedDirectScrollTransformTransaction, PropertyScrollScenePlanError> {
    if !plan.is_canonical() {
        return Err(PropertyScrollScenePlanError::InvalidContract);
    }
    let [super::frame_recorder::RecordedTransformSurfaceStep::Artifact(content_artifact)] =
        plan.scaffold.content_steps.as_slice()
    else {
        return Err(PropertyScrollScenePlanError::InvalidContract);
    };
    let validated = super::compiler::validate_transform_property_surface_artifact(
        content_artifact,
        plan.scaffold.admission.transform_content,
        plan.scaffold.transform.id,
    )
    .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
    let content_opaque_terminal = checked_property_scroll_opaque_order_count(content_artifact)
        .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
    let artifact_span = super::compiler::validated_transform_property_surface_artifact_span_stamp(
        &validated,
        0,
        0..content_opaque_terminal,
    )
    .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
    let target = RetainedSurfaceRasterInputs {
        color: plan.backing.color_desc.clone(),
        depth: plan.backing.depth_desc.clone(),
        scale_factor_bits: plan.scaffold.scale_factor_bits,
        source_bounds_bits: plan.backing.source_bounds_bits,
    };
    let stamp = super::compiler::validated_property_scene_surface_raster_stamp(
        plan.scaffold.admission.transform_content,
        plan.scaffold.admission.transform_content_stable_id,
        plan.backing.color_key,
        0,
        target,
        vec![super::RetainedSurfaceRasterStepStamp::ArtifactSpan(
            artifact_span.clone(),
        )],
        0..content_opaque_terminal,
    )
    .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
    let boundary = SceneBoundaryId {
        ordinal: 0,
        owner: plan.scaffold.admission.boundary_root,
        kind: SceneBoundaryKind::ScrollContents,
    };
    let contract = ScrollTransformDirectCompilerContract {
        boundary,
        scene_root: plan.scaffold.admission.boundary_root,
        scene_root_stable_id: plan.scaffold.admission.stable_id,
        transform: plan.scaffold.transform.id,
        transform_stable_id: plan.scaffold.admission.transform_content_stable_id,
        source_bounds_bits: plan.backing.source_bounds_bits,
        artifact_span: artifact_span.clone(),
        planned_scene_root: plan.scaffold.admission.boundary_root,
        planned_scene_root_stable_id: plan.scaffold.admission.stable_id,
        planned_transform: plan.scaffold.transform.id,
        planned_transform_stable_id: plan.scaffold.admission.transform_content_stable_id,
        planned_source_bounds_bits: plan.backing.source_bounds_bits,
        planned_artifact_span: artifact_span,
    };
    if !contract.validates_stamp(&stamp) {
        return Err(PropertyScrollScenePlanError::InvalidContract);
    }
    let transaction = RetainedPropertyScrollSceneTransaction {
        seal: RetainedPropertyScrollJointSeal {
            roots: vec![RetainedPropertyScrollJointRootStamp {
                ordinal: 0,
                root: plan.scaffold.admission.boundary_root,
                stable_id: plan.scaffold.admission.stable_id,
                boundary_span: 0..1,
            }],
            ordered_boundaries: vec![boundary],
            generic_bindings: vec![RetainedPropertyScrollGenericBindingStamp {
                boundary,
                resident_key: stamp.identity.resident_key(),
                color_key: stamp.identity.color_key,
            }],
            scroll_bindings: Vec::new(),
        },
        generic_authority: RetainedPropertyScrollGenericAuthority::ScrollTransformDirectCompiler(
            contract,
        ),
        generic_full_set: vec![stamp.clone()],
        scroll_groups: Vec::new(),
    };
    let validated = ValidatedDirectScrollTransformTransaction {
        plan,
        stamp,
        transaction,
        content_opaque_terminal,
    };
    validated
        .is_canonical()
        .then_some(validated)
        .ok_or(PropertyScrollScenePlanError::InvalidContract)
}

/// Compiles the graph-inert exact nested-scroll scaffold into a residency
/// transaction. Only the leaf content becomes persistent; the outer assembly
/// edge remains a keyless compiler contract through prepare.
fn compile_nested_scroll_transaction(
    plan: super::frame_plan::FramePaintPlan,
    target_format: wgpu::TextureFormat,
    budget: ScrollSceneSingleTextureBudget,
) -> Result<ValidatedNestedScrollScene, PropertyScrollScenePlanError> {
    let scaffold = plan
        .nested_scroll_planning_scaffold()
        .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
    let [outer, inner] = scaffold.boundaries.as_slice() else {
        return Err(PropertyScrollScenePlanError::InvalidContract);
    };
    let [
        super::frame_plan::NestedScrollSceneScheduledStep::HostBefore {
            boundary: super::frame_plan::NestedScrollBoundarySlot::Outer,
            artifact: h0,
        },
        super::frame_plan::NestedScrollSceneScheduledStep::HostBefore {
            boundary: super::frame_plan::NestedScrollBoundarySlot::Inner,
            artifact: h1,
        },
        super::frame_plan::NestedScrollSceneScheduledStep::ContentReceiver(receiver),
        super::frame_plan::NestedScrollSceneScheduledStep::OverlayAfter {
            boundary: super::frame_plan::NestedScrollBoundarySlot::Inner,
            artifact: o1,
        },
        super::frame_plan::NestedScrollSceneScheduledStep::OverlayAfter {
            boundary: super::frame_plan::NestedScrollBoundarySlot::Outer,
            artifact: o0,
        },
    ] = scaffold.schedule.steps.as_slice()
    else {
        return Err(PropertyScrollScenePlanError::InvalidContract);
    };
    let admission = scaffold.admission;
    if receiver.stable_id != admission.content_leaf_stable_id
        || receiver.witness.content_root() != admission.content_leaf
    {
        return Err(PropertyScrollScenePlanError::InvalidContract);
    }
    let leaf_artifact = receiver.artifact.artifact();
    let leaf_terminal = checked_property_scroll_opaque_order_count(leaf_artifact)
        .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
    let recorded_leaf_bounds_bits = bounds_bits(content_zero_bounds(inner.scroll));
    let leaf_source_bounds_bits = bounds_bits(
        nested_receiver_local_content_bounds(admission.inner_source_bounds, inner.scroll)
            .ok_or(PropertyScrollScenePlanError::InvalidContract)?,
    );
    let leaf_artifact_span = super::compiler::validated_nested_scroll_content_artifact_span_stamp(
        leaf_artifact,
        admission.content_leaf,
        outer.scroll.id,
        outer.contents_clip,
        recorded_leaf_bounds_bits,
        leaf_source_bounds_bits,
        0,
        0..leaf_terminal,
    )
    .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
    let mut local_scroll = inner.scroll;
    local_scroll.layout_content_bounds_at_zero = crate::view::base_component::Rect {
        x: f32::from_bits(leaf_source_bounds_bits[0]),
        y: f32::from_bits(leaf_source_bounds_bits[1]),
        width: f32::from_bits(leaf_source_bounds_bits[2]),
        height: f32::from_bits(leaf_source_bounds_bits[3]),
    };
    let backing = plan_property_scroll_backing(
        admission.content_leaf_stable_id,
        local_scroll,
        inner.contents_clip,
        1.0,
        target_format,
        property_scroll_budget(budget),
    )
    .ok_or(PropertyScrollScenePlanError::BackingBudget)?;
    let PropertyScrollBackingPlan::Single(single) = backing else {
        return Err(PropertyScrollScenePlanError::BackingBudget);
    };
    let target = RetainedSurfaceRasterInputs {
        color: single.color_desc.clone(),
        depth: single.depth_desc.clone(),
        scale_factor_bits: 1.0_f32.to_bits(),
        source_bounds_bits: leaf_source_bounds_bits,
    };
    let leaf_stamp = validated_scroll_content_raster_stamp(
        admission.content_leaf,
        admission.content_leaf_stable_id,
        target,
        leaf_artifact_span.clone(),
        0..leaf_terminal,
    )
    .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
    if leaf_stamp.identity.color_key != single.color_key
        || leaf_stamp.target.color != single.color_desc
        || leaf_stamp.target.depth != single.depth_desc
    {
        return Err(PropertyScrollScenePlanError::InvalidContract);
    }
    let outer_boundary = SceneBoundaryId {
        ordinal: 0,
        owner: admission.outer_boundary_root,
        kind: SceneBoundaryKind::ScrollContents,
    };
    let inner_boundary = SceneBoundaryId {
        ordinal: 1,
        owner: admission.inner_boundary_root,
        kind: SceneBoundaryKind::ScrollContents,
    };
    let content_bounds = exact_u32_bounds_from_bits(leaf_source_bounds_bits)
        .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
    let group = RetainedPropertyScrollResidentGroup {
        boundary: inner_boundary,
        content_root: admission.content_leaf,
        content_stable_id: admission.content_leaf_stable_id,
        signature: RetainedPropertyScrollGroupSignature {
            content_bounds,
            tile_edge: single.budget.tile_edge,
            gutter: single.budget.gutter,
            overscan: single.budget.overscan,
            scale_factor_bits: 1.0_f32.to_bits(),
            color_format: target_format,
        },
        backing: RetainedPropertyScrollResidentBacking::Single(leaf_stamp.clone()),
    };
    if !group.is_canonical() {
        return Err(PropertyScrollScenePlanError::InvalidContract);
    }
    let make_step = |phase, artifact: &PaintArtifact| {
        Some(NestedScrollCompilerStepContract {
            phase,
            artifact: PropertyScrollPhaseArtifactIdentity::from_artifact(artifact)?,
        })
    };
    let steps = vec![
        make_step(NestedScrollCompilerPhase::OuterHostBefore, h0.artifact()),
        make_step(NestedScrollCompilerPhase::InnerHostBefore, h1.artifact()),
        make_step(NestedScrollCompilerPhase::LeafContent, leaf_artifact),
        make_step(NestedScrollCompilerPhase::InnerOverlayAfter, o1.artifact()),
        make_step(NestedScrollCompilerPhase::OuterOverlayAfter, o0.artifact()),
    ]
    .into_iter()
    .collect::<Option<Vec<_>>>()
    .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
    let witness = NestedScrollCompilerWitness {
        scene_root: admission.outer_boundary_root,
        scene_root_stable_id: admission.outer_stable_id,
        boundaries: vec![
            NestedScrollCompilerBoundaryContract {
                boundary: outer_boundary,
                parent: None,
                stable_id: admission.outer_stable_id,
                scroll: outer.scroll,
                contents_clip: outer.contents_clip,
                source_bounds_bits: bounds_bits(admission.outer_source_bounds),
            },
            NestedScrollCompilerBoundaryContract {
                boundary: inner_boundary,
                parent: Some(outer_boundary),
                stable_id: admission.inner_stable_id,
                scroll: inner.scroll,
                contents_clip: inner.contents_clip,
                source_bounds_bits: bounds_bits(admission.inner_source_bounds),
            },
        ],
        assembly_binding: NestedScrollAssemblyBindingContract {
            outer: outer_boundary,
            child: inner_boundary,
        },
        resident_binding: NestedScrollResidentBindingContract {
            boundary: inner_boundary,
            content_root: admission.content_leaf,
            content_stable_id: admission.content_leaf_stable_id,
        },
        steps,
        leaf_recorded_bounds_bits: recorded_leaf_bounds_bits,
        leaf_source_bounds_bits,
        leaf_artifact_span: leaf_artifact_span.clone(),
        leaf_stamp: leaf_stamp.clone(),
    };
    let contract = NestedScrollCompilerContract {
        compiled: witness.clone(),
        planned: witness,
    };
    let binding = RetainedPropertyScrollGroupBindingStamp {
        boundary: inner_boundary,
        content_root: group.content_root,
        content_stable_id: group.content_stable_id,
        backing_rank: group.backing_rank(),
        ordered_resident_keys: group.active_resident_keys(),
    };
    let transaction = RetainedPropertyScrollSceneTransaction {
        seal: RetainedPropertyScrollJointSeal {
            roots: vec![RetainedPropertyScrollJointRootStamp {
                ordinal: 0,
                root: admission.outer_boundary_root,
                stable_id: admission.outer_stable_id,
                boundary_span: 0..2,
            }],
            ordered_boundaries: vec![outer_boundary, inner_boundary],
            generic_bindings: Vec::new(),
            scroll_bindings: vec![binding],
        },
        generic_authority: RetainedPropertyScrollGenericAuthority::NestedScrollCompiler(contract),
        generic_full_set: Vec::new(),
        scroll_groups: vec![group],
    };
    let validated = ValidatedNestedScrollScene {
        plan,
        leaf_stamp,
        transaction,
    };
    validated
        .is_canonical()
        .then_some(validated)
        .ok_or(PropertyScrollScenePlanError::InvalidContract)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn plan_and_validate_direct_scroll_transform_scene(
    arena: &NodeArena,
    roots: &[NodeKey],
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    scale_factor: f32,
    incoming_paint_offset: [f32; 2],
    outer_scissor_rect: Option<[u32; 4]>,
    target_format: wgpu::TextureFormat,
    budget: ScrollSceneSingleTextureBudget,
) -> Result<ValidatedDirectScrollTransformTransaction, PropertyScrollScenePlanError> {
    let scaffold = plan_direct_scroll_transform_scene_scaffold(
        arena,
        roots,
        property_trees,
        paint_generations,
        scale_factor,
        incoming_paint_offset,
        outer_scissor_rect,
    )?;
    let geometry = plan_direct_scroll_transform_geometry(arena, scaffold, target_format, budget)?;
    compile_direct_scroll_transform_transaction(geometry)
}

/// S3b closes every direct S->T precondition before graph mutation.
#[allow(clippy::too_many_arguments)]
pub(crate) fn prepare_direct_scroll_transform_scene_from_pool<'a>(
    viewport: &'a mut Viewport,
    validated: ValidatedDirectScrollTransformTransaction,
    graph: &'a mut FrameGraph,
    ctx: UiBuildContext,
    clear_rgba: [f32; 4],
    frame_owner: RetainedSurfaceFrameStageOwner,
) -> Result<PreparedDirectScrollTransformScene<'a>, RetainedPropertyScrollScenePrepareError> {
    if !validated.is_canonical() {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    }
    validate_parent_target(graph, &ctx)
        .map_err(|_| RetainedPropertyScrollScenePrepareError::ParentTarget)?;
    if ctx.viewport().scale_factor().to_bits() != validated.plan.scaffold.scale_factor_bits
        || ctx.paint_offset().map(f32::to_bits) != [0.0_f32.to_bits(); 2]
        || ctx.graphics_pass_context().scissor_rect.is_some()
        || ctx.opaque_rect_order() != 0
        || ctx.current_render_transform().is_some()
        || ctx.viewport().target_format() != validated.plan.backing.color_desc.format()
        || clear_rgba.iter().any(|component| !component.is_finite())
    {
        return Err(RetainedPropertyScrollScenePrepareError::ContextMismatch);
    }
    if !viewport.retained_property_scroll_scene_stage_is_available()
        || !viewport.retained_surface_frame_stage_owner_is_active(frame_owner)
    {
        return Err(RetainedPropertyScrollScenePrepareError::StageUnavailable);
    }
    let depth_key = validated
        .plan
        .backing
        .color_key
        .depth_stencil()
        .ok_or(RetainedPropertyScrollScenePrepareError::DescriptorPair)?;
    if graph
        .declared_persistent_texture_keys()
        .any(|key| key == validated.plan.backing.color_key || key == depth_key)
    {
        return Err(
            RetainedPropertyScrollScenePrepareError::PersistentKeyAlreadyDeclared(
                validated.plan.backing.color_key,
            ),
        );
    }
    if validated.stamp.target.color != validated.plan.backing.color_desc
        || validated.stamp.target.depth != validated.plan.backing.depth_desc
        || validated.stamp.target.source_bounds_bits != validated.plan.backing.source_bounds_bits
        || validated.stamp.identity.color_key != validated.plan.backing.color_key
        || canonical_pair_bytes(
            &validated.plan.backing.color_desc,
            &validated.plan.backing.depth_desc,
        ) != Some(validated.plan.backing.pair_bytes)
        || validated.plan.backing.pair_bytes > validated.plan.backing.max_pair_bytes
        || [
            validated.plan.backing.color_desc.width(),
            validated.plan.backing.color_desc.height(),
            validated.plan.backing.depth_desc.width(),
            validated.plan.backing.depth_desc.height(),
        ]
        .into_iter()
        .any(|dimension| dimension > validated.plan.backing.max_dimension_2d)
    {
        return Err(RetainedPropertyScrollScenePrepareError::DescriptorPair);
    }
    let [
        super::frame_recorder::RecordedTransformSurfaceStep::Artifact(host_before),
        super::frame_recorder::RecordedTransformSurfaceStep::Boundary(marker),
        super::frame_recorder::RecordedTransformSurfaceStep::Artifact(overlay_after),
    ] = validated.plan.scaffold.host_steps.as_slice()
    else {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    };
    if *marker != validated.plan.scaffold.insertion {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    }
    let [super::frame_recorder::RecordedTransformSurfaceStep::Artifact(content)] =
        validated.plan.scaffold.content_steps.as_slice()
    else {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    };
    let host_bounds_bits = bounds_bits(validated.plan.scaffold.admission.source_bounds);
    if validate_scroll_scene_host_before_artifact(
        host_before.clone(),
        validated.plan.scaffold.admission.boundary_root,
        host_bounds_bits,
    )
    .is_none()
        || validate_scroll_scene_overlay_artifact(
            overlay_after.clone(),
            validated.plan.scaffold.admission.boundary_root,
            validated.plan.scaffold.scroll,
            host_bounds_bits,
        )
        .is_none()
        || super::compiler::validate_transform_property_surface_artifact(
            content,
            validated.plan.scaffold.admission.transform_content,
            validated.plan.scaffold.transform.id,
        )
        .is_none()
    {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    }
    let host_terminal = checked_property_scroll_opaque_order_count(host_before)
        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
    let overlay_terminal = checked_property_scroll_opaque_order_count(overlay_after)
        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
    let parent_terminal = host_terminal
        .checked_add(overlay_terminal)
        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
    if checked_property_scroll_opaque_order_count(content)
        != Some(validated.content_opaque_terminal)
    {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    }
    let actions = viewport
        .freeze_retained_property_scroll_scene_compile_actions_from_pool(&validated.transaction)
        .ok_or(RetainedPropertyScrollScenePrepareError::PoolContract)?;
    let resident_key = validated.stamp.identity.resident_key();
    let Some(action) = actions.get(&resident_key).copied() else {
        return Err(RetainedPropertyScrollScenePrepareError::PoolContract);
    };
    if actions.len() != 1 {
        return Err(RetainedPropertyScrollScenePrepareError::PoolContract);
    }
    let trace = RetainedPropertyScrollSceneBuildTrace {
        root_count: 1,
        generic_surface_count: 1,
        effect_surface_count: 0,
        scroll_group_count: 0,
        backing: ScrollSceneBackingKind::Single,
        tile_count: 1,
        reraster_count: usize::from(action == RetainedSurfaceCompileAction::Reraster),
        reuse_count: usize::from(action == RetainedSurfaceCompileAction::Reuse),
        content_pair_bytes: validated.plan.backing.pair_bytes,
    };
    let ValidatedDirectScrollTransformTransaction {
        plan,
        stamp,
        transaction,
        content_opaque_terminal,
    } = validated;
    let DirectScrollTransformGeometryPlan {
        scaffold,
        geometry,
        backing,
        ..
    } = plan;
    let boundary_root = scaffold.admission.boundary_root;
    let transform_content = scaffold.admission.transform_content;
    let transform = scaffold.transform.id;
    let scroll = scaffold.scroll;
    let host_bounds_bits = bounds_bits(scaffold.admission.source_bounds);
    let mut host_steps = scaffold.host_steps.into_iter();
    let Some(super::frame_recorder::RecordedTransformSurfaceStep::Artifact(host_before)) =
        host_steps.next()
    else {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    };
    let Some(super::frame_recorder::RecordedTransformSurfaceStep::Boundary(_)) = host_steps.next()
    else {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    };
    let Some(super::frame_recorder::RecordedTransformSurfaceStep::Artifact(overlay_after)) =
        host_steps.next()
    else {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    };
    let mut content_steps = scaffold.content_steps.into_iter();
    let Some(super::frame_recorder::RecordedTransformSurfaceStep::Artifact(content)) =
        content_steps.next()
    else {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    };
    if host_steps.next().is_some() || content_steps.next().is_some() {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    }
    Ok(PreparedDirectScrollTransformScene {
        viewport,
        graph,
        parent_ctx: ctx,
        host_before,
        overlay_after,
        content,
        boundary_root,
        transform_content,
        transform,
        scroll,
        host_bounds_bits,
        geometry,
        stamp,
        color_key: backing.color_key,
        color_desc: backing.color_desc,
        action,
        transaction,
        host_terminal,
        content_terminal: content_opaque_terminal,
        overlay_terminal: parent_terminal,
        clear_rgba_bits: clear_rgba.map(f32::to_bits),
        frame_owner,
        trace,
    })
}

/// Emits the direct S->T grammar in its sealed H/T/O order. The detached T
/// target owns a scene-local opaque cursor; only its attachment pair is merged
/// back into the parent before the single final composite.
pub(crate) fn emit_prepared_direct_scroll_transform_scene(
    prepared: PreparedDirectScrollTransformScene<'_>,
) -> RetainedPropertyScrollSceneBuildOutcome {
    let PreparedDirectScrollTransformScene {
        viewport,
        graph,
        mut parent_ctx,
        host_before,
        overlay_after,
        content,
        boundary_root,
        transform_content,
        transform,
        scroll,
        host_bounds_bits,
        geometry,
        stamp,
        color_key,
        color_desc,
        action,
        transaction,
        host_terminal,
        content_terminal,
        overlay_terminal,
        clear_rgba_bits,
        frame_owner,
        trace,
    } = prepared;
    assert!(viewport.retained_surface_frame_stage_owner_is_active(frame_owner));
    assert!(viewport.retained_property_scroll_scene_stage_is_available());
    assert_eq!(parent_ctx.opaque_rect_order(), 0);
    assert_eq!(stamp.identity.color_key, color_key);

    let parent_target = parent_ctx
        .current_target()
        .unwrap_or_else(|| parent_ctx.allocate_target(graph));
    parent_ctx.set_current_target(parent_target);
    graph.add_graphics_pass(ClearPass::new(
        crate::view::render_pass::clear_pass::ClearParams::new(clear_rgba_bits.map(f32::from_bits)),
        crate::view::render_pass::clear_pass::ClearInput {
            pass_context: parent_ctx.graphics_pass_context(),
            clear_depth_stencil: true,
        },
        crate::view::render_pass::clear_pass::ClearOutput {
            render_target: parent_target,
        },
    ));
    if let Some(handle) = parent_target.handle() {
        parent_ctx.set_color_target(Some(handle));
    }

    let host =
        validate_scroll_scene_host_before_artifact(host_before, boundary_root, host_bounds_bits)
            .expect("prepared direct S->T host artifact remains sealed");
    emit_validated_scroll_scene_host_before_artifact(host, graph, &mut parent_ctx);
    assert_eq!(parent_ctx.opaque_rect_order(), host_terminal);

    let mut content_ctx = UiBuildContext::from_parts(
        parent_ctx.viewport(),
        parent_ctx.layer_subtree_state_with_ancestor_clip(AncestorClipContext::default()),
    );
    content_ctx.set_current_render_transform(parent_ctx.current_render_transform());
    let content_target =
        content_ctx.allocate_persistent_target_with_desc(graph, color_desc, color_key);
    content_ctx.set_current_target(content_target);
    match action {
        RetainedSurfaceCompileAction::Reraster => {
            graph.add_graphics_pass(ClearPass::new(
                crate::view::render_pass::clear_pass::ClearParams::new([0.0; 4]),
                crate::view::render_pass::clear_pass::ClearInput {
                    pass_context: content_ctx.graphics_pass_context(),
                    clear_depth_stencil: true,
                },
                crate::view::render_pass::clear_pass::ClearOutput {
                    render_target: content_target,
                },
            ));
            let validated = super::compiler::validate_transform_property_surface_artifact(
                &content,
                transform_content,
                transform,
            )
            .expect("prepared direct S->T content artifact remains sealed");
            super::compiler::emit_validated_transform_property_surface_artifact(
                validated,
                graph,
                &mut content_ctx,
            );
        }
        RetainedSurfaceCompileAction::Reuse => {
            content_ctx.replay_opaque_rect_order_exact(0, content_terminal);
        }
    }
    assert_eq!(content_ctx.opaque_rect_order(), content_terminal);
    parent_ctx.merge_child_target_pairs(&content_ctx.into_state());
    assert_eq!(parent_ctx.opaque_rect_order(), host_terminal);

    parent_ctx.set_current_target(parent_target);
    graph.add_graphics_pass(TextureCompositePass::new(
        geometry.params(),
        TextureCompositeInput::from_render_target(
            TextureCompositeSourceIn::with_handle(
                content_target
                    .handle()
                    .expect("prepared direct S->T target has a handle"),
            ),
            Default::default(),
            parent_ctx.graphics_pass_context(),
        ),
        TextureCompositeOutput {
            render_target: parent_target,
        },
    ));

    let overlay = validate_scroll_scene_overlay_artifact(
        overlay_after,
        boundary_root,
        scroll,
        host_bounds_bits,
    )
    .expect("prepared direct S->T overlay artifact remains sealed");
    emit_validated_scroll_scene_overlay_artifact(overlay, graph, &mut parent_ctx);
    assert_eq!(parent_ctx.opaque_rect_order(), overlay_terminal);
    assert!(
        viewport.stage_retained_property_scroll_scene(transaction),
        "prepared direct S->T scene stages its joint transaction exactly once"
    );
    RetainedPropertyScrollSceneBuildOutcome {
        state: parent_ctx.into_state(),
        trace,
    }
}

/// Plans and compiler-seals the first executable mixed property grammar:
/// exact top-level translation receivers containing exactly one scroll
/// boundary. Every other schedule shape (including any effect or reverse
/// S->T edge) rejects the whole forest before pool or graph access.
#[allow(clippy::too_many_arguments)]
pub(crate) fn plan_and_validate_frame_root_scroll_scene(
    arena: &NodeArena,
    roots: &[NodeKey],
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    scale_factor: f32,
    incoming_paint_offset: [f32; 2],
    outer_scissor_rect: Option<[u32; 4]>,
    target_format: wgpu::TextureFormat,
) -> Result<ValidatedFrameRootScrollScene, PropertyScrollScenePlanError> {
    macro_rules! invalid_frame_root {
        ($stage:literal) => {{
            let _ = $stage;
            return Err(PropertyScrollScenePlanError::InvalidContract);
        }};
    }
    if roots.is_empty()
        || !scale_factor.is_finite()
        || scale_factor <= 0.0
        || incoming_paint_offset.map(f32::to_bits) != [0.0_f32.to_bits(); 2]
        || outer_scissor_rect.is_some()
        || !property_trees.validation_errors.is_empty()
        || !property_trees.transforms.is_empty()
        || !property_trees.effects.is_empty()
        || property_trees.scrolls.is_empty()
        || property_trees.scrolls.len() > roots.len()
        || !paint_generations.matches_live_snapshot(arena, roots, property_trees)
    {
        invalid_frame_root!("precondition");
    }
    let context = super::TransformSurfacePlanContext::new(incoming_paint_offset, None);
    let frame_plan = super::frame_plan::plan_property_scroll_interleave_scaffold_with_context(
        arena,
        roots,
        property_trees,
        paint_generations,
        context,
    )
    .map_err(PropertyScrollScenePlanError::Frame)?;
    let scaffold = frame_plan
        .property_scroll_planning_scaffold()
        .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
    if scaffold.roots.len() != roots.len()
        || scaffold.boundaries.len() != property_trees.scrolls.len()
        || scaffold.frame_receiver_insertions.len() != property_trees.scrolls.len()
        || !scaffold.receiver_insertions.is_empty()
        || !scaffold.effect_receiver_insertions.is_empty()
        || !scaffold.transform_effect_receiver_insertions.is_empty()
    {
        invalid_frame_root!("scaffold-shape");
    }
    let mut validated_roots = Vec::with_capacity(roots.len());
    for root in &scaffold.roots {
        let root_steps = &scaffold.schedule.steps[root.step_span.clone()];
        if root_steps.is_empty() {
            let receiver_steps = super::frame_recorder::record_property_scene_steps_for_plan(
                arena,
                &[root.root],
                property_trees,
                paint_generations,
                incoming_paint_offset,
                &super::PlannedBoundaryCutoutSet::default(),
            )
            .map_err(|fallbacks| {
                PropertyScrollScenePlanError::Frame(FramePaintPlanError {
                    reasons: fallbacks
                        .into_iter()
                        .map(FramePaintPlanRejection::Coverage)
                        .collect(),
                })
            })?;
            let receiver_compiler =
                super::compiler::validate_frame_root_plain_receiver_steps(receiver_steps)
                    .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
            validated_roots.push(ValidatedFrameRootSceneRoot::Plain(
                ValidatedFrameRootPlainRoot {
                    scene_root_ordinal: root.ordinal,
                    receiver_root: root.root,
                    receiver_stable_id: root.stable_id,
                    receiver_compiler,
                },
            ));
            continue;
        }
        let [
            super::frame_plan::PropertySceneScheduledStep::ScrollBoundary {
                boundary_ordinal,
                basis: super::frame_plan::ScrollCompositeBasis::FrameRoot,
                ..
            },
        ] = root_steps
        else {
            invalid_frame_root!("root-schedule");
        };
        let boundary = scaffold
            .boundaries
            .get(*boundary_ordinal as usize)
            .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        let insertion = scaffold
            .frame_receiver_insertions
            .iter()
            .find(|insertion| insertion.scene_root_ordinal == root.ordinal)
            .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        let cutouts = super::PlannedBoundaryCutoutSet::from_iter([(
            insertion.scroll_cutout.root,
            insertion.scroll_cutout,
        )]);
        let receiver_steps = super::frame_recorder::record_property_scene_steps_for_plan(
            arena,
            &[root.root],
            property_trees,
            paint_generations,
            incoming_paint_offset,
            &cutouts,
        )
        .map_err(|fallbacks| {
            PropertyScrollScenePlanError::Frame(FramePaintPlanError {
                reasons: fallbacks
                    .into_iter()
                    .map(FramePaintPlanRejection::Coverage)
                    .collect(),
            })
        })?;
        let insertion_matches = insertion.validates_recorded_steps(&receiver_steps);
        if !insertion_matches {
            invalid_frame_root!("receiver-steps");
        }
        let scroll_host = arena
            .get(boundary.scroll.owner)
            .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        let [content_root] = scroll_host.element.children() else {
            return Err(PropertyScrollScenePlanError::InvalidContract);
        };
        let content_root = *content_root;
        let mut scroll_host_steps =
            super::frame_recorder::record_frame_root_scroll_host_steps_for_plan(
                arena,
                boundary.scroll.owner,
                content_root,
                property_trees,
                paint_generations,
                boundary.scroll,
                boundary.contents_clip,
                incoming_paint_offset,
                insertion.scroll_cutout,
                None,
            )
            .map_err(|fallbacks| {
                PropertyScrollScenePlanError::Frame(FramePaintPlanError {
                    reasons: fallbacks
                        .into_iter()
                        .map(FramePaintPlanRejection::Coverage)
                        .collect(),
                })
            })?;
        let scroll_host_parent = arena
            .parent_of(boundary.scroll.owner)
            .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        for step in &mut scroll_host_steps {
            if let super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact) = step {
                let [owner] = artifact.owner_nodes.as_mut_slice() else {
                    return Err(PropertyScrollScenePlanError::InvalidContract);
                };
                if owner.owner != boundary.scroll.owner || owner.parent.is_some() {
                    return Err(PropertyScrollScenePlanError::InvalidContract);
                }
                owner.parent = Some(scroll_host_parent);
            }
        }
        let mut expanded_receiver_steps = Vec::new();
        for step in receiver_steps.iter().cloned() {
            match step {
                super::frame_recorder::RecordedTransformSurfaceStep::Boundary(marker)
                    if marker == insertion.scroll_cutout =>
                {
                    expanded_receiver_steps.extend(scroll_host_steps.iter().cloned());
                }
                step => expanded_receiver_steps.push(step),
            }
        }
        let Some(receiver_compiler) = super::compiler::validate_frame_root_scroll_receiver_steps(
            expanded_receiver_steps.clone(),
            insertion.scroll_cutout,
            boundary.scroll.owner,
            boundary.scroll,
        ) else {
            invalid_frame_root!("scroll-host-receiver-compiler");
        };
        let content_node = arena
            .get(content_root)
            .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        let content_element = content_node
            .element
            .as_any()
            .downcast_ref::<crate::view::base_component::Element>()
            .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        let required_paint_offset = content_element
            .exact_retained_scroll_content_subtree_recording_offset([
                boundary.scroll.offset.x,
                boundary.scroll.offset.y,
            ])
            .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        let content_witness = PaintScrollContentWitness::new(
            boundary.scroll.owner,
            content_root,
            boundary.scroll,
            boundary.contents_clip,
        )
        .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        let mut text_area_roots = Vec::new();
        let mut pending = content_node.element.children().to_vec();
        while let Some(descendant) = pending.pop() {
            let descendant_node = arena
                .get(descendant)
                .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
            if descendant_node
                .element
                .as_any()
                .is::<crate::view::base_component::TextArea>()
            {
                text_area_roots.push(descendant);
                if text_area_roots.len() > 1 {
                    return Err(PropertyScrollScenePlanError::InvalidContract);
                }
            }
            pending.extend(descendant_node.element.children().iter().copied());
        }
        let text_area_witness = text_area_roots
            .first()
            .copied()
            .map(|text_area_root| {
                let text_area_node = arena
                    .get(text_area_root)
                    .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
                let text_area = text_area_node
                    .element
                    .as_any()
                    .downcast_ref::<crate::view::base_component::TextArea>()
                    .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
                let paint_grammar = if text_area.exact_retained_property_scroll_glyph_subtree(
                    text_area_root,
                    arena,
                    required_paint_offset,
                ) {
                    crate::view::base_component::text_area::RetainedTextAreaPaintGrammar::GlyphOnly
                } else {
                    text_area
                        .exact_retained_property_scroll_selection_glyph_subtree(
                            text_area_root,
                            arena,
                            required_paint_offset,
                        )
                        .ok_or(PropertyScrollScenePlanError::InvalidContract)?
                };
                let local_scissor = text_area
                    .retained_property_scroll_local_contents_scissor(
                        content_witness.normalization_paint_offset(),
                    )
                    .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
                let local_clip_id = ClipNodeId {
                    owner: text_area_root,
                    role: ClipNodeRole::ContentsClip,
                };
                let live_chain = property_trees
                    .clip_snapshot_for(Some(local_clip_id))
                    .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
                let [live_text_area_clip, live_outer_clip] = live_chain.as_slice() else {
                    return Err(PropertyScrollScenePlanError::InvalidContract);
                };
                if *live_outer_clip != boundary.contents_clip {
                    return Err(PropertyScrollScenePlanError::InvalidContract);
                }
                PaintScrollTextAreaSubtreeWitness::new(
                    content_witness,
                    text_area_root,
                    *live_text_area_clip,
                    local_scissor,
                    paint_grammar,
                )
                .ok_or(PropertyScrollScenePlanError::InvalidContract)
            })
            .transpose()?;
        match text_area_witness {
            Some(witness) if boundary.local_content_clips == [witness.live_contents_clip()] => {}
            None if boundary.local_content_clips.is_empty() => {}
            _ => return Err(PropertyScrollScenePlanError::InvalidContract),
        }
        let content_artifact =
            super::frame_recorder::record_generalized_scroll_content_artifact_for_plan(
                arena,
                property_trees,
                paint_generations,
                content_witness,
                text_area_witness,
                required_paint_offset,
            )
            .map_err(|fallbacks| {
                PropertyScrollScenePlanError::Frame(FramePaintPlanError {
                    reasons: fallbacks
                        .into_iter()
                        .map(FramePaintPlanRejection::Coverage)
                        .collect(),
                })
            })?;
        let expected_local_clips = text_area_witness
            .map(|witness| vec![witness.local_contents_clip()])
            .unwrap_or_default();
        if content_artifact.clip_nodes != expected_local_clips {
            invalid_frame_root!("content-artifact");
        }
        let content_compiler = super::compiler::validate_frame_root_scroll_content_artifact(
            content_artifact,
            content_root,
            text_area_witness,
        )
        .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        validated_roots.push(ValidatedFrameRootSceneRoot::Scroll(
            ValidatedFrameRootScrollRoot {
                scene_root_ordinal: root.ordinal,
                receiver_root: root.root,
                receiver_stable_id: root.stable_id,
                insertion: insertion.clone(),
                receiver_identity_steps: receiver_steps,
                receiver_compiler,
                boundary: boundary.clone(),
                content_root,
                content_stable_id: content_node.element.stable_id(),
                text_area_witness,
                content_compiler,
                required_paint_offset_bits: required_paint_offset.map(f32::to_bits),
            },
        ));
    }
    if !paint_generations.matches_live_snapshot(arena, roots, property_trees) {
        return Err(PropertyScrollScenePlanError::LiveSnapshotDrift);
    }
    let scene = ValidatedFrameRootScrollScene {
        roots: validated_roots,
        scale_factor_bits: scale_factor.to_bits(),
        target_format,
    };
    if !scene.is_canonical() {
        invalid_frame_root!("scene-canonical");
    }
    Ok(scene)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn plan_and_validate_transform_scroll_scene(
    arena: &NodeArena,
    roots: &[NodeKey],
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    scale_factor: f32,
    incoming_paint_offset: [f32; 2],
    outer_scissor_rect: Option<[u32; 4]>,
    semantic_frame_time: crate::time::Instant,
    target_format: wgpu::TextureFormat,
    budget: ScrollSceneSingleTextureBudget,
) -> Result<ValidatedTransformScrollScene, PropertyScrollScenePlanError> {
    if roots.is_empty()
        || !scale_factor.is_finite()
        || scale_factor <= 0.0
        || incoming_paint_offset.map(f32::to_bits) != [0.0_f32.to_bits(); 2]
        || outer_scissor_rect.is_some()
        || !property_trees.validation_errors.is_empty()
        || !paint_generations.matches_live_snapshot(arena, roots, property_trees)
    {
        return Err(PropertyScrollScenePlanError::InvalidContract);
    }
    let context = super::TransformSurfacePlanContext::new(incoming_paint_offset, None);
    let frame_plan = super::frame_plan::plan_property_scroll_interleave_scaffold_with_context(
        arena,
        roots,
        property_trees,
        paint_generations,
        context,
    )
    .map_err(PropertyScrollScenePlanError::Frame)?;
    let scaffold = frame_plan
        .property_scroll_planning_scaffold()
        .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
    if scaffold.roots.len() != roots.len()
        || scaffold.boundaries.len() != roots.len()
        || scaffold
            .receiver_insertions
            .len()
            .checked_add(scaffold.same_owner_transform_scroll_insertions.len())
            != Some(roots.len())
    {
        return Err(PropertyScrollScenePlanError::InvalidContract);
    }
    let mut validated_roots = Vec::with_capacity(roots.len());
    let mut seen_receivers = FxHashSet::default();
    let mut seen_boundaries = FxHashSet::default();
    for root in &scaffold.roots {
        let [
            super::frame_plan::PropertySceneScheduledStep::RetainedSurface {
                boundary: super::frame_plan::PropertyScheduledSurfaceBoundary::Transform(receiver),
                parent: None,
            },
            super::frame_plan::PropertySceneScheduledStep::ScrollBoundary {
                boundary_ordinal,
                basis: super::frame_plan::ScrollCompositeBasis::Transform(basis),
                ..
            },
        ] = &scaffold.schedule.steps[root.step_span.clone()]
        else {
            return Err(PropertyScrollScenePlanError::InvalidContract);
        };
        let boundary = scaffold
            .boundaries
            .get(*boundary_ordinal as usize)
            .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        let generic_insertion = scaffold
            .receiver_insertions
            .iter()
            .find(|insertion| insertion.scene_root_ordinal == root.ordinal);
        let same_owner_insertion = scaffold
            .same_owner_transform_scroll_insertions
            .iter()
            .find(|insertion| insertion.receiver.scene_root_ordinal == root.ordinal);
        let insertion = match (generic_insertion, same_owner_insertion) {
            (Some(insertion), None) => insertion,
            (None, Some(insertion)) => &insertion.receiver,
            _ => return Err(PropertyScrollScenePlanError::InvalidContract),
        };
        if *receiver != *basis
            || receiver.owner != root.root
            || receiver.parent.is_some()
            || super::compiler::direct_translation_bits(receiver.viewport_matrix).is_none()
            || insertion.scroll_boundary_ordinal != *boundary_ordinal
            || !seen_receivers.insert(receiver.owner)
            || !seen_boundaries.insert(boundary.scroll.owner)
        {
            return Err(PropertyScrollScenePlanError::InvalidContract);
        }
        let receiver_node = arena
            .get(receiver.owner)
            .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        let receiver_element = receiver_node
            .element
            .as_any()
            .downcast_ref::<crate::view::base_component::Element>()
            .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        let receiver_steps = if let Some(same_owner) = same_owner_insertion {
            super::frame_recorder::record_same_owner_transform_scroll_receiver_steps_for_plan(
                arena,
                receiver.owner,
                property_trees,
                paint_generations,
                same_owner.transform,
                same_owner.scroll,
                same_owner.contents_clip,
                insertion.scroll_cutout,
            )
        } else {
            super::frame_recorder::record_property_scroll_receiver_steps_for_plan(
                arena,
                receiver.owner,
                property_trees,
                paint_generations,
                super::PaintTransformSurfaceWitness::canonical_root(receiver.owner),
                incoming_paint_offset,
                insertion.scroll_cutout,
            )
        }
        .map_err(|fallbacks| {
            PropertyScrollScenePlanError::Frame(FramePaintPlanError {
                reasons: fallbacks
                    .into_iter()
                    .map(FramePaintPlanRejection::Coverage)
                    .collect(),
            })
        })?;
        if !insertion.validates_recorded_steps(&receiver_steps)
            || receiver_steps.iter().any(|step| match step {
                super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact) => {
                    !artifact.clip_nodes.is_empty()
                        || !artifact.effect_nodes.is_empty()
                        || super::compiler::validate_transform_property_surface_artifact(
                            artifact,
                            receiver.owner,
                            receiver.id,
                        )
                        .is_none()
                }
                super::frame_recorder::RecordedTransformSurfaceStep::Boundary(marker) => {
                    *marker != insertion.scroll_cutout
                }
            })
        {
            return Err(PropertyScrollScenePlanError::InvalidContract);
        }
        let scroll = if same_owner_insertion.is_some() {
            plan_exact_same_owner_transform_scroll_boundary(
                arena,
                *receiver,
                boundary,
                property_trees,
                paint_generations,
                scale_factor,
            )
        } else {
            plan_exact_transform_scroll_boundary(
                arena,
                *receiver,
                boundary,
                property_trees,
                paint_generations,
                scale_factor,
                None,
            )
        }
        .map_err(PropertyScrollScenePlanError::Frame)?;
        let scroll = property_scroll_plan_from_exact_scene(
            scroll,
            scale_factor,
            semantic_frame_time,
            target_format,
            budget,
        )?;
        let boundary = validate_property_scroll_boundary_from_frozen_plan(scroll)
            .map_err(|_| PropertyScrollScenePlanError::InvalidContract)?;
        let raster_bounds = transform_scroll_receiver_raster_bounds(
            &receiver_steps,
            boundary.planner.seal.admission.source_bounds,
        )
        .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        let geometry = receiver_element
            .exact_transform_receiver_geometry_snapshot_for_raster_bounds(
                raster_bounds,
                incoming_paint_offset,
                None,
            )
            .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        if geometry
            .viewport_transform
            .to_cols_array()
            .map(f32::to_bits)
            != receiver.viewport_matrix.to_cols_array().map(f32::to_bits)
            || geometry.outer_scissor_rect.is_some()
        {
            return Err(PropertyScrollScenePlanError::InvalidContract);
        }
        validated_roots.push(ValidatedTransformScrollRoot {
            scene_root_ordinal: root.ordinal,
            receiver_root: receiver.owner,
            receiver_stable_id: root.stable_id,
            receiver: *receiver,
            geometry,
            insertion: insertion.clone(),
            same_owner_insertion: same_owner_insertion.cloned(),
            receiver_steps,
            boundary,
        });
    }
    if !paint_generations.matches_live_snapshot(arena, roots, property_trees) {
        return Err(PropertyScrollScenePlanError::LiveSnapshotDrift);
    }
    let scene = ValidatedTransformScrollScene {
        roots: validated_roots,
        scale_factor_bits: scale_factor.to_bits(),
        semantic_frame_time,
        target_format,
        budget: property_scroll_budget(budget),
    };
    scene
        .is_canonical()
        .then_some(scene)
        .ok_or(PropertyScrollScenePlanError::InvalidContract)
}

#[allow(clippy::too_many_arguments)]
#[allow(dead_code)]
pub(crate) fn plan_and_validate_effect_scroll_scene_checkpoint(
    arena: &NodeArena,
    roots: &[NodeKey],
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    scale_factor: f32,
    incoming_paint_offset: [f32; 2],
    outer_scissor_rect: Option<[u32; 4]>,
    semantic_frame_time: crate::time::Instant,
    target_format: wgpu::TextureFormat,
    budget: ScrollSceneSingleTextureBudget,
) -> Result<ValidatedEffectScrollSceneCheckpoint, PropertyScrollScenePlanError> {
    if roots.is_empty()
        || !scale_factor.is_finite()
        || scale_factor <= 0.0
        || incoming_paint_offset.map(f32::to_bits) != [0.0_f32.to_bits(); 2]
        || outer_scissor_rect.is_some()
        || !property_trees.validation_errors.is_empty()
        || !paint_generations.matches_live_snapshot(arena, roots, property_trees)
    {
        return Err(PropertyScrollScenePlanError::InvalidContract);
    }
    let context = super::TransformSurfacePlanContext::new(incoming_paint_offset, None);
    let frame_plan = super::frame_plan::plan_property_scroll_interleave_scaffold_with_context(
        arena,
        roots,
        property_trees,
        paint_generations,
        context,
    )
    .map_err(PropertyScrollScenePlanError::Frame)?;
    let scaffold = frame_plan
        .property_scroll_planning_scaffold()
        .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
    if scaffold.roots.len() != roots.len()
        || scaffold.boundaries.len() != roots.len()
        || !scaffold.receiver_insertions.is_empty()
        || scaffold.effect_receiver_insertions.len()
            + scaffold.same_owner_effect_scroll_insertions.len()
            != roots.len()
    {
        return Err(PropertyScrollScenePlanError::InvalidContract);
    }
    let mut validated_roots = Vec::with_capacity(roots.len());
    let mut seen_receivers = FxHashSet::default();
    let mut seen_boundaries = FxHashSet::default();
    for root in &scaffold.roots {
        let [
            super::frame_plan::PropertySceneScheduledStep::RetainedSurface {
                boundary: super::frame_plan::PropertyScheduledSurfaceBoundary::Effect(receiver),
                parent: None,
            },
            super::frame_plan::PropertySceneScheduledStep::ScrollBoundary {
                boundary_ordinal,
                basis: super::frame_plan::ScrollCompositeBasis::Effect(basis),
                ..
            },
        ] = &scaffold.schedule.steps[root.step_span.clone()]
        else {
            return Err(PropertyScrollScenePlanError::InvalidContract);
        };
        let boundary = scaffold
            .boundaries
            .get(*boundary_ordinal as usize)
            .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        let generic_insertion = scaffold
            .effect_receiver_insertions
            .iter()
            .find(|insertion| insertion.scene_root_ordinal == root.ordinal);
        let same_owner_insertion = scaffold
            .same_owner_effect_scroll_insertions
            .iter()
            .find(|insertion| insertion.receiver.scene_root_ordinal == root.ordinal);
        let insertion = match (generic_insertion, same_owner_insertion) {
            (Some(insertion), None) => insertion,
            (None, Some(insertion)) => &insertion.receiver,
            _ => return Err(PropertyScrollScenePlanError::InvalidContract),
        };
        if *receiver != *basis
            || receiver.owner != root.root
            || receiver.parent.is_some()
            || insertion.scroll_boundary_ordinal != *boundary_ordinal
            || !seen_receivers.insert(receiver.owner)
            || !seen_boundaries.insert(boundary.scroll.owner)
        {
            return Err(PropertyScrollScenePlanError::InvalidContract);
        }
        let receiver_steps = if let Some(same_owner) = same_owner_insertion {
            super::frame_recorder::record_same_owner_effect_scroll_receiver_steps_for_plan(
                arena,
                receiver.owner,
                property_trees,
                paint_generations,
                &insertion.artifact_contract,
                same_owner.effect,
                same_owner.scroll,
                same_owner.contents_clip,
                insertion.scroll_cutout,
            )
        } else {
            super::frame_recorder::record_property_effect_scroll_receiver_steps_for_plan(
                arena,
                receiver.owner,
                property_trees,
                paint_generations,
                &insertion.artifact_contract,
                incoming_paint_offset,
                insertion.scroll_cutout,
                None,
            )
        }
        .map_err(|fallbacks| {
            PropertyScrollScenePlanError::Frame(FramePaintPlanError {
                reasons: fallbacks
                    .into_iter()
                    .map(FramePaintPlanRejection::Coverage)
                    .collect(),
            })
        })?;
        if !insertion.validates_recorded_steps(&receiver_steps)
            || receiver_steps.iter().any(|step| match step {
                super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact) => {
                    super::compiler::validate_effect_property_surface_artifact(
                        artifact,
                        &insertion.artifact_contract,
                    )
                    .is_none()
                }
                super::frame_recorder::RecordedTransformSurfaceStep::Boundary(marker) => {
                    *marker != insertion.scroll_cutout
                }
            })
        {
            return Err(PropertyScrollScenePlanError::InvalidContract);
        }
        let scroll = if let Some(same_owner) = same_owner_insertion {
            plan_exact_same_owner_effect_scroll_boundary(
                arena,
                same_owner,
                boundary,
                property_trees,
                paint_generations,
                scale_factor,
            )
        } else {
            plan_exact_effect_scroll_boundary_checkpoint(
                arena,
                *receiver,
                boundary,
                property_trees,
                paint_generations,
                scale_factor,
                None,
            )
        }
        .map_err(PropertyScrollScenePlanError::Frame)?;
        let scroll = property_scroll_plan_from_exact_scene(
            scroll,
            scale_factor,
            semantic_frame_time,
            target_format,
            budget,
        )?;
        let boundary = validate_property_scroll_boundary_from_frozen_plan(scroll)
            .map_err(|_| PropertyScrollScenePlanError::InvalidContract)?;
        let actual_bounds = transform_scroll_receiver_raster_bounds(
            &receiver_steps,
            boundary.planner.seal.admission.source_bounds,
        )
        .map(bounds_bits)
        .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        if actual_bounds != insertion.raster_bounds_bits {
            return Err(PropertyScrollScenePlanError::InvalidContract);
        }
        let composite = EffectScrollCompositeWitness::new(insertion, *receiver)
            .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        validated_roots.push(ValidatedEffectScrollRootCheckpoint {
            scene_root_ordinal: root.ordinal,
            receiver_root: receiver.owner,
            receiver_stable_id: root.stable_id,
            receiver: *receiver,
            insertion: insertion.clone(),
            same_owner_insertion: same_owner_insertion.cloned(),
            composite,
            receiver_steps,
            boundary,
        });
    }
    if !paint_generations.matches_live_snapshot(arena, roots, property_trees) {
        return Err(PropertyScrollScenePlanError::LiveSnapshotDrift);
    }
    let scene = ValidatedEffectScrollSceneCheckpoint {
        roots: validated_roots,
        scale_factor_bits: scale_factor.to_bits(),
        semantic_frame_time,
        target_format,
        budget: property_scroll_budget(budget),
    };
    scene
        .is_canonical()
        .then_some(scene)
        .ok_or(PropertyScrollScenePlanError::InvalidContract)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn plan_and_validate_transform_effect_scroll_scene(
    arena: &NodeArena,
    roots: &[NodeKey],
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    scale_factor: f32,
    incoming_paint_offset: [f32; 2],
    outer_scissor_rect: Option<[u32; 4]>,
    semantic_frame_time: crate::time::Instant,
    target_format: wgpu::TextureFormat,
    budget: ScrollSceneSingleTextureBudget,
) -> Result<ValidatedTransformEffectScrollScene, PropertyScrollScenePlanError> {
    if roots.is_empty()
        || !scale_factor.is_finite()
        || scale_factor <= 0.0
        || incoming_paint_offset.map(f32::to_bits) != [0.0_f32.to_bits(); 2]
        || outer_scissor_rect.is_some()
        || !property_trees.validation_errors.is_empty()
        || !paint_generations.matches_live_snapshot(arena, roots, property_trees)
    {
        return Err(PropertyScrollScenePlanError::InvalidContract);
    }
    let context = super::TransformSurfacePlanContext::new(incoming_paint_offset, None);
    let frame_plan = super::frame_plan::plan_property_scroll_interleave_scaffold_with_context(
        arena,
        roots,
        property_trees,
        paint_generations,
        context,
    )
    .map_err(PropertyScrollScenePlanError::Frame)?;
    let scaffold = frame_plan
        .property_scroll_planning_scaffold()
        .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
    if scaffold.roots.len() != roots.len()
        || scaffold.boundaries.len() != roots.len()
        || !scaffold.receiver_insertions.is_empty()
        || !scaffold.effect_receiver_insertions.is_empty()
        || scaffold.transform_effect_receiver_insertions.len() != roots.len()
    {
        return Err(PropertyScrollScenePlanError::InvalidContract);
    }
    let coverage_error = |fallbacks: Vec<super::FrameArtifactFallbackReason>| {
        PropertyScrollScenePlanError::Frame(FramePaintPlanError {
            reasons: fallbacks
                .into_iter()
                .map(FramePaintPlanRejection::Coverage)
                .collect(),
        })
    };
    let mut validated_roots = Vec::with_capacity(roots.len());
    // Owner identity and boundary-role identity are deliberately separate:
    // one native node may own the sealed T -> E pair, while each role remains
    // unique across the forest.
    let mut seen_transform_owners = FxHashSet::default();
    let mut seen_effect_owners = FxHashSet::default();
    let mut seen_scroll_owners = FxHashSet::default();
    for root in &scaffold.roots {
        let [
            super::frame_plan::PropertySceneScheduledStep::RetainedSurface {
                boundary: super::frame_plan::PropertyScheduledSurfaceBoundary::Transform(outer),
                parent: None,
            },
            super::frame_plan::PropertySceneScheduledStep::RetainedSurface {
                boundary: super::frame_plan::PropertyScheduledSurfaceBoundary::Effect(inner),
                parent:
                    Some(super::frame_plan::PropertyScheduledSurfaceBoundaryId::Transform(parent)),
            },
            super::frame_plan::PropertySceneScheduledStep::ScrollBoundary {
                boundary_ordinal,
                basis: super::frame_plan::ScrollCompositeBasis::Effect(basis),
                ..
            },
        ] = &scaffold.schedule.steps[root.step_span.clone()]
        else {
            return Err(PropertyScrollScenePlanError::InvalidContract);
        };
        let boundary_contract = scaffold
            .boundaries
            .get(*boundary_ordinal as usize)
            .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        let insertion = scaffold
            .transform_effect_receiver_insertions
            .iter()
            .find(|insertion| insertion.scene_root_ordinal == root.ordinal)
            .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        if outer.id != *parent
            || inner != basis
            || outer.owner != root.root
            || insertion.outer_receiver != *outer
            || insertion.inner.receiver != *inner
            || insertion.inner.scroll_boundary_ordinal != *boundary_ordinal
            || !seen_transform_owners.insert(outer.owner)
            || !seen_effect_owners.insert(inner.owner)
            || !seen_scroll_owners.insert(boundary_contract.scroll.owner)
        {
            return Err(PropertyScrollScenePlanError::InvalidContract);
        }
        let outer_cutouts =
            super::PlannedBoundaryCutoutSet::from_iter([(inner.owner, insertion.effect_cutout)]);
        let outer_steps = super::frame_recorder::record_transform_property_surface_steps_for_plan(
            arena,
            outer.owner,
            property_trees,
            paint_generations,
            super::PaintTransformSurfaceWitness::canonical_root(outer.owner),
            incoming_paint_offset,
            &outer_cutouts,
        )
        .map_err(&coverage_error)?;
        let same_owner = outer.owner == inner.owner;
        let inner_steps = if same_owner {
            let consumed =
                super::ConsumedSameOwnerTransformBoundaryWitness::new(outer.owner, outer.id)
                    .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
            super::frame_recorder::record_same_owner_transform_effect_surface_steps_for_plan(
                arena,
                property_trees,
                paint_generations,
                &insertion.inner.artifact_contract,
                incoming_paint_offset,
                &super::PlannedBoundaryCutoutSet::from_iter([(
                    insertion.inner.scroll_cutout.root,
                    insertion.inner.scroll_cutout,
                )]),
                consumed,
            )
            .map_err(&coverage_error)?
        } else {
            let consumed =
                super::ConsumedAncestorTransformWitness::new(outer.owner, inner.owner, outer.id)
                    .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
            super::frame_recorder::record_property_effect_scroll_receiver_steps_for_plan(
                arena,
                inner.owner,
                property_trees,
                paint_generations,
                &insertion.inner.artifact_contract,
                incoming_paint_offset,
                insertion.inner.scroll_cutout,
                Some(consumed),
            )
            .map_err(&coverage_error)?
        };
        if !insertion.validates_outer_recorded_steps(&outer_steps)
            || !insertion.inner.validates_recorded_steps(&inner_steps)
        {
            return Err(PropertyScrollScenePlanError::InvalidContract);
        }
        let consumed_scroll_transform = super::ConsumedAncestorTransformWitness::new(
            outer.owner,
            boundary_contract.scroll.owner,
            outer.id,
        )
        .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        let scroll = plan_exact_effect_scroll_boundary_checkpoint(
            arena,
            *inner,
            boundary_contract,
            property_trees,
            paint_generations,
            scale_factor,
            Some(consumed_scroll_transform),
        )
        .map_err(PropertyScrollScenePlanError::Frame)?;
        let scroll = property_scroll_plan_from_exact_scene(
            scroll,
            scale_factor,
            semantic_frame_time,
            target_format,
            budget,
        )?;
        let boundary = validate_property_scroll_boundary_from_frozen_plan(scroll)
            .map_err(|_| PropertyScrollScenePlanError::InvalidContract)?;
        let actual_inner_bounds = transform_scroll_receiver_raster_bounds(
            &inner_steps,
            boundary.planner.seal.admission.source_bounds,
        )
        .map(bounds_bits)
        .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        if actual_inner_bounds != insertion.inner.raster_bounds_bits {
            return Err(PropertyScrollScenePlanError::InvalidContract);
        }
        let composite = EffectScrollCompositeWitness::new(&insertion.inner, *inner)
            .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        validated_roots.push(ValidatedTransformEffectScrollRoot {
            scene_root_ordinal: root.ordinal,
            outer_receiver: *outer,
            outer_stable_id: root.stable_id,
            outer_geometry: insertion.outer_geometry,
            outer_steps,
            insertion: insertion.clone(),
            inner_steps,
            composite,
            boundary,
        });
    }
    if !paint_generations.matches_live_snapshot(arena, roots, property_trees) {
        return Err(PropertyScrollScenePlanError::LiveSnapshotDrift);
    }
    let scene = ValidatedTransformEffectScrollScene {
        roots: validated_roots,
        scale_factor_bits: scale_factor.to_bits(),
        semantic_frame_time,
        target_format,
        budget: property_scroll_budget(budget),
    };
    scene
        .is_canonical()
        .then_some(scene)
        .ok_or(PropertyScrollScenePlanError::InvalidContract)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn plan_and_validate_effect_transform_scroll_scene(
    arena: &NodeArena,
    roots: &[NodeKey],
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    scale_factor: f32,
    incoming_paint_offset: [f32; 2],
    outer_scissor_rect: Option<[u32; 4]>,
    semantic_frame_time: crate::time::Instant,
    target_format: wgpu::TextureFormat,
    budget: ScrollSceneSingleTextureBudget,
) -> Result<ValidatedEffectTransformScrollScene, PropertyScrollScenePlanError> {
    if roots.is_empty()
        || !scale_factor.is_finite()
        || scale_factor <= 0.0
        || incoming_paint_offset.map(f32::to_bits) != [0.0_f32.to_bits(); 2]
        || outer_scissor_rect.is_some()
        || !property_trees.validation_errors.is_empty()
        || !paint_generations.matches_live_snapshot(arena, roots, property_trees)
    {
        return Err(PropertyScrollScenePlanError::InvalidContract);
    }
    let context = super::TransformSurfacePlanContext::new(incoming_paint_offset, None);
    let frame_plan = super::frame_plan::plan_property_scroll_interleave_scaffold_with_context(
        arena,
        roots,
        property_trees,
        paint_generations,
        context,
    )
    .map_err(PropertyScrollScenePlanError::Frame)?;
    let scaffold = frame_plan
        .property_scroll_planning_scaffold()
        .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
    if scaffold.roots.len() != roots.len()
        || scaffold.boundaries.len() != roots.len()
        || scaffold.effect_transform_receiver_insertions.len() != roots.len()
        || !scaffold.receiver_insertions.is_empty()
        || !scaffold.effect_receiver_insertions.is_empty()
        || !scaffold.transform_effect_receiver_insertions.is_empty()
    {
        return Err(PropertyScrollScenePlanError::InvalidContract);
    }
    let coverage_error = |fallbacks: Vec<super::FrameArtifactFallbackReason>| {
        PropertyScrollScenePlanError::Frame(FramePaintPlanError {
            reasons: fallbacks
                .into_iter()
                .map(FramePaintPlanRejection::Coverage)
                .collect(),
        })
    };
    let mut validated_roots = Vec::with_capacity(roots.len());
    let mut seen_owners = FxHashSet::default();
    for root in &scaffold.roots {
        let [
            super::frame_plan::PropertySceneScheduledStep::RetainedSurface {
                boundary: super::frame_plan::PropertyScheduledSurfaceBoundary::Effect(outer),
                parent: None,
            },
            super::frame_plan::PropertySceneScheduledStep::RetainedSurface {
                boundary: super::frame_plan::PropertyScheduledSurfaceBoundary::Transform(inner),
                parent: Some(super::frame_plan::PropertyScheduledSurfaceBoundaryId::Effect(parent)),
            },
            super::frame_plan::PropertySceneScheduledStep::ScrollBoundary {
                boundary_ordinal,
                basis: super::frame_plan::ScrollCompositeBasis::Transform(basis),
                ..
            },
        ] = &scaffold.schedule.steps[root.step_span.clone()]
        else {
            return Err(PropertyScrollScenePlanError::InvalidContract);
        };
        let boundary_contract = scaffold
            .boundaries
            .get(*boundary_ordinal as usize)
            .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        let insertion = scaffold
            .effect_transform_receiver_insertions
            .iter()
            .find(|insertion| insertion.scene_root_ordinal == root.ordinal)
            .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        if outer.id != *parent
            || inner != basis
            || insertion.outer_receiver != *outer
            || insertion.inner.receiver != *inner
            || insertion.inner.scroll_boundary_ordinal != *boundary_ordinal
            || !seen_owners.insert(outer.owner)
            || !seen_owners.insert(inner.owner)
            || !seen_owners.insert(boundary_contract.scroll.owner)
        {
            return Err(PropertyScrollScenePlanError::InvalidContract);
        }
        let outer_cutouts =
            super::PlannedBoundaryCutoutSet::from_iter([(inner.owner, insertion.transform_cutout)]);
        let outer_steps = super::frame_recorder::record_effect_property_surface_steps_for_plan(
            arena,
            property_trees,
            paint_generations,
            &insertion.outer_artifact_contract,
            incoming_paint_offset,
            &outer_cutouts,
            None,
        )
        .map_err(&coverage_error)?;
        let consumed_effect = super::ConsumedAncestorEffectWitness::new(
            outer.owner,
            inner.owner,
            *outer,
            Some(outer.id),
            outer.parent,
        )
        .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        let inner_cutouts = super::PlannedBoundaryCutoutSet::from_iter([(
            boundary_contract.scroll.owner,
            insertion.inner.scroll_cutout,
        )]);
        let inner_steps =
            super::frame_recorder::record_effect_transform_property_surface_steps_for_plan(
                arena,
                inner.owner,
                property_trees,
                paint_generations,
                super::PaintTransformSurfaceWitness::canonical_root(inner.owner),
                incoming_paint_offset,
                &inner_cutouts,
                consumed_effect,
            )
            .map_err(&coverage_error)?;
        if !insertion.validates_outer_recorded_steps(&outer_steps)
            || !insertion.inner.validates_recorded_steps(&inner_steps)
        {
            return Err(PropertyScrollScenePlanError::InvalidContract);
        }
        let scroll = plan_exact_transform_scroll_boundary(
            arena,
            *inner,
            boundary_contract,
            property_trees,
            paint_generations,
            scale_factor,
            Some(consumed_effect),
        )
        .map_err(PropertyScrollScenePlanError::Frame)?;
        let scroll = property_scroll_plan_from_exact_scene(
            scroll,
            scale_factor,
            semantic_frame_time,
            target_format,
            budget,
        )?;
        let boundary = validate_property_scroll_boundary_from_frozen_plan(scroll)
            .map_err(|_| PropertyScrollScenePlanError::InvalidContract)?;
        let actual_inner_bounds = transform_scroll_receiver_raster_bounds(
            &inner_steps,
            boundary.planner.seal.admission.source_bounds,
        )
        .map(bounds_bits)
        .ok_or(PropertyScrollScenePlanError::InvalidContract)?;
        if actual_inner_bounds != bounds_bits(insertion.inner_geometry.source_bounds) {
            return Err(PropertyScrollScenePlanError::InvalidContract);
        }
        let outer_composite = EffectScrollCompositeWitness {
            source_bounds_bits: insertion.outer_raster_bounds_bits,
            opacity_bits: outer.opacity.to_bits(),
            effect_generation: outer.generation,
        };
        if !outer_composite.matches_receiver(*outer) {
            return Err(PropertyScrollScenePlanError::InvalidContract);
        }
        validated_roots.push(ValidatedEffectTransformScrollRoot {
            scene_root_ordinal: root.ordinal,
            insertion: insertion.clone(),
            outer_steps,
            inner_steps,
            outer_composite,
            boundary,
        });
    }
    if !paint_generations.matches_live_snapshot(arena, roots, property_trees) {
        return Err(PropertyScrollScenePlanError::LiveSnapshotDrift);
    }
    let scene = ValidatedEffectTransformScrollScene {
        roots: validated_roots,
        scale_factor_bits: scale_factor.to_bits(),
        semantic_frame_time,
        target_format,
        budget: property_scroll_budget(budget),
    };
    scene
        .is_canonical()
        .then_some(scene)
        .ok_or(PropertyScrollScenePlanError::InvalidContract)
}

fn exact_u32_bounds_from_bits(bits: [u32; 4]) -> Option<[u32; 4]> {
    let values = bits.map(f32::from_bits);
    if values.iter().any(|value| {
        !value.is_finite()
            || *value < 0.0
            || value.fract().to_bits() != 0.0_f32.to_bits()
            || *value > u32::MAX as f32
    }) || values[2] <= 0.0
        || values[3] <= 0.0
    {
        return None;
    }
    let bounds = values.map(|value| value as u32);
    bounds[0].checked_add(bounds[2])?;
    bounds[1].checked_add(bounds[3])?;
    Some(bounds)
}

fn retained_property_scroll_group_from_prepared(
    boundary: SceneBoundaryId,
    content_root: NodeKey,
    content_stable_id: u64,
    content_bounds: [u32; 4],
    backing: &PreparedRetainedPropertyScrollBacking,
    compile_backing: &PropertyScrollContentBackingCompileStamp,
) -> Option<RetainedPropertyScrollResidentGroup> {
    let (signature, resident_backing) = match (backing, compile_backing) {
        (
            PreparedRetainedPropertyScrollBacking::Single { stamp, .. },
            PropertyScrollContentBackingCompileStamp::Single(compile),
        ) => (
            RetainedPropertyScrollGroupSignature {
                content_bounds,
                tile_edge: compile.budget.tile_edge,
                gutter: compile.budget.gutter,
                overscan: compile.budget.overscan,
                scale_factor_bits: stamp.target.scale_factor_bits,
                color_format: stamp.target.color.format(),
            },
            RetainedPropertyScrollResidentBacking::Single(stamp.clone()),
        ),
        (
            PreparedRetainedPropertyScrollBacking::Tiled { tiles, .. },
            PropertyScrollContentBackingCompileStamp::Tiled(compile),
        ) => (
            RetainedPropertyScrollGroupSignature {
                content_bounds,
                tile_edge: compile.tile_edge,
                gutter: compile.gutter,
                overscan: compile.overscan,
                scale_factor_bits: tiles.first()?.stamp.target.scale_factor_bits,
                color_format: tiles.first()?.stamp.target.color.format(),
            },
            RetainedPropertyScrollResidentBacking::Tiled(
                tiles.iter().map(|tile| tile.stamp.clone()).collect(),
            ),
        ),
        _ => return None,
    };
    let group = RetainedPropertyScrollResidentGroup {
        boundary,
        content_root,
        content_stable_id,
        signature,
        backing: resident_backing,
    };
    group.is_canonical().then_some(group)
}

enum PreparedRetainedPropertyScrollBoundaryAuthority {
    Existing {
        host_before: ValidatedScrollSceneHostBeforeArtifact,
        content: ValidatedScrollSceneContentArtifact,
        overlay: ValidatedScrollSceneOverlayArtifact,
    },
    AtomicProjectionTextArea {
        emission: ValidatedScrollSceneAtomicProjectionTextAreaHostEmission,
    },
    AtomicProjectionSelectionTextArea {
        emission: ValidatedScrollSceneAtomicProjectionSelectionTextAreaHostEmission,
    },
}

impl PreparedRetainedPropertyScrollBoundaryAuthority {
    fn existing_host_overlay(
        &self,
    ) -> Option<(
        &ValidatedScrollSceneHostBeforeArtifact,
        &ValidatedScrollSceneOverlayArtifact,
    )> {
        match self {
            Self::Existing {
                host_before,
                overlay,
                ..
            } => Some((host_before, overlay)),
            Self::AtomicProjectionTextArea { .. }
            | Self::AtomicProjectionSelectionTextArea { .. } => None,
        }
    }
}

struct PreparedRetainedPropertyScrollBoundaryParts {
    authority: PreparedRetainedPropertyScrollBoundaryAuthority,
    backing: PreparedRetainedPropertyScrollBacking,
    post_composite: PropertyScrollPostCompositeSchedule,
    group: RetainedPropertyScrollResidentGroup,
    host_local_terminal: u32,
    content_local_terminal: u32,
    parent_local_terminal: u32,
    pair_bytes: u64,
    tile_count: usize,
    backing_kind: ScrollSceneBackingKind,
}

fn validate_property_scroll_content_authority(
    planner: &PropertyScrollScenePlanSeal,
    artifact: PaintArtifact,
    content_bounds_bits: [u32; 4],
) -> Option<ValidatedScrollSceneContentArtifact> {
    if !planner.admission.exactly_corresponds_to(
        planner.text_area_subtree_admission,
        planner.interactive_text_area_subtree_admission,
        &planner.post_composite,
    ) {
        return None;
    }
    if let Some(text_area) = planner.admission.text_area_subtree_snapshot() {
        let local_clip = match artifact.clip_nodes.as_slice() {
            [local_clip] => *local_clip,
            _ => return None,
        };
        validate_scroll_scene_text_area_content_artifact(
            artifact,
            planner.admission.child,
            text_area.text_area_root,
            text_area.paint_grammar,
            local_clip,
            content_bounds_bits,
        )
    } else if let Some(text_area) = planner.admission.interactive_text_area_subtree_snapshot() {
        let local_clip = match artifact.clip_nodes.as_slice() {
            [local_clip] => *local_clip,
            _ => return None,
        };
        let preedit = match planner.interactive_resident.as_ref() {
            Some(RetainedInteractiveTextAreaResidentRasterSeal::FocusedPreeditGlyphs(seal)) => {
                Some(seal.clone())
            }
            _ => None,
        };
        let validated = validate_scroll_scene_interactive_text_area_content_artifact(
            artifact,
            planner.admission.child,
            text_area.text_area_root,
            text_area.paint_grammar,
            preedit,
            local_clip,
            content_bounds_bits,
        )?;
        let (content, resident) = validated.into_parts();
        (Some(&resident) == planner.interactive_resident.as_ref()).then_some(content)
    } else {
        validate_scroll_scene_content_artifact(
            artifact,
            planner.admission.child,
            content_bounds_bits,
        )
    }
}

fn prepare_retained_property_scroll_boundary_parts(
    boundary: ValidatedPropertyScrollBoundary,
    global_boundary: SceneBoundaryId,
    scale_factor_bits: u32,
    graph_keys: &FxHashSet<PersistentTextureKey>,
    declared: &mut FxHashSet<PersistentTextureKey>,
) -> Result<PreparedRetainedPropertyScrollBoundaryParts, RetainedPropertyScrollScenePrepareError> {
    let scale_factor = f32::from_bits(scale_factor_bits);
    if !scale_factor.is_finite() || scale_factor <= 0.0 {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    }
    if matches!(
        boundary.steps.as_slice(),
        [
            PropertyScrollCompiledStep::AtomicProjectionSelectionHostBefore { .. },
            PropertyScrollCompiledStep::AtomicProjectionSelectionDetachedContent { .. },
            PropertyScrollCompiledStep::AtomicProjectionSelectionOverlayAfter { .. },
        ]
    ) {
        return prepare_retained_atomic_projection_selection_text_area_scroll_boundary_parts(
            boundary,
            global_boundary,
            scale_factor_bits,
            graph_keys,
            declared,
        );
    }
    if matches!(
        boundary.steps.as_slice(),
        [
            PropertyScrollCompiledStep::AtomicProjectionHostBefore { .. },
            PropertyScrollCompiledStep::AtomicProjectionDetachedContent { .. },
            PropertyScrollCompiledStep::AtomicProjectionOverlayAfter { .. },
        ]
    ) {
        return prepare_retained_atomic_projection_text_area_scroll_boundary_parts(
            boundary,
            global_boundary,
            scale_factor_bits,
            graph_keys,
            declared,
        );
    }
    prepare_retained_existing_property_scroll_boundary_parts(
        boundary,
        global_boundary,
        scale_factor_bits,
        graph_keys,
        declared,
    )
}

fn prepare_retained_atomic_projection_selection_text_area_scroll_boundary_parts(
    boundary: ValidatedPropertyScrollBoundary,
    global_boundary: SceneBoundaryId,
    scale_factor_bits: u32,
    graph_keys: &FxHashSet<PersistentTextureKey>,
    declared: &mut FxHashSet<PersistentTextureKey>,
) -> Result<PreparedRetainedPropertyScrollBoundaryParts, RetainedPropertyScrollScenePrepareError> {
    if !boundary.is_canonical()
        || global_boundary.owner != boundary.planner.seal.scene_root
        || global_boundary.kind != SceneBoundaryKind::ScrollContents
        || global_boundary != boundary.planner.seal.boundary
    {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    }
    let [host, content, overlay] = boundary.steps.as_slice() else {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    };
    let (
        PropertyScrollCompiledStep::AtomicProjectionSelectionHostBefore {
            authority: host_authority,
            dependency: host_identity,
            parent_span: host_span,
        },
        PropertyScrollCompiledStep::AtomicProjectionSelectionDetachedContent {
            boundary: compiled_boundary,
            authority: content_authority,
            dependency: content_identity,
            stamp: compile_stamp,
            composite,
            clip_split,
            post_composite,
            parent_before,
            parent_after,
        },
        PropertyScrollCompiledStep::AtomicProjectionSelectionOverlayAfter {
            authority: overlay_authority,
            dependency: overlay_identity,
            parent_span: overlay_span,
        },
    ) = (host, content, overlay)
    else {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    };
    let planner = &boundary.planner.seal;
    let Some(admission) = planner
        .admission
        .atomic_projection_selection_text_area_subtree_snapshot()
    else {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    };
    let Some(host_terminal) = host_authority.host_before_opaque_order_count() else {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    };
    let Some(content_terminal) = content_authority.content_opaque_order_count() else {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    };
    let Some(overlay_count) = overlay_authority.overlay_opaque_order_count() else {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    };
    let parent_terminal = host_terminal
        .checked_add(overlay_count)
        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
    let expected_span = content_authority
        .content_artifact_span_stamp(0, 0..content_terminal)
        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
    let local_clips = content_authority
        .local_clip_snapshots()
        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
    let content_bounds_bits = bounds_bits(content_zero_bounds(planner.scroll));
    let authority_identity = host_authority.identity();
    let resident = content_authority.resident();
    if *compiled_boundary != global_boundary
        || !Arc::ptr_eq(host_authority, content_authority)
        || !Arc::ptr_eq(host_authority, overlay_authority)
        || !host_authority.same_authority(content_authority)
        || !host_authority.same_authority(overlay_authority)
        || !host_authority.matches_admission_geometry(
            bounds_bits(planner.admission.source_bounds),
            planner.scroll,
        )
        || !content_authority.matches_admission_geometry(
            bounds_bits(planner.admission.source_bounds),
            planner.scroll,
        )
        || !overlay_authority.matches_admission_geometry(
            bounds_bits(planner.admission.source_bounds),
            planner.scroll,
        )
        || host_identity != content_identity
        || host_identity != overlay_identity
        || host_identity != &authority_identity
        || host_span != &(0..host_terminal)
        || *parent_before != host_terminal
        || *parent_after != host_terminal
        || overlay_span != &(host_terminal..parent_terminal)
        || !matches!(
            post_composite,
            PropertyScrollPostCompositeSchedule::NoneForExistingGrammar
        )
        || post_composite != &planner.post_composite
        || !planner.admission.exactly_corresponds_to_with_atomic(
            planner.text_area_subtree_admission,
            planner.interactive_text_area_subtree_admission,
            planner
                .atomic_projection_text_area_subtree_admission
                .as_ref(),
            planner
                .focused_atomic_projection_text_area_subtree_admission
                .as_ref(),
            &planner.post_composite,
        )
        || !planner
            .admission
            .exactly_corresponds_to_resident_with_atomic(
                planner.interactive_resident.as_ref(),
                planner.atomic_projection_resident.as_ref(),
            )
        || planner.text_area_subtree_admission.is_some()
        || planner.interactive_text_area_subtree_admission.is_some()
        || planner
            .atomic_projection_text_area_subtree_admission
            .is_some()
        || planner.interactive_resident.is_some()
        || planner.atomic_projection_resident.is_some()
        || compile_stamp.interactive_resident.is_some()
        || compile_stamp.atomic_projection_resident.is_some()
        || host_authority.boundary_root() != planner.scene_root
        || host_authority.content_root() != planner.admission.child
        || host_authority.text_area_root() != admission.text_area_root
        || host_authority.outer_scroll() != planner.scroll
        || host_authority.outer_contents_clip() != planner.contents_clip
        || compile_stamp.content.content_root != planner.admission.child
        || compile_stamp.content.content_stable_id != planner.admission.child_stable_id
        || compile_stamp.content.source_bounds_bits != content_bounds_bits
        || compile_stamp.content.local_opaque_span != (0..content_terminal)
        || compile_stamp.content.artifact_span != expected_span
        || compile_stamp.local_opaque_terminal != content_terminal
        || compile_stamp.local_raster_clips.as_slice() != local_clips
        || local_clips != [resident.contents_clip]
        || clip_split.local_raster_clips.as_slice() != local_clips
        || clip_split.own_contents_clip != planner.contents_clip
        || !clip_split.ancestor_composite_clips.is_empty()
        || !matches!(composite.basis, ScrollCompositeBasis::FrameRoot)
        || composite.source_bounds_bits != content_bounds_bits
        || composite.offset_bits
            != [
                planner.scroll.offset.x.to_bits(),
                planner.scroll.offset.y.to_bits(),
            ]
        || composite.contents_clip != planner.contents_clip.logical_scissor
        || admission.content_wrapper != compile_stamp.content.content_root
        || admission.paint_grammar != resident.source_grammar
        || !resident.is_canonical()
    {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    }
    let PropertyScrollContentBackingCompileStamp::Single(single) = &compile_stamp.backing else {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    };
    if single.content != compile_stamp.content || single.budget != planner.budget {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    }
    let depth_key = single
        .color_key
        .depth_stencil()
        .ok_or(RetainedPropertyScrollScenePrepareError::DescriptorPair)?;
    let target = RetainedSurfaceRasterInputs {
        color: single.color_desc.clone(),
        depth: single.depth_desc.clone(),
        scale_factor_bits,
        source_bounds_bits: content_bounds_bits,
    };
    let stamp = validated_scroll_atomic_projection_selection_text_area_content_raster_stamp(
        compile_stamp.content.content_root,
        compile_stamp.content.content_stable_id,
        target,
        expected_span,
        0..content_terminal,
        resident.clone(),
    )
    .ok_or(RetainedPropertyScrollScenePrepareError::DescriptorPair)?;
    if stamp.target.color != single.color_desc
        || stamp.target.depth != single.depth_desc
        || stamp.identity.color_key != single.color_key
        || !content_authority.matches_atomic_raster_stamp(&stamp)
    {
        return Err(RetainedPropertyScrollScenePrepareError::DescriptorPair);
    }
    let geometry = PreparedScrollContentCompositeGeometry::from_validated_content_stamp(
        &stamp,
        planner.scroll,
        planner.contents_clip,
    )
    .ok_or(RetainedPropertyScrollScenePrepareError::GeometryContract)?;
    let prepared_backing = PreparedRetainedPropertyScrollBacking::Single {
        stamp: stamp.clone(),
        color_key: single.color_key,
        color_desc: single.color_desc.clone(),
        geometry,
        pair_bytes: single.pair_bytes,
    };
    let content_bounds = exact_u32_bounds_from_bits(content_bounds_bits)
        .ok_or(RetainedPropertyScrollScenePrepareError::GeometryContract)?;
    let group = retained_property_scroll_group_from_prepared(
        global_boundary,
        compile_stamp.content.content_root,
        compile_stamp.content.content_stable_id,
        content_bounds,
        &prepared_backing,
        &compile_stamp.backing,
    )
    .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
    let emission = prepare_validated_scroll_scene_atomic_projection_selection_text_area_emission(
        Arc::clone(host_authority),
        &stamp,
    )
    .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;

    if declared.contains(&single.color_key)
        || declared.contains(&depth_key)
        || graph_keys.contains(&single.color_key)
        || graph_keys.contains(&depth_key)
    {
        return Err(
            RetainedPropertyScrollScenePrepareError::PersistentKeyAlreadyDeclared(single.color_key),
        );
    }
    let prepared = PreparedRetainedPropertyScrollBoundaryParts {
        authority:
            PreparedRetainedPropertyScrollBoundaryAuthority::AtomicProjectionSelectionTextArea {
                emission,
            },
        backing: prepared_backing,
        post_composite: post_composite.clone(),
        group,
        host_local_terminal: host_terminal,
        content_local_terminal: content_terminal,
        parent_local_terminal: parent_terminal,
        pair_bytes: single.pair_bytes,
        tile_count: 1,
        backing_kind: ScrollSceneBackingKind::Single,
    };
    let color_inserted = declared.insert(single.color_key);
    let depth_inserted = declared.insert(depth_key);
    debug_assert!(color_inserted && depth_inserted);
    Ok(prepared)
}

fn prepare_retained_atomic_projection_text_area_scroll_boundary_parts(
    boundary: ValidatedPropertyScrollBoundary,
    global_boundary: SceneBoundaryId,
    scale_factor_bits: u32,
    graph_keys: &FxHashSet<PersistentTextureKey>,
    declared: &mut FxHashSet<PersistentTextureKey>,
) -> Result<PreparedRetainedPropertyScrollBoundaryParts, RetainedPropertyScrollScenePrepareError> {
    if !boundary.is_canonical()
        || global_boundary.owner != boundary.planner.seal.scene_root
        || global_boundary.kind != SceneBoundaryKind::ScrollContents
        || global_boundary != boundary.planner.seal.boundary
    {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    }
    let [host, content, overlay] = boundary.steps.as_slice() else {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    };
    let (
        PropertyScrollCompiledStep::AtomicProjectionHostBefore {
            authority: host_authority,
            dependency: host_identity,
            parent_span: host_span,
        },
        PropertyScrollCompiledStep::AtomicProjectionDetachedContent {
            boundary: compiled_boundary,
            authority: content_authority,
            dependency: content_identity,
            stamp: compile_stamp,
            composite,
            clip_split,
            post_composite,
            parent_before,
            parent_after,
        },
        PropertyScrollCompiledStep::AtomicProjectionOverlayAfter {
            authority: overlay_authority,
            dependency: overlay_identity,
            parent_span: overlay_span,
        },
    ) = (host, content, overlay)
    else {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    };
    let planner = &boundary.planner.seal;
    let atomic_admission = planner
        .admission
        .atomic_projection_text_area_subtree_snapshot();
    let focused_admission = planner
        .admission
        .focused_atomic_projection_text_area_subtree_snapshot();
    let Some(planner_resident) = planner.atomic_projection_resident.as_ref() else {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    };
    let (admission_content_wrapper, admission_text_area_root, admission_source_grammar) =
        if let Some(admission) = atomic_admission {
            (
                admission.content_wrapper,
                admission.text_area_root,
                &admission.paint_grammar,
            )
        } else if let Some(admission) = focused_admission {
            (
                admission.content_wrapper,
                admission.text_area_root,
                &admission.paint_grammar.atomic_source,
            )
        } else {
            return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
        };
    let Some(host_terminal) = host_authority.host_before_opaque_order_count() else {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    };
    let Some(content_terminal) = content_authority.content_opaque_order_count() else {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    };
    let Some(overlay_count) = overlay_authority.overlay_opaque_order_count() else {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    };
    let post_composite_delta = post_composite
        .opaque_order_delta()
        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
    let overlay_start = host_terminal
        .checked_add(post_composite_delta)
        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
    let parent_terminal = overlay_start
        .checked_add(overlay_count)
        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
    let expected_span = content_authority
        .content_artifact_span_stamp(0, 0..content_terminal)
        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
    let local_clips = content_authority
        .local_clip_snapshots()
        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
    let content_bounds_bits = bounds_bits(content_zero_bounds(planner.scroll));
    let authority_identity = host_authority.identity();
    let projection_sidecars_are_exact = match (atomic_admission, focused_admission) {
        (Some(admission), None) => {
            planner
                .atomic_projection_text_area_subtree_admission
                .as_ref()
                .is_some_and(|sidecar| admission.bitwise_eq(sidecar))
                && planner
                    .focused_atomic_projection_text_area_subtree_admission
                    .is_none()
                && matches!(
                    post_composite,
                    PropertyScrollPostCompositeSchedule::NoneForExistingGrammar
                )
        }
        (None, Some(admission)) => {
            planner
                .focused_atomic_projection_text_area_subtree_admission
                .as_ref()
                .is_some_and(|sidecar| admission.bitwise_eq(sidecar))
                && planner
                    .atomic_projection_text_area_subtree_admission
                    .is_none()
                && matches!(
                    post_composite,
                    PropertyScrollPostCompositeSchedule::FocusedAtomicProjectionSidecars(caret)
                        if caret.is_canonical()
                            && caret.caret.owner == admission.text_area_root
                            && caret.caret.stable_id == admission.text_area_stable_id
                            && caret.outer_clip == planner.contents_clip
                )
        }
        _ => false,
    };
    if *compiled_boundary != global_boundary
        || !Arc::ptr_eq(host_authority, content_authority)
        || !Arc::ptr_eq(host_authority, overlay_authority)
        || !host_authority.same_authority(content_authority)
        || !host_authority.same_authority(overlay_authority)
        || !host_authority.matches_admission_geometry(
            bounds_bits(planner.admission.source_bounds),
            planner.scroll,
        )
        || !content_authority.matches_admission_geometry(
            bounds_bits(planner.admission.source_bounds),
            planner.scroll,
        )
        || !overlay_authority.matches_admission_geometry(
            bounds_bits(planner.admission.source_bounds),
            planner.scroll,
        )
        || host_identity != content_identity
        || host_identity != overlay_identity
        || host_identity != &authority_identity
        || host_span != &(0..host_terminal)
        || *parent_before != host_terminal
        || *parent_after != overlay_start
        || overlay_span != &(overlay_start..parent_terminal)
        || post_composite != &planner.post_composite
        || !planner.admission.exactly_corresponds_to_with_atomic(
            planner.text_area_subtree_admission,
            planner.interactive_text_area_subtree_admission,
            planner
                .atomic_projection_text_area_subtree_admission
                .as_ref(),
            planner
                .focused_atomic_projection_text_area_subtree_admission
                .as_ref(),
            &planner.post_composite,
        )
        || !planner
            .admission
            .exactly_corresponds_to_resident_with_atomic(
                planner.interactive_resident.as_ref(),
                planner.atomic_projection_resident.as_ref(),
            )
        || !projection_sidecars_are_exact
        || planner.text_area_subtree_admission.is_some()
        || planner.interactive_text_area_subtree_admission.is_some()
        || planner.interactive_resident.is_some()
        || compile_stamp.interactive_resident.is_some()
        || compile_stamp.atomic_projection_resident.as_ref() != Some(planner_resident)
        || content_authority.resident() != planner_resident
        || host_authority.boundary_root() != planner.scene_root
        || host_authority.content_root() != planner.admission.child
        || host_authority.text_area_root() != admission_text_area_root
        || host_authority.outer_scroll() != planner.scroll
        || host_authority.outer_contents_clip() != planner.contents_clip
        || compile_stamp.content.content_root != planner.admission.child
        || compile_stamp.content.content_stable_id != planner.admission.child_stable_id
        || compile_stamp.content.source_bounds_bits != content_bounds_bits
        || compile_stamp.content.local_opaque_span != (0..content_terminal)
        || compile_stamp.content.artifact_span != expected_span
        || compile_stamp.local_opaque_terminal != content_terminal
        || compile_stamp.local_raster_clips.as_slice() != local_clips
        || local_clips != [planner_resident.contents_clip]
        || clip_split.local_raster_clips.as_slice() != local_clips
        || clip_split.own_contents_clip != planner.contents_clip
        || !clip_split.ancestor_composite_clips.is_empty()
        || !matches!(composite.basis, ScrollCompositeBasis::FrameRoot)
        || composite.source_bounds_bits != content_bounds_bits
        || composite.offset_bits
            != [
                planner.scroll.offset.x.to_bits(),
                planner.scroll.offset.y.to_bits(),
            ]
        || composite.contents_clip != planner.contents_clip.logical_scissor
        || admission_content_wrapper != compile_stamp.content.content_root
        || admission_source_grammar != &planner_resident.source_grammar
    {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    }
    let PropertyScrollContentBackingCompileStamp::Single(single) = &compile_stamp.backing else {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    };
    if single.content != compile_stamp.content || single.budget != planner.budget {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    }
    let depth_key = single
        .color_key
        .depth_stencil()
        .ok_or(RetainedPropertyScrollScenePrepareError::DescriptorPair)?;
    let target = RetainedSurfaceRasterInputs {
        color: single.color_desc.clone(),
        depth: single.depth_desc.clone(),
        scale_factor_bits,
        source_bounds_bits: content_bounds_bits,
    };
    let stamp = validated_scroll_atomic_projection_text_area_content_raster_stamp(
        compile_stamp.content.content_root,
        compile_stamp.content.content_stable_id,
        target,
        expected_span,
        0..content_terminal,
        planner_resident.clone(),
    )
    .ok_or(RetainedPropertyScrollScenePrepareError::DescriptorPair)?;
    if stamp.target.color != single.color_desc
        || stamp.target.depth != single.depth_desc
        || stamp.identity.color_key != single.color_key
        || !content_authority.matches_atomic_raster_stamp(&stamp)
    {
        return Err(RetainedPropertyScrollScenePrepareError::DescriptorPair);
    }
    let geometry = PreparedScrollContentCompositeGeometry::from_validated_content_stamp(
        &stamp,
        planner.scroll,
        planner.contents_clip,
    )
    .ok_or(RetainedPropertyScrollScenePrepareError::GeometryContract)?;
    let prepared_backing = PreparedRetainedPropertyScrollBacking::Single {
        stamp: stamp.clone(),
        color_key: single.color_key,
        color_desc: single.color_desc.clone(),
        geometry,
        pair_bytes: single.pair_bytes,
    };
    let content_bounds = exact_u32_bounds_from_bits(content_bounds_bits)
        .ok_or(RetainedPropertyScrollScenePrepareError::GeometryContract)?;
    let group = retained_property_scroll_group_from_prepared(
        global_boundary,
        compile_stamp.content.content_root,
        compile_stamp.content.content_stable_id,
        content_bounds,
        &prepared_backing,
        &compile_stamp.backing,
    )
    .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
    let emission = prepare_validated_scroll_scene_atomic_projection_text_area_emission(
        Arc::clone(host_authority),
        &stamp,
    )
    .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;

    // Atomic C3a is all-or-nothing: both persistent keys are checked only
    // after every other fallible seal has been built, and insertion is the
    // final operation before returning the prepared authority.
    if declared.contains(&single.color_key)
        || declared.contains(&depth_key)
        || graph_keys.contains(&single.color_key)
        || graph_keys.contains(&depth_key)
    {
        return Err(
            RetainedPropertyScrollScenePrepareError::PersistentKeyAlreadyDeclared(single.color_key),
        );
    }
    let prepared = PreparedRetainedPropertyScrollBoundaryParts {
        authority: PreparedRetainedPropertyScrollBoundaryAuthority::AtomicProjectionTextArea {
            emission,
        },
        backing: prepared_backing,
        post_composite: post_composite.clone(),
        group,
        host_local_terminal: host_terminal,
        content_local_terminal: content_terminal,
        parent_local_terminal: parent_terminal,
        pair_bytes: single.pair_bytes,
        tile_count: 1,
        backing_kind: ScrollSceneBackingKind::Single,
    };
    let color_inserted = declared.insert(single.color_key);
    let depth_inserted = declared.insert(depth_key);
    debug_assert!(color_inserted && depth_inserted);
    Ok(prepared)
}

fn prepare_retained_existing_property_scroll_boundary_parts(
    boundary: ValidatedPropertyScrollBoundary,
    global_boundary: SceneBoundaryId,
    scale_factor_bits: u32,
    graph_keys: &FxHashSet<PersistentTextureKey>,
    declared: &mut FxHashSet<PersistentTextureKey>,
) -> Result<PreparedRetainedPropertyScrollBoundaryParts, RetainedPropertyScrollScenePrepareError> {
    if !boundary.is_canonical()
        || global_boundary.owner != boundary.planner.seal.scene_root
        || global_boundary.kind != SceneBoundaryKind::ScrollContents
    {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    }
    let [host, content, overlay] = boundary.steps.as_slice() else {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    };
    let (
        PropertyScrollCompiledStep::HostBefore {
            artifact: host_artifact,
            parent_span: host_span,
            ..
        },
        PropertyScrollCompiledStep::DetachedContent {
            artifact: content_artifact,
            stamp: compile_stamp,
            post_composite,
            parent_before,
            parent_after,
            ..
        },
        PropertyScrollCompiledStep::OverlayAfter {
            artifact: overlay_artifact,
            parent_span: overlay_span,
            ..
        },
    ) = (host, content, overlay)
    else {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    };
    let host_terminal = checked_property_scroll_opaque_order_count(host_artifact)
        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
    let content_terminal = checked_property_scroll_opaque_order_count(content_artifact)
        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
    let overlay_count = checked_property_scroll_opaque_order_count(overlay_artifact)
        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
    let post_composite_delta = post_composite
        .opaque_order_delta()
        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
    let overlay_start = host_terminal
        .checked_add(post_composite_delta)
        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
    let parent_terminal = overlay_start
        .checked_add(overlay_count)
        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
    if host_span != &(0..host_terminal)
        || *parent_before != host_terminal
        || *parent_after != overlay_start
        || overlay_span != &(overlay_start..parent_terminal)
        || compile_stamp.local_opaque_terminal != content_terminal
        || compile_stamp.content.local_opaque_span != (0..content_terminal)
        || post_composite != &boundary.planner.seal.post_composite
        || compile_stamp.interactive_resident != boundary.planner.seal.interactive_resident
    {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    }
    let planner = &boundary.planner.seal;
    if planner.interactive_text_area_subtree_admission.is_some()
        && !matches!(
            compile_stamp.backing,
            PropertyScrollContentBackingCompileStamp::Single(_)
        )
    {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    }
    let host_bounds_bits = bounds_bits(planner.admission.source_bounds);
    let content_bounds_bits = bounds_bits(content_zero_bounds(planner.scroll));
    let host_before = validate_scroll_scene_host_before_artifact(
        host_artifact.clone(),
        planner.scene_root,
        host_bounds_bits,
    )
    .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
    let content_authority = validate_property_scroll_content_authority(
        planner,
        content_artifact.clone(),
        content_bounds_bits,
    )
    .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
    let overlay = validate_scroll_scene_overlay_artifact(
        overlay_artifact.clone(),
        planner.scene_root,
        planner.scroll,
        host_bounds_bits,
    )
    .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
    let expected_span =
        validated_scroll_content_artifact_span_stamp(&content_authority, 0, 0..content_terminal)
            .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
    if expected_span != compile_stamp.content.artifact_span {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    }
    let mut admit_pair = |color_key: PersistentTextureKey| {
        let Some(depth_key) = color_key.depth_stencil() else {
            return false;
        };
        declared.insert(color_key)
            && declared.insert(depth_key)
            && !graph_keys.contains(&color_key)
            && !graph_keys.contains(&depth_key)
    };
    let prepared_backing = match &compile_stamp.backing {
        PropertyScrollContentBackingCompileStamp::Single(single) => {
            if !admit_pair(single.color_key) {
                return Err(
                    RetainedPropertyScrollScenePrepareError::PersistentKeyAlreadyDeclared(
                        single.color_key,
                    ),
                );
            }
            let target = RetainedSurfaceRasterInputs {
                color: single.color_desc.clone(),
                depth: single.depth_desc.clone(),
                scale_factor_bits,
                source_bounds_bits: content_bounds_bits,
            };
            let stamp = if let Some(text_area) = planner.admission.text_area_subtree_snapshot() {
                validated_scroll_text_area_content_raster_stamp(
                    compile_stamp.content.content_root,
                    compile_stamp.content.content_stable_id,
                    target,
                    expected_span.clone(),
                    0..content_terminal,
                    text_area.paint_grammar,
                )
            } else if planner
                .admission
                .interactive_text_area_subtree_snapshot()
                .is_some()
            {
                validated_scroll_interactive_text_area_content_raster_stamp(
                    compile_stamp.content.content_root,
                    compile_stamp.content.content_stable_id,
                    target,
                    expected_span.clone(),
                    0..content_terminal,
                    compile_stamp
                        .interactive_resident
                        .clone()
                        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?,
                )
            } else {
                validated_scroll_content_raster_stamp(
                    compile_stamp.content.content_root,
                    compile_stamp.content.content_stable_id,
                    target,
                    expected_span.clone(),
                    0..content_terminal,
                )
            }
            .ok_or(RetainedPropertyScrollScenePrepareError::DescriptorPair)?;
            if stamp.target.color != single.color_desc
                || stamp.target.depth != single.depth_desc
                || stamp.identity.color_key != single.color_key
            {
                return Err(RetainedPropertyScrollScenePrepareError::DescriptorPair);
            }
            let geometry = PreparedScrollContentCompositeGeometry::from_validated_content_stamp(
                &stamp,
                planner.scroll,
                planner.contents_clip,
            )
            .ok_or(RetainedPropertyScrollScenePrepareError::GeometryContract)?;
            PreparedRetainedPropertyScrollBacking::Single {
                stamp,
                color_key: single.color_key,
                color_desc: single.color_desc.clone(),
                geometry,
                pair_bytes: single.pair_bytes,
            }
        }
        PropertyScrollContentBackingCompileStamp::Tiled(tiled) => {
            let mut tiles = Vec::with_capacity(tiled.tiles.len());
            for tile in &tiled.tiles {
                if !admit_pair(tile.color_key) {
                    return Err(
                        RetainedPropertyScrollScenePrepareError::PersistentKeyAlreadyDeclared(
                            tile.color_key,
                        ),
                    );
                }
                let raster_identity = super::ScrollContentTileRasterIdentity::new(
                    tile.index,
                    tiled.content_bounds,
                    tile.bounds,
                    tiled.tile_edge,
                    tiled.gutter,
                )
                .ok_or(RetainedPropertyScrollScenePrepareError::GeometryContract)?;
                let target = RetainedSurfaceRasterInputs {
                    color: tile.color_desc.clone(),
                    depth: tile.depth_desc.clone(),
                    scale_factor_bits,
                    source_bounds_bits: tile.bounds.raster.map(|value| (value as f32).to_bits()),
                };
                let stamp = super::validated_scroll_content_tile_raster_stamp(
                    compile_stamp.content.content_root,
                    compile_stamp.content.content_stable_id,
                    raster_identity,
                    target,
                    expected_span.clone(),
                    0..content_terminal,
                )
                .ok_or(RetainedPropertyScrollScenePrepareError::DescriptorPair)?;
                if stamp.target.color != tile.color_desc
                    || stamp.target.depth != tile.depth_desc
                    || stamp.identity.color_key != tile.color_key
                {
                    return Err(RetainedPropertyScrollScenePrepareError::DescriptorPair);
                }
                let geometry =
                    super::PreparedScrollContentTileCompositeGeometry::from_validated_tile_stamp(
                        &stamp,
                        planner.scroll,
                        planner.contents_clip,
                    )
                    .ok_or(RetainedPropertyScrollScenePrepareError::GeometryContract)?;
                tiles.push(PreparedRetainedPropertyScrollTile {
                    stamp,
                    color_key: tile.color_key,
                    color_desc: tile.color_desc.clone(),
                    geometry,
                });
            }
            PreparedRetainedPropertyScrollBacking::Tiled {
                tiles,
                total_pair_bytes: tiled.total_pair_bytes,
            }
        }
    };
    let content_bounds = exact_u32_bounds_from_bits(content_bounds_bits)
        .ok_or(RetainedPropertyScrollScenePrepareError::GeometryContract)?;
    let group = retained_property_scroll_group_from_prepared(
        global_boundary,
        compile_stamp.content.content_root,
        compile_stamp.content.content_stable_id,
        content_bounds,
        &prepared_backing,
        &compile_stamp.backing,
    )
    .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
    let (backing_kind, tile_count, pair_bytes) = match &prepared_backing {
        PreparedRetainedPropertyScrollBacking::Single { pair_bytes, .. } => {
            (ScrollSceneBackingKind::Single, 1, *pair_bytes)
        }
        PreparedRetainedPropertyScrollBacking::Tiled {
            tiles,
            total_pair_bytes,
        } => (
            ScrollSceneBackingKind::Tiled,
            tiles.len(),
            *total_pair_bytes,
        ),
    };
    Ok(PreparedRetainedPropertyScrollBoundaryParts {
        authority: PreparedRetainedPropertyScrollBoundaryAuthority::Existing {
            host_before,
            content: content_authority,
            overlay,
        },
        backing: prepared_backing,
        post_composite: post_composite.clone(),
        group,
        host_local_terminal: host_terminal,
        content_local_terminal: content_terminal,
        parent_local_terminal: parent_terminal,
        pair_bytes,
        tile_count,
        backing_kind,
    })
}

/// Converts the graph-inert B1 compiler authority into the sole B2 pre-clear
/// emission token.  No graph declaration or resident mutation occurs here.
#[cfg(test)]
fn prepare_retained_property_scroll_scene_from_pool<'a>(
    viewport: &'a mut Viewport,
    boundary: ValidatedPropertyScrollBoundary,
    graph: &'a mut FrameGraph,
    ctx: UiBuildContext,
) -> Result<PreparedRetainedPropertyScrollScene<'a>, RetainedPropertyScrollScenePrepareError> {
    if !boundary.is_canonical() {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    }
    validate_parent_target(graph, &ctx)
        .map_err(|_| RetainedPropertyScrollScenePrepareError::ParentTarget)?;
    if ctx.viewport().scale_factor().to_bits() != 1.0_f32.to_bits()
        || ctx.paint_offset().map(f32::to_bits) != [0.0_f32.to_bits(); 2]
        || ctx.graphics_pass_context().scissor_rect.is_some()
        || ctx.opaque_rect_order() != 0
        || ctx.viewport().target_format() != boundary.planner.seal.target_format
    {
        return Err(RetainedPropertyScrollScenePrepareError::ContextMismatch);
    }
    if !viewport.retained_property_scroll_scene_stage_is_available() {
        return Err(RetainedPropertyScrollScenePrepareError::StageUnavailable);
    }
    let [host, content, overlay] = boundary.steps.as_slice() else {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    };
    let (
        PropertyScrollCompiledStep::HostBefore {
            artifact: host_artifact,
            parent_span: host_span,
            ..
        },
        PropertyScrollCompiledStep::DetachedContent {
            boundary: compiled_boundary,
            artifact: content_artifact,
            stamp: compile_stamp,
            parent_before,
            parent_after,
            ..
        },
        PropertyScrollCompiledStep::OverlayAfter {
            artifact: overlay_artifact,
            parent_span: overlay_span,
            ..
        },
    ) = (host, content, overlay)
    else {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    };
    let host_terminal = checked_property_scroll_opaque_order_count(host_artifact)
        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
    let content_terminal = checked_property_scroll_opaque_order_count(content_artifact)
        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
    let overlay_count = checked_property_scroll_opaque_order_count(overlay_artifact)
        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
    let parent_terminal = host_terminal
        .checked_add(overlay_count)
        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
    if host_span != &(0..host_terminal)
        || *parent_before != host_terminal
        || *parent_after != host_terminal
        || overlay_span != &(host_terminal..parent_terminal)
        || compile_stamp.local_opaque_terminal != content_terminal
        || compile_stamp.content.local_opaque_span != (0..content_terminal)
    {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    }
    let planner = &boundary.planner.seal;
    let host_bounds_bits = bounds_bits(planner.admission.source_bounds);
    let content_bounds_bits = bounds_bits(content_zero_bounds(planner.scroll));
    let host_before = validate_scroll_scene_host_before_artifact(
        host_artifact.clone(),
        planner.scene_root,
        host_bounds_bits,
    )
    .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
    let content_authority = validate_property_scroll_content_authority(
        planner,
        content_artifact.clone(),
        content_bounds_bits,
    )
    .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
    let overlay = validate_scroll_scene_overlay_artifact(
        overlay_artifact.clone(),
        planner.scene_root,
        planner.scroll,
        host_bounds_bits,
    )
    .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
    let expected_span =
        validated_scroll_content_artifact_span_stamp(&content_authority, 0, 0..content_terminal)
            .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
    if expected_span != compile_stamp.content.artifact_span {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    }
    let mut declared = FxHashSet::default();
    let graph_keys = graph
        .declared_persistent_texture_keys()
        .collect::<FxHashSet<_>>();
    let mut admit_pair = |color_key: PersistentTextureKey| {
        let Some(depth_key) = color_key.depth_stencil() else {
            return false;
        };
        declared.insert(color_key)
            && declared.insert(depth_key)
            && !graph_keys.contains(&color_key)
            && !graph_keys.contains(&depth_key)
    };
    let prepared_backing = match &compile_stamp.backing {
        PropertyScrollContentBackingCompileStamp::Single(single) => {
            if !admit_pair(single.color_key) {
                return Err(
                    RetainedPropertyScrollScenePrepareError::PersistentKeyAlreadyDeclared(
                        single.color_key,
                    ),
                );
            }
            let target = RetainedSurfaceRasterInputs {
                color: single.color_desc.clone(),
                depth: single.depth_desc.clone(),
                scale_factor_bits: 1.0_f32.to_bits(),
                source_bounds_bits: content_bounds_bits,
            };
            let stamp = if let Some(text_area) = planner.admission.text_area_subtree_snapshot() {
                validated_scroll_text_area_content_raster_stamp(
                    compile_stamp.content.content_root,
                    compile_stamp.content.content_stable_id,
                    target,
                    expected_span.clone(),
                    0..content_terminal,
                    text_area.paint_grammar,
                )
            } else {
                validated_scroll_content_raster_stamp(
                    compile_stamp.content.content_root,
                    compile_stamp.content.content_stable_id,
                    target,
                    expected_span.clone(),
                    0..content_terminal,
                )
            }
            .ok_or(RetainedPropertyScrollScenePrepareError::DescriptorPair)?;
            if stamp.target.color != single.color_desc
                || stamp.target.depth != single.depth_desc
                || stamp.identity.color_key != single.color_key
            {
                return Err(RetainedPropertyScrollScenePrepareError::DescriptorPair);
            }
            let geometry = PreparedScrollContentCompositeGeometry::from_validated_content_stamp(
                &stamp,
                planner.scroll,
                planner.contents_clip,
            )
            .ok_or(RetainedPropertyScrollScenePrepareError::GeometryContract)?;
            PreparedRetainedPropertyScrollBacking::Single {
                stamp,
                color_key: single.color_key,
                color_desc: single.color_desc.clone(),
                geometry,
                pair_bytes: single.pair_bytes,
            }
        }
        PropertyScrollContentBackingCompileStamp::Tiled(tiled) => {
            let mut tiles = Vec::with_capacity(tiled.tiles.len());
            for tile in &tiled.tiles {
                if !admit_pair(tile.color_key) {
                    return Err(
                        RetainedPropertyScrollScenePrepareError::PersistentKeyAlreadyDeclared(
                            tile.color_key,
                        ),
                    );
                }
                let raster_identity = super::ScrollContentTileRasterIdentity::new(
                    tile.index,
                    tiled.content_bounds,
                    tile.bounds,
                    tiled.tile_edge,
                    tiled.gutter,
                )
                .ok_or(RetainedPropertyScrollScenePrepareError::GeometryContract)?;
                let target = RetainedSurfaceRasterInputs {
                    color: tile.color_desc.clone(),
                    depth: tile.depth_desc.clone(),
                    scale_factor_bits: 1.0_f32.to_bits(),
                    source_bounds_bits: tile.bounds.raster.map(|value| (value as f32).to_bits()),
                };
                let stamp = super::validated_scroll_content_tile_raster_stamp(
                    compile_stamp.content.content_root,
                    compile_stamp.content.content_stable_id,
                    raster_identity,
                    target,
                    expected_span.clone(),
                    0..content_terminal,
                )
                .ok_or(RetainedPropertyScrollScenePrepareError::DescriptorPair)?;
                if stamp.target.color != tile.color_desc
                    || stamp.target.depth != tile.depth_desc
                    || stamp.identity.color_key != tile.color_key
                {
                    return Err(RetainedPropertyScrollScenePrepareError::DescriptorPair);
                }
                let geometry =
                    super::PreparedScrollContentTileCompositeGeometry::from_validated_tile_stamp(
                        &stamp,
                        planner.scroll,
                        planner.contents_clip,
                    )
                    .ok_or(RetainedPropertyScrollScenePrepareError::GeometryContract)?;
                tiles.push(PreparedRetainedPropertyScrollTile {
                    stamp,
                    color_key: tile.color_key,
                    color_desc: tile.color_desc.clone(),
                    geometry,
                });
            }
            PreparedRetainedPropertyScrollBacking::Tiled {
                tiles,
                total_pair_bytes: tiled.total_pair_bytes,
            }
        }
    };
    let content_bounds = exact_u32_bounds_from_bits(content_bounds_bits)
        .ok_or(RetainedPropertyScrollScenePrepareError::GeometryContract)?;
    let group = retained_property_scroll_group_from_prepared(
        *compiled_boundary,
        compile_stamp.content.content_root,
        compile_stamp.content.content_stable_id,
        content_bounds,
        &prepared_backing,
        &compile_stamp.backing,
    )
    .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
    let joint = &planner.joint_transaction;
    if !joint.generic_full_set.is_empty()
        || joint.scroll_groups.len() != 1
        || joint.scroll_groups[0].boundary != *compiled_boundary
    {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    }
    let group_binding = RetainedPropertyScrollGroupBindingStamp {
        boundary: *compiled_boundary,
        content_root: group.content_root,
        content_stable_id: group.content_stable_id,
        backing_rank: group.backing_rank(),
        ordered_resident_keys: group.active_resident_keys(),
    };
    let transaction = RetainedPropertyScrollSceneTransaction {
        seal: RetainedPropertyScrollJointSeal {
            roots: joint
                .roots
                .iter()
                .map(|root| RetainedPropertyScrollJointRootStamp {
                    ordinal: root.ordinal,
                    root: root.root,
                    stable_id: root.stable_id,
                    boundary_span: root.boundary_span.clone(),
                })
                .collect(),
            ordered_boundaries: joint.ordered_boundaries.clone(),
            generic_bindings: Vec::new(),
            scroll_bindings: vec![group_binding],
        },
        generic_authority: RetainedPropertyScrollGenericAuthority::Empty,
        generic_full_set: Vec::new(),
        scroll_groups: vec![group],
    };
    if !transaction.is_canonical() {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    }
    let actions = viewport
        .freeze_retained_property_scroll_scene_compile_actions_from_pool(&transaction)
        .ok_or(RetainedPropertyScrollScenePrepareError::PoolContract)?;
    let expected_keys = transaction
        .ordered_stamps()
        .into_iter()
        .map(|stamp| stamp.identity.resident_key())
        .collect::<FxHashSet<_>>();
    if actions.len() != expected_keys.len()
        || actions.keys().copied().collect::<FxHashSet<_>>() != expected_keys
    {
        return Err(RetainedPropertyScrollScenePrepareError::PoolContract);
    }
    let (backing, tile_count, pair_bytes) = match &prepared_backing {
        PreparedRetainedPropertyScrollBacking::Single { pair_bytes, .. } => {
            (ScrollSceneBackingKind::Single, 1, *pair_bytes)
        }
        PreparedRetainedPropertyScrollBacking::Tiled {
            tiles,
            total_pair_bytes,
        } => (
            ScrollSceneBackingKind::Tiled,
            tiles.len(),
            *total_pair_bytes,
        ),
    };
    let reraster_count = actions
        .values()
        .filter(|action| **action == RetainedSurfaceCompileAction::Reraster)
        .count();
    let reuse_count = actions.len() - reraster_count;
    let trace = RetainedPropertyScrollSceneBuildTrace {
        root_count: transaction.seal.roots.len(),
        generic_surface_count: transaction.generic_full_set.len(),
        effect_surface_count: transaction
            .generic_full_set
            .iter()
            .filter(|stamp| stamp.identity.role == RetainedSurfaceRasterRole::PropertyEffect)
            .count(),
        scroll_group_count: transaction.scroll_groups.len(),
        backing,
        tile_count,
        reraster_count,
        reuse_count,
        content_pair_bytes: pair_bytes,
    };
    Ok(PreparedRetainedPropertyScrollScene {
        viewport,
        graph,
        parent_ctx: ctx,
        host_before,
        content: content_authority,
        overlay,
        backing: prepared_backing,
        actions,
        transaction,
        host_parent_terminal: host_terminal,
        content_local_terminal: content_terminal,
        parent_terminal,
        trace,
    })
}

/// Closes the B4-1 forest in one read-only pool transaction. Failure at any
/// boundary returns before graph declaration or resident staging.
pub(crate) fn prepare_native_scroll_forest_transaction_from_pool(
    viewport: &Viewport,
    plan: &super::FramePaintPlan,
    target_format: wgpu::TextureFormat,
) -> Result<PreparedNativeScrollForestTransaction, RetainedPropertyScrollScenePrepareError> {
    prepare_native_scroll_forest_transaction_with_pool_policy(viewport, plan, target_format, false)
}

#[cfg(test)]
pub(crate) fn prepare_native_scroll_forest_transaction_with_forced_pool_for_test(
    viewport: &Viewport,
    plan: &super::FramePaintPlan,
    target_format: wgpu::TextureFormat,
) -> Result<PreparedNativeScrollForestTransaction, RetainedPropertyScrollScenePrepareError> {
    prepare_native_scroll_forest_transaction_with_pool_policy(viewport, plan, target_format, true)
}

fn prepare_native_scroll_forest_transaction_with_pool_policy(
    viewport: &Viewport,
    plan: &super::FramePaintPlan,
    target_format: wgpu::TextureFormat,
    allow_forced_pair_witness: bool,
) -> Result<PreparedNativeScrollForestTransaction, RetainedPropertyScrollScenePrepareError> {
    let scaffold = plan
        .native_scroll_forest_planning_scaffold()
        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
    let scale_factor = f32::from_bits(scaffold.scale_factor_bits);
    if !scale_factor.is_finite() || scale_factor <= 0.0 {
        return Err(RetainedPropertyScrollScenePrepareError::ContextMismatch);
    }
    let mut groups = Vec::with_capacity(scaffold.boundaries.len());
    let mut stamps = Vec::with_capacity(scaffold.boundaries.len());
    let mut geometries = Vec::with_capacity(scaffold.boundaries.len());
    let mut keys = FxHashSet::default();
    let programs = (0..scaffold.boundaries.len())
        .map(|index| {
            super::compiler::validate_native_scroll_forest_boundary_program_for_emission(
                scaffold, index,
            )
        })
        .collect::<Option<Vec<_>>>()
        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
    if programs
        .iter()
        .enumerate()
        .any(|(index, program)| program.boundary().0 as usize != index)
    {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    }
    let root_boundaries = scaffold
        .boundaries
        .iter()
        .filter(|boundary| boundary.parent.is_none())
        .map(|boundary| boundary.id)
        .collect::<Vec<_>>();
    if root_boundaries.is_empty() {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    }
    for (index, (boundary, program)) in scaffold
        .boundaries
        .iter()
        .zip(&scaffold.programs)
        .enumerate()
    {
        if boundary.id.0 as usize != index || program.boundary != boundary.id {
            return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
        }
        let source = content_zero_bounds(boundary.scroll);
        let source_bounds = bounds_bits(source);
        let content_bounds = exact_u32_bounds_from_bits(source_bounds)
            .ok_or(RetainedPropertyScrollScenePrepareError::GeometryContract)?;
        let color_key = scroll_content_layer_stable_key(program.receiver_stable_id);
        if !keys.insert(color_key)
            || !keys.insert(
                color_key
                    .depth_stencil()
                    .ok_or(RetainedPropertyScrollScenePrepareError::DescriptorPair)?,
            )
        {
            return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
        }
        let color = texture_desc_for_logical_bounds(source, scale_factor, None, target_format);
        let (color, depth) = persistent_target_texture_descriptors(color, color_key);
        let stamp = super::compiler::validated_native_scroll_forest_content_raster_stamp(
            boundary.admission.content_root,
            program.receiver_stable_id,
            RetainedSurfaceRasterInputs {
                color,
                depth,
                scale_factor_bits: scaffold.scale_factor_bits,
                source_bounds_bits: source_bounds,
            },
            program.compiler_stamp.content_artifact_span.clone(),
            program.child_dependencies.clone(),
            0..program.content_program_opaque_terminal,
        )
        .ok_or(RetainedPropertyScrollScenePrepareError::DescriptorPair)?;
        let geometry =
            PreparedScrollContentCompositeGeometry::from_validated_native_scroll_forest_content_stamp(
                &stamp,
                boundary.scroll,
                boundary.contents_clip,
            )
            .ok_or(RetainedPropertyScrollScenePrepareError::GeometryContract)?;
        if geometry.source_key() != color_key
            || geometry.source_bounds_bits() != stamp.target.source_bounds_bits
        {
            return Err(RetainedPropertyScrollScenePrepareError::GeometryContract);
        }
        let group = RetainedPropertyScrollResidentGroup {
            boundary: SceneBoundaryId {
                ordinal: boundary.id.0,
                owner: boundary.boundary_root,
                kind: SceneBoundaryKind::ScrollContents,
            },
            content_root: boundary.admission.content_root,
            content_stable_id: program.receiver_stable_id,
            signature: RetainedPropertyScrollGroupSignature {
                content_bounds,
                tile_edge: SCROLL_CONTENT_TILE_EDGE,
                gutter: SCROLL_CONTENT_TILE_GUTTER,
                overscan: 0,
                scale_factor_bits: scaffold.scale_factor_bits,
                color_format: target_format,
            },
            backing: RetainedPropertyScrollResidentBacking::Single(stamp.clone()),
        };
        if !group.is_native_scroll_forest_canonical() {
            return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
        }
        stamps.push(stamp);
        geometries.push(geometry);
        groups.push(group);
    }
    let ordered_boundaries = groups
        .iter()
        .map(|group| group.boundary)
        .collect::<Vec<_>>();
    let scroll_bindings = groups
        .iter()
        .map(|group| RetainedPropertyScrollGroupBindingStamp {
            boundary: group.boundary,
            content_root: group.content_root,
            content_stable_id: group.content_stable_id,
            backing_rank: group.backing_rank(),
            ordered_resident_keys: group.active_resident_keys(),
        })
        .collect();
    let transaction = RetainedPropertyScrollSceneTransaction {
        seal: RetainedPropertyScrollJointSeal {
            roots: scaffold
                .roots
                .iter()
                .map(|root| RetainedPropertyScrollJointRootStamp {
                    ordinal: root.ordinal,
                    root: root.root,
                    stable_id: root.stable_id,
                    boundary_span: root.boundary_span.clone(),
                })
                .collect(),
            ordered_boundaries,
            generic_bindings: Vec::new(),
            scroll_bindings,
        },
        generic_authority: RetainedPropertyScrollGenericAuthority::NativeScrollForest,
        generic_full_set: Vec::new(),
        scroll_groups: groups,
    };
    if !transaction.is_canonical() {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    }
    #[cfg(test)]
    let mut actions = if allow_forced_pair_witness {
        viewport
            .freeze_retained_property_scroll_scene_compile_actions_for_forced_test(&transaction)
            .ok_or(RetainedPropertyScrollScenePrepareError::PoolContract)?
    } else {
        viewport
            .freeze_retained_property_scroll_scene_compile_actions_from_pool(&transaction)
            .ok_or(RetainedPropertyScrollScenePrepareError::PoolContract)?
    };
    #[cfg(not(test))]
    let mut actions = {
        debug_assert!(!allow_forced_pair_witness);
        viewport
            .freeze_retained_property_scroll_scene_compile_actions_from_pool(&transaction)
            .ok_or(RetainedPropertyScrollScenePrepareError::PoolContract)?
    };
    let expected = stamps
        .iter()
        .map(|stamp| stamp.identity.resident_key())
        .collect::<FxHashSet<_>>();
    if actions.keys().copied().collect::<FxHashSet<_>>() != expected {
        return Err(RetainedPropertyScrollScenePrepareError::PoolContract);
    }
    for boundary in scaffold.boundaries.iter().rev() {
        let child_key = stamps[boundary.id.0 as usize].identity.resident_key();
        if actions.get(&child_key) == Some(&RetainedSurfaceCompileAction::Reraster) {
            if let Some(parent) = boundary.parent {
                let parent_key = stamps[parent.0 as usize].identity.resident_key();
                *actions
                    .get_mut(&parent_key)
                    .ok_or(RetainedPropertyScrollScenePrepareError::PoolContract)? =
                    RetainedSurfaceCompileAction::Reraster;
            }
        }
    }
    Ok(PreparedNativeScrollForestTransaction {
        transaction,
        stamps,
        actions,
        programs,
        geometries,
        root_boundaries,
    })
}

fn consume_reused_native_scroll_forest_descendants(
    boundary: super::frame_plan::NativeScrollBoundaryId,
    programs: &[super::compiler::ValidatedNativeScrollForestBoundaryProgram],
    stamps: &[RetainedSurfaceRasterStamp],
    actions: &mut FxHashMap<super::RetainedSurfaceResidentKey, RetainedSurfaceCompileAction>,
) {
    for step in programs[boundary.0 as usize].content_step_kinds() {
        let super::compiler::ValidatedNativeScrollForestContentStepKind::ChildBoundary(child) =
            step
        else {
            continue;
        };
        let key = stamps[child.0 as usize].identity.resident_key();
        assert_eq!(
            actions
                .remove(&key)
                .expect("prepared reused native child action is frozen"),
            RetainedSurfaceCompileAction::Reuse,
            "a reused native parent C must consume only reused descendant actions"
        );
        consume_reused_native_scroll_forest_descendants(child, programs, stamps, actions);
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_prepared_native_scroll_forest_boundary(
    boundary: super::frame_plan::NativeScrollBoundaryId,
    programs: &[super::compiler::ValidatedNativeScrollForestBoundaryProgram],
    stamps: &[RetainedSurfaceRasterStamp],
    geometries: &[PreparedScrollContentCompositeGeometry],
    actions: &mut FxHashMap<super::RetainedSurfaceResidentKey, RetainedSurfaceCompileAction>,
    graph: &mut FrameGraph,
    parent_ctx: &mut UiBuildContext,
    parent_target: RenderTargetOut,
    parent_masks: &mut super::compiler::NativeScrollForestEmissionMaskStack,
) {
    let index = boundary.0 as usize;
    let program = &programs[index];
    let stamp = &stamps[index];
    let geometry = geometries[index];
    assert_eq!(program.boundary(), boundary);
    assert_eq!(geometry.source_key(), stamp.identity.color_key);
    assert_eq!(
        geometry.source_bounds_bits(),
        stamp.target.source_bounds_bits
    );

    parent_ctx.set_current_target(parent_target);
    program.emit_host_before(graph, parent_ctx, parent_masks);

    let action = actions
        .remove(&stamp.identity.resident_key())
        .expect("prepared native boundary action is frozen");
    let mut content_ctx = UiBuildContext::from_parts(
        parent_ctx.viewport(),
        parent_ctx.layer_subtree_state_with_ancestor_clip(AncestorClipContext::default()),
    );
    content_ctx.set_current_render_transform(None);
    let content_target = content_ctx.allocate_persistent_target_with_desc(
        graph,
        stamp.target.color.clone(),
        stamp.identity.color_key,
    );
    content_ctx.set_current_target(content_target);
    match action {
        RetainedSurfaceCompileAction::Reraster => {
            graph.add_graphics_pass(ClearPass::new(
                crate::view::render_pass::clear_pass::ClearParams::new([0.0; 4]),
                crate::view::render_pass::clear_pass::ClearInput {
                    pass_context: content_ctx.graphics_pass_context(),
                    clear_depth_stencil: true,
                },
                crate::view::render_pass::clear_pass::ClearOutput {
                    render_target: content_target,
                },
            ));
            let mut content_masks = super::compiler::NativeScrollForestEmissionMaskStack::new();
            program.emit_content_steps(
                graph,
                &mut content_ctx,
                &mut content_masks,
                |child, graph, content_ctx, content_masks| {
                    let active_content_target = content_ctx
                        .current_target()
                        .expect("native parent C remains the active child host target");
                    emit_prepared_native_scroll_forest_boundary(
                        child,
                        programs,
                        stamps,
                        geometries,
                        actions,
                        graph,
                        content_ctx,
                        active_content_target,
                        content_masks,
                    );
                },
            );
            assert!(
                content_masks.is_empty(),
                "native C raster closes every child boundary mask"
            );
        }
        RetainedSurfaceCompileAction::Reuse => {
            content_ctx
                .replay_opaque_rect_order_exact(0, program.content_program_opaque_terminal());
            consume_reused_native_scroll_forest_descendants(boundary, programs, stamps, actions);
        }
    }
    assert_eq!(
        content_ctx.opaque_rect_order(),
        program.content_program_opaque_terminal(),
        "native C reaches its frozen target-local opaque terminal"
    );
    parent_ctx.merge_child_target_pairs(&content_ctx.into_state());
    parent_ctx.set_current_target(parent_target);
    graph.add_graphics_pass(
        geometry.into_texture_composite_pass(
            TextureCompositeInput::from_render_target(
                TextureCompositeSourceIn::with_handle(
                    content_target
                        .handle()
                        .expect("prepared native persistent C target has a handle"),
                ),
                Default::default(),
                parent_ctx.graphics_pass_context(),
            ),
            TextureCompositeOutput {
                render_target: parent_target,
            },
        ),
    );
    program.emit_overlay_after(graph, parent_ctx, parent_masks);
}

/// Infallible consume for a graph-inert native forest preparation. Every
/// boundary owns one persistent C target; a reused ancestor consumes the
/// complete cached subtree action set, while a rerasterized ancestor emits
/// child H/O into its C and composites each child C under the active mask.
pub(crate) fn emit_prepared_native_scroll_forest_transaction(
    viewport: &mut Viewport,
    graph: &mut FrameGraph,
    mut parent_ctx: UiBuildContext,
    prepared: PreparedNativeScrollForestTransaction,
) -> BuildState {
    let PreparedNativeScrollForestTransaction {
        transaction,
        stamps,
        mut actions,
        programs,
        geometries,
        root_boundaries,
    } = prepared;
    assert!(viewport.retained_property_scroll_scene_stage_is_available());
    assert_eq!(stamps.len(), programs.len());
    assert_eq!(stamps.len(), geometries.len());
    let parent_target = parent_ctx
        .current_target()
        .unwrap_or_else(|| parent_ctx.allocate_target(graph));
    parent_ctx.set_current_target(parent_target);
    for root in root_boundaries {
        let mut masks = super::compiler::NativeScrollForestEmissionMaskStack::new();
        emit_prepared_native_scroll_forest_boundary(
            root,
            &programs,
            &stamps,
            &geometries,
            &mut actions,
            graph,
            &mut parent_ctx,
            parent_target,
            &mut masks,
        );
        assert!(
            masks.is_empty(),
            "each native scene root closes its independent mask stack"
        );
    }
    assert!(
        actions.is_empty(),
        "native forest consumes every frozen action exactly once"
    );
    assert!(
        viewport.stage_retained_property_scroll_scene(transaction),
        "native forest stages its joint transaction exactly once"
    );
    parent_ctx.into_state()
}

pub(crate) fn prepare_frame_root_scroll_scene<'a>(
    viewport: &'a mut Viewport,
    scene: ValidatedFrameRootScrollScene,
    graph: &'a mut FrameGraph,
    ctx: UiBuildContext,
    clear_rgba: [f32; 4],
    frame_owner: RetainedSurfaceFrameStageOwner,
) -> Result<PreparedFrameRootScrollScene<'a>, RetainedPropertyScrollScenePrepareError> {
    if !scene.is_canonical() {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    }
    validate_parent_target(graph, &ctx)
        .map_err(|_| RetainedPropertyScrollScenePrepareError::ParentTarget)?;
    let scale_factor = f32::from_bits(scene.scale_factor_bits);
    if ctx.viewport().scale_factor().to_bits() != scene.scale_factor_bits
        || ctx.paint_offset().map(f32::to_bits) != [0.0_f32.to_bits(); 2]
        || ctx.graphics_pass_context().scissor_rect.is_some()
        || ctx.opaque_rect_order() != 0
        || ctx.viewport().target_format() != scene.target_format
        || clear_rgba.iter().any(|component| !component.is_finite())
    {
        return Err(RetainedPropertyScrollScenePrepareError::ContextMismatch);
    }
    if !viewport.retained_property_scroll_scene_stage_is_available()
        || !viewport.retained_surface_frame_stage_owner_is_active(frame_owner)
    {
        return Err(RetainedPropertyScrollScenePrepareError::StageUnavailable);
    }
    let root_count = scene.roots.len();
    let graph_keys = graph
        .declared_persistent_texture_keys()
        .collect::<FxHashSet<_>>();
    let mut declared = FxHashSet::default();
    let mut prepared_roots = Vec::with_capacity(root_count);
    let mut joint_roots = Vec::with_capacity(root_count);
    let mut ordered_boundaries = Vec::with_capacity(root_count);
    let mut scroll_groups = Vec::with_capacity(root_count);
    let mut pair_bytes = 0_u64;
    let mut boundary_cursor = 0_u32;
    for root in scene.roots {
        let root = match root {
            ValidatedFrameRootSceneRoot::Plain(root) => {
                joint_roots.push(RetainedPropertyScrollJointRootStamp {
                    ordinal: root.scene_root_ordinal,
                    root: root.receiver_root,
                    stable_id: root.receiver_stable_id,
                    boundary_span: boundary_cursor..boundary_cursor,
                });
                prepared_roots.push(PreparedFrameRootSceneRoot::Plain {
                    receiver_compiler: root.receiver_compiler,
                });
                continue;
            }
            ValidatedFrameRootSceneRoot::Scroll(root) => root,
        };
        let boundary = SceneBoundaryId {
            ordinal: root.boundary.ordinal,
            owner: root.boundary.scroll.owner,
            kind: SceneBoundaryKind::ScrollContents,
        };
        if boundary.ordinal != boundary_cursor {
            return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
        }
        let content_terminal =
            super::compiler::frame_root_scroll_content_opaque_order_count(&root.content_compiler)
                .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
        let span = super::compiler::validated_property_scroll_content_artifact_span_stamp(
            &root.content_compiler,
            0,
            0..content_terminal,
        )
        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
        let content_bounds = content_zero_bounds(root.boundary.scroll);
        let content_bounds_u32 = exact_dpr1_u32_bounds(content_bounds)
            .ok_or(RetainedPropertyScrollScenePrepareError::GeometryContract)?;
        let color_key = scroll_content_layer_stable_key(root.content_stable_id);
        let depth_key = color_key
            .depth_stencil()
            .ok_or(RetainedPropertyScrollScenePrepareError::DescriptorPair)?;
        if graph_keys.contains(&color_key)
            || graph_keys.contains(&depth_key)
            || !declared.insert(color_key)
            || !declared.insert(depth_key)
        {
            return Err(
                RetainedPropertyScrollScenePrepareError::PersistentKeyAlreadyDeclared(color_key),
            );
        }
        let color = texture_desc_for_logical_bounds(
            content_bounds,
            scale_factor,
            None,
            scene.target_format,
        );
        let (color_desc, depth_desc) = persistent_target_texture_descriptors(color, color_key);
        let root_pair_bytes = canonical_pair_bytes(&color_desc, &depth_desc)
            .ok_or(RetainedPropertyScrollScenePrepareError::DescriptorPair)?;
        pair_bytes = pair_bytes
            .checked_add(root_pair_bytes)
            .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
        let raster_inputs = RetainedSurfaceRasterInputs {
            color: color_desc.clone(),
            depth: depth_desc,
            scale_factor_bits: scene.scale_factor_bits,
            source_bounds_bits: bounds_bits(content_bounds),
        };
        let stamp = match root.text_area_witness {
            Some(witness) => validated_scroll_text_area_content_raster_stamp(
                root.content_root,
                root.content_stable_id,
                raster_inputs,
                span,
                0..content_terminal,
                witness.paint_grammar(),
            ),
            None => validated_scroll_content_raster_stamp(
                root.content_root,
                root.content_stable_id,
                raster_inputs,
                span,
                0..content_terminal,
            ),
        }
        .ok_or(RetainedPropertyScrollScenePrepareError::DescriptorPair)?;
        let geometry = PreparedScrollContentCompositeGeometry::from_validated_content_stamp(
            &stamp,
            root.boundary.scroll,
            root.boundary.contents_clip,
        )
        .ok_or(RetainedPropertyScrollScenePrepareError::GeometryContract)?;
        let backing = PreparedRetainedPropertyScrollBacking::Single {
            stamp: stamp.clone(),
            color_key,
            color_desc: color_desc.clone(),
            geometry,
            pair_bytes: root_pair_bytes,
        };
        let group = RetainedPropertyScrollResidentGroup {
            boundary,
            content_root: root.content_root,
            content_stable_id: root.content_stable_id,
            signature: RetainedPropertyScrollGroupSignature {
                content_bounds: content_bounds_u32,
                tile_edge: SCROLL_CONTENT_TILE_EDGE,
                gutter: SCROLL_CONTENT_TILE_GUTTER,
                overscan: 0,
                scale_factor_bits: scene.scale_factor_bits,
                color_format: scene.target_format,
            },
            backing: RetainedPropertyScrollResidentBacking::Single(stamp),
        };
        if !group.is_canonical() {
            return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
        }
        joint_roots.push(RetainedPropertyScrollJointRootStamp {
            ordinal: root.scene_root_ordinal,
            root: root.receiver_root,
            stable_id: root.receiver_stable_id,
            boundary_span: boundary_cursor..boundary_cursor.saturating_add(1),
        });
        boundary_cursor = boundary_cursor.saturating_add(1);
        ordered_boundaries.push(boundary);
        scroll_groups.push(group);
        prepared_roots.push(PreparedFrameRootSceneRoot::Scroll(
            PreparedFrameRootScrollRoot {
                receiver_compiler: root.receiver_compiler,
                scroll_cutout: root.insertion.scroll_cutout,
                content_compiler: root.content_compiler,
                backing,
                content_local_terminal: content_terminal,
            },
        ));
    }
    let scroll_bindings = scroll_groups
        .iter()
        .map(|group| RetainedPropertyScrollGroupBindingStamp {
            boundary: group.boundary,
            content_root: group.content_root,
            content_stable_id: group.content_stable_id,
            backing_rank: group.backing_rank(),
            ordered_resident_keys: group.active_resident_keys(),
        })
        .collect();
    let transaction = RetainedPropertyScrollSceneTransaction {
        seal: RetainedPropertyScrollJointSeal {
            roots: joint_roots,
            ordered_boundaries,
            generic_bindings: Vec::new(),
            scroll_bindings,
        },
        generic_authority: RetainedPropertyScrollGenericAuthority::FrameRootReceiver,
        generic_full_set: Vec::new(),
        scroll_groups,
    };
    if !transaction.is_canonical() {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    }
    let actions = viewport
        .freeze_retained_property_scroll_scene_compile_actions_from_pool(&transaction)
        .ok_or(RetainedPropertyScrollScenePrepareError::PoolContract)?;
    let expected_keys = transaction
        .ordered_stamps()
        .into_iter()
        .map(|stamp| stamp.identity.resident_key())
        .collect::<FxHashSet<_>>();
    if actions.keys().copied().collect::<FxHashSet<_>>() != expected_keys {
        return Err(RetainedPropertyScrollScenePrepareError::PoolContract);
    }
    let reraster_count = actions
        .values()
        .filter(|action| **action == RetainedSurfaceCompileAction::Reraster)
        .count();
    let retained_content_count = actions.len();
    Ok(PreparedFrameRootScrollScene {
        viewport,
        graph,
        parent_ctx: ctx,
        roots: prepared_roots,
        actions,
        transaction,
        clear_rgba_bits: clear_rgba.map(f32::to_bits),
        frame_owner,
        trace: RetainedPropertyScrollSceneBuildTrace {
            root_count,
            generic_surface_count: 0,
            effect_surface_count: 0,
            scroll_group_count: retained_content_count,
            backing: ScrollSceneBackingKind::Single,
            tile_count: retained_content_count,
            reraster_count,
            reuse_count: retained_content_count - reraster_count,
            content_pair_bytes: pair_bytes,
        },
    })
}

pub(crate) fn prepare_retained_property_scroll_forest_from_pool<'a>(
    viewport: &'a mut Viewport,
    scene: ValidatedPropertyScrollScene,
    graph: &'a mut FrameGraph,
    ctx: UiBuildContext,
    clear_rgba: [f32; 4],
    frame_owner: RetainedSurfaceFrameStageOwner,
) -> Result<PreparedRetainedPropertyScrollForest<'a>, RetainedPropertyScrollScenePrepareError> {
    if !scene.is_canonical() {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    }
    validate_parent_target(graph, &ctx)
        .map_err(|_| RetainedPropertyScrollScenePrepareError::ParentTarget)?;
    if ctx.viewport().scale_factor().to_bits() != 1.0_f32.to_bits()
        || ctx.paint_offset().map(f32::to_bits) != [0.0_f32.to_bits(); 2]
        || ctx.graphics_pass_context().scissor_rect.is_some()
        || ctx.opaque_rect_order() != 0
        || ctx.viewport().target_format() != scene.seal.target_format
        || clear_rgba.iter().any(|component| !component.is_finite())
    {
        return Err(RetainedPropertyScrollScenePrepareError::ContextMismatch);
    }
    if !viewport.retained_property_scroll_scene_stage_is_available()
        || !viewport.retained_surface_frame_stage_owner_is_active(frame_owner)
    {
        return Err(RetainedPropertyScrollScenePrepareError::StageUnavailable);
    }
    let ValidatedPropertyScrollScene { boundaries, seal } = scene;
    let graph_keys = graph
        .declared_persistent_texture_keys()
        .collect::<FxHashSet<_>>();
    let mut declared = FxHashSet::default();
    let mut prepared_boundaries = Vec::with_capacity(boundaries.len());
    let mut groups = Vec::with_capacity(boundaries.len());
    let mut parent_terminal = 0_u32;
    let mut pair_bytes = 0_u64;
    let mut tile_count = 0_usize;
    let mut has_tiled = false;
    for (boundary, global_boundary) in boundaries
        .into_iter()
        .zip(seal.ordered_boundaries.iter().copied())
    {
        let scale_factor_bits = boundary.planner.seal.scale_factor_bits;
        let prepared = prepare_retained_property_scroll_boundary_parts(
            boundary,
            global_boundary,
            scale_factor_bits,
            &graph_keys,
            &mut declared,
        )?;
        parent_terminal = parent_terminal
            .checked_add(prepared.parent_local_terminal)
            .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
        pair_bytes = pair_bytes
            .checked_add(prepared.pair_bytes)
            .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
        tile_count = tile_count
            .checked_add(prepared.tile_count)
            .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
        has_tiled |= prepared.backing_kind == ScrollSceneBackingKind::Tiled;
        groups.push(prepared.group.clone());
        prepared_boundaries.push(prepared);
    }
    if prepared_boundaries.len() != seal.roots.len()
        || prepared_boundaries.len() != seal.ordered_boundaries.len()
        || pair_bytes != seal.aggregate_pair_bytes
        || pair_bytes > seal.budget.max_active_pair_bytes
        || seal.schedule.last().map(|step| step.parent_span.end) != Some(parent_terminal)
    {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    }
    let scroll_bindings = groups
        .iter()
        .map(|group| RetainedPropertyScrollGroupBindingStamp {
            boundary: group.boundary,
            content_root: group.content_root,
            content_stable_id: group.content_stable_id,
            backing_rank: group.backing_rank(),
            ordered_resident_keys: group.active_resident_keys(),
        })
        .collect();
    let transaction = RetainedPropertyScrollSceneTransaction {
        seal: RetainedPropertyScrollJointSeal {
            roots: seal
                .roots
                .iter()
                .map(|root| RetainedPropertyScrollJointRootStamp {
                    ordinal: root.ordinal,
                    root: root.root,
                    stable_id: root.stable_id,
                    boundary_span: root.boundary_span.clone(),
                })
                .collect(),
            ordered_boundaries: seal.ordered_boundaries.clone(),
            generic_bindings: Vec::new(),
            scroll_bindings,
        },
        generic_authority: RetainedPropertyScrollGenericAuthority::Empty,
        generic_full_set: Vec::new(),
        scroll_groups: groups,
    };
    if !transaction.is_canonical() {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    }
    let actions = viewport
        .freeze_retained_property_scroll_scene_compile_actions_from_pool(&transaction)
        .ok_or(RetainedPropertyScrollScenePrepareError::PoolContract)?;
    let expected_keys = transaction
        .ordered_stamps()
        .into_iter()
        .map(|stamp| stamp.identity.resident_key())
        .collect::<FxHashSet<_>>();
    if actions.len() != expected_keys.len()
        || actions.keys().copied().collect::<FxHashSet<_>>() != expected_keys
    {
        return Err(RetainedPropertyScrollScenePrepareError::PoolContract);
    }
    let reraster_count = actions
        .values()
        .filter(|action| **action == RetainedSurfaceCompileAction::Reraster)
        .count();
    let trace = RetainedPropertyScrollSceneBuildTrace {
        root_count: transaction.seal.roots.len(),
        generic_surface_count: 0,
        effect_surface_count: 0,
        scroll_group_count: transaction.scroll_groups.len(),
        backing: if has_tiled {
            ScrollSceneBackingKind::Tiled
        } else {
            ScrollSceneBackingKind::Single
        },
        tile_count,
        reraster_count,
        reuse_count: actions.len() - reraster_count,
        content_pair_bytes: pair_bytes,
    };
    Ok(PreparedRetainedPropertyScrollForest {
        viewport,
        graph,
        parent_ctx: ctx,
        boundaries: prepared_boundaries,
        actions,
        transaction,
        schedule: seal.schedule,
        clear_rgba_bits: clear_rgba.map(f32::to_bits),
        target_policy: PropertyScrollRootTargetPolicy::ContextRootTarget,
        frame_owner,
        budget: seal.budget,
        aggregate_pair_bytes: seal.aggregate_pair_bytes,
        parent_terminal,
        trace,
    })
}

/// Freezes the exact T->S receiver stamps, detached content groups and their
/// one-to-one joint transaction. No target is declared and no graph pass is
/// added until this function succeeds completely.
pub(crate) fn prepare_retained_transform_scroll_scene_from_pool<'a>(
    viewport: &'a mut Viewport,
    scene: ValidatedTransformScrollScene,
    graph: &'a mut FrameGraph,
    ctx: UiBuildContext,
    clear_rgba: [f32; 4],
    frame_owner: RetainedSurfaceFrameStageOwner,
) -> Result<PreparedRetainedTransformScrollScene<'a>, RetainedPropertyScrollScenePrepareError> {
    if !scene.is_canonical() {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    }
    if scene.roots.iter().any(|root| {
        matches!(
            root.boundary.steps.as_slice(),
            [
                PropertyScrollCompiledStep::AtomicProjectionSelectionHostBefore { .. },
                PropertyScrollCompiledStep::AtomicProjectionSelectionDetachedContent { .. },
                PropertyScrollCompiledStep::AtomicProjectionSelectionOverlayAfter { .. },
            ]
        )
    }) {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    }
    validate_parent_target(graph, &ctx)
        .map_err(|_| RetainedPropertyScrollScenePrepareError::ParentTarget)?;
    if ctx.viewport().scale_factor().to_bits() != scene.scale_factor_bits
        || ctx.paint_offset().map(f32::to_bits) != [0.0_f32.to_bits(); 2]
        || ctx.graphics_pass_context().scissor_rect.is_some()
        || ctx.opaque_rect_order() != 0
        || ctx.viewport().target_format() != scene.target_format
        || clear_rgba.iter().any(|component| !component.is_finite())
    {
        return Err(RetainedPropertyScrollScenePrepareError::ContextMismatch);
    }
    if !viewport.retained_property_scroll_scene_stage_is_available()
        || !viewport.retained_surface_frame_stage_owner_is_active(frame_owner)
    {
        return Err(RetainedPropertyScrollScenePrepareError::StageUnavailable);
    }
    let graph_keys = graph
        .declared_persistent_texture_keys()
        .collect::<FxHashSet<_>>();
    let mut declared = FxHashSet::default();
    let mut prepared_roots = Vec::with_capacity(scene.roots.len());
    let mut generic_stamps = Vec::with_capacity(scene.roots.len());
    let mut groups = Vec::with_capacity(scene.roots.len());
    let mut roots = Vec::with_capacity(scene.roots.len());
    let mut ordered_boundaries = Vec::with_capacity(scene.roots.len());
    let mut generic_bindings = Vec::with_capacity(scene.roots.len());
    let mut pair_bytes = 0_u64;
    let mut aggregate_resident_pair_bytes = 0_u64;
    let mut tile_count = 0usize;
    let mut has_tiled = false;

    let scale_factor_bits = scene.scale_factor_bits;
    for root in scene.roots {
        let ordinal = root.scene_root_ordinal;
        let scroll_planner = &root.boundary.planner.seal;
        let scroll = scroll_planner.scroll;
        let contents_clip = scroll_planner.contents_clip;
        let boundary_root = scroll_planner.scene_root;
        let boundary_stable_id = scroll_planner.scene_root_stable_id;
        let content_root = scroll_planner.admission.child;
        let content_stable_id = scroll_planner.admission.child_stable_id;
        let global_boundary = SceneBoundaryId {
            ordinal,
            owner: boundary_root,
            kind: SceneBoundaryKind::ScrollContents,
        };
        if !matches!(
            root.boundary.steps.as_slice(),
            [
                PropertyScrollCompiledStep::HostBefore { .. },
                PropertyScrollCompiledStep::DetachedContent { .. },
                PropertyScrollCompiledStep::OverlayAfter { .. },
            ]
        ) {
            return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
        }
        let prepared_boundary = prepare_retained_property_scroll_boundary_parts(
            root.boundary,
            global_boundary,
            scale_factor_bits,
            &graph_keys,
            &mut declared,
        )?;
        pair_bytes = pair_bytes
            .checked_add(prepared_boundary.pair_bytes)
            .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
        aggregate_resident_pair_bytes = aggregate_resident_pair_bytes
            .checked_add(prepared_boundary.pair_bytes)
            .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
        tile_count = tile_count
            .checked_add(prepared_boundary.tile_count)
            .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
        has_tiled |= prepared_boundary.backing_kind == ScrollSceneBackingKind::Tiled;

        let receiver_color_key =
            crate::view::base_component::transformed_layer_stable_key(root.receiver_stable_id);
        let receiver_depth_key = receiver_color_key
            .depth_stencil()
            .ok_or(RetainedPropertyScrollScenePrepareError::DescriptorPair)?;
        if !declared.insert(receiver_color_key)
            || !declared.insert(receiver_depth_key)
            || graph_keys.contains(&receiver_color_key)
            || graph_keys.contains(&receiver_depth_key)
        {
            return Err(
                RetainedPropertyScrollScenePrepareError::PersistentKeyAlreadyDeclared(
                    receiver_color_key,
                ),
            );
        }
        let receiver_color = texture_desc_for_logical_bounds(
            root.geometry.source_bounds,
            f32::from_bits(scale_factor_bits),
            None,
            scene.target_format,
        );
        let (receiver_color_desc, receiver_depth_desc) =
            persistent_target_texture_descriptors(receiver_color, receiver_color_key);
        aggregate_resident_pair_bytes = aggregate_resident_pair_bytes
            .checked_add(
                canonical_pair_bytes(&receiver_color_desc, &receiver_depth_desc)
                    .ok_or(RetainedPropertyScrollScenePrepareError::DescriptorPair)?,
            )
            .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;

        let content_stamps = prepared_boundary.group.ordered_stamps().to_vec();
        let (host_before, overlay) = prepared_boundary
            .authority
            .existing_host_overlay()
            .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
        let host_start = root.insertion.receiver_opaque_before;
        let host_end = host_start
            .checked_add(prepared_boundary.host_local_terminal)
            .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
        let overlay_count = prepared_boundary
            .parent_local_terminal
            .checked_sub(prepared_boundary.host_local_terminal)
            .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
        let overlay_end = host_end
            .checked_add(overlay_count)
            .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
        let host_span = host_start..host_end;
        let overlay_span = host_end..overlay_end;
        let host_artifact = super::compiler::validated_scroll_host_before_artifact_span_stamp(
            host_before,
            0,
            host_span.clone(),
        )
        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
        let overlay_artifact = super::compiler::validated_scroll_overlay_artifact_span_stamp(
            overlay,
            2,
            overlay_span.clone(),
        )
        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;

        let mut stamp_steps = Vec::with_capacity(root.receiver_steps.len());
        let mut receiver_cursor = 0_u32;
        for (step_index, step) in root.receiver_steps.iter().enumerate() {
            match step {
                super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact) => {
                    let validated = super::compiler::validate_transform_property_surface_artifact(
                        artifact,
                        root.receiver_root,
                        root.receiver.id,
                    )
                    .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
                    let count = checked_property_scroll_opaque_order_count(artifact)
                        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
                    let end = receiver_cursor
                        .checked_add(count)
                        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
                    let span =
                        super::compiler::validated_transform_property_surface_artifact_span_stamp(
                            &validated,
                            step_index,
                            receiver_cursor..end,
                        )
                        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
                    stamp_steps.push(super::RetainedSurfaceRasterStepStamp::ArtifactSpan(span));
                    receiver_cursor = end;
                }
                super::frame_recorder::RecordedTransformSurfaceStep::Boundary(marker) => {
                    if step_index != root.insertion.insertion_index
                        || *marker != root.insertion.scroll_cutout
                        || receiver_cursor != root.insertion.receiver_opaque_before
                    {
                        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
                    }
                    let dependency = super::compiler::TransformScrollBoundaryRasterDependency {
                        step_index,
                        scene_root_ordinal: ordinal,
                        receiver_owner: root.receiver.owner,
                        receiver_transform_id: root.receiver.id,
                        receiver_stable_id: root.receiver_stable_id,
                        scroll_boundary_ordinal: ordinal,
                        boundary_root,
                        boundary_stable_id,
                        content_root,
                        content_stable_id,
                        insertion_index: root.insertion.insertion_index,
                        receiver_step_count: root.receiver_steps.len(),
                        before_span: root.insertion.before_span.clone(),
                        after_span: root.insertion.after_span.clone(),
                        recorded_receiver_opaque_before: root.insertion.receiver_opaque_before,
                        recorded_receiver_opaque_after: root.insertion.receiver_opaque_after,
                        host_parent_span: host_span.clone(),
                        content_local_span: 0..prepared_boundary.content_local_terminal,
                        overlay_parent_span: overlay_span.clone(),
                        host_artifact: host_artifact.clone(),
                        overlay_artifact: overlay_artifact.clone(),
                        content_stamps: content_stamps.clone(),
                        scroll,
                        contents_clip,
                        receiver_local_raster_clips: Vec::new(),
                        receiver_ancestor_composite_clips: Vec::new(),
                        same_owner_role: root.same_owner_insertion.as_ref().map(|insertion| {
                            super::compiler::SameOwnerTransformScrollRasterRoleStamp {
                                owner: insertion.owner,
                                stable_id: insertion.stable_id,
                                transform: insertion.transform.id,
                                scroll: insertion.scroll.id,
                                contents_clip: insertion.contents_clip.id,
                                content_root: insertion.content_root,
                                content_stable_id: insertion.content_stable_id,
                            }
                        }),
                    };
                    if !super::compiler::transform_scroll_boundary_dependency_is_canonical(
                        &dependency,
                    ) {
                        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
                    }
                    stamp_steps.push(super::RetainedSurfaceRasterStepStamp::ScrollBoundary(
                        dependency,
                    ));
                    receiver_cursor = overlay_end;
                }
            }
        }
        let receiver_target = RetainedSurfaceRasterInputs {
            color: receiver_color_desc.clone(),
            depth: receiver_depth_desc,
            scale_factor_bits,
            source_bounds_bits: bounds_bits(root.geometry.source_bounds),
        };
        let receiver_stamp = super::compiler::validated_transform_scroll_receiver_raster_stamp(
            root.receiver_root,
            root.receiver_stable_id,
            receiver_target,
            stamp_steps,
            0..receiver_cursor,
        )
        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
        roots.push(RetainedPropertyScrollJointRootStamp {
            ordinal,
            root: root.receiver_root,
            stable_id: root.receiver_stable_id,
            boundary_span: ordinal
                ..ordinal
                    .checked_add(1)
                    .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?,
        });
        ordered_boundaries.push(global_boundary);
        generic_bindings.push(RetainedPropertyScrollGenericBindingStamp {
            boundary: global_boundary,
            resident_key: receiver_stamp.identity.resident_key(),
            color_key: receiver_stamp.identity.color_key,
        });
        groups.push(prepared_boundary.group.clone());
        generic_stamps.push(receiver_stamp.clone());
        prepared_roots.push(PreparedTransformScrollRoot {
            receiver: root.receiver,
            receiver_stable_id: root.receiver_stable_id,
            geometry: root.geometry,
            receiver_steps: root.receiver_steps,
            boundary: prepared_boundary,
            receiver_stamp,
            receiver_color_key,
            receiver_color_desc,
            receiver_opaque_terminal: receiver_cursor,
        });
    }
    let scroll_bindings = groups
        .iter()
        .map(|group| RetainedPropertyScrollGroupBindingStamp {
            boundary: group.boundary,
            content_root: group.content_root,
            content_stable_id: group.content_stable_id,
            backing_rank: group.backing_rank(),
            ordered_resident_keys: group.active_resident_keys(),
        })
        .collect();
    let transaction = RetainedPropertyScrollSceneTransaction {
        seal: RetainedPropertyScrollJointSeal {
            roots,
            ordered_boundaries,
            generic_bindings,
            scroll_bindings,
        },
        generic_authority: RetainedPropertyScrollGenericAuthority::TransformScrollCompiler,
        generic_full_set: generic_stamps,
        scroll_groups: groups,
    };
    if aggregate_resident_pair_bytes > scene.budget.max_active_pair_bytes
        || !transaction.is_canonical()
    {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    }
    let mut actions = viewport
        .freeze_retained_property_scroll_scene_compile_actions_from_pool(&transaction)
        .ok_or(RetainedPropertyScrollScenePrepareError::PoolContract)?;
    let expected_keys = transaction
        .ordered_stamps()
        .into_iter()
        .map(|stamp| stamp.identity.resident_key())
        .collect::<FxHashSet<_>>();
    if actions.len() != expected_keys.len()
        || actions.keys().copied().collect::<FxHashSet<_>>() != expected_keys
    {
        return Err(RetainedPropertyScrollScenePrepareError::PoolContract);
    }
    // Seal the only mixed pool case that cannot safely reach emission as-is:
    // rebuilding detached content while retaining the receiver would leave
    // stale baked content in that receiver. Upgrade the receiver before the
    // pre-clear capability exists, never during emission.
    for root in &prepared_roots {
        let content_requires_raster = root
            .boundary
            .group
            .active_resident_keys()
            .iter()
            .any(|key| actions.get(key).copied() == Some(RetainedSurfaceCompileAction::Reraster));
        if content_requires_raster {
            let receiver_action = actions
                .get_mut(&root.receiver_stamp.identity.resident_key())
                .ok_or(RetainedPropertyScrollScenePrepareError::PoolContract)?;
            if *receiver_action == RetainedSurfaceCompileAction::Reuse {
                *receiver_action = RetainedSurfaceCompileAction::Reraster;
            }
        }
    }
    let reraster_count = actions
        .values()
        .filter(|action| **action == RetainedSurfaceCompileAction::Reraster)
        .count();
    let trace = RetainedPropertyScrollSceneBuildTrace {
        root_count: prepared_roots.len(),
        generic_surface_count: prepared_roots.len(),
        effect_surface_count: 0,
        scroll_group_count: prepared_roots.len(),
        backing: if has_tiled {
            ScrollSceneBackingKind::Tiled
        } else {
            ScrollSceneBackingKind::Single
        },
        tile_count,
        reraster_count,
        reuse_count: actions.len() - reraster_count,
        content_pair_bytes: pair_bytes,
    };
    Ok(PreparedRetainedTransformScrollScene {
        viewport,
        graph,
        parent_ctx: ctx,
        roots: prepared_roots,
        actions,
        transaction,
        clear_rgba_bits: clear_rgba.map(f32::to_bits),
        frame_owner,
        trace,
    })
}

/// Freezes the exact direct E->S receiver stamps, detached content groups,
/// final composite witnesses and their one-to-one joint transaction. No
/// target is declared and no graph pass is added until every root succeeds.
pub(crate) fn prepare_retained_effect_scroll_scene_from_pool<'a>(
    viewport: &'a mut Viewport,
    scene: ValidatedEffectScrollSceneCheckpoint,
    graph: &'a mut FrameGraph,
    ctx: UiBuildContext,
    clear_rgba: [f32; 4],
    frame_owner: RetainedSurfaceFrameStageOwner,
) -> Result<PreparedRetainedEffectScrollScene<'a>, RetainedPropertyScrollScenePrepareError> {
    if !scene.is_canonical() {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    }
    if scene.roots.iter().any(|root| {
        matches!(
            root.boundary.steps.as_slice(),
            [
                PropertyScrollCompiledStep::AtomicProjectionSelectionHostBefore { .. },
                PropertyScrollCompiledStep::AtomicProjectionSelectionDetachedContent { .. },
                PropertyScrollCompiledStep::AtomicProjectionSelectionOverlayAfter { .. },
            ]
        )
    }) {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    }
    validate_parent_target(graph, &ctx)
        .map_err(|_| RetainedPropertyScrollScenePrepareError::ParentTarget)?;
    if ctx.viewport().scale_factor().to_bits() != scene.scale_factor_bits
        || ctx.paint_offset().map(f32::to_bits) != [0.0_f32.to_bits(); 2]
        || ctx.graphics_pass_context().scissor_rect.is_some()
        || ctx.opaque_rect_order() != 0
        || ctx.viewport().target_format() != scene.target_format
        || clear_rgba.iter().any(|component| !component.is_finite())
    {
        return Err(RetainedPropertyScrollScenePrepareError::ContextMismatch);
    }
    if !viewport.retained_property_scroll_scene_stage_is_available()
        || !viewport.retained_surface_frame_stage_owner_is_active(frame_owner)
    {
        return Err(RetainedPropertyScrollScenePrepareError::StageUnavailable);
    }

    let graph_keys = graph
        .declared_persistent_texture_keys()
        .collect::<FxHashSet<_>>();
    let mut declared = FxHashSet::default();
    let mut prepared_roots = Vec::with_capacity(scene.roots.len());
    let mut generic_stamps = Vec::with_capacity(scene.roots.len());
    let mut effect_contracts = Vec::with_capacity(scene.roots.len());
    let mut groups = Vec::with_capacity(scene.roots.len());
    let mut roots = Vec::with_capacity(scene.roots.len());
    let mut ordered_boundaries = Vec::with_capacity(scene.roots.len());
    let mut generic_bindings = Vec::with_capacity(scene.roots.len());
    let mut pair_bytes = 0_u64;
    let mut aggregate_resident_pair_bytes = 0_u64;
    let mut tile_count = 0usize;
    let mut has_tiled = false;

    let scale_factor_bits = scene.scale_factor_bits;
    for root in scene.roots {
        let ordinal = root.scene_root_ordinal;
        let scroll_planner = &root.boundary.planner.seal;
        let scroll = scroll_planner.scroll;
        let contents_clip = scroll_planner.contents_clip;
        let boundary_root = scroll_planner.scene_root;
        let boundary_stable_id = scroll_planner.scene_root_stable_id;
        let content_root = scroll_planner.admission.child;
        let content_stable_id = scroll_planner.admission.child_stable_id;
        let global_boundary = SceneBoundaryId {
            ordinal,
            owner: boundary_root,
            kind: SceneBoundaryKind::ScrollContents,
        };
        if !matches!(
            root.boundary.steps.as_slice(),
            [
                PropertyScrollCompiledStep::HostBefore { .. },
                PropertyScrollCompiledStep::DetachedContent { .. },
                PropertyScrollCompiledStep::OverlayAfter { .. },
            ]
        ) {
            return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
        }
        let prepared_boundary = prepare_retained_property_scroll_boundary_parts(
            root.boundary,
            global_boundary,
            scale_factor_bits,
            &graph_keys,
            &mut declared,
        )?;
        pair_bytes = pair_bytes
            .checked_add(prepared_boundary.pair_bytes)
            .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
        aggregate_resident_pair_bytes = aggregate_resident_pair_bytes
            .checked_add(prepared_boundary.pair_bytes)
            .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
        tile_count = tile_count
            .checked_add(prepared_boundary.tile_count)
            .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
        has_tiled |= prepared_boundary.backing_kind == ScrollSceneBackingKind::Tiled;

        let receiver_color_key =
            crate::view::base_component::isolation_layer_stable_key(root.receiver_stable_id);
        let receiver_depth_key = receiver_color_key
            .depth_stencil()
            .ok_or(RetainedPropertyScrollScenePrepareError::DescriptorPair)?;
        if !declared.insert(receiver_color_key)
            || !declared.insert(receiver_depth_key)
            || graph_keys.contains(&receiver_color_key)
            || graph_keys.contains(&receiver_depth_key)
        {
            return Err(
                RetainedPropertyScrollScenePrepareError::PersistentKeyAlreadyDeclared(
                    receiver_color_key,
                ),
            );
        }
        let source_bounds = RetainedSurfaceBounds {
            x: f32::from_bits(root.composite.source_bounds_bits[0]),
            y: f32::from_bits(root.composite.source_bounds_bits[1]),
            width: f32::from_bits(root.composite.source_bounds_bits[2]),
            height: f32::from_bits(root.composite.source_bounds_bits[3]),
            corner_radii: [0.0; 4],
        };
        let receiver_color = texture_desc_for_logical_bounds(
            source_bounds,
            f32::from_bits(scale_factor_bits),
            None,
            scene.target_format,
        );
        let (receiver_color_desc, receiver_depth_desc) =
            persistent_target_texture_descriptors(receiver_color, receiver_color_key);
        aggregate_resident_pair_bytes = aggregate_resident_pair_bytes
            .checked_add(
                canonical_pair_bytes(&receiver_color_desc, &receiver_depth_desc)
                    .ok_or(RetainedPropertyScrollScenePrepareError::DescriptorPair)?,
            )
            .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;

        let content_stamps = prepared_boundary.group.ordered_stamps().to_vec();
        let (host_before, overlay) = prepared_boundary
            .authority
            .existing_host_overlay()
            .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
        let host_start = root.insertion.receiver_opaque_before;
        let host_end = host_start
            .checked_add(prepared_boundary.host_local_terminal)
            .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
        let overlay_count = prepared_boundary
            .parent_local_terminal
            .checked_sub(prepared_boundary.host_local_terminal)
            .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
        let overlay_end = host_end
            .checked_add(overlay_count)
            .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
        let host_span = host_start..host_end;
        let overlay_span = host_end..overlay_end;
        let host_artifact = super::compiler::validated_scroll_host_before_artifact_span_stamp(
            host_before,
            0,
            host_span.clone(),
        )
        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
        let overlay_artifact = super::compiler::validated_scroll_overlay_artifact_span_stamp(
            overlay,
            2,
            overlay_span.clone(),
        )
        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;

        let mut stamp_steps = Vec::with_capacity(root.receiver_steps.len());
        let mut receiver_cursor = 0_u32;
        for (step_index, step) in root.receiver_steps.iter().enumerate() {
            match step {
                super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact) => {
                    let validated = super::compiler::validate_effect_property_surface_artifact(
                        artifact,
                        &root.insertion.artifact_contract,
                    )
                    .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
                    let count = checked_property_scroll_opaque_order_count(artifact)
                        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
                    let end = receiver_cursor
                        .checked_add(count)
                        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
                    let span =
                        super::compiler::validated_effect_property_surface_artifact_span_stamp(
                            &validated,
                            step_index,
                            receiver_cursor..end,
                        )
                        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
                    stamp_steps.push(super::RetainedSurfaceRasterStepStamp::ArtifactSpan(span));
                    receiver_cursor = end;
                }
                super::frame_recorder::RecordedTransformSurfaceStep::Boundary(marker) => {
                    if step_index != root.insertion.insertion_index
                        || *marker != root.insertion.scroll_cutout
                        || receiver_cursor != root.insertion.receiver_opaque_before
                    {
                        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
                    }
                    let dependency = super::compiler::EffectScrollBoundaryRasterDependency {
                        step_index,
                        scene_root_ordinal: ordinal,
                        receiver_owner: root.receiver.owner,
                        receiver_stable_id: root.receiver_stable_id,
                        scroll_boundary_ordinal: ordinal,
                        boundary_root,
                        boundary_stable_id,
                        content_root,
                        content_stable_id,
                        insertion_index: root.insertion.insertion_index,
                        receiver_step_count: root.receiver_steps.len(),
                        before_span: root.insertion.before_span.clone(),
                        after_span: root.insertion.after_span.clone(),
                        recorded_receiver_opaque_before: root.insertion.receiver_opaque_before,
                        recorded_receiver_opaque_after: root.insertion.receiver_opaque_after,
                        host_parent_span: host_span.clone(),
                        content_local_span: 0..prepared_boundary.content_local_terminal,
                        overlay_parent_span: overlay_span.clone(),
                        host_artifact: host_artifact.clone(),
                        overlay_artifact: overlay_artifact.clone(),
                        content_stamps: content_stamps.clone(),
                        scroll,
                        contents_clip,
                        receiver_local_raster_clips: Vec::new(),
                        receiver_ancestor_composite_clips: Vec::new(),
                        same_owner_role: root.same_owner_insertion.as_ref().map(|insertion| {
                            super::compiler::SameOwnerEffectScrollRasterRoleStamp {
                                owner: insertion.owner,
                                stable_id: insertion.stable_id,
                                effect: insertion.effect.id,
                                scroll: insertion.scroll.id,
                                contents_clip: insertion.contents_clip.id,
                                content_root: insertion.content_root,
                                content_stable_id: insertion.content_stable_id,
                            }
                        }),
                    };
                    if !super::compiler::effect_scroll_boundary_dependency_is_canonical(&dependency)
                    {
                        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
                    }
                    stamp_steps.push(super::RetainedSurfaceRasterStepStamp::EffectScrollBoundary(
                        dependency,
                    ));
                    receiver_cursor = overlay_end;
                }
            }
        }
        let receiver_target = RetainedSurfaceRasterInputs {
            color: receiver_color_desc.clone(),
            depth: receiver_depth_desc,
            scale_factor_bits,
            source_bounds_bits: root.composite.source_bounds_bits,
        };
        let receiver_stamp = super::compiler::validated_effect_scroll_receiver_raster_stamp(
            &root.insertion.artifact_contract,
            receiver_target,
            stamp_steps,
            0..receiver_cursor,
        )
        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
        roots.push(RetainedPropertyScrollJointRootStamp {
            ordinal,
            root: root.receiver_root,
            stable_id: root.receiver_stable_id,
            boundary_span: ordinal
                ..ordinal
                    .checked_add(1)
                    .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?,
        });
        ordered_boundaries.push(global_boundary);
        generic_bindings.push(RetainedPropertyScrollGenericBindingStamp {
            boundary: global_boundary,
            resident_key: receiver_stamp.identity.resident_key(),
            color_key: receiver_stamp.identity.color_key,
        });
        groups.push(prepared_boundary.group.clone());
        generic_stamps.push(receiver_stamp.clone());
        effect_contracts.push(root.insertion.artifact_contract.clone());
        prepared_roots.push(PreparedEffectScrollRoot {
            receiver: root.receiver,
            receiver_stable_id: root.receiver_stable_id,
            artifact_contract: root.insertion.artifact_contract,
            composite: root.composite,
            receiver_steps: root.receiver_steps,
            boundary: prepared_boundary,
            receiver_stamp,
            receiver_color_key,
            receiver_color_desc,
            receiver_opaque_terminal: receiver_cursor,
        });
    }

    let scroll_bindings = groups
        .iter()
        .map(|group| RetainedPropertyScrollGroupBindingStamp {
            boundary: group.boundary,
            content_root: group.content_root,
            content_stable_id: group.content_stable_id,
            backing_rank: group.backing_rank(),
            ordered_resident_keys: group.active_resident_keys(),
        })
        .collect();
    let transaction = RetainedPropertyScrollSceneTransaction {
        seal: RetainedPropertyScrollJointSeal {
            roots,
            ordered_boundaries,
            generic_bindings,
            scroll_bindings,
        },
        generic_authority: RetainedPropertyScrollGenericAuthority::EffectScrollCompiler(
            effect_contracts,
        ),
        generic_full_set: generic_stamps,
        scroll_groups: groups,
    };
    if aggregate_resident_pair_bytes > scene.budget.max_active_pair_bytes
        || !transaction.is_canonical()
    {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    }
    let mut actions = viewport
        .freeze_retained_property_scroll_scene_compile_actions_from_pool(&transaction)
        .ok_or(RetainedPropertyScrollScenePrepareError::PoolContract)?;
    let expected_keys = transaction
        .ordered_stamps()
        .into_iter()
        .map(|stamp| stamp.identity.resident_key())
        .collect::<FxHashSet<_>>();
    if actions.len() != expected_keys.len()
        || actions.keys().copied().collect::<FxHashSet<_>>() != expected_keys
    {
        return Err(RetainedPropertyScrollScenePrepareError::PoolContract);
    }
    for root in &prepared_roots {
        let content_requires_raster = root
            .boundary
            .group
            .active_resident_keys()
            .iter()
            .any(|key| actions.get(key).copied() == Some(RetainedSurfaceCompileAction::Reraster));
        if content_requires_raster {
            let receiver_action = actions
                .get_mut(&root.receiver_stamp.identity.resident_key())
                .ok_or(RetainedPropertyScrollScenePrepareError::PoolContract)?;
            if *receiver_action == RetainedSurfaceCompileAction::Reuse {
                *receiver_action = RetainedSurfaceCompileAction::Reraster;
            }
        }
    }
    let reraster_count = actions
        .values()
        .filter(|action| **action == RetainedSurfaceCompileAction::Reraster)
        .count();
    let trace = RetainedPropertyScrollSceneBuildTrace {
        root_count: prepared_roots.len(),
        generic_surface_count: prepared_roots.len(),
        effect_surface_count: prepared_roots.len(),
        scroll_group_count: prepared_roots.len(),
        backing: if has_tiled {
            ScrollSceneBackingKind::Tiled
        } else {
            ScrollSceneBackingKind::Single
        },
        tile_count,
        reraster_count,
        reuse_count: actions.len() - reraster_count,
        content_pair_bytes: pair_bytes,
    };
    Ok(PreparedRetainedEffectScrollScene {
        viewport,
        graph,
        parent_ctx: ctx,
        roots: prepared_roots,
        actions,
        transaction,
        clear_rgba_bits: clear_rgba.map(f32::to_bits),
        frame_owner,
        trace,
    })
}

/// Freezes T, E and detached content as one pool transaction. No descriptor
/// is declared in the graph and no pass is emitted until every root, action
/// and aggregate byte count has been sealed.
pub(crate) fn prepare_retained_transform_effect_scroll_scene_from_pool<'a>(
    viewport: &'a mut Viewport,
    scene: ValidatedTransformEffectScrollScene,
    graph: &'a mut FrameGraph,
    ctx: UiBuildContext,
    clear_rgba: [f32; 4],
    frame_owner: RetainedSurfaceFrameStageOwner,
) -> Result<PreparedRetainedTransformEffectScrollScene<'a>, RetainedPropertyScrollScenePrepareError>
{
    if !scene.is_canonical() {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    }
    if scene.roots.iter().any(|root| {
        matches!(
            root.boundary.steps.as_slice(),
            [
                PropertyScrollCompiledStep::AtomicProjectionSelectionHostBefore { .. },
                PropertyScrollCompiledStep::AtomicProjectionSelectionDetachedContent { .. },
                PropertyScrollCompiledStep::AtomicProjectionSelectionOverlayAfter { .. },
            ]
        )
    }) {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    }
    validate_parent_target(graph, &ctx)
        .map_err(|_| RetainedPropertyScrollScenePrepareError::ParentTarget)?;
    if ctx.viewport().scale_factor().to_bits() != scene.scale_factor_bits
        || ctx.paint_offset().map(f32::to_bits) != [0.0_f32.to_bits(); 2]
        || ctx.graphics_pass_context().scissor_rect.is_some()
        || ctx.opaque_rect_order() != 0
        || ctx.viewport().target_format() != scene.target_format
        || clear_rgba.iter().any(|component| !component.is_finite())
    {
        return Err(RetainedPropertyScrollScenePrepareError::ContextMismatch);
    }
    if !viewport.retained_property_scroll_scene_stage_is_available()
        || !viewport.retained_surface_frame_stage_owner_is_active(frame_owner)
    {
        return Err(RetainedPropertyScrollScenePrepareError::StageUnavailable);
    }
    let graph_keys = graph
        .declared_persistent_texture_keys()
        .collect::<FxHashSet<_>>();
    let mut declared = FxHashSet::default();
    let mut prepared_roots = Vec::with_capacity(scene.roots.len());
    let mut generic_stamps = Vec::with_capacity(scene.roots.len().saturating_mul(2));
    let mut contracts = Vec::with_capacity(scene.roots.len());
    let mut groups = Vec::with_capacity(scene.roots.len());
    let mut roots = Vec::with_capacity(scene.roots.len());
    let mut ordered_boundaries = Vec::with_capacity(scene.roots.len());
    let mut generic_bindings = Vec::with_capacity(scene.roots.len().saturating_mul(2));
    let mut pair_bytes = 0_u64;
    let mut aggregate_resident_pair_bytes = 0_u64;
    let mut tile_count = 0usize;
    let mut has_tiled = false;
    let max_dimension_2d = scene.budget.max_dimension_2d;

    let scale_factor_bits = scene.scale_factor_bits;
    for root in scene.roots {
        let ordinal = root.scene_root_ordinal;
        let inner_insertion = &root.insertion.inner;
        let scroll_planner = &root.boundary.planner.seal;
        let scroll = scroll_planner.scroll;
        let contents_clip = scroll_planner.contents_clip;
        let boundary_root = scroll_planner.scene_root;
        let boundary_stable_id = scroll_planner.scene_root_stable_id;
        let content_root = scroll_planner.admission.child;
        let content_stable_id = scroll_planner.admission.child_stable_id;
        let global_boundary = SceneBoundaryId {
            ordinal,
            owner: boundary_root,
            kind: SceneBoundaryKind::ScrollContents,
        };
        if !matches!(
            root.boundary.steps.as_slice(),
            [
                PropertyScrollCompiledStep::HostBefore { .. },
                PropertyScrollCompiledStep::DetachedContent { .. },
                PropertyScrollCompiledStep::OverlayAfter { .. },
            ]
        ) {
            return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
        }
        let prepared_boundary = prepare_retained_property_scroll_boundary_parts(
            root.boundary,
            global_boundary,
            scale_factor_bits,
            &graph_keys,
            &mut declared,
        )?;
        pair_bytes = pair_bytes
            .checked_add(prepared_boundary.pair_bytes)
            .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
        aggregate_resident_pair_bytes = aggregate_resident_pair_bytes
            .checked_add(prepared_boundary.pair_bytes)
            .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
        tile_count = tile_count
            .checked_add(prepared_boundary.tile_count)
            .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
        has_tiled |= prepared_boundary.backing_kind == ScrollSceneBackingKind::Tiled;

        let inner_color_key = crate::view::base_component::isolation_layer_stable_key(
            inner_insertion.receiver_stable_id,
        );
        let outer_color_key =
            crate::view::base_component::transformed_layer_stable_key(root.outer_stable_id);
        for color_key in [inner_color_key, outer_color_key] {
            let depth_key = color_key
                .depth_stencil()
                .ok_or(RetainedPropertyScrollScenePrepareError::DescriptorPair)?;
            if !declared.insert(color_key)
                || !declared.insert(depth_key)
                || graph_keys.contains(&color_key)
                || graph_keys.contains(&depth_key)
            {
                return Err(
                    RetainedPropertyScrollScenePrepareError::PersistentKeyAlreadyDeclared(
                        color_key,
                    ),
                );
            }
        }
        let inner_bounds_values = root.composite.source_bounds_bits.map(f32::from_bits);
        let inner_bounds = RetainedSurfaceBounds {
            x: inner_bounds_values[0],
            y: inner_bounds_values[1],
            width: inner_bounds_values[2],
            height: inner_bounds_values[3],
            corner_radii: [0.0; 4],
        };
        let inner_color = texture_desc_for_logical_bounds(
            inner_bounds,
            f32::from_bits(scale_factor_bits),
            None,
            scene.target_format,
        );
        let (inner_color_desc, inner_depth_desc) =
            persistent_target_texture_descriptors(inner_color, inner_color_key);
        let outer_color = texture_desc_for_logical_bounds(
            root.outer_geometry.source_bounds,
            f32::from_bits(scale_factor_bits),
            None,
            scene.target_format,
        );
        let (outer_color_desc, outer_depth_desc) =
            persistent_target_texture_descriptors(outer_color, outer_color_key);
        for (color, depth) in [
            (&inner_color_desc, &inner_depth_desc),
            (&outer_color_desc, &outer_depth_desc),
        ] {
            if color.width() != depth.width()
                || color.height() != depth.height()
                || color.width() > max_dimension_2d
                || color.height() > max_dimension_2d
            {
                return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
            }
            aggregate_resident_pair_bytes = aggregate_resident_pair_bytes
                .checked_add(
                    canonical_pair_bytes(color, depth)
                        .ok_or(RetainedPropertyScrollScenePrepareError::DescriptorPair)?,
                )
                .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
        }

        let content_stamps = prepared_boundary.group.ordered_stamps().to_vec();
        let (host_before, overlay) = prepared_boundary
            .authority
            .existing_host_overlay()
            .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
        let host_start = inner_insertion.receiver_opaque_before;
        let host_end = host_start
            .checked_add(prepared_boundary.host_local_terminal)
            .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
        let overlay_count = prepared_boundary
            .parent_local_terminal
            .checked_sub(prepared_boundary.host_local_terminal)
            .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
        let overlay_end = host_end
            .checked_add(overlay_count)
            .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
        let host_span = host_start..host_end;
        let overlay_span = host_end..overlay_end;
        let host_artifact = super::compiler::validated_scroll_host_before_artifact_span_stamp(
            host_before,
            0,
            host_span.clone(),
        )
        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
        let overlay_artifact = super::compiler::validated_scroll_overlay_artifact_span_stamp(
            overlay,
            2,
            overlay_span.clone(),
        )
        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;

        let mut inner_stamp_steps = Vec::with_capacity(root.inner_steps.len());
        let mut inner_cursor = 0_u32;
        for (step_index, step) in root.inner_steps.iter().enumerate() {
            match step {
                super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact) => {
                    let validated = super::compiler::validate_effect_property_surface_artifact(
                        artifact,
                        &inner_insertion.artifact_contract,
                    )
                    .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
                    let end = inner_cursor
                        .checked_add(
                            checked_property_scroll_opaque_order_count(artifact)
                                .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?,
                        )
                        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
                    let span =
                        super::compiler::validated_effect_property_surface_artifact_span_stamp(
                            &validated,
                            step_index,
                            inner_cursor..end,
                        )
                        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
                    inner_stamp_steps
                        .push(super::RetainedSurfaceRasterStepStamp::ArtifactSpan(span));
                    inner_cursor = end;
                }
                super::frame_recorder::RecordedTransformSurfaceStep::Boundary(marker) => {
                    if step_index != inner_insertion.insertion_index
                        || *marker != inner_insertion.scroll_cutout
                        || inner_cursor != inner_insertion.receiver_opaque_before
                    {
                        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
                    }
                    let dependency = super::compiler::EffectScrollBoundaryRasterDependency {
                        step_index,
                        scene_root_ordinal: ordinal,
                        receiver_owner: inner_insertion.receiver.owner,
                        receiver_stable_id: inner_insertion.receiver_stable_id,
                        scroll_boundary_ordinal: ordinal,
                        boundary_root,
                        boundary_stable_id,
                        content_root,
                        content_stable_id,
                        insertion_index: inner_insertion.insertion_index,
                        receiver_step_count: root.inner_steps.len(),
                        before_span: inner_insertion.before_span.clone(),
                        after_span: inner_insertion.after_span.clone(),
                        recorded_receiver_opaque_before: inner_insertion.receiver_opaque_before,
                        recorded_receiver_opaque_after: inner_insertion.receiver_opaque_after,
                        host_parent_span: host_span.clone(),
                        content_local_span: 0..prepared_boundary.content_local_terminal,
                        overlay_parent_span: overlay_span.clone(),
                        host_artifact: host_artifact.clone(),
                        overlay_artifact: overlay_artifact.clone(),
                        content_stamps: content_stamps.clone(),
                        scroll,
                        contents_clip,
                        receiver_local_raster_clips: Vec::new(),
                        receiver_ancestor_composite_clips: Vec::new(),
                        same_owner_role: None,
                    };
                    if !super::compiler::effect_scroll_boundary_dependency_is_canonical(&dependency)
                    {
                        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
                    }
                    inner_stamp_steps.push(
                        super::RetainedSurfaceRasterStepStamp::EffectScrollBoundary(dependency),
                    );
                    inner_cursor = overlay_end;
                }
            }
        }
        let inner_target = RetainedSurfaceRasterInputs {
            color: inner_color_desc.clone(),
            depth: inner_depth_desc,
            scale_factor_bits,
            source_bounds_bits: root.composite.source_bounds_bits,
        };
        let inner_stamp = super::compiler::validated_effect_scroll_receiver_raster_stamp(
            &inner_insertion.artifact_contract,
            inner_target,
            inner_stamp_steps,
            0..inner_cursor,
        )
        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;

        let mut outer_stamp_steps = Vec::with_capacity(root.outer_steps.len());
        let mut outer_cursor = 0_u32;
        for (step_index, step) in root.outer_steps.iter().enumerate() {
            match step {
                super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact) => {
                    let validated = super::compiler::validate_transform_property_surface_artifact(
                        artifact,
                        root.outer_receiver.owner,
                        root.outer_receiver.id,
                    )
                    .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
                    let end = outer_cursor
                        .checked_add(
                            checked_property_scroll_opaque_order_count(artifact)
                                .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?,
                        )
                        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
                    let span =
                        super::compiler::validated_transform_property_surface_artifact_span_stamp(
                            &validated,
                            step_index,
                            outer_cursor..end,
                        )
                        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
                    outer_stamp_steps
                        .push(super::RetainedSurfaceRasterStepStamp::ArtifactSpan(span));
                    outer_cursor = end;
                }
                super::frame_recorder::RecordedTransformSurfaceStep::Boundary(marker) => {
                    if step_index != root.insertion.outer_insertion_index
                        || *marker != root.insertion.effect_cutout
                        || outer_cursor != root.insertion.outer_opaque_before
                    {
                        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
                    }
                    let dependency = super::compiler::TransformEffectScrollChildRasterDependency {
                        step_index,
                        child_stamp: Box::new(inner_stamp.clone()),
                        child_source_bounds_bits: inner_stamp.target.source_bounds_bits,
                        child_opacity_bits: root.composite.opacity_bits,
                        child_effect_generation: root.composite.effect_generation,
                        local_basis: root.outer_receiver.id,
                        parent_opaque_order_before: outer_cursor,
                        parent_opaque_order_after: outer_cursor,
                    };
                    if !super::compiler::transform_effect_scroll_child_dependency_validates_contract(
                        &dependency,
                        root.outer_receiver.id,
                        &inner_insertion.artifact_contract,
                    ) {
                        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
                    }
                    outer_stamp_steps.push(
                        super::RetainedSurfaceRasterStepStamp::TransformEffectScrollChild(
                            dependency,
                        ),
                    );
                }
            }
        }
        let outer_target = RetainedSurfaceRasterInputs {
            color: outer_color_desc.clone(),
            depth: outer_depth_desc,
            scale_factor_bits,
            source_bounds_bits: bounds_bits(root.outer_geometry.source_bounds),
        };
        let outer_stamp = super::compiler::validated_transform_effect_scroll_outer_raster_stamp(
            root.outer_receiver.id,
            root.outer_stable_id,
            &inner_insertion.artifact_contract,
            outer_target,
            outer_stamp_steps,
            0..outer_cursor,
        )
        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;

        roots.push(RetainedPropertyScrollJointRootStamp {
            ordinal,
            root: root.outer_receiver.owner,
            stable_id: root.outer_stable_id,
            boundary_span: ordinal
                ..ordinal
                    .checked_add(1)
                    .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?,
        });
        ordered_boundaries.push(global_boundary);
        for stamp in [&outer_stamp, &inner_stamp] {
            generic_bindings.push(RetainedPropertyScrollGenericBindingStamp {
                boundary: global_boundary,
                resident_key: stamp.identity.resident_key(),
                color_key: stamp.identity.color_key,
            });
        }
        groups.push(prepared_boundary.group.clone());
        generic_stamps.push(outer_stamp.clone());
        generic_stamps.push(inner_stamp.clone());
        contracts.push(TransformEffectScrollCompilerContract {
            outer_transform: root.outer_receiver.id,
            child: inner_insertion.artifact_contract.clone(),
        });
        prepared_roots.push(PreparedTransformEffectScrollRoot {
            outer_receiver: root.outer_receiver,
            outer_stable_id: root.outer_stable_id,
            outer_geometry: root.outer_geometry,
            outer_steps: root.outer_steps,
            outer_stamp,
            outer_color_key,
            outer_color_desc,
            outer_opaque_terminal: outer_cursor,
            inner: PreparedEffectScrollRoot {
                receiver: inner_insertion.receiver,
                receiver_stable_id: inner_insertion.receiver_stable_id,
                artifact_contract: inner_insertion.artifact_contract.clone(),
                composite: root.composite,
                receiver_steps: root.inner_steps,
                boundary: prepared_boundary,
                receiver_stamp: inner_stamp,
                receiver_color_key: inner_color_key,
                receiver_color_desc: inner_color_desc,
                receiver_opaque_terminal: inner_cursor,
            },
        });
    }

    let scroll_bindings = groups
        .iter()
        .map(|group| RetainedPropertyScrollGroupBindingStamp {
            boundary: group.boundary,
            content_root: group.content_root,
            content_stable_id: group.content_stable_id,
            backing_rank: group.backing_rank(),
            ordered_resident_keys: group.active_resident_keys(),
        })
        .collect();
    let transaction = RetainedPropertyScrollSceneTransaction {
        seal: RetainedPropertyScrollJointSeal {
            roots,
            ordered_boundaries,
            generic_bindings,
            scroll_bindings,
        },
        generic_authority: RetainedPropertyScrollGenericAuthority::TransformEffectScrollCompiler(
            contracts,
        ),
        generic_full_set: generic_stamps,
        scroll_groups: groups,
    };
    if aggregate_resident_pair_bytes > scene.budget.max_active_pair_bytes
        || !transaction.is_canonical()
    {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    }
    let mut actions = viewport
        .freeze_retained_property_scroll_scene_compile_actions_from_pool(&transaction)
        .ok_or(RetainedPropertyScrollScenePrepareError::PoolContract)?;
    let expected_keys = transaction
        .ordered_stamps()
        .into_iter()
        .map(|stamp| stamp.identity.resident_key())
        .collect::<FxHashSet<_>>();
    if actions.len() != expected_keys.len()
        || actions.keys().copied().collect::<FxHashSet<_>>() != expected_keys
    {
        return Err(RetainedPropertyScrollScenePrepareError::PoolContract);
    }
    for root in &prepared_roots {
        let content_requires_raster = root
            .inner
            .boundary
            .group
            .active_resident_keys()
            .iter()
            .any(|key| actions.get(key).copied() == Some(RetainedSurfaceCompileAction::Reraster));
        if content_requires_raster {
            let inner_action = actions
                .get_mut(&root.inner.receiver_stamp.identity.resident_key())
                .ok_or(RetainedPropertyScrollScenePrepareError::PoolContract)?;
            if *inner_action == RetainedSurfaceCompileAction::Reuse {
                *inner_action = RetainedSurfaceCompileAction::Reraster;
            }
        }
    }
    let reraster_count = actions
        .values()
        .filter(|action| **action == RetainedSurfaceCompileAction::Reraster)
        .count();
    let trace = RetainedPropertyScrollSceneBuildTrace {
        root_count: prepared_roots.len(),
        generic_surface_count: prepared_roots.len().saturating_mul(2),
        effect_surface_count: prepared_roots.len(),
        scroll_group_count: prepared_roots.len(),
        backing: if has_tiled {
            ScrollSceneBackingKind::Tiled
        } else {
            ScrollSceneBackingKind::Single
        },
        tile_count,
        reraster_count,
        reuse_count: actions.len() - reraster_count,
        content_pair_bytes: pair_bytes,
    };
    Ok(PreparedRetainedTransformEffectScrollScene {
        viewport,
        graph,
        parent_ctx: ctx,
        roots: prepared_roots,
        actions,
        transaction,
        clear_rgba_bits: clear_rgba.map(f32::to_bits),
        frame_owner,
        trace,
    })
}

/// Freezes the exact E -> T -> ScrollContents scene as one joint pool
/// transaction. T owns H/C/O, E owns the typed T composite edge, and neither
/// graph descriptors nor passes are emitted until the complete scene seals.
pub(crate) fn prepare_retained_effect_transform_scroll_scene_from_pool<'a>(
    viewport: &'a mut Viewport,
    scene: ValidatedEffectTransformScrollScene,
    graph: &'a mut FrameGraph,
    ctx: UiBuildContext,
    clear_rgba: [f32; 4],
    frame_owner: RetainedSurfaceFrameStageOwner,
) -> Result<PreparedRetainedEffectTransformScrollScene<'a>, RetainedPropertyScrollScenePrepareError>
{
    if !scene.is_canonical()
        || scene.roots.iter().any(|root| {
            matches!(
                root.boundary.steps.as_slice(),
                [
                    PropertyScrollCompiledStep::AtomicProjectionSelectionHostBefore { .. },
                    PropertyScrollCompiledStep::AtomicProjectionSelectionDetachedContent { .. },
                    PropertyScrollCompiledStep::AtomicProjectionSelectionOverlayAfter { .. },
                ]
            )
        })
    {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    }
    validate_parent_target(graph, &ctx)
        .map_err(|_| RetainedPropertyScrollScenePrepareError::ParentTarget)?;
    if ctx.viewport().scale_factor().to_bits() != scene.scale_factor_bits
        || ctx.paint_offset().map(f32::to_bits) != [0.0_f32.to_bits(); 2]
        || ctx.graphics_pass_context().scissor_rect.is_some()
        || ctx.opaque_rect_order() != 0
        || ctx.viewport().target_format() != scene.target_format
        || clear_rgba.iter().any(|component| !component.is_finite())
    {
        return Err(RetainedPropertyScrollScenePrepareError::ContextMismatch);
    }
    if !viewport.retained_property_scroll_scene_stage_is_available()
        || !viewport.retained_surface_frame_stage_owner_is_active(frame_owner)
    {
        return Err(RetainedPropertyScrollScenePrepareError::StageUnavailable);
    }

    let graph_keys = graph
        .declared_persistent_texture_keys()
        .collect::<FxHashSet<_>>();
    let mut declared = FxHashSet::default();
    let mut prepared_roots = Vec::with_capacity(scene.roots.len());
    let mut generic_stamps = Vec::with_capacity(scene.roots.len().saturating_mul(2));
    let mut contracts = Vec::with_capacity(scene.roots.len());
    let mut groups = Vec::with_capacity(scene.roots.len());
    let mut roots = Vec::with_capacity(scene.roots.len());
    let mut ordered_boundaries = Vec::with_capacity(scene.roots.len());
    let mut generic_bindings = Vec::with_capacity(scene.roots.len().saturating_mul(2));
    let mut pair_bytes = 0_u64;
    let mut aggregate_resident_pair_bytes = 0_u64;
    let mut tile_count = 0usize;
    let mut has_tiled = false;
    let scale_factor_bits = scene.scale_factor_bits;

    for root in scene.roots {
        let ordinal = root.scene_root_ordinal;
        let insertion = &root.insertion;
        let inner = &insertion.inner;
        let scroll_planner = &root.boundary.planner.seal;
        let scroll = scroll_planner.scroll;
        let contents_clip = scroll_planner.contents_clip;
        let boundary_root = scroll_planner.scene_root;
        let boundary_stable_id = scroll_planner.scene_root_stable_id;
        let content_root = scroll_planner.admission.child;
        let content_stable_id = scroll_planner.admission.child_stable_id;
        let global_boundary = SceneBoundaryId {
            ordinal,
            owner: boundary_root,
            kind: SceneBoundaryKind::ScrollContents,
        };
        if !matches!(
            root.boundary.steps.as_slice(),
            [
                PropertyScrollCompiledStep::HostBefore { .. },
                PropertyScrollCompiledStep::DetachedContent { .. },
                PropertyScrollCompiledStep::OverlayAfter { .. },
            ]
        ) {
            return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
        }
        let prepared_boundary = prepare_retained_property_scroll_boundary_parts(
            root.boundary,
            global_boundary,
            scale_factor_bits,
            &graph_keys,
            &mut declared,
        )?;
        pair_bytes = pair_bytes
            .checked_add(prepared_boundary.pair_bytes)
            .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
        aggregate_resident_pair_bytes = aggregate_resident_pair_bytes
            .checked_add(prepared_boundary.pair_bytes)
            .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
        tile_count = tile_count
            .checked_add(prepared_boundary.tile_count)
            .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
        has_tiled |= prepared_boundary.backing_kind == ScrollSceneBackingKind::Tiled;

        let outer_color_key =
            crate::view::base_component::isolation_layer_stable_key(insertion.outer_stable_id);
        let inner_color_key =
            crate::view::base_component::transformed_layer_stable_key(inner.receiver_stable_id);
        for color_key in [outer_color_key, inner_color_key] {
            let depth_key = color_key
                .depth_stencil()
                .ok_or(RetainedPropertyScrollScenePrepareError::DescriptorPair)?;
            if !declared.insert(color_key)
                || !declared.insert(depth_key)
                || graph_keys.contains(&color_key)
                || graph_keys.contains(&depth_key)
            {
                return Err(
                    RetainedPropertyScrollScenePrepareError::PersistentKeyAlreadyDeclared(
                        color_key,
                    ),
                );
            }
        }
        let outer_values = insertion.outer_raster_bounds_bits.map(f32::from_bits);
        let outer_bounds = RetainedSurfaceBounds {
            x: outer_values[0],
            y: outer_values[1],
            width: outer_values[2],
            height: outer_values[3],
            corner_radii: [0.0; 4],
        };
        let outer_color = texture_desc_for_logical_bounds(
            outer_bounds,
            f32::from_bits(scale_factor_bits),
            None,
            scene.target_format,
        );
        let (outer_color_desc, outer_depth_desc) =
            persistent_target_texture_descriptors(outer_color, outer_color_key);
        let inner_color = texture_desc_for_logical_bounds(
            insertion.inner_geometry.source_bounds,
            f32::from_bits(scale_factor_bits),
            None,
            scene.target_format,
        );
        let (inner_color_desc, inner_depth_desc) =
            persistent_target_texture_descriptors(inner_color, inner_color_key);
        for (color, depth) in [
            (&outer_color_desc, &outer_depth_desc),
            (&inner_color_desc, &inner_depth_desc),
        ] {
            if color.width() != depth.width()
                || color.height() != depth.height()
                || color.width() > scene.budget.max_dimension_2d
                || color.height() > scene.budget.max_dimension_2d
            {
                return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
            }
            aggregate_resident_pair_bytes = aggregate_resident_pair_bytes
                .checked_add(
                    canonical_pair_bytes(color, depth)
                        .ok_or(RetainedPropertyScrollScenePrepareError::DescriptorPair)?,
                )
                .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
        }

        let content_stamps = prepared_boundary.group.ordered_stamps().to_vec();
        let (host_before, overlay) = prepared_boundary
            .authority
            .existing_host_overlay()
            .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
        let host_start = inner.receiver_opaque_before;
        let host_end = host_start
            .checked_add(prepared_boundary.host_local_terminal)
            .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
        let overlay_count = prepared_boundary
            .parent_local_terminal
            .checked_sub(prepared_boundary.host_local_terminal)
            .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
        let overlay_end = host_end
            .checked_add(overlay_count)
            .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
        let host_span = host_start..host_end;
        let overlay_span = host_end..overlay_end;
        let host_artifact = super::compiler::validated_scroll_host_before_artifact_span_stamp(
            host_before,
            0,
            host_span.clone(),
        )
        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
        let overlay_artifact = super::compiler::validated_scroll_overlay_artifact_span_stamp(
            overlay,
            2,
            overlay_span.clone(),
        )
        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;

        let validated_inner_steps = super::compiler::validate_effect_transform_scroll_inner_steps(
            root.inner_steps.clone(),
            inner.scroll_cutout,
            inner.receiver.owner,
            inner.receiver.id,
        )
        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;

        let mut inner_stamp_steps = Vec::with_capacity(root.inner_steps.len());
        let mut inner_cursor = 0_u32;
        for (step_index, step) in root.inner_steps.iter().enumerate() {
            match step {
                super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact) => {
                    let end = inner_cursor
                        .checked_add(
                            checked_property_scroll_opaque_order_count(artifact)
                                .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?,
                        )
                        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
                    let span = super::compiler::validated_ordered_receiver_artifact_span_stamp(
                        &validated_inner_steps,
                        step_index,
                        inner_cursor..end,
                    )
                    .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
                    inner_stamp_steps
                        .push(super::RetainedSurfaceRasterStepStamp::ArtifactSpan(span));
                    inner_cursor = end;
                }
                super::frame_recorder::RecordedTransformSurfaceStep::Boundary(marker) => {
                    if step_index != inner.insertion_index
                        || *marker != inner.scroll_cutout
                        || inner_cursor != inner.receiver_opaque_before
                    {
                        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
                    }
                    let dependency = super::compiler::TransformScrollBoundaryRasterDependency {
                        step_index,
                        scene_root_ordinal: ordinal,
                        receiver_owner: inner.receiver.owner,
                        receiver_transform_id: inner.receiver.id,
                        receiver_stable_id: inner.receiver_stable_id,
                        scroll_boundary_ordinal: ordinal,
                        boundary_root,
                        boundary_stable_id,
                        content_root,
                        content_stable_id,
                        insertion_index: inner.insertion_index,
                        receiver_step_count: root.inner_steps.len(),
                        before_span: inner.before_span.clone(),
                        after_span: inner.after_span.clone(),
                        recorded_receiver_opaque_before: inner.receiver_opaque_before,
                        recorded_receiver_opaque_after: inner.receiver_opaque_after,
                        host_parent_span: host_span.clone(),
                        content_local_span: 0..prepared_boundary.content_local_terminal,
                        overlay_parent_span: overlay_span.clone(),
                        host_artifact: host_artifact.clone(),
                        overlay_artifact: overlay_artifact.clone(),
                        content_stamps: content_stamps.clone(),
                        scroll,
                        contents_clip,
                        receiver_local_raster_clips: Vec::new(),
                        receiver_ancestor_composite_clips: Vec::new(),
                        same_owner_role: None,
                    };
                    if !super::compiler::transform_scroll_boundary_dependency_is_canonical(
                        &dependency,
                    ) {
                        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
                    }
                    inner_stamp_steps.push(super::RetainedSurfaceRasterStepStamp::ScrollBoundary(
                        dependency,
                    ));
                    inner_cursor = overlay_end;
                }
            }
        }
        let inner_target = RetainedSurfaceRasterInputs {
            color: inner_color_desc.clone(),
            depth: inner_depth_desc,
            scale_factor_bits,
            source_bounds_bits: bounds_bits(insertion.inner_geometry.source_bounds),
        };
        let inner_stamp = super::compiler::validated_transform_scroll_receiver_raster_stamp(
            inner.receiver.owner,
            inner.receiver_stable_id,
            inner_target,
            inner_stamp_steps,
            0..inner_cursor,
        )
        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;

        let child_geometry_stamp =
            super::compiler::retained_surface_composite_geometry_stamp(insertion.inner_geometry)
                .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
        let mut outer_stamp_steps = Vec::with_capacity(root.outer_steps.len());
        let mut outer_cursor = 0_u32;
        for (step_index, step) in root.outer_steps.iter().enumerate() {
            match step {
                super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact) => {
                    let validated = super::compiler::validate_effect_property_surface_artifact(
                        artifact,
                        &insertion.outer_artifact_contract,
                    )
                    .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
                    let end = outer_cursor
                        .checked_add(
                            checked_property_scroll_opaque_order_count(artifact)
                                .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?,
                        )
                        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
                    let span =
                        super::compiler::validated_effect_property_surface_artifact_span_stamp(
                            &validated,
                            step_index,
                            outer_cursor..end,
                        )
                        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
                    outer_stamp_steps
                        .push(super::RetainedSurfaceRasterStepStamp::ArtifactSpan(span));
                    outer_cursor = end;
                }
                super::frame_recorder::RecordedTransformSurfaceStep::Boundary(marker) => {
                    if step_index != insertion.outer_insertion_index
                        || *marker != insertion.transform_cutout
                        || outer_cursor != insertion.outer_opaque_before
                    {
                        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
                    }
                    let dependency = super::compiler::EffectTransformScrollChildRasterDependency {
                        step_index,
                        child_stamp: Box::new(inner_stamp.clone()),
                        child_composite_geometry: child_geometry_stamp.clone(),
                        child_transform: inner.receiver.id,
                        parent_opaque_order_before: outer_cursor,
                        parent_opaque_order_after: outer_cursor,
                    };
                    if !super::compiler::effect_transform_scroll_child_dependency_validates_contract(
                        &dependency,
                        inner.receiver.id,
                        insertion.inner_geometry,
                    ) {
                        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
                    }
                    outer_stamp_steps.push(
                        super::RetainedSurfaceRasterStepStamp::EffectTransformScrollChild(
                            dependency,
                        ),
                    );
                }
            }
        }
        let outer_target = RetainedSurfaceRasterInputs {
            color: outer_color_desc.clone(),
            depth: outer_depth_desc,
            scale_factor_bits,
            source_bounds_bits: insertion.outer_raster_bounds_bits,
        };
        let outer_stamp = super::compiler::validated_effect_transform_scroll_outer_raster_stamp(
            &insertion.outer_artifact_contract,
            inner.receiver.id,
            insertion.inner_geometry,
            outer_target,
            outer_stamp_steps,
            0..outer_cursor,
        )
        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;

        roots.push(RetainedPropertyScrollJointRootStamp {
            ordinal,
            root: insertion.outer_receiver.owner,
            stable_id: insertion.outer_stable_id,
            boundary_span: ordinal
                ..ordinal
                    .checked_add(1)
                    .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?,
        });
        ordered_boundaries.push(global_boundary);
        for stamp in [&outer_stamp, &inner_stamp] {
            generic_bindings.push(RetainedPropertyScrollGenericBindingStamp {
                boundary: global_boundary,
                resident_key: stamp.identity.resident_key(),
                color_key: stamp.identity.color_key,
            });
        }
        groups.push(prepared_boundary.group.clone());
        generic_stamps.push(outer_stamp.clone());
        generic_stamps.push(inner_stamp.clone());
        contracts.push(EffectTransformScrollCompilerContract {
            outer: insertion.outer_artifact_contract.clone(),
            child_transform: inner.receiver.id,
            child_geometry: insertion.inner_geometry,
        });
        prepared_roots.push(PreparedEffectTransformScrollRoot {
            outer_receiver: insertion.outer_receiver,
            outer_stable_id: insertion.outer_stable_id,
            outer_artifact_contract: insertion.outer_artifact_contract.clone(),
            outer_composite: root.outer_composite,
            outer_steps: root.outer_steps,
            outer_stamp,
            outer_color_key,
            outer_color_desc,
            outer_opaque_terminal: outer_cursor,
            validated_inner_steps,
            inner: PreparedTransformScrollRoot {
                receiver: inner.receiver,
                receiver_stable_id: inner.receiver_stable_id,
                geometry: insertion.inner_geometry,
                receiver_steps: root.inner_steps,
                boundary: prepared_boundary,
                receiver_stamp: inner_stamp,
                receiver_color_key: inner_color_key,
                receiver_color_desc: inner_color_desc,
                receiver_opaque_terminal: inner_cursor,
            },
        });
    }

    let scroll_bindings = groups
        .iter()
        .map(|group| RetainedPropertyScrollGroupBindingStamp {
            boundary: group.boundary,
            content_root: group.content_root,
            content_stable_id: group.content_stable_id,
            backing_rank: group.backing_rank(),
            ordered_resident_keys: group.active_resident_keys(),
        })
        .collect();
    let transaction = RetainedPropertyScrollSceneTransaction {
        seal: RetainedPropertyScrollJointSeal {
            roots,
            ordered_boundaries,
            generic_bindings,
            scroll_bindings,
        },
        generic_authority: RetainedPropertyScrollGenericAuthority::EffectTransformScrollCompiler(
            contracts,
        ),
        generic_full_set: generic_stamps,
        scroll_groups: groups,
    };
    if aggregate_resident_pair_bytes > scene.budget.max_active_pair_bytes
        || !transaction.is_canonical()
    {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    }
    let mut actions = viewport
        .freeze_retained_property_scroll_scene_compile_actions_from_pool(&transaction)
        .ok_or(RetainedPropertyScrollScenePrepareError::PoolContract)?;
    let expected_keys = transaction
        .ordered_stamps()
        .into_iter()
        .map(|stamp| stamp.identity.resident_key())
        .collect::<FxHashSet<_>>();
    if actions.len() != expected_keys.len()
        || actions.keys().copied().collect::<FxHashSet<_>>() != expected_keys
    {
        return Err(RetainedPropertyScrollScenePrepareError::PoolContract);
    }
    for root in &prepared_roots {
        let content_requires_raster = root
            .inner
            .boundary
            .group
            .active_resident_keys()
            .iter()
            .any(|key| actions.get(key).copied() == Some(RetainedSurfaceCompileAction::Reraster));
        if content_requires_raster {
            let inner_action = actions
                .get_mut(&root.inner.receiver_stamp.identity.resident_key())
                .ok_or(RetainedPropertyScrollScenePrepareError::PoolContract)?;
            if *inner_action == RetainedSurfaceCompileAction::Reuse {
                *inner_action = RetainedSurfaceCompileAction::Reraster;
            }
        }
    }
    let reraster_count = actions
        .values()
        .filter(|action| **action == RetainedSurfaceCompileAction::Reraster)
        .count();
    let trace = RetainedPropertyScrollSceneBuildTrace {
        root_count: prepared_roots.len(),
        generic_surface_count: prepared_roots.len().saturating_mul(2),
        effect_surface_count: prepared_roots.len(),
        scroll_group_count: prepared_roots.len(),
        backing: if has_tiled {
            ScrollSceneBackingKind::Tiled
        } else {
            ScrollSceneBackingKind::Single
        },
        tile_count,
        reraster_count,
        reuse_count: actions.len() - reraster_count,
        content_pair_bytes: pair_bytes,
    };
    Ok(PreparedRetainedEffectTransformScrollScene {
        viewport,
        graph,
        parent_ctx: ctx,
        roots: prepared_roots,
        actions,
        transaction,
        clear_rgba_bits: clear_rgba.map(f32::to_bits),
        frame_owner,
        trace,
    })
}

/// Emits the exact T->S forest. The root target is cleared once, every H/C/O
/// sequence is rastered inside its receiver target, and the receiver's
/// absolute translation matrix appears only on the final texture composite.
pub(crate) fn prepare_retained_scroll_content_effect_scene_from_pool<'a>(
    viewport: &'a mut Viewport,
    scene: ValidatedScrollContentEffectScene,
    graph: &'a mut FrameGraph,
    ctx: UiBuildContext,
    clear_rgba: [f32; 4],
    frame_owner: RetainedSurfaceFrameStageOwner,
) -> Result<PreparedRetainedScrollContentEffectScene<'a>, RetainedPropertyScrollScenePrepareError> {
    if !scene.is_canonical() {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    }
    validate_parent_target(graph, &ctx)
        .map_err(|_| RetainedPropertyScrollScenePrepareError::ParentTarget)?;
    if ctx.viewport().scale_factor().to_bits() != scene.scale_factor_bits
        || ctx.paint_offset().map(f32::to_bits) != [0.0_f32.to_bits(); 2]
        || ctx.graphics_pass_context().scissor_rect.is_some()
        || ctx.opaque_rect_order() != 0
        || ctx.viewport().target_format() != scene.target_format
        || clear_rgba.iter().any(|component| !component.is_finite())
    {
        return Err(RetainedPropertyScrollScenePrepareError::ContextMismatch);
    }
    if !viewport.retained_property_scroll_scene_stage_is_available()
        || !viewport.retained_surface_frame_stage_owner_is_active(frame_owner)
    {
        return Err(RetainedPropertyScrollScenePrepareError::StageUnavailable);
    }
    let (transaction, frozen) = freeze_scroll_content_effect_transaction(&scene)
        .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
    let graph_keys = graph
        .declared_persistent_texture_keys()
        .collect::<FxHashSet<_>>();
    let mut persistent_keys = FxHashSet::default();
    let mut aggregate_pair_bytes = 0_u64;
    for stamp in transaction.ordered_stamps() {
        let color_key = stamp.identity.color_key;
        let depth_key = color_key
            .depth_stencil()
            .ok_or(RetainedPropertyScrollScenePrepareError::DescriptorPair)?;
        if !persistent_keys.insert(color_key)
            || !persistent_keys.insert(depth_key)
            || graph_keys.contains(&color_key)
            || graph_keys.contains(&depth_key)
        {
            return Err(
                RetainedPropertyScrollScenePrepareError::PersistentKeyAlreadyDeclared(color_key),
            );
        }
        aggregate_pair_bytes = aggregate_pair_bytes
            .checked_add(
                canonical_pair_bytes(&stamp.target.color, &stamp.target.depth)
                    .ok_or(RetainedPropertyScrollScenePrepareError::DescriptorPair)?,
            )
            .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
    }
    if aggregate_pair_bytes > scene.budget.max_active_pair_bytes {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    }
    let mut actions = viewport
        .freeze_retained_property_scroll_scene_compile_actions_from_pool(&transaction)
        .ok_or(RetainedPropertyScrollScenePrepareError::PoolContract)?;
    let expected_keys = transaction
        .ordered_stamps()
        .into_iter()
        .map(|stamp| stamp.identity.resident_key())
        .collect::<FxHashSet<_>>();
    if actions.len() != expected_keys.len()
        || actions.keys().copied().collect::<FxHashSet<_>>() != expected_keys
    {
        return Err(RetainedPropertyScrollScenePrepareError::PoolContract);
    }
    upgrade_scroll_content_effect_actions(&frozen, &mut actions)
        .ok_or(RetainedPropertyScrollScenePrepareError::PoolContract)?;
    let prepared_roots = scene
        .roots
        .into_iter()
        .zip(frozen)
        .map(|(validated, frozen)| {
            let content_geometry =
                PreparedScrollContentCompositeGeometry::from_validated_scroll_content_effect_stamp(
                    &frozen.content_stamp,
                    validated.boundary.scroll,
                    validated.boundary.contents_clip,
                    &validated.insertion.artifact_contract,
                )
                .ok_or(RetainedPropertyScrollScenePrepareError::BoundaryDrift)?;
            Ok(PreparedScrollContentEffectRoot {
                validated,
                frozen,
                content_geometry,
            })
        })
        .collect::<Result<Vec<_>, RetainedPropertyScrollScenePrepareError>>()?;
    let reraster_count = actions
        .values()
        .filter(|action| **action == RetainedSurfaceCompileAction::Reraster)
        .count();
    let trace = RetainedPropertyScrollSceneBuildTrace {
        root_count: prepared_roots.len(),
        generic_surface_count: prepared_roots
            .iter()
            .map(|root| 1 + usize::from(root.frozen.outer_stamp.is_some()))
            .sum(),
        effect_surface_count: prepared_roots.len(),
        scroll_group_count: prepared_roots.len(),
        backing: ScrollSceneBackingKind::Single,
        tile_count: 0,
        reraster_count,
        reuse_count: actions.len() - reraster_count,
        content_pair_bytes: aggregate_pair_bytes,
    };
    Ok(PreparedRetainedScrollContentEffectScene {
        viewport,
        graph,
        parent_ctx: ctx,
        roots: prepared_roots,
        actions,
        transaction,
        clear_rgba_bits: clear_rgba.map(f32::to_bits),
        frame_owner,
        trace,
    })
}

pub(crate) fn emit_prepared_retained_scroll_content_effect_scene(
    prepared: PreparedRetainedScrollContentEffectScene<'_>,
) -> RetainedPropertyScrollSceneBuildOutcome {
    let PreparedRetainedScrollContentEffectScene {
        viewport,
        graph,
        mut parent_ctx,
        roots,
        mut actions,
        transaction,
        clear_rgba_bits,
        frame_owner,
        trace,
    } = prepared;
    assert!(viewport.retained_surface_frame_stage_owner_is_active(frame_owner));
    assert!(viewport.retained_property_scroll_scene_stage_is_available());
    assert_eq!(parent_ctx.opaque_rect_order(), 0);
    let parent_target = parent_ctx
        .current_target()
        .unwrap_or_else(|| parent_ctx.allocate_target(graph));
    parent_ctx.set_current_target(parent_target);
    graph.add_graphics_pass(ClearPass::new(
        crate::view::render_pass::clear_pass::ClearParams::new(clear_rgba_bits.map(f32::from_bits)),
        crate::view::render_pass::clear_pass::ClearInput {
            pass_context: parent_ctx.graphics_pass_context(),
            clear_depth_stencil: true,
        },
        crate::view::render_pass::clear_pass::ClearOutput {
            render_target: parent_target,
        },
    ));
    if let Some(handle) = parent_target.handle() {
        parent_ctx.set_color_target(Some(handle));
    }

    for root in roots {
        let PreparedScrollContentEffectRoot {
            validated,
            frozen,
            content_geometry,
        } = root;
        let effect_action = actions
            .remove(&frozen.effect_stamp.identity.resident_key())
            .expect("prepared Phase3 E action is frozen");
        let content_action = actions
            .remove(&frozen.content_stamp.identity.resident_key())
            .expect("prepared Phase3 C action is frozen");
        assert!(
            effect_action != RetainedSurfaceCompileAction::Reraster
                || content_action == RetainedSurfaceCompileAction::Reraster
        );

        let mut effect_ctx = UiBuildContext::from_parts(
            parent_ctx.viewport(),
            parent_ctx.layer_subtree_state_with_ancestor_clip(AncestorClipContext::default()),
        );
        effect_ctx.set_current_render_transform(None);
        let effect_target = effect_ctx.allocate_persistent_target_with_desc(
            graph,
            frozen.effect_stamp.target.color.clone(),
            frozen.effect_stamp.identity.color_key,
        );
        effect_ctx.set_current_target(effect_target);
        if effect_action == RetainedSurfaceCompileAction::Reraster {
            graph.add_graphics_pass(ClearPass::new(
                crate::view::render_pass::clear_pass::ClearParams::new([0.0; 4]),
                crate::view::render_pass::clear_pass::ClearInput {
                    pass_context: effect_ctx.graphics_pass_context(),
                    clear_depth_stencil: true,
                },
                crate::view::render_pass::clear_pass::ClearOutput {
                    render_target: effect_target,
                },
            ));
            for artifact in &validated.effect_steps {
                let super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact) =
                    artifact
                else {
                    unreachable!("Phase3 E program contains artifacts only")
                };
                let artifact = super::compiler::validate_effect_property_surface_artifact(
                    artifact,
                    &validated.insertion.artifact_contract,
                )
                .expect("prepared Phase3 E artifact remains valid");
                super::compiler::emit_validated_effect_property_surface_artifact(
                    artifact,
                    graph,
                    &mut effect_ctx,
                );
            }
        } else {
            effect_ctx.replay_opaque_rect_order_exact(0, frozen.effect_stamp.opaque_order_span.end);
        }
        assert_eq!(
            effect_ctx.opaque_rect_order(),
            frozen.effect_stamp.opaque_order_span.end
        );
        let effect_state = effect_ctx.into_state();

        let mut content_ctx = UiBuildContext::from_parts(
            parent_ctx.viewport(),
            parent_ctx.layer_subtree_state_with_ancestor_clip(AncestorClipContext::default()),
        );
        content_ctx.set_current_render_transform(None);
        content_ctx.merge_child_target_pairs(&effect_state);
        let content_target = content_ctx.allocate_persistent_target_with_desc(
            graph,
            frozen.content_stamp.target.color.clone(),
            frozen.content_stamp.identity.color_key,
        );
        content_ctx.set_current_target(content_target);
        if content_action == RetainedSurfaceCompileAction::Reraster {
            graph.add_graphics_pass(ClearPass::new(
                crate::view::render_pass::clear_pass::ClearParams::new([0.0; 4]),
                crate::view::render_pass::clear_pass::ClearInput {
                    pass_context: content_ctx.graphics_pass_context(),
                    clear_depth_stencil: true,
                },
                crate::view::render_pass::clear_pass::ClearOutput {
                    render_target: content_target,
                },
            ));
            let effect_cutout = validated.insertion.effect_cutout;
            let effect = validated.insertion.effect;
            let bounds = frozen
                .effect_stamp
                .target
                .source_bounds_bits
                .map(f32::from_bits);
            super::compiler::emit_validated_scroll_content_effect_receiver(
                validated.receiver_program,
                graph,
                &mut content_ctx,
                |marker, graph, ctx| {
                    assert_eq!(marker, effect_cutout);
                    graph.add_graphics_pass(CompositeLayerPass::new(
                        CompositeLayerParams {
                            rect_pos: [bounds[0], bounds[1]],
                            rect_size: [bounds[2], bounds[3]],
                            corner_radii: [0.0; 4],
                            opacity: effect.opacity,
                            scissor_rect: None,
                            clear_target: false,
                        },
                        CompositeLayerInput {
                            layer: LayerIn::with_handle(
                                effect_target
                                    .handle()
                                    .expect("prepared Phase3 E target has a handle"),
                            ),
                            pass_context: ctx.graphics_pass_context(),
                        },
                        CompositeLayerOutput {
                            render_target: content_target,
                        },
                    ));
                },
            );
        } else {
            assert_eq!(effect_action, RetainedSurfaceCompileAction::Reuse);
            content_ctx
                .replay_opaque_rect_order_exact(0, frozen.content_stamp.opaque_order_span.end);
        }
        assert_eq!(
            content_ctx.opaque_rect_order(),
            frozen.content_stamp.opaque_order_span.end
        );
        let content_state = content_ctx.into_state();
        let emit_content = |graph: &mut FrameGraph, ctx: &mut UiBuildContext, target| {
            ctx.set_current_target(target);
            graph.add_graphics_pass(
                content_geometry.into_texture_composite_pass(
                    TextureCompositeInput::from_render_target(
                        TextureCompositeSourceIn::with_handle(
                            content_target
                                .handle()
                                .expect("prepared Phase3 C target has a handle"),
                        ),
                        Default::default(),
                        ctx.graphics_pass_context(),
                    ),
                    TextureCompositeOutput {
                        render_target: target,
                    },
                ),
            );
        };

        match (
            frozen.outer_stamp,
            validated.insertion.outer_transform,
            validated.outer_program,
        ) {
            (None, None, None) => {
                parent_ctx.merge_child_target_pairs(&content_state);
                let scroll_marker = validated.scroll_content_marker;
                super::compiler::emit_validated_frame_root_scroll_receiver(
                    validated.scroll_host_program,
                    graph,
                    &mut parent_ctx,
                    |marker, graph, ctx| {
                        assert_eq!(marker, scroll_marker);
                        emit_content(graph, ctx, parent_target);
                    },
                );
            }
            (Some(outer_stamp), Some(outer), Some(outer_program)) => {
                let outer_action = actions
                    .remove(&outer_stamp.identity.resident_key())
                    .expect("prepared Phase3 T action is frozen");
                assert!(
                    content_action != RetainedSurfaceCompileAction::Reraster
                        || outer_action == RetainedSurfaceCompileAction::Reraster
                );
                let mut outer_ctx = UiBuildContext::from_parts(
                    parent_ctx.viewport(),
                    parent_ctx
                        .layer_subtree_state_with_ancestor_clip(AncestorClipContext::default()),
                );
                outer_ctx.set_current_render_transform(None);
                outer_ctx.merge_child_target_pairs(&content_state);
                let outer_target = outer_ctx.allocate_persistent_target_with_desc(
                    graph,
                    outer_stamp.target.color.clone(),
                    outer_stamp.identity.color_key,
                );
                outer_ctx.set_current_target(outer_target);
                if outer_action == RetainedSurfaceCompileAction::Reraster {
                    graph.add_graphics_pass(ClearPass::new(
                        crate::view::render_pass::clear_pass::ClearParams::new([0.0; 4]),
                        crate::view::render_pass::clear_pass::ClearInput {
                            pass_context: outer_ctx.graphics_pass_context(),
                            clear_depth_stencil: true,
                        },
                        crate::view::render_pass::clear_pass::ClearOutput {
                            render_target: outer_target,
                        },
                    ));
                    let scroll_cutout = outer.receiver.scroll_cutout;
                    let scroll_marker = validated.scroll_content_marker;
                    super::compiler::emit_validated_frame_root_scroll_receiver(
                        outer_program,
                        graph,
                        &mut outer_ctx,
                        |marker, graph, ctx| {
                            assert_eq!(marker, scroll_cutout);
                            super::compiler::emit_validated_frame_root_scroll_receiver(
                                validated.scroll_host_program.clone(),
                                graph,
                                ctx,
                                |marker, graph, ctx| {
                                    assert_eq!(marker, scroll_marker);
                                    emit_content(graph, ctx, outer_target);
                                },
                            );
                        },
                    );
                } else {
                    assert_eq!(content_action, RetainedSurfaceCompileAction::Reuse);
                    assert_eq!(effect_action, RetainedSurfaceCompileAction::Reuse);
                    outer_ctx.replay_opaque_rect_order_exact(0, outer_stamp.opaque_order_span.end);
                }
                assert_eq!(
                    outer_ctx.opaque_rect_order(),
                    outer_stamp.opaque_order_span.end
                );
                let outer_state = outer_ctx.into_state();
                let parent_before = parent_ctx.opaque_rect_order();
                let parent_after = parent_before.max(outer_stamp.opaque_order_span.end);
                parent_ctx.merge_child_render_state_exact(
                    &outer_state,
                    parent_before,
                    outer_stamp.opaque_order_span.end,
                    parent_after,
                );
                parent_ctx.set_current_target(parent_target);
                graph.add_graphics_pass(TextureCompositePass::new(
                    outer.geometry.texture_composite_params(),
                    TextureCompositeInput::from_render_target(
                        TextureCompositeSourceIn::with_handle(
                            outer_target
                                .handle()
                                .expect("prepared Phase3 T target has a handle"),
                        ),
                        Default::default(),
                        parent_ctx.graphics_pass_context(),
                    ),
                    TextureCompositeOutput {
                        render_target: parent_target,
                    },
                ));
            }
            _ => unreachable!("prepared Phase3 outer-T authority is atomic"),
        }
    }
    assert!(actions.is_empty());
    assert!(viewport.stage_retained_property_scroll_scene(transaction));
    RetainedPropertyScrollSceneBuildOutcome {
        state: parent_ctx.into_state(),
        trace,
    }
}

/// Joint prepare for the DAG-selected exact grammar. The match consumes the
/// graph-inert compiler token before delegating to one existing pool freezer;
/// therefore no variant can partially prepare and retry as another grammar.
#[allow(dead_code)]
pub(crate) fn prepare_property_boundary_dag_scene_from_pool<'a>(
    viewport: &'a mut Viewport,
    scene: ValidatedPropertyBoundaryDagScene,
    graph: &'a mut FrameGraph,
    ctx: UiBuildContext,
    clear_rgba: [f32; 4],
    frame_owner: RetainedSurfaceFrameStageOwner,
) -> Result<PreparedPropertyBoundaryDagScene<'a>, RetainedPropertyScrollScenePrepareError> {
    if !scene.is_canonical() {
        return Err(RetainedPropertyScrollScenePrepareError::BoundaryDrift);
    }
    match scene {
        ValidatedPropertyBoundaryDagScene::FrameRootScroll(scene) => {
            prepare_frame_root_scroll_scene(viewport, scene, graph, ctx, clear_rgba, frame_owner)
                .map(PreparedPropertyBoundaryDagScene::FrameRootScroll)
        }
        ValidatedPropertyBoundaryDagScene::TransformScroll(scene) => {
            prepare_retained_transform_scroll_scene_from_pool(
                viewport,
                scene,
                graph,
                ctx,
                clear_rgba,
                frame_owner,
            )
            .map(PreparedPropertyBoundaryDagScene::TransformScroll)
        }
        ValidatedPropertyBoundaryDagScene::EffectScroll(scene) => {
            prepare_retained_effect_scroll_scene_from_pool(
                viewport,
                scene,
                graph,
                ctx,
                clear_rgba,
                frame_owner,
            )
            .map(PreparedPropertyBoundaryDagScene::EffectScroll)
        }
        ValidatedPropertyBoundaryDagScene::TransformEffectScroll(scene) => {
            prepare_retained_transform_effect_scroll_scene_from_pool(
                viewport,
                scene,
                graph,
                ctx,
                clear_rgba,
                frame_owner,
            )
            .map(PreparedPropertyBoundaryDagScene::TransformEffectScroll)
        }
        ValidatedPropertyBoundaryDagScene::EffectTransformScroll(scene) => {
            prepare_retained_effect_transform_scroll_scene_from_pool(
                viewport,
                scene,
                graph,
                ctx,
                clear_rgba,
                frame_owner,
            )
            .map(PreparedPropertyBoundaryDagScene::EffectTransformScroll)
        }
        ValidatedPropertyBoundaryDagScene::ScrollEffect(scene) => {
            prepare_retained_scroll_content_effect_scene_from_pool(
                viewport,
                scene,
                graph,
                ctx,
                clear_rgba,
                frame_owner,
            )
            .map(PreparedPropertyBoundaryDagScene::ScrollEffect)
        }
        ValidatedPropertyBoundaryDagScene::TransformScrollEffect(scene) => {
            prepare_retained_scroll_content_effect_scene_from_pool(
                viewport,
                scene,
                graph,
                ctx,
                clear_rgba,
                frame_owner,
            )
            .map(PreparedPropertyBoundaryDagScene::TransformScrollEffect)
        }
    }
}

pub(crate) fn emit_prepared_retained_transform_scroll_scene(
    prepared: PreparedRetainedTransformScrollScene<'_>,
) -> RetainedPropertyScrollSceneBuildOutcome {
    let PreparedRetainedTransformScrollScene {
        viewport,
        graph,
        mut parent_ctx,
        roots,
        mut actions,
        transaction,
        clear_rgba_bits,
        frame_owner,
        trace,
    } = prepared;
    assert!(viewport.retained_surface_frame_stage_owner_is_active(frame_owner));
    assert!(viewport.retained_property_scroll_scene_stage_is_available());
    assert_eq!(parent_ctx.opaque_rect_order(), 0);
    let parent_target = parent_ctx
        .current_target()
        .unwrap_or_else(|| parent_ctx.allocate_target(graph));
    parent_ctx.set_current_target(parent_target);
    graph.add_graphics_pass(ClearPass::new(
        crate::view::render_pass::clear_pass::ClearParams::new(clear_rgba_bits.map(f32::from_bits)),
        crate::view::render_pass::clear_pass::ClearInput {
            pass_context: parent_ctx.graphics_pass_context(),
            clear_depth_stencil: true,
        },
        crate::view::render_pass::clear_pass::ClearOutput {
            render_target: parent_target,
        },
    ));
    if let Some(handle) = parent_target.handle() {
        parent_ctx.set_color_target(Some(handle));
    }

    for prepared_root in roots {
        let PreparedTransformScrollRoot {
            receiver,
            receiver_stable_id,
            geometry,
            receiver_steps,
            boundary,
            receiver_stamp,
            receiver_color_key,
            receiver_color_desc,
            receiver_opaque_terminal,
        } = prepared_root;
        assert_eq!(receiver.id.0, receiver_stamp.identity.boundary_root);
        assert_eq!(receiver_stable_id, receiver_stamp.identity.stable_id);
        assert!(
            super::compiler::transform_scroll_receiver_raster_stamp_is_canonical(&receiver_stamp)
        );
        let receiver_key = receiver_stamp.identity.resident_key();
        let receiver_action = actions
            .remove(&receiver_key)
            .expect("prepared receiver action is frozen");
        let content_keys = boundary.group.active_resident_keys();
        let content_requires_raster = content_keys
            .iter()
            .any(|key| actions.get(key).copied() == Some(RetainedSurfaceCompileAction::Reraster));
        assert!(
            receiver_action == RetainedSurfaceCompileAction::Reraster || !content_requires_raster,
            "mixed receiver/content actions are normalized before emission"
        );
        let receiver_requires_raster = receiver_action == RetainedSurfaceCompileAction::Reraster;

        let mut receiver_ctx = UiBuildContext::from_parts(
            parent_ctx.viewport(),
            parent_ctx.layer_subtree_state_with_ancestor_clip(AncestorClipContext::default()),
        );
        // Raster geometry stays in the pre-transform logical basis.
        receiver_ctx.set_current_render_transform(parent_ctx.current_render_transform());
        let receiver_target = receiver_ctx.allocate_persistent_target_with_desc(
            graph,
            receiver_color_desc,
            receiver_color_key,
        );
        receiver_ctx.set_current_target(receiver_target);
        if receiver_requires_raster {
            graph.add_graphics_pass(ClearPass::new(
                crate::view::render_pass::clear_pass::ClearParams::new([0.0; 4]),
                crate::view::render_pass::clear_pass::ClearInput {
                    pass_context: receiver_ctx.graphics_pass_context(),
                    clear_depth_stencil: true,
                },
                crate::view::render_pass::clear_pass::ClearOutput {
                    render_target: receiver_target,
                },
            ));
            let mut boundary = Some(boundary);
            for (recorded, stamped) in receiver_steps
                .into_iter()
                .zip(receiver_stamp.ordered_steps.iter())
            {
                match (recorded, stamped) {
                    (
                        super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact),
                        super::RetainedSurfaceRasterStepStamp::ArtifactSpan(span),
                    ) => {
                        assert_eq!(
                            receiver_ctx.opaque_rect_order(),
                            span.opaque_order_span.start
                        );
                        let validated =
                            super::compiler::validate_transform_property_surface_artifact(
                                &artifact,
                                receiver.owner,
                                receiver.id,
                            )
                            .expect("prepared receiver artifact remains compiler-valid");
                        super::compiler::emit_validated_transform_property_surface_artifact(
                            validated,
                            graph,
                            &mut receiver_ctx,
                        );
                        assert_eq!(receiver_ctx.opaque_rect_order(), span.opaque_order_span.end);
                    }
                    (
                        super::frame_recorder::RecordedTransformSurfaceStep::Boundary(marker),
                        super::RetainedSurfaceRasterStepStamp::ScrollBoundary(dependency),
                    ) => {
                        assert_eq!(marker, dependency.scroll_cutout());
                        let prepared_boundary = boundary
                            .take()
                            .expect("prepared receiver owns exactly one scroll boundary");
                        emit_prepared_property_scroll_boundary_parts(
                            prepared_boundary,
                            graph,
                            &mut receiver_ctx,
                            receiver_target,
                            &mut actions,
                        );
                    }
                    _ => panic!("prepared T->S receiver step grammar drifted"),
                }
            }
            assert!(boundary.is_none());
            assert_eq!(receiver_ctx.opaque_rect_order(), receiver_opaque_terminal);
        } else {
            for key in content_keys {
                assert_eq!(
                    actions.remove(&key),
                    Some(RetainedSurfaceCompileAction::Reuse),
                    "receiver reuse requires every detached content resident"
                );
            }
            receiver_ctx.replay_opaque_rect_order_exact(0, receiver_opaque_terminal);
        }
        let receiver_state = receiver_ctx.into_state();
        let parent_before = parent_ctx.opaque_rect_order();
        let parent_after = parent_before.max(receiver_opaque_terminal);
        parent_ctx.merge_child_render_state_exact(
            &receiver_state,
            parent_before,
            receiver_opaque_terminal,
            parent_after,
        );
        parent_ctx.set_current_target(parent_target);
        graph.add_graphics_pass(TextureCompositePass::new(
            geometry.texture_composite_params(),
            TextureCompositeInput::from_render_target(
                TextureCompositeSourceIn::with_handle(
                    receiver_target
                        .handle()
                        .expect("prepared T->S receiver target has a handle"),
                ),
                Default::default(),
                parent_ctx.graphics_pass_context(),
            ),
            TextureCompositeOutput {
                render_target: parent_target,
            },
        ));
    }
    assert!(actions.is_empty());
    assert!(
        viewport.stage_retained_property_scroll_scene(transaction),
        "prepared T->S scene stages its joint transaction exactly once"
    );
    RetainedPropertyScrollSceneBuildOutcome {
        state: parent_ctx.into_state(),
        trace,
    }
}

/// Emits an exact direct E->S forest. The root target is cleared once; H/C/O
/// are rastered in the effect receiver target; the receiver's own opacity is
/// applied exactly once by the final composite and never enters raster reuse.
pub(crate) fn emit_prepared_retained_effect_scroll_scene(
    prepared: PreparedRetainedEffectScrollScene<'_>,
) -> RetainedPropertyScrollSceneBuildOutcome {
    let PreparedRetainedEffectScrollScene {
        viewport,
        graph,
        mut parent_ctx,
        roots,
        mut actions,
        transaction,
        clear_rgba_bits,
        frame_owner,
        trace,
    } = prepared;
    assert!(viewport.retained_surface_frame_stage_owner_is_active(frame_owner));
    assert!(viewport.retained_property_scroll_scene_stage_is_available());
    assert_eq!(parent_ctx.opaque_rect_order(), 0);
    let parent_target = parent_ctx
        .current_target()
        .unwrap_or_else(|| parent_ctx.allocate_target(graph));
    parent_ctx.set_current_target(parent_target);
    graph.add_graphics_pass(ClearPass::new(
        crate::view::render_pass::clear_pass::ClearParams::new(clear_rgba_bits.map(f32::from_bits)),
        crate::view::render_pass::clear_pass::ClearInput {
            pass_context: parent_ctx.graphics_pass_context(),
            clear_depth_stencil: true,
        },
        crate::view::render_pass::clear_pass::ClearOutput {
            render_target: parent_target,
        },
    ));
    if let Some(handle) = parent_target.handle() {
        parent_ctx.set_color_target(Some(handle));
    }

    for prepared_root in roots {
        let PreparedEffectScrollRoot {
            receiver,
            receiver_stable_id,
            artifact_contract,
            composite,
            receiver_steps,
            boundary,
            receiver_stamp,
            receiver_color_key,
            receiver_color_desc,
            receiver_opaque_terminal,
        } = prepared_root;
        assert_eq!(receiver.owner, receiver_stamp.identity.boundary_root);
        assert_eq!(receiver_stable_id, receiver_stamp.identity.stable_id);
        assert!(composite.matches_receiver(receiver));
        assert_eq!(
            composite.source_bounds_bits,
            receiver_stamp.target.source_bounds_bits
        );
        assert!(
            super::compiler::effect_scroll_receiver_raster_stamp_validates_contract(
                &receiver_stamp,
                &artifact_contract,
            )
        );
        let receiver_key = receiver_stamp.identity.resident_key();
        let receiver_action = actions
            .remove(&receiver_key)
            .expect("prepared effect receiver action is frozen");
        let content_keys = boundary.group.active_resident_keys();
        let content_requires_raster = content_keys
            .iter()
            .any(|key| actions.get(key).copied() == Some(RetainedSurfaceCompileAction::Reraster));
        assert!(
            receiver_action == RetainedSurfaceCompileAction::Reraster || !content_requires_raster,
            "mixed effect receiver/content actions are normalized before emission"
        );
        let receiver_requires_raster = receiver_action == RetainedSurfaceCompileAction::Reraster;

        let mut receiver_ctx = UiBuildContext::from_parts(
            parent_ctx.viewport(),
            parent_ctx.layer_subtree_state_with_ancestor_clip(AncestorClipContext::default()),
        );
        receiver_ctx.set_current_render_transform(parent_ctx.current_render_transform());
        let receiver_target = receiver_ctx.allocate_persistent_target_with_desc(
            graph,
            receiver_color_desc,
            receiver_color_key,
        );
        receiver_ctx.set_current_target(receiver_target);
        if receiver_requires_raster {
            graph.add_graphics_pass(ClearPass::new(
                crate::view::render_pass::clear_pass::ClearParams::new([0.0; 4]),
                crate::view::render_pass::clear_pass::ClearInput {
                    pass_context: receiver_ctx.graphics_pass_context(),
                    clear_depth_stencil: true,
                },
                crate::view::render_pass::clear_pass::ClearOutput {
                    render_target: receiver_target,
                },
            ));
            let mut boundary = Some(boundary);
            for (recorded, stamped) in receiver_steps
                .into_iter()
                .zip(receiver_stamp.ordered_steps.iter())
            {
                match (recorded, stamped) {
                    (
                        super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact),
                        super::RetainedSurfaceRasterStepStamp::ArtifactSpan(span),
                    ) => {
                        assert_eq!(
                            receiver_ctx.opaque_rect_order(),
                            span.opaque_order_span.start
                        );
                        let validated = super::compiler::validate_effect_property_surface_artifact(
                            &artifact,
                            &artifact_contract,
                        )
                        .expect("prepared effect receiver artifact remains compiler-valid");
                        super::compiler::emit_validated_effect_property_surface_artifact(
                            validated,
                            graph,
                            &mut receiver_ctx,
                        );
                        assert_eq!(receiver_ctx.opaque_rect_order(), span.opaque_order_span.end);
                    }
                    (
                        super::frame_recorder::RecordedTransformSurfaceStep::Boundary(marker),
                        super::RetainedSurfaceRasterStepStamp::EffectScrollBoundary(dependency),
                    ) => {
                        assert_eq!(marker, dependency.scroll_cutout());
                        let prepared_boundary = boundary
                            .take()
                            .expect("prepared effect receiver owns exactly one scroll boundary");
                        emit_prepared_property_scroll_boundary_parts(
                            prepared_boundary,
                            graph,
                            &mut receiver_ctx,
                            receiver_target,
                            &mut actions,
                        );
                    }
                    _ => panic!("prepared E->S receiver step grammar drifted"),
                }
            }
            assert!(boundary.is_none());
            assert_eq!(receiver_ctx.opaque_rect_order(), receiver_opaque_terminal);
        } else {
            for key in content_keys {
                assert_eq!(
                    actions.remove(&key),
                    Some(RetainedSurfaceCompileAction::Reuse),
                    "effect receiver reuse requires every detached content resident"
                );
            }
            receiver_ctx.replay_opaque_rect_order_exact(0, receiver_opaque_terminal);
        }
        let receiver_state = receiver_ctx.into_state();
        let parent_before = parent_ctx.opaque_rect_order();
        let parent_after = parent_before.max(receiver_opaque_terminal);
        parent_ctx.merge_child_render_state_exact(
            &receiver_state,
            parent_before,
            receiver_opaque_terminal,
            parent_after,
        );
        parent_ctx.set_current_target(parent_target);
        let bounds = composite.source_bounds_bits.map(f32::from_bits);
        graph.add_graphics_pass(CompositeLayerPass::new(
            CompositeLayerParams {
                rect_pos: [bounds[0], bounds[1]],
                rect_size: [bounds[2], bounds[3]],
                corner_radii: [0.0; 4],
                opacity: f32::from_bits(composite.opacity_bits),
                scissor_rect: None,
                clear_target: false,
            },
            CompositeLayerInput {
                layer: LayerIn::with_handle(
                    receiver_target
                        .handle()
                        .expect("prepared E->S receiver target has a handle"),
                ),
                pass_context: parent_ctx.graphics_pass_context(),
            },
            CompositeLayerOutput {
                render_target: parent_target,
            },
        ));
    }
    assert!(actions.is_empty());
    assert!(
        viewport.stage_retained_property_scroll_scene(transaction),
        "prepared E->S scene stages its joint transaction exactly once"
    );
    RetainedPropertyScrollSceneBuildOutcome {
        state: parent_ctx.into_state(),
        trace,
    }
}

/// Emits H/C/O in E, composites E into T exactly once with the frozen
/// opacity, then composites T into the root exactly once with the frozen
/// translation. The three residents are staged as one transaction only after
/// all prepared work has been emitted.
pub(crate) fn emit_prepared_retained_transform_effect_scroll_scene(
    prepared: PreparedRetainedTransformEffectScrollScene<'_>,
) -> RetainedPropertyScrollSceneBuildOutcome {
    let PreparedRetainedTransformEffectScrollScene {
        viewport,
        graph,
        mut parent_ctx,
        roots,
        mut actions,
        transaction,
        clear_rgba_bits,
        frame_owner,
        trace,
    } = prepared;
    assert!(viewport.retained_surface_frame_stage_owner_is_active(frame_owner));
    assert!(viewport.retained_property_scroll_scene_stage_is_available());
    assert_eq!(parent_ctx.opaque_rect_order(), 0);
    let parent_target = parent_ctx
        .current_target()
        .unwrap_or_else(|| parent_ctx.allocate_target(graph));
    parent_ctx.set_current_target(parent_target);
    graph.add_graphics_pass(ClearPass::new(
        crate::view::render_pass::clear_pass::ClearParams::new(clear_rgba_bits.map(f32::from_bits)),
        crate::view::render_pass::clear_pass::ClearInput {
            pass_context: parent_ctx.graphics_pass_context(),
            clear_depth_stencil: true,
        },
        crate::view::render_pass::clear_pass::ClearOutput {
            render_target: parent_target,
        },
    ));
    if let Some(handle) = parent_target.handle() {
        parent_ctx.set_color_target(Some(handle));
    }

    for root in roots {
        let PreparedTransformEffectScrollRoot {
            outer_receiver,
            outer_stable_id,
            outer_geometry,
            outer_steps,
            outer_stamp,
            outer_color_key,
            outer_color_desc,
            outer_opaque_terminal,
            inner,
        } = root;
        let PreparedEffectScrollRoot {
            receiver: inner_receiver,
            receiver_stable_id: inner_stable_id,
            artifact_contract,
            composite,
            receiver_steps: inner_steps,
            boundary,
            receiver_stamp: inner_stamp,
            receiver_color_key: inner_color_key,
            receiver_color_desc: inner_color_desc,
            receiver_opaque_terminal: inner_opaque_terminal,
        } = inner;
        assert!(
            super::compiler::transform_effect_scroll_outer_raster_stamp_validates_contract(
                &outer_stamp,
                outer_receiver.id,
                &artifact_contract,
            )
        );
        assert!(
            super::compiler::effect_scroll_receiver_raster_stamp_validates_contract(
                &inner_stamp,
                &artifact_contract,
            )
        );
        assert_eq!(outer_stable_id, outer_stamp.identity.stable_id);
        assert_eq!(inner_stable_id, inner_stamp.identity.stable_id);

        let outer_action = actions
            .remove(&outer_stamp.identity.resident_key())
            .expect("prepared outer T action is frozen");
        let inner_action = actions
            .remove(&inner_stamp.identity.resident_key())
            .expect("prepared inner E action is frozen");
        let content_keys = boundary.group.active_resident_keys();
        let content_requires_raster = content_keys
            .iter()
            .any(|key| actions.get(key).copied() == Some(RetainedSurfaceCompileAction::Reraster));
        assert!(inner_action == RetainedSurfaceCompileAction::Reraster || !content_requires_raster);

        let mut inner_ctx = UiBuildContext::from_parts(
            parent_ctx.viewport(),
            parent_ctx.layer_subtree_state_with_ancestor_clip(AncestorClipContext::default()),
        );
        inner_ctx.set_current_render_transform(parent_ctx.current_render_transform());
        let inner_target = inner_ctx.allocate_persistent_target_with_desc(
            graph,
            inner_color_desc,
            inner_color_key,
        );
        inner_ctx.set_current_target(inner_target);
        if inner_action == RetainedSurfaceCompileAction::Reraster {
            graph.add_graphics_pass(ClearPass::new(
                crate::view::render_pass::clear_pass::ClearParams::new([0.0; 4]),
                crate::view::render_pass::clear_pass::ClearInput {
                    pass_context: inner_ctx.graphics_pass_context(),
                    clear_depth_stencil: true,
                },
                crate::view::render_pass::clear_pass::ClearOutput {
                    render_target: inner_target,
                },
            ));
            let mut boundary = Some(boundary);
            for (recorded, stamped) in inner_steps
                .into_iter()
                .zip(inner_stamp.ordered_steps.iter())
            {
                match (recorded, stamped) {
                    (
                        super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact),
                        super::RetainedSurfaceRasterStepStamp::ArtifactSpan(span),
                    ) => {
                        assert_eq!(inner_ctx.opaque_rect_order(), span.opaque_order_span.start);
                        let validated = super::compiler::validate_effect_property_surface_artifact(
                            &artifact,
                            &artifact_contract,
                        )
                        .expect("prepared inner E artifact remains valid");
                        super::compiler::emit_validated_effect_property_surface_artifact(
                            validated,
                            graph,
                            &mut inner_ctx,
                        );
                    }
                    (
                        super::frame_recorder::RecordedTransformSurfaceStep::Boundary(marker),
                        super::RetainedSurfaceRasterStepStamp::EffectScrollBoundary(dependency),
                    ) => {
                        assert_eq!(marker, dependency.scroll_cutout());
                        emit_prepared_property_scroll_boundary_parts(
                            boundary.take().expect("one inner scroll boundary"),
                            graph,
                            &mut inner_ctx,
                            inner_target,
                            &mut actions,
                        );
                    }
                    _ => panic!("prepared inner E->S grammar drifted"),
                }
            }
            assert!(boundary.is_none());
            assert_eq!(inner_ctx.opaque_rect_order(), inner_opaque_terminal);
        } else {
            for key in content_keys {
                assert_eq!(
                    actions.remove(&key),
                    Some(RetainedSurfaceCompileAction::Reuse)
                );
            }
            inner_ctx.replay_opaque_rect_order_exact(0, inner_opaque_terminal);
        }
        let inner_state = inner_ctx.into_state();

        let mut outer_ctx = UiBuildContext::from_parts(
            parent_ctx.viewport(),
            parent_ctx.layer_subtree_state_with_ancestor_clip(AncestorClipContext::default()),
        );
        outer_ctx.set_current_render_transform(parent_ctx.current_render_transform());
        outer_ctx.merge_child_target_pairs(&inner_state);
        let outer_target = outer_ctx.allocate_persistent_target_with_desc(
            graph,
            outer_color_desc,
            outer_color_key,
        );
        outer_ctx.set_current_target(outer_target);
        if outer_action == RetainedSurfaceCompileAction::Reraster {
            graph.add_graphics_pass(ClearPass::new(
                crate::view::render_pass::clear_pass::ClearParams::new([0.0; 4]),
                crate::view::render_pass::clear_pass::ClearInput {
                    pass_context: outer_ctx.graphics_pass_context(),
                    clear_depth_stencil: true,
                },
                crate::view::render_pass::clear_pass::ClearOutput {
                    render_target: outer_target,
                },
            ));
            for (recorded, stamped) in outer_steps
                .into_iter()
                .zip(outer_stamp.ordered_steps.iter())
            {
                match (recorded, stamped) {
                    (
                        super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact),
                        super::RetainedSurfaceRasterStepStamp::ArtifactSpan(span),
                    ) => {
                        assert_eq!(outer_ctx.opaque_rect_order(), span.opaque_order_span.start);
                        let validated =
                            super::compiler::validate_transform_property_surface_artifact(
                                &artifact,
                                outer_receiver.owner,
                                outer_receiver.id,
                            )
                            .expect("prepared outer T artifact remains valid");
                        super::compiler::emit_validated_transform_property_surface_artifact(
                            validated,
                            graph,
                            &mut outer_ctx,
                        );
                    }
                    (
                        super::frame_recorder::RecordedTransformSurfaceStep::Boundary(marker),
                        super::RetainedSurfaceRasterStepStamp::TransformEffectScrollChild(
                            dependency,
                        ),
                    ) => {
                        assert_eq!(marker.root, inner_receiver.owner);
                        assert_eq!(dependency.child_stamp.as_ref(), &inner_stamp);
                        assert_eq!(
                            outer_ctx.opaque_rect_order(),
                            dependency.parent_opaque_order_before
                        );
                        let bounds = composite.source_bounds_bits.map(f32::from_bits);
                        graph.add_graphics_pass(CompositeLayerPass::new(
                            CompositeLayerParams {
                                rect_pos: [bounds[0], bounds[1]],
                                rect_size: [bounds[2], bounds[3]],
                                corner_radii: [0.0; 4],
                                opacity: f32::from_bits(composite.opacity_bits),
                                scissor_rect: None,
                                clear_target: false,
                            },
                            CompositeLayerInput {
                                layer: LayerIn::with_handle(
                                    inner_target
                                        .handle()
                                        .expect("prepared inner E target has a handle"),
                                ),
                                pass_context: outer_ctx.graphics_pass_context(),
                            },
                            CompositeLayerOutput {
                                render_target: outer_target,
                            },
                        ));
                        assert_eq!(
                            outer_ctx.opaque_rect_order(),
                            dependency.parent_opaque_order_after
                        );
                    }
                    _ => panic!("prepared outer T->E grammar drifted"),
                }
            }
            assert_eq!(outer_ctx.opaque_rect_order(), outer_opaque_terminal);
        } else {
            outer_ctx.replay_opaque_rect_order_exact(0, outer_opaque_terminal);
        }
        let outer_state = outer_ctx.into_state();
        let parent_before = parent_ctx.opaque_rect_order();
        let parent_after = parent_before.max(outer_opaque_terminal);
        parent_ctx.merge_child_render_state_exact(
            &outer_state,
            parent_before,
            outer_opaque_terminal,
            parent_after,
        );
        parent_ctx.set_current_target(parent_target);
        graph.add_graphics_pass(TextureCompositePass::new(
            outer_geometry.texture_composite_params(),
            TextureCompositeInput::from_render_target(
                TextureCompositeSourceIn::with_handle(
                    outer_target
                        .handle()
                        .expect("prepared outer T target has a handle"),
                ),
                Default::default(),
                parent_ctx.graphics_pass_context(),
            ),
            TextureCompositeOutput {
                render_target: parent_target,
            },
        ));
    }
    assert!(actions.is_empty());
    assert!(viewport.stage_retained_property_scroll_scene(transaction));
    RetainedPropertyScrollSceneBuildOutcome {
        state: parent_ctx.into_state(),
        trace,
    }
}

/// Emits H/C/O into T, composites the frozen transform texture into E, then
/// applies E opacity exactly once at the frame edge. Pool staging occurs only
/// after the complete prepared scene has been emitted.
pub(crate) fn emit_prepared_retained_effect_transform_scroll_scene(
    prepared: PreparedRetainedEffectTransformScrollScene<'_>,
) -> RetainedPropertyScrollSceneBuildOutcome {
    let PreparedRetainedEffectTransformScrollScene {
        viewport,
        graph,
        mut parent_ctx,
        roots,
        mut actions,
        transaction,
        clear_rgba_bits,
        frame_owner,
        trace,
    } = prepared;
    assert!(viewport.retained_surface_frame_stage_owner_is_active(frame_owner));
    assert!(viewport.retained_property_scroll_scene_stage_is_available());
    assert_eq!(parent_ctx.opaque_rect_order(), 0);
    let parent_target = parent_ctx
        .current_target()
        .unwrap_or_else(|| parent_ctx.allocate_target(graph));
    parent_ctx.set_current_target(parent_target);
    graph.add_graphics_pass(ClearPass::new(
        crate::view::render_pass::clear_pass::ClearParams::new(clear_rgba_bits.map(f32::from_bits)),
        crate::view::render_pass::clear_pass::ClearInput {
            pass_context: parent_ctx.graphics_pass_context(),
            clear_depth_stencil: true,
        },
        crate::view::render_pass::clear_pass::ClearOutput {
            render_target: parent_target,
        },
    ));
    if let Some(handle) = parent_target.handle() {
        parent_ctx.set_color_target(Some(handle));
    }

    for root in roots {
        let PreparedEffectTransformScrollRoot {
            outer_receiver,
            outer_stable_id,
            outer_artifact_contract,
            outer_composite,
            outer_steps,
            outer_stamp,
            outer_color_key,
            outer_color_desc,
            outer_opaque_terminal,
            validated_inner_steps,
            inner,
        } = root;
        let PreparedTransformScrollRoot {
            receiver: inner_receiver,
            receiver_stable_id: inner_stable_id,
            geometry: inner_geometry,
            receiver_steps: _,
            boundary,
            receiver_stamp: inner_stamp,
            receiver_color_key: inner_color_key,
            receiver_color_desc: inner_color_desc,
            receiver_opaque_terminal: inner_opaque_terminal,
        } = inner;
        assert!(outer_composite.matches_receiver(outer_receiver));
        assert_eq!(outer_stable_id, outer_stamp.identity.stable_id);
        assert_eq!(inner_stable_id, inner_stamp.identity.stable_id);
        assert!(super::compiler::transform_scroll_receiver_raster_stamp_is_canonical(&inner_stamp));
        assert!(
            super::compiler::effect_transform_scroll_outer_raster_stamp_validates_contract(
                &outer_stamp,
                &outer_artifact_contract,
                inner_receiver.id,
                inner_geometry,
            )
        );

        let outer_action = actions
            .remove(&outer_stamp.identity.resident_key())
            .expect("prepared outer E action is frozen");
        let inner_action = actions
            .remove(&inner_stamp.identity.resident_key())
            .expect("prepared inner T action is frozen");
        let content_keys = boundary.group.active_resident_keys();
        let content_requires_raster = content_keys
            .iter()
            .any(|key| actions.get(key).copied() == Some(RetainedSurfaceCompileAction::Reraster));
        assert!(inner_action == RetainedSurfaceCompileAction::Reraster || !content_requires_raster);

        let mut inner_ctx = UiBuildContext::from_parts(
            parent_ctx.viewport(),
            parent_ctx.layer_subtree_state_with_ancestor_clip(AncestorClipContext::default()),
        );
        inner_ctx.set_current_render_transform(parent_ctx.current_render_transform());
        let inner_target = inner_ctx.allocate_persistent_target_with_desc(
            graph,
            inner_color_desc,
            inner_color_key,
        );
        inner_ctx.set_current_target(inner_target);
        if inner_action == RetainedSurfaceCompileAction::Reraster {
            graph.add_graphics_pass(ClearPass::new(
                crate::view::render_pass::clear_pass::ClearParams::new([0.0; 4]),
                crate::view::render_pass::clear_pass::ClearInput {
                    pass_context: inner_ctx.graphics_pass_context(),
                    clear_depth_stencil: true,
                },
                crate::view::render_pass::clear_pass::ClearOutput {
                    render_target: inner_target,
                },
            ));
            let mut boundary = Some(boundary);
            let expected_cutout = inner_stamp
                .ordered_steps
                .iter()
                .find_map(|step| match step {
                    super::RetainedSurfaceRasterStepStamp::ScrollBoundary(dependency) => {
                        Some(dependency.scroll_cutout())
                    }
                    _ => None,
                })
                .expect("prepared inner T owns one scroll boundary");
            super::compiler::emit_validated_frame_root_scroll_receiver(
                validated_inner_steps,
                graph,
                &mut inner_ctx,
                |marker, graph, inner_ctx| {
                    assert_eq!(marker, expected_cutout);
                    emit_prepared_property_scroll_boundary_parts(
                        boundary.take().expect("one inner scroll boundary"),
                        graph,
                        inner_ctx,
                        inner_target,
                        &mut actions,
                    );
                },
            );
            assert!(boundary.is_none());
            assert_eq!(inner_ctx.opaque_rect_order(), inner_opaque_terminal);
        } else {
            for key in content_keys {
                assert_eq!(
                    actions.remove(&key),
                    Some(RetainedSurfaceCompileAction::Reuse)
                );
            }
            inner_ctx.replay_opaque_rect_order_exact(0, inner_opaque_terminal);
        }
        let inner_state = inner_ctx.into_state();

        let mut outer_ctx = UiBuildContext::from_parts(
            parent_ctx.viewport(),
            parent_ctx.layer_subtree_state_with_ancestor_clip(AncestorClipContext::default()),
        );
        outer_ctx.set_current_render_transform(parent_ctx.current_render_transform());
        outer_ctx.merge_child_target_pairs(&inner_state);
        let outer_target = outer_ctx.allocate_persistent_target_with_desc(
            graph,
            outer_color_desc,
            outer_color_key,
        );
        outer_ctx.set_current_target(outer_target);
        if outer_action == RetainedSurfaceCompileAction::Reraster {
            graph.add_graphics_pass(ClearPass::new(
                crate::view::render_pass::clear_pass::ClearParams::new([0.0; 4]),
                crate::view::render_pass::clear_pass::ClearInput {
                    pass_context: outer_ctx.graphics_pass_context(),
                    clear_depth_stencil: true,
                },
                crate::view::render_pass::clear_pass::ClearOutput {
                    render_target: outer_target,
                },
            ));
            for (recorded, stamped) in outer_steps
                .into_iter()
                .zip(outer_stamp.ordered_steps.iter())
            {
                match (recorded, stamped) {
                    (
                        super::frame_recorder::RecordedTransformSurfaceStep::Artifact(artifact),
                        super::RetainedSurfaceRasterStepStamp::ArtifactSpan(span),
                    ) => {
                        assert_eq!(outer_ctx.opaque_rect_order(), span.opaque_order_span.start);
                        let validated = super::compiler::validate_effect_property_surface_artifact(
                            &artifact,
                            &outer_artifact_contract,
                        )
                        .expect("prepared outer E artifact remains valid");
                        super::compiler::emit_validated_effect_property_surface_artifact(
                            validated,
                            graph,
                            &mut outer_ctx,
                        );
                    }
                    (
                        super::frame_recorder::RecordedTransformSurfaceStep::Boundary(marker),
                        super::RetainedSurfaceRasterStepStamp::EffectTransformScrollChild(
                            dependency,
                        ),
                    ) => {
                        assert_eq!(marker.root, inner_receiver.owner);
                        assert_eq!(dependency.child_stamp.as_ref(), &inner_stamp);
                        assert_eq!(
                            outer_ctx.opaque_rect_order(),
                            dependency.parent_opaque_order_before
                        );
                        graph.add_graphics_pass(TextureCompositePass::new(
                            inner_geometry.texture_composite_params(),
                            TextureCompositeInput::from_render_target(
                                TextureCompositeSourceIn::with_handle(
                                    inner_target
                                        .handle()
                                        .expect("prepared inner T target has a handle"),
                                ),
                                Default::default(),
                                outer_ctx.graphics_pass_context(),
                            ),
                            TextureCompositeOutput {
                                render_target: outer_target,
                            },
                        ));
                        assert_eq!(
                            outer_ctx.opaque_rect_order(),
                            dependency.parent_opaque_order_after
                        );
                    }
                    _ => panic!("prepared outer E->T grammar drifted"),
                }
            }
            assert_eq!(outer_ctx.opaque_rect_order(), outer_opaque_terminal);
        } else {
            outer_ctx.replay_opaque_rect_order_exact(0, outer_opaque_terminal);
        }
        let outer_state = outer_ctx.into_state();
        let parent_before = parent_ctx.opaque_rect_order();
        let parent_after = parent_before.max(outer_opaque_terminal);
        parent_ctx.merge_child_render_state_exact(
            &outer_state,
            parent_before,
            outer_opaque_terminal,
            parent_after,
        );
        parent_ctx.set_current_target(parent_target);
        let bounds = outer_composite.source_bounds_bits.map(f32::from_bits);
        graph.add_graphics_pass(CompositeLayerPass::new(
            CompositeLayerParams {
                rect_pos: [bounds[0], bounds[1]],
                rect_size: [bounds[2], bounds[3]],
                corner_radii: [0.0; 4],
                opacity: f32::from_bits(outer_composite.opacity_bits),
                scissor_rect: None,
                clear_target: false,
            },
            CompositeLayerInput {
                layer: LayerIn::with_handle(
                    outer_target
                        .handle()
                        .expect("prepared outer E target has a handle"),
                ),
                pass_context: parent_ctx.graphics_pass_context(),
            },
            CompositeLayerOutput {
                render_target: parent_target,
            },
        ));
    }
    assert!(actions.is_empty());
    assert!(viewport.stage_retained_property_scroll_scene(transaction));
    RetainedPropertyScrollSceneBuildOutcome {
        state: parent_ctx.into_state(),
        trace,
    }
}

fn emit_prepared_property_scroll_boundary_parts(
    prepared: PreparedRetainedPropertyScrollBoundaryParts,
    graph: &mut FrameGraph,
    parent_ctx: &mut UiBuildContext,
    parent_target: RenderTargetOut,
    actions: &mut FxHashMap<super::RetainedSurfaceResidentKey, RetainedSurfaceCompileAction>,
) {
    let PreparedRetainedPropertyScrollBoundaryParts {
        authority,
        backing,
        post_composite,
        host_local_terminal,
        content_local_terminal,
        parent_local_terminal,
        ..
    } = prepared;
    match authority {
        PreparedRetainedPropertyScrollBoundaryAuthority::Existing {
            host_before,
            content,
            overlay,
        } => emit_prepared_existing_property_scroll_boundary_parts(
            host_before,
            content,
            overlay,
            backing,
            post_composite,
            host_local_terminal,
            content_local_terminal,
            parent_local_terminal,
            graph,
            parent_ctx,
            parent_target,
            actions,
        ),
        PreparedRetainedPropertyScrollBoundaryAuthority::AtomicProjectionTextArea { emission } => {
            emit_prepared_atomic_projection_text_area_scroll_boundary_parts(
                emission,
                backing,
                post_composite,
                host_local_terminal,
                content_local_terminal,
                parent_local_terminal,
                graph,
                parent_ctx,
                parent_target,
                actions,
            );
        }
        PreparedRetainedPropertyScrollBoundaryAuthority::AtomicProjectionSelectionTextArea {
            emission,
        } => {
            emit_prepared_atomic_projection_selection_text_area_scroll_boundary_parts(
                emission,
                backing,
                post_composite,
                host_local_terminal,
                content_local_terminal,
                parent_local_terminal,
                graph,
                parent_ctx,
                parent_target,
                actions,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_prepared_existing_property_scroll_boundary_parts(
    host_before: ValidatedScrollSceneHostBeforeArtifact,
    content: ValidatedScrollSceneContentArtifact,
    overlay: ValidatedScrollSceneOverlayArtifact,
    backing: PreparedRetainedPropertyScrollBacking,
    post_composite: PropertyScrollPostCompositeSchedule,
    host_local_terminal: u32,
    content_local_terminal: u32,
    parent_local_terminal: u32,
    graph: &mut FrameGraph,
    parent_ctx: &mut UiBuildContext,
    parent_target: RenderTargetOut,
    actions: &mut FxHashMap<super::RetainedSurfaceResidentKey, RetainedSurfaceCompileAction>,
) {
    let parent_start = parent_ctx.opaque_rect_order();
    let host_end = parent_start
        .checked_add(host_local_terminal)
        .expect("prepared parent cursor cannot overflow");
    let parent_end = parent_start
        .checked_add(parent_local_terminal)
        .expect("prepared parent cursor cannot overflow");
    emit_validated_scroll_scene_host_before_artifact(host_before, graph, parent_ctx);
    assert_eq!(parent_ctx.opaque_rect_order(), host_end);
    match backing {
        PreparedRetainedPropertyScrollBacking::Single {
            stamp,
            color_key,
            color_desc,
            geometry,
            ..
        } => {
            let action = actions
                .remove(&stamp.identity.resident_key())
                .expect("prepared single scroll action is frozen");
            let mut content_ctx = UiBuildContext::from_parts(
                parent_ctx.viewport(),
                parent_ctx.layer_subtree_state_with_ancestor_clip(AncestorClipContext::default()),
            );
            content_ctx.set_current_render_transform(None);
            let content_target =
                content_ctx.allocate_persistent_target_with_desc(graph, color_desc, color_key);
            content_ctx.set_current_target(content_target);
            match action {
                RetainedSurfaceCompileAction::Reraster => {
                    graph.add_graphics_pass(ClearPass::new(
                        crate::view::render_pass::clear_pass::ClearParams::new([0.0; 4]),
                        crate::view::render_pass::clear_pass::ClearInput {
                            pass_context: content_ctx.graphics_pass_context(),
                            clear_depth_stencil: true,
                        },
                        crate::view::render_pass::clear_pass::ClearOutput {
                            render_target: content_target,
                        },
                    ));
                    emit_validated_scroll_scene_content_artifact(&content, graph, &mut content_ctx);
                }
                RetainedSurfaceCompileAction::Reuse => {
                    content_ctx.replay_opaque_rect_order_exact(0, content_local_terminal);
                }
            }
            assert_eq!(content_ctx.opaque_rect_order(), content_local_terminal);
            parent_ctx.merge_child_target_pairs(&content_ctx.into_state());
            parent_ctx.set_current_target(parent_target);
            graph.add_graphics_pass(
                geometry.into_texture_composite_pass(
                    TextureCompositeInput::from_render_target(
                        TextureCompositeSourceIn::with_handle(
                            content_target
                                .handle()
                                .expect("prepared persistent property-scroll target has a handle"),
                        ),
                        Default::default(),
                        parent_ctx.graphics_pass_context(),
                    ),
                    TextureCompositeOutput {
                        render_target: parent_target,
                    },
                ),
            );
        }
        PreparedRetainedPropertyScrollBacking::Tiled { tiles, .. } => {
            for tile in tiles {
                let action = actions
                    .remove(&tile.stamp.identity.resident_key())
                    .expect("prepared tile action is frozen");
                let mut content_ctx = UiBuildContext::from_parts(
                    parent_ctx.viewport(),
                    parent_ctx
                        .layer_subtree_state_with_ancestor_clip(AncestorClipContext::default()),
                );
                content_ctx.set_current_render_transform(None);
                content_ctx.push_scissor_rect(Some(tile.geometry.raster_bounds()));
                let content_target = content_ctx.allocate_persistent_target_with_desc(
                    graph,
                    tile.color_desc,
                    tile.color_key,
                );
                content_ctx.set_current_target(content_target);
                match action {
                    RetainedSurfaceCompileAction::Reraster => {
                        graph.add_graphics_pass(ClearPass::new(
                            crate::view::render_pass::clear_pass::ClearParams::new([0.0; 4]),
                            crate::view::render_pass::clear_pass::ClearInput {
                                pass_context: content_ctx.graphics_pass_context(),
                                clear_depth_stencil: true,
                            },
                            crate::view::render_pass::clear_pass::ClearOutput {
                                render_target: content_target,
                            },
                        ));
                        emit_validated_scroll_scene_content_artifact(
                            &content,
                            graph,
                            &mut content_ctx,
                        );
                    }
                    RetainedSurfaceCompileAction::Reuse => {
                        content_ctx.replay_opaque_rect_order_exact(0, content_local_terminal);
                    }
                }
                assert_eq!(content_ctx.opaque_rect_order(), content_local_terminal);
                parent_ctx.merge_child_target_pairs(&content_ctx.into_state());
                parent_ctx.set_current_target(parent_target);
                graph.add_graphics_pass(
                    tile.geometry.into_texture_composite_pass(
                        TextureCompositeInput::from_render_target(
                            TextureCompositeSourceIn::with_handle(
                                content_target.handle().expect(
                                    "prepared persistent property-scroll tile has a handle",
                                ),
                            ),
                            Default::default(),
                            parent_ctx.graphics_pass_context(),
                        ),
                        TextureCompositeOutput {
                            render_target: parent_target,
                        },
                    ),
                );
            }
        }
    }
    assert_eq!(
        parent_ctx.opaque_rect_order(),
        host_end,
        "detached content cannot advance the scene parent cursor"
    );
    post_composite.emit(graph, parent_ctx);
    emit_validated_scroll_scene_overlay_artifact(overlay, graph, parent_ctx);
    assert_eq!(parent_ctx.opaque_rect_order(), parent_end);
    parent_ctx.set_current_target(parent_target);
}

#[allow(clippy::too_many_arguments)]
fn emit_prepared_atomic_projection_selection_text_area_scroll_boundary_parts(
    emission: ValidatedScrollSceneAtomicProjectionSelectionTextAreaHostEmission,
    backing: PreparedRetainedPropertyScrollBacking,
    post_composite: PropertyScrollPostCompositeSchedule,
    host_local_terminal: u32,
    content_local_terminal: u32,
    parent_local_terminal: u32,
    graph: &mut FrameGraph,
    parent_ctx: &mut UiBuildContext,
    parent_target: RenderTargetOut,
    actions: &mut FxHashMap<super::RetainedSurfaceResidentKey, RetainedSurfaceCompileAction>,
) {
    let PreparedRetainedPropertyScrollBacking::Single {
        stamp,
        color_key,
        color_desc,
        geometry,
        ..
    } = backing
    else {
        unreachable!("prepared atomic projection selection TextArea backing is Single")
    };
    let parent_start = parent_ctx.opaque_rect_order();
    let host_end = parent_start
        .checked_add(host_local_terminal)
        .expect("prepared atomic selection parent cursor cannot overflow");
    let parent_end = parent_start
        .checked_add(parent_local_terminal)
        .expect("prepared atomic selection parent cursor cannot overflow");
    let content_emission = emit_validated_scroll_scene_atomic_projection_selection_text_area_host(
        emission, graph, parent_ctx,
    );
    assert_eq!(parent_ctx.opaque_rect_order(), host_end);

    let action = actions
        .remove(&stamp.identity.resident_key())
        .expect("prepared atomic projection selection TextArea action is frozen");
    let mut content_ctx = UiBuildContext::from_parts(
        parent_ctx.viewport(),
        parent_ctx.layer_subtree_state_with_ancestor_clip(AncestorClipContext::default()),
    );
    content_ctx.set_current_render_transform(None);
    let content_target =
        content_ctx.allocate_persistent_target_with_desc(graph, color_desc, color_key);
    content_ctx.set_current_target(content_target);
    let overlay_emission = match action {
        RetainedSurfaceCompileAction::Reraster => {
            graph.add_graphics_pass(ClearPass::new(
                crate::view::render_pass::clear_pass::ClearParams::new([0.0; 4]),
                crate::view::render_pass::clear_pass::ClearInput {
                    pass_context: content_ctx.graphics_pass_context(),
                    clear_depth_stencil: true,
                },
                crate::view::render_pass::clear_pass::ClearOutput {
                    render_target: content_target,
                },
            ));
            emit_validated_scroll_scene_atomic_projection_selection_text_area_content(
                content_emission,
                graph,
                &mut content_ctx,
            )
        }
        RetainedSurfaceCompileAction::Reuse => {
            content_ctx.replay_opaque_rect_order_exact(0, content_local_terminal);
            reuse_validated_scroll_scene_atomic_projection_selection_text_area_content(
                content_emission,
            )
        }
    };
    assert_eq!(content_ctx.opaque_rect_order(), content_local_terminal);
    parent_ctx.merge_child_target_pairs(&content_ctx.into_state());
    parent_ctx.set_current_target(parent_target);
    graph.add_graphics_pass(
        geometry.into_texture_composite_pass(
            TextureCompositeInput::from_render_target(
                TextureCompositeSourceIn::with_handle(
                    content_target.handle().expect(
                        "prepared atomic projection selection TextArea target has a handle",
                    ),
                ),
                Default::default(),
                parent_ctx.graphics_pass_context(),
            ),
            TextureCompositeOutput {
                render_target: parent_target,
            },
        ),
    );
    assert_eq!(
        parent_ctx.opaque_rect_order(),
        host_end,
        "atomic selection detached content cannot advance the scene parent cursor"
    );
    post_composite.emit(graph, parent_ctx);
    emit_validated_scroll_scene_atomic_projection_selection_text_area_overlay(
        overlay_emission,
        graph,
        parent_ctx,
    );
    assert_eq!(parent_ctx.opaque_rect_order(), parent_end);
    parent_ctx.set_current_target(parent_target);
}

#[allow(clippy::too_many_arguments)]
fn emit_prepared_atomic_projection_text_area_scroll_boundary_parts(
    emission: ValidatedScrollSceneAtomicProjectionTextAreaHostEmission,
    backing: PreparedRetainedPropertyScrollBacking,
    post_composite: PropertyScrollPostCompositeSchedule,
    host_local_terminal: u32,
    content_local_terminal: u32,
    parent_local_terminal: u32,
    graph: &mut FrameGraph,
    parent_ctx: &mut UiBuildContext,
    parent_target: RenderTargetOut,
    actions: &mut FxHashMap<super::RetainedSurfaceResidentKey, RetainedSurfaceCompileAction>,
) {
    let PreparedRetainedPropertyScrollBacking::Single {
        stamp,
        color_key,
        color_desc,
        geometry,
        ..
    } = backing
    else {
        // Only the dedicated C3a prepare constructor can create this enum
        // branch, and that constructor admits exactly one Single backing.
        unreachable!("prepared atomic projection TextArea backing is Single")
    };
    let parent_start = parent_ctx.opaque_rect_order();
    let host_end = parent_start
        .checked_add(host_local_terminal)
        .expect("prepared atomic parent cursor cannot overflow");
    let parent_end = parent_start
        .checked_add(parent_local_terminal)
        .expect("prepared atomic parent cursor cannot overflow");
    let content_emission =
        emit_validated_scroll_scene_atomic_projection_text_area_host(emission, graph, parent_ctx);
    assert_eq!(parent_ctx.opaque_rect_order(), host_end);

    let action = actions
        .remove(&stamp.identity.resident_key())
        .expect("prepared atomic projection TextArea action is frozen");
    let mut content_ctx = UiBuildContext::from_parts(
        parent_ctx.viewport(),
        parent_ctx.layer_subtree_state_with_ancestor_clip(AncestorClipContext::default()),
    );
    content_ctx.set_current_render_transform(None);
    let content_target =
        content_ctx.allocate_persistent_target_with_desc(graph, color_desc, color_key);
    content_ctx.set_current_target(content_target);
    let overlay_emission = match action {
        RetainedSurfaceCompileAction::Reraster => {
            graph.add_graphics_pass(ClearPass::new(
                crate::view::render_pass::clear_pass::ClearParams::new([0.0; 4]),
                crate::view::render_pass::clear_pass::ClearInput {
                    pass_context: content_ctx.graphics_pass_context(),
                    clear_depth_stencil: true,
                },
                crate::view::render_pass::clear_pass::ClearOutput {
                    render_target: content_target,
                },
            ));
            emit_validated_scroll_scene_atomic_projection_text_area_content(
                content_emission,
                graph,
                &mut content_ctx,
            )
        }
        RetainedSurfaceCompileAction::Reuse => {
            content_ctx.replay_opaque_rect_order_exact(0, content_local_terminal);
            reuse_validated_scroll_scene_atomic_projection_text_area_content(content_emission)
        }
    };
    assert_eq!(content_ctx.opaque_rect_order(), content_local_terminal);
    parent_ctx.merge_child_target_pairs(&content_ctx.into_state());
    parent_ctx.set_current_target(parent_target);
    graph.add_graphics_pass(
        geometry.into_texture_composite_pass(
            TextureCompositeInput::from_render_target(
                TextureCompositeSourceIn::with_handle(
                    content_target
                        .handle()
                        .expect("prepared atomic projection TextArea target has a handle"),
                ),
                Default::default(),
                parent_ctx.graphics_pass_context(),
            ),
            TextureCompositeOutput {
                render_target: parent_target,
            },
        ),
    );
    assert_eq!(
        parent_ctx.opaque_rect_order(),
        host_end,
        "atomic detached content cannot advance the scene parent cursor"
    );
    post_composite.emit(graph, parent_ctx);
    emit_validated_scroll_scene_atomic_projection_text_area_overlay(
        overlay_emission,
        graph,
        parent_ctx,
    );
    assert_eq!(parent_ctx.opaque_rect_order(), parent_end);
    parent_ctx.set_current_target(parent_target);
}

/// Infallible B4-1 emission. It accepts no clear input or replaceable context;
/// those authorities are already owned by the prepared token.
pub(crate) fn emit_prepared_retained_property_scroll_forest(
    prepared: PreparedRetainedPropertyScrollForest<'_>,
) -> RetainedPropertyScrollSceneBuildOutcome {
    let PreparedRetainedPropertyScrollForest {
        viewport,
        graph,
        mut parent_ctx,
        boundaries,
        mut actions,
        transaction,
        schedule,
        clear_rgba_bits,
        target_policy,
        frame_owner,
        budget,
        aggregate_pair_bytes,
        parent_terminal,
        trace,
    } = prepared;
    assert!(viewport.retained_surface_frame_stage_owner_is_active(frame_owner));
    assert!(viewport.retained_property_scroll_scene_stage_is_available());
    assert_eq!(trace.content_pair_bytes, aggregate_pair_bytes);
    assert!(aggregate_pair_bytes <= budget.max_active_pair_bytes);
    assert_eq!(schedule.len(), boundaries.len() * 3);
    assert_eq!(parent_ctx.opaque_rect_order(), 0);
    let parent_target = match target_policy {
        PropertyScrollRootTargetPolicy::ContextRootTarget => parent_ctx
            .current_target()
            .unwrap_or_else(|| parent_ctx.allocate_target(graph)),
    };
    parent_ctx.set_current_target(parent_target);
    graph.add_graphics_pass(ClearPass::new(
        crate::view::render_pass::clear_pass::ClearParams::new(clear_rgba_bits.map(f32::from_bits)),
        crate::view::render_pass::clear_pass::ClearInput {
            pass_context: parent_ctx.graphics_pass_context(),
            clear_depth_stencil: true,
        },
        crate::view::render_pass::clear_pass::ClearOutput {
            render_target: parent_target,
        },
    ));
    if let Some(handle) = parent_target.handle() {
        parent_ctx.set_color_target(Some(handle));
    }
    parent_ctx.set_current_target(parent_target);

    for (index, boundary) in boundaries.into_iter().enumerate() {
        let schedule_base = index * 3;
        let [host, content, overlay] = &schedule[schedule_base..schedule_base + 3] else {
            unreachable!("prepared scene schedule is H/C/O exact")
        };
        assert_eq!(host.phase, PropertyScrollScenePhase::HostBefore);
        assert_eq!(content.phase, PropertyScrollScenePhase::DetachedContent);
        assert_eq!(overlay.phase, PropertyScrollScenePhase::OverlayAfter);
        assert_eq!(host.boundary, content.boundary);
        assert_eq!(host.boundary, overlay.boundary);
        assert_eq!(parent_ctx.opaque_rect_order(), host.parent_span.start);
        emit_prepared_property_scroll_boundary_parts(
            boundary,
            graph,
            &mut parent_ctx,
            parent_target,
            &mut actions,
        );
        assert_eq!(parent_ctx.opaque_rect_order(), overlay.parent_span.end);
    }
    assert!(actions.is_empty());
    assert_eq!(parent_ctx.opaque_rect_order(), parent_terminal);
    assert!(
        viewport.stage_retained_property_scroll_scene(transaction),
        "prepared forest stages its one joint transaction exactly once"
    );
    RetainedPropertyScrollSceneBuildOutcome {
        state: parent_ctx.into_state(),
        trace,
    }
}

pub(crate) fn emit_prepared_frame_root_scroll_scene(
    prepared: PreparedFrameRootScrollScene<'_>,
) -> RetainedPropertyScrollSceneBuildOutcome {
    let PreparedFrameRootScrollScene {
        viewport,
        graph,
        mut parent_ctx,
        roots,
        mut actions,
        transaction,
        clear_rgba_bits,
        frame_owner,
        trace,
    } = prepared;
    assert!(viewport.retained_surface_frame_stage_owner_is_active(frame_owner));
    let parent_target = parent_ctx
        .current_target()
        .unwrap_or_else(|| parent_ctx.allocate_target(graph));
    parent_ctx.set_current_target(parent_target);
    graph.add_graphics_pass(ClearPass::new(
        crate::view::render_pass::clear_pass::ClearParams::new(clear_rgba_bits.map(f32::from_bits)),
        crate::view::render_pass::clear_pass::ClearInput {
            pass_context: parent_ctx.graphics_pass_context(),
            clear_depth_stencil: true,
        },
        crate::view::render_pass::clear_pass::ClearOutput {
            render_target: parent_target,
        },
    ));
    if let Some(handle) = parent_target.handle() {
        parent_ctx.set_color_target(Some(handle));
    }

    for root in roots {
        let root = match root {
            PreparedFrameRootSceneRoot::Plain { receiver_compiler } => {
                super::compiler::emit_validated_frame_root_scroll_receiver(
                    receiver_compiler,
                    graph,
                    &mut parent_ctx,
                    |_, _, _| unreachable!("plain frame root has no detached boundary"),
                );
                continue;
            }
            PreparedFrameRootSceneRoot::Scroll(root) => root,
        };
        let mut content = Some(root.content_compiler);
        let mut backing = Some(root.backing);
        super::compiler::emit_validated_frame_root_scroll_receiver(
            root.receiver_compiler,
            graph,
            &mut parent_ctx,
            |marker, graph, parent_ctx| {
                assert_eq!(marker, root.scroll_cutout);
                let content_compiler = content.take().expect("one detached content artifact");
                let PreparedRetainedPropertyScrollBacking::Single {
                    stamp,
                    color_key,
                    color_desc,
                    geometry,
                    ..
                } = backing.take().expect("one detached content backing")
                else {
                    unreachable!("frame-root P0 seals single-texture content")
                };
                let action = actions
                    .remove(&stamp.identity.resident_key())
                    .expect("prepared frame-root content action is frozen");
                let mut content_ctx = UiBuildContext::from_parts(
                    parent_ctx.viewport(),
                    parent_ctx
                        .layer_subtree_state_with_ancestor_clip(AncestorClipContext::default()),
                );
                let content_target =
                    content_ctx.allocate_persistent_target_with_desc(graph, color_desc, color_key);
                content_ctx.set_current_target(content_target);
                match action {
                    RetainedSurfaceCompileAction::Reraster => {
                        graph.add_graphics_pass(ClearPass::new(
                            crate::view::render_pass::clear_pass::ClearParams::new([0.0; 4]),
                            crate::view::render_pass::clear_pass::ClearInput {
                                pass_context: content_ctx.graphics_pass_context(),
                                clear_depth_stencil: true,
                            },
                            crate::view::render_pass::clear_pass::ClearOutput {
                                render_target: content_target,
                            },
                        ));
                        super::compiler::emit_validated_frame_root_scroll_content(
                            content_compiler,
                            graph,
                            &mut content_ctx,
                        );
                    }
                    RetainedSurfaceCompileAction::Reuse => {
                        content_ctx.replay_opaque_rect_order_exact(0, root.content_local_terminal)
                    }
                }
                assert_eq!(content_ctx.opaque_rect_order(), root.content_local_terminal);
                parent_ctx.merge_child_target_pairs(&content_ctx.into_state());
                parent_ctx.set_current_target(parent_target);
                graph.add_graphics_pass(
                    geometry.into_texture_composite_pass(
                        TextureCompositeInput::from_render_target(
                            TextureCompositeSourceIn::with_handle(
                                content_target
                                    .handle()
                                    .expect("detached persistent content target handle"),
                            ),
                            Default::default(),
                            parent_ctx.graphics_pass_context(),
                        ),
                        TextureCompositeOutput {
                            render_target: parent_target,
                        },
                    ),
                );
            },
        );
        assert!(content.is_none());
        assert!(backing.is_none());
    }
    assert!(actions.is_empty());
    assert!(
        viewport.stage_retained_property_scroll_scene(transaction),
        "prepared frame-root scroll scene stages its joint transaction exactly once"
    );
    RetainedPropertyScrollSceneBuildOutcome {
        state: parent_ctx.into_state(),
        trace,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
enum PropertyBoundaryDagEmitSurface {
    Transform,
    Effect,
}

#[allow(dead_code)]
enum PropertyBoundaryDagEmitNode {
    Scroll,
    Surface {
        kind: PropertyBoundaryDagEmitSurface,
        child: Box<PropertyBoundaryDagEmitNode>,
    },
}

#[allow(dead_code)]
enum PropertyBoundaryDagPreparedBackend<'a> {
    FrameRootScroll(PreparedFrameRootScrollScene<'a>),
    TransformScroll(PreparedRetainedTransformScrollScene<'a>),
    EffectScroll(PreparedRetainedEffectScrollScene<'a>),
    TransformEffectScroll(PreparedRetainedTransformEffectScrollScene<'a>),
    EffectTransformScroll(PreparedRetainedEffectTransformScrollScene<'a>),
}

#[allow(dead_code)]
fn emit_prepared_property_boundary_dag_node(
    node: PropertyBoundaryDagEmitNode,
    backend: PropertyBoundaryDagPreparedBackend<'_>,
    surface_path: &mut Vec<PropertyBoundaryDagEmitSurface>,
) -> RetainedPropertyScrollSceneBuildOutcome {
    match node {
        PropertyBoundaryDagEmitNode::Surface { kind, child } => {
            surface_path.push(kind);
            emit_prepared_property_boundary_dag_node(*child, backend, surface_path)
        }
        PropertyBoundaryDagEmitNode::Scroll => match (surface_path.as_slice(), backend) {
            ([], PropertyBoundaryDagPreparedBackend::FrameRootScroll(prepared)) => {
                emit_prepared_frame_root_scroll_scene(prepared)
            }
            (
                [PropertyBoundaryDagEmitSurface::Transform],
                PropertyBoundaryDagPreparedBackend::TransformScroll(prepared),
            ) => emit_prepared_retained_transform_scroll_scene(prepared),
            (
                [PropertyBoundaryDagEmitSurface::Effect],
                PropertyBoundaryDagPreparedBackend::EffectScroll(prepared),
            ) => emit_prepared_retained_effect_scroll_scene(prepared),
            (
                [
                    PropertyBoundaryDagEmitSurface::Transform,
                    PropertyBoundaryDagEmitSurface::Effect,
                ],
                PropertyBoundaryDagPreparedBackend::TransformEffectScroll(prepared),
            ) => emit_prepared_retained_transform_effect_scroll_scene(prepared),
            (
                [
                    PropertyBoundaryDagEmitSurface::Effect,
                    PropertyBoundaryDagEmitSurface::Transform,
                ],
                PropertyBoundaryDagPreparedBackend::EffectTransformScroll(prepared),
            ) => emit_prepared_retained_effect_transform_scroll_scene(prepared),
            _ => unreachable!("DAG prepared backend is sealed to its recursive surface path"),
        },
    }
}

/// Recursive Phase1 emitter. Surface wrappers are descended in DAG order and
/// the exact legacy emitter is invoked only at the terminal Scroll node. The
/// private program/backend pairing makes unsupported or reordered paths
/// unconstructible outside this module.
#[allow(dead_code)]
pub(crate) fn emit_prepared_property_boundary_dag_scene(
    prepared: PreparedPropertyBoundaryDagScene<'_>,
) -> RetainedPropertyScrollSceneBuildOutcome {
    let (node, backend) = match prepared {
        PreparedPropertyBoundaryDagScene::FrameRootScroll(prepared) => (
            PropertyBoundaryDagEmitNode::Scroll,
            PropertyBoundaryDagPreparedBackend::FrameRootScroll(prepared),
        ),
        PreparedPropertyBoundaryDagScene::TransformScroll(prepared) => (
            PropertyBoundaryDagEmitNode::Surface {
                kind: PropertyBoundaryDagEmitSurface::Transform,
                child: Box::new(PropertyBoundaryDagEmitNode::Scroll),
            },
            PropertyBoundaryDagPreparedBackend::TransformScroll(prepared),
        ),
        PreparedPropertyBoundaryDagScene::EffectScroll(prepared) => (
            PropertyBoundaryDagEmitNode::Surface {
                kind: PropertyBoundaryDagEmitSurface::Effect,
                child: Box::new(PropertyBoundaryDagEmitNode::Scroll),
            },
            PropertyBoundaryDagPreparedBackend::EffectScroll(prepared),
        ),
        PreparedPropertyBoundaryDagScene::TransformEffectScroll(prepared) => (
            PropertyBoundaryDagEmitNode::Surface {
                kind: PropertyBoundaryDagEmitSurface::Transform,
                child: Box::new(PropertyBoundaryDagEmitNode::Surface {
                    kind: PropertyBoundaryDagEmitSurface::Effect,
                    child: Box::new(PropertyBoundaryDagEmitNode::Scroll),
                }),
            },
            PropertyBoundaryDagPreparedBackend::TransformEffectScroll(prepared),
        ),
        PreparedPropertyBoundaryDagScene::EffectTransformScroll(prepared) => (
            PropertyBoundaryDagEmitNode::Surface {
                kind: PropertyBoundaryDagEmitSurface::Effect,
                child: Box::new(PropertyBoundaryDagEmitNode::Surface {
                    kind: PropertyBoundaryDagEmitSurface::Transform,
                    child: Box::new(PropertyBoundaryDagEmitNode::Scroll),
                }),
            },
            PropertyBoundaryDagPreparedBackend::EffectTransformScroll(prepared),
        ),
        PreparedPropertyBoundaryDagScene::ScrollEffect(prepared)
        | PreparedPropertyBoundaryDagScene::TransformScrollEffect(prepared) => {
            return emit_prepared_retained_scroll_content_effect_scene(prepared);
        }
    };
    emit_prepared_property_boundary_dag_node(node, backend, &mut Vec::new())
}

/// Infallible B2 emission.  The prepared token is the only authority accepted;
/// the viewport receives one joint stage after the complete ordered scene has
/// been emitted.
#[cfg(test)]
fn emit_prepared_retained_property_scroll_scene_inner(
    prepared: PreparedRetainedPropertyScrollScene<'_>,
    root_clear: Option<[f32; 4]>,
) -> RetainedPropertyScrollSceneBuildOutcome {
    let PreparedRetainedPropertyScrollScene {
        viewport,
        graph,
        mut parent_ctx,
        host_before,
        content,
        overlay,
        backing,
        mut actions,
        transaction,
        host_parent_terminal,
        content_local_terminal,
        parent_terminal,
        trace,
    } = prepared;
    assert!(
        viewport.retained_property_scroll_scene_stage_is_available(),
        "exclusive prepared token keeps the single stage slot available"
    );
    assert_eq!(parent_ctx.opaque_rect_order(), 0);
    let parent_target = parent_ctx
        .current_target()
        .unwrap_or_else(|| parent_ctx.allocate_target(graph));
    parent_ctx.set_current_target(parent_target);
    if let Some(clear_rgba) = root_clear {
        graph.add_graphics_pass(ClearPass::new(
            crate::view::render_pass::clear_pass::ClearParams::new(clear_rgba),
            crate::view::render_pass::clear_pass::ClearInput {
                pass_context: parent_ctx.graphics_pass_context(),
                clear_depth_stencil: true,
            },
            crate::view::render_pass::clear_pass::ClearOutput {
                render_target: parent_target,
            },
        ));
        if let Some(handle) = parent_target.handle() {
            parent_ctx.set_color_target(Some(handle));
        }
        parent_ctx.set_current_target(parent_target);
    }
    emit_validated_scroll_scene_host_before_artifact(host_before, graph, &mut parent_ctx);
    assert_eq!(parent_ctx.opaque_rect_order(), host_parent_terminal);
    match backing {
        PreparedRetainedPropertyScrollBacking::Single {
            stamp,
            color_key,
            color_desc,
            geometry,
            ..
        } => {
            let action = actions
                .remove(&stamp.identity.resident_key())
                .expect("prepared single scroll action is frozen");
            let mut content_ctx = UiBuildContext::from_parts(
                parent_ctx.viewport(),
                parent_ctx.layer_subtree_state_with_ancestor_clip(AncestorClipContext::default()),
            );
            content_ctx.set_current_render_transform(None);
            let content_target =
                content_ctx.allocate_persistent_target_with_desc(graph, color_desc, color_key);
            content_ctx.set_current_target(content_target);
            match action {
                RetainedSurfaceCompileAction::Reraster => {
                    graph.add_graphics_pass(ClearPass::new(
                        crate::view::render_pass::clear_pass::ClearParams::new([0.0; 4]),
                        crate::view::render_pass::clear_pass::ClearInput {
                            pass_context: content_ctx.graphics_pass_context(),
                            clear_depth_stencil: true,
                        },
                        crate::view::render_pass::clear_pass::ClearOutput {
                            render_target: content_target,
                        },
                    ));
                    emit_validated_scroll_scene_content_artifact(&content, graph, &mut content_ctx);
                }
                RetainedSurfaceCompileAction::Reuse => {
                    content_ctx.replay_opaque_rect_order_exact(0, content_local_terminal);
                }
            }
            assert_eq!(content_ctx.opaque_rect_order(), content_local_terminal);
            parent_ctx.merge_child_target_pairs(&content_ctx.into_state());
            parent_ctx.set_current_target(parent_target);
            graph.add_graphics_pass(
                geometry.into_texture_composite_pass(
                    TextureCompositeInput::from_render_target(
                        TextureCompositeSourceIn::with_handle(
                            content_target
                                .handle()
                                .expect("prepared persistent property-scroll target has a handle"),
                        ),
                        Default::default(),
                        parent_ctx.graphics_pass_context(),
                    ),
                    TextureCompositeOutput {
                        render_target: parent_target,
                    },
                ),
            );
        }
        PreparedRetainedPropertyScrollBacking::Tiled { tiles, .. } => {
            for tile in tiles {
                let action = actions
                    .remove(&tile.stamp.identity.resident_key())
                    .expect("prepared tile action is frozen");
                let mut content_ctx = UiBuildContext::from_parts(
                    parent_ctx.viewport(),
                    parent_ctx
                        .layer_subtree_state_with_ancestor_clip(AncestorClipContext::default()),
                );
                content_ctx.set_current_render_transform(None);
                content_ctx.push_scissor_rect(Some(tile.geometry.raster_bounds()));
                let content_target = content_ctx.allocate_persistent_target_with_desc(
                    graph,
                    tile.color_desc,
                    tile.color_key,
                );
                content_ctx.set_current_target(content_target);
                match action {
                    RetainedSurfaceCompileAction::Reraster => {
                        graph.add_graphics_pass(ClearPass::new(
                            crate::view::render_pass::clear_pass::ClearParams::new([0.0; 4]),
                            crate::view::render_pass::clear_pass::ClearInput {
                                pass_context: content_ctx.graphics_pass_context(),
                                clear_depth_stencil: true,
                            },
                            crate::view::render_pass::clear_pass::ClearOutput {
                                render_target: content_target,
                            },
                        ));
                        emit_validated_scroll_scene_content_artifact(
                            &content,
                            graph,
                            &mut content_ctx,
                        );
                    }
                    RetainedSurfaceCompileAction::Reuse => {
                        content_ctx.replay_opaque_rect_order_exact(0, content_local_terminal);
                    }
                }
                assert_eq!(content_ctx.opaque_rect_order(), content_local_terminal);
                parent_ctx.merge_child_target_pairs(&content_ctx.into_state());
                parent_ctx.set_current_target(parent_target);
                graph.add_graphics_pass(
                    tile.geometry.into_texture_composite_pass(
                        TextureCompositeInput::from_render_target(
                            TextureCompositeSourceIn::with_handle(
                                content_target.handle().expect(
                                    "prepared persistent property-scroll tile has a handle",
                                ),
                            ),
                            Default::default(),
                            parent_ctx.graphics_pass_context(),
                        ),
                        TextureCompositeOutput {
                            render_target: parent_target,
                        },
                    ),
                );
            }
        }
    }
    assert!(
        actions.is_empty(),
        "all frozen property-scroll actions are consumed"
    );
    assert_eq!(
        parent_ctx.opaque_rect_order(),
        host_parent_terminal,
        "detached content cannot advance the parent cursor"
    );
    emit_validated_scroll_scene_overlay_artifact(overlay, graph, &mut parent_ctx);
    assert_eq!(parent_ctx.opaque_rect_order(), parent_terminal);
    parent_ctx.set_current_target(parent_target);
    assert!(
        viewport.stage_retained_property_scroll_scene(transaction),
        "pre-clear sealed property-scroll transaction stages exactly once"
    );
    RetainedPropertyScrollSceneBuildOutcome {
        state: parent_ctx.into_state(),
        trace,
    }
}

#[cfg(test)]
pub(crate) fn emit_prepared_retained_property_scroll_scene(
    prepared: PreparedRetainedPropertyScrollScene<'_>,
) -> RetainedPropertyScrollSceneBuildOutcome {
    emit_prepared_retained_property_scroll_scene_inner(prepared, None)
}

/// RetainedAuto's exclusive B3 consume entry. The common parent target and
/// clear are created only after every B2 lease proof has been frozen, while
/// the token still prevents any competing viewport or graph mutation.
#[cfg(test)]
pub(crate) fn emit_prepared_retained_property_scroll_scene_with_root_clear(
    prepared: PreparedRetainedPropertyScrollScene<'_>,
    clear_rgba: [f32; 4],
) -> RetainedPropertyScrollSceneBuildOutcome {
    emit_prepared_retained_property_scroll_scene_inner(prepared, Some(clear_rgba))
}

#[allow(clippy::too_many_arguments)]
fn prepare_tiled_content_backing(
    content_root: NodeKey,
    content_stable_id: u64,
    content_bounds: RetainedSurfaceBounds,
    scroll: ScrollNodeSnapshot,
    contents_clip: ClipNodeSnapshot,
    span: super::RetainedSurfaceArtifactSpanStamp,
    content_local_span: Range<u32>,
    graph: &FrameGraph,
    target_format: wgpu::TextureFormat,
    budget: ScrollSceneSingleTextureBudget,
) -> Result<PreparedScrollContentBacking, ScrollScenePrepareError> {
    let content_bounds_u32 =
        exact_dpr1_u32_bounds(content_bounds).ok_or(ScrollScenePrepareError::GeometryContract)?;
    let active_manifest = super::plan_active_scroll_content_tiles_dpr1(
        content_bounds_u32,
        [scroll.offset.x, scroll.offset.y],
        contents_clip.logical_scissor,
        SCROLL_CONTENT_TILE_EDGE,
        SCROLL_CONTENT_TILE_GUTTER,
        SCROLL_CONTENT_TILE_OVERSCAN,
    )
    .ok_or(ScrollScenePrepareError::GeometryContract)?;
    let transaction_manifest = super::ScrollContentTileSetTransactionStamp::from_active_manifest(
        content_root,
        content_stable_id,
        &active_manifest,
    )
    .ok_or(ScrollScenePrepareError::GeometryContract)?;
    let mut declared_keys = FxHashSet::default();
    let mut total_pair_bytes = 0_u64;
    let mut tiles = Vec::with_capacity(active_manifest.tiles().len());
    for &(index, bounds) in active_manifest.tiles() {
        let tile = super::ScrollContentTileRasterIdentity::new(
            index,
            content_bounds_u32,
            bounds,
            SCROLL_CONTENT_TILE_EDGE,
            SCROLL_CONTENT_TILE_GUTTER,
        )
        .ok_or(ScrollScenePrepareError::GeometryContract)?;
        let color_key = crate::view::base_component::scroll_content_tile_layer_stable_key(
            content_stable_id,
            index.column,
            index.row,
        )
        .ok_or(ScrollScenePrepareError::DescriptorPair)?;
        let [x, y, width, height] = bounds.raster;
        let color = texture_desc_for_logical_bounds(
            RetainedSurfaceBounds {
                x: x as f32,
                y: y as f32,
                width: width as f32,
                height: height as f32,
                corner_radii: [0.0; 4],
            },
            1.0,
            None,
            target_format,
        );
        let (color_desc, depth_desc) = persistent_target_texture_descriptors(color, color_key);
        if [
            color_desc.width(),
            color_desc.height(),
            depth_desc.width(),
            depth_desc.height(),
        ]
        .into_iter()
        .any(|dimension| dimension > budget.max_dimension_2d)
        {
            return Err(ScrollScenePrepareError::SingleTextureLimit);
        }
        let color_cost = crate::view::raster_cost::texture_desc_payload_bytes(&color_desc);
        let depth_cost = crate::view::raster_cost::texture_desc_payload_bytes(&depth_desc);
        if !color_cost.confidence.budget_usable() || !depth_cost.confidence.budget_usable() {
            return Err(ScrollScenePrepareError::RasterCostUnknown);
        }
        let pair_bytes = color_cost
            .bytes
            .checked_add(depth_cost.bytes)
            .ok_or(ScrollScenePrepareError::RasterCostOverflow)?;
        total_pair_bytes = total_pair_bytes
            .checked_add(pair_bytes)
            .ok_or(ScrollScenePrepareError::RasterCostOverflow)?;
        let depth_key = color_key
            .depth_stencil()
            .ok_or(ScrollScenePrepareError::DescriptorPair)?;
        for key in [color_key, depth_key] {
            if !declared_keys.insert(key)
                || graph
                    .declared_persistent_texture_keys()
                    .any(|declared| declared == key)
            {
                return Err(ScrollScenePrepareError::PersistentKeyAlreadyDeclared(key));
            }
        }
        let stamp = super::validated_scroll_content_tile_raster_stamp(
            content_root,
            content_stable_id,
            tile,
            RetainedSurfaceRasterInputs {
                color: color_desc.clone(),
                depth: depth_desc,
                scale_factor_bits: 1.0_f32.to_bits(),
                source_bounds_bits: bounds.raster.map(|value| (value as f32).to_bits()),
            },
            span.clone(),
            content_local_span.clone(),
        )
        .ok_or(ScrollScenePrepareError::ArtifactStore)?;
        let geometry =
            super::PreparedScrollContentTileCompositeGeometry::from_validated_tile_stamp(
                &stamp,
                scroll,
                contents_clip,
            )
            .ok_or(ScrollScenePrepareError::GeometryContract)?;
        if geometry.source_key() != color_key
            || geometry.raster_bounds() != bounds.raster
            || geometry.interior_bounds() != bounds.interior
        {
            return Err(ScrollScenePrepareError::GeometryContract);
        }
        tiles.push(PreparedScrollTile {
            stamp,
            color_key,
            color_desc,
            geometry,
        });
    }
    if total_pair_bytes > budget.max_pair_bytes {
        return Err(ScrollScenePrepareError::ActiveTileBudget);
    }
    Ok(PreparedScrollContentBacking::Tiled {
        manifest: transaction_manifest,
        tiles,
        total_pair_bytes,
    })
}

fn extract_root_scene_chunk(
    source: &PaintArtifact,
    chunk_index: usize,
    root: NodeKey,
) -> Option<PaintArtifact> {
    let chunk = source.chunks.get(chunk_index)?;
    let ops = source.ops.get(chunk.op_range.clone())?.to_vec();
    let mut chunk = chunk.clone();
    chunk.op_range = 0..ops.len();
    Some(PaintArtifact {
        target: PaintArtifactTarget::CurrentTarget,
        chunks: vec![chunk],
        ops,
        clip_nodes: Vec::new(),
        effect_nodes: Vec::new(),
        owner_nodes: vec![PaintOwnerSnapshot {
            owner: root,
            parent: None,
        }],
    })
}

#[allow(clippy::too_many_arguments)]
fn plan_single_root_scroll_scene(
    arena: &NodeArena,
    roots: &[NodeKey],
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    scale_factor: f32,
    incoming_paint_offset: [f32; 2],
    outer_scissor_rect: Option<[u32; 4]>,
) -> Result<ScrollScenePlan, FramePaintPlanError> {
    let [root] = roots else {
        return Err(FramePaintPlanError {
            reasons: vec![FramePaintPlanRejection::RootCount(roots.len())],
        });
    };
    // The direct-leaf corpus owns one outer contents clip.  TextArea subtree
    // variants add local contents/projection clips; the exact planner below
    // remains final authority for the accepted shapes.
    if property_trees.scrolls.len() != 1 || !(1..=3).contains(&property_trees.clips.len()) {
        return Err(FramePaintPlanError {
            reasons: vec![FramePaintPlanRejection::InvalidScrollHost(*root)],
        });
    }
    plan_exact_root_scroll_scene(
        arena,
        *root,
        property_trees,
        paint_generations,
        scale_factor,
        incoming_paint_offset,
        outer_scissor_rect,
    )
}

#[allow(clippy::too_many_arguments)]
fn plan_exact_root_scroll_scene(
    arena: &NodeArena,
    root: NodeKey,
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    scale_factor: f32,
    incoming_paint_offset: [f32; 2],
    outer_scissor_rect: Option<[u32; 4]>,
) -> Result<ScrollScenePlan, FramePaintPlanError> {
    let invalid = || FramePaintPlanError {
        reasons: vec![FramePaintPlanRejection::InvalidScrollHost(root)],
    };
    if scale_factor.to_bits() != 1.0_f32.to_bits()
        || incoming_paint_offset.map(f32::to_bits) != [0.0_f32.to_bits(); 2]
        || outer_scissor_rect.is_some()
        || !property_trees.validation_errors.is_empty()
        || !property_trees.transforms.is_empty()
        || !property_trees.effects.is_empty()
        || arena.parent_of(root).is_some()
    {
        return Err(invalid());
    }
    let node = arena.get(root).ok_or_else(invalid)?;
    let element = node
        .element
        .as_any()
        .downcast_ref::<crate::view::base_component::Element>()
        .ok_or_else(invalid)?;
    let direct_admission = element.exact_retained_scroll_host_admission(root, arena, scale_factor);
    let text_area_subtree_admission = direct_admission
        .is_none()
        .then(|| {
            element.exact_retained_scroll_text_area_subtree_admission(root, arena, scale_factor)
        })
        .flatten();
    let interactive_text_area_subtree_admission = direct_admission
        .is_none()
        .then(|| {
            element.exact_retained_scroll_interactive_text_area_subtree_admission(
                root,
                arena,
                scale_factor,
            )
        })
        .flatten();
    let atomic_projection_text_area_subtree_admission = direct_admission
        .is_none()
        .then(|| {
            element.exact_retained_scroll_atomic_projection_text_area_subtree_admission(
                root,
                arena,
                scale_factor,
            )
        })
        .flatten();
    let focused_atomic_projection_text_area_subtree_admission = direct_admission
        .is_none()
        .then(|| {
            element.exact_retained_scroll_focused_atomic_projection_text_area_subtree_admission(
                root,
                arena,
                scale_factor,
            )
        })
        .flatten();
    let atomic_projection_selection_text_area_subtree_admission = direct_admission
        .is_none()
        .then(|| {
            element.exact_retained_scroll_atomic_projection_selection_text_area_subtree_admission(
                root,
                arena,
                scale_factor,
            )
        })
        .flatten();
    let admission = match (
        direct_admission,
        text_area_subtree_admission,
        interactive_text_area_subtree_admission,
        atomic_projection_text_area_subtree_admission.as_ref(),
        focused_atomic_projection_text_area_subtree_admission.as_ref(),
        atomic_projection_selection_text_area_subtree_admission.as_ref(),
    ) {
        (Some(admission), None, None, None, None, None) => {
            PropertyScrollHostAdmission::direct_leaf(admission)
        }
        (None, Some(admission), None, None, None, None) => {
            PropertyScrollHostAdmission::text_area_subtree(admission)
        }
        (None, None, Some(admission), None, None, None) => {
            PropertyScrollHostAdmission::interactive_text_area_subtree(admission)
        }
        (None, None, None, Some(admission), None, None) => {
            PropertyScrollHostAdmission::atomic_projection_text_area_subtree(admission.clone())
        }
        (None, None, None, None, Some(admission), None) => {
            PropertyScrollHostAdmission::focused_atomic_projection_text_area_subtree(
                admission.clone(),
            )
        }
        (None, None, None, None, None, Some(admission)) => {
            PropertyScrollHostAdmission::atomic_projection_selection_text_area_subtree(
                admission.clone(),
            )
        }
        _ => return Err(invalid()),
    };
    let scroll_id = ScrollNodeId(root);
    let clip_id = ClipNodeId {
        owner: root,
        role: ClipNodeRole::ContentsClip,
    };
    let scroll = property_trees
        .scroll_snapshot_for(scroll_id)
        .ok_or_else(invalid)?;
    let clip_chain = property_trees
        .clip_snapshot_for(Some(clip_id))
        .ok_or_else(invalid)?;
    let [contents_clip] = clip_chain.as_slice() else {
        return Err(invalid());
    };
    let contents_clip = *contents_clip;
    let expected_contents = PropertyTreeState {
        clip: Some(clip_id),
        scroll: Some(scroll_id),
        ..Default::default()
    };
    if !admission.matches_scroll_node(scroll)
        || scroll.owner != root
        || scroll.parent.is_some()
        || scroll.generation == 0
        || contents_clip.id != clip_id
        || contents_clip.owner != root
        || contents_clip.parent.is_some()
        || contents_clip.behavior != ClipBehavior::Intersect
        || contents_clip.generation == 0
        || property_trees.states.get(&root).is_none_or(|state| {
            state.paint != PropertyTreeState::default() || state.descendants != expected_contents
        })
        || property_trees
            .states
            .get(&admission.child)
            .is_none_or(|state| {
                state.paint != expected_contents || state.descendants != expected_contents
            })
    {
        return Err(invalid());
    }
    let baked_witness =
        super::PaintBakedScrollHostWitness::new(root, admission.child, scroll, clip_id)
            .ok_or_else(invalid)?;
    if let Some(focused_admission) = focused_atomic_projection_text_area_subtree_admission.as_ref()
    {
        let host = super::frame_recorder::record_baked_scroll_focused_atomic_projection_text_area_subtree_host_artifact_for_plan(
            arena,
            &[root],
            property_trees,
            paint_generations,
            focused_admission,
            baked_witness,
        )
        .map_err(|fallbacks| FramePaintPlanError {
            reasons: fallbacks
                .into_iter()
                .map(FramePaintPlanRejection::Coverage)
                .collect(),
        })?;
        let content_witness =
            PaintScrollContentWitness::new(root, admission.child, scroll, contents_clip)
                .ok_or_else(invalid)?;
        let local = super::frame_recorder::record_scroll_focused_atomic_projection_text_area_subtree_local_artifact_for_plan(
            arena,
            property_trees,
            paint_generations,
            focused_admission,
            content_witness,
        )
        .map_err(|fallbacks| FramePaintPlanError {
            reasons: fallbacks
                .into_iter()
                .map(FramePaintPlanRejection::Coverage)
                .collect(),
        })?;
        let parts =
            super::frame_recorder::validate_recorded_focused_atomic_projection_text_area_plan_parts(
                host, local,
            )
            .ok_or_else(invalid)?;
        let focused_geometry_matches = parts.matches_admission_geometry(
            [
                admission.source_bounds.x.to_bits(),
                admission.source_bounds.y.to_bits(),
                admission.source_bounds.width.to_bits(),
                admission.source_bounds.height.to_bits(),
            ],
            scroll,
        );
        if parts.boundary_root() != root
            || parts.content_root() != admission.child
            || parts.text_area_root() != focused_admission.text_area_root
            || parts.outer_scroll() != scroll
            || parts.outer_contents_clip() != contents_clip
            || !focused_geometry_matches
        {
            return Err(invalid());
        }
        let local_clip_id = ClipNodeId {
            owner: focused_admission.text_area_root,
            role: ClipNodeRole::ContentsClip,
        };
        let live_chain = property_trees
            .clip_snapshot_for(Some(local_clip_id))
            .ok_or_else(invalid)?;
        let [text_area_clip, live_outer_clip] = live_chain.as_slice() else {
            return Err(invalid());
        };
        if *live_outer_clip != contents_clip {
            return Err(invalid());
        }
        let Some(focused_sidecars) = PropertyScrollFocusedAtomicProjectionSidecarSeal::new(
            parts.caret(),
            parts.preedit(),
            *text_area_clip,
            *live_outer_clip,
        ) else {
            return Err(invalid());
        };
        let post_composite = PropertyScrollPostCompositeSchedule::FocusedAtomicProjectionSidecars(
            Box::new(focused_sidecars),
        );
        let focused_corresponds = admission.exactly_corresponds_to_with_atomic(
            None,
            None,
            None,
            Some(focused_admission),
            &post_composite,
        );
        let focused_resident_corresponds =
            admission.exactly_corresponds_to_resident_with_atomic(None, Some(parts.resident()));
        if !focused_corresponds || !focused_resident_corresponds {
            return Err(invalid());
        }
        let resident = parts.resident().clone();
        return Ok(ScrollScenePlan {
            boundary_root: root,
            root_stable_id: admission.stable_id,
            content_root: admission.child,
            content_stable_id: admission.child_stable_id,
            admission: admission.clone(),
            text_area_subtree_admission: None,
            interactive_text_area_subtree_admission: None,
            atomic_projection_text_area_subtree_admission: None,
            focused_atomic_projection_text_area_subtree_admission:
                focused_atomic_projection_text_area_subtree_admission.clone(),
            post_composite: post_composite.clone(),
            interactive_resident: None,
            atomic_projection_resident: Some(resident.clone()),
            scroll,
            contents_clip,
            planned_admission_witness: admission,
            planned_text_area_subtree_admission: None,
            planned_interactive_text_area_subtree_admission: None,
            planned_atomic_projection_text_area_subtree_admission: None,
            planned_focused_atomic_projection_text_area_subtree_admission:
                focused_atomic_projection_text_area_subtree_admission,
            planned_post_composite: post_composite,
            planned_interactive_resident: None,
            planned_atomic_projection_resident: Some(resident),
            planned_scroll_witness: scroll,
            planned_clip_witness: contents_clip,
            recorded: ScrollSceneRecordedAuthority::FocusedAtomicProjectionTextArea(Box::new(
                parts,
            )),
        });
    }
    if let Some(selection_admission) =
        atomic_projection_selection_text_area_subtree_admission.as_ref()
    {
        let host = super::frame_recorder::record_baked_scroll_atomic_projection_selection_text_area_subtree_host_artifact_for_plan(
            arena,
            &[root],
            property_trees,
            paint_generations,
            selection_admission,
            baked_witness,
        )
        .map_err(|fallbacks| FramePaintPlanError {
            reasons: fallbacks
                .into_iter()
                .map(FramePaintPlanRejection::Coverage)
                .collect(),
        })?;
        let content_witness =
            PaintScrollContentWitness::new(root, admission.child, scroll, contents_clip)
                .ok_or_else(invalid)?;
        let local = super::frame_recorder::record_scroll_atomic_projection_selection_text_area_subtree_local_artifact_for_plan(
            arena,
            property_trees,
            paint_generations,
            selection_admission,
            content_witness,
        )
        .map_err(|fallbacks| FramePaintPlanError {
            reasons: fallbacks
                .into_iter()
                .map(FramePaintPlanRejection::Coverage)
                .collect(),
        })?;
        let authority = super::frame_recorder::validate_recorded_atomic_projection_selection_text_area_authority(host, local)
            .ok_or_else(invalid)?;
        let parts = super::frame_recorder::validate_recorded_atomic_projection_selection_text_area_plan_parts(authority)
            .ok_or_else(invalid)?;
        if parts.boundary_root() != root
            || parts.content_root() != admission.child
            || parts.text_area_root() != selection_admission.text_area_root
            || parts.outer_scroll() != scroll
            || parts.outer_contents_clip() != contents_clip
            || !parts.matches_admission_geometry(
                [
                    admission.source_bounds.x.to_bits(),
                    admission.source_bounds.y.to_bits(),
                    admission.source_bounds.width.to_bits(),
                    admission.source_bounds.height.to_bits(),
                ],
                scroll,
            )
            || !admission.exactly_corresponds_to_with_atomic(
                None,
                None,
                None,
                None,
                &PropertyScrollPostCompositeSchedule::NoneForExistingGrammar,
            )
            || !admission.exactly_corresponds_to_resident_with_atomic(None, None)
        {
            return Err(invalid());
        }
        return Ok(ScrollScenePlan {
            boundary_root: root,
            root_stable_id: admission.stable_id,
            content_root: admission.child,
            content_stable_id: admission.child_stable_id,
            admission: admission.clone(),
            text_area_subtree_admission: None,
            interactive_text_area_subtree_admission: None,
            atomic_projection_text_area_subtree_admission: None,
            focused_atomic_projection_text_area_subtree_admission: None,
            post_composite: PropertyScrollPostCompositeSchedule::NoneForExistingGrammar,
            interactive_resident: None,
            atomic_projection_resident: None,
            scroll,
            contents_clip,
            planned_admission_witness: admission,
            planned_text_area_subtree_admission: None,
            planned_interactive_text_area_subtree_admission: None,
            planned_atomic_projection_text_area_subtree_admission: None,
            planned_focused_atomic_projection_text_area_subtree_admission: None,
            planned_post_composite: PropertyScrollPostCompositeSchedule::NoneForExistingGrammar,
            planned_interactive_resident: None,
            planned_atomic_projection_resident: None,
            planned_scroll_witness: scroll,
            planned_clip_witness: contents_clip,
            recorded: ScrollSceneRecordedAuthority::AtomicProjectionSelectionTextArea(parts),
        });
    }
    if let Some(atomic_admission) = atomic_projection_text_area_subtree_admission.as_ref() {
        let host = super::frame_recorder::record_baked_scroll_atomic_projection_text_area_subtree_host_artifact_for_plan(
            arena,
            &[root],
            property_trees,
            paint_generations,
            atomic_admission,
            baked_witness,
        )
        .map_err(|fallbacks| FramePaintPlanError {
            reasons: fallbacks
                .into_iter()
                .map(FramePaintPlanRejection::Coverage)
                .collect(),
        })?;
        let content_witness =
            PaintScrollContentWitness::new(root, admission.child, scroll, contents_clip)
                .ok_or_else(invalid)?;
        let local = super::frame_recorder::record_scroll_atomic_projection_text_area_subtree_local_artifact_for_plan(
            arena,
            property_trees,
            paint_generations,
            atomic_admission,
            content_witness,
        )
        .map_err(|fallbacks| FramePaintPlanError {
            reasons: fallbacks
                .into_iter()
                .map(FramePaintPlanRejection::Coverage)
                .collect(),
        })?;
        let parts =
            super::frame_recorder::validate_recorded_atomic_projection_text_area_plan_parts(
                host, local,
            )
            .ok_or_else(invalid)?;
        if parts.boundary_root() != root
            || parts.content_root() != admission.child
            || parts.text_area_root() != atomic_admission.text_area_root
            || parts.outer_scroll() != scroll
            || parts.outer_contents_clip() != contents_clip
            || !admission.exactly_corresponds_to_with_atomic(
                None,
                None,
                Some(atomic_admission),
                None,
                &PropertyScrollPostCompositeSchedule::NoneForExistingGrammar,
            )
            || !admission.exactly_corresponds_to_resident_with_atomic(None, Some(parts.resident()))
        {
            return Err(invalid());
        }
        let resident = parts.resident().clone();
        return Ok(ScrollScenePlan {
            boundary_root: root,
            root_stable_id: admission.stable_id,
            content_root: admission.child,
            content_stable_id: admission.child_stable_id,
            admission: admission.clone(),
            text_area_subtree_admission: None,
            interactive_text_area_subtree_admission: None,
            atomic_projection_text_area_subtree_admission:
                atomic_projection_text_area_subtree_admission.clone(),
            focused_atomic_projection_text_area_subtree_admission: None,
            post_composite: PropertyScrollPostCompositeSchedule::NoneForExistingGrammar,
            interactive_resident: None,
            atomic_projection_resident: Some(resident.clone()),
            scroll,
            contents_clip,
            planned_admission_witness: admission,
            planned_text_area_subtree_admission: None,
            planned_interactive_text_area_subtree_admission: None,
            planned_atomic_projection_text_area_subtree_admission:
                atomic_projection_text_area_subtree_admission,
            planned_focused_atomic_projection_text_area_subtree_admission: None,
            planned_post_composite: PropertyScrollPostCompositeSchedule::NoneForExistingGrammar,
            planned_interactive_resident: None,
            planned_atomic_projection_resident: Some(resident),
            planned_scroll_witness: scroll,
            planned_clip_witness: contents_clip,
            recorded: ScrollSceneRecordedAuthority::AtomicProjectionTextArea(parts),
        });
    }
    let baked_artifact = if let Some(text_area_admission) = text_area_subtree_admission {
        super::frame_recorder::record_baked_scroll_text_area_subtree_host_artifact_for_plan(
            arena,
            &[root],
            property_trees,
            paint_generations,
            text_area_admission,
            baked_witness,
        )
    } else if let Some(text_area_admission) = interactive_text_area_subtree_admission {
        super::frame_recorder::record_baked_scroll_interactive_text_area_subtree_host_artifact_for_plan(
            arena,
            &[root],
            property_trees,
            paint_generations,
            text_area_admission,
            baked_witness,
        )
    } else {
        super::frame_recorder::record_baked_scroll_host_artifact_for_plan(
            arena,
            &[root],
            property_trees,
            paint_generations,
            baked_witness,
        )
    }
    .map_err(|fallbacks| FramePaintPlanError {
        reasons: fallbacks
            .into_iter()
            .map(FramePaintPlanRejection::Coverage)
            .collect(),
    })?;
    if text_area_subtree_admission.is_none()
        && interactive_text_area_subtree_admission.is_none()
        && !super::compiler::validate_baked_scroll_host_artifact_for_plan(
            &baked_artifact,
            root,
            admission.child,
            scroll,
            contents_clip,
        )
    {
        return Err(invalid());
    }
    let Some(root_before) = baked_artifact.chunks.first() else {
        return Err(invalid());
    };
    let Some(overlay) = baked_artifact.chunks.last() else {
        return Err(invalid());
    };
    if root_before.owner != root
        || !baked_artifact
            .chunks
            .iter()
            .any(|chunk| chunk.owner == admission.child)
        || overlay.owner != root
    {
        return Err(invalid());
    }
    let host_before = extract_root_scene_chunk(&baked_artifact, 0, root).ok_or_else(invalid)?;
    let overlay_artifact = extract_root_scene_chunk(
        &baked_artifact,
        baked_artifact.chunks.len().saturating_sub(1),
        root,
    )
    .ok_or_else(invalid)?;
    let content_witness =
        PaintScrollContentWitness::new(root, admission.child, scroll, contents_clip)
            .ok_or_else(invalid)?;
    let (content_local, interactive_resident, post_composite) = if let Some(text_area_admission) =
        text_area_subtree_admission
    {
        let artifact =
            super::frame_recorder::record_scroll_text_area_subtree_local_artifact_for_plan(
                arena,
                property_trees,
                paint_generations,
                text_area_admission,
                content_witness,
            )
            .map_err(|fallbacks| FramePaintPlanError {
                reasons: fallbacks
                    .into_iter()
                    .map(FramePaintPlanRejection::Coverage)
                    .collect(),
            })?;
        (
            artifact,
            None,
            PropertyScrollPostCompositeSchedule::NoneForExistingGrammar,
        )
    } else if let Some(text_area_admission) = interactive_text_area_subtree_admission {
        let recorded = super::frame_recorder::record_scroll_interactive_text_area_subtree_local_artifact_for_plan(
            arena,
            property_trees,
            paint_generations,
            text_area_admission,
            content_witness,
        )
        .map_err(|fallbacks| FramePaintPlanError {
            reasons: fallbacks
                .into_iter()
                .map(FramePaintPlanRejection::Coverage)
                .collect(),
        })?;
        let local_clip = recorded
            .artifact
            .clip_nodes
            .first()
            .copied()
            .ok_or_else(invalid)?;
        let validated = validate_scroll_scene_interactive_text_area_content_artifact(
            recorded.artifact.clone(),
            admission.child,
            text_area_admission.text_area_root,
            text_area_admission.paint_grammar,
            recorded.preedit_seal.clone(),
            local_clip,
            bounds_bits(content_zero_bounds(scroll)),
        )
        .ok_or_else(invalid)?;
        let (_, resident) = validated.into_parts();
        let caret =
            PropertyScrollInteractiveTextAreaCaretSeal::from_recorded(&recorded.caret_overlay)
                .ok_or_else(invalid)?;
        (
            recorded.artifact,
            Some(resident),
            PropertyScrollPostCompositeSchedule::InteractiveTextAreaCaret(caret),
        )
    } else {
        let artifact = super::frame_recorder::record_scroll_content_local_artifact_for_plan(
            arena,
            property_trees,
            paint_generations,
            content_witness,
        )
        .map_err(|fallbacks| FramePaintPlanError {
            reasons: fallbacks
                .into_iter()
                .map(FramePaintPlanRejection::Coverage)
                .collect(),
        })?;
        (
            artifact,
            None,
            PropertyScrollPostCompositeSchedule::NoneForExistingGrammar,
        )
    };
    if !admission.exactly_corresponds_to(
        text_area_subtree_admission,
        interactive_text_area_subtree_admission,
        &post_composite,
    ) {
        return Err(invalid());
    }
    let host_terminal = opaque_order_count(&host_before);
    let content_terminal = opaque_order_count(&content_local);
    // Opaque order is render-target local. Detached content starts at zero on
    // its persistent target and must never advance the parent target cursor.
    let overlay_count = opaque_order_count(&overlay_artifact);
    let post_composite_delta = post_composite.opaque_order_delta().ok_or_else(invalid)?;
    let overlay_start = host_terminal
        .checked_add(post_composite_delta)
        .ok_or_else(invalid)?;
    let parent_terminal = overlay_start
        .checked_add(overlay_count)
        .ok_or_else(invalid)?;
    Ok(ScrollScenePlan {
        boundary_root: root,
        root_stable_id: admission.stable_id,
        content_root: admission.child,
        content_stable_id: admission.child_stable_id,
        admission: admission.clone(),
        text_area_subtree_admission,
        interactive_text_area_subtree_admission,
        atomic_projection_text_area_subtree_admission: None,
        focused_atomic_projection_text_area_subtree_admission: None,
        post_composite: post_composite.clone(),
        interactive_resident: interactive_resident.clone(),
        atomic_projection_resident: None,
        scroll,
        contents_clip,
        planned_admission_witness: admission,
        planned_text_area_subtree_admission: text_area_subtree_admission,
        planned_interactive_text_area_subtree_admission: interactive_text_area_subtree_admission,
        planned_atomic_projection_text_area_subtree_admission: None,
        planned_focused_atomic_projection_text_area_subtree_admission: None,
        planned_post_composite: post_composite,
        planned_interactive_resident: interactive_resident,
        planned_atomic_projection_resident: None,
        planned_scroll_witness: scroll,
        planned_clip_witness: contents_clip,
        recorded: ScrollSceneRecordedAuthority::Existing {
            host_before,
            content_local,
            overlay: overlay_artifact,
            host_parent_span: 0..host_terminal,
            content_local_span: 0..content_terminal,
            overlay_parent_span: overlay_start..parent_terminal,
        },
    })
}

fn validate_parent_target(
    graph: &FrameGraph,
    ctx: &UiBuildContext,
) -> Result<(), ScrollScenePrepareError> {
    let viewport = ctx.viewport();
    if let Some(handle) = ctx.current_target().and_then(|target| target.handle()) {
        let Some(desc) = graph.texture_desc_for_handle(handle) else {
            return Err(ScrollScenePrepareError::ParentTarget);
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
            return Err(ScrollScenePrepareError::ParentTarget);
        }
    }
    Ok(())
}

fn prepare_planned_scroll_scene(
    plan: ScrollScenePlan,
    graph: &FrameGraph,
    ctx: &UiBuildContext,
    budget: ScrollSceneSingleTextureBudget,
) -> Result<PreparedScrollScene, ScrollScenePrepareError> {
    validate_parent_target(graph, ctx)?;
    if plan.boundary_root == plan.content_root
        || plan.root_stable_id == 0
        || plan.content_stable_id == 0
        || plan.admission.boundary_root != plan.boundary_root
        || plan.admission.stable_id != plan.root_stable_id
        || plan.admission.child != plan.content_root
        || plan.admission.child_stable_id != plan.content_stable_id
        || !plan.admission.matches_scroll_node(plan.scroll)
        || !plan
            .scroll
            .has_canonical_vertical_geometry_with_contents_clip(plan.contents_clip)
    {
        return Err(ScrollScenePrepareError::PlanShape);
    }
    if !plan.admission.bitwise_eq(&plan.planned_admission_witness)
        || plan.scroll != plan.planned_scroll_witness
        || plan.contents_clip != plan.planned_clip_witness
    {
        return Err(ScrollScenePrepareError::FrozenWitness);
    }
    // Interactive caret emission belongs exclusively to the atomic forest
    // path. This legacy singleton prepare entry remains fail-closed.
    if plan.interactive_text_area_subtree_admission.is_some()
        || !matches!(
            plan.post_composite,
            PropertyScrollPostCompositeSchedule::NoneForExistingGrammar
        )
    {
        return Err(ScrollScenePrepareError::PlanShape);
    }
    let viewport = ctx.viewport();
    if viewport.scale_factor().to_bits() != 1.0_f32.to_bits()
        || ctx.paint_offset().map(f32::to_bits) != [0.0_f32.to_bits(); 2]
        || ctx.graphics_pass_context().scissor_rect.is_some()
        || ctx.opaque_rect_order() != 0
    {
        return Err(ScrollScenePrepareError::ContextMismatch);
    }
    let ScrollScenePlan {
        boundary_root,
        root_stable_id: _,
        content_root,
        content_stable_id,
        admission,
        text_area_subtree_admission: _,
        interactive_text_area_subtree_admission: _,
        atomic_projection_text_area_subtree_admission: _,
        focused_atomic_projection_text_area_subtree_admission: _,
        post_composite: _,
        interactive_resident: _,
        atomic_projection_resident: _,
        scroll,
        contents_clip,
        planned_admission_witness: _,
        planned_text_area_subtree_admission: _,
        planned_interactive_text_area_subtree_admission: _,
        planned_atomic_projection_text_area_subtree_admission: _,
        planned_focused_atomic_projection_text_area_subtree_admission: _,
        planned_post_composite: _,
        planned_interactive_resident: _,
        planned_atomic_projection_resident: _,
        planned_scroll_witness: _,
        planned_clip_witness: _,
        recorded,
    } = plan;
    let ScrollSceneRecordedAuthority::Existing {
        host_before,
        content_local,
        overlay,
        host_parent_span,
        content_local_span,
        overlay_parent_span,
    } = recorded
    else {
        // C3a remains graph-inert in this segment. No atomic authority can
        // reach the legacy singleton prepare path.
        return Err(ScrollScenePrepareError::PlanShape);
    };
    let host_bounds_bits = bounds_bits(admission.source_bounds);
    let content_bounds = content_zero_bounds(scroll);
    let content_bounds_bits = bounds_bits(content_bounds);
    let host_terminal = opaque_order_count(&host_before);
    let content_terminal = opaque_order_count(&content_local);
    let overlay_count = opaque_order_count(&overlay);
    let parent_terminal = host_terminal
        .checked_add(overlay_count)
        .ok_or(ScrollScenePrepareError::PlanShape)?;
    if host_parent_span != (0..host_terminal)
        || content_local_span != (0..content_terminal)
        || overlay_parent_span != (host_terminal..parent_terminal)
    {
        return Err(ScrollScenePrepareError::PlanShape);
    }
    let host_before =
        validate_scroll_scene_host_before_artifact(host_before, boundary_root, host_bounds_bits)
            .ok_or(ScrollScenePrepareError::ArtifactStore)?;
    let content =
        validate_scroll_scene_content_artifact(content_local, content_root, content_bounds_bits)
            .ok_or(ScrollScenePrepareError::ArtifactStore)?;
    let overlay =
        validate_scroll_scene_overlay_artifact(overlay, boundary_root, scroll, host_bounds_bits)
            .ok_or(ScrollScenePrepareError::ArtifactStore)?;
    let span =
        validated_scroll_content_artifact_span_stamp(&content, 0, content_local_span.clone())
            .ok_or(ScrollScenePrepareError::ArtifactStore)?;
    let content_color_key = scroll_content_layer_stable_key(content_stable_id);
    let color = texture_desc_for_logical_bounds(
        content_bounds,
        viewport.scale_factor(),
        None,
        viewport.target_format(),
    );
    let (content_color_desc, content_depth_desc) =
        persistent_target_texture_descriptors(color, content_color_key);
    let color_bytes = crate::view::raster_cost::texture_desc_payload_bytes(&content_color_desc);
    let depth_bytes = crate::view::raster_cost::texture_desc_payload_bytes(&content_depth_desc);
    if !color_bytes.confidence.budget_usable() || !depth_bytes.confidence.budget_usable() {
        return Err(ScrollScenePrepareError::RasterCostUnknown);
    }
    let pair_bytes = color_bytes
        .bytes
        .checked_add(depth_bytes.bytes)
        .ok_or(ScrollScenePrepareError::RasterCostOverflow)?;
    let depth_key = content_color_key
        .depth_stencil()
        .ok_or(ScrollScenePrepareError::DescriptorPair)?;
    let single_rejection = if [
        content_color_desc.width(),
        content_color_desc.height(),
        content_depth_desc.width(),
        content_depth_desc.height(),
    ]
    .into_iter()
    .any(|dimension| dimension > budget.max_dimension_2d)
    {
        Some(SingleTextureAdmissionError::DimensionExceeded)
    } else if pair_bytes > budget.max_pair_bytes {
        Some(SingleTextureAdmissionError::PairBudgetExceeded)
    } else {
        None
    };
    let content_backing = match single_rejection {
        None => {
            for key in [content_color_key, depth_key] {
                if graph
                    .declared_persistent_texture_keys()
                    .any(|declared| declared == key)
                {
                    return Err(ScrollScenePrepareError::PersistentKeyAlreadyDeclared(key));
                }
            }
            let target = RetainedSurfaceRasterInputs {
                color: content_color_desc.clone(),
                depth: content_depth_desc,
                scale_factor_bits: viewport.scale_factor().to_bits(),
                source_bounds_bits: content_bounds_bits,
            };
            let content_stamp = validated_scroll_content_raster_stamp(
                content_root,
                content_stable_id,
                target,
                span,
                content_local_span,
            )
            .ok_or(ScrollScenePrepareError::ArtifactStore)?;
            if content_stamp.identity.role != RetainedSurfaceRasterRole::ScrollContent
                || content_stamp.identity.color_key != content_color_key
                || content_stamp.identity.boundary_root != content_root
                || content_stamp.identity.stable_id != content_stable_id
            {
                return Err(ScrollScenePrepareError::ArtifactStore);
            }
            let content_geometry =
                PreparedScrollContentCompositeGeometry::from_validated_content_stamp(
                    &content_stamp,
                    scroll,
                    contents_clip,
                )
                .ok_or(ScrollScenePrepareError::GeometryContract)?;
            if content_geometry.source_key() != content_color_key
                || content_geometry.source_bounds_bits() != content_bounds_bits
            {
                return Err(ScrollScenePrepareError::GeometryContract);
            }
            PreparedScrollContentBacking::Single {
                stamp: content_stamp,
                color_key: content_color_key,
                color_desc: content_color_desc,
                geometry: content_geometry,
                pair_bytes,
            }
        }
        Some(
            SingleTextureAdmissionError::DimensionExceeded
            | SingleTextureAdmissionError::PairBudgetExceeded,
        ) => prepare_tiled_content_backing(
            content_root,
            content_stable_id,
            content_bounds,
            scroll,
            contents_clip,
            span,
            content_local_span,
            graph,
            viewport.target_format(),
            budget,
        )?,
    };
    Ok(PreparedScrollScene {
        host_before,
        content,
        overlay,
        content_backing,
        parent_target: ctx.current_target(),
        host_parent_terminal: host_terminal,
        content_local_terminal: content_terminal,
        parent_terminal,
    })
}

/// The only production A2 preparation seam. All live authorities remain
/// borrowed across a fresh plan+prepare transaction, and the returned token
/// owns its validated artifacts so no stale external plan can be supplied.
#[allow(clippy::too_many_arguments)]
fn prepare_scroll_scene_from_live(
    arena: &NodeArena,
    roots: &[NodeKey],
    property_trees: &PropertyTrees,
    paint_generations: &PaintGenerationTracker,
    graph: &FrameGraph,
    ctx: &UiBuildContext,
    budget: ScrollSceneSingleTextureBudget,
) -> Result<PreparedScrollScene, ScrollSceneFromLiveError> {
    if !paint_generations.matches_live_snapshot(arena, roots, property_trees) {
        return Err(ScrollSceneFromLiveError::LiveSnapshotDrift);
    }
    let plan = plan_single_root_scroll_scene(
        arena,
        roots,
        property_trees,
        paint_generations,
        ctx.viewport().scale_factor(),
        ctx.paint_offset(),
        ctx.graphics_pass_context().scissor_rect,
    )
    .map_err(ScrollSceneFromLiveError::Plan)?;
    if !paint_generations.matches_live_snapshot(arena, roots, property_trees) {
        return Err(ScrollSceneFromLiveError::LiveSnapshotDrift);
    }
    prepare_planned_scroll_scene(plan, graph, ctx, budget)
        .map_err(ScrollSceneFromLiveError::Prepare)
}

const PRODUCTION_SCROLL_CONTENT_PAIR_BUDGET_BYTES: u64 = 128 * 1024 * 1024;

pub(crate) fn production_single_texture_budget(
    viewport: &Viewport,
) -> ScrollSceneSingleTextureBudget {
    let max_dimension_2d = viewport
        .device()
        .map(|device| device.limits().max_texture_dimension_2d)
        .unwrap_or_else(|| wgpu::Limits::default().max_texture_dimension_2d);
    ScrollSceneSingleTextureBudget::new(
        max_dimension_2d,
        PRODUCTION_SCROLL_CONTENT_PAIR_BUDGET_BYTES,
    )
    .expect("production scroll-scene budget is non-zero")
}

fn emit_frozen_scroll_scene(
    frozen: FrozenPreparedScrollScene,
    graph: &mut FrameGraph,
    mut parent_ctx: UiBuildContext,
) -> (BuildState, ScrollSceneStaging, ScrollSceneBuildTrace) {
    let emission = frozen.into_emission_parts();
    assert_eq!(
        parent_ctx.opaque_rect_order(),
        0,
        "scroll scene must begin at the root parent opaque cursor"
    );

    let parent_target = emission.parent_target.unwrap_or_else(|| {
        let target = parent_ctx.allocate_target(graph);
        parent_ctx.set_current_target(target);
        target
    });
    parent_ctx.set_current_target(parent_target);
    emit_validated_scroll_scene_host_before_artifact(emission.host_before, graph, &mut parent_ctx);
    assert_eq!(
        parent_ctx.opaque_rect_order(),
        emission.host_parent_terminal,
        "scroll host-before artifact must reach its frozen parent terminal"
    );

    let (staging, trace) = match (emission.content_backing, emission.content_actions) {
        (
            PreparedScrollContentBacking::Single {
                stamp,
                color_key,
                color_desc,
                geometry,
                pair_bytes,
            },
            FrozenScrollContentActions::Single(action),
        ) => {
            assert_eq!(geometry.source_key(), color_key);
            assert_eq!(
                geometry.source_bounds_bits(),
                stamp.target.source_bounds_bits
            );
            assert_eq!(stamp.identity.color_key, color_key);
            assert_eq!(stamp.target.color, color_desc);
            let mut content_ctx = UiBuildContext::from_parts(
                parent_ctx.viewport(),
                parent_ctx.layer_subtree_state_with_ancestor_clip(AncestorClipContext::default()),
            );
            content_ctx.set_current_render_transform(None);
            let content_target =
                content_ctx.allocate_persistent_target_with_desc(graph, color_desc, color_key);
            content_ctx.set_current_target(content_target);
            match action {
                RetainedSurfaceCompileAction::Reraster => {
                    graph.add_graphics_pass(ClearPass::new(
                        crate::view::render_pass::clear_pass::ClearParams::new([0.0; 4]),
                        crate::view::render_pass::clear_pass::ClearInput {
                            pass_context: content_ctx.graphics_pass_context(),
                            clear_depth_stencil: true,
                        },
                        crate::view::render_pass::clear_pass::ClearOutput {
                            render_target: content_target,
                        },
                    ));
                    emit_validated_scroll_scene_content_artifact(
                        &emission.content,
                        graph,
                        &mut content_ctx,
                    );
                }
                RetainedSurfaceCompileAction::Reuse => {
                    content_ctx.replay_opaque_rect_order_exact(0, emission.content_local_terminal)
                }
            }
            assert_eq!(
                content_ctx.opaque_rect_order(),
                emission.content_local_terminal,
                "scroll content artifact must reach its frozen target-local terminal"
            );
            parent_ctx.merge_child_target_pairs(&content_ctx.into_state());
            parent_ctx.set_current_target(parent_target);
            graph.add_graphics_pass(
                geometry.into_texture_composite_pass(
                    TextureCompositeInput::from_render_target(
                        TextureCompositeSourceIn::with_handle(
                            content_target
                                .handle()
                                .expect("prepared persistent scroll content target has a handle"),
                        ),
                        Default::default(),
                        parent_ctx.graphics_pass_context(),
                    ),
                    TextureCompositeOutput {
                        render_target: parent_target,
                    },
                ),
            );
            let trace = ScrollSceneBuildTrace {
                backing: ScrollSceneBackingKind::Single,
                action,
                content_root: stamp.identity.boundary_root,
                descriptor_size: [stamp.target.color.width(), stamp.target.color.height()],
                content_chunk_count: stamp.chunks.len(),
                content_op_count: stamp.op_count,
                content_pair_bytes: pair_bytes,
                tile_count: 1,
                reraster_count: usize::from(action == RetainedSurfaceCompileAction::Reraster),
                reuse_count: usize::from(action == RetainedSurfaceCompileAction::Reuse),
            };
            (ScrollSceneStaging::Single(stamp), trace)
        }
        (
            PreparedScrollContentBacking::Tiled {
                manifest,
                tiles,
                total_pair_bytes,
            },
            FrozenScrollContentActions::Tiled(actions),
        ) => {
            assert_eq!(tiles.len(), actions.len());
            let content_root = manifest.content_root();
            let mut stamps = Vec::with_capacity(tiles.len());
            let mut descriptor_size = [0, 0];
            let mut content_chunk_count = 0;
            let mut content_op_count = 0;
            let mut reraster_count = 0;
            let mut reuse_count = 0;
            for (tile, action) in tiles.into_iter().zip(actions) {
                assert_eq!(tile.geometry.source_key(), tile.color_key);
                assert_eq!(
                    tile.geometry
                        .raster_bounds()
                        .map(|value| (value as f32).to_bits()),
                    tile.stamp.target.source_bounds_bits
                );
                assert_eq!(tile.stamp.identity.color_key, tile.color_key);
                assert_eq!(tile.stamp.target.color, tile.color_desc);
                let tile_descriptor_size = [tile.color_desc.width(), tile.color_desc.height()];
                let mut content_ctx = UiBuildContext::from_parts(
                    parent_ctx.viewport(),
                    parent_ctx
                        .layer_subtree_state_with_ancestor_clip(AncestorClipContext::default()),
                );
                content_ctx.set_current_render_transform(None);
                content_ctx.push_scissor_rect(Some(tile.geometry.raster_bounds()));
                let content_target = content_ctx.allocate_persistent_target_with_desc(
                    graph,
                    tile.color_desc,
                    tile.color_key,
                );
                content_ctx.set_current_target(content_target);
                match action {
                    RetainedSurfaceCompileAction::Reraster => {
                        reraster_count += 1;
                        graph.add_graphics_pass(ClearPass::new(
                            crate::view::render_pass::clear_pass::ClearParams::new([0.0; 4]),
                            crate::view::render_pass::clear_pass::ClearInput {
                                pass_context: content_ctx.graphics_pass_context(),
                                clear_depth_stencil: true,
                            },
                            crate::view::render_pass::clear_pass::ClearOutput {
                                render_target: content_target,
                            },
                        ));
                        emit_validated_scroll_scene_content_artifact(
                            &emission.content,
                            graph,
                            &mut content_ctx,
                        );
                    }
                    RetainedSurfaceCompileAction::Reuse => {
                        reuse_count += 1;
                        content_ctx
                            .replay_opaque_rect_order_exact(0, emission.content_local_terminal);
                    }
                }
                assert_eq!(
                    content_ctx.opaque_rect_order(),
                    emission.content_local_terminal,
                    "each scroll tile must reach the frozen target-local terminal"
                );
                parent_ctx.merge_child_target_pairs(&content_ctx.into_state());
                parent_ctx.set_current_target(parent_target);
                graph.add_graphics_pass(
                    tile.geometry.into_texture_composite_pass(
                        TextureCompositeInput::from_render_target(
                            TextureCompositeSourceIn::with_handle(
                                content_target
                                    .handle()
                                    .expect("prepared persistent scroll tile target has a handle"),
                            ),
                            Default::default(),
                            parent_ctx.graphics_pass_context(),
                        ),
                        TextureCompositeOutput {
                            render_target: parent_target,
                        },
                    ),
                );
                descriptor_size[0] = descriptor_size[0].max(tile_descriptor_size[0]);
                descriptor_size[1] = descriptor_size[1].max(tile_descriptor_size[1]);
                content_chunk_count = content_chunk_count.max(tile.stamp.chunks.len());
                content_op_count = content_op_count.max(tile.stamp.op_count);
                stamps.push(tile.stamp);
            }
            let action = if reraster_count > 0 {
                RetainedSurfaceCompileAction::Reraster
            } else {
                RetainedSurfaceCompileAction::Reuse
            };
            let trace = ScrollSceneBuildTrace {
                backing: ScrollSceneBackingKind::Tiled,
                action,
                content_root,
                descriptor_size,
                content_chunk_count,
                content_op_count,
                content_pair_bytes: total_pair_bytes,
                tile_count: stamps.len(),
                reraster_count,
                reuse_count,
            };
            (ScrollSceneStaging::Tiled { manifest, stamps }, trace)
        }
        _ => unreachable!("frozen content actions must match prepared backing"),
    };
    assert_eq!(
        parent_ctx.opaque_rect_order(),
        emission.host_parent_terminal,
        "content-local raster and composite cannot advance the parent opaque cursor"
    );
    emit_validated_scroll_scene_overlay_artifact(emission.overlay, graph, &mut parent_ctx);
    assert_eq!(
        parent_ctx.opaque_rect_order(),
        emission.parent_terminal,
        "scroll overlay must reach the frozen parent terminal"
    );
    parent_ctx.set_current_target(parent_target);
    (parent_ctx.into_state(), staging, trace)
}

enum ScrollSceneStaging {
    Single(RetainedSurfaceRasterStamp),
    Tiled {
        manifest: super::ScrollContentTileSetTransactionStamp,
        stamps: Vec<RetainedSurfaceRasterStamp>,
    },
}

#[cfg(test)]
impl ScrollSceneStaging {
    fn single_stamp(&self) -> &RetainedSurfaceRasterStamp {
        match self {
            Self::Single(stamp) => stamp,
            Self::Tiled { .. } => panic!("single stamp requested from tiled staging"),
        }
    }
}

/// The sole production A3 authority. Live scene observation, pool action
/// selection, typestate freeze, infallible emission, and full-set staging stay
/// in one function so no caller can mix the detached scene with baked scroll.
pub(crate) fn build_scroll_scene_from_pool(
    viewport: &mut Viewport,
    arena: &NodeArena,
    roots: &[NodeKey],
    graph: &mut FrameGraph,
    ctx: UiBuildContext,
) -> Result<ScrollSceneBuildOutcome, ScrollSceneFromLiveError> {
    let budget = production_single_texture_budget(viewport);
    build_scroll_scene_from_pool_with_budget(
        viewport,
        arena,
        roots,
        graph,
        ctx,
        budget,
        ScrollScenePoolActionPolicy::Production,
    )
}

#[derive(Clone, Copy)]
enum ScrollScenePoolActionPolicy {
    Production,
    #[cfg(test)]
    ForcedPairWitness,
}

fn build_scroll_scene_from_pool_with_budget(
    viewport: &mut Viewport,
    arena: &NodeArena,
    roots: &[NodeKey],
    graph: &mut FrameGraph,
    ctx: UiBuildContext,
    budget: ScrollSceneSingleTextureBudget,
    action_policy: ScrollScenePoolActionPolicy,
) -> Result<ScrollSceneBuildOutcome, ScrollSceneFromLiveError> {
    let prepared = {
        let (property_trees, paint_generations) = viewport.scroll_scene_live_authorities();
        prepare_scroll_scene_from_live(
            arena,
            roots,
            property_trees,
            paint_generations,
            graph,
            &ctx,
            budget,
        )?
    };
    let frozen = match &prepared.content_backing {
        PreparedScrollContentBacking::Single { stamp, .. } => {
            let action = match action_policy {
                ScrollScenePoolActionPolicy::Production => {
                    viewport.retained_surface_compile_action_from_pool(stamp)
                }
                #[cfg(test)]
                ScrollScenePoolActionPolicy::ForcedPairWitness => viewport
                    .retained_surface_compile_action_for_forced_test(
                        stamp,
                        stamp.identity.color_key,
                    ),
            };
            prepared.freeze_content_action(action)
        }
        PreparedScrollContentBacking::Tiled {
            manifest, tiles, ..
        } => {
            let stamps = tiles
                .iter()
                .map(|tile| tile.stamp.clone())
                .collect::<Vec<_>>();
            let actions = match action_policy {
                ScrollScenePoolActionPolicy::Production => viewport
                    .freeze_retained_scroll_tile_compile_actions_from_pool(manifest, &stamps),
                #[cfg(test)]
                ScrollScenePoolActionPolicy::ForcedPairWitness => viewport
                    .freeze_retained_scroll_tile_compile_actions_for_forced_test(manifest, &stamps),
            }
            .expect("prepared tile manifest and stamps must remain canonical");
            prepared
                .freeze_tile_actions(actions)
                .expect("one frozen action must exist for every prepared tile")
        }
    };
    let (state, staging, trace) = emit_frozen_scroll_scene(frozen, graph, ctx);
    let staged = match staging {
        ScrollSceneStaging::Single(stamp) => viewport.stage_retained_surface_full_set([stamp]),
        ScrollSceneStaging::Tiled { manifest, stamps } => {
            viewport.stage_retained_scroll_tile_active_set(manifest, stamps)
        }
    };
    assert!(
        staged,
        "prepared scroll content transaction must stage canonically"
    );
    Ok(ScrollSceneBuildOutcome { state, trace })
}

#[cfg(test)]
pub(crate) fn build_scroll_scene_from_pool_with_budget_for_test(
    viewport: &mut Viewport,
    arena: &NodeArena,
    roots: &[NodeKey],
    graph: &mut FrameGraph,
    ctx: UiBuildContext,
    max_dimension_2d: u32,
    max_pair_bytes: u64,
) -> Result<ScrollSceneBuildOutcome, ScrollSceneFromLiveError> {
    let budget = ScrollSceneSingleTextureBudget::new(max_dimension_2d, max_pair_bytes)
        .expect("test scroll-scene budget must be non-zero");
    build_scroll_scene_from_pool_with_budget(
        viewport,
        arena,
        roots,
        graph,
        ctx,
        budget,
        ScrollScenePoolActionPolicy::Production,
    )
}

#[cfg(test)]
fn build_scroll_scene_from_pool_with_forced_pair_witness_for_test(
    viewport: &mut Viewport,
    arena: &NodeArena,
    roots: &[NodeKey],
    graph: &mut FrameGraph,
    ctx: UiBuildContext,
    max_dimension_2d: u32,
    max_pair_bytes: u64,
) -> Result<ScrollSceneBuildOutcome, ScrollSceneFromLiveError> {
    let budget = ScrollSceneSingleTextureBudget::new(max_dimension_2d, max_pair_bytes)
        .expect("test scroll-scene budget must be non-zero");
    build_scroll_scene_from_pool_with_budget(
        viewport,
        arena,
        roots,
        graph,
        ctx,
        budget,
        ScrollScenePoolActionPolicy::ForcedPairWitness,
    )
}

#[cfg(test)]
fn prepare_scroll_scene(
    plan: ScrollScenePlan,
    graph: &FrameGraph,
    ctx: &UiBuildContext,
    budget: ScrollSceneSingleTextureBudget,
) -> Result<PreparedScrollScene, ScrollScenePrepareError> {
    prepare_planned_scroll_scene(plan, graph, ctx, budget)
}

#[cfg(test)]
pub(crate) use tests::{
    NestedMediaLeafKind, NestedTextFallbackKind, nested_scroll_unready_media_fixture_for_test,
    nested_scroll_unready_text_fixture_for_test, retained_auto_scroll_content_effect_fixture,
};

#[cfg(test)]
mod tests;
