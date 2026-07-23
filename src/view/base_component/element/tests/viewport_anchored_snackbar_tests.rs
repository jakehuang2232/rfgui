use super::*;

/// Repro for the user's video bug: when a tree-ancestor's inner area
/// lies entirely outside the viewport (e.g. a Window dragged so its
/// content area sits below viewport), a viewport-anchored
/// `clip:Viewport` descendant gets dropped from the deferred list and
/// never rendered. The ancestor itself still passes
/// `should_render` (its frame intersects viewport at the top edge),
/// but `has_visible_inner_render_area` returns false because the
/// inner rect's intersection with the current scissor is empty —
/// the overflow loop is skipped and the descendant is never appended
/// via `register_deferred`.
#[test]
fn viewport_anchored_descendant_collected_when_ancestor_inner_below_viewport() {
    // Window: clip:Viewport, top in viewport, content stretches below.
    let mut window = Element::new(0.0, 0.0, 460.0, 1500.0);
    let mut window_style = Style::new();
    window_style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .left(Length::px(100.0))
                .top(Length::px(700.0))
                .clip(ClipMode::Viewport),
        ),
    );
    window.apply_style(window_style);

    // Section: positioned far down inside Window so its frame sits
    // entirely below viewport y=800.
    let section = Element::new(0.0, 1000.0, 460.0, 200.0);

    // Snackbar wrapper: viewport-anchored + clip:Viewport, bottom-left.
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
    let section_k = commit_child(&mut arena, win_k, Box::new(section));
    let snackbar_k = commit_child(&mut arena, section_k, Box::new(snackbar));
    let snackbar_id = arena.get(snackbar_k).unwrap().element.stable_id();

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

    // Sanity: snackbar layout still anchored to viewport.
    let snap = child_snapshot(&arena, snackbar_k);
    assert!(
        (snap.x - 16.0).abs() < 0.5 && (snap.y - 744.0).abs() < 0.5,
        "snackbar should still be anchored to viewport, got ({}, {})",
        snap.x,
        snap.y
    );

    // Build via FrameGraph.
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

    let deferred = drain_deferred(&mut ctx);
    assert!(
        deferred.iter().any(|node| node.stable_id == snackbar_id),
        "BUG: snackbar should be in deferred list when ancestor inner is below viewport, got {:?}",
        deferred
    );
}

/// Closer repro of the user's video: Window with column flow content,
/// multiple sections in the column, snackbar nested inside one of the
/// later sections (Accordion-style). Window dragged down so the
/// section that holds the snackbar sits entirely below viewport.
/// Expected: snackbar (viewport-anchored) still rendered.
#[test]
fn viewport_anchored_snackbar_through_flow_column_below_viewport() {
    // Window outer at y=700, height=1500 (extends well below viewport).
    let mut window = Element::new(0.0, 0.0, 460.0, 1500.0);
    let mut window_style = Style::new();
    window_style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .left(Length::px(100.0))
                .top(Length::px(700.0))
                .clip(ClipMode::Viewport),
        ),
    );
    window_style.insert(
        PropertyId::Layout,
        ParsedValue::Layout(Layout::flow().column().no_wrap().into()),
    );
    window.apply_style(window_style);

    // 3 sections in the column (each 200 tall). With Window at y=700,
    // section heights: 200/200/200 → bottoms at 900/1100/1300 — all
    // below viewport=800.
    let section1 = Element::new(0.0, 0.0, 460.0, 200.0);
    let section2 = Element::new(0.0, 0.0, 460.0, 200.0);
    let section3 = Element::new(0.0, 0.0, 460.0, 200.0);

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
    let _s1_k = commit_child(&mut arena, win_k, Box::new(section1));
    let _s2_k = commit_child(&mut arena, win_k, Box::new(section2));
    let s3_k = commit_child(&mut arena, win_k, Box::new(section3));
    let snackbar_k = commit_child(&mut arena, s3_k, Box::new(snackbar));
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

    // Section 3 should be below viewport (its parent Window is at y=700,
    // section3 follows after section1+section2 = +400 → y=1100).
    let s3_snap = child_snapshot(&arena, s3_k);
    eprintln!(
        "[s3] x={} y={} w={} h={} should_render? -- need internal access",
        s3_snap.x, s3_snap.y, s3_snap.width, s3_snap.height
    );

    let snap = child_snapshot(&arena, snackbar_k);
    eprintln!(
        "[snackbar] x={} y={} w={} h={}",
        snap.x, snap.y, snap.width, snap.height
    );

    // Snackbar must still anchor to viewport.
    assert!(
        (snap.x - 16.0).abs() < 0.5 && (snap.y - 744.0).abs() < 0.5,
        "snackbar anchored to viewport, got ({}, {})",
        snap.x,
        snap.y
    );

    // Check render path: build then defer.
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

    let deferred = drain_deferred(&mut ctx);
    eprintln!("[deferred ids] {:?}", deferred);
    eprintln!("[snackbar id] {}", snackbar_id);
    assert!(
        deferred.iter().any(|node| node.stable_id == snackbar_id),
        "BUG: snackbar must be deferred when its tree-ancestor section is below viewport"
    );
}

