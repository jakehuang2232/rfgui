use super::*;

/// Repro for the user-reported bug: when an ancestor extends beyond the
/// viewport, an `absolute + Anchor::Viewport + clip:Viewport` descendant
/// (e.g. a snackbar pinned to the viewport bottom) gets clipped /
/// culled by the offscreen ancestor. Expected: the descendant should
/// render at its viewport-anchored position, full viewport scissor,
/// `should_render = true`, regardless of ancestor geometry.
#[test]
fn viewport_anchored_child_renders_when_ancestor_partly_offscreen() {
    // Window-like parent: clip:Viewport, dragged so left half is
    // offscreen — frame at (-200, 100), size 460x380.
    let mut parent = Element::new(0.0, 0.0, 460.0, 380.0);
    let mut parent_style = Style::new();
    parent_style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .left(Length::px(-200.0))
                .top(Length::px(100.0))
                .clip(ClipMode::Viewport),
        ),
    );
    parent.apply_style(parent_style);

    // Snackbar-like child: absolute, Anchor::Viewport, clip:Viewport,
    // bottom=16 left=16 right=16 (spans width minus gaps), height=40.
    let mut child = Element::new(0.0, 0.0, 0.0, 40.0);
    let mut child_style = Style::new();
    child_style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .anchor(crate::style::Anchor::Viewport)
                .left(Length::px(16.0))
                .right(Length::px(16.0))
                .bottom(Length::px(16.0))
                .clip(ClipMode::Viewport),
        ),
    );
    child.apply_style(child_style);

    let mut arena = new_test_arena();
    let parent_key = commit_element(&mut arena, Box::new(parent));
    let child_k = commit_child(&mut arena, parent_key, Box::new(child));

    // Viewport 1280x800.
    measure_and_place(
        &mut arena,
        parent_key,
        LayoutConstraints {
            max_width: 1280.0,
            max_height: 800.0,
            viewport_width: 1280.0,
            viewport_height: 800.0,
            percent_base_width: Some(1280.0),
            percent_base_height: Some(800.0),
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 1280.0,
            available_height: 800.0,
            viewport_width: 1280.0,
            viewport_height: 800.0,
            percent_base_width: Some(1280.0),
            percent_base_height: Some(800.0),
        },
    );

    // Expected: child anchored to viewport, NOT to parent.
    // x = 16 (viewport left + 16), width = 1280 - 16 - 16 = 1248.
    // y = 800 - 16 - 40 = 744.
    let snap = child_snapshot(&arena, child_k);
    eprintln!(
        "[viewport-anchored snap] x={} y={} w={} h={}",
        snap.x, snap.y, snap.width, snap.height
    );
    assert!(
        (snap.x - 16.0).abs() < 0.5,
        "child x should pin to viewport+16, got {}",
        snap.x
    );
    assert!(
        (snap.y - 744.0).abs() < 0.5,
        "child y should pin to viewport bottom-16-40, got {}",
        snap.y
    );
    assert!(
        (snap.width - 1248.0).abs() < 0.5,
        "child width should span viewport minus 2*16, got {}",
        snap.width
    );

    // Should render — frame is fully inside viewport.
    let child_el = crate::view::test_support::get_element::<Element>(&arena, child_k);
    assert!(
        child_el.layout_state.should_render,
        "viewport-anchored child should render despite ancestor offscreen"
    );

    // absolute_clip_rect should be the full viewport rect (escape clip).
    let abs_clip = child_el
        .absolute_clip_rect
        .expect("clip_rect set for absolute");
    assert!(
        (abs_clip.x - 0.0).abs() < 0.01
            && (abs_clip.y - 0.0).abs() < 0.01
            && (abs_clip.width - 1280.0).abs() < 0.01
            && (abs_clip.height - 800.0).abs() < 0.01,
        "absolute_clip_rect should be viewport rect, got {:?}",
        abs_clip
    );
}

