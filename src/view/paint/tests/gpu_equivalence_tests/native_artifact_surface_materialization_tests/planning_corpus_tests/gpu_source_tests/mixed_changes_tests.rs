use super::lifecycle_tests::*;
use super::*;
use crate::view::frame_graph::execution_failure_test_support::arm_after;
use crate::view::gpu_paint::GpuPaintWork;
use crate::view::test_support::get_element_mut;

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_gpu_sources_update_independently_and_recover_from_execution_failure() -> Result<(), String>
{
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().unwrap();
    for dpr in [1, 2] {
        let (mut arena, root, host, native, _group) = direct_scene();
        let group2 = commit_child(
            &mut arena,
            root,
            Box::new(element(
                0x6e10,
                [20., 16.],
                style([20., 16.], Some([60., 8.]), None),
            )),
        );
        let other = commit_child(&mut arena, group2, Box::new(GpuHost::new(0x6e11)));
        let mut v = Viewport::new();
        v.set_paint_renderer_mode(ViewportPaintRendererMode::RetainedAuto);
        v.install_single_viewport_scene_for_test(arena, root);
        let mut steps = 0;
        // Source A changes twice as frequently as B. Native-only edits never
        // enter either source's identity. Failure follows actual source work.
        for frame in 0..8 {
            for (key, change) in [(host, matches!(frame, 2 | 4)), (other, frame == 4)] {
                if change {
                    let mut n = v.node_arena().get_mut(key).unwrap();
                    let h = n.element.as_any_mut().downcast_mut::<GpuHost>().unwrap();
                    h.color = [0., 1., 0., 1.];
                    h.revision += 1;
                    h.dirty = DirtyFlags::PAINT;
                }
            }
            if frame == 3 {
                get_element_mut::<Element>(v.node_arena(), native).apply_style(style(
                    [16., 16.],
                    Some([40., 8.]),
                    Some(RED),
                ));
            }
            if frame == 6 {
                v.set_paint_renderer_mode(ViewportPaintRendererMode::RetainedAuto);
            }
            begin(&mut v, gpu, dpr)?;
            if frame == 4 {
                // Last successful execution position guarantees the producer
                // has run. Its pending green result must not become resident.
                let fault = arm_after(steps);
                v.render_single_viewport_execution_failure_for_test()?;
                assert!(fault.fired());
                assert!(v.gpu_paint_observations().iter().all(|s| !s.valid_resident));
                continue;
            }
            let counter = arm_after(usize::MAX);
            let observed = if frame == 5 {
                v.render_single_viewport_legacy_recovery_for_test()?
            } else {
                v.render_single_viewport_scene_for_test()?
            };
            if frame == 2 {
                steps = counter.steps();
                assert!(steps > 2);
            }
            drop(counter);
            let pixels = read_submitted_texture(&observed.texture, gpu, [80 * dpr, 64 * dpr])?;
            probe(
                &pixels,
                dpr,
                [8, 12],
                if frame >= 2 {
                    [0, 255, 0, 255]
                } else {
                    [255, 0, 0, 255]
                },
                "source A",
            );
            probe(
                &pixels,
                dpr,
                [64, 12],
                if frame >= 4 {
                    [0, 255, 0, 255]
                } else {
                    [255, 0, 0, 255]
                },
                "source B",
            );
            probe(
                &pixels,
                dpr,
                [44, 12],
                if frame >= 3 {
                    [255, 0, 0, 128]
                } else {
                    [0, 0, 255, 128]
                },
                "native content",
            );
            let sources = v.gpu_paint_observations();
            assert_eq!(sources.len(), 2);
            assert!(sources.iter().all(|s| s.valid_resident));
            let expected = match frame {
                0 | 5 => [GpuPaintWork::Rendered; 2],
                2 => [GpuPaintWork::Rendered, GpuPaintWork::Reused],
                _ => [GpuPaintWork::Reused; 2],
            };
            assert_eq!(
                sources.iter().map(|s| s.work.unwrap()).collect::<Vec<_>>(),
                expected,
                "source work frame={frame}"
            );
            if frame == 5 {
                assert!(observed.legacy_selected);
            } else {
                assert!(observed.artifact_selected);
                assert_eq!(
                    observed.actions,
                    [if matches!(frame, 0 | 3 | 6) {
                        RetainedSurfaceCompileAction::Reraster
                    } else {
                        RetainedSurfaceCompileAction::Reuse
                    }],
                    "native work frame={frame}"
                );
            }
        }
    }
    Ok(())
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_gpu_source_visibility_resize_and_opacity_use_current_pixels() -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().unwrap();
    for mode in [
        ViewportPaintRendererMode::RetainedAuto,
        ViewportPaintRendererMode::Legacy,
    ] {
        for dpr in [1, 2] {
            let (arena, root, host, _, group) = direct_scene();
            let mut v = Viewport::new();
            v.set_paint_renderer_mode(mode);
            v.install_single_viewport_scene_for_test(arena, root);
            for frame in 0..8 {
                if frame == 2 {
                    get_element_mut::<Element>(v.node_arena(), group).apply_style(style(
                        [20., 16.],
                        Some([12., 8.]),
                        None,
                    ));
                }
                if frame == 3 || frame == 6 {
                    let size = if frame == 6 { [24., 20.] } else { [20., 16.] };
                    let mut s = style(size, Some([12., 8.]), None);
                    s.insert(PropertyId::Opacity, ParsedValue::Opacity(Opacity::new(0.5)));
                    get_element_mut::<Element>(v.node_arena(), group).apply_style(s);
                    if frame == 6 {
                        let mut n = v.node_arena().get_mut(host).unwrap();
                        let h = n.element.as_any_mut().downcast_mut::<GpuHost>().unwrap();
                        h.size = size;
                        h.dirty = DirtyFlags::ALL;
                    }
                }
                if frame == 4 || frame == 5 {
                    let mut n = v.node_arena().get_mut(host).unwrap();
                    let h = n.element.as_any_mut().downcast_mut::<GpuHost>().unwrap();
                    h.visible = frame == 5;
                    h.dirty = DirtyFlags::PAINT;
                }
                begin(&mut v, gpu, dpr)?;
                let observed = v.render_single_viewport_scene_for_test()?;
                let pixels = read_submitted_texture(&observed.texture, gpu, [80 * dpr, 64 * dpr])?;
                probe(
                    &pixels,
                    dpr,
                    [if frame >= 2 { 16 } else { 8 }, 12],
                    if frame == 4 {
                        [0; 4]
                    } else {
                        [255, 0, 0, if frame >= 3 { 128 } else { 255 }]
                    },
                    "source visibility and group opacity",
                );
                probe(
                    &pixels,
                    dpr,
                    [34, 26],
                    if frame >= 6 { [255, 0, 0, 128] } else { [0; 4] },
                    "newly exposed resized source",
                );
                probe(
                    &pixels,
                    dpr,
                    [44, 12],
                    [0, 0, 255, 128],
                    "native remains unchanged",
                );
                let sources = v.gpu_paint_observations();
                if frame == 4 {
                    assert!(sources.is_empty());
                } else {
                    assert_eq!(sources.len(), 1);
                    assert!(sources[0].valid_resident);
                    let work = if matches!(frame, 0 | 5 | 6) {
                        Some(GpuPaintWork::Rendered)
                    } else if frame == 7 && mode == ViewportPaintRendererMode::RetainedAuto {
                        None
                    } else {
                        Some(GpuPaintWork::Reused)
                    };
                    assert_eq!(sources[0].work, work, "{mode:?} frame={frame}");
                }
                if mode == ViewportPaintRendererMode::RetainedAuto {
                    assert!(observed.artifact_selected);
                    if frame > 0 {
                        assert!(
                            observed
                                .actions
                                .contains(&RetainedSurfaceCompileAction::Reuse),
                            "independent native remains reusable: frame={frame} actions={:?}",
                            observed.actions
                        );
                    }
                }
            }
        }
    }
    Ok(())
}
