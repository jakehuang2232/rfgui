use super::*;

#[test]
fn replay_preserves_full_recording_and_planning_across_content_and_geometry_edits() {
    let (arena, root, mut properties, mut generations) =
        prepared_leaf(0xfeed_9001, Color::rgb(255, 0, 0), 0.5, false);
    let mut recording = RecordingCache::default();
    let mut planning = PlanningCache::default();
    let context = ArtifactSurfaceRasterContext::new(
        1.,
        wgpu::TextureFormat::Rgba8Unorm,
        [0., 0.],
        None,
        [4096, 4096],
        8192,
        128 * 1024 * 1024,
    )
    .unwrap();
    // Ordered sequence: cold, unchanged, color only, unchanged, opacity only,
    // then explicit cache release and reconstruction of the same content.
    for frame in 0..6 {
        if frame == 2 {
            let mut node = arena.get_mut(root).unwrap();
            let element = node.element.as_any_mut().downcast_mut::<Element>().unwrap();
            let mut style = Style::new();
            style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
            style.insert(
                PropertyId::BackgroundColor,
                ParsedValue::color_like(Color::rgb(0, 0, 255)),
            );
            element.apply_style(style);
            element.set_opacity(0.5);
        }
        if frame == 4 {
            arena
                .get_mut(root)
                .unwrap()
                .element
                .as_any_mut()
                .downcast_mut::<Element>()
                .unwrap()
                .set_opacity(0.25);
        }
        if frame == 5 {
            recording.finish(false);
            planning.finish(false);
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
        // Full structured debug includes all fields and operations; this fixture
        // has deterministic native rectangle payloads, no pointer debug values.
        assert_eq!(
            format!("{cached:?}"),
            format!("{fresh:?}"),
            "recording frame {frame}"
        );
        let cached =
            prepare_artifact_surface_raster_plan_cached(cached, context, &mut planning).unwrap();
        let fresh = prepare_artifact_surface_raster_plan(fresh, context).unwrap();
        assert_eq!(
            format!("{cached:?}"),
            format!("{fresh:?}"),
            "planning frame {frame}"
        );
        assert_eq!(
            seal_prepared_artifact_surface_frame(cached)
                .unwrap()
                .residents(),
            seal_prepared_artifact_surface_frame(fresh)
                .unwrap()
                .residents()
        );
        if frame == 4 {
            assert_eq!(
                recording.scope_store_hits, 1,
                "stable scope topology patches the current effect value"
            );
            assert_eq!(recording.scope_store_effect_updates, 1);
            assert_eq!(
                planning.geometry_hits, 0,
                "opacity rebuilds current materialization"
            );
            assert_eq!(
                planning.coverage_hits, 1,
                "opacity preserves chunk membership"
            );
            assert_eq!(
                planning.graph_hits, 1,
                "opacity preserves validated parent graphs"
            );
            assert_eq!(
                planning.placement_hits, 1,
                "opacity preserves host placement"
            );
        }
        if frame == 1 || frame == 3 {
            assert_eq!(
                recording.scope_hits, 1,
                "unchanged live scopes share immutable storage"
            );
            assert_eq!(
                recording.scope_store_hits, 1,
                "unchanged observations replay the ordered snapshot store"
            );
            assert_eq!(recording.hits, 1);
            assert_eq!(recording.misses, 0);
            assert_eq!(planning.geometry_hits, 1);
            assert!(planning.localized_hits > 0 || planning.raster_span_hits() > 0);
            assert_eq!(planning.localized_misses, 0);
        } else {
            assert_eq!(
                recording.hits, 0,
                "changed/released recording must not replay"
            );
            assert!(planning.localized_misses > 0);
        }
    }
}

#[test]
fn valid_unknown_host_always_runs_its_full_hook() {
    let (arena, root, records, properties, generations) = malformed_host(MalformedChunk::Valid);
    let mut cache = RecordingCache::default();
    for frame in 1..=3 {
        let _ = record_surface_dag_frame_artifact_cached(
            &arena,
            &[root],
            &properties,
            &generations,
            &mut cache,
        );
        assert_eq!(records.load(Ordering::Relaxed), frame);
        assert_eq!(
            cache.hits, 0,
            "exact metadata alone cannot authorize a custom host"
        );
    }
}

#[test]
fn absent_owners_release_recorded_commands_before_reentry() {
    let (arena, root, properties, generations) =
        prepared_leaf(0xfeed_9002, Color::rgb(255, 0, 0), 0.5, false);
    let mut cache = RecordingCache::default();
    for roots in [&[root][..], &[root][..], &[][..], &[root][..]] {
        let _ = record_surface_dag_frame_artifact_cached(
            &arena,
            roots,
            &properties,
            &generations,
            &mut cache,
        );
    }
    assert_eq!(cache.hits, 0, "reentry cannot replay an evicted owner");
    assert_eq!(cache.misses, 1);
    assert_eq!(
        cache.scope_hits, 0,
        "reentry cannot retain an absent owner's scope either"
    );
    assert_eq!(
        cache.scope_store_hits, 0,
        "empty-frame store must replace the prior scope key"
    );
}

#[test]
fn large_optional_payload_does_not_set_every_command_storage_size() {
    use std::mem::size_of;
    eprintln!(
        "command bytes={} scrollbar payload={} inline payload={} identity={}",
        size_of::<PaintOp>(),
        size_of::<PreparedScrollbarOverlayOp>(),
        size_of::<PreparedInlineIfcDecorationOp>(),
        size_of::<PaintPayloadIdentity>()
    );
    eprintln!(
        "recording context bytes={}",
        size_of::<PaintRecordingContext>()
    );
    assert!(size_of::<PaintOp>() < size_of::<PreparedScrollbarOverlayOp>());
    assert!(size_of::<PaintPayloadIdentity>() < size_of::<PreparedScrollbarOverlayIdentity>());
}

#[test]
fn unknown_transparent_hosts_keep_the_second_capability_walk() {
    let (arena, root, _, properties, generations) = malformed_host(MalformedChunk::Transparent);
    let mut cache = RecordingCache::default();
    for frame in 1..=2 {
        let _ = record_surface_dag_frame_artifact_cached(
            &arena,
            &[root],
            &properties,
            &generations,
            &mut cache,
        );
        let node = arena.get(root).unwrap();
        let host = node
            .element
            .as_any()
            .downcast_ref::<MalformedRecordingHost>()
            .unwrap();
        assert_eq!(
            host.capability_calls.load(Ordering::Relaxed),
            frame * 2,
            "no-chunk custom hosts still execute their original two-pass contract"
        );
    }
}

#[test]
fn unknown_hooks_disable_native_replay_during_the_second_walk() {
    let (mut arena, native, _, _) = prepared_leaf(0xfeed_9004, Color::rgb(255, 0, 0), 0.5, false);
    let unknown = commit_element(
        &mut arena,
        Box::new(MalformedRecordingHost {
            id: 45,
            malformed: MalformedChunk::Transparent,
            capability_calls: Arc::new(AtomicUsize::new(0)),
            full_records: Arc::new(AtomicUsize::new(0)),
        }),
    );
    let roots = [native, unknown];
    let (properties, generations) = sync_identity(&arena, &roots);
    let mut cache = RecordingCache::default();
    for _ in 0..3 {
        let _ = record_surface_dag_frame_artifact_cached(
            &arena,
            &roots,
            &properties,
            &generations,
            &mut cache,
        );
        assert_eq!(
            cache.hits, 0,
            "full walk must not substitute a preflight context for a current hook context"
        );
    }
}

#[test]
fn local_content_edit_reuses_unaffected_raster_span_and_matches_fresh_planning() {
    let (arena, roots, _) = prepared_plain_tree();
    let (mut properties, mut generations) = sync_identity(&arena, &roots);
    let context = ArtifactSurfaceRasterContext::new(
        1.,
        wgpu::TextureFormat::Rgba8Unorm,
        [0., 0.],
        None,
        [4096, 4096],
        8192,
        128 * 1024 * 1024,
    )
    .unwrap();
    let mut cache = PlanningCache::default();
    for frame in 0..3 {
        if frame == 2 {
            crate::view::test_support::get_element_mut::<Element>(&arena, roots[0])
                .set_background_color(Color::rgb(123, 45, 67));
            properties.sync(&arena, &roots);
            generations.sync(&arena, &roots, &properties);
        }
        let source = record_surface_dag_frame_artifact(&arena, &roots, &properties, &generations);
        let fresh = prepare_artifact_surface_raster_plan(source.clone(), context).unwrap();
        let cached =
            prepare_artifact_surface_raster_plan_cached(source, context, &mut cache).unwrap();
        assert_eq!(format!("{cached:?}"), format!("{fresh:?}"), "frame {frame}");
        if frame == 1 {
            assert_eq!(cache.raster_span_hits(), 2);
        }
        if frame == 2 {
            assert_eq!(
                cache.raster_span_hits(),
                1,
                "only the edited root span must rebuild"
            );
            assert!(
                cache.localized_hits > 0,
                "unchanged child in the edited span keeps its localized ops"
            );
            assert_eq!(cache.localized_misses, 1);
        }
    }
}

#[test]
fn parent_graph_replay_uses_live_spatial_and_clip_values() {
    let mut arena = new_test_arena();
    let make = |id, width, height, scroll| {
        let mut element = Element::new_with_id(id, 0.0, 0.0, width, height);
        let mut style = Style::new();
        style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
        style.insert(PropertyId::Width, ParsedValue::Length(Length::px(width)));
        style.insert(PropertyId::Height, ParsedValue::Length(Length::px(height)));
        style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::rgb(21, 43, 65)),
        );
        if scroll {
            style.insert(
                PropertyId::ScrollDirection,
                ParsedValue::ScrollDirection(ScrollDirection::Vertical),
            );
        }
        element.apply_style(style);
        element
    };
    let root = commit_element(&mut arena, Box::new(make(0xfeed_9081, 40.0, 32.0, true)));
    commit_child(
        &mut arena,
        root,
        Box::new(make(0xfeed_9082, 20.0, 120.0, false)),
    );
    let mut viewport = crate::view::viewport::Viewport::new();
    crate::view::viewport::layout_artifact_style_scene_for_test(
        &mut viewport,
        &mut arena,
        root,
        [320.0, 240.0],
    );
    let (properties, generations) = sync_identity(&arena, &[root]);
    let baseline = record_surface_dag_frame_artifact(&arena, &[root], &properties, &generations);
    assert!(!baseline.layout_position_nodes.is_empty());
    assert!(!baseline.visual_offset_nodes.is_empty());
    assert!(!baseline.scroll_nodes.is_empty());
    assert!(!baseline.clip_nodes.is_empty());
    let context = ArtifactSurfaceRasterContext::new(
        1.,
        wgpu::TextureFormat::Rgba8Unorm,
        [0., 0.],
        None,
        [4096, 4096],
        8192,
        128 * 1024 * 1024,
    )
    .unwrap();
    for field in 0..4 {
        for valid in [true, false] {
            let mut cache = PlanningCache::default();
            prepare_artifact_surface_raster_plan_cached(baseline.clone(), context, &mut cache)
                .unwrap();
            let mut changed = baseline.clone();
            match (field, valid) {
                (0, true) => {
                    changed.layout_position_nodes[0]
                        .translation_at_scroll_zero
                        .x += 2.0
                }
                (0, false) => {
                    changed.layout_position_nodes[0]
                        .translation_at_scroll_zero
                        .x = f32::NAN
                }
                (1, true) => changed.visual_offset_nodes[0].offset.y += 3.0,
                (1, false) => changed.visual_offset_nodes[0].generation = 0,
                (2, true) => changed.scroll_nodes[0].offset.y += 4.0,
                (2, false) => changed.scroll_nodes[0].offset.y = f32::INFINITY,
                (3, true) => changed.clip_nodes[0].generation += 1,
                (3, false) => changed.clip_nodes[0].generation = 0,
                _ => unreachable!(),
            }
            let fresh = prepare_artifact_surface_raster_plan(changed.clone(), context);
            let cached = prepare_artifact_surface_raster_plan_cached(changed, context, &mut cache);
            assert_eq!(
                format!("{cached:?}"),
                format!("{fresh:?}"),
                "field={field} valid={valid}"
            );
            assert_eq!(cached.is_ok(), valid, "field={field}");
            if valid {
                assert_eq!(
                    cache.graph_hits, 1,
                    "must reuse parent graphs for field={field}"
                );
            }
        }
    }
}

