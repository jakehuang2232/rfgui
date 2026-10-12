use super::*;

#[test]
fn rounded_loading_and_error_svg_wrappers_record_exact_child_mask_scope() {
    use crate::view::paint::{
        CoverageRecordingMode, PaintCoverageItem, PaintNodePhase, RETAINED_CHILD_MASK_SLOT,
        record_coverage_manifest,
    };

    let mut svg = freeze_ready_svg(0x9308, unique_svg("rounded-ready"), 1.0);
    let mut style = Style::new();
    style.set_border_radius(BorderRadius::uniform(crate::style::Length::px(12.0)));
    svg.apply_style(style);
    let mut arena = new_test_arena();
    let owner = commit_element(&mut arena, Box::new(svg));
    let node = arena.get(owner).unwrap();
    let context = node
        .element
        .shadow_paint_recording_context(&Default::default());
    assert_eq!(
        node.element
            .shadow_paint_recording_capability(&arena, false, &context),
        ShadowPaintRecordingCapability::Recordable
    );
    assert!(
        node.element
            .retained_child_mask_plan(&arena, &context)
            .is_none(),
        "ready Svg has no active slot children, so no child-mask scope is required"
    );
    drop(node);

    for (index, state) in [ActiveSlot::Loading, ActiveSlot::Error]
        .into_iter()
        .enumerate()
    {
        let (mut arena, owner, active_root, ..) =
            active_slot_svg_fixture(0x9320 + index as u64 * 0x20, state);
        make_svg_wrapper_rounded(&mut arena, owner);
        let roots = [owner];
        let mut properties = PropertyTrees::default();
        properties.sync(&arena, &roots);
        let mut generations = PaintGenerationTracker::default();
        generations.sync(&arena, &roots, &properties);
        let manifest = record_coverage_manifest(
            &arena,
            &roots,
            true,
            CoverageRecordingMode::FullArtifact,
            &properties,
            &generations,
        );
        let mask_indices = manifest
            .items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| match item {
                PaintCoverageItem::ArtifactChunk { chunk, .. }
                    if chunk.owner == owner && chunk.id.slot == RETAINED_CHILD_MASK_SLOT =>
                {
                    Some((index, chunk.id.phase))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            mask_indices,
            vec![
                (1, PaintNodePhase::BeforeChildren),
                (4, PaintNodePhase::AfterChildren),
            ],
            "{state:?}"
        );
        assert!(matches!(
            manifest.items.get(2),
            Some(PaintCoverageItem::ArtifactChunk { chunk, .. })
                if chunk.owner == active_root
        ));
    }
}

#[test]
fn canonical_hidden_svg_wrappers_cull_but_visible_active_slots_do_not() {
    for (index, state) in [ActiveSlot::Loading, ActiveSlot::Error]
        .into_iter()
        .enumerate()
    {
        let (mut arena, owner, ..) =
            active_slot_svg_fixture(0x9380 + index as u64 * 0x20, state);
        let node = arena.get(owner).unwrap();
        let context = node
            .element
            .shadow_paint_recording_context(&Default::default());
        assert_eq!(
            node.element
                .shadow_paint_recording_capability(&arena, false, &context),
            ShadowPaintRecordingCapability::Recordable,
            "visible {state:?} child subtree must remain traversable"
        );
        drop(node);
        arena.with_element_taken(owner, |element, _arena| {
            let svg = element.as_any_mut().downcast_mut::<Svg>().unwrap();
            svg.element.layout_state.should_render = false;
            svg.element.layout_state.layout_size = Size {
                width: 0.0,
                height: 0.0,
            };
        });
        let node = arena.get(owner).unwrap();
        let context = node
            .element
            .shadow_paint_recording_context(&Default::default());
        assert_eq!(
            node.element
                .shadow_paint_recording_capability(&arena, false, &context),
            ShadowPaintRecordingCapability::CulledSubtree,
            "zero-area {state:?} wrapper culls its complete subtree"
        );
    }

    let svg = freeze_ready_svg(0x93c0, unique_svg("culled-ready"), 1.0);
    let mut arena = new_test_arena();
    let owner = commit_element(&mut arena, Box::new(svg));
    arena.with_element_taken(owner, |element, _arena| {
        let svg = element.as_any_mut().downcast_mut::<Svg>().unwrap();
        svg.element.layout_state.should_render = false;
    });
    let node = arena.get(owner).unwrap();
    assert_eq!(
        node.element.shadow_paint_recording_capability(
            &arena,
            false,
            &node.element
                .shadow_paint_recording_context(&Default::default()),
        ),
        ShadowPaintRecordingCapability::CulledSubtree
    );
}

#[test]
fn ready_svg_with_two_inactive_subtrees_records_only_svg_content() {
    use crate::view::paint::{
        CoverageRecordingMode, PaintCoverageItem, PaintNodePhase, PaintPropertyScope,
        record_coverage_manifest,
    };

    let (arena, owner, loading_root, loading_child, error_root, error_child) =
        prepared_ready_svg_with_inactive_slots(0x93a0);
    let node = arena.get(owner).unwrap();
    assert!(node.children().is_empty());
    assert!(node.element.children().is_empty());
    drop(node);
    assert_eq!(arena.parent_of(loading_root), Some(owner));
    assert_eq!(arena.parent_of(error_root), Some(owner));

    let roots = [owner];
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &roots);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &roots, &properties);
    let metadata = record_coverage_manifest(
        &arena,
        &roots,
        true,
        CoverageRecordingMode::MetadataOnly,
        &properties,
        &generations,
    );
    let full = record_coverage_manifest(
        &arena,
        &roots,
        true,
        CoverageRecordingMode::FullArtifact,
        &properties,
        &generations,
    );
    let [
        PaintCoverageItem::ArtifactChunk {
            chunk: metadata_chunk,
            ops: None,
            ..
        },
    ] = metadata.items.as_slice()
    else {
        panic!("Ready SVG metadata must contain only SvgContent")
    };
    let [
        PaintCoverageItem::ArtifactChunk {
            chunk: full_chunk,
            ops: Some(full_ops),
            ..
        },
    ] = full.items.as_slice()
    else {
        panic!("Ready SVG full recording must contain only SvgContent")
    };
    assert_eq!(metadata_chunk.id, full_chunk.id);
    assert_eq!(metadata_chunk.payload_identity, full_chunk.payload_identity);
    assert_eq!(metadata_chunk.owner, owner);
    assert_eq!(metadata_chunk.id.scope, PaintPropertyScope::SelfPaint);
    assert_eq!(metadata_chunk.id.phase, PaintNodePhase::BeforeChildren);
    assert_eq!(metadata_chunk.id.slot, 0);
    assert_eq!(
        metadata_chunk.id.role,
        crate::view::paint::PaintChunkRole::SvgContent
    );
    assert!(
        full_ops
            .iter()
            .any(|op| matches!(op, crate::view::paint::PaintOp::PreparedSvg(_)))
    );
    for inactive in [loading_root, loading_child, error_root, error_child] {
        assert!(metadata.items.iter().all(|item| !matches!(
            item,
            PaintCoverageItem::ArtifactChunk { chunk, .. } if chunk.owner == inactive
        )));
        assert!(full.items.iter().all(|item| !matches!(
            item,
            PaintCoverageItem::ArtifactChunk { chunk, .. } if chunk.owner == inactive
        )));
    }
}

