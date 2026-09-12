use super::*;
use crate::view::paint::compiler::ArtifactSurfaceResolvedClip;
use crate::view::paint::{
    ArtifactSurfaceLocalizationError, FrameArtifactRecordOutcome, PaintOp,
    PreparedInlineIfcDecorationDescriptor, PreparedInlineIfcDecorationOp,
    PreparedScrollbarOverlayOp, PreparedShadowOp, RendererMode, ResolvedClip,
    SurfaceDagExecutionTargetId, artifact_surface_op_corresponds_to_source_for_test,
    artifact_surface_op_has_baked_opacity_for_test, neutralize_artifact_surface_opacity_for_test,
    prepare_artifact_surface_raster_plan, record_closed_single_target_frame_artifact,
    resolve_artifact_surface_clip_for_test, seal_prepared_artifact_surface_frame,
};
use crate::view::render_pass::render_target::GraphicsPassScissor;

#[cfg(not(target_arch = "wasm32"))]
fn plain_host_op(host: &str, predicate: impl Fn(&PaintOp) -> bool) -> PaintOp {
    const SVG: &str = "<svg xmlns='http://www.w3.org/2000/svg' width='18' height='14'><rect width='18' height='14' fill='#38bdf8'/></svg>";
    let element: Box<dyn ElementTrait> = match host {
        "text" => Box::new(Text::new_with_id(
            0xc3_b3a2, 0.0, 0.0, 40.0, 20.0, "stage c",
        )),
        "image" => Box::new(Image::new_with_id(
            0xc3_b3a3,
            ImageSource::Rgba {
                width: 1,
                height: 1,
                pixels: Arc::from([64_u8, 160, 255, 255]),
            },
        )),
        "svg" => Box::new(Svg::new_with_id(0xc3_b3a4, SvgSource::Content(SVG.into()))),
        _ => panic!("unknown plain host"),
    };
    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, element);
    arena
        .with_element_taken(root, |element, arena| element.sync_arena(arena))
        .expect("plain host sync");
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
    if host == "svg" {
        arena
            .get_mut(root)
            .expect("svg host")
            .element
            .as_any_mut()
            .downcast_mut::<Svg>()
            .expect("Svg host")
            .prepare_content_paint_for_test(SVG, (18.0, 14.0), 1.0)
            .expect("prepare exact SVG paint");
    }
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &[root]);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &[root], &properties);
    let FrameArtifactRecordOutcome::Artifact { artifact, .. } =
        record_closed_single_target_frame_artifact(
            &arena,
            &[root],
            &properties,
            &generations,
            RendererMode::ForcedForTests,
        )
        .expect("native effect fixture must record")
    else {
        panic!("forced native effect fixture cannot silently fall back")
    };
    artifact
        .ops
        .into_iter()
        .find(predicate)
        .expect("native effect fixture must contain requested op")
}

