use super::*;

#[test]
fn wrapping_inline_span_owns_typed_decoration_before_text_and_matches_legacy() {
    let (arena, roots, span_key, text_key, fragment_count) =
        prepared_wrapping_inline_span_tree();
    let (properties, generations) = sync_identity(&arena, &roots);
    take_full_artifact_record_count();
    let (artifact, eligibility) =
        whole_frame_artifact(&arena, &roots, &properties, &generations);
    assert!(eligibility.eligible);
    assert_eq!(artifact.chunks.len(), 2);
    assert_eq!(artifact.chunks[0].owner, span_key);
    assert_eq!(artifact.chunks[0].id.role, PaintChunkRole::SelfDecoration);
    assert_eq!(artifact.chunks[1].owner, text_key);
    assert_eq!(artifact.chunks[1].id.role, PaintChunkRole::TextGlyphs);
    assert!(matches!(
        artifact.chunks[0].payload_identity,
        PaintPayloadIdentity::InlineIfcDecorations(_, _)
    ));
    assert_eq!(artifact.chunks[0].op_range.len(), fragment_count);
    assert!(
        artifact.ops[artifact.chunks[0].op_range.clone()]
            .iter()
            .all(|op| matches!(op, PaintOp::PreparedInlineIfcDecoration(_)))
    );
    assert_eq!(take_full_artifact_record_count(), 2);

    let artifact_rects = compiled_whole_frame_graph(&artifact).test_rect_pass_snapshots();
    let (legacy_arena, legacy_roots, _, _, _) = prepared_wrapping_inline_span_tree();
    let legacy_rects =
        legacy_roots_graph(legacy_arena, &legacy_roots).test_rect_pass_snapshots();
    assert_eq!(artifact_rects, legacy_rects);
}