/// Deeper repro: ancestor chain (Window > content > section > snackbar)
/// where Window is dragged so most of it sits offscreen. Verify the
/// viewport-anchored snackbar still computes correct viewport-aligned
/// frame and `should_render`.
#[test]
fn viewport_anchored_child_through_deep_offscreen_chain() {
    // Window outer: clip:Viewport, position absolute at (-300, 50),
    // size 460x380.
    let mut window = Element::new(0.0, 0.0, 460.0, 380.0);
    let mut window_style = Style::new();
    window_style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .left(Length::px(-300.0))
                .top(Length::px(50.0))
                .clip(ClipMode::Viewport),
        ),
    );
    window.apply_style(window_style);

    // Window content (column flow, 100% width/height of window, with
    // padding to mimic the title bar etc).
    let mut content = Element::new(0.0, 0.0, 460.0, 350.0);
    let mut content_style = Style::new();
    content_style.insert(
        PropertyId::Layout,
        ParsedValue::Layout(Layout::flow().column().no_wrap().into()),
    );
    content.apply_style(content_style);

    // Section wrapper inside content.
    let section = Element::new(0.0, 0.0, 460.0, 200.0);

    // Snackbar wrapper: absolute, Anchor::Viewport, clip:Viewport.
    let mut snackbar = Element::new(0.0, 0.0, 0.0, 40.0);
    let mut snackbar_style = Style::new();
    snackbar_style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .anchor(crate::style::Anchor::Viewport)
                .left(Length::px(16.0))
                .right(Length::px(16.0))
                .bottom(Length::px(16.0))
                .clip(ClipMode::Viewport),
        ),
    );
    snackbar.apply_style(snackbar_style);

    let mut arena = new_test_arena();
    let win_k = commit_element(&mut arena, Box::new(window));
    let content_k = commit_child(&mut arena, win_k, Box::new(content));
    let section_k = commit_child(&mut arena, content_k, Box::new(section));
    let snackbar_k = commit_child(&mut arena, section_k, Box::new(snackbar));

    // Viewport 1280x800.
    measure_and_place(
        &mut arena,
        win_k,
        LayoutConstraints {
            max_width: 1280.0,
            max_height: 800.0,
            viewport_width: 1280.0,
            viewport_height: 800.0,
            percent_base_width: Some(1280.0),
            percent_base_height: Some(800.0),
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 1280.0,
            available_height: 800.0,
            viewport_width: 1280.0,
            viewport_height: 800.0,
            percent_base_width: Some(1280.0),
            percent_base_height: Some(800.0),
        },
    );

    let snap = child_snapshot(&arena, snackbar_k);
    assert!(
        (snap.x - 16.0).abs() < 0.5,
        "deep child x should pin to viewport, got {}",
        snap.x
    );
    assert!(
        (snap.y - 744.0).abs() < 0.5,
        "deep child y should pin to viewport bottom-16-40, got {}",
        snap.y
    );

    let snackbar_el = crate::view::test_support::get_element::<Element>(&arena, snackbar_k);
    assert!(
        snackbar_el.layout_state.should_render,
        "deep viewport-anchored child should render"
    );
}

/// Render-level repro: drive Element::build through the deep-offscreen
/// ancestor chain, then run the deferred build for the snackbar's
/// node id. Inspect the recorded FrameGraph pass count to confirm the
/// snackbar actually emitted draw passes.
#[test]
fn viewport_anchored_child_renders_passes_through_offscreen_ancestor() {
    // Same scene as the layout-only test, but exercise the build/defer
    // pipeline.
    let mut window = Element::new(0.0, 0.0, 460.0, 380.0);
    let mut window_style = Style::new();
    window_style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::hex("#222222")),
    );
    window_style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .left(Length::px(-300.0))
                .top(Length::px(50.0))
                .clip(ClipMode::Viewport),
        ),
    );
    window.apply_style(window_style);

    let mut snackbar = Element::new(0.0, 0.0, 0.0, 40.0);
    let mut snackbar_style = Style::new();
    snackbar_style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::hex("#ff0000")),
    );
    snackbar_style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .anchor(crate::style::Anchor::Viewport)
                .left(Length::px(16.0))
                .right(Length::px(16.0))
                .bottom(Length::px(16.0))
                .clip(ClipMode::Viewport),
        ),
    );
    snackbar.apply_style(snackbar_style);

    let mut arena = new_test_arena();
    let win_k = commit_element(&mut arena, Box::new(window));
    let snackbar_k = commit_child(&mut arena, win_k, Box::new(snackbar));
    let snackbar_id = arena.get(snackbar_k).unwrap().element.stable_id();

    measure_and_place(
        &mut arena,
        win_k,
        LayoutConstraints {
            max_width: 1280.0,
            max_height: 800.0,
            viewport_width: 1280.0,
            viewport_height: 800.0,
            percent_base_width: Some(1280.0),
            percent_base_height: Some(800.0),
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 1280.0,
            available_height: 800.0,
            viewport_width: 1280.0,
            viewport_height: 800.0,
            percent_base_width: Some(1280.0),
            percent_base_height: Some(800.0),
        },
    );

    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(1280, 800, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let target = ctx.allocate_target(&mut graph);
    ctx.set_current_target(target);

    // Main walk.
    let ctx_for_build = UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone());
    let next_state = arena
        .with_element_taken(win_k, |el, a| el.build(&mut graph, a, ctx_for_build))
        .expect("window build returns state");
    ctx.set_state(next_state);

    let pass_count_before_defer = graph.pass_descriptors().len();

    // Defer pass.
    let deferred = drain_deferred(&mut ctx);
    assert!(
        deferred.iter().any(|node| node.stable_id == snackbar_id),
        "snackbar should be in deferred list, got {:?}",
        deferred
    );
    for node in &deferred {
        crate::view::base_component::build_node_by_key(
            node.key,
            node.stable_id,
            &mut graph,
            &mut arena,
            &mut ctx,
        );
    }

    let pass_count_after_defer = graph.pass_descriptors().len();
    assert!(
        pass_count_after_defer > pass_count_before_defer,
        "deferred snackbar should emit at least one render pass (before={}, after={})",
        pass_count_before_defer,
        pass_count_after_defer
    );
}

