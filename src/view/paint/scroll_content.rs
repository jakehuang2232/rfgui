#![allow(dead_code)] // E2A0 contract scaffold; production scene dispatch lands in E2A3.

use crate::view::base_component::TransformSurfaceGeometrySnapshot;
use crate::view::base_component::{RetainedSurfaceBounds, scroll_content_layer_stable_key};
use crate::view::compositor::property_tree::{
    ClipNodeSnapshot, ScrollNodeSnapshot, TransformNodeSnapshot,
};
use crate::view::frame_graph::PersistentTextureKey;
use crate::view::render_pass::texture_composite_pass::{
    TextureCompositeInput, TextureCompositeOutput, TextureCompositeParams, TextureCompositePass,
};

/// Frozen direct-composite geometry for one exact `ScrollContents ->
/// Transform` edge.  The child raster and UV remain offset-zero; the final
/// quad applies the transform once, then projects the complete 2D scroll
/// offset once.  The scrollport is final-composite scissor authority.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PreparedScrollTransformContentCompositeGeometry {
    source_bounds_bits: [u32; 4],
    transform: TransformNodeSnapshot,
    scroll: ScrollNodeSnapshot,
    contents_clip: ClipNodeSnapshot,
    params: TextureCompositeParams,
}

impl PreparedScrollTransformContentCompositeGeometry {
    pub(super) fn new(
        source_bounds: RetainedSurfaceBounds,
        transform_geometry: TransformSurfaceGeometrySnapshot,
        transform: TransformNodeSnapshot,
        scroll: ScrollNodeSnapshot,
        contents_clip: ClipNodeSnapshot,
    ) -> Option<Self> {
        let source_bounds_bits = [
            source_bounds.x.to_bits(),
            source_bounds.y.to_bits(),
            source_bounds.width.to_bits(),
            source_bounds.height.to_bits(),
        ];
        if source_bounds_bits
            != [
                transform_geometry.source_bounds.x.to_bits(),
                transform_geometry.source_bounds.y.to_bits(),
                transform_geometry.source_bounds.width.to_bits(),
                transform_geometry.source_bounds.height.to_bits(),
            ]
            || source_bounds.corner_radii.map(f32::to_bits) != [0.0_f32.to_bits(); 4]
            || transform.id.0 != transform.owner
            || transform.parent.is_some()
            || transform.generation == 0
            || transform.viewport_matrix.to_cols_array().map(f32::to_bits)
                != transform_geometry
                    .viewport_transform
                    .to_cols_array()
                    .map(f32::to_bits)
            || super::compiler::direct_translation_bits(transform.viewport_matrix).is_none()
            || transform_geometry.outer_scissor_rect.is_some()
            || !scroll.has_canonical_vertical_geometry_with_contents_clip(contents_clip)
            || scroll.owner != scroll.id.0
            || transform.owner == scroll.owner
        {
            return None;
        }
        let offset = [scroll.offset.x, scroll.offset.y];
        if offset.into_iter().any(|value| !value.is_finite()) {
            return None;
        }
        let mut quad_positions = transform_geometry.quad_positions;
        for point in &mut quad_positions {
            point[0] -= offset[0];
            point[1] -= offset[1];
            if point.iter().any(|value| !value.is_finite()) {
                return None;
            }
        }
        let min_x = quad_positions
            .iter()
            .map(|point| point[0])
            .fold(f32::INFINITY, f32::min);
        let min_y = quad_positions
            .iter()
            .map(|point| point[1])
            .fold(f32::INFINITY, f32::min);
        let max_x = quad_positions
            .iter()
            .map(|point| point[0])
            .fold(f32::NEG_INFINITY, f32::max);
        let max_y = quad_positions
            .iter()
            .map(|point| point[1])
            .fold(f32::NEG_INFINITY, f32::max);
        let bounds = [min_x, min_y, max_x - min_x, max_y - min_y];
        if bounds.iter().any(|value| !value.is_finite()) || bounds[2] <= 0.0 || bounds[3] <= 0.0 {
            return None;
        }
        Some(Self {
            source_bounds_bits,
            transform,
            scroll,
            contents_clip,
            params: TextureCompositeParams {
                bounds,
                quad_positions: Some(quad_positions),
                uv_bounds: Some(transform_geometry.uv_bounds),
                mask_uv_bounds: None,
                use_mask: false,
                source_is_premultiplied: true,
                opacity: 1.0,
                scissor_rect: Some(contents_clip.logical_scissor),
            },
        })
    }

    pub(crate) fn params(self) -> TextureCompositeParams {
        self.params
    }

    pub(crate) fn source_bounds_bits(self) -> [u32; 4] {
        self.source_bounds_bits
    }

    pub(crate) fn matches_inputs(
        self,
        transform: TransformNodeSnapshot,
        scroll: ScrollNodeSnapshot,
        contents_clip: ClipNodeSnapshot,
    ) -> bool {
        self.transform == transform && self.scroll == scroll && self.contents_clip == contents_clip
    }

