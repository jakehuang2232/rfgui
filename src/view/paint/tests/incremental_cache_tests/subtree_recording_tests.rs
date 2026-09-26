use super::*;

#[test]
fn continuously_changing_subtree_rejoins_cache_after_its_inputs_settle() {
    let (arena, root, first, _, mut properties, mut generations) =
        prepared_shadow_owner_tree(0xfeed_9820, 0.5);
    let mut cache = RecordingCache::default();
    for frame in 0..12 {
        if (1..=5).contains(&frame) {
            crate::view::test_support::get_element_mut::<Element>(&arena, first)
                .set_background_color(Color::rgb(frame * 20, 27, 98));
        }
        if frame == 6 {
            // The last mutation also changes inherited recording inputs.
            // Settling the local revision must not revive the earlier context.
            crate::view::test_support::get_element_mut::<Element>(&arena, root).set_opacity(0.75);
        }
        if frame == 10 {
            cache.finish(false);
        }
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
        let cached = record_surface_dag_frame_artifact_cached(
            &arena,
            &[root],
            &properties,
            &generations,
            &mut cache,
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
        if [9, 11].contains(&frame) {
            assert!(cache.subtrees.hits > 0, "settled frame {frame}");
        }
        if frame == 10 {
            assert_eq!(cache.subtrees.hits, 0, "failed attempt discards proofs");
        }
    }
}

#[test]
fn exact_ancestor_recordings_allow_noop_mutation_but_preserve_changed_effects() {
    let (arena, root, _, _, mut properties, mut generations) =
        prepared_shadow_owner_tree(0xfeed_9816, 0.5);
    let mut cache = RecordingCache::default();
    for frame in 0..8 {
        if frame > 0 {
            drop(arena.get_mut(root));
        }
        if frame == 4 {
            crate::view::test_support::get_element_mut::<Element>(&arena, root).set_opacity(0.75);
        }
        if frame == 7 {
            cache.finish(false);
        }
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
        let cached = record_surface_dag_frame_artifact_cached(
            &arena,
            &[root],
            &properties,
            &generations,
            &mut cache,
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
        if [2, 3, 6].contains(&frame) {
            assert!(
                cache.subtrees.hits >= 2,
                "unchanged native child recordings, frame {frame}"
            );
        }
        if [4, 7].contains(&frame) {
            assert_eq!(
                cache.subtrees.hits, 0,
                "changed effect or failed attempt, frame {frame}"
            );
        }
    }
}

#[test]
fn changing_branch_keeps_replaying_unchanged_siblings() {
    let (arena, root, first, _, mut properties, mut generations) =
        prepared_shadow_owner_tree(0xfeed_9812, 0.5);
    let mut cache = RecordingCache::default();
    for frame in 0..6 {
        if frame > 0 {
            arena
                .get_mut(first)
                .unwrap()
                .element
                .as_any_mut()
                .downcast_mut::<Element>()
                .unwrap()
                .set_background_color(Color::rgb(frame * 10, 27, 98));
            arena.clear_element_dirty_flags(first, DirtyFlags::ALL);
        }
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
        let cached = record_surface_dag_frame_artifact_cached(
            &arena,
            &[root],
            &properties,
            &generations,
            &mut cache,
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
        if frame >= 2 {
            assert!(
                cache.subtrees.hits > 0,
                "changing ancestors must not starve stable children"
            );
        }
    }
}

#[test]
fn retained_subtree_recording_matches_fresh_after_local_edits_and_snapshot_damage() {
    let (arena, root, first, _, mut properties, mut generations) =
        prepared_shadow_owner_tree(0xfeed_9811, 0.5);
    let mut cache = RecordingCache::default();
    for frame in 0..9 {
        if frame == 2 {
            arena
                .get_mut(first)
                .unwrap()
                .element
                .as_any_mut()
                .downcast_mut::<Element>()
                .unwrap()
                .set_background_color(Color::rgb(14, 27, 98));
            arena.clear_element_dirty_flags(first, DirtyFlags::ALL);
        }
        if frame == 6 {
            // Re-observe the owning native storage after damaged derived state.
            arena
                .get_mut(root)
                .unwrap()
                .element
                .as_any_mut()
                .downcast_mut::<Element>()
                .unwrap()
                .set_opacity(0.5);
        }
        if frame != 5 {
            properties.sync(&arena, &[root]);
            generations.sync(&arena, &[root], &properties);
        } else {
            // No node mutation and no generation bump. Snapshot equality must
            // still reject a formerly valid subtree when an endpoint disappears.
            let id = properties
                .node_state_for(root)
                .unwrap()
                .paint
                .effect
                .unwrap();
            properties.effects.remove(&id);
        }
        if frame == 8 {
            cache.finish(false);
        }
        let cached = record_surface_dag_frame_artifact_cached(
            &arena,
            &[root],
            &properties,
            &generations,
            &mut cache,
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
        // After a local edit, one settled frame coalesces the subtree before replay.
        if [1, 4, 7].contains(&frame) {
            assert!(cache.subtrees.hits > 0, "frame {frame}");
        }
        if frame == 5 {
            assert!(matches!(
                cached,
                FrameArtifactRecordOutcome::WholeFrameLegacyFallback(_)
            ));
        }
        if frame >= 6 {
            assert!(matches!(
                cached,
                FrameArtifactRecordOutcome::Artifact { .. }
            ));
        }
        if frame == 8 {
            assert_eq!(cache.subtrees.hits, 0);
        }
    }
}

#[test]
fn recorded_commands_share_immutable_storage_and_detach_before_corruption() {
    let (arena, root, _, _, properties, generations) = prepared_shadow_owner_tree(0xfeed_9813, 0.5);
    let mut cache = RecordingCache::default();
    let mut record = || {
        artifact(
            record_surface_dag_frame_artifact_cached(
                &arena,
                &[root],
                &properties,
                &generations,
                &mut cache,
            )
            .unwrap(),
        )
    };
    // First materialization installs the shared command blocks. Subsequent
    // frames retain those same bytes after the complete recording proof.
    let _ = record();
    let first = record();
    let mut second = record();
    assert_eq!(first.chunks.len(), second.chunks.len());
    for (a, b) in first.chunks.iter().zip(&second.chunks) {
        let a = &first.ops[a.op_range.clone()];
        let b = &second.ops[b.op_range.clone()];
        assert!(
            std::ptr::eq(a, b),
            "warm command blocks must retain their allocation"
        );
    }
    let first_before = format!("{first:?}");
    second.ops.clear();
    assert_eq!(format!("{first:?}"), first_before);
    let context = ArtifactSurfaceRasterContext::new(
        1.,
        wgpu::TextureFormat::Rgba8Unorm,
        [0., 0.],
        None,
        8192,
        128 * 1024 * 1024,
    )
    .unwrap();
    let mut planning = PlanningCache::default();
    assert!(
        prepare_artifact_surface_raster_plan_cached(first.clone(), context, &mut planning).is_ok()
    );
    assert!(prepare_artifact_surface_raster_plan_cached(second, context, &mut planning).is_err());
    assert!(prepare_artifact_surface_raster_plan_cached(first, context, &mut planning).is_ok());
}

#[test]
fn scroll_subtree_replay_matches_live_recording_after_offset_changes() {
    let mut arena = new_test_arena();
    let mut scroll = leaf_element(0xfeed_9814, Color::rgb(10, 20, 30), 1., false);
    let mut style = Style::new();
    style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(crate::style::ScrollDirection::Both),
    );
    scroll.apply_style(style);
    let root = commit_element(&mut arena, Box::new(scroll));
    let mut child = leaf_element(0xfeed_9815, Color::rgb(80, 100, 120), 1., false);
    let mut style = Style::new();
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(500.)));
    style.insert(PropertyId::Height, ParsedValue::Length(Length::px(500.)));
    child.apply_style(style);
    let child = commit_child(&mut arena, root, Box::new(child));
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    for key in [root, child] {
        arena.clear_element_dirty_flags(key, DirtyFlags::ALL);
    }
    arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
    arena.refresh_subtree_dirty_cache(root);
    let mut properties = PropertyTrees::default();
    let mut generations = PaintGenerationTracker::default();
    let mut cache = RecordingCache::default();
    for frame in 0..7 {
        if frame == 2 {
            crate::view::test_support::get_element_mut::<Element>(&arena, root)
                .set_scroll_offset((5., 7.));
            let (measure, place) = constraints();
            measure_and_place(&mut arena, root, measure, place);
            for key in [root, child] {
                arena.clear_element_dirty_flags(key, DirtyFlags::ALL);
            }
            arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
            arena.refresh_subtree_dirty_cache(root);
        }
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
        assert!(
            properties
                .scroll_snapshot_for(crate::view::compositor::property_tree::ScrollNodeId(root))
                .is_some(),
            "frame {frame}: {:?} {:?}",
            properties.validation_errors,
            arena
                .get(root)
                .unwrap()
                .element
                .scroll_geometry_observation(root, &arena)
        );
        let cached = record_surface_dag_frame_artifact_cached(
            &arena,
            &[root],
            &properties,
            &generations,
            &mut cache,
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
        assert!(matches!(
            cached,
            FrameArtifactRecordOutcome::Artifact { .. }
        ));
        if frame == 1 || frame == 6 {
            assert!(cache.subtrees.hits > 0);
        }
    }
}

#[test]
fn lazy_owner_inputs_preserve_deferred_descendant_recording_and_reentry() {
    let (mut arena, root, first, second, mut properties, mut generations) =
        prepared_shadow_owner_tree(0xfeed_9818, 0.5);
    let mut style = Style::new();
    style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .left(Length::px(0.))
                .top(Length::px(0.))
                .clip(ClipMode::Viewport),
        ),
    );
    crate::view::test_support::get_element_mut::<Element>(&arena, second).apply_style(style);
    let (measure, place) = constraints();
    measure_and_place(&mut arena, root, measure, place);
    assert!(
        arena
            .get(second)
            .unwrap()
            .element
            .is_deferred_to_root_viewport_render()
    );
    let mut cache = RecordingCache::default();
    let mut warm_hits = 0;
    for frame in 0..7 {
        if frame == 3 {
            crate::view::test_support::get_element_mut::<Element>(&arena, first).set_opacity(0.75);
        }
        if frame == 4 {
            crate::view::test_support::get_element_mut::<Element>(&arena, second)
                .set_background_color(Color::rgb(12, 34, 56));
        }
        if frame == 5 {
            cache.finish(false);
        }
        properties.sync(&arena, &[root]);
        generations.sync_arena(&arena, &[root], &properties);
        let cached = record_surface_dag_frame_artifact_cached(
            &arena,
            &[root],
            &properties,
            &generations,
            &mut cache,
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
        assert!(matches!(
            cached,
            FrameArtifactRecordOutcome::Artifact { .. }
        ));
        warm_hits += cache.subtrees.hits;
    }
    assert!(warm_hits > 0);
}
