//! Content scrolled by an offset behaves as the same content moved up by that
//! offset in a fresh viewport: it paints the same pixels, hits the same
//! pointer targets with the same event coordinates, and puts the IME cursor
//! in the same place. The offsets change in place across frames, and the
//! content above the fold changes between them, so geometry an earlier frame
//! left behind shows up as a difference.
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use rfgui::platform::ImeCommand;
use rfgui::style::{
    Anchor, Angle, ClipMode, Collision, CollisionBoundary, Color, Layout, Length, Position, Rotate,
    ScrollDirection, Transform,
};
use rfgui::time::Instant;
use rfgui::ui::{
    PointerDownHandlerProp, Rect, RsxNode, ScrollIntoViewOptions, on_pointer_down, rsx,
};
use rfgui::view::node_arena::{NodeArena, NodeKey};
use rfgui::view::viewport::{PointerButton, set_scroll_offset_by_id};
use rfgui::view::{Element, Text, TextArea, Viewport};

#[path = "../../lib/rfgui-components/tests/retained_controls/gpu.rs"]
mod gpu;

/// A pointer-down that targeted a labeled node.
#[derive(Clone, Debug)]
struct Hit {
    label: String,
    local: [f32; 2],
    bounds: Rect,
    parent: Option<Rect>,
}

type Log = Rc<RefCell<Vec<Hit>>>;

/// Logs each pointer-down that targets the node it is attached to.
fn logged(label: impl Into<String>, log: &Log) -> PointerDownHandlerProp {
    let label = label.into();
    let log = log.clone();
    on_pointer_down(move |event| {
        if event.meta.target_id() != event.meta.current_target_id() {
            return;
        }
        let target = event.meta.current_target();
        log.borrow_mut().push(Hit {
            label: label.clone(),
            local: [event.pointer.local_x, event.pointer.local_y],
            bounds: target.bounds,
            parent: target.parent().map(|parent| parent.bounds),
        });
    })
}

/// The content of a scene in one variant. Content in an inner scroll
/// container comes moved up by `inner`, which the scrolled scene leaves at
/// zero and scrolls instead.
type Scene = fn(variant: usize, inner: f32, log: &Log) -> Vec<RsxNode>;

/// One frame: the outer and inner scroll offsets and the content variant.
#[derive(Clone, Copy)]
struct Step {
    outer: f32,
    inner: f32,
    variant: usize,
}

const fn step(outer: f32, inner: f32, variant: usize) -> Step {
    Step {
        outer,
        inner,
        variant,
    }
}

fn scrolled(content: Vec<RsxNode>) -> RsxNode {
    rsx! {
        <Element style={{width:Length::px(400.), height:Length::px(400.), layout:Layout::flow().column(), scroll_direction:ScrollDirection::Vertical}}>
            <Element style={{width:Length::px(400.), layout:Layout::flow().column()}}>
                {content}
            </Element>
        </Element>
    }
}

/// What the scroll container must show once scrolled by `offset`.
fn moved_up(content: Vec<RsxNode>, offset: f32) -> RsxNode {
    rsx! {
        <Element style={{width:Length::px(400.), height:Length::px(400.)}}>
            <Element style={{
                position: Position::absolute().left(Length::px(0.)).top(Length::px(-offset)),
                width: Length::px(400.),
            }}>
                <Element style={{width:Length::px(400.), layout:Layout::flow().column()}}>
                    {content}
                </Element>
            </Element>
        </Element>
    }
}

fn spacer(height: f32) -> RsxNode {
    rsx! { <Element style={{width:Length::px(400.), height:Length::px(height)}} /> }
}

/// Rows of text, one of them above the fold growing and shrinking, and a
/// rotated box with a child.
fn rows(variant: usize, _inner: f32, log: &Log) -> Vec<RsxNode> {
    let mut content = (0..40)
        .map(|i| {
            let height = if i == 1 {
                [24., 48., 24., 36.][variant % 4]
            } else {
                24.
            };
            rsx! {
                <Element
                    style={{width:Length::px(360.), height:Length::px(height), background:Color::rgb(40+(i%4) as u8*30,60,80)}}
                    on_pointer_down={logged(format!("row {i}"), log)}
                >
                    <Text>{format!("Row {i}")}</Text>
                </Element>
            }
        })
        .collect::<Vec<_>>();
    content.insert(
        9,
        rsx! {
            <Element
                style={{width:Length::px(120.), height:Length::px(40.), background:Color::rgb(200,120,40), transform:Transform::new([Rotate::z(Angle::deg(12.))])}}
                on_pointer_down={logged("turned", log)}
            >
                <Element
                    style={{width:Length::px(40.), height:Length::px(20.), background:Color::rgb(40,200,120)}}
                    on_pointer_down={logged("turned child", log)}
                />
            </Element>
        },
    );
    content
}

