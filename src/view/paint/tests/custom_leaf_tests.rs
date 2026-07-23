use super::*;

#[test]
fn custom_legacy_subtree_builds_exactly_once_and_recording_does_not_touch_deferred() {
    let builds = Arc::new(AtomicUsize::new(0));
    let mut arena = NodeArena::new();
    let root = arena.insert(Node::new(Box::new(RecordingHost {
        id: 40,
        builds: builds.clone(),
        fill: None,
    })));
    arena.push_root(root);
    let (properties, generations) = sync_identity(&arena, &[root]);

    let mut ctx = UiBuildContext::new(100, 100, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    ctx.register_deferred(root, 40);
    let outcome = record_root(&arena, root, &properties, &generations);
    assert_eq!(ctx.next_deferred().map(|node| node.key), Some(root));
    let PaintRecordOutcome::LegacySubtree(legacy) = outcome else {
        panic!("custom host should remain legacy");
    };
    assert_eq!(legacy.reason, LegacyPaintReason::UnknownHost);

    let mut graph = FrameGraph::new();
    let _ =
        arena.with_element_taken(root, |element, arena| element.build(&mut graph, arena, ctx));
    assert_eq!(builds.load(Ordering::Relaxed), 1);
}

#[test]
fn custom_leaf_typed_adapter_records_canonical_fill_and_compiles() {
    let host = CustomLeafPaintHost::fill(0x8f10);
    let expected = host.bounds;
    let (arena, root, properties, generations) = custom_leaf_fixture(host);
    let _ = take_full_artifact_record_count();
    let (artifact, eligibility) =
        whole_frame_artifact(&arena, &[root], &properties, &generations);
    assert!(eligibility.eligible);
    assert_eq!(take_full_artifact_record_count(), 1);
    assert_eq!(artifact.chunks.len(), 1);
    assert_eq!(artifact.chunks[0].owner, root);
    assert_eq!(artifact.chunks[0].id.scope, PaintPropertyScope::SelfPaint);
    assert_eq!(artifact.chunks[0].id.phase, PaintNodePhase::BeforeChildren);
    assert_eq!(artifact.chunks[0].id.slot, 0);
    assert_eq!(artifact.chunks[0].id.role, PaintChunkRole::SelfDecoration);
    assert!(matches!(
        artifact.ops.as_slice(),
        [PaintOp::DrawRect(rect)]
            if rect.mode == crate::view::render_pass::draw_rect_pass::RectRenderMode::FillOnly
    ));
    let graph = compiled_whole_frame_graph(&artifact);
    let rects = graph.test_rect_pass_snapshots();
    let [rect] = rects.as_slice() else {
        panic!("custom fill must compile to exactly one rect pass")
    };
    assert_eq!(
        rect.position_bits,
        [expected.x, expected.y].map(f32::to_bits)
    );
    assert_eq!(
        rect.size_bits,
        [expected.width, expected.height].map(f32::to_bits)
    );
    assert_eq!(rect.opacity_bits, 1.0_f32.to_bits());
    assert_eq!(rect.fill_color_bits[3], 0.75_f32.to_bits());
}

#[test]
fn custom_leaf_cannot_forge_scroll_placement_normalization_capability() {
    let host = CustomLeafPaintHost::fill(0x8f2f);
    assert!(host.retained_scroll_normalized_paint_capability().is_none());
}

#[test]
fn custom_leaf_invalid_bounds_opacity_or_cardinality_stays_unknown_without_full_record() {
    let invalid_hosts = [
        {
            let mut host = CustomLeafPaintHost::fill(0x8f11);
            host.mode = CustomLeafRecordMode::InvalidBounds;
            host
        },
        {
            let mut host = CustomLeafPaintHost::fill(0x8f12);
            host.mode = CustomLeafRecordMode::Fill {
                rgba: [0.1, 0.2, 0.3, 1.0],
                opacity: f32::NAN,
            };
            host
        },
        {
            let mut host = CustomLeafPaintHost::fill(0x8f13);
            host.mode = CustomLeafRecordMode::Fill {
                rgba: [0.1, 0.2, 0.3, 1.0],
                opacity: 1.01,
            };
            host
        },
        {
            let mut host = CustomLeafPaintHost::fill(0x8f14);
            host.mode = CustomLeafRecordMode::DoubleFill;
            host
        },
    ];
    for host in invalid_hosts {
        let (arena, root, properties, generations) = custom_leaf_fixture(host);
        let _ = take_full_artifact_record_count();
        let FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility) =
            record_frame_artifact(
                &arena,
                &[root],
                &properties,
                &generations,
                RendererMode::Auto,
            )
            .unwrap()
        else {
            panic!("invalid public command must fail closed")
        };
        assert!(
            eligibility
                .reasons
                .contains(&FrameArtifactFallbackReason::LegacyBoundary(
                    LegacyPaintReason::UnknownHost
                ))
        );
        assert_eq!(
            eligibility.debug_boundaries,
            vec![FrameArtifactDebugBoundary {
                owner: root,
                kind: FrameArtifactDebugBoundaryKind::Legacy(LegacyPaintReason::UnknownHost,),
            }],
            "RetainedAuto diagnostics must preserve the exact unsupported custom host",
        );
        assert_eq!(take_full_artifact_record_count(), 0);
    }
}

