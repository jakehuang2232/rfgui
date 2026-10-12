use super::*;

#[test]
fn external_stable_id_alias_cannot_replay_an_inactive_scroll_observation() {
    let mut arena = new_test_arena();
    let stable = 0xfeed_9930;
    let mut element = leaf_element(stable, Color::rgb(1, 2, 3), 1., false);
    let mut style = Style::new();
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(80.)));
    style.insert(PropertyId::Height, ParsedValue::Length(Length::px(60.)));
    style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(crate::style::ScrollDirection::Both),
    );
    element.apply_style(style);
    let root = commit_element(&mut arena, Box::new(element));
    let mut child = leaf_element(stable + 1, Color::rgb(3, 2, 1), 1., false);
    let mut style = Style::new();
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(1.)));
    style.insert(PropertyId::Height, ParsedValue::Length(Length::px(1.)));
    child.apply_style(style);
    let child = commit_child(&mut arena, root, Box::new(child));
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    for key in [root, child] {
        arena.clear_element_dirty_flags(key, DirtyFlags::ALL);
    }
    arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
    arena.refresh_subtree_dirty_cache(root);
    assert!(matches!(
        arena
            .get(root)
            .unwrap()
            .element
            .scroll_geometry_observation(root, &arena),
        crate::view::base_component::ScrollGeometryObservation::Inactive
    ));
    let mut properties = PropertyTrees::default();
    let mut generations = PaintGenerationTracker::default();
    let mut recording = RecordingCache::default();
    let mut alias = None;
    for frame in 0..5 {
        if frame == 3 {
            // This owner is outside the recorded roots. No recorded node or
            // ancestor receives a mutation, but native stable-ID reads change.
            alias = Some(commit_element(
                &mut arena,
                Box::new(leaf_element(stable, Color::rgb(9, 8, 7), 1., false)),
            ));
            assert_eq!(arena.find_by_stable_id(stable), alias);
        }
        if frame == 4 {
            arena.remove_subtree(alias.unwrap());
            arena.refresh_stable_id_index();
            assert_eq!(arena.find_by_stable_id(stable), Some(root));
        }
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
        let cached = record_surface_dag_frame_artifact_cached(
            &arena,
            &[root],
            &properties,
            &generations,
            &mut recording,
        );
        let fresh = record_surface_dag_frame_artifact(&arena, &[root], &properties, &generations);
        assert_eq!(format!("{cached:?}"), format!("{fresh:?}"), "frame {frame}");
        if frame == 2 {
            assert!(
                recording.subtrees.hits > 0,
                "the witness must exercise warm replay"
            );
        }
    }
}
