use super::super::style_pipeline_tests::read_submitted_texture;
use super::*;
use crate::view::viewport::ViewportPaintRendererMode;

const WINDOW: [u32; 2] = [1280, 720];
const LIMIT: u64 = 128 * 1024 * 1024;

// The large DPR 2 scroll source keeps its full 1280x1440 logical envelope,
// while its 720px receiver reads only the first guarded 256px-grid window.
// Ordinary opacity layers have no finite receiver-clip proof and stay whole.
fn expected_physical_size(tall: bool, dpr: u32) -> [u32; 2] {
    [
        WINDOW[0] * dpr,
        if tall && dpr == 2 {
            1536
        } else {
            WINDOW[1] * dpr * if tall { 2 } else { 1 }
        },
    ]
}

fn window_fixture(layers: usize, tall: bool) -> Fixture {
    let mut arena = new_test_arena();
    let mut roots = Vec::new();
    for index in 0..layers {
        let size = WINDOW.map(|v| v as f32);
        let mut s = style(size, None, if tall { None } else { Some(RED) });
        if tall {
            s.insert(
                PropertyId::ScrollDirection,
                ParsedValue::ScrollDirection(ScrollDirection::Vertical),
            );
        } else {
            s.insert(PropertyId::Opacity, ParsedValue::Opacity(Opacity::new(0.5)));
        }
        let root = commit_element(
            &mut arena,
            Box::new(element(0xc3_b000 + index as u64, size, s)),
        );
        if tall {
            let size = [size[0], size[1] * 2.0];
            commit_child(
                &mut arena,
                root,
                Box::new(element(0xc3_b100, size, style(size, None, Some(RED)))),
            );
        }
        roots.push(root);
    }
    Fixture {
        arena,
        roots,
        paint_owners: vec![],
        probes: vec![],
    }
}

fn laid_out(layers: usize, tall: bool) -> Fixture {
    let mut f = window_fixture(layers, tall);
    let mut viewport = Viewport::new();
    for &root in &f.roots {
        crate::view::viewport::layout_artifact_style_scene_for_test(
            &mut viewport,
            &mut f.arena,
            root,
            WINDOW.map(|v| v as f32),
        );
    }
    f
}

