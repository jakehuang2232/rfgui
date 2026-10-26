//! An atomic box inside an inline span stays on the retained renderer and
//! paints where its formatting context placed it, scrolled or not.
use rfgui::style::{Color, Layout, Length, Padding, ScrollDirection};
use rfgui::time::Instant;
use rfgui::ui::{RsxNode, rsx};
use rfgui::view::base_component::BoxModelSnapshot;
use rfgui::view::viewport::set_scroll_offset_by_id;
use rfgui::view::{Element, Text, Viewport};

#[path = "../../lib/rfgui-components/tests/retained_controls/gpu.rs"]
mod gpu;

const SIZE: [u32; 2] = [400, 200];

/// A padded, blue span holding text and a 40px red box, 100px down a
/// scroll container.
fn scene() -> RsxNode {
    rsx! {
        <Element style={{
            width: Length::px(400.0),
            height: Length::px(200.0),
            layout: Layout::flow().column(),
            scroll_direction: ScrollDirection::Vertical,
        }}>
            <Element style={{width: Length::px(300.0), height: Length::px(100.0)}} />
            <Element style={{width: Length::px(300.0), height: Length::px(120.0)}}>
                <Element style={{
                    padding: Padding::uniform(Length::px(10.0)),
                    background: Color::rgb(0, 0, 255),
                }}>
                    <Text>{"ab"}</Text>
                    <Element style={{
                        width: Length::px(40.0),
                        height: Length::px(40.0),
                        background: Color::rgb(255, 0, 0),
                    }} />
                </Element>
            </Element>
            <Element style={{width: Length::px(300.0), height: Length::px(300.0)}} />
        </Element>
    }
}

/// The laid-out box of the 40px red element, in the viewport.
fn red_box(viewport: &Viewport) -> BoxModelSnapshot {
    viewport
        .frame_box_models()
        .iter()
        .find(|b| b.width == 40.0 && b.height == 40.0)
        .cloned()
        .expect("red box")
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn atomic_box_in_an_inline_span_paints_where_it_is_placed() -> Result<(), String> {
    let gpu = gpu::Gpu::new()?;
    let tree = scene();
    for dpr in [1_u32, 2] {
        let size = SIZE.map(|n| n * dpr);
        let mut viewport = Viewport::new();
        for scroll in [0.0, 80.0] {
            if scroll > 0.0 {
                let arena = viewport.node_arena();
                let root = arena.roots()[0];
                let id = arena.get(root).unwrap().element.stable_id();
                assert!(set_scroll_offset_by_id(arena, root, id, (0.0, scroll)));
            }
            let frame = viewport.render_rsx_offscreen_for_test(
                &tree,
                gpu.device.clone(),
                gpu.queue.clone(),
                size,
                dpr as f32,
                Instant::now(),
            )?;
            let pixels = gpu.read(&frame.texture, size)?;
            let b = red_box(&viewport);
            assert!(b.y + b.height <= 200.0, "DPR {dpr} scroll {scroll}: {b:?}");
            // The red pixels' bounds, within a pixel of the laid-out box.
            let mut found: Option<[u32; 4]> = None;
            for (index, p) in pixels.chunks_exact(4).enumerate() {
                if p[3] > 0 && p[0] > p[1] && p[0] > p[2] {
                    let (x, y) = (index as u32 % size[0], index as u32 / size[0]);
                    found = Some(match found {
                        None => [x, y, x + 1, y + 1],
                        Some([l, t, r, bt]) => [l.min(x), t.min(y), r.max(x + 1), bt.max(y + 1)],
                    });
                }
            }
            let found = found.ok_or("no red box")?;
            let expected = [b.x, b.y, b.x + b.width, b.y + b.height].map(|v| v * dpr as f32);
            assert!(
                found
                    .iter()
                    .zip(expected)
                    .all(|(&got, want)| (got as f32 - want).abs() <= 1.0),
                "DPR {dpr} scroll {scroll}: painted {found:?}, laid out {expected:?}"
            );
            // The span's background shows in its padding beside the box.
            let at = (((b.y + b.height / 2.0) * dpr as f32) as u32 * size[0]
                + ((b.x + b.width + 5.0) * dpr as f32) as u32) as usize
                * 4;
            assert_eq!(
                pixels[at..at + 4],
                [0, 0, 255, 255],
                "DPR {dpr} scroll {scroll}"
            );
        }
    }
    Ok(())
}
