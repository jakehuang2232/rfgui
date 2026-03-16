use std::collections::{HashMap, HashSet, VecDeque};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::OnceLock;
use std::time::Instant;

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
pub struct GraphicsColorAttachmentDescriptor {
    pub target: AttachmentTarget,
    pub load_op: AttachmentLoadOp,
    pub store_op: AttachmentStoreOp,
    pub clear_color: Option<[f64; 4]>,
}

impl GraphicsColorAttachmentDescriptor {
    pub fn load(target: AttachmentTarget) -> Self {
        Self {
            target,
            load_op: AttachmentLoadOp::Load,
            store_op: AttachmentStoreOp::Store,
            clear_color: None,
        }
    }

    pub fn clear(target: AttachmentTarget, clear_color: [f64; 4]) -> Self {
        Self {
            target,
            load_op: AttachmentLoadOp::Clear,
            store_op: AttachmentStoreOp::Store,
            clear_color: Some(clear_color),
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
    texture_allocation_ids: HashMap<TextureHandle, AllocationId>,
    buffer_allocation_ids: HashMap<BufferHandle, AllocationId>,
    texture_stable_keys: HashMap<TextureHandle, u64>,
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

impl FrameGraph {
    pub fn new() -> Self {
        Self {
            passes: Vec::new(),
            textures: Vec::new(),
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

    pub(crate) fn texture_desc(&self, handle: TextureHandle) -> Option<TextureDesc> {
        self.textures.get(handle.0 as usize).copied()
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

    pub fn compile(&mut self) -> Result<(), FrameGraphError> {
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

        for node in &mut self.passes {
            let mut builder = PassBuilderState {
                descriptor: &mut node.descriptor,
                textures: &mut textures,
                buffers: &mut buffers,
                texture_metadata: &mut texture_metadata,
                buffer_metadata: &mut buffer_metadata,
                usages: &mut node.usages,
                build_errors: &mut build_errors,
            };
            node.pass.setup(&mut builder);
        }

        self.textures = textures;
        self.buffers = buffers;
        self.texture_metadata = texture_metadata;
        self.buffer_metadata = buffer_metadata;
        self.build_errors = build_errors;

        if let Some(err) = self.build_errors.pop() {
            return Err(err);
        }

        self.annotate_resource_versions();

        let compiled_graph = self.build_compiled_graph()?;
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
        Ok(())
    }

    pub fn compile_with_upload(&mut self, viewport: &mut Viewport) -> Result<(), FrameGraphError> {
        self.compile()?;
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
        Ok(())
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
        let mut latest_texture_version: HashMap<TextureHandle, TextureVersionId> = HashMap::new();
        let mut latest_buffer_version: HashMap<BufferHandle, BufferVersionId> = HashMap::new();
        let mut version_producers: HashMap<ResourceVersionId, usize> = HashMap::new();
        let mut consumed_versions: Vec<ConsumedVersion> = Vec::new();
        let mut produced_versions: Vec<ProducedVersion> = Vec::new();
        let mut next_texture_version = 0_u32;
        let mut next_buffer_version = 0_u32;

        for (pass_index, node) in self.passes.iter_mut().enumerate() {
            let descriptor = node.descriptor.clone();
            for usage in &mut node.usages {
                let (read_version, write_version) = annotate_usage_version(
                    usage.resource,
                    usage.usage,
                    &descriptor,
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

    fn build_compiled_graph(&self) -> Result<CompiledGraph, FrameGraphError> {
        let version_producers = self.build_version_producers();
        let latest_resource_versions = self.latest_resource_versions();
        let sink_passes =
            self.discover_sink_passes(&version_producers, &latest_resource_versions)?;
        let live_passes = self.discover_live_passes(&sink_passes, &version_producers)?;
        let (graph_edges, indegree) =
            self.build_live_dependency_graph(&live_passes, &version_producers)?;
        let ordered_passes = self.toposort_live_passes(&live_passes, &graph_edges, indegree)?;
        let execution_steps = self.build_execution_plan(&ordered_passes);
        let (pass_state_transitions, resource_transitions, resource_timelines) =
            self.build_resource_state_timelines(&ordered_passes)?;
        let (resources, allocation_plan, texture_allocation_ids, buffer_allocation_ids) =
            self.build_compiled_resources(&live_passes, &ordered_passes);
        let texture_stable_keys = resources
            .iter()
            .filter_map(|resource| match resource.handle {
                ResourceHandle::Texture(handle) => resource.stable_key.map(|key| (handle, key)),
                _ => None,
            })
            .collect::<HashMap<_, _>>();
        let culled_passes = (0..self.passes.len())
            .filter(|index| !live_passes.contains(index))
            .collect::<Vec<_>>();
        let live_set = live_passes.iter().copied().collect::<HashSet<_>>();
        let compiled_passes = ordered_passes
            .iter()
            .map(|&index| {
                let mut dependencies = graph_edges[index].iter().copied().collect::<Vec<_>>();
                dependencies.sort_unstable();
                CompiledPass {
                    original_index: index,
                    name: self.passes[index].descriptor.name,
                    descriptor: self.passes[index].descriptor.clone(),
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
            .filter(|pass| live_set.contains(&pass.original_index))
            .collect::<Vec<_>>();

        Ok(CompiledGraph {
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
        })
    }

    fn discover_sink_passes(
        &self,
        version_producers: &HashMap<ResourceVersionId, usize>,
        latest_resource_versions: &HashMap<ResourceHandle, ResourceVersionId>,
    ) -> Result<Vec<usize>, FrameGraphError> {
        if self.external_sinks.is_empty() {
            return Ok((0..self.passes.len()).collect());
        }

        let mut sink_passes = Vec::new();
        let mut seen = HashSet::new();
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
        version_producers: &HashMap<ResourceVersionId, usize>,
    ) -> Result<HashSet<usize>, FrameGraphError> {
        let mut live = HashSet::new();
        let mut stack = sink_passes.to_vec();
        while let Some(pass_index) = stack.pop() {
            if !live.insert(pass_index) {
                continue;
            }
            for (resource, version) in self.pass_input_versions(pass_index) {
                if let Some(&producer) = version_producers.get(&version) {
                    if producer != pass_index {
                        stack.push(producer);
                    }
                } else if !self.resource_has_external_input(resource) {
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
        live_passes: &HashSet<usize>,
        version_producers: &HashMap<ResourceVersionId, usize>,
    ) -> Result<(Vec<HashSet<usize>>, Vec<usize>), FrameGraphError> {
        let mut indegree = vec![0usize; self.passes.len()];
        let mut graph_edges: Vec<HashSet<usize>> = vec![HashSet::new(); self.passes.len()];

        self.validate_live_passes(live_passes, version_producers)?;

        for &index in live_passes {
            for (_, version) in self.pass_input_versions(index) {
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
        live_passes: &HashSet<usize>,
        version_producers: &HashMap<ResourceVersionId, usize>,
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

    fn build_version_producers(&self) -> HashMap<ResourceVersionId, usize> {
        let mut producers = HashMap::new();
        for (pass_index, node) in self.passes.iter().enumerate() {
            for usage in &node.usages {
                if let Some(version) = usage.write_version {
                    producers.insert(version, pass_index);
                }
            }
        }
        producers
    }

    fn latest_resource_versions(&self) -> HashMap<ResourceHandle, ResourceVersionId> {
        let mut latest = HashMap::new();
        for node in &self.passes {
            for usage in &node.usages {
                if let Some(version) = usage.write_version {
                    latest.insert(usage.resource, version);
                }
            }
        }
        latest
    }

    fn pass_input_versions(&self, pass_index: usize) -> Vec<(ResourceHandle, ResourceVersionId)> {
        let mut seen = HashSet::new();
        let mut versions = Vec::new();
        for usage in &self.passes[pass_index].usages {
            let Some(version) = usage.read_version else {
                continue;
            };
            if seen.insert((usage.resource, version)) {
                versions.push((usage.resource, version));
            }
        }
        versions
    }

    fn pass_consumed_versions(&self, pass_index: usize) -> Vec<ConsumedVersion> {
        let mut seen = HashSet::new();
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
        let mut seen = HashSet::new();
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
        live_passes: &HashSet<usize>,
        graph_edges: &[HashSet<usize>],
        indegree: &[usize],
        compatibility_keys: &[Option<RenderPassCompatibilityKey>],
    ) -> Vec<BatchAnchorInfo> {
        let analysis_order = topological_order_for_analysis(live_passes, graph_edges, indegree);
        let mut info = vec![BatchAnchorInfo::default(); self.passes.len()];

        for &pass_index in analysis_order.iter().rev() {
            let mut best_downstream: Option<BatchAnchorInfo> = None;
            for &consumer in &graph_edges[pass_index] {
                if !live_passes.contains(&consumer) {
                    continue;
                }
                let candidate = &info[consumer];
                if candidate.anchor_signature.is_none() {
                    continue;
                }
                if best_downstream
                    .as_ref()
                    .is_none_or(|best| candidate.distance_to_anchor < best.distance_to_anchor)
                {
                    best_downstream = Some(candidate.clone());
                }
            }

            let mergeable_signature =
                is_mergeable_graphics_pass(&self.passes[pass_index].descriptor)
                    .then(|| compatibility_keys[pass_index].clone())
                    .flatten();

            info[pass_index] = if let Some(downstream) = best_downstream {
                if mergeable_signature.is_none() {
                    BatchAnchorInfo {
                        anchor_signature: downstream.anchor_signature,
                        distance_to_anchor: downstream.distance_to_anchor.saturating_add(1),
                    }
                } else {
                    BatchAnchorInfo {
                        anchor_signature: downstream.anchor_signature,
                        distance_to_anchor: downstream.distance_to_anchor.saturating_add(1),
                    }
                }
            } else if let Some(signature) = mergeable_signature {
                BatchAnchorInfo {
                    anchor_signature: Some(signature),
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
        live_passes: &HashSet<usize>,
        graph_edges: &[HashSet<usize>],
        mut indegree: Vec<usize>,
    ) -> Result<Vec<usize>, FrameGraphError> {
        let mut order = Vec::new();
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
        let mut last_signature: Option<RenderPassCompatibilityKey> = None;

        while !queue.is_empty() {
            let n = select_next_ready_node(
                &queue,
                &compatibility_keys,
                &batch_anchor_info,
                last_signature.as_ref(),
                graph_edges,
                &indegree,
                live_passes,
            );
            let n = remove_from_queue(&mut queue, n);
            order.push(n);
            last_signature = compatibility_keys[n].clone();
            for &m in &graph_edges[n] {
                indegree[m] -= 1;
                if indegree[m] == 0 && live_passes.contains(&m) {
                    queue.push_back(m);
                }
            }
        }

        if order.len() != live_passes.len() {
            return Err(FrameGraphError::CyclicDependency);
        }

        Ok(order)
    }

    fn build_execution_plan(&self, order: &[usize]) -> Vec<CompiledExecuteStep> {
        let mut steps = Vec::new();
        let mut cursor = 0usize;
        while cursor < order.len() {
            let index = order[cursor];
            match self.passes[index].descriptor.kind {
                PassKind::Graphics => {
                    let Some(current_key) =
                        render_pass_compatibility_key(&self.passes[index].descriptor)
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
                        if self.passes[next_index].descriptor.kind != PassKind::Graphics {
                            break;
                        }
                        let Some(next_key) =
                            render_pass_compatibility_key(&self.passes[next_index].descriptor)
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
                                absorbed_load_variant = Some(next_key.clone());
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

    fn build_compiled_resources(
        &self,
        live_passes: &HashSet<usize>,
        ordered_passes: &[usize],
    ) -> (
        Vec<CompiledResource>,
        AllocationPlan,
        HashMap<TextureHandle, AllocationId>,
        HashMap<BufferHandle, AllocationId>,
    ) {
        let ordered_positions = ordered_passes
            .iter()
            .enumerate()
            .map(|(order, &pass_index)| (pass_index, order))
            .collect::<HashMap<_, _>>();
        let mut resources = HashMap::<ResourceHandle, CompiledResource>::new();

        for (pass_index, node) in self.passes.iter().enumerate() {
            if !live_passes.contains(&pass_index) {
                continue;
            }
            let Some(order_index) = ordered_positions.get(&pass_index).copied() else {
                continue;
            };
            let mut producers = HashSet::new();
            let mut consumers = HashSet::new();
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
            HashMap<usize, Vec<CompiledPassResourceTransition>>,
            Vec<CompiledResourceTransition>,
            Vec<CompiledResourceTimeline>,
        ),
        FrameGraphError,
    > {
        let mut current_states = HashMap::<ResourceHandle, ResourceState>::new();
        let mut pass_transitions = HashMap::<usize, Vec<CompiledPassResourceTransition>>::new();
        let mut resource_transitions = Vec::<CompiledResourceTransition>::new();
        let mut timeline_map = HashMap::<ResourceHandle, Vec<CompiledResourceTransition>>::new();

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

        let mut resources: HashSet<ResourceHandle> = HashSet::new();
        let mut write_edges: HashSet<(usize, ResourceHandle)> = HashSet::new();
        let mut read_edges: HashSet<(ResourceHandle, usize)> = HashSet::new();
        let mut modify_edges: HashSet<(usize, ResourceHandle)> = HashSet::new();

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
        let mut pass_timings: HashMap<String, f64> = HashMap::new();
        let mut pass_counts: HashMap<String, usize> = HashMap::new();
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
        for step in self.execute_steps.clone() {
            match step {
                ExecuteStep::GraphicsPass { index } => self.execute_graphics_pass(
                    index,
                    &mut ctx,
                    &mut pass_timings,
                    &mut pass_counts,
                    &mut pass_first_seen_order,
                ),
                ExecuteStep::ComputePass { index } => self.execute_compute_pass(
                    index,
                    &mut ctx,
                    &mut pass_timings,
                    &mut pass_counts,
                    &mut pass_first_seen_order,
                ),
                ExecuteStep::TransferPass { index } => self.execute_transfer_pass(
                    index,
                    &mut ctx,
                    &mut pass_timings,
                    &mut pass_counts,
                    &mut pass_first_seen_order,
                ),
                ExecuteStep::GraphicsPassGroup(group) => self.execute_graphics_group(
                    &group,
                    &mut ctx,
                    &mut pass_timings,
                    &mut pass_counts,
                    &mut pass_first_seen_order,
                ),
            }
        }
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
    anchor_signature: Option<RenderPassCompatibilityKey>,
    distance_to_anchor: usize,
}

impl FrameGraph {
    #[allow(clippy::too_many_arguments)]
    fn execute_graphics_pass(
        &mut self,
        index: usize,
        ctx: &mut RecordContext<'_, '_>,
        pass_timings: &mut HashMap<String, f64>,
        pass_counts: &mut HashMap<String, usize>,
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
        pass_timings: &mut HashMap<String, f64>,
        pass_counts: &mut HashMap<String, usize>,
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
        pass_timings: &mut HashMap<String, f64>,
        pass_counts: &mut HashMap<String, usize>,
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
        pass_timings: &mut HashMap<String, f64>,
        pass_counts: &mut HashMap<String, usize>,
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
        pass_timings: &mut HashMap<String, f64>,
        pass_counts: &mut HashMap<String, usize>,
        pass_first_seen_order: &mut Vec<String>,
    ) {
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
    pass_timings: &mut HashMap<String, f64>,
    pass_counts: &mut HashMap<String, usize>,
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
    latest_texture_version: &mut HashMap<TextureHandle, TextureVersionId>,
    latest_buffer_version: &mut HashMap<BufferHandle, BufferVersionId>,
    next_texture_version: &mut u32,
    next_buffer_version: &mut u32,
) -> (Option<ResourceVersionId>, Option<ResourceVersionId>) {
    match resource {
        ResourceHandle::Texture(handle) => annotate_texture_usage_version(
            handle,
            usage,
            descriptor,
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
    latest_versions: &mut HashMap<TextureHandle, TextureVersionId>,
    next_version: &mut u32,
) -> (Option<ResourceVersionId>, Option<ResourceVersionId>) {
    let current = latest_versions
        .get(&handle)
        .copied()
        .map(ResourceVersionId::Texture);
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
            let read =
                if color_attachment_requires_input(descriptor, ResourceHandle::Texture(handle)) {
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
    latest_versions: &mut HashMap<BufferHandle, BufferVersionId>,
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
    latest_versions: &mut HashMap<TextureHandle, TextureVersionId>,
    next_version: &mut u32,
) -> TextureVersionId {
    let version = TextureVersionId(*next_version);
    *next_version = next_version.saturating_add(1);
    latest_versions.insert(handle, version);
    version
}

fn allocate_buffer_version(
    handle: BufferHandle,
    latest_versions: &mut HashMap<BufferHandle, BufferVersionId>,
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
    let mut grouped = HashMap::<ResourceHandle, Vec<ResourceUsage>>::new();
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

fn remove_from_queue(queue: &mut VecDeque<usize>, value: usize) -> usize {
    if let Some(pos) = queue.iter().position(|&v| v == value) {
        queue.remove(pos).expect("queue index should exist")
    } else {
        queue.pop_front().expect("queue should not be empty")
    }
}

fn select_next_ready_node(
    queue: &VecDeque<usize>,
    signatures: &[Option<RenderPassCompatibilityKey>],
    batch_anchor_info: &[BatchAnchorInfo],
    last_signature: Option<&RenderPassCompatibilityKey>,
    graph_edges: &[HashSet<usize>],
    indegree: &[usize],
    live_passes: &HashSet<usize>,
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

    let mut anchor_counts = HashMap::<RenderPassCompatibilityKey, usize>::new();
    for &idx in queue {
        let Some(anchor_signature) = batch_anchor_info[idx].anchor_signature.clone() else {
            continue;
        };
        *anchor_counts.entry(anchor_signature).or_insert(0) += 1;
    }
    let mut best_anchor_choice: Option<(usize, usize, usize)> = None;
    for &idx in queue {
        let Some(anchor_signature) = batch_anchor_info[idx].anchor_signature.as_ref() else {
            continue;
        };
        let ready_count = anchor_counts.get(anchor_signature).copied().unwrap_or(0);
        let distance = batch_anchor_info[idx].distance_to_anchor;
        if best_anchor_choice
            .as_ref()
            .is_none_or(|(best_idx, best_count, best_distance)| {
                ready_count > *best_count
                    || (ready_count == *best_count && distance > *best_distance)
                    || (ready_count == *best_count && distance == *best_distance && idx < *best_idx)
            })
        {
            best_anchor_choice = Some((idx, ready_count, distance));
        }
    }
    if let Some((idx, ready_count, distance)) = best_anchor_choice
        && (ready_count > 1 || distance > 0)
    {
        return idx;
    }

    let mut best_graphics_choice: Option<(usize, usize)> = None;
    for &idx in queue {
        let Some(signature) = signatures[idx].as_ref() else {
            continue;
        };
        let run_len = estimate_compatible_run_length(
            idx,
            signature,
            queue,
            signatures,
            graph_edges,
            indegree,
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
    live_passes: &HashSet<usize>,
    graph_edges: &[HashSet<usize>],
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
    queue: &VecDeque<usize>,
    signatures: &[Option<RenderPassCompatibilityKey>],
    graph_edges: &[HashSet<usize>],
    indegree: &[usize],
    live_passes: &HashSet<usize>,
) -> usize {
    let mut simulated_indegree = indegree.to_vec();
    let mut ready: HashSet<usize> = queue.iter().copied().collect();
    let mut run_len = 0usize;
    let mut current = Some(start_idx);

    while let Some(node) = current.take() {
        if !ready.remove(&node) {
            break;
        }
        run_len += 1;
        for &next in &graph_edges[node] {
            simulated_indegree[next] = simulated_indegree[next].saturating_sub(1);
            if simulated_indegree[next] == 0 && live_passes.contains(&next) {
                ready.insert(next);
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
    HashMap<TextureHandle, AllocationId>,
    HashMap<BufferHandle, AllocationId>,
) {
    #[derive(Clone, Copy)]
    struct TextureSlot {
        id: AllocationId,
        desc: TextureDesc,
        last_use_pass_index: usize,
    }

    let mut next_id = 0u32;
    let mut texture_slots: Vec<TextureSlot> = Vec::new();
    let mut texture_allocations: Vec<TextureAllocationPlanEntry> = Vec::new();
    let mut buffer_allocations: Vec<BufferAllocationPlanEntry> = Vec::new();
    let mut texture_allocation_ids = HashMap::new();
    let mut buffer_allocation_ids = HashMap::new();

    for resource in resources {
        match resource.handle {
            ResourceHandle::Texture(handle) => {
                if resource.lifetime != ResourceLifetime::Transient {
                    continue;
                }
                let Some(desc) = graph.textures.get(handle.0 as usize).copied() else {
                    continue;
                };
                let chosen = texture_slots
                    .iter_mut()
                    .find(|slot| {
                        slot.last_use_pass_index < resource.first_use_pass_index
                            && slot.desc.width() == desc.width()
                            && slot.desc.height() == desc.height()
                            && slot.desc.format() == desc.format()
                            && slot.desc.dimension() == desc.dimension()
                            && slot.desc.usage() == desc.usage()
                            && slot.desc.sample_count() == desc.sample_count()
                    })
                    .map(|slot| {
                        slot.last_use_pass_index = resource.last_use_pass_index;
                        slot.id
                    })
                    .unwrap_or_else(|| {
                        let id = AllocationId(next_id);
                        next_id = next_id.saturating_add(1);
                        texture_slots.push(TextureSlot {
                            id,
                            desc,
                            last_use_pass_index: resource.last_use_pass_index,
                        });
                        texture_allocations.push(TextureAllocationPlanEntry {
                            allocation_id: id,
                            owner: AllocationOwner::AllocatorManaged,
                            resources: Vec::new(),
                        });
                        id
                    });
                texture_allocation_ids.insert(handle, chosen);
                if let Some(entry) = texture_allocations
                    .iter_mut()
                    .find(|entry| entry.allocation_id == chosen)
                {
                    entry.resources.push(handle);
                }
            }
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
    texture_allocation_ids: &'b HashMap<TextureHandle, AllocationId>,
    texture_stable_keys: &'b HashMap<TextureHandle, u64>,
    buffer_allocation_ids: &'b HashMap<BufferHandle, AllocationId>,
}

impl<'a, 'b> PrepareContext<'a, 'b> {
    pub(crate) fn new(
        viewport: &'a mut Viewport,
        textures: &'b [TextureDesc],
        buffers: &'b [BufferDesc],
        texture_allocation_ids: &'b HashMap<TextureHandle, AllocationId>,
        texture_stable_keys: &'b HashMap<TextureHandle, u64>,
        buffer_allocation_ids: &'b HashMap<BufferHandle, AllocationId>,
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
    texture_allocation_ids: &'b HashMap<TextureHandle, AllocationId>,
    texture_stable_keys: &'b HashMap<TextureHandle, u64>,
    buffer_allocation_ids: &'b HashMap<BufferHandle, AllocationId>,
    detail_timings: HashMap<String, f64>,
    detail_counts: HashMap<String, usize>,
    detail_order: Vec<String>,
}

impl<'a, 'b> RecordContext<'a, 'b> {
    pub(crate) fn new(
        viewport: &'a mut Viewport,
        textures: &'b [TextureDesc],
        buffers: &'b [BufferDesc],
        texture_allocation_ids: &'b HashMap<TextureHandle, AllocationId>,
        texture_stable_keys: &'b HashMap<TextureHandle, u64>,
        buffer_allocation_ids: &'b HashMap<BufferHandle, AllocationId>,
    ) -> Self {
        Self {
            viewport,
            textures,
            buffers,
            texture_allocation_ids,
            texture_stable_keys,
            buffer_allocation_ids,
            detail_timings: HashMap::new(),
            detail_counts: HashMap::new(),
            detail_order: Vec::new(),
        }
    }

    pub fn record_detail_timing(&mut self, name: &'static str, elapsed_ms: f64) {
        if !self.viewport.debug_trace_render_time() || elapsed_ms <= 0.0 {
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
        if !self.viewport.debug_trace_render_time() {
            return;
        }
        let name = name.to_string();
        if !self.detail_timings.contains_key(&name) {
            self.detail_order.push(name.clone());
        }
        self.detail_timings.entry(name.clone()).or_insert(0.0);
        *self.detail_counts.entry(name).or_insert(0) += 1;
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
    texture_allocation_ids: &'res HashMap<TextureHandle, AllocationId>,
    texture_stable_keys: &'res HashMap<TextureHandle, u64>,
    buffer_allocation_ids: &'res HashMap<BufferHandle, AllocationId>,
    detail_timings: &'ctx mut HashMap<String, f64>,
    detail_counts: &'ctx mut HashMap<String, usize>,
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
        if !self.viewport.debug_trace_render_time() || elapsed_ms <= 0.0 {
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
        if !self.viewport.debug_trace_render_time() {
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
    texture_allocation_ids: &'res HashMap<TextureHandle, AllocationId>,
    texture_stable_keys: &'res HashMap<TextureHandle, u64>,
    buffer_allocation_ids: &'res HashMap<BufferHandle, AllocationId>,
    detail_timings: &'ctx mut HashMap<String, f64>,
    detail_counts: &'ctx mut HashMap<String, usize>,
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
        if !self.viewport.debug_trace_render_time() || elapsed_ms <= 0.0 {
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
        if !self.viewport.debug_trace_render_time() {
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
    texture_allocation_ids: &'res HashMap<TextureHandle, AllocationId>,
    texture_stable_keys: &'res HashMap<TextureHandle, u64>,
    buffer_allocation_ids: &'res HashMap<BufferHandle, AllocationId>,
    detail_timings: &'ctx mut HashMap<String, f64>,
    detail_counts: &'ctx mut HashMap<String, usize>,
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
        if !self.viewport.debug_trace_render_time() || elapsed_ms <= 0.0 {
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
        if !self.viewport.debug_trace_render_time() {
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

pub struct ResourceCache<T> {
    store: HashMap<u64, T>,
}

impl<T> ResourceCache<T> {
    pub fn new() -> Self {
        Self {
            store: HashMap::new(),
        }
    }

    pub fn clear(&mut self) {
        self.store.clear();
    }

    pub fn get_or_insert_with<F: FnOnce() -> T>(&mut self, key: u64, create: F) -> &mut T {
        self.store.entry(key).or_insert_with(create)
    }
}

impl<T> Default for ResourceCache<T> {
    fn default() -> Self {
        Self::new()
    }
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
            builder.write_color(
                &self.output,
                GraphicsColorAttachmentDescriptor::clear(target, [0.0, 0.0, 0.0, 0.0]),
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
            builder.write_color(
                &self.target,
                GraphicsColorAttachmentDescriptor::load(target),
            );
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
            builder.write_surface_color(GraphicsColorAttachmentDescriptor::clear(
                builder.surface_target(),
                [0.0, 0.0, 0.0, 0.0],
            ));
        }

        fn execute(&mut self, _ctx: &mut GraphicsCtx<'_, '_, '_, '_>) {}
    }

    impl GraphicsPass for MergeableSurfacePass {
        fn setup(&mut self, builder: &mut GraphicsPassBuilder<'_, '_>) {
            builder.set_graphics_merge_policy(GraphicsPassMergePolicy::Mergeable);
            builder.write_surface_color(GraphicsColorAttachmentDescriptor::clear(
                builder.surface_target(),
                [0.0, 0.0, 0.0, 0.0],
            ));
        }

        fn execute(&mut self, _ctx: &mut GraphicsCtx<'_, '_, '_, '_>) {}
    }

    impl GraphicsPass for MergeablePrepPass {
        fn setup(&mut self, builder: &mut GraphicsPassBuilder<'_, '_>) {
            builder.set_graphics_merge_policy(GraphicsPassMergePolicy::Mergeable);
            let target = builder
                .texture_target(&self.output)
                .expect("prep output should have texture target");
            builder.write_color(
                &self.output,
                GraphicsColorAttachmentDescriptor::clear(target, [0.0, 0.0, 0.0, 0.0]),
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
            builder.write_surface_color(GraphicsColorAttachmentDescriptor::clear(
                builder.surface_target(),
                [0.0, 0.0, 0.0, 0.0],
            ));
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
            builder.write_color(
                &self.target,
                GraphicsColorAttachmentDescriptor::load(target),
            );
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
            builder.write_color(
                &self.output,
                GraphicsColorAttachmentDescriptor::clear(target, [0.0, 0.0, 0.0, 0.0]),
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
