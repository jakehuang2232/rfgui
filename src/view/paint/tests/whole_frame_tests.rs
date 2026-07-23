use super::*;

#[test]
fn whole_frame_artifact_matches_legacy_for_nested_multi_root_order() {
    let (arena, roots, child) = prepared_plain_tree();
    let (properties, generations) = sync_identity(&arena, &roots);
    take_full_artifact_record_count();
    let (artifact, eligibility) =
        whole_frame_artifact(&arena, &roots, &properties, &generations);
    assert_eq!(
        take_full_artifact_record_count(),
        3,
        "eligible preflight must invoke the full hook exactly once per node"
    );
    assert!(eligibility.eligible);
    assert_eq!(eligibility.chunk_count, 3);
    assert_eq!(
        artifact
            .chunks
            .iter()
            .map(|chunk| chunk.owner)
            .collect::<Vec<_>>(),
        vec![roots[0], child, roots[1]],
        "parent self decoration must precede its child and later roots"
    );
    assert_eq!(artifact.chunks[0].op_range, 0..1);
    assert_eq!(artifact.chunks[1].op_range, 1..2);
    assert_eq!(artifact.chunks[2].op_range, 2..3);

    let artifact_graph = compiled_whole_frame_graph(&artifact);
    let (legacy_arena, legacy_roots, _) = prepared_plain_tree();
    let legacy_graph = legacy_roots_graph(legacy_arena, &legacy_roots);
    assert_eq!(
        artifact_graph.test_rect_pass_snapshots(),
        legacy_graph.test_rect_pass_snapshots()
    );
}

#[test]
fn whole_frame_zero_opacity_keeps_empty_chunk_and_matches_legacy() {
    let mut arena = new_test_arena();
    let mut empty_element = leaf_element(110, Color::rgb(255, 0, 0), 1.0, false);
    let mut empty_style = Style::new();
    empty_style.insert(PropertyId::Opacity, ParsedValue::Opacity(Opacity::new(0.0)));
    empty_element.apply_style(empty_style);
    let empty = commit_element(&mut arena, Box::new(empty_element));
    let visible = commit_element(
        &mut arena,
        Box::new(leaf_element(111, Color::rgb(0, 0, 255), 1.0, false)),
    );
    let roots = vec![empty, visible];
    let (measure, place) = constraints();
    measure_and_place(&mut arena, empty, measure, place);
    measure_and_place(&mut arena, visible, measure, place);
    let (properties, generations) = sync_identity(&arena, &roots);
    let (artifact, eligibility) =
        whole_frame_artifact(&arena, &roots, &properties, &generations);
    assert_eq!(eligibility.chunk_count, 2);
    assert_eq!(eligibility.op_count, 1);
    assert_eq!(artifact.chunks[0].op_range, 0..0);
    assert_eq!(artifact.chunks[1].op_range, 0..1);

    let artifact_graph = compiled_whole_frame_graph(&artifact);
    let mut legacy_arena = new_test_arena();
    let mut legacy_empty_element = leaf_element(110, Color::rgb(255, 0, 0), 1.0, false);
    let mut legacy_empty_style = Style::new();
    legacy_empty_style.insert(PropertyId::Opacity, ParsedValue::Opacity(Opacity::new(0.0)));
    legacy_empty_element.apply_style(legacy_empty_style);
    let legacy_empty = commit_element(&mut legacy_arena, Box::new(legacy_empty_element));
    let legacy_visible = commit_element(
        &mut legacy_arena,
        Box::new(leaf_element(111, Color::rgb(0, 0, 255), 1.0, false)),
    );
    measure_and_place(&mut legacy_arena, legacy_empty, measure, place);
    measure_and_place(&mut legacy_arena, legacy_visible, measure, place);
    let legacy_graph = legacy_roots_graph(legacy_arena, &[legacy_empty, legacy_visible]);
    assert_eq!(
        artifact_graph.test_rect_pass_snapshots(),
        legacy_graph.test_rect_pass_snapshots()
    );
}

