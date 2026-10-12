use super::super::style_pipeline_tests::read_submitted_texture;
use super::*;

const WINDOW: [u32; 2] = [1280, 720];
const LIMIT: u64 = 128 * 1024 * 1024;

// The large DPR 2 scroll source keeps its full 1280x1440 logical envelope,
// while its 720px receiver reads only the first guarded 256px-grid window.
// Ordinary opacity layers fill the whole target, so they stay whole.
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
                    [4096, 4096],
                    max_dimension,
                    limit,
                )
                .unwrap()
            };
            let kept = |limit| {
                prepare_artifact_surface_raster_plan(artifact.clone(), context(limit, 8192))
                    .expect("a surface past the budget is still planned")
                    .nodes()
                    .iter()
                    .filter(|node| node.retained())
                    .count()
            };
            let plan = prepare_artifact_surface_raster_plan(artifact.clone(), context(bytes, 8192))
                .unwrap();
            assert_eq!(plan.nodes().len(), layers);
            for node in plan.nodes() {
                assert_eq!(
                    (node.target().color.width(), node.target().color.height()),
                    (physical_width, physical_height)
                );
            }
            // The exact byte limit is inclusive; one byte less drops a layer
            // from the cache.
            assert_eq!(kept(bytes), layers);
            assert_eq!(kept(bytes - 1), layers - 1);
            let dimension = physical_width.max(physical_height);
            assert!(
                prepare_artifact_surface_raster_plan(artifact.clone(), context(bytes, dimension))
                    .is_ok()
            );
            // Below the device limit, a source too long only along its scroll
            // axis windows to the scrollport at full resolution; one too wide
            // for the target it fills draws at a lower resolution. Neither is
            // rejected for its size.
            let limited = prepare_artifact_surface_raster_plan(
                artifact.clone(),
                context(bytes, dimension - 1),
            )
            .expect("no surface is rejected for its size");
            for node in limited.nodes() {
                let size = [node.target().color.width(), node.target().color.height()];
                assert!(size.iter().all(|n| *n < dimension), "{size:?}");
                if tall && physical_height > physical_width {
                    assert_eq!(
                        size[0], physical_width,
                        "the scroll source keeps its resolution"
                    );
                } else {
                    assert!(
                        size[0] < physical_width,
                        "the layer draws at a lower resolution"
                    );
                }
            }
            assert_eq!(kept(LIMIT) == layers, bytes <= LIMIT);
        }
    }
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_full_window_budget_descriptors_and_reuse() -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native GPU");
    for (layers, tall) in [(1, false), (3, false), (4, false), (1, true)] {
        for initial_dpr in [1_u32, 2] {
            if layers == 4 && initial_dpr == 2 {
                continue;
            }
            let f = window_fixture(layers, tall);
            let mut viewport = Viewport::new();
            viewport.install_single_viewport_forest_for_test(f.arena, f.roots);
            let mut first = std::collections::HashMap::<
                u32,
                Vec<(
                    crate::view::frame_graph::PersistentTextureKey,
                    crate::view::frame_graph::TextureDesc,
                )>,
            >::new();
            // Four layers cross the cache budget on DPR 1 -> 2, then return
            // to 1. Past the budget one layer is drawn every frame without
            // being kept; every DPR change rasters the kept layers again.
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
                let observed = viewport.render_single_viewport_scene_for_test()?;
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
                        "{layers}/{tall}/{dpr}/{frame} {x},{y}: {:?}",
                        &pixels[at..at + 4]
                    );
                }
                let kept = if bytes > LIMIT { layers - 1 } else { layers };
                assert_eq!(observed.color_targets.len(), kept);
                // Admission reserves color + potential depth (12 B/px); only color
                // is persistent now (4 B/px), even on cold raster frames.
                assert_eq!(
                    observed.texture_bytes,
                    bytes / 3 / layers as u64 * kept as u64
                );
                let cached = if frame > 0 && sequence[frame - 1] == dpr {
                    RetainedSurfaceCompileAction::Reuse
                } else {
                    RetainedSurfaceCompileAction::Reraster
                };
                assert_eq!(
                    observed
                        .actions
                        .iter()
                        .filter(|action| **action == RetainedSurfaceCompileAction::Transient)
                        .count(),
                    layers - kept
                );
                assert!(observed.actions.iter().all(|action| *action == cached
                    || *action == RetainedSurfaceCompileAction::Transient));
                for (key, desc) in &observed.color_targets {
                    assert_eq!(
                        (desc.width(), desc.height()),
                        (physical_width, physical_height)
                    );
                    assert!(viewport.has_compatible_persistent_render_target(*key, desc));
                }
                let first = first
                    .entry(dpr)
                    .or_insert_with(|| observed.color_targets.clone());
                assert_eq!(&observed.color_targets, first);
            }
            eprintln!("full window: layers={layers} tall={tall} DPR sequence={sequence:?} passed");
        }
    }
    Ok(())
}