    pub(crate) fn bitwise_eq(self, other: Self) -> bool {
        let params_equal = |left: TextureCompositeParams, right: TextureCompositeParams| {
            left.bounds.map(f32::to_bits) == right.bounds.map(f32::to_bits)
                && match (left.quad_positions, right.quad_positions) {
                    (Some(left), Some(right)) => {
                        left.map(|point| point.map(f32::to_bits))
                            == right.map(|point| point.map(f32::to_bits))
                    }
                    (None, None) => true,
                    _ => false,
                }
                && match (left.uv_bounds, right.uv_bounds) {
                    (Some(left), Some(right)) => left.map(f32::to_bits) == right.map(f32::to_bits),
                    (None, None) => true,
                    _ => false,
                }
                && match (left.mask_uv_bounds, right.mask_uv_bounds) {
                    (Some(left), Some(right)) => left.map(f32::to_bits) == right.map(f32::to_bits),
                    (None, None) => true,
                    _ => false,
                }
                && left.use_mask == right.use_mask
                && left.source_is_premultiplied == right.source_is_premultiplied
                && left.opacity.to_bits() == right.opacity.to_bits()
                && left.scissor_rect == right.scissor_rect
        };
        self.source_bounds_bits == other.source_bounds_bits
            && self.transform == other.transform
            && self.scroll == other.scroll
            && self.contents_clip == other.contents_clip
            && params_equal(self.params, other.params)
    }
}

/// Frozen compositor-only geometry for one offset-zero scroll-content raster.
///
/// This token intentionally owns no raster stamp. Scroll offset and contents
/// clip affect only the final texture composite, never content reuse.
/// E2A3 preparation must still bitwise-bind `zero_bounds` to the accepted
/// ScrollContent stamp source bounds and bind the sampled source handle to
/// that stamp's ScrollContent persistent key before graph mutation.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PreparedScrollContentCompositeGeometry {
    source_key: PersistentTextureKey,
    source_bounds_bits: [u32; 4],
    params: TextureCompositeParams,
}

impl PreparedScrollContentCompositeGeometry {
    pub(crate) fn from_validated_native_scroll_forest_content_stamp(
        stamp: &super::RetainedSurfaceRasterStamp,
        scroll: ScrollNodeSnapshot,
        contents_clip: ClipNodeSnapshot,
    ) -> Option<Self> {
        if !super::compiler::native_scroll_forest_content_raster_stamp_is_canonical(stamp)
            || stamp.identity.role != super::RetainedSurfaceRasterRole::ScrollContent
            || stamp.identity.stable_id == 0
            || stamp.identity.color_key != scroll_content_layer_stable_key(stamp.identity.stable_id)
            || !scroll.has_canonical_geometry_with_contents_clip_parent_ids(
                contents_clip,
                scroll.parent,
                contents_clip.parent,
            )
        {
            return None;
        }
        let zero_bounds = RetainedSurfaceBounds {
            x: scroll.layout_content_bounds_at_zero.x,
            y: scroll.layout_content_bounds_at_zero.y,
            width: scroll.layout_content_bounds_at_zero.width,
            height: scroll.layout_content_bounds_at_zero.height,
            corner_radii: [0.0; 4],
        };
        if stamp.target.source_bounds_bits
            != [
                zero_bounds.x.to_bits(),
                zero_bounds.y.to_bits(),
                zero_bounds.width.to_bits(),
                zero_bounds.height.to_bits(),
            ]
        {
            return None;
        }
        Self::from_parts(
            zero_bounds,
            [scroll.offset.x, scroll.offset.y],
            contents_clip.logical_scissor,
            stamp.identity.color_key,
        )
    }

    pub(crate) fn from_exact_transient_scroll(
        content_stable_id: u64,
        scroll: ScrollNodeSnapshot,
        contents_clip: ClipNodeSnapshot,
    ) -> Option<Self> {
        if content_stable_id == 0 || !scroll.is_canonical_with_ancestor_contents_clip(contents_clip)
        {
            return None;
        }
        Self::from_parts(
            RetainedSurfaceBounds {
                x: scroll.layout_content_bounds_at_zero.x,
                y: scroll.layout_content_bounds_at_zero.y,
                width: scroll.layout_content_bounds_at_zero.width,
                height: scroll.layout_content_bounds_at_zero.height,
                corner_radii: [0.0; 4],
            },
            [scroll.offset.x, scroll.offset.y],
            contents_clip.logical_scissor,
            scroll_content_layer_stable_key(content_stable_id),
        )
    }

