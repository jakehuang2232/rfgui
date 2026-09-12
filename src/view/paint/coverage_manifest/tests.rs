use super::*;
use std::any::Any;

use crate::style::{
    Angle, ClipMode, Layout, Length, ParsedValue, Position, PropertyId, Rotate, Style, Transform,
};
use crate::view::base_component::{
    BoxModelSnapshot, BuildState, Element, ElementTrait, EventTarget, LayoutConstraints,
    LayoutPlacement, Layoutable, Renderable, ShadowPaintRecordingCapability, Text, UiBuildContext,
};
use crate::view::frame_graph::FrameGraph;
use crate::view::node_arena::Node;
use slotmap::Key;

#[derive(Clone)]
struct PlanShape {
    before: Vec<(PaintNodePhase, u16)>,
    after: Vec<(PaintNodePhase, u16)>,
}

impl PlanShape {
    fn new(before: &[u16], after: &[u16]) -> Self {
        Self {
            before: before
                .iter()
                .copied()
                .map(|slot| (PaintNodePhase::BeforeChildren, slot))
                .collect(),
            after: after
                .iter()
                .copied()
                .map(|slot| (PaintNodePhase::AfterChildren, slot))
                .collect(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PlanHostMode {
    Recordable,
    Transparent,
}

#[derive(Clone, Copy)]
enum ConsumedAuthorityAttack {
    Clear,
    Replace,
}

struct PlanHost {
    id: u64,
    mode: PlanHostMode,
    metadata_scope: PaintPropertyScope,
    full_scope: PaintPropertyScope,
    metadata: PlanShape,
    full: PlanShape,
    children: Vec<NodeKey>,
    deferred: bool,
    contents_scissor: Option<[u32; 4]>,
    consumed_authority_attack: Option<ConsumedAuthorityAttack>,
    clear_paint_offset_for_node: bool,
    clear_opacity_authority_for_node: bool,
    clear_opacity_authority_for_child: bool,
    required_opacity_authority: Option<super::super::PaintOpacityAuthority>,
}

impl PlanHost {
    fn recordable(id: u64, before: &[u16], after: &[u16]) -> Self {
        let shape = PlanShape::new(before, after);
        Self {
            id,
            mode: PlanHostMode::Recordable,
            metadata_scope: PaintPropertyScope::SelfPaint,
            full_scope: PaintPropertyScope::SelfPaint,
            metadata: shape.clone(),
            full: shape,
            children: Vec::new(),
            deferred: false,
            contents_scissor: None,
            consumed_authority_attack: None,
            clear_paint_offset_for_node: false,
            clear_opacity_authority_for_node: false,
            clear_opacity_authority_for_child: false,
            required_opacity_authority: None,
        }
    }

    fn transparent(id: u64) -> Self {
        Self {
            id,
            mode: PlanHostMode::Transparent,
            metadata_scope: PaintPropertyScope::SelfPaint,
            full_scope: PaintPropertyScope::SelfPaint,
            metadata: PlanShape::new(&[], &[]),
            full: PlanShape::new(&[], &[]),
            children: Vec::new(),
            deferred: false,
            contents_scissor: None,
            consumed_authority_attack: None,
            clear_paint_offset_for_node: false,
            clear_opacity_authority_for_node: false,
            clear_opacity_authority_for_child: false,
            required_opacity_authority: None,
        }
    }

    fn metadata_for(
        owner: NodeKey,
        properties: crate::view::compositor::property_tree::PropertyTreeState,
        revision: PaintContentRevision,
        scope: PaintPropertyScope,
        phase: PaintNodePhase,
        slot: u16,
    ) -> PaintChunkMetadata {
        PaintChunkMetadata {
            id: crate::view::paint::PaintChunkId {
                owner,
                scope,
                phase,
                slot,
                role: crate::view::paint::PaintChunkRole::SelfDecoration,
            },
            owner,
            bounds: crate::view::base_component::Rect {
                x: 0.0,
                y: 0.0,
                width: 1.0,
                height: 1.0,
            },
            properties,
            content_revision: revision,
            payload_identity: crate::view::paint::PaintPayloadIdentity::prepared_shadows(
                std::iter::empty::<&crate::view::paint::PreparedShadowOp>(),
            ),
        }
    }

    fn metadata_plan(
        shape: &PlanShape,
        owner: NodeKey,
        self_properties: crate::view::compositor::property_tree::PropertyTreeState,
        contents_properties: crate::view::compositor::property_tree::PropertyTreeState,
        revision: PaintContentRevision,
        scope: PaintPropertyScope,
    ) -> PaintNodePlan<PaintChunkMetadata> {
        let properties = match scope {
            PaintPropertyScope::SelfPaint => self_properties,
            PaintPropertyScope::Contents => contents_properties,
        };
        PaintNodePlan {
            before_children: shape
                .before
                .iter()
                .map(|&(phase, slot)| {
                    Self::metadata_for(owner, properties, revision, scope, phase, slot)
                })
                .collect(),
            after_children: shape
                .after
                .iter()
                .map(|&(phase, slot)| {
                    Self::metadata_for(owner, properties, revision, scope, phase, slot)
                })
                .collect(),
        }
    }

    fn artifact_plan(
        shape: &PlanShape,
        owner: NodeKey,
        self_properties: crate::view::compositor::property_tree::PropertyTreeState,
        contents_properties: crate::view::compositor::property_tree::PropertyTreeState,
        revision: PaintContentRevision,
        scope: PaintPropertyScope,
    ) -> PaintNodePlan<crate::view::paint::PaintArtifact> {
        let metadata = Self::metadata_plan(
            shape,
            owner,
            self_properties,
            contents_properties,
            revision,
            scope,
        );
        let artifact = |chunk: PaintChunkMetadata| crate::view::paint::PaintArtifact {
            target: Default::default(),
            chunks: vec![crate::view::paint::PaintChunk {
                id: chunk.id,
                owner: chunk.owner,
                op_range: 0..0,
                bounds: chunk.bounds,
                properties: chunk.properties,
                content_revision: chunk.content_revision,
                payload_identity: chunk.payload_identity,
            }],
            ops: Vec::new(),
            clip_nodes: Vec::new(),
            effect_nodes: Vec::new(),
            transform_nodes: Vec::new(),
            layout_position_nodes: Vec::new(),
            visual_offset_nodes: Vec::new(),
            scroll_nodes: Vec::new(),
            owner_property_states: Vec::new(),
            owner_nodes: Vec::new(),
        };
        PaintNodePlan {
            before_children: metadata
                .before_children
                .into_iter()
                .map(&artifact)
                .collect(),
            after_children: metadata.after_children.into_iter().map(artifact).collect(),
        }
    }
}

impl Layoutable for PlanHost {
    fn measure(&mut self, _constraints: LayoutConstraints, _arena: &mut NodeArena) {}
    fn place(&mut self, _placement: LayoutPlacement, _arena: &mut NodeArena) {}
    fn measured_size(&self) -> (f32, f32) {
        (1.0, 1.0)
    }
    fn set_layout_width(&mut self, _width: f32) {}
    fn set_layout_height(&mut self, _height: f32) {}
}

impl EventTarget for PlanHost {}

impl Renderable for PlanHost {
    fn build(
        &mut self,
        _graph: &mut FrameGraph,
        _arena: &mut NodeArena,
        ctx: UiBuildContext,
    ) -> BuildState {
        ctx.into_state()
    }
}

impl ElementTrait for PlanHost {
    fn stable_id(&self) -> u64 {
        self.id
    }

    fn box_model_snapshot(&self) -> BoxModelSnapshot {
        BoxModelSnapshot {
            node_id: self.id,
            parent_id: None,
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
            border_radius: 0.0,
            should_render: true,
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn shadow_paint_recording_capability(
        &self,
        _arena: &NodeArena,
        _deferred_phase_root: bool,
        recording_context: &PaintRecordingContext,
    ) -> ShadowPaintRecordingCapability {
        if self
            .required_opacity_authority
            .is_some_and(|required| recording_context.opacity_authority != required)
        {
            return ShadowPaintRecordingCapability::Unsupported;
        }
        match self.mode {
            PlanHostMode::Recordable => ShadowPaintRecordingCapability::Recordable,
            PlanHostMode::Transparent => ShadowPaintRecordingCapability::Transparent,
        }
    }

    fn record_shadow_paint_metadata_plan(
        &self,
        owner: NodeKey,
        properties: crate::view::compositor::property_tree::PropertyTreeState,
        contents_properties: crate::view::compositor::property_tree::PropertyTreeState,
        revision: PaintContentRevision,
        _arena: &NodeArena,
        _recording_context: &PaintRecordingContext,
    ) -> Option<PaintNodePlan<PaintChunkMetadata>> {
        (self.mode == PlanHostMode::Recordable).then(|| {
            Self::metadata_plan(
                &self.metadata,
                owner,
                properties,
                contents_properties,
                revision,
                self.metadata_scope,
            )
        })
    }

    fn record_shadow_paint_artifact_plan(
        &self,
        owner: NodeKey,
        properties: crate::view::compositor::property_tree::PropertyTreeState,
        contents_properties: crate::view::compositor::property_tree::PropertyTreeState,
        revision: PaintContentRevision,
        _arena: &NodeArena,
        _recording_context: &PaintRecordingContext,
    ) -> Option<PaintNodePlan<crate::view::paint::PaintArtifact>> {
        (self.mode == PlanHostMode::Recordable).then(|| {
            Self::artifact_plan(
                &self.full,
                owner,
                properties,
                contents_properties,
                revision,
                self.full_scope,
            )
        })
    }

    fn children(&self) -> &[NodeKey] {
        &self.children
    }

    fn sync_children_mirror(&mut self, children: &[NodeKey]) {
        self.children.clear();
        self.children.extend_from_slice(children);
    }

    fn is_deferred_to_root_viewport_render(&self) -> bool {
        self.deferred
    }

    fn shadow_paint_recording_context(
        &self,
        parent: &PaintRecordingContext,
    ) -> PaintRecordingContext {
        let mut parent = *parent;
        if self.clear_paint_offset_for_node {
            parent.paint_offset = [0.0, 0.0];
        }
        if self.clear_opacity_authority_for_node {
            parent.opacity_authority = super::super::PaintOpacityAuthority::Baked;
        }
        parent
    }

    fn contents_logical_scissor(&self) -> Option<[u32; 4]> {
        self.contents_scissor
    }

    fn shadow_paint_recording_context_for_child(
        &self,
        child: NodeKey,
        _arena: &NodeArena,
        parent: &PaintRecordingContext,
    ) -> PaintRecordingContext {
        let mut parent = *parent;
        match self.consumed_authority_attack {
            None => {}
            Some(ConsumedAuthorityAttack::Clear) => {
                parent.consumed_ancestor_property = None;
            }
            Some(ConsumedAuthorityAttack::Replace) => {
                parent.consumed_ancestor_property =
                    Some(super::super::ConsumedAncestorProperty::Transform(
                        super::super::ConsumedAncestorTransformWitness {
                            parent_boundary: child,
                            child_boundary: child,
                            transform: TransformNodeId(child),
                            target_owner: child,
                        },
                    ));
            }
        }
        if self.clear_opacity_authority_for_child {
            parent.opacity_authority = super::super::PaintOpacityAuthority::Baked;
        }
        parent
    }
}

fn insert(arena: &mut NodeArena, id: u64) -> NodeKey {
    let mut element = Element::new_with_id(id, 0.0, 0.0, 10.0, 10.0);
    let mut style = Style::new();
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    element.apply_style(style);
    arena.insert(Node::new(Box::new(element)))
}

fn append(arena: &mut NodeArena, parent: NodeKey, child: NodeKey) {
    arena.set_parent(child, Some(parent));
    arena.push_child(parent, child);
}

fn identity(arena: &NodeArena, roots: &[NodeKey]) -> (PropertyTrees, PaintGenerationTracker) {
    let mut properties = PropertyTrees::default();
    properties.sync(arena, roots);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(arena, roots, &properties);
    (properties, generations)
}

fn record(arena: &NodeArena, roots: &[NodeKey], force_legacy_roots: bool) -> PaintCoverageManifest {
    let (properties, generations) = identity(arena, roots);
    record_coverage_manifest(
        arena,
        roots,
        force_legacy_roots,
        true,
        CoverageRecordingMode::MetadataOnly,
        &properties,
        &generations,
    )
}

fn insert_plan(arena: &mut NodeArena, host: PlanHost) -> NodeKey {
    arena.insert(Node::new(Box::new(host)))
}

fn deferred_element(id: u64) -> Element {
    let mut element = Element::new_with_id(id, 0.0, 0.0, 10.0, 10.0);
    let mut style = Style::new();
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .left(Length::px(0.0))
                .clip(ClipMode::Viewport),
        ),
    );
    element.apply_style(style);
    element
}

mod fallback_boundary_tests;
mod recording_authority_tests;
mod recording_order_tests;
mod validation_tests;
