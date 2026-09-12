use super::*;

fn artifact(outcome: FrameArtifactRecordOutcome) -> PaintArtifact {
    match outcome {
        FrameArtifactRecordOutcome::Artifact { artifact, .. } => artifact,
        other => panic!("expected artifact: {other:?}"),
    }
}

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
        let cached = artifact(
            record_surface_dag_frame_artifact_cached(
                &arena,
                &[root],
                &properties,
                &generations,
                &mut recording,
            )
            .unwrap(),
        );
        let fresh = artifact(
            record_surface_dag_frame_artifact(
                &arena,
                &[root],
                &properties,
                &generations,
                RendererMode::Auto,
            )
            .unwrap(),
        );
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
        if frame == 1 || frame == 3 {
            assert_eq!(recording.hits, 1);
            assert_eq!(recording.misses, 0);
            assert_eq!(planning.geometry_hits, 1);
            assert!(planning.localized_hits > 0);
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
fn invalid_metadata_still_rejects_before_full_recording_with_cache() {
    let (arena, root, records, properties, generations) =
        malformed_host(MalformedChunk::MetadataProperties);
    let mut cache = RecordingCache::default();
    for _ in 0..2 {
        let outcome = record_surface_dag_frame_artifact_cached(
            &arena,
            &[root],
            &properties,
            &generations,
            &mut cache,
        )
        .unwrap();
        assert!(matches!(
            outcome,
            FrameArtifactRecordOutcome::WholeFrameLegacyFallback(_)
        ));
        assert_eq!(cache.hits, 0);
        assert_eq!(
            records.load(Ordering::Relaxed),
            0,
            "preflight rejection precedes full hook"
        );
    }
}

#[test]
fn valid_unknown_host_always_runs_its_full_hook() {
    let (arena, root, records, properties, generations) = malformed_host(MalformedChunk::Valid);
    let mut cache = RecordingCache::default();
    for frame in 1..=3 {
        let _ = artifact(
            record_surface_dag_frame_artifact_cached(
                &arena,
                &[root],
                &properties,
                &generations,
                &mut cache,
            )
            .unwrap(),
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
        let _ = artifact(
            record_surface_dag_frame_artifact_cached(
                &arena,
                roots,
                &properties,
                &generations,
                &mut cache,
            )
            .unwrap(),
        );
    }
    assert_eq!(cache.hits, 0, "reentry cannot replay an evicted owner");
    assert_eq!(cache.misses, 1);
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
fn warm_geometry_never_skips_current_command_identity_validation() {
    let (arena, root, properties, generations) =
        prepared_leaf(0xfeed_9003, Color::rgb(255, 0, 0), 0.5, false);
    let source = artifact(
        record_surface_dag_frame_artifact(
            &arena,
            &[root],
            &properties,
            &generations,
            RendererMode::Auto,
        )
        .unwrap(),
    );
    let context = ArtifactSurfaceRasterContext::new(
        1.,
        wgpu::TextureFormat::Rgba8Unorm,
        [0., 0.],
        None,
        8192,
        128 * 1024 * 1024,
    )
    .unwrap();
    let mut cache = PlanningCache::default();
    prepare_artifact_surface_raster_plan_cached(source.clone(), context, &mut cache).unwrap();
    let mut changed = source;
    let PaintOp::DrawRect(rect) = &mut changed.ops[0] else {
        panic!("rectangle fixture");
    };
    rect.params.fill_color[0] = 0.99;
    assert!(
        prepare_artifact_surface_raster_plan_cached(changed, context, &mut cache).is_err(),
        "unchanged geometry and stale payload identity cannot certify changed commands"
    );
}

#[test]
fn unknown_transparent_hosts_keep_the_second_capability_walk() {
    let (arena, root, _, properties, generations) = malformed_host(MalformedChunk::Transparent);
    let mut cache = RecordingCache::default();
    for frame in 1..=2 {
        let _ = artifact(
            record_surface_dag_frame_artifact_cached(
                &arena,
                &[root],
                &properties,
                &generations,
                &mut cache,
            )
            .unwrap(),
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
    let (mut arena, native, _, _) = prepared_leaf(0xfeed_9004, Color::rgb(255,0,0), 0.5, false);
    let unknown = commit_element(&mut arena, Box::new(MalformedRecordingHost {
        id: 45, malformed: MalformedChunk::Transparent,
        capability_calls: Arc::new(AtomicUsize::new(0)),
        full_records: Arc::new(AtomicUsize::new(0)),
    }));
    let roots = [native, unknown];
    let (properties, generations) = sync_identity(&arena, &roots);
    let mut cache = RecordingCache::default();
    for _ in 0..3 {
        let _ = artifact(record_surface_dag_frame_artifact_cached(&arena, &roots, &properties, &generations, &mut cache).unwrap());
        assert_eq!(cache.hits, 0, "full walk must not substitute a preflight context for a current hook context");
    }
}
