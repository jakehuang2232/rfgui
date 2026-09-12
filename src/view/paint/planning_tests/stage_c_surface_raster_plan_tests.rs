use super::*;
use crate::view::paint::compiler::ArtifactSurfaceResolvedClip;
use crate::view::paint::{
    ArtifactSurfaceCompositeGeometryStamp, ArtifactSurfaceLocalizationError,
    ArtifactSurfacePaintOpKind, ArtifactSurfaceRasterContext, ArtifactSurfaceRasterPlanError,
    FrameArtifactRecordOutcome, PreparedArtifactSurfaceRasterStep,
    PreparedInlineIfcDecorationDescriptor, PreparedInlineIfcDecorationOp,
    PreparedScrollbarOverlayOp, RendererMode, SurfaceDagExecutionTargetId,
    localize_artifact_surface_op, prepare_artifact_surface_raster_plan,
    record_closed_single_target_frame_artifact,
};
use crate::view::render_pass::render_target::GraphicsPassScissor;

pub(super) fn scroll_surface_artifact() -> PaintArtifact {
    use crate::view::base_component::{
        ScrollAxisSnapshot, ScrollContentsClipWitness, ScrollbarInteractionWitness,
        ScrollbarOverlayWitness, ScrollbarPaintStateWitness,
    };
    use crate::view::compositor::property_tree::{
        ClipBehavior, ClipNodeId, ClipNodeRole, ClipNodeSnapshot, ScrollNodeId, ScrollNodeSnapshot,
    };

    let mut arena = new_test_arena();
    let element = |stable_id, color: Option<Color>| {
        let mut element = Element::new_with_id(stable_id, 0.0, 0.0, 100.0, 100.0);
        let mut style = Style::new();
        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        if let Some(color) = color {
            style.insert(PropertyId::BackgroundColor, ParsedValue::color_like(color));
        }
        element.apply_style(style);
        element
    };
    let root = commit_element(&mut arena, Box::new(element(0xc3_b200, None)));
    let child = commit_child(
        &mut arena,
        root,
        Box::new(element(0xc3_b201, Some(Color::rgb(20, 80, 160)))),
    );
    measure_and_place(
        &mut arena,
        root,
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
    );
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[root]);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[root], &properties);
    let FrameArtifactRecordOutcome::Artifact { mut artifact, .. } =
        record_closed_single_target_frame_artifact(
            &arena,
            &[root],
            &properties,
            &generations,
            RendererMode::ForcedForTests,
        )
        .expect("plain scroll fixture must be recordable")
    else {
        panic!("forced scroll fixture cannot silently fall back")
    };

    let scroll = ScrollNodeId(root);
    let contents_clip = ClipNodeId {
        owner: root,
        role: ClipNodeRole::ContentsClip,
    };
    let state = PropertyTreeState {
        clip: Some(contents_clip),
        scroll: Some(scroll),
        ..Default::default()
    };
    artifact.clip_nodes.push(ClipNodeSnapshot {
        id: contents_clip,
        owner: root,
        parent: None,
        logical_scissor: [0, 0, 100, 100],
        behavior: ClipBehavior::Intersect,
        generation: 7,
    });
    artifact.scroll_nodes.push(ScrollNodeSnapshot {
        id: scroll,
        owner: root,
        parent: None,
        offset: glam::Vec2::new(7.0, 11.0),
        configured_axis: ScrollAxisSnapshot::Vertical,
        viewport: Rect {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
        },
        content_size: Size {
            width: 200.0,
            height: 200.0,
        },
        layout_content_bounds_at_zero: Rect {
            x: 0.0,
            y: 0.0,
            width: 200.0,
            height: 200.0,
        },
        scrollbar_overlay: ScrollbarOverlayWitness {
            vertical_track: None,
            vertical_thumb: None,
            horizontal_track: None,
            horizontal_thumb: None,
            interaction: ScrollbarInteractionWitness {
                hovered: false,
                dragging_axis: None,
                has_interaction_timestamp: false,
            },
            paint_state: ScrollbarPaintStateWitness::NotPaintable,
            sampled_alpha: 0.0,
            shadow_blur_radius: 0.0,
        },
        contents_clip: ScrollContentsClipWitness::ExactRect([0, 0, 100, 100]),
        generation: 9,
    });
    let root_endpoints = artifact
        .owner_property_states
        .iter_mut()
        .find(|snapshot| snapshot.owner == root)
        .expect("root endpoints");
    root_endpoints.paint = PropertyTreeState::default();
    root_endpoints.descendants = state;
    let child_endpoints = artifact
        .owner_property_states
        .iter_mut()
        .find(|snapshot| snapshot.owner == child)
        .expect("child endpoints");
    child_endpoints.paint = state;
    child_endpoints.descendants = state;
    for chunk in &mut artifact.chunks {
        chunk.properties = if chunk.owner == child {
            state
        } else {
            PropertyTreeState::default()
        };
    }
    artifact
}

