use super::*;
use crate::style::{Transform, Translate};
use crate::view::node_arena::NodeKey;

const ROWS: usize = 20;
const ROW_HEIGHT: f32 = 20.0;

/// A 100x50 scroller whose rows sit one unclipped wrapper below it, so only
/// an inherited paint interest can cull them.
fn nested_rows_tree(scroller_height: f32, wrapper_transform: Option<Transform>) -> RsxNode {
    let rows = (0..ROWS)
        .map(|_| {
            rsx! {
                <HostElement style={{
                    width: Length::px(80.0),
                    height: Length::px(ROW_HEIGHT),
                    background_color: Color::hex("#336699"),
                }} />
            }
        })
        .collect::<Vec<_>>();
    let wrapper = match wrapper_transform {
        Some(transform) => rsx! {
            <HostElement style={{
                layout: Layout::flow().column().no_wrap(),
                width: Length::px(80.0),
                transform: transform,
            }}>
                {rows}
            </HostElement>
        },
        None => rsx! {
            <HostElement style={{
                layout: Layout::flow().column().no_wrap(),
                width: Length::px(80.0),
            }}>
                {rows}
            </HostElement>
        },
    };
    rsx! {
        <HostElement style={{
            width: Length::px(100.0),
            height: Length::px(scroller_height),
            scroll_direction: ScrollDirection::Vertical,
        }}>
            {wrapper}
        </HostElement>
    }
}

fn row_keys(viewport: &Viewport) -> (NodeKey, Vec<NodeKey>) {
    let root = viewport.scene.ui_root_keys[0];
    let wrapper = viewport.scene.node_arena.children_of(root)[0];
    let rows = viewport.scene.node_arena.children_of(wrapper);
    assert_eq!(rows.len(), ROWS);
    (root, rows)
}

fn rendered_rows(viewport: &Viewport) -> Vec<usize> {
    let (_, rows) = row_keys(viewport);
    rows.iter()
        .enumerate()
        .filter(|(_, key)| {
            viewport
                .scene
                .node_arena
                .get(**key)
                .expect("row")
                .element
                .box_model_snapshot()
                .should_render
        })
        .map(|(index, _)| index)
        .collect()
}

fn scroll_root_to(viewport: &mut Viewport, y: f32) {
    let root = viewport.scene.ui_root_keys[0];
    let id = viewport
        .scene
        .node_arena
        .get(root)
        .expect("root")
        .element
        .stable_id();
    assert!(crate::view::viewport::dispatch::set_scroll_offset_by_id(
        &viewport.scene.node_arena,
        root,
        id,
        (0.0, y),
    ));
}

/// The scroller interest is its 50px viewport plus overscan aligned to
/// 96px content steps: rows intersecting content y in [-96, 96) record.
#[test]
fn rows_below_an_unclipped_wrapper_are_culled_by_the_scroller() {
    let mut viewport = Viewport::new();
    viewport
        .render_rsx(&nested_rows_tree(50.0, None))
        .expect("cold render");
    run_layout_for_test(&mut viewport, 200.0, 200.0);
    assert_eq!(rendered_rows(&viewport), (0..5).collect::<Vec<_>>());
}

#[test]
fn scrolling_reevaluates_nested_row_culling() {
    let mut viewport = Viewport::new();
    viewport
        .render_rsx(&nested_rows_tree(50.0, None))
        .expect("cold render");
    run_layout_for_test(&mut viewport, 200.0, 200.0);

    // Visible content [200, 250) aligns to the interest [96, 288).
    scroll_root_to(&mut viewport, 200.0);
    run_layout_for_test(&mut viewport, 200.0, 200.0);
    assert_eq!(rendered_rows(&viewport), (4..15).collect::<Vec<_>>());

    scroll_root_to(&mut viewport, 0.0);
    run_layout_for_test(&mut viewport, 200.0, 200.0);
    assert_eq!(rendered_rows(&viewport), (0..5).collect::<Vec<_>>());
}

#[test]
fn growing_the_scroller_exposes_rows_that_did_not_move() {
    let mut viewport = Viewport::new();
    viewport.set_use_incremental_commit(true);
    viewport
        .render_rsx(&nested_rows_tree(50.0, None))
        .expect("cold render");
    run_layout_for_test(&mut viewport, 400.0, 400.0);
    let (_, rows_before) = row_keys(&viewport);

    // Interest [-96, 288) once the viewport is 200px tall.
    viewport
        .render_rsx(&nested_rows_tree(200.0, None))
        .expect("resize commits incrementally");
    run_layout_for_test(&mut viewport, 400.0, 400.0);
    assert_eq!(row_keys(&viewport).1, rows_before);
    assert_eq!(rendered_rows(&viewport), (0..15).collect::<Vec<_>>());
}

/// Rows of a transformed wrapper live in its untransformed space; the
/// scroller's interest does not apply to them there.
#[test]
fn transformed_wrapper_children_keep_their_own_interest() {
    let mut viewport = Viewport::new();
    viewport
        .render_rsx(&nested_rows_tree(
            50.0,
            Some(Transform::new([
                Translate::x(Length::px(0.0)).with_y(Length::px(-200.0))
            ])),
        ))
        .expect("cold render");
    run_layout_for_test(&mut viewport, 200.0, 200.0);
    assert_eq!(rendered_rows(&viewport), (0..ROWS).collect::<Vec<_>>());
}