/// Even when ancestor is FULLY offscreen (intersects viewport = false),
/// a viewport-anchored descendant must still render.
#[test]
fn viewport_anchored_child_renders_when_ancestor_fully_offscreen() {
    let mut window = Element::new(0.0, 0.0, 460.0, 380.0);
    let mut window_style = Style::new();
    window_style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .left(Length::px(-2000.0)) // way off the left edge
                .top(Length::px(-2000.0))
                .clip(ClipMode::Viewport),
        ),
    );
    window.apply_style(window_style);

    let mut snackbar = Element::new(0.0, 0.0, 0.0, 40.0);
    let mut snackbar_style = Style::new();
    snackbar_style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::hex("#00ff00")),
    );
    snackbar_style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .anchor(crate::style::Anchor::Viewport)
                .left(Length::px(16.0))
                .right(Length::px(16.0))
                .bottom(Length::px(16.0))
                .clip(ClipMode::Viewport),
        ),
    );
    snackbar.apply_style(snackbar_style);

    let mut arena = new_test_arena();
    let win_k = commit_element(&mut arena, Box::new(window));
    let snackbar_k = commit_child(&mut arena, win_k, Box::new(snackbar));
    let snackbar_id = arena.get(snackbar_k).unwrap().element.stable_id();

    measure_and_place(
        &mut arena,
        win_k,
        LayoutConstraints {
            max_width: 1280.0,
            max_height: 800.0,
            viewport_width: 1280.0,
            viewport_height: 800.0,
            percent_base_width: Some(1280.0),
            percent_base_height: Some(800.0),
        },
        LayoutPlacement {
            parent_x: 0.0,
            parent_y: 0.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 1280.0,
            available_height: 800.0,
            viewport_width: 1280.0,
            viewport_height: 800.0,
            percent_base_width: Some(1280.0),
            percent_base_height: Some(800.0),
        },
    );

    // Window should NOT render (fully offscreen).
    let window_el = crate::view::test_support::get_element::<Element>(&arena, win_k);
    assert!(
        !window_el.layout_state.should_render,
        "fully offscreen window should NOT render"
    );
    drop(window_el);

    // Snackbar should still render — it's anchored to viewport.
    let snackbar_el = crate::view::test_support::get_element::<Element>(&arena, snackbar_k);
    assert!(
        snackbar_el.layout_state.should_render,
        "viewport-anchored snackbar should render even when window is fully offscreen"
    );
    drop(snackbar_el);

    let mut graph = FrameGraph::new();
    let mut ctx = UiBuildContext::new(1280, 800, wgpu::TextureFormat::Bgra8Unorm, 1.0);
    let target = ctx.allocate_target(&mut graph);
    ctx.set_current_target(target);
    // Mirror `Viewport::render_rsx`: seed the ctx defer list once
    // from the arena.
    let mut popup_stack = crate::view::popup_stack::PopupStack::new();
    arena.seed_defer_render_with_stack(&mut popup_stack, &mut ctx);

    let ctx_for_build = UiBuildContext::from_parts(ctx.viewport(), ctx.state_clone());
    let next_state = arena
        .with_element_taken(win_k, |el, a| el.build(&mut graph, a, ctx_for_build))
        .expect("window build returns state");
    ctx.set_state(next_state);

    // Window's build with should_render=false should still collect
    // viewport-anchored descendants into the deferred list.
    let deferred = drain_deferred(&mut ctx);
    assert!(
        deferred.iter().any(|node| node.stable_id == snackbar_id),
        "snackbar should be deferred even when window not rendered, got {:?}",
        deferred
    );

    let pass_count_before_defer = graph.pass_descriptors().len();
    for node in &deferred {
        crate::view::base_component::build_node_by_key(
            node.key,
            node.stable_id,
            &mut graph,
            &mut arena,
            &mut ctx,
        );
    }
    let pass_count_after_defer = graph.pass_descriptors().len();
    assert!(
        pass_count_after_defer > pass_count_before_defer,
        "snackbar should emit passes even when ancestor is offscreen (before={}, after={})",
        pass_count_before_defer,
        pass_count_after_defer
    );
}
