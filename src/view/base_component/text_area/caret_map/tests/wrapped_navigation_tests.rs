use super::*;

#[test]
fn repeated_build_reuses_unified_package_navigation_map() {
    let (text_area_ptr, arena) = build_wrapped_textarea("cache me", 800.0);
    // SAFETY: the arena remains alive and is only borrowed immutably.
    let text_area = unsafe { &*text_area_ptr };
    let first = CaretNavigationMap::build(text_area, &arena);
    let second = CaretNavigationMap::build(text_area, &arena);
    assert!(std::rc::Rc::ptr_eq(&first, &second));
}

#[test]
fn hard_newline_down_lands_on_visual_line_below_at_similar_x() {
    let (map, len) = build_map_for("line1\nline2", 800.0);
    assert!(map.lines.len() >= 2, "expected >=2 visual lines");
    assert_eq!(len, 11);

    // Caret at end of "line1" (char 5) — Down should land on "line2"
    // near the same column (sticky_x = caret x at char 5 ≈ tail of
    // "line1").
    let stop = map
        .caret_stop_for_char(5, CaretAffinity::Downstream)
        .expect("caret stop for char 5 exists");
    let target = map
        .vertical_target(
            5,
            CaretAffinity::Downstream,
            stop.x,
            VerticalDirection::Down,
        )
        .expect("Down target exists");
    // char 5 is end of "line1", char 6 is start of "line2"
    // (the \n char itself); after one Down the caret should land at
    // a char inside the "line2" paragraph (>= 6, <= 11).
    assert!(
        (6..=11).contains(&target),
        "Down target should be inside line2 paragraph, got {target}",
    );
}

#[test]
fn hard_newline_up_then_down_round_trips_to_original_line() {
    let (map, _) = build_map_for("line1\nline2", 800.0);
    let start_char = 8; // somewhere mid "line2"
    let stop = map
        .caret_stop_for_char(start_char, CaretAffinity::Downstream)
        .expect("caret stop for start char");
    let up = map
        .vertical_target(
            start_char,
            CaretAffinity::Downstream,
            stop.x,
            VerticalDirection::Up,
        )
        .expect("Up target");
    // Up should leave the line2 paragraph (target < 6, the start of
    // paragraph 2).
    assert!(up <= 5, "Up should land in line1, got {up}");
    let down = map
        .vertical_target(
            up,
            CaretAffinity::Downstream,
            stop.x,
            VerticalDirection::Down,
        )
        .expect("Down round-trip target");
    // The round-trip should approximate the original char (within a
    // glyph or two of slop). Tighter equality requires a fixed font
    // metric; we assert it lands back on the line2 paragraph.
    assert!(
        (6..=11).contains(&down),
        "Down round-trip should re-enter line2, got {down}",
    );
}

#[test]
fn soft_wrap_within_run_yields_two_visual_lines() {
    // 60-char-ish content with a tight max_width forces the text
    // layout adapter to soft-wrap inside a single Run.
    let content = "the quick brown fox jumps over the lazy dog";
    let (map, _) = build_map_for(content, 80.0);
    assert!(
        map.lines.len() >= 2,
        "soft-wrap should create multiple visual lines, got {}",
        map.lines.len()
    );
    // Down from char 0 should land on a char in the second visual
    // line (i.e. y_top strictly greater than line 0's y_top).
    let line0_y = map.lines[0].y_top;
    let stop0 = map
        .caret_stop_for_char(0, CaretAffinity::Downstream)
        .expect("char 0 stop");
    let target = map
        .vertical_target(
            0,
            CaretAffinity::Downstream,
            stop0.x,
            VerticalDirection::Down,
        )
        .expect("Down from char 0");
    let target_stop = map
        .caret_stop_for_char(target, CaretAffinity::Downstream)
        .expect("target stop exists");
    assert!(
        target_stop.y_top > line0_y,
        "Down should land on a visually lower line; line0_y={line0_y}, target_y={}",
        target_stop.y_top,
    );
}

