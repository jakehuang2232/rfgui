//! A transformed box is clipped by its ancestors where it is drawn, not
//! where it sat before the transform, and by its own clip in its own space.
use rfgui::style::{
    Anchor, Angle, ClipMode, Color, Layout, Length, Opacity, Padding, Position, Rotate,
    ScrollDirection, Transform,
};
use rfgui::time::{Duration, Instant};
use rfgui::ui::{RsxNode, rsx};
use rfgui::view::{Element, Viewport};

#[path = "../../lib/rfgui-components/tests/retained_controls/gpu.rs"]
mod gpu;

const SIZE: [u32; 2] = [300, 200];

/// A 160x60 box rotated 10° about its center. Opacity below 1 draws it into
/// an effect surface inside its transform surface.
fn rotated_box(opacity: f32, anchor_clip: bool) -> RsxNode {
    if anchor_clip {
        return rsx! {
            <Element style={{
                position: Position::absolute()
                    .left(Length::px(40.0))
                    .top(Length::px(70.0))
                    .anchor(Anchor::Parent)
                    .clip(ClipMode::AnchorParent),
                width: Length::px(160.0),
                height: Length::px(60.0),
                background: Color::hex("#3080ff"),
                opacity: Opacity::new(opacity),
                transform: Transform::new([Rotate::z(Angle::deg(10.0))]),
            }} />
        };
    }
    rsx! {
        <Element style={{
            width: Length::px(160.0),
            height: Length::px(60.0),
            background: Color::hex("#3080ff"),
            opacity: Opacity::new(opacity),
            transform: Transform::new([Rotate::z(Angle::deg(10.0))]),
        }} />
    }
}

/// A box `height` tall that clips the rotated box through an AnchorParent
/// clip: on the box itself, or on an absolute element around it.
fn anchor_clipped(height: f32, opacity: f32, on_box: bool) -> RsxNode {
    let clipped = if on_box {
        rotated_box(opacity, true)
    } else {
        rsx! {
            <Element style={{
                position: Position::absolute()
                    .left(Length::px(0.0))
                    .top(Length::px(0.0))
                    .anchor(Anchor::Parent)
                    .clip(ClipMode::AnchorParent),
                width: Length::px(260.0),
                height: Length::px(170.0),
                layout: Layout::flow().column().no_wrap(),
                padding: Padding::new().top(Length::px(70.0)).left(Length::px(40.0)),
            }}>
                {rotated_box(opacity, false)}
            </Element>
        }
    };
    rsx! {
        <Element style={{ width: Length::px(260.0), height: Length::px(height) }}>
            <Element style={{ width: Length::px(260.0), height: Length::px(height) }}>
                {clipped}
            </Element>
        </Element>
    }
}

#[derive(Clone, Copy, Debug)]
enum ClipSource {
    /// A scrolled scroll box with a scrollbar.
    Scroll,
    /// A scroll box with nothing to scroll vertically, whose surface folds.
    FoldedScroll,
    AnchorAncestor,
    AnchorOnBox,
    /// A scrolled scroll box with a scrollbar, around an AnchorParent clip.
    ScrollAroundAnchor,
    /// A scroll box that folds, around an AnchorParent clip.
    FoldedScrollAroundAnchor,
}

/// The scene, its clip rectangle on screen, and whether it scrolls by 30px.
/// `reach` moves the clip's edge past the rotated box without moving it.
fn scene(source: ClipSource, opacity: f32, reach: bool) -> (RsxNode, [u32; 4], bool) {
    let content = match source {
        ClipSource::Scroll => rsx! {
            <Element style={{
                width: Length::px(240.0),
                height: Length::px(if reach { 160.0 } else { 100.0 }),
                scroll_direction: ScrollDirection::Vertical,
                layout: Layout::flow().column().no_wrap(),
            }}>
                <Element style={{
                    width: Length::px(240.0),
                    height: Length::px(400.0),
                    padding: Padding::new().top(Length::px(110.0)).left(Length::px(40.0)),
                    layout: Layout::flow().column().no_wrap(),
                }}>
                    {rotated_box(opacity, false)}
                </Element>
            </Element>
        },
        ClipSource::FoldedScroll => rsx! {
            <Element style={{
                width: Length::px(if reach { 280.0 } else { 140.0 }),
                height: Length::px(150.0),
                scroll_direction: ScrollDirection::Vertical,
                layout: Layout::flow().column().no_wrap(),
            }}>
                <Element style={{
                    width: Length::px(200.0),
                    height: Length::px(100.0),
                    padding: Padding::new().top(Length::px(40.0)).left(Length::px(40.0)),
                    layout: Layout::flow().column().no_wrap(),
                }}>
                    {rotated_box(opacity, false)}
                </Element>
            </Element>
        },
        ClipSource::AnchorAncestor | ClipSource::AnchorOnBox => anchor_clipped(
            if reach { 170.0 } else { 100.0 },
            opacity,
            matches!(source, ClipSource::AnchorOnBox),
        ),
        ClipSource::ScrollAroundAnchor => rsx! {
            <Element style={{
                width: Length::px(260.0),
                height: Length::px(160.0),
                scroll_direction: ScrollDirection::Vertical,
                layout: Layout::flow().column().no_wrap(),
            }}>
                <Element style={{
                    width: Length::px(260.0),
                    height: Length::px(400.0),
                    padding: Padding::new().top(Length::px(30.0)),
                    layout: Layout::flow().column().no_wrap(),
                }}>
                    {anchor_clipped(if reach { 170.0 } else { 100.0 }, opacity, false)}
                </Element>
            </Element>
        },
        ClipSource::FoldedScrollAroundAnchor => rsx! {
            <Element style={{
                width: Length::px(260.0),
                height: Length::px(175.0),
                scroll_direction: ScrollDirection::Vertical,
                layout: Layout::flow().column().no_wrap(),
            }}>
                {anchor_clipped(if reach { 170.0 } else { 100.0 }, opacity, false)}
            </Element>
        },
    };
    let clip = match source {
        ClipSource::Scroll => [20, 20, 240, 100],
        ClipSource::FoldedScroll => [20, 20, 140, 150],
        _ => [20, 20, 260, 100],
    };
    let root = rsx! {
        <Element style={{
            width: Length::px(300.0),
            height: Length::px(200.0),
            background: Color::hex("#000000"),
            padding: Padding::new().top(Length::px(20.0)).left(Length::px(20.0)),
            layout: Layout::flow().column().no_wrap(),
        }}>
            {content}
        </Element>
    };
    let scrolls = matches!(source, ClipSource::Scroll | ClipSource::ScrollAroundAnchor);
    (root, clip, scrolls)
}

