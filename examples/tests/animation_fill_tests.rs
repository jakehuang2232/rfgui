//! An animation without a forwards fill stops affecting its element once
//! its active interval ends, even when no frame lands near the end. The
//! timeline follows the frame clock, not the frames that were drawn.
use rfgui::style::{
    Angle, Animation, Animator, Color, FillMode, Keyframe, Layout, Length, Opacity, Perspective,
    Rotate, Style, Transform, Translate,
};
use rfgui::time::{Duration, Instant};
use rfgui::ui::{RsxNode, rsx};
use rfgui::view::{Element, Viewport};

#[path = "../../lib/rfgui-components/tests/retained_controls/gpu.rs"]
mod gpu;

const SIDE: u32 = 100;
const BOXES: u32 = 5;
const SIZE: [u32; 2] = [SIDE * BOXES, SIDE];

fn flip(opacity: f32, degrees: f32) -> Style {
    rfgui::style! {
        opacity: Opacity::new(opacity),
        transform: Transform::new([Perspective::px(600.0), Rotate::x(Angle::deg(degrees))]),
    }
}

/// Fades in while turning up from lying face-down, like the intro
/// deck's title letters.
fn flip_in(delay_ms: i32) -> Animator {
    Animator::new([Animation::new([
        Keyframe::new(0.0, flip(0.0, -100.0)),
        Keyframe::new(1.0, flip(1.0, 0.0)),
    ])])
    .duration(500)
    .delay(delay_ms)
    .fill_mode(FillMode::Backwards)
    .linear()
}

/// A long animation that leaves opacity and the turn alone.
fn hold_still() -> Animator {
    let still =
        || rfgui::style! { transform: Transform::new([Translate::xy(Length::Zero, Length::Zero)]) };
    Animator::new([Animation::new([
        Keyframe::new(0.0, still()),
        Keyframe::new(1.0, still()),
    ])])
    .duration(5_000)
}

#[derive(Clone, Copy, PartialEq)]
enum Stage {
    Start,
    Swapped,
    Late,
}

/// White boxes, left to right: flipping in; flipping in after 300ms; flip
/// removed at `Swapped`; flip replaced at `Swapped`; flip attached at `Late`.
fn scene(stage: Stage) -> RsxNode {
    let animators = [
        Some(flip_in(0)),
        Some(flip_in(300)),
        (stage == Stage::Start).then(|| flip_in(0)),
        Some(if stage == Stage::Start {
            flip_in(0)
        } else {
            hold_still()
        }),
        (stage == Stage::Late).then(|| flip_in(0)),
    ];
    let side = Length::px(SIDE as f32);
    let boxes = animators
        .into_iter()
        .enumerate()
        .map(|(index, animator)| match animator {
            Some(animator) => rsx! {
                <Element key={index} style={{
                    width: side,
                    height: side,
                    background: Color::rgb(255, 255, 255),
                    animator: animator,
                }} />
            },
            None => rsx! {
                <Element key={index} style={{
                    width: side,
                    height: side,
                    background: Color::rgb(255, 255, 255),
                }} />
            },
        })
        .collect::<Vec<RsxNode>>();
    rsx! {
        <Element style={{
            width: Length::px(SIZE[0] as f32),
            height: side,
            layout: Layout::flex().row(),
        }}>
            {boxes}
        </Element>
    }
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn unfilled_animations_release_their_element_after_a_frame_gap() -> Result<(), String> {
    let gpu = gpu::Gpu::new()?;
    let start = Instant::now();
    let mut viewport = Viewport::new();
    let mut failures = Vec::new();
    const ANY: Option<u8> = None;
    const CLEAR: Option<u8> = Some(0);
    const FIFTH: Option<u8> = Some(51);
    const FULL: Option<u8> = Some(255);
    // (ms, stage, expected alpha at each box's centre). The turn pivots on
    // the centre, so only a box at rest or fully clear reads the same near
    // its corner.
    let frames = [
        (0, Stage::Start, [ANY; 5]),
        (16, Stage::Start, [ANY; 5]),
        (100, Stage::Start, [FIFTH, CLEAR, FIFTH, FIFTH, FULL]),
        // Removing or replacing a running flip hands the box back at once.
        (116, Stage::Swapped, [ANY, CLEAR, FULL, FULL, FULL]),
        // No frames until long after both flips should have ended: one was
        // midway, the other still in its delay.
        (2_000, Stage::Swapped, [FULL; 5]),
        (2_016, Stage::Swapped, [FULL; 5]),
        // A flip attached after an idle stretch starts on the frame that
        // attaches it, not back where the previous frame was drawn.
        (5_000, Stage::Late, [FULL, FULL, FULL, FULL, CLEAR]),
        (5_100, Stage::Late, [ANY, ANY, ANY, ANY, FIFTH]),
        (5_600, Stage::Late, [ANY, ANY, ANY, ANY, FULL]),
    ];
    for (ms, stage, expected) in frames {
        let frame = viewport.render_rsx_offscreen_for_test(
            &scene(stage),
            gpu.device.clone(),
            gpu.queue.clone(),
            SIZE,
            1.0,
            start + Duration::from_millis(ms),
        )?;
        let pixels = gpu.read(&frame.texture, SIZE)?;
        for (index, expected) in expected.into_iter().enumerate() {
            let Some(expected) = expected else { continue };
            let left = index as u32 * SIDE;
            let mut points = vec![("centre", left + SIDE / 2, SIDE / 2)];
            if expected != FIFTH.unwrap() {
                points.push(("corner", left + 10, 10));
            }
            for (point, x, y) in points {
                let alpha = pixels[((y * SIZE[0] + x) * 4 + 3) as usize];
                if alpha.abs_diff(expected) > 4 {
                    failures.push(format!(
                        "{ms}ms box {index} {point}: alpha {alpha}, expected {expected}"
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    Ok(())
}
