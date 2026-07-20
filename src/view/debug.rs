//! Read-only debug capture and query APIs.
//!
//! The debug surface snapshots the live viewport into a stable capture first,
//! then answers interactive queries against that capture. This keeps inspector
//! reads consistent while keeping [`crate::view::node_arena::NodeArena`] as an
//! engine storage detail rather than the public document model.

use std::collections::HashMap;

use bitflags::bitflags;

use crate::view::base_component::{BoxModelSnapshot, DirtyFlags};
use crate::view::node_arena::{NodeArena, NodeKey};
use crate::view::viewport::PointerButton;

bitflags! {
    /// Selects the element-scoped diagnostics attached to an `Element`.
    ///
    /// Concrete flags are intentionally added together with their matching
    /// debug output path. An empty value is the default and carries no debug
    /// behavior.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct DebugType: u32 {}
}

impl crate::ui::IntoPropValue for DebugType {
    fn into_prop_value(self) -> crate::ui::PropValue {
        crate::ui::PropValue::I64(i64::from(self.bits()))
    }
}

impl crate::ui::FromPropValue for DebugType {
    fn from_prop_value(value: crate::ui::PropValue) -> Result<Self, String> {
        match value {
            crate::ui::PropValue::I64(bits) => u32::try_from(bits)
                .map(Self::from_bits_retain)
                .map_err(|_| "expected DebugType bits".to_string()),
            _ => Err("expected DebugType value".to_string()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DebugCaptureOptions {
    pub include_arena: bool,
    pub include_layout: bool,
    pub include_style: bool,
    pub include_interaction: bool,
    pub include_dirty: bool,
    pub include_render: bool,
    /// Captures the last RetainedAuto authority attempt supplied by the
    /// viewport. Callers should avoid constructing the integration input when
    /// this is false; debug capture performs no RetainedAuto-only traversal.
    pub include_retained_auto: bool,
    pub include_component: bool,
}

impl Default for DebugCaptureOptions {
    fn default() -> Self {
        Self {
            include_arena: true,
            include_layout: true,
            include_style: false,
            include_interaction: true,
            include_dirty: true,
            include_render: true,
            include_retained_auto: false,
            include_component: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DebugNodeId(String);

impl DebugNodeId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for DebugNodeId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DebugArenaNodeId(String);

impl DebugArenaNodeId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug)]
pub struct DebugCapture {
    options: DebugCaptureOptions,
    document: DebugDocumentCapture,
    arena: Option<DebugArenaCapture>,
    index: DebugCaptureIndex,
}

#[derive(Clone, Debug)]
pub struct DebugDocumentCapture {
    pub roots: Vec<DebugNodeId>,
    pub node_count: usize,
    pub viewport: DebugViewportSnapshot,
}

#[derive(Clone, Debug)]
pub struct DebugArenaCapture {
    pub nodes: Vec<DebugArenaNodeSnapshot>,
}

#[derive(Clone, Debug)]
pub struct DebugViewportSnapshot {
    pub logical_width: f32,
    pub logical_height: f32,
    pub scale_factor: f32,
    pub focused_node: Option<DebugNodeId>,
    pub hovered_node: Option<DebugNodeId>,
    pub pointer_capture_node: Option<DebugNodeId>,
    pub keyboard_capture_node: Option<DebugNodeId>,
    pub pointer_position: Option<(f32, f32)>,
    pub pressed_pointer_buttons: Vec<PointerButton>,
    /// Last completed or rejected RetainedAuto attempt known to the viewport.
    /// This is independent from the frame currently visible on screen.
    pub retained_auto: Option<DebugRetainedAutoSnapshot>,
}

#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DebugPaintRequestedMode {
    Legacy,
    ArtifactCanary,
    RetainedTransformCanary,
    RetainedSurfaceTreeCanary,
    RetainedIsolationCanary,
    RetainedEffectTreeCanary,
    RetainedScrollHostCanary,
    RetainedScrollSceneCanary,
    RetainedAuto,
}

#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DebugFramePaintAuthority {
    /// Selection failed before any paint authority was committed.
    Unselected,
    Legacy,
    Artifact,
    PropertyScene,
    RetainedTransformSurface,
    RetainedEffectSurface,
    RetainedScrollHost,
    RetainedScrollScene,
}

#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DebugFrameDisposition {
    Presented,
    FellBackToLegacy,
    Rejected,
    Aborted,
}

#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DebugFallbackStage {
    Selection,
    Planning,
    Recording,
    Preparation,
    Compilation,
    Execution,
    Terminal,
}

#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DebugFallbackCategory {
    UnsupportedHost,
    PropertyTopology,
    DeferredPaint,
    LayoutTransition,
    Coverage,
    Validation,
    Resource,
    Capacity,
    Compiler,
    Runtime,
    ForcedFailure,
    Unknown,
}

#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DebugFallbackDetail {
    None,
    Code {
        code: &'static str,
    },
    Boundary {
        reason: &'static str,
    },
    Validation {
        invariant: &'static str,
    },
    Resource {
        resource: &'static str,
    },
    Capacity {
        resource: &'static str,
        requested: u64,
        limit: u64,
    },
}

#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DebugCoverageKind {
    ArtifactChunk,
    PropertySurface,
    RetainedSurface,
    LegacyBoundary,
    Transparent,
    Culled,
    Uncovered,
}

#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DebugSurfaceKind {
    Transform,
    Effect,
    Isolation,
    ScrollHost,
    ScrollContent,
    RootOpacityGroup,
}

