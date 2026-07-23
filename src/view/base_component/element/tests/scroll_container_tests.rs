use super::*;

#[test]
fn scrollbar_fade_uses_one_frame_sample_and_stops_after_hidden() {
    let mut element = Element::new(0.0, 0.0, 100.0, 80.0);
    let mut style = Style::new();
    style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(ScrollDirection::Vertical),
    );
    element.apply_style(style);
    element.layout_state.content_size = Size {
        width: 100.0,
        height: 300.0,
    };

    let frame = crate::time::Instant::now();
    assert!(element.set_hovered(true));
    assert!(element.wants_animation_frame());
    assert!(
        element
            .tick_post_layout_animation_frame(frame)
            .contains(DirtyFlags::PAINT)
    );
    assert_eq!(
        element.scrollbar_visibility_alpha().to_bits(),
        1.0_f32.to_bits()
    );

    assert!(element.set_hovered(false));
    let leave_frame = frame + crate::time::Duration::from_millis(10);
    assert!(
        element
            .tick_post_layout_animation_frame(leave_frame)
            .contains(DirtyFlags::PAINT)
    );
    assert!(element.wants_animation_frame());

    let fade_frame = leave_frame + crate::time::Duration::from_millis(1_000);
    assert!(
        element
            .tick_post_layout_animation_frame(fade_frame)
            .contains(DirtyFlags::PAINT)
    );
    assert!((0.0..1.0).contains(&element.scrollbar_visibility_alpha()));
    assert!(element.wants_animation_frame());

    let hidden_frame = leave_frame + crate::time::Duration::from_millis(1_250);
    assert!(
        element
            .tick_post_layout_animation_frame(hidden_frame)
            .contains(DirtyFlags::PAINT)
    );
    assert_eq!(
        element.scrollbar_visibility_alpha().to_bits(),
        0.0_f32.to_bits()
    );
    assert!(!element.wants_animation_frame());
    assert!(
        element
            .tick_post_layout_animation_frame(
                hidden_frame + crate::time::Duration::from_millis(16),
            )
            .is_empty()
    );

    element.scrollbar_drag = Some(ScrollbarDragState {
        axis: ScrollbarAxis::Vertical,
        grab_offset: 0.0,
        reanchor_on_first_move: false,
    });
    let drag_frame = hidden_frame + crate::time::Duration::from_millis(32);
    assert!(
        element
            .tick_post_layout_animation_frame(drag_frame)
            .contains(DirtyFlags::PAINT)
    );
    assert!(element.cancel_pointer_interaction());
    assert!(element.wants_animation_frame());
    assert!(
        element
            .tick_post_layout_animation_frame(drag_frame)
            .contains(DirtyFlags::PAINT)
    );
    assert_eq!(
        element.scrollbar_visibility_alpha().to_bits(),
        1.0_f32.to_bits()
    );
}

#[test]
fn retained_scroll_content_subtree_offset_allows_absolute_descendants_and_fails_closed() {
    let mut content = Element::new_with_id(91_000, 10.25, 20.75, 200.0, 160.0);
    let mut content_style = Style::new();
    content_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    content_style.insert(
        PropertyId::Position,
        ParsedValue::Position(Position::absolute().left(Length::px(10.25))),
    );
    content.apply_style(content_style);
    let mut arena = new_test_arena();
    let root = commit_element(&mut arena, Box::new(content));

    let mut absolute_child = Element::new_with_id(91_001, 30.0, 40.0, 20.0, 20.0);
    let mut child_style = Style::new();
    child_style.insert(
        PropertyId::Position,
        ParsedValue::Position(Position::absolute().left(Length::px(30.0))),
    );
    absolute_child.apply_style(child_style);
    commit_child(&mut arena, root, Box::new(absolute_child));

    let offset = arena
        .get(root)
        .unwrap()
        .element
        .as_any()
        .downcast_ref::<Element>()
        .unwrap()
        .exact_retained_scroll_content_subtree_recording_offset([4.0, 8.0]);
    assert_eq!(offset, Some([3.75, 8.25]));

    let mut scroll_style = Style::new();
    scroll_style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(ScrollDirection::Both),
    );
    let mut node = arena.get_mut(root).unwrap();
    let content = node.element.as_any_mut().downcast_mut::<Element>().unwrap();
    content.apply_style(scroll_style);
    assert!(
        content
            .exact_retained_scroll_content_subtree_recording_offset([4.0, 8.0])
            .is_none()
    );

    let mut non_scroll_style = Style::new();
    non_scroll_style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(ScrollDirection::None),
    );
    content.apply_style(non_scroll_style);
    content.set_resolved_transform_for_test(Some(Mat4::IDENTITY));
    assert!(
        content
            .exact_retained_scroll_content_subtree_recording_offset([4.0, 8.0])
            .is_none()
    );
}

