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
        .shadow_paint_recording_context(&Default::default());
    assert_eq!(
        node.element
            .shadow_paint_recording_capability(&arena, false, &context),
        ShadowPaintRecordingCapability::Recordable
    );
    let revision = crate::view::paint::PaintContentRevision {
        self_paint_revision: 1,
        composite_revision: 1,
        topology_revision: 1,
    };
    let metadata = node
        .element
        .record_shadow_paint_metadata(owner, Default::default(), revision, &arena, &context)
        .expect("ready shadow SVG metadata");
    let artifact = node
        .element
        .record_shadow_paint_artifact(owner, Default::default(), revision, &arena, &context)
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
    let mut reordered = artifact.clone();
    assert!(matches!(
        reordered.ops.get(1),
        Some(crate::view::paint::PaintOp::DrawRect(_))
    ));
    reordered.ops.swap(0, 1);
    let (baseline_media, baseline_shadows, baseline_decoration) = match &metadata.payload_identity {
        crate::view::paint::PaintPayloadIdentity::SvgWithShadows(media, shadows, decoration) => {
            (media.clone(), shadows.clone(), decoration.clone())
        }
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
        .shadow_paint_recording_context(&Default::default());
    let changed = node
        .element
        .record_shadow_paint_metadata(
            owner,
            Default::default(),
            revision,
            &arena,
            &changed_context,
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
        record_coverage_manifest(&arena, &roots, true, mode, &properties, &generations)
    };
    let metadata_manifest = record(CoverageRecordingMode::MetadataOnly);
    let full = record(CoverageRecordingMode::FullArtifact);
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
    let [clip] = clip_snapshot.as_ref() else {
        panic!("exact clipped Svg must carry one complete self-clip snapshot")
    };
    assert_eq!(clip.id, full_chunk.properties.clip.unwrap());
}
