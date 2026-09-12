use super::*;

#[test]
fn changed_command_geometry_reuses_relations_but_validates_current_bounds() {
    let (mut arena, root, _, _) = prepared_leaf(0xfeed_9092, Color::rgb(255, 0, 0), 0.5, false);
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
    let mut previous_bounds = None;
    for frame in 0..3 {
        let (measure, mut place) = constraints();
        place.parent_x = frame as f32 * 9.0;
        measure_and_place(&mut arena, root, measure, place);
        let (properties, generations) = sync_identity(&arena, &[root]);
        let input = artifact(
            record_surface_dag_frame_artifact(
                &arena,
                &[root],
                &properties,
                &generations,
                RendererMode::Auto,
            )
            .unwrap(),
        );
        let bounds = input.chunks[0].bounds.x.to_bits();
        if let Some(previous) = previous_bounds {
            assert_ne!(
                bounds, previous,
                "fixture must actually move recorded geometry"
            );
        }
        previous_bounds = Some(bounds);
        let cached =
            prepare_artifact_surface_raster_plan_cached(input.clone(), context, &mut cache)
                .unwrap();
        let fresh = prepare_artifact_surface_raster_plan(input.clone(), context).unwrap();
        assert_eq!(format!("{cached:?}"), format!("{fresh:?}"), "frame {frame}");
        assert_eq!(cache.relation_hits, usize::from(frame != 0));
        assert_eq!(
            cache.geometry_hits, 0,
            "new bounds still reach geometry planning"
        );
        if frame == 2 {
            // A matching relationship key must not hide malformed current
            // geometry. This is rejected before relation replay can authorize it.
            let mut malformed = input;
            malformed.chunks[0].bounds.x = f32::NAN;
            assert!(prepare_artifact_surface_raster_plan(malformed.clone(), context).is_err());
            assert!(
                prepare_artifact_surface_raster_plan_cached(malformed, context, &mut cache)
                    .is_err()
            );
            assert_eq!(cache.relation_hits, 0);
        }
    }
}

#[test]
fn ancestor_effect_updates_preserve_current_scope_store_and_recording() {
    let (arena, roots, _) = prepared_plain_tree();
    let mut cache = RecordingCache::default();
    for (frame, opacity) in [0.5, 0.25, 0.75].into_iter().enumerate() {
        arena
            .get_mut(roots[0])
            .unwrap()
            .element
            .as_any_mut()
            .downcast_mut::<Element>()
            .unwrap()
            .set_opacity(opacity);
        let (properties, generations) = sync_identity(&arena, &roots);
        let cached = artifact(
            record_surface_dag_frame_artifact_cached(
                &arena,
                &roots,
                &properties,
                &generations,
                &mut cache,
            )
            .unwrap(),
        );
        let fresh = artifact(
            record_surface_dag_frame_artifact(
                &arena,
                &roots,
                &properties,
                &generations,
                RendererMode::Auto,
            )
            .unwrap(),
        );
        assert_eq!(format!("{cached:?}"), format!("{fresh:?}"), "frame {frame}");
        assert_eq!(cache.scope_store_hits, usize::from(frame != 0));
        assert_eq!(cache.scope_store_effect_updates, usize::from(frame != 0));
    }
}
