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
    /// Captures the viewport's last RetainedAuto frame: its statistics and
    /// what the surface cache did with each surface.
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
    /// The last RetainedAuto frame the viewport recorded diagnostics for.
    /// This is independent from the frame currently visible on screen.
    pub retained_auto: Option<DebugRetainedAutoSnapshot>,
}

#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DebugFrameDisposition {
    Presented,
    /// Compilation or execution failed; the frame was discarded and the next
    /// frame paints again.
    Aborted,
}

/// What a retained surface rasters.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DebugSurfaceKind {
    Transform,
    Effect,
    ScrollContent,
    /// Output of a custom GPU paint source.
    GpuSource,
}

/// What the surface cache did with a surface this frame.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DebugSurfaceAction {
    /// Kept pixels from an earlier frame were composited unchanged.
    Reuse,
    /// The surface was drawn again and kept for later frames.
    Reraster,
    /// The surface was drawn into a frame-local target and not kept.
    Transient,
}

#[non_exhaustive]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DebugRetainedAutoStatistics {
    pub paint_chunks: u64,
    pub paint_ops: u64,
    pub surfaces: u64,
    pub gpu_sources: u64,
    pub reuses: u64,
    pub rerasterizations: u64,
    pub transients: u64,
    pub transform_nodes: u64,
    pub effect_nodes: u64,
    pub scroll_nodes: u64,
}

#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub struct DebugRetainedAutoSurfaceSnapshot {
    pub owner: Option<DebugNodeId>,
    pub stable_id: Option<u64>,
    pub element_type: &'static str,
    /// The owner node's logical bounds.
    pub bounds: Option<DebugRect>,
    pub kind: DebugSurfaceKind,
    pub action: DebugSurfaceAction,
    /// Physical texture size.
    pub size: (u32, u32),
    /// Device pixels per logical pixel of the raster.
    pub scale_factor: f32,
}

#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub struct DebugRetainedAutoFrameSnapshot {
    pub frame_number: u64,
    pub disposition: DebugFrameDisposition,
    pub statistics: DebugRetainedAutoStatistics,
}

#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub struct DebugRetainedAutoSnapshot {
    pub frame: DebugRetainedAutoFrameSnapshot,
    pub surfaces: Vec<DebugRetainedAutoSurfaceSnapshot>,
}

impl DebugSurfaceKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Transform => "transform",
            Self::Effect => "effect",
            Self::ScrollContent => "scroll",
            Self::GpuSource => "gpu",
        }
    }
}

impl DebugSurfaceAction {
    pub fn label(self) -> &'static str {
        match self {
            Self::Reuse => "reuse",
            Self::Reraster => "reraster",
            Self::Transient => "transient",
        }
    }
}

impl DebugRetainedAutoSurfaceSnapshot {
    /// One-line description, e.g. `Element transform reraster 400x300 @2x`.
    pub fn label(&self) -> String {
        let element_type = self
            .element_type
            .rsplit("::")
            .next()
            .unwrap_or(self.element_type);
        format!(
            "{element_type} {} {} {}x{} @{}x",
            self.kind.label(),
            self.action.label(),
            self.size.0,
            self.size.1,
            format_scale_factor(self.scale_factor)
        )
    }
}

/// `2` for an integral scale, otherwise up to two decimals (`1.5`).
pub(crate) fn format_scale_factor(scale: f32) -> String {
    if scale.fract() == 0.0 {
        format!("{scale:.0}")
    } else {
        format!("{scale:.2}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_owned()
    }
}

impl std::fmt::Display for DebugRetainedAutoSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let frame = &self.frame;
        let stats = &frame.statistics;
        let disposition = match frame.disposition {
            DebugFrameDisposition::Presented => "presented",
            DebugFrameDisposition::Aborted => "aborted",
        };
        writeln!(
            f,
            "retained-auto frame {}: {disposition}, {} chunks, {} ops",
            frame.frame_number, stats.paint_chunks, stats.paint_ops
        )?;
        writeln!(
            f,
            "  {} surfaces, {} gpu sources: {} reuse, {} reraster, {} transient",
            stats.surfaces,
            stats.gpu_sources,
            stats.reuses,
            stats.rerasterizations,
            stats.transients
        )?;
        writeln!(
            f,
            "  property nodes: {} transform, {} effect, {} scroll",
            stats.transform_nodes, stats.effect_nodes, stats.scroll_nodes
        )?;
        for surface in &self.surfaces {
            write!(f, "  {}", surface.label())?;
            if let Some(id) = surface.stable_id {
                write!(f, " #{id}")?;
            }
            if let Some(bounds) = surface.bounds {
                write!(
                    f,
                    " at ({}, {}) {}x{}",
                    bounds.x, bounds.y, bounds.width, bounds.height
                )?;
            }
            writeln!(f)?;
        }
        Ok(())
    }
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

/// The viewport's record of its last RetainedAuto frame, resolved into a
/// [`DebugRetainedAutoSnapshot`] at capture time.
#[derive(Clone, Debug)]
pub(crate) struct DebugRetainedAutoCaptureInput {
    pub frame: DebugRetainedAutoFrameSnapshot,
    pub surfaces: Vec<DebugRetainedAutoSurfaceCaptureInput>,
}

#[derive(Clone, Debug)]
pub(crate) struct DebugRetainedAutoSurfaceCaptureInput {
    pub owner: Option<NodeKey>,
    pub stable_id: Option<u64>,
    pub element_type: &'static str,
    pub bounds: Option<DebugRect>,
    pub kind: DebugSurfaceKind,
    pub action: DebugSurfaceAction,
    pub size: (u32, u32),
    pub scale_factor: f32,
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
                    action: surface.action,
                    size: surface.size,
                    scale_factor: surface.scale_factor,
                })
                .collect();
            DebugRetainedAutoSnapshot {
                frame: input.frame,
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
        let snapshot = arena.box_model_in_viewport(key, element.box_model_snapshot());
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
mod tests;
