use super::*;

#[test]
fn inline_ifc_owned_nodes_do_not_keep_placement_dirty() {
    use crate::view::Text as HostText;

    // Regression: IFC-owned spans/texts never run their own place(), so
    // the install must clear their local PLACEMENT dirt. If it does not,
    // the subtree aggregate stays dirty and the whole tree re-places (and
    // re-installs every IFC plan) on every frame, even fully idle ones.
    let paragraphs = (0..3)
        .map(|i| {
            rsx! {
                <HostElement style={{
                    layout: Layout::Inline,
                    width: Length::percent(100.0),
                }}>
                    <HostElement style={{
                        padding: Padding::uniform(Length::px(3.0)),
                    }}>
                        <HostText>{format!("badge {i}")}</HostText>
                    </HostElement>
                    <HostText>{format!("paragraph body {i} with words")}</HostText>
                </HostElement>
            }
        })
        .collect::<Vec<_>>();
    let tree = rsx! {
        <HostElement style={{
            layout: Layout::flow().column().no_wrap(),
            width: Length::px(400.0),
        }}>{paragraphs}</HostElement>
    };

    let mut viewport = Viewport::new();
    viewport.set_size(400, 600);
    viewport.render_rsx(&tree).expect("render tree");
    run_layout_for_test(&mut viewport, 400.0, 600.0);

    // Second layout with identical constraints and placement: the whole
    // tree must skip — no node re-placed, no IFC install re-applied.
    let (_gate_profile, place_profile) =
        run_layout_for_test_with_gate_profile(&mut viewport, 400.0, 600.0);
    assert_eq!(
        place_profile.node_count, 0,
        "idle relayout must not re-place any node"
    );
    assert_eq!(
        place_profile.inline_ifc_root_install_calls, 0,
        "idle relayout must not re-run any IFC root install"
    );
}

#[test]
fn inline_ifc_pure_move_shift_matches_full_apply() {
    use crate::view::Text as HostText;
    use crate::view::base_component::Text as TextHost;

    // A pure root move re-applies an unchanged install plan via the
    // in-place delta-shift fast path. The resulting owned geometry must
    // be identical to a full plan apply at the target position.
    fn tree() -> RsxNode {
        rsx! {
            <HostElement style={{
                layout: Layout::Inline,
                width: Length::px(300.0),
            }}>
                <HostElement style={{
                    padding: Padding::uniform(Length::px(3.0)),
                }}>
                    <HostText>"badge text"</HostText>
                </HostElement>
                <HostText>"trailing words that wrap across the line"</HostText>
            </HostElement>
        }
    }

    fn text_lines_at(viewport: &Viewport, offsets: &[(f32, f32)]) -> Vec<Vec<(String, f32, f32)>> {
        let root_key = viewport.scene.ui_root_keys[0];
        let children = viewport.scene.node_arena.children_of(root_key);
        let badge_text_key = viewport.scene.node_arena.children_of(children[0])[0];
        let _ = offsets;
        [badge_text_key, children[1]]
            .iter()
            .map(|&key| {
                viewport
                    .scene
                    .node_arena
                    .get(key)
                    .expect("text node")
                    .element
                    .as_any()
                    .downcast_ref::<TextHost>()
                    .expect("text node")
                    .inline_fragment_positions()
                    .into_iter()
                    .map(|(content, position)| (content, position.x, position.y))
                    .collect()
            })
            .collect()
    }

    fn place_at(viewport: &mut Viewport, x: f32, y: f32) {
        let constraints = crate::view::base_component::LayoutConstraints {
            max_width: 400.0,
            max_height: 300.0,
            viewport_width: 400.0,
            viewport_height: 300.0,
            percent_base_width: Some(400.0),
            percent_base_height: Some(300.0),
        };
        let placement = crate::view::base_component::LayoutPlacement {
            parent_x: x,
            parent_y: y,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 400.0,
            available_height: 300.0,
            viewport_width: 400.0,
            viewport_height: 300.0,
            percent_base_width: Some(400.0),
            percent_base_height: Some(300.0),
        };
        let mut arena = std::mem::take(&mut viewport.scene.node_arena);
        let root_keys = viewport.scene.ui_root_keys.clone();
        for &root in &root_keys {
            arena.refresh_subtree_dirty_cache(root);
        }
        for &root in &root_keys {
            arena.with_element_taken(root, |el, arena| {
                el.measure(constraints, arena);
            });
        }
        for &root in &root_keys {
            arena.refresh_subtree_dirty_cache(root);
        }
        for &root in &root_keys {
            arena.with_element_taken(root, |el, arena| {
                el.place(placement, arena);
            });
        }
        viewport.scene.node_arena = arena;
    }

    // Shift path: layout at origin, then move to (7, 11).
    let mut moved = Viewport::new();
    moved.set_size(400, 300);
    moved.render_rsx(&tree()).expect("render moved tree");
    run_layout_for_test(&mut moved, 400.0, 300.0);
    place_at(&mut moved, 7.0, 11.0);

    // Full-apply reference: fresh tree placed directly at (7, 11).
    let mut reference = Viewport::new();
    reference.set_size(400, 300);
    reference
        .render_rsx(&tree())
        .expect("render reference tree");
    place_at(&mut reference, 7.0, 11.0);

    let moved_lines = text_lines_at(&moved, &[]);
    let reference_lines = text_lines_at(&reference, &[]);
    assert_eq!(
        moved_lines, reference_lines,
        "delta-shifted owned text geometry must match a full apply at the same origin"
    );
}
