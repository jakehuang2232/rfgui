use super::*;

fn host(direction: ScrollDirection) -> Element {
    let mut host = Element::new(0., 0., 400., 400.);
    host.scroll_direction = direction;
    host.layout_state.layout_inner_position = Position { x: 37., y: 60. };
    host.layout_state.layout_inner_size = Size {
        width: 400.,
        height: 400.,
    };
    host
}

#[test]
fn paint_interest_is_stable_in_content_space_until_grid_boundary() {
    let mut element = host(ScrollDirection::Both);
    let first = element.child_paint_cull_rect();
    element.scroll_offset = Position { x: 12., y: 12. };
    let second = element.child_paint_cull_rect();
    assert_eq!(
        [first.x, first.y, first.width, first.height],
        [second.x + 12., second.y + 12., second.width, second.height]
    );
    element.scroll_offset = Position { x: 96., y: 96. };
    let crossed = element.child_paint_cull_rect();
    assert_ne!(
        [first.x, first.y, first.width, first.height],
        [
            crossed.x + 96.,
            crossed.y + 96.,
            crossed.width,
            crossed.height
        ]
    );
}

#[test]
fn paint_interest_preserves_existing_overscan_and_non_scrolling_axes() {
    for direction in [
        ScrollDirection::None,
        ScrollDirection::Horizontal,
        ScrollDirection::Vertical,
        ScrollDirection::Both,
    ] {
        let mut element = host(direction);
        let margin = Element::SHOULD_RENDER_OVERSCAN_PX;
        for offset in [-321.25, -12., 0., 12., 24., 100.5, 4096.] {
            element.scroll_offset = Position {
                x: offset,
                y: offset,
            };
            let rect = element.child_paint_cull_rect();
            assert!(rect.x <= 37. - margin && rect.y <= 60. - margin);
            assert!(rect.x + rect.width >= 437. + margin);
            assert!(rect.y + rect.height >= 460. + margin);
            if matches!(direction, ScrollDirection::None | ScrollDirection::Vertical) {
                assert_eq!((rect.x, rect.width), (37. - margin, 400. + margin * 2.));
            }
            if matches!(
                direction,
                ScrollDirection::None | ScrollDirection::Horizontal
            ) {
                assert_eq!((rect.y, rect.height), (60. - margin, 400. + margin * 2.));
            }
        }
    }
}

#[test]
fn inline_translation_preserves_content_extent_and_child_mask() {
    use crate::style::{ParsedValue, PropertyId, Style};
    use crate::view::test_support::{
        commit_child, commit_element, measure_and_place, new_test_arena,
    };
    let mut arena = new_test_arena();
    let mut row = Element::new_with_id(0x7e21, 0., 0., 400., 24.);
    let mut style = Style::new();
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    row.apply_style(style);
    let root = commit_element(&mut arena, Box::new(row));
    commit_child(
        &mut arena,
        root,
        Box::new(Text::new(0., 0., 160., 20., "moving text")),
    );
    let mut expected = None;
    for y in [60., 48., 60., 12., -12., 60.] {
        measure_and_place(
            &mut arena,
            root,
            LayoutConstraints {
                max_width: 400.,
                max_height: 24.,
                viewport_width: 640.,
                viewport_height: 480.,
                percent_base_width: Some(400.),
                percent_base_height: Some(24.),
            },
            LayoutPlacement {
                parent_x: 37.,
                parent_y: y,
                visual_offset_x: 0.,
                visual_offset_y: 0.,
                available_width: 400.,
                available_height: 24.,
                viewport_width: 640.,
                viewport_height: 480.,
                percent_base_width: Some(400.),
                percent_base_height: Some(24.),
            },
        );
        let node = arena.get(root).unwrap();
        let row = node.element.as_any().downcast_ref::<Element>().unwrap();
        let actual = (
            row.layout_state.content_size.width,
            row.layout_state.content_size.height,
            row.requires_child_mask_surface(&arena),
        );
        if let Some(expected) = expected {
            assert_eq!(actual, expected, "parent y={y}");
        } else {
            assert!(actual.0 > 0. && actual.1 > 0.);
            expected = Some(actual);
        }
        drop(node);
        let keys = arena.iter().map(|(key, _)| key).collect::<Vec<_>>();
        for key in keys {
            arena
                .get_mut(key)
                .unwrap()
                .element
                .clear_local_dirty_flags(DirtyFlags::ALL);
        }
        arena.clear_arena_dirty_subtree(root, DirtyFlags::ALL);
    }
}
