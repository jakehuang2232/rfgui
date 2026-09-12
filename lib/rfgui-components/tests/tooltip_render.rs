//! Deferred tooltip glyphs after nested rounded clipping, through real
//! components, pointer events, layout and rendering on a native GPU.
use rfgui::style::{Layout, Length, Padding};
use rfgui::ui::{PointerEnterHandlerProp, PointerLeaveHandlerProp, RsxNode, rsx};
use rfgui::view::base_component::BoxModelSnapshot;
use rfgui::view::viewport::ViewportPaintRendererMode;
use rfgui::view::{Element, NodeArena, NodeKey, Text, Viewport};
use rfgui_components::material_symbol::FavoriteIcon;
use rfgui_components::{Accordion, Button, Tooltip, TooltipPlacement, Window, use_tooltip_ref};

#[path = "retained_controls/gpu.rs"]
mod gpu;

const SIZE: [u32; 2] = [640, 480];
const LABEL: &str = "Tooltip text";
const PLACEMENTS: [TooltipPlacement; 12] = [
    TooltipPlacement::Top,
    TooltipPlacement::TopStart,
    TooltipPlacement::TopEnd,
    TooltipPlacement::Bottom,
    TooltipPlacement::BottomStart,
    TooltipPlacement::BottomEnd,
    TooltipPlacement::Left,
    TooltipPlacement::LeftStart,
    TooltipPlacement::LeftEnd,
    TooltipPlacement::Right,
    TooltipPlacement::RightStart,
    TooltipPlacement::RightEnd,
];

struct TooltipScene;

fn scene(placement: TooltipPlacement, rich: bool) -> RsxNode {
    rfgui::ui::rsx_scope(|| {
        rfgui::ui::render_component::<TooltipScene, _>(|| {
            let handle = use_tooltip_ref();
            let enter = handle.clone();
            let leave = handle.clone();
            let trigger = if rich {
                rsx! {
                    <Element
                        style={{layout: Layout::flow().row().no_wrap(), padding: Padding::uniform(Length::px(8.0))}}
                        on_pointer_enter={PointerEnterHandlerProp::new(move |_| enter.show())}
                        on_pointer_leave={PointerLeaveHandlerProp::new(move |_| leave.hide())}
                    >
                        <Text>"Hover target"</Text>
                        <Tooltip handle={handle} placement={placement}>
                            <Element style={{layout: Layout::flow().row().no_wrap(), gap: Length::px(4.0)}}>
                                <FavoriteIcon />
                                <Text>{LABEL}</Text>
                            </Element>
                        </Tooltip>
                    </Element>
                }
            } else {
                rsx! {
                    <Button tooltip={Some(rsx! {<Tooltip placement={placement}>{LABEL}</Tooltip>})}>
                        "Hover target"
                    </Button>
                }
            };
            rsx! {
                <Window title="Component Test" width=500.0 height=400.0 position={(70.0, 45.0)}>
                    <Accordion title="Tooltip" default_expanded={Some(true)}>
                        <Text>"Button with tooltip (placement)"</Text>
                        <Element style={{layout: Layout::flow().row().no_wrap(), padding: Padding::uniform(Length::px(20.0))}}>
                            {trigger}
                        </Element>
                    </Accordion>
                </Window>
            }
        })
    })
}

fn text_bounds(viewport: &Viewport, needle: &str) -> Option<BoxModelSnapshot> {
    fn find(arena: &NodeArena, key: NodeKey, needle: &str) -> Option<BoxModelSnapshot> {
        let node = arena.get(key)?;
        if node
            .element
            .as_any()
            .downcast_ref::<rfgui::view::base_component::Text>()
            .is_some_and(|text| text.content() == needle)
        {
            return Some(node.element.box_model_snapshot());
        }
        drop(node);
        arena
            .children_of(key)
            .iter()
            .find_map(|key| find(arena, *key, needle))
    }
    viewport
        .node_arena()
        .roots()
        .iter()
        .find_map(|key| find(viewport.node_arena(), *key, needle))
}