#[test]
fn custom_leaf_structural_and_property_boundaries_fail_closed_before_full_record() {
    let (mut child_arena, child_root, _, _) =
        custom_leaf_fixture(CustomLeafPaintHost::fill(0x8f20));
    let _ = commit_child(
        &mut child_arena,
        child_root,
        Box::new(leaf_element(0x8f21, Color::rgb(1, 2, 3), 1.0, false)),
    );
    let (child_properties, child_generations) = sync_identity(&child_arena, &[child_root]);
    let _ = take_full_artifact_record_count();
    assert!(matches!(
        record_frame_artifact(
            &child_arena,
            &[child_root],
            &child_properties,
            &child_generations,
            RendererMode::Auto,
        ),
        Ok(FrameArtifactRecordOutcome::WholeFrameLegacyFallback(_))
    ));
    assert_eq!(take_full_artifact_record_count(), 0);

    let mut arena_only_host = CustomLeafPaintHost::fill(0x8f22);
    arena_only_host.expose_children = false;
    let (mut arena_only, arena_only_root, _, _) = custom_leaf_fixture(arena_only_host);
    let _ = commit_child(
        &mut arena_only,
        arena_only_root,
        Box::new(leaf_element(0x8f23, Color::rgb(4, 5, 6), 1.0, false)),
    );
    assert!(
        arena_only.get(arena_only_root).is_some_and(
            |node| !node.children().is_empty() && node.element.children().is_empty()
        )
    );
    let (arena_only_properties, arena_only_generations) =
        sync_identity(&arena_only, &[arena_only_root]);
    let _ = take_full_artifact_record_count();
    assert_eq!(
        arena_only
            .get(arena_only_root)
            .unwrap()
            .element
            .shadow_paint_recording_capability(
                &arena_only,
                false,
                PaintRecordingContext::default()
            ),
        ShadowPaintRecordingCapability::Recordable
    );
    let FrameArtifactRecordOutcome::WholeFrameLegacyFallback(arena_only_legacy) =
        record_frame_artifact(
            &arena_only,
            &[arena_only_root],
            &arena_only_properties,
            &arena_only_generations,
            RendererMode::Auto,
        )
        .unwrap()
    else {
        panic!("arena child must block a trait-opaque custom leaf")
    };
    assert!(
        arena_only_legacy
            .reasons
            .contains(&FrameArtifactFallbackReason::LegacyBoundary(
                LegacyPaintReason::MissingPaintIdentity
            ))
    );
    assert_eq!(take_full_artifact_record_count(), 0);

    let (property_arena, property_root, mut property_state, property_generations) =
        custom_leaf_fixture(CustomLeafPaintHost::fill(0x8f24));
    property_state
        .states
        .get_mut(&property_root)
        .unwrap()
        .paint
        .transform = Some(TransformNodeId(property_root));
    let _ = take_full_artifact_record_count();
    assert!(matches!(
        record_frame_artifact(
            &property_arena,
            &[property_root],
            &property_state,
            &property_generations,
            RendererMode::Auto,
        ),
        Ok(FrameArtifactRecordOutcome::WholeFrameLegacyFallback(_))
    ));
    assert_eq!(take_full_artifact_record_count(), 0);
}