    /// Builds compositor geometry only from the already validated content
    /// raster identity.  This atomically freezes the exact source key/bounds,
    /// current full 2D offset, and authoritative contents clip.
    pub(super) fn from_validated_content_stamp(
        stamp: &super::RetainedSurfaceRasterStamp,
        scroll: ScrollNodeSnapshot,
        contents_clip: ClipNodeSnapshot,
    ) -> Option<Self> {
        if !super::retained_surface_raster_stamp_is_canonical(stamp)
            || stamp.identity.role != super::RetainedSurfaceRasterRole::ScrollContent
            || stamp.identity.boundary_root == scroll.owner
            || stamp.identity.stable_id == 0
            || stamp.identity.color_key != scroll_content_layer_stable_key(stamp.identity.stable_id)
            || stamp.scroll_host.is_some()
            || !scroll.has_canonical_vertical_geometry_with_contents_clip(contents_clip)
            || scroll.owner != scroll.id.0
        {
            return None;
        }
        // ScrollContent stamp canonicality above accepts either the original
        // clip-free leaf grammar or C1's one compiler-sealed, parentless
        // TextArea ContentsClip.  Both are raster-local; compositor geometry
        // consumes only the outer scroll and outer contents clip here.
        let zero_bounds = RetainedSurfaceBounds {
            x: scroll.layout_content_bounds_at_zero.x,
            y: scroll.layout_content_bounds_at_zero.y,
            width: scroll.layout_content_bounds_at_zero.width,
            height: scroll.layout_content_bounds_at_zero.height,
            corner_radii: [0.0; 4],
        };
        let expected_bounds_bits = [
            zero_bounds.x.to_bits(),
            zero_bounds.y.to_bits(),
            zero_bounds.width.to_bits(),
            zero_bounds.height.to_bits(),
        ];
        if stamp.target.source_bounds_bits != expected_bounds_bits {
            return None;
        }
        Self::from_parts(
            zero_bounds,
            [scroll.offset.x, scroll.offset.y],
            contents_clip.logical_scissor,
            stamp.identity.color_key,
        )
    }

    pub(super) fn from_validated_scroll_content_effect_stamp(
        stamp: &super::RetainedSurfaceRasterStamp,
        scroll: ScrollNodeSnapshot,
        contents_clip: ClipNodeSnapshot,
        effect_contract: &super::EffectPropertySurfaceArtifactContract,
    ) -> Option<Self> {
        if !super::compiler::scroll_content_effect_receiver_raster_stamp_validates_contract(
            stamp,
            stamp.identity.boundary_root,
            stamp.identity.stable_id,
            effect_contract,
        ) || stamp.identity.boundary_root == scroll.owner
            || !scroll.has_canonical_vertical_geometry_with_contents_clip(contents_clip)
            || scroll.owner != scroll.id.0
        {
            return None;
        }
        let zero_bounds = RetainedSurfaceBounds {
            x: scroll.layout_content_bounds_at_zero.x,
            y: scroll.layout_content_bounds_at_zero.y,
            width: scroll.layout_content_bounds_at_zero.width,
            height: scroll.layout_content_bounds_at_zero.height,
            corner_radii: [0.0; 4],
        };
        let expected_bounds_bits = [
            zero_bounds.x.to_bits(),
            zero_bounds.y.to_bits(),
            zero_bounds.width.to_bits(),
            zero_bounds.height.to_bits(),
        ];
        if stamp.target.source_bounds_bits != expected_bounds_bits {
            return None;
        }
        Self::from_parts(
            zero_bounds,
            [scroll.offset.x, scroll.offset.y],
            contents_clip.logical_scissor,
            stamp.identity.color_key,
        )
    }

    #[cfg(test)]
    pub(crate) fn new(
        zero_bounds: RetainedSurfaceBounds,
        offset: [f32; 2],
        contents_clip: [u32; 4],
    ) -> Option<Self> {
        Self::from_parts(
            zero_bounds,
            offset,
            contents_clip,
            scroll_content_layer_stable_key(1),
        )
    }

    fn from_parts(
        zero_bounds: RetainedSurfaceBounds,
        offset: [f32; 2],
        contents_clip: [u32; 4],
        source_key: PersistentTextureKey,
    ) -> Option<Self> {
        let [x, y, width, height] = [
            zero_bounds.x,
            zero_bounds.y,
            zero_bounds.width,
            zero_bounds.height,
        ];
        if ![x, y, width, height, offset[0], offset[1]]
            .into_iter()
            .all(f32::is_finite)
            || x < 0.0
            || y < 0.0
            || width <= 0.0
            || height <= 0.0
            || zero_bounds.corner_radii.map(f32::to_bits) != [0.0_f32.to_bits(); 4]
            || !(x + width).is_finite()
            || !(y + height).is_finite()
            || contents_clip[2] == 0
            || contents_clip[3] == 0
            || contents_clip[0].checked_add(contents_clip[2]).is_none()
            || contents_clip[1].checked_add(contents_clip[3]).is_none()
        {
            return None;
        }
        let destination = [x - offset[0], y - offset[1], width, height];
        if !destination.iter().all(|value| value.is_finite()) {
            return None;
        }
        Some(Self {
            source_key,
            source_bounds_bits: [x.to_bits(), y.to_bits(), width.to_bits(), height.to_bits()],
            params: TextureCompositeParams {
                bounds: destination,
                quad_positions: None,
                uv_bounds: Some([x, y, width, height]),
                mask_uv_bounds: None,
                use_mask: false,
                source_is_premultiplied: true,
                opacity: 1.0,
                scissor_rect: Some(contents_clip),
            },
        })
    }