fn assert_glyph_pixels(
    pixels: &[u8],
    bounds: BoxModelSnapshot,
    dpr: u32,
    minimum_ink: usize,
    case: &str,
) {
    assert!(
        bounds.should_render && bounds.width > 0.0 && bounds.height > 0.0,
        "{case}: {bounds:?}"
    );
    let scale = dpr as f32;
    let mut dark = 0;
    let mut light = 0;
    for y in (bounds.y * scale).ceil().max(0.0) as u32
        ..((bounds.y + bounds.height) * scale)
            .floor()
            .min((SIZE[1] * dpr) as f32) as u32
    {
        for x in (bounds.x * scale).ceil().max(0.0) as u32
            ..((bounds.x + bounds.width) * scale)
                .floor()
                .min((SIZE[0] * dpr) as f32) as u32
        {
            let i = ((y * SIZE[0] * dpr + x) * 4) as usize;
            let pixel = &pixels[i..i + 4];
            if pixel[3] > 200 {
                dark += usize::from(pixel[..3].iter().all(|channel| *channel < 50));
                light += usize::from(pixel[..3].iter().all(|channel| *channel > 80));
            }
        }
    }
    // Require dark glyphs AND their light inverse background. A blank bubble
    // or a missing bubble exposing the dark window must both fail, without
    // relying on the other renderer as the pixel oracle.
    assert!(
        dark >= minimum_ink && light > 20,
        "{case}: dark={dark}, light={light}, bounds={bounds:?}"
    );
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn tooltip_glyphs_survive_nested_clips_and_hover_remounts() -> Result<(), String> {
    // One device for the matrix, matching the renderer resource caches.
    let gpu = gpu::Gpu::new()?;
    let mut cases = 0;
    let mut visible_frames = 0;
    for mode in [
        ViewportPaintRendererMode::Legacy,
        ViewportPaintRendererMode::RetainedAuto,
    ] {
        for dpr in [1, 2] {
            for placement in PLACEMENTS {
                for rich in [false, true] {
                    let case = format!("{mode:?} {placement:?} DPR={dpr} rich={rich}");
                    let mut viewport = Viewport::new();
                    viewport.set_paint_renderer_mode(mode);
                    let now = rfgui::time::Instant::now();
                    let mut previous = None;
                    for frame in 0..7 {
                        if frame == 1 || frame == 4 {
                            let b = text_bounds(&viewport, "Hover target").expect("trigger exists");
                            viewport.set_pointer_position_viewport(
                                b.x + b.width / 2.0,
                                b.y + b.height / 2.0,
                            );
                            viewport.dispatch_pointer_move_event();
                        } else if frame == 3 || frame == 6 {
                            viewport.set_pointer_position_viewport(630.0, 470.0);
                            viewport.dispatch_pointer_move_event();
                        }
                        let output = viewport
                            .render_rsx_offscreen_for_test(
                                &scene(placement, rich),
                                gpu.device.clone(),
                                gpu.queue.clone(),
                                SIZE.map(|s| s * dpr),
                                dpr as f32,
                                now + std::time::Duration::from_secs(frame),
                            )
                            .map_err(|error| format!("{case} frame={frame}: {error}"))?;
                        if frame == 0 || frame == 3 || frame == 6 {
                            assert!(
                                text_bounds(&viewport, LABEL).is_none(),
                                "{case}: hidden tooltip must unmount"
                            );
                            continue;
                        }
                        let pixels = gpu.read(&output.texture, SIZE.map(|s| s * dpr))?;
                        let context = format!("{case} frame={frame}");
                        assert_glyph_pixels(
                            &pixels,
                            text_bounds(&viewport, LABEL).expect("tooltip text exists"),
                            dpr,
                            20,
                            &context,
                        );
                        if rich {
                            assert_glyph_pixels(
                                &pixels,
                                text_bounds(&viewport, "favorite").expect("tooltip icon exists"),
                                dpr,
                                8,
                                &context,
                            );
                        }
                        if frame == 2 || frame == 5 {
                            assert!(
                                previous.as_ref() == Some(&pixels),
                                "{context}: settled pixels must remain stable"
                            );
                        }
                        previous = Some(pixels);
                        visible_frames += 1;
                    }
                    cases += 1;
                }
            }
        }
    }
    eprintln!("tooltip glyph coverage: {cases} cases, {visible_frames} visible frames");
    Ok(())
}
