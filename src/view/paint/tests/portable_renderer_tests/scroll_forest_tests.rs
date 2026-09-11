//! Independent replacement for the retired multi-root / branching scroll
//! planner's runtime coverage. Expectations come from authored geometry; no
//! historical planner, detached recorder or Legacy readback supplies them.
use super::*;
use crate::view::test_support::get_element_mut;

const SIZE: [u32; 2] = [104, 56];

fn scroll_style(size: [f32; 2], at: [f32; 2]) -> Style {
    let mut value = style(size, Some(at), None);
    value.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(ScrollDirection::Vertical),
    );
    value
}

fn content_style(height: f32, first: [u8; 4]) -> Style {
    let mut value = style([20.0, height], None, None);
    let color = Color::rgba(first[0], first[1], first[2], first[3]);
    value.set_background_image(
        Gradient::linear(SideOrCorner::Bottom)
            .stop(color, Some(Length::px(0.0)))
            .stop(color, Some(Length::px(32.0)))
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
                Box::new(element(
                    0xc8_100,
                    [48.0, 40.0],
                    scroll_style([48.0, 40.0], [4.0, 4.0]),
                )),
            );
            // Absolute panes do not establish scrollable layout extent.
            // This in-flow document box authors the parent's 48x64 extent.
            commit_child(
                &mut arena,
                root,
                Box::new(element(
                    0xc8_120,
                    [48.0, 64.0],
                    style([48.0, 64.0], None, None),
                )),
            );
            for (id, x, offset, color) in [(0xc8_101, 0.0, 4.0, RED), (0xc8_103, 24.0, 12.0, GREEN)]
            {
                let child = commit_child(
                    &mut arena,
                    root,
                    Box::new(element(
                        id,
                        [20.0, 64.0],
                        scroll_style([20.0, 64.0], [x, 0.0]),
                    )),
                );
                commit_child(
                    &mut arena,
                    child,
                    Box::new(element(id + 1, [20.0, 96.0], content_style(96.0, color))),
                );
                get_element_mut::<Element>(&arena, child).set_scroll_offset((0.0, offset));
            }
            let independent = commit_element(
                &mut arena,
                Box::new(element(
                    0xc8_110,
                    [20.0, 24.0],
                    scroll_style([20.0, 24.0], [72.0, 4.0]),
                )),
            );
            commit_child(
                &mut arena,
                independent,
                Box::new(element(0xc8_111, [20.0, 80.0], content_style(80.0, RED))),
            );
            get_element_mut::<Element>(&arena, independent).set_scroll_offset((0.0, 16.0));
            let mut viewport = Viewport::new();
            viewport.set_paint_renderer_mode(mode);
            viewport.install_single_viewport_forest_for_test(arena, vec![root, independent]);
            let mut first_targets = None;
            // Ordered sequence: an independent-root offset round trip must
            // reuse every target. Then retain the ancestor-scroll round trip:
            // it also moves child scrollports and changes their clipped bounds
            // at y=0, so it does not establish placement-only raster inputs.
            // Its pixels and resident descriptors remain mandatory; do not
            // claim that those last two frames prove all-target reuse.
            for (frame, (outer, independent_offset)) in
                [(8_u32, 16_u32), (8, 24), (8, 16), (16, 16), (8, 16)]
                    .into_iter()
                    .enumerate()
            {
                get_element_mut::<Element>(viewport.node_arena(), independent)
                    .set_scroll_offset((0.0, independent_offset as f32));
                get_element_mut::<Element>(viewport.node_arena(), root)
                    .set_scroll_offset((0.0, outer as f32));
                let label = format!("scroll forest {mode:?} DPR={dpr} frame={frame}");
                progress(&label);
                begin(&mut viewport, gpu, SIZE, dpr)?;
                let observed = viewport
                    .render_single_viewport_scene_for_test()
                    .map_err(|e| format!("{label}: {e}"))?;
                assert_eq!(
                    get_element_mut::<Element>(viewport.node_arena(), root).get_scroll_offset(),
                    (0.0, outer as f32),
                    "{label}: layout must preserve the authored scroll offset"
                );
                let pixels = read_pixels(gpu, &observed.texture, SIZE.map(|v| v * dpr)).await?;
                // screen boundary = root_y + source_y - outer_offset - child_offset.
                let left = 4 + 32 - outer - 4;
                let right = 4 + 32 - outer - 12;
                for (name, at, expected) in [
                    ("left before hard boundary", [8, left - 4], RED),
                    ("left after hard boundary", [8, left + 4], BLUE),
                    ("right before hard boundary", [32, right - 4], GREEN),
                    ("right after hard boundary", [32, right + 4], BLUE),
                    ("gap between sibling contents", [26, 12], CLEAR),
                    ("outer bottom clip inside left content", [8, 46], CLEAR),
                    (
                        "independent root before boundary",
                        [76, 4 + 32 - independent_offset - 4],
                        RED,
                    ),
                    (
                        "independent root after boundary",
                        [76, 4 + 32 - independent_offset + 4],
                        BLUE,
                    ),
                    ("independent root bottom clip", [76, 30], CLEAR),
                ] {
                    probe(&pixels, SIZE, dpr, name, at, expected)
                        .map_err(|e| format!("{label}: {e}"))?;
                }
                if mode == ViewportPaintRendererMode::RetainedAuto {
                    assert!(observed.artifact_selected, "{label}");
                    assert_eq!(
                        observed.actions.len(),
                        4,
                        "two roots plus two nested sibling targets"
                    );
                    if frame <= 2 {
                        let action = if frame == 0 {
                            RetainedSurfaceCompileAction::Reraster
                        } else {
                            RetainedSurfaceCompileAction::Reuse
                        };
                        assert!(
                            observed.actions.iter().all(|a| *a == action),
                            "{label}: {:?}",
                            observed.actions
                        );
                    }
                    if frame > 2 {
                        // The receiver's child-composite clip changes; the
                        // three child/independent rasters themselves stay exact.
                        assert_eq!(
                            observed
                                .actions
                                .iter()
                                .filter(|a| **a == RetainedSurfaceCompileAction::Reraster)
                                .count(),
                            1,
                            "{label}"
                        );
                        assert_eq!(
                            observed
                                .actions
                                .iter()
                                .filter(|a| **a == RetainedSurfaceCompileAction::Reuse)
                                .count(),
                            3,
                            "{label}"
                        );
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    eprintln!("{label}: actions={:?}", observed.actions);
                    // The parent conservatively unions complete child destinations,
                    // before receiver clipping: y in [-4,92] and [-12,84],
                    // plus its own [0,40]. The union is [-12,92], height 104.
                    // Child targets stay 20x96; the other root is 20x80.
                    // Every target has four color and eight depth bytes.
                    assert_eq!(
                        observed.texture_bytes,
                        (48 * 104 + 2 * 20 * 96 + 20 * 80) * 12 * u64::from(dpr * dpr),
                        "{label}: descriptors={:?}",
                        observed.color_targets
                    );
                    for (key, desc) in &observed.color_targets {
                        assert!(viewport.has_compatible_persistent_render_target_pair(*key, desc));
                    }
                    if let Some(first) = &first_targets {
                        assert_eq!(&observed.color_targets, first);
                    } else {
                        first_targets = Some(observed.color_targets);
                    }
                } else {
                    assert!(observed.legacy_selected, "{label}");
                }
                frames += 1;
            }
        }
    }
    assert_eq!(frames, 20);
    Ok(frames)
}
