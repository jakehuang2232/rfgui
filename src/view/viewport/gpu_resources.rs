//! GPU resource management methods for [`Viewport`].
//!
//! This module contains methods that manage offscreen render targets, sampled texture
//! caches, frame buffer pools, draw-rect uniform pools, and bind groups.

use super::*;

/// Pool-canonical artifact residents whose validity depends only on their
/// sealed values. Unlike compile actions, this proof remains valid while it is
/// staged and committed because it contains no observation of pool residency.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct PoolCanonicalArtifactSurfaceResidents {
    residents: crate::view::paint::SealedArtifactSurfaceResidentSet,
}

impl PoolCanonicalArtifactSurfaceResidents {
    pub(crate) fn residents(&self) -> &crate::view::paint::SealedArtifactSurfaceResidentSet {
        &self.residents
    }

    fn into_ordered_entries(self) -> Vec<crate::view::paint::SealedArtifactSurfaceResidentEntry> {
        self.residents.into_ordered_entries()
    }
}

/// One emission-scoped binding between pool-canonical residents and actions
/// derived from the pool's current state. It is consumed before staging so an
/// action snapshot can never masquerade as a persistent validity proof.
#[derive(Debug)]
pub(crate) struct PreparedArtifactSurfacePoolEmission<'pool> {
    residents: PoolCanonicalArtifactSurfaceResidents,
    ordered_actions: Vec<(
        crate::view::paint::RetainedSurfaceResidentKey,
        crate::view::paint::RetainedSurfaceCompileAction,
    )>,
    pool: std::marker::PhantomData<&'pool Viewport>,
}

impl PreparedArtifactSurfacePoolEmission<'_> {
    pub(crate) fn residents(&self) -> &crate::view::paint::SealedArtifactSurfaceResidentSet {
        self.residents.residents()
    }

    pub(crate) fn ordered_actions(
        &self,
    ) -> &[(
        crate::view::paint::RetainedSurfaceResidentKey,
        crate::view::paint::RetainedSurfaceCompileAction,
    )] {
        &self.ordered_actions
    }

    pub(crate) fn into_canonical_residents(self) -> PoolCanonicalArtifactSurfaceResidents {
        self.residents
    }
}

fn complete_persistent_pair_witness(color_compatible: bool, depth_compatible: bool) -> bool {
    color_compatible && depth_compatible
}

fn canonical_retained_surface_pair_bytes(
    stamp: &crate::view::paint::RetainedSurfaceRasterStamp,
) -> Option<u64> {
    if !stamp
        .target
        .has_canonical_descriptor_pair_for(stamp.identity)
    {
        return None;
    }
    let color = crate::view::raster_cost::texture_desc_payload_bytes(&stamp.target.color);
    let depth = crate::view::raster_cost::texture_desc_payload_bytes(&stamp.target.depth);
    if !color.confidence.budget_usable() || !depth.confidence.budget_usable() {
        return None;
    }
    color.bytes.checked_add(depth.bytes)
}

fn artifact_surface_resident_set_is_pool_canonical(
    residents: &crate::view::paint::SealedArtifactSurfaceResidentSet,
) -> bool {
    if !residents.is_canonical() {
        return false;
    }
    // An empty set intentionally passes the empty validation below: it owns
    // no resident or persistent keys and represents an exact empty replacement.
    let mut resident_keys = FxHashSet::default();
    let mut persistent_keys = FxHashSet::default();
    residents.ordered_entries().iter().all(|entry| {
        let stamp = entry.stamp();
        let Some(depth_key) = stamp.identity.color_key.depth_stencil() else {
            return false;
        };
        canonical_retained_surface_pair_bytes(stamp).is_some()
            && resident_keys.insert(entry.resident_key())
            && persistent_keys.insert(stamp.identity.color_key)
            && persistent_keys.insert(depth_key)
    })
}

impl Viewport {
    #[cfg(test)]
    pub(crate) fn retained_surface_transaction_shape_for_test(&self) -> (usize, Option<usize>) {
        (
            self.compositor.retained_surfaces.entries.len(),
            self.compositor
                .pending_retained_surfaces
                .as_ref()
                .map(|pending| match pending {
                    PendingRetainedSurfaceTransaction::Clear => 0,
                    PendingRetainedSurfaceTransaction::CommitArtifactSurfaceSet { residents } => {
                        residents.residents().len()
                    }
                }),
        )
    }

    #[cfg(test)]
    pub(crate) fn pending_artifact_surface_resident_keys_for_test(
        &self,
    ) -> Option<Vec<crate::view::paint::RetainedSurfaceResidentKey>> {
        let PendingRetainedSurfaceTransaction::CommitArtifactSurfaceSet { residents } =
            self.compositor.pending_retained_surfaces.as_ref()?
        else {
            return None;
        };
        Some(
            residents
                .residents()
                .ordered_entries()
                .iter()
                .map(crate::view::paint::SealedArtifactSurfaceResidentEntry::resident_key)
                .collect(),
        )
    }

