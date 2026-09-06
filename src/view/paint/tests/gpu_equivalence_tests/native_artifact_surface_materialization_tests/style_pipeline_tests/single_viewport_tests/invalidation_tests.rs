use super::*;

#[derive(Clone, Copy)]
struct FrameChange {
    label: &'static str,
    mode: ViewportPaintRendererMode,
    tx: f32,
    rgb: [u8; 3],
    opacity: f32,
    dpr: u32,
    expected_action: RetainedSurfaceCompileAction,
    expected_pixel: [u8; 4],
}

const BASE: FrameChange = FrameChange {
    label: "cold red",
    mode: ViewportPaintRendererMode::RetainedAuto,
    tx: 9.0,
    rgb: [224, 36, 28],
    opacity: 0.5,
    dpr: 1,
    expected_action: RetainedSurfaceCompileAction::Reraster,
    expected_pixel: [190, 4, 3, 128],
};

fn content_and_scale_changes() -> Vec<FrameChange> {
    let moved = FrameChange {
        label: "placement only",
        tx: 17.0,
        expected_action: RetainedSurfaceCompileAction::Reuse,
        ..BASE
    };
    let blue = FrameChange {
        label: "content red to blue",
        rgb: [24, 72, 224],
        expected_pixel: [2, 17, 190, 128],
        expected_action: RetainedSurfaceCompileAction::Reraster,
        ..moved
    };
    let faded = FrameChange {
        label: "opacity only",
        opacity: 0.25,
        // RGBA8 stores the premultiplied blue as [1, 4, 48, 64]. Presentation
        // divides RGB by stored alpha: round([1,4,48] * 255 / 64).
        // Keep the 1-LSB tolerance; do not compare to unquantized straight RGB.
        expected_pixel: [4, 16, 191, 64],
        expected_action: RetainedSurfaceCompileAction::Reuse,
        ..blue
    };
    let scaled = FrameChange {
        label: "DPR 1 to 2",
        dpr: 2,
        expected_action: RetainedSurfaceCompileAction::Reraster,
        ..faded
    };
    vec![
        BASE,
        moved,
        blue,
        FrameChange {
            label: "unchanged blue",
            expected_action: RetainedSurfaceCompileAction::Reuse,
            ..blue
        },
        faded,
        scaled,
        FrameChange {
            label: "DPR 2 placement",
            tx: 9.0,
            expected_action: RetainedSurfaceCompileAction::Reuse,
            ..scaled
        },
        FrameChange {
            label: "DPR 2 to 1",
            tx: 9.0,
            dpr: 1,
            ..scaled
        },
    ]
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_single_viewport_content_opacity_and_dpr_invalidation() -> Result<(), String> {
    run_changes(&content_and_scale_changes())
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_single_viewport_legacy_content_opacity_and_dpr_pixels() -> Result<(), String> {
    let mut changes = content_and_scale_changes();
    for change in &mut changes {
        change.mode = ViewportPaintRendererMode::Legacy;
    }
    run_changes(&changes)
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_single_viewport_mode_switch_discards_stale_retained_content() -> Result<(), String> {
    let legacy_blue = FrameChange {
        label: "legacy blue",
        mode: ViewportPaintRendererMode::Legacy,
        rgb: [24, 72, 224],
        expected_pixel: [2, 17, 190, 128],
        ..BASE
    };
    let mut changes = [
        BASE,
        FrameChange {
            label: "warm retained red",
            expected_action: RetainedSurfaceCompileAction::Reuse,
            ..BASE
        },
        legacy_blue,
        FrameChange {
            label: "return to retained blue",
            mode: ViewportPaintRendererMode::RetainedAuto,
            ..legacy_blue
        },
        FrameChange {
            label: "warm retained blue",
            mode: ViewportPaintRendererMode::RetainedAuto,
            expected_action: RetainedSurfaceCompileAction::Reuse,
            ..legacy_blue
        },
    ];
    for dpr in [1, 2] {
        for change in &mut changes {
            change.dpr = dpr;
        }
        run_changes(&changes)?;
    }
    Ok(())
}

// These are ordered frames on one persistent viewport, not independent cases.
// Labels and expected actions describe changes relative to the preceding frame;
// inserting or reordering a frame requires rechecking both adjacent transitions.
fn run_changes(changes: &[FrameChange]) -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native hardware graphics context");
    let mut viewport = Viewport::new();
    let (root, mut style) = install_translucent_scene(&mut viewport);
    for (frame, change) in changes.iter().enumerate() {
        if viewport.paint_renderer_mode() != change.mode {
            viewport.set_paint_renderer_mode(change.mode);
        }
        style.set_transform(Transform::new([Translate::xy(
            Length::px(change.tx),
            Length::px(4.0),
        )]));
        style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::rgb(change.rgb[0], change.rgb[1], change.rgb[2])),
        );
        style.insert(
            PropertyId::Opacity,
            ParsedValue::Opacity(Opacity::new(change.opacity)),
        );
        get_element_mut::<Element>(viewport.node_arena(), root).apply_style(style.clone());
        viewport.begin_offscreen_test_frame(
            gpu.device.clone(),
            gpu.queue.clone(),
            WIDTH * change.dpr,
            HEIGHT * change.dpr,
            FORMAT,
        )?;
        // Offscreen frame acquisition resets DPR; restore it before layout.
        viewport.set_scale_factor(change.dpr as f32);
        assert_eq!(viewport.scale_factor(), change.dpr as f32);
        assert_eq!(viewport.logical_size(), (WIDTH as f32, HEIGHT as f32));
        let observed = viewport.render_single_viewport_scene_for_test()?;
        assert_eq!(observed.frame_number, frame as u64 + 1);
        let pixels = read_submitted_texture(
            &observed.texture,
            gpu,
            [WIDTH * change.dpr, HEIGHT * change.dpr],
        )?;
        // Validate pixels before reuse counters: stale content is a correctness
        // failure even if the action and allocation accounting look plausible.
        for (probe_index, (x, y, expected)) in
            style_pixel_probes(StyleScene::TranslucentFill, [change.tx, 4.0], 0.0)?
                .into_iter()
                .enumerate()
        {
            // TranslucentFill's first probe samples the uniform fill interior.
            // Preserve every other probe's own expectation, regardless of alpha.
            let expected = if probe_index == 0 {
                change.expected_pixel
            } else {
                expected
            };
            let i = ((y * change.dpr * WIDTH * change.dpr + x * change.dpr) * 4) as usize;
            let actual: [u8; 4] = pixels[i..i + 4].try_into().unwrap();
            if actual
                .into_iter()
                .zip(expected)
                .any(|(a, e)| a.abs_diff(e) > 1)
            {
                return Err(format!(
                    "{} {:?} frame {frame} @({x},{y}): {actual:?}, expected {expected:?}",
                    change.label, change.mode
                ));
            }
        }
        if change.mode == ViewportPaintRendererMode::RetainedAuto {
            assert!(
                observed.artifact_selected,
                "{}: must select artifact",
                change.label
            );
            assert_eq!(
                observed.actions,
                [change.expected_action],
                "{}",
                change.label
            );
            assert_eq!(
                observed.texture_bytes,
                20 * 16 * 12 * u64::from(change.dpr * change.dpr),
                "{}",
                change.label
            );
            assert_eq!(observed.color_targets.len(), 1);
            let (key, desc) = &observed.color_targets[0];
            assert_eq!(
                (desc.width(), desc.height()),
                (20 * change.dpr, 16 * change.dpr),
                "{}",
                change.label
            );
            assert!(viewport.has_compatible_persistent_render_target_pair(*key, desc));
        } else {
            assert!(observed.legacy_selected, "{}", change.label);
        }
    }
    eprintln!(
        "single Viewport invalidation sequence passed on {}",
        gpu.label()
    );
    Ok(())
}