fn effect_depth_artifact(effect_depth: usize) -> PaintArtifact {
    use crate::view::compositor::property_tree::{EffectNodeId, EffectNodeSnapshot};

    assert!(effect_depth > 0);

    let mut arena = new_test_arena();
    let element = |stable_id| {
        let mut element = Element::new_with_id(stable_id, 0.0, 0.0, 100.0, 100.0);
        let mut style = Style::new();
        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        element.apply_style(style);
        element
    };
    let root = commit_element(&mut arena, Box::new(element(0xc3_b100)));
    let mut owners = vec![root];
    for offset in 1..=effect_depth + 1 {
        let parent = *owners.last().expect("depth chain parent");
        owners.push(commit_child(
            &mut arena,
            parent,
            Box::new(element(0xc3_b100 + offset as u64)),
        ));
    }
    let leaf = *owners.last().expect("depth chain leaf");
    crate::view::test_support::get_element_mut::<Element>(&arena, leaf)
        .set_background_color_value(Color::rgb(30, 90, 180));
    measure_and_place(
        &mut arena,
        root,
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
    );
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[root]);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[root], &properties);
    let FrameArtifactRecordOutcome::Artifact { mut artifact, .. } =
        record_closed_single_target_frame_artifact(
            &arena,
            &[root],
            &properties,
            &generations,
            RendererMode::ForcedForTests,
        )
        .expect("plain depth chain must be fully recordable")
    else {
        panic!("forced depth-chain recording cannot silently fall back")
    };
    assert_eq!(artifact.chunks.len(), owners.len());
    assert_eq!(artifact.owner_nodes.len(), owners.len());

    for (index, owner) in owners.iter().copied().enumerate() {
        if (1..=effect_depth).contains(&index) {
            artifact.effect_nodes.push(EffectNodeSnapshot {
                id: EffectNodeId(owner),
                owner,
                parent: index
                    .checked_sub(2)
                    .map(|parent| EffectNodeId(owners[parent + 1])),
                opacity: 1.0,
                generation: index as u64,
            });
        }
        let state = PropertyTreeState {
            effect: (index > 0).then(|| EffectNodeId(owners[index.min(effect_depth)])),
            ..Default::default()
        };
        let paint = if index <= effect_depth {
            PropertyTreeState {
                effect: index
                    .checked_sub(2)
                    .map(|parent| EffectNodeId(owners[parent + 1])),
                ..Default::default()
            }
        } else {
            state
        };
        let endpoints = artifact
            .owner_property_states
            .iter_mut()
            .find(|snapshot| snapshot.owner == owner)
            .expect("recorded depth-chain owner endpoints");
        endpoints.paint = paint;
        endpoints.descendants = state;
    }
    for chunk in &mut artifact.chunks {
        let index = owners
            .iter()
            .position(|owner| *owner == chunk.owner)
            .expect("recorded chunk owner belongs to depth chain");
        chunk.properties.effect =
            (index > 0).then(|| EffectNodeId(owners[index.min(effect_depth)]));
    }
    artifact
}