    #[cfg(test)]
    pub(crate) fn committed_retained_surface_resident_keys_for_test(
        &self,
    ) -> FxHashSet<crate::view::paint::RetainedSurfaceResidentKey> {
        self.compositor
            .retained_surfaces
            .entries
            .keys()
            .copied()
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn encode_persistent_render_target_readback_for_test(
        &self,
        stable_key: crate::view::frame_graph::PersistentTextureKey,
        encoder: &mut wgpu::CommandEncoder,
        buffer: &wgpu::Buffer,
        padded_bytes_per_row: u32,
        width: u32,
        height: u32,
    ) -> Result<(), String> {
        self.frame
            .offscreen_render_target_pool
            .encode_persistent_readback(
                stable_key,
                encoder,
                buffer,
                padded_bytes_per_row,
                width,
                height,
            )
    }

    #[cfg(test)]
    pub(crate) fn retained_surface_release_log_for_test(
        &self,
    ) -> &[crate::view::frame_graph::PersistentTextureKey] {
        &self.compositor.retained_surface_release_log
    }

    fn prepare_artifact_surface_pool_emission(
        &self,
        residents: crate::view::paint::SealedArtifactSurfaceResidentSet,
        allow_forced_pair_witness: bool,
    ) -> Option<PreparedArtifactSurfacePoolEmission<'_>> {
        if !artifact_surface_resident_set_is_pool_canonical(&residents) {
            return None;
        }
        let ordered_actions = residents
            .ordered_entries()
            .iter()
            .map(|entry| {
                let key = entry.resident_key();
                let stamp = entry.stamp();
                (
                    key,
                    self.retained_surface_compile_action_against_resident(
                        stamp,
                        self.compositor.retained_surfaces.entries.get(&key),
                        allow_forced_pair_witness,
                    ),
                )
            })
            .collect::<Vec<_>>();
        Some(PreparedArtifactSurfacePoolEmission {
            residents: PoolCanonicalArtifactSurfaceResidents { residents },
            ordered_actions,
            pool: std::marker::PhantomData,
        })
    }

