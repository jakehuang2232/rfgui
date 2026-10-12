use super::*;

#[test]
fn wrapping_inline_span_owns_typed_decoration_before_text() {
    let (arena, roots, span_key, text_key, fragment_count) = prepared_wrapping_inline_span_tree();
    let (properties, generations) = sync_identity(&arena, &roots);
    take_full_artifact_record_count();
    let artifact = whole_frame_artifact(&arena, &roots, &properties, &generations);
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
}

#[test]
fn wrapping_inline_span_shadows_preserve_fragment_order() {
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
            // The compiler snaps the span over an unsnapped root.
            crate::view::base_component::paint_offset_after_owner_snap(
                [
                    span.layout_state.layout_position.x,
                    span.layout_state.layout_position.y,
                ],
                [0.0, 0.0],
                1.0,
            )
            .unwrap(),
        )
    };
    assert!(
        fragment_count >= 2,
        "fixture must wrap across multiple lines"
    );
    let (properties, generations) = sync_identity(&arena, &roots);
    let manifest =
        |mode| record_coverage_manifest(&arena, &roots, true, mode, &properties, &generations);
    let metadata = manifest(CoverageRecordingMode::MetadataOnly);
    let full = manifest(CoverageRecordingMode::FullArtifact);
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

    let artifact = whole_frame_artifact(&arena, &roots, &properties, &generations);
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
            let min_x = shadow.shape.x;
            let min_y = shadow.shape.y;
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
fn sampled_inline_span_layout_transition_keeps_metadata_full() {
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
    let manifest =
        |mode| record_coverage_manifest(&arena, &roots, true, mode, &properties, &generations);
    let metadata = manifest(CoverageRecordingMode::MetadataOnly);
    let full = manifest(CoverageRecordingMode::FullArtifact);
    assert!(canonical_manifest_matches_for_test(&metadata, &full));

    let _ = whole_frame_artifact(&arena, &roots, &properties, &generations);
}

#[test]
fn nested_inline_spans_preserve_source_owner_dfs() {
    let (arena, roots, expected_owners) = prepared_nested_inline_span_tree();
    let (properties, generations) = sync_identity(&arena, &roots);
    let artifact = whole_frame_artifact(&arena, &roots, &properties, &generations);
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
}

#[test]
fn inline_span_paint_mutation_refreshes_same_constraints_frame() {
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
    let artifact = whole_frame_artifact(&arena, &roots, &properties, &generations);
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
    let mut artifact = whole_frame_artifact(&arena, &roots, &properties, &generations);
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