fn render(gpu: &gpu::Gpu, scene: &RsxNode, scroll: bool) -> Result<Vec<u8>, String> {
    let mut viewport = Viewport::new();
    let root = rfgui::ui::render_root(|| scene.clone());
    let start = Instant::now();
    let mut last = None;
    for frame in 0..6u64 {
        if frame == 2 && scroll {
            viewport.set_pointer_position_viewport(60.0, 60.0);
            viewport.dispatch_pointer_move_event();
            viewport.dispatch_pointer_wheel_event(0.0, 30.0);
        }
        last = Some(viewport.render_rsx_offscreen_for_test(
            &root,
            gpu.device.clone(),
            gpu.queue.clone(),
            SIZE,
            1.0,
            start + Duration::from_millis(frame * 400),
        )?);
    }
    let frame = last.ok_or("no frame")?;
    gpu.read(&frame.texture, SIZE)
}

fn is_blue(pixels: &[u8], x: u32, y: u32) -> bool {
    let at = ((y * SIZE[0] + x) * 4) as usize;
    pixels[at + 2] > 150 && pixels[at] < 100
}

/// Every clip edge is straight on screen: inside the clip rectangle the box
/// draws exactly as with the clip out of reach, and nothing draws outside.
#[test]
#[ignore = "requires native hardware graphics adapter"]
fn ancestor_clips_cut_a_transformed_box_on_screen() -> Result<(), String> {
    let gpu = gpu::Gpu::new()?;
    let mut failures = Vec::new();
    for source in [
        ClipSource::Scroll,
        ClipSource::FoldedScroll,
        ClipSource::AnchorAncestor,
        ClipSource::AnchorOnBox,
        ClipSource::ScrollAroundAnchor,
        ClipSource::FoldedScrollAroundAnchor,
    ] {
        for opacity in [1.0, 0.8] {
            let (clipped, [x0, y0, width, height], scroll) = scene(source, opacity, false);
            let (unclipped, _, _) = scene(source, opacity, true);
            let actual = render(&gpu, &clipped, scroll)?;
            let expected = render(&gpu, &unclipped, scroll)?;
            let (mut inside, mut missing, mut leaked) = (0, 0, 0);
            for y in 0..SIZE[1] {
                for x in 0..SIZE[0] {
                    let in_clip = (x0..x0 + width).contains(&x) && (y0..y0 + height).contains(&y);
                    let (drawn, wanted) = (is_blue(&actual, x, y), is_blue(&expected, x, y));
                    if in_clip {
                        inside += wanted as u32;
                        missing += (wanted && !drawn) as u32;
                    } else {
                        leaked += drawn as u32;
                    }
                }
            }
            if inside == 0 || missing > 0 || leaked > 0 {
                failures.push(format!(
                    "{source:?} opacity {opacity}: {missing} of {inside} pixels missing, {leaked} leaked"
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
    Ok(())
}

/// A transformed scroll box clips its own content along its turned edges.
#[test]
#[ignore = "requires native hardware graphics adapter"]
fn a_transformed_box_clips_its_content_in_its_own_space() -> Result<(), String> {
    let gpu = gpu::Gpu::new()?;
    for opacity in [1.0, 0.8] {
        let scene = rsx! {
            <Element style={{
                width: Length::px(300.0),
                height: Length::px(200.0),
                background: Color::hex("#000000"),
                padding: Padding::new().top(Length::px(70.0)).left(Length::px(60.0)),
                layout: Layout::flow().column().no_wrap(),
            }}>
                <Element style={{
                    width: Length::px(160.0),
                    height: Length::px(60.0),
                    scroll_direction: ScrollDirection::Vertical,
                    opacity: Opacity::new(opacity),
                    transform: Transform::new([Rotate::z(Angle::deg(20.0))]),
                    layout: Layout::flow().column().no_wrap(),
                }}>
                    <Element style={{
                        width: Length::px(300.0),
                        height: Length::px(60.0),
                        background: Color::hex("#3080ff"),
                    }} />
                </Element>
            </Element>
        };
        let pixels = render(&gpu, &scene, false)?;
        // Inside the turned box, below where its edge sat before turning.
        assert!(
            is_blue(&pixels, 197, 147),
            "opacity {opacity}: content cut short"
        );
        // Past the turned box's right edge, where the content overflows.
        assert!(
            !is_blue(&pixels, 234, 134),
            "opacity {opacity}: content not clipped"
        );
    }
    Ok(())
}