/// An inner scroll container below the fold of the outer one.
fn nested(variant: usize, inner: f32, log: &Log) -> Vec<RsxNode> {
    let inner_rows = rsx! {
        <Element style={{width:Length::px(180.), layout:Layout::flow().column()}}>
            {(0..20).map(|i| rsx! {
                <Element
                    style={{width:Length::px(180.), height:Length::px(24.), background:Color::rgb(80+(i%5) as u8*30,200,90)}}
                    on_pointer_down={logged(format!("inner {i}"), log)}
                >
                    <Text>{format!("Inner {i}")}</Text>
                </Element>
            }).collect::<Vec<_>>()}
        </Element>
    };
    let inner_content = if inner == 0. {
        inner_rows
    } else {
        rsx! {
            <Element style={{position: Position::absolute().left(Length::px(0.)).top(Length::px(-inner)), width:Length::px(180.)}}>
                {inner_rows}
            </Element>
        }
    };
    vec![
        spacer([300., 260.][variant % 2]),
        rsx! {
            <Element
                style={{width:Length::px(200.), height:Length::px(150.), layout:Layout::flow().column(), scroll_direction:ScrollDirection::Vertical, background:Color::rgb(60,60,60)}}
                on_pointer_down={logged("inner scroller", log)}
            >
                {inner_content}
            </Element>
        },
        spacer(600.),
    ]
}

/// A menu anchored below a trigger that flips above it near the bottom of
/// the viewport, a box clipped to its anchor's parent, and a box pinned to
/// the viewport.
fn popups(variant: usize, _inner: f32, log: &Log) -> Vec<RsxNode> {
    vec![
        spacer([330., 300.][variant % 2]),
        rsx! {
            <Element style={{width:Length::px(400.), height:Length::px(24.)}}>
                <Element
                    anchor="menu_trigger"
                    style={{width:Length::px(160.), height:Length::px(24.), background:Color::rgb(90,90,90)}}
                    on_pointer_down={logged("trigger", log)}
                />
                <Element
                    style={{
                        position: Position::absolute()
                            .anchor("menu_trigger")
                            .top(Length::percent(100.))
                            .left(Length::px(0.))
                            .collision(Collision::FlipFit, CollisionBoundary::Viewport)
                            .clip(ClipMode::Viewport),
                        width: Length::px(160.),
                        height: Length::px(120.),
                        layout: Layout::flow().column(),
                        background: Color::rgb(30,160,60),
                    }}
                    on_pointer_down={logged("menu", log)}
                >
                    <Element
                        style={{width:Length::px(160.), height:Length::px(24.), background:Color::rgb(30,60,160)}}
                        on_pointer_down={logged("menu item", log)}
                    />
                </Element>
            </Element>
        },
        rsx! {
            <Element style={{width:Length::px(140.), height:Length::px(110.), layout:Layout::flow().column(), background:Color::rgb(70,70,70)}}>
                <Element anchor="clip_anchor" style={{width:Length::px(56.), height:Length::px(26.), background:Color::rgb(30,80,220)}} />
                <Element
                    style={{
                        position: Position::absolute()
                            .anchor("clip_anchor")
                            .top(Length::px(0.))
                            .left(Length::px(38.))
                            .clip(ClipMode::AnchorParent),
                        width: Length::px(150.),
                        height: Length::px(22.),
                        background: Color::rgb(40,200,90),
                    }}
                    on_pointer_down={logged("anchor clipped", log)}
                />
                <Element
                    style={{
                        position: Position::absolute()
                            .anchor(Anchor::Viewport)
                            .bottom(Length::px(10.))
                            .left(Length::px(220.))
                            .clip(ClipMode::Viewport),
                        width: Length::px(120.),
                        height: Length::px(30.),
                        background: Color::rgb(240,160,20),
                    }}
                    on_pointer_down={logged("pinned", log)}
                />
            </Element>
        },
        spacer(800.),
    ]
}

/// A multi-line text area below a block that grows and shrinks.
fn text_area(variant: usize, _inner: f32, log: &Log) -> Vec<RsxNode> {
    vec![
        spacer([200., 230.][variant % 2]),
        rsx! {
            <Element
                style={{width:Length::px(300.), height:Length::px(80.), background:Color::rgb(30,30,30)}}
                on_pointer_down={logged("text area box", log)}
            >
                <TextArea
                    content="First line of the area\nSecond line\nThird line, the last"
                    style={{ font_size: 16 }}
                />
            </Element>
        },
        spacer(600.),
    ]
}