pub(super) fn depth_three_effect_artifact() -> PaintArtifact {
    effect_depth_artifact(3)
}

pub(super) fn depth_four_effect_artifact() -> PaintArtifact {
    effect_depth_artifact(4)
}

pub(super) fn raster_context() -> ArtifactSurfaceRasterContext {
    ArtifactSurfaceRasterContext::new(
        2.0,
        wgpu::TextureFormat::Bgra8Unorm,
        [3.0, 5.0],
        Some([1, 2, 300, 200]),
        4096,
        256 * 1024 * 1024,
    )
    .expect("canonical raster context")
}

pub(crate) fn exact_self_clip_shadow_artifact() -> PaintArtifact {
    let (arena, roots) = crate::view::paint::tests::anchor_parent_self_clip_shadow_root();
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &roots);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &roots, &properties);
    let FrameArtifactRecordOutcome::Artifact { artifact, .. } =
        record_closed_single_target_frame_artifact(
            &arena,
            &roots,
            &properties,
            &generations,
            RendererMode::ForcedForTests,
        )
        .expect("exact self-clip shadow fixture must be recordable")
    else {
        panic!("forced self-clip shadow fixture cannot silently fall back")
    };
    assert_eq!(artifact.chunks.len(), 1);
    assert!(matches!(
        artifact.ops.first(),
        Some(PaintOp::PreparedShadow(_))
    ));
    artifact
}

pub(super) fn raster_context_with_incoming_scissor(
    scissor: [u32; 4],
) -> ArtifactSurfaceRasterContext {
    ArtifactSurfaceRasterContext::new(
        1.0,
        wgpu::TextureFormat::Bgra8Unorm,
        [0.0, 0.0],
        Some(scissor),
        4096,
        256 * 1024 * 1024,
    )
    .expect("canonical raster context with incoming scissor")
}

#[test]
fn artifact_surface_raster_context_rejects_each_invalid_value_family() {
    assert!(
        ArtifactSurfaceRasterContext::new(
            f32::NAN,
            wgpu::TextureFormat::Bgra8Unorm,
            [0.0, 0.0],
            None,
            4096,
            1,
        )
        .is_none()
    );
    assert!(
        ArtifactSurfaceRasterContext::new(
            1.0,
            wgpu::TextureFormat::Bgra8Unorm,
            [f32::INFINITY, 0.0],
            None,
            4096,
            1,
        )
        .is_none()
    );
    assert!(
        ArtifactSurfaceRasterContext::new(
            1.0,
            wgpu::TextureFormat::Bgra8Unorm,
            [0.0, 0.0],
            Some([0, 0, 0, 1]),
            4096,
            1,
        )
        .is_none()
    );
    assert!(
        ArtifactSurfaceRasterContext::new(
            1.0,
            wgpu::TextureFormat::Bgra8Unorm,
            [0.0, 0.0],
            None,
            0,
            1,
        )
        .is_none()
    );
    assert!(
        ArtifactSurfaceRasterContext::new(
            1.0,
            wgpu::TextureFormat::Bgra8Unorm,
            [0.0, 0.0],
            None,
            4096,
            0,
        )
        .is_none()
    );
}

#[test]
fn self_replace_clip_seals_an_incoming_shadow_prefix_and_unintersected_suffix() {
    let plan = prepare_artifact_surface_raster_plan(
        exact_self_clip_shadow_artifact(),
        raster_context_with_incoming_scissor([4, 6, 24, 18]),
    )
    .expect("exact self-clip shadow raster plan");
    assert!(
        plan.nodes().is_empty(),
        "a self clip does not mint a surface"
    );
    let span = plan
        .roots()
        .iter()
        .flat_map(|root| root.steps())
        .find_map(|step| match step {
            PreparedArtifactSurfaceRasterStep::ArtifactSpan(span) => Some(span),
            PreparedArtifactSurfaceRasterStep::NestedSurface(_) => None,
        })
        .expect("scene root owns the exact self-clip shadow span");
    let chunk = span.chunks().first().expect("one prepared shadow chunk");
    assert_eq!(
        chunk.clip_schedule_for_test(),
        (
            Some(1),
            ArtifactSurfaceResolvedClip::Scissor(GraphicsPassScissor::Logical([0, 0, 320, 240,])),
        ),
        "the shadow prefix retains the incoming scissor while Replace severs it for the suffix",
    );
}