    /// Validates compiler-sealed artifact residents once, then binds them to
    /// actions derived from the pool's current state. Only the pure resident
    /// capability may survive past emission; the action snapshot may not.
    pub(crate) fn prepare_artifact_surface_pool_emission_from_pool(
        &self,
        residents: crate::view::paint::SealedArtifactSurfaceResidentSet,
    ) -> Option<PreparedArtifactSurfacePoolEmission<'_>> {
        self.prepare_artifact_surface_pool_emission(residents, false)
    }

    #[cfg(test)]
    pub(crate) fn prepare_artifact_surface_pool_emission_for_forced_test(
        &self,
        residents: crate::view::paint::SealedArtifactSurfaceResidentSet,
    ) -> Option<PreparedArtifactSurfacePoolEmission<'_>> {
        self.prepare_artifact_surface_pool_emission(residents, true)
    }

    fn retained_surface_compile_action_against_resident(
        &self,
        stamp: &crate::view::paint::RetainedSurfaceRasterStamp,
        resident: Option<&crate::view::paint::RetainedSurfaceRasterStamp>,
        allow_forced_pair_witness: bool,
    ) -> crate::view::paint::RetainedSurfaceCompileAction {
        let color_key = stamp.identity.color_key;
        let forced_pair_witness = {
            #[cfg(test)]
            {
                allow_forced_pair_witness
                    && self
                        .compositor
                        .retained_surface_pair_witnesses
                        .contains(&color_key)
            }
            #[cfg(not(test))]
            {
                let _ = allow_forced_pair_witness;
                false
            }
        };
        let pair_compatible = stamp
            .target
            .has_canonical_descriptor_pair_for(stamp.identity)
            && (self.has_compatible_persistent_render_target_pair(color_key, &stamp.target.color)
                || forced_pair_witness);
        if pair_compatible && resident == Some(stamp) {
            crate::view::paint::RetainedSurfaceCompileAction::Reuse
        } else {
            crate::view::paint::RetainedSurfaceCompileAction::Reraster
        }
    }

    #[cfg(test)]
    pub(crate) fn forget_retained_surface_pair_witness_for_test(
        &mut self,
        color_key: crate::view::frame_graph::PersistentTextureKey,
    ) {
        self.compositor
            .retained_surface_pair_witnesses
            .remove(&color_key);
    }

    fn release_retained_surface_pair(
        &mut self,
        color_key: crate::view::frame_graph::PersistentTextureKey,
    ) {
        #[cfg(test)]
        {
            self.compositor
                .retained_surface_pair_witnesses
                .remove(&color_key);
            self.compositor.retained_surface_release_log.push(color_key);
        }
        self.frame
            .offscreen_render_target_pool
            .release_persistent_pair(color_key);
    }

    /// Stages only pool-canonical artifact `(resident key, stamp)` pairs for
    /// this exact frame owner. Unlike legacy staging, this entry point accepts
    /// only the linear capability and therefore cannot revalidate, substitute,
    /// or re-derive a different key.
    pub(crate) fn stage_artifact_surface_resident_set(
        &mut self,
        owner: RetainedSurfaceFrameStageOwner,
        residents: PoolCanonicalArtifactSurfaceResidents,
    ) -> bool {
        if !self.retained_surface_frame_stage_owner_is_active(owner) {
            return false;
        }
        debug_assert!(self.compositor.pending_retained_surfaces.is_none());
        self.compositor.pending_retained_surfaces =
            Some(PendingRetainedSurfaceTransaction::CommitArtifactSurfaceSet { residents });
        self.compositor.pending_retained_surface_owner = Some(owner.generation);
        true
    }

    fn allocate_retained_surface_owner(&mut self) -> u64 {
        let owner = self.compositor.next_retained_surface_owner;
        self.compositor.next_retained_surface_owner = owner
            .checked_add(1)
            .expect("retained surface owner generation exhausted");
        assert_ne!(owner, 0, "retained surface owner generation is non-zero");
        owner
    }

    /// Reserves the owner generation inherited by every retained stage in
    /// this frame. An already-pending transaction belongs to another owner,
    /// so the frame receives no finish capability and must leave it intact.
    pub(crate) fn begin_retained_surface_frame_stage(
        &mut self,
    ) -> Option<RetainedSurfaceFrameStageOwner> {
        if self.compositor.pending_retained_surfaces.is_some()
            || self
                .compositor
                .active_retained_surface_frame_owner
                .is_some()
        {
            return None;
        }
        let generation = self.allocate_retained_surface_owner();
        self.compositor.active_retained_surface_frame_owner = Some(generation);
        Some(RetainedSurfaceFrameStageOwner { generation })
    }

    pub(crate) fn retained_surface_frame_stage_owner_is_active(
        &self,
        owner: RetainedSurfaceFrameStageOwner,
    ) -> bool {
        self.compositor.active_retained_surface_frame_owner == Some(owner.generation)
            && self.compositor.pending_retained_surfaces.is_none()
    }

    /// Every retained producer shares one transaction slot. Replacing a
    /// pending owner would orphan its graph-declared persistent pairs, so the
    /// only legal cancellation path is the explicit finish/invalidate
    /// lifecycle.
    fn try_stage_retained_surface_transaction(
        &mut self,
        pending: PendingRetainedSurfaceTransaction,
    ) -> bool {
        if self.compositor.pending_retained_surfaces.is_some() {
            return false;
        }
        let owner = self
            .compositor
            .active_retained_surface_frame_owner
            .unwrap_or_else(|| self.allocate_retained_surface_owner());
        self.compositor.pending_retained_surfaces = Some(pending);
        self.compositor.pending_retained_surface_owner = Some(owner);
        true
    }

    #[allow(dead_code)] // C4A staging authority; production dispatch lands in C4B.
    pub(crate) fn stage_retained_surface_clear(&mut self) -> bool {
        self.try_stage_retained_surface_transaction(PendingRetainedSurfaceTransaction::Clear)
    }

    pub(crate) fn finish_retained_surface_transaction(&mut self, succeeded: bool) {
        if !succeeded {
            self.invalidate_retained_surfaces();
            return;
        }
        match self.compositor.pending_retained_surfaces.take() {
            Some(PendingRetainedSurfaceTransaction::CommitArtifactSurfaceSet { residents }) => {
                // This pending variant is constructible only from a
                // pool-canonical linear capability. The former revalidation
                // and duplicate-key fallback were therefore unreachable.
                let ordered_entries = residents.into_ordered_entries();
                let next_color_keys = ordered_entries
                    .iter()
                    .map(|entry| entry.stamp().identity.color_key)
                    .collect::<FxHashSet<_>>();
                let next = ordered_entries
                    .into_iter()
                    .map(crate::view::paint::SealedArtifactSurfaceResidentEntry::into_parts)
                    .collect::<FxHashMap<_, _>>();
                let previous = std::mem::take(&mut self.compositor.retained_surfaces.entries);
                for color_key in previous
                    .values()
                    .map(|stamp| stamp.identity.color_key)
                    .collect::<FxHashSet<_>>()
                {
                    if !next_color_keys.contains(&color_key) {
                        self.release_retained_surface_pair(color_key);
                    }
                }
                self.compositor.retained_surfaces.entries = next;
                #[cfg(test)]
                self.compositor
                    .retained_surface_pair_witnesses
                    .extend(next_color_keys);
            }
            Some(PendingRetainedSurfaceTransaction::Clear) | None => {
                self.invalidate_retained_surfaces()
            }
        }
        self.compositor.pending_retained_surface_owner = None;
    }

    /// Consumes only the pending transaction staged by this exact frame
    /// owner. Missing, wrong, or stale tokens are graph-result agnostic and
    /// cannot mutate a foreign pending transaction or resident state.
    pub(crate) fn finish_retained_surface_transaction_for_frame(
        &mut self,
        owner: Option<RetainedSurfaceFrameStageOwner>,
        succeeded: bool,
    ) -> bool {
        let Some(owner) = owner else {
            return false;
        };
        if self.compositor.active_retained_surface_frame_owner != Some(owner.generation) {
            return false;
        }
        match self.compositor.pending_retained_surface_owner {
            Some(pending_owner) if pending_owner != owner.generation => return false,
            _ => {}
        }
        self.compositor.active_retained_surface_frame_owner = None;
        if self.compositor.pending_retained_surfaces.is_some() {
            self.finish_retained_surface_transaction(succeeded);
        }
        true
    }

    pub(crate) fn invalidate_retained_surfaces(&mut self) {
        let mut color_keys = self
            .compositor
            .retained_surfaces
            .entries
            .values()
            .map(|stamp| stamp.identity.color_key)
            .collect::<FxHashSet<_>>();
        if let Some(PendingRetainedSurfaceTransaction::CommitArtifactSurfaceSet { residents }) =
            self.compositor.pending_retained_surfaces.as_ref()
        {
            color_keys.extend(
                residents
                    .residents()
                    .ordered_entries()
                    .iter()
                    .map(|entry| entry.stamp().identity.color_key),
            );
        }

        self.compositor.retained_surfaces.entries.clear();
        self.compositor.pending_retained_surfaces = None;
        self.compositor.pending_retained_surface_owner = None;
        self.compositor.active_retained_surface_frame_owner = None;
        for color_key in color_keys {
            self.release_retained_surface_pair(color_key);
        }
        #[cfg(test)]
        self.compositor.retained_surface_pair_witnesses.clear();
    }

    pub(crate) fn reclaim_idle_frame_gpu_pools(&mut self) {
        const MAX_IDLE_FRAMES: u64 = 120;
        let frame_number = self.frame.frame_number;
        self.frame.draw_rect_uniform_pool.retain(|entry| {
            let keep = frame_number.saturating_sub(entry.last_used_frame) <= MAX_IDLE_FRAMES;
            if !keep {
                entry.buffer.destroy();
            }
            keep
        });
        self.frame.draw_rect_uniform_cursor = self
            .frame
            .draw_rect_uniform_cursor
            .min(self.frame.draw_rect_uniform_pool.len());

        let Some(entry) = self.frame.gradient_stops_buffer.as_ref() else {
            return;
        };
        if frame_number.saturating_sub(entry.last_used_frame) > MAX_IDLE_FRAMES {
            if let Some(entry) = self.frame.gradient_stops_buffer.take() {
                entry.buffer.destroy();
            }
            for entry in &mut self.frame.draw_rect_uniform_pool {
                entry.bind_groups.clear();
            }
            return;
        }

        use crate::view::render_pass::draw_rect_pass::GRADIENT_STOPS_BUFFER_INITIAL_CAPACITY;
        let previous_usage = self.frame.gradient_stops_byte_cursor;
        let should_shrink = entry.size > GRADIENT_STOPS_BUFFER_INITIAL_CAPACITY
            && previous_usage.saturating_mul(4) <= entry.size
            && frame_number.saturating_sub(entry.last_high_usage_frame) > MAX_IDLE_FRAMES;
        if !should_shrink {
            return;
        }
        let Some(device) = self.gpu.device.as_ref() else {
            return;
        };
        let new_size = previous_usage
            .max(1)
            .checked_next_power_of_two()
            .unwrap_or(u64::MAX)
            .max(GRADIENT_STOPS_BUFFER_INITIAL_CAPACITY);
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Gradient Stops Storage Buffer"),
            size: new_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        if let Some(old) = self
            .frame
            .gradient_stops_buffer
            .replace(GradientStopsBufferEntry {
                buffer,
                size: new_size,
                last_used_frame: frame_number,
                last_high_usage_frame: frame_number,
            })
        {
            old.buffer.destroy();
        }
        for entry in &mut self.frame.draw_rect_uniform_pool {
            entry.bind_groups.clear();
        }
    }

    pub(crate) fn touch_persistent_render_targets(
        &mut self,
        stable_keys: impl IntoIterator<Item = crate::view::frame_graph::PersistentTextureKey>,
    ) {
        for stable_key in stable_keys {
            self.frame
                .offscreen_render_target_pool
                .touch_persistent(stable_key);
        }
    }

    pub(crate) fn acquire_offscreen_render_target(
        &mut self,
        allocation_id: AllocationId,
        desc: TextureDesc,
    ) -> Option<RenderTargetBundle> {
        let device = self.gpu.device.as_ref()?;
        let sample_count = desc.sample_count().max(1);
        self.frame
            .offscreen_render_target_pool
            .acquire(device, allocation_id, desc, sample_count)
    }

    pub(crate) fn acquire_persistent_render_target(
        &mut self,
        stable_key: crate::view::frame_graph::PersistentTextureKey,
        desc: TextureDesc,
    ) -> Option<RenderTargetBundle> {
        let device = self.gpu.device.as_ref()?;
        let sample_count = desc.sample_count().max(1);
        self.frame.offscreen_render_target_pool.acquire_persistent(
            device,
            stable_key,
            desc,
            sample_count,
        )
    }

    #[allow(dead_code)] // C2b threads this pool fact into artifact compilation.
    pub(crate) fn has_compatible_persistent_render_target(
        &self,
        stable_key: crate::view::frame_graph::PersistentTextureKey,
        desc: &TextureDesc,
    ) -> bool {
        self.frame
            .offscreen_render_target_pool
            .has_compatible_persistent(stable_key, desc, desc.sample_count().max(1))
    }

    pub(crate) fn has_compatible_persistent_render_target_pair(
        &self,
        color_key: crate::view::frame_graph::PersistentTextureKey,
        color_desc: &TextureDesc,
    ) -> bool {
        let (_, depth_desc) = crate::view::base_component::persistent_target_texture_descriptors(
            color_desc.clone(),
            color_key,
        );
        let color_compatible = self.has_compatible_persistent_render_target(color_key, color_desc);
        let depth_compatible = color_key.depth_stencil().is_some_and(|depth_key| {
            self.has_compatible_persistent_render_target(depth_key, &depth_desc)
        });
        complete_persistent_pair_witness(color_compatible, depth_compatible)
    }

    pub(crate) fn release_persistent_render_target_pair(
        &mut self,
        color_key: crate::view::frame_graph::PersistentTextureKey,
    ) -> bool {
        self.frame
            .offscreen_render_target_pool
            .release_persistent_pair(color_key)
    }

    pub(crate) fn ensure_sampled_texture(
        &mut self,
        upload: &crate::view::sampled_texture::SampledTextureUpload,
    ) -> bool {
        let Some(validated) = upload.validate_rgba8() else {
            return false;
        };
        let Some(device) = self.gpu.device.as_ref() else {
            return false;
        };
        let Some(queue) = self.gpu.queue.as_ref() else {
            return false;
        };
        let width = validated.width;
        let height = validated.height;
        let frame_number = self.frame.frame_number;
        let recreate = self
            .frame
            .sampled_texture_cache
            .get(&upload.id)
            .is_none_or(|entry| {
                entry.width != width
                    || entry.height != height
                    || entry.format != upload.format
                    || entry.alpha_mode != upload.alpha_mode
            });
        if recreate {
            // Destroy the old texture explicitly before replacing it, so GPU
            // memory is freed immediately rather than waiting for JS GC.
            if let Some(old) = self.frame.sampled_texture_cache.remove(&upload.id) {
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
                format: upload.format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            self.frame.sampled_texture_cache.insert(
                upload.id,
                SampledTextureEntry {
                    texture,
                    view,
                    width,
                    height,
                    format: upload.format,
                    alpha_mode: upload.alpha_mode,
                    generation: upload.generation,
                    byte_size: width as u64 * height as u64 * 4,
                    last_used_frame: frame_number,
                },
            );
        }
        let Some(entry) = self.frame.sampled_texture_cache.get_mut(&upload.id) else {
            return false;
        };
        entry.last_used_frame = frame_number;
        let requires_upload = recreate || entry.generation != upload.generation;
        if requires_upload {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &entry.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                upload.pixels.as_ref(),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(validated.bytes_per_row),
                    rows_per_image: Some(height),
                },
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );
            entry.generation = upload.generation;
            self.frame.sampled_texture_upload_count =
                self.frame.sampled_texture_upload_count.saturating_add(1);
        }
        self.evict_sampled_textures_under_pressure();
        true
    }

    pub(crate) fn sampled_texture_view(
        &self,
        id: crate::view::sampled_texture::SampledTextureId,
    ) -> Option<wgpu::TextureView> {
        self.frame
            .sampled_texture_cache
            .get(&id)
            .map(|entry| entry.view.clone())
    }

    #[cfg(all(test, not(target_arch = "wasm32")))]
    pub(crate) fn evict_sampled_texture_for_test(
        &mut self,
        id: crate::view::sampled_texture::SampledTextureId,
    ) {
        if let Some(entry) = self.frame.sampled_texture_cache.remove(&id) {
            entry.texture.destroy();
        }
    }

    fn total_sampled_texture_bytes(&self) -> u64 {
        self.frame
            .sampled_texture_cache
            .values()
            .map(|entry| entry.byte_size)
            .sum()
    }

    fn evict_sampled_textures_under_pressure(&mut self) {
        let mut total_bytes = self.total_sampled_texture_bytes();

        // Entries touched by an actual draw in this frame are pinned until the
        // next frame, including the entry uploaded immediately before this
        // pressure pass.
        let frame_number = self.frame.frame_number;
        let mut candidates = self
            .frame
            .sampled_texture_cache
            .iter()
            .filter_map(|(key, entry)| {
                (entry.last_used_frame != frame_number).then_some((*key, entry.last_used_frame))
            })
            .collect::<Vec<_>>();

        // --- Time-based eviction (Chromium TileManager-style) ---
        // Evict stale entries even when under the pressure threshold.
        if !candidates.is_empty() {
            let stale_keys = candidates
                .iter()
                .filter(|(_, last_used_frame)| {
                    frame_number.saturating_sub(*last_used_frame)
                        > Self::SAMPLED_TEXTURE_STALE_FRAMES
                })
                .map(|(key, _)| *key)
                .collect::<Vec<_>>();
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

        candidates.sort_by_key(|(_, last_used_frame)| *last_used_frame);

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
        self.frame
            .frame_buffer_pool
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
                    && self
                        .frame
                        .draw_rect_uniform_offset
                        .saturating_add(slot_size)
                        <= entry.size
            });
        if !has_current_capacity
            && self
                .frame
                .draw_rect_uniform_pool
                .get(self.frame.draw_rect_uniform_cursor)
                .is_some()
        {
            self.frame.draw_rect_uniform_cursor =
                self.frame.draw_rect_uniform_cursor.saturating_add(1);
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
            self.frame
                .draw_rect_uniform_pool
                .push(DrawRectUniformBufferEntry {
                    buffer,
                    size: required_size,
                    last_used_frame: self.frame.frame_number,
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
                    last_used_frame: self.frame.frame_number,
                    bind_groups: FxHashMap::default(),
                },
            );
            old.buffer.destroy();
        }
        let dynamic_offset = self.frame.draw_rect_uniform_offset;
        self.frame.draw_rect_uniform_pool[target_index].last_used_frame = self.frame.frame_number;
        let buffer = self.frame.draw_rect_uniform_pool[target_index]
            .buffer
            .clone();
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
        self.frame.draw_rect_uniform_offset = self
            .frame
            .draw_rect_uniform_offset
            .saturating_add(slot_size);
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
            GRADIENT_STOP_STRIDE, GRADIENT_STOPS_BUFFER_INITIAL_CAPACITY,
        };
        if stops.is_empty() {
            return None;
        }
        let device = self.gpu.device.as_ref()?.clone();
        let stop_bytes: &[u8] = bytemuck::cast_slice(stops);
        let byte_len = stop_bytes.len() as u64;
        let needed_end = self
            .frame
            .gradient_stops_byte_cursor
            .saturating_add(byte_len);

        let current_size = self
            .frame
            .gradient_stops_buffer
            .as_ref()
            .map(|e| e.size)
            .unwrap_or(0);
        let mut buffer_grew = false;
        if needed_end > current_size {
            let mut new_size = current_size
                .max(GRADIENT_STOPS_BUFFER_INITIAL_CAPACITY)
                .max(1);
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
                last_used_frame: self.frame.frame_number,
                last_high_usage_frame: self.frame.frame_number,
            });
            buffer_grew = true;
        }

        if buffer_grew {
            // Existing cached draw-rect bind groups reference the stale storage buffer.
            for entry in self.frame.draw_rect_uniform_pool.iter_mut() {
                entry.bind_groups.clear();
            }
        }

        let entry = self.frame.gradient_stops_buffer.as_mut()?;
        entry.last_used_frame = self.frame.frame_number;
        if needed_end.saturating_mul(2) > entry.size {
            entry.last_high_usage_frame = self.frame.frame_number;
        }
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
                last_used_frame: self.frame.frame_number,
                last_high_usage_frame: self.frame.frame_number,
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
        let uniform_buffer = self
            .frame
            .draw_rect_uniform_pool
            .get(pool_index)?
            .buffer
            .clone();
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
        self.frame
            .draw_rect_uniform_pool
            .get_mut(pool_index)?
            .bind_groups
            .insert(layout_cache_key, bg.clone());
        Some(bg)
    }

    pub fn release_render_resource_caches(&mut self) {
        self.invalidate_retained_surfaces();
        crate::view::render_pass::draw_rect_pass::clear_draw_rect_resources_cache();
        crate::view::render_pass::shadow_module::clear_shadow_resources_cache();
        crate::view::render_pass::text_pass::clear_text_resources_cache();
        crate::view::render_pass::blur_module::clear_blur_resources_cache();
        crate::view::render_pass::composite_layer_pass::clear_composite_layer_resources_cache();
        crate::view::render_pass::texture_composite_pass::clear_texture_composite_resources_cache(
            self.render_resource_scope_id(),
        );
        crate::view::render_pass::present_surface_pass::clear_present_surface_resources_cache();
        self.frame.offscreen_render_target_pool.clear();
        for entry in self.frame.sampled_texture_cache.values() {
            entry.texture.destroy();
        }
        self.frame.sampled_texture_cache.clear();
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
        #[cfg(not(target_arch = "wasm32"))]
        self.gpu.in_flight_submissions.clear();
    }
}

