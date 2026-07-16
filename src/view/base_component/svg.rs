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
            opacity: self.element.promotion_node_info().opacity.clamp(0.0, 1.0),
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
        _deferred_phase_root: bool,
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
                if let Some(blocker) =
                    self.element
                        .shadow_paint_blocker(arena, false, false, false, recording_context)
                {
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
                    // M9E2 authorizes only the wrapper's canonical zero-blur
                    // outer-shadow payload. Deferred emission remains out.
                    false,
                    false,
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
                    if properties.clip.is_some() {
                        return Err(super::ShadowPaintBlocker::SelfClip);
                    }
                    if properties.scroll.is_some() {
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
            opacity: self.element.promotion_node_info().opacity.clamp(0.0, 1.0),
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

    fn box_model_snapshot(&self) -> BoxModelSnapshot {
        self.element.box_model_snapshot()
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
        match self.classify_shadow_paint(arena, None, None, deferred_phase_root, recording_context)
        {
            Ok(_) => super::ShadowPaintRecordingCapability::Recordable,
            Err(blocker) => super::ShadowPaintRecordingCapability::Legacy(blocker),
        }
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
                false,
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
                metadata.payload_identity =
                    crate::view::paint::PaintPayloadIdentity::svg_with_decoration(
                        identity,
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
                false,
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
                    .self_decoration_paint_ops(
                        prepared.params.opacity,
                        recording_context.paint_offset,
                    )
                    .into_iter()
                    .map(crate::view::paint::PaintOp::DrawRect)
                    .collect::<Vec<_>>();
                metadata.payload_identity =
                    crate::view::paint::PaintPayloadIdentity::svg_with_decoration(
                        identity,
                        ops.iter().filter_map(|op| match op {
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

    fn promotion_node_info(&self) -> crate::view::promotion::PromotionNodeInfo {
        self.element.promotion_node_info()
    }

    fn has_active_animator(&self) -> bool {
        self.element.has_active_animator()
    }

    fn promotion_self_signature(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.element.promotion_self_signature().hash(&mut hasher);
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

    fn promotion_signature_is_complete(&self) -> bool {
        true
    }

    fn promotion_clip_intersection_signature(
        &self,
        arena: &crate::view::node_arena::NodeArena,
    ) -> u64 {
        self.element.promotion_clip_intersection_signature(arena)
    }

    fn promotion_composite_bounds(&self) -> super::PromotionCompositeBounds {
        self.element.promotion_composite_bounds()
    }

    fn retained_transform_surface_bounds(
        &self,
        arena: &crate::view::node_arena::NodeArena,
        paint_offset: [f32; 2],
    ) -> Option<super::PromotionCompositeBounds> {
        self.element
            .retained_transform_surface_bounds(arena, paint_offset)
    }

    fn retained_transform_output_bounds(
        &self,
        arena: &crate::view::node_arena::NodeArena,
        paint_offset: [f32; 2],
    ) -> Option<super::PromotionCompositeBounds> {
        let wrapper = self
            .element
            .retained_transform_render_output_bounds(arena, paint_offset)?;
        let media = super::image::paint_adjusted_media_bounds(&self.element, paint_offset);
        Element::checked_union_transform_surface_bounds(wrapper, media)
    }

    fn legacy_transform_output_bounds(
        &self,
        arena: &crate::view::node_arena::NodeArena,
        paint_offset: [f32; 2],
    ) -> Option<super::PromotionCompositeBounds> {
        let wrapper = self
            .element
            .legacy_transform_render_output_bounds(arena, paint_offset)?;
        let media = super::image::paint_adjusted_media_bounds(&self.element, paint_offset);
        Element::checked_union_transform_surface_bounds(wrapper, media)
    }

    fn retained_transform_raster_seed_bounds(&self) -> Option<super::PromotionCompositeBounds> {
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
        let opacity = if ctx.is_node_promoted(self.stable_id()) {
            1.0
        } else {
            self.frozen_paint
                .as_ref()
                .map_or(0.0, |paint| paint.opacity)
        };
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
mod tests {
    use super::{ActiveSlot, FrozenSvgPaint, Svg};
    use crate::style::{
        BoxShadow, Color, ComputedStyle, EdgeInsets, Layout, ParsedValue, PropertyId,
        ScrollDirection, Style,
    };
    use crate::time::{Duration, Instant};
    use crate::view::SvgSource;
    use crate::view::base_component::{
        ComputedStyleConsumer, DirtyFlags, Element, ElementTrait, EventTarget, LayoutConstraints,
        LayoutPlacement, Layoutable, PaintResourcePreparationContext, ShadowPaintBlocker,
        ShadowPaintRecordingCapability, Size,
    };
    use crate::view::compositor::{PaintGenerationTracker, PropertyTrees};
    use crate::view::image_resource::{ImageSnapshot, ReadyImage};
    use crate::view::node_arena::{Node, NodeArena, NodeKey};
    use crate::view::sampled_texture::{SampledTextureId, SvgRasterAssetId};
    use crate::view::svg_resource::{
        SvgDocumentSnapshot, SvgRasterMode, SvgRasterRequest, acquire_svg_raster,
        prime_svg_document_ready_for_test, prime_svg_raster_ready_for_test,
        remove_svg_document_entry_for_test, remove_svg_raster_entry_for_test,
        replace_svg_raster_ready_for_test, set_svg_document_error_for_test,
        set_svg_document_loading_for_test, set_svg_raster_error_for_test,
        set_svg_raster_loading_for_test, set_svg_raster_ready_for_test, snapshot_svg_document,
        snapshot_svg_raster, svg_raster_ref_count_for_test,
    };
    use crate::view::test_support::{
        commit_child, commit_element, measure_and_place, new_test_arena,
    };
    use glam::{Mat4, Vec3};
    use rustc_hash::FxHashSet;

    fn simple_svg() -> SvgSource {
        SvgSource::Content(
            r##"<svg width="80" height="40" viewBox="0 0 80 40" xmlns="http://www.w3.org/2000/svg"><rect width="80" height="40" fill="#ff0000"/></svg>"##.to_string(),
        )
    }

    #[test]
    fn svg_wrapper_forwards_scrollbar_post_layout_lifecycle() {
        let mut svg = Svg::new_with_id(0xa0f0, simple_svg());
        let mut style = Style::new();
        style.insert(
            PropertyId::ScrollDirection,
            ParsedValue::ScrollDirection(ScrollDirection::Vertical),
        );
        svg.element.apply_style(style);
        svg.element.layout_state.content_size = Size {
            width: 120.0,
            height: 300.0,
        };

        let now = crate::time::Instant::now();
        assert!(svg.set_hovered(true));
        assert!(svg.wants_animation_frame());
        assert!(
            svg.tick_post_layout_animation_frame(now)
                .contains(DirtyFlags::PAINT)
        );
        assert!(!svg.wants_animation_frame());

        assert!(svg.set_hovered(false));
        assert!(svg.wants_animation_frame());
        assert!(
            svg.tick_post_layout_animation_frame(now)
                .contains(DirtyFlags::PAINT)
        );
        assert!(svg.wants_animation_frame());
        assert!(
            svg.tick_post_layout_animation_frame(now + crate::time::Duration::from_millis(1_250),)
                .contains(DirtyFlags::PAINT)
        );
        assert!(!svg.wants_animation_frame());
    }

    #[test]
    fn transformed_svg_wrapper_and_untransformed_media_expand_parent_exact_bounds() {
        let mut parent = Element::new_with_id(0xA200, 0.0, 0.0, 10.0, 10.0);
        parent.set_resolved_transform_for_test(Some(Mat4::from_translation(Vec3::new(
            100.0, 0.0, 0.0,
        ))));
        let mut svg = Svg::new_with_id(0xA201, simple_svg());
        svg.element = Element::new_with_id(0xA201, 100.0, 2.0, 4.0, 2.0);
        svg.element
            .set_resolved_transform_for_test(Some(Mat4::from_translation(Vec3::new(
                -100.0, 0.0, 0.0,
            ))));

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let _svg_key = commit_child(&mut arena, parent_key, Box::new(svg));
        let geometry = crate::view::test_support::get_element::<Element>(&arena, parent_key)
            .exact_transform_surface_geometry_snapshot(&arena, [0.0, 0.0], None)
            .expect("Svg explicitly supplies exact wrapper plus media coverage");
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
            ]
        );
    }

    fn insert_inactive_slot_subtree(
        arena: &mut NodeArena,
        owner: NodeKey,
        id: u64,
    ) -> (NodeKey, NodeKey) {
        let root = arena.insert(Node::with_parent(
            Box::new(crate::view::base_component::Element::new_with_id(
                id, 0.0, 0.0, 1.0, 1.0,
            )),
            Some(owner),
        ));
        let child = arena.insert(Node::with_parent(
            Box::new(crate::view::base_component::Element::new_with_id(
                id + 1,
                0.0,
                0.0,
                1.0,
                1.0,
            )),
            Some(root),
        ));
        arena.set_children(root, vec![child]);
        (root, child)
    }

    fn active_slot_svg_fixture(
        id: u64,
        state: ActiveSlot,
    ) -> (NodeArena, NodeKey, NodeKey, NodeKey, NodeKey, NodeKey) {
        let mut svg = Svg::new_with_id(id, unique_svg(&format!("active-slot-{id}")));
        let mut style = Style::new();
        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        style.set_box_shadow(vec![
            BoxShadow::new()
                .color(Color::rgb(220, 30, 20))
                .offset_x(1.5)
                .offset_y(-2.25),
        ]);
        svg.apply_style(style);
        match state {
            ActiveSlot::Loading => set_svg_document_loading_for_test(svg.source_key),
            ActiveSlot::Error => set_svg_document_error_for_test(svg.source_key),
            ActiveSlot::None => unreachable!(),
        }

        let mut arena = new_test_arena();
        let owner = commit_element(&mut arena, Box::new(svg));
        let (loading_root, loading_child) = insert_inactive_slot_subtree(&mut arena, owner, id + 1);
        let (error_root, error_child) = insert_inactive_slot_subtree(&mut arena, owner, id + 0x10);
        arena.with_element_taken(owner, |element, _arena| {
            let svg = element.as_any_mut().downcast_mut::<Svg>().unwrap();
            svg.attach_loading_slot_cold(vec![loading_root]);
            svg.attach_error_slot_cold(vec![error_root]);
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
        let (active_root, active_child, inactive_root, inactive_child) = match state {
            ActiveSlot::Loading => (loading_root, loading_child, error_root, error_child),
            ActiveSlot::Error => (error_root, error_child, loading_root, loading_child),
            ActiveSlot::None => unreachable!(),
        };
        (
            arena,
            owner,
            active_root,
            active_child,
            inactive_root,
            inactive_child,
        )
    }

    #[test]
    fn svg_replaces_inactive_loading_and_active_error_slots_atomically() {
        let mut arena = new_test_arena();
        let owner = commit_element(&mut arena, Box::new(Svg::new_with_id(0x9200, simple_svg())));
        let (old_loading, old_loading_child) =
            insert_inactive_slot_subtree(&mut arena, owner, 0x9210);
        let (old_error, old_error_child) = insert_inactive_slot_subtree(&mut arena, owner, 0x9220);
        let (new_loading, _) = insert_inactive_slot_subtree(&mut arena, owner, 0x9230);
        let (new_error, _) = insert_inactive_slot_subtree(&mut arena, owner, 0x9240);

        arena.with_element_taken(owner, |element, arena| {
            let svg = element.as_any_mut().downcast_mut::<Svg>().unwrap();
            svg.attach_loading_slot_cold(vec![old_loading]);
            svg.attach_error_slot_cold(vec![old_error]);
            svg.sync_active_slot(arena, ActiveSlot::Error);
            assert_eq!(svg.element.children(), &[old_error]);
            assert_eq!(arena.children_of(owner), vec![old_error]);

            svg.replace_loading_slot_incremental(arena, owner, &[new_loading])
                .unwrap();
            assert_eq!(svg.active_slot, ActiveSlot::None);
            assert_eq!(svg.loading_slot, vec![new_loading]);
            assert_eq!(svg.error_slot, vec![old_error]);
            assert!(svg.element.children().is_empty());
            assert!(arena.children_of(owner).is_empty());

            svg.sync_active_slot(arena, ActiveSlot::Error);
            svg.replace_error_slot_incremental(arena, owner, &[new_error])
                .unwrap();
            assert_eq!(svg.active_slot, ActiveSlot::None);
            assert_eq!(svg.loading_slot, vec![new_loading]);
            assert_eq!(svg.error_slot, vec![new_error]);
            assert_eq!(arena.parent_of(new_loading), Some(owner));
            assert_eq!(arena.parent_of(new_error), Some(owner));
            assert_eq!(arena.children_of(owner), svg.element.children());
        });

        assert!(!arena.contains_key(old_loading));
        assert!(!arena.contains_key(old_loading_child));
        assert!(!arena.contains_key(old_error));
        assert!(!arena.contains_key(old_error_child));
        assert!(arena.contains_key(new_loading));
        assert!(arena.contains_key(new_error));
    }

    #[test]
    fn loading_and_error_wrappers_record_active_subtree_in_canonical_order() {
        use crate::view::paint::{
            CoverageRecordingMode, PaintCoverageItem, PaintNodePhase, PaintPropertyScope,
            record_coverage_manifest,
        };

        for (index, state) in [ActiveSlot::Loading, ActiveSlot::Error]
            .into_iter()
            .enumerate()
        {
            let (arena, owner, active_root, active_child, inactive_root, inactive_child) =
                active_slot_svg_fixture(0x9250 + index as u64 * 0x20, state);
            let node = arena.get(owner).unwrap();
            assert_eq!(node.children(), &[active_root]);
            assert_eq!(node.element.children(), &[active_root]);
            let context = node
                .element
                .shadow_paint_recording_context(Default::default());
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
                .unwrap();
            let artifact = node
                .element
                .record_shadow_paint_artifact(owner, Default::default(), revision, &arena, context)
                .unwrap();
            assert_eq!(metadata.id.scope, PaintPropertyScope::SelfPaint);
            assert_eq!(metadata.id.phase, PaintNodePhase::BeforeChildren);
            assert_eq!(metadata.id.slot, 0);
            assert_eq!(
                metadata.id.role,
                crate::view::paint::PaintChunkRole::SelfDecoration
            );
            assert_eq!(
                artifact.chunks[0].payload_identity,
                metadata.payload_identity
            );
            assert!(matches!(
                &metadata.payload_identity,
                crate::view::paint::PaintPayloadIdentity::PreparedShadows(shadows, _)
                    if shadows.len() == 1
            ));
            assert!(matches!(
                artifact.ops.first(),
                Some(crate::view::paint::PaintOp::PreparedShadow(_))
            ));
            assert!(
                artifact
                    .ops
                    .iter()
                    .all(|op| { !matches!(op, crate::view::paint::PaintOp::PreparedSvg(_)) })
            );
            drop(node);

            let roots = [owner];
            let mut properties = PropertyTrees::default();
            properties.sync(&arena, &roots);
            let mut generations = PaintGenerationTracker::default();
            generations.sync(&arena, &roots, &properties);
            let record = |mode| {
                record_coverage_manifest(
                    &arena,
                    &roots,
                    &FxHashSet::default(),
                    None,
                    false,
                    true,
                    mode,
                    &properties,
                    &generations,
                )
            };
            let metadata_manifest = record(CoverageRecordingMode::MetadataOnly);
            let full_manifest = record(CoverageRecordingMode::FullArtifact);
            assert!(metadata_manifest.validation_errors.is_empty());
            assert!(full_manifest.validation_errors.is_empty());
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
            let metadata_summary = summarize(&metadata_manifest);
            assert_eq!(metadata_summary, summarize(&full_manifest));
            assert_eq!(
                metadata_summary
                    .iter()
                    .map(|(owner, ..)| *owner)
                    .collect::<Vec<_>>(),
                vec![owner, active_root, active_child]
            );
            assert!(metadata_summary.iter().all(|(recorded, ..)| {
                *recorded != inactive_root && *recorded != inactive_child
            }));
        }
    }

    #[test]
    fn active_wrapper_topology_alias_and_resource_key_drift_fail_closed() {
        #[derive(Clone, Copy)]
        enum Drift {
            Alias,
            Parent,
            Mirror,
            ActiveRasterKey,
            PendingRasterKey,
        }

        for (index, drift) in [
            Drift::Alias,
            Drift::Parent,
            Drift::Mirror,
            Drift::ActiveRasterKey,
            Drift::PendingRasterKey,
        ]
        .into_iter()
        .enumerate()
        {
            let (mut arena, owner, active_root, active_child, inactive_root, _) =
                active_slot_svg_fixture(0x92a0 + index as u64 * 0x20, ActiveSlot::Loading);
            match drift {
                Drift::Alias => {
                    arena.with_element_taken(owner, |element, _arena| {
                        element
                            .as_any_mut()
                            .downcast_mut::<Svg>()
                            .unwrap()
                            .error_slot = vec![active_child];
                    });
                }
                Drift::Parent => arena.set_parent(inactive_root, Some(active_root)),
                Drift::Mirror => {
                    arena.with_element_taken(active_root, |element, _arena| {
                        element.sync_children_mirror(&[]);
                    });
                }
                Drift::ActiveRasterKey => {
                    arena.with_element_taken(owner, |element, _arena| {
                        element
                            .as_any_mut()
                            .downcast_mut::<Svg>()
                            .unwrap()
                            .active_raster_key = Some(0xdead_1000 + index as u64);
                    });
                }
                Drift::PendingRasterKey => {
                    arena.with_element_taken(owner, |element, _arena| {
                        element
                            .as_any_mut()
                            .downcast_mut::<Svg>()
                            .unwrap()
                            .pending_raster_key = Some(0xdead_2000 + index as u64);
                    });
                }
            }
            let node = arena.get(owner).unwrap();
            let context = node
                .element
                .shadow_paint_recording_context(Default::default());
            assert_eq!(
                node.element
                    .shadow_paint_recording_capability(&arena, false, context),
                ShadowPaintRecordingCapability::Legacy(ShadowPaintBlocker::MissingPreparedSvg)
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
    fn active_wrapper_rejects_property_boundaries_except_matching_root_opacity() {
        use crate::view::compositor::property_tree::{
            ClipNodeId, ClipNodeRole, EffectNodeId, PropertyTreeState, ScrollNodeId,
            TransformNodeId,
        };

        let (arena, owner, active_root, ..) = active_slot_svg_fixture(0x9360, ActiveSlot::Error);
        let revision = crate::view::paint::PaintContentRevision {
            self_paint_revision: 1,
            composite_revision: 1,
            topology_revision: 1,
        };
        let node = arena.get(owner).unwrap();
        let context = node
            .element
            .shadow_paint_recording_context(Default::default());
        for properties in [
            PropertyTreeState {
                transform: Some(TransformNodeId(owner)),
                ..Default::default()
            },
            PropertyTreeState {
                scroll: Some(ScrollNodeId(owner)),
                ..Default::default()
            },
            PropertyTreeState {
                clip: Some(ClipNodeId {
                    owner,
                    role: ClipNodeRole::ContentsClip,
                }),
                ..Default::default()
            },
            PropertyTreeState {
                effect: Some(EffectNodeId(owner)),
                ..Default::default()
            },
            PropertyTreeState {
                effect: Some(EffectNodeId(active_root)),
                ..Default::default()
            },
        ] {
            assert!(
                node.element
                    .record_shadow_paint_metadata(owner, properties, revision, &arena, context,)
                    .is_none()
            );
            assert!(
                node.element
                    .record_shadow_paint_artifact(owner, properties, revision, &arena, context,)
                    .is_none()
            );
        }

        let effect = EffectNodeId(owner);
        let properties = PropertyTreeState {
            effect: Some(effect),
            ..Default::default()
        };
        let root_opacity_context = crate::view::paint::PaintRecordingContext {
            opacity_authority: crate::view::paint::PaintOpacityAuthority::NeutralRootEffect(effect),
            ..context
        };
        let metadata = node
            .element
            .record_shadow_paint_metadata(owner, properties, revision, &arena, root_opacity_context)
            .expect("matching root-opacity authority");
        let artifact = node
            .element
            .record_shadow_paint_artifact(owner, properties, revision, &arena, root_opacity_context)
            .expect("matching root-opacity artifact");
        assert_eq!(
            metadata.id.role,
            crate::view::paint::PaintChunkRole::SelfDecoration
        );
        assert!(matches!(
            artifact.ops.as_slice(),
            [crate::view::paint::PaintOp::PreparedShadow(shadow), ..]
                if shadow.params.opacity.to_bits() == 1.0_f32.to_bits()
        ));
        assert!(
            artifact
                .ops
                .iter()
                .all(|op| { !matches!(op, crate::view::paint::PaintOp::PreparedSvg(_)) })
        );
    }

    #[test]
    fn ready_svg_with_two_inactive_subtrees_records_only_svg_content() {
        use crate::view::paint::{
            CoverageRecordingMode, PaintCoverageItem, PaintNodePhase, PaintPropertyScope,
            record_coverage_manifest,
        };

        let (arena, owner, loading_root, loading_child, error_root, error_child) =
            prepared_ready_svg_with_inactive_slots(0x93a0);
        let node = arena.get(owner).unwrap();
        assert!(node.children().is_empty());
        assert!(node.element.children().is_empty());
        drop(node);
        assert_eq!(arena.parent_of(loading_root), Some(owner));
        assert_eq!(arena.parent_of(error_root), Some(owner));

        let roots = [owner];
        let mut properties = PropertyTrees::default();
        properties.sync(&arena, &roots);
        let mut generations = PaintGenerationTracker::default();
        generations.sync(&arena, &roots, &properties);
        let metadata = record_coverage_manifest(
            &arena,
            &roots,
            &FxHashSet::default(),
            None,
            false,
            true,
            CoverageRecordingMode::MetadataOnly,
            &properties,
            &generations,
        );
        let full = record_coverage_manifest(
            &arena,
            &roots,
            &FxHashSet::default(),
            None,
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
            panic!("Ready SVG metadata must contain only SvgContent")
        };
        let [
            PaintCoverageItem::ArtifactChunk {
                chunk: full_chunk,
                ops: Some(full_ops),
                ..
            },
        ] = full.items.as_slice()
        else {
            panic!("Ready SVG full recording must contain only SvgContent")
        };
        assert_eq!(metadata_chunk.id, full_chunk.id);
        assert_eq!(metadata_chunk.payload_identity, full_chunk.payload_identity);
        assert_eq!(metadata_chunk.owner, owner);
        assert_eq!(metadata_chunk.id.scope, PaintPropertyScope::SelfPaint);
        assert_eq!(metadata_chunk.id.phase, PaintNodePhase::BeforeChildren);
        assert_eq!(metadata_chunk.id.slot, 0);
        assert_eq!(
            metadata_chunk.id.role,
            crate::view::paint::PaintChunkRole::SvgContent
        );
        assert!(
            full_ops
                .iter()
                .any(|op| matches!(op, crate::view::paint::PaintOp::PreparedSvg(_)))
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
    fn ready_svg_media_with_outer_shadow_remains_legacy() {
        let mut svg = freeze_ready_svg(0x93b0, unique_svg("ready-shadow-fallback"), 1.0);
        let mut style = Style::new();
        style.set_box_shadow(vec![BoxShadow::new().offset_x(1.0)]);
        svg.apply_style(style);
        let mut arena = new_test_arena();
        let owner = commit_element(&mut arena, Box::new(svg));
        let node = arena.get(owner).unwrap();
        let context = node
            .element
            .shadow_paint_recording_context(Default::default());
        assert_eq!(
            node.element
                .shadow_paint_recording_capability(&arena, false, context),
            ShadowPaintRecordingCapability::Legacy(ShadowPaintBlocker::BoxShadow)
        );
        let revision = crate::view::paint::PaintContentRevision {
            self_paint_revision: 1,
            composite_revision: 1,
            topology_revision: 1,
        };
        assert!(
            node.element
                .record_shadow_paint_metadata(owner, Default::default(), revision, &arena, context,)
                .is_none()
        );
        assert!(
            node.element
                .record_shadow_paint_artifact(owner, Default::default(), revision, &arena, context,)
                .is_none()
        );
    }

    #[test]
    fn svg_wrapper_outer_shadow_root_opacity_is_applied_once() {
        let (arena, owner, ..) = active_slot_svg_fixture(0x93b5, ActiveSlot::Loading);
        {
            let mut node = arena.get_mut(owner).unwrap();
            let svg = node.element.as_any_mut().downcast_mut::<Svg>().unwrap();
            let mut style = Style::new();
            style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
            style.insert(
                PropertyId::BackgroundColor,
                ParsedValue::color_like(Color::rgb(20, 180, 40)),
            );
            style.insert(
                PropertyId::Opacity,
                ParsedValue::Opacity(crate::style::Opacity::new(0.4)),
            );
            style.set_box_shadow(vec![BoxShadow::new().offset_x(1.5).offset_y(-2.25)]);
            svg.apply_style(style);
        }
        let mut properties = PropertyTrees::default();
        properties.sync(&arena, &[owner]);
        let mut generations = PaintGenerationTracker::default();
        generations.sync(&arena, &[owner], &properties);
        let outcome = crate::view::paint::record_root_group_opacity_frame_artifact(
            &arena,
            &[owner],
            &FxHashSet::default(),
            &properties,
            &generations,
            crate::view::paint::RendererMode::ForcedForTests,
        )
        .unwrap();
        let crate::view::paint::FrameArtifactRecordOutcome::Artifact { artifact, .. } = outcome
        else {
            panic!("SVG wrapper root-opacity must record")
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
                ..
            ] if shadow.params.opacity.to_bits() == 1.0_f32.to_bits()
                && fill.params.opacity.to_bits() == 1.0_f32.to_bits()
        ));
    }

    #[test]
    fn ready_svg_rejects_invalid_inactive_roots_and_children_mirror_drift() {
        enum Drift {
            Missing,
            Duplicate,
            WrongParent,
            ChildrenMirror,
        }

        for (index, drift) in [
            Drift::Missing,
            Drift::Duplicate,
            Drift::WrongParent,
            Drift::ChildrenMirror,
        ]
        .into_iter()
        .enumerate()
        {
            let (mut arena, owner, loading_root, _, _, _) =
                prepared_ready_svg_with_inactive_slots(0x93d0 + index as u64 * 0x10);
            match drift {
                Drift::Missing => {
                    arena.remove_subtree(loading_root);
                }
                Drift::Duplicate => {
                    arena.with_element_taken(owner, |element, _arena| {
                        element
                            .as_any_mut()
                            .downcast_mut::<Svg>()
                            .unwrap()
                            .error_slot = vec![loading_root];
                    });
                }
                Drift::WrongParent => arena.set_parent(loading_root, None),
                Drift::ChildrenMirror => arena.set_children(owner, vec![loading_root]),
            }
            assert_missing_prepared_svg_hooks(&arena, owner);
        }
    }

    #[test]
    fn ready_svg_inactive_slots_do_not_bypass_source_raster_or_request_drift() {
        enum Drift {
            Source,
            Raster,
            Request,
        }

        for (index, drift) in [Drift::Source, Drift::Raster, Drift::Request]
            .into_iter()
            .enumerate()
        {
            let (arena, owner, ..) =
                prepared_ready_svg_with_inactive_slots(0x9420 + index as u64 * 0x10);
            let mut restore_key = None;
            {
                let mut node = arena.get_mut(owner).unwrap();
                let svg = node.element.as_any_mut().downcast_mut::<Svg>().unwrap();
                match drift {
                    Drift::Source => {
                        restore_key = Some((true, svg.source_key));
                        svg.source_key = svg.source_key.wrapping_add(0x1000);
                    }
                    Drift::Raster => {
                        restore_key = Some((false, svg.active_raster_key.unwrap()));
                        svg.active_raster_key = svg.active_raster_key.map(|key| key + 0x1000);
                    }
                    Drift::Request => {
                        let request = svg.active_raster_request.unwrap();
                        svg.active_raster_request = Some(SvgRasterRequest::new(
                            request.physical_width.saturating_add(8),
                            request.physical_height,
                            request.mode,
                        ));
                    }
                }
            }
            assert_missing_prepared_svg_hooks(&arena, owner);
            if let Some((source, key)) = restore_key {
                let mut node = arena.get_mut(owner).unwrap();
                let svg = node.element.as_any_mut().downcast_mut::<Svg>().unwrap();
                if source {
                    svg.source_key = key;
                } else {
                    svg.active_raster_key = Some(key);
                }
            }
        }
    }

    fn unique_svg(marker: &str) -> SvgSource {
        SvgSource::Content(format!(
            r##"<svg width="80" height="40" viewBox="0 0 80 40" xmlns="http://www.w3.org/2000/svg"><rect width="80" height="40" fill="#ff0000"/><desc>{marker}</desc></svg>"##
        ))
    }

    fn wait_until_document_ready(key: u64) {
        for _ in 0..500 {
            if matches!(
                snapshot_svg_document(key),
                Some(SvgDocumentSnapshot::Ready { .. })
            ) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        panic!("SVG document did not become ready");
    }

    fn wait_until_raster_ready(key: u64) {
        for _ in 0..500 {
            if matches!(snapshot_svg_raster(key), Some(ImageSnapshot::Ready(_))) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        panic!("SVG raster did not become ready");
    }

    fn layout_svg_element(svg: &mut Svg, width: f32, height: f32) {
        let mut style = Style::new();
        style.insert(
            crate::style::PropertyId::Width,
            crate::style::ParsedValue::Length(crate::style::Length::px(width)),
        );
        style.insert(
            crate::style::PropertyId::Height,
            crate::style::ParsedValue::Length(crate::style::Length::px(height)),
        );
        svg.apply_style(style);
        let mut arena = new_test_arena();
        svg.element.measure(
            LayoutConstraints {
                max_width: width,
                max_height: height,
                viewport_width: width,
                viewport_height: height,
                percent_base_width: Some(width),
                percent_base_height: Some(height),
            },
            &mut arena,
        );
        svg.element.place(
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: width,
                available_height: height,
                viewport_width: width,
                viewport_height: height,
                percent_base_width: Some(width),
                percent_base_height: Some(height),
            },
            &mut arena,
        );
    }

    fn freeze_ready_svg(id: u64, source: SvgSource, scale: f32) -> Svg {
        let source = match source {
            SvgSource::Content(content) => {
                SvgSource::Content(format!("{content}<!-- m9b2-fixture-{id} -->"))
            }
            SvgSource::Path(path) => SvgSource::Path(std::path::PathBuf::from(format!(
                "{}-m9b2-fixture-{id}",
                path.display()
            ))),
        };
        let primed_document = prime_svg_document_ready_for_test(&source, 80.0, 40.0);
        let mut svg = Svg::new_with_id(id, source);
        assert_eq!(svg.source_key, primed_document);
        layout_svg_element(&mut svg, 80.0, 40.0);
        let mut arena = new_test_arena();
        svg.sync_arena(&mut arena);
        let request = svg
            .resolve_raster_plan(80.0, 40.0, 80.0, 40.0, scale)
            .unwrap()
            .request;
        let pixels: std::sync::Arc<[u8]> = std::sync::Arc::from(vec![
            id as u8;
            (request.physical_width * request.physical_height * 4)
                as usize
        ]);
        let (primed_raster, _) = prime_svg_raster_ready_for_test(svg.source_key, request, pixels);
        svg.prepare_frozen_paint(PaintResourcePreparationContext {
            frame_number: 1,
            device_scale: scale,
            now: Instant::now(),
        });
        assert_eq!(svg.active_raster_key, Some(primed_raster));
        svg.sync_arena(&mut arena);
        svg.prepare_frozen_paint(PaintResourcePreparationContext {
            frame_number: 2,
            device_scale: scale,
            now: Instant::now(),
        });
        assert!(svg.frozen_request_is_exact);
        svg
    }

    fn prepared_ready_svg_with_inactive_slots(
        id: u64,
    ) -> (NodeArena, NodeKey, NodeKey, NodeKey, NodeKey, NodeKey) {
        let svg = freeze_ready_svg(id, unique_svg(&format!("ready-inactive-{id}")), 1.0);
        let mut arena = new_test_arena();
        let owner = commit_element(&mut arena, Box::new(svg));
        let (loading_root, loading_child) = insert_inactive_slot_subtree(&mut arena, owner, id + 1);
        let (error_root, error_child) = insert_inactive_slot_subtree(&mut arena, owner, id + 3);
        arena.with_element_taken(owner, |element, _arena| {
            let svg = element.as_any_mut().downcast_mut::<Svg>().unwrap();
            svg.attach_loading_slot_cold(vec![loading_root]);
            svg.attach_error_slot_cold(vec![error_root]);
        });
        (
            arena,
            owner,
            loading_root,
            loading_child,
            error_root,
            error_child,
        )
    }

    fn assert_missing_prepared_svg_hooks(arena: &NodeArena, owner: NodeKey) {
        let node = arena.get(owner).unwrap();
        let context = node
            .element
            .shadow_paint_recording_context(Default::default());
        assert!(matches!(
            node.element
                .shadow_paint_recording_capability(arena, false, context),
            ShadowPaintRecordingCapability::Legacy(
                ShadowPaintBlocker::MissingPreparedSvg
                    | ShadowPaintBlocker::MissingPreparedInlineRoot
            )
        ));
        let revision = crate::view::paint::PaintContentRevision {
            self_paint_revision: 1,
            composite_revision: 1,
            topology_revision: 1,
        };
        assert!(
            node.element
                .record_shadow_paint_metadata(owner, Default::default(), revision, arena, context,)
                .is_none()
        );
        assert!(
            node.element
                .record_shadow_paint_artifact(owner, Default::default(), revision, arena, context,)
                .is_none()
        );
    }

    #[test]
    fn promotion_signature_covers_source_fit_sampling_and_raster_generation() {
        use std::hash::Hasher;

        let mut svg = Svg::new_with_id(1, simple_svg());
        assert!(svg.promotion_signature_is_complete());
        let initial = svg.promotion_self_signature();

        svg.set_fit(crate::view::ImageFit::Cover);
        let fit = svg.promotion_self_signature();
        assert_ne!(fit, initial);

        svg.set_sampling(crate::view::ImageSampling::Nearest);
        let sampling = svg.promotion_self_signature();
        assert_ne!(sampling, fit);

        svg.set_source(SvgSource::Content(
            r##"<svg width="40" height="20" xmlns="http://www.w3.org/2000/svg"><rect width="40" height="20" fill="#00ff00"/></svg>"##
                .to_string(),
        ));
        assert_ne!(svg.promotion_self_signature(), sampling);

        let pixels = std::sync::Arc::<[u8]>::from(vec![255; 16]);
        let first = ImageSnapshot::Ready(ReadyImage {
            sampled_texture_id: SampledTextureId::SvgRaster(SvgRasterAssetId::for_test(1)),
            width: 2,
            height: 2,
            pixels: pixels.clone(),
            generation: 20,
        });
        let second = ImageSnapshot::Ready(ReadyImage {
            sampled_texture_id: SampledTextureId::SvgRaster(SvgRasterAssetId::for_test(1)),
            width: 2,
            height: 2,
            pixels,
            generation: 21,
        });
        let mut first_hasher = std::collections::hash_map::DefaultHasher::new();
        super::hash_svg_raster_state(Some(7), Some((2, 2)), Some(&first), &mut first_hasher);
        let mut second_hasher = std::collections::hash_map::DefaultHasher::new();
        super::hash_svg_raster_state(Some(7), Some((2, 2)), Some(&second), &mut second_hasher);
        assert_ne!(first_hasher.finish(), second_hasher.finish());
    }

    #[test]
    fn auto_size_uses_svg_intrinsic_dimensions_when_loaded() {
        let mut svg = Svg::new_with_id(1, simple_svg());
        svg.apply_style(Style::new());
        std::thread::sleep(std::time::Duration::from_millis(10));
        let mut arena = new_test_arena();
        svg.measure(
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
        assert_eq!(svg.measured_size(), (80.0, 40.0));
    }

    #[test]
    fn computed_style_consumer_syncs_svg_element_render_state() {
        let mut svg = Svg::new_with_id(2, simple_svg());
        let mut computed = ComputedStyle::default();
        computed.background_color = Color::rgb(30, 40, 50);
        computed.border_colors = EdgeInsets {
            top: Color::rgb(210, 0, 0),
            right: Color::rgb(0, 210, 0),
            bottom: Color::rgb(0, 0, 210),
            left: Color::rgb(210, 210, 0),
        };
        computed.opacity = 0.45;

        ComputedStyleConsumer::apply_computed_style(&mut svg, computed, None);

        let render_state = svg.element.debug_render_state();
        assert_eq!(render_state.background_rgba, [30, 40, 50, 255]);
        assert_eq!(render_state.border_top_rgba, [210, 0, 0, 255]);
        assert_eq!(render_state.border_right_rgba, [0, 210, 0, 255]);
        assert_eq!(render_state.border_bottom_rgba, [0, 0, 210, 255]);
        assert_eq!(render_state.border_left_rgba, [210, 210, 0, 255]);
        assert!((render_state.opacity - 0.45).abs() < 0.001);
    }

    #[test]
    fn resize_cooldown_only_keeps_safe_oversampled_shrink() {
        let mut svg = Svg::new_with_id(2, simple_svg());
        svg.active_raster_request = Some(SvgRasterRequest::new(128, 64, SvgRasterMode::Uniform));
        svg.active_device_scale_bits = Some(1.0_f32.to_bits());
        svg.last_raster_request_at = Some(Instant::now());
        assert!(svg.should_keep_existing_raster(
            SvgRasterRequest::new(96, 48, SvgRasterMode::Uniform),
            1.0,
            Instant::now()
        ));
        assert!(!svg.should_keep_existing_raster(
            SvgRasterRequest::new(160, 80, SvgRasterMode::Uniform),
            1.0,
            Instant::now()
        ));
    }

    #[test]
    fn expired_cooldown_or_device_scale_change_requests_new_raster() {
        let mut svg = Svg::new_with_id(3, simple_svg());
        svg.active_raster_request = Some(SvgRasterRequest::new(128, 64, SvgRasterMode::Uniform));
        svg.active_device_scale_bits = Some(1.0_f32.to_bits());
        svg.last_raster_request_at = Some(Instant::now() - Duration::from_millis(200));
        assert!(!svg.should_keep_existing_raster(
            SvgRasterRequest::new(96, 48, SvgRasterMode::Uniform),
            1.0,
            Instant::now()
        ));
        svg.last_raster_request_at = Some(Instant::now());
        assert!(!svg.should_keep_existing_raster(
            SvgRasterRequest::new(96, 48, SvgRasterMode::Uniform),
            2.0,
            Instant::now()
        ));
    }

    #[test]
    fn intrinsic_mapping_is_independent_of_bucket_backing_for_all_fit_modes() {
        let mut svg = Svg::new_with_id(4, simple_svg());
        let contain = svg
            .resolve_raster_plan(80.0, 40.0, 100.0, 100.0, 1.0)
            .unwrap();
        assert_eq!(contain.request.physical_width, 128);
        assert_eq!(contain.request.physical_height, 64);
        assert_eq!(contain.local_draw_bounds, [0.0, 25.0, 100.0, 50.0]);
        assert_eq!(contain.uv_bounds, [0.0, 0.0, 128.0, 64.0]);

        svg.set_fit(crate::view::ImageFit::Cover);
        let cover = svg
            .resolve_raster_plan(80.0, 40.0, 100.0, 100.0, 1.0)
            .unwrap();
        assert_eq!(
            (cover.request.physical_width, cover.request.physical_height),
            (224, 112)
        );
        assert_eq!(cover.local_draw_bounds, [0.0, 0.0, 100.0, 100.0]);
        assert_eq!(cover.uv_bounds, [56.0, 0.0, 112.0, 112.0]);

        svg.set_fit(crate::view::ImageFit::Fill);
        let fill = svg
            .resolve_raster_plan(80.0, 40.0, 100.0, 100.0, 1.0)
            .unwrap();
        assert_eq!(
            (fill.request.physical_width, fill.request.physical_height),
            (128, 128)
        );
        assert_eq!(fill.local_draw_bounds, [0.0, 0.0, 100.0, 100.0]);
        assert_eq!(fill.uv_bounds, [0.0, 0.0, 128.0, 128.0]);
    }

    #[test]
    fn uniform_uv_uses_actual_resvg_scale_not_padded_backing_axis() {
        let svg = Svg::new_with_id(5, simple_svg());
        let wide = svg
            .resolve_raster_plan(101.0, 37.0, 101.0, 37.0, 1.0)
            .unwrap();
        assert_eq!(
            (wide.request.physical_width, wide.request.physical_height),
            (128, 47)
        );
        assert_eq!(wide.uv_bounds[2], 128.0);
        assert!(wide.uv_bounds[3] < 47.0);

        let tall = svg
            .resolve_raster_plan(37.0, 101.0, 37.0, 101.0, 1.0)
            .unwrap();
        assert_eq!(
            (tall.request.physical_width, tall.request.physical_height),
            (47, 128)
        );
        assert!(tall.uv_bounds[2] < 47.0);
        assert_eq!(tall.uv_bounds[3], 128.0);
    }

    #[test]
    fn viewport_scale_changes_physical_extent_without_changing_logical_mapping() {
        let svg = Svg::new_with_id(6, simple_svg());
        assert!(
            svg.resolve_raster_plan(80.0, 40.0, 80.0, 40.0, f32::NAN)
                .is_none()
        );
        assert!(
            svg.resolve_raster_plan(80.0, 40.0, 80.0, 40.0, f32::INFINITY)
                .is_none()
        );
        assert!(
            svg.resolve_raster_plan(80.0, 40.0, 80.0, 40.0, 0.0)
                .is_none()
        );
        let one = svg
            .resolve_raster_plan(80.0, 40.0, 80.0, 40.0, 1.0)
            .unwrap();
        let two = svg
            .resolve_raster_plan(80.0, 40.0, 80.0, 40.0, 2.0)
            .unwrap();
        assert_eq!(one.local_draw_bounds, two.local_draw_bounds);
        assert_eq!(
            (one.request.physical_width, one.request.physical_height),
            (96, 48)
        );
        assert_eq!(
            (two.request.physical_width, two.request.physical_height),
            (160, 80)
        );
    }

    #[test]
    fn prepared_svg_payload_owns_straight_srgb_upload_and_intrinsic_mapping() {
        let mut svg = Svg::new_with_id(61, simple_svg());
        let plan = svg
            .resolve_raster_plan(80.0, 40.0, 80.0, 40.0, 1.0)
            .unwrap();
        let image = ReadyImage {
            sampled_texture_id: SampledTextureId::SvgRaster(SvgRasterAssetId::for_test(61)),
            width: plan.request.physical_width,
            height: plan.request.physical_height,
            pixels: std::sync::Arc::from(vec![
                0_u8;
                (plan.request.physical_width * plan.request.physical_height * 4)
                    as usize
            ]),
            generation: 1,
        };
        let upload = svg.upload_for_image(&image).unwrap();
        svg.frozen_paint = Some(FrozenSvgPaint {
            document_key: svg.source_key,
            raster_key: 0,
            device_scale_bits: 1.0_f32.to_bits(),
            plan,
            inner_origin: [10.0, 20.0],
            upload,
            opacity: 0.75,
        });
        let prepared = svg.prepared_svg_op([0.25, -0.5], 0.75).unwrap();
        assert_eq!(
            prepared.upload.alpha_mode,
            crate::view::sampled_texture::SampledTextureAlphaMode::Straight
        );
        assert!(!prepared.params.source_is_premultiplied);
        assert_eq!(prepared.params.bounds, [10.25, 19.5, 80.0, 40.0]);
        assert_eq!(prepared.params.uv_bounds, Some([0.0, 0.0, 96.0, 48.0]));
    }

    #[test]
    fn postlayout_prepare_uses_final_bounds_once_and_refreshes_same_bucket_scale_identity() {
        let mut svg = freeze_ready_svg(62, simple_svg(), 1.0);
        let first = svg
            .resolve_raster_plan(80.0, 40.0, 80.0, 40.0, 1.0)
            .unwrap();
        let same_bucket = svg
            .resolve_raster_plan(80.0, 40.0, 80.0, 40.0, 1.01)
            .unwrap();
        assert_eq!(same_bucket.request, first.request);
        svg.prepare_frozen_paint(PaintResourcePreparationContext {
            frame_number: 10,
            device_scale: 1.01,
            now: Instant::now(),
        });
        assert!(
            svg.frozen_request_is_exact,
            "kind={:?} active={:?} desired={:?} scale={:?} pending={:?} paint={} slot={:?}",
            svg.source_kind,
            svg.active_raster_request,
            svg.frozen_desired_request,
            svg.active_device_scale_bits,
            svg.pending_raster_request,
            svg.frozen_paint.is_some(),
            svg.active_slot,
        );
        assert_eq!(svg.active_device_scale_bits, Some(1.01_f32.to_bits()));
        assert_eq!(svg.frozen_desired_request, Some(same_bucket.request));
        let frozen_bounds = svg.frozen_paint.as_ref().unwrap().plan.local_draw_bounds;

        layout_svg_element(&mut svg, 240.0, 120.0);
        svg.prepare_frozen_paint(PaintResourcePreparationContext {
            frame_number: 10,
            device_scale: 2.0,
            now: Instant::now(),
        });
        assert_eq!(
            svg.frozen_paint.as_ref().unwrap().plan.local_draw_bounds,
            frozen_bounds,
            "same-frame prepare must not refreeze after final layout"
        );
        assert_eq!(svg.frozen_desired_request, Some(same_bucket.request));
    }

    #[test]
    fn ready_resource_with_loading_slot_topology_stays_unprepared_until_next_prelayout() {
        let mut svg = freeze_ready_svg(63, simple_svg(), 1.0);
        svg.active_slot = super::ActiveSlot::Loading;
        svg.prepared_frame_number = None;
        svg.prepare_frozen_paint(PaintResourcePreparationContext {
            frame_number: 2,
            device_scale: 1.0,
            now: Instant::now(),
        });
        assert!(svg.frozen_paint.is_none());
        assert!(!svg.frozen_request_is_exact);
    }

    #[test]
    fn document_intrinsic_transition_marks_layout_while_slot_remains_loading() {
        let mut svg = Svg::new_with_id(64, simple_svg());
        svg.frozen_document = Some(SvgDocumentSnapshot::Loading);
        svg.frozen_active_raster = None;
        svg.active_slot = super::ActiveSlot::Loading;
        wait_until_document_ready(svg.source_key);
        svg.clear_local_dirty_flags(DirtyFlags::ALL);
        let mut arena = new_test_arena();

        svg.sync_arena(&mut arena);

        assert_eq!(svg.active_slot, super::ActiveSlot::Loading);
        assert!(svg.local_dirty_flags().contains(DirtyFlags::LAYOUT));
        svg.measure(
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
        assert_eq!(svg.measured_size(), (80.0, 40.0));
    }

    #[test]
    fn content_and_path_ready_record_matching_typed_owning_artifacts() {
        let mut arena = new_test_arena();
        let root = commit_element(
            &mut arena,
            Box::new(freeze_ready_svg(65, simple_svg(), 1.0)),
        );
        let node = arena.get(root).unwrap();
        assert_eq!(
            node.element.shadow_paint_recording_capability(
                &arena,
                false,
                crate::view::paint::PaintRecordingContext::default(),
            ),
            ShadowPaintRecordingCapability::Recordable
        );
        let artifact = node
            .element
            .record_shadow_paint_artifact(
                root,
                crate::view::compositor::property_tree::PropertyTreeState::default(),
                crate::view::paint::PaintContentRevision {
                    self_paint_revision: 1,
                    composite_revision: 1,
                    topology_revision: 1,
                },
                &arena,
                crate::view::paint::PaintRecordingContext::default(),
            )
            .expect("eligible Content SVG should record");
        assert_eq!(
            artifact.chunks[0].id.role,
            crate::view::paint::PaintChunkRole::SvgContent
        );
        assert!(matches!(
            artifact.ops.last(),
            Some(crate::view::paint::PaintOp::PreparedSvg(_))
        ));
        let Some(crate::view::paint::PaintOp::PreparedSvg(owned)) = artifact.ops.last() else {
            unreachable!();
        };
        let owned_identity = crate::view::paint::PreparedSvgIdentity::from_op(owned).unwrap();
        let owned_pixels = owned.upload.pixels.clone();
        drop(node);
        drop(arena);
        let Some(crate::view::paint::PaintOp::PreparedSvg(owned_after_drop)) = artifact.ops.last()
        else {
            unreachable!();
        };
        assert_eq!(
            crate::view::paint::PreparedSvgIdentity::from_op(owned_after_drop),
            Some(owned_identity)
        );
        assert!(std::sync::Arc::ptr_eq(
            &owned_pixels,
            &owned_after_drop.upload.pixels
        ));

        let path_svg = freeze_ready_svg(
            66,
            SvgSource::Path(std::path::PathBuf::from("never-read-by-test.svg")),
            1.0,
        );
        let path_document_key = path_svg.source_key;
        let path_raster_key = path_svg.active_raster_key.expect("ready Path raster key");
        let path_pixels = path_svg
            .frozen_paint
            .as_ref()
            .expect("ready Path frozen paint")
            .upload
            .pixels
            .clone();
        let mut path_arena = new_test_arena();
        let path_root = commit_element(&mut path_arena, Box::new(path_svg));
        let path_node = path_arena.get(path_root).unwrap();
        assert_eq!(
            path_node.element.shadow_paint_recording_capability(
                &path_arena,
                false,
                crate::view::paint::PaintRecordingContext::default(),
            ),
            ShadowPaintRecordingCapability::Recordable
        );
        let revision = crate::view::paint::PaintContentRevision {
            self_paint_revision: 2,
            composite_revision: 2,
            topology_revision: 2,
        };
        let metadata = path_node
            .element
            .record_shadow_paint_metadata(
                path_root,
                Default::default(),
                revision,
                &path_arena,
                Default::default(),
            )
            .expect("eligible Path SVG metadata");
        let path_artifact = path_node
            .element
            .record_shadow_paint_artifact(
                path_root,
                Default::default(),
                revision,
                &path_arena,
                Default::default(),
            )
            .expect("eligible Path SVG artifact");
        let Some(crate::view::paint::PaintOp::PreparedSvg(prepared)) = path_artifact.ops.last()
        else {
            panic!("Path artifact must own PreparedSvg")
        };
        let identity = crate::view::paint::PreparedSvgIdentity::from_op(prepared).unwrap();
        assert_eq!(identity.pixel_ptr, path_pixels.as_ptr() as usize);
        assert!(matches!(
            metadata.payload_identity,
            crate::view::paint::PaintPayloadIdentity::Svg(actual, _) if actual == identity
        ));
        assert_eq!(
            path_artifact.chunks[0].payload_identity,
            metadata.payload_identity
        );
        drop(path_node);
        drop(path_arena);
        remove_svg_raster_entry_for_test(path_raster_key);
        remove_svg_document_entry_for_test(path_document_key);
        let Some(crate::view::paint::PaintOp::PreparedSvg(prepared)) = path_artifact.ops.last()
        else {
            unreachable!()
        };
        assert!(std::sync::Arc::ptr_eq(
            &path_pixels,
            &prepared.upload.pixels
        ));
        assert!(prepared.upload.validate_rgba8().is_some());
    }

    #[test]
    fn path_source_request_and_device_scale_drift_fail_closed() {
        let mut stale = freeze_ready_svg(0x6b01, SvgSource::Path("stale-source-a.svg".into()), 1.0);
        let stale_document_key = stale.source_key;
        let stale_raster_key = stale.active_raster_key.expect("stale raster key");
        let stale_request = stale.active_raster_request.expect("stale request");
        let stale_paint = stale.frozen_paint.clone();
        let next_source = SvgSource::Path("stale-source-b-m9b2.svg".into());
        let next_document_key = prime_svg_document_ready_for_test(&next_source, 80.0, 40.0);
        stale.set_source(next_source);
        stale.frozen_document_key = Some(stale_document_key);
        stale.frozen_document = Some(SvgDocumentSnapshot::Ready {
            intrinsic_width: 80.0,
            intrinsic_height: 40.0,
        });
        stale.active_raster_key = Some(stale_raster_key);
        stale.active_raster_request = Some(stale_request);
        stale.active_device_scale_bits = Some(1.0_f32.to_bits());
        stale.frozen_desired_request = Some(stale_request);
        stale.frozen_active_raster = stale_paint.as_ref().map(|paint| {
            ImageSnapshot::Ready(ReadyImage {
                sampled_texture_id: paint.upload.id,
                width: paint.upload.width,
                height: paint.upload.height,
                pixels: paint.upload.pixels.clone(),
                generation: paint.upload.generation,
            })
        });
        stale.frozen_paint = stale_paint;
        stale.frozen_request_is_exact = true;
        stale.active_slot = super::ActiveSlot::None;
        let mut arena = new_test_arena();
        let root = commit_element(&mut arena, Box::new(stale));
        assert_eq!(
            arena
                .get(root)
                .unwrap()
                .element
                .shadow_paint_recording_capability(&arena, false, Default::default()),
            ShadowPaintRecordingCapability::Legacy(ShadowPaintBlocker::MissingPreparedSvg)
        );
        drop(arena);
        remove_svg_raster_entry_for_test(stale_raster_key);
        remove_svg_document_entry_for_test(stale_document_key);
        remove_svg_document_entry_for_test(next_document_key);

        for (id, mutate) in [(0x6b02, 0_u8), (0x6b03, 1_u8), (0x6b04, 2_u8)] {
            let mut svg = freeze_ready_svg(
                id,
                SvgSource::Path(format!("authority-drift-{id}.svg").into()),
                1.0,
            );
            match mutate {
                0 => {
                    svg.active_raster_request =
                        Some(SvgRasterRequest::new(160, 80, SvgRasterMode::Uniform));
                }
                1 => svg.active_device_scale_bits = Some(2.0_f32.to_bits()),
                2 => {
                    svg.pending_raster_request =
                        Some(SvgRasterRequest::new(160, 80, SvgRasterMode::Uniform));
                }
                _ => unreachable!(),
            }
            let mut arena = new_test_arena();
            let root = commit_element(&mut arena, Box::new(svg));
            assert_eq!(
                arena
                    .get(root)
                    .unwrap()
                    .element
                    .shadow_paint_recording_capability(&arena, false, Default::default()),
                ShadowPaintRecordingCapability::Legacy(ShadowPaintBlocker::MissingPreparedSvg)
            );
        }
    }

    #[test]
    fn path_generation_and_pixel_arc_are_frozen_across_metadata_and_full_recording() {
        let svg = freeze_ready_svg(0x6b10, SvgSource::Path("generation-freeze.svg".into()), 1.0);
        let raster_key = svg.active_raster_key.expect("ready raster key");
        let request = svg.active_raster_request.expect("ready raster request");
        let old_generation = svg
            .frozen_paint
            .as_ref()
            .expect("ready frozen paint")
            .upload
            .generation;
        let old_pixel_ptr = svg.frozen_paint.as_ref().unwrap().upload.pixels.as_ptr() as usize;
        let mut arena = new_test_arena();
        let root = commit_element(&mut arena, Box::new(svg));
        let revision = crate::view::paint::PaintContentRevision {
            self_paint_revision: 7,
            composite_revision: 7,
            topology_revision: 7,
        };
        let metadata = arena
            .get(root)
            .unwrap()
            .element
            .record_shadow_paint_metadata(
                root,
                Default::default(),
                revision,
                &arena,
                Default::default(),
            )
            .expect("Path metadata");
        let new_pixels: std::sync::Arc<[u8]> = std::sync::Arc::from(vec![
            9_u8;
            (request.physical_width * request.physical_height * 4)
                as usize
        ]);
        let new_generation = replace_svg_raster_ready_for_test(
            raster_key,
            request.physical_width,
            request.physical_height,
            new_pixels.clone(),
        );
        let artifact = arena
            .get(root)
            .unwrap()
            .element
            .record_shadow_paint_artifact(
                root,
                Default::default(),
                revision,
                &arena,
                Default::default(),
            )
            .expect("same-frame Path artifact");
        let Some(crate::view::paint::PaintOp::PreparedSvg(prepared)) = artifact.ops.last() else {
            panic!("Path artifact must own PreparedSvg")
        };
        let frozen_identity = crate::view::paint::PreparedSvgIdentity::from_op(prepared).unwrap();
        assert_eq!(frozen_identity.generation, old_generation);
        assert_eq!(frozen_identity.pixel_ptr, old_pixel_ptr);
        assert!(matches!(
            metadata.payload_identity,
            crate::view::paint::PaintPayloadIdentity::Svg(actual, _) if actual == frozen_identity
        ));

        let mut replaced_arc = prepared.clone();
        replaced_arc.upload.pixels = new_pixels.clone();
        replaced_arc.upload.generation = old_generation;
        let replaced_identity =
            crate::view::paint::PreparedSvgIdentity::from_op(&replaced_arc).unwrap();
        assert_ne!(replaced_identity, frozen_identity);
        assert_ne!(replaced_identity.pixel_ptr, frozen_identity.pixel_ptr);

        arena.with_element_taken(root, |element, arena| {
            let svg = element.as_any_mut().downcast_mut::<Svg>().unwrap();
            svg.sync_arena(arena);
            svg.prepare_frozen_paint(PaintResourcePreparationContext {
                frame_number: 3,
                device_scale: 1.0,
                now: Instant::now(),
            });
        });
        let next_artifact = arena
            .get(root)
            .unwrap()
            .element
            .record_shadow_paint_artifact(
                root,
                Default::default(),
                revision,
                &arena,
                Default::default(),
            )
            .expect("next-frame Path artifact");
        let Some(crate::view::paint::PaintOp::PreparedSvg(next)) = next_artifact.ops.last() else {
            unreachable!()
        };
        let next_identity = crate::view::paint::PreparedSvgIdentity::from_op(next).unwrap();
        assert_eq!(next_identity.generation, new_generation);
        assert_eq!(next_identity.pixel_ptr, new_pixels.as_ptr() as usize);
    }

    #[test]
    fn path_document_and_raster_loading_error_record_wrappers_but_invalid_ready_fails() {
        fn assert_wrapper(svg: Svg) {
            let mut arena = new_test_arena();
            let root = commit_element(&mut arena, Box::new(svg));
            let node = arena.get(root).unwrap();
            assert_eq!(
                node.element
                    .shadow_paint_recording_capability(&arena, false, Default::default()),
                ShadowPaintRecordingCapability::Recordable
            );
            let revision = crate::view::paint::PaintContentRevision {
                self_paint_revision: 1,
                composite_revision: 1,
                topology_revision: 1,
            };
            let metadata = node
                .element
                .record_shadow_paint_metadata(
                    root,
                    Default::default(),
                    revision,
                    &arena,
                    Default::default(),
                )
                .unwrap();
            let artifact = node
                .element
                .record_shadow_paint_artifact(
                    root,
                    Default::default(),
                    revision,
                    &arena,
                    Default::default(),
                )
                .unwrap();
            assert_eq!(
                metadata.id.role,
                crate::view::paint::PaintChunkRole::SelfDecoration
            );
            assert!(
                artifact
                    .ops
                    .iter()
                    .all(|op| { !matches!(op, crate::view::paint::PaintOp::PreparedSvg(_)) })
            );
        }

        fn assert_legacy(svg: Svg) {
            let mut arena = new_test_arena();
            let root = commit_element(&mut arena, Box::new(svg));
            assert_eq!(
                arena
                    .get(root)
                    .unwrap()
                    .element
                    .shadow_paint_recording_capability(&arena, false, Default::default()),
                ShadowPaintRecordingCapability::Legacy(ShadowPaintBlocker::MissingPreparedSvg)
            );
        }

        let mut document_loading =
            freeze_ready_svg(0x6b20, SvgSource::Path("document-loading.svg".into()), 1.0);
        set_svg_document_loading_for_test(document_loading.source_key);
        let mut sync_arena = new_test_arena();
        document_loading.sync_arena(&mut sync_arena);
        assert_wrapper(document_loading);

        let mut document_error =
            freeze_ready_svg(0x6b21, SvgSource::Path("document-error.svg".into()), 1.0);
        set_svg_document_error_for_test(document_error.source_key);
        document_error.sync_arena(&mut sync_arena);
        assert_wrapper(document_error);

        let mut raster_loading =
            freeze_ready_svg(0x6b22, SvgSource::Path("raster-loading.svg".into()), 1.0);
        set_svg_raster_loading_for_test(raster_loading.active_raster_key.unwrap());
        raster_loading.sync_arena(&mut sync_arena);
        assert_wrapper(raster_loading);

        let mut raster_error =
            freeze_ready_svg(0x6b23, SvgSource::Path("raster-error.svg".into()), 1.0);
        set_svg_raster_error_for_test(raster_error.active_raster_key.unwrap());
        raster_error.sync_arena(&mut sync_arena);
        assert_wrapper(raster_error);

        let mut invalid =
            freeze_ready_svg(0x6b24, SvgSource::Path("invalid-raster.svg".into()), 1.0);
        let request = invalid.active_raster_request.unwrap();
        replace_svg_raster_ready_for_test(
            invalid.active_raster_key.unwrap(),
            request.physical_width,
            request.physical_height,
            std::sync::Arc::from([1_u8, 2, 3]),
        );
        invalid.sync_arena(&mut sync_arena);
        invalid.prepare_frozen_paint(PaintResourcePreparationContext {
            frame_number: 3,
            device_scale: 1.0,
            now: Instant::now(),
        });
        assert_legacy(invalid);
    }

    #[test]
    fn shadow_svg_root_group_records_neutral_opacity_and_matching_identity() {
        let mut svg = freeze_ready_svg(0x6c40, simple_svg(), 1.0);
        svg.frozen_paint
            .as_mut()
            .expect("ready SVG has frozen paint")
            .opacity = 0.4;
        let mut arena = new_test_arena();
        let root = commit_element(&mut arena, Box::new(svg));
        let effect = crate::view::compositor::property_tree::EffectNodeId(root);
        let properties = crate::view::compositor::property_tree::PropertyTreeState {
            effect: Some(effect),
            ..Default::default()
        };
        let context = crate::view::paint::PaintRecordingContext {
            opacity_authority: crate::view::paint::PaintOpacityAuthority::NeutralRootEffect(effect),
            ..Default::default()
        };
        let revision = crate::view::paint::PaintContentRevision {
            self_paint_revision: 1,
            composite_revision: 1,
            topology_revision: 1,
        };
        let node = arena.get(root).unwrap();
        let metadata = node
            .element
            .record_shadow_paint_metadata(root, properties, revision, &arena, context)
            .expect("neutral SVG metadata");
        let artifact = node
            .element
            .record_shadow_paint_artifact(root, properties, revision, &arena, context)
            .expect("neutral SVG artifact");
        let Some(crate::view::paint::PaintOp::PreparedSvg(prepared)) = artifact.ops.last() else {
            panic!("neutral SVG must retain PreparedSvg")
        };
        assert_eq!(prepared.params.opacity.to_bits(), 1.0_f32.to_bits());
        let identity = crate::view::paint::PreparedSvgIdentity::from_op(prepared).unwrap();
        assert_eq!(identity.opacity_bits, 1.0_f32.to_bits());
        assert!(matches!(
            metadata.payload_identity,
            crate::view::paint::PaintPayloadIdentity::Svg(actual, _) if actual == identity
        ));
        assert_eq!(
            artifact.chunks[0].payload_identity,
            metadata.payload_identity
        );
    }

    #[test]
    fn actual_svg_artifact_compiles_after_arena_drop_and_forced_registry_removal() {
        let mut svg = Svg::new_with_id(67, unique_svg("owning-artifact-drop"));
        wait_until_document_ready(svg.source_key);
        layout_svg_element(&mut svg, 80.0, 40.0);
        let mut sync_arena = new_test_arena();
        svg.sync_arena(&mut sync_arena);
        svg.prepare_frozen_paint(PaintResourcePreparationContext {
            frame_number: 1,
            device_scale: 1.0,
            now: Instant::now(),
        });
        let raster_key = svg
            .active_raster_key
            .expect("first prepare requests raster");
        let request = svg.active_raster_request.expect("request identity frozen");
        set_svg_raster_ready_for_test(raster_key, request.physical_width, request.physical_height);
        svg.sync_arena(&mut sync_arena);
        svg.prepare_frozen_paint(PaintResourcePreparationContext {
            frame_number: 2,
            device_scale: 1.0,
            now: Instant::now(),
        });
        assert!(svg.frozen_request_is_exact);
        let document_key = svg.source_key;

        let mut arena = new_test_arena();
        let root = commit_element(&mut arena, Box::new(svg));
        let node = arena.get(root).unwrap();
        let artifact = node
            .element
            .record_shadow_paint_artifact(
                root,
                crate::view::compositor::property_tree::PropertyTreeState::default(),
                crate::view::paint::PaintContentRevision {
                    self_paint_revision: 2,
                    composite_revision: 2,
                    topology_revision: 2,
                },
                &arena,
                crate::view::paint::PaintRecordingContext::default(),
            )
            .expect("actual frozen SVG hook should record");
        drop(node);
        drop(arena);
        remove_svg_raster_entry_for_test(raster_key);
        remove_svg_document_entry_for_test(document_key);

        let mut graph = crate::view::frame_graph::FrameGraph::new();
        let mut ctx = crate::view::base_component::UiBuildContext::new(
            80,
            40,
            wgpu::TextureFormat::Bgra8Unorm,
            1.0,
        );
        let target = ctx.allocate_target(&mut graph);
        ctx.set_current_target(target);
        let _ = crate::view::paint::compile_artifact(&artifact, &mut graph, ctx);
        assert_eq!(
            graph
                .test_graphics_passes::<crate::view::render_pass::TextureCompositePass>()
                .len(),
            1
        );
    }

    #[test]
    fn visible_child_and_nonexact_svg_fail_preflight_without_full_artifact_hook() {
        fn assert_missing_prepared_svg(
            arena: &crate::view::node_arena::NodeArena,
            root: crate::view::node_arena::NodeKey,
        ) {
            let mut properties = crate::view::compositor::PropertyTrees::default();
            properties.sync(arena, &[root]);
            let mut generations = crate::view::compositor::PaintGenerationTracker::default();
            generations.sync(arena, &[root], &properties);
            let preflight = crate::view::paint::record_coverage_manifest(
                arena,
                &[root],
                &rustc_hash::FxHashSet::default(),
                None,
                false,
                true,
                crate::view::paint::CoverageRecordingMode::MetadataOnly,
                &properties,
                &generations,
            );
            assert!(
                matches!(
                    preflight.items.as_slice(),
                    [crate::view::paint::PaintCoverageItem::LegacyBoundary {
                        reason: crate::view::paint::LegacyPaintReason::MissingPreparedSvg
                            | crate::view::paint::LegacyPaintReason::MissingPreparedInlineRoot,
                        ..
                    }]
                ),
                "unexpected SVG preflight: {:#?}",
                preflight.items
            );
            let _ = crate::view::paint::take_full_artifact_record_count();
            let full = crate::view::paint::record_coverage_manifest(
                arena,
                &[root],
                &rustc_hash::FxHashSet::default(),
                None,
                false,
                true,
                crate::view::paint::CoverageRecordingMode::FullArtifact,
                &properties,
                &generations,
            );
            assert!(matches!(
                full.items.as_slice(),
                [crate::view::paint::PaintCoverageItem::LegacyBoundary {
                    reason: crate::view::paint::LegacyPaintReason::MissingPreparedSvg
                        | crate::view::paint::LegacyPaintReason::MissingPreparedInlineRoot,
                    ..
                }]
            ));
            assert_eq!(crate::view::paint::take_full_artifact_record_count(), 0);
        }

        let mut child_arena = new_test_arena();
        let child_root = commit_element(
            &mut child_arena,
            Box::new(freeze_ready_svg(69, simple_svg(), 1.0)),
        );
        let _ = commit_child(
            &mut child_arena,
            child_root,
            Box::new(crate::view::base_component::Element::new_with_id(
                690, 0.0, 0.0, 4.0, 4.0,
            )),
        );
        assert_missing_prepared_svg(&child_arena, child_root);

        let mut nonexact_arena = new_test_arena();
        let mut nonexact = freeze_ready_svg(70, simple_svg(), 1.0);
        nonexact.frozen_request_is_exact = false;
        nonexact.pending_raster_request =
            Some(SvgRasterRequest::new(160, 80, SvgRasterMode::Uniform));
        let nonexact_root = commit_element(&mut nonexact_arena, Box::new(nonexact));
        assert_missing_prepared_svg(&nonexact_arena, nonexact_root);
    }

    #[test]
    fn setters_mark_dirty_only_when_render_identity_changes() {
        let mut svg = Svg::new_with_id(7, simple_svg());
        svg.clear_local_dirty_flags(crate::view::base_component::DirtyFlags::ALL);
        svg.set_fit(crate::view::ImageFit::Contain);
        svg.set_sampling(crate::view::ImageSampling::Linear);
        svg.set_source(simple_svg());
        assert!(svg.local_dirty_flags().is_empty());

        svg.set_fit(crate::view::ImageFit::Cover);
        assert_eq!(
            svg.local_dirty_flags(),
            crate::view::base_component::DirtyFlags::PAINT
        );
        svg.clear_local_dirty_flags(crate::view::base_component::DirtyFlags::ALL);
        svg.set_sampling(crate::view::ImageSampling::Nearest);
        assert_eq!(
            svg.local_dirty_flags(),
            crate::view::base_component::DirtyFlags::PAINT
        );
        svg.clear_local_dirty_flags(crate::view::base_component::DirtyFlags::ALL);
        svg.set_source(SvgSource::Content(
            r##"<svg width="1" height="1" xmlns="http://www.w3.org/2000/svg"/>"##.into(),
        ));
        assert_eq!(
            svg.local_dirty_flags(),
            crate::view::base_component::DirtyFlags::ALL
        );
    }

    #[test]
    fn normalized_equivalent_path_source_keeps_document_and_raster_state() {
        let relative = std::path::PathBuf::from("target/nonexistent-equivalent.svg");
        let absolute = std::env::current_dir().unwrap().join(&relative);
        let mut svg = Svg::new_with_id(8, SvgSource::Path(relative));
        let document_key = svg.source_key;
        let marker = SvgRasterRequest::new(96, 48, SvgRasterMode::Uniform);
        svg.active_raster_request = Some(marker);
        svg.clear_local_dirty_flags(crate::view::base_component::DirtyFlags::ALL);

        svg.set_source(SvgSource::Path(absolute));

        assert_eq!(svg.source_key, document_key);
        assert_eq!(svg.active_raster_request, Some(marker));
        assert!(svg.local_dirty_flags().is_empty());
    }

    #[test]
    fn returning_to_active_request_cancels_pending_lease() {
        let mut svg = Svg::new_with_id(9, unique_svg("pending-cancel"));
        let active_request = SvgRasterRequest::new(96, 48, SvgRasterMode::Uniform);
        let pending_request = SvgRasterRequest::new(128, 64, SvgRasterMode::Uniform);
        let active_key = acquire_svg_raster(svg.source_key, active_request);
        let pending_key = acquire_svg_raster(svg.source_key, pending_request);
        svg.active_raster_key = Some(active_key);
        svg.active_raster_request = Some(active_request);
        svg.active_device_scale_bits = Some(1.0_f32.to_bits());
        svg.pending_raster_key = Some(pending_key);
        svg.pending_raster_request = Some(pending_request);
        svg.pending_device_scale_bits = Some(1.0_f32.to_bits());
        assert_eq!(svg_raster_ref_count_for_test(pending_key), Some(1));

        assert_eq!(
            svg.sync_raster_key(active_request, 1.0, Instant::now()),
            Some(active_key)
        );
        assert_eq!(svg.pending_raster_key, None);
        assert_eq!(svg_raster_ref_count_for_test(pending_key), Some(0));
    }

    #[test]
    fn failed_pending_request_is_memoized_until_request_identity_changes() {
        let mut svg = Svg::new_with_id(10, unique_svg("failed-memo"));
        let active_request = SvgRasterRequest::new(96, 48, SvgRasterMode::Uniform);
        let failed_request = SvgRasterRequest::new(128, 64, SvgRasterMode::Uniform);
        let active_key = acquire_svg_raster(svg.source_key, active_request);
        let failed_key = acquire_svg_raster(svg.source_key, failed_request);
        svg.active_raster_key = Some(active_key);
        svg.active_raster_request = Some(active_request);
        svg.active_device_scale_bits = Some(1.0_f32.to_bits());
        svg.pending_raster_key = Some(failed_key);
        svg.pending_raster_request = Some(failed_request);
        svg.pending_device_scale_bits = Some(1.0_f32.to_bits());
        set_svg_raster_error_for_test(failed_key);

        assert_eq!(
            svg.sync_raster_key(failed_request, 1.0, Instant::now()),
            Some(active_key)
        );
        assert_eq!(svg.failed_raster_request, Some(failed_request));
        assert_eq!(svg_raster_ref_count_for_test(failed_key), Some(0));
        for _ in 0..3 {
            assert_eq!(
                svg.sync_raster_key(failed_request, 1.0, Instant::now()),
                Some(active_key)
            );
            assert_eq!(svg.pending_raster_key, None);
            assert_eq!(svg_raster_ref_count_for_test(failed_key), Some(0));
        }

        let changed_request = SvgRasterRequest::new(160, 80, SvgRasterMode::Uniform);
        assert_eq!(
            svg.sync_raster_key(changed_request, 1.0, Instant::now()),
            Some(active_key)
        );
        assert_eq!(svg.failed_raster_request, None);
        assert_eq!(svg.pending_raster_request, Some(changed_request));
    }

    #[test]
    fn pending_readiness_is_invisible_until_next_prelayout_freeze_then_swaps() {
        let mut svg = Svg::new_with_id(11, unique_svg("promotion-pending"));
        wait_until_document_ready(svg.source_key);
        let active_request = SvgRasterRequest::new(96, 48, SvgRasterMode::Uniform);
        let pending_request = SvgRasterRequest::new(128, 64, SvgRasterMode::Uniform);
        let active_key = acquire_svg_raster(svg.source_key, active_request);
        let pending_key = acquire_svg_raster(svg.source_key, pending_request);
        wait_until_raster_ready(active_key);
        wait_until_raster_ready(pending_key);
        set_svg_raster_loading_for_test(pending_key);
        svg.active_raster_key = Some(active_key);
        svg.active_raster_request = Some(active_request);
        svg.active_device_scale_bits = Some(1.0_f32.to_bits());
        svg.pending_raster_key = Some(pending_key);
        svg.pending_raster_request = Some(pending_request);
        svg.pending_device_scale_bits = Some(1.0_f32.to_bits());

        let mut arena = new_test_arena();
        svg.sync_arena(&mut arena);
        let loading = svg.promotion_self_signature();
        assert_eq!(loading, svg.promotion_self_signature());
        set_svg_raster_ready_for_test(
            pending_key,
            pending_request.physical_width,
            pending_request.physical_height,
        );
        let ready = svg.promotion_self_signature();
        assert_eq!(ready, loading);
        svg.sync_arena(&mut arena);
        let next_frame_ready = svg.promotion_self_signature();
        assert_ne!(next_frame_ready, loading);
        assert_eq!(
            svg.sync_raster_key(pending_request, 1.0, Instant::now()),
            Some(pending_key)
        );
        assert_eq!(svg.active_raster_request, Some(pending_request));
        assert_eq!(svg.pending_raster_key, None);
        assert_eq!(svg_raster_ref_count_for_test(active_key), Some(0));
    }
}
