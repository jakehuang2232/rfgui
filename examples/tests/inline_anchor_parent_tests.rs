//! A positioned child of an inline span is placed against the span's box,
//! paints after its normal siblings whatever their arena order, and clips to
//! its anchor's parent, all on the retained renderer.
use rfgui::style::{ClipMode, Color, Layout, Length, Position, ScrollDirection};
use rfgui::time::Instant;
use rfgui::ui::{RsxNode, rsx};
use rfgui::view::viewport::set_scroll_offset_by_id;
use rfgui::view::{Element, Text, Viewport};

#[path = "../../lib/rfgui-components/tests/retained_controls/gpu.rs"]
mod gpu;

const SIZE: [u32; 2] = [400, 120];
// Pure primaries read back the same from the linear target.
const SPAN: [u8; 3] = [255, 0, 0];
const ANCHORED: [u8; 3] = [0, 0, 255];
const NORMAL: [u8; 3] = [0, 255, 0];

fn positioned(color: [u8; 3], left: f32, clip: ClipMode) -> RsxNode {
    rsx! {
        <Element style={{
            position: Position::absolute()
                .anchor("span")
                .top(Length::px(0.))
                .left(Length::px(left))
                .clip(clip),
            width: Length::px(12.),
            height: Length::px(8.),
            background: Color::rgb(color[0], color[1], color[2]),
        }} />
    }
}

