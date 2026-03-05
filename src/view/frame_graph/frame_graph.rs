use std::collections::{HashMap, HashSet, VecDeque};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::Instant;

use super::buffer_resource::{BufferDesc, BufferHandle};
use super::dependency_resource::DepHandle;
use super::texture_resource::{TextureDesc, TextureHandle};
use crate::view::render_pass::draw_rect_pass::{DrawRectPass, OpaqueRectPass};
use crate::view::render_pass::text_pass::TextPass;
use crate::view::render_pass::{PassWrapper, RenderPass, RenderPassDyn};
use crate::view::viewport::Viewport;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResourceHandle {
    Texture(TextureHandle),
    Buffer(BufferHandle),
    Dep(DepHandle),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PassHandle(usize);

struct PassNode {
    pass: Box<dyn RenderPassDyn>,
    reads: Vec<ResourceHandle>,
    writes: Vec<ResourceHandle>,
}

pub struct FrameGraph {
    passes: Vec<PassNode>,
    textures: Vec<TextureDesc>,
    buffers: Vec<BufferDesc>,
    dep_count: u32,
    order: Vec<usize>,
    compiled: bool,
    build_errors: Vec<FrameGraphError>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct BatchSignature {
    pass_name: &'static str,
    key: u64,
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
            dep_count: 0,
            order: Vec::new(),
            compiled: false,
            build_errors: Vec::new(),
        }
    }

    pub fn add_pass<P: RenderPass + 'static>(&mut self, pass: P) -> PassHandle {
        let node = PassNode {
            pass: Box::new(PassWrapper { pass }),
            reads: Vec::new(),
            writes: Vec::new(),
        };
        let handle = PassHandle(self.passes.len());
        self.passes.push(node);
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

    pub fn declare_dep_token(&mut self) -> DepHandle {
        let handle = DepHandle(self.dep_count);
        self.dep_count = self.dep_count.saturating_add(1);
        handle
    }

    pub fn compile(&mut self) -> Result<(), FrameGraphError> {
        self.order.clear();
        self.compiled = false;

        for node in &mut self.passes {
            node.reads.clear();
            node.writes.clear();
        }

        let mut textures = std::mem::take(&mut self.textures);
        let mut buffers = std::mem::take(&mut self.buffers);
        let mut build_errors: Vec<FrameGraphError> = Vec::new();

        for node in &mut self.passes {
            let mut builder = super::builder::BuildContext {
                textures: &mut textures,
                buffers: &mut buffers,
                reads: &mut node.reads,
                writes: &mut node.writes,
                build_errors: &mut build_errors,
            };
            node.pass.build(&mut builder);
        }

        self.textures = textures;
        self.buffers = buffers;
        self.build_errors = build_errors;

        if let Some(err) = self.build_errors.pop() {
            return Err(err);
        }

        let mut single_writer_map: HashMap<ResourceHandle, usize> = HashMap::new();
        let mut texture_writers: HashMap<ResourceHandle, Vec<usize>> = HashMap::new();
        for (index, node) in self.passes.iter().enumerate() {
            for &handle in &node.writes {
                match handle {
                    ResourceHandle::Texture(_) => {
                        texture_writers.entry(handle).or_default().push(index);
                    }
                    _ => {
                        if single_writer_map.insert(handle, index).is_some() {
                            return Err(FrameGraphError::MultipleWriters);
                        }
                    }
                }
            }
        }

        let mut indegree = vec![0usize; self.passes.len()];
        let mut graph_edges: Vec<HashSet<usize>> = vec![HashSet::new(); self.passes.len()];

        for (index, node) in self.passes.iter().enumerate() {
            for &handle in &node.reads {
                match handle {
                    ResourceHandle::Texture(_) => {
                        let Some(writers) = texture_writers.get(&handle) else {
                            return Err(FrameGraphError::MissingInput("resource has no writer"));
                        };
                        let mut has_prior_writer = false;
                        for &writer in writers {
                            if writer < index && graph_edges[writer].insert(index) {
                                indegree[index] += 1;
                                has_prior_writer = true;
                            }
                        }
                        if !has_prior_writer
                            && !writers.iter().copied().any(|writer| writer == index)
                        {
                            return Err(FrameGraphError::MissingInput("resource has no prior writer"));
                        }
                    }
                    _ => {
                        let Some(&writer) = single_writer_map.get(&handle) else {
                            return Err(FrameGraphError::MissingInput("resource has no writer"));
                        };
                        if writer != index && graph_edges[writer].insert(index) {
                            indegree[index] += 1;
                        }
                    }
                }
            }
        }

        let mut queue: VecDeque<usize> = indegree
            .iter()
            .enumerate()
            .filter_map(|(idx, &deg)| if deg == 0 { Some(idx) } else { None })
            .collect();

        let batch_signatures: Vec<Option<BatchSignature>> = self
            .passes
            .iter()
            .map(|node| {
                if !node.pass.batchable() {
                    return None;
                }
                node.pass.get_batch_key().map(|key| BatchSignature {
                    pass_name: node.pass.name(),
                    key,
                })
            })
            .collect();
        let mut last_signature: Option<BatchSignature> = None;

        while !queue.is_empty() {
            let n = select_next_ready_node(&queue, &batch_signatures, last_signature);
            let n = remove_from_queue(&mut queue, n);
            self.order.push(n);
            last_signature = batch_signatures[n];
            for &m in &graph_edges[n] {
                indegree[m] -= 1;
                if indegree[m] == 0 {
                    queue.push_back(m);
                }
            }
        }

        if self.order.len() != self.passes.len() {
            return Err(FrameGraphError::CyclicDependency);
        }

        self.compiled = true;
        Ok(())
    }

    pub fn compile_with_upload(&mut self, viewport: &mut Viewport) -> Result<(), FrameGraphError> {
        self.compile()?;
        let textures = self.textures.clone();
        let buffers = self.buffers.clone();
        let mut ctx = PassContext::new(viewport, &textures, &buffers);
        for &index in &self.order {
            self.passes[index].pass.compile_upload(&mut ctx);
        }
        Ok(())
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
                ResourceHandle::Dep(h) => format!("dep#{}", h.0),
            }
        }

        fn resource_node_id(handle: ResourceHandle) -> String {
            match handle {
                ResourceHandle::Texture(h) => format!("r_tex_{}", h.0),
                ResourceHandle::Buffer(h) => format!("r_buf_{}", h.0),
                ResourceHandle::Dep(h) => format!("r_dep_{}", h.0),
            }
        }

        fn resource_sort_key(handle: ResourceHandle) -> (u8, u32) {
            match handle {
                ResourceHandle::Texture(h) => (0, h.0),
                ResourceHandle::Buffer(h) => (1, h.0),
                ResourceHandle::Dep(h) => (2, h.0),
            }
        }

        let mut resources: HashSet<ResourceHandle> = HashSet::new();
        let mut write_edges: HashSet<(usize, ResourceHandle)> = HashSet::new();
        let mut read_edges: HashSet<(ResourceHandle, usize)> = HashSet::new();
        let mut dep_writers: HashMap<DepHandle, usize> = HashMap::new();

        for (index, node) in self.passes.iter().enumerate() {
            for &handle in &node.writes {
                resources.insert(handle);
                write_edges.insert((index, handle));
                if let ResourceHandle::Dep(dep) = handle {
                    dep_writers.insert(dep, index);
                }
            }
            for &handle in &node.reads {
                resources.insert(handle);
                read_edges.insert((handle, index));
            }
        }

        let mut execution_dep_edges: Vec<(usize, usize, DepHandle)> = Vec::new();
        for (index, node) in self.passes.iter().enumerate() {
            for &handle in &node.reads {
                let ResourceHandle::Dep(dep) = handle else {
                    continue;
                };
                let Some(&writer) = dep_writers.get(&dep) else {
                    continue;
                };
                if writer != index {
                    execution_dep_edges.push((writer, index, dep));
                }
            }
        }
        execution_dep_edges.sort_by_key(|(from, to, dep)| (*from, *to, dep.0));
        execution_dep_edges.dedup_by_key(|(from, to, dep)| (*from, *to, dep.0));

        let mut resource_nodes = resources.into_iter().collect::<Vec<_>>();
        resource_nodes.sort_by_key(|handle| resource_sort_key(*handle));
        let mut write_edges = write_edges.into_iter().collect::<Vec<_>>();
        write_edges.sort_by_key(|(from, handle)| (*from, resource_sort_key(*handle)));
        let mut read_edges = read_edges.into_iter().collect::<Vec<_>>();
        read_edges.sort_by_key(|(handle, to)| (resource_sort_key(*handle), *to));

        let mut dot = String::new();
        dot.push_str("digraph FrameGraph {\n");
        dot.push_str("  rankdir=LR;\n");
        dot.push_str("  graph [splines=true, ranksep=1.0, nodesep=0.35];\n");
        dot.push_str("  node [fontname=\"Helvetica\"];\n");
        dot.push_str("  edge [fontname=\"Helvetica\"];\n");
        dot.push_str("  node [shape=box, style=rounded];\n");
        for (index, node) in self.passes.iter().enumerate() {
            let label = escape_dot_label(node.pass.name());
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
        for (from, to, dep) in execution_dep_edges {
            dot.push_str(&format!(
                "  p{from} -> p{to} [color=\"gray\", fontcolor=\"gray\", label=\"dep#{}\", constraint=true];\n",
                dep.0
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
        let mut ctx = PassContext::new(viewport, &textures, &buffers);
        let mut cursor = 0usize;
        while cursor < self.order.len() {
            let index = self.order[cursor];
            let pass_name = self.passes[index].pass.name().to_string();
            let (batchable, batch_key) = {
                let pass = &mut self.passes[index].pass;
                (pass.batchable(), pass.get_batch_key())
            };

            if batchable && batch_key.is_some() {
                let mut end = cursor + 1;
                while end < self.order.len() {
                    let next_index = self.order[end];
                    let (next_name, next_batchable, next_key) = {
                        let pass = &mut self.passes[next_index].pass;
                        (pass.name(), pass.batchable(), pass.get_batch_key())
                    };
                    if !next_batchable || next_name != pass_name.as_str() || next_key != batch_key {
                        break;
                    }
                    end += 1;
                }

                let pass_started_at = Instant::now();
                let result = catch_unwind(AssertUnwindSafe(|| {
                    self.execute_batch_if_supported(cursor, end, &pass_name, &mut ctx)
                }));
                let elapsed_ms = pass_started_at.elapsed().as_secs_f64() * 1000.0;
                match result {
                    Ok(true) => {
                        if !pass_timings.contains_key(&pass_name) {
                            pass_first_seen_order.push(pass_name.clone());
                        }
                        *pass_timings.entry(pass_name.clone()).or_insert(0.0) += elapsed_ms;
                        *pass_counts.entry(pass_name).or_insert(0) += end - cursor;
                        cursor = end;
                        continue;
                    }
                    Ok(false) => {}
                    Err(payload) => {
                        if !pass_timings.contains_key(&pass_name) {
                            pass_first_seen_order.push(pass_name.clone());
                        }
                        *pass_timings.entry(pass_name.clone()).or_insert(0.0) += elapsed_ms;
                        *pass_counts.entry(pass_name.clone()).or_insert(0) += end - cursor;
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
                        cursor = end;
                        continue;
                    }
                }
            }

            let pass_started_at = Instant::now();
            let result = catch_unwind(AssertUnwindSafe(|| {
                self.passes[index].pass.execute(&mut ctx);
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
            cursor += 1;
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

    fn execute_batch_if_supported(
        &mut self,
        _start: usize,
        _end: usize,
        pass_name: &str,
        _ctx: &mut PassContext<'_, '_>,
    ) -> bool {
        if pass_name == std::any::type_name::<TextPass>() {
            return false;
        }
        if pass_name == std::any::type_name::<DrawRectPass>() {
            return false;
        }
        if pass_name == std::any::type_name::<OpaqueRectPass>() {
            return false;
        }
        false
    }
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
    signatures: &[Option<BatchSignature>],
    last_signature: Option<BatchSignature>,
) -> usize {
    if let Some(last_signature) = last_signature {
        let mut best: Option<usize> = None;
        for &idx in queue {
            if signatures[idx] == Some(last_signature) {
                if best.is_none_or(|current| idx < current) {
                    best = Some(idx);
                }
            }
        }
        if let Some(best) = best {
            return best;
        }
    }

    let mut grouped_counts: HashMap<BatchSignature, usize> = HashMap::new();
    for &idx in queue {
        if let Some(signature) = signatures[idx] {
            *grouped_counts.entry(signature).or_insert(0) += 1;
        }
    }

    let mut best_signature: Option<(BatchSignature, usize)> = None;
    for (signature, count) in grouped_counts {
        if best_signature.is_none_or(|(_, best_count)| count > best_count) {
            best_signature = Some((signature, count));
        }
    }

    if let Some((target_signature, count)) = best_signature
        && count > 1
    {
        let mut best: Option<usize> = None;
        for &idx in queue {
            if signatures[idx] == Some(target_signature) {
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

pub struct PassContext<'a, 'b> {
    pub(crate) viewport: &'a mut Viewport,
    pub(crate) textures: &'b [TextureDesc],
    pub(crate) buffers: &'b [BufferDesc],
    detail_timings: HashMap<String, f64>,
    detail_counts: HashMap<String, usize>,
    detail_order: Vec<String>,
}

impl<'a, 'b> PassContext<'a, 'b> {
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

    pub fn buffer_desc(&self, handle: BufferHandle) -> Option<BufferDesc> {
        self.buffers.get(handle.0 as usize).copied()
    }

    pub fn acquire_buffer(&mut self, handle: BufferHandle) -> Option<wgpu::Buffer> {
        let desc = self.buffer_desc(handle)?;
        self.viewport.acquire_frame_buffer(handle, desc)
    }

    pub fn upload_buffer(&mut self, handle: BufferHandle, offset: u64, data: &[u8]) -> bool {
        let Some(desc) = self.buffer_desc(handle) else {
            return false;
        };
        self.viewport
            .upload_frame_buffer(handle, desc, offset, data)
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

#[derive(Debug)]
pub enum FrameGraphError {
    MissingInput(&'static str),
    MissingOutput(&'static str),
    MultipleWriters,
    CyclicDependency,
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
