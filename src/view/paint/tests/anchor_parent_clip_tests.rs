use super::*;

#[test]
fn anchor_parent_leaf_self_clip_replaces_then_restores_ancestor_scissor_strictly() {
    for (format, scale_factor) in [
        (wgpu::TextureFormat::Bgra8Unorm, 1.0),
        (wgpu::TextureFormat::Rgba8Unorm, 2.0),
    ] {
        for (opacity, border) in [(1.0, false), (1.0, true), (0.55, false), (0.55, true)] {
            let config = PaintParityConfig {
                format,
                scale_factor,
                initial_scissor: Some([4, 6, 24, 18]),
                ..PaintParityConfig::default()
            };
            let snapshots = assert_whole_frame_structural_parity(
                || anchor_parent_self_clip_roots(opacity, border),
                config,
            );
            let clipped_op_count = if border { 2 } else { 1 };
            assert_eq!(snapshots.len(), clipped_op_count + 1);
            for snapshot in &snapshots[..clipped_op_count] {
                assert_eq!(snapshot.effective_scissor_rect, Some([0, 0, 320, 240]));
            }
            assert_eq!(
                snapshots[clipped_op_count].effective_scissor_rect,
                Some([4, 6, 24, 18]),
                "the following root must observe the restored ancestor scissor"
            );
            assert_eq!(snapshots[0].opaque, opacity == 1.0);
        }
    }
}

