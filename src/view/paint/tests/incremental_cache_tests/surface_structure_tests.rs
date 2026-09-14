use super::*;
use crate::style::Scale;
use crate::view::test_support::get_element_mut;

#[test]
fn structural_replay_recomputes_transfer_and_target_elimination_from_current_values() {
    let mut arena = new_test_arena();
    // Co-located transform/effect boundaries put raster work inside the
    // effect, leaving the transform eligible for transfer when it is a translation.
    let root = commit_element(
        &mut arena,
        Box::new(leaf_element(
            0xfeed_9920,
            Color::rgb(180, 40, 20),
            0.5,
            false,
        )),
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
    let mut latest = None;
    for frame in 0..3 {
        get_element_mut::<Element>(&arena, root).set_transform_value(if frame == 1 {
            Transform::new([Scale::uniform(1.25)])
        } else {
            Transform::new([Translate::xy(Length::px(3. + frame as f32), Length::px(4.))])
        });
        get_element_mut::<Element>(&arena, root).set_opacity(0.5 + frame as f32 * 0.1);
        let (measure, place) = constraints();
        measure_and_place(&mut arena, root, measure, place);
        let (properties, generations) = sync_identity(&arena, &[root]);
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
        let fresh = prepare_artifact_surface_raster_plan(source.clone(), context).unwrap();
        let cached =
            prepare_artifact_surface_raster_plan_cached(source.clone(), context, &mut cache)
                .unwrap();
        assert_eq!(format!("{cached:?}"), format!("{fresh:?}"), "frame {frame}");
        assert_eq!(cache.surface_structure_hits, usize::from(frame != 0));
        assert_eq!(
            cached.nodes().len(),
            if frame == 1 { 2 } else { 1 },
            "frame {frame}: {:?}",
            cached.materialization_decisions()
        );
        latest = Some(source);
    }
    let latest = latest.unwrap();
    let mut invalid = latest.clone();
    invalid.transform_nodes[0].local_generation = 0;
    let fresh = prepare_artifact_surface_raster_plan(invalid.clone(), context).unwrap_err();
    let cached =
        prepare_artifact_surface_raster_plan_cached(invalid, context, &mut cache).unwrap_err();
    assert_eq!(format!("{cached:?}"), format!("{fresh:?}"));
    let recovered =
        prepare_artifact_surface_raster_plan_cached(latest.clone(), context, &mut cache).unwrap();
    assert_eq!(
        cache.surface_structure_hits, 0,
        "a failed frame discarded the proof"
    );
    assert_eq!(
        format!("{recovered:?}"),
        format!(
            "{:?}",
            prepare_artifact_surface_raster_plan(latest, context).unwrap()
        )
    );
}
