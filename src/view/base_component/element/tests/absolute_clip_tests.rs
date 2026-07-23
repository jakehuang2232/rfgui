use super::*;

#[test]
fn absolute_clip_viewport_allows_render_outside_parent_bounds() {
    let parent = Element::new(0.0, 0.0, 100.0, 80.0);
    let mut child = Element::new(0.0, 0.0, 30.0, 20.0);
    let mut child_style = Style::new();
    child_style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::hex("#ff0000")),
    );
    child_style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .left(Length::px(130.0))
                .top(Length::px(10.0))
                .clip(ClipMode::Viewport),
        ),
    );
    child.apply_style(child_style);

    let mut arena = new_test_arena();
    let parent_key = commit_element(&mut arena, Box::new(parent));
    let child_k = commit_child(&mut arena, parent_key, Box::new(child));

    measure_and_place(
        &mut arena,
        parent_key,
        LayoutConstraints {
            max_width: 400.0,
            max_height: 300.0,
            viewport_width: 400.0,
            percent_base_width: Some(400.0),
            percent_base_height: Some(300.0),
            viewport_height: 300.0,
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 400.0,
            available_height: 300.0,
            viewport_width: 400.0,
            percent_base_width: Some(400.0),
            percent_base_height: Some(300.0),
            viewport_height: 300.0,
        },
    );

    let rendered = crate::view::test_support::get_element::<Element>(&arena, child_k)
        .layout_state
        .should_render;
    assert!(rendered);
}

#[test]
fn viewport_clipped_absolute_descendant_is_deferred_even_if_parent_is_not_rendered() {
    let mut parent = Element::new(0.0, 0.0, 100.0, 80.0);
    let mut child = Element::new(0.0, 0.0, 30.0, 20.0);
    let mut child_style = Style::new();
    child_style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .left(Length::px(10.0))
                .top(Length::px(10.0))
                .clip(ClipMode::Viewport),
        ),
    );
    child.apply_style(child_style);
    parent.layout_state.should_render = false;

    let mut arena = new_test_arena();
    let parent_key = commit_element(&mut arena, Box::new(parent));
    let child_k = commit_child(&mut arena, parent_key, Box::new(child));

    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(400, 300, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let target = ctx.allocate_target(&mut graph);
    ctx.set_current_target(target);
    // Mirror `Viewport::render_rsx`: seed the ctx defer list once
    // from the arena.
    let mut popup_stack = crate::view::popup_stack::PopupStack::new();
    arena.seed_defer_render_with_stack(&mut popup_stack, &mut ctx);
    let ctx_for_build = UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone());
    let next_state = arena
        .with_element_taken(parent_key, |el, a| el.build(&mut graph, a, ctx_for_build))
        .expect("parent build returns state");
    ctx.set_state(next_state);

    let deferred = drain_deferred(&mut ctx);
    let child_id = arena.get(child_k).unwrap().element.stable_id();
    assert_eq!(
        deferred
            .iter()
            .filter(|node| node.key == child_k && node.stable_id == child_id)
            .count(),
        1,
        "canonical pre-seed and build-time registration must deduplicate by NodeKey"
    );
}

#[test]
fn absolute_clip_anchor_parent_falls_back_to_grandparent_without_anchor() {
    let parent = Element::new(0.0, 0.0, 100.0, 80.0);
    let mut child = Element::new(0.0, 0.0, 30.0, 20.0);
    let mut child_style = Style::new();
    child_style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .left(Length::px(130.0))
                .top(Length::px(10.0))
                .clip(ClipMode::AnchorParent),
        ),
    );
    child.apply_style(child_style);

    let mut arena = new_test_arena();
    let parent_key = commit_element(&mut arena, Box::new(parent));
    let child_k = commit_child(&mut arena, parent_key, Box::new(child));

    measure_and_place(
        &mut arena,
        parent_key,
        LayoutConstraints {
            max_width: 400.0,
            max_height: 300.0,
            viewport_width: 400.0,
            percent_base_width: Some(400.0),
            percent_base_height: Some(300.0),
            viewport_height: 300.0,
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 400.0,
            available_height: 300.0,
            viewport_width: 400.0,
            percent_base_width: Some(400.0),
            percent_base_height: Some(300.0),
            viewport_height: 300.0,
        },
    );

    // AnchorParent without explicit anchor uses grandparent (= proposal/viewport
    // 400x300) as the clip rect. Child at x=130, y=10, size 30x20 fits inside
    // the grandparent clip even though it overflows the immediate parent (100x80).
    let rendered = crate::view::test_support::get_element::<Element>(&arena, child_k)
        .layout_state
        .should_render;
    assert!(rendered);
}

