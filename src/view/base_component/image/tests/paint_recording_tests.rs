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
            .shadow_paint_recording_context(&Default::default());
        assert_eq!(
            node.element
                .shadow_paint_recording_capability(&arena, false, &context),
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
            .shadow_paint_recording_context(&Default::default());
        assert_eq!(
            node.element
                .shadow_paint_recording_capability(&arena, false, &context),
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
            &node.element
                .shadow_paint_recording_context(&Default::default()),
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