#[test]
fn wrapping_inline_span_shadows_preserve_fragment_order_and_match_legacy() {
    let shadows = || {
        vec![
            BoxShadow::new()
                .color(Color::rgb(30, 100, 220))
                .offset_x(2.25)
                .offset_y(-1.5)
                .blur(6.5)
                .spread(1.25),
            BoxShadow::new()
                .color(Color::rgb(190, 40, 100))
                .offset_x(-1.0)
                .offset_y(2.75)
                .blur(4.25)
                .spread(1.25)
                .inset(true),
        ]
    };
    let (arena, roots, span_key, text_key, fragment_count) =
        prepared_wrapping_inline_span_tree_with_opacity_and_shadows(1.0, shadows());
    let (fragments, shadow_recording_offset) = {
        let span_node = arena.get(span_key).unwrap();
        let span = span_node
            .element
            .as_any()
            .downcast_ref::<Element>()
            .unwrap();
        (
            span.inline_fragment_rects().to_vec(),
            span.shadow_paint_recording_context(PaintRecordingContext::default())
                .paint_offset,
        )
    };
    assert!(
        fragment_count >= 2,
        "fixture must wrap across multiple lines"
    );
    let (properties, generations) = sync_identity(&arena, &roots);
    let manifest = |mode| {
        record_coverage_manifest(&arena, &roots, false, true, mode, &properties, &generations)
    };
    let metadata = manifest(CoverageRecordingMode::MetadataOnly);
    let full = manifest(CoverageRecordingMode::FullArtifact);
    assert!(metadata.validation_errors.is_empty());
    assert!(full.validation_errors.is_empty());
    assert!(canonical_manifest_matches_for_test(&metadata, &full));
    let span_identity = metadata
        .items
        .iter()
        .find_map(|item| match item {
            PaintCoverageItem::ArtifactChunk { chunk, .. } if chunk.owner == span_key => {
                Some(&chunk.payload_identity)
            }
            _ => None,
        })
        .expect("metadata must contain the inline span chunk");
    assert!(matches!(
        span_identity,
        PaintPayloadIdentity::InlineIfcDecorations(shadow_ids, decoration_ids)
            if shadow_ids.len() == fragment_count * 2
                && decoration_ids.len() == fragment_count
    ));

    let (artifact, eligibility) =
        whole_frame_artifact(&arena, &roots, &properties, &generations);
    assert!(eligibility.eligible);
    assert_eq!(artifact.chunks[0].owner, span_key);
    assert_eq!(artifact.chunks[1].owner, text_key);
    let span_ops = &artifact.ops[artifact.chunks[0].op_range.clone()];
    let shadow_count = fragment_count * 2;
    assert!(
        span_ops[..shadow_count]
            .iter()
            .all(|op| matches!(op, PaintOp::PreparedShadow(_)))
    );
    assert!(
        span_ops[shadow_count..]
            .iter()
            .all(|op| matches!(op, PaintOp::PreparedInlineIfcDecoration(_)))
    );
    for (fragment, pair) in fragments
        .iter()
        .zip(span_ops[..shadow_count].chunks_exact(2))
    {
        for op in pair {
            let PaintOp::PreparedShadow(shadow) = op else {
                unreachable!()
            };
            let min_x = shadow
                .mesh
                .vertices
                .iter()
                .map(|vertex| vertex[0])
                .fold(f32::INFINITY, f32::min);
            let min_y = shadow
                .mesh
                .vertices
                .iter()
                .map(|vertex| vertex[1])
                .fold(f32::INFINITY, f32::min);
            assert!(
                (min_x - (fragment.x - 1.25 + shadow_recording_offset[0])).abs() < 0.001,
                "shadow min_x={min_x}, expected={}, fragment={fragment:?}",
                fragment.x - 1.25 + shadow_recording_offset[0]
            );
            assert!(
                (min_y - (fragment.y - 1.25 + shadow_recording_offset[1])).abs() < 0.001,
                "shadow min_y={min_y}, expected={}, fragment={fragment:?}",
                fragment.y - 1.25 + shadow_recording_offset[1]
            );
        }
        let PaintOp::PreparedShadow(outer) = &pair[0] else {
            unreachable!()
        };
        let PaintOp::PreparedShadow(inset) = &pair[1] else {
            unreachable!()
        };
        assert!(!outer.params.clip_to_geometry);
        assert!(inset.params.clip_to_geometry);
    }
    drop(arena);

    let artifact_graph = compiled_whole_frame_graph(&artifact);
    let (legacy_arena, legacy_roots, ..) =
        prepared_wrapping_inline_span_tree_with_opacity_and_shadows(1.0, shadows());
    let legacy_graph = legacy_roots_graph(legacy_arena, &legacy_roots);
    assert_eq!(
        artifact_graph.pass_descriptors(),
        legacy_graph.pass_descriptors()
    );
    assert_eq!(
        artifact_graph
            .test_graphics_passes::<crate::view::render_pass::shadow_module::ShadowFillPass>()
            .len(),
        fragment_count * 3,
        "each fragment emits one outer fill plus inset shadow and mask fills"
    );
    assert_eq!(
        artifact_graph
            .pass_descriptors()
            .iter()
            .filter(|pass| pass.name.ends_with("blur_module::BlurStagePass"))
            .count(),
        fragment_count * 4,
        "each fragment has two independently blurred shadows"
    );
}

#[test]
fn sampled_inline_span_layout_transition_keeps_metadata_full_and_legacy_parity() {
    fn sample_transition(arena: &NodeArena, span_key: NodeKey) {
        let mut node = arena.get_mut(span_key).unwrap();
        let span = node.element.as_any_mut().downcast_mut::<Element>().unwrap();
        let package_before = span
            .inline_ifc_decoration_package_for_test()
            .expect("layout must install the inline decoration package")
            .clone();
        span.set_layout_transition_width(71.0);
        span.set_layout_transition_height(39.0);
        assert_eq!(
            span.inline_ifc_decoration_package_for_test()
                .expect("sampling must preserve the installed paint package"),
            &package_before
        );
        span.clear_local_dirty_flags(DirtyFlags::ALL);
    }

    let (arena, roots, span_key, _, _) = prepared_wrapping_inline_span_tree();
    sample_transition(&arena, span_key);
    arena.clear_arena_dirty_subtree(span_key, DirtyFlags::ALL);
    arena.refresh_subtree_dirty_cache(span_key);
    let (properties, generations) = sync_identity(&arena, &roots);
    let manifest = |mode| {
        record_coverage_manifest(&arena, &roots, false, true, mode, &properties, &generations)
    };
    let metadata = manifest(CoverageRecordingMode::MetadataOnly);
    let full = manifest(CoverageRecordingMode::FullArtifact);
    assert!(metadata.validation_errors.is_empty());
    assert!(full.validation_errors.is_empty());
    assert!(
        metadata
            .items
            .iter()
            .all(|item| !matches!(item, PaintCoverageItem::LegacyBoundary { .. }))
    );
    assert!(canonical_manifest_matches_for_test(&metadata, &full));

    let (artifact, eligibility) =
        whole_frame_artifact(&arena, &roots, &properties, &generations);
    assert!(eligibility.eligible);
    let artifact_rects = compiled_whole_frame_graph(&artifact).test_rect_pass_snapshots();

    let (legacy_arena, legacy_roots, legacy_span, _, _) = prepared_wrapping_inline_span_tree();
    sample_transition(&legacy_arena, legacy_span);
    let legacy_rects =
        legacy_roots_graph(legacy_arena, &legacy_roots).test_rect_pass_snapshots();
    assert_eq!(artifact_rects, legacy_rects);
}