#[test]
fn empty_self_replace_suffix_keeps_the_shadow_prefix_out_of_opaque_order() {
    let mut artifact = exact_self_clip_shadow_artifact();
    let self_clip = artifact
        .clip_nodes
        .iter_mut()
        .find(|clip| clip.behavior == ClipBehavior::Replace)
        .expect("self Replace clip");
    self_clip.logical_scissor = [0, 0, 0, 0];
    let plan = prepare_artifact_surface_raster_plan(
        artifact,
        raster_context_with_incoming_scissor([4, 6, 24, 18]),
    )
    .expect("empty self-clip suffix raster plan");
    let span = plan
        .roots()
        .iter()
        .flat_map(|root| root.steps())
        .find_map(|step| match step {
            PreparedArtifactSurfaceRasterStep::ArtifactSpan(span) => Some(span),
            PreparedArtifactSurfaceRasterStep::NestedSurface(_) => None,
        })
        .expect("scene root owns the empty self-clip span");
    assert_eq!(
        span.chunks()[0].clip_schedule_for_test(),
        (Some(1), ArtifactSurfaceResolvedClip::Empty),
    );
    assert_eq!(span.opaque_order_count(), 0);
}

#[test]
fn depth_four_artifact_seals_an_arbitrary_depth_surface_program() {
    let artifact = depth_four_effect_artifact();
    let plan = prepare_artifact_surface_raster_plan(artifact, raster_context())
        .expect("depth-four artifact surface raster plan");

    assert_eq!(plan.context(), raster_context());
    assert!(
        plan.nodes().len() >= 4,
        "fixture must seal at least depth four"
    );
    let mut max_depth = 0_usize;
    for node in plan.nodes() {
        let mut depth = 1_usize;
        let mut receiver = node.receiver();
        while let SurfaceDagExecutionTargetId::Surface(parent) = receiver {
            depth += 1;
            receiver = plan.nodes()[parent.index()].receiver();
        }
        max_depth = max_depth.max(depth);
    }
    assert!(
        max_depth >= 4,
        "sealed execution tree must retain depth four"
    );
}

#[test]
fn artifact_surface_raster_plan_rejects_a_too_small_texture_budget() {
    let artifact = depth_four_effect_artifact();
    let context = ArtifactSurfaceRasterContext::new(
        2.0,
        wgpu::TextureFormat::Bgra8Unorm,
        [0.0, 0.0],
        None,
        4096,
        1,
    )
    .expect("value-valid but insufficient texture budget");
    assert!(matches!(
        prepare_artifact_surface_raster_plan(artifact, context),
        Err(ArtifactSurfaceRasterPlanError::TextureBudgetExceeded(_))
    ));
}

#[test]
fn artifact_surface_raster_origin_rejects_unrepresentable_physical_projection_before_descriptor() {
    let context = ArtifactSurfaceRasterContext::new(
        f32::MAX,
        wgpu::TextureFormat::Bgra8Unorm,
        [0.0, 0.0],
        None,
        u32::MAX,
        u64::MAX,
    )
    .expect("finite scale is value-valid before bounds projection");
    assert!(matches!(
        prepare_artifact_surface_raster_plan(depth_four_effect_artifact(), context),
        Err(ArtifactSurfaceRasterPlanError::InvalidRasterOrigin(_)),
    ));
}

