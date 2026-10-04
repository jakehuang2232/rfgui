//! Per-frame rectangle instance and gradient-stop storage for [`Viewport`].
//!
//! Draw-rect preparation appends one [`RectInstance`] per rectangle, plus its
//! gradient stops, to CPU arrays that restart every frame. After every pass
//! has prepared, one flush uploads the new tail of each array into a storage
//! buffer. Recording resolves the bind group, so a buffer replaced by a later
//! flush in the same frame is picked up by every draw recorded after it.

use super::*;
use crate::view::render_pass::draw_rect_pass::{
    GRADIENT_STOPS_BUFFER_INITIAL_CAPACITY, GradientStopGpu, RECT_INSTANCE_BUFFER_INITIAL_CAPACITY,
    RectInstance,
};

/// Buffers unused for this many frames are released; an oversized buffer
/// that stayed below a quarter of its capacity this long is shrunk.
const MAX_IDLE_FRAMES: u64 = 120;

struct StorageBufferEntry {
    buffer: wgpu::Buffer,
    size: u64,
    last_used_frame: u64,
    last_high_usage_frame: u64,
}

/// One growable storage buffer fed from a CPU array that restarts per frame.
struct StorageStream<T> {
    staged: Vec<T>,
    /// Leading elements of `staged` already copied into `buffer` this frame.
    uploaded: usize,
    buffer: Option<StorageBufferEntry>,
    label: &'static str,
    initial_capacity: u64,
}

impl<T: bytemuck::Pod> StorageStream<T> {
    const STRIDE: u64 = std::mem::size_of::<T>() as u64;

    fn new(label: &'static str, initial_capacity: u64) -> Self {
        Self {
            staged: Vec::new(),
            uploaded: 0,
            buffer: None,
            label,
            initial_capacity,
        }
    }

    fn restart(&mut self) {
        self.staged.clear();
        self.uploaded = 0;
    }

    fn staged_bytes(&self) -> u64 {
        self.staged.len() as u64 * Self::STRIDE
    }

    fn create_buffer(&self, device: &wgpu::Device, size: u64, frame: u64) -> StorageBufferEntry {
        StorageBufferEntry {
            buffer: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(self.label),
                size,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            size,
            last_used_frame: frame,
            last_high_usage_frame: frame,
        }
    }

    /// Uploads the elements staged since the previous flush. Returns whether
    /// the buffer was replaced, or `None` when it cannot hold the frame.
    fn flush(
        &mut self,
        device: &wgpu::Device,
        uploader: &mut StorageUploader<'_>,
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
            let max_size = limits
                .max_storage_buffer_binding_size
                .min(limits.max_buffer_size);
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
    fn ensure_buffer(&mut self, device: &wgpu::Device, frame: u64) -> wgpu::Buffer {
        if self.buffer.is_none() {
            self.buffer = Some(self.create_buffer(device, self.initial_capacity, frame));
        }
        self.buffer
            .as_ref()
            .map(|entry| entry.buffer.clone())
            .expect("storage buffer was just ensured")
    }

    /// Runs before the frame restarts, so `staged` still describes the
    /// previous frame's usage. Returns whether the buffer changed.
    fn reclaim(&mut self, device: Option<&wgpu::Device>, frame: u64) -> bool {
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

    fn release(&mut self) {
        if let Some(entry) = self.buffer.take() {
            entry.buffer.destroy();
        }
        self.staged = Vec::new();
        self.uploaded = 0;
    }
}

/// Copies staged bytes into storage buffers at the current frame position.
struct StorageUploader<'a> {
    #[cfg(not(target_arch = "wasm32"))]
    staging_belt: &'a mut StagingBelt,
    #[cfg(not(target_arch = "wasm32"))]
    encoder: &'a mut wgpu::CommandEncoder,
    #[cfg(target_arch = "wasm32")]
    queue: &'a wgpu::Queue,
}

impl StorageUploader<'_> {
    fn write(&mut self, buffer: &wgpu::Buffer, offset: u64, bytes: &[u8]) {
        // On WebGPU, StagingBelt's async mapping may not resolve before the
        // next frame; queue writes have no mapping dependency.
        #[cfg(target_arch = "wasm32")]
        self.queue.write_buffer(buffer, offset, bytes);
        #[cfg(not(target_arch = "wasm32"))]
        {
            let size = wgpu::BufferSize::new(bytes.len() as u64).expect("nonempty storage upload");
            let mut mapped = self
                .staging_belt
                .write_buffer(self.encoder, buffer, offset, size);
            mapped.slice(..).copy_from_slice(bytes);
        }
    }
}

pub(super) struct RectInstanceFrame {
    instances: StorageStream<RectInstance>,
    gradient_stops: StorageStream<GradientStopGpu>,
    /// Bind groups keyed by draw-rect pipeline layout key. They bind the
    /// current buffers, so replacing either buffer clears the map.
    bind_groups: FxHashMap<u64, wgpu::BindGroup>,
}

impl Default for RectInstanceFrame {
    fn default() -> Self {
        Self {
            instances: StorageStream::new(
                "DrawRect Instance Storage Buffer",
                RECT_INSTANCE_BUFFER_INITIAL_CAPACITY,
            ),
            gradient_stops: StorageStream::new(
                "Gradient Stops Storage Buffer",
                GRADIENT_STOPS_BUFFER_INITIAL_CAPACITY,
            ),
            bind_groups: FxHashMap::default(),
        }
    }
}

impl Viewport {
    /// Stages one rectangle for this frame and returns its instance index.
    pub(crate) fn push_rect_instance(&mut self, instance: RectInstance) -> u32 {
        let staged = &mut self.frame.rect_instances.instances.staged;
        let index = u32::try_from(staged.len()).expect("rect instance index exceeds u32");
        staged.push(instance);
        index
    }

