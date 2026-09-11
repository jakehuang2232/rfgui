use super::super::style_pipeline_tests::read_submitted_texture;
use super::*;
use crate::view::viewport::ViewportPaintRendererMode;

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_single_viewport_text_area_caret_selection_ime_artifact() -> Result<(), String> {
    run_frames(ViewportPaintRendererMode::RetainedAuto)
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_single_viewport_text_area_caret_selection_ime_legacy() -> Result<(), String> {
    run_frames(ViewportPaintRendererMode::Legacy)
}

fn run_frames(mode: ViewportPaintRendererMode) -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().expect("native GPU");
    let _cleanup = TextGpuCleanup;
    for case in 0..4 {
        for dpr in [1_u32, 2] {
            // Fixture setup lays out the navigation/IME selection once. After
            // transfer, every frame re-enters production layout and paint in
            // this same Viewport. Read navigation geometry after that layout:
            // an outer scroll scope can change projected text line wrapping.
            let (arena, roots, caret) = fixture(case);
            let text_owner = arena.children_of(roots[0])[0];
            // Explicit outer scroll scope exercises the old selector's
            // TextArea exclusion, in addition to the child's own paint state.
            let mut outer_style = Style::new();
            outer_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
            outer_style.insert(PropertyId::Opacity, ParsedValue::Opacity(Opacity::new(0.5)));
            outer_style.insert(
                PropertyId::ScrollDirection,
                ParsedValue::ScrollDirection(ScrollDirection::Vertical),
            );
            crate::view::test_support::get_element_mut::<Element>(&arena, roots[0])
                .apply_style(outer_style);
            let mut viewport = Viewport::new();
            viewport.set_paint_renderer_mode(mode);
            viewport.install_single_viewport_forest_for_test(arena, roots);
            let mut targets = None;
            for frame in 0..3 {
                eprintln!("single Viewport TextArea {mode:?} case={case} DPR={dpr} frame={frame}");
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
                let pixels =
                    read_submitted_texture(&observed.texture, gpu, EXTENT.map(|v| v * dpr))?;
                let current_caret = if case == 1 {
                    caret // The selection-over-spaces probe is fixture geometry.
                } else {
                    let node = viewport.node_arena().get(text_owner).unwrap();
                    let text = node.element.as_any().downcast_ref::<TextArea>().unwrap();
                    let (x, y, height) = text.caret_screen_position(viewport.node_arena()).unwrap();
                    assert!(height > 4.0);
                    [x, y + 2.0]
                };
                // Colors and rectangle coverage still come from the same
                // independent oracle; no recorded op/readback supplies them.
                check_pixels(&pixels, current_caret, dpr, case);
                if mode == ViewportPaintRendererMode::Legacy {
                    assert!(observed.legacy_selected);
                } else {
                    assert!(observed.artifact_selected);
                    assert!(
                        !observed.actions.is_empty(),
                        "must use common artifact pool execution"
                    );
                    assert!(
                        observed.actions.iter().all(|a| *a
                            == if frame == 0 {
                                RetainedSurfaceCompileAction::Reraster
                            } else {
                                RetainedSurfaceCompileAction::Reuse
                            }),
                        "{case}/{dpr}/{frame}: {:?}",
                        observed.actions
                    );
                    assert_eq!(observed.actions.len(), observed.color_targets.len());
                    for (key, desc) in &observed.color_targets {
                        assert!(viewport.has_compatible_persistent_render_target_pair(*key, desc));
                    }
                    if let Some(first) = &targets {
                        assert_eq!(first, &observed.color_targets);
                    } else {
                        targets = Some(observed.color_targets);
                    }
                }
            }
        }
    }
    Ok(())
}
