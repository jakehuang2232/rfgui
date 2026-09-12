use super::*;
use std::sync::{Arc, Mutex};

/// An immutable successful span seal. Tests can mutate through copy-on-write;
/// that breaks allocation equality and forces the complete validator again.
#[derive(Clone)]
pub(super) struct SealedProgramSpan {
    value: Arc<ArtifactSurfaceRasterProgramSpanStamp>,
    validated: Arc<ArtifactSurfaceRasterProgramSpanStamp>,
    boundary_root: NodeKey,
}
impl std::ops::Deref for SealedProgramSpan {
    type Target = ArtifactSurfaceRasterProgramSpanStamp;
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}
#[cfg(test)]
impl std::ops::DerefMut for SealedProgramSpan {
    fn deref_mut(&mut self) -> &mut Self::Target {
        Arc::make_mut(&mut self.value)
    }
}
impl std::fmt::Debug for SealedProgramSpan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.value.fmt(f)
    }
}
impl PartialEq for SealedProgramSpan {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}
impl Eq for SealedProgramSpan {}
impl SealedProgramSpan {
    pub(super) fn is_canonical(&self, boundary: NodeKey, step: usize, start: u32) -> bool {
        if self.boundary_root == boundary
            && self.step_index == step
            && self.opaque_order_span.start == start
            && Arc::ptr_eq(&self.value, &self.validated)
        {
            true
        } else {
            artifact_surface_program_span_is_canonical(self, boundary, step, start)
        }
    }
    #[cfg(test)]
    pub(super) fn shares_seal(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.value, &other.value)
    }
}

struct Entry {
    chunks: Arc<[PreparedArtifactSurfaceRasterChunk]>,
    topology: Vec<PaintOwnerSnapshot>,
    clips: Vec<ClipNodeSnapshot>,
    opaque_count: u32,
    sealed: SealedProgramSpan,
}
#[derive(Default)]
pub(super) struct SpanSealCache(Mutex<Option<Entry>>);
impl std::fmt::Debug for SpanSealCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Cache population is not part of the prepared program's semantics.
        f.write_str("SpanSealCache")
    }
}
impl SpanSealCache {
    pub(super) fn seal(
        &self,
        surface: SurfaceDagNodeId,
        boundary: NodeKey,
        step: usize,
        start: u32,
        span: &PreparedArtifactSurfaceRasterSpan,
    ) -> Result<SealedProgramSpan, ArtifactSurfaceResidentSealError> {
        let mut memo = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(old) = memo.as_ref().filter(|old| {
            Arc::ptr_eq(&old.chunks, &span.chunks)
                && old.topology == span.owner_topology
                && old.clips == span.local_clips
                && old.opaque_count == span.opaque_order_count
                && old.sealed.boundary_root == boundary
                && old.sealed.step_index == step
                && old.sealed.opaque_order_span.start == start
        }) {
            crate::view::paint::work_profile::count("span_seal_replays", 1);
            return Ok(old.sealed.clone());
        }
        // The chunk allocation strongly owns current commands, revisions,
        // schedules and payloads. Any replacement/COW mutation misses; the
        // original sealer retains every live-value and topology check.
        let value = Arc::new(seal_artifact_surface_program_span(
            surface, boundary, step, start, span,
        )?);
        let sealed = SealedProgramSpan {
            value: value.clone(),
            validated: value,
            boundary_root: boundary,
        };
        *memo = Some(Entry {
            chunks: span.chunks.clone(),
            topology: span.owner_topology.clone(),
            clips: span.local_clips.clone(),
            opaque_count: span.opaque_order_count,
            sealed: sealed.clone(),
        });
        Ok(sealed)
    }
}
