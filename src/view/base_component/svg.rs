use crate::style::{ComputedStyle, ParsedValue, PropertyId, Style};
use crate::time::{Duration, Instant};
use crate::view::frame_graph::FrameGraph;
use crate::view::image_resource::ImageSnapshot;
use crate::view::render_pass::TextureCompositePass;
use crate::view::render_pass::texture_composite_pass::{
    TextureCompositeInput, TextureCompositeOutput, TextureCompositeParams,
};
use crate::view::sampled_texture::{SampledTextureAlphaMode, SampledTextureUpload};
use crate::view::svg_resource::{
    SvgDocumentSnapshot, SvgRasterMode, SvgRasterRequest, acquire_svg_document, acquire_svg_raster,
    quantize_svg_raster_size, quantize_svg_uniform_raster_size, release_svg_document,
    release_svg_raster, snapshot_svg_document, snapshot_svg_raster,
    svg_raster_asset_id_for_request,
};
use crate::view::{ImageFit, ImageSampling, SvgSource};
use rustc_hash::FxHashSet;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use super::resource_slot::{self, ActiveSlot, SlotReplacementError};
use super::{
    BoxModelSnapshot, ComputedStyleConsumer, Element, ElementStyleSnapshot, ElementTrait,
    EventTarget, LayoutConstraints, LayoutPlacement, Layoutable, Renderable, UiBuildContext,
};

const PLACEHOLDER_SIZE: f32 = 120.0;
const SVG_RESIZE_REQUEST_COOLDOWN: Duration = Duration::from_millis(90);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum SvgSourceKind {
    Path,
    Content,
}

impl SvgSourceKind {
    fn from_source(source: &SvgSource) -> Self {
        match source {
            SvgSource::Path(_) => Self::Path,
            SvgSource::Content(_) => Self::Content,
        }
    }
}

fn hash_svg_raster_state<H: Hasher>(
    raster_key: Option<u64>,
    raster_size: Option<(u32, u32)>,
    snapshot: Option<&ImageSnapshot>,
    hasher: &mut H,
) {
    raster_key.hash(hasher);
    raster_size.hash(hasher);
    super::image::hash_image_snapshot(snapshot, hasher);
}

fn same_document_snapshot(
    left: Option<&SvgDocumentSnapshot>,
    right: Option<&SvgDocumentSnapshot>,
) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(SvgDocumentSnapshot::Loading), Some(SvgDocumentSnapshot::Loading)) => true,
        (
            Some(SvgDocumentSnapshot::Ready {
                intrinsic_width: left_width,
                intrinsic_height: left_height,
            }),
            Some(SvgDocumentSnapshot::Ready {
                intrinsic_width: right_width,
                intrinsic_height: right_height,
            }),
        ) => {
            left_width.to_bits() == right_width.to_bits()
                && left_height.to_bits() == right_height.to_bits()
        }
        (Some(SvgDocumentSnapshot::Error(left)), Some(SvgDocumentSnapshot::Error(right))) => {
            left == right
        }
        _ => false,
    }
}

fn same_raster_snapshot(left: Option<&ImageSnapshot>, right: Option<&ImageSnapshot>) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(ImageSnapshot::Loading), Some(ImageSnapshot::Loading)) => true,
        (Some(ImageSnapshot::Error(left)), Some(ImageSnapshot::Error(right))) => left == right,
        (Some(ImageSnapshot::Ready(left)), Some(ImageSnapshot::Ready(right))) => {
            left.sampled_texture_id == right.sampled_texture_id
                && left.generation == right.generation
                && left.width == right.width
                && left.height == right.height
                && left.pixels.len() == right.pixels.len()
        }
        _ => false,
    }
}

#[derive(Clone, Copy, Debug)]
struct SvgRasterPlan {
    request: SvgRasterRequest,
    local_draw_bounds: [f32; 4],
    uv_bounds: [f32; 4],
}

#[derive(Clone, Debug)]
struct FrozenSvgPaint {
    document_key: u64,
    raster_key: u64,
    device_scale_bits: u32,
    plan: SvgRasterPlan,
    inner_origin: [f32; 2],
    upload: SampledTextureUpload,
    opacity: f32,
}

#[derive(Clone, Debug)]
enum SvgShadowPaintClass {
    ReadyExact(crate::view::paint::PreparedSvgOp),
    ActiveSlotWrapper(ActiveSlot),
}

pub struct Svg {
    element: Element,
    source_key: u64,
    source_kind: SvgSourceKind,
    fit: ImageFit,
    sampling: ImageSampling,
    loading_slot: Vec<crate::view::node_arena::NodeKey>,
    error_slot: Vec<crate::view::node_arena::NodeKey>,
    active_slot: ActiveSlot,
    active_raster_key: Option<u64>,
    active_raster_request: Option<SvgRasterRequest>,
    active_device_scale_bits: Option<u32>,
    pending_raster_key: Option<u64>,
    pending_raster_request: Option<SvgRasterRequest>,
    pending_device_scale_bits: Option<u32>,
    failed_raster_request: Option<SvgRasterRequest>,
    last_raster_request_at: Option<Instant>,
    frozen_document_key: Option<u64>,
    frozen_document: Option<SvgDocumentSnapshot>,
    frozen_active_raster_key: Option<u64>,
    frozen_active_raster: Option<ImageSnapshot>,
    frozen_pending_raster_key: Option<u64>,
    frozen_pending_raster: Option<ImageSnapshot>,
    frozen_paint: Option<FrozenSvgPaint>,
    frozen_desired_request: Option<SvgRasterRequest>,
    frozen_request_is_exact: bool,
    prepared_by_arena_sync: bool,
    prepared_frame_number: Option<u64>,
}

impl Svg {
    #[cfg(test)]
    pub(crate) fn set_layout_transition_width_for_test(&mut self, width: f32) {
        self.element.set_layout_transition_width(width);
    }

    #[cfg(test)]
    pub(crate) fn set_document_loading_for_transform_test(&self) {
        crate::view::svg_resource::set_svg_document_loading_for_test(self.source_key);
    }