#[test]
fn nested_inline_spans_preserve_source_owner_dfs_and_legacy_rect_order() {
    let (arena, roots, expected_owners) = prepared_nested_inline_span_tree();
    let (properties, generations) = sync_identity(&arena, &roots);
    let (artifact, eligibility) =
        whole_frame_artifact(&arena, &roots, &properties, &generations);
    assert!(eligibility.eligible);
    assert_eq!(
        artifact
            .chunks
            .iter()
            .map(|chunk| chunk.owner)
            .collect::<Vec<_>>(),
        expected_owners
    );
    assert!(matches!(
        artifact.chunks[0].payload_identity,
        PaintPayloadIdentity::InlineIfcDecorations(_, _)
    ));
    assert!(matches!(
        artifact.chunks[2].payload_identity,
        PaintPayloadIdentity::InlineIfcDecorations(_, _)
    ));
    let artifact_rects = compiled_whole_frame_graph(&artifact).test_rect_pass_snapshots();
    let (legacy_arena, legacy_roots, _) = prepared_nested_inline_span_tree();
    let legacy_rects =
        legacy_roots_graph(legacy_arena, &legacy_roots).test_rect_pass_snapshots();
    assert_eq!(artifact_rects, legacy_rects);
}

#[test]
fn missing_or_malformed_inline_span_package_falls_back_before_full_hooks() {
    for malformed in [false, true] {
        let (arena, roots, span_key, _, _) = prepared_wrapping_inline_span_tree();
        let mut node = arena.get_mut(span_key).unwrap();
        let span = node.element.as_any_mut().downcast_mut::<Element>().unwrap();
        let package = span
            .inline_ifc_decoration_package_for_test()
            .expect("fixture must install a decoration package");
        if malformed {
            package.fragments[0].metadata.position[0] = f32::NAN;
        } else {
            package.fragments.clear();
        }
        drop(node);
        let (properties, generations) = sync_identity(&arena, &roots);
        take_full_artifact_record_count();
        let outcome = record_frame_artifact(
            &arena,
            &roots,
            &properties,
            &generations,
            RendererMode::Auto,
        )
        .unwrap();
        let FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility) = outcome else {
            panic!("invalid installed inline decoration must fail closed")
        };
        assert!(
            eligibility
                .reasons
                .contains(&FrameArtifactFallbackReason::LegacyBoundary(
                    LegacyPaintReason::MissingPreparedInlineDecoration
                ))
        );
        assert_eq!(take_full_artifact_record_count(), 0);
    }
}

#[test]
fn cross_owner_inline_span_package_falls_back_before_full_hooks() {
    let (arena, roots, span_key, _, _) = prepared_wrapping_inline_span_tree();
    let mut node = arena.get_mut(span_key).unwrap();
    let span = node.element.as_any_mut().downcast_mut::<Element>().unwrap();
    let package = span
        .inline_ifc_decoration_package_for_test()
        .expect("fixture must install a decoration package");
    package.source.0 = package.source.0.wrapping_add(1000);
    for fragment in &mut package.fragments {
        fragment.source = package.source;
    }
    drop(node);
    let (properties, generations) = sync_identity(&arena, &roots);
    take_full_artifact_record_count();
    let outcome = record_frame_artifact(
        &arena,
        &roots,
        &properties,
        &generations,
        RendererMode::Auto,
    )
    .unwrap();
    let FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility) = outcome else {
        panic!("a package from another source owner must fail closed")
    };
    assert!(
        eligibility
            .reasons
            .contains(&FrameArtifactFallbackReason::LegacyBoundary(
                LegacyPaintReason::MissingPreparedInlineDecoration
            ))
    );
    assert_eq!(take_full_artifact_record_count(), 0);
}

