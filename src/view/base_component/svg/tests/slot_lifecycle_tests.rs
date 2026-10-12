use super::*;

#[test]
fn svg_replaces_inactive_loading_and_active_error_slots_atomically() {
    let mut arena = new_test_arena();
    let owner = commit_element(&mut arena, Box::new(Svg::new_with_id(0x9200, simple_svg())));
    let (old_loading, old_loading_child) =
        insert_inactive_slot_subtree(&mut arena, owner, 0x9210);
    let (old_error, old_error_child) = insert_inactive_slot_subtree(&mut arena, owner, 0x9220);
    let (new_loading, _) = insert_inactive_slot_subtree(&mut arena, owner, 0x9230);
    let (new_error, _) = insert_inactive_slot_subtree(&mut arena, owner, 0x9240);

    arena.with_element_taken(owner, |element, arena| {
        let svg = element.as_any_mut().downcast_mut::<Svg>().unwrap();
        svg.attach_loading_slot_cold(vec![old_loading]);
        svg.attach_error_slot_cold(vec![old_error]);
        svg.sync_active_slot(arena, ActiveSlot::Error);
        assert_eq!(svg.element.children(), &[old_error]);
        assert_eq!(arena.children_of(owner), vec![old_error]);

        svg.replace_loading_slot_incremental(arena, owner, &[new_loading])
            .unwrap();
        assert_eq!(svg.active_slot, ActiveSlot::None);
        assert_eq!(svg.loading_slot, vec![new_loading]);
        assert_eq!(svg.error_slot, vec![old_error]);
        assert!(svg.element.children().is_empty());
        assert!(arena.children_of(owner).is_empty());

        svg.sync_active_slot(arena, ActiveSlot::Error);
        svg.replace_error_slot_incremental(arena, owner, &[new_error])
            .unwrap();
        assert_eq!(svg.active_slot, ActiveSlot::None);
        assert_eq!(svg.loading_slot, vec![new_loading]);
        assert_eq!(svg.error_slot, vec![new_error]);
        assert_eq!(arena.parent_of(new_loading), Some(owner));
        assert_eq!(arena.parent_of(new_error), Some(owner));
        assert_eq!(arena.children_of(owner), svg.element.children());
    });

    assert!(!arena.contains_key(old_loading));
    assert!(!arena.contains_key(old_loading_child));
    assert!(!arena.contains_key(old_error));
    assert!(!arena.contains_key(old_error_child));
    assert!(arena.contains_key(new_loading));
    assert!(arena.contains_key(new_error));
}

#[test]
fn loading_and_error_wrappers_record_active_subtree_in_canonical_order() {
    use crate::view::paint::{
        CoverageRecordingMode, PaintCoverageItem, PaintNodePhase, PaintPropertyScope,
        record_coverage_manifest,
    };

    for (index, state) in [ActiveSlot::Loading, ActiveSlot::Error]
        .into_iter()
        .enumerate()
    {
        let (arena, owner, active_root, active_child, inactive_root, inactive_child) =
            active_slot_svg_fixture(0x9250 + index as u64 * 0x20, state);
        let node = arena.get(owner).unwrap();
        assert_eq!(node.children(), &[active_root]);
        assert_eq!(node.element.children(), &[active_root]);
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
            .unwrap();
        let artifact = node
            .element
            .record_shadow_paint_artifact(owner, Default::default(), revision, &arena, &context)
            .unwrap();
        assert_eq!(metadata.id.scope, PaintPropertyScope::SelfPaint);
        assert_eq!(metadata.id.phase, PaintNodePhase::BeforeChildren);
        assert_eq!(metadata.id.slot, 0);
        assert_eq!(
            metadata.id.role,
            crate::view::paint::PaintChunkRole::SelfDecoration
        );
        assert_eq!(
            artifact.chunks[0].payload_identity,
            metadata.payload_identity
        );
        assert!(matches!(
            &metadata.payload_identity,
            crate::view::paint::PaintPayloadIdentity::PreparedShadows(shadows, _)
                if shadows.len() == 1
        ));
        assert!(matches!(
            artifact.ops.first(),
            Some(crate::view::paint::PaintOp::PreparedShadow(_))
        ));
        assert!(
            artifact
                .ops
                .iter()
                .all(|op| { !matches!(op, crate::view::paint::PaintOp::PreparedSvg(_)) })
        );
        drop(node);

        let roots = [owner];
        let mut properties = PropertyTrees::default();
        properties.sync(&arena, &roots);
        let mut generations = PaintGenerationTracker::default();
        generations.sync(&arena, &roots, &properties);
        let record = |mode: CoverageRecordingMode| {
            record_coverage_manifest(&arena, &roots, true, mode, &properties, &generations)
        };
        let metadata_manifest = record(CoverageRecordingMode::MetadataOnly);
        let full_manifest = record(CoverageRecordingMode::FullArtifact);
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
        let metadata_summary = summarize(&metadata_manifest);
        assert_eq!(metadata_summary, summarize(&full_manifest));
        assert_eq!(
            metadata_summary
                .iter()
                .map(|(owner, ..)| *owner)
                .collect::<Vec<_>>(),
            vec![owner, active_root, active_child]
        );
        assert!(metadata_summary.iter().all(|(recorded, ..)| {
            *recorded != inactive_root && *recorded != inactive_child
        }));
    }
}
