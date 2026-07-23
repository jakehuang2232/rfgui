use super::*;

#[test]
fn text_area_measure_and_place_clear_local_layout_dirty_flags() {
    let mut text_area = TextArea::new();
    text_area.content = "dirty flag contract".to_string();
    text_area.font_size = 14.0;
    text_area.line_height = 1.25;

    let mut arena = crate::view::test_support::new_test_arena();
    let root = crate::view::test_support::commit_element(
        &mut arena,
        Box::new(text_area) as Box<dyn ElementTrait>,
    );
    arena.with_element_taken(root, |el, _| {
        el.as_any_mut()
            .downcast_mut::<TextArea>()
            .expect("TextArea root")
            .set_self_node_key(root);
    });

    let constraints = LayoutConstraints {
        max_width: 180.0,
        max_height: 80.0,
        viewport_width: 180.0,
        viewport_height: 80.0,
        percent_base_width: Some(180.0),
        percent_base_height: Some(80.0),
    };
    let placement = LayoutPlacement {
        parent_x: 0.0,
        parent_y: 0.0,
        visual_offset_x: 0.0,
        visual_offset_y: 0.0,
        available_width: 180.0,
        available_height: 80.0,
        viewport_width: 180.0,
        viewport_height: 80.0,
        percent_base_width: Some(180.0),
        percent_base_height: Some(80.0),
    };

    arena.with_element_taken(root, |el, arena| {
        el.measure(constraints, arena);
    });
    {
        let measured = crate::view::test_support::get_element::<TextArea>(&arena, root);
        assert!(!measured.local_dirty_flags().intersects(DirtyFlags::LAYOUT));
        assert!(measured.local_dirty_flags().intersects(DirtyFlags::PLACE));
    }

    arena.with_element_taken(root, |el, arena| {
        el.place(placement, arena);
    });
    {
        let placed = crate::view::test_support::get_element::<TextArea>(&arena, root);
        assert!(
            !placed
                .local_dirty_flags()
                .intersects(placement_dirty_flags())
        );
    }
}

#[test]
fn text_area_projection_segment_measure_and_place_clear_layout_dirty_flags() {
    let mut segment = super::super::super::TextAreaProjectionSegment::new();
    let mut arena = crate::view::test_support::new_test_arena();
    let constraints = LayoutConstraints {
        max_width: 120.0,
        max_height: 40.0,
        viewport_width: 120.0,
        viewport_height: 40.0,
        percent_base_width: Some(120.0),
        percent_base_height: Some(40.0),
    };
    let placement = LayoutPlacement {
        parent_x: 8.0,
        parent_y: 12.0,
        visual_offset_x: 0.0,
        visual_offset_y: 0.0,
        available_width: 120.0,
        available_height: 40.0,
        viewport_width: 120.0,
        viewport_height: 40.0,
        percent_base_width: Some(120.0),
        percent_base_height: Some(40.0),
    };

    segment.measure(constraints, &mut arena);
    assert!(!segment.local_dirty_flags().intersects(DirtyFlags::LAYOUT));
    assert!(segment.local_dirty_flags().intersects(DirtyFlags::PLACE));

    segment.place(placement, &mut arena);
    assert!(
        !segment
            .local_dirty_flags()
            .intersects(placement_dirty_flags())
    );
}