#[test]
fn scroll_content_surface_seals_typed_offset_generation_and_receiver_clip_geometry() {
    let artifact = scroll_surface_artifact();
    let expected = artifact
        .scroll_nodes
        .first()
        .copied()
        .expect("scroll surface fixture snapshot");
    let plan = prepare_artifact_surface_raster_plan(artifact, raster_context())
        .expect("scroll-content raster plan");
    let node = plan
        .nodes()
        .iter()
        .find(|node| {
            matches!(
                node.geometry(),
                ArtifactSurfaceCompositeGeometryStamp::ScrollContent { .. }
            )
        })
        .expect("typed ScrollContent geometry");
    let ArtifactSurfaceCompositeGeometryStamp::ScrollContent {
        offset_bits,
        generation,
        receiver_clip,
        ..
    } = node.geometry()
    else {
        unreachable!()
    };
    assert_eq!(
        offset_bits,
        [expected.offset.x, expected.offset.y].map(f32::to_bits)
    );
    assert_eq!(generation, expected.generation);
    assert_eq!(
        receiver_clip,
        node.clip_closure()
            .map(|clip| clip.receiver_clip())
            .flatten()
    );
    let localized = node
        .steps()
        .iter()
        .find_map(|step| match step {
            PreparedArtifactSurfaceRasterStep::ArtifactSpan(span) => span
                .chunks()
                .iter()
                .find(|chunk| !chunk.localized_ops().is_empty()),
            PreparedArtifactSurfaceRasterStep::NestedSurface(_) => None,
        })
        .expect("scroll surface owns one localized payload chunk");
    let [source_x, source_y, ..] = localized.source().bounds_bits.map(f32::from_bits);
    let [local_x, local_y, ..] = localized.localized_bounds_bits().map(f32::from_bits);
    let raw = [source_x + expected.offset.x, source_y + expected.offset.y];
    let scale = 2.0_f32;
    let expected_local = [
        raw[0] - (raw[0] * scale).floor() / scale,
        raw[1] - (raw[1] * scale).floor() / scale,
    ];
    assert_eq!(
        [local_x, local_y].map(f32::to_bits),
        expected_local.map(f32::to_bits),
        "scroll localization must consume the offset then normalize the surface raster origin",
    );
}

#[test]
fn artifact_surface_paint_op_taxonomy_is_a_closed_eight_variant_set() {
    fn label(kind: ArtifactSurfacePaintOpKind) -> &'static str {
        match kind {
            ArtifactSurfacePaintOpKind::DrawRect => "draw-rect",
            ArtifactSurfacePaintOpKind::InlineIfcDecoration => "inline-ifc-decoration",
            ArtifactSurfacePaintOpKind::Shadow => "shadow",
            ArtifactSurfacePaintOpKind::ScrollbarOverlay => "scrollbar-overlay",
            ArtifactSurfacePaintOpKind::Text => "text",
            ArtifactSurfacePaintOpKind::Image => "image",
            ArtifactSurfacePaintOpKind::Svg => "svg",
            ArtifactSurfacePaintOpKind::Gpu => "gpu",
        }
    }
    let _ = label as fn(ArtifactSurfacePaintOpKind) -> &'static str;
}

#[test]
fn artifact_surface_localization_rejects_nonfinite_translation_with_typed_reason() {
    let artifact = scroll_surface_artifact();
    let op = artifact
        .ops
        .iter()
        .find(|op| matches!(op, PaintOp::DrawRect(_)))
        .expect("scroll fixture carries one draw op");
    assert!(matches!(
        localize_artifact_surface_op(op, [f32::NAN, 0.0]),
        Err(ArtifactSurfaceLocalizationError::NonFiniteTranslation)
    ));
}

