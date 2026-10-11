use super::lifecycle_tests::{begin, probe};
use super::*;
use crate::view::paint::tests::{CustomLeafPaintHost, CustomLeafRecordMode};

/// Custom leaves deferred to the root viewport, a GPU source and a recorded
/// fill, stay on the retained renderer and paint after the rest of the
/// scene: over the blue sibling that follows and covers them.
#[test]
#[ignore = "requires native hardware graphics adapter"]
fn deferred_custom_leaves_paint_over_later_siblings() -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().unwrap();
    for dpr in [1_u32, 2] {
        for fill in [false, true] {
            let (host, center): (Box<dyn ElementTrait>, [u32; 2]) = if fill {
                let mut host = CustomLeafPaintHost::fill(0x6e61);
                host.mode = CustomLeafRecordMode::Fill {
                    rgba: [1., 0., 0., 1.],
                    opacity: 1.,
                };
                host.deferred = true;
                // Its fixed 20x10 box at [4, 6].
                (Box::new(host), [14, 11])
            } else {
                let mut host = GpuHost::new(0x6e61);
                host.deferred = true;
                // Its 20x16 box at the origin.
                (Box::new(host), [10, 8])
            };
            let mut a = NodeArena::new();
            let root = commit_element(
                &mut a,
                Box::new(element(0x6e60, [80., 64.], style([80., 64.], None, None))),
            );
            commit_child(&mut a, root, host);
            commit_child(
                &mut a,
                root,
                Box::new(element(
                    0x6e62,
                    [40., 32.],
                    style([40., 32.], Some([0., 0.]), Some(BLUE)),
                )),
            );
            let mut v = Viewport::new();
            v.install_single_viewport_scene_for_test(a, root);
            begin(&mut v, gpu, dpr)?;
            let label = if fill { "fill" } else { "GPU source" };
            let observed = v.render_single_viewport_scene_for_test()?;
            assert!(observed.artifact_selected, "{label} DPR {dpr}");
            let pixels = read_submitted_texture(&observed.texture, gpu, [80 * dpr, 64 * dpr])?;
            probe(&pixels, dpr, center, [255, 0, 0, 255], label);
            probe(
                &pixels,
                dpr,
                [30, 24],
                [0, 0, 255, 255],
                "sibling beside it",
            );
        }
    }
    Ok(())
}