#[test]
fn inline_span_paint_mutation_refreshes_same_constraints_frame_for_typed_and_legacy() {
    fn mutate_paint(arena: &NodeArena, span_key: NodeKey) {
        let mut node = arena.get_mut(span_key).unwrap();
        let span = node.element.as_any_mut().downcast_mut::<Element>().unwrap();
        span.set_background_color_value(Color::rgb(22, 163, 74));
        span.set_opacity(0.6);
        span.set_border_left_color(Color::rgb(126, 34, 206));
        span.set_border_right_color(Color::rgb(126, 34, 206));
        span.set_border_top_color(Color::rgb(126, 34, 206));
        span.set_border_bottom_color(Color::rgb(126, 34, 206));
    }

    let (stale_arena, stale_roots, stale_span, stale_text, _) =
        prepared_wrapping_inline_span_tree();
    let stale_parent = stale_arena.parent_of(stale_span).unwrap();
    settle_wrapping_inline_span_frame(&stale_arena, stale_parent, stale_span, stale_text);
    mutate_paint(&stale_arena, stale_span);
    let (stale_properties, stale_generations) = sync_identity(&stale_arena, &stale_roots);
    take_full_artifact_record_count();
    let stale = record_frame_artifact(
        &stale_arena,
        &stale_roots,
        &stale_properties,
        &stale_generations,
        RendererMode::Auto,
    )
    .unwrap();
    let FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility) = stale else {
        panic!("recording without layout must not consume stale paint packages")
    };
    assert!(
        eligibility
            .reasons
            .contains(&FrameArtifactFallbackReason::LegacyBoundary(
                LegacyPaintReason::MissingPreparedInlineDecoration
            ))
    );
    assert_eq!(take_full_artifact_record_count(), 0);

    let (mut arena, roots, span_key, text_key, _) = prepared_wrapping_inline_span_tree();
    let parent_key = arena.parent_of(span_key).unwrap();
    settle_wrapping_inline_span_frame(&arena, parent_key, span_key, text_key);
    mutate_paint(&arena, span_key);
    let (measure, place) = wrapping_inline_span_constraints();
    crate::view::base_component::reset_layout_place_profile();
    crate::view::base_component::set_layout_place_profile_enabled(true);
    measure_and_place(&mut arena, parent_key, measure, place);
    crate::view::base_component::set_layout_place_profile_enabled(false);
    let profile = crate::view::base_component::take_layout_place_profile();
    assert_eq!(
        (
            profile.ifc_measure_cheap,
            profile.ifc_measure_shortcircuit,
            profile.ifc_measure_full,
        ),
        (0, 0, 0),
        "identical constraints must skip re-measuring the owning IFC root"
    );
    assert_eq!(profile.inline_ifc_root_install_calls, 1);
    assert_eq!(
        profile.inline_ifc_root_install_reuse_calls, 0,
        "paint-only damage must rebuild the installed package in this frame"
    );
    let (properties, generations) = sync_identity(&arena, &roots);
    let (artifact, eligibility) =
        whole_frame_artifact(&arena, &roots, &properties, &generations);
    assert!(eligibility.eligible);
    let PaintOp::PreparedInlineIfcDecoration(first) = &artifact.ops[0] else {
        panic!("refreshed span must record inline decoration")
    };
    assert_eq!(
        first.fill.fill_color.map(f32::to_bits),
        Color::rgb(22, 163, 74).to_rgba_f32().map(f32::to_bits)
    );
    assert_eq!(first.fill.opacity.to_bits(), 0.6_f32.to_bits());
    assert_eq!(
        first.border.as_ref().unwrap().border_side_colors[0].map(f32::to_bits),
        Color::rgb(126, 34, 206).to_rgba_f32().map(f32::to_bits)
    );
    let artifact_rects = compiled_whole_frame_graph(&artifact).test_rect_pass_snapshots();

    let (mut legacy_arena, legacy_roots, legacy_span, legacy_text, _) =
        prepared_wrapping_inline_span_tree();
    let legacy_parent = legacy_arena.parent_of(legacy_span).unwrap();
    settle_wrapping_inline_span_frame(&legacy_arena, legacy_parent, legacy_span, legacy_text);
    mutate_paint(&legacy_arena, legacy_span);
    measure_and_place(&mut legacy_arena, legacy_parent, measure, place);
    {
        let mut node = legacy_arena.get_mut(legacy_span).unwrap();
        let span = node.element.as_any_mut().downcast_mut::<Element>().unwrap();
        let package = span
            .inline_ifc_decoration_package_for_test()
            .expect("same frame must install a fresh legacy package");
        let first = package.fragments.first().unwrap();
        assert_eq!(
            first.metadata.fill_color.map(f32::to_bits),
            Color::rgb(22, 163, 74).to_rgba_f32().map(f32::to_bits)
        );
        assert_eq!(first.metadata.opacity.to_bits(), 0.6_f32.to_bits());
        assert_eq!(
            first.metadata.border_colors[0].map(f32::to_bits),
            Color::rgb(126, 34, 206).to_rgba_f32().map(f32::to_bits)
        );
    }
    let legacy_rects =
        legacy_roots_graph(legacy_arena, &legacy_roots).test_rect_pass_snapshots();
    assert_eq!(artifact_rects, legacy_rects);
}

