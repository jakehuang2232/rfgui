use super::*;
use rfgui::view::gpu_paint::GpuPaintSource;

fn animation_state(viewport: &Viewport) -> (usize, Option<GpuPaintSource>) {
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
            .is::<pages::custom_gpu::particle_demo::ParticleCanvas>()
        {
            particle = node.element.prepared_gpu_paint_source().cloned();
        }
    }
    (animators, particle)
}

/// The keyframe demo's animator on the Animation page, and the particles
/// on the Custom GPU Paint page.
#[test]
#[ignore = "requires native hardware graphics adapter"]
fn inspector_animation_switch_stops_and_restarts_demo_styles_and_particle_updates()
-> Result<(), String> {
    let gpu = gpu::Gpu::new()?;
    for (page, animators_on, particles) in [(Page::Animation, 1, false), (Page::CustomGpu, 0, true)]
    {
        for dpr in [1_u32, 2] {
            let case = 110 + dpr + 10 * page as u32;
            let mut viewport = Viewport::new();
            let now = rfgui::time::Instant::now();
            let mut root = None;
            let mut frozen = None;
            for frame in 0..11_u64 {
                // The Inspector opens from the sidebar.
                let label = if frame == 1 {
                    "Inspector"
                } else {
                    "Animation On"
                };
                if matches!(frame, 1 | 2 | 6 | 8) {
                    press_label(&mut viewport, label);
                    pointer_up_and_click(&mut viewport);
                }
                if root.is_none() || rfgui::ui::peek_state_dirty().needs_rebuild() {
                    root = Some(scene(case, page));
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
                assert_eq!(
                    source.is_some(),
                    particles,
                    "{page:?} DPR={dpr} frame={frame}"
                );
                if matches!(frame, 0 | 1 | 6 | 7) {
                    assert_eq!(
                        animators, animators_on,
                        "demo styles must be present: {page:?} DPR={dpr} frame={frame}"
                    );
                    if frame == 7 && particles {
                        assert_ne!(frozen, source);
                    }
                } else {
                    assert_eq!(
                        animators, 0,
                        "demo Animation styles must be removed: {page:?} DPR={dpr} frame={frame}"
                    );
                    if matches!(frame, 2 | 8) {
                        frozen = source.clone();
                    }
                    assert_eq!(
                        frozen, source,
                        "paused particles must keep the same prepared content"
                    );
                    if matches!(frame, 4 | 5 | 10) {
                        assert!(
                            !viewport.is_animating(),
                            "transitions settle without disabling engine animations: {page:?} DPR={dpr} frame={frame}"
                        );
                    }
                }
            }
        }
    }
    Ok(())
}
