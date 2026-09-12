use super::super::style_pipeline_tests::read_submitted_texture;
use super::*;
use crate::view::test_support::get_element_mut;
use crate::view::viewport::ViewportPaintRendererMode;

fn scene() -> (NodeArena, NodeKey, NodeKey, NodeKey) {
    let mut arena = NodeArena::new();
    let mut root_style = style([40.0, 40.0], None, None);
    root_style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(ScrollDirection::Vertical),
    );
    let root = commit_element(
        &mut arena,
        Box::new(element(0x6d00, [40.0, 40.0], root_style)),
    );
    let content = commit_child(
        &mut arena,
        root,
        Box::new(element(
            0x6d01,
            [40.0, 240.0],
            style([40.0, 240.0], None, None),
        )),
    );
    commit_child(
        &mut arena,
        content,
        Box::new(element(
            0x6d02,
            [16.0, 16.0],
            style([16.0, 16.0], Some([0.0, 20.0]), Some(BLUE)),
        )),
    );
    let hidden = commit_child(
        &mut arena,
        content,
        Box::new(element(
            0x6d03,
            [16.0, 16.0],
            style([16.0, 16.0], Some([0.0, 180.0]), Some(RED)),
        )),
    );
    let mut deferred_style = style([12.0, 12.0], None, Some(GREEN));
    deferred_style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .left(Length::px(52.0))
                .top(Length::px(20.0))
                .clip(ClipMode::Viewport),
        ),
    );
    let deferred = commit_child(
        &mut arena,
        content,
        Box::new(element(0x6d04, [12.0, 12.0], deferred_style)),
    );
    (arena, root, hidden, deferred)
}

