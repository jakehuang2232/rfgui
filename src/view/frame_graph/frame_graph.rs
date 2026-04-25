use crate::time::Instant;
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::VecDeque;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use super::buffer_resource::{BufferDesc, BufferHandle};
use super::builder::PassBuilderState;
use super::texture_resource::{TextureDesc, TextureHandle};
use crate::view::render_pass::draw_rect_pass::{DrawRectPass, OpaqueRectPass};
use crate::view::render_pass::render_target::{
    render_target_attachment_view, render_target_msaa_view, render_target_view,
};
use crate::view::render_pass::{
    ComputeCtx, ComputePass, ComputePassWrapper, GraphicsCtx, GraphicsPass, GraphicsPassWrapper,
    PassNodeDyn, TransferCtx, TransferPass, TransferPassWrapper,
};
use crate::view::viewport::Viewport;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResourceHandle {
    Texture(TextureHandle),
    Buffer(BufferHandle),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AllocationId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResourceKind {
    Texture,
    Buffer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AllocationClass {
    Texture,
    Buffer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResourceLifetime {
    Imported,
    Transient,
    Persistent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AllocationOwner {
    AllocatorManaged,
    ExternalOwned,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ExternalResource {
    Surface,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ExternalSinkId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ExternalSinkKind {
    SurfacePresent,
    Readback,
    DebugCapture,
    ExportTexture,
    ExportBuffer,
}

impl ExternalSinkKind {
    fn supports_pass_target(self) -> bool {
        matches!(self, ExternalSinkKind::SurfacePresent)
    }

    fn supports_texture_target(self) -> bool {
        matches!(
            self,
            ExternalSinkKind::DebugCapture
                | ExternalSinkKind::ExportTexture
                | ExternalSinkKind::Readback
        )
    }

    fn supports_buffer_target(self) -> bool {
        matches!(
            self,
            ExternalSinkKind::DebugCapture
                | ExternalSinkKind::ExportBuffer
                | ExternalSinkKind::Readback
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ExternalSinkTarget {
    Pass(PassHandle),
    Resource(ResourceHandle),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ExternalSink {
    pub id: ExternalSinkId,
    pub kind: ExternalSinkKind,
    pub target: ExternalSinkTarget,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ResourceMetadata {
    pub stable_key: Option<u64>,
    pub kind: ResourceKind,
    pub allocation_class: AllocationClass,
    pub lifetime: ResourceLifetime,
}

impl ResourceMetadata {
    fn transient(kind: ResourceKind, allocation_class: AllocationClass) -> Self {
        Self {
            stable_key: None,
            kind,
            allocation_class,
            lifetime: ResourceLifetime::Transient,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResourceAccess {
    Read,
    Write,
    Modify,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResourceUsage {
    Produced,
    BufferWrite,
    SampledRead,
    ColorAttachmentWrite,
    DepthRead,
    DepthWrite,
    StencilRead,
    StencilWrite,
    CopySrc,
    CopyDst,
    UniformRead,
    VertexRead,
    IndexRead,
    StorageRead,
    StorageWrite,
}

impl ResourceUsage {
    pub fn effective_access(self) -> ResourceAccess {
        match self {
            ResourceUsage::Produced
            | ResourceUsage::BufferWrite
            | ResourceUsage::ColorAttachmentWrite
            | ResourceUsage::DepthWrite
            | ResourceUsage::StencilWrite
            | ResourceUsage::CopyDst
            | ResourceUsage::StorageWrite => ResourceAccess::Write,
            ResourceUsage::SampledRead
            | ResourceUsage::DepthRead
            | ResourceUsage::StencilRead
            | ResourceUsage::CopySrc
            | ResourceUsage::UniformRead
            | ResourceUsage::VertexRead
            | ResourceUsage::IndexRead
            | ResourceUsage::StorageRead => ResourceAccess::Read,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextureAspectState {
    Read,
    Write,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextureResourceState {
    Undefined,
    Sampled,
    ColorAttachment,
    DepthStencilAttachment {
        depth: Option<TextureAspectState>,
        stencil: Option<TextureAspectState>,
    },
    StorageRead,
    StorageWrite,
    CopySrc,
    CopyDst,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BufferResourceState {
    Undefined,
    Written,
    UniformRead,
    VertexRead,
    IndexRead,
    StorageRead,
    StorageWrite,
    CopySrc,
    CopyDst,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResourceState {
    Texture(TextureResourceState),
    Buffer(BufferResourceState),
}

impl ResourceState {
    fn undefined(kind: ResourceKind) -> Self {
        match kind {
            ResourceKind::Texture => Self::Texture(TextureResourceState::Undefined),
            ResourceKind::Buffer => Self::Buffer(BufferResourceState::Undefined),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PassResourceUsage {
    pub resource: ResourceHandle,
    pub usage: ResourceUsage,
    pub read_version: Option<ResourceVersionId>,
    pub write_version: Option<ResourceVersionId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextureVersionId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BufferVersionId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResourceVersionId {
    Texture(TextureVersionId),
    Buffer(BufferVersionId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ProducedVersion {
    pub resource: ResourceHandle,
    pub version: ResourceVersionId,
    pub producer_pass_index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ConsumedVersion {
    pub resource: ResourceHandle,
    pub version: ResourceVersionId,
    pub consumer_pass_index: usize,
    pub usage: ResourceUsage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompiledPassResourceTransition {
    pub resource: ResourceHandle,
    pub before: ResourceState,
    pub after: ResourceState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompiledResourceTransition {
    pub resource: ResourceHandle,
    pub pass_index: usize,
    pub execution_index: usize,
    pub before: ResourceState,
    pub after: ResourceState,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompiledResourceTimeline {
    pub resource: ResourceHandle,
    pub initial_state: ResourceState,
    pub transitions: Vec<CompiledResourceTransition>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PassHandle(usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PassKind {
    Graphics,
    Compute,
    Transfer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AttachmentTarget {
    Surface,
    Texture(TextureHandle),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AttachmentLoadOp {
    Load,
    Clear,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AttachmentStoreOp {
    Store,
    Discard,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SampleCountPolicy {
    SurfaceDefault,
    Fixed(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum ViewportPolicy {
    FixedToTarget,
    #[default]
    Dynamic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum ScissorPolicy {
    Disabled,
    #[default]
    Dynamic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum GraphicsPassMergePolicy {
    #[default]
    RequiresOwnPass,
    Mergeable,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GraphicsColorAttachmentOps {
    pub load_op: AttachmentLoadOp,
    pub store_op: AttachmentStoreOp,
    pub clear_color: Option<[f64; 4]>,
}

impl GraphicsColorAttachmentOps {
    pub fn load() -> Self {
        Self {
            load_op: AttachmentLoadOp::Load,
            store_op: AttachmentStoreOp::Store,
            clear_color: None,
        }
    }

    pub fn clear(clear_color: [f64; 4]) -> Self {
        Self {
            load_op: AttachmentLoadOp::Clear,
            store_op: AttachmentStoreOp::Store,
            clear_color: Some(clear_color),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GraphicsColorAttachmentDescriptor {
    pub target: AttachmentTarget,
    pub load_op: AttachmentLoadOp,
    pub store_op: AttachmentStoreOp,
    pub clear_color: Option<[f64; 4]>,
}

impl GraphicsColorAttachmentDescriptor {
    pub fn from_ops(target: AttachmentTarget, ops: GraphicsColorAttachmentOps) -> Self {
        Self {
            target,
            load_op: ops.load_op,
            store_op: ops.store_op,
            clear_color: ops.clear_color,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GraphicsDepthAspectDescriptor {
    pub load_op: AttachmentLoadOp,
    pub store_op: AttachmentStoreOp,
    pub clear_depth: Option<f32>,
    pub usage: ResourceUsage,
}

impl GraphicsDepthAspectDescriptor {
    pub fn read() -> Self {
        Self {
            load_op: AttachmentLoadOp::Load,
            store_op: AttachmentStoreOp::Store,
            clear_depth: None,
            usage: ResourceUsage::DepthRead,
        }
    }

    pub fn write(load_op: AttachmentLoadOp, clear_depth: Option<f32>) -> Self {
        Self {
            load_op,
            store_op: AttachmentStoreOp::Store,
            clear_depth,
            usage: ResourceUsage::DepthWrite,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GraphicsStencilAspectDescriptor {
    pub load_op: AttachmentLoadOp,
    pub store_op: AttachmentStoreOp,
    pub clear_stencil: Option<u32>,
    pub usage: ResourceUsage,
}

impl GraphicsStencilAspectDescriptor {
    pub fn read() -> Self {
        Self {
            load_op: AttachmentLoadOp::Load,
            store_op: AttachmentStoreOp::Store,
            clear_stencil: None,
            usage: ResourceUsage::StencilRead,
        }
    }

    pub fn write(load_op: AttachmentLoadOp, clear_stencil: Option<u32>) -> Self {
        Self {
            load_op,
            store_op: AttachmentStoreOp::Store,
            clear_stencil,
            usage: ResourceUsage::StencilWrite,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GraphicsDepthStencilAttachmentDescriptor {
    pub target: AttachmentTarget,
    pub depth: Option<GraphicsDepthAspectDescriptor>,
    pub stencil: Option<GraphicsStencilAspectDescriptor>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GraphicsPassDescriptor {
    pub color_attachments: Vec<GraphicsColorAttachmentDescriptor>,
    pub depth_stencil_attachment: Option<GraphicsDepthStencilAttachmentDescriptor>,
    pub sample_count: SampleCountPolicy,
    pub viewport_policy: ViewportPolicy,
    pub scissor_policy: ScissorPolicy,
    pub merge_policy: GraphicsPassMergePolicy,
    pub requirements: GraphicsPipelineRequirements,
}

impl Default for GraphicsPassDescriptor {
    fn default() -> Self {
        Self {
            color_attachments: Vec::new(),
            depth_stencil_attachment: None,
            sample_count: SampleCountPolicy::SurfaceDefault,
            viewport_policy: ViewportPolicy::Dynamic,
            scissor_policy: ScissorPolicy::Dynamic,
            merge_policy: GraphicsPassMergePolicy::RequiresOwnPass,
            requirements: GraphicsPipelineRequirements::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct GraphicsPipelineRequirements {
    pub requires_color_attachment: bool,
    pub uses_depth: bool,
    pub uses_stencil: bool,
    pub writes_depth: bool,
    pub writes_stencil: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ComputePassDescriptor;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TransferPassDescriptor;

#[derive(Clone, Debug, PartialEq)]
pub enum PassDetails {
    Graphics(GraphicsPassDescriptor),
    Compute(ComputePassDescriptor),
    Transfer(TransferPassDescriptor),
}

impl Default for PassDetails {
    fn default() -> Self {
        Self::Graphics(GraphicsPassDescriptor::default())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PassDescriptor {
    pub name: &'static str,
    pub kind: PassKind,
    pub details: PassDetails,
}

impl PassDescriptor {
    pub fn graphics(name: &'static str) -> Self {
        Self {
            name,
            kind: PassKind::Graphics,
            details: PassDetails::Graphics(GraphicsPassDescriptor::default()),
        }
    }

    pub fn compute(name: &'static str) -> Self {
        Self {
            name,
            kind: PassKind::Compute,
            details: PassDetails::Compute(ComputePassDescriptor),
        }
    }

    pub fn transfer(name: &'static str) -> Self {
        Self {
            name,
            kind: PassKind::Transfer,
            details: PassDetails::Transfer(TransferPassDescriptor),
        }
    }

    pub fn graphics_mut(&mut self) -> &mut GraphicsPassDescriptor {
        self.kind = PassKind::Graphics;
        if !matches!(self.details, PassDetails::Graphics(_)) {
            self.details = PassDetails::Graphics(GraphicsPassDescriptor::default());
        }
        match &mut self.details {
            PassDetails::Graphics(descriptor) => descriptor,
            PassDetails::Compute(_) | PassDetails::Transfer(_) => unreachable!(),
        }
    }
}

struct PassNode {
    pass: Box<dyn PassNodeDyn>,
    descriptor: PassDescriptor,
    usages: Vec<PassResourceUsage>,
}

#[derive(Clone, Debug)]
pub struct CompiledPass {
    pub original_index: usize,
    pub name: &'static str,
    pub descriptor: PassDescriptor,
    pub dependencies: Vec<usize>,
    pub resource_usages: Vec<PassResourceUsage>,
    pub input_versions: Vec<ConsumedVersion>,
    pub output_versions: Vec<ProducedVersion>,
    pub resource_transitions: Vec<CompiledPassResourceTransition>,
    pub is_root: bool,
}

#[derive(Clone, Debug)]
pub struct CompiledResource {
    pub handle: ResourceHandle,
    pub stable_key: Option<u64>,
    pub kind: ResourceKind,
    pub allocation_class: AllocationClass,
    pub lifetime: ResourceLifetime,
    pub first_use_pass_index: usize,
    pub last_use_pass_index: usize,
    pub producer_passes: Vec<usize>,
    pub consumer_passes: Vec<usize>,
    pub allocation_id: Option<AllocationId>,
    pub allocation_owner: AllocationOwner,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextureAllocationPlanEntry {
    pub allocation_id: AllocationId,
    pub owner: AllocationOwner,
    pub resources: Vec<TextureHandle>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BufferAllocationPlanEntry {
    pub allocation_id: AllocationId,
    pub owner: AllocationOwner,
    pub resources: Vec<BufferHandle>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExternalAllocationPlanEntry {
    pub resource: ExternalResource,
    pub owner: AllocationOwner,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AllocationPlan {
    pub texture_allocations: Vec<TextureAllocationPlanEntry>,
    pub buffer_allocations: Vec<BufferAllocationPlanEntry>,
    pub external_resources: Vec<ExternalAllocationPlanEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RenderPassCompatibleColorAttachment {
    pub target: AttachmentTarget,
    pub resolve_target: Option<AttachmentTarget>,
    pub load_op: AttachmentLoadOp,
    pub store_op: AttachmentStoreOp,
    pub clear_color_bits: Option<[u64; 4]>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RenderPassCompatibleDepthAspect {
    pub load_op: AttachmentLoadOp,
    pub store_op: AttachmentStoreOp,
    pub clear_depth_bits: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RenderPassCompatibleStencilAspect {
    pub load_op: AttachmentLoadOp,
    pub store_op: AttachmentStoreOp,
    pub clear_stencil: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct RenderPassCompatibilityKey {
    pub color_attachments: Vec<RenderPassCompatibleColorAttachment>,
    pub depth_stencil_attachment: Option<(
        AttachmentTarget,
        Option<RenderPassCompatibleDepthAspect>,
        Option<RenderPassCompatibleStencilAspect>,
    )>,
    pub sample_count: SampleCountPolicy,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RenderPassGroup {
    pub pass_indices: Vec<usize>,
    pub compatibility: RenderPassCompatibilityKey,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CompiledExecuteStep {
    GraphicsPass { pass_index: usize },
    GraphicsPassGroup(RenderPassGroup),
    ComputePass { pass_index: usize },
    TransferPass { pass_index: usize },
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ExecutionPlan {
    pub ordered_passes: Vec<usize>,
    pub steps: Vec<CompiledExecuteStep>,
}

#[derive(Clone, Debug, Default)]
pub struct CompiledGraph {
    pub passes: Vec<CompiledPass>,
    pub resources: Vec<CompiledResource>,
    pub external_sinks: Vec<ExternalSink>,
    pub allocation_plan: AllocationPlan,
    pub culled_passes: Vec<usize>,
    pub(crate) execution_plan: ExecutionPlan,
    pub resource_transitions: Vec<CompiledResourceTransition>,
    pub resource_timelines: Vec<CompiledResourceTimeline>,
    texture_allocation_ids: FxHashMap<TextureHandle, AllocationId>,
    buffer_allocation_ids: FxHashMap<BufferHandle, AllocationId>,
    texture_stable_keys: FxHashMap<TextureHandle, u64>,
}

#[derive(Clone)]
enum ExecuteStep {
    GraphicsPass { index: usize },
    GraphicsPassGroup(RenderPassGroup),
    ComputePass { index: usize },
    TransferPass { index: usize },
}

pub struct FrameGraph {
    passes: Vec<PassNode>,
    textures: Vec<TextureDesc>,
    texture_attachment_pairs: FxHashMap<TextureHandle, AttachmentTarget>,
    buffers: Vec<BufferDesc>,
    texture_metadata: Vec<ResourceMetadata>,
    buffer_metadata: Vec<ResourceMetadata>,
    external_sinks: Vec<ExternalSink>,
    compiled_graph: Option<CompiledGraph>,
    order: Vec<usize>,
    compiled: bool,
    build_errors: Vec<FrameGraphError>,
    execute_steps: Vec<ExecuteStep>,
}

#[derive(Clone, Debug, Default)]
pub struct ExecuteProfile {
    pub total_ms: f64,
    pub pass_count: usize,
    pub ordered_passes: Vec<(String, f64, usize)>,
    pub detail_ordered: Vec<(String, f64, usize)>,
}

#[derive(Clone, Debug, Default)]
pub struct CompileCountStat {
    pub label: String,
    pub count: usize,
}

#[derive(Clone, Debug, Default)]
pub struct CompileDegreeStat {
    pub pass_index: usize,
    pub pass_name: String,
    pub degree: usize,
}

#[derive(Clone, Debug, Default)]
pub struct CompileGraphProfile {
    pub total_ms: f64,
    pub build_version_producers_ms: f64,
    pub latest_resource_versions_ms: f64,
    pub discover_sink_passes_ms: f64,
    pub discover_live_passes_ms: f64,
    pub build_live_dependency_graph_ms: f64,
    pub toposort_live_passes_ms: f64,
    pub build_execution_plan_ms: f64,
    pub build_resource_state_timelines_ms: f64,
    pub build_compiled_resources_ms: f64,
    pub assemble_compiled_passes_ms: f64,
    pub sink_pass_count: usize,
    pub live_pass_count: usize,
    pub ordered_pass_count: usize,
    pub execution_step_count: usize,
    pub compiled_resource_count: usize,
    pub culled_pass_count: usize,
    pub live_dependency_edge_count: usize,
    pub graphics_pass_count: usize,
    pub compute_pass_count: usize,
    pub transfer_pass_count: usize,
    pub graphics_step_count: usize,
    pub graphics_group_count: usize,
    pub max_graphics_group_size: usize,
    pub pass_name_counts: Vec<CompileCountStat>,
    pub versioned_resource_counts: Vec<CompileCountStat>,
    pub top_indegree_passes: Vec<CompileDegreeStat>,
    pub top_outdegree_passes: Vec<CompileDegreeStat>,
}

#[derive(Clone, Debug, Default)]
pub struct CompileProfile {
    pub total_ms: f64,
    pub setup_passes_ms: f64,
    pub annotate_resource_versions_ms: f64,
    pub build_compiled_graph_ms: f64,
    pub prepare_upload_ms: f64,
    pub setup_pass_count: usize,
    pub prepare_pass_count: usize,
    /// True when `annotate_resource_versions` + `build_compiled_graph` were skipped
    /// because the topology hash matched the cached result from the previous frame.
    pub topology_cache_hit: bool,
    pub graph: CompileGraphProfile,
}

impl FrameGraph {
    pub fn new() -> Self {
        Self {
            passes: Vec::new(),
            textures: Vec::new(),
            texture_attachment_pairs: FxHashMap::default(),
            buffers: Vec::new(),
            texture_metadata: Vec::new(),
            buffer_metadata: Vec::new(),
            external_sinks: Vec::new(),
            compiled_graph: None,
            order: Vec::new(),
            compiled: false,
            build_errors: Vec::new(),
            execute_steps: Vec::new(),
        }
    }

    pub fn add_graphics_pass<P: GraphicsPass + 'static>(&mut self, pass: P) -> PassHandle {
        let name = std::any::type_name::<P>();
        let node = PassNode {
            pass: Box::new(GraphicsPassWrapper { pass }),
            descriptor: PassDescriptor::graphics(name),
            usages: Vec::new(),
        };
        let handle = PassHandle(self.passes.len());
        self.passes.push(node);
        self.compiled_graph = None;
        self.compiled = false;
        handle
    }

    pub fn add_compute_pass<P: ComputePass + 'static>(&mut self, pass: P) -> PassHandle {
        let name = std::any::type_name::<P>();
        let node = PassNode {
            pass: Box::new(ComputePassWrapper { pass }),
            descriptor: PassDescriptor::compute(name),
            usages: Vec::new(),
        };
        let handle = PassHandle(self.passes.len());
        self.passes.push(node);
        self.compiled_graph = None;
        self.compiled = false;
        handle
    }

    pub fn add_transfer_pass<P: TransferPass + 'static>(&mut self, pass: P) -> PassHandle {
        let name = std::any::type_name::<P>();
        let node = PassNode {
            pass: Box::new(TransferPassWrapper { pass }),
            descriptor: PassDescriptor::transfer(name),
            usages: Vec::new(),
        };
        let handle = PassHandle(self.passes.len());
        self.passes.push(node);
        self.compiled_graph = None;
        self.compiled = false;
        handle
    }

    pub(crate) fn pair_texture_attachment(
        &mut self,
        color: TextureHandle,
        depth_stencil: AttachmentTarget,
    ) {
        self.texture_attachment_pairs.insert(color, depth_stencil);
    }

    pub fn add_pass_sink(
        &mut self,
        pass: PassHandle,
        kind: ExternalSinkKind,
    ) -> Result<ExternalSinkId, FrameGraphError> {
        if !kind.supports_pass_target() {
            return Err(FrameGraphError::Validation(format!(
                "sink kind {:?} does not support pass targets",
                kind
            )));
        }
        Ok(self.add_external_sink(kind, ExternalSinkTarget::Pass(pass)))
    }

    pub fn add_texture_sink<Tag>(
        &mut self,
        source: &super::slot::OutSlot<super::texture_resource::TextureResource, Tag>,
        kind: ExternalSinkKind,
    ) -> Result<ExternalSinkId, FrameGraphError> {
        if !kind.supports_texture_target() {
            return Err(FrameGraphError::Validation(format!(
                "sink kind {:?} does not support texture targets",
                kind
            )));
        }
        let Some(handle) = source.handle() else {
            return Err(FrameGraphError::MissingOutput(
                "texture sink source has no handle",
            ));
        };
        Ok(self.add_external_sink(
            kind,
            ExternalSinkTarget::Resource(ResourceHandle::Texture(handle)),
        ))
    }

    pub fn add_buffer_sink<Tag>(
        &mut self,
        source: &super::slot::OutSlot<super::buffer_resource::BufferResource, Tag>,
        kind: ExternalSinkKind,
    ) -> Result<ExternalSinkId, FrameGraphError> {
        if !kind.supports_buffer_target() {
            return Err(FrameGraphError::Validation(format!(
                "sink kind {:?} does not support buffer targets",
                kind
            )));
        }
        let Some(handle) = source.handle() else {
            return Err(FrameGraphError::MissingOutput(
                "buffer sink source has no handle",
            ));
        };
        Ok(self.add_external_sink(
            kind,
            ExternalSinkTarget::Resource(ResourceHandle::Buffer(handle)),
        ))
    }

    fn add_external_sink(
        &mut self,
        kind: ExternalSinkKind,
        target: ExternalSinkTarget,
    ) -> ExternalSinkId {
        let id = ExternalSinkId(self.external_sinks.len() as u32);
        self.external_sinks.push(ExternalSink { id, kind, target });
        self.compiled_graph = None;
        self.compiled = false;
        id
    }

    pub fn declare_texture<Tag>(
        &mut self,
        desc: TextureDesc,
    ) -> super::slot::OutSlot<super::texture_resource::TextureResource, Tag> {
        self.declare_texture_internal(desc, ResourceLifetime::Transient, None)
    }

    pub fn texture_desc(&self, handle: TextureHandle) -> Option<TextureDesc> {
        self.textures.get(handle.0 as usize).cloned()
    }

    pub(crate) fn declare_texture_internal<Tag>(
        &mut self,
        desc: TextureDesc,
        lifetime: ResourceLifetime,
        stable_key: Option<u64>,
    ) -> super::slot::OutSlot<super::texture_resource::TextureResource, Tag> {
        let handle = TextureHandle(self.textures.len() as u32);
        self.textures.push(desc);
        self.texture_metadata.push(ResourceMetadata {
            stable_key,
            kind: ResourceKind::Texture,
            allocation_class: AllocationClass::Texture,
            lifetime,
        });
        super::slot::OutSlot::with_handle(handle)
    }

    #[allow(dead_code)]
    pub(crate) fn declare_buffer_internal<Tag>(
        &mut self,
        desc: BufferDesc,
        lifetime: ResourceLifetime,
        stable_key: Option<u64>,
    ) -> super::slot::OutSlot<super::buffer_resource::BufferResource, Tag> {
        let handle = BufferHandle(self.buffers.len() as u32);
        self.buffers.push(desc);
        self.buffer_metadata.push(ResourceMetadata {
            stable_key,
            kind: ResourceKind::Buffer,
            allocation_class: AllocationClass::Buffer,
            lifetime,
        });
        super::slot::OutSlot::with_handle(handle)
    }

    fn compile_profiled_internal(&mut self) -> Result<CompileProfile, FrameGraphError> {
        let compile_started_at = Instant::now();
        self.order.clear();
        self.compiled_graph = None;
        self.compiled = false;

        for node in &mut self.passes {
            node.usages.clear();
        }

        let mut textures = std::mem::take(&mut self.textures);
        let mut buffers = std::mem::take(&mut self.buffers);
        let mut texture_metadata = std::mem::take(&mut self.texture_metadata);
        let mut buffer_metadata = std::mem::take(&mut self.buffer_metadata);
        let mut build_errors: Vec<FrameGraphError> = Vec::new();

        let setup_started_at = Instant::now();
        for node in &mut self.passes {
            let mut builder = PassBuilderState {
                descriptor: &mut node.descriptor,
                textures: &mut textures,
                texture_attachment_pairs: &self.texture_attachment_pairs,
                buffers: &mut buffers,
                texture_metadata: &mut texture_metadata,
                buffer_metadata: &mut buffer_metadata,
                usages: &mut node.usages,
                build_errors: &mut build_errors,
            };
            node.pass.setup(&mut builder);
        }
        let setup_passes_ms = setup_started_at.elapsed().as_secs_f64() * 1000.0;

        self.textures = textures;
        self.buffers = buffers;
        self.texture_metadata = texture_metadata;
        self.buffer_metadata = buffer_metadata;
        self.build_errors = build_errors;

        if let Some(err) = self.build_errors.pop() {
            return Err(err);
        }

        let annotate_started_at = Instant::now();
        self.annotate_resource_versions();
        let annotate_resource_versions_ms = annotate_started_at.elapsed().as_secs_f64() * 1000.0;

        let build_compiled_graph_started_at = Instant::now();
        let (compiled_graph, graph_profile) = self.build_compiled_graph_profiled()?;
        let build_compiled_graph_ms =
            build_compiled_graph_started_at.elapsed().as_secs_f64() * 1000.0;
        self.order = compiled_graph.execution_plan.ordered_passes.clone();
        self.execute_steps = compiled_graph
            .execution_plan
            .steps
            .iter()
            .map(|step| match *step {
                CompiledExecuteStep::GraphicsPass { pass_index } => {
                    ExecuteStep::GraphicsPass { index: pass_index }
                }
                CompiledExecuteStep::GraphicsPassGroup(ref group) => {
                    ExecuteStep::GraphicsPassGroup(group.clone())
                }
                CompiledExecuteStep::ComputePass { pass_index } => {
                    ExecuteStep::ComputePass { index: pass_index }
                }
                CompiledExecuteStep::TransferPass { pass_index } => {
                    ExecuteStep::TransferPass { index: pass_index }
                }
            })
            .collect();

        if batch_trace_enabled() {
            for (pos, &pass_index) in self.order.iter().enumerate() {
                let pass = &self.passes[pass_index].pass;
                if is_rect_pass_name(pass.name()) {
                    eprintln!(
                        "[batch][compile] pos={} pass={} key={:?}",
                        pos,
                        pass.name(),
                        render_pass_compatibility_key(&self.passes[pass_index].descriptor)
                    );
                }
            }
        }

        self.compiled_graph = Some(compiled_graph);
        self.compiled = true;
        Ok(CompileProfile {
            total_ms: compile_started_at.elapsed().as_secs_f64() * 1000.0,
            setup_passes_ms,
            annotate_resource_versions_ms,
            build_compiled_graph_ms,
            prepare_upload_ms: 0.0,
            setup_pass_count: self.passes.len(),
            prepare_pass_count: 0,
            topology_cache_hit: false,
            graph: graph_profile,
        })
    }

    pub fn compile(&mut self) -> Result<(), FrameGraphError> {
        self.compile_profiled_internal().map(|_| ())
    }

    /// Compute a hash of the graph topology after setup() has populated usages.
    /// Covers pass names, kinds, resource usages (excluding annotated versions),
    /// external sinks, and resource counts.
    fn compute_topology_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.passes.len().hash(&mut hasher);
        for node in &self.passes {
            node.descriptor.name.hash(&mut hasher);
            node.descriptor.kind.hash(&mut hasher);
            node.usages.len().hash(&mut hasher);
            for usage in &node.usages {
                // Hash only the resource+usage pair; read_version/write_version come
                // from annotate_resource_versions() and are not yet populated here.
                usage.resource.hash(&mut hasher);
                usage.usage.hash(&mut hasher);
            }
        }
        self.external_sinks.hash(&mut hasher);
        self.textures.len().hash(&mut hasher);
        self.buffers.len().hash(&mut hasher);
        hasher.finish()
    }

    /// Run annotate + build_compiled_graph phases; returns the compiled graph and timings.
    fn compile_annotate_and_build(
        &mut self,
    ) -> Result<(CompiledGraph, f64, f64, CompileGraphProfile), FrameGraphError> {
        let annotate_started_at = Instant::now();
        self.annotate_resource_versions();
        let annotate_resource_versions_ms = annotate_started_at.elapsed().as_secs_f64() * 1000.0;

        let build_started_at = Instant::now();
        let (compiled_graph, graph_profile) = self.build_compiled_graph_profiled()?;
        let build_compiled_graph_ms = build_started_at.elapsed().as_secs_f64() * 1000.0;

        Ok((
            compiled_graph,
            annotate_resource_versions_ms,
            build_compiled_graph_ms,
            graph_profile,
        ))
    }

    /// Like [`compile_with_upload`] but accepts an optional cached `(topology_hash, CompiledGraph)`.
    /// When the topology hash of the current frame matches the cached hash, the expensive
    /// `annotate_resource_versions` + `build_compiled_graph_profiled` phases are skipped.
    /// Returns `(profile, topology_hash, compiled_graph)` so the caller can cache the result.
    pub fn compile_with_upload_cached(
        &mut self,
        viewport: &mut Viewport,
        cache: Option<(u64, CompiledGraph)>,
    ) -> Result<(CompileProfile, u64, CompiledGraph), FrameGraphError> {
        let compile_started_at = Instant::now();
        self.order.clear();
        self.compiled_graph = None;
        self.compiled = false;

        for node in &mut self.passes {
            node.usages.clear();
        }

        let mut textures = std::mem::take(&mut self.textures);
        let mut buffers = std::mem::take(&mut self.buffers);
        let mut texture_metadata = std::mem::take(&mut self.texture_metadata);
        let mut buffer_metadata = std::mem::take(&mut self.buffer_metadata);
        let mut build_errors: Vec<FrameGraphError> = Vec::new();

        let setup_started_at = Instant::now();
        for node in &mut self.passes {
            let mut builder = PassBuilderState {
                descriptor: &mut node.descriptor,
                textures: &mut textures,
                texture_attachment_pairs: &self.texture_attachment_pairs,
                buffers: &mut buffers,
                texture_metadata: &mut texture_metadata,
                buffer_metadata: &mut buffer_metadata,
                usages: &mut node.usages,
                build_errors: &mut build_errors,
            };
            node.pass.setup(&mut builder);
        }
        let setup_passes_ms = setup_started_at.elapsed().as_secs_f64() * 1000.0;

        self.textures = textures;
        self.buffers = buffers;
        self.texture_metadata = texture_metadata;
        self.buffer_metadata = buffer_metadata;
        self.build_errors = build_errors;

        if let Some(err) = self.build_errors.pop() {
            return Err(err);
        }

        // Compute topology hash after setup has populated usages.
        let topology_hash = self.compute_topology_hash();

        // Try to reuse cached CompiledGraph; fall back to full compile on miss.
        let mut topology_cache_hit = false;
        let (compiled_graph, annotate_resource_versions_ms, build_compiled_graph_ms, graph_profile) =
            if let Some((cached_hash, cached_graph)) = cache {
                if cached_hash == topology_hash {
                    topology_cache_hit = true;
                    (
                        cached_graph,
                        0.0_f64,
                        0.0_f64,
                        CompileGraphProfile::default(),
                    )
                } else {
                    self.compile_annotate_and_build()?
                }
            } else {
                self.compile_annotate_and_build()?
            };

        self.order = compiled_graph.execution_plan.ordered_passes.clone();
        self.execute_steps = compiled_graph
            .execution_plan
            .steps
            .iter()
            .map(|step| match *step {
                CompiledExecuteStep::GraphicsPass { pass_index } => {
                    ExecuteStep::GraphicsPass { index: pass_index }
                }
                CompiledExecuteStep::GraphicsPassGroup(ref group) => {
                    ExecuteStep::GraphicsPassGroup(group.clone())
                }
                CompiledExecuteStep::ComputePass { pass_index } => {
                    ExecuteStep::ComputePass { index: pass_index }
                }
                CompiledExecuteStep::TransferPass { pass_index } => {
                    ExecuteStep::TransferPass { index: pass_index }
                }
            })
            .collect();

        if batch_trace_enabled() {
            for (pos, &pass_index) in self.order.iter().enumerate() {
                let pass = &self.passes[pass_index].pass;
                if is_rect_pass_name(pass.name()) {
                    eprintln!(
                        "[batch][compile] pos={} pass={} key={:?}",
                        pos,
                        pass.name(),
                        render_pass_compatibility_key(&self.passes[pass_index].descriptor)
                    );
                }
            }
        }

        self.compiled_graph = Some(compiled_graph.clone());
        self.compiled = true;

        // Prepare phase
        let textures = self.textures.clone();
        let buffers = self.buffers.clone();
        let (texture_allocations, texture_stable_keys, buffer_allocations) = {
            let compiled = self
                .compiled_graph
                .as_ref()
                .expect("compiled graph should exist");
            (
                compiled.texture_allocation_ids.clone(),
                compiled.texture_stable_keys.clone(),
                compiled.buffer_allocation_ids.clone(),
            )
        };
        let prepare_started_at = Instant::now();
        let mut ctx = PrepareContext::new(
            viewport,
            &textures,
            &buffers,
            &texture_allocations,
            &texture_stable_keys,
            &buffer_allocations,
        );
        for &index in &self.order {
            self.passes[index].pass.prepare(&mut ctx);
        }
        let prepare_upload_ms = prepare_started_at.elapsed().as_secs_f64() * 1000.0;

        let profile = CompileProfile {
            total_ms: compile_started_at.elapsed().as_secs_f64() * 1000.0,
            setup_passes_ms,
            annotate_resource_versions_ms,
            build_compiled_graph_ms,
            prepare_upload_ms,
            setup_pass_count: self.passes.len(),
            prepare_pass_count: self.order.len(),
            topology_cache_hit,
            graph: graph_profile,
        };

        Ok((profile, topology_hash, compiled_graph))
    }

    pub fn compile_with_upload(
        &mut self,
        viewport: &mut Viewport,
    ) -> Result<CompileProfile, FrameGraphError> {
        let mut profile = self.compile_profiled_internal()?;
        let textures = self.textures.clone();
        let buffers = self.buffers.clone();
        let (texture_allocations, texture_stable_keys, buffer_allocations) = {
            let compiled = self
                .compiled_graph
                .as_ref()
                .expect("compiled graph should exist");
            (
                compiled.texture_allocation_ids.clone(),
                compiled.texture_stable_keys.clone(),
                compiled.buffer_allocation_ids.clone(),
            )
        };
        let prepare_started_at = Instant::now();
        let mut ctx = PrepareContext::new(
            viewport,
            &textures,
            &buffers,
            &texture_allocations,
            &texture_stable_keys,
            &buffer_allocations,
        );
        for &index in &self.order {
            self.passes[index].pass.prepare(&mut ctx);
        }
        profile.prepare_upload_ms = prepare_started_at.elapsed().as_secs_f64() * 1000.0;
        profile.prepare_pass_count = self.order.len();
        profile.total_ms += profile.prepare_upload_ms;
        Ok(profile)
    }

    pub fn pass_descriptors(&self) -> Vec<&PassDescriptor> {
        self.passes.iter().map(|node| &node.descriptor).collect()
    }

    pub fn compiled_graph(&self) -> Option<&CompiledGraph> {
        self.compiled_graph.as_ref()
    }

    pub fn debug_graphics_split_reasons(&self) -> Vec<String> {
        fn step_label(step: &ExecuteStep, passes: &[PassNode]) -> Option<String> {
            match step {
                ExecuteStep::GraphicsPass { index } => {
                    Some(format!("{}#{}", passes[*index].pass.name(), index))
                }
                ExecuteStep::GraphicsPassGroup(group) => {
                    let first = group.pass_indices.first().copied()?;
                    let last = group.pass_indices.last().copied()?;
                    let first_name = passes[first].pass.name();
                    let last_name = passes[last].pass.name();
                    Some(format!(
                        "{}#{} .. {}#{} ({} passes)",
                        first_name,
                        first,
                        last_name,
                        last,
                        group.pass_indices.len()
                    ))
                }
                _ => None,
            }
        }

        fn step_key(step: &ExecuteStep, passes: &[PassNode]) -> Option<RenderPassCompatibilityKey> {
            match step {
                ExecuteStep::GraphicsPass { index } => {
                    render_pass_descriptor_compatibility(&passes[*index].descriptor)
                }
                ExecuteStep::GraphicsPassGroup(group) => Some(group.compatibility.clone()),
                _ => None,
            }
        }

        fn summarize_attachment_target(
            key: &RenderPassCompatibilityKey,
        ) -> Option<AttachmentTarget> {
            key.color_attachments
                .first()
                .map(|attachment| attachment.target)
        }

        fn split_reasons(
            left: &RenderPassCompatibilityKey,
            right: &RenderPassCompatibilityKey,
        ) -> Vec<String> {
            let mut reasons = Vec::new();
            if left.sample_count != right.sample_count {
                reasons.push(format!(
                    "sample_count {:?} -> {:?}",
                    left.sample_count, right.sample_count
                ));
            }
            if left.color_attachments != right.color_attachments {
                if left.color_attachments.len() != right.color_attachments.len() {
                    reasons.push(format!(
                        "color_attachment_count {} -> {}",
                        left.color_attachments.len(),
                        right.color_attachments.len()
                    ));
                } else {
                    for (idx, (a, b)) in left
                        .color_attachments
                        .iter()
                        .zip(right.color_attachments.iter())
                        .enumerate()
                    {
                        if a.target != b.target {
                            reasons.push(format!(
                                "color[{idx}].target {:?} -> {:?}",
                                a.target, b.target
                            ));
                        }
                        if a.resolve_target != b.resolve_target {
                            reasons.push(format!(
                                "color[{idx}].resolve {:?} -> {:?}",
                                a.resolve_target, b.resolve_target
                            ));
                        }
                        if a.load_op != b.load_op {
                            reasons.push(format!(
                                "color[{idx}].load {:?} -> {:?}",
                                a.load_op, b.load_op
                            ));
                        }
                        if a.store_op != b.store_op {
                            reasons.push(format!(
                                "color[{idx}].store {:?} -> {:?}",
                                a.store_op, b.store_op
                            ));
                        }
                        if a.clear_color_bits != b.clear_color_bits {
                            reasons.push(format!(
                                "color[{idx}].clear {:?} -> {:?}",
                                a.clear_color_bits, b.clear_color_bits
                            ));
                        }
                    }
                }
            }
            if left.depth_stencil_attachment != right.depth_stencil_attachment {
                match (
                    left.depth_stencil_attachment.as_ref(),
                    right.depth_stencil_attachment.as_ref(),
                ) {
                    (Some((lt, ld, ls)), Some((rt, rd, rs))) => {
                        if lt != rt {
                            reasons.push(format!("depth_stencil.target {:?} -> {:?}", lt, rt));
                        }
                        if ld != rd {
                            reasons.push(format!("depth {:?} -> {:?}", ld, rd));
                        }
                        if ls != rs {
                            reasons.push(format!("stencil {:?} -> {:?}", ls, rs));
                        }
                    }
                    (None, Some(_)) => reasons.push("depth_stencil none -> some".to_string()),
                    (Some(_), None) => reasons.push("depth_stencil some -> none".to_string()),
                    (None, None) => {}
                }
            }
            reasons
        }

        let mut out = Vec::new();
        let mut previous: Option<(&ExecuteStep, RenderPassCompatibilityKey, String)> = None;
        for step in &self.execute_steps {
            let Some(label) = step_label(step, &self.passes) else {
                previous = None;
                continue;
            };
            let Some(key) = step_key(step, &self.passes) else {
                previous = None;
                continue;
            };
            if let Some((_, prev_key, prev_label)) = &previous
                && prev_key != &key
            {
                let same_target =
                    summarize_attachment_target(prev_key) == summarize_attachment_target(&key);
                let reasons = split_reasons(prev_key, &key);
                let target_note = if same_target {
                    "same_target"
                } else {
                    "different_target"
                };
                out.push(format!(
                    "{target_note}: {}  ->  {} | {}",
                    prev_label,
                    label,
                    reasons.join(", ")
                ));
            }
            previous = Some((step, key, label));
        }
        out
    }

    fn annotate_resource_versions(&mut self) {
        let mut latest_texture_version: FxHashMap<TextureHandle, TextureVersionId> =
            FxHashMap::default();
        let mut latest_buffer_version: FxHashMap<BufferHandle, BufferVersionId> =
            FxHashMap::default();
        let mut version_producers: FxHashMap<ResourceVersionId, usize> = FxHashMap::default();
        let mut consumed_versions: Vec<ConsumedVersion> = Vec::new();
        let mut produced_versions: Vec<ProducedVersion> = Vec::new();
        let mut next_texture_version = 0_u32;
        let mut next_buffer_version = 0_u32;

        for (pass_index, node) in self.passes.iter_mut().enumerate() {
            // Borrow descriptor and usages as separate fields so no clone is needed.
            let descriptor = &node.descriptor;
            for usage in &mut node.usages {
                let (read_version, write_version) = annotate_usage_version(
                    usage.resource,
                    usage.usage,
                    descriptor,
                    &self.texture_metadata,
                    &mut latest_texture_version,
                    &mut latest_buffer_version,
                    &mut next_texture_version,
                    &mut next_buffer_version,
                );
                usage.read_version = read_version;
                usage.write_version = write_version;

                if let Some(version) = read_version {
                    consumed_versions.push(ConsumedVersion {
                        resource: usage.resource,
                        version,
                        consumer_pass_index: pass_index,
                        usage: usage.usage,
                    });
                }
                if let Some(version) = write_version {
                    version_producers.insert(version, pass_index);
                    produced_versions.push(ProducedVersion {
                        resource: usage.resource,
                        version,
                        producer_pass_index: pass_index,
                    });
                }
            }
        }

        let _ = (version_producers, consumed_versions, produced_versions);
    }

    fn build_compiled_graph_profiled(
        &self,
    ) -> Result<(CompiledGraph, CompileGraphProfile), FrameGraphError> {
        let total_started_at = Instant::now();

        let build_version_producers_started_at = Instant::now();
        let version_producers = self.build_version_producers();
        let build_version_producers_ms =
            build_version_producers_started_at.elapsed().as_secs_f64() * 1000.0;

        let latest_resource_versions_started_at = Instant::now();
        let latest_resource_versions = self.latest_resource_versions();
        let latest_resource_versions_ms =
            latest_resource_versions_started_at.elapsed().as_secs_f64() * 1000.0;

        let discover_sink_passes_started_at = Instant::now();
        let sink_passes =
            self.discover_sink_passes(&version_producers, &latest_resource_versions)?;
        let discover_sink_passes_ms =
            discover_sink_passes_started_at.elapsed().as_secs_f64() * 1000.0;

        let discover_live_passes_started_at = Instant::now();
        let live_passes = self.discover_live_passes(&sink_passes, &version_producers)?;
        let discover_live_passes_ms =
            discover_live_passes_started_at.elapsed().as_secs_f64() * 1000.0;

        let build_live_dependency_graph_started_at = Instant::now();
        let (graph_edges, indegree) =
            self.build_live_dependency_graph(&live_passes, &version_producers)?;
        let build_live_dependency_graph_ms = build_live_dependency_graph_started_at
            .elapsed()
            .as_secs_f64()
            * 1000.0;

        let toposort_live_passes_started_at = Instant::now();
        let ordered_passes = self.toposort_live_passes(&live_passes, &graph_edges, &indegree)?;
        let toposort_live_passes_ms =
            toposort_live_passes_started_at.elapsed().as_secs_f64() * 1000.0;

        let resolve_compiler_managed_attachments_started_at = Instant::now();
        let compiled_descriptors = self.resolve_compiler_managed_descriptors(&ordered_passes);
        let _resolve_compiler_managed_attachments_ms =
            resolve_compiler_managed_attachments_started_at
                .elapsed()
                .as_secs_f64()
                * 1000.0;

        let build_execution_plan_started_at = Instant::now();
        let execution_steps = self.build_execution_plan(&ordered_passes, &compiled_descriptors);
        let build_execution_plan_ms =
            build_execution_plan_started_at.elapsed().as_secs_f64() * 1000.0;

        let build_resource_state_timelines_started_at = Instant::now();
        let (pass_state_transitions, resource_transitions, resource_timelines) =
            self.build_resource_state_timelines(&ordered_passes)?;
        let build_resource_state_timelines_ms = build_resource_state_timelines_started_at
            .elapsed()
            .as_secs_f64()
            * 1000.0;

        let build_compiled_resources_started_at = Instant::now();
        let (resources, allocation_plan, texture_allocation_ids, buffer_allocation_ids) =
            self.build_compiled_resources(&live_passes, &ordered_passes);
        let build_compiled_resources_ms =
            build_compiled_resources_started_at.elapsed().as_secs_f64() * 1000.0;

        let texture_stable_keys = resources
            .iter()
            .filter_map(|resource| match resource.handle {
                ResourceHandle::Texture(handle) => resource.stable_key.map(|key| (handle, key)),
                _ => None,
            })
            .collect::<FxHashMap<_, _>>();
        let culled_passes = (0..self.passes.len())
            .filter(|index| !live_passes.contains(index))
            .collect::<Vec<_>>();
        let assemble_compiled_passes_started_at = Instant::now();
        let compiled_passes = ordered_passes
            .iter()
            .map(|&index| {
                let mut dependencies = graph_edges[index].iter().copied().collect::<Vec<_>>();
                dependencies.sort_unstable();
                CompiledPass {
                    original_index: index,
                    name: self.passes[index].descriptor.name,
                    descriptor: compiled_descriptors[index].clone(),
                    dependencies,
                    resource_usages: self.passes[index].usages.clone(),
                    input_versions: self.pass_consumed_versions(index),
                    output_versions: self.pass_produced_versions(index),
                    resource_transitions: pass_state_transitions
                        .get(&index)
                        .cloned()
                        .unwrap_or_default(),
                    is_root: sink_passes.contains(&index),
                }
            })
            .collect::<Vec<_>>();
        let assemble_compiled_passes_ms =
            assemble_compiled_passes_started_at.elapsed().as_secs_f64() * 1000.0;

        let compiled_graph = CompiledGraph {
            passes: compiled_passes,
            resources,
            external_sinks: self.external_sinks.clone(),
            allocation_plan,
            culled_passes,
            execution_plan: ExecutionPlan {
                ordered_passes,
                steps: execution_steps,
            },
            resource_transitions,
            resource_timelines,
            texture_allocation_ids,
            buffer_allocation_ids,
            texture_stable_keys,
        };
        let profile = CompileGraphProfile {
            total_ms: total_started_at.elapsed().as_secs_f64() * 1000.0,
            build_version_producers_ms,
            latest_resource_versions_ms,
            discover_sink_passes_ms,
            discover_live_passes_ms,
            build_live_dependency_graph_ms,
            toposort_live_passes_ms,
            build_execution_plan_ms,
            build_resource_state_timelines_ms,
            build_compiled_resources_ms,
            assemble_compiled_passes_ms,
            sink_pass_count: sink_passes.len(),
            live_pass_count: live_passes.len(),
            ordered_pass_count: compiled_graph.execution_plan.ordered_passes.len(),
            execution_step_count: compiled_graph.execution_plan.steps.len(),
            compiled_resource_count: compiled_graph.resources.len(),
            culled_pass_count: compiled_graph.culled_passes.len(),
            live_dependency_edge_count: count_graph_edges(&graph_edges, &live_passes),
            graphics_pass_count: count_live_passes_by_kind(self, &live_passes, PassKind::Graphics),
            compute_pass_count: count_live_passes_by_kind(self, &live_passes, PassKind::Compute),
            transfer_pass_count: count_live_passes_by_kind(self, &live_passes, PassKind::Transfer),
            graphics_step_count: count_graphics_steps(&compiled_graph.execution_plan.steps),
            graphics_group_count: count_graphics_groups(&compiled_graph.execution_plan.steps),
            max_graphics_group_size: max_graphics_group_size(&compiled_graph.execution_plan.steps),
            pass_name_counts: summarize_pass_name_counts(self, &live_passes, 8),
            versioned_resource_counts: summarize_versioned_resource_counts(self, &live_passes, 8),
            top_indegree_passes: summarize_degree_counts(self, &live_passes, &indegree, 5),
            top_outdegree_passes: summarize_outdegree_counts(self, &live_passes, &graph_edges, 5),
        };

        Ok((compiled_graph, profile))
    }

    fn discover_sink_passes(
        &self,
        version_producers: &FxHashMap<ResourceVersionId, usize>,
        latest_resource_versions: &FxHashMap<ResourceHandle, ResourceVersionId>,
    ) -> Result<Vec<usize>, FrameGraphError> {
        if self.external_sinks.is_empty() {
            return Ok((0..self.passes.len()).collect());
        }

        let mut sink_passes = Vec::new();
        let mut seen = FxHashSet::default();
        for sink in &self.external_sinks {
            match sink.target {
                ExternalSinkTarget::Pass(pass) => {
                    let index = pass.0;
                    if index >= self.passes.len() {
                        return Err(FrameGraphError::MissingOutput(
                            "external sink references an unknown pass",
                        ));
                    }
                    if seen.insert(index) {
                        sink_passes.push(index);
                    }
                }
                ExternalSinkTarget::Resource(source) => {
                    let Some(version) = latest_resource_versions.get(&source).copied() else {
                        return Err(FrameGraphError::MissingInput(
                            "external sink source has no produced version",
                        ));
                    };
                    let Some(&producer) = version_producers.get(&version) else {
                        return Err(FrameGraphError::MissingInput(
                            "external sink source has no producer",
                        ));
                    };
                    if seen.insert(producer) {
                        sink_passes.push(producer);
                    }
                }
            }
        }

        Ok(sink_passes)
    }

    fn discover_live_passes(
        &self,
        sink_passes: &[usize],
        version_producers: &FxHashMap<ResourceVersionId, usize>,
    ) -> Result<FxHashSet<usize>, FrameGraphError> {
        let mut live = FxHashSet::default();
        let mut stack = sink_passes.to_vec();
        while let Some(pass_index) = stack.pop() {
            if !live.insert(pass_index) {
                continue;
            }
            for usage in &self.passes[pass_index].usages {
                let Some(version) = usage.read_version else {
                    continue;
                };
                if let Some(&producer) = version_producers.get(&version) {
                    if producer != pass_index {
                        stack.push(producer);
                    }
                } else if !self.resource_has_external_input(usage.resource) {
                    return Err(FrameGraphError::MissingInput(
                        "live pass requires a resource version without a producer",
                    ));
                }
            }
        }
        Ok(live)
    }

    fn build_live_dependency_graph(
        &self,
        live_passes: &FxHashSet<usize>,
        version_producers: &FxHashMap<ResourceVersionId, usize>,
    ) -> Result<(Vec<FxHashSet<usize>>, Vec<usize>), FrameGraphError> {
        let mut indegree = vec![0usize; self.passes.len()];
        let mut graph_edges: Vec<FxHashSet<usize>> = vec![FxHashSet::default(); self.passes.len()];

        self.validate_live_passes(live_passes, version_producers)?;

        for &index in live_passes {
            for usage in &self.passes[index].usages {
                let Some(version) = usage.read_version else {
                    continue;
                };
                if let Some(&producer) = version_producers.get(&version)
                    && producer != index
                    && graph_edges[producer].insert(index)
                {
                    indegree[index] += 1;
                }
            }
        }

        Ok((graph_edges, indegree))
    }

    fn validate_live_passes(
        &self,
        live_passes: &FxHashSet<usize>,
        version_producers: &FxHashMap<ResourceVersionId, usize>,
    ) -> Result<(), FrameGraphError> {
        for &index in live_passes {
            validate_pass_descriptor(&self.passes[index].descriptor, &self.textures)?;

            for usage in &self.passes[index].usages {
                let Some(version) = usage.read_version else {
                    continue;
                };
                if version_producers.contains_key(&version) {
                    continue;
                }
                if self.resource_has_external_input(usage.resource) {
                    continue;
                }

                return Err(FrameGraphError::MissingInput(
                    "resource version is consumed before any valid producer",
                ));
            }
        }

        Ok(())
    }

    fn build_version_producers(&self) -> FxHashMap<ResourceVersionId, usize> {
        let mut producers = FxHashMap::default();
        for (pass_index, node) in self.passes.iter().enumerate() {
            for usage in &node.usages {
                if let Some(version) = usage.write_version {
                    producers.insert(version, pass_index);
                }
            }
        }
        producers
    }

    fn latest_resource_versions(&self) -> FxHashMap<ResourceHandle, ResourceVersionId> {
        let mut latest = FxHashMap::default();
        for node in &self.passes {
            for usage in &node.usages {
                if let Some(version) = usage.write_version {
                    latest.insert(usage.resource, version);
                }
            }
        }
        latest
    }

    fn pass_consumed_versions(&self, pass_index: usize) -> Vec<ConsumedVersion> {
        let mut seen = FxHashSet::default();
        let mut versions = Vec::new();
        for usage in &self.passes[pass_index].usages {
            let Some(version) = usage.read_version else {
                continue;
            };
            if seen.insert((usage.resource, version, usage.usage)) {
                versions.push(ConsumedVersion {
                    resource: usage.resource,
                    version,
                    consumer_pass_index: pass_index,
                    usage: usage.usage,
                });
            }
        }
        versions
    }

    fn pass_produced_versions(&self, pass_index: usize) -> Vec<ProducedVersion> {
        let mut seen = FxHashSet::default();
        let mut versions = Vec::new();
        for usage in &self.passes[pass_index].usages {
            let Some(version) = usage.write_version else {
                continue;
            };
            if seen.insert((usage.resource, version)) {
                versions.push(ProducedVersion {
                    resource: usage.resource,
                    version,
                    producer_pass_index: pass_index,
                });
            }
        }
        versions
    }

    fn resource_has_external_input(&self, resource: ResourceHandle) -> bool {
        matches!(
            self.resource_metadata(resource).lifetime,
            ResourceLifetime::Imported | ResourceLifetime::Persistent
        )
    }

    fn compute_batch_anchor_info(
        &self,
        live_passes: &FxHashSet<usize>,
        graph_edges: &[FxHashSet<usize>],
        indegree: &[usize],
        compatibility_keys: &[Option<RenderPassCompatibilityKey>],
    ) -> Vec<BatchAnchorInfo> {
        let analysis_order = topological_order_for_analysis(live_passes, graph_edges, indegree);
        let mut info = vec![BatchAnchorInfo::default(); self.passes.len()];

        for &pass_index in analysis_order.iter().rev() {
            // Find the closest downstream anchor (minimum distance), by index only.
            let mut best_downstream_consumer: Option<usize> = None;
            for &consumer in &graph_edges[pass_index] {
                if !live_passes.contains(&consumer) || info[consumer].anchor_pass_index.is_none() {
                    continue;
                }
                if best_downstream_consumer.is_none_or(|best| {
                    info[consumer].distance_to_anchor < info[best].distance_to_anchor
                }) {
                    best_downstream_consumer = Some(consumer);
                }
            }

            info[pass_index] = if let Some(consumer) = best_downstream_consumer {
                // Propagate the anchor from the downstream consumer (no clone needed).
                BatchAnchorInfo {
                    anchor_pass_index: info[consumer].anchor_pass_index,
                    distance_to_anchor: info[consumer].distance_to_anchor.saturating_add(1),
                }
            } else if is_mergeable_graphics_pass(&self.passes[pass_index].descriptor)
                && compatibility_keys[pass_index].is_some()
            {
                // This pass is itself a mergeable anchor.
                BatchAnchorInfo {
                    anchor_pass_index: Some(pass_index),
                    distance_to_anchor: 0,
                }
            } else {
                BatchAnchorInfo::default()
            };
        }

        info
    }

    fn toposort_live_passes(
        &self,
        live_passes: &FxHashSet<usize>,
        graph_edges: &[FxHashSet<usize>],
        indegree: &[usize],
    ) -> Result<Vec<usize>, FrameGraphError> {
        let mut indegree = indegree.to_vec();
        let mut order = Vec::new();
        let mut queue: FxHashSet<usize> = indegree
            .iter()
            .enumerate()
            .filter_map(|(idx, &deg)| {
                if deg == 0 && live_passes.contains(&idx) {
                    Some(idx)
                } else {
                    None
                }
            })
            .collect();

        let compatibility_keys: Vec<Option<RenderPassCompatibilityKey>> = self
            .passes
            .iter()
            .enumerate()
            .map(|(index, node)| {
                if !live_passes.contains(&index) {
                    return None;
                }
                render_pass_compatibility_key(&node.descriptor)
            })
            .collect();
        let batch_anchor_info = self.compute_batch_anchor_info(
            live_passes,
            graph_edges,
            &indegree,
            &compatibility_keys,
        );
        let mut last_signature: Option<&RenderPassCompatibilityKey> = None;

        while !queue.is_empty() {
            let n = select_next_ready_node(
                &queue,
                &compatibility_keys,
                &batch_anchor_info,
                last_signature,
                graph_edges,
                &indegree,
                live_passes,
            );
            queue.remove(&n);
            order.push(n);
            last_signature = compatibility_keys[n].as_ref();
            for &m in &graph_edges[n] {
                indegree[m] -= 1;
                if indegree[m] == 0 && live_passes.contains(&m) {
                    queue.insert(m);
                }
            }
        }

        if order.len() != live_passes.len() {
            return Err(FrameGraphError::CyclicDependency);
        }

        Ok(order)
    }

    fn build_execution_plan(
        &self,
        order: &[usize],
        descriptors: &[PassDescriptor],
    ) -> Vec<CompiledExecuteStep> {
        let mut steps = Vec::new();
        let mut cursor = 0usize;
        while cursor < order.len() {
            let index = order[cursor];
            match descriptors[index].kind {
                PassKind::Graphics => {
                    let Some(current_key) = render_pass_compatibility_key(&descriptors[index])
                    else {
                        steps.push(CompiledExecuteStep::GraphicsPass { pass_index: index });
                        cursor += 1;
                        continue;
                    };
                    let mut pass_indices = vec![index];
                    let mut end = cursor + 1;
                    let mut absorbed_load_variant: Option<RenderPassCompatibilityKey> = None;
                    while end < order.len() {
                        let next_index = order[end];
                        if descriptors[next_index].kind != PassKind::Graphics {
                            break;
                        }
                        let Some(next_key) =
                            render_pass_compatibility_key(&descriptors[next_index])
                        else {
                            break;
                        };
                        let matches_current = current_key == next_key;
                        let matches_absorbed = absorbed_load_variant
                            .as_ref()
                            .is_some_and(|key| key == &next_key);
                        if !matches_current && !matches_absorbed {
                            if absorbed_load_variant.is_none()
                                && can_absorb_leading_clear_pass(&current_key, &next_key)
                            {
                                absorbed_load_variant = Some(next_key);
                            } else {
                                break;
                            }
                        }
                        pass_indices.push(next_index);
                        end += 1;
                    }
                    if pass_indices.len() > 1 {
                        steps.push(CompiledExecuteStep::GraphicsPassGroup(RenderPassGroup {
                            pass_indices,
                            compatibility: current_key,
                        }));
                        cursor = end;
                    } else {
                        steps.push(CompiledExecuteStep::GraphicsPass { pass_index: index });
                        cursor += 1;
                    }
                }
                PassKind::Compute => {
                    steps.push(CompiledExecuteStep::ComputePass { pass_index: index });
                    cursor += 1;
                }
                PassKind::Transfer => {
                    steps.push(CompiledExecuteStep::TransferPass { pass_index: index });
                    cursor += 1;
                }
            }
        }
        steps
    }

    fn resolve_compiler_managed_descriptors(
        &self,
        ordered_passes: &[usize],
    ) -> Vec<PassDescriptor> {
        let mut descriptors = self
            .passes
            .iter()
            .map(|node| node.descriptor.clone())
            .collect::<Vec<_>>();
        let mut seen_transient_color_targets = FxHashSet::<TextureHandle>::default();

        for &pass_index in ordered_passes {
            let PassDetails::Graphics(graphics) = &mut descriptors[pass_index].details else {
                continue;
            };

            for attachment in &mut graphics.color_attachments {
                if attachment.load_op != AttachmentLoadOp::Load {
                    continue;
                }
                let AttachmentTarget::Texture(handle) = attachment.target else {
                    continue;
                };
                if self
                    .resource_metadata(ResourceHandle::Texture(handle))
                    .lifetime
                    != ResourceLifetime::Transient
                {
                    continue;
                }
                if seen_transient_color_targets.insert(handle) {
                    attachment.load_op = AttachmentLoadOp::Clear;
                    attachment.clear_color = Some([0.0, 0.0, 0.0, 0.0]);
                }
            }
        }

        descriptors
    }

    fn build_compiled_resources(
        &self,
        live_passes: &FxHashSet<usize>,
        ordered_passes: &[usize],
    ) -> (
        Vec<CompiledResource>,
        AllocationPlan,
        FxHashMap<TextureHandle, AllocationId>,
        FxHashMap<BufferHandle, AllocationId>,
    ) {
        let ordered_positions = ordered_passes
            .iter()
            .enumerate()
            .map(|(order, &pass_index)| (pass_index, order))
            .collect::<FxHashMap<_, _>>();
        let mut resources = FxHashMap::<ResourceHandle, CompiledResource>::default();

        for (pass_index, node) in self.passes.iter().enumerate() {
            if !live_passes.contains(&pass_index) {
                continue;
            }
            let Some(order_index) = ordered_positions.get(&pass_index).copied() else {
                continue;
            };
            let mut producers = FxHashSet::default();
            let mut consumers = FxHashSet::default();
            for usage in &node.usages {
                let resource = usage.resource;
                let metadata = self.resource_metadata(resource);
                let compiled = resources.entry(resource).or_insert(CompiledResource {
                    handle: resource,
                    stable_key: metadata.stable_key,
                    kind: metadata.kind,
                    allocation_class: metadata.allocation_class,
                    lifetime: metadata.lifetime,
                    first_use_pass_index: order_index,
                    last_use_pass_index: order_index,
                    producer_passes: Vec::new(),
                    consumer_passes: Vec::new(),
                    allocation_id: None,
                    allocation_owner: AllocationOwner::AllocatorManaged,
                });
                compiled.first_use_pass_index = compiled.first_use_pass_index.min(order_index);
                compiled.last_use_pass_index = compiled.last_use_pass_index.max(order_index);
                if usage.write_version.is_some() && producers.insert(resource) {
                    compiled.producer_passes.push(pass_index);
                }
                if usage.read_version.is_some() && consumers.insert(resource) {
                    compiled.consumer_passes.push(pass_index);
                }
            }
        }

        let mut ordered = resources.into_values().collect::<Vec<_>>();
        ordered.sort_by_key(|resource| match resource.handle {
            ResourceHandle::Texture(handle) => (0, handle.0),
            ResourceHandle::Buffer(handle) => (1, handle.0),
        });
        let (allocation_plan, texture_allocation_ids, buffer_allocation_ids) =
            build_allocation_plan(&ordered, ordered_passes, self);

        for resource in &mut ordered {
            match resource.handle {
                ResourceHandle::Texture(handle) => {
                    resource.allocation_id = texture_allocation_ids.get(&handle).copied();
                }
                ResourceHandle::Buffer(handle) => {
                    resource.allocation_id = buffer_allocation_ids.get(&handle).copied();
                }
            }
            if resource.lifetime == ResourceLifetime::Imported {
                resource.allocation_owner = AllocationOwner::ExternalOwned;
            }
        }

        (
            ordered,
            allocation_plan,
            texture_allocation_ids,
            buffer_allocation_ids,
        )
    }

    fn build_resource_state_timelines(
        &self,
        ordered_passes: &[usize],
    ) -> Result<
        (
            FxHashMap<usize, Vec<CompiledPassResourceTransition>>,
            Vec<CompiledResourceTransition>,
            Vec<CompiledResourceTimeline>,
        ),
        FrameGraphError,
    > {
        let mut current_states = FxHashMap::<ResourceHandle, ResourceState>::default();
        let mut pass_transitions =
            FxHashMap::<usize, Vec<CompiledPassResourceTransition>>::default();
        let mut resource_transitions = Vec::<CompiledResourceTransition>::new();
        let mut timeline_map =
            FxHashMap::<ResourceHandle, Vec<CompiledResourceTransition>>::default();

        for (execution_index, &pass_index) in ordered_passes.iter().enumerate() {
            let per_resource = group_pass_usages_by_resource(&self.passes[pass_index].usages);
            let mut pass_entries = Vec::with_capacity(per_resource.len());

            for (resource, usages) in per_resource {
                let before = current_states.get(&resource).copied().unwrap_or_else(|| {
                    ResourceState::undefined(self.resource_metadata(resource).kind)
                });
                let after = derive_resource_state(
                    self.passes[pass_index].descriptor.name,
                    resource,
                    &usages,
                )?;
                let pass_transition = CompiledPassResourceTransition {
                    resource,
                    before,
                    after,
                };
                let resource_transition = CompiledResourceTransition {
                    resource,
                    pass_index,
                    execution_index,
                    before,
                    after,
                };
                pass_entries.push(pass_transition);
                resource_transitions.push(resource_transition);
                timeline_map
                    .entry(resource)
                    .or_default()
                    .push(resource_transition);
                current_states.insert(resource, after);
            }

            pass_transitions.insert(pass_index, pass_entries);
        }

        let mut resource_timelines = timeline_map
            .into_iter()
            .map(|(resource, transitions)| CompiledResourceTimeline {
                resource,
                initial_state: ResourceState::undefined(self.resource_metadata(resource).kind),
                transitions,
            })
            .collect::<Vec<_>>();
        resource_timelines.sort_by_key(|timeline| resource_sort_key(timeline.resource));

        Ok((pass_transitions, resource_transitions, resource_timelines))
    }

    fn resource_metadata(&self, handle: ResourceHandle) -> ResourceMetadata {
        match handle {
            ResourceHandle::Texture(handle) => self
                .texture_metadata
                .get(handle.0 as usize)
                .copied()
                .unwrap_or_else(|| {
                    ResourceMetadata::transient(ResourceKind::Texture, AllocationClass::Texture)
                }),
            ResourceHandle::Buffer(handle) => self
                .buffer_metadata
                .get(handle.0 as usize)
                .copied()
                .unwrap_or_else(|| {
                    ResourceMetadata::transient(ResourceKind::Buffer, AllocationClass::Buffer)
                }),
        }
    }

    fn describe_resource_handle(&self, handle: ResourceHandle) -> String {
        match handle {
            ResourceHandle::Texture(texture) => format!("tex#{}", texture.0),
            ResourceHandle::Buffer(buffer) => format!("buf#{}", buffer.0),
        }
    }

    pub fn to_dot(&self) -> String {
        fn escape_dot_label(text: &str) -> String {
            text.replace('\\', "\\\\")
                .replace('\"', "\\\"")
                .replace('\n', "\\n")
        }

        fn resource_label(handle: ResourceHandle) -> String {
            match handle {
                ResourceHandle::Texture(h) => format!("tex#{}", h.0),
                ResourceHandle::Buffer(h) => format!("buf#{}", h.0),
            }
        }

        fn resource_node_id(handle: ResourceHandle) -> String {
            match handle {
                ResourceHandle::Texture(h) => format!("r_tex_{}", h.0),
                ResourceHandle::Buffer(h) => format!("r_buf_{}", h.0),
            }
        }

        fn resource_sort_key(handle: ResourceHandle) -> (u8, u32) {
            match handle {
                ResourceHandle::Texture(h) => (0, h.0),
                ResourceHandle::Buffer(h) => (1, h.0),
            }
        }

        let mut resources: FxHashSet<ResourceHandle> = FxHashSet::default();
        let mut write_edges: FxHashSet<(usize, ResourceHandle)> = FxHashSet::default();
        let mut read_edges: FxHashSet<(ResourceHandle, usize)> = FxHashSet::default();
        let mut modify_edges: FxHashSet<(usize, ResourceHandle)> = FxHashSet::default();

        for (index, node) in self.passes.iter().enumerate() {
            for usage in &node.usages {
                resources.insert(usage.resource);
                match usage.usage.effective_access() {
                    ResourceAccess::Read => {
                        read_edges.insert((usage.resource, index));
                    }
                    ResourceAccess::Write => {
                        write_edges.insert((index, usage.resource));
                    }
                    ResourceAccess::Modify => {
                        modify_edges.insert((index, usage.resource));
                    }
                }
            }
        }

        let mut resource_nodes = resources.into_iter().collect::<Vec<_>>();
        resource_nodes.sort_by_key(|handle| resource_sort_key(*handle));
        let mut write_edges = write_edges.into_iter().collect::<Vec<_>>();
        write_edges.sort_by_key(|(from, handle)| (*from, resource_sort_key(*handle)));
        let mut read_edges = read_edges.into_iter().collect::<Vec<_>>();
        read_edges.sort_by_key(|(handle, to)| (resource_sort_key(*handle), *to));
        let mut modify_edges = modify_edges.into_iter().collect::<Vec<_>>();
        modify_edges.sort_by_key(|(from, handle)| (*from, resource_sort_key(*handle)));

        let mut dot = String::new();
        dot.push_str("digraph FrameGraph {\n");
        dot.push_str("  rankdir=LR;\n");
        dot.push_str("  graph [splines=true, ranksep=1.0, nodesep=0.35];\n");
        dot.push_str("  node [fontname=\"Helvetica\"];\n");
        dot.push_str("  edge [fontname=\"Helvetica\"];\n");
        dot.push_str("  node [shape=box, style=rounded];\n");
        for (index, node) in self.passes.iter().enumerate() {
            let label = escape_dot_label(&format!(
                "{}\\n{:?}",
                node.descriptor.name, node.descriptor.kind
            ));
            dot.push_str(&format!("  p{index} [label=\"{index}: {label}\"];\n"));
        }
        dot.push_str("  node [shape=ellipse, style=solid];\n");
        for handle in &resource_nodes {
            let node_id = resource_node_id(*handle);
            let label = escape_dot_label(&resource_label(*handle));
            dot.push_str(&format!("  {node_id} [label=\"{label}\"];\n"));
        }
        if !self.passes.is_empty() {
            dot.push_str("  { rank=same; ");
            for index in 0..self.passes.len() {
                dot.push_str(&format!("p{index}; "));
            }
            dot.push_str("}\n");
        }
        for (from, handle) in write_edges {
            let node_id = resource_node_id(handle);
            let label = escape_dot_label(&resource_label(handle));
            dot.push_str(&format!(
                "  p{from} -> {node_id} [color=\"red\", fontcolor=\"red\", label=\"{label}\", constraint=false];\n"
            ));
        }
        for (handle, to) in read_edges {
            let node_id = resource_node_id(handle);
            let label = escape_dot_label(&resource_label(handle));
            dot.push_str(&format!(
                "  {node_id} -> p{to} [color=\"blue\", fontcolor=\"blue\", label=\"{label}\", constraint=false];\n"
            ));
        }
        for (from, handle) in modify_edges {
            let node_id = resource_node_id(handle);
            let label = escape_dot_label(&resource_label(handle));
            dot.push_str(&format!(
                "  {node_id} -> p{from} [color=\"purple\", fontcolor=\"purple\", label=\"{label} (modify)\", constraint=false];\n"
            ));
            dot.push_str(&format!(
                "  p{from} -> {node_id} [color=\"purple\", fontcolor=\"purple\", label=\"{label} (modify)\", constraint=false];\n"
            ));
        }
        dot.push_str("}\n");
        dot
    }

    pub(crate) fn execute_profiled(
        &mut self,
        viewport: &mut Viewport,
    ) -> Result<ExecuteProfile, FrameGraphError> {
        if !self.compiled {
            return Err(FrameGraphError::NotCompiled);
        }
        let execute_started_at = Instant::now();
        let mut pass_timings: FxHashMap<String, f64> = FxHashMap::default();
        let mut pass_counts: FxHashMap<String, usize> = FxHashMap::default();
        let mut pass_first_seen_order: Vec<String> = Vec::new();
        let textures = self.textures.clone();
        let buffers = self.buffers.clone();
        let (texture_allocations, texture_stable_keys, buffer_allocations) = {
            let compiled = self
                .compiled_graph
                .as_ref()
                .expect("compiled graph should exist");
            (
                compiled.texture_allocation_ids.clone(),
                compiled.texture_stable_keys.clone(),
                compiled.buffer_allocation_ids.clone(),
            )
        };
        let mut ctx = RecordContext::new(
            viewport,
            &textures,
            &buffers,
            &texture_allocations,
            &texture_stable_keys,
            &buffer_allocations,
        );
        // Take execute_steps out of self so we can call &mut self methods while iterating,
        // then restore it afterward — zero clones, no heap allocations.
        let execute_steps = std::mem::take(&mut self.execute_steps);
        for step in &execute_steps {
            match step {
                ExecuteStep::GraphicsPass { index } => self.execute_graphics_pass(
                    *index,
                    &mut ctx,
                    &mut pass_timings,
                    &mut pass_counts,
                    &mut pass_first_seen_order,
                ),
                ExecuteStep::ComputePass { index } => self.execute_compute_pass(
                    *index,
                    &mut ctx,
                    &mut pass_timings,
                    &mut pass_counts,
                    &mut pass_first_seen_order,
                ),
                ExecuteStep::TransferPass { index } => self.execute_transfer_pass(
                    *index,
                    &mut ctx,
                    &mut pass_timings,
                    &mut pass_counts,
                    &mut pass_first_seen_order,
                ),
                ExecuteStep::GraphicsPassGroup(group) => self.execute_graphics_group(
                    group,
                    &mut ctx,
                    &mut pass_timings,
                    &mut pass_counts,
                    &mut pass_first_seen_order,
                ),
            }
        }
        self.execute_steps = execute_steps;
        let mut top_passes: Vec<(String, f64)> = pass_timings
            .iter()
            .map(|(name, ms)| (name.clone(), *ms))
            .collect();
        top_passes.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        if top_passes.len() > 6 {
            top_passes.truncate(6);
        }
        let ordered_passes = pass_first_seen_order
            .into_iter()
            .filter_map(|name| {
                let elapsed_ms = pass_timings.get(&name).copied()?;
                let count = pass_counts.get(&name).copied().unwrap_or(0);
                Some((name, elapsed_ms, count))
            })
            .collect();
        Ok(ExecuteProfile {
            total_ms: execute_started_at.elapsed().as_secs_f64() * 1000.0,
            pass_count: self.order.len(),
            ordered_passes,
            detail_ordered: ctx.take_detail_timings(),
        })
    }
}

#[derive(Clone, Debug, Default)]
struct BatchAnchorInfo {
    /// Index into the `compatibility_keys` slice of the downstream anchor pass, if any.
    anchor_pass_index: Option<usize>,
    distance_to_anchor: usize,
}

impl FrameGraph {
    #[allow(clippy::too_many_arguments)]
    fn execute_graphics_pass(
        &mut self,
        index: usize,
        ctx: &mut RecordContext<'_, '_>,
        pass_timings: &mut FxHashMap<String, f64>,
        pass_counts: &mut FxHashMap<String, usize>,
        pass_first_seen_order: &mut Vec<String>,
    ) {
        let encoder_ptr = {
            let Some(parts) = ctx.viewport.frame_parts() else {
                return;
            };
            parts.encoder as *mut wgpu::CommandEncoder
        };
        let result = catch_unwind(AssertUnwindSafe(|| {
            let encoder = unsafe { &mut *encoder_ptr };
            let compatibility =
                render_pass_descriptor_compatibility(&self.passes[index].descriptor)
                    .expect("graphics pass descriptor should produce render-pass compatibility");
            self.execute_graphics_passes(
                &[index],
                &compatibility,
                ctx,
                encoder,
                pass_timings,
                pass_counts,
                pass_first_seen_order,
            );
        }));
        if let Err(payload) = result {
            let detail = if let Some(message) = payload.downcast_ref::<&str>() {
                *message
            } else if let Some(message) = payload.downcast_ref::<String>() {
                message.as_str()
            } else {
                "unknown panic payload"
            };
            eprintln!(
                "[warn] render pass panicked and was skipped: {} ({})",
                self.passes[index].pass.name(),
                detail
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn execute_compute_pass(
        &mut self,
        index: usize,
        ctx: &mut RecordContext<'_, '_>,
        pass_timings: &mut FxHashMap<String, f64>,
        pass_counts: &mut FxHashMap<String, usize>,
        pass_first_seen_order: &mut Vec<String>,
    ) {
        let pass_name = self.passes[index].pass.name().to_string();
        let pass_started_at = Instant::now();
        let encoder_ptr = {
            let Some(parts) = ctx.viewport.frame_parts() else {
                return;
            };
            parts.encoder as *mut wgpu::CommandEncoder
        };
        let result = catch_unwind(AssertUnwindSafe(|| {
            let encoder = unsafe { &mut *encoder_ptr };
            let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("FrameGraph ComputePass"),
                ..Default::default()
            });
            let mut compute_ctx = ComputeRecordContext::new(ctx);
            let mut compute_ctx =
                ComputeCtx::from_compute_pass(&mut compute_ctx, &mut compute_pass);
            self.passes[index].pass.execute_compute(&mut compute_ctx);
        }));
        record_pass_timing(
            pass_name,
            pass_started_at,
            pass_timings,
            pass_counts,
            pass_first_seen_order,
        );
        if let Err(payload) = result {
            let detail = if let Some(message) = payload.downcast_ref::<&str>() {
                *message
            } else if let Some(message) = payload.downcast_ref::<String>() {
                message.as_str()
            } else {
                "unknown panic payload"
            };
            eprintln!(
                "[warn] compute pass panicked and was skipped: {} ({})",
                self.passes[index].pass.name(),
                detail
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn execute_transfer_pass(
        &mut self,
        index: usize,
        ctx: &mut RecordContext<'_, '_>,
        pass_timings: &mut FxHashMap<String, f64>,
        pass_counts: &mut FxHashMap<String, usize>,
        pass_first_seen_order: &mut Vec<String>,
    ) {
        let pass_name = self.passes[index].pass.name().to_string();
        let pass_started_at = Instant::now();
        let encoder_ptr = {
            let Some(parts) = ctx.viewport.frame_parts() else {
                return;
            };
            parts.encoder as *mut wgpu::CommandEncoder
        };
        let result = catch_unwind(AssertUnwindSafe(|| {
            let encoder = unsafe { &mut *encoder_ptr };
            let mut transfer_ctx = TransferRecordContext::new(ctx);
            let mut transfer_ctx = TransferCtx::new(&mut transfer_ctx, encoder);
            self.passes[index].pass.execute_transfer(&mut transfer_ctx);
        }));
        record_pass_timing(
            pass_name,
            pass_started_at,
            pass_timings,
            pass_counts,
            pass_first_seen_order,
        );
        if let Err(payload) = result {
            let detail = if let Some(message) = payload.downcast_ref::<&str>() {
                *message
            } else if let Some(message) = payload.downcast_ref::<String>() {
                message.as_str()
            } else {
                "unknown panic payload"
            };
            eprintln!(
                "[warn] transfer pass panicked and was skipped: {} ({})",
                self.passes[index].pass.name(),
                detail
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn execute_graphics_group(
        &mut self,
        group: &RenderPassGroup,
        ctx: &mut RecordContext<'_, '_>,
        pass_timings: &mut FxHashMap<String, f64>,
        pass_counts: &mut FxHashMap<String, usize>,
        pass_first_seen_order: &mut Vec<String>,
    ) {
        let Some(parts) = ctx.viewport.frame_parts() else {
            return;
        };
        let encoder = parts.encoder as *mut wgpu::CommandEncoder;
        let encoder = unsafe { &mut *encoder };
        self.execute_graphics_passes(
            &group.pass_indices,
            &group.compatibility,
            ctx,
            encoder,
            pass_timings,
            pass_counts,
            pass_first_seen_order,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn execute_graphics_passes(
        &mut self,
        pass_indices: &[usize],
        compatibility: &RenderPassCompatibilityKey,
        ctx: &mut RecordContext<'_, '_>,
        encoder: &mut wgpu::CommandEncoder,
        pass_timings: &mut FxHashMap<String, f64>,
        pass_counts: &mut FxHashMap<String, usize>,
        pass_first_seen_order: &mut Vec<String>,
    ) {
        let pass_names = pass_indices
            .iter()
            .map(|&index| self.passes[index].descriptor.name.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let (surface_view, surface_resolve_view, depth_view) = {
            let Some(parts) = ctx.viewport.frame_parts() else {
                return;
            };
            (
                parts.view.clone(),
                parts.resolve_view.cloned(),
                parts.depth_view.cloned(),
            )
        };
        let mut owned_color_views: Vec<(wgpu::TextureView, Option<wgpu::TextureView>)> = Vec::new();
        let mut color_attachments = Vec::with_capacity(compatibility.color_attachments.len());
        for attachment in &compatibility.color_attachments {
            let (view, resolve_target) = resolve_color_attachment_views(
                ctx,
                attachment.target,
                attachment.resolve_target,
                &surface_view,
                surface_resolve_view.as_ref(),
            );
            let Some(view) = view else {
                return;
            };
            owned_color_views.push((view, resolve_target));
        }
        let expected_sample_count = resolve_pass_sample_count(compatibility.sample_count);
        for attachment in &compatibility.color_attachments {
            let actual_sample_count =
                color_attachment_sample_count(ctx, attachment.target, attachment.resolve_target);
            if expected_sample_count != actual_sample_count {
                eprintln!(
                    "[error] frame graph color attachment sample-count mismatch before wgpu validation: passes=[{}], target={:?}, expected_sample_count={}, actual_sample_count={}, resolve_target={:?}",
                    pass_names,
                    attachment.target,
                    expected_sample_count,
                    actual_sample_count,
                    attachment.resolve_target,
                );
                panic!(
                    "frame graph color attachment sample-count mismatch: passes=[{}]",
                    pass_names
                );
            }
        }
        if let Some((target, _, _)) = compatibility.depth_stencil_attachment.as_ref() {
            let depth_sample_count = depth_attachment_sample_count(ctx, *target);
            if expected_sample_count != depth_sample_count {
                eprintln!(
                    "[error] frame graph depth attachment sample-count mismatch before wgpu validation: passes=[{}], expected_sample_count={}, depth_target={:?}, depth_sample_count={}",
                    pass_names, expected_sample_count, target, depth_sample_count,
                );
                panic!(
                    "frame graph depth attachment sample-count mismatch: passes=[{}]",
                    pass_names
                );
            }
        }
        for (index, attachment) in compatibility.color_attachments.iter().enumerate() {
            let (view, resolve_target) = &owned_color_views[index];
            color_attachments.push(Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: resolve_target.as_ref(),
                ops: wgpu::Operations {
                    load: to_wgpu_load_op(attachment.load_op, attachment.clear_color_bits),
                    store: to_wgpu_store_op(attachment.store_op),
                },
                depth_slice: None,
            }));
        }
        let owned_depth_attachment =
            compatibility
                .depth_stencil_attachment
                .as_ref()
                .and_then(|(target, depth, stencil)| {
                    let view = match target {
                        AttachmentTarget::Surface => depth_view.clone(),
                        AttachmentTarget::Texture(handle) => {
                            render_target_attachment_view(ctx, *handle)
                        }
                    }?;
                    Some((view, depth.clone(), stencil.clone()))
                });
        let depth_attachment = owned_depth_attachment
            .as_ref()
            .map(
                |(view, depth, stencil)| wgpu::RenderPassDepthStencilAttachment {
                    view,
                    depth_ops: depth.as_ref().map(|aspect| wgpu::Operations {
                        load: to_wgpu_depth_load_op(aspect.load_op, aspect.clear_depth_bits),
                        store: to_wgpu_store_op(aspect.store_op),
                    }),
                    stencil_ops: stencil.as_ref().map(|aspect| wgpu::Operations {
                        load: to_wgpu_stencil_load_op(aspect.load_op, aspect.clear_stencil),
                        store: to_wgpu_store_op(aspect.store_op),
                    }),
                },
            );
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("FrameGraph GraphicsGroup"),
            color_attachments: &color_attachments,
            depth_stencil_attachment: depth_attachment,
            ..Default::default()
        });

        for &index in pass_indices {
            let pass_name = self.passes[index].pass.name().to_string();
            let pass_started_at = Instant::now();
            let mut graphics_ctx = GraphicsRecordContext::new(ctx);
            let mut pass_ctx = GraphicsCtx::new(&mut graphics_ctx, &mut render_pass);
            self.passes[index].pass.execute_graphics(&mut pass_ctx);
            record_pass_timing(
                pass_name,
                pass_started_at,
                pass_timings,
                pass_counts,
                pass_first_seen_order,
            );
        }
    }
}

fn record_pass_timing(
    pass_name: String,
    pass_started_at: Instant,
    pass_timings: &mut FxHashMap<String, f64>,
    pass_counts: &mut FxHashMap<String, usize>,
    pass_first_seen_order: &mut Vec<String>,
) {
    let elapsed_ms = pass_started_at.elapsed().as_secs_f64() * 1000.0;
    if !pass_timings.contains_key(&pass_name) {
        pass_first_seen_order.push(pass_name.clone());
    }
    *pass_timings.entry(pass_name.clone()).or_insert(0.0) += elapsed_ms;
    *pass_counts.entry(pass_name).or_insert(0) += 1;
}

fn resolve_color_attachment_views(
    ctx: &mut RecordContext<'_, '_>,
    target: AttachmentTarget,
    resolve_target: Option<AttachmentTarget>,
    surface_view: &wgpu::TextureView,
    surface_resolve_view: Option<&wgpu::TextureView>,
) -> (Option<wgpu::TextureView>, Option<wgpu::TextureView>) {
    match target {
        AttachmentTarget::Surface => {
            if resolve_target.is_some() {
                (
                    Some(surface_view.clone()),
                    resolve_target.and_then(|_| surface_resolve_view.cloned()),
                )
            } else {
                let attachment_view = surface_resolve_view
                    .cloned()
                    .unwrap_or_else(|| surface_view.clone());
                (Some(attachment_view), None)
            }
        }
        AttachmentTarget::Texture(handle) => {
            let resolve_view = render_target_view(ctx, handle);
            if resolve_target.is_some() && render_target_msaa_view(ctx, handle).is_none() {
                panic!(
                    "frame graph resolve target mismatch before wgpu validation: texture {:?} requested resolve attachment but has no MSAA view",
                    handle
                );
            }
            let attachment_view = if resolve_target.is_some() {
                render_target_msaa_view(ctx, handle).or_else(|| resolve_view.clone())
            } else {
                resolve_view.clone()
            };
            (
                attachment_view,
                resolve_view.filter(|_| resolve_target.is_some()),
            )
        }
    }
}

fn resolve_pass_sample_count(policy: SampleCountPolicy) -> u32 {
    match policy {
        SampleCountPolicy::Fixed(count) => count.max(1),
        SampleCountPolicy::SurfaceDefault => 1,
    }
}

fn color_attachment_sample_count(
    ctx: &mut RecordContext<'_, '_>,
    target: AttachmentTarget,
    resolve_target: Option<AttachmentTarget>,
) -> u32 {
    match target {
        AttachmentTarget::Surface => 1,
        AttachmentTarget::Texture(handle) => {
            if resolve_target.is_some() && render_target_msaa_view(ctx, handle).is_some() {
                ctx.viewport.msaa_sample_count()
            } else {
                1
            }
        }
    }
}

fn depth_attachment_sample_count(ctx: &mut RecordContext<'_, '_>, target: AttachmentTarget) -> u32 {
    match target {
        AttachmentTarget::Surface => 1,
        AttachmentTarget::Texture(handle) => {
            let Some(desc) = ctx.textures().get(handle.0 as usize) else {
                return 1;
            };
            desc.sample_count().max(1)
        }
    }
}

fn to_wgpu_store_op(store_op: AttachmentStoreOp) -> wgpu::StoreOp {
    match store_op {
        AttachmentStoreOp::Store => wgpu::StoreOp::Store,
        AttachmentStoreOp::Discard => wgpu::StoreOp::Discard,
    }
}

fn to_wgpu_load_op(
    load_op: AttachmentLoadOp,
    clear_color_bits: Option<[u64; 4]>,
) -> wgpu::LoadOp<wgpu::Color> {
    match load_op {
        AttachmentLoadOp::Load => wgpu::LoadOp::Load,
        AttachmentLoadOp::Clear => {
            let [r, g, b, a] = clear_color_bits
                .expect("clear color bits should exist when load_op is Clear")
                .map(f64::from_bits);
            wgpu::LoadOp::Clear(wgpu::Color { r, g, b, a })
        }
    }
}

fn to_wgpu_depth_load_op(
    load_op: AttachmentLoadOp,
    clear_depth_bits: Option<u32>,
) -> wgpu::LoadOp<f32> {
    match load_op {
        AttachmentLoadOp::Load => wgpu::LoadOp::Load,
        AttachmentLoadOp::Clear => wgpu::LoadOp::Clear(
            clear_depth_bits
                .map(f32::from_bits)
                .expect("clear depth bits should exist when load_op is Clear"),
        ),
    }
}

fn to_wgpu_stencil_load_op(
    load_op: AttachmentLoadOp,
    clear_stencil: Option<u32>,
) -> wgpu::LoadOp<u32> {
    match load_op {
        AttachmentLoadOp::Load => wgpu::LoadOp::Load,
        AttachmentLoadOp::Clear => wgpu::LoadOp::Clear(
            clear_stencil.expect("clear stencil should exist when load_op is Clear"),
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn annotate_usage_version(
    resource: ResourceHandle,
    usage: ResourceUsage,
    descriptor: &PassDescriptor,
    texture_metadata: &[ResourceMetadata],
    latest_texture_version: &mut FxHashMap<TextureHandle, TextureVersionId>,
    latest_buffer_version: &mut FxHashMap<BufferHandle, BufferVersionId>,
    next_texture_version: &mut u32,
    next_buffer_version: &mut u32,
) -> (Option<ResourceVersionId>, Option<ResourceVersionId>) {
    match resource {
        ResourceHandle::Texture(handle) => annotate_texture_usage_version(
            handle,
            usage,
            descriptor,
            texture_metadata,
            latest_texture_version,
            next_texture_version,
        ),
        ResourceHandle::Buffer(handle) => {
            annotate_buffer_usage_version(handle, usage, latest_buffer_version, next_buffer_version)
        }
    }
}

fn annotate_texture_usage_version(
    handle: TextureHandle,
    usage: ResourceUsage,
    descriptor: &PassDescriptor,
    texture_metadata: &[ResourceMetadata],
    latest_versions: &mut FxHashMap<TextureHandle, TextureVersionId>,
    next_version: &mut u32,
) -> (Option<ResourceVersionId>, Option<ResourceVersionId>) {
    let current = latest_versions
        .get(&handle)
        .copied()
        .map(ResourceVersionId::Texture);
    let lifetime = texture_metadata
        .get(handle.0 as usize)
        .map(|metadata| metadata.lifetime)
        .unwrap_or(ResourceLifetime::Transient);
    match usage {
        ResourceUsage::Produced => {
            let write = ResourceVersionId::Texture(allocate_texture_version(
                handle,
                latest_versions,
                next_version,
            ));
            (None, Some(write))
        }
        ResourceUsage::SampledRead
        | ResourceUsage::DepthRead
        | ResourceUsage::StencilRead
        | ResourceUsage::CopySrc
        | ResourceUsage::UniformRead
        | ResourceUsage::VertexRead
        | ResourceUsage::IndexRead
        | ResourceUsage::StorageRead => (
            current.or_else(|| {
                Some(ResourceVersionId::Texture(allocate_texture_version(
                    handle,
                    latest_versions,
                    next_version,
                )))
            }),
            None,
        ),
        ResourceUsage::ColorAttachmentWrite => {
            let should_preserve_existing =
                color_attachment_requires_input(descriptor, ResourceHandle::Texture(handle))
                    && !(current.is_none() && lifetime == ResourceLifetime::Transient);
            let read = if should_preserve_existing {
                current.or_else(|| {
                    Some(ResourceVersionId::Texture(allocate_texture_version(
                        handle,
                        latest_versions,
                        next_version,
                    )))
                })
            } else {
                None
            };
            let write = ResourceVersionId::Texture(allocate_texture_version(
                handle,
                latest_versions,
                next_version,
            ));
            (read, Some(write))
        }
        ResourceUsage::DepthWrite => {
            let read = if depth_stencil_aspect_requires_input(
                descriptor,
                ResourceHandle::Texture(handle),
                true,
            ) {
                current.or_else(|| {
                    Some(ResourceVersionId::Texture(allocate_texture_version(
                        handle,
                        latest_versions,
                        next_version,
                    )))
                })
            } else {
                None
            };
            let write = ResourceVersionId::Texture(allocate_texture_version(
                handle,
                latest_versions,
                next_version,
            ));
            (read, Some(write))
        }
        ResourceUsage::StencilWrite => {
            let read = if depth_stencil_aspect_requires_input(
                descriptor,
                ResourceHandle::Texture(handle),
                false,
            ) {
                current.or_else(|| {
                    Some(ResourceVersionId::Texture(allocate_texture_version(
                        handle,
                        latest_versions,
                        next_version,
                    )))
                })
            } else {
                None
            };
            let write = ResourceVersionId::Texture(allocate_texture_version(
                handle,
                latest_versions,
                next_version,
            ));
            (read, Some(write))
        }
        ResourceUsage::CopyDst | ResourceUsage::StorageWrite => {
            let write = ResourceVersionId::Texture(allocate_texture_version(
                handle,
                latest_versions,
                next_version,
            ));
            (None, Some(write))
        }
        ResourceUsage::BufferWrite => (None, None),
    }
}

fn annotate_buffer_usage_version(
    handle: BufferHandle,
    usage: ResourceUsage,
    latest_versions: &mut FxHashMap<BufferHandle, BufferVersionId>,
    next_version: &mut u32,
) -> (Option<ResourceVersionId>, Option<ResourceVersionId>) {
    let current = latest_versions
        .get(&handle)
        .copied()
        .map(ResourceVersionId::Buffer);
    match usage {
        ResourceUsage::Produced => {
            let write = ResourceVersionId::Buffer(allocate_buffer_version(
                handle,
                latest_versions,
                next_version,
            ));
            (None, Some(write))
        }
        ResourceUsage::BufferWrite | ResourceUsage::CopyDst | ResourceUsage::StorageWrite => {
            let write = ResourceVersionId::Buffer(allocate_buffer_version(
                handle,
                latest_versions,
                next_version,
            ));
            (None, Some(write))
        }
        ResourceUsage::UniformRead
        | ResourceUsage::VertexRead
        | ResourceUsage::IndexRead
        | ResourceUsage::CopySrc
        | ResourceUsage::StorageRead => (
            current.or_else(|| {
                Some(ResourceVersionId::Buffer(allocate_buffer_version(
                    handle,
                    latest_versions,
                    next_version,
                )))
            }),
            None,
        ),
        ResourceUsage::SampledRead
        | ResourceUsage::ColorAttachmentWrite
        | ResourceUsage::DepthRead
        | ResourceUsage::DepthWrite
        | ResourceUsage::StencilRead
        | ResourceUsage::StencilWrite => (None, None),
    }
}

fn allocate_texture_version(
    handle: TextureHandle,
    latest_versions: &mut FxHashMap<TextureHandle, TextureVersionId>,
    next_version: &mut u32,
) -> TextureVersionId {
    let version = TextureVersionId(*next_version);
    *next_version = next_version.saturating_add(1);
    latest_versions.insert(handle, version);
    version
}

fn allocate_buffer_version(
    handle: BufferHandle,
    latest_versions: &mut FxHashMap<BufferHandle, BufferVersionId>,
    next_version: &mut u32,
) -> BufferVersionId {
    let version = BufferVersionId(*next_version);
    *next_version = next_version.saturating_add(1);
    latest_versions.insert(handle, version);
    version
}

fn color_attachment_requires_input(descriptor: &PassDescriptor, resource: ResourceHandle) -> bool {
    let PassDetails::Graphics(graphics) = &descriptor.details else {
        return false;
    };
    graphics.color_attachments.iter().any(|attachment| {
        attachment.load_op == AttachmentLoadOp::Load
            && matches_resource_target(resource, attachment.target)
    })
}

fn depth_stencil_aspect_requires_input(
    descriptor: &PassDescriptor,
    resource: ResourceHandle,
    depth: bool,
) -> bool {
    let PassDetails::Graphics(graphics) = &descriptor.details else {
        return false;
    };
    let Some(attachment) = graphics.depth_stencil_attachment else {
        return false;
    };
    if !matches_resource_target(resource, attachment.target) {
        return false;
    }
    if depth {
        attachment
            .depth
            .is_some_and(|aspect| aspect.load_op == AttachmentLoadOp::Load)
    } else {
        attachment
            .stencil
            .is_some_and(|aspect| aspect.load_op == AttachmentLoadOp::Load)
    }
}

fn matches_resource_target(resource: ResourceHandle, target: AttachmentTarget) -> bool {
    match (resource, target) {
        (ResourceHandle::Texture(handle), AttachmentTarget::Texture(target_handle)) => {
            handle == target_handle
        }
        _ => false,
    }
}

fn resource_sort_key(handle: ResourceHandle) -> (u8, u32) {
    match handle {
        ResourceHandle::Texture(h) => (0, h.0),
        ResourceHandle::Buffer(h) => (1, h.0),
    }
}

fn group_pass_usages_by_resource(
    usages: &[PassResourceUsage],
) -> Vec<(ResourceHandle, Vec<ResourceUsage>)> {
    let mut grouped = FxHashMap::<ResourceHandle, Vec<ResourceUsage>>::default();
    for usage in usages {
        grouped.entry(usage.resource).or_default().push(usage.usage);
    }
    let mut grouped = grouped.into_iter().collect::<Vec<_>>();
    grouped.sort_by_key(|(resource, _)| resource_sort_key(*resource));
    grouped
}

fn derive_resource_state(
    pass_name: &str,
    resource: ResourceHandle,
    usages: &[ResourceUsage],
) -> Result<ResourceState, FrameGraphError> {
    match resource {
        ResourceHandle::Texture(_) => {
            derive_texture_resource_state(pass_name, usages).map(ResourceState::Texture)
        }
        ResourceHandle::Buffer(_) => {
            derive_buffer_resource_state(pass_name, usages).map(ResourceState::Buffer)
        }
    }
}

fn derive_texture_resource_state(
    pass_name: &str,
    usages: &[ResourceUsage],
) -> Result<TextureResourceState, FrameGraphError> {
    let mut state: Option<TextureResourceState> = None;
    let mut depth = None;
    let mut stencil = None;

    for usage in usages {
        match usage {
            ResourceUsage::Produced => {}
            ResourceUsage::ColorAttachmentWrite => {
                merge_texture_state_candidate(
                    pass_name,
                    &mut state,
                    TextureResourceState::ColorAttachment,
                )?;
            }
            ResourceUsage::DepthRead => {
                depth = Some(TextureAspectState::Read);
            }
            ResourceUsage::DepthWrite => {
                depth = Some(TextureAspectState::Write);
            }
            ResourceUsage::StencilRead => {
                stencil = Some(TextureAspectState::Read);
            }
            ResourceUsage::StencilWrite => {
                stencil = Some(TextureAspectState::Write);
            }
            ResourceUsage::SampledRead => {
                merge_texture_state_candidate(
                    pass_name,
                    &mut state,
                    TextureResourceState::Sampled,
                )?;
            }
            ResourceUsage::CopySrc => {
                merge_texture_state_candidate(
                    pass_name,
                    &mut state,
                    TextureResourceState::CopySrc,
                )?;
            }
            ResourceUsage::CopyDst => {
                merge_texture_state_candidate(
                    pass_name,
                    &mut state,
                    TextureResourceState::CopyDst,
                )?;
            }
            ResourceUsage::StorageRead => {
                merge_texture_state_candidate(
                    pass_name,
                    &mut state,
                    TextureResourceState::StorageRead,
                )?;
            }
            ResourceUsage::StorageWrite => {
                merge_texture_state_candidate(
                    pass_name,
                    &mut state,
                    TextureResourceState::StorageWrite,
                )?;
            }
            ResourceUsage::BufferWrite
            | ResourceUsage::UniformRead
            | ResourceUsage::VertexRead
            | ResourceUsage::IndexRead => {
                return Err(FrameGraphError::Validation(format!(
                    "{pass_name} declares buffer usage on a texture resource"
                )));
            }
        }
    }

    if depth.is_some() || stencil.is_some() {
        merge_texture_state_candidate(
            pass_name,
            &mut state,
            TextureResourceState::DepthStencilAttachment { depth, stencil },
        )?;
    }

    Ok(state.unwrap_or(TextureResourceState::Undefined))
}

fn merge_texture_state_candidate(
    pass_name: &str,
    current: &mut Option<TextureResourceState>,
    next: TextureResourceState,
) -> Result<(), FrameGraphError> {
    match current {
        None => {
            *current = Some(next);
            Ok(())
        }
        Some(existing) if *existing == next => Ok(()),
        Some(existing) => Err(FrameGraphError::Validation(format!(
            "{pass_name} declares incompatible texture states in one pass: {existing:?} vs {next:?}"
        ))),
    }
}

fn derive_buffer_resource_state(
    pass_name: &str,
    usages: &[ResourceUsage],
) -> Result<BufferResourceState, FrameGraphError> {
    let mut state: Option<BufferResourceState> = None;

    for usage in usages {
        let candidate = match usage {
            ResourceUsage::Produced => continue,
            ResourceUsage::BufferWrite => BufferResourceState::Written,
            ResourceUsage::UniformRead => BufferResourceState::UniformRead,
            ResourceUsage::VertexRead => BufferResourceState::VertexRead,
            ResourceUsage::IndexRead => BufferResourceState::IndexRead,
            ResourceUsage::StorageRead => BufferResourceState::StorageRead,
            ResourceUsage::StorageWrite => BufferResourceState::StorageWrite,
            ResourceUsage::CopySrc => BufferResourceState::CopySrc,
            ResourceUsage::CopyDst => BufferResourceState::CopyDst,
            ResourceUsage::SampledRead
            | ResourceUsage::ColorAttachmentWrite
            | ResourceUsage::DepthRead
            | ResourceUsage::DepthWrite
            | ResourceUsage::StencilRead
            | ResourceUsage::StencilWrite => {
                return Err(FrameGraphError::Validation(format!(
                    "{pass_name} declares texture usage on a buffer resource"
                )));
            }
        };
        match state {
            None => state = Some(candidate),
            Some(existing) if existing == candidate => {}
            Some(existing) => {
                return Err(FrameGraphError::Validation(format!(
                    "{pass_name} declares incompatible buffer states in one pass: {existing:?} vs {candidate:?}"
                )));
            }
        }
    }

    Ok(state.unwrap_or(BufferResourceState::Undefined))
}

fn validate_pass_descriptor(
    descriptor: &PassDescriptor,
    textures: &[TextureDesc],
) -> Result<(), FrameGraphError> {
    let PassDetails::Graphics(graphics) = &descriptor.details else {
        return Ok(());
    };

    if graphics.requirements.requires_color_attachment && graphics.color_attachments.is_empty() {
        return Err(FrameGraphError::Validation(format!(
            "{} requires a color attachment",
            descriptor.name
        )));
    }

    if graphics.requirements.uses_depth
        && graphics
            .depth_stencil_attachment
            .is_none_or(|attachment| attachment.depth.is_none())
    {
        return Err(FrameGraphError::Validation(format!(
            "{} requires depth but no depth attachment was declared",
            descriptor.name
        )));
    }

    if graphics.requirements.uses_stencil
        && graphics
            .depth_stencil_attachment
            .is_none_or(|attachment| attachment.stencil.is_none())
    {
        return Err(FrameGraphError::Validation(format!(
            "{} requires stencil but no stencil attachment was declared",
            descriptor.name
        )));
    }

    if let Some(attachment) = graphics.depth_stencil_attachment {
        if let AttachmentTarget::Texture(handle) = attachment.target {
            let Some(desc) = textures.get(handle.0 as usize) else {
                return Err(FrameGraphError::Validation(format!(
                    "{} references missing depth/stencil texture",
                    descriptor.name
                )));
            };
            let format = desc.format();
            if attachment.depth.is_some() && !format.has_depth_aspect() {
                return Err(FrameGraphError::Validation(format!(
                    "{} uses depth on non-depth texture {:?}",
                    descriptor.name, format
                )));
            }
            if attachment.stencil.is_some() && !format.has_stencil_aspect() {
                return Err(FrameGraphError::Validation(format!(
                    "{} uses stencil on non-stencil texture {:?}",
                    descriptor.name, format
                )));
            }
        }
    }

    for attachment in &graphics.color_attachments {
        if let AttachmentTarget::Texture(handle) = attachment.target {
            let Some(desc) = textures.get(handle.0 as usize) else {
                return Err(FrameGraphError::Validation(format!(
                    "{} references missing color attachment texture",
                    descriptor.name
                )));
            };
            if desc.format().has_depth_aspect() || desc.format().has_stencil_aspect() {
                return Err(FrameGraphError::Validation(format!(
                    "{} declares color attachment with depth/stencil format {:?}",
                    descriptor.name,
                    desc.format()
                )));
            }
        }
    }

    if let SampleCountPolicy::Fixed(0) = graphics.sample_count {
        return Err(FrameGraphError::Validation(format!(
            "{} declares invalid sample count 0",
            descriptor.name
        )));
    }

    Ok(())
}

fn select_next_ready_node(
    queue: &FxHashSet<usize>,
    signatures: &[Option<RenderPassCompatibilityKey>],
    batch_anchor_info: &[BatchAnchorInfo],
    last_signature: Option<&RenderPassCompatibilityKey>,
    graph_edges: &[FxHashSet<usize>],
    indegree: &[usize],
    live_passes: &FxHashSet<usize>,
) -> usize {
    if let Some(last_signature) = last_signature {
        let mut best: Option<usize> = None;
        for &idx in queue {
            if signatures[idx]
                .as_ref()
                .is_some_and(|signature| signature == last_signature)
            {
                if best.is_none_or(|current| idx < current) {
                    best = Some(idx);
                }
            }
        }
        if let Some(best) = best {
            return best;
        }
    }

    // Single pass: group by anchor_signature, tracking (count, best_distance, best_idx).
    // This replaces the two separate scans that previously built anchor_counts then searched it,
    // and eliminates the per-element clone of RenderPassCompatibilityKey.
    struct AnchorGroupBest {
        count: usize,
        best_distance: usize,
        best_idx: usize,
    }
    let mut anchor_groups: FxHashMap<&RenderPassCompatibilityKey, AnchorGroupBest> =
        FxHashMap::default();
    for &idx in queue {
        // Resolve the anchor's signature through the index stored in BatchAnchorInfo —
        // no clone required; we borrow directly from the signatures slice.
        let Some(anchor_pass_idx) = batch_anchor_info[idx].anchor_pass_index else {
            continue;
        };
        let Some(anchor_signature) = signatures[anchor_pass_idx].as_ref() else {
            continue;
        };
        let distance = batch_anchor_info[idx].distance_to_anchor;
        let entry = anchor_groups
            .entry(anchor_signature)
            .or_insert(AnchorGroupBest {
                count: 0,
                best_distance: 0,
                best_idx: usize::MAX,
            });
        entry.count += 1;
        if distance > entry.best_distance
            || (distance == entry.best_distance && idx < entry.best_idx)
        {
            entry.best_distance = distance;
            entry.best_idx = idx;
        }
    }
    // Iterate over groups (O(unique anchor signatures), not O(queue)).
    let best_anchor_choice = anchor_groups
        .values()
        .max_by(|a, b| {
            a.count
                .cmp(&b.count)
                .then(a.best_distance.cmp(&b.best_distance))
                .then(b.best_idx.cmp(&a.best_idx))
        })
        .map(|g| (g.best_idx, g.count, g.best_distance));
    if let Some((idx, ready_count, distance)) = best_anchor_choice
        && (ready_count > 1 || distance > 0)
    {
        return idx;
    }

    // Allocate simulation state once; estimate_compatible_run_length uses an undo log
    // to restore both after each candidate evaluation, avoiding per-call clones.
    let mut sim_ready = queue.clone();
    let mut sim_indegree = indegree.to_vec();
    let mut best_graphics_choice: Option<(usize, usize)> = None;
    for &idx in queue {
        let Some(signature) = signatures[idx].as_ref() else {
            continue;
        };
        let run_len = estimate_compatible_run_length(
            idx,
            signature,
            &mut sim_ready,
            &mut sim_indegree,
            signatures,
            graph_edges,
            live_passes,
        );
        if best_graphics_choice
            .as_ref()
            .is_none_or(|(best_idx, best_len)| {
                run_len > *best_len || (run_len == *best_len && idx < *best_idx)
            })
        {
            best_graphics_choice = Some((idx, run_len));
        }
    }

    if let Some((idx, run_len)) = best_graphics_choice
        && run_len > 1
    {
        return idx;
    }

    queue.iter().copied().min().unwrap_or(0)
}

fn topological_order_for_analysis(
    live_passes: &FxHashSet<usize>,
    graph_edges: &[FxHashSet<usize>],
    indegree: &[usize],
) -> Vec<usize> {
    let mut indegree = indegree.to_vec();
    let mut queue: VecDeque<usize> = indegree
        .iter()
        .enumerate()
        .filter_map(|(idx, &deg)| {
            if deg == 0 && live_passes.contains(&idx) {
                Some(idx)
            } else {
                None
            }
        })
        .collect();
    let mut order = Vec::with_capacity(live_passes.len());
    while let Some(node) = queue.pop_front() {
        order.push(node);
        for &next in &graph_edges[node] {
            indegree[next] = indegree[next].saturating_sub(1);
            if indegree[next] == 0 && live_passes.contains(&next) {
                queue.push_back(next);
            }
        }
    }
    order
}

fn estimate_compatible_run_length(
    start_idx: usize,
    target_signature: &RenderPassCompatibilityKey,
    ready: &mut FxHashSet<usize>,
    sim_indegree: &mut Vec<usize>,
    signatures: &[Option<RenderPassCompatibilityKey>],
    graph_edges: &[FxHashSet<usize>],
    live_passes: &FxHashSet<usize>,
) -> usize {
    // Undo logs so we can restore `ready` and `sim_indegree` after the simulation
    // instead of cloning them fresh on every call.
    let mut removed_from_ready: Vec<usize> = Vec::new();
    let mut added_to_ready: Vec<usize> = Vec::new();
    // (index, value_before_decrement) — must be restored in reverse order.
    let mut decremented: Vec<(usize, usize)> = Vec::new();

    let mut run_len = 0usize;
    let mut current = Some(start_idx);

    while let Some(node) = current.take() {
        if !ready.remove(&node) {
            break;
        }
        removed_from_ready.push(node);
        run_len += 1;
        for &next in &graph_edges[node] {
            let old = sim_indegree[next];
            let new_val = old.saturating_sub(1);
            if new_val != old {
                sim_indegree[next] = new_val;
                decremented.push((next, old));
            }
            if sim_indegree[next] == 0 && live_passes.contains(&next) {
                if ready.insert(next) {
                    added_to_ready.push(next);
                }
            }
        }
        current = ready
            .iter()
            .copied()
            .filter(|&idx| {
                signatures[idx]
                    .as_ref()
                    .is_some_and(|signature| signature == target_signature)
            })
            .min();
    }

    // Restore state. Decrements are replayed in reverse to handle the case where
    // the same index was decremented multiple times within a single simulation run.
    for idx in added_to_ready {
        ready.remove(&idx);
    }
    for idx in removed_from_ready {
        ready.insert(idx);
    }
    for (idx, old_val) in decremented.into_iter().rev() {
        sim_indegree[idx] = old_val;
    }

    run_len
}

fn is_rect_pass_name(name: &str) -> bool {
    name == std::any::type_name::<DrawRectPass>()
        || name == std::any::type_name::<OpaqueRectPass>()
        || name.starts_with("DrawRectPass::")
        || name.starts_with("OpaqueRectPass::")
}

fn is_mergeable_graphics_pass(descriptor: &PassDescriptor) -> bool {
    matches!(
        &descriptor.details,
        PassDetails::Graphics(graphics)
            if graphics.merge_policy == GraphicsPassMergePolicy::Mergeable
    )
}

fn render_pass_compatibility_key(
    descriptor: &PassDescriptor,
) -> Option<RenderPassCompatibilityKey> {
    let PassDetails::Graphics(graphics) = &descriptor.details else {
        return None;
    };
    if graphics.merge_policy != GraphicsPassMergePolicy::Mergeable {
        return None;
    }

    Some(render_pass_compatibility_for_graphics(graphics))
}

fn render_pass_descriptor_compatibility(
    descriptor: &PassDescriptor,
) -> Option<RenderPassCompatibilityKey> {
    let PassDetails::Graphics(graphics) = &descriptor.details else {
        return None;
    };
    Some(render_pass_compatibility_for_graphics(graphics))
}

fn can_absorb_leading_clear_pass(
    clear_key: &RenderPassCompatibilityKey,
    load_key: &RenderPassCompatibilityKey,
) -> bool {
    if clear_key.sample_count != load_key.sample_count {
        return false;
    }
    if clear_key.color_attachments.len() != load_key.color_attachments.len() {
        return false;
    }
    for (clear, load) in clear_key
        .color_attachments
        .iter()
        .zip(load_key.color_attachments.iter())
    {
        if clear.target != load.target
            || clear.resolve_target != load.resolve_target
            || clear.store_op != load.store_op
        {
            return false;
        }
        if clear.load_op != AttachmentLoadOp::Clear {
            return false;
        }
        if load.load_op != AttachmentLoadOp::Load {
            return false;
        }
    }

    match (
        clear_key.depth_stencil_attachment.as_ref(),
        load_key.depth_stencil_attachment.as_ref(),
    ) {
        (None, None) => {}
        (
            Some((clear_target, clear_depth, clear_stencil)),
            Some((load_target, load_depth, load_stencil)),
        ) => {
            if clear_target != load_target {
                return false;
            }
            match (clear_depth, load_depth) {
                (None, None) => {}
                (Some(clear_depth), Some(load_depth)) => {
                    if clear_depth.store_op != load_depth.store_op
                        || clear_depth.load_op != AttachmentLoadOp::Clear
                        || load_depth.load_op != AttachmentLoadOp::Load
                    {
                        return false;
                    }
                }
                _ => return false,
            }
            match (clear_stencil, load_stencil) {
                (None, None) => {}
                (Some(clear_stencil), Some(load_stencil)) => {
                    if clear_stencil.store_op != load_stencil.store_op
                        || clear_stencil.load_op != AttachmentLoadOp::Clear
                        || load_stencil.load_op != AttachmentLoadOp::Load
                    {
                        return false;
                    }
                }
                _ => return false,
            }
        }
        _ => return false,
    }

    true
}

fn render_pass_compatibility_for_graphics(
    graphics: &GraphicsPassDescriptor,
) -> RenderPassCompatibilityKey {
    RenderPassCompatibilityKey {
        color_attachments: graphics
            .color_attachments
            .iter()
            .map(|attachment| RenderPassCompatibleColorAttachment {
                target: attachment.target,
                resolve_target: resolve_target_for_attachment(
                    attachment.target,
                    graphics.sample_count,
                ),
                load_op: attachment.load_op,
                store_op: attachment.store_op,
                clear_color_bits: attachment.clear_color.map(|color| color.map(f64::to_bits)),
            })
            .collect(),
        depth_stencil_attachment: graphics.depth_stencil_attachment.map(|attachment| {
            (
                attachment.target,
                attachment
                    .depth
                    .map(|depth| RenderPassCompatibleDepthAspect {
                        load_op: depth.load_op,
                        store_op: depth.store_op,
                        clear_depth_bits: depth.clear_depth.map(f32::to_bits),
                    }),
                attachment
                    .stencil
                    .map(|stencil| RenderPassCompatibleStencilAspect {
                        load_op: stencil.load_op,
                        store_op: stencil.store_op,
                        clear_stencil: stencil.clear_stencil,
                    }),
            )
        }),
        sample_count: graphics.sample_count,
    }
}

fn resolve_target_for_attachment(
    target: AttachmentTarget,
    sample_count: SampleCountPolicy,
) -> Option<AttachmentTarget> {
    if matches!(target, AttachmentTarget::Surface) {
        return None;
    }
    match sample_count {
        SampleCountPolicy::Fixed(count) if count <= 1 => None,
        _ => Some(target),
    }
}

fn batch_trace_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("RFGUI_TRACE_BATCH")
            .ok()
            .is_some_and(|value| value == "1")
    })
}

fn build_allocation_plan(
    resources: &[CompiledResource],
    ordered_passes: &[usize],
    graph: &FrameGraph,
) -> (
    AllocationPlan,
    FxHashMap<TextureHandle, AllocationId>,
    FxHashMap<BufferHandle, AllocationId>,
) {
    #[derive(Clone)]
    struct TextureSlot {
        id: AllocationId,
        desc: TextureDesc,
        last_use_pass_index: usize,
    }

    #[derive(Clone)]
    struct TextureCandidate {
        handle: TextureHandle,
        desc: TextureDesc,
        first_use_pass_index: usize,
        last_use_pass_index: usize,
    }

    let mut next_id = 0u32;
    let mut texture_slots: Vec<TextureSlot> = Vec::new();
    let mut texture_allocations: Vec<TextureAllocationPlanEntry> = Vec::new();
    // Index from AllocationId → position in texture_allocations for O(1) lookup.
    let mut texture_allocation_index: FxHashMap<AllocationId, usize> = FxHashMap::default();
    let mut buffer_allocations: Vec<BufferAllocationPlanEntry> = Vec::new();
    let mut texture_allocation_ids = FxHashMap::default();
    let mut buffer_allocation_ids = FxHashMap::default();

    let mut texture_candidates = resources
        .iter()
        .filter_map(|resource| {
            if resource.lifetime != ResourceLifetime::Transient {
                return None;
            }
            let ResourceHandle::Texture(handle) = resource.handle else {
                return None;
            };
            let desc = graph.textures.get(handle.0 as usize).cloned()?;
            Some(TextureCandidate {
                handle,
                desc,
                first_use_pass_index: resource.first_use_pass_index,
                last_use_pass_index: resource.last_use_pass_index,
            })
        })
        .collect::<Vec<_>>();
    texture_candidates.sort_by(|a, b| {
        let a_area = a.desc.width() as u64 * a.desc.height() as u64;
        let b_area = b.desc.width() as u64 * b.desc.height() as u64;
        a.first_use_pass_index
            .cmp(&b.first_use_pass_index)
            .then_with(|| b_area.cmp(&a_area))
            .then_with(|| b.desc.width().cmp(&a.desc.width()))
            .then_with(|| b.desc.height().cmp(&a.desc.height()))
            .then_with(|| a.last_use_pass_index.cmp(&b.last_use_pass_index))
            .then_with(|| a.handle.0.cmp(&b.handle.0))
    });

    for candidate in texture_candidates {
        let desc = candidate.desc.clone();
        let chosen = texture_slots
            .iter_mut()
            .enumerate()
            .filter(|(_, slot)| {
                slot.last_use_pass_index < candidate.first_use_pass_index
                    && slot.desc.format() == desc.format()
                    && slot.desc.dimension() == desc.dimension()
                    && slot.desc.usage() == desc.usage()
                    && slot.desc.sample_count() == desc.sample_count()
                    && slot.desc.label() == desc.label()
                    && slot.desc.width() >= desc.width()
                    && slot.desc.height() >= desc.height()
            })
            .min_by(|(_, a), (_, b)| {
                let a_area = a.desc.width() as u64 * a.desc.height() as u64;
                let b_area = b.desc.width() as u64 * b.desc.height() as u64;
                a_area
                    .cmp(&b_area)
                    .then_with(|| a.desc.width().cmp(&b.desc.width()))
                    .then_with(|| a.desc.height().cmp(&b.desc.height()))
            })
            .map(|(index, _)| index)
            .map(|index| {
                let slot = &mut texture_slots[index];
                slot.last_use_pass_index = candidate.last_use_pass_index;
                slot.id
            })
            .unwrap_or_else(|| {
                let id = AllocationId(next_id);
                next_id = next_id.saturating_add(1);
                texture_slots.push(TextureSlot {
                    id,
                    desc: desc.clone(),
                    last_use_pass_index: candidate.last_use_pass_index,
                });
                texture_allocation_index.insert(id, texture_allocations.len());
                texture_allocations.push(TextureAllocationPlanEntry {
                    allocation_id: id,
                    owner: AllocationOwner::AllocatorManaged,
                    resources: Vec::new(),
                });
                id
            });
        texture_allocation_ids.insert(candidate.handle, chosen);
        if let Some(&idx) = texture_allocation_index.get(&chosen) {
            texture_allocations[idx].resources.push(candidate.handle);
        }
    }

    for resource in resources {
        match resource.handle {
            ResourceHandle::Texture(_) => {}
            ResourceHandle::Buffer(handle) => {
                if resource.lifetime != ResourceLifetime::Transient {
                    continue;
                }
                let Some(_desc) = graph.buffers.get(handle.0 as usize).copied() else {
                    continue;
                };
                let chosen = {
                    let id = AllocationId(next_id);
                    next_id = next_id.saturating_add(1);
                    buffer_allocations.push(BufferAllocationPlanEntry {
                        allocation_id: id,
                        owner: AllocationOwner::AllocatorManaged,
                        resources: Vec::new(),
                    });
                    id
                };
                buffer_allocation_ids.insert(handle, chosen);
                if let Some(entry) = buffer_allocations
                    .iter_mut()
                    .find(|entry| entry.allocation_id == chosen)
                {
                    entry.resources.push(handle);
                }
            }
        }
    }

    let uses_surface = ordered_passes.iter().any(|&pass_index| {
        let PassDetails::Graphics(graphics) = &graph.passes[pass_index].descriptor.details else {
            return false;
        };
        graphics
            .color_attachments
            .iter()
            .any(|attachment| attachment.target == AttachmentTarget::Surface)
            || graphics
                .depth_stencil_attachment
                .is_some_and(|attachment| attachment.target == AttachmentTarget::Surface)
    });

    let mut external_resources = Vec::new();
    if uses_surface {
        external_resources.push(ExternalAllocationPlanEntry {
            resource: ExternalResource::Surface,
            owner: AllocationOwner::ExternalOwned,
        });
    }

    (
        AllocationPlan {
            texture_allocations,
            buffer_allocations,
            external_resources,
        },
        texture_allocation_ids,
        buffer_allocation_ids,
    )
}

fn count_graph_edges(graph_edges: &[FxHashSet<usize>], live_passes: &FxHashSet<usize>) -> usize {
    live_passes
        .iter()
        .map(|&index| graph_edges[index].len())
        .sum()
}

fn count_live_passes_by_kind(
    graph: &FrameGraph,
    live_passes: &FxHashSet<usize>,
    kind: PassKind,
) -> usize {
    live_passes
        .iter()
        .filter(|&&index| graph.passes[index].descriptor.kind == kind)
        .count()
}

fn count_graphics_steps(steps: &[CompiledExecuteStep]) -> usize {
    steps
        .iter()
        .filter(|step| {
            matches!(
                step,
                CompiledExecuteStep::GraphicsPass { .. }
                    | CompiledExecuteStep::GraphicsPassGroup(_)
            )
        })
        .count()
}

fn count_graphics_groups(steps: &[CompiledExecuteStep]) -> usize {
    steps
        .iter()
        .filter(|step| matches!(step, CompiledExecuteStep::GraphicsPassGroup(_)))
        .count()
}

fn max_graphics_group_size(steps: &[CompiledExecuteStep]) -> usize {
    steps
        .iter()
        .filter_map(|step| match step {
            CompiledExecuteStep::GraphicsPassGroup(group) => Some(group.pass_indices.len()),
            _ => None,
        })
        .max()
        .unwrap_or(0)
}

fn summarize_pass_name_counts(
    graph: &FrameGraph,
    live_passes: &FxHashSet<usize>,
    limit: usize,
) -> Vec<CompileCountStat> {
    let mut counts = FxHashMap::<String, usize>::default();
    for &index in live_passes {
        *counts
            .entry(graph.passes[index].descriptor.name.to_string())
            .or_default() += 1;
    }
    sort_count_stats(counts, limit)
}

fn summarize_versioned_resource_counts(
    graph: &FrameGraph,
    live_passes: &FxHashSet<usize>,
    limit: usize,
) -> Vec<CompileCountStat> {
    let mut counts = FxHashMap::<ResourceHandle, usize>::default();
    for &index in live_passes {
        for usage in &graph.passes[index].usages {
            if usage.read_version.is_some() {
                *counts.entry(usage.resource).or_default() += 1;
            }
            if usage.write_version.is_some() {
                *counts.entry(usage.resource).or_default() += 1;
            }
        }
    }

    let mut items = counts
        .into_iter()
        .map(|(resource, count)| CompileCountStat {
            label: graph.describe_resource_handle(resource),
            count,
        })
        .collect::<Vec<_>>();
    items.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.label.cmp(&right.label))
    });
    items.truncate(limit);
    items
}

fn summarize_degree_counts(
    graph: &FrameGraph,
    live_passes: &FxHashSet<usize>,
    indegree: &[usize],
    limit: usize,
) -> Vec<CompileDegreeStat> {
    let mut items = live_passes
        .iter()
        .map(|&index| CompileDegreeStat {
            pass_index: index,
            pass_name: graph.passes[index].descriptor.name.to_string(),
            degree: indegree[index],
        })
        .collect::<Vec<_>>();
    sort_degree_stats(&mut items, limit);
    items
}

fn summarize_outdegree_counts(
    graph: &FrameGraph,
    live_passes: &FxHashSet<usize>,
    graph_edges: &[FxHashSet<usize>],
    limit: usize,
) -> Vec<CompileDegreeStat> {
    let mut items = live_passes
        .iter()
        .map(|&index| CompileDegreeStat {
            pass_index: index,
            pass_name: graph.passes[index].descriptor.name.to_string(),
            degree: graph_edges[index].len(),
        })
        .collect::<Vec<_>>();
    sort_degree_stats(&mut items, limit);
    items
}

fn sort_count_stats(counts: FxHashMap<String, usize>, limit: usize) -> Vec<CompileCountStat> {
    let mut items = counts
        .into_iter()
        .map(|(label, count)| CompileCountStat { label, count })
        .collect::<Vec<_>>();
    items.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.label.cmp(&right.label))
    });
    items.truncate(limit);
    items
}

fn sort_degree_stats(items: &mut Vec<CompileDegreeStat>, limit: usize) {
    items.sort_by(|left, right| {
        right
            .degree
            .cmp(&left.degree)
            .then_with(|| left.pass_name.cmp(&right.pass_name))
            .then_with(|| left.pass_index.cmp(&right.pass_index))
    });
    items.truncate(limit);
}

pub trait FrameResourceContext {
    fn viewport(&mut self) -> &mut Viewport;
    fn textures(&self) -> &[TextureDesc];
    fn buffers(&self) -> &[BufferDesc];
    fn texture_allocation_id(&self, handle: TextureHandle) -> Option<AllocationId>;
    fn texture_stable_key(&self, handle: TextureHandle) -> Option<u64>;
    fn buffer_allocation_id(&self, handle: BufferHandle) -> Option<AllocationId>;

    fn buffer_desc(&self, handle: BufferHandle) -> Option<BufferDesc> {
        self.buffers().get(handle.0 as usize).copied()
    }

    fn acquire_buffer(&mut self, handle: BufferHandle) -> Option<wgpu::Buffer> {
        let desc = self.buffer_desc(handle)?;
        let allocation_id = self.buffer_allocation_id(handle)?;
        self.viewport().acquire_frame_buffer(allocation_id, desc)
    }
}

pub struct PrepareContext<'a, 'b> {
    pub(crate) viewport: &'a mut Viewport,
    pub(crate) textures: &'b [TextureDesc],
    pub(crate) buffers: &'b [BufferDesc],
    texture_allocation_ids: &'b FxHashMap<TextureHandle, AllocationId>,
    texture_stable_keys: &'b FxHashMap<TextureHandle, u64>,
    buffer_allocation_ids: &'b FxHashMap<BufferHandle, AllocationId>,
}

impl<'a, 'b> PrepareContext<'a, 'b> {
    pub(crate) fn new(
        viewport: &'a mut Viewport,
        textures: &'b [TextureDesc],
        buffers: &'b [BufferDesc],
        texture_allocation_ids: &'b FxHashMap<TextureHandle, AllocationId>,
        texture_stable_keys: &'b FxHashMap<TextureHandle, u64>,
        buffer_allocation_ids: &'b FxHashMap<BufferHandle, AllocationId>,
    ) -> Self {
        Self {
            viewport,
            textures,
            buffers,
            texture_allocation_ids,
            texture_stable_keys,
            buffer_allocation_ids,
        }
    }

    pub fn upload_buffer(&mut self, handle: BufferHandle, offset: u64, data: &[u8]) -> bool {
        let Some(desc) = self.buffer_desc(handle) else {
            return false;
        };
        let Some(allocation_id) = self.buffer_allocation_id(handle) else {
            return false;
        };
        self.viewport
            .upload_frame_buffer(allocation_id, desc, offset, data)
    }
}

impl FrameResourceContext for PrepareContext<'_, '_> {
    fn viewport(&mut self) -> &mut Viewport {
        self.viewport
    }

    fn textures(&self) -> &[TextureDesc] {
        self.textures
    }

    fn buffers(&self) -> &[BufferDesc] {
        self.buffers
    }

    fn texture_allocation_id(&self, handle: TextureHandle) -> Option<AllocationId> {
        self.texture_allocation_ids.get(&handle).copied()
    }

    fn texture_stable_key(&self, handle: TextureHandle) -> Option<u64> {
        self.texture_stable_keys.get(&handle).copied()
    }

    fn buffer_allocation_id(&self, handle: BufferHandle) -> Option<AllocationId> {
        self.buffer_allocation_ids.get(&handle).copied()
    }
}

pub struct RecordContext<'a, 'b> {
    pub(crate) viewport: &'a mut Viewport,
    pub(crate) textures: &'b [TextureDesc],
    pub(crate) buffers: &'b [BufferDesc],
    texture_allocation_ids: &'b FxHashMap<TextureHandle, AllocationId>,
    texture_stable_keys: &'b FxHashMap<TextureHandle, u64>,
    buffer_allocation_ids: &'b FxHashMap<BufferHandle, AllocationId>,
    detail_timings: FxHashMap<String, f64>,
    detail_counts: FxHashMap<String, usize>,
    detail_order: Vec<String>,
}

impl<'a, 'b> RecordContext<'a, 'b> {
    pub(crate) fn new(
        viewport: &'a mut Viewport,
        textures: &'b [TextureDesc],
        buffers: &'b [BufferDesc],
        texture_allocation_ids: &'b FxHashMap<TextureHandle, AllocationId>,
        texture_stable_keys: &'b FxHashMap<TextureHandle, u64>,
        buffer_allocation_ids: &'b FxHashMap<BufferHandle, AllocationId>,
    ) -> Self {
        Self {
            viewport,
            textures,
            buffers,
            texture_allocation_ids,
            texture_stable_keys,
            buffer_allocation_ids,
            detail_timings: FxHashMap::default(),
            detail_counts: FxHashMap::default(),
            detail_order: Vec::new(),
        }
    }

    fn take_detail_timings(&mut self) -> Vec<(String, f64, usize)> {
        let mut ordered = Vec::with_capacity(self.detail_order.len());
        for name in self.detail_order.drain(..) {
            let Some(elapsed_ms) = self.detail_timings.remove(&name) else {
                continue;
            };
            let count = self.detail_counts.remove(&name).unwrap_or(0);
            ordered.push((name, elapsed_ms, count));
        }
        ordered
    }
}

impl FrameResourceContext for RecordContext<'_, '_> {
    fn viewport(&mut self) -> &mut Viewport {
        self.viewport
    }

    fn textures(&self) -> &[TextureDesc] {
        self.textures
    }

    fn buffers(&self) -> &[BufferDesc] {
        self.buffers
    }

    fn texture_allocation_id(&self, handle: TextureHandle) -> Option<AllocationId> {
        self.texture_allocation_ids.get(&handle).copied()
    }

    fn texture_stable_key(&self, handle: TextureHandle) -> Option<u64> {
        self.texture_stable_keys.get(&handle).copied()
    }

    fn buffer_allocation_id(&self, handle: BufferHandle) -> Option<AllocationId> {
        self.buffer_allocation_ids.get(&handle).copied()
    }
}

pub struct GraphicsRecordContext<'ctx, 'res> {
    pub(crate) viewport: &'ctx mut Viewport,
    pub(crate) textures: &'res [TextureDesc],
    pub(crate) buffers: &'res [BufferDesc],
    texture_allocation_ids: &'res FxHashMap<TextureHandle, AllocationId>,
    texture_stable_keys: &'res FxHashMap<TextureHandle, u64>,
    buffer_allocation_ids: &'res FxHashMap<BufferHandle, AllocationId>,
    detail_timings: &'ctx mut FxHashMap<String, f64>,
    detail_counts: &'ctx mut FxHashMap<String, usize>,
    detail_order: &'ctx mut Vec<String>,
}

impl<'ctx, 'res> GraphicsRecordContext<'ctx, 'res> {
    pub(crate) fn new(record: &'ctx mut RecordContext<'_, 'res>) -> Self {
        let RecordContext {
            viewport,
            textures,
            buffers,
            texture_allocation_ids,
            texture_stable_keys,
            buffer_allocation_ids,
            detail_timings,
            detail_counts,
            detail_order,
        } = record;
        Self {
            viewport,
            textures,
            buffers,
            texture_allocation_ids,
            texture_stable_keys,
            buffer_allocation_ids,
            detail_timings,
            detail_counts,
            detail_order,
        }
    }

    pub fn viewport(&mut self) -> &mut Viewport {
        self.viewport
    }

    pub fn record_detail_timing(&mut self, name: &'static str, elapsed_ms: f64) {
        if !self.viewport.debug_options().trace_render_time || elapsed_ms <= 0.0 {
            return;
        }
        let name = name.to_string();
        if !self.detail_timings.contains_key(&name) {
            self.detail_order.push(name.clone());
        }
        *self.detail_timings.entry(name.clone()).or_insert(0.0) += elapsed_ms;
        *self.detail_counts.entry(name).or_insert(0) += 1;
    }

    pub fn record_detail_count(&mut self, name: &'static str) {
        if !self.viewport.debug_options().trace_render_time {
            return;
        }
        let name = name.to_string();
        if !self.detail_timings.contains_key(&name) {
            self.detail_order.push(name.clone());
        }
        self.detail_timings.entry(name.clone()).or_insert(0.0);
        *self.detail_counts.entry(name).or_insert(0) += 1;
    }
}

impl FrameResourceContext for GraphicsRecordContext<'_, '_> {
    fn viewport(&mut self) -> &mut Viewport {
        self.viewport
    }

    fn textures(&self) -> &[TextureDesc] {
        self.textures
    }

    fn buffers(&self) -> &[BufferDesc] {
        self.buffers
    }

    fn texture_allocation_id(&self, handle: TextureHandle) -> Option<AllocationId> {
        self.texture_allocation_ids.get(&handle).copied()
    }

    fn texture_stable_key(&self, handle: TextureHandle) -> Option<u64> {
        self.texture_stable_keys.get(&handle).copied()
    }

    fn buffer_allocation_id(&self, handle: BufferHandle) -> Option<AllocationId> {
        self.buffer_allocation_ids.get(&handle).copied()
    }
}

pub struct ComputeRecordContext<'ctx, 'res> {
    pub(crate) viewport: &'ctx mut Viewport,
    pub(crate) textures: &'res [TextureDesc],
    pub(crate) buffers: &'res [BufferDesc],
    texture_allocation_ids: &'res FxHashMap<TextureHandle, AllocationId>,
    texture_stable_keys: &'res FxHashMap<TextureHandle, u64>,
    buffer_allocation_ids: &'res FxHashMap<BufferHandle, AllocationId>,
    detail_timings: &'ctx mut FxHashMap<String, f64>,
    detail_counts: &'ctx mut FxHashMap<String, usize>,
    detail_order: &'ctx mut Vec<String>,
}

impl<'ctx, 'res> ComputeRecordContext<'ctx, 'res> {
    pub(crate) fn new(record: &'ctx mut RecordContext<'_, 'res>) -> Self {
        let RecordContext {
            viewport,
            textures,
            buffers,
            texture_allocation_ids,
            texture_stable_keys,
            buffer_allocation_ids,
            detail_timings,
            detail_counts,
            detail_order,
        } = record;
        Self {
            viewport,
            textures,
            buffers,
            texture_allocation_ids,
            texture_stable_keys,
            buffer_allocation_ids,
            detail_timings,
            detail_counts,
            detail_order,
        }
    }

    pub fn viewport(&mut self) -> &mut Viewport {
        self.viewport
    }

    pub fn record_detail_timing(&mut self, name: &'static str, elapsed_ms: f64) {
        if !self.viewport.debug_options().trace_render_time || elapsed_ms <= 0.0 {
            return;
        }
        let name = name.to_string();
        if !self.detail_timings.contains_key(&name) {
            self.detail_order.push(name.clone());
        }
        *self.detail_timings.entry(name.clone()).or_insert(0.0) += elapsed_ms;
        *self.detail_counts.entry(name).or_insert(0) += 1;
    }

    pub fn record_detail_count(&mut self, name: &'static str) {
        if !self.viewport.debug_options().trace_render_time {
            return;
        }
        let name = name.to_string();
        if !self.detail_timings.contains_key(&name) {
            self.detail_order.push(name.clone());
        }
        self.detail_timings.entry(name.clone()).or_insert(0.0);
        *self.detail_counts.entry(name).or_insert(0) += 1;
    }
}

impl FrameResourceContext for ComputeRecordContext<'_, '_> {
    fn viewport(&mut self) -> &mut Viewport {
        self.viewport
    }

    fn textures(&self) -> &[TextureDesc] {
        self.textures
    }

    fn buffers(&self) -> &[BufferDesc] {
        self.buffers
    }

    fn texture_allocation_id(&self, handle: TextureHandle) -> Option<AllocationId> {
        self.texture_allocation_ids.get(&handle).copied()
    }

    fn texture_stable_key(&self, handle: TextureHandle) -> Option<u64> {
        self.texture_stable_keys.get(&handle).copied()
    }

    fn buffer_allocation_id(&self, handle: BufferHandle) -> Option<AllocationId> {
        self.buffer_allocation_ids.get(&handle).copied()
    }
}

pub struct TransferRecordContext<'ctx, 'res> {
    pub(crate) viewport: &'ctx mut Viewport,
    pub(crate) textures: &'res [TextureDesc],
    pub(crate) buffers: &'res [BufferDesc],
    texture_allocation_ids: &'res FxHashMap<TextureHandle, AllocationId>,
    texture_stable_keys: &'res FxHashMap<TextureHandle, u64>,
    buffer_allocation_ids: &'res FxHashMap<BufferHandle, AllocationId>,
    detail_timings: &'ctx mut FxHashMap<String, f64>,
    detail_counts: &'ctx mut FxHashMap<String, usize>,
    detail_order: &'ctx mut Vec<String>,
}

impl<'ctx, 'res> TransferRecordContext<'ctx, 'res> {
    pub(crate) fn new(record: &'ctx mut RecordContext<'_, 'res>) -> Self {
        let RecordContext {
            viewport,
            textures,
            buffers,
            texture_allocation_ids,
            texture_stable_keys,
            buffer_allocation_ids,
            detail_timings,
            detail_counts,
            detail_order,
        } = record;
        Self {
            viewport,
            textures,
            buffers,
            texture_allocation_ids,
            texture_stable_keys,
            buffer_allocation_ids,
            detail_timings,
            detail_counts,
            detail_order,
        }
    }

    pub fn viewport(&mut self) -> &mut Viewport {
        self.viewport
    }

    pub fn record_detail_timing(&mut self, name: &'static str, elapsed_ms: f64) {
        if !self.viewport.debug_options().trace_render_time || elapsed_ms <= 0.0 {
            return;
        }
        let name = name.to_string();
        if !self.detail_timings.contains_key(&name) {
            self.detail_order.push(name.clone());
        }
        *self.detail_timings.entry(name.clone()).or_insert(0.0) += elapsed_ms;
        *self.detail_counts.entry(name).or_insert(0) += 1;
    }

    pub fn record_detail_count(&mut self, name: &'static str) {
        if !self.viewport.debug_options().trace_render_time {
            return;
        }
        let name = name.to_string();
        if !self.detail_timings.contains_key(&name) {
            self.detail_order.push(name.clone());
        }
        self.detail_timings.entry(name.clone()).or_insert(0.0);
        *self.detail_counts.entry(name).or_insert(0) += 1;
    }
}

impl FrameResourceContext for TransferRecordContext<'_, '_> {
    fn viewport(&mut self) -> &mut Viewport {
        self.viewport
    }

    fn textures(&self) -> &[TextureDesc] {
        self.textures
    }

    fn buffers(&self) -> &[BufferDesc] {
        self.buffers
    }

    fn texture_allocation_id(&self, handle: TextureHandle) -> Option<AllocationId> {
        self.texture_allocation_ids.get(&handle).copied()
    }

    fn texture_stable_key(&self, handle: TextureHandle) -> Option<u64> {
        self.texture_stable_keys.get(&handle).copied()
    }

    fn buffer_allocation_id(&self, handle: BufferHandle) -> Option<AllocationId> {
        self.buffer_allocation_ids.get(&handle).copied()
    }
}

#[derive(Debug)]
pub enum FrameGraphError {
    MissingInput(&'static str),
    MissingOutput(&'static str),
    MultipleWriters,
    Validation(String),
    CyclicDependency,
    MissingRootPass,
    NotCompiled,
}

/// Counters for a cache's lifetime. Incremented with `Relaxed` atomics so the
/// overhead is ~2ns per access; kept in every build so stats work in release.
pub struct CacheStats {
    pub name: &'static str,
    pub hits: AtomicU64,
    pub misses: AtomicU64,
    pub evictions: AtomicU64,
}

impl CacheStats {
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            evictions: AtomicU64::new(0),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CacheStatSnapshot {
    pub name: &'static str,
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
}

impl CacheStatSnapshot {
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }
}

fn cache_stats_registry() -> &'static Mutex<Vec<&'static CacheStats>> {
    static REGISTRY: OnceLock<Mutex<Vec<&'static CacheStats>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(Vec::new()))
}

pub fn register_cache_stats(stats: &'static CacheStats) {
    cache_stats_registry().lock().unwrap().push(stats);
}

pub fn dump_cache_stats() -> Vec<CacheStatSnapshot> {
    cache_stats_registry()
        .lock()
        .unwrap()
        .iter()
        .map(|s| CacheStatSnapshot {
            name: s.name,
            hits: s.hits.load(Ordering::Relaxed),
            misses: s.misses.load(Ordering::Relaxed),
            evictions: s.evictions.load(Ordering::Relaxed),
        })
        .collect()
}

pub struct ResourceCache<T> {
    store: FxHashMap<u64, T>,
    stats: Option<&'static CacheStats>,
}

impl<T> ResourceCache<T> {
    pub fn new() -> Self {
        Self {
            store: FxHashMap::default(),
            stats: None,
        }
    }

    pub fn with_stats(stats: &'static CacheStats) -> Self {
        Self {
            store: FxHashMap::default(),
            stats: Some(stats),
        }
    }

    pub fn clear(&mut self) {
        if let Some(stats) = self.stats {
            stats
                .evictions
                .fetch_add(self.store.len() as u64, Ordering::Relaxed);
        }
        self.store.clear();
    }

    pub fn get_or_insert_with<F: FnOnce() -> T>(&mut self, key: u64, create: F) -> &mut T {
        if let Some(stats) = self.stats {
            if self.store.contains_key(&key) {
                stats.hits.fetch_add(1, Ordering::Relaxed);
            } else {
                stats.misses.fetch_add(1, Ordering::Relaxed);
            }
        }
        self.store.entry(key).or_insert_with(create)
    }

    pub fn len(&self) -> usize {
        self.store.len()
    }
}

impl<T> Default for ResourceCache<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Declares a process-wide `ResourceCache<T>` accessor fn.
///
/// Native: `static OnceLock<Mutex<...>>`, shared across threads.
/// wasm32: `thread_local! RefCell<Option<...>>`, because wgpu resource types
/// wrap `Rc<RefCell<..>>` on the Web backend and are therefore `!Send`/`!Sync`.
#[macro_export]
macro_rules! static_resource_cache {
    (
        fn $fn_name:ident -> ResourceCache<$cache_ty:ty> = stats($stats_name:literal)
    ) => {
        fn $fn_name<__R>(
            f: impl FnOnce(&mut $crate::view::frame_graph::ResourceCache<$cache_ty>) -> __R,
        ) -> __R {
            static STATS: $crate::view::frame_graph::CacheStats =
                $crate::view::frame_graph::CacheStats::new($stats_name);

            #[cfg(not(target_arch = "wasm32"))]
            {
                static CACHE: ::std::sync::OnceLock<
                    ::std::sync::Mutex<$crate::view::frame_graph::ResourceCache<$cache_ty>>,
                > = ::std::sync::OnceLock::new();
                let cache = CACHE.get_or_init(|| {
                    $crate::view::frame_graph::register_cache_stats(&STATS);
                    ::std::sync::Mutex::new($crate::view::frame_graph::ResourceCache::with_stats(
                        &STATS,
                    ))
                });
                f(&mut cache.lock().unwrap())
            }
            #[cfg(target_arch = "wasm32")]
            {
                ::std::thread_local! {
                    static CACHE: ::std::cell::RefCell<
                        ::std::option::Option<
                            $crate::view::frame_graph::ResourceCache<$cache_ty>,
                        >,
                    > = const { ::std::cell::RefCell::new(None) };
                }
                CACHE.with(|c| {
                    let mut borrowed = c.borrow_mut();
                    if borrowed.is_none() {
                        $crate::view::frame_graph::register_cache_stats(&STATS);
                        *borrowed =
                            Some($crate::view::frame_graph::ResourceCache::with_stats(&STATS));
                    }
                    f(borrowed.as_mut().unwrap())
                })
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::frame_graph::slot::{InSlot, OutSlot};
    use crate::view::frame_graph::texture_resource::{TextureDesc, TextureResource};
    use crate::view::frame_graph::{
        BufferReadUsage, BufferResource, ComputePassBuilder, GraphicsPassBuilder,
        TransferPassBuilder,
    };
    use crate::view::render_pass::draw_rect_pass::{
        DrawRectInput, DrawRectOutput, DrawRectPass, OpaqueRectPass, RectPassParams,
        RenderTargetIn, RenderTargetOut,
    };
    use crate::view::render_pass::present_surface_pass::{
        PresentSurfaceInput, PresentSurfaceOutput, PresentSurfaceParams, PresentSurfacePass,
    };
    use crate::view::render_pass::{
        ComputeCtx, ComputePass, GraphicsCtx, GraphicsPass, TransferCtx, TransferPass,
    };

    #[derive(Default)]
    struct WritePass {
        output: OutSlot<TextureResource, ()>,
    }

    impl GraphicsPass for WritePass {
        fn setup(&mut self, builder: &mut GraphicsPassBuilder<'_, '_>) {
            let target = builder
                .texture_target(&self.output)
                .expect("test output should have texture target");
            let _ = target;
            builder.write_color(
                &self.output,
                GraphicsColorAttachmentOps::clear([0.0, 0.0, 0.0, 0.0]),
            );
        }

        fn execute(&mut self, _ctx: &mut GraphicsCtx<'_, '_, '_, '_>) {}
    }

    #[derive(Default)]
    struct ReadPass {
        input: InSlot<TextureResource, ()>,
    }

    impl GraphicsPass for ReadPass {
        fn setup(&mut self, builder: &mut GraphicsPassBuilder<'_, '_>) {
            if let Some(handle) = self.input.handle() {
                builder.read_texture(&mut self.input, &OutSlot::with_handle(handle));
            }
        }

        fn execute(&mut self, _ctx: &mut GraphicsCtx<'_, '_, '_, '_>) {}
    }

    #[derive(Default)]
    struct ModifyPass {
        target: OutSlot<TextureResource, ()>,
    }

    impl GraphicsPass for ModifyPass {
        fn setup(&mut self, builder: &mut GraphicsPassBuilder<'_, '_>) {
            let target = builder
                .texture_target(&self.target)
                .expect("test output should have texture target");
            let _ = target;
            builder.write_color(&self.target, GraphicsColorAttachmentOps::load());
        }

        fn execute(&mut self, _ctx: &mut GraphicsCtx<'_, '_, '_, '_>) {}
    }

    #[derive(Default)]
    struct SurfacePass;

    #[derive(Default)]
    struct MergeableSurfacePass;

    struct MergeablePrepPass {
        output: OutSlot<TextureResource, ()>,
    }

    struct MergeableFinalReadPass {
        input: InSlot<TextureResource, ()>,
    }

    impl GraphicsPass for SurfacePass {
        fn setup(&mut self, builder: &mut GraphicsPassBuilder<'_, '_>) {
            builder.write_surface_color(GraphicsColorAttachmentOps::clear([0.0, 0.0, 0.0, 0.0]));
        }

        fn execute(&mut self, _ctx: &mut GraphicsCtx<'_, '_, '_, '_>) {}
    }

    impl GraphicsPass for MergeableSurfacePass {
        fn setup(&mut self, builder: &mut GraphicsPassBuilder<'_, '_>) {
            builder.set_graphics_merge_policy(GraphicsPassMergePolicy::Mergeable);
            builder.write_surface_color(GraphicsColorAttachmentOps::clear([0.0, 0.0, 0.0, 0.0]));
        }

        fn execute(&mut self, _ctx: &mut GraphicsCtx<'_, '_, '_, '_>) {}
    }

    impl GraphicsPass for MergeablePrepPass {
        fn setup(&mut self, builder: &mut GraphicsPassBuilder<'_, '_>) {
            builder.set_graphics_merge_policy(GraphicsPassMergePolicy::Mergeable);
            let target = builder
                .texture_target(&self.output)
                .expect("prep output should have texture target");
            let _ = target;
            builder.write_color(
                &self.output,
                GraphicsColorAttachmentOps::clear([0.0, 0.0, 0.0, 0.0]),
            );
        }

        fn execute(&mut self, _ctx: &mut GraphicsCtx<'_, '_, '_, '_>) {}
    }

    impl GraphicsPass for MergeableFinalReadPass {
        fn setup(&mut self, builder: &mut GraphicsPassBuilder<'_, '_>) {
            builder.set_graphics_merge_policy(GraphicsPassMergePolicy::Mergeable);
            if let Some(handle) = self.input.handle() {
                builder.read_texture(&mut self.input, &OutSlot::with_handle(handle));
            }
            builder.write_surface_color(GraphicsColorAttachmentOps::clear([0.0, 0.0, 0.0, 0.0]));
        }

        fn execute(&mut self, _ctx: &mut GraphicsCtx<'_, '_, '_, '_>) {}
    }

    #[derive(Default)]
    struct PersistentInternalPass {
        output: OutSlot<TextureResource, ()>,
    }

    #[derive(Default)]
    struct BufferWritePass {
        output: OutSlot<BufferResource, ()>,
    }

    struct ExistingBufferWritePass {
        output: OutSlot<BufferResource, ()>,
    }

    struct BufferReadPass {
        input: OutSlot<BufferResource, ()>,
    }

    #[derive(Default)]
    struct InlineLoadPass {
        target: OutSlot<TextureResource, ()>,
    }

    #[derive(Default)]
    struct DepthStencilWritePass {
        target: OutSlot<TextureResource, ()>,
    }

    #[derive(Default)]
    struct DepthStencilReadPass {
        target: OutSlot<TextureResource, ()>,
    }

    #[derive(Default)]
    struct ComputeStubPass;

    #[derive(Default)]
    struct TransferStubPass;

    impl GraphicsPass for BufferWritePass {
        fn setup(&mut self, builder: &mut GraphicsPassBuilder<'_, '_>) {
            self.output = builder.create_buffer(BufferDesc {
                size: 16,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::UNIFORM,
                label: Some("Test Buffer"),
            });
            builder.read_buffer(&self.output, BufferReadUsage::Uniform);
        }

        fn execute(&mut self, _ctx: &mut GraphicsCtx<'_, '_, '_, '_>) {}
    }

    impl GraphicsPass for ExistingBufferWritePass {
        fn setup(&mut self, builder: &mut GraphicsPassBuilder<'_, '_>) {
            builder.write_buffer(&self.output);
        }

        fn execute(&mut self, _ctx: &mut GraphicsCtx<'_, '_, '_, '_>) {}
    }

    impl GraphicsPass for BufferReadPass {
        fn setup(&mut self, builder: &mut GraphicsPassBuilder<'_, '_>) {
            builder.read_buffer(&self.input, BufferReadUsage::Uniform);
        }

        fn execute(&mut self, _ctx: &mut GraphicsCtx<'_, '_, '_, '_>) {}
    }

    impl GraphicsPass for InlineLoadPass {
        fn setup(&mut self, builder: &mut GraphicsPassBuilder<'_, '_>) {
            builder.set_graphics_merge_policy(GraphicsPassMergePolicy::Mergeable);
            let target = builder
                .texture_target(&self.target)
                .expect("inline test output should have texture target");
            let _ = target;
            builder.write_color(&self.target, GraphicsColorAttachmentOps::load());
        }

        fn execute(&mut self, _ctx: &mut GraphicsCtx<'_, '_, '_, '_>) {}
    }

    impl GraphicsPass for DepthStencilWritePass {
        fn setup(&mut self, builder: &mut GraphicsPassBuilder<'_, '_>) {
            let target = builder
                .texture_target(&self.target)
                .expect("depth/stencil target should exist");
            builder.write_depth(target, AttachmentLoadOp::Clear, Some(1.0));
            builder.write_stencil(target, AttachmentLoadOp::Clear, Some(0));
        }

        fn execute(&mut self, _ctx: &mut GraphicsCtx<'_, '_, '_, '_>) {}
    }

    impl GraphicsPass for DepthStencilReadPass {
        fn setup(&mut self, builder: &mut GraphicsPassBuilder<'_, '_>) {
            let target = builder
                .texture_target(&self.target)
                .expect("depth/stencil target should exist");
            builder.read_depth(target);
            builder.read_stencil(target);
        }

        fn execute(&mut self, _ctx: &mut GraphicsCtx<'_, '_, '_, '_>) {}
    }

    impl ComputePass for ComputeStubPass {
        fn setup(&mut self, _builder: &mut ComputePassBuilder<'_, '_>) {}

        fn execute(&mut self, _ctx: &mut ComputeCtx<'_, '_, '_, '_>) {}
    }

    impl TransferPass for TransferStubPass {
        fn setup(&mut self, _builder: &mut TransferPassBuilder<'_, '_>) {}

        fn execute(&mut self, _ctx: &mut TransferCtx<'_, '_, '_>) {}
    }

    impl GraphicsPass for PersistentInternalPass {
        fn setup(&mut self, builder: &mut GraphicsPassBuilder<'_, '_>) {
            self.output = builder.create_texture_internal(
                test_texture_desc(),
                ResourceLifetime::Persistent,
                Some(0xCAFE),
            );
            let target = builder
                .texture_target(&self.output)
                .expect("persistent output should have texture target");
            let _ = target;
            builder.write_color(
                &self.output,
                GraphicsColorAttachmentOps::clear([0.0, 0.0, 0.0, 0.0]),
            );
        }

        fn execute(&mut self, _ctx: &mut GraphicsCtx<'_, '_, '_, '_>) {}
    }

    fn test_texture_desc() -> TextureDesc {
        TextureDesc::new(
            1,
            1,
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureDimension::D2,
        )
    }

    fn test_depth_stencil_texture_desc() -> TextureDesc {
        TextureDesc::new(
            1,
            1,
            wgpu::TextureFormat::Depth24PlusStencil8,
            wgpu::TextureDimension::D2,
        )
    }

    fn test_buffer_desc() -> BufferDesc {
        BufferDesc {
            size: 16,
            usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
            label: Some("test-buffer"),
        }
    }

    fn make_present_pass(texture: &OutSlot<TextureResource, ()>) -> PresentSurfacePass {
        PresentSurfacePass::new(
            PresentSurfaceParams,
            PresentSurfaceInput {
                source: RenderTargetIn::with_handle(
                    texture.handle().expect("test texture should have handle"),
                ),
            },
            PresentSurfaceOutput,
        )
    }

    #[test]
    fn compile_orders_write_then_read_from_usage() {
        let mut graph = FrameGraph::new();
        let texture = graph.declare_texture::<()>(test_texture_desc());
        let writer = graph.add_graphics_pass(WritePass {
            output: texture.clone(),
        });
        let reader = graph.add_graphics_pass(ReadPass {
            input: InSlot::with_handle(
                texture
                    .handle()
                    .expect("declared texture should have handle"),
            ),
        });

        graph.compile().expect("compile should succeed");

        assert_eq!(graph.order, vec![writer.0, reader.0]);
    }

    #[test]
    fn compile_orders_modify_chain_from_usage() {
        let mut graph = FrameGraph::new();
        let texture = graph.declare_texture::<()>(test_texture_desc());
        let writer = graph.add_graphics_pass(WritePass {
            output: texture.clone(),
        });
        let modify_a = graph.add_graphics_pass(ModifyPass {
            target: texture.clone(),
        });
        let modify_b = graph.add_graphics_pass(ModifyPass { target: texture });

        graph.compile().expect("compile should succeed");

        assert_eq!(graph.order, vec![writer.0, modify_a.0, modify_b.0]);
    }

    #[test]
    fn compile_populates_version_metadata_for_write_then_read() {
        let mut graph = FrameGraph::new();
        let texture = graph.declare_texture::<()>(test_texture_desc());
        let writer = graph.add_graphics_pass(WritePass {
            output: texture.clone(),
        });
        let reader = graph.add_graphics_pass(ReadPass {
            input: InSlot::with_handle(
                texture
                    .handle()
                    .expect("declared texture should have handle"),
            ),
        });

        graph.compile().expect("compile should succeed");

        let writer_usage = graph.passes[writer.0]
            .usages
            .first()
            .copied()
            .expect("writer should have one usage");
        let reader_usage = graph.passes[reader.0]
            .usages
            .first()
            .copied()
            .expect("reader should have one usage");

        assert_eq!(writer_usage.read_version, None);
        assert_eq!(
            writer_usage.write_version,
            Some(ResourceVersionId::Texture(TextureVersionId(0)))
        );
        assert_eq!(
            reader_usage.read_version,
            Some(ResourceVersionId::Texture(TextureVersionId(0)))
        );
        assert_eq!(reader_usage.write_version, None);
    }

    #[test]
    fn compile_populates_version_metadata_for_load_modify() {
        let mut graph = FrameGraph::new();
        let texture = graph.declare_texture::<()>(test_texture_desc());
        let writer = graph.add_graphics_pass(WritePass {
            output: texture.clone(),
        });
        let modify = graph.add_graphics_pass(ModifyPass { target: texture });

        graph.compile().expect("compile should succeed");

        let writer_usage = graph.passes[writer.0]
            .usages
            .first()
            .copied()
            .expect("writer should have one usage");
        let modify_usage = graph.passes[modify.0]
            .usages
            .first()
            .copied()
            .expect("modify should have one usage");

        assert_eq!(
            writer_usage.write_version,
            Some(ResourceVersionId::Texture(TextureVersionId(0)))
        );
        assert_eq!(
            modify_usage.read_version,
            Some(ResourceVersionId::Texture(TextureVersionId(0)))
        );
        assert_eq!(
            modify_usage.write_version,
            Some(ResourceVersionId::Texture(TextureVersionId(1)))
        );
    }

    #[test]
    fn compiled_pass_exposes_versioned_inputs_outputs() {
        let mut graph = FrameGraph::new();
        let texture = graph.declare_texture::<()>(test_texture_desc());
        graph.add_graphics_pass(WritePass {
            output: texture.clone(),
        });
        graph.add_graphics_pass(ModifyPass {
            target: texture.clone(),
        });

        graph.compile().expect("compile should succeed");

        let compiled = graph.compiled_graph().expect("compiled graph should exist");
        assert_eq!(compiled.passes.len(), 2);
        assert_eq!(compiled.passes[0].input_versions.len(), 0);
        assert_eq!(compiled.passes[0].output_versions.len(), 1);
        assert_eq!(
            compiled.passes[0].output_versions[0].version,
            ResourceVersionId::Texture(TextureVersionId(0))
        );
        assert_eq!(compiled.passes[1].input_versions.len(), 1);
        assert_eq!(compiled.passes[1].output_versions.len(), 1);
        assert_eq!(
            compiled.passes[1].input_versions[0].version,
            ResourceVersionId::Texture(TextureVersionId(0))
        );
        assert_eq!(
            compiled.passes[1].output_versions[0].version,
            ResourceVersionId::Texture(TextureVersionId(1))
        );
    }

    #[test]
    fn compile_orders_buffer_write_then_read_from_usage_api() {
        let mut graph = FrameGraph::new();
        let buffer = graph.declare_buffer_internal::<()>(
            test_buffer_desc(),
            ResourceLifetime::Transient,
            None,
        );
        let buffer_handle = buffer.handle().expect("declared buffer should have handle");
        let writer = graph.add_graphics_pass(ExistingBufferWritePass {
            output: OutSlot::with_handle(buffer_handle),
        });
        let reader = graph.add_graphics_pass(BufferReadPass {
            input: OutSlot::with_handle(buffer_handle),
        });

        graph.compile().expect("compile should succeed");

        assert_eq!(graph.order, vec![writer.0, reader.0]);
    }

    #[test]
    fn compile_captures_graphics_pass_descriptor() {
        let mut graph = FrameGraph::new();
        let texture = graph.declare_texture::<()>(test_texture_desc());
        graph.add_graphics_pass(WritePass { output: texture });

        graph.compile().expect("compile should succeed");

        let descriptors = graph.pass_descriptors();
        assert_eq!(descriptors.len(), 1);
        assert_eq!(descriptors[0].kind, PassKind::Graphics);
        let PassDetails::Graphics(graphics) = &descriptors[0].details else {
            panic!("expected graphics pass details");
        };
        assert_eq!(graphics.color_attachments.len(), 1);
        assert_eq!(
            graphics.color_attachments[0].load_op,
            AttachmentLoadOp::Clear
        );
    }

    #[test]
    fn compiler_clears_first_transient_color_load_attachment() {
        let mut graph = FrameGraph::new();
        let texture = graph.declare_texture::<()>(test_texture_desc());
        let writer = graph.add_graphics_pass(ModifyPass {
            target: texture.clone(),
        });
        let present = graph.add_graphics_pass(make_present_pass(&texture));
        graph
            .add_pass_sink(present, ExternalSinkKind::SurfacePresent)
            .expect("sink registration should succeed");

        graph.compile().expect("compile should succeed");

        let compiled = graph.compiled_graph().expect("graph should be compiled");
        let writer_pass = compiled
            .passes
            .iter()
            .find(|pass| pass.original_index == writer.0)
            .expect("writer pass should be live");
        let PassDetails::Graphics(graphics) = &writer_pass.descriptor.details else {
            panic!("expected graphics pass details");
        };
        assert_eq!(graphics.color_attachments.len(), 1);
        assert_eq!(
            graphics.color_attachments[0].load_op,
            AttachmentLoadOp::Clear
        );
        assert_eq!(
            graphics.color_attachments[0].clear_color,
            Some([0.0, 0.0, 0.0, 0.0])
        );
    }

    #[test]
    fn compiler_keeps_first_persistent_color_load_attachment() {
        let mut graph = FrameGraph::new();
        let target = graph.declare_texture_internal::<()>(
            test_texture_desc(),
            ResourceLifetime::Persistent,
            Some(0xBEEF),
        );
        let writer = graph.add_graphics_pass(ModifyPass {
            target: target.clone(),
        });
        let present = graph.add_graphics_pass(make_present_pass(&target));
        graph
            .add_pass_sink(present, ExternalSinkKind::SurfacePresent)
            .expect("sink registration should succeed");

        graph.compile().expect("compile should succeed");

        let compiled = graph.compiled_graph().expect("graph should be compiled");
        let writer_pass = compiled
            .passes
            .iter()
            .find(|pass| pass.original_index == writer.0)
            .expect("writer pass should be live");
        let PassDetails::Graphics(graphics) = &writer_pass.descriptor.details else {
            panic!("expected graphics pass details");
        };
        assert_eq!(graphics.color_attachments.len(), 1);
        assert_eq!(
            graphics.color_attachments[0].load_op,
            AttachmentLoadOp::Load
        );
        assert_eq!(graphics.color_attachments[0].clear_color, None);
    }

    #[test]
    fn compile_culls_passes_outside_present_chain() {
        let mut graph = FrameGraph::new();
        let live_texture = graph.declare_texture::<()>(test_texture_desc());
        let dead_texture = graph.declare_texture::<()>(test_texture_desc());
        let live_writer = graph.add_graphics_pass(WritePass {
            output: live_texture.clone(),
        });
        let dead_writer = graph.add_graphics_pass(WritePass {
            output: dead_texture,
        });
        let present = graph.add_graphics_pass(make_present_pass(&live_texture));
        graph
            .add_pass_sink(present, ExternalSinkKind::SurfacePresent)
            .expect("pass sink should register");

        graph.compile().expect("compile should succeed");

        let compiled = graph.compiled_graph().expect("compiled graph should exist");
        assert_eq!(
            compiled.execution_plan.ordered_passes,
            vec![live_writer.0, present.0]
        );
        assert!(compiled.culled_passes.contains(&dead_writer.0));
    }

    #[test]
    fn compile_discovers_live_passes_from_resource_sink() {
        let mut graph = FrameGraph::new();
        let live_texture = graph.declare_texture::<()>(test_texture_desc());
        let dead_texture = graph.declare_texture::<()>(test_texture_desc());
        let live_writer = graph.add_graphics_pass(WritePass {
            output: live_texture.clone(),
        });
        let dead_writer = graph.add_graphics_pass(WritePass {
            output: dead_texture,
        });
        graph
            .add_texture_sink(&live_texture, ExternalSinkKind::DebugCapture)
            .expect("resource sink should register");

        graph.compile().expect("compile should succeed");

        let compiled = graph.compiled_graph().expect("compiled graph should exist");
        assert_eq!(compiled.execution_plan.ordered_passes, vec![live_writer.0]);
        assert!(compiled.culled_passes.contains(&dead_writer.0));
        assert_eq!(
            compiled.external_sinks,
            vec![ExternalSink {
                id: ExternalSinkId(0),
                kind: ExternalSinkKind::DebugCapture,
                target: ExternalSinkTarget::Resource(ResourceHandle::Texture(
                    live_texture
                        .handle()
                        .expect("live texture should have handle")
                )),
            }]
        );
    }

    #[test]
    fn compile_allows_multiple_writers_on_same_resource() {
        let mut graph = FrameGraph::new();
        let texture = graph.declare_texture::<()>(test_texture_desc());
        let writer_a = graph.add_graphics_pass(WritePass {
            output: texture.clone(),
        });
        let writer_b = graph.add_graphics_pass(WritePass { output: texture });

        graph.compile().expect("compile should succeed");

        assert_eq!(graph.order, vec![writer_a.0, writer_b.0]);
    }

    #[test]
    fn compile_culls_dead_overwritten_writer_from_version_flow() {
        let mut graph = FrameGraph::new();
        let texture = graph.declare_texture::<()>(test_texture_desc());
        let dead_writer = graph.add_graphics_pass(WritePass {
            output: texture.clone(),
        });
        let live_writer = graph.add_graphics_pass(WritePass {
            output: texture.clone(),
        });
        graph
            .add_texture_sink(&texture, ExternalSinkKind::ExportTexture)
            .expect("texture sink should be added");

        graph.compile().expect("compile should succeed");

        assert_eq!(graph.order, vec![live_writer.0]);
        let compiled = graph.compiled_graph().expect("compiled graph should exist");
        assert_eq!(compiled.culled_passes, vec![dead_writer.0]);
    }

    #[test]
    fn texture_sink_rejects_buffer_export_kind() {
        let mut graph = FrameGraph::new();
        let texture = graph.declare_texture::<()>(test_texture_desc());

        let err = graph
            .add_texture_sink(&texture, ExternalSinkKind::ExportBuffer)
            .expect_err("texture sink should reject buffer export kind");

        assert!(matches!(err, FrameGraphError::Validation(_)));
    }

    #[test]
    fn buffer_sink_rejects_texture_export_kind() {
        let mut graph = FrameGraph::new();
        let buffer = graph.declare_buffer_internal::<()>(
            test_buffer_desc(),
            ResourceLifetime::Transient,
            None,
        );

        let err = graph
            .add_buffer_sink(&buffer, ExternalSinkKind::ExportTexture)
            .expect_err("buffer sink should reject texture export kind");

        assert!(matches!(err, FrameGraphError::Validation(_)));
    }

    #[test]
    fn compile_rejects_read_without_producer() {
        let mut graph = FrameGraph::new();
        let texture = graph.declare_texture::<()>(test_texture_desc());
        graph.add_graphics_pass(ReadPass {
            input: InSlot::with_handle(
                texture
                    .handle()
                    .expect("declared texture should have handle"),
            ),
        });

        let err = graph.compile().expect_err("compile should fail");
        assert!(matches!(err, FrameGraphError::MissingInput(_)));
    }

    #[test]
    fn compile_tracks_resource_lifetime_by_compiled_pass_index() {
        let mut graph = FrameGraph::new();
        let texture = graph.declare_texture::<()>(test_texture_desc());
        graph.add_graphics_pass(WritePass {
            output: texture.clone(),
        });
        graph.add_graphics_pass(ModifyPass {
            target: texture.clone(),
        });
        graph.add_graphics_pass(ReadPass {
            input: InSlot::with_handle(
                texture
                    .handle()
                    .expect("declared texture should have handle"),
            ),
        });

        graph.compile().expect("compile should succeed");

        let compiled = graph.compiled_graph().expect("compiled graph should exist");
        let resource = compiled
            .resources
            .iter()
            .find(|resource| resource.handle == ResourceHandle::Texture(texture.handle().unwrap()))
            .expect("compiled resource should exist");
        assert_eq!(resource.first_use_pass_index, 0);
        assert_eq!(resource.last_use_pass_index, 2);
    }

    #[test]
    fn compile_tracks_resource_state_transitions_per_pass() {
        let mut graph = FrameGraph::new();
        let texture = graph.declare_texture::<()>(test_texture_desc());
        graph.add_graphics_pass(WritePass {
            output: texture.clone(),
        });
        graph.add_graphics_pass(ReadPass {
            input: InSlot::with_handle(
                texture
                    .handle()
                    .expect("declared texture should have handle"),
            ),
        });

        graph.compile().expect("compile should succeed");

        let compiled = graph.compiled_graph().expect("compiled graph should exist");
        let transitions = compiled
            .resource_transitions
            .iter()
            .filter(|transition| {
                transition.resource == ResourceHandle::Texture(texture.handle().unwrap())
            })
            .copied()
            .collect::<Vec<_>>();
        assert_eq!(
            transitions,
            vec![
                CompiledResourceTransition {
                    resource: ResourceHandle::Texture(texture.handle().unwrap()),
                    pass_index: 0,
                    execution_index: 0,
                    before: ResourceState::Texture(TextureResourceState::Undefined),
                    after: ResourceState::Texture(TextureResourceState::ColorAttachment),
                },
                CompiledResourceTransition {
                    resource: ResourceHandle::Texture(texture.handle().unwrap()),
                    pass_index: 1,
                    execution_index: 1,
                    before: ResourceState::Texture(TextureResourceState::ColorAttachment),
                    after: ResourceState::Texture(TextureResourceState::Sampled),
                },
            ]
        );
        assert_eq!(compiled.passes[0].resource_transitions.len(), 1);
        assert_eq!(
            compiled.passes[0].resource_transitions[0],
            CompiledPassResourceTransition {
                resource: ResourceHandle::Texture(texture.handle().unwrap()),
                before: ResourceState::Texture(TextureResourceState::Undefined),
                after: ResourceState::Texture(TextureResourceState::ColorAttachment),
            }
        );
    }

    #[test]
    fn compile_tracks_depth_stencil_state_in_timeline() {
        let mut graph = FrameGraph::new();
        let texture = graph.declare_texture::<()>(test_depth_stencil_texture_desc());
        graph.add_graphics_pass(DepthStencilWritePass {
            target: texture.clone(),
        });
        graph.add_graphics_pass(DepthStencilReadPass {
            target: texture.clone(),
        });

        graph.compile().expect("compile should succeed");

        let compiled = graph.compiled_graph().expect("compiled graph should exist");
        let timeline = compiled
            .resource_timelines
            .iter()
            .find(|timeline| {
                timeline.resource == ResourceHandle::Texture(texture.handle().unwrap())
            })
            .expect("resource timeline should exist");
        assert_eq!(
            timeline.transitions,
            vec![
                CompiledResourceTransition {
                    resource: ResourceHandle::Texture(texture.handle().unwrap()),
                    pass_index: 0,
                    execution_index: 0,
                    before: ResourceState::Texture(TextureResourceState::Undefined),
                    after: ResourceState::Texture(TextureResourceState::DepthStencilAttachment {
                        depth: Some(TextureAspectState::Write),
                        stencil: Some(TextureAspectState::Write),
                    }),
                },
                CompiledResourceTransition {
                    resource: ResourceHandle::Texture(texture.handle().unwrap()),
                    pass_index: 1,
                    execution_index: 1,
                    before: ResourceState::Texture(TextureResourceState::DepthStencilAttachment {
                        depth: Some(TextureAspectState::Write),
                        stencil: Some(TextureAspectState::Write),
                    },),
                    after: ResourceState::Texture(TextureResourceState::DepthStencilAttachment {
                        depth: Some(TextureAspectState::Read),
                        stencil: Some(TextureAspectState::Read),
                    }),
                },
            ]
        );
    }

    #[test]
    fn compile_allows_opaque_rect_to_modify_same_render_target() {
        let mut graph = FrameGraph::new();
        let texture = graph.declare_texture::<()>(test_texture_desc());
        graph.add_graphics_pass(WritePass {
            output: texture.clone(),
        });
        graph.add_graphics_pass(OpaqueRectPass::from_draw_rect_pass(DrawRectPass::new(
            RectPassParams::default(),
            DrawRectInput {
                render_target: RenderTargetIn::with_handle(
                    texture
                        .handle()
                        .expect("declared texture should have handle"),
                ),
                ..Default::default()
            },
            DrawRectOutput {
                render_target: RenderTargetOut::with_handle(
                    texture
                        .handle()
                        .expect("declared texture should have handle"),
                ),
            },
        )));

        graph.compile().expect("compile should succeed");

        let compiled = graph.compiled_graph().expect("compiled graph should exist");
        let timeline = compiled
            .resource_timelines
            .iter()
            .find(|timeline| {
                timeline.resource == ResourceHandle::Texture(texture.handle().unwrap())
            })
            .expect("resource timeline should exist");
        assert_eq!(timeline.transitions.len(), 2);
        assert_eq!(
            timeline.transitions[1].after,
            ResourceState::Texture(TextureResourceState::ColorAttachment)
        );
    }

    #[test]
    fn compile_groups_inline_graphics_passes_from_descriptor_compatibility() {
        let mut graph = FrameGraph::new();
        let texture = graph.declare_texture::<()>(test_texture_desc());
        let writer = graph.add_graphics_pass(WritePass {
            output: texture.clone(),
        });
        let inline_a = graph.add_graphics_pass(InlineLoadPass {
            target: texture.clone(),
        });
        let inline_b = graph.add_graphics_pass(InlineLoadPass {
            target: texture.clone(),
        });
        let present = graph.add_graphics_pass(make_present_pass(&texture));

        graph.compile().expect("compile should succeed");

        let compiled = graph.compiled_graph().expect("compiled graph should exist");
        assert_eq!(
            compiled.execution_plan.ordered_passes,
            vec![writer.0, inline_a.0, inline_b.0, present.0]
        );
        assert!(matches!(
            &compiled.execution_plan.steps[..],
            [
                CompiledExecuteStep::GraphicsPass { pass_index },
                CompiledExecuteStep::GraphicsPassGroup(RenderPassGroup { pass_indices, .. }),
                CompiledExecuteStep::GraphicsPass { pass_index: present_index },
            ] if *pass_index == writer.0
                && *present_index == present.0
                && pass_indices == &vec![inline_a.0, inline_b.0]
        ));
    }

    #[test]
    fn compile_prefers_longer_graphics_run_over_lower_index_compute() {
        let mut graph = FrameGraph::new();
        let compute = graph.add_compute_pass(ComputeStubPass);
        let surface_a = graph.add_graphics_pass(MergeableSurfacePass);
        let surface_b = graph.add_graphics_pass(MergeableSurfacePass);

        graph.compile().expect("compile should succeed");

        let compiled = graph.compiled_graph().expect("compiled graph should exist");
        assert_eq!(
            compiled.execution_plan.ordered_passes,
            vec![surface_a.0, surface_b.0, compute.0]
        );
        assert!(matches!(
            &compiled.execution_plan.steps[..],
            [
                CompiledExecuteStep::GraphicsPassGroup(RenderPassGroup { pass_indices, .. }),
                CompiledExecuteStep::ComputePass { pass_index },
            ] if pass_indices == &vec![surface_a.0, surface_b.0] && *pass_index == compute.0
        ));
    }

    #[test]
    fn compile_finishes_parallel_prep_before_final_target_batch() {
        let mut graph = FrameGraph::new();
        let prep_tex_a = graph.declare_texture::<()>(test_texture_desc());
        let prep_tex_b = graph.declare_texture::<()>(test_texture_desc());
        let prep_a = graph.add_graphics_pass(MergeablePrepPass {
            output: prep_tex_a.clone(),
        });
        let prep_b = graph.add_graphics_pass(MergeablePrepPass {
            output: prep_tex_b.clone(),
        });
        let final_a = graph.add_graphics_pass(MergeableFinalReadPass {
            input: InSlot::with_handle(prep_tex_a.handle().expect("texture a handle")),
        });
        let final_b = graph.add_graphics_pass(MergeableFinalReadPass {
            input: InSlot::with_handle(prep_tex_b.handle().expect("texture b handle")),
        });

        graph.compile().expect("compile should succeed");

        let compiled = graph.compiled_graph().expect("compiled graph should exist");
        assert_eq!(
            compiled.execution_plan.ordered_passes,
            vec![prep_a.0, prep_b.0, final_a.0, final_b.0]
        );
        assert!(matches!(
            &compiled.execution_plan.steps[..],
            [
                CompiledExecuteStep::GraphicsPass { pass_index: prep_first },
                CompiledExecuteStep::GraphicsPass { pass_index: prep_second },
                CompiledExecuteStep::GraphicsPassGroup(RenderPassGroup { pass_indices, .. }),
            ] if *prep_first == prep_a.0
                && *prep_second == prep_b.0
                && pass_indices == &vec![final_a.0, final_b.0]
        ));
    }

    #[test]
    fn compile_emits_execution_step_shapes_for_compute_and_transfer() {
        let mut graph = FrameGraph::new();
        let compute = graph.add_compute_pass(ComputeStubPass);
        let transfer = graph.add_transfer_pass(TransferStubPass);

        graph.compile().expect("compile should succeed");

        let compiled = graph.compiled_graph().expect("compiled graph should exist");
        assert_eq!(
            compiled.execution_plan.ordered_passes,
            vec![compute.0, transfer.0]
        );
        assert_eq!(
            compiled.execution_plan.steps,
            vec![
                CompiledExecuteStep::ComputePass {
                    pass_index: compute.0
                },
                CompiledExecuteStep::TransferPass {
                    pass_index: transfer.0
                },
            ]
        );
    }

    #[test]
    fn compile_aliases_transient_textures_when_lifetimes_do_not_overlap() {
        let mut graph = FrameGraph::new();
        let texture_a = graph.declare_texture::<()>(test_texture_desc());
        let texture_b = graph.declare_texture::<()>(test_texture_desc());
        graph.add_graphics_pass(WritePass {
            output: texture_a.clone(),
        });
        graph.add_graphics_pass(WritePass {
            output: texture_b.clone(),
        });

        graph.compile().expect("compile should succeed");

        let compiled = graph.compiled_graph().expect("compiled graph should exist");
        let resource_a = compiled
            .resources
            .iter()
            .find(|resource| {
                resource.handle == ResourceHandle::Texture(texture_a.handle().unwrap())
            })
            .expect("resource A should exist");
        let resource_b = compiled
            .resources
            .iter()
            .find(|resource| {
                resource.handle == ResourceHandle::Texture(texture_b.handle().unwrap())
            })
            .expect("resource B should exist");
        assert_eq!(resource_a.allocation_id, resource_b.allocation_id);
        assert_eq!(compiled.allocation_plan.texture_allocations.len(), 1);
    }

    #[test]
    fn compile_aliases_smaller_transient_texture_into_earlier_larger_slot() {
        let mut graph = FrameGraph::new();
        let large = graph.declare_texture::<()>(TextureDesc::new(
            200,
            100,
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureDimension::D2,
        ));
        let small = graph.declare_texture::<()>(TextureDesc::new(
            100,
            50,
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureDimension::D2,
        ));
        graph.add_graphics_pass(WritePass {
            output: large.clone(),
        });
        graph.add_graphics_pass(WritePass {
            output: small.clone(),
        });

        graph.compile().expect("compile should succeed");

        let compiled = graph.compiled_graph().expect("compiled graph should exist");
        let large_resource = compiled
            .resources
            .iter()
            .find(|resource| resource.handle == ResourceHandle::Texture(large.handle().unwrap()))
            .expect("large resource should exist");
        let small_resource = compiled
            .resources
            .iter()
            .find(|resource| resource.handle == ResourceHandle::Texture(small.handle().unwrap()))
            .expect("small resource should exist");
        assert_eq!(large_resource.allocation_id, small_resource.allocation_id);
        assert_eq!(compiled.allocation_plan.texture_allocations.len(), 1);
    }

    #[test]
    fn compile_does_not_alias_larger_transient_texture_after_earlier_smaller_slot() {
        let mut graph = FrameGraph::new();
        let small = graph.declare_texture::<()>(TextureDesc::new(
            100,
            50,
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureDimension::D2,
        ));
        let large = graph.declare_texture::<()>(TextureDesc::new(
            200,
            100,
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureDimension::D2,
        ));
        graph.add_graphics_pass(WritePass {
            output: small.clone(),
        });
        graph.add_graphics_pass(WritePass {
            output: large.clone(),
        });

        graph.compile().expect("compile should succeed");

        let compiled = graph.compiled_graph().expect("compiled graph should exist");
        let small_resource = compiled
            .resources
            .iter()
            .find(|resource| resource.handle == ResourceHandle::Texture(small.handle().unwrap()))
            .expect("small resource should exist");
        let large_resource = compiled
            .resources
            .iter()
            .find(|resource| resource.handle == ResourceHandle::Texture(large.handle().unwrap()))
            .expect("large resource should exist");
        assert_ne!(small_resource.allocation_id, large_resource.allocation_id);
        assert_eq!(compiled.allocation_plan.texture_allocations.len(), 2);
    }

    #[test]
    fn compile_marks_surface_as_external_owned() {
        let mut graph = FrameGraph::new();
        graph.add_graphics_pass(SurfacePass);

        graph.compile().expect("compile should succeed");

        let compiled = graph.compiled_graph().expect("compiled graph should exist");
        assert_eq!(
            compiled.allocation_plan.external_resources,
            vec![ExternalAllocationPlanEntry {
                resource: ExternalResource::Surface,
                owner: AllocationOwner::ExternalOwned,
            }]
        );
    }

    #[test]
    fn compile_keeps_internal_persistent_resources_out_of_aliasing() {
        let mut graph = FrameGraph::new();
        graph.add_graphics_pass(PersistentInternalPass::default());

        graph.compile().expect("compile should succeed");

        let compiled = graph.compiled_graph().expect("compiled graph should exist");
        let resource = compiled
            .resources
            .iter()
            .find(|resource| resource.lifetime == ResourceLifetime::Persistent)
            .expect("persistent resource should exist");
        assert_eq!(resource.stable_key, Some(0xCAFE));
        assert_eq!(resource.allocation_id, None);
        assert!(compiled.allocation_plan.texture_allocations.is_empty());
    }

    #[test]
    fn compile_keeps_transient_buffers_on_distinct_allocations() {
        let mut graph = FrameGraph::new();
        graph.add_graphics_pass(BufferWritePass::default());
        graph.add_graphics_pass(BufferWritePass::default());

        graph.compile().expect("compile should succeed");

        let compiled = graph.compiled_graph().expect("compiled graph should exist");
        let buffer_resources = compiled
            .resources
            .iter()
            .filter(|resource| matches!(resource.handle, ResourceHandle::Buffer(_)))
            .collect::<Vec<_>>();
        assert_eq!(buffer_resources.len(), 2);
        assert_ne!(
            buffer_resources[0].allocation_id,
            buffer_resources[1].allocation_id
        );
        assert_eq!(compiled.allocation_plan.buffer_allocations.len(), 2);
    }
}
