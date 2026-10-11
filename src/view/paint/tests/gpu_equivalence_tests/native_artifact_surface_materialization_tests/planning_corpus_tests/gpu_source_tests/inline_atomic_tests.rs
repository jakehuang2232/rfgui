use super::lifecycle_tests::begin;
use super::*;
use crate::style::Layout;

/// The `GpuHost`, which reports no inline atomic measurement, vertical
/// alignment or placement, as the only child of an inline box at [8, 12].
fn inline_scene() -> (NodeArena, NodeKey, NodeKey) {
    let mut a = NodeArena::new();
    let root = commit_element(
        &mut a,
        Box::new(element(0x6e40, [80., 64.], style([80., 64.], None, None))),
    );
    let mut inline = style([40., 30.], Some([8., 12.]), None);
    inline.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    let parent = commit_child(&mut a, root, Box::new(element(0x6e41, [40., 30.], inline)));
    let host = commit_child(&mut a, parent, Box::new(GpuHost::new(0x6e42)));
    (a, root, host)
}

/// The frame stays retained and paints the host's quad over exactly the box
/// the inline formatting context placed it in.
#[test]
#[ignore = "requires native hardware graphics adapter"]
fn custom_host_in_an_inline_box_paints_where_it_is_placed() -> Result<(), String> {
    let gpu = native_gpu_test_context()?;
    let gpu = gpu.as_ref().unwrap();
    for dpr in [1_u32, 2] {
        let (arena, root, host) = inline_scene();
        let mut v = Viewport::new();
        v.install_single_viewport_scene_for_test(arena, root);
        begin(&mut v, gpu, dpr)?;
        let observed = v.render_single_viewport_scene_for_test()?;
        assert!(observed.artifact_selected, "DPR {dpr}");
        let pixels = read_submitted_texture(&observed.texture, gpu, [80 * dpr, 64 * dpr])?;
        let b = v
            .node_arena()
            .get(host)
            .unwrap()
            .element
            .box_model_snapshot();
        assert!(b.x >= 8. && b.y >= 12. && b.width == 20. && b.height == 16.);
        let pixel = |x: f32, y: f32| {
            let at = (((y * dpr as f32) as u32 * 80 * dpr + (x * dpr as f32) as u32) * 4) as usize;
            <[u8; 4]>::try_from(&pixels[at..at + 4]).unwrap()
        };
        let (cx, cy) = (b.x + b.width / 2., b.y + b.height / 2.);
        assert_eq!(pixel(cx, cy), [255, 0, 0, 255], "DPR {dpr}: {b:?}");
        for (x, y) in [
            (b.x - 2., cy),
            (b.x + b.width + 2., cy),
            (cx, b.y - 2.),
            (cx, b.y + b.height + 2.),
        ] {
            assert_eq!(pixel(x, y), [0; 4], "DPR {dpr}: ({x}, {y}) outside {b:?}");
        }
    }
    Ok(())
}
