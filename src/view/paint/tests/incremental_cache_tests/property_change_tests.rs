use super::*;

#[test]
fn planning_changes_survive_dirty_consumption_without_becoming_reuse_authority() {
    let (arena, root, mut properties, mut generations) =
        prepared_leaf(0xfeed_9091, Color::rgb(255, 0, 0), 0.5, false);
    let mut cache = PlanningCache::default();
    let context = ArtifactSurfaceRasterContext::new(
        1.,
        wgpu::TextureFormat::Rgba8Unorm,
        [0., 0.],
        None,
        8192,
        128 * 1024 * 1024,
    )
    .unwrap();
    // Order carries the previous accepted geometry: cold, warm, opacity edit,
    // redundant layout/paint dirty. Dirty is consumed before observing outputs.
    for frame in 0..4 {
        if frame == 2 {
            let mut node = arena.get_mut(root).unwrap();
            node.element
                .as_any_mut()
                .downcast_mut::<Element>()
                .unwrap()
                .set_opacity(0.25);
        }
        if frame == 3 {
            arena.mark_dirty(root, DirtyFlags::ALL);
        }
        arena.clear_arena_dirty(root, DirtyFlags::ALL);
        arena
            .get_mut(root)
            .unwrap()
            .element
            .clear_local_dirty_flags(DirtyFlags::ALL);
        properties.sync(&arena, &[root]);
        generations.sync(&arena, &[root], &properties);
        cache.observe_property_changes(&properties);
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
        let cached =
            prepare_artifact_surface_raster_plan_cached(input.clone(), context, &mut cache)
                .unwrap();
        let fresh = prepare_artifact_surface_raster_plan(input, context).unwrap();
        assert_eq!(format!("{cached:?}"), format!("{fresh:?}"), "frame {frame}");
        assert_eq!(
            cache.geometry_changed_this_frame(),
            frame == 2,
            "frame {frame}"
        );
        assert_eq!(
            cache.geometry_hits,
            usize::from(frame == 1 || frame == 3),
            "frame {frame}"
        );
    }
    // A caller may omit change hints or corrupt an input after observation.
    // Neither empty dirty nor an unchanged property observation certifies it.
    let mut invalid = artifact(
        record_surface_dag_frame_artifact(
            &arena,
            &[root],
            &properties,
            &generations,
            RendererMode::Auto,
        )
        .unwrap(),
    );
    invalid.effect_nodes[0].generation = 0;
    assert!(prepare_artifact_surface_raster_plan_cached(invalid, context, &mut cache).is_err());
}
