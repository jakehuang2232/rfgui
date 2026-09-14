use super::*;
use crate::view::test_support::get_element_mut;

/// Timing evidence, not a portable wall-time assertion. Run alone and compare
/// repeated runs; the ordinary equality tests remain the correctness gate.
#[test]
#[ignore = "CPU scaling benchmark; run alone with --nocapture"]
fn incremental_recording_and_planning_cost_with_unrelated_static_nodes() {
    for count in [20, 200, 2000] {
        let mut arena = new_test_arena();
        let root = commit_element(
            &mut arena,
            Box::new(leaf_element(0x10000, Color::rgb(0, 0, 0), 1., false)),
        );
        let active = commit_element(
            &mut arena,
            Box::new(leaf_element(0x10001, Color::rgb(200, 0, 0), 0.5, false)),
        );
        for index in 0..count {
            let mut child = leaf_element(0x20000 + index, Color::rgb(0, 120, 30), 1., false);
            let mut style = Style::new();
            style.insert(
                PropertyId::Position,
                ParsedValue::Position(
                    Position::absolute()
                        .left(Length::px(0.))
                        .top(Length::px(0.)),
                ),
            );
            child.apply_style(style);
            commit_child(&mut arena, root, Box::new(child));
        }
        for key in [root, active] {
            let (measure, place) = constraints();
            measure_and_place(&mut arena, key, measure, place);
        }
        let mut properties = PropertyTrees::default();
        let mut generations = PaintGenerationTracker::default();
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
        let mut samples = Vec::new();
        for frame in 0..330 {
            get_element_mut::<Element>(&arena, active).set_opacity(if frame % 2 == 0 {
                0.5
            } else {
                0.75
            });
            let start = crate::time::Instant::now();
            properties.sync(&arena, &[root, active]);
            generations.sync_arena(&arena, &[root, active], &properties);
            let sync_ms = start.elapsed().as_secs_f64() * 1000.;
            let start = crate::time::Instant::now();
            let recorded = artifact(
                record_surface_dag_frame_artifact_cached(
                    &arena,
                    &[root, active],
                    &properties,
                    &generations,
                    &mut recording,
                )
                .unwrap(),
            );
            assert!(
                recorded.chunks.len() >= count as usize,
                "static nodes must actually be recorded"
            );
            let record_ms = start.elapsed().as_secs_f64() * 1000.;
            let start = crate::time::Instant::now();
            let prepared =
                prepare_artifact_surface_raster_plan_cached(recorded, context, &mut planning)
                    .unwrap();
            let plan_ms = start.elapsed().as_secs_f64() * 1000.;
            std::hint::black_box(prepared);
            if frame >= 30 {
                let (reused, validated) = planning.command_validation_counts();
                assert!(
                    reused >= count as usize,
                    "unrelated static commands must retain their full proof"
                );
                assert!(
                    validated <= 4,
                    "only the changed owner needs command validation"
                );
                samples.push([sync_ms, record_ms, plan_ms]);
            }
        }
        let medians = [0, 1, 2].map(|phase| {
            let mut values = samples.iter().map(|row| row[phase]).collect::<Vec<_>>();
            values.sort_by(f64::total_cmp);
            (values[149] + values[150]) / 2.
        });
        println!(
            "incremental-scaling static_nodes={count} samples=300 sync_record_plan_median_ms={medians:?}"
        );
    }
}