#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DebugResidentAction {
    None,
    Commit,
    Reuse,
    Reraster,
    Clear,
}

#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub struct DebugRetainedAutoFallbackSnapshot {
    pub stage: DebugFallbackStage,
    pub category: DebugFallbackCategory,
    pub detail: DebugFallbackDetail,
    pub node: Option<DebugNodeId>,
    pub stable_id: Option<u64>,
    pub element_type: Option<&'static str>,
    pub bounds: Option<DebugRect>,
}

#[non_exhaustive]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DebugRetainedAutoStatistics {
    pub reachable_nodes: u64,
    pub covered_nodes: u64,
    pub artifact_chunks: u64,
    pub property_surfaces: u64,
    pub retained_surfaces: u64,
    pub legacy_nodes: u64,
    pub culled_nodes: u64,
    pub fallback_count: u64,
    pub resident_commits: u64,
    pub resident_reuses: u64,
    pub resident_rerasterizations: u64,
}

#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub struct DebugRetainedAutoNodeSnapshot {
    pub node: Option<DebugNodeId>,
    pub stable_id: Option<u64>,
    pub element_type: &'static str,
    pub bounds: Option<DebugRect>,
    pub coverage: Vec<DebugCoverageKind>,
    pub resident_action: Option<DebugResidentAction>,
    pub fallbacks: Vec<DebugRetainedAutoFallbackSnapshot>,
}

#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub struct DebugRetainedAutoSurfaceSnapshot {
    pub owner: Option<DebugNodeId>,
    pub stable_id: Option<u64>,
    pub element_type: &'static str,
    pub bounds: Option<DebugRect>,
    pub kind: DebugSurfaceKind,
    pub coverage: DebugCoverageKind,
    pub resident_action: DebugResidentAction,
}

#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub struct DebugRetainedAutoFrameSnapshot {
    pub attempt_id: u64,
    pub requested_mode: DebugPaintRequestedMode,
    pub selected_authority: DebugFramePaintAuthority,
    pub disposition: DebugFrameDisposition,
    pub fallback_stages: Vec<DebugRetainedAutoFallbackSnapshot>,
    pub statistics: DebugRetainedAutoStatistics,
}

#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub struct DebugRetainedAutoSnapshot {
    pub frame: DebugRetainedAutoFrameSnapshot,
    pub nodes: Vec<DebugRetainedAutoNodeSnapshot>,
    pub surfaces: Vec<DebugRetainedAutoSurfaceSnapshot>,
}

#[derive(Clone, Debug)]
pub struct DebugNodeSummary {
    pub id: DebugNodeId,
    pub arena_id: Option<DebugArenaNodeId>,
    pub parent: Option<DebugNodeId>,
    pub children: Vec<DebugNodeId>,
    pub depth: usize,
    pub index_in_parent: Option<usize>,
    pub element_type: &'static str,
    pub stable_id: Option<u64>,
}

#[derive(Clone, Debug)]
pub struct DebugArenaNodeSnapshot {
    pub id: DebugArenaNodeId,
    pub document_node: DebugNodeId,
    pub parent: Option<DebugArenaNodeId>,
    pub children: Vec<DebugArenaNodeId>,
    pub element_type: &'static str,
    pub stable_id: Option<u64>,
    pub local_dirty: DebugDirtyFlags,
    pub arena_local_dirty: DebugDirtyFlags,
    pub cached_subtree_dirty: DebugDirtyFlags,
}

#[derive(Clone, Debug)]
pub struct DebugElementState {
    pub identity: DebugElementIdentity,
    pub tree: DebugTreeState,
    pub layout: Option<DebugLayoutState>,
    pub style: Option<DebugStyleState>,
    pub interaction: Option<DebugInteractionState>,
    pub dirty: Option<DebugDirtyState>,
    pub render: Option<DebugRenderState>,
    pub component: Option<DebugComponentState>,
    pub arena: Option<DebugArenaNodeSummary>,
}

#[derive(Clone, Debug)]
pub struct DebugElementIdentity {
    pub node: DebugNodeId,
    pub arena_id: Option<DebugArenaNodeId>,
    pub element_type: &'static str,
    pub stable_id: Option<u64>,
}

#[derive(Clone, Debug)]
pub struct DebugTreeState {
    pub parent: Option<DebugNodeId>,
    pub children: Vec<DebugNodeId>,
    pub root: DebugNodeId,
    pub depth: usize,
    pub index_in_parent: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DebugRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl DebugRect {
    fn contains(self, x: f32, y: f32) -> bool {
        x >= self.x && y >= self.y && x <= self.x + self.width && y <= self.y + self.height
    }
}

#[derive(Clone, Debug)]
pub struct DebugLayoutState {
    pub rect: DebugRect,
    pub border_radius: f32,
    pub should_render: bool,
    pub scroll_offset: (f32, f32),
}

#[derive(Clone, Debug)]
pub struct DebugStyleState {
    pub available: bool,
}

#[derive(Clone, Debug)]
pub struct DebugInteractionState {
    pub focused: bool,
    pub hovered: bool,
    pub pointer_captured: bool,
    pub keyboard_captured: bool,
}

#[derive(Clone, Debug)]
pub struct DebugDirtyState {
    pub local: DebugDirtyFlags,
    pub arena_local: DebugDirtyFlags,
    pub subtree: DebugDirtyFlags,
}

#[derive(Clone, Debug)]
pub struct DebugDirtyFlags {
    pub layout: bool,
    pub place: bool,
    pub box_model: bool,
    pub hit_test: bool,
    pub paint: bool,
    pub composite: bool,
}

impl DebugDirtyFlags {
    fn from_flags(flags: DirtyFlags) -> Self {
        Self {
            layout: flags.contains(DirtyFlags::LAYOUT),
            place: flags.contains(DirtyFlags::PLACE),
            box_model: flags.contains(DirtyFlags::BOX_MODEL),
            hit_test: flags.contains(DirtyFlags::HIT_TEST),
            paint: flags.contains(DirtyFlags::PAINT),
            composite: flags.contains(DirtyFlags::COMPOSITE),
        }
    }

