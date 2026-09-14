use super::*;
use rfgui::view::gpu_paint::GpuPaintSource;

fn animation_state(viewport: &Viewport) -> (usize, GpuPaintSource) {
    let arena = viewport.node_arena();
    let mut pending = arena.roots().to_vec();
    let mut animators = 0;
    let mut particle = None;
    while let Some(key) = pending.pop() {
        pending.extend(arena.children_of(key));
        let node = arena.get(key).unwrap();
        animators += usize::from(node.element.has_active_animator());
        if node
            .element
            .as_any()
            .is::<scene_windows::particle_demo::ParticleCanvas>()
        {
            particle = node.element.prepared_gpu_paint_source().cloned();
        }
    }
    (animators, particle.expect("mounted particle source"))
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn inspector_animation_switch_stops_and_restarts_demo_styles_and_particle_updates()
-> Result<(), String> {
    let gpu = gpu::Gpu::new()?;
    for mode in [
        ViewportPaintRendererMode::Legacy,
        ViewportPaintRendererMode::RetainedAuto,
    ] {
        for dpr in [1_u32, 2] {
            let case = 100
                + dpr
                + if mode == ViewportPaintRendererMode::Legacy {
                    0
                } else {
                    10
                };
            let mut viewport = Viewport::new();
            viewport.set_paint_renderer_mode(mode);
            let now = rfgui::time::Instant::now();
            let mut root = None;
            let mut frozen = None;
            for frame in 0..11_u64 {
                match frame {
                    1 => {
                        // Bring the real Inspector to the front before clicking.
                        pointer_down(&mut viewport, 188., 60.);
                        pointer_up_and_click(&mut viewport);
                    }
                    2 | 6 | 8 => {
                        press_label(&mut viewport, "Animaton On");
                        pointer_up_and_click(&mut viewport);
                    }
                    _ => {}
                }
                if root.is_none() || rfgui::ui::peek_state_dirty().needs_rebuild() {
                    root = Some(scene(case));
                }
                viewport.render_rsx_offscreen_for_test(
                    root.as_ref().unwrap(),
                    gpu.device.clone(),
                    gpu.queue.clone(),
                    [1280 * dpr, 800 * dpr],
                    dpr as f32,
                    now + std::time::Duration::from_secs(frame),
                )?;
                let (animators, source) = animation_state(&viewport);
                if matches!(frame, 0 | 1 | 6 | 7) {
                    assert_eq!(
                        animators, 2,
                        "both demo styles must be present: {mode:?} DPR={dpr} frame={frame}"
                    );
                    if frame == 7 {
                        assert_ne!(frozen.as_ref(), Some(&source));
                    }
                } else {
                    assert_eq!(
                        animators, 0,
                        "demo Animation styles must be removed: {mode:?} DPR={dpr} frame={frame}"
                    );
                    if matches!(frame, 2 | 8) {
                        frozen = Some(source.clone());
                    }
                    assert_eq!(
                        frozen.as_ref(),
                        Some(&source),
                        "paused particles must keep the same prepared content"
                    );
                    if matches!(frame, 4 | 5 | 10) {
                        assert!(
                            !viewport.is_animating(),
                            "transitions settle without disabling engine animations"
                        );
                    }
                }
            }
        }
    }
    Ok(())
}
