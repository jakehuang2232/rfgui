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
            .unwrap();
        let artifact = node
            .element
            .record_shadow_paint_artifact(owner, Default::default(), revision, &arena, context)
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
            record_coverage_manifest(
                &arena,
                &roots,
                false,
                true,
                mode,
                &properties,
                &generations,
            )
        };
        let metadata_manifest = record(CoverageRecordingMode::MetadataOnly);
        let full_manifest = record(CoverageRecordingMode::FullArtifact);
        assert!(metadata_manifest.validation_errors.is_empty());
        assert!(full_manifest.validation_errors.is_empty());
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

#[test]
fn active_wrapper_topology_alias_and_resource_key_drift_fail_closed() {
    #[derive(Clone, Copy)]
    enum Drift {
        Alias,
        Parent,
        Mirror,
        ActiveRasterKey,
        PendingRasterKey,
    }

    for (index, drift) in [
        Drift::Alias,
        Drift::Parent,
        Drift::Mirror,
        Drift::ActiveRasterKey,
        Drift::PendingRasterKey,
    ]
    .into_iter()
    .enumerate()
    {
        let (mut arena, owner, active_root, active_child, inactive_root, _) =
            active_slot_svg_fixture(0x92a0 + index as u64 * 0x20, ActiveSlot::Loading);
        match drift {
            Drift::Alias => {
                arena.with_element_taken(owner, |element, _arena| {
                    element
                        .as_any_mut()
                        .downcast_mut::<Svg>()
                        .unwrap()
                        .error_slot = vec![active_child];
                });
            }
            Drift::Parent => arena.set_parent(inactive_root, Some(active_root)),
            Drift::Mirror => {
                arena.with_element_taken(active_root, |element, _arena| {
                    element.sync_children_mirror(&[]);
                });
            }
            Drift::ActiveRasterKey => {
                arena.with_element_taken(owner, |element, _arena| {
                    element
                        .as_any_mut()
                        .downcast_mut::<Svg>()
                        .unwrap()
                        .active_raster_key = Some(0xdead_1000 + index as u64);
                });
            }
            Drift::PendingRasterKey => {
                arena.with_element_taken(owner, |element, _arena| {
                    element
                        .as_any_mut()
                        .downcast_mut::<Svg>()
                        .unwrap()
                        .pending_raster_key = Some(0xdead_2000 + index as u64);
                });
            }
        }
        let node = arena.get(owner).unwrap();
        let context = node
            .element
            .shadow_paint_recording_context(Default::default());
        assert_eq!(
            node.element
                .shadow_paint_recording_capability(&arena, false, context),
            ShadowPaintRecordingCapability::Legacy(ShadowPaintBlocker::MissingPreparedSvg)
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
fn active_wrapper_accepts_inherited_clip_and_rejects_unproven_property_boundaries() {
    use crate::view::compositor::property_tree::{
        ClipNodeId, ClipNodeRole, EffectNodeId, PropertyTreeState, ScrollNodeId,
        TransformNodeId,
    };

    let (arena, owner, active_root, ..) = active_slot_svg_fixture(0x9360, ActiveSlot::Error);
    let revision = crate::view::paint::PaintContentRevision {
        self_paint_revision: 1,
        composite_revision: 1,
        topology_revision: 1,
    };
    let node = arena.get(owner).unwrap();
    let context = node
        .element
        .shadow_paint_recording_context(Default::default());
    for properties in [
        PropertyTreeState {
            transform: Some(TransformNodeId(owner)),
            ..Default::default()
        },
        PropertyTreeState {
            scroll: Some(ScrollNodeId(owner)),
            ..Default::default()
        },
        PropertyTreeState {
            effect: Some(EffectNodeId(owner)),
            ..Default::default()
        },
        PropertyTreeState {
            effect: Some(EffectNodeId(active_root)),
            ..Default::default()
        },
    ] {
        assert!(
            node.element
                .record_shadow_paint_metadata(owner, properties, revision, &arena, context,)
                .is_none()
        );
        assert!(
            node.element
                .record_shadow_paint_artifact(owner, properties, revision, &arena, context,)
                .is_none()
        );
    }

    let clip = ClipNodeId {
        owner,
        role: ClipNodeRole::ContentsClip,
    };
    let clipped_properties = PropertyTreeState {
        clip: Some(clip),
        ..Default::default()
    };
    let clipped_metadata = node
        .element
        .record_shadow_paint_metadata(owner, clipped_properties, revision, &arena, context)
        .expect("active wrapper may inherit a canonical clip property");
    let clipped_artifact = node
        .element
        .record_shadow_paint_artifact(owner, clipped_properties, revision, &arena, context)
        .expect("clipped active-wrapper artifact");
    assert_eq!(clipped_metadata.properties.clip, Some(clip));
    assert_eq!(
        clipped_artifact.chunks[0].payload_identity,
        clipped_metadata.payload_identity
    );

    let effect = EffectNodeId(owner);
    let properties = PropertyTreeState {
        effect: Some(effect),
        ..Default::default()
    };
    let root_opacity_context = crate::view::paint::PaintRecordingContext {
        opacity_authority: crate::view::paint::PaintOpacityAuthority::NeutralRootEffect(effect),
        ..context
    };
    let metadata = node
        .element
        .record_shadow_paint_metadata(owner, properties, revision, &arena, root_opacity_context)
        .expect("matching root-opacity authority");
    let artifact = node
        .element
        .record_shadow_paint_artifact(owner, properties, revision, &arena, root_opacity_context)
        .expect("matching root-opacity artifact");
    assert_eq!(
        metadata.id.role,
        crate::view::paint::PaintChunkRole::SelfDecoration
    );
    assert!(matches!(
        artifact.ops.as_slice(),
        [crate::view::paint::PaintOp::PreparedShadow(shadow), ..]
            if shadow.params.opacity.to_bits() == 1.0_f32.to_bits()
    ));
    assert!(
        artifact
            .ops
            .iter()
            .all(|op| { !matches!(op, crate::view::paint::PaintOp::PreparedSvg(_)) })
    );
}