#[cfg(not(target_arch = "wasm32"))]
fn seven_opacity_carriers() -> Vec<PaintOp> {
    use crate::view::base_component::{
        Rect, ScrollbarInteractionWitness, ScrollbarOverlayWitness, ScrollbarPaintStateWitness,
    };
    use crate::view::render_pass::shadow_module::{ShadowMesh, ShadowParams};

    let draw = PaintOp::DrawRect(crate::view::paint::DrawRectOp {
        params: crate::view::render_pass::draw_rect_pass::RectPassParams {
            position: [1.0, 2.0],
            size: [20.0, 10.0],
            fill_color: [0.1, 0.2, 0.3, 1.0],
            opacity: 0.5,
            ..Default::default()
        },
        mode: crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly,
    });

    let descriptor = PreparedInlineIfcDecorationDescriptor {
        source: 0xc3_b3a1,
        line_index: 0,
        range: 0..1,
        style_key: [1, 2, 3, 255],
        slice_insets: [0.0; 4],
        is_first_for_source: true,
        is_last_for_source: true,
    };
    let inline = PreparedInlineIfcDecorationOp::new(
        descriptor,
        crate::view::render_pass::draw_rect_pass::RectPassParams {
            position: [3.0, 4.0],
            size: [20.0, 10.0],
            fill_color: [0.2, 0.3, 0.4, 1.0],
            opacity: 0.5,
            ..Default::default()
        },
        None,
    )
    .expect("canonical inline decoration");
    let shadow = PreparedShadowOp::new(
        ShadowMesh::rounded_rect(1.0, 2.0, 20.0, 10.0, 2.0),
        ShadowParams {
            opacity: 0.5,
            ..Default::default()
        },
    )
    .expect("canonical shadow");
    let overlay = PreparedScrollbarOverlayOp::from_witness(ScrollbarOverlayWitness {
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
        horizontal_track: None,
        horizontal_thumb: None,
        interaction: ScrollbarInteractionWitness {
            hovered: false,
            dragging_axis: None,
            has_interaction_timestamp: false,
        },
        paint_state: ScrollbarPaintStateWitness::OpaqueNow,
        sampled_alpha: 1.0,
        shadow_blur_radius: 2.0,
    })
    .and_then(|overlay| overlay.with_baked_opacity(0.5))
    .expect("canonical opacity-bearing scrollbar overlay");

    vec![
        draw,
        PaintOp::inline_decoration(inline),
        PaintOp::PreparedShadow(shadow),
        PaintOp::scrollbar_overlay(overlay),
        plain_host_op("text", |op| matches!(op, PaintOp::PreparedText(_))),
        plain_host_op("image", |op| matches!(op, PaintOp::PreparedImage(_))),
        plain_host_op("svg", |op| matches!(op, PaintOp::PreparedSvg(_))),
    ]
    .into_iter()
    .map(|op| match op {
        PaintOp::PreparedText(mut text) => {
            for glyph in &mut std::sync::Arc::make_mut(&mut text.params)
                .staging_input
                .glyphs
            {
                glyph.paint.opacity = 0.5;
            }
            PaintOp::PreparedText(
                crate::view::paint::PreparedTextOp::new(text.params).expect("rebuilt text opacity"),
            )
        }
        PaintOp::PreparedImage(mut image) => {
            image.params.opacity = 0.5;
            PaintOp::PreparedImage(image)
        }
        PaintOp::PreparedSvg(mut svg) => {
            svg.params.opacity = 0.5;
            PaintOp::PreparedSvg(svg)
        }
        other => other,
    })
    .collect()
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn all_seven_artifact_ops_neutralize_exact_owner_local_opacity() {
    let ops = seven_opacity_carriers();
    assert_eq!(ops.len(), 7);
    for op in ops {
        assert!(artifact_surface_op_has_baked_opacity_for_test(
            &op,
            0.5_f32.to_bits(),
        ));
        let neutralized =
            neutralize_artifact_surface_opacity_for_test(op.clone(), 0.5_f32.to_bits())
                .expect("exact baked opacity must neutralize without division");
        assert!(artifact_surface_op_has_baked_opacity_for_test(
            &neutralized,
            1.0_f32.to_bits(),
        ));
        assert_eq!((1.0_f32 * 0.5).to_bits(), 0.5_f32.to_bits());
        assert!(artifact_surface_op_corresponds_to_source_for_test(
            &op,
            &neutralized,
            [0.0, 0.0],
            Some(0.5_f32.to_bits()),
        ));
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn opacity_neutralization_rejects_a_nonmatching_source_bitwise() {
    let op = seven_opacity_carriers().remove(0);
    assert!(matches!(
        neutralize_artifact_surface_opacity_for_test(op, 0.75_f32.to_bits()),
        Err(ArtifactSurfaceLocalizationError::OpacityMismatch(
            crate::view::paint::ArtifactSurfacePaintOpKind::DrawRect,
        ))
    ));
}

#[test]
fn artifact_surface_localization_error_taxonomy_is_exhaustive() {
    fn label(error: ArtifactSurfaceLocalizationError) -> &'static str {
        match error {
            ArtifactSurfaceLocalizationError::NonFiniteTranslation => "non-finite-translation",
            ArtifactSurfaceLocalizationError::EmbeddedClip(_) => "embedded-clip",
            ArtifactSurfaceLocalizationError::InvalidLocalizedOp(_) => "invalid-op",
            ArtifactSurfaceLocalizationError::OpacityMismatch(_) => "opacity-mismatch",
        }
    }
    let _ = label as fn(ArtifactSurfaceLocalizationError) -> &'static str;
}

#[test]
fn nested_effect_plan_keeps_source_identity_and_applies_each_opacity_once() {
    let mut artifact = super::stage_c_surface_raster_plan_tests::depth_three_effect_artifact();
    let opacity_bits = [0.0_f32.to_bits(), 0.25_f32.to_bits(), 0.75_f32.to_bits()];
    for (effect, bits) in artifact.effect_nodes.iter_mut().zip(opacity_bits) {
        effect.opacity = f32::from_bits(bits);
        let chunk = artifact
            .chunks
            .iter_mut()
            .find(|chunk| chunk.owner == effect.owner)
            .expect("each authored effect owner has one direct chunk");
        let ops = &mut artifact.ops[chunk.op_range.clone()];
        for op in ops.iter_mut() {
            let PaintOp::DrawRect(rect) = op else {
                panic!("depth effect fixture direct chunks use DrawRect")
            };
            rect.params.opacity = effect.opacity;
        }
        chunk.payload_identity = chunk
            .payload_identity
            .rebuild_from_localized_ops(ops)
            .expect("updated source payload identity");
    }
    let expected_sources = artifact
        .chunks
        .iter()
        .map(|chunk| (chunk.id, chunk.payload_identity.clone()))
        .collect::<FxHashMap<_, _>>();
    let plan = prepare_artifact_surface_raster_plan(
        artifact,
        super::stage_c_surface_raster_plan_tests::raster_context(),
    )
    .expect("nested exact-opacity raster plan");
    let mut observed = Vec::new();
    let mut inherited_chunks = 0_usize;
    for node in plan.nodes() {
        let crate::view::paint::ArtifactSurfaceCompositeGeometryStamp::Effect {
            opacity_bits, ..
        } = node.geometry()
        else {
            continue;
        };
        observed.push(opacity_bits);
        for span in node.steps().iter().filter_map(|step| match step {
            crate::view::paint::PreparedArtifactSurfaceRasterStep::ArtifactSpan(span) => Some(span),
            crate::view::paint::PreparedArtifactSurfaceRasterStep::NestedSurface(_) => None,
        }) {
            for chunk in span.chunks() {
                assert_eq!(
                    expected_sources.get(&chunk.source().id),
                    Some(&chunk.source().payload_identity),
                    "the exact source identity remains frozen beside the rebased payload",
                );
                assert!(chunk.localized_ops().iter().all(|op| {
                    artifact_surface_op_has_baked_opacity_for_test(op, 1.0_f32.to_bits())
                }));
                if chunk.source().owner != node.identity().boundary_root {
                    inherited_chunks += 1;
                }
            }
        }
    }
    assert!(
        inherited_chunks > 0,
        "the deepest surface must retain one inherited-opacity chunk at neutral 1.0",
    );
    observed.sort_unstable();
    let mut expected = opacity_bits.to_vec();
    expected.sort_unstable();
    assert_eq!(observed, expected);
}

#[test]
fn artifact_clip_resolver_seals_unclipped_replace_intersect_and_empty_results() {
    use crate::view::compositor::property_tree::{
        ClipBehavior, ClipNodeId, ClipNodeRole, ClipNodeSnapshot,
    };

    let mut arena = new_test_arena();
    let root = commit_element(
        &mut arena,
        Box::new(Element::new_with_id(0xc3_b3a5, 0.0, 0.0, 20.0, 20.0)),
    );
    let child = commit_child(
        &mut arena,
        root,
        Box::new(Element::new_with_id(0xc3_b3a6, 0.0, 0.0, 20.0, 20.0)),
    );
    let replace = ClipNodeId {
        owner: root,
        role: ClipNodeRole::SelfClip,
    };
    let intersect = ClipNodeId {
        owner: child,
        role: ClipNodeRole::ContentsClip,
    };
    let inherited_intersect = ClipNodeId {
        owner: root,
        role: ClipNodeRole::ContentsClip,
    };
    let clips = [
        ClipNodeSnapshot {
            id: replace,
            owner: root,
            parent: None,
            logical_scissor: [10, 10, 20, 20],
            behavior: ClipBehavior::Replace,
            generation: 71,
        },
        ClipNodeSnapshot {
            id: intersect,
            owner: child,
            parent: Some(replace),
            logical_scissor: [15, 15, 20, 20],
            behavior: ClipBehavior::Intersect,
            generation: 73,
        },
        ClipNodeSnapshot {
            id: inherited_intersect,
            owner: root,
            parent: None,
            logical_scissor: [15, 15, 20, 20],
            behavior: ClipBehavior::Intersect,
            generation: 75,
        },
    ];
    assert_eq!(
        resolve_artifact_surface_clip_for_test(None, &clips, None),
        Some(ResolvedClip::Unclipped),
    );
    assert_eq!(
        resolve_artifact_surface_clip_for_test(Some(replace), &clips, None),
        Some(ResolvedClip::Scissor([10, 10, 20, 20])),
    );
    assert_eq!(
        resolve_artifact_surface_clip_for_test(Some(replace), &clips, Some([4, 6, 24, 18]),),
        Some(ResolvedClip::Scissor([10, 10, 20, 20])),
        "Replace severs the incoming frame scissor",
    );
    assert_eq!(
        resolve_artifact_surface_clip_for_test(Some(intersect), &clips, Some([20, 20, 20, 20]),),
        Some(ResolvedClip::Scissor([15, 15, 15, 15])),
        "a parent Replace severs the incoming scissor before its child Intersect",
    );
    assert_eq!(
        resolve_artifact_surface_clip_for_test(
            Some(inherited_intersect),
            &clips,
            Some([20, 20, 20, 20]),
        ),
        Some(ResolvedClip::Scissor([20, 20, 15, 15])),
    );
    assert_eq!(
        resolve_artifact_surface_clip_for_test(
            Some(inherited_intersect),
            &clips,
            Some([40, 40, 5, 5]),
        ),
        Some(ResolvedClip::Empty),
    );
}

#[test]
fn receiver_clip_is_sealed_and_nested_receivers_exclude_frame_scissor() {
    use crate::view::compositor::property_tree::{
        ClipBehavior, ClipNodeId, ClipNodeRole, ClipNodeSnapshot,
    };

    let mut top_level_artifact =
        super::stage_c_surface_raster_plan_tests::scroll_surface_artifact();
    let scroll_owner = top_level_artifact.scroll_nodes[0].owner;
    let contents_clip = ClipNodeId {
        owner: scroll_owner,
        role: ClipNodeRole::ContentsClip,
    };
    let receiver_clip = ClipNodeId {
        owner: scroll_owner,
        role: ClipNodeRole::SelfClip,
    };
    top_level_artifact.clip_nodes.push(ClipNodeSnapshot {
        id: receiver_clip,
        owner: scroll_owner,
        parent: None,
        logical_scissor: [10, 12, 80, 70],
        behavior: ClipBehavior::Replace,
        generation: 79,
    });
    top_level_artifact
        .clip_nodes
        .iter_mut()
        .find(|clip| clip.id == contents_clip)
        .expect("scroll contents clip")
        .parent = Some(receiver_clip);
    for chunk in &mut top_level_artifact.chunks {
        if chunk.owner == scroll_owner {
            chunk.properties.clip = Some(receiver_clip);
        }
    }
    let root_endpoints = top_level_artifact
        .owner_property_states
        .iter_mut()
        .find(|endpoints| endpoints.owner == scroll_owner)
        .expect("scroll root endpoints");
    root_endpoints.paint.clip = Some(receiver_clip);
    let top_level = prepare_artifact_surface_raster_plan(
        top_level_artifact,
        super::stage_c_surface_raster_plan_tests::raster_context_with_incoming_scissor([
            20, 20, 20, 20,
        ]),
    )
    .expect("top-level scroll raster plan");
    let top_level = top_level
        .nodes()
        .iter()
        .find(|node| matches!(node.receiver(), SurfaceDagExecutionTargetId::SceneRoot(_)))
        .expect("scroll fixture owns a top-level surface");
    assert_eq!(
        top_level.geometry().resolved_receiver_clip(),
        ArtifactSurfaceResolvedClip::Scissor(GraphicsPassScissor::Logical([10, 12, 80, 70,])),
        "a top-level receiver Replace severs the frame incoming scissor before sealing",
    );

    let plan = prepare_artifact_surface_raster_plan(
        super::stage_c_surface_raster_plan_tests::depth_three_effect_artifact(),
        super::stage_c_surface_raster_plan_tests::raster_context(),
    )
    .expect("nested effect raster plan");
    let nested = plan
        .nodes()
        .iter()
        .find(|node| matches!(node.receiver(), SurfaceDagExecutionTargetId::Surface(_)))
        .expect("depth-three fixture owns a nested receiver");
    assert_eq!(
        nested.geometry().resolved_receiver_clip(),
        ArtifactSurfaceResolvedClip::Unclipped,
        "frame incoming scissor applies above the nested receiver",
    );
}

#[test]
fn empty_surface_chunk_stays_in_identity_but_not_opaque_emission_order() {
    use crate::view::paint::{RetainedSurfaceCompileAction, RetainedSurfaceRasterRole};

    let artifact = super::stage_c_surface_resident_stamp_tests::scroll_artifact_with_a_local_clip();
    let nonempty = seal_prepared_artifact_surface_frame(
        prepare_artifact_surface_raster_plan(
            artifact.clone(),
            super::stage_c_surface_raster_plan_tests::raster_context(),
        )
        .expect("nonempty local clip plan"),
    )
    .expect("nonempty local clip seal");
    let mut empty_artifact = artifact;
    let local = empty_artifact
        .clip_nodes
        .iter_mut()
        .find(|clip| clip.parent.is_some())
        .expect("local clip");
    local.logical_scissor[2] = 0;
    let empty = seal_prepared_artifact_surface_frame(
        prepare_artifact_surface_raster_plan(
            empty_artifact,
            super::stage_c_surface_raster_plan_tests::raster_context(),
        )
        .expect("empty local clip plan"),
    )
    .expect("empty local clip seal");
    let find_scroll = |frame: &crate::view::paint::PreparedArtifactSurfaceFrame| {
        frame
            .residents()
            .ordered_entries()
            .iter()
            .find(|entry| entry.stamp().identity.role == RetainedSurfaceRasterRole::ScrollContent)
            .expect("scroll resident")
            .stamp()
            .clone()
    };
    let nonempty_stamp = find_scroll(&nonempty);
    let empty_stamp = find_scroll(&empty);
    assert_eq!(nonempty_stamp.op_count, empty_stamp.op_count);
    assert!(
        nonempty_stamp
            .artifact_surface_program_resolved_clips_for_test()
            .expect("artifact program")
            .contains(&ArtifactSurfaceResolvedClip::Scissor(
                GraphicsPassScissor::TargetPhysical([0, 0, 154, 130]),
            )),
        "the local clip must share the surface's target-physical raster-origin projection",
    );
    assert!(
        empty_stamp
            .artifact_surface_program_resolved_clips_for_test()
            .expect("artifact program")
            .contains(&ArtifactSurfaceResolvedClip::Empty)
    );
    assert!(empty_stamp.opaque_order_span.end < nonempty_stamp.opaque_order_span.end);
    assert_eq!(
        crate::view::viewport::retained_surface_compile_action_against_resident_for_test(
            nonempty_stamp,
            &empty_stamp,
        ),
        RetainedSurfaceCompileAction::Reraster,
    );
}
