//! Retained surfaces and clipped content nested in scrolled content stay in
//! place: scrolled by an offset, they paint what the same content moved up by
//! that offset paints in a fresh viewport.
use rfgui::style::{ClipMode, Color, Layout, Length, Opacity, Position, ScrollDirection};
use rfgui::time::Instant;
use rfgui::ui::{RsxNode, rsx};
use rfgui::view::viewport::{ViewportPaintRendererMode, set_scroll_offset_by_id};
use rfgui::view::{Element, Text, Viewport};

#[path = "../../lib/rfgui-components/tests/retained_controls/gpu.rs"]
mod gpu;

/// An opacity group and an inner scroll container, both below the fold of
/// the outer scroll container until it scrolls.
fn nested_surfaces() -> Vec<RsxNode> {
    vec![
        rsx! {
            <Element style={{width:Length::px(400.), height:Length::px(600.), background:Color::rgb(20,40,60)}} />
        },
        rsx! {
            <Element style={{width:Length::px(200.), height:Length::px(120.), background:Color::rgb(220,80,40), opacity:Opacity::new(0.5)}}>
                <Element style={{width:Length::px(120.), height:Length::px(60.), background:Color::rgb(40,160,220), opacity:Opacity::new(0.6)}} />
            </Element>
        },
        rsx! {
            <Element style={{width:Length::px(200.), height:Length::px(120.), layout:Layout::flow().column(), scroll_direction:ScrollDirection::Vertical, background:Color::rgb(60,60,60)}}>
                {(0..12).map(|i| rsx! {
                    <Element style={{width:Length::px(180.), height:Length::px(24.), background:Color::rgb(80+(i%5) as u8*30,200,90)}}>
                        <Text>{format!("Row {i}")}</Text>
                    </Element>
                }).collect::<Vec<_>>()}
            </Element>
        },
        rsx! {
            <Element style={{width:Length::px(400.), height:Length::px(600.), background:Color::rgb(20,40,60)}} />
        },
    ]
}

/// Absolutely positioned children clipped to their anchor parent and to the
/// viewport, both below the fold of the outer scroll container until it
/// scrolls. Their chunks carry clip nodes of their own.
fn clipped_chunks() -> Vec<RsxNode> {
    vec![
        rsx! {
            <Element style={{width:Length::px(400.), height:Length::px(600.), background:Color::rgb(20,40,60)}} />
        },
        rsx! {
            <Element style={{width:Length::px(140.), height:Length::px(110.), layout:Layout::flow().column(), background:Color::rgb(70,70,70)}}>
                <Element style={{width:Length::px(56.), height:Length::px(26.), background:Color::rgb(30,80,220)}} anchor="clipped_anchor" />
                <Element style={{
                    position: Position::absolute()
                        .anchor("clipped_anchor")
                        .top(Length::px(0.))
                        .left(Length::px(38.))
                        .clip(ClipMode::AnchorParent),
                    width: Length::px(150.),
                    height: Length::px(22.),
                    background: Color::rgb(40,200,90),
                }} />
            </Element>
        },
        rsx! {
            <Element style={{width:Length::px(110.), height:Length::px(110.), background:Color::rgb(90,60,60)}}>
                <Element style={{
                    position: Position::absolute()
                        .top(Length::px(56.))
                        .left(Length::px(20.))
                        .clip(ClipMode::Viewport),
                    width: Length::px(140.),
                    height: Length::px(24.),
                    background: Color::rgb(240,160,20),
                }} />
            </Element>
        },
        rsx! {
            <Element style={{width:Length::px(400.), height:Length::px(600.), background:Color::rgb(20,40,60)}} />
        },
    ]
}

/// `content` in the outer scroll container.
fn scrolled(content: Vec<RsxNode>) -> RsxNode {
    rsx! {
        <Element style={{width:Length::px(400.), height:Length::px(400.), layout:Layout::flow().column(), scroll_direction:ScrollDirection::Vertical}}>
            {content}
        </Element>
    }
}

/// `content` moved up by `offset` without any scroll container: what the
/// scroll container must show once scrolled by `offset`.
fn moved_up(content: Vec<RsxNode>, offset: f32) -> RsxNode {
    rsx! {
        <Element style={{width:Length::px(400.), height:Length::px(400.)}}>
            <Element style={{
                position: Position::absolute().left(Length::px(0.)).top(Length::px(-offset)),
                width: Length::px(400.),
                layout: Layout::flow().column(),
            }}>
                {content}
            </Element>
        </Element>
    }
}

/// Scrolls `content` through offsets in one viewport and requires each frame
/// to equal `content` moved up by the offset in a fresh viewport.
fn assert_scrolled_in_place(content: fn() -> Vec<RsxNode>) -> Result<(), String> {
    let gpu = gpu::Gpu::new()?;
    let offsets = [0., 300., 500., 640.];
    for dpr in [1, 2] {
        let size = [400 * dpr, 400 * dpr];
        for mode in [
            ViewportPaintRendererMode::Legacy,
            ViewportPaintRendererMode::RetainedAuto,
        ] {
            let render = |viewport: &mut Viewport, tree: &RsxNode| -> Result<Vec<u8>, String> {
                let output = viewport.render_rsx_offscreen_for_test(
                    tree,
                    gpu.device.clone(),
                    gpu.queue.clone(),
                    size,
                    dpr as f32,
                    Instant::now(),
                )?;
                gpu.read(&output.texture, size)
            };
            let mut viewport = Viewport::new();
            viewport.set_paint_renderer_mode(mode);
            for (frame, &offset) in offsets.iter().enumerate() {
                if frame > 0 {
                    let arena = viewport.node_arena();
                    let root = arena.roots()[0];
                    let id = arena.get(root).unwrap().element.stable_id();
                    assert!(set_scroll_offset_by_id(arena, root, id, (0., offset)));
                }
                let pixels = render(&mut viewport, &scrolled(content()))?;
                let mut fresh = Viewport::new();
                fresh.set_paint_renderer_mode(mode);
                let expected = render(&mut fresh, &moved_up(content(), offset))?;
                let differing = pixels
                    .chunks_exact(4)
                    .zip(expected.chunks_exact(4))
                    .filter(|(a, b)| a != b)
                    .count();
                assert_eq!(
                    differing, 0,
                    "{mode:?} pixels differ from the moved-up content at DPR {dpr} offset {offset}"
                );
            }
        }
    }
    Ok(())
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn surfaces_nested_in_scrolled_content_stay_in_place() -> Result<(), String> {
    assert_scrolled_in_place(nested_surfaces)
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn clipped_chunks_in_scrolled_content_stay_in_place() -> Result<(), String> {
    assert_scrolled_in_place(clipped_chunks)
}
