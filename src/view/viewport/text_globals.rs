//! Per-frame text draw uniforms and fragment storage for [`Viewport`].
//!
//! Text preparation appends one [`TextDrawUniform`] per pass, at a dynamic
//! offset, and the pass's fragments to one storage array; the draw uniform
//! names where they start. Both flush once after every pass has prepared,
//! instead of two small uploads per pass. Recording resolves the bind group,
//! so a buffer replaced by a later flush in the same frame is picked up by
//! every draw recorded after it.

use super::frame_streams::{FrameStream, FrameStreamUploader};
use super::*;
use crate::view::render_pass::text_pass::{
    FragmentUniform, TEXT_DRAW_BUFFER_INITIAL_CAPACITY, TEXT_DRAW_UNIFORM_BINDING_SIZE,
    TEXT_FRAGMENT_BUFFER_INITIAL_CAPACITY, TextDrawUniform,
};

pub(super) struct TextGlobalsFrame {
    draws: FrameStream<TextDrawUniform>,
    fragments: FrameStream<FragmentUniform>,
    /// Binds the current buffers for the layout it was made with, so
    /// replacing either buffer clears it.
    bind_group: Option<(wgpu::BindGroupLayout, wgpu::BindGroup)>,
}

impl Default for TextGlobalsFrame {
    fn default() -> Self {
        Self {
            draws: FrameStream::new(
                "Text Draw Uniform Buffer",
                TEXT_DRAW_BUFFER_INITIAL_CAPACITY,
                wgpu::BufferUsages::UNIFORM,
            ),
            fragments: FrameStream::new(
                "Text Fragment Storage Buffer",
                TEXT_FRAGMENT_BUFFER_INITIAL_CAPACITY,
                wgpu::BufferUsages::STORAGE,
            ),
            bind_group: None,
        }
    }
}

impl Viewport {
    /// Stages one text draw for this frame and returns the dynamic offset of
    /// its draw uniform, or `None` past the range a dynamic offset can name.
    pub(crate) fn push_text_draw(
        &mut self,
        screen_size: [f32; 2],
        fragments: &[FragmentUniform],
    ) -> Option<u32> {
        let text = &mut self.frame.text_globals;
        let fragment_base = u32::try_from(text.fragments.staged.len()).ok()?;
        let offset =
            u32::try_from(text.draws.staged.len() as u64 * FrameStream::<TextDrawUniform>::STRIDE)
                .ok()?;
        text.fragments.staged.extend_from_slice(fragments);
        text.draws
            .staged
            .push(TextDrawUniform::new(screen_size, fragment_base));
        Some(offset)
    }

    /// Uploads the text draws staged since the previous flush in this frame,
    /// one copy per buffer, before any graphics pass records.
    pub(crate) fn flush_text_globals_uploads(&mut self) -> bool {
        let frame_number = self.frame.frame_number;
        let text = &mut self.frame.text_globals;
        if text.draws.uploaded == text.draws.staged.len()
            && text.fragments.uploaded == text.fragments.staged.len()
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
        let Some(draws_replaced) = text.draws.flush(&device, &mut uploader, frame_number) else {
            return false;
        };
        let Some(fragments_replaced) = text.fragments.flush(&device, &mut uploader, frame_number)
        else {
            return false;
        };
        if draws_replaced || fragments_replaced {
            text.bind_group = None;
        }
        true
    }

    /// The bind group exposing this frame's text draw uniforms and fragments
    /// to a text pipeline with `layout`; draws pick theirs by dynamic offset.
    pub(crate) fn text_globals_bind_group(
        &mut self,
        layout: &wgpu::BindGroupLayout,
    ) -> Option<wgpu::BindGroup> {
        let frame_number = self.frame.frame_number;
        let text = &mut self.frame.text_globals;
        if let Some((cached_layout, bind_group)) = &text.bind_group
            && cached_layout == layout
        {
            return Some(bind_group.clone());
        }
        let device = self.gpu.device.as_ref()?;
        let draws = text.draws.buffer.as_ref()?.buffer.clone();
        let fragments = text.fragments.ensure_buffer(device, frame_number);
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Text Globals Bind Group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &draws,
                        offset: 0,
                        size: wgpu::BufferSize::new(TEXT_DRAW_UNIFORM_BINDING_SIZE),
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: fragments.as_entire_binding(),
                },
            ],
        });
        text.bind_group = Some((layout.clone(), bind_group.clone()));
        Some(bind_group)
    }

    /// Restarts text draw and fragment indices for a new frame. Buffers
    /// persist.
    pub(super) fn reset_text_globals_frame(&mut self) {
        let text = &mut self.frame.text_globals;
        text.draws.restart();
        text.fragments.restart();
    }

    pub(super) fn reclaim_idle_text_globals_buffers(&mut self) {
        let frame_number = self.frame.frame_number;
        let device = self.gpu.device.as_ref();
        let text = &mut self.frame.text_globals;
        let draws_changed = text.draws.reclaim(device, frame_number);
        let fragments_changed = text.fragments.reclaim(device, frame_number);
        if draws_changed || fragments_changed {
            text.bind_group = None;
        }
    }

    pub(super) fn release_text_globals_buffers(&mut self) {
        let text = &mut self.frame.text_globals;
        text.draws.release();
        text.fragments.release();
        text.bind_group = None;
    }

    /// The draw uniform and fragment buffers, once both exist.
    #[cfg(test)]
    pub(crate) fn text_globals_buffers_for_test(&self) -> Option<[wgpu::Buffer; 2]> {
        let text = &self.frame.text_globals;
        Some([
            text.draws.buffer.as_ref()?.buffer.clone(),
            text.fragments.buffer.as_ref()?.buffer.clone(),
        ])
    }
}
