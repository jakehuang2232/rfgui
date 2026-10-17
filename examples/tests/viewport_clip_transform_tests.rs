//! A `ClipMode::Viewport` element escapes every ancestor clip, also the one
//! its transformed or translucent ancestors composite under, while it still
//! follows their transform and opacity.
use rfgui::style::{
    Angle, Animation, Animator, BoxShadow, ClipMode, Color, Direction, FillMode, Keyframe, Layout,
    Length, Opacity, Padding, Position, Repeat, Rotate, Scale, Transform, Translate,
};
use rfgui::time::{Duration, Instant};
use rfgui::ui::{RsxNode, render_root, rsx};
use rfgui::view::{Element, Viewport};

#[path = "../../lib/rfgui-components/tests/retained_controls/gpu.rs"]
mod gpu;

const SIZE: [u32; 2] = [320, 260];

/// An endless ±7px vertical bob.
fn float() -> Animator {
    let frame = |offset: f32| {
        rfgui::style! {
            transform: Transform::new([Translate::xy(Length::Zero, Length::px(offset))]),
        }
    };
    Animator::new([Animation::new([
        Keyframe::new(0.0, frame(-7.0)),
        Keyframe::new(1.0, frame(7.0)),
    ])])
    .duration(3_000)
    .delay(-400)
    .repeat(Repeat::Infinite)
    .direction(Direction::Alternate)
    .fill_mode(FillMode::Both)
    .ease_in_out()
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Ancestor {
    None,
    /// A box bobbing up and down by an animation.
    Floating,
    Rotated,
    Translucent,
    /// A rotated box inside a floating one.
    Nested,
}

/// A glowing card escaping clips to the viewport at the top of a padded
/// row, inside `ancestor`, optionally with a transform of its own.
fn scene(ancestor: Ancestor, transformed: bool) -> RsxNode {
    let transform = if transformed {
        Transform::new([Scale::uniform(1.0)])
    } else {
        Transform::default()
    };
    let content = rsx! {
        <Element style={{ width: Length::px(160.0), height: Length::px(100.0) }}>
            <Element style={{
                position: Position::absolute()
                    .left(Length::Zero)
                    .top(Length::Zero)
                    .clip(ClipMode::Viewport),
                width: Length::px(160.0),
                height: Length::px(100.0),
                border_radius: Length::px(24.0),
                background: Color::rgb(60, 60, 60),
                transform: transform,
                box_shadow: vec![BoxShadow::new().color(Color::rgba(255, 120, 80, 220)).blur(40.0)],
            }} />
        </Element>
    };
    let rotated = |child: RsxNode| {
        rsx! {
            <Element style={{
                layout: Layout::flow(),
                transform: Transform::new([Rotate::z(Angle::deg(8.0))]),
            }}>
                {child}
            </Element>
        }
    };
    let floating = |child: RsxNode| {
        rsx! { <Element style={{ layout: Layout::flow(), animator: float() }}>{child}</Element> }
    };
    let content = match ancestor {
        Ancestor::None => content,
        Ancestor::Floating => floating(content),
        Ancestor::Rotated => rotated(content),
        Ancestor::Translucent => rsx! {
            <Element style={{ layout: Layout::flow(), opacity: Opacity::new(0.8) }}>
                {content}
            </Element>
        },
        Ancestor::Nested => floating(rotated(content)),
    };
    rsx! {
        <Element style={{
            width: Length::px(SIZE[0] as f32),
            height: Length::px(SIZE[1] as f32),
            background: Color::rgb(0, 0, 40),
            padding: Padding::uniform(Length::px(80.0)),
            layout: Layout::flex().row(),
        }}>
            {content}
        </Element>
    }
}

fn render(gpu: &gpu::Gpu, scene: RsxNode) -> Result<Vec<u8>, String> {
    let mut viewport = Viewport::new();
    let root = render_root(|| scene);
    let start = Instant::now();
    let mut last = None;
    for frame in 0..10u64 {
        last = Some(viewport.render_rsx_offscreen_for_test(
            &root,
            gpu.device.clone(),
            gpu.queue.clone(),
            SIZE,
            1.0,
            start + Duration::from_millis(frame * 50),
        )?);
    }
    gpu.read(&last.ok_or("no frame")?.texture, SIZE)
}

/// Glow pixels in the band above the row's content box, where only an
/// escaped glow can reach.
fn glow_above_content(pixels: &[u8]) -> usize {
    (40..70)
        .flat_map(|y| (80..240).map(move |x| (x, y)))
        .filter(|&(x, y)| pixels[((y * SIZE[0] + x) * 4) as usize] > 20)
        .count()
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn viewport_clip_escapes_the_clip_its_ancestor_surfaces_composite_under() -> Result<(), String> {
    let gpu = gpu::Gpu::new()?;
    let still = glow_above_content(&render(&gpu, scene(Ancestor::None, false))?);
    assert!(
        still > 1_000,
        "the glow reaches above the content box: {still}"
    );
    let mut failures = Vec::new();
    for ancestor in [
        Ancestor::Floating,
        Ancestor::Rotated,
        Ancestor::Translucent,
        Ancestor::Nested,
    ] {
        for transformed in [false, true] {
            let drawn = glow_above_content(&render(&gpu, scene(ancestor, transformed))?);
            if drawn * 2 < still {
                failures.push(format!(
                    "{ancestor:?}, own transform {transformed}: {drawn} of {still} glow pixels"
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
    Ok(())
}
