//! Rounded clips nested deeper than the stencil's 255 levels stay on the
//! retained renderer and clip like a shallow nest.
use rfgui::style::{Color, Length};
use rfgui::time::Instant;
use rfgui::ui::{RsxNode, rsx};
use rfgui::view::{Element, Viewport};

#[path = "../../lib/rfgui-components/tests/retained_controls/gpu.rs"]
mod gpu;

const SIZE: [u32; 2] = [400, 400];

/// A rounded box of `size` clipping `child`.
fn clip(size: f32, child: RsxNode) -> RsxNode {
    rsx! {
        <Element style={{
            width: Length::px(size),
            height: Length::px(size),
            border_radius: Length::px(40.0),
        }}>
            {child}
        </Element>
    }
}

/// `outer` rounded 200px boxes around `inner` rounded 400px ones, each box
/// clipping the next, around a 400px red box. Only the 200px boxes cut the
/// red box down.
fn nest(outer: usize, inner: usize) -> RsxNode {
    let mut node = rsx! {
        <Element style={{
            width: Length::px(400.0),
            height: Length::px(400.0),
            background: Color::rgb(255, 0, 0),
        }} />
    };
    for _ in 0..inner {
        node = clip(400.0, node);
    }
    for _ in 0..outer {
        node = clip(200.0, node);
    }
    node
}

fn render(gpu: &gpu::Gpu, tree: &RsxNode, dpr: u32) -> Result<Vec<u8>, String> {
    let size = SIZE.map(|n| n * dpr);
    let frame = Viewport::new().render_rsx_offscreen_for_test(
        tree,
        gpu.device.clone(),
        gpu.queue.clone(),
        size,
        dpr as f32,
        Instant::now(),
    )?;
    assert!(frame.artifact_selected);
    gpu.read(&frame.texture, size)
}

/// Three hundred nested masks clip exactly as three do. The masks past the
/// stencil's 255 are the large ones, so only the outer masks' stencil test
/// keeps the red box inside the 200px corner-cut square.
#[test]
#[ignore = "requires native hardware graphics adapter"]
fn rounded_clips_nested_past_the_stencil_depth_clip_like_a_shallow_nest() {
    // Layout and recording recurse once per level.
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(|| -> Result<(), String> {
            let gpu = gpu::Gpu::new()?;
            for dpr in [1_u32, 2] {
                let shallow = render(&gpu, &nest(1, 2), dpr)?;
                let width = (SIZE[0] * dpr) as usize;
                let at = |x: usize, y: usize| ((y * dpr as usize) * width + x * dpr as usize) * 4;
                assert_eq!(shallow[at(100, 100)..][..4], [255, 0, 0, 255]);
                assert_eq!(shallow[at(2, 2)..][..4], [0, 0, 0, 0]);
                assert_eq!(shallow[at(300, 300)..][..4], [0, 0, 0, 0]);
                let deep = render(&gpu, &nest(250, 50), dpr)?;
                assert!(deep == shallow, "DPR {dpr}: 300 masks differ from 3");
            }
            Ok(())
        })
        .unwrap()
        .join()
        .unwrap()
        .unwrap();
}
