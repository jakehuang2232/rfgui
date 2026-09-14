use super::*;

#[test]
fn census_reuse_keeps_paint_edits_but_rejects_reordered_or_duplicate_topology() {
    let (mut arena, root, first, second, mut properties, mut generations) =
        prepared_shadow_owner_tree(0xfeed_9910, 0.5);
    let mut recording = RecordingCache::default();
    for frame in 0..4 {
        if frame == 1 {
            let before = recording.topology.replay(&arena, &[root]).unwrap();
            crate::view::test_support::get_element_mut::<Element>(&arena, first)
                .set_background_color(Color::rgb(90, 80, 70));
            let after = recording.topology.replay(&arena, &[root]).unwrap();
            assert!(
                Arc::ptr_eq(&before, &after),
                "paint changes do not rebuild topology"
            );
        }
        if frame == 2 {
            arena.set_children(root, vec![second, first]);
            assert!(recording.topology.replay(&arena, &[root]).is_none());
        }
        if frame == 3 {
            let duplicate_id = arena.get(second).unwrap().element.stable_id();
            *arena.get_mut(first).unwrap().element =
                Box::new(leaf_element(duplicate_id, Color::rgb(1, 2, 3), 1., false));
            assert!(recording.topology.replay(&arena, &[root]).is_none());
        }
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
        let cached = record_surface_dag_frame_artifact_cached(
            &arena,
            &[root],
            &properties,
            &generations,
            &mut recording,
        )
        .unwrap();
        let fresh = record_surface_dag_frame_artifact(
            &arena,
            &[root],
            &properties,
            &generations,
            RendererMode::Auto,
        )
        .unwrap();
        assert_eq!(format!("{cached:?}"), format!("{fresh:?}"), "frame {frame}");
        if frame == 3 {
            assert!(matches!(
                cached,
                FrameArtifactRecordOutcome::WholeFrameLegacyFallback(_)
            ));
        }
    }
}

#[test]
fn census_rejects_lost_history_foreign_arena_removed_root_and_failed_attempt() {
    let (mut arena, root, properties, generations) =
        prepared_leaf(0xfeed_9920, Color::rgb(1, 2, 3), 1., false);
    let mut recording = RecordingCache::default();
    let _ = record_surface_dag_frame_artifact_cached(
        &arena,
        &[root],
        &properties,
        &generations,
        &mut recording,
    )
    .unwrap();
    assert!(recording.topology.replay(&arena, &[root]).is_some());
    let (foreign, foreign_root, _, _) = prepared_leaf(0xfeed_9920, Color::rgb(1, 2, 3), 1., false);
    assert!(
        recording
            .topology
            .replay(&foreign, &[foreign_root])
            .is_none()
    );
    for _ in 0..5000 {
        drop(arena.get_mut(root));
    }
    assert!(recording.topology.replay(&arena, &[root]).is_none());
    let _ = record_surface_dag_frame_artifact_cached(
        &arena,
        &[root],
        &properties,
        &generations,
        &mut recording,
    )
    .unwrap();
    assert!(recording.topology.replay(&arena, &[root]).is_some());
    recording.finish(false);
    assert!(recording.topology.replay(&arena, &[root]).is_none());
    let _ = record_surface_dag_frame_artifact_cached(
        &arena,
        &[root],
        &properties,
        &generations,
        &mut recording,
    )
    .unwrap();
    arena.remove_subtree(root);
    assert!(recording.topology.replay(&arena, &[root]).is_none());
}

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
        )
        .unwrap();
        let fresh = record_surface_dag_frame_artifact(
            &arena,
            &[root],
            &properties,
            &generations,
            RendererMode::Auto,
        )
        .unwrap();
        assert_eq!(format!("{cached:?}"), format!("{fresh:?}"), "frame {frame}");
        assert_eq!(
            matches!(fresh, FrameArtifactRecordOutcome::Artifact { .. }),
            frame != 3
        );
        if frame == 2 {
            assert!(
                recording.subtrees.hits > 0,
                "the witness must exercise warm replay"
            );
        }
    }
}