#[test]
fn full_window_budget_uses_exact_aggregate_bytes_and_physical_dimensions() {
    for (layers, tall) in [(1, false), (3, false), (4, false), (1, true)] {
        let artifact = record(&laid_out(layers, tall));
        for dpr in [1_u32, 2] {
            let [physical_width, physical_height] = expected_physical_size(tall, dpr);
            let bytes = u64::from(physical_width) * u64::from(physical_height) * 12 * layers as u64;
            let context = |limit, max_dimension| {
                ArtifactSurfaceRasterContext::new(
                    dpr as f32,
                    FORMAT,
                    [0.0; 2],
                    None,
                    max_dimension,
                    limit,
                )
                .unwrap()
            };
            let plan = prepare_artifact_surface_raster_plan(artifact.clone(), context(bytes, 8192))
                .expect("exact byte limit is inclusive");
            assert_eq!(plan.nodes().len(), layers);
            for node in plan.nodes() {
                assert_eq!(
                    (node.target().color.width(), node.target().color.height()),
                    (physical_width, physical_height)
                );
            }
            assert!(matches!(
                prepare_artifact_surface_raster_plan(artifact.clone(), context(bytes - 1, 8192)),
                Err(crate::view::paint::ArtifactSurfaceRasterPlanError::TextureBudgetExceeded(_))
            ));
            let dimension = physical_width.max(physical_height);
            assert!(
                prepare_artifact_surface_raster_plan(artifact.clone(), context(bytes, dimension))
                    .is_ok()
            );
            assert!(matches!(
                prepare_artifact_surface_raster_plan(
                    artifact.clone(),
                    context(bytes, dimension - 1)
                ),
                Err(crate::view::paint::ArtifactSurfaceRasterPlanError::TextureBudgetExceeded(_))
            ));
            assert_eq!(
                prepare_artifact_surface_raster_plan(artifact.clone(), context(LIMIT, 8192))
                    .is_ok(),
                bytes <= LIMIT
            );
        }
    }
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_full_window_budget_descriptors_reuse_and_whole_frame_legacy() -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native GPU");
    for mode in [
        ViewportPaintRendererMode::RetainedAuto,
        ViewportPaintRendererMode::Legacy,
    ] {
        for (layers, tall) in [(1, false), (3, false), (4, false), (1, true)] {
            for initial_dpr in [1_u32, 2] {
                if layers == 4 && initial_dpr == 2 {
                    continue;
                }
                let f = window_fixture(layers, tall);
                let mut viewport = Viewport::new();
                viewport.set_paint_renderer_mode(mode);
                viewport.install_single_viewport_forest_for_test(f.arena, f.roots);
                let mut first: Option<
                    Vec<(
                        crate::view::frame_graph::PersistentTextureKey,
                        crate::view::frame_graph::TextureDesc,
                    )>,
                > = None;
                // Four layers cross the real policy boundary on DPR 1 -> 2,
                // then return to 1. Rejection must invalidate prior residents;
                // its next admitted frame must reraster before becoming warm.
                let sequence = if layers == 4 {
                    vec![1, 1, 2, 2, 1, 1]
                } else {
                    vec![initial_dpr; 3]
                };
                for (frame, dpr) in sequence.iter().copied().enumerate() {
                    let [physical_width, physical_height] = expected_physical_size(tall, dpr);
                    let bytes =
                        u64::from(physical_width) * u64::from(physical_height) * 12 * layers as u64;
                    viewport.begin_offscreen_test_frame(
                        gpu.device.clone(),
                        gpu.queue.clone(),
                        WINDOW[0] * dpr,
                        WINDOW[1] * dpr,
                        FORMAT,
                    )?;
                    viewport.set_scale_factor(dpr as f32);
                    assert_eq!(viewport.logical_size(), (1280.0, 720.0));
                    let rejected = mode == ViewportPaintRendererMode::RetainedAuto && bytes > LIMIT;
                    let observed = if rejected {
                        viewport.render_single_viewport_budget_fallback_for_test()?
                    } else {
                        viewport.render_single_viewport_scene_for_test()?
                    };
                    let pixels =
                        read_submitted_texture(&observed.texture, gpu, WINDOW.map(|v| v * dpr))?;
                    // Repeated independent half-opacity red roots: alpha = 1 - 0.5^N.
                    // Tall scrolling content is opaque. Probe opposite corners and center.
                    let alpha = if tall {
                        255
                    } else {
                        ((1.0 - 0.5_f32.powi(layers as i32)) * 255.0).round() as u8
                    };
                    for [x, y] in [[4, 4], [640, 360], [1275, 715]] {
                        let at = ((y * dpr * WINDOW[0] * dpr + x * dpr) * 4) as usize;
                        assert!(
                            pixels[at..at + 4]
                                .iter()
                                .zip([255, 0, 0, alpha])
                                .all(|(a, b)| a.abs_diff(b) <= 1),
                            "{mode:?} {layers}/{tall}/{dpr}/{frame} {x},{y}: {:?}",
                            &pixels[at..at + 4]
                        );
                    }
                    if mode == ViewportPaintRendererMode::Legacy || rejected {
                        assert!(observed.legacy_selected);
                        assert!(observed.actions.is_empty());
                        if rejected {
                            for (key, desc) in
                                first.as_ref().expect("DPR 1 populated the prior pairs")
                            {
                                assert!(
                                    !viewport.has_compatible_persistent_render_target(*key, desc),
                                    "budget fallback must release the displaced generic pairs"
                                );
                            }
                        }
                        continue;
                    }
                    assert!(observed.artifact_selected);
                    assert_eq!(observed.color_targets.len(), layers);
                    // Admission reserves color + potential depth (12 B/px); only color
                    // is persistent now (4 B/px), even on cold raster frames.
                    assert_eq!(observed.texture_bytes, bytes / 3);
                    assert_eq!(
                        observed.actions,
                        vec![
                            if frame == 0 || (layers == 4 && frame == 4) {
                                RetainedSurfaceCompileAction::Reraster
                            } else {
                                RetainedSurfaceCompileAction::Reuse
                            };
                            layers
                        ]
                    );
                    for (key, desc) in &observed.color_targets {
                        assert_eq!(
                            (desc.width(), desc.height()),
                            (physical_width, physical_height)
                        );
                        assert!(viewport.has_compatible_persistent_render_target(*key, desc));
                    }
                    if let Some(first) = &first {
                        assert_eq!(&observed.color_targets, first);
                    } else {
                        first = Some(observed.color_targets);
                    }
                }
                eprintln!(
                    "full window {mode:?}: layers={layers} tall={tall} DPR sequence={sequence:?} passed"
                );
            }
        }
    }
    Ok(())
}