#[test]
fn clean_inline_span_origin_move_preserves_install_reuse_fast_path() {
    let (mut arena, _, span_key, text_key, _) = prepared_wrapping_inline_span_tree();
    let parent_key = arena.parent_of(span_key).unwrap();
    settle_wrapping_inline_span_frame(&arena, parent_key, span_key, text_key);
    let before = arena
        .get_mut(span_key)
        .unwrap()
        .element
        .as_any_mut()
        .downcast_mut::<Element>()
        .unwrap()
        .inline_ifc_decoration_package_for_test()
        .unwrap()
        .clone();

    let (measure, mut place) = wrapping_inline_span_constraints();
    place.parent_x = 7.0;
    place.parent_y = 11.0;
    crate::view::base_component::reset_layout_place_profile();
    crate::view::base_component::set_layout_place_profile_enabled(true);
    measure_and_place(&mut arena, parent_key, measure, place);
    crate::view::base_component::set_layout_place_profile_enabled(false);
    let profile = crate::view::base_component::take_layout_place_profile();
    assert_eq!(
        (
            profile.ifc_measure_cheap,
            profile.ifc_measure_shortcircuit,
            profile.ifc_measure_full,
        ),
        (0, 0, 0),
        "a clean same-constraints move must skip IFC measure"
    );
    assert_eq!(profile.inline_ifc_root_install_calls, 1);
    assert_eq!(
        profile.inline_ifc_root_install_reuse_calls, 1,
        "the paint freshness guard must preserve clean origin-only reuse"
    );

    let after = arena
        .get_mut(span_key)
        .unwrap()
        .element
        .as_any_mut()
        .downcast_mut::<Element>()
        .unwrap()
        .inline_ifc_decoration_package_for_test()
        .unwrap()
        .clone();
    let mut expected = before;
    for fragment in &mut expected.fragments {
        fragment.metadata.position[0] += 7.0;
        fragment.metadata.position[1] += 11.0;
    }
    assert_eq!(after, expected);
}

#[test]
fn non_painting_inline_span_uses_only_typed_empty_decoration() {
    let (arena, roots, span_key, _, _) = prepared_wrapping_inline_span_tree();
    let mut node = arena.get_mut(span_key).unwrap();
    let span = node.element.as_any_mut().downcast_mut::<Element>().unwrap();
    span.set_should_paint_for_test(false);
    span.inline_ifc_decoration_package_for_test()
        .unwrap()
        .fragments
        .clear();
    drop(node);
    let (properties, generations) = sync_identity(&arena, &roots);
    let (mut artifact, eligibility) =
        whole_frame_artifact(&arena, &roots, &properties, &generations);
    assert!(eligibility.eligible);
    let span_chunk = artifact
        .chunks
        .iter()
        .find(|chunk| chunk.owner == span_key)
        .unwrap();
    assert!(span_chunk.op_range.is_empty());
    assert!(matches!(
        &span_chunk.payload_identity,
        PaintPayloadIdentity::InlineIfcDecorations(shadows, decorations)
            if shadows.is_empty() && decorations.is_empty()
    ));
    artifact.chunks[0].bounds.width = 0.0;
    artifact.chunks[0].bounds.height = 0.0;
    take_artifact_compile_count();
    let _ = compiled_whole_frame_graph(&artifact);
    assert_eq!(take_artifact_compile_count(), 1);
}