/// Viewport points the pointer presses, over the whole viewport.
fn probe_points() -> Vec<[f32; 2]> {
    let mut points = Vec::new();
    for y in [
        12., 41., 95., 133., 175., 214., 255., 296., 335., 375., 391.,
    ] {
        for x in [9., 30., 77., 150., 250., 330.] {
            points.push([x, y]);
        }
    }
    points
}

/// What one press reported: the labeled targets it hit and the last IME
/// cursor rect it set.
#[derive(Debug, Default)]
struct Press {
    hits: Vec<Hit>,
    ime_cursor: Option<[f32; 4]>,
}

fn press_all(viewport: &mut Viewport, log: &Log) -> Vec<Press> {
    let _ = viewport.drain_platform_requests();
    probe_points()
        .into_iter()
        .map(|[x, y]| {
            log.borrow_mut().clear();
            viewport.set_pointer_position_viewport(x, y);
            viewport.dispatch_pointer_move_event();
            viewport.set_pointer_button_pressed(PointerButton::Left, true);
            viewport.dispatch_pointer_down_event(PointerButton::Left);
            viewport.set_pointer_button_pressed(PointerButton::Left, false);
            viewport.dispatch_pointer_up_event(PointerButton::Left);
            let ime_cursor = viewport
                .drain_platform_requests()
                .ime_commands
                .into_iter()
                .rev()
                .find_map(|command| match command {
                    ImeCommand::SetCursorRect(x, y, w, h) => Some([x, y, w, h]),
                    _ => None,
                });
            Press {
                hits: log.borrow().clone(),
                ime_cursor,
            }
        })
        .collect()
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() <= 1e-3
}

fn rects_close(a: Rect, b: Rect) -> bool {
    close(a.x, b.x) && close(a.y, b.y) && close(a.width, b.width) && close(a.height, b.height)
}

fn assert_presses_match(scrolled: &[Press], fresh: &[Press], context: &str) {
    for ((point, scrolled), fresh) in probe_points().iter().zip(scrolled).zip(fresh) {
        let same_hits = scrolled.hits.len() == fresh.hits.len()
            && scrolled.hits.iter().zip(&fresh.hits).all(|(a, b)| {
                a.label == b.label
                    && close(a.local[0], b.local[0])
                    && close(a.local[1], b.local[1])
                    && rects_close(a.bounds, b.bounds)
                    && match (a.parent, b.parent) {
                        (Some(a), Some(b)) => rects_close(a, b),
                        (a, b) => a.is_none() && b.is_none(),
                    }
            });
        let same_ime = match (scrolled.ime_cursor, fresh.ime_cursor) {
            (Some(a), Some(b)) => a.iter().zip(b).all(|(a, b)| close(*a, b)),
            (a, b) => a.is_none() && b.is_none(),
        };
        assert!(
            same_hits && same_ime,
            "{context}, press at {point:?}:\nscrolled {scrolled:?}\nfresh {fresh:?}"
        );
    }
}

/// Stable ids of the scroll containers under `key`, outer first.
fn scroll_containers(arena: &NodeArena, key: NodeKey, out: &mut Vec<u64>) {
    let Some(node) = arena.get(key) else {
        return;
    };
    let element = &node.element;
    if element.can_scroll_by(0., 1.) || element.can_scroll_by(0., -1.) {
        out.push(element.stable_id());
    }
    drop(node);
    for child in arena.children_of(key) {
        scroll_containers(arena, child, out);
    }
}

fn scroll_to(viewport: &Viewport, step: Step) {
    let arena = viewport.node_arena();
    let root = arena.roots()[0];
    let mut ids = Vec::new();
    scroll_containers(arena, root, &mut ids);
    for (id, offset) in ids.into_iter().zip([step.outer, step.inner]) {
        set_scroll_offset_by_id(arena, root, id, (0., offset));
    }
}

fn render(
    gpu: &gpu::Gpu,
    viewport: &mut Viewport,
    tree: &RsxNode,
    dpr: u32,
    now: Instant,
) -> Result<Vec<u8>, String> {
    let size = [400 * dpr, 400 * dpr];
    let output = viewport.render_rsx_offscreen_for_test(
        tree,
        gpu.device.clone(),
        gpu.queue.clone(),
        size,
        dpr as f32,
        now,
    )?;
    gpu.read(&output.texture, size)
}

