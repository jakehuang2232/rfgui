use super::*;

#[test]
fn empty_paragraph_between_text_yields_navigable_stop() {
    let (map, _) = build_map_for("a\n\nb", 800.0);
    // Three visual lines: "a", "", "b".
    assert!(
        map.lines.len() >= 3,
        "expected >=3 visual lines for a\\n\\nb, got {}: {:#?}",
        map.lines.len(),
        map.lines
    );
    // Down from char 0 ('a') should land on the empty middle line —
    // char 2 (after the first \n).
    let stop0 = map
        .caret_stop_for_char(0, CaretAffinity::Downstream)
        .unwrap();
    let target = map
        .vertical_target(
            0,
            CaretAffinity::Downstream,
            stop0.x,
            VerticalDirection::Down,
        )
        .expect("Down from char 0");
    // char 2 = start of the empty paragraph (after the first `\n`).
    // It must be a distinct caret target rather than skipping straight
    // to char 3 (the following paragraph).
    assert_eq!(target, 2, "Down from line 1 must land on the empty line");
}

#[test]
fn newline_only_content_exposes_a_pointer_caret_stop_on_each_empty_line() {
    let (map, _) = build_map_for("\n", 300.0);
    assert!(
        map.lines.len() >= 2,
        "a lone newline must create two empty visual lines: {:#?}",
        map.lines
    );
    for (line_index, line) in map.lines.iter().take(2).enumerate() {
        let target = map
            .pointer_target(0.0, (line.y_top + line.y_bottom) * 0.5)
            .expect("empty visual line should accept a caret pointer target");
        assert_eq!(
            map.line_index_for_char(target.char_index, target.affinity),
            Some(line_index),
            "pointer target must remain on empty line {line_index}: {target:?}"
        );
    }
}

/// Repro: caret should resolve to a screen position on every kind of
/// empty visual line (middle empty paragraph, trailing newline, fully
/// empty content). Failure here = caret invisible in editor.
#[test]
fn caret_screen_position_resolves_on_every_empty_line_kind() {
    fn check(content: &str, cursor: usize, label: &str) {
        let mut text_area = TextArea::new();
        text_area.content = content.to_string();
        text_area.font_size = 14.0;
        text_area.line_height = 1.25;
        text_area.is_focused = true;
        text_area.cursor_char = cursor;
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
                parent_x: 0.0,
                parent_y: 0.0,
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
        let pos = arena
            .with_element_taken_ref(root, |el, arena| {
                el.as_any()
                    .downcast_ref::<TextArea>()
                    .unwrap()
                    .caret_screen_position(arena)
            })
            .flatten();
        assert!(pos.is_some(), "{label}: caret should resolve");
    }
    check("", 0, "fully empty");
    check("\n", 0, "newline-only first empty line");
    check("\n", 1, "newline-only trailing empty line");
    check("a\n", 2, "trailing-newline empty line");
    check("a\n\nb", 2, "middle empty paragraph");
}

/// `pointer_target` is the three-step shape used by hit-test:
/// (1) line-by-y (inside-band wins, else nearest), (2) nearest stop
/// by x within that line, (3) return its char_index. Verify all three
/// steps independently.
#[test]
fn pointer_target_picks_line_by_y_then_nearest_stop_by_x() {
    // Two paragraphs side by side stacked vertically gives us two
    // visual lines with predictable y bands.
    let (map, _) = build_map_for("line one\nline two", 800.0);
    assert!(map.lines.len() >= 2, "expected >= 2 visual lines");

    // Step 1: y above the first line clamps to line 0.
    let line0 = &map.lines[0];
    let line0_mid_y = (line0.y_top + line0.y_bottom) * 0.5;
    let target = map
        .pointer_target(0.0, line0.y_top - 1000.0)
        .expect("clamp above-first should find a target");
    let stop = map
        .caret_stop_for_char(target.char_index, target.affinity)
        .expect("stop");
    assert!(
        (stop.y_top - line0.y_top).abs() < 0.5,
        "above-first click should clamp to line 0 (y_top={})",
        line0.y_top,
    );

    // Step 2 & 3: y inside line 0, x near line 0's last stop should
    // pick that last stop's char (= 8, end of "line one").
    let last_stop_x = line0.stops.last().expect("non-empty").x;
    let target = map
        .pointer_target(last_stop_x + 1000.0, line0_mid_y)
        .expect("inside line 0 picks a target");
    assert_eq!(
        target.char_index,
        line0.stops.last().expect("non-empty").char_index,
        "x past line 0 should snap to its rightmost stop",
    );

    // y below the last line clamps to last line.
    let last_line = map.lines.last().expect("non-empty");
    let target_below = map
        .pointer_target(0.0, last_line.y_bottom + 1000.0)
        .expect("clamp below-last");
    let stop_below = map
        .caret_stop_for_char(target_below.char_index, target_below.affinity)
        .expect("stop");
    assert!(
        (stop_below.y_top - last_line.y_top).abs() < 0.5,
        "below-last click should clamp to last line",
    );
}

#[test]
fn pointer_target_preserves_upper_affinity_at_soft_wrap_tail() {
    let content = "甲乙丙丁戊己庚辛壬癸子丑寅卯辰巳午未申酉戌亥";
    let (map, _) = build_map_for(content, 80.0);
    assert!(map.lines.len() >= 2, "soft-wrap expected");
    let line0 = &map.lines[0];
    let line0_mid_y = (line0.y_top + line0.y_bottom) * 0.5;
    let upper_tail = line0.stops.last().expect("upper line has tail stop");

    let target = map
        .pointer_target(upper_tail.x + 1000.0, line0_mid_y)
        .expect("line-tail click should resolve");

    assert_eq!(target.char_index, upper_tail.char_index);
    assert_eq!(
        target.affinity,
        CaretAffinity::Upstream,
        "clicking the upper visual line tail must keep the caret on that line",
    );
}
