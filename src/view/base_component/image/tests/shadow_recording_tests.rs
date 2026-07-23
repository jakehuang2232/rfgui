use super::*;

#[test]
fn error_wrapper_outer_shadow_records_before_active_subtree_and_excludes_inactive_slot() {
    use crate::view::paint::{
        CoverageRecordingMode, PaintCoverageItem, record_coverage_manifest,
    };

    let mut image = Image::new_with_id(0x9170, path_source("error-shadow-subtree"));
    let mut style = Style::new();
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    style.set_box_shadow(vec![
        BoxShadow::new()
            .color(Color::rgb(20, 40, 220))
            .offset_x(-3.0)
            .offset_y(4.5),
    ]);
    image.apply_style(style);
    crate::view::image_resource::set_image_error_for_test(
        image.source_handle.asset_id(),
        "synthetic decode error",
    );
    let mut arena = new_test_arena();
    let owner = commit_element(&mut arena, Box::new(image));
    let (loading_root, loading_child) = insert_inactive_slot_subtree(&mut arena, owner, 0x9171);
    let (error_root, error_child) = insert_inactive_slot_subtree(&mut arena, owner, 0x9181);
    arena.with_element_taken(owner, |element, _arena| {
        let image = element.as_any_mut().downcast_mut::<Image>().unwrap();
        image.attach_loading_slot_cold(vec![loading_root]);
        image.attach_error_slot_cold(vec![error_root]);
    });
    measure_and_place(
        &mut arena,
        owner,
        LayoutConstraints {
            max_width: 100.0,
            max_height: 100.0,
            viewport_width: 100.0,
            viewport_height: 100.0,
            percent_base_width: None,
            percent_base_height: None,
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
            percent_base_width: None,
            percent_base_height: None,
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
    let metadata = record(CoverageRecordingMode::MetadataOnly);
    let full = record(CoverageRecordingMode::FullArtifact);
    assert!(metadata.validation_errors.is_empty());
    assert!(full.validation_errors.is_empty());
    assert_eq!(
        metadata
            .items
            .iter()
            .map(|item| match item {
                PaintCoverageItem::ArtifactChunk { chunk, .. } => chunk.owner,
                other => panic!("unexpected coverage item: {other:?}"),
            })
            .collect::<Vec<_>>(),
        vec![owner, error_root, error_child]
    );
    let PaintCoverageItem::ArtifactChunk {
        chunk: metadata_chunk,
        ..
    } = &metadata.items[0]
    else {
        unreachable!()
    };
    let PaintCoverageItem::ArtifactChunk {
        chunk: full_chunk,
        ops: Some(ops),
        ..
    } = &full.items[0]
    else {
        unreachable!()
    };
    assert_eq!(metadata_chunk.payload_identity, full_chunk.payload_identity);
    assert!(matches!(
        &metadata_chunk.payload_identity,
        crate::view::paint::PaintPayloadIdentity::PreparedShadows(shadows, _)
            if shadows.len() == 1
    ));
    assert!(matches!(
        ops.first(),
        Some(crate::view::paint::PaintOp::PreparedShadow(_))
    ));
    assert!(
        ops.iter()
            .all(|op| !matches!(op, crate::view::paint::PaintOp::PreparedImage(_)))
    );
    assert!(metadata.items.iter().all(|item| !matches!(
        item,
        PaintCoverageItem::ArtifactChunk { chunk, .. }
            if chunk.owner == loading_root || chunk.owner == loading_child
    )));
}

#[test]
fn ready_image_media_with_outer_shadow_records_typed_shadow_prefix() {
    let (mut arena, owner, ..) = prepared_ready_image(
        0x9190,
        path_source("ready-shadow-fallback"),
        2,
        2,
        std::sync::Arc::from([0x4d_u8; 16]),
    );
    {
        let mut node = arena.get_mut(owner).unwrap();
        let image = node.element.as_any_mut().downcast_mut::<Image>().unwrap();
        let mut style = Style::new();
        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        style.set_box_shadow(vec![BoxShadow::new().offset_x(1.0)]);
        image.apply_style(style);
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
            parent_x: 1.25,
            parent_y: 2.75,
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
    let context = image_recording_context(&arena, owner);
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
        .expect("ready shadow Image metadata");
    let artifact = node
        .element
        .record_shadow_paint_artifact(owner, Default::default(), revision, &arena, context)
        .expect("ready shadow Image artifact");
    assert_eq!(
        artifact.chunks[0].payload_identity,
        metadata.payload_identity
    );
    assert!(
        matches!(
            &metadata.payload_identity,
            crate::view::paint::PaintPayloadIdentity::ImageWithShadows(_, shadows, _)
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
            crate::view::paint::PaintOp::PreparedImage(_)
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
            crate::view::paint::PaintPayloadIdentity::ImageWithShadows(
                media,
                shadows,
                decoration,
            ) => (media.clone(), shadows.clone(), decoration.clone()),
            _ => unreachable!(),
        };
    drop(node);
    {
        let mut node = arena.get_mut(owner).unwrap();
        let image = node.element.as_any_mut().downcast_mut::<Image>().unwrap();
        let mut style = Style::new();
        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        style.set_box_shadow(vec![BoxShadow::new().offset_x(9.0).offset_y(-4.0)]);
        image.apply_style(style);
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
            parent_x: 1.25,
            parent_y: 2.75,
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
    let changed_context = image_recording_context(&arena, owner);
    let changed = node
        .element
        .record_shadow_paint_metadata(
            owner,
            Default::default(),
            revision,
            &arena,
            changed_context,
        )
        .expect("shadow-mutated Image metadata");
    let crate::view::paint::PaintPayloadIdentity::ImageWithShadows(
        changed_media,
        changed_shadows,
        changed_decoration,
    ) = changed.payload_identity
    else {
        panic!("shadow-mutated Image must retain typed media identity")
    };
    assert_eq!(changed_media, baseline_media);
    assert_ne!(changed_shadows, baseline_shadows);
    assert_eq!(changed_decoration, baseline_decoration);
}

#[test]
fn ready_image_exact_self_clip_shadow_metadata_and_full_are_canonical() {
    use crate::view::paint::{
        CoverageRecordingMode, PaintCoverageItem, PaintPayloadIdentity,
        record_coverage_manifest,
    };

    let (mut arena, owner, ..) = prepared_ready_image(
        0x9191,
        path_source("ready-exact-self-clip-shadow"),
        2,
        2,
        std::sync::Arc::from([0x5a_u8; 16]),
    );
    {
        let mut node = arena.get_mut(owner).unwrap();
        let image = node.element.as_any_mut().downcast_mut::<Image>().unwrap();
        let mut style = Style::new();
        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(1.25))
                    .top(Length::px(2.75))
                    .clip(ClipMode::AnchorParent),
            ),
        );
        style.set_box_shadow(vec![BoxShadow::new().offset_x(-3.0).offset_y(4.5)]);
        image.apply_style(style);
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
    let roots = [owner];
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &roots);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &roots, &properties);
    let state = properties.paint_state_for(owner).unwrap();
    let node = arena.get(owner).unwrap();
    let mut direct_context = node
        .element
        .shadow_paint_recording_context(Default::default());
    direct_context.is_frame_root = true;
    direct_context.recording_owner = Some(owner);
    direct_context.recording_owner_stable_id = Some(node.element.stable_id());
    direct_context.authoritative_self_clip =
        properties.authoritative_self_clip_for_owner(owner, state);
    assert_eq!(
        node.element
            .shadow_paint_recording_capability(&arena, false, direct_context),
        ShadowPaintRecordingCapability::Recordable,
        "state={state:?} context={direct_context:?}"
    );
    drop(node);
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
            "exact clipped Image metadata must remain one native chunk: {:?}",
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
        panic!("exact clipped Image full recording must carry its clip snapshot")
    };
    assert_eq!(metadata.payload_identity, full_chunk.payload_identity);
    assert!(matches!(
        &metadata.payload_identity,
        PaintPayloadIdentity::ImageWithShadows(_, shadows, _) if shadows.len() == 1
    ));
    assert!(matches!(
        ops.first(),
        Some(crate::view::paint::PaintOp::PreparedShadow(_))
    ));
    assert!(matches!(
        ops.last(),
        Some(crate::view::paint::PaintOp::PreparedImage(_))
    ));
    let [clip] = clip_snapshot.as_slice() else {
        panic!("exact clipped Image must carry one complete self-clip snapshot")
    };
    assert_eq!(clip.id, full_chunk.properties.clip.unwrap());
}

