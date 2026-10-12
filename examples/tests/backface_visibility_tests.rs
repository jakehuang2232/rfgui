//! `backface_visibility: Hidden` drops every transformable host while its
//! own transform shows the viewer its back, and follows
//! an animated turn frame by frame.
use rfgui::style::{
    Angle, Animation, Animator, BackfaceVisibility, Color, FillMode, Keyframe, Layout, Length,
    Perspective, Rotate, Transform, TransformOrigin,
};
use rfgui::time::Instant;
use rfgui::ui::{RsxNode, rsx};
use rfgui::view::{Element, Image, ImageSource, Svg, SvgSource, Text, Viewport};
use std::sync::Arc;
use std::time::Duration;

#[path = "../../lib/rfgui-components/tests/retained_controls/gpu.rs"]
mod gpu;

const SIZE: [u32; 2] = [420, 160];
const SIDE: f32 = 120.0;

fn turned(degrees: f32) -> Transform {
    Transform::new([Perspective::px(600.0), Rotate::y(Angle::deg(degrees))])
}

#[derive(Clone, Copy, Debug)]
enum Host {
    Element,
    Text,
    Image,
    Svg,
}

fn face(host: Host, [r, g, b]: [u8; 3], degrees: f32, backface: BackfaceVisibility) -> RsxNode {
    let color = Color::rgb(r, g, b);
    match host {
        Host::Element => rsx! {
            <Element style={{
                width: Length::px(SIDE),
                height: Length::px(SIDE),
                background: color,
                transform_origin: TransformOrigin::center(),
                transform: turned(degrees),
                backface_visibility: backface,
            }} />
        },
        Host::Text => rsx! {
            <Text style={{
                font_size: 72.0,
                font_weight: 900,
                color: color,
                transform_origin: TransformOrigin::center(),
                transform: turned(degrees),
                backface_visibility: backface,
            }}>{"MM"}</Text>
        },
        Host::Image => {
            let pixels: Arc<[u8]> = [r, g, b, 255].repeat(16 * 16).into();
            rsx! {
                <Image
                    source={ImageSource::Rgba { width: 16, height: 16, pixels }}
                    style={{
                        width: Length::px(SIDE),
                        height: Length::px(SIDE),
                        transform_origin: TransformOrigin::center(),
                        transform: turned(degrees),
                        backface_visibility: backface,
                    }}
                />
            }
        }
        Host::Svg => {
            let content = format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><rect width="16" height="16" fill="rgb({r},{g},{b})"/></svg>"#
            );
            rsx! {
                <Svg
                    source={SvgSource::Content(content)}
                    style={{
                        width: Length::px(SIDE),
                        height: Length::px(SIDE),
                        transform_origin: TransformOrigin::center(),
                        transform: turned(degrees),
                        backface_visibility: backface,
                    }}
                />
            }
        }
    }
}

/// Red turned away and hidden, green turned away but visible, blue hidden
/// yet still facing the viewer.
fn faces(host: Host) -> RsxNode {
    rsx! {
        <Element style={{
            width: Length::px(420.0),
            height: Length::px(160.0),
            layout: Layout::flex().row(),
            gap: Length::px(20.0),
        }}>
            {face(host, [255, 0, 0], 140.0, BackfaceVisibility::Hidden)}
            {face(host, [0, 255, 0], 140.0, BackfaceVisibility::Visible)}
            {face(host, [0, 0, 255], 30.0, BackfaceVisibility::Hidden)}
        </Element>
    }
}

/// Pixels whose `channel` clearly dominates.
fn count(pixels: &[u8], channel: usize) -> usize {
    pixels
        .chunks_exact(4)
        .filter(|p| p[channel] > 128 && (0..3).all(|c| c == channel || p[c] < 64))
        .count()
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn hidden_backface_is_dropped_for_every_host() -> Result<(), String> {
    let gpu = gpu::Gpu::new()?;
    let start = Instant::now();
    for host in [Host::Element, Host::Text, Host::Image, Host::Svg] {
        let mut viewport = Viewport::new();
        let mut pixels = Vec::new();
        // Image and SVG sources may arrive a frame or two late.
        for frame in 0..4 {
            let rendered = viewport.render_rsx_offscreen_for_test(
                &faces(host),
                gpu.device.clone(),
                gpu.queue.clone(),
                SIZE,
                1.0,
                start + Duration::from_millis(frame * 16),
            )?;
            pixels = gpu.read(&rendered.texture, SIZE)?;
            assert_eq!(
                count(&pixels, 0),
                0,
                "{host:?}: the hidden back face painted"
            );
        }
        assert!(
            count(&pixels, 1) > 100,
            "{host:?}: the visible back face is missing"
        );
        assert!(
            count(&pixels, 2) > 100,
            "{host:?}: the front-facing face is missing"
        );
    }
    Ok(())
}

fn spinning() -> RsxNode {
    let frame = |degrees: f32| {
        let mut style = rfgui::style::Style::new();
        style.set_transform(turned(degrees));
        style
    };
    rsx! {
        <Element style={{
            width: Length::px(120.0),
            height: Length::px(120.0),
            background: Color::rgb(255, 0, 0),
            transform_origin: TransformOrigin::center(),
            backface_visibility: BackfaceVisibility::Hidden,
            animator: Animator::new([Animation::new([
                Keyframe::new(0.0, frame(0.0)),
                Keyframe::new(1.0, frame(180.0)),
            ])])
            .duration(1_000)
            .fill_mode(FillMode::Forwards)
            .linear(),
        }} />
    }
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn an_animated_turn_drops_the_face_once_it_shows_its_back() -> Result<(), String> {
    let gpu = gpu::Gpu::new()?;
    let start = Instant::now();
    let mut viewport = Viewport::new();
    let mut shown = Vec::new();
    for ms in (0..=1_200).step_by(100) {
        let frame = viewport.render_rsx_offscreen_for_test(
            &spinning(),
            gpu.device.clone(),
            gpu.queue.clone(),
            SIZE,
            1.0,
            start + Duration::from_millis(ms),
        )?;
        let pixels = gpu.read(&frame.texture, SIZE)?;
        shown.push((ms, count(&pixels, 0) > 0));
    }
    // Turned about its own centre, the face is edge-on at 90 degrees
    // (500ms, left unchecked) and shows its back after; it must stay gone
    // through the forwards fill.
    for (ms, visible) in shown.into_iter().filter(|&(ms, _)| ms != 500) {
        assert_eq!(visible, ms < 500, "at {ms}ms the face visible = {visible}");
    }
    Ok(())
}
