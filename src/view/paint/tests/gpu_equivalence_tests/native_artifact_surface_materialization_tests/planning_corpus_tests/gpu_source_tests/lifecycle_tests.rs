use super::*;
use crate::view::gpu_paint::GpuPaintWork;
use crate::view::test_support::get_element_mut;

pub(super) fn direct_scene() -> (NodeArena, NodeKey, NodeKey, NodeKey, NodeKey) {
    let (arena, root, host, native) = scene();
    let group = arena.parent_of(host).unwrap();
    let mut plain = style([20., 16.], Some([4., 8.]), None);
    plain.insert(PropertyId::Opacity, ParsedValue::Opacity(Opacity::new(1.)));
    get_element_mut::<Element>(&arena, group).apply_style(plain);
    (arena, root, host, native, group)
}
pub(super) fn begin(v: &mut Viewport, gpu: &NativeGpu, dpr: u32) -> Result<(), String> {
    v.begin_offscreen_test_frame(
        gpu.device.clone(),
        gpu.queue.clone(),
        80 * dpr,
        64 * dpr,
        FORMAT,
    )?;
    v.set_scale_factor(dpr as f32);
    assert_eq!(v.logical_size(), (80., 64.));
    Ok(())
}
pub(super) fn probe(pixels: &[u8], dpr: u32, point: [u32; 2], expected: [u8; 4], label: &str) {
    let at = ((point[1] * dpr * 80 * dpr + point[0] * dpr) * 4) as usize;
    assert!(
        pixels[at..at + 4]
            .iter()
            .zip(expected)
            .all(|(a, b)| a.abs_diff(b) <= 1),
        "{label} {point:?}: {:?} expected {expected:?}",
        &pixels[at..at + 4]
    );
}
#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_gpu_source_residency_and_shared_consumers() -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().unwrap();
    for mode in [
        ViewportPaintRendererMode::RetainedAuto,
        ViewportPaintRendererMode::Legacy,
    ] {
        let (mut arena, root, host, _native, group) = direct_scene();
        let id = arena
            .get(host)
            .unwrap()
            .element
            .as_any()
            .downcast_ref::<GpuHost>()
            .unwrap()
            .source_id;
        let mut second = GpuHost::new(0x6e05);
        second.source_id = id;
        let other_group = commit_child(
            &mut arena,
            root,
            Box::new(element(
                0x6e04,
                [20., 16.],
                style([20., 16.], Some([60., 8.]), None),
            )),
        );
        let other = commit_child(&mut arena, other_group, Box::new(second));
        let mut v = Viewport::new();
        v.set_paint_renderer_mode(mode);
        v.install_single_viewport_scene_for_test(arena, root);
        // Sequential states: warm, payload edit, placement, DPR, backing loss,
        // then remove one of two consumers and finally the last one.
        for frame in 0..9 {
            let dpr = if frame == 4 { 2 } else { 1 };
            if frame == 2 {
                for key in [host, other] {
                    let mut node = v.node_arena().get_mut(key).unwrap();
                    let h = node.element.as_any_mut().downcast_mut::<GpuHost>().unwrap();
                    h.color = [0., 1., 0., 1.];
                    h.revision += 1;
                    h.dirty = DirtyFlags::PAINT;
                }
            }
            if frame == 3 {
                get_element_mut::<Element>(v.node_arena(), group).apply_style(style(
                    [20., 16.],
                    Some([12., 8.]),
                    None,
                ));
            }
            if frame == 6 {
                let source = v
                    .node_arena()
                    .get(host)
                    .unwrap()
                    .element
                    .prepared_gpu_paint_source()
                    .unwrap()
                    .clone();
                assert!(v.release_persistent_render_target_pair(source.key()));
                assert!(
                    !v.has_compatible_persistent_render_target(source.key(), &source.descriptor())
                );
                assert!(!v.release_persistent_render_target_pair(source.key()));
            }
            if frame == 7 {
                v.edit_scene_arena_for_test(|a| {
                    a.remove_subtree(other_group);
                });
            }
            if frame == 8 {
                v.edit_scene_arena_for_test(|a| {
                    a.remove_subtree(group);
                });
            }
            begin(&mut v, gpu, dpr)?;
            let observed = v.render_single_viewport_scene_for_test()?;
            let pixels = read_submitted_texture(&observed.texture, gpu, [80 * dpr, 64 * dpr])?;
            let color = if frame < 2 {
                [255, 0, 0, 255]
            } else {
                [0, 255, 0, 255]
            };
            probe(
                &pixels,
                dpr,
                [if frame >= 3 { 16 } else { 8 }, 12],
                if frame == 8 { [0; 4] } else { color },
                "first source consumer",
            );
            probe(
                &pixels,
                dpr,
                [64, 12],
                if frame >= 7 { [0; 4] } else { color },
                "shared source consumer",
            );
            probe(
                &pixels,
                dpr,
                [44, 12],
                [0, 0, 255, 128],
                "independent native",
            );
            let sources = v.gpu_paint_observations();
            if frame == 8 {
                assert!(
                    sources.is_empty(),
                    "last consumer releases cache and backing"
                );
            } else {
                assert_eq!(
                    sources.len(),
                    1,
                    "two consumers share one source allocation"
                );
                assert!(sources[0].valid_resident);
                assert_eq!(sources[0].extent, [20 * dpr, 16 * dpr]);
                let work = if matches!(frame, 0 | 2 | 4 | 5 | 6) {
                    GpuPaintWork::Rendered
                } else {
                    GpuPaintWork::Reused
                };
                assert_eq!(sources[0].work, Some(work), "{mode:?} frame={frame}");
            }
            if mode == ViewportPaintRendererMode::RetainedAuto {
                assert!(observed.artifact_selected);
                assert_eq!(
                    observed.actions,
                    vec![if matches!(frame, 0 | 4 | 5) {
                        RetainedSurfaceCompileAction::Reraster
                    } else {
                        RetainedSurfaceCompileAction::Reuse
                    }],
                    "independent native raster {frame}"
                );
            }
        }
    }
    Ok(())
}
