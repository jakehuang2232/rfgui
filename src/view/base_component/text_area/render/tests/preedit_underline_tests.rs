use super::*;

#[test]
fn preedit_underline_uses_middle_empty_paragraph_run() {
    let (arena, root) = plain_preedit_fixture("a\n\nb", 2);

    let rects = arena
        .with_element_taken_ref(root, |el, arena| {
            el.as_any()
                .downcast_ref::<TextArea>()
                .expect("TextArea root")
                .preedit_underline_screen_rects(arena)
        })
        .expect("root exists");

    assert!(!rects.is_empty(), "expected empty-line IME underline");
    assert!(
        rects
            .iter()
            .all(|rect| rect.height == 1.0 && rect.width >= 1.0),
        "IME underline should be visible 1px strokes: {rects:?}"
    );
}

#[test]
fn preedit_underline_uses_trailing_empty_paragraph_run() {
    let (arena, root) = plain_preedit_fixture("a\n", 2);

    let rects = arena
        .with_element_taken_ref(root, |el, arena| {
            el.as_any()
                .downcast_ref::<TextArea>()
                .expect("TextArea root")
                .preedit_underline_screen_rects(arena)
        })
        .expect("root exists");

    assert!(
        !rects.is_empty(),
        "expected trailing empty-line IME underline"
    );
    assert!(
        rects
            .iter()
            .all(|rect| rect.height == 1.0 && rect.width >= 1.0),
        "IME underline should be visible 1px strokes: {rects:?}"
    );
}

#[test]
fn soft_wrap_tail_preedit_uses_current_line_when_space_allows() {
    use super::super::super::caret_map::CaretAffinity;

    let content = "the quick brown fox jumps over the lazy dog";
    let width = 80.0;
    let (base_arena, base_root) = wrapped_plain_fixture(content, width);
    let (upper_tail, lower_head) = consumed_soft_wrap_slots(&base_arena, base_root);
    let cursor = upper_tail;
    let (upper_y, lower_y) = base_arena
        .with_element_taken_ref(base_root, |el, arena| {
            let text_area = el
                .as_any()
                .downcast_ref::<TextArea>()
                .expect("TextArea root");
            let map = super::super::super::caret_map::CaretNavigationMap::build(text_area, arena);
            let upper = map
                .caret_stop_for_char(upper_tail, CaretAffinity::Upstream)
                .expect("upstream upper-tail stop");
            let lower = map
                .caret_stop_for_char(lower_head, CaretAffinity::Downstream)
                .expect("downstream lower-head stop");
            (upper.y_top, lower.y_top)
        })
        .expect("root exists");
    assert!(
        upper_y < lower_y,
        "fixture boundary must span two visual lines"
    );
    let midpoint = (upper_y + lower_y) * 0.5;

    let (up_arena, up_root) = plain_preedit_fixture_with_options(
        content,
        cursor,
        ".",
        Some((".".len(), ".".len())),
        CaretAffinity::Upstream,
        width,
    );
    let (_, up_caret_y, _) = caret_position(&up_arena, up_root);
    let up_fragments = run_text_pass_fragments(&up_arena, up_root);
    let up_rects = up_arena
        .with_element_taken_ref(up_root, |el, arena| {
            el.as_any()
                .downcast_ref::<TextArea>()
                .expect("TextArea root")
                .preedit_underline_screen_rects(arena)
        })
        .expect("root exists");
    assert!(
        up_caret_y < midpoint,
        "upstream preedit caret should stay on upper visual line: caret_y={up_caret_y}, upper_y={upper_y}, lower_y={lower_y}"
    );
    assert!(
        up_fragments
            .iter()
            .any(|(content, rect)| content.contains('.') && rect.y < midpoint),
        "upstream preedit glyph should be painted on upper visual line: fragments={up_fragments:?}, upper_y={upper_y}, lower_y={lower_y}"
    );
    assert!(
        up_rects.iter().any(|rect| rect.y < lower_y),
        "upstream preedit underline should start on upper visual line: rects={up_rects:?}, upper_y={upper_y}, lower_y={lower_y}"
    );

    let (down_arena, down_root) = plain_preedit_fixture_with_options(
        content,
        cursor,
        ".",
        Some((".".len(), ".".len())),
        CaretAffinity::Downstream,
        width,
    );
    let (_, down_caret_y, _) = caret_position(&down_arena, down_root);
    let down_fragments = run_text_pass_fragments(&down_arena, down_root);
    assert!(
        down_caret_y < midpoint,
        "preedit should stay on current line when there is enough remaining space even with downstream affinity: caret_y={down_caret_y}, upper_y={upper_y}, lower_y={lower_y}"
    );
    assert!(
        down_fragments
            .iter()
            .any(|(content, rect)| content.contains('.') && rect.y < midpoint),
        "preedit glyph should be painted on current line when there is enough remaining space: fragments={down_fragments:?}, upper_y={upper_y}, lower_y={lower_y}"
    );
}

