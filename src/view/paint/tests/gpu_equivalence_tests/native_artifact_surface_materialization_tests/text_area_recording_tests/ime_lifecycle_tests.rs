use super::super::style_pipeline_tests::read_submitted_texture;
use super::*;
use crate::platform::input::{Key, Modifiers};
use crate::ui::{InputType, KeyEventData, KeyLocation};
use crate::view::test_support::{commit_child, commit_element, get_element, get_element_mut};
use crate::view::viewport::{PointerButton, ViewportPaintRendererMode};

const SIZE: [u32; 2] = [220, 96];
const FRAME_COUNT: usize = 33;

fn scene(dpr: u32) -> (Viewport, NodeKey) {
    let mut arena = NodeArena::new();
    let mut wrapper = Element::new_with_id(0xc03300, 0.0, 0.0, 200.0, 80.0);
    let mut style = Style::new();
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    style.insert(PropertyId::Opacity, ParsedValue::Opacity(Opacity::new(0.5)));
    wrapper.apply_style(style);
    let root = commit_element(&mut arena, Box::new(wrapper));
    let mut area = TextArea::with_stable_id(0xc03301);
    area.set_text("MMMM  ".into());
    area.font_size = 20.0;
    area.color = Color::rgb(255, 0, 0);
    area.selection_background_color = Color::rgb(0, 0, 255);
    area.set_layout_offset(8.0, 12.0);
    let owner = commit_child(&mut arena, root, Box::new(area));
    get_element_mut::<TextArea>(&arena, owner).set_self_node_key(owner);
    let mut viewport = Viewport::new();
    viewport.set_scale_factor(dpr as f32);
    crate::view::viewport::layout_artifact_style_scene_for_test(
        &mut viewport,
        &mut arena,
        root,
        SIZE.map(|v| v as f32),
    );
    viewport.install_single_viewport_scene_for_test(arena, root);
    (viewport, owner)
}

fn time_ms(frame: usize) -> u64 {
    match frame {
        // Last caret reset is frame 20 at 200ms. The documented blink is
        // visible for 530ms of each 1060ms period; exercise both edges without sleeps.
        22 => 800,
        23 => 810,
        24 => 1300,
        25 => 1310,
        26.. => 1320 + (frame as u64 - 26) * 10,
        _ => frame as u64 * 10,
    }
}

fn is_warm(frame: usize) -> bool {
    matches!(
        frame,
        2 | 5 | 7 | 10 | 13 | 16 | 18 | 21 | 23 | 25 | 28 | 30 | 32
    )
}

fn click(viewport: &mut Viewport, x: f32, y: f32) {
    viewport.set_pointer_position_viewport(x, y);
    viewport.dispatch_pointer_down_event(PointerButton::Left);
    viewport.dispatch_pointer_up_event(PointerButton::Left);
}

fn key(viewport: &mut Viewport, key: Key, modifiers: Modifiers, now: crate::time::Instant) {
    assert!(viewport.dispatch_key_down_event(KeyEventData {
        key,
        modifiers,
        characters: None,
        repeat: false,
        is_composing: false,
        location: KeyLocation::Standard,
        timestamp: now,
    }));
}

