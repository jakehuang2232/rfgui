use super::*;

#[test]
fn ifc_builder_flattens_nested_styled_text_into_one_backing_string() {
    let ifc = fixture(180.0);

    assert_eq!(
        ifc.backing_text(),
        "plain outer strong tail wraps after  box"
    );
    let strong = ifc.backing_text().find("strong").unwrap();
    assert_eq!(ifc.source_for_byte(strong + 1), Some(INNER));
    assert_eq!(
        ifc.style_at_byte(strong + 1).map(|style| style.brush),
        Some([3, 3, 3, 255])
    );
    assert_eq!(
        ifc.style_at_byte(strong + 1).map(|style| style.font_weight),
        Some(700)
    );
    assert!(
        ifc.style_ranges()
            .iter()
            .any(|range| range.range.contains(&(strong + 1))
                && range.style.brush == [3, 3, 3, 255]),
        "style ranges should preserve source-byte style lookup"
    );
}

#[test]
fn ifc_builder_keeps_source_ranges_for_text_and_spans() {
    let ifc = fixture(180.0);
    let outer = ifc
        .source_ranges()
        .iter()
        .find(|range| range.source == OUTER && range.kind == InlineIfcSourceKind::Span)
        .expect("outer span should have a source range");
    let inner = ifc
        .source_ranges()
        .iter()
        .find(|range| range.source == INNER && range.kind == InlineIfcSourceKind::Span)
        .expect("inner span should have a source range");

    assert!(outer.range.start < inner.range.start);
    assert!(inner.range.end <= outer.range.end);
    assert_eq!(ifc.source_for_byte(inner.range.start), Some(INNER));
}

#[test]
fn ifc_builder_maps_inline_boxes_back_to_source_nodes() {
    let ifc = fixture(180.0);
    let inline_box = ifc.inline_boxes().first().expect("expected inline box");

    assert_eq!(inline_box.source, BOX_NODE);
    assert_eq!(ifc.source_for_inline_box(inline_box.id), Some(BOX_NODE));
    assert!((inline_box.measurement.measured_size.width - 28.0).abs() < 0.01);
    assert!((inline_box.measurement.measured_size.height - 18.0).abs() < 0.01);

    let placements = ifc.inline_box_placements();
    assert_eq!(placements.len(), 1);
    assert_eq!(placements[0].source, BOX_NODE);
    assert!((placements[0].width - 28.0).abs() < 0.01);
    assert!((placements[0].height - 18.0).abs() < 0.01);
}

#[test]
fn ifc_builder_rebuilds_line_fragments_for_span_decoration() {
    let ifc = fixture(92.0);
    let fragments = ifc.line_fragments();
    let outer_fragments = fragments
        .iter()
        .filter(|fragment| fragment.source == OUTER)
        .collect::<Vec<_>>();

    assert!(
        outer_fragments.len() >= 2,
        "narrow layout should split a span into drawable per-line fragments: {outer_fragments:?}",
    );
    assert!(
        outer_fragments
            .iter()
            .all(|fragment| fragment.x1 >= fragment.x0 && fragment.y1 > fragment.y0),
        "fragments should expose drawable rects: {outer_fragments:?}",
    );
}

#[test]
fn ifc_builder_keeps_style_lookup_independent_from_line_boundaries() {
    let ifc = fixture(180.0);
    let first_line_sources = ifc
        .line_fragments()
        .into_iter()
        .filter(|fragment| fragment.line_index == 0)
        .map(|fragment| fragment.source)
        .collect::<Vec<_>>();

    assert!(first_line_sources.contains(&ROOT));
    assert!(first_line_sources.contains(&OUTER));
    assert!(first_line_sources.contains(&INNER));
    assert_eq!(
        ifc.style_at_byte(ifc.backing_text().find("outer").unwrap())
            .map(|style| style.brush),
        Some([2, 2, 2, 255])
    );
}