#[cfg(test)]
mod persistent_pair_witness_tests {
    use super::complete_persistent_pair_witness;

    #[test]
    fn root_effect_pair_witness_rejects_either_missing_or_incompatible_half() {
        assert!(complete_persistent_pair_witness(true, true));
        assert!(!complete_persistent_pair_witness(true, false));
        assert!(!complete_persistent_pair_witness(false, true));
        assert!(!complete_persistent_pair_witness(false, false));
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod sampled_texture_tests {
    use super::Viewport;
    use crate::view::ImageSampling;
    use crate::view::frame_graph::FrameGraph;
    use crate::view::render_pass::TextureCompositePass;
    use crate::view::render_pass::texture_composite_pass::{
        TextureCompositeInput, TextureCompositeOutput, TextureCompositeParams,
        texture_composite_resources_cache_len,
    };
    use crate::view::sampled_texture::{
        ImageAssetId, SampledTextureAlphaMode, SampledTextureId, SampledTextureUpload,
        SvgRasterAssetId,
    };
    use std::sync::Arc;

    fn request_gpu() -> Result<(wgpu::Instance, wgpu::Device, wgpu::Queue), String> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            flags: wgpu::InstanceFlags::empty(),
            memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
            backend_options: wgpu::BackendOptions::default(),
            display: None,
        });
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            compatible_surface: None,
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        }))
        .map_err(|error| format!("sampled texture test requires a GPU adapter: {error:?}"))?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("rfgui sampled texture residency test device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            experimental_features: wgpu::ExperimentalFeatures::default(),
            memory_hints: wgpu::MemoryHints::default(),
            trace: wgpu::Trace::Off,
        }))
        .map_err(|error| format!("failed to create sampled texture test device: {error:?}"))?;
        Ok((instance, device, queue))
    }

    fn request_two_devices() -> Result<
        (
            wgpu::Instance,
            wgpu::Device,
            wgpu::Queue,
            wgpu::Device,
            wgpu::Queue,
        ),
        String,
    > {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            flags: wgpu::InstanceFlags::empty(),
            memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
            backend_options: wgpu::BackendOptions::default(),
            display: None,
        });
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            compatible_surface: None,
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        }))
        .map_err(|error| format!("two-device test requires a GPU adapter: {error:?}"))?;
        let descriptor = wgpu::DeviceDescriptor {
            label: Some("rfgui TextureComposite scope test device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            experimental_features: wgpu::ExperimentalFeatures::default(),
            memory_hints: wgpu::MemoryHints::default(),
            trace: wgpu::Trace::Off,
        };
        let (first_device, first_queue) =
            pollster::block_on(adapter.request_device(&descriptor))
                .map_err(|error| format!("failed to create first test device: {error:?}"))?;
        let (second_device, second_queue) = pollster::block_on(adapter.request_device(&descriptor))
            .map_err(|error| format!("failed to create second test device: {error:?}"))?;
        Ok((
            instance,
            first_device,
            first_queue,
            second_device,
            second_queue,
        ))
    }

    fn attach_gpu(viewport: &mut Viewport, device: &wgpu::Device, queue: &wgpu::Queue) {
        viewport.gpu.device = Some(device.clone());
        viewport.gpu.queue = Some(queue.clone());
    }

    fn upload(id: SampledTextureId, generation: u64) -> SampledTextureUpload {
        SampledTextureUpload {
            id,
            generation,
            width: 2,
            height: 2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            alpha_mode: SampledTextureAlphaMode::Straight,
            pixels: Arc::from([255_u8; 16]),
            sampling: ImageSampling::Linear,
        }
    }

    fn execute_texture_composite(
        viewport: &mut Viewport,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        sampled_source: SampledTextureUpload,
    ) -> Result<(), String> {
        viewport.begin_offscreen_test_frame(
            device.clone(),
            queue.clone(),
            4,
            4,
            wgpu::TextureFormat::Rgba8UnormSrgb,
        )?;
        let mut graph = FrameGraph::new();
        graph.add_graphics_pass(TextureCompositePass::new(
            TextureCompositeParams {
                bounds: [0.0, 0.0, 2.0, 2.0],
                ..Default::default()
            },
            TextureCompositeInput::from_sampled_texture(
                sampled_source,
                Default::default(),
                Default::default(),
            ),
            TextureCompositeOutput::default(),
        ));
        graph
            .compile_with_upload(viewport)
            .map_err(|error| format!("TextureComposite graph compile failed: {error:?}"))?;
        graph
            .execute_profiled(viewport, false)
            .map_err(|error| format!("TextureComposite graph execute failed: {error:?}"))?;
        viewport.end_offscreen_test_frame()
    }

    #[test]
    #[ignore = "requires a native GPU adapter"]
    fn residency_is_per_viewport_generation_aware_and_recoverable() -> Result<(), String> {
        let (_instance, device, queue) = request_gpu()?;
        let id = SampledTextureId::Image(ImageAssetId::for_test(41));
        let first_generation = upload(id, 1);
        let mut first = Viewport::new();
        let mut second = Viewport::new();
        attach_gpu(&mut first, &device, &queue);
        attach_gpu(&mut second, &device, &queue);

        assert!(first.ensure_sampled_texture(&first_generation));
        assert_eq!(first.frame.sampled_texture_upload_count, 1);
        first.frame.frame_number += 1;
        assert!(first.ensure_sampled_texture(&first_generation));
        assert_eq!(first.frame.sampled_texture_upload_count, 1);

        assert!(second.ensure_sampled_texture(&first_generation));
        assert_eq!(second.frame.sampled_texture_upload_count, 1);

        let next_generation = upload(id, 2);
        assert!(first.ensure_sampled_texture(&next_generation));
        assert_eq!(first.frame.sampled_texture_upload_count, 2);

        first
            .frame
            .sampled_texture_cache
            .get_mut(&id)
            .unwrap()
            .byte_size = Viewport::SAMPLED_TEXTURE_PRESSURE_BYTES + 1;
        first.frame.frame_number += 1;
        first.evict_sampled_textures_under_pressure();
        assert!(!first.frame.sampled_texture_cache.contains_key(&id));
        assert!(first.ensure_sampled_texture(&next_generation));
        assert_eq!(first.frame.sampled_texture_upload_count, 3);

        first.release_render_resource_caches();
        assert!(first.ensure_sampled_texture(&next_generation));
        assert_eq!(first.frame.sampled_texture_upload_count, 4);
        Ok(())
    }

    #[test]
    #[ignore = "requires a native GPU adapter"]
    fn current_frame_is_pinned_and_equal_image_svg_ids_do_not_alias() -> Result<(), String> {
        let (_instance, device, queue) = request_gpu()?;
        let mut viewport = Viewport::new();
        attach_gpu(&mut viewport, &device, &queue);
        let image_id = SampledTextureId::Image(ImageAssetId::for_test(9));
        let svg_id = SampledTextureId::SvgRaster(SvgRasterAssetId::for_test(9));

        assert!(viewport.ensure_sampled_texture(&upload(image_id, 1)));
        viewport
            .frame
            .sampled_texture_cache
            .get_mut(&image_id)
            .unwrap()
            .byte_size = Viewport::SAMPLED_TEXTURE_PRESSURE_BYTES + 1;
        assert!(viewport.ensure_sampled_texture(&upload(image_id, 1)));
        assert!(viewport.frame.sampled_texture_cache.contains_key(&image_id));

        assert!(viewport.ensure_sampled_texture(&upload(svg_id, 1)));
        assert!(viewport.frame.sampled_texture_cache.contains_key(&image_id));
        assert!(viewport.frame.sampled_texture_cache.contains_key(&svg_id));
        assert_eq!(viewport.frame.sampled_texture_cache.len(), 2);
        Ok(())
    }

    #[test]
    #[ignore = "requires two native GPU devices"]
    fn texture_composite_resources_are_device_scoped_and_drop_reclaimed() -> Result<(), String> {
        let (_instance, first_device, first_queue, second_device, second_queue) =
            request_two_devices()?;
        let baseline = texture_composite_resources_cache_len();
        let mut first = Viewport::new();
        let mut second = Viewport::new();

        execute_texture_composite(
            &mut first,
            &first_device,
            &first_queue,
            upload(SampledTextureId::Image(ImageAssetId::for_test(71)), 1),
        )?;
        assert_eq!(texture_composite_resources_cache_len(), baseline + 1);
        execute_texture_composite(
            &mut second,
            &second_device,
            &second_queue,
            upload(SampledTextureId::Image(ImageAssetId::for_test(72)), 1),
        )?;
        assert_eq!(texture_composite_resources_cache_len(), baseline + 2);

        first.release_render_resource_caches();
        assert_eq!(texture_composite_resources_cache_len(), baseline + 1);
        drop(first);
        execute_texture_composite(
            &mut second,
            &second_device,
            &second_queue,
            upload(SampledTextureId::Image(ImageAssetId::for_test(72)), 1),
        )?;
        assert_eq!(texture_composite_resources_cache_len(), baseline + 1);

        drop(second);
        assert_eq!(texture_composite_resources_cache_len(), baseline);
        Ok(())
    }
}
