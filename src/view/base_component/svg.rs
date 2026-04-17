use crate::time::{Duration, Instant};
use crate::view::frame_graph::FrameGraph;
use crate::view::image_resource::ImageSnapshot;
use crate::view::render_pass::TextureCompositePass;
use crate::view::render_pass::texture_composite_pass::{
    TextureCompositeInput, TextureCompositeOutput, TextureCompositeParams,
};
use crate::view::svg_resource::{
    SvgDocumentSnapshot, acquire_svg_document, acquire_svg_raster, needs_upload,
    quantize_svg_raster_size, release_svg_document, release_svg_raster, snapshot_svg_document,
    snapshot_svg_raster,
};
use crate::view::{ImageFit, ImageSampling, SvgSource};
use crate::{ParsedValue, PropertyId, Style};

use super::{
    BoxModelSnapshot, Element, ElementTrait, EventTarget, LayoutConstraints, LayoutPlacement,
    Layoutable, Renderable, UiBuildContext,
};

const PLACEHOLDER_SIZE: f32 = 120.0;
const SVG_RESIZE_REQUEST_COOLDOWN: Duration = Duration::from_millis(90);
const SVG_RESIZE_HYSTERESIS_PX: u32 = 24;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActiveSlot {
    None,
    Loading,
    Error,
}

pub struct Svg {
    element: Element,
    source_key: u64,
    fit: ImageFit,
    sampling: ImageSampling,
    loading_slot: Vec<Box<dyn ElementTrait>>,
    error_slot: Vec<Box<dyn ElementTrait>>,
    active_slot: ActiveSlot,
    active_raster_key: Option<u64>,
    active_raster_size: Option<(u32, u32)>,
    last_raster_request_at: Option<Instant>,
}

impl Svg {
    pub fn new_with_id(id: u64, source: SvgSource) -> Self {
        let mut element = Element::new_with_id(id, 0.0, 0.0, PLACEHOLDER_SIZE, PLACEHOLDER_SIZE);
        let mut base_style = Style::new();
        base_style.insert(PropertyId::Width, ParsedValue::Auto);
        base_style.insert(PropertyId::Height, ParsedValue::Auto);
        element.apply_style(base_style);
        Self {
            element,
            source_key: acquire_svg_document(&source),
            fit: ImageFit::Contain,
            sampling: ImageSampling::Linear,
            loading_slot: Vec::new(),
            error_slot: Vec::new(),
            active_slot: ActiveSlot::None,
            active_raster_key: None,
            active_raster_size: None,
            last_raster_request_at: None,
        }
    }

    pub fn set_fit(&mut self, fit: ImageFit) {
        self.fit = fit;
    }

    pub fn set_sampling(&mut self, sampling: ImageSampling) {
        self.sampling = sampling;
    }

    pub fn apply_style(&mut self, style: crate::Style) {
        self.element.apply_style(style);
    }

    pub fn set_loading_slot(&mut self, slot: Vec<Box<dyn ElementTrait>>) {
        self.loading_slot = slot;
    }

    pub fn set_error_slot(&mut self, slot: Vec<Box<dyn ElementTrait>>) {
        self.error_slot = slot;
    }

    fn document_snapshot(&self) -> SvgDocumentSnapshot {
        snapshot_svg_document(self.source_key).unwrap_or(SvgDocumentSnapshot::Loading)
    }