#[test]
fn inline_ifc_decoration_localization_translates_fill_and_border_and_rebuilds_identity() {
    let descriptor = PreparedInlineIfcDecorationDescriptor {
        source: 0xc3_b301,
        line_index: 2,
        range: 4..9,
        style_key: [11, 22, 33, 255],
        slice_insets: [1.0, 2.0, 3.0, 4.0],
        is_first_for_source: true,
        is_last_for_source: true,
    };
    let mut fill = crate::view::render_pass::draw_rect_pass::RectPassParams {
        position: [10.25, 20.5],
        size: [30.0, 12.0],
        fill_color: [0.2, 0.4, 0.6, 1.0],
        opacity: 0.75,
        ..Default::default()
    };
    fill.set_border_width(1.0);
    fill.use_border_side_colors = true;
    let mut border = fill.clone();
    border.fill_color = [0.0; 4];
    let op = PreparedInlineIfcDecorationOp::new(descriptor.clone(), fill.clone(), Some(border))
        .expect("canonical inline IFC decoration");
    let delta = [3.5, -2.25];
    let PaintOp::PreparedInlineIfcDecoration(localized) =
        localize_artifact_surface_op(&PaintOp::PreparedInlineIfcDecoration(op), delta)
            .expect("inline IFC localization")
    else {
        unreachable!()
    };

    assert_eq!(
        localized.fill.position.map(f32::to_bits),
        [fill.position[0] + delta[0], fill.position[1] + delta[1]].map(f32::to_bits)
    );
    assert_eq!(
        localized.fill.size.map(f32::to_bits),
        fill.size.map(f32::to_bits)
    );
    let localized_border = localized.border.as_ref().expect("localized border");
    assert_eq!(
        localized_border.position.map(f32::to_bits),
        [fill.position[0] + delta[0], fill.position[1] + delta[1]].map(f32::to_bits)
    );
    assert_eq!(localized.descriptor.source, descriptor.source);
    assert_eq!(localized.descriptor.range, descriptor.range);
    assert!(localized.has_canonical_identity());
}

#[test]
fn scrollbar_overlay_localization_translates_both_axes_and_rebuilds_identity() {
    use crate::view::base_component::{
        ScrollbarInteractionWitness, ScrollbarOverlayWitness, ScrollbarPaintStateWitness,
    };

    let witness = ScrollbarOverlayWitness {
        vertical_track: Some(Rect {
            x: 90.0,
            y: 10.0,
            width: 8.0,
            height: 100.0,
        }),
        vertical_thumb: Some(Rect {
            x: 90.0,
            y: 30.0,
            width: 8.0,
            height: 24.0,
        }),
        horizontal_track: Some(Rect {
            x: 10.0,
            y: 102.0,
            width: 80.0,
            height: 8.0,
        }),
        horizontal_thumb: Some(Rect {
            x: 28.0,
            y: 102.0,
            width: 20.0,
            height: 8.0,
        }),
        interaction: ScrollbarInteractionWitness {
            hovered: false,
            dragging_axis: None,
            has_interaction_timestamp: false,
        },
        paint_state: ScrollbarPaintStateWitness::OpaqueNow,
        sampled_alpha: 1.0,
        shadow_blur_radius: 2.0,
    };
    let original = PreparedScrollbarOverlayOp::from_witness(witness)
        .expect("canonical two-axis scrollbar overlay");
    let original_identity = original.frozen_identity();
    let original_primary_vertices = original.track_shadow.mesh.vertices.clone();
    let original_primary_position = original.track.params.position;
    let (_, original_secondary_track, _, original_secondary_thumb) =
        original.secondary_axis().expect("original secondary axis");
    let original_secondary_track_position = original_secondary_track.params.position;
    let original_secondary_thumb_position = original_secondary_thumb.params.position;
    let delta = [-6.5, 4.25];
    let PaintOp::PreparedScrollbarOverlay(localized) =
        localize_artifact_surface_op(&PaintOp::PreparedScrollbarOverlay(original), delta)
            .expect("scrollbar overlay localization")
    else {
        unreachable!()
    };

    assert_eq!(
        localized.track.params.position.map(f32::to_bits),
        [
            original_primary_position[0] + delta[0],
            original_primary_position[1] + delta[1],
        ]
        .map(f32::to_bits)
    );
    for (before, after) in original_primary_vertices
        .iter()
        .zip(&localized.track_shadow.mesh.vertices)
    {
        assert_eq!(
            after.map(f32::to_bits),
            [before[0] + delta[0], before[1] + delta[1]].map(f32::to_bits)
        );
    }
    let (_, localized_secondary_track, _, localized_secondary_thumb) = localized
        .secondary_axis()
        .expect("localized secondary axis");
    assert_eq!(
        localized_secondary_track.params.position.map(f32::to_bits),
        [
            original_secondary_track_position[0] + delta[0],
            original_secondary_track_position[1] + delta[1],
        ]
        .map(f32::to_bits)
    );
    assert_eq!(
        localized_secondary_thumb.params.position.map(f32::to_bits),
        [
            original_secondary_thumb_position[0] + delta[0],
            original_secondary_thumb_position[1] + delta[1],
        ]
        .map(f32::to_bits)
    );
    assert_ne!(localized.frozen_identity(), original_identity);
    assert!(localized.has_canonical_identity());
}

