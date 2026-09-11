use super::*;

#[test]
fn production_safe_leaf_uses_direct_legacy_build_without_artifact_recording() {
    let (arena, roots) = prepared_safe_leaf();
    crate::view::paint::take_full_artifact_record_count();
    crate::view::paint::take_artifact_compile_count();
    let production_graph = build_roots_graph(arena, &roots, true);
    assert_eq!(
        crate::view::paint::take_full_artifact_record_count(),
        0,
        "production legacy authority must not invoke the full artifact recorder"
    );
    assert_eq!(crate::view::paint::take_artifact_compile_count(), 0);

    let (legacy_arena, legacy_roots) = prepared_safe_leaf();
    let direct_legacy_graph = build_roots_graph(legacy_arena, &legacy_roots, false);
    assert!(!production_graph.test_rect_pass_snapshots().is_empty());
    assert_eq!(
        production_graph.test_rect_pass_snapshots(),
        direct_legacy_graph.test_rect_pass_snapshots(),
        "production dispatch must preserve the direct legacy pass snapshot"
    );
}

#[test]
fn production_multi_root_frame_never_mixes_artifact_and_legacy_authority() {
    let (arena, roots) = prepared_mixed_eligibility_roots();
    crate::view::paint::take_full_artifact_record_count();
    let production_graph = build_roots_graph(arena, &roots, true);
    assert_eq!(
        crate::view::paint::take_full_artifact_record_count(),
        0,
        "safe roots must not record artifacts beside legacy-only roots"
    );

    let (legacy_arena, legacy_roots) = prepared_mixed_eligibility_roots();
    let direct_legacy_graph = build_roots_graph(legacy_arena, &legacy_roots, false);
    assert_eq!(
        production_graph.test_rect_pass_snapshots(),
        direct_legacy_graph.test_rect_pass_snapshots(),
        "every root in the frame must use the same direct legacy authority"
    );
}
