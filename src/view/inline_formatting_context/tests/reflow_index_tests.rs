use super::*;

#[test]
fn caret_stop_index_preserves_soft_wrap_identity_and_first_geometry() {
    let normal = InlineIfcCaretStop {
        source: ROOT,
        byte_index: 5,
        affinity: InlineIfcCaretAffinity::Downstream,
        line_index: 1,
        x: 10.0,
        y: 20.0,
        height: 14.0,
        style: None,
        is_line_head: true,
        is_line_tail: false,
        is_soft_wrap_boundary: false,
    };
    let wrapped = InlineIfcCaretStop {
        is_soft_wrap_boundary: true,
        ..normal.clone()
    };
    let next_line = InlineIfcCaretStop {
        line_index: 2,
        ..normal.clone()
    };
    let tail = InlineIfcCaretStop {
        is_line_head: false,
        is_line_tail: true,
        ..normal.clone()
    };
    let mut stops = InlineIfcCaretStopBuffer::default();
    stops.push(normal.clone(), true);
    // A repeated cluster/source must not replace the first position.
    stops.push(
        InlineIfcCaretStop {
            source: INNER,
            x: 999.0,
            ..normal.clone()
        },
        true,
    );
    stops.push(wrapped.clone(), true);
    // Empty-line and box edges ignore the soft-wrap flag, but not line/edge.
    stops.push(wrapped.clone(), false);
    stops.push(next_line.clone(), false);
    stops.push(tail.clone(), false);
    assert_eq!(stops.values, vec![normal, wrapped, next_line, tail]);
}

// Compare indexed lookups against independent full scans, including shared
// line edges, UTF-8 clusters, bidi runs, empty lines and changing wrap widths.
#[test]
fn reflow_indexes_preserve_line_glyph_and_caret_geometry() {
    for align in [
        crate::style::VerticalAlign::Baseline,
        crate::style::VerticalAlign::Top,
        crate::style::VerticalAlign::Middle,
        crate::style::VerticalAlign::Bottom,
    ] {
        for width in [160.0, 280.0, 164.0, 160.0] {
            let text =
                "License 中文 e\u{301} office שלום مرحبا   \n\nTail line wraps here.\n".repeat(16);
            let ifc = InlineFormattingContext::build(
                InlineIfcInput::new(vec![InlineIfcItem::TextSpan {
                    source: ROOT,
                    text,
                    style: Some(InlineIfcStyle {
                        vertical_align: align,
                        ..InlineIfcStyle::default()
                    }),
                }])
                .with_max_width(width),
            );
            let glyphs = ifc.glyph_items_ref();
            let snapshot = ifc.text_layout_snapshot_ref();
            let stops = ifc.visual_caret_stops_ref();
            let stop_positions: std::collections::HashSet<_> = stops
                .iter()
                .map(|stop| (stop.line_index, stop.byte_index, stop.affinity))
                .collect();
            for (index, line) in ifc.layout.lines().enumerate() {
                let metrics = line.metrics();
                for y in [
                    metrics.block_min_coord,
                    metrics.block_min_coord + metrics.line_height * 0.5,
                    metrics.block_min_coord + metrics.line_height,
                ] {
                    let expected = ifc.layout.lines().enumerate().find_map(|(i, line)| {
                        let m = line.metrics();
                        (m.block_min_coord <= y && y <= m.block_min_coord + m.line_height)
                            .then_some(i)
                    });
                    assert_eq!(ifc.line_index_for_cursor_y(y), expected);
                }
                let expected: Vec<_> = glyphs.iter().filter(|g| g.line_index == index).collect();
                assert_eq!(snapshot.lines[index].glyphs.len(), expected.len());
                for (actual, glyph) in snapshot.lines[index].glyphs.iter().zip(expected) {
                    assert_eq!(actual.cluster_range, glyph.cluster_range);
                    assert_eq!(
                        (actual.glyph_id, actual.x, actual.y),
                        (glyph.glyph_id, glyph.x, glyph.y)
                    );
                    for (byte, affinity) in [
                        (
                            glyph.cluster_range.start,
                            InlineIfcCaretAffinity::Downstream,
                        ),
                        (glyph.cluster_range.end, InlineIfcCaretAffinity::Upstream),
                    ] {
                        let caret = ifc.caret_geometry_for_byte(byte, affinity).unwrap();
                        if caret.line_index == index {
                            assert!(stop_positions.contains(&(index, byte, affinity)));
                        }
                    }
                }
            }
            let rects = ifc.source_text_line_rects(ROOT);
            for stop in stops {
                let caret = ifc
                    .caret_geometry_for_byte(stop.byte_index, stop.affinity)
                    .unwrap();
                if let Some((_, rect)) = rects.iter().find(|(i, _)| *i == caret.line_index) {
                    assert_eq!((caret.y, caret.height), (rect.y, rect.height));
                }
            }
        }
    }
}