#[test]
fn build_translates_unified_root_caret_stops_for_wrapped_navigation() {
    let (text_area_ptr, arena) =
        build_wrapped_textarea("甲乙丙丁戊己庚辛壬癸子丑寅卯辰巳午未申酉戌亥", 80.0);
    let text_area: &TextArea = unsafe { &*text_area_ptr };
    let map = CaretNavigationMap::build(text_area, &arena);
    let origin_x = text_area.layout_state.layout_position.x - text_area.scroll_x;
    let origin_y = text_area.layout_state.layout_position.y - text_area.scroll_y;
    let expected_lines = text_area
        .unified_inline_ifc_render_package(&arena)
        .expect("root package")
        .visual_caret_lines()
        .into_iter()
        .map(|line| {
            let stops = line
                .stops
                .into_iter()
                .map(|stop| {
                    (
                        stop.char_index,
                        origin_x + stop.x,
                        origin_y + stop.y_top,
                        stop.height,
                    )
                })
                .collect::<Vec<_>>();
            (origin_y + line.y_top, origin_y + line.y_bottom, stops)
        })
        .collect::<Vec<_>>();

    assert!(
        expected_lines.len() >= 2,
        "fixture should exercise wrapped root IFC-backed caret stops"
    );
    assert_eq!(map.lines.len(), expected_lines.len());
    for (actual, (expected_y_top, expected_y_bottom, expected_stops)) in
        map.lines.iter().zip(expected_lines.iter())
    {
        assert_eq!(
            (actual.y_top, actual.y_bottom),
            (*expected_y_top, *expected_y_bottom)
        );
        assert_eq!(actual.stops.len(), expected_stops.len());
        for (actual_stop, expected_stop) in actual.stops.iter().zip(expected_stops.iter()) {
            assert_eq!(
                (
                    actual_stop.char_index,
                    actual_stop.x,
                    actual_stop.y_top,
                    actual_stop.height,
                ),
                *expected_stop,
                "CaretNavigationMap should build from TextArea unified root caret stops"
            );
        }
    }
}

#[test]
fn visual_line_home_end_split_at_soft_wrap() {
    // Soft-wrap forces "the quick brown fox jumps over the lazy dog"
    // into multiple visual lines at width 80. Cmd+Left/Right must
    // honour the *visual* edge, not the paragraph (no `\n` here).
    let content = "the quick brown fox jumps over the lazy dog";
    let (map, len) = build_map_for(content, 80.0);
    assert!(
        map.lines.len() >= 2,
        "soft-wrap expected, got {}",
        map.lines.len()
    );
    let mid_char = len / 2;
    let line_idx = map
        .line_index_for_char(mid_char, CaretAffinity::Downstream)
        .expect("mid char on a visual line");
    let expected_home = map.lines[line_idx].stops.first().unwrap().char_index;
    let expected_end = map.lines[line_idx].stops.last().unwrap().char_index;
    assert_eq!(
        map.visual_line_home_for_char(mid_char, CaretAffinity::Downstream),
        Some(expected_home)
    );
    assert_eq!(
        map.visual_line_end_for_char(mid_char, CaretAffinity::Downstream),
        Some(expected_end)
    );
    // Non-trivial: visual home must not be `0` for any line past the
    // first wrap, otherwise this collapsed to paragraph behaviour.
    if line_idx > 0 {
        assert_ne!(expected_home, 0, "visual home of wrapped line ≠ 0");
    }
}

/// At a soft-wrap, the consumed whitespace byte has no glyph on
/// either visual line — it's a single source position with two
/// caret slots. `cursor_affinity` decides which:
///   * `Upstream`   → upper line's tail (caret immediately after
///                    the last visible glyph of the upper run).
///   * `Downstream` → lower line's head (caret at the first glyph
///                    of the lower run).
#[test]
fn wrap_gap_byte_caret_splits_by_affinity() {
    let (text_area_ptr, arena) =
        build_wrapped_textarea("甲乙丙丁戊己庚辛壬癸子丑寅卯辰巳午未申酉戌亥", 80.0);
    let text_area: &crate::view::base_component::TextArea = unsafe { &*text_area_ptr };
    let map = CaretNavigationMap::build(text_area, &arena);
    assert!(map.lines.len() >= 2);
    let boundary_char = map
        .lines
        .windows(2)
        .find_map(|pair| {
            pair[0].stops.iter().find_map(|upper| {
                pair[1]
                    .stops
                    .iter()
                    .any(|lower| lower.char_index == upper.char_index)
                    .then_some(upper.char_index)
            })
        })
        .expect("adapter should synthesize a shared boundary stop at the wrap");
    let (up, down) = {
        let package = text_area
            .unified_inline_ifc_render_package(&arena)
            .expect("unified package");
        let origin_x = text_area.layout_state.layout_position.x - text_area.scroll_x;
        let origin_y = text_area.layout_state.layout_position.y - text_area.scroll_y;
        let u = package
            .caret_geometry_for_char(boundary_char, CaretAffinity::Upstream)
            .expect("upstream caret");
        let d = package
            .caret_geometry_for_char(boundary_char, CaretAffinity::Downstream)
            .expect("downstream caret");
        (
            (origin_x + u.x, origin_y + u.y_top, u.height),
            (origin_x + d.x, origin_y + d.y_top, d.height),
        )
    };
    assert!(
        up.1 < down.1,
        "Upstream y ({}) on upper line, Downstream y ({}) on lower",
        up.1,
        down.1,
    );
    assert!(
        up.1 < map.lines[1].y_top,
        "Upstream caret y ({}) should be on upper line (< {})",
        up.1,
        map.lines[1].y_top,
    );
    assert!(
        (down.1 - map.lines[1].y_top).abs() < 1.0,
        "Downstream caret y ({}) should match lower line top ({})",
        down.1,
        map.lines[1].y_top,
    );
}

