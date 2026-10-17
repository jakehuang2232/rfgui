//! Layout transitions frame by frame: a position sample is placed in the
//! frame that takes it.

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
/// relayout they ask for, then the property sync.
fn frame(viewport: &mut Viewport, scene: &RsxNode, dt: f32, now: f64) {
    viewport.render_rsx(scene).expect("render");
    viewport.run_layout_pass();
    if viewport
        .run_post_layout_transitions(dt, now)
        .relayout_required
    {
        viewport.run_layout_pass();
    }
    viewport.sync_compositor_property_trees();
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
