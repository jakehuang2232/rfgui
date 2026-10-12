use super::*;

#[test]
fn standalone_text_root_and_nested_fractional_offset_compile_strictly() {
    for nested in [false, true] {
        let rects = eligible_whole_frame_rects(
            || {
                let (arena, roots, _) = prepared_text_tree(nested);
                (arena, roots)
            },
            PaintParityConfig {
                width: 640,
                height: 480,
                format: wgpu::TextureFormat::Rgba8Unorm,
                scale_factor: 2.0,
                initial_scissor: None,
            },
        );
        assert!(rects.is_empty(), "text fixture should not emit rect passes");
    }
}

#[test]
fn prepared_text_artifact_compiles_after_arena_is_dropped() {
    let artifact = {
        let (arena, roots, _) = prepared_text_tree(true);
        let (properties, generations) = sync_identity(&arena, &roots);
        whole_frame_artifact(&arena, &roots, &properties, &generations)
    };
    assert!(
        artifact
            .ops
            .iter()
            .any(|op| matches!(op, PaintOp::PreparedText(_)))
    );
    let mut graph = compiled_whole_frame_graph(&artifact);
    graph
        .test_compile_snapshot()
        .expect("retained text params must compile without the arena");
}

#[test]
fn text_color_only_change_reuses_shaping_and_changes_retained_payload() {
    let (arena, roots, text_key) = prepared_text_tree(false);
    let mut properties = PropertyTrees::default();
    properties.sync(&arena, &roots);
    let mut generations = PaintGenerationTracker::default();
    generations.sync(&arena, &roots, &properties);
    let before_context = arena
        .get(text_key)
        .unwrap()
        .element
        .as_any()
        .downcast_ref::<Text>()
        .unwrap()
        .shaped_context_for_test()
        .unwrap()
        .clone();
    let before = whole_frame_artifact(&arena, &roots, &properties, &generations);

    arena
        .get_mut(text_key)
        .unwrap()
        .element
        .as_any_mut()
        .downcast_mut::<Text>()
        .unwrap()
        .set_color(Color::rgb(230, 40, 70));
    properties.sync(&arena, &roots);
    generations.sync(&arena, &roots, &properties);
    let after_context = arena
        .get(text_key)
        .unwrap()
        .element
        .as_any()
        .downcast_ref::<Text>()
        .unwrap()
        .shaped_context_for_test()
        .unwrap()
        .clone();
    let after = whole_frame_artifact(&arena, &roots, &properties, &generations);

    assert!(Arc::ptr_eq(&before_context, &after_context));
    assert_ne!(
        first_text_color_bits(&before),
        first_text_color_bits(&after)
    );
}

#[test]
fn unchanged_text_records_identical_strict_snapshots_across_frames() {
    let (arena, roots, _) = prepared_text_tree(true);
    let (properties, generations) = sync_identity(&arena, &roots);
    let first = whole_frame_artifact(&arena, &roots, &properties, &generations);
    let second = whole_frame_artifact(&arena, &roots, &properties, &generations);
    let mut first_graph = compiled_whole_frame_graph(&first);
    let mut second_graph = compiled_whole_frame_graph(&second);
    assert_eq!(
        first_graph.test_compile_snapshot().unwrap(),
        second_graph.test_compile_snapshot().unwrap()
    );
}

#[test]
fn hidden_empty_and_zero_opacity_text_are_transparent_without_chunks() {
    for kind in 0..3 {
        let mut arena = new_test_arena();
        let mut text = Text::new_with_id(
            181 + kind,
            0.0,
            0.0,
            if kind == 1 { 0.0 } else { 80.0 },
            30.0,
            if kind == 0 { "" } else { "text" },
        );
        if kind == 2 {
            text.set_opacity(0.0);
        }
        let root = commit_element(&mut arena, Box::new(text));
        let (measure, place) = constraints();
        measure_and_place(&mut arena, root, measure, place);
        let roots = [root];
        let (properties, generations) = sync_identity(&arena, &roots);
        let manifest =
            |mode| record_coverage_manifest(&arena, &roots, true, mode, &properties, &generations);
        let metadata = manifest(CoverageRecordingMode::MetadataOnly);
        let full = manifest(CoverageRecordingMode::FullArtifact);
        assert!(matches!(
            metadata.items.as_slice(),
            [PaintCoverageItem::TransparentNode { owner, .. }] if *owner == root
        ));
        assert!(canonical_manifest_matches_for_test(&metadata, &full));

        let artifact = whole_frame_artifact(&arena, &roots, &properties, &generations);
        assert!(artifact.chunks.is_empty());
        assert!(artifact.ops.is_empty());
    }
}

#[test]
fn inline_owned_text_records_source_owned_glyphs_in_one_pass() {
    let (arena, roots, text_key) = prepared_inline_owned_text_tree();
    let (properties, generations) = sync_identity(&arena, &roots);
    take_full_artifact_record_count();
    let artifact = whole_frame_artifact(&arena, &roots, &properties, &generations);
    assert_eq!(artifact.chunks.len(), 1);
    assert_eq!(artifact.chunks[0].owner, text_key);
    assert!(
        matches!(
            artifact.chunks[0].payload_identity,
            PaintPayloadIdentity::PreparedTexts(_)
        ),
        "source Text must own the complete prepared glyph identity"
    );
    assert_eq!(take_full_artifact_record_count(), 1);

    let artifact_graph = compiled_whole_frame_graph(&artifact);
    let artifact_passes = artifact_graph
        .test_graphics_passes::<crate::view::render_pass::text_pass::TextPreparedInputPass>();
    assert_eq!(artifact_passes.len(), 1);
}

#[test]
fn empty_text_records_canonical_transparent_node_without_payload() {
    let mut arena = new_test_arena();
    let text_key = commit_element(
        &mut arena,
        Box::new(Text::new_with_id(0x7b20, 0.0, 0.0, 0.0, 0.0, "")),
    );
    let (measure, place) = constraints();
    measure_and_place(&mut arena, text_key, measure, place);
    let roots = [text_key];
    let (properties, generations) = sync_identity(&arena, &roots);
    let manifest =
        |mode| record_coverage_manifest(&arena, &roots, true, mode, &properties, &generations);
    let metadata = manifest(CoverageRecordingMode::MetadataOnly);
    let full = manifest(CoverageRecordingMode::FullArtifact);
    assert!(matches!(
        metadata.items.as_slice(),
        [PaintCoverageItem::TransparentNode { owner, .. }] if *owner == text_key
    ));
    assert!(canonical_manifest_matches_for_test(&metadata, &full));

    let artifact = whole_frame_artifact(&arena, &roots, &properties, &generations);
    assert!(artifact.chunks.is_empty());
    assert!(artifact.ops.is_empty());
    take_artifact_compile_count();
    let _ = compiled_whole_frame_graph(&artifact);
    assert_eq!(take_artifact_compile_count(), 1);
}
