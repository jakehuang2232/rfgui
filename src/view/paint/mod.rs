//! Retained paint recording scaffold.
//!
//! The first slice deliberately records only side-effect-free leaf Element
//! decoration. Every other host remains on the existing Renderable path.

mod artifact;
mod compiler;
mod coverage_manifest;
mod frame_plan;
mod frame_recorder;
mod recorder;
mod retained_surface_executor;
mod scroll_content;
mod scroll_scene;
mod scroll_tiles;

#[allow(unused_imports)]
pub(crate) use artifact::{
    ConsumedAncestorEffectWitness, ConsumedAncestorProperty, ConsumedAncestorPropertyStackWitness,
    ConsumedAncestorScrollContentsWitness, ConsumedAncestorTransformWitness, DrawRectOp,
    EffectPropertyContentWitness, EffectPropertySurfaceArtifactContract, PaintArtifact,
    PaintArtifactTarget, PaintBakedScrollHostWitness, PaintChunk, PaintChunkId, PaintChunkMetadata,
    PaintChunkRole, PaintContentRevision, PaintDeferredViewportSelfClipWitness,
    PaintNestedScrollContentWitness, PaintNodePhase, PaintNodePlan, PaintOp, PaintOpacityAuthority,
    PaintOwnerSnapshot, PaintPayloadIdentity, PaintPropertyScope, PaintRecordingContext,
    PaintScrollAtomicProjectionSelectionTextAreaSubtreeWitness,
    PaintScrollAtomicProjectionTextAreaRecorderWitness,
    PaintScrollAtomicProjectionTextAreaSubtreeWitness, PaintScrollContentWitness,
    PaintScrollFocusedAtomicProjectionTextAreaSubtreeWitness,
    PaintScrollInteractiveTextAreaSubtreeWitness, PaintScrollTextAreaSubtreeWitness,
    PaintTextPreeditWitness, PaintTextSelectionWitness, PaintTransformSurfaceWitness,
    PreparedImageIdentity, PreparedImageOp, PreparedInlineIfcDecorationDescriptor,
    PreparedInlineIfcDecorationIdentity, PreparedInlineIfcDecorationOp,
    PreparedScrollbarOverlayIdentity, PreparedScrollbarOverlayOp, PreparedShadowIdentity,
    PreparedShadowOp, PreparedSvgIdentity, PreparedSvgOp, PreparedTextIdentity, PreparedTextOp,
    RecordedRetainedTextAreaCaretOverlay, RetainedAtomicProjectionTextAreaChunkRasterSeal,
    RetainedTextAreaCaretOverlayIdentity, RetainedTextAreaCaretOverlayPaintIdentity,
    RetainedTextAreaGeneratedNodeKind, RetainedTextAreaGeneratedNodeSeal,
    RetainedTextAreaPreeditRasterSeal, has_canonical_paint_bounds, preedit_glyph_identity_is_exact,
    preedit_underline_identity_is_exact,
};
#[cfg(test)]
pub(crate) use compiler::validate_media_content_artifact_for_test;
#[allow(unused_imports)]
// C3 state/lifecycle landed before the C4 producer consumes every stamp type.
pub(crate) use compiler::{
    ArtifactCompileErrorKind, NestedSurfaceRasterDependency, PropertyEffectCompositeBasisStamp,
    PropertyEffectRasterIdentityInputs, RetainedAtomicProjectionTextAreaResidentRasterSeal,
    RetainedPropertySceneTransactionStamp, RetainedScrollHostRasterDependency,
    RetainedSurfaceArtifactSpanStamp, RetainedSurfaceChunkStamp, RetainedSurfaceCompileAction,
    RetainedSurfaceCompositeGeometryStamp, RetainedSurfaceRasterIdentity,
    RetainedSurfaceRasterInputs, RetainedSurfaceRasterRole, RetainedSurfaceRasterStamp,
    RetainedSurfaceRasterStepStamp, RetainedSurfaceResidentKey, RootEffectCompileAction,
    RootEffectRasterInputs, RootEffectRasterStamp, ValidatedEffectPropertySurfaceArtifact,
    emit_validated_effect_property_surface_artifact,
    property_effect_composite_geometry_stamp_is_canonical,
    property_effect_surface_raster_stamp_is_canonical_at_depth,
    property_effect_surface_raster_stamp_validates_contract_at_depth,
    retained_isolation_composite_geometry_stamp,
    retained_nested_isolation_composite_geometry_stamp,
    retained_property_effect_composite_geometry_stamp, retained_surface_composite_geometry_stamp,
    retained_surface_raster_stamp_is_canonical,
    retained_surface_raster_stamp_is_canonical_at_depth, try_compile_artifact,
    try_compile_root_effect_artifact, validate_effect_property_surface_artifact,
    validated_effect_property_surface_artifact_span_stamp,
    validated_isolation_surface_artifact_span_stamp,
    validated_property_effect_surface_raster_stamp, validated_retained_surface_artifact_span_stamp,
    validated_retained_surface_raster_stamp, validated_retained_surface_tree_raster_stamp,
    validated_root_effect_raster_stamp, validated_scroll_content_raster_stamp,
    validated_scroll_content_tile_raster_stamp, validated_scroll_host_artifact_span_stamp,
    validated_scroll_host_raster_stamp, validated_scroll_text_area_content_raster_stamp,
};
#[cfg(test)]
pub(crate) use compiler::{compile_artifact, take_artifact_compile_count};
#[allow(unused_imports)]
pub(crate) use coverage_manifest::{
    CoverageRecordingMode, PaintCoverageItem, PaintCoverageManifest, PaintCoverageStats,
    PaintCoverageValidationError, PlannedBoundary, PlannedBoundaryCutoutSet, PlannedBoundaryKind,
    record_coverage_manifest,
};
#[cfg(test)]
pub(crate) use frame_plan::tests::nested_scroll_plan_fixture;
#[allow(unused_imports)]
pub(crate) use frame_plan::{
    ArtifactSpanPlan, FramePaintPlan, FramePaintPlanError, FramePaintPlanRejection, PaintPlanStep,
    RetainedSurfacePlan, SurfaceKind, TransformSurfacePlanContext,
    plan_nested_scroll_scene_scaffold_with_context, plan_property_effect_scene_with_context,
    plan_single_root_isolation_surface, plan_single_root_scroll_host_surface,
    plan_single_root_transform_child_isolation_surface,
    plan_single_root_transform_child_isolation_surface_with_context,
    plan_single_root_transform_surface, plan_single_root_transform_surface_with_context,
    plan_transform_property_scene_with_context,
};
#[cfg(test)]
pub(crate) use frame_plan::{
    PropertySceneTopLevelSurfaceWitness, PropertySceneTransactionRootWitness,
    PropertySceneTransactionSurfaceKind, PropertySceneTransactionSurfaceWitness,
    PropertySceneTransactionWitness,
};
#[allow(unused_imports)]
pub(crate) use frame_recorder::{
    ForcedFrameArtifactError, FrameArtifactDebugBoundary, FrameArtifactDebugBoundaryKind,
    FrameArtifactEligibility, FrameArtifactFallbackReason, FrameArtifactRecordOutcome,
    RendererMode, record_clip_enabled_frame_artifact, record_frame_artifact,
    record_property_neutral_frame_artifact, record_root_group_opacity_frame_artifact,
};
#[cfg(test)]
pub(crate) fn canonical_manifest_matches_for_test(
    metadata: &PaintCoverageManifest,
    full: &PaintCoverageManifest,
) -> bool {
    frame_recorder::canonical_manifest_matches(metadata, full)
}
#[allow(unused_imports)]
pub(crate) use recorder::{LegacyPaintReason, PaintRecordOutcome, record_root};
#[cfg(test)]
pub(crate) use recorder::{note_full_artifact_record, take_full_artifact_record_count};
#[cfg(test)]
#[allow(unused_imports)]
pub(crate) use retained_surface_executor::{
    ForcedTransformSurfaceError, build_retained_property_scene_with_forced_pool_for_test,
    execute_forced_transform_surface_for_test, prepare_forced_retained_surface_stamp_for_test,
    prepare_retained_property_scene_stamps_for_test, prepare_retained_scroll_host_stamp_for_test,
};
#[allow(unused_imports)]
pub(crate) use retained_surface_executor::{
    PreparedRetainedPropertyScene, RetainedPropertySceneBuildOutcome,
    RetainedPropertySceneBuildTrace, RetainedPropertySceneTransaction, RetainedSurfaceBuildOutcome,
    RetainedSurfaceBuildTrace, RetainedSurfacePrepareError, RetainedSurfaceTreeBuildOutcome,
    build_retained_effect_tree_from_pool, build_retained_isolation_surface_from_pool,
    build_retained_scroll_host_surface_from_pool, build_retained_surface_from_pool,
    build_retained_surface_tree_from_pool, emit_prepared_retained_property_scene,
    prepare_retained_property_scene_from_pool,
};
#[allow(unused_imports)]
pub(crate) use scroll_content::{
    PreparedScrollContentCompositeGeometry, PreparedScrollContentTileCompositeGeometry,
    PreparedScrollTransformContentCompositeGeometry,
};
#[cfg(test)]
pub(crate) use scroll_scene::{
    NestedMediaLeafKind, NestedTextFallbackKind, build_scroll_scene_from_pool_with_budget_for_test,
    nested_scroll_unready_media_fixture_for_test, nested_scroll_unready_text_fixture_for_test,
};
#[allow(unused_imports)]
pub(crate) use scroll_scene::{
    PreparedNestedScrollReceiverGeometry, PreparedRetainedPropertyScrollForest,
    PropertyScrollScenePlan, PropertyScrollScenePlanError, RetainedPropertyScrollGroupSignature,
    RetainedPropertyScrollResidentGroup, RetainedPropertyScrollSceneBuildOutcome,
    RetainedPropertyScrollSceneBuildTrace, RetainedPropertyScrollScenePrepareError,
    RetainedPropertyScrollSceneTransaction, ScrollSceneBackingKind, ScrollSceneBuildOutcome,
    ScrollSceneBuildTrace, ScrollSceneFromLiveError, ScrollSceneSingleTextureBudget,
    ValidatedDirectScrollTransformTransaction, ValidatedEffectScrollSceneCheckpoint,
    ValidatedPropertyScrollScene, ValidatedTransformEffectScrollScene,
    ValidatedTransformScrollScene, build_scroll_scene_from_pool,
    emit_prepared_direct_scroll_transform_scene, emit_prepared_nested_scroll_scene,
    emit_prepared_retained_effect_scroll_scene, emit_prepared_retained_property_scroll_forest,
    emit_prepared_retained_transform_effect_scroll_scene,
    emit_prepared_retained_transform_scroll_scene, plan_and_prepare_nested_scroll_scene,
    plan_and_validate_direct_scroll_transform_scene,
    plan_and_validate_effect_scroll_scene_checkpoint, plan_and_validate_property_scroll_scene,
    plan_and_validate_transform_effect_scroll_scene, plan_and_validate_transform_scroll_scene,
    plan_property_scroll_scene_scaffold, prepare_direct_scroll_transform_scene_from_pool,
    prepare_nested_scroll_scene_from_pool, prepare_retained_effect_scroll_scene_from_pool,
    prepare_retained_property_scroll_forest_from_pool,
    prepare_retained_transform_effect_scroll_scene_from_pool,
    prepare_retained_transform_scroll_scene_from_pool, production_single_texture_budget,
};
#[allow(unused_imports)]
pub(crate) use scroll_tiles::{
    ScrollContentActiveTileManifest, ScrollContentTileBounds, ScrollContentTileIndex,
    ScrollContentTileRasterIdentity, ScrollContentTileSetTransactionStamp,
    plan_active_scroll_content_tiles_dpr1,
};

#[cfg(test)]
mod tests {
    #[cfg(not(target_arch = "wasm32"))]
    mod gpu_equivalence_tests;

    use std::any::Any;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use slotmap::Key;

    use crate::style::{
        Border, BorderRadius, BoxShadow, ClipMode, Color, ColorLike, Gradient, Layout, Length,
        Opacity, ParsedValue, Position, PropertyId, ScrollDirection, SideOrCorner, Style,
        TextAlign, TextWrap, Transform, Translate,
    };
    use crate::view::base_component::text_area::{TextAreaProjectionSegment, TextAreaTextRun};
    use crate::view::base_component::{
        BoxModelSnapshot, BuildState, CustomLeafPaintContext, CustomLeafPaintRecorder,
        CustomWrapperPaintContext, CustomWrapperPaintRecorder, DirtyFlags, Element, ElementTrait,
        EventTarget, Image, LayoutConstraints, LayoutPlacement, Layoutable,
        OwningInlineIfcRootWitnessDamage, Rect, Renderable, ShadowPaintBlocker,
        ShadowPaintRecordingCapability, Size, Svg, Text, TextArea, UiBuildContext,
    };
    use crate::view::compositor::property_tree::{
        ClipBehavior, ClipNodeId, ClipNodeRole, ClipNodeSnapshot, EffectNodeId, EffectNodeSnapshot,
        PropertyTreeState, TransformNodeId,
    };
    use crate::view::compositor::{PaintGenerationTracker, PropertyTrees};
    use crate::view::frame_graph::{FrameGraph, FrameGraphTestSnapshot, FramePassTestPayload};
    use crate::view::node_arena::{Node, NodeArena, NodeKey};
    use crate::view::render_pass::draw_rect_pass::{
        DrawRectInput, DrawRectOutput, DrawRectPass, RectPassParams, RectPassTestSnapshot,
        RectRenderMode,
    };
    use crate::view::test_support::{
        commit_child, commit_element, measure_and_place, new_test_arena,
    };
    use crate::view::{ImageSource, SvgSource};

    use super::*;

    pub(super) fn exact_isolation_fixture(
        opacity: f32,
    ) -> (NodeArena, NodeKey, PropertyTrees, PaintGenerationTracker) {
        let styled = |id, x, y, width, height, color| {
            let mut element = Element::new_with_id(id, x, y, width, height);
            let mut style = Style::new();
            style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
            style.insert(PropertyId::BackgroundColor, ParsedValue::color_like(color));
            element.apply_style(style);
            element
        };
        let mut arena = new_test_arena();
        let root = commit_element(
            &mut arena,
            Box::new(styled(
                0x9f_3001,
                8.0,
                7.0,
                120.0,
                90.0,
                Color::rgb(220, 40, 30),
            )),
        );
        commit_child(
            &mut arena,
            root,
            Box::new(styled(
                0x9f_3002,
                18.0,
                12.0,
                26.0,
                20.0,
                Color::rgb(20, 80, 230),
            )),
        );
        measure_and_place(
            &mut arena,
            root,
            LayoutConstraints {
                max_width: 160.0,
                max_height: 120.0,
                viewport_width: 160.0,
                viewport_height: 120.0,
                percent_base_width: Some(160.0),
                percent_base_height: Some(120.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 160.0,
                available_height: 120.0,
                viewport_width: 160.0,
                viewport_height: 120.0,
                percent_base_width: Some(160.0),
                percent_base_height: Some(120.0),
            },
        );
        crate::view::test_support::get_element_mut::<Element>(&arena, root).set_opacity(opacity);
        let mut properties = PropertyTrees::default();
        properties.sync(&arena, &[root]);
        let mut generations = PaintGenerationTracker::default();
        generations.sync(&arena, &[root], &properties);
        (arena, root, properties, generations)
    }

    fn constraints() -> (LayoutConstraints, LayoutPlacement) {
        (
            LayoutConstraints {
                max_width: 320.0,
                max_height: 240.0,
                viewport_width: 320.0,
                viewport_height: 240.0,
                percent_base_width: Some(320.0),
                percent_base_height: Some(240.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 320.0,
                available_height: 240.0,
                viewport_width: 320.0,
                viewport_height: 240.0,
                percent_base_width: Some(320.0),
                percent_base_height: Some(240.0),
            },
        )
    }

    fn leaf_element(id: u64, color: Color, opacity: f32, border: bool) -> Element {
        let mut element = Element::new_with_id(id, 10.25, 20.75, 80.0, 40.0);
        let mut style = Style::new();
        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        style.insert(PropertyId::BackgroundColor, ParsedValue::color_like(color));
        if border {
            style.set_border(Border::uniform(Length::px(3.0), &Color::hex("#102030")));
        }
        element.apply_style(style);
        element.set_opacity(opacity);
        element
    }

    fn gradient(start: &str, end: &str) -> Gradient {
        Gradient::linear(SideOrCorner::Right)
            .stop(Color::hex(start), Some(Length::percent(0.0)))
            .stop(Color::hex(end), Some(Length::percent(100.0)))
            .build()
    }

    fn apply_gradient_style(
        element: &mut Element,
        background_start: &str,
        background_end: &str,
        border_start: &str,
        border_end: &str,
    ) {
        let mut style = Style::new();
        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::rgb(20, 20, 20)),
        );
        style.set_border(Border::uniform(Length::px(3.0), &Color::hex("#102030")));
        style.set_background_image(gradient(background_start, background_end));
        style.set_border_image(gradient(border_start, border_end));
        element.apply_style(style);
    }

    fn sync_identity(
        arena: &NodeArena,
        roots: &[NodeKey],
    ) -> (PropertyTrees, PaintGenerationTracker) {
        let mut properties = PropertyTrees::default();
        properties.sync(arena, roots);
        let mut generations = PaintGenerationTracker::default();
        generations.sync(arena, roots, &properties);
        (properties, generations)
    }

    fn prepared_leaf(
        id: u64,
        color: Color,
        opacity: f32,
        border: bool,
    ) -> (NodeArena, NodeKey, PropertyTrees, PaintGenerationTracker) {
        let mut arena = new_test_arena();
        let root = commit_element(
            &mut arena,
            Box::new(leaf_element(id, color, opacity, border)),
        );
        let (measure, place) = constraints();
        measure_and_place(&mut arena, root, measure, place);
        let (properties, generations) = sync_identity(&arena, &[root]);
        (arena, root, properties, generations)
    }

    fn prepared_shadow_leaf(
        id: u64,
        opacity: f32,
        shadows: Vec<BoxShadow>,
        border: bool,
    ) -> (NodeArena, NodeKey, PropertyTrees, PaintGenerationTracker) {
        let mut element = Element::new_with_id(id, 10.25, 20.75, 80.0, 40.0);
        let mut style = Style::new();
        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::rgb(40, 80, 160)),
        );
        style.set_box_shadow(shadows);
        if border {
            style.set_border(Border::uniform(Length::px(3.0), &Color::hex("#102030")));
        }
        element.apply_style(style);
        element.set_opacity(opacity);
        let mut arena = new_test_arena();
        let root = commit_element(&mut arena, Box::new(element));
        let (measure, place) = constraints();
        measure_and_place(&mut arena, root, measure, place);
        let (properties, generations) = sync_identity(&arena, &[root]);
        (arena, root, properties, generations)
    }

    fn prepared_shadow_owner_tree(
        id: u64,
        opacity: f32,
    ) -> (
        NodeArena,
        NodeKey,
        NodeKey,
        NodeKey,
        PropertyTrees,
        PaintGenerationTracker,
    ) {
        let (mut arena, root, _, _) = prepared_shadow_leaf(id, opacity, two_outer_shadows(), true);
        let small_child = |id, color| {
            let mut child = Element::new_with_id(id, 0.0, 0.0, 8.0, 8.0);
            let mut style = Style::new();
            style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
            style.insert(PropertyId::Width, ParsedValue::Length(Length::px(8.0)));
            style.insert(PropertyId::Height, ParsedValue::Length(Length::px(8.0)));
            style.insert(PropertyId::BackgroundColor, ParsedValue::color_like(color));
            child.apply_style(style);
            child
        };
        let first = commit_child(
            &mut arena,
            root,
            Box::new(small_child(id + 1, Color::rgb(20, 180, 40))),
        );
        let second = commit_child(
            &mut arena,
            root,
            Box::new(small_child(id + 2, Color::rgb(180, 40, 120))),
        );
        let (measure, place) = constraints();
        measure_and_place(&mut arena, root, measure, place);
        let (properties, generations) = sync_identity(&arena, &[root]);
        (arena, root, first, second, properties, generations)
    }

    fn anchor_parent_self_clip_roots(opacity: f32, border: bool) -> (NodeArena, Vec<NodeKey>) {
        let mut arena = new_test_arena();
        let mut clipped = leaf_element(210, Color::rgb(220, 40, 30), opacity, border);
        let mut position = Style::new();
        position.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(10.25))
                    .top(Length::px(12.75))
                    .clip(ClipMode::AnchorParent),
            ),
        );
        clipped.apply_style(position);
        clipped.set_opacity(opacity);
        let clipped = commit_element(&mut arena, Box::new(clipped));
        let sibling = commit_element(
            &mut arena,
            Box::new(leaf_element(211, Color::rgb(30, 60, 220), 1.0, false)),
        );
        let (measure, place) = constraints();
        measure_and_place(&mut arena, clipped, measure, place);
        measure_and_place(&mut arena, sibling, measure, place);
        (arena, vec![clipped, sibling])
    }

    fn anchor_parent_self_clip_shadow_root() -> (NodeArena, Vec<NodeKey>) {
        let mut arena = new_test_arena();
        let mut clipped = leaf_element(212, Color::rgb(220, 40, 30), 1.0, true);
        let mut style = Style::new();
        style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(10.25))
                    .top(Length::px(12.75))
                    .clip(ClipMode::AnchorParent),
            ),
        );
        style.set_box_shadow(vec![
            BoxShadow::new()
                .color(Color::rgb(20, 40, 220))
                .offset_x(-3.0)
                .offset_y(4.5),
        ]);
        clipped.apply_style(style);
        let clipped = commit_element(&mut arena, Box::new(clipped));
        let (measure, place) = constraints();
        measure_and_place(&mut arena, clipped, measure, place);
        (arena, vec![clipped])
    }

    fn nested_anchor_parent_mixed_siblings(
        anchor_first: bool,
    ) -> (NodeArena, Vec<NodeKey>, NodeKey) {
        let mut arena = new_test_arena();
        let mut root = Element::new_with_id(220, 0.0, 0.0, 320.0, 240.0);
        let mut root_style = Style::new();
        root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        root.apply_style(root_style);
        let root = commit_element(&mut arena, Box::new(root));

        let mut anchor = leaf_element(221, Color::rgb(220, 30, 20), 1.0, false);
        let mut anchor_style = Style::new();
        anchor_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(30.0))
                    .top(Length::px(24.0))
                    .clip(ClipMode::AnchorParent),
            ),
        );
        anchor.apply_style(anchor_style);
        anchor.set_background_color_value(Color::rgb(220, 30, 20));
        anchor.set_opacity(1.0);

        let mut normal = leaf_element(222, Color::rgb(20, 40, 220), 1.0, false);
        let mut normal_style = Style::new();
        normal_style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(30.0))
                    .top(Length::px(24.0))
                    .clip(ClipMode::Parent),
            ),
        );
        normal.apply_style(normal_style);
        normal.set_background_color_value(Color::rgb(20, 40, 220));
        normal.set_opacity(1.0);

        let (anchor, normal) = if anchor_first {
            (
                commit_child(&mut arena, root, Box::new(anchor)),
                commit_child(&mut arena, root, Box::new(normal)),
            )
        } else {
            let normal = commit_child(&mut arena, root, Box::new(normal));
            let anchor = commit_child(&mut arena, root, Box::new(anchor));
            (anchor, normal)
        };
        let _ = normal;
        let (measure, place) = constraints();
        measure_and_place(&mut arena, root, measure, place);
        (arena, vec![root], anchor)
    }

    fn artifact_graph(
        arena: &NodeArena,
        root: NodeKey,
        properties: &PropertyTrees,
        generations: &PaintGenerationTracker,
    ) -> FrameGraph {
        let outcome = record_root(arena, root, properties, generations);
        let PaintRecordOutcome::Artifact(artifact) = outcome else {
            panic!("safe leaf should record an artifact: {outcome:?}");
        };
        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let target = ctx.allocate_target(&mut graph);
        ctx.set_current_target(target);
        let _ = compile_artifact(&artifact, &mut graph, ctx);
        graph
    }

    fn legacy_graph(mut arena: NodeArena, root: NodeKey) -> FrameGraph {
        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let target = ctx.allocate_target(&mut graph);
        ctx.set_current_target(target);
        let _ = arena
            .with_element_taken(root, |element, arena| element.build(&mut graph, arena, ctx))
            .expect("legacy root should build");
        graph
    }

    #[test]
    fn paint_artifact_leaf_fill_border_opacity_and_opaque_match_legacy() {
        for (opacity, expected_opaque) in [(1.0, true), (0.65, false)] {
            let (arena, root, properties, generations) =
                prepared_leaf(10, Color::rgb(220, 30, 40), opacity, true);
            let artifact = artifact_graph(&arena, root, &properties, &generations);

            let (legacy_arena, legacy_root, _, _) =
                prepared_leaf(10, Color::rgb(220, 30, 40), opacity, true);
            let legacy = legacy_graph(legacy_arena, legacy_root);

            assert_eq!(
                artifact.test_rect_pass_snapshots(),
                legacy.test_rect_pass_snapshots()
            );
            assert_eq!(
                artifact.test_rect_pass_snapshots()[0].opaque,
                expected_opaque
            );
            assert_eq!(
                artifact.test_rect_pass_snapshots()[0].opacity_bits,
                opacity.to_bits()
            );
            assert_eq!(artifact.test_rect_pass_snapshots().len(), 2);
        }
    }

    #[test]
    fn paint_artifact_chunk_identity_stays_stable_and_revision_tracks_paint() {
        let (arena, root, mut properties, mut generations) =
            prepared_leaf(11, Color::rgb(10, 20, 30), 1.0, false);
        let outcome = record_root(&arena, root, &properties, &generations);
        let PaintRecordOutcome::Artifact(first) = outcome else {
            panic!("safe leaf should record: {outcome:?}");
        };

        arena
            .get_mut(root)
            .expect("root exists")
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .expect("root is Element")
            .set_background_color_value(Color::rgb(40, 50, 60));
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
        let PaintRecordOutcome::Artifact(second) =
            record_root(&arena, root, &properties, &generations)
        else {
            panic!("safe leaf should still record");
        };

        assert_eq!(first.chunks[0].id, second.chunks[0].id);
        assert_ne!(
            first.chunks[0].content_revision,
            second.chunks[0].content_revision
        );
        assert_ne!(
            first.chunks[0].content_revision.self_paint_revision,
            second.chunks[0].content_revision.self_paint_revision
        );
        assert_eq!(first.chunks[0].properties, second.chunks[0].properties);
    }

    #[test]
    fn opacity_change_keeps_chunk_id_but_changes_baked_content_revision() {
        let (arena, root, mut properties, mut generations) =
            prepared_leaf(12, Color::rgb(10, 20, 30), 1.0, false);
        let PaintRecordOutcome::Artifact(first) =
            record_root(&arena, root, &properties, &generations)
        else {
            panic!("safe leaf should record");
        };

        arena
            .get_mut(root)
            .expect("root exists")
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .expect("root is Element")
            .set_opacity(0.4);
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
        let PaintRecordOutcome::Artifact(second) =
            record_root(&arena, root, &properties, &generations)
        else {
            panic!("safe leaf should still record");
        };

        assert_eq!(first.chunks[0].id, second.chunks[0].id);
        assert_eq!(
            first.chunks[0].content_revision.self_paint_revision,
            second.chunks[0].content_revision.self_paint_revision,
            "opacity remains excluded from the self-paint signature"
        );
        assert_ne!(
            first.chunks[0].content_revision.composite_revision,
            second.chunks[0].content_revision.composite_revision
        );
        assert_ne!(
            first.chunks[0].content_revision, second.chunks[0].content_revision,
            "opacity is still baked into DrawRectOp in this slice"
        );
    }

    #[test]
    fn resolved_gradient_changes_advance_self_and_content_revision() {
        let mut element = Element::new_with_id(13, 10.25, 20.75, 80.0, 40.0);
        apply_gradient_style(&mut element, "#ff0000", "#0000ff", "#ffffff", "#000000");
        let mut arena = new_test_arena();
        let root = commit_element(&mut arena, Box::new(element));
        let (measure, place) = constraints();
        measure_and_place(&mut arena, root, measure, place);
        let (mut properties, mut generations) = sync_identity(&arena, &[root]);
        let PaintRecordOutcome::Artifact(first) =
            record_root(&arena, root, &properties, &generations)
        else {
            panic!("gradient leaf should record");
        };

        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
        let PaintRecordOutcome::Artifact(unchanged) =
            record_root(&arena, root, &properties, &generations)
        else {
            panic!("unchanged gradient leaf should record");
        };
        assert_eq!(first.chunks[0].id, unchanged.chunks[0].id);
        assert_eq!(
            first.chunks[0].content_revision,
            unchanged.chunks[0].content_revision
        );

        {
            let mut node = arena.get_mut(root).expect("root exists");
            let element = node
                .element
                .as_any_mut()
                .downcast_mut::<Element>()
                .expect("root is Element");
            apply_gradient_style(element, "#00ff00", "#0000ff", "#ffffff", "#000000");
        }
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
        let PaintRecordOutcome::Artifact(background_changed) =
            record_root(&arena, root, &properties, &generations)
        else {
            panic!("background-gradient mutation should remain recordable");
        };
        assert_eq!(first.chunks[0].id, background_changed.chunks[0].id);
        assert_ne!(
            first.chunks[0].content_revision.self_paint_revision,
            background_changed.chunks[0]
                .content_revision
                .self_paint_revision
        );

        {
            let mut node = arena.get_mut(root).expect("root exists");
            let element = node
                .element
                .as_any_mut()
                .downcast_mut::<Element>()
                .expect("root is Element");
            apply_gradient_style(element, "#00ff00", "#0000ff", "#ff00ff", "#000000");
        }
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
        let PaintRecordOutcome::Artifact(border_changed) =
            record_root(&arena, root, &properties, &generations)
        else {
            panic!("border-gradient mutation should remain recordable");
        };
        assert_eq!(first.chunks[0].id, border_changed.chunks[0].id);
        assert_ne!(
            background_changed.chunks[0]
                .content_revision
                .self_paint_revision,
            border_changed.chunks[0]
                .content_revision
                .self_paint_revision
        );
        assert_ne!(
            background_changed.chunks[0].content_revision,
            border_changed.chunks[0].content_revision
        );
    }

    fn fallback_reason(element: Box<dyn ElementTrait>) -> LegacyPaintReason {
        let mut arena = new_test_arena();
        let root = commit_element(&mut arena, element);
        let (properties, generations) = sync_identity(&arena, &[root]);
        let PaintRecordOutcome::LegacySubtree(legacy) =
            record_root(&arena, root, &properties, &generations)
        else {
            panic!("host should remain legacy");
        };
        legacy.reason
    }

    #[test]
    fn resource_text_and_editable_hosts_default_to_legacy() {
        assert_eq!(
            fallback_reason(Box::new(Text::new(0.0, 0.0, 20.0, 20.0, "text"))),
            LegacyPaintReason::UnknownHost
        );
        assert_eq!(
            fallback_reason(Box::new(Image::new_with_id(
                20,
                ImageSource::Rgba {
                    width: 1,
                    height: 1,
                    pixels: Arc::from([255, 255, 255, 255]),
                },
            ))),
            LegacyPaintReason::UnknownHost
        );
        assert_eq!(
            fallback_reason(Box::new(Svg::new_with_id(
                21,
                SvgSource::Content("<svg xmlns='http://www.w3.org/2000/svg'/>".into()),
            ))),
            LegacyPaintReason::UnknownHost
        );
        assert_eq!(
            fallback_reason(Box::new(TextArea::with_stable_id(22))),
            LegacyPaintReason::UnknownHost
        );
    }

    fn prepared_image_fixture(
        pixels: Arc<[u8]>,
        fit: crate::view::ImageFit,
        sampling: crate::view::ImageSampling,
        opacity: f32,
    ) -> (NodeArena, Vec<NodeKey>) {
        let mut image = Image::new_with_id(
            24,
            ImageSource::Rgba {
                width: 2,
                height: 2,
                pixels,
            },
        );
        image.set_fit(fit);
        image.set_sampling(sampling);
        let mut style = Style::new();
        style.insert(PropertyId::Width, ParsedValue::Length(Length::px(48.0)));
        style.insert(PropertyId::Height, ParsedValue::Length(Length::px(32.0)));
        style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::rgb(20, 40, 60)),
        );
        style.set_border(Border::uniform(Length::px(4.0), &Color::rgb(180, 30, 20)));
        style.insert(
            PropertyId::Opacity,
            ParsedValue::Opacity(Opacity::new(opacity)),
        );
        style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(10.25))
                    .top(Length::px(12.75)),
            ),
        );
        image.apply_style(style);
        let mut arena = new_test_arena();
        let root = commit_element(&mut arena, Box::new(image));
        let (measure, place) = constraints();
        measure_and_place(&mut arena, root, measure, place);
        (arena, vec![root])
    }

    struct TransparentContentsClipParent {
        id: u64,
        opacity: f32,
        scissor: [u32; 4],
        children: Vec<NodeKey>,
    }

    impl Layoutable for TransparentContentsClipParent {
        fn measure(&mut self, _constraints: LayoutConstraints, _arena: &mut NodeArena) {}
        fn place(&mut self, _placement: LayoutPlacement, _arena: &mut NodeArena) {}
        fn measured_size(&self) -> (f32, f32) {
            (1.0, 1.0)
        }
        fn set_layout_width(&mut self, _width: f32) {}
        fn set_layout_height(&mut self, _height: f32) {}
    }

    impl EventTarget for TransparentContentsClipParent {}

    impl Renderable for TransparentContentsClipParent {
        fn build(
            &mut self,
            _graph: &mut FrameGraph,
            _arena: &mut NodeArena,
            ctx: UiBuildContext,
        ) -> BuildState {
            ctx.into_state()
        }
    }

    impl ElementTrait for TransparentContentsClipParent {
        fn stable_id(&self) -> u64 {
            self.id
        }

        fn box_model_snapshot(&self) -> BoxModelSnapshot {
            BoxModelSnapshot {
                node_id: self.id,
                parent_id: None,
                x: 0.0,
                y: 0.0,
                width: 1.0,
                height: 1.0,
                border_radius: 0.0,
                should_render: true,
            }
        }

        fn as_any(&self) -> &dyn Any {
            self
        }

        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }

        fn shadow_paint_recording_capability(
            &self,
            _arena: &NodeArena,
            _deferred_phase_root: bool,
            _recording_context: PaintRecordingContext,
        ) -> ShadowPaintRecordingCapability {
            ShadowPaintRecordingCapability::Transparent
        }

        fn contents_logical_scissor(&self) -> Option<[u32; 4]> {
            Some(self.scissor)
        }

        fn retained_paint_properties(
            &self,
        ) -> crate::view::base_component::RetainedPaintProperties {
            crate::view::base_component::RetainedPaintProperties {
                opacity: self.opacity,
                ..Default::default()
            }
        }

        fn children(&self) -> &[NodeKey] {
            &self.children
        }

        fn sync_children_mirror(&mut self, children: &[NodeKey]) {
            self.children.clear();
            self.children.extend_from_slice(children);
        }
    }

    fn root_opacity_contents_clip_fixture(
        scissor: [u32; 4],
    ) -> (
        NodeArena,
        NodeKey,
        NodeKey,
        PropertyTrees,
        PaintGenerationTracker,
    ) {
        let mut arena = new_test_arena();
        let root = commit_element(
            &mut arena,
            Box::new(TransparentContentsClipParent {
                id: 0x8c20,
                opacity: 0.5,
                scissor,
                children: Vec::new(),
            }),
        );
        let child = commit_child(
            &mut arena,
            root,
            Box::new(leaf_element(0x8c21, Color::rgb(30, 180, 90), 1.0, false)),
        );
        let (measure, place) = constraints();
        measure_and_place(&mut arena, child, measure, place);
        let (properties, generations) = sync_identity(&arena, &[root]);
        (arena, root, child, properties, generations)
    }

    fn bare_image_fixture(
        pixels: Arc<[u8]>,
        fit: crate::view::ImageFit,
        sampling: crate::view::ImageSampling,
        opacity: f32,
    ) -> (NodeArena, Vec<NodeKey>) {
        let mut image = Image::new_with_id(
            26,
            ImageSource::Rgba {
                width: 2,
                height: 2,
                pixels,
            },
        );
        image.set_fit(fit);
        image.set_sampling(sampling);
        let mut style = Style::new();
        style.insert(PropertyId::Width, ParsedValue::Length(Length::px(47.0)));
        style.insert(PropertyId::Height, ParsedValue::Length(Length::px(31.0)));
        style.insert(
            PropertyId::Opacity,
            ParsedValue::Opacity(Opacity::new(opacity)),
        );
        image.apply_style(style);
        let mut arena = new_test_arena();
        let root = commit_element(&mut arena, Box::new(image));
        let (measure, place) = constraints();
        measure_and_place(&mut arena, root, measure, place);
        (arena, vec![root])
    }

    #[test]
    fn prepared_image_fill_records_in_legacy_order_and_matches_strictly_after_arena_drop() {
        let pixels: Arc<[u8]> = Arc::from([
            255_u8, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 255, 255, 255, 0, 64,
        ]);
        let (artifact_arena, artifact_roots) = prepared_image_fixture(
            pixels.clone(),
            crate::view::ImageFit::Fill,
            crate::view::ImageSampling::Linear,
            0.65,
        );
        let (properties, generations) = sync_identity(&artifact_arena, &artifact_roots);
        let (artifact, eligibility) =
            whole_frame_artifact(&artifact_arena, &artifact_roots, &properties, &generations);
        assert!(eligibility.eligible);
        assert_eq!(artifact.chunks.len(), 1);
        assert_eq!(artifact.chunks[0].id.role, PaintChunkRole::ImageContent);
        assert!(matches!(
            artifact.ops.as_slice(),
            [
                PaintOp::DrawRect(_),
                PaintOp::DrawRect(_),
                PaintOp::PreparedImage(_)
            ]
        ));
        drop(artifact_arena);

        let (legacy_arena, legacy_roots) = prepared_image_fixture(
            pixels,
            crate::view::ImageFit::Fill,
            crate::view::ImageSampling::Linear,
            0.65,
        );
        let mut artifact_graph = compiled_whole_frame_graph(&artifact);
        let mut legacy_graph = legacy_roots_graph(legacy_arena, &legacy_roots);
        assert_eq!(
            strict_paint_snapshot(&mut artifact_graph, PaintParityConfig::default()),
            strict_paint_snapshot(&mut legacy_graph, PaintParityConfig::default())
        );
    }

    #[test]
    fn malformed_inline_image_falls_back_in_metadata_without_full_recording() {
        let mut image = Image::new_with_id(
            25,
            ImageSource::Rgba {
                width: 0,
                height: 2,
                pixels: Arc::from([]),
            },
        );
        let mut style = Style::new();
        style.insert(PropertyId::Width, ParsedValue::Length(Length::px(20.0)));
        style.insert(PropertyId::Height, ParsedValue::Length(Length::px(20.0)));
        image.apply_style(style);
        let mut arena = new_test_arena();
        let root = commit_element(&mut arena, Box::new(image));
        let (measure, place) = constraints();
        measure_and_place(&mut arena, root, measure, place);
        let (properties, generations) = sync_identity(&arena, &[root]);
        let _ = take_full_artifact_record_count();
        let FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility) =
            record_frame_artifact(
                &arena,
                &[root],
                &properties,
                &generations,
                RendererMode::Auto,
            )
            .expect("auto renderer falls back")
        else {
            panic!("malformed image must not record")
        };
        assert!(
            eligibility
                .reasons
                .contains(&FrameArtifactFallbackReason::LegacyBoundary(
                    LegacyPaintReason::MissingPreparedImage
                ))
        );
        assert_eq!(take_full_artifact_record_count(), 0);
    }

    #[test]
    fn undecorated_prepared_image_fit_sampling_opacity_scale_and_format_match_strictly() {
        let pixels: Arc<[u8]> = Arc::from([
            255_u8, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 255, 255, 255, 0, 64,
        ]);
        for (fit, sampling, opacity, config) in [
            (
                crate::view::ImageFit::Fill,
                crate::view::ImageSampling::Linear,
                1.0,
                PaintParityConfig::default(),
            ),
            (
                crate::view::ImageFit::Contain,
                crate::view::ImageSampling::Nearest,
                0.4,
                PaintParityConfig {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    scale_factor: 1.5,
                    ..PaintParityConfig::default()
                },
            ),
            (
                crate::view::ImageFit::Cover,
                crate::view::ImageSampling::Linear,
                0.0,
                PaintParityConfig {
                    scale_factor: 2.0,
                    ..PaintParityConfig::default()
                },
            ),
        ] {
            let (artifact_arena, artifact_roots) =
                bare_image_fixture(pixels.clone(), fit, sampling, opacity);
            let (properties, generations) = sync_identity(&artifact_arena, &artifact_roots);
            let (artifact, eligibility) =
                whole_frame_artifact(&artifact_arena, &artifact_roots, &properties, &generations);
            assert!(eligibility.eligible);
            assert!(matches!(
                artifact.ops.as_slice(),
                [PaintOp::PreparedImage(_)]
            ));
            let (legacy_arena, legacy_roots) =
                bare_image_fixture(pixels.clone(), fit, sampling, opacity);
            drop(artifact_arena);
            let mut artifact_graph = compiled_whole_frame_graph_with_config(&artifact, config);
            let mut legacy_graph =
                legacy_roots_graph_with_config(legacy_arena, &legacy_roots, config);
            assert_eq!(
                strict_paint_snapshot(&mut artifact_graph, config),
                strict_paint_snapshot(&mut legacy_graph, config),
                "fit={fit:?} sampling={sampling:?} opacity={opacity}"
            );
        }
    }

    fn assert_image_metadata_fallback(
        arena: &NodeArena,
        roots: &[NodeKey],
        expected: LegacyPaintReason,
    ) {
        let (properties, generations) = sync_identity(arena, roots);
        let _ = take_full_artifact_record_count();
        let FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility) =
            record_frame_artifact(arena, roots, &properties, &generations, RendererMode::Auto)
                .expect("auto fallback")
        else {
            panic!("image must remain legacy: expected {expected:?}")
        };
        assert!(
            eligibility
                .reasons
                .contains(&FrameArtifactFallbackReason::LegacyBoundary(expected)),
            "{eligibility:?}"
        );
        assert_eq!(take_full_artifact_record_count(), 0);
    }

    #[test]
    fn path_loading_or_error_matches_strictly_while_slots_and_inner_clip_fall_back() {
        let mut size = Style::new();
        size.insert(PropertyId::Width, ParsedValue::Length(Length::px(20.0)));
        size.insert(PropertyId::Height, ParsedValue::Length(Length::px(20.0)));
        let (measure, place) = constraints();
        let missing_path_fixture = || {
            let mut path_image = Image::new_with_id(
                27,
                ImageSource::Path(std::path::PathBuf::from(
                    "/definitely/missing/rfgui-m4-image.png",
                )),
            );
            path_image.apply_style(size.clone());
            let mut arena = new_test_arena();
            let root = commit_element(&mut arena, Box::new(path_image));
            measure_and_place(&mut arena, root, measure, place);
            (arena, vec![root])
        };
        let (path_arena, path_roots) = missing_path_fixture();
        let (properties, generations) = sync_identity(&path_arena, &path_roots);
        let (artifact, eligibility) =
            whole_frame_artifact(&path_arena, &path_roots, &properties, &generations);
        assert!(eligibility.eligible, "{eligibility:?}");
        assert!(
            artifact
                .ops
                .iter()
                .all(|op| !matches!(op, PaintOp::PreparedImage(_)))
        );
        drop(path_arena);
        let (legacy_arena, legacy_roots) = missing_path_fixture();
        assert_eq!(
            strict_paint_snapshot(
                &mut compiled_whole_frame_graph(&artifact),
                PaintParityConfig::default(),
            ),
            strict_paint_snapshot(
                &mut legacy_roots_graph(legacy_arena, &legacy_roots),
                PaintParityConfig::default(),
            ),
        );

        let pixels: Arc<[u8]> = Arc::from([255_u8; 16]);
        let (mut slot_arena, slot_roots) = bare_image_fixture(
            pixels.clone(),
            crate::view::ImageFit::Fill,
            crate::view::ImageSampling::Linear,
            1.0,
        );
        let slot_root = slot_roots[0];
        slot_arena.with_element_taken(slot_root, |element, _| {
            let mut style = Style::new();
            style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
            element
                .as_any_mut()
                .downcast_mut::<Image>()
                .expect("image")
                .apply_style(style);
        });
        let slot = commit_child(
            &mut slot_arena,
            slot_root,
            Box::new(Element::new_with_id(28, 0.0, 0.0, 1.0, 1.0)),
        );
        slot_arena.with_element_taken(slot_root, |element, _| {
            element
                .as_any_mut()
                .downcast_mut::<Image>()
                .expect("image")
                .attach_loading_slot_cold(vec![slot]);
        });
        measure_and_place(&mut slot_arena, slot_root, measure, place);
        assert_image_metadata_fallback(
            &slot_arena,
            &slot_roots,
            LegacyPaintReason::MissingPreparedImage,
        );

        let (mut child_arena, child_roots) = bare_image_fixture(
            pixels.clone(),
            crate::view::ImageFit::Fill,
            crate::view::ImageSampling::Linear,
            1.0,
        );
        child_arena.with_element_taken(child_roots[0], |element, _| {
            let mut style = Style::new();
            style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
            element
                .as_any_mut()
                .downcast_mut::<Image>()
                .expect("image")
                .apply_style(style);
        });
        commit_child(
            &mut child_arena,
            child_roots[0],
            Box::new(Element::new_with_id(30, 0.0, 0.0, 1.0, 1.0)),
        );
        measure_and_place(&mut child_arena, child_roots[0], measure, place);
        assert_image_metadata_fallback(
            &child_arena,
            &child_roots,
            LegacyPaintReason::MissingPreparedImage,
        );

        let clipped_fixture = || {
            let mut clipped = Image::new_with_id(
                29,
                ImageSource::Rgba {
                    width: 2,
                    height: 2,
                    pixels: pixels.clone(),
                },
            );
            let mut clip_style = size.clone();
            clip_style.insert(
                PropertyId::Position,
                ParsedValue::Position(
                    Position::absolute()
                        .left(Length::px(4.0))
                        .top(Length::px(5.0))
                        .clip(ClipMode::AnchorParent),
                ),
            );
            clipped.apply_style(clip_style);
            let mut arena = new_test_arena();
            let root = commit_element(&mut arena, Box::new(clipped));
            measure_and_place(&mut arena, root, measure, place);
            (arena, vec![root])
        };
        let (clip_arena, clip_roots) = clipped_fixture();
        let (properties, generations) = sync_identity(&clip_arena, &clip_roots);
        let (artifact, eligibility) =
            whole_frame_artifact(&clip_arena, &clip_roots, &properties, &generations);
        assert!(eligibility.eligible, "{eligibility:?}");
        assert_eq!(artifact.clip_nodes.len(), 1);
        assert!(matches!(
            artifact.ops.last(),
            Some(PaintOp::PreparedImage(_))
        ));
        let mut graph = compiled_whole_frame_graph(&artifact);
        let snapshot = graph.test_compile_snapshot().unwrap();
        let composites = snapshot
            .pass_payloads()
            .iter()
            .filter_map(|payload| match payload {
                FramePassTestPayload::TextureComposite(composite)
                    if composite.sampled_source.is_some() =>
                {
                    Some(composite)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(composites.len(), 1);
        assert_eq!(composites[0].effective_scissor_rect, Some([0, 0, 320, 240]));
    }

    #[test]
    fn image_accepts_property_clip_but_rejects_transform_and_scroll_properties() {
        let pixels: Arc<[u8]> = Arc::from([255_u8; 16]);
        let (arena, roots) = bare_image_fixture(
            pixels,
            crate::view::ImageFit::Fill,
            crate::view::ImageSampling::Linear,
            1.0,
        );
        let root = roots[0];
        let revision = PaintContentRevision {
            self_paint_revision: 1,
            composite_revision: 1,
            topology_revision: 1,
        };
        let rejected = [
            PropertyTreeState {
                transform: Some(TransformNodeId(root)),
                ..PropertyTreeState::default()
            },
            PropertyTreeState {
                scroll: Some(crate::view::compositor::property_tree::ScrollNodeId(root)),
                ..PropertyTreeState::default()
            },
        ];
        let element = &arena.get(root).expect("image").element;
        for properties in rejected {
            assert!(
                element
                    .record_shadow_paint_metadata(
                        root,
                        properties,
                        revision,
                        &arena,
                        PaintRecordingContext::default(),
                    )
                    .is_none()
            );
        }
        let clip = ClipNodeId {
            owner: root,
            role: ClipNodeRole::SelfClip,
        };
        let properties = PropertyTreeState {
            clip: Some(clip),
            ..PropertyTreeState::default()
        };
        assert_eq!(
            element
                .record_shadow_paint_metadata(
                    root,
                    properties,
                    revision,
                    &arena,
                    PaintRecordingContext::default(),
                )
                .expect("property-tree clip is compiler-owned")
                .properties
                .clip,
            Some(clip)
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn recorder_compiles_real_image_and_svg_descendants_with_parent_contents_clip() {
        const SCISSOR: [u32; 4] = [2, 3, 20, 10];
        const SVG_CONTENT: &str = "<svg xmlns='http://www.w3.org/2000/svg' width='24' height='18'><rect width='24' height='18' fill='#22c55e'/></svg>";

        let mut arena = new_test_arena();
        let parent = commit_element(
            &mut arena,
            Box::new(TransparentContentsClipParent {
                id: 0x8c10,
                opacity: 1.0,
                scissor: SCISSOR,
                children: Vec::new(),
            }),
        );

        let mut image = Image::new_with_id(
            0x8c11,
            ImageSource::Rgba {
                width: 1,
                height: 1,
                pixels: Arc::from([255, 255, 255, 255]),
            },
        );
        let mut image_style = Style::new();
        image_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(24.0)));
        image_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(18.0)));
        image.apply_style(image_style);
        let image = commit_child(&mut arena, parent, Box::new(image));

        let mut svg = Svg::new_with_id(0x8c12, SvgSource::Content(SVG_CONTENT.into()));
        let mut svg_style = Style::new();
        svg_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(24.0)));
        svg_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(18.0)));
        svg.apply_style(svg_style);
        let svg = commit_child(&mut arena, parent, Box::new(svg));

        let (measure, place) = constraints();
        measure_and_place(&mut arena, image, measure, place);
        measure_and_place(&mut arena, svg, measure, place);
        arena
            .get_mut(svg)
            .expect("svg")
            .element
            .as_any_mut()
            .downcast_mut::<Svg>()
            .expect("Svg host")
            .prepare_content_paint_for_test(SVG_CONTENT, (24.0, 18.0), 1.0)
            .expect("prepare exact SVG paint");

        let (properties, generations) = sync_identity(&arena, &[parent]);
        let expected_clip = ClipNodeId {
            owner: parent,
            role: ClipNodeRole::ContentsClip,
        };
        for child in [image, svg] {
            assert_eq!(
                properties
                    .node_state_for(child)
                    .expect("child property state")
                    .paint
                    .clip,
                Some(expected_clip)
            );
        }

        let (artifact, eligibility) =
            whole_frame_artifact(&arena, &[parent], &properties, &generations);
        assert!(eligibility.eligible, "{eligibility:?}");
        assert_eq!(
            artifact
                .chunks
                .iter()
                .map(|chunk| (chunk.owner, chunk.id.role, chunk.properties.clip))
                .collect::<Vec<_>>(),
            vec![
                (image, PaintChunkRole::ImageContent, Some(expected_clip)),
                (svg, PaintChunkRole::SvgContent, Some(expected_clip)),
            ]
        );
        assert!(matches!(
            artifact.clip_nodes.as_slice(),
            [ClipNodeSnapshot {
                id,
                owner,
                logical_scissor: SCISSOR,
                behavior: ClipBehavior::Intersect,
                ..
            }] if *id == expected_clip && *owner == parent
        ));

        let mut graph = compiled_whole_frame_graph(&artifact);
        let _ = strict_paint_snapshot(&mut graph, PaintParityConfig::default());
        let passes =
            graph.test_graphics_passes_mut::<crate::view::render_pass::TextureCompositePass>();
        assert_eq!(passes.len(), 2);
        assert!(passes.iter().all(|pass| {
            let snapshot = pass.test_snapshot();
            snapshot.explicit_scissor_rect.is_none()
                && snapshot.effective_scissor_rect == Some(SCISSOR)
        }));
    }

    #[test]
    fn image_payload_identity_detects_fit_drift_between_metadata_and_full_recording() {
        let pixels: Arc<[u8]> = Arc::from([255_u8; 16]);
        let (arena, roots) = bare_image_fixture(
            pixels,
            crate::view::ImageFit::Contain,
            crate::view::ImageSampling::Linear,
            1.0,
        );
        let (properties, generations) = sync_identity(&arena, &roots);
        let preflight = record_coverage_manifest(
            &arena,
            &roots,
            false,
            true,
            CoverageRecordingMode::MetadataOnly,
            &properties,
            &generations,
        );
        arena
            .get_mut(roots[0])
            .expect("image")
            .element
            .as_any_mut()
            .downcast_mut::<Image>()
            .expect("image")
            .set_fit(crate::view::ImageFit::Cover);
        let full = record_coverage_manifest(
            &arena,
            &roots,
            false,
            true,
            CoverageRecordingMode::FullArtifact,
            &properties,
            &generations,
        );
        assert!(!super::frame_recorder::canonical_manifest_matches(
            &preflight, &full
        ));
    }

    #[test]
    fn effect_snapshot_drift_between_metadata_and_full_is_not_canonical() {
        let (arena, root, properties, generations) =
            prepared_leaf(0x6b10, Color::rgb(10, 20, 30), 0.5, false);
        let preflight = record_coverage_manifest(
            &arena,
            &[root],
            false,
            true,
            CoverageRecordingMode::MetadataOnly,
            &properties,
            &generations,
        );
        let mut full = record_coverage_manifest(
            &arena,
            &[root],
            false,
            true,
            CoverageRecordingMode::FullArtifact,
            &properties,
            &generations,
        );
        let PaintCoverageItem::ArtifactChunk {
            effect_snapshot, ..
        } = &mut full.items[0]
        else {
            panic!("effect fixture must be an artifact chunk")
        };
        assert_eq!(effect_snapshot[0].opacity.to_bits(), 0.5_f32.to_bits());
        effect_snapshot[0].opacity = 0.25;
        assert!(!super::frame_recorder::canonical_manifest_matches(
            &preflight, &full
        ));
    }

    #[test]
    fn owner_topology_drift_between_metadata_and_full_is_not_canonical() {
        let (arena, roots, child) = prepared_plain_tree();
        let (properties, generations) = sync_identity(&arena, &roots);
        let preflight = record_coverage_manifest(
            &arena,
            &roots,
            false,
            true,
            CoverageRecordingMode::MetadataOnly,
            &properties,
            &generations,
        );
        let mut full = record_coverage_manifest(
            &arena,
            &roots,
            false,
            true,
            CoverageRecordingMode::FullArtifact,
            &properties,
            &generations,
        );
        let PaintCoverageItem::ArtifactChunk { owner_snapshot, .. } = &mut full.items[1] else {
            panic!("child fixture must be an artifact chunk")
        };
        owner_snapshot
            .iter_mut()
            .find(|snapshot| snapshot.owner == child)
            .expect("child owner snapshot")
            .parent = None;
        assert!(!super::frame_recorder::canonical_manifest_matches(
            &preflight, &full
        ));
    }

    #[test]
    fn nested_elements_stay_legacy() {
        let mut nested_arena = new_test_arena();
        let nested_root = commit_element(
            &mut nested_arena,
            Box::new(leaf_element(31, Color::rgb(1, 2, 3), 1.0, false)),
        );
        let _ = commit_child(
            &mut nested_arena,
            nested_root,
            Box::new(leaf_element(32, Color::rgb(4, 5, 6), 1.0, false)),
        );
        let (properties, generations) = sync_identity(&nested_arena, &[nested_root]);
        let PaintRecordOutcome::LegacySubtree(nested) =
            record_root(&nested_arena, nested_root, &properties, &generations)
        else {
            panic!("nested root should remain legacy");
        };
        assert_eq!(nested.reason, LegacyPaintReason::HasChildren);
    }

    struct RecordingHost {
        id: u64,
        builds: Arc<AtomicUsize>,
        fill: Option<[f32; 4]>,
    }

    enum CustomLeafRecordMode {
        Fill { rgba: [f32; 4], opacity: f32 },
        InvalidBounds,
        DoubleFill,
        Drift { calls: Arc<AtomicUsize> },
    }

    struct CustomLeafPaintHost {
        id: u64,
        bounds: Rect,
        mode: CustomLeafRecordMode,
        children: Vec<NodeKey>,
        expose_children: bool,
        deferred: bool,
        active_animator: bool,
        retained_properties: crate::view::base_component::RetainedPaintProperties,
    }

    enum CustomWrapperRecordMode {
        Canonical,
        InvalidBounds,
        Empty,
        Overflow,
        Drift { calls: Arc<AtomicUsize> },
    }

    struct CustomWrapperPaintHost {
        id: u64,
        bounds: Rect,
        mode: CustomWrapperRecordMode,
        children: Vec<NodeKey>,
    }

    impl CustomWrapperPaintHost {
        fn canonical(id: u64) -> Self {
            Self {
                id,
                bounds: Rect {
                    x: 3.0,
                    y: 5.0,
                    width: 24.0,
                    height: 12.0,
                },
                mode: CustomWrapperRecordMode::Canonical,
                children: Vec::new(),
            }
        }

        fn emit_legacy_fill(
            graph: &mut FrameGraph,
            ctx: &mut UiBuildContext,
            bounds: Rect,
            mut rgba: [f32; 4],
            opacity: f32,
        ) {
            rgba[3] *= opacity;
            let mut pass = DrawRectPass::new(
                RectPassParams {
                    position: [bounds.x, bounds.y],
                    size: [bounds.width, bounds.height],
                    fill_color: rgba,
                    opacity: 1.0,
                    ..Default::default()
                },
                DrawRectInput::default(),
                DrawRectOutput::default(),
            );
            pass.set_render_mode(RectRenderMode::FillOnly);
            ctx.emit_draw_rect_pass(graph, pass);
        }
    }

    impl Layoutable for CustomWrapperPaintHost {
        fn measure(&mut self, _constraints: LayoutConstraints, _arena: &mut NodeArena) {}
        fn place(&mut self, _placement: LayoutPlacement, _arena: &mut NodeArena) {}
        fn measured_size(&self) -> (f32, f32) {
            (self.bounds.width, self.bounds.height)
        }
        fn set_layout_width(&mut self, width: f32) {
            self.bounds.width = width;
        }
        fn set_layout_height(&mut self, height: f32) {
            self.bounds.height = height;
        }
    }

    impl EventTarget for CustomWrapperPaintHost {}

    impl Renderable for CustomWrapperPaintHost {
        fn build(
            &mut self,
            graph: &mut FrameGraph,
            arena: &mut NodeArena,
            mut ctx: UiBuildContext,
        ) -> BuildState {
            for (rgba, opacity) in [([0.8, 0.0, 0.0, 1.0], 1.0), ([0.0, 0.8, 0.0, 1.0], 0.5)] {
                Self::emit_legacy_fill(graph, &mut ctx, self.bounds, rgba, opacity);
            }
            for child_key in self.children.clone() {
                let child_ctx = UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone());
                if let Some(state) = arena.with_element_taken(child_key, |child, arena| {
                    child.build(graph, arena, child_ctx)
                }) {
                    ctx.set_state(state);
                }
            }
            for (rgba, opacity) in [([0.0, 0.0, 0.8, 1.0], 1.0), ([0.8, 0.8, 0.0, 1.0], 0.25)] {
                Self::emit_legacy_fill(graph, &mut ctx, self.bounds, rgba, opacity);
            }
            ctx.into_state()
        }
    }

    impl ElementTrait for CustomWrapperPaintHost {
        fn stable_id(&self) -> u64 {
            self.id
        }

        fn box_model_snapshot(&self) -> BoxModelSnapshot {
            BoxModelSnapshot {
                node_id: self.id,
                parent_id: None,
                x: self.bounds.x,
                y: self.bounds.y,
                width: self.bounds.width,
                height: self.bounds.height,
                border_radius: 0.0,
                should_render: true,
            }
        }

        fn as_any(&self) -> &dyn Any {
            self
        }

        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }

        fn record_custom_wrapper_paint(
            &self,
            context: CustomWrapperPaintContext,
            recorder: &mut CustomWrapperPaintRecorder,
        ) {
            let bounds = context.bounds();
            match &self.mode {
                CustomWrapperRecordMode::Canonical => {
                    recorder.fill_rect_before_children(bounds, [0.8, 0.0, 0.0, 1.0], 1.0);
                    recorder.fill_rect_before_children(bounds, [0.0, 0.8, 0.0, 1.0], 0.5);
                    recorder.fill_rect_after_children(bounds, [0.0, 0.0, 0.8, 1.0], 1.0);
                    recorder.fill_rect_after_children(bounds, [0.8, 0.8, 0.0, 1.0], 0.25);
                }
                CustomWrapperRecordMode::InvalidBounds => {
                    recorder.fill_rect_before_children(
                        Rect {
                            x: f32::NAN,
                            ..bounds
                        },
                        [0.8, 0.0, 0.0, 1.0],
                        1.0,
                    );
                }
                CustomWrapperRecordMode::Empty => {}
                CustomWrapperRecordMode::Overflow => {
                    for _ in 0..=(u16::MAX as usize + 1) {
                        recorder.fill_rect_before_children(bounds, [0.8, 0.0, 0.0, 1.0], 1.0);
                    }
                }
                CustomWrapperRecordMode::Drift { calls } => {
                    let call = calls.fetch_add(1, Ordering::Relaxed);
                    let green = if call < 2 { 0.2 } else { 0.7 };
                    recorder.fill_rect_before_children(bounds, [0.8, green, 0.0, 1.0], 1.0);
                    recorder.fill_rect_after_children(bounds, [0.0, 0.0, 0.8, 1.0], 1.0);
                }
            }
        }

        fn children(&self) -> &[NodeKey] {
            &self.children
        }

        fn sync_children_mirror(&mut self, children: &[NodeKey]) {
            self.children.clear();
            self.children.extend_from_slice(children);
        }
    }

    impl CustomLeafPaintHost {
        fn fill(id: u64) -> Self {
            Self {
                id,
                bounds: Rect {
                    x: 4.0,
                    y: 6.0,
                    width: 20.0,
                    height: 10.0,
                },
                mode: CustomLeafRecordMode::Fill {
                    rgba: [0.1, 0.2, 0.3, 1.0],
                    opacity: 0.75,
                },
                children: Vec::new(),
                expose_children: true,
                deferred: false,
                active_animator: false,
                retained_properties: Default::default(),
            }
        }
    }

    impl Layoutable for CustomLeafPaintHost {
        fn measure(&mut self, _constraints: LayoutConstraints, _arena: &mut NodeArena) {}
        fn place(&mut self, _placement: LayoutPlacement, _arena: &mut NodeArena) {}
        fn measured_size(&self) -> (f32, f32) {
            (self.bounds.width, self.bounds.height)
        }
        fn set_layout_width(&mut self, width: f32) {
            self.bounds.width = width;
        }
        fn set_layout_height(&mut self, height: f32) {
            self.bounds.height = height;
        }
    }

    impl EventTarget for CustomLeafPaintHost {}

    impl Renderable for CustomLeafPaintHost {
        fn build(
            &mut self,
            graph: &mut FrameGraph,
            _arena: &mut NodeArena,
            mut ctx: UiBuildContext,
        ) -> BuildState {
            if let CustomLeafRecordMode::Fill { rgba, opacity } = &self.mode {
                let pass = DrawRectPass::new(
                    RectPassParams {
                        position: [self.bounds.x, self.bounds.y],
                        size: [self.bounds.width, self.bounds.height],
                        fill_color: *rgba,
                        opacity: *opacity,
                        ..Default::default()
                    },
                    DrawRectInput::default(),
                    DrawRectOutput::default(),
                );
                ctx.emit_draw_rect_pass(graph, pass);
            }
            ctx.into_state()
        }
    }

    impl ElementTrait for CustomLeafPaintHost {
        fn stable_id(&self) -> u64 {
            self.id
        }

        fn box_model_snapshot(&self) -> BoxModelSnapshot {
            BoxModelSnapshot {
                node_id: self.id,
                parent_id: None,
                x: self.bounds.x,
                y: self.bounds.y,
                width: self.bounds.width,
                height: self.bounds.height,
                border_radius: 0.0,
                should_render: true,
            }
        }

        fn as_any(&self) -> &dyn Any {
            self
        }

        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }

        fn record_custom_leaf_paint(
            &self,
            context: CustomLeafPaintContext,
            recorder: &mut CustomLeafPaintRecorder,
        ) {
            let bounds = context.bounds();
            match &self.mode {
                CustomLeafRecordMode::Fill { rgba, opacity } => {
                    recorder.fill_rect(bounds, *rgba, *opacity);
                }
                CustomLeafRecordMode::InvalidBounds => recorder.fill_rect(
                    Rect {
                        x: f32::NAN,
                        ..bounds
                    },
                    [0.1, 0.2, 0.3, 1.0],
                    1.0,
                ),
                CustomLeafRecordMode::DoubleFill => {
                    recorder.fill_rect(bounds, [0.1, 0.2, 0.3, 1.0], 1.0);
                    recorder.fill_rect(bounds, [0.4, 0.5, 0.6, 1.0], 1.0);
                }
                CustomLeafRecordMode::Drift { calls } => {
                    let call = calls.fetch_add(1, Ordering::Relaxed);
                    let green = if call < 2 { 0.2 } else { 0.8 };
                    recorder.fill_rect(bounds, [0.1, green, 0.3, 1.0], 1.0);
                }
            }
        }

        fn children(&self) -> &[NodeKey] {
            if self.expose_children {
                &self.children
            } else {
                &[]
            }
        }

        fn sync_children_mirror(&mut self, children: &[NodeKey]) {
            self.children.clear();
            self.children.extend_from_slice(children);
        }

        fn is_deferred_to_root_viewport_render(&self) -> bool {
            self.deferred
        }

        fn has_active_animator(&self) -> bool {
            self.active_animator
        }

        fn retained_paint_properties(
            &self,
        ) -> crate::view::base_component::RetainedPaintProperties {
            self.retained_properties
        }
    }

    #[derive(Clone, Copy)]
    enum MalformedChunk {
        MetadataNaNBounds,
        MetadataNegativeBounds,
        MetadataProperties,
        MetadataRevision,
        FullOwner,
        FullChunkOwner,
        FullProperties,
        FullRevision,
        FullRange,
        FullBounds,
    }

    struct MalformedRecordingHost {
        id: u64,
        malformed: MalformedChunk,
        full_records: Arc<AtomicUsize>,
    }

    impl Layoutable for MalformedRecordingHost {
        fn measure(&mut self, _constraints: LayoutConstraints, _arena: &mut NodeArena) {}
        fn place(&mut self, _placement: LayoutPlacement, _arena: &mut NodeArena) {}
        fn measured_size(&self) -> (f32, f32) {
            (10.0, 10.0)
        }
        fn set_layout_width(&mut self, _width: f32) {}
        fn set_layout_height(&mut self, _height: f32) {}
    }

    impl EventTarget for MalformedRecordingHost {}

    impl Renderable for MalformedRecordingHost {
        fn build(
            &mut self,
            _graph: &mut FrameGraph,
            _arena: &mut NodeArena,
            ctx: UiBuildContext,
        ) -> BuildState {
            ctx.into_state()
        }
    }

    impl MalformedRecordingHost {
        fn metadata(
            &self,
            owner: NodeKey,
            properties: PropertyTreeState,
            revision: PaintContentRevision,
        ) -> PaintChunkMetadata {
            let fake_owner = NodeKey::null();
            let metadata_properties = matches!(self.malformed, MalformedChunk::MetadataProperties);
            let metadata_revision = matches!(self.malformed, MalformedChunk::MetadataRevision);
            let bounds = match self.malformed {
                MalformedChunk::MetadataNaNBounds => Rect {
                    x: f32::NAN,
                    y: 0.0,
                    width: 10.0,
                    height: 10.0,
                },
                MalformedChunk::MetadataNegativeBounds => Rect {
                    x: 0.0,
                    y: 0.0,
                    width: -1.0,
                    height: 10.0,
                },
                _ => Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 10.0,
                    height: 10.0,
                },
            };
            PaintChunkMetadata {
                id: PaintChunkId {
                    owner,
                    scope: PaintPropertyScope::SelfPaint,
                    phase: PaintNodePhase::BeforeChildren,
                    slot: 0,
                    role: PaintChunkRole::SelfDecoration,
                },
                owner,
                bounds,
                properties: if metadata_properties {
                    PropertyTreeState {
                        transform: Some(TransformNodeId(fake_owner)),
                        ..PropertyTreeState::default()
                    }
                } else {
                    properties
                },
                content_revision: if metadata_revision {
                    PaintContentRevision {
                        self_paint_revision: revision.self_paint_revision.wrapping_add(1),
                        ..revision
                    }
                } else {
                    revision
                },
                payload_identity: PaintPayloadIdentity::None,
            }
        }
    }

    impl ElementTrait for MalformedRecordingHost {
        fn stable_id(&self) -> u64 {
            self.id
        }
        fn box_model_snapshot(&self) -> BoxModelSnapshot {
            BoxModelSnapshot {
                node_id: self.id,
                parent_id: None,
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 10.0,
                border_radius: 0.0,
                should_render: true,
            }
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }
        fn shadow_paint_recording_capability(
            &self,
            _arena: &NodeArena,
            _deferred_phase_root: bool,
            _recording_context: PaintRecordingContext,
        ) -> ShadowPaintRecordingCapability {
            ShadowPaintRecordingCapability::Recordable
        }
        fn record_shadow_paint_metadata(
            &self,
            owner: NodeKey,
            properties: PropertyTreeState,
            revision: PaintContentRevision,
            _arena: &NodeArena,
            _recording_context: PaintRecordingContext,
        ) -> Option<PaintChunkMetadata> {
            Some(self.metadata(owner, properties, revision))
        }
        fn record_shadow_paint_artifact(
            &self,
            owner: NodeKey,
            properties: PropertyTreeState,
            revision: PaintContentRevision,
            _arena: &NodeArena,
            _recording_context: PaintRecordingContext,
        ) -> Option<PaintArtifact> {
            self.full_records.fetch_add(1, Ordering::Relaxed);
            let mut chunk = self.metadata(owner, properties, revision);
            let fake_owner = NodeKey::null();
            match self.malformed {
                MalformedChunk::FullOwner => {
                    chunk.id.owner = fake_owner;
                    chunk.owner = fake_owner;
                }
                MalformedChunk::FullChunkOwner => chunk.owner = fake_owner,
                MalformedChunk::FullProperties => {
                    chunk.properties.transform = Some(TransformNodeId(fake_owner));
                }
                MalformedChunk::FullRevision => {
                    chunk.content_revision.self_paint_revision =
                        chunk.content_revision.self_paint_revision.wrapping_add(1);
                }
                _ => {}
            }
            if matches!(self.malformed, MalformedChunk::FullBounds) {
                chunk.bounds.width += 1.0;
            }
            Some(PaintArtifact {
                target: Default::default(),
                chunks: vec![PaintChunk {
                    id: chunk.id,
                    owner: chunk.owner,
                    op_range: if matches!(self.malformed, MalformedChunk::FullRange) {
                        1..0
                    } else {
                        0..0
                    },
                    bounds: chunk.bounds,
                    properties: chunk.properties,
                    content_revision: chunk.content_revision,
                    payload_identity: chunk.payload_identity,
                }],
                ops: Vec::new(),
                clip_nodes: Vec::new(),
                effect_nodes: Vec::new(),
                owner_nodes: Vec::new(),
            })
        }
    }

    impl Layoutable for RecordingHost {
        fn measure(&mut self, _constraints: LayoutConstraints, _arena: &mut NodeArena) {}
        fn place(&mut self, _placement: LayoutPlacement, _arena: &mut NodeArena) {}
        fn measured_size(&self) -> (f32, f32) {
            (1.0, 1.0)
        }
        fn set_layout_width(&mut self, _width: f32) {}
        fn set_layout_height(&mut self, _height: f32) {}
    }

    impl EventTarget for RecordingHost {}

    impl Renderable for RecordingHost {
        fn build(
            &mut self,
            _graph: &mut FrameGraph,
            _arena: &mut NodeArena,
            mut ctx: UiBuildContext,
        ) -> BuildState {
            self.builds.fetch_add(1, Ordering::Relaxed);
            if let Some(fill_color) = self.fill {
                let pass = DrawRectPass::new(
                    RectPassParams {
                        position: [0.0, 0.0],
                        size: [1.0, 1.0],
                        fill_color,
                        opacity: 1.0,
                        ..Default::default()
                    },
                    DrawRectInput::default(),
                    DrawRectOutput::default(),
                );
                ctx.emit_draw_rect_pass(_graph, pass);
            }
            ctx.into_state()
        }
    }

    impl ElementTrait for RecordingHost {
        fn stable_id(&self) -> u64 {
            self.id
        }
        fn box_model_snapshot(&self) -> BoxModelSnapshot {
            BoxModelSnapshot {
                node_id: self.id,
                parent_id: None,
                x: 0.0,
                y: 0.0,
                width: 1.0,
                height: 1.0,
                border_radius: 0.0,
                should_render: true,
            }
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }
    }

    #[test]
    fn custom_legacy_subtree_builds_exactly_once_and_recording_does_not_touch_deferred() {
        let builds = Arc::new(AtomicUsize::new(0));
        let mut arena = NodeArena::new();
        let root = arena.insert(Node::new(Box::new(RecordingHost {
            id: 40,
            builds: builds.clone(),
            fill: None,
        })));
        arena.push_root(root);
        let (properties, generations) = sync_identity(&arena, &[root]);

        let mut ctx = UiBuildContext::new(100, 100, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        ctx.register_deferred(root, 40);
        let outcome = record_root(&arena, root, &properties, &generations);
        assert_eq!(ctx.next_deferred().map(|node| node.key), Some(root));
        let PaintRecordOutcome::LegacySubtree(legacy) = outcome else {
            panic!("custom host should remain legacy");
        };
        assert_eq!(legacy.reason, LegacyPaintReason::UnknownHost);

        let mut graph = FrameGraph::new();
        let _ =
            arena.with_element_taken(root, |element, arena| element.build(&mut graph, arena, ctx));
        assert_eq!(builds.load(Ordering::Relaxed), 1);
    }

    fn custom_leaf_fixture(
        host: CustomLeafPaintHost,
    ) -> (NodeArena, NodeKey, PropertyTrees, PaintGenerationTracker) {
        let mut arena = new_test_arena();
        let root = commit_element(&mut arena, Box::new(host));
        let (properties, generations) = sync_identity(&arena, &[root]);
        (arena, root, properties, generations)
    }

    #[test]
    fn custom_leaf_typed_adapter_records_canonical_fill_and_compiles() {
        let host = CustomLeafPaintHost::fill(0x8f10);
        let expected = host.bounds;
        let (arena, root, properties, generations) = custom_leaf_fixture(host);
        let _ = take_full_artifact_record_count();
        let (artifact, eligibility) =
            whole_frame_artifact(&arena, &[root], &properties, &generations);
        assert!(eligibility.eligible);
        assert_eq!(take_full_artifact_record_count(), 1);
        assert_eq!(artifact.chunks.len(), 1);
        assert_eq!(artifact.chunks[0].owner, root);
        assert_eq!(artifact.chunks[0].id.scope, PaintPropertyScope::SelfPaint);
        assert_eq!(artifact.chunks[0].id.phase, PaintNodePhase::BeforeChildren);
        assert_eq!(artifact.chunks[0].id.slot, 0);
        assert_eq!(artifact.chunks[0].id.role, PaintChunkRole::SelfDecoration);
        assert!(matches!(
            artifact.ops.as_slice(),
            [PaintOp::DrawRect(rect)]
                if rect.mode == crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly
        ));
        let graph = compiled_whole_frame_graph(&artifact);
        let rects = graph.test_rect_pass_snapshots();
        let [rect] = rects.as_slice() else {
            panic!("custom fill must compile to exactly one rect pass")
        };
        assert_eq!(
            rect.position_bits,
            [expected.x, expected.y].map(f32::to_bits)
        );
        assert_eq!(
            rect.size_bits,
            [expected.width, expected.height].map(f32::to_bits)
        );
        assert_eq!(rect.opacity_bits, 1.0_f32.to_bits());
        assert_eq!(rect.fill_color_bits[3], 0.75_f32.to_bits());
    }

    #[test]
    fn custom_leaf_invalid_bounds_opacity_or_cardinality_stays_unknown_without_full_record() {
        let invalid_hosts = [
            {
                let mut host = CustomLeafPaintHost::fill(0x8f11);
                host.mode = CustomLeafRecordMode::InvalidBounds;
                host
            },
            {
                let mut host = CustomLeafPaintHost::fill(0x8f12);
                host.mode = CustomLeafRecordMode::Fill {
                    rgba: [0.1, 0.2, 0.3, 1.0],
                    opacity: f32::NAN,
                };
                host
            },
            {
                let mut host = CustomLeafPaintHost::fill(0x8f13);
                host.mode = CustomLeafRecordMode::Fill {
                    rgba: [0.1, 0.2, 0.3, 1.0],
                    opacity: 1.01,
                };
                host
            },
            {
                let mut host = CustomLeafPaintHost::fill(0x8f14);
                host.mode = CustomLeafRecordMode::DoubleFill;
                host
            },
        ];
        for host in invalid_hosts {
            let (arena, root, properties, generations) = custom_leaf_fixture(host);
            let _ = take_full_artifact_record_count();
            let FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility) =
                record_frame_artifact(
                    &arena,
                    &[root],
                    &properties,
                    &generations,
                    RendererMode::Auto,
                )
                .unwrap()
            else {
                panic!("invalid public command must fail closed")
            };
            assert!(
                eligibility
                    .reasons
                    .contains(&FrameArtifactFallbackReason::LegacyBoundary(
                        LegacyPaintReason::UnknownHost
                    ))
            );
            assert_eq!(
                eligibility.debug_boundaries,
                vec![FrameArtifactDebugBoundary {
                    owner: root,
                    kind: FrameArtifactDebugBoundaryKind::Legacy(LegacyPaintReason::UnknownHost,),
                }],
                "RetainedAuto diagnostics must preserve the exact unsupported custom host",
            );
            assert_eq!(take_full_artifact_record_count(), 0);
        }
    }

    #[test]
    fn custom_leaf_structural_and_property_boundaries_fail_closed_before_full_record() {
        let (mut child_arena, child_root, _, _) =
            custom_leaf_fixture(CustomLeafPaintHost::fill(0x8f20));
        let _ = commit_child(
            &mut child_arena,
            child_root,
            Box::new(leaf_element(0x8f21, Color::rgb(1, 2, 3), 1.0, false)),
        );
        let (child_properties, child_generations) = sync_identity(&child_arena, &[child_root]);
        let _ = take_full_artifact_record_count();
        assert!(matches!(
            record_frame_artifact(
                &child_arena,
                &[child_root],
                &child_properties,
                &child_generations,
                RendererMode::Auto,
            ),
            Ok(FrameArtifactRecordOutcome::WholeFrameLegacyFallback(_))
        ));
        assert_eq!(take_full_artifact_record_count(), 0);

        let mut arena_only_host = CustomLeafPaintHost::fill(0x8f22);
        arena_only_host.expose_children = false;
        let (mut arena_only, arena_only_root, _, _) = custom_leaf_fixture(arena_only_host);
        let _ = commit_child(
            &mut arena_only,
            arena_only_root,
            Box::new(leaf_element(0x8f23, Color::rgb(4, 5, 6), 1.0, false)),
        );
        assert!(
            arena_only.get(arena_only_root).is_some_and(
                |node| !node.children().is_empty() && node.element.children().is_empty()
            )
        );
        let (arena_only_properties, arena_only_generations) =
            sync_identity(&arena_only, &[arena_only_root]);
        let _ = take_full_artifact_record_count();
        assert_eq!(
            arena_only
                .get(arena_only_root)
                .unwrap()
                .element
                .shadow_paint_recording_capability(
                    &arena_only,
                    false,
                    PaintRecordingContext::default()
                ),
            ShadowPaintRecordingCapability::Recordable
        );
        let FrameArtifactRecordOutcome::WholeFrameLegacyFallback(arena_only_legacy) =
            record_frame_artifact(
                &arena_only,
                &[arena_only_root],
                &arena_only_properties,
                &arena_only_generations,
                RendererMode::Auto,
            )
            .unwrap()
        else {
            panic!("arena child must block a trait-opaque custom leaf")
        };
        assert!(
            arena_only_legacy
                .reasons
                .contains(&FrameArtifactFallbackReason::LegacyBoundary(
                    LegacyPaintReason::MissingPaintIdentity
                ))
        );
        assert_eq!(take_full_artifact_record_count(), 0);

        let (property_arena, property_root, mut property_state, property_generations) =
            custom_leaf_fixture(CustomLeafPaintHost::fill(0x8f24));
        property_state
            .states
            .get_mut(&property_root)
            .unwrap()
            .paint
            .transform = Some(TransformNodeId(property_root));
        let _ = take_full_artifact_record_count();
        assert!(matches!(
            record_frame_artifact(
                &property_arena,
                &[property_root],
                &property_state,
                &property_generations,
                RendererMode::Auto,
            ),
            Ok(FrameArtifactRecordOutcome::WholeFrameLegacyFallback(_))
        ));
        assert_eq!(take_full_artifact_record_count(), 0);
    }

    #[test]
    fn custom_leaf_deferred_animating_and_root_opacity_stay_legacy() {
        for (id, configure) in [
            (
                0x8f30,
                (
                    true,
                    false,
                    crate::view::base_component::RetainedPaintProperties::default(),
                ),
            ),
            (
                0x8f31,
                (
                    false,
                    true,
                    crate::view::base_component::RetainedPaintProperties::default(),
                ),
            ),
        ] {
            let mut host = CustomLeafPaintHost::fill(id);
            host.deferred = configure.0;
            host.active_animator = configure.1;
            host.retained_properties = configure.2;
            let (arena, root, properties, generations) = custom_leaf_fixture(host);
            let _ = take_full_artifact_record_count();
            assert!(matches!(
                record_frame_artifact(
                    &arena,
                    &[root],
                    &properties,
                    &generations,
                    RendererMode::Auto,
                ),
                Ok(FrameArtifactRecordOutcome::WholeFrameLegacyFallback(_))
            ));
            assert_eq!(take_full_artifact_record_count(), 0);
        }

        let mut opacity_host = CustomLeafPaintHost::fill(0x8f33);
        opacity_host.retained_properties.opacity = 0.5;
        let (opacity_arena, opacity_root, opacity_properties, opacity_generations) =
            custom_leaf_fixture(opacity_host);
        let opacity_context = PaintRecordingContext {
            opacity_authority: PaintOpacityAuthority::NeutralRootEffect(EffectNodeId(opacity_root)),
            ..Default::default()
        };
        assert_eq!(
            opacity_arena
                .get(opacity_root)
                .unwrap()
                .element
                .shadow_paint_recording_capability(&opacity_arena, false, opacity_context),
            ShadowPaintRecordingCapability::Unsupported
        );
        let _ = take_full_artifact_record_count();
        assert!(
            record_root_group_opacity_frame_artifact(
                &opacity_arena,
                &[opacity_root],
                &opacity_properties,
                &opacity_generations,
                RendererMode::ForcedForTests,
            )
            .is_err()
        );
        assert_eq!(take_full_artifact_record_count(), 0);
    }

    #[test]
    fn custom_leaf_owner_pointer_mismatch_rejects_duplicate_stable_id() {
        let mut arena = new_test_arena();
        let first = commit_element(&mut arena, Box::new(CustomLeafPaintHost::fill(0x8f40)));
        let second = commit_element(&mut arena, Box::new(CustomLeafPaintHost::fill(0x8f40)));
        let first_node = arena.get(first).unwrap();
        assert!(
            first_node
                .element
                .record_shadow_paint_metadata(
                    second,
                    PropertyTreeState::default(),
                    PaintContentRevision {
                        self_paint_revision: 1,
                        composite_revision: 1,
                        topology_revision: 1,
                    },
                    &arena,
                    PaintRecordingContext::default(),
                )
                .is_none()
        );
    }

    #[test]
    fn custom_leaf_metadata_full_drift_forces_whole_frame_fallback() {
        let calls = Arc::new(AtomicUsize::new(0));
        let mut host = CustomLeafPaintHost::fill(0x8f50);
        host.mode = CustomLeafRecordMode::Drift {
            calls: calls.clone(),
        };
        let (arena, root, properties, generations) = custom_leaf_fixture(host);
        let _ = take_full_artifact_record_count();
        let outcome = record_frame_artifact(
            &arena,
            &[root],
            &properties,
            &generations,
            RendererMode::Auto,
        )
        .unwrap();
        assert!(matches!(
            outcome,
            FrameArtifactRecordOutcome::WholeFrameLegacyFallback(_)
        ));
        assert_eq!(calls.load(Ordering::Relaxed), 4);
        assert_eq!(take_full_artifact_record_count(), 1);
    }

    fn custom_wrapper_fixture(
        host: CustomWrapperPaintHost,
        child: Box<dyn ElementTrait>,
    ) -> (
        NodeArena,
        NodeKey,
        NodeKey,
        PropertyTrees,
        PaintGenerationTracker,
    ) {
        let mut arena = new_test_arena();
        let root = commit_element(&mut arena, Box::new(host));
        let child = commit_child(&mut arena, root, child);
        let (properties, generations) = sync_identity(&arena, &[root]);
        (arena, root, child, properties, generations)
    }

    #[test]
    fn custom_wrapper_public_typed_phases_preserve_order_slots_and_compile() {
        let host = CustomWrapperPaintHost::canonical(0x8f60);
        let (mut arena, root, child, properties, generations) = custom_wrapper_fixture(
            host,
            Box::new(leaf_element(0x8f61, Color::rgb(1, 2, 3), 1.0, false)),
        );

        let _ = take_full_artifact_record_count();
        let (artifact, eligibility) =
            whole_frame_artifact(&arena, &[root], &properties, &generations);
        assert!(eligibility.eligible);
        let order = artifact
            .chunks
            .iter()
            .map(|chunk| (chunk.owner, chunk.id.phase, chunk.id.slot))
            .collect::<Vec<_>>();
        assert_eq!(
            order,
            vec![
                (root, PaintNodePhase::BeforeChildren, 0),
                (root, PaintNodePhase::BeforeChildren, 1),
                (child, PaintNodePhase::BeforeChildren, 0),
                (root, PaintNodePhase::AfterChildren, 0),
                (root, PaintNodePhase::AfterChildren, 1),
            ]
        );
        assert!(
            artifact
                .chunks
                .iter()
                .filter(|chunk| chunk.owner == root)
                .all(|chunk| chunk.id.scope == PaintPropertyScope::SelfPaint
                    && chunk.id.role == PaintChunkRole::SelfDecoration)
        );
        assert!(
            artifact
                .chunks
                .iter()
                .filter(|chunk| chunk.owner == root)
                .all(|chunk| chunk.op_range.len() == 1)
        );

        let compiled_graph = compiled_whole_frame_graph(&artifact);
        let compiled_rects = compiled_graph.test_rect_pass_snapshots();
        assert_eq!(compiled_rects.len(), 5);
        assert_eq!(compiled_rects[1].opacity_bits, 1.0_f32.to_bits());
        assert_eq!(compiled_rects[1].fill_color_bits[3], 0.5_f32.to_bits());
        assert_eq!(compiled_rects[4].opacity_bits, 1.0_f32.to_bits());
        assert_eq!(compiled_rects[4].fill_color_bits[3], 0.25_f32.to_bits());

        let mut legacy_graph = FrameGraph::new();
        let legacy_ctx = UiBuildContext::new(64, 64, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        arena
            .with_element_taken(root, |element, arena| {
                element.build(&mut legacy_graph, arena, legacy_ctx)
            })
            .expect("wrapper root");
        let legacy_rects = legacy_graph.test_rect_pass_snapshots();
        assert_eq!(legacy_rects.len(), compiled_rects.len());
        for (index, (compiled, legacy)) in compiled_rects.iter().zip(&legacy_rects).enumerate() {
            assert_eq!(compiled.position_bits, legacy.position_bits);
            assert_eq!(compiled.size_bits, legacy.size_bits);
            assert_eq!(compiled.fill_color_bits, legacy.fill_color_bits);
            assert_eq!(compiled.opacity_bits, legacy.opacity_bits);
            assert_eq!(compiled.border_width_bits, legacy.border_width_bits);
            assert_eq!(compiled.border_radius_bits, legacy.border_radius_bits);
            assert_eq!(compiled.border_color_bits, legacy.border_color_bits);
            // The built-in child uses the visually equivalent zero-border
            // `Combined` legacy mode while its artifact canonicalizes to
            // `FillOnly`. Wrapper commands on either side must match exactly.
            if index != 2 {
                assert_eq!(compiled.mode, legacy.mode);
            }
        }
    }

    #[test]
    fn custom_wrapper_legacy_build_traverses_child_exactly_once_between_phases() {
        let builds = Arc::new(AtomicUsize::new(0));
        let (mut arena, root, _, _, _) = custom_wrapper_fixture(
            CustomWrapperPaintHost::canonical(0x8f62),
            Box::new(RecordingHost {
                id: 0x8f63,
                builds: builds.clone(),
                fill: Some([0.2, 0.3, 0.4, 1.0]),
            }),
        );
        let mut graph = FrameGraph::new();
        let ctx = UiBuildContext::new(64, 64, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        arena
            .with_element_taken(root, |element, arena| element.build(&mut graph, arena, ctx))
            .expect("wrapper root");
        assert_eq!(builds.load(Ordering::Relaxed), 1);
        let rects = graph.test_rect_pass_snapshots();
        assert_eq!(rects.len(), 5);
        assert_eq!(
            rects[0].fill_color_bits,
            [0.8, 0.0, 0.0, 1.0].map(f32::to_bits)
        );
        assert_eq!(
            rects[1].fill_color_bits,
            [0.0, 0.8, 0.0, 0.5].map(f32::to_bits)
        );
        assert_eq!(
            rects[2].fill_color_bits,
            [0.2, 0.3, 0.4, 1.0].map(f32::to_bits)
        );
        assert_eq!(
            rects[3].fill_color_bits,
            [0.0, 0.0, 0.8, 1.0].map(f32::to_bits)
        );
        assert_eq!(
            rects[4].fill_color_bits,
            [0.8, 0.8, 0.0, 0.25].map(f32::to_bits)
        );
    }

    #[test]
    fn custom_wrapper_drift_forces_whole_frame_fallback_after_one_full_plan() {
        let calls = Arc::new(AtomicUsize::new(0));
        let mut host = CustomWrapperPaintHost::canonical(0x8f64);
        host.mode = CustomWrapperRecordMode::Drift {
            calls: calls.clone(),
        };
        let (arena, root, _, properties, generations) = custom_wrapper_fixture(
            host,
            Box::new(leaf_element(0x8f65, Color::rgb(1, 2, 3), 1.0, false)),
        );
        let _ = take_full_artifact_record_count();
        assert!(matches!(
            record_frame_artifact(
                &arena,
                &[root],
                &properties,
                &generations,
                RendererMode::Auto,
            ),
            Ok(FrameArtifactRecordOutcome::WholeFrameLegacyFallback(_))
        ));
        assert_eq!(calls.load(Ordering::Relaxed), 4);
        assert!(take_full_artifact_record_count() >= 1);
    }

    #[test]
    fn custom_wrapper_invalid_empty_and_slot_overflow_fail_before_full_recording() {
        for (id, mode) in [
            (0x8f66, CustomWrapperRecordMode::InvalidBounds),
            (0x8f68, CustomWrapperRecordMode::Empty),
            (0x8f6a, CustomWrapperRecordMode::Overflow),
        ] {
            let mut host = CustomWrapperPaintHost::canonical(id);
            host.mode = mode;
            let (arena, root, _, properties, generations) = custom_wrapper_fixture(
                host,
                Box::new(leaf_element(id + 1, Color::rgb(1, 2, 3), 1.0, false)),
            );
            let _ = take_full_artifact_record_count();
            assert!(matches!(
                record_frame_artifact(
                    &arena,
                    &[root],
                    &properties,
                    &generations,
                    RendererMode::Auto,
                ),
                Ok(FrameArtifactRecordOutcome::WholeFrameLegacyFallback(_))
            ));
            assert_eq!(take_full_artifact_record_count(), 0);
        }
    }

    #[test]
    fn custom_wrapper_topology_properties_and_unknown_child_fail_closed() {
        let (mut topology_arena, topology_root, _, _, _) = custom_wrapper_fixture(
            CustomWrapperPaintHost::canonical(0x8f70),
            Box::new(leaf_element(0x8f71, Color::rgb(1, 2, 3), 1.0, false)),
        );
        topology_arena.set_children(topology_root, Vec::new());
        assert_eq!(
            topology_arena
                .get(topology_root)
                .unwrap()
                .element
                .shadow_paint_recording_capability(
                    &topology_arena,
                    false,
                    PaintRecordingContext::default(),
                ),
            ShadowPaintRecordingCapability::Unsupported
        );

        for contents in [false, true] {
            let (arena, root, _, mut properties, generations) = custom_wrapper_fixture(
                CustomWrapperPaintHost::canonical(0x8f72 + u64::from(contents)),
                Box::new(leaf_element(
                    0x8f74 + u64::from(contents),
                    Color::rgb(1, 2, 3),
                    1.0,
                    false,
                )),
            );
            let state = properties.states.get_mut(&root).unwrap();
            if contents {
                state.descendants.transform = Some(TransformNodeId(root));
            } else {
                state.paint.transform = Some(TransformNodeId(root));
            }
            let _ = take_full_artifact_record_count();
            assert!(matches!(
                record_frame_artifact(
                    &arena,
                    &[root],
                    &properties,
                    &generations,
                    RendererMode::Auto,
                ),
                Ok(FrameArtifactRecordOutcome::WholeFrameLegacyFallback(_))
            ));
            assert_eq!(take_full_artifact_record_count(), 0);
        }

        let builds = Arc::new(AtomicUsize::new(0));
        let (arena, root, _, properties, generations) = custom_wrapper_fixture(
            CustomWrapperPaintHost::canonical(0x8f76),
            Box::new(RecordingHost {
                id: 0x8f77,
                builds,
                fill: None,
            }),
        );
        let _ = take_full_artifact_record_count();
        assert!(matches!(
            record_frame_artifact(
                &arena,
                &[root],
                &properties,
                &generations,
                RendererMode::Auto,
            ),
            Ok(FrameArtifactRecordOutcome::WholeFrameLegacyFallback(_))
        ));
        assert_eq!(take_full_artifact_record_count(), 0);
    }

    fn malformed_host(
        malformed: MalformedChunk,
    ) -> (
        NodeArena,
        NodeKey,
        Arc<AtomicUsize>,
        PropertyTrees,
        PaintGenerationTracker,
    ) {
        let full_records = Arc::new(AtomicUsize::new(0));
        let mut arena = new_test_arena();
        let root = commit_element(
            &mut arena,
            Box::new(MalformedRecordingHost {
                id: 45,
                malformed,
                full_records: full_records.clone(),
            }),
        );
        let (properties, generations) = sync_identity(&arena, &[root]);
        (arena, root, full_records, properties, generations)
    }

    #[test]
    fn malformed_metadata_properties_and_revision_fail_preflight_without_full_hooks() {
        for (malformed, expected) in [
            (
                MalformedChunk::MetadataProperties,
                PaintCoverageValidationError::InvalidChunkProperties
                    as fn(NodeKey) -> PaintCoverageValidationError,
            ),
            (
                MalformedChunk::MetadataRevision,
                PaintCoverageValidationError::InvalidChunkRevision,
            ),
        ] {
            let (arena, root, full_records, properties, generations) = malformed_host(malformed);
            let error = record_frame_artifact(
                &arena,
                &[root],
                &properties,
                &generations,
                RendererMode::ForcedForTests,
            )
            .expect_err("malformed metadata must fail preflight");
            assert_eq!(full_records.load(Ordering::Relaxed), 0);
            assert!(
                error
                    .reasons
                    .contains(&FrameArtifactFallbackReason::Validation(expected(root)))
            );
        }
    }

    #[test]
    fn malformed_metadata_bounds_fail_preflight_without_full_hooks() {
        for malformed in [
            MalformedChunk::MetadataNaNBounds,
            MalformedChunk::MetadataNegativeBounds,
        ] {
            let (arena, root, full_records, properties, generations) = malformed_host(malformed);
            let error = record_frame_artifact(
                &arena,
                &[root],
                &properties,
                &generations,
                RendererMode::ForcedForTests,
            )
            .expect_err("non-canonical metadata bounds must fail preflight");
            assert_eq!(full_records.load(Ordering::Relaxed), 0);
            assert!(
                error
                    .reasons
                    .contains(&FrameArtifactFallbackReason::Validation(
                        PaintCoverageValidationError::InvalidChunkBounds(root)
                    ))
            );
        }
    }

    #[test]
    fn malformed_full_owner_properties_revision_and_range_fail_closed() {
        for (malformed, expected) in [
            (
                MalformedChunk::FullOwner,
                PaintCoverageValidationError::InvalidChunkIdOwner
                    as fn(NodeKey) -> PaintCoverageValidationError,
            ),
            (
                MalformedChunk::FullChunkOwner,
                PaintCoverageValidationError::InvalidChunkOwner,
            ),
            (
                MalformedChunk::FullProperties,
                PaintCoverageValidationError::InvalidChunkProperties,
            ),
            (
                MalformedChunk::FullRevision,
                PaintCoverageValidationError::InvalidChunkRevision,
            ),
        ] {
            let (arena, root, full_records, properties, generations) = malformed_host(malformed);
            let error = record_frame_artifact(
                &arena,
                &[root],
                &properties,
                &generations,
                RendererMode::ForcedForTests,
            )
            .expect_err("malformed full chunk must fail closed");
            assert_eq!(full_records.load(Ordering::Relaxed), 1);
            assert!(
                error
                    .reasons
                    .contains(&FrameArtifactFallbackReason::Validation(expected(root)))
            );
        }

        let (arena, root, full_records, properties, generations) =
            malformed_host(MalformedChunk::FullRange);
        let error = record_frame_artifact(
            &arena,
            &[root],
            &properties,
            &generations,
            RendererMode::ForcedForTests,
        )
        .expect_err("malformed range must fail closed");
        assert_eq!(full_records.load(Ordering::Relaxed), 1);
        assert!(
            error
                .reasons
                .contains(&FrameArtifactFallbackReason::Validation(
                    PaintCoverageValidationError::InvalidArtifactOpRange {
                        node: root,
                        start: 1,
                        end: 0,
                        op_count: 0,
                    }
                ))
        );
    }

    #[test]
    fn canonical_preflight_full_mismatch_fails_closed() {
        let (arena, root, full_records, properties, generations) =
            malformed_host(MalformedChunk::FullBounds);
        let error = record_frame_artifact(
            &arena,
            &[root],
            &properties,
            &generations,
            RendererMode::ForcedForTests,
        )
        .expect_err("canonical identity drift must fail closed");
        assert_eq!(full_records.load(Ordering::Relaxed), 1);
        assert_eq!(
            error.reasons,
            vec![FrameArtifactFallbackReason::Validation(
                PaintCoverageValidationError::RecordingPassMismatch
            )]
        );
    }

    #[test]
    fn compiler_validates_entire_store_before_emitting_any_pass() {
        let (arena, root, properties, generations) =
            prepared_leaf(46, Color::rgb(20, 30, 40), 1.0, false);
        let PaintRecordOutcome::Artifact(mut artifact) =
            record_root(&arena, root, &properties, &generations)
        else {
            panic!("safe leaf must record")
        };
        let mut malformed_late_chunk = artifact.chunks[0].clone();
        malformed_late_chunk.op_range = 2..1;
        artifact.chunks.push(malformed_late_chunk);

        let graph = compiled_whole_frame_graph(&artifact);
        assert!(
            graph.test_rect_pass_snapshots().is_empty(),
            "a malformed later chunk must not leave an earlier partial pass"
        );
    }

    fn compiler_test_artifact() -> PaintArtifact {
        let (arena, root, properties, generations) =
            prepared_leaf(47, Color::rgb(20, 30, 40), 1.0, false);
        let PaintRecordOutcome::Artifact(artifact) =
            record_root(&arena, root, &properties, &generations)
        else {
            panic!("safe leaf must record")
        };
        artifact
    }

    fn rect_phase_op(x: f32, color: [f32; 4]) -> DrawRectOp {
        DrawRectOp {
            params: RectPassParams {
                position: [x, 2.0],
                size: [5.0, 7.0],
                fill_color: color,
                opacity: 1.0,
                ..RectPassParams::default()
            },
            mode: crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly,
        }
    }

    fn compiler_rect_phase_artifact(role: PaintChunkRole, rects: Vec<DrawRectOp>) -> PaintArtifact {
        let mut artifact = compiler_test_artifact();
        let mut chunk = artifact.chunks[0].clone();
        chunk.id.role = role;
        chunk.op_range = 0..rects.len();
        chunk.payload_identity = PaintPayloadIdentity::prepared_rects(rects.iter())
            .expect("rect phase fixture must have canonical identity");
        artifact.ops = rects.into_iter().map(PaintOp::DrawRect).collect();
        artifact.chunks = vec![chunk];
        artifact
    }

    fn refresh_rect_phase_identity(artifact: &mut PaintArtifact) {
        let range = artifact.chunks[0].op_range.clone();
        artifact.chunks[0].payload_identity = PaintPayloadIdentity::prepared_rects(
            artifact.ops[range].iter().filter_map(|op| match op {
                PaintOp::DrawRect(rect) => Some(rect),
                _ => None,
            }),
        )
        .expect("remaining rect parameters must stay canonical");
    }

    #[test]
    fn generic_rect_phase_roles_compile_in_frozen_chunk_and_op_order() {
        let mut artifact = compiler_test_artifact();
        let template = artifact.chunks[0].clone();
        let rects = [
            rect_phase_op(1.0, [1.0, 0.0, 0.0, 1.0]),
            rect_phase_op(2.0, [0.0, 1.0, 0.0, 1.0]),
            rect_phase_op(3.0, [0.0, 0.0, 1.0, 1.0]),
            rect_phase_op(4.0, [1.0, 1.0, 0.0, 1.0]),
            rect_phase_op(5.0, [1.0, 0.0, 1.0, 1.0]),
        ];
        artifact.ops = rects.iter().cloned().map(PaintOp::DrawRect).collect();
        artifact.chunks = [
            (
                PaintChunkRole::SelectionUnderlay,
                PaintNodePhase::BeforeChildren,
                0,
                0..2,
            ),
            (
                PaintChunkRole::TextDecoration,
                PaintNodePhase::AfterChildren,
                0,
                2..4,
            ),
            (
                PaintChunkRole::Caret,
                PaintNodePhase::AfterChildren,
                1,
                4..5,
            ),
        ]
        .into_iter()
        .map(|(role, phase, slot, range)| {
            let mut chunk = template.clone();
            chunk.id.role = role;
            chunk.id.phase = phase;
            chunk.id.slot = slot;
            chunk.op_range = range.clone();
            chunk.payload_identity =
                PaintPayloadIdentity::prepared_rects(artifact.ops[range].iter().filter_map(|op| {
                    match op {
                        PaintOp::DrawRect(rect) => Some(rect),
                        _ => None,
                    }
                }))
                .unwrap();
            chunk
        })
        .collect();

        assert_eq!(
            artifact
                .chunks
                .iter()
                .map(|chunk| (chunk.id.role, chunk.id.phase, chunk.id.slot))
                .collect::<Vec<_>>(),
            vec![
                (
                    PaintChunkRole::SelectionUnderlay,
                    PaintNodePhase::BeforeChildren,
                    0,
                ),
                (
                    PaintChunkRole::TextDecoration,
                    PaintNodePhase::AfterChildren,
                    0,
                ),
                (PaintChunkRole::Caret, PaintNodePhase::AfterChildren, 1,),
            ]
        );
        let graph = compiled_whole_frame_graph(&artifact);
        let snapshots = graph.test_rect_pass_snapshots();
        assert_eq!(snapshots.len(), 5);
        assert_eq!(
            snapshots
                .iter()
                .map(|snapshot| snapshot.fill_color_bits)
                .collect::<Vec<_>>(),
            rects
                .iter()
                .map(|rect| rect.params.fill_color.map(f32::to_bits))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn compiler_rejects_every_invalid_generic_rect_phase_before_emit() {
        let mut empty = compiler_rect_phase_artifact(
            PaintChunkRole::SelectionUnderlay,
            vec![rect_phase_op(1.0, [1.0, 0.0, 0.0, 1.0])],
        );
        empty.ops.clear();
        empty.chunks[0].op_range = 0..0;
        empty.chunks[0].payload_identity =
            PaintPayloadIdentity::prepared_rects(std::iter::empty()).unwrap();
        assert_compiler_rejects_before_emit(&empty, "empty generic rect phase");

        let caret_multi = compiler_rect_phase_artifact(
            PaintChunkRole::Caret,
            vec![
                rect_phase_op(1.0, [1.0, 0.0, 0.0, 1.0]),
                rect_phase_op(2.0, [0.0, 1.0, 0.0, 1.0]),
            ],
        );
        assert_compiler_rejects_before_emit(&caret_multi, "multi-rect caret");

        let mut wrong_mode = compiler_rect_phase_artifact(
            PaintChunkRole::TextDecoration,
            vec![rect_phase_op(1.0, [1.0, 0.0, 0.0, 1.0])],
        );
        let PaintOp::DrawRect(rect) = &mut wrong_mode.ops[0] else {
            unreachable!()
        };
        rect.mode = crate::view::render_pass::draw_rect_pass::RectRenderMode::BorderOnly;
        refresh_rect_phase_identity(&mut wrong_mode);
        assert_compiler_rejects_before_emit(&wrong_mode, "non-FillOnly rect phase");

        let mut mixed = compiler_rect_phase_artifact(
            PaintChunkRole::SelectionUnderlay,
            vec![
                rect_phase_op(1.0, [1.0, 0.0, 0.0, 1.0]),
                rect_phase_op(2.0, [0.0, 1.0, 0.0, 1.0]),
            ],
        );
        let image = compiler_image_test_artifact(false);
        mixed.ops[1] = image.ops[0].clone();
        assert_compiler_rejects_before_emit(&mixed, "mixed rect and non-rect phase");

        let mut reordered = compiler_rect_phase_artifact(
            PaintChunkRole::TextDecoration,
            vec![
                rect_phase_op(1.0, [1.0, 0.0, 0.0, 1.0]),
                rect_phase_op(2.0, [0.0, 1.0, 0.0, 1.0]),
            ],
        );
        reordered.chunks[0].payload_identity = PaintPayloadIdentity::prepared_rects(
            reordered.ops.iter().rev().filter_map(|op| match op {
                PaintOp::DrawRect(rect) => Some(rect),
                _ => None,
            }),
        )
        .unwrap();
        assert_compiler_rejects_before_emit(&reordered, "reordered rect identity");

        let mut tampered = compiler_rect_phase_artifact(
            PaintChunkRole::SelectionUnderlay,
            vec![rect_phase_op(1.0, [1.0, 0.0, 0.0, 1.0])],
        );
        let PaintOp::DrawRect(rect) = &mut tampered.ops[0] else {
            unreachable!()
        };
        rect.params.position[0] = f32::from_bits(rect.params.position[0].to_bits() ^ 1);
        assert_compiler_rejects_before_emit(&tampered, "tampered rect params");
    }

    #[test]
    fn compiler_rejects_duplicate_owner_phase_slot_even_when_roles_differ() {
        let mut artifact = compiler_test_artifact();
        let mut duplicate_slot = artifact.chunks[0].clone();
        duplicate_slot.id.role = PaintChunkRole::TextGlyphs;
        duplicate_slot.op_range = artifact.ops.len()..artifact.ops.len();
        duplicate_slot.payload_identity =
            PaintPayloadIdentity::prepared_texts(std::iter::empty::<&PreparedTextOp>());
        artifact.chunks.push(duplicate_slot);

        assert_compiler_rejects_before_emit(
            &artifact,
            "duplicate owner/phase/slot with a distinct role",
        );
    }

    fn compiler_image_test_artifact(with_decoration: bool) -> PaintArtifact {
        let pixels: Arc<[u8]> = Arc::from([
            255_u8, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 0, 255,
        ]);
        let (arena, roots) = if with_decoration {
            prepared_image_fixture(
                pixels,
                crate::view::ImageFit::Fill,
                crate::view::ImageSampling::Linear,
                1.0,
            )
        } else {
            bare_image_fixture(
                pixels,
                crate::view::ImageFit::Fill,
                crate::view::ImageSampling::Linear,
                1.0,
            )
        };
        let (properties, generations) = sync_identity(&arena, &roots);
        whole_frame_artifact(&arena, &roots, &properties, &generations).0
    }

    fn compiler_svg_test_artifact(with_decoration: bool) -> PaintArtifact {
        let mut artifact = compiler_image_test_artifact(with_decoration);
        let chunk = artifact
            .chunks
            .first_mut()
            .expect("image fixture must contain one content chunk");
        chunk.id.role = PaintChunkRole::SvgContent;
        let PaintPayloadIdentity::Image(_, decoration) = &chunk.payload_identity else {
            panic!("image fixture must carry composite identity")
        };
        let decoration = Arc::clone(decoration);
        let prepared = artifact
            .ops
            .last_mut()
            .expect("image fixture must end in a prepared payload");
        let PaintOp::PreparedImage(image) = prepared else {
            panic!("image fixture must end in PreparedImage")
        };
        image.upload.id = crate::view::sampled_texture::SampledTextureId::SvgRaster(
            crate::view::sampled_texture::SvgRasterAssetId::for_test(77),
        );
        let svg = PreparedSvgOp {
            params: image.params,
            upload: image.upload.clone(),
        };
        chunk.payload_identity = PaintPayloadIdentity::Svg(
            PreparedSvgIdentity::from_op(&svg).expect("fixture must have typed SVG identity"),
            decoration,
        );
        *prepared = PaintOp::PreparedSvg(svg);
        artifact
    }

    fn refresh_svg_standard_draw_rect_identity(artifact: &mut PaintArtifact) {
        let prepared = artifact.ops.iter().find_map(|op| match op {
            PaintOp::PreparedSvg(prepared) => Some(prepared),
            _ => None,
        });
        let identity = PreparedSvgIdentity::from_op(prepared.expect("prepared SVG")).unwrap();
        artifact.chunks[0].payload_identity = PaintPayloadIdentity::svg_with_decoration(
            identity,
            artifact.ops.iter().filter_map(|op| match op {
                PaintOp::DrawRect(rect) => Some(rect),
                _ => None,
            }),
        )
        .unwrap();
    }

    #[test]
    fn compiler_rejects_border_only_image_grammar_before_emit() {
        let mut artifact = compiler_image_test_artifact(true);
        assert!(matches!(artifact.ops[0], PaintOp::DrawRect(_)));
        let PaintOp::DrawRect(border) = artifact.ops.remove(1) else {
            panic!("fixture second op must be border")
        };
        artifact.ops.remove(0);
        artifact.ops.insert(0, PaintOp::DrawRect(border));
        artifact.chunks[0].op_range = 0..artifact.ops.len();
        let graph = compiled_whole_frame_graph(&artifact);
        assert_eq!(graph.pass_descriptors().len(), 1, "only clear may remain");
    }

    #[test]
    fn compiler_rejects_late_invalid_prepared_image_before_any_artifact_emit() {
        let mut artifact = compiler_image_test_artifact(false);
        let PaintOp::PreparedImage(mut invalid_op) = artifact.ops[0].clone() else {
            panic!("bare image fixture")
        };
        invalid_op.upload.pixels = Arc::from([1_u8, 2, 3]);
        let identity = PaintPayloadIdentity::image_with_decoration(
            PreparedImageIdentity::from_op(&invalid_op),
            std::iter::empty(),
        )
        .unwrap();
        let mut key_arena = NodeArena::new();
        let second_owner = key_arena.insert(Node::new(Box::new(Element::new_with_id(
            999, 0.0, 0.0, 1.0, 1.0,
        ))));
        let start = artifact.ops.len();
        artifact.ops.push(PaintOp::PreparedImage(invalid_op));
        let mut late = artifact.chunks[0].clone();
        late.id.owner = second_owner;
        late.owner = second_owner;
        late.op_range = start..artifact.ops.len();
        late.payload_identity = identity;
        artifact.chunks.push(late);

        let graph = compiled_whole_frame_graph(&artifact);
        assert_eq!(graph.pass_descriptors().len(), 1, "only clear may remain");
    }

    #[test]
    fn compiler_emits_typed_straight_srgb_svg_as_texture_composite() {
        let artifact = compiler_svg_test_artifact(false);
        assert_eq!(artifact.chunks[0].id.role, PaintChunkRole::SvgContent);
        assert_eq!(artifact.chunks[0].properties, PropertyTreeState::default());

        let mut graph = compiled_whole_frame_graph(&artifact);
        let passes =
            graph.test_graphics_passes_mut::<crate::view::render_pass::TextureCompositePass>();
        assert_eq!(passes.len(), 1);
        let snapshot = passes[0].test_snapshot();
        let upload = snapshot
            .sampled_source
            .expect("SVG artifact must retain an owning sampled upload");
        assert!(matches!(
            upload.id,
            crate::view::sampled_texture::SampledTextureId::SvgRaster(_)
        ));
        assert_eq!(upload.format, wgpu::TextureFormat::Rgba8UnormSrgb);
        assert_eq!(
            upload.alpha_mode,
            crate::view::sampled_texture::SampledTextureAlphaMode::Straight
        );
        assert!(!snapshot.source_is_premultiplied);
        assert!(!snapshot.use_mask);
        assert!(snapshot.quad_position_bits.is_none());
        assert!(snapshot.mask_uv_bounds_bits.is_none());
        assert!(snapshot.explicit_scissor_rect.is_none());
        assert!(snapshot.effective_scissor_rect.is_none());
        assert!(snapshot.uv_bounds_bits.is_some());

        // The strict whole-graph adapter already knows TextureComposite; this
        // must remain a complete structural snapshot rather than an unchecked
        // pass hidden behind the common Image/SVG compiler implementation.
        let _ = strict_paint_snapshot(&mut graph, PaintParityConfig::default());
    }

    #[test]
    fn compiler_accepts_only_svg_decoration_then_payload_grammar() {
        let undecorated = compiler_svg_test_artifact(false);
        assert!(undecorated.ops.len() == 1);
        assert!(
            compiled_whole_frame_graph(&undecorated)
                .pass_descriptors()
                .len()
                > 1
        );

        let decorated = compiler_svg_test_artifact(true);
        assert!(matches!(
            decorated.ops.last(),
            Some(PaintOp::PreparedSvg(_))
        ));
        assert!(
            compiled_whole_frame_graph(&decorated)
                .pass_descriptors()
                .len()
                > 1
        );

        let mut fill_only = decorated.clone();
        fill_only.ops.remove(1);
        fill_only.chunks[0].op_range = 0..fill_only.ops.len();
        refresh_svg_standard_draw_rect_identity(&mut fill_only);
        assert!(
            compiled_whole_frame_graph(&fill_only)
                .pass_descriptors()
                .len()
                > 1,
            "fill-only SVG decoration is a valid grammar prefix"
        );

        let mut border_only = decorated.clone();
        let PaintOp::DrawRect(border) = border_only.ops.remove(1) else {
            panic!("decorated fixture second op must be border")
        };
        border_only.ops.remove(0);
        border_only.ops.insert(0, PaintOp::DrawRect(border));
        border_only.chunks[0].op_range = 0..border_only.ops.len();
        assert_eq!(
            compiled_whole_frame_graph(&border_only)
                .pass_descriptors()
                .len(),
            1,
            "border-only SVG decoration is not a valid grammar prefix"
        );

        let mut payload_not_last = compiler_svg_test_artifact(false);
        payload_not_last.ops.push(PaintOp::DrawRect(DrawRectOp {
            params: RectPassParams::default(),
            mode: crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly,
        }));
        payload_not_last.chunks[0].op_range = 0..payload_not_last.ops.len();
        assert_eq!(
            compiled_whole_frame_graph(&payload_not_last)
                .pass_descriptors()
                .len(),
            1,
            "PreparedSvg must be the final and unique content payload"
        );

        let mut wrong_payload_type = compiler_svg_test_artifact(false);
        let PaintOp::PreparedSvg(svg) = wrong_payload_type.ops.remove(0) else {
            unreachable!()
        };
        wrong_payload_type
            .ops
            .push(PaintOp::PreparedImage(PreparedImageOp {
                params: svg.params,
                upload: svg.upload,
            }));
        assert_eq!(
            compiled_whole_frame_graph(&wrong_payload_type)
                .pass_descriptors()
                .len(),
            1,
            "SvgContent must not accept a PreparedImage payload"
        );
    }

    #[test]
    fn compiler_rejects_standard_draw_rect_composite_identity_drift_before_emit() {
        let (arena, root, properties, generations) =
            prepared_leaf(0x6a70, Color::rgb(20, 80, 160), 1.0, true);
        let (element, _) = whole_frame_artifact(&arena, &[root], &properties, &generations);
        assert!(
            compiled_whole_frame_graph(&element)
                .pass_descriptors()
                .len()
                > 1
        );

        let mut fill_drift = element.clone();
        let fill = fill_drift
            .ops
            .iter_mut()
            .find_map(|op| match op {
                PaintOp::DrawRect(rect)
                    if rect.mode
                        == crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly =>
                {
                    Some(rect)
                }
                _ => None,
            })
            .unwrap();
        fill.params.fill_color[0] = f32::from_bits(fill.params.fill_color[0].to_bits() ^ 1);
        assert_compiler_rejects_before_emit(&fill_drift, "Element fill params drift");

        let mut border_drift = element.clone();
        let border = border_drift
            .ops
            .iter_mut()
            .find_map(|op| match op {
                PaintOp::DrawRect(rect)
                    if rect.mode
                        == crate::view::render_pass::draw_rect_pass::RectRenderMode::BorderOnly =>
                {
                    Some(rect)
                }
                _ => None,
            })
            .unwrap();
        border.params.border_widths[0] =
            f32::from_bits(border.params.border_widths[0].to_bits() ^ 1);
        assert_compiler_rejects_before_emit(&border_drift, "Element border params drift");

        let image = compiler_image_test_artifact(true);
        assert!(compiled_whole_frame_graph(&image).pass_descriptors().len() > 1);
        let mut image_drift = image.clone();
        let PaintOp::DrawRect(rect) = &mut image_drift.ops[0] else {
            panic!("decorated Image must start with DrawRect")
        };
        rect.params.opacity = f32::from_bits(rect.params.opacity.to_bits() ^ 1);
        assert_compiler_rejects_before_emit(&image_drift, "Image decoration drift");

        let svg = compiler_svg_test_artifact(true);
        assert!(compiled_whole_frame_graph(&svg).pass_descriptors().len() > 1);
        let mut svg_drift = svg.clone();
        let PaintOp::DrawRect(rect) = &mut svg_drift.ops[0] else {
            panic!("decorated SVG must start with DrawRect")
        };
        rect.params.position[0] = f32::from_bits(rect.params.position[0].to_bits() ^ 1);
        assert_compiler_rejects_before_emit(&svg_drift, "SVG decoration drift");

        let mut missing = element.clone();
        missing.chunks[0].payload_identity = PaintPayloadIdentity::None;
        assert_compiler_rejects_before_emit(&missing, "missing DrawRect composite identity");

        let mut missing_decoration = element.clone();
        missing_decoration.chunks[0].payload_identity =
            PaintPayloadIdentity::prepared_shadows(std::iter::empty());
        assert_compiler_rejects_before_emit(
            &missing_decoration,
            "content-only identity missing DrawRect composite",
        );

        let mut wrong = element;
        wrong.chunks[0].payload_identity = image.chunks[0].payload_identity.clone();
        assert_compiler_rejects_before_emit(&wrong, "wrong composite identity variant");
    }

    #[test]
    fn standard_draw_rect_identity_accepts_and_freezes_fill_and_border_gradients() {
        let mut arena = new_test_arena();
        let mut element = Element::new_with_id(0x6a71, 0.0, 0.0, 96.0, 48.0);
        apply_gradient_style(&mut element, "#ff0000", "#0000ff", "#ffffff", "#000000");
        let root = commit_element(&mut arena, Box::new(element));
        let (measure, place) = constraints();
        measure_and_place(&mut arena, root, measure, place);
        let (properties, generations) = sync_identity(&arena, &[root]);
        let (artifact, _) = whole_frame_artifact(&arena, &[root], &properties, &generations);
        assert!(
            compiled_whole_frame_graph(&artifact)
                .pass_descriptors()
                .len()
                > 1
        );

        let mut fill_axis_drift = artifact.clone();
        let fill = fill_axis_drift
            .ops
            .iter_mut()
            .find_map(|op| match op {
                PaintOp::DrawRect(rect)
                    if rect.mode
                        == crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly =>
                {
                    Some(rect)
                }
                _ => None,
            })
            .unwrap();
        let gradient = fill.params.gradient.as_mut().expect("fill gradient");
        gradient.axis[0] = f32::from_bits(gradient.axis[0].to_bits() ^ 1);
        assert_compiler_rejects_before_emit(&fill_axis_drift, "fill gradient axis drift");

        let mut border_stop_drift = artifact;
        let border = border_stop_drift
            .ops
            .iter_mut()
            .find_map(|op| match op {
                PaintOp::DrawRect(rect)
                    if rect.mode
                        == crate::view::render_pass::draw_rect_pass::RectRenderMode::BorderOnly =>
                {
                    Some(rect)
                }
                _ => None,
            })
            .unwrap();
        let gradient = border
            .params
            .border_gradient
            .as_mut()
            .expect("border gradient");
        let stops = Arc::make_mut(&mut gradient.stops);
        stops[0].color[0] = f32::from_bits(stops[0].color[0].to_bits() ^ 1);
        assert_compiler_rejects_before_emit(&border_stop_drift, "border gradient stop drift");
    }

    #[test]
    fn compiler_rejects_svg_identity_drift_and_wrong_asset_namespace() {
        let valid = compiler_svg_test_artifact(false);

        let mut drift = valid.clone();
        let PaintPayloadIdentity::Svg(mut identity, decoration) =
            drift.chunks[0].payload_identity.clone()
        else {
            panic!("SVG fixture identity")
        };
        identity.opacity_bits ^= 1;
        drift.chunks[0].payload_identity = PaintPayloadIdentity::Svg(identity, decoration);
        assert_eq!(
            compiled_whole_frame_graph(&drift).pass_descriptors().len(),
            1
        );

        let mut wrong_namespace = valid;
        let PaintOp::PreparedSvg(prepared) = &mut wrong_namespace.ops[0] else {
            panic!("SVG fixture payload")
        };
        prepared.upload.id = crate::view::sampled_texture::SampledTextureId::Image(
            crate::view::sampled_texture::ImageAssetId::for_test(77),
        );
        assert_eq!(
            compiled_whole_frame_graph(&wrong_namespace)
                .pass_descriptors()
                .len(),
            1,
            "SVG content must never accept an Image asset id"
        );
    }

    #[test]
    fn compiler_rejects_every_unsupported_svg_composite_input_and_property() {
        let valid = compiler_svg_test_artifact(false);
        let mut cases = Vec::new();

        let mut invalid = valid.clone();
        let PaintOp::PreparedSvg(op) = &mut invalid.ops[0] else {
            unreachable!()
        };
        op.params.source_is_premultiplied = true;
        cases.push(("premultiplied source", invalid));

        let mut invalid = valid.clone();
        let PaintOp::PreparedSvg(op) = &mut invalid.ops[0] else {
            unreachable!()
        };
        op.params.use_mask = true;
        cases.push(("mask", invalid));

        let mut invalid = valid.clone();
        let PaintOp::PreparedSvg(op) = &mut invalid.ops[0] else {
            unreachable!()
        };
        op.params.mask_uv_bounds = Some([0.0, 0.0, 1.0, 1.0]);
        cases.push(("mask UV", invalid));

        let mut invalid = valid.clone();
        let PaintOp::PreparedSvg(op) = &mut invalid.ops[0] else {
            unreachable!()
        };
        op.params.quad_positions = Some([[0.0; 2]; 4]);
        cases.push(("quad", invalid));

        let mut invalid = valid.clone();
        let PaintOp::PreparedSvg(op) = &mut invalid.ops[0] else {
            unreachable!()
        };
        op.params.scissor_rect = Some([0, 0, 1, 1]);
        cases.push(("op scissor", invalid));

        let mut invalid = valid.clone();
        let PaintOp::PreparedSvg(op) = &mut invalid.ops[0] else {
            unreachable!()
        };
        op.params.uv_bounds = None;
        cases.push(("missing source UV", invalid));

        let mut invalid = valid.clone();
        let PaintOp::PreparedSvg(op) = &mut invalid.ops[0] else {
            unreachable!()
        };
        op.upload.format = wgpu::TextureFormat::Rgba8Unorm;
        cases.push(("non-sRGB upload", invalid));

        let mut invalid = valid.clone();
        invalid.chunks[0].properties.transform = Some(TransformNodeId(NodeKey::null()));
        cases.push(("transform property", invalid));

        let mut invalid = valid.clone();
        invalid.chunks[0].properties.clip = Some(ClipNodeId {
            owner: invalid.chunks[0].owner,
            role: ClipNodeRole::SelfClip,
        });
        cases.push(("clip property", invalid));

        let mut invalid = valid;
        invalid.chunks[0].properties.scroll = Some(
            crate::view::compositor::property_tree::ScrollNodeId(NodeKey::null()),
        );
        cases.push(("scroll property", invalid));

        for (name, invalid) in cases {
            assert_eq!(
                compiled_whole_frame_graph(&invalid)
                    .pass_descriptors()
                    .len(),
                1,
                "unsupported SVG {name} must fail closed"
            );
        }
    }

    #[test]
    fn compiler_prevalidates_late_invalid_svg_before_emitting_earlier_svg() {
        let mut artifact = compiler_svg_test_artifact(false);
        let PaintOp::PreparedSvg(mut invalid_op) = artifact.ops[0].clone() else {
            panic!("SVG fixture payload")
        };
        invalid_op.upload.pixels = Arc::from([1_u8, 2, 3]);
        let mut key_arena = NodeArena::new();
        let second_owner = key_arena.insert(Node::new(Box::new(Element::new_with_id(
            1001, 0.0, 0.0, 1.0, 1.0,
        ))));
        let start = artifact.ops.len();
        artifact.ops.push(PaintOp::PreparedSvg(invalid_op));
        let mut late = artifact.chunks[0].clone();
        late.id.owner = second_owner;
        late.owner = second_owner;
        late.op_range = start..artifact.ops.len();
        artifact.chunks.push(late);

        assert_eq!(
            compiled_whole_frame_graph(&artifact)
                .pass_descriptors()
                .len(),
            1,
            "late invalid SVG must leave only the pre-existing clear pass"
        );
    }

    fn compiler_clip_test_artifact() -> PaintArtifact {
        let (arena, roots) = anchor_parent_self_clip_roots(1.0, false);
        let (properties, generations) = sync_identity(&arena, &roots);
        let artifact = whole_frame_artifact(&arena, &roots, &properties, &generations).0;
        assert_eq!(artifact.clip_nodes.len(), 1);
        assert!(artifact.chunks[0].properties.clip.is_some());
        assert!(artifact.chunks[1].properties.clip.is_none());
        artifact
    }

    fn unique_synthetic_owner(artifact: &PaintArtifact) -> NodeKey {
        let mut arena = NodeArena::new();
        loop {
            let key = arena.insert(Node::new(Box::new(Element::new_with_id(
                0x8c00 + arena.len() as u64,
                0.0,
                0.0,
                1.0,
                1.0,
            ))));
            if artifact
                .owner_nodes
                .iter()
                .all(|snapshot| snapshot.owner != key)
            {
                return key;
            }
        }
    }

    fn add_inherited_contents_clip(
        artifact: &mut PaintArtifact,
        logical_scissor: [u32; 4],
    ) -> ClipNodeId {
        let owner = unique_synthetic_owner(artifact);
        for snapshot in &mut artifact.owner_nodes {
            if snapshot.parent.is_none() {
                snapshot.parent = Some(owner);
            }
        }
        artifact.owner_nodes.push(PaintOwnerSnapshot {
            owner,
            parent: None,
        });
        let id = ClipNodeId {
            owner,
            role: ClipNodeRole::ContentsClip,
        };
        artifact.clip_nodes.push(ClipNodeSnapshot {
            id,
            owner,
            parent: None,
            logical_scissor,
            behavior: ClipBehavior::Intersect,
            generation: 1,
        });
        for chunk in &mut artifact.chunks {
            chunk.properties.clip = Some(id);
        }
        id
    }

    #[test]
    fn image_and_svg_compile_with_validated_inherited_contents_clip() {
        for (name, mut artifact) in [
            ("image", compiler_image_test_artifact(false)),
            ("svg", compiler_svg_test_artifact(false)),
        ] {
            add_inherited_contents_clip(&mut artifact, [2, 3, 20, 10]);
            let mut graph = compiled_whole_frame_graph(&artifact);
            let _ = strict_paint_snapshot(&mut graph, PaintParityConfig::default());
            let passes =
                graph.test_graphics_passes_mut::<crate::view::render_pass::TextureCompositePass>();
            assert_eq!(passes.len(), 1, "{name} inherited clip must compile");
            let snapshot = passes[0].test_snapshot();
            assert!(snapshot.explicit_scissor_rect.is_none());
            assert_eq!(snapshot.effective_scissor_rect, Some([2, 3, 20, 10]));
        }
    }

    #[test]
    fn contents_clip_intersects_ancestor_replace_and_explicit_empty_culls() {
        let mut artifact = compiler_image_test_artifact(false);
        let contents = add_inherited_contents_clip(&mut artifact, [20, 30, 80, 80]);
        let outer_owner = unique_synthetic_owner(&artifact);
        artifact
            .owner_nodes
            .iter_mut()
            .find(|snapshot| snapshot.owner == contents.owner)
            .expect("contents owner")
            .parent = Some(outer_owner);
        artifact.owner_nodes.push(PaintOwnerSnapshot {
            owner: outer_owner,
            parent: None,
        });
        let outer = ClipNodeId {
            owner: outer_owner,
            role: ClipNodeRole::SelfClip,
        };
        artifact
            .clip_nodes
            .iter_mut()
            .find(|snapshot| snapshot.id == contents)
            .expect("contents clip")
            .parent = Some(outer);
        artifact.clip_nodes.push(ClipNodeSnapshot {
            id: outer,
            owner: outer_owner,
            parent: None,
            logical_scissor: [0, 0, 50, 60],
            behavior: ClipBehavior::Replace,
            generation: 1,
        });

        let mut graph = compiled_whole_frame_graph(&artifact);
        let _ = strict_paint_snapshot(&mut graph, PaintParityConfig::default());
        let passes =
            graph.test_graphics_passes_mut::<crate::view::render_pass::TextureCompositePass>();
        assert_eq!(passes.len(), 1);
        assert_eq!(
            passes[0].test_snapshot().effective_scissor_rect,
            Some([20, 30, 30, 30])
        );

        let mut empty = artifact;
        empty
            .clip_nodes
            .iter_mut()
            .find(|snapshot| snapshot.id == contents)
            .expect("contents clip")
            .logical_scissor = [20, 30, 0, 0];
        let mut graph = compiled_whole_frame_graph(&empty);
        assert!(
            graph
                .test_graphics_passes_mut::<crate::view::render_pass::TextureCompositePass>()
                .is_empty(),
            "explicit empty contents clip must suppress the clipped pass"
        );
    }

    #[test]
    fn nested_self_replace_escapes_ancestor_contents_intersection() {
        let mut artifact = compiler_image_test_artifact(false);
        let leaf = artifact.chunks[0].owner;
        let outer_owner = unique_synthetic_owner(&artifact);
        artifact
            .owner_nodes
            .iter_mut()
            .find(|snapshot| snapshot.owner == leaf)
            .unwrap()
            .parent = Some(outer_owner);
        artifact.owner_nodes.push(PaintOwnerSnapshot {
            owner: outer_owner,
            parent: None,
        });
        let contents = ClipNodeId {
            owner: outer_owner,
            role: ClipNodeRole::ContentsClip,
        };
        let own = ClipNodeId {
            owner: leaf,
            role: ClipNodeRole::SelfClip,
        };
        artifact.clip_nodes.extend([
            ClipNodeSnapshot {
                id: contents,
                owner: outer_owner,
                parent: None,
                logical_scissor: [20, 30, 10, 10],
                behavior: ClipBehavior::Intersect,
                generation: 1,
            },
            ClipNodeSnapshot {
                id: own,
                owner: leaf,
                parent: Some(contents),
                logical_scissor: [5, 6, 80, 70],
                behavior: ClipBehavior::Replace,
                generation: 1,
            },
        ]);
        artifact.chunks[0].properties.clip = Some(own);

        let mut graph = compiled_whole_frame_graph(&artifact);
        let _ = strict_paint_snapshot(&mut graph, PaintParityConfig::default());
        let passes =
            graph.test_graphics_passes_mut::<crate::view::render_pass::TextureCompositePass>();
        assert_eq!(passes.len(), 1);
        assert_eq!(
            passes[0].test_snapshot().effective_scissor_rect,
            Some([5, 6, 80, 70])
        );
    }

    #[test]
    fn compiler_rejects_invalid_contents_clip_role_behavior_and_ancestry() {
        let mut valid = compiler_image_test_artifact(false);
        let contents = add_inherited_contents_clip(&mut valid, [1, 2, 3, 4]);

        let mut wrong_behavior = valid.clone();
        wrong_behavior.clip_nodes[0].behavior = ClipBehavior::Replace;
        assert_compiler_rejects_before_emit(&wrong_behavior, "contents clip replace behavior");

        let mut wrong_role = valid.clone();
        wrong_role.clip_nodes[0].id.role = ClipNodeRole::SelfClip;
        wrong_role.chunks[0].properties.clip = Some(wrong_role.clip_nodes[0].id);
        assert_compiler_rejects_before_emit(&wrong_role, "intersect self-clip role");

        let mut wrong_owner = valid;
        let unrelated = unique_synthetic_owner(&wrong_owner);
        wrong_owner.clip_nodes[0].id.owner = unrelated;
        wrong_owner.clip_nodes[0].owner = unrelated;
        wrong_owner.chunks[0].properties.clip = Some(wrong_owner.clip_nodes[0].id);
        assert_ne!(unrelated, contents.owner);
        assert_compiler_rejects_before_emit(&wrong_owner, "clip owner outside chunk ancestry");
    }

    fn compiler_effect_test_artifact(
        parent_opacity: f32,
        child_opacity: f32,
    ) -> (PaintArtifact, NodeKey, NodeKey) {
        let mut arena = new_test_arena();
        let parent = commit_element(
            &mut arena,
            Box::new(leaf_element(
                0x6b00,
                Color::rgb(200, 20, 30),
                parent_opacity,
                false,
            )),
        );
        let mut child_element = leaf_element(0x6b01, Color::rgb(20, 200, 30), child_opacity, false);
        child_element.set_position(0.0, 0.0);
        let child = commit_child(&mut arena, parent, Box::new(child_element));
        let (measure, place) = constraints();
        measure_and_place(&mut arena, parent, measure, place);
        let roots = [parent];
        let (properties, generations) = sync_identity(&arena, &roots);
        let artifact = whole_frame_artifact(&arena, &roots, &properties, &generations).0;
        (artifact, parent, child)
    }

    fn compiler_sibling_effect_artifact() -> (PaintArtifact, NodeKey, NodeKey, NodeKey) {
        let mut arena = new_test_arena();
        let first = commit_element(
            &mut arena,
            Box::new(leaf_element(0x6b21, Color::rgb(40, 50, 60), 0.5, false)),
        );
        let second = commit_element(
            &mut arena,
            Box::new(leaf_element(0x6b22, Color::rgb(70, 80, 90), 0.25, false)),
        );
        let synthetic_parent = arena.insert(Node::new(Box::new(Element::new_with_id(
            0x6b20, 0.0, 0.0, 1.0, 1.0,
        ))));
        let (measure, place) = constraints();
        measure_and_place(&mut arena, first, measure, place);
        measure_and_place(&mut arena, second, measure, place);
        let roots = [first, second];
        let (properties, generations) = sync_identity(&arena, &roots);
        let mut artifact = whole_frame_artifact(&arena, &roots, &properties, &generations).0;
        for snapshot in &mut artifact.owner_nodes {
            snapshot.parent = Some(synthetic_parent);
        }
        artifact.owner_nodes.push(PaintOwnerSnapshot {
            owner: synthetic_parent,
            parent: None,
        });
        // This compiler-only fixture models two canonical siblings without
        // introducing Element child-clip eligibility concerns.
        assert_eq!(
            compiled_whole_frame_graph(&artifact)
                .test_rect_pass_snapshots()
                .len(),
            2
        );
        (artifact, synthetic_parent, first, second)
    }

    fn compiler_three_level_effect_artifact() -> (PaintArtifact, NodeKey, NodeKey, NodeKey) {
        let mut arena = new_test_arena();
        let grandparent = commit_element(
            &mut arena,
            Box::new(leaf_element(0x6b30, Color::rgb(10, 20, 30), 0.5, false)),
        );
        let mut parent_element = leaf_element(0x6b31, Color::rgb(40, 50, 60), 0.25, false);
        parent_element.set_position(0.0, 0.0);
        let parent = commit_child(&mut arena, grandparent, Box::new(parent_element));
        let mut child_element = leaf_element(0x6b32, Color::rgb(70, 80, 90), 1.0, false);
        child_element.set_position(0.0, 0.0);
        let child = commit_child(&mut arena, parent, Box::new(child_element));
        let (measure, place) = constraints();
        measure_and_place(&mut arena, grandparent, measure, place);
        let roots = [grandparent];
        let (properties, generations) = sync_identity(&arena, &roots);
        let artifact = whole_frame_artifact(&arena, &roots, &properties, &generations).0;
        (artifact, grandparent, parent, child)
    }

    fn assert_compiler_rejects_before_emit(artifact: &PaintArtifact, case: &str) {
        let graph = compiled_whole_frame_graph(artifact);
        assert!(
            graph.test_rect_pass_snapshots().is_empty(),
            "{case} must reject the entire store before the first pass"
        );
    }

    fn refresh_inline_decoration_payload_identity(artifact: &mut PaintArtifact) {
        let range = artifact.chunks[0].op_range.clone();
        let identity = PaintPayloadIdentity::inline_ifc_decorations(
            artifact.ops[range].iter().filter_map(|op| match op {
                PaintOp::PreparedInlineIfcDecoration(prepared) => Some(prepared),
                _ => None,
            }),
        );
        artifact.chunks[0].payload_identity = identity;
    }

    #[test]
    fn compiler_rejects_every_invalid_clip_snapshot_before_emit() {
        let valid = compiler_clip_test_artifact();

        let mut missing_leaf = valid.clone();
        missing_leaf.clip_nodes.clear();
        assert_compiler_rejects_before_emit(&missing_leaf, "missing clip leaf");

        let mut dangling_parent = valid.clone();
        dangling_parent.clip_nodes[0].parent = Some(ClipNodeId {
            owner: NodeKey::null(),
            role: ClipNodeRole::SelfClip,
        });
        assert_compiler_rejects_before_emit(&dangling_parent, "dangling clip parent");

        let mut cycle = valid.clone();
        cycle.clip_nodes[0].parent = Some(cycle.clip_nodes[0].id);
        assert_compiler_rejects_before_emit(&cycle, "clip cycle");

        let mut invalid_owner = valid.clone();
        invalid_owner.clip_nodes[0].owner = NodeKey::null();
        assert_compiler_rejects_before_emit(&invalid_owner, "invalid clip owner");

        let mut invalid_role = valid.clone();
        invalid_role.clip_nodes[0].id.role = ClipNodeRole::ContentsClip;
        invalid_role.chunks[0].properties.clip = Some(invalid_role.clip_nodes[0].id);
        assert_compiler_rejects_before_emit(&invalid_role, "invalid clip role");

        let mut invalid_generation = valid.clone();
        invalid_generation.clip_nodes[0].generation = 0;
        assert_compiler_rejects_before_emit(&invalid_generation, "invalid clip generation");

        let mut excessive_depth = valid;
        let leaf_id = excessive_depth.clip_nodes[0].id;
        let mut key_arena = NodeArena::new();
        let mut ids = Vec::new();
        while ids.len() < usize::from(u8::MAX) {
            let key = key_arena.insert(Node::new(Box::new(Element::new_with_id(
                10_000 + ids.len() as u64,
                0.0,
                0.0,
                1.0,
                1.0,
            ))));
            let id = ClipNodeId {
                owner: key,
                role: ClipNodeRole::SelfClip,
            };
            if id != leaf_id {
                ids.push(id);
            }
        }
        excessive_depth.clip_nodes[0].parent = Some(ids[0]);
        for (index, id) in ids.iter().copied().enumerate() {
            excessive_depth.clip_nodes.push(ClipNodeSnapshot {
                id,
                owner: id.owner,
                parent: ids.get(index + 1).copied(),
                logical_scissor: [0, 0, 320, 240],
                behavior: ClipBehavior::Replace,
                generation: 1,
            });
        }
        assert_compiler_rejects_before_emit(&excessive_depth, "clip depth above 255");
    }

    #[test]
    fn effect_snapshots_preserve_zero_half_one_and_parent_child_baked_semantics() {
        let (nested, parent, child) = compiler_effect_test_artifact(0.5, 0.25);
        assert_eq!(nested.effect_nodes.len(), 2);
        let parent_snapshot = nested
            .effect_nodes
            .iter()
            .find(|snapshot| snapshot.owner == parent)
            .expect("parent effect snapshot");
        let child_snapshot = nested
            .effect_nodes
            .iter()
            .find(|snapshot| snapshot.owner == child)
            .expect("child effect snapshot");
        assert_eq!(parent_snapshot.opacity.to_bits(), 0.5_f32.to_bits());
        assert_eq!(child_snapshot.opacity.to_bits(), 0.25_f32.to_bits());
        assert_eq!(child_snapshot.parent, Some(parent_snapshot.id));
        assert_eq!(
            compiled_whole_frame_graph(&nested)
                .test_rect_pass_snapshots()
                .len(),
            2
        );

        let (inherited, _, inherited_child) = compiler_effect_test_artifact(0.5, 1.0);
        assert_eq!(inherited.effect_nodes.len(), 1);
        let child_chunk = inherited
            .chunks
            .iter()
            .find(|chunk| chunk.owner == inherited_child)
            .expect("inherited child chunk");
        let PaintOp::DrawRect(child_op) = &inherited.ops[child_chunk.op_range.start] else {
            panic!("Element child must record DrawRect")
        };
        assert_eq!(child_op.params.opacity.to_bits(), 1.0_f32.to_bits());
        assert_eq!(
            compiled_whole_frame_graph(&inherited)
                .test_rect_pass_snapshots()
                .len(),
            2
        );

        let (zero, _, _) = compiler_effect_test_artifact(0.0, 1.0);
        assert!(
            zero.effect_nodes
                .iter()
                .any(|snapshot| snapshot.opacity.to_bits() == 0.0_f32.to_bits())
        );
        let zero_snapshots = compiled_whole_frame_graph(&zero).test_rect_pass_snapshots();
        assert_eq!(zero_snapshots.len(), 2);
        assert_eq!(zero_snapshots[0].opacity_bits, 0.0_f32.to_bits());
        assert_eq!(zero_snapshots[1].opacity_bits, 1.0_f32.to_bits());
    }

    #[test]
    fn compiler_rejects_every_invalid_effect_store_before_any_emit() {
        let (valid, parent, child) = compiler_effect_test_artifact(0.5, 0.25);

        let mut missing = valid.clone();
        missing.effect_nodes.clear();
        assert_compiler_rejects_before_emit(&missing, "missing effect leaf");

        let mut duplicate = valid.clone();
        duplicate.effect_nodes.push(duplicate.effect_nodes[0]);
        assert_compiler_rejects_before_emit(&duplicate, "duplicate effect id");

        let mut wrong_owner = valid.clone();
        wrong_owner.effect_nodes[0].owner = NodeKey::null();
        assert_compiler_rejects_before_emit(&wrong_owner, "wrong effect owner");

        let mut generation_zero = valid.clone();
        generation_zero.effect_nodes[0].generation = 0;
        assert_compiler_rejects_before_emit(&generation_zero, "zero effect generation");

        let mut non_finite = valid.clone();
        non_finite.effect_nodes[0].opacity = f32::NAN;
        assert_compiler_rejects_before_emit(&non_finite, "non-finite effect opacity");

        let mut out_of_range = valid.clone();
        out_of_range.effect_nodes[0].opacity = 1.25;
        assert_compiler_rejects_before_emit(&out_of_range, "out-of-range effect opacity");

        let mut dangling = valid.clone();
        let child_index = dangling
            .effect_nodes
            .iter()
            .position(|snapshot| snapshot.owner == child)
            .unwrap();
        dangling.effect_nodes[child_index].parent = Some(EffectNodeId(NodeKey::null()));
        assert_compiler_rejects_before_emit(&dangling, "dangling effect parent");

        let mut cycle = valid.clone();
        let parent_index = cycle
            .effect_nodes
            .iter()
            .position(|snapshot| snapshot.owner == parent)
            .unwrap();
        cycle.effect_nodes[parent_index].parent = Some(EffectNodeId(child));
        assert_compiler_rejects_before_emit(&cycle, "effect cycle");

        let mut wrong_ref = valid.clone();
        wrong_ref.chunks[0].properties.effect = Some(EffectNodeId(NodeKey::null()));
        assert_compiler_rejects_before_emit(&wrong_ref, "missing chunk effect ref");

        let mut unreferenced = valid.clone();
        let mut key_arena = NodeArena::new();
        let extra_owner = key_arena.insert(Node::new(Box::new(Element::new_with_id(
            0x6bff, 0.0, 0.0, 1.0, 1.0,
        ))));
        unreferenced.effect_nodes.push(EffectNodeSnapshot {
            id: EffectNodeId(extra_owner),
            owner: extra_owner,
            parent: None,
            opacity: 0.75,
            generation: 1,
        });
        assert_compiler_rejects_before_emit(&unreferenced, "unreferenced effect node");

        let mut baked_mismatch = valid;
        let PaintOp::DrawRect(first) = &mut baked_mismatch.ops[0] else {
            panic!("Element fixture must start with DrawRect")
        };
        first.params.opacity = 1.0;
        assert_compiler_rejects_before_emit(&baked_mismatch, "baked local opacity mismatch");
    }

    #[test]
    fn compiler_requires_effect_owners_to_follow_canonical_owner_ancestry() {
        let (sibling_valid, parent, first, second) = compiler_sibling_effect_artifact();
        let first_effect = sibling_valid
            .effect_nodes
            .iter()
            .find(|snapshot| snapshot.owner == first)
            .unwrap()
            .id;
        let second_effect = sibling_valid
            .effect_nodes
            .iter()
            .find(|snapshot| snapshot.owner == second)
            .unwrap()
            .id;

        let mut sibling_rebind = sibling_valid.clone();
        sibling_rebind
            .chunks
            .iter_mut()
            .find(|chunk| chunk.owner == first)
            .unwrap()
            .properties
            .effect = Some(second_effect);
        sibling_rebind
            .chunks
            .iter_mut()
            .find(|chunk| chunk.owner == second)
            .unwrap()
            .properties
            .effect = Some(first_effect);
        assert_compiler_rejects_before_emit(&sibling_rebind, "sibling effect rebind");

        let (mut descendant_rebind, _, descendant) = compiler_effect_test_artifact(0.5, 0.25);
        descendant_rebind.chunks[0].properties.effect = Some(EffectNodeId(descendant));
        assert_compiler_rejects_before_emit(&descendant_rebind, "descendant effect rebind");

        let mut unrelated_arena = new_test_arena();
        let first_root = commit_element(
            &mut unrelated_arena,
            Box::new(leaf_element(0x6b40, Color::rgb(10, 20, 30), 0.5, false)),
        );
        let second_root = commit_element(
            &mut unrelated_arena,
            Box::new(leaf_element(0x6b41, Color::rgb(40, 50, 60), 0.25, false)),
        );
        let (measure, place) = constraints();
        measure_and_place(&mut unrelated_arena, first_root, measure, place);
        measure_and_place(&mut unrelated_arena, second_root, measure, place);
        let unrelated_roots = [first_root, second_root];
        let (unrelated_properties, unrelated_generations) =
            sync_identity(&unrelated_arena, &unrelated_roots);
        let mut unrelated = whole_frame_artifact(
            &unrelated_arena,
            &unrelated_roots,
            &unrelated_properties,
            &unrelated_generations,
        )
        .0;
        unrelated.chunks[0].properties.effect = Some(EffectNodeId(second_root));
        unrelated.chunks[1].properties.effect = Some(EffectNodeId(first_root));
        assert_compiler_rejects_before_emit(&unrelated, "unrelated-root effect rebind");

        let mut wrong_parent = sibling_valid;
        wrong_parent
            .owner_nodes
            .iter_mut()
            .find(|snapshot| snapshot.owner == first)
            .unwrap()
            .parent = Some(second);
        // The store is complete and acyclic, but this claimed parent conflicts
        // with the effect chain captured for the same canonical frame walk.
        assert_compiler_rejects_before_emit(&wrong_parent, "wrong traversal parent");
        let _ = parent;
    }

    #[test]
    fn compiler_rejects_invalid_owner_store_and_late_failure_before_emit() {
        let (valid, _, first, second) = compiler_sibling_effect_artifact();

        let mut missing = valid.clone();
        missing
            .owner_nodes
            .retain(|snapshot| snapshot.owner != first);
        assert_compiler_rejects_before_emit(&missing, "missing chunk owner");

        let mut duplicate = valid.clone();
        duplicate.owner_nodes.push(duplicate.owner_nodes[0]);
        assert_compiler_rejects_before_emit(&duplicate, "duplicate chunk owner");

        let mut null_owner = valid.clone();
        let original_owner = null_owner.chunks[0].owner;
        null_owner.chunks[0].id.owner = NodeKey::null();
        null_owner.chunks[0].owner = NodeKey::null();
        null_owner
            .owner_nodes
            .iter_mut()
            .find(|snapshot| snapshot.owner == original_owner)
            .expect("first chunk owner snapshot")
            .owner = NodeKey::null();
        assert_compiler_rejects_before_emit(&null_owner, "null chunk owner");

        let mut cycle = valid.clone();
        cycle
            .owner_nodes
            .iter_mut()
            .find(|snapshot| snapshot.owner == first)
            .unwrap()
            .parent = Some(first);
        assert_compiler_rejects_before_emit(&cycle, "owner cycle");

        let mut dangling = valid.clone();
        dangling
            .owner_nodes
            .iter_mut()
            .find(|snapshot| snapshot.owner == first)
            .unwrap()
            .parent = Some(NodeKey::null());
        assert_compiler_rejects_before_emit(&dangling, "missing owner parent");

        let mut unreferenced = valid.clone();
        let mut key_arena = NodeArena::new();
        let extra = key_arena.insert(Node::new(Box::new(Element::new_with_id(
            0x6b42, 0.0, 0.0, 1.0, 1.0,
        ))));
        unreferenced.owner_nodes.push(PaintOwnerSnapshot {
            owner: extra,
            parent: None,
        });
        assert_compiler_rejects_before_emit(&unreferenced, "unreferenced owner node");

        let mut late_invalid = valid;
        late_invalid
            .owner_nodes
            .iter_mut()
            .find(|snapshot| snapshot.owner == second)
            .unwrap()
            .parent = Some(second);
        assert_compiler_rejects_before_emit(&late_invalid, "late invalid owner cycle");
    }

    #[test]
    fn compiler_requires_complete_nearest_active_effect_chain() {
        let (valid, grandparent, parent, child) = compiler_three_level_effect_artifact();
        let mut skip_nearest = valid.clone();
        skip_nearest
            .chunks
            .iter_mut()
            .find(|chunk| chunk.owner == child)
            .unwrap()
            .properties
            .effect = Some(EffectNodeId(grandparent));
        assert_compiler_rejects_before_emit(&skip_nearest, "skipped nearest parent effect");

        let mut missing_inherited = valid;
        missing_inherited
            .chunks
            .iter_mut()
            .find(|chunk| chunk.owner == child)
            .unwrap()
            .properties
            .effect = None;
        assert_compiler_rejects_before_emit(&missing_inherited, "missing inherited effect");

        let _ = parent;
    }

    #[test]
    fn compiler_checks_baked_opacity_for_text_image_svg_and_decorations() {
        let (text_arena, text_roots, _) = prepared_text_tree(false);
        let (text_properties, text_generations) = sync_identity(&text_arena, &text_roots);
        let mut text = whole_frame_artifact(
            &text_arena,
            &text_roots,
            &text_properties,
            &text_generations,
        )
        .0;
        let PaintOp::PreparedText(text_op) = &mut text.ops[0] else {
            panic!("text fixture must record PreparedText")
        };
        text_op.params.staging_input.glyphs[0].paint.opacity = 1.0;
        assert_compiler_rejects_before_emit(&text, "prepared text glyph opacity mismatch");

        let pixels: Arc<[u8]> = Arc::from([255_u8; 16]);
        let (image_arena, image_roots) = bare_image_fixture(
            pixels.clone(),
            crate::view::ImageFit::Fill,
            crate::view::ImageSampling::Linear,
            0.5,
        );
        let (image_properties, image_generations) = sync_identity(&image_arena, &image_roots);
        let mut image = whole_frame_artifact(
            &image_arena,
            &image_roots,
            &image_properties,
            &image_generations,
        )
        .0;
        let PaintOp::PreparedImage(image_op) = image.ops.last_mut().unwrap() else {
            panic!("image fixture must record PreparedImage")
        };
        image_op.params.opacity = 1.0;
        image.chunks[0].payload_identity = PaintPayloadIdentity::image_with_decoration(
            PreparedImageIdentity::from_op(image_op),
            std::iter::empty(),
        )
        .unwrap();
        assert_compiler_rejects_before_emit(&image, "prepared image opacity mismatch");

        let (svg_arena, svg_roots) = bare_image_fixture(
            pixels,
            crate::view::ImageFit::Fill,
            crate::view::ImageSampling::Linear,
            0.5,
        );
        let (svg_properties, svg_generations) = sync_identity(&svg_arena, &svg_roots);
        let mut svg =
            whole_frame_artifact(&svg_arena, &svg_roots, &svg_properties, &svg_generations).0;
        svg.chunks[0].id.role = PaintChunkRole::SvgContent;
        let PaintOp::PreparedImage(image_op) = svg.ops.last_mut().unwrap() else {
            panic!("source fixture must record PreparedImage")
        };
        image_op.upload.id = crate::view::sampled_texture::SampledTextureId::SvgRaster(
            crate::view::sampled_texture::SvgRasterAssetId::for_test(0x6b11),
        );
        let mut svg_op = PreparedSvgOp {
            params: image_op.params,
            upload: image_op.upload.clone(),
        };
        svg_op.params.opacity = 1.0;
        svg.chunks[0].payload_identity = PaintPayloadIdentity::svg_with_decoration(
            PreparedSvgIdentity::from_op(&svg_op).expect("typed SVG identity"),
            std::iter::empty(),
        )
        .unwrap();
        *svg.ops.last_mut().unwrap() = PaintOp::PreparedSvg(svg_op);
        assert_compiler_rejects_before_emit(&svg, "prepared SVG opacity mismatch");

        let (decorated_arena, decorated_roots) = prepared_image_fixture(
            Arc::from([255_u8; 16]),
            crate::view::ImageFit::Fill,
            crate::view::ImageSampling::Linear,
            0.5,
        );
        let (decorated_properties, decorated_generations) =
            sync_identity(&decorated_arena, &decorated_roots);
        let mut decorated = whole_frame_artifact(
            &decorated_arena,
            &decorated_roots,
            &decorated_properties,
            &decorated_generations,
        )
        .0;
        let PaintOp::DrawRect(decoration) = &mut decorated.ops[0] else {
            panic!("decorated image must begin with DrawRect")
        };
        decoration.params.opacity = 1.0;
        assert_compiler_rejects_before_emit(&decorated, "image decoration opacity mismatch");
    }

    #[test]
    fn neutral_element_text_and_image_direct_artifacts_compile_after_arena_drop() {
        fn record_direct(arena: &NodeArena, owner: NodeKey) -> PaintArtifact {
            arena
                .get(owner)
                .expect("direct host exists")
                .element
                .record_shadow_paint_artifact(
                    owner,
                    PropertyTreeState::default(),
                    PaintContentRevision {
                        self_paint_revision: 1,
                        composite_revision: 1,
                        topology_revision: 1,
                    },
                    arena,
                    PaintRecordingContext::default(),
                )
                .expect("neutral direct host must record")
        }

        fn assert_owning_root_and_compile(artifact: PaintArtifact, owner: NodeKey) {
            assert_eq!(
                artifact.owner_nodes,
                vec![PaintOwnerSnapshot {
                    owner,
                    parent: None,
                }]
            );
            let mut graph = FrameGraph::new();
            let mut ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
            let target = ctx.allocate_target(&mut graph);
            ctx.set_current_target(target);
            assert!(try_compile_artifact(&artifact, &mut graph, ctx).is_ok());
        }

        let mut element_arena = new_test_arena();
        let element = commit_element(
            &mut element_arena,
            Box::new(leaf_element(0x6b50, Color::rgb(10, 20, 30), 1.0, false)),
        );
        let (measure, place) = constraints();
        measure_and_place(&mut element_arena, element, measure, place);
        let element_artifact = record_direct(&element_arena, element);
        drop(element_arena);
        assert_owning_root_and_compile(element_artifact, element);

        let mut text_arena = new_test_arena();
        let mut text = Text::new_with_id(0x6b51, 0.0, 0.0, 120.0, 40.0, "owning text");
        text.set_opacity(1.0);
        let text = commit_element(&mut text_arena, Box::new(text));
        measure_and_place(&mut text_arena, text, measure, place);
        let text_artifact = record_direct(&text_arena, text);
        drop(text_arena);
        assert_owning_root_and_compile(text_artifact, text);

        let (image_arena, image_roots) = bare_image_fixture(
            Arc::from([255_u8; 16]),
            crate::view::ImageFit::Fill,
            crate::view::ImageSampling::Linear,
            1.0,
        );
        let image = image_roots[0];
        let image_artifact = record_direct(&image_arena, image);
        drop(image_arena);
        assert_owning_root_and_compile(image_artifact, image);
    }

    #[test]
    fn empty_clip_emits_nothing_and_does_not_consume_opaque_order() {
        let mut artifact = compiler_clip_test_artifact();
        artifact.clip_nodes[0].logical_scissor[2] = 0;

        let graph = compiled_whole_frame_graph(&artifact);
        let snapshots = graph.test_rect_pass_snapshots();
        assert_eq!(snapshots.len(), 1, "only the unclipped sibling should emit");
        assert!(snapshots[0].opaque);
        assert_eq!(snapshots[0].opaque_depth_order, Some(0));
    }

    fn distinct_chunk(mut chunk: PaintChunk) -> PaintChunk {
        chunk.id.owner = NodeKey::null();
        chunk.owner = NodeKey::null();
        chunk
    }

    #[test]
    fn compiler_rejects_overlapping_ranges_before_emit() {
        let mut artifact = compiler_test_artifact();
        artifact.ops.push(artifact.ops[0].clone());
        let mut overlapping = distinct_chunk(artifact.chunks[0].clone());
        overlapping.op_range = 0..2;
        artifact.chunks.push(overlapping);
        assert_compiler_rejects_before_emit(&artifact, "overlapping ranges");
    }

    #[test]
    fn compiler_rejects_out_of_order_ranges_before_emit() {
        let mut artifact = compiler_test_artifact();
        artifact.ops.push(artifact.ops[0].clone());
        let mut second = distinct_chunk(artifact.chunks[0].clone());
        artifact.chunks[0].op_range = 1..2;
        second.op_range = 0..1;
        artifact.chunks.push(second);
        assert_compiler_rejects_before_emit(&artifact, "out-of-order ranges");
    }

    #[test]
    fn compiler_rejects_internal_gap_before_emit() {
        let mut artifact = compiler_test_artifact();
        artifact.ops.push(artifact.ops[0].clone());
        artifact.ops.push(artifact.ops[0].clone());
        let mut after_gap = distinct_chunk(artifact.chunks[0].clone());
        after_gap.op_range = 2..3;
        artifact.chunks.push(after_gap);
        assert_compiler_rejects_before_emit(&artifact, "internal op gap");
    }

    #[test]
    fn compiler_rejects_trailing_unowned_ops_before_emit() {
        let mut artifact = compiler_test_artifact();
        artifact.ops.push(artifact.ops[0].clone());
        assert_compiler_rejects_before_emit(&artifact, "trailing unowned ops");
    }

    #[test]
    fn compiler_rejects_duplicate_chunk_id_before_emit() {
        let mut artifact = compiler_test_artifact();
        artifact.ops.push(artifact.ops[0].clone());
        let mut duplicate = artifact.chunks[0].clone();
        duplicate.op_range = 1..2;
        artifact.chunks.push(duplicate);
        assert_compiler_rejects_before_emit(&artifact, "duplicate PaintChunkId");
    }

    #[test]
    fn artifact_and_legacy_roots_keep_document_order() {
        let builds = Arc::new(AtomicUsize::new(0));
        let mut arena = new_test_arena();
        let artifact_root = commit_element(
            &mut arena,
            Box::new(leaf_element(50, Color::rgb(255, 0, 0), 1.0, false)),
        );
        let legacy_root = commit_element(
            &mut arena,
            Box::new(RecordingHost {
                id: 51,
                builds: builds.clone(),
                fill: Some([0.0, 0.0, 1.0, 1.0]),
            }),
        );
        let (measure, place) = constraints();
        measure_and_place(&mut arena, artifact_root, measure, place);
        let roots = [artifact_root, legacy_root];
        let (properties, generations) = sync_identity(&arena, &roots);

        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
        let target = ctx.allocate_target(&mut graph);
        ctx.set_current_target(target);
        for root in roots {
            let child_ctx = UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone());
            let next = match record_root(&arena, root, &properties, &generations) {
                PaintRecordOutcome::Artifact(artifact) => {
                    compile_artifact(&artifact, &mut graph, child_ctx)
                }
                PaintRecordOutcome::LegacySubtree(_) => arena
                    .with_element_taken(root, |element, arena| {
                        element.build(&mut graph, arena, child_ctx)
                    })
                    .expect("legacy root builds"),
            };
            ctx.set_state(next);
        }

        let snapshots = graph.test_rect_pass_snapshots();
        assert_eq!(
            snapshots.len(),
            2,
            "passes={:?} builds={}",
            graph
                .pass_descriptors()
                .into_iter()
                .map(|pass| pass.name)
                .collect::<Vec<_>>(),
            builds.load(Ordering::Relaxed)
        );
        assert!(f32::from_bits(snapshots[0].fill_color_bits[0]) > 0.9);
        assert!(f32::from_bits(snapshots[1].fill_color_bits[2]) > 0.9);
        assert_eq!(builds.load(Ordering::Relaxed), 1);
    }

    fn whole_frame_artifact(
        arena: &NodeArena,
        roots: &[NodeKey],
        properties: &PropertyTrees,
        generations: &PaintGenerationTracker,
    ) -> (PaintArtifact, FrameArtifactEligibility) {
        let FrameArtifactRecordOutcome::Artifact {
            artifact,
            eligibility,
        } = record_frame_artifact(
            arena,
            roots,
            properties,
            generations,
            RendererMode::ForcedForTests,
        )
        .expect("plain Element frame should be fully recordable")
        else {
            panic!("forced artifact recording cannot silently fall back")
        };
        (artifact, eligibility)
    }

    fn root_group_artifact(
        arena: &NodeArena,
        roots: &[NodeKey],
        properties: &PropertyTrees,
        generations: &PaintGenerationTracker,
    ) -> (PaintArtifact, FrameArtifactEligibility) {
        let FrameArtifactRecordOutcome::Artifact {
            artifact,
            eligibility,
        } = record_root_group_opacity_frame_artifact(
            arena,
            roots,
            properties,
            generations,
            RendererMode::ForcedForTests,
        )
        .expect("single root effect should be fully recordable")
        else {
            panic!("forced root group recording cannot silently fall back")
        };
        (artifact, eligibility)
    }

    fn assert_neutral_opacity(op: &PaintOp) {
        match op {
            PaintOp::DrawRect(op) => assert_eq!(op.params.opacity.to_bits(), 1.0_f32.to_bits()),
            PaintOp::PreparedInlineIfcDecoration(op) => {
                assert_eq!(op.fill.opacity.to_bits(), 1.0_f32.to_bits());
                if let Some(border) = &op.border {
                    assert_eq!(border.opacity.to_bits(), 1.0_f32.to_bits());
                }
            }
            PaintOp::PreparedShadow(op) => {
                assert_eq!(op.params.opacity.to_bits(), 1.0_f32.to_bits())
            }
            PaintOp::PreparedScrollbarOverlay(op) => {
                assert!(op.has_baked_opacity(1.0_f32.to_bits()))
            }
            PaintOp::PreparedText(op) => assert!(
                op.params
                    .staging_input
                    .glyphs
                    .iter()
                    .all(|glyph| glyph.paint.opacity.to_bits() == 1.0_f32.to_bits())
            ),
            PaintOp::PreparedImage(op) => {
                assert_eq!(op.params.opacity.to_bits(), 1.0_f32.to_bits())
            }
            PaintOp::PreparedSvg(op) => {
                assert_eq!(op.params.opacity.to_bits(), 1.0_f32.to_bits())
            }
        }
    }

    fn two_outer_shadows() -> Vec<BoxShadow> {
        vec![
            BoxShadow::new()
                .color(Color::rgb(220, 30, 20))
                .offset_x(1.5)
                .offset_y(-2.25),
            BoxShadow::new()
                .color(Color::rgb(20, 40, 220))
                .offset_x(-3.0)
                .offset_y(4.5),
        ]
    }

    #[test]
    fn outer_shadow_artifact_owns_ordered_fractional_payload_and_strict_pass_sequence() {
        let (arena, root, properties, generations) =
            prepared_shadow_leaf(0x6d10, 1.0, two_outer_shadows(), true);
        let (artifact, eligibility) =
            whole_frame_artifact(&arena, &[root], &properties, &generations);
        assert!(eligibility.eligible);
        let shadows = artifact
            .ops
            .iter()
            .filter_map(|op| match op {
                PaintOp::PreparedShadow(shadow) => Some(shadow),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(shadows.len(), 2);
        assert_eq!(
            shadows[0].params.color,
            Color::rgb(220, 30, 20).to_rgba_f32()
        );
        assert_eq!(
            shadows[1].params.color,
            Color::rgb(20, 40, 220).to_rgba_f32()
        );
        assert_eq!(shadows[0].mesh.vertices[0], [10.0, 21.0]);
        assert!(shadows.iter().all(|shadow| shadow.has_canonical_identity()));

        drop(arena);
        let mut graph = compiled_whole_frame_graph(&artifact);
        let snapshot = graph.test_compile_snapshot().unwrap();
        let payloads = snapshot.pass_payloads();
        assert!(
            matches!(payloads, [
            FramePassTestPayload::Clear(_),
            FramePassTestPayload::ShadowFill(_),
            FramePassTestPayload::Clear(_),
            FramePassTestPayload::ShadowFill(_),
            FramePassTestPayload::Clear(_),
            FramePassTestPayload::TextureComposite(_),
            FramePassTestPayload::TextureComposite(_),
            FramePassTestPayload::DrawRect(fill),
            FramePassTestPayload::DrawRect(border),
        ] if fill.mode == crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly
            && border.mode == crate::view::render_pass::draw_rect_pass::RectRenderMode::BorderOnly),
            "payloads={payloads:?}"
        );
        let shadow_fills = payloads
            .iter()
            .filter_map(|payload| match payload {
                FramePassTestPayload::ShadowFill(fill) => Some(fill),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(shadow_fills.len(), 2);
        assert_eq!(
            shadow_fills[0].color_bits,
            Color::rgb(220, 30, 20).to_rgba_f32().map(f32::to_bits)
        );
        assert_eq!(
            shadow_fills[1].color_bits,
            Color::rgb(20, 40, 220).to_rgba_f32().map(f32::to_bits)
        );
        let first_rect = payloads
            .iter()
            .position(|payload| matches!(payload, FramePassTestPayload::DrawRect(_)))
            .unwrap();
        assert!(
            payloads[..first_rect]
                .iter()
                .any(|payload| { matches!(payload, FramePassTestPayload::TextureComposite(_)) })
        );
        let rect_modes = payloads
            .iter()
            .filter_map(|payload| match payload {
                FramePassTestPayload::DrawRect(rect) => Some(rect.mode),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            rect_modes,
            vec![
                crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly,
                crate::view::render_pass::draw_rect_pass::RectRenderMode::BorderOnly,
            ]
        );
    }

    #[test]
    fn outer_shadow_owner_with_two_children_records_before_children_and_matches_legacy() {
        let (arena, root, first, second, properties, generations) =
            prepared_shadow_owner_tree(0x6d30, 1.0);
        let metadata = record_coverage_manifest(
            &arena,
            &[root],
            false,
            true,
            CoverageRecordingMode::MetadataOnly,
            &properties,
            &generations,
        );
        let full = record_coverage_manifest(
            &arena,
            &[root],
            false,
            true,
            CoverageRecordingMode::FullArtifact,
            &properties,
            &generations,
        );
        assert!(metadata.validation_errors.is_empty());
        assert!(full.validation_errors.is_empty());
        assert!(frame_recorder::canonical_manifest_matches(&metadata, &full));
        assert_eq!(
            metadata
                .items
                .iter()
                .map(|item| match item {
                    PaintCoverageItem::ArtifactChunk { chunk, .. } => {
                        (chunk.owner, chunk.id.role, chunk.id.phase, chunk.id.slot)
                    }
                    other => panic!("unexpected owner-tree coverage: {other:?}"),
                })
                .collect::<Vec<_>>(),
            vec![
                (
                    root,
                    PaintChunkRole::SelfDecoration,
                    PaintNodePhase::BeforeChildren,
                    0,
                ),
                (
                    first,
                    PaintChunkRole::SelfDecoration,
                    PaintNodePhase::BeforeChildren,
                    0,
                ),
                (
                    second,
                    PaintChunkRole::SelfDecoration,
                    PaintNodePhase::BeforeChildren,
                    0,
                ),
            ]
        );
        let PaintCoverageItem::ArtifactChunk {
            chunk: owner_chunk, ..
        } = &metadata.items[0]
        else {
            unreachable!()
        };
        assert!(matches!(
            &owner_chunk.payload_identity,
            PaintPayloadIdentity::PreparedShadows(shadows, decoration)
                if shadows.len() == 2 && decoration.len() == 2
        ));

        let (artifact, eligibility) =
            whole_frame_artifact(&arena, &[root], &properties, &generations);
        assert!(eligibility.eligible);
        assert_eq!(
            artifact
                .chunks
                .iter()
                .map(|chunk| chunk.owner)
                .collect::<Vec<_>>(),
            vec![root, first, second]
        );
        let first_non_shadow = artifact
            .ops
            .iter()
            .position(|op| !matches!(op, PaintOp::PreparedShadow(_)))
            .unwrap();
        assert_eq!(first_non_shadow, 2);

        let (legacy_arena, legacy_root, _, _, _, _) = prepared_shadow_owner_tree(0x6d30, 1.0);
        assert_eq!(
            compiled_whole_frame_graph(&artifact).pass_descriptors(),
            legacy_roots_graph(legacy_arena, &[legacy_root]).pass_descriptors()
        );
    }

    #[test]
    fn outer_shadow_artifact_respects_baked_and_root_group_opacity_authority() {
        let (arena, root, properties, generations) =
            prepared_shadow_leaf(0x6d11, 0.4, two_outer_shadows(), false);
        let (baked, _) = whole_frame_artifact(&arena, &[root], &properties, &generations);
        assert!(
            baked
                .ops
                .iter()
                .filter_map(|op| match op {
                    PaintOp::PreparedShadow(shadow) => Some(shadow.params.opacity),
                    _ => None,
                })
                .all(|opacity| opacity.to_bits() == 0.4_f32.to_bits())
        );

        let (group, _) = root_group_artifact(&arena, &[root], &properties, &generations);
        group.ops.iter().for_each(assert_neutral_opacity);
        let mut graph = compiled_whole_frame_graph(&group);
        let snapshot = graph.test_compile_snapshot().unwrap();
        assert!(snapshot.pass_payloads().iter().any(|payload| {
            matches!(payload, FramePassTestPayload::CompositeLayer(composite)
                if composite.opacity_bits == 0.4_f32.to_bits())
        }));
        assert!(
            snapshot
                .pass_payloads()
                .iter()
                .filter_map(|payload| match payload {
                    FramePassTestPayload::ShadowFill(fill) => Some(fill.color_bits[3]),
                    _ => None,
                })
                .all(|alpha| alpha == 1.0_f32.to_bits())
        );
    }

    #[test]
    fn outer_shadow_owner_opacity_is_applied_once_and_metadata_detects_shadow_drift() {
        let (arena, root, _, _, properties, generations) = prepared_shadow_owner_tree(0x6d33, 0.4);
        let (baked, _) = whole_frame_artifact(&arena, &[root], &properties, &generations);
        assert!(
            baked
                .ops
                .iter()
                .filter_map(|op| match op {
                    PaintOp::PreparedShadow(shadow) => Some(shadow.params.opacity),
                    _ => None,
                })
                .all(|opacity| opacity.to_bits() == 0.4_f32.to_bits())
        );

        let (group, _) = root_group_artifact(&arena, &[root], &properties, &generations);
        group.ops.iter().for_each(assert_neutral_opacity);
        let snapshot = compiled_whole_frame_graph(&group)
            .test_compile_snapshot()
            .unwrap();
        assert_eq!(
            snapshot
                .pass_payloads()
                .iter()
                .filter(|payload| matches!(
                    payload,
                    FramePassTestPayload::CompositeLayer(composite)
                        if composite.opacity_bits == 0.4_f32.to_bits()
                ))
                .count(),
            1
        );

        let metadata = record_coverage_manifest(
            &arena,
            &[root],
            false,
            true,
            CoverageRecordingMode::MetadataOnly,
            &properties,
            &generations,
        );
        arena
            .get_mut(root)
            .unwrap()
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .unwrap()
            .set_box_shadows(vec![
                BoxShadow::new()
                    .color(Color::rgb(20, 40, 220))
                    .offset_x(-9.0)
                    .offset_y(4.5),
                BoxShadow::new()
                    .color(Color::rgb(220, 30, 20))
                    .offset_x(1.5)
                    .offset_y(-2.25),
            ]);
        let full = record_coverage_manifest(
            &arena,
            &[root],
            false,
            true,
            CoverageRecordingMode::FullArtifact,
            &properties,
            &generations,
        );
        assert!(!frame_recorder::canonical_manifest_matches(
            &metadata, &full
        ));
    }

    #[test]
    fn outer_shadow_owner_child_boundaries_fallback_before_full_recording() {
        #[derive(Clone, Copy)]
        enum ChildBoundary {
            Unknown,
            Deferred,
        }

        for (index, boundary) in [ChildBoundary::Unknown, ChildBoundary::Deferred]
            .into_iter()
            .enumerate()
        {
            let id = 0x6d40 + index as u64 * 0x10;
            let (mut arena, root, _, _) = prepared_shadow_leaf(id, 1.0, two_outer_shadows(), false);
            let _child = match boundary {
                ChildBoundary::Unknown => commit_child(
                    &mut arena,
                    root,
                    Box::new(TextAreaTextRun::new("unknown".to_string(), 0..7)),
                ),
                ChildBoundary::Deferred => {
                    let mut child = leaf_element(id + 1, Color::rgb(20, 180, 40), 1.0, false);
                    let mut style = Style::new();
                    style.insert(
                        PropertyId::Position,
                        ParsedValue::Position(Position::absolute().clip(ClipMode::Viewport)),
                    );
                    child.apply_style(style);
                    commit_child(&mut arena, root, Box::new(child))
                }
            };
            let (measure, place) = constraints();
            measure_and_place(&mut arena, root, measure, place);
            let (properties, generations) = sync_identity(&arena, &[root]);
            take_full_artifact_record_count();
            let outcome = record_clip_enabled_frame_artifact(
                &arena,
                &[root],
                &properties,
                &generations,
                RendererMode::Auto,
            )
            .unwrap();
            let FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility) = outcome else {
                panic!("child boundary must force whole-frame fallback")
            };
            match boundary {
                ChildBoundary::Unknown => assert!(eligibility.reasons.contains(
                    &FrameArtifactFallbackReason::LegacyBoundary(LegacyPaintReason::UnknownHost)
                )),
                ChildBoundary::Deferred => assert!(eligibility.reasons.iter().any(|reason| {
                    matches!(
                        reason,
                        FrameArtifactFallbackReason::LegacyBoundary(LegacyPaintReason::Deferred)
                            | FrameArtifactFallbackReason::DeferredBoundary(_)
                    )
                })),
            }
            assert_eq!(take_full_artifact_record_count(), 0);
        }
    }

    #[test]
    fn outer_shadow_artifact_preflight_and_compiler_fail_closed() {
        for (case, shadow) in [
            ("tiny-positive-blur", BoxShadow::new().blur(0.000_5)),
            ("inset", BoxShadow::new().inset(true)),
        ] {
            let (arena, root, properties, generations) =
                prepared_shadow_leaf(0x6d20, 1.0, vec![shadow], false);
            let _ = take_full_artifact_record_count();
            let error = record_property_neutral_frame_artifact(
                &arena,
                &[root],
                &properties,
                &generations,
                RendererMode::ForcedForTests,
            )
            .unwrap_err();
            assert!(
                error
                    .reasons
                    .contains(&FrameArtifactFallbackReason::LegacyBoundary(
                        LegacyPaintReason::BoxShadow
                    )),
                "{case}: {error:?}"
            );
            assert_eq!(take_full_artifact_record_count(), 0, "{case}");
        }

        let (arena, root, properties, generations) =
            prepared_shadow_leaf(0x6d21, 1.0, two_outer_shadows(), false);
        let (mut artifact, _) = whole_frame_artifact(&arena, &[root], &properties, &generations);
        let PaintOp::PreparedShadow(late) = artifact
            .ops
            .iter_mut()
            .rev()
            .find(|op| matches!(op, PaintOp::PreparedShadow(_)))
            .unwrap()
        else {
            unreachable!()
        };
        late.mesh.indices[0] = u32::MAX;
        assert_eq!(
            compiled_whole_frame_graph(&artifact)
                .pass_descriptors()
                .len(),
            1,
            "late invalid shadow must emit zero artifact passes"
        );

        let (arena, root, properties, generations) =
            prepared_shadow_leaf(0x6d22, 1.0, two_outer_shadows(), false);
        let (mut reordered, _) = whole_frame_artifact(&arena, &[root], &properties, &generations);
        let fill_index = reordered
            .ops
            .iter()
            .position(|op| matches!(op, PaintOp::DrawRect(_)))
            .unwrap();
        reordered.ops.swap(0, fill_index);
        assert_eq!(
            compiled_whole_frame_graph(&reordered)
                .pass_descriptors()
                .len(),
            1,
            "shadow after FillOnly must reject before the first artifact pass"
        );

        let mut clipped = Element::new_with_id(0x6d23, 10.25, 20.75, 80.0, 40.0);
        let mut style = Style::new();
        style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::rgb(40, 80, 160)),
        );
        style.set_box_shadow(two_outer_shadows());
        style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(10.0))
                    .top(Length::px(20.0))
                    .clip(ClipMode::AnchorParent),
            ),
        );
        clipped.apply_style(style);
        let mut arena = new_test_arena();
        let root = commit_element(&mut arena, Box::new(clipped));
        let (measure, place) = constraints();
        measure_and_place(&mut arena, root, measure, place);
        let (properties, generations) = sync_identity(&arena, &[root]);
        let _ = take_full_artifact_record_count();
        let outcome = record_frame_artifact(
            &arena,
            &[root],
            &properties,
            &generations,
            RendererMode::ForcedForTests,
        )
        .expect("exact single-owner self clip + canonical outer shadow records");
        let FrameArtifactRecordOutcome::Artifact {
            artifact,
            eligibility,
        } = outcome
        else {
            panic!("exact clipped outer shadow must not fall back")
        };
        assert!(eligibility.eligible);
        assert_eq!(artifact.chunks.len(), 1);
        assert!(matches!(
            artifact.ops.first(),
            Some(PaintOp::PreparedShadow(_))
        ));
        assert!(
            compiled_whole_frame_graph(&artifact)
                .pass_descriptors()
                .len()
                > 1
        );
        assert_eq!(take_full_artifact_record_count(), 1);

        let (mut arena, root, _, _) = prepared_shadow_leaf(0x6d24, 1.0, two_outer_shadows(), false);
        let mut rounded = Style::new();
        rounded.set_border_radius(BorderRadius::uniform(Length::px(12.0)));
        arena
            .get_mut(root)
            .unwrap()
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .unwrap()
            .apply_style(rounded);
        commit_child(
            &mut arena,
            root,
            Box::new(leaf_element(0x6d25, Color::rgb(20, 180, 40), 1.0, false)),
        );
        let (measure, place) = constraints();
        measure_and_place(&mut arena, root, measure, place);
        let (properties, generations) = sync_identity(&arena, &[root]);
        let _ = take_full_artifact_record_count();
        let error = record_property_neutral_frame_artifact(
            &arena,
            &[root],
            &properties,
            &generations,
            RendererMode::ForcedForTests,
        )
        .unwrap_err();
        assert!(
            error
                .reasons
                .contains(&FrameArtifactFallbackReason::LegacyBoundary(
                    LegacyPaintReason::ChildClip
                )),
            "{error:?}"
        );
        assert_eq!(take_full_artifact_record_count(), 0);
    }

    #[test]
    fn self_decoration_grammar_accepts_empty_but_rejects_shadow_only_and_border_only() {
        let (arena, root, properties, generations) =
            prepared_shadow_leaf(0x6d26, 1.0, two_outer_shadows(), true);
        let (valid, _) = whole_frame_artifact(&arena, &[root], &properties, &generations);

        let mut empty = valid.clone();
        empty.ops.clear();
        empty.chunks[0].op_range = 0..0;
        empty.chunks[0].payload_identity =
            PaintPayloadIdentity::prepared_shadows(std::iter::empty());
        let _ = take_artifact_compile_count();
        let _ = compiled_whole_frame_graph(&empty);
        assert_eq!(take_artifact_compile_count(), 1);

        let mut empty_with_stale_identity = empty.clone();
        empty_with_stale_identity.chunks[0].payload_identity =
            valid.chunks[0].payload_identity.clone();
        let _ = take_artifact_compile_count();
        let _ = compiled_whole_frame_graph(&empty_with_stale_identity);
        assert_eq!(take_artifact_compile_count(), 0);

        let mut shadow_only = valid.clone();
        shadow_only
            .ops
            .retain(|op| matches!(op, PaintOp::PreparedShadow(_)));
        shadow_only.chunks[0].op_range = 0..shadow_only.ops.len();
        let _ = take_artifact_compile_count();
        let _ = compiled_whole_frame_graph(&shadow_only);
        assert_eq!(take_artifact_compile_count(), 0);

        let mut border_only = valid;
        let border = border_only
            .ops
            .iter()
            .find(|op| {
                matches!(op, PaintOp::DrawRect(rect)
                    if rect.mode == crate::view::render_pass::draw_rect_pass::RectRenderMode::BorderOnly)
            })
            .cloned()
            .unwrap();
        border_only.ops = vec![border];
        border_only.chunks[0].op_range = 0..1;
        let _ = take_artifact_compile_count();
        let _ = compiled_whole_frame_graph(&border_only);
        assert_eq!(take_artifact_compile_count(), 0);
    }

    #[test]
    fn shadow_metadata_identity_detects_order_and_param_drift_before_full_authority() {
        let (arena, root, properties, generations) =
            prepared_shadow_leaf(0x6d27, 1.0, two_outer_shadows(), false);
        let metadata = record_coverage_manifest(
            &arena,
            &[root],
            false,
            true,
            CoverageRecordingMode::MetadataOnly,
            &properties,
            &generations,
        );
        let PaintCoverageItem::ArtifactChunk { chunk, .. } = &metadata.items[0] else {
            panic!("shadow metadata must be an artifact chunk")
        };
        let PaintPayloadIdentity::PreparedShadows(identities, _) = &chunk.payload_identity else {
            panic!("shadow metadata must own ordered structural identities")
        };
        assert_eq!(identities.len(), 2);

        arena
            .get_mut(root)
            .unwrap()
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .unwrap()
            .set_box_shadows(vec![
                BoxShadow::new()
                    .color(Color::rgb(20, 40, 220))
                    .offset_x(-7.0)
                    .offset_y(4.5),
                BoxShadow::new()
                    .color(Color::rgb(220, 30, 20))
                    .offset_x(1.5)
                    .offset_y(-2.25),
            ]);
        let full = record_coverage_manifest(
            &arena,
            &[root],
            false,
            true,
            CoverageRecordingMode::FullArtifact,
            &properties,
            &generations,
        );
        assert!(!frame_recorder::canonical_manifest_matches(
            &metadata, &full
        ));
    }

    #[test]
    fn css_opacity_zero_records_canonical_empty_self_decoration_and_shadow_identity() {
        let mut element = Element::new_with_id(0x6d28, 10.25, 20.75, 80.0, 40.0);
        let mut style = Style::new();
        style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::rgb(40, 80, 160)),
        );
        style.set_box_shadow(two_outer_shadows());
        style.insert(PropertyId::Opacity, ParsedValue::Opacity(Opacity::new(0.0)));
        element.apply_style(style);
        let mut arena = new_test_arena();
        let root = commit_element(&mut arena, Box::new(element));
        let (measure, place) = constraints();
        measure_and_place(&mut arena, root, measure, place);
        let (properties, generations) = sync_identity(&arena, &[root]);
        let (artifact, eligibility) =
            whole_frame_artifact(&arena, &[root], &properties, &generations);
        assert!(eligibility.eligible);
        assert!(artifact.ops.is_empty());
        assert!(matches!(
            &artifact.chunks[0].payload_identity,
            PaintPayloadIdentity::PreparedShadows(identities, decoration)
                if identities.is_empty() && decoration.is_empty()
        ));
        let _ = take_artifact_compile_count();
        let _ = compiled_whole_frame_graph(&artifact);
        assert_eq!(take_artifact_compile_count(), 1);
    }

    #[test]
    fn css_opacity_zero_does_not_bypass_remaining_metadata_capability_blockers() {
        fn element(id: u64, position: Option<Position>, transform: bool) -> Element {
            let mut element = Element::new_with_id(id, 10.25, 20.75, 80.0, 40.0);
            let mut style = Style::new();
            style.insert(
                PropertyId::BackgroundColor,
                ParsedValue::color_like(Color::rgb(40, 80, 160)),
            );
            style.set_box_shadow(two_outer_shadows());
            style.insert(PropertyId::Opacity, ParsedValue::Opacity(Opacity::new(0.0)));
            if let Some(position) = position {
                style.insert(PropertyId::Position, ParsedValue::Position(position));
            }
            if transform {
                style.set_transform(Transform::new([Translate::x(Length::px(3.0))]));
            }
            element.apply_style(style);
            element
        }

        let cases = [(
            "transform",
            element(0x6d29, None, true),
            LegacyPaintReason::Transform,
        )];
        for (case, element, expected) in cases {
            let mut arena = new_test_arena();
            let root = commit_element(&mut arena, Box::new(element));
            let (measure, place) = constraints();
            measure_and_place(&mut arena, root, measure, place);
            let (properties, generations) = sync_identity(&arena, &[root]);
            let _ = take_full_artifact_record_count();
            let error = record_frame_artifact(
                &arena,
                &[root],
                &properties,
                &generations,
                RendererMode::ForcedForTests,
            )
            .unwrap_err();
            assert!(
                error
                    .reasons
                    .contains(&FrameArtifactFallbackReason::LegacyBoundary(expected)),
                "{case}: {error:?}"
            );
            assert_eq!(take_full_artifact_record_count(), 0, "{case}");
        }

        let mut arena = new_test_arena();
        let root = commit_element(&mut arena, Box::new(element(0x6d2c, None, false)));
        commit_child(
            &mut arena,
            root,
            Box::new(leaf_element(0x6d2d, Color::rgb(20, 180, 40), 1.0, false)),
        );
        let (measure, place) = constraints();
        measure_and_place(&mut arena, root, measure, place);
        let (properties, generations) = sync_identity(&arena, &[root]);
        let _ = take_full_artifact_record_count();
        let error = record_property_neutral_frame_artifact(
            &arena,
            &[root],
            &properties,
            &generations,
            RendererMode::ForcedForTests,
        )
        .unwrap_err();
        assert!(
            error
                .reasons
                .contains(&FrameArtifactFallbackReason::LegacyBoundary(
                    LegacyPaintReason::MissingPreparedInlineRoot
                )),
            "children: {error:?}"
        );
        assert_eq!(take_full_artifact_record_count(), 0);
    }

    #[test]
    fn root_group_element_records_neutral_content_and_composites_effect_once() {
        let (arena, root, properties, generations) =
            prepared_leaf(0x6c10, Color::rgb(220, 40, 30), 0.5, true);
        let (artifact, eligibility) =
            root_group_artifact(&arena, &[root], &properties, &generations);
        assert!(eligibility.eligible);
        assert!(matches!(
            artifact.target,
            PaintArtifactTarget::RootOpacityGroup {
                root: target_root,
                effect: EffectNodeId(effect_root),
                ..
            } if target_root == root && effect_root == root
        ));
        artifact.ops.iter().for_each(assert_neutral_opacity);

        let mut graph = compiled_whole_frame_graph(&artifact);
        let root_color = crate::view::base_component::root_effect_stable_key(root);
        let declared = graph
            .declared_persistent_texture_keys()
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(declared.len(), 2);
        assert!(declared.contains(&root_color));
        assert!(declared.contains(&root_color.depth_stencil().unwrap()));
        let snapshot = graph.test_compile_snapshot().unwrap();
        assert!(
            matches!(
                snapshot.pass_payloads(),
                [
                    FramePassTestPayload::Clear(_),
                    FramePassTestPayload::DrawRect(_),
                    FramePassTestPayload::DrawRect(_),
                    FramePassTestPayload::Clear(_),
                    FramePassTestPayload::CompositeLayer(composite),
                ] if composite.opacity_bits == 0.5_f32.to_bits()
            ),
            "payloads={:?}",
            snapshot.pass_payloads()
        );
    }

    #[test]
    fn root_opacity_group_records_contents_clip_neutrally_and_metadata_matches_full() {
        const SCISSOR: [u32; 4] = [7, 11, 23, 19];
        let (arena, root, child, properties, generations) =
            root_opacity_contents_clip_fixture(SCISSOR);
        let expected_clip = ClipNodeId {
            owner: root,
            role: ClipNodeRole::ContentsClip,
        };
        let root_state = properties.node_state_for(root).expect("root state");
        let child_state = properties.node_state_for(child).expect("child state");
        assert_eq!(
            root_state.paint.clip, None,
            "contents clip excludes self paint"
        );
        assert_eq!(root_state.descendants.clip, Some(expected_clip));
        assert_eq!(child_state.paint.clip, Some(expected_clip));

        let recording_context = PaintRecordingContext {
            opacity_authority: PaintOpacityAuthority::NeutralRootEffect(EffectNodeId(root)),
            ..PaintRecordingContext::default()
        };
        let metadata = super::coverage_manifest::record_coverage_manifest_with_context(
            &arena,
            &[root],
            false,
            true,
            CoverageRecordingMode::MetadataOnly,
            &properties,
            &generations,
            recording_context,
            None,
            &Default::default(),
        );
        let mut full = super::coverage_manifest::record_coverage_manifest_with_context(
            &arena,
            &[root],
            false,
            true,
            CoverageRecordingMode::FullArtifact,
            &properties,
            &generations,
            recording_context,
            None,
            &Default::default(),
        );
        assert!(super::frame_recorder::canonical_manifest_matches(
            &metadata, &full
        ));
        let clip_snapshot = full
            .items
            .iter_mut()
            .find_map(|item| match item {
                PaintCoverageItem::ArtifactChunk { clip_snapshot, .. }
                    if !clip_snapshot.is_empty() =>
                {
                    Some(clip_snapshot)
                }
                _ => None,
            })
            .expect("child chunk carries the root contents clip snapshot");
        clip_snapshot[0].logical_scissor[0] += 1;
        assert!(
            !super::frame_recorder::canonical_manifest_matches(&metadata, &full),
            "clip snapshot drift must fail metadata/full parity"
        );

        let (artifact, eligibility) =
            root_group_artifact(&arena, &[root], &properties, &generations);
        assert!(eligibility.eligible, "{eligibility:?}");
        assert_eq!(artifact.chunks.len(), 1);
        assert_eq!(artifact.chunks[0].owner, child);
        assert_eq!(artifact.chunks[0].properties.clip, Some(expected_clip));
        assert!(matches!(
            artifact.clip_nodes.as_slice(),
            [ClipNodeSnapshot {
                id,
                owner,
                logical_scissor: SCISSOR,
                behavior: ClipBehavior::Intersect,
                ..
            }] if *id == expected_clip && *owner == root
        ));
        artifact.ops.iter().for_each(assert_neutral_opacity);

        let baseline_stamp =
            validated_root_effect_raster_stamp(&artifact, root_effect_raster_inputs())
                .expect("clipped root group has a valid raster stamp");
        let mut clip_changed = artifact.clone();
        clip_changed.clip_nodes[0].logical_scissor[2] += 1;
        assert_ne!(
            validated_root_effect_raster_stamp(&clip_changed, root_effect_raster_inputs())
                .expect("changed clip remains a valid artifact"),
            baseline_stamp,
            "clip geometry must invalidate root-effect raster reuse"
        );

        let mut graph = compiled_whole_frame_graph(&artifact);
        let rects = graph.test_rect_pass_snapshots();
        assert_eq!(rects.len(), 1);
        assert_eq!(rects[0].effective_scissor_rect, Some(SCISSOR));
        let snapshot = graph
            .test_compile_snapshot()
            .expect("strict graph snapshot");
        assert!(matches!(
            snapshot.pass_payloads().last(),
            Some(FramePassTestPayload::CompositeLayer(composite))
                if composite.opacity_bits == 0.5_f32.to_bits()
        ));
        assert_eq!(
            graph
                .test_graphics_passes::<crate::view::render_pass::composite_layer_pass::CompositeLayerPass>()
                .len(),
            1,
            "root effect must composite exactly once"
        );
    }

    #[test]
    fn root_opacity_group_explicit_empty_contents_clip_culls_only_contents() {
        let (arena, root, child, properties, generations) =
            root_opacity_contents_clip_fixture([13, 17, 0, 0]);
        let (artifact, eligibility) =
            root_group_artifact(&arena, &[root], &properties, &generations);
        assert!(eligibility.eligible, "{eligibility:?}");
        assert_eq!(artifact.chunks.len(), 1);
        assert_eq!(artifact.chunks[0].owner, child);
        artifact.ops.iter().for_each(assert_neutral_opacity);

        let graph = compiled_whole_frame_graph(&artifact);
        assert!(
            graph.test_rect_pass_snapshots().is_empty(),
            "explicit empty contents clip must suppress the child raster"
        );
        assert_eq!(
            graph
                .test_graphics_passes::<crate::view::render_pass::composite_layer_pass::CompositeLayerPass>()
                .len(),
            1,
            "the root effect group itself still composites once"
        );
    }

    fn root_effect_raster_inputs() -> RootEffectRasterInputs {
        RootEffectRasterInputs {
            width: 320,
            height: 240,
            format: wgpu::TextureFormat::Bgra8Unorm,
            sample_count: 1,
            scale_factor_bits: 1.0_f32.to_bits(),
        }
    }

    #[test]
    fn root_effect_stamp_excludes_root_opacity_and_root_composite_only() {
        let (arena, root, properties, generations) =
            prepared_leaf(0x6c20, Color::rgb(20, 120, 220), 0.5, true);
        let (artifact, _) = root_group_artifact(&arena, &[root], &properties, &generations);
        let baseline = validated_root_effect_raster_stamp(&artifact, root_effect_raster_inputs())
            .expect("valid root effect stamp");

        let mut opacity_only = artifact.clone();
        opacity_only.effect_nodes[0].opacity = 0.25;
        for chunk in &mut opacity_only.chunks {
            if chunk.owner == root {
                chunk.content_revision.composite_revision =
                    chunk.content_revision.composite_revision.wrapping_add(100);
            }
        }
        assert_eq!(
            validated_root_effect_raster_stamp(&opacity_only, root_effect_raster_inputs())
                .expect("root opacity remains valid"),
            baseline
        );

        let mut self_paint = artifact.clone();
        self_paint.chunks[0].content_revision.self_paint_revision = self_paint.chunks[0]
            .content_revision
            .self_paint_revision
            .wrapping_add(1);
        assert_ne!(
            validated_root_effect_raster_stamp(&self_paint, root_effect_raster_inputs()).unwrap(),
            baseline
        );

        let mut topology = artifact.clone();
        topology.chunks[0].content_revision.topology_revision = topology.chunks[0]
            .content_revision
            .topology_revision
            .wrapping_add(1);
        assert_ne!(
            validated_root_effect_raster_stamp(&topology, root_effect_raster_inputs()).unwrap(),
            baseline
        );
    }

    #[test]
    fn root_effect_stamp_tracks_nested_exact_self_clip_snapshot() {
        let (arena, roots, anchor) = nested_anchor_parent_mixed_siblings(false);
        let root = roots[0];
        arena
            .get_mut(root)
            .unwrap()
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .unwrap()
            .set_opacity(0.5);
        let (properties, generations) = sync_identity(&arena, &roots);
        let (artifact, eligibility) =
            root_group_artifact(&arena, &roots, &properties, &generations);
        assert!(eligibility.eligible, "{eligibility:?}");
        let own = ClipNodeId {
            owner: anchor,
            role: ClipNodeRole::SelfClip,
        };
        let clip_index = artifact
            .clip_nodes
            .iter()
            .position(|clip| clip.id == own)
            .expect("nested exact self clip snapshot");
        let baseline = validated_root_effect_raster_stamp(&artifact, root_effect_raster_inputs())
            .expect("valid nested self clip stamp");

        let mut changed = artifact;
        changed.clip_nodes[clip_index].logical_scissor[2] += 1;
        assert_ne!(
            validated_root_effect_raster_stamp(&changed, root_effect_raster_inputs()).unwrap(),
            baseline
        );
    }

    #[test]
    fn root_effect_stamp_tracks_every_exact_viewport_raster_input() {
        let (arena, root, properties, generations) =
            prepared_leaf(0x6c21, Color::rgb(20, 120, 220), 0.5, true);
        let (artifact, _) = root_group_artifact(&arena, &[root], &properties, &generations);
        let baseline_inputs = root_effect_raster_inputs();
        let baseline =
            validated_root_effect_raster_stamp(&artifact, baseline_inputs).expect("baseline");

        let mut mismatches = Vec::new();
        let mut width = baseline_inputs;
        width.width += 1;
        mismatches.push(width);
        let mut height = baseline_inputs;
        height.height += 1;
        mismatches.push(height);
        let mut format = baseline_inputs;
        format.format = wgpu::TextureFormat::Rgba16Float;
        mismatches.push(format);
        let mut samples = baseline_inputs;
        samples.sample_count = 4;
        mismatches.push(samples);
        let mut scale = baseline_inputs;
        scale.scale_factor_bits = 2.0_f32.to_bits();
        mismatches.push(scale);

        for inputs in mismatches {
            assert_ne!(
                validated_root_effect_raster_stamp(&artifact, inputs).unwrap(),
                baseline
            );
        }
    }

    #[test]
    fn root_effect_stamp_tracks_descendant_composite_owner_topology_and_payload() {
        let mut arena = new_test_arena();
        let root = commit_element(
            &mut arena,
            Box::new(leaf_element(0x6c24, Color::rgb(220, 30, 20), 0.5, false)),
        );
        let mut child_element = leaf_element(0x6c25, Color::rgb(20, 40, 220), 1.0, false);
        child_element.set_position(0.0, 0.0);
        let child = commit_child(&mut arena, root, Box::new(child_element));
        let (measure, place) = constraints();
        measure_and_place(&mut arena, root, measure, place);
        let (properties, generations) = sync_identity(&arena, &[root]);
        let (artifact, _) = root_group_artifact(&arena, &[root], &properties, &generations);
        let baseline = validated_root_effect_raster_stamp(&artifact, root_effect_raster_inputs())
            .expect("baseline");

        let mut child_composite = artifact.clone();
        child_composite
            .chunks
            .iter_mut()
            .find(|chunk| chunk.owner == child)
            .expect("child chunk")
            .content_revision
            .composite_revision += 1;
        assert_ne!(
            validated_root_effect_raster_stamp(&child_composite, root_effect_raster_inputs())
                .unwrap(),
            baseline
        );

        let mut owner_order = artifact.clone();
        owner_order.owner_nodes.reverse();
        assert_ne!(
            validated_root_effect_raster_stamp(&owner_order, root_effect_raster_inputs()).unwrap(),
            baseline
        );

        let pixels: Arc<[u8]> = Arc::from([255_u8; 16]);
        let (image_arena, image_roots) = prepared_image_fixture(
            pixels,
            crate::view::ImageFit::Fill,
            crate::view::ImageSampling::Linear,
            0.4,
        );
        let (image_properties, image_generations) = sync_identity(&image_arena, &image_roots);
        let (image, _) = root_group_artifact(
            &image_arena,
            &image_roots,
            &image_properties,
            &image_generations,
        );
        let image_baseline =
            validated_root_effect_raster_stamp(&image, root_effect_raster_inputs()).unwrap();
        let mut payload_changed = image.clone();
        let decoration = payload_changed
            .chunks
            .iter()
            .find_map(|chunk| match &chunk.payload_identity {
                PaintPayloadIdentity::Image(_, decoration) => Some(Arc::clone(decoration)),
                _ => None,
            })
            .expect("image chunk composite identity");
        let prepared = payload_changed
            .ops
            .iter_mut()
            .find_map(|op| match op {
                PaintOp::PreparedImage(prepared) => Some(prepared),
                _ => None,
            })
            .expect("prepared image");
        prepared.upload.generation = prepared.upload.generation.wrapping_add(1);
        let identity = PreparedImageIdentity::from_op(prepared);
        payload_changed
            .chunks
            .iter_mut()
            .find(|chunk| chunk.id.role == PaintChunkRole::ImageContent)
            .expect("image chunk")
            .payload_identity = PaintPayloadIdentity::Image(identity, decoration);
        assert_ne!(
            validated_root_effect_raster_stamp(&payload_changed, root_effect_raster_inputs())
                .unwrap(),
            image_baseline
        );
    }

    #[test]
    fn root_effect_compiler_reuse_declares_pair_but_emits_only_composite() {
        let (arena, root, properties, generations) =
            prepared_leaf(0x6c22, Color::rgb(20, 120, 220), 0.5, true);
        let (artifact, _) = root_group_artifact(&arena, &[root], &properties, &generations);

        fn compile(
            artifact: &PaintArtifact,
            action: RootEffectCompileAction,
        ) -> (FrameGraph, BuildState) {
            let mut graph = FrameGraph::new();
            let mut ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);
            let target = ctx.allocate_target(&mut graph);
            ctx.set_current_target(target);
            graph.add_graphics_pass(crate::view::frame_graph::ClearPass::new(
                crate::view::render_pass::clear_pass::ClearParams::new([0.0; 4]),
                crate::view::render_pass::clear_pass::ClearInput {
                    pass_context: ctx.graphics_pass_context(),
                    clear_depth_stencil: true,
                },
                crate::view::render_pass::clear_pass::ClearOutput {
                    render_target: target,
                },
            ));
            let state = try_compile_root_effect_artifact(artifact, action, &mut graph, ctx)
                .unwrap_or_else(|_| panic!("valid root artifact"));
            (graph, state)
        }

        let (mut reraster, _) = compile(&artifact, RootEffectCompileAction::Reraster);
        let reraster_snapshot = reraster.test_compile_snapshot().unwrap();
        assert!(reraster_snapshot.pass_payloads().len() > 2);

        let (mut reuse, state) = compile(&artifact, RootEffectCompileAction::Reuse);
        let reuse_snapshot = reuse.test_compile_snapshot().unwrap();
        assert!(matches!(
            reuse_snapshot.pass_payloads(),
            [
                FramePassTestPayload::Clear(_),
                FramePassTestPayload::CompositeLayer(_)
            ]
        ));
        let declared = reuse
            .declared_persistent_texture_keys()
            .collect::<std::collections::HashSet<_>>();
        let color = crate::view::base_component::root_effect_stable_key(root);
        assert_eq!(declared.len(), 2);
        assert!(declared.contains(&color));
        assert!(declared.contains(&color.depth_stencil().unwrap()));
        assert!(state.current_target().is_some());
    }

    #[test]
    fn malformed_root_effect_reuse_rejects_before_any_emit() {
        let (arena, root, properties, generations) =
            prepared_leaf(0x6c23, Color::rgb(20, 120, 220), 0.5, true);
        let (mut artifact, _) = root_group_artifact(&arena, &[root], &properties, &generations);
        let PaintOp::DrawRect(op) = &mut artifact.ops[0] else {
            panic!("fixture begins with rect");
        };
        op.params.opacity = 0.5;
        let mut graph = FrameGraph::new();
        let ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8Unorm, 1.0);

        assert!(
            try_compile_root_effect_artifact(
                &artifact,
                RootEffectCompileAction::Reuse,
                &mut graph,
                ctx,
            )
            .is_err()
        );
        assert!(graph.pass_descriptors().is_empty());
        assert_eq!(graph.declared_persistent_texture_keys().count(), 0);
    }

    #[test]
    fn zero_and_half_root_group_have_identical_raster_payloads() {
        fn snapshot(opacity: f32) -> FrameGraphTestSnapshot {
            let (arena, root, properties, generations) =
                prepared_leaf(0x6c11, Color::rgb(20, 120, 220), opacity, true);
            let (artifact, _) = root_group_artifact(&arena, &[root], &properties, &generations);
            artifact.ops.iter().for_each(assert_neutral_opacity);
            let mut graph = compiled_whole_frame_graph(&artifact);
            graph.test_compile_snapshot().unwrap()
        }
        let zero = snapshot(0.0);
        let half = snapshot(0.5);
        assert_eq!(zero.pass_payloads().len(), half.pass_payloads().len());
        for (zero, half) in zero.pass_payloads().iter().zip(half.pass_payloads()) {
            match (zero, half) {
                (
                    FramePassTestPayload::CompositeLayer(zero),
                    FramePassTestPayload::CompositeLayer(half),
                ) => {
                    assert_eq!(zero.opacity_bits, 0.0_f32.to_bits());
                    assert_eq!(half.opacity_bits, 0.5_f32.to_bits());
                    let mut zero = zero.clone();
                    zero.opacity_bits = half.opacity_bits;
                    assert_eq!(&zero, half);
                }
                _ => assert_eq!(zero, half),
            }
        }
    }

    #[test]
    fn root_group_text_and_image_record_native_neutral_payloads_and_identities() {
        let (text_arena, text_roots, _) = prepared_text_tree(false);
        let (text_properties, text_generations) = sync_identity(&text_arena, &text_roots);
        let (text, _) = root_group_artifact(
            &text_arena,
            &text_roots,
            &text_properties,
            &text_generations,
        );
        assert!(
            text.ops
                .iter()
                .any(|op| matches!(op, PaintOp::PreparedText(_)))
        );
        text.ops.iter().for_each(assert_neutral_opacity);

        let (inline_arena, inline_roots, inline_owner) =
            prepared_inline_owned_text_tree(InlineOwnedTextDamage::None);
        let (inline_properties, inline_generations) = sync_identity(&inline_arena, &inline_roots);
        let (inline_text, _) = root_group_artifact(
            &inline_arena,
            &inline_roots,
            &inline_properties,
            &inline_generations,
        );
        assert!(matches!(
            inline_text.target,
            PaintArtifactTarget::RootOpacityGroup { root, .. } if root == inline_owner
        ));
        assert!(matches!(
            inline_text.chunks[0].payload_identity,
            PaintPayloadIdentity::PreparedTexts(_)
        ));
        inline_text.ops.iter().for_each(assert_neutral_opacity);
        let inline_graph = compiled_whole_frame_graph(&inline_text);
        assert_eq!(
            inline_graph
                .test_graphics_passes::<crate::view::render_pass::composite_layer_pass::CompositeLayerPass>()
                .len(),
            1
        );

        let pixels: Arc<[u8]> = Arc::from([255_u8; 16]);
        let (image_arena, image_roots) = prepared_image_fixture(
            pixels,
            crate::view::ImageFit::Fill,
            crate::view::ImageSampling::Linear,
            0.4,
        );
        let (image_properties, image_generations) = sync_identity(&image_arena, &image_roots);
        let (image, _) = root_group_artifact(
            &image_arena,
            &image_roots,
            &image_properties,
            &image_generations,
        );
        image.ops.iter().for_each(assert_neutral_opacity);
        let prepared = image
            .ops
            .iter()
            .find_map(|op| match op {
                PaintOp::PreparedImage(op) => Some(op),
                _ => None,
            })
            .expect("image group must own a prepared image");
        assert!(matches!(
            image.chunks[0].payload_identity,
            PaintPayloadIdentity::Image(actual, _)
                if actual == PreparedImageIdentity::from_op(prepared)
        ));
        let PaintPayloadIdentity::Image(identity, _) = image.chunks[0].payload_identity else {
            unreachable!()
        };
        assert_eq!(identity.opacity_bits, 1.0_f32.to_bits());
    }

    #[test]
    fn root_group_preflight_rejects_nested_non_effect_and_deferred_before_full_hooks() {
        fn nested_fixture() -> (NodeArena, Vec<NodeKey>) {
            let mut arena = new_test_arena();
            let root = commit_element(
                &mut arena,
                Box::new(leaf_element(0x6c20, Color::rgb(220, 30, 20), 0.5, false)),
            );
            commit_child(
                &mut arena,
                root,
                Box::new(leaf_element(0x6c21, Color::rgb(20, 40, 220), 0.25, false)),
            );
            let (measure, place) = constraints();
            measure_and_place(&mut arena, root, measure, place);
            (arena, vec![root])
        }

        let (nested_arena, nested_roots) = nested_fixture();
        let (nested_properties, nested_generations) = sync_identity(&nested_arena, &nested_roots);
        let _ = take_full_artifact_record_count();
        let nested = record_root_group_opacity_frame_artifact(
            &nested_arena,
            &nested_roots,
            &nested_properties,
            &nested_generations,
            RendererMode::ForcedForTests,
        )
        .unwrap_err();
        assert!(
            nested
                .reasons
                .iter()
                .any(|reason| matches!(reason, FrameArtifactFallbackReason::NestedEffect(_)))
        );
        assert_eq!(take_full_artifact_record_count(), 0);

        let (neutral_arena, neutral_root, neutral_properties, neutral_generations) =
            prepared_leaf(0x6c25, Color::rgb(80, 90, 100), 1.0, false);
        let missing = record_root_group_opacity_frame_artifact(
            &neutral_arena,
            &[neutral_root],
            &neutral_properties,
            &neutral_generations,
            RendererMode::ForcedForTests,
        )
        .unwrap_err();
        assert_eq!(
            missing.reasons,
            vec![FrameArtifactFallbackReason::MissingRootEffect(neutral_root)]
        );

        let (arena, root, mut properties, generations) =
            prepared_leaf(0x6c22, Color::rgb(30, 200, 80), 0.5, false);
        let state = properties.states.get_mut(&root).unwrap();
        state.paint.transform = Some(TransformNodeId(root));
        state.descendants.transform = Some(TransformNodeId(root));
        let non_effect = record_root_group_opacity_frame_artifact(
            &arena,
            &[root],
            &properties,
            &generations,
            RendererMode::ForcedForTests,
        )
        .unwrap_err();
        assert!(
            non_effect
                .reasons
                .contains(&FrameArtifactFallbackReason::NonEffectProperty(root))
        );

        let (arena, root, mut properties, generations) =
            prepared_leaf(0x6c26, Color::rgb(30, 200, 80), 0.5, false);
        let state = properties.states.get_mut(&root).unwrap();
        state.paint.scroll = Some(crate::view::compositor::property_tree::ScrollNodeId(root));
        state.descendants.scroll = Some(crate::view::compositor::property_tree::ScrollNodeId(root));
        let scroll = record_root_group_opacity_frame_artifact(
            &arena,
            &[root],
            &properties,
            &generations,
            RendererMode::ForcedForTests,
        )
        .unwrap_err();
        assert!(
            scroll
                .reasons
                .contains(&FrameArtifactFallbackReason::NonEffectProperty(root))
        );

        let mut deferred = leaf_element(0x6c24, Color::rgb(30, 200, 80), 0.5, false);
        let mut style = Style::new();
        style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(1.0))
                    .top(Length::px(1.0))
                    .clip(ClipMode::Viewport),
            ),
        );
        style.insert(PropertyId::Opacity, ParsedValue::Opacity(Opacity::new(0.5)));
        deferred.apply_style(style);
        let mut arena = new_test_arena();
        let root = commit_element(&mut arena, Box::new(deferred));
        let (measure, place) = constraints();
        measure_and_place(&mut arena, root, measure, place);
        let (properties, generations) = sync_identity(&arena, &[root]);
        let deferred = record_root_group_opacity_frame_artifact(
            &arena,
            &[root],
            &properties,
            &generations,
            RendererMode::ForcedForTests,
        )
        .unwrap_err();
        assert!(
            deferred
                .reasons
                .contains(&FrameArtifactFallbackReason::DeferredBoundary(root)),
            "{deferred:?}"
        );
    }

    #[test]
    fn root_group_compiler_rejects_missing_dangling_baked_and_double_applied_effects_before_emit() {
        let mut arena = new_test_arena();
        let root = commit_element(
            &mut arena,
            Box::new(leaf_element(0x6c30, Color::rgb(220, 30, 20), 0.5, false)),
        );
        let mut child_element = leaf_element(0x6c31, Color::rgb(20, 40, 220), 1.0, false);
        child_element.set_position(0.0, 0.0);
        let child = commit_child(&mut arena, root, Box::new(child_element));
        let (measure, place) = constraints();
        measure_and_place(&mut arena, root, measure, place);
        let (properties, generations) = sync_identity(&arena, &[root]);
        let (valid, _) = root_group_artifact(&arena, &[root], &properties, &generations);

        let mut missing = valid.clone();
        missing.effect_nodes.clear();
        assert_compiler_rejects_before_emit(&missing, "missing root group effect");

        let mut dangling = valid.clone();
        dangling.effect_nodes[0].parent = Some(EffectNodeId(NodeKey::null()));
        assert_compiler_rejects_before_emit(&dangling, "dangling root group effect");

        let mut baked = valid.clone();
        let PaintOp::DrawRect(op) = &mut baked.ops[0] else {
            panic!("root fixture begins with DrawRect")
        };
        op.params.opacity = 0.5;
        assert_compiler_rejects_before_emit(&baked, "baked root group opacity");

        let mut transformed = valid.clone();
        transformed.chunks[0].properties.transform = Some(TransformNodeId(root));
        assert_compiler_rejects_before_emit(&transformed, "root group transform property");

        let mut scrolled = valid.clone();
        scrolled.chunks[0].properties.scroll =
            Some(crate::view::compositor::property_tree::ScrollNodeId(root));
        assert_compiler_rejects_before_emit(&scrolled, "root group scroll property");

        let mut double_applied = valid;
        let child_chunk = double_applied
            .chunks
            .iter()
            .find(|chunk| chunk.owner == child)
            .unwrap();
        let PaintOp::DrawRect(op) = &mut double_applied.ops[child_chunk.op_range.start] else {
            panic!("child fixture begins with DrawRect")
        };
        op.params.opacity = 0.5;
        assert_compiler_rejects_before_emit(&double_applied, "double-applied child opacity");
    }

    fn compiled_whole_frame_graph(artifact: &PaintArtifact) -> FrameGraph {
        compiled_whole_frame_graph_with_config(artifact, PaintParityConfig::default())
    }

    #[derive(Clone, Copy)]
    struct PaintParityConfig {
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
        scale_factor: f32,
        initial_scissor: Option<[u32; 4]>,
    }

    impl Default for PaintParityConfig {
        fn default() -> Self {
            Self {
                width: 320,
                height: 240,
                format: wgpu::TextureFormat::Bgra8Unorm,
                scale_factor: 1.0,
                initial_scissor: None,
            }
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct ViewportRasterFingerprint {
        logical_width_bits: u32,
        logical_height_bits: u32,
        target_width: u32,
        target_height: u32,
        target_format: wgpu::TextureFormat,
        scale_factor_bits: u32,
    }

    impl From<PaintParityConfig> for ViewportRasterFingerprint {
        fn from(config: PaintParityConfig) -> Self {
            let scale_factor = config.scale_factor.max(0.0001);
            Self {
                logical_width_bits: (config.width as f32 / scale_factor).to_bits(),
                logical_height_bits: (config.height as f32 / scale_factor).to_bits(),
                target_width: config.width,
                target_height: config.height,
                target_format: config.format,
                scale_factor_bits: scale_factor.to_bits(),
            }
        }
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct PaintParitySnapshot {
        viewport: ViewportRasterFingerprint,
        graph: FrameGraphTestSnapshot,
    }

    fn strict_paint_snapshot(
        graph: &mut FrameGraph,
        config: PaintParityConfig,
    ) -> PaintParitySnapshot {
        PaintParitySnapshot {
            viewport: config.into(),
            graph: graph
                .test_compile_snapshot()
                .expect("paint parity graph must have complete strict test coverage"),
        }
    }

    fn compiled_whole_frame_graph_with_config(
        artifact: &PaintArtifact,
        config: PaintParityConfig,
    ) -> FrameGraph {
        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(
            config.width,
            config.height,
            config.format,
            config.scale_factor,
        );
        let target = ctx.allocate_target(&mut graph);
        ctx.set_current_target(target.clone());
        let clear = crate::view::frame_graph::ClearPass::new(
            crate::view::render_pass::clear_pass::ClearParams::new([0.0, 0.0, 0.0, 0.0]),
            crate::view::render_pass::clear_pass::ClearInput {
                pass_context: ctx.graphics_pass_context(),
                clear_depth_stencil: true,
            },
            crate::view::render_pass::clear_pass::ClearOutput {
                render_target: target.clone(),
            },
        );
        if let Some(handle) = target.handle() {
            ctx.set_color_target(Some(handle));
        }
        graph.add_graphics_pass(clear);
        ctx.set_current_target(target);
        if let Some(scissor) = config.initial_scissor {
            ctx.replace_scissor_rect(Some(scissor));
        }
        let _ = compile_artifact(artifact, &mut graph, ctx);
        graph
    }

    fn legacy_roots_graph(arena: NodeArena, roots: &[NodeKey]) -> FrameGraph {
        legacy_roots_graph_with_config(arena, roots, PaintParityConfig::default())
    }

    fn legacy_roots_graph_with_config(
        mut arena: NodeArena,
        roots: &[NodeKey],
        config: PaintParityConfig,
    ) -> FrameGraph {
        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(
            config.width,
            config.height,
            config.format,
            config.scale_factor,
        );
        let target = ctx.allocate_target(&mut graph);
        ctx.set_current_target(target.clone());
        let clear = crate::view::frame_graph::ClearPass::new(
            crate::view::render_pass::clear_pass::ClearParams::new([0.0, 0.0, 0.0, 0.0]),
            crate::view::render_pass::clear_pass::ClearInput {
                pass_context: ctx.graphics_pass_context(),
                clear_depth_stencil: true,
            },
            crate::view::render_pass::clear_pass::ClearOutput {
                render_target: target.clone(),
            },
        );
        if let Some(handle) = target.handle() {
            ctx.set_color_target(Some(handle));
        }
        graph.add_graphics_pass(clear);
        ctx.set_current_target(target);
        if let Some(scissor) = config.initial_scissor {
            ctx.replace_scissor_rect(Some(scissor));
        }
        for &root in roots {
            let child_ctx = UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone());
            let next = arena
                .with_element_taken(root, |element, arena| {
                    element.build(&mut graph, arena, child_ctx)
                })
                .expect("legacy root should build");
            ctx.set_state(next);
        }
        graph
    }

    fn assert_whole_frame_structural_parity<F>(
        fixture: F,
        config: PaintParityConfig,
    ) -> Vec<RectPassTestSnapshot>
    where
        F: Fn() -> (NodeArena, Vec<NodeKey>),
    {
        let (artifact_arena, artifact_roots) = fixture();
        let (properties, generations) = sync_identity(&artifact_arena, &artifact_roots);
        let (artifact, eligibility) =
            whole_frame_artifact(&artifact_arena, &artifact_roots, &properties, &generations);
        assert!(eligibility.eligible);
        drop(artifact_arena);
        let mut artifact_graph = compiled_whole_frame_graph_with_config(&artifact, config);

        let (legacy_arena, legacy_roots) = fixture();
        let mut legacy_graph = legacy_roots_graph_with_config(legacy_arena, &legacy_roots, config);

        let artifact_snapshot = strict_paint_snapshot(&mut artifact_graph, config);
        let legacy_snapshot = strict_paint_snapshot(&mut legacy_graph, config);
        assert_eq!(artifact_snapshot, legacy_snapshot);
        artifact_graph.test_rect_pass_snapshots()
    }

    fn prepared_text_tree(nested: bool) -> (NodeArena, Vec<NodeKey>, NodeKey) {
        let mut arena = new_test_arena();
        let mut text = Text::new_with_id(
            180,
            3.4,
            5.6,
            if nested { 120.0 } else { 92.0 },
            if nested { 40.0 } else { 72.0 },
            if nested {
                "nested retained text"
            } else {
                "retained text wraps across lines\nwith alignment"
            },
        );
        text.set_color(Color::rgb(24, 96, 210));
        text.set_font("sans-serif");
        text.set_font_size(19.5);
        text.set_font_weight(650);
        text.set_line_height(1.35);
        text.set_text_align(TextAlign::Center);
        text.set_text_wrap(TextWrap::Wrap);
        text.set_opacity(0.72);

        let (roots, text_key) = if nested {
            let mut parent = Element::new_with_id(179, 10.25, 20.75, 300.0, 180.0);
            let mut parent_style = Style::new();
            parent_style.insert(
                PropertyId::Layout,
                ParsedValue::Layout(Layout::flex().column().into()),
            );
            parent.apply_style(parent_style);
            let root = commit_element(&mut arena, Box::new(parent));
            let text_key = commit_child(&mut arena, root, Box::new(text));
            (vec![root], text_key)
        } else {
            let text_key = commit_element(&mut arena, Box::new(text));
            (vec![text_key], text_key)
        };
        let (measure, place) = constraints();
        for &root in &roots {
            measure_and_place(&mut arena, root, measure, place);
        }
        (arena, roots, text_key)
    }

    fn prepared_plain_text_area_tree_with(
        content: &str,
        placeholder: &str,
        width: f32,
        origin: [f32; 2],
    ) -> (NodeArena, Vec<NodeKey>, NodeKey) {
        let mut arena = new_test_arena();
        let mut text_area = TextArea::with_stable_id(0x7e00);
        text_area.set_text(content.to_string());
        text_area.placeholder = placeholder.to_string();
        text_area.font_families = vec!["sans-serif".to_string()];
        text_area.font_size = 17.5;
        text_area.line_height = 1.3;
        text_area.set_layout_offset(0.35, 0.65);
        let root = commit_element(&mut arena, Box::new(text_area));
        arena.with_element_taken(root, |element, _arena| {
            element
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .unwrap()
                .set_self_node_key(root);
        });
        let measure = LayoutConstraints {
            max_width: width,
            max_height: 240.0,
            viewport_width: 320.0,
            viewport_height: 240.0,
            percent_base_width: Some(320.0),
            percent_base_height: Some(240.0),
        };
        let place = LayoutPlacement {
            parent_x: origin[0],
            parent_y: origin[1],
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: width,
            available_height: 240.0,
            viewport_width: 320.0,
            viewport_height: 240.0,
            percent_base_width: Some(320.0),
            percent_base_height: Some(240.0),
        };
        measure_and_place(&mut arena, root, measure, place);
        let mut keys = vec![root];
        keys.extend(arena.children_of(root));
        for key in keys {
            arena
                .get_mut(key)
                .unwrap()
                .element
                .clear_local_dirty_flags(DirtyFlags::ALL);
        }
        arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
        arena.refresh_subtree_dirty_cache(root);
        (arena, vec![root], root)
    }

    fn prepared_plain_text_area_tree(content: &str) -> (NodeArena, Vec<NodeKey>, NodeKey) {
        prepared_plain_text_area_tree_with(content, "", 108.0, [7.25, 11.75])
    }

    fn prepared_plain_text_area_selection_tree(
        content: &str,
        width: f32,
        anchor: usize,
        focus: usize,
    ) -> (NodeArena, Vec<NodeKey>, NodeKey) {
        let (arena, roots, root) =
            prepared_plain_text_area_tree_with(content, "", width, [7.25, 11.75]);
        {
            let mut node = arena.get_mut(root).unwrap();
            let text_area = node
                .element
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .unwrap();
            text_area.selection_anchor_char = Some(anchor);
            text_area.selection_focus_char = Some(focus);
        }
        (arena, roots, root)
    }

    fn prepared_plain_text_area_preedit_tree(
        content: &str,
        width: f32,
        cursor_char: usize,
        preedit: &str,
        preedit_cursor: Option<(usize, usize)>,
    ) -> (NodeArena, Vec<NodeKey>, NodeKey) {
        let (mut arena, roots, root) =
            prepared_plain_text_area_tree_with(content, "", width, [7.25, 11.75]);
        arena.with_element_taken(root, |element, _arena| {
            let text_area = element.as_any_mut().downcast_mut::<TextArea>().unwrap();
            text_area.cursor_char = cursor_char;
            text_area.ime_preedit = preedit.to_string();
            text_area.ime_preedit_cursor = preedit_cursor;
            text_area.is_focused = true;
            text_area.caret_visible = true;
            text_area.caret_blink_epoch = None;
            text_area.children_dirty = true;
            text_area.bump_unified_ifc_source_revision();
            text_area.dirty_flags = DirtyFlags::ALL;
        });
        let measure = LayoutConstraints {
            max_width: width,
            max_height: 240.0,
            viewport_width: 320.0,
            viewport_height: 240.0,
            percent_base_width: Some(320.0),
            percent_base_height: Some(240.0),
        };
        let place = LayoutPlacement {
            parent_x: 7.25,
            parent_y: 11.75,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: width,
            available_height: 240.0,
            viewport_width: 320.0,
            viewport_height: 240.0,
            percent_base_width: Some(320.0),
            percent_base_height: Some(240.0),
        };
        measure_and_place(&mut arena, root, measure, place);
        settle_plain_text_area(&arena, root);
        (arena, roots, root)
    }

    fn prepared_projection_text_area_tree_with(
        content: &'static str,
        projection_range: std::ops::Range<usize>,
        projected_content: &'static str,
    ) -> (NodeArena, Vec<NodeKey>, NodeKey, NodeKey, NodeKey) {
        let width = 132.0;
        let (mut arena, roots, root) =
            prepared_plain_text_area_tree_with(content, "", width, [7.25, 11.75]);
        arena.with_element_taken(root, |element, _arena| {
            let text_area = element.as_any_mut().downcast_mut::<TextArea>().unwrap();
            text_area.on_render_handler = Some(crate::ui::on_text_area_render(move |render| {
                render.range(projection_range.clone(), move |_text_area| {
                    crate::ui::RsxNode::text(projected_content)
                })
            }));
            text_area.children_dirty = true;
            text_area.bump_unified_ifc_source_revision();
            text_area.dirty_flags = DirtyFlags::ALL;
        });
        let measure = LayoutConstraints {
            max_width: width,
            max_height: 240.0,
            viewport_width: 320.0,
            viewport_height: 240.0,
            percent_base_width: Some(320.0),
            percent_base_height: Some(240.0),
        };
        let place = LayoutPlacement {
            parent_x: 7.25,
            parent_y: 11.75,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: width,
            available_height: 240.0,
            viewport_width: 320.0,
            viewport_height: 240.0,
            percent_base_width: Some(320.0),
            percent_base_height: Some(240.0),
        };
        measure_and_place(&mut arena, root, measure, place);
        let projection = arena
            .children_of(root)
            .into_iter()
            .find(|&key| {
                arena
                    .get(key)
                    .unwrap()
                    .element
                    .as_any()
                    .is::<TextAreaProjectionSegment>()
            })
            .expect("fixture must build one projection wrapper");
        let mut projection_descendants = Vec::new();
        let mut stack = arena.children_of(projection);
        while let Some(key) = stack.pop() {
            stack.extend(arena.children_of(key));
            projection_descendants.push(key);
        }
        let projected_text = projection_descendants
            .iter()
            .copied()
            .find(|&key| arena.get(key).unwrap().element.as_any().is::<Text>())
            .expect("fixture projection must contain one Text descendant");
        let mut stack = vec![root];
        while let Some(key) = stack.pop() {
            stack.extend(arena.children_of(key));
            arena
                .get_mut(key)
                .unwrap()
                .element
                .clear_local_dirty_flags(DirtyFlags::ALL);
        }
        arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
        arena.refresh_subtree_dirty_cache(root);
        (arena, roots, root, projection, projected_text)
    }

    fn prepared_projection_text_area_tree() -> (NodeArena, Vec<NodeKey>, NodeKey, NodeKey, NodeKey)
    {
        prepared_projection_text_area_tree_with("before projected after", 7..16, "projected")
    }

    #[derive(Clone, Copy)]
    struct AtomicProjectionScrollFixture {
        content: &'static str,
        projection_start: usize,
        projection_end: usize,
        projected_content: &'static str,
        font_size: f32,
        line_height: f32,
        width: f32,
        content_height: f32,
        scroll_y: f32,
    }

    impl AtomicProjectionScrollFixture {
        fn baseline(projected_content: &'static str, scroll_y: f32) -> Self {
            Self {
                content: "before projected after",
                projection_start: 7,
                projection_end: 16,
                projected_content,
                font_size: 14.0,
                line_height: 1.25,
                width: 132.0,
                content_height: 300.0,
                scroll_y,
            }
        }
    }

    fn prepared_atomic_projection_scroll_shell_with(
        projected_content: &'static str,
    ) -> (NodeArena, NodeKey, NodeKey, NodeKey) {
        prepared_atomic_projection_scroll_shell_fixture(AtomicProjectionScrollFixture::baseline(
            projected_content,
            20.0,
        ))
    }

    fn prepared_atomic_projection_scroll_shell_fixture(
        fixture: AtomicProjectionScrollFixture,
    ) -> (NodeArena, NodeKey, NodeKey, NodeKey) {
        let mut text_component = TextArea::new();
        text_component.content = fixture.content.to_string();
        text_component.font_size = fixture.font_size;
        text_component.line_height = fixture.line_height;
        let projection_start = fixture.projection_start;
        let projection_end = fixture.projection_end;
        let projected_content = fixture.projected_content;
        text_component.on_render_handler = Some(crate::ui::on_text_area_render(move |render| {
            render.range(projection_start..projection_end, move |_text_area| {
                crate::ui::RsxNode::text(projected_content)
            });
        }));
        let mut arena = new_test_arena();
        let text_area = commit_element(&mut arena, Box::new(text_component));
        arena.with_element_taken(text_area, |element, _| {
            element
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .unwrap()
                .set_self_node_key(text_area);
        });
        measure_and_place(
            &mut arena,
            text_area,
            LayoutConstraints {
                max_width: fixture.width,
                max_height: 240.0,
                viewport_width: 320.0,
                viewport_height: 240.0,
                percent_base_width: Some(320.0),
                percent_base_height: Some(240.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: fixture.width,
                available_height: 240.0,
                viewport_width: 320.0,
                viewport_height: 240.0,
                percent_base_width: Some(320.0),
                percent_base_height: Some(240.0),
            },
        );
        let scroll_y = fixture.scroll_y;
        let wrapper = commit_element(
            &mut arena,
            Box::new(Element::new_with_id(
                0xc3a_3001,
                0.0,
                -scroll_y,
                fixture.width,
                fixture.content_height,
            )),
        );
        let root = commit_element(
            &mut arena,
            Box::new(Element::new_with_id(
                0xc3a_3000,
                0.0,
                0.0,
                fixture.width,
                80.0,
            )),
        );
        arena.set_parent(text_area, Some(wrapper));
        arena.set_children(wrapper, vec![text_area]);
        arena.set_parent(wrapper, Some(root));
        arena.set_children(root, vec![wrapper]);
        arena.with_element_taken(text_area, |element, arena| {
            element.place(
                LayoutPlacement {
                    parent_x: 0.0,
                    parent_y: -scroll_y,
                    visual_offset_x: 0.0,
                    visual_offset_y: 0.0,
                    available_width: fixture.width,
                    available_height: 240.0,
                    viewport_width: 320.0,
                    viewport_height: 240.0,
                    percent_base_width: Some(320.0),
                    percent_base_height: Some(240.0),
                },
                arena,
            );
        });
        let mut wrapper_style = Style::new();
        wrapper_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        crate::view::test_support::get_element_mut::<Element>(&arena, wrapper)
            .apply_style(wrapper_style);
        let mut root_style = Style::new();
        root_style.insert(
            PropertyId::ScrollDirection,
            ParsedValue::ScrollDirection(ScrollDirection::Vertical),
        );
        root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        {
            let mut root_element =
                crate::view::test_support::get_element_mut::<Element>(&arena, root);
            root_element.apply_style(root_style);
            root_element.layout_state.content_size = Size {
                width: fixture.width,
                height: fixture.content_height,
            };
            root_element.set_scroll_offset((0.0, scroll_y));
            root_element.clear_local_dirty_flags(DirtyFlags::ALL);
        }
        let mut stack = vec![wrapper];
        while let Some(key) = stack.pop() {
            stack.extend(arena.children_of(key));
            arena
                .get_mut(key)
                .unwrap()
                .element
                .clear_local_dirty_flags(DirtyFlags::ALL);
        }
        arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
        arena.refresh_subtree_dirty_cache(root);
        (arena, root, wrapper, text_area)
    }

    fn prepared_atomic_projection_scroll_shell() -> (NodeArena, NodeKey, NodeKey, NodeKey) {
        prepared_atomic_projection_scroll_shell_with("projected")
    }

    fn validated_atomic_projection_scroll_scene_at(
        projected_content: &'static str,
        scroll_y: f32,
    ) -> super::scroll_scene::ValidatedPropertyScrollScene {
        validated_atomic_projection_scroll_scene_fixture(AtomicProjectionScrollFixture::baseline(
            projected_content,
            scroll_y,
        ))
    }

    fn validated_atomic_projection_scroll_scene_fixture(
        fixture: AtomicProjectionScrollFixture,
    ) -> super::scroll_scene::ValidatedPropertyScrollScene {
        let (arena, root, _, _) = prepared_atomic_projection_scroll_shell_fixture(fixture);
        let (properties, generations) = sync_identity(&arena, &[root]);
        let budget =
            super::scroll_scene::ScrollSceneSingleTextureBudget::new(8192, 128 * 1024 * 1024)
                .unwrap();
        super::scroll_scene::plan_and_validate_property_scroll_scene(
            &arena,
            &[root],
            &properties,
            &generations,
            1.0,
            [0.0; 2],
            None,
            crate::time::Instant::now(),
            wgpu::TextureFormat::Bgra8UnormSrgb,
            budget,
        )
        .expect("valid C3a fixture must compiler-seal one property-scroll scene")
    }

    fn validated_atomic_projection_selection_scroll_scene_at(
        selection_end: usize,
    ) -> super::scroll_scene::ValidatedPropertyScrollScene {
        validated_atomic_projection_selection_scroll_scene_fixture(
            AtomicProjectionScrollFixture::baseline("projected", 20.0),
            selection_end,
        )
    }

    fn validated_atomic_projection_selection_scroll_scene_fixture(
        fixture: AtomicProjectionScrollFixture,
        selection_end: usize,
    ) -> super::scroll_scene::ValidatedPropertyScrollScene {
        let (arena, root, _, text_area) = prepared_atomic_projection_scroll_shell_fixture(fixture);
        {
            let mut node = arena.get_mut(text_area).unwrap();
            let text_area = node
                .element
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .unwrap();
            text_area.selection_anchor_char = Some(0);
            text_area.selection_focus_char = Some(selection_end);
        }
        let (properties, generations) = sync_identity(&arena, &[root]);
        let budget =
            super::scroll_scene::ScrollSceneSingleTextureBudget::new(8192, 128 * 1024 * 1024)
                .unwrap();
        super::scroll_scene::plan_and_validate_property_scroll_scene(
            &arena,
            &[root],
            &properties,
            &generations,
            1.0,
            [0.0; 2],
            None,
            crate::time::Instant::now(),
            wgpu::TextureFormat::Bgra8UnormSrgb,
            budget,
        )
        .expect("valid selection grammar must selector-plan-compile one property-scroll scene")
    }

    fn atomic_projection_content_stamp_for_test(
        projected_content: &'static str,
        stable_id: u64,
    ) -> Option<RetainedSurfaceRasterStamp> {
        atomic_projection_emission_fixture_for_test(projected_content, stable_id)
            .map(|(_, stamp)| stamp)
    }

    fn atomic_projection_emission_fixture_for_test(
        projected_content: &'static str,
        stable_id: u64,
    ) -> Option<(
        std::sync::Arc<super::compiler::ValidatedScrollSceneAtomicProjectionTextAreaPlanParts>,
        RetainedSurfaceRasterStamp,
    )> {
        let (arena, root, wrapper, _) =
            prepared_atomic_projection_scroll_shell_with(projected_content);
        let root_node = arena.get(root)?;
        let root_element = root_node.element.as_any().downcast_ref::<Element>()?;
        let admission = root_element
            .exact_retained_scroll_atomic_projection_text_area_subtree_admission(
                root, &arena, 1.0,
            )?;
        drop(root_node);
        let (properties, generations) = sync_identity(&arena, &[root]);
        let scroll = properties
            .scroll_snapshot_for(crate::view::compositor::property_tree::ScrollNodeId(root))?;
        let outer_clip = *properties
            .clip_snapshot_for(Some(ClipNodeId {
                owner: root,
                role: ClipNodeRole::ContentsClip,
            }))?
            .last()?;
        let outer = PaintScrollContentWitness::new(root, wrapper, scroll, outer_clip)?;
        let baked = PaintBakedScrollHostWitness::new(root, wrapper, scroll, outer_clip.id)?;
        let local = super::frame_recorder::record_scroll_atomic_projection_text_area_subtree_local_artifact_for_plan(
            &arena,
            &properties,
            &generations,
            &admission,
            outer,
        ).ok()?;
        let host = super::frame_recorder::record_baked_scroll_atomic_projection_text_area_subtree_host_artifact_for_plan(
            &arena,
            &[root],
            &properties,
            &generations,
            &admission,
            baked,
        ).ok()?;
        let plan_parts =
            super::frame_recorder::validate_recorded_atomic_projection_text_area_plan_parts(
                host, local,
            )?;
        let terminal = plan_parts.content_opaque_order_count()?;
        let span = plan_parts.content_artifact_span_stamp(0, 0..terminal)?;
        let [x, y, width, height] = plan_parts
            .resident()
            .wrapper_chunk
            .bounds_bits
            .map(f32::from_bits);
        let bounds = crate::view::base_component::RetainedSurfaceBounds {
            x,
            y,
            width,
            height,
            corner_radii: [0.0; 4],
        };
        let color_key = crate::view::base_component::scroll_content_layer_stable_key(stable_id);
        let color = crate::view::base_component::texture_desc_for_logical_bounds(
            bounds,
            1.0,
            None,
            wgpu::TextureFormat::Bgra8UnormSrgb,
        );
        let (color, depth) =
            crate::view::base_component::persistent_target_texture_descriptors(color, color_key);
        let stamp =
            super::compiler::validated_scroll_atomic_projection_text_area_content_raster_stamp(
                wrapper,
                stable_id,
                RetainedSurfaceRasterInputs {
                    color,
                    depth,
                    scale_factor_bits: 1.0_f32.to_bits(),
                    source_bounds_bits: [x, y, width, height].map(f32::to_bits),
                },
                span,
                0..terminal,
                plan_parts.resident().clone(),
            )?;
        Some((std::sync::Arc::new(plan_parts), stamp))
    }

    fn atomic_projection_selection_content_stamp_for_test(
        selection_end: usize,
        stable_id: u64,
    ) -> Option<RetainedSurfaceRasterStamp> {
        atomic_projection_selection_emission_fixture_for_test(selection_end, stable_id)
            .map(|(_, stamp)| stamp)
    }

    fn atomic_projection_selection_emission_fixture_for_test(
        selection_end: usize,
        stable_id: u64,
    ) -> Option<(
        std::sync::Arc<
            super::compiler::ValidatedScrollSceneAtomicProjectionSelectionTextAreaPlanParts,
        >,
        RetainedSurfaceRasterStamp,
    )> {
        let (arena, root, wrapper, text_area) = prepared_atomic_projection_scroll_shell();
        {
            let mut node = arena.get_mut(text_area)?;
            let text_area = node.element.as_any_mut().downcast_mut::<TextArea>()?;
            text_area.selection_anchor_char = Some(0);
            text_area.selection_focus_char = Some(selection_end);
        }
        let root_node = arena.get(root)?;
        let root_element = root_node.element.as_any().downcast_ref::<Element>()?;
        let admission = root_element
            .exact_retained_scroll_atomic_projection_selection_text_area_subtree_admission(
                root, &arena, 1.0,
            )?;
        drop(root_node);
        let (properties, generations) = sync_identity(&arena, &[root]);
        let scroll = properties
            .scroll_snapshot_for(crate::view::compositor::property_tree::ScrollNodeId(root))?;
        let outer_clip = *properties
            .clip_snapshot_for(Some(ClipNodeId {
                owner: root,
                role: ClipNodeRole::ContentsClip,
            }))?
            .last()?;
        let outer = PaintScrollContentWitness::new(root, wrapper, scroll, outer_clip)?;
        let baked = PaintBakedScrollHostWitness::new(root, wrapper, scroll, outer_clip.id)?;
        let local = super::frame_recorder::record_scroll_atomic_projection_selection_text_area_subtree_local_artifact_for_plan(
            &arena,
            &properties,
            &generations,
            &admission,
            outer,
        ).ok()?;
        let host = super::frame_recorder::record_baked_scroll_atomic_projection_selection_text_area_subtree_host_artifact_for_plan(
            &arena,
            &[root],
            &properties,
            &generations,
            &admission,
            baked,
        ).ok()?;
        let authority =
            super::frame_recorder::validate_recorded_atomic_projection_selection_text_area_authority(
                host, local,
            )?;
        let plan_parts =
            super::frame_recorder::validate_recorded_atomic_projection_selection_text_area_plan_parts(
                authority,
            )?;
        let terminal = plan_parts.content_opaque_order_count()?;
        let span = plan_parts.content_artifact_span_stamp(0, 0..terminal)?;
        let [x, y, width, height] = plan_parts
            .resident()
            .wrapper_chunk
            .bounds_bits
            .map(f32::from_bits);
        let bounds = crate::view::base_component::RetainedSurfaceBounds {
            x,
            y,
            width,
            height,
            corner_radii: [0.0; 4],
        };
        let color_key = crate::view::base_component::scroll_content_layer_stable_key(stable_id);
        let color = crate::view::base_component::texture_desc_for_logical_bounds(
            bounds,
            1.0,
            None,
            wgpu::TextureFormat::Bgra8UnormSrgb,
        );
        let (color, depth) =
            crate::view::base_component::persistent_target_texture_descriptors(color, color_key);
        let stamp = super::compiler::validated_scroll_atomic_projection_selection_text_area_content_raster_stamp(
            wrapper,
            stable_id,
            RetainedSurfaceRasterInputs {
                color,
                depth,
                scale_factor_bits: 1.0_f32.to_bits(),
                source_bounds_bits: [x, y, width, height].map(f32::to_bits),
            },
            span,
            0..terminal,
            plan_parts.resident().clone(),
        )?;
        Some((std::sync::Arc::new(plan_parts), stamp))
    }

    fn prepared_projection_text_area_preedit_tree(
        cursor_char: usize,
        preedit: &str,
        preedit_cursor: Option<(usize, usize)>,
    ) -> (NodeArena, Vec<NodeKey>, NodeKey, NodeKey, NodeKey) {
        let width = 132.0;
        let (mut arena, roots, root, _, _) = prepared_projection_text_area_tree();
        arena.with_element_taken(root, |element, _arena| {
            let text_area = element.as_any_mut().downcast_mut::<TextArea>().unwrap();
            text_area.cursor_char = cursor_char;
            text_area.ime_preedit = preedit.to_string();
            text_area.ime_preedit_cursor = preedit_cursor;
            text_area.is_focused = true;
            text_area.caret_visible = true;
            text_area.children_dirty = true;
            text_area.bump_unified_ifc_source_revision();
            text_area.dirty_flags = DirtyFlags::ALL;
        });
        let measure = LayoutConstraints {
            max_width: width,
            max_height: 240.0,
            viewport_width: 320.0,
            viewport_height: 240.0,
            percent_base_width: Some(320.0),
            percent_base_height: Some(240.0),
        };
        let place = LayoutPlacement {
            parent_x: 7.25,
            parent_y: 11.75,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: width,
            available_height: 240.0,
            viewport_width: 320.0,
            viewport_height: 240.0,
            percent_base_width: Some(320.0),
            percent_base_height: Some(240.0),
        };
        measure_and_place(&mut arena, root, measure, place);
        let projection = arena
            .children_of(root)
            .into_iter()
            .find(|&key| {
                arena
                    .get(key)
                    .unwrap()
                    .element
                    .as_any()
                    .is::<TextAreaProjectionSegment>()
            })
            .unwrap();
        let projection_children = arena.children_of(projection);
        let [projected_text] = projection_children.as_slice() else {
            panic!("projection preedit fixture requires one direct Text")
        };
        let projected_text = *projected_text;
        assert!(
            arena
                .get(projected_text)
                .unwrap()
                .element
                .as_any()
                .is::<Text>()
        );
        let mut stack = vec![root];
        while let Some(key) = stack.pop() {
            stack.extend(arena.children_of(key));
            arena
                .get_mut(key)
                .unwrap()
                .element
                .clear_local_dirty_flags(DirtyFlags::ALL);
        }
        arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
        arena.refresh_subtree_dirty_cache(root);
        (arena, roots, root, projection, projected_text)
    }

    fn settle_plain_text_area(arena: &NodeArena, root: NodeKey) {
        let mut keys = vec![root];
        keys.extend(arena.children_of(root));
        for key in keys {
            arena
                .get_mut(key)
                .unwrap()
                .element
                .clear_local_dirty_flags(DirtyFlags::ALL);
        }
        arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
        arena.refresh_subtree_dirty_cache(root);
    }

    fn place_text_area_with_baked_scroll(
        arena: &mut NodeArena,
        root: NodeKey,
        width: f32,
        height: f32,
        scroll: [f32; 2],
    ) {
        arena.with_element_taken(root, |element, arena| {
            element.set_layout_height(height);
            {
                let text_area = element.as_any_mut().downcast_mut::<TextArea>().unwrap();
                let max_x = (text_area.layout_state.content_size.width
                    - text_area.viewport_size.width)
                    .max(0.0);
                let max_y = (text_area.layout_state.content_size.height
                    - text_area.viewport_size.height)
                    .max(0.0);
                assert!(
                    scroll[0] <= max_x,
                    "horizontal fixture scroll must be clamped"
                );
                assert!(
                    scroll[1] <= max_y,
                    "vertical fixture scroll must be clamped"
                );
                text_area.scroll_x = scroll[0];
                text_area.scroll_y = scroll[1];
            }
            element.place(
                LayoutPlacement {
                    parent_x: 7.25,
                    parent_y: 11.75,
                    visual_offset_x: 0.0,
                    visual_offset_y: 0.0,
                    available_width: width,
                    available_height: height,
                    viewport_width: 320.0,
                    viewport_height: 240.0,
                    percent_base_width: Some(320.0),
                    percent_base_height: Some(240.0),
                },
                arena,
            );
        });
        settle_plain_text_area(arena, root);
    }

    fn assert_text_area_fallback_before_full(
        arena: &NodeArena,
        roots: &[NodeKey],
    ) -> FrameArtifactEligibility {
        let (properties, generations) = sync_identity(arena, roots);
        take_full_artifact_record_count();
        let outcome =
            record_frame_artifact(arena, roots, &properties, &generations, RendererMode::Auto)
                .unwrap();
        let FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility) = outcome else {
            panic!("unsafe TextArea state must fail metadata preflight")
        };
        assert_eq!(take_full_artifact_record_count(), 0);
        eligibility
    }

    #[derive(Clone, Copy)]
    enum InlineOwnedTextDamage {
        None,
        MissingGlyphs,
        MissingFont,
    }

    fn prepared_inline_owned_text_tree(
        damage: InlineOwnedTextDamage,
    ) -> (NodeArena, Vec<NodeKey>, NodeKey) {
        let (arena, roots, text_key) = prepared_text_tree(false);
        let mut paint_input = {
            let node = arena.get(text_key).unwrap();
            let text = node.element.as_any().downcast_ref::<Text>().unwrap();
            text.shaped_context_for_test()
                .unwrap()
                .text_pass_paint_input()
        };
        match damage {
            InlineOwnedTextDamage::None => {}
            InlineOwnedTextDamage::MissingGlyphs => {
                paint_input.glyphs.clear();
                paint_input.batches.clear();
            }
            InlineOwnedTextDamage::MissingFont => {
                paint_input
                    .glyphs
                    .first_mut()
                    .expect("inline text fixture must contain a glyph")
                    .font_data = None;
            }
        }
        arena
            .get_mut(text_key)
            .unwrap()
            .element
            .as_any_mut()
            .downcast_mut::<Text>()
            .unwrap()
            .install_inline_ifc_owned_geometry(
                Vec::new(),
                Arc::new(paint_input),
                crate::ui::Rect {
                    x: 3.4,
                    y: 5.6,
                    width: 92.0,
                    height: 72.0,
                },
            );
        (arena, roots, text_key)
    }

    fn prepared_wrapping_inline_span_tree() -> (NodeArena, Vec<NodeKey>, NodeKey, NodeKey, usize) {
        prepared_wrapping_inline_span_tree_with_opacity(1.0)
    }

    fn wrapping_inline_span_constraints() -> (LayoutConstraints, LayoutPlacement) {
        let (mut measure, mut place) = constraints();
        measure.max_width = 92.0;
        measure.max_height = 220.0;
        place.available_width = 92.0;
        place.available_height = 220.0;
        (measure, place)
    }

    fn settle_wrapping_inline_span_frame(
        arena: &NodeArena,
        parent_key: NodeKey,
        span_key: NodeKey,
        text_key: NodeKey,
    ) {
        for key in [parent_key, span_key, text_key] {
            arena
                .get_mut(key)
                .unwrap()
                .element
                .clear_local_dirty_flags(DirtyFlags::ALL);
        }
        arena.clear_arena_dirty_subtree(parent_key, DirtyFlags::ALL);
    }

    fn prepared_wrapping_inline_span_tree_with_opacity(
        opacity: f32,
    ) -> (NodeArena, Vec<NodeKey>, NodeKey, NodeKey, usize) {
        let mut arena = new_test_arena();
        let mut parent = Element::new_with_id(0x7b00, 0.0, 0.0, 92.0, 0.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(92.0)));
        parent_style.insert(PropertyId::Height, ParsedValue::Auto);
        parent.apply_style(parent_style);
        let parent_key = commit_element(&mut arena, Box::new(parent));

        let mut span = Element::new_with_id(0x7b01, 0.0, 0.0, 0.0, 0.0);
        let mut span_style = Style::new();
        span_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        span_style.insert(PropertyId::Width, ParsedValue::Auto);
        span_style.insert(PropertyId::Height, ParsedValue::Auto);
        span_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#bfdbfe")),
        );
        span_style.set_border(Border::uniform(Length::px(2.0), &Color::hex("#2563eb")));
        span_style.set_padding(crate::style::Padding::uniform(Length::px(4.0)));
        span.apply_style(span_style);
        span.set_border_radius(5.0);
        span.set_opacity(opacity);
        let span_key = commit_child(&mut arena, parent_key, Box::new(span));
        let text_key = commit_child(
            &mut arena,
            span_key,
            Box::new(Text::new_with_id(
                0x7b02,
                0.0,
                0.0,
                0.0,
                0.0,
                "alpha beta gamma delta epsilon zeta",
            )),
        );

        let (measure, place) = wrapping_inline_span_constraints();
        measure_and_place(&mut arena, parent_key, measure, place);
        let fragment_count = arena
            .get(span_key)
            .unwrap()
            .element
            .as_any()
            .downcast_ref::<Element>()
            .unwrap()
            .inline_fragment_rects()
            .len();
        assert!(fragment_count >= 2, "M7B fixture must wrap the source span");
        (arena, vec![span_key], span_key, text_key, fragment_count)
    }

    fn prepared_owning_wrapping_inline_span_tree_with_opacity(
        opacity: f32,
    ) -> (NodeArena, Vec<NodeKey>, NodeKey, NodeKey, NodeKey, usize) {
        let mut arena = new_test_arena();
        let mut root = Element::new_with_id(0x7c00, 0.0, 0.0, 92.0, 0.0);
        let mut root_style = Style::new();
        root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        root_style.insert(PropertyId::Width, ParsedValue::Auto);
        root_style.insert(PropertyId::Height, ParsedValue::Auto);
        root_style.set_padding(crate::style::Padding::uniform(Length::px(8.0)));
        root_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#f8fafc")),
        );
        root.apply_style(root_style);
        root.set_opacity(opacity);
        let root_key = commit_element(&mut arena, Box::new(root));

        let mut span = Element::new_with_id(0x7c01, 0.0, 0.0, 0.0, 0.0);
        let mut span_style = Style::new();
        span_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        span_style.insert(PropertyId::Width, ParsedValue::Auto);
        span_style.insert(PropertyId::Height, ParsedValue::Auto);
        span_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#fde68a")),
        );
        span.apply_style(span_style);
        let span_key = commit_child(&mut arena, root_key, Box::new(span));
        let text_key = commit_child(
            &mut arena,
            span_key,
            Box::new(Text::new_with_id(
                0x7c02,
                0.0,
                0.0,
                0.0,
                0.0,
                "alpha beta gamma delta epsilon zeta",
            )),
        );
        let (measure, place) = wrapping_inline_span_constraints();
        measure_and_place(&mut arena, root_key, measure, place);
        settle_wrapping_inline_span_frame(&arena, root_key, span_key, text_key);
        let fragment_count = arena
            .get(span_key)
            .unwrap()
            .element
            .as_any()
            .downcast_ref::<Element>()
            .unwrap()
            .inline_fragment_rects()
            .len();
        assert!(fragment_count >= 2);
        (
            arena,
            vec![root_key],
            root_key,
            span_key,
            text_key,
            fragment_count,
        )
    }

    fn prepared_owning_inline_text_root() -> (NodeArena, Vec<NodeKey>, NodeKey, NodeKey) {
        let mut arena = new_test_arena();
        let mut root = Element::new_with_id(0x7a10, 0.0, 0.0, 120.0, 40.0);
        let mut style = Style::new();
        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        style.insert(PropertyId::Width, ParsedValue::Auto);
        style.insert(PropertyId::Height, ParsedValue::Auto);
        root.apply_style(style);
        let root = commit_element(&mut arena, Box::new(root));
        let text = commit_child(
            &mut arena,
            root,
            Box::new(Text::new_with_id(
                0x7a11,
                0.0,
                0.0,
                100.0,
                30.0,
                "inline child",
            )),
        );
        let (measure, place) = constraints();
        measure_and_place(&mut arena, root, measure, place);
        for key in [root, text] {
            arena
                .get_mut(key)
                .unwrap()
                .element
                .clear_local_dirty_flags(DirtyFlags::ALL);
        }
        arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
        (arena, vec![root], root, text)
    }

    fn prepared_owning_inline_two_text_root() -> (NodeArena, Vec<NodeKey>, NodeKey) {
        let mut arena = new_test_arena();
        let mut root = Element::new_with_id(0x7a18, 0.0, 0.0, 160.0, 40.0);
        let mut style = Style::new();
        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        style.insert(PropertyId::Width, ParsedValue::Auto);
        style.insert(PropertyId::Height, ParsedValue::Auto);
        root.apply_style(style);
        let root = commit_element(&mut arena, Box::new(root));
        let first = commit_child(
            &mut arena,
            root,
            Box::new(Text::new_with_id(
                0x7a19,
                0.0,
                0.0,
                0.0,
                0.0,
                "first payload ",
            )),
        );
        let second = commit_child(
            &mut arena,
            root,
            Box::new(Text::new_with_id(
                0x7a1a,
                0.0,
                0.0,
                0.0,
                0.0,
                "second payload is different",
            )),
        );
        let (measure, place) = constraints();
        measure_and_place(&mut arena, root, measure, place);
        for key in [root, first, second] {
            arena
                .get_mut(key)
                .unwrap()
                .element
                .clear_local_dirty_flags(DirtyFlags::ALL);
        }
        arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
        (arena, vec![root], root)
    }

    fn prepared_fixed_owning_inline_text_root() -> (NodeArena, Vec<NodeKey>, NodeKey, NodeKey) {
        let mut arena = new_test_arena();
        let mut root = Element::new_with_id(0x7c10, 0.0, 0.0, 120.0, 40.0);
        let mut style = Style::new();
        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        style.insert(PropertyId::Width, ParsedValue::Length(Length::px(120.0)));
        style.insert(PropertyId::Height, ParsedValue::Length(Length::px(40.0)));
        root.apply_style(style);
        let root = commit_element(&mut arena, Box::new(root));
        let text = commit_child(
            &mut arena,
            root,
            Box::new(Text::new_with_id(
                0x7c11,
                0.0,
                0.0,
                100.0,
                30.0,
                "fixed inline child",
            )),
        );
        let (measure, place) = constraints();
        measure_and_place(&mut arena, root, measure, place);
        for key in [root, text] {
            arena
                .get_mut(key)
                .unwrap()
                .element
                .clear_local_dirty_flags(DirtyFlags::ALL);
        }
        arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
        (arena, vec![root], root, text)
    }

    fn prepared_percent_owning_inline_text_root() -> (NodeArena, Vec<NodeKey>, NodeKey, NodeKey) {
        let mut arena = new_test_arena();
        let mut root = Element::new_with_id(0x7c18, 0.0, 0.0, 160.0, 200.0);
        let mut style = Style::new();
        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        style.insert(
            PropertyId::Width,
            ParsedValue::Length(Length::percent(50.0)),
        );
        style.insert(PropertyId::Height, ParsedValue::Length(Length::px(200.0)));
        root.apply_style(style);
        let root = commit_element(&mut arena, Box::new(root));
        let text = commit_child(
            &mut arena,
            root,
            Box::new(Text::new_with_id(
                0x7c19,
                0.0,
                0.0,
                0.0,
                0.0,
                "alpha beta gamma delta epsilon zeta eta theta iota kappa",
            )),
        );
        let (measure, place) = constraints();
        measure_and_place(&mut arena, root, measure, place);
        for key in [root, text] {
            arena
                .get_mut(key)
                .unwrap()
                .element
                .clear_local_dirty_flags(DirtyFlags::ALL);
        }
        arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
        (arena, vec![root], root, text)
    }

    fn prepared_owning_inline_root_with_atomic()
    -> (NodeArena, Vec<NodeKey>, NodeKey, NodeKey, NodeKey, NodeKey) {
        let mut arena = new_test_arena();
        let mut root = Element::new_with_id(0x7c20, 0.0, 0.0, 160.0, 40.0);
        let mut root_style = Style::new();
        root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        root_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(160.0)));
        root_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(40.0)));
        root.apply_style(root_style);
        let root = commit_element(&mut arena, Box::new(root));
        let before = commit_child(
            &mut arena,
            root,
            Box::new(Text::new_with_id(0x7c21, 0.0, 0.0, 0.0, 0.0, "before ")),
        );
        let mut atomic = Element::new_with_id(0x7c22, 0.0, 0.0, 24.0, 18.0);
        atomic.set_background_color_value(Color::rgb(34, 197, 94));
        let atomic = commit_child(&mut arena, root, Box::new(atomic));
        let after = commit_child(
            &mut arena,
            root,
            Box::new(Text::new_with_id(0x7c23, 0.0, 0.0, 0.0, 0.0, " after")),
        );
        let (measure, place) = constraints();
        measure_and_place(&mut arena, root, measure, place);
        for key in [root, before, atomic, after] {
            arena
                .get_mut(key)
                .unwrap()
                .element
                .clear_local_dirty_flags(DirtyFlags::ALL);
        }
        arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
        (arena, vec![root], root, before, atomic, after)
    }

    fn prepared_owning_inline_root_with_two_atomics() -> (NodeArena, Vec<NodeKey>, NodeKey) {
        let mut arena = new_test_arena();
        let mut root = Element::new_with_id(0x7c28, 0.0, 0.0, 160.0, 40.0);
        let mut root_style = Style::new();
        root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        root_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(160.0)));
        root_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(40.0)));
        root.apply_style(root_style);
        let root = commit_element(&mut arena, Box::new(root));
        let before = commit_child(
            &mut arena,
            root,
            Box::new(Text::new_with_id(0x7c29, 0.0, 0.0, 0.0, 0.0, "before ")),
        );
        let mut first = Element::new_with_id(0x7c2a, 0.0, 0.0, 20.0, 16.0);
        first.set_background_color_value(Color::rgb(34, 197, 94));
        let first = commit_child(&mut arena, root, Box::new(first));
        let mut second = Element::new_with_id(0x7c2b, 0.0, 0.0, 18.0, 14.0);
        second.set_background_color_value(Color::rgb(59, 130, 246));
        let second = commit_child(&mut arena, root, Box::new(second));
        let after = commit_child(
            &mut arena,
            root,
            Box::new(Text::new_with_id(0x7c2c, 0.0, 0.0, 0.0, 0.0, " after")),
        );
        let (measure, place) = constraints();
        measure_and_place(&mut arena, root, measure, place);
        for key in [root, before, first, second, after] {
            arena
                .get_mut(key)
                .unwrap()
                .element
                .clear_local_dirty_flags(DirtyFlags::ALL);
        }
        arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
        (arena, vec![root], root)
    }

    #[allow(clippy::type_complexity)]
    fn prepared_owning_inline_root_with_atomic_subtree() -> (
        NodeArena,
        Vec<NodeKey>,
        NodeKey,
        NodeKey,
        NodeKey,
        NodeKey,
        NodeKey,
    ) {
        let mut arena = new_test_arena();
        let mut root = Element::new_with_id(0x7c60, 0.0, 0.0, 160.0, 40.0);
        let mut root_style = Style::new();
        root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        root_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(160.0)));
        root_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(40.0)));
        root.apply_style(root_style);
        let root = commit_element(&mut arena, Box::new(root));
        let before = commit_child(
            &mut arena,
            root,
            Box::new(Text::new_with_id(0x7c61, 0.0, 0.0, 0.0, 0.0, "before ")),
        );
        let mut atomic = Element::new_with_id(0x7c62, 0.0, 0.0, 24.0, 18.0);
        let mut atomic_style = Style::new();
        atomic_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        atomic_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(24.0)));
        atomic_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(18.0)));
        atomic_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::rgb(34, 197, 94)),
        );
        atomic.apply_style(atomic_style);
        let atomic = commit_child(&mut arena, root, Box::new(atomic));
        let mut grandchild = Element::new_with_id(0x7c63, 0.0, 0.0, 8.0, 8.0);
        grandchild.set_background_color_value(Color::rgb(59, 130, 246));
        let grandchild = commit_child(&mut arena, atomic, Box::new(grandchild));
        let after = commit_child(
            &mut arena,
            root,
            Box::new(Text::new_with_id(0x7c64, 0.0, 0.0, 0.0, 0.0, " after")),
        );
        let (measure, place) = constraints();
        measure_and_place(&mut arena, root, measure, place);
        for key in [root, before, atomic, grandchild, after] {
            arena
                .get_mut(key)
                .unwrap()
                .element
                .clear_local_dirty_flags(DirtyFlags::ALL);
        }
        arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
        (arena, vec![root], root, before, atomic, grandchild, after)
    }

    fn prepared_owning_inline_root_with_image_atomic()
    -> (NodeArena, Vec<NodeKey>, NodeKey, NodeKey, NodeKey, NodeKey) {
        let mut arena = new_test_arena();
        let mut root = Element::new_with_id(0x7c40, 0.0, 0.0, 120.0, 0.0);
        let mut root_style = Style::new();
        root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        root_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(120.0)));
        root_style.insert(PropertyId::Height, ParsedValue::Auto);
        root.apply_style(root_style);
        let root = commit_element(&mut arena, Box::new(root));
        let before = commit_child(
            &mut arena,
            root,
            Box::new(Text::new_with_id(0x7c41, 0.0, 0.0, 0.0, 0.0, "image ")),
        );
        let mut image = Image::new_with_id(
            0x7c42,
            ImageSource::Rgba {
                width: 1,
                height: 1,
                pixels: Arc::from([255, 0, 0, 255]),
            },
        );
        let mut image_style = Style::new();
        image_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        image_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(20.0)));
        image_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(16.0)));
        image.apply_style(image_style);
        let image = commit_child(&mut arena, root, Box::new(image));
        let after = commit_child(
            &mut arena,
            root,
            Box::new(Text::new_with_id(0x7c43, 0.0, 0.0, 0.0, 0.0, " tail")),
        );
        let (measure, place) = constraints();
        measure_and_place(&mut arena, root, measure, place);
        for key in [root, before, image, after] {
            arena
                .get_mut(key)
                .unwrap()
                .element
                .clear_local_dirty_flags(DirtyFlags::ALL);
        }
        arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
        (arena, vec![root], root, before, image, after)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn prepared_owning_inline_root_with_svg_atomic()
    -> (NodeArena, Vec<NodeKey>, NodeKey, NodeKey, NodeKey, NodeKey) {
        const SVG: &str = "<svg xmlns='http://www.w3.org/2000/svg' width='20' height='16'><rect width='20' height='16' fill='#16a34a'/></svg>";
        let mut arena = new_test_arena();
        let mut root = Element::new_with_id(0x7c50, 0.0, 0.0, 120.0, 0.0);
        let mut root_style = Style::new();
        root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        root_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(120.0)));
        root_style.insert(PropertyId::Height, ParsedValue::Auto);
        root.apply_style(root_style);
        let root = commit_element(&mut arena, Box::new(root));
        let before = commit_child(
            &mut arena,
            root,
            Box::new(Text::new_with_id(0x7c51, 0.0, 0.0, 0.0, 0.0, "svg ")),
        );
        let mut svg = Svg::new_with_id(0x7c52, SvgSource::Content(SVG.into()));
        let mut svg_style = Style::new();
        svg_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        svg_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(20.0)));
        svg_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(16.0)));
        svg.apply_style(svg_style);
        let svg = commit_child(&mut arena, root, Box::new(svg));
        let after = commit_child(
            &mut arena,
            root,
            Box::new(Text::new_with_id(0x7c53, 0.0, 0.0, 0.0, 0.0, " tail")),
        );
        let (measure, place) = constraints();
        measure_and_place(&mut arena, root, measure, place);
        {
            let mut node = arena.get_mut(svg).unwrap();
            let svg_host = node.element.as_any_mut().downcast_mut::<Svg>().unwrap();
            svg_host
                .prepare_content_paint_for_test(SVG, (20.0, 16.0), 1.0)
                .unwrap();
            svg_host.clear_local_dirty_flags(DirtyFlags::ALL);
        }
        arena.set_children(svg, Vec::new());
        for key in [root, before, svg, after] {
            arena
                .get_mut(key)
                .unwrap()
                .element
                .clear_local_dirty_flags(DirtyFlags::ALL);
        }
        arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
        (arena, vec![root], root, before, svg, after)
    }

    #[allow(clippy::type_complexity)]
    fn prepared_mixed_wrapping_inline_root() -> (
        NodeArena,
        Vec<NodeKey>,
        NodeKey,
        NodeKey,
        NodeKey,
        NodeKey,
        NodeKey,
        NodeKey,
        usize,
    ) {
        let mut arena = new_test_arena();
        let mut root = Element::new_with_id(0x7c30, 0.0, 0.0, 108.0, 0.0);
        let mut root_style = Style::new();
        root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        root_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(108.0)));
        root_style.insert(PropertyId::Height, ParsedValue::Auto);
        root.apply_style(root_style);
        let root = commit_element(&mut arena, Box::new(root));

        // Allocate in an order deliberately different from the live DOM.
        // Neither NodeKey order nor install-plan order may become paint order.
        let mut atomic = Element::new_with_id(0x7c31, 0.0, 0.0, 22.0, 17.0);
        atomic.set_background_color_value(Color::rgb(34, 197, 94));
        let atomic = commit_child(&mut arena, root, Box::new(atomic));
        let after = commit_child(
            &mut arena,
            root,
            Box::new(Text::new_with_id(0x7c32, 0.0, 0.0, 0.0, 0.0, " tail")),
        );
        let before = commit_child(
            &mut arena,
            root,
            Box::new(Text::new_with_id(0x7c33, 0.0, 0.0, 0.0, 0.0, "head ")),
        );
        let mut span = Element::new_with_id(0x7c34, 0.0, 0.0, 0.0, 0.0);
        let mut span_style = Style::new();
        span_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        span_style.insert(PropertyId::Width, ParsedValue::Auto);
        span_style.insert(PropertyId::Height, ParsedValue::Auto);
        span_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#fde68a")),
        );
        span.apply_style(span_style);
        let span = commit_child(&mut arena, root, Box::new(span));
        let nested_text = commit_child(
            &mut arena,
            span,
            Box::new(Text::new_with_id(
                0x7c35,
                0.0,
                0.0,
                0.0,
                0.0,
                "alpha beta gamma delta epsilon",
            )),
        );
        arena.set_children(root, vec![before, span, atomic, after]);

        let (measure, place) = constraints();
        measure_and_place(&mut arena, root, measure, place);
        let fragment_count = arena
            .get(span)
            .unwrap()
            .element
            .as_any()
            .downcast_ref::<Element>()
            .unwrap()
            .inline_fragment_rects()
            .len();
        for key in [root, before, span, nested_text, atomic, after] {
            arena
                .get_mut(key)
                .unwrap()
                .element
                .clear_local_dirty_flags(DirtyFlags::ALL);
        }
        arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
        (
            arena,
            vec![root],
            root,
            before,
            span,
            nested_text,
            atomic,
            after,
            fragment_count,
        )
    }

    fn prepared_nested_inline_span_tree() -> (NodeArena, Vec<NodeKey>, Vec<NodeKey>) {
        let mut arena = new_test_arena();
        let mut parent = Element::new_with_id(0x7b10, 0.0, 0.0, 108.0, 0.0);
        let mut parent_style = Style::new();
        parent_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(108.0)));
        parent_style.insert(PropertyId::Height, ParsedValue::Auto);
        parent.apply_style(parent_style);
        let parent_key = commit_element(&mut arena, Box::new(parent));

        let mut outer = Element::new_with_id(0x7b11, 0.0, 0.0, 0.0, 0.0);
        let mut outer_style = Style::new();
        outer_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        outer_style.insert(PropertyId::Width, ParsedValue::Auto);
        outer_style.insert(PropertyId::Height, ParsedValue::Auto);
        outer_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#dbeafe")),
        );
        outer_style.set_padding(crate::style::Padding::uniform(Length::px(3.0)));
        outer.apply_style(outer_style);
        let outer_key = commit_child(&mut arena, parent_key, Box::new(outer));
        let before_key = commit_child(
            &mut arena,
            outer_key,
            Box::new(Text::new_with_id(
                0x7b12,
                0.0,
                0.0,
                0.0,
                0.0,
                "outer alpha ",
            )),
        );

        let mut inner = Element::new_with_id(0x7b13, 0.0, 0.0, 0.0, 0.0);
        let mut inner_style = Style::new();
        inner_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        inner_style.insert(PropertyId::Width, ParsedValue::Auto);
        inner_style.insert(PropertyId::Height, ParsedValue::Auto);
        inner_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#fecaca")),
        );
        inner_style.set_border(Border::uniform(Length::px(1.0), &Color::hex("#dc2626")));
        inner_style.set_padding(crate::style::Padding::uniform(Length::px(2.0)));
        inner.apply_style(inner_style);
        let inner_key = commit_child(&mut arena, outer_key, Box::new(inner));
        let inner_text_key = commit_child(
            &mut arena,
            inner_key,
            Box::new(Text::new_with_id(
                0x7b14,
                0.0,
                0.0,
                0.0,
                0.0,
                "inner beta gamma",
            )),
        );
        let after_key = commit_child(
            &mut arena,
            outer_key,
            Box::new(Text::new_with_id(
                0x7b15,
                0.0,
                0.0,
                0.0,
                0.0,
                " tail delta epsilon",
            )),
        );
        let (mut measure, mut place) = constraints();
        measure.max_width = 108.0;
        place.available_width = 108.0;
        measure_and_place(&mut arena, parent_key, measure, place);
        assert!(
            arena
                .get(outer_key)
                .unwrap()
                .element
                .as_any()
                .downcast_ref::<Element>()
                .unwrap()
                .inline_fragment_rects()
                .len()
                >= 2
        );
        (
            arena,
            vec![outer_key],
            vec![outer_key, before_key, inner_key, inner_text_key, after_key],
        )
    }

    fn first_text_color_bits(artifact: &PaintArtifact) -> [u32; 4] {
        artifact
            .ops
            .iter()
            .find_map(|op| match op {
                PaintOp::PreparedText(op) => op
                    .params
                    .staging_input
                    .glyphs
                    .first()
                    .map(|glyph| glyph.paint.color.map(f32::to_bits)),
                PaintOp::DrawRect(_) => None,
                PaintOp::PreparedInlineIfcDecoration(_)
                | PaintOp::PreparedShadow(_)
                | PaintOp::PreparedScrollbarOverlay(_)
                | PaintOp::PreparedImage(_)
                | PaintOp::PreparedSvg(_) => None,
            })
            .expect("fixture must retain at least one prepared glyph")
    }

    #[test]
    fn standalone_text_root_and_nested_fractional_offset_match_legacy_strictly() {
        for nested in [false, true] {
            let rects = assert_whole_frame_structural_parity(
                || {
                    let (arena, roots, _) = prepared_text_tree(nested);
                    (arena, roots)
                },
                PaintParityConfig {
                    width: 640,
                    height: 480,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    scale_factor: 2.0,
                    initial_scissor: None,
                },
            );
            assert!(rects.is_empty(), "text fixture should not emit rect passes");
        }
    }

    #[test]
    fn prepared_text_artifact_compiles_after_arena_is_dropped() {
        let artifact = {
            let (arena, roots, _) = prepared_text_tree(true);
            let (properties, generations) = sync_identity(&arena, &roots);
            whole_frame_artifact(&arena, &roots, &properties, &generations).0
        };
        assert!(
            artifact
                .ops
                .iter()
                .any(|op| matches!(op, PaintOp::PreparedText(_)))
        );
        let mut graph = compiled_whole_frame_graph(&artifact);
        graph
            .test_compile_snapshot()
            .expect("retained text params must compile without the arena");
    }

    #[test]
    fn text_color_only_change_reuses_shaping_and_changes_retained_payload() {
        let (arena, roots, text_key) = prepared_text_tree(false);
        let mut properties = PropertyTrees::default();
        properties.sync(&arena, &roots);
        let mut generations = PaintGenerationTracker::default();
        generations.sync(&arena, &roots, &properties);
        let before_context = arena
            .get(text_key)
            .unwrap()
            .element
            .as_any()
            .downcast_ref::<Text>()
            .unwrap()
            .shaped_context_for_test()
            .unwrap()
            .clone();
        let before = whole_frame_artifact(&arena, &roots, &properties, &generations).0;

        arena
            .get_mut(text_key)
            .unwrap()
            .element
            .as_any_mut()
            .downcast_mut::<Text>()
            .unwrap()
            .set_color(Color::rgb(230, 40, 70));
        properties.sync(&arena, &roots);
        generations.sync(&arena, &roots, &properties);
        let after_context = arena
            .get(text_key)
            .unwrap()
            .element
            .as_any()
            .downcast_ref::<Text>()
            .unwrap()
            .shaped_context_for_test()
            .unwrap()
            .clone();
        let after = whole_frame_artifact(&arena, &roots, &properties, &generations).0;

        assert!(Arc::ptr_eq(&before_context, &after_context));
        assert_ne!(
            first_text_color_bits(&before),
            first_text_color_bits(&after)
        );
    }

    #[test]
    fn unchanged_text_records_identical_strict_snapshots_across_frames() {
        let (arena, roots, _) = prepared_text_tree(true);
        let (properties, generations) = sync_identity(&arena, &roots);
        let first = whole_frame_artifact(&arena, &roots, &properties, &generations).0;
        let second = whole_frame_artifact(&arena, &roots, &properties, &generations).0;
        let mut first_graph = compiled_whole_frame_graph(&first);
        let mut second_graph = compiled_whole_frame_graph(&second);
        assert_eq!(
            first_graph.test_compile_snapshot().unwrap(),
            second_graph.test_compile_snapshot().unwrap()
        );
    }

    #[test]
    fn hidden_empty_and_zero_opacity_text_are_transparent_without_chunks() {
        for kind in 0..3 {
            let mut arena = new_test_arena();
            let mut text = Text::new_with_id(
                181 + kind,
                0.0,
                0.0,
                if kind == 1 { 0.0 } else { 80.0 },
                30.0,
                if kind == 0 { "" } else { "text" },
            );
            if kind == 2 {
                text.set_opacity(0.0);
            }
            let root = commit_element(&mut arena, Box::new(text));
            let (measure, place) = constraints();
            measure_and_place(&mut arena, root, measure, place);
            let roots = [root];
            let (properties, generations) = sync_identity(&arena, &roots);
            let manifest = |mode| {
                record_coverage_manifest(
                    &arena,
                    &roots,
                    false,
                    true,
                    mode,
                    &properties,
                    &generations,
                )
            };
            let metadata = manifest(CoverageRecordingMode::MetadataOnly);
            let full = manifest(CoverageRecordingMode::FullArtifact);
            assert!(matches!(
                metadata.items.as_slice(),
                [PaintCoverageItem::TransparentNode { owner, .. }] if *owner == root
            ));
            assert!(canonical_manifest_matches_for_test(&metadata, &full));

            let (artifact, eligibility) =
                whole_frame_artifact(&arena, &roots, &properties, &generations);
            assert!(eligibility.eligible);
            assert_eq!(eligibility.chunk_count, 0);
            assert_eq!(eligibility.op_count, 0);
            assert!(artifact.chunks.is_empty());
            assert!(artifact.ops.is_empty());
        }
    }

    fn prepared_plain_tree() -> (NodeArena, Vec<NodeKey>, NodeKey) {
        let mut arena = new_test_arena();
        let first = commit_element(
            &mut arena,
            Box::new(leaf_element(100, Color::rgb(230, 20, 30), 1.0, false)),
        );
        let mut child_element = leaf_element(101, Color::rgb(20, 210, 40), 1.0, false);
        child_element.set_position(0.0, 0.0);
        let child = commit_child(&mut arena, first, Box::new(child_element));
        let second = commit_element(
            &mut arena,
            Box::new(leaf_element(102, Color::rgb(30, 40, 220), 1.0, false)),
        );
        let (measure, place) = constraints();
        measure_and_place(&mut arena, first, measure, place);
        measure_and_place(&mut arena, second, measure, place);
        (arena, vec![first, second], child)
    }

    fn prepared_asymmetric_border_tree() -> (NodeArena, Vec<NodeKey>) {
        let mut element = Element::new_with_id(103, 10.25, 20.75, 80.0, 40.0);
        let top = Color::hex("#ff0000");
        let right = Color::hex("#00ff00");
        let bottom = Color::hex("#0000ff");
        let left = Color::hex("#ffff00");
        let mut style = Style::new();
        style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::hex("#203040")),
        );
        style.set_border(
            Border::uniform(Length::px(1.0), &Color::hex("#ffffff"))
                .top(Some(Length::px(2.0)), Some(&top))
                .right(Some(Length::px(3.0)), Some(&right))
                .bottom(Some(Length::px(4.0)), Some(&bottom))
                .left(Some(Length::px(5.0)), Some(&left)),
        );
        style.set_border_radius(
            BorderRadius::uniform(Length::px(2.0))
                .top_right(Length::px(6.0))
                .bottom_right(Length::px(10.0))
                .bottom_left(Length::px(14.0)),
        );
        element.apply_style(style);

        let mut arena = new_test_arena();
        let root = commit_element(&mut arena, Box::new(element));
        let (measure, place) = constraints();
        measure_and_place(&mut arena, root, measure, place);
        (arena, vec![root])
    }

    fn prepared_gradient_tree() -> (NodeArena, Vec<NodeKey>) {
        let mut element = Element::new_with_id(104, 10.25, 20.75, 80.0, 40.0);
        apply_gradient_style(&mut element, "#ff0000", "#0000ff", "#ffffff", "#000000");
        let mut arena = new_test_arena();
        let root = commit_element(&mut arena, Box::new(element));
        let (measure, place) = constraints();
        measure_and_place(&mut arena, root, measure, place);
        (arena, vec![root])
    }

    fn prepared_zero_opacity_tree() -> (NodeArena, Vec<NodeKey>) {
        let mut arena = new_test_arena();
        let mut empty_element = leaf_element(110, Color::rgb(255, 0, 0), 1.0, false);
        let mut empty_style = Style::new();
        empty_style.insert(PropertyId::Opacity, ParsedValue::Opacity(Opacity::new(0.0)));
        empty_element.apply_style(empty_style);
        let empty = commit_element(&mut arena, Box::new(empty_element));
        let visible = commit_element(
            &mut arena,
            Box::new(leaf_element(111, Color::rgb(0, 0, 255), 1.0, false)),
        );
        let (measure, place) = constraints();
        measure_and_place(&mut arena, empty, measure, place);
        measure_and_place(&mut arena, visible, measure, place);
        (arena, vec![empty, visible])
    }

    #[test]
    fn strict_structural_parity_covers_opaque_alpha_and_uniform_border() {
        for (opacity, expected_opaque) in [(1.0, true), (0.5, false)] {
            let snapshots = assert_whole_frame_structural_parity(
                || {
                    let (arena, root, _, _) =
                        prepared_leaf(105, Color::rgb(220, 30, 40), opacity, true);
                    (arena, vec![root])
                },
                PaintParityConfig::default(),
            );
            assert_eq!(snapshots.len(), 2, "fill and border must both be captured");
            assert_eq!(snapshots[0].opaque, expected_opaque);
        }
    }

    #[test]
    fn strict_structural_parity_covers_asymmetric_border_radius_and_colors() {
        let snapshots = assert_whole_frame_structural_parity(
            prepared_asymmetric_border_tree,
            PaintParityConfig::default(),
        );
        assert_eq!(snapshots.len(), 2);
        assert!(snapshots[1].use_border_side_colors);
        assert_eq!(
            snapshots[1].border_width_bits,
            [5.0_f32, 3.0, 2.0, 4.0].map(f32::to_bits)
        );
        assert_eq!(
            snapshots[1].border_radius_bits,
            [[2.0_f32, 2.0], [6.0, 6.0], [10.0, 10.0], [14.0, 14.0]]
                .map(|radius| radius.map(f32::to_bits))
        );
    }

    #[test]
    fn strict_structural_parity_covers_background_and_border_gradients() {
        let snapshots = assert_whole_frame_structural_parity(
            prepared_gradient_tree,
            PaintParityConfig::default(),
        );
        assert_eq!(snapshots.len(), 2);
        assert!(snapshots[0].gradient.is_some());
        assert!(snapshots[1].border_gradient.is_some());
    }

    #[test]
    fn strict_structural_parity_covers_nested_multi_root_order() {
        let snapshots = assert_whole_frame_structural_parity(
            || {
                let (arena, roots, _) = prepared_plain_tree();
                (arena, roots)
            },
            PaintParityConfig::default(),
        );
        assert_eq!(snapshots.len(), 3);
        assert!(
            f32::from_bits(snapshots[0].fill_color_bits[0])
                > f32::from_bits(snapshots[0].fill_color_bits[1])
        );
        assert!(
            f32::from_bits(snapshots[1].fill_color_bits[1])
                > f32::from_bits(snapshots[1].fill_color_bits[0])
        );
        assert!(
            f32::from_bits(snapshots[2].fill_color_bits[2])
                > f32::from_bits(snapshots[2].fill_color_bits[0])
        );
    }

    #[test]
    fn anchor_parent_leaf_self_clip_replaces_then_restores_ancestor_scissor_strictly() {
        for (format, scale_factor) in [
            (wgpu::TextureFormat::Bgra8Unorm, 1.0),
            (wgpu::TextureFormat::Rgba8Unorm, 2.0),
        ] {
            for (opacity, border) in [(1.0, false), (1.0, true), (0.55, false), (0.55, true)] {
                let config = PaintParityConfig {
                    format,
                    scale_factor,
                    initial_scissor: Some([4, 6, 24, 18]),
                    ..PaintParityConfig::default()
                };
                let snapshots = assert_whole_frame_structural_parity(
                    || anchor_parent_self_clip_roots(opacity, border),
                    config,
                );
                let clipped_op_count = if border { 2 } else { 1 };
                assert_eq!(snapshots.len(), clipped_op_count + 1);
                for snapshot in &snapshots[..clipped_op_count] {
                    assert_eq!(snapshot.effective_scissor_rect, Some([0, 0, 320, 240]));
                }
                assert_eq!(
                    snapshots[clipped_op_count].effective_scissor_rect,
                    Some([4, 6, 24, 18]),
                    "the following root must observe the restored ancestor scissor"
                );
                assert_eq!(snapshots[0].opaque, opacity == 1.0);
            }
        }
    }

    #[test]
    fn exact_single_owner_self_clip_keeps_outer_shadow_outside_owner_clip() {
        let (arena, roots) = anchor_parent_self_clip_shadow_root();
        let (properties, generations) = sync_identity(&arena, &roots);
        let (artifact, eligibility) =
            whole_frame_artifact(&arena, &roots, &properties, &generations);
        assert!(eligibility.eligible);
        assert_eq!(artifact.chunks.len(), 1);
        assert!(matches!(
            artifact.ops.first(),
            Some(PaintOp::PreparedShadow(_))
        ));
        assert!(matches!(
            artifact.chunks[0].payload_identity,
            PaintPayloadIdentity::PreparedShadows(_, _)
        ));

        let incoming = [4, 6, 24, 18];
        let mut graph = compiled_whole_frame_graph_with_config(
            &artifact,
            PaintParityConfig {
                initial_scissor: Some(incoming),
                ..PaintParityConfig::default()
            },
        );
        let snapshot = graph.test_compile_snapshot().unwrap();
        let shadow_composite = snapshot
            .pass_payloads()
            .iter()
            .find_map(|payload| match payload {
                FramePassTestPayload::TextureComposite(composite)
                    if composite.sampled_source.is_none() =>
                {
                    Some(composite)
                }
                _ => None,
            })
            .expect("outer shadow must composite before decoration");
        assert_eq!(shadow_composite.pass_context.scissor_rect, Some(incoming));
        let rects = snapshot
            .pass_payloads()
            .iter()
            .filter_map(|payload| match payload {
                FramePassTestPayload::DrawRect(rect) => Some(rect),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(rects.len(), 2);
        assert!(
            rects
                .iter()
                .all(|rect| rect.effective_scissor_rect == Some([0, 0, 320, 240]))
        );

        let mut fragmented = artifact.clone();
        fragmented.owner_nodes[0].parent = Some(roots[0]);
        assert_compiler_rejects_before_emit(&fragmented, "fragmented self-clip shadow owner");
    }

    #[test]
    fn nested_anchor_parent_requires_legacy_order_and_matches_strictly_when_partitioned() {
        for anchor_first in [true, false] {
            let (arena, roots, anchor) = nested_anchor_parent_mixed_siblings(anchor_first);
            let (properties, generations) = sync_identity(&arena, &roots);
            let clip = properties
                .paint_state_for(anchor)
                .and_then(|state| state.clip);

            if !anchor_first {
                assert_eq!(
                    clip,
                    Some(ClipNodeId {
                        owner: anchor,
                        role: ClipNodeRole::SelfClip,
                    })
                );
                let snapshots = assert_whole_frame_structural_parity(
                    || {
                        let (arena, roots, _) = nested_anchor_parent_mixed_siblings(false);
                        (arena, roots)
                    },
                    PaintParityConfig {
                        initial_scissor: Some([4, 6, 24, 18]),
                        ..PaintParityConfig::default()
                    },
                );
                let visible = snapshots
                    .iter()
                    .filter(|snapshot| f32::from_bits(snapshot.fill_color_bits[3]) > 0.0)
                    .collect::<Vec<_>>();
                assert_eq!(visible.len(), 2);
                assert_eq!(visible[0].opaque_depth_order, Some(0));
                assert_eq!(visible[1].opaque_depth_order, Some(1));
                assert_eq!(visible[1].effective_scissor_rect, Some([0, 0, 320, 240]));

                let (production_arena, production_roots, _) =
                    nested_anchor_parent_mixed_siblings(false);
                let (production_properties, production_generations) =
                    sync_identity(&production_arena, &production_roots);
                take_full_artifact_record_count();
                take_artifact_compile_count();
                let FrameArtifactRecordOutcome::Artifact {
                    artifact,
                    eligibility,
                } = record_clip_enabled_frame_artifact(
                    &production_arena,
                    &production_roots,
                    &production_properties,
                    &production_generations,
                    RendererMode::Auto,
                )
                .unwrap()
                else {
                    panic!("ordered nested AnchorParent must enter production clip authority")
                };
                assert!(eligibility.eligible);
                assert_eq!(take_full_artifact_record_count(), 3);
                let _ = compiled_whole_frame_graph(&artifact);
                assert_eq!(take_artifact_compile_count(), 1);
                continue;
            }

            assert_eq!(clip, None);

            take_full_artifact_record_count();
            let FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility) =
                record_frame_artifact(
                    &arena,
                    &roots,
                    &properties,
                    &generations,
                    RendererMode::Auto,
                )
                .expect("misordered nested AnchorParent must fail closed to legacy")
            else {
                panic!("nested AnchorParent must not produce an artifact")
            };
            assert!(
                eligibility
                    .reasons
                    .contains(&FrameArtifactFallbackReason::LegacyBoundary(
                        LegacyPaintReason::SelfClip
                    ))
            );
            assert_eq!(
                take_full_artifact_record_count(),
                0,
                "metadata rejection must happen before any full artifact hook"
            );

            let legacy = legacy_roots_graph(arena, &roots).test_rect_pass_snapshots();
            let visible = legacy
                .iter()
                .filter(|snapshot| f32::from_bits(snapshot.fill_color_bits[3]) > 0.0)
                .collect::<Vec<_>>();
            assert_eq!(visible.len(), 2);
            assert!(
                f32::from_bits(visible[0].fill_color_bits[2])
                    > f32::from_bits(visible[0].fill_color_bits[0]),
                "normal blue sibling paints before the overflow AnchorParent child"
            );
            assert!(
                f32::from_bits(visible[1].fill_color_bits[0])
                    > f32::from_bits(visible[1].fill_color_bits[2]),
                "overflow AnchorParent child paints in the legacy late phase"
            );
            assert_eq!(visible[0].opaque_depth_order, Some(0));
            assert_eq!(visible[1].opaque_depth_order, Some(1));
        }
    }

    #[test]
    fn nested_self_clip_metadata_and_full_hooks_require_owner_bound_witness() {
        let (arena, roots, anchor) = nested_anchor_parent_mixed_siblings(false);
        let normal = arena
            .children_of(roots[0])
            .into_iter()
            .find(|child| *child != anchor)
            .unwrap();
        let (properties, generations) = sync_identity(&arena, &roots);
        let state = properties.paint_state_for(anchor).unwrap();
        let generation = generations.local_generations_for(anchor).unwrap();
        let revision = PaintContentRevision {
            self_paint_revision: generation.self_paint_revision,
            composite_revision: generation.composite_revision,
            topology_revision: generation.topology_revision,
        };
        let node = arena.get(anchor).unwrap();

        assert!(
            node.element
                .record_shadow_paint_metadata(
                    anchor,
                    state,
                    revision,
                    &arena,
                    PaintRecordingContext::default(),
                )
                .is_none()
        );
        assert!(
            node.element
                .record_shadow_paint_artifact(
                    anchor,
                    state,
                    revision,
                    &arena,
                    PaintRecordingContext::default(),
                )
                .is_none()
        );

        let clip = state.clip.unwrap();
        let normal_stable_id = arena.get(normal).unwrap().element.stable_id();
        let leaked = PaintRecordingContext {
            recording_owner: Some(normal),
            recording_owner_stable_id: Some(normal_stable_id),
            authoritative_self_clip: Some(clip),
            ..Default::default()
        };
        assert!(!leaked.authorizes_self_clip_for(node.element.stable_id()));
        assert_eq!(
            node.element
                .shadow_paint_recording_capability(&arena, false, leaked),
            ShadowPaintRecordingCapability::Legacy(ShadowPaintBlocker::SelfClip)
        );
    }

    #[test]
    fn nested_anchor_parent_with_viewport_sibling_fails_before_full_recording() {
        let (arena, roots, anchor) = nested_anchor_parent_mixed_siblings(false);
        let normal = arena
            .children_of(roots[0])
            .into_iter()
            .find(|child| *child != anchor)
            .unwrap();
        let mut style = Style::new();
        style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(30.0))
                    .top(Length::px(24.0))
                    .clip(ClipMode::Viewport),
            ),
        );
        arena
            .get_mut(normal)
            .unwrap()
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .unwrap()
            .apply_style(style);

        let (properties, generations) = sync_identity(&arena, &roots);
        assert_eq!(
            properties
                .paint_state_for(anchor)
                .and_then(|state| state.clip),
            None
        );
        take_full_artifact_record_count();
        let outcome = record_clip_enabled_frame_artifact(
            &arena,
            &roots,
            &properties,
            &generations,
            RendererMode::Auto,
        )
        .unwrap();
        assert!(matches!(
            outcome,
            FrameArtifactRecordOutcome::WholeFrameLegacyFallback(_)
        ));
        assert_eq!(take_full_artifact_record_count(), 0);

        let (mut arena, roots, anchor) = nested_anchor_parent_mixed_siblings(false);
        let normal = arena
            .children_of(roots[0])
            .into_iter()
            .find(|child| *child != anchor)
            .unwrap();
        let mut deferred = CustomLeafPaintHost::fill(0x8d90);
        deferred.deferred = true;
        let deferred = arena.insert(Node::with_parent(Box::new(deferred), Some(roots[0])));
        arena.set_parent(normal, None);
        arena.set_children(roots[0], vec![deferred, anchor]);
        let (properties, generations) = sync_identity(&arena, &roots);
        assert_eq!(
            properties
                .paint_state_for(anchor)
                .and_then(|state| state.clip),
            None,
            "a non-Element deferred sibling must invalidate the exact ordering witness"
        );
        take_full_artifact_record_count();
        let outcome = record_clip_enabled_frame_artifact(
            &arena,
            &roots,
            &properties,
            &generations,
            RendererMode::Auto,
        )
        .unwrap();
        assert!(matches!(
            outcome,
            FrameArtifactRecordOutcome::WholeFrameLegacyFallback(_)
        ));
        assert_eq!(take_full_artifact_record_count(), 0);
    }

    #[test]
    fn strict_structural_parity_covers_target_size_format_and_scale() {
        for config in [
            PaintParityConfig::default(),
            PaintParityConfig {
                width: 640,
                height: 480,
                format: wgpu::TextureFormat::Rgba8Unorm,
                scale_factor: 2.0,
                initial_scissor: None,
            },
        ] {
            let snapshots = assert_whole_frame_structural_parity(
                || {
                    let (arena, root, _, _) =
                        prepared_leaf(106, Color::rgb(20, 40, 60), 1.0, false);
                    (arena, vec![root])
                },
                config,
            );
            assert_eq!(snapshots.len(), 1);
            assert!(snapshots[0].opaque);
        }
    }

    #[test]
    fn strict_snapshot_is_sensitive_to_scale_factor_alone() {
        let (arena, root, properties, generations) =
            prepared_leaf(108, Color::rgb(20, 40, 60), 1.0, false);
        let (artifact, _) = whole_frame_artifact(&arena, &[root], &properties, &generations);
        let base = PaintParityConfig::default();
        let scaled = PaintParityConfig {
            scale_factor: 2.0,
            ..base
        };
        let mut base_graph = compiled_whole_frame_graph_with_config(&artifact, base);
        let mut scaled_graph = compiled_whole_frame_graph_with_config(&artifact, scaled);
        let base_snapshot = strict_paint_snapshot(&mut base_graph, base);
        let scaled_snapshot = strict_paint_snapshot(&mut scaled_graph, scaled);

        assert_eq!(
            base_snapshot.graph, scaled_snapshot.graph,
            "scale is a viewport raster input, not FrameGraph topology"
        );
        assert_ne!(base_snapshot.viewport, scaled_snapshot.viewport);
        assert_ne!(base_snapshot, scaled_snapshot);
    }

    #[test]
    fn strict_structural_parity_tracks_opacity_classification_transition() {
        let before = assert_whole_frame_structural_parity(
            || {
                let (arena, root, _, _) = prepared_leaf(107, Color::rgb(50, 70, 90), 1.0, false);
                (arena, vec![root])
            },
            PaintParityConfig::default(),
        );
        let after = assert_whole_frame_structural_parity(
            || {
                let (arena, root, _, _) = prepared_leaf(107, Color::rgb(50, 70, 90), 0.5, false);
                (arena, vec![root])
            },
            PaintParityConfig::default(),
        );
        assert!(before[0].opaque);
        assert!(!after[0].opaque);
    }

    #[test]
    fn strict_structural_parity_covers_zero_opacity_without_partial_output() {
        let snapshots = assert_whole_frame_structural_parity(
            prepared_zero_opacity_tree,
            PaintParityConfig::default(),
        );
        assert_eq!(snapshots.len(), 1);
        assert!(f32::from_bits(snapshots[0].fill_color_bits[2]) > 0.9);
    }

    #[test]
    fn whole_frame_artifact_matches_legacy_for_nested_multi_root_order() {
        let (arena, roots, child) = prepared_plain_tree();
        let (properties, generations) = sync_identity(&arena, &roots);
        take_full_artifact_record_count();
        let (artifact, eligibility) =
            whole_frame_artifact(&arena, &roots, &properties, &generations);
        assert_eq!(
            take_full_artifact_record_count(),
            3,
            "eligible preflight must invoke the full hook exactly once per node"
        );
        assert!(eligibility.eligible);
        assert_eq!(eligibility.chunk_count, 3);
        assert_eq!(
            artifact
                .chunks
                .iter()
                .map(|chunk| chunk.owner)
                .collect::<Vec<_>>(),
            vec![roots[0], child, roots[1]],
            "parent self decoration must precede its child and later roots"
        );
        assert_eq!(artifact.chunks[0].op_range, 0..1);
        assert_eq!(artifact.chunks[1].op_range, 1..2);
        assert_eq!(artifact.chunks[2].op_range, 2..3);

        let artifact_graph = compiled_whole_frame_graph(&artifact);
        let (legacy_arena, legacy_roots, _) = prepared_plain_tree();
        let legacy_graph = legacy_roots_graph(legacy_arena, &legacy_roots);
        assert_eq!(
            artifact_graph.test_rect_pass_snapshots(),
            legacy_graph.test_rect_pass_snapshots()
        );
    }

    #[test]
    fn whole_frame_zero_opacity_keeps_empty_chunk_and_matches_legacy() {
        let mut arena = new_test_arena();
        let mut empty_element = leaf_element(110, Color::rgb(255, 0, 0), 1.0, false);
        let mut empty_style = Style::new();
        empty_style.insert(PropertyId::Opacity, ParsedValue::Opacity(Opacity::new(0.0)));
        empty_element.apply_style(empty_style);
        let empty = commit_element(&mut arena, Box::new(empty_element));
        let visible = commit_element(
            &mut arena,
            Box::new(leaf_element(111, Color::rgb(0, 0, 255), 1.0, false)),
        );
        let roots = vec![empty, visible];
        let (measure, place) = constraints();
        measure_and_place(&mut arena, empty, measure, place);
        measure_and_place(&mut arena, visible, measure, place);
        let (properties, generations) = sync_identity(&arena, &roots);
        let (artifact, eligibility) =
            whole_frame_artifact(&arena, &roots, &properties, &generations);
        assert_eq!(eligibility.chunk_count, 2);
        assert_eq!(eligibility.op_count, 1);
        assert_eq!(artifact.chunks[0].op_range, 0..0);
        assert_eq!(artifact.chunks[1].op_range, 0..1);

        let artifact_graph = compiled_whole_frame_graph(&artifact);
        let mut legacy_arena = new_test_arena();
        let mut legacy_empty_element = leaf_element(110, Color::rgb(255, 0, 0), 1.0, false);
        let mut legacy_empty_style = Style::new();
        legacy_empty_style.insert(PropertyId::Opacity, ParsedValue::Opacity(Opacity::new(0.0)));
        legacy_empty_element.apply_style(legacy_empty_style);
        let legacy_empty = commit_element(&mut legacy_arena, Box::new(legacy_empty_element));
        let legacy_visible = commit_element(
            &mut legacy_arena,
            Box::new(leaf_element(111, Color::rgb(0, 0, 255), 1.0, false)),
        );
        measure_and_place(&mut legacy_arena, legacy_empty, measure, place);
        measure_and_place(&mut legacy_arena, legacy_visible, measure, place);
        let legacy_graph = legacy_roots_graph(legacy_arena, &[legacy_empty, legacy_visible]);
        assert_eq!(
            artifact_graph.test_rect_pass_snapshots(),
            legacy_graph.test_rect_pass_snapshots()
        );
    }

    #[test]
    fn property_neutral_canary_rejects_zero_op_non_neutral_node_before_full_recording() {
        let (arena, roots) = prepared_zero_opacity_tree();
        let (properties, generations) = sync_identity(&arena, &roots);
        take_full_artifact_record_count();
        let outcome = record_property_neutral_frame_artifact(
            &arena,
            &roots,
            &properties,
            &generations,
            RendererMode::Auto,
        )
        .expect("canary uses whole-frame fallback");
        let FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility) = outcome else {
            panic!("zero-op effect node must not enter M6A authority")
        };
        assert!(
            eligibility
                .reasons
                .contains(&FrameArtifactFallbackReason::PropertyBoundary(roots[0]))
        );
        assert_eq!(
            take_full_artifact_record_count(),
            0,
            "reachable property state must reject during metadata preflight"
        );
    }

    #[test]
    fn property_neutral_canary_rejects_deferred_boundaries_preflight() {
        let mut deferred = Element::new_with_id(0x6a02, 0.0, 0.0, 20.0, 20.0);
        let mut style = Style::new();
        style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(0.0))
                    .clip(ClipMode::Viewport),
            ),
        );
        deferred.apply_style(style);
        let mut deferred_arena = new_test_arena();
        let deferred_root = commit_element(&mut deferred_arena, Box::new(deferred));
        let (measure, place) = constraints();
        measure_and_place(&mut deferred_arena, deferred_root, measure, place);
        let (properties, generations) = sync_identity(&deferred_arena, &[deferred_root]);
        take_full_artifact_record_count();
        let deferred_outcome = record_property_neutral_frame_artifact(
            &deferred_arena,
            &[deferred_root],
            &properties,
            &generations,
            RendererMode::Auto,
        )
        .expect("deferred paint is a whole-frame fallback");
        assert!(matches!(
            deferred_outcome,
            FrameArtifactRecordOutcome::WholeFrameLegacyFallback(FrameArtifactEligibility {
                reasons,
                ..
            }) if reasons.contains(&FrameArtifactFallbackReason::PropertyBoundary(deferred_root))
        ));
        assert_eq!(take_full_artifact_record_count(), 0);
    }

    #[test]
    fn whole_frame_auto_falls_back_and_forced_reports_before_compilation() {
        let mut arena = new_test_arena();
        let safe = commit_element(
            &mut arena,
            Box::new(leaf_element(120, Color::rgb(1, 2, 3), 1.0, false)),
        );
        let unsupported = commit_element(
            &mut arena,
            Box::new(Text::new(0.0, 0.0, 20.0, 20.0, "text")),
        );
        let roots = [safe, unsupported];
        let (measure, place) = constraints();
        measure_and_place(&mut arena, safe, measure, place);
        let (properties, generations) = sync_identity(&arena, &roots);
        take_full_artifact_record_count();

        let auto = record_frame_artifact(
            &arena,
            &roots,
            &properties,
            &generations,
            RendererMode::Auto,
        )
        .expect("Auto must use whole-frame fallback");
        let FrameArtifactRecordOutcome::WholeFrameLegacyFallback(auto) = auto else {
            panic!("mixed host frame must not return a partial artifact")
        };
        assert_eq!(
            take_full_artifact_record_count(),
            0,
            "a late unsupported boundary must prevent all full recording"
        );
        assert!(!auto.eligible);
        assert!(
            auto.reasons
                .contains(&FrameArtifactFallbackReason::LegacyBoundary(
                    LegacyPaintReason::MissingPreparedText
                ))
        );

        let forced = record_frame_artifact(
            &arena,
            &roots,
            &properties,
            &generations,
            RendererMode::ForcedForTests,
        )
        .expect_err("forced mode must surface eligibility failure");
        assert_eq!(
            take_full_artifact_record_count(),
            0,
            "ForcedForTests uses the same metadata-only preflight"
        );
        assert!(
            forced
                .reasons
                .contains(&FrameArtifactFallbackReason::LegacyBoundary(
                    LegacyPaintReason::MissingPreparedText
                ))
        );

        let legacy_mode = record_frame_artifact(
            &arena,
            &roots,
            &properties,
            &generations,
            RendererMode::Legacy,
        )
        .expect("production-default legacy mode is a no-op");
        assert!(matches!(
            legacy_mode,
            FrameArtifactRecordOutcome::WholeFrameLegacyFallback(FrameArtifactEligibility {
                reasons,
                ..
            }) if reasons == vec![FrameArtifactFallbackReason::RendererLegacy]
        ));
    }

    #[test]
    fn whole_frame_recording_is_deterministic_and_revisions_track_mutation() {
        let (arena, roots, _) = prepared_plain_tree();
        let (mut properties, mut generations) = sync_identity(&arena, &roots);
        let (first, _) = whole_frame_artifact(&arena, &roots, &properties, &generations);
        let (unchanged, _) = whole_frame_artifact(&arena, &roots, &properties, &generations);
        assert_eq!(format!("{first:?}"), format!("{unchanged:?}"));

        arena
            .get_mut(roots[0])
            .expect("root exists")
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .expect("root is Element")
            .set_background_color_value(Color::rgb(90, 80, 70));
        properties.sync(&arena, &roots);
        generations.sync(&arena, &roots, &properties);
        let (changed, _) = whole_frame_artifact(&arena, &roots, &properties, &generations);
        assert_eq!(first.chunks[0].id, changed.chunks[0].id);
        assert_ne!(
            first.chunks[0].content_revision,
            changed.chunks[0].content_revision
        );
        assert_eq!(first.chunks[1].id, changed.chunks[1].id);
        assert_eq!(
            first.chunks[1].content_revision,
            changed.chunks[1].content_revision
        );
    }

    #[test]
    fn inline_owned_text_records_source_owned_glyphs_and_matches_legacy_pass() {
        let (arena, roots, text_key) = prepared_inline_owned_text_tree(InlineOwnedTextDamage::None);
        let (properties, generations) = sync_identity(&arena, &roots);
        take_full_artifact_record_count();
        let (artifact, eligibility) =
            whole_frame_artifact(&arena, &roots, &properties, &generations);
        assert!(eligibility.eligible);
        assert_eq!(artifact.chunks.len(), 1);
        assert_eq!(artifact.chunks[0].owner, text_key);
        assert!(
            matches!(
                artifact.chunks[0].payload_identity,
                PaintPayloadIdentity::PreparedTexts(_)
            ),
            "source Text must own the complete prepared glyph identity"
        );
        assert_eq!(take_full_artifact_record_count(), 1);

        let artifact_graph = compiled_whole_frame_graph(&artifact);
        let (legacy_arena, legacy_roots, _) =
            prepared_inline_owned_text_tree(InlineOwnedTextDamage::None);
        let legacy_graph = legacy_roots_graph(legacy_arena, &legacy_roots);
        let artifact_passes = artifact_graph
            .test_graphics_passes::<crate::view::render_pass::text_pass::TextPreparedInputPass>(
        );
        let legacy_passes = legacy_graph
            .test_graphics_passes::<crate::view::render_pass::text_pass::TextPreparedInputPass>();
        assert_eq!(artifact_passes.len(), 1);
        assert_eq!(legacy_passes.len(), 1);
        assert_eq!(
            artifact_passes[0].test_snapshot(),
            legacy_passes[0].test_snapshot()
        );
    }

    #[test]
    fn missing_or_malformed_inline_owned_text_falls_back_before_every_full_hook() {
        for damage in [
            InlineOwnedTextDamage::MissingGlyphs,
            InlineOwnedTextDamage::MissingFont,
        ] {
            let (arena, roots, _) = prepared_inline_owned_text_tree(damage);
            let (properties, generations) = sync_identity(&arena, &roots);
            take_full_artifact_record_count();
            let outcome = record_frame_artifact(
                &arena,
                &roots,
                &properties,
                &generations,
                RendererMode::Auto,
            )
            .unwrap();
            let FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility) = outcome else {
                panic!("invalid installed text input must keep the whole frame on legacy")
            };
            assert!(
                eligibility
                    .reasons
                    .contains(&FrameArtifactFallbackReason::LegacyBoundary(
                        LegacyPaintReason::MissingPreparedText
                    ))
            );
            assert_eq!(take_full_artifact_record_count(), 0);
        }
    }

    #[test]
    fn compiler_rejects_empty_or_tampered_prepared_text_before_emit() {
        let (arena, roots, _) = prepared_inline_owned_text_tree(InlineOwnedTextDamage::None);
        let (properties, generations) = sync_identity(&arena, &roots);
        let artifact = whole_frame_artifact(&arena, &roots, &properties, &generations).0;

        let mut empty = artifact.clone();
        let PaintOp::PreparedText(op) = &mut empty.ops[0] else {
            panic!("fixture must contain prepared text")
        };
        op.params.staging_input.glyphs.clear();
        assert_compiler_rejects_before_emit(&empty, "empty prepared text op");

        let mut op_scissor = artifact.clone();
        let PaintOp::PreparedText(op) = &op_scissor.ops[0] else {
            panic!("fixture must contain prepared text")
        };
        let mut params = op.params.clone();
        params.scissor_rect = Some([0, 0, 10, 10]);
        op_scissor.ops[0] = PaintOp::PreparedText(
            PreparedTextOp::new(params).expect("non-empty op scissor remains canonical payload"),
        );
        let range = op_scissor.chunks[0].op_range.clone();
        op_scissor.chunks[0].payload_identity =
            PaintPayloadIdentity::prepared_texts(op_scissor.ops[range].iter().filter_map(|op| {
                match op {
                    PaintOp::PreparedText(prepared) => Some(prepared),
                    _ => None,
                }
            }));
        assert_compiler_rejects_before_emit(&op_scissor, "prepared text op scissor");

        let mut tampered = artifact;
        let PaintOp::PreparedText(op) = &mut tampered.ops[0] else {
            panic!("fixture must contain prepared text")
        };
        op.params.fragments[0].origin[0] += 1.0;
        assert_compiler_rejects_before_emit(&tampered, "tampered prepared text fragment");
    }

    #[test]
    fn wrapping_inline_span_owns_typed_decoration_before_text_and_matches_legacy() {
        let (arena, roots, span_key, text_key, fragment_count) =
            prepared_wrapping_inline_span_tree();
        let (properties, generations) = sync_identity(&arena, &roots);
        take_full_artifact_record_count();
        let (artifact, eligibility) =
            whole_frame_artifact(&arena, &roots, &properties, &generations);
        assert!(eligibility.eligible);
        assert_eq!(artifact.chunks.len(), 2);
        assert_eq!(artifact.chunks[0].owner, span_key);
        assert_eq!(artifact.chunks[0].id.role, PaintChunkRole::SelfDecoration);
        assert_eq!(artifact.chunks[1].owner, text_key);
        assert_eq!(artifact.chunks[1].id.role, PaintChunkRole::TextGlyphs);
        assert!(matches!(
            artifact.chunks[0].payload_identity,
            PaintPayloadIdentity::InlineIfcDecorations(_)
        ));
        assert_eq!(artifact.chunks[0].op_range.len(), fragment_count);
        assert!(
            artifact.ops[artifact.chunks[0].op_range.clone()]
                .iter()
                .all(|op| matches!(op, PaintOp::PreparedInlineIfcDecoration(_)))
        );
        assert_eq!(take_full_artifact_record_count(), 2);

        let artifact_rects = compiled_whole_frame_graph(&artifact).test_rect_pass_snapshots();
        let (legacy_arena, legacy_roots, _, _, _) = prepared_wrapping_inline_span_tree();
        let legacy_rects =
            legacy_roots_graph(legacy_arena, &legacy_roots).test_rect_pass_snapshots();
        assert_eq!(artifact_rects, legacy_rects);
    }

    #[test]
    fn sampled_inline_span_layout_transition_keeps_metadata_full_and_legacy_parity() {
        fn sample_transition(arena: &NodeArena, span_key: NodeKey) {
            let mut node = arena.get_mut(span_key).unwrap();
            let span = node.element.as_any_mut().downcast_mut::<Element>().unwrap();
            let package_before = span
                .inline_ifc_decoration_package_for_test()
                .expect("layout must install the inline decoration package")
                .clone();
            span.set_layout_transition_width(71.0);
            span.set_layout_transition_height(39.0);
            assert_eq!(
                span.inline_ifc_decoration_package_for_test()
                    .expect("sampling must preserve the installed paint package"),
                &package_before
            );
            span.clear_local_dirty_flags(DirtyFlags::ALL);
        }

        let (arena, roots, span_key, _, _) = prepared_wrapping_inline_span_tree();
        sample_transition(&arena, span_key);
        arena.clear_arena_dirty_subtree(span_key, DirtyFlags::ALL);
        arena.refresh_subtree_dirty_cache(span_key);
        let (properties, generations) = sync_identity(&arena, &roots);
        let manifest = |mode| {
            record_coverage_manifest(&arena, &roots, false, true, mode, &properties, &generations)
        };
        let metadata = manifest(CoverageRecordingMode::MetadataOnly);
        let full = manifest(CoverageRecordingMode::FullArtifact);
        assert!(metadata.validation_errors.is_empty());
        assert!(full.validation_errors.is_empty());
        assert!(
            metadata
                .items
                .iter()
                .all(|item| !matches!(item, PaintCoverageItem::LegacyBoundary { .. }))
        );
        assert!(canonical_manifest_matches_for_test(&metadata, &full));

        let (artifact, eligibility) =
            whole_frame_artifact(&arena, &roots, &properties, &generations);
        assert!(eligibility.eligible);
        let artifact_rects = compiled_whole_frame_graph(&artifact).test_rect_pass_snapshots();

        let (legacy_arena, legacy_roots, legacy_span, _, _) = prepared_wrapping_inline_span_tree();
        sample_transition(&legacy_arena, legacy_span);
        let legacy_rects =
            legacy_roots_graph(legacy_arena, &legacy_roots).test_rect_pass_snapshots();
        assert_eq!(artifact_rects, legacy_rects);
    }

    #[test]
    fn nested_inline_spans_preserve_source_owner_dfs_and_legacy_rect_order() {
        let (arena, roots, expected_owners) = prepared_nested_inline_span_tree();
        let (properties, generations) = sync_identity(&arena, &roots);
        let (artifact, eligibility) =
            whole_frame_artifact(&arena, &roots, &properties, &generations);
        assert!(eligibility.eligible);
        assert_eq!(
            artifact
                .chunks
                .iter()
                .map(|chunk| chunk.owner)
                .collect::<Vec<_>>(),
            expected_owners
        );
        assert!(matches!(
            artifact.chunks[0].payload_identity,
            PaintPayloadIdentity::InlineIfcDecorations(_)
        ));
        assert!(matches!(
            artifact.chunks[2].payload_identity,
            PaintPayloadIdentity::InlineIfcDecorations(_)
        ));
        let artifact_rects = compiled_whole_frame_graph(&artifact).test_rect_pass_snapshots();
        let (legacy_arena, legacy_roots, _) = prepared_nested_inline_span_tree();
        let legacy_rects =
            legacy_roots_graph(legacy_arena, &legacy_roots).test_rect_pass_snapshots();
        assert_eq!(artifact_rects, legacy_rects);
    }

    #[test]
    fn missing_or_malformed_inline_span_package_falls_back_before_full_hooks() {
        for malformed in [false, true] {
            let (arena, roots, span_key, _, _) = prepared_wrapping_inline_span_tree();
            let mut node = arena.get_mut(span_key).unwrap();
            let span = node.element.as_any_mut().downcast_mut::<Element>().unwrap();
            let package = span
                .inline_ifc_decoration_package_for_test()
                .expect("fixture must install a decoration package");
            if malformed {
                package.fragments[0].metadata.position[0] = f32::NAN;
            } else {
                package.fragments.clear();
            }
            drop(node);
            let (properties, generations) = sync_identity(&arena, &roots);
            take_full_artifact_record_count();
            let outcome = record_frame_artifact(
                &arena,
                &roots,
                &properties,
                &generations,
                RendererMode::Auto,
            )
            .unwrap();
            let FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility) = outcome else {
                panic!("invalid installed inline decoration must fail closed")
            };
            assert!(
                eligibility
                    .reasons
                    .contains(&FrameArtifactFallbackReason::LegacyBoundary(
                        LegacyPaintReason::MissingPreparedInlineDecoration
                    ))
            );
            assert_eq!(take_full_artifact_record_count(), 0);
        }
    }

    #[test]
    fn cross_owner_inline_span_package_falls_back_before_full_hooks() {
        let (arena, roots, span_key, _, _) = prepared_wrapping_inline_span_tree();
        let mut node = arena.get_mut(span_key).unwrap();
        let span = node.element.as_any_mut().downcast_mut::<Element>().unwrap();
        let package = span
            .inline_ifc_decoration_package_for_test()
            .expect("fixture must install a decoration package");
        package.source.0 = package.source.0.wrapping_add(1000);
        for fragment in &mut package.fragments {
            fragment.source = package.source;
        }
        drop(node);
        let (properties, generations) = sync_identity(&arena, &roots);
        take_full_artifact_record_count();
        let outcome = record_frame_artifact(
            &arena,
            &roots,
            &properties,
            &generations,
            RendererMode::Auto,
        )
        .unwrap();
        let FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility) = outcome else {
            panic!("a package from another source owner must fail closed")
        };
        assert!(
            eligibility
                .reasons
                .contains(&FrameArtifactFallbackReason::LegacyBoundary(
                    LegacyPaintReason::MissingPreparedInlineDecoration
                ))
        );
        assert_eq!(take_full_artifact_record_count(), 0);
    }

    #[test]
    fn inline_span_paint_mutation_refreshes_same_constraints_frame_for_typed_and_legacy() {
        fn mutate_paint(arena: &NodeArena, span_key: NodeKey) {
            let mut node = arena.get_mut(span_key).unwrap();
            let span = node.element.as_any_mut().downcast_mut::<Element>().unwrap();
            span.set_background_color_value(Color::rgb(22, 163, 74));
            span.set_opacity(0.6);
            span.set_border_left_color(Color::rgb(126, 34, 206));
            span.set_border_right_color(Color::rgb(126, 34, 206));
            span.set_border_top_color(Color::rgb(126, 34, 206));
            span.set_border_bottom_color(Color::rgb(126, 34, 206));
        }

        let (stale_arena, stale_roots, stale_span, stale_text, _) =
            prepared_wrapping_inline_span_tree();
        let stale_parent = stale_arena.parent_of(stale_span).unwrap();
        settle_wrapping_inline_span_frame(&stale_arena, stale_parent, stale_span, stale_text);
        mutate_paint(&stale_arena, stale_span);
        let (stale_properties, stale_generations) = sync_identity(&stale_arena, &stale_roots);
        take_full_artifact_record_count();
        let stale = record_frame_artifact(
            &stale_arena,
            &stale_roots,
            &stale_properties,
            &stale_generations,
            RendererMode::Auto,
        )
        .unwrap();
        let FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility) = stale else {
            panic!("recording without layout must not consume stale paint packages")
        };
        assert!(
            eligibility
                .reasons
                .contains(&FrameArtifactFallbackReason::LegacyBoundary(
                    LegacyPaintReason::MissingPreparedInlineDecoration
                ))
        );
        assert_eq!(take_full_artifact_record_count(), 0);

        let (mut arena, roots, span_key, text_key, _) = prepared_wrapping_inline_span_tree();
        let parent_key = arena.parent_of(span_key).unwrap();
        settle_wrapping_inline_span_frame(&arena, parent_key, span_key, text_key);
        mutate_paint(&arena, span_key);
        let (measure, place) = wrapping_inline_span_constraints();
        crate::view::base_component::reset_layout_place_profile();
        crate::view::base_component::set_layout_place_profile_enabled(true);
        measure_and_place(&mut arena, parent_key, measure, place);
        crate::view::base_component::set_layout_place_profile_enabled(false);
        let profile = crate::view::base_component::take_layout_place_profile();
        assert_eq!(
            (
                profile.ifc_measure_cheap,
                profile.ifc_measure_shortcircuit,
                profile.ifc_measure_full,
            ),
            (0, 0, 0),
            "identical constraints must skip re-measuring the owning IFC root"
        );
        assert_eq!(profile.inline_ifc_root_install_calls, 1);
        assert_eq!(
            profile.inline_ifc_root_install_reuse_calls, 0,
            "paint-only damage must rebuild the installed package in this frame"
        );
        let (properties, generations) = sync_identity(&arena, &roots);
        let (artifact, eligibility) =
            whole_frame_artifact(&arena, &roots, &properties, &generations);
        assert!(eligibility.eligible);
        let PaintOp::PreparedInlineIfcDecoration(first) = &artifact.ops[0] else {
            panic!("refreshed span must record inline decoration")
        };
        assert_eq!(
            first.fill.fill_color.map(f32::to_bits),
            Color::rgb(22, 163, 74).to_rgba_f32().map(f32::to_bits)
        );
        assert_eq!(first.fill.opacity.to_bits(), 0.6_f32.to_bits());
        assert_eq!(
            first.border.as_ref().unwrap().border_side_colors[0].map(f32::to_bits),
            Color::rgb(126, 34, 206).to_rgba_f32().map(f32::to_bits)
        );
        let artifact_rects = compiled_whole_frame_graph(&artifact).test_rect_pass_snapshots();

        let (mut legacy_arena, legacy_roots, legacy_span, legacy_text, _) =
            prepared_wrapping_inline_span_tree();
        let legacy_parent = legacy_arena.parent_of(legacy_span).unwrap();
        settle_wrapping_inline_span_frame(&legacy_arena, legacy_parent, legacy_span, legacy_text);
        mutate_paint(&legacy_arena, legacy_span);
        measure_and_place(&mut legacy_arena, legacy_parent, measure, place);
        {
            let mut node = legacy_arena.get_mut(legacy_span).unwrap();
            let span = node.element.as_any_mut().downcast_mut::<Element>().unwrap();
            let package = span
                .inline_ifc_decoration_package_for_test()
                .expect("same frame must install a fresh legacy package");
            let first = package.fragments.first().unwrap();
            assert_eq!(
                first.metadata.fill_color.map(f32::to_bits),
                Color::rgb(22, 163, 74).to_rgba_f32().map(f32::to_bits)
            );
            assert_eq!(first.metadata.opacity.to_bits(), 0.6_f32.to_bits());
            assert_eq!(
                first.metadata.border_colors[0].map(f32::to_bits),
                Color::rgb(126, 34, 206).to_rgba_f32().map(f32::to_bits)
            );
        }
        let legacy_rects =
            legacy_roots_graph(legacy_arena, &legacy_roots).test_rect_pass_snapshots();
        assert_eq!(artifact_rects, legacy_rects);
    }

    #[test]
    fn clean_inline_span_origin_move_preserves_install_reuse_fast_path() {
        let (mut arena, _, span_key, text_key, _) = prepared_wrapping_inline_span_tree();
        let parent_key = arena.parent_of(span_key).unwrap();
        settle_wrapping_inline_span_frame(&arena, parent_key, span_key, text_key);
        let before = arena
            .get_mut(span_key)
            .unwrap()
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .unwrap()
            .inline_ifc_decoration_package_for_test()
            .unwrap()
            .clone();

        let (measure, mut place) = wrapping_inline_span_constraints();
        place.parent_x = 7.0;
        place.parent_y = 11.0;
        crate::view::base_component::reset_layout_place_profile();
        crate::view::base_component::set_layout_place_profile_enabled(true);
        measure_and_place(&mut arena, parent_key, measure, place);
        crate::view::base_component::set_layout_place_profile_enabled(false);
        let profile = crate::view::base_component::take_layout_place_profile();
        assert_eq!(
            (
                profile.ifc_measure_cheap,
                profile.ifc_measure_shortcircuit,
                profile.ifc_measure_full,
            ),
            (0, 0, 0),
            "a clean same-constraints move must skip IFC measure"
        );
        assert_eq!(profile.inline_ifc_root_install_calls, 1);
        assert_eq!(
            profile.inline_ifc_root_install_reuse_calls, 1,
            "the paint freshness guard must preserve clean origin-only reuse"
        );

        let after = arena
            .get_mut(span_key)
            .unwrap()
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .unwrap()
            .inline_ifc_decoration_package_for_test()
            .unwrap()
            .clone();
        let mut expected = before;
        for fragment in &mut expected.fragments {
            fragment.metadata.position[0] += 7.0;
            fragment.metadata.position[1] += 11.0;
        }
        assert_eq!(after, expected);
    }

    #[test]
    fn non_painting_inline_span_uses_only_typed_empty_decoration() {
        let (arena, roots, span_key, _, _) = prepared_wrapping_inline_span_tree();
        let mut node = arena.get_mut(span_key).unwrap();
        let span = node.element.as_any_mut().downcast_mut::<Element>().unwrap();
        span.set_should_paint_for_test(false);
        span.inline_ifc_decoration_package_for_test()
            .unwrap()
            .fragments
            .clear();
        drop(node);
        let (properties, generations) = sync_identity(&arena, &roots);
        let (mut artifact, eligibility) =
            whole_frame_artifact(&arena, &roots, &properties, &generations);
        assert!(eligibility.eligible);
        let span_chunk = artifact
            .chunks
            .iter()
            .find(|chunk| chunk.owner == span_key)
            .unwrap();
        assert!(span_chunk.op_range.is_empty());
        assert!(matches!(
            &span_chunk.payload_identity,
            PaintPayloadIdentity::InlineIfcDecorations(identities) if identities.is_empty()
        ));
        artifact.chunks[0].bounds.width = 0.0;
        artifact.chunks[0].bounds.height = 0.0;
        take_artifact_compile_count();
        let _ = compiled_whole_frame_graph(&artifact);
        assert_eq!(take_artifact_compile_count(), 1);
    }

    #[test]
    fn empty_text_records_canonical_transparent_node_without_payload() {
        let mut arena = new_test_arena();
        let text_key = commit_element(
            &mut arena,
            Box::new(Text::new_with_id(0x7b20, 0.0, 0.0, 0.0, 0.0, "")),
        );
        let (measure, place) = constraints();
        measure_and_place(&mut arena, text_key, measure, place);
        let roots = [text_key];
        let (properties, generations) = sync_identity(&arena, &roots);
        let manifest = |mode| {
            record_coverage_manifest(&arena, &roots, false, true, mode, &properties, &generations)
        };
        let metadata = manifest(CoverageRecordingMode::MetadataOnly);
        let full = manifest(CoverageRecordingMode::FullArtifact);
        assert!(matches!(
            metadata.items.as_slice(),
            [PaintCoverageItem::TransparentNode { owner, .. }] if *owner == text_key
        ));
        assert!(canonical_manifest_matches_for_test(&metadata, &full));

        let (artifact, eligibility) =
            whole_frame_artifact(&arena, &roots, &properties, &generations);
        assert!(eligibility.eligible);
        assert_eq!(eligibility.chunk_count, 0);
        assert_eq!(eligibility.op_count, 0);
        assert!(artifact.chunks.is_empty());
        assert!(artifact.ops.is_empty());
        take_artifact_compile_count();
        let _ = compiled_whole_frame_graph(&artifact);
        assert_eq!(take_artifact_compile_count(), 1);
    }

    #[test]
    fn inline_decoration_constructor_and_compiler_reject_link_or_identity_drift() {
        let (arena, roots, _, _, _) = prepared_wrapping_inline_span_tree();
        let (properties, generations) = sync_identity(&arena, &roots);
        let artifact = whole_frame_artifact(&arena, &roots, &properties, &generations).0;
        let PaintOp::PreparedInlineIfcDecoration(first) = &artifact.ops[0] else {
            panic!("fixture must start with an inline decoration")
        };
        let mut mismatched_border = first.border.clone().expect("fixture must have a border");
        mismatched_border.depth = 1.0;
        assert!(
            PreparedInlineIfcDecorationOp::new(
                first.descriptor.clone(),
                first.fill.clone(),
                Some(mismatched_border),
            )
            .is_none(),
            "constructor must reject linked-field mismatch"
        );
        let mut gradient_fill = first.fill.clone();
        gradient_fill.gradient = Some(Default::default());
        assert!(
            PreparedInlineIfcDecorationOp::new(
                first.descriptor.clone(),
                gradient_fill,
                first.border.clone(),
            )
            .is_none(),
            "M7B explicitly excludes gradients"
        );
        let mut overflow_fill = first.fill.clone();
        overflow_fill.position[0] = f32::MAX;
        overflow_fill.size[0] = f32::MAX;
        let mut overflow_border = first.border.clone().unwrap();
        overflow_border.position[0] = f32::MAX;
        overflow_border.size[0] = f32::MAX;
        assert!(
            PreparedInlineIfcDecorationOp::new(
                first.descriptor.clone(),
                overflow_fill,
                Some(overflow_border),
            )
            .is_none(),
            "large finite rect inputs whose edge overflows must fail closed"
        );

        let mut tampered_params = artifact.clone();
        let PaintOp::PreparedInlineIfcDecoration(op) = &mut tampered_params.ops[0] else {
            unreachable!()
        };
        op.fill.position[0] += 1.0;
        assert_compiler_rejects_before_emit(&tampered_params, "inline fill param drift");

        let mut nan_bounds = artifact.clone();
        nan_bounds.chunks[0].bounds.x = f32::NAN;
        assert_compiler_rejects_before_emit(&nan_bounds, "NaN chunk bounds");
        let mut negative_bounds = artifact.clone();
        negative_bounds.chunks[0].bounds.width = -1.0;
        assert_compiler_rejects_before_emit(&negative_bounds, "negative chunk bounds");

        let mut tampered_descriptor = artifact.clone();
        let PaintOp::PreparedInlineIfcDecoration(op) = &mut tampered_descriptor.ops[0] else {
            unreachable!()
        };
        op.descriptor.source = op.descriptor.source.wrapping_add(1);
        assert_compiler_rejects_before_emit(&tampered_descriptor, "inline descriptor drift");

        let mut missing_fragment = artifact;
        missing_fragment.ops.remove(0);
        missing_fragment.chunks[0].op_range.end -= 1;
        for chunk in &mut missing_fragment.chunks[1..] {
            chunk.op_range.start -= 1;
            chunk.op_range.end -= 1;
        }
        assert_compiler_rejects_before_emit(&missing_fragment, "missing inline fragment");

        let (arena, roots, _, _, _) = prepared_wrapping_inline_span_tree();
        let (properties, generations) = sync_identity(&arena, &roots);
        let artifact = whole_frame_artifact(&arena, &roots, &properties, &generations).0;
        let mut swapped = artifact.clone();
        swapped.ops.swap(0, 1);
        refresh_inline_decoration_payload_identity(&mut swapped);
        assert_compiler_rejects_before_emit(
            &swapped,
            "inline fragment order drift with rebuilt payload",
        );

        let mut endpoint_drift = artifact.clone();
        let PaintOp::PreparedInlineIfcDecoration(op) = endpoint_drift.ops[0].clone() else {
            unreachable!()
        };
        let mut descriptor = op.descriptor;
        descriptor.is_first_for_source = false;
        endpoint_drift.ops[0] = PaintOp::PreparedInlineIfcDecoration(
            PreparedInlineIfcDecorationOp::new(descriptor, op.fill, op.border).unwrap(),
        );
        refresh_inline_decoration_payload_identity(&mut endpoint_drift);
        assert_compiler_rejects_before_emit(
            &endpoint_drift,
            "inline endpoint drift with rebuilt op and payload",
        );

        let mut cross_source = artifact;
        let PaintOp::PreparedInlineIfcDecoration(op) = cross_source.ops[1].clone() else {
            unreachable!()
        };
        let mut descriptor = op.descriptor;
        descriptor.source = descriptor.source.wrapping_add(1);
        cross_source.ops[1] = PaintOp::PreparedInlineIfcDecoration(
            PreparedInlineIfcDecorationOp::new(descriptor, op.fill, op.border).unwrap(),
        );
        refresh_inline_decoration_payload_identity(&mut cross_source);
        assert_compiler_rejects_before_emit(
            &cross_source,
            "cross-source fragment with rebuilt op and payload",
        );
    }

    #[test]
    fn root_opacity_group_neutralizes_inline_span_and_text_once() {
        let (arena, roots, span_key, _, _) = prepared_wrapping_inline_span_tree_with_opacity(0.5);
        let (properties, generations) = sync_identity(&arena, &roots);
        let (artifact, eligibility) =
            root_group_artifact(&arena, &roots, &properties, &generations);
        assert!(eligibility.eligible);
        assert!(matches!(
            artifact.target,
            PaintArtifactTarget::RootOpacityGroup { root, .. } if root == span_key
        ));
        assert!(matches!(
            artifact.chunks[0].payload_identity,
            PaintPayloadIdentity::InlineIfcDecorations(_)
        ));
        assert!(
            artifact
                .ops
                .iter()
                .any(|op| matches!(op, PaintOp::PreparedText(_)))
        );
        artifact.ops.iter().for_each(assert_neutral_opacity);
        let graph = compiled_whole_frame_graph(&artifact);
        assert_eq!(
            graph
                .test_graphics_passes::<crate::view::render_pass::composite_layer_pass::CompositeLayerPass>()
                .len(),
            1
        );
    }

    #[test]
    fn deferred_inline_span_remains_fallback_before_full_hooks() {
        let (arena, roots, span_key, _, _) = prepared_wrapping_inline_span_tree();
        let mut node = arena.get_mut(span_key).unwrap();
        let span = node.element.as_any_mut().downcast_mut::<Element>().unwrap();
        let mut style = Style::new();
        style.insert(
            PropertyId::Position,
            ParsedValue::Position(Position::absolute().clip(ClipMode::Viewport)),
        );
        span.apply_style(style);
        drop(node);
        let (properties, generations) = sync_identity(&arena, &roots);
        take_full_artifact_record_count();
        let outcome = record_frame_artifact(
            &arena,
            &roots,
            &properties,
            &generations,
            RendererMode::Auto,
        )
        .unwrap();
        let FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility) = outcome else {
            panic!("deferred inline span must remain on legacy")
        };
        assert!(eligibility.reasons.iter().any(|reason| matches!(
            reason,
            FrameArtifactFallbackReason::LegacyBoundary(LegacyPaintReason::Deferred)
                | FrameArtifactFallbackReason::DeferredBoundary(_)
        )));
        assert_eq!(take_full_artifact_record_count(), 0);
    }

    #[test]
    fn atomic_inline_span_remains_fallback_before_full_hooks() {
        let (arena, roots, span_key, _, _) = prepared_wrapping_inline_span_tree();
        let mut node = arena.get_mut(span_key).unwrap();
        node.element
            .as_any_mut()
            .downcast_mut::<Element>()
            .unwrap()
            .install_empty_inline_ifc_atomic_package_for_test();
        drop(node);
        let (properties, generations) = sync_identity(&arena, &roots);
        take_full_artifact_record_count();
        let outcome = record_frame_artifact(
            &arena,
            &roots,
            &properties,
            &generations,
            RendererMode::Auto,
        )
        .unwrap();
        let FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility) = outcome else {
            panic!("atomic placement stays outside M7B")
        };
        assert!(
            eligibility
                .reasons
                .contains(&FrameArtifactFallbackReason::LegacyBoundary(
                    LegacyPaintReason::InlineIfc
                ))
        );
        assert_eq!(take_full_artifact_record_count(), 0);
    }

    #[test]
    fn fixed_inline_root_with_text_uses_the_owning_ifc_artifact_path() {
        let (arena, roots, root, text) = prepared_fixed_owning_inline_text_root();
        let (properties, generations) = sync_identity(&arena, &roots);
        let (artifact, eligibility) =
            whole_frame_artifact(&arena, &roots, &properties, &generations);
        assert!(eligibility.eligible);
        assert_eq!(
            artifact
                .chunks
                .iter()
                .map(|chunk| (chunk.owner, chunk.id.role))
                .collect::<Vec<_>>(),
            vec![
                (root, PaintChunkRole::SelfDecoration),
                (text, PaintChunkRole::TextGlyphs),
            ]
        );

        let (legacy_arena, legacy_roots, ..) = prepared_fixed_owning_inline_text_root();
        assert_eq!(
            compiled_whole_frame_graph(&artifact).pass_descriptors(),
            legacy_roots_graph(legacy_arena, &legacy_roots).pass_descriptors()
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn standalone_inline_element_image_and_svg_do_not_require_an_owning_ifc_install() {
        fn assert_eligible(arena: &NodeArena, root: NodeKey, label: &str) {
            let roots = [root];
            let (properties, generations) = sync_identity(arena, &roots);
            let (_, eligibility) = whole_frame_artifact(arena, &roots, &properties, &generations);
            assert!(eligibility.eligible, "{label}: {eligibility:?}");
        }

        let mut element_arena = new_test_arena();
        let mut element = Element::new_with_id(0x7d10, 0.0, 0.0, 24.0, 18.0);
        let mut element_style = Style::new();
        element_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        element_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(24.0)));
        element_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(18.0)));
        element_style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::rgb(59, 130, 246)),
        );
        element.apply_style(element_style);
        let element_root = commit_element(&mut element_arena, Box::new(element));
        let (measure, place) = constraints();
        measure_and_place(&mut element_arena, element_root, measure, place);
        element_arena
            .get_mut(element_root)
            .unwrap()
            .element
            .clear_local_dirty_flags(DirtyFlags::ALL);
        element_arena.clear_arena_dirty_subtree(element_root, DirtyFlags::ALL);
        assert_eligible(&element_arena, element_root, "Element");

        let mut image_arena = new_test_arena();
        let mut image = Image::new_with_id(
            0x7d11,
            ImageSource::Rgba {
                width: 1,
                height: 1,
                pixels: Arc::from([255, 255, 255, 255]),
            },
        );
        let mut image_style = Style::new();
        image_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        image_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(24.0)));
        image_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(18.0)));
        image.apply_style(image_style);
        let image_root = commit_element(&mut image_arena, Box::new(image));
        measure_and_place(&mut image_arena, image_root, measure, place);
        image_arena
            .get_mut(image_root)
            .unwrap()
            .element
            .clear_local_dirty_flags(DirtyFlags::ALL);
        image_arena.clear_arena_dirty_subtree(image_root, DirtyFlags::ALL);
        assert_eligible(&image_arena, image_root, "Image");

        const SVG: &str = "<svg xmlns='http://www.w3.org/2000/svg' width='24' height='18'><rect width='24' height='18' fill='#22c55e'/></svg>";
        let mut svg_arena = new_test_arena();
        let mut svg = Svg::new_with_id(0x7d12, SvgSource::Content(SVG.into()));
        let mut svg_style = Style::new();
        svg_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        svg_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(24.0)));
        svg_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(18.0)));
        svg.apply_style(svg_style);
        let svg_root = commit_element(&mut svg_arena, Box::new(svg));
        measure_and_place(&mut svg_arena, svg_root, measure, place);
        {
            let mut node = svg_arena.get_mut(svg_root).unwrap();
            let svg = node.element.as_any_mut().downcast_mut::<Svg>().unwrap();
            svg.prepare_content_paint_for_test(SVG, (24.0, 18.0), 1.0)
                .unwrap();
            svg.clear_local_dirty_flags(DirtyFlags::ALL);
        }
        svg_arena.set_children(svg_root, Vec::new());
        svg_arena.clear_arena_dirty_subtree(svg_root, DirtyFlags::ALL);
        assert_eligible(&svg_arena, svg_root, "Svg");
    }

    #[test]
    fn fixed_inline_root_missing_install_falls_back_before_full_hooks() {
        let (arena, roots, root, _) = prepared_fixed_owning_inline_text_root();
        arena
            .get_mut(root)
            .unwrap()
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .unwrap()
            .damage_owning_inline_ifc_root_witness_for_test(
                OwningInlineIfcRootWitnessDamage::MissingCurrent,
            );
        let (properties, generations) = sync_identity(&arena, &roots);
        take_full_artifact_record_count();
        let outcome = record_frame_artifact(
            &arena,
            &roots,
            &properties,
            &generations,
            RendererMode::Auto,
        )
        .unwrap();
        let FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility) = outcome else {
            panic!("fixed owning IFC root without a live install must fail closed")
        };
        assert!(
            eligibility
                .reasons
                .contains(&FrameArtifactFallbackReason::LegacyBoundary(
                    LegacyPaintReason::MissingPreparedInlineRoot
                ))
        );
        assert_eq!(take_full_artifact_record_count(), 0);
    }

    #[test]
    fn owning_inline_root_with_text_records_dom_order_and_matches_legacy() {
        let (arena, roots, root, text) = prepared_owning_inline_text_root();
        let (properties, generations) = sync_identity(&arena, &roots);
        let (artifact, eligibility) =
            whole_frame_artifact(&arena, &roots, &properties, &generations);
        assert!(eligibility.eligible);
        assert_eq!(artifact.chunks.len(), 2);
        assert_eq!(
            artifact
                .chunks
                .iter()
                .map(|chunk| (chunk.owner, chunk.id.role))
                .collect::<Vec<_>>(),
            vec![
                (root, PaintChunkRole::SelfDecoration),
                (text, PaintChunkRole::TextGlyphs),
            ]
        );
        let artifact_graph = compiled_whole_frame_graph(&artifact);
        let artifact_passes = artifact_graph.pass_descriptors();

        let (legacy_arena, legacy_roots, ..) = prepared_owning_inline_text_root();
        let legacy_graph = legacy_roots_graph(legacy_arena, &legacy_roots);
        let legacy_passes = legacy_graph.pass_descriptors();
        assert_eq!(artifact_passes, legacy_passes);
    }

    #[test]
    fn owning_inline_root_with_decorated_span_records_dom_dfs_and_matches_legacy() {
        let (arena, roots, root, span, text, fragment_count) =
            prepared_owning_wrapping_inline_span_tree_with_opacity(1.0);
        let (properties, generations) = sync_identity(&arena, &roots);
        let (artifact, eligibility) =
            whole_frame_artifact(&arena, &roots, &properties, &generations);
        assert!(eligibility.eligible);
        assert_eq!(
            artifact
                .chunks
                .iter()
                .map(|chunk| (chunk.owner, chunk.id.role))
                .collect::<Vec<_>>(),
            vec![
                (root, PaintChunkRole::SelfDecoration),
                (span, PaintChunkRole::SelfDecoration),
                (text, PaintChunkRole::TextGlyphs),
            ],
            "coverage DOM DFS, not IFC plan order, is paint order"
        );
        assert_eq!(artifact.chunks[1].op_range.len(), fragment_count);
        assert!(matches!(
            artifact.chunks[1].payload_identity,
            PaintPayloadIdentity::InlineIfcDecorations(_)
        ));
        let artifact_graph = compiled_whole_frame_graph(&artifact);
        let artifact_passes = artifact_graph.pass_descriptors();

        let (legacy_arena, legacy_roots, ..) =
            prepared_owning_wrapping_inline_span_tree_with_opacity(1.0);
        let legacy_graph = legacy_roots_graph(legacy_arena, &legacy_roots);
        assert_eq!(artifact_passes, legacy_graph.pass_descriptors());
    }

    #[test]
    fn owning_inline_root_witness_drift_falls_back_before_full_hooks() {
        for damage in [
            OwningInlineIfcRootWitnessDamage::MissingCurrent,
            OwningInlineIfcRootWitnessDamage::Pending,
            OwningInlineIfcRootWitnessDamage::ChildrenSnapshot,
            OwningInlineIfcRootWitnessDamage::PlanMissing,
            OwningInlineIfcRootWitnessDamage::PlanDuplicate,
            OwningInlineIfcRootWitnessDamage::InstalledMissing,
            OwningInlineIfcRootWitnessDamage::InstalledDuplicate,
            OwningInlineIfcRootWitnessDamage::CacheKey,
            OwningInlineIfcRootWitnessDamage::WrongKind,
            OwningInlineIfcRootWitnessDamage::LayoutDirty,
            OwningInlineIfcRootWitnessDamage::PlacementDirty,
        ] {
            let (arena, roots, root, _) = prepared_owning_inline_text_root();
            let mut node = arena.get_mut(root).unwrap();
            node.element
                .as_any_mut()
                .downcast_mut::<Element>()
                .unwrap()
                .damage_owning_inline_ifc_root_witness_for_test(damage);
            drop(node);

            let (properties, generations) = sync_identity(&arena, &roots);
            take_full_artifact_record_count();
            let outcome = record_frame_artifact(
                &arena,
                &roots,
                &properties,
                &generations,
                RendererMode::Auto,
            )
            .unwrap();
            let FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility) = outcome else {
                panic!("owning IFC root witness drift must fail closed: {damage:?}")
            };
            assert!(
                eligibility
                    .reasons
                    .contains(&FrameArtifactFallbackReason::LegacyBoundary(
                        LegacyPaintReason::MissingPreparedInlineRoot
                    )),
                "{damage:?}: {eligibility:?}"
            );
            assert_eq!(take_full_artifact_record_count(), 0, "{damage:?}");
        }
    }

    #[test]
    fn owning_inline_root_atomic_witness_field_drift_falls_back_before_full_hooks() {
        for damage in [
            OwningInlineIfcRootWitnessDamage::AtomicStableId,
            OwningInlineIfcRootWitnessDamage::AtomicSource,
            OwningInlineIfcRootWitnessDamage::AtomicInlineBoxId,
            OwningInlineIfcRootWitnessDamage::AtomicInsertionByte,
            OwningInlineIfcRootWitnessDamage::AtomicLineIndex,
            OwningInlineIfcRootWitnessDamage::AtomicMeasurementMaxWidth,
            OwningInlineIfcRootWitnessDamage::AtomicMeasurementAvailableHeight,
            OwningInlineIfcRootWitnessDamage::AtomicMeasurementViewport,
            OwningInlineIfcRootWitnessDamage::AtomicMeasurementPercentBase,
            OwningInlineIfcRootWitnessDamage::AtomicMeasurementSizing,
            OwningInlineIfcRootWitnessDamage::AtomicMeasurementSize,
            OwningInlineIfcRootWitnessDamage::AtomicRawRect,
            OwningInlineIfcRootWitnessDamage::AtomicAlignedRect,
            OwningInlineIfcRootWitnessDamage::AtomicVerticalAlign,
        ] {
            let (arena, roots, root, ..) = prepared_owning_inline_root_with_atomic();
            arena
                .get_mut(root)
                .unwrap()
                .element
                .as_any_mut()
                .downcast_mut::<Element>()
                .unwrap()
                .damage_owning_inline_ifc_root_witness_for_test(damage);
            let (properties, generations) = sync_identity(&arena, &roots);
            take_full_artifact_record_count();
            let outcome = record_frame_artifact(
                &arena,
                &roots,
                &properties,
                &generations,
                RendererMode::Auto,
            )
            .unwrap();
            let FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility) = outcome else {
                panic!("atomic witness field drift must fail closed: {damage:?}")
            };
            assert!(
                eligibility
                    .reasons
                    .contains(&FrameArtifactFallbackReason::LegacyBoundary(
                        LegacyPaintReason::MissingPreparedInlineRoot
                    )),
                "{damage:?}: {eligibility:?}"
            );
            assert_eq!(take_full_artifact_record_count(), 0, "{damage:?}");
        }
    }

    #[test]
    fn owning_inline_root_requires_exactly_one_live_atomic_package_placement() {
        for (damage, fixture) in [
            (
                OwningInlineIfcRootWitnessDamage::AtomicPackageZeroPlacements,
                prepared_owning_inline_root_with_atomic
                    as fn() -> (NodeArena, Vec<NodeKey>, NodeKey, NodeKey, NodeKey, NodeKey),
            ),
            (
                OwningInlineIfcRootWitnessDamage::AtomicPackageDuplicatePlacements,
                || {
                    let (arena, roots, root) = prepared_owning_inline_root_with_two_atomics();
                    (arena, roots, root, root, root, root)
                },
            ),
        ] {
            let (arena, roots, root, ..) = fixture();
            arena
                .get_mut(root)
                .unwrap()
                .element
                .as_any_mut()
                .downcast_mut::<Element>()
                .unwrap()
                .damage_owning_inline_ifc_root_witness_for_test(damage);
            let (properties, generations) = sync_identity(&arena, &roots);
            take_full_artifact_record_count();
            let outcome = record_frame_artifact(
                &arena,
                &roots,
                &properties,
                &generations,
                RendererMode::Auto,
            )
            .unwrap();
            let FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility) = outcome else {
                panic!("non-exact atomic package cardinality must fail closed: {damage:?}")
            };
            assert!(
                eligibility
                    .reasons
                    .contains(&FrameArtifactFallbackReason::LegacyBoundary(
                        LegacyPaintReason::MissingPreparedInlineRoot
                    )),
                "{damage:?}: {eligibility:?}"
            );
            assert_eq!(take_full_artifact_record_count(), 0, "{damage:?}");
        }
    }

    #[test]
    fn owning_inline_root_rejects_live_atomic_last_placement_drift() {
        let (mut arena, roots, _, _, atomic, _) = prepared_owning_inline_root_with_atomic();
        let mut drifted = arena
            .get(atomic)
            .unwrap()
            .element
            .last_placement()
            .expect("atomic fixture must retain its IFC placement");
        drifted.parent_x += 1.0;
        arena.with_element_taken(atomic, |child, arena| child.place(drifted, arena));
        arena
            .get_mut(atomic)
            .unwrap()
            .element
            .clear_local_dirty_flags(DirtyFlags::ALL);
        arena.clear_arena_dirty_subtree(roots[0], DirtyFlags::ALL);

        let (properties, generations) = sync_identity(&arena, &roots);
        take_full_artifact_record_count();
        let outcome = record_frame_artifact(
            &arena,
            &roots,
            &properties,
            &generations,
            RendererMode::Auto,
        )
        .unwrap();
        let FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility) = outcome else {
            panic!("live atomic placement drift must fail closed")
        };
        assert!(
            eligibility
                .reasons
                .contains(&FrameArtifactFallbackReason::LegacyBoundary(
                    LegacyPaintReason::MissingPreparedInlineRoot
                ))
        );
        assert_eq!(take_full_artifact_record_count(), 0);
    }

    #[test]
    fn owning_inline_root_requires_atomic_subtree_layout_placement_cleanliness() {
        let (arena, roots, root, before, atomic, grandchild, after) =
            prepared_owning_inline_root_with_atomic_subtree();
        let (properties, generations) = sync_identity(&arena, &roots);
        let (artifact, eligibility) =
            whole_frame_artifact(&arena, &roots, &properties, &generations);
        assert!(eligibility.eligible);
        assert_eq!(
            artifact
                .chunks
                .iter()
                .map(|chunk| (chunk.owner, chunk.id.role))
                .collect::<Vec<_>>(),
            vec![
                (root, PaintChunkRole::SelfDecoration),
                (before, PaintChunkRole::TextGlyphs),
                (atomic, PaintChunkRole::SelfDecoration),
                (grandchild, PaintChunkRole::SelfDecoration),
                (after, PaintChunkRole::TextGlyphs),
            ],
            "normal atomic subtrees remain coverage-DOM-DFS recordable"
        );
        let (legacy_arena, legacy_roots, ..) = prepared_owning_inline_root_with_atomic_subtree();
        assert_eq!(
            compiled_whole_frame_graph(&artifact).pass_descriptors(),
            legacy_roots_graph(legacy_arena, &legacy_roots).pass_descriptors()
        );

        arena
            .get_mut(grandchild)
            .unwrap()
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .unwrap()
            .damage_owning_inline_ifc_root_witness_for_test(
                OwningInlineIfcRootWitnessDamage::LayoutDirty,
            );
        let mask = DirtyFlags::LAYOUT.union(DirtyFlags::PLACE);
        assert!(
            !arena
                .get(root)
                .unwrap()
                .element
                .local_dirty_flags()
                .intersects(mask)
        );
        assert!(
            !arena
                .get(atomic)
                .unwrap()
                .element
                .local_dirty_flags()
                .intersects(mask)
        );
        assert_eq!(arena.arena_local_dirty(root), DirtyFlags::NONE);
        assert_eq!(arena.arena_local_dirty(atomic), DirtyFlags::NONE);
        assert_eq!(arena.arena_local_dirty(grandchild), DirtyFlags::NONE);

        let (properties, generations) = sync_identity(&arena, &roots);
        take_full_artifact_record_count();
        let outcome = record_frame_artifact(
            &arena,
            &roots,
            &properties,
            &generations,
            RendererMode::Auto,
        )
        .unwrap();
        let FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility) = outcome else {
            panic!("dirty atomic grandchild must fail closed before recording")
        };
        assert!(
            eligibility
                .reasons
                .contains(&FrameArtifactFallbackReason::LegacyBoundary(
                    LegacyPaintReason::MissingPreparedInlineRoot
                )),
            "{eligibility:?}"
        );
        assert_eq!(take_full_artifact_record_count(), 0);
    }

    #[test]
    fn owning_inline_root_text_live_shift_drift_falls_back_before_full_hooks() {
        let (arena, roots, _, text) = prepared_owning_inline_text_root();
        arena
            .get_mut(text)
            .unwrap()
            .element
            .as_any_mut()
            .downcast_mut::<Text>()
            .unwrap()
            .shift_inline_ifc_owned_geometry(1.0, 0.0);
        let (properties, generations) = sync_identity(&arena, &roots);
        take_full_artifact_record_count();
        let outcome = record_frame_artifact(
            &arena,
            &roots,
            &properties,
            &generations,
            RendererMode::Auto,
        )
        .unwrap();
        let FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility) = outcome else {
            panic!("live Text geometry not bound to the current plan must fail closed")
        };
        assert!(
            eligibility
                .reasons
                .contains(&FrameArtifactFallbackReason::LegacyBoundary(
                    LegacyPaintReason::MissingPreparedInlineRoot
                ))
        );
        assert_eq!(take_full_artifact_record_count(), 0);
    }

    #[test]
    fn owning_inline_root_same_kind_text_plan_swap_falls_back_before_full_hooks() {
        let (arena, roots, root) = prepared_owning_inline_two_text_root();
        arena
            .get_mut(root)
            .unwrap()
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .unwrap()
            .damage_owning_inline_ifc_root_witness_for_test(
                OwningInlineIfcRootWitnessDamage::TextPlanPayloadSwap,
            );
        let (properties, generations) = sync_identity(&arena, &roots);
        take_full_artifact_record_count();
        let outcome = record_frame_artifact(
            &arena,
            &roots,
            &properties,
            &generations,
            RendererMode::Auto,
        )
        .unwrap();
        let FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility) = outcome else {
            panic!("same-kind Text plan payload swap must fail closed")
        };
        assert!(
            eligibility
                .reasons
                .contains(&FrameArtifactFallbackReason::LegacyBoundary(
                    LegacyPaintReason::MissingPreparedInlineRoot
                ))
        );
        assert_eq!(take_full_artifact_record_count(), 0);
    }

    #[test]
    fn fixed_inline_root_with_atomic_records_standard_chunks_and_matches_legacy() {
        let (arena, roots, root, before, atomic, after) = prepared_owning_inline_root_with_atomic();
        let (properties, generations) = sync_identity(&arena, &roots);
        let (artifact, eligibility) =
            whole_frame_artifact(&arena, &roots, &properties, &generations);
        assert!(eligibility.eligible);
        assert_eq!(
            artifact
                .chunks
                .iter()
                .map(|chunk| (chunk.owner, chunk.id.role))
                .collect::<Vec<_>>(),
            vec![
                (root, PaintChunkRole::SelfDecoration),
                (before, PaintChunkRole::TextGlyphs),
                (atomic, PaintChunkRole::SelfDecoration),
                (after, PaintChunkRole::TextGlyphs),
            ],
            "atomic children keep standard coverage chunks in live DOM order"
        );

        let (legacy_arena, legacy_roots, ..) = prepared_owning_inline_root_with_atomic();
        assert_eq!(
            compiled_whole_frame_graph(&artifact).pass_descriptors(),
            legacy_roots_graph(legacy_arena, &legacy_roots).pass_descriptors()
        );
    }

    #[test]
    fn owning_inline_root_atomic_move_and_paint_refresh_preserve_authority_and_order() {
        fn record(
            arena: &NodeArena,
            roots: &[NodeKey],
        ) -> (PaintArtifact, Vec<(NodeKey, PaintChunkRole)>) {
            let (properties, generations) = sync_identity(arena, roots);
            let (artifact, eligibility) =
                whole_frame_artifact(arena, roots, &properties, &generations);
            assert!(eligibility.eligible, "{eligibility:?}");
            let order = artifact
                .chunks
                .iter()
                .map(|chunk| (chunk.owner, chunk.id.role))
                .collect();
            (artifact, order)
        }

        let (mut arena, roots, root, _, atomic, _) = prepared_owning_inline_root_with_atomic();
        let (initial, initial_order) = record(&arena, &roots);
        let initial_root_range = initial
            .chunks
            .iter()
            .find(|chunk| chunk.owner == root)
            .unwrap()
            .op_range
            .clone();

        let mut moved = arena
            .get(root)
            .unwrap()
            .element
            .last_placement()
            .expect("root fixture must retain placement");
        moved.parent_x += 13.0;
        moved.parent_y += 7.0;
        arena.with_element_taken(root, |root, arena| root.place(moved, arena));
        let (after_move, move_order) = record(&arena, &roots);
        assert_eq!(move_order, initial_order);

        let mut node = arena.get_mut(atomic).unwrap();
        let atomic_element = node.element.as_any_mut().downcast_mut::<Element>().unwrap();
        atomic_element.set_background_color_value(Color::rgb(239, 68, 68));
        drop(node);
        let (after_paint, paint_order) = record(&arena, &roots);
        assert_eq!(paint_order, initial_order);
        assert_eq!(
            after_paint
                .chunks
                .iter()
                .find(|chunk| chunk.owner == root)
                .unwrap()
                .op_range,
            initial_root_range,
            "owning root remains SelfDecoration-only after atomic paint refresh"
        );
        assert_eq!(
            after_paint
                .chunks
                .iter()
                .filter(|chunk| chunk.owner == atomic)
                .count(),
            1,
            "atomic paint remains its own standard chunk"
        );
        assert_eq!(
            compiled_whole_frame_graph(&after_move).pass_descriptors(),
            compiled_whole_frame_graph(&after_paint).pass_descriptors()
        );
    }

    #[test]
    fn mixed_wrapping_inline_root_uses_live_dom_dfs_and_matches_legacy() {
        let (arena, roots, root, before, span, nested_text, atomic, after, fragment_count) =
            prepared_mixed_wrapping_inline_root();
        assert!(fragment_count >= 2, "fixture must exercise a wrapped span");
        assert!(
            atomic.data().as_ffi() < before.data().as_ffi(),
            "fixture must allocate the atomic before its earlier DOM sibling"
        );
        let (properties, generations) = sync_identity(&arena, &roots);
        let (artifact, eligibility) =
            whole_frame_artifact(&arena, &roots, &properties, &generations);
        assert!(eligibility.eligible);
        assert_eq!(
            artifact
                .chunks
                .iter()
                .map(|chunk| (chunk.owner, chunk.id.role))
                .collect::<Vec<_>>(),
            vec![
                (root, PaintChunkRole::SelfDecoration),
                (before, PaintChunkRole::TextGlyphs),
                (span, PaintChunkRole::SelfDecoration),
                (nested_text, PaintChunkRole::TextGlyphs),
                (atomic, PaintChunkRole::SelfDecoration),
                (after, PaintChunkRole::TextGlyphs),
            ],
            "coverage DOM DFS alone owns paint order"
        );

        let (legacy_arena, legacy_roots, ..) = prepared_mixed_wrapping_inline_root();
        assert_eq!(
            compiled_whole_frame_graph(&artifact).pass_descriptors(),
            legacy_roots_graph(legacy_arena, &legacy_roots).pass_descriptors()
        );
    }

    #[test]
    fn owning_inline_root_with_image_atomic_keeps_image_chunk_and_matches_legacy() {
        let (arena, roots, root, before, image, after) =
            prepared_owning_inline_root_with_image_atomic();
        let (properties, generations) = sync_identity(&arena, &roots);
        let (artifact, eligibility) =
            whole_frame_artifact(&arena, &roots, &properties, &generations);
        assert!(eligibility.eligible);
        assert_eq!(
            artifact
                .chunks
                .iter()
                .map(|chunk| (chunk.owner, chunk.id.role))
                .collect::<Vec<_>>(),
            vec![
                (root, PaintChunkRole::SelfDecoration),
                (before, PaintChunkRole::TextGlyphs),
                (image, PaintChunkRole::ImageContent),
                (after, PaintChunkRole::TextGlyphs),
            ]
        );
        let (legacy_arena, legacy_roots, ..) = prepared_owning_inline_root_with_image_atomic();
        assert_eq!(
            compiled_whole_frame_graph(&artifact).pass_descriptors(),
            legacy_roots_graph(legacy_arena, &legacy_roots).pass_descriptors()
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn owning_inline_root_with_svg_atomic_keeps_svg_chunk_and_matches_legacy() {
        let (arena, roots, root, before, svg, after) =
            prepared_owning_inline_root_with_svg_atomic();
        let (properties, generations) = sync_identity(&arena, &roots);
        let (artifact, eligibility) =
            whole_frame_artifact(&arena, &roots, &properties, &generations);
        assert!(eligibility.eligible);
        assert_eq!(
            artifact
                .chunks
                .iter()
                .map(|chunk| (chunk.owner, chunk.id.role))
                .collect::<Vec<_>>(),
            vec![
                (root, PaintChunkRole::SelfDecoration),
                (before, PaintChunkRole::TextGlyphs),
                (svg, PaintChunkRole::SvgContent),
                (after, PaintChunkRole::TextGlyphs),
            ]
        );
        let (legacy_arena, legacy_roots, ..) = prepared_owning_inline_root_with_svg_atomic();
        assert_eq!(
            compiled_whole_frame_graph(&artifact).pass_descriptors(),
            legacy_roots_graph(legacy_arena, &legacy_roots).pass_descriptors()
        );
    }

    #[test]
    fn owning_inline_root_atomic_child_participates_in_root_opacity_group_once() {
        fn set_root_opacity(arena: &NodeArena, root: NodeKey) {
            let mut node = arena.get_mut(root).unwrap();
            let element = node.element.as_any_mut().downcast_mut::<Element>().unwrap();
            let mut style = Style::new();
            style.insert(PropertyId::Opacity, ParsedValue::Opacity(Opacity::new(0.5)));
            element.apply_style(style);
            element.clear_local_dirty_flags(DirtyFlags::ALL);
            drop(node);
            arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
        }

        let (arena, roots, root, _, atomic, _) = prepared_owning_inline_root_with_atomic();
        set_root_opacity(&arena, root);
        let (properties, generations) = sync_identity(&arena, &roots);
        let (artifact, eligibility) =
            root_group_artifact(&arena, &roots, &properties, &generations);
        assert!(eligibility.eligible);
        assert!(matches!(
            artifact.target,
            PaintArtifactTarget::RootOpacityGroup { root: owner, .. } if owner == root
        ));
        assert!(artifact.chunks.iter().any(|chunk| {
            chunk.owner == atomic && chunk.id.role == PaintChunkRole::SelfDecoration
        }));
        artifact.ops.iter().for_each(assert_neutral_opacity);

        assert_eq!(
            compiled_whole_frame_graph(&artifact)
                .test_graphics_passes::<crate::view::render_pass::composite_layer_pass::CompositeLayerPass>()
                .len(),
            1
        );
    }

    #[test]
    fn owning_inline_root_with_text_area_atomic_fails_closed_before_full_hooks() {
        let mut arena = new_test_arena();
        let mut root = Element::new_with_id(0x7d20, 0.0, 0.0, 160.0, 40.0);
        let mut root_style = Style::new();
        root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
        root_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(160.0)));
        root_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(40.0)));
        root.apply_style(root_style);
        let root = commit_element(&mut arena, Box::new(root));
        let text_area = commit_child(&mut arena, root, Box::new(TextArea::with_stable_id(0x7d21)));
        let (measure, place) = constraints();
        measure_and_place(&mut arena, root, measure, place);
        for key in [root, text_area] {
            arena
                .get_mut(key)
                .unwrap()
                .element
                .clear_local_dirty_flags(DirtyFlags::ALL);
        }
        arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
        let roots = [root];
        let (properties, generations) = sync_identity(&arena, &roots);
        take_full_artifact_record_count();
        let outcome = record_frame_artifact(
            &arena,
            &roots,
            &properties,
            &generations,
            RendererMode::Auto,
        )
        .unwrap();
        let FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility) = outcome else {
            panic!("TextArea atomic host must remain fail closed before full hooks")
        };
        assert!(
            eligibility
                .reasons
                .contains(&FrameArtifactFallbackReason::LegacyBoundary(
                    LegacyPaintReason::MissingPreparedInlineRoot
                ))
        );
        assert_eq!(take_full_artifact_record_count(), 0);
    }

    #[test]
    fn owning_inline_root_package_drift_falls_back_before_full_hooks() {
        let (arena, roots, _, span, _, _) =
            prepared_owning_wrapping_inline_span_tree_with_opacity(1.0);
        let mut node = arena.get_mut(span).unwrap();
        node.element
            .as_any_mut()
            .downcast_mut::<Element>()
            .unwrap()
            .inline_ifc_decoration_package_for_test()
            .expect("fixture must install a decoration package")
            .fragments
            .clear();
        drop(node);

        let (properties, generations) = sync_identity(&arena, &roots);
        take_full_artifact_record_count();
        let outcome = record_frame_artifact(
            &arena,
            &roots,
            &properties,
            &generations,
            RendererMode::Auto,
        )
        .unwrap();
        let FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility) = outcome else {
            panic!("owning IFC package drift must fail closed")
        };
        assert!(
            eligibility
                .reasons
                .contains(&FrameArtifactFallbackReason::LegacyBoundary(
                    LegacyPaintReason::MissingPreparedInlineRoot
                ))
        );
        assert_eq!(take_full_artifact_record_count(), 0);
    }

    #[test]
    fn owning_inline_root_span_live_shift_drift_falls_back_before_full_hooks() {
        let (arena, roots, _, span, _, _) =
            prepared_owning_wrapping_inline_span_tree_with_opacity(1.0);
        arena
            .get_mut(span)
            .unwrap()
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .unwrap()
            .shift_inline_ifc_owned_geometry(1.0, 0.0);
        let (properties, generations) = sync_identity(&arena, &roots);
        take_full_artifact_record_count();
        let outcome = record_frame_artifact(
            &arena,
            &roots,
            &properties,
            &generations,
            RendererMode::Auto,
        )
        .unwrap();
        let FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility) = outcome else {
            panic!("live Span geometry not bound to the current plan must fail closed")
        };
        assert!(
            eligibility
                .reasons
                .contains(&FrameArtifactFallbackReason::LegacyBoundary(
                    LegacyPaintReason::MissingPreparedInlineRoot
                ))
        );
        assert_eq!(take_full_artifact_record_count(), 0);
    }

    #[test]
    fn owning_inline_root_paint_refresh_preserves_auto_wrap_geometry() {
        fn fragment_geometry(arena: &NodeArena, span: NodeKey) -> Vec<[u32; 4]> {
            arena
                .get(span)
                .unwrap()
                .element
                .as_any()
                .downcast_ref::<Element>()
                .unwrap()
                .inline_fragment_rects()
                .iter()
                .map(|rect| {
                    [
                        rect.x.to_bits(),
                        rect.y.to_bits(),
                        rect.width.to_bits(),
                        rect.height.to_bits(),
                    ]
                })
                .collect()
        }

        fn root_geometry(arena: &NodeArena, root: NodeKey) -> ([u32; 2], [u32; 2]) {
            let node = arena.get(root).unwrap();
            let root = node.element.as_any().downcast_ref::<Element>().unwrap();
            let measured = root.measured_size();
            (
                [measured.0.to_bits(), measured.1.to_bits()],
                [
                    root.inline_ifc_root_build_width_for_test()
                        .expect("fixture must retain an IFC install")
                        .to_bits(),
                    root.inline_ifc_root_applied_width_for_test()
                        .expect("fixture must retain an IFC install")
                        .to_bits(),
                ],
            )
        }

        let (mut arena, roots, root, span, _, _) =
            prepared_owning_wrapping_inline_span_tree_with_opacity(1.0);
        let before = fragment_geometry(&arena, span);
        let root_before = root_geometry(&arena, root);
        assert!(before.len() >= 2, "fixture must wrap before paint damage");

        arena
            .get_mut(span)
            .unwrap()
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .unwrap()
            .set_background_color_value(Color::rgb(22, 163, 74));
        let (measure, place) = wrapping_inline_span_constraints();
        measure_and_place(&mut arena, root, measure, place);

        assert_eq!(
            fragment_geometry(&arena, span),
            before,
            "paint-only same-constraints refresh must retain the original shaping width"
        );
        assert_eq!(
            root_geometry(&arena, root),
            root_before,
            "paint-only same-constraints refresh must preserve root size and build authority"
        );
        let (properties, generations) = sync_identity(&arena, &roots);
        let (artifact, eligibility) =
            whole_frame_artifact(&arena, &roots, &properties, &generations);
        assert!(eligibility.eligible);
        let refreshed = artifact
            .ops
            .iter()
            .find_map(|op| match op {
                PaintOp::PreparedInlineIfcDecoration(fragment) => Some(fragment),
                _ => None,
            })
            .expect("refreshed span must emit inline decoration ops");
        assert_eq!(
            refreshed.fill.fill_color.map(f32::to_bits),
            Color::rgb(22, 163, 74).to_rgba_f32().map(f32::to_bits)
        );
    }

    #[test]
    fn owning_inline_root_move_and_paint_preserve_dual_width_authority() {
        fn fragments(arena: &NodeArena, span: NodeKey) -> Vec<[f32; 4]> {
            arena
                .get(span)
                .unwrap()
                .element
                .as_any()
                .downcast_ref::<Element>()
                .unwrap()
                .inline_fragment_rects()
                .iter()
                .map(|rect| [rect.x, rect.y, rect.width, rect.height])
                .collect()
        }
        fn widths(arena: &NodeArena, root: NodeKey) -> [u32; 2] {
            let node = arena.get(root).unwrap();
            let root = node.element.as_any().downcast_ref::<Element>().unwrap();
            [
                root.inline_ifc_root_build_width_for_test()
                    .unwrap()
                    .to_bits(),
                root.inline_ifc_root_applied_width_for_test()
                    .unwrap()
                    .to_bits(),
            ]
        }

        let (mut arena, roots, root, span, _, _) =
            prepared_owning_wrapping_inline_span_tree_with_opacity(1.0);
        let before_fragments = fragments(&arena, span);
        let before_widths = widths(&arena, root);
        arena
            .get_mut(span)
            .unwrap()
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .unwrap()
            .set_background_color_value(Color::rgb(22, 163, 74));

        let (measure, mut place) = wrapping_inline_span_constraints();
        place.parent_x = 7.0;
        place.parent_y = 11.0;
        measure_and_place(&mut arena, root, measure, place);

        assert_eq!(widths(&arena, root), before_widths);
        assert_eq!(
            fragments(&arena, span),
            before_fragments
                .iter()
                .map(|rect| [rect[0] + 7.0, rect[1] + 11.0, rect[2], rect[3]])
                .collect::<Vec<_>>()
        );
        let (properties, generations) = sync_identity(&arena, &roots);
        let (artifact, eligibility) =
            whole_frame_artifact(&arena, &roots, &properties, &generations);
        assert!(eligibility.eligible);
        let refreshed = artifact
            .ops
            .iter()
            .find_map(|op| match op {
                PaintOp::PreparedInlineIfcDecoration(fragment) => Some(fragment),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            refreshed.fill.fill_color.map(f32::to_bits),
            Color::rgb(22, 163, 74).to_rgba_f32().map(f32::to_bits)
        );
    }

    #[test]
    fn owning_inline_root_assigned_width_change_reshapes_current_install() {
        fn state(arena: &NodeArena, root: NodeKey, text: NodeKey) -> ([u32; 2], [u32; 4]) {
            let node = arena.get(root).unwrap();
            let root = node.element.as_any().downcast_ref::<Element>().unwrap();
            let widths = [
                root.inline_ifc_root_build_width_for_test()
                    .unwrap()
                    .to_bits(),
                root.inline_ifc_root_applied_width_for_test()
                    .unwrap()
                    .to_bits(),
            ];
            let bounds = arena
                .get(text)
                .unwrap()
                .element
                .as_any()
                .downcast_ref::<Text>()
                .unwrap()
                .inline_ifc_owned_paint_geometry_for_test()
                .unwrap()
                .0;
            (
                widths,
                [
                    bounds.x.to_bits(),
                    bounds.y.to_bits(),
                    bounds.width.to_bits(),
                    bounds.height.to_bits(),
                ],
            )
        }

        let (mut arena, _roots, root, text) = prepared_percent_owning_inline_text_root();
        let before = state(&arena, root, text);
        let (_, mut place) = constraints();
        place.available_width = 240.0;
        place.percent_base_width = Some(480.0);
        arena.with_element_taken(root, |element, _arena| {
            element.set_layout_width(240.0);
        });
        arena.refresh_subtree_dirty_cache(root);
        arena.with_element_taken(root, |element, arena| {
            element.place(place, arena);
        });
        let after = state(&arena, root, text);

        assert_ne!(after.0[1], before.0[1], "assigned width must change");
        assert_eq!(
            after.0[0], after.0[1],
            "a real assigned-width change establishes a new build authority"
        );
        assert_ne!(
            after.1, before.1,
            "the assigned width change must reshape text geometry"
        );
    }

    #[test]
    fn owning_inline_root_opacity_group_neutralizes_root_span_and_text_once() {
        let (arena, roots, root, span, text, _) =
            prepared_owning_wrapping_inline_span_tree_with_opacity(0.5);
        let (properties, generations) = sync_identity(&arena, &roots);
        let (artifact, eligibility) =
            root_group_artifact(&arena, &roots, &properties, &generations);
        assert!(eligibility.eligible);
        assert!(matches!(
            artifact.target,
            PaintArtifactTarget::RootOpacityGroup { root: target, .. } if target == root
        ));
        assert_eq!(
            artifact
                .chunks
                .iter()
                .map(|chunk| chunk.owner)
                .collect::<Vec<_>>(),
            vec![root, span, text]
        );
        artifact.ops.iter().for_each(assert_neutral_opacity);
        assert_eq!(
            compiled_whole_frame_graph(&artifact)
                .test_graphics_passes::<crate::view::render_pass::composite_layer_pass::CompositeLayerPass>()
                .len(),
            1
        );
    }

    #[test]
    fn plain_text_area_records_one_contents_glyph_chunk_and_transparent_runs() {
        let (arena, roots, root) = prepared_plain_text_area_tree(
            "plain TextArea wraps across a deliberately narrow viewport",
        );
        let (properties, generations) = sync_identity(&arena, &roots);
        take_full_artifact_record_count();
        let (artifact, eligibility) =
            whole_frame_artifact(&arena, &roots, &properties, &generations);
        assert!(eligibility.eligible);
        assert_eq!(artifact.chunks.len(), 1);
        assert_eq!(artifact.chunks[0].owner, root);
        assert_eq!(artifact.chunks[0].id.scope, PaintPropertyScope::Contents);
        assert_eq!(artifact.chunks[0].id.phase, PaintNodePhase::BeforeChildren);
        assert_eq!(artifact.chunks[0].id.slot, 1);
        assert_eq!(artifact.chunks[0].id.role, PaintChunkRole::TextGlyphs);
        assert_eq!(artifact.ops.len(), 1, "children must not duplicate glyphs");
        let PaintOp::PreparedText(op) = &artifact.ops[0] else {
            panic!("plain TextArea must freeze a prepared text op")
        };
        assert_eq!(op.params.scissor_rect, None);
        assert_eq!(op.params.stencil_clip_id, None);
        assert_eq!(take_full_artifact_record_count(), 1);

        let state = properties.node_state_for(root).unwrap();
        assert_eq!(artifact.chunks[0].properties, state.descendants);
        assert_ne!(state.paint.clip, state.descendants.clip);
        let graph = compiled_whole_frame_graph(&artifact);
        let passes = graph
            .test_graphics_passes::<crate::view::render_pass::text_pass::TextPreparedInputPass>();
        assert_eq!(passes.len(), 1);
        assert!(
            passes[0]
                .test_snapshot()
                .pass_context
                .scissor_rect
                .is_some(),
            "ContentsClip must reach the pass while the prepared op keeps scissor None"
        );
    }

    #[test]
    fn plain_text_area_selection_orders_underlay_before_slot_one_glyphs() {
        let record = |anchor, focus| {
            let (arena, roots, root) = prepared_plain_text_area_selection_tree(
                "forward and reverse selection",
                108.0,
                anchor,
                focus,
            );
            let (properties, generations) = sync_identity(&arena, &roots);
            let (artifact, eligibility) =
                whole_frame_artifact(&arena, &roots, &properties, &generations);
            assert!(eligibility.eligible);
            assert_eq!(
                artifact
                    .chunks
                    .iter()
                    .map(|chunk| (chunk.id.role, chunk.id.slot, chunk.id.scope))
                    .collect::<Vec<_>>(),
                vec![
                    (
                        PaintChunkRole::SelectionUnderlay,
                        0,
                        PaintPropertyScope::Contents,
                    ),
                    (PaintChunkRole::TextGlyphs, 1, PaintPropertyScope::Contents,),
                ]
            );
            assert!(artifact.chunks[0].op_range.len() >= 1);
            assert_eq!(artifact.chunks[1].op_range.len(), 1);
            assert_eq!(
                artifact
                    .ops
                    .iter()
                    .filter(|op| matches!(op, PaintOp::PreparedText(_)))
                    .count(),
                1,
                "Run children must not duplicate glyphs"
            );
            assert!(artifact.ops[..artifact.chunks[0].op_range.end]
                .iter()
                .all(|op| matches!(op, PaintOp::DrawRect(rect) if rect.mode == crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly)));
            assert_eq!(
                artifact.chunks[0].properties,
                properties.node_state_for(root).unwrap().descendants
            );
            (artifact, root)
        };

        let (forward, _) = record(2, 19);
        let (reverse, _) = record(19, 2);
        assert_eq!(
            forward.chunks[0].payload_identity, reverse.chunks[0].payload_identity,
            "selection direction must not perturb ordered geometry"
        );
    }

    #[test]
    fn focused_plain_text_area_records_contents_caret_after_children_and_matches_legacy() {
        let focused_fixture = |content: &str, selection: Option<(usize, usize)>| {
            let (arena, roots, root) = prepared_plain_text_area_tree(content);
            {
                let mut node = arena.get_mut(root).unwrap();
                let text_area = node
                    .element
                    .as_any_mut()
                    .downcast_mut::<TextArea>()
                    .unwrap();
                text_area.is_focused = true;
                text_area.caret_visible = true;
                text_area.caret_blink_epoch = None;
                if let Some((anchor, focus)) = selection {
                    text_area.selection_anchor_char = Some(anchor);
                    text_area.selection_focus_char = Some(focus);
                }
            }
            settle_plain_text_area(&arena, root);
            (arena, roots, root)
        };

        for selection in [None, Some((1, 8))] {
            let (arena, roots, root) = focused_fixture("focused caret artifact", selection);
            let (properties, generations) = sync_identity(&arena, &roots);
            let (artifact, eligibility) =
                whole_frame_artifact(&arena, &roots, &properties, &generations);
            assert!(eligibility.eligible);
            let expected_roles = if selection.is_some() {
                vec![
                    PaintChunkRole::SelectionUnderlay,
                    PaintChunkRole::TextGlyphs,
                    PaintChunkRole::Caret,
                ]
            } else {
                vec![PaintChunkRole::TextGlyphs, PaintChunkRole::Caret]
            };
            assert_eq!(
                artifact
                    .chunks
                    .iter()
                    .map(|chunk| chunk.id.role)
                    .collect::<Vec<_>>(),
                expected_roles
            );
            let caret = artifact.chunks.last().unwrap();
            assert_eq!(caret.owner, root);
            assert_eq!(caret.id.scope, PaintPropertyScope::Contents);
            assert_eq!(caret.id.phase, PaintNodePhase::AfterChildren);
            assert_eq!(caret.id.slot, 1);
            assert_eq!(caret.op_range.len(), 1);
            let PaintOp::DrawRect(caret_op) = &artifact.ops[caret.op_range.clone()][0] else {
                panic!("caret must freeze one draw rect")
            };
            assert_eq!(
                caret_op.mode,
                crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly
            );
            assert_eq!(caret_op.params.size[0].to_bits(), 1.0_f32.to_bits());
            assert!(caret_op.params.size[1] >= 1.0);
            assert_eq!(caret_op.params.opacity.to_bits(), 1.0_f32.to_bits());
            assert_eq!(
                caret.properties,
                properties.node_state_for(root).unwrap().descendants
            );

            if selection.is_some() {
                let mut graph = compiled_whole_frame_graph(&artifact);
                let snapshot = graph.test_compile_snapshot().unwrap();
                let payloads = snapshot.pass_payloads();
                assert!(
                    matches!(payloads, [
                        FramePassTestPayload::Clear(_),
                        FramePassTestPayload::DrawRect(selection),
                        FramePassTestPayload::PreparedText(glyphs),
                        FramePassTestPayload::DrawRect(caret),
                    ] if selection.mode == crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly
                        && caret.mode == crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly
                        && selection.effective_scissor_rect == caret.effective_scissor_rect
                        && glyphs.pass_context.scissor_rect == caret.effective_scissor_rect
                        && glyphs.pass_context.stencil_clip_id == caret.pass_context.stencil_clip_id),
                    "selection, glyphs, children boundary, and caret must compile in phased order with one Contents clip/stencil authority: {payloads:?}"
                );
            }
        }

        assert_whole_frame_structural_parity(
            || {
                let (arena, roots, _) = focused_fixture("focused caret parity", Some((1, 7)));
                (arena, roots)
            },
            PaintParityConfig::default(),
        );
    }

    #[test]
    fn empty_focused_plain_text_area_is_caret_only_and_contents_clip_can_cull_it() {
        let make = |empty_viewport: bool| {
            let (arena, roots, root) = prepared_plain_text_area_tree("");
            {
                let mut node = arena.get_mut(root).unwrap();
                let text_area = node
                    .element
                    .as_any_mut()
                    .downcast_mut::<TextArea>()
                    .unwrap();
                text_area.is_focused = true;
                text_area.caret_visible = true;
                text_area.caret_blink_epoch = None;
                if empty_viewport {
                    text_area.viewport_size.height = 0.0;
                }
            }
            settle_plain_text_area(&arena, root);
            (arena, roots, root)
        };

        let (arena, roots, root) = make(false);
        let (properties, generations) = sync_identity(&arena, &roots);
        let (artifact, eligibility) =
            whole_frame_artifact(&arena, &roots, &properties, &generations);
        assert!(eligibility.eligible);
        assert_eq!(artifact.chunks.len(), 1);
        assert_eq!(artifact.chunks[0].id.role, PaintChunkRole::Caret);
        assert_eq!(artifact.chunks[0].id.phase, PaintNodePhase::AfterChildren);
        assert_eq!(artifact.ops.len(), 1);
        assert!(arena.children_of(root).is_empty());
        assert_eq!(
            compiled_whole_frame_graph(&artifact)
                .test_rect_pass_snapshots()
                .len(),
            1
        );

        let (arena, roots, _) = make(true);
        let (properties, generations) = sync_identity(&arena, &roots);
        let (artifact, eligibility) =
            whole_frame_artifact(&arena, &roots, &properties, &generations);
        assert!(eligibility.eligible);
        assert_eq!(artifact.chunks.len(), 1);
        assert!(
            artifact
                .clip_nodes
                .iter()
                .any(|clip| clip.logical_scissor[2] == 0 || clip.logical_scissor[3] == 0)
        );
        assert!(
            compiled_whole_frame_graph(&artifact)
                .test_rect_pass_snapshots()
                .is_empty()
        );
    }

    #[test]
    fn retained_caret_phase_flip_changes_only_self_paint_generation() {
        let (mut arena, roots, root) = prepared_plain_text_area_tree("generation caret");
        let t0 = crate::time::Instant::now();
        {
            let mut node = arena.get_mut(root).unwrap();
            let text_area = node
                .element
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .unwrap();
            text_area.is_focused = true;
            text_area.caret_visible = true;
            text_area.caret_blink_epoch = Some(t0);
        }
        settle_plain_text_area(&arena, root);

        let mut properties = PropertyTrees::default();
        properties.sync(&arena, &roots);
        let mut generations = PaintGenerationTracker::default();
        generations.sync(&arena, &roots, &properties);
        let initial = generations.snapshot(root).unwrap();

        assert!(!crate::view::base_component::tick_animation_frames(
            &mut arena,
            &roots,
            t0 + crate::time::Duration::from_millis(529),
        ));
        assert!(
            arena
                .get(root)
                .unwrap()
                .element
                .local_dirty_flags()
                .is_empty()
        );
        properties.sync(&arena, &roots);
        generations.sync(&arena, &roots, &properties);
        assert_eq!(generations.snapshot(root).unwrap(), initial);

        assert!(crate::view::base_component::tick_animation_frames(
            &mut arena,
            &roots,
            t0 + crate::time::Duration::from_millis(530),
        ));
        let dirty = arena.get(root).unwrap().element.local_dirty_flags();
        assert_eq!(dirty, DirtyFlags::PAINT);
        assert_eq!(arena.arena_local_dirty(root), DirtyFlags::PAINT);
        properties.sync(&arena, &roots);
        generations.sync(&arena, &roots, &properties);
        let flipped = generations.snapshot(root).unwrap();
        assert_ne!(flipped.self_paint_revision, initial.self_paint_revision);
        assert_eq!(flipped.composite_revision, initial.composite_revision);
        assert_eq!(flipped.topology_revision, initial.topology_revision);
    }

    #[test]
    fn retained_caret_metadata_full_visibility_drift_is_not_canonical() {
        let (arena, roots, root) = prepared_plain_text_area_tree("caret drift");
        {
            let mut node = arena.get_mut(root).unwrap();
            let text_area = node
                .element
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .unwrap();
            text_area.is_focused = true;
            text_area.caret_visible = true;
        }
        settle_plain_text_area(&arena, root);
        let (properties, generations) = sync_identity(&arena, &roots);
        let metadata = record_coverage_manifest(
            &arena,
            &roots,
            false,
            true,
            CoverageRecordingMode::MetadataOnly,
            &properties,
            &generations,
        );
        arena
            .get_mut(root)
            .unwrap()
            .element
            .as_any_mut()
            .downcast_mut::<TextArea>()
            .unwrap()
            .caret_visible = false;
        let full = record_coverage_manifest(
            &arena,
            &roots,
            false,
            true,
            CoverageRecordingMode::FullArtifact,
            &properties,
            &generations,
        );
        assert!(!super::frame_recorder::canonical_manifest_matches(
            &metadata, &full
        ));
    }

    #[test]
    fn plain_caret_artifact_honours_soft_wrap_affinity_and_matches_legacy() {
        fn fixture(upstream: bool) -> (NodeArena, Vec<NodeKey>, NodeKey) {
            let content = "甲乙丙丁戊己庚辛壬癸子丑寅卯辰巳午未申酉戌亥";
            let (arena, roots, root) =
                prepared_plain_text_area_tree_with(content, "", 80.0, [7.25, 11.75]);
            let boundary = {
                let node = arena.get(root).unwrap();
                let text_area = node.element.as_any().downcast_ref::<TextArea>().unwrap();
                (0..=content.chars().count())
                    .find(|&char_index| {
                        let upstream =
                            crate::view::base_component::text_area::caret_map_probe_with_affinity(
                                text_area, &arena, char_index, true,
                            );
                        let downstream =
                            crate::view::base_component::text_area::caret_map_probe_with_affinity(
                                text_area, &arena, char_index, false,
                            );
                        upstream
                            .zip(downstream)
                            .is_some_and(|(up, down)| (up.2 - down.2).abs() > 0.5)
                    })
                    .expect("narrow fixture must expose a soft-wrap affinity boundary")
            };
            {
                let mut node = arena.get_mut(root).unwrap();
                let text_area = node
                    .element
                    .as_any_mut()
                    .downcast_mut::<TextArea>()
                    .unwrap();
                text_area.cursor_char = boundary;
                crate::view::base_component::text_area::set_caret_affinity_probe(
                    text_area, upstream,
                );
                text_area.is_focused = true;
                text_area.caret_visible = true;
                text_area.caret_blink_epoch = None;
            }
            settle_plain_text_area(&arena, root);
            (arena, roots, root)
        }

        let caret_position = |upstream| {
            let (arena, roots, _) = fixture(upstream);
            let (properties, generations) = sync_identity(&arena, &roots);
            let (artifact, eligibility) =
                whole_frame_artifact(&arena, &roots, &properties, &generations);
            assert!(eligibility.eligible);
            let caret = artifact
                .chunks
                .iter()
                .find(|chunk| chunk.id.role == PaintChunkRole::Caret)
                .unwrap();
            let PaintOp::DrawRect(op) = &artifact.ops[caret.op_range.clone()][0] else {
                panic!("caret artifact must contain one rect")
            };
            op.params.position
        };
        let upstream = caret_position(true);
        let downstream = caret_position(false);
        assert!(
            upstream[1] < downstream[1],
            "upstream caret must remain on the upper visual line: up={upstream:?}, down={downstream:?}"
        );

        for upstream in [true, false] {
            assert_whole_frame_structural_parity(
                || {
                    let (arena, roots, _) = fixture(upstream);
                    (arena, roots)
                },
                PaintParityConfig::default(),
            );
        }
    }

    #[test]
    fn plain_text_area_selection_multiline_wrapped_and_clamped_cases_match_legacy() {
        for (content, width, anchor, focus) in [
            ("first line\nsecond line", 108.0, 2, 19),
            (
                "selection wraps across multiple visual lines in a narrow viewport",
                64.0,
                3,
                54,
            ),
            ("clamp this selection", 108.0, 0, usize::MAX),
            ("aé中🙂z", 108.0, 1, 4),
        ] {
            assert_whole_frame_structural_parity(
                || {
                    let (arena, roots, _) =
                        prepared_plain_text_area_selection_tree(content, width, anchor, focus);
                    (arena, roots)
                },
                PaintParityConfig::default(),
            );

            let (arena, roots, _) =
                prepared_plain_text_area_selection_tree(content, width, anchor, focus);
            let (properties, generations) = sync_identity(&arena, &roots);
            let (artifact, eligibility) =
                whole_frame_artifact(&arena, &roots, &properties, &generations);
            assert!(eligibility.eligible);
            assert_eq!(
                artifact.chunks[0].id.role,
                PaintChunkRole::SelectionUnderlay
            );
            if content.contains('\n') || width < 100.0 {
                assert!(artifact.chunks[0].op_range.len() >= 2);
            }
        }

        for (anchor, focus) in [(3, 3), (usize::MAX, usize::MAX)] {
            let (arena, roots, _) = prepared_plain_text_area_selection_tree(
                "collapsed selection",
                108.0,
                anchor,
                focus,
            );
            let (properties, generations) = sync_identity(&arena, &roots);
            let (artifact, eligibility) =
                whole_frame_artifact(&arena, &roots, &properties, &generations);
            assert!(eligibility.eligible);
            assert_eq!(artifact.chunks.len(), 1);
            assert_eq!(artifact.chunks[0].id.role, PaintChunkRole::TextGlyphs);
            assert_eq!(artifact.chunks[0].id.slot, 1);
        }
    }

    #[test]
    fn plain_text_area_selection_contents_clip_handles_explicit_empty_viewport() {
        let (arena, roots, root) =
            prepared_plain_text_area_selection_tree("clipped selection", 108.0, 0, 7);
        arena
            .get_mut(root)
            .unwrap()
            .element
            .as_any_mut()
            .downcast_mut::<TextArea>()
            .unwrap()
            .viewport_size
            .height = 0.0;
        let (properties, generations) = sync_identity(&arena, &roots);
        let (artifact, eligibility) =
            whole_frame_artifact(&arena, &roots, &properties, &generations);
        assert!(eligibility.eligible);
        assert_eq!(artifact.chunks.len(), 2);
        assert!(
            artifact
                .clip_nodes
                .iter()
                .any(|clip| clip.logical_scissor[2] == 0 || clip.logical_scissor[3] == 0),
            "clips={:?}",
            artifact.clip_nodes
        );
        let graph = compiled_whole_frame_graph(&artifact);
        assert!(graph.test_rect_pass_snapshots().is_empty());
        assert!(
            graph
                .test_graphics_passes::<crate::view::render_pass::text_pass::TextPreparedInputPass>(
                )
                .is_empty()
        );
    }

    #[test]
    fn plain_text_area_selection_metadata_and_full_hooks_reread_live_color_and_range() {
        let (arena, roots, root) =
            prepared_plain_text_area_selection_tree("metadata drift", 108.0, 1, 10);
        let (properties, generations) = sync_identity(&arena, &roots);
        let state = properties.node_state_for(root).unwrap();
        let generation = generations.local_generations_for(root).unwrap();
        let revision = PaintContentRevision {
            self_paint_revision: generation.self_paint_revision,
            composite_revision: generation.composite_revision,
            topology_revision: generation.topology_revision,
        };
        let metadata = arena
            .get(root)
            .unwrap()
            .element
            .record_shadow_paint_metadata_plan(
                root,
                state.paint,
                state.descendants,
                revision,
                &arena,
                PaintRecordingContext::default(),
            )
            .unwrap();
        assert_eq!(metadata.before_children.len(), 2);
        let old_selection_identity = metadata.before_children[0].payload_identity.clone();

        {
            let mut node = arena.get_mut(root).unwrap();
            node.element
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .unwrap()
                .selection_background_color = Color::rgba(240, 32, 80, 128);
        }
        let full = arena
            .get(root)
            .unwrap()
            .element
            .record_shadow_paint_artifact_plan(
                root,
                state.paint,
                state.descendants,
                revision,
                &arena,
                PaintRecordingContext::default(),
            )
            .unwrap();
        assert_eq!(full.before_children.len(), 2);
        assert_ne!(
            old_selection_identity, full.before_children[0].chunks[0].payload_identity,
            "full recording must freeze the live selection color"
        );

        {
            let mut node = arena.get_mut(root).unwrap();
            let text_area = node
                .element
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .unwrap();
            text_area.selection_anchor_char = Some(4);
            text_area.selection_focus_char = Some(4);
        }
        let collapsed = arena
            .get(root)
            .unwrap()
            .element
            .record_shadow_paint_metadata_plan(
                root,
                state.paint,
                state.descendants,
                revision,
                &arena,
                PaintRecordingContext::default(),
            )
            .unwrap();
        assert_eq!(collapsed.before_children.len(), 1);
        assert_eq!(
            collapsed.before_children[0].id.role,
            PaintChunkRole::TextGlyphs
        );
        assert_eq!(collapsed.before_children[0].id.slot, 1);
    }

    #[test]
    fn plain_text_area_preedit_variants_emit_exact_decoration_and_match_legacy() {
        for (content, width, cursor_char, preedit, preedit_cursor) in [
            ("abcdef", 108.0, 3, "中🙂", None),
            ("abcdef", 108.0, 2, "中🙂", Some((0, "中".len()))),
            ("abcdef", 108.0, 2, "中🙂", Some((0, 1))),
            ("", 108.0, 0, "入力", Some((0, usize::MAX))),
            ("first\nsecond", 108.0, 5, "長い入力", None),
            (
                "preedit wraps across several visual lines in a narrow viewport",
                64.0,
                9,
                "composition",
                Some((0, 6)),
            ),
        ] {
            assert_whole_frame_structural_parity(
                || {
                    let (arena, roots, _) = prepared_plain_text_area_preedit_tree(
                        content,
                        width,
                        cursor_char,
                        preedit,
                        preedit_cursor,
                    );
                    (arena, roots)
                },
                PaintParityConfig::default(),
            );

            let (arena, roots, root) = prepared_plain_text_area_preedit_tree(
                content,
                width,
                cursor_char,
                preedit,
                preedit_cursor,
            );
            let (properties, generations) = sync_identity(&arena, &roots);
            let (artifact, eligibility) =
                whole_frame_artifact(&arena, &roots, &properties, &generations);
            assert!(eligibility.eligible);
            assert_eq!(
                artifact
                    .chunks
                    .iter()
                    .map(|chunk| (chunk.id.phase, chunk.id.slot, chunk.id.role))
                    .collect::<Vec<_>>(),
                vec![
                    (
                        PaintNodePhase::BeforeChildren,
                        1,
                        PaintChunkRole::TextGlyphs
                    ),
                    (
                        PaintNodePhase::AfterChildren,
                        0,
                        PaintChunkRole::TextDecoration,
                    ),
                    (PaintNodePhase::AfterChildren, 1, PaintChunkRole::Caret),
                ]
            );
            let decoration = artifact
                .chunks
                .iter()
                .find(|chunk| chunk.id.role == PaintChunkRole::TextDecoration)
                .unwrap();
            assert!(!decoration.op_range.is_empty());
            for op in &artifact.ops[decoration.op_range.clone()] {
                let PaintOp::DrawRect(op) = op else {
                    panic!("preedit decoration must contain only rect ops")
                };
                assert_eq!(
                    op.mode,
                    crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly
                );
                assert_eq!(op.params.size[1].to_bits(), 1.0_f32.to_bits());
                assert!(op.params.size[0] >= 1.0);
                assert_eq!(op.params.opacity.to_bits(), 1.0_f32.to_bits());
            }
            let transient_runs = arena
                .children_of(root)
                .into_iter()
                .filter(|&key| {
                    arena
                        .get(key)
                        .unwrap()
                        .element
                        .as_any()
                        .downcast_ref::<TextAreaTextRun>()
                        .is_some_and(|run| run.is_preedit_run())
                })
                .count();
            assert_eq!(transient_runs, 1);
        }
    }

    #[test]
    fn plain_text_area_preedit_selection_glyph_underline_caret_order_and_clip_are_exact() {
        let make = |empty_viewport: bool| {
            let (arena, roots, root) = prepared_plain_text_area_preedit_tree(
                "selection composition",
                108.0,
                9,
                "中",
                Some((0, 3)),
            );
            {
                let mut node = arena.get_mut(root).unwrap();
                let text_area = node
                    .element
                    .as_any_mut()
                    .downcast_mut::<TextArea>()
                    .unwrap();
                text_area.selection_anchor_char = Some(0);
                text_area.selection_focus_char = Some(4);
                if empty_viewport {
                    text_area.viewport_size.height = 0.0;
                }
            }
            (arena, roots, root)
        };

        let (arena, roots, root) = make(false);
        let (properties, generations) = sync_identity(&arena, &roots);
        let (artifact, eligibility) =
            whole_frame_artifact(&arena, &roots, &properties, &generations);
        assert!(eligibility.eligible);
        assert_eq!(
            artifact
                .chunks
                .iter()
                .map(|chunk| (chunk.id.phase, chunk.id.slot, chunk.id.role))
                .collect::<Vec<_>>(),
            vec![
                (
                    PaintNodePhase::BeforeChildren,
                    0,
                    PaintChunkRole::SelectionUnderlay,
                ),
                (
                    PaintNodePhase::BeforeChildren,
                    1,
                    PaintChunkRole::TextGlyphs
                ),
                (
                    PaintNodePhase::AfterChildren,
                    0,
                    PaintChunkRole::TextDecoration,
                ),
                (PaintNodePhase::AfterChildren, 1, PaintChunkRole::Caret),
            ]
        );
        assert!(artifact.chunks.iter().all(|chunk| {
            chunk.id.owner == root
                && chunk.id.scope == PaintPropertyScope::Contents
                && chunk.properties == properties.node_state_for(root).unwrap().descendants
        }));
        let mut graph = compiled_whole_frame_graph(&artifact);
        let snapshot = graph.test_compile_snapshot().unwrap();
        let payloads = snapshot.pass_payloads();
        assert!(
            matches!(
                payloads,
                [
                    FramePassTestPayload::Clear(_),
                    FramePassTestPayload::DrawRect(selection),
                    FramePassTestPayload::PreparedText(glyphs),
                    FramePassTestPayload::DrawRect(underline),
                    FramePassTestPayload::DrawRect(caret),
                ] if selection.mode == crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly
                    && underline.mode == crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly
                    && caret.mode == crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly
                    && selection.effective_scissor_rect == underline.effective_scissor_rect
                    && underline.effective_scissor_rect == caret.effective_scissor_rect
                    && glyphs.pass_context.scissor_rect == caret.effective_scissor_rect
                    && glyphs.pass_context.stencil_clip_id == caret.pass_context.stencil_clip_id
            ),
            "payloads={payloads:?}"
        );

        let (arena, roots, _) = make(true);
        let (properties, generations) = sync_identity(&arena, &roots);
        let (artifact, eligibility) =
            whole_frame_artifact(&arena, &roots, &properties, &generations);
        assert!(eligibility.eligible);
        assert_eq!(artifact.chunks.len(), 4);
        let graph = compiled_whole_frame_graph(&artifact);
        assert!(graph.test_rect_pass_snapshots().is_empty());
        assert!(
            graph
                .test_graphics_passes::<crate::view::render_pass::text_pass::TextPreparedInputPass>(
                )
                .is_empty()
        );
    }

    #[test]
    fn plain_text_area_bounded_baked_scroll_is_canonical_and_matches_legacy() {
        let fixture = || {
            let (mut arena, roots, root) = prepared_plain_text_area_preedit_tree(
                "selection composition stays aligned while the viewport scrolls",
                108.0,
                9,
                "中",
                Some((0, 3)),
            );
            {
                let mut node = arena.get_mut(root).unwrap();
                let text_area = node
                    .element
                    .as_any_mut()
                    .downcast_mut::<TextArea>()
                    .unwrap();
                text_area.selection_anchor_char = Some(0);
                text_area.selection_focus_char = Some(4);
            }
            place_text_area_with_baked_scroll(&mut arena, root, 108.0, 28.0, [0.0, 9.0]);
            (arena, roots)
        };

        assert_whole_frame_structural_parity(fixture, PaintParityConfig::default());

        let (arena, roots) = fixture();
        let root = roots[0];
        let (properties, generations) = sync_identity(&arena, &roots);
        assert!(
            properties.scrolls.is_empty(),
            "TextArea scroll stays baked into paint"
        );
        let root_node = arena.get(root).unwrap();
        let text_area = root_node
            .element
            .as_any()
            .downcast_ref::<TextArea>()
            .unwrap();
        assert!(!text_area.retained_paint_properties().is_scroll_container);
        assert_eq!(
            text_area.shadow_paint_recording_capability(
                &arena,
                false,
                PaintRecordingContext::default(),
            ),
            ShadowPaintRecordingCapability::Recordable
        );
        drop(root_node);

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
        assert!(super::frame_recorder::canonical_manifest_matches(
            &metadata, &full
        ));

        let (artifact, eligibility) =
            whole_frame_artifact(&arena, &roots, &properties, &generations);
        assert!(eligibility.eligible);
        assert_eq!(
            artifact
                .chunks
                .iter()
                .map(|chunk| chunk.id.role)
                .collect::<Vec<_>>(),
            vec![
                PaintChunkRole::SelectionUnderlay,
                PaintChunkRole::TextGlyphs,
                PaintChunkRole::TextDecoration,
                PaintChunkRole::Caret,
            ]
        );
        assert!(artifact.chunks.iter().all(|chunk| {
            chunk.id.scope == PaintPropertyScope::Contents
                && chunk.properties == properties.node_state_for(root).unwrap().descendants
        }));
    }

    #[test]
    fn text_area_baked_scroll_changes_self_paint_revision_only_after_exact_replacement() {
        let (mut arena, roots, root) = prepared_plain_text_area_tree(
            "paint identity must change when a bounded internal scroll changes",
        );
        let mut properties = PropertyTrees::default();
        properties.sync(&arena, &roots);
        let mut generations = PaintGenerationTracker::default();
        generations.sync(&arena, &roots, &properties);
        let before = generations.snapshot(root).unwrap();

        place_text_area_with_baked_scroll(&mut arena, root, 108.0, 28.0, [0.0, 7.0]);
        properties.sync(&arena, &roots);
        generations.sync(&arena, &roots, &properties);
        let scrolled = generations.snapshot(root).unwrap();
        assert_ne!(scrolled.self_paint_revision, before.self_paint_revision);
        assert!(properties.scrolls.is_empty());

        properties.sync(&arena, &roots);
        generations.sync(&arena, &roots, &properties);
        assert_eq!(
            generations.snapshot(root).unwrap().self_paint_revision,
            scrolled.self_paint_revision,
            "unchanged baked scroll must keep a deterministic paint identity"
        );
    }

    #[test]
    fn plain_text_area_preedit_tampered_state_run_and_package_fail_before_full_hooks() {
        for case in [
            "ime",
            "run_text",
            "run_range",
            "run_cursor",
            "missing_run",
            "duplicate_run",
            "backing_range",
            "preedit_range",
            "caret_byte",
            "source",
        ] {
            let (arena, roots, root) =
                prepared_plain_text_area_preedit_tree("abcdef", 108.0, 3, "中🙂", Some((0, 3)));
            let (preedit_index, preedit_key) = arena
                .children_of(root)
                .into_iter()
                .enumerate()
                .find(|(_, key)| {
                    arena
                        .get(*key)
                        .unwrap()
                        .element
                        .as_any()
                        .downcast_ref::<TextAreaTextRun>()
                        .is_some_and(|run| run.is_preedit_run())
                })
                .expect("fixture must contain one transient preedit Run");
            match case {
                "ime" => {
                    arena
                        .get_mut(root)
                        .unwrap()
                        .element
                        .as_any_mut()
                        .downcast_mut::<TextArea>()
                        .unwrap()
                        .ime_preedit
                        .push('!');
                }
                "run_text" => {
                    arena
                        .get_mut(preedit_key)
                        .unwrap()
                        .element
                        .as_any_mut()
                        .downcast_mut::<TextAreaTextRun>()
                        .unwrap()
                        .text
                        .push('!');
                }
                "run_range" => {
                    arena
                        .get_mut(preedit_key)
                        .unwrap()
                        .element
                        .as_any_mut()
                        .downcast_mut::<TextAreaTextRun>()
                        .unwrap()
                        .char_range = 2..2;
                }
                "run_cursor" => {
                    arena
                        .get_mut(preedit_key)
                        .unwrap()
                        .element
                        .as_any_mut()
                        .downcast_mut::<TextAreaTextRun>()
                        .unwrap()
                        .preedit_cursor = None;
                }
                "missing_run" => {
                    arena
                        .get_mut(preedit_key)
                        .unwrap()
                        .element
                        .as_any_mut()
                        .downcast_mut::<TextAreaTextRun>()
                        .unwrap()
                        .is_preedit_run = false;
                }
                "duplicate_run" => {
                    let other = arena
                        .children_of(root)
                        .into_iter()
                        .find(|key| *key != preedit_key)
                        .unwrap();
                    arena
                        .get_mut(other)
                        .unwrap()
                        .element
                        .as_any_mut()
                        .downcast_mut::<TextAreaTextRun>()
                        .unwrap()
                        .is_preedit_run = true;
                }
                "backing_range" => {
                    arena
                        .get(root)
                        .unwrap()
                        .element
                        .as_any()
                        .downcast_ref::<TextArea>()
                        .unwrap()
                        .tamper_cached_unified_segment_backing_range_for_test(preedit_index, 1..1);
                }
                "preedit_range" => {
                    arena
                        .get(root)
                        .unwrap()
                        .element
                        .as_any()
                        .downcast_ref::<TextArea>()
                        .unwrap()
                        .tamper_cached_unified_segment_preedit_range_for_test(preedit_index, None);
                }
                "caret_byte" => {
                    arena
                        .get(root)
                        .unwrap()
                        .element
                        .as_any()
                        .downcast_ref::<TextArea>()
                        .unwrap()
                        .tamper_cached_unified_segment_preedit_caret_for_test(
                            preedit_index,
                            Some(usize::MAX),
                        );
                }
                "source" => {
                    arena
                        .get(root)
                        .unwrap()
                        .element
                        .as_any()
                        .downcast_ref::<TextArea>()
                        .unwrap()
                        .tamper_cached_unified_segment_source_for_test(preedit_index);
                }
                _ => unreachable!(),
            }
            let eligibility = assert_text_area_fallback_before_full(&arena, &roots);
            assert!(!eligibility.eligible, "{case}");
        }
    }

    #[test]
    fn plain_text_area_preedit_commit_and_cancel_return_to_plain_slice() {
        let relayout = |arena: &mut NodeArena, root: NodeKey| {
            let measure = LayoutConstraints {
                max_width: 108.0,
                max_height: 240.0,
                viewport_width: 320.0,
                viewport_height: 240.0,
                percent_base_width: Some(320.0),
                percent_base_height: Some(240.0),
            };
            let place = LayoutPlacement {
                parent_x: 7.25,
                parent_y: 11.75,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: 108.0,
                available_height: 240.0,
                viewport_width: 320.0,
                viewport_height: 240.0,
                percent_base_width: Some(320.0),
                percent_base_height: Some(240.0),
            };
            measure_and_place(arena, root, measure, place);
            arena
                .get_mut(root)
                .unwrap()
                .element
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .unwrap()
                .pending_caret_scroll = false;
            settle_plain_text_area(arena, root);
        };

        for commit in [false, true] {
            let (mut arena, roots, root) =
                prepared_plain_text_area_preedit_tree("abcd", 108.0, 2, "中", Some((0, 3)));
            arena.with_element_taken(root, |element, _arena| {
                let text_area = element.as_any_mut().downcast_mut::<TextArea>().unwrap();
                if commit {
                    assert!(text_area.commit_preedit_for_paint_test());
                } else {
                    assert!(text_area.clear_preedit_for_paint_test());
                }
            });
            relayout(&mut arena, root);

            let root_node = arena.get(root).unwrap();
            let text_area = root_node
                .element
                .as_any()
                .downcast_ref::<TextArea>()
                .unwrap();
            assert!(text_area.ime_preedit.is_empty());
            assert_eq!(text_area.ime_preedit_cursor, None);
            assert_eq!(text_area.content, if commit { "ab中cd" } else { "abcd" });
            drop(root_node);
            assert!(arena.children_of(root).into_iter().all(|key| {
                arena
                    .get(key)
                    .unwrap()
                    .element
                    .as_any()
                    .downcast_ref::<TextAreaTextRun>()
                    .is_none_or(|run| !run.is_preedit_run())
            }));

            let (properties, generations) = sync_identity(&arena, &roots);
            let (artifact, eligibility) =
                whole_frame_artifact(&arena, &roots, &properties, &generations);
            assert!(eligibility.eligible);
            assert!(
                artifact
                    .chunks
                    .iter()
                    .all(|chunk| chunk.id.role != PaintChunkRole::TextDecoration)
            );
        }
    }

    #[test]
    fn plain_text_area_preedit_metadata_full_drift_and_boundaries_fail_closed() {
        let (arena, roots, root) =
            prepared_plain_text_area_preedit_tree("drift", 108.0, 2, "中", Some((0, 3)));
        let (properties, generations) = sync_identity(&arena, &roots);
        let metadata = record_coverage_manifest(
            &arena,
            &roots,
            false,
            true,
            CoverageRecordingMode::MetadataOnly,
            &properties,
            &generations,
        );
        arena
            .get_mut(root)
            .unwrap()
            .element
            .as_any_mut()
            .downcast_mut::<TextArea>()
            .unwrap()
            .ime_preedit
            .push('!');
        let full = record_coverage_manifest(
            &arena,
            &roots,
            false,
            true,
            CoverageRecordingMode::FullArtifact,
            &properties,
            &generations,
        );
        assert!(!super::frame_recorder::canonical_manifest_matches(
            &metadata, &full
        ));

        let (arena, roots, root) =
            prepared_plain_text_area_preedit_tree("scroll", 108.0, 2, "中", None);
        arena
            .get_mut(root)
            .unwrap()
            .element
            .as_any_mut()
            .downcast_mut::<TextArea>()
            .unwrap()
            .scroll_y = 1.0;
        assert_text_area_fallback_before_full(&arena, &roots);

        let (arena, _roots, root) =
            prepared_plain_text_area_preedit_tree("deferred", 108.0, 2, "中", None);
        let node = arena.get(root).unwrap();
        let text_area = node.element.as_any().downcast_ref::<TextArea>().unwrap();
        assert_eq!(
            text_area.shadow_paint_recording_capability(
                &arena,
                true,
                PaintRecordingContext::default(),
            ),
            ShadowPaintRecordingCapability::Legacy(ShadowPaintBlocker::Deferred)
        );
        drop(node);
    }

    #[test]
    fn text_area_projection_atomic_wrapper_is_transparent_and_matches_legacy() {
        assert_whole_frame_structural_parity(
            || {
                let (arena, roots, ..) = prepared_projection_text_area_tree();
                (arena, roots)
            },
            PaintParityConfig::default(),
        );

        let (arena, roots, root, projection, projected_text) = prepared_projection_text_area_tree();
        let projection_node = arena.get(projection).unwrap();
        assert_eq!(
            projection_node.element.shadow_paint_recording_capability(
                &arena,
                false,
                PaintRecordingContext {
                    inside_text_area: true,
                    ..Default::default()
                },
            ),
            ShadowPaintRecordingCapability::Transparent
        );
        drop(projection_node);

        let (properties, generations) = sync_identity(&arena, &roots);
        let (artifact, eligibility) =
            whole_frame_artifact(&arena, &roots, &properties, &generations);
        assert!(eligibility.eligible);
        assert_eq!(
            artifact
                .chunks
                .iter()
                .map(|chunk| (chunk.owner, chunk.id.role))
                .collect::<Vec<_>>(),
            vec![
                (root, PaintChunkRole::TextGlyphs),
                (projected_text, PaintChunkRole::TextGlyphs),
            ]
        );
        assert!(
            artifact
                .owner_nodes
                .iter()
                .any(|snapshot| { snapshot.owner == projection && snapshot.parent == Some(root) })
        );
        assert!(
            artifact
                .owner_nodes
                .iter()
                .any(|snapshot| { snapshot.owner == projected_text && snapshot.parent.is_some() })
        );
        assert_eq!(
            artifact.chunks[1].properties,
            properties.node_state_for(projected_text).unwrap().paint
        );
        assert_eq!(
            artifact.chunks[1].properties.clip,
            properties.node_state_for(root).unwrap().descendants.clip
        );
    }

    #[test]
    fn atomic_projection_emission_constructor_requires_the_full_canonical_stamp() {
        let (plan, stamp) =
            atomic_projection_emission_fixture_for_test("projected", 0xc3a_4100).unwrap();
        assert!(
            super::compiler::prepare_validated_scroll_scene_atomic_projection_text_area_emission(
                plan.clone(),
                &stamp,
            )
            .is_some()
        );

        let mut drifted = stamp;
        drifted.chunks[0].bounds_bits[0] ^= 1;
        assert!(
            super::compiler::prepare_validated_scroll_scene_atomic_projection_text_area_emission(
                plan, &drifted,
            )
            .is_none()
        );
    }

    #[test]
    fn atomic_projection_reraster_emission_consumes_host_content_overlay_in_order() {
        let (plan, stamp) =
            atomic_projection_emission_fixture_for_test("projected", 0xc3a_4101).unwrap();
        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
        let target = ctx.allocate_target(&mut graph);
        ctx.set_current_target(target);
        take_artifact_compile_count();

        let host =
            super::compiler::prepare_validated_scroll_scene_atomic_projection_text_area_emission(
                plan, &stamp,
            )
            .unwrap();
        let content = super::compiler::emit_validated_scroll_scene_atomic_projection_text_area_host(
            host, &mut graph, &mut ctx,
        );
        let overlay =
            super::compiler::emit_validated_scroll_scene_atomic_projection_text_area_content(
                content, &mut graph, &mut ctx,
            );
        super::compiler::emit_validated_scroll_scene_atomic_projection_text_area_overlay(
            overlay, &mut graph, &mut ctx,
        );

        assert_eq!(take_artifact_compile_count(), 3);
    }

    #[test]
    fn atomic_projection_reuse_emission_skips_only_detached_content_compile() {
        let (plan, stamp) =
            atomic_projection_emission_fixture_for_test("projected", 0xc3a_4102).unwrap();
        let mut graph = FrameGraph::new();
        let mut ctx = UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
        let target = ctx.allocate_target(&mut graph);
        ctx.set_current_target(target);
        take_artifact_compile_count();

        let host =
            super::compiler::prepare_validated_scroll_scene_atomic_projection_text_area_emission(
                plan, &stamp,
            )
            .unwrap();
        let content = super::compiler::emit_validated_scroll_scene_atomic_projection_text_area_host(
            host, &mut graph, &mut ctx,
        );
        let overlay =
            super::compiler::reuse_validated_scroll_scene_atomic_projection_text_area_content(
                content,
            );
        super::compiler::emit_validated_scroll_scene_atomic_projection_text_area_overlay(
            overlay, &mut graph, &mut ctx,
        );

        assert_eq!(take_artifact_compile_count(), 2);
    }

    #[test]
    fn focused_atomic_projection_element_admission_is_graph_inert_and_exact() {
        for caret_visible in [true, false] {
            let (arena, root, wrapper, text_area) = prepared_atomic_projection_scroll_shell();
            {
                let mut node = arena.get_mut(text_area).unwrap();
                let text_area = node
                    .element
                    .as_any_mut()
                    .downcast_mut::<TextArea>()
                    .unwrap();
                text_area.is_focused = true;
                text_area.caret_visible = caret_visible;
                text_area.cursor_char = 7;
            }
            let root_node = arena.get(root).unwrap();
            let root_element = root_node
                .element
                .as_any()
                .downcast_ref::<Element>()
                .unwrap();
            let admission = root_element
                .exact_retained_scroll_focused_atomic_projection_text_area_subtree_admission(
                    root, &arena, 1.0,
                )
                .expect("focused atomic projection source must admit without planning");
            assert_eq!(
                (admission.content_wrapper, admission.text_area_root),
                (wrapper, text_area),
            );
            assert!(admission.paint_grammar.is_canonical());
            assert!(admission.bitwise_eq(&admission.clone()));
            assert_eq!(admission.paint_grammar.caret.caret_visible, caret_visible);
            assert!(matches!(
                (&admission.paint_grammar.caret.paint, caret_visible),
                (
                    crate::view::base_component::text_area::FocusedAtomicCaretSourcePaintSeal::Present { .. },
                    true,
                ) | (
                    crate::view::base_component::text_area::FocusedAtomicCaretSourcePaintSeal::Hidden,
                    false,
                )
            ));
            assert!(
                root_element
                    .exact_retained_scroll_atomic_projection_text_area_subtree_admission(
                        root, &arena, 1.0,
                    )
                    .is_none(),
                "existing non-focused atomic admission must remain closed",
            );
            assert!(
                root_element
                    .exact_retained_scroll_interactive_text_area_subtree_admission(
                        root, &arena, 1.0,
                    )
                    .is_none(),
                "generated-run interactive admission must remain projection-free",
            );
            drop(root_node);
            let (properties, _) = sync_identity(&arena, &[root]);
            let scroll = properties
                .scroll_snapshot_for(crate::view::compositor::property_tree::ScrollNodeId(root))
                .unwrap();
            assert!(admission.matches_scroll_node(scroll));
        }
    }

    #[test]
    fn focused_atomic_projection_local_recorder_suppresses_caret_into_post_fact() {
        for caret_visible in [true, false] {
            let (arena, root, wrapper, text_area) = prepared_atomic_projection_scroll_shell();
            {
                let mut node = arena.get_mut(text_area).unwrap();
                let text_area = node
                    .element
                    .as_any_mut()
                    .downcast_mut::<TextArea>()
                    .unwrap();
                text_area.is_focused = true;
                text_area.caret_visible = caret_visible;
                text_area.cursor_char = 7;
            }
            let root_node = arena.get(root).unwrap();
            let root_element = root_node
                .element
                .as_any()
                .downcast_ref::<Element>()
                .unwrap();
            let admission = root_element
                .exact_retained_scroll_focused_atomic_projection_text_area_subtree_admission(
                    root, &arena, 1.0,
                )
                .expect("focused atomic projection source must admit");
            drop(root_node);

            let (properties, generations) = sync_identity(&arena, &[root]);
            let scroll_id = crate::view::compositor::property_tree::ScrollNodeId(root);
            let scroll = properties.scroll_snapshot_for(scroll_id).unwrap();
            let outer_clip_id = ClipNodeId {
                owner: root,
                role: ClipNodeRole::ContentsClip,
            };
            let clip_chain = properties.clip_snapshot_for(Some(outer_clip_id)).unwrap();
            let outer_clip = *clip_chain.last().unwrap();
            let outer = PaintScrollContentWitness::new(root, wrapper, scroll, outer_clip).unwrap();
            let local = super::frame_recorder::record_scroll_focused_atomic_projection_text_area_subtree_local_artifact_for_plan(
                &arena,
                &properties,
                &generations,
                &admission,
                outer,
            )
            .expect("focused atomic projection local recorder");

            assert!(local.is_canonical_for_test());
            assert_eq!(local.caret_for_test().caret_visible, caret_visible);
            assert_eq!(
                local
                    .artifact_for_test()
                    .chunks
                    .iter()
                    .map(|chunk| chunk.id.role)
                    .collect::<Vec<_>>(),
                vec![
                    PaintChunkRole::SelfDecoration,
                    PaintChunkRole::TextGlyphs,
                    PaintChunkRole::TextGlyphs,
                ],
                "caret must stay out of the resident local artifact",
            );
            assert!(
                !local
                    .artifact_for_test()
                    .chunks
                    .iter()
                    .any(|chunk| chunk.id.role == PaintChunkRole::Caret)
            );
        }
    }

    #[test]
    fn focused_atomic_projection_host_local_plan_keeps_caret_out_of_resident() {
        for (caret_visible, cursor_char) in [(true, 0), (true, 7), (false, 7)] {
            let (arena, root, wrapper, text_area) = prepared_atomic_projection_scroll_shell();
            {
                let mut node = arena.get_mut(text_area).unwrap();
                let text_area = node
                    .element
                    .as_any_mut()
                    .downcast_mut::<TextArea>()
                    .unwrap();
                text_area.is_focused = true;
                text_area.caret_visible = caret_visible;
                text_area.cursor_char = cursor_char;
            }
            let root_node = arena.get(root).unwrap();
            let root_element = root_node
                .element
                .as_any()
                .downcast_ref::<Element>()
                .unwrap();
            let admission = root_element
                .exact_retained_scroll_focused_atomic_projection_text_area_subtree_admission(
                    root, &arena, 1.0,
                )
                .expect("focused source admission");
            assert!(
                root_element
                    .exact_retained_scroll_atomic_projection_text_area_subtree_admission(
                        root, &arena, 1.0,
                    )
                    .is_none(),
                "focused path must not widen unfocused C3a admission",
            );
            drop(root_node);

            let (properties, generations) = sync_identity(&arena, &[root]);
            let scroll = properties
                .scroll_snapshot_for(crate::view::compositor::property_tree::ScrollNodeId(root))
                .unwrap();
            let outer_clip = *properties
                .clip_snapshot_for(Some(ClipNodeId {
                    owner: root,
                    role: ClipNodeRole::ContentsClip,
                }))
                .unwrap()
                .last()
                .unwrap();
            let outer = PaintScrollContentWitness::new(root, wrapper, scroll, outer_clip).unwrap();
            let baked = PaintBakedScrollHostWitness::new(root, wrapper, scroll, outer_clip.id)
                .expect("baked scroll host");
            let host = super::frame_recorder::record_baked_scroll_focused_atomic_projection_text_area_subtree_host_artifact_for_plan(
                &arena,
                &[root],
                &properties,
                &generations,
                &admission,
                baked,
            )
            .expect("focused host recorder");
            let local = super::frame_recorder::record_scroll_focused_atomic_projection_text_area_subtree_local_artifact_for_plan(
                &arena,
                &properties,
                &generations,
                &admission,
                outer,
            )
            .expect("focused local recorder");
            let plan =
                super::frame_recorder::validate_recorded_focused_atomic_projection_text_area_plan_parts(
                    host, local,
                )
                .expect("focused plan parts");

            assert!(plan.is_canonical());
            assert_eq!(plan.caret_for_test().caret_visible, caret_visible);
            assert_eq!(
                plan.resident_for_test().source_grammar,
                admission.paint_grammar.atomic_source,
                "resident stamp must carry only the base atomic glyph grammar",
            );
        }
    }

    #[test]
    fn focused_atomic_projection_scroll_scene_plan_is_canonical_and_live_exact() {
        for (caret_visible, cursor_char) in [(true, 0), (true, 7), (false, 7)] {
            let (arena, root, _, text_area) = prepared_atomic_projection_scroll_shell();
            {
                let mut node = arena.get_mut(text_area).unwrap();
                let text_area = node
                    .element
                    .as_any_mut()
                    .downcast_mut::<TextArea>()
                    .unwrap();
                text_area.is_focused = true;
                text_area.caret_visible = caret_visible;
                text_area.cursor_char = cursor_char;
            }
            let (properties, generations) = sync_identity(&arena, &[root]);
            let budget =
                super::scroll_scene::ScrollSceneSingleTextureBudget::new(8192, 128 * 1024 * 1024)
                    .unwrap();
            let sampled_at = crate::time::Instant::now();
            let plan = super::scroll_scene::plan_property_scroll_scene_scaffold(
                &arena,
                &[root],
                &properties,
                &generations,
                1.0,
                [0.0; 2],
                None,
                sampled_at,
                wgpu::TextureFormat::Bgra8UnormSrgb,
                budget,
            )
            .expect("focused atomic projection shell must plan as a property-scroll scene");

            assert!(plan.is_canonical());
            assert!(plan.matches_live_inputs(
                &arena,
                &[root],
                &properties,
                &generations,
                sampled_at,
            ));
        }
    }

    #[test]
    fn atomic_projection_text_area_graph_inert_record_and_validator_are_fail_closed() {
        let (arena, root, wrapper, text_area) = prepared_atomic_projection_scroll_shell();
        let root_node = arena.get(root).unwrap();
        let root_element = root_node
            .element
            .as_any()
            .downcast_ref::<Element>()
            .unwrap();
        let text_node = arena.get(text_area).unwrap();
        let text_component = text_node
            .element
            .as_any()
            .downcast_ref::<TextArea>()
            .unwrap();
        assert!(
            text_component
                .exact_retained_property_scroll_atomic_projection_subtree(
                    text_area,
                    &arena,
                    [0.0, -20.0],
                )
                .is_some(),
            "source oracle must remain exact after shell placement",
        );
        drop(text_node);
        let admission = root_element
            .exact_retained_scroll_atomic_projection_text_area_subtree_admission(root, &arena, 1.0)
            .expect("atomic projection shell must admit");
        drop(root_node);
        assert_eq!(
            (admission.content_wrapper, admission.text_area_root),
            (wrapper, text_area)
        );
        let (properties, generations) = sync_identity(&arena, &[root]);
        let scroll_id = crate::view::compositor::property_tree::ScrollNodeId(root);
        let scroll = properties.scroll_snapshot_for(scroll_id).unwrap();
        let outer_clip_id = ClipNodeId {
            owner: root,
            role: ClipNodeRole::ContentsClip,
        };
        let clip_chain = properties.clip_snapshot_for(Some(outer_clip_id)).unwrap();
        let outer_clip = *clip_chain.last().unwrap();
        let outer = PaintScrollContentWitness::new(root, wrapper, scroll, outer_clip).unwrap();
        let baked = PaintBakedScrollHostWitness::new(root, wrapper, scroll, outer_clip.id).unwrap();

        let local = super::frame_recorder::record_scroll_atomic_projection_text_area_subtree_local_artifact_for_plan(
            &arena,
            &properties,
            &generations,
            &admission,
            outer,
        ).expect("closed local recorder");
        assert_eq!(local.artifact_for_test().chunks.len(), 3);
        let host = super::frame_recorder::record_baked_scroll_atomic_projection_text_area_subtree_host_artifact_for_plan(
            &arena,
            &[root],
            &properties,
            &generations,
            &admission,
            baked,
        ).expect("closed host recorder");
        assert_eq!(host.chunk_count_for_test(), 5);
        assert!(host.is_canonical_for_test());
        let plan_parts =
            super::frame_recorder::validate_recorded_atomic_projection_text_area_plan_parts(
                host.clone(),
                local.clone(),
            )
            .expect("typed host/local bridge must seal one atomic plan authority");
        assert!(plan_parts.is_canonical());
        assert_eq!(plan_parts.chunk_counts_for_test(), (1, 3, 1));
        assert!(plan_parts.same_authority(&plan_parts.clone()));
        assert_eq!(plan_parts.identity(), plan_parts.clone().identity());
        assert_eq!(plan_parts.local_clip_snapshots().unwrap().len(), 1);
        let content_terminal = plan_parts.content_opaque_order_count().unwrap();
        assert!(
            plan_parts
                .content_artifact_span_stamp(0, 0..content_terminal)
                .is_some()
        );
        assert!(
            !plan_parts
                .clone()
                .tamper_content_bounds_for_test()
                .is_canonical()
        );
        assert!(
            !plan_parts
                .clone()
                .tamper_content_resolved_clips_for_test()
                .is_canonical()
        );
        assert!(!plan_parts.clone().tamper_resident_for_test().is_canonical());
        for tampered_host in [
            host.clone().tamper_cross_parity_bounds_for_test(0),
            host.clone().tamper_cross_parity_bounds_for_test(1),
            host.clone().tamper_cross_parity_bounds_for_test(4),
            host.clone().tamper_cross_parity_payload_for_test(2, 3),
            host.clone().tamper_cross_parity_order_for_test(1, 2),
        ] {
            assert!(
                super::frame_recorder::validate_recorded_atomic_projection_text_area_plan_parts(
                    tampered_host,
                    local.clone(),
                )
                .is_none(),
                "synchronized host tamper must reach and fail the bridge parity gate",
            );
        }
        assert!(
            super::frame_recorder::validate_recorded_atomic_projection_text_area_plan_parts(
                host.clone().tamper_cross_parity_bounds_for_test(1),
                local.clone().tamper_cross_parity_bounds_for_test(0),
            )
            .is_none(),
            "synchronized host/local wrapper drift must fail independent scroll geometry",
        );
        let tampered_host = host.clone().tamper_artifact_for_test(|artifact| {
            artifact.chunks[0].bounds.x += 1.0;
        });
        assert!(!tampered_host.is_canonical_for_test());
        let tampered_host_payload = host.clone().tamper_artifact_for_test(|artifact| {
            let projection_op = artifact.ops[artifact.chunks[3].op_range.start].clone();
            let projection_payload = artifact.chunks[3].payload_identity.clone();
            artifact.ops[artifact.chunks[2].op_range.start] = projection_op;
            artifact.chunks[2].payload_identity = projection_payload;
        });
        assert!(!tampered_host_payload.is_canonical_for_test());
        let tampered_host_clip = host.clone().tamper_artifact_for_test(|artifact| {
            artifact.clip_nodes[1].logical_scissor[0] ^= 1;
        });
        assert!(!tampered_host_clip.is_canonical_for_test());
        let mut drifted_admission = admission.clone();
        drifted_admission.paint_grammar.projection_text_stable_id ^= 1;
        assert!(
            super::frame_recorder::record_scroll_atomic_projection_text_area_subtree_local_artifact_for_plan(
                &arena,
                &properties,
                &generations,
                &drifted_admission,
                outer,
            )
            .is_err(),
            "source/admission drift must fail before recording",
        );
        let validate = |recorded| {
            super::frame_recorder::validate_recorded_atomic_projection_text_area_subtree(recorded)
        };
        let validated = validate(local.clone()).expect("dedicated compiler validator");
        assert!(validated.resident_for_test().is_canonical());

        let mut resident_bounds = validated.resident_for_test().clone();
        resident_bounds.text_area_glyph_chunk.bounds_bits[0] ^= 1;
        assert!(!resident_bounds.is_canonical());
        let mut resident_payload = validated.resident_for_test().clone();
        resident_payload.text_area_glyph_chunk.payload_identity = resident_payload
            .projection_glyph_chunk
            .payload_identity
            .clone();
        assert!(!resident_payload.is_canonical());
        let mut resident_clip = validated.resident_for_test().clone();
        resident_clip.contents_clip.logical_scissor[0] ^= 1;
        assert!(!resident_clip.is_canonical());

        let mut cases = Vec::new();
        cases.push(
            local
                .clone()
                .tamper_artifact_for_test(|artifact| artifact.chunks.swap(1, 2)),
        );
        cases.push(local.clone().tamper_artifact_for_test(|artifact| {
            artifact.owner_nodes[2].parent = Some(wrapper);
        }));
        cases.push(local.clone().tamper_artifact_for_test(|artifact| {
            artifact.chunks[2].owner = text_area;
        }));
        cases.push(local.clone().tamper_artifact_for_test(|artifact| {
            artifact.chunks[2].id.slot = 0;
        }));
        cases.push(local.clone().tamper_artifact_for_test(|artifact| {
            artifact.chunks[2].id.role = PaintChunkRole::SelfDecoration;
        }));
        cases.push(local.clone().tamper_artifact_for_test(|artifact| {
            artifact.clip_nodes[0].logical_scissor[0] ^= 1;
        }));
        cases.push(local.clone().tamper_artifact_for_test(|artifact| {
            artifact.chunks[2].payload_identity = PaintPayloadIdentity::None;
        }));
        cases.push(local.clone().tamper_artifact_for_test(|artifact| {
            artifact.chunks[1].bounds.x += 1.0;
        }));
        cases.push(local.clone().tamper_artifact_for_test(|artifact| {
            artifact.chunks[2].bounds.x += 1.0;
        }));
        cases.push(local.clone().tamper_artifact_for_test(|artifact| {
            let projection_op = artifact.ops[artifact.chunks[2].op_range.start].clone();
            let projection_payload = artifact.chunks[2].payload_identity.clone();
            artifact.ops[artifact.chunks[1].op_range.start] = projection_op;
            artifact.chunks[1].payload_identity = projection_payload;
        }));
        cases.push(local.clone().tamper_artifact_for_test(|artifact| {
            let root_op = artifact.ops[artifact.chunks[1].op_range.start].clone();
            let root_payload = artifact.chunks[1].payload_identity.clone();
            artifact.ops[artifact.chunks[2].op_range.start] = root_op;
            artifact.chunks[2].payload_identity = root_payload;
        }));
        cases.push(local.clone().tamper_artifact_for_test(|artifact| {
            artifact.ops.push(artifact.ops[0].clone());
        }));
        cases.push(local.clone().tamper_artifact_for_test(|artifact| {
            artifact.effect_nodes.push(EffectNodeSnapshot {
                id: EffectNodeId(text_area),
                owner: text_area,
                parent: None,
                opacity: 1.0,
                generation: 1,
            });
        }));
        let extra_chunk = local.artifact_for_test().chunks[2].clone();
        cases.push(local.clone().tamper_artifact_for_test(|artifact| {
            artifact.chunks.push(extra_chunk);
        }));
        for (index, recorded) in cases.into_iter().enumerate() {
            assert!(
                validate(recorded).is_none(),
                "tamper case {index} must fail closed",
            );
        }

        let budget =
            super::scroll_scene::ScrollSceneSingleTextureBudget::new(8192, 128 * 1024 * 1024)
                .unwrap();
        let scene = super::scroll_scene::plan_property_scroll_scene_scaffold(
            &arena,
            &[root],
            &properties,
            &generations,
            1.0,
            [0.0; 2],
            None,
            crate::time::Instant::now(),
            wgpu::TextureFormat::Bgra8UnormSrgb,
            budget,
        )
        .expect("atomic selector must produce one graph-inert property-scroll plan");
        assert!(scene.is_canonical());
        assert!(scene.atomic_projection_contract_for_test());
        assert!(scene.atomic_projection_tamper_matrix_for_test());
        let validated_scene = super::scroll_scene::plan_and_validate_property_scroll_scene(
            &arena,
            &[root],
            &properties,
            &generations,
            1.0,
            [0.0; 2],
            None,
            crate::time::Instant::now(),
            wgpu::TextureFormat::Bgra8UnormSrgb,
            budget,
        )
        .expect("atomic selector must compiler-seal one graph-inert boundary");
        assert!(validated_scene.is_canonical());
        assert!(
            validated_scene.atomic_projection_prepare_and_collision_are_atomic_for_test(),
            "C3a atomic prepare must succeed exactly once and reject collisions without local declarations",
        );
        let mut viewport = crate::view::viewport::Viewport::new();
        let frame_owner = viewport
            .begin_retained_surface_frame_stage()
            .expect("fresh viewport must admit one retained frame owner");
        let pool_before = viewport.retained_surface_transaction_shape_for_test();
        let mut graph = FrameGraph::new();
        let graph_before = graph.build_state_snapshot_for_test();
        let prepare = super::scroll_scene::prepare_retained_property_scroll_forest_from_pool(
            &mut viewport,
            validated_scene,
            &mut graph,
            UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0),
            [0.0, 0.0, 0.0, 1.0],
            frame_owner,
        );
        let prepared = prepare.expect("C3a atomic authority must prepare without graph mutation");
        drop(prepared);
        assert_eq!(graph.build_state_snapshot_for_test(), graph_before);
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            pool_before,
        );
        assert!(viewport.retained_surface_frame_stage_owner_is_active(frame_owner));
    }

    #[test]
    fn atomic_projection_selection_record_consume_is_typed_and_fail_closed() {
        let (arena, root, wrapper, text_area) = prepared_atomic_projection_scroll_shell();
        {
            let mut node = arena.get_mut(text_area).unwrap();
            let text_area = node
                .element
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .unwrap();
            text_area.selection_anchor_char = Some(0);
            text_area.selection_focus_char = Some(6);
        }
        let root_node = arena.get(root).unwrap();
        let root_element = root_node
            .element
            .as_any()
            .downcast_ref::<Element>()
            .unwrap();
        let admission = root_element
            .exact_retained_scroll_atomic_projection_selection_text_area_subtree_admission(
                root, &arena, 1.0,
            )
            .expect("disjoint root selection plus one projection must admit");
        assert!(admission.paint_grammar.is_canonical());
        assert!(admission.bitwise_eq(&admission.clone()));
        assert!(
            root_element
                .exact_retained_scroll_atomic_projection_text_area_subtree_admission(
                    root, &arena, 1.0,
                )
                .is_none(),
            "existing atomic glyph selector must remain selection-free",
        );
        assert!(
            root_element
                .exact_retained_scroll_text_area_subtree_admission(root, &arena, 1.0)
                .is_none(),
            "C1/C2 selector must remain projection-free",
        );
        drop(root_node);
        let (properties, generations) = sync_identity(&arena, &[root]);
        let scroll = properties
            .scroll_snapshot_for(crate::view::compositor::property_tree::ScrollNodeId(root))
            .unwrap();
        let outer_clip = *properties
            .clip_snapshot_for(Some(ClipNodeId {
                owner: root,
                role: ClipNodeRole::ContentsClip,
            }))
            .unwrap()
            .last()
            .unwrap();
        let outer = PaintScrollContentWitness::new(root, wrapper, scroll, outer_clip).unwrap();
        let baked = PaintBakedScrollHostWitness::new(root, wrapper, scroll, outer_clip.id).unwrap();
        let local = super::frame_recorder::record_scroll_atomic_projection_selection_text_area_subtree_local_artifact_for_plan(
            &arena,
            &properties,
            &generations,
            &admission,
            outer,
        )
        .expect("typed four-chunk local recording");
        let host = super::frame_recorder::record_baked_scroll_atomic_projection_selection_text_area_subtree_host_artifact_for_plan(
            &arena,
            &[root],
            &properties,
            &generations,
            &admission,
            baked,
        )
        .expect("typed H/content/O recording");
        assert_eq!(host.chunk_count_for_test(), 6);
        assert_eq!(local.chunk_count_for_test(), 4);
        assert!(host.is_canonical_for_test());
        assert!(local.is_canonical_for_test());
        let authority = super::frame_recorder::validate_recorded_atomic_projection_selection_text_area_authority(
            host.clone(),
            local.clone(),
        )
        .expect("normalized typed pair must consume");
        assert!(authority.is_canonical_for_test());
        assert_eq!(authority.chunk_counts_for_test(), (6, 4));
        assert!(
            authority.localized_selection_changed_for_test(),
            "nonzero outer scroll must localize selection rectangles",
        );
        let plan_parts = super::frame_recorder::validate_recorded_atomic_projection_selection_text_area_plan_parts(authority)
            .expect("typed authority must consume into opaque fixed H/content/O plan parts");
        assert!(plan_parts.is_canonical());
        assert_eq!(plan_parts.chunk_counts_for_test(), (1, 4, 1));
        assert_eq!(
            (
                plan_parts.host_before_opaque_order_count(),
                plan_parts.content_opaque_order_count(),
                plan_parts.overlay_opaque_order_count(),
            ),
            (Some(0), Some(0), Some(0)),
        );
        assert!(plan_parts.same_authority(&plan_parts.clone()));
        assert!(
            !plan_parts.clone().tamper_host_for_test().is_canonical()
                && !plan_parts
                    .clone()
                    .tamper_content_order_for_test()
                    .is_canonical()
                && !plan_parts.clone().tamper_geometry_for_test().is_canonical()
                && !plan_parts.clone().tamper_topology_for_test().is_canonical()
                && !plan_parts
                    .tamper_selection_synchronized_for_test()
                    .is_canonical(),
            "private plan identity must reject H/local order/geometry/topology/selection drift",
        );

        assert!(
            super::frame_recorder::validate_recorded_atomic_projection_selection_text_area_authority(
                host.clone().tamper_order_for_test(2, 3),
                local.clone(),
            )
            .is_none(),
            "synchronized host order tamper must fail",
        );
        assert!(
            super::frame_recorder::validate_recorded_atomic_projection_selection_text_area_authority(
                host.clone(),
                local.clone().tamper_selection_payload_for_test(),
            )
            .is_none(),
            "synchronized local payload tamper must fail",
        );
        assert!(
            super::frame_recorder::validate_recorded_atomic_projection_selection_text_area_authority(
                host.clone().tamper_selection_payload_for_test(),
                local.clone(),
            )
            .is_none(),
            "synchronized host payload tamper must fail",
        );
        assert!(
            super::frame_recorder::validate_recorded_atomic_projection_selection_text_area_authority(
                host.clone().tamper_wrapper_bounds_for_test(),
                local.clone().tamper_wrapper_bounds_for_test(),
            )
            .is_none(),
            "synchronized host/local bounds drift must fail independent scroll geometry",
        );
        assert!(
            super::frame_recorder::validate_recorded_atomic_projection_selection_text_area_authority(
                host.clone(),
                local.clone().tamper_owner_parent_for_test(),
            )
            .is_none(),
            "synchronized owner topology tamper must fail",
        );
        assert!(
            super::frame_recorder::validate_recorded_atomic_projection_selection_text_area_authority(
                host.clone().tamper_source_line_for_test(),
                local.clone().tamper_source_line_for_test(),
            )
            .is_none(),
            "synchronized public source grammar tamper must fail private identity",
        );
        assert!(
            super::frame_recorder::validate_recorded_atomic_projection_selection_text_area_authority(
                host,
                local.tamper_local_clip_for_test(),
            )
            .is_none(),
            "synchronized local clip tamper must fail",
        );
    }

    #[test]
    fn atomic_projection_selection_live_authority_builds_exact4_same_key_raster_stamp() {
        let stable_id = 0xc3b4_4301;
        let baseline = atomic_projection_selection_content_stamp_for_test(6, stable_id)
            .expect("live recorded selection authority must build the dedicated stamp");
        let changed = atomic_projection_selection_content_stamp_for_test(5, stable_id)
            .expect("changed live selection output must remain admissible");
        assert!(retained_surface_raster_stamp_is_canonical(&baseline));
        assert!(retained_surface_raster_stamp_is_canonical(&changed));
        assert_eq!(baseline.chunks.len(), 4);
        assert_eq!(changed.chunks.len(), 4);
        assert_eq!(
            baseline.identity.resident_key(),
            changed.identity.resident_key(),
            "selection output changes must keep the same resident allocation key",
        );
        assert_ne!(
            baseline, changed,
            "exact local selection output must participate in raster identity",
        );
        assert!(baseline.text_area_paint_grammar.is_none());
        assert!(baseline.interactive_text_area_resident.is_none());
        assert!(matches!(
            baseline.atomic_projection_text_area_resident,
            Some(super::compiler::RetainedAtomicProjectionTextAreaRasterDependency::Selection(_))
        ));
    }

    #[test]
    fn atomic_projection_selection_emission_constructor_requires_full_canonical_stamp() {
        let (plan, stamp) = atomic_projection_selection_emission_fixture_for_test(6, 0xc3b4_4303)
            .expect("canonical selection emission fixture");
        assert!(
            super::compiler::prepare_validated_scroll_scene_atomic_projection_selection_text_area_emission(
                plan.clone(),
                &stamp,
            )
            .is_some()
        );

        let mut drifted = stamp;
        drifted.chunks[1].bounds_bits[0] ^= 1;
        assert!(
            super::compiler::prepare_validated_scroll_scene_atomic_projection_selection_text_area_emission(
                plan,
                &drifted,
            )
            .is_none()
        );
    }

    #[test]
    fn atomic_projection_selection_raster_stamp_rejects_hybrid_tile_role_and_tamper() {
        let stable_id = 0xc3b4_4302;
        let stamp = atomic_projection_selection_content_stamp_for_test(6, stable_id)
            .expect("canonical live selection stamp");
        let glyph_stamp = atomic_projection_content_stamp_for_test("projected", stable_id)
            .expect("canonical C3a glyph control stamp");

        let mut selection_with_plain = stamp.clone();
        selection_with_plain.text_area_paint_grammar =
            Some(crate::view::base_component::text_area::RetainedTextAreaPaintGrammar::GlyphOnly);
        assert!(!retained_surface_raster_stamp_is_canonical(
            &selection_with_plain
        ));

        let mut selection_with_interactive = stamp.clone();
        selection_with_interactive.interactive_text_area_resident =
            Some(super::artifact::RetainedInteractiveTextAreaResidentRasterSeal::FocusedGlyphs);
        assert!(!retained_surface_raster_stamp_is_canonical(
            &selection_with_interactive
        ));

        let glyph_dependency = glyph_stamp
            .atomic_projection_text_area_resident
            .clone()
            .expect("C3a glyph dependency");
        let selection_dependency = stamp
            .atomic_projection_text_area_resident
            .clone()
            .expect("selection dependency");
        let mut selection_with_glyph_dependency = stamp.clone();
        selection_with_glyph_dependency.atomic_projection_text_area_resident =
            Some(glyph_dependency);
        assert!(!retained_surface_raster_stamp_is_canonical(
            &selection_with_glyph_dependency
        ));
        let mut glyph_with_selection_dependency = glyph_stamp;
        glyph_with_selection_dependency.atomic_projection_text_area_resident =
            Some(selection_dependency);
        assert!(!retained_surface_raster_stamp_is_canonical(
            &glyph_with_selection_dependency
        ));

        let mut wrong_role = stamp.clone();
        wrong_role.identity.role = RetainedSurfaceRasterRole::Transform;
        assert!(!retained_surface_raster_stamp_is_canonical(&wrong_role));

        let content_bounds = stamp.target.source_bounds_bits.map(|bits| {
            let value = f32::from_bits(bits);
            assert!(value >= 0.0 && value.fract() == 0.0);
            value as u32
        });
        let index = ScrollContentTileIndex { column: 0, row: 0 };
        let tile_edge = content_bounds[2].max(content_bounds[3]);
        let tile_bounds =
            ScrollContentTileBounds::for_index(content_bounds, tile_edge, 0, index).unwrap();
        let tile =
            ScrollContentTileRasterIdentity::new(index, content_bounds, tile_bounds, tile_edge, 0)
                .unwrap();
        let mut tile_misuse = stamp.clone();
        tile_misuse.identity.scroll_content_tile = Some(tile);
        assert!(!retained_surface_raster_stamp_is_canonical(&tile_misuse));

        let mut synchronized_tamper = stamp;
        let Some(super::compiler::RetainedAtomicProjectionTextAreaRasterDependency::Selection(
            resident,
        )) = synchronized_tamper
            .atomic_projection_text_area_resident
            .as_mut()
        else {
            panic!("selection dependency variant")
        };
        let drifted = (f32::from_bits(resident.selection_chunk.bounds_bits[0]) + 1.0).to_bits();
        resident.selection_chunk.bounds_bits[0] = drifted;
        synchronized_tamper.chunks[1].bounds_bits[0] = drifted;
        let [RetainedSurfaceRasterStepStamp::ArtifactSpan(span)] =
            synchronized_tamper.ordered_steps.as_mut_slice()
        else {
            panic!("selection stamp must own one exact4 span")
        };
        span.chunks[1].bounds_bits[0] = drifted;
        assert_eq!(synchronized_tamper.chunks, span.chunks);
        assert!(!retained_surface_raster_stamp_is_canonical(
            &synchronized_tamper
        ));
    }

    #[test]
    fn atomic_projection_selection_property_scroll_cold_warm_and_collision_are_closed_loop() {
        let ctx = || UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
        let clear = [0.0, 0.0, 0.0, 1.0];
        let mut viewport = crate::view::viewport::Viewport::new();

        let cold_owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut cold_graph = FrameGraph::new();
        let cold_scene = validated_atomic_projection_selection_scroll_scene_at(6);
        assert!(cold_scene.is_canonical());
        assert!(cold_scene.atomic_projection_selection_contract_for_test());
        assert!(cold_scene.atomic_projection_selection_tamper_matrix_for_test());
        assert!(cold_scene.atomic_projection_selection_prepare_failure_matrix_is_atomic_for_test());
        let cold = super::scroll_scene::prepare_retained_property_scroll_forest_from_pool(
            &mut viewport,
            cold_scene,
            &mut cold_graph,
            ctx(),
            clear,
            cold_owner,
        )
        .expect("cold selection scene prepares without graph mutation");
        let cold_stamps = cold.scroll_content_stamps_for_test();
        let [cold_stamp] = cold_stamps.as_slice() else {
            panic!("one selection root owns one Single content stamp")
        };
        let cold_stamp = cold_stamp.clone();
        take_artifact_compile_count();
        let cold = super::scroll_scene::emit_prepared_retained_property_scroll_forest(cold);
        let (cold_state, cold_trace) = cold.into_parts();
        let cold_passes = cold_graph
            .pass_descriptors()
            .iter()
            .map(|descriptor| descriptor.name.rsplit("::").next().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            cold_passes,
            [
                "ClearPass",
                "DrawRectPass",
                "ClearPass",
                "DrawRectPass",
                "DrawRectPass",
                "TextPreparedInputPass",
                "TextPreparedInputPass",
                "TextureCompositePass",
            ],
            "root clear -> H -> content clear -> selection/root/projection local raster -> composite -> empty O",
        );
        assert_eq!((cold_trace.reraster_count, cold_trace.reuse_count), (1, 0));
        assert_eq!(take_artifact_compile_count(), 3, "cold emits H/C/O");
        assert_eq!(cold_state.opaque_rect_order(), 0);
        assert_eq!(
            cold_graph
                .test_graphics_passes::<crate::view::render_pass::ClearPass>()
                .len(),
            2
        );
        assert!(viewport.finish_retained_surface_transaction_for_frame(Some(cold_owner), true));

        let warm_owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut warm_graph = FrameGraph::new();
        let mut warm = super::scroll_scene::prepare_retained_property_scroll_forest_from_pool(
            &mut viewport,
            validated_atomic_projection_selection_scroll_scene_fixture(
                AtomicProjectionScrollFixture::baseline("projected", 44.0),
                6,
            ),
            &mut warm_graph,
            ctx(),
            clear,
            warm_owner,
        )
        .expect("outer-scroll-only selection scene prepares");
        warm.refresh_actions_from_committed_test_pool();
        let warm_stamps = warm.scroll_content_stamps_for_test();
        let [warm_stamp] = warm_stamps.as_slice() else {
            panic!("one warm selection content stamp")
        };
        assert_eq!(
            warm_stamp.identity.resident_key(),
            cold_stamp.identity.resident_key()
        );
        assert_eq!(
            warm_stamp, &cold_stamp,
            "outer scroll is composite-only state"
        );
        take_artifact_compile_count();
        let warm = super::scroll_scene::emit_prepared_retained_property_scroll_forest(warm);
        let (warm_state, warm_trace) = warm.into_parts();
        let warm_passes = warm_graph
            .pass_descriptors()
            .iter()
            .map(|descriptor| descriptor.name.rsplit("::").next().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            warm_passes,
            ["ClearPass", "DrawRectPass", "TextureCompositePass"]
        );
        assert_eq!((warm_trace.reraster_count, warm_trace.reuse_count), (0, 1));
        assert_eq!(take_artifact_compile_count(), 2, "reuse emits H/O only");
        assert_eq!(warm_state.opaque_rect_order(), 0);
        assert_eq!(
            warm_graph
                .test_graphics_passes::<crate::view::render_pass::ClearPass>()
                .len(),
            1
        );
        assert!(viewport.finish_retained_surface_transaction_for_frame(Some(warm_owner), true));

        let collision_scene = validated_atomic_projection_selection_scroll_scene_at(6);
        let (collision_key, collision_desc) = collision_scene
            .first_single_backing_declaration_for_test()
            .unwrap();
        let collision_owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut collision_graph = FrameGraph::new();
        let _ = collision_graph.declare_persistent_texture_internal::<
            crate::view::render_pass::draw_rect_pass::RenderTargetTag,
        >(collision_desc, collision_key);
        let graph_before = collision_graph.build_state_snapshot_for_test();
        let pool_before = viewport.retained_surface_transaction_shape_for_test();
        take_artifact_compile_count();
        assert_eq!(
            super::scroll_scene::prepare_retained_property_scroll_forest_from_pool(
                &mut viewport,
                collision_scene,
                &mut collision_graph,
                ctx(),
                clear,
                collision_owner,
            )
            .err(),
            Some(
                super::scroll_scene::RetainedPropertyScrollScenePrepareError::PersistentKeyAlreadyDeclared(
                    collision_key,
                ),
            )
        );
        assert_eq!(
            take_artifact_compile_count(),
            0,
            "prepare cannot compile artifacts"
        );
        assert_eq!(
            collision_graph.build_state_snapshot_for_test(),
            graph_before
        );
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            pool_before
        );
        assert!(viewport.retained_surface_frame_stage_owner_is_active(collision_owner));
        assert!(
            viewport.finish_retained_surface_transaction_for_frame(Some(collision_owner), false)
        );

        let recovery_owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut recovery_graph = FrameGraph::new();
        let mut recovery = super::scroll_scene::prepare_retained_property_scroll_forest_from_pool(
            &mut viewport,
            validated_atomic_projection_selection_scroll_scene_at(6),
            &mut recovery_graph,
            ctx(),
            clear,
            recovery_owner,
        )
        .expect("collision cannot disturb committed selection resident");
        recovery.refresh_actions_from_committed_test_pool();
        let recovery = super::scroll_scene::emit_prepared_retained_property_scroll_forest(recovery);
        let (_, recovery_trace) = recovery.into_parts();
        assert_eq!(
            (recovery_trace.reraster_count, recovery_trace.reuse_count),
            (0, 1)
        );
        assert!(viewport.finish_retained_surface_transaction_for_frame(Some(recovery_owner), true));
    }

    #[test]
    fn atomic_projection_selection_property_scroll_local_output_change_matrix_rerasterizes_same_resident()
     {
        let baseline = AtomicProjectionScrollFixture::baseline("projected", 20.0);
        let mut source = baseline;
        source.content = "source projected after";
        let mut style = baseline;
        style.font_size = 16.0;
        let mut payload = baseline;
        payload.projected_content = "projection";
        let mut geometry = baseline;
        geometry.content_height = 340.0;
        let mut topology = baseline;
        topology.projection_start = 6;
        topology.projection_end = 15;
        let mut local_clip = baseline;
        local_clip.width = 108.0;

        let ctx = || UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
        for (name, fixture, selection_end) in [
            ("selection", baseline, 5),
            ("source", source, 6),
            ("style", style, 6),
            ("payload", payload, 6),
            ("geometry", geometry, 6),
            ("topology", topology, 6),
            ("local-clip", local_clip, 6),
        ] {
            let mut viewport = crate::view::viewport::Viewport::new();
            let cold_owner = viewport.begin_retained_surface_frame_stage().unwrap();
            let mut cold_graph = FrameGraph::new();
            let cold = super::scroll_scene::prepare_retained_property_scroll_forest_from_pool(
                &mut viewport,
                validated_atomic_projection_selection_scroll_scene_fixture(baseline, 6),
                &mut cold_graph,
                ctx(),
                [0.0; 4],
                cold_owner,
            )
            .unwrap();
            let cold_stamps = cold.scroll_content_stamps_for_test();
            let [cold_stamp] = cold_stamps.as_slice() else {
                panic!("{name}: baseline owns one selection content stamp")
            };
            let cold_stamp = cold_stamp.clone();
            let _ = super::scroll_scene::emit_prepared_retained_property_scroll_forest(cold);
            assert!(viewport.finish_retained_surface_transaction_for_frame(Some(cold_owner), true));

            let changed_owner = viewport.begin_retained_surface_frame_stage().unwrap();
            let mut changed_graph = FrameGraph::new();
            let mut changed =
                super::scroll_scene::prepare_retained_property_scroll_forest_from_pool(
                    &mut viewport,
                    validated_atomic_projection_selection_scroll_scene_fixture(
                        fixture,
                        selection_end,
                    ),
                    &mut changed_graph,
                    ctx(),
                    [0.0; 4],
                    changed_owner,
                )
                .unwrap_or_else(|error| panic!("{name}: valid selection variant: {error:?}"));
            changed.refresh_actions_from_committed_test_pool();
            let changed_stamps = changed.scroll_content_stamps_for_test();
            let [changed_stamp] = changed_stamps.as_slice() else {
                panic!("{name}: changed scene owns one selection content stamp")
            };
            assert_eq!(
                changed_stamp.identity.resident_key(),
                cold_stamp.identity.resident_key(),
                "{name}: stable resident allocation identity"
            );
            assert_ne!(
                changed_stamp, &cold_stamp,
                "{name}: full local raster identity"
            );
            if name == "geometry" {
                assert_ne!(
                    changed_stamp.target.source_bounds_bits,
                    cold_stamp.target.source_bounds_bits
                );
            }
            if name == "local-clip" {
                assert_ne!(changed_stamp.clip_nodes, cold_stamp.clip_nodes);
            }
            take_artifact_compile_count();
            let changed =
                super::scroll_scene::emit_prepared_retained_property_scroll_forest(changed);
            let (_, trace) = changed.into_parts();
            assert_eq!((trace.reraster_count, trace.reuse_count), (1, 0), "{name}");
            assert_eq!(take_artifact_compile_count(), 3, "{name}: H/C/O");
            assert_eq!(
                changed_graph
                    .test_graphics_passes::<crate::view::render_pass::ClearPass>()
                    .len(),
                2,
                "{name}: root and content clear"
            );
            assert!(
                viewport.finish_retained_surface_transaction_for_frame(Some(changed_owner), true)
            );
        }
    }

    #[test]
    fn atomic_projection_property_scroll_cold_warm_reuse_and_collision_are_closed_loop() {
        let ctx = || UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
        let clear = [0.0, 0.0, 0.0, 1.0];
        let mut viewport = crate::view::viewport::Viewport::new();

        let cold_owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut cold_graph = FrameGraph::new();
        let cold = super::scroll_scene::prepare_retained_property_scroll_forest_from_pool(
            &mut viewport,
            validated_atomic_projection_scroll_scene_at("projected", 20.0),
            &mut cold_graph,
            ctx(),
            clear,
            cold_owner,
        )
        .expect("cold C3a scene prepares before graph mutation");
        let cold_stamps = cold.scroll_content_stamps_for_test();
        let [cold_stamp] = cold_stamps.as_slice() else {
            panic!("one C3a scroll root owns one Single content stamp")
        };
        let cold_stamp = cold_stamp.clone();
        take_artifact_compile_count();
        let cold = super::scroll_scene::emit_prepared_retained_property_scroll_forest(cold);
        let (cold_state, cold_trace) = cold.into_parts();
        let cold_passes = cold_graph
            .pass_descriptors()
            .iter()
            .map(|descriptor| descriptor.name.rsplit("::").next().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            cold_passes,
            [
                "ClearPass",
                "DrawRectPass",
                "ClearPass",
                "DrawRectPass",
                "TextPreparedInputPass",
                "TextPreparedInputPass",
                "TextureCompositePass",
            ],
            "root clear -> host -> detached content -> composite; the empty overlay still consumes the compiler token after composite",
        );
        assert_eq!((cold_trace.reraster_count, cold_trace.reuse_count), (1, 0));
        assert_eq!(take_artifact_compile_count(), 3, "cold emits H/C/O");
        assert_eq!(cold_state.opaque_rect_order(), 0);
        assert_eq!(
            cold_graph
                .test_graphics_passes::<crate::view::render_pass::ClearPass>()
                .len(),
            2,
            "root clear plus detached-content clear"
        );
        assert_eq!(
            cold_graph
                .test_graphics_passes::<crate::view::render_pass::TextureCompositePass>()
                .len(),
            1
        );
        assert!(viewport.finish_retained_surface_transaction_for_frame(Some(cold_owner), true));

        let warm_owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut warm_graph = FrameGraph::new();
        let mut warm = super::scroll_scene::prepare_retained_property_scroll_forest_from_pool(
            &mut viewport,
            validated_atomic_projection_scroll_scene_at("projected", 44.0),
            &mut warm_graph,
            ctx(),
            clear,
            warm_owner,
        )
        .expect("outer-scroll-only C3a scene prepares");
        warm.refresh_actions_from_committed_test_pool();
        let warm_stamps = warm.scroll_content_stamps_for_test();
        let [warm_stamp] = warm_stamps.as_slice() else {
            panic!("one warm C3a content stamp")
        };
        assert_eq!(
            warm_stamp.identity.resident_key(),
            cold_stamp.identity.resident_key()
        );
        assert_eq!(
            warm_stamp, &cold_stamp,
            "outer scroll is composite-only state"
        );
        take_artifact_compile_count();
        let warm = super::scroll_scene::emit_prepared_retained_property_scroll_forest(warm);
        let (warm_state, warm_trace) = warm.into_parts();
        let warm_passes = warm_graph
            .pass_descriptors()
            .iter()
            .map(|descriptor| descriptor.name.rsplit("::").next().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            warm_passes,
            ["ClearPass", "DrawRectPass", "TextureCompositePass"],
            "reuse keeps host/composite order and emits no detached-content passes",
        );
        assert_eq!((warm_trace.reraster_count, warm_trace.reuse_count), (0, 1));
        assert_eq!(
            take_artifact_compile_count(),
            2,
            "reuse emits H/O and replays the detached content cursor"
        );
        assert_eq!(warm_state.opaque_rect_order(), 0);
        assert_eq!(
            warm_graph
                .test_graphics_passes::<crate::view::render_pass::ClearPass>()
                .len(),
            1,
            "reuse has only the root clear"
        );
        assert_eq!(
            warm_graph
                .test_graphics_passes::<crate::view::render_pass::TextureCompositePass>()
                .len(),
            1
        );
        assert!(viewport.finish_retained_surface_transaction_for_frame(Some(warm_owner), true));

        let collision_scene = validated_atomic_projection_scroll_scene_at("projected", 44.0);
        let (collision_key, collision_desc) = collision_scene
            .first_single_backing_declaration_for_test()
            .unwrap();
        let collision_owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut collision_graph = FrameGraph::new();
        let _ = collision_graph.declare_persistent_texture_internal::<
            crate::view::render_pass::draw_rect_pass::RenderTargetTag,
        >(collision_desc, collision_key);
        let graph_before = collision_graph.build_state_snapshot_for_test();
        let pool_before = viewport.retained_surface_transaction_shape_for_test();
        assert_eq!(
            super::scroll_scene::prepare_retained_property_scroll_forest_from_pool(
                &mut viewport,
                collision_scene,
                &mut collision_graph,
                ctx(),
                clear,
                collision_owner,
            )
            .err(),
            Some(
                super::scroll_scene::RetainedPropertyScrollScenePrepareError::PersistentKeyAlreadyDeclared(
                    collision_key,
                ),
            )
        );
        assert_eq!(
            collision_graph.build_state_snapshot_for_test(),
            graph_before
        );
        assert_eq!(
            viewport.retained_surface_transaction_shape_for_test(),
            pool_before
        );
        assert!(viewport.retained_surface_frame_stage_owner_is_active(collision_owner));
        assert!(
            viewport.finish_retained_surface_transaction_for_frame(Some(collision_owner), false)
        );

        let recovery_owner = viewport.begin_retained_surface_frame_stage().unwrap();
        let mut recovery_graph = FrameGraph::new();
        let mut recovery = super::scroll_scene::prepare_retained_property_scroll_forest_from_pool(
            &mut viewport,
            validated_atomic_projection_scroll_scene_at("projected", 44.0),
            &mut recovery_graph,
            ctx(),
            clear,
            recovery_owner,
        )
        .expect("collision cannot disturb the committed resident");
        recovery.refresh_actions_from_committed_test_pool();
        let recovery = super::scroll_scene::emit_prepared_retained_property_scroll_forest(recovery);
        let (_, recovery_trace) = recovery.into_parts();
        assert_eq!(
            (recovery_trace.reraster_count, recovery_trace.reuse_count),
            (0, 1)
        );
        assert!(viewport.finish_retained_surface_transaction_for_frame(Some(recovery_owner), true));
    }

    #[test]
    fn atomic_projection_property_scroll_local_output_change_matrix_rerasterizes_same_resident() {
        let baseline = AtomicProjectionScrollFixture::baseline("projected", 20.0);
        let mut source = baseline;
        source.content = "source projected after";
        let mut style = baseline;
        style.font_size = 16.0;
        let mut payload = baseline;
        payload.projected_content = "projection";
        let mut geometry = baseline;
        geometry.content_height = 340.0;
        let mut topology = baseline;
        topology.projection_start = 6;
        topology.projection_end = 15;
        let mut local_clip = baseline;
        local_clip.width = 108.0;

        let ctx = || UiBuildContext::new(320, 240, wgpu::TextureFormat::Bgra8UnormSrgb, 1.0);
        for (name, variant) in [
            ("source", source),
            ("style", style),
            ("payload", payload),
            ("geometry", geometry),
            ("topology", topology),
            ("local-clip", local_clip),
        ] {
            let mut viewport = crate::view::viewport::Viewport::new();
            let cold_owner = viewport.begin_retained_surface_frame_stage().unwrap();
            let mut cold_graph = FrameGraph::new();
            let cold = super::scroll_scene::prepare_retained_property_scroll_forest_from_pool(
                &mut viewport,
                validated_atomic_projection_scroll_scene_fixture(baseline),
                &mut cold_graph,
                ctx(),
                [0.0; 4],
                cold_owner,
            )
            .unwrap();
            let cold_stamps = cold.scroll_content_stamps_for_test();
            let [cold_stamp] = cold_stamps.as_slice() else {
                panic!("{name}: baseline owns one content stamp")
            };
            let cold_stamp = cold_stamp.clone();
            let _ = super::scroll_scene::emit_prepared_retained_property_scroll_forest(cold);
            assert!(viewport.finish_retained_surface_transaction_for_frame(Some(cold_owner), true));

            let changed_owner = viewport.begin_retained_surface_frame_stage().unwrap();
            let mut changed_graph = FrameGraph::new();
            let mut changed =
                super::scroll_scene::prepare_retained_property_scroll_forest_from_pool(
                    &mut viewport,
                    validated_atomic_projection_scroll_scene_fixture(variant),
                    &mut changed_graph,
                    ctx(),
                    [0.0; 4],
                    changed_owner,
                )
                .unwrap_or_else(|error| panic!("{name}: valid C3a variant: {error:?}"));
            changed.refresh_actions_from_committed_test_pool();
            let changed_stamps = changed.scroll_content_stamps_for_test();
            let [changed_stamp] = changed_stamps.as_slice() else {
                panic!("{name}: changed scene owns one content stamp")
            };
            assert_eq!(
                changed_stamp.identity.resident_key(),
                cold_stamp.identity.resident_key(),
                "{name}: resident identity"
            );
            assert_ne!(changed_stamp, &cold_stamp, "{name}: local raster output");
            if name == "geometry" {
                assert_ne!(
                    changed_stamp.target.source_bounds_bits,
                    cold_stamp.target.source_bounds_bits
                );
            }
            if name == "local-clip" {
                assert_ne!(changed_stamp.clip_nodes, cold_stamp.clip_nodes);
            }
            take_artifact_compile_count();
            let changed =
                super::scroll_scene::emit_prepared_retained_property_scroll_forest(changed);
            let (_, trace) = changed.into_parts();
            assert_eq!((trace.reraster_count, trace.reuse_count), (1, 0), "{name}");
            assert_eq!(take_artifact_compile_count(), 3, "{name}: H/C/O");
            assert_eq!(
                changed_graph
                    .test_graphics_passes::<crate::view::render_pass::ClearPass>()
                    .len(),
                2,
                "{name}: root and content clear"
            );
            assert!(
                viewport.finish_retained_surface_transaction_for_frame(Some(changed_owner), true)
            );
        }
    }

    #[test]
    fn atomic_projection_text_area_content_raster_stamp_is_closed_and_one_hot() {
        let stable_id = 0xc3a_4301;
        let stamp = atomic_projection_content_stamp_for_test("projected", stable_id)
            .expect("dedicated atomic content stamp constructor");
        assert!(retained_surface_raster_stamp_is_canonical(&stamp));
        assert!(stamp.text_area_paint_grammar.is_none());
        assert!(stamp.interactive_text_area_resident.is_none());
        assert!(stamp.atomic_projection_text_area_resident.is_some());

        let RetainedSurfaceRasterStepStamp::ArtifactSpan(atomic_span) = &stamp.ordered_steps[0]
        else {
            panic!("atomic content stamp must have one artifact span")
        };
        let mut legacy_span = atomic_span.clone();
        let projection = legacy_span.chunks.pop().unwrap();
        legacy_span.op_count = legacy_span
            .op_count
            .checked_sub(projection.op_count)
            .unwrap();
        let plain = super::compiler::validated_scroll_text_area_content_raster_stamp(
            stamp.identity.boundary_root,
            stable_id,
            stamp.target.clone(),
            legacy_span.clone(),
            stamp.opaque_order_span.clone(),
            crate::view::base_component::text_area::RetainedTextAreaPaintGrammar::GlyphOnly,
        )
        .expect("C1 plain TextArea control stamp");
        assert!(retained_surface_raster_stamp_is_canonical(&plain));
        let interactive =
            super::compiler::validated_scroll_interactive_text_area_content_raster_stamp(
                stamp.identity.boundary_root,
                stable_id,
                stamp.target.clone(),
                legacy_span,
                stamp.opaque_order_span.clone(),
                super::artifact::RetainedInteractiveTextAreaResidentRasterSeal::FocusedGlyphs,
            )
            .expect("C2 interactive TextArea control stamp");
        assert!(retained_surface_raster_stamp_is_canonical(&interactive));
        let mut generic_span = atomic_span.clone();
        generic_span.owner_topology.truncate(1);
        generic_span.clip_nodes.clear();
        generic_span.chunks.truncate(1);
        generic_span.op_count = generic_span.chunks[0].op_count;
        let generic = super::compiler::validated_scroll_content_raster_stamp(
            stamp.identity.boundary_root,
            stable_id,
            stamp.target.clone(),
            generic_span,
            stamp.opaque_order_span.clone(),
        )
        .expect("generic scroll-content control stamp");
        assert!(retained_surface_raster_stamp_is_canonical(&generic));
        let atomic_resident = stamp
            .atomic_projection_text_area_resident
            .as_ref()
            .unwrap()
            .clone();
        let mut generic_with_atomic = generic;
        generic_with_atomic.atomic_projection_text_area_resident = Some(atomic_resident.clone());
        assert!(!retained_surface_raster_stamp_is_canonical(
            &generic_with_atomic
        ));
        let mut plain_with_interactive = plain.clone();
        plain_with_interactive.interactive_text_area_resident =
            Some(super::artifact::RetainedInteractiveTextAreaResidentRasterSeal::FocusedGlyphs);
        assert!(!retained_surface_raster_stamp_is_canonical(
            &plain_with_interactive
        ));
        let mut plain_with_atomic = plain;
        plain_with_atomic.atomic_projection_text_area_resident = Some(atomic_resident.clone());
        assert!(!retained_surface_raster_stamp_is_canonical(
            &plain_with_atomic
        ));
        let mut interactive_with_plain = interactive.clone();
        interactive_with_plain.text_area_paint_grammar =
            Some(crate::view::base_component::text_area::RetainedTextAreaPaintGrammar::GlyphOnly);
        assert!(!retained_surface_raster_stamp_is_canonical(
            &interactive_with_plain
        ));
        let mut interactive_with_atomic = interactive;
        interactive_with_atomic.atomic_projection_text_area_resident = Some(atomic_resident);
        assert!(!retained_surface_raster_stamp_is_canonical(
            &interactive_with_atomic
        ));

        let mut plain_atomic_hybrid = stamp.clone();
        plain_atomic_hybrid.text_area_paint_grammar =
            Some(crate::view::base_component::text_area::RetainedTextAreaPaintGrammar::GlyphOnly);
        assert!(!retained_surface_raster_stamp_is_canonical(
            &plain_atomic_hybrid
        ));

        let mut interactive_atomic_hybrid = stamp.clone();
        interactive_atomic_hybrid.interactive_text_area_resident =
            Some(super::artifact::RetainedInteractiveTextAreaResidentRasterSeal::FocusedGlyphs);
        assert!(!retained_surface_raster_stamp_is_canonical(
            &interactive_atomic_hybrid
        ));

        let mut synchronized_public_tamper = stamp.clone();
        let Some(super::compiler::RetainedAtomicProjectionTextAreaRasterDependency::Glyph(
            atomic_resident,
        )) = synchronized_public_tamper
            .atomic_projection_text_area_resident
            .as_ref()
        else {
            panic!("C3a stamp must carry the glyph dependency")
        };
        let original_x = atomic_resident.wrapper_chunk.bounds_bits[0];
        let drifted_x = (f32::from_bits(original_x) + 1.0).to_bits();
        let Some(super::compiler::RetainedAtomicProjectionTextAreaRasterDependency::Glyph(
            atomic_resident,
        )) = synchronized_public_tamper
            .atomic_projection_text_area_resident
            .as_mut()
        else {
            panic!("C3a stamp must carry the glyph dependency")
        };
        atomic_resident.wrapper_chunk.bounds_bits[0] = drifted_x;
        synchronized_public_tamper.chunks[0].bounds_bits[0] = drifted_x;
        synchronized_public_tamper.target.source_bounds_bits[0] = drifted_x;
        let [target_x, target_y, target_width, target_height] = synchronized_public_tamper
            .target
            .source_bounds_bits
            .map(f32::from_bits);
        let rebuilt_color = crate::view::base_component::texture_desc_for_logical_bounds(
            crate::view::base_component::RetainedSurfaceBounds {
                x: target_x,
                y: target_y,
                width: target_width,
                height: target_height,
                corner_radii: [0.0; 4],
            },
            1.0,
            None,
            synchronized_public_tamper.target.color.format(),
        );
        let (rebuilt_color, rebuilt_depth) =
            crate::view::base_component::persistent_target_texture_descriptors(
                rebuilt_color,
                synchronized_public_tamper.identity.color_key,
            );
        synchronized_public_tamper.target.color = rebuilt_color;
        synchronized_public_tamper.target.depth = rebuilt_depth;
        let [RetainedSurfaceRasterStepStamp::ArtifactSpan(span)] =
            synchronized_public_tamper.ordered_steps.as_mut_slice()
        else {
            panic!("atomic content stamp must have one artifact span")
        };
        span.chunks[0].bounds_bits[0] = drifted_x;
        assert_eq!(synchronized_public_tamper.chunks, span.chunks);
        assert_eq!(synchronized_public_tamper.op_count, span.op_count);
        let Some(super::compiler::RetainedAtomicProjectionTextAreaRasterDependency::Glyph(
            atomic_resident,
        )) = synchronized_public_tamper
            .atomic_projection_text_area_resident
            .as_ref()
        else {
            panic!("C3a stamp must carry the glyph dependency")
        };
        assert_eq!(
            synchronized_public_tamper.target.source_bounds_bits,
            atomic_resident.wrapper_chunk.bounds_bits,
        );
        assert!(
            synchronized_public_tamper
                .target
                .has_canonical_descriptor_pair_for(synchronized_public_tamper.identity),
            "synchronized source-position drift must pass public target structure",
        );
        assert!(!retained_surface_raster_stamp_is_canonical(
            &synchronized_public_tamper
        ));

        let mut role_misuse = stamp.clone();
        role_misuse.identity.role = RetainedSurfaceRasterRole::Transform;
        assert!(!retained_surface_raster_stamp_is_canonical(&role_misuse));

        let mut executor_transform = stamp.clone();
        executor_transform.text_area_paint_grammar = None;
        executor_transform.interactive_text_area_resident = None;
        executor_transform.atomic_projection_text_area_resident = None;
        executor_transform.identity.role = RetainedSurfaceRasterRole::Transform;
        executor_transform.identity.color_key =
            crate::view::base_component::transformed_layer_stable_key(stable_id);
        let [x, y, width, height] = executor_transform
            .target
            .source_bounds_bits
            .map(f32::from_bits);
        let transform_color = crate::view::base_component::texture_desc_for_logical_bounds(
            crate::view::base_component::RetainedSurfaceBounds {
                x,
                y,
                width,
                height,
                corner_radii: [0.0; 4],
            },
            1.0,
            None,
            executor_transform.target.color.format(),
        );
        let (transform_color, transform_depth) =
            crate::view::base_component::persistent_target_texture_descriptors(
                transform_color,
                executor_transform.identity.color_key,
            );
        executor_transform.target.color = transform_color;
        executor_transform.target.depth = transform_depth;
        assert!(
            !super::retained_surface_executor::legacy_property_executor_rejects_transform_effect_scroll_child_for_test(
                &executor_transform,
            ),
            "legacy property Transform control must reach its private canonicalizer",
        );
        let mut executor_plain = executor_transform.clone();
        executor_plain.text_area_paint_grammar =
            Some(crate::view::base_component::text_area::RetainedTextAreaPaintGrammar::GlyphOnly);
        assert!(
            super::retained_surface_executor::legacy_property_executor_rejects_transform_effect_scroll_child_for_test(
                &executor_plain,
            )
        );
        let mut executor_interactive = executor_transform.clone();
        executor_interactive.interactive_text_area_resident =
            Some(super::artifact::RetainedInteractiveTextAreaResidentRasterSeal::FocusedGlyphs);
        assert!(
            super::retained_surface_executor::legacy_property_executor_rejects_transform_effect_scroll_child_for_test(
                &executor_interactive,
            )
        );
        let mut executor_atomic = executor_transform;
        executor_atomic.atomic_projection_text_area_resident =
            stamp.atomic_projection_text_area_resident.clone();
        assert!(
            super::retained_surface_executor::legacy_property_executor_rejects_transform_effect_scroll_child_for_test(
                &executor_atomic,
            )
        );

        let content_bounds = stamp.target.source_bounds_bits.map(|bits| {
            let value = f32::from_bits(bits);
            assert!(value >= 0.0 && value.fract() == 0.0);
            value as u32
        });
        let index = ScrollContentTileIndex { column: 0, row: 0 };
        let tile_edge = content_bounds[2].max(content_bounds[3]);
        let tile_bounds =
            ScrollContentTileBounds::for_index(content_bounds, tile_edge, 0, index).unwrap();
        let tile =
            ScrollContentTileRasterIdentity::new(index, content_bounds, tile_bounds, tile_edge, 0)
                .unwrap();
        let mut tile_misuse = stamp.clone();
        tile_misuse.identity.scroll_content_tile = Some(tile);
        tile_misuse.identity.color_key =
            crate::view::base_component::scroll_content_tile_layer_stable_key(
                stable_id,
                index.column,
                index.row,
            )
            .unwrap();
        let [tile_x, tile_y, tile_width, tile_height] =
            tile.bounds.raster.map(|value| value as f32);
        let tile_target_bounds = crate::view::base_component::RetainedSurfaceBounds {
            x: tile_x,
            y: tile_y,
            width: tile_width,
            height: tile_height,
            corner_radii: [0.0; 4],
        };
        let tile_color = crate::view::base_component::texture_desc_for_logical_bounds(
            tile_target_bounds,
            1.0,
            None,
            wgpu::TextureFormat::Bgra8UnormSrgb,
        );
        let (tile_color, tile_depth) =
            crate::view::base_component::persistent_target_texture_descriptors(
                tile_color,
                tile_misuse.identity.color_key,
            );
        tile_misuse.target = RetainedSurfaceRasterInputs {
            color: tile_color,
            depth: tile_depth,
            scale_factor_bits: 1.0_f32.to_bits(),
            source_bounds_bits: tile.bounds.raster.map(|value| (value as f32).to_bits()),
        };
        assert!(
            tile_misuse
                .target
                .has_canonical_descriptor_pair_for(tile_misuse.identity),
            "tile misuse must reach the TextArea dependency prohibition",
        );
        assert!(!retained_surface_raster_stamp_is_canonical(&tile_misuse));

        let changed = atomic_projection_content_stamp_for_test("projection", stable_id)
            .expect("changed atomic resident stamp");
        assert!(retained_surface_raster_stamp_is_canonical(&changed));
        assert_eq!(
            stamp.identity.resident_key(),
            changed.identity.resident_key()
        );
        assert_ne!(
            stamp, changed,
            "same key must retain resident stamp changes"
        );
    }

    #[test]
    fn text_area_projection_baked_scroll_translates_root_and_absolute_child_once() {
        fn fixture(scroll_y: f32) -> (NodeArena, Vec<NodeKey>, NodeKey, NodeKey) {
            let (mut arena, roots, root, _, projected_text) = prepared_projection_text_area_tree();
            place_text_area_with_baked_scroll(&mut arena, root, 132.0, 8.0, [0.0, scroll_y]);
            (arena, roots, root, projected_text)
        }

        assert_whole_frame_structural_parity(
            || {
                let (arena, roots, ..) = fixture(4.0);
                (arena, roots)
            },
            PaintParityConfig::default(),
        );

        let first_glyph_position = |artifact: &PaintArtifact, owner: NodeKey| {
            let chunk = artifact
                .chunks
                .iter()
                .find(|chunk| chunk.owner == owner && chunk.id.role == PaintChunkRole::TextGlyphs)
                .expect("owner must retain one glyph chunk");
            let PaintOp::PreparedText(op) = &artifact.ops[chunk.op_range.start] else {
                panic!("glyph chunk must reference a prepared text op")
            };
            op.params.staging_input.glyphs[0].final_paint_pos
        };

        let (arena, roots, root, projected_text) = fixture(0.0);
        let (properties, generations) = sync_identity(&arena, &roots);
        let (unscrolled, eligibility) =
            whole_frame_artifact(&arena, &roots, &properties, &generations);
        assert!(eligibility.eligible);
        let root_before = first_glyph_position(&unscrolled, root);
        let projected_before = first_glyph_position(&unscrolled, projected_text);

        let (arena, roots, root, projected_text) = fixture(4.0);
        let (properties, generations) = sync_identity(&arena, &roots);
        let (scrolled, eligibility) =
            whole_frame_artifact(&arena, &roots, &properties, &generations);
        assert!(eligibility.eligible);
        let root_after = first_glyph_position(&scrolled, root);
        let projected_after = first_glyph_position(&scrolled, projected_text);
        assert_eq!(root_after[1], root_before[1] - 4.0);
        assert_eq!(projected_after[1], projected_before[1] - 4.0);
    }

    #[test]
    fn text_area_projection_preedit_direct_text_is_path_scoped_ordered_and_matches_legacy() {
        assert_whole_frame_structural_parity(
            || {
                let (arena, roots, ..) =
                    prepared_projection_text_area_preedit_tree(8, "中🙂", Some((0, 7)));
                (arena, roots)
            },
            PaintParityConfig::default(),
        );

        let (arena, roots, root, projection, projected_text) =
            prepared_projection_text_area_preedit_tree(8, "中🙂", Some((0, 7)));
        let root_context = arena
            .get(root)
            .unwrap()
            .element
            .shadow_paint_recording_context(PaintRecordingContext::default());
        let projection_context = arena
            .get(root)
            .unwrap()
            .element
            .shadow_paint_recording_context_for_child(projection, &arena, root_context);
        let witness = projection_context
            .text_area_preedit
            .expect("target projection edge must carry preedit authority");
        assert_eq!((witness.local_start_char, witness.local_end_char), (1, 3));
        let text_context = arena
            .get(projection)
            .unwrap()
            .element
            .shadow_paint_recording_context_for_child(projected_text, &arena, projection_context);
        assert_eq!(text_context.text_area_preedit, Some(witness));
        for sibling in arena
            .children_of(root)
            .into_iter()
            .filter(|key| *key != projection)
        {
            assert_eq!(
                arena
                    .get(root)
                    .unwrap()
                    .element
                    .shadow_paint_recording_context_for_child(sibling, &arena, root_context,)
                    .text_area_preedit,
                None
            );
        }

        let (properties, generations) = sync_identity(&arena, &roots);
        let (artifact, eligibility) =
            whole_frame_artifact(&arena, &roots, &properties, &generations);
        assert!(eligibility.eligible);
        assert_eq!(
            artifact
                .chunks
                .iter()
                .map(|chunk| (chunk.owner, chunk.id.phase, chunk.id.slot, chunk.id.role))
                .collect::<Vec<_>>(),
            vec![
                (
                    root,
                    PaintNodePhase::BeforeChildren,
                    1,
                    PaintChunkRole::TextGlyphs
                ),
                (
                    projected_text,
                    PaintNodePhase::BeforeChildren,
                    1,
                    PaintChunkRole::TextGlyphs,
                ),
                (
                    root,
                    PaintNodePhase::AfterChildren,
                    0,
                    PaintChunkRole::TextDecoration
                ),
                (
                    root,
                    PaintNodePhase::AfterChildren,
                    1,
                    PaintChunkRole::Caret
                ),
            ]
        );
        let decoration = artifact
            .chunks
            .iter()
            .find(|chunk| chunk.id.role == PaintChunkRole::TextDecoration)
            .unwrap();
        assert!(artifact.ops[decoration.op_range.clone()].iter().all(
            |op| matches!(op, PaintOp::DrawRect(op) if op.params.size[1].to_bits() == 1.0_f32.to_bits())
        ));
    }

    #[test]
    fn text_area_projection_preedit_utf8_cursor_clamps_to_prior_boundary() {
        let caret_position = |cursor| {
            let (arena, roots, ..) = prepared_projection_text_area_preedit_tree(8, "中🙂", cursor);
            let (properties, generations) = sync_identity(&arena, &roots);
            let (artifact, eligibility) =
                whole_frame_artifact(&arena, &roots, &properties, &generations);
            assert!(eligibility.eligible);
            let chunk = artifact
                .chunks
                .iter()
                .find(|chunk| chunk.id.role == PaintChunkRole::Caret)
                .unwrap();
            let PaintOp::DrawRect(op) = &artifact.ops[chunk.op_range.start] else {
                panic!("caret must be a rect")
            };
            op.params.position
        };
        let start = caret_position(Some((0, 0)));
        assert_eq!(caret_position(Some((0, 1))), start);
        let after_cjk = caret_position(Some((0, 3)));
        assert!(after_cjk[0] > start[0]);
        assert_eq!(caret_position(Some((0, 4))), after_cjk);
        let end = caret_position(None);
        assert!(end[0] > after_cjk[0] || end[1] > after_cjk[1]);
    }

    #[test]
    fn mixed_projection_with_plain_transient_preedit_remains_eligible() {
        let (arena, roots, root, projection, projected_text) =
            prepared_projection_text_area_preedit_tree(2, "中", Some((0, 3)));
        let root_context = arena
            .get(root)
            .unwrap()
            .element
            .shadow_paint_recording_context(PaintRecordingContext::default());
        assert_eq!(
            arena
                .get(root)
                .unwrap()
                .element
                .shadow_paint_recording_context_for_child(projection, &arena, root_context,)
                .text_area_preedit,
            None
        );
        assert!(arena.children_of(root).into_iter().any(|key| {
            arena.get(key).is_some_and(|node| {
                node.element
                    .as_any()
                    .downcast_ref::<TextAreaTextRun>()
                    .is_some_and(|run| run.is_preedit_run())
            })
        }));
        let (properties, generations) = sync_identity(&arena, &roots);
        let (artifact, eligibility) =
            whole_frame_artifact(&arena, &roots, &properties, &generations);
        assert!(eligibility.eligible);
        assert!(artifact.chunks.iter().any(|chunk| {
            chunk.owner == projected_text && chunk.id.role == PaintChunkRole::TextGlyphs
        }));
        assert!(artifact.chunks.iter().any(|chunk| {
            chunk.owner == root && chunk.id.role == PaintChunkRole::TextDecoration
        }));
    }

    #[test]
    fn text_area_projection_atomic_witness_tamper_fails_before_full_hooks() {
        for case in [
            "live_width",
            "flow_offset",
            "range",
            "source",
            "backing",
            "atomic_missing",
            "atomic_duplicate",
            "measurement_constraint",
            "measurement_size",
            "insertion",
            "orphan",
            "dirty",
            "scroll",
        ] {
            let (mut arena, roots, root, projection, _) = prepared_projection_text_area_tree();
            let projection_index = arena
                .children_of(root)
                .iter()
                .position(|key| *key == projection)
                .unwrap();
            match case {
                "live_width" => {
                    let width = arena
                        .get(projection)
                        .unwrap()
                        .element
                        .box_model_snapshot()
                        .width;
                    arena.with_element_taken(projection, |element, _arena| {
                        element.set_layout_width(width + 1.0);
                    });
                }
                "flow_offset" => {
                    arena.with_element_taken(projection, |element, _arena| {
                        element.set_layout_offset(999.0, 0.0);
                    });
                }
                "range" => {
                    arena
                        .get_mut(projection)
                        .unwrap()
                        .element
                        .as_any_mut()
                        .downcast_mut::<TextAreaProjectionSegment>()
                        .unwrap()
                        .set_char_range(0..1);
                }
                "source" => {
                    arena
                        .get(root)
                        .unwrap()
                        .element
                        .as_any()
                        .downcast_ref::<TextArea>()
                        .unwrap()
                        .tamper_cached_unified_segment_source_for_test(projection_index);
                }
                "backing" => {
                    arena
                        .get(root)
                        .unwrap()
                        .element
                        .as_any()
                        .downcast_ref::<TextArea>()
                        .unwrap()
                        .tamper_cached_unified_segment_backing_range_for_test(
                            projection_index,
                            0..1,
                        );
                }
                "atomic_missing" | "atomic_duplicate" => {
                    arena
                        .get(root)
                        .unwrap()
                        .element
                        .as_any()
                        .downcast_ref::<TextArea>()
                        .unwrap()
                        .tamper_cached_unified_atomic_sources_for_test(case == "atomic_duplicate");
                }
                "measurement_constraint" | "measurement_size" => {
                    arena
                        .get(root)
                        .unwrap()
                        .element
                        .as_any()
                        .downcast_ref::<TextArea>()
                        .unwrap()
                        .tamper_cached_unified_atomic_measurement_for_test(
                            projection_index,
                            case == "measurement_constraint",
                        );
                }
                "insertion" => {
                    arena
                        .get(root)
                        .unwrap()
                        .element
                        .as_any()
                        .downcast_ref::<TextArea>()
                        .unwrap()
                        .tamper_cached_unified_atomic_insertion_for_test(projection_index);
                }
                "orphan" => arena.set_parent(projection, None),
                "dirty" => arena.mark_dirty(projection, DirtyFlags::LAYOUT),
                "scroll" => {
                    arena
                        .get_mut(root)
                        .unwrap()
                        .element
                        .as_any_mut()
                        .downcast_mut::<TextArea>()
                        .unwrap()
                        .scroll_x = 1.0;
                }
                _ => unreachable!(),
            }
            let eligibility = assert_text_area_fallback_before_full(&arena, &roots);
            assert!(!eligibility.eligible, "{case}");
        }
    }

    #[test]
    fn text_area_projection_preedit_topology_and_witness_tamper_fail_closed() {
        let (mut arena, roots, _root, projection, _) =
            prepared_projection_text_area_preedit_tree(8, "中", Some((0, 3)));
        commit_child(
            &mut arena,
            projection,
            Box::new(Text::from_content_with_id(0x7e98, "duplicate")),
        );
        assert_text_area_fallback_before_full(&arena, &roots);

        let (mut arena, roots, _root, projection, projected_text) =
            prepared_projection_text_area_preedit_tree(8, "中", Some((0, 3)));
        let wrapper = commit_child(
            &mut arena,
            projection,
            Box::new(Element::new_with_id(0x7e97, 0.0, 0.0, 1.0, 1.0)),
        );
        arena.set_parent(projected_text, Some(wrapper));
        arena.set_children(wrapper, vec![projected_text]);
        arena.with_element_taken(wrapper, |element, _| {
            element.sync_children_mirror(&[projected_text]);
        });
        arena.set_children(projection, vec![wrapper]);
        arena.with_element_taken(projection, |element, _| {
            element.sync_children_mirror(&[wrapper]);
        });
        assert_text_area_fallback_before_full(&arena, &roots);

        let (arena, _roots, root, projection, projected_text) =
            prepared_projection_text_area_preedit_tree(8, "中", Some((0, 3)));
        let root_context = arena
            .get(root)
            .unwrap()
            .element
            .shadow_paint_recording_context(PaintRecordingContext::default());
        let projection_context = arena
            .get(root)
            .unwrap()
            .element
            .shadow_paint_recording_context_for_child(projection, &arena, root_context);
        let mut text_context = arena
            .get(projection)
            .unwrap()
            .element
            .shadow_paint_recording_context_for_child(projected_text, &arena, projection_context);
        text_context
            .text_area_preedit
            .as_mut()
            .unwrap()
            .target_caret_byte = usize::MAX;
        assert!(
            arena
                .get(projected_text)
                .unwrap()
                .element
                .record_shadow_paint_metadata_plan(
                    projected_text,
                    Default::default(),
                    Default::default(),
                    PaintContentRevision {
                        self_paint_revision: 1,
                        composite_revision: 1,
                        topology_revision: 1,
                    },
                    &arena,
                    text_context,
                )
                .is_none()
        );
    }

    #[test]
    fn text_area_projection_preedit_metadata_full_drift_is_detected() {
        for drift_cursor in [false, true] {
            let (arena, roots, root, _projection, projected_text) =
                prepared_projection_text_area_preedit_tree(8, "中🙂", Some((0, 7)));
            let (properties, generations) = sync_identity(&arena, &roots);
            let metadata = record_coverage_manifest(
                &arena,
                &roots,
                false,
                true,
                CoverageRecordingMode::MetadataOnly,
                &properties,
                &generations,
            );
            if drift_cursor {
                arena
                    .get_mut(root)
                    .unwrap()
                    .element
                    .as_any_mut()
                    .downcast_mut::<TextArea>()
                    .unwrap()
                    .ime_preedit_cursor = Some((0, 3));
            } else {
                arena
                    .get_mut(projected_text)
                    .unwrap()
                    .element
                    .as_any_mut()
                    .downcast_mut::<Text>()
                    .unwrap()
                    .set_text("p中🙂rojected!");
            }
            let full = record_coverage_manifest(
                &arena,
                &roots,
                false,
                true,
                CoverageRecordingMode::FullArtifact,
                &properties,
                &generations,
            );
            assert!(!super::frame_recorder::canonical_manifest_matches(
                &metadata, &full
            ));
        }
    }

    #[test]
    fn text_area_projection_preedit_state_boundaries_fail_closed() {
        for case in ["selection", "scroll"] {
            let (arena, roots, root, ..) =
                prepared_projection_text_area_preedit_tree(8, "中", Some((0, 3)));
            {
                let mut node = arena.get_mut(root).unwrap();
                let text_area = node
                    .element
                    .as_any_mut()
                    .downcast_mut::<TextArea>()
                    .unwrap();
                match case {
                    "selection" => {
                        text_area.selection_anchor_char = Some(7);
                        text_area.selection_focus_char = Some(8);
                    }
                    "scroll" => text_area.scroll_x = 1.0,
                    _ => unreachable!(),
                }
            }
            assert_text_area_fallback_before_full(&arena, &roots);
        }

        let (arena, _roots, _root, projection, _) =
            prepared_projection_text_area_preedit_tree(8, "中", Some((0, 3)));
        let projection_node = arena.get(projection).unwrap();
        assert_eq!(
            projection_node.element.shadow_paint_recording_capability(
                &arena,
                true,
                PaintRecordingContext {
                    inside_text_area: true,
                    ..Default::default()
                },
            ),
            ShadowPaintRecordingCapability::Legacy(ShadowPaintBlocker::Deferred)
        );
        drop(projection_node);
    }

    #[test]
    fn text_area_projection_selection_is_path_scoped_ordered_and_matches_legacy() {
        let selected_fixture = || {
            let (arena, roots, root, projection, projected_text) =
                prepared_projection_text_area_tree();
            {
                let mut node = arena.get_mut(root).unwrap();
                let text_area = node
                    .element
                    .as_any_mut()
                    .downcast_mut::<TextArea>()
                    .unwrap();
                text_area.selection_anchor_char = Some(8);
                text_area.selection_focus_char = Some(15);
            }
            (arena, roots, root, projection, projected_text)
        };
        assert_whole_frame_structural_parity(
            || {
                let (arena, roots, ..) = selected_fixture();
                (arena, roots)
            },
            PaintParityConfig::default(),
        );

        let (arena, roots, root, projection, projected_text) = selected_fixture();
        let root_context = arena
            .get(root)
            .unwrap()
            .element
            .shadow_paint_recording_context(PaintRecordingContext::default());
        let projection_context = arena
            .get(root)
            .unwrap()
            .element
            .shadow_paint_recording_context_for_child(projection, &arena, root_context);
        let witness = projection_context
            .text_area_selection
            .expect("selected projection edge must own a witness");
        assert_eq!((witness.local_start, witness.local_end), (1, 8));
        assert_eq!(witness.target_owner, projected_text);
        assert_eq!(
            witness.target_stable_id,
            arena.get(projected_text).unwrap().element.stable_id()
        );
        for sibling in arena
            .children_of(root)
            .into_iter()
            .filter(|child| *child != projection)
        {
            let sibling_context = arena
                .get(root)
                .unwrap()
                .element
                .shadow_paint_recording_context_for_child(sibling, &arena, root_context);
            assert_eq!(
                sibling_context.text_area_selection, None,
                "selection authority must not leak to a TextArea sibling"
            );
        }
        let wrapper_context = arena
            .get(projection)
            .unwrap()
            .element
            .shadow_paint_recording_context(projection_context);
        let text_context = arena
            .get(projection)
            .unwrap()
            .element
            .shadow_paint_recording_context_for_child(projected_text, &arena, wrapper_context);
        assert_eq!(text_context.text_area_selection, Some(witness));

        let (properties, generations) = sync_identity(&arena, &roots);
        let (artifact, eligibility) =
            crate::view::base_component::with_text_area_selection_render_context(
                Some(
                    crate::view::base_component::TextAreaSelectionRenderContext {
                        start: 0,
                        end: 9,
                        fill: [1.0, 0.0, 0.0, 1.0],
                    },
                ),
                || whole_frame_artifact(&arena, &roots, &properties, &generations),
            );
        assert!(eligibility.eligible);
        assert_eq!(
            artifact
                .chunks
                .iter()
                .map(|chunk| (chunk.owner, chunk.id.slot, chunk.id.role))
                .collect::<Vec<_>>(),
            vec![
                (root, 1, PaintChunkRole::TextGlyphs),
                (projected_text, 0, PaintChunkRole::SelectionUnderlay),
                (projected_text, 1, PaintChunkRole::TextGlyphs),
            ]
        );
        let selection_chunk = artifact
            .chunks
            .iter()
            .find(|chunk| chunk.id.role == PaintChunkRole::SelectionUnderlay)
            .unwrap();
        assert!(
            artifact.ops[selection_chunk.op_range.clone()]
                .iter()
                .all(|op| {
                    matches!(op, PaintOp::DrawRect(op) if op.params.fill_color == witness.fill)
                })
        );

        let selected_glyph_id = artifact
            .chunks
            .iter()
            .find(|chunk| {
                chunk.owner == projected_text && chunk.id.role == PaintChunkRole::TextGlyphs
            })
            .unwrap()
            .id;
        {
            let mut node = arena.get_mut(root).unwrap();
            let text_area = node
                .element
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .unwrap();
            text_area.selection_anchor_char = None;
            text_area.selection_focus_char = None;
        }
        let (properties, generations) = sync_identity(&arena, &roots);
        let (artifact, eligibility) =
            whole_frame_artifact(&arena, &roots, &properties, &generations);
        assert!(eligibility.eligible);
        assert_eq!(
            artifact
                .chunks
                .iter()
                .find(|chunk| {
                    chunk.owner == projected_text && chunk.id.role == PaintChunkRole::TextGlyphs
                })
                .unwrap()
                .id,
            selected_glyph_id,
            "projection Text glyph identity must not move when selection toggles"
        );
    }

    #[test]
    fn text_area_atomic_projection_disjoint_root_selection_is_ordered_and_matches_legacy() {
        let fixture = || {
            let (arena, roots, root, projection, projected_text) =
                prepared_projection_text_area_tree();
            {
                let mut node = arena.get_mut(root).unwrap();
                let text_area = node
                    .element
                    .as_any_mut()
                    .downcast_mut::<TextArea>()
                    .unwrap();
                text_area.selection_anchor_char = Some(0);
                text_area.selection_focus_char = Some(6);
            }
            (arena, roots, root, projection, projected_text)
        };
        assert_whole_frame_structural_parity(
            || {
                let (arena, roots, ..) = fixture();
                (arena, roots)
            },
            PaintParityConfig::default(),
        );

        let (arena, roots, root, projection, projected_text) = fixture();
        let (properties, generations) = sync_identity(&arena, &roots);
        let (artifact, eligibility) =
            whole_frame_artifact(&arena, &roots, &properties, &generations);
        assert!(eligibility.eligible, "{eligibility:?}");
        assert_eq!(
            artifact
                .chunks
                .iter()
                .map(|chunk| (chunk.owner, chunk.id.slot, chunk.id.role))
                .collect::<Vec<_>>(),
            vec![
                (root, 0, PaintChunkRole::SelectionUnderlay),
                (root, 1, PaintChunkRole::TextGlyphs),
                (projected_text, 1, PaintChunkRole::TextGlyphs),
            ],
            "root-owned selection must precede both root and projection glyphs",
        );
        let root_context = arena
            .get(root)
            .unwrap()
            .element
            .shadow_paint_recording_context(PaintRecordingContext::default());
        let projection_context = arena
            .get(root)
            .unwrap()
            .element
            .shadow_paint_recording_context_for_child(projection, &arena, root_context);
        assert_eq!(
            projection_context.text_area_selection, None,
            "disjoint root selection must not mint projection-owned authority",
        );
    }

    #[test]
    fn text_area_selection_crossing_projection_is_split_between_root_and_child() {
        let fixture = || {
            let (arena, roots, root, projection, projected_text) =
                prepared_projection_text_area_tree();
            {
                let mut node = arena.get_mut(root).unwrap();
                let text_area = node
                    .element
                    .as_any_mut()
                    .downcast_mut::<TextArea>()
                    .unwrap();
                text_area.selection_anchor_char = Some(0);
                text_area.selection_focus_char = Some(10);
            }
            (arena, roots, root, projection, projected_text)
        };
        assert_whole_frame_structural_parity(
            || {
                let (arena, roots, ..) = fixture();
                (arena, roots)
            },
            PaintParityConfig::default(),
        );

        let (arena, roots, root, projection, projected_text) = fixture();
        let (properties, generations) = sync_identity(&arena, &roots);
        let (artifact, eligibility) =
            whole_frame_artifact(&arena, &roots, &properties, &generations);
        assert!(eligibility.eligible, "{eligibility:?}");
        assert!(artifact.chunks.iter().any(|chunk| {
            chunk.owner == root && chunk.id.role == PaintChunkRole::SelectionUnderlay
        }));
        assert!(artifact.chunks.iter().any(|chunk| {
            chunk.owner == projected_text && chunk.id.role == PaintChunkRole::SelectionUnderlay
        }));

        let root_context = arena
            .get(root)
            .unwrap()
            .element
            .shadow_paint_recording_context(PaintRecordingContext::default());
        let projection_context = arena
            .get(root)
            .unwrap()
            .element
            .shadow_paint_recording_context_for_child(projection, &arena, root_context);
        let witness = projection_context
            .text_area_selection
            .expect("crossing selection must mint projection-local authority");
        assert_eq!(witness.local_start, 0);
        assert_eq!(witness.local_end, 3);
    }

    #[test]
    fn text_area_projection_selection_utf8_local_range_and_metadata_full_identity_are_exact() {
        let utf8_fixture = || {
            let (arena, roots, root, projection, projected_text) =
                prepared_projection_text_area_tree_with("前🙂投影中文後", 2..6, "投影中文");
            {
                let mut node = arena.get_mut(root).unwrap();
                let text_area = node
                    .element
                    .as_any_mut()
                    .downcast_mut::<TextArea>()
                    .unwrap();
                text_area.selection_anchor_char = Some(3);
                text_area.selection_focus_char = Some(5);
            }
            (arena, roots, root, projection, projected_text)
        };
        assert_whole_frame_structural_parity(
            || {
                let (arena, roots, ..) = utf8_fixture();
                (arena, roots)
            },
            PaintParityConfig::default(),
        );
        let (arena, _roots, root, projection, _) = utf8_fixture();
        let root_context = arena
            .get(root)
            .unwrap()
            .element
            .shadow_paint_recording_context(PaintRecordingContext::default());
        let witness = arena
            .get(root)
            .unwrap()
            .element
            .shadow_paint_recording_context_for_child(projection, &arena, root_context)
            .text_area_selection
            .unwrap();
        assert_eq!((witness.local_start, witness.local_end), (1, 3));

        for mutate_fill in [false, true] {
            let (arena, roots, root, ..) = utf8_fixture();
            let (properties, generations) = sync_identity(&arena, &roots);
            let metadata = record_coverage_manifest(
                &arena,
                &roots,
                false,
                true,
                CoverageRecordingMode::MetadataOnly,
                &properties,
                &generations,
            );
            {
                let mut node = arena.get_mut(root).unwrap();
                let text_area = node
                    .element
                    .as_any_mut()
                    .downcast_mut::<TextArea>()
                    .unwrap();
                if mutate_fill {
                    text_area.selection_background_color = Color::rgba(240, 32, 80, 128);
                } else {
                    text_area.selection_anchor_char = Some(2);
                    text_area.selection_focus_char = Some(4);
                }
            }
            let full = record_coverage_manifest(
                &arena,
                &roots,
                false,
                true,
                CoverageRecordingMode::FullArtifact,
                &properties,
                &generations,
            );
            assert!(
                !super::frame_recorder::canonical_manifest_matches(&metadata, &full),
                "metadata/full must detect {} drift",
                if mutate_fill { "fill" } else { "local range" }
            );
        }
    }

    #[test]
    fn text_area_projection_selection_ambiguous_owner_and_witness_tamper_fail_closed() {
        let (mut arena, roots, root, projection, _) = prepared_projection_text_area_tree();
        {
            let mut node = arena.get_mut(root).unwrap();
            let text_area = node
                .element
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .unwrap();
            text_area.selection_anchor_char = Some(8);
            text_area.selection_focus_char = Some(15);
        }
        commit_child(
            &mut arena,
            projection,
            Box::new(Text::from_content_with_id(0x7e99, "projected")),
        );
        let mut stack = vec![root];
        while let Some(key) = stack.pop() {
            stack.extend(arena.children_of(key));
            arena
                .get_mut(key)
                .unwrap()
                .element
                .clear_local_dirty_flags(DirtyFlags::ALL);
        }
        arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
        arena.refresh_subtree_dirty_cache(root);
        assert_text_area_fallback_before_full(&arena, &roots);

        let (arena, _roots, root, projection, projected_text) =
            prepared_projection_text_area_tree();
        {
            let mut node = arena.get_mut(root).unwrap();
            let text_area = node
                .element
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .unwrap();
            text_area.selection_anchor_char = Some(8);
            text_area.selection_focus_char = Some(15);
        }
        let root_context = arena
            .get(root)
            .unwrap()
            .element
            .shadow_paint_recording_context(PaintRecordingContext::default());
        let projection_context = arena
            .get(root)
            .unwrap()
            .element
            .shadow_paint_recording_context_for_child(projection, &arena, root_context);
        let wrapper_context = arena
            .get(projection)
            .unwrap()
            .element
            .shadow_paint_recording_context(projection_context);
        let text_context = arena
            .get(projection)
            .unwrap()
            .element
            .shadow_paint_recording_context_for_child(projected_text, &arena, wrapper_context);
        for tamper_stable_id in [false, true] {
            let mut tampered = text_context;
            let witness = tampered.text_area_selection.as_mut().unwrap();
            if tamper_stable_id {
                witness.target_stable_id = witness.target_stable_id.wrapping_add(1);
            } else {
                witness.local_end = usize::MAX;
            }
            assert!(
                arena
                    .get(projected_text)
                    .unwrap()
                    .element
                    .record_shadow_paint_metadata_plan(
                        projected_text,
                        Default::default(),
                        Default::default(),
                        PaintContentRevision {
                            self_paint_revision: 1,
                            composite_revision: 1,
                            topology_revision: 1,
                        },
                        &arena,
                        tampered,
                    )
                    .is_none(),
                "tampered projection selection witness must fail closed"
            );
        }
    }

    #[test]
    fn text_area_projection_selection_visibility_gate_prevents_artifact_only_underlay() {
        for case in ["should_render", "opacity_zero"] {
            let fixture = || {
                let (arena, roots, root, _projection, projected_text) =
                    prepared_projection_text_area_tree();
                {
                    let mut node = arena.get_mut(root).unwrap();
                    let text_area = node
                        .element
                        .as_any_mut()
                        .downcast_mut::<TextArea>()
                        .unwrap();
                    text_area.selection_anchor_char = Some(8);
                    text_area.selection_focus_char = Some(15);
                }
                {
                    let mut node = arena.get_mut(projected_text).unwrap();
                    let text = node.element.as_any_mut().downcast_mut::<Text>().unwrap();
                    match case {
                        "should_render" => text.set_should_render_for_test(false),
                        "opacity_zero" => text.set_opacity(0.0),
                        _ => unreachable!(),
                    }
                }
                (arena, roots, root, projected_text)
            };
            assert_whole_frame_structural_parity(
                || {
                    let (arena, roots, ..) = fixture();
                    (arena, roots)
                },
                PaintParityConfig::default(),
            );
            let (arena, roots, _root, projected_text) = fixture();
            let (properties, generations) = sync_identity(&arena, &roots);
            let (artifact, eligibility) =
                whole_frame_artifact(&arena, &roots, &properties, &generations);
            assert!(eligibility.eligible, "{case}");
            assert!(
                artifact.chunks.iter().all(|chunk| {
                    chunk.owner != projected_text
                        && chunk.id.role != PaintChunkRole::SelectionUnderlay
                }),
                "{case} must not emit a projected Text glyph or selection underlay"
            );
        }

        let (arena, roots, standalone_text) = prepared_text_tree(false);
        arena
            .get_mut(standalone_text)
            .unwrap()
            .element
            .as_any_mut()
            .downcast_mut::<Text>()
            .unwrap()
            .set_should_render_for_test(false);
        assert_eq!(
            arena
                .get(standalone_text)
                .unwrap()
                .element
                .shadow_paint_recording_capability(
                    &arena,
                    false,
                    PaintRecordingContext::default(),
                ),
            ShadowPaintRecordingCapability::Transparent,
            "standalone invisible Text must close as transparent coverage"
        );
        let (properties, generations) = sync_identity(&arena, &roots);
        let (artifact, eligibility) =
            whole_frame_artifact(&arena, &roots, &properties, &generations);
        assert!(eligibility.eligible);
        assert!(
            artifact.chunks.is_empty(),
            "standalone invisible Text must not emit a typed zero-op chunk"
        );

        let (arena, roots, root, projection, projected_text) = prepared_projection_text_area_tree();
        {
            let mut node = arena.get_mut(root).unwrap();
            let text_area = node
                .element
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .unwrap();
            text_area.selection_anchor_char = Some(8);
            text_area.selection_focus_char = Some(15);
        }
        let root_context = arena
            .get(root)
            .unwrap()
            .element
            .shadow_paint_recording_context(PaintRecordingContext::default());
        let projection_context = arena
            .get(root)
            .unwrap()
            .element
            .shadow_paint_recording_context_for_child(projection, &arena, root_context);
        let wrapper_context = arena
            .get(projection)
            .unwrap()
            .element
            .shadow_paint_recording_context(projection_context);
        let text_context = arena
            .get(projection)
            .unwrap()
            .element
            .shadow_paint_recording_context_for_child(projected_text, &arena, wrapper_context);
        arena
            .get_mut(projected_text)
            .unwrap()
            .element
            .as_any_mut()
            .downcast_mut::<Text>()
            .unwrap()
            .set_text("");
        assert_eq!(
            arena
                .get(projected_text)
                .unwrap()
                .element
                .shadow_paint_recording_capability(&arena, false, text_context),
            ShadowPaintRecordingCapability::Transparent,
            "empty content must close the shared Text paint gate before selection"
        );
        assert_text_area_fallback_before_full(&arena, &roots);
    }

    #[test]
    fn text_area_projection_deferred_and_invalid_scroll_boundaries_remain_fail_closed() {
        let (arena, _roots, root, projection, _) = prepared_projection_text_area_tree();
        {
            let mut node = arena.get_mut(root).unwrap();
            let text_area = node
                .element
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .unwrap();
            text_area.selection_anchor_char = Some(8);
            text_area.selection_focus_char = Some(15);
        }
        let node = arena.get(projection).unwrap();
        assert_eq!(
            node.element.shadow_paint_recording_capability(
                &arena,
                true,
                PaintRecordingContext {
                    inside_text_area: true,
                    ..Default::default()
                },
            ),
            ShadowPaintRecordingCapability::Legacy(ShadowPaintBlocker::Deferred)
        );
        drop(node);

        let (arena, roots, root, ..) = prepared_projection_text_area_tree();
        {
            let mut node = arena.get_mut(root).unwrap();
            let text_area = node
                .element
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .unwrap();
            text_area.selection_anchor_char = Some(8);
            text_area.selection_focus_char = Some(15);
            text_area.scroll_x = 1.0;
        }
        assert_text_area_fallback_before_full(&arena, &roots);
    }

    #[test]
    fn plain_text_area_placeholder_newline_fractional_and_empty_cases_match_legacy() {
        for (content, placeholder, width, origin) in [
            ("", "placeholder text", 108.0, [7.25, 11.75]),
            ("first\nsecond", "", 108.0, [7.25, 11.75]),
            (
                "soft wrapping text across several visual lines",
                "",
                64.0,
                [13.375, 17.625],
            ),
        ] {
            let config = PaintParityConfig::default();
            assert_whole_frame_structural_parity(
                || {
                    let (arena, roots, _) =
                        prepared_plain_text_area_tree_with(content, placeholder, width, origin);
                    (arena, roots)
                },
                config,
            );
        }

        let (arena, roots, root) = prepared_plain_text_area_tree("");
        let (properties, generations) = sync_identity(&arena, &roots);
        take_full_artifact_record_count();
        let (artifact, eligibility) =
            whole_frame_artifact(&arena, &roots, &properties, &generations);
        assert!(eligibility.eligible);
        assert!(artifact.chunks.is_empty());
        assert!(artifact.ops.is_empty());
        assert_eq!(take_full_artifact_record_count(), 0);
        assert!(arena.children_of(root).is_empty());
    }

    #[test]
    fn plain_text_area_unsafe_stateful_states_fail_before_full_hooks() {
        for case in ["selection_mixed", "preedit", "scroll"] {
            let (arena, roots, root) = prepared_plain_text_area_tree("state matrix");
            {
                let mut node = arena.get_mut(root).unwrap();
                let text_area = node
                    .element
                    .as_any_mut()
                    .downcast_mut::<TextArea>()
                    .unwrap();
                match case {
                    "selection_mixed" => {
                        text_area.selection_anchor_char = Some(0);
                        text_area.selection_focus_char = None;
                    }
                    "preedit" => text_area.ime_preedit = "x".to_string(),
                    "scroll" => text_area.scroll_x = -0.0,
                    _ => unreachable!(),
                }
            }
            let eligibility = assert_text_area_fallback_before_full(&arena, &roots);
            assert!(!eligibility.eligible, "{case}");
        }
    }

    #[test]
    fn plain_text_area_paint_neutral_transient_states_remain_recordable() {
        for case in [
            "pointer",
            "pending_scroll",
            "realized_zero_projection_handler",
        ] {
            let (arena, roots, root) = prepared_plain_text_area_tree("state matrix");
            {
                let mut node = arena.get_mut(root).unwrap();
                let text_area = node
                    .element
                    .as_any_mut()
                    .downcast_mut::<TextArea>()
                    .unwrap();
                match case {
                    "pointer" => text_area.pointer_selecting = true,
                    "pending_scroll" => text_area.pending_caret_scroll = true,
                    "realized_zero_projection_handler" => {
                        text_area.on_render_handler =
                            Some(crate::ui::on_text_area_render(|_render| {}));
                    }
                    _ => unreachable!(),
                }
            }
            let (properties, generations) = sync_identity(&arena, &roots);
            let (artifact, eligibility) =
                whole_frame_artifact(&arena, &roots, &properties, &generations);
            assert!(eligibility.eligible, "{case}: {eligibility:?}");
            assert!(artifact.chunks.iter().any(|chunk| {
                chunk.owner == root && chunk.id.role == PaintChunkRole::TextGlyphs
            }));
        }
    }

    #[test]
    fn plain_text_area_stale_dirty_topology_and_range_drift_fail_closed() {
        let (arena, roots, root) = prepared_plain_text_area_tree("stale package");
        arena
            .get_mut(root)
            .unwrap()
            .element
            .as_any_mut()
            .downcast_mut::<TextArea>()
            .unwrap()
            .bump_unified_ifc_source_revision();
        assert_text_area_fallback_before_full(&arena, &roots);

        let (arena, roots, root) = prepared_plain_text_area_tree("direct mutation");
        let child = arena.children_of(root)[0];
        arena
            .get_mut(child)
            .unwrap()
            .element
            .as_any_mut()
            .downcast_mut::<TextAreaTextRun>()
            .unwrap()
            .text
            .push('!');
        assert_text_area_fallback_before_full(&arena, &roots);

        let (arena, roots, root) = prepared_plain_text_area_tree("range drift");
        let child = arena.children_of(root)[0];
        let wrong = 1..12;
        {
            let mut child_node = arena.get_mut(child).unwrap();
            child_node
                .element
                .as_any_mut()
                .downcast_mut::<TextAreaTextRun>()
                .unwrap()
                .char_range = wrong.clone();
        }
        {
            let mut root_node = arena.get_mut(root).unwrap();
            let text_area = root_node
                .element
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .unwrap();
            text_area.child_char_ranges[0] = wrong.clone();
            text_area.tamper_cached_unified_segment_char_range_for_test(0, wrong);
        }
        assert_text_area_fallback_before_full(&arena, &roots);

        let (arena, roots, root) = prepared_plain_text_area_tree("dirty child");
        let child = arena.children_of(root)[0];
        arena.mark_dirty(child, DirtyFlags::LAYOUT);
        assert_text_area_fallback_before_full(&arena, &roots);

        let (mut arena, roots, root) = prepared_plain_text_area_tree("orphan child");
        let child = arena.children_of(root)[0];
        arena.set_parent(child, None);
        assert_text_area_fallback_before_full(&arena, &roots);

        let (mut arena, roots, root) = prepared_plain_text_area_tree("wrong parent");
        let child = arena.children_of(root)[0];
        let wrong_parent = commit_element(
            &mut arena,
            Box::new(Element::new_with_id(0x7eff, 0.0, 0.0, 10.0, 10.0)),
        );
        arena.set_parent(child, Some(wrong_parent));
        assert_text_area_fallback_before_full(&arena, &roots);
    }

    #[test]
    fn plain_text_area_live_empty_ignores_stale_package_and_apply_authority() {
        let (mut arena, roots, root) = prepared_plain_text_area_tree("clear me");
        arena.with_element_taken(root, |element, _arena| {
            element
                .as_any_mut()
                .downcast_mut::<TextArea>()
                .unwrap()
                .set_text(String::new());
        });
        let (measure, place) = constraints();
        measure_and_place(&mut arena, root, measure, place);
        settle_plain_text_area(&arena, root);
        let text_area_node = arena.get(root).unwrap();
        let text_area = text_area_node
            .element
            .as_any()
            .downcast_ref::<TextArea>()
            .unwrap();
        assert!(text_area.last_unified_apply.get().is_some());
        let capability = text_area.shadow_paint_recording_capability(
            &arena,
            false,
            PaintRecordingContext::default(),
        );
        assert_eq!(
            capability,
            ShadowPaintRecordingCapability::Transparent,
            "children={:?} children_dirty={} local={:?} arena={:?} pending={}",
            text_area.children,
            text_area.children_dirty,
            text_area.dirty_flags,
            arena.arena_local_dirty(root),
            text_area.pending_caret_scroll,
        );
        drop(text_area_node);

        let (properties, generations) = sync_identity(&arena, &roots);
        take_full_artifact_record_count();
        let (artifact, eligibility) =
            whole_frame_artifact(&arena, &roots, &properties, &generations);
        assert!(eligibility.eligible);
        assert!(artifact.chunks.is_empty());
        assert!(artifact.ops.is_empty());
        assert_eq!(take_full_artifact_record_count(), 0);
    }

    #[test]
    fn text_area_leaf_deferred_or_wrong_context_never_turns_transparent() {
        let (arena, _roots, root) = prepared_plain_text_area_tree("boundary");
        let root_node = arena.get(root).unwrap();
        let text_area = root_node
            .element
            .as_any()
            .downcast_ref::<TextArea>()
            .unwrap();
        assert_eq!(
            text_area.shadow_paint_recording_capability(
                &arena,
                true,
                PaintRecordingContext::default(),
            ),
            ShadowPaintRecordingCapability::Legacy(ShadowPaintBlocker::Deferred)
        );
        drop(root_node);

        let mut standalone = new_test_arena();
        let run = commit_element(
            &mut standalone,
            Box::new(TextAreaTextRun::new("orphan".to_string(), 0..6)),
        );
        let run_node = standalone.get(run).unwrap();
        assert_eq!(
            run_node.element.shadow_paint_recording_capability(
                &standalone,
                false,
                PaintRecordingContext::default(),
            ),
            ShadowPaintRecordingCapability::Unsupported
        );
    }

    fn hidden_element_subtree(root_id: u64, child_id: u64) -> (NodeArena, NodeKey, NodeKey) {
        let mut arena = new_test_arena();
        let root = commit_element(
            &mut arena,
            Box::new(Element::new_with_id(root_id, 0.0, 0.0, 0.0, 10.0)),
        );
        let (measure, place) = constraints();
        measure_and_place(&mut arena, root, measure, place);
        assert!(
            !arena
                .get(root)
                .unwrap()
                .element
                .box_model_snapshot()
                .should_render
        );
        let child = commit_child(
            &mut arena,
            root,
            Box::new(leaf_element(child_id, Color::rgb(220, 40, 30), 1.0, false)),
        );
        measure_and_place(&mut arena, child, measure, place);
        assert!(
            arena
                .get(child)
                .unwrap()
                .element
                .box_model_snapshot()
                .should_render
        );
        (arena, root, child)
    }

    #[test]
    fn whole_frame_should_render_false_culls_the_complete_subtree() {
        let (arena, root, child) = hidden_element_subtree(130, 131);
        let (properties, generations) = sync_identity(&arena, &[root]);
        assert_eq!(
            arena
                .get(root)
                .unwrap()
                .element
                .shadow_paint_recording_capability(
                    &arena,
                    false,
                    PaintRecordingContext::default(),
                ),
            ShadowPaintRecordingCapability::CulledSubtree
        );
        let FrameArtifactRecordOutcome::Artifact {
            artifact,
            eligibility,
        } = record_frame_artifact(
            &arena,
            &[root],
            &properties,
            &generations,
            RendererMode::ForcedForTests,
        )
        .expect("effect-neutral hidden subtree is fully covered")
        else {
            panic!("forced recording cannot silently fall back")
        };
        assert!(eligibility.eligible, "{eligibility:?}");
        assert!(artifact.chunks.is_empty());
        assert!(artifact.ops.is_empty());

        let manifest = record_coverage_manifest(
            &arena,
            &[root],
            false,
            true,
            CoverageRecordingMode::MetadataOnly,
            &properties,
            &generations,
        );
        assert!(matches!(
            manifest.items.as_slice(),
            [PaintCoverageItem::CulledSubtree { owner, .. }] if *owner == root
        ));
        assert!(manifest.items.iter().all(
            |item| !matches!(item, PaintCoverageItem::ArtifactChunk { chunk, .. } if chunk.owner == child)
        ));
        let stats = manifest.stats();
        assert_eq!(stats.culled_subtrees, 1);
        assert_eq!(stats.artifact_chunks, 0);
    }

    #[test]
    fn culled_subtree_multi_root_keeps_only_the_visible_root_artifact() {
        let (mut arena, hidden, hidden_child) = hidden_element_subtree(140, 141);
        let visible = commit_element(
            &mut arena,
            Box::new(leaf_element(142, Color::rgb(20, 90, 220), 1.0, false)),
        );
        let (measure, place) = constraints();
        measure_and_place(&mut arena, visible, measure, place);
        let roots = [hidden, visible];
        let (properties, generations) = sync_identity(&arena, &roots);
        let FrameArtifactRecordOutcome::Artifact {
            artifact,
            eligibility,
        } = record_clip_enabled_frame_artifact(
            &arena,
            &roots,
            &properties,
            &generations,
            RendererMode::ForcedForTests,
        )
        .expect("effect-neutral multi-root frame is recordable")
        else {
            panic!("forced recording cannot silently fall back")
        };
        assert!(eligibility.eligible, "{eligibility:?}");
        assert!(!artifact.chunks.is_empty());
        assert!(artifact.chunks.iter().all(|chunk| chunk.owner == visible));
        assert!(
            artifact
                .chunks
                .iter()
                .all(|chunk| chunk.owner != hidden && chunk.owner != hidden_child)
        );
    }

    #[test]
    fn culled_subtree_metadata_full_parity_and_topology_revision_are_canonical() {
        let (mut arena, root, child) = hidden_element_subtree(150, 151);
        let mut properties = PropertyTrees::default();
        properties.sync(&arena, &[root]);
        let mut generations = PaintGenerationTracker::default();
        generations.sync(&arena, &[root], &properties);
        let record = |mode, properties: &PropertyTrees, generations: &PaintGenerationTracker| {
            record_coverage_manifest(&arena, &[root], false, true, mode, properties, generations)
        };
        let metadata = record(
            CoverageRecordingMode::MetadataOnly,
            &properties,
            &generations,
        );
        let mut full = record(
            CoverageRecordingMode::FullArtifact,
            &properties,
            &generations,
        );
        assert!(super::frame_recorder::canonical_manifest_matches(
            &metadata, &full
        ));
        let baseline_topology = match &metadata.items[0] {
            PaintCoverageItem::CulledSubtree {
                content_revision, ..
            } => content_revision.topology_revision,
            item => panic!("expected culled subtree, got {item:?}"),
        };
        let PaintCoverageItem::CulledSubtree {
            content_revision, ..
        } = &mut full.items[0]
        else {
            unreachable!()
        };
        content_revision.topology_revision = content_revision.topology_revision.wrapping_add(1);
        assert!(!super::frame_recorder::canonical_manifest_matches(
            &metadata, &full
        ));

        arena
            .get_mut(child)
            .unwrap()
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .unwrap()
            .set_background_color_value(Color::rgb(10, 240, 80));
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
        let child_mutated = record_coverage_manifest(
            &arena,
            &[root],
            false,
            true,
            CoverageRecordingMode::MetadataOnly,
            &properties,
            &generations,
        );
        assert!(matches!(
            child_mutated.items.as_slice(),
            [PaintCoverageItem::CulledSubtree { owner, .. }] if *owner == root
        ));

        let added = commit_child(
            &mut arena,
            root,
            Box::new(leaf_element(152, Color::rgb(180, 30, 160), 1.0, false)),
        );
        let (measure, place) = constraints();
        measure_and_place(&mut arena, added, measure, place);
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
        let topology_changed = record_coverage_manifest(
            &arena,
            &[root],
            false,
            true,
            CoverageRecordingMode::MetadataOnly,
            &properties,
            &generations,
        );
        let changed_topology = match &topology_changed.items[0] {
            PaintCoverageItem::CulledSubtree {
                content_revision, ..
            } => content_revision.topology_revision,
            item => panic!("expected culled subtree, got {item:?}"),
        };
        assert_ne!(changed_topology, baseline_topology);
    }

    #[test]
    fn culled_subtree_keeps_root_effect_and_deferred_fail_closed() {
        let (arena, root, _) = hidden_element_subtree(156, 157);
        let mut transform_style = Style::new();
        transform_style.set_transform(Transform::new([Translate::x(Length::px(3.0))]));
        arena
            .get_mut(root)
            .unwrap()
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .unwrap()
            .apply_style(transform_style);
        assert_eq!(
            arena
                .get(root)
                .unwrap()
                .element
                .shadow_paint_recording_capability(
                    &arena,
                    false,
                    PaintRecordingContext::default(),
                ),
            ShadowPaintRecordingCapability::Legacy(ShadowPaintBlocker::Transform)
        );

        let (arena, root, _) = hidden_element_subtree(158, 159);
        let mut scroll_style = Style::new();
        scroll_style.insert(
            PropertyId::ScrollDirection,
            ParsedValue::ScrollDirection(crate::style::ScrollDirection::Vertical),
        );
        arena
            .get_mut(root)
            .unwrap()
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .unwrap()
            .apply_style(scroll_style);
        assert_eq!(
            arena
                .get(root)
                .unwrap()
                .element
                .shadow_paint_recording_capability(
                    &arena,
                    false,
                    PaintRecordingContext::default(),
                ),
            ShadowPaintRecordingCapability::Legacy(ShadowPaintBlocker::ScrollContainer)
        );

        let (arena, root, child) = hidden_element_subtree(166, 167);
        arena
            .get_mut(child)
            .unwrap()
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .unwrap()
            .set_opacity(0.5);
        let (properties, generations) = sync_identity(&arena, &[root]);
        take_full_artifact_record_count();
        let child_effect = record_frame_artifact(
            &arena,
            &[root],
            &properties,
            &generations,
            RendererMode::ForcedForTests,
        )
        .unwrap_err();
        assert!(child_effect.reasons.iter().any(|reason| matches!(
            reason,
            FrameArtifactFallbackReason::LegacyBoundary(LegacyPaintReason::StatefulPaint)
        )));
        assert_eq!(take_full_artifact_record_count(), 0);

        let (arena, root, child) = hidden_element_subtree(162, 163);
        let mut deferred_style = Style::new();
        deferred_style.insert(
            PropertyId::Position,
            ParsedValue::Position(Position::absolute().clip(ClipMode::Viewport)),
        );
        arena
            .get_mut(child)
            .unwrap()
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .unwrap()
            .apply_style(deferred_style);
        let (properties, generations) = sync_identity(&arena, &[root]);
        take_full_artifact_record_count();
        let deferred = record_clip_enabled_frame_artifact(
            &arena,
            &[root],
            &properties,
            &generations,
            RendererMode::ForcedForTests,
        )
        .unwrap_err();
        assert!(deferred.reasons.iter().any(|reason| matches!(
            reason,
            FrameArtifactFallbackReason::LegacyBoundary(LegacyPaintReason::Deferred)
        )));
        assert_eq!(take_full_artifact_record_count(), 0);

        let (arena, root, _) = hidden_element_subtree(164, 165);
        arena
            .get_mut(root)
            .unwrap()
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .unwrap()
            .set_opacity(0.5);
        let (properties, generations) = sync_identity(&arena, &[root]);
        take_full_artifact_record_count();
        let effect = record_root_group_opacity_frame_artifact(
            &arena,
            &[root],
            &properties,
            &generations,
            RendererMode::ForcedForTests,
        )
        .unwrap_err();
        assert!(effect.reasons.iter().any(|reason| matches!(
            reason,
            FrameArtifactFallbackReason::LegacyBoundary(LegacyPaintReason::StatefulPaint)
                | FrameArtifactFallbackReason::PropertyBoundary(_)
        )));
        assert_eq!(take_full_artifact_record_count(), 0);
    }
}
