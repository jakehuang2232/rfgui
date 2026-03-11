use std::collections::{HashMap, HashSet, VecDeque};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::OnceLock;
use std::time::Instant;

use super::buffer_resource::{BufferDesc, BufferHandle};
use super::builder::PassBuilder;
use super::texture_resource::{TextureDesc, TextureHandle};
use crate::view::render_pass::draw_rect_pass::{DrawRectPass, OpaqueRectPass};
use crate::view::render_pass::debug_overlay_pass::DebugOverlayPass;
use crate::view::render_pass::present_surface_pass::PresentSurfacePass;
use crate::view::render_pass::render_target::{render_target_msaa_view, render_target_view};
use crate::view::render_pass::{PassWrapper, RenderPass, RenderPassBatchKey, RenderPassDyn};
use crate::view::viewport::Viewport;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResourceHandle {
    Texture(TextureHandle),
    Buffer(BufferHandle),
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
pub struct PassResourceUsage {
    pub resource: ResourceHandle,
    pub usage: ResourceUsage,
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
    pass: Box<dyn RenderPassDyn>,
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
    pub is_root: bool,
}

#[derive(Clone, Debug)]
pub struct CompiledResource {
    pub handle: ResourceHandle,
    pub first_pass_order: usize,
    pub last_pass_order: usize,
    pub producer_passes: Vec<usize>,
    pub consumer_passes: Vec<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompiledExecuteStep {
    Single { pass_index: usize },
    SharedRun { start: usize, end: usize },
}

#[derive(Clone, Debug, Default)]
pub struct ExecutionPlan {
    pub ordered_passes: Vec<usize>,
    pub steps: Vec<CompiledExecuteStep>,
}

#[derive(Clone, Debug, Default)]
pub struct CompiledGraph {
    pub passes: Vec<CompiledPass>,
    pub resources: Vec<CompiledResource>,
    pub culled_passes: Vec<usize>,
    pub execution_plan: ExecutionPlan,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PassResourceSummary {
    requires_input: bool,
    writes: bool,
    produced: bool,
}

#[derive(Clone, Copy)]
enum ExecuteStep {
    Single { index: usize },
    SharedRun { start: usize, end: usize },
}

pub struct FrameGraph {
    passes: Vec<PassNode>,
    textures: Vec<TextureDesc>,
    buffers: Vec<BufferDesc>,
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
            compiled_graph: None,
            order: Vec::new(),
            compiled: false,
            build_errors: Vec::new(),
            execute_steps: Vec::new(),
        }
    }

    pub fn add_pass<P: RenderPass + 'static>(&mut self, pass: P) -> PassHandle {
        let name = std::any::type_name::<P>();
        let node = PassNode {
            pass: Box::new(PassWrapper { pass }),
            descriptor: PassDescriptor::graphics(name),
            usages: Vec::new(),
        };
        let handle = PassHandle(self.passes.len());
        self.passes.push(node);
        self.compiled_graph = None;
        self.compiled = false;
        handle
    }

    pub fn declare_texture<Tag>(
        &mut self,
        desc: TextureDesc,
    ) -> super::slot::OutSlot<super::texture_resource::TextureResource, Tag> {
        let handle = TextureHandle(self.textures.len() as u32);
        self.textures.push(desc);
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
        let mut build_errors: Vec<FrameGraphError> = Vec::new();

        for node in &mut self.passes {
            let mut builder = PassBuilder {
                descriptor: &mut node.descriptor,
                textures: &mut textures,
                buffers: &mut buffers,
                usages: &mut node.usages,
                build_errors: &mut build_errors,
            };
            node.pass.setup(&mut builder);
        }

        self.textures = textures;
        self.buffers = buffers;
        self.build_errors = build_errors;

        if let Some(err) = self.build_errors.pop() {
            return Err(err);
        }

        let compiled_graph = self.build_compiled_graph()?;
        self.order = compiled_graph.execution_plan.ordered_passes.clone();
        self.execute_steps = compiled_graph
            .execution_plan
            .steps
            .iter()
            .map(|step| match *step {
                CompiledExecuteStep::Single { pass_index } => ExecuteStep::Single { index: pass_index },
                CompiledExecuteStep::SharedRun { start, end } => ExecuteStep::SharedRun { start, end },
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
                        pass.batch_key()
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
        let mut ctx = PrepareContext::new(viewport, &textures, &buffers);
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

    fn build_compiled_graph(&self) -> Result<CompiledGraph, FrameGraphError> {
        let mut root_passes = self.discover_root_passes();
        if root_passes.is_empty() {
            root_passes = (0..self.passes.len()).collect();
        }

        let pass_summaries = self.pass_resource_summaries()?;
        let live_passes = self.discover_live_passes(&root_passes, &pass_summaries)?;
        let (graph_edges, indegree) =
            self.build_live_dependency_graph(&live_passes, &pass_summaries)?;
        let ordered_passes = self.toposort_live_passes(&live_passes, &graph_edges, indegree)?;
        let execution_steps = self.build_execution_plan(&ordered_passes);
        let resources = self.build_compiled_resources(&live_passes, &ordered_passes, &pass_summaries);
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
                    is_root: root_passes.contains(&index),
                }
            })
            .filter(|pass| live_set.contains(&pass.original_index))
            .collect::<Vec<_>>();

        Ok(CompiledGraph {
            passes: compiled_passes,
            resources,
            culled_passes,
            execution_plan: ExecutionPlan {
                ordered_passes,
                steps: execution_steps,
            },
        })
    }

    fn discover_root_passes(&self) -> Vec<usize> {
        let present_name = std::any::type_name::<PresentSurfacePass>();
        let debug_overlay_name = std::any::type_name::<DebugOverlayPass>();
        self.passes
            .iter()
            .enumerate()
            .filter_map(|(index, node)| {
                let name = node.descriptor.name;
                if name == present_name || name == debug_overlay_name {
                    Some(index)
                } else {
                    None
                }
            })
            .collect()
    }

    fn pass_resource_summaries(
        &self,
    ) -> Result<Vec<HashMap<ResourceHandle, PassResourceSummary>>, FrameGraphError> {
        self.passes
            .iter()
            .map(|node| summarize_pass_resources(&node.descriptor, &node.usages))
            .collect()
    }

    fn discover_live_passes(
        &self,
        root_passes: &[usize],
        pass_summaries: &[HashMap<ResourceHandle, PassResourceSummary>],
    ) -> Result<HashSet<usize>, FrameGraphError> {
        let mut live = HashSet::new();
        let mut stack = root_passes.to_vec();
        while let Some(pass_index) = stack.pop() {
            if !live.insert(pass_index) {
                continue;
            }
            for (&resource, summary) in &pass_summaries[pass_index] {
                if !summary.requires_input || summary.produced {
                    continue;
                }
                let Some(producer) =
                    find_latest_writer_before(pass_summaries, resource, pass_index)
                else {
                    return Err(FrameGraphError::MissingInput(
                        "live pass requires a resource before it is written",
                    ));
                };
                stack.push(producer);
            }
        }
        Ok(live)
    }

    fn build_live_dependency_graph(
        &self,
        live_passes: &HashSet<usize>,
        pass_summaries: &[HashMap<ResourceHandle, PassResourceSummary>],
    ) -> Result<(Vec<HashSet<usize>>, Vec<usize>), FrameGraphError> {
        let mut indegree = vec![0usize; self.passes.len()];
        let mut graph_edges: Vec<HashSet<usize>> = vec![HashSet::new(); self.passes.len()];
        let mut per_resource_usages: HashMap<ResourceHandle, Vec<(usize, ResourceAccess)>> =
            HashMap::new();

        self.validate_live_passes(live_passes, pass_summaries)?;

        for (index, summary_map) in pass_summaries.iter().enumerate() {
            if !live_passes.contains(&index) {
                continue;
            }
            for (&resource, summary) in summary_map {
                if let Some(access) = summary_to_access(*summary) {
                    per_resource_usages
                        .entry(resource)
                        .or_default()
                        .push((index, access));
                }
            }
        }

        for (_resource, mut usages) in per_resource_usages {
            usages.sort_by_key(|(index, _)| *index);
            let mut prior_reads: Vec<usize> = Vec::new();
            let mut last_writer: Option<usize> = None;

            for (index, access) in usages {
                match access {
                    ResourceAccess::Read => {
                        let Some(writer) = last_writer else {
                            return Err(FrameGraphError::MissingInput(
                                "resource has no producer in live graph",
                            ));
                        };
                        if writer != index && graph_edges[writer].insert(index) {
                            indegree[index] += 1;
                        }
                        prior_reads.push(index);
                    }
                    ResourceAccess::Write => {
                        if let Some(writer) = last_writer
                            && writer != index
                            && graph_edges[writer].insert(index)
                        {
                            indegree[index] += 1;
                        }
                        for reader in prior_reads.drain(..) {
                            if reader != index && graph_edges[reader].insert(index) {
                                indegree[index] += 1;
                            }
                        }
                        last_writer = Some(index);
                    }
                    ResourceAccess::Modify => {
                        let Some(writer) = last_writer else {
                            return Err(FrameGraphError::MissingInput(
                                "resource modify requires prior producer",
                            ));
                        };
                        if writer != index && graph_edges[writer].insert(index) {
                            indegree[index] += 1;
                        }
                        for reader in prior_reads.drain(..) {
                            if reader != index && graph_edges[reader].insert(index) {
                                indegree[index] += 1;
                            }
                        }
                        last_writer = Some(index);
                    }
                }
            }

        }

        Ok((graph_edges, indegree))
    }

    fn validate_live_passes(
        &self,
        live_passes: &HashSet<usize>,
        pass_summaries: &[HashMap<ResourceHandle, PassResourceSummary>],
    ) -> Result<(), FrameGraphError> {
        for &index in live_passes {
            validate_pass_descriptor(&self.passes[index].descriptor, &self.textures)?;
        }

        let mut touched_resources = HashSet::new();
        for (index, summary_map) in pass_summaries.iter().enumerate() {
            if !live_passes.contains(&index) {
                continue;
            }
            for &resource in summary_map.keys() {
                touched_resources.insert(resource);
            }
        }

        for resource in touched_resources {
            let mut has_writer = false;
            let mut seen_pure_writer = false;
            for (index, summary_map) in pass_summaries.iter().enumerate() {
                if !live_passes.contains(&index) {
                    continue;
                }
                let Some(summary) = summary_map.get(&resource).copied() else {
                    continue;
                };
                let Some(access) = summary_to_access(summary) else {
                    continue;
                };
                match access {
                    ResourceAccess::Read => {
                        if !has_writer {
                            return Err(FrameGraphError::MissingInput(
                                "resource is read before any live producer",
                            ));
                        }
                    }
                    ResourceAccess::Modify => {
                        if !has_writer {
                            return Err(FrameGraphError::MissingInput(
                                "resource is modified before any live producer",
                            ));
                        }
                        has_writer = true;
                    }
                    ResourceAccess::Write => {
                        if has_writer && seen_pure_writer {
                            return Err(FrameGraphError::MultipleWriters);
                        }
                        has_writer = true;
                        seen_pure_writer = true;
                    }
                }
            }
        }

        Ok(())
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

        let batch_signatures: Vec<Option<RenderPassBatchKey>> = self
            .passes
            .iter()
            .enumerate()
            .map(|(index, node)| {
                if !live_passes.contains(&index) || !node.pass.batchable() {
                    return None;
                }
                node.pass.batch_key()
            })
            .collect();
        let mut last_signature: Option<RenderPassBatchKey> = None;

        while !queue.is_empty() {
            let n = select_next_ready_node(&queue, &batch_signatures, last_signature);
            let n = remove_from_queue(&mut queue, n);
            order.push(n);
            last_signature = batch_signatures[n];
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
            let key = self.passes[index].pass.batch_key();
            if !self.passes[index].pass.shared_render_pass_capable() || key.is_none() {
                steps.push(CompiledExecuteStep::Single { pass_index: index });
                cursor += 1;
                continue;
            }
            let current_key = key;
            let mut end = cursor + 1;
            while end < order.len() {
                let next_index = order[end];
                if !self.passes[next_index].pass.shared_render_pass_capable() {
                    break;
                }
                let next_key = self.passes[next_index].pass.batch_key();
                let compatible = match (current_key, next_key) {
                    (Some(a), Some(b)) => batch_keys_compatible(a, b),
                    _ => false,
                };
                if !compatible {
                    break;
                }
                end += 1;
            }
            if end > cursor + 1 {
                steps.push(CompiledExecuteStep::SharedRun { start: cursor, end });
                cursor = end;
            } else {
                steps.push(CompiledExecuteStep::Single { pass_index: index });
                cursor += 1;
            }
        }
        steps
    }

    fn build_compiled_resources(
        &self,
        live_passes: &HashSet<usize>,
        ordered_passes: &[usize],
        pass_summaries: &[HashMap<ResourceHandle, PassResourceSummary>],
    ) -> Vec<CompiledResource> {
        let ordered_positions = ordered_passes
            .iter()
            .enumerate()
            .map(|(order, &pass_index)| (pass_index, order))
            .collect::<HashMap<_, _>>();
        let mut resources = HashMap::<ResourceHandle, CompiledResource>::new();

        for (pass_index, summary_map) in pass_summaries.iter().enumerate() {
            if !live_passes.contains(&pass_index) {
                continue;
            }
            let Some(order_index) = ordered_positions.get(&pass_index).copied() else {
                continue;
            };
            for (&resource, summary) in summary_map {
                let compiled = resources.entry(resource).or_insert(CompiledResource {
                    handle: resource,
                    first_pass_order: order_index,
                    last_pass_order: order_index,
                    producer_passes: Vec::new(),
                    consumer_passes: Vec::new(),
                });
                compiled.first_pass_order = compiled.first_pass_order.min(order_index);
                compiled.last_pass_order = compiled.last_pass_order.max(order_index);
                if summary.writes || summary.produced {
                    compiled.producer_passes.push(pass_index);
                }
                if summary.requires_input {
                    compiled.consumer_passes.push(pass_index);
                }
            }
        }

        let mut ordered = resources.into_values().collect::<Vec<_>>();
        ordered.sort_by_key(|resource| match resource.handle {
            ResourceHandle::Texture(handle) => (0, handle.0),
            ResourceHandle::Buffer(handle) => (1, handle.0),
        });
        ordered
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

    pub fn normalize_opaque_rect_depths(&mut self) {
        let mut total = 0_u32;
        for node in &mut self.passes {
            if node
                .pass
                .as_any_mut()
                .downcast_mut::<OpaqueRectPass>()
                .is_some()
            {
                total = total.saturating_add(1);
            }
        }
        if total == 0 {
            return;
        }
        for node in &mut self.passes {
            if let Some(pass) = node.pass.as_any_mut().downcast_mut::<OpaqueRectPass>() {
                pass.normalize_depth(total);
            }
        }
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
        let mut ctx = RecordContext::new(viewport, &textures, &buffers);
        for step in self.execute_steps.clone() {
            match step {
                ExecuteStep::Single { index } => {
                    let pass_name = self.passes[index].pass.name().to_string();
                    let pass_started_at = Instant::now();
                    let result = catch_unwind(AssertUnwindSafe(|| {
                        let mut graphics_ctx = GraphicsRecordContext::new(&mut ctx, None);
                        self.passes[index].pass.record(&mut graphics_ctx);
                    }));
                    let elapsed_ms = pass_started_at.elapsed().as_secs_f64() * 1000.0;
                    if !pass_timings.contains_key(&pass_name) {
                        pass_first_seen_order.push(pass_name.clone());
                    }
                    *pass_timings.entry(pass_name.clone()).or_insert(0.0) += elapsed_ms;
                    *pass_counts.entry(pass_name.clone()).or_insert(0) += 1;
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
                            pass_name, detail
                        );
                    }
                }
                ExecuteStep::SharedRun { start, end } => {
                    self.execute_shared_run(
                        start,
                        end,
                        &mut ctx,
                        &mut pass_timings,
                        &mut pass_counts,
                        &mut pass_first_seen_order,
                    );
                }
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

impl FrameGraph {
    #[allow(clippy::too_many_arguments)]
    fn execute_shared_run(
        &mut self,
        start: usize,
        end: usize,
        ctx: &mut RecordContext<'_, '_>,
        pass_timings: &mut HashMap<String, f64>,
        pass_counts: &mut HashMap<String, usize>,
        pass_first_seen_order: &mut Vec<String>,
    ) {
        let Some(first_index) = self.order.get(start).copied() else {
            return;
        };
        let Some(key) = self.passes[first_index].pass.batch_key() else {
            for offset in start..end {
                let index = self.order[offset];
                let pass_name = self.passes[index].pass.name().to_string();
                let pass_started_at = Instant::now();
                let result = catch_unwind(AssertUnwindSafe(|| {
                    let mut graphics_ctx = GraphicsRecordContext::new(ctx, None);
                    self.passes[index].pass.record(&mut graphics_ctx);
                }));
                let elapsed_ms = pass_started_at.elapsed().as_secs_f64() * 1000.0;
                if !pass_timings.contains_key(&pass_name) {
                    pass_first_seen_order.push(pass_name.clone());
                }
                *pass_timings.entry(pass_name.clone()).or_insert(0.0) += elapsed_ms;
                *pass_counts.entry(pass_name.clone()).or_insert(0) += 1;
                if result.is_err() {
                    eprintln!("[warn] render pass panicked and was skipped: {}", pass_name);
                }
            }
            return;
        };

        let (offscreen_view, offscreen_msaa_view) = match key.color_target {
            Some(handle) => (
                render_target_view(ctx, handle),
                render_target_msaa_view(ctx, handle),
            ),
            None => (None, None),
        };
        let msaa_enabled = ctx.viewport.msaa_sample_count() > 1;
        let (encoder_ptr, surface_view, surface_resolve_view, depth_view) = {
            let Some(parts) = ctx.viewport.frame_parts() else {
                return;
            };
            (
                parts.encoder as *mut wgpu::CommandEncoder,
                parts.view.clone(),
                parts.resolve_view.cloned(),
                parts.depth_view.cloned(),
            )
        };
        let surface_resolve = if msaa_enabled {
            surface_resolve_view.as_ref()
        } else {
            None
        };
        let (color_view, resolve_target) =
            match (offscreen_view.as_ref(), offscreen_msaa_view.as_ref()) {
                (Some(resolve_view), Some(msaa_view)) => (msaa_view, Some(resolve_view)),
                (Some(resolve_view), None) => (resolve_view, None),
                (None, _) => (&surface_view, surface_resolve),
            };
        let depth_attachment =
            depth_view
                .as_ref()
                .map(|view| wgpu::RenderPassDepthStencilAttachment {
                    view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                });
        let encoder = unsafe { &mut *encoder_ptr };
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("FrameGraph ShareRun"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color_view,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
                resolve_target,
            })],
            depth_stencil_attachment: depth_attachment,
            ..Default::default()
        });

        for offset in start..end {
            let index = self.order[offset];
            let pass_name = self.passes[index].pass.name().to_string();
            let pass_started_at = Instant::now();
            let mut graphics_ctx = GraphicsRecordContext::new(ctx, Some(&mut render_pass));
            self.passes[index].pass.record(&mut graphics_ctx);
            let elapsed_ms = pass_started_at.elapsed().as_secs_f64() * 1000.0;
            if !pass_timings.contains_key(&pass_name) {
                pass_first_seen_order.push(pass_name.clone());
            }
            *pass_timings.entry(pass_name.clone()).or_insert(0.0) += elapsed_ms;
            *pass_counts.entry(pass_name).or_insert(0) += 1;
        }
    }
}

