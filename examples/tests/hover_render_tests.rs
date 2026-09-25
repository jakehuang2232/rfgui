//! Real pointer dispatch, dirty consumption and pixel output in both renderers.
use rfgui::style::{Color, Length};
use rfgui::time::Instant;
use rfgui::ui::rsx;
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