    pub(crate) fn source_key(self) -> PersistentTextureKey {
        self.source_key
    }

    pub(crate) fn source_bounds_bits(self) -> [u32; 4] {
        self.source_bounds_bits
    }

    pub(crate) fn texture_composite_params(self) -> TextureCompositeParams {
        self.params
    }

    pub(crate) fn into_texture_composite_pass(
        self,
        input: TextureCompositeInput,
        output: TextureCompositeOutput,
    ) -> TextureCompositePass {
        TextureCompositePass::new(self.params, input, output)
    }
}

/// Frozen compositor geometry for one tile of an offset-zero scroll-content
/// raster. The raster gutter is sampled but only the non-overlapping interior
/// is composited.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PreparedScrollContentTileCompositeGeometry {
    source_key: PersistentTextureKey,
    raster_bounds: [u32; 4],
    interior_bounds: [u32; 4],
    params: TextureCompositeParams,
}

impl PreparedScrollContentTileCompositeGeometry {
    pub(crate) fn from_validated_tile_stamp(
        stamp: &super::RetainedSurfaceRasterStamp,
        scroll: ScrollNodeSnapshot,
        contents_clip: ClipNodeSnapshot,
    ) -> Option<Self> {
        if !super::retained_surface_raster_stamp_is_canonical(stamp)
            || stamp.identity.role != super::RetainedSurfaceRasterRole::ScrollContent
            || stamp.identity.boundary_root == scroll.owner
            || stamp.scroll_host.is_some()
            || !stamp.clip_nodes.is_empty()
            || !scroll.has_canonical_vertical_geometry_with_contents_clip(contents_clip)
        {
            return None;
        }
        let tile = stamp.identity.scroll_content_tile?;
        let expected_content = [
            scroll.layout_content_bounds_at_zero.x.to_bits(),
            scroll.layout_content_bounds_at_zero.y.to_bits(),
            scroll.layout_content_bounds_at_zero.width.to_bits(),
            scroll.layout_content_bounds_at_zero.height.to_bits(),
        ];
        if expected_content != tile.content_bounds.map(|value| (value as f32).to_bits())
            || stamp.target.source_bounds_bits
                != tile.bounds.raster.map(|value| (value as f32).to_bits())
        {
            return None;
        }
        let [x, y, width, height] = tile.bounds.interior.map(|value| value as f32);
        let destination = [x - scroll.offset.x, y - scroll.offset.y, width, height];
        if !destination.iter().all(|value| value.is_finite()) {
            return None;
        }
        Some(Self {
            source_key: stamp.identity.color_key,
            raster_bounds: tile.bounds.raster,
            interior_bounds: tile.bounds.interior,
            params: TextureCompositeParams {
                bounds: destination,
                quad_positions: None,
                uv_bounds: Some([x, y, width, height]),
                mask_uv_bounds: None,
                use_mask: false,
                source_is_premultiplied: true,
                opacity: 1.0,
                scissor_rect: Some(contents_clip.logical_scissor),
            },
        })
    }

    pub(crate) fn source_key(self) -> PersistentTextureKey {
        self.source_key
    }

    pub(crate) fn raster_bounds(self) -> [u32; 4] {
        self.raster_bounds
    }

    pub(crate) fn interior_bounds(self) -> [u32; 4] {
        self.interior_bounds
    }

    pub(crate) fn texture_composite_params(self) -> TextureCompositeParams {
        self.params
    }