#[test]
fn exact_single_owner_self_clip_keeps_outer_shadow_outside_owner_clip() {
    let (arena, roots) = anchor_parent_self_clip_shadow_root();
    let (properties, generations) = sync_identity(&arena, &roots);
    let (artifact, eligibility) =
        whole_frame_artifact(&arena, &roots, &properties, &generations);
    assert!(eligibility.eligible);
    assert_eq!(artifact.chunks.len(), 1);
    assert!(matches!(
        artifact.ops.first(),
        Some(PaintOp::PreparedShadow(_))
    ));
    assert!(matches!(
        artifact.chunks[0].payload_identity,
        PaintPayloadIdentity::PreparedShadows(_, _)
    ));

    let incoming = [4, 6, 24, 18];
    let mut graph = compiled_whole_frame_graph_with_config(
        &artifact,
        PaintParityConfig {
            initial_scissor: Some(incoming),
            ..PaintParityConfig::default()
        },
    );
    let snapshot = graph.test_compile_snapshot().unwrap();
    let shadow_composite = snapshot
        .pass_payloads()
        .iter()
        .find_map(|payload| match payload {
            FramePassTestPayload::TextureComposite(composite)
                if composite.sampled_source.is_none() =>
            {
                Some(composite)
            }
            _ => None,
        })
        .expect("outer shadow must composite before decoration");
    assert_eq!(shadow_composite.pass_context.scissor_rect, Some(incoming));
    let rects = snapshot
        .pass_payloads()
        .iter()
        .filter_map(|payload| match payload {
            FramePassTestPayload::DrawRect(rect) => Some(rect),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(rects.len(), 2);
    assert!(
        rects
            .iter()
            .all(|rect| rect.effective_scissor_rect == Some([0, 0, 320, 240]))
    );

    let mut fragmented = artifact.clone();
    fragmented.owner_nodes[0].parent = Some(roots[0]);
    assert_compiler_rejects_before_emit(&fragmented, "fragmented self-clip shadow owner");
}

#[test]
fn nested_anchor_parent_requires_legacy_order_and_matches_strictly_when_partitioned() {
    for anchor_first in [true, false] {
        let (arena, roots, anchor) = nested_anchor_parent_mixed_siblings(anchor_first);
        let (properties, generations) = sync_identity(&arena, &roots);
        let clip = properties
            .paint_state_for(anchor)
            .and_then(|state| state.clip);

        if !anchor_first {
            assert_eq!(
                clip,
                Some(ClipNodeId {
                    owner: anchor,
                    role: ClipNodeRole::SelfClip,
                })
            );
            let snapshots = assert_whole_frame_structural_parity(
                || {
                    let (arena, roots, _) = nested_anchor_parent_mixed_siblings(false);
                    (arena, roots)
                },
                PaintParityConfig {
                    initial_scissor: Some([4, 6, 24, 18]),
                    ..PaintParityConfig::default()
                },
            );
            let visible = snapshots
                .iter()
                .filter(|snapshot| f32::from_bits(snapshot.fill_color_bits[3]) > 0.0)
                .collect::<Vec<_>>();
            assert_eq!(visible.len(), 2);
            assert_eq!(visible[0].opaque_depth_order, Some(0));
            assert_eq!(visible[1].opaque_depth_order, Some(1));
            assert_eq!(visible[1].effective_scissor_rect, Some([0, 0, 320, 240]));

            let (production_arena, production_roots, _) =
                nested_anchor_parent_mixed_siblings(false);
            let (production_properties, production_generations) =
                sync_identity(&production_arena, &production_roots);
            take_full_artifact_record_count();
            take_artifact_compile_count();
            let FrameArtifactRecordOutcome::Artifact {
                artifact,
                eligibility,
            } = record_clip_enabled_frame_artifact(
                &production_arena,
                &production_roots,
                &production_properties,
                &production_generations,
                RendererMode::Auto,
            )
            .unwrap()
            else {
                panic!("ordered nested AnchorParent must enter production clip authority")
            };
            assert!(eligibility.eligible);
            assert_eq!(take_full_artifact_record_count(), 3);
            let _ = compiled_whole_frame_graph(&artifact);
            assert_eq!(take_artifact_compile_count(), 1);
            continue;
        }

        assert_eq!(clip, None);

        take_full_artifact_record_count();
        let FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility) =
            record_frame_artifact(
                &arena,
                &roots,
                &properties,
                &generations,
                RendererMode::Auto,
            )
            .expect("misordered nested AnchorParent must fail closed to legacy")
        else {
            panic!("nested AnchorParent must not produce an artifact")
        };
        assert!(
            eligibility
                .reasons
                .contains(&FrameArtifactFallbackReason::LegacyBoundary(
                    LegacyPaintReason::SelfClip
                ))
        );
        assert_eq!(
            take_full_artifact_record_count(),
            0,
            "metadata rejection must happen before any full artifact hook"
        );

        let legacy = legacy_roots_graph(arena, &roots).test_rect_pass_snapshots();
        let visible = legacy
            .iter()
            .filter(|snapshot| f32::from_bits(snapshot.fill_color_bits[3]) > 0.0)
            .collect::<Vec<_>>();
        assert_eq!(visible.len(), 2);
        assert!(
            f32::from_bits(visible[0].fill_color_bits[2])
                > f32::from_bits(visible[0].fill_color_bits[0]),
            "normal blue sibling paints before the overflow AnchorParent child"
        );
        assert!(
            f32::from_bits(visible[1].fill_color_bits[0])
                > f32::from_bits(visible[1].fill_color_bits[2]),
            "overflow AnchorParent child paints in the legacy late phase"
        );
        assert_eq!(visible[0].opaque_depth_order, Some(0));
        assert_eq!(visible[1].opaque_depth_order, Some(1));
    }
}

#[test]
fn nested_and_multiple_deferred_viewport_roots_record_once_in_late_dfs_order() {
    let (arena, roots, normal, first, nested_child, second) = nested_deferred_viewport_popups();
    let root = roots[0];
    let (properties, generations) = sync_identity(&arena, &roots);
    for deferred in [first, second] {
        assert_eq!(
            properties
                .paint_state_for(deferred)
                .and_then(|state| state.clip),
            Some(ClipNodeId {
                owner: deferred,
                role: ClipNodeRole::SelfClip,
            })
        );
    }
    assert_eq!(
        properties
            .paint_state_for(nested_child)
            .and_then(|state| state.clip),
        Some(ClipNodeId {
            owner: first,
            role: ClipNodeRole::SelfClip,
        }),
        "ordinary popup descendants inherit the deferred root Replace clip"
    );

    let outcome = record_clip_enabled_frame_artifact(
        &arena,
        &roots,
        &properties,
        &generations,
        RendererMode::Auto,
    )
    .expect("nested deferred native popup tree must pass metadata preflight");
    let FrameArtifactRecordOutcome::Artifact {
        artifact,
        eligibility,
    } = outcome
    else {
        panic!("valid nested deferred tree must stay on retained authority: {outcome:?}")
    };
    assert!(eligibility.eligible, "{eligibility:?}");
    assert_eq!(
        artifact
            .chunks
            .iter()
            .map(|chunk| chunk.owner)
            .collect::<Vec<_>>(),
        vec![root, normal, first, nested_child, second],
        "main phase precedes deferred roots; each deferred subtree remains DFS-ordered"
    );
    let _ = compiled_whole_frame_graph(&artifact);

    let mut tampered = properties;
    tampered
        .clips
        .get_mut(&ClipNodeId {
            owner: first,
            role: ClipNodeRole::SelfClip,
        })
        .expect("first popup clip")
        .behavior = ClipBehavior::Intersect;
    take_full_artifact_record_count();
    let FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility) =
        record_clip_enabled_frame_artifact(
            &arena,
            &roots,
            &tampered,
            &generations,
            RendererMode::Auto,
        )
        .expect("tamper is a fail-closed selection, not a recorder error")
    else {
        panic!("non-Replace deferred clip must never compile as retained")
    };
    assert!(eligibility.reasons.iter().any(|reason| matches!(
        reason,
        FrameArtifactFallbackReason::LegacyBoundary(LegacyPaintReason::Deferred)
            | FrameArtifactFallbackReason::Validation(_)
    )));
    assert_eq!(
        take_full_artifact_record_count(),
        0,
        "deferred clip tamper must fail during metadata-only preflight"
    );
}

