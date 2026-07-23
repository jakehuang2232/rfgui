use super::*;

#[test]
fn texture_desc_for_logical_bounds_keeps_logical_scale_mapping() {
    let bounds = super::super::RetainedSurfaceBounds {
        x: 10.0,
        y: 20.0,
        width: 30.0,
        height: 40.0,
        corner_radii: [0.0; 4],
    };

    let unscaled = super::super::texture_desc_for_logical_bounds(
        bounds,
        1.0,
        None,
        wgpu::TextureFormat::Bgra8Unorm,
    );
    let scaled = super::super::texture_desc_for_logical_bounds(
        bounds,
        1.0,
        Some(Mat4::from_scale(Vec3::new(2.0, 2.0, 1.0))),
        wgpu::TextureFormat::Bgra8Unorm,
    );

    assert_eq!(unscaled.width(), 30);
    assert_eq!(unscaled.height(), 40);
    assert_eq!(scaled.width(), 30);
    assert_eq!(scaled.height(), 40);
}

#[test]
fn build_context_render_transform_propagates_to_child_without_leaking_back() {
    let mut parent_ctx = UiBuildContext::new(120, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let parent_transform = Mat4::from_scale(Vec3::new(2.0, 1.5, 1.0));
    parent_ctx.set_current_render_transform(Some(parent_transform));

    let parent_viewport = parent_ctx.viewport();
    let mut child_ctx =
        UiBuildContext::from_parts(parent_viewport.clone(), parent_ctx.state_clone());
    assert_eq!(child_ctx.current_render_transform(), Some(parent_transform));

    let child_transform = Mat4::from_scale(Vec3::new(3.0, 3.0, 1.0));
    child_ctx.set_current_render_transform(Some(child_transform));

    let restored_parent = UiBuildContext::from_parts(parent_viewport, child_ctx.into_state());
    assert_eq!(
        restored_parent.current_render_transform(),
        Some(parent_transform)
    );
}

#[test]
fn layer_subtree_does_not_inherit_ancestor_stencil_clip_id() {
    let mut ctx = UiBuildContext::new(120, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    assert_eq!(
        ctx.graphics_pass_context().stencil_clip_id,
        None,
        "fresh build context should not start with an active clip"
    );

    let pushed = ctx.push_clip_id();
    assert_eq!(pushed, Some(1), "first pushed clip id should be 1");

    let ancestor_clip = ctx.ancestor_clip_context();
    let layer_state = ctx.layer_subtree_state_with_ancestor_clip(ancestor_clip);
    let layer_ctx = UiBuildContext::from_parts(ctx.viewport(), layer_state);

    assert_eq!(
        layer_ctx.graphics_pass_context().stencil_clip_id,
        None,
        "offscreen layer subtree should not inherit ancestor stencil clip id"
    );
}

#[test]
fn transformed_layer_subtree_starts_without_ancestor_scissor_rect() {
    let mut ctx = UiBuildContext::new(120, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let previous = ctx.push_scissor_rect(Some([10, 10, 40, 40]));
    assert_eq!(previous, None);
    assert_eq!(
        ctx.graphics_pass_context().scissor_rect,
        Some([10, 10, 40, 40])
    );

    let layer_state =
        ctx.layer_subtree_state_with_ancestor_clip(super::super::AncestorClipContext::default());
    let layer_ctx = UiBuildContext::from_parts(ctx.viewport(), layer_state);

    assert_eq!(
        layer_ctx.graphics_pass_context().scissor_rect,
        None,
        "transformed offscreen subtree should rasterize from viewport clip, not ancestor scissor"
    );
}

#[test]
fn zero_opacity_sets_should_paint_false_but_keeps_render() {
    let mut arena = new_test_arena();
    let mut el = Element::new(0.0, 0.0, 100.0, 40.0);
    let mut style = Style::new();
    style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::hex("#112233")),
    );
    style.insert(PropertyId::Opacity, ParsedValue::Opacity(Opacity::new(0.0)));
    el.apply_style(style);
    let key = commit_element(&mut arena, Box::new(el));

    measure_and_place(
        &mut arena,
        key,
        LayoutConstraints {
            max_width: 100.0,
            max_height: 40.0,
            viewport_width: 100.0,
            viewport_height: 40.0,
            percent_base_width: Some(100.0),
            percent_base_height: Some(40.0),
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 100.0,
            available_height: 40.0,
            viewport_width: 100.0,
            viewport_height: 40.0,
            percent_base_width: Some(100.0),
            percent_base_height: Some(40.0),
        },
    );

    let el = crate::view::test_support::get_element::<Element>(&arena, key);
    assert!(el.layout_state.should_render);
    assert!(!el.core.should_paint);
}

#[test]
fn transformed_bounds_are_used_for_clip_culling() {
    let mut arena = new_test_arena();
    let mut el = Element::new(120.0, 0.0, 40.0, 40.0);
    let mut style = Style::new();
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(40.0)));
    style.insert(PropertyId::Height, ParsedValue::Length(Length::px(40.0)));
    style.set_transform(Transform::new([Translate::x(Length::px(-80.0))]));
    style.set_transform_origin(TransformOrigin::center());
    el.apply_style(style);
    let key = commit_element(&mut arena, Box::new(el));

    measure_and_place(
        &mut arena,
        key,
        LayoutConstraints {
            max_width: 200.0,
            max_height: 100.0,
            viewport_width: 200.0,
            viewport_height: 100.0,
            percent_base_width: Some(200.0),
            percent_base_height: Some(100.0),
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 100.0,
            available_height: 100.0,
            viewport_width: 200.0,
            viewport_height: 100.0,
            percent_base_width: Some(100.0),
            percent_base_height: Some(100.0),
        },
    );

    let el = crate::view::test_support::get_element::<Element>(&arena, key);
    let transformed = el.transformed_frame_bounding_rect(super::super::LayoutFrame {
        x: el.layout_state.layout_position.x,
        y: el.layout_state.layout_position.y,
        width: el.layout_state.layout_size.width,
        height: el.layout_state.layout_size.height,
    });
    assert!((transformed.x - 40.0).abs() < 0.01, "{transformed:?}");
    assert!((transformed.width - 40.0).abs() < 0.01, "{transformed:?}");
    assert!(
        el.layout_state.should_render,
        "translate 後的 bounding box 已進入 parent clip，不應被提前剔除"
    );
}