#[test]
fn property_closure_replay_reads_live_values() {
    use crate::view::compositor::property_tree::LayoutPositionNodeId;
    for change in 0..3 {
        let (arena, root, mut trees, generations) =
            prepared_leaf(0xfeed_90a0 + change, Color::rgb(17, 43, 61), 0.5, false);
        let mut cache = RecordingCache::default();
        record_surface_dag_frame_artifact_cached(&arena, &[root], &trees, &generations, &mut cache);
        let position = LayoutPositionNodeId(root);
        assert!(trees.layout_positions.contains_key(&position));
        match change {
            0 => {
                trees
                    .layout_positions
                    .get_mut(&position)
                    .unwrap()
                    .translation_at_scroll_zero
                    .x += 3.0
            }
            1 => {
                trees
                    .visual_offsets
                    .values_mut()
                    .next()
                    .expect("visual fixture")
                    .offset
                    .y += 4.0
            }
            2 => {
                trees
                    .effects
                    .values_mut()
                    .next()
                    .expect("effect fixture")
                    .opacity = 0.25
            }
            _ => unreachable!(),
        }
        let cached = record_surface_dag_frame_artifact_cached(
            &arena,
            &[root],
            &trees,
            &generations,
            &mut cache,
        );
        let fresh = record_surface_dag_frame_artifact(&arena, &[root], &trees, &generations);
        assert_eq!(
            format!("{cached:?}"),
            format!("{fresh:?}"),
            "change {change}"
        );
        assert_eq!(
            cache.property_closure_hits, 1,
            "values are fetched through the prior closure"
        );
    }
}

mod property_change_tests;
mod span_dependency_tests;

mod relation_dependency_tests;
mod scaling_tests;
mod native_observation_tests;
mod subtree_recording_tests;
mod topology_history_tests;

mod surface_structure_tests;

mod clip_value_refresh_tests;
