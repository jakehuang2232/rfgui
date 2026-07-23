use super::*;

#[test]
fn text_layout_snapshot_preserves_single_line_nested_span_payload() {
    let input = InlineIfcInput::new(vec![InlineIfcItem::Span {
        source: ROOT,
        style: Some(style([1, 1, 1, 255], 400)),
        children: vec![
            InlineIfcItem::TextSpan {
                source: ROOT,
                text: "alpha ".to_string(),
                style: None,
            },
            InlineIfcItem::Span {
                source: INNER,
                style: Some(style([9, 9, 9, 255], 700)),
                children: vec![InlineIfcItem::TextSpan {
                    source: INNER,
                    text: "bold".to_string(),
                    style: None,
                }],
                edge_insets: [0.0; 2],
            },
        ],
        edge_insets: [0.0; 2],
    }])
    .with_max_width(400.0);
    let ifc = InlineFormattingContext::build(input);
    let snapshot = ifc.text_layout_snapshot();
    let bold_start = ifc.backing_text().find("bold").unwrap();

    assert_eq!(snapshot.lines.len(), 1);
    let line = &snapshot.lines[0];
    assert_eq!(line.line_index, 0);
    assert_eq!(line.range, 0..ifc.backing_text().len());
    assert!(line.width > 0.0);
    assert!(line.height > 0.0);
    assert!(line.baseline > 0.0);
    assert!(
        line.glyphs.iter().any(|glyph| {
            glyph.source == INNER
                && glyph.cluster_range.contains(&bold_start)
                && glyph.batch_key.brush == [9, 9, 9, 255]
                && glyph.batch_key.font_weight == 700
                && glyph.font_data_id == glyph.batch_key.font_data_id
                && glyph.font_index == glyph.batch_key.font_index
                && glyph.normalized_coords_hash == glyph.batch_key.normalized_coords_hash
                && glyph.font_data.is_some()
        }),
        "snapshot glyphs should preserve source/style/font identity: {snapshot:?}"
    );
}

#[test]
fn text_layout_snapshot_exposes_wrapped_line_ranges() {
    let ifc = fixture(72.0);
    let snapshot = ifc.text_layout_snapshot();

    assert!(
        snapshot.lines.len() >= 2,
        "narrow IFC layout should produce multiple snapshot lines: {snapshot:?}"
    );
    assert!(snapshot.lines.iter().all(|line| line.height > 0.0));
    assert!(snapshot.lines.windows(2).all(|pair| {
        pair[0].line_index + 1 == pair[1].line_index && pair[0].range.end <= pair[1].range.end
    }));
    assert!(snapshot.lines.iter().all(|line| {
        line.glyphs.iter().all(|glyph| {
            line.range.start <= glyph.cluster_range.start
                && glyph.cluster_range.end <= line.range.end
                && glyph.source != BOX_NODE
        })
    }));
}

#[test]
fn text_layout_snapshot_keeps_atomic_boxes_out_of_glyph_lines() {
    let ifc = fixture(180.0);
    let snapshot = ifc.text_layout_snapshot();

    assert_eq!(snapshot.inline_boxes.len(), 1);
    assert_eq!(snapshot.inline_boxes[0].source, BOX_NODE);
    assert!(
        snapshot
            .lines
            .iter()
            .flat_map(|line| line.glyphs.iter())
            .all(|glyph| glyph.source != BOX_NODE)
    );
    assert!(
        snapshot.lines.iter().any(|line| !line.glyphs.is_empty()),
        "text glyph payload should still coexist with inline box placements: {snapshot:?}"
    );
}

#[test]
fn text_layout_snapshot_updates_paint_for_brush_only_cache_update() {
    let mut cache = InlineIfcCache::new();
    let previous = cache_fixture_input();
    cache.put(previous);
    let previous_snapshot = cache
        .lookup_input(&cache_fixture_input())
        .cached_entry()
        .expect("same input should be cached")
        .context()
        .text_layout_snapshot();
    let previous_shape = text_layout_snapshot_shape(&previous_snapshot);
    let previous_handles = previous_snapshot
        .lines
        .iter()
        .flat_map(|line| line.glyphs.iter())
        .map(|glyph| {
            assert_font_render_handle(&glyph.font_data, glyph.font_data_id, glyph.font_index);
            (
                glyph.cluster_range.clone(),
                glyph
                    .font_data
                    .as_ref()
                    .expect("previous glyph should carry FontData")
                    .data
                    .id(),
                glyph.font_index,
                glyph.normalized_coords_hash,
            )
        })
        .collect::<Vec<_>>();

    let mut next = cache_fixture_input();
    let InlineIfcItem::Span { children, .. } = &mut next.items[0] else {
        panic!("expected root span");
    };
    let InlineIfcItem::TextSpan { style, .. } = &mut children[0] else {
        panic!("expected text span");
    };
    style.as_mut().unwrap().brush = [240, 20, 20, 255];

    let next_snapshot = {
        let update = cache.update(next);
        assert_eq!(update.invalidation, InlineIfcInvalidation::RepaintOnly);
        update.entry.context().text_layout_snapshot()
    };

    assert_eq!(
        previous_shape,
        text_layout_snapshot_shape(&next_snapshot),
        "brush-only updates should keep line/glyph positioning shape stable"
    );
    assert!(
        next_snapshot
            .lines
            .iter()
            .flat_map(|line| line.glyphs.iter())
            .all(|glyph| glyph.batch_key.brush == [240, 20, 20, 255])
    );
    let next_handles = next_snapshot
        .lines
        .iter()
        .flat_map(|line| line.glyphs.iter())
        .map(|glyph| {
            assert_font_render_handle(&glyph.font_data, glyph.font_data_id, glyph.font_index);
            (
                glyph.cluster_range.clone(),
                glyph
                    .font_data
                    .as_ref()
                    .expect("next glyph should carry FontData")
                    .data
                    .id(),
                glyph.font_index,
                glyph.normalized_coords_hash,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        previous_handles, next_handles,
        "brush-only cache updates must not mutate font render handles or variation hashes"
    );
}