/// A 300px wide root whose first inline item is a span holding text and
/// `children`.
fn scene(children: Vec<RsxNode>) -> RsxNode {
    rsx! {
        <Element style={{width: Length::px(300.), height: Length::px(100.)}}>
            <Element style={{background: Color::rgb(SPAN[0], SPAN[1], SPAN[2])}} anchor="span">
                <Text>{"inline span text"}</Text>
                {children}
            </Element>
            <Text>{" after"}</Text>
        </Element>
    }
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

/// Whether `color` dominates a painted pixel. Edge pixels of a box at a
/// fractional position are partly covered, so coverage is not required.
fn is(pixel: &[u8], color: [u8; 3]) -> bool {
    let channel = color.iter().position(|&c| c == 255).unwrap();
    pixel[3] > 0 && (0..3).all(|other| other == channel || pixel[channel] > pixel[other])
}

/// The physical bounding box of the pixels `keep` selects.
fn bounds(pixels: &[u8], dpr: u32, keep: impl Fn(&[u8]) -> bool) -> Option<[u32; 4]> {
    let width = SIZE[0] * dpr;
    let mut found: Option<[u32; 4]> = None;
    for (index, pixel) in pixels.chunks_exact(4).enumerate() {
        if keep(pixel) {
            let (x, y) = (index as u32 % width, index as u32 / width);
            found = Some(match found {
                None => [x, y, x, y],
                Some([l, t, r, b]) => [l.min(x), t.min(y), r.max(x), b.max(y)],
            });
        }
    }
    found
}

/// `scene` 300px down a scroll container.
fn scrolled_scene(children: Vec<RsxNode>) -> RsxNode {
    rsx! {
        <Element style={{
            width: Length::px(400.),
            height: Length::px(120.),
            layout: Layout::flow().column(),
            scroll_direction: ScrollDirection::Vertical,
        }}>
            <Element style={{width: Length::px(300.), height: Length::px(300.)}} />
            {scene(children)}
            <Element style={{width: Length::px(300.), height: Length::px(300.)}} />
        </Element>
    }
}

/// `scrolled_scene` scrolled until the root sits 20px down the viewport.
fn render_scrolled(gpu: &gpu::Gpu, children: Vec<RsxNode>, dpr: u32) -> Result<Vec<u8>, String> {
    let size = SIZE.map(|n| n * dpr);
    let tree = scrolled_scene(children);
    let mut viewport = Viewport::new();
    for scroll in [false, true] {
        if scroll {
            let arena = viewport.node_arena();
            let root = arena.roots()[0];
            let id = arena.get(root).unwrap().element.stable_id();
            assert!(set_scroll_offset_by_id(arena, root, id, (0., 280.)));
        }
        let frame = viewport.render_rsx_offscreen_for_test(
            &tree,
            gpu.device.clone(),
            gpu.queue.clone(),
            size,
            dpr as f32,
            Instant::now(),
        )?;
        assert!(frame.artifact_selected);
        if scroll {
            return gpu.read(&frame.texture, size);
        }
    }
    unreachable!()
}

/// Scrolled, the anchored box still sits at the span's top-left corner, and
/// moved to the root's right edge it still clips to the root.
#[test]
#[ignore = "requires native hardware graphics adapter"]
fn positioned_child_of_an_inline_span_follows_the_span_when_scrolled() -> Result<(), String> {
    let gpu = gpu::Gpu::new()?;
    for dpr in [1_u32, 2] {
        let pixels = render_scrolled(
            &gpu,
            vec![positioned(ANCHORED, 0., ClipMode::AnchorParent)],
            dpr,
        )?;
        let anchored = bounds(&pixels, dpr, |p| is(p, ANCHORED)).ok_or("no anchored box")?;
        let span = bounds(&pixels, dpr, |p| is(p, SPAN) || is(p, ANCHORED)).ok_or("no span")?;
        assert_eq!(
            [anchored[0], anchored[1]],
            [span[0], span[1]],
            "DPR {dpr}: anchored {anchored:?}, span {span:?}"
        );
        // The root's top lands at 20; the span sits a fraction lower.
        assert!(
            (20 * dpr..=23 * dpr).contains(&span[1]),
            "DPR {dpr}: the span scrolled to {span:?}"
        );
        let left = 294. - (anchored[0] / dpr) as f32;
        let pixels = render_scrolled(
            &gpu,
            vec![positioned(ANCHORED, left, ClipMode::AnchorParent)],
            dpr,
        )?;
        let shown = bounds(&pixels, dpr, |p| is(p, ANCHORED)).ok_or("no anchored box")?;
        assert_eq!(
            [shown[0], shown[2] + 1],
            [294 * dpr, 300 * dpr],
            "DPR {dpr}: {shown:?}"
        );
    }
    Ok(())
}

/// The anchored box sits at the span's top-left corner: its bounds start
/// where the span's background and the box together start.
#[test]
#[ignore = "requires native hardware graphics adapter"]
fn positioned_child_of_an_inline_span_sits_at_the_span() -> Result<(), String> {
    let gpu = gpu::Gpu::new()?;
    for dpr in [1_u32, 2] {
        let pixels = render(
            &gpu,
            &scene(vec![positioned(ANCHORED, 0., ClipMode::AnchorParent)]),
            dpr,
        )?;
        let anchored = bounds(&pixels, dpr, |p| is(p, ANCHORED)).ok_or("no anchored box")?;
        let span = bounds(&pixels, dpr, |p| is(p, SPAN) || is(p, ANCHORED)).ok_or("no span")?;
        assert_eq!(
            [anchored[0], anchored[1]],
            [span[0], span[1]],
            "DPR {dpr}: anchored {anchored:?}, span {span:?}"
        );
        // The span sits a fraction of a pixel down its line; the box may
        // cover one more partial row.
        let size = [anchored[2] - anchored[0] + 1, anchored[3] - anchored[1] + 1];
        assert!(
            size[0] == 12 * dpr && (8 * dpr..=8 * dpr + 1).contains(&size[1]),
            "DPR {dpr}: the whole box shows: {size:?}"
        );
    }
    Ok(())
}

/// An anchor-parent child and a parent-clipped sibling at the same place:
/// the anchor-parent child shows on top in either arena order.
#[test]
#[ignore = "requires native hardware graphics adapter"]
fn anchor_parent_child_of_an_inline_span_paints_after_its_normal_sibling() -> Result<(), String> {
    let gpu = gpu::Gpu::new()?;
    for anchored_first in [true, false] {
        for dpr in [1_u32, 2] {
            let anchored = positioned(ANCHORED, 0., ClipMode::AnchorParent);
            let normal = positioned(NORMAL, 0., ClipMode::Parent);
            let children = if anchored_first {
                vec![anchored, normal]
            } else {
                vec![normal, anchored]
            };
            let pixels = render(&gpu, &scene(children), dpr)?;
            assert!(
                bounds(&pixels, dpr, |p| is(p, ANCHORED)).is_some(),
                "anchored_first={anchored_first} DPR {dpr}: the anchor-parent child is covered"
            );
            assert!(
                bounds(&pixels, dpr, |p| is(p, NORMAL)).is_none(),
                "anchored_first={anchored_first} DPR {dpr}: the sibling paints over it"
            );
        }
    }
    Ok(())
}

/// Moved to the root's right edge, the anchor-parent child clips to the
/// root, its anchor's parent.
#[test]
#[ignore = "requires native hardware graphics adapter"]
fn anchor_parent_child_of_an_inline_span_clips_to_the_anchors_parent() -> Result<(), String> {
    let gpu = gpu::Gpu::new()?;
    for dpr in [1_u32, 2] {
        let span_left = {
            let pixels = render(
                &gpu,
                &scene(vec![positioned(ANCHORED, 0., ClipMode::AnchorParent)]),
                dpr,
            )?;
            bounds(&pixels, dpr, |p| is(p, ANCHORED)).ok_or("no anchored box")?[0] / dpr
        };
        // The box spans [294, 306) logical: half inside the 300px root.
        let left = 294. - span_left as f32;
        let pixels = render(
            &gpu,
            &scene(vec![positioned(ANCHORED, left, ClipMode::AnchorParent)]),
            dpr,
        )?;
        let shown = bounds(&pixels, dpr, |p| is(p, ANCHORED)).ok_or("no anchored box")?;
        assert_eq!(
            [shown[0], shown[2] + 1],
            [294 * dpr, 300 * dpr],
            "DPR {dpr}: {shown:?}"
        );
    }
    Ok(())
}