    pub fn is_empty(&self) -> bool {
        !self.layout
            && !self.place
            && !self.box_model
            && !self.hit_test
            && !self.paint
            && !self.composite
    }
}

#[derive(Clone, Debug)]
pub struct DebugRenderState {
    pub should_render: bool,
    pub retained_auto: Option<DebugRetainedAutoNodeSnapshot>,
}

#[derive(Clone, Debug)]
pub struct DebugComponentState {
    pub available: bool,
}

#[derive(Clone, Debug)]
pub struct DebugArenaNodeSummary {
    pub id: DebugArenaNodeId,
    pub parent: Option<DebugArenaNodeId>,
    pub children: Vec<DebugArenaNodeId>,
}

#[derive(Clone, Debug)]
pub enum DebugQuery {
    GetDocument,
    GetNode {
        node: DebugNodeId,
    },
    GetChildren {
        node: DebugNodeId,
    },
    GetAncestors {
        node: DebugNodeId,
    },
    GetSubtree {
        node: DebugNodeId,
        depth: Option<u32>,
    },
    PickNode {
        x: f32,
        y: f32,
    },
    GetElementState {
        node: DebugNodeId,
    },
    GetLayout {
        node: DebugNodeId,
    },
    GetBoxModel {
        node: DebugNodeId,
    },
    GetComputedStyle {
        node: DebugNodeId,
    },
    GetInteractionState {
        node: DebugNodeId,
    },
    GetDirtyState {
        node: DebugNodeId,
    },
    GetRenderState {
        node: DebugNodeId,
    },
    GetArenaNode {
        node: DebugNodeId,
    },
}

#[derive(Clone, Debug)]
pub enum DebugResponse {
    Document(DebugDocumentCapture),
    Node(DebugNodeSummary),
    Nodes(Vec<DebugNodeSummary>),
    Pick(Option<DebugNodeId>),
    ElementState(DebugElementState),
    Layout(Option<DebugLayoutState>),
    BoxModel(Option<DebugLayoutState>),
    ComputedStyle(Option<DebugStyleState>),
    InteractionState(Option<DebugInteractionState>),
    DirtyState(Option<DebugDirtyState>),
    RenderState(Option<DebugRenderState>),
    ArenaNode(Option<DebugArenaNodeSnapshot>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DebugError {
    UnknownNode(DebugNodeId),
}

pub type DebugResult<T> = Result<T, DebugError>;

#[derive(Clone, Debug)]
pub(crate) struct DebugViewportCaptureInput {
    pub logical_size: (f32, f32),
    pub scale_factor: f32,
    pub focused_node: Option<NodeKey>,
    pub hovered_node: Option<NodeKey>,
    pub pointer_capture_node: Option<NodeKey>,
    pub keyboard_capture_node: Option<NodeKey>,
    pub pointer_position: Option<(f32, f32)>,
    pub pressed_pointer_buttons: Vec<PointerButton>,
}

/// Single handoff object produced by the viewport's retained paint telemetry.
/// The viewport should construct this only when
/// [`DebugCaptureOptions::include_retained_auto`] is enabled.
#[derive(Clone, Debug)]
pub(crate) struct DebugRetainedAutoCaptureInput {
    pub frame: DebugRetainedAutoFrameCaptureInput,
    pub nodes: Vec<DebugRetainedAutoNodeCaptureInput>,
    pub surfaces: Vec<DebugRetainedAutoSurfaceCaptureInput>,
}

#[derive(Clone, Debug)]
pub(crate) struct DebugRetainedAutoFrameCaptureInput {
    pub attempt_id: u64,
    pub requested_mode: DebugPaintRequestedMode,
    pub selected_authority: DebugFramePaintAuthority,
    pub disposition: DebugFrameDisposition,
    pub fallback_stages: Vec<DebugRetainedAutoFallbackCaptureInput>,
    pub statistics: DebugRetainedAutoStatistics,
}

#[derive(Clone, Debug)]
pub(crate) struct DebugRetainedAutoFallbackCaptureInput {
    pub stage: DebugFallbackStage,
    pub category: DebugFallbackCategory,
    pub detail: DebugFallbackDetail,
    pub owner: Option<NodeKey>,
    pub stable_id: Option<u64>,
    pub element_type: Option<&'static str>,
    pub bounds: Option<DebugRect>,
}

#[derive(Clone, Debug)]
pub(crate) struct DebugRetainedAutoNodeCaptureInput {
    pub owner: Option<NodeKey>,
    pub stable_id: Option<u64>,
    pub element_type: &'static str,
    pub bounds: Option<DebugRect>,
    pub coverage: Vec<DebugCoverageKind>,
    pub resident_action: Option<DebugResidentAction>,
    pub fallbacks: Vec<DebugRetainedAutoFallbackCaptureInput>,
}

#[derive(Clone, Debug)]
pub(crate) struct DebugRetainedAutoSurfaceCaptureInput {
    pub owner: Option<NodeKey>,
    pub stable_id: Option<u64>,
    pub element_type: &'static str,
    pub bounds: Option<DebugRect>,
    pub kind: DebugSurfaceKind,
    pub coverage: DebugCoverageKind,
    pub resident_action: DebugResidentAction,
}

#[derive(Clone, Debug)]
struct CapturedNode {
    summary: DebugNodeSummary,
    root: DebugNodeId,
    layout: Option<DebugLayoutState>,
    interaction: Option<DebugInteractionState>,
    dirty: Option<DebugDirtyState>,
    render: Option<DebugRenderState>,
    arena: Option<DebugArenaNodeSnapshot>,
}

#[derive(Clone, Debug, Default)]
struct DebugCaptureIndex {
    nodes: HashMap<DebugNodeId, CapturedNode>,
    child_order: HashMap<DebugNodeId, Vec<DebugNodeId>>,
    roots: Vec<DebugNodeId>,
}

impl DebugCapture {
    #[allow(dead_code)] // Compatibility entry point for callers without retained paint telemetry.
    pub(crate) fn from_arena(
        options: DebugCaptureOptions,
        arena: &NodeArena,
        roots: &[NodeKey],
        viewport: DebugViewportCaptureInput,
    ) -> Self {
        Self::from_arena_with_retained_auto(options, arena, roots, viewport, None)
    }

    /// Integration entry point for the viewport. Passing `None`, or disabling
    /// `include_retained_auto`, preserves the ordinary capture cost and does
    /// not perform a RetainedAuto-specific arena traversal.
    pub(crate) fn from_arena_with_retained_auto(
        options: DebugCaptureOptions,
        arena: &NodeArena,
        roots: &[NodeKey],
        viewport: DebugViewportCaptureInput,
        retained_auto: Option<DebugRetainedAutoCaptureInput>,
    ) -> Self {
        let retained_auto = options
            .include_retained_auto
            .then_some(retained_auto)
            .flatten();
        let mut builder = DebugCaptureBuilder::new(options, viewport, retained_auto);
        for root in roots {
            builder.capture_node(arena, *root, None, None, 0, *root);
        }
        builder.finish()
    }

    pub fn document(&self) -> &DebugDocumentCapture {
        &self.document
    }

    pub fn arena(&self) -> Option<&DebugArenaCapture> {
        self.arena.as_ref()
    }

    pub fn query(&self, query: DebugQuery) -> DebugResult<DebugResponse> {
        match query {
            DebugQuery::GetDocument => Ok(DebugResponse::Document(self.document.clone())),
            DebugQuery::GetNode { node } => {
                Ok(DebugResponse::Node(self.node(&node)?.summary.clone()))
            }
            DebugQuery::GetChildren { node } => {
                let children = self
                    .node(&node)?
                    .summary
                    .children
                    .iter()
                    .filter_map(|id| self.index.nodes.get(id).map(|node| node.summary.clone()))
                    .collect();
                Ok(DebugResponse::Nodes(children))
            }
            DebugQuery::GetAncestors { node } => {
                let mut out = Vec::new();
                let mut next = self.node(&node)?.summary.parent.clone();
                while let Some(id) = next {
                    let ancestor = self.node(&id)?;
                    out.push(ancestor.summary.clone());
                    next = ancestor.summary.parent.clone();
                }
                Ok(DebugResponse::Nodes(out))
            }
            DebugQuery::GetSubtree { node, depth } => {
                self.node(&node)?;
                let mut out = Vec::new();
                self.collect_subtree(&node, depth.unwrap_or(u32::MAX), &mut out);
                Ok(DebugResponse::Nodes(out))
            }
            DebugQuery::PickNode { x, y } => Ok(DebugResponse::Pick(self.pick_node(x, y))),
            DebugQuery::GetElementState { node } => {
                Ok(DebugResponse::ElementState(self.element_state(&node)?))
            }
            DebugQuery::GetLayout { node } => {
                Ok(DebugResponse::Layout(self.node(&node)?.layout.clone()))
            }
            DebugQuery::GetBoxModel { node } => {
                Ok(DebugResponse::BoxModel(self.node(&node)?.layout.clone()))
            }
            DebugQuery::GetComputedStyle { node } => {
                self.node(&node)?;
                Ok(DebugResponse::ComputedStyle(
                    self.options
                        .include_style
                        .then_some(DebugStyleState { available: false }),
                ))
            }
            DebugQuery::GetInteractionState { node } => Ok(DebugResponse::InteractionState(
                self.node(&node)?.interaction.clone(),
            )),
            DebugQuery::GetDirtyState { node } => {
                Ok(DebugResponse::DirtyState(self.node(&node)?.dirty.clone()))
            }
            DebugQuery::GetRenderState { node } => {
                Ok(DebugResponse::RenderState(self.node(&node)?.render.clone()))
            }
            DebugQuery::GetArenaNode { node } => {
                Ok(DebugResponse::ArenaNode(self.node(&node)?.arena.clone()))
            }
        }
    }

    fn node(&self, node: &DebugNodeId) -> DebugResult<&CapturedNode> {
        self.index
            .nodes
            .get(node)
            .ok_or_else(|| DebugError::UnknownNode(node.clone()))
    }

    fn element_state(&self, node: &DebugNodeId) -> DebugResult<DebugElementState> {
        let captured = self.node(node)?;
        Ok(DebugElementState {
            identity: DebugElementIdentity {
                node: captured.summary.id.clone(),
                arena_id: captured.summary.arena_id.clone(),
                element_type: captured.summary.element_type,
                stable_id: captured.summary.stable_id,
            },
            tree: DebugTreeState {
                parent: captured.summary.parent.clone(),
                children: captured.summary.children.clone(),
                root: captured.root.clone(),
                depth: captured.summary.depth,
                index_in_parent: captured.summary.index_in_parent,
            },
            layout: captured.layout.clone(),
            style: self
                .options
                .include_style
                .then_some(DebugStyleState { available: false }),
            interaction: captured.interaction.clone(),
            dirty: captured.dirty.clone(),
            render: captured.render.clone(),
            component: self
                .options
                .include_component
                .then_some(DebugComponentState { available: false }),
            arena: captured.arena.as_ref().map(|arena| DebugArenaNodeSummary {
                id: arena.id.clone(),
                parent: arena.parent.clone(),
                children: arena.children.clone(),
            }),
        })
    }

    fn collect_subtree(&self, node: &DebugNodeId, depth: u32, out: &mut Vec<DebugNodeSummary>) {
        let Some(captured) = self.index.nodes.get(node) else {
            return;
        };
        out.push(captured.summary.clone());
        if depth == 0 {
            return;
        }
        if let Some(children) = self.index.child_order.get(node) {
            for child in children {
                self.collect_subtree(child, depth - 1, out);
            }
        }
    }

    fn pick_node(&self, x: f32, y: f32) -> Option<DebugNodeId> {
        for root in self.index.roots.iter().rev() {
            if let Some(hit) = self.pick_in_subtree(root, x, y) {
                return Some(hit);
            }
        }
        None
    }

    fn pick_in_subtree(&self, node: &DebugNodeId, x: f32, y: f32) -> Option<DebugNodeId> {
        if let Some(children) = self.index.child_order.get(node) {
            for child in children.iter().rev() {
                if let Some(hit) = self.pick_in_subtree(child, x, y) {
                    return Some(hit);
                }
            }
        }
        let captured = self.index.nodes.get(node)?;
        let layout = captured.layout.as_ref()?;
        if layout.should_render && layout.rect.contains(x, y) {
            Some(captured.summary.id.clone())
        } else {
            None
        }
    }
}

struct DebugCaptureBuilder {
    options: DebugCaptureOptions,
    viewport: DebugViewportCaptureInput,
    index: DebugCaptureIndex,
    arena_nodes: Vec<DebugArenaNodeSnapshot>,
    key_to_node: HashMap<NodeKey, DebugNodeId>,
    key_to_arena: HashMap<NodeKey, DebugArenaNodeId>,
    retained_auto: Option<DebugRetainedAutoCaptureInput>,
}

impl DebugCaptureBuilder {
    fn new(
        options: DebugCaptureOptions,
        viewport: DebugViewportCaptureInput,
        retained_auto: Option<DebugRetainedAutoCaptureInput>,
    ) -> Self {
        Self {
            options,
            viewport,
            index: DebugCaptureIndex::default(),
            arena_nodes: Vec::new(),
            key_to_node: HashMap::new(),
            key_to_arena: HashMap::new(),
            retained_auto,
        }
    }

    fn finish(mut self) -> DebugCapture {
        let retained_auto = self.retained_auto.take().map(|input| {
            let frame = DebugRetainedAutoFrameSnapshot {
                attempt_id: input.frame.attempt_id,
                requested_mode: input.frame.requested_mode,
                selected_authority: input.frame.selected_authority,
                disposition: input.frame.disposition,
                fallback_stages: input
                    .frame
                    .fallback_stages
                    .into_iter()
                    .map(|fallback| self.resolve_retained_auto_fallback(fallback))
                    .collect(),
                statistics: input.frame.statistics,
            };
            let mut nodes = Vec::with_capacity(input.nodes.len());
            for node in input.nodes {
                let owner = node.owner;
                let snapshot = DebugRetainedAutoNodeSnapshot {
                    node: owner.and_then(|owner| self.key_to_node.get(&owner).cloned()),
                    stable_id: node.stable_id,
                    element_type: node.element_type,
                    bounds: node.bounds,
                    coverage: node.coverage,
                    resident_action: node.resident_action,
                    fallbacks: node
                        .fallbacks
                        .into_iter()
                        .map(|fallback| self.resolve_retained_auto_fallback(fallback))
                        .collect(),
                };
                if let Some(id) = snapshot.node.as_ref()
                    && let Some(render) = self
                        .index
                        .nodes
                        .get_mut(id)
                        .and_then(|captured| captured.render.as_mut())
                {
                    render.retained_auto = Some(snapshot.clone());
                }
                nodes.push(snapshot);
            }
            let surfaces = input
                .surfaces
                .into_iter()
                .map(|surface| DebugRetainedAutoSurfaceSnapshot {
                    owner: surface
                        .owner
                        .and_then(|owner| self.key_to_node.get(&owner).cloned()),
                    stable_id: surface.stable_id,
                    element_type: surface.element_type,
                    bounds: surface.bounds,
                    kind: surface.kind,
                    coverage: surface.coverage,
                    resident_action: surface.resident_action,
                })
                .collect();
            DebugRetainedAutoSnapshot {
                frame,
                nodes,
                surfaces,
            }
        });
        let viewport = DebugViewportSnapshot {
            logical_width: self.viewport.logical_size.0,
            logical_height: self.viewport.logical_size.1,
            scale_factor: self.viewport.scale_factor,
            focused_node: self.resolve_node(self.viewport.focused_node),
            hovered_node: self.resolve_node(self.viewport.hovered_node),
            pointer_capture_node: self.resolve_node(self.viewport.pointer_capture_node),
            keyboard_capture_node: self.resolve_node(self.viewport.keyboard_capture_node),
            pointer_position: self.viewport.pointer_position,
            pressed_pointer_buttons: self.viewport.pressed_pointer_buttons,
            retained_auto,
        };
        let document = DebugDocumentCapture {
            roots: self.index.roots.clone(),
            node_count: self.index.nodes.len(),
            viewport,
        };
        DebugCapture {
            options: self.options,
            document,
            arena: self.options.include_arena.then_some(DebugArenaCapture {
                nodes: self.arena_nodes,
            }),
            index: self.index,
        }
    }

    fn resolve_retained_auto_fallback(
        &self,
        input: DebugRetainedAutoFallbackCaptureInput,
    ) -> DebugRetainedAutoFallbackSnapshot {
        DebugRetainedAutoFallbackSnapshot {
            stage: input.stage,
            category: input.category,
            detail: input.detail,
            node: input
                .owner
                .and_then(|owner| self.key_to_node.get(&owner).cloned()),
            stable_id: input.stable_id,
            element_type: input.element_type,
            bounds: input.bounds,
        }
    }

    fn capture_node(
        &mut self,
        arena: &NodeArena,
        key: NodeKey,
        parent_node: Option<DebugNodeId>,
        parent_arena: Option<DebugArenaNodeId>,
        depth: usize,
        root_key: NodeKey,
    ) -> Option<DebugNodeId> {
        if let Some(existing) = self.key_to_node.get(&key) {
            return Some(existing.clone());
        }
        let node_ref = arena.get(key)?;
        let element = node_ref.element.as_ref();
        let id = DebugNodeId(format!("node-{}", self.key_to_node.len()));
        let arena_id = DebugArenaNodeId(format!("arena-{:?}", key));
        self.key_to_node.insert(key, id.clone());
        self.key_to_arena.insert(key, arena_id.clone());
        if parent_node.is_none() {
            self.index.roots.push(id.clone());
        }
        let stable_id = non_zero_stable_id(element.stable_id());
        let children_keys = node_ref.children.clone();
        let root = if key == root_key {
            id.clone()
        } else {
            self.key_to_node
                .get(&root_key)
                .cloned()
                .unwrap_or_else(|| id.clone())
        };
        let snapshot = element.box_model_snapshot();
        let scroll_offset = element.get_scroll_offset();
        let layout = self
            .options
            .include_layout
            .then(|| layout_state_from_snapshot(snapshot, scroll_offset));
        let local_dirty = DebugDirtyFlags::from_flags(element.local_dirty_flags());
        let arena_local_dirty = DebugDirtyFlags::from_flags(node_ref.arena_local_dirty.get());
        let cached_subtree_dirty = DebugDirtyFlags::from_flags(node_ref.cached_subtree_dirty.get());
        let interaction = self
            .options
            .include_interaction
            .then(|| self.interaction_state_for(key));
        let dirty = self.options.include_dirty.then(|| DebugDirtyState {
            local: local_dirty.clone(),
            arena_local: arena_local_dirty.clone(),
            subtree: cached_subtree_dirty.clone(),
        });
        let render = self.options.include_render.then_some(DebugRenderState {
            should_render: snapshot.should_render,
            retained_auto: None,
        });

        let summary = DebugNodeSummary {
            id: id.clone(),
            arena_id: self.options.include_arena.then_some(arena_id.clone()),
            parent: parent_node.clone(),
            children: Vec::new(),
            depth,
            index_in_parent: None,
            element_type: element.element_type_name(),
            stable_id,
        };
        let captured = CapturedNode {
            summary,
            root,
            layout,
            interaction,
            dirty,
            render,
            arena: None,
        };
        self.index.nodes.insert(id.clone(), captured);
        drop(node_ref);

        let mut child_nodes = Vec::new();
        let mut child_arena_ids = Vec::new();
        for (index, child_key) in children_keys.iter().copied().enumerate() {
            if let Some(child_id) = self.capture_node(
                arena,
                child_key,
                Some(id.clone()),
                Some(arena_id.clone()),
                depth + 1,
                root_key,
            ) {
                if let Some(child) = self.index.nodes.get_mut(&child_id) {
                    child.summary.index_in_parent = Some(index);
                }
                if let Some(child_arena_id) = self.key_to_arena.get(&child_key) {
                    child_arena_ids.push(child_arena_id.clone());
                }
                child_nodes.push(child_id);
            }
        }

        self.index
            .child_order
            .insert(id.clone(), child_nodes.clone());
        if let Some(captured) = self.index.nodes.get_mut(&id) {
            captured.summary.children = child_nodes;
            if self.options.include_arena {
                captured.arena = Some(DebugArenaNodeSnapshot {
                    id: arena_id.clone(),
                    document_node: id.clone(),
                    parent: parent_arena.clone(),
                    children: child_arena_ids.clone(),
                    element_type: captured.summary.element_type,
                    stable_id,
                    local_dirty,
                    arena_local_dirty,
                    cached_subtree_dirty,
                });
                if let Some(arena_snapshot) = captured.arena.clone() {
                    self.arena_nodes.push(arena_snapshot);
                }
            }
        }

        Some(id)
    }

    fn interaction_state_for(&self, key: NodeKey) -> DebugInteractionState {
        DebugInteractionState {
            focused: self.viewport.focused_node == Some(key),
            hovered: self.viewport.hovered_node == Some(key),
            pointer_captured: self.viewport.pointer_capture_node == Some(key),
            keyboard_captured: self.viewport.keyboard_capture_node == Some(key),
        }
    }

    fn resolve_node(&self, key: Option<NodeKey>) -> Option<DebugNodeId> {
        key.and_then(|key| self.key_to_node.get(&key).cloned())
    }
}

fn layout_state_from_snapshot(
    snapshot: BoxModelSnapshot,
    scroll_offset: (f32, f32),
) -> DebugLayoutState {
    DebugLayoutState {
        rect: DebugRect {
            x: snapshot.x,
            y: snapshot.y,
            width: snapshot.width,
            height: snapshot.height,
        },
        border_radius: snapshot.border_radius,
        should_render: snapshot.should_render,
        scroll_offset,
    }
}

fn non_zero_stable_id(id: u64) -> Option<u64> {
    (id != 0).then_some(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::base_component::Element;
    use crate::view::node_arena::Node;

    fn sample_capture() -> (DebugCapture, DebugNodeId, DebugNodeId) {
        let mut arena = NodeArena::new();
        let root = arena.insert(Node::new(Box::new(Element::new_with_id(
            10, 0.0, 0.0, 100.0, 80.0,
        ))));
        let child = arena.insert(Node::with_parent(
            Box::new(Element::new_with_id(20, 10.0, 12.0, 20.0, 16.0)),
            Some(root),
        ));
        arena.push_child(root, child);
        arena.set_roots(vec![root]);
        arena.refresh_subtree_dirty_cache(root);

        let capture = DebugCapture::from_arena(
            DebugCaptureOptions::default(),
            &arena,
            &[root],
            DebugViewportCaptureInput {
                logical_size: (320.0, 240.0),
                scale_factor: 2.0,
                focused_node: Some(child),
                hovered_node: Some(root),
                pointer_capture_node: None,
                keyboard_capture_node: Some(child),
                pointer_position: Some((12.0, 14.0)),
                pressed_pointer_buttons: Vec::new(),
            },
        );
        let root_id = capture.document.roots[0].clone();
        let child_id = match capture
            .query(DebugQuery::GetChildren {
                node: root_id.clone(),
            })
            .unwrap()
        {
            DebugResponse::Nodes(nodes) => nodes[0].id.clone(),
            response => panic!("unexpected response: {response:?}"),
        };
        (capture, root_id, child_id)
    }

    #[test]
    fn document_capture_keeps_document_and_arena_ids_separate() {
        let (capture, root_id, child_id) = sample_capture();

        assert_eq!(capture.document.roots, vec![root_id.clone()]);
        assert_eq!(capture.document.node_count, 2);
        assert_ne!(root_id.as_str(), child_id.as_str());

        let root = match capture
            .query(DebugQuery::GetNode {
                node: root_id.clone(),
            })
            .unwrap()
        {
            DebugResponse::Node(node) => node,
            response => panic!("unexpected response: {response:?}"),
        };
        assert_eq!(root.id, root_id);
        assert_eq!(root.stable_id, Some(10));
        assert!(root.arena_id.is_some());
        assert_eq!(root.children, vec![child_id]);
    }

    #[test]
    fn capture_answers_interactive_state_queries_consistently() {
        let (capture, root_id, child_id) = sample_capture();

        let child_state = match capture
            .query(DebugQuery::GetElementState {
                node: child_id.clone(),
            })
            .unwrap()
        {
            DebugResponse::ElementState(state) => state,
            response => panic!("unexpected response: {response:?}"),
        };
        assert_eq!(child_state.identity.stable_id, Some(20));
        assert_eq!(child_state.tree.parent, Some(root_id.clone()));
        assert!(child_state.interaction.as_ref().unwrap().focused);
        assert!(child_state.interaction.as_ref().unwrap().keyboard_captured);

        let ancestors = match capture
            .query(DebugQuery::GetAncestors { node: child_id })
            .unwrap()
        {
            DebugResponse::Nodes(nodes) => nodes,
            response => panic!("unexpected response: {response:?}"),
        };
        assert_eq!(ancestors.len(), 1);
        assert_eq!(ancestors[0].id, root_id);
    }

    #[test]
    fn pick_node_uses_captured_layout_without_live_arena_borrow() {
        let (capture, _root_id, child_id) = sample_capture();

        let picked = match capture
            .query(DebugQuery::PickNode { x: 12.0, y: 14.0 })
            .unwrap()
        {
            DebugResponse::Pick(node) => node,
            response => panic!("unexpected response: {response:?}"),
        };
        assert_eq!(picked, Some(child_id));
    }

    #[test]
    fn missing_node_returns_stable_error() {
        let (capture, _, _) = sample_capture();
        let missing = DebugNodeId::from("missing".to_string());

        assert_eq!(
            capture
                .query(DebugQuery::GetNode {
                    node: missing.clone()
                })
                .unwrap_err(),
            DebugError::UnknownNode(missing)
        );
    }

    fn retained_auto_input(root: NodeKey, child: NodeKey) -> DebugRetainedAutoCaptureInput {
        let child_bounds = DebugRect {
            x: 10.0,
            y: 12.0,
            width: 20.0,
            height: 16.0,
        };
        let fallback = DebugRetainedAutoFallbackCaptureInput {
            stage: DebugFallbackStage::Planning,
            category: DebugFallbackCategory::PropertyTopology,
            detail: DebugFallbackDetail::Boundary {
                reason: "nested-effect",
            },
            owner: Some(child),
            stable_id: Some(20),
            element_type: Some("Element"),
            bounds: Some(child_bounds),
        };
        DebugRetainedAutoCaptureInput {
            frame: DebugRetainedAutoFrameCaptureInput {
                attempt_id: 42,
                requested_mode: DebugPaintRequestedMode::RetainedAuto,
                selected_authority: DebugFramePaintAuthority::Legacy,
                disposition: DebugFrameDisposition::FellBackToLegacy,
                fallback_stages: vec![fallback.clone()],
                statistics: DebugRetainedAutoStatistics {
                    reachable_nodes: 2,
                    covered_nodes: 1,
                    legacy_nodes: 1,
                    fallback_count: 1,
                    ..Default::default()
                },
            },
            nodes: vec![DebugRetainedAutoNodeCaptureInput {
                owner: Some(child),
                stable_id: Some(20),
                element_type: "Element",
                bounds: Some(child_bounds),
                coverage: vec![DebugCoverageKind::LegacyBoundary],
                resident_action: Some(DebugResidentAction::None),
                fallbacks: vec![fallback],
            }],
            surfaces: vec![DebugRetainedAutoSurfaceCaptureInput {
                owner: Some(root),
                stable_id: Some(10),
                element_type: "Element",
                bounds: Some(DebugRect {
                    x: 0.0,
                    y: 0.0,
                    width: 100.0,
                    height: 80.0,
                }),
                kind: DebugSurfaceKind::Effect,
                coverage: DebugCoverageKind::PropertySurface,
                resident_action: DebugResidentAction::Reuse,
            }],
        }
    }

    #[test]
    fn retained_auto_capture_maps_existing_node_ids_and_structured_reasons() {
        let mut arena = NodeArena::new();
        let root = arena.insert(Node::new(Box::new(Element::new_with_id(
            10, 0.0, 0.0, 100.0, 80.0,
        ))));
        let child = arena.insert(Node::with_parent(
            Box::new(Element::new_with_id(20, 10.0, 12.0, 20.0, 16.0)),
            Some(root),
        ));
        arena.push_child(root, child);
        arena.set_roots(vec![root]);
        let mut options = DebugCaptureOptions::default();
        options.include_retained_auto = true;
        let capture = DebugCapture::from_arena_with_retained_auto(
            options,
            &arena,
            &[root],
            DebugViewportCaptureInput {
                logical_size: (320.0, 240.0),
                scale_factor: 2.0,
                focused_node: None,
                hovered_node: None,
                pointer_capture_node: None,
                keyboard_capture_node: None,
                pointer_position: None,
                pressed_pointer_buttons: Vec::new(),
            },
            Some(retained_auto_input(root, child)),
        );

        let retained = capture
            .document()
            .viewport
            .retained_auto
            .as_ref()
            .expect("last attempt is captured");
        assert_eq!(retained.frame.attempt_id, 42);
        assert_eq!(retained.frame.fallback_stages.len(), 1);
        assert!(matches!(
            retained.frame.fallback_stages[0].detail,
            DebugFallbackDetail::Boundary {
                reason: "nested-effect"
            }
        ));
        assert_eq!(retained.nodes[0].stable_id, Some(20));
        assert!(retained.nodes[0].node.is_some());
        assert_eq!(
            retained.surfaces[0].resident_action,
            DebugResidentAction::Reuse
        );

        let child_id = retained.nodes[0].node.clone().unwrap();
        let DebugResponse::RenderState(Some(render)) = capture
            .query(DebugQuery::GetRenderState { node: child_id })
            .unwrap()
        else {
            panic!("node render state must exist")
        };
        assert_eq!(
            render.retained_auto.unwrap().coverage,
            vec![DebugCoverageKind::LegacyBoundary]
        );
    }

    #[test]
    fn retained_auto_input_is_ignored_when_capture_option_is_off() {
        let mut arena = NodeArena::new();
        let root = arena.insert(Node::new(Box::new(Element::new_with_id(
            10, 0.0, 0.0, 100.0, 80.0,
        ))));
        let child = arena.insert(Node::with_parent(
            Box::new(Element::new_with_id(20, 10.0, 12.0, 20.0, 16.0)),
            Some(root),
        ));
        arena.push_child(root, child);
        let capture = DebugCapture::from_arena_with_retained_auto(
            DebugCaptureOptions::default(),
            &arena,
            &[root],
            DebugViewportCaptureInput {
                logical_size: (320.0, 240.0),
                scale_factor: 1.0,
                focused_node: None,
                hovered_node: None,
                pointer_capture_node: None,
                keyboard_capture_node: None,
                pointer_position: None,
                pressed_pointer_buttons: Vec::new(),
            },
            Some(retained_auto_input(root, child)),
        );
        assert!(capture.document().viewport.retained_auto.is_none());
    }
}