#[test]
fn transform_surface_bounds_ignore_finite_zero_area_child() {
    let mut arena = new_test_arena();
    let mut parent = Element::new(10.0, 20.0, 120.0, 80.0);
    parent.set_resolved_transform_for_test(Some(Mat4::IDENTITY));
    let parent_key = commit_element(&mut arena, Box::new(parent));
    let zero_width_text = Text::new(14.0, 28.0, 0.0, 16.0, "no paint coverage");
    commit_child(&mut arena, parent_key, Box::new(zero_width_text));

    let parent = crate::view::test_support::get_element::<Element>(&arena, parent_key);
    let expected = [10.0_f32, 20.0, 120.0, 80.0].map(f32::to_bits);
    let legacy = parent
        .legacy_transform_surface_bounds(&arena, [0.0, 0.0])
        .expect("zero-area child is an empty contribution, not a bounds failure");
    assert_eq!(
        [legacy.x, legacy.y, legacy.width, legacy.height].map(f32::to_bits),
        expected
    );
    let retained = parent
        .retained_transform_surface_bounds(&arena, [0.0, 0.0])
        .expect("retained transform bounds use the same empty contribution semantics");
    assert_eq!(
        [retained.x, retained.y, retained.width, retained.height,].map(f32::to_bits),
        expected
    );
}

#[test]
fn transparent_borderless_shadowless_element_does_not_paint_even_with_child() {
    let mut arena = new_test_arena();
    let parent = Element::new(0.0, 0.0, 120.0, 120.0);
    let parent_key = commit_element(&mut arena, Box::new(parent));

    let mut child = Element::new(0.0, 0.0, 60.0, 40.0);
    let mut child_style = Style::new();
    child_style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::hex("#ff0000")),
    );
    child.apply_style(child_style);
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

    let parent = crate::view::test_support::get_element::<Element>(&arena, parent_key);
    assert!(parent.layout_state.should_render);
    assert!(!parent.core.should_paint);
}

#[test]
fn zero_inner_area_sets_should_paint_false() {
    let mut arena = new_test_arena();
    let mut el = Element::new(0.0, 0.0, 20.0, 20.0);
    let mut style = Style::new();
    style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::hex("#112233")),
    );
    style.insert(
        PropertyId::PaddingLeft,
        ParsedValue::Length(Length::px(10.0)),
    );
    style.insert(
        PropertyId::PaddingRight,
        ParsedValue::Length(Length::px(10.0)),
    );
    style.insert(
        PropertyId::PaddingTop,
        ParsedValue::Length(Length::px(10.0)),
    );
    style.insert(
        PropertyId::PaddingBottom,
        ParsedValue::Length(Length::px(10.0)),
    );
    el.apply_style(style);
    let key = commit_element(&mut arena, Box::new(el));

    measure_and_place(
        &mut arena,
        key,
        LayoutConstraints {
            max_width: 20.0,
            max_height: 20.0,
            viewport_width: 20.0,
            viewport_height: 20.0,
            percent_base_width: Some(20.0),
            percent_base_height: Some(20.0),
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 20.0,
            available_height: 20.0,
            viewport_width: 20.0,
            viewport_height: 20.0,
            percent_base_width: Some(20.0),
            percent_base_height: Some(20.0),
        },
    );

    let el = crate::view::test_support::get_element::<Element>(&arena, key);
    assert_eq!(el.layout_state.layout_inner_size.width, 0.0);
    assert_eq!(el.layout_state.layout_inner_size.height, 0.0);
    assert!(el.layout_state.should_render);
    assert!(!el.core.should_paint);
}
