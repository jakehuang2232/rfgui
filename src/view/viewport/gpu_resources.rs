//! GPU resource management methods for [`Viewport`].
//!
//! This module contains methods that manage offscreen render targets, sampled texture
//! caches, frame buffer pools, draw-rect uniform pools, and bind groups.

use super::*;

impl Viewport {
    pub(crate) fn acquire_offscreen_render_target(
        &mut self,
        allocation_id: AllocationId,
        desc: TextureDesc,
    ) -> Option<RenderTargetBundle> {
        let device = self.gpu.device.as_ref()?;
        let sample_count = desc.sample_count().max(1);
        self.frame.offscreen_render_target_pool
            .acquire(device, allocation_id, desc, sample_count)
    }

    pub(crate) fn acquire_persistent_render_target(
        &mut self,
        stable_key: u64,
        desc: TextureDesc,
    ) -> Option<RenderTargetBundle> {
        let device = self.gpu.device.as_ref()?;
        let sample_count = desc.sample_count().max(1);
        self.frame.offscreen_render_target_pool
            .acquire_persistent(device, stable_key, desc, sample_count)
    }

    pub(crate) fn upload_sampled_texture_rgba(
        &mut self,
        stable_key: u64,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
        bytes: &[u8],
    ) -> bool {
        let Some(device) = self.gpu.device.as_ref() else {
            return false;
        };
        let Some(queue) = self.gpu.queue.as_ref() else {
            return false;
        };
        let width = width.max(1);
        let height = height.max(1);
        let recreate = self
            .frame
            .sampled_texture_cache
            .get(&stable_key)
            .is_none_or(|entry| {
                entry.width != width || entry.height != height || entry.format != format
            });
        if recreate {
            // Destroy the old texture explicitly before replacing it, so GPU
            // memory is freed immediately rather than waiting for JS GC.
            if let Some(old) = self.frame.sampled_texture_cache.remove(&stable_key) {
                old.texture.destroy();
            }
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Sampled Image Texture"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            self.frame.sampled_texture_cache.insert(
                stable_key,
                SampledTextureEntry {
                    texture,
                    view,
                    width,
                    height,
                    format,
                    byte_size: width as u64 * height as u64 * 4,
                },
            );
        }
        let Some(entry) = self.frame.sampled_texture_cache.get(&stable_key) else {
            return false;
        };
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &entry.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width.saturating_mul(4)),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.evict_sampled_textures_under_pressure();
        true
    }

    pub(crate) fn sampled_texture_view(&self, stable_key: u64) -> Option<wgpu::TextureView> {
        self.frame.sampled_texture_cache
            .get(&stable_key)
            .map(|entry| entry.view.clone())
    }

    fn total_sampled_texture_bytes(&self) -> u64 {
        self.frame.sampled_texture_cache
            .values()
            .map(|entry| entry.byte_size)
            .sum()
    }

    fn evict_sampled_textures_under_pressure(&mut self) {
        let mut total_bytes = self.total_sampled_texture_bytes();

        // Collect unreferenced candidates with their last-access tick.
        let mut candidates: Vec<(u64, u64)> = self
            .frame
            .sampled_texture_cache
            .keys()
            .filter_map(|key| {
                let retention = crate::view::image_resource::image_asset_retention_info(*key)
                    .or_else(|| crate::view::svg_resource::svg_asset_retention_info(*key))?;
                (retention.ref_count == 0).then_some((*key, retention.last_access_tick))
            })
            .collect();

        // --- Time-based eviction (Chromium TileManager-style) ---
        // Evict stale entries even when under the pressure threshold.
        if !candidates.is_empty() {
            let newest_tick = candidates.iter().map(|(_, t)| *t).max().unwrap_or(0);
            let stale_keys: Vec<u64> = candidates
                .iter()
                .filter(|(_, tick)| newest_tick.saturating_sub(*tick) > Self::SAMPLED_TEXTURE_STALE_TICKS)
                .map(|(key, _)| *key)
                .collect();
            for key in &stale_keys {
                if let Some(entry) = self.frame.sampled_texture_cache.remove(key) {
                    entry.texture.destroy();
                    total_bytes = total_bytes.saturating_sub(entry.byte_size);
                }
            }
            candidates.retain(|(key, _)| !stale_keys.contains(key));
        }

        // --- Pressure-based eviction (Skia GrResourceCache-style) ---
        if total_bytes <= Self::SAMPLED_TEXTURE_PRESSURE_BYTES {
            return;
        }

        candidates.sort_by_key(|(_, tick)| *tick);

        for (key, _) in candidates {
            if total_bytes <= Self::SAMPLED_TEXTURE_EVICT_TO_BYTES {
                break;
            }
            if let Some(entry) = self.frame.sampled_texture_cache.remove(&key) {
                entry.texture.destroy();
                total_bytes = total_bytes.saturating_sub(entry.byte_size);
            }
        }
    }

    pub(crate) fn acquire_frame_buffer(
        &mut self,
        allocation_id: AllocationId,
        desc: BufferDesc,
    ) -> Option<wgpu::Buffer> {
        let device = self.gpu.device.as_ref()?;
        let key = allocation_id.0;
        let recreate = self
            .frame
            .frame_buffer_pool
            .get(&key)
            .is_none_or(|entry| entry.size != desc.size || entry.usage != desc.usage);
        if recreate {
            let usage = desc.usage | wgpu::BufferUsages::COPY_DST;
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: desc.label,
                size: desc.size.max(1),
                usage,
                mapped_at_creation: false,
            });
            if let Some(old) = self.frame.frame_buffer_pool.insert(
                key,
                FrameBufferEntry {
                    buffer: buffer.clone(),
                    size: desc.size.max(1),
                    usage: desc.usage,
                },
            ) {
                old.buffer.destroy();
            }
        }
        self.frame.frame_buffer_pool
            .get(&key)
            .map(|entry| entry.buffer.clone())
    }

    pub(crate) fn upload_frame_buffer(
        &mut self,
        allocation_id: AllocationId,
        desc: BufferDesc,
        offset: u64,
        data: &[u8],
    ) -> bool {
        if data.is_empty() {
            return true;
        }
        if offset % wgpu::COPY_BUFFER_ALIGNMENT != 0 {
            return false;
        }
        let Some(buffer) = self.acquire_frame_buffer(allocation_id, desc) else {
            return false;
        };
        let align = wgpu::COPY_BUFFER_ALIGNMENT as usize;
        let rem = data.len() % align;
        let padded_len = if rem == 0 {
            data.len()
        } else {
            data.len() + (align - rem)
        };
        let end = offset.saturating_add(padded_len as u64);
        if end > desc.size.max(1) {
            return false;
        }
        // On WebGPU (wasm32), StagingBelt's async buffer mapping (map_async → JS
        // Promise) can fail to resolve before the next frame, causing
        // "Buffer is not mapped" panics and unbounded memory growth.  Use the
        // simpler queue.write_buffer path which has no mapping dependency.
        #[cfg(target_arch = "wasm32")]
        {
            let Some(queue) = self.gpu.queue.as_ref() else {
                return false;
            };
            let mut padded = vec![0u8; padded_len];
            padded[..data.len()].copy_from_slice(data);
            queue.write_buffer(&buffer, offset, &padded);
            return true;
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            if self.gpu.upload_staging_belt.is_none() {
                let Some(device) = self.gpu.device.as_ref().cloned() else {
                    return false;
                };
                self.gpu.upload_staging_belt = Some(StagingBelt::new(device, 1024 * 1024));
            }
            let Some(frame) = self.frame.frame_state.as_mut() else {
                return false;
            };
            let Some(staging_belt) = self.gpu.upload_staging_belt.as_mut() else {
                return false;
            };
            let Some(size) = wgpu::BufferSize::new(padded_len as u64) else {
                return false;
            };
            let mut mapped = staging_belt.write_buffer(&mut frame.encoder, &buffer, offset, size);
            mapped.slice(..).fill(0);
            mapped.slice(..data.len()).copy_from_slice(data);
            drop(mapped);
            true
        }
    }

    pub(crate) fn upload_draw_rect_uniform(
        &mut self,
        data: &[u8],
        slot_size: u64,
        chunk_size: u64,
    ) -> Option<(wgpu::Buffer, u32, usize)> {
        if data.is_empty() || data.len() as u64 > slot_size {
            return None;
        }
        let device = self.gpu.device.as_ref()?.clone();
        #[cfg(not(target_arch = "wasm32"))]
        if self.gpu.upload_staging_belt.is_none() {
            self.gpu.upload_staging_belt = Some(StagingBelt::new(device.clone(), 1024 * 1024));
        }
        let required_size = chunk_size.max(slot_size).max(1);
        let has_current_capacity = self
            .frame
            .draw_rect_uniform_pool
            .get(self.frame.draw_rect_uniform_cursor)
            .is_some_and(|entry| {
                entry.size >= required_size
                    && self.frame.draw_rect_uniform_offset.saturating_add(slot_size) <= entry.size
            });
        if !has_current_capacity
            && self
                .frame
                .draw_rect_uniform_pool
                .get(self.frame.draw_rect_uniform_cursor)
                .is_some()
        {
            self.frame.draw_rect_uniform_cursor = self.frame.draw_rect_uniform_cursor.saturating_add(1);
            self.frame.draw_rect_uniform_offset = 0;
        }
        let target_index = self.frame.draw_rect_uniform_cursor;
        if self.frame.draw_rect_uniform_pool.len() <= target_index {
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("DrawRect Uniform Ring Buffer"),
                size: required_size,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.frame.draw_rect_uniform_pool.push(DrawRectUniformBufferEntry {
                buffer,
                size: required_size,
                bind_groups: FxHashMap::default(),
            });
        } else if self.frame.draw_rect_uniform_pool[target_index].size < required_size {
            // Buffer reallocated — invalidate all cached bind groups for this slot.
            let old = std::mem::replace(
                &mut self.frame.draw_rect_uniform_pool[target_index],
                DrawRectUniformBufferEntry {
                    buffer: device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("DrawRect Uniform Ring Buffer"),
                        size: required_size,
                        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    }),
                    size: required_size,
                    bind_groups: FxHashMap::default(),
                },
            );
            old.buffer.destroy();
        }
        let dynamic_offset = self.frame.draw_rect_uniform_offset;
        let buffer = self.frame.draw_rect_uniform_pool[target_index].buffer.clone();
        #[cfg(target_arch = "wasm32")]
        {
            let queue = self.gpu.queue.as_ref()?;
            let mut padded = vec![0u8; slot_size as usize];
            padded[..data.len()].copy_from_slice(data);
            queue.write_buffer(&buffer, dynamic_offset, &padded);
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let Some(size) = wgpu::BufferSize::new(slot_size) else {
                return None;
            };
            let frame = self.frame.frame_state.as_mut()?;
            let staging_belt = self.gpu.upload_staging_belt.as_mut()?;
            let mut mapped =
                staging_belt.write_buffer(&mut frame.encoder, &buffer, dynamic_offset, size);
            mapped.slice(..).fill(0);
            mapped.slice(..data.len()).copy_from_slice(data);
            drop(mapped);
        }
        self.frame.draw_rect_uniform_offset = self.frame.draw_rect_uniform_offset.saturating_add(slot_size);
        Some((buffer, dynamic_offset as u32, target_index))
    }

    /// Upload a run of gradient stops into the persistent gradient stops storage buffer,
    /// returning the starting stop index (not byte offset).  Grows the buffer if needed,
    /// invalidating cached draw-rect bind groups since they reference the old buffer.
    pub(crate) fn upload_gradient_stops(
        &mut self,
        stops: &[crate::view::render_pass::draw_rect_pass::GradientStopGpu],
    ) -> Option<u32> {
        use crate::view::render_pass::draw_rect_pass::{
            GRADIENT_STOPS_BUFFER_INITIAL_CAPACITY, GRADIENT_STOP_STRIDE,
        };
        if stops.is_empty() {
            return None;
        }
        let device = self.gpu.device.as_ref()?.clone();
        let stop_bytes: &[u8] = bytemuck::cast_slice(stops);
        let byte_len = stop_bytes.len() as u64;
        let needed_end = self.frame.gradient_stops_byte_cursor.saturating_add(byte_len);

        let current_size = self
            .frame
            .gradient_stops_buffer
            .as_ref()
            .map(|e| e.size)
            .unwrap_or(0);
        let mut buffer_grew = false;
        if needed_end > current_size {
            let mut new_size = current_size.max(GRADIENT_STOPS_BUFFER_INITIAL_CAPACITY).max(1);
            while new_size < needed_end {
                new_size = new_size.saturating_mul(2);
            }
            if let Some(old) = self.frame.gradient_stops_buffer.take() {
                old.buffer.destroy();
            }
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Gradient Stops Storage Buffer"),
                size: new_size,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.frame.gradient_stops_buffer = Some(GradientStopsBufferEntry {
                buffer,
                size: new_size,
            });
            buffer_grew = true;
        }

        if buffer_grew {
            // Existing cached draw-rect bind groups reference the stale storage buffer.
            for entry in self.frame.draw_rect_uniform_pool.iter_mut() {
                entry.bind_groups.clear();
            }
        }

        let entry = self.frame.gradient_stops_buffer.as_ref()?;
        let byte_offset = self.frame.gradient_stops_byte_cursor;
        #[cfg(target_arch = "wasm32")]
        {
            let queue = self.gpu.queue.as_ref()?;
            queue.write_buffer(&entry.buffer, byte_offset, stop_bytes);
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            if self.gpu.upload_staging_belt.is_none() {
                self.gpu.upload_staging_belt = Some(StagingBelt::new(device.clone(), 1024 * 1024));
            }
            let frame = self.frame.frame_state.as_mut()?;
            let staging_belt = self.gpu.upload_staging_belt.as_mut()?;
            let Some(size) = wgpu::BufferSize::new(byte_len) else {
                return None;
            };
            let mut mapped =
                staging_belt.write_buffer(&mut frame.encoder, &entry.buffer, byte_offset, size);
            mapped.slice(..).copy_from_slice(stop_bytes);
            drop(mapped);
        }

        self.frame.gradient_stops_byte_cursor = needed_end;
        let start_index = (byte_offset / GRADIENT_STOP_STRIDE) as u32;
        Some(start_index)
    }

    pub(crate) fn ensure_gradient_stops_buffer(&mut self) -> Option<&wgpu::Buffer> {
        use crate::view::render_pass::draw_rect_pass::GRADIENT_STOPS_BUFFER_INITIAL_CAPACITY;
        if self.frame.gradient_stops_buffer.is_none() {
            let device = self.gpu.device.as_ref()?.clone();
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Gradient Stops Storage Buffer"),
                size: GRADIENT_STOPS_BUFFER_INITIAL_CAPACITY,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.frame.gradient_stops_buffer = Some(GradientStopsBufferEntry {
                buffer,
                size: GRADIENT_STOPS_BUFFER_INITIAL_CAPACITY,
            });
        }
        self.frame.gradient_stops_buffer.as_ref().map(|e| &e.buffer)
    }

    /// Return a cached bind group for the given uniform pool slot and pipeline layout key,
    /// creating and storing it on the first call.  Bind groups bind the pool buffer at
    /// offset 0 / size=slot_size; dynamic offsets are supplied per-draw, so one bind group
    /// is valid for every slot in the same pool buffer.
    pub(crate) fn get_or_create_draw_rect_bind_group(
        &mut self,
        pool_index: usize,
        layout_cache_key: u64,
        layout: &wgpu::BindGroupLayout,
        slot_size: u64,
    ) -> Option<wgpu::BindGroup> {
        let entry = self.frame.draw_rect_uniform_pool.get(pool_index)?;
        if let Some(bg) = entry.bind_groups.get(&layout_cache_key) {
            return Some(bg.clone());
        }
        // Ensure the gradient stops buffer exists so binding 1 can resolve.
        self.ensure_gradient_stops_buffer();
        let stops_buffer = self.frame.gradient_stops_buffer.as_ref()?.buffer.clone();
        let uniform_buffer = self.frame.draw_rect_uniform_pool.get(pool_index)?.buffer.clone();
        let device = self.gpu.device.as_ref()?;
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("DrawRect Bind Group (Cached)"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &uniform_buffer,
                        offset: 0,
                        size: wgpu::BufferSize::new(slot_size),
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: stops_buffer.as_entire_binding(),
                },
            ],
        });
        self.frame.draw_rect_uniform_pool
            .get_mut(pool_index)?
            .bind_groups
            .insert(layout_cache_key, bg.clone());
        Some(bg)
    }

    pub fn release_render_resource_caches(&mut self) {
        crate::view::render_pass::draw_rect_pass::clear_draw_rect_resources_cache();
        crate::view::render_pass::shadow_module::clear_shadow_resources_cache();
        crate::view::render_pass::text_pass::clear_text_resources_cache();
        crate::view::render_pass::blur_module::clear_blur_resources_cache();
        crate::view::render_pass::composite_layer_pass::clear_composite_layer_resources_cache();
        crate::view::render_pass::texture_composite_pass::clear_texture_composite_resources_cache();
        crate::view::render_pass::present_surface_pass::clear_present_surface_resources_cache();
        self.frame.offscreen_render_target_pool.clear();
        for entry in self.frame.sampled_texture_cache.values() {
            entry.texture.destroy();
        }
        self.frame.sampled_texture_cache.clear();
        crate::view::image_resource::invalidate_uploaded_images();
        crate::view::svg_resource::invalidate_uploaded_images();
        for entry in self.frame.frame_buffer_pool.values() {
            entry.buffer.destroy();
        }
        self.frame.frame_buffer_pool.clear();
        for entry in &self.frame.draw_rect_uniform_pool {
            entry.buffer.destroy();
        }
        self.frame.draw_rect_uniform_pool.clear();
        self.frame.draw_rect_uniform_cursor = 0;
        self.frame.draw_rect_uniform_offset = 0;
        if let Some(entry) = self.frame.gradient_stops_buffer.take() {
            entry.buffer.destroy();
        }
        self.frame.gradient_stops_byte_cursor = 0;
        self.gpu.upload_staging_belt = None;
    }
}
