use crate::view::frame_graph::FrameGraph;
use crate::view::image_resource::{
    ImageHandle, ImageSnapshot, acquire_image_resource, needs_upload, snapshot_image,
};
use crate::view::render_pass::TextureCompositePass;
use crate::view::render_pass::texture_composite_pass::{
    TextureCompositeInput, TextureCompositeOutput, TextureCompositeParams,
};
use crate::view::{ImageFit, ImageSampling, ImageSource};
use crate::{ParsedValue, PropertyId, Style};

use super::{
    BoxModelSnapshot, Element, ElementTrait, EventTarget, LayoutConstraints, LayoutPlacement,
    Layoutable, Renderable, UiBuildContext,
};
use crate::view::node_arena::{NodeArena, NodeKey};

const PLACEHOLDER_SIZE: f32 = 120.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActiveSlot {
    None,
    Loading,
    Error,
}

pub struct Image {
    element: Element,
    fit: ImageFit,
    sampling: ImageSampling,
    source_handle: ImageHandle,
    /// Pending loading-slot wrapper keys (detached from `Element.children`
    /// until `sync_active_slot` promotes them). Live in the same arena as
    /// the owning Image but not traversed while inactive.
    loading_slot: Vec<NodeKey>,
    error_slot: Vec<NodeKey>,
    active_slot: ActiveSlot,
}

impl Image {
    pub fn new_with_id(id: u64, source: ImageSource) -> Self {
        let mut element = Element::new_with_id(id, 0.0, 0.0, PLACEHOLDER_SIZE, PLACEHOLDER_SIZE);
        let mut base_style = Style::new();
        base_style.insert(PropertyId::Width, ParsedValue::Auto);
        base_style.insert(PropertyId::Height, ParsedValue::Auto);
        element.apply_style(base_style);
        Self {
            element,
            source_handle: acquire_image_resource(&source),
            fit: ImageFit::Contain,
            sampling: ImageSampling::Linear,
            loading_slot: Vec::new(),
            error_slot: Vec::new(),
            active_slot: ActiveSlot::None,
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

    /// Register a pre-committed loading-slot wrapper. Caller is responsible
    /// for inserting the wrapper (and its subtree) into the arena with
    /// parent set to this Image's own `NodeKey` before calling.
    pub fn set_loading_slot(&mut self, slot: Vec<NodeKey>) {
        self.loading_slot = slot;
    }

    pub fn set_error_slot(&mut self, slot: Vec<NodeKey>) {
        self.error_slot = slot;
    }

    fn snapshot(&mut self) -> ImageSnapshot {
        snapshot_image(self.source_handle.key()).unwrap_or(ImageSnapshot::Loading)
    }

    fn sync_active_slot(&mut self, arena: &mut NodeArena, next_slot: ActiveSlot) {
        if self.active_slot == next_slot {
            return;
        }
        let next_children = match next_slot {
            ActiveSlot::None => Vec::new(),
            ActiveSlot::Loading => std::mem::take(&mut self.loading_slot),
            ActiveSlot::Error => std::mem::take(&mut self.error_slot),
        };
        let previous_children = self.element.replace_children(arena, next_children);
        match self.active_slot {
            ActiveSlot::None => {}
            ActiveSlot::Loading => self.loading_slot = previous_children,
            ActiveSlot::Error => self.error_slot = previous_children,
        }
        self.active_slot = next_slot;
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

impl ElementTrait for Image {
    fn id(&self) -> u64 {
        self.element.id()
    }

    fn box_model_snapshot(&self) -> BoxModelSnapshot {
        self.element.box_model_snapshot()
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

    fn has_active_animator(&self) -> bool {
        self.element.has_active_animator()
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

impl EventTarget for Image {
    crate::view::base_component::forward_event_target!(full element);
}

impl Layoutable for Image {
    fn measure(
        &mut self,
        constraints: LayoutConstraints,
        arena: &mut crate::view::node_arena::NodeArena,
    ) {
        let snapshot = self.snapshot();
        self.sync_active_slot(arena, Self::resolve_slot(&snapshot));
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

impl Renderable for Image {
    fn build(
        &mut self,
        graph: &mut FrameGraph,
        arena: &mut crate::view::node_arena::NodeArena,
        ctx: UiBuildContext,
    ) -> super::BuildState {
        let snapshot = self.snapshot();
        self.sync_active_slot(arena, Self::resolve_slot(&snapshot));

        let viewport = ctx.viewport();
        let base_state = self.element.build_base_only(graph, arena, ctx);
        let mut ctx = UiBuildContext::from_parts(viewport, base_state);
        let ImageSnapshot::Ready(image) = snapshot else {
            return ctx.into_state();
        };

        let (inner_x, inner_y, inner_w, inner_h) = self.element.inner_content_rect_for_render();
        if inner_w <= 0.0 || inner_h <= 0.0 {
            return ctx.into_state();
        }
        let Some(parent_target) = ctx.current_target() else {
            return ctx.into_state();
        };

        let should_upload = needs_upload(self.source_handle.key(), image.generation);
        let (local_draw_bounds, uv_bounds) = compute_image_mapping(
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
                sampled_source_key: Some(self.source_handle.key()),
                sampled_source_size: Some((image.width, image.height)),
                sampled_source_upload: if should_upload {
                    Some(image.pixels.clone())
                } else {
                    None
                },
                sampled_upload_state_key: if should_upload {
                    Some(self.source_handle.key())
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

pub(crate) fn compute_image_mapping(
    fit: ImageFit,
    source_w: f32,
    source_h: f32,
    dest_w: f32,
    dest_h: f32,
) -> ([f32; 4], [f32; 4]) {
    if source_w <= 0.0 || source_h <= 0.0 || dest_w <= 0.0 || dest_h <= 0.0 {
        return (
            [0.0, 0.0, dest_w.max(1.0), dest_h.max(1.0)],
            [0.0, 0.0, source_w.max(1.0), source_h.max(1.0)],
        );
    }
    match fit {
        ImageFit::Fill => ([0.0, 0.0, dest_w, dest_h], [0.0, 0.0, source_w, source_h]),
        ImageFit::Contain => {
            let scale = (dest_w / source_w).min(dest_h / source_h);
            let draw_w = (source_w * scale).max(1.0);
            let draw_h = (source_h * scale).max(1.0);
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
    use super::Image;
    use crate::Layout;
    use crate::view::ImageSource;
    use crate::view::base_component::{
        Element, ElementTrait, LayoutConstraints, LayoutPlacement, Layoutable,
    };
    use crate::view::test_support::{commit_child, commit_element, new_test_arena};
    use crate::{Length, ParsedValue, PropertyId, Style};

    fn rgba_source(width: u32, height: u32) -> ImageSource {
        ImageSource::Rgba {
            width,
            height,
            pixels: std::sync::Arc::<[u8]>::from(vec![255; (width * height * 4) as usize]),
        }
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
            ParsedValue::Flex(crate::flex().shrink(1.0)),
        );
        image.apply_style(image_style);

        let mut sibling = Element::new(0.0, 0.0, 120.0, 20.0);
        let mut sibling_style = Style::new();
        sibling_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(120.0)));
        sibling_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(20.0)));
        sibling_style.insert(
            PropertyId::Flex,
            ParsedValue::Flex(crate::flex().shrink(1.0)),
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