/// At the lower-line head char (first glyph of the wrapped run)
/// affinity *is* meaningful: Downstream → lower head, Upstream →
/// upper tail. This is the position Cmd+Right may pin Upstream when
/// the visual line end stop coincides with the lower-run head.
#[test]
fn wrap_lower_head_caret_honours_affinity() {
    let (text_area_ptr, arena) =
        build_wrapped_textarea("甲乙丙丁戊己庚辛壬癸子丑寅卯辰巳午未申酉戌亥", 80.0);
    let text_area: &crate::view::base_component::TextArea = unsafe { &*text_area_ptr };
    let map = CaretNavigationMap::build(text_area, &arena);
    let lower_head = map.lines[1].stops.first().unwrap().char_index;
    let (up, down) = {
        let package = text_area
            .unified_inline_ifc_render_package(&arena)
            .expect("unified package");
        let origin_x = text_area.layout_state.layout_position.x - text_area.scroll_x;
        let origin_y = text_area.layout_state.layout_position.y - text_area.scroll_y;
        let u = package
            .caret_geometry_for_char(lower_head, CaretAffinity::Upstream)
            .expect("upstream caret");
        let d = package
            .caret_geometry_for_char(lower_head, CaretAffinity::Downstream)
            .expect("downstream caret");
        (
            (origin_x + u.x, origin_y + u.y_top, u.height),
            (origin_x + d.x, origin_y + d.y_top, d.height),
        )
    };
    assert!(
        up.1 < down.1,
        "Upstream y on upper line, Downstream on lower"
    );
    assert!(
        up.0 > down.0,
        "Upstream sits at upper tail x ({}) > Downstream lower head x ({})",
        up.0,
        down.0,
    );
}

#[test]
fn vertical_target_preserves_boundary_line_with_affinity() {
    let (map, _) = build_map_for("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ", 80.0);
    let (upper_idx, boundary_char, sticky_x) = map
        .lines
        .windows(2)
        .enumerate()
        .find_map(|(idx, pair)| {
            let upper_tail = pair[0].stops.last()?;
            let lower_head = pair[1].stops.first()?;
            (upper_tail.char_index == lower_head.char_index).then_some((
                idx,
                upper_tail.char_index,
                upper_tail.x,
            ))
        })
        .expect("fixture should have a shared soft-wrap boundary char");

    let lower_line = map.lines.get(upper_idx + 1).expect("lower line");
    let current = lower_line
        .stops
        .iter()
        .find(|stop| stop.char_index != boundary_char)
        .or_else(|| lower_line.stops.first())
        .expect("lower line has a current stop");
    let target = map
        .vertical_target_with_affinity(
            current.char_index,
            CaretAffinity::Downstream,
            sticky_x,
            VerticalDirection::Up,
        )
        .expect("Up target exists");

    assert_eq!(target.char_index, boundary_char);
    assert_eq!(target.affinity, CaretAffinity::Upstream);
    assert_eq!(
        map.line_index_for_char(target.char_index, target.affinity),
        Some(upper_idx),
        "target affinity should resolve back to the selected upper visual line",
    );
}

#[test]
fn vertical_target_returns_none_at_edges() {
    let (map, len) = build_map_for("solo line", 800.0);
    assert!(!map.is_empty());
    let stop0 = map
        .caret_stop_for_char(0, CaretAffinity::Downstream)
        .unwrap();
    assert!(
        map.vertical_target(0, CaretAffinity::Downstream, stop0.x, VerticalDirection::Up)
            .is_none()
    );
    let stop_end = map
        .caret_stop_for_char(len, CaretAffinity::Downstream)
        .unwrap();
    assert!(
        map.vertical_target(
            len,
            CaretAffinity::Downstream,
            stop_end.x,
            VerticalDirection::Down
        )
        .is_none()
    );
}
