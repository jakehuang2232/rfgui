//! Retained surfaces nested in scrolled content stay visible and match the
//! legacy renderer.
use rfgui::style::{Color, Layout, Length, Opacity, ScrollDirection};
use rfgui::time::Instant;
use rfgui::ui::{RsxNode, rsx};
use rfgui::view::viewport::{ViewportPaintRendererMode, set_scroll_offset_by_id};
use rfgui::view::{Element, Text, Viewport};

#[path = "../../lib/rfgui-components/tests/retained_controls/gpu.rs"]
mod gpu;

/// An opacity group and an inner scroll container, both below the fold of
/// the outer scroll container until it scrolls.
fn scene() -> RsxNode {
    rsx! {
        <Element style={{width:Length::px(400.), height:Length::px(400.), layout:Layout::flow().column(), scroll_direction:ScrollDirection::Vertical}}>
            <Element style={{width:Length::px(400.), height:Length::px(600.), background:Color::rgb(20,40,60)}} />
            <Element style={{width:Length::px(200.), height:Length::px(120.), background:Color::rgb(220,80,40), opacity:Opacity::new(0.5)}}>
                <Element style={{width:Length::px(120.), height:Length::px(60.), background:Color::rgb(40,160,220), opacity:Opacity::new(0.6)}} />
            </Element>
            <Element style={{width:Length::px(200.), height:Length::px(120.), layout:Layout::flow().column(), scroll_direction:ScrollDirection::Vertical, background:Color::rgb(60,60,60)}}>
                {(0..12).map(|i| rsx! {
                    <Element style={{width:Length::px(180.), height:Length::px(24.), background:Color::rgb(80+(i%5) as u8*30,200,90)}}>
                        <Text>{format!("Row {i}")}</Text>
                    </Element>
                }).collect::<Vec<_>>()}
            </Element>
            <Element style={{width:Length::px(400.), height:Length::px(600.), background:Color::rgb(20,40,60)}} />
        </Element>
    }
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn surfaces_nested_in_scrolled_content_match_legacy() -> Result<(), String> {
    let gpu = gpu::Gpu::new()?;
    let offsets = [(0., 0.), (0., 300.), (0., 500.), (0., 640.)];
    for dpr in [1, 2] {
        let size = [400 * dpr, 400 * dpr];
        let mut reference = Vec::new();
        for mode in [
            ViewportPaintRendererMode::Legacy,
            ViewportPaintRendererMode::RetainedAuto,
        ] {
            let mut viewport = Viewport::new();
            viewport.set_paint_renderer_mode(mode);
            let now = Instant::now();
            for (frame, &offset) in offsets.iter().enumerate() {
                if frame > 0 {
                    let arena = viewport.node_arena();
                    let root = arena.roots()[0];
                    let id = arena.get(root).unwrap().element.stable_id();
                    assert!(set_scroll_offset_by_id(arena, root, id, offset));
                }
                let tree = scene();
                let output = viewport.render_rsx_offscreen_for_test(
                    &tree,
                    gpu.device.clone(),
                    gpu.queue.clone(),
                    size,
                    dpr as f32,
                    now,
                )?;
                let pixels = gpu.read(&output.texture, size)?;
                if mode == ViewportPaintRendererMode::Legacy {
                    reference.push(pixels);
                    continue;
                }
                let differing = pixels
                    .chunks_exact(4)
                    .zip(reference[frame].chunks_exact(4))
                    .filter(|(a, b)| a.iter().zip(b.iter()).any(|(a, b)| a.abs_diff(*b) > 2))
                    .count();
                assert_eq!(
                    differing, 0,
                    "pixels beyond blending tolerance at DPR {dpr} offset {offset:?}"
                );
            }
        }
    }
    Ok(())
}
