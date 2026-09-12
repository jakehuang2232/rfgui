use super::super::style_pipeline_tests::read_submitted_texture;
use super::*;
use crate::view::test_support::get_element_mut;
use crate::view::viewport::ViewportPaintRendererMode;
fn scene() -> (NodeArena, NodeKey) {
    let mut a = NodeArena::new();
    let mut scroll = style([40., 40.], None, None);
    scroll.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(ScrollDirection::Vertical),
    );
    let root = commit_element(&mut a, Box::new(element(0x6f00, [40., 40.], scroll)));
    let content = commit_child(
        &mut a,
        root,
        Box::new(element(
            0x6f01,
            [40., 100_000.],
            style([40., 100_000.], None, None),
        )),
    );
    commit_child(
        &mut a,
        content,
        Box::new(element(
            0x6f02,
            [40., 50_000.],
            style([40., 50_000.], Some([0., 0.]), Some(RED)),
        )),
    );
    commit_child(
        &mut a,
        content,
        Box::new(element(
            0x6f03,
            [40., 50_000.],
            style([40., 50_000.], Some([0., 50_000.]), Some(BLUE)),
        )),
    );
    let mut masked = element(
        0x6f04,
        [20., 20.],
        style([20., 20.], Some([0., 90_000.]), None),
    );
    masked.set_border_radius(8.);
    let mask = commit_child(&mut a, content, Box::new(masked));
    commit_child(
        &mut a,
        mask,
        Box::new(element(
            0x6f05,
            [20., 20.],
            style([20., 20.], None, Some(RED)),
        )),
    );
    (a, root)
}
#[test]
fn long_content_window_preserves_envelope_and_seals_source_origin() {
    let (mut arena, root) = scene();
    let mut v = Viewport::new();
    crate::view::viewport::layout_artifact_style_scene_for_test(
        &mut v,
        &mut arena,
        root,
        [80., 64.],
    );
    let (properties, generations) = sync_identity(&arena, &[root]);
    let FrameArtifactRecordOutcome::Artifact { artifact, .. } = record_surface_dag_frame_artifact(
        &arena,
        &[root],
        &properties,
        &generations,
        RendererMode::ForcedForTests,
    )
    .unwrap() else {
        panic!("long content must record")
    };
    let plan = prepare(artifact, 1., [0.; 2]);
    let [node] = plan.nodes() else {
        panic!("one materialized scroll target")
    };
    let (full, window) = node
        .raster_window_bounds_for_test()
        .expect("large source needs a proved window");
    assert_eq!(full[3], 100_000.);
    assert_eq!(window, [0., 0., 40., 256.]);
    assert_eq!(
        [node.target().color.width(), node.target().color.height()],
        [40, 256]
    );
}
#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_long_content_windows_scroll_across_color_boundary() -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().unwrap();
    for mode in [
        ViewportPaintRendererMode::RetainedAuto,
        ViewportPaintRendererMode::Legacy,
    ] {
        for dpr in [1_u32, 2] {
            let (arena, root) = scene();
            let mut v = Viewport::new();
            v.set_paint_renderer_mode(mode);
            v.install_single_viewport_scene_for_test(arena, root);
            // Each new source region is followed by an identical frame. The
            // 50,000px red/blue edge proves that a different window contains
            // newly rasterized content, not old pixels under a new position.
            for (frame, scroll) in [
                0., 0., 300., 300., 49_992., 49_992., 50_008., 50_008., 0., 0.,
            ]
            .into_iter()
            .enumerate()
            {
                get_element_mut::<Element>(v.node_arena(), root).set_scroll_offset((0., scroll));
                v.begin_offscreen_test_frame(
                    gpu.device.clone(),
                    gpu.queue.clone(),
                    80 * dpr,
                    64 * dpr,
                    FORMAT,
                )?;
                v.set_scale_factor(dpr as f32);
                let observed = v.render_single_viewport_scene_for_test()?;
                let pixels = read_submitted_texture(&observed.texture, gpu, [80 * dpr, 64 * dpr])?;
                for ([x, y], expected) in [
                    ([4, 4], if scroll + 4. < 50_000. { RED } else { BLUE }),
                    ([4, 20], if scroll + 20. < 50_000. { RED } else { BLUE }),
                    ([4, 44], CLEAR),
                    ([44, 4], CLEAR),
                ] {
                    let at = ((y * dpr * 80 * dpr + x * dpr) * 4) as usize;
                    assert!(
                        pixels[at..at + 4]
                            .iter()
                            .zip(expected)
                            .all(|(a, b)| a.abs_diff(b) <= 1),
                        "{mode:?} DPR={dpr} frame={frame} scroll={scroll} ({x},{y}) {:?} expected {expected:?}",
                        &pixels[at..at + 4]
                    );
                }
                if mode == ViewportPaintRendererMode::RetainedAuto {
                    assert!(observed.artifact_selected);
                    assert_eq!(observed.actions.len(), 1);
                    if frame % 2 == 1 {
                        assert_eq!(observed.actions, [RetainedSurfaceCompileAction::Reuse]);
                    }
                    for (key, desc) in &observed.color_targets {
                        assert!(desc.height() <= 512);
                        assert!(v.has_compatible_persistent_render_target(*key, desc));
                    }
                }
            }
        }
    }
    Ok(())
}