#[test]
fn hard_newline_tail_preedit_uses_current_line_when_space_allows() {
    use super::super::super::caret_map::CaretAffinity;

    let content = "abc\ndef";
    let width = 120.0;
    let cursor = 3;
    let (arena, root) = plain_preedit_fixture_with_options(
        content,
        cursor,
        "\u{4E2D}",
        Some(("\u{4E2D}".len(), "\u{4E2D}".len())),
        CaretAffinity::Downstream,
        width,
    );
    let fragments = run_text_pass_fragments(&arena, root);
    let abc_y = fragments
        .iter()
        .find_map(|(content, rect)| content.contains("abc").then_some(rect.y))
        .expect("abc fragment");
    let preedit_y = fragments
        .iter()
        .find_map(|(content, rect)| content.contains('\u{4E2D}').then_some(rect.y))
        .expect("preedit fragment");
    let def_y = fragments
        .iter()
        .find_map(|(content, rect)| content.contains("def").then_some(rect.y))
        .expect("def fragment");
    // The CJK preedit run has a taller ascent than the latin "abc"
    // run, so sharing a baseline legitimately separates the two
    // fragment tops by a few pixels. Assert line membership against
    // the next line's midpoint instead of per-font fragment tops.
    let line_midpoint = (abc_y + def_y) * 0.5;
    assert!(
        preedit_y < line_midpoint,
        "hard-newline tail preedit should stay before newline when space allows: fragments={fragments:?}"
    );
    let (_, caret_y, _) = caret_position(&arena, root);
    assert!(
        caret_y < line_midpoint,
        "preedit caret should stay with glyph before newline: caret_y={caret_y}, fragments={fragments:?}"
    );
    let rects = arena
        .with_element_taken_ref(root, |el, arena| {
            el.as_any()
                .downcast_ref::<TextArea>()
                .expect("TextArea root")
                .preedit_underline_screen_rects(arena)
        })
        .expect("root exists");
    assert!(
        rects.iter().any(|rect| (rect.y - abc_y).abs() <= 20.0),
        "preedit underline should stay with glyph before newline: rects={rects:?}, fragments={fragments:?}"
    );
}

#[test]
fn hard_newline_tail_preedit_wraps_when_space_is_insufficient() {
    use super::super::super::caret_map::CaretAffinity;

    let content = "abcdefgh\nz";
    // Tight enough that the first preedit glyph cannot fit on the
    // prefix line even with the wrap epsilon slack.
    let width = 66.0;
    let cursor = 8;
    let (arena, root) = plain_preedit_fixture_with_options(
        content,
        cursor,
        "\u{4E2D}\u{4E2D}",
        Some(("\u{4E2D}\u{4E2D}".len(), "\u{4E2D}\u{4E2D}".len())),
        CaretAffinity::Downstream,
        width,
    );
    let fragments = run_text_pass_fragments(&arena, root);
    let first_y = fragments
        .iter()
        .find_map(|(content, rect)| content.contains("abcdefgh").then_some(rect.y))
        .expect("prefix fragment");
    let preedit_y = fragments
        .iter()
        .find_map(|(content, rect)| content.contains('\u{4E2D}').then_some(rect.y))
        .expect("preedit fragment");
    assert!(
        preedit_y > first_y + 1.0,
        "hard-newline tail preedit should wrap when remaining space is insufficient: fragments={fragments:?}"
    );
    let (_, caret_y, _) = caret_position(&arena, root);
    assert!(
        caret_y > first_y + 1.0,
        "preedit caret should wrap with glyph before newline: caret_y={caret_y}, fragments={fragments:?}"
    );
    let rects = arena
        .with_element_taken_ref(root, |el, arena| {
            el.as_any()
                .downcast_ref::<TextArea>()
                .expect("TextArea root")
                .preedit_underline_screen_rects(arena)
        })
        .expect("root exists");
    assert!(
        rects.iter().any(|rect| rect.y > first_y + 1.0),
        "preedit underline should wrap with glyph before newline: rects={rects:?}, fragments={fragments:?}"
    );
}

