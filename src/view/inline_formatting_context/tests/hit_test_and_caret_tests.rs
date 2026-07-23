use super::*;

#[test]
fn hit_test_point_on_nested_text_returns_deepest_source_and_byte() {
    let ifc = fixture(240.0);
    let strong_start = ifc.backing_text().find("strong").unwrap();
    let caret = ifc
        .caret_geometry_for_byte(strong_start + 1, InlineIfcCaretAffinity::Downstream)
        .expect("nested strong byte should have caret geometry");

    let hit = ifc
        .hit_test_point(caret.x, caret.y)
        .expect("point near nested text should hit text");

    let InlineIfcHitTarget::Text {
        source,
        byte_index,
        line_index,
        style,
    } = hit.target
    else {
        panic!("expected text hit target: {hit:?}");
    };
    assert_eq!(source, INNER);
    assert!(byte_index >= strong_start);
    assert!(byte_index <= strong_start + "strong".len());
    assert_eq!(line_index, caret.line_index);
    assert_eq!(style.map(|style| style.font_weight), Some(700));
}

#[test]
fn hit_test_point_on_atomic_inline_box_returns_inline_box_source() {
    let ifc = fixture(240.0);
    let placement = ifc
        .inline_box_placements()
        .into_iter()
        .find(|placement| placement.source == BOX_NODE)
        .expect("atomic box should have a placement");

    let hit = ifc
        .hit_test_point(
            placement.x + placement.width / 2.0,
            placement.y + placement.height / 2.0,
        )
        .expect("point inside atomic box should hit inline box");

    assert_eq!(
        hit.target,
        InlineIfcHitTarget::InlineBox {
            source: BOX_NODE,
            id: placement.id,
            line_index: placement.line_index,
        }
    );
}

#[test]
fn caret_geometry_for_nested_byte_preserves_source_and_finite_rect() {
    let ifc = fixture(240.0);
    let strong_start = ifc.backing_text().find("strong").unwrap();
    let caret = ifc
        .caret_geometry_for_byte(strong_start + 2, InlineIfcCaretAffinity::Downstream)
        .expect("nested byte should have caret geometry");

    assert_eq!(caret.source, INNER);
    assert_eq!(caret.byte_index, strong_start + 2);
    assert_eq!(caret.affinity, InlineIfcCaretAffinity::Downstream);
    assert!(caret.x.is_finite());
    assert!(caret.y.is_finite());
    assert!(caret.height.is_finite() && caret.height > 0.0);
    assert_eq!(caret.style.map(|style| style.brush), Some([3, 3, 3, 255]));
}

#[test]
fn soft_wrap_trailing_whitespace_has_no_caret_stop() {
    let ifc = fixture(72.0);
    let stops = ifc.visual_caret_stops();
    let (soft_tail, soft_head) = stops
        .iter()
        .filter(|stop| stop.is_line_tail && stop.affinity == InlineIfcCaretAffinity::Upstream)
        .find_map(|tail| {
            stops
                .iter()
                .find(|head| {
                    head.is_line_head
                        && head.affinity == InlineIfcCaretAffinity::Downstream
                        && head.line_index == tail.line_index + 1
                        && head.byte_index > tail.byte_index
                        && ifc.backing_text()[tail.byte_index..head.byte_index]
                            .chars()
                            .all(char::is_whitespace)
                })
                .map(|head| (tail, head))
        })
        .expect("fixture should wrap after whitespace");

    assert!(
        soft_tail.byte_index < soft_head.byte_index,
        "the upper tail must sit before consumed whitespace: tail={soft_tail:?} head={soft_head:?}"
    );
    assert!(
        ifc.text_paint_glyphs().iter().all(|glyph| {
            glyph.cluster_range.end <= soft_tail.byte_index
                || glyph.cluster_range.start >= soft_head.byte_index
        }),
        "soft-wrap trailing whitespace must not produce paint glyphs"
    );
    assert!(soft_tail.style.is_some());
    assert!(soft_head.style.is_some());
}

#[test]
fn hard_line_break_keeps_distinct_affinity_stops() {
    let content = "line1\nline2";
    let ifc = InlineFormattingContext::build(
        InlineIfcInput::new(vec![InlineIfcItem::TextSpan {
            source: ROOT,
            text: content.to_string(),
            style: None,
        }])
        .with_max_width(300.0),
    );

    assert!(
        ifc.soft_wrap_trailing_whitespace_ranges().is_empty(),
        "an explicit newline must not be reported as consumed soft-wrap whitespace"
    );
    let stops = ifc.visual_caret_stops();
    let (upstream, downstream) = stops
        .iter()
        .filter(|stop| stop.is_line_tail && stop.affinity == InlineIfcCaretAffinity::Upstream)
        .find_map(|tail| {
            stops
                .iter()
                .find(|head| {
                    head.is_line_head
                        && head.affinity == InlineIfcCaretAffinity::Downstream
                        && head.line_index == tail.line_index + 1
                        && &ifc.backing_text()[tail.byte_index..head.byte_index] == "\n"
                })
                .map(|head| (tail, head))
        })
        .expect("hard break must separate upper-tail and lower-head caret stops");
    assert!(upstream.line_index < downstream.line_index);
}

#[test]
fn visual_caret_stops_include_line_heads_and_tails_for_navigation_maps() {
    let ifc = fixture(72.0);
    let stops = ifc.visual_caret_stops();
    let line_count = ifc.layout.lines().count();

    for line_index in 0..line_count {
        assert!(
            stops
                .iter()
                .any(|stop| stop.line_index == line_index && stop.is_line_head),
            "line {line_index} should have a visual head caret stop: {stops:?}"
        );
        assert!(
            stops
                .iter()
                .any(|stop| stop.line_index == line_index && stop.is_line_tail),
            "line {line_index} should have a visual tail caret stop: {stops:?}"
        );
    }
    assert!(
        stops.iter().all(|stop| {
            stop.x.is_finite()
                && stop.y.is_finite()
                && stop.height > 0.0
                && stop.style.is_some()
        }),
        "visual caret stops should carry finite geometry and resolved style: {stops:?}"
    );
}