#[test]
fn anchor_parent_ordering_classifies_mixed_element_image_and_svg_via_trait_witness() {
    let (arena, roots, anchors) = mixed_native_anchor_parent_siblings(false);
    let (properties, _) = sync_identity(&arena, &roots);
    for anchor in &anchors {
        assert_eq!(
            properties
                .paint_state_for(*anchor)
                .and_then(|state| state.clip),
            Some(ClipNodeId {
                owner: *anchor,
                role: ClipNodeRole::SelfClip,
            }),
            "normal mixed-native siblings precede the overflow phase"
        );
    }

    let (arena, roots, anchors) = mixed_native_anchor_parent_siblings(true);
    let (properties, _) = sync_identity(&arena, &roots);
    for anchor in anchors {
        assert_eq!(
            properties
                .paint_state_for(anchor)
                .and_then(|state| state.clip),
            None,
            "a normal native sibling after overflow must invalidate every exact witness"
        );
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn deferred_image_and_svg_descendants_use_the_same_late_phase_trait_contract() {
    const SVG: &str = "<svg xmlns='http://www.w3.org/2000/svg' width='12' height='10'><rect width='12' height='10' fill='#22c55e'/></svg>";
    let mut arena = new_test_arena();
    let mut root = Element::new_with_id(0x8d40, 0.0, 0.0, 320.0, 240.0);
    let mut root_style = Style::new();
    root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    root_style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::rgb(20, 40, 80)),
    );
    root.apply_style(root_style);
    let root = commit_element(&mut arena, Box::new(root));
    let media_style = || {
        let mut style = Style::new();
        style.insert(PropertyId::Width, ParsedValue::Length(Length::px(12.0)));
        style.insert(PropertyId::Height, ParsedValue::Length(Length::px(10.0)));
        style.insert(
            PropertyId::Position,
            ParsedValue::Position(
                Position::absolute()
                    .left(Length::px(18.0))
                    .top(Length::px(16.0))
                    .clip(ClipMode::Viewport),
            ),
        );
        style
    };
    let mut image = Image::new_with_id(
        0x8d41,
        ImageSource::Rgba {
            width: 1,
            height: 1,
            pixels: Arc::from([255, 255, 255, 255]),
        },
    );
    image.apply_style(media_style());
    let image = commit_child(&mut arena, root, Box::new(image));
    let mut svg = Svg::new_with_id(0x8d42, SvgSource::Content(SVG.into()));
    svg.apply_style(media_style());
    let svg = commit_child(&mut arena, root, Box::new(svg));
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    arena
        .get_mut(svg)
        .expect("svg")
        .element
        .as_any_mut()
        .downcast_mut::<Svg>()
        .expect("Svg host")
        .prepare_content_paint_for_test(SVG, (12.0, 10.0), 1.0)
        .expect("prepare deferred SVG paint");

    let roots = [root];
    let (properties, generations) = sync_identity(&arena, &roots);
    let outcome = record_clip_enabled_frame_artifact(
        &arena,
        &roots,
        &properties,
        &generations,
        RendererMode::Auto,
    )
    .expect("deferred native media metadata preflight");
    let FrameArtifactRecordOutcome::Artifact {
        artifact,
        eligibility,
    } = outcome
    else {
        panic!("deferred native media must remain on retained authority: {outcome:?}")
    };
    assert!(eligibility.eligible, "{eligibility:?}");
    assert_eq!(
        artifact
            .chunks
            .iter()
            .map(|chunk| chunk.owner)
            .collect::<Vec<_>>(),
        vec![root, image, svg]
    );
    assert!(matches!(artifact.ops[1], PaintOp::PreparedImage(_)));
    assert!(matches!(artifact.ops[2], PaintOp::PreparedSvg(_)));
    let _ = compiled_whole_frame_graph(&artifact);
}