    #[cfg(test)]
    pub(crate) fn set_document_error_for_transform_test(&self) {
        crate::view::svg_resource::set_svg_document_error_for_test(self.source_key);
    }

    #[cfg(test)]
    pub(crate) fn replace_active_raster_generation_for_test(&self, fill: u8) -> u64 {
        let key = self
            .active_raster_key
            .expect("ready SVG test fixture must own an active raster");
        let request = self
            .active_raster_request
            .expect("ready SVG test fixture must own an active request");
        crate::view::svg_resource::replace_svg_raster_ready_for_test(
            key,
            request.physical_width,
            request.physical_height,
            std::sync::Arc::from(vec![
                fill;
                (request.physical_width * request.physical_height * 4)
                    as usize
            ]),
        )
    }

    pub fn new_with_id(id: u64, source: SvgSource) -> Self {
        let mut element = Element::new_with_id(id, 0.0, 0.0, PLACEHOLDER_SIZE, PLACEHOLDER_SIZE);
        let mut base_style = Style::new();
        base_style.insert(PropertyId::Width, ParsedValue::Auto);
        base_style.insert(PropertyId::Height, ParsedValue::Auto);
        element.apply_style(base_style);
        let source_kind = SvgSourceKind::from_source(&source);
        Self {
            element,
            source_key: acquire_svg_document(&source),
            source_kind,
            fit: ImageFit::Contain,
            sampling: ImageSampling::Linear,
            loading_slot: Vec::new(),
            error_slot: Vec::new(),
            active_slot: ActiveSlot::None,
            active_raster_key: None,
            active_raster_request: None,
            active_device_scale_bits: None,
            pending_raster_key: None,
            pending_raster_request: None,
            pending_device_scale_bits: None,
            failed_raster_request: None,
            last_raster_request_at: None,
            frozen_document_key: None,
            frozen_document: None,
            frozen_active_raster_key: None,
            frozen_active_raster: None,
            frozen_pending_raster_key: None,
            frozen_pending_raster: None,
            frozen_paint: None,
            frozen_desired_request: None,
            frozen_request_is_exact: false,
            prepared_by_arena_sync: false,
            prepared_frame_number: None,
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

    pub fn apply_style(&mut self, style: crate::style::Style) {
        self.element.apply_style(style);
    }

    /// Cold descriptor-commit hook; runtime replacement must use the
    /// arena-aware `replace_*_slot_incremental` path below.
    pub(crate) fn attach_loading_slot_cold(&mut self, slot: Vec<crate::view::node_arena::NodeKey>) {
        resource_slot::attach_slot_cold(self.active_slot, &mut self.loading_slot, slot);
    }

    pub(crate) fn attach_error_slot_cold(&mut self, slot: Vec<crate::view::node_arena::NodeKey>) {
        resource_slot::attach_slot_cold(self.active_slot, &mut self.error_slot, slot);
    }

    /// 軌 1 #3: mirror of `Image::replace_loading_slot_incremental`
    /// for the incremental-commit hot-swap path. See that method for
    /// the invariant sequence (atomic preflight → drain active slot → drop
    /// target old keys → install new keys; pre-layout sync re-runs next frame).
    pub(crate) fn replace_loading_slot_incremental(
        &mut self,
        arena: &mut crate::view::node_arena::NodeArena,
        owner: crate::view::node_arena::NodeKey,
        new_keys: &[crate::view::node_arena::NodeKey],
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
        arena: &mut crate::view::node_arena::NodeArena,
        owner: crate::view::node_arena::NodeKey,
        new_keys: &[crate::view::node_arena::NodeKey],
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
    /// when inactive (mirror of `Image::loading_slot_len`).
    #[cfg(test)]
    pub(crate) fn loading_slot_len(&self) -> usize {
        use crate::view::base_component::ElementTrait;
        if matches!(self.active_slot, ActiveSlot::Loading) {
            self.element.children().len()
        } else {
            self.loading_slot.len()
        }
    }

    /// 軌 1 #4: hot-swap the Svg source. Releases the old document +
    /// any live raster and acquires the new document. The next
    /// `measure` call will re-resolve the active slot and re-request
    /// a raster sized to the new document.
    pub fn set_source(&mut self, source: SvgSource) {
        let next_source_kind = SvgSourceKind::from_source(&source);
        let next_source_key = acquire_svg_document(&source);
        if next_source_key == self.source_key {
            // The registry compares normalized, tagged owning identities. Undo
            // the speculative retain and keep the ready raster alive.
            release_svg_document(next_source_key);
            return;
        }
        release_svg_document(self.source_key);
        self.source_key = next_source_key;
        self.source_kind = next_source_kind;
        if let Some(raster_key) = self.active_raster_key.take() {
            release_svg_raster(raster_key);
        }
        if let Some(raster_key) = self.pending_raster_key.take() {
            release_svg_raster(raster_key);
        }
        self.active_raster_request = None;
        self.active_device_scale_bits = None;
        self.pending_raster_request = None;
        self.pending_device_scale_bits = None;
        self.failed_raster_request = None;
        self.last_raster_request_at = None;
        self.frozen_document_key = None;
        self.frozen_document = None;
        self.frozen_active_raster_key = None;
        self.frozen_active_raster = None;
        self.frozen_pending_raster_key = None;
        self.frozen_pending_raster = None;
        self.frozen_paint = None;
        self.frozen_desired_request = None;
        self.frozen_request_is_exact = false;
        self.prepared_by_arena_sync = false;
        self.prepared_frame_number = None;
        self.element.mark_layout_dirty();
    }

    fn document_snapshot(&self) -> SvgDocumentSnapshot {
        snapshot_svg_document(self.source_key).unwrap_or(SvgDocumentSnapshot::Loading)
    }

    fn refresh_frozen_resources(&mut self, arena: &mut crate::view::node_arena::NodeArena) {
        let document = self.document_snapshot();
        let active_raster = self
            .active_raster_key
            .and_then(snapshot_svg_raster)
            .or_else(|| self.active_raster_key.map(|_| ImageSnapshot::Loading));
        let pending_raster = self
            .pending_raster_key
            .and_then(snapshot_svg_raster)
            .or_else(|| self.pending_raster_key.map(|_| ImageSnapshot::Loading));
        let next_slot = Self::resolve_frozen_slot(&document, active_raster.as_ref());
        let document_changed =
            !same_document_snapshot(self.frozen_document.as_ref(), Some(&document));
        let raster_changed =
            !same_raster_snapshot(self.frozen_active_raster.as_ref(), active_raster.as_ref())
                || !same_raster_snapshot(
                    self.frozen_pending_raster.as_ref(),
                    pending_raster.as_ref(),
                );
        let slot_changed = self.active_slot != next_slot;

        self.frozen_document_key = Some(self.source_key);
        self.frozen_document = Some(document);
        self.frozen_active_raster_key = self.active_raster_key;
        self.frozen_active_raster = active_raster;
        self.frozen_pending_raster_key = self.pending_raster_key;
        self.frozen_pending_raster = pending_raster;
        self.frozen_paint = None;
        self.frozen_desired_request = None;
        self.frozen_request_is_exact = false;
        self.prepared_frame_number = None;
        self.sync_active_slot(arena, next_slot);
        if document_changed || slot_changed {
            self.element.mark_layout_dirty();
        } else if raster_changed {
            self.element.mark_paint_dirty();
        }
    }

    fn sync_active_slot(
        &mut self,
        arena: &mut crate::view::node_arena::NodeArena,
        next_slot: ActiveSlot,
    ) {
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

    fn intrinsic_size(snapshot: &SvgDocumentSnapshot) -> (f32, f32) {
        match snapshot {
            SvgDocumentSnapshot::Ready {
                intrinsic_width,
                intrinsic_height,
            } => (intrinsic_width.max(1.0), intrinsic_height.max(1.0)),
            SvgDocumentSnapshot::Loading | SvgDocumentSnapshot::Error(_) => {
                (PLACEHOLDER_SIZE, PLACEHOLDER_SIZE)
            }
        }
    }

    fn resolve_frozen_slot(
        document: &SvgDocumentSnapshot,
        raster: Option<&ImageSnapshot>,
    ) -> ActiveSlot {
        match document {
            SvgDocumentSnapshot::Loading => ActiveSlot::Loading,
            SvgDocumentSnapshot::Error(message) => {
                let _ = message;
                ActiveSlot::Error
            }
            SvgDocumentSnapshot::Ready { .. } => match raster {
                Some(ImageSnapshot::Ready(_)) => ActiveSlot::None,
                Some(ImageSnapshot::Error(_)) => ActiveSlot::Error,
                Some(ImageSnapshot::Loading) | None => ActiveSlot::Loading,
            },
        }
    }

    fn resolve_raster_plan(
        &self,
        source_w: f32,
        source_h: f32,
        dest_w: f32,
        dest_h: f32,
        device_scale: f32,
    ) -> Option<SvgRasterPlan> {
        if source_w <= 0.0
            || source_h <= 0.0
            || dest_w <= 0.0
            || dest_h <= 0.0
            || !device_scale.is_finite()
            || device_scale <= 0.0
        {
            return None;
        }
        let device_scale = device_scale.max(0.0001);
        let request = match self.fit {
            ImageFit::Fill => {
                let (width, height) = quantize_svg_raster_size(
                    (dest_w * device_scale).ceil().max(1.0) as u32,
                    (dest_h * device_scale).ceil().max(1.0) as u32,
                );
                SvgRasterRequest::new(width, height, SvgRasterMode::Fill)
            }
            ImageFit::Contain | ImageFit::Cover => {
                let fit_scale = match self.fit {
                    ImageFit::Contain => (dest_w / source_w).min(dest_h / source_h),
                    ImageFit::Cover => (dest_w / source_w).max(dest_h / source_h),
                    ImageFit::Fill => unreachable!(),
                };
                let (width, height) =
                    quantize_svg_uniform_raster_size(source_w, source_h, fit_scale * device_scale);
                SvgRasterRequest::new(width, height, SvgRasterMode::Uniform)
            }
        };
        Self::raster_plan_for_request(self.fit, source_w, source_h, dest_w, dest_h, request)
    }

    fn raster_plan_for_request(
        fit: ImageFit,
        source_w: f32,
        source_h: f32,
        dest_w: f32,
        dest_h: f32,
        request: SvgRasterRequest,
    ) -> Option<SvgRasterPlan> {
        if source_w <= 0.0 || source_h <= 0.0 || dest_w <= 0.0 || dest_h <= 0.0 {
            return None;
        }
        let (local_draw_bounds, intrinsic_uv) =
            super::image::compute_image_mapping(fit, source_w, source_h, dest_w, dest_h);
        let (scale_x, scale_y) = match request.mode {
            SvgRasterMode::Uniform => {
                let scale = (request.physical_width as f32 / source_w)
                    .min(request.physical_height as f32 / source_h);
                (scale, scale)
            }
            SvgRasterMode::Fill => (
                request.physical_width as f32 / source_w,
                request.physical_height as f32 / source_h,
            ),
        };
        Some(SvgRasterPlan {
            request,
            local_draw_bounds,
            uv_bounds: [
                intrinsic_uv[0] * scale_x,
                intrinsic_uv[1] * scale_y,
                intrinsic_uv[2] * scale_x,
                intrinsic_uv[3] * scale_y,
            ],
        })
    }

    fn upload_for_image(
        &self,
        image: &crate::view::image_resource::ReadyImage,
    ) -> Option<SampledTextureUpload> {
        let upload = SampledTextureUpload {
            id: image.sampled_texture_id,
            generation: image.generation,
            width: image.width,
            height: image.height,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            // rasterize_svg normalized tiny-skia's encoded-premultiplied
            // Pixmap bytes into straight sRGB before publishing the snapshot.
            alpha_mode: SampledTextureAlphaMode::Straight,
            pixels: image.pixels.clone(),
            sampling: self.sampling,
        };
        upload.validate_rgba8()?;
        Some(upload)
    }

    fn prepared_svg_op(
        &self,
        paint_offset: [f32; 2],
        opacity: f32,
    ) -> Option<crate::view::paint::PreparedSvgOp> {
        let frozen = self.frozen_paint.as_ref()?;
        Some(crate::view::paint::PreparedSvgOp {
            params: TextureCompositeParams {
                bounds: [
                    frozen.inner_origin[0] + frozen.plan.local_draw_bounds[0] + paint_offset[0],
                    frozen.inner_origin[1] + frozen.plan.local_draw_bounds[1] + paint_offset[1],
                    frozen.plan.local_draw_bounds[2],
                    frozen.plan.local_draw_bounds[3],
                ],
                quad_positions: None,
                uv_bounds: Some(frozen.plan.uv_bounds),
                mask_uv_bounds: None,
                use_mask: false,
                source_is_premultiplied: false,
                opacity: opacity.clamp(0.0, 1.0),
                scissor_rect: None,
            },
            upload: frozen.upload.clone(),
        })
    }

    fn should_keep_existing_raster(
        &self,
        request: SvgRasterRequest,
        device_scale: f32,
        now: Instant,
    ) -> bool {
        let Some(current) = self.active_raster_request else {
            return false;
        };
        if current == request {
            return true;
        }
        if current.mode != request.mode
            || self.active_device_scale_bits != Some(device_scale.max(0.0001).to_bits())
        {
            return false;
        }
        // Never keep an undersized backing. Hysteresis/cooldown are safe only
        // while shrinking, where retaining the larger raster preserves detail.
        if request.physical_width > current.physical_width
            || request.physical_height > current.physical_height
        {
            return false;
        }
        let within_cooldown = self
            .last_raster_request_at
            .is_some_and(|last| now.duration_since(last) < SVG_RESIZE_REQUEST_COOLDOWN);
        within_cooldown
    }

    fn sync_raster_key(
        &mut self,
        request: SvgRasterRequest,
        device_scale: f32,
        now: Instant,
    ) -> Option<u64> {
        let scale_bits = device_scale.max(0.0001).to_bits();
        if self.active_raster_request == Some(request) {
            if let Some(stale_pending) = self.pending_raster_key.take() {
                release_svg_raster(stale_pending);
            }
            self.pending_raster_request = None;
            self.pending_device_scale_bits = None;
            // The physical bucket may remain identical across a DPR change.
            // Reusing that raster is exact, but its frame-scale identity must
            // still advance or eligibility would remain stale forever.
            self.active_device_scale_bits = Some(scale_bits);
            self.failed_raster_request = None;
            return self.active_raster_key;
        }
        if self.failed_raster_request == Some(request) {
            return self.active_raster_key;
        }
        self.failed_raster_request = None;
        if self.pending_raster_request == Some(request) {
            let pending_key = self.pending_raster_key?;
            match self.frozen_pending_raster.take() {
                Some(ready @ ImageSnapshot::Ready(_)) => {
                    if let Some(previous) = self.active_raster_key.replace(pending_key) {
                        release_svg_raster(previous);
                    }
                    self.active_raster_request = self.pending_raster_request.take();
                    self.pending_device_scale_bits = None;
                    self.active_device_scale_bits = Some(scale_bits);
                    self.pending_raster_key = None;
                    self.frozen_active_raster = Some(ready);
                    return self.active_raster_key;
                }
                Some(ImageSnapshot::Error(_)) | None => {
                    release_svg_raster(pending_key);
                    self.pending_raster_key = None;
                    self.failed_raster_request = self.pending_raster_request.take();
                    self.pending_device_scale_bits = None;
                }
                Some(ImageSnapshot::Loading) => {
                    self.frozen_pending_raster = Some(ImageSnapshot::Loading);
                }
            }
            return self.active_raster_key;
        }
        if self.should_keep_existing_raster(request, device_scale, now) {
            return self.active_raster_key;
        }
        if let Some(previous_pending) = self.pending_raster_key.take() {
            release_svg_raster(previous_pending);
        }
        self.pending_raster_request = None;
        self.pending_device_scale_bits = None;
        if self
            .active_raster_request
            .is_some_and(|active| active.mode != request.mode)
        {
            if let Some(previous) = self.active_raster_key.take() {
                release_svg_raster(previous);
            }
            self.active_raster_request = None;
            self.active_device_scale_bits = None;
        }
        let raster_key = acquire_svg_raster(self.source_key, request);
        self.last_raster_request_at = Some(now);
        if self.active_raster_key.is_none() {
            self.active_raster_key = Some(raster_key);
            self.active_raster_request = Some(request);
            self.active_device_scale_bits = Some(scale_bits);
            // A key acquired after the pre-layout freeze is deliberately not
            // snapshotted in this frame, even if another host already made
            // the shared registry entry ready.
            self.frozen_active_raster = Some(ImageSnapshot::Loading);
        } else {
            self.pending_raster_key = Some(raster_key);
            self.pending_raster_request = Some(request);
            self.pending_device_scale_bits = Some(scale_bits);
            self.frozen_pending_raster = Some(ImageSnapshot::Loading);
        }
        self.active_raster_key
    }

    fn prepare_frozen_paint(&mut self, context: super::PaintResourcePreparationContext) {
        if self.prepared_frame_number == Some(context.frame_number) {
            return;
        }
        self.prepared_frame_number = Some(context.frame_number);
        self.frozen_paint = None;
        self.frozen_desired_request = None;
        self.frozen_request_is_exact = false;

        let Some(SvgDocumentSnapshot::Ready {
            intrinsic_width,
            intrinsic_height,
        }) = self.frozen_document.clone()
        else {
            return;
        };
        if self.frozen_document_key != Some(self.source_key) {
            return;
        }
        let (inner_x, inner_y, inner_w, inner_h) = self.element.inner_content_rect_for_render();
        let Some(desired_plan) = self.resolve_raster_plan(
            intrinsic_width,
            intrinsic_height,
            inner_w,
            inner_h,
            context.device_scale,
        ) else {
            return;
        };
        self.frozen_desired_request = Some(desired_plan.request);
        let _ = self.sync_raster_key(desired_plan.request, context.device_scale, context.now);

        // A post-layout resource completion must never rewrite the child slot
        // selected before layout. If the frozen resource now implies a
        // different topology, this frame stays legacy and the next frame's
        // pre-layout sync performs the slot transition.
        if Self::resolve_frozen_slot(
            self.frozen_document.as_ref().expect("document was frozen"),
            self.frozen_active_raster.as_ref(),
        ) != self.active_slot
        {
            return;
        }
        let Some(ImageSnapshot::Ready(image)) = self.frozen_active_raster.as_ref() else {
            return;
        };
        let Some(active_raster_key) = self.active_raster_key else {
            return;
        };
        let Some(active_request) = self.active_raster_request else {
            return;
        };
        let device_scale_bits = context.device_scale.max(0.0001).to_bits();
        let Some(expected_asset_id) =
            svg_raster_asset_id_for_request(active_raster_key, self.source_key, active_request)
        else {
            return;
        };
        if image.sampled_texture_id
            != crate::view::sampled_texture::SampledTextureId::SvgRaster(expected_asset_id)
        {
            return;
        }
        let Some(active_plan) = Self::raster_plan_for_request(
            self.fit,
            intrinsic_width,
            intrinsic_height,
            inner_w,
            inner_h,
            active_request,
        ) else {
            return;
        };
        let Some(upload) = self.upload_for_image(image) else {
            return;
        };
        self.frozen_paint = Some(FrozenSvgPaint {
            document_key: self.source_key,
            raster_key: active_raster_key,
            device_scale_bits,
            plan: active_plan,
            inner_origin: [inner_x, inner_y],
            upload,
            opacity: self
                .element
                .retained_paint_properties()
                .opacity
                .clamp(0.0, 1.0),
        });
        self.frozen_request_is_exact = self.active_raster_request == Some(desired_plan.request)
            && self.active_device_scale_bits == Some(device_scale_bits)
            && self.pending_raster_request.is_none();
    }

    fn classify_shadow_paint(
        &self,
        arena: &crate::view::node_arena::NodeArena,
        expected_owner: Option<crate::view::node_arena::NodeKey>,
        properties: Option<crate::view::compositor::property_tree::PropertyTreeState>,
        deferred_phase_root: bool,
        recording_context: crate::view::paint::PaintRecordingContext,
    ) -> Result<SvgShadowPaintClass, super::ShadowPaintBlocker> {
        let indexed_owner = arena
            .find_by_stable_id(self.stable_id())
            .ok_or(super::ShadowPaintBlocker::MissingPreparedSvg)?;
        let owner = expected_owner.unwrap_or(indexed_owner);
        if owner != indexed_owner
            || !arena.contains_key(owner)
            || self.element.children() != arena.children_of(owner)
        {
            return Err(super::ShadowPaintBlocker::MissingPreparedSvg);
        }

        match (&self.frozen_document, self.active_slot) {
            (Some(SvgDocumentSnapshot::Ready { .. }), ActiveSlot::None) => {
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
                if !self.element.children().is_empty() || !self.frozen_request_is_exact {
                    return Err(super::ShadowPaintBlocker::MissingPreparedSvg);
                }
                let mut inactive_roots = FxHashSet::default();
                for &inactive_root in self.loading_slot.iter().chain(self.error_slot.iter()) {
                    if !inactive_roots.insert(inactive_root)
                        || !arena.contains_key(inactive_root)
                        || arena.parent_of(inactive_root) != Some(owner)
                    {
                        return Err(super::ShadowPaintBlocker::MissingPreparedSvg);
                    }
                }
                let prepared = self
                    .prepared_svg_op(
                        recording_context.paint_offset,
                        recording_context
                            .paint_opacity(self.frozen_paint.as_ref().map_or(0.0, |p| p.opacity)),
                    )
                    .ok_or(super::ShadowPaintBlocker::MissingPreparedSvg)?;
                if crate::view::paint::PreparedSvgIdentity::from_op(&prepared).is_none() {
                    return Err(super::ShadowPaintBlocker::MissingPreparedSvg);
                }
                let frozen = self
                    .frozen_paint
                    .as_ref()
                    .ok_or(super::ShadowPaintBlocker::MissingPreparedSvg)?;
                let current_asset_id = svg_raster_asset_id_for_request(
                    frozen.raster_key,
                    frozen.document_key,
                    frozen.plan.request,
                );
                if frozen.document_key != self.source_key
                    || self.frozen_document_key != Some(self.source_key)
                    || self.active_raster_key != Some(frozen.raster_key)
                    || self.active_raster_request != Some(frozen.plan.request)
                    || self.frozen_desired_request != Some(frozen.plan.request)
                    || self.active_device_scale_bits != Some(frozen.device_scale_bits)
                    || self.pending_raster_key.is_some()
                    || self.pending_raster_request.is_some()
                    || self.pending_device_scale_bits.is_some()
                    || current_asset_id
                        .map(crate::view::sampled_texture::SampledTextureId::SvgRaster)
                        != Some(frozen.upload.id)
                    || frozen.plan.local_draw_bounds[2] <= 0.0
                    || frozen.plan.local_draw_bounds[3] <= 0.0
                {
                    return Err(super::ShadowPaintBlocker::MissingPreparedSvg);
                }
                Ok(SvgShadowPaintClass::ReadyExact(prepared))
            }
            (Some(document), ActiveSlot::Loading | ActiveSlot::Error) => {
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
                    if let Some(effect) = properties.effect
                        && !matches!(
                            recording_context.opacity_authority,
                            crate::view::paint::PaintOpacityAuthority::NeutralRootEffect(authority)
                                if authority == effect
                        )
                    {
                        return Err(super::ShadowPaintBlocker::StatefulPaint);
                    }
                }
                let resolved =
                    Self::resolve_frozen_slot(document, self.frozen_active_raster.as_ref());
                if self.frozen_document_key != Some(self.source_key)
                    || resolved != self.active_slot
                    || self.active_raster_key != self.frozen_active_raster_key
                    || self.pending_raster_key != self.frozen_pending_raster_key
                    || self.frozen_active_raster_key.is_some()
                        != self.frozen_active_raster.is_some()
                    || self.frozen_pending_raster_key.is_some()
                        != self.frozen_pending_raster.is_some()
                    || self.frozen_paint.is_some()
                    || self.frozen_request_is_exact
                {
                    return Err(super::ShadowPaintBlocker::MissingPreparedSvg);
                }
                let active_target_is_empty = match self.active_slot {
                    ActiveSlot::Loading => self.loading_slot.is_empty(),
                    ActiveSlot::Error => self.error_slot.is_empty(),
                    ActiveSlot::None => false,
                };
                if !active_target_is_empty {
                    return Err(super::ShadowPaintBlocker::MissingPreparedSvg);
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
                        return Err(super::ShadowPaintBlocker::MissingPreparedSvg);
                    }
                    let node = arena
                        .get(key)
                        .ok_or(super::ShadowPaintBlocker::MissingPreparedSvg)?;
                    if node.parent() != Some(expected_parent)
                        || node.element.children() != node.children()
                    {
                        return Err(super::ShadowPaintBlocker::MissingPreparedSvg);
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
                        return Err(super::ShadowPaintBlocker::MissingPreparedSvg);
                    }
                }
                Ok(SvgShadowPaintClass::ActiveSlotWrapper(self.active_slot))
            }
            _ => Err(super::ShadowPaintBlocker::MissingPreparedSvg),
        }
    }

    fn has_canonical_culled_subtree_state(
        &self,
        arena: &crate::view::node_arena::NodeArena,
    ) -> bool {
        if self.element.layout_state.should_render {
            return false;
        }
        let Some(owner) = arena.find_by_stable_id(self.stable_id()) else {
            return false;
        };
        if !arena.contains_key(owner) || self.element.children() != arena.children_of(owner) {
            return false;
        }

        match (&self.frozen_document, self.active_slot) {
            (Some(SvgDocumentSnapshot::Ready { .. }), ActiveSlot::None) => {
                let Some(frozen) = self.frozen_paint.as_ref() else {
                    return false;
                };
                let current_asset_id = svg_raster_asset_id_for_request(
                    frozen.raster_key,
                    frozen.document_key,
                    frozen.plan.request,
                );
                if !self.element.children().is_empty()
                    || !self.frozen_request_is_exact
                    || frozen.document_key != self.source_key
                    || self.frozen_document_key != Some(self.source_key)
                    || self.active_raster_key != Some(frozen.raster_key)
                    || self.active_raster_request != Some(frozen.plan.request)
                    || self.frozen_desired_request != Some(frozen.plan.request)
                    || self.active_device_scale_bits != Some(frozen.device_scale_bits)
                    || self.pending_raster_key.is_some()
                    || self.pending_raster_request.is_some()
                    || self.pending_device_scale_bits.is_some()
                    || current_asset_id
                        .map(crate::view::sampled_texture::SampledTextureId::SvgRaster)
                        != Some(frozen.upload.id)
                    || frozen.upload.validate_rgba8().is_none()
                {
                    return false;
                }
            }
            (Some(document), ActiveSlot::Loading | ActiveSlot::Error) => {
                let resolved =
                    Self::resolve_frozen_slot(document, self.frozen_active_raster.as_ref());
                let active_target_is_empty = match self.active_slot {
                    ActiveSlot::Loading => self.loading_slot.is_empty(),
                    ActiveSlot::Error => self.error_slot.is_empty(),
                    ActiveSlot::None => false,
                };
                if !active_target_is_empty
                    || self.frozen_document_key != Some(self.source_key)
                    || resolved != self.active_slot
                    || self.active_raster_key != self.frozen_active_raster_key
                    || self.pending_raster_key != self.frozen_pending_raster_key
                    || self.frozen_active_raster_key.is_some()
                        != self.frozen_active_raster.is_some()
                    || self.frozen_pending_raster_key.is_some()
                        != self.frozen_pending_raster.is_some()
                    || self.frozen_paint.is_some()
                    || self.frozen_request_is_exact
                {
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

    #[cfg(all(test, not(target_arch = "wasm32")))]
    pub(crate) fn prepare_content_paint_for_test(
        &mut self,
        svg_text: &str,
        intrinsic_size: (f32, f32),
        device_scale: f32,
    ) -> Result<(), String> {
        let (inner_x, inner_y, inner_w, inner_h) = self.element.inner_content_rect_for_render();
        let plan = self
            .resolve_raster_plan(
                intrinsic_size.0,
                intrinsic_size.1,
                inner_w,
                inner_h,
                device_scale,
            )
            .ok_or_else(|| "invalid SVG fixture geometry".to_string())?;
        let pixels = crate::view::svg_resource::rasterize_svg_text_for_test(svg_text, plan.request)
            .map_err(|error| error.to_string())?;
        let (primed_key, _) = crate::view::svg_resource::prime_svg_raster_ready_for_test(
            self.source_key,
            plan.request,
            pixels,
        );
        let raster_key = acquire_svg_raster(self.source_key, plan.request);
        debug_assert_eq!(raster_key, primed_key);
        let ImageSnapshot::Ready(image) = snapshot_svg_raster(raster_key)
            .ok_or_else(|| "missing primed SVG fixture raster".to_string())?
        else {
            return Err("primed SVG fixture raster is not ready".to_string());
        };
        let upload = self
            .upload_for_image(&image)
            .ok_or_else(|| "failed to prepare SVG fixture upload".to_string())?;
        self.active_raster_key = Some(raster_key);
        self.active_raster_request = Some(plan.request);
        self.active_device_scale_bits = Some(device_scale.max(0.0001).to_bits());
        self.pending_raster_key = None;
        self.pending_raster_request = None;
        self.pending_device_scale_bits = None;
        self.frozen_document_key = Some(self.source_key);
        self.frozen_document = Some(SvgDocumentSnapshot::Ready {
            intrinsic_width: intrinsic_size.0,
            intrinsic_height: intrinsic_size.1,
        });
        self.frozen_active_raster = Some(ImageSnapshot::Ready(image));
        self.frozen_desired_request = Some(plan.request);
        self.frozen_paint = Some(FrozenSvgPaint {
            document_key: self.source_key,
            raster_key,
            device_scale_bits: device_scale.max(0.0001).to_bits(),
            plan,
            inner_origin: [inner_x, inner_y],
            upload,
            opacity: self
                .element
                .retained_paint_properties()
                .opacity
                .clamp(0.0, 1.0),
        });
        self.active_slot = ActiveSlot::None;
        self.loading_slot.clear();
        self.error_slot.clear();
        self.element.sync_children_mirror(&[]);
        self.frozen_request_is_exact = true;
        Ok(())
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
pub(crate) fn prepare_svg_fixture_for_test(
    svg_text: &str,
    fit: ImageFit,
    intrinsic_size: (f32, f32),
    destination: [f32; 4],
    device_scale: f32,
) -> Result<crate::view::paint::PreparedSvgOp, String> {
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEST_ASSET_ID: AtomicU64 = AtomicU64::new(10_000);
    let mut svg = Svg::new_with_id(
        NEXT_TEST_ASSET_ID.fetch_add(1, Ordering::Relaxed),
        SvgSource::Content(svg_text.to_string()),
    );
    svg.set_fit(fit);
    svg.set_sampling(ImageSampling::Nearest);
    let plan = svg
        .resolve_raster_plan(
            intrinsic_size.0,
            intrinsic_size.1,
            destination[2],
            destination[3],
            device_scale,
        )
        .ok_or_else(|| "invalid SVG fixture geometry".to_string())?;
    let pixels = crate::view::svg_resource::rasterize_svg_text_for_test(svg_text, plan.request)
        .map_err(|error| error.to_string())?;
    let image = crate::view::image_resource::ReadyImage {
        sampled_texture_id: crate::view::sampled_texture::SampledTextureId::SvgRaster(
            crate::view::sampled_texture::SvgRasterAssetId::for_test(
                NEXT_TEST_ASSET_ID.fetch_add(1, Ordering::Relaxed),
            ),
        ),
        width: plan.request.physical_width,
        height: plan.request.physical_height,
        pixels,
        generation: 1,
    };
    let upload = svg
        .upload_for_image(&image)
        .ok_or_else(|| "failed to prepare SVG fixture upload".to_string())?;
    svg.frozen_paint = Some(FrozenSvgPaint {
        document_key: svg.source_key,
        raster_key: 0,
        device_scale_bits: device_scale.max(0.0001).to_bits(),
        plan,
        inner_origin: [destination[0], destination[1]],
        upload,
        opacity: 1.0,
    });
    svg.prepared_svg_op([0.0, 0.0], 1.0)
        .ok_or_else(|| "failed to prepare SVG fixture".to_string())
}

impl ComputedStyleConsumer for Svg {
    type Snapshot = ElementStyleSnapshot;

    fn apply_computed_style(
        &mut self,
        computed: ComputedStyle,
        previous_snapshot: Option<&ElementStyleSnapshot>,
    ) {
        ComputedStyleConsumer::apply_computed_style(&mut self.element, computed, previous_snapshot);
    }
}

impl ElementTrait for Svg {
    fn stable_id(&self) -> u64 {
        self.element.stable_id()
    }

    fn retained_scroll_normalized_paint_capability(
        &self,
    ) -> Option<super::RetainedScrollNormalizedPaintCapability> {
        Some(super::RetainedScrollNormalizedPaintCapability::native(
            super::RetainedScrollNormalizedPaintKind::Svg,
        ))
    }

    fn exact_retained_self_clip_scissor_rect(
        &self,
        owner: crate::view::node_arena::NodeKey,
        arena: &crate::view::node_arena::NodeArena,
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
        owner: crate::view::node_arena::NodeKey,
        arena: &crate::view::node_arena::NodeArena,
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

    fn children(&self) -> &[crate::view::node_arena::NodeKey] {
        self.element.children()
    }

    fn sync_children_mirror(&mut self, children: &[crate::view::node_arena::NodeKey]) {
        self.element.sync_children_mirror(children);
    }

    #[allow(private_interfaces)]
    fn shadow_paint_recording_capability(
        &self,
        arena: &crate::view::node_arena::NodeArena,
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
                    super::ShadowPaintBlocker::MissingPreparedSvg,
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
        arena: &crate::view::node_arena::NodeArena,
        recording_context: crate::view::paint::PaintRecordingContext,
    ) -> Option<crate::view::paint::RetainedChildMaskPlan> {
        self.element
            .prepared_retained_child_mask_plan(arena, recording_context)
    }

    #[allow(private_interfaces)]
    fn record_shadow_paint_metadata(
        &self,
        owner: crate::view::node_arena::NodeKey,
        properties: crate::view::compositor::property_tree::PropertyTreeState,
        content_revision: crate::view::paint::PaintContentRevision,
        arena: &crate::view::node_arena::NodeArena,
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
            SvgShadowPaintClass::ReadyExact(prepared) => {
                let identity = crate::view::paint::PreparedSvgIdentity::from_op(&prepared)?;
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
                metadata.id.role = crate::view::paint::PaintChunkRole::SvgContent;
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
                    crate::view::paint::PaintPayloadIdentity::svg_with_shadows_and_decoration(
                        identity,
                        shadows.iter(),
                        decoration.iter(),
                    )?;
                Some(metadata)
            }
            SvgShadowPaintClass::ActiveSlotWrapper(_slot) => self
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
        owner: crate::view::node_arena::NodeKey,
        properties: crate::view::compositor::property_tree::PropertyTreeState,
        content_revision: crate::view::paint::PaintContentRevision,
        arena: &crate::view::node_arena::NodeArena,
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
            SvgShadowPaintClass::ReadyExact(prepared) => {
                let identity = crate::view::paint::PreparedSvgIdentity::from_op(&prepared)?;
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
                metadata.id.role = crate::view::paint::PaintChunkRole::SvgContent;
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
                    crate::view::paint::PaintPayloadIdentity::svg_with_shadows_and_decoration(
                        identity,
                        ops[..shadow_count].iter().filter_map(|op| match op {
                            crate::view::paint::PaintOp::PreparedShadow(shadow) => Some(shadow),
                            _ => None,
                        }),
                        ops[shadow_count..].iter().filter_map(|op| match op {
                            crate::view::paint::PaintOp::DrawRect(rect) => Some(rect),
                            _ => None,
                        }),
                    )?;
                ops.push(crate::view::paint::PaintOp::PreparedSvg(prepared));
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
            SvgShadowPaintClass::ActiveSlotWrapper(_slot) => self
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
        self.source_key.hash(&mut hasher);
        self.source_kind.hash(&mut hasher);
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
        match self.frozen_document.as_ref() {
            None => 0_u8.hash(&mut hasher),
            Some(SvgDocumentSnapshot::Loading) => 1_u8.hash(&mut hasher),
            Some(SvgDocumentSnapshot::Ready {
                intrinsic_width,
                intrinsic_height,
            }) => {
                2_u8.hash(&mut hasher);
                intrinsic_width.to_bits().hash(&mut hasher);
                intrinsic_height.to_bits().hash(&mut hasher);
            }
            Some(SvgDocumentSnapshot::Error(message)) => {
                3_u8.hash(&mut hasher);
                message.as_ref().hash(&mut hasher);
            }
        }
        hash_svg_raster_state(
            self.active_raster_key,
            self.active_raster_request
                .map(|request| (request.physical_width, request.physical_height)),
            self.frozen_active_raster.as_ref(),
            &mut hasher,
        );
        self.active_device_scale_bits.hash(&mut hasher);
        hash_svg_raster_state(
            self.pending_raster_key,
            self.pending_raster_request
                .map(|request| (request.physical_width, request.physical_height)),
            self.frozen_pending_raster.as_ref(),
            &mut hasher,
        );
        self.pending_raster_request.hash(&mut hasher);
        self.pending_device_scale_bits.hash(&mut hasher);
        self.failed_raster_request.hash(&mut hasher);
        self.frozen_desired_request.hash(&mut hasher);
        self.frozen_request_is_exact.hash(&mut hasher);
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
        let media = super::image::paint_adjusted_media_bounds(&self.element, paint_offset);
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
        let media = super::image::paint_adjusted_media_bounds(&self.element, paint_offset);
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
                _ => return Err(format!("unknown prop `{}` on <Svg>", key)),
            }
        }
        Ok(())
    }

    fn attach_side_slot(
        &mut self,
        name: &'static str,
        keys: Vec<crate::view::node_arena::NodeKey>,
    ) {
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
            return Err("<Svg> does not accept children; use loading/error props".to_string());
        }
        Ok(Vec::new())
    }

    fn apply_prop(
        &mut self,
        arena: &mut crate::view::node_arena::NodeArena,
        self_key: crate::view::node_arena::NodeKey,
        _ctx: &crate::view::fiber_work::ApplyContext<'_>,
        name: &'static str,
        value: crate::ui::PropValue,
    ) -> crate::view::fiber_work::PropApplyOutcome {
        use crate::ui::FromPropValue;
        use crate::view::fiber_work::PropApplyOutcome;
        use crate::view::node_arena::NodeKey;
        use crate::view::renderer_adapter::{
            StyleCascadeContext, as_element_style, commit_descriptor_tree, convert_image_slot_desc,
        };

        match name {
            "source" => {
                let Ok(source) = SvgSource::from_prop_value(value) else {
                    return PropApplyOutcome::DecodeFailed(name);
                };
                self.set_source(source);
                PropApplyOutcome::Applied
            }
            "style" => {
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
                    eprintln!("[Svg] rejected invalid {name} slot replacement: {error:?}");
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
        _arena: &mut crate::view::node_arena::NodeArena,
        _self_key: crate::view::node_arena::NodeKey,
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

impl EventTarget for Svg {
    crate::view::base_component::forward_event_target!(full element);
}

impl Layoutable for Svg {
    fn sync_arena(&mut self, arena: &mut crate::view::node_arena::NodeArena) {
        self.refresh_frozen_resources(arena);
        self.prepared_by_arena_sync = true;
    }

    fn requires_arena_sync(&self) -> bool {
        true
    }

    fn prepare_paint_resources(&mut self, context: super::PaintResourcePreparationContext) {
        self.prepare_frozen_paint(context);
    }

    fn measure(
        &mut self,
        constraints: LayoutConstraints,
        arena: &mut crate::view::node_arena::NodeArena,
    ) {
        if !self.prepared_by_arena_sync {
            self.refresh_frozen_resources(arena);
        }
        let snapshot = self
            .frozen_document
            .clone()
            .unwrap_or(SvgDocumentSnapshot::Loading);
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

impl Drop for Svg {
    fn drop(&mut self) {
        if let Some(raster_key) = self.active_raster_key.take() {
            release_svg_raster(raster_key);
        }
        if let Some(raster_key) = self.pending_raster_key.take() {
            release_svg_raster(raster_key);
        }
        release_svg_document(self.source_key);
    }
}

impl Renderable for Svg {
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
        let Some(parent_target) = ctx.current_target() else {
            return ctx.into_state();
        };
        let opacity = self
            .frozen_paint
            .as_ref()
            .map_or(0.0, |paint| paint.opacity);
        let Some(prepared) = self.prepared_svg_op(
            super::image::paint_adjusted_offset(&self.element, parent_paint_offset),
            opacity,
        ) else {
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

#[cfg(test)]
mod tests;