#[test]
fn custom_leaf_deferred_animating_and_root_opacity_stay_legacy() {
    for (id, configure) in [
        (
            0x8f30,
            (
                true,
                false,
                crate::view::base_component::RetainedPaintProperties::default(),
            ),
        ),
        (
            0x8f31,
            (
                false,
                true,
                crate::view::base_component::RetainedPaintProperties::default(),
            ),
        ),
    ] {
        let mut host = CustomLeafPaintHost::fill(id);
        host.deferred = configure.0;
        host.active_animator = configure.1;
        host.retained_properties = configure.2;
        let (arena, root, properties, generations) = custom_leaf_fixture(host);
        let _ = take_full_artifact_record_count();
        assert!(matches!(
            record_frame_artifact(
                &arena,
                &[root],
                &properties,
                &generations,
                RendererMode::Auto,
            ),
            Ok(FrameArtifactRecordOutcome::WholeFrameLegacyFallback(_))
        ));
        assert_eq!(take_full_artifact_record_count(), 0);
    }

    let mut opacity_host = CustomLeafPaintHost::fill(0x8f33);
    opacity_host.retained_properties.opacity = 0.5;
    let (opacity_arena, opacity_root, opacity_properties, opacity_generations) =
        custom_leaf_fixture(opacity_host);
    let opacity_context = PaintRecordingContext {
        opacity_authority: PaintOpacityAuthority::NeutralRootEffect(EffectNodeId(opacity_root)),
        ..Default::default()
    };
    assert_eq!(
        opacity_arena
            .get(opacity_root)
            .unwrap()
            .element
            .shadow_paint_recording_capability(&opacity_arena, false, opacity_context),
        ShadowPaintRecordingCapability::Unsupported
    );
    let _ = take_full_artifact_record_count();
    assert!(
        record_root_group_opacity_frame_artifact(
            &opacity_arena,
            &[opacity_root],
            &opacity_properties,
            &opacity_generations,
            RendererMode::ForcedForTests,
        )
        .is_err()
    );
    assert_eq!(take_full_artifact_record_count(), 0);
}

#[test]
fn custom_leaf_owner_pointer_mismatch_rejects_duplicate_stable_id() {
    let mut arena = new_test_arena();
    let first = commit_element(&mut arena, Box::new(CustomLeafPaintHost::fill(0x8f40)));
    let second = commit_element(&mut arena, Box::new(CustomLeafPaintHost::fill(0x8f40)));
    let first_node = arena.get(first).unwrap();
    assert!(
        first_node
            .element
            .record_shadow_paint_metadata(
                second,
                PropertyTreeState::default(),
                PaintContentRevision {
                    self_paint_revision: 1,
                    composite_revision: 1,
                    topology_revision: 1,
                },
                &arena,
                PaintRecordingContext::default(),
            )
            .is_none()
    );
}

#[test]
fn custom_leaf_metadata_full_drift_forces_whole_frame_fallback() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut host = CustomLeafPaintHost::fill(0x8f50);
    host.mode = CustomLeafRecordMode::Drift {
        calls: calls.clone(),
    };
    let (arena, root, properties, generations) = custom_leaf_fixture(host);
    let _ = take_full_artifact_record_count();
    let outcome = record_frame_artifact(
        &arena,
        &[root],
        &properties,
        &generations,
        RendererMode::Auto,
    )
    .unwrap();
    assert!(matches!(
        outcome,
        FrameArtifactRecordOutcome::WholeFrameLegacyFallback(_)
    ));
    assert_eq!(calls.load(Ordering::Relaxed), 4);
    assert_eq!(take_full_artifact_record_count(), 1);
}
