use super::style_pipeline_tests::read_submitted_texture;
use super::*;
use crate::style::Opacity;
use crate::view::test_support::{commit_child, commit_element, get_element_mut};
use crate::view::viewport::ViewportPaintRendererMode;

const EXTENT: [u32; 2] = [220, 80];

struct TextGpuCleanup;
impl Drop for TextGpuCleanup {
    fn drop(&mut self) {
        crate::view::render_pass::text_pass::clear_text_resources_cache();
    }
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_text_preparation_loss_recovers_without_changing_content() -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native GPU");
    let _cleanup = TextGpuCleanup;
    for mode in [
        ViewportPaintRendererMode::RetainedAuto,
        ViewportPaintRendererMode::Legacy,
    ] {
        for dpr in [1_u32, 2] {
            let mut arena = NodeArena::new();
            let mut root = Element::new_with_id(0xc0_1000, 0.0, 0.0, 200.0, 64.0);
            let mut style = Style::new();
            style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
            style.insert(PropertyId::Opacity, ParsedValue::Opacity(Opacity::new(0.5)));
            root.apply_style(style);
            let root = commit_element(&mut arena, Box::new(root));
            let mut text = Text::new_with_id(0xc0_1001, 8.0, 8.0, 180.0, 32.0, "MMMM");
            text.set_font("sans-serif");
            text.set_font_size(24.0);
            text.set_color(Color::rgb(255, 0, 0));
            let text = commit_child(&mut arena, root, Box::new(text));
            let mut viewport = Viewport::new();
            viewport.set_paint_renderer_mode(mode);
            viewport.install_single_viewport_scene_for_test(arena, root);
            let mut previous_pixels = None;
            let mut targets = None;
            // Ordered sequence: initial/warm, loss/recovery, edit/warm,
            // loss/recovery, color/warm, edit/warm, loss/recovery.
            for frame in 0..14 {
                match frame {
                    2 | 6 | 12 => {
                        // Inject only loss of CPU preparation. No source edit,
                        // geometry change or explicit layout invalidation.
                        get_element_mut::<Text>(viewport.node_arena(), text)
                            .clear_prepared_standalone_text_for_test();
                    }
                    4 => get_element_mut::<Text>(viewport.node_arena(), text).set_text("IIII"),
                    8 => get_element_mut::<Text>(viewport.node_arena(), text)
                        .set_color(Color::rgb(0, 0, 255)),
                    10 => get_element_mut::<Text>(viewport.node_arena(), text).set_text("MMMM"),
                    _ => {}
                }
                viewport.begin_offscreen_test_frame(
                    gpu.device.clone(),
                    gpu.queue.clone(),
                    EXTENT[0] * dpr,
                    EXTENT[1] * dpr,
                    FORMAT,
                )?;
                viewport.set_scale_factor(dpr as f32);
                assert_eq!(viewport.logical_size(), (220.0, 80.0));
                let observed = viewport.render_single_viewport_scene_for_test()?;
                let pixels =
                    read_submitted_texture(&observed.texture, gpu, EXTENT.map(|v| v * dpr))?;
                let channel = if frame < 8 { 0 } else { 2 };
                let mut solid_ink = 0;
                for (i, p) in pixels.chunks_exact(4).enumerate() {
                    let x = (i as u32 % (EXTENT[0] * dpr)) as f32 / dpr as f32;
                    let y = (i as u32 / (EXTENT[0] * dpr)) as f32 / dpr as f32;
                    if p[3] != 0 {
                        assert!(
                            x >= 8.0 && x < 188.0 && y >= 8.0 && y < 40.0,
                            "glyph outside its authored box: {mode:?}/{dpr}/{frame} ({x},{y}) {p:?}"
                        );
                        assert!(p[3] <= 129, "group opacity applied incorrectly: {p:?}");
                        assert_eq!(p[1], 0);
                        assert_eq!(p[2 - channel], 0);
                        if p[3] >= 127 {
                            assert!(p[channel] >= 254, "wrong full-coverage glyph color: {p:?}");
                            solid_ink += 1;
                        }
                    }
                }
                assert!(
                    solid_ink > 20 * dpr * dpr,
                    "must actually draw glyph interiors"
                );
                if let Some(previous) = &previous_pixels {
                    if matches!(frame, 4 | 8 | 10) {
                        assert_ne!(
                            &pixels, previous,
                            "authored edit must change visible output"
                        );
                    } else {
                        // A same-path temporal check supplements the independent
                        // color/coverage assertions; Legacy is never an oracle.
                        assert_eq!(&pixels, previous, "warm/recovered pixels must be unchanged");
                    }
                }
                previous_pixels = Some(pixels);
                if mode == ViewportPaintRendererMode::RetainedAuto {
                    assert!(observed.artifact_selected);
                    let expected = if matches!(frame, 0 | 4 | 8 | 10) {
                        RetainedSurfaceCompileAction::Reraster
                    } else {
                        RetainedSurfaceCompileAction::Reuse
                    };
                    assert_eq!(observed.actions, [expected], "{mode:?}/{dpr}/{frame}");
                    assert_eq!(observed.texture_bytes, 200 * 64 * 4 * u64::from(dpr * dpr));
                    assert_eq!(observed.color_targets.len(), 1);
                    for (key, desc) in &observed.color_targets {
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
                eprintln!("text preparation recovery {mode:?} dpr={dpr} frame={frame} passed");
            }
        }
    }
    Ok(())
}
