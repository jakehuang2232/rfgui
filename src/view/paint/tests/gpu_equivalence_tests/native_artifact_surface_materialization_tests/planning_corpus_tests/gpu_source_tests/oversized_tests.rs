use super::lifecycle_tests::{begin, probe};
use super::*;

/// A host far wider than the device's texture limit renders its source at a
/// resolution that fits, stretched over its whole box.
#[test]
#[ignore = "requires native hardware graphics adapter"]
fn gpu_source_wider_than_the_texture_limit_renders_at_a_fitting_size() -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().unwrap();
    for dpr in [1_u32, 2] {
        let mut a = NodeArena::new();
        let root = commit_element(
            &mut a,
            Box::new(element(0x6e50, [80., 64.], style([80., 64.], None, None))),
        );
        let wide = commit_child(
            &mut a,
            root,
            Box::new(element(
                0x6e51,
                [20000., 10.],
                style([20000., 10.], Some([0., 0.]), None),
            )),
        );
        let mut host = GpuHost::new(0x6e52);
        host.size = [20000., 10.];
        let host = commit_child(&mut a, wide, Box::new(host));
        let mut v = Viewport::new();
        v.install_single_viewport_scene_for_test(a, root);
        begin(&mut v, gpu, dpr)?;
        let observed = v.render_single_viewport_scene_for_test()?;
        assert!(observed.artifact_selected, "DPR {dpr}");
        let pixels = read_submitted_texture(&observed.texture, gpu, [80 * dpr, 64 * dpr])?;
        probe(&pixels, dpr, [40, 5], [255, 0, 0, 255], "inside the host");
        probe(&pixels, dpr, [40, 20], [0; 4], "below the host");
        let limit = v.max_texture_dimension_2d();
        let source = v
            .node_arena()
            .get(host)
            .unwrap()
            .element
            .prepared_gpu_paint_source()
            .unwrap()
            .clone();
        assert!(
            source.extent()[0] > limit,
            "DPR {dpr}: {:?}",
            source.extent()
        );
        let resident = source.descriptor(limit);
        assert_eq!(resident.width(), limit, "DPR {dpr}");
        assert!(v.has_compatible_persistent_render_target(source.key(), &resident));
    }
    Ok(())
}

/// A source larger than the surface cache budget is still drawn, produced
/// again every frame into a frame-local texture instead of being kept.
#[test]
#[ignore = "requires native hardware graphics adapter"]
fn gpu_source_past_the_cache_budget_is_produced_every_frame() -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().unwrap();
    let mut a = NodeArena::new();
    let root = commit_element(
        &mut a,
        Box::new(element(0x6e53, [80., 64.], style([80., 64.], None, None))),
    );
    // 6000 x 6000 x 4 bytes is past the 128 MiB budget.
    let large = commit_child(
        &mut a,
        root,
        Box::new(element(
            0x6e54,
            [6000., 6000.],
            style([6000., 6000.], Some([0., 0.]), None),
        )),
    );
    let mut host = GpuHost::new(0x6e55);
    host.size = [6000., 6000.];
    commit_child(&mut a, large, Box::new(host));
    let mut v = Viewport::new();
    v.install_single_viewport_scene_for_test(a, root);
    for frame in 0..2 {
        begin(&mut v, gpu, 1)?;
        let observed = v.render_single_viewport_scene_for_test()?;
        assert!(observed.artifact_selected, "frame {frame}");
        let pixels = read_submitted_texture(&observed.texture, gpu, [80, 64])?;
        probe(&pixels, 1, [40, 32], [255, 0, 0, 255], "inside the host");
        let [source] = v
            .gpu_paint_observations()
            .try_into()
            .unwrap_or_else(|_| panic!("one GPU source"));
        assert_eq!(
            source.work,
            Some(crate::view::gpu_paint::GpuPaintWork::Rendered),
            "frame {frame}"
        );
        assert!(!source.valid_resident, "frame {frame}");
    }
    Ok(())
}