#[test]
fn scrollbar_hover_resolves_against_same_frame_final_layout_geometry() {
    let mut element = Element::new(0.0, 0.0, 100.0, 80.0);
    let mut style = Style::new();
    style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(ScrollDirection::Vertical),
    );
    element.apply_style(style);
    element.layout_state.content_size = Size {
        width: 100.0,
        height: 80.0,
    };
    assert!(element.set_hovered(true));

    let semantic_now = crate::time::Instant::now();
    assert!(
        element.tick_animation_frame(semantic_now).is_empty(),
        "pre-layout animation must not consume scrollbar pending state"
    );

    // Simulate this frame's final layout turning the same host from
    // non-scrollable into scrollable before property observation.
    element.layout_state.content_size.height = 300.0;
    assert!(
        element
            .tick_post_layout_animation_frame(semantic_now)
            .contains(DirtyFlags::PAINT)
    );
    assert_eq!(
        element.scrollbar_visibility_alpha().to_bits(),
        1.0_f32.to_bits()
    );
    assert!(element.last_scrollbar_interaction.is_some());
}

#[test]
fn scroll_container_build_restores_scissor_and_clip_state() {
    let mut parent = Element::new(0.0, 0.0, 120.0, 120.0);
    let mut parent_style = Style::new();
    parent_style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(crate::style::ScrollDirection::Vertical),
    );
    parent.apply_style(parent_style);
    let child = Element::new(0.0, 0.0, 120.0, 360.0);

    let mut arena = new_test_arena();
    let parent_key = commit_element(&mut arena, Box::new(parent));
    let _ = commit_child(&mut arena, parent_key, Box::new(child));

    measure_and_place(
        &mut arena,
        parent_key,
        LayoutConstraints {
            max_width: 120.0,
            max_height: 120.0,
            viewport_width: 120.0,
            viewport_height: 120.0,
            percent_base_width: Some(120.0),
            percent_base_height: Some(120.0),
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 120.0,
            available_height: 120.0,
            viewport_width: 120.0,
            viewport_height: 120.0,
            percent_base_width: Some(120.0),
            percent_base_height: Some(120.0),
        },
    );

    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(120, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let target = ctx.allocate_target(&mut graph);
    ctx.set_current_target(target);

    let ctx_for_build = UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone());
    let next_state = arena
        .with_element_taken(parent_key, |el, a| el.build(&mut graph, a, ctx_for_build))
        .expect("parent build returns state");

    assert_eq!(
        next_state.scissor_rect, None,
        "scroll container build should not leak scissor rect to sibling roots"
    );
    assert!(
        next_state.clip_id_stack.is_empty(),
        "scroll container build should not leak clip ids to sibling roots"
    );
}

#[test]
fn vertical_scroll_container_does_not_expand_auto_height_flex_row_child() {
    let mut parent = Element::new(0.0, 0.0, 120.0, 120.0);
    let mut parent_style = Style::new();
    parent_style.insert(
        PropertyId::Layout,
        ParsedValue::Layout(
            Layout::flow()
                .column()
                .no_wrap()
                .cross_size(CrossSize::Stretch)
                .into(),
        ),
    );
    parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(120.0)));
    parent_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(120.0)));
    parent_style.insert(
        PropertyId::ScrollDirection,
        ParsedValue::ScrollDirection(crate::style::ScrollDirection::Vertical),
    );
    parent.apply_style(parent_style);

    let mut row_child = Element::new(0.0, 0.0, 0.0, 0.0);
    let mut row_style = Style::new();
    row_style.insert(
        PropertyId::Layout,
        ParsedValue::Layout(Layout::flex().row().into()),
    );
    row_style.insert(
        PropertyId::Width,
        ParsedValue::Length(Length::percent(100.0)),
    );
    row_child.apply_style(row_style);

    let mut arena = new_test_arena();
    let parent_key = commit_element(&mut arena, Box::new(parent));
    let row_key = commit_child(&mut arena, parent_key, Box::new(row_child));
    let _ = commit_child(
        &mut arena,
        row_key,
        Box::new(Element::new(0.0, 0.0, 40.0, 24.0)),
    );

    measure_and_place(
        &mut arena,
        parent_key,
        LayoutConstraints {
            max_width: 120.0,
            max_height: 120.0,
            viewport_width: 120.0,
            viewport_height: 120.0,
            percent_base_width: Some(120.0),
            percent_base_height: Some(120.0),
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 120.0,
            available_height: 120.0,
            viewport_width: 120.0,
            viewport_height: 120.0,
            percent_base_width: Some(120.0),
            percent_base_height: Some(120.0),
        },
    );

    let row_snapshot = nth_child_snapshot(&arena, parent_key, 0);
    assert!((row_snapshot.height - 24.0).abs() < 0.01);
    let parent_ref = crate::view::test_support::get_element::<Element>(&arena, parent_key);
    assert!((parent_ref.layout_state.content_size.height - 24.0).abs() < 0.01);
}