fn summarize_pass_resources(
    descriptor: &PassDescriptor,
    usages: &[PassResourceUsage],
) -> Result<HashMap<ResourceHandle, PassResourceSummary>, FrameGraphError> {
    let mut summaries = HashMap::new();
    for usage in usages {
        let entry = summaries.entry(usage.resource).or_insert(PassResourceSummary {
            requires_input: false,
            writes: false,
            produced: false,
        });
        match usage.usage {
            ResourceUsage::Produced => {
                entry.produced = true;
                entry.writes = true;
            }
            ResourceUsage::SampledRead
            | ResourceUsage::DepthRead
            | ResourceUsage::StencilRead
            | ResourceUsage::CopySrc
            | ResourceUsage::UniformRead
            | ResourceUsage::VertexRead
            | ResourceUsage::IndexRead
            | ResourceUsage::StorageRead => {
                entry.requires_input = !entry.produced;
            }
            ResourceUsage::ColorAttachmentWrite => {
                entry.writes = true;
                if color_attachment_requires_input(descriptor, usage.resource) && !entry.produced {
                    entry.requires_input = true;
                }
            }
            ResourceUsage::DepthWrite => {
                entry.writes = true;
                if depth_stencil_aspect_requires_input(descriptor, usage.resource, true) && !entry.produced {
                    entry.requires_input = true;
                }
            }
            ResourceUsage::StencilWrite => {
                entry.writes = true;
                if depth_stencil_aspect_requires_input(descriptor, usage.resource, false) && !entry.produced {
                    entry.requires_input = true;
                }
            }
            ResourceUsage::CopyDst | ResourceUsage::StorageWrite => {
                entry.writes = true;
            }
        }
    }
    Ok(summaries)
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

fn summary_to_access(summary: PassResourceSummary) -> Option<ResourceAccess> {
    match (summary.requires_input, summary.writes) {
        (false, false) => None,
        (true, false) => Some(ResourceAccess::Read),
        (false, true) => Some(ResourceAccess::Write),
        (true, true) => Some(ResourceAccess::Modify),
    }
}

fn find_latest_writer_before(
    pass_summaries: &[HashMap<ResourceHandle, PassResourceSummary>],
    resource: ResourceHandle,
    pass_index: usize,
) -> Option<usize> {
    (0..pass_index).rev().find(|candidate| {
        pass_summaries[*candidate]
            .get(&resource)
            .is_some_and(|summary| summary.writes || summary.produced)
    })
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
    signatures: &[Option<RenderPassBatchKey>],
    last_signature: Option<RenderPassBatchKey>,
) -> usize {
    if let Some(last_signature) = last_signature {
        let mut best: Option<usize> = None;
        for &idx in queue {
            if signatures[idx]
                .is_some_and(|signature| batch_keys_compatible(last_signature, signature))
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

    let mut best_signature: Option<(RenderPassBatchKey, usize)> = None;
    let unique_signatures: HashSet<RenderPassBatchKey> =
        queue.iter().filter_map(|&idx| signatures[idx]).collect();
    for signature in unique_signatures {
        let count = queue
            .iter()
            .filter_map(|&idx| signatures[idx])
            .filter(|&candidate| batch_keys_compatible(signature, candidate))
            .count();
        if best_signature.is_none_or(|(_, best_count)| count > best_count) {
            best_signature = Some((signature, count));
        }
    }

    if let Some((target_signature, count)) = best_signature
        && count > 1
    {
        let mut best: Option<usize> = None;
        for &idx in queue {
            if signatures[idx]
                .is_some_and(|signature| batch_keys_compatible(target_signature, signature))
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

    queue.iter().copied().min().unwrap_or(0)
}

fn is_rect_pass_name(name: &str) -> bool {
    name == std::any::type_name::<DrawRectPass>() || name == std::any::type_name::<OpaqueRectPass>()
}

fn batch_keys_compatible(current: RenderPassBatchKey, next: RenderPassBatchKey) -> bool {
    current.color_target == next.color_target
}

fn batch_trace_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("RFGUI_TRACE_BATCH")
            .ok()
            .is_some_and(|value| value == "1")
    })
}

pub trait FrameResourceContext {
    fn viewport(&mut self) -> &mut Viewport;
    fn textures(&self) -> &[TextureDesc];
    fn buffers(&self) -> &[BufferDesc];

    fn buffer_desc(&self, handle: BufferHandle) -> Option<BufferDesc> {
        self.buffers().get(handle.0 as usize).copied()
    }

    fn acquire_buffer(&mut self, handle: BufferHandle) -> Option<wgpu::Buffer> {
        let desc = self.buffer_desc(handle)?;
        self.viewport().acquire_frame_buffer(handle, desc)
    }
}

pub struct PrepareContext<'a, 'b> {
    pub(crate) viewport: &'a mut Viewport,
    pub(crate) textures: &'b [TextureDesc],
    pub(crate) buffers: &'b [BufferDesc],
}

impl<'a, 'b> PrepareContext<'a, 'b> {
    pub(crate) fn new(
        viewport: &'a mut Viewport,
        textures: &'b [TextureDesc],
        buffers: &'b [BufferDesc],
    ) -> Self {
        Self {
            viewport,
            textures,
            buffers,
        }
    }

    pub fn upload_buffer(&mut self, handle: BufferHandle, offset: u64, data: &[u8]) -> bool {
        let Some(desc) = self.buffer_desc(handle) else {
            return false;
        };
        self.viewport
            .upload_frame_buffer(handle, desc, offset, data)
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
}

pub struct RecordContext<'a, 'b> {
    pub(crate) viewport: &'a mut Viewport,
    pub(crate) textures: &'b [TextureDesc],
    pub(crate) buffers: &'b [BufferDesc],
    detail_timings: HashMap<String, f64>,
    detail_counts: HashMap<String, usize>,
    detail_order: Vec<String>,
}

impl<'a, 'b> RecordContext<'a, 'b> {
    pub(crate) fn new(
        viewport: &'a mut Viewport,
        textures: &'b [TextureDesc],
        buffers: &'b [BufferDesc],
    ) -> Self {
        Self {
            viewport,
            textures,
            buffers,
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
}

pub struct GraphicsRecordContext<'ctx, 'rp, 'res> {
    pub(crate) viewport: &'ctx mut Viewport,
    pub(crate) textures: &'res [TextureDesc],
    pub(crate) buffers: &'res [BufferDesc],
    detail_timings: &'ctx mut HashMap<String, f64>,
    detail_counts: &'ctx mut HashMap<String, usize>,
    detail_order: &'ctx mut Vec<String>,
    render_pass: Option<&'ctx mut wgpu::RenderPass<'rp>>,
}

impl<'ctx, 'rp, 'res> GraphicsRecordContext<'ctx, 'rp, 'res> {
    pub(crate) fn new(
        record: &'ctx mut RecordContext<'_, 'res>,
        render_pass: Option<&'ctx mut wgpu::RenderPass<'rp>>,
    ) -> Self {
        let RecordContext {
            viewport,
            textures,
            buffers,
            detail_timings,
            detail_counts,
            detail_order,
        } = record;
        Self {
            viewport,
            textures,
            buffers,
            detail_timings,
            detail_counts,
            detail_order,
            render_pass,
        }
    }

    pub fn active_render_pass(&mut self) -> Option<&mut wgpu::RenderPass<'rp>> {
        self.render_pass.as_deref_mut()
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

impl FrameResourceContext for GraphicsRecordContext<'_, '_, '_> {
    fn viewport(&mut self) -> &mut Viewport {
        self.viewport
    }

    fn textures(&self) -> &[TextureDesc] {
        self.textures
    }

    fn buffers(&self) -> &[BufferDesc] {
        self.buffers
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
    use crate::view::render_pass::present_surface_pass::{
        PresentSurfaceInput, PresentSurfaceOutput, PresentSurfaceParams, PresentSurfacePass,
    };
    use crate::view::render_pass::draw_rect_pass::RenderTargetIn;
    use crate::view::render_pass::RenderPass;

    #[derive(Default)]
    struct WritePass {
        output: OutSlot<TextureResource, ()>,
    }

    impl RenderPass for WritePass {
        fn setup(&mut self, builder: &mut PassBuilder<'_>) {
            let target = builder
                .texture_target(&self.output)
                .expect("test output should have texture target");
            builder.declare_color_attachment(
                &self.output,
                GraphicsColorAttachmentDescriptor::clear(target, [0.0, 0.0, 0.0, 0.0]),
            );
        }

        fn record(&mut self, _ctx: &mut GraphicsRecordContext<'_, '_, '_>) {}
    }

    #[derive(Default)]
    struct ReadPass {
        input: InSlot<TextureResource, ()>,
    }

    impl RenderPass for ReadPass {
        fn setup(&mut self, builder: &mut PassBuilder<'_>) {
            if let Some(handle) = self.input.handle() {
                builder.declare_sampled_texture(&mut self.input, &OutSlot::with_handle(handle));
            }
        }

        fn record(&mut self, _ctx: &mut GraphicsRecordContext<'_, '_, '_>) {}
    }

    #[derive(Default)]
    struct ModifyPass {
        target: OutSlot<TextureResource, ()>,
    }

    impl RenderPass for ModifyPass {
        fn setup(&mut self, builder: &mut PassBuilder<'_>) {
            let target = builder
                .texture_target(&self.target)
                .expect("test output should have texture target");
            builder.declare_color_attachment(
                &self.target,
                GraphicsColorAttachmentDescriptor::load(target),
            );
        }

        fn record(&mut self, _ctx: &mut GraphicsRecordContext<'_, '_, '_>) {}
    }

    fn test_texture_desc() -> TextureDesc {
        TextureDesc::new(1, 1, wgpu::TextureFormat::Rgba8Unorm, wgpu::TextureDimension::D2)
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
        let writer = graph.add_pass(WritePass {
            output: texture.clone(),
        });
        let reader = graph.add_pass(ReadPass {
            input: InSlot::with_handle(
                texture.handle().expect("declared texture should have handle"),
            ),
        });

        graph.compile().expect("compile should succeed");

        assert_eq!(graph.order, vec![writer.0, reader.0]);
    }

    #[test]
    fn compile_orders_modify_chain_from_usage() {
        let mut graph = FrameGraph::new();
        let texture = graph.declare_texture::<()>(test_texture_desc());
        let writer = graph.add_pass(WritePass {
            output: texture.clone(),
        });
        let modify_a = graph.add_pass(ModifyPass {
            target: texture.clone(),
        });
        let modify_b = graph.add_pass(ModifyPass { target: texture });

        graph.compile().expect("compile should succeed");

        assert_eq!(graph.order, vec![writer.0, modify_a.0, modify_b.0]);
    }

    #[test]
    fn compile_captures_graphics_pass_descriptor() {
        let mut graph = FrameGraph::new();
        let texture = graph.declare_texture::<()>(test_texture_desc());
        graph.add_pass(WritePass { output: texture });

        graph.compile().expect("compile should succeed");

        let descriptors = graph.pass_descriptors();
        assert_eq!(descriptors.len(), 1);
        assert_eq!(descriptors[0].kind, PassKind::Graphics);
        let PassDetails::Graphics(graphics) = &descriptors[0].details else {
            panic!("expected graphics pass details");
        };
        assert_eq!(graphics.color_attachments.len(), 1);
        assert_eq!(graphics.color_attachments[0].load_op, AttachmentLoadOp::Clear);
    }

    #[test]
    fn compile_culls_passes_outside_present_chain() {
        let mut graph = FrameGraph::new();
        let live_texture = graph.declare_texture::<()>(test_texture_desc());
        let dead_texture = graph.declare_texture::<()>(test_texture_desc());
        let live_writer = graph.add_pass(WritePass {
            output: live_texture.clone(),
        });
        let dead_writer = graph.add_pass(WritePass {
            output: dead_texture,
        });
        let present = graph.add_pass(make_present_pass(&live_texture));

        graph.compile().expect("compile should succeed");

        let compiled = graph.compiled_graph().expect("compiled graph should exist");
        assert_eq!(compiled.execution_plan.ordered_passes, vec![live_writer.0, present.0]);
        assert!(compiled.culled_passes.contains(&dead_writer.0));
    }

    #[test]
    fn compile_rejects_multiple_live_writers_on_same_resource() {
        let mut graph = FrameGraph::new();
        let texture = graph.declare_texture::<()>(test_texture_desc());
        graph.add_pass(WritePass {
            output: texture.clone(),
        });
        graph.add_pass(WritePass {
            output: texture,
        });

        let err = graph.compile().expect_err("compile should reject multiple writers");
        assert!(matches!(err, FrameGraphError::MultipleWriters));
    }

    #[test]
    fn compile_rejects_read_without_producer() {
        let mut graph = FrameGraph::new();
        let texture = graph.declare_texture::<()>(test_texture_desc());
        graph.add_pass(ReadPass {
            input: InSlot::with_handle(
                texture.handle().expect("declared texture should have handle"),
            ),
        });

        let err = graph.compile().expect_err("compile should fail");
        assert!(matches!(err, FrameGraphError::MissingInput(_)));
    }
}