#[test]
fn upstream_soft_wrap_preedit_can_wrap_across_lines() {
    use super::super::super::caret_map::CaretAffinity;

    let content = "the quick brown fox jumps over the lazy dog";
    let width = 80.0;
    let (base_arena, base_root) = wrapped_plain_fixture(content, width);
    let (upper_tail, _) = consumed_soft_wrap_slots(&base_arena, base_root);
    let preedit = "\u{4E2D}".repeat(12);
    let (arena, root) = plain_preedit_fixture_with_options(
        content,
        upper_tail,
        &preedit,
        Some((preedit.len(), preedit.len())),
        CaretAffinity::Upstream,
        width,
    );

    let fragments = run_text_pass_fragments(&arena, root);
    let preedit_fragment_ys = fragments
        .iter()
        .filter_map(|(content, rect)| content.contains('\u{4E2D}').then_some(rect.y))
        .collect::<Vec<_>>();
    assert!(
        preedit_fragment_ys.len() >= 2,
        "long preedit glyphs should be painted as multiple visual fragments: fragments={fragments:?}"
    );
    assert!(
        preedit_fragment_ys
            .iter()
            .fold(f32::NEG_INFINITY, |max, y| max.max(*y))
            - preedit_fragment_ys
                .iter()
                .fold(f32::INFINITY, |min, y| min.min(*y))
            > 1.0,
        "long preedit glyph fragments should span multiple visual lines: fragments={fragments:?}"
    );

    let rects = arena
        .with_element_taken_ref(root, |el, arena| {
            el.as_any()
                .downcast_ref::<TextArea>()
                .expect("TextArea root")
                .preedit_underline_screen_rects(arena)
        })
        .expect("root exists");
    assert!(
        rects.len() >= 2,
        "long preedit should keep multi-line underline fragments: {rects:?}"
    );
    let min_y = rects
        .iter()
        .map(|rect| rect.y)
        .fold(f32::INFINITY, f32::min);
    let max_y = rects
        .iter()
        .map(|rect| rect.y)
        .fold(f32::NEG_INFINITY, f32::max);
    assert!(
        max_y - min_y > 1.0,
        "long preedit underline should span multiple visual lines: {rects:?}"
    );

    let (_, caret_y, _) = caret_position(&arena, root);
    assert!(
        caret_y >= min_y - 24.0 && caret_y <= max_y + 1.0,
        "preedit caret should land on one of the composed visual lines: caret_y={caret_y}, rects={rects:?}"
    );
}

#[test]
fn projection_preedit_underline_uses_projection_text_rects() {
    let mut text_area = TextArea::new();
    text_area.content = "abXYZcd".to_string();
    text_area.font_size = 14.0;
    text_area.line_height = 1.25;
    text_area.cursor_char = 3;
    text_area.ime_preedit = "\u{4E2D}".to_string();
    text_area.on_render_handler = Some(crate::ui::on_text_area_render(move |render| {
        render.range(2..5, |_text_area_node| {
            RsxNode::tagged(
                "Element",
                RsxTagDescriptor::for_tag::<crate::view::tags::Element>(),
            )
            .with_prop(
                "style",
                ElementStylePropSchema {
                    width: Some(Length::px(90.0)),
                    height: Some(Length::px(42.0)),
                    ..Default::default()
                },
            )
            .with_child(
                RsxNode::tagged(
                    "Text",
                    RsxTagDescriptor::for_tag::<crate::view::tags::Text>(),
                )
                .with_child(RsxNode::text("XYZ")),
            )
        });
    }));

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
    crate::view::test_support::measure_and_place(
        &mut arena,
        root,
        LayoutConstraints {
            max_width: 300.0,
            max_height: 300.0,
            viewport_width: 300.0,
            viewport_height: 300.0,
            percent_base_width: None,
            percent_base_height: None,
        },
        LayoutPlacement {
            parent_x: 10.0,
            parent_y: 20.0,
            visual_offset_x: 0.0,
            visual_offset_y: 0.0,
            available_width: 300.0,
            available_height: 300.0,
            viewport_width: 300.0,
            viewport_height: 300.0,
            percent_base_width: None,
            percent_base_height: None,
        },
    );

    let projection_snap = projection_snapshot(&arena, root);
    let rects = arena
        .with_element_taken_ref(root, |el, arena| {
            el.as_any()
                .downcast_ref::<TextArea>()
                .expect("TextArea root")
                .preedit_underline_screen_rects(arena)
        })
        .expect("root exists");

    assert!(!rects.is_empty(), "expected projection IME underline");
    assert!(
        rects.iter().all(|rect| rect.height == 1.0),
        "IME underline should be 1px high: {rects:?}"
    );
    assert!(
        rects.iter().all(|rect| {
            rect.x >= projection_snap.x - 0.5
                && rect.x + rect.width <= projection_snap.x + projection_snap.width + 0.5
                && rect.y >= projection_snap.y - 0.5
                && rect.y <= projection_snap.y + projection_snap.height + 0.5
        }),
        "IME underline should be drawn inside projection bounds: rects={rects:?}, projection={projection_snap:?}"
    );
}
