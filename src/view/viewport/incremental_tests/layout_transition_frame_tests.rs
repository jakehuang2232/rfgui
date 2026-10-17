//! Layout transitions frame by frame: a position sample is placed in the
//! frame that takes it, and a size transition settles on a fractional size.

use super::super::Viewport;
use crate::style::{Layout, Length, Position, ScrollDirection, Transition, TransitionProperty};
use crate::ui::{RsxNode, rsx};
use crate::view::Element as HostElement;
use crate::view::compositor::property_tree::ScrollNodeId;

/// A scrolling box around tall content, whose absolute child moves to
/// `left` with a transition, as a slider thumb does.
fn scene(left: f32) -> RsxNode {
    rsx! {
        <HostElement style={{
            width: Length::px(300.0),
            height: Length::px(200.0),
            layout: Layout::flow().column().no_wrap(),
            scroll_direction: ScrollDirection::Vertical,
        }}>
            <HostElement style={{ width: Length::px(300.0), height: Length::px(400.0) }}>
                <HostElement style={{
                    position: Position::absolute().left(Length::px(left)).top(Length::Zero),
                    width: Length::px(20.0),
                    height: Length::px(20.0),
                    transition: [Transition::new(TransitionProperty::Position, 1000).linear()],
                }} />
            </HostElement>
        </HostElement>
    }
}

/// One frame in the production order: layout, transition samples, the
/// relayout they ask for, then the property sync. Returns whether a
/// transition still runs.
fn frame(viewport: &mut Viewport, scene: &RsxNode, dt: f32, now: f64) -> bool {
    viewport.render_rsx(scene).expect("render");
    viewport.run_layout_pass();
    let transitions = viewport.run_post_layout_transitions(dt, now);
    if transitions.relayout_required {
        viewport.run_layout_pass();
    }
    viewport.sync_compositor_property_trees();
    transitions.redraw_changed
}

/// The frame's property trees see the element where it is drawn, so a
/// scroll container around it keeps its scroll geometry.
#[test]
fn a_position_transition_sample_is_placed_before_the_property_sync() {
    let mut viewport = Viewport::new();
    viewport.set_size(300, 200);
    viewport.set_use_incremental_commit(true);
    frame(&mut viewport, &scene(0.0), 0.0, 0.0);
    frame(&mut viewport, &scene(100.0), 0.5, 0.5);

    let scroller = viewport.scene.ui_root_keys[0];
    let arena = &viewport.scene.node_arena;
    let thumb = arena.children_of(arena.children_of(scroller)[0])[0];
    let x = arena
        .get(thumb)
        .expect("thumb")
        .element
        .box_model_snapshot()
        .x;
    assert!((x - 50.0).abs() < 0.01, "halfway from 0 to 100: {x}");
    let trees = &viewport.compositor.property_trees;
    assert!(
        trees.validation_errors.is_empty(),
        "{:?}",
        trees.validation_errors
    );
    assert!(trees.scroll_snapshot_for(ScrollNodeId(scroller)).is_some());
}

/// A box as tall as its child, whose own height transitions.
fn sized_by_child(width: f32, height: f32) -> RsxNode {
    rsx! {
        <HostElement style={{
            width: Length::px(300.0),
            layout: Layout::flow().column().no_wrap(),
            transition: [Transition::new(TransitionProperty::Height, 100).linear()],
        }}>
            <HostElement style={{ width: Length::px(width), height: Length::px(height) }} />
        </HostElement>
    }
}

/// Samples land on whole pixels; the track still ends, the box then takes
/// its exact content height, and laying it out again at that height starts
/// nothing.
#[test]
fn a_height_transition_to_a_fractional_height_settles() {
    let mut viewport = Viewport::new();
    viewport.set_size(300, 200);
    viewport.set_use_incremental_commit(true);
    frame(&mut viewport, &sized_by_child(300.0, 20.5), 0.0, 0.0);
    let mut running = Vec::new();
    for step in 1..=10 {
        let now = f64::from(step) * 0.05;
        running.push(frame(
            &mut viewport,
            &sized_by_child(300.0, 40.5),
            0.05,
            now,
        ));
    }
    assert!(
        !running[4..].iter().any(|&r| r),
        "the 100ms transition still runs 250ms later: {running:?}"
    );
    let root = viewport.scene.ui_root_keys[0];
    let snapshot = viewport
        .scene
        .node_arena
        .get(root)
        .expect("root")
        .element
        .box_model_snapshot();
    assert_eq!(snapshot.height, 40.5);

    let relaid = (11..=14)
        .map(|step| {
            frame(
                &mut viewport,
                &sized_by_child(280.0, 40.5),
                0.05,
                f64::from(step) * 0.05,
            )
        })
        .collect::<Vec<_>>();
    assert!(!relaid.iter().any(|&r| r), "a new transition: {relaid:?}");
}