    fn sync_active_slot(&mut self, next_slot: ActiveSlot) {
        if self.active_slot == next_slot {
            return;
        }
        let next_children = match next_slot {
            ActiveSlot::None => Vec::new(),
            ActiveSlot::Loading => std::mem::take(&mut self.loading_slot),
            ActiveSlot::Error => std::mem::take(&mut self.error_slot),
        };
        let previous_children = self.element.replace_children(next_children);
        match self.active_slot {
            ActiveSlot::None => {}
            ActiveSlot::Loading => self.loading_slot = previous_children,
            ActiveSlot::Error => self.error_slot = previous_children,
        }
        self.active_slot = next_slot;
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

    fn resolve_document_slot(snapshot: &SvgDocumentSnapshot) -> ActiveSlot {
        match snapshot {
            SvgDocumentSnapshot::Loading => ActiveSlot::Loading,
            SvgDocumentSnapshot::Error(message) => {
                let _ = message;
                ActiveSlot::Error
            }
            SvgDocumentSnapshot::Ready { .. } => ActiveSlot::None,
        }
    }

    fn resolve_raster_size(
        &self,
        source_w: f32,
        source_h: f32,
        dest_w: f32,
        dest_h: f32,
    ) -> (u32, u32) {
        let (draw_bounds, _) =
            super::image::compute_image_mapping(self.fit, source_w, source_h, dest_w, dest_h);
        quantize_svg_raster_size(
            draw_bounds[2].round().max(1.0) as u32,
            draw_bounds[3].round().max(1.0) as u32,
        )
    }

    fn should_keep_existing_raster(&self, raster_size: (u32, u32), now: Instant) -> bool {
        let Some(current_size) = self.active_raster_size else {
            return false;
        };
        if current_size == raster_size {
            return true;
        }
        let within_hysteresis = current_size.0.abs_diff(raster_size.0) <= SVG_RESIZE_HYSTERESIS_PX
            && current_size.1.abs_diff(raster_size.1) <= SVG_RESIZE_HYSTERESIS_PX;
        let within_cooldown = self
            .last_raster_request_at
            .is_some_and(|last| now.duration_since(last) < SVG_RESIZE_REQUEST_COOLDOWN);
        within_hysteresis || within_cooldown
    }

    fn sync_raster_key(&mut self, raster_size: (u32, u32), now: Instant) -> Option<u64> {
        if self.active_raster_size == Some(raster_size) {
            return self.active_raster_key;
        }
        if self.should_keep_existing_raster(raster_size, now) {
            return self.active_raster_key;
        }
        if let Some(previous_key) = self.active_raster_key.take() {
            release_svg_raster(previous_key);
        }
        let raster_key = acquire_svg_raster(self.source_key, raster_size.0, raster_size.1);
        self.active_raster_size = Some(raster_size);
        self.active_raster_key = Some(raster_key);
        self.last_raster_request_at = Some(now);
        self.active_raster_key
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

impl ElementTrait for Svg {
    fn id(&self) -> u64 {
        self.element.id()
    }

    fn parent_id(&self) -> Option<u64> {
        self.element.parent_id()
    }

    fn set_parent_id(&mut self, parent_id: Option<u64>) {
        self.element.set_parent_id(parent_id);
    }

    fn box_model_snapshot(&self) -> BoxModelSnapshot {
        self.element.box_model_snapshot()
    }

    fn children(&self) -> Option<&[Box<dyn ElementTrait>]> {
        self.element.children()
    }

    fn children_mut(&mut self) -> Option<&mut [Box<dyn ElementTrait>]> {
        self.element.children_mut()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn snapshot_state(&self) -> Option<Box<dyn std::any::Any>> {
        self.element.snapshot_state()
    }

    fn restore_state(&mut self, snapshot: &dyn std::any::Any) -> bool {
        self.element.restore_state(snapshot)
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

    fn promotion_self_signature(&self) -> u64 {
        self.element.promotion_self_signature()
    }

    fn promotion_clip_intersection_signature(&self) -> u64 {
        self.element.promotion_clip_intersection_signature()
    }

    fn promotion_composite_bounds(&self) -> super::PromotionCompositeBounds {
        self.element.promotion_composite_bounds()
    }

    fn local_dirty_flags(&self) -> super::DirtyFlags {
        self.element.local_dirty_flags()
    }

    fn clear_local_dirty_flags(&mut self, flags: super::DirtyFlags) {
        self.element.clear_local_dirty_flags(flags);
    }
}

impl EventTarget for Svg {
    fn dispatch_mouse_down(
        &mut self,
        event: &mut crate::ui::MouseDownEvent,
        control: &mut crate::view::viewport::ViewportControl<'_>,
    ) {
        self.element.dispatch_mouse_down(event, control);
    }

    fn dispatch_mouse_up(
        &mut self,
        event: &mut crate::ui::MouseUpEvent,
        control: &mut crate::view::viewport::ViewportControl<'_>,
    ) {
        self.element.dispatch_mouse_up(event, control);
    }

    fn dispatch_mouse_move(
        &mut self,
        event: &mut crate::ui::MouseMoveEvent,
        control: &mut crate::view::viewport::ViewportControl<'_>,
    ) {
        self.element.dispatch_mouse_move(event, control);
    }

    fn dispatch_mouse_enter(&mut self, event: &mut crate::ui::MouseEnterEvent) {
        self.element.dispatch_mouse_enter(event);
    }

    fn dispatch_mouse_leave(&mut self, event: &mut crate::ui::MouseLeaveEvent) {
        self.element.dispatch_mouse_leave(event);
    }

    fn dispatch_click(
        &mut self,
        event: &mut crate::ui::ClickEvent,
        control: &mut crate::view::viewport::ViewportControl<'_>,
    ) {
        self.element.dispatch_click(event, control);
    }

    fn dispatch_key_down(
        &mut self,
        event: &mut crate::ui::KeyDownEvent,
        control: &mut crate::view::viewport::ViewportControl<'_>,
    ) {
        self.element.dispatch_key_down(event, control);
    }

    fn dispatch_key_up(
        &mut self,
        event: &mut crate::ui::KeyUpEvent,
        control: &mut crate::view::viewport::ViewportControl<'_>,
    ) {
        self.element.dispatch_key_up(event, control);
    }

    fn dispatch_focus(
        &mut self,
        event: &mut crate::ui::FocusEvent,
        control: &mut crate::view::viewport::ViewportControl<'_>,
    ) {
        self.element.dispatch_focus(event, control);
    }

    fn dispatch_blur(
        &mut self,
        event: &mut crate::ui::BlurEvent,
        control: &mut crate::view::viewport::ViewportControl<'_>,
    ) {
        self.element.dispatch_blur(event, control);
    }

    fn cancel_pointer_interaction(&mut self) -> bool {
        self.element.cancel_pointer_interaction()
    }

    fn set_hovered(&mut self, hovered: bool) -> bool {
        self.element.set_hovered(hovered)
    }

    fn scroll_by(&mut self, dx: f32, dy: f32) -> bool {
        self.element.scroll_by(dx, dy)
    }

    fn can_scroll_by(&self, dx: f32, dy: f32) -> bool {
        self.element.can_scroll_by(dx, dy)
    }

    fn get_scroll_offset(&self) -> (f32, f32) {
        self.element.get_scroll_offset()
    }

    fn set_scroll_offset(&mut self, offset: (f32, f32)) {
        self.element.set_scroll_offset(offset);
    }

    fn cursor(&self) -> crate::Cursor {
        self.element.cursor()
    }

    fn wants_animation_frame(&self) -> bool {
        self.element.wants_animation_frame()
    }

    fn take_style_transition_requests(&mut self) -> Vec<crate::transition::StyleTrackRequest> {
        self.element.take_style_transition_requests()
    }

    fn take_layout_transition_requests(&mut self) -> Vec<crate::transition::LayoutTrackRequest> {
        self.element.take_layout_transition_requests()
    }

    fn take_visual_transition_requests(&mut self) -> Vec<crate::transition::VisualTrackRequest> {
        self.element.take_visual_transition_requests()
    }
}

impl Layoutable for Svg {
    fn measure(&mut self, constraints: LayoutConstraints) {
        let snapshot = self.document_snapshot();
        self.sync_active_slot(Self::resolve_document_slot(&snapshot));
        self.element.measure(constraints);
        self.apply_intrinsic_measurement(constraints, Self::intrinsic_size(&snapshot));
    }

    fn place(&mut self, placement: LayoutPlacement) {
        self.element.place(placement);
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
        release_svg_document(self.source_key);
    }
}

impl Renderable for Svg {
    fn build(&mut self, graph: &mut FrameGraph, ctx: UiBuildContext) -> super::BuildState {
        let document_snapshot = self.document_snapshot();
        self.sync_active_slot(Self::resolve_document_slot(&document_snapshot));

        let viewport = ctx.viewport();
        let base_state = self.element.build_base_only(graph, ctx);
        let mut ctx = UiBuildContext::from_parts(viewport, base_state);
        let SvgDocumentSnapshot::Ready {
            intrinsic_width,
            intrinsic_height,
        } = document_snapshot
        else {
            return ctx.into_state();
        };

        let (inner_x, inner_y, inner_w, inner_h) = self.element.inner_content_rect_for_render();
        if inner_w <= 0.0 || inner_h <= 0.0 {
            return ctx.into_state();
        }
        let Some(parent_target) = ctx.current_target() else {
            return ctx.into_state();
        };

        let raster_size =
            self.resolve_raster_size(intrinsic_width, intrinsic_height, inner_w, inner_h);
        let Some(raster_key) = self.sync_raster_key(raster_size, Instant::now()) else {
            return ctx.into_state();
        };
        let snapshot = snapshot_svg_raster(raster_key).unwrap_or(ImageSnapshot::Loading);
        let active_slot = match &snapshot {
            ImageSnapshot::Loading => ActiveSlot::Loading,
            ImageSnapshot::Error(_) => ActiveSlot::Error,
            ImageSnapshot::Ready(_) => ActiveSlot::None,
        };
        self.sync_active_slot(active_slot);
        let ImageSnapshot::Ready(image) = snapshot else {
            return ctx.into_state();
        };

        let should_upload = needs_upload(raster_key, image.generation);
        let (local_draw_bounds, uv_bounds) = super::image::compute_image_mapping(
            self.fit,
            image.width as f32,
            image.height as f32,
            inner_w,
            inner_h,
        );
        let draw_bounds = [
            inner_x + local_draw_bounds[0],
            inner_y + local_draw_bounds[1],
            local_draw_bounds[2],
            local_draw_bounds[3],
        ];

        graph.add_graphics_pass(TextureCompositePass::new(
            TextureCompositeParams {
                bounds: draw_bounds,
                quad_positions: None,
                uv_bounds: Some(uv_bounds),
                mask_uv_bounds: None,
                use_mask: false,
                source_is_premultiplied: false,
                opacity: if ctx.is_node_promoted(self.id()) {
                    1.0
                } else {
                    self.element.promotion_node_info().opacity.clamp(0.0, 1.0)
                },
                scissor_rect: None,
            },
            TextureCompositeInput {
                source: Default::default(),
                sampled_source_key: Some(raster_key),
                sampled_source_size: Some((image.width, image.height)),
                sampled_source_upload: if should_upload {
                    Some(image.pixels.clone())
                } else {
                    None
                },
                sampled_upload_state_key: if should_upload {
                    Some(raster_key)
                } else {
                    None
                },
                sampled_upload_generation: if should_upload {
                    Some(image.generation)
                } else {
                    None
                },
                sampled_source_sampling: Some(self.sampling),
                mask: Default::default(),
                pass_context: ctx.graphics_pass_context(),
            },
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
    use super::Svg;
    use crate::Style;
    use crate::time::{Duration, Instant};
    use crate::view::SvgSource;
    use crate::view::base_component::{LayoutConstraints, Layoutable};

    fn simple_svg() -> SvgSource {
        SvgSource::Content(
            r##"<svg width="80" height="40" viewBox="0 0 80 40" xmlns="http://www.w3.org/2000/svg"><rect width="80" height="40" fill="#ff0000"/></svg>"##.to_string(),
        )
    }

    #[test]
    fn auto_size_uses_svg_intrinsic_dimensions_when_loaded() {
        let mut svg = Svg::new_with_id(1, simple_svg());
        svg.apply_style(Style::new());
        std::thread::sleep(std::time::Duration::from_millis(10));
        svg.measure(LayoutConstraints {
            max_width: 500.0,
            max_height: 500.0,
            viewport_width: 500.0,
            viewport_height: 500.0,
            percent_base_width: None,
            percent_base_height: None,
        });
        assert_eq!(svg.measured_size(), (80.0, 40.0));
    }

    #[test]
    fn keep_existing_raster_during_resize_cooldown() {
        let mut svg = Svg::new_with_id(2, simple_svg());
        svg.active_raster_size = Some((128, 64));
        svg.last_raster_request_at = Some(Instant::now());
        assert!(svg.should_keep_existing_raster((160, 96), Instant::now()));
    }

    #[test]
    fn keep_existing_raster_within_hysteresis_window() {
        let mut svg = Svg::new_with_id(3, simple_svg());
        svg.active_raster_size = Some((128, 64));
        svg.last_raster_request_at = Some(Instant::now() - Duration::from_millis(200));
        assert!(svg.should_keep_existing_raster((144, 80), Instant::now()));
    }
}
