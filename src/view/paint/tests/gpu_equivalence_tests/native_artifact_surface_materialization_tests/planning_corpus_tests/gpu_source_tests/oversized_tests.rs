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
