use super::*;

#[test]
fn rounded_loading_and_error_image_wrappers_record_exact_child_mask_scope() {
    use crate::view::paint::{
        CoverageRecordingMode, PaintCoverageItem, PaintNodePhase, RETAINED_CHILD_MASK_SLOT,
        record_coverage_manifest,
    };

    let (mut arena, owner, ..) = prepared_ready_image(
        0x9188,
        path_source("rounded-ready"),
        2,
        2,
        std::sync::Arc::from([0x44_u8; 16]),
    );
    arena.with_element_taken(owner, |element, _arena| {
        let image = element.as_any_mut().downcast_mut::<Image>().unwrap();
        let mut style = Style::new();
        style.set_border_radius(BorderRadius::uniform(Length::px(12.0)));
        image.apply_style(style);
    });
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
        "ready Image has no active slot children, so no child-mask scope is required"
    );
    drop(node);

    for (index, state) in [ActiveSlot::Loading, ActiveSlot::Error]
        .into_iter()
        .enumerate()
    {
        let (arena, owner, active_root) =
            rounded_active_slot_image_fixture(0x91a0 + index as u64 * 0x20, state);
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
        assert_eq!(manifest.stats().legacy_boundaries, 0, "{state:?}");
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
fn canonical_hidden_image_wrappers_cull_but_visible_active_slots_do_not() {
    for (index, state) in [ActiveSlot::Loading, ActiveSlot::Error]
        .into_iter()
        .enumerate()
    {
        let (mut arena, owner, _) =
            rounded_active_slot_image_fixture(0x91e0 + index as u64 * 0x20, state);
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
            let image = element.as_any_mut().downcast_mut::<Image>().unwrap();
            image.element.layout_state.should_render = false;
            image.element.layout_state.layout_size = Size {
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

    let (mut arena, owner, ..) = prepared_ready_image(
        0x9220,
        path_source("culled-ready"),
        2,
        2,
        std::sync::Arc::from([0x55_u8; 16]),
    );
    arena.with_element_taken(owner, |element, _arena| {
        let image = element.as_any_mut().downcast_mut::<Image>().unwrap();
        image.element.layout_state.should_render = false;
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
fn ready_image_with_two_inactive_slot_subtrees_records_only_image_content() {
    use crate::view::paint::{
        CoverageRecordingMode, PaintCoverageItem, PaintNodePhase, PaintPropertyScope,
        record_coverage_manifest,
    };

    let (arena, root, loading_root, loading_child, error_root, error_child) =
        prepared_ready_image_with_inactive_slots(0x9180);
    let image_node = arena.get(root).unwrap();
    assert!(image_node.children().is_empty());
    assert!(image_node.element.children().is_empty());
    drop(image_node);
    assert_eq!(arena.parent_of(loading_root), Some(root));
    assert_eq!(arena.parent_of(error_root), Some(root));

    let roots = [root];
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
        panic!("Ready + inactive slots metadata must contain only ImageContent")
    };
    let [
        PaintCoverageItem::ArtifactChunk {
            chunk: full_chunk,
            ops: Some(full_ops),
            ..
        },
    ] = full.items.as_slice()
    else {
        panic!("Ready + inactive slots full recording must contain only ImageContent")
    };
    assert_eq!(metadata_chunk.id, full_chunk.id);
    assert_eq!(metadata_chunk.payload_identity, full_chunk.payload_identity);
    assert_eq!(metadata_chunk.owner, root);
    assert_eq!(metadata_chunk.id.scope, PaintPropertyScope::SelfPaint);
    assert_eq!(metadata_chunk.id.phase, PaintNodePhase::BeforeChildren);
    assert_eq!(metadata_chunk.id.slot, 0);
    assert_eq!(
        metadata_chunk.id.role,
        crate::view::paint::PaintChunkRole::ImageContent
    );
    assert!(
        full_ops
            .iter()
            .any(|op| matches!(op, crate::view::paint::PaintOp::PreparedImage(_)))
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
fn ready_image_rejects_invalid_inactive_roots_and_children_mirror_drift() {
    enum InvalidInactiveRoot {
        Missing,
        Duplicate,
        WrongParent,
        ChildrenMirror,
    }

    for (index, invalid) in [
        InvalidInactiveRoot::Missing,
        InvalidInactiveRoot::Duplicate,
        InvalidInactiveRoot::WrongParent,
        InvalidInactiveRoot::ChildrenMirror,
    ]
    .into_iter()
    .enumerate()
    {
        let id = 0x91a0 + index as u64 * 0x10;
        let (mut arena, root, _, _) = prepared_ready_image(
            id,
            path_source(&format!("ready-invalid-inactive-{id}")),
            2,
            2,
            std::sync::Arc::from([0x7c_u8; 16]),
        );
        match invalid {
            InvalidInactiveRoot::Missing => {
                let stale = arena.insert(Node::with_parent(
                    Box::new(Element::new_with_id(id + 1, 0.0, 0.0, 1.0, 1.0)),
                    Some(root),
                ));
                arena.with_element_taken(root, |element, _arena| {
                    element
                        .as_any_mut()
                        .downcast_mut::<Image>()
                        .unwrap()
                        .attach_loading_slot_cold(vec![stale]);
                });
                arena.remove_subtree(stale);
            }
            InvalidInactiveRoot::Duplicate => {
                let duplicate = arena.insert(Node::with_parent(
                    Box::new(Element::new_with_id(id + 1, 0.0, 0.0, 1.0, 1.0)),
                    Some(root),
                ));
                arena.with_element_taken(root, |element, _arena| {
                    let image = element.as_any_mut().downcast_mut::<Image>().unwrap();
                    image.attach_loading_slot_cold(vec![duplicate]);
                    image.attach_error_slot_cold(vec![duplicate]);
                });
            }
            InvalidInactiveRoot::WrongParent => {
                let wrong_parent = arena.insert(Node::new(Box::new(Element::new_with_id(
                    id + 1,
                    0.0,
                    0.0,
                    1.0,
                    1.0,
                ))));
                arena.with_element_taken(root, |element, _arena| {
                    element
                        .as_any_mut()
                        .downcast_mut::<Image>()
                        .unwrap()
                        .attach_error_slot_cold(vec![wrong_parent]);
                });
            }
            InvalidInactiveRoot::ChildrenMirror => {
                let mirrored_only = arena.insert(Node::with_parent(
                    Box::new(Element::new_with_id(id + 1, 0.0, 0.0, 1.0, 1.0)),
                    Some(root),
                ));
                arena.set_children(root, vec![mirrored_only]);
                arena.with_element_taken(root, |element, _arena| {
                    let image = element.as_any_mut().downcast_mut::<Image>().unwrap();
                    let mut style = Style::new();
                    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
                    image.apply_style(style);
                    image.element.sync_children_mirror(&[]);
                });
                assert_eq!(arena.children_of(root), vec![mirrored_only]);
            }
        }
        assert_missing_prepared_image_fallback(&arena, root);
    }
}

#[test]
fn ready_image_inactive_slots_do_not_bypass_current_handle_resource_drift() {
    let (arena, root, loading_root, _, error_root, _) =
        prepared_ready_image_with_inactive_slots(0x91d0);
    {
        let mut node = arena.get_mut(root).unwrap();
        let image = node.element.as_any_mut().downcast_mut::<Image>().unwrap();
        let stale = image.frozen_snapshot.clone().unwrap();
        image.set_source(path_source("ready-inactive-source-drift"));
        image.frozen_snapshot = Some(stale);
        image.prepared_by_arena_sync = true;
    }
    assert_eq!(arena.parent_of(loading_root), Some(root));
    assert_eq!(arena.parent_of(error_root), Some(root));
    assert_missing_prepared_image_fallback(&arena, root);
}

#[test]
fn path_loading_and_error_wrappers_record_canonical_decoration_while_invalid_ready_fails() {
    for (index, state) in ["loading", "error"].into_iter().enumerate() {
        let image = Image::new_with_id(0x9110 + index as u64, path_source(state));
        let asset_id = image.source_handle.asset_id();
        match state {
            "loading" => crate::view::image_resource::set_image_loading_for_test(asset_id),
            "error" => crate::view::image_resource::set_image_error_for_test(
                asset_id,
                "synthetic decode error",
            ),
            _ => unreachable!(),
        }
        let mut arena = new_test_arena();
        let root = commit_element(&mut arena, Box::new(image));
        measure_and_place(
            &mut arena,
            root,
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
        let (metadata, artifact) = record_image_metadata_and_artifact(&arena, root);
        assert_eq!(
            metadata.id.scope,
            crate::view::paint::PaintPropertyScope::SelfPaint,
            "{state}"
        );
        assert_eq!(
            metadata.id.phase,
            crate::view::paint::PaintNodePhase::BeforeChildren,
            "{state}"
        );
        assert_eq!(metadata.id.slot, 0, "{state}");
        assert_eq!(
            metadata.id.role,
            crate::view::paint::PaintChunkRole::SelfDecoration,
            "{state}"
        );
        assert_eq!(artifact.chunks.len(), 1, "{state}");
        let chunk = &artifact.chunks[0];
        assert_eq!(chunk.id, metadata.id, "{state}");
        assert_eq!(chunk.owner, metadata.owner, "{state}");
        assert_eq!(
            chunk.bounds.x.to_bits(),
            metadata.bounds.x.to_bits(),
            "{state}"
        );
        assert_eq!(
            chunk.bounds.y.to_bits(),
            metadata.bounds.y.to_bits(),
            "{state}"
        );
        assert_eq!(
            chunk.bounds.width.to_bits(),
            metadata.bounds.width.to_bits(),
            "{state}"
        );
        assert_eq!(
            chunk.bounds.height.to_bits(),
            metadata.bounds.height.to_bits(),
            "{state}"
        );
        assert_eq!(chunk.properties, metadata.properties, "{state}");
        assert_eq!(chunk.content_revision, metadata.content_revision, "{state}");
        assert_eq!(chunk.payload_identity, metadata.payload_identity, "{state}");
        assert!(
            artifact
                .ops
                .iter()
                .all(|op| !matches!(op, crate::view::paint::PaintOp::PreparedImage(_))),
            "{state} wrapper must not prepare image content"
        );
    }

    let invalid = Image::new_with_id(0x9112, path_source("invalid"));
    let invalid_asset_id = invalid.source_handle.asset_id();
    crate::view::image_resource::replace_ready_image_for_test(
        invalid_asset_id,
        2,
        2,
        std::sync::Arc::from([0_u8; 3]),
    );
    let mut invalid_arena = new_test_arena();
    let invalid_root = commit_element(&mut invalid_arena, Box::new(invalid));
    measure_and_place(
        &mut invalid_arena,
        invalid_root,
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
    let invalid_context = image_recording_context(&invalid_arena, invalid_root);
    assert_eq!(
        invalid_arena
            .get(invalid_root)
            .unwrap()
            .element
            .shadow_paint_recording_capability(&invalid_arena, false, invalid_context),
        ShadowPaintRecordingCapability::Legacy(ShadowPaintBlocker::MissingPreparedImage)
    );

    let rgba_pixels: std::sync::Arc<[u8]> = std::sync::Arc::from([21_u8; 16]);
    let (arena, root, asset_id, generation) =
        prepared_ready_image(0x9120, rgba_source(2, 2), 2, 2, rgba_pixels.clone());
    let (metadata, artifact) = record_image_metadata_and_artifact(&arena, root);
    let crate::view::paint::PaintPayloadIdentity::Image(identity, _) =
        &metadata.payload_identity
    else {
        unreachable!()
    };
    assert_eq!(
        (identity.sampled_texture_id, identity.generation),
        (SampledTextureId::Image(asset_id), generation)
    );
    assert_eq!(identity.pixel_ptr, rgba_pixels.as_ptr() as usize);
    assert!(
        artifact
            .ops
            .iter()
            .any(|op| matches!(op, crate::view::paint::PaintOp::PreparedImage(_)))
    );
}
