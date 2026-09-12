use super::*;

/// Projection containing `<Text>` — caret-stop_for_char must succeed
/// for char indices that fall inside the projection. Without the
/// projection branch the map had no entry for these chars and
/// vertical-arrow handling silently bailed.
#[test]
fn projection_with_text_emits_stops_for_inner_chars() {
    // chars: 0..3 "abc", 3..6 "XYZ" (projection w/ Text), 6..9 "def".
    let fx = build_projection_fixture(
        "abcXYZdef",
        3..6,
        Some("XYZ"),
        ElementStylePropSchema::default(),
        800.0,
    );
    let map = fx.map();
    for cur in 3..=6 {
        assert!(
            map.caret_stop_for_char(cur, CaretAffinity::Downstream)
                .is_some(),
            "missing caret stop for projection char {cur}",
        );
    }
}

/// Caret inside a projection (with `<Text>` descendant) on its own
/// visual line: Down should leave to the next paragraph's line, Up
/// should leave to the previous paragraph's line. Pre-fix this used
/// to be a no-op because projection chars had no map entry at all.
#[test]
fn projection_with_text_caret_inside_can_move_up_and_down() {
    // 3 paragraphs, projection on its own line (line 1).
    // chars: 0..3 "abc", 3..4 "\n", 4..7 "XYZ" (projection),
    //        7..8 "\n", 8..11 "def".
    let fx = build_projection_fixture(
        "abc\nXYZ\ndef",
        4..7,
        Some("XYZ"),
        ElementStylePropSchema::default(),
        800.0,
    );
    let map = fx.map();
    let inside = 5; // mid-projection char
    let stop = map
        .caret_stop_for_char(inside, CaretAffinity::Downstream)
        .expect("projection char has a stop");
    let down = map
        .vertical_target(
            inside,
            CaretAffinity::Downstream,
            stop.x,
            VerticalDirection::Down,
        )
        .expect("Down should land somewhere");
    // Down should leave the projection line (chars 4..=7) to "def"
    // (chars 8..=11).
    assert!(
        (8..=11).contains(&down),
        "Down from projection char {inside} should land on the def line, got {down}",
    );
    let up = map
        .vertical_target(
            inside,
            CaretAffinity::Downstream,
            stop.x,
            VerticalDirection::Up,
        )
        .expect("Up should land somewhere");
    // Up should leave the projection line to "abc" (chars 0..=3).
    assert!(
        up <= 3,
        "Up from projection char {inside} should land on the abc line, got {up}",
    );
}

/// sticky-x at a column inside the projection's horizontal extent
/// should land *on* a projection char when Up/Down crosses the
/// projection's row. Pre-fix the projection's row had no stops so
/// vertical_target snapped to a non-projection char on the same y.
#[test]
fn vertical_target_lands_inside_projection_when_sticky_x_overlaps() {
    // Same fixture: projection on its own line.
    let fx = build_projection_fixture(
        "abc\nXYZ\ndef",
        4..7,
        Some("XYZ"),
        ElementStylePropSchema::default(),
        800.0,
    );
    let map = fx.map();
    // Pick the projection's middle char and use its x as sticky_x;
    // a Down from line 0 ("abc") at that x should land on a
    // projection char (4..=7).
    let mid_stop = map
        .caret_stop_for_char(5, CaretAffinity::Downstream)
        .expect("projection mid stop");
    let line0_target = map
        .vertical_target(
            0,
            CaretAffinity::Downstream,
            mid_stop.x,
            VerticalDirection::Down,
        )
        .expect("Down from char 0");
    assert!(
        (4..=7).contains(&line0_target),
        "Down from line0 at projection-x should land in projection chars, got {line0_target}",
    );
}

/// Icon-only projection (no `<Text>` descendant) sharing a line
/// with surrounding Run text. The projection's char range still
/// needs map entries so caret-inside Up/Down isn't a no-op. Stops
/// land at the projection's box (per `inline_fragment_rects` /
/// box-snapshot fallback).
#[test]
fn icon_only_projection_caret_inside_can_move_up_and_down() {
    // chars: 0..3 "abc", 3..4 "\n", 4..7 "XYZ" (icon projection),
    //        7..8 "\n", 8..11 "def".
    let fx = build_projection_fixture("abc\nXYZ\ndef", 4..7, None, fixed_box_style(), 800.0);
    let map = fx.map();
    let inside = 5;
    let stop = map
        .caret_stop_for_char(inside, CaretAffinity::Downstream)
        .expect("icon projection should still emit stops");
    let down = map
        .vertical_target(
            inside,
            CaretAffinity::Downstream,
            stop.x,
            VerticalDirection::Down,
        )
        .expect("Down from inside icon projection");
    assert!(
        (8..=11).contains(&down),
        "Down from icon projection char {inside} should land on the def line, got {down}",
    );
    let up = map
        .vertical_target(
            inside,
            CaretAffinity::Downstream,
            stop.x,
            VerticalDirection::Up,
        )
        .expect("Up from inside icon projection");
    assert!(
        up <= 3,
        "Up from icon projection char {inside} should land on the abc line, got {up}",
    );
}

