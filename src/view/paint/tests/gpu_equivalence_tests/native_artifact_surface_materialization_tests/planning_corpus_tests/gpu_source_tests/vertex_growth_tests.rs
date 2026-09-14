use super::lifecycle_tests::{begin, direct_scene, probe};
use super::*;

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn native_gpu_source_vertex_growth_and_empty_draw_preserve_current_pixels() -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().unwrap();
    for mode in [
        ViewportPaintRendererMode::RetainedAuto,
        ViewportPaintRendererMode::Legacy,
    ] {
        for dpr in [1, 2] {
            let (arena, root, host, _, _) = direct_scene();
            let mut viewport = Viewport::new();
            viewport.set_paint_renderer_mode(mode);
            viewport.install_single_viewport_scene_for_test(arena, root);
            let mut previous_vertices: Option<wgpu::Buffer> = None;
            // 3, 4, 5 triangles share capacity; 2 -> 3 crosses it. Empty
            // payload must clear the old source and never draw reserved bytes.
            for (frame, triangles) in [2, 3, 4, 5, 5, 0, 2].into_iter().enumerate() {
                let color = if frame % 2 == 0 {
                    [1., 0., 0., 1.]
                } else {
                    [0., 1., 0., 1.]
                };
                {
                    let mut node = viewport.node_arena().get_mut(host).unwrap();
                    let host = node.element.as_any_mut().downcast_mut::<GpuHost>().unwrap();
                    host.triangles = triangles;
                    host.color = color;
                    host.revision += 1;
                    host.dirty = DirtyFlags::PAINT;
                }
                begin(&mut viewport, gpu, dpr)?;
                let observed = viewport.render_single_viewport_scene_for_test()?;
                let pixels = read_submitted_texture(&observed.texture, gpu, [80 * dpr, 64 * dpr])?;
                if triangles > 0 {
                    let capacity = (triangles as u64 * 3 * 8).next_power_of_two();
                    let buffers = viewport
                        .frame_buffers_for_test()
                        .into_iter()
                        .map(|(_, buffer)| buffer)
                        .filter(|buffer| {
                            buffer.usage().contains(wgpu::BufferUsages::VERTEX)
                                && buffer.size() == capacity
                        })
                        .collect::<Vec<_>>();
                    assert_eq!(buffers.len(), 1, "fixture has one source vertex allocation");
                    let vertices = buffers.into_iter().next().unwrap();
                    if let Some(previous) = previous_vertices.as_ref() {
                        if (2..=4).contains(&frame) {
                            assert_eq!(
                                *previous, vertices,
                                "same capacity reuses the actual GPU buffer"
                            );
                        } else if frame == 1 {
                            assert_ne!(
                                *previous, vertices,
                                "capacity growth replaces the GPU buffer"
                            );
                        }
                    }
                    previous_vertices = Some(vertices);
                }
                let expected = if triangles == 0 {
                    [0; 4]
                } else if frame % 2 == 0 {
                    [255, 0, 0, 255]
                } else {
                    [0, 255, 0, 255]
                };
                probe(&pixels, dpr, [8, 12], expected, "current vertex payload");
            }
        }
    }
    Ok(())
}
