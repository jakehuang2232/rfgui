use super::*;
use rfgui::style::{Color, Layout, Length, Opacity, Position};
use rfgui::view::gpu_paint::GpuPaintWork;
use rfgui::view::viewport::ViewportPaintRendererMode;
use rfgui::view::{Element, Viewport};
mod gpu;
fn tree() -> RsxNode {
    rfgui::ui::rsx_scope(|| {
        rfgui::ui::rsx! {
            <Element style={{width:Length::px(160.),height:Length::px(96.),layout:Layout::Grid}}>
                <Element style={{width:Length::px(80.),height:Length::px(64.),layout:Layout::Grid,position:Position::absolute().left(Length::px(0.)).top(Length::px(0.))}}>
                    {host_builder_node::<ParticleCanvas>("ParticleCanvas")}
                </Element>
                <Element style={{width:Length::px(20.),height:Length::px(16.),position:Position::absolute().left(Length::px(100.)).top(Length::px(8.)),background:Color::rgb(0,0,255),opacity:Opacity::new(0.5)}} />
            </Element>
        }
    })
}
#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_particle_canvas_changes_pixels_while_native_raster_reuses() -> Result<(), String> {
    let gpu = gpu::Gpu::new()?;
    for mode in [
        ViewportPaintRendererMode::RetainedAuto,
        ViewportPaintRendererMode::Legacy,
    ] {
        for dpr in [1, 2] {
            for trace in [false, true] {
                let now = Instant::now();
                PARTICLE_SYSTEM.with(|s| {
                    let mut s = s.borrow_mut();
                    *s = ParticleSystemInner::new();
                    s.last_update = now;
                    s.particles.push(Particle {
                        x: 0.3,
                        y: 0.5,
                        z: 0.,
                        vx: 0.8,
                        vy: 0.,
                        vz: 0.,
                        color: [1., 0., 0., 1.],
                        size_norm: 0.3,
                        life: 5.,
                        max_life: 5.,
                    });
                });
                let mut v = Viewport::new();
                v.set_paint_renderer_mode(mode);
                let mut options = v.debug_options();
                options.trace_render_time = trace;
                v.set_debug_options(options);
                let mut previous = None;
                let mut changes = 0;
                for frame in 0..6 {
                    let o = v.render_rsx_offscreen_for_test(
                        &tree(),
                        gpu.device.clone(),
                        gpu.queue.clone(),
                        [160 * dpr, 96 * dpr],
                        dpr as f32,
                        now + std::time::Duration::from_millis((frame + 1) * 32),
                    )?;
                    let pixels = gpu.read(&o.texture, [160 * dpr, 96 * dpr])?;
                    for ([x, y], expected) in
                        [([104, 12], [0, 0, 255, 128]), ([84, 84], [0, 0, 0, 0])]
                    {
                        let at = ((y * dpr * 160 * dpr + x * dpr) * 4) as usize;
                        assert!(
                            pixels[at..at + 4]
                                .iter()
                                .zip(expected)
                                .all(|(a, b)| a.abs_diff(b) <= 1),
                            "native/clear probe {mode:?} DPR={dpr} frame={frame}"
                        );
                    }
                    let mut region = Vec::new();
                    for y in 0..64 * dpr {
                        let at = (y * 160 * dpr * 4) as usize;
                        region.extend_from_slice(&pixels[at..at + (80 * dpr * 4) as usize]);
                    }
                    assert!(
                        region.chunks_exact(4).any(|p| p[3] > 0),
                        "real particle shader must paint visible content"
                    );
                    if let Some(old) = previous.replace(region.clone()) {
                        if old != region {
                            changes += 1;
                        }
                    }
                    let sources = v.gpu_paint_observations();
                    assert_eq!(sources.len(), 1);
                    assert!(sources[0].valid_resident);
                    assert_eq!(sources[0].work, Some(GpuPaintWork::Rendered));
                    assert_eq!(sources[0].revision, frame + 1);
                    if mode == ViewportPaintRendererMode::RetainedAuto {
                        assert!(o.artifact_selected);
                        assert_eq!(o.resident_pairs, 1);
                        assert_eq!(o.rerasterizations, usize::from(frame == 0));
                        assert_eq!(o.reuses, usize::from(frame > 0));
                    }
                }
                assert!(
                    changes >= 4,
                    "particle pixels must keep changing independently of native reuse"
                );
            }
        }
    }
    Ok(())
}