#[test]
fn inline_decoration_constructor_and_compiler_reject_link_or_identity_drift() {
    let (arena, roots, _, _, _) = prepared_wrapping_inline_span_tree();
    let (properties, generations) = sync_identity(&arena, &roots);
    let artifact = whole_frame_artifact(&arena, &roots, &properties, &generations).0;
    let PaintOp::PreparedInlineIfcDecoration(first) = &artifact.ops[0] else {
        panic!("fixture must start with an inline decoration")
    };
    let mut mismatched_border = first.border.clone().expect("fixture must have a border");
    mismatched_border.depth = 1.0;
    assert!(
        PreparedInlineIfcDecorationOp::new(
            first.descriptor.clone(),
            first.fill.clone(),
            Some(mismatched_border),
        )
        .is_none(),
        "constructor must reject linked-field mismatch"
    );
    let mut gradient_fill = first.fill.clone();
    gradient_fill.gradient = Some(Default::default());
    assert!(
        PreparedInlineIfcDecorationOp::new(
            first.descriptor.clone(),
            gradient_fill,
            first.border.clone(),
        )
        .is_none(),
        "M7B explicitly excludes gradients"
    );
    let mut overflow_fill = first.fill.clone();
    overflow_fill.position[0] = f32::MAX;
    overflow_fill.size[0] = f32::MAX;
    let mut overflow_border = first.border.clone().unwrap();
    overflow_border.position[0] = f32::MAX;
    overflow_border.size[0] = f32::MAX;
    assert!(
        PreparedInlineIfcDecorationOp::new(
            first.descriptor.clone(),
            overflow_fill,
            Some(overflow_border),
        )
        .is_none(),
        "large finite rect inputs whose edge overflows must fail closed"
    );

    let mut tampered_params = artifact.clone();
    let PaintOp::PreparedInlineIfcDecoration(op) = &mut tampered_params.ops[0] else {
        unreachable!()
    };
    op.fill.position[0] += 1.0;
    assert_compiler_rejects_before_emit(&tampered_params, "inline fill param drift");

    let mut nan_bounds = artifact.clone();
    nan_bounds.chunks[0].bounds.x = f32::NAN;
    assert_compiler_rejects_before_emit(&nan_bounds, "NaN chunk bounds");
    let mut negative_bounds = artifact.clone();
    negative_bounds.chunks[0].bounds.width = -1.0;
    assert_compiler_rejects_before_emit(&negative_bounds, "negative chunk bounds");

    let mut tampered_descriptor = artifact.clone();
    let PaintOp::PreparedInlineIfcDecoration(op) = &mut tampered_descriptor.ops[0] else {
        unreachable!()
    };
    op.descriptor.source = op.descriptor.source.wrapping_add(1);
    assert_compiler_rejects_before_emit(&tampered_descriptor, "inline descriptor drift");

    let mut missing_fragment = artifact;
    missing_fragment.ops.remove(0);
    missing_fragment.chunks[0].op_range.end -= 1;
    for chunk in &mut missing_fragment.chunks[1..] {
        chunk.op_range.start -= 1;
        chunk.op_range.end -= 1;
    }
    assert_compiler_rejects_before_emit(&missing_fragment, "missing inline fragment");

    let (arena, roots, _, _, _) = prepared_wrapping_inline_span_tree();
    let (properties, generations) = sync_identity(&arena, &roots);
    let artifact = whole_frame_artifact(&arena, &roots, &properties, &generations).0;
    let mut swapped = artifact.clone();
    swapped.ops.swap(0, 1);
    refresh_inline_decoration_payload_identity(&mut swapped);
    assert_compiler_rejects_before_emit(
        &swapped,
        "inline fragment order drift with rebuilt payload",
    );

    let mut endpoint_drift = artifact.clone();
    let PaintOp::PreparedInlineIfcDecoration(op) = endpoint_drift.ops[0].clone() else {
        unreachable!()
    };
    let mut descriptor = op.descriptor;
    descriptor.is_first_for_source = false;
    endpoint_drift.ops[0] = PaintOp::PreparedInlineIfcDecoration(
        PreparedInlineIfcDecorationOp::new(descriptor, op.fill, op.border).unwrap(),
    );
    refresh_inline_decoration_payload_identity(&mut endpoint_drift);
    assert_compiler_rejects_before_emit(
        &endpoint_drift,
        "inline endpoint drift with rebuilt op and payload",
    );

    let mut cross_source = artifact;
    let PaintOp::PreparedInlineIfcDecoration(op) = cross_source.ops[1].clone() else {
        unreachable!()
    };
    let mut descriptor = op.descriptor;
    descriptor.source = descriptor.source.wrapping_add(1);
    cross_source.ops[1] = PaintOp::PreparedInlineIfcDecoration(
        PreparedInlineIfcDecorationOp::new(descriptor, op.fill, op.border).unwrap(),
    );
    refresh_inline_decoration_payload_identity(&mut cross_source);
    assert_compiler_rejects_before_emit(
        &cross_source,
        "cross-source fragment with rebuilt op and payload",
    );
}

