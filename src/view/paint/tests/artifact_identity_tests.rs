use super::*;

#[test]
fn paint_artifact_leaf_fill_border_opacity_and_opaque() {
    for (opacity, expected_opaque) in [(1.0, true), (0.65, false)] {
        let (arena, root, properties, generations) =
            prepared_leaf(10, Color::rgb(220, 30, 40), opacity, true);
        let artifact = artifact_graph(&arena, root, &properties, &generations);

        assert_eq!(
            artifact.test_rect_pass_snapshots()[0].opaque,
            expected_opaque
        );
        assert_eq!(
            artifact.test_rect_pass_snapshots()[0].opacity_bits,
            opacity.to_bits()
        );
        assert_eq!(artifact.test_rect_pass_snapshots().len(), 2);
    }
}

#[test]
fn paint_artifact_chunk_identity_stays_stable_and_revision_tracks_paint() {
    let (arena, root, mut properties, mut generations) =
        prepared_leaf(11, Color::rgb(10, 20, 30), 1.0, false);
    let first = whole_frame_artifact(&arena, &[root], &properties, &generations);

    arena
        .get_mut(root)
        .expect("root exists")
        .element
        .as_any_mut()
        .downcast_mut::<Element>()
        .expect("root is Element")
        .set_background_color_value(Color::rgb(40, 50, 60));
    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);
    let second = whole_frame_artifact(&arena, &[root], &properties, &generations);

    assert_eq!(first.chunks[0].id, second.chunks[0].id);
    assert_ne!(
        first.chunks[0].content_revision,
        second.chunks[0].content_revision
    );
    assert_ne!(
        first.chunks[0].content_revision.self_paint_revision,
        second.chunks[0].content_revision.self_paint_revision
    );
    assert_eq!(first.chunks[0].properties, second.chunks[0].properties);
}

#[test]
fn opacity_change_keeps_chunk_id_but_changes_baked_content_revision() {
    let (arena, root, mut properties, mut generations) =
        prepared_leaf(12, Color::rgb(10, 20, 30), 1.0, false);
    let first = whole_frame_artifact(&arena, &[root], &properties, &generations);

    arena
        .get_mut(root)
        .expect("root exists")
        .element
        .as_any_mut()
        .downcast_mut::<Element>()
        .expect("root is Element")
        .set_opacity(0.4);
    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);
    let second = whole_frame_artifact(&arena, &[root], &properties, &generations);

    assert_eq!(first.chunks[0].id, second.chunks[0].id);
    assert_eq!(
        first.chunks[0].content_revision.self_paint_revision,
        second.chunks[0].content_revision.self_paint_revision,
        "opacity remains excluded from the self-paint signature"
    );
    assert_ne!(
        first.chunks[0].content_revision.composite_revision,
        second.chunks[0].content_revision.composite_revision
    );
    assert_ne!(
        first.chunks[0].content_revision, second.chunks[0].content_revision,
        "opacity is still baked into DrawRectOp in this slice"
    );
}

#[test]
fn resolved_gradient_changes_advance_self_and_content_revision() {
    let mut element = Element::new_with_id(13, 10.25, 20.75, 80.0, 40.0);
    apply_gradient_style(&mut element, "#ff0000", "#0000ff", "#ffffff", "#000000");
    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(element));
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    let (mut properties, mut generations) = sync_identity(&arena, &[root]);
    let first = whole_frame_artifact(&arena, &[root], &properties, &generations);

    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);
    let unchanged = whole_frame_artifact(&arena, &[root], &properties, &generations);
    assert_eq!(first.chunks[0].id, unchanged.chunks[0].id);
    assert_eq!(
        first.chunks[0].content_revision,
        unchanged.chunks[0].content_revision
    );

    {
        let mut node = arena.get_mut(root).expect("root exists");
        let element = node
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .expect("root is Element");
        apply_gradient_style(element, "#00ff00", "#0000ff", "#ffffff", "#000000");
    }
    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);
    let background_changed = whole_frame_artifact(&arena, &[root], &properties, &generations);
    assert_eq!(first.chunks[0].id, background_changed.chunks[0].id);
    assert_ne!(
        first.chunks[0].content_revision.self_paint_revision,
        background_changed.chunks[0]
            .content_revision
            .self_paint_revision
    );

    {
        let mut node = arena.get_mut(root).expect("root exists");
        let element = node
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .expect("root is Element");
        apply_gradient_style(element, "#00ff00", "#0000ff", "#ff00ff", "#000000");
    }
    properties.sync(&arena, &[root]);
    generations.sync(&arena, &[root], &properties);
    let border_changed = whole_frame_artifact(&arena, &[root], &properties, &generations);
    assert_eq!(first.chunks[0].id, border_changed.chunks[0].id);
    assert_ne!(
        background_changed.chunks[0]
            .content_revision
            .self_paint_revision,
        border_changed.chunks[0]
            .content_revision
            .self_paint_revision
    );
    assert_ne!(
        background_changed.chunks[0].content_revision,
        border_changed.chunks[0].content_revision
    );
}
