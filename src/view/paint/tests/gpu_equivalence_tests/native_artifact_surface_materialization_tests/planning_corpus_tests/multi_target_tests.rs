use super::super::style_pipeline_tests::read_submitted_texture;
use super::*;
use crate::view::frame_graph::execution_failure_test_support::arm_after;
use crate::view::viewport::ViewportPaintRendererMode;

const SIZE: [u32; 2] = [96, 64];

fn forest() -> Fixture {
    let mut arena = new_test_arena();
    let mut group_style = style([48.0, 24.0], Some([0.0, 0.0]), None);
    group_style.insert(PropertyId::Opacity, ParsedValue::Opacity(Opacity::new(0.5)));
    let group = commit_element(
        &mut arena,
        Box::new(element(0xc3_f001, [48.0, 24.0], group_style)),
    );
    for (index, x, color) in [(0, 4.0, RED), (1, 24.0, BLUE)] {
        let mut s = style([16.0, 16.0], Some([x, 4.0]), Some(color));
        s.insert(PropertyId::Opacity, ParsedValue::Opacity(Opacity::new(0.5)));
        commit_child(
            &mut arena,
            group,
            Box::new(element(0xc3_f002 + index, [16.0, 16.0], s)),
        );
    }
    let mut s = style([12.0, 12.0], Some([64.0, 4.0]), Some(GREEN));
    s.insert(PropertyId::Opacity, ParsedValue::Opacity(Opacity::new(0.5)));
    let independent = commit_element(&mut arena, Box::new(element(0xc3_f010, [12.0, 12.0], s)));
    Fixture {
        arena,
        roots: vec![group, independent],
        paint_owners: vec![],
        probes: vec![
            ("first branch", [8, 8], [255, 0, 0, 64]),
            ("second branch", [28, 8], [0, 0, 255, 64]),
            ("independent root", [68, 8], [0, 255, 0, 128]),
            ("between branches", [22, 8], CLEAR),
            ("below group", [8, 28], CLEAR),
        ],
    }
}

fn prepared_forest(dpr: f32) -> crate::view::paint::PreparedArtifactSurfaceFrame {
    let mut f = forest();
    let mut layout = Viewport::new();
    for &root in &f.roots {
        crate::view::viewport::layout_artifact_style_scene_for_test(
            &mut layout,
            &mut f.arena,
            root,
            SIZE.map(|v| v as f32),
        );
    }
    let plan = prepare(record(&f), dpr, [0.0; 2]);
    assert_eq!(plan.roots().len(), 2);
    assert_eq!(
        plan.nodes().len(),
        4,
        "two branches, their receiver, an independent root"
    );
    seal_prepared_artifact_surface_frame(plan).unwrap()
}

/// A surface whose color or depth key the graph already declared is drawn
/// into a frame-local target and dropped from the cache; the rest are not.
#[test]
fn surface_with_a_taken_color_or_depth_key_alone_is_drawn_without_being_kept() {
    let entries = prepared_forest(1.0)
        .residents()
        .ordered_entries()
        .iter()
        .map(|e| {
            (
                e.resident_key(),
                e.stamp().identity.color_key,
                e.stamp().target.clone(),
            )
        })
        .collect::<Vec<_>>();
    for (index, (resident_key, color, target)) in entries.iter().enumerate() {
        for (key, desc) in [
            (*color, target.color.clone()),
            (color.depth_stencil().unwrap(), target.depth.clone()),
        ] {
            let mut viewport = Viewport::new();
            let owner = viewport.begin_retained_surface_frame_stage().unwrap();
            emit_prepared_artifact_surface_frame_from_pool(
                &mut viewport,
                Some(owner),
                prepared_forest(1.0),
                &mut FrameGraph::new(),
                UiBuildContext::new(96, 64, FORMAT, 1.0),
            );
            assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));
            assert_eq!(
                viewport
                    .committed_retained_surface_resident_keys_for_test()
                    .len(),
                4
            );
            let owner = viewport.begin_retained_surface_frame_stage().unwrap();
            let mut graph = FrameGraph::new();
            graph.declare_persistent_texture_internal::<()>(desc, key);
            let _ = take_last_production_actions_for_test();
            emit_prepared_artifact_surface_frame_from_pool(
                &mut viewport,
                Some(owner),
                prepared_forest(1.0),
                &mut graph,
                UiBuildContext::new(96, 64, FORMAT, 1.0),
            );
            let transient = take_last_production_actions_for_test()
                .into_iter()
                .map(|action| action == RetainedSurfaceCompileAction::Transient)
                .collect::<Vec<_>>();
            assert_eq!(
                transient,
                (0..4).map(|i| i == index).collect::<Vec<_>>(),
                "entry {index}"
            );
            let kept = viewport
                .pending_artifact_surface_resident_keys_for_test()
                .unwrap();
            assert_eq!(kept.len(), 3, "entry {index}");
            assert!(!kept.contains(resident_key), "entry {index}");
            assert!(viewport.finish_retained_surface_transaction_for_frame(Some(owner), true));
            assert_eq!(
                viewport.retained_surface_release_log_for_test(),
                [*color],
                "entry {index}"
            );
        }
    }
}

