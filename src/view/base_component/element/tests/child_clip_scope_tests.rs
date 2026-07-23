use super::*;

#[test]
fn child_clip_scope_is_skipped_when_children_are_fully_inside_inner_rect() {
    let parent = Element::new(0.0, 0.0, 120.0, 120.0);
    let child = Element::new(20.0, 20.0, 40.0, 40.0);

    let mut arena = new_test_arena();
    let parent_key = commit_element(&mut arena, Box::new(parent));
    let _ = commit_child(&mut arena, parent_key, Box::new(child));

    measure_and_place(
        &mut arena,
        parent_key,
        LayoutConstraints {
            max_width: 200.0,
            max_height: 200.0,
            viewport_width: 200.0,
            viewport_height: 200.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(200.0),
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 200.0,
            available_height: 200.0,
            viewport_width: 200.0,
            viewport_height: 200.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(200.0),
        },
    );

    let child_count = arena.children_of(parent_key).len();
    let parent_ref = crate::view::test_support::get_element::<Element>(&arena, parent_key);
    let inner_radii = parent_ref.inner_clip_radii(normalize_corner_radii(
        parent_ref.border_radii,
        parent_ref.layout_state.layout_size.width.max(0.0),
        parent_ref.layout_state.layout_size.height.max(0.0),
    ));
    let overflow_child_indices: Vec<bool> = (0..child_count)
        .map(|idx| parent_ref.child_renders_outside_inner_clip(idx, &arena))
        .collect();
    assert!(!parent_ref.should_clip_children(&overflow_child_indices, inner_radii, &arena));
}

#[test]
fn child_clip_scope_is_required_when_child_overflows_inner_rect() {
    let parent = Element::new(0.0, 0.0, 100.0, 100.0);
    let mut child = Element::new(0.0, 0.0, 140.0, 40.0);
    let mut style = Style::new();
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(140.0)));
    child.apply_style(style);

    let mut arena = new_test_arena();
    let parent_key = commit_element(&mut arena, Box::new(parent));
    let _ = commit_child(&mut arena, parent_key, Box::new(child));

    measure_and_place(
        &mut arena,
        parent_key,
        LayoutConstraints {
            max_width: 200.0,
            max_height: 200.0,
            viewport_width: 200.0,
            viewport_height: 200.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(200.0),
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 200.0,
            available_height: 200.0,
            viewport_width: 200.0,
            viewport_height: 200.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(200.0),
        },
    );

    let child_count = arena.children_of(parent_key).len();
    let parent_ref = crate::view::test_support::get_element::<Element>(&arena, parent_key);
    let inner_radii = parent_ref.inner_clip_radii(normalize_corner_radii(
        parent_ref.border_radii,
        parent_ref.layout_state.layout_size.width.max(0.0),
        parent_ref.layout_state.layout_size.height.max(0.0),
    ));
    let overflow_child_indices: Vec<bool> = (0..child_count)
        .map(|idx| parent_ref.child_renders_outside_inner_clip(idx, &arena))
        .collect();
    assert!(parent_ref.should_clip_children(&overflow_child_indices, inner_radii, &arena));
}

#[test]
fn child_clip_scope_uses_stencil_without_rounding() {
    let parent = Element::new(0.0, 0.0, 100.0, 100.0);
    let mut child = Element::new(0.0, 0.0, 140.0, 40.0);
    let mut style = Style::new();
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(140.0)));
    child.apply_style(style);

    let mut arena = new_test_arena();
    let parent_key = commit_element(&mut arena, Box::new(parent));
    let _ = commit_child(&mut arena, parent_key, Box::new(child));

    measure_and_place(
        &mut arena,
        parent_key,
        LayoutConstraints {
            max_width: 200.0,
            max_height: 200.0,
            viewport_width: 200.0,
            viewport_height: 200.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(200.0),
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 200.0,
            available_height: 200.0,
            viewport_width: 200.0,
            viewport_height: 200.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(200.0),
        },
    );

    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(200, 200, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let target = ctx.allocate_target(&mut graph);
    ctx.set_current_target(target);

    let inner_radii = {
        let parent_ref = crate::view::test_support::get_element::<Element>(&arena, parent_key);
        parent_ref.inner_clip_radii(normalize_corner_radii(
            parent_ref.border_radii,
            parent_ref.layout_state.layout_size.width.max(0.0),
            parent_ref.layout_state.layout_size.height.max(0.0),
        ))
    };
    assert!(!inner_radii.has_any_rounding());

    let mut parent_mut =
        crate::view::test_support::get_element_mut::<Element>(&arena, parent_key);
    let scope = parent_mut.begin_child_clip_scope(&mut graph, &mut ctx, inner_radii);
    assert!(scope.is_some());
    assert!(scope.as_ref().is_some_and(|scope| scope.child_clip_id != 0));
}

