use super::*;

#[test]
fn image_replaces_inactive_error_and_active_loading_slots_atomically() {
    let mut arena = new_test_arena();
    let owner = commit_element(
        &mut arena,
        Box::new(Image::new_with_id(0x9100, rgba_source(1, 1))),
    );
    let (old_loading, old_loading_child) =
        insert_inactive_slot_subtree(&mut arena, owner, 0x9110);
    let (old_error, old_error_child) = insert_inactive_slot_subtree(&mut arena, owner, 0x9120);
    let (new_loading, _) = insert_inactive_slot_subtree(&mut arena, owner, 0x9130);
    let (new_error, _) = insert_inactive_slot_subtree(&mut arena, owner, 0x9140);

    arena.with_element_taken(owner, |element, arena| {
        let image = element.as_any_mut().downcast_mut::<Image>().unwrap();
        image.attach_loading_slot_cold(vec![old_loading]);
        image.attach_error_slot_cold(vec![old_error]);
        image.sync_active_slot(arena, ActiveSlot::Loading);
        assert_eq!(image.element.children(), &[old_loading]);
        assert_eq!(arena.children_of(owner), vec![old_loading]);

        image
            .replace_error_slot_incremental(arena, owner, &[new_error])
            .unwrap();
        assert_eq!(image.active_slot, ActiveSlot::None);
        assert_eq!(image.loading_slot, vec![old_loading]);
        assert_eq!(image.error_slot, vec![new_error]);
        assert!(image.element.children().is_empty());
        assert!(arena.children_of(owner).is_empty());

        image.sync_active_slot(arena, ActiveSlot::Loading);
        image
            .replace_loading_slot_incremental(arena, owner, &[new_loading])
            .unwrap();
        assert_eq!(image.active_slot, ActiveSlot::None);
        assert_eq!(image.loading_slot, vec![new_loading]);
        assert_eq!(image.error_slot, vec![new_error]);
        assert_eq!(arena.parent_of(new_loading), Some(owner));
        assert_eq!(arena.parent_of(new_error), Some(owner));
        assert_eq!(arena.children_of(owner), image.element.children());
    });

    assert!(!arena.contains_key(old_loading));
    assert!(!arena.contains_key(old_loading_child));
    assert!(!arena.contains_key(old_error));
    assert!(!arena.contains_key(old_error_child));
    assert!(arena.contains_key(new_loading));
    assert!(arena.contains_key(new_error));
}

#[test]
fn error_wrapper_rejects_wrong_parent_in_inactive_loading_slot() {
    let image = Image::new_with_id(0x9130, path_source("error-inactive-parent"));
    let asset_id = image.source_handle.asset_id();
    crate::view::image_resource::set_image_error_for_test(asset_id, "synthetic decode error");
    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(image));
    let inactive = arena.insert(Node::new(Box::new(Element::new_with_id(
        0x9131, 0.0, 0.0, 1.0, 1.0,
    ))));
    arena.with_element_taken(root, |element, _arena| {
        element
            .as_any_mut()
            .downcast_mut::<Image>()
            .unwrap()
            .attach_loading_slot_cold(vec![inactive]);
    });
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
    let context = image_recording_context(&arena, root);
    let node = arena.get(root).unwrap();
    assert_eq!(
        node.element
            .shadow_paint_recording_capability(&arena, false, context),
        ShadowPaintRecordingCapability::Legacy(ShadowPaintBlocker::MissingPreparedImage)
    );
    let revision = crate::view::paint::PaintContentRevision {
        self_paint_revision: 1,
        composite_revision: 1,
        topology_revision: 1,
    };
    assert!(
        node.element
            .record_shadow_paint_metadata(root, Default::default(), revision, &arena, context)
            .is_none()
    );
    assert!(
        node.element
            .record_shadow_paint_artifact(root, Default::default(), revision, &arena, context)
            .is_none()
    );
}