/// Even deeper: viewport-clip element nested 4+ levels under a section
/// that's below viewport (mimics `Window > content > Section >
/// Accordion > AccordionContent > Snackbar wrapper`). Each intermediate
/// ancestor's visibility gate fails because its inner is below viewport,
/// so we need RECURSIVE defer collection (collect_root walks subtree).
#[test]
fn viewport_anchored_snackbar_deeply_nested_under_offscreen_section() {
    // Window outer.
    let mut window = Element::new(0.0, 0.0, 460.0, 1500.0);
    let mut window_style = Style::new();
    window_style.insert(
        PropertyId::Position,
        ParsedValue::Position(
            Position::absolute()
                .left(Length::px(100.0))
                .top(Length::px(700.0))
                .clip(ClipMode::Viewport),
        ),
    );
    window_style.insert(
        PropertyId::Layout,
        ParsedValue::Layout(Layout::flow().column().no_wrap().into()),
    );
    window.apply_style(window_style);

    // Section1 (placeholder, fills first 200).
    let section1 = Element::new(0.0, 0.0, 460.0, 200.0);
    // Section2 = Snackbar Section (after section1, so y=900, well below
    // viewport=800).
    let section2 = Element::new(0.0, 0.0, 460.0, 200.0);
    // Inside section2: Accordion wrapper (~190 tall).
    let accordion = Element::new(0.0, 0.0, 460.0, 190.0);
    // Accordion content (after header).
    let accordion_content = Element::new(0.0, 0.0, 460.0, 150.0);

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
    let _s1_k = commit_child(&mut arena, win_k, Box::new(section1));
    let s2_k = commit_child(&mut arena, win_k, Box::new(section2));
    let acc_k = commit_child(&mut arena, s2_k, Box::new(accordion));
    let acc_content_k = commit_child(&mut arena, acc_k, Box::new(accordion_content));
    let snackbar_k = commit_child(&mut arena, acc_content_k, Box::new(snackbar));
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

    let snap = child_snapshot(&arena, snackbar_k);
    assert!(
        (snap.x - 16.0).abs() < 0.5 && (snap.y - 744.0).abs() < 0.5,
        "snackbar still anchored to viewport, got ({}, {})",
        snap.x,
        snap.y
    );

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

    let deferred = drain_deferred(&mut ctx);
    assert!(
        deferred.iter().any(|node| node.stable_id == snackbar_id),
        "BUG: deeply nested snackbar must still be deferred. defer={:?} snackbar_id={}",
        deferred,
        snackbar_id
    );
}

// ---- inline-baseline Sprint 1 plumbing tests ----
//
// Per `docs/design/inline-baseline.md` Sprint 1 acceptance: every
// inline fragment must surface a non-trivial `baseline` value.
// Tests cover all four producer paths.

// ---- Sprint 3: D3 vertical-align offset formula ----