#[test]
fn child_clip_stencil_mask_uses_paint_snapped_destination_origin() {
    let parent = Element::new(0.0, 0.0, 100.5, 50.25);
    let mut child = Element::new(0.0, 0.0, 140.0, 40.0);
    let mut style = Style::new();
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(140.0)));
    child.apply_style(style);

    let mut arena = new_test_arena();
    let parent_key = commit_element(&mut arena, Box::new(parent));
    let _ = commit_child(&mut arena, parent_key, Box::new(child));

    measure_and_place(
        &mut arena,
        parent_key,
        LayoutConstraints {
            max_width: 200.0,
            max_height: 200.0,
            viewport_width: 200.0,
            viewport_height: 200.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(200.0),
        },
        LayoutPlacement {
            parent_x: 10.25,
            parent_y: 20.75,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 200.0,
            available_height: 200.0,
            viewport_width: 200.0,
            viewport_height: 200.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(200.0),
        },
    );

    let mut ctx = UiBuildContext::new(200, 200, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    ctx.translate_paint_offset(-0.25, -0.75);
    let parent_ref = crate::view::test_support::get_element::<Element>(&arena, parent_key);
    let inner_radii = parent_ref.inner_clip_radii(normalize_corner_radii(
        parent_ref.border_radii,
        parent_ref.layout_state.layout_size.width.max(0.0),
        parent_ref.layout_state.layout_size.height.max(0.0),
    ));
    let params = parent_ref.child_clip_stencil_pass_params(&ctx, inner_radii);

    assert_eq!(params.position, [10.0, 20.0]);
    assert_eq!(params.size, [100.5, 50.25]);
}

#[test]
fn fractional_inner_clip_scissor_preserves_raw_coverage() {
    let parent = Element::new(0.0, 0.0, 100.5, 50.25);
    let mut child = Element::new(0.0, 0.0, 140.0, 40.0);
    let mut style = Style::new();
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(140.0)));
    child.apply_style(style);

    let mut arena = new_test_arena();
    let parent_key = commit_element(&mut arena, Box::new(parent));
    let _ = commit_child(&mut arena, parent_key, Box::new(child));

    measure_and_place(
        &mut arena,
        parent_key,
        LayoutConstraints {
            max_width: 200.0,
            max_height: 200.0,
            viewport_width: 200.0,
            viewport_height: 200.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(200.0),
        },
        LayoutPlacement {
            parent_x: 10.25,
            parent_y: 20.75,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 200.0,
            available_height: 200.0,
            viewport_width: 200.0,
            viewport_height: 200.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(200.0),
        },
    );

    let inner_radii = {
        let parent_ref = crate::view::test_support::get_element::<Element>(&arena, parent_key);
        assert_eq!(
            parent_ref.inner_clip_scissor_rect(),
            Some([10, 20, 101, 51])
        );
        parent_ref.inner_clip_radii(normalize_corner_radii(
            parent_ref.border_radii,
            parent_ref.layout_state.layout_size.width.max(0.0),
            parent_ref.layout_state.layout_size.height.max(0.0),
        ))
    };
    let mut ctx = UiBuildContext::new(200, 200, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    ctx.translate_paint_offset(0.4, -0.6);
    let parent_ref = crate::view::test_support::get_element::<Element>(&arena, parent_key);
    let params = parent_ref.child_clip_stencil_pass_params(&ctx, inner_radii);

    assert!((params.position[0] - 10.65).abs() < 0.001);
    assert!((params.position[1] - 20.15).abs() < 0.001);
    assert_eq!(params.size, [100.5, 50.25]);
}

#[test]
fn child_clip_scope_is_skipped_when_inner_scissor_is_outside_ancestor_scissor() {
    let parent = Element::new(100.0, 100.0, 50.0, 50.0);
    let mut child = Element::new(0.0, 0.0, 80.0, 20.0);
    let mut style = Style::new();
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(80.0)));
    child.apply_style(style);

    let mut arena = new_test_arena();
    let parent_key = commit_element(&mut arena, Box::new(parent));
    let _ = commit_child(&mut arena, parent_key, Box::new(child));

    measure_and_place(
        &mut arena,
        parent_key,
        LayoutConstraints {
            max_width: 200.0,
            max_height: 200.0,
            viewport_width: 200.0,
            viewport_height: 200.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(200.0),
        },
        LayoutPlacement {
            parent_x: 100.0,
            parent_y: 100.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 200.0,
            available_height: 200.0,
            viewport_width: 200.0,
            viewport_height: 200.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(200.0),
        },
    );

    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(200, 200, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let target = ctx.allocate_target(&mut graph);
    ctx.set_current_target(target);
    ctx.push_scissor_rect(Some([0, 0, 20, 20]));

    let inner_radii = {
        let parent_ref = crate::view::test_support::get_element::<Element>(&arena, parent_key);
        parent_ref.inner_clip_radii(normalize_corner_radii(
            parent_ref.border_radii,
            parent_ref.layout_state.layout_size.width.max(0.0),
            parent_ref.layout_state.layout_size.height.max(0.0),
        ))
    };

    let mut parent_mut =
        crate::view::test_support::get_element_mut::<Element>(&arena, parent_key);
    let scope = parent_mut.begin_child_clip_scope(&mut graph, &mut ctx, inner_radii);

    assert!(scope.is_none());
    assert_eq!(ctx.current_clip_id(), 0);
    assert_eq!(ctx.scissor_rect(), Some([0, 0, 20, 20]));
}
