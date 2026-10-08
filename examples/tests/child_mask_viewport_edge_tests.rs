//! A rounded owner that clips its children stays in the retained renderer
//! while it lies wholly above or left of the viewport, inside the paint
//! interest overscan of a window that is still partly visible.
use rfgui::style::{BorderRadius, Color, Layout, Length, Position};
use rfgui::time::Instant;
use rfgui::ui::{RsxNode, rsx};
use rfgui::view::viewport::ViewportPaintRendererMode;
use rfgui::view::{Element, Text, Viewport};

#[path = "../../lib/rfgui-components/tests/retained_controls/gpu.rs"]
mod gpu;

const SIZE: [u32; 2] = [160, 120];

/// A window with a rounded title bar; the bar's leading badge is rounded too
/// and holds a child larger than itself.
fn window_at([left, top]: [f32; 2]) -> RsxNode {
    rsx! {
        <Element style={{
            position: Position::absolute().left(Length::px(left)).top(Length::px(top)),
            width: Length::px(120.),
            height: Length::px(90.),
            layout: Layout::flow().column(),
            background: Color::rgb(30, 40, 60),
        }}>
            <Element style={{
                width: Length::px(120.),
                height: Length::px(24.),
                layout: Layout::flow().row(),
                border_radius: BorderRadius::uniform(Length::px(12.)),
                background: Color::rgb(90, 100, 130),
            }}>
                <Element style={{
                    width: Length::px(20.),
                    height: Length::px(20.),
                    border_radius: BorderRadius::uniform(Length::px(10.)),
                    background: Color::rgb(40, 160, 90),
                }}>
                    <Element style={{
                        width: Length::px(30.),
                        height: Length::px(30.),
                        background: Color::rgb(220, 60, 40),
                    }} />
                </Element>
                <Text style={{ color: Color::rgb(240, 240, 240) }}>Title</Text>
            </Element>
            <Element style={{
                width: Length::px(120.),
                height: Length::px(66.),
                background: Color::rgb(60, 70, 90),
            }} />
        </Element>
    }
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn child_masks_beyond_the_top_left_edge_render_retained_like_legacy() -> Result<(), String> {
    let gpu = gpu::Gpu::new()?;
    // The title bar wholly above the viewport; the badge wholly left of it;
    // both at once.
    for at in [[10.0, -30.0], [-30.0, 10.0], [-30.0, -30.0]] {
        for dpr in [1_u32, 2] {
            let size = [SIZE[0] * dpr, SIZE[1] * dpr];
            let mut frames = Vec::new();
            for mode in [
                ViewportPaintRendererMode::Legacy,
                ViewportPaintRendererMode::RetainedAuto,
            ] {
                let mut viewport = Viewport::new();
                // Moved there from inside the viewport, as a drag does, then
                // rendered fresh.
                for (fresh, at) in [(false, [10.0, 10.0]), (false, at), (true, at)] {
                    if fresh {
                        viewport = Viewport::new();
                    }
                    viewport.set_paint_renderer_mode(mode);
                    let frame = viewport
                        .render_rsx_offscreen_for_test(
                            &window_at(at),
                            gpu.device.clone(),
                            gpu.queue.clone(),
                            size,
                            dpr as f32,
                            Instant::now(),
                        )
                        .map_err(|error| format!("{mode:?} at {at:?} DPR {dpr}: {error}"))?;
                    frames.push(gpu.read(&frame.texture, size)?);
                }
            }
            let (legacy, retained) = frames.split_at(3);
            assert!(
                legacy[1] == legacy[2],
                "legacy pixels differ after moving to {at:?} DPR {dpr}"
            );
            assert!(
                legacy == retained,
                "retained pixels differ from legacy at {at:?} DPR {dpr}"
            );
        }
    }
    Ok(())
}