fn apply_event(viewport: &mut Viewport, owner: NodeKey, frame: usize, now: crate::time::Instant) {
    // This is an ordered state machine, shared by CPU and GPU tests. No
    // focus/preedit/caret/dirty fields or prepared artifacts are injected.
    match frame {
        1 | 17 => click(viewport, 180.0, 20.0),
        3 => key(viewport, Key::ArrowLeft, Modifiers::SHIFT, now),
        4 => key(viewport, Key::End, Modifiers::empty(), now),
        5 => assert!(viewport.dispatch_ime_enabled_event()),
        6 => assert!(viewport.dispatch_ime_preedit_event("中a".into(), Some((4, 4)))),
        8 => assert!(viewport.dispatch_ime_preedit_event("中b".into(), Some((4, 4)))),
        9 => {
            assert!(viewport.dispatch_ime_commit_event("界".into()));
            // The backend also emits TextInput(ImeCommit). It must not insert twice.
            viewport.dispatch_text_input_event_full("界".into(), InputType::ImeCommit, false);
        }
        10 | 12 | 31 => assert!(viewport.dispatch_ime_disabled_event()),
        11 | 14 | 26 | 29 => {
            assert!(viewport.dispatch_ime_preedit_event("文".into(), Some((3, 3))))
        }
        15 => click(viewport, 219.0, 95.0),
        16 => assert!(
            !viewport.dispatch_ime_preedit_event("must not reach blurred owner".into(), None)
        ),
        18 | 20 => assert!(viewport.dispatch_ime_preedit_event(String::new(), None)),
        19 => assert!(viewport.dispatch_ime_preedit_event("x".into(), Some((1, 1)))),
        27 | 30 => {
            // Public SetText target used by incremental commit. A changed
            // value cancels preedit; an identical controlled value must not.
            get_element_mut::<TextArea>(viewport.node_arena(), owner).set_text("MMMM  ".into());
        }
        _ => {}
    }
}

fn assert_state(viewport: &Viewport, owner: NodeKey, frame: usize) -> [f32; 3] {
    let area = get_element::<TextArea>(viewport.node_arena(), owner);
    let expected_content = match frame {
        0..=8 | 27.. => "MMMM  ",
        9..=14 => "MMMM  界",
        _ => "MMMM  界文",
    };
    let expected_preedit = match frame {
        6 | 7 => "中a",
        8 => "中b",
        11 | 14 | 26 | 29 | 30 => "文",
        19 => "x",
        _ => "",
    };
    let focused = !matches!(frame, 0 | 15 | 16);
    assert_eq!(
        area.content, expected_content,
        "committed text at frame {frame}"
    );
    assert_eq!(
        area.ime_preedit, expected_preedit,
        "preedit at frame {frame}"
    );
    assert_eq!(
        area.ime_preedit_cursor,
        (!expected_preedit.is_empty()).then_some((expected_preedit.len(), expected_preedit.len()))
    );
    assert_eq!(area.is_focused, focused, "focus at frame {frame}");
    assert_eq!(viewport.focused_node_id(), focused.then_some(owner));
    assert_eq!(area.caret_visible, focused && !matches!(frame, 22 | 23));
    if frame == 3 {
        assert_eq!(area.selection_anchor_char, Some(6));
        assert_eq!(area.selection_focus_char, Some(5));
    } else {
        assert!(area.selection_anchor_char.is_none() && area.selection_focus_char.is_none());
    }
    let (x, y, height) = area.caret_screen_position(viewport.node_arena()).unwrap();
    assert!(height > 4.0 && x >= 8.0 && x + 1.0 < 200.0 && y >= 12.0 && y + height < 80.0);
    [x, y, height]
}

#[test]
fn text_area_ime_events_preserve_valid_owner_state_through_blur_and_cancel() {
    for dpr in [1, 2] {
        let (mut viewport, owner) = scene(dpr);
        let start = crate::time::Instant::now();
        for frame in 0..FRAME_COUNT {
            let now = start + std::time::Duration::from_millis(time_ms(frame));
            apply_event(&mut viewport, owner, frame, now);
            viewport.layout_single_viewport_interaction_for_test(now);
            assert_state(&viewport, owner, frame);
        }
    }
}

fn pixel(pixels: &[u8], at: [f32; 2], dpr: u32) -> [u8; 4] {
    assert!(at.iter().all(|v| v.is_finite()));
    let x = (at[0] * dpr as f32).floor() as u32;
    let y = (at[1] * dpr as f32).floor() as u32;
    assert!(x < SIZE[0] * dpr && y < SIZE[1] * dpr);
    let i = ((y * SIZE[0] * dpr + x) * 4) as usize;
    pixels[i..i + 4].try_into().unwrap()
}

