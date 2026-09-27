//! Real pointer dispatch, dirty consumption and pixel output in both renderers.
use rfgui::style::{Color, Length, Position};
use rfgui::time::Instant;
use rfgui::ui::{RsxNode, rsx};
use rfgui::view::viewport::ViewportPaintRendererMode;
use rfgui::view::{Element, Viewport};

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
        let mut reference = Vec::new();
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
                if mode == ViewportPaintRendererMode::Legacy {
                    reference.push(pixels);
                } else {
                    assert_eq!(
                        pixels, reference[frame],
                        "fragment parity DPR {dpr} frame {frame}"
                    );
                }
            }
        }
    }
    Ok(())
}
