use super::*;

#[test]
fn component_child_context_cannot_clear_replace_or_retarget_recorder_owned_consumed_authority() {
    for attack in [
        ConsumedAuthorityAttack::Clear,
        ConsumedAuthorityAttack::Replace,
    ] {
        let mut arena = NodeArena::new();
        let parent = insert_plan(&mut arena, PlanHost::transparent(0x8f20));
        let child = insert_plan(&mut arena, PlanHost::recordable(0x8f21, &[0], &[]));
        let descendant = insert_plan(&mut arena, PlanHost::recordable(0x8f22, &[0], &[]));
        append(&mut arena, parent, child);
        append(&mut arena, child, descendant);
        arena
            .get_mut(child)
            .unwrap()
            .element
            .as_any_mut()
            .downcast_mut::<PlanHost>()
            .unwrap()
            .consumed_authority_attack = Some(attack);

        let (mut properties, generations) = identity(&arena, &[parent]);
        let transform = TransformNodeId(parent);
        properties.transforms.insert(
            transform,
            crate::view::compositor::property_tree::TransformNode {
                owner: parent,
                parent: None,
                local_matrix: glam::Mat4::IDENTITY,
                local_origin: glam::Vec3::ZERO,
                local_generation: 1,
                generation: 1,
                derived_projection: Some(
                    crate::view::compositor::property_tree::DerivedSpatialProjection {
                        owner_viewport_position: glam::Vec2::ZERO,
                        owner_viewport_transform: glam::Mat4::IDENTITY,
                    },
                ),
            },
        );
        for owner in [child, descendant] {
            let state = properties.states.get_mut(&owner).unwrap();
            state.paint.transform = Some(transform);
            state.descendants.transform = Some(transform);
        }
        let witness =
            super::super::super::ConsumedAncestorTransformWitness::new(parent, child, transform)
                .unwrap();
        let context = PaintRecordingContext {
            consumed_ancestor_property: Some(
                super::super::super::ConsumedAncestorProperty::Transform(witness),
            ),
            ..Default::default()
        };
        let record = |mode| {
            record_coverage_manifest_with_context(
                &arena,
                &[child],
                false,
                false,
                mode,
                &properties,
                &generations,
                context,
                None,
                &Default::default(),
            )
        };
        let metadata = record(CoverageRecordingMode::MetadataOnly);
        let full = record(CoverageRecordingMode::FullArtifact);
        assert!(metadata.validation_errors.is_empty());
        assert!(full.validation_errors.is_empty());
        assert!(super::super::super::frame_recorder::canonical_manifest_matches(&metadata, &full));
        for manifest in [&metadata, &full] {
            let owners = manifest
                .items
                .iter()
                .filter_map(|item| match item {
                    PaintCoverageItem::ArtifactChunk { chunk, .. } => {
                        assert_eq!(chunk.properties.transform, None);
                        Some(chunk.owner)
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(owners, vec![child, descendant]);
        }
    }
}

#[test]
fn component_hooks_cannot_clear_or_replace_recorder_owned_scroll_contents_authority() {
    for attack in [
        ConsumedAuthorityAttack::Clear,
        ConsumedAuthorityAttack::Replace,
    ] {
        let mut arena = NodeArena::new();
        let parent = insert_plan(&mut arena, PlanHost::transparent(0x8f24));
        let child = insert_plan(&mut arena, PlanHost::recordable(0x8f25, &[0], &[]));
        let descendant = insert_plan(&mut arena, PlanHost::recordable(0x8f26, &[0], &[]));
        append(&mut arena, parent, child);
        append(&mut arena, child, descendant);
        arena
            .get_mut(child)
            .unwrap()
            .element
            .as_any_mut()
            .downcast_mut::<PlanHost>()
            .unwrap()
            .consumed_authority_attack = Some(attack);

        let (mut properties, generations) = identity(&arena, &[parent]);
        let scroll = crate::view::compositor::property_tree::ScrollNodeId(parent);
        let clip = crate::view::compositor::property_tree::ClipNodeId {
            owner: parent,
            role: crate::view::compositor::property_tree::ClipNodeRole::ContentsClip,
        };
        for owner in [child, descendant] {
            let state = properties.states.get_mut(&owner).unwrap();
            state.paint.scroll = Some(scroll);
            state.paint.clip = Some(clip);
            state.descendants.scroll = Some(scroll);
            state.descendants.clip = Some(clip);
        }
        let witness = super::super::super::ConsumedAncestorScrollContentsWitness::new(
            parent, child, scroll, clip,
        )
        .unwrap();
        let context = PaintRecordingContext {
            consumed_ancestor_property: Some(
                super::super::super::ConsumedAncestorProperty::ScrollContents(witness),
            ),
            ..Default::default()
        };
        let record = |mode| {
            record_coverage_manifest_with_context(
                &arena,
                &[child],
                false,
                false,
                mode,
                &properties,
                &generations,
                context,
                None,
                &Default::default(),
            )
        };
        let metadata = record(CoverageRecordingMode::MetadataOnly);
        let full = record(CoverageRecordingMode::FullArtifact);
        assert!(metadata.validation_errors.is_empty());
        assert!(full.validation_errors.is_empty());
        assert!(super::super::super::frame_recorder::canonical_manifest_matches(&metadata, &full));
        for manifest in [&metadata, &full] {
            let chunks = manifest
                .items
                .iter()
                .filter_map(|item| match item {
                    PaintCoverageItem::ArtifactChunk { chunk, .. } => Some(chunk),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(chunks.len(), 2);
            assert!(
                chunks
                    .iter()
                    .all(|chunk| chunk.properties == Default::default())
            );
        }
    }
}

#[test]
fn component_hook_cannot_clear_required_scroll_content_paint_offset() {
    let mut arena = NodeArena::new();
    let root = insert_plan(&mut arena, PlanHost::recordable(0x8f2f, &[0], &[]));
    arena
        .get_mut(root)
        .unwrap()
        .element
        .as_any_mut()
        .downcast_mut::<PlanHost>()
        .unwrap()
        .clear_paint_offset_for_node = true;
    let (properties, generations) = identity(&arena, &[root]);
    let context = PaintRecordingContext {
        paint_offset: [3.5, 47.25],
        required_scroll_content_paint_offset_bits: Some([3.5_f32, 47.25_f32].map(f32::to_bits)),
        ..Default::default()
    };
    for mode in [
        CoverageRecordingMode::MetadataOnly,
        CoverageRecordingMode::FullArtifact,
    ] {
        let manifest = record_coverage_manifest_with_context(
            &arena,
            &[root],
            false,
            false,
            mode,
            &properties,
            &generations,
            context,
            None,
            &Default::default(),
        );
        assert!(manifest.items.iter().any(|item| matches!(
            item,
            PaintCoverageItem::LegacyBoundary {
                reason: LegacyPaintReason::MissingPaintIdentity,
                ..
            }
        )));
        assert!(
            !manifest
                .items
                .iter()
                .any(|item| matches!(item, PaintCoverageItem::ArtifactChunk { .. }))
        );
    }
}

#[test]
fn component_node_and_child_hooks_cannot_clear_recorder_owned_opacity_authority() {
    let mut arena = NodeArena::new();
    let root = insert_plan(&mut arena, PlanHost::transparent(0x8f30));
    let child = insert_plan(&mut arena, PlanHost::recordable(0x8f31, &[0], &[]));
    append(&mut arena, root, child);
    let effect = EffectNodeId(root);
    {
        let mut node = arena.get_mut(root).unwrap();
        let host = node
            .element
            .as_any_mut()
            .downcast_mut::<PlanHost>()
            .unwrap();
        host.clear_opacity_authority_for_node = true;
        host.clear_opacity_authority_for_child = true;
        host.required_opacity_authority =
            Some(super::super::super::PaintOpacityAuthority::NeutralRootEffect(effect));
    }
    arena
        .get_mut(child)
        .unwrap()
        .element
        .as_any_mut()
        .downcast_mut::<PlanHost>()
        .unwrap()
        .required_opacity_authority =
        Some(super::super::super::PaintOpacityAuthority::NeutralRootEffect(effect));

    let (properties, generations) = identity(&arena, &[root]);
    let context = PaintRecordingContext {
        opacity_authority: super::super::super::PaintOpacityAuthority::NeutralRootEffect(effect),
        ..Default::default()
    };
    let record = |mode| {
        record_coverage_manifest_with_context(
            &arena,
            &[root],
            false,
            false,
            mode,
            &properties,
            &generations,
            context,
            None,
            &Default::default(),
        )
    };
    let metadata = record(CoverageRecordingMode::MetadataOnly);
    let full = record(CoverageRecordingMode::FullArtifact);
    assert!(metadata.validation_errors.is_empty());
    assert!(full.validation_errors.is_empty());
    assert!(super::super::super::frame_recorder::canonical_manifest_matches(&metadata, &full));
    for manifest in [&metadata, &full] {
        assert!(matches!(
            manifest.items.as_slice(),
            [
                PaintCoverageItem::TransparentNode { owner, .. },
                PaintCoverageItem::ArtifactChunk { chunk, .. },
            ] if *owner == root && chunk.owner == child
        ));
    }
}