fn expect_pixel(actual: [u8; 4], expected: [u8; 4], label: &str, frame: usize, dpr: u32) {
    assert!(
        actual.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 1),
        "{label}: frame {frame} DPR {dpr}: {actual:?} != {expected:?}"
    );
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_text_area_ime_event_lifecycle_stays_retained() -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native GPU");
    let _cleanup = TextGpuCleanup;
    for mode in [
        ViewportPaintRendererMode::RetainedAuto,
        ViewportPaintRendererMode::Legacy,
    ] {
        for dpr in [1_u32, 2] {
            let (mut viewport, owner) = scene(dpr);
            viewport.set_paint_renderer_mode(mode);
            let start = crate::time::Instant::now();
            let mut previous_pixels = None;
            let mut previous_caret = None;
            let mut selection_probe = None;
            let mut targets = None;
            let mut cancel_baseline = None;
            let mut initial_focused_pixels = None;
            for frame in 0..FRAME_COUNT {
                let now = start + std::time::Duration::from_millis(time_ms(frame));
                apply_event(&mut viewport, owner, frame, now);
                viewport.begin_offscreen_test_frame(
                    gpu.device.clone(),
                    gpu.queue.clone(),
                    SIZE[0] * dpr,
                    SIZE[1] * dpr,
                    FORMAT,
                )?;
                viewport.set_scale_factor(dpr as f32);
                assert_eq!(viewport.logical_size(), (220.0, 96.0));
                let observed = viewport.render_single_viewport_interaction_frame_for_test(now)?;
                let caret = assert_state(&viewport, owner, frame);
                let pixels = read_submitted_texture(&observed.texture, gpu, SIZE.map(|v| v * dpr))?;
                if matches!(frame, 6 | 11 | 14 | 19 | 26 | 29) {
                    let before: [f32; 3] = previous_caret.unwrap();
                    // Before composition this interval is blank. Sample glyph
                    // interiors, excluding the caret and bottom underline;
                    // checking only the caret would allow missing preedit ink.
                    let left = ((before[0] + 1.0) * dpr as f32).ceil() as u32;
                    let right = ((caret[0] - 2.0) * dpr as f32).floor() as u32;
                    let top = ((caret[1] + 2.0) * dpr as f32).ceil() as u32;
                    let bottom = ((caret[1] + caret[2] - 4.0) * dpr as f32).floor() as u32;
                    assert!(left < right && top < bottom);
                    assert!(right < SIZE[0] * dpr && bottom < SIZE[1] * dpr);
                    let mut ink = 0;
                    for y in top..bottom {
                        for x in left..right {
                            let i = ((y * SIZE[0] * dpr + x) * 4) as usize;
                            ink += usize::from(pixels[i] >= 254 && pixels[i + 3] >= 64);
                        }
                    }
                    assert!(ink > 2, "preedit glyphs must paint at frame {frame}");
                }
                if frame == 4 {
                    initial_focused_pixels = Some(pixels.clone());
                } else if frame == 27 {
                    // External replacement restores the original committed
                    // value: no stale preedit glyphs, underline or caret offset.
                    assert_eq!(Some(&pixels), initial_focused_pixels.as_ref());
                }
                if matches!(frame, 10 | 18 | 28) {
                    cancel_baseline = Some(pixels.clone());
                } else if matches!(frame, 12 | 20 | 31) {
                    // The same renderer before and after cancellation must
                    // recover identical pixels; Legacy is never the oracle.
                    assert_eq!(
                        Some(&pixels),
                        cancel_baseline.as_ref(),
                        "cancel at frame {frame}"
                    );
                }
                assert!(
                    pixels
                        .chunks_exact(4)
                        .filter(|p| p[0] >= 254 && p[3] >= 127)
                        .count()
                        > 20,
                    "committed glyphs must remain visible"
                );
                expect_pixel(
                    pixel(&pixels, [210.0, 20.0], dpr),
                    [0; 4],
                    "outside right",
                    frame,
                    dpr,
                );
                expect_pixel(
                    pixel(&pixels, [20.0, 90.0], dpr),
                    [0; 4],
                    "outside bottom",
                    frame,
                    dpr,
                );
                if frame == 3 {
                    let previous: [f32; 3] = previous_caret.unwrap();
                    assert!(
                        previous[0] - caret[0] > 3.0,
                        "selected blank must contain an interior sample"
                    );
                    selection_probe = Some([(previous[0] + caret[0]) * 0.5, caret[1] + 2.0]);
                }
                if let Some(at) = selection_probe {
                    expect_pixel(
                        pixel(&pixels, at, dpr),
                        if frame == 3 { [0, 0, 255, 128] } else { [0; 4] },
                        "selection over the trailing blank",
                        frame,
                        dpr,
                    );
                }
                if frame != 0 && frame != 3 {
                    let physical_x = (caret[0] * dpr as f32 + 0.5).floor();
                    let center_x = (physical_x + 0.5) / dpr as f32;
                    assert!(center_x >= caret[0] && center_x < caret[0] + 1.0);
                    let visible = !matches!(frame, 15 | 16 | 22 | 23);
                    let expected = if visible {
                        // Same analytic rectangle coverage and RGBA8/group
                        // quantization as the existing caret geometry gates.
                        let distance = (center_x - (caret[0] + 0.5)).abs() - 0.5;
                        let aa = 1.0 / dpr as f32;
                        let t = ((aa - distance) / (2.0 * aa)).clamp(0.0, 1.0);
                        let coverage = t * t * (3.0 - 2.0 * t);
                        [255, 0, 0, ((coverage * 255.0).round() * 0.5).round() as u8]
                    } else {
                        [0; 4]
                    };
                    expect_pixel(
                        pixel(&pixels, [center_x, caret[1] + 2.0], dpr),
                        expected,
                        "caret visibility",
                        frame,
                        dpr,
                    );
                }
                if is_warm(frame) {
                    assert_eq!(
                        Some(&pixels),
                        previous_pixels.as_ref(),
                        "unchanged frame {frame}"
                    );
                }
                previous_pixels = Some(pixels);
                previous_caret = Some(caret);
                if mode == ViewportPaintRendererMode::RetainedAuto {
                    assert!(observed.artifact_selected);
                    assert_eq!(
                        observed.actions,
                        [if is_warm(frame) {
                            RetainedSurfaceCompileAction::Reuse
                        } else {
                            RetainedSurfaceCompileAction::Reraster
                        }],
                        "frame {frame} DPR {dpr}"
                    );
                    assert_eq!(observed.color_targets.len(), 1);
                    // This gate proves stable allocation through interactions,
                    // not an exact text-envelope policy. The measured target
                    // must contain the 200x80 owner and fit this fixture's
                    // 220x96 extent; target identity and descriptors stay fixed.
                    for (key, desc) in &observed.color_targets {
                        assert_eq!(desc.origin(), (0, 0));
                        assert!((200 * dpr..=SIZE[0] * dpr).contains(&desc.width()));
                        assert!((80 * dpr..=SIZE[1] * dpr).contains(&desc.height()));
                        assert_eq!(
                            observed.texture_bytes,
                            u64::from(desc.width()) * u64::from(desc.height()) * 4
                        );
                        assert!(viewport.has_compatible_persistent_render_target(*key, desc));
                    }
                    if let Some(first) = &targets {
                        assert_eq!(first, &observed.color_targets);
                    } else {
                        targets = Some(observed.color_targets);
                    }
                } else {
                    assert!(observed.legacy_selected);
                }
                eprintln!("IME lifecycle {mode:?} dpr={dpr} frame={frame} passed");
            }
        }
    }
    Ok(())
}