#[test]
fn artifact_surface_raster_plan_error_taxonomy_is_exhaustive() {
    fn label(error: ArtifactSurfaceRasterPlanError) -> &'static str {
        match error {
            ArtifactSurfaceRasterPlanError::ArtifactProgram(_) => "artifact-program",
            ArtifactSurfaceRasterPlanError::SurfaceDag(_) => "surface-dag",
            ArtifactSurfaceRasterPlanError::MissingSurfaceSnapshot(_) => "missing-snapshot",
            ArtifactSurfaceRasterPlanError::MissingCoverageNode(_) => "missing-coverage",
            ArtifactSurfaceRasterPlanError::EmptySurfaceBounds(_) => "empty-bounds",
            ArtifactSurfaceRasterPlanError::InvalidSurfaceBounds(_) => "invalid-bounds",
            ArtifactSurfaceRasterPlanError::InvalidDescriptor(_) => "invalid-descriptor",
            ArtifactSurfaceRasterPlanError::TextureBudgetExceeded(_) => "texture-budget",
            ArtifactSurfaceRasterPlanError::InvalidCoverageSpan(_) => "coverage-span",
            ArtifactSurfaceRasterPlanError::InvalidOwnerTopology { .. } => "owner-topology",
            ArtifactSurfaceRasterPlanError::InvalidChunkBounds { .. } => "chunk-bounds",
            ArtifactSurfaceRasterPlanError::InvalidResolvedClip { .. } => "resolved-clip",
            ArtifactSurfaceRasterPlanError::InvalidReceiverClip(_) => "receiver-clip",
            ArtifactSurfaceRasterPlanError::SourceCorrespondence { .. } => "source-correspondence",
            ArtifactSurfaceRasterPlanError::Localization { .. } => "localization",
            ArtifactSurfaceRasterPlanError::LocalizedPayload { .. } => "payload",
            ArtifactSurfaceRasterPlanError::InvalidNestedSurface { .. } => "nested-surface",
            ArtifactSurfaceRasterPlanError::InvalidRasterOrigin(_) => "raster-origin",
            ArtifactSurfaceRasterPlanError::GpuSourceBudgetExceeded => "gpu-source-budget",
            ArtifactSurfaceRasterPlanError::InvalidGpuSource => "gpu-source-descriptor",
        }
    }
    let _ = label as fn(ArtifactSurfaceRasterPlanError) -> &'static str;
}

#[test]
fn artifact_surface_plan_step_taxonomy_is_exactly_generic_span_or_nested_surface() {
    fn label(step: &PreparedArtifactSurfaceRasterStep) -> &'static str {
        match step {
            PreparedArtifactSurfaceRasterStep::ArtifactSpan(_) => "artifact-span",
            PreparedArtifactSurfaceRasterStep::NestedSurface(_) => "nested-surface",
        }
    }
    let _ = label as fn(&PreparedArtifactSurfaceRasterStep) -> &'static str;
}
