use super::*;
use crate::view::gpu_paint::{
    GpuPaintObservation, GpuPaintSource, GpuPaintWork, render::SourceResources,
};
use std::sync::Arc;

pub(super) struct CachedSource {
    source: GpuPaintSource,
    resources: Arc<SourceResources>,
    valid: bool,
    pending: bool,
    written: bool,
    work: Option<GpuPaintWork>,
}
pub(super) type RasterDiagnostic = (
    crate::view::frame_graph::PersistentTextureKey,
    crate::view::frame_graph::texture_resource::TextureDesc,
    crate::view::paint::RetainedSurfaceCompileAction,
);
impl Viewport {
    pub(crate) fn record_retained_raster_diagnostics(&mut self, entries: Vec<RasterDiagnostic>) {
        self.frame.retained_raster_diagnostics = entries;
    }
    // These are planner decisions. A parent Reuse or a clipped composite can
    // leave a candidate unexecuted; Reraster alone is not a GPU-write witness.
    pub(super) fn print_retained_raster_diagnostics(&self, frame: u64) {
        for (key, desc, action) in &self.frame.retained_raster_diagnostics {
            println!(
                "retained-raster frame={frame} key={key:?} extent={}x{} planned_action={action:?} resident={}",
                desc.width(),
                desc.height(),
                self.has_compatible_persistent_render_target(*key, desc)
            );
        }
    }

    pub(crate) fn prepare_gpu_paint_source(
        &mut self,
        source: &GpuPaintSource,
    ) -> (bool, Arc<SourceResources>) {
        // Prepare runs before this source's output attachment is acquired. A
        // stable key is insufficient: the physical backing must still exist.
        let resident =
            self.has_compatible_persistent_render_target(source.key(), &source.descriptor());
        if let Some(entry) = self.frame.gpu_paint_sources.get(&source.id.get()) {
            if entry.valid && resident && entry.source == *source {
                return (true, entry.resources.clone());
            }
        }
        let resources = self
            .frame
            .gpu_paint_sources
            .get(&source.id.get())
            .filter(|entry| entry.source.program == source.program)
            .map(|entry| entry.resources.clone())
            .unwrap_or_else(|| {
                Arc::new(SourceResources::new(
                    self.device().expect("active GPU frame"),
                    &source.program,
                ))
            });
        self.frame.gpu_paint_sources.insert(
            source.id.get(),
            CachedSource {
                source: source.clone(),
                resources: resources.clone(),
                valid: false,
                pending: true,
                written: false,
                work: None,
            },
        );
        (false, resources)
    }
    pub(crate) fn note_gpu_paint_source_written(&mut self, owner: u64) {
        if let Some(entry) = self.frame.gpu_paint_sources.get_mut(&owner) {
            entry.written = true;
            entry.work = Some(GpuPaintWork::Rendered);
        }
    }
    pub(crate) fn note_gpu_paint_source_reused(&mut self, id: u64) {
        if let Some(entry) = self.frame.gpu_paint_sources.get_mut(&id) {
            entry.work = Some(GpuPaintWork::Reused);
        }
    }
    /// Source GPU work, distinct from native raster actions and pool allocations.
    pub fn gpu_paint_observations(&self) -> Vec<GpuPaintObservation> {
        let mut result: Vec<_> = self
            .frame
            .gpu_paint_sources
            .values()
            .map(|entry| GpuPaintObservation {
                id: entry.source.id,
                revision: entry.source.revision,
                extent: entry.source.extent,
                work: entry.work,
                valid_resident: entry.valid
                    && self.has_compatible_persistent_render_target(
                        entry.source.key(),
                        &entry.source.descriptor(),
                    ),
            })
            .collect();
        result.sort_by_key(|entry| entry.id.get());
        result
    }
    pub(crate) fn finish_gpu_paint_frame(&mut self, submitted: bool) {
        let mut release = Vec::new();
        for entry in self.frame.gpu_paint_sources.values_mut() {
            if entry.pending {
                entry.valid = submitted && entry.written;
                if !entry.valid {
                    release.push(entry.source.key());
                }
            }
            entry.pending = false;
            entry.written = false;
        }
        for key in release {
            self.release_persistent_render_target_pair(key);
        }
    }
    pub(super) fn prune_gpu_paint_sources(&mut self) {
        self.frame.retained_raster_diagnostics.clear();
        if self.frame.gpu_paint_sources.is_empty() {
            return;
        }
        for entry in self.frame.gpu_paint_sources.values_mut() {
            entry.work = None;
        }
        let live: FxHashSet<u64> = self
            .scene
            .node_arena
            .iter()
            .filter_map(|(_, node)| node.element.prepared_gpu_paint_source().map(|s| s.id.get()))
            .collect();
        let mut release = Vec::new();
        self.frame.gpu_paint_sources.retain(|owner, entry| {
            if live.contains(owner) {
                true
            } else {
                release.push(entry.source.key());
                false
            }
        });
        for key in release {
            self.release_persistent_render_target_pair(key);
        }
    }
}