#[test]
fn culled_scroll_descendant_and_deferred_scope_record_from_live_layout() {
    let (mut arena, root, hidden, deferred) = scene();
    let mut viewport = Viewport::new();
    crate::view::viewport::layout_artifact_style_scene_for_test(
        &mut viewport,
        &mut arena,
        root,
        EXTENT.map(|x| x as f32),
    );
    assert!(
        !arena
            .get(hidden)
            .unwrap()
            .element
            .box_model_snapshot()
            .should_render
    );
    let (properties, generations) = sync_identity(&arena, &[root]);
    assert!(properties.paint_state_for(hidden).unwrap().scroll.is_some());
    let FrameArtifactRecordOutcome::Artifact { artifact, .. } = record_surface_dag_frame_artifact(
        &arena,
        &[root],
        &properties,
        &generations,
        RendererMode::ForcedForTests,
    )
    .expect("culled inherited scroll and late Replace clip are supported") else {
        panic!("fallback")
    };
    assert!(!artifact.chunks.iter().any(|c| c.owner == hidden));
    assert_eq!(
        artifact
            .chunks
            .iter()
            .filter(|c| c.owner == deferred)
            .count(),
        1,
        "late content is recorded exactly once"
    );
    prepare(artifact, 1.0, [0.0; 2]);
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_scroll_culling_reentry_and_deferred_pixels() -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().unwrap();
    for mode in [
        ViewportPaintRendererMode::RetainedAuto,
        ViewportPaintRendererMode::Legacy,
    ] {
        for dpr in [1_u32, 2] {
            let (arena, root, hidden, _) = scene();
            let mut viewport = Viewport::new();
            viewport.set_paint_renderer_mode(mode);
            viewport.install_single_viewport_scene_for_test(arena, root);
            // Each changed placement is followed by a truly unchanged frame.
            // Re-entry must not reuse a raster missing the formerly culled red.
            for (frame, scroll) in [0.0, 0.0, 160.0, 160.0, 0.0, 0.0].into_iter().enumerate() {
                get_element_mut::<Element>(viewport.node_arena(), root)
                    .set_scroll_offset((0.0, scroll));
                viewport.begin_offscreen_test_frame(
                    gpu.device.clone(),
                    gpu.queue.clone(),
                    EXTENT[0] * dpr,
                    EXTENT[1] * dpr,
                    FORMAT,
                )?;
                viewport.set_scale_factor(dpr as f32);
                assert_eq!(
                    viewport.logical_size(),
                    (EXTENT[0] as f32, EXTENT[1] as f32)
                );
                let observed = viewport.render_single_viewport_scene_for_test()?;
                assert_eq!(
                    viewport
                        .node_arena()
                        .get(hidden)
                        .unwrap()
                        .element
                        .box_model_snapshot()
                        .should_render,
                    scroll > 0.0
                );
                let pixels =
                    read_submitted_texture(&observed.texture, gpu, EXTENT.map(|x| x * dpr))?;
                // Deferred content inherits scroll displacement but escapes the
                // scrollport clip. No color here is sampled from the other mode.
                for ([x, y], expected) in [
                    ([4, 24], if scroll == 0.0 { BLUE } else { RED }),
                    ([54, 24], if scroll == 0.0 { GREEN } else { CLEAR }),
                    ([4, 44], CLEAR),
                ] {
                    let at = ((y * dpr * EXTENT[0] * dpr + x * dpr) * 4) as usize;
                    assert!(
                        pixels[at..at + 4]
                            .iter()
                            .zip(expected)
                            .all(|(a, b)| a.abs_diff(b) <= 1),
                        "{mode:?} DPR={dpr} frame={frame} ({x},{y}): {:?} expected {expected:?}",
                        &pixels[at..at + 4]
                    );
                }
                if mode == ViewportPaintRendererMode::RetainedAuto {
                    assert!(observed.artifact_selected);
                    assert!(!observed.actions.is_empty());
                    if frame % 2 == 1 {
                        assert!(
                            observed
                                .actions
                                .iter()
                                .all(|a| *a == RetainedSurfaceCompileAction::Reuse),
                            "warm {frame}: {:?}",
                            observed.actions
                        );
                    }
                    for (key, desc) in &observed.color_targets {
                        assert!(viewport.has_compatible_persistent_render_target(*key, desc));
                    }
                } else {
                    assert!(observed.legacy_selected);
                }
            }
        }
    }
    Ok(())
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_asymmetric_child_mask_keeps_large_corner_geometry() -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().unwrap();
    for mode in [
        ViewportPaintRendererMode::RetainedAuto,
        ViewportPaintRendererMode::Legacy,
    ] {
        for dpr in [1_u32, 2] {
            let mut arena = NodeArena::new();
            let mut rounded = style([150., 150.], None, None);
            rounded.set_border_radius(
                crate::style::BorderRadius::new()
                    .top_left(Length::px(8.))
                    .top_right(Length::px(32.))
                    .bottom_right(Length::px(8.))
                    .bottom_left(Length::px(135.)),
            );
            let root = commit_element(&mut arena, Box::new(element(0x6d10, [150., 150.], rounded)));
            commit_child(
                &mut arena,
                root,
                Box::new(element(
                    0x6d11,
                    [150., 150.],
                    style([150., 150.], None, Some(GREEN)),
                )),
            );
            let mut v = Viewport::new();
            v.set_paint_renderer_mode(mode);
            v.install_single_viewport_scene_for_test(arena, root);
            for frame in 0..2 {
                v.begin_offscreen_test_frame(
                    gpu.device.clone(),
                    gpu.queue.clone(),
                    160 * dpr,
                    160 * dpr,
                    FORMAT,
                )?;
                v.set_scale_factor(dpr as f32);
                let observed = v.render_single_viewport_scene_for_test()?;
                let pixels =
                    read_submitted_texture(&observed.texture, gpu, [160 * dpr, 160 * dpr])?;
                // Bottom-left arc: center (135,15), radius 135. (20,110)
                // is outside it but INSIDE a wrongly clamped 75px corner.
                for ([x, y], expected) in [
                    ([20, 110], CLEAR),
                    ([50, 80], GREEN),
                    ([140, 140], GREEN),
                    ([154, 100], CLEAR),
                ] {
                    let at = ((y * dpr * 160 * dpr + x * dpr) * 4) as usize;
                    assert!(
                        pixels[at..at + 4]
                            .iter()
                            .zip(expected)
                            .all(|(a, b)| a.abs_diff(b) <= 1),
                        "{mode:?} DPR={dpr} frame={frame} ({x},{y}) {:?}",
                        &pixels[at..at + 4]
                    );
                }
                if mode == ViewportPaintRendererMode::RetainedAuto {
                    assert!(observed.artifact_selected);
                }
            }
        }
    }
    Ok(())
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_nested_deferred_zero_size_reentry_preserves_late_order() -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().unwrap();
    for mode in [
        ViewportPaintRendererMode::RetainedAuto,
        ViewportPaintRendererMode::Legacy,
    ] {
        for dpr in [1, 2] {
            let (mut arena, root, _, deferred) = scene();
            let nested_style = |size: [f32; 2]| {
                let mut s = style(size, None, Some(RED));
                s.insert(
                    PropertyId::Position,
                    ParsedValue::Position(
                        Position::absolute()
                            .left(Length::px(0.))
                            .top(Length::px(0.))
                            .clip(ClipMode::Viewport),
                    ),
                );
                s
            };
            let nested = commit_child(
                &mut arena,
                deferred,
                Box::new(element(0x6d20, [0.; 2], nested_style([0.; 2]))),
            );
            let mut v = Viewport::new();
            v.set_paint_renderer_mode(mode);
            v.install_single_viewport_scene_for_test(arena, root);
            for (frame, size) in [[0.; 2], [0.; 2], [8.; 2], [8.; 2], [0.; 2], [0.; 2]]
                .into_iter()
                .enumerate()
            {
                get_element_mut::<Element>(v.node_arena(), nested).apply_style(nested_style(size));
                v.begin_offscreen_test_frame(
                    gpu.device.clone(),
                    gpu.queue.clone(),
                    EXTENT[0] * dpr,
                    EXTENT[1] * dpr,
                    FORMAT,
                )?;
                v.set_scale_factor(dpr as f32);
                let observed = v.render_single_viewport_scene_for_test()?;
                let pixels =
                    read_submitted_texture(&observed.texture, gpu, EXTENT.map(|n| n * dpr))?;
                for ([x, y], expected) in [
                    ([54, 24], if size[0] == 0. { GREEN } else { RED }),
                    ([62, 24], GREEN),
                    ([4, 24], BLUE),
                ] {
                    let at = ((y * dpr * EXTENT[0] * dpr + x * dpr) * 4) as usize;
                    assert!(
                        pixels[at..at + 4]
                            .iter()
                            .zip(expected)
                            .all(|(a, b)| a.abs_diff(b) <= 1),
                        "{mode:?} DPR={dpr} frame={frame} ({x},{y}): {:?} expected {expected:?}",
                        &pixels[at..at + 4]
                    );
                }
                if mode == ViewportPaintRendererMode::RetainedAuto {
                    assert!(observed.artifact_selected);
                    if frame % 2 == 1 {
                        assert!(
                            observed
                                .actions
                                .iter()
                                .all(|a| *a == RetainedSurfaceCompileAction::Reuse)
                        );
                    }
                }
            }
        }
    }
    Ok(())
}