fn install() -> Viewport {
    let f = forest();
    let mut viewport = Viewport::new();
    viewport.set_paint_renderer_mode(ViewportPaintRendererMode::RetainedAuto);
    viewport.install_single_viewport_forest_for_test(f.arena, f.roots);
    viewport
}

fn begin(viewport: &mut Viewport, gpu: &NativeGpu, dpr: u32) -> Result<(), String> {
    viewport.begin_offscreen_test_frame(
        gpu.device.clone(),
        gpu.queue.clone(),
        SIZE[0] * dpr,
        SIZE[1] * dpr,
        FORMAT,
    )?;
    viewport.set_scale_factor(dpr as f32);
    assert_eq!(viewport.logical_size(), (96.0, 64.0));
    Ok(())
}

fn pixels(pixels: &[u8], dpr: u32) {
    for (name, [x, y], expected) in forest().probes {
        let at = ((y * dpr * SIZE[0] * dpr + x * dpr) * 4) as usize;
        assert!(
            pixels[at..at + 4]
                .iter()
                .zip(expected)
                .all(|(a, b)| a.abs_diff(b) <= 1),
            "{name} DPR={dpr}: {:?}",
            &pixels[at..at + 4]
        );
    }
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_multi_target_failure_at_every_execution_step_recovers_the_whole_forest()
-> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native GPU");
    for dpr in [1_u32, 2] {
        let mut baseline = install();
        let mut counts = Vec::new();
        let mut targets = Vec::new();
        for warm in [false, true] {
            begin(&mut baseline, gpu, dpr)?;
            let counter = arm_after(usize::MAX);
            let observed = baseline.render_single_viewport_scene_for_test()?;
            pixels(
                &read_submitted_texture(&observed.texture, gpu, SIZE.map(|v| v * dpr))?,
                dpr,
            );
            assert_eq!(observed.color_targets.len(), 4);
            assert_eq!(
                observed.actions,
                vec![
                    if warm {
                        RetainedSurfaceCompileAction::Reuse
                    } else {
                        RetainedSurfaceCompileAction::Reraster
                    };
                    4
                ]
            );
            assert!(!counter.fired());
            assert!(counter.steps() > 0);
            if !warm {
                assert!(
                    counter.steps() >= 4,
                    "cold execution must visit multiple targets"
                );
            }
            counts.push(counter.steps());
            targets = observed.color_targets;
        }
        // Enumerate every actual cold and warm execute step, including the last
        // one just before commit/submit. No hard-coded pass order is assumed.
        for (warm, steps) in [(false, counts[0]), (true, counts[1])] {
            for step in 1..=steps {
                let mut viewport = install();
                if warm {
                    begin(&mut viewport, gpu, dpr)?;
                    viewport.render_single_viewport_scene_for_test()?;
                }
                begin(&mut viewport, gpu, dpr)?;
                {
                    let fault = arm_after(step);
                    viewport.render_single_viewport_execution_failure_for_test()?;
                    assert!(fault.fired());
                    assert_eq!(fault.steps(), step);
                }
                assert!(
                    viewport
                        .committed_retained_surface_resident_keys_for_test()
                        .is_empty()
                );
                assert_eq!(
                    viewport.pending_artifact_surface_resident_keys_for_test(),
                    None
                );
                for (key, desc) in &targets {
                    assert!(
                        !viewport.has_compatible_persistent_render_target(*key, desc),
                        "every physical pair must be discarded, including unexecuted targets"
                    );
                }
                // The execute failure latches Legacy until explicit reset.
                // All outputs still have independent geometry/color probes.
                for recovery in 0..4 {
                    if recovery == 2 {
                        viewport.set_paint_renderer_mode(ViewportPaintRendererMode::RetainedAuto);
                    }
                    begin(&mut viewport, gpu, dpr)?;
                    let observed = if recovery < 2 {
                        viewport.render_single_viewport_legacy_recovery_for_test()?
                    } else {
                        viewport.render_single_viewport_scene_for_test()?
                    };
                    pixels(
                        &read_submitted_texture(&observed.texture, gpu, SIZE.map(|v| v * dpr))?,
                        dpr,
                    );
                    if recovery < 2 {
                        assert!(observed.legacy_selected);
                    } else {
                        assert!(observed.artifact_selected);
                        assert_eq!(
                            observed.actions,
                            vec![
                                if recovery == 2 {
                                    RetainedSurfaceCompileAction::Reraster
                                } else {
                                    RetainedSurfaceCompileAction::Reuse
                                };
                                4
                            ]
                        );
                        assert_eq!(observed.color_targets, targets);
                        for (key, desc) in &observed.color_targets {
                            assert!(viewport.has_compatible_persistent_render_target(*key, desc));
                        }
                    }
                }
            }
            eprintln!(
                "multi target DPR={dpr} warm={warm}: all {steps} execution failure positions recovered"
            );
        }
    }
    Ok(())
}
