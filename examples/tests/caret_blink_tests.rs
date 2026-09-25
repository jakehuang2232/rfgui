//! Exercise retained deadlines through the real RSX entry and GPU submission.
use rfgui::time::{Duration, Instant};
use rfgui::ui::{RsxNode, RsxTagDescriptor, next_timer_deadline, run_due_timers};
use rfgui::view::Viewport;
use rfgui::view::viewport::ViewportPaintRendererMode;

#[path = "../../lib/rfgui-components/tests/retained_controls/gpu.rs"]
mod gpu;

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn caret_deadlines_submit_only_blink_frames_in_both_renderers() -> Result<(), String> {
    let gpu = gpu::Gpu::new()?;
    for mode in [
        ViewportPaintRendererMode::Legacy,
        ViewportPaintRendererMode::RetainedAuto,
    ] {
        for dpr in [1_u32, 2] {
            let root = RsxNode::tagged(
                "TextArea",
                RsxTagDescriptor::for_tag::<rfgui::view::TextArea>(),
            )
            .with_prop("content", "M".to_string());
            let mut viewport = Viewport::new();
            viewport.set_paint_renderer_mode(mode);
            let start = Instant::now();
            let size = [120 * dpr, 60 * dpr];
            let render = |viewport: &mut Viewport, now| {
                viewport.render_rsx_offscreen_for_test(
                    &root,
                    gpu.device.clone(),
                    gpu.queue.clone(),
                    size,
                    dpr as f32,
                    now,
                )
            };
            render(&mut viewport, start)?;
            let owner = viewport.node_arena().roots()[0];
            viewport.set_focused_node_id(Some(owner));
            let visible = render(&mut viewport, start)?;
            let visible_pixels = gpu.read(&visible.texture, size)?;
            // Focus dispatch may request a follow-up for event handlers.
            viewport.drain_platform_requests();
            let mut frames = 0;
            for millis in 1..=1_590 {
                let now = start + Duration::from_millis(millis);
                run_due_timers(now);
                if !viewport.redraw_requested() {
                    continue;
                }
                frames += 1;
                assert!(viewport.drain_platform_requests().request_redraw);
                let frame = render(&mut viewport, now)?;
                let pixels = gpu.read(&frame.texture, size)?;
                if millis == 1_060 {
                    assert_eq!(pixels, visible_pixels, "visible caret must return");
                } else {
                    assert_ne!(pixels, visible_pixels, "hidden caret must change pixels");
                }
                assert!(
                    !viewport.drain_platform_requests().request_redraw,
                    "painted blink must not queue a redundant frame: {mode:?} DPR {dpr}"
                );
                assert_eq!(
                    next_timer_deadline(),
                    Some(now + Duration::from_millis(530))
                );
            }
            assert_eq!(frames, 3, "no frames between blink deadlines");
            viewport.set_focused_node_id(None);
            render(&mut viewport, start + Duration::from_millis(1_600))?;
            assert_eq!(next_timer_deadline(), None);
        }
    }
    Ok(())
}