/// Steps one viewport through `steps`, scrolling in place, and requires each
/// frame to paint what a fresh viewport paints for the content moved up, and
/// presses at every probe point to report the same.
fn assert_scrolled_in_place(scene: Scene, steps: &[Step]) -> Result<(), String> {
    assert!(steps[0].outer == 0. && steps[0].inner == 0.);
    let gpu = gpu::Gpu::new()?;
    let now = Instant::now();
    let log: Log = Rc::default();
    // Pixels, without any input: a press hovers and focuses.
    for dpr in [1, 2] {
        let mut viewport = Viewport::new();
        for (frame, &step) in steps.iter().enumerate() {
            if frame > 0 {
                scroll_to(&viewport, step);
            }
            let pixels = render(
                &gpu,
                &mut viewport,
                &scrolled(scene(step.variant, 0., &log)),
                dpr,
                now,
            )?;
            let expected = render(
                &gpu,
                &mut Viewport::new(),
                &moved_up(scene(step.variant, step.inner, &log), step.outer),
                dpr,
                now,
            )?;
            let differing = pixels
                .chunks_exact(4)
                .zip(expected.chunks_exact(4))
                .filter(|(a, b)| a != b)
                .count();
            assert_eq!(
                differing, 0,
                "pixels differ at DPR {dpr}, outer {}, inner {}, variant {}",
                step.outer, step.inner, step.variant
            );
        }
    }
    // Presses, in a viewport that keeps the focus and hover they leave.
    let mut viewport = Viewport::new();
    for (frame, &step) in steps.iter().enumerate() {
        if frame > 0 {
            scroll_to(&viewport, step);
        }
        render(
            &gpu,
            &mut viewport,
            &scrolled(scene(step.variant, 0., &log)),
            1,
            now,
        )?;
        let scrolled_presses = press_all(&mut viewport, &log);
        let mut fresh = Viewport::new();
        render(
            &gpu,
            &mut fresh,
            &moved_up(scene(step.variant, step.inner, &log), step.outer),
            1,
            now,
        )?;
        let fresh_presses = press_all(&mut fresh, &log);
        assert_presses_match(
            &scrolled_presses,
            &fresh_presses,
            &format!(
                "outer {}, inner {}, variant {}",
                step.outer, step.inner, step.variant
            ),
        );
    }
    Ok(())
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn rows_scrolled_in_place() -> Result<(), String> {
    assert_scrolled_in_place(
        rows,
        &[
            step(0., 0., 0),
            step(120., 0., 0),
            step(120., 0., 1),
            step(437., 0., 1),
            step(437., 0., 2),
            step(60., 0., 3),
        ],
    )
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn nested_scroll_containers_scrolled_in_place() -> Result<(), String> {
    assert_scrolled_in_place(
        nested,
        &[
            step(0., 0., 0),
            step(200., 0., 0),
            step(200., 90., 0),
            step(250., 90., 1),
            step(250., 330., 1),
            step(40., 13., 0),
        ],
    )
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn anchored_and_pinned_boxes_scrolled_in_place() -> Result<(), String> {
    assert_scrolled_in_place(
        popups,
        &[
            step(0., 0., 0),
            step(100., 0., 0),
            step(100., 0., 1),
            step(300., 0., 1),
            step(40., 0., 0),
        ],
    )
}

#[test]
#[ignore = "requires native hardware graphics adapter"]
fn text_area_scrolled_in_place() -> Result<(), String> {
    assert_scrolled_in_place(
        text_area,
        &[
            step(0., 0., 0),
            step(120., 0., 0),
            step(120., 0., 1),
            step(180., 0., 1),
            step(30., 0., 0),
        ],
    )
}

/// Wheel scrolling reaches the same offsets through the scroll container's
/// own input path, animated over frames. The scrollbar it reveals keeps
/// pixels out of the comparison.
#[test]
#[ignore = "requires native hardware graphics adapter"]
fn rows_wheeled_in_place() -> Result<(), String> {
    let gpu = gpu::Gpu::new()?;
    let start = Instant::now();
    let mut frame = 0_u64;
    let mut now = || {
        frame += 1;
        start + std::time::Duration::from_millis(frame * 16)
    };
    let log: Log = Rc::default();
    let mut viewport = Viewport::new();
    render(&gpu, &mut viewport, &scrolled(rows(0, 0., &log)), 1, now())?;
    let offset_of = |viewport: &Viewport| {
        let arena = viewport.node_arena();
        arena
            .get(arena.roots()[0])
            .unwrap()
            .element
            .get_scroll_offset()
            .1
    };
    let mut offsets = Vec::new();
    for (variant, dy) in [(0, 90.), (1, 140.), (1, -60.), (2, 300.)] {
        viewport.set_pointer_position_viewport(200., 200.);
        viewport.dispatch_pointer_wheel_event(0., dy);
        // Frames until the scroll animation settles.
        let mut offset = f32::NAN;
        for _ in 0..60 {
            render(
                &gpu,
                &mut viewport,
                &scrolled(rows(variant, 0., &log)),
                1,
                now(),
            )?;
            let next = offset_of(&viewport);
            if next == offset {
                break;
            }
            offset = next;
        }
        offsets.push(offset);
        let at = now();
        render(
            &gpu,
            &mut viewport,
            &scrolled(rows(variant, 0., &log)),
            1,
            at,
        )?;
        let scrolled_presses = press_all(&mut viewport, &log);
        let mut fresh = Viewport::new();
        render(
            &gpu,
            &mut fresh,
            &moved_up(rows(variant, 0., &log), offset),
            1,
            at,
        )?;
        let fresh_presses = press_all(&mut fresh, &log);
        assert_presses_match(
            &scrolled_presses,
            &fresh_presses,
            &format!("wheeled to {offset}, variant {variant}"),
        );
    }
    // Every wheel moved the list.
    assert!(
        offsets.windows(2).all(|pair| pair[0] != pair[1]) && offsets[0] > 0.,
        "offsets {offsets:?}"
    );
    Ok(())
}

/// Bringing a node into view scrolls the nearest scroll container that can
/// reveal it, by as little as it takes, and no container above that one.
/// Presses then hit the revealed node where that leaves it.
#[test]
#[ignore = "requires native hardware graphics adapter"]
fn scroll_into_view_scrolls_the_nearest_container() -> Result<(), String> {
    let gpu = gpu::Gpu::new()?;
    let now = Instant::now();
    let log: Log = Rc::default();
    let target: Rc<Cell<Option<NodeKey>>> = Rc::default();
    let scene = |log: &Log| {
        let target = target.clone();
        let reveal = on_pointer_down(move |event| {
            if let Some(target) = target.get() {
                event
                    .viewport
                    .scroll_into_view(target, ScrollIntoViewOptions::default());
            }
        });
        let mut content = nested(0, 0., log);
        content.push(rsx! {
            <Element
                style={{
                    position: Position::absolute()
                        .anchor(Anchor::Viewport)
                        .top(Length::px(0.))
                        .left(Length::px(300.))
                        .clip(ClipMode::Viewport),
                    width: Length::px(100.),
                    height: Length::px(30.),
                    background: Color::rgb(120,40,40),
                }}
                on_pointer_down={reveal}
            />
        });
        scrolled(content)
    };
    let press = |viewport: &mut Viewport, x: f32, y: f32| {
        log.borrow_mut().clear();
        viewport.set_pointer_position_viewport(x, y);
        viewport.dispatch_pointer_move_event();
        viewport.set_pointer_button_pressed(PointerButton::Left, true);
        viewport.dispatch_pointer_down_event(PointerButton::Left);
        viewport.set_pointer_button_pressed(PointerButton::Left, false);
        viewport.dispatch_pointer_up_event(PointerButton::Left);
        log.borrow().clone()
    };
    let mut viewport = Viewport::new();
    render(&gpu, &mut viewport, &scene(&log), 1, now)?;
    // Outer scroll container > content > [spacer, inner scroll container >
    // rows > row 0..]
    let arena = viewport.node_arena();
    let content = arena.children_of(arena.roots()[0])[0];
    let inner = arena.children_of(content)[1];
    let row = arena.children_of(arena.children_of(inner)[0])[10];
    // The inner container, 300px down and 150px tall, ends 50px below the
    // viewport: the outer container scrolls 50px.
    target.set(Some(inner));
    press(&mut viewport, 350., 10.);
    render(&gpu, &mut viewport, &scene(&log), 1, now)?;
    let hits = press(&mut viewport, 190., 395.);
    assert!(
        matches!(hits.as_slice(), [hit] if hit.label == "inner scroller" && close(hit.bounds.y, 250.)),
        "{hits:?}"
    );
    // Row 10 ends 264px into the inner container: only the inner container
    // scrolls, by 114px, which puts the row at its bottom edge.
    target.set(Some(row));
    press(&mut viewport, 350., 10.);
    render(&gpu, &mut viewport, &scene(&log), 1, now)?;
    let hits = press(&mut viewport, 150., 390.);
    assert!(
        matches!(hits.as_slice(), [hit] if hit.label == "inner 10" && close(hit.bounds.y, 376.)),
        "{hits:?}"
    );
    Ok(())
}