#[test]
fn flow_cross_size_stretch_aligns_using_current_then_final_cross_size() {
    for align in [Align::Center, Align::End] {
        let mut parent = Element::new(0.0, 0.0, 320.0, 140.0);
        let mut parent_style = Style::new();
        parent_style.insert(
            PropertyId::Layout,
            ParsedValue::Layout(
                Layout::flow()
                    .row()
                    .no_wrap()
                    .align(Align::Start)
                    .cross_size(CrossSize::Fit)
                    .into(),
            ),
        );
        parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(320.0)));
        parent_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(140.0)));
        parent.apply_style(parent_style);

        let mut tall = Element::new(0.0, 0.0, 120.0, 100.0);
        let mut tall_style = Style::new();
        tall_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(120.0)));
        tall_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(100.0)));
        tall.apply_style(tall_style);

        let mut stretched = Element::new(0.0, 0.0, 120.0, 0.0);
        let mut stretched_style = Style::new();
        stretched_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(120.0)));
        stretched_style.insert(
            PropertyId::Transition,
            ParsedValue::Transition(Transition::new(TransitionProperty::Height, 180).into()),
        );
        stretched.apply_style(stretched_style);

        let mut arena = new_test_arena();
        let parent_key = commit_element(&mut arena, Box::new(parent));
        let _ = commit_child(&mut arena, parent_key, Box::new(tall));
        let stretched_key = commit_child(&mut arena, parent_key, Box::new(stretched));
        let _ = commit_child(
            &mut arena,
            stretched_key,
            Box::new(Element::new(0.0, 0.0, 120.0, 40.0)),
        );

        let constraints = LayoutConstraints {
            max_width: 320.0,
            max_height: 140.0,
            viewport_width: 320.0,
            viewport_height: 140.0,
            percent_base_width: Some(320.0),
            percent_base_height: Some(140.0),
        };
        let placement = LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 320.0,
            available_height: 140.0,
            viewport_width: 320.0,
            viewport_height: 140.0,
            percent_base_width: Some(320.0),
            percent_base_height: Some(140.0),
        };
        measure_and_place(&mut arena, parent_key, constraints, placement);
        let _ = crate::view::test_support::get_element_mut::<Element>(&arena, stretched_key)
            .take_layout_transition_requests();

        let mut next_parent_style = Style::new();
        next_parent_style.insert(
            PropertyId::Layout,
            ParsedValue::Layout(
                Layout::flow()
                    .row()
                    .no_wrap()
                    .align(align)
                    .cross_size(CrossSize::Stretch)
                    .into(),
            ),
        );
        next_parent_style.insert(PropertyId::Width, ParsedValue::Length(Length::px(320.0)));
        next_parent_style.insert(PropertyId::Height, ParsedValue::Length(Length::px(140.0)));
        crate::view::test_support::get_element_mut::<Element>(&arena, parent_key)
            .apply_style(next_parent_style);
        arena.with_element_taken(parent_key, |el, a| el.measure(constraints, a));
        {
            let parent_ref =
                crate::view::test_support::get_element::<Element>(&arena, parent_key);
            assert_eq!(
                parent_ref.computed_style.layout_axis_cross_size(),
                CrossSize::Stretch
            );
        }
        {
            let stretched_ref =
                crate::view::test_support::get_element::<Element>(&arena, stretched_key);
            assert!(stretched_ref.flex_props().allows_cross_stretch(true));
        }
        arena.with_element_taken(parent_key, |el, a| el.place(placement, a));

        let stretched_snapshot = child_snapshot(&arena, stretched_key);
        let expected_animated_y = match align {
            Align::Start => 0.0,
            Align::Center => 50.0,
            Align::End => 100.0,
        };

        assert!(
            (stretched_snapshot.y - expected_animated_y).abs() < 0.01,
            "stretched child should align using current animated height for {align:?}, got y={}, expected {}",
            stretched_snapshot.y,
            expected_animated_y
        );
        assert!(
            (stretched_snapshot.height - 40.0).abs() < 0.01,
            "stretched child should still render previous height during animation for {align:?}, got h={}",
            stretched_snapshot.height
        );

        crate::view::test_support::get_element_mut::<Element>(&arena, stretched_key)
            .set_layout_transition_height(100.0);

        arena.with_element_taken(parent_key, |el, a| el.place(placement, a));

        {
            let mut stretched_mut =
                crate::view::test_support::get_element_mut::<Element>(&arena, stretched_key);
            stretched_mut.layout_transition_override_height = None;
            stretched_mut.layout_transition_target_height = None;
        }

        arena.with_element_taken(parent_key, |el, a| el.place(placement, a));

        let stretched_snapshot = child_snapshot(&arena, stretched_key);
        let expected_final_y = match align {
            Align::Start => 0.0,
            Align::Center => 20.0,
            Align::End => 40.0,
        };
        assert!(
            (stretched_snapshot.y - expected_final_y).abs() < 0.01,
            "stretched child should align using final cross size after animation for {align:?}, got y={}, expected {}",
            stretched_snapshot.y,
            expected_final_y
        );
    }
}
