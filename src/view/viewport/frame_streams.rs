//! Per-frame GPU buffers fed from CPU arrays that restart every frame.
//!
//! Preparation appends elements; after every pass has prepared, one flush
//! copies the new tail of each array into its buffer, so a frame uploads
//! each stream with one copy however many passes fed it.

#[cfg(not(target_arch = "wasm32"))]
use wgpu::util::StagingBelt;

/// Buffers unused for this many frames are released; an oversized buffer
/// that stayed below a quarter of its capacity this long is shrunk.
const MAX_IDLE_FRAMES: u64 = 120;

pub(super) struct FrameBufferEntry {
    pub(super) buffer: wgpu::Buffer,
    pub(super) size: u64,
    last_used_frame: u64,
    last_high_usage_frame: u64,
}

/// One growable buffer fed from a CPU array that restarts per frame.
pub(super) struct FrameStream<T> {
    pub(super) staged: Vec<T>,
    /// Leading elements of `staged` already copied into `buffer` this frame.
    pub(super) uploaded: usize,
    pub(super) buffer: Option<FrameBufferEntry>,
    label: &'static str,
    initial_capacity: u64,
    /// How the buffer is bound; `COPY_DST` is added for the uploads.
    usage: wgpu::BufferUsages,
}

impl<T: bytemuck::Pod> FrameStream<T> {
    pub(super) const STRIDE: u64 = std::mem::size_of::<T>() as u64;

    pub(super) fn new(
        label: &'static str,
        initial_capacity: u64,
        usage: wgpu::BufferUsages,
    ) -> Self {
        Self {
            staged: Vec::new(),
            uploaded: 0,
            buffer: None,
            label,
            initial_capacity,
            usage,
        }
    }

    pub(super) fn restart(&mut self) {
        self.staged.clear();
        self.uploaded = 0;
    }

    fn staged_bytes(&self) -> u64 {
        self.staged.len() as u64 * Self::STRIDE
    }

    fn create_buffer(&self, device: &wgpu::Device, size: u64, frame: u64) -> FrameBufferEntry {
        FrameBufferEntry {
            buffer: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(self.label),
                size,
                usage: self.usage | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            size,
            last_used_frame: frame,
            last_high_usage_frame: frame,
        }
    }

    /// Uploads the elements staged since the previous flush. Returns whether
    /// the buffer was replaced, or `None` when it cannot hold the frame.
    pub(super) fn flush(
        &mut self,
        device: &wgpu::Device,
        uploader: &mut FrameStreamUploader<'_>,
        frame: u64,
    ) -> Option<bool> {
        if self.uploaded == self.staged.len() {
            return Some(false);
        }
        let needed = self.staged_bytes();
        let current = self.buffer.as_ref().map_or(0, |entry| entry.size);
        let mut replaced = false;
        if needed > current {
            let limits = device.limits();
            // A storage stream binds the whole buffer; a uniform stream binds
            // one element at a time, at a dynamic offset.
            let max_size = if self.usage.contains(wgpu::BufferUsages::STORAGE) {
                limits
                    .max_storage_buffer_binding_size
                    .min(limits.max_buffer_size)
            } else {
                limits.max_buffer_size
            };
            if needed > max_size {
                return None;
            }
            let mut size = current.max(self.initial_capacity).max(Self::STRIDE);
            while size < needed {
                size = size.saturating_mul(2);
            }
            // Commands already recorded in this frame keep the old buffer
            // alive; destroying it here would invalidate them. Drop the handle
            // and re-upload the whole frame into the new buffer.
            self.buffer = Some(self.create_buffer(device, size.min(max_size), frame));
            self.uploaded = 0;
            replaced = true;
        }
        let entry = self.buffer.as_mut()?;
        let offset = self.uploaded as u64 * Self::STRIDE;
        uploader.write(
            &entry.buffer,
            offset,
            bytemuck::cast_slice(&self.staged[self.uploaded..]),
        );
        entry.last_used_frame = frame;
        if needed.saturating_mul(2) > entry.size {
            entry.last_high_usage_frame = frame;
        }
        self.uploaded = self.staged.len();
        Some(replaced)
    }

    /// The buffer for binding, created empty when nothing was uploaded yet.
    pub(super) fn ensure_buffer(&mut self, device: &wgpu::Device, frame: u64) -> wgpu::Buffer {
        if self.buffer.is_none() {
            self.buffer = Some(self.create_buffer(device, self.initial_capacity, frame));
        }
        self.buffer
            .as_ref()
            .map(|entry| entry.buffer.clone())
            .expect("frame buffer was just ensured")
    }

    /// Runs before the frame restarts, so `staged` still describes the
    /// previous frame's usage. Returns whether the buffer changed.
    pub(super) fn reclaim(&mut self, device: Option<&wgpu::Device>, frame: u64) -> bool {
        let Some(entry) = self.buffer.as_ref() else {
            return false;
        };
        if frame.saturating_sub(entry.last_used_frame) > MAX_IDLE_FRAMES {
            if let Some(entry) = self.buffer.take() {
                entry.buffer.destroy();
            }
            self.staged = Vec::new();
            return true;
        }
        let previous_usage = self.staged_bytes();
        let should_shrink = entry.size > self.initial_capacity
            && previous_usage.saturating_mul(4) <= entry.size
            && frame.saturating_sub(entry.last_high_usage_frame) > MAX_IDLE_FRAMES;
        let Some(device) = device.filter(|_| should_shrink) else {
            return false;
        };
        let new_size = previous_usage
            .max(1)
            .checked_next_power_of_two()
            .unwrap_or(u64::MAX)
            .max(self.initial_capacity);
        let replacement = self.create_buffer(device, new_size, frame);
        if let Some(old) = self.buffer.replace(replacement) {
            old.buffer.destroy();
        }
        self.staged
            .shrink_to((new_size / Self::STRIDE).try_into().unwrap_or(usize::MAX));
        true
    }

    pub(super) fn release(&mut self) {
        if let Some(entry) = self.buffer.take() {
            entry.buffer.destroy();
        }
        self.staged = Vec::new();
        self.uploaded = 0;
    }
}

/// Copies staged bytes into frame buffers at the current frame position.
pub(super) struct FrameStreamUploader<'a> {
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) staging_belt: &'a mut StagingBelt,
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) encoder: &'a mut wgpu::CommandEncoder,
    #[cfg(target_arch = "wasm32")]
    pub(super) queue: &'a wgpu::Queue,
}

impl FrameStreamUploader<'_> {
    fn write(&mut self, buffer: &wgpu::Buffer, offset: u64, bytes: &[u8]) {
        // On WebGPU, StagingBelt's async mapping may not resolve before the
        // next frame; queue writes have no mapping dependency.
        #[cfg(target_arch = "wasm32")]
        self.queue.write_buffer(buffer, offset, bytes);
        #[cfg(not(target_arch = "wasm32"))]
        {
            let size = wgpu::BufferSize::new(bytes.len() as u64).expect("nonempty frame upload");
            let mut mapped = self
                .staging_belt
                .write_buffer(self.encoder, buffer, offset, size);
            mapped.slice(..).copy_from_slice(bytes);
        }
    }
}
