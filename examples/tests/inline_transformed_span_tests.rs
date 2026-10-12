//! A transformed inline span stays on the retained renderer and transforms
//! its whole content about its box, as an element's transform does.
use rfgui::style::{Angle, Color, Length, Padding, Rotate, Transform, TransformOrigin};
use rfgui::time::Instant;
use rfgui::ui::{RsxNode, rsx};
use rfgui::view::base_component::BoxModelSnapshot;
use rfgui::view::{Element, NodeArena, NodeKey, Text, Viewport};

#[path = "../../lib/rfgui-components/tests/retained_controls/gpu.rs"]
mod gpu;

const SIZE: [u32; 2] = [300, 200];

/// A padded, blue span holding text then a 40px red box, turned by
/// `degrees` about its center, inside a 300x200 inline root.
fn scene(degrees: f32) -> RsxNode {
    rsx! {
        <Element style={{width: Length::px(300.0), height: Length::px(200.0)}}>
            <Element style={{
                padding: Padding::uniform(Length::px(30.0)),
                background: Color::rgb(0, 0, 255),
                transform_origin: TransformOrigin::center(),
                transform: Transform::new([Rotate::z(Angle::deg(degrees))]),
            }}>
                <Text>{"ab"}</Text>
                <Element style={{
                    width: Length::px(40.0),
                    height: Length::px(40.0),
                    background: Color::rgb(255, 0, 0),
                }} />
            </Element>
        </Element>
    }
}

/// The laid-out box of the first element `keep` accepts.
fn find(arena: &NodeArena, keep: impl Fn(&BoxModelSnapshot) -> bool + Copy) -> BoxModelSnapshot {
    fn walk(
        arena: &NodeArena,
        key: NodeKey,
        keep: impl Fn(&BoxModelSnapshot) -> bool + Copy,
    ) -> Option<BoxModelSnapshot> {
        let b = arena.get(key)?.element.box_model_snapshot();
        if keep(&b) {
            return Some(b);
        }
        arena
            .children_of(key)
            .into_iter()
            .find_map(|child| walk(arena, child, keep))
    }
    walk(arena, arena.roots()[0], keep).expect("laid-out box")
}

/// The physical bounds of the red pixels, as [left, top, right, bottom).
fn red_bounds(pixels: &[u8], width: u32) -> Option<[u32; 4]> {
    let mut found: Option<[u32; 4]> = None;
    for (index, p) in pixels.chunks_exact(4).enumerate() {
        if p[3] > 0 && p[0] > p[1] && p[0] > p[2] {
            let (x, y) = (index as u32 % width, index as u32 / width);
            found = Some(match found {
                None => [x, y, x + 1, y + 1],
                Some([l, t, r, b]) => [l.min(x), t.min(y), r.max(x + 1), b.max(y + 1)],
            });
        }
    }
    found
}

/// Turned half a revolution, the red box lands mirrored through the span's
/// center: where the untransformed one would, reflected.
#[test]
#[ignore = "requires native hardware graphics adapter"]
fn transformed_span_turns_its_content_about_its_box() -> Result<(), String> {
    let gpu = gpu::Gpu::new()?;
    for dpr in [1_u32, 2] {
        let size = SIZE.map(|n| n * dpr);
        let mut viewport = Viewport::new();
        let frame = viewport.render_rsx_offscreen_for_test(
            &scene(180.0),
            gpu.device.clone(),
            gpu.queue.clone(),
            size,
            dpr as f32,
            Instant::now(),
        )?;
        let pixels = gpu.read(&frame.texture, size)?;
        let arena = viewport.node_arena();
        let red = find(arena, |b| b.width == 40.0 && b.height == 40.0);
        let span = find(arena, |b| b.width > 40.0 && b.width < 300.0);
        let (cx, cy) = (span.x + span.width / 2.0, span.y + span.height / 2.0);
        let expected = [
            2.0 * cx - (red.x + red.width),
            2.0 * cy - (red.y + red.height),
            2.0 * cx - red.x,
            2.0 * cy - red.y,
        ]
        .map(|v| v * dpr as f32);
        let painted = red_bounds(&pixels, size[0]).ok_or("no red box")?;
        assert!(
            painted
                .iter()
                .zip(expected)
                .all(|(&got, want)| (got as f32 - want).abs() <= 1.0),
            "DPR {dpr}: painted {painted:?}, expected {expected:?} (span {span:?}, box {red:?})"
        );
    }
    Ok(())
}
