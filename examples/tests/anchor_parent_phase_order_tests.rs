//! An `AnchorParent` child paints after its normal siblings, as in Legacy,
//! whatever its arena position, and the frame stays on the retained renderer.
use rfgui::style::{ClipMode, Color, Length, Position};
use rfgui::time::Instant;
use rfgui::ui::{RsxNode, rsx};
use rfgui::view::{Element, Viewport};

#[path = "../../lib/rfgui-components/tests/retained_controls/gpu.rs"]
mod gpu;

const SIZE: [u32; 2] = [120, 90];
const ANCHOR: [u8; 3] = [220, 30, 20];
const NORMAL: [u8; 3] = [20, 40, 220];

fn child(color: [u8; 3], clip: ClipMode) -> RsxNode {
    rsx! {
        <Element style={{
            position: Position::absolute()
                .left(Length::px(30.))
                .top(Length::px(24.))
                .clip(clip),
            width: Length::px(40.),
            height: Length::px(30.),
            background: Color::rgb(color[0], color[1], color[2]),
        }} />
    }
}

/// Two children at the same place; only the arena order differs.
fn scene(anchor_first: bool) -> RsxNode {
    let anchor = child(ANCHOR, ClipMode::AnchorParent);
    let normal = child(NORMAL, ClipMode::Parent);
    let children = if anchor_first {
        vec![anchor, normal]
    } else {
        vec![normal, anchor]
    };
    rsx! {
        <Element style={{
            width: Length::px(100.),
            height: Length::px(80.),
            background: Color::rgb(40, 40, 40),
        }}>
            {children}
        </Element>
    }
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn anchor_parent_child_paints_over_its_normal_sibling_in_either_arena_order() -> Result<(), String>
{
    let gpu = gpu::Gpu::new()?;
    for anchor_first in [true, false] {
        for dpr in [1_u32, 2] {
            let size = [SIZE[0] * dpr, SIZE[1] * dpr];
            let mut viewport = Viewport::new();
            let frame = viewport.render_rsx_offscreen_for_test(
                &scene(anchor_first),
                gpu.device.clone(),
                gpu.queue.clone(),
                size,
                dpr as f32,
                Instant::now(),
            )?;
            assert!(
                frame.artifact_selected,
                "anchor_first={anchor_first} DPR {dpr} fell back to Legacy"
            );
            let pixels = gpu.read(&frame.texture, size)?;
            let [x, y] = [50 * dpr, 39 * dpr];
            let at = ((y * size[0] + x) * 4) as usize;
            let [r, _, b, _] = <[u8; 4]>::try_from(&pixels[at..at + 4]).unwrap();
            assert!(
                r > 150 && b < 100,
                "anchor_first={anchor_first} DPR {dpr}: the overlap shows rgb {:?}, not the anchor",
                &pixels[at..at + 3]
            );
        }
    }
    Ok(())
}