    pub(crate) fn into_texture_composite_pass(
        self,
        input: TextureCompositeInput,
        output: TextureCompositeOutput,
    ) -> TextureCompositePass {
        TextureCompositePass::new(self.params, input, output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use crate::style::{Layout, ParsedValue, PropertyId, ScrollDirection, Style};
    use crate::view::base_component::{
        DirtyPassMask, Element, ElementTrait, EventTarget, Rect, ScrollbarInteractionWitness,
        ScrollbarOverlayWitness, ScrollbarPaintStateWitness, Size, UiBuildContext,
        persistent_target_texture_descriptors, scroll_content_layer_stable_key,
        scroll_content_tile_layer_stable_key, texture_desc_for_logical_bounds,
    };
    use crate::view::compositor::PropertyTrees;
    use crate::view::compositor::property_tree::{ClipNodeId, ClipNodeRole, ScrollNodeId};
    use crate::view::frame_graph::FrameGraph;
    use crate::view::node_arena::{Node, NodeArena};
    use crate::view::paint::{
        PaintOwnerSnapshot, PaintPayloadIdentity, PreparedScrollbarOverlayOp,
        RetainedSurfaceArtifactSpanStamp, RetainedSurfaceChunkStamp, RetainedSurfaceCompileAction,
        RetainedSurfaceRasterInputs, RetainedSurfaceRasterStepStamp, ScrollContentTileBounds,
        ScrollContentTileIndex, ScrollContentTileRasterIdentity,
        validated_scroll_content_raster_stamp, validated_scroll_content_tile_raster_stamp,
    };
    use crate::view::render_pass::composite_layer_pass::CompositeLayerPass;
    use crate::view::render_pass::texture_composite_pass::{
        TextureCompositeInput, TextureCompositeOutput, TextureCompositePass,
        TextureCompositeSourceIn,
    };

    fn bounds() -> RetainedSurfaceBounds {
        RetainedSurfaceBounds {
            x: 10.0,
            y: 20.0,
            width: 300.0,
            height: 900.0,
            corner_radii: [0.0; 4],
        }
    }

    #[test]
    fn zero_offset_keeps_destination_and_offset_zero_uv_distinct() {
        let params =
            PreparedScrollContentCompositeGeometry::new(bounds(), [0.0, 0.0], [10, 20, 300, 200])
                .unwrap()
                .texture_composite_params();
        assert_eq!(
            params.bounds.map(f32::to_bits),
            [10.0, 20.0, 300.0, 900.0].map(f32::to_bits)
        );
        assert_eq!(
            params.uv_bounds.unwrap().map(f32::to_bits),
            [10.0, 20.0, 300.0, 900.0].map(f32::to_bits)
        );
        assert_eq!(params.scissor_rect, Some([10, 20, 300, 200]));
        assert!(params.source_is_premultiplied);
        assert!(!params.use_mask);
        assert!(params.quad_positions.is_none());
        assert!(params.mask_uv_bounds.is_none());
    }

    #[test]
    fn full_two_dimensional_offset_moves_only_destination() {
        let params =
            PreparedScrollContentCompositeGeometry::new(bounds(), [3.5, 47.25], [10, 20, 300, 200])
                .unwrap()
                .texture_composite_params();
        assert_eq!(
            params.bounds.map(f32::to_bits),
            [6.5, -27.25, 300.0, 900.0].map(f32::to_bits)
        );
        assert_eq!(
            params.uv_bounds.unwrap().map(f32::to_bits),
            [10.0, 20.0, 300.0, 900.0].map(f32::to_bits)
        );
        assert_eq!(params.scissor_rect, Some([10, 20, 300, 200]));
    }

    #[test]
    fn nonzero_single_axis_offset_does_not_mask_or_shift_uv() {
        let params =
            PreparedScrollContentCompositeGeometry::new(bounds(), [0.0, 47.25], [10, 20, 300, 200])
                .unwrap()
                .texture_composite_params();
        assert_eq!(
            params.bounds.map(f32::to_bits),
            [10.0, -27.25, 300.0, 900.0].map(f32::to_bits)
        );
        assert_eq!(
            params.uv_bounds.unwrap().map(f32::to_bits),
            [10.0, 20.0, 300.0, 900.0].map(f32::to_bits)
        );
    }

    #[test]
    fn prepared_geometry_builds_only_a_texture_composite_pass() {
        let prepared =
            PreparedScrollContentCompositeGeometry::new(bounds(), [3.5, 47.25], [10, 20, 300, 200])
                .unwrap();
        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(800, 600, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
        let source = ctx.allocate_target(&mut graph);
        let output = ctx.allocate_target(&mut graph);
        let pass = prepared.into_texture_composite_pass(
            TextureCompositeInput::from_render_target(
                TextureCompositeSourceIn::with_handle(source.handle().unwrap()),
                Default::default(),
                ctx.graphics_pass_context(),
            ),
            TextureCompositeOutput {
                render_target: output,
            },
        );
        graph.add_graphics_pass(pass);
        assert_eq!(
            graph.test_graphics_passes::<TextureCompositePass>().len(),
            1
        );
        assert!(
            graph
                .test_graphics_passes::<CompositeLayerPass>()
                .is_empty()
        );
    }

    #[test]
    fn invalid_source_offset_or_clip_fails_closed() {
        let mut invalid_bounds = bounds();
        invalid_bounds.width = f32::NAN;
        assert!(
            PreparedScrollContentCompositeGeometry::new(
                invalid_bounds,
                [0.0, 0.0],
                [10, 20, 300, 200]
            )
            .is_none()
        );
        assert!(
            PreparedScrollContentCompositeGeometry::new(
                bounds(),
                [f32::INFINITY, 0.0],
                [10, 20, 300, 200]
            )
            .is_none()
        );
        assert!(
            PreparedScrollContentCompositeGeometry::new(bounds(), [0.0, 0.0], [10, 20, 0, 200])
                .is_none()
        );
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct ExternalScrollObservation {
        offset_bits: [u32; 2],
        clip: [u32; 4],
        scrollbar_alpha_bits: u32,
    }

    fn try_content_stamp(
        root: crate::view::node_arena::NodeKey,
        stable_id: u64,
        width: f32,
        role: crate::view::paint::PaintChunkRole,
        payload_identity: PaintPayloadIdentity,
    ) -> Option<crate::view::paint::RetainedSurfaceRasterStamp> {
        let source_bounds = RetainedSurfaceBounds {
            x: 10.0,
            y: 20.0,
            width,
            height: 900.0,
            corner_radii: [0.0; 4],
        };
        let key = scroll_content_layer_stable_key(stable_id);
        let color = texture_desc_for_logical_bounds(
            source_bounds,
            1.0,
            None,
            wgpu::TextureFormat::Bgra8UnormSrgb,
        );
        let (color, depth) = persistent_target_texture_descriptors(color, key);
        let bounds_bits = [
            source_bounds.x.to_bits(),
            source_bounds.y.to_bits(),
            source_bounds.width.to_bits(),
            source_bounds.height.to_bits(),
        ];
        let chunk = RetainedSurfaceChunkStamp {
            id: crate::view::paint::PaintChunkId {
                owner: root,
                scope: crate::view::paint::PaintPropertyScope::SelfPaint,
                phase: crate::view::paint::PaintNodePhase::BeforeChildren,
                slot: 0,
                role,
            },
            owner: root,
            bounds_bits,
            clip: None,
            non_boundary_self_paint_revision: None,
            topology_revision: 1,
            non_boundary_composite_revision: None,
            payload_identity,
            op_count: 1,
        };
        validated_scroll_content_raster_stamp(
            root,
            stable_id,
            RetainedSurfaceRasterInputs {
                color,
                depth,
                scale_factor_bits: 1.0_f32.to_bits(),
                source_bounds_bits: bounds_bits,
            },
            RetainedSurfaceArtifactSpanStamp {
                step_index: 0,
                owner_topology: vec![PaintOwnerSnapshot {
                    owner: root,
                    parent: None,
                }],
                clip_nodes: Vec::new(),
                chunks: vec![chunk],
                op_count: 1,
                opaque_order_span: 0..1,
                scroll_placement_normalized_owners: Vec::new(),
            },
            0..1,
        )
    }

    fn content_stamp(
        root: crate::view::node_arena::NodeKey,
        stable_id: u64,
        width: f32,
        payload_identity: PaintPayloadIdentity,
    ) -> crate::view::paint::RetainedSurfaceRasterStamp {
        try_content_stamp(
            root,
            stable_id,
            width,
            crate::view::paint::PaintChunkRole::SelfDecoration,
            payload_identity,
        )
        .unwrap()
    }

    fn tile_stamp(
        root: crate::view::node_arena::NodeKey,
        stable_id: u64,
        index: ScrollContentTileIndex,
        payload_identity: PaintPayloadIdentity,
    ) -> Option<crate::view::paint::RetainedSurfaceRasterStamp> {
        let base = content_stamp(root, stable_id, 300.0, payload_identity);
        let RetainedSurfaceRasterStepStamp::ArtifactSpan(mut span) = base.ordered_steps[0].clone()
        else {
            unreachable!()
        };
        let content_bounds = [0, 0, 300, 900];
        span.chunks[0].bounds_bits = content_bounds.map(|value| (value as f32).to_bits());
        let tile_bounds = ScrollContentTileBounds::for_index(content_bounds, 128, 1, index)?;
        let tile =
            ScrollContentTileRasterIdentity::new(index, content_bounds, tile_bounds, 128, 1)?;
        let [x, y, width, height] = tile_bounds.raster;
        let raster_bounds = RetainedSurfaceBounds {
            x: x as f32,
            y: y as f32,
            width: width as f32,
            height: height as f32,
            corner_radii: [0.0; 4],
        };
        let key = scroll_content_tile_layer_stable_key(stable_id, index.column, index.row)?;
        let color = texture_desc_for_logical_bounds(
            raster_bounds,
            1.0,
            None,
            wgpu::TextureFormat::Bgra8UnormSrgb,
        );
        let (color, depth) = persistent_target_texture_descriptors(color, key);
        validated_scroll_content_tile_raster_stamp(
            root,
            stable_id,
            tile,
            RetainedSurfaceRasterInputs {
                color,
                depth,
                scale_factor_bits: 1.0_f32.to_bits(),
                source_bounds_bits: tile_bounds.raster.map(|value| (value as f32).to_bits()),
            },
            span,
            0..1,
        )
    }

    fn scroll_fixture(
        offset: [f32; 2],
    ) -> (
        NodeArena,
        crate::view::node_arena::NodeKey,
        crate::view::node_arena::NodeKey,
        crate::view::compositor::property_tree::ScrollNodeSnapshot,
        crate::view::compositor::property_tree::ClipNodeSnapshot,
    ) {
        let mut arena = NodeArena::new();
        let root = arena.insert(Node::new(Box::new(Element::new_with_id(
            71_001, 0.0, 0.0, 100.0, 200.0,
        ))));
        let child = arena.insert(Node::new(Box::new(Element::new_with_id(
            71_002, -offset[0], -offset[1], 300.0, 900.0,
        ))));
        arena.set_parent(child, Some(root));
        arena.push_child(root, child);
        let mut style = Style::new();
        style.insert(
            PropertyId::ScrollDirection,
            ParsedValue::ScrollDirection(ScrollDirection::Vertical),
        );
        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        {
            let mut root_node = arena.get_mut(root).unwrap();
            let root_element = root_node
                .element
                .as_any_mut()
                .downcast_mut::<Element>()
                .unwrap();
            root_element.apply_style(style);
            root_element.layout_state.content_size = Size {
                width: 300.0,
                height: 900.0,
            };
            root_element.set_scroll_offset((offset[0], offset[1]));
            root_element
                .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
        }
        arena
            .get_mut(child)
            .unwrap()
            .element
            .clear_local_dirty_flags(DirtyPassMask::LAYOUT.union(DirtyPassMask::PLACEMENT));
        arena.refresh_subtree_dirty_cache(root);
        let mut properties = PropertyTrees::default();
        properties.sync(&arena, &[root]);
        assert!(
            properties.validation_errors.is_empty(),
            "{:?}",
            properties.validation_errors
        );
        let scroll = properties.scroll_snapshot_for(ScrollNodeId(root)).unwrap();
        let clip = properties
            .clip_snapshot_for(Some(ClipNodeId {
                owner: root,
                role: ClipNodeRole::ContentsClip,
            }))
            .unwrap()[0];
        (arena, root, child, scroll, clip)
    }

    #[test]
    fn external_scroll_clip_and_scrollbar_drift_do_not_enter_content_stamp() {
        let first_observation = ExternalScrollObservation {
            offset_bits: [0.0_f32.to_bits(), 20.0_f32.to_bits()],
            clip: [10, 20, 300, 200],
            scrollbar_alpha_bits: 1.0_f32.to_bits(),
        };
        let second_observation = ExternalScrollObservation {
            offset_bits: [7.5_f32.to_bits(), 411.25_f32.to_bits()],
            clip: [12, 24, 280, 180],
            scrollbar_alpha_bits: 0.35_f32.to_bits(),
        };
        assert_ne!(first_observation, second_observation);

        let mut slots = slotmap::SlotMap::<crate::view::node_arena::NodeKey, ()>::with_key();
        let child = slots.insert(());
        let payload = PaintPayloadIdentity::PreparedRects(Arc::from([]));
        let resident = content_stamp(child, 7001, 300.0, payload.clone());
        let candidate = content_stamp(child, 7001, 300.0, payload);
        assert_eq!(resident, candidate);
        assert!(resident.scroll_host.is_none());
        assert!(resident.clip_nodes.is_empty());
        assert!(resident.chunks.iter().all(|chunk| chunk.clip.is_none()));
        assert_eq!(
            crate::view::viewport::retained_surface_compile_action_against_resident_for_test(
                resident, &candidate,
            ),
            RetainedSurfaceCompileAction::Reuse
        );
    }

    #[test]
    fn content_payload_or_target_drift_rerasterizes() {
        let mut slots = slotmap::SlotMap::<crate::view::node_arena::NodeKey, ()>::with_key();
        let child = slots.insert(());
        let resident = content_stamp(child, 7001, 300.0, PaintPayloadIdentity::None);
        let payload_changed = content_stamp(
            child,
            7001,
            300.0,
            PaintPayloadIdentity::PreparedRects(Arc::from([])),
        );
        let target_changed = content_stamp(child, 7001, 301.0, PaintPayloadIdentity::None);
        assert_eq!(
            crate::view::viewport::retained_surface_compile_action_against_resident_for_test(
                resident.clone(),
                &payload_changed,
            ),
            RetainedSurfaceCompileAction::Reraster
        );
        assert_eq!(
            crate::view::viewport::retained_surface_compile_action_against_resident_for_test(
                resident,
                &target_changed,
            ),
            RetainedSurfaceCompileAction::Reraster
        );
    }

    #[test]
    fn tile_stamp_is_structural_and_excludes_scroll_clip_and_overlay_state() {
        let mut slots = slotmap::SlotMap::<crate::view::node_arena::NodeKey, ()>::with_key();
        let child = slots.insert(());
        let index = ScrollContentTileIndex { column: 1, row: 2 };
        let first = tile_stamp(child, 7001, index, PaintPayloadIdentity::None).unwrap();
        let second = tile_stamp(child, 7001, index, PaintPayloadIdentity::None).unwrap();
        assert_eq!(first, second);
        assert!(first.scroll_host.is_none());
        assert!(first.clip_nodes.is_empty());
        assert!(first.chunks.iter().all(|chunk| {
            chunk.clip.is_none()
                && chunk.id.role != crate::view::paint::PaintChunkRole::ScrollbarOverlay
        }));
        assert_eq!(
            crate::view::viewport::retained_surface_compile_action_against_resident_for_test(
                first.clone(),
                &second,
            ),
            RetainedSurfaceCompileAction::Reuse
        );

        let adjacent = tile_stamp(
            child,
            7001,
            ScrollContentTileIndex { column: 0, row: 2 },
            PaintPayloadIdentity::None,
        )
        .unwrap();
        assert_ne!(first.identity.color_key, adjacent.identity.color_key);
        assert_ne!(
            first.identity.resident_key(),
            adjacent.identity.resident_key()
        );
    }

    #[test]
    fn tile_composite_binds_gutter_interior_full_offset_and_contents_clip() {
        let (_arena, _root, child, scroll, clip) = scroll_fixture([3.5, 47.25]);
        let stamp = tile_stamp(
            child,
            71_002,
            ScrollContentTileIndex { column: 0, row: 1 },
            PaintPayloadIdentity::None,
        )
        .unwrap();
        let prepared = PreparedScrollContentTileCompositeGeometry::from_validated_tile_stamp(
            &stamp, scroll, clip,
        )
        .unwrap();
        assert_eq!(prepared.source_key(), stamp.identity.color_key);
        assert_eq!(prepared.raster_bounds(), [0, 127, 129, 130]);
        assert_eq!(prepared.interior_bounds(), [0, 128, 128, 128]);
        let params = prepared.texture_composite_params();
        assert_eq!(
            params.bounds.map(f32::to_bits),
            [-3.5, 80.75, 128.0, 128.0].map(f32::to_bits)
        );
        assert_eq!(
            params.uv_bounds.unwrap().map(f32::to_bits),
            [0.0, 128.0, 128.0, 128.0].map(f32::to_bits)
        );
        assert_eq!(params.scissor_rect, Some(clip.logical_scissor));
        assert!(params.source_is_premultiplied);
        assert!(!params.use_mask);

        let changed_offset_stamp = stamp.clone();
        let (_arena, _root, _child, changed_scroll, changed_clip) = scroll_fixture([9.25, 61.5]);
        let changed = PreparedScrollContentTileCompositeGeometry::from_validated_tile_stamp(
            &changed_offset_stamp,
            changed_scroll,
            changed_clip,
        )
        .unwrap();
        assert_eq!(stamp, changed_offset_stamp);
        assert_ne!(
            params.bounds.map(f32::to_bits),
            changed.texture_composite_params().bounds.map(f32::to_bits)
        );
    }

    #[test]
    fn malicious_scrollbar_role_is_rejected_with_benign_payload() {
        let mut slots = slotmap::SlotMap::<crate::view::node_arena::NodeKey, ()>::with_key();
        let child = slots.insert(());
        assert!(
            try_content_stamp(
                child,
                7001,
                300.0,
                crate::view::paint::PaintChunkRole::ScrollbarOverlay,
                PaintPayloadIdentity::PreparedRects(Arc::from([])),
            )
            .is_none()
        );
    }

    #[test]
    fn typed_scrollbar_payload_is_rejected_under_content_role() {
        let overlay = PreparedScrollbarOverlayOp::from_vertical_witness(ScrollbarOverlayWitness {
            vertical_track: Some(Rect {
                x: 100.0,
                y: 20.0,
                width: 6.0,
                height: 180.0,
            }),
            vertical_thumb: Some(Rect {
                x: 100.0,
                y: 40.0,
                width: 6.0,
                height: 48.0,
            }),
            horizontal_track: None,
            horizontal_thumb: None,
            interaction: ScrollbarInteractionWitness {
                hovered: true,
                dragging_axis: None,
                has_interaction_timestamp: false,
            },
            paint_state: ScrollbarPaintStateWitness::OpaqueNow,
            sampled_alpha: 1.0,
            shadow_blur_radius: 4.0,
        })
        .unwrap();
        let typed_payload = PaintPayloadIdentity::prepared_scrollbar_overlay(&overlay);
        let mut slots = slotmap::SlotMap::<crate::view::node_arena::NodeKey, ()>::with_key();
        let child = slots.insert(());
        assert!(
            try_content_stamp(
                child,
                7001,
                300.0,
                crate::view::paint::PaintChunkRole::SelfDecoration,
                typed_payload,
            )
            .is_none()
        );
    }
}
