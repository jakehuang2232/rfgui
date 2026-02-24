use std::any::TypeId;
use std::collections::{HashMap, HashSet, VecDeque};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::Instant;

use super::buffer_resource::{BufferDesc, BufferHandle};
use super::texture_resource::{TextureDesc, TextureHandle};
use crate::view::render_pass::text_pass::{TextPass, execute_text_pass_batch};
use crate::view::render_pass::{PassWrapper, RenderPass, RenderPassDyn};
use crate::view::viewport::Viewport;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResourceHandle {
    Texture(TextureHandle),
    Buffer(BufferHandle),
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
    order: Vec<usize>,
    compiled: bool,
    build_errors: Vec<FrameGraphError>,
    cache: ResourceCache,
}

#[derive(Clone, Debug, Default)]
pub struct ExecuteProfile {
    pub total_ms: f64,
    pub pass_count: usize,
    pub top_passes: Vec<(String, f64)>,
}

impl FrameGraph {
    pub fn new() -> Self {
        Self {
            passes: Vec::new(),
            textures: Vec::new(),
            buffers: Vec::new(),
            order: Vec::new(),
            compiled: false,
            build_errors: Vec::new(),
            cache: ResourceCache::new(),
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

        let mut writer_map: HashMap<ResourceHandle, usize> = HashMap::new();
        for (index, node) in self.passes.iter().enumerate() {
            for &handle in &node.writes {
                if writer_map.insert(handle, index).is_some() {
                    return Err(FrameGraphError::MultipleWriters);
                }
            }
        }

        let mut indegree = vec![0usize; self.passes.len()];
        let mut graph_edges: Vec<HashSet<usize>> = vec![HashSet::new(); self.passes.len()];

        for (index, node) in self.passes.iter().enumerate() {
            for &handle in &node.reads {
                let Some(&writer) = writer_map.get(&handle) else {
                    return Err(FrameGraphError::MissingInput("resource has no writer"));
                };
                if writer != index && graph_edges[writer].insert(index) {
                    indegree[index] += 1;
                }
            }
        }

        let mut queue: VecDeque<usize> = indegree
            .iter()
            .enumerate()
            .filter_map(|(idx, &deg)| if deg == 0 { Some(idx) } else { None })
            .collect();

        while let Some(n) = queue.pop_front() {
            self.order.push(n);
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

    pub fn execute(&mut self, viewport: &mut Viewport) -> Result<(), FrameGraphError> {
        let _ = self.execute_profiled(viewport)?;
        Ok(())
    }

    pub fn execute_profiled(
        &mut self,
        viewport: &mut Viewport,
    ) -> Result<ExecuteProfile, FrameGraphError> {
        if !self.compiled {
            return Err(FrameGraphError::NotCompiled);
        }
        let execute_started_at = Instant::now();
        let mut pass_timings: HashMap<String, f64> = HashMap::new();
        let mut ctx = PassContext::new(viewport, &self.textures, &self.buffers, &mut self.cache);
        let mut cursor = 0usize;
        while cursor < self.order.len() {
            let index = self.order[cursor];
            let batch_key = self.passes[index]
                .pass
                .as_any_mut()
                .downcast_mut::<TextPass>()
                .map(|pass| pass.batch_key());

            if let Some(batch_key) = batch_key {
                let mut end = cursor + 1;
                while end < self.order.len() {
                    let next_index = self.order[end];
                    let next_key = self.passes[next_index]
                        .pass
                        .as_any_mut()
                        .downcast_mut::<TextPass>()
                        .map(|pass| pass.batch_key());
                    if next_key != Some(batch_key) {
                        break;
                    }
                    end += 1;
                }

                let pass_started_at = Instant::now();
                let result = catch_unwind(AssertUnwindSafe(|| {
                    let mut batch = Vec::with_capacity(end - cursor);
                    for pos in cursor..end {
                        let pass_index = self.order[pos];
                        if let Some(text_pass) = self.passes[pass_index]
                            .pass
                            .as_any_mut()
                            .downcast_mut::<TextPass>()
                        {
                            batch.push(text_pass.snapshot_draw());
                        }
                    }
                    execute_text_pass_batch(batch, &mut ctx);
                }));
                let elapsed_ms = pass_started_at.elapsed().as_secs_f64() * 1000.0;
                *pass_timings
                    .entry(std::any::type_name::<TextPass>().to_string())
                    .or_insert(0.0) += elapsed_ms;
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
                        std::any::type_name::<TextPass>(),
                        detail
                    );
                }
                cursor = end;
                continue;
            }

            let pass_name = self.passes[index].pass.name();
            let pass_started_at = Instant::now();
            let result = catch_unwind(AssertUnwindSafe(|| {
                self.passes[index].pass.execute(&mut ctx);
            }));
            let elapsed_ms = pass_started_at.elapsed().as_secs_f64() * 1000.0;
            *pass_timings.entry(pass_name.to_string()).or_insert(0.0) += elapsed_ms;
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
        let mut top_passes: Vec<(String, f64)> = pass_timings.into_iter().collect();
        top_passes.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        if top_passes.len() > 6 {
            top_passes.truncate(6);
        }
        Ok(ExecuteProfile {
            total_ms: execute_started_at.elapsed().as_secs_f64() * 1000.0,
            pass_count: self.order.len(),
            top_passes,
        })
    }
}

pub struct PassContext<'a, 'b> {
    pub viewport: &'a mut Viewport,
    pub textures: &'b [TextureDesc],
    pub buffers: &'b [BufferDesc],
    pub cache: &'b mut ResourceCache,
}

impl<'a, 'b> PassContext<'a, 'b> {
    pub fn new(
        viewport: &'a mut Viewport,
        textures: &'b [TextureDesc],
        buffers: &'b [BufferDesc],
        cache: &'b mut ResourceCache,
    ) -> Self {
        Self {
            viewport,
            textures,
            buffers,
            cache,
        }
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

pub struct ResourceCache {
    store: HashMap<(TypeId, u64), Box<dyn std::any::Any>>,
}

impl ResourceCache {
    fn new() -> Self {
        Self {
            store: HashMap::new(),
        }
    }

    pub fn get_or_insert_with<T: 'static, F: FnOnce() -> T>(
        &mut self,
        key: u64,
        create: F,
    ) -> &mut T {
        let entry_key = (TypeId::of::<T>(), key);
        if !self.store.contains_key(&entry_key) {
            self.store.insert(entry_key, Box::new(create()));
        }
        self.store
            .get_mut(&entry_key)
            .unwrap()
            .downcast_mut::<T>()
            .unwrap()
    }
}