#[test]
fn content_and_path_ready_record_matching_typed_owning_artifacts() {
    let mut arena = new_test_arena();
    let root = commit_element(
        &mut arena,
        Box::new(freeze_ready_svg(65, simple_svg(), 1.0)),
    );
    let node = arena.get(root).unwrap();
    assert_eq!(
        node.element.shadow_paint_recording_capability(
            &arena,
            false,
            &crate::view::paint::PaintRecordingContext::default(),
        ),
        ShadowPaintRecordingCapability::Recordable
    );
    let artifact = node
        .element
        .record_shadow_paint_artifact(
            root,
            crate::view::compositor::property_tree::PropertyTreeState::default(),
            crate::view::paint::PaintContentRevision {
                self_paint_revision: 1,
                composite_revision: 1,
                topology_revision: 1,
            },
            &arena,
            &crate::view::paint::PaintRecordingContext::default(),
        )
        .expect("eligible Content SVG should record");
    assert_eq!(
        artifact.chunks[0].id.role,
        crate::view::paint::PaintChunkRole::SvgContent
    );
    assert!(matches!(
        artifact.ops.last(),
        Some(crate::view::paint::PaintOp::PreparedSvg(_))
    ));
    let Some(crate::view::paint::PaintOp::PreparedSvg(owned)) = artifact.ops.last() else {
        unreachable!();
    };
    let owned_identity = crate::view::paint::PreparedSvgIdentity::from_op(owned).unwrap();
    let owned_pixels = owned.upload.pixels.clone();
    drop(node);
    drop(arena);
    let Some(crate::view::paint::PaintOp::PreparedSvg(owned_after_drop)) = artifact.ops.last()
    else {
        unreachable!();
    };
    assert_eq!(
        crate::view::paint::PreparedSvgIdentity::from_op(owned_after_drop),
        Some(owned_identity)
    );
    assert!(std::sync::Arc::ptr_eq(
        &owned_pixels,
        &owned_after_drop.upload.pixels
    ));

    let path_svg = freeze_ready_svg(
        66,
        SvgSource::Path(std::path::PathBuf::from("never-read-by-test.svg")),
        1.0,
    );
    let path_document_key = path_svg.source_key;
    let path_raster_key = path_svg.active_raster_key.expect("ready Path raster key");
    let path_pixels = path_svg
        .frozen_paint
        .as_ref()
        .expect("ready Path frozen paint")
        .upload
        .pixels
        .clone();
    let mut path_arena = new_test_arena();
    let path_root = commit_element(&mut path_arena, Box::new(path_svg));
    let path_node = path_arena.get(path_root).unwrap();
    assert_eq!(
        path_node.element.shadow_paint_recording_capability(
            &path_arena,
            false,
            &crate::view::paint::PaintRecordingContext::default(),
        ),
        ShadowPaintRecordingCapability::Recordable
    );
    let revision = crate::view::paint::PaintContentRevision {
        self_paint_revision: 2,
        composite_revision: 2,
        topology_revision: 2,
    };
    let metadata = path_node
        .element
        .record_shadow_paint_metadata(
            path_root,
            Default::default(),
            revision,
            &path_arena,
            &Default::default(),
        )
        .expect("eligible Path SVG metadata");
    let path_artifact = path_node
        .element
        .record_shadow_paint_artifact(
            path_root,
            Default::default(),
            revision,
            &path_arena,
            &Default::default(),
        )
        .expect("eligible Path SVG artifact");
    let Some(crate::view::paint::PaintOp::PreparedSvg(prepared)) = path_artifact.ops.last()
    else {
        panic!("Path artifact must own PreparedSvg")
    };
    let identity = crate::view::paint::PreparedSvgIdentity::from_op(prepared).unwrap();
    assert_eq!(identity.pixel_ptr, path_pixels.as_ptr() as usize);
    assert!(matches!(
        metadata.payload_identity,
        crate::view::paint::PaintPayloadIdentity::Svg(actual, _) if actual == identity
    ));
    assert_eq!(
        path_artifact.chunks[0].payload_identity,
        metadata.payload_identity
    );
    drop(path_node);
    drop(path_arena);
    remove_svg_raster_entry_for_test(path_raster_key);
    remove_svg_document_entry_for_test(path_document_key);
    let Some(crate::view::paint::PaintOp::PreparedSvg(prepared)) = path_artifact.ops.last()
    else {
        unreachable!()
    };
    assert!(std::sync::Arc::ptr_eq(
        &path_pixels,
        &prepared.upload.pixels
    ));
    assert!(prepared.upload.validate_rgba8().is_some());
}