#[test]
fn nested_self_clip_metadata_and_full_hooks_require_owner_bound_witness() {
    let (arena, roots, anchor) = nested_anchor_parent_mixed_siblings(false);
    let normal = arena
        .children_of(roots[0])
        .into_iter()
        .find(|child| *child != anchor)
        .unwrap();
    let (properties, generations) = sync_identity(&arena, &roots);
    let state = properties.paint_state_for(anchor).unwrap();
    let generation = generations.local_generations_for(anchor).unwrap();
    let revision = PaintContentRevision {
        self_paint_revision: generation.self_paint_revision,
        composite_revision: generation.composite_revision,
        topology_revision: generation.topology_revision,
    };
    let node = arena.get(anchor).unwrap();

    assert!(
        node.element
            .record_shadow_paint_metadata(
                anchor,
                state,
                revision,
                &arena,
                PaintRecordingContext::default(),
            )
            .is_none()
    );
    assert!(
        node.element
            .record_shadow_paint_artifact(
                anchor,
                state,
                revision,
                &arena,
                PaintRecordingContext::default(),
            )
            .is_none()
    );

    let clip = state.clip.unwrap();
    let normal_stable_id = arena.get(normal).unwrap().element.stable_id();
    let leaked = PaintRecordingContext {
        recording_owner: Some(normal),
        recording_owner_stable_id: Some(normal_stable_id),
        authoritative_self_clip: Some(clip),
        ..Default::default()
    };
    assert!(!leaked.authorizes_self_clip_for(node.element.stable_id()));
    assert_eq!(
        node.element
            .shadow_paint_recording_capability(&arena, false, leaked),
        ShadowPaintRecordingCapability::Legacy(ShadowPaintBlocker::SelfClip)
    );
}

#[test]
fn nested_anchor_parent_with_viewport_sibling_fails_before_full_recording() {
    let (arena, roots, anchor) = nested_anchor_parent_mixed_siblings(false);
    let normal = arena
        .children_of(roots[0])
        .into_iter()
        .find(|child| *child != anchor)
        .unwrap();
    let mut style = Style::new();
    style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .left(Length::px(30.0))
                .top(Length::px(24.0))
                .clip(ClipMode::Viewport),
        ),
    );
    arena
        .get_mut(normal)
        .unwrap()
        .element
        .as_any_mut()
        .downcast_mut::<Element>()
        .unwrap()
        .apply_style(style);

    let (properties, generations) = sync_identity(&arena, &roots);
    assert_eq!(
        properties
            .paint_state_for(anchor)
            .and_then(|state| state.clip),
        None
    );
    take_full_artifact_record_count();
    let outcome = record_clip_enabled_frame_artifact(
        &arena,
        &roots,
        &properties,
        &generations,
        RendererMode::Auto,
    )
    .unwrap();
    assert!(matches!(
        outcome,
        FrameArtifactRecordOutcome::WholeFrameLegacyFallback(_)
    ));
    assert_eq!(take_full_artifact_record_count(), 0);

    let (mut arena, roots, anchor) = nested_anchor_parent_mixed_siblings(false);
    let normal = arena
        .children_of(roots[0])
        .into_iter()
        .find(|child| *child != anchor)
        .unwrap();
    let mut deferred = CustomLeafPaintHost::fill(0x8d90);
    deferred.deferred = true;
    let deferred = arena.insert(Node::with_parent(Box::new(deferred), Some(roots[0])));
    arena.set_parent(normal, None);
    arena.set_children(roots[0], vec![deferred, anchor]);
    let (properties, generations) = sync_identity(&arena, &roots);
    assert_eq!(
        properties
            .paint_state_for(anchor)
            .and_then(|state| state.clip),
        None,
        "a non-Element deferred sibling must invalidate the exact ordering witness"
    );
    take_full_artifact_record_count();
    let outcome = record_clip_enabled_frame_artifact(
        &arena,
        &roots,
        &properties,
        &generations,
        RendererMode::Auto,
    )
    .unwrap();
    assert!(matches!(
        outcome,
        FrameArtifactRecordOutcome::WholeFrameLegacyFallback(_)
    ));
    assert_eq!(take_full_artifact_record_count(), 0);
}
