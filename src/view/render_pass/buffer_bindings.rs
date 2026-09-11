//! Exact buffer state for one wgpu render pass, shared by its logical passes.
//!
//! wgpu 30 already suppresses identical pipeline/bind-group commands, but
//! setting a vertex buffer always marks its slot dirty, even if unchanged.
//! Keep bind groups (including every dynamic-offset change) on the wgpu path.
//! This state owns handles, not address/hash proxies, and never survives a
//! render pass. Pipeline changes preserve vertex/index bindings in wgpu.

#[derive(PartialEq, Eq)]
struct BufferRange {
    buffer: wgpu::Buffer,
    offset: u64,
    size: u64,
}

impl BufferRange {
    fn matches(&self, slice: wgpu::BufferSlice<'_>) -> bool {
        self.buffer == *slice.buffer() && self.offset == slice.offset() && self.size == slice.size()
    }

    fn from_slice(slice: wgpu::BufferSlice<'_>) -> Self {
        Self {
            buffer: slice.buffer().clone(),
            offset: slice.offset(),
            size: slice.size(),
        }
    }
}

#[derive(Default)]
pub(crate) struct GraphicsBufferBindings {
    // Cache the eight standard slots without allocating per draw. Higher
    // slots still go to wgpu, which validates them against the device limit.
    vertex: [Option<BufferRange>; 8],
    index: Option<(BufferRange, wgpu::IndexFormat)>,
    #[cfg(test)]
    counts: [[usize; 2]; 2],
}

impl GraphicsBufferBindings {
    pub(crate) fn set_vertex_buffer(
        &mut self,
        pass: &mut wgpu::RenderPass<'_>,
        slot: u32,
        slice: wgpu::BufferSlice<'_>,
    ) -> bool {
        let cached = self.vertex.get_mut(slot as usize);
        let unchanged = cached
            .as_ref()
            .is_some_and(|entry| entry.as_ref().is_some_and(|entry| entry.matches(slice)));
        // Empty slices must still take the wgpu validation path.
        let emit = !unchanged || slice.size() == 0;
        if emit {
            pass.set_vertex_buffer(slot, slice);
            if let Some(entry) = cached {
                *entry = Some(BufferRange::from_slice(slice));
            }
        }
        #[cfg(test)]
        self.record_binding_for_test(0, emit);
        emit
    }

    pub(crate) fn set_index_buffer(
        &mut self,
        pass: &mut wgpu::RenderPass<'_>,
        slice: wgpu::BufferSlice<'_>,
        format: wgpu::IndexFormat,
    ) -> bool {
        let unchanged = self
            .index
            .as_ref()
            .is_some_and(|(entry, previous)| entry.matches(slice) && *previous == format);
        let emit = !unchanged || slice.size() == 0;
        if emit {
            pass.set_index_buffer(slice, format);
            self.index = Some((BufferRange::from_slice(slice), format));
        }
        #[cfg(test)]
        self.record_binding_for_test(1, emit);
        emit
    }
}

// Per-thread evidence counts actual calls at the command boundary, never
// process-global totals that parallel tests could change underneath a gate.
#[cfg(test)]
std::thread_local! {
    static COUNTS: std::cell::RefCell<Vec<[[usize; 2]; 2]>> = const {
        std::cell::RefCell::new(Vec::new())
    };
}

#[cfg(test)]
impl GraphicsBufferBindings {
    fn record_binding_for_test(&mut self, kind: usize, emitted: bool) {
        self.counts[kind][0] += 1;
        self.counts[kind][1] += usize::from(emitted);
    }
}

#[cfg(test)]
impl Drop for GraphicsBufferBindings {
    fn drop(&mut self) {
        if self.counts != [[0; 2]; 2] {
            COUNTS.with(|counts| counts.borrow_mut().push(self.counts));
        }
    }
}

#[cfg(test)]
pub(crate) fn take_counts_for_test() -> Vec<[[usize; 2]; 2]> {
    COUNTS.with(|counts| std::mem::take(&mut *counts.borrow_mut()))
}

#[cfg(test)]
mod tests;
