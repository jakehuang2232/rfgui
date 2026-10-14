//! Text shows `color` animating on an Element above it, as CSS inherits the
//! animated value, and runs its own style transitions like an Element.
use rfgui::style::{
    Animation, Animator, Color, Keyframe, Layout, Length, Opacity, Transition, TransitionProperty,
};
use rfgui::time::{Duration, Instant};
use rfgui::ui::{RsxNode, rsx};
use rfgui::view::{Element, Text, Viewport};

#[path = "../../lib/rfgui-components/tests/retained_controls/gpu.rs"]
mod gpu;

const SIZE: [u32; 2] = [300, 120];
const RED: Color = Color::rgb(255, 0, 0);
const BLUE: Color = Color::rgb(0, 0, 255);

fn fade(property: TransitionProperty) -> Vec<Transition> {
    vec![Transition::new(property, 1000).linear()]
}

/// The average color and alpha of the glyphs' solid pixels.
fn ink(pixels: &[u8]) -> [u32; 4] {
    let peak = pixels.chunks(4).map(|px| px[3]).max().unwrap_or(0);
    if peak <= 20 {
        return [0; 4];
    }
    let core = pixels
        .chunks(4)
        .filter(|px| px[3] >= peak - 10)
        .collect::<Vec<_>>();
    let mut sum = [0u32; 4];
    for px in &core {
        for i in 0..4 {
            sum[i] += px[i] as u32;
        }
    }
    sum.map(|value| value / core.len() as u32)
}

fn run(frames: &[(u64, RsxNode)]) -> Result<Vec<[u32; 4]>, String> {
    let gpu = gpu::Gpu::new()?;
    let start = Instant::now();
    let mut viewport = Viewport::new();
    let mut out = Vec::new();
    for (ms, scene) in frames {
        let frame = viewport.render_rsx_offscreen_for_test(
            scene,
            gpu.device.clone(),
            gpu.queue.clone(),
            SIZE,
            1.0,
            start + Duration::from_millis(*ms),
        )?;
        out.push(ink(&gpu.read(&frame.texture, SIZE)?));
    }
    Ok(out)
}

fn assert_color(label: &str, ink: [u32; 4], expected: [u32; 3]) {
    let off = (0..3).map(|i| ink[i].abs_diff(expected[i])).max().unwrap();
    assert!(off <= 12, "{label}: ink {ink:?}, expected {expected:?}");
}

/// A transition starts at the previous frame, so the change frame has moved
/// one frame's worth from the start color, not jumped to the target.
fn assert_near_start(label: &str, ink: [u32; 4]) {
    assert!(
        ink[0] >= 180 && ink[2] <= 60,
        "{label}: ink {ink:?}, expected close to red"
    );
}

fn assert_between(label: &str, ink: [u32; 4]) {
    assert!(
        (40..=215).contains(&ink[0]) && (40..=215).contains(&ink[2]) && ink[1] < 40,
        "{label}: ink {ink:?}, expected a red-to-blue mix"
    );
}

fn inherited(layout: Layout, color: Color) -> RsxNode {
    rsx! {
        <Element style={{
            width: Length::px(300.0),
            height: Length::px(120.0),
            layout: layout,
            color: color,
            transition: fade(TransitionProperty::Color),
        }}>
            <Text style={{ font_size: 80.0 }}>{"MMMM"}</Text>
        </Element>
    }
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn text_inherits_an_element_color_transition() -> Result<(), String> {
    for (name, layout) in [
        ("flow", Layout::flow().column().into()),
        ("inline", Layout::Inline),
    ] {
        let at = |color| inherited(layout, color);
        let inks = run(&[
            (0, at(RED)),
            (16, at(RED)),
            (100, at(BLUE)),
            (600, at(BLUE)),
            (1_200, at(BLUE)),
        ])?;
        assert_color(&format!("{name} before"), inks[1], [255, 0, 0]);
        assert_near_start(&format!("{name} change frame"), inks[2]);
        assert_between(&format!("{name} halfway"), inks[3]);
        assert_color(&format!("{name} after"), inks[4], [0, 0, 255]);
    }
    Ok(())
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn text_inherits_an_element_color_animation() -> Result<(), String> {
    let animator = Animator::new([Animation::new([
        Keyframe::new(0.0, rfgui::style! { color: RED }),
        Keyframe::new(1.0, rfgui::style! { color: BLUE }),
    ])])
    .duration(1000)
    .linear();
    let scene = rsx! {
        <Element style={{
            width: Length::px(300.0),
            height: Length::px(120.0),
            layout: Layout::flow().column(),
            animator: animator,
        }}>
            <Text style={{ font_size: 80.0 }}>{"MMMM"}</Text>
        </Element>
    };
    let inks = run(&[
        (0, scene.clone()),
        (500, scene.clone()),
        (1_100, scene.clone()),
        (1_116, scene),
    ])?;
    assert_color("start", inks[0], [255, 0, 0]);
    assert_between("halfway", inks[1]);
    // Without a forwards fill the Text gets its own color back: #111111.
    assert_color("after", inks[3], [1, 1, 1]);
    Ok(())
}

fn own(layout: Layout, color: Color, opacity: f32, property: TransitionProperty) -> RsxNode {
    rsx! {
        <Element style={{
            width: Length::px(300.0),
            height: Length::px(120.0),
            layout: layout,
        }}>
            <Text style={{
                font_size: 80.0,
                color: color,
                opacity: Opacity::new(opacity),
                transition: fade(property),
            }}>{"MMMM"}</Text>
        </Element>
    }
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn text_runs_its_own_style_transitions() -> Result<(), String> {
    for (name, layout) in [
        ("flow", Layout::flow().column().into()),
        ("inline", Layout::Inline),
    ] {
        let color = |c| own(layout, c, 1.0, TransitionProperty::Color);
        let inks = run(&[
            (0, color(RED)),
            (16, color(RED)),
            (100, color(BLUE)),
            (600, color(BLUE)),
            (1_200, color(BLUE)),
        ])?;
        assert_near_start(&format!("{name} color change frame"), inks[2]);
        assert_between(&format!("{name} color halfway"), inks[3]);
        assert_color(&format!("{name} color after"), inks[4], [0, 0, 255]);

        let opacity = |o| own(layout, BLUE, o, TransitionProperty::Opacity);
        let inks = run(&[
            (0, opacity(1.0)),
            (16, opacity(1.0)),
            (100, opacity(0.0)),
            (600, opacity(0.0)),
            (1_200, opacity(0.0)),
        ])?;
        assert!(
            inks[2][3] > 200,
            "{name} opacity change frame: {:?}",
            inks[2]
        );
        assert!(
            (60..=200).contains(&inks[3][3]),
            "{name} opacity halfway: {:?}",
            inks[3]
        );
        assert_eq!(inks[4][3], 0, "{name} opacity after: {:?}", inks[4]);
    }
    Ok(())
}
