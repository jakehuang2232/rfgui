//! Real pointer dispatch, dirty consumption and pixel output in both renderers.
use rfgui::style::{
    Color, Layout, Length, Position, ScrollDirection, Transition, TransitionProperty,
};
use rfgui::time::Instant;
use rfgui::ui::{PointerEnterHandlerProp, RsxNode, rsx};
use rfgui::view::viewport::ViewportPaintRendererMode;
use rfgui::view::{Element, Viewport};
use std::cell::Cell;
use std::rc::Rc;

#[path = "../../lib/rfgui-components/tests/retained_controls/gpu.rs"]
mod gpu;

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn pointer_hover_pixels_restore_after_dirty_consumption_in_both_renderers() -> Result<(), String> {
    let gpu = gpu::Gpu::new()?;
    for mode in [
        ViewportPaintRendererMode::Legacy,
        ViewportPaintRendererMode::RetainedAuto,
    ] {
        for dpr in [1_u32, 2] {
            let root = rsx! {
                <Element style={{
                    width: Length::px(40.),
                    height: Length::px(40.),
                    background_color: Color::hex("#ff0000"),
                    hover: { background_color: Color::hex("#0000ff") },
                }} />
            };
            let mut viewport = Viewport::new();
            viewport.set_paint_renderer_mode(mode);
            let size = [64 * dpr, 64 * dpr];
            let render = |viewport: &mut Viewport| -> Result<Vec<u8>, String> {
                let frame = viewport.render_rsx_offscreen_for_test(
                    &root,
                    gpu.device.clone(),
                    gpu.queue.clone(),
                    size,
                    dpr as f32,
                    Instant::now(),
                )?;
                gpu.read(&frame.texture, size)
            };
            let idle = render(&mut viewport)?;
            let offset = ((10 * dpr * size[0] + 10 * dpr) * 4) as usize;
            assert_eq!(&idle[offset..offset + 4], &[255, 0, 0, 255]);
            viewport.set_pointer_position_viewport(10., 10.);
            viewport.dispatch_pointer_move_event();
            let hovered = render(&mut viewport)?;
            assert_eq!(&hovered[offset..offset + 4], &[0, 0, 255, 255]);
            viewport.set_pointer_position_viewport(11., 11.);
            viewport.dispatch_pointer_move_event();
            assert_eq!(render(&mut viewport)?, hovered);
            viewport.clear_pointer_position_viewport();
            assert_eq!(render(&mut viewport)?, idle, "{mode:?} DPR {dpr}");
        }
    }
    Ok(())
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn fragment_root_hover_crossing_and_leave_restore_pixels() -> Result<(), String> {
    let gpu = gpu::Gpu::new()?;
    for dpr in [1_u32, 2] {
        for mode in [
            ViewportPaintRendererMode::Legacy,
            ViewportPaintRendererMode::RetainedAuto,
        ] {
            let tree = RsxNode::fragment((0..2).map(|index| rsx! {
                <Element style={{
                    position: Position::absolute().left(Length::px(index as f32 * 64.)).top(Length::px(0.)),
                    width: Length::px(40.), height: Length::px(40.),
                    background_color: Color::hex("#ff0000"),
                    hover: { background_color: Color::hex("#0000ff") },
                }} />
            }).collect());
            let mut viewport = Viewport::new();
            viewport.set_paint_renderer_mode(mode);
            let now = Instant::now();
            let size = [128 * dpr, 64 * dpr];
            let mut idle = None;
            for (frame, pointer) in [None, Some(10.), Some(74.), Some(75.), None]
                .into_iter()
                .enumerate()
            {
                if let Some(x) = pointer {
                    viewport.set_pointer_position_viewport(x, 10.);
                    viewport.dispatch_pointer_move_event();
                } else {
                    viewport.clear_pointer_position_viewport();
                }
                let output = viewport.render_rsx_offscreen_for_test(
                    &tree,
                    gpu.device.clone(),
                    gpu.queue.clone(),
                    size,
                    dpr as f32,
                    now,
                )?;
                assert_eq!(viewport.node_arena().roots().len(), 2);
                let pixels = gpu.read(&output.texture, size)?;
                for (root, x) in [10, 74].into_iter().enumerate() {
                    let hovered = pointer.is_some_and(|p| (p >= 64.) == (root == 1));
                    let expected = if hovered {
                        [0, 0, 255, 255]
                    } else {
                        [255, 0, 0, 255]
                    };
                    let offset = ((10 * dpr * size[0] + x * dpr) * 4) as usize;
                    assert_eq!(
                        &pixels[offset..offset + 4],
                        &expected,
                        "{mode:?} DPR {dpr} frame {frame}"
                    );
                }
                if frame == 0 {
                    idle = Some(pixels.clone());
                }
                if frame == 4 {
                    assert_eq!(Some(&pixels), idle.as_ref());
                }
            }
        }
    }
    Ok(())
}

/// Wheel scrolling moves a box with a hover background transition under a
/// stationary pointer. The hover must not paint the transition's end value
/// for a frame before the transition starts from the old value.
#[test]
#[ignore = "requires native hardware graphics adapter"]
fn hover_transition_from_scrolling_under_a_still_pointer_does_not_flash() -> Result<(), String> {
    let gpu = gpu::Gpu::new()?;
    let size = [200, 200];
    let pointer = (60_u32, 190_u32);
    for mode in [
        ViewportPaintRendererMode::Legacy,
        ViewportPaintRendererMode::RetainedAuto,
    ] {
        let entered = Rc::new(Cell::new(0));
        let tree = {
            let entered = entered.clone();
            rsx! {
                <Element style={{width:Length::px(200.), height:Length::px(200.), layout:Layout::flow().column(), scroll_direction:ScrollDirection::Vertical}}>
                    <Element style={{width:Length::px(200.), height:Length::px(300.)}} />
                    <Element
                        style={{
                            width: Length::px(120.),
                            height: Length::px(120.),
                            background_color: Color::hex("#ff0000"),
                            hover: { background_color: Color::hex("#0000ff") },
                            transition: [Transition::new(TransitionProperty::BackgroundColor, 150)],
                        }}
                        on_pointer_enter={PointerEnterHandlerProp::new(move |_| entered.set(entered.get() + 1))}
                    />
                    <Element style={{width:Length::px(200.), height:Length::px(600.)}} />
                </Element>
            }
        };
        let mut viewport = Viewport::new();
        viewport.set_paint_renderer_mode(mode);
        let start = Instant::now();
        let mut painted = Vec::new();
        for frame in 0..60_u64 {
            if frame == 1 {
                viewport.set_pointer_position_viewport(pointer.0 as f32, pointer.1 as f32);
                viewport.dispatch_pointer_move_event();
            }
            if (2..40).contains(&frame) && frame % 3 == 0 {
                viewport.dispatch_pointer_wheel_event(0.0, 40.0);
            }
            let output = viewport.render_rsx_offscreen_for_test(
                &tree,
                gpu.device.clone(),
                gpu.queue.clone(),
                size,
                1.0,
                start + std::time::Duration::from_millis(frame * 16),
            )?;
            let pixels = gpu.read(&output.texture, size)?;
            let offset = ((pointer.1 * size[0] + pointer.0) * 4) as usize;
            let [red, _, blue, _] = [0, 1, 2, 3].map(|channel| pixels[offset + channel]);
            if red != 0 || blue != 0 {
                painted.push((frame, red, blue));
            }
        }
        assert!(
            painted
                .first()
                .is_some_and(|&(_, red, blue)| (red, blue) == (255, 0)),
            "{mode:?}: the box enters under the pointer unhovered: {painted:?}"
        );
        assert!(
            painted
                .last()
                .is_some_and(|&(_, red, blue)| (red, blue) == (0, 255)),
            "{mode:?}: the hover transition completes: {painted:?}"
        );
        assert!(
            painted
                .windows(2)
                .all(|pair| pair[1].1 <= pair[0].1 && pair[1].2 >= pair[0].2),
            "{mode:?}: the box color moves only toward the hover color: {painted:?}"
        );
        assert_eq!(entered.get(), 1, "{mode:?}");
    }
    Ok(())
}

/// A hovered scroll container shows its scrollbar, also when it is nested in
/// another element.
#[test]
#[ignore = "requires native hardware graphics adapter"]
fn hovered_nested_scroll_container_paints_its_scrollbar_in_both_renderers() -> Result<(), String> {
    let gpu = gpu::Gpu::new()?;
    let tree = rsx! {
        <Element style={{width:Length::px(240.), height:Length::px(240.), background_color:Color::hex("#1e1e1e")}}>
            <Element style={{width:Length::px(200.), height:Length::px(200.), layout:Layout::flow().column(), scroll_direction:ScrollDirection::Vertical, background_color:Color::hex("#0a0a0a")}}>
                <Element style={{width:Length::px(150.), height:Length::px(600.), background_color:Color::hex("#14283c")}} />
            </Element>
        </Element>
    };
    for dpr in [1_u32, 2] {
        let size = [240 * dpr, 240 * dpr];
        // The scrollbar sits at the scroll container's right edge, beside
        // the content column.
        let scrollbar_pixels = |pixels: &[u8]| {
            (0..200 * dpr)
                .flat_map(|y| (180 * dpr..200 * dpr).map(move |x| (x, y)))
                .filter(|&(x, y)| {
                    let offset = ((y * size[0] + x) * 4) as usize;
                    pixels[offset..offset + 3].iter().any(|&c| c > 60)
                })
                .count()
        };
        for mode in [
            ViewportPaintRendererMode::Legacy,
            ViewportPaintRendererMode::RetainedAuto,
        ] {
            let mut viewport = Viewport::new();
            viewport.set_paint_renderer_mode(mode);
            let now = Instant::now();
            for frame in 0..2 {
                if frame == 1 {
                    viewport.set_pointer_position_viewport(80., 80.);
                    viewport.dispatch_pointer_move_event();
                }
                let output = viewport.render_rsx_offscreen_for_test(
                    &tree,
                    gpu.device.clone(),
                    gpu.queue.clone(),
                    size,
                    dpr as f32,
                    now,
                )?;
                let pixels = gpu.read(&output.texture, size)?;
                assert_eq!(
                    scrollbar_pixels(&pixels) > 0,
                    frame == 1,
                    "{mode:?} DPR {dpr} frame {frame}: the scrollbar shows only while hovered"
                );
            }
        }
    }
    Ok(())
}
