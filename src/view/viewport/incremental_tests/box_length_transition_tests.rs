//! Padding, border width and gap transitions run through layout: each frame
//! lays out with the interpolated lengths, nested percent bases included.

use super::super::Viewport;
use super::common::run_layout_for_test;
use crate::style::{Border, Color, Layout, Length, Padding, Transition, TransitionProperty};
use crate::ui::{RsxNode, rsx};
use crate::view::Element as HostElement;
use crate::view::node_arena::NodeKey;

/// The outer box goes from 40px padding, no border and no gap to 5%
/// padding, a 10px border and a 20px gap. The inner box's left padding goes
/// from 0 to 50% of the outer content box, which moves at the same time.
fn scene(toggled: bool) -> RsxNode {
    let transparent = Color::rgba(0, 0, 0, 0);
    rsx! {
        <HostElement style={{
            width: Length::px(400.0),
            height: Length::px(300.0),
            layout: Layout::flow().column().no_wrap(),
        }}>
            <HostElement style={{
                width: Length::px(400.0),
                height: Length::px(200.0),
                layout: Layout::flow().column().no_wrap(),
                padding: if toggled {
                    Padding::uniform(Length::percent(5.0))
                } else {
                    Padding::uniform(Length::px(40.0))
                },
                border: Border::uniform(
                    Length::px(if toggled { 10.0 } else { 0.0 }),
                    &transparent,
                ),
                gap: Length::px(if toggled { 20.0 } else { 0.0 }),
                transition: [Transition::new(TransitionProperty::All, 1000).linear()],
            }}>
                <HostElement style={{
                    width: Length::percent(100.0),
                    height: Length::px(100.0),
                    layout: Layout::flow().column().no_wrap(),
                    padding: Padding::new().left(if toggled {
                        Length::percent(50.0)
                    } else {
                        Length::px(0.0)
                    }),
                    transition: [Transition::new(TransitionProperty::Padding, 1000).linear()],
                }}>
                    <HostElement style={{ width: Length::px(10.0), height: Length::px(10.0) }} />
                </HostElement>
                <HostElement style={{ width: Length::px(10.0), height: Length::px(10.0) }} />
            </HostElement>
        </HostElement>
    }
}

/// Renders `scene`, runs one frame of transitions ending at `now`, and lays
/// out again with the samples.
fn frame(viewport: &mut Viewport, scene: &RsxNode, dt: f32, now: f64) {
    viewport.render_rsx(scene).expect("render");
    run_layout_for_test(viewport, 400.0, 300.0);
    let result = viewport.run_post_layout_transitions(dt, now);
    if result.relayout_required {
        run_layout_for_test(viewport, 400.0, 300.0);
    }
}

/// Where the inner box's child and the outer box's second child sit.
fn positions(viewport: &Viewport) -> [f32; 3] {
    let arena = &viewport.scene.node_arena;
    let outer = arena.children_of(viewport.scene.ui_root_keys[0])[0];
    let [inner, sibling]: [NodeKey; 2] = arena.children_of(outer).try_into().expect("two");
    let leaf = arena.children_of(inner)[0];
    let snapshot = |key| arena.get(key).expect("node").element.box_model_snapshot();
    [snapshot(leaf).x, snapshot(leaf).y, snapshot(sibling).y]
}

fn assert_positions(label: &str, actual: [f32; 3], expected: [f32; 3]) {
    let off = (0..3)
        .map(|i| (actual[i] - expected[i]).abs())
        .fold(0.0, f32::max);
    assert!(
        off < 0.01,
        "{label}: [leaf x, leaf y, sibling y] = {actual:?}, expected {expected:?}"
    );
}

#[test]
fn padding_border_width_and_gap_transition_through_nested_layout() {
    let mut viewport = Viewport::new();
    viewport.set_size(400, 300);
    viewport.set_use_incremental_commit(true);
    frame(&mut viewport, &scene(false), 0.0, 0.0);
    assert_positions("start", positions(&viewport), [40.0, 40.0, 140.0]);

    // Halfway: border 5, padding calc(20px + 2.5%) = 30 left, 27.5 top,
    // gap 10. The outer content box is 330 wide, so the inner left padding,
    // calc(25%), is 82.5.
    frame(&mut viewport, &scene(true), 0.5, 0.5);
    assert_positions("halfway", positions(&viewport), [117.5, 32.5, 142.5]);

    // Turning back starts from the halfway lengths, not from either end.
    frame(&mut viewport, &scene(false), 0.0, 0.5);
    assert_positions("turned back", positions(&viewport), [117.5, 32.5, 142.5]);

    frame(&mut viewport, &scene(false), 1.0, 1.5);
    assert_positions("end", positions(&viewport), [40.0, 40.0, 140.0]);
}
