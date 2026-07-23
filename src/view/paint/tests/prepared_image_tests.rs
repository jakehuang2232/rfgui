use super::*;

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
    assert!(!super::super::frame_recorder::canonical_manifest_matches(
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
    assert!(!super::super::frame_recorder::canonical_manifest_matches(
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
    assert!(!super::super::frame_recorder::canonical_manifest_matches(
        &preflight, &full
    ));
}