/// Integration check for the boundary-cursor affinity behavior.
/// With content soft-wrapped and `cursor_char` parked at the
/// wrap-consumed whitespace char (= the boundary cursor), caret y
/// follows `cursor_affinity`:
///   * `Upstream`   → upper visual line.
///   * `Downstream` → lower visual line.
#[test]
fn caret_at_boundary_cursor_splits_by_affinity() {
    let content = "甲乙丙丁戊己庚辛壬癸子丑寅卯辰巳午未申酉戌亥";
    let max_width = 80.0;
    let upper_tail = {
        let (map, _) = build_map_for(content, max_width);
        assert!(map.lines.len() >= 2, "soft-wrap expected");
        map.lines[0].stops.last().unwrap().char_index
    };

    let mut up_y: Option<f32> = None;
    let mut down_y: Option<f32> = None;
    let mut upper_y_ref = 0.0;
    let mut lower_y_ref = 0.0;
    for affinity in [CaretAffinity::Downstream, CaretAffinity::Upstream] {
        let mut text_area = TextArea::new();
        text_area.content = content.to_string();
        text_area.font_size = 14.0;
        text_area.line_height = 1.25;
        text_area.is_focused = true;
        text_area.cursor_char = upper_tail;
        text_area.cursor_affinity = affinity;
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
                max_width,
                max_height: 600.0,
                viewport_width: max_width,
                viewport_height: 600.0,
                percent_base_width: Some(max_width),
                percent_base_height: Some(600.0),
            },
            LayoutPlacement {
                parent_x: 0.0,
                parent_y: 0.0,
                visual_offset_x: 0.0,
                visual_offset_y: 0.0,
                available_width: max_width,
                available_height: 600.0,
                viewport_width: max_width,
                viewport_height: 600.0,
                percent_base_width: Some(max_width),
                percent_base_height: Some(600.0),
            },
        );
        let (caret, upper_y, lower_y) = arena
            .with_element_taken_ref(root, |el, arena| {
                let ta = el.as_any().downcast_ref::<TextArea>().unwrap();
                let map = CaretNavigationMap::build(ta, arena);
                let upper = map.lines[0].y_top;
                let lower = map.lines[1].y_top;
                let caret = ta.caret_screen_position(arena);
                (caret, upper, lower)
            })
            .unwrap();
        let (_, y, _) = caret.expect("caret resolves");
        upper_y_ref = upper_y;
        lower_y_ref = lower_y;
        match affinity {
            CaretAffinity::Upstream => up_y = Some(y),
            CaretAffinity::Downstream => down_y = Some(y),
        }
    }
    let bup = up_y.unwrap();
    let bdown = down_y.unwrap();
    assert!(
        bup < bdown,
        "Upstream y ({bup}) on upper line, Downstream y ({bdown}) on lower",
    );
    assert!(
        (bup - upper_y_ref).abs() < (lower_y_ref - upper_y_ref) * 0.5,
        "Upstream y ({bup}) should match upper line ({upper_y_ref})",
    );
    assert!(
        (bdown - lower_y_ref).abs() < (lower_y_ref - upper_y_ref) * 0.5,
        "Downstream y ({bdown}) should match lower line ({lower_y_ref})",
    );
}

/// Boundary char between a Run and the following projection: only
/// one stop survives per visual line (the projection's owning stop)
/// so vertical_target's nearest-x search isn't fooled by a duplicate
/// at the same x.
#[test]
fn boundary_char_between_run_and_projection_is_deduped_per_line() {
    // chars: 0..3 "abc", 3..6 "XYZ" (projection w/ Text), 6..9 "def".
    // Boundary chars: 3 (Run "abc" tail == projection leading) and
    // 6 (projection tail == Run "def" leading).
    let fx = build_projection_fixture(
        "abcXYZdef",
        3..6,
        Some("XYZ"),
        ElementStylePropSchema::default(),
        800.0,
    );
    let map = fx.map();
    // All siblings lay on a single visual line at this width.
    assert_eq!(
        map.lines.len(),
        1,
        "expected single visual line, got {}",
        map.lines.len()
    );
    let line = &map.lines[0];
    for boundary in [3usize, 6usize] {
        let count = line
            .stops
            .iter()
            .filter(|s| s.char_index == boundary)
            .count();
        assert_eq!(
            count, 1,
            "boundary char {boundary} should have a single deduped stop, got {count}",
        );
    }
}
