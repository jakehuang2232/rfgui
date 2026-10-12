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
#[should_panic(expected = "one render target cannot mix logical and target-physical scissors")]
fn one_build_state_rejects_cross_space_scissor_intersection() {
    let mut ctx = UiBuildContext::new(20, 20, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    ctx.push_graphics_pass_scissor(Some(
        crate::view::render_pass::render_target::GraphicsPassScissor::Logical([0, 0, 10, 10]),
    ));
    ctx.push_graphics_pass_scissor(Some(
        crate::view::render_pass::render_target::GraphicsPassScissor::TargetPhysical([
            0, 0, 10, 10,
        ]),
    ));
}