#[test]
fn property_neutral_canary_rejects_zero_op_non_neutral_node_before_full_recording() {
    let (arena, roots) = prepared_zero_opacity_tree();
    let (properties, generations) = sync_identity(&arena, &roots);
    take_full_artifact_record_count();
    let outcome = record_property_neutral_frame_artifact(
        &arena,
        &roots,
        &properties,
        &generations,
        RendererMode::Auto,
    )
    .expect("canary uses whole-frame fallback");
    let FrameArtifactRecordOutcome::WholeFrameLegacyFallback(eligibility) = outcome else {
        panic!("zero-op effect node must not enter M6A authority")
    };
    assert!(
        eligibility
            .reasons
            .contains(&FrameArtifactFallbackReason::PropertyBoundary(roots[0]))
    );
    assert_eq!(
        take_full_artifact_record_count(),
        0,
        "reachable property state must reject during metadata preflight"
    );
}

#[test]
fn property_neutral_canary_rejects_deferred_boundaries_preflight() {
    let mut deferred = Element::new_with_id(0x6a02, 0.0, 0.0, 20.0, 20.0);
    let mut style = Style::new();
    style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .left(Length::px(0.0))
                .clip(ClipMode::Viewport),
        ),
    );
    deferred.apply_style(style);
    let mut deferred_arena = new_test_arena();
    let deferred_root = commit_element(&mut deferred_arena, Box::new(deferred));
    let (measure, place) = constraints();
    measure_and_place(&mut deferred_arena, deferred_root, measure, place);
    let (properties, generations) = sync_identity(&deferred_arena, &[deferred_root]);
    take_full_artifact_record_count();
    let deferred_outcome = record_property_neutral_frame_artifact(
        &deferred_arena,
        &[deferred_root],
        &properties,
        &generations,
        RendererMode::Auto,
    )
    .expect("deferred paint is a whole-frame fallback");
    assert!(matches!(
        deferred_outcome,
        FrameArtifactRecordOutcome::WholeFrameLegacyFallback(FrameArtifactEligibility {
            reasons,
            ..
        }) if reasons.contains(&FrameArtifactFallbackReason::PropertyBoundary(deferred_root))
    ));
    assert_eq!(take_full_artifact_record_count(), 0);
}

#[test]
fn whole_frame_auto_falls_back_and_forced_reports_before_compilation() {
    let mut arena = new_test_arena();
    let safe = commit_element(
        &mut arena,
        Box::new(leaf_element(120, Color::rgb(1, 2, 3), 1.0, false)),
    );
    let unsupported = commit_element(
        &mut arena,
        Box::new(Text::new(0.0, 0.0, 20.0, 20.0, "text")),
    );
    let roots = [safe, unsupported];
    let (measure, place) = constraints();
    measure_and_place(&mut arena, safe, measure, place);
    let (properties, generations) = sync_identity(&arena, &roots);
    take_full_artifact_record_count();

    let auto = record_frame_artifact(
        &arena,
        &roots,
        &properties,
        &generations,
        RendererMode::Auto,
    )
    .expect("Auto must use whole-frame fallback");
    let FrameArtifactRecordOutcome::WholeFrameLegacyFallback(auto) = auto else {
        panic!("mixed host frame must not return a partial artifact")
    };
    assert_eq!(
        take_full_artifact_record_count(),
        0,
        "a late unsupported boundary must prevent all full recording"
    );
    assert!(!auto.eligible);
    assert!(
        auto.reasons
            .contains(&FrameArtifactFallbackReason::LegacyBoundary(
                LegacyPaintReason::MissingPreparedText
            ))
    );

    let forced = record_frame_artifact(
        &arena,
        &roots,
        &properties,
        &generations,
        RendererMode::ForcedForTests,
    )
    .expect_err("forced mode must surface eligibility failure");
    assert_eq!(
        take_full_artifact_record_count(),
        0,
        "ForcedForTests uses the same metadata-only preflight"
    );
    assert!(
        forced
            .reasons
            .contains(&FrameArtifactFallbackReason::LegacyBoundary(
                LegacyPaintReason::MissingPreparedText
            ))
    );

    let legacy_mode = record_frame_artifact(
        &arena,
        &roots,
        &properties,
        &generations,
        RendererMode::Legacy,
    )
    .expect("production-default legacy mode is a no-op");
    assert!(matches!(
        legacy_mode,
        FrameArtifactRecordOutcome::WholeFrameLegacyFallback(FrameArtifactEligibility {
            reasons,
            ..
        }) if reasons == vec![FrameArtifactFallbackReason::RendererLegacy]
    ));
}