#[test]
fn absolute_clip_anchor_parent_uses_anchor_parent_bounds() {
    let parent = Element::new(0.0, 0.0, 500.0, 200.0);
    let mut anchor = Element::new(300.0, 20.0, 40.0, 40.0);
    anchor.set_anchor_name(Some(AnchorName::new("menu_button")));

    let mut child = Element::new(0.0, 0.0, 30.0, 20.0);
    let mut child_style = Style::new();
    child_style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::hex("#ff0000")),
    );
    child_style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .anchor("menu_button")
                .left(Length::px(50.0))
                .top(Length::px(0.0))
                .clip(ClipMode::AnchorParent),
        ),
    );
    child.apply_style(child_style);

    let mut arena = new_test_arena();
    let parent_key = commit_element(&mut arena, Box::new(parent));
    let _anchor_k = commit_child(&mut arena, parent_key, Box::new(anchor));
    let child_k = commit_child(&mut arena, parent_key, Box::new(child));

    measure_and_place(
        &mut arena,
        parent_key,
        LayoutConstraints {
            max_width: 600.0,
            max_height: 300.0,
            viewport_width: 600.0,
            percent_base_width: Some(600.0),
            percent_base_height: Some(300.0),
            viewport_height: 300.0,
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 600.0,
            available_height: 300.0,
            viewport_width: 600.0,
            percent_base_width: Some(600.0),
            percent_base_height: Some(300.0),
            viewport_height: 300.0,
        },
    );

    let rendered = crate::view::test_support::get_element::<Element>(&arena, child_k)
        .layout_state
        .should_render;
    assert!(rendered);
}

#[test]
fn absolute_clip_anchor_parent_scissor_uses_anchor_parent_bounds() {
    let parent = Element::new(0.0, 0.0, 500.0, 200.0);
    let mut anchor = Element::new(300.0, 20.0, 40.0, 40.0);
    anchor.set_anchor_name(Some(AnchorName::new("menu_button")));

    let mut child = Element::new(0.0, 0.0, 150.0, 22.0);
    let mut child_style = Style::new();
    child_style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .anchor("menu_button")
                .left(Length::px(38.0))
                .top(Length::px(0.0))
                .clip(ClipMode::AnchorParent),
        ),
    );
    child.apply_style(child_style);

    let mut arena = new_test_arena();
    let parent_key = commit_element(&mut arena, Box::new(parent));
    let _anchor_k = commit_child(&mut arena, parent_key, Box::new(anchor));
    let child_k = commit_child(&mut arena, parent_key, Box::new(child));

    measure_and_place(
        &mut arena,
        parent_key,
        LayoutConstraints {
            max_width: 600.0,
            max_height: 300.0,
            viewport_width: 600.0,
            percent_base_width: Some(600.0),
            percent_base_height: Some(300.0),
            viewport_height: 300.0,
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 600.0,
            available_height: 300.0,
            viewport_width: 600.0,
            percent_base_width: Some(600.0),
            percent_base_height: Some(300.0),
            viewport_height: 300.0,
        },
    );

    let child_el = crate::view::test_support::get_element::<Element>(&arena, child_k);
    assert_eq!(
        child_el.absolute_clip_scissor_rect(),
        Some([0, 0, 500, 200])
    );
}

#[test]
fn absolute_clip_anchor_parent_scissor_falls_back_to_grandparent_without_anchor() {
    let parent = Element::new(0.0, 0.0, 100.0, 80.0);
    let mut child = Element::new(0.0, 0.0, 30.0, 20.0);
    let mut child_style = Style::new();
    child_style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .left(Length::px(130.0))
                .top(Length::px(10.0))
                .clip(ClipMode::AnchorParent),
        ),
    );
    child.apply_style(child_style);

    let mut arena = new_test_arena();
    let parent_key = commit_element(&mut arena, Box::new(parent));
    let child_k = commit_child(&mut arena, parent_key, Box::new(child));

    measure_and_place(
        &mut arena,
        parent_key,
        LayoutConstraints {
            max_width: 400.0,
            max_height: 300.0,
            viewport_width: 400.0,
            percent_base_width: Some(400.0),
            percent_base_height: Some(300.0),
            viewport_height: 300.0,
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 400.0,
            available_height: 300.0,
            viewport_width: 400.0,
            percent_base_width: Some(400.0),
            percent_base_height: Some(300.0),
            viewport_height: 300.0,
        },
    );

    // AnchorParent without anchor → grandparent's clip. Root parent's
    // grandparent clip falls back to the proposal viewport (400x300).
    let child_el = crate::view::test_support::get_element::<Element>(&arena, child_k);
    assert_eq!(
        child_el.absolute_clip_scissor_rect(),
        Some([0, 0, 400, 300])
    );
}