    /// Stages a run of gradient stops for this frame and returns the index of
    /// its first stop, or `None` for an empty run.
    pub(crate) fn push_gradient_stops(
        &mut self,
        stops: impl IntoIterator<Item = GradientStopGpu>,
    ) -> Option<u32> {
        let staged = &mut self.frame.rect_instances.gradient_stops.staged;
        let start = staged.len();
        staged.extend(stops);
        (staged.len() > start)
            .then(|| u32::try_from(start).expect("gradient stop index exceeds u32"))
    }

    /// Uploads everything staged since the previous flush in this frame, one
    /// copy per storage buffer, before any graphics pass records.
    pub(crate) fn flush_rect_instance_uploads(&mut self) -> bool {
        let frame_number = self.frame.frame_number;
        let rect = &mut self.frame.rect_instances;
        if rect.instances.uploaded == rect.instances.staged.len()
            && rect.gradient_stops.uploaded == rect.gradient_stops.staged.len()
        {
            return true;
        }
        let Some(device) = self.gpu.device.clone() else {
            return false;
        };
        #[cfg(target_arch = "wasm32")]
        let mut uploader = {
            let Some(queue) = self.gpu.queue.as_ref() else {
                return false;
            };
            StorageUploader { queue }
        };
        #[cfg(not(target_arch = "wasm32"))]
        let mut uploader = {
            let Some(frame) = self.frame.frame_state.as_mut() else {
                return false;
            };
            let staging_belt = self
                .gpu
                .upload_staging_belt
                .get_or_insert_with(|| StagingBelt::new(device.clone(), 1024 * 1024));
            StorageUploader {
                staging_belt,
                encoder: &mut frame.encoder,
            }
        };
        let uploads_instances = rect.instances.uploaded < rect.instances.staged.len();
        let Some(instances_replaced) = rect.instances.flush(&device, &mut uploader, frame_number)
        else {
            return false;
        };
        if uploads_instances {
            crate::ui::work_profile::count(|p| p.rect_instance_uploads += 1);
        }
        let Some(stops_replaced) = rect
            .gradient_stops
            .flush(&device, &mut uploader, frame_number)
        else {
            return false;
        };
        if instances_replaced || stops_replaced {
            rect.bind_groups.clear();
        }
        true
    }

    /// The bind group exposing this frame's instance storage (and gradient
    /// stops, for gradient layouts) to a draw-rect pipeline layout.
    pub(crate) fn rect_bind_group(
        &mut self,
        layout_key: u64,
        layout: &wgpu::BindGroupLayout,
        uses_gradient_stops: bool,
    ) -> Option<wgpu::BindGroup> {
        let frame_number = self.frame.frame_number;
        let rect = &mut self.frame.rect_instances;
        if let Some(bind_group) = rect.bind_groups.get(&layout_key) {
            return Some(bind_group.clone());
        }
        let device = self.gpu.device.as_ref()?;
        let instance_buffer = rect.instances.buffer.as_ref()?.buffer.clone();
        let stops_buffer =
            uses_gradient_stops.then(|| rect.gradient_stops.ensure_buffer(device, frame_number));
        let mut entries = vec![wgpu::BindGroupEntry {
            binding: 0,
            resource: instance_buffer.as_entire_binding(),
        }];
        if let Some(stops_buffer) = &stops_buffer {
            entries.push(wgpu::BindGroupEntry {
                binding: 1,
                resource: stops_buffer.as_entire_binding(),
            });
        }
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("DrawRect Instance Bind Group"),
            layout,
            entries: &entries,
        });
        rect.bind_groups.insert(layout_key, bind_group.clone());
        Some(bind_group)
    }

    /// Restarts instance and stop indices for a new frame. Buffers persist.
    pub(super) fn reset_rect_instance_frame(&mut self) {
        let rect = &mut self.frame.rect_instances;
        rect.instances.restart();
        rect.gradient_stops.restart();
    }

    pub(super) fn reclaim_idle_rect_instance_buffers(&mut self) {
        let frame_number = self.frame.frame_number;
        let device = self.gpu.device.as_ref();
        let rect = &mut self.frame.rect_instances;
        let instances_changed = rect.instances.reclaim(device, frame_number);
        let stops_changed = rect.gradient_stops.reclaim(device, frame_number);
        if instances_changed || stops_changed {
            rect.bind_groups.clear();
        }
    }

    pub(super) fn release_rect_instance_buffers(&mut self) {
        let rect = &mut self.frame.rect_instances;
        rect.instances.release();
        rect.gradient_stops.release();
        rect.bind_groups.clear();
    }

    #[cfg(test)]
    pub(crate) fn has_gradient_stops_buffer_for_test(&self) -> bool {
        self.frame.rect_instances.gradient_stops.buffer.is_some()
    }

    #[cfg(test)]
    pub(crate) fn gradient_stops_buffer_size_for_test(&self) -> Option<u64> {
        self.frame
            .rect_instances
            .gradient_stops
            .buffer
            .as_ref()
            .map(|entry| entry.size)
    }

    /// `(staged, uploaded, buffer)` for the instance stream.
    #[cfg(test)]
    pub(crate) fn rect_instance_stream_for_test(&self) -> (usize, usize, Option<wgpu::Buffer>) {
        let instances = &self.frame.rect_instances.instances;
        (
            instances.staged.len(),
            instances.uploaded,
            instances.buffer.as_ref().map(|entry| entry.buffer.clone()),
        )
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;
