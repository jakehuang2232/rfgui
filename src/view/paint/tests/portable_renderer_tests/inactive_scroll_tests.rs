//! Empty / fitting / overflowing transitions through one production Viewport.
//! Both renderers face geometry-derived expectations; neither is an oracle.
use super::*;
use crate::view::test_support::get_element_mut;

const SIZE: [u32; 2] = [64, 112];

fn host_style(height: f32) -> Style {
    let mut value = style([48.0, height], Some([4.0, 4.0]), None);
    value.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(ScrollDirection::Vertical),
    );
    value
}

fn content_style(height: f32) -> Style {
    let mut value = style([20.0, height], None, None);
    value.set_background_image(
        Gradient::linear(SideOrCorner::Bottom)
            .stop(Color::rgb(255, 0, 0), Some(Length::px(0.0)))
            .stop(Color::rgb(255, 0, 0), Some(Length::px(32.0)))
            .stop(Color::rgb(0, 0, 255), Some(Length::px(32.0)))
            .stop(Color::rgb(0, 0, 255), Some(Length::percent(100.0)))
            .build(),
    );
    value
}

pub(super) async fn run(gpu: &Gpu) -> Result<usize, String> {
    let mut frames = 0;
    for mode in [
        ViewportPaintRendererMode::RetainedAuto,
        ViewportPaintRendererMode::Legacy,
    ] {
        for dpr in [1, 2] {
            let mut arena = new_test_arena();
            let root = commit_element(
                &mut arena,
                Box::new(element(0xa210, [48.0, 40.0], host_style(40.0))),
            );
            get_element_mut::<Element>(&arena, root).set_scrollbar_shadow_blur_radius_for_test(0.0);
            let mut viewport = Viewport::new();
            viewport.set_paint_renderer_mode(mode);
            viewport.install_single_viewport_scene_for_test(arena, root);
            let mut child = None;
            let mut last_target = None;
            // Ordered states. Only frames 3/11 are no-op, frame 4 is offset-only,
            // frame 5 is hover-only. Later mutations must not be cited as
            // single-variable invalidation evidence.
            for (frame, (child_height, host_height, offset, hovered)) in [
                (None, 40.0, 0.0, false),
                (Some(20.0), 40.0, 0.0, false),
                (Some(80.0), 40.0, 0.0, false),
                (Some(80.0), 40.0, 0.0, false),
                (Some(80.0), 40.0, 16.0, false),
                (Some(80.0), 40.0, 16.0, true),
                (Some(80.0), 100.0, 16.0, true),
                (Some(80.0), 40.0, 0.0, true),
                (Some(20.0), 40.0, 0.0, true),
                (None, 40.0, 0.0, true),
                (Some(80.0), 40.0, 0.0, true),
                (Some(80.0), 40.0, 0.0, true),
            ]
            .into_iter()
            .enumerate()
            {
                viewport.edit_scene_arena_for_test(|arena| {
                    if ![3, 4, 5, 11].contains(&frame) {
                        get_element_mut::<Element>(arena, root)
                            .apply_style(host_style(host_height));
                        match (child, child_height) {
                            (Some(key), None) => {
                                arena.remove_subtree(key);
                                child = None;
                            }
                            (None, Some(height)) => {
                                child = Some(commit_child(
                                    arena,
                                    root,
                                    Box::new(element(
                                        0xa211,
                                        [20.0, height],
                                        content_style(height),
                                    )),
                                ));
                            }
                            (Some(key), Some(height)) => get_element_mut::<Element>(arena, key)
                                .apply_style(content_style(height)),
                            (None, None) => {}
                        }
                    }
                    let mut host = get_element_mut::<Element>(arena, root);
                    host.set_scroll_offset((0.0, offset));
                    host.set_hovered(hovered);
                });
                let label = format!("inactive scroll {mode:?} DPR={dpr} frame={frame}");
                progress(&label);
                begin(&mut viewport, gpu, SIZE, dpr)?;
                let observed = viewport.render_single_viewport_scene_for_test()?;
                let pixels = read_pixels(gpu, &observed.texture, SIZE.map(|v| v * dpr)).await?;
                let active = child_height.is_some_and(|height| height > host_height);
                let effective_offset = if active { offset } else { 0.0 };
                assert_eq!(
                    get_element_mut::<Element>(viewport.node_arena(), root).get_scroll_offset(),
                    (0.0, effective_offset),
                    "{label}: layout must clamp offset when content fits"
                );
                // Probe x=8 is always inside the child's horizontal extent.
                // y=52 is below the 40px port, but inside the enlarged 100px port.
                for y in [8_u32, 28, 52, 88] {
                    let content_y = y as f32 - 4.0 + effective_offset;
                    let color = if y as f32 >= 4.0 + host_height
                        || child_height.is_none_or(|height| content_y >= height)
                    {
                        CLEAR
                    } else if content_y < 32.0 {
                        RED
                    } else {
                        BLUE
                    };
                    probe(&pixels, SIZE, dpr, "content or bottom clip", [8, y], color)
                        .map_err(|e| format!("{label}: {e}"))?;
                }
                probe(&pixels, SIZE, dpr, "left exterior", [2, 8], CLEAR)?;
                probe(&pixels, SIZE, dpr, "right exterior", [56, 8], CLEAR)?;
                // Track x=43..49, y=7..41; (46,38) is below the thumb
                // for both offsets. Zero-blur shadow alpha .5 then fill .35
                // gives .675 -> 172. Inactive hosts must leave no ghost overlay.
                let alpha = pixels[((38 * dpr * SIZE[0] * dpr + 46 * dpr) * 4 + 3) as usize];
                let expected_alpha = if active && hovered { 172_u8 } else { 0 };
                assert!(
                    alpha.abs_diff(expected_alpha) <= 1,
                    "{label}: scrollbar alpha {alpha} != {expected_alpha}"
                );
                if mode == ViewportPaintRendererMode::RetainedAuto {
                    assert!(observed.artifact_selected, "{label}");
                    assert_eq!(observed.color_targets.len(), usize::from(active), "{label}");
                    if active {
                        assert_eq!(observed.actions.len(), 1, "{label}");
                        assert_eq!(
                            observed.actions[0],
                            if [3, 4, 5, 11].contains(&frame) {
                                RetainedSurfaceCompileAction::Reuse
                            } else {
                                RetainedSurfaceCompileAction::Reraster
                            },
                            "{label}"
                        );
                        let (key, desc) = &observed.color_targets[0];
                        assert!(viewport.has_compatible_persistent_render_target(*key, desc));
                        // Envelope includes the 48px scrollport width and the
                        // complete 80px content height, never just the 40px port.
                        assert_eq!(observed.texture_bytes, 48 * 80 * 4 * u64::from(dpr * dpr));
                        if [3, 4, 5, 11].contains(&frame) {
                            assert_eq!(last_target.as_ref(), Some(&observed.color_targets));
                        }
                        last_target = Some(observed.color_targets);
                    } else {
                        assert!(observed.actions.is_empty());
                        assert_eq!(observed.texture_bytes, 0);
                    }
                } else {
                    assert!(observed.legacy_selected, "{label}");
                }
                frames += 1;
            }
        }
    }
    assert_eq!(frames, 48);
    Ok(frames)
}
