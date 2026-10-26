use super::*;
use crate::style::{ParsedValue, PropertyId};
use crate::view::test_support::{commit_child, commit_element, get_element_mut, new_test_arena};

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
            // Contents sit at their scroll-zero positions; the clamped
            // offset moves them in the viewport.
            for key in [child, descendant] {
                let content = get_element_mut::<Element>(&arena, key);
                let position = content.layout_state.layout_position;
                drop(content);
                let [dx, dy] = arena.viewport_offset(key);
                assert_eq!(
                    (position.x, position.y, position.x + dx, position.y + dy),
                    (
                        origin.x,
                        origin.y,
                        origin.x - final_offset,
                        origin.y - final_offset
                    ),
                    "{layout:?}: host={host_size}, content={content_size}, owner={key:?}"
                );
            }
        }
    }
}
