use super::*;

#[test]
fn prepared_image_fill_preserves_payload_after_arena_drop() {
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
    let artifact =
        whole_frame_artifact(&artifact_arena, &artifact_roots, &properties, &generations);
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

    let recorded = compiled_whole_frame_graph(&artifact)
        .test_compile_snapshot()
        .unwrap();
    let recorded_images = recorded
        .pass_payloads()
        .iter()
        .filter(|pass| {
            matches!(
                pass,
                FramePassTestPayload::TextureComposite(texture) if texture.sampled_source.is_some()
            )
        })
        .count();
    assert_eq!(recorded_images, 1);
}

#[test]
fn undecorated_prepared_image_fit_sampling_opacity_scale_and_format_compile_strictly() {
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
        let artifact =
            whole_frame_artifact(&artifact_arena, &artifact_roots, &properties, &generations);
        assert!(matches!(
            artifact.ops.as_slice(),
            [PaintOp::PreparedImage(_)]
        ));
        drop(artifact_arena);
        let mut artifact_graph = compiled_whole_frame_graph_with_config(&artifact, config);
        let _ = strict_paint_snapshot(&mut artifact_graph, config);
    }
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

    let artifact = whole_frame_artifact(&arena, &[parent], &properties, &generations);
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
            geometry: crate::view::compositor::property_tree::ClipGeometry::Viewport(SCISSOR),
            behavior: ClipBehavior::Intersect,
            ..
        }] if *id == expected_clip && *owner == parent
    ));

    let mut graph = compiled_whole_frame_graph(&artifact);
    let _ = strict_paint_snapshot(&mut graph, PaintParityConfig::default());
    let passes = graph.test_graphics_passes_mut::<crate::view::render_pass::TextureCompositePass>();
    assert_eq!(passes.len(), 2);
    assert!(passes.iter().all(|pass| {
        let snapshot = pass.test_snapshot();
        snapshot.explicit_scissor_rect.is_none() && snapshot.effective_scissor_rect == Some(SCISSOR)
    }));
}
