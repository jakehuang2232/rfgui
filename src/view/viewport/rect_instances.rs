//! Per-frame rectangle instance and gradient-stop storage for [`Viewport`].
//!
//! Draw-rect preparation appends one [`RectInstance`] per rectangle, plus its
//! gradient stops, to CPU arrays that restart every frame. After every pass
//! has prepared, one flush uploads the new tail of each array into a storage
//! buffer. Recording resolves the bind group, so a buffer replaced by a later
//! flush in the same frame is picked up by every draw recorded after it.

use super::frame_streams::{FrameStream, FrameStreamUploader};
use super::*;
use crate::view::render_pass::draw_rect_pass::{
    GRADIENT_STOPS_BUFFER_INITIAL_CAPACITY, GradientStopGpu, RECT_INSTANCE_BUFFER_INITIAL_CAPACITY,
    RectInstance,
};

pub(super) struct RectInstanceFrame {
    instances: FrameStream<RectInstance>,
    gradient_stops: FrameStream<GradientStopGpu>,
    /// Bind groups keyed by draw-rect pipeline layout key. They bind the
    /// current buffers, so replacing either buffer clears the map.
    bind_groups: FxHashMap<u64, wgpu::BindGroup>,
}

impl Default for RectInstanceFrame {
    fn default() -> Self {
        Self {
            instances: FrameStream::new(
                "DrawRect Instance Storage Buffer",
                RECT_INSTANCE_BUFFER_INITIAL_CAPACITY,
                wgpu::BufferUsages::STORAGE,
            ),
            gradient_stops: FrameStream::new(
                "Gradient Stops Storage Buffer",
                GRADIENT_STOPS_BUFFER_INITIAL_CAPACITY,
                wgpu::BufferUsages::STORAGE,
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
            FrameStreamUploader { queue }
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
            FrameStreamUploader {
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