#[test]
fn path_generation_and_pixel_arc_are_frozen_across_metadata_and_full_recording() {
    let svg = freeze_ready_svg(0x6b10, SvgSource::Path("generation-freeze.svg".into()), 1.0);
    let raster_key = svg.active_raster_key.expect("ready raster key");
    let request = svg.active_raster_request.expect("ready raster request");
    let old_generation = svg
        .frozen_paint
        .as_ref()
        .expect("ready frozen paint")
        .upload
        .generation;
    let old_pixel_ptr = svg.frozen_paint.as_ref().unwrap().upload.pixels.as_ptr() as usize;
    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(svg));
    let revision = crate::view::paint::PaintContentRevision {
        self_paint_revision: 7,
        composite_revision: 7,
        topology_revision: 7,
    };
    let metadata = arena
        .get(root)
        .unwrap()
        .element
        .record_shadow_paint_metadata(
            root,
            Default::default(),
            revision,
            &arena,
            &Default::default(),
        )
        .expect("Path metadata");
    let new_pixels: std::sync::Arc<[u8]> = std::sync::Arc::from(vec![
        9_u8;
        (request.physical_width * request.physical_height * 4)
            as usize
    ]);
    let new_generation = replace_svg_raster_ready_for_test(
        raster_key,
        request.physical_width,
        request.physical_height,
        new_pixels.clone(),
    );
    let artifact = arena
        .get(root)
        .unwrap()
        .element
        .record_shadow_paint_artifact(
            root,
            Default::default(),
            revision,
            &arena,
            &Default::default(),
        )
        .expect("same-frame Path artifact");
    let Some(crate::view::paint::PaintOp::PreparedSvg(prepared)) = artifact.ops.last() else {
        panic!("Path artifact must own PreparedSvg")
    };
    let frozen_identity = crate::view::paint::PreparedSvgIdentity::from_op(prepared).unwrap();
    assert_eq!(frozen_identity.generation, old_generation);
    assert_eq!(frozen_identity.pixel_ptr, old_pixel_ptr);
    assert!(matches!(
        metadata.payload_identity,
        crate::view::paint::PaintPayloadIdentity::Svg(actual, _) if actual == frozen_identity
    ));

    let mut replaced_arc = prepared.clone();
    replaced_arc.upload.pixels = new_pixels.clone();
    replaced_arc.upload.generation = old_generation;
    let replaced_identity =
        crate::view::paint::PreparedSvgIdentity::from_op(&replaced_arc).unwrap();
    assert_ne!(replaced_identity, frozen_identity);
    assert_ne!(replaced_identity.pixel_ptr, frozen_identity.pixel_ptr);

    arena.with_element_taken(root, |element, arena| {
        let svg = element.as_any_mut().downcast_mut::<Svg>().unwrap();
        svg.sync_arena(arena);
        svg.prepare_frozen_paint(PaintResourcePreparationContext {
            frame_number: 3,
            device_scale: 1.0,
            now: Instant::now(),
        });
    });
    let next_artifact = arena
        .get(root)
        .unwrap()
        .element
        .record_shadow_paint_artifact(
            root,
            Default::default(),
            revision,
            &arena,
            &Default::default(),
        )
        .expect("next-frame Path artifact");
    let Some(crate::view::paint::PaintOp::PreparedSvg(next)) = next_artifact.ops.last() else {
        unreachable!()
    };
    let next_identity = crate::view::paint::PreparedSvgIdentity::from_op(next).unwrap();
    assert_eq!(next_identity.generation, new_generation);
    assert_eq!(next_identity.pixel_ptr, new_pixels.as_ptr() as usize);
}
