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
        .shadow_paint_recording_context(Default::default());
    assert_eq!(
        node.element
            .shadow_paint_recording_capability(&arena, false, context),
        ShadowPaintRecordingCapability::Recordable
    );
    assert!(
        node.element
            .retained_child_mask_plan(&arena, context)
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
            false,
            true,
            CoverageRecordingMode::FullArtifact,
            &properties,
            &generations,
        );
        assert!(manifest.validation_errors.is_empty(), "{state:?}");
        assert_eq!(
            manifest.stats().legacy_boundaries,
            0,
            "{state:?}: {manifest:?}"
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
            .shadow_paint_recording_context(Default::default());
        assert_eq!(
            node.element
                .shadow_paint_recording_capability(&arena, false, context),
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
            .shadow_paint_recording_context(Default::default());
        assert_eq!(
            node.element
                .shadow_paint_recording_capability(&arena, false, context),
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
            node.element
                .shadow_paint_recording_context(Default::default()),
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
    assert!(metadata.validation_errors.is_empty());
    assert!(full.validation_errors.is_empty());
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
fn ready_svg_rejects_invalid_inactive_roots_and_children_mirror_drift() {
    enum Drift {
        Missing,
        Duplicate,
        WrongParent,
        ChildrenMirror,
    }

    for (index, drift) in [
        Drift::Missing,
        Drift::Duplicate,
        Drift::WrongParent,
        Drift::ChildrenMirror,
    ]
    .into_iter()
    .enumerate()
    {
        let (mut arena, owner, loading_root, _, _, _) =
            prepared_ready_svg_with_inactive_slots(0x93d0 + index as u64 * 0x10);
        match drift {
            Drift::Missing => {
                arena.remove_subtree(loading_root);
            }
            Drift::Duplicate => {
                arena.with_element_taken(owner, |element, _arena| {
                    element
                        .as_any_mut()
                        .downcast_mut::<Svg>()
                        .unwrap()
                        .error_slot = vec![loading_root];
                });
            }
            Drift::WrongParent => arena.set_parent(loading_root, None),
            Drift::ChildrenMirror => arena.set_children(owner, vec![loading_root]),
        }
        assert_missing_prepared_svg_hooks(&arena, owner);
    }
}

#[test]
fn ready_svg_inactive_slots_do_not_bypass_source_raster_or_request_drift() {
    enum Drift {
        Source,
        Raster,
        Request,
    }

    for (index, drift) in [Drift::Source, Drift::Raster, Drift::Request]
        .into_iter()
        .enumerate()
    {
        let (arena, owner, ..) =
            prepared_ready_svg_with_inactive_slots(0x9420 + index as u64 * 0x10);
        let mut restore_key = None;
        {
            let mut node = arena.get_mut(owner).unwrap();
            let svg = node.element.as_any_mut().downcast_mut::<Svg>().unwrap();
            match drift {
                Drift::Source => {
                    restore_key = Some((true, svg.source_key));
                    svg.source_key = svg.source_key.wrapping_add(0x1000);
                }
                Drift::Raster => {
                    restore_key = Some((false, svg.active_raster_key.unwrap()));
                    svg.active_raster_key = svg.active_raster_key.map(|key| key + 0x1000);
                }
                Drift::Request => {
                    let request = svg.active_raster_request.unwrap();
                    svg.active_raster_request = Some(SvgRasterRequest::new(
                        request.physical_width.saturating_add(8),
                        request.physical_height,
                        request.mode,
                    ));
                }
            }
        }
        assert_missing_prepared_svg_hooks(&arena, owner);
        if let Some((source, key)) = restore_key {
            let mut node = arena.get_mut(owner).unwrap();
            let svg = node.element.as_any_mut().downcast_mut::<Svg>().unwrap();
            if source {
                svg.source_key = key;
            } else {
                svg.active_raster_key = Some(key);
            }
        }
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
            crate::view::paint::PaintRecordingContext::default(),
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
            crate::view::paint::PaintRecordingContext::default(),
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
            crate::view::paint::PaintRecordingContext::default(),
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
            Default::default(),
        )
        .expect("eligible Path SVG metadata");
    let path_artifact = path_node
        .element
        .record_shadow_paint_artifact(
            path_root,
            Default::default(),
            revision,
            &path_arena,
            Default::default(),
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
            Default::default(),
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
            Default::default(),
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
            Default::default(),
        )
        .expect("next-frame Path artifact");
    let Some(crate::view::paint::PaintOp::PreparedSvg(next)) = next_artifact.ops.last() else {
        unreachable!()
    };
    let next_identity = crate::view::paint::PreparedSvgIdentity::from_op(next).unwrap();
    assert_eq!(next_identity.generation, new_generation);
    assert_eq!(next_identity.pixel_ptr, new_pixels.as_ptr() as usize);
}

#[test]
fn path_document_and_raster_loading_error_record_wrappers_but_invalid_ready_fails() {
    fn assert_wrapper(svg: Svg) {
        let mut arena = new_test_arena();
        let root = commit_element(&mut arena, Box::new(svg));
        let node = arena.get(root).unwrap();
        assert_eq!(
            node.element
                .shadow_paint_recording_capability(&arena, false, Default::default()),
            ShadowPaintRecordingCapability::Recordable
        );
        let revision = crate::view::paint::PaintContentRevision {
            self_paint_revision: 1,
            composite_revision: 1,
            topology_revision: 1,
        };
        let metadata = node
            .element
            .record_shadow_paint_metadata(
                root,
                Default::default(),
                revision,
                &arena,
                Default::default(),
            )
            .unwrap();
        let artifact = node
            .element
            .record_shadow_paint_artifact(
                root,
                Default::default(),
                revision,
                &arena,
                Default::default(),
            )
            .unwrap();
        assert_eq!(
            metadata.id.role,
            crate::view::paint::PaintChunkRole::SelfDecoration
        );
        assert!(
            artifact
                .ops
                .iter()
                .all(|op| { !matches!(op, crate::view::paint::PaintOp::PreparedSvg(_)) })
        );
    }

    fn assert_legacy(svg: Svg) {
        let mut arena = new_test_arena();
        let root = commit_element(&mut arena, Box::new(svg));
        assert_eq!(
            arena
                .get(root)
                .unwrap()
                .element
                .shadow_paint_recording_capability(&arena, false, Default::default()),
            ShadowPaintRecordingCapability::Legacy(ShadowPaintBlocker::MissingPreparedSvg)
        );
    }

    let mut document_loading =
        freeze_ready_svg(0x6b20, SvgSource::Path("document-loading.svg".into()), 1.0);
    set_svg_document_loading_for_test(document_loading.source_key);
    let mut sync_arena = new_test_arena();
    document_loading.sync_arena(&mut sync_arena);
    assert_wrapper(document_loading);

    let mut document_error =
        freeze_ready_svg(0x6b21, SvgSource::Path("document-error.svg".into()), 1.0);
    set_svg_document_error_for_test(document_error.source_key);
    document_error.sync_arena(&mut sync_arena);
    assert_wrapper(document_error);

    let mut raster_loading =
        freeze_ready_svg(0x6b22, SvgSource::Path("raster-loading.svg".into()), 1.0);
    set_svg_raster_loading_for_test(raster_loading.active_raster_key.unwrap());
    raster_loading.sync_arena(&mut sync_arena);
    assert_wrapper(raster_loading);

    let mut raster_error =
        freeze_ready_svg(0x6b23, SvgSource::Path("raster-error.svg".into()), 1.0);
    set_svg_raster_error_for_test(raster_error.active_raster_key.unwrap());
    raster_error.sync_arena(&mut sync_arena);
    assert_wrapper(raster_error);

    let mut invalid =
        freeze_ready_svg(0x6b24, SvgSource::Path("invalid-raster.svg".into()), 1.0);
    let request = invalid.active_raster_request.unwrap();
    replace_svg_raster_ready_for_test(
        invalid.active_raster_key.unwrap(),
        request.physical_width,
        request.physical_height,
        std::sync::Arc::from([1_u8, 2, 3]),
    );
    invalid.sync_arena(&mut sync_arena);
    invalid.prepare_frozen_paint(PaintResourcePreparationContext {
        frame_number: 3,
        device_scale: 1.0,
        now: Instant::now(),
    });
    assert_legacy(invalid);
}