#[test]
fn root_opacity_group_neutralizes_inline_span_and_text_once() {
    let (arena, roots, span_key, _, _) = prepared_wrapping_inline_span_tree_with_opacity(0.5);
    let (properties, generations) = sync_identity(&arena, &roots);
    let (artifact, eligibility) =
        root_group_artifact(&arena, &roots, &properties, &generations);
    assert!(eligibility.eligible);
    assert!(matches!(
        artifact.target,
        PaintArtifactTarget::RootOpacityGroup { root, .. } if root == span_key
    ));
    assert!(matches!(
        artifact.chunks[0].payload_identity,
        PaintPayloadIdentity::InlineIfcDecorations(_, _)
    ));
    assert!(
        artifact
            .ops
            .iter()
            .any(|op| matches!(op, PaintOp::PreparedText(_)))
    );
    artifact.ops.iter().for_each(assert_neutral_opacity);
    let graph = compiled_whole_frame_graph(&artifact);
    assert_eq!(
        graph
            .test_graphics_passes::<crate::view::render_pass::composite_layer_pass::CompositeLayerPass>()
            .len(),
        1
    );
}

#[test]
fn deferred_inline_span_remains_fallback_before_full_hooks() {
    let (arena, roots, span_key, _, _) = prepared_wrapping_inline_span_tree();
    let mut node = arena.get_mut(span_key).unwrap();
    let span = node.element.as_any_mut().downcast_mut::<Element>().unwrap();
    let mut style = Style::new();
    style.insert(
        PropertyId::Position,
        ParsedValue::Position(Position::absolute().clip(ClipMode::Viewport)),
    );
    span.apply_style(style);
    drop(node);
    let (properties, generations) = sync_identity(&arena, &roots);
    take_full_artifact_record_count();
    let outcome = record_frame_artifact(
        &arena,
        &roots,
        &properties,
        &generations,
        RendererMode::Auto,
    )
    .unwrap();
    let FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility) = outcome else {
        panic!("deferred inline span must remain on legacy")
    };
    assert!(eligibility.reasons.iter().any(|reason| matches!(
        reason,
        FrameArtifactFallbackReason::LegacyBoundary(LegacyPaintReason::Deferred)
            | FrameArtifactFallbackReason::DeferredBoundary(_)
    )));
    assert_eq!(take_full_artifact_record_count(), 0);
}

#[test]
fn atomic_inline_span_remains_fallback_before_full_hooks() {
    let (arena, roots, span_key, _, _) = prepared_wrapping_inline_span_tree();
    let mut node = arena.get_mut(span_key).unwrap();
    node.element
        .as_any_mut()
        .downcast_mut::<Element>()
        .unwrap()
        .install_empty_inline_ifc_atomic_package_for_test();
    drop(node);
    let (properties, generations) = sync_identity(&arena, &roots);
    take_full_artifact_record_count();
    let outcome = record_frame_artifact(
        &arena,
        &roots,
        &properties,
        &generations,
        RendererMode::Auto,
    )
    .unwrap();
    let FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility) = outcome else {
        panic!("atomic placement stays outside M7B")
    };
    assert!(
        eligibility
            .reasons
            .contains(&FrameArtifactFallbackReason::LegacyBoundary(
                LegacyPaintReason::InlineIfc
            ))
    );
    assert_eq!(take_full_artifact_record_count(), 0);
}
