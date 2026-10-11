//! An auto-sized inline box holding only empty text is as big as its
//! padding, on the retained renderer, under a transformed ancestor whose
//! surface it must not stretch.
use rfgui::style::{Align, Color, Layout, Length, Padding, Transform, Translate};
use rfgui::time::Instant;
use rfgui::ui::{RsxNode, rsx};
use rfgui::view::{Element, Text, Viewport};

#[path = "../../lib/rfgui-components/tests/retained_controls/gpu.rs"]
mod gpu;

const SIZE: [u32; 2] = [400, 200];

/// `body` followed by a blue marker, as flex items of a transformed row:
/// the marker sits where `body` ends.
fn scene(body: RsxNode) -> RsxNode {
    rsx! {
        <Element style={{width: Length::px(400.0), height: Length::px(200.0)}}>
            <Element style={{
                width: Length::px(400.0),
                height: Length::px(200.0),
                padding: Padding::uniform(Length::px(40.0)),
                layout: Layout::flex().row().align(Align::Start),
                transform: Transform::new([Translate::xy(Length::Zero, Length::px(0.0))]),
            }}>
                {body}
                <Element style={{
                    width: Length::px(20.0),
                    height: Length::px(20.0),
                    background: Color::rgb(0, 0, 255),
                }} />
            </Element>
        </Element>
    }
}

/// Padding, 28x20 in all, around `text`.
fn padded(text: &str) -> RsxNode {
    rsx! {
        <Element style={{padding: Padding::uniform(Length::px(10.0)).x(Length::px(14.0))}}>
            <Text>{text}</Text>
        </Element>
    }
}

/// The 28x20 box `padded("")` must be.
fn sized() -> RsxNode {
    rsx! {
        <Element style={{width: Length::px(28.0), height: Length::px(20.0)}} />
    }
}

/// Renders `scene(body)` for each body in turn and returns the last frame.
fn render(gpu: &gpu::Gpu, bodies: &[RsxNode], dpr: u32) -> Result<Vec<u8>, String> {
    let size = SIZE.map(|n| n * dpr);
    let mut viewport = Viewport::new();
    let mut pixels = Vec::new();
    for body in bodies {
        let frame = viewport.render_rsx_offscreen_for_test(
            &scene(body.clone()),
            gpu.device.clone(),
            gpu.queue.clone(),
            size,
            dpr as f32,
            Instant::now(),
        )?;
        assert!(frame.artifact_selected);
        for (target, extent) in &frame.persistent_targets {
            assert!(
                extent[0] <= size[0] && extent[1] <= size[1],
                "DPR {dpr}: {target} is {extent:?}"
            );
        }
        pixels = gpu.read(&frame.texture, size)?;
    }
    Ok(pixels)
}

/// Fresh, or after its text empties, the box places the marker where a
/// 28x20 box does.
#[test]
#[ignore = "requires native hardware graphics adapter"]
fn inline_box_holding_only_empty_text_is_as_big_as_its_padding() -> Result<(), String> {
    let gpu = gpu::Gpu::new()?;
    for dpr in [1_u32, 2] {
        let expected = render(&gpu, &[sized()], dpr)?;
        // The marker shows, its center 78px in.
        let at = ((50 * dpr * SIZE[0] * dpr + 78 * dpr) * 4) as usize;
        assert_eq!(expected[at..at + 4], [0, 0, 255, 255], "DPR {dpr}");
        for bodies in [vec![padded("")], vec![padded("Lots"), padded("")]] {
            let pixels = render(&gpu, &bodies, dpr)?;
            assert!(
                pixels == expected,
                "DPR {dpr}, {} frames: the box is not 28x20",
                bodies.len()
            );
        }
    }
    Ok(())
}
