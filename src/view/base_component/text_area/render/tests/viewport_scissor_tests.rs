use super::*;

#[test]
fn legacy_and_typed_viewport_scissor_preserve_explicit_empty() {
    assert_eq!(
        rect_to_logical_scissor_rect(Rect {
            x: 12.0,
            y: 18.0,
            width: 0.0,
            height: 20.0,
        }),
        [12, 18, 0, 20]
    );
    assert_eq!(
        rect_to_logical_scissor_rect(Rect {
            x: -5.0,
            y: -7.0,
            width: -10.0,
            height: -20.0,
        }),
        [0, 0, 0, 0]
    );

    let mut text_area = TextArea::new();
    text_area.layout_state.layout_position =
        crate::view::base_component::Position { x: 12.0, y: 18.0 };
    text_area.viewport_size = crate::view::base_component::Size {
        width: 0.0,
        height: 20.0,
    };
    assert_eq!(text_area.viewport_logical_scissor_rect(), [12, 18, 0, 20]);
    assert_eq!(text_area.viewport_scissor_rect(), Some([12, 18, 0, 20]));
    assert_eq!(
        ElementTrait::contents_logical_scissor(&text_area),
        Some([12, 18, 0, 20]),
        "legacy and typed clip authorities must agree on explicit empty",
    );
}

#[test]
fn viewport_scissor_uses_text_area_viewport_not_content_width() {
    let mut text_area = TextArea::new();
    text_area.layout_state.layout_position =
        crate::view::base_component::Position { x: 10.2, y: 20.6 };
    text_area.viewport_size = crate::view::base_component::Size {
        width: 120.1,
        height: 40.2,
    };
    text_area.layout_state.content_size = crate::view::base_component::Size {
        width: 360.0,
        height: 90.0,
    };

    assert_eq!(text_area.viewport_scissor_rect(), Some([10, 20, 121, 41]));
}

#[test]
fn text_area_build_restores_viewport_scissor() {
    let (mut arena, root) = projection_fixture(3, true);
    crate::view::test_support::measure_and_place(
        &mut arena,
        root,
        LayoutConstraints {
            max_width: 90.0,
            max_height: 36.0,
            viewport_width: 90.0,
            viewport_height: 36.0,
            percent_base_width: Some(90.0),
            percent_base_height: Some(36.0),
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 90.0,
            available_height: 36.0,
            viewport_width: 90.0,
            viewport_height: 36.0,
            percent_base_width: Some(90.0),
            percent_base_height: Some(36.0),
        },
    );

    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(160, 120, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let target = ctx.allocate_target(&mut graph);
    ctx.set_current_target(target);
    ctx.replace_scissor_rect(Some([5, 6, 70, 30]));
    let ctx_for_build = UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone());
    let next_state = arena
        .with_element_taken(root, |el, a| el.build(&mut graph, a, ctx_for_build))
        .expect("TextArea build returns state");
    ctx.set_state(next_state);

    assert_eq!(
        ctx.graphics_pass_context().scissor_rect,
        Some([5, 6, 70, 30]),
        "TextArea viewport scissor must restore the exact ancestor authority",
    );
}