#[test]
fn whole_frame_recording_is_deterministic_and_revisions_track_mutation() {
    let (arena, roots, _) = prepared_plain_tree();
    let (mut properties, mut generations) = sync_identity(&arena, &roots);
    let (first, _) = whole_frame_artifact(&arena, &roots, &properties, &generations);
    let (unchanged, _) = whole_frame_artifact(&arena, &roots, &properties, &generations);
    assert_eq!(format!("{first:?}"), format!("{unchanged:?}"));

    arena
        .get_mut(roots[0])
        .expect("root exists")
        .element
        .as_any_mut()
        .downcast_mut::<Element>()
        .expect("root is Element")
        .set_background_color_value(Color::rgb(90, 80, 70));
    properties.sync(&arena, &roots);
    generations.sync(&arena, &roots, &properties);
    let (changed, _) = whole_frame_artifact(&arena, &roots, &properties, &generations);
    assert_eq!(first.chunks[0].id, changed.chunks[0].id);
    assert_ne!(
        first.chunks[0].content_revision,
        changed.chunks[0].content_revision
    );
    assert_eq!(first.chunks[1].id, changed.chunks[1].id);
    assert_eq!(
        first.chunks[1].content_revision,
        changed.chunks[1].content_revision
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn window_like_native_showcase_manifest_is_canonical_and_explains_property_cutout() {
    let (arena, roots) = window_like_native_showcase_fixture();
    let (properties, generations) = sync_identity(&arena, &roots);
    let manifest = |mode| {
        record_coverage_manifest(&arena, &roots, false, true, mode, &properties, &generations)
    };
    let metadata = manifest(CoverageRecordingMode::MetadataOnly);
    let full = manifest(CoverageRecordingMode::FullArtifact);
    let legacy_boundaries = |manifest: &PaintCoverageManifest| {
        manifest
            .items
            .iter()
            .filter_map(|item| match item {
                PaintCoverageItem::LegacyBoundary {
                    root,
                    stable_id,
                    reason,
                    ..
                } => {
                    let node = arena.get(*root).expect("legacy owner must exist");
                    let component = node
                        .element
                        .element_type_name()
                        .rsplit("::")
                        .next()
                        .unwrap_or("<unknown>");
                    Some(format!(
                        "component={component} stable_id={stable_id} owner={root:?} reason={reason:?}"
                    ))
                }
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let metadata_legacy = legacy_boundaries(&metadata);
    let full_legacy = legacy_boundaries(&full);

    assert!(
        metadata.validation_errors.is_empty(),
        "metadata validation errors: {:?}",
        metadata.validation_errors
    );
    assert!(
        full.validation_errors.is_empty(),
        "full validation errors: {:?}",
        full.validation_errors
    );
    let assert_property_cutout = |phase: &str, boundaries: &[String]| {
        assert_eq!(
            boundaries.len(),
            1,
            "{phase} must contain only the property-authority scroll cutout, not native host fallbacks:\n{}",
            boundaries.join("\n")
        );
        assert!(
            boundaries[0].contains("component=Element stable_id=32513")
                && boundaries[0].contains("reason=ScrollContainer"),
            "{phase} must identify the exact property-owned Both scroll host:\n{}",
            boundaries.join("\n")
        );
    };
    assert_property_cutout("metadata", &metadata_legacy);
    assert_property_cutout("full", &full_legacy);
    assert!(canonical_manifest_matches_for_test(&metadata, &full));
}
