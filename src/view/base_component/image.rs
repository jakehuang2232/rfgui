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

    fn box_model_snapshot(&self) -> BoxModelSnapshot {
        self.element.box_model_snapshot()
    }

    fn retained_paint_properties(&self) -> super::RetainedPaintProperties {
        self.element.retained_paint_properties()
    }

    fn tick_post_layout_animation_frame(&mut self, now: crate::time::Instant) -> super::DirtyFlags {
        self.element.tick_post_layout_animation_frame(now)
    }

    fn placement_eligibility_metadata(
        &self,
    ) -> crate::view::node_arena::PlacementEligibilityMetadata {
        self.element.placement_eligibility_metadata()
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
        match self.classify_shadow_paint(arena, None, None, deferred_phase_root, recording_context)
        {
            Ok(_) => super::ShadowPaintRecordingCapability::Recordable,
            Err(blocker) => super::ShadowPaintRecordingCapability::Legacy(blocker),
        }
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
                false,
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
                false,
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
mod tests {
    use super::{ActiveSlot, Image};
    use crate::style::{BoxShadow, ClipMode, Color, Position};
    use crate::style::{ComputedStyle, EdgeInsets, Length, ParsedValue, PropertyId, Style};
    use crate::style::{Layout, ScrollDirection};
    use crate::view::ImageSource;
    use crate::view::base_component::{
        ComputedStyleConsumer, DirtyFlags, Element, ElementTrait, EventTarget, LayoutConstraints,
        LayoutPlacement, Layoutable, ShadowPaintBlocker, ShadowPaintRecordingCapability, Size,
        UiBuildContext,
    };
    use crate::view::compositor::{PaintGenerationTracker, PropertyTrees};
    use crate::view::frame_graph::FrameGraph;
    use crate::view::image_resource::{ImageSnapshot, ReadyImage};
    use crate::view::node_arena::{Node, NodeArena, NodeKey};
    use crate::view::sampled_texture::{ImageAssetId, SampledTextureId};
    use crate::view::test_support::{
        commit_child, commit_element, measure_and_place, new_test_arena,
    };
    use glam::{Mat4, Vec3};

    fn rgba_source(width: u32, height: u32) -> ImageSource {
        ImageSource::Rgba {
            width,
            height,
            pixels: std::sync::Arc::<[u8]>::from(vec![255; (width * height * 4) as usize]),
        }
    }

    #[test]
    fn image_delegates_retained_paint_properties_to_its_element() {
        let mut image = Image::new_with_id(0x90ef, rgba_source(1, 1));
        let mut style = Style::new();
        style.set_border(crate::style::Border::uniform(
            Length::px(1.0),
            &Color::hex("#ffffff"),
        ));
        style.insert(
            PropertyId::ScrollDirection,
            ParsedValue::ScrollDirection(ScrollDirection::Vertical),
        );
        image.element.apply_style(style);
        image.element.set_opacity(0.4);
        image.element.set_border_radius(3.0);
        image
            .element
            .set_box_shadows(vec![BoxShadow::new().offset(1.0)]);

        let properties = image.retained_paint_properties();
        assert_eq!(properties, image.element.retained_paint_properties());
        assert_eq!(properties.opacity.to_bits(), 0.4_f32.to_bits());
        assert!(properties.has_rounded_clip);
        assert!(properties.has_box_shadow);
        assert!(properties.has_border);
        assert!(properties.is_scroll_container);
    }

    #[test]
    fn image_wrapper_forwards_scrollbar_post_layout_lifecycle() {
        let mut image = Image::new_with_id(0x90f0, rgba_source(1, 1));
        let mut style = Style::new();
        style.insert(
            PropertyId::ScrollDirection,
            ParsedValue::ScrollDirection(ScrollDirection::Vertical),
        );
        image.element.apply_style(style);
        image.element.layout_state.content_size = Size {
            width: 120.0,
            height: 300.0,
        };

        let now = crate::time::Instant::now();
        assert!(image.set_hovered(true));
        assert!(image.wants_animation_frame());
        assert!(
            image
                .tick_post_layout_animation_frame(now)
                .contains(DirtyFlags::PAINT)
        );
        assert!(!image.wants_animation_frame());

        assert!(image.set_hovered(false));
        assert!(image.wants_animation_frame());
        assert!(
            image
                .tick_post_layout_animation_frame(now)
                .contains(DirtyFlags::PAINT)
        );
        assert!(image.wants_animation_frame());
        assert!(
            image
                .tick_post_layout_animation_frame(now + crate::time::Duration::from_millis(1_250),)
                .contains(DirtyFlags::PAINT)
        );
        assert!(!image.wants_animation_frame());
    }

    fn insert_inactive_slot_subtree(
        arena: &mut NodeArena,
        owner: NodeKey,
        id: u64,
    ) -> (NodeKey, NodeKey) {
        let root = arena.insert(Node::with_parent(
            Box::new(Element::new_with_id(id, 0.0, 0.0, 1.0, 1.0)),
            Some(owner),
        ));
        let child = arena.insert(Node::with_parent(
            Box::new(Element::new_with_id(id + 1, 0.0, 0.0, 1.0, 1.0)),
            Some(root),
        ));
        arena.set_children(root, vec![child]);
        (root, child)
    }

    #[test]
    fn image_replaces_inactive_error_and_active_loading_slots_atomically() {
        let mut arena = new_test_arena();
        let owner = commit_element(
            &mut arena,
            Box::new(Image::new_with_id(0x9100, rgba_source(1, 1))),
        );
        let (old_loading, old_loading_child) =
            insert_inactive_slot_subtree(&mut arena, owner, 0x9110);
        let (old_error, old_error_child) = insert_inactive_slot_subtree(&mut arena, owner, 0x9120);
        let (new_loading, _) = insert_inactive_slot_subtree(&mut arena, owner, 0x9130);
        let (new_error, _) = insert_inactive_slot_subtree(&mut arena, owner, 0x9140);

        arena.with_element_taken(owner, |element, arena| {
            let image = element.as_any_mut().downcast_mut::<Image>().unwrap();
            image.attach_loading_slot_cold(vec![old_loading]);
            image.attach_error_slot_cold(vec![old_error]);
            image.sync_active_slot(arena, ActiveSlot::Loading);
            assert_eq!(image.element.children(), &[old_loading]);
            assert_eq!(arena.children_of(owner), vec![old_loading]);

            image
                .replace_error_slot_incremental(arena, owner, &[new_error])
                .unwrap();
            assert_eq!(image.active_slot, ActiveSlot::None);
            assert_eq!(image.loading_slot, vec![old_loading]);
            assert_eq!(image.error_slot, vec![new_error]);
            assert!(image.element.children().is_empty());
            assert!(arena.children_of(owner).is_empty());

            image.sync_active_slot(arena, ActiveSlot::Loading);
            image
                .replace_loading_slot_incremental(arena, owner, &[new_loading])
                .unwrap();
            assert_eq!(image.active_slot, ActiveSlot::None);
            assert_eq!(image.loading_slot, vec![new_loading]);
            assert_eq!(image.error_slot, vec![new_error]);
            assert_eq!(arena.parent_of(new_loading), Some(owner));
            assert_eq!(arena.parent_of(new_error), Some(owner));
            assert_eq!(arena.children_of(owner), image.element.children());
        });

        assert!(!arena.contains_key(old_loading));
        assert!(!arena.contains_key(old_loading_child));
        assert!(!arena.contains_key(old_error));
        assert!(!arena.contains_key(old_error_child));
        assert!(arena.contains_key(new_loading));
        assert!(arena.contains_key(new_error));
    }

    fn path_source(label: &str) -> ImageSource {
        ImageSource::Path(std::path::PathBuf::from(format!(
            "/rfgui-m9b1-no-io-{label}.png"
        )))
    }

    fn prepared_ready_image(
        id: u64,
        source: ImageSource,
        width: u32,
        height: u32,
        pixels: std::sync::Arc<[u8]>,
    ) -> (
        crate::view::node_arena::NodeArena,
        crate::view::node_arena::NodeKey,
        ImageAssetId,
        u64,
    ) {
        let mut image = Image::new_with_id(id, source);
        let asset_id = image.source_handle.asset_id();
        let generation = crate::view::image_resource::replace_ready_image_for_test(
            asset_id, width, height, pixels,
        );
        let mut style = Style::new();
        style.insert(PropertyId::Width, ParsedValue::Length(Length::px(8.0)));
        style.insert(PropertyId::Height, ParsedValue::Length(Length::px(8.0)));
        image.apply_style(style);
        let mut arena = new_test_arena();
        let root = commit_element(&mut arena, Box::new(image));
        measure_and_place(
            &mut arena,
            root,
            LayoutConstraints {
                max_width: 100.0,
                max_height: 100.0,
                viewport_width: 100.0,
                viewport_height: 100.0,
                percent_base_width: Some(100.0),
                percent_base_height: Some(100.0),
            },
            LayoutPlacement {
                parent_x: 1.25,
                parent_y: 2.75,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 100.0,
                available_height: 100.0,
                viewport_width: 100.0,
                viewport_height: 100.0,
                percent_base_width: Some(100.0),
                percent_base_height: Some(100.0),
            },
        );
        arena
            .get_mut(root)
            .unwrap()
            .element
            .clear_local_dirty_flags(DirtyFlags::ALL);
        arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
        arena.refresh_subtree_dirty_cache(root);
        (arena, root, asset_id, generation)
    }

    fn image_recording_context(
        arena: &crate::view::node_arena::NodeArena,
        root: crate::view::node_arena::NodeKey,
    ) -> crate::view::paint::PaintRecordingContext {
        arena
            .get(root)
            .unwrap()
            .element
            .shadow_paint_recording_context(Default::default())
    }

    fn record_image_metadata_and_artifact(
        arena: &crate::view::node_arena::NodeArena,
        root: crate::view::node_arena::NodeKey,
    ) -> (
        crate::view::paint::PaintChunkMetadata,
        crate::view::paint::PaintArtifact,
    ) {
        let context = image_recording_context(arena, root);
        let revision = crate::view::paint::PaintContentRevision {
            self_paint_revision: 1,
            composite_revision: 1,
            topology_revision: 1,
        };
        let node = arena.get(root).unwrap();
        assert_eq!(
            node.element
                .shadow_paint_recording_capability(arena, false, context),
            ShadowPaintRecordingCapability::Recordable
        );
        let metadata = node
            .element
            .record_shadow_paint_metadata(root, Default::default(), revision, arena, context)
            .expect("ready Image metadata");
        let artifact = node
            .element
            .record_shadow_paint_artifact(root, Default::default(), revision, arena, context)
            .expect("ready Image artifact");
        (metadata, artifact)
    }

    fn assert_missing_prepared_image_fallback(arena: &NodeArena, root: NodeKey) {
        let context = image_recording_context(arena, root);
        let revision = crate::view::paint::PaintContentRevision {
            self_paint_revision: 1,
            composite_revision: 1,
            topology_revision: 1,
        };
        let node = arena.get(root).unwrap();
        assert_eq!(
            node.element
                .shadow_paint_recording_capability(arena, false, context),
            ShadowPaintRecordingCapability::Legacy(ShadowPaintBlocker::MissingPreparedImage)
        );
        assert!(
            node.element
                .record_shadow_paint_metadata(root, Default::default(), revision, arena, context)
                .is_none()
        );
        assert!(
            node.element
                .record_shadow_paint_artifact(root, Default::default(), revision, arena, context)
                .is_none()
        );
    }

    fn prepared_ready_image_with_inactive_slots(
        id: u64,
    ) -> (NodeArena, NodeKey, NodeKey, NodeKey, NodeKey, NodeKey) {
        let (mut arena, root, _, _) = prepared_ready_image(
            id,
            path_source(&format!("ready-inactive-{id}")),
            2,
            2,
            std::sync::Arc::from([0x5a_u8; 16]),
        );
        let (loading_root, loading_child) = insert_inactive_slot_subtree(&mut arena, root, id + 1);
        let (error_root, error_child) = insert_inactive_slot_subtree(&mut arena, root, id + 3);
        arena.with_element_taken(root, |element, _arena| {
            let image = element.as_any_mut().downcast_mut::<Image>().unwrap();
            image.attach_loading_slot_cold(vec![loading_root]);
            image.attach_error_slot_cold(vec![error_root]);
        });
        (
            arena,
            root,
            loading_root,
            loading_child,
            error_root,
            error_child,
        )
    }

    #[test]
    fn retained_paint_signature_covers_source_fit_sampling_and_resource_generation() {
        use std::hash::Hasher;

        let mut image = Image::new_with_id(1, rgba_source(8, 4));
        assert!(image.retained_paint_signature_is_complete());
        let initial = image.retained_paint_signature();

        image.set_fit(crate::view::ImageFit::Cover);
        let fit = image.retained_paint_signature();
        assert_ne!(fit, initial);

        image.set_sampling(crate::view::ImageSampling::Nearest);
        let sampling = image.retained_paint_signature();
        assert_ne!(sampling, fit);

        image.set_source(rgba_source(9, 4));
        assert_ne!(image.retained_paint_signature(), sampling);

        let pixels = std::sync::Arc::<[u8]>::from(vec![255; 16]);
        let first = ImageSnapshot::Ready(ReadyImage {
            sampled_texture_id: SampledTextureId::Image(ImageAssetId::for_test(1)),
            width: 2,
            height: 2,
            pixels: pixels.clone(),
            generation: 10,
        });
        let second = ImageSnapshot::Ready(ReadyImage {
            sampled_texture_id: SampledTextureId::Image(ImageAssetId::for_test(1)),
            width: 2,
            height: 2,
            pixels,
            generation: 11,
        });
        let mut first_hasher = std::collections::hash_map::DefaultHasher::new();
        super::hash_image_snapshot(Some(&first), &mut first_hasher);
        let mut second_hasher = std::collections::hash_map::DefaultHasher::new();
        super::hash_image_snapshot(Some(&second), &mut second_hasher);
        assert_ne!(first_hasher.finish(), second_hasher.finish());
    }

    #[test]
    fn image_setters_mark_only_the_required_dirty_scope_and_same_source_is_noop() {
        let pixels: std::sync::Arc<[u8]> = std::sync::Arc::from([255_u8; 4]);
        let source = ImageSource::Rgba {
            width: 1,
            height: 1,
            pixels: pixels.clone(),
        };
        let mut image = Image::new_with_id(30, source.clone());
        image.clear_local_dirty_flags(DirtyFlags::ALL);

        image.set_fit(crate::view::ImageFit::Contain);
        image.set_sampling(crate::view::ImageSampling::Linear);
        image.set_source(source);
        assert!(image.local_dirty_flags().is_empty());

        image.set_fit(crate::view::ImageFit::Cover);
        assert_eq!(image.local_dirty_flags(), DirtyFlags::PAINT);
        image.clear_local_dirty_flags(DirtyFlags::ALL);
        image.set_sampling(crate::view::ImageSampling::Nearest);
        assert_eq!(image.local_dirty_flags(), DirtyFlags::PAINT);
        image.clear_local_dirty_flags(DirtyFlags::ALL);

        image.set_source(ImageSource::Rgba {
            width: 1,
            height: 1,
            pixels: std::sync::Arc::from([255_u8; 4]),
        });
        assert_eq!(image.local_dirty_flags(), DirtyFlags::ALL);
        assert!(image.frozen_snapshot.is_none());
    }

    #[test]
    fn arena_sync_freezes_one_resource_generation_across_repeated_measure_and_identity_reads() {
        let mut image = Image::new_with_id(31, rgba_source(1, 1));
        let asset_id = image.source_handle.asset_id();
        let initial_signature = image.retained_paint_signature();
        crate::view::image_resource::replace_ready_image_for_test(
            asset_id,
            2,
            1,
            std::sync::Arc::from([1_u8; 8]),
        );
        assert_eq!(
            image.retained_paint_signature(),
            initial_signature,
            "retained identity must not observe registry state ahead of the frame freeze"
        );

        let mut arena = new_test_arena();
        image.clear_local_dirty_flags(DirtyFlags::ALL);
        image.sync_arena(&mut arena);
        let frozen_signature = image.retained_paint_signature();
        assert_ne!(frozen_signature, initial_signature);
        assert!(image.local_dirty_flags().contains(DirtyFlags::LAYOUT));

        crate::view::image_resource::replace_ready_image_for_test(
            asset_id,
            5,
            1,
            std::sync::Arc::from([2_u8; 20]),
        );
        image.measure(
            LayoutConstraints {
                max_width: 100.0,
                max_height: 100.0,
                viewport_width: 100.0,
                viewport_height: 100.0,
                percent_base_width: None,
                percent_base_height: None,
            },
            &mut arena,
        );
        assert_eq!(image.measured_size(), (2.0, 1.0));
        assert_eq!(image.retained_paint_signature(), frozen_signature);

        image.measure(
            LayoutConstraints {
                max_width: 100.0,
                max_height: 100.0,
                viewport_width: 100.0,
                viewport_height: 100.0,
                percent_base_width: None,
                percent_base_height: None,
            },
            &mut arena,
        );
        assert_eq!(image.measured_size(), (2.0, 1.0));
        assert_eq!(image.retained_paint_signature(), frozen_signature);

        image.sync_arena(&mut arena);
        image.measure(
            LayoutConstraints {
                max_width: 100.0,
                max_height: 100.0,
                viewport_width: 100.0,
                viewport_height: 100.0,
                percent_base_width: None,
                percent_base_height: None,
            },
            &mut arena,
        );
        assert_eq!(image.measured_size(), (5.0, 1.0));
        assert_ne!(image.retained_paint_signature(), frozen_signature);
    }

    #[test]
    fn path_ready_leaf_records_one_canonical_frozen_upload_after_handle_drop() {
        let pixels: std::sync::Arc<[u8]> = std::sync::Arc::from([17_u8; 24]);
        let (arena, root, asset_id, generation) =
            prepared_ready_image(0x9101, path_source("ready"), 3, 2, pixels.clone());
        {
            let mut node = arena.get_mut(root).unwrap();
            let image = node.element.as_any_mut().downcast_mut::<Image>().unwrap();
            image.set_fit(crate::view::ImageFit::Cover);
            image.set_sampling(crate::view::ImageSampling::Nearest);
        }
        let (metadata, artifact) = record_image_metadata_and_artifact(&arena, root);
        let crate::view::paint::PaintPayloadIdentity::Image(metadata_identity, decoration) =
            &metadata.payload_identity
        else {
            panic!("Path Ready metadata must use Image identity")
        };
        assert!(decoration.len() <= 2);
        assert_eq!(
            metadata_identity.sampled_texture_id,
            SampledTextureId::Image(asset_id)
        );
        assert_eq!(metadata_identity.generation, generation);
        assert_eq!((metadata_identity.width, metadata_identity.height), (3, 2));
        assert_eq!(metadata_identity.pixel_len, 24);
        assert_eq!(metadata_identity.pixel_ptr, pixels.as_ptr() as usize);
        assert_eq!(
            metadata_identity.sampling,
            crate::view::ImageSampling::Nearest
        );
        assert_eq!(
            metadata_identity.uv_bounds_bits,
            Some([0.5, 0.0, 2.0, 2.0].map(f32::to_bits)),
            "Cover fit mapping must be frozen into metadata identity"
        );

        let prepared = artifact
            .ops
            .iter()
            .find_map(|op| match op {
                crate::view::paint::PaintOp::PreparedImage(op) => Some(op),
                _ => None,
            })
            .expect("Path Ready full artifact upload");
        assert_eq!(
            crate::view::paint::PreparedImageIdentity::from_op(prepared),
            metadata_identity.clone()
        );
        assert!(std::sync::Arc::ptr_eq(&prepared.upload.pixels, &pixels));

        drop(arena);
        crate::view::image_resource::remove_image_entry_for_test(asset_id);
        assert!(prepared.upload.validate_rgba8().is_some());
        assert_eq!(prepared.upload.pixels.as_ref(), &[17_u8; 24]);
    }

    #[test]
    fn ready_image_with_two_inactive_slot_subtrees_records_only_image_content() {
        use crate::view::paint::{
            CoverageRecordingMode, PaintCoverageItem, PaintNodePhase, PaintPropertyScope,
            record_coverage_manifest,
        };

        let (arena, root, loading_root, loading_child, error_root, error_child) =
            prepared_ready_image_with_inactive_slots(0x9180);
        let image_node = arena.get(root).unwrap();
        assert!(image_node.children().is_empty());
        assert!(image_node.element.children().is_empty());
        drop(image_node);
        assert_eq!(arena.parent_of(loading_root), Some(root));
        assert_eq!(arena.parent_of(error_root), Some(root));

        let roots = [root];
        let mut properties = PropertyTrees::default();
        properties.sync(&arena, &roots);
        let mut generations = PaintGenerationTracker::default();
        generations.sync(&arena, &roots, &properties);
        let metadata = record_coverage_manifest(
            &arena,
            &roots,
            false,
            true,
            CoverageRecordingMode::MetadataOnly,
            &properties,
            &generations,
        );
        let full = record_coverage_manifest(
            &arena,
            &roots,
            false,
            true,
            CoverageRecordingMode::FullArtifact,
            &properties,
            &generations,
        );
        assert!(metadata.validation_errors.is_empty());
        assert!(full.validation_errors.is_empty());
        let [
            PaintCoverageItem::ArtifactChunk {
                chunk: metadata_chunk,
                ops: None,
                ..
            },
        ] = metadata.items.as_slice()
        else {
            panic!("Ready + inactive slots metadata must contain only ImageContent")
        };
        let [
            PaintCoverageItem::ArtifactChunk {
                chunk: full_chunk,
                ops: Some(full_ops),
                ..
            },
        ] = full.items.as_slice()
        else {
            panic!("Ready + inactive slots full recording must contain only ImageContent")
        };
        assert_eq!(metadata_chunk.id, full_chunk.id);
        assert_eq!(metadata_chunk.payload_identity, full_chunk.payload_identity);
        assert_eq!(metadata_chunk.owner, root);
        assert_eq!(metadata_chunk.id.scope, PaintPropertyScope::SelfPaint);
        assert_eq!(metadata_chunk.id.phase, PaintNodePhase::BeforeChildren);
        assert_eq!(metadata_chunk.id.slot, 0);
        assert_eq!(
            metadata_chunk.id.role,
            crate::view::paint::PaintChunkRole::ImageContent
        );
        assert!(
            full_ops
                .iter()
                .any(|op| matches!(op, crate::view::paint::PaintOp::PreparedImage(_)))
        );
        for inactive in [loading_root, loading_child, error_root, error_child] {
            assert!(metadata.items.iter().all(|item| !matches!(
                item,
                PaintCoverageItem::ArtifactChunk { chunk, .. } if chunk.owner == inactive
            )));
            assert!(full.items.iter().all(|item| !matches!(
                item,
                PaintCoverageItem::ArtifactChunk { chunk, .. } if chunk.owner == inactive
            )));
        }
    }

    #[test]
    fn ready_image_rejects_invalid_inactive_roots_and_children_mirror_drift() {
        enum InvalidInactiveRoot {
            Missing,
            Duplicate,
            WrongParent,
            ChildrenMirror,
        }

        for (index, invalid) in [
            InvalidInactiveRoot::Missing,
            InvalidInactiveRoot::Duplicate,
            InvalidInactiveRoot::WrongParent,
            InvalidInactiveRoot::ChildrenMirror,
        ]
        .into_iter()
        .enumerate()
        {
            let id = 0x91a0 + index as u64 * 0x10;
            let (mut arena, root, _, _) = prepared_ready_image(
                id,
                path_source(&format!("ready-invalid-inactive-{id}")),
                2,
                2,
                std::sync::Arc::from([0x7c_u8; 16]),
            );
            match invalid {
                InvalidInactiveRoot::Missing => {
                    let stale = arena.insert(Node::with_parent(
                        Box::new(Element::new_with_id(id + 1, 0.0, 0.0, 1.0, 1.0)),
                        Some(root),
                    ));
                    arena.with_element_taken(root, |element, _arena| {
                        element
                            .as_any_mut()
                            .downcast_mut::<Image>()
                            .unwrap()
                            .attach_loading_slot_cold(vec![stale]);
                    });
                    arena.remove_subtree(stale);
                }
                InvalidInactiveRoot::Duplicate => {
                    let duplicate = arena.insert(Node::with_parent(
                        Box::new(Element::new_with_id(id + 1, 0.0, 0.0, 1.0, 1.0)),
                        Some(root),
                    ));
                    arena.with_element_taken(root, |element, _arena| {
                        let image = element.as_any_mut().downcast_mut::<Image>().unwrap();
                        image.attach_loading_slot_cold(vec![duplicate]);
                        image.attach_error_slot_cold(vec![duplicate]);
                    });
                }
                InvalidInactiveRoot::WrongParent => {
                    let wrong_parent = arena.insert(Node::new(Box::new(Element::new_with_id(
                        id + 1,
                        0.0,
                        0.0,
                        1.0,
                        1.0,
                    ))));
                    arena.with_element_taken(root, |element, _arena| {
                        element
                            .as_any_mut()
                            .downcast_mut::<Image>()
                            .unwrap()
                            .attach_error_slot_cold(vec![wrong_parent]);
                    });
                }
                InvalidInactiveRoot::ChildrenMirror => {
                    let mirrored_only = arena.insert(Node::with_parent(
                        Box::new(Element::new_with_id(id + 1, 0.0, 0.0, 1.0, 1.0)),
                        Some(root),
                    ));
                    arena.set_children(root, vec![mirrored_only]);
                    arena.with_element_taken(root, |element, _arena| {
                        let image = element.as_any_mut().downcast_mut::<Image>().unwrap();
                        let mut style = Style::new();
                        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
                        image.apply_style(style);
                        image.element.sync_children_mirror(&[]);
                    });
                    assert_eq!(arena.children_of(root), vec![mirrored_only]);
                }
            }
            assert_missing_prepared_image_fallback(&arena, root);
        }
    }

    #[test]
    fn ready_image_inactive_slots_do_not_bypass_current_handle_resource_drift() {
        let (arena, root, loading_root, _, error_root, _) =
            prepared_ready_image_with_inactive_slots(0x91d0);
        {
            let mut node = arena.get_mut(root).unwrap();
            let image = node.element.as_any_mut().downcast_mut::<Image>().unwrap();
            let stale = image.frozen_snapshot.clone().unwrap();
            image.set_source(path_source("ready-inactive-source-drift"));
            image.frozen_snapshot = Some(stale);
            image.prepared_by_arena_sync = true;
        }
        assert_eq!(arena.parent_of(loading_root), Some(root));
        assert_eq!(arena.parent_of(error_root), Some(root));
        assert_missing_prepared_image_fallback(&arena, root);
    }

    #[test]
    fn path_source_swap_rejects_stale_frozen_snapshot_by_current_handle_identity() {
        let (arena, root, _old_asset_id, _) = prepared_ready_image(
            0x9102,
            path_source("swap-a"),
            2,
            2,
            std::sync::Arc::from([3_u8; 16]),
        );
        {
            let mut node = arena.get_mut(root).unwrap();
            let image = node.element.as_any_mut().downcast_mut::<Image>().unwrap();
            let stale = image.frozen_snapshot.clone().unwrap();
            image.set_source(path_source("swap-b"));
            image.frozen_snapshot = Some(stale);
            image.prepared_by_arena_sync = true;
        }
        let context = image_recording_context(&arena, root);
        let revision = crate::view::paint::PaintContentRevision {
            self_paint_revision: 1,
            composite_revision: 1,
            topology_revision: 1,
        };
        let node = arena.get(root).unwrap();
        assert_eq!(
            node.element
                .shadow_paint_recording_capability(&arena, false, context),
            ShadowPaintRecordingCapability::Legacy(ShadowPaintBlocker::MissingPreparedImage)
        );
        assert!(
            node.element
                .record_shadow_paint_metadata(root, Default::default(), revision, &arena, context,)
                .is_none()
        );
        assert!(
            node.element
                .record_shadow_paint_artifact(root, Default::default(), revision, &arena, context,)
                .is_none()
        );
    }

    #[test]
    fn path_generation_drift_keeps_one_frame_freeze_then_advances_on_sync() {
        let old_pixels: std::sync::Arc<[u8]> = std::sync::Arc::from([5_u8; 16]);
        let (mut arena, root, asset_id, old_generation) =
            prepared_ready_image(0x9103, path_source("generation"), 2, 2, old_pixels.clone());
        let new_pixels: std::sync::Arc<[u8]> = std::sync::Arc::from([9_u8; 24]);
        let new_generation = crate::view::image_resource::replace_ready_image_for_test(
            asset_id,
            3,
            2,
            new_pixels.clone(),
        );
        let (metadata, artifact) = record_image_metadata_and_artifact(&arena, root);
        let crate::view::paint::PaintPayloadIdentity::Image(identity, _) =
            &metadata.payload_identity
        else {
            unreachable!()
        };
        assert_eq!(identity.generation, old_generation);
        assert_eq!(identity.pixel_ptr, old_pixels.as_ptr() as usize);
        let prepared = artifact
            .ops
            .iter()
            .find_map(|op| match op {
                crate::view::paint::PaintOp::PreparedImage(op) => Some(op),
                _ => None,
            })
            .unwrap();
        assert_eq!(prepared.upload.generation, old_generation);

        arena.with_element_taken(root, |element, arena| {
            element
                .as_any_mut()
                .downcast_mut::<Image>()
                .unwrap()
                .sync_arena(arena);
        });
        let (metadata, artifact) = record_image_metadata_and_artifact(&arena, root);
        let crate::view::paint::PaintPayloadIdentity::Image(identity, _) =
            &metadata.payload_identity
        else {
            unreachable!()
        };
        assert_eq!(identity.generation, new_generation);
        assert_eq!((identity.width, identity.height), (3, 2));
        assert_eq!(identity.pixel_ptr, new_pixels.as_ptr() as usize);
        let prepared = artifact
            .ops
            .iter()
            .find_map(|op| match op {
                crate::view::paint::PaintOp::PreparedImage(op) => Some(op),
                _ => None,
            })
            .unwrap();
        assert_eq!(prepared.upload.generation, new_generation);
    }

    #[test]
    fn path_loading_and_error_wrappers_record_canonical_decoration_while_invalid_ready_fails() {
        for (index, state) in ["loading", "error"].into_iter().enumerate() {
            let image = Image::new_with_id(0x9110 + index as u64, path_source(state));
            let asset_id = image.source_handle.asset_id();
            match state {
                "loading" => crate::view::image_resource::set_image_loading_for_test(asset_id),
                "error" => crate::view::image_resource::set_image_error_for_test(
                    asset_id,
                    "synthetic decode error",
                ),
                _ => unreachable!(),
            }
            let mut arena = new_test_arena();
            let root = commit_element(&mut arena, Box::new(image));
            measure_and_place(
                &mut arena,
                root,
                LayoutConstraints {
                    max_width: 100.0,
                    max_height: 100.0,
                    viewport_width: 100.0,
                    viewport_height: 100.0,
                    percent_base_width: None,
                    percent_base_height: None,
                },
                LayoutPlacement {
                    parent_x: 0.0,
                    parent_y: 0.0,
                    visual_offset_x: 0.0,
                    visual_offset_y: 0.0,
                    available_width: 100.0,
                    available_height: 100.0,
                    viewport_width: 100.0,
                    viewport_height: 100.0,
                    percent_base_width: None,
                    percent_base_height: None,
                },
            );
            let (metadata, artifact) = record_image_metadata_and_artifact(&arena, root);
            assert_eq!(
                metadata.id.scope,
                crate::view::paint::PaintPropertyScope::SelfPaint,
                "{state}"
            );
            assert_eq!(
                metadata.id.phase,
                crate::view::paint::PaintNodePhase::BeforeChildren,
                "{state}"
            );
            assert_eq!(metadata.id.slot, 0, "{state}");
            assert_eq!(
                metadata.id.role,
                crate::view::paint::PaintChunkRole::SelfDecoration,
                "{state}"
            );
            assert_eq!(artifact.chunks.len(), 1, "{state}");
            let chunk = &artifact.chunks[0];
            assert_eq!(chunk.id, metadata.id, "{state}");
            assert_eq!(chunk.owner, metadata.owner, "{state}");
            assert_eq!(
                chunk.bounds.x.to_bits(),
                metadata.bounds.x.to_bits(),
                "{state}"
            );
            assert_eq!(
                chunk.bounds.y.to_bits(),
                metadata.bounds.y.to_bits(),
                "{state}"
            );
            assert_eq!(
                chunk.bounds.width.to_bits(),
                metadata.bounds.width.to_bits(),
                "{state}"
            );
            assert_eq!(
                chunk.bounds.height.to_bits(),
                metadata.bounds.height.to_bits(),
                "{state}"
            );
            assert_eq!(chunk.properties, metadata.properties, "{state}");
            assert_eq!(chunk.content_revision, metadata.content_revision, "{state}");
            assert_eq!(chunk.payload_identity, metadata.payload_identity, "{state}");
            assert!(
                artifact
                    .ops
                    .iter()
                    .all(|op| !matches!(op, crate::view::paint::PaintOp::PreparedImage(_))),
                "{state} wrapper must not prepare image content"
            );
        }

        let invalid = Image::new_with_id(0x9112, path_source("invalid"));
        let invalid_asset_id = invalid.source_handle.asset_id();
        crate::view::image_resource::replace_ready_image_for_test(
            invalid_asset_id,
            2,
            2,
            std::sync::Arc::from([0_u8; 3]),
        );
        let mut invalid_arena = new_test_arena();
        let invalid_root = commit_element(&mut invalid_arena, Box::new(invalid));
        measure_and_place(
            &mut invalid_arena,
            invalid_root,
            LayoutConstraints {
                max_width: 100.0,
                max_height: 100.0,
                viewport_width: 100.0,
                viewport_height: 100.0,
                percent_base_width: None,
                percent_base_height: None,
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 100.0,
                available_height: 100.0,
                viewport_width: 100.0,
                viewport_height: 100.0,
                percent_base_width: None,
                percent_base_height: None,
            },
        );
        let invalid_context = image_recording_context(&invalid_arena, invalid_root);
        assert_eq!(
            invalid_arena
                .get(invalid_root)
                .unwrap()
                .element
                .shadow_paint_recording_capability(&invalid_arena, false, invalid_context),
            ShadowPaintRecordingCapability::Legacy(ShadowPaintBlocker::MissingPreparedImage)
        );

        let rgba_pixels: std::sync::Arc<[u8]> = std::sync::Arc::from([21_u8; 16]);
        let (arena, root, asset_id, generation) =
            prepared_ready_image(0x9120, rgba_source(2, 2), 2, 2, rgba_pixels.clone());
        let (metadata, artifact) = record_image_metadata_and_artifact(&arena, root);
        let crate::view::paint::PaintPayloadIdentity::Image(identity, _) =
            &metadata.payload_identity
        else {
            unreachable!()
        };
        assert_eq!(
            (identity.sampled_texture_id, identity.generation),
            (SampledTextureId::Image(asset_id), generation)
        );
        assert_eq!(identity.pixel_ptr, rgba_pixels.as_ptr() as usize);
        assert!(
            artifact
                .ops
                .iter()
                .any(|op| matches!(op, crate::view::paint::PaintOp::PreparedImage(_)))
        );
    }

    #[test]
    fn error_wrapper_rejects_wrong_parent_in_inactive_loading_slot() {
        let image = Image::new_with_id(0x9130, path_source("error-inactive-parent"));
        let asset_id = image.source_handle.asset_id();
        crate::view::image_resource::set_image_error_for_test(asset_id, "synthetic decode error");
        let mut arena = new_test_arena();
        let root = commit_element(&mut arena, Box::new(image));
        let inactive = arena.insert(Node::new(Box::new(Element::new_with_id(
            0x9131, 0.0, 0.0, 1.0, 1.0,
        ))));
        arena.with_element_taken(root, |element, _arena| {
            element
                .as_any_mut()
                .downcast_mut::<Image>()
                .unwrap()
                .attach_loading_slot_cold(vec![inactive]);
        });
        measure_and_place(
            &mut arena,
            root,
            LayoutConstraints {
                max_width: 100.0,
                max_height: 100.0,
                viewport_width: 100.0,
                viewport_height: 100.0,
                percent_base_width: None,
                percent_base_height: None,
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 100.0,
                available_height: 100.0,
                viewport_width: 100.0,
                viewport_height: 100.0,
                percent_base_width: None,
                percent_base_height: None,
            },
        );
        let context = image_recording_context(&arena, root);
        let node = arena.get(root).unwrap();
        assert_eq!(
            node.element
                .shadow_paint_recording_capability(&arena, false, context),
            ShadowPaintRecordingCapability::Legacy(ShadowPaintBlocker::MissingPreparedImage)
        );
        let revision = crate::view::paint::PaintContentRevision {
            self_paint_revision: 1,
            composite_revision: 1,
            topology_revision: 1,
        };
        assert!(
            node.element
                .record_shadow_paint_metadata(root, Default::default(), revision, &arena, context)
                .is_none()
        );
        assert!(
            node.element
                .record_shadow_paint_artifact(root, Default::default(), revision, &arena, context)
                .is_none()
        );
    }

    #[test]
    fn loading_wrapper_rejects_inactive_root_aliasing_an_active_grandchild_and_topology_drift() {
        #[derive(Clone, Copy)]
        enum Drift {
            AliasOnly,
            Parent,
            ChildrenMirror,
        }

        fn fixture(id: u64, drift: Drift) -> (NodeArena, NodeKey, NodeKey, NodeKey) {
            let mut image = Image::new_with_id(id, path_source(&format!("alias-{id}")));
            let mut style = Style::new();
            style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
            image.apply_style(style);
            crate::view::image_resource::set_image_loading_for_test(image.source_handle.asset_id());

            let mut arena = new_test_arena();
            let owner = commit_element(&mut arena, Box::new(image));
            let (active_root, active_grandchild) =
                insert_inactive_slot_subtree(&mut arena, owner, id + 1);
            arena.with_element_taken(owner, |element, _arena| {
                let image = element.as_any_mut().downcast_mut::<Image>().unwrap();
                image.attach_loading_slot_cold(vec![active_root]);
                image.attach_error_slot_cold(vec![active_grandchild]);
            });
            measure_and_place(
                &mut arena,
                owner,
                LayoutConstraints {
                    max_width: 100.0,
                    max_height: 100.0,
                    viewport_width: 100.0,
                    viewport_height: 100.0,
                    percent_base_width: None,
                    percent_base_height: None,
                },
                LayoutPlacement {
                    parent_x: 0.0,
                    parent_y: 0.0,
                    visual_offset_x: 0.0,
                    visual_offset_y: 0.0,
                    available_width: 100.0,
                    available_height: 100.0,
                    viewport_width: 100.0,
                    viewport_height: 100.0,
                    percent_base_width: None,
                    percent_base_height: None,
                },
            );

            if matches!(drift, Drift::Parent) {
                // Reproduce the reviewer case: the inactive root now appears
                // directly owned by Image while the active root still reaches
                // it through its frozen child edge.
                arena.set_parent(active_grandchild, Some(owner));
            }
            if matches!(drift, Drift::ChildrenMirror) {
                // Node topology retains the edge while the active Element
                // mirror is stale. DFS must reject this independently of the
                // inactive-root alias check.
                arena.with_element_taken(active_root, |element, _arena| {
                    element.sync_children_mirror(&[]);
                });
            }
            (arena, owner, active_root, active_grandchild)
        }

        for (index, drift) in [Drift::AliasOnly, Drift::Parent, Drift::ChildrenMirror]
            .into_iter()
            .enumerate()
        {
            let (arena, owner, _active_root, _active_grandchild) =
                fixture(0x9160 + index as u64 * 0x10, drift);
            let context = image_recording_context(&arena, owner);
            let node = arena.get(owner).unwrap();
            assert_eq!(
                node.element
                    .shadow_paint_recording_capability(&arena, false, context),
                ShadowPaintRecordingCapability::Legacy(ShadowPaintBlocker::MissingPreparedImage)
            );
            let revision = crate::view::paint::PaintContentRevision {
                self_paint_revision: 1,
                composite_revision: 1,
                topology_revision: 1,
            };
            assert!(
                node.element
                    .record_shadow_paint_metadata(
                        owner,
                        Default::default(),
                        revision,
                        &arena,
                        context,
                    )
                    .is_none()
            );
            assert!(
                node.element
                    .record_shadow_paint_artifact(
                        owner,
                        Default::default(),
                        revision,
                        &arena,
                        context,
                    )
                    .is_none()
            );
        }
    }

    #[test]
    fn loading_wrapper_coverage_traverses_only_the_active_slot_subtree_in_canonical_order() {
        use crate::view::paint::{
            CoverageRecordingMode, PaintCoverageItem, PaintNodePhase, PaintPropertyScope,
            record_coverage_manifest,
        };

        let mut image = Image::new_with_id(0x9140, path_source("loading-active-subtree"));
        let mut image_style = Style::new();
        image_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        image_style.set_box_shadow(vec![
            BoxShadow::new()
                .color(Color::rgb(220, 30, 20))
                .offset_x(1.5)
                .offset_y(-2.25),
        ]);
        image.apply_style(image_style);
        let asset_id = image.source_handle.asset_id();
        crate::view::image_resource::set_image_loading_for_test(asset_id);
        let mut arena = new_test_arena();
        let root = commit_element(&mut arena, Box::new(image));
        let (loading_root, loading_child) = insert_inactive_slot_subtree(&mut arena, root, 0x9141);
        let (inactive_error_root, inactive_error_child) =
            insert_inactive_slot_subtree(&mut arena, root, 0x9151);
        arena.with_element_taken(root, |element, _arena| {
            let image = element.as_any_mut().downcast_mut::<Image>().unwrap();
            image.attach_loading_slot_cold(vec![loading_root]);
            image.attach_error_slot_cold(vec![inactive_error_root]);
        });
        measure_and_place(
            &mut arena,
            root,
            LayoutConstraints {
                max_width: 100.0,
                max_height: 100.0,
                viewport_width: 100.0,
                viewport_height: 100.0,
                percent_base_width: None,
                percent_base_height: None,
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 100.0,
                available_height: 100.0,
                viewport_width: 100.0,
                viewport_height: 100.0,
                percent_base_width: None,
                percent_base_height: None,
            },
        );

        let node = arena.get(root).unwrap();
        assert_eq!(node.children(), &[loading_root]);
        assert_eq!(node.element.children(), &[loading_root]);
        drop(node);
        assert_eq!(arena.parent_of(loading_root), Some(root));
        assert_eq!(arena.parent_of(inactive_error_root), Some(root));

        let roots = [root];
        let mut properties = PropertyTrees::default();
        properties.sync(&arena, &roots);
        let mut generations = PaintGenerationTracker::default();
        generations.sync(&arena, &roots, &properties);
        let record = |mode: CoverageRecordingMode| {
            record_coverage_manifest(&arena, &roots, false, true, mode, &properties, &generations)
        };
        let metadata = record(CoverageRecordingMode::MetadataOnly);
        let full = record(CoverageRecordingMode::FullArtifact);
        assert!(metadata.validation_errors.is_empty());
        assert!(full.validation_errors.is_empty());

        let summarize = |manifest: &crate::view::paint::PaintCoverageManifest| {
            manifest
                .items
                .iter()
                .map(|item| match item {
                    PaintCoverageItem::ArtifactChunk { chunk, .. } => (
                        chunk.owner,
                        chunk.id.scope,
                        chunk.id.phase,
                        chunk.id.slot,
                        chunk.id.role,
                        chunk.payload_identity.clone(),
                    ),
                    other => panic!("unexpected coverage item: {other:?}"),
                })
                .collect::<Vec<_>>()
        };
        let metadata_summary = summarize(&metadata);
        let full_summary = summarize(&full);
        assert_eq!(metadata_summary, full_summary);
        assert!(matches!(
            &metadata_summary[0].5,
            crate::view::paint::PaintPayloadIdentity::PreparedShadows(shadows, _)
                if shadows.len() == 1
        ));
        let PaintCoverageItem::ArtifactChunk {
            ops: Some(root_ops),
            ..
        } = &full.items[0]
        else {
            panic!("Image wrapper root must own full ops")
        };
        assert!(matches!(
            root_ops.first(),
            Some(crate::view::paint::PaintOp::PreparedShadow(shadow))
                if shadow.params.opacity.to_bits() == 1.0_f32.to_bits()
        ));
        assert!(
            root_ops
                .iter()
                .all(|op| !matches!(op, crate::view::paint::PaintOp::PreparedImage(_)))
        );
        assert_eq!(
            metadata_summary
                .iter()
                .map(|(owner, ..)| *owner)
                .collect::<Vec<_>>(),
            vec![root, loading_root, loading_child]
        );
        assert!(
            metadata_summary
                .iter()
                .all(|(_, scope, phase, slot, _, _)| {
                    *scope == PaintPropertyScope::SelfPaint
                        && *phase == PaintNodePhase::BeforeChildren
                        && *slot == 0
                })
        );
        assert!(metadata_summary.iter().all(|(owner, ..)| {
            *owner != inactive_error_root && *owner != inactive_error_child
        }));
    }

    #[test]
    fn error_wrapper_outer_shadow_records_before_active_subtree_and_excludes_inactive_slot() {
        use crate::view::paint::{
            CoverageRecordingMode, PaintCoverageItem, record_coverage_manifest,
        };

        let mut image = Image::new_with_id(0x9170, path_source("error-shadow-subtree"));
        let mut style = Style::new();
        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        style.set_box_shadow(vec![
            BoxShadow::new()
                .color(Color::rgb(20, 40, 220))
                .offset_x(-3.0)
                .offset_y(4.5),
        ]);
        image.apply_style(style);
        crate::view::image_resource::set_image_error_for_test(
            image.source_handle.asset_id(),
            "synthetic decode error",
        );
        let mut arena = new_test_arena();
        let owner = commit_element(&mut arena, Box::new(image));
        let (loading_root, loading_child) = insert_inactive_slot_subtree(&mut arena, owner, 0x9171);
        let (error_root, error_child) = insert_inactive_slot_subtree(&mut arena, owner, 0x9181);
        arena.with_element_taken(owner, |element, _arena| {
            let image = element.as_any_mut().downcast_mut::<Image>().unwrap();
            image.attach_loading_slot_cold(vec![loading_root]);
            image.attach_error_slot_cold(vec![error_root]);
        });
        measure_and_place(
            &mut arena,
            owner,
            LayoutConstraints {
                max_width: 100.0,
                max_height: 100.0,
                viewport_width: 100.0,
                viewport_height: 100.0,
                percent_base_width: None,
                percent_base_height: None,
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 100.0,
                available_height: 100.0,
                viewport_width: 100.0,
                viewport_height: 100.0,
                percent_base_width: None,
                percent_base_height: None,
            },
        );
        let roots = [owner];
        let mut properties = PropertyTrees::default();
        properties.sync(&arena, &roots);
        let mut generations = PaintGenerationTracker::default();
        generations.sync(&arena, &roots, &properties);
        let record = |mode: CoverageRecordingMode| {
            record_coverage_manifest(&arena, &roots, false, true, mode, &properties, &generations)
        };
        let metadata = record(CoverageRecordingMode::MetadataOnly);
        let full = record(CoverageRecordingMode::FullArtifact);
        assert!(metadata.validation_errors.is_empty());
        assert!(full.validation_errors.is_empty());
        assert_eq!(
            metadata
                .items
                .iter()
                .map(|item| match item {
                    PaintCoverageItem::ArtifactChunk { chunk, .. } => chunk.owner,
                    other => panic!("unexpected coverage item: {other:?}"),
                })
                .collect::<Vec<_>>(),
            vec![owner, error_root, error_child]
        );
        let PaintCoverageItem::ArtifactChunk {
            chunk: metadata_chunk,
            ..
        } = &metadata.items[0]
        else {
            unreachable!()
        };
        let PaintCoverageItem::ArtifactChunk {
            chunk: full_chunk,
            ops: Some(ops),
            ..
        } = &full.items[0]
        else {
            unreachable!()
        };
        assert_eq!(metadata_chunk.payload_identity, full_chunk.payload_identity);
        assert!(matches!(
            &metadata_chunk.payload_identity,
            crate::view::paint::PaintPayloadIdentity::PreparedShadows(shadows, _)
                if shadows.len() == 1
        ));
        assert!(matches!(
            ops.first(),
            Some(crate::view::paint::PaintOp::PreparedShadow(_))
        ));
        assert!(
            ops.iter()
                .all(|op| !matches!(op, crate::view::paint::PaintOp::PreparedImage(_)))
        );
        assert!(metadata.items.iter().all(|item| !matches!(
            item,
            PaintCoverageItem::ArtifactChunk { chunk, .. }
                if chunk.owner == loading_root || chunk.owner == loading_child
        )));
    }

    #[test]
    fn ready_image_media_with_outer_shadow_records_typed_shadow_prefix() {
        let (mut arena, owner, ..) = prepared_ready_image(
            0x9190,
            path_source("ready-shadow-fallback"),
            2,
            2,
            std::sync::Arc::from([0x4d_u8; 16]),
        );
        {
            let mut node = arena.get_mut(owner).unwrap();
            let image = node.element.as_any_mut().downcast_mut::<Image>().unwrap();
            let mut style = Style::new();
            style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
            style.set_box_shadow(vec![BoxShadow::new().offset_x(1.0)]);
            image.apply_style(style);
        }
        measure_and_place(
            &mut arena,
            owner,
            LayoutConstraints {
                max_width: 100.0,
                max_height: 100.0,
                viewport_width: 100.0,
                viewport_height: 100.0,
                percent_base_width: Some(100.0),
                percent_base_height: Some(100.0),
            },
            LayoutPlacement {
                parent_x: 1.25,
                parent_y: 2.75,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 100.0,
                available_height: 100.0,
                viewport_width: 100.0,
                viewport_height: 100.0,
                percent_base_width: Some(100.0),
                percent_base_height: Some(100.0),
            },
        );
        let node = arena.get(owner).unwrap();
        let context = image_recording_context(&arena, owner);
        assert_eq!(
            node.element
                .shadow_paint_recording_capability(&arena, false, context),
            ShadowPaintRecordingCapability::Recordable
        );
        let revision = crate::view::paint::PaintContentRevision {
            self_paint_revision: 1,
            composite_revision: 1,
            topology_revision: 1,
        };
        let metadata = node
            .element
            .record_shadow_paint_metadata(owner, Default::default(), revision, &arena, context)
            .expect("ready shadow Image metadata");
        let artifact = node
            .element
            .record_shadow_paint_artifact(owner, Default::default(), revision, &arena, context)
            .expect("ready shadow Image artifact");
        assert_eq!(
            artifact.chunks[0].payload_identity,
            metadata.payload_identity
        );
        assert!(
            matches!(
                &metadata.payload_identity,
                crate::view::paint::PaintPayloadIdentity::ImageWithShadows(_, shadows, _)
                    if shadows.len() == 1
            ),
            "{:?}",
            metadata.payload_identity
        );
        assert!(matches!(
            artifact.ops.as_slice(),
            [
                crate::view::paint::PaintOp::PreparedShadow(_),
                ..,
                crate::view::paint::PaintOp::PreparedImage(_)
            ]
        ));
        assert!(
            crate::view::paint::validate_media_content_artifact_for_test(&artifact),
            "compiler must accept the exact typed shadow prefix"
        );
        let mut reordered = artifact.clone();
        assert!(matches!(
            reordered.ops.get(1),
            Some(crate::view::paint::PaintOp::DrawRect(_))
        ));
        reordered.ops.swap(0, 1);
        assert!(
            !crate::view::paint::validate_media_content_artifact_for_test(&reordered),
            "a shadow after decoration must fail closed"
        );
        let (baseline_media, baseline_shadows, baseline_decoration) =
            match &metadata.payload_identity {
                crate::view::paint::PaintPayloadIdentity::ImageWithShadows(
                    media,
                    shadows,
                    decoration,
                ) => (media.clone(), shadows.clone(), decoration.clone()),
                _ => unreachable!(),
            };
        drop(node);
        {
            let mut node = arena.get_mut(owner).unwrap();
            let image = node.element.as_any_mut().downcast_mut::<Image>().unwrap();
            let mut style = Style::new();
            style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
            style.set_box_shadow(vec![BoxShadow::new().offset_x(9.0).offset_y(-4.0)]);
            image.apply_style(style);
        }
        measure_and_place(
            &mut arena,
            owner,
            LayoutConstraints {
                max_width: 100.0,
                max_height: 100.0,
                viewport_width: 100.0,
                viewport_height: 100.0,
                percent_base_width: Some(100.0),
                percent_base_height: Some(100.0),
            },
            LayoutPlacement {
                parent_x: 1.25,
                parent_y: 2.75,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 100.0,
                available_height: 100.0,
                viewport_width: 100.0,
                viewport_height: 100.0,
                percent_base_width: Some(100.0),
                percent_base_height: Some(100.0),
            },
        );
        let node = arena.get(owner).unwrap();
        let changed_context = image_recording_context(&arena, owner);
        let changed = node
            .element
            .record_shadow_paint_metadata(
                owner,
                Default::default(),
                revision,
                &arena,
                changed_context,
            )
            .expect("shadow-mutated Image metadata");
        let crate::view::paint::PaintPayloadIdentity::ImageWithShadows(
            changed_media,
            changed_shadows,
            changed_decoration,
        ) = changed.payload_identity
        else {
            panic!("shadow-mutated Image must retain typed media identity")
        };
        assert_eq!(changed_media, baseline_media);
        assert_ne!(changed_shadows, baseline_shadows);
        assert_eq!(changed_decoration, baseline_decoration);
    }

    #[test]
    fn ready_image_exact_self_clip_shadow_metadata_and_full_are_canonical() {
        use crate::view::paint::{
            CoverageRecordingMode, PaintCoverageItem, PaintPayloadIdentity,
            record_coverage_manifest,
        };

        let (mut arena, owner, ..) = prepared_ready_image(
            0x9191,
            path_source("ready-exact-self-clip-shadow"),
            2,
            2,
            std::sync::Arc::from([0x5a_u8; 16]),
        );
        {
            let mut node = arena.get_mut(owner).unwrap();
            let image = node.element.as_any_mut().downcast_mut::<Image>().unwrap();
            let mut style = Style::new();
            style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
            style.insert(
                PropertyId::Position,
                ParsedValue::Position(
                    Position::absolute()
                        .left(Length::px(1.25))
                        .top(Length::px(2.75))
                        .clip(ClipMode::AnchorParent),
                ),
            );
            style.set_box_shadow(vec![BoxShadow::new().offset_x(-3.0).offset_y(4.5)]);
            image.apply_style(style);
        }
        measure_and_place(
            &mut arena,
            owner,
            LayoutConstraints {
                max_width: 100.0,
                max_height: 100.0,
                viewport_width: 100.0,
                viewport_height: 100.0,
                percent_base_width: Some(100.0),
                percent_base_height: Some(100.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 100.0,
                available_height: 100.0,
                viewport_width: 100.0,
                viewport_height: 100.0,
                percent_base_width: Some(100.0),
                percent_base_height: Some(100.0),
            },
        );
        let roots = [owner];
        let mut properties = PropertyTrees::default();
        properties.sync(&arena, &roots);
        let mut generations = PaintGenerationTracker::default();
        generations.sync(&arena, &roots, &properties);
        let state = properties.paint_state_for(owner).unwrap();
        let node = arena.get(owner).unwrap();
        let mut direct_context = node
            .element
            .shadow_paint_recording_context(Default::default());
        direct_context.is_frame_root = true;
        direct_context.recording_owner = Some(owner);
        direct_context.recording_owner_stable_id = Some(node.element.stable_id());
        direct_context.authoritative_self_clip =
            properties.authoritative_self_clip_for_owner(owner, state);
        assert_eq!(
            node.element
                .shadow_paint_recording_capability(&arena, false, direct_context),
            ShadowPaintRecordingCapability::Recordable,
            "state={state:?} context={direct_context:?}"
        );
        drop(node);
        let record = |mode: CoverageRecordingMode| {
            record_coverage_manifest(&arena, &roots, false, true, mode, &properties, &generations)
        };
        let metadata_manifest = record(CoverageRecordingMode::MetadataOnly);
        let full = record(CoverageRecordingMode::FullArtifact);
        assert!(metadata_manifest.validation_errors.is_empty());
        assert!(full.validation_errors.is_empty());
        let [
            PaintCoverageItem::ArtifactChunk {
                chunk: metadata, ..
            },
        ] = metadata_manifest.items.as_slice()
        else {
            panic!(
                "exact clipped Image metadata must remain one native chunk: {:?}",
                metadata_manifest.items
            )
        };
        let [
            PaintCoverageItem::ArtifactChunk {
                chunk: full_chunk,
                ops: Some(ops),
                clip_snapshot,
                ..
            },
        ] = full.items.as_slice()
        else {
            panic!("exact clipped Image full recording must carry its clip snapshot")
        };
        assert_eq!(metadata.payload_identity, full_chunk.payload_identity);
        assert!(matches!(
            &metadata.payload_identity,
            PaintPayloadIdentity::ImageWithShadows(_, shadows, _) if shadows.len() == 1
        ));
        assert!(matches!(
            ops.first(),
            Some(crate::view::paint::PaintOp::PreparedShadow(_))
        ));
        assert!(matches!(
            ops.last(),
            Some(crate::view::paint::PaintOp::PreparedImage(_))
        ));
        let [clip] = clip_snapshot.as_slice() else {
            panic!("exact clipped Image must carry one complete self-clip snapshot")
        };
        assert_eq!(clip.id, full_chunk.properties.clip.unwrap());
    }

    #[test]
    fn image_wrapper_outer_shadow_root_opacity_is_applied_once() {
        let mut image = Image::new_with_id(0x9195, path_source("shadow-root-opacity"));
        let mut style = Style::new();
        style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::rgb(20, 180, 40)),
        );
        style.insert(
            PropertyId::Opacity,
            ParsedValue::Opacity(crate::style::Opacity::new(0.4)),
        );
        style.set_border(crate::style::Border::uniform(
            Length::px(2.0),
            &Color::hex("#102030"),
        ));
        style.set_box_shadow(vec![BoxShadow::new().offset_x(1.5).offset_y(-2.25)]);
        image.apply_style(style);
        crate::view::image_resource::set_image_loading_for_test(image.source_handle.asset_id());
        let mut arena = new_test_arena();
        let owner = commit_element(&mut arena, Box::new(image));
        measure_and_place(
            &mut arena,
            owner,
            LayoutConstraints {
                max_width: 100.0,
                max_height: 100.0,
                viewport_width: 100.0,
                viewport_height: 100.0,
                percent_base_width: None,
                percent_base_height: None,
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 100.0,
                available_height: 100.0,
                viewport_width: 100.0,
                viewport_height: 100.0,
                percent_base_width: None,
                percent_base_height: None,
            },
        );
        let mut properties = PropertyTrees::default();
        properties.sync(&arena, &[owner]);
        let mut generations = PaintGenerationTracker::default();
        generations.sync(&arena, &[owner], &properties);
        let outcome = crate::view::paint::record_root_group_opacity_frame_artifact(
            &arena,
            &[owner],
            &properties,
            &generations,
            crate::view::paint::RendererMode::ForcedForTests,
        )
        .unwrap();
        let crate::view::paint::FrameArtifactRecordOutcome::Artifact { artifact, .. } = outcome
        else {
            panic!("Image wrapper root-opacity must record")
        };
        assert_eq!(artifact.effect_nodes.len(), 1);
        assert_eq!(
            artifact.effect_nodes[0].opacity.to_bits(),
            0.4_f32.to_bits()
        );
        assert!(matches!(
            artifact.ops.as_slice(),
            [
                crate::view::paint::PaintOp::PreparedShadow(shadow),
                crate::view::paint::PaintOp::DrawRect(fill),
                crate::view::paint::PaintOp::DrawRect(border),
                ..
            ] if shadow.params.opacity.to_bits() == 1.0_f32.to_bits()
                && fill.params.opacity.to_bits() == 1.0_f32.to_bits()
                && border.params.opacity.to_bits() == 1.0_f32.to_bits()
        ));
    }

    #[test]
    fn arena_sync_loading_slot_topology_marks_layout_dirty_before_measure() {
        let mut arena = new_test_arena();
        let image_key = commit_element(
            &mut arena,
            Box::new(Image::new_with_id(32, rgba_source(1, 1))),
        );
        let slot_key = commit_child(
            &mut arena,
            image_key,
            Box::new(Element::new_with_id(33, 0.0, 0.0, 4.0, 4.0)),
        );
        arena.with_element_taken(image_key, |element, arena| {
            let image = element
                .as_any_mut()
                .downcast_mut::<Image>()
                .expect("image host");
            image.attach_loading_slot_cold(vec![slot_key]);
            crate::view::image_resource::set_image_loading_for_test(image.source_handle.asset_id());
            image.clear_local_dirty_flags(DirtyFlags::ALL);
            image.sync_arena(arena);
            assert_eq!(image.active_slot, super::ActiveSlot::Loading);
            assert_eq!(image.element.children(), &[slot_key]);
            assert!(image.local_dirty_flags().contains(DirtyFlags::LAYOUT));
        });
    }

    #[test]
    fn auto_size_uses_intrinsic_dimensions_when_loaded() {
        let mut image = Image::new_with_id(1, rgba_source(80, 40));
        image.apply_style(Style::new());
        let mut arena = new_test_arena();
        image.measure(
            LayoutConstraints {
                max_width: 500.0,
                max_height: 500.0,
                viewport_width: 500.0,
                viewport_height: 500.0,
                percent_base_width: None,
                percent_base_height: None,
            },
            &mut arena,
        );
        assert_eq!(image.measured_size(), (80.0, 40.0));
    }

    #[test]
    fn texture_bounds_apply_host_paint_offset_without_changing_size() {
        let element = Element::new(10.25, 20.75, 100.0, 50.0);
        let parent_paint_offset = [0.2, -0.3];
        let bounds = [18.25, 24.5, 80.0, 40.0];

        let adjusted = super::paint_adjusted_texture_bounds(&element, parent_paint_offset, bounds);

        let expected_dx = (10.25_f32 + parent_paint_offset[0]).round()
            - (10.25_f32 + parent_paint_offset[0])
            + parent_paint_offset[0];
        let expected_dy = (20.75_f32 + parent_paint_offset[1]).round()
            - (20.75_f32 + parent_paint_offset[1])
            + parent_paint_offset[1];
        assert!((adjusted[0] - (bounds[0] + expected_dx)).abs() < 0.001);
        assert!((adjusted[1] - (bounds[1] + expected_dy)).abs() < 0.001);
        assert_eq!(adjusted[2], bounds[2]);
        assert_eq!(adjusted[3], bounds[3]);
    }

    #[test]
    fn transformed_image_wrapper_and_untransformed_media_expand_parent_surface_in_order() {
        let mut parent = Element::new_with_id(0x9200, 0.0, 0.0, 10.0, 10.0);
        parent.set_resolved_transform_for_test(Some(Mat4::from_translation(Vec3::new(
            100.0, 0.0, 0.0,
        ))));
        let mut image = Image::new_with_id(0x9201, rgba_source(4, 2));
        image.element = Element::new_with_id(0x9201, 100.0, 2.0, 4.0, 2.0);
        image
            .element
            .set_resolved_transform_for_test(Some(Mat4::from_translation(Vec3::new(
                -100.0, 0.0, 0.0,
            ))));

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let _image_key = commit_child(&mut arena, parent_key, Box::new(image));
        let geometry = crate::view::test_support::get_element::<Element>(&arena, parent_key)
            .exact_transform_surface_geometry_snapshot(&arena, [0.0, 0.0], None)
            .expect("Image explicitly supplies exact wrapper plus media coverage");
        assert_eq!(
            [
                geometry.source_bounds.x.to_bits(),
                geometry.source_bounds.y.to_bits(),
                geometry.source_bounds.width.to_bits(),
                geometry.source_bounds.height.to_bits(),
            ],
            [
                0.0_f32.to_bits(),
                0.0_f32.to_bits(),
                104.0_f32.to_bits(),
                10.0_f32.to_bits(),
            ],
            "wrapper moves to x=0..4, but the sampled media still paints at x=100..104"
        );

        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(100, 80, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let outer_target = ctx.allocate_target(&mut graph);
        ctx.set_current_target(outer_target);
        arena
            .with_element_taken(parent_key, |element, arena| {
                element.build(&mut graph, arena, ctx)
            })
            .expect("transformed parent containing Image");

        let composites =
            graph.test_graphics_passes::<crate::view::render_pass::TextureCompositePass>();
        assert_eq!(composites.len(), 3);
        let wrapper = composites[0].test_snapshot();
        let media = composites[1].test_snapshot();
        let parent = composites[2].test_snapshot();
        assert!(wrapper.source_handle.is_some());
        assert!(media.source_handle.is_none(), "media is a sampled upload");
        assert_eq!(
            media.bounds_bits,
            [100.0, 2.0, 4.0, 2.0].map(f32::to_bits),
            "the media pass remains untransformed even though the embedded Element wrapper moves"
        );
        assert_eq!(wrapper.output_target, media.output_target);
        assert_eq!(media.output_target, parent.source_handle);
        assert_eq!(parent.output_target, outer_target.handle());
        assert_eq!(
            graph.declared_persistent_textures().count(),
            4,
            "parent and Image wrapper each own one color/depth surface pair"
        );
    }

    #[test]
    fn contain_preserves_aspect_ratio_inside_subpixel_destination() {
        let (draw, uv) =
            super::compute_image_mapping(crate::view::ImageFit::Contain, 4.0, 2.0, 0.5, 0.5);
        assert_eq!(
            draw.map(f32::to_bits),
            [0.0, 0.125, 0.5, 0.25].map(f32::to_bits)
        );
        assert_eq!(uv.map(f32::to_bits), [0.0, 0.0, 4.0, 2.0].map(f32::to_bits));
        assert!(draw[0] >= 0.0 && draw[1] >= 0.0);
        assert!(draw[0] + draw[2] <= 0.5);
        assert!(draw[1] + draw[3] <= 0.5);
    }

    #[test]
    fn invalid_image_mapping_is_empty() {
        assert_eq!(
            super::compute_image_mapping(crate::view::ImageFit::Fill, 4.0, 2.0, 0.0, 0.5),
            ([0.0; 4], [0.0; 4])
        );
    }

    #[test]
    fn computed_style_consumer_syncs_image_element_render_state() {
        let mut image = Image::new_with_id(2, rgba_source(80, 40));
        let mut computed = ComputedStyle::default();
        computed.background_color = Color::rgb(20, 30, 40);
        computed.border_colors = EdgeInsets {
            top: Color::rgb(200, 0, 0),
            right: Color::rgb(0, 200, 0),
            bottom: Color::rgb(0, 0, 200),
            left: Color::rgb(200, 200, 0),
        };
        computed.opacity = 0.4;

        ComputedStyleConsumer::apply_computed_style(&mut image, computed, None);

        let render_state = image.element.debug_render_state();
        assert_eq!(render_state.background_rgba, [20, 30, 40, 255]);
        assert_eq!(render_state.border_top_rgba, [200, 0, 0, 255]);
        assert_eq!(render_state.border_right_rgba, [0, 200, 0, 255]);
        assert_eq!(render_state.border_bottom_rgba, [0, 0, 200, 255]);
        assert_eq!(render_state.border_left_rgba, [200, 200, 0, 255]);
        assert!((render_state.opacity - 0.4).abs() < 0.001);
    }

    #[test]
    fn flex_distribution_does_not_feed_back_into_image_basis() {
        let mut parent = Element::new(0.0, 0.0, 100.0, 100.0);
        let mut parent_style = Style::new();
        parent_style.insert(
            PropertyId::Layout,
            ParsedValue::Layout(Layout::flex().row().into()),
        );
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(100.0)));
        parent_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(100.0)));
        parent.apply_style(parent_style);

        let mut image = Image::new_with_id(2, rgba_source(20, 20));
        let mut image_style = Style::new();
        image_style.insert(
            PropertyId::Flex,
            ParsedValue::Flex(crate::style::flex().shrink(1.0)),
        );
        image.apply_style(image_style);

        let mut sibling = Element::new(0.0, 0.0, 120.0, 20.0);
        let mut sibling_style = Style::new();
        sibling_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(120.0)));
        sibling_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(20.0)));
        sibling_style.insert(
            PropertyId::Flex,
            ParsedValue::Flex(crate::style::flex().shrink(1.0)),
        );
        sibling.apply_style(sibling_style);

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let image_key = commit_child(&mut arena, parent_key, Box::new(image));
        let sibling_key = commit_child(&mut arena, parent_key, Box::new(sibling));

        let constraints = LayoutConstraints {
            max_width: 800.0,
            max_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        };
        let placement = LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 800.0,
            available_height: 600.0,
            viewport_width: 800.0,
            percent_base_width: Some(800.0),
            percent_base_height: Some(600.0),
            viewport_height: 600.0,
        };

        arena.with_element_taken(parent_key, |el, arena_ref| {
            el.measure(constraints, arena_ref);
            el.place(placement, arena_ref);
        });

        let image_snapshot = arena.get(image_key).unwrap().element.box_model_snapshot();
        let sibling_snapshot = arena.get(sibling_key).unwrap().element.box_model_snapshot();
        assert_eq!(image_snapshot.width, 14.285714);
        assert_eq!(sibling_snapshot.width, 85.71429);

        arena.with_element_taken(parent_key, |el, arena_ref| {
            if let Some(e) = el.as_any_mut().downcast_mut::<Element>() {
                e.mark_layout_dirty();
            }
            el.measure(constraints, arena_ref);
            el.place(placement, arena_ref);
        });
        let image_snapshot = arena.get(image_key).unwrap().element.box_model_snapshot();
        let sibling_snapshot = arena.get(sibling_key).unwrap().element.box_model_snapshot();
        assert_eq!(image_snapshot.width, 14.285714);
        assert_eq!(sibling_snapshot.width, 85.71429);
    }
}
