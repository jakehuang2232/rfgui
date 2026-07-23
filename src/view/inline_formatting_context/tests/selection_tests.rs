use super::*;

#[test]
fn selection_across_line_wrap_returns_text_rects_with_source_and_style() {
    let ifc = fixture(72.0);
    let outer_range = ifc
        .source_ranges()
        .iter()
        .find(|range| range.source == OUTER && range.kind == InlineIfcSourceKind::Span)
        .expect("outer span should have a range")
        .range
        .clone();

    let rects = ifc.selection_rects_for_global_range(outer_range);

    assert!(
        rects.len() >= 2,
        "wrapped selection should emit multiple per-line rects: {rects:?}"
    );
    assert!(
        rects
            .windows(2)
            .any(|pair| pair[0].line_index != pair[1].line_index),
        "selection rects should preserve visual line identity: {rects:?}"
    );
    assert!(
        rects.iter().all(|rect| {
            (rect.source == OUTER || rect.source == INNER)
                && rect.rect.width > 0.0
                && rect.rect.height > 0.0
                && rect.style.is_some()
        }),
        "selection rects should keep deepest text source and style: {rects:?}"
    );
}

#[test]
fn source_filtered_selection_keeps_only_matching_text_source() {
    let ifc = fixture(72.0);
    let outer_range = ifc
        .source_ranges()
        .iter()
        .find(|range| range.source == OUTER && range.kind == InlineIfcSourceKind::Span)
        .expect("outer span should have a range")
        .range
        .clone();

    let rects = ifc.selection_rects_for_source_range(INNER, outer_range);

    assert!(!rects.is_empty(), "inner selection rects should exist");
    assert!(
        rects.iter().all(|rect| rect.source == INNER),
        "source-filtered selection should only return the requested source: {rects:?}"
    );
    assert!(
        rects
            .iter()
            .all(|rect| rect.style.as_ref().map(|style| style.font_weight) == Some(700)),
        "source-filtered selection should keep resolved style: {rects:?}"
    );
}

#[test]
fn utf8_and_combining_mark_selection_clamps_to_char_boundaries() {
    let text = "aé e\u{301} 中z";
    let input = InlineIfcInput::new(vec![InlineIfcItem::TextSpan {
        source: ROOT,
        text: text.to_string(),
        style: Some(style([12, 34, 56, 255], 400)),
    }])
    .with_max_width(240.0);
    let ifc = InlineFormattingContext::build(input);
    let accent_start = ifc.backing_text().find('é').unwrap();
    let cjk_start = ifc.backing_text().find('中').unwrap();

    let rects = ifc.selection_rects_for_global_range((accent_start + 1)..(cjk_start + 2));

    assert!(
        !rects.is_empty(),
        "UTF-8 selection should not panic or disappear"
    );
    assert!(
        rects.iter().all(|rect| {
            ifc.backing_text().is_char_boundary(rect.range.start)
                && ifc.backing_text().is_char_boundary(rect.range.end)
                && rect.rect.width > 0.0
                && rect.rect.height > 0.0
        }),
        "selection rect ranges should be clamped to UTF-8 char boundaries: {rects:?}"
    );
    assert_eq!(
        rects.first().map(|rect| rect.range.start),
        Some(accent_start)
    );
    assert_eq!(rects.last().map(|rect| rect.range.end), Some(cjk_start));
}

#[test]
fn nested_span_boundary_selection_splits_by_source_and_style() {
    let ifc = fixture(240.0);
    let outer_start = ifc.backing_text().find("outer").unwrap();
    let strong_end = ifc.backing_text().find("strong").unwrap() + "strong".len();

    let rects = ifc.selection_rects_for_global_range(outer_start..strong_end);

    assert!(
        rects.iter().any(|rect| rect.source == OUTER
            && rect.style.as_ref().map(|style| style.brush) == Some([2, 2, 2, 255])),
        "selection should keep the outer text source/style: {rects:?}"
    );
    assert!(
        rects.iter().any(|rect| rect.source == INNER
            && rect.style.as_ref().map(|style| style.font_weight) == Some(700)),
        "selection should split at nested span source/style boundary: {rects:?}"
    );
}

#[test]
fn selection_near_atomic_inline_box_does_not_select_atomic_source() {
    let ifc = fixture(240.0);
    let selection_start = ifc.backing_text().find("after").unwrap();
    let selection_end = ifc.backing_text().find("box").unwrap() + "box".len();

    let rects = ifc.selection_rects_for_global_range(selection_start..selection_end);

    assert!(!rects.is_empty(), "text around atomic box should select");
    assert!(
        rects.iter().all(|rect| rect.source != BOX_NODE),
        "text selection primitives should not include atomic boxes implicitly: {rects:?}"
    );
    assert!(
        ifc.inline_box_placements()
            .iter()
            .any(|placement| placement.source == BOX_NODE),
        "atomic box remains available for explicit hit-test/selection handling"
    );
}
