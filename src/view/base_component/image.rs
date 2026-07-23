use crate::style::{ComputedStyle, ParsedValue, PropertyId, Style};
use crate::view::frame_graph::FrameGraph;
use crate::view::image_resource::{
    ImageHandle, ImageSnapshot, acquire_image_resource, snapshot_image,
};
use crate::view::render_pass::TextureCompositePass;
use crate::view::render_pass::texture_composite_pass::{
    TextureCompositeInput, TextureCompositeOutput, TextureCompositeParams,
};
use crate::view::sampled_texture::{SampledTextureAlphaMode, SampledTextureUpload};
use crate::view::{ImageFit, ImageSampling, ImageSource};

use super::{
    BoxModelSnapshot, ComputedStyleConsumer, Element, ElementStyleSnapshot, ElementTrait,
    EventTarget, LayoutConstraints, LayoutPlacement, Layoutable, Renderable, UiBuildContext,
    round_layout_value,
};
use crate::view::node_arena::{NodeArena, NodeKey};
use rustc_hash::FxHashSet;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use super::resource_slot::{self, ActiveSlot, SlotReplacementError};

const PLACEHOLDER_SIZE: f32 = 120.0;

pub(super) fn hash_image_snapshot<H: Hasher>(snapshot: Option<&ImageSnapshot>, hasher: &mut H) {
    match snapshot {
        None => 0_u8.hash(hasher),
        Some(ImageSnapshot::Loading) => 1_u8.hash(hasher),
        Some(ImageSnapshot::Ready(image)) => {
            2_u8.hash(hasher);
            image.sampled_texture_id.hash(hasher);
            image.width.hash(hasher);
            image.height.hash(hasher);
            image.generation.hash(hasher);
        }
        Some(ImageSnapshot::Error(message)) => {
            3_u8.hash(hasher);
            message.as_ref().hash(hasher);
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum ImageSnapshotIdentity<'a> {
    Missing,
    Loading,
    Ready {
        id: crate::view::sampled_texture::SampledTextureId,
        width: u32,
        height: u32,
        generation: u64,
        pixel_len: usize,
    },
    Error(&'a str),
}

#[derive(Clone, Debug)]
enum ImageShadowPaintClass {
    ReadyExact(crate::view::paint::PreparedImageOp),
    ActiveSlotWrapper(ActiveSlot),
}

fn image_snapshot_identity(snapshot: Option<&ImageSnapshot>) -> ImageSnapshotIdentity<'_> {
    match snapshot {
        None => ImageSnapshotIdentity::Missing,
        Some(ImageSnapshot::Loading) => ImageSnapshotIdentity::Loading,
        Some(ImageSnapshot::Ready(image)) => ImageSnapshotIdentity::Ready {
            id: image.sampled_texture_id,
            width: image.width,
            height: image.height,
            generation: image.generation,
            pixel_len: image.pixels.len(),
        },
        Some(ImageSnapshot::Error(message)) => ImageSnapshotIdentity::Error(message),
    }
}

pub struct Image {
    element: Element,
    fit: ImageFit,
    sampling: ImageSampling,
    source_handle: ImageHandle,
    /// Pending loading-slot wrapper keys (detached from `Element.children`
    /// until `sync_active_slot` activates them). Live in the same arena as
    /// the owning Image but not traversed while inactive.
    loading_slot: Vec<NodeKey>,
    error_slot: Vec<NodeKey>,
    active_slot: ActiveSlot,
    /// Frame-frozen resource truth. The viewport refreshes this once from the
    /// pre-layout arena sync hook; layout, recording and legacy build all read
    /// the same immutable snapshot for the rest of that frame.
    frozen_snapshot: Option<ImageSnapshot>,
    prepared_by_arena_sync: bool,
}

impl Image {
    #[cfg(test)]
    pub(crate) fn set_layout_transition_width_for_test(&mut self, width: f32) {
        self.element.set_layout_transition_width(width);
    }

    #[cfg(test)]
    pub(crate) fn set_resource_loading_for_test(&self) {
        crate::view::image_resource::set_image_loading_for_test(self.source_handle.asset_id());
    }

    #[cfg(test)]
    pub(crate) fn set_resource_error_for_test(&self) {
        crate::view::image_resource::set_image_error_for_test(
            self.source_handle.asset_id(),
            "synthetic retained transform error",
        );
    }

    pub fn new_with_id(id: u64, source: ImageSource) -> Self {
        let mut element = Element::new_with_id(id, 0.0, 0.0, PLACEHOLDER_SIZE, PLACEHOLDER_SIZE);
        let mut base_style = Style::new();
        base_style.insert(PropertyId::Width, ParsedValue::Auto);
        base_style.insert(PropertyId::Height, ParsedValue::Auto);
        element.apply_style(base_style);
        let source_handle = acquire_image_resource(&source);
        let frozen_snapshot = snapshot_image(source_handle.asset_id());
        Self {
            element,
            source_handle,
            fit: ImageFit::Contain,
            sampling: ImageSampling::Linear,
            loading_slot: Vec::new(),
            error_slot: Vec::new(),
            active_slot: ActiveSlot::None,
            frozen_snapshot,
            prepared_by_arena_sync: false,
        }
    }

    pub fn set_fit(&mut self, fit: ImageFit) {
        if self.fit == fit {
            return;
        }
        self.fit = fit;
        self.element.mark_paint_dirty();
    }

    pub fn set_sampling(&mut self, sampling: ImageSampling) {
        if self.sampling == sampling {
            return;
        }
        self.sampling = sampling;
        self.element.mark_paint_dirty();
    }

    /// 軌 1 #4: hot-swap the image source. Dropping the old
    /// `ImageHandle` via its `Drop` impl releases the resource entry;
    /// the next pre-layout sync (or direct `measure`) freezes the new
    /// source state before layout and paint consume it.
    pub fn set_source(&mut self, source: ImageSource) {
        let next = acquire_image_resource(&source);
        if next.asset_id() == self.source_handle.asset_id() {
            return;
        }
        self.source_handle = next;
        self.frozen_snapshot = None;
        self.prepared_by_arena_sync = false;
        self.element.mark_layout_dirty();
    }

    pub fn apply_style(&mut self, style: crate::style::Style) {
        self.element.apply_style(style);
    }

    /// Cold-commit hook for a pre-committed loading-slot wrapper. This is not
    /// a runtime mutation API: the host must still be inactive and the target
    /// slot must be empty. The descriptor committer owns those invariants.
    pub(crate) fn attach_loading_slot_cold(&mut self, slot: Vec<NodeKey>) {
        resource_slot::attach_slot_cold(self.active_slot, &mut self.loading_slot, slot);
    }

    pub(crate) fn attach_error_slot_cold(&mut self, slot: Vec<NodeKey>) {
        resource_slot::attach_slot_cold(self.active_slot, &mut self.error_slot, slot);
    }

    /// M4 #3 incremental hot-swap: replace the loading or error slot
    /// in-place. Called from `fiber_work` when the reconciler surfaces
    /// a changed `loading` / `error` prop on an `<Image>`.
    ///
    /// Sequence:
    /// 1. Preflight every new root (live, unique, parented to `owner`, and
    ///    disjoint from the other slot) before mutating old topology.
    /// 2. Drain any currently-active slot back into its Vec so
    ///    `Element.children` is empty — otherwise the active subtree
    ///    would become orphaned when we overwrite the storage Vec.
    /// 3. Take the old keys out of the target slot and
    ///    `remove_subtree` each one to free arena storage.
    /// 4. Install `new_keys`, mirror topology, and invalidate layout.
    /// 5. Let the next pre-layout arena sync install the slot from the
    ///    frame-frozen `ImageSnapshot`.
    pub(crate) fn replace_loading_slot_incremental(
        &mut self,
        arena: &mut NodeArena,
        owner: NodeKey,
        new_keys: &[NodeKey],
    ) -> Result<(), SlotReplacementError> {
        resource_slot::replace_slot(
            arena,
            owner,
            &mut self.element,
            &mut self.loading_slot,
            &mut self.error_slot,
            &mut self.active_slot,
            ActiveSlot::Loading,
            new_keys,
        )
    }

    pub(crate) fn replace_error_slot_incremental(
        &mut self,
        arena: &mut NodeArena,
        owner: NodeKey,
        new_keys: &[NodeKey],
    ) -> Result<(), SlotReplacementError> {
        resource_slot::replace_slot(
            arena,
            owner,
            &mut self.element,
            &mut self.loading_slot,
            &mut self.error_slot,
            &mut self.active_slot,
            ActiveSlot::Error,
            new_keys,
        )
    }

    /// Test/debug accessor: count slot keys held in the loading Vec
    /// when inactive, or `Element.children.len()` when active.
    #[cfg(test)]
    pub(crate) fn loading_slot_len(&self) -> usize {
        use crate::view::base_component::ElementTrait;
        if matches!(self.active_slot, ActiveSlot::Loading) {
            self.element.children().len()
        } else {
            self.loading_slot.len()
        }
    }

    fn frozen_snapshot(&self) -> ImageSnapshot {
        self.frozen_snapshot
            .clone()
            .unwrap_or(ImageSnapshot::Loading)
    }

    fn refresh_frozen_resource(&mut self, arena: &mut NodeArena) {
        let next = snapshot_image(self.source_handle.asset_id()).unwrap_or(ImageSnapshot::Loading);
        let resource_changed = image_snapshot_identity(self.frozen_snapshot.as_ref())
            != image_snapshot_identity(Some(&next));
        let next_slot = Self::resolve_slot(&next);
        let slot_changed = self.active_slot != next_slot;
        self.sync_active_slot(arena, next_slot);
        self.frozen_snapshot = Some(next);
        if resource_changed || slot_changed {
            self.element.mark_layout_dirty();
        }
    }

    fn sync_active_slot(&mut self, arena: &mut NodeArena, next_slot: ActiveSlot) {
        let self_key = arena.find_by_stable_id(self.element.stable_id());
        resource_slot::sync_active_slot(
            arena,
            self_key,
            &mut self.element,
            &mut self.loading_slot,
            &mut self.error_slot,
            &mut self.active_slot,
            next_slot,
        );
    }

    fn intrinsic_size(snapshot: &ImageSnapshot) -> (f32, f32) {
        match snapshot {
            ImageSnapshot::Ready(image) => (image.width.max(1) as f32, image.height.max(1) as f32),
            ImageSnapshot::Loading | ImageSnapshot::Error(_) => {
                (PLACEHOLDER_SIZE, PLACEHOLDER_SIZE)
            }
        }
    }

    fn resolve_slot(snapshot: &ImageSnapshot) -> ActiveSlot {
        match snapshot {
            ImageSnapshot::Loading => ActiveSlot::Loading,
            ImageSnapshot::Error(message) => {
                let _ = message;
                ActiveSlot::Error
            }
            ImageSnapshot::Ready(_) => ActiveSlot::None,
        }
    }

    fn frozen_upload(&self) -> Option<SampledTextureUpload> {
        let ImageSnapshot::Ready(image) = self.frozen_snapshot.as_ref()? else {
            return None;
        };
        let expected_id =
            crate::view::sampled_texture::SampledTextureId::Image(self.source_handle.asset_id());
        if image.sampled_texture_id != expected_id {
            return None;
        }
        let upload = SampledTextureUpload {
            id: image.sampled_texture_id,
            generation: image.generation,
            width: image.width,
            height: image.height,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            alpha_mode: SampledTextureAlphaMode::Straight,
            pixels: image.pixels.clone(),
            sampling: self.sampling,
        };
        upload.validate_rgba8()?;
        Some(upload)
    }

    fn classify_shadow_paint(
        &self,
        arena: &NodeArena,
        expected_owner: Option<NodeKey>,
        properties: Option<crate::view::compositor::property_tree::PropertyTreeState>,
        deferred_phase_root: bool,
        recording_context: crate::view::paint::PaintRecordingContext,
    ) -> Result<ImageShadowPaintClass, super::ShadowPaintBlocker> {
        let indexed_owner = arena
            .find_by_stable_id(self.stable_id())
            .ok_or(super::ShadowPaintBlocker::MissingPreparedImage)?;
        let owner = expected_owner.unwrap_or(indexed_owner);
        if owner != indexed_owner
            || self.element.children() != arena.children_of(owner)
            || !arena.contains_key(owner)
        {
            return Err(super::ShadowPaintBlocker::MissingPreparedImage);
        }

        match (&self.frozen_snapshot, self.active_slot) {
            (Some(ImageSnapshot::Ready(_)), ActiveSlot::None) => {
                if let Some(blocker) = self.element.shadow_paint_blocker(
                    arena,
                    deferred_phase_root,
                    recording_context.authorizes_self_clip_for(self.stable_id())
                        || recording_context
                            .authorizes_deferred_viewport_self_clip_for(self.stable_id()),
                    true,
                    recording_context,
                ) {
                    return Err(blocker);
                }
                if let Some(properties) = properties {
                    if properties.transform.is_some()
                        && !recording_context
                            .authorizes_transform_surface_owner(properties.transform)
                    {
                        return Err(super::ShadowPaintBlocker::Transform);
                    }
                    if properties.scroll.is_some()
                        && !recording_context
                            .authorizes_nested_scroll_content_properties(owner, properties)
                    {
                        return Err(super::ShadowPaintBlocker::ScrollContainer);
                    }
                }
                if !self.element.children().is_empty() {
                    return Err(super::ShadowPaintBlocker::MissingPreparedImage);
                }
                let mut inactive_roots = FxHashSet::default();
                for &inactive_root in self.loading_slot.iter().chain(self.error_slot.iter()) {
                    if !inactive_roots.insert(inactive_root)
                        || !arena.contains_key(inactive_root)
                        || arena.parent_of(inactive_root) != Some(owner)
                    {
                        return Err(super::ShadowPaintBlocker::MissingPreparedImage);
                    }
                }
                let prepared = self
                    .prepared_image_op(
                        recording_context.paint_offset,
                        recording_context
                            .paint_opacity(self.element.retained_paint_properties().opacity),
                    )
                    .ok_or(super::ShadowPaintBlocker::MissingPreparedImage)?;
                Ok(ImageShadowPaintClass::ReadyExact(prepared))
            }
            (Some(ImageSnapshot::Loading), ActiveSlot::Loading)
            | (Some(ImageSnapshot::Error(_)), ActiveSlot::Error) => {
                if let Some(blocker) = self.element.shadow_paint_blocker(
                    arena,
                    // Authorize only the wrapper's canonical zero-blur
                    // outer-shadow payload under an exact recorder-owned clip.
                    deferred_phase_root,
                    recording_context.authorizes_self_clip_for(self.stable_id())
                        || recording_context
                            .authorizes_deferred_viewport_self_clip_for(self.stable_id()),
                    true,
                    recording_context,
                ) {
                    return Err(blocker);
                }
                if let Some(properties) = properties {
                    if properties.transform.is_some()
                        && !recording_context
                            .authorizes_transform_surface_owner(properties.transform)
                    {
                        return Err(super::ShadowPaintBlocker::Transform);
                    }
                    if properties.scroll.is_some()
                        && !recording_context
                            .authorizes_nested_scroll_content_properties(owner, properties)
                    {
                        return Err(super::ShadowPaintBlocker::ScrollContainer);
                    }
                }
                if let Some(effect) = properties.and_then(|properties| properties.effect)
                    && !matches!(
                        recording_context.opacity_authority,
                        crate::view::paint::PaintOpacityAuthority::NeutralRootEffect(authority)
                            if authority == effect
                    )
                {
                    return Err(super::ShadowPaintBlocker::StatefulPaint);
                }
                let active_target_is_empty = match self.active_slot {
                    ActiveSlot::Loading => self.loading_slot.is_empty(),
                    ActiveSlot::Error => self.error_slot.is_empty(),
                    ActiveSlot::None => false,
                };
                if !active_target_is_empty {
                    return Err(super::ShadowPaintBlocker::MissingPreparedImage);
                }

                let mut active_reachable = FxHashSet::default();
                let mut active_stack = self
                    .element
                    .children()
                    .iter()
                    .rev()
                    .copied()
                    .map(|child| (owner, child))
                    .collect::<Vec<_>>();
                while let Some((expected_parent, key)) = active_stack.pop() {
                    if !active_reachable.insert(key) {
                        return Err(super::ShadowPaintBlocker::MissingPreparedImage);
                    }
                    let node = arena
                        .get(key)
                        .ok_or(super::ShadowPaintBlocker::MissingPreparedImage)?;
                    if node.parent() != Some(expected_parent)
                        || node.element.children() != node.children()
                    {
                        return Err(super::ShadowPaintBlocker::MissingPreparedImage);
                    }
                    let children = node.children().to_vec();
                    drop(node);
                    active_stack.extend(children.into_iter().rev().map(|child| (key, child)));
                }

                let mut inactive_roots = FxHashSet::default();
                for &inactive_root in self.loading_slot.iter().chain(self.error_slot.iter()) {
                    if active_reachable.contains(&inactive_root)
                        || !inactive_roots.insert(inactive_root)
                        || !arena.contains_key(inactive_root)
                        || arena.parent_of(inactive_root) != Some(owner)
                    {
                        return Err(super::ShadowPaintBlocker::MissingPreparedImage);
                    }
                }
                Ok(ImageShadowPaintClass::ActiveSlotWrapper(self.active_slot))
            }
            _ => Err(super::ShadowPaintBlocker::MissingPreparedImage),
        }
    }

    fn has_canonical_culled_subtree_state(&self, arena: &NodeArena) -> bool {
        if self.element.layout_state.should_render {
            return false;
        }
        let Some(owner) = arena.find_by_stable_id(self.stable_id()) else {
            return false;
        };
        if !arena.contains_key(owner) || self.element.children() != arena.children_of(owner) {
            return false;
        }

        match (&self.frozen_snapshot, self.active_slot) {
            (Some(ImageSnapshot::Ready(_)), ActiveSlot::None) => {
                if !self.element.children().is_empty() || self.frozen_upload().is_none() {
                    return false;
                }
            }
            (Some(ImageSnapshot::Loading), ActiveSlot::Loading)
            | (Some(ImageSnapshot::Error(_)), ActiveSlot::Error) => {
                let active_target_is_empty = match self.active_slot {
                    ActiveSlot::Loading => self.loading_slot.is_empty(),
                    ActiveSlot::Error => self.error_slot.is_empty(),
                    ActiveSlot::None => false,
                };
                if !active_target_is_empty {
                    return false;
                }
            }
            _ => return false,
        }

        let mut active_reachable = FxHashSet::default();
        let mut active_stack = self
            .element
            .children()
            .iter()
            .rev()
            .copied()
            .map(|child| (owner, child))
            .collect::<Vec<_>>();
        while let Some((expected_parent, key)) = active_stack.pop() {
            if !active_reachable.insert(key) {
                return false;
            }
            let Some(node) = arena.get(key) else {
                return false;
            };
            if node.parent() != Some(expected_parent) || node.element.children() != node.children()
            {
                return false;
            }
            active_stack.extend(
                node.children()
                    .iter()
                    .rev()
                    .copied()
                    .map(|child| (key, child)),
            );
        }

        let mut inactive_roots = FxHashSet::default();
        self.loading_slot
            .iter()
            .chain(self.error_slot.iter())
            .all(|&inactive_root| {
                !active_reachable.contains(&inactive_root)
                    && inactive_roots.insert(inactive_root)
                    && arena.contains_key(inactive_root)
                    && arena.parent_of(inactive_root) == Some(owner)
            })
    }

    fn prepared_image_op_with_upload(
        &self,
        upload: SampledTextureUpload,
        paint_offset: [f32; 2],
        opacity: f32,
    ) -> Option<crate::view::paint::PreparedImageOp> {
        let (inner_x, inner_y, inner_w, inner_h) = self.element.inner_content_rect_for_render();
        if inner_w <= 0.0 || inner_h <= 0.0 {
            return None;
        }
        let (local_draw_bounds, uv_bounds) = compute_image_mapping(
            self.fit,
            upload.width as f32,
            upload.height as f32,
            inner_w,
            inner_h,
        );
        Some(crate::view::paint::PreparedImageOp {
            params: TextureCompositeParams {
                bounds: [
                    inner_x + local_draw_bounds[0] + paint_offset[0],
                    inner_y + local_draw_bounds[1] + paint_offset[1],
                    local_draw_bounds[2],
                    local_draw_bounds[3],
                ],
                quad_positions: None,
                uv_bounds: Some(uv_bounds),
                mask_uv_bounds: None,
                use_mask: false,
                source_is_premultiplied: false,
                opacity: opacity.clamp(0.0, 1.0),
                scissor_rect: None,
            },
            upload,
        })
    }

    fn prepared_image_op(
        &self,
        paint_offset: [f32; 2],
        opacity: f32,
    ) -> Option<crate::view::paint::PreparedImageOp> {
        self.prepared_image_op_with_upload(self.frozen_upload()?, paint_offset, opacity)
    }

    fn apply_intrinsic_measurement(
        &mut self,
        constraints: LayoutConstraints,
        intrinsic: (f32, f32),
    ) {
        let width_auto = self.element.width_is_auto();
        let height_auto = self.element.height_is_auto();
        let ratio = if intrinsic.1 <= 0.0 {
            1.0
        } else {
            (intrinsic.0 / intrinsic.1).max(0.0001)
        };
        let measured = self.element.measured_size();

        match (width_auto, height_auto) {
            (true, true) => {
                self.element.set_size(
                    intrinsic.0.min(constraints.max_width).max(1.0),
                    intrinsic.1.min(constraints.max_height).max(1.0),
                );
            }
            (true, false) => {
                let height = measured.1.max(1.0);
                self.element
                    .set_width((height * ratio).min(constraints.max_width).max(1.0));
            }
            (false, true) => {
                let width = measured.0.max(1.0);
                self.element
                    .set_height((width / ratio).min(constraints.max_height).max(1.0));
            }
            (false, false) => {}
        }
    }
}

impl ComputedStyleConsumer for Image {
    type Snapshot = ElementStyleSnapshot;

    fn apply_computed_style(
        &mut self,
        computed: ComputedStyle,
        previous_snapshot: Option<&ElementStyleSnapshot>,
    ) {
        ComputedStyleConsumer::apply_computed_style(&mut self.element, computed, previous_snapshot);
    }
}

impl ElementTrait for Image {
    fn stable_id(&self) -> u64 {
        self.element.stable_id()
    }

    fn retained_scroll_normalized_paint_capability(
        &self,
    ) -> Option<super::RetainedScrollNormalizedPaintCapability> {
        Some(super::RetainedScrollNormalizedPaintCapability::native(
            super::RetainedScrollNormalizedPaintKind::Image,
        ))
    }

    fn exact_retained_self_clip_scissor_rect(
        &self,
        owner: NodeKey,
        arena: &NodeArena,
        is_frame_root: bool,
    ) -> Option<[u32; 4]> {
        self.element
            .exact_anchor_parent_leaf_self_clip_scissor_rect(owner, arena, is_frame_root)
            .or_else(|| {
                self.element
                    .exact_deferred_viewport_root_self_clip_scissor_rect(
                        owner,
                        arena,
                        is_frame_root,
                    )
            })
    }

    fn retained_absolute_clip_mode_witness(
        &self,
        owner: NodeKey,
        arena: &NodeArena,
    ) -> super::RetainedAbsoluteClipModeWitness {
        self.element
            .retained_absolute_clip_mode_witness(owner, arena)
    }

    fn box_model_snapshot(&self) -> BoxModelSnapshot {
        self.element.box_model_snapshot()
    }

    fn retained_paint_properties(&self) -> super::RetainedPaintProperties {
        self.element.retained_paint_properties()
    }

    fn admits_exact_retained_root_opacity_artifact(&self) -> bool {
        true
    }

    fn tick_post_layout_animation_frame(&mut self, now: crate::time::Instant) -> super::DirtyFlags {
        self.element.tick_post_layout_animation_frame(now)
    }

    fn placement_eligibility_metadata(
        &self,
    ) -> crate::view::node_arena::PlacementEligibilityMetadata {
        self.element.placement_eligibility_metadata()
    }

    fn retained_sampled_layout_transition_snapshot(
        &self,
    ) -> Option<super::RetainedSampledLayoutTransitionSnapshot> {
        self.element
            .exact_sampled_layout_transition_snapshot_for_paint_signature(
                self.retained_paint_signature(),
            )
    }

    fn last_placement(&self) -> Option<crate::view::base_component::LayoutPlacement> {
        self.element.last_placement()
    }

    #[allow(private_interfaces)]
    fn inline_atomic_measurement_snapshot(
        &self,
    ) -> Option<crate::view::inline_formatting_context::InlineIfcMeasuredAtomicBox> {
        self.element
            .inline_atomic_measurement_snapshot_with_intrinsic(Some(self.measured_size()))
    }

    fn inline_atomic_vertical_align(&self) -> Option<crate::style::VerticalAlign> {
        self.element.inline_atomic_vertical_align()
    }

    fn hit_test_clip_rect(&self) -> Option<crate::view::base_component::Rect> {
        self.element.hit_test_clip_rect()
    }

    fn translate_in_place(&mut self, dx: f32, dy: f32) {
        self.element.translate_in_place(dx, dy);
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn children(&self) -> &[NodeKey] {
        self.element.children()
    }

    fn sync_children_mirror(&mut self, children: &[NodeKey]) {
        self.element.sync_children_mirror(children);
    }

    #[allow(private_interfaces)]
    fn shadow_paint_recording_capability(
        &self,
        arena: &NodeArena,
        deferred_phase_root: bool,
        recording_context: crate::view::paint::PaintRecordingContext,
    ) -> super::ShadowPaintRecordingCapability {
        if !self.element.layout_state.should_render {
            let paint = self.element.retained_paint_properties();
            let blocker = if self.element.has_retained_transform_surface()
                && !recording_context.authorizes_transform_surface_root(self.stable_id())
            {
                Some(super::ShadowPaintBlocker::Transform)
            } else if self.element.is_deferred_to_root_viewport_render() || deferred_phase_root {
                Some(super::ShadowPaintBlocker::Deferred)
            } else if paint.is_scroll_container {
                Some(super::ShadowPaintBlocker::ScrollContainer)
            } else if paint.opacity.to_bits() != 1.0_f32.to_bits()
                || !matches!(
                    recording_context.opacity_authority,
                    crate::view::paint::PaintOpacityAuthority::Baked
                )
            {
                Some(super::ShadowPaintBlocker::StatefulPaint)
            } else {
                None
            };
            if let Some(blocker) = blocker {
                return super::ShadowPaintRecordingCapability::Legacy(blocker);
            }
            return if self.has_canonical_culled_subtree_state(arena) {
                super::ShadowPaintRecordingCapability::CulledSubtree
            } else {
                super::ShadowPaintRecordingCapability::Legacy(
                    super::ShadowPaintBlocker::MissingPreparedImage,
                )
            };
        }
        match self.classify_shadow_paint(arena, None, None, deferred_phase_root, recording_context)
        {
            Ok(_) => super::ShadowPaintRecordingCapability::Recordable,
            Err(blocker) => super::ShadowPaintRecordingCapability::Legacy(blocker),
        }
    }

    #[allow(private_interfaces)]
    fn retained_child_mask_plan(
        &self,
        arena: &NodeArena,
        recording_context: crate::view::paint::PaintRecordingContext,
    ) -> Option<crate::view::paint::RetainedChildMaskPlan> {
        self.element
            .prepared_retained_child_mask_plan(arena, recording_context)
    }

    #[allow(private_interfaces)]
    fn record_shadow_paint_metadata(
        &self,
        owner: NodeKey,
        properties: crate::view::compositor::property_tree::PropertyTreeState,
        content_revision: crate::view::paint::PaintContentRevision,
        arena: &NodeArena,
        recording_context: crate::view::paint::PaintRecordingContext,
    ) -> Option<crate::view::paint::PaintChunkMetadata> {
        match self
            .classify_shadow_paint(
                arena,
                Some(owner),
                Some(properties),
                recording_context.authorizes_deferred_viewport_self_clip_for(self.stable_id()),
                recording_context,
            )
            .ok()?
        {
            ImageShadowPaintClass::ReadyExact(prepared) => {
                let mut metadata = self
                    .element
                    .record_shadow_node_paint_metadata(
                        owner,
                        properties,
                        content_revision,
                        Some(arena),
                        recording_context,
                    )
                    .ok()?;
                metadata.id.role = crate::view::paint::PaintChunkRole::ImageContent;
                let decoration = self
                    .element
                    .self_decoration_paint_ops(
                        prepared.params.opacity,
                        recording_context.paint_offset,
                    )
                    .into_iter()
                    .collect::<Vec<_>>();
                let shadows = self.element.prepared_outer_shadow_ops(recording_context)?;
                metadata.payload_identity =
                    crate::view::paint::PaintPayloadIdentity::image_with_shadows_and_decoration(
                        crate::view::paint::PreparedImageIdentity::from_op(&prepared),
                        shadows.iter(),
                        decoration.iter(),
                    )?;
                Some(metadata)
            }
            ImageShadowPaintClass::ActiveSlotWrapper(_slot) => self
                .element
                .record_shadow_node_paint_metadata(
                    owner,
                    properties,
                    content_revision,
                    Some(arena),
                    recording_context,
                )
                .ok(),
        }
    }

    #[allow(private_interfaces)]
    fn record_shadow_paint_artifact(
        &self,
        owner: NodeKey,
        properties: crate::view::compositor::property_tree::PropertyTreeState,
        content_revision: crate::view::paint::PaintContentRevision,
        arena: &NodeArena,
        recording_context: crate::view::paint::PaintRecordingContext,
    ) -> Option<crate::view::paint::PaintArtifact> {
        let classification = self
            .classify_shadow_paint(
                arena,
                Some(owner),
                Some(properties),
                recording_context.authorizes_deferred_viewport_self_clip_for(self.stable_id()),
                recording_context,
            )
            .ok()?;
        let artifact = match classification {
            ImageShadowPaintClass::ReadyExact(prepared) => {
                let mut metadata = self
                    .element
                    .record_shadow_node_paint_metadata(
                        owner,
                        properties,
                        content_revision,
                        Some(arena),
                        recording_context,
                    )
                    .ok()?;
                metadata.id.role = crate::view::paint::PaintChunkRole::ImageContent;
                let mut ops = self
                    .element
                    .prepared_outer_shadow_ops(recording_context)?
                    .into_iter()
                    .map(crate::view::paint::PaintOp::PreparedShadow)
                    .collect::<Vec<_>>();
                ops.extend(
                    self.element
                        .self_decoration_paint_ops(
                            prepared.params.opacity,
                            recording_context.paint_offset,
                        )
                        .into_iter()
                        .map(crate::view::paint::PaintOp::DrawRect),
                );
                let shadow_count = ops
                    .iter()
                    .take_while(|op| matches!(op, crate::view::paint::PaintOp::PreparedShadow(_)))
                    .count();
                metadata.payload_identity =
                    crate::view::paint::PaintPayloadIdentity::image_with_shadows_and_decoration(
                        crate::view::paint::PreparedImageIdentity::from_op(&prepared),
                        ops[..shadow_count].iter().filter_map(|op| match op {
                            crate::view::paint::PaintOp::PreparedShadow(shadow) => Some(shadow),
                            _ => None,
                        }),
                        ops[shadow_count..].iter().filter_map(|op| match op {
                            crate::view::paint::PaintOp::DrawRect(rect) => Some(rect),
                            _ => None,
                        }),
                    )?;
                ops.push(crate::view::paint::PaintOp::PreparedImage(prepared));
                crate::view::paint::PaintArtifact {
                    target: Default::default(),
                    chunks: vec![crate::view::paint::PaintChunk {
                        id: metadata.id,
                        owner: metadata.owner,
                        op_range: 0..ops.len(),
                        bounds: metadata.bounds,
                        properties: metadata.properties,
                        content_revision: metadata.content_revision,
                        payload_identity: metadata.payload_identity,
                    }],
                    ops,
                    clip_nodes: Vec::new(),
                    effect_nodes: Vec::new(),
                    owner_nodes: vec![crate::view::paint::PaintOwnerSnapshot {
                        owner,
                        parent: None,
                    }],
                }
            }
            ImageShadowPaintClass::ActiveSlotWrapper(_slot) => self
                .element
                .record_shadow_node_paint_artifact(
                    owner,
                    properties,
                    content_revision,
                    arena,
                    recording_context,
                )
                .ok()?,
        };
        #[cfg(test)]
        crate::view::paint::note_full_artifact_record();
        Some(artifact)
    }

    #[allow(private_interfaces)]
    fn shadow_paint_recording_context(
        &self,
        parent: crate::view::paint::PaintRecordingContext,
    ) -> crate::view::paint::PaintRecordingContext {
        self.element.shadow_paint_recording_context(parent)
    }

    fn intercepts_pointer_at(&self, viewport_x: f32, viewport_y: f32) -> bool {
        self.element.intercepts_pointer_at(viewport_x, viewport_y)
    }

    fn hit_test_visible_at(&self, viewport_x: f32, viewport_y: f32) -> bool {
        self.element.hit_test_visible_at(viewport_x, viewport_y)
    }

    fn has_active_animator(&self) -> bool {
        self.element.has_active_animator()
    }

    fn is_deferred_to_root_viewport_render(&self) -> bool {
        self.element.is_deferred_to_root_viewport_render()
    }

    fn retained_paint_signature(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.element.retained_paint_signature().hash(&mut hasher);
        self.source_handle.asset_id().hash(&mut hasher);
        match self.fit {
            ImageFit::Contain => 0_u8,
            ImageFit::Cover => 1_u8,
            ImageFit::Fill => 2_u8,
        }
        .hash(&mut hasher);
        match self.sampling {
            ImageSampling::Linear => 0_u8,
            ImageSampling::Nearest => 1_u8,
        }
        .hash(&mut hasher);
        match self.active_slot {
            ActiveSlot::None => 0_u8,
            ActiveSlot::Loading => 1_u8,
            ActiveSlot::Error => 2_u8,
        }
        .hash(&mut hasher);
        hash_image_snapshot(self.frozen_snapshot.as_ref(), &mut hasher);
        hasher.finish()
    }

    fn retained_paint_signature_is_complete(&self) -> bool {
        true
    }

    fn retained_transform_surface_bounds(
        &self,
        arena: &crate::view::node_arena::NodeArena,
        paint_offset: [f32; 2],
    ) -> Option<super::RetainedSurfaceBounds> {
        self.element
            .retained_transform_surface_bounds(arena, paint_offset)
    }

    fn retained_transform_output_bounds(
        &self,
        arena: &crate::view::node_arena::NodeArena,
        paint_offset: [f32; 2],
    ) -> Option<super::RetainedSurfaceBounds> {
        let wrapper = self
            .element
            .retained_transform_render_output_bounds(arena, paint_offset)?;
        let media = paint_adjusted_media_bounds(&self.element, paint_offset);
        Element::checked_union_transform_surface_bounds(wrapper, media)
    }

    fn exact_nested_isolation_render_output_bounds(
        &self,
        owner: crate::view::node_arena::NodeKey,
        arena: &crate::view::node_arena::NodeArena,
        parent_snapped_paint_offset: [f32; 2],
    ) -> Option<super::RetainedSurfaceBounds> {
        super::exact_native_nested_isolation_render_output_bounds(
            self,
            owner,
            arena,
            parent_snapped_paint_offset,
        )
    }

    fn legacy_transform_output_bounds(
        &self,
        arena: &crate::view::node_arena::NodeArena,
        paint_offset: [f32; 2],
    ) -> Option<super::RetainedSurfaceBounds> {
        let wrapper = self
            .element
            .legacy_transform_render_output_bounds(arena, paint_offset)?;
        let media = paint_adjusted_media_bounds(&self.element, paint_offset);
        Element::checked_union_transform_surface_bounds(wrapper, media)
    }

    fn retained_transform_raster_seed_bounds(&self) -> Option<super::RetainedSurfaceBounds> {
        self.element.retained_transform_raster_seed_bounds()
    }

    fn has_retained_transform_surface(&self) -> bool {
        self.element.has_retained_transform_surface()
    }

    fn compositor_viewport_transform_snapshot(&self) -> Option<super::ViewportTransformSnapshot> {
        self.element.compositor_viewport_transform_snapshot()
    }

    fn local_dirty_flags(&self) -> super::DirtyFlags {
        self.element.local_dirty_flags()
    }

    fn clear_local_dirty_flags(&mut self, flags: super::DirtyFlags) {
        self.element.clear_local_dirty_flags(flags);
    }

    fn ingest_props(&mut self, node: &crate::ui::RsxElementNode) -> Result<(), String> {
        use crate::ui::FromPropValue;
        for (key, value) in node.props.iter() {
            match *key {
                // Cold-path-owned: identity, layered style, the
                // required `source` constructor arg, and slot subtrees
                // (which need cold-path child path / global path).
                "key" | "style" | "source" | "loading" | "error" => {}
                "fit" => self.set_fit(ImageFit::from_prop_value(value.clone())?),
                "sampling" => self.set_sampling(ImageSampling::from_prop_value(value.clone())?),
                _ => return Err(format!("unknown prop `{}` on <Image>", key)),
            }
        }
        Ok(())
    }

    fn attach_side_slot(&mut self, name: &'static str, keys: Vec<NodeKey>) {
        match name {
            "loading" => self.attach_loading_slot_cold(keys),
            "error" => self.attach_error_slot_cold(keys),
            _ => {}
        }
    }

    fn build_children(
        &self,
        node: &crate::ui::RsxElementNode,
        _path: &[u64],
        _global_path: Option<&crate::view::renderer_adapter::GlobalNodePath>,
        _inherited: &crate::view::renderer_adapter::StyleCascadeContext,
    ) -> Result<Vec<crate::view::renderer_adapter::ElementDescriptor>, String> {
        if !node.children.is_empty() {
            return Err("<Image> does not accept children; use loading/error props".to_string());
        }
        Ok(Vec::new())
    }

    fn apply_prop(
        &mut self,
        arena: &mut NodeArena,
        self_key: NodeKey,
        _ctx: &crate::view::fiber_work::ApplyContext<'_>,
        name: &'static str,
        value: crate::ui::PropValue,
    ) -> crate::view::fiber_work::PropApplyOutcome {
        use crate::ui::FromPropValue;
        use crate::view::fiber_work::PropApplyOutcome;
        use crate::view::renderer_adapter::{
            StyleCascadeContext, as_element_style, commit_descriptor_tree, convert_image_slot_desc,
        };

        match name {
            // 軌 1 #4: source hot-swap. Dropping the old `ImageHandle`
            // via RAII releases the old resource entry.
            "source" => {
                let Ok(source) = ImageSource::from_prop_value(value) else {
                    return PropApplyOutcome::DecodeFailed(name);
                };
                self.set_source(source);
                PropApplyOutcome::Applied
            }
            "style" => {
                // Image uses ElementStylePropSchema; forward the
                // decoded Style to the inner Element.
                let Ok(style) = as_element_style(&value, name) else {
                    return PropApplyOutcome::DecodeFailed(name);
                };
                self.apply_style(style);
                PropApplyOutcome::Applied
            }
            "loading" | "error" => {
                let inherited = StyleCascadeContext::default();
                let Ok(descriptors) = convert_image_slot_desc(&value, &[], None, &inherited, name)
                else {
                    return PropApplyOutcome::DecodeFailed(name);
                };
                let mut new_keys: Vec<NodeKey> = Vec::with_capacity(descriptors.len());
                for desc in descriptors {
                    let new_key = commit_descriptor_tree(arena, Some(self_key), desc);
                    new_keys.push(new_key);
                }
                let replacement = match name {
                    "loading" => self.replace_loading_slot_incremental(arena, self_key, &new_keys),
                    "error" => self.replace_error_slot_incremental(arena, self_key, &new_keys),
                    _ => unreachable!(),
                };
                if let Err(error) = replacement {
                    for key in new_keys {
                        arena.remove_subtree(key);
                    }
                    eprintln!("[Image] rejected invalid {name} slot replacement: {error:?}");
                    return PropApplyOutcome::RequiresColdRebuild(name);
                }
                PropApplyOutcome::Applied
            }
            "fit" => {
                let Ok(fit) = ImageFit::from_prop_value(value) else {
                    return PropApplyOutcome::DecodeFailed(name);
                };
                self.set_fit(fit);
                PropApplyOutcome::Applied
            }
            "sampling" => {
                let Ok(sampling) = ImageSampling::from_prop_value(value) else {
                    return PropApplyOutcome::DecodeFailed(name);
                };
                self.set_sampling(sampling);
                PropApplyOutcome::Applied
            }
            _ => PropApplyOutcome::UnknownProp,
        }
    }

    fn reset_prop(
        &mut self,
        _arena: &mut NodeArena,
        _self_key: NodeKey,
        _ctx: &crate::view::fiber_work::ApplyContext<'_>,
        name: &'static str,
    ) -> crate::view::fiber_work::PropApplyOutcome {
        use crate::view::fiber_work::PropApplyOutcome;
        match name {
            "fit" => {
                self.set_fit(ImageFit::Contain);
                PropApplyOutcome::Applied
            }
            "sampling" => {
                self.set_sampling(ImageSampling::Linear);
                PropApplyOutcome::Applied
            }
            _ => PropApplyOutcome::CannotReset(name),
        }
    }
}

impl EventTarget for Image {
    crate::view::base_component::forward_event_target!(full element);
}

impl Layoutable for Image {
    fn sync_arena(&mut self, arena: &mut NodeArena) {
        self.refresh_frozen_resource(arena);
        self.prepared_by_arena_sync = true;
    }

    fn requires_arena_sync(&self) -> bool {
        true
    }

    fn measure(
        &mut self,
        constraints: LayoutConstraints,
        arena: &mut crate::view::node_arena::NodeArena,
    ) {
        if !self.prepared_by_arena_sync {
            self.refresh_frozen_resource(arena);
        }
        let snapshot = self.frozen_snapshot();
        self.element.measure(constraints, arena);
        self.apply_intrinsic_measurement(constraints, Self::intrinsic_size(&snapshot));
    }

    fn place(
        &mut self,
        placement: LayoutPlacement,
        arena: &mut crate::view::node_arena::NodeArena,
    ) {
        self.element.place(placement, arena);
    }

    fn measured_size(&self) -> (f32, f32) {
        self.element.measured_size()
    }

    fn set_layout_width(&mut self, width: f32) {
        self.element.set_layout_width(width);
    }

    fn set_layout_height(&mut self, height: f32) {
        self.element.set_layout_height(height);
    }

    fn flex_props(&self) -> crate::view::base_component::FlexProps {
        let (measured_w, measured_h) = self.measured_size();
        crate::view::base_component::FlexProps {
            intrinsic_width: Some(measured_w),
            intrinsic_height: Some(measured_h),
            intrinsic_feeds_auto_min: false,
            intrinsic_feeds_auto_base: true,
            ..self.element.flex_props()
        }
    }

    fn set_layout_offset(&mut self, x: f32, y: f32) {
        self.element.set_layout_offset(x, y);
    }
}

#[cfg(test)]
pub(crate) fn paint_adjusted_texture_bounds(
    element: &Element,
    parent_paint_offset: [f32; 2],
    mut bounds: [f32; 4],
) -> [f32; 4] {
    let paint_offset = paint_adjusted_offset(element, parent_paint_offset);
    bounds[0] += paint_offset[0];
    bounds[1] += paint_offset[1];
    bounds
}

pub(crate) fn paint_adjusted_offset(element: &Element, parent_paint_offset: [f32; 2]) -> [f32; 2] {
    let paint_x = element.layout_state.layout_position.x + parent_paint_offset[0];
    let paint_y = element.layout_state.layout_position.y + parent_paint_offset[1];
    [
        parent_paint_offset[0] + round_layout_value(paint_x) - paint_x,
        parent_paint_offset[1] + round_layout_value(paint_y) - paint_y,
    ]
}

pub(crate) fn paint_adjusted_media_bounds(
    element: &Element,
    parent_paint_offset: [f32; 2],
) -> super::RetainedSurfaceBounds {
    let snapshot = element.box_model_snapshot();
    let paint_offset = paint_adjusted_offset(element, parent_paint_offset);
    super::RetainedSurfaceBounds {
        x: snapshot.x + paint_offset[0],
        y: snapshot.y + paint_offset[1],
        width: snapshot.width,
        height: snapshot.height,
        corner_radii: [0.0; 4],
    }
}

impl Renderable for Image {
    fn build(
        &mut self,
        graph: &mut FrameGraph,
        arena: &mut crate::view::node_arena::NodeArena,
        ctx: UiBuildContext,
    ) -> super::BuildState {
        let parent_paint_offset = ctx.paint_offset();
        let viewport = ctx.viewport();
        let base_state = self.element.build_base_only(graph, arena, ctx);
        let mut ctx = UiBuildContext::from_parts(viewport, base_state);
        let opacity = self.element.retained_paint_properties().opacity;
        let Some(prepared) = self.prepared_image_op_with_upload(
            match self.frozen_upload() {
                Some(upload) => upload,
                None => return ctx.into_state(),
            },
            paint_adjusted_offset(&self.element, parent_paint_offset),
            opacity,
        ) else {
            return ctx.into_state();
        };
        let Some(parent_target) = ctx.current_target() else {
            return ctx.into_state();
        };
        graph.add_graphics_pass(TextureCompositePass::new(
            prepared.params,
            TextureCompositeInput::from_sampled_texture(
                prepared.upload,
                Default::default(),
                ctx.graphics_pass_context(),
            ),
            TextureCompositeOutput {
                render_target: parent_target,
            },
        ));
        ctx.set_current_target(parent_target);
        ctx.into_state()
    }
}

pub(crate) fn compute_image_mapping(
    fit: ImageFit,
    source_w: f32,
    source_h: f32,
    dest_w: f32,
    dest_h: f32,
) -> ([f32; 4], [f32; 4]) {
    if source_w <= 0.0 || source_h <= 0.0 || dest_w <= 0.0 || dest_h <= 0.0 {
        return ([0.0; 4], [0.0; 4]);
    }
    match fit {
        ImageFit::Fill => ([0.0, 0.0, dest_w, dest_h], [0.0, 0.0, source_w, source_h]),
        ImageFit::Contain => {
            let scale = (dest_w / source_w).min(dest_h / source_h);
            // Preserve the source aspect ratio even for legal sub-pixel
            // destinations. Independently clamping each axis to one logical
            // pixel changes the ratio and can make the image escape its box.
            let draw_w = source_w * scale;
            let draw_h = source_h * scale;
            let offset_x = (dest_w - draw_w) * 0.5;
            let offset_y = (dest_h - draw_h) * 0.5;
            (
                [offset_x, offset_y, draw_w, draw_h],
                [0.0, 0.0, source_w, source_h],
            )
        }
        ImageFit::Cover => {
            let source_ratio = source_w / source_h;
            let dest_ratio = dest_w / dest_h;
            if source_ratio > dest_ratio {
                let crop_w = source_h * dest_ratio;
                let offset_x = (source_w - crop_w) * 0.5;
                (
                    [0.0, 0.0, dest_w, dest_h],
                    [offset_x, 0.0, crop_w, source_h],
                )
            } else {
                let crop_h = source_w / dest_ratio;
                let offset_y = (source_h - crop_h) * 0.5;
                (
                    [0.0, 0.0, dest_w, dest_h],
                    [0.0, offset_y, source_w, crop_h],
                )
            }
        }
    }
}

#[cfg(test)]
mod tests;
