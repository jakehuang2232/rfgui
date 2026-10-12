use super::*;

#[test]
fn whole_frame_artifact_keeps_nested_multi_root_order() {
    let (arena, roots, child) = prepared_plain_tree();
    let (properties, generations) = sync_identity(&arena, &roots);
    take_full_artifact_record_count();
    let artifact = whole_frame_artifact(&arena, &roots, &properties, &generations);
    assert_eq!(
        take_full_artifact_record_count(),
        3,
        "eligible preflight must invoke the full hook exactly once per node"
    );
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
}

#[test]
fn whole_frame_zero_opacity_keeps_empty_chunk() {
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
    let artifact = whole_frame_artifact(&arena, &roots, &properties, &generations);
    assert_eq!(artifact.chunks[0].op_range, 0..0);
    assert_eq!(artifact.chunks[1].op_range, 0..1);
}

#[test]
fn whole_frame_recording_is_deterministic_and_revisions_track_mutation() {
    let (arena, roots, _) = prepared_plain_tree();
    let (mut properties, mut generations) = sync_identity(&arena, &roots);
    let first = whole_frame_artifact(&arena, &roots, &properties, &generations);
    let unchanged = whole_frame_artifact(&arena, &roots, &properties, &generations);
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
    let changed = whole_frame_artifact(&arena, &roots, &properties, &generations);
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
