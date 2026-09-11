use super::*;
use crate::style::{ParsedValue, PropertyId};
use crate::view::test_support::{commit_child, commit_element, get_element_mut, new_test_arena};

fn fixture() -> (
    crate::view::node_arena::NodeArena,
    crate::view::node_arena::NodeKey,
) {
    let mut arena = new_test_arena();
    let mut element = Element::new_with_id(0xa201, 0.0, 0.0, 48.0, 40.0);
    let mut style = Style::new();
    style.insert(
        PropertyId::Layout,
        ParsedValue::Layout(crate::style::Layout::Grid),
    );
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(48.0)));
    style.insert(PropertyId::Height, ParsedValue::Length(Length::px(40.0)));
    style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(ScrollDirection::Both),
    );
    element.apply_style(style);
    let root = commit_element(&mut arena, Box::new(element));
    let mut viewport = crate::view::viewport::Viewport::new();
    crate::view::viewport::layout_artifact_style_scene_for_test(
        &mut viewport,
        &mut arena,
        root,
        [160.0, 120.0],
    );
    (arena, root)
}

fn inactive(
    arena: &crate::view::node_arena::NodeArena,
    root: crate::view::node_arena::NodeKey,
) -> bool {
    let node = arena.get(root).unwrap();
    node.element
        .as_any()
        .downcast_ref::<Element>()
        .unwrap()
        .has_exact_inactive_scroll_paint(arena)
}

#[test]
fn inactive_scroll_paint_requires_zero_displacement_and_no_overlay() {
    let (arena, root) = fixture();
    assert!(inactive(&arena, root));
    for offset in [
        (1.0, 0.0),
        (0.0, -1.0),
        (f32::NAN, 0.0),
        (0.0, f32::INFINITY),
    ] {
        // Bypass dirty flags deliberately: offset itself must defeat the proof.
        let mut element = get_element_mut::<Element>(&arena, root);
        element.scroll_offset.x = offset.0;
        element.scroll_offset.y = offset.1;
        drop(element);
        assert!(!inactive(&arena, root));
    }
    let mut element = get_element_mut::<Element>(&arena, root);
    element.scroll_offset.x = 0.0;
    element.scroll_offset.y = 0.0;
    // Even an empty child list cannot discard an existing overlay obligation.
    element.layout_state.content_size = Size {
        width: 48.0,
        height: 120.0,
    };
    drop(element);
    assert!(!inactive(&arena, root));
}

#[test]
fn inactive_scroll_paint_rejects_dirty_and_inconsistent_live_ownership() {
    let (mut arena, root) = fixture();
    {
        let mut element = get_element_mut::<Element>(&arena, root);
        element.dirty_flags = element.dirty_flags.union(DirtyPassMask::PLACEMENT);
        drop(element);
        assert!(!inactive(&arena, root));
    }
    let mut viewport = crate::view::viewport::Viewport::new();
    crate::view::viewport::layout_artifact_style_scene_for_test(
        &mut viewport,
        &mut arena,
        root,
        [160.0, 120.0],
    );
    assert!(inactive(&arena, root));
    let child = commit_child(
        &mut arena,
        root,
        Box::new(Element::new_with_id(0xa202, 0.0, 0.0, 20.0, 20.0)),
    );
    crate::view::viewport::layout_artifact_style_scene_for_test(
        &mut viewport,
        &mut arena,
        root,
        [160.0, 120.0],
    );
    assert!(inactive(&arena, root));
    arena.set_arena_children_without_mirror_for_test(root, vec![]);
    assert!(arena.contains_key(child));
    assert!(!inactive(&arena, root));
}

#[test]
fn clamped_scroll_installs_child_and_descendant_positions_in_the_same_layout_pass() {
    use crate::style::Layout;
    fn sized(layout: Layout, size: f32, scroll: ScrollDirection) -> Style {
        let mut style = Style::new();
        style.insert(PropertyId::Layout, ParsedValue::Layout(layout));
        style.insert(PropertyId::Width, ParsedValue::Length(Length::px(size)));
        style.insert(PropertyId::Height, ParsedValue::Length(Length::px(size)));
        style.insert(
            PropertyId::ScrollDirection,
            ParsedValue::ScrollDirection(scroll),
        );
        style
    }
    for layout in [Layout::Grid, Layout::flow().column().no_wrap().into()] {
        let mut arena = new_test_arena();
        let mut host = Element::new_with_id(0xa203, 0.0, 0.0, 40.0, 40.0);
        host.apply_style(sized(layout, 40.0, ScrollDirection::Both));
        let root = commit_element(&mut arena, Box::new(host));
        let mut content = Element::new_with_id(0xa204, 0.0, 0.0, 80.0, 80.0);
        content.apply_style(sized(Layout::Grid, 80.0, ScrollDirection::None));
        let child = commit_child(&mut arena, root, Box::new(content));
        let mut inner = Element::new_with_id(0xa205, 0.0, 0.0, 16.0, 16.0);
        inner.apply_style(sized(Layout::Grid, 16.0, ScrollDirection::None));
        let descendant = commit_child(&mut arena, child, Box::new(inner));
        let mut viewport = crate::view::viewport::Viewport::new();
        // Resize to fitting clamps both axes to zero; content shrink while
        // still overflowing clamps to a smaller, nonzero range. Every row
        // gets exactly one production layout pass, including the first row.
        for (host_size, content_size, requested, final_offset) in [
            (40.0, 80.0, 16.0, 16.0),
            (100.0, 80.0, 16.0, 0.0),
            (40.0, 80.0, 16.0, 16.0),
            (40.0, 48.0, 16.0, 8.0),
        ] {
            get_element_mut::<Element>(&arena, root).apply_style(sized(
                layout,
                host_size,
                ScrollDirection::Both,
            ));
            get_element_mut::<Element>(&arena, root).set_scroll_offset((requested, requested));
            get_element_mut::<Element>(&arena, child).apply_style(sized(
                Layout::Grid,
                content_size,
                ScrollDirection::None,
            ));
            crate::view::viewport::layout_artifact_style_scene_for_test(
                &mut viewport,
                &mut arena,
                root,
                [160.0, 120.0],
            );
            let host = get_element_mut::<Element>(&arena, root);
            assert_eq!(host.get_scroll_offset(), (final_offset, final_offset));
            let origin = host.layout_state.layout_inner_position;
            drop(host);
            for key in [child, descendant] {
                let content = get_element_mut::<Element>(&arena, key);
                assert_eq!(
                    (
                        content.layout_state.layout_position.x,
                        content.layout_state.layout_position.y
                    ),
                    (origin.x - final_offset, origin.y - final_offset),
                    "{layout:?}: host={host_size}, content={content_size}, owner={key:?}"
                );
            }
            assert_eq!(inactive(&arena, root), final_offset == 0.0);
        }
    }
}
