use super::*;

#[test]
fn ready_svg_media_with_outer_shadow_records_typed_shadow_prefix() {
    let mut svg = freeze_ready_svg(0x93b0, unique_svg("ready-shadow-fallback"), 1.0);
    let mut style = Style::new();
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    style.set_box_shadow(vec![BoxShadow::new().offset_x(1.0)]);
    svg.apply_style(style);
    let mut arena = new_test_arena();
    let owner = commit_element(&mut arena, Box::new(svg));
    measure_and_place(
        &mut arena,
        owner,
        LayoutConstraints {
            max_width: 100.0,
            max_height: 100.0,
            viewport_width: 100.0,
            viewport_height: 100.0,
            percent_base_width: Some(100.0),
            percent_base_height: Some(100.0),
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
            percent_base_width: Some(100.0),
            percent_base_height: Some(100.0),
        },
    );
    let node = arena.get(owner).unwrap();
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
        .expect("ready shadow SVG metadata");
    let artifact = node
        .element
        .record_shadow_paint_artifact(owner, Default::default(), revision, &arena, context)
        .expect("ready shadow SVG artifact");
    assert_eq!(
        artifact.chunks[0].payload_identity,
        metadata.payload_identity
    );
    assert!(
        matches!(
            &metadata.payload_identity,
            crate::view::paint::PaintPayloadIdentity::SvgWithShadows(_, shadows, _)
                if shadows.len() == 1
        ),
        "{:?}",
        metadata.payload_identity
    );
    assert!(matches!(
        artifact.ops.as_slice(),
        [
            crate::view::paint::PaintOp::PreparedShadow(_),
            ..,
            crate::view::paint::PaintOp::PreparedSvg(_)
        ]
    ));
    assert!(
        crate::view::paint::validate_media_content_artifact_for_test(&artifact),
        "compiler must accept the exact typed shadow prefix"
    );
    let mut reordered = artifact.clone();
    assert!(matches!(
        reordered.ops.get(1),
        Some(crate::view::paint::PaintOp::DrawRect(_))
    ));
    reordered.ops.swap(0, 1);
    assert!(
        !crate::view::paint::validate_media_content_artifact_for_test(&reordered),
        "a shadow after decoration must fail closed"
    );
    let (baseline_media, baseline_shadows, baseline_decoration) =
        match &metadata.payload_identity {
            crate::view::paint::PaintPayloadIdentity::SvgWithShadows(
                media,
                shadows,
                decoration,
            ) => (media.clone(), shadows.clone(), decoration.clone()),
            _ => unreachable!(),
        };
    drop(node);
    {
        let mut node = arena.get_mut(owner).unwrap();
        let svg = node.element.as_any_mut().downcast_mut::<Svg>().unwrap();
        let mut style = Style::new();
        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        style.set_box_shadow(vec![BoxShadow::new().offset_x(9.0).offset_y(-4.0)]);
        svg.apply_style(style);
    }
    measure_and_place(
        &mut arena,
        owner,
        LayoutConstraints {
            max_width: 100.0,
            max_height: 100.0,
            viewport_width: 100.0,
            viewport_height: 100.0,
            percent_base_width: Some(100.0),
            percent_base_height: Some(100.0),
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
            percent_base_width: Some(100.0),
            percent_base_height: Some(100.0),
        },
    );
    let node = arena.get(owner).unwrap();
    let changed_context = node
        .element
        .shadow_paint_recording_context(Default::default());
    let changed = node
        .element
        .record_shadow_paint_metadata(
            owner,
            Default::default(),
            revision,
            &arena,
            changed_context,
        )
        .expect("shadow-mutated SVG metadata");
    let crate::view::paint::PaintPayloadIdentity::SvgWithShadows(
        changed_media,
        changed_shadows,
        changed_decoration,
    ) = changed.payload_identity
    else {
        panic!("shadow-mutated SVG must retain typed media identity")
    };
    assert_eq!(changed_media, baseline_media);
    assert_ne!(changed_shadows, baseline_shadows);
    assert_eq!(changed_decoration, baseline_decoration);
}

#[test]
fn ready_svg_exact_self_clip_shadow_metadata_and_full_are_canonical() {
    use crate::view::paint::{
        CoverageRecordingMode, PaintCoverageItem, PaintPayloadIdentity,
        record_coverage_manifest,
    };

    let mut svg = freeze_ready_svg(0x93b1, unique_svg("ready-exact-self-clip-shadow"), 1.0);
    let mut style = Style::new();
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .left(crate::style::Length::px(1.25))
                .top(crate::style::Length::px(2.75))
                .clip(ClipMode::AnchorParent),
        ),
    );
    style.set_box_shadow(vec![BoxShadow::new().offset_x(-3.0).offset_y(4.5)]);
    svg.apply_style(style);
    let mut arena = new_test_arena();
    let owner = commit_element(&mut arena, Box::new(svg));
    measure_and_place(
        &mut arena,
        owner,
        LayoutConstraints {
            max_width: 100.0,
            max_height: 100.0,
            viewport_width: 100.0,
            viewport_height: 100.0,
            percent_base_width: Some(100.0),
            percent_base_height: Some(100.0),
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
            percent_base_width: Some(100.0),
            percent_base_height: Some(100.0),
        },
    );
    let roots = [owner];
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &roots);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &roots, &properties);
    let record = |mode: CoverageRecordingMode| {
        record_coverage_manifest(&arena, &roots, false, true, mode, &properties, &generations)
    };
    let metadata_manifest = record(CoverageRecordingMode::MetadataOnly);
    let full = record(CoverageRecordingMode::FullArtifact);
    assert!(metadata_manifest.validation_errors.is_empty());
    assert!(full.validation_errors.is_empty());
    let [
        PaintCoverageItem::ArtifactChunk {
            chunk: metadata, ..
        },
    ] = metadata_manifest.items.as_slice()
    else {
        panic!(
            "exact clipped Svg metadata must remain one native chunk: {:?}",
            metadata_manifest.items
        )
    };
    let [
        PaintCoverageItem::ArtifactChunk {
            chunk: full_chunk,
            ops: Some(ops),
            clip_snapshot,
            ..
        },
    ] = full.items.as_slice()
    else {
        panic!("exact clipped Svg full recording must carry its clip snapshot")
    };
    assert_eq!(metadata.payload_identity, full_chunk.payload_identity);
    assert!(matches!(
        &metadata.payload_identity,
        PaintPayloadIdentity::SvgWithShadows(_, shadows, _) if shadows.len() == 1
    ));
    assert!(matches!(
        ops.first(),
        Some(crate::view::paint::PaintOp::PreparedShadow(_))
    ));
    assert!(matches!(
        ops.last(),
        Some(crate::view::paint::PaintOp::PreparedSvg(_))
    ));
    let [clip] = clip_snapshot.as_slice() else {
        panic!("exact clipped Svg must carry one complete self-clip snapshot")
    };
    assert_eq!(clip.id, full_chunk.properties.clip.unwrap());
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
