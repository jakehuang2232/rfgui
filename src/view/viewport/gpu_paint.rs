use super::*;
use crate::view::gpu_paint::{
    GpuPaintObservation, GpuPaintSource, GpuPaintWork, render::SourceResources,
};
use std::sync::Arc;

/// A shadow template is valid once a submitted frame wrote it.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct ShadowTemplateResidency {
    valid: bool,
    written: bool,
}

pub(super) struct CachedSource {
    source: GpuPaintSource,
    resources: Arc<SourceResources>,
    valid: bool,
    pending: bool,
    written: bool,
    work: Option<GpuPaintWork>,
}
/// What the surface cache did with one surface or GPU source this frame.
/// Recorded only while the debug overlay, the render-time trace or an F9
/// report needs it.
#[derive(Clone, Debug)]
pub(crate) struct SurfaceDiagnostic {
    pub(crate) owner: crate::view::node_arena::NodeKey,
    pub(crate) kind: crate::view::debug::DebugSurfaceKind,
    pub(crate) action: crate::view::paint::RetainedSurfaceCompileAction,
    pub(crate) key: crate::view::frame_graph::PersistentTextureKey,
    pub(crate) desc: crate::view::frame_graph::texture_resource::TextureDesc,
    pub(crate) scale_factor: f32,
}

impl Viewport {
    pub(crate) fn surface_diagnostics_requested(&self) -> bool {
        self.debug_options.trace_render_time
            || self.debug_options.retained_auto_overlay
            || self.frame.retained_auto_report_pending
    }

    pub(crate) fn record_surface_diagnostics(&mut self, entries: Vec<SurfaceDiagnostic>) {
        self.frame.surface_diagnostics = entries;
    }

    // These are planner decisions. A parent Reuse or a clipped composite can
    // leave a candidate unexecuted; Reraster alone is not a GPU-write witness.
    pub(super) fn print_retained_raster_diagnostics(&self, frame: u64) {
        for entry in &self.frame.surface_diagnostics {
            println!(
                "retained-raster frame={frame} kind={:?} key={:?} extent={}x{} planned_action={:?} resident={}",
                entry.kind,
                entry.key,
                entry.desc.width(),
                entry.desc.height(),
                entry.action,
                self.has_compatible_persistent_render_target(entry.key, &entry.desc)
            );
        }
    }

    /// Whether `source`'s kept output is still valid and resident, so a
    /// retained source can be reused instead of produced.
    fn gpu_paint_source_reusable(&self, source: &GpuPaintSource) -> bool {
        // A stable key is insufficient: the physical backing must still exist.
        let resident = self.has_compatible_persistent_render_target(
            source.key(),
            &source.descriptor(self.max_texture_dimension_2d()),
        );
        self.frame
            .gpu_paint_sources
            .get(&source.id.get())
            .is_some_and(|entry| entry.valid && resident && entry.source == *source)
    }

    /// The action `prepare_gpu_paint_source` will take for `source`.
    pub(crate) fn planned_gpu_paint_source_action(
        &self,
        source: &GpuPaintSource,
        retain: bool,
    ) -> crate::view::paint::RetainedSurfaceCompileAction {
        use crate::view::paint::RetainedSurfaceCompileAction as Action;
        if !retain {
            Action::Transient
        } else if self.gpu_paint_source_reusable(source) {
            Action::Reuse
        } else {
            Action::Reraster
        }
    }

    /// Whether `source`'s kept output can be reused, and its pipeline. A
    /// source the cache does not `retain` this frame is always produced.
    pub(crate) fn prepare_gpu_paint_source(
        &mut self,
        source: &GpuPaintSource,
        retain: bool,
    ) -> (bool, Arc<SourceResources>) {
        // Prepare runs before this source's output attachment is acquired.
        if retain
            && self.gpu_paint_source_reusable(source)
            && let Some(entry) = self.frame.gpu_paint_sources.get(&source.id.get())
        {
            return (true, entry.resources.clone());
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
    /// A produced output becomes reusable only if it was `kept`; a frame-local
    /// output leaves the source to be produced again and frees its old backing.
    pub(crate) fn note_gpu_paint_source_written(&mut self, owner: u64, kept: bool) {
        if let Some(entry) = self.frame.gpu_paint_sources.get_mut(&owner) {
            entry.written = kept;
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
                        &entry.source.descriptor(self.max_texture_dimension_2d()),
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
    /// Valid templates whose backing is still resident. Evicted or
    /// incompatible entries, and writes whose frame never reported submit
    /// or abort, are forgotten and must be produced again.
    pub(crate) fn resident_shadow_templates(
        &mut self,
    ) -> FxHashSet<crate::view::render_pass::shadow_module::ShadowTemplateKey> {
        let mut stale = Vec::new();
        let mut resident = FxHashSet::default();
        for (key, state) in &self.frame.shadow_templates {
            let persistent = crate::view::frame_graph::PersistentTextureKey::ShadowTemplate(*key);
            if state.valid
                && !state.written
                && self.has_compatible_persistent_render_target(persistent, &key.texture_desc())
            {
                resident.insert(*key);
            } else {
                stale.push(*key);
            }
        }
        for key in stale {
            self.frame.shadow_templates.remove(&key);
            self.release_persistent_render_target_pair(
                crate::view::frame_graph::PersistentTextureKey::ShadowTemplate(key),
            );
        }
        resident
    }

    pub(crate) fn note_shadow_template_written(
        &mut self,
        key: crate::view::render_pass::shadow_module::ShadowTemplateKey,
    ) {
        self.frame.shadow_templates.entry(key).or_default().written = true;
    }

    pub(crate) fn finish_shadow_template_frame(&mut self, submitted: bool) {
        let mut release = Vec::new();
        self.frame.shadow_templates.retain(|key, state| {
            if std::mem::take(&mut state.written) {
                state.valid = submitted;
            }
            if !state.valid {
                release.push(*key);
            }
            state.valid
        });
        for key in release {
            self.release_persistent_render_target_pair(
                crate::view::frame_graph::PersistentTextureKey::ShadowTemplate(key),
            );
        }
    }

    pub(super) fn prune_gpu_paint_sources(&mut self) {
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