#[test]
fn loading_wrapper_rejects_inactive_root_aliasing_an_active_grandchild_and_topology_drift() {
    #[derive(Clone, Copy)]
    enum Drift {
        AliasOnly,
        Parent,
        ChildrenMirror,
    }

    fn fixture(id: u64, drift: Drift) -> (NodeArena, NodeKey, NodeKey, NodeKey) {
        let mut image = Image::new_with_id(id, path_source(&format!("alias-{id}")));
        let mut style = Style::new();
        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        image.apply_style(style);
        crate::view::image_resource::set_image_loading_for_test(image.source_handle.asset_id());

        let mut arena = new_test_arena();
        let owner = commit_element(&mut arena, Box::new(image));
        let (active_root, active_grandchild) =
            insert_inactive_slot_subtree(&mut arena, owner, id + 1);
        arena.with_element_taken(owner, |element, _arena| {
            let image = element.as_any_mut().downcast_mut::<Image>().unwrap();
            image.attach_loading_slot_cold(vec![active_root]);
            image.attach_error_slot_cold(vec![active_grandchild]);
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

        if matches!(drift, Drift::Parent) {
            // Reproduce the reviewer case: the inactive root now appears
            // directly owned by Image while the active root still reaches
            // it through its frozen child edge.
            arena.set_parent(active_grandchild, Some(owner));
        }
        if matches!(drift, Drift::ChildrenMirror) {
            // Node topology retains the edge while the active Element
            // mirror is stale. DFS must reject this independently of the
            // inactive-root alias check.
            arena.with_element_taken(active_root, |element, _arena| {
                element.sync_children_mirror(&[]);
            });
        }
        (arena, owner, active_root, active_grandchild)
    }

    for (index, drift) in [Drift::AliasOnly, Drift::Parent, Drift::ChildrenMirror]
        .into_iter()
        .enumerate()
    {
        let (arena, owner, _active_root, _active_grandchild) =
            fixture(0x9160 + index as u64 * 0x10, drift);
        let context = image_recording_context(&arena, owner);
        let node = arena.get(owner).unwrap();
        assert_eq!(
            node.element
                .shadow_paint_recording_capability(&arena, false, context),
            ShadowPaintRecordingCapability::Legacy(ShadowPaintBlocker::MissingPreparedImage)
        );
        let revision = crate::view::paint::PaintContentRevision {
            self_paint_revision: 1,
            composite_revision: 1,
            topology_revision: 1,
        };
        assert!(
            node.element
                .record_shadow_paint_metadata(
                    owner,
                    Default::default(),
                    revision,
                    &arena,
                    context,
                )
                .is_none()
        );
        assert!(
            node.element
                .record_shadow_paint_artifact(
                    owner,
                    Default::default(),
                    revision,
                    &arena,
                    context,
                )
                .is_none()
        );
    }
}

#[test]
fn loading_wrapper_coverage_traverses_only_the_active_slot_subtree_in_canonical_order() {
    use crate::view::paint::{
        CoverageRecordingMode, PaintCoverageItem, PaintNodePhase, PaintPropertyScope,
        record_coverage_manifest,
    };

    let mut image = Image::new_with_id(0x9140, path_source("loading-active-subtree"));
    let mut image_style = Style::new();
    image_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    image_style.set_box_shadow(vec![
        BoxShadow::new()
            .color(Color::rgb(220, 30, 20))
            .offset_x(1.5)
            .offset_y(-2.25),
    ]);
    image.apply_style(image_style);
    let asset_id = image.source_handle.asset_id();
    crate::view::image_resource::set_image_loading_for_test(asset_id);
    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(image));
    let (loading_root, loading_child) = insert_inactive_slot_subtree(&mut arena, root, 0x9141);
    let (inactive_error_root, inactive_error_child) =
        insert_inactive_slot_subtree(&mut arena, root, 0x9151);
    arena.with_element_taken(root, |element, _arena| {
        let image = element.as_any_mut().downcast_mut::<Image>().unwrap();
        image.attach_loading_slot_cold(vec![loading_root]);
        image.attach_error_slot_cold(vec![inactive_error_root]);
    });
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

    let node = arena.get(root).unwrap();
    assert_eq!(node.children(), &[loading_root]);
    assert_eq!(node.element.children(), &[loading_root]);
    drop(node);
    assert_eq!(arena.parent_of(loading_root), Some(root));
    assert_eq!(arena.parent_of(inactive_error_root), Some(root));

    let roots = [root];
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

    let summarize = |manifest: &crate::view::paint::PaintCoverageManifest| {
        manifest
            .items
            .iter()
            .map(|item| match item {
                PaintCoverageItem::ArtifactChunk { chunk, .. } => (
                    chunk.owner,
                    chunk.id.scope,
                    chunk.id.phase,
                    chunk.id.slot,
                    chunk.id.role,
                    chunk.payload_identity.clone(),
                ),
                other => panic!("unexpected coverage item: {other:?}"),
            })
            .collect::<Vec<_>>()
    };
    let metadata_summary = summarize(&metadata);
    let full_summary = summarize(&full);
    assert_eq!(metadata_summary, full_summary);
    assert!(matches!(
        &metadata_summary[0].5,
        crate::view::paint::PaintPayloadIdentity::PreparedShadows(shadows, _)
            if shadows.len() == 1
    ));
    let PaintCoverageItem::ArtifactChunk {
        ops: Some(root_ops),
        ..
    } = &full.items[0]
    else {
        panic!("Image wrapper root must own full ops")
    };
    assert!(matches!(
        root_ops.first(),
        Some(crate::view::paint::PaintOp::PreparedShadow(shadow))
            if shadow.params.opacity.to_bits() == 1.0_f32.to_bits()
    ));
    assert!(
        root_ops
            .iter()
            .all(|op| !matches!(op, crate::view::paint::PaintOp::PreparedImage(_)))
    );
    assert_eq!(
        metadata_summary
            .iter()
            .map(|(owner, ..)| *owner)
            .collect::<Vec<_>>(),
        vec![root, loading_root, loading_child]
    );
    assert!(
        metadata_summary
            .iter()
            .all(|(_, scope, phase, slot, _, _)| {
                *scope == PaintPropertyScope::SelfPaint
                    && *phase == PaintNodePhase::BeforeChildren
                    && *slot == 0
            })
    );
    assert!(metadata_summary.iter().all(|(owner, ..)| {
        *owner != inactive_error_root && *owner != inactive_error_child
    }));
}

#[test]
fn arena_sync_loading_slot_topology_marks_layout_dirty_before_measure() {
    let mut arena = new_test_arena();
    let image_key = commit_element(
        &mut arena,
        Box::new(Image::new_with_id(32, rgba_source(1, 1))),
    );
    let slot_key = commit_child(
        &mut arena,
        image_key,
        Box::new(Element::new_with_id(33, 0.0, 0.0, 4.0, 4.0)),
    );
    arena.with_element_taken(image_key, |element, arena| {
        let image = element
            .as_any_mut()
            .downcast_mut::<Image>()
            .expect("image host");
        image.attach_loading_slot_cold(vec![slot_key]);
        crate::view::image_resource::set_image_loading_for_test(image.source_handle.asset_id());
        image.clear_local_dirty_flags(DirtyFlags::ALL);
        image.sync_arena(arena);
        assert_eq!(image.active_slot, super::super::ActiveSlot::Loading);
        assert_eq!(image.element.children(), &[slot_key]);
        assert!(image.local_dirty_flags().contains(DirtyFlags::LAYOUT));
    });
}