#[test]
fn image_wrapper_outer_shadow_root_opacity_is_applied_once() {
    let mut image = Image::new_with_id(0x9195, path_source("shadow-root-opacity"));
    let mut style = Style::new();
    style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::rgb(20, 180, 40)),
    );
    style.insert(
        PropertyId::Opacity,
        ParsedValue::Opacity(crate::style::Opacity::new(0.4)),
    );
    style.set_border(crate::style::Border::uniform(
        Length::px(2.0),
        &Color::hex("#102030"),
    ));
    style.set_box_shadow(vec![BoxShadow::new().offset_x(1.5).offset_y(-2.25)]);
    image.apply_style(style);
    crate::view::image_resource::set_image_loading_for_test(image.source_handle.asset_id());
    let mut arena = new_test_arena();
    let owner = commit_element(&mut arena, Box::new(image));
    measure_and_place(
        &mut arena,
        owner,
        LayoutConstraints {
            max_width: 100.0,
            max_height: 100.0,
            viewport_width: 100.0,
            viewport_height: 100.0,
            percent_base_width: None,
            percent_base_height: None,
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
            percent_base_width: None,
            percent_base_height: None,
        },
    );
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
        panic!("Image wrapper root-opacity must record")
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
            crate::view::paint::PaintOp::DrawRect(border),
            ..
        ] if shadow.params.opacity.to_bits() == 1.0_f32.to_bits()
            && fill.params.opacity.to_bits() == 1.0_f32.to_bits()
            && border.params.opacity.to_bits() == 1.0_f32.to_bits()
    ));
}
